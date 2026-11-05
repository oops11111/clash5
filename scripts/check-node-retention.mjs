// Static release gate for source-scoped node retention and the inactive-node UI.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => readFile(path.join(root, file), 'utf8')

const [test, page, en, zh, docs, chains, materialized, profiles, deleteProfile] = await Promise.all([
  read('crates/clash5-data/tests/m1.rs'),
  read('src/pages/node-pool.tsx'),
  read('src/locales/en/nodePool.json'),
  read('src/locales/zh/nodePool.json'),
  read('docs/clash5/CLASH5-BACKLOG-ALL-ROUNDS-2026-11-04.md'),
  read('src-tauri/src/clash5/chains.rs'),
  read('src-tauri/src/clash5/materialized.rs'),
  read('src-tauri/src/config/profiles.rs'),
  read('src-tauri/src/cmd/profile.rs'),
])

const checks = [
  [
    'subscription deletion retains a recoverable node and orphan binding',
    test.includes('removing_a_subscription_keeps_referenced_node_and_binding_as_recoverable_records') &&
      test.includes('node_records(true)') &&
      test.includes('effective_bindings'),
  ],
  [
    'node pool marks inactive records as not effective',
    page.includes("t('nodePool.notEffective')") &&
      page.includes("t('nodePool.notEffectiveHint')"),
  ],
  [
    'runtime catalog uses the installation identity for subscription nodes',
    chains.includes('runtime_identity_key') &&
      chains.includes('store.identity_key().cloned()') &&
      chains.includes('subscription_catalog_identity_uses_the_installation_key'),
  ],
  [
    'removed subscription nodes are retained as encrypted runtime snapshots',
    materialized.includes('serialize_encrypted') &&
      materialized.includes('snapshot_profile_yaml') &&
      materialized.includes('load_for_profile') &&
      materialized.includes('proxy-provider:{profile_id}:{name}') &&
      materialized.includes('read_provider_payload') &&
      chains.includes('strip_prefix(&format!("proxy-provider:{profile_id}:"))') &&
      chains.includes('provider.get("override")') &&
      profiles.includes('materialized::snapshot_profile_yaml') &&
      deleteProfile.includes('snapshot_materialized_profile'),
  ],
  [
    'English and Chinese inactive-node copy is present',
    en.includes('"notEffective"') &&
      en.includes('"notEffectiveHint"') &&
      zh.includes('"notEffective"') &&
      zh.includes('"notEffectiveHint"'),
  ],
  [
    'backlog records the remaining runtime materialization evidence',
    docs.includes('P0-02') && docs.includes('未生效') && docs.includes('订阅删除被引用节点后仍可用'),
  ],
]

for (const [name, ok] of checks) {
  assert.ok(ok, `node retention policy failed: ${name}`)
  console.log(`ok   ${name}`)
}
console.log(`Validated ${checks.length} node retention policy checks.`)
