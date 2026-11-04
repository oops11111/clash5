import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const policy = JSON.parse(
  await readFile(path.join(root, 'scripts/perf/redline.json'), 'utf8'),
)
assert.equal(policy.schema, 'traffic-webview-redline-v1')
assert.equal(policy.maxRegressionPercent, 20)
assert.ok(
  Number.isInteger(policy.minRunsPerGroup) && policy.minRunsPerGroup >= 3,
)
assert.ok(Array.isArray(policy.metrics) && policy.metrics.length === 12)
const phases = new Set(['foreground', 'hidden', 'restored'])
const roles = new Set(['webview', 'main'])
const names = new Set(['cpuMean', 'rssP50'])
const keys = new Set()
for (const metric of policy.metrics) {
  assert.ok(typeof metric.key === 'string' && !keys.has(metric.key))
  keys.add(metric.key)
  const [phase, role, name] = metric.key.split('/')
  assert.ok(phases.has(phase) && roles.has(role) && names.has(name))
  assert.equal(metric.direction, 'lower-is-better')
  assert.ok(metric.unit === 'percent' || metric.unit === 'bytes')
}
assert.equal(keys.size, 12)
console.log(
  'Performance redline policy is fixed at 20% across all traffic phases and process roles.',
)
