import { Tab, Tabs } from '@mui/material'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { DnsCheckPanel } from '@/components/diagnostics/dns-check-panel'

import ConnectionsPage from './connections'
import LogsPage from './logs'
import UnlockPage from './unlock'

type View = 'connections' | 'logs' | 'unlock' | 'dns'

// Runtime diagnostics in one place: live connections, logs, unlock and DNS checks.
const DiagnosticsPage = () => {
  const { t } = useTranslation()
  const [view, setView] = useState<View>('connections')

  return (
    <>
      <Tabs
        value={view}
        onChange={(_, value: View) => setView(value)}
        sx={{ px: 2 }}
      >
        <Tab value="connections" label={t('connections.page.title')} />
        <Tab value="logs" label={t('logs.page.title')} />
        <Tab value="unlock" label={t('tests.unlock.page.title')} />
        <Tab value="dns" label={t('tests.dns.title')} />
      </Tabs>
      {view === 'connections' && <ConnectionsPage />}
      {view === 'logs' && <LogsPage />}
      {view === 'unlock' && <UnlockPage />}
      {view === 'dns' && <DnsCheckPanel />}
    </>
  )
}

export default DiagnosticsPage
