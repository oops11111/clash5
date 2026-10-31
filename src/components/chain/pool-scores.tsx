import { Stack, Typography } from '@mui/material'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { getClash5PoolScores } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import type { PoolMemberScore } from '@/services/tauri-commands.generated'

interface Props {
  poolId: string
  // Changes when the workspace reloads, so the ranking follows membership and probe updates.
  refreshKey: unknown
}

// Members of a node pool, best first, with the latency and failure record behind each score.
export const PoolScoreList = ({ poolId, refreshKey }: Props) => {
  const { t } = useTranslation()
  const [rows, setRows] = useState<PoolMemberScore[] | null>(null)

  useEffect(() => {
    let cancelled = false
    getClash5PoolScores(poolId)
      .then((result) => {
        if (!cancelled) setRows(result)
      })
      .catch((err) => {
        if (!cancelled) {
          setRows([])
          showNotice.error(err)
        }
      })
    return () => {
      cancelled = true
    }
  }, [poolId, refreshKey])

  if (rows === null) return null
  if (rows.length === 0) {
    return (
      <Typography variant="caption" color="text.secondary">
        {t('chains.poolScores.empty')}
      </Typography>
    )
  }

  return (
    <Stack spacing={0.5} sx={{ mt: 1.5 }}>
      <Typography variant="caption" color="text.secondary">
        {t('chains.poolScores.hint')}
      </Typography>
      {rows.map((row, index) => (
        <Stack
          key={`${row.display_name}-${index}`}
          direction="row"
          spacing={1}
          sx={{ alignItems: 'center', justifyContent: 'space-between' }}
        >
          <Typography variant="body2" noWrap sx={{ minWidth: 0 }}>
            {row.rank !== null ? `${row.rank}. ` : ''}
            {row.display_name}
          </Typography>
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ flexShrink: 0 }}
          >
            {row.failed
              ? t('chains.poolScores.failed')
              : row.delay_ms !== null
                ? `${row.delay_ms} ms`
                : t('chains.poolScores.unmeasured')}
            {row.score !== null ? ` · ${row.score.toFixed(2)}` : ''}
          </Typography>
        </Stack>
      ))}
    </Stack>
  )
}
