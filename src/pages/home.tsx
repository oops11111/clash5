import {
  DnsOutlined,
  HelpOutlineRounded,
  HistoryEduOutlined,
} from '@mui/icons-material'
import { Box, Grid, IconButton, Skeleton, Tooltip } from '@mui/material'
import { useLockFn } from 'ahooks'
import { Suspense, lazy } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import { CurrentProxyCard } from '@/components/home/current-proxy-card'
import { DnsLeakCard } from '@/components/home/dns-leak-card'
import { EnhancedCard } from '@/components/home/enhanced-card'
import { PerAppProxyCard } from '@/components/home/per-app-proxy-card'
import { ProxyTunCard } from '@/components/home/proxy-tun-card'
import { entry_lightweight_mode } from '@/services/cmds'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'
import { openExternalUrl } from '@/utils/open-external-url'

const preloadTestCard = () =>
  import('@/components/home/test-card').then((module) => ({
    default: module.TestCard,
  }))

const LazyTestCard = lazy(preloadTestCard)

// Used by bootstrap to initiate optional card imports without blocking render.
// eslint-disable-next-line react-refresh/only-export-components
export const preloadHomePageCards = () => preloadTestCard().catch(() => {})

const cardFallback = <Skeleton variant="rectangular" height={200} />

const HomePage = () => {
  const { t } = useTranslation()

  // 文档链接函数 - point to clash5's own docs, not upstream
  const toGithubDoc = useLockFn(() =>
    openExternalUrl('https://github.com/oops11111/clash5#readme').catch(
      showNotice.error,
    ),
  )

  return (
    <BasePage
      title={t('home.page.title')}
      contentStyle={{ padding: 2 }}
      header={
        <Box sx={{ display: 'flex', alignItems: 'center' }}>
          <Tooltip title={t('home.page.tooltips.lightweightMode')} arrow>
            <IconButton
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
            <IconButton onClick={toGithubDoc} size="small" color="inherit">
              <HelpOutlineRounded />
            </IconButton>
          </Tooltip>
        </Box>
      }
    >
      {/* Fixed by design: the dashboard shows the same five cards for everyone. */}
      <Grid container spacing={1.5} columns={{ xs: 6, sm: 6, md: 12 }}>
        <Grid size={6}>
          <NetworkSettingsCard />
        </Grid>
        <Grid size={6}>
          <CurrentProxyCard />
        </Grid>
        <Grid size={6}>
          <Suspense fallback={cardFallback}>
            <LazyTestCard />
          </Suspense>
        </Grid>
        <Grid size={6}>
          <DnsLeakCard />
        </Grid>
        <Grid size={12}>
          <PerAppProxyCard />
        </Grid>
      </Grid>
    </BasePage>
  )
}

// 增强版网络设置卡片组件
const NetworkSettingsCard = () => {
  const { t } = useTranslation()
  return (
    <EnhancedCard
      title={t('home.page.cards.networkSettings')}
      icon={<DnsOutlined />}
      iconColor="primary"
      action={null}
    >
      <ProxyTunCard />
    </EnhancedCard>
  )
}

export default HomePage
