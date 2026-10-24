import ArrowForwardIcon from '@mui/icons-material/ArrowForward'
import { Fragment, useCallback, useEffect, useState } from 'react'
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
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import {
  activateClash5Chain,
  deleteClash5AppRoute,
  getClash5ChainSamples,
  deleteClash5Chain,
  deleteClash5NodePool,
  detectClash5ExitIp,
  getClash5ChainWorkspace,
  listClash5AppRoutes,
  saveClash5AppRoute,
  saveClash5Chain,
  saveClash5NodePool,
  testClash5Chain,
} from '@/services/cmds'
import type {
  AppMatcherRef,
  AppRouteDef,
  ChainHopRef,
  ChainSample,
  ChainNodeOption,
  ChainNodeRef,
  ChainWorkspace,
  Clash5Chain,
  ExitIpResult,
  RouteTargetRef,
} from '@/types/clash5-chain'

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

const newRoute = (): AppRouteDef => ({
  schema_version: 1,
  id: crypto.randomUUID(),
  name: '',
  matchers: [{ kind: 'process_name', name: '' }],
  target: { kind: 'direct' },
  enabled: true,
  nameserver: null,
})
const matcherValue = (matcher: AppMatcherRef) =>
  matcher.kind === 'process_name'
    ? matcher.name
    : matcher.kind === 'process_path'
      ? matcher.path
      : matcher.suffix
const matcherWith = (
  kind: AppMatcherRef['kind'],
  value: string,
): AppMatcherRef =>
  kind === 'process_name'
    ? { kind, name: value }
    : kind === 'process_path'
      ? { kind, path: value }
      : { kind, suffix: value }
const targetValue = (target: RouteTargetRef) =>
  target.kind === 'chain' ? `chain:${target.chain_id}` : target.kind
const targetFromValue = (value: string): RouteTargetRef =>
  value.startsWith('chain:')
    ? { kind: 'chain', chain_id: value.slice(6) }
    : value === 'reject'
      ? { kind: 'reject' }
      : { kind: 'direct' }
