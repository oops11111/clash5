import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => readFile(path.join(root, file), 'utf8')
const config = JSON.parse(await read('src-tauri/tauri.conf.json'))
const security = config.app.security

for (const [mode, csp] of Object.entries({
  production: security.csp,
  development: security.devCsp,
})) {
  assert.ok(csp && typeof csp === 'object', `${mode}: CSP must be explicit`)
  assert.equal(csp['default-src'], "'self'")
  assert.equal(csp['object-src'], "'none'")
  assert.equal(csp['base-uri'], "'none'")
  assert.equal(csp['frame-src'], "'none'")
  assert.equal(csp['form-action'], "'none'")
  assert.ok(!JSON.stringify(csp).includes('unsafe-eval'))
  assert.ok(
    !csp['connect-src']
      .split(' ')
      .some((s) => ['*', 'https:', 'http:', 'ws:', 'wss:'].includes(s)),
  )
}
assert.equal(security.csp['script-src'], "'self'")
assert.deepEqual(security.assetProtocol.scope.allow, ['$APPDATA/icons/**'])

for (const file of ['migrated.json', 'desktop.json', 'desktop-windows.json']) {
  const capability = JSON.parse(await read(`src-tauri/capabilities/${file}`))
  for (const permission of capability.permissions) {
    if (typeof permission === 'string') {
      assert.ok(
        ![
          'fs:allow-write-file',
          'core:window:allow-create',
          'core:webview:allow-create-webview',
          'core:webview:allow-create-webview-window',
        ].includes(permission),
        `${file}: unexpected permission ${permission}`,
      )
    } else if (permission.identifier === 'fs:scope') {
      assert.deepEqual(permission.allow, ['$APPDATA/**'])
    }
  }
}
const monaco = await read('src/services/monaco.ts')
assert.match(monaco, /enableSchemaRequest: false/)
assert.doesNotMatch(monaco, /enableSchemaRequest: true/)
console.log(
  'WebView static security boundary checks passed; desktop runtime checks remain required.',
)
