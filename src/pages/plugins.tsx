import ExtensionOutlinedIcon from '@mui/icons-material/ExtensionOutlined'
import {
  Alert,
  Button,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControl,
  InputLabel,
  MenuItem,
  Paper,
  Select,
  Stack,
  Tab,
  TextField,
  Tabs,
  Typography,
} from '@mui/material'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import { IdentityRecoveryPanel } from '@/components/identity-recovery-panel'
import {
  getClash5PluginBindingConfig,
  installStagedClash5Plugin,
  getClash5PluginEffectiveConfig,
  getClash5ChainWorkspace,
  listClash5Nodes,
  listClash5PluginBindings,
  listClash5PluginAuditEvents,
  listClash5Plugins,
  revokeClash5PluginSigner,
  setClash5PluginEnabled,
  stageClash5PluginPackage,
  saveClash5PluginBindings,
  deleteClash5PluginBinding,
  setClash5PluginBindingEnabled,
  saveClash5PluginBinding,
} from '@/services/cmds'
import type {
  Clash5PluginBinding,
  Clash5PluginAuditEvent,
  Clash5PluginBindingInput,
  Clash5PluginEffectiveConfig,
  Clash5PluginPackagePreview,
  Clash5PluginSummary,
} from '@/types/clash5-plugin'
import type { Clash5PluginBindingTarget } from '@/types/clash5-plugin'

type PluginTab = 'installed' | 'market' | 'binding' | 'settings'

const tabs: PluginTab[] = ['installed', 'market', 'binding', 'settings']

const valueAtPointer = (
  config: Record<string, unknown> | null,
  pointer: string,
) => {
  if (!config) return undefined
  return pointer
    .split('/')
    .slice(1)
    .map((segment) => segment.replaceAll('~1', '/').replaceAll('~0', '~'))
    .reduce<unknown>((value, segment) => {
      if (!value || typeof value !== 'object') return undefined
      return (value as Record<string, unknown>)[segment]
    }, config)
}

type BindingTargetOption = {
  key: string
  label: string
  target: Clash5PluginBindingTarget
}

