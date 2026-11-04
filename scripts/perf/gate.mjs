import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

import { compare } from './report.mjs'

const DEFAULT_POLICY = resolve('scripts/perf/redline.json')

export function loadReport(input) {
  const path = resolve(input)
  if (statSync(path).isFile()) return JSON.parse(readFileSync(path, 'utf8'))
  const summary = join(path, 'summary.json')
  if (existsSync(summary)) return loadReport(summary)
  const reports = readdirSync(path)
    .sort()
    .map((name) => loadReport(join(path, name)))
  const first = reports[0]
  if (
    !first ||
    reports.some(
      (report) =>
        report.build?.binarySha256 !== first.build?.binarySha256 ||
        ['invalid', 'incomparable'].includes(compare(first, report).verdict),
    )
  )
    throw new Error('group contains mixed builds, contracts, or invalid runs')
  return { ...first, runs: reports.flatMap((report) => report.runs) }
}

export function evaluateRedlineComparison(comparison, policy) {
  if (!['inconclusive'].includes(comparison?.verdict))
    return {
      verdict: 'invalid',
      reason: comparison?.reason ?? 'invalid comparison',
    }

  const metrics = Object.fromEntries(
    policy.metrics.map(({ key }) => {
      const metric = comparison.metrics?.[key]
      if (!metric || !Number.isFinite(metric.before) || metric.before <= 0)
        return [
          key,
          { verdict: 'invalid', reason: 'missing positive baseline' },
        ]
      if (!Number.isFinite(metric.after) || metric.after < 0)
        return [key, { verdict: 'invalid', reason: 'missing candidate value' }]
      const regressionPercent =
        ((metric.after - metric.before) / metric.before) * 100
      const limit = metric.before * (1 + policy.maxRegressionPercent / 100)
      return [
        key,
        {
          before: metric.before,
          after: metric.after,
          limit,
          regressionPercent,
          verdict: metric.after <= limit ? 'passed' : 'failed',
        },
      ]
    }),
  )
  const invalid = Object.entries(metrics).filter(
    ([, metric]) => metric.verdict === 'invalid',
  )
  if (invalid.length)
    return {
      verdict: 'invalid',
      reason: 'redline metric is unavailable',
      metrics,
    }
  const violations = Object.entries(metrics)
    .filter(([, metric]) => metric.verdict === 'failed')
    .map(([key, metric]) => ({ key, ...metric }))
  return {
    verdict: violations.length ? 'failed' : 'passed',
    maxRegressionPercent: policy.maxRegressionPercent,
    metrics,
    violations,
  }
}

export function evaluateRedline(baseline, candidate, policy) {
  if (
    baseline.runs.length < policy.minRunsPerGroup ||
    candidate.runs.length < policy.minRunsPerGroup
  )
    return {
      verdict: 'invalid',
      reason: `both reports require at least ${policy.minRunsPerGroup} independent runs`,
    }
  if (baseline.build?.binarySha256 === candidate.build?.binarySha256)
    return {
      verdict: 'invalid',
      reason: 'baseline and candidate must use different binaries',
    }
  return evaluateRedlineComparison(compare(baseline, candidate, 0), policy)
}

function readPolicy(input = DEFAULT_POLICY) {
  return JSON.parse(readFileSync(resolve(input), 'utf8'))
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  const args = process.argv.slice(2).filter((value) => value !== '--')
  if (args.length < 2)
    throw new Error(
      'Usage: pnpm perf:gate -- baseline/summary.json candidate/summary.json [policy.json]',
    )
  const policy = readPolicy(args[2])
  let result
  try {
    result = evaluateRedline(loadReport(args[0]), loadReport(args[1]), policy)
  } catch (error) {
    result = { verdict: 'invalid', reason: String(error) }
  }
  console.log(JSON.stringify(result, null, 2))
  if (result.verdict === 'invalid') process.exitCode = 2
  if (result.verdict === 'failed') process.exitCode = 1
}
