import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  MenuItem,
  Paper,
  Select,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
  Typography,
} from '@mui/material'
import { useTheme } from '@mui/material/styles'
import { readText } from '@tauri-apps/plugin-clipboard-manager'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { BasePage } from '@/components/base'
import {
  getProfiles,
  importClash5ManualProxies,
  importProfile,
  listClash5Nodes,
  probeClash5Nodes,
} from '@/services/cmds'
import type { Clash5NodeRecord } from '@/types/clash5-node'
import parseUri from '@/utils/uri-parser'

type Filter =
  | 'all'
  | 'manual'
  | 'subscription'
  | 'duplicates'
  | 'highQuality'
  | 'inactive'
type Sort = 'name' | 'source' | 'quality'

const recordKey = (record: Clash5NodeRecord) =>
  `${record.source.kind}:${record.source.kind === 'subscription' ? record.source.provider_id : ''}:${record.stable_id}`

const isSubscriptionUrl = (value: string) => {
  try {
    const url = new URL(value)
    if (!['http:', 'https:'].includes(url.protocol) || !url.hostname) {
      return false
    }
    const explicitPort =
      /^https?:\/\/(?:[^/@]+@)?(?:\[[^\]]+\]|[^/:?#]+):\d+(?:[/?#]|$)/i.test(
        value,
      )
    const looksLikeProxyEndpoint =
      Boolean(url.username || url.password || explicitPort) &&
      url.pathname === '/'
    return !looksLikeProxyEndpoint
  } catch {
    return false
  }
}

