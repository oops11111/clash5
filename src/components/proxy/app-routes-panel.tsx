import {
  Alert,
  Autocomplete,
  Button,
  FormControlLabel,
  MenuItem,
  Paper,
  Stack,
  Switch,
  TextField,
  Typography,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  deleteClash5AppRoute,
  getClash5ChainWorkspace,
  listClash5AppRoutes,
  listClash5Processes,
  saveClash5AppRoute,
} from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import type {
  AppMatcherRef,
  AppRouteDef,
  ChainNodeRef,
  ChainWorkspace,
  RouteTargetRef,
} from '@/types/clash5-chain'

const MAX_MATCHERS = 32

const sameNode = (left: ChainNodeRef, right: ChainNodeRef) =>
  left.stable_id === right.stable_id &&
  left.source.kind === right.source.kind &&
  (left.source.kind !== 'subscription' ||
    (right.source.kind === 'subscription' &&
      left.source.provider_id === right.source.provider_id))

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

const nodeKey = (reference: ChainNodeRef) => JSON.stringify(reference)

const targetValue = (target: RouteTargetRef) =>
  target.kind === 'chain'
    ? `chain:${target.chain_id}`
    : target.kind === 'group'
      ? `group:${target.name}`
      : target.kind === 'node'
        ? `node:${nodeKey(target.node)}`
        : target.kind

const targetFromValue = (value: string): RouteTargetRef | null => {
  if (value.startsWith('group:')) return { kind: 'group', name: value.slice(6) }
  if (value.startsWith('chain:'))
    return { kind: 'chain', chain_id: value.slice('chain:'.length) }
  if (value.startsWith('node:')) {
    try {
      return { kind: 'node', node: JSON.parse(value.slice(5)) as ChainNodeRef }
    } catch {
      return null
    }
  }
  if (value === 'direct') return { kind: 'direct' }
  if (value === 'reject') return { kind: 'reject' }
  return null
}

const targetAvailable = (target: RouteTargetRef, workspace: ChainWorkspace) => {
  if (target.kind === 'direct' || target.kind === 'reject') return true
  if (target.kind === 'group') return workspace.route_groups.includes(target.name)
  if (target.kind === 'chain') {
    return workspace.chains.some(
      (chain) => chain.id === target.chain_id && chain.enabled,
    )
  }
  return workspace.nodes.some((node) => sameNode(node.reference, target.node))
}

