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

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ProcessEntry {
  processName: string
  processPath: string
  lastSeen: number
  connectionCount: number
  mapping?: PerAppProxyMapping
  runtimeStatus: 'idle' | 'active' | 'failed' | 'recovery_required' | 'unavailable'
  chainId?: string
  errorDetail?: string
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function deriveProcessList(
  connections: IConnectionsItem[],
  mappings: PerAppProxyMapping[],
  globalStatus: ProcessProxyStatus,
): ProcessEntry[] {
  const map = new Map<string, ProcessEntry>()
  const mappingByKey = new Map<string, PerAppProxyMapping>()
  for (const m of mappings) {
    const key = m.processPath ? `${m.processName}|${m.processPath}` : m.processName
    mappingByKey.set(key, m)
  }

  for (const conn of connections) {
    const processName = conn.metadata.process?.trim()
    if (!processName) continue
    const processPath = conn.metadata.processPath?.trim() || ''
    const key = processPath ? `${processName}|${processPath}` : processName
    const startTime = Date.parse(conn.start) || 0
    const existing = map.get(key)
    const mapping = mappingByKey.get(key)

    if (existing) {
      existing.connectionCount += 1
      if (startTime > existing.lastSeen) existing.lastSeen = startTime
    } else {
      let runtimeStatus: ProcessEntry['runtimeStatus'] = 'idle'
      let errorDetail: string | undefined

      if (globalStatus.status === 'unavailable') {
        runtimeStatus = 'unavailable'
      } else if (globalStatus.status === 'recovery_required') {
        runtimeStatus = 'recovery_required'
        errorDetail = globalStatus.detail
      } else if (globalStatus.status === 'active') {
        // Only the mapping that matches the live session's chain is the one
        // actually being proxied; M5 keeps a single session at a time.
        const matchesLiveChain =
          mapping?.enabled && globalStatus.chain_id === mapping.chainId
        runtimeStatus = matchesLiveChain ? 'active' : 'idle'
      }

      map.set(key, {
        processName,
        processPath,
        lastSeen: startTime,
        connectionCount: 1,
        mapping,
        runtimeStatus,
        chainId: mapping?.chainId,
        errorDetail,
      })
    }
  }

  // Also include mappings that have no recent connections (manually added).
  for (const m of mappings) {
    const key = m.processPath ? `${m.processName}|${m.processPath}` : m.processName
    if (!map.has(key)) {
      const active =
        globalStatus.status === 'active' &&
        m.enabled &&
        globalStatus.chain_id === m.chainId
      map.set(key, {
        processName: m.processName,
        processPath: m.processPath ?? '',
        lastSeen: 0,
        connectionCount: 0,
        mapping: m,
        runtimeStatus: active ? 'active' : 'idle',
        chainId: m.chainId,
      })
    }
  }

  return Array.from(map.values()).sort((a, b) => {
    if (a.mapping?.enabled !== b.mapping?.enabled) {
      return a.mapping?.enabled ? -1 : 1
    }
    return b.lastSeen - a.lastSeen
  })
}

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

const StatusChip = ({ status, errorDetail }: { status: ProcessEntry['runtimeStatus']; errorDetail?: string }) => {
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
  const [proxyStatus, setProxyStatus] = useState<ProcessProxyStatus>({ status: 'idle' })
  const [loading, setLoading] = useState(true)
  const [attaching, setAttaching] = useState<string | null>(null)
  const [confirmTarget, setConfirmTarget] = useState<ProcessEntry | null>(null)

  const isTunActive = verge?.enable_tun_mode && isTunModeAvailable
  const isWindows = getSystem() === 'windows'
  const isServiceAvailable = runState.serviceUsable

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      try {
        const [s, c, status] = await Promise.all([
          clash5GetPerAppProxySettings(),
          getClash5ChainWorkspace(),
          clash5ProcessProxyStatus().catch(() => ({ status: 'unavailable' as const })),
        ])
        if (cancelled) return
        setSettings(s)
        setChains(c.chains.filter((ch) => ch.enabled))
        setProxyStatus(status)
      } catch {
        // Non-critical: the card will show empty state.
      } finally {
        if (!cancelled) setLoading(false)
      }
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
      deriveProcessList(
        [
          ...(response.data?.activeConnections ?? []),
          ...(response.data?.closedConnections ?? []),
        ],
        mappings,
        proxyStatus,
      ),
    [response.data, mappings, proxyStatus],
  )

  const chainNameById = useMemo(() => {
    const map = new Map<string, string>()
    for (const c of chains) map.set(c.id, c.name)
    return map
  }, [chains])

  const hasActiveSession = proxyStatus.status === 'active'

  const mappingKey = (entry: {
    processName: string
    processPath?: string
  }) =>
    entry.processPath
      ? `${entry.processName}|${entry.processPath}`
      : entry.processName

  /** Upsert a mapping for this process and persist it. Returns false on failure. */
  const persistMapping = useCallback(
    async (
      entry: Pick<ProcessEntry, 'processName' | 'processPath' | 'chainId'>,
      patch: Partial<PerAppProxyMapping>,
    ): Promise<boolean> => {
      const key = mappingKey(entry)
      const previous = settings
      const newMappings = [...(previous.mappings ?? [])]
      const idx = newMappings.findIndex((m) => mappingKey(m) === key)
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
    async (entry: ProcessEntry, newChainId: string) => {
      await persistMapping(entry, { chainId: newChainId })
    },
    [persistMapping],
  )

  const handleToggle = useLockFn(async (entry: ProcessEntry) => {
    if (entry.runtimeStatus === 'active') {
      setAttaching(mappingKey(entry))
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
    setAttaching(mappingKey(entry))
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
    try {
      const [s, c, status] = await Promise.all([
        clash5GetPerAppProxySettings(),
        getClash5ChainWorkspace(),
        clash5ProcessProxyStatus().catch(() => ({ status: 'unavailable' as const })),
      ])
      setSettings(s)
      setChains(c.chains.filter((ch) => ch.enabled))
      setProxyStatus(status)
    } catch (err) {
      showNotice.error(err)
    } finally {
      setLoading(false)
    }
  })

  const isDisabled = (entry: ProcessEntry): { disabled: boolean; reason?: string } => {
    if (!isWindows) return { disabled: true, reason: t('home.components.perAppProxy.disabled.nonWindows') }
    if (!isServiceAvailable) return { disabled: true, reason: t('home.components.perAppProxy.disabled.noService') }
    if (isTunActive) return { disabled: true, reason: t('home.components.perAppProxy.disabled.tunActive') }
    if (hasActiveSession && entry.runtimeStatus !== 'active') {
      return { disabled: true, reason: t('home.components.perAppProxy.disabled.singleSession') }
    }
    if (entry.runtimeStatus === 'unavailable') {
      return { disabled: true, reason: t('home.components.perAppProxy.disabled.unavailable') }
    }
    if (attaching !== null) return { disabled: true, reason: t('home.components.perAppProxy.disabled.attaching') }
    return { disabled: false }
  }

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
              const key = entry.processPath
                ? `${entry.processName}|${entry.processPath}`
                : entry.processName
              const { disabled, reason } = isDisabled(entry)
              const isAttaching = attaching === key
              // A stored chain that no longer exists must be surfaced, not
              // silently dropped.
              const chainMissing =
                entry.chainId !== undefined &&
                entry.chainId !== '' &&
                !chainNameById.has(entry.chainId)

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