const PluginsPage = () => {
  const { t } = useTranslation()
  const [tab, setTab] = useState<PluginTab>('installed')
  const [bindings, setBindings] = useState<Clash5PluginBinding[] | null>(null)
  const [bindingError, setBindingError] = useState(false)
  const [effectiveConfig, setEffectiveConfig] =
    useState<Clash5PluginEffectiveConfig | null>(null)
  const [plugins, setPlugins] = useState<Clash5PluginSummary[] | null>(null)
  const [auditEvents, setAuditEvents] = useState<
    Clash5PluginAuditEvent[] | null
  >(null)
  const [auditError, setAuditError] = useState(false)
  const [pluginError, setPluginError] = useState(false)
  const [packagePreview, setPackagePreview] =
    useState<Clash5PluginPackagePreview | null>(null)
  const [busy, setBusy] = useState(false)
  const [signerToRevoke, setSignerToRevoke] = useState<string | null>(null)
  const [bindingDialogOpen, setBindingDialogOpen] = useState(false)
  const [targetOptions, setTargetOptions] = useState<BindingTargetOption[]>([])
  const [bindingPluginId, setBindingPluginId] = useState('')
  const [bindingTargetKeys, setBindingTargetKeys] = useState<string[]>([])
  const [bindingPriority, setBindingPriority] = useState('0')
  const [bindingConfigText, setBindingConfigText] = useState('{}')
  const [bindingFormError, setBindingFormError] = useState(false)
  const [bindingEditId, setBindingEditId] = useState<string | null>(null)
  const [bindingEditEnabled, setBindingEditEnabled] = useState(true)

  useEffect(() => {
    if (tab !== 'binding') return
    let cancelled = false
    listClash5PluginBindings()
      .then((items) => {
        if (!cancelled) setBindings(items)
      })
      .catch(() => {
        if (!cancelled) setBindingError(true)
      })
    getClash5PluginEffectiveConfig()
      .then((items) => {
        if (!cancelled) setEffectiveConfig(items)
      })
      .catch(() => {
        if (!cancelled) setBindingError(true)
      })
    Promise.all([
      getClash5ChainWorkspace(),
      listClash5Nodes(true),
      listClash5Plugins(),
    ])
      .then(([workspace, nodes, installed]) => {
        if (cancelled) return
        setPlugins(installed)
        setTargetOptions([
          ...workspace.chains.map((chain) => ({
            key: JSON.stringify({ kind: 'chain', chain_id: chain.id }),
            label: t('layout.components.navigation.empty.bindingTargetChain', {
              id: chain.name,
            }),
            target: {
              kind: 'chain',
              chain_id: chain.id,
            } as Clash5PluginBindingTarget,
          })),
          ...nodes
            .filter((node) => node.active)
            .map((node) => ({
              key: JSON.stringify({
                kind: 'node',
                source: node.source,
                stable_id: node.stable_id,
              }),
              label: t('layout.components.navigation.empty.bindingTargetNode', {
                id: node.display_name,
              }),
              target: {
                kind: 'node',
                source: node.source,
                stable_id: node.stable_id,
              } as Clash5PluginBindingTarget,
            })),
        ])
      })
      .catch(() => {
        if (!cancelled) setBindingError(true)
      })
    return () => {
      cancelled = true
    }
  }, [tab, t])

  useEffect(() => {
    if (tab !== 'installed' && tab !== 'settings') return
    let cancelled = false
    listClash5Plugins()
      .then((items) => {
        if (!cancelled) setPlugins(items)
      })
      .catch(() => {
        if (!cancelled) setPluginError(true)
      })
    if (tab === 'settings') {
      listClash5PluginAuditEvents()
        .then((items) => {
          if (!cancelled) setAuditEvents(items)
        })
        .catch(() => {
          if (!cancelled) setAuditError(true)
        })
    }
    return () => {
      cancelled = true
    }
  }, [tab, t])

  const installPackage = async () => {
    if (!packagePreview) return
    setBusy(true)
    try {
      await installStagedClash5Plugin(
        packagePreview.staging_id,
        packagePreview.signer,
      )
      setPackagePreview(null)
      setPlugins(await listClash5Plugins())
    } catch {
      setPluginError(true)
    } finally {
      setBusy(false)
    }
  }

  const togglePlugin = async (plugin: Clash5PluginSummary) => {
    setBusy(true)
    try {
      await setClash5PluginEnabled(plugin.id, !plugin.enabled)
      setPlugins(await listClash5Plugins())
    } catch {
      setPluginError(true)
    } finally {
      setBusy(false)
    }
  }

  const revokeSigner = async () => {
    if (!signerToRevoke) return
    setBusy(true)
    try {
      await revokeClash5PluginSigner(signerToRevoke)
      setSignerToRevoke(null)
      setPlugins(await listClash5Plugins())
    } catch {
      setPluginError(true)
    } finally {
      setBusy(false)
    }
  }

  const saveBinding = async () => {
    const targets = targetOptions.filter((option) =>
      bindingTargetKeys.includes(option.key),
    )
    const priority = Number(bindingPriority)
    if (
      !targets.length ||
      !bindingPluginId ||
      !Number.isSafeInteger(priority)
    ) {
      setBindingFormError(true)
      return
    }
    let config: Record<string, unknown>
    try {
      const parsed: unknown = JSON.parse(bindingConfigText)
      if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
        setBindingFormError(true)
        return
      }
      config = parsed as Record<string, unknown>
    } catch {
      setBindingFormError(true)
      return
    }
    if (bindingEditId && targets.length !== 1) {
      setBindingFormError(true)
      return
    }
    const batch: Clash5PluginBindingInput[] = targets.map(({ target }) => ({
      id: bindingEditId ?? crypto.randomUUID(),
      plugin_id: bindingPluginId,
      target,
      priority,
      enabled: bindingEditId ? bindingEditEnabled : true,
      config,
    }))
    setBusy(true)
    try {
      if (bindingEditId) await saveClash5PluginBinding(batch[0])
      else await saveClash5PluginBindings(batch)
      setBindings(await listClash5PluginBindings())
      setEffectiveConfig(await getClash5PluginEffectiveConfig())
      setBindingDialogOpen(false)
      setBindingTargetKeys([])
      setBindingEditId(null)
      setBindingFormError(false)
    } catch {
      setBindingFormError(true)
    } finally {
      setBusy(false)
    }
  }

  const openNewBinding = () => {
    setBindingEditId(null)
    setBindingEditEnabled(true)
    setBindingPluginId('')
    setBindingTargetKeys([])
    setBindingPriority('0')
    setBindingConfigText('{}')
    setBindingFormError(false)
    setBindingDialogOpen(true)
  }

  const editBinding = async (binding: Clash5PluginBinding) => {
    const targetOption = targetOptions.find(
      (option) =>
        JSON.stringify(option.target) === JSON.stringify(binding.target),
    )
    if (!targetOption) {
      setBindingError(true)
      return
    }
    setBusy(true)
    try {
      const config = await getClash5PluginBindingConfig(
        binding.target,
        binding.id,
      )
      setBindingEditId(binding.id)
      setBindingEditEnabled(binding.enabled)
      setBindingPluginId(binding.plugin_id)
      setBindingTargetKeys([targetOption.key])
      setBindingPriority(String(binding.priority))
      setBindingConfigText(JSON.stringify(config, null, 2))
      setBindingFormError(false)
      setBindingDialogOpen(true)
    } catch {
      setBindingError(true)
    } finally {
      setBusy(false)
    }
  }

  const removeBinding = async (binding: Clash5PluginBinding) => {
    setBusy(true)
    try {
      await deleteClash5PluginBinding(binding.target, binding.id)
      setBindings(await listClash5PluginBindings())
      setEffectiveConfig(await getClash5PluginEffectiveConfig())
    } catch {
      setBindingError(true)
    } finally {
      setBusy(false)
    }
  }

  const setBindingEnabled = async (binding: Clash5PluginBinding) => {
    setBusy(true)
    try {
      await setClash5PluginBindingEnabled(
        binding.target,
        binding.id,
        !binding.enabled,
      )
      setBindings(await listClash5PluginBindings())
      setEffectiveConfig(await getClash5PluginEffectiveConfig())
    } catch {
      setBindingError(true)
    } finally {
      setBusy(false)
    }
  }

  const stagePackage = async () => {
    setBusy(true)
    setPluginError(false)
    try {
      const preview = await stageClash5PluginPackage()
      if (preview) setPackagePreview(preview)
    } catch {
      setPluginError(true)
    } finally {
      setBusy(false)
    }
  }

  const renderInstalled = () => {
    if (pluginError) {
      return (
        <Alert severity="error">
          {t('layout.components.navigation.empty.pluginError')}
        </Alert>
      )
    }
    if (plugins === null) {
      return (
        <Typography color="text.secondary">
          {t('layout.components.navigation.empty.pluginLoading')}
        </Typography>
      )
    }
    return (
      <Stack spacing={2}>
        <Stack direction="row" sx={{ justifyContent: 'flex-end' }}>
          <Button
            variant="contained"
            onClick={() => void stagePackage()}
            disabled={busy}
          >
            {t('layout.components.navigation.pluginInstall')}
          </Button>
        </Stack>
        {plugins.length === 0 ? (
          <Paper variant="outlined" sx={{ p: 4, borderRadius: 3 }}>
            <Stack
              spacing={1}
              sx={{ alignItems: 'center', textAlign: 'center' }}
            >
              <ExtensionOutlinedIcon color="primary" fontSize="large" />
              <Typography variant="h6">
                {t('layout.components.navigation.empty.installedTitle')}
              </Typography>
              <Typography color="text.secondary">
                {t('layout.components.navigation.empty.installedDescription')}
              </Typography>
            </Stack>
          </Paper>
        ) : (
          plugins.map((plugin) => (
            <Paper
              key={plugin.id}
              variant="outlined"
              sx={{ p: 2, borderRadius: 2 }}
            >
              <Stack spacing={1}>
                <Stack direction="row" sx={{ justifyContent: 'space-between' }}>
                  <Typography sx={{ fontWeight: 600 }}>
                    {plugin.name}
                  </Typography>
                  <Chip size="small" label={`v${plugin.version}`} />
                </Stack>
                <Typography variant="body2" color="text.secondary">
                  {plugin.description}
                </Typography>
                <Typography variant="caption" color="text.secondary">
                  {t('layout.components.navigation.pluginSigner', {
                    fingerprint: plugin.signer_fingerprint,
                  })}
                </Typography>
                <Stack
                  direction="row"
                  spacing={0.75}
                  useFlexGap
                  sx={{ flexWrap: 'wrap' }}
                >
                  {plugin.permissions.map((permission) => (
                    <Chip key={permission} size="small" label={permission} />
                  ))}
                  {plugin.writable_paths.map((path) => (
                    <Chip
                      key={path}
                      size="small"
                      variant="outlined"
                      label={path}
                    />
                  ))}
                </Stack>
                <Stack direction="row" sx={{ justifyContent: 'flex-end' }}>
                  <Button
                    size="small"
                    disabled={busy}
                    onClick={() => void togglePlugin(plugin)}
                  >
                    {t(
                      plugin.enabled
                        ? 'layout.components.navigation.pluginDisableBinding'
                        : 'layout.components.navigation.pluginEnableBinding',
                    )}
                  </Button>
                </Stack>
              </Stack>
            </Paper>
          ))
        )}
      </Stack>
    )
  }

  const renderSettings = () => {
    const auditPanel = (
      <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
        <Stack spacing={1}>
          <Typography sx={{ fontWeight: 600 }}>
            {t('layout.components.navigation.pluginAuditTitle')}
          </Typography>
          {auditError ? (
            <Alert severity="error">
              {t('layout.components.navigation.pluginAuditError')}
            </Alert>
          ) : auditEvents === null ? (
            <Typography color="text.secondary">
              {t('layout.components.navigation.empty.pluginLoading')}
            </Typography>
          ) : auditEvents.length === 0 ? (
            <Typography color="text.secondary">
              {t('layout.components.navigation.pluginAuditEmpty')}
            </Typography>
          ) : (
            auditEvents.map((event) => (
              <Stack
                key={event.id}
                direction="row"
                spacing={1}
                useFlexGap
                sx={{ flexWrap: 'wrap', alignItems: 'center' }}
              >
                <Typography variant="caption" color="text.secondary">
                  {new Date(event.created_at).toLocaleString()}
                </Typography>
                <Chip size="small" label={event.event} />
                <Typography variant="body2">
                  {String(event.details.plugin_id ?? '')}
                </Typography>
                {typeof event.details.binding_id === 'string' && (
                  <Typography variant="caption" color="text.secondary">
                    {event.details.binding_id}
                  </Typography>
                )}
                {typeof event.details.result === 'string' && (
                  <Chip
                    size="small"
                    color={
                      event.details.result === 'success' ? 'success' : 'error'
                    }
                    label={event.details.result}
                  />
                )}
              </Stack>
            ))
          )}
        </Stack>
      </Paper>
    )
    if (pluginError) {
      return (
        <Stack spacing={2}>
          <Alert severity="error">
            {t('layout.components.navigation.empty.pluginError')}
          </Alert>
          {auditPanel}
        </Stack>
      )
    }
    if (plugins === null) {
      return (
        <Stack spacing={2}>
          <Typography color="text.secondary">
            {t('layout.components.navigation.empty.pluginLoading')}
          </Typography>
          {auditPanel}
        </Stack>
      )
    }
    if (plugins.length === 0) {
      return (
        <Stack spacing={2}>
          <Paper variant="outlined" sx={{ p: 4, borderRadius: 3 }}>
            <Stack
              spacing={1}
              sx={{ alignItems: 'center', textAlign: 'center' }}
            >
              <ExtensionOutlinedIcon color="primary" fontSize="large" />
              <Typography variant="h6">
                {t('layout.components.navigation.empty.settingsTitle')}
              </Typography>
              <Typography color="text.secondary">
                {t('layout.components.navigation.empty.settingsDescription')}
              </Typography>
            </Stack>
          </Paper>
          {auditPanel}
        </Stack>
      )
    }
    return (
      <Stack spacing={2}>
        <Typography color="text.secondary">
          {t('layout.components.navigation.pluginSettingsHint')}
        </Typography>
        <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
          <Stack spacing={1}>
            <Typography sx={{ fontWeight: 600 }}>
              {t('layout.components.navigation.pluginSignersTitle')}
            </Typography>
            {Array.from(
              new Set(plugins.map((plugin) => plugin.signer_fingerprint)),
            ).map((fingerprint) => (
              <Stack
                key={fingerprint}
                direction="row"
                sx={{ justifyContent: 'space-between', alignItems: 'center' }}
              >
                <Typography variant="body2" sx={{ wordBreak: 'break-all' }}>
                  {t('layout.components.navigation.pluginSigner', {
                    fingerprint,
                  })}
                  {` (${plugins.filter((plugin) => plugin.signer_fingerprint === fingerprint).length})`}
                </Typography>
                <Button
                  size="small"
                  color="error"
                  disabled={busy}
                  onClick={() => setSignerToRevoke(fingerprint)}
                >
                  {t('layout.components.navigation.pluginSignerRevoke')}
                </Button>
              </Stack>
            ))}
          </Stack>
        </Paper>
        <Dialog
          open={signerToRevoke !== null}
          onClose={() => !busy && setSignerToRevoke(null)}
        >
          <DialogTitle>
            {t('layout.components.navigation.pluginSignerRevokeTitle')}
          </DialogTitle>
          <DialogContent>
            <Stack spacing={1}>
              <Typography>
                {t(
                  'layout.components.navigation.pluginSignerRevokeDescription',
                )}
              </Typography>
              {plugins
                .filter(
                  (plugin) => plugin.signer_fingerprint === signerToRevoke,
                )
                .map((plugin) => (
                  <Typography key={plugin.id} variant="body2">
                    {plugin.name}
                  </Typography>
                ))}
            </Stack>
          </DialogContent>
          <DialogActions>
            <Button onClick={() => setSignerToRevoke(null)} disabled={busy}>
              {t('shared.actions.cancel')}
            </Button>
            <Button
              color="error"
              onClick={() => void revokeSigner()}
              disabled={busy}
            >
              {t('layout.components.navigation.pluginSignerRevokeConfirm')}
            </Button>
          </DialogActions>
        </Dialog>
        {plugins.map((plugin) => (
          <Paper
            key={plugin.id}
            variant="outlined"
            sx={{ p: 2, borderRadius: 2 }}
          >
            <Stack spacing={1}>
              <Typography sx={{ fontWeight: 600 }}>{plugin.name}</Typography>
              <Typography variant="body2" color="text.secondary">
                {plugin.description}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                {t('layout.components.navigation.pluginSigner', {
                  fingerprint: plugin.signer_fingerprint,
                })}
              </Typography>
              <Stack
                direction="row"
                spacing={0.75}
                useFlexGap
                sx={{ flexWrap: 'wrap' }}
              >
                {plugin.permissions.map((permission) => (
                  <Chip key={permission} size="small" label={permission} />
                ))}
                {plugin.writable_paths.map((path) => (
                  <Chip
                    key={path}
                    size="small"
                    variant="outlined"
                    label={path}
                  />
                ))}
              </Stack>
            </Stack>
          </Paper>
        ))}
        <Button
          sx={{ alignSelf: 'flex-start' }}
          onClick={() => setTab('binding')}
        >
          {t('layout.components.navigation.pluginTabs.binding')}
        </Button>
        {auditPanel}
      </Stack>
    )
  }

  const renderBindings = () => {
    if (bindingError) {
      return (
        <Alert severity="error">
          {t('layout.components.navigation.empty.bindingError')}
        </Alert>
      )
    }
    if (bindings === null) {
      return (
        <Typography color="text.secondary">
          {t('layout.components.navigation.empty.bindingLoading')}
        </Typography>
      )
    }
    if (bindings.length === 0) {
      return (
        <Stack spacing={1} sx={{ alignItems: 'center', textAlign: 'center' }}>
          <ExtensionOutlinedIcon color="primary" fontSize="large" />
          <Typography variant="h6">
            {t('layout.components.navigation.empty.bindingTitle')}
          </Typography>
          <Typography color="text.secondary">
            {t('layout.components.navigation.empty.bindingDescription')}
          </Typography>
          <Button
            onClick={openNewBinding}
            disabled={!plugins?.length || !targetOptions.length}
          >
            {t('layout.components.navigation.pluginAddBinding')}
          </Button>
        </Stack>
      )
    }
    return (
      <Stack spacing={1.5}>
        <Stack direction="row" sx={{ justifyContent: 'flex-end' }}>
          <Button
            onClick={openNewBinding}
            disabled={!plugins?.length || !targetOptions.length}
          >
            {t('layout.components.navigation.pluginAddBinding')}
          </Button>
        </Stack>
        {bindings.map((binding) => (
          <Paper
            key={binding.id}
            variant="outlined"
            sx={{ p: 2, borderRadius: 2 }}
          >
            <Stack
              direction={{ xs: 'column', sm: 'row' }}
              spacing={2}
              sx={{
                alignItems: { sm: 'center' },
                justifyContent: 'space-between',
              }}
            >
              <Stack spacing={0.5}>
                <Typography sx={{ fontWeight: 600 }}>
                  {binding.plugin_id}
                </Typography>
                <Typography variant="body2" color="text.secondary">
                  {binding.target.kind === 'chain'
                    ? t(
                        'layout.components.navigation.empty.bindingTargetChain',
                        {
                          id: binding.target.chain_id,
                        },
                      )
                    : t(
                        'layout.components.navigation.empty.bindingTargetNode',
                        {
                          id: binding.target.stable_id,
                        },
                      )}
                </Typography>
              </Stack>
              <Stack direction="row" spacing={1}>
                <Chip
                  size="small"
                  label={t(
                    'layout.components.navigation.empty.bindingPriority',
                    { priority: binding.priority },
                  )}
                />
                <Chip
                  size="small"
                  color={
                    binding.orphan
                      ? 'warning'
                      : binding.enabled
                        ? 'success'
                        : 'default'
                  }
                  label={t(
                    binding.orphan
                      ? 'layout.components.navigation.empty.bindingOrphan'
                      : binding.enabled
                        ? 'layout.components.navigation.empty.bindingEnabled'
                        : 'layout.components.navigation.empty.bindingDisabled',
                  )}
                />
                <Button
                  size="small"
                  color="error"
                  disabled={busy}
                  onClick={() => void removeBinding(binding)}
                >
                  {t('layout.components.navigation.pluginRemoveBinding')}
                </Button>
                <Button
                  size="small"
                  disabled={busy || binding.orphan}
                  onClick={() => void setBindingEnabled(binding)}
                >
                  {t(
                    binding.enabled
                      ? 'layout.components.navigation.pluginDisableBinding'
                      : 'layout.components.navigation.pluginEnableBinding',
                  )}
                </Button>
                <Button
                  size="small"
                  disabled={busy || binding.orphan}
                  onClick={() => void editBinding(binding)}
                >
                  {t('layout.components.navigation.pluginEditBinding')}
                </Button>
              </Stack>
            </Stack>
          </Paper>
        ))}
        {effectiveConfig &&
          Object.keys(effectiveConfig.provenance).length > 0 && (
            <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
              <Stack spacing={1}>
                <Typography sx={{ fontWeight: 600 }}>
                  {t('layout.components.navigation.pluginProvenanceTitle')}
                </Typography>
                {Object.entries(effectiveConfig.provenance).map(
                  ([path, source]) => {
                    const value = valueAtPointer(effectiveConfig.config, path)
                    const conflicts = effectiveConfig.conflicts.filter(
                      (conflict) => conflict.path === path,
                    )
                    return (
                      <Stack
                        key={path}
                        direction={{ xs: 'column', sm: 'row' }}
                        spacing={1}
                        sx={{ alignItems: { sm: 'center' } }}
                      >
                        <Typography
                          variant="body2"
                          sx={{ minWidth: 180, overflowWrap: 'anywhere' }}
                        >
                          {path}
                        </Typography>
                        <Typography
                          variant="body2"
                          color="text.secondary"
                          sx={{ flex: 1, overflowWrap: 'anywhere' }}
                        >
                          {t(
                            'layout.components.navigation.pluginEffectiveValue',
                          )}
                          : {JSON.stringify(value) ?? '—'}
                        </Typography>
                        <Typography variant="caption" color="text.secondary">
                          {source.kind === 'plugin'
                            ? `${source.plugin_id} · ${t('layout.components.navigation.empty.bindingPriority', { priority: source.priority })}`
                            : t(
                                'layout.components.navigation.pluginProvenanceProfile',
                              )}
                        </Typography>
                        {conflicts.length > 0 && (
                          <Chip
                            size="small"
                            color="warning"
                            label={t(
                              'layout.components.navigation.pluginConflictTitle',
                            )}
                          />
                        )}
                      </Stack>
                    )
                  },
                )}
                {effectiveConfig.conflicts.map((conflict) => (
                  <Typography
                    key={`${conflict.path}:${conflict.loser_binding_id}`}
                    variant="caption"
                    color="text.secondary"
                    sx={{ overflowWrap: 'anywhere' }}
                  >
                    {conflict.path} · {conflict.winner_plugin_id} (
                    {conflict.winner_priority}) → {conflict.loser_plugin_id} (
                    {conflict.loser_priority})
                  </Typography>
                ))}
              </Stack>
            </Paper>
          )}
      </Stack>
    )
  }

  return (
    <BasePage title={t('layout.components.navigation.tabs.plugins')}>
      <Stack spacing={2}>
        <Tabs
          value={tab}
          onChange={(_, value: PluginTab) => setTab(value)}
          variant="scrollable"
          scrollButtons="auto"
          aria-label={t('layout.components.navigation.tabs.plugins')}
        >
          {tabs.map((item) => (
            <Tab
              key={item}
              value={item}
              label={t(`layout.components.navigation.pluginTabs.${item}`)}
            />
          ))}
        </Tabs>
        {tab === 'installed' ? (
          renderInstalled()
        ) : tab === 'binding' ? (
          <Stack spacing={2}>
            {renderBindings()}
            <IdentityRecoveryPanel />
          </Stack>
        ) : tab === 'settings' ? (
          renderSettings()
        ) : (
          <Paper variant="outlined" sx={{ p: 4, borderRadius: 3 }}>
            <Stack
              spacing={1}
              sx={{ alignItems: 'center', textAlign: 'center' }}
            >
              <ExtensionOutlinedIcon color="primary" fontSize="large" />
              <Typography variant="h6">
                {t(`layout.components.navigation.empty.${tab}Title`)}
              </Typography>
              <Typography color="text.secondary">
                {t(`layout.components.navigation.empty.${tab}Description`)}
              </Typography>
            </Stack>
          </Paper>
        )}
      </Stack>
      <Dialog
        open={Boolean(packagePreview)}
        onClose={() => !busy && setPackagePreview(null)}
      >
        <DialogTitle>
          {t('layout.components.navigation.pluginInstallTitle')}
        </DialogTitle>
        <DialogContent>
          {packagePreview && (
            <Stack spacing={1.5} sx={{ pt: 1 }}>
              <Typography variant="h6">{packagePreview.name}</Typography>
              <Typography color="text.secondary">
                {packagePreview.description} · v{packagePreview.version}
              </Typography>
              <Typography variant="body2">
                {t('layout.components.navigation.pluginSigner', {
                  fingerprint: packagePreview.signer_fingerprint,
                })}
              </Typography>
              <Stack direction="row" spacing={1}>
                {packagePreview.permissions.map((permission) => (
                  <Chip key={permission} size="small" label={permission} />
                ))}
              </Stack>
              <Alert severity="warning">
                {t('layout.components.navigation.pluginTrustWarning')}
              </Alert>
            </Stack>
          )}
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setPackagePreview(null)} disabled={busy}>
            {t('layout.components.navigation.pluginCancel')}
          </Button>
          <Button
            onClick={() => void installPackage()}
            disabled={busy}
            variant="contained"
          >
            {t('layout.components.navigation.pluginTrustInstall')}
          </Button>
        </DialogActions>
      </Dialog>
      <Dialog
        open={bindingDialogOpen}
        onClose={() => !busy && setBindingDialogOpen(false)}
      >
        <DialogTitle>
          {t(
            bindingEditId
              ? 'layout.components.navigation.pluginEditBinding'
              : 'layout.components.navigation.pluginAddBinding',
          )}
        </DialogTitle>
        <DialogContent>
          <Stack spacing={2} sx={{ pt: 1, minWidth: 360 }}>
            {bindingFormError && (
              <Alert severity="error">
                {t('layout.components.navigation.pluginBindingSaveError')}
              </Alert>
            )}
            <FormControl fullWidth>
              <InputLabel>
                {t('layout.components.navigation.pluginSelect')}
              </InputLabel>
              <Select
                label={t('layout.components.navigation.pluginSelect')}
                value={bindingPluginId}
                onChange={(event) => setBindingPluginId(event.target.value)}
              >
                {(plugins ?? []).map((plugin) => (
                  <MenuItem key={plugin.id} value={plugin.id}>
                    {plugin.name}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
            <FormControl fullWidth>
              <InputLabel>
                {t('layout.components.navigation.pluginTarget')}
              </InputLabel>
              <Select
                multiple
                disabled={Boolean(bindingEditId)}
                label={t('layout.components.navigation.pluginTarget')}
                value={bindingTargetKeys}
                onChange={(event) =>
                  setBindingTargetKeys(
                    typeof event.target.value === 'string'
                      ? event.target.value.split(',')
                      : event.target.value,
                  )
                }
              >
                {targetOptions.map((option) => (
                  <MenuItem key={option.key} value={option.key}>
                    {option.label}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
            <TextField
              label={t('layout.components.navigation.pluginPriority')}
              type="number"
              value={bindingPriority}
              onChange={(event) => setBindingPriority(event.target.value)}
            />
            <TextField
              label={t('layout.components.navigation.pluginConfig')}
              value={bindingConfigText}
              onChange={(event) => setBindingConfigText(event.target.value)}
              multiline
              minRows={3}
            />
          </Stack>
        </DialogContent>
        <DialogActions>
          <Button
            onClick={() => {
              setBindingDialogOpen(false)
              setBindingEditId(null)
            }}
            disabled={busy}
          >
            {t('layout.components.navigation.pluginCancel')}
          </Button>
          <Button
            onClick={() => void saveBinding()}
            disabled={busy || !bindingPluginId || !bindingTargetKeys.length}
            variant="contained"
          >
            {t('layout.components.navigation.pluginSaveBinding')}
          </Button>
        </DialogActions>
      </Dialog>
    </BasePage>
  )
}

export default PluginsPage
