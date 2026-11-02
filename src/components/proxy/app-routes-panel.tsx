import {
  Autocomplete,
  Button,
  MenuItem,
  Paper,
  Stack,
  TextField,
  Typography,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import { useEffect, useState } from 'react'
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
  AppRouteDef,
  Clash5Chain,
  ChainWorkspace,
  ChainNodeRef,
  RouteTargetRef,
} from '@/types/clash5-chain'

// Per-program egress uses an explicit target; removing a route restores the default egress.
// Removing a route returns that program to the default egress.
export const AppRoutesPanel = () => {
  const { t } = useTranslation()
  const [routes, setRoutes] = useState<AppRouteDef[]>([])
  const [chains, setChains] = useState<Clash5Chain[]>([])
  const [groups, setGroups] = useState<string[]>([])
  const [nodes, setNodes] = useState<ChainWorkspace['nodes']>([])
  const [name, setName] = useState('')
  const [target, setTarget] = useState<string>('direct')
  const [processes, setProcesses] = useState<string[]>([])

  // Opening the field lists the programs running now; typing a name that is not listed still works.
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
      const [nextRoutes, workspace] = await Promise.all([
        listClash5AppRoutes(),
        getClash5ChainWorkspace(),
      ])
      setRoutes(nextRoutes)
      setChains(workspace.chains.filter((chain) => chain.enabled))
      setGroups(workspace.route_groups)
      setNodes(workspace.nodes)
    } catch (error) {
      showNotice.error(error)
    }
  })

  useEffect(() => {
    void load()
  }, [load])

  const toTarget = (value: string): RouteTargetRef | null => {
    if (value.startsWith('group:'))
      return { kind: 'group', name: value.slice(6) }
    if (value.startsWith('node:'))
      return { kind: 'node', node: JSON.parse(value.slice(5)) as ChainNodeRef }
    if (value === 'direct') return { kind: 'direct' }
    if (value === 'reject') return { kind: 'reject' }
    if (value.startsWith('chain:')) {
      return { kind: 'chain', chain_id: value.slice('chain:'.length) }
    }
    return null
  }

  const targetValue = (routeTarget: RouteTargetRef) =>
    routeTarget.kind === 'group'
      ? `group:${routeTarget.name}`
      : routeTarget.kind === 'node'
        ? `node:${JSON.stringify(routeTarget.node)}`
        : routeTarget.kind === 'chain'
          ? `chain:${routeTarget.chain_id}`
          : routeTarget.kind

  const targets = [
    { value: 'direct', label: t('proxies.apps.direct') },
    { value: 'reject', label: t('proxies.apps.reject') },
    ...groups.map((group) => ({
      value: `group:${group}`,
      label: `${t('chains.appPolicyGroup')}: ${group}`,
    })),
    ...nodes.map((node) => ({
      value: `node:${JSON.stringify(node.reference)}`,
      label: `${t('chains.appSingleNode')}: ${node.display_name}${node.source_name ? ` · ${node.source_name}` : ''}`,
    })),
    ...chains.map((chain) => ({
      value: `chain:${chain.id}`,
      label: `${t('chains.appMultiHop')}: ${chain.name}`,
    })),
  ]

  const save = async (route: AppRouteDef) => {
    try {
      await saveClash5AppRoute(route)
      await load()
      return true
    } catch (error) {
      showNotice.error(error)
      return false
    }
  }

  const remove = async (route: AppRouteDef) => {
    try {
      await deleteClash5AppRoute(route.id)
      await load()
    } catch (error) {
      showNotice.error(error)
    }
  }

  const add = async () => {
    const processName = name.trim()
    const routeTarget = toTarget(target)
    if (!processName || !routeTarget) return
    const saved = await save({
      schema_version: 1,
      id: crypto.randomUUID(),
      name: processName,
      matchers: [{ kind: 'process_name', name: processName }],
      target: routeTarget,
      enabled: true,
      nameserver: null,
    })
    if (saved) setName('')
  }

  const programRoutes = routes.filter((route) =>
    route.matchers.some((matcher) => matcher.kind === 'process_name'),
  )

  return (
    <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
      <Stack spacing={1.5}>
        <Typography variant="subtitle1">{t('proxies.apps.title')}</Typography>
        <Typography variant="body2" color="text.secondary">
          {t('proxies.apps.hint')}
        </Typography>

        {programRoutes.length === 0 && (
          <Typography variant="body2" color="text.secondary">
            {t('proxies.apps.empty')}
          </Typography>
        )}

        {programRoutes.map((route) => {
          const processName = route.matchers.find(
            (matcher) => matcher.kind === 'process_name',
          )?.name
          return (
            <Stack
              key={route.id}
              direction="row"
              spacing={1}
              sx={{ alignItems: 'center', flexWrap: 'wrap' }}
            >
              <Typography sx={{ minWidth: 160, fontWeight: 600 }}>
                {processName ?? route.name}
              </Typography>
              <TextField
                select
                size="small"
                sx={{ minWidth: 200 }}
                value={targetValue(route.target)}
                onChange={(event) => {
                  const next = toTarget(event.target.value)
                  if (next) void save({ ...route, target: next })
                }}
                slotProps={{
                  htmlInput: {
                    'aria-label': t('proxies.apps.target'),
                  },
                }}
              >
                {!targets.some(
                  (option) => option.value === targetValue(route.target),
                ) && (
                  <MenuItem value={targetValue(route.target)} disabled>
                    {t('chains.appMissingTarget')}
                  </MenuItem>
                )}
                {targets.map((option) => (
                  <MenuItem key={option.value} value={option.value}>
                    {option.label}
                  </MenuItem>
                ))}
              </TextField>
              <Button
                size="small"
                color="error"
                onClick={() => void remove(route)}
              >
                {t('proxies.apps.remove')}
              </Button>
            </Stack>
          )
        })}

        <Stack
          direction="row"
          spacing={1}
          sx={{ alignItems: 'center', flexWrap: 'wrap' }}
        >
          <Autocomplete
            freeSolo
            size="small"
            sx={{ minWidth: 200 }}
            options={processes}
            inputValue={name}
            onInputChange={(_, value) => setName(value)}
            onOpen={() => void loadProcesses()}
            renderInput={(params) => (
              <TextField
                {...params}
                placeholder={t('proxies.apps.placeholder')}
              />
            )}
          />
          <TextField
            select
            size="small"
            sx={{ minWidth: 200 }}
            value={target}
            onChange={(event) => setTarget(event.target.value)}
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
            disabled={!name.trim()}
            onClick={() => void add()}
          >
            {t('proxies.apps.add')}
          </Button>
        </Stack>
      </Stack>
    </Paper>
  )
}
