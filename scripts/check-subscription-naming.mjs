// Static release gate for token-safe subscription names and profile update logs.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => readFile(path.join(root, file), 'utf8')

const [prfitem, profiles, profile, command, tray, docs] = await Promise.all([
  read('src-tauri/src/config/prfitem.rs'),
  read('src-tauri/src/config/profiles.rs'),
  read('src-tauri/src/feat/profile.rs'),
  read('src-tauri/src/cmd/profile.rs'),
  read('src-tauri/src/core/tray/mod.rs'),
  read('docs/clash5/CLASH5-BACKLOG-ALL-ROUNDS-2026-11-04.md'),
])

const checks = [
  [
    'default names prefer disposition then host',
    prfitem.includes('filename_from_disposition') &&
      prfitem.includes('default_remote_profile_name(&url, filename)') &&
      prfitem.includes('url.host_str()'),
  ],
  [
    'legacy URL-tail names are migrated while custom names survive',
    profiles.includes('legacy_name') &&
      profiles.includes('each.name.as_deref() == legacy_name.as_deref()') &&
      profiles.includes('each.name = item.name.clone()'),
  ],
  [
    'profile update logging masks URLs and user notices use display names',
    command.includes('mask_url(&url)') &&
      profile.includes('mask_url(') &&
      profile.includes('format!("{profile_name} - {last_err}")'),
  ],
  [
    'tray tooltip uses the stored display name only',
    tray.includes('profile.name') &&
      tray.includes('current_profile_name') &&
      !tray.includes('profile.url'),
  ],
  [
    'backlog records the remaining desktop redaction evidence',
    docs.includes('P0-10') && docs.includes('外部桌面') && docs.includes('G-12'),
  ],
]

for (const [name, ok] of checks) {
  assert.ok(ok, `subscription naming policy failed: ${name}`)
  console.log(`ok   ${name}`)
}
console.log(`Validated ${checks.length} subscription naming policy checks.`)
