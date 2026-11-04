import assert from 'node:assert/strict'
import test from 'node:test'

import { evaluateRedlineComparison } from './gate.mjs'

const policy = {
  maxRegressionPercent: 20,
  metrics: [
    { key: 'foreground/webview/cpuMean' },
    { key: 'foreground/webview/rssP50' },
  ],
}

test('performance redline passes at or below the fixed 20 percent ceiling', () => {
  const result = evaluateRedlineComparison(
    {
      verdict: 'inconclusive',
      metrics: {
        'foreground/webview/cpuMean': { before: 10, after: 12 },
        'foreground/webview/rssP50': { before: 100, after: 120 },
      },
    },
    policy,
  )
  assert.equal(result.verdict, 'passed')
  assert.equal(result.violations.length, 0)
})

test('performance redline fails when any metric exceeds 20 percent', () => {
  const result = evaluateRedlineComparison(
    {
      verdict: 'inconclusive',
      metrics: {
        'foreground/webview/cpuMean': { before: 10, after: 12.01 },
        'foreground/webview/rssP50': { before: 100, after: 100 },
      },
    },
    policy,
  )
  assert.equal(result.verdict, 'failed')
  assert.deepEqual(
    result.violations.map(({ key }) => key),
    ['foreground/webview/cpuMean'],
  )
})

test('performance redline fails closed when a metric is missing', () => {
  const result = evaluateRedlineComparison(
    { verdict: 'inconclusive', metrics: {} },
    policy,
  )
  assert.equal(result.verdict, 'invalid')
})
