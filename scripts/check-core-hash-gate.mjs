import assert from 'node:assert/strict'
import fs from 'node:fs'

const prebuild = fs.readFileSync('scripts/prebuild.mjs', 'utf8')
const service = fs.readFileSync('src-tauri/src/core/service.rs', 'utf8')
const installer = fs.readFileSync('src-tauri/packages/windows/installer.nsi', 'utf8')

assert.match(prebuild, /CORE_HASHES_JSON\s*=\s*path\.join\(RESOURCES_DIR, ['"]core-hashes\.json['"]\)/)
assert.match(prebuild, /schema_version:\s*1/)
assert.match(prebuild, /sha256:\s*digest/)
assert.match(service, /join\(['"]core-hashes\.json['"]\)/)
assert.match(service, /verify_packaged_core_hash\(&executable, &core\.name, &core\.path\)/)
assert.match(service, /verify_packaged_core_hash[\s\S]*?invoke_service_install\(&cores, false\)/)
assert.match(installer, /--install-core.*--sha256.*\$\{MIHOMO_SHA256\}/)

console.log('core hash gate audit passed')
