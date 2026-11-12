import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const frontend = fs.readFileSync(
  path.join(root, 'src/services/delay.ts'),
  'utf8',
)
const backendConfig = fs.readFileSync(
  path.join(root, 'src-tauri/src/config/verge.rs'),
  'utf8',
)
const backendProbe = fs.readFileSync(
  path.join(root, 'src-tauri/src/cmd/clash5_data.rs'),
  'utf8',
)
const presetMatch = backendConfig.match(
  /pub const fn probe_concurrency[\s\S]*?(?=\n\s*pub const fn probe_interval_ms)/,
)
assert.ok(presetMatch, 'Could not find backend preset concurrency mapping')
const presets = presetMatch[0]

const capture = (source, pattern, label) => {
  const match = source.match(pattern)
  assert.ok(match, `Could not find ${label}`)
  return Number(match[1])
}

const frontendDefault = capture(
  frontend,
  /DEFAULT_DELAY_CONCURRENCY\s*=\s*(\d+)/,
  'frontend default concurrency',
)
const frontendMaximum = capture(
  frontend,
  /MAX_DELAY_CONCURRENCY\s*=\s*(\d+)/,
  'frontend concurrency cap',
)
const backendDefault = capture(
  backendConfig,
  /DEFAULT_PROBE_CONCURRENCY:\s*usize\s*=\s*(\d+)/,
  'backend default concurrency',
)
const backendMaximum = capture(
  backendConfig,
  /MAX_PROBE_CONCURRENCY:\s*usize\s*=\s*(\d+)/,
  'backend concurrency cap',
)

assert.equal(frontendDefault, 10, 'frontend default must remain 10')
assert.equal(
  frontendDefault,
  backendDefault,
  'frontend and backend defaults must match',
)
assert.equal(frontendMaximum, 16, 'frontend cap must remain 16')
assert.equal(
  frontendMaximum,
  backendMaximum,
  'frontend and backend caps must match',
)
assert.match(
  presets,
  /Self::Balanced\s*=>\s*DEFAULT_PROBE_CONCURRENCY/,
  'balanced preset must use the backend default',
)
assert.match(
  backendConfig,
  /Self::Performance\s*=>\s*MAX_PROBE_CONCURRENCY/,
  'performance preset must use the backend cap',
)
assert.match(
  backendProbe,
  /PROBE_CONCURRENCY:\s*usize\s*=\s*MAX_PROBE_CONCURRENCY/,
  'backend execution cap must use the declared maximum',
)
assert.match(
  frontend,
  /effectiveDelayConcurrency\(\s*concurrency,\s*proxies\.length,?\s*\)/,
  'batch probing must use the tested effective concurrency helper',
)
assert.match(
  backendProbe,
  /concurrency\.min\(PROBE_CONCURRENCY\)/,
  'backend effective concurrency must use the declared cap',
)

console.log(
  'Frontend and backend probe defaults and effective caps match (10 / 16).',
)
