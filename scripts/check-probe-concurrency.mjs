// Keep the frontend and backend latency probe ceilings aligned.
import { promises as fs } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => fs.readFile(path.join(root, file), 'utf8')
const [frontend, backendPreset, backendProbe] = await Promise.all([
  read('src/services/delay.ts'),
  read('src-tauri/src/config/verge.rs'),
  read('src-tauri/src/cmd/clash5_data.rs'),
])

const checks = [
  [
    'frontend default is 10',
    frontend.includes('DEFAULT_DELAY_CONCURRENCY = 10'),
  ],
  ['frontend ceiling is 16', frontend.includes('MAX_DELAY_CONCURRENCY = 16')],
  ['frontend has no legacy default 36', !frontend.includes('concurrency = 36')],
  [
    'backend balanced default is 10',
    backendPreset.includes('Self::Balanced => 10'),
  ],
  [
    'backend hard ceiling is 16',
    backendProbe.includes('PROBE_CONCURRENCY: usize = 16'),
  ],
]

const failures = checks.filter(([, ok]) => !ok)
for (const [name, ok] of checks) console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}`)
if (failures.length) process.exit(1)
console.log('Frontend and backend latency probe limits are aligned.')
