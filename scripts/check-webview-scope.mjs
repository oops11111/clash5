import { readFile } from 'node:fs/promises'
import path from 'node:path'

/**
 * Guards the WebView permission surface (release gate G-04).
 *
 *   node scripts/check-webview-scope.mjs
 *
 * Fails when any of these regress:
 * - a filesystem or asset-protocol scope allows `**` or a parent escape
 * - the Tauri CSP is missing (null) or lacks a `default-src 'self'`
 * - the frontend HTTP allow-list grants hosts outside the IP lookup set
 *   used by src/services/api.ts (the only plugin-http consumer)
 * - open_devtools lost its release-build guard in src-tauri/src/cmd/app.rs
 *
 * Adding a new fetched host requires updating both desktop.json and the
 * allow-list below; the gate fails closed until then.
 */

const ROOT = path.resolve(import.meta.dirname, '..')

const HTTP_ALLOW_LIST = new Set([
  'https://api.ip.sb/*',
  'https://ipapi.co/*',
  'https://api.ipapi.is/*',
  'https://ipwho.is/*',
  'https://ip.api.skk.moe/*',
  'https://get.geojs.io/*',
])

const failures = []
const fail = (message) => failures.push(message)

const readJson = async (relative) =>
  JSON.parse(await readFile(path.join(ROOT, relative), 'utf8'))

const walk = (value, visit) => {
  if (Array.isArray(value)) {
    value.forEach((entry) => {
      walk(entry, visit)
    })
  } else if (value && typeof value === 'object') {
    Object.entries(value).forEach(([key, entry]) => {
      visit(key, entry)
      walk(entry, visit)
    })
  }
}

const checkScopeAllow = (file, data) => {
  walk(data, (key, entry) => {
    if (key !== 'allow' || !Array.isArray(entry)) return
    entry.forEach((item) => {
      const url = typeof item === 'string' ? item : item?.url
      if (typeof url !== 'string') return
      if (url === '**' || url.startsWith('**/')) {
        fail(`${file}: unscoped wildcard ${JSON.stringify(url)}`)
      }
      if (url.includes('..')) {
        fail(`${file}: parent-escape scope ${JSON.stringify(url)}`)
      }
    })
  })
}

const tauriConf = await readJson('src-tauri/tauri.conf.json')
const csp = tauriConf?.app?.security?.csp
if (!csp || typeof csp !== 'object') {
  fail('src-tauri/tauri.conf.json: CSP is missing or null')
} else if (!String(csp['default-src'] ?? '').includes("'self'")) {
  fail("src-tauri/tauri.conf.json: CSP default-src must include 'self'")
}
checkScopeAllow(
  'src-tauri/tauri.conf.json',
  tauriConf?.app?.security?.assetProtocol?.scope,
)

for (const file of [
  'src-tauri/capabilities/desktop.json',
  'src-tauri/capabilities/desktop-windows.json',
  'src-tauri/capabilities/migrated.json',
]) {
  // The opener scope (http://* / https://*) is intentionally out of this
  // check: it opens the system browser for user-initiated links, not the
  // WebView. Only filesystem/asset scopes are wildcard-guarded here; the
  // HTTP fetch allow-list is checked separately below.
  checkScopeAllow(file, await readJson(file))
}

const desktop = await readJson('src-tauri/capabilities/desktop.json')
walk(desktop, (key, entry) => {
  if (key !== 'allow' || !Array.isArray(entry)) return
  entry.forEach((item) => {
    const url = typeof item === 'string' ? item : item?.url
    if (typeof url !== 'string' || !/^https?:\/\//.test(url)) return
    if (!HTTP_ALLOW_LIST.has(url)) {
      fail(`desktop.json: HTTP URL not allow-listed: ${url}`)
    }
  })
})

const appRs = await readFile(
  path.join(ROOT, 'src-tauri/src/cmd/app.rs'),
  'utf8',
)
if (!appRs.includes('#[cfg(not(debug_assertions))]')) {
  fail('src-tauri/src/cmd/app.rs: open_devtools release-build guard is missing')
}

if (failures.length > 0) {
  failures.forEach((message) => {
    console.error(`error: ${message}`)
  })
  process.exit(1)
}
console.log('WebView scope guard passed.')
