import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), 'utf8')
const app = JSON.parse(await read('src-tauri/tauri.conf.json'))
const appId = app.identifier

assert.equal(app.productName, 'clash5')
assert.equal(appId, 'io.github.oops11111.clash5')

for (const platform of ['windows', 'macos', 'linux']) {
  const config = JSON.parse(await read(`src-tauri/tauri.${platform}.conf.json`))
  assert.equal(config.identifier, appId, `${platform} bundle identifier`)
}

const entitlements = await read('src-tauri/packages/macos/entitlements.plist')
assert.ok(
  !entitlements.includes('com.apple.security.application-groups'),
  'clash5 does not use an app group; do not inherit Clash Verge Rev shared storage',
)
assert.ok(!entitlements.includes('io.github.clash-verge-rev'))

const linux = JSON.parse(await read('src-tauri/tauri.linux.conf.json'))
assert.ok(linux.bundle.linux.deb.provides.includes('clash5'))
assert.ok(linux.bundle.externalBin.every((path) => path.includes('/clash5-')))
const desktop = await read('src-tauri/packages/linux/clash5.desktop')
assert.ok(desktop.includes('Exec={{{exec}}}'))
assert.ok(desktop.includes('MimeType=x-scheme-handler/clash5;'))
assert.ok(!desktop.includes('x-scheme-handler/clash;'))
assert.deepEqual(app.plugins['deep-link'].desktop.schemes, ['clash5'])

const windowsInstaller = await read('src-tauri/packages/windows/installer.nsi')
assert.ok(windowsInstaller.includes('clash5-service-install.exe'))
assert.ok(!/DeleteRegKey[^\r\n]*Clash Verge/i.test(windowsInstaller))
assert.ok(!/RMDir[^\r\n]*Clash Verge/i.test(windowsInstaller))

const macService = await read('src-tauri/packages/macos/info_merge.plist')
assert.ok(
  macService.includes('io.github.clash-verge-rev.clash-verge-rev.dev.service'),
)
assert.ok(
  !macService.includes('io.github.clash-verge-rev.clash-verge-rev.service'),
)

console.log('R-18 platform identity audit passed')
