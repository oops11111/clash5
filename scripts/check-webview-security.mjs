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
  assert.equal(csp['form-action'], "'none'")
  assert.equal(csp['frame-src'], "'none'")
  assert.ok(!JSON.stringify(csp).includes('unsafe-eval'))

  const connectSources = csp['connect-src']?.split(/\s+/) ?? []
  assert.ok(connectSources.length > 0, `${mode}: connect-src must be explicit`)
  assert.ok(
    !connectSources.some(
      (source) =>
        ['*', 'http:', 'https:', 'ws:', 'wss:'].includes(source) ||
        /:\/\/\*/.test(source),
    ),
    `${mode}: connect-src must not allow arbitrary destinations`,
  )
}

assert.equal(security.csp['script-src'], "'self'")
assert.deepEqual(security.assetProtocol.scope.allow, ['$APPDATA/icons/**'])
assert.equal(security.assetProtocol.scope.requireLiteralLeadingDot, true)

for (const file of ['migrated.json', 'desktop.json', 'desktop-windows.json']) {
  const capability = JSON.parse(await read(`src-tauri/capabilities/${file}`))
  for (const permission of capability.permissions) {
    if (typeof permission === 'string') {
      if (permission.startsWith('fs:')) {
        assert.equal(
          permission,
          'fs:allow-read-text-file',
          `${file}: unexpected filesystem permission ${permission}`,
        )
      }
    } else if (permission.identifier === 'fs:scope') {
      assert.fail(
        `${file}: broad filesystem scope must come from a native picker`,
      )
    }
  }
}

const migrated = JSON.parse(await read('src-tauri/capabilities/migrated.json'))
assert.ok(migrated.permissions.includes('fs:allow-read-text-file'))
const monaco = await read('src/services/monaco.ts')
assert.match(monaco, /enableSchemaRequest:\s*false/)
assert.doesNotMatch(monaco, /enableSchemaRequest:\s*true/)

const profiles = await read('src/pages/profiles.tsx')
assert.match(
  profiles,
  /openDialog\(\{[\s\S]*?extensions:\s*\['yaml',\s*'yml'\]/,
)
const backupImport = await read('src/components/setting/mods/backup-viewer.tsx')
const backupExport = await read(
  'src/components/setting/mods/backup-history-viewer.tsx',
)
assert.match(backupImport, /openDialog\(/)
assert.match(backupExport, /\bsave\(/)

console.log(
  'WebView static boundaries passed; native dialog scope and denied-path behavior still require desktop acceptance.',
)
