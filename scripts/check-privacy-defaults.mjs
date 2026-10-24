// Checks that the privacy-relevant defaults in the code match what PRIVACY.md says.
// Reads source and documentation only; it does not start the application.
import { promises as fs } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => fs.readFile(path.join(ROOT, file), 'utf8')

const checks = [
  {
    name: 'update check is off by default',
    file: 'src-tauri/src/config/verge.rs',
    ok: (text) => text.includes('auto_check_update: Some(false)'),
  },
  {
    name: 'Home IP card is off by default',
    file: 'src/pages/home.tsx',
    ok: (text) => /DEFAULT_HOME_CARDS[\s\S]*?\bip: false,/.test(text),
  },
  {
    name: 'Miscellaneous update check defaults to off in the UI',
    file: 'src/components/setting/mods/misc-viewer.tsx',
    ok: (text) =>
      text.includes('autoCheckUpdate: false,') &&
      text.includes('auto_check_update ?? false'),
  },
  {
    name: 'DNS listener binds loopback by default',
    file: 'src-tauri/src/utils/init.rs',
    ok: (text) => text.includes('Value::String("127.0.0.1:53".into())'),
  },
  {
    name: 'default DNS has no plaintext UDP upstream and no system bootstrap',
    file: 'src-tauri/src/utils/init.rs',
    ok: (text) => {
      const start = text.indexOf('fn default_dns_section()')
      const end = text.indexOf('pub(super) async fn init_dns_config()')
      const section = text.slice(start, end)
      return (
        start >= 0 &&
        end > start &&
        !section.includes('"8.8.8.8"') &&
        !section.includes('"system"')
      )
    },
  },
  {
    name: 'controller secret has no constant fallback except the legacy comparison',
    file: 'src-tauri/src/config/clash.rs',
    ok: (text) => text.split('"set-your-secret"').length === 2,
  },
  {
    name: 'PRIVACY.md states the IP card is off by default',
    file: 'PRIVACY.md',
    ok: (text) => text.includes('**off by default, opt-in**'),
  },
  {
    name: 'PRIVACY.md states the DNS listener is loopback-only',
    file: 'PRIVACY.md',
    ok: (text) => text.includes('binds `127.0.0.1` only'),
  },
  {
    name: 'PRIVACY.md does not reference upstream update hosts',
    file: 'PRIVACY.md',
    ok: (text) =>
      !text.includes('update.hwdns.net') && !text.includes('gh-proxy.org'),
  },
]

let failures = 0
for (const check of checks) {
  const text = await read(check.file)
  if (check.ok(text)) {
    console.log(`ok   ${check.name}`)
  } else {
    failures += 1
    console.error(
      `::error file=${check.file}::privacy default mismatch: ${check.name}`,
    )
  }
}

if (failures > 0) {
  console.error(`${failures} privacy default check(s) failed`)
  process.exit(1)
}
console.log('All privacy defaults match PRIVACY.md.')
