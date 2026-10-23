import { Channel, invoke } from '@tauri-apps/api/core'
import type { DownloadEvent as UpdaterDownloadEvent } from '@tauri-apps/plugin-updater'
import dayjs from 'dayjs'

import {
  nodeFromWire,
  profilesFromWire,
  proxyViewFromWire,
  vergeFromWire,
  withoutNulls,
} from '@/services/command-wire'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'
import type {
  Binding as GeneratedBinding,
  ChainWorkspace as GeneratedChainWorkspace,
  ClashInfo,
  CommandFailure_Serialize,
  DownloadEvent as ContractDownloadEvent,
  IProfiles_Serialize,
  IVerge_Serialize,
  MemoryMitigationStatus,
  NetworkInterfaceWire,
  NodeRecord,
  PendingIdentityRebind as GeneratedPendingIdentityRebind,
  PluginAuditEvent as GeneratedPluginAuditEvent,
  PluginEffectiveConfig as GeneratedPluginEffectiveConfig,
  ProxyViewV1_Serialize,
  RunStateView,
  RunningMode as GeneratedRunningMode,
  ServiceInstallOutcome as GeneratedServiceInstallOutcome,
  UnlockItem as GeneratedUnlockItem,
  ValidationOutcome,
  WebDavBackupEntry,
} from '@/services/tauri-commands.generated'
import type {
  ChainWorkspace,
  Clash5Chain,
  ExitIpResult,
} from '@/types/clash5-chain'
import type {
  Clash5PluginBinding,
  Clash5PluginBindingInput,
  Clash5PluginBindingBatchInput,
  Clash5PluginAuditEvent,
  Clash5PluginEffectiveConfig,
  Clash5PluginPackagePreview,
  Clash5PluginSummary,
} from '@/types/clash5-plugin'
import type { ProxyViewV1 } from '@/types/proxy-view'
import { debugLog } from '@/utils/debug'

export async function copyClashEnv() {
  return invoke<null>('copy_clash_env')
}

export const getMemoryMitigationStatus = () =>
  invoke<MemoryMitigationStatus>('clash5_memory_mitigation_status')

export async function getProfiles() {
  return profilesFromWire(await invoke<IProfiles_Serialize>('get_profiles'))
}

export const getClash5ChainWorkspace = () =>
  invoke<GeneratedChainWorkspace>(
    'clash5_chain_workspace',
  ) as Promise<ChainWorkspace>
export const saveClash5Chain = (chain: Clash5Chain) =>
  invoke<null>('clash5_save_chain', { chain })
export const deleteClash5Chain = (id: string) =>
  invoke<null>('clash5_delete_chain', { id })
export const testClash5Chain = (id: string, testUrl: string | null = null) =>
  invoke<{ delay_ms: number; group_name: string }>('clash5_test_chain', {
    id,
    testUrl,
  })
export const activateClash5Chain = (id: string, policyGroup: string) =>
  invoke<{ selection_persisted: boolean }>('clash5_activate_chain', {
    id,
    policyGroup,
  })
export const getClash5SingboxDegraded = () =>
  invoke<boolean>('clash5_singbox_degraded')
export const detectClash5ExitIp = (id: string, serviceUrl: string | null) =>
  invoke<ExitIpResult>('clash5_detect_exit_ip', { id, serviceUrl })

export async function listClash5Nodes(includeInactive = false) {
  return (
    await invoke<NodeRecord[]>('clash5_list_nodes', { includeInactive })
  ).map(nodeFromWire)
}

function pluginRecord(value: unknown): Record<string, unknown> {
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, unknown>
  }
  throw new Error('Invalid plugin data')
}

export const listClash5PluginBindings = () =>
  invoke<Clash5PluginBinding[]>('clash5_list_plugin_bindings')
export const listClash5PluginAuditEvents = async (
  limit = 50,
): Promise<Clash5PluginAuditEvent[]> =>
  (
    await invoke<GeneratedPluginAuditEvent[]>(
      'clash5_list_plugin_audit_events',
      { limit },
    )
  ).map((event) => ({ ...event, details: pluginRecord(event.details) }))
