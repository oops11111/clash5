import { SpeedOutlined } from '@mui/icons-material'
import {
  Alert,
  Box,
  Button,
  Chip,
  Skeleton,
  Stack,
  Typography,
} from '@mui/material'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { EnhancedCard } from '@/components/home/enhanced-card'
import { listClash5Nodes, probeClash5Nodes } from '@/services/cmds'
import type { Clash5NodeRecord } from '@/types/clash5-node'

const qualityOrder = { healthy: 0, slow: 1, failed: 2 } as const
const qualityStatusKey = {
  healthy: 'nodePool.healthy',
  slow: 'nodePool.slow',
  failed: 'nodePool.failed',
} as const

type QualityNode = Clash5NodeRecord & {
  quality: NonNullable<Clash5NodeRecord['quality']>
}

const hasQuality = (node: Clash5NodeRecord): node is QualityNode =>
  node.quality !== null

const nodeDelay = (node: Clash5NodeRecord) =>
  node.quality?.short_delay_ms ?? node.quality?.handshake_delay_ms ?? null

export const QualityNodesCard = () => {
  const { t } = useTranslation()
  const [nodes, setNodes] = useState<Clash5NodeRecord[] | null>(null)
  const [loading, setLoading] = useState(true)
  const [probing, setProbing] = useState(false)
  const [error, setError] = useState<'load' | 'probe' | null>(null)

  const refresh = useCallback(async () => {
    setError(null)
    try {
      setNodes(await listClash5Nodes())
    } catch {
      setError('load')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const topNodes = useMemo(
    () =>
      (nodes ?? [])
        .filter((node): node is QualityNode => node.active && hasQuality(node))
        .sort((left, right) => {
          const statusDelta =
            qualityOrder[left.quality.status] -
            qualityOrder[right.quality.status]
          if (statusDelta !== 0) return statusDelta
          return (
            (nodeDelay(left) ?? Number.MAX_SAFE_INTEGER) -
            (nodeDelay(right) ?? Number.MAX_SAFE_INTEGER)
          )
        })
        .slice(0, 2),
    [nodes],
  )

  const runProbe = async () => {
    setProbing(true)
    setError(null)
    try {
      await probeClash5Nodes()
      try {
        setNodes(await listClash5Nodes())
      } catch {
        setError('load')
      }
    } catch {
      setError('probe')
    } finally {
      setLoading(false)
      setProbing(false)
    }
  }

  return (
    <EnhancedCard
      title={t('home.page.cards.qualityNodes')}
      icon={<SpeedOutlined />}
      iconColor="success"
      action={
        <Button size="small" disabled={loading || probing} onClick={runProbe}>
          {probing ? t('nodePool.probing') : t('nodePool.probe')}
        </Button>
      }
      minHeight={180}
    >
      {loading ? (
        <Stack spacing={1}>
          <Skeleton height={36} />
          <Skeleton height={36} />
        </Stack>
      ) : error ? (
        <Alert severity="error">
          {error === 'load' ? t('nodePool.error') : t('nodePool.probeError')}
        </Alert>
      ) : topNodes.length > 0 ? (
        <Stack spacing={1}>
          {topNodes.map((node) => {
            const delay = nodeDelay(node)
            return (
              <Box
                key={`${node.source.kind}:${node.source.kind === 'subscription' ? node.source.provider_id : ''}:${node.stable_id}`}
                sx={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: 1,
                  minWidth: 0,
                }}
              >
                <Typography variant="body2" noWrap title={node.display_name}>
                  {node.display_name}
                </Typography>
                <Stack
                  direction="row"
                  spacing={0.75}
                  sx={{ alignItems: 'center' }}
                >
                  <Chip
                    size="small"
                    color={
                      node.quality.status === 'healthy'
                        ? 'success'
                        : node.quality.status === 'slow'
                          ? 'warning'
                          : 'error'
                    }
                    label={t(qualityStatusKey[node.quality.status])}
                  />
                  {delay !== null && (
                    <Typography variant="caption" color="text.secondary">
                      {t('nodePool.milliseconds', { count: delay })}
                    </Typography>
                  )}
                </Stack>
              </Box>
            )
          })}
        </Stack>
      ) : (
        <Typography variant="body2" color="text.secondary">
          {t('nodePool.notTested')}
        </Typography>
      )}
    </EnhancedCard>
  )
}
