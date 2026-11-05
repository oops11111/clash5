// Static release gate for installer-owned task cleanup.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => readFile(path.join(root, file), 'utf8')
const [installer, tasks, docs] = await Promise.all([
  read('src-tauri/packages/windows/installer.nsi'),
  read('src-tauri/src/utils/schtasks.rs'),
  read('docs/clash5/CLASH5-BACKLOG-ALL-ROUNDS-2026-11-04.md'),
])

const checks = [
  [
    'Windows uninstaller removes both app-owned scheduled tasks',
    installer.includes('RemoveClash5AutoLaunchTasks') &&
      installer.includes('schtasks.exe /Delete /TN "clash5" /F') &&
      installer.includes('schtasks.exe /Delete /TN "clash5 (Admin)" /F') &&
      installer.includes('!insertmacro RemoveClash5AutoLaunchTasks'),
  ],
  [
    'runtime task names match installer cleanup names',
    tasks.includes('const TASK_NAME_USER: &str = "clash5"') &&
      tasks.includes('const TASK_NAME_ADMIN: &str = "clash5 (Admin)"'),
  ],
  [
    'backlog records the remaining proxy-ownership desktop evidence',
    docs.includes('P0-09') && docs.includes('仅当代理仍指向本应用时复位') && docs.includes('真机 VM'),
  ],
]

for (const [name, ok] of checks) {
  assert.ok(ok, `uninstall cleanup policy failed: ${name}`)
  console.log(`ok   ${name}`)
}
console.log(`Validated ${checks.length} uninstall cleanup policy checks.`)
