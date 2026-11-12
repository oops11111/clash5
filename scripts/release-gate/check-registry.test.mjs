import test from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import {
  validateRegistry,
  validateAutomatedEvidence,
  validateWebviewSecurity,
  validateBackupRestoreSecurity,
  verifyAutomatedEvidence,
  verifyNoInAppCoreUpgrade,
  verifyWebviewSecurity,
  verifyBackupRestoreSecurity,
} from './check-registry.mjs'

const registry = await readFile(
  new URL('../../RELEASE-GATES.md', import.meta.url),
  'utf8',
)

test('release gate registry covers all reviewed gate groups', () => {
  assert.equal(validateRegistry(registry), 24)
})

test('the in-app core upgrade command and implementation are absent', async () => {
  await verifyNoInAppCoreUpgrade()
})

test('P0-06 traversal and absolute-path regression tests remain registered', async () => {
  await verifyAutomatedEvidence()
})

test('registry validation fails if a required gate test is removed', async () => {
  const backupSource = await readFile(
    new URL('../../src-tauri/src/feat/backup.rs', import.meta.url),
    'utf8',
  )
  assert.throws(
    () =>
      validateAutomatedEvidence(
        backupSource.replace(
          'fn export_rejects_traversal()',
          'fn removed_test()',
        ),
      ),
    /missing automated test export_rejects_traversal/u,
  )
})

test('P0-04 keeps scoped assets, local workers, and system dialogs', async () => {
  await verifyWebviewSecurity()
})

test('P0-13 enforces strict WebDAV TLS and staged archive restore bounds', async () => {
  await verifyBackupRestoreSecurity()
})

test('P0-13 rejects invalid-certificate WebDAV configuration', () => {
  assert.throws(
    () =>
      validateBackupRestoreSecurity('.danger_accept_invalid_certs(true)', ''),
    /must validate certificates/u,
  )
})

test('registry policy rejects an unrestricted filesystem scope', async () => {
  const config = JSON.parse(
    await readFile(
      new URL('../../src-tauri/tauri.conf.json', import.meta.url),
      'utf8',
    ),
  )
  const capability = JSON.parse(
    await readFile(
      new URL('../../src-tauri/capabilities/migrated.json', import.meta.url),
      'utf8',
    ),
  )
  const monaco = await readFile(
    new URL('../../src/services/monaco.ts', import.meta.url),
    'utf8',
  )
  capability.permissions
    .find((permission) => permission.identifier === 'fs:scope')
    .allow.push('**')
  assert.throws(
    () => validateWebviewSecurity(config, capability, monaco),
    /filesystem scopes must stay inside app data/u,
  )
})

test('registry policy rejects an unrestricted asset scope and network policy', async () => {
  const config = JSON.parse(
    await readFile(
      new URL('../../src-tauri/tauri.conf.json', import.meta.url),
      'utf8',
    ),
  )
  const capability = JSON.parse(
    await readFile(
      new URL('../../src-tauri/capabilities/migrated.json', import.meta.url),
      'utf8',
    ),
  )
  const monaco = await readFile(
    new URL('../../src/services/monaco.ts', import.meta.url),
    'utf8',
  )
  config.app.security.assetProtocol.scope.allow.push('**')
  assert.throws(
    () => validateWebviewSecurity(config, capability, monaco),
    /asset scope must stay inside/u,
  )
  config.app.security.assetProtocol.scope.allow.pop()
  config.app.security.csp['connect-src'] += ' https:'
  assert.throws(
    () => validateWebviewSecurity(config, capability, monaco),
    /CSP must preserve local workers/u,
  )
})

test('registry validation fails when a gate row is missing', () => {
  const incomplete = registry.replace(/^\| G-10 .*\r?\n/mu, '')
  assert.throws(() => validateRegistry(incomplete), /expected 24 gate rows/u)
})

test('registry validation fails for an unknown state', () => {
  const invalid = registry
    .replace('| G-01 |', '| G-01 |')
    .replace(' | OPEN |', ' | READY |')
  assert.throws(() => validateRegistry(invalid), /unknown state/u)
})
