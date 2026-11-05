// Static release gate for the plugin signer lifecycle. Runtime desktop and
// four-case package rejection evidence remain separate acceptance artifacts.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => readFile(path.join(root, file), 'utf8')

const [plugins, commands, registry, page, catalog, docs] = await Promise.all([
  read('src-tauri/src/clash5/plugins.rs'),
  read('src-tauri/src/cmd/clash5_data.rs'),
  read('src-tauri/src/lib.rs'),
  read('src/pages/plugins.tsx'),
  read('src-tauri/src/clash5/catalog.rs'),
  read('docs/clash5/m3-plugins.md'),
])

const checks = [
  [
    'persistent signer file is bounded and validated',
    plugins.includes('TRUSTED_SIGNERS_FILE') &&
      plugins.includes('TRUSTED_SIGNERS_MAX_BYTES') &&
      plugins.includes('minisign_verify::PublicKey::decode'),
  ],
  [
    'bundled and persistent signers are exposed separately',
    plugins.includes('pub(crate) struct PluginTrustedSigner') &&
      plugins.includes('pub(crate) fn trusted_signers') &&
      plugins.includes('BUNDLED_SIGNER'),
  ],
  [
    'plugin load and enable paths require trusted signers',
    plugins.includes('if !signer_is_trusted(plugin_root, signer)?') &&
      plugins.includes('&& signer_is_trusted(plugin_root, signer)?'),
  ],
  [
    'independent trust, list and revoke commands are registered',
    commands.includes('pub async fn clash5_list_trusted_plugin_signers') &&
      commands.includes('pub async fn clash5_trust_plugin_signer') &&
      commands.includes('pub async fn clash5_revoke_plugin_signer') &&
      registry.split('cmd::clash5_list_trusted_plugin_signers').length >= 3 &&
      registry.split('cmd::clash5_trust_plugin_signer').length >= 3,
  ],
  ['bundled signer cannot be revoked', commands.includes('the bundled signer cannot be revoked')],
  [
    'settings UI previews fingerprint and supports explicit trust/revoke',
    page.includes('trustedSignerPreviewFingerprint') &&
      page.includes('trustClash5PluginSigner') &&
      page.includes('listClash5TrustedPluginSigners') &&
      page.includes('!signer.bundled'),
  ],
  [
    'catalog freshness and sequence rollback protections remain enabled',
    catalog.includes('ensure_fresh') && catalog.includes('ensure_sequence_not_older'),
  ],
  [
    'plugin documentation describes independent signer management',
    docs.includes('独立列出受信签名者') && docs.includes('plugin.signer.trust'),
  ],
]

for (const [name, ok] of checks) {
  assert.ok(ok, `plugin trust policy failed: ${name}`)
  console.log(`ok   ${name}`)
}
console.log(`Validated ${checks.length} plugin trust policy checks.`)
