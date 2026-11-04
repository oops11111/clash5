import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (relative) => readFile(path.join(root, relative), 'utf8')
const config = JSON.parse(await read('src-tauri/tauri.conf.json'))
assert.deepEqual(config.plugins['deep-link'].desktop.schemes, ['clash5'])

const init = await read('src-tauri/src/utils/init.rs')
assert.match(init, /Software\\\\Classes\\\\Clash5/)
assert.match(init, /const DEEP_LINK_SCHEMES: &\[&str\] = &\["clash5"\]/)
assert.doesNotMatch(init, /create_subkey\("Software\\\\Classes\\\\Clash\\\\/)
assert.doesNotMatch(
  init,
  /DEEP_LINK_SCHEMES[^\n]*clash-verge|DEEP_LINK_SCHEMES[^\n]*\["clash"/,
)

const resolver = await read('src-tauri/src/utils/resolve/scheme.rs')
assert.match(resolver, /link_parsed\.scheme\(\) != "clash5"/)
assert.doesNotMatch(resolver, /"clash-verge"|"clash"/)

const desktop = await read('src-tauri/packages/linux/clash5.desktop')
assert.match(desktop, /x-scheme-handler\/clash5;/)
assert.doesNotMatch(desktop, /x-scheme-handler\/clash;/)
console.log(
  'Deep-link registration and runtime parsing are restricted to clash5://.',
)
