import {
  DnsOutlined,
  HelpOutlineRounded,
  HistoryEduOutlined,
} from '@mui/icons-material'
import { Alert, Box, Grid, IconButton, Tooltip } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import { CurrentProxyCard } from '@/components/home/current-proxy-card'
import { EnhancedCard } from '@/components/home/enhanced-card'
import { FirstRunEmptyState } from '@/components/home/first-run-empty-state'
import { shouldShowFirstRunEmptyState } from '@/components/home/first-run-empty-state-model'
import { ProxyTunCard } from '@/components/home/proxy-tun-card'
import { QualityNodesCard } from '@/components/home/quality-nodes-card'
import { entry_lightweight_mode, listClash5Nodes } from '@/services/cmds'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'
import { openExternalUrl } from '@/utils/open-external-url'

const HomePage = () => {
  const { t } = useTranslation()
  const [nodeInventory, setNodeInventory] = useState<
    { active: boolean }[] | null
  >(null)
  const [nodeInventoryError, setNodeInventoryError] = useState(false)

  useEffect(() => {
    let cancelled = false
    listClash5Nodes()
      .then((nodes) => {
        if (!cancelled) setNodeInventory(nodes)
      })
      .catch(() => {
        if (!cancelled) setNodeInventoryError(true)
      })
    return () => {
      cancelled = true
    }
  }, [])

  const toGithubDoc = useLockFn(() =>
    openExternalUrl('https://clash-verge-rev.github.io/index.html').catch(
      showNotice.error,
    ),
  )

  return (
    <BasePage
      title={t('home.page.title')}
      contentStyle={{ padding: 2 }}
      header={
        <Box
          data-testid="home-header-actions"
          sx={{ display: 'flex', alignItems: 'center' }}
        >
          <Tooltip title={t('home.page.tooltips.lightweightMode')} arrow>
            <IconButton
              data-testid="home-action-lightweight"
              onClick={async () =>
                await mutate(() => entry_lightweight_mode(), {
                  id: 'entry-lightweight-mode',
                  errorNotice: false,
                })
              }
              size="small"
              color="inherit"
            >
              <HistoryEduOutlined />
            </IconButton>
          </Tooltip>
          <Tooltip title={t('home.page.tooltips.manual')} arrow>
            <IconButton
              data-testid="home-action-manual"
              onClick={toGithubDoc}
              size="small"
              color="inherit"
            >
              <HelpOutlineRounded />
            </IconButton>
          </Tooltip>
        </Box>
      }
    >
      {shouldShowFirstRunEmptyState(nodeInventory, nodeInventoryError) ? (
        <FirstRunEmptyState />
      ) : (
        <Grid
          container
          spacing={1.5}
          columns={12}
          data-testid="home-fixed-dashboard"
        >
          <Grid
            size={{ xs: 12, md: 6 }}
            data-testid="home-card-system-proxy"
            sx={{ minHeight: 220 }}
          >
            <SystemProxyCard />
          </Grid>
          <Grid
            size={{ xs: 12, md: 6 }}
            data-testid="home-card-proxy-groups"
            sx={{ minHeight: 220 }}
          >
            <CurrentProxyCard />
          </Grid>
          <Grid
            size={{ xs: 12, md: 6 }}
            data-testid="home-card-quality-nodes"
            sx={{ minHeight: 220 }}
          >
            <QualityNodesCard />
          </Grid>
          <Grid
            size={{ xs: 12, md: 6 }}
            data-testid="home-card-dns-leak"
            sx={{ minHeight: 220 }}
          >
            <DnsLeakCard />
          </Grid>
        </Grid>
      )}
    </BasePage>
  )
}

const SystemProxyCard = () => {
  const { t } = useTranslation()
  return (
    <EnhancedCard
      title={t('home.page.cards.systemProxy')}
      icon={<DnsOutlined />}
      iconColor="primary"
      action={null}
      minHeight={220}
    >
      <ProxyTunCard />
    </EnhancedCard>
  )
}

const DnsLeakCard = () => {
  const { t } = useTranslation()
  return (
    <EnhancedCard
      title={t('home.page.cards.dnsLeak')}
      icon={<DnsOutlined />}
      iconColor="info"
      minHeight={180}
    >
      <Box sx={{ display: 'flex', flex: 1, alignItems: 'center' }}>
        <Alert severity="info" sx={{ width: '100%' }}>
          {t('home.page.dnsLeakUnavailable')}
        </Alert>
      </Box>
    </EnhancedCard>
  )
}

export default HomePage
