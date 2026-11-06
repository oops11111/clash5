import { Box, Tab, Tabs, Typography } from '@mui/material'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { DnsCheckPanel } from '@/components/diagnostics/dns-check-panel'

import ConnectionsPage from './connections'
import LogsPage from './logs'
import UnlockPage from './unlock'

type View = 'connections' | 'logs' | 'unlock' | 'dns'

interface DiagnosticsPageProps {
  initialView?: View
}

// Runtime diagnostics in one place: live connections, logs, unlock and DNS checks.
const DiagnosticsPage = ({
  initialView = 'connections',
}: DiagnosticsPageProps) => {
  const { t } = useTranslation()
  const [view, setView] = useState<View>(initialView)

  return (
    <Box
      sx={{
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        minHeight: 0,
      }}
    >
      <Box sx={{ px: 2, pt: 2, pb: 1 }}>
        <Typography variant="subtitle1" sx={{ fontWeight: 700 }}>
          {t('settings.page.groups.diagnostics')}
        </Typography>
        <Typography variant="body2" color="text.secondary">
          {t('settings.page.diagnostics.description')}
        </Typography>
      </Box>
      <Tabs
        value={view}
        onChange={(_, value: View) => setView(value)}
        sx={{ px: 2, flexShrink: 0 }}
      >
        <Tab value="connections" label={t('connections.page.title')} />
        <Tab value="logs" label={t('logs.page.title')} />
        <Tab value="unlock" label={t('tests.unlock.page.title')} />
        <Tab value="dns" label={t('tests.dns.title')} />
      </Tabs>
      <Box sx={{ flex: 1, minHeight: 0, overflow: 'hidden' }}>
        {view === 'connections' && <ConnectionsPage embedded />}
        {view === 'logs' && <LogsPage embedded />}
        {view === 'unlock' && <UnlockPage embedded />}
        {view === 'dns' && <DnsCheckPanel />}
      </Box>
    </Box>
  )
}

export default DiagnosticsPage