export const getClash5PluginBindingConfig = (
  target: Clash5PluginBinding['target'],
  bindingId: string,
) =>
  invoke<unknown>('clash5_get_plugin_binding_config', {
    target,
    bindingId,
  }).then(pluginRecord)
export const saveClash5PluginBinding = (binding: Clash5PluginBindingInput) =>
  mutate(() => invoke<null>('clash5_save_plugin_binding', { binding }), {
    id: `clash5.plugin.binding.${JSON.stringify(binding.target)}`,
  })
export const saveClash5PluginBindings = (
  bindings: Clash5PluginBindingBatchInput,
) =>
  mutate(() => invoke<null>('clash5_save_plugin_bindings', { bindings }), {
    id: 'clash5.plugin.bindings.batch',
  })
export const deleteClash5PluginBinding = (
  target: Clash5PluginBinding['target'],
  bindingId: string,
) =>
  mutate(
    () => invoke<null>('clash5_delete_plugin_binding', { target, bindingId }),
    {
      id: `clash5.plugin.binding.delete.${bindingId}`,
    },
  )
export const setClash5PluginBindingEnabled = (
  target: Clash5PluginBinding['target'],
  bindingId: string,
  enabled: boolean,
) =>
  mutate(
    () =>
      invoke<null>('clash5_set_plugin_binding_enabled', {
        target,
        bindingId,
        enabled,
      }),
    { id: `clash5.plugin.binding.enabled.${bindingId}` },
  )
export const setClash5PluginEnabled = (pluginId: string, enabled: boolean) =>
  mutate(
    () => invoke<null>('clash5_set_plugin_enabled', { pluginId, enabled }),
    { id: `clash5.plugin.enabled.${pluginId}` },
  )
export const getClash5PluginEffectiveConfig =
  async (): Promise<Clash5PluginEffectiveConfig> => {
    const result = await invoke<GeneratedPluginEffectiveConfig>(
      'clash5_get_plugin_effective_config',
    )
    return {
      ...result,
      config: result.config === null ? null : pluginRecord(result.config),
    }
  }
export const listClash5Plugins = () =>
  invoke<Clash5PluginSummary[]>('clash5_list_plugins')
export const stageClash5PluginPackage = () =>
  invoke<Clash5PluginPackagePreview | null>('clash5_stage_plugin_package')
export const installStagedClash5Plugin = (
  stagingId: string,
  confirmedSigner: string,
) =>
  invoke<Clash5PluginSummary>('clash5_install_staged_plugin', {
    stagingId,
    confirmedSigner,
  })

export type Clash5PendingIdentityRebind = GeneratedPendingIdentityRebind
export type Clash5Binding = GeneratedBinding
export type UnlockItem = GeneratedUnlockItem

export interface Clash5OrphanCleanupPreview {
  retention_days: number
  total_orphans: number
  eligible_orphans: number
}

export async function getClash5IdentityStatus() {
  return invoke<{ recovery_mode: boolean; pending_rebinds: number }>(
    'clash5_identity_status',
  )
}

export async function resetClash5Identity() {
  return invoke<null>('clash5_reset_identity')
}

export async function rotateClash5Identity() {
  return invoke<null>('clash5_rotate_identity')
}

export async function listClash5PendingIdentityRebinds() {
  return invoke<Clash5PendingIdentityRebind[]>(
    'clash5_list_pending_identity_rebinds',
    { source: null },
  )
}

export async function listClash5Bindings() {
  return invoke<Clash5Binding[]>('clash5_list_bindings', { source: null })
}

export async function getClash5OrphanCleanupPreview() {
  return invoke<Clash5OrphanCleanupPreview>('clash5_orphan_cleanup_preview')
}

export async function setClash5OrphanRetentionDays(days: number) {
  return invoke<null>('clash5_set_orphan_retention_days', { days })
}

