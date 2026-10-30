import { DnsOutlined, RefreshRounded } from '@mui/icons-material'
import { Box, IconButton, Stack, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { runClash5DnsCheck } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import type { DnsCheckReport } from '@/services/tauri-commands.generated'

import { EnhancedCard } from './enhanced-card'

// Summary of the local DNS leak check; the full report lives on the diagnostics page.
export const DnsCheckCard = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [report, setReport] = useState<DnsCheckReport | null>(null)

  const run = useLockFn(async () => {
    try {
      setReport(await runClash5DnsCheck())
    } catch (err) {
      showNotice.error(err)
    }
  })

  useEffect(() => {
    void run()
  }, [run])

  const failing =
    report?.checks.filter((check) => check.status === 'fail') ?? []
  const checked = report?.checks.some((check) => check.status !== 'unsupported')

  const status = !report
    ? t('home.components.dnsCheck.status.checking')
    : failing.length > 0
      ? t('home.components.dnsCheck.status.leak', {
          count: failing.reduce(
            (total, check) => total + check.details.length,
            0,
          ),
        })
      : checked
        ? t('home.components.dnsCheck.status.clean')
        : t('home.components.dnsCheck.status.unsupported')

  return (
    <EnhancedCard
      title={t('home.components.dnsCheck.title')}
      icon={<DnsOutlined />}
      iconColor={failing.length > 0 ? 'error' : 'success'}
      action={
        <IconButton
          size="small"
          title={t('home.components.dnsCheck.actions.refresh')}
          onClick={() => void run()}
        >
          <RefreshRounded fontSize="small" />
        </IconButton>
      }
    >
      <Stack spacing={1} sx={{ p: 1 }}>
        <Typography variant="body2" color="text.secondary">
          {status}
        </Typography>
        {report && (
          <Box>
            <Typography variant="caption" color="text.secondary">
              {t('home.components.dnsCheck.primary')}:{' '}
              {report.resolvers.nameserver[0] ?? '-'}
            </Typography>
          </Box>
        )}
        <Typography
          variant="caption"
          color="primary"
          sx={{ cursor: 'pointer', alignSelf: 'flex-start' }}
          onClick={() => navigate('/diagnostics')}
        >
          {t('home.components.dnsCheck.actions.details')}
        </Typography>
      </Stack>
    </EnhancedCard>
  )
}
