import { readFile } from 'node:fs/promises'

const files = [
  'scripts/prebuild.mjs',
  'src-tauri/tauri.conf.json',
  'src-tauri/tauri.linux.conf.json',
  'src-tauri/packages/windows/installer.nsi',
  'src-tauri/src/lib.rs',
  'src-tauri/src/cmd/mod.rs',
  'src-tauri/src/core/mod.rs',
  'src/services/cmds.ts',
  'src/services/tauri-commands.generated.ts',
  'src/components/setting/setting-clash.tsx',
]

const forbidden = /enableLoopback\.exe|invoke_uwp_tool|uwp-tool|win_uwp/
const violations = []
for (const file of files) {
  const source = await readFile(file, 'utf8')
  if (forbidden.test(source)) violations.push(file)
}

if (violations.length) {
  throw new Error(`loopback helper remains in production files: ${violations.join(', ')}`)
}

console.log('Windows loopback helper packaging audit passed')
