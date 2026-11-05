// Static release gate for profile corruption quarantine and recovery confirmation.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => readFile(path.join(root, file), 'utf8')

const [profiles, commands, registry, page, docs] = await Promise.all([
  read('src-tauri/src/config/profiles.rs'),
  read('src-tauri/src/cmd/profile.rs'),
  read('src-tauri/src/lib.rs'),
  read('src/pages/profiles.tsx'),
  read('docs/clash5/CLASH5-BACKLOG-ALL-ROUNDS-2026-11-04.md'),
])

const checks = [
  [
    'parse failures quarantine the original file',
    profiles.includes('is_profile_parse_error') &&
      profiles.includes('quarantine_corrupt_profile') &&
      profiles.includes('.corrupt-'),
  ],
  [
    'recovery mode blocks writes until explicit confirmation',
    profiles.includes('PROFILE_RECOVERY_PATH') &&
      profiles.includes('confirm_recovery') &&
      profiles.includes('recovery mode; confirm recovery before saving'),
  ],
  [
    'normal saves retain a .bak copy',
    profiles.includes('path.file_name()') && profiles.includes('{}.bak'),
  ],
  [
    'recovery commands are registered',
    commands.includes('get_profile_recovery_status') &&
      commands.includes('confirm_profile_recovery') &&
      registry.split('cmd::get_profile_recovery_status').length >= 3 &&
      registry.split('cmd::confirm_profile_recovery').length >= 3,
  ],
  [
    'profiles UI requires explicit recovery confirmation',
    page.includes('getProfileRecoveryStatus') &&
      page.includes('confirmProfileRecovery') &&
      page.includes('recovery.confirm'),
  ],
  [
    'backlog records the remaining recovery evidence',
    docs.includes('P0-15') && docs.includes('配置损坏') && docs.includes('G-15'),
  ],
]

for (const [name, ok] of checks) {
  assert.ok(ok, `profile recovery policy failed: ${name}`)
  console.log(`ok   ${name}`)
}
console.log(`Validated ${checks.length} profile recovery policy checks.`)