export async function purgeExpiredClash5Orphans() {
  return invoke<number | null>('clash5_purge_expired_orphans')
}

export async function bindClash5Node(binding: Clash5Binding) {
  return invoke<null>('clash5_bind_node', { binding })
}

export async function resolveClash5IdentityRebind(
  source: Clash5PendingIdentityRebind['source'],
  oldId: string,
  newId: string,
) {
  return invoke<null>('clash5_resolve_identity_rebind', {
    source,
    oldId,
    newId,
  })
}

export async function importClash5ManualProxies(proxies: IProxyConfig[]) {
  return invoke<{ imported: number }>('clash5_import_manual_proxies', {
    proxies,
  })
}

export interface VergeImportReport {
  imported: number
  skipped: string[]
  binding_sync_failures: number
  source_manifest_before: string
  source_manifest_after: string
}

export async function importClash5VergeData(sourceDir: string) {
  return invoke<VergeImportReport>('clash5_import_verge_data', {
    sourceDir,
  })
}

export async function probeClash5Nodes() {
  return invoke<{
    probed: number
    cached: number
    unavailable: number
    failed: number
  }>('clash5_probe_nodes')
}

export async function enhanceProfiles() {
  return (
    (await invoke<ValidationOutcome>('enhance_profiles')).status === 'valid'
  )
}

export async function patchProfilesConfig(profiles: IProfilesConfig) {
  return invoke<ValidationOutcome>('patch_profiles_config', { profiles })
}

export async function createProfile(
  item: Partial<IProfileItem>,
  fileData?: string | null,
) {
  return invoke<null>('create_profile', { item, fileData: fileData ?? null })
}

export async function viewProfile(index: string) {
  return invoke<null>('view_profile', { index })
}

export async function readProfileFile(index: string) {
  return invoke<string>('read_profile_file', { index })
}

export async function saveProfileFile(index: string, fileData: string) {
  return (
    (
      await invoke<ValidationOutcome>('save_profile_file', {
        index,
        fileData,
      })
    ).status === 'valid'
  )
}

export async function importProfile(url: string, option?: IProfileOption) {
  return invoke<null>('import_profile', {
    url,
    option: option || { with_proxy: true },
  })
}

export async function reorderProfile(activeId: string, overId: string) {
  return invoke<null>('reorder_profile', {
    activeId,
    overId,
  })
}

export async function updateProfile(index: string, option?: IProfileOption) {
  return invoke<null>('update_profile', { index, option: option ?? null })
}

export async function deleteProfile(index: string) {
  return invoke<null>('delete_profile', { index })
}

export async function patchProfile(
  index: string,
  profile: Partial<IProfileItem>,
) {
  return invoke<null>('patch_profile', { index, profile })
}

export async function getClashInfo() {
  return withoutNulls(await invoke<ClashInfo>('get_clash_info'))
}

// Fallback mode read independent of strict mihomo `/configs` deserialization.
export async function getClashMode() {
  return invoke<string | null>('get_clash_mode')
}

export async function getRuntimeConfig() {
  return (await invoke<unknown | null>(
    'get_runtime_config',
  )) as IConfigData | null
}

export async function getRuntimeYaml() {
  return invoke<string>('get_runtime_yaml')
}

export async function getRuntimeLogs() {
  return invoke<Record<string, [string, string][]>>('get_runtime_logs')
}

export async function updateProxyChainConfigInRuntime(
  proxyChainConfig: unknown,
) {
  return invoke<null>('update_proxy_chain_config_in_runtime', {
    proxyChainConfig,
  })
}

export async function patchClashConfig(payload: Partial<IConfigData>) {
  return invoke<null>('patch_clash_config', { payload })
}

export async function patchClashMode(payload: string) {
  return invoke<null>('patch_clash_mode', { payload })
}

export async function syncTrayProxySelection() {
  return invoke<null>('sync_tray_proxy_selection')
}