const AppRoutesPanel = () => {
  const { t } = useTranslation()
  const [routes, setRoutes] = useState<AppRouteDef[]>([])
  const [workspace, setWorkspace] = useState<ChainWorkspace | null>(null)
  const [draft, setDraft] = useState<AppRouteDef | null>(null)
  const [processes, setProcesses] = useState<string[]>([])
  const [quickName, setQuickName] = useState('')
  const [quickTarget, setQuickTarget] = useState('direct')
  const [busy, setBusy] = useState(false)
  const reportedMissingRoutesRef = useRef(new Set<string>())

  const loadProcesses = useLockFn(async () => {
    try {
      const running = await listClash5Processes()
      setProcesses(running.map((process) => process.name))
    } catch (error) {
      showNotice.error(error)
    }
  })

  const load = useLockFn(async () => {
    try {
      const [nextRoutes, nextWorkspace] = await Promise.all([
        listClash5AppRoutes(),
        getClash5ChainWorkspace(),
      ])
      setRoutes(nextRoutes)
      setWorkspace(nextWorkspace)
      for (const route of nextRoutes) {
        const missing = !targetAvailable(route.target, nextWorkspace)
        if (missing && !reportedMissingRoutesRef.current.has(route.id)) {
          reportedMissingRoutesRef.current.add(route.id)
          showNotice.error('chains.appMissingTarget')
        } else if (!missing) {
          reportedMissingRoutesRef.current.delete(route.id)
        }
      }
    } catch (error) {
      showNotice.error(error)
    }
  })

  useEffect(() => {
    void load()
  }, [load])

  const run = async (action: () => Promise<void>) => {
    setBusy(true)
    try {
      await action()
    } catch (error) {
      showNotice.error(error)
    } finally {
      setBusy(false)
    }
  }

  const targets = workspace
    ? [
        { value: 'direct', label: t('proxies.apps.direct') },
        { value: 'reject', label: t('proxies.apps.reject') },
        ...workspace.route_groups.map((name) => ({
          value: `group:${name}`,
          label: `${t('chains.appPolicyGroup')}: ${name}`,
        })),
        ...workspace.nodes.map((node) => ({
          value: `node:${nodeKey(node.reference)}`,
          label: `${t('chains.appSingleNode')}: ${node.display_name}${node.source_name ? ` · ${node.source_name}` : ''}`,
        })),
        ...workspace.chains
          .filter((chain) => chain.enabled)
          .map((chain) => ({
            value: `chain:${chain.id}`,
            label: `${t('chains.appMultiHop')}: ${chain.name}`,
          })),
      ]
    : []

  const targetLabel = (target: RouteTargetRef) => {
    if (!workspace) return t('chains.appMissingTarget')
    if (target.kind === 'direct') return t('proxies.apps.direct')
    if (target.kind === 'reject') return t('proxies.apps.reject')
    if (target.kind === 'group') return target.name
    if (target.kind === 'chain')
      return (
        workspace.chains.find((chain) => chain.id === target.chain_id)?.name ??
        t('chains.appMissingTarget')
      )
    return (
      workspace.nodes.find((node) => sameNode(node.reference, target.node))
        ?.display_name ?? t('chains.appMissingTarget')
    )
  }

  const save = (route: AppRouteDef) =>
    run(async () => {
      await saveClash5AppRoute(route)
      setDraft(null)
      await load()
    })

  const remove = (route: AppRouteDef) =>
    run(async () => {
      await deleteClash5AppRoute(route.id)
      await load()
    })

  const addProcess = (name: string, targetValueToSave: string) => {
    const processName = name.trim()
    const target = targetFromValue(targetValueToSave)
    if (!processName || !target) return
    void save({
      ...newRoute(),
      name: processName,
      matchers: [{ kind: 'process_name', name: processName }],
      target,
    })
    setQuickName('')
  }

  return (
    <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
      <Stack spacing={1.5}>
        <Typography variant="subtitle1">{t('proxies.apps.title')}</Typography>
        <Typography variant="body2" color="text.secondary">
          {t('proxies.apps.hint')}
        </Typography>

        {routes.length === 0 && (
          <Typography variant="body2" color="text.secondary">
            {t('proxies.apps.empty')}
          </Typography>
        )}

        {routes.map((route) => (
          <Stack key={route.id} spacing={0.75}>
            <Stack
              direction="row"
              spacing={1}
              sx={{ alignItems: 'center', flexWrap: 'wrap' }}
            >
              <Stack sx={{ minWidth: 180, flex: 1 }}>
                <Typography sx={{ fontWeight: 600 }}>{route.name}</Typography>
                <Typography variant="caption" color="text.secondary">
                  {route.matchers.map(matcherValue).join(', ')} → {targetLabel(route.target)}
                </Typography>
              </Stack>
              <Typography variant="caption" color="text.secondary">
                {t(route.enabled ? 'chains.enabled' : 'chains.disabled')}
              </Typography>
              <Button
                size="small"
                disabled={busy}
                onClick={() => setDraft(structuredClone(route))}
              >
                {t('chains.appEditRoute')}
              </Button>
              <Button
                size="small"
                color="error"
                disabled={busy}
                onClick={() => void remove(route)}
              >
                {t('proxies.apps.remove')}
              </Button>
            </Stack>
            {workspace && !targetAvailable(route.target, workspace) && (
              <Alert severity="error" variant="outlined">
                {t('chains.appMissingTarget')}
              </Alert>
            )}
          </Stack>
        ))}

        {draft && (
          <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
            <Stack spacing={1.5}>
              <TextField
                size="small"
                label={t('chains.appRouteName')}
                value={draft.name}
                disabled={busy}
                onChange={(event) => setDraft({ ...draft, name: event.target.value })}
              />
              {draft.matchers.map((matcher, index) => (
                <Stack
                  key={`${matcher.kind}:${matcherValue(matcher)}`}
                  direction="row"
                  spacing={1}
                  sx={{ alignItems: 'center', flexWrap: 'wrap' }}
                >
                  <TextField
                    select
                    size="small"
                    sx={{ minWidth: 170 }}
                    label={t('chains.appMatcherKind')}
                    value={matcher.kind}
                    disabled={busy}
                    onChange={(event) => {
                      const kind = event.target.value as AppMatcherRef['kind']
                      setDraft({
                        ...draft,
                        matchers: draft.matchers.map((entry, position) =>
                          position === index
                            ? matcherWith(kind, matcherValue(entry))
                            : entry,
                        ),
                      })
                    }}
                  >
                    <MenuItem value="process_name">{t('chains.appProcessName')}</MenuItem>
                    <MenuItem value="process_path">{t('chains.appProcessPath')}</MenuItem>
                    <MenuItem value="domain_suffix">{t('chains.appDomainSuffix')}</MenuItem>
                  </TextField>
                  {matcher.kind === 'process_name' ? (
                    <Autocomplete
                      freeSolo
                      size="small"
                      sx={{ flex: 1, minWidth: 180 }}
                      options={processes}
                      inputValue={matcher.name}
                      onOpen={() => void loadProcesses()}
                      onInputChange={(_, value) =>
                        setDraft({
                          ...draft,
                          matchers: draft.matchers.map((entry, position) =>
                            position === index
                              ? { kind: 'process_name', name: value }
                              : entry,
                          ),
                        })
                      }
                      renderInput={(params) => (
                        <TextField {...params} label={t('chains.appMatcherValue')} />
                      )}
                    />
                  ) : (
                    <TextField
                      size="small"
                      sx={{ flex: 1, minWidth: 180 }}
                      label={t('chains.appMatcherValue')}
                      value={matcherValue(matcher)}
                      disabled={busy}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          matchers: draft.matchers.map((entry, position) =>
                            position === index
                              ? matcherWith(entry.kind, event.target.value)
                              : entry,
                          ),
                        })
                      }
                    />
                  )}
                  <Button
                    size="small"
                    color="error"
                    disabled={busy || draft.matchers.length <= 1}
                    onClick={() =>
                      setDraft({
                        ...draft,
                        matchers: draft.matchers.filter((_, position) => position !== index),
                      })
                    }
                  >
                    {t('chains.appRemoveMatcher')}
                  </Button>
                </Stack>
              ))}
              {draft.matchers.some((matcher) => matcher.kind === 'domain_suffix') && (
                <TextField
                  size="small"
                  label={t('chains.appNameserver')}
                  helperText={t('chains.appNameserverHint')}
                  value={draft.nameserver ?? ''}
                  disabled={busy}
                  onChange={(event) => setDraft({ ...draft, nameserver: event.target.value })}
                />
              )}
              <Button
                size="small"
                sx={{ alignSelf: 'flex-start' }}
                disabled={busy || draft.matchers.length >= MAX_MATCHERS}
                onClick={() =>
                  setDraft({
                    ...draft,
                    matchers: [...draft.matchers, { kind: 'process_name', name: '' }],
                  })
                }
              >
                {t('chains.appAddMatcher')}
              </Button>
              <TextField
                select
                size="small"
                label={t('chains.appTarget')}
                value={targetValue(draft.target)}
                disabled={busy}
                onChange={(event) => {
                  const target = targetFromValue(event.target.value)
                  if (target) setDraft({ ...draft, target })
                }}
              >
                {!targets.some((option) => option.value === targetValue(draft.target)) && (
                  <MenuItem value={targetValue(draft.target)} disabled>
                    {t('chains.appMissingTarget')}
                  </MenuItem>
                )}
                {targets.map((option) => (
                  <MenuItem key={option.value} value={option.value}>
                    {option.label}
                  </MenuItem>
                ))}
              </TextField>
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
              <Stack direction="row" spacing={1}>
                <Button
                  variant="contained"
                  disabled={
                    busy ||
                    !draft.name.trim() ||
                    draft.matchers.some((matcher) => !matcherValue(matcher).trim())
                  }
                  onClick={() =>
                    void save({
                      ...draft,
                      name: draft.name.trim(),
                      nameserver: draft.nameserver?.trim() || null,
                    })
                  }
                >
                  {t('chains.appSaveRoute')}
                </Button>
                <Button disabled={busy} onClick={() => setDraft(null)}>
                  {t('shared.actions.cancel')}
                </Button>
              </Stack>
            </Stack>
          </Paper>
        )}

        <Stack direction="row" spacing={1} sx={{ alignItems: 'center', flexWrap: 'wrap' }}>
          <Autocomplete
            freeSolo
            size="small"
            sx={{ minWidth: 200 }}
            options={processes}
            inputValue={quickName}
            onInputChange={(_, value) => setQuickName(value)}
            onOpen={() => void loadProcesses()}
            renderInput={(params) => (
              <TextField {...params} placeholder={t('proxies.apps.placeholder')} />
            )}
          />
          <TextField
            select
            size="small"
            sx={{ minWidth: 200 }}
            label={t('proxies.apps.target')}
            value={quickTarget}
            onChange={(event) => setQuickTarget(event.target.value)}
          >
            {targets.map((option) => (
              <MenuItem key={option.value} value={option.value}>
                {option.label}
              </MenuItem>
            ))}
          </TextField>
          <Button
            size="small"
            variant="contained"
            disabled={busy || !quickName.trim()}
            onClick={() => addProcess(quickName, quickTarget)}
          >
            {t('proxies.apps.add')}
          </Button>
          <Button
            size="small"
            variant="outlined"
            disabled={busy}
            onClick={() => setDraft(newRoute())}
          >
            {t('chains.appNewRoute')}
          </Button>
        </Stack>
      </Stack>
    </Paper>
  )
}

export { AppRoutesPanel }
