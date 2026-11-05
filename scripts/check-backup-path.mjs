import { execFileSync } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const output = path.join(root, '.cache-native', `backup-path-tests${process.platform === 'win32' ? '.exe' : ''}`)

mkdirSync(path.dirname(output), { recursive: true })
const testEnv = {
  ...process.env,
  TEMP: path.join(root, '.cache-native'),
  TMP: path.join(root, '.cache-native'),
}
try {
  execFileSync(
    'rustc',
    ['--edition', '2024', '--test', path.join(root, 'src-tauri/src/feat/backup_path.rs'), '-o', output],
    { cwd: root, stdio: 'inherit', env: testEnv },
  )
  execFileSync(output, [], { cwd: root, stdio: 'inherit', env: testEnv })
} finally {
  rmSync(output, { force: true })
}

console.log('Backup path regression tests passed.')