/** Sends one selection pair so the backend can merge against current profile state. */
export async function recordSelectedNode(groupName: string, node: string) {
  return invoke<null>('record_selected_node', { groupName, node })
}

export async function forgetSelectedNode(groupName: string) {
  return invoke<null>('forget_selected_node', { groupName })
}

export async function getProxyView(): Promise<ProxyViewV1> {
  return proxyViewFromWire(
    await invoke<ProxyViewV1_Serialize>('get_proxy_view'),
  )
}

export async function getClashLogs() {
  const regex = /time="(.+?)"\s+level=(.+?)\s+msg="(.+?)"/
  const newRegex = /(.+?)\s+(.+?)\s+(.+)/
  const logs = await invoke<string[]>('get_clash_logs')

  return logs.reduce<ILogItem[]>((acc, log) => {
    const result = log.match(regex)
    if (result) {
      const [_, _time, type, payload] = result
      const time = dayjs(_time).format('MM-DD HH:mm:ss')
      acc.push({ time, type, payload })
      return acc
    }

    const result2 = log.match(newRegex)
    if (result2) {
      const [_, time, type, payload] = result2
      acc.push({ time, type, payload })
    }
    return acc
  }, [])
}

export async function getVergeConfig() {
  return vergeFromWire(await invoke<IVerge_Serialize>('get_verge_config'))
}

export async function patchVergeConfig(payload: IVergeConfig) {
  return invoke<null>('patch_verge_config', { payload })
}

export async function setDnsOverride(
  profileUid: string,
  enabled: boolean,
  confirmation?: string,
) {
  return invoke<
    { status: 'applied' } | { status: 'confirmation_required'; source: string }
  >('set_dns_override', {
    profileUid,
    enabled,
    confirmation: confirmation ?? null,
  })
}

export async function getDnsConfigContent() {
  return invoke<string | null>('get_dns_config_content')
}

export async function saveDnsConfig(dnsConfig: Record<string, unknown>) {
  return invoke<null>('save_dns_config', { dnsConfig })
}

export async function validateDnsConfig() {
  return invoke<ValidationOutcome>('validate_dns_config')
}

export async function applyDnsConfig(apply: boolean) {
  return invoke<null>('apply_dns_config', { apply })
}

export async function takeDnsOverrideNotice() {
  return invoke<boolean>('take_dns_override_notice')
}

export type ServiceFallbackNotice =
  | { kind: 'unavailable' }
  | { kind: 'coreRejected'; reason: string }
  | { kind: 'notAutoStarted' }

export async function takeServiceFallbackNotice() {
  return invoke<ServiceFallbackNotice | null>('take_service_fallback_notice')
}

export interface CoreFailure {
  kind: 'startFailed' | 'serviceCoreStopped'
  detail: string
}

export async function getCoreStartupError() {
  return invoke<CoreFailure | null>('get_core_startup_error')
}

export async function takeServiceRepairNotice() {
  return invoke<boolean>('take_service_repair_notice')
}

export async function takeServiceOwnerNotice() {
  return invoke<string | null>('take_service_owner_notice')
}

export async function takeDiscardedKeysNotice() {
  return invoke<string | null>('take_discarded_keys_notice')
}

export async function getSystemProxy() {
  return invoke<{
    enable: boolean
    server: string
    bypass: string
  }>('get_sys_proxy')
}

export async function getAutotemProxy() {
  try {
    debugLog('[API] 开始调用 get_auto_proxy')
    const result = await invoke<{
      enable: boolean
      url: string
    }>('get_auto_proxy')
    debugLog('[API] get_auto_proxy 调用成功:', result)
    return result
  } catch (error) {
    console.error('[API] get_auto_proxy 调用失败:', error)
    return {
      enable: false,
      url: '',
    }
  }
}

export async function getEmbeddedServerPort() {
  return invoke<number>('get_embedded_server_port')
}

export async function changeClashCore(clashCore: string) {
  const failure = await invoke<CommandFailure_Serialize | null>(
    'change_clash_core',
    { clashCore },
  )
  return failure === null ? null : withoutNulls(failure)
}

