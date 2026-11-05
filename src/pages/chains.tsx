// Hop, pool member and sample rows have no stable id and are controlled inputs,
// so their index is a safe key.
/* eslint-disable @eslint-react/no-array-index-key */
import ArrowForwardIcon from '@mui/icons-material/ArrowForward'
import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  FormControlLabel,
  MenuItem,
  Paper,
  Stack,
  Switch,
  Tab,
  Tabs,
  TextField,
  Typography,
} from '@mui/material'
import { Fragment, useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { BasePage, ConfirmDialog } from '@/components/base'
import { PoolPolicy } from '@/components/chain/pool-policy'
import { PoolScoreList } from '@/components/chain/pool-scores'
import {
  activateClash5Chain,
  getClash5ChainSamples,
  deleteClash5Chain,
  deleteClash5NodePool,
  detectClash5ExitIp,
  getClash5ChainWorkspace,
  getClash5PoolSwitchEvents,
  runClash5QualityTest,
  saveClash5Chain,
  saveClash5NodePool,
  testClash5Chain,
} from '@/services/cmds'
import { appStorage } from '@/services/storage'
import type { SwitchEvent } from '@/services/tauri-commands.generated'
import type {
  ChainHopRef,
  ChainSample,
  ChainNodeRef,
  ChainQualityReport,
  ChainWorkspace,
  Clash5Chain,
  ExitIpResult,
} from '@/types/clash5-chain'

// Matches the backend default for the long-lived connection quality test.
const QUALITY_DEFAULT_BYTES = 5_000_000
const formatMegabytesPerSecond = (bps: number) =>
  `${(bps / 1_000_000).toFixed(2)} MB/s`
const formatMegabytes = (bytes: number) =>
  `${(bytes / 1_000_000).toFixed(1)} MB`
const nodeKey = (reference: ChainNodeRef) => JSON.stringify(reference)
const sameNode = (left: ChainNodeRef, right: ChainNodeRef) =>
  left.stable_id === right.stable_id &&
  left.source.kind === right.source.kind &&
  (left.source.kind !== 'subscription' ||
    (right.source.kind === 'subscription' &&
      left.source.provider_id === right.source.provider_id))
const emptyNode: ChainNodeRef = { source: { kind: 'manual' }, stable_id: '' }
const MAX_HOPS = 6
const MAX_POOL_MEMBERS = 32
const sameHop = (left: ChainHopRef, right: ChainHopRef) =>
  left.kind === 'node'
    ? right.kind === 'node' && sameNode(left.node, right.node)
    : right.kind === 'pool' && left.pool_id === right.pool_id
const hopValue = (hop: ChainHopRef) =>
  hop.kind === 'node' ? `node:${nodeKey(hop.node)}` : `pool:${hop.pool_id}`
const newChain = (workspace: ChainWorkspace): Clash5Chain => ({
  schema_version: 2,
  id: crypto.randomUUID(),
  name: '',
  profile_id: workspace.profile_id,
  hops: [
    { kind: 'node', node: workspace.nodes[0]?.reference ?? emptyNode },
    { kind: 'node', node: workspace.nodes[1]?.reference ?? emptyNode },
  ],
  enabled: true,
  exit_policy: { mode: 'warn' },
  pinned_exit_ip: null,
})

const failureCode = (error: unknown) => {
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    typeof error.code === 'string' &&
    /^C5-[A-Z]+-\d+$/.test(error.code)
  )
    return error.code
  return 'C5-CHAIN-000'
}