const NodePoolPage = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const dark = useTheme().palette.mode === 'dark'
  const [records, setRecords] = useState<Clash5NodeRecord[]>([])
  const [profileNames, setProfileNames] = useState<Record<string, string>>({})
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState(false)
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState<Filter>('all')
  const [sort, setSort] = useState<Sort>('name')
  const [selectedKey, setSelectedKey] = useState<string | null>(null)
  const [probeBusy, setProbeBusy] = useState(false)
  const [probeMessage, setProbeMessage] = useState('')
  const [probeError, setProbeError] = useState(false)
  const [importBusy, setImportBusy] = useState(false)
  const [importMessage, setImportMessage] = useState('')
  const [importError, setImportError] = useState(false)

  const refresh = useCallback(async () => {
    setLoading(true)
    setError(false)
    try {
      const nodes = await listClash5Nodes(true)
      setRecords(nodes)
      const profiles = await getProfiles().catch(() => null)
      setProfileNames(
        Object.fromEntries(
          (profiles?.items ?? [])
            .filter((item) => item.uid)
            .map((item) => [item.uid, item.name ?? item.uid] as const),
        ),
      )
    } catch {
      setError(true)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const runProbe = useCallback(async () => {
    setProbeBusy(true)
    setProbeError(false)
    setProbeMessage('')
    try {
      const result = await probeClash5Nodes()
      setProbeMessage(t('nodePool.probeSummary', result))
      await refresh()
    } catch {
      setProbeError(true)
    } finally {
      setProbeBusy(false)
    }
  }, [refresh, t])

  const importClipboard = useCallback(async () => {
    setImportBusy(true)
    setImportError(false)
    setImportMessage('')
    delete document.documentElement.dataset.clash5ClipboardImportFailure
    let importStage = 'clipboard-read'
    try {
      const value = (await readText())?.trim() ?? ''
      importStage = 'subscription-detection'
      if (isSubscriptionUrl(value)) {
        importStage = 'subscription-import'
        await importProfile(value)
        importStage = 'refresh-after-subscription-import'
        await refresh()
        setImportMessage(t('nodePool.importSuccess'))
        return
      }

      let shareText = value
      if (!shareText.includes('://')) {
        try {
          const decoded = atob(shareText)
          if (decoded.includes('://')) shareText = decoded
        } catch {
          // Plain-text share links are handled below.
        }
      }
      if (isSubscriptionUrl(shareText)) {
        importStage = 'subscription-import'
        await importProfile(shareText)
        importStage = 'refresh-after-subscription-import'
        await refresh()
        setImportMessage(t('nodePool.importSuccess'))
        return
      }
      const lines = shareText
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
      const proxies: IProxyConfig[] = []
      importStage = 'share-link-parse'
      for (const line of lines) {
        const proxy = parseUri(line)
        if (!proxy.server || !proxy.port) {
          throw new Error('Manual proxy link has no endpoint')
        }
        proxies.push(proxy)
      }
      if (proxies.length === 0) {
        setImportError(true)
        setImportMessage(t('nodePool.importInvalid'))
        return
      }
      importStage = 'manual-proxy-import'
      const result = await importClash5ManualProxies(proxies)
      importStage = 'refresh-after-manual-import'
      await refresh()
      setImportMessage(
        t('nodePool.manualImportSuccess', { count: result.imported }),
      )
    } catch (error) {
      const errorName = error instanceof Error ? error.name : typeof error
      const rawMessage = error instanceof Error ? error.message : ''
      const errorMessage = rawMessage
        .replace(/[a-z][a-z\d+.-]*:\/\/[^\s"'<>]+/gi, '[redacted-uri]')
        .slice(0, 200)
      document.documentElement.dataset.clash5ClipboardImportFailure =
        JSON.stringify({ stage: importStage, errorName })
      console.error('[NodePool] Clipboard import failed', {
        stage: importStage,
        errorName,
        errorMessage,
      })
      setImportError(true)
      setImportMessage(t('nodePool.importError'))
    } finally {
      setImportBusy(false)
    }
  }, [refresh, t])

  const sourceName = useCallback(
    (record: Clash5NodeRecord) => {
      if (record.source.kind === 'manual') return t('nodePool.manual')
      const providerId = record.source.provider_id
      const providerMatch = /^proxy-provider:[^:]+:(.+)$/.exec(providerId)
      return providerMatch
        ? t('nodePool.proxyProvider', { name: providerMatch[1] })
        : (profileNames[providerId] ?? providerId)
    },
    [profileNames, t],
  )
  const sourceCounts = useMemo(() => {
    const counts = new Map<string, number>()
    for (const record of records.filter((node) => node.active)) {
      counts.set(record.stable_id, (counts.get(record.stable_id) ?? 0) + 1)
    }
    return counts
  }, [records])
  const visible = useMemo(() => {
    const query = search.trim().toLocaleLowerCase()
    return records
      .filter((record) => {
        const duplicate =
          record.duplicate_count > 1 ||
          (sourceCounts.get(record.stable_id) ?? 0) > 1
        if (filter === 'manual' && record.source.kind !== 'manual') return false
        if (filter === 'subscription' && record.source.kind !== 'subscription')
          return false
        if (filter === 'duplicates' && !duplicate) return false
        if (filter === 'highQuality' && record.quality?.status !== 'healthy')
          return false
        if (filter === 'inactive' && record.active) return false
        if (filter !== 'inactive' && !record.active) return false
        return (
          !query ||
          [
            record.display_name,
            record.kind,
            record.server,
            sourceName(record),
          ].some((value) => value.toLocaleLowerCase().includes(query))
        )
      })
      .sort((a, b) => {
        const rank = (record: Clash5NodeRecord) => {
          if (!record.quality) return 2_000_000
          if (record.quality.status === 'failed') return 1_000_000
          return (
            (record.quality.status === 'slow' ? 100_000 : 0) +
            (record.quality.handshake_delay_ms ?? 0)
          )
        }
        const first =
          sort === 'name'
            ? a.display_name.localeCompare(b.display_name)
            : sort === 'source'
              ? sourceName(a).localeCompare(sourceName(b))
              : rank(a) - rank(b)
        return first || a.stable_id.localeCompare(b.stable_id)
      })
  }, [records, search, filter, sort, sourceCounts, sourceName])
  const activeSelectedKey = visible.find(
    (record) => recordKey(record) === selectedKey,
  )
    ? selectedKey
    : visible[0]
      ? recordKey(visible[0])
      : null
  const selected =
    visible.find((record) => recordKey(record) === activeSelectedKey) ?? null

  const effectiveRows = selected
    ? (
        [
          ['protocol', selected.kind],
          ['server', selected.server],
          ['port', selected.port],
        ] as const
      ).filter(([, value]) => value !== null && value !== '')
    : []

  return (
    <BasePage
      header={
        <Button onClick={() => navigate('/proxies')}>
          {t('layout.components.navigation.proxyGroups')}
        </Button>
      }
      title={
        <>
          <Box component="span">{t('nodePool.title')}</Box>
          <Box
            component="span"
            sx={{
              display: 'block',
              fontSize: 11,
              fontWeight: 400,
              color: 'text.secondary',
            }}
          >
            {t('nodePool.subtitle')}
          </Box>
        </>
      }
    >
      <Stack
        spacing={1.75}
        sx={{
          p: 2,
          height: '100%',
          minHeight: 0,
          bgcolor: dark ? 'transparent' : '#f1f5fb',
        }}
      >
        <Stack
          direction={{ xs: 'column', lg: 'row' }}
          sx={{
            alignItems: { lg: 'center' },
            justifyContent: 'space-between',
            gap: 1.5,
          }}
        >
          <Stack
            direction="row"
            spacing={1}
            useFlexGap
            sx={{ alignItems: 'center', flexWrap: 'wrap' }}
          >
            <TextField
              size="small"
              placeholder={t('nodePool.search')}
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              sx={{
                width: 260,
                '& .MuiOutlinedInput-root': { borderRadius: 2 },
              }}
            />
            <Chip
              size="small"
              label={`${visible.length} ${t('nodePool.nodeCount')}`}
              sx={{
                bgcolor: dark ? 'action.selected' : '#e8f0ff',
                color: dark ? 'primary.light' : '#2f6bff',
                fontWeight: 600,
              }}
            />
            {selected && (
              <Chip
                size="small"
                label={t('nodePool.oneSelected')}
                sx={{
                  bgcolor: dark ? 'action.selected' : '#e8f9ef',
                  color: dark ? 'success.light' : '#15803d',
                  fontWeight: 600,
                }}
              />
            )}
          </Stack>
          <Stack
            direction="row"
            spacing={1}
            useFlexGap
            sx={{ flexWrap: 'wrap' }}
          >
            <Button
              variant="outlined"
              size="small"
              onClick={() => void importClipboard()}
              disabled={loading || importBusy}
            >
              {t('nodePool.importClipboard')}
            </Button>
            <Button
              variant="contained"
              size="small"
              onClick={() => void runProbe()}
              disabled={loading || probeBusy}
            >
              {t(probeBusy ? 'nodePool.probing' : 'nodePool.probe')}
            </Button>
            <Button
              variant="outlined"
              size="small"
              onClick={() => void refresh()}
              disabled={loading}
            >
              {t('nodePool.refresh')}
            </Button>
          </Stack>
        </Stack>
        <Stack
          direction="row"
          spacing={1}
          useFlexGap
          sx={{ alignItems: 'center', flexWrap: 'wrap' }}
        >
          <Select
            size="small"
            value={sort}
            onChange={(event) => setSort(event.target.value as Sort)}
            sx={{ minWidth: 170, borderRadius: 2 }}
          >
            <MenuItem value="name">{t('nodePool.sortName')}</MenuItem>
            <MenuItem value="source">{t('nodePool.sortSource')}</MenuItem>
            <MenuItem value="quality">{t('nodePool.sortQuality')}</MenuItem>
          </Select>
          {(
            [
              'all',
              'manual',
              'subscription',
              'duplicates',
              'highQuality',
              'inactive',
            ] as const
          ).map((value) => (
            <Chip
              key={value}
              label={t(`nodePool.${value}`)}
              color={filter === value ? 'primary' : 'default'}
              variant={filter === value ? 'filled' : 'outlined'}
              onClick={() => {
                setFilter(value)
                setSelectedKey(null)
              }}
            />
          ))}
        </Stack>
        {error && <Alert severity="error">{t('nodePool.error')}</Alert>}
        {importMessage && (
          <Alert severity={importError ? 'error' : 'success'}>
            {importMessage}
          </Alert>
        )}
        {probeError && (
          <Alert severity="error">{t('nodePool.probeError')}</Alert>
        )}
        {probeMessage && <Alert severity="info">{probeMessage}</Alert>}
        {loading ? (
          <Box sx={{ display: 'flex', justifyContent: 'center', p: 4 }}>
            <CircularProgress />
          </Box>
        ) : (
          <Box
            sx={{
              display: 'grid',
              gridTemplateColumns: { xs: '1fr', lg: 'minmax(0, 1fr) 320px' },
              gap: 1.75,
              alignItems: 'start',
              minHeight: 0,
            }}
          >
            <Paper
              variant="outlined"
              sx={{
                p: 1,
                borderRadius: '14px',
                borderColor: 'divider',
                boxShadow: '0 4px 16px rgba(47,107,255,.04)',
                maxHeight: '100%',
                overflowY: 'auto',
              }}
            >
              {visible.length === 0 && (
                <Alert severity="info">{t('nodePool.empty')}</Alert>
              )}
              {visible.map((record) => (
                <Box
                  component="button"
                  type="button"
                  key={recordKey(record)}
                  onClick={() => setSelectedKey(recordKey(record))}
                  aria-pressed={activeSelectedKey === recordKey(record)}
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1.25,
                    width: '100%',
                    p: 1.25,
                    mb: 0.25,
                    textAlign: 'left',
                    cursor: 'pointer',
                    border: '1px solid',
                    borderRadius: 2,
                    borderColor:
                      activeSelectedKey === recordKey(record)
                        ? dark
                          ? 'primary.main'
                          : '#c5d7ff'
                        : 'transparent',
                    bgcolor:
                      activeSelectedKey === recordKey(record)
                        ? dark
                          ? 'action.selected'
                          : '#e8f0ff'
                        : 'transparent',
                    color: 'text.primary',
                    '&:hover': {
                      bgcolor:
                        activeSelectedKey === recordKey(record)
                          ? dark
                            ? 'action.selected'
                            : '#e8f0ff'
                          : dark
                            ? 'action.hover'
                            : '#eef2f8',
                    },
                    '&:focus-visible': {
                      outline: '2px solid #2f6bff',
                      outlineOffset: 1,
                    },
                  }}
                >
                  <Box
                    aria-hidden="true"
                    sx={{
                      width: 22,
                      height: 22,
                      flexShrink: 0,
                      borderRadius: 1,
                      bgcolor: dark ? 'action.hover' : '#eef2f8',
                      display: 'grid',
                      placeItems: 'center',
                      color: dark ? 'primary.light' : '#2f6bff',
                      fontSize: 12,
                      fontWeight: 700,
                    }}
                  >
                    {record.display_name.slice(0, 1).toUpperCase()}
                  </Box>
                  <Box sx={{ minWidth: 0, flex: 1 }}>
                    <Typography sx={{ fontSize: 13.5, fontWeight: 700 }} noWrap>
                      {record.display_name}
                    </Typography>
                    <Typography
                      sx={{ fontSize: 12, color: 'text.secondary' }}
                      noWrap
                    >
                      {record.kind || t('nodePool.unknown')} ·{' '}
                      {sourceName(record)}
                    </Typography>
                  </Box>
                  <Stack
                    direction="row"
                    spacing={0.5}
                    sx={{ alignItems: 'center', flexShrink: 0 }}
                  >
                    {!record.active && (
                      <Chip
                        size="small"
                        color="warning"
                        label={t('nodePool.inactive')}
                      />
                    )}
                    {(record.duplicate_count > 1 ||
                      (sourceCounts.get(record.stable_id) ?? 0) > 1) && (
                      <Chip
                        size="small"
                        color="info"
                        label={t('nodePool.duplicates')}
                      />
                    )}
                    <Typography
                      sx={{
                        fontSize: 12,
                        fontWeight: 700,
                        color:
                          record.quality?.status === 'healthy'
                            ? dark
                              ? 'success.light'
                              : '#15803d'
                            : record.quality?.status === 'failed'
                              ? dark
                                ? 'error.light'
                                : '#b91c1c'
                              : 'text.secondary',
                        minWidth: 54,
                        textAlign: 'right',
                      }}
                    >
                      {record.quality?.handshake_delay_ms != null
                        ? t('nodePool.milliseconds', {
                            count: record.quality.handshake_delay_ms,
                          })
                        : record.quality
                          ? t(`nodePool.${record.quality.status}`)
                          : '—'}
                    </Typography>
                  </Stack>
                </Box>
              ))}
            </Paper>
            <Paper
              variant="outlined"
              sx={{
                p: 2,
                borderRadius: '14px',
                borderColor: 'divider',
                boxShadow: '0 4px 16px rgba(47,107,255,.04)',
                overflowWrap: 'anywhere',
              }}
            >
              <Stack
                direction="row"
                sx={{
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: 1,
                  mb: 1.5,
                }}
              >
                <Typography sx={{ fontSize: 13.5, fontWeight: 700 }}>
                  {t('nodePool.sourceConfig')}
                  {selected ? ` · ${selected.display_name}` : ''}
                </Typography>
                {selected && (
                  <Chip
                    size="small"
                    color={selected.active ? 'success' : 'warning'}
                    label={t(
                      selected.active ? 'nodePool.active' : 'nodePool.removed',
                    )}
                  />
                )}
              </Stack>
              {selected ? (
                <>
                  <TableContainer>
                    <Table
                      size="small"
                      aria-label={t('nodePool.sourceConfig')}
                      sx={{
                        '& .MuiTableCell-root': {
                          px: 0.75,
                          py: 1,
                          borderColor: 'divider',
                          fontSize: 12,
                        },
                      }}
                    >
                      <TableHead>
                        <TableRow>
                          <TableCell>{t('nodePool.key')}</TableCell>
                          <TableCell>{t('nodePool.value')}</TableCell>
                          <TableCell>{t('nodePool.source')}</TableCell>
                        </TableRow>
                      </TableHead>
                      <TableBody>
                        {effectiveRows.map(([label, value]) => (
                          <TableRow key={label}>
                            <TableCell component="th" scope="row">
                              {t(`nodePool.${label}`)}
                            </TableCell>
                            <TableCell>{value}</TableCell>
                            <TableCell sx={{ color: 'text.secondary' }}>
                              {sourceName(selected)}
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </TableContainer>
                  <Box
                    sx={{
                      borderTop: '1px solid',
                      borderColor: 'divider',
                      mt: 2,
                      pt: 1.5,
                      display: 'grid',
                      gridTemplateColumns: '1fr 1fr',
                      gap: 1.5,
                    }}
                  >
                    {(
                      [
                        [
                          'quality',
                          selected.quality
                            ? t(`nodePool.${selected.quality.status}`)
                            : t('nodePool.notTested'),
                        ],
                        [
                          'shortDelay',
                          selected.quality?.short_delay_ms != null
                            ? t('nodePool.milliseconds', {
                                count: selected.quality.short_delay_ms,
                              })
                            : '—',
                        ],
                        [
                          'handshakeDelay',
                          selected.quality?.handshake_delay_ms != null
                            ? t('nodePool.milliseconds', {
                                count: selected.quality.handshake_delay_ms,
                              })
                            : '—',
                        ],
                        [
                          'sourceCount',
                          sourceCounts.get(selected.stable_id) ?? 0,
                        ],
                        ['duplicateCount', selected.duplicate_count],
                      ] as const
                    ).map(([label, value]) => (
                      <Box key={label}>
                        <Typography
                          sx={{ fontSize: 11, color: 'text.secondary' }}
                        >
                          {t(`nodePool.${label}`)}
                        </Typography>
                        <Typography sx={{ fontSize: 12.5, fontWeight: 600 }}>
                          {value}
                        </Typography>
                      </Box>
                    ))}
                  </Box>
                  <Typography
                    sx={{
                      mt: 2,
                      fontSize: 10,
                      color: 'text.secondary',
                      overflowWrap: 'anywhere',
                    }}
                  >
                    {t('nodePool.stableId')}: {selected.stable_id}
                  </Typography>
                </>
              ) : (
                <Typography color="text.secondary">
                  {t('nodePool.select')}
                </Typography>
              )}
            </Paper>
          </Box>
        )}
      </Stack>
    </BasePage>
  )
}

export default NodePoolPage