export async function restartCore() {
  return invoke<null>('restart_core')
}

export interface CoreUpgradeReport {
  /** False when the managed core was already at the latest version. */
  upgraded: boolean
  from: string
  to: string
}

export async function upgradeClashCore(force = false) {
  return invoke<CoreUpgradeReport>('upgrade_clash_core', { force })
}

export async function restartApp() {
  return invoke<null>('restart_app')
}

export async function installUpdate(
  version: string,
  onEvent: (event: UpdaterDownloadEvent) => void,
) {
  const channel = new Channel<ContractDownloadEvent>()
  channel.onmessage = (event) => {
    if (event.event === 'Started') {
      const { contentLength } = event.data
      onEvent({
        event: 'Started',
        data: contentLength == null ? {} : { contentLength },
      })
    } else {
      onEvent(event)
    }
  }
  return invoke<boolean>('install_update', { version, onEvent: channel })
}

export async function cancelUpdateDownload() {
  return invoke<void>('cancel_update_download')
}

export async function getAppDir() {
  return invoke<string>('get_app_dir')
}

export async function openAppDir() {
  return invoke<null>('open_app_dir').catch((err) => showNotice.error(err))
}

export async function openCoreDir() {
  return invoke<null>('open_core_dir').catch((err) => showNotice.error(err))
}

export async function openLogsDir() {
  return invoke<null>('open_logs_dir').catch((err) => showNotice.error(err))
}

export async function syncRuntimeProviders() {
  return invoke<void>('sync_runtime_providers').catch((err) => {
    console.warn('failed to queue the provider cache sync', err)
  })
}

export async function cmdTestDelay(url: string) {
  return invoke<number>('test_delay', { url })
}

export async function invoke_uwp_tool() {
  return invoke<null>('invoke_uwp_tool').catch((err) =>
    showNotice.error(err, 1500),
  )
}

export async function openDevTools() {
  return invoke<void>('open_devtools')
}

export async function exitApp() {
  return invoke<void>('exit_app')
}

export async function exportDiagnosticInfo() {
  return invoke<null>('export_diagnostic_info')
}

interface SystemInfo {
  system_name: string
  system_version: string
  system_kernel_version: string
  system_arch: string
  app_version: string
  app_core_mode: string
  app_is_admin: boolean
}

export async function getSystemInfo() {
  return invoke<SystemInfo>('get_system_info')
}

export async function copyIconFile(
  path: string,
  name: 'common' | 'sysproxy' | 'tun',
) {
  const key = `icon_${name}_update_time`
  const previousTime = localStorage.getItem(key) || ''

  const currentTime = String(Date.now())
  localStorage.setItem(key, currentTime)

  const iconInfo = {
    name,
    previous_t: previousTime,
    current_t: currentTime,
  }

  return invoke<string>('copy_icon_file', { path, iconInfo })
}

export async function downloadIconCache(url: string, name: string) {
  return invoke<string>('download_icon_cache', { url, name })
}

export async function getNetworkInterfaces() {
  return invoke<string[]>('get_network_interfaces')
}

export async function getSystemHostname() {
  return invoke<string>('get_system_hostname')
}

export async function getNetworkInterfacesInfo() {
  return withoutNulls(
    await invoke<NetworkInterfaceWire[]>('get_network_interfaces_info'),
  )
}

export async function createWebdavBackup() {
  return invoke<null>('create_webdav_backup')
}

export async function createLocalBackup() {
  return invoke<null>('create_local_backup')
}

export async function deleteWebdavBackup(filename: string) {
  return invoke<null>('delete_webdav_backup', { filename })
}

export async function deleteLocalBackup(filename: string) {
  return invoke<null>('delete_local_backup', { filename })
}

export async function restoreWebDavBackup(filename: string) {
  return invoke<null>('restore_webdav_backup', { filename })
}

export async function restoreLocalBackup(filename: string) {
  return invoke<null>('restore_local_backup', { filename })
}