const ChainsPage = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [workspace, setWorkspace] = useState<ChainWorkspace | null>(null)
  const [draft, setDraft] = useState<Clash5Chain | null>(null)
  const [tab, setTab] = useState('chains')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const [severity, setSeverity] = useState<'success' | 'warning' | 'error'>(
    'success',
  )
  const [policyGroup, setPolicyGroup] = useState('')
  const [exitResult, setExitResult] = useState<ExitIpResult | null>(null)
  const [quality, setQuality] = useState<ChainQualityReport | null>(null)
  const [qualityConfirmOpen, setQualityConfirmOpen] = useState(false)
  const [samples, setSamples] = useState<ChainSample[]>([])
  const [poolName, setPoolName] = useState('')
  const [switchEvents, setSwitchEvents] = useState<SwitchEvent[]>([])
  const [poolMemberKeys, setPoolMemberKeys] = useState<string[]>([])
  const [exitEnabled, setExitEnabled] = useState(
    () => appStorage.getItem('clash5-exit-ip-enabled') !== 'false',
  )
  const [serviceUrl, setServiceUrl] = useState(
    () => appStorage.getItem('clash5-exit-ip-url') ?? 'https://api64.ipify.org',
  )

  const load = useCallback(async (selectedId?: string) => {
    const next = await getClash5ChainWorkspace()
    setWorkspace(next)
    const selected =
      next.chains.find((chain) => chain.id === selectedId) ?? next.chains[0]
    setDraft(selected ? structuredClone(selected) : newChain(next))
    setSamples([])
    if (selected) {
      void getClash5ChainSamples(selected.id, 10)
        .then(setSamples)
        .catch(() => undefined)
    }
    setPolicyGroup((current) =>
      next.policy_groups.includes(current)
        ? current
        : (next.policy_groups[0] ?? ''),
    )
    setExitResult(null)
    setQuality(null)
  }, [])

  // Pool edits change only the pool list, so an unsaved chain draft is kept.
  const refreshWorkspace = useCallback(async () => {
    setWorkspace(await getClash5ChainWorkspace())
  }, [])

  useEffect(() => {
    getClash5PoolSwitchEvents()
      .then(setSwitchEvents)
      .catch(() => setSwitchEvents([]))
  }, [workspace])

  const refreshSamples = useCallback(async (chainId: string) => {
    try {
      setSamples(await getClash5ChainSamples(chainId, 10))
    } catch {
      setSamples([])
    }
  }, [])

  useEffect(() => {
    void load().catch((error: unknown) => {
      setSeverity('error')
      setMessage(t('chains.failed', { code: failureCode(error) }))
    })
  }, [load, t])

  const saved = workspace?.chains.find((chain) => chain.id === draft?.id)
  const clean = saved && JSON.stringify(saved) === JSON.stringify(draft)
  const canTest = Boolean(clean && saved?.enabled && !busy)
  const hopInfo = (hop: ChainHopRef) => {
    if (hop.kind === 'pool') {
      const pool = workspace?.pools.find((entry) => entry.id === hop.pool_id)
      return {
        title: pool?.name ?? t('chains.missingPool'),
        detail: pool
          ? t('chains.poolMembersCount', { count: pool.members.length })
          : '—',
        ok: Boolean(pool),
      }
    }
    const node = workspace?.nodes.find((entry) =>
      sameNode(entry.reference, hop.node),
    )
    return {
      title: node?.display_name ?? t('chains.missingNode'),
      detail: node
        ? `${node.kind.toUpperCase()} · ${node.server}:${node.port}`
        : '—',
      ok: Boolean(node),
    }
  }
  const hopsValid = Boolean(
    draft &&
      draft.hops.length >= 2 &&
      draft.hops.length <= MAX_HOPS &&
      draft.hops.every((hop) => hopInfo(hop).ok) &&
      draft.hops
        .slice(1)
        .every((hop, index) => !sameHop(hop, draft.hops[index])),
  )

  const perform = async (action: () => Promise<void>) => {
    setBusy(true)
    setMessage('')
    try {
      await action()
    } catch (error) {
      setSeverity('error')
      setMessage(t('chains.failed', { code: failureCode(error) }))
    } finally {
      setBusy(false)
    }
  }

  const report = (text: string, level: 'success' | 'warning' = 'success') => {
    setMessage(text)
    setSeverity(level)
  }

  const runQualityTest = () => {
    setQualityConfirmOpen(false)
    void perform(async () => {
      if (!draft) return
      const result = await runClash5QualityTest(draft.id)
      setQuality(result)
      void refreshSamples(draft.id)
      if (result.status !== 'healthy')
        report(t(`chains.quality.${result.status}`), 'warning')
    })
  }

  const choose = (chain: Clash5Chain) => {
    setDraft(structuredClone(chain))
    setExitResult(null)
    setQuality(null)
    setMessage('')
    setTab('chains')
  }

  const updateHop = (index: number, hop: ChainHopRef) => {
    if (!draft) return
    setDraft({
      ...draft,
      hops: draft.hops.map((entry, position) =>
        position === index ? hop : entry,
      ),
    })
  }

  const moveHop = (index: number, delta: number) => {
    if (!draft) return
    const target = index + delta
    if (target < 0 || target >= draft.hops.length) return
    const hops = [...draft.hops]
    ;[hops[index], hops[target]] = [hops[target], hops[index]]
    setDraft({ ...draft, hops })
  }

  const removeHop = (index: number) => {
    if (!draft) return
    setDraft({
      ...draft,
      hops: draft.hops.filter((_, position) => position !== index),
    })
  }

  const addHop = () => {
    if (!draft || !workspace) return
    setDraft({
      ...draft,
      hops: [
        ...draft.hops,
        { kind: 'node', node: workspace.nodes[0]?.reference ?? emptyNode },
      ],
    })
  }

  const renderHop = (index: number, hop: ChainHopRef) => {
    const info = hopInfo(hop)
    const label = t('chains.hop', { index: index + 1 })
    return (
      <Paper
        variant="outlined"
        sx={{
          p: 2,
          borderRadius: 3,
          flex: 1,
          minWidth: 0,
          width: { xs: '100%', lg: 'auto' },
          bgcolor: 'action.hover',
        }}
      >
        <Stack spacing={1}>
          <Stack
            direction="row"
            sx={{ justifyContent: 'space-between', alignItems: 'center' }}
          >
            <Typography color="text.secondary" variant="caption">
              {label}
            </Typography>
            <Stack direction="row">
              <Button
                size="small"
                disabled={busy || index === 0}
                onClick={() => moveHop(index, -1)}
              >
                {t('chains.moveUp')}
              </Button>
              <Button
                size="small"
                disabled={busy || !draft || index === draft.hops.length - 1}
                onClick={() => moveHop(index, 1)}
              >
                {t('chains.moveDown')}
              </Button>
              <Button
                size="small"
                color="error"
                disabled={busy || !draft || draft.hops.length <= 2}
                onClick={() => removeHop(index)}
              >
                {t('chains.removeHop')}
              </Button>
            </Stack>
          </Stack>
          <TextField
            select
            fullWidth
            size="small"
            label={label}
            value={hopValue(hop)}
            disabled={busy}
            onChange={(event) => {
              const value = event.target.value
              if (value.startsWith('pool:')) {
                updateHop(index, { kind: 'pool', pool_id: value.slice(5) })
                return
              }
              const selected = workspace?.nodes.find(
                (entry) => `node:${nodeKey(entry.reference)}` === value,
              )
              if (selected)
                updateHop(index, { kind: 'node', node: selected.reference })
            }}
          >
            {workspace?.nodes.map((entry) => (
              <MenuItem
                key={`node:${nodeKey(entry.reference)}`}
                value={`node:${nodeKey(entry.reference)}`}
              >
                {entry.display_name} · {entry.kind} ·{' '}
                {entry.source_name ?? t('chains.manual')}
              </MenuItem>
            ))}
            {workspace?.pools.map((pool) => (
              <MenuItem key={`pool:${pool.id}`} value={`pool:${pool.id}`}>
                {t('chains.poolOption', {
                  name: pool.name,
                  count: pool.members.length,
                })}
              </MenuItem>
            ))}
          </TextField>
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ display: 'block', overflowWrap: 'anywhere' }}
          >
            {info.detail}
          </Typography>
        </Stack>
      </Paper>
    )
  }

  return (
    <BasePage title={t('layout.components.navigation.tabs.chains')}>
      <Stack spacing={2} data-testid="chains-workspace">
        <Stack
          direction="row"
          sx={{
            gap: 1,
            flexWrap: 'wrap',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <Tabs
            value={tab}
            onChange={(_, value: string) => setTab(value)}
            variant="scrollable"
          >
            <Tab value="chains" label={t('chains.chainsTab')} />
            <Tab value="pools" label={t('chains.pools')} />
            <Tab value="apps" label={t('chains.apps')} />
          </Tabs>
          <Stack direction="row" sx={{ gap: 1, flexWrap: 'wrap' }}>
            <Button
              size="small"
              disabled={!canTest || !exitEnabled}
              onClick={() =>
                void perform(async () => {
                  if (!draft) return
                  const result = await detectClash5ExitIp(
                    draft.id,
                    exitEnabled ? serviceUrl : null,
                  )
                  setExitResult(result)
                  void refreshSamples(draft.id)
                  if (result.status === 'warning')
                    report(t('chains.exitWarning'), 'warning')
                  if (result.status === 'drift')
                    report(
                      t('chains.exitDrift', {
                        pinned: result.pinned_ip,
                        observed: result.observation.ip,
                      }),
                      'warning',
                    )
                  if (result.status === 'switched')
                    report(
                      t('chains.exitSwitched'),
                      result.selection_persisted ? 'success' : 'warning',
                    )
                })
              }
            >
              {t('chains.detectExit')}
            </Button>
            <Button
              size="small"
              disabled={!canTest}
              title={t('chains.quality.note')}
              onClick={() => setQualityConfirmOpen(true)}
            >
              {t('chains.quality.run')}
            </Button>
            <Button
              size="small"
              disabled={!canTest}
              onClick={() =>
                void perform(async () => {
                  if (!draft) return
                  const result = await testClash5Chain(draft.id)
                  void refreshSamples(draft.id)
                  report(t('chains.testSuccess', { delay: result.delay_ms }))
                })
              }
            >
              {t('chains.test')}
            </Button>
            <Button
              size="small"
              variant="contained"
              disabled={busy || !draft || !draft.name.trim() || !hopsValid}
              onClick={() =>
                void perform(async () => {
                  if (!draft) return
                  await saveClash5Chain(draft)
                  await load(draft.id)
                  report(t('chains.saved'))
                })
              }
            >
              {t('chains.save')}
            </Button>
          </Stack>
        </Stack>
        {message && <Alert severity={severity}>{message}</Alert>}
        {busy && <CircularProgress size={20} />}
        {!workspace && !message && <CircularProgress size={24} />}
        {workspace && tab === 'chains' && (
          <Stack spacing={1}>
            <Button
              variant="outlined"
              disabled={busy}
              onClick={() => {
                setDraft(newChain(workspace))
                setTab('chains')
                setExitResult(null)
              }}
            >
              {t('chains.create')}
            </Button>
            {workspace.chains.length === 0 && (
              <Alert severity="info">{t('chains.empty')}</Alert>
            )}
            {workspace.chains.map((chain) => (
              <Paper
                key={chain.id}
                variant="outlined"
                sx={{ p: 2, borderRadius: 3 }}
              >
                <Stack
                  direction="row"
                  sx={{ gap: 1, alignItems: 'center', flexWrap: 'wrap' }}
                >
                  <Button onClick={() => choose(chain)} disabled={busy}>
                    {chain.name}
                  </Button>
                  <Chip
                    size="small"
                    label={t(
                      chain.enabled ? 'chains.enabled' : 'chains.disabled',
                    )}
                  />
                  <Button
                    color="error"
                    disabled={busy}
                    onClick={() =>
                      void perform(async () => {
                        await deleteClash5Chain(chain.id)
                        await load()
                        report(t('chains.deleted'))
                      })
                    }
                  >
                    {t('chains.delete')}
                  </Button>
                </Stack>
              </Paper>
            ))}
          </Stack>
        )}
        {workspace && draft && tab === 'chains' && (
          <Box
            sx={{
              display: 'grid',
              gridTemplateColumns: { xs: '1fr', xl: 'minmax(0, 1fr) 280px' },
              gap: 2,
            }}
          >
            <Stack spacing={2}>
              <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
                <Stack direction="row" sx={{ gap: 1, alignItems: 'center' }}>
                  <TextField
                    label={t('chains.name')}
                    value={draft.name}
                    size="small"
                    sx={{ flex: 1, minWidth: 0 }}
                    disabled={busy}
                    onChange={(event) =>
                      setDraft({ ...draft, name: event.target.value })
                    }
                  />
                  <Chip
                    size="small"
                    color={draft.enabled ? 'success' : 'default'}
                    label={t(
                      draft.enabled ? 'chains.enabled' : 'chains.disabled',
                    )}
                  />
                </Stack>
                <Stack
                  direction={{ xs: 'column', lg: 'row' }}
                  sx={{ py: 3, gap: 2, alignItems: 'center', minWidth: 0 }}
                >
                  {draft.hops.map((hop, index) => (
                    <Fragment key={index}>
                      {index > 0 && (
                        <ArrowForwardIcon
                          color="primary"
                          sx={{
                            transform: { xs: 'rotate(90deg)', lg: 'none' },
                          }}
                        />
                      )}
                      {renderHop(index, hop)}
                    </Fragment>
                  ))}
                </Stack>
                <Stack direction="row" sx={{ gap: 1, flexWrap: 'wrap' }}>
                  <Button
                    size="small"
                    disabled={busy || draft.hops.length >= MAX_HOPS}
                    onClick={addHop}
                  >
                    {t('chains.addHop')}
                  </Button>
                  <Button size="small" onClick={() => navigate('/nodes')}>
                    {t('chains.manageNodes')}
                  </Button>
                </Stack>
                <Typography color="text.secondary" variant="body2">
                  {t('chains.hopsHint')}
                </Typography>
                {draft.hops
                  .slice(1)
                  .some((hop, index) => sameHop(hop, draft.hops[index])) && (
                  <Alert severity="warning" sx={{ mt: 1 }}>
                    {t('chains.adjacentHops')}
                  </Alert>
                )}
                {workspace.nodes.length < 2 && (
                  <Alert severity="info" sx={{ mt: 2 }}>
                    {t('chains.needNodes')}
                  </Alert>
                )}
              </Paper>
              <Box
                sx={{
                  display: 'grid',
                  gridTemplateColumns: {
                    xs: '1fr',
                    lg: 'repeat(3, minmax(0, 1fr))',
                  },
                  gap: 2,
                }}
              >
                <Paper
                  variant="outlined"
                  sx={{ p: 2, borderRadius: 3, overflowWrap: 'anywhere' }}
                >
                  <Typography variant="subtitle2">
                    {t('chains.effectiveChain')}
                  </Typography>
                  <Typography variant="caption" color="text.secondary">
                    {t('chains.routeName')}
                  </Typography>
                  <Typography variant="body2" sx={{ my: 1 }}>
                    clash5-chain-{draft.id}
                  </Typography>
                  <Typography variant="body2">
                    {t('chains.pluginPending')}
                  </Typography>
                </Paper>
                {draft.hops.map((hop, index) => {
                  const info = hopInfo(hop)
                  return (
                    <Paper
                      key={index}
                      variant="outlined"
                      sx={{ p: 2, borderRadius: 3, overflowWrap: 'anywhere' }}
                    >
                      <Typography variant="subtitle2">
                        {t('chains.hop', { index: index + 1 })}
                      </Typography>
                      <Typography variant="body2" sx={{ my: 1 }}>
                        {info.title}
                      </Typography>
                      <Typography variant="caption" color="text.secondary">
                        {info.detail}
                      </Typography>
                    </Paper>
                  )
                })}
              </Box>
            </Stack>
            <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
              <Stack spacing={2}>
                <Typography variant="subtitle1">
                  {t('chains.inspector')}
                </Typography>
                <FormControlLabel
                  label={t('chains.enabled')}
                  control={
                    <Switch
                      checked={draft.enabled}
                      disabled={busy}
                      onChange={(_, enabled) => setDraft({ ...draft, enabled })}
                    />
                  }
                />
                <TextField
                  select
                  size="small"
                  label={t('chains.failurePolicy')}
                  value={draft.exit_policy.mode}
                  disabled={busy}
                  onChange={(event) =>
                    setDraft({
                      ...draft,
                      exit_policy:
                        event.target.value === 'warn'
                          ? { mode: 'warn' }
                          : { mode: 'switch', fallback_chain_id: '' },
                    })
                  }
                >
                  <MenuItem value="warn">{t('chains.warnOnly')}</MenuItem>
                  <MenuItem value="switch">
                    {t('chains.switchVerified')}
                  </MenuItem>
                </TextField>
                {draft.exit_policy.mode === 'switch' && (
                  <TextField
                    select
                    size="small"
                    label={t('chains.fallback')}
                    value={draft.exit_policy.fallback_chain_id}
                    disabled={busy}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        exit_policy: {
                          mode: 'switch',
                          fallback_chain_id: event.target.value,
                        },
                      })
                    }
                  >
                    {workspace.chains
                      .filter((chain) => chain.id !== draft.id && chain.enabled)
                      .map((chain) => (
                        <MenuItem key={chain.id} value={chain.id}>
                          {chain.name}
                        </MenuItem>
                      ))}
                  </TextField>
                )}
                <TextField
                  select
                  size="small"
                  label={t('chains.policyGroup')}
                  value={policyGroup}
                  disabled={busy}
                  onChange={(event) => setPolicyGroup(event.target.value)}
                >
                  {workspace.policy_groups.map((group) => (
                    <MenuItem key={group} value={group}>
                      {group}
                    </MenuItem>
                  ))}
                </TextField>
                <Button
                  variant="outlined"
                  disabled={!canTest || !policyGroup}
                  onClick={() =>
                    void perform(async () => {
                      const result = await activateClash5Chain(
                        draft.id,
                        policyGroup,
                      )
                      report(
                        t(
                          result.selection_persisted
                            ? 'chains.activated'
                            : 'chains.activatedSession',
                        ),
                        result.selection_persisted ? 'success' : 'warning',
                      )
                    })
                  }
                >
                  {t('chains.activate')}
                </Button>
                {!clean && (
                  <Typography color="text.secondary" variant="caption">
                    {t('chains.saveBeforeTest')}
                  </Typography>
                )}
                <Typography variant="caption" color="text.secondary">
                  {draft.pinned_exit_ip
                    ? t('chains.pinnedExit', { ip: draft.pinned_exit_ip })
                    : t('chains.notPinned')}
                </Typography>
                <Stack direction="row" sx={{ gap: 1, flexWrap: 'wrap' }}>
                  <Button
                    size="small"
                    disabled={
                      busy ||
                      !exitResult ||
                      (exitResult.status !== 'healthy' &&
                        exitResult.status !== 'drift')
                    }
                    onClick={() => {
                      if (
                        exitResult &&
                        (exitResult.status === 'healthy' ||
                          exitResult.status === 'drift')
                      )
                        setDraft({
                          ...draft,
                          pinned_exit_ip: exitResult.observation.ip,
                        })
                    }}
                  >
                    {t('chains.pinExit')}
                  </Button>
                  <Button
                    size="small"
                    disabled={busy || !draft.pinned_exit_ip}
                    onClick={() => setDraft({ ...draft, pinned_exit_ip: null })}
                  >
                    {t('chains.unpinExit')}
                  </Button>
                </Stack>
                {quality && (
                  <Alert
                    severity={
                      quality.status === 'healthy'
                        ? 'success'
                        : quality.status === 'warning'
                          ? 'warning'
                          : 'error'
                    }
                    sx={{ my: 1 }}
                  >
                    <Typography variant="body2">
                      {t('chains.quality.status')}:{' '}
                      {t(`chains.quality.${quality.status}`)}
                    </Typography>
                    <Typography variant="caption" sx={{ display: 'block' }}>
                      {t('chains.quality.median')}:{' '}
                      {formatMegabytesPerSecond(quality.throughput_median_bps)}{' '}
                      · {t('chains.quality.p10')}:{' '}
                      {formatMegabytesPerSecond(quality.throughput_p10_bps)}
                    </Typography>
                    <Typography variant="caption" sx={{ display: 'block' }}>
                      {t('chains.quality.firstByte')}: {quality.first_byte_ms}{' '}
                      ms · {t('chains.quality.jitter')}: {quality.jitter_ms} ms
                      · {t('chains.quality.rttP95')}: {quality.rtt_p95_ms} ms
                    </Typography>
                    <Typography variant="caption" sx={{ display: 'block' }}>
                      {t('chains.quality.disconnects')}: {quality.disconnects} ·{' '}
                      {t('chains.quality.bytesUsed')}:{' '}
                      {formatMegabytes(quality.bytes_used)}
                    </Typography>
                  </Alert>
                )}
                <Typography variant="subtitle2">
                  {t('chains.history')}
                </Typography>
                {samples.length === 0 ? (
                  <Typography color="text.secondary" variant="caption">
                    {t('chains.historyEmpty')}
                  </Typography>
                ) : (
                  samples.map((sample, index) => (
                    <Typography
                      key={index}
                      variant="caption"
                      sx={{ display: 'block' }}
                    >
                      {sample.measured_at.replace('T', ' ').slice(0, 19)} ·{' '}
                      {t(`chains.sample.${sample.status}`)} ·{' '}
                      {sample.exit_ip ?? '—'}
                      {sample.delay_ms !== null
                        ? ` · ${sample.delay_ms} ms`
                        : ''}
                    </Typography>
                  ))
                )}
                <FormControlLabel
                  label={t('chains.exitEnabled')}
                  control={
                    <Switch
                      checked={exitEnabled}
                      disabled={busy}
                      onChange={(_, enabled) => {
                        setExitEnabled(enabled)
                        appStorage.setItem(
                          'clash5-exit-ip-enabled',
                          String(enabled),
                        )
                      }}
                    />
                  }
                />
                <TextField
                  size="small"
                  label={t('chains.exitService')}
                  value={serviceUrl}
                  disabled={!exitEnabled || busy}
                  onChange={(event) => {
                    setServiceUrl(event.target.value)
                    appStorage.setItem('clash5-exit-ip-url', event.target.value)
                  }}
                />
                <Typography variant="caption" color="text.secondary">
                  {t('chains.exitPrivacy')}
                </Typography>
                {exitResult &&
                  (exitResult.status === 'healthy' ||
                    exitResult.status === 'switched') && (
                    <Alert severity="success">
                      {exitResult.observation.ip}
                      <br />
                      {exitResult.observation.elapsed_ms} ms
                    </Alert>
                  )}
              </Stack>
            </Paper>
          </Box>
        )}
        {workspace && tab === 'apps' && (
          <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
            <Stack spacing={2}>
              <Typography variant="h6">{t('chains.apps')}</Typography>
              <Typography color="text.secondary" variant="body2">
                {t('chains.appRouteHint')}
              </Typography>
              <Button
                variant="contained"
                sx={{ alignSelf: 'flex-start' }}
                onClick={() => navigate('/proxies')}
              >
                {t('proxies.apps.title')}
              </Button>
            </Stack>
          </Paper>
        )}
        {workspace && tab === 'pools' && (
          <Stack spacing={2}>
            <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
              <Stack spacing={2}>
                <Typography variant="subtitle1">
                  {t('chains.createPool')}
                </Typography>
                <Typography color="text.secondary" variant="body2">
                  {t('chains.poolEmpty')}
                </Typography>
                <TextField
                  size="small"
                  label={t('chains.poolName')}
                  value={poolName}
                  disabled={busy}
                  onChange={(event) => setPoolName(event.target.value)}
                />
                <TextField
                  select
                  size="small"
                  label={t('chains.poolMembers')}
                  value={poolMemberKeys}
                  disabled={busy}
                  slotProps={{ select: { multiple: true } }}
                  onChange={(event) => {
                    const value = event.target.value
                    setPoolMemberKeys(
                      typeof value === 'string' ? value.split(',') : value,
                    )
                  }}
                >
                  {workspace.nodes.map((entry) => (
                    <MenuItem
                      key={nodeKey(entry.reference)}
                      value={nodeKey(entry.reference)}
                    >
                      {entry.display_name} · {entry.kind} ·{' '}
                      {entry.source_name ?? t('chains.manual')}
                    </MenuItem>
                  ))}
                </TextField>
                <Button
                  variant="contained"
                  sx={{ alignSelf: 'flex-start' }}
                  disabled={
                    busy ||
                    !poolName.trim() ||
                    poolMemberKeys.length === 0 ||
                    poolMemberKeys.length > MAX_POOL_MEMBERS
                  }
                  onClick={() =>
                    void perform(async () => {
                      const members = workspace.nodes
                        .filter((entry) =>
                          poolMemberKeys.includes(nodeKey(entry.reference)),
                        )
                        .map((entry) => entry.reference)
                      await saveClash5NodePool({
                        schema_version: 2,
                        id: crypto.randomUUID(),
                        name: poolName.trim(),
                        members,
                        mode: 'pinned',
                        pinned: members[0],
                      })
                      setPoolName('')
                      setPoolMemberKeys([])
                      await refreshWorkspace()
                      report(t('chains.poolSaved'))
                    })
                  }
                >
                  {t('chains.savePool')}
                </Button>
              </Stack>
            </Paper>
            {workspace.pools.length === 0 && (
              <Alert severity="info">{t('chains.poolNone')}</Alert>
            )}
            {workspace.pools.map((pool) => (
              <Paper
                key={pool.id}
                variant="outlined"
                sx={{ p: 2, borderRadius: 3 }}
              >
                <Stack
                  direction="row"
                  sx={{
                    gap: 1,
                    alignItems: 'center',
                    justifyContent: 'space-between',
                  }}
                >
                  <Stack>
                    <Typography sx={{ fontWeight: 600 }}>
                      {pool.name}
                    </Typography>
                    <Typography variant="caption" color="text.secondary">
                      {t('chains.poolMembersCount', {
                        count: pool.members.length,
                      })}
                    </Typography>
                  </Stack>
                  <Button
                    color="error"
                    disabled={busy}
                    onClick={() =>
                      void perform(async () => {
                        await deleteClash5NodePool(pool.id)
                        await refreshWorkspace()
                        report(t('chains.poolDeleted'))
                      })
                    }
                  >
                    {t('chains.deletePool')}
                  </Button>
                </Stack>
                <PoolPolicy
                  pool={pool}
                  nodes={workspace.nodes}
                  lastSwitch={switchEvents
                    .filter((event) => event.pool_id === pool.id)
                    .at(-1)}
                  busy={busy}
                  onSaved={refreshWorkspace}
                />
                <PoolScoreList poolId={pool.id} refreshKey={workspace} />
              </Paper>
            ))}
          </Stack>
        )}
      </Stack>
      <ConfirmDialog
        open={qualityConfirmOpen}
        title={t('chains.quality.confirmTitle')}
        message={t('chains.quality.confirm', {
          size: Math.round(QUALITY_DEFAULT_BYTES / 1_000_000),
        })}
        confirmLabel={t('shared.actions.confirm')}
        cancelLabel={t('layout.components.navigation.cancel')}
        loading={busy}
        onConfirm={runQualityTest}
        onCancel={() => !busy && setQualityConfirmOpen(false)}
      />
    </BasePage>
  )
}

export default ChainsPage
