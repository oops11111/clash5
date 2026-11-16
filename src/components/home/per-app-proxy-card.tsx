import {
  AppsOutlined,
  BlockRounded,
  CheckCircleOutlineRounded,
  ErrorOutlineRounded,
  HelpOutlineRounded,
  RefreshRounded,
  WarningAmberRounded,
} from '@mui/icons-material'
import {
  Box,
  Button,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  FormControl,
  IconButton,
  InputLabel,
  MenuItem,
  Select,
  Skeleton,
  Tooltip,
  Typography,
  alpha,
  useTheme,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { EnhancedCard } from '@/components/home/enhanced-card'
import {
  derivePerAppEntries,
  entryKey,
  gateEntry,
  isChainMissing,
  type PerAppEntry,
  type PerAppGateReason,
} from '@/components/home/per-app-proxy-model'
import { useConnectionData } from '@/hooks/use-connection-data'
import { useSystemState } from '@/hooks/use-system-state'
import { useVerge } from '@/hooks/use-verge'
import {
  clash5AttachProcessProxy,
  clash5DetachProcessProxy,
  clash5GetPerAppProxySettings,
  clash5ListProcessCandidates,
  clash5ProcessProxyStatus,
  clash5SetPerAppProxySettings,
  getClash5ChainWorkspace,
} from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import type {
  PerAppProxyMapping,
  PerAppProxySettings,
  ProcessProxyStatus,
  ProxyChain,
} from '@/services/tauri-commands.generated'
import getSystem from '@/utils/get-system'

function formatRelativeTime(timestamp: number): string {
  if (!timestamp) return ''
  const diff = Date.now() - timestamp
  if (diff < 60_000) return 'just now'
  const minutes = Math.floor(diff / 60_000)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  return `${Math.floor(hours / 24)}d ago`
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

const StatusChip = ({ status, errorDetail }: { status: PerAppEntry['runtimeStatus']; errorDetail?: string }) => {
  const { t } = useTranslation()
  switch (status) {
    case 'active':
      return <Chip size="small" icon={<CheckCircleOutlineRounded />} label={t('home.components.perAppProxy.status.active')} color="success" variant="outlined" />
    case 'failed':
      return (
        <Tooltip title={errorDetail || t('home.components.perAppProxy.status.failed')}>
          <Chip size="small" icon={<ErrorOutlineRounded />} label={t('home.components.perAppProxy.status.failed')} color="error" variant="outlined" />
        </Tooltip>
      )
    case 'recovery_required':
      return (
        <Tooltip title={errorDetail || t('home.components.perAppProxy.status.recoveryRequired')}>
          <Chip size="small" icon={<WarningAmberRounded />} label={t('home.components.perAppProxy.status.recoveryRequired')} color="warning" variant="outlined" />
        </Tooltip>
      )
    case 'unavailable':
      return <Chip size="small" icon={<BlockRounded />} label={t('home.components.perAppProxy.status.unavailable')} color="default" variant="outlined" />
    default:
      return <Chip size="small" label={t('home.components.perAppProxy.status.idle')} variant="outlined" />
  }
}

// ---------------------------------------------------------------------------
// Confirmation dialog
// ---------------------------------------------------------------------------

interface AttachConfirmProps {
  open: boolean
  processName: string
  chainName: string
  onConfirm: () => void
  onCancel: () => void
}

const AttachConfirmDialog = ({ open, processName, chainName, onConfirm, onCancel }: AttachConfirmProps) => {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onClose={onCancel} maxWidth="xs" fullWidth>
      <DialogTitle>{t('home.components.perAppProxy.confirm.title')}</DialogTitle>
      <DialogContent>
        <DialogContentText>
          {t('home.components.perAppProxy.confirm.body', { processName, chainName })}
        </DialogContentText>
        <DialogContentText sx={{ mt: 1, fontWeight: 'bold' }}>
          {t('home.components.perAppProxy.confirm.warning')}
        </DialogContentText>
      </DialogContent>
      <DialogActions>
        <Button onClick={onCancel}>{t('shared.actions.cancel')}</Button>
        <Button onClick={onConfirm} color="primary" variant="contained">
          {t('shared.actions.confirm')}
        </Button>
      </DialogActions>
    </Dialog>
  )
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export const PerAppProxyCard = () => {
  const { t } = useTranslation()
  const theme = useTheme()
  const { response } = useConnectionData()
  const { runState, isTunModeAvailable } = useSystemState()
  const { verge } = useVerge()

  const [settings, setSettings] = useState<PerAppProxySettings>({ mappings: [] })
  const [chains, setChains] = useState<ProxyChain[]>([])
  // Distinguishes "chain was deleted" from "we could not read the chain list".
  const [chainsLoaded, setChainsLoaded] = useState(false)
  const [proxyStatus, setProxyStatus] = useState<ProcessProxyStatus>({ status: 'idle' })
  const [loading, setLoading] = useState(true)
  const [attaching, setAttaching] = useState<string | null>(null)
  const [confirmTarget, setConfirmTarget] = useState<PerAppEntry | null>(null)

  const isTunActive = verge?.enable_tun_mode === true && isTunModeAvailable
  const isWindows = getSystem() === 'windows'
  const isServiceAvailable = runState.serviceUsable

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      // Each source is independent: a dead core must not hide the stored
      // mappings, and a missing chain list must not hide the process rows.
      const [settingsResult, chainsResult, statusResult] =
        await Promise.allSettled([
          clash5GetPerAppProxySettings(),
          getClash5ChainWorkspace(),
          clash5ProcessProxyStatus(),
        ])
      if (cancelled) return

      if (settingsResult.status === 'fulfilled') {
        setSettings(settingsResult.value)
      }
      if (chainsResult.status === 'fulfilled') {
        setChains(chainsResult.value.chains.filter((ch) => ch.enabled))
        setChainsLoaded(true)
      }
      setProxyStatus(
        statusResult.status === 'fulfilled'
          ? statusResult.value
          : { status: 'unavailable' },
      )
      setLoading(false)
    }
    void load()
    return () => { cancelled = true }
  }, [])

  useEffect(() => {
    const timer = setInterval(() => {
      clash5ProcessProxyStatus()
        .then(setProxyStatus)
        .catch(() => setProxyStatus({ status: 'unavailable' }))
    }, 5000)
    return () => clearInterval(timer)
  }, [])

  // `mappings` is optional on the wire; treat a missing list as empty.
  const mappings = useMemo(() => settings.mappings ?? [], [settings])

  const processList = useMemo(
    () =>
      derivePerAppEntries({
        connections: [
          ...(response.data?.activeConnections ?? []),
          ...(response.data?.closedConnections ?? []),
        ],
        mappings,
        status: proxyStatus,
      }),
    [response.data, mappings, proxyStatus],
  )

  const chainNameById = useMemo(() => {
    const map = new Map<string, string>()
    for (const c of chains) map.set(c.id, c.name)
    return map
  }, [chains])

  const knownChainIds = useMemo(
    () => new Set(chainNameById.keys()),
    [chainNameById],
  )

  const hasActiveSession = proxyStatus.status === 'active'

  /** Upsert a mapping for this process and persist it. Returns false on failure. */
  const persistMapping = useCallback(
    async (
      entry: Pick<PerAppEntry, 'processName' | 'processPath' | 'chainId'>,
      patch: Partial<PerAppProxyMapping>,
    ): Promise<boolean> => {
      const key = entryKey(entry)
      const previous = settings
      const newMappings = [...(previous.mappings ?? [])]
      const idx = newMappings.findIndex((m) => entryKey(m) === key)
      if (idx >= 0) {
        newMappings[idx] = { ...newMappings[idx], ...patch }
      } else {
        newMappings.push({
          processName: entry.processName,
          processPath: entry.processPath,
          chainId: patch.chainId ?? entry.chainId ?? '',
          enabled: patch.enabled ?? false,
          order: newMappings.length,
        })
      }
      const newSettings = { mappings: newMappings }
      setSettings(newSettings)
      try {
        await clash5SetPerAppProxySettings(newSettings)
        return true
      } catch (err) {
        showNotice.error(err)
        setSettings(previous)
        return false
      }
    },
    [settings],
  )

  const handleChainChange = useCallback(
    async (entry: PerAppEntry, newChainId: string) => {
      await persistMapping(entry, { chainId: newChainId })
    },
    [persistMapping],
  )

  const handleToggle = useLockFn(async (entry: PerAppEntry) => {
    if (entry.runtimeStatus === 'active') {
      setAttaching(entryKey(entry))
      try {
        const status = await clash5DetachProcessProxy()
        setProxyStatus(status)
        await persistMapping(entry, { enabled: false })
        showNotice.success(t('home.components.perAppProxy.feedback.detached'))
      } catch (err) {
        showNotice.error(err)
      } finally {
        setAttaching(null)
      }
      return
    }

    setConfirmTarget(entry)
  })

  const handleConfirmAttach = useLockFn(async () => {
    const entry = confirmTarget
    if (!entry) return
    setConfirmTarget(null)

    const chainId = entry.chainId
    if (!chainId) {
      showNotice.error(t('home.components.perAppProxy.feedback.noChainSelected'))
      return
    }

    // Only allow attaching to a process that is actually running: the live
    // enumeration is the authority on PID, since connection metadata carries
    // no PID and PIDs get reused.
    setAttaching(entryKey(entry))
    try {
      const candidates = await clash5ListProcessCandidates()
      const sameName = candidates.filter((c) => c.processName === entry.processName)
      const exactPath = entry.processPath
        ? sameName.filter(
            (c) => c.processPath.toLowerCase() === entry.processPath.toLowerCase(),
          )
        : []
      const pool = exactPath.length > 0 ? exactPath : sameName

      if (pool.length === 0) {
        showNotice.error(t('home.components.perAppProxy.feedback.processNotRunning'))
        return
      }

      // Multi-instance apps (browsers, Electron) all share one image path, so
      // pick the earliest-created instance: that is the parent of the process
      // tree the redirect follows.
      const candidate = pool.reduce((earliest, next) =>
        next.createdAt < earliest.createdAt ? next : earliest,
      )

      const status = await clash5AttachProcessProxy(chainId, candidate.pid)
      setProxyStatus(status)
      await persistMapping(entry, { enabled: true, chainId })
      showNotice.success(t('home.components.perAppProxy.feedback.attached'))
    } catch (err) {
      showNotice.error(err)
      clash5ProcessProxyStatus().then(setProxyStatus).catch(() => {})
    } finally {
      setAttaching(null)
    }
  })

  const handleRefresh = useLockFn(async () => {
    setLoading(true)
    const [settingsResult, chainsResult, statusResult] =
      await Promise.allSettled([
        clash5GetPerAppProxySettings(),
        getClash5ChainWorkspace(),
        clash5ProcessProxyStatus(),
      ])

    if (settingsResult.status === 'fulfilled') {
      setSettings(settingsResult.value)
    } else {
      showNotice.error(settingsResult.reason)
    }
    if (chainsResult.status === 'fulfilled') {
      setChains(chainsResult.value.chains.filter((ch) => ch.enabled))
      setChainsLoaded(true)
    }
    setProxyStatus(
      statusResult.status === 'fulfilled'
        ? statusResult.value
        : { status: 'unavailable' },
    )
    setLoading(false)
  })

  const gateReasonKey: Record<PerAppGateReason, string> = {
    nonWindows: 'home.components.perAppProxy.disabled.nonWindows',
    noService: 'home.components.perAppProxy.disabled.noService',
    tunActive: 'home.components.perAppProxy.disabled.tunActive',
    singleSession: 'home.components.perAppProxy.disabled.singleSession',
    unavailable: 'home.components.perAppProxy.disabled.unavailable',
    busy: 'home.components.perAppProxy.disabled.attaching',
  }

  const gateFor = (entry: PerAppEntry): PerAppGateReason | null =>
    gateEntry({
      entry,
      platform: isWindows ? 'windows' : getSystem(),
      serviceUsable: isServiceAvailable,
      tunActive: isTunActive,
      activeSession: hasActiveSession,
      busy: attaching !== null,
    })

  if (loading) {
    return (
      <EnhancedCard
        title={t('home.page.cards.perAppProxy')}
        icon={<AppsOutlined />}
        iconColor="primary"
      >
        <Skeleton variant="rectangular" height={200} />
      </EnhancedCard>
    )
  }

  return (
    <>
      <EnhancedCard
        title={t('home.page.cards.perAppProxy')}
        icon={<AppsOutlined />}
        iconColor="primary"
        action={
          <Tooltip title={t('home.components.perAppProxy.actions.refresh')}>
            <IconButton size="small" onClick={handleRefresh}>
              <RefreshRounded />
            </IconButton>
          </Tooltip>
        }
      >
        {!isWindows && (
          <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
            {t('home.components.perAppProxy.disabled.nonWindows')}
          </Typography>
        )}

        {processList.length === 0 ? (
          <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'center', minHeight: 120 }}>
            <Typography variant="body2" color="text.secondary">
              {t('home.components.perAppProxy.emptyState')}
            </Typography>
          </Box>
        ) : (
          <Box
            sx={{
              display: 'flex',
              flexDirection: 'column',
              gap: 0.5,
              maxHeight: 360,
              overflowY: 'auto',
            }}
          >
            {processList.map((entry) => {
              const key = entryKey(entry)
              const gate = gateFor(entry)
              const disabled = gate !== null
              const reason = gate ? t(gateReasonKey[gate]) : undefined
              const isAttaching = attaching === key
              const chainMissing = isChainMissing(
                entry,
                knownChainIds,
                chainsLoaded,
              )

              return (
                <Box
                  key={key}
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1,
                    py: 1,
                    px: 1,
                    borderRadius: 1,
                    opacity: disabled ? 0.6 : 1,
                    '&:hover': {
                      backgroundColor: alpha(theme.palette.primary.main, 0.04),
                    },
                  }}
                >
                  <Box sx={{ minWidth: 0, flex: 1, overflow: 'hidden' }}>
                    <Typography
                      variant="body2"
                      sx={{
                        fontWeight: 500,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {entry.processName}
                    </Typography>
                    {entry.processPath && (
                      <Typography
                        variant="caption"
                        sx={{
                          display: 'block',
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                          color: 'text.disabled',
                        }}
                      >
                        {entry.processPath}
                      </Typography>
                    )}
                    {entry.lastSeen > 0 && (
                      <Typography variant="caption" color="text.disabled">
                        {entry.connectionCount} conn &middot; {formatRelativeTime(entry.lastSeen)}
                      </Typography>
                    )}
                  </Box>

                  <StatusChip status={entry.runtimeStatus} errorDetail={entry.errorDetail} />

                  <FormControl size="small" sx={{ minWidth: 120, maxWidth: 180 }}>
                    <InputLabel>{t('home.components.perAppProxy.chainLabel')}</InputLabel>
                    <Select
                      value={entry.chainId || ''}
                      label={t('home.components.perAppProxy.chainLabel')}
                      disabled={disabled || entry.runtimeStatus === 'active'}
                      onChange={(e) => handleChainChange(entry, e.target.value)}
                      size="small"
                    >
                      <MenuItem value="">
                        <em>{t('home.components.perAppProxy.noChain')}</em>
                      </MenuItem>
                      {chains.map((c) => (
                        <MenuItem key={c.id} value={c.id}>
                          {c.name}
                        </MenuItem>
                      ))}
                    </Select>
                  </FormControl>

                  {chainMissing && (
                    <Tooltip title={t('home.components.perAppProxy.chainInvalid')}>
                      <WarningAmberRounded sx={{ fontSize: 18, color: 'warning.main' }} />
                    </Tooltip>
                  )}

                  <Tooltip title={reason || (entry.runtimeStatus === 'active' ? t('home.components.perAppProxy.actions.detach') : t('home.components.perAppProxy.actions.attach'))}>
                    <span>
                      <Button
                        size="small"
                        variant={entry.runtimeStatus === 'active' ? 'contained' : 'outlined'}
                        color={entry.runtimeStatus === 'active' ? 'error' : 'primary'}
                        disabled={disabled}
                        onClick={() => handleToggle(entry)}
                        sx={{ minWidth: 72 }}
                      >
                        {isAttaching ? (
                          <CircularProgress size={16} />
                        ) : entry.runtimeStatus === 'active' ? (
                          t('home.components.perAppProxy.actions.detach')
                        ) : (
                          t('home.components.perAppProxy.actions.attach')
                        )}
                      </Button>
                    </span>
                  </Tooltip>

                  {reason && (
                    <Tooltip title={reason}>
                      <HelpOutlineRounded sx={{ fontSize: 16, color: 'text.disabled' }} />
                    </Tooltip>
                  )}
                </Box>
              )
            })}
          </Box>
        )}
      </EnhancedCard>

      {confirmTarget && (
        <AttachConfirmDialog
          open
          processName={confirmTarget.processName}
          chainName={confirmTarget.chainId ? chainNameById.get(confirmTarget.chainId) ?? '' : ''}
          onConfirm={handleConfirmAttach}
          onCancel={() => setConfirmTarget(null)}
        />
      )}
    </>
  )
}