export async function importLocalBackup(source: string) {
  return invoke<string>('import_local_backup', { source })
}

export async function exportLocalBackup(filename: string, destination: string) {
  return invoke<null>('export_local_backup', { filename, destination })
}

export async function saveWebdavConfig(
  url: string,
  username: string,
  password: string,
) {
  return invoke<null>('save_webdav_config', {
    url,
    username,
    password,
  })
}

export async function listWebDavBackup() {
  const list = await invoke<WebDavBackupEntry[]>('list_webdav_backup')
  return list.map(
    (item): IWebDavFile => ({
      ...item,
      filename: item.href.split('/').pop() ?? '',
      tag: item.tag ?? '',
    }),
  )
}

export async function listLocalBackup() {
  return invoke<ILocalBackupFile[]>('list_local_backup')
}

export type RunningMode = GeneratedRunningMode
export type RunState = RunStateView

export const getRuntimeState = async () => {
  return invoke<RunStateView>('get_runtime_state')
}

export type FailedOperation =
  | 'systemProxyEnable'
  | 'systemProxyDisable'
  | 'systemProxyRestore'
  | 'systemProxyGuard'

export interface PendingFailure {
  code: string
  detail: string
  operation: FailedOperation
  sequence: number
}

export const getPendingFailures = async () => {
  return invoke<PendingFailure[]>('get_pending_failures')
}

export interface SidecarFailureSnapshot {
  revision: number
  detail: string | null
}

export const getSidecarFailure = async () => {
  return invoke<SidecarFailureSnapshot>('get_sidecar_failure')
}

export const getAppUptime = async () => {
  return invoke<number>('get_app_uptime')
}

export type ServiceInstallOutcome = GeneratedServiceInstallOutcome

export const installService = async () => {
  return invoke<ServiceInstallOutcome>('install_service')
}

export const uninstallService = async () => {
  return invoke<null>('uninstall_service')
}

export const reinstallService = async () => {
  return invoke<ServiceInstallOutcome>('reinstall_service')
}

export const repairService = async () => {
  return invoke<ServiceInstallOutcome>('repair_service')
}

export const continueWithSidecar = async () => {
  return invoke<null>('continue_with_sidecar')
}

export const entry_lightweight_mode = async () => {
  return invoke<null>('entry_lightweight_mode')
}

export async function getNextUpdateTime(uid: string) {
  return invoke<number | null>('get_next_update_time', { uid })
}

interface ToggleableProxyPort {
  enabled: boolean
  port: number
}

export interface ProxyPortSettings {
  mixedPort: number
  socks: ToggleableProxyPort
  http: ToggleableProxyPort
  redir: ToggleableProxyPort
  tproxy: ToggleableProxyPort
}

export type ListenerTransport = 'tcp' | 'udp'

export interface ListenerProbe {
  address: string
  transports: ListenerTransport[]
}

export type ListenerProbeOutcome =
  | { status: 'available' }
  | {
      status: 'conflict'
      port: number
      transport: ListenerTransport
    }
  | { status: 'invalid'; message: string }
  | { status: 'indeterminate'; message: string }

export type SaveProxyPortsOutcome =
  | { status: 'saved' }
  | { status: 'conflict'; port: number; transport: ListenerTransport }

export const probeListener = async (request: ListenerProbe) => {
  return invoke<ListenerProbeOutcome>('probe_listener', { request })
}

export const saveProxyPorts = async (settings: ProxyPortSettings) => {
  return invoke<SaveProxyPortsOutcome>('save_proxy_ports', { settings })
}

export async function getUnlockItems() {
  return invoke<UnlockItem[]>('get_unlock_items')
}

export async function checkMediaUnlock(onComplete: Channel<UnlockItem>) {
  return invoke<UnlockItem[]>('check_media_unlock', { onComplete })
}

export async function checkMediaUnlockItem(name: string) {
  return invoke<UnlockItem>('check_media_unlock_item', { name })
}
