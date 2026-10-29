import {
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
  saveClash5AppRoute,
} from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import type {
  AppRouteDef,
  Clash5Chain,
  RouteTargetRef,
} from '@/types/clash5-chain'

// Per-program egress: each process name gets one route that goes to a chain, direct, or reject.
// Removing a route returns that program to the default egress.
export const AppRoutesPanel = () => {
  const { t } = useTranslation()
  const [routes, setRoutes] = useState<AppRouteDef[]>([])
  const [chains, setChains] = useState<Clash5Chain[]>([])
  const [name, setName] = useState('')
  const [target, setTarget] = useState<string>('default')

  const load = useLockFn(async () => {
    try {
      const [nextRoutes, workspace] = await Promise.all([
        listClash5AppRoutes(),
        getClash5ChainWorkspace(),
      ])
      setRoutes(nextRoutes)
      setChains(workspace.chains.filter((chain) => chain.enabled))
    } catch (error) {
      showNotice.error(error)
    }
  })

  useEffect(() => {
    void load()
  }, [load])

  const toTarget = (value: string): RouteTargetRef | null => {
    if (value === 'direct') return { kind: 'direct' }
    if (value === 'reject') return { kind: 'reject' }
    if (value.startsWith('chain:')) {
      return { kind: 'chain', chain_id: value.slice('chain:'.length) }
    }
    return null
  }

  const targetValue = (routeTarget: RouteTargetRef) =>
    routeTarget.kind === 'chain'
      ? `chain:${routeTarget.chain_id}`
      : routeTarget.kind

  const save = async (route: AppRouteDef) => {
    try {
      await saveClash5AppRoute(route)
      await load()
    } catch (error) {
      showNotice.error(error)
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
    await save({
      schema_version: 1,
      id: crypto.randomUUID(),
      name: processName,
      matchers: [{ kind: 'process_name', name: processName }],
      target: routeTarget,
      enabled: true,
      nameserver: null,
    })
    setName('')
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
                <MenuItem value="direct">{t('proxies.apps.direct')}</MenuItem>
                <MenuItem value="reject">{t('proxies.apps.reject')}</MenuItem>
                {chains.map((chain) => (
                  <MenuItem key={chain.id} value={`chain:${chain.id}`}>
                    {chain.name}
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
          <TextField
            size="small"
            placeholder={t('proxies.apps.placeholder')}
            value={name}
            onChange={(event) => setName(event.target.value)}
            sx={{ minWidth: 200 }}
          />
          <TextField
            select
            size="small"
            sx={{ minWidth: 200 }}
            value={target}
            onChange={(event) => setTarget(event.target.value)}
          >
            <MenuItem value="direct">{t('proxies.apps.direct')}</MenuItem>
            <MenuItem value="reject">{t('proxies.apps.reject')}</MenuItem>
            {chains.map((chain) => (
              <MenuItem key={chain.id} value={`chain:${chain.id}`}>
                {chain.name}
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