const targetLabel = (
  target: RouteTargetRef,
  chains: readonly { id: string; name: string }[],
  labels: { missing: string; direct: string; reject: string },
) => {
  if (target.kind !== 'chain') {
    return target.kind === 'direct' ? labels.direct : labels.reject
  }
  const chainId = target.chain_id
  return chains.find((chain) => chain.id === chainId)?.name ?? labels.missing
}
const MAX_MATCHERS = 32

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
  const [workspace, setWorkspace] = useState<ChainWorkspace | null>(null)
  const [draft, setDraft] = useState<Clash5Chain | null>(null)
  const [tab, setTab] = useState('visual')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const [severity, setSeverity] = useState<'success' | 'warning' | 'error'>(
    'success',
  )
  const [policyGroup, setPolicyGroup] = useState('')
  const [exitResult, setExitResult] = useState<ExitIpResult | null>(null)
  const [appRoutes, setAppRoutes] = useState<AppRouteDef[]>([])
  const [samples, setSamples] = useState<ChainSample[]>([])
  const [routeDraft, setRouteDraft] = useState<AppRouteDef | null>(null)
  const [poolName, setPoolName] = useState('')
  const [poolMemberKeys, setPoolMemberKeys] = useState<string[]>([])
  const [exitEnabled, setExitEnabled] = useState(
    () => localStorage.getItem('clash5-exit-ip-enabled') !== 'false',
  )
  const [serviceUrl, setServiceUrl] = useState(
    () =>
      localStorage.getItem('clash5-exit-ip-url') ?? 'https://api64.ipify.org',
  )

  const load = useCallback(async (selectedId?: string) => {
    const [next, routes] = await Promise.all([
      getClash5ChainWorkspace(),
      listClash5AppRoutes(),
    ])
    setWorkspace(next)
    setAppRoutes(routes)
    const selected =
      next.chains.find((chain) => chain.id === selectedId) ?? next.chains[0]
    setDraft(selected ? structuredClone(selected) : newChain(next))
    setSamples([])
    if (selected) {
      void getClash5ChainSamples(selected.id, 10)
        .then(setSamples)
        .catch(() => setSamples([]))
    }
    setPolicyGroup((current) =>
      next.policy_groups.includes(current)
        ? current
        : (next.policy_groups[0] ?? ''),
    )
    setExitResult(null)
  }, [])

  // Pool edits change only the pool list, so an unsaved chain draft is kept.
  const refreshWorkspace = useCallback(async () => {
    setWorkspace(await getClash5ChainWorkspace())
  }, [])

  const refreshSamples = useCallback(async (chainId: string) => {
    try {
      setSamples(await getClash5ChainSamples(chainId, 10))
    } catch {
      setSamples([])
    }
  }, [])

  const refreshRoutes = useCallback(async () => {
    setAppRoutes(await listClash5AppRoutes())
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
    draft.hops.slice(1).every((hop, index) => !sameHop(hop, draft.hops[index])),
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

  const choose = (chain: Clash5Chain) => {
    setDraft(structuredClone(chain))
    setExitResult(null)
    setMessage('')
    setTab('visual')
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
            <Tab value="visual" label={t('chains.visual')} />
            <Tab value="list" label={t('chains.list')} />
            <Tab value="pools" label={t('chains.pools')} />
            <Tab value="apps" label={t('chains.apps')} />
            <Tab value="templates" label={t('chains.templates')} />
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
        {workspace && draft && tab === 'visual' && (
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
                <Button
                  size="small"
                  disabled={busy || draft.hops.length >= MAX_HOPS}
                  onClick={addHop}
                >
                  {t('chains.addHop')}
                </Button>
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
                        localStorage.setItem(
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
                    localStorage.setItem(
                      'clash5-exit-ip-url',
                      event.target.value,
                    )
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
        {workspace && tab === 'list' && (
          <Stack spacing={1}>
            <Button
              variant="outlined"
              disabled={busy}
              onClick={() => {
                setDraft(newChain(workspace))
                setTab('visual')
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
        {workspace && tab === 'apps' && (
          <Stack spacing={2}>
            <Stack direction="row">
              <Button
                variant="outlined"
                disabled={busy}
                onClick={() => setRouteDraft(newRoute())}
              >
                {t('chains.appNewRoute')}
              </Button>
            </Stack>
            <Typography color="text.secondary" variant="body2">
              {t('chains.appRouteHint')}
            </Typography>
            {routeDraft && (
              <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
                <Stack spacing={2}>
                  <TextField
                    size="small"
                    label={t('chains.appRouteName')}
                    value={routeDraft.name}
                    disabled={busy}
                    onChange={(event) =>
                      setRouteDraft({ ...routeDraft, name: event.target.value })
                    }
                  />
                  {routeDraft.matchers.map((matcher, index) => (
                    <Stack
                      key={index}
                      direction="row"
                      sx={{ gap: 1, alignItems: 'center', flexWrap: 'wrap' }}
                    >
                      <TextField
                        select
                        size="small"
                        sx={{ minWidth: 180 }}
                        label={t('chains.appMatcherKind')}
                        value={matcher.kind}
                        disabled={busy}
                        onChange={(event) => {
                          const kind = event.target
                            .value as AppMatcherRef['kind']
                          setRouteDraft({
                            ...routeDraft,
                            matchers: routeDraft.matchers.map(
                              (entry, position) =>
                                position === index
                                  ? matcherWith(kind, matcherValue(entry))
                                  : entry,
                            ),
                          })
                        }}
                      >
                        <MenuItem value="process_name">
                          {t('chains.appProcessName')}
                        </MenuItem>
                        <MenuItem value="process_path">
                          {t('chains.appProcessPath')}
                        </MenuItem>
                        <MenuItem value="domain_suffix">
                          {t('chains.appDomainSuffix')}
                        </MenuItem>
                      </TextField>
                      <TextField
                        size="small"
                        sx={{ flex: 1, minWidth: 160 }}
                        label={t('chains.appMatcherValue')}
                        value={matcherValue(matcher)}
                        disabled={busy}
                        onChange={(event) =>
                          setRouteDraft({
                            ...routeDraft,
                            matchers: routeDraft.matchers.map(
                              (entry, position) =>
                                position === index
                                  ? matcherWith(entry.kind, event.target.value)
                                  : entry,
                            ),
                          })
                        }
                      />
                      <Button
                        size="small"
                        color="error"
                        disabled={busy || routeDraft.matchers.length <= 1}
                        onClick={() =>
                          setRouteDraft({
                            ...routeDraft,
                            matchers: routeDraft.matchers.filter(
                              (_, position) => position !== index,
                            ),
                          })
                        }
                      >
                        {t('chains.appRemoveMatcher')}
                      </Button>
                    </Stack>
                  ))}
                  {routeDraft.matchers.some(
                    (matcher) => matcher.kind === 'domain_suffix',
                  ) && (
                    <>
                      <TextField
                        size="small"
                        label={t('chains.appNameserver')}
                        helperText={t('chains.appNameserverHint')}
                        value={routeDraft.nameserver ?? ''}
                        disabled={busy}
                        onChange={(event) =>
                          setRouteDraft({
                            ...routeDraft,
                            nameserver: event.target.value,
                          })
                        }
                      />
                      <Alert severity="info">
                        {t('chains.appDomainScope')}
                      </Alert>
                    </>
                  )}
                  <Button
                    size="small"
                    sx={{ alignSelf: 'flex-start' }}
                    disabled={
                      busy || routeDraft.matchers.length >= MAX_MATCHERS
                    }
                    onClick={() =>
                      setRouteDraft({
                        ...routeDraft,
                        matchers: [
                          ...routeDraft.matchers,
                          { kind: 'process_name', name: '' },
                        ],
                      })
                    }
                  >
                    {t('chains.appAddMatcher')}
                  </Button>
                  <TextField
                    select
                    size="small"
                    label={t('chains.appTarget')}
                    value={targetValue(routeDraft.target)}
                    disabled={busy}
                    onChange={(event) =>
                      setRouteDraft({
                        ...routeDraft,
                        target: targetFromValue(event.target.value),
                      })
                    }
                  >
                    <MenuItem value="direct">{t('chains.appDirect')}</MenuItem>
                    <MenuItem value="reject">{t('chains.appReject')}</MenuItem>
                    {workspace.chains
                      .filter((chain) => chain.enabled)
                      .map((chain) => (
                        <MenuItem key={chain.id} value={`chain:${chain.id}`}>
                          {chain.name}
                        </MenuItem>
                      ))}
                  </TextField>
                  <FormControlLabel
                    label={t('chains.enabled')}
                    control={
                      <Switch
                        checked={routeDraft.enabled}
                        disabled={busy}
                        onChange={(_, enabled) =>
                          setRouteDraft({ ...routeDraft, enabled })
                        }
                      />
                    }
                  />
                  <Stack direction="row" sx={{ gap: 1 }}>
                    <Button
                      variant="contained"
                      disabled={
                        busy ||
                        !routeDraft.name.trim() ||
                        routeDraft.matchers.some(
                          (matcher) => !matcherValue(matcher).trim(),
                        )
                      }
                      onClick={() =>
                        void perform(async () => {
                          await saveClash5AppRoute({
                            ...routeDraft,
                            nameserver: routeDraft.nameserver?.trim() || null,
                          })
                          setRouteDraft(null)
                          await refreshRoutes()
                          report(t('chains.appSaved'))
                        })
                      }
                    >
                      {t('chains.appSaveRoute')}
                    </Button>
                    <Button disabled={busy} onClick={() => setRouteDraft(null)}>
                      {t('shared.actions.cancel')}
                    </Button>
                  </Stack>
                </Stack>
              </Paper>
            )}
            {appRoutes.length === 0 && (
              <Alert severity="info">{t('chains.appEmpty')}</Alert>
            )}
            {appRoutes.map((route) => (
              <Paper
                key={route.id}
                variant="outlined"
                sx={{ p: 2, borderRadius: 3 }}
              >
                <Stack
                  direction="row"
                  sx={{
                    gap: 1,
                    alignItems: 'center',
                    flexWrap: 'wrap',
                    justifyContent: 'space-between',
                  }}
                >
                  <Stack>
                    <Typography sx={{ fontWeight: 600 }}>
                      {route.name}
                    </Typography>
                    <Typography variant="caption" color="text.secondary">
                      {route.matchers.map(matcherValue).join(', ')}
                      {' → '}
                      {targetLabel(route.target, workspace.chains, {
                        missing: t('chains.missingChain'),
                        direct: t('chains.appDirect'),
                        reject: t('chains.appReject'),
                      })}
                    </Typography>
                  </Stack>
                  <Stack direction="row" sx={{ gap: 1, alignItems: 'center' }}>
                    <Chip
                      size="small"
                      label={t(
                        route.enabled ? 'chains.enabled' : 'chains.disabled',
                      )}
                    />
                    <Button
                      size="small"
                      disabled={busy}
                      onClick={() => setRouteDraft(structuredClone(route))}
                    >
                      {t('chains.appEditRoute')}
                    </Button>
                    <Button
                      size="small"
                      color="error"
                      disabled={busy}
                      onClick={() =>
                        void perform(async () => {
                          await deleteClash5AppRoute(route.id)
                          await refreshRoutes()
                          report(t('chains.appDeleted'))
                        })
                      }
                    >
                      {t('chains.appDeleteRoute')}
                    </Button>
                  </Stack>
                </Stack>
              </Paper>
            ))}
          </Stack>
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
                        schema_version: 1,
                        id: crypto.randomUUID(),
                        name: poolName.trim(),
                        members,
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
              </Paper>
            ))}
          </Stack>
        )}
        {workspace && tab === 'templates' && (
          <Paper variant="outlined" sx={{ p: 3, borderRadius: 3 }}>
            <Typography variant="subtitle1">
              {t('chains.twoHopTemplate')}
            </Typography>
            <Typography color="text.secondary" sx={{ my: 1 }}>
              {t('chains.twoHopHint')}
            </Typography>
            <Button
              disabled={busy}
              onClick={() => {
                setDraft(newChain(workspace))
                setTab('visual')
                setExitResult(null)
              }}
            >
              {t('chains.useTemplate')}
            </Button>
          </Paper>
        )}
      </Stack>
    </BasePage>
  )
}

export default ChainsPage
