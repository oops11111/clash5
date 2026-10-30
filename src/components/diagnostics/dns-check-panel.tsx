import { Box, Button, Chip, Paper, Stack, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { runClash5DnsCheck, runClash5DnsSamples } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import type {
  DnsCheckReport,
  DnsSample,
  LeakCheck,
} from '@/services/tauri-commands.generated'

const STATUS_COLOR = {
  pass: 'success',
  fail: 'error',
  unsupported: 'default',
} as const

const joinOrDash = (values: string[]) =>
  values.length > 0 ? values.join(', ') : '—'

const Row = ({ label, value }: { label: string; value: string }) => (
  <Stack
    direction="row"
    spacing={2}
    sx={{ justifyContent: 'space-between', py: 0.75 }}
  >
    <Typography variant="body2" color="text.secondary">
      {label}
    </Typography>
    <Typography variant="body2" sx={{ textAlign: 'right', minWidth: 0 }}>
      {value}
    </Typography>
  </Stack>
)

export const DnsCheckPanel = () => {
  const { t } = useTranslation()
  const [report, setReport] = useState<DnsCheckReport | null>(null)
  const [samples, setSamples] = useState<DnsSample[] | null>(null)

  const run = useLockFn(async () => {
    try {
      setReport(await runClash5DnsCheck())
    } catch (err) {
      showNotice.error(err)
    }
  })

  // Samples are slower than the leak check, so they load after it and never block it.
  const sample = useLockFn(async () => {
    try {
      setSamples(await runClash5DnsSamples())
    } catch (err) {
      showNotice.error(err)
    }
  })

  useEffect(() => {
    void run()
    void sample()
  }, [run, sample])

  const checkDetail = (check: LeakCheck) => {
    if (check.status === 'fail') return check.details.join(', ')
    if (check.status === 'unsupported') return check.details[0] ?? ''
    return t('tests.dns.checks.passDetail')
  }

  return (
    <Box sx={{ p: 2, display: 'flex', flexDirection: 'column', gap: 2 }}>
      <Stack
        direction="row"
        spacing={1}
        sx={{ justifyContent: 'space-between', alignItems: 'center' }}
      >
        <Box>
          <Typography variant="h6">{t('tests.dns.title')}</Typography>
          <Typography variant="body2" color="text.secondary">
            {t('tests.dns.hint')}
          </Typography>
        </Box>
        <Button
          variant="contained"
          onClick={() => {
            void run()
            void sample()
          }}
        >
          {t('tests.dns.rerun')}
        </Button>
      </Stack>

      {report && (
        <>
          <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
            <Typography sx={{ fontWeight: 600, mb: 1 }}>
              {t('tests.dns.resolvers.title')}
            </Typography>
            <Row
              label={t('tests.dns.resolvers.nameserver')}
              value={joinOrDash(report.resolvers.nameserver)}
            />
            <Row
              label={t('tests.dns.resolvers.fallback')}
              value={joinOrDash(report.resolvers.fallback)}
            />
            <Row
              label={t('tests.dns.resolvers.defaultNameserver')}
              value={joinOrDash(report.resolvers.default_nameserver)}
            />
            <Row
              label={t('tests.dns.resolvers.listen')}
              value={report.resolvers.listen ?? '—'}
            />
          </Paper>

          <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
            <Typography sx={{ fontWeight: 600 }}>
              {t('tests.dns.checks.title')}
            </Typography>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
              {t('tests.dns.checks.hint')}
            </Typography>
            {report.checks.map((check) => (
              <Stack
                key={check.id}
                direction="row"
                spacing={2}
                sx={{
                  py: 1,
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  borderTop: 1,
                  borderColor: 'divider',
                }}
              >
                <Box sx={{ minWidth: 0 }}>
                  <Typography variant="body2" sx={{ fontWeight: 600 }}>
                    {check.id === 'system_resolver'
                      ? t('tests.dns.checks.system_resolver')
                      : t('tests.dns.checks.plaintext_dns')}
                  </Typography>
                  <Typography
                    variant="caption"
                    color="text.secondary"
                    sx={{ wordBreak: 'break-all' }}
                  >
                    {checkDetail(check)}
                  </Typography>
                </Box>
                <Chip
                  size="small"
                  color={STATUS_COLOR[check.status]}
                  variant="outlined"
                  label={t(`tests.dns.status.${check.status}`)}
                />
              </Stack>
            ))}
          </Paper>

          <Paper variant="outlined" sx={{ p: 2, borderRadius: 3 }}>
            <Typography sx={{ fontWeight: 600 }}>
              {t('tests.dns.samples.title')}
            </Typography>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
              {t('tests.dns.samples.hint')}
            </Typography>
            {(samples ?? []).map((item) => (
              <Stack
                key={item.domain}
                direction="row"
                spacing={2}
                sx={{
                  py: 1,
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  borderTop: 1,
                  borderColor: 'divider',
                }}
              >
                <Typography
                  variant="body2"
                  sx={{ minWidth: 0, wordBreak: 'break-all' }}
                >
                  {item.domain}
                </Typography>
                <Typography
                  variant="body2"
                  color={item.error ? 'error' : 'text.primary'}
                  sx={{ textAlign: 'right' }}
                >
                  {item.error ?? item.answer ?? t('tests.dns.samples.noAnswer')}
                </Typography>
                <Typography variant="caption" color="text.secondary">
                  {t('tests.dns.samples.duration', { ms: item.duration_ms })}
                </Typography>
              </Stack>
            ))}
            {samples === null && (
              <Typography variant="body2" color="text.secondary">
                {t('tests.dns.samples.loading')}
              </Typography>
            )}
          </Paper>

          <Typography variant="caption" color="text.secondary">
            {t('tests.dns.checkedAt', {
              time: new Date(report.checked_at).toLocaleString(),
            })}
          </Typography>
        </>
      )}
    </Box>
  )
}
