import assert from 'node:assert/strict'
import fs from 'node:fs'

const proxyControl = fs.readFileSync('src-tauri/src/core/proxy_control.rs', 'utf8')
const sysopt = fs.readFileSync('src-tauri/src/core/sysopt.rs', 'utf8')
const diagnosticCommand = fs.readFileSync(
  'crates/tauri-plugin-clash-verge-sysinfo/src/commands.rs',
  'utf8',
)
const logger = fs.readFileSync('src-tauri/src/core/logger.rs', 'utf8')

assert.doesNotMatch(proxyControl, /crashlytics/i)
assert.doesNotMatch(sysopt, /crashlytics/i)
assert.match(diagnosticCommand, /clash_verge_logging::redact/)
assert.match(diagnosticCommand, /clipboard\.write_text/)
assert.match(logger, /panic hook/i)

console.log('local diagnostics audit passed')
