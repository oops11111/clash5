// Static release gate for the persisted proxy lease and abnormal-exit path.
// OS-level kill/restart acceptance remains a desktop artifact.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (file) => readFile(path.join(root, file), 'utf8')
const serviceRevision = '6cc9a8aee1dcc4e40fdd22469a42b31cc4cc77b4'
const [proxy, state, budget, tests, gates, cargoManifest, cargoLock] =
  await Promise.all([
    read('src-tauri/src/core/proxy_control.rs'),
    read('src-tauri/src/core/manager/state.rs'),
    read('src-tauri/src/core/restart_budget.rs'),
    read('src-tauri/tests/proxy_recovery.rs'),
    read('RELEASE-GATES.md'),
    read('src-tauri/Cargo.toml'),
    read('Cargo.lock'),
  ])

const checks = [
  [
    'proxy writes use a persisted lease',
    proxy.includes('LeaseStore::new') &&
      proxy.includes('store.begin(lease_owner(), target)') &&
      proxy.includes('store.settle(lease_owner(), target)'),
  ],
  [
    'unexpected exits clear only owned state and preserve foreign markers',
    proxy.includes('clear_after_unexpected_exit') &&
      proxy.includes('ForeignProxyPolicy::PreserveMarker') &&
      proxy.includes('clear_and_retire_lease'),
  ],
  [
    'sidecar cleanup is generation- and PID-scoped',
    state.includes('invalidate_core_readiness_if(core_readiness_generation)') &&
      state.includes('should_clear_terminated_sidecar') &&
      state.includes('request_sidecar_recovery'),
  ],
  [
    'recovery is bounded by the rolling restart budget',
    budget.includes('MAX_RESTARTS') &&
      budget.includes('RESTART_WINDOW') &&
      budget.includes('try_start'),
  ],
  [
    'isolated ownership, failure and budget tests exist',
    tests.includes('abnormal_exit_clears_own_proxy') &&
      tests.includes(
        'mixed_protocol_proxy_does_not_clear_another_application',
      ) &&
      tests.includes('persistent_budget_allows_only_three_attempts'),
  ],
  [
    'service watchdog is pinned to the matching three-per-five-minute policy',
    cargoManifest.includes(
      `https://github.com/oops11111/clash5-service-ipc.git", rev = "${serviceRevision}"`,
    ) &&
      cargoLock.includes(
        `git+https://github.com/oops11111/clash5-service-ipc.git?rev=${serviceRevision}#${serviceRevision}`,
      ),
  ],
  [
    'release gate records the remaining desktop evidence',
    gates.includes('| G-09 | P0-07 | partial |') &&
      gates.includes(
        'actual Windows process-kill/startup recovery remain unverified',
      ),
  ],
]

for (const [name, ok] of checks) {
  assert.ok(ok, `proxy recovery policy failed: ${name}`)
  console.log(`ok   ${name}`)
}
console.log(`Validated ${checks.length} proxy recovery policy checks.`)
