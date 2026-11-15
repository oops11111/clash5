import { AppsOutlined } from '@mui/icons-material'
import { Box, Skeleton, Typography, alpha, useTheme } from '@mui/material'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { EnhancedCard } from '@/components/home/enhanced-card'
import { useConnectionData } from '@/hooks/use-connection-data'

interface ProcessEntry {
  process: string
  processPath: string
  lastSeen: number
  connectionCount: number
}

/**
 * Derive a deduplicated, recency-sorted list of processes from active and
 * closed connection metadata. The `start` field is an ISO timestamp; we parse
 * it to rank by most-recent activity.
 */
const deriveProcessList = (
  connections: IConnectionsItem[],
): ProcessEntry[] => {
  const map = new Map<string, ProcessEntry>()

  for (const conn of connections) {
    const process = conn.metadata.process?.trim()
    if (!process) continue

    const processPath = conn.metadata.processPath?.trim() || ''
    const key = processPath ? `${process}|${processPath}` : process
    const startTime = Date.parse(conn.start) || 0
    const existing = map.get(key)

    if (existing) {
      existing.connectionCount += 1
      if (startTime > existing.lastSeen) existing.lastSeen = startTime
    } else {
      map.set(key, {
        process,
        processPath,
        lastSeen: startTime,
        connectionCount: 1,
      })
    }
  }

  return Array.from(map.values()).sort(
    (a, b) => b.lastSeen - a.lastSeen,
  )
}

const formatRelativeTime = (timestamp: number): string => {
  if (!timestamp) return ''
  const diff = Date.now() - timestamp
  if (diff < 60_000) return 'just now'
  const minutes = Math.floor(diff / 60_000)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  return `${Math.floor(hours / 24)}d ago`
}

/** A read-only list of processes that recently had traffic through the core. */
export const PerAppProxyCard = () => {
  const { t } = useTranslation()
  const theme = useTheme()
  const { response } = useConnectionData()

  const processList = useMemo(
    () =>
      deriveProcessList([
        ...(response.data?.activeConnections ?? []),
        ...(response.data?.closedConnections ?? []),
      ]),
    [response.data],
  )

  const hasData = processList.length > 0

  return (
    <EnhancedCard
      title={t('home.page.cards.perAppProxy')}
      icon={<AppsOutlined />}
      iconColor="primary"
      action={null}
    >
      {!hasData ? (
        <Box
          sx={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            minHeight: 120,
          }}
        >
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
            maxHeight: 280,
            overflowY: 'auto',
          }}
        >
          {processList.map((entry) => (
            <Box
              key={`${entry.process}|${entry.processPath}`}
              sx={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                py: 0.75,
                px: 1,
                borderRadius: 1,
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
                  {entry.process}
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
              </Box>
              <Box
                sx={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 1,
                  flexShrink: 0,
                  ml: 1,
                }}
              >
                <Typography variant="caption" color="text.secondary">
                  {entry.connectionCount}
                </Typography>
                {entry.lastSeen > 0 && (
                  <Typography variant="caption" color="text.disabled">
                    {formatRelativeTime(entry.lastSeen)}
                  </Typography>
                )}
              </Box>
            </Box>
          ))}
        </Box>
      )}
    </EnhancedCard>
  )
}

// Re-export the skeleton for lazy-loading fallbacks.
export const PerAppProxyCardSkeleton = () => (
  <Skeleton variant="rectangular" height={200} />
)
