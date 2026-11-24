import { RouterOutlined, SpeedOutlined } from '@mui/icons-material'
import { Grid, Skeleton } from '@mui/material'
import { Suspense, lazy } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import { ClashModeCard } from '@/components/home/clash-mode-card'
import { EnhancedCard } from '@/components/home/enhanced-card'
import { EnhancedTrafficStats } from '@/components/home/enhanced-traffic-stats'

const preloadIpInfoCard = () =>
  import('@/components/home/ip-info-card').then((module) => ({
    default: module.IpInfoCard,
  }))
const preloadClashInfoCard = () =>
  import('@/components/home/clash-info-card').then((module) => ({
    default: module.ClashInfoCard,
  }))
const preloadSystemInfoCard = () =>
  import('@/components/home/system-info-card').then((module) => ({
    default: module.SystemInfoCard,
  }))

const LazyIpInfoCard = lazy(preloadIpInfoCard)
const LazyClashInfoCard = lazy(preloadClashInfoCard)
const LazySystemInfoCard = lazy(preloadSystemInfoCard)

// Used by bootstrap to initiate optional card imports without blocking render.
// eslint-disable-next-line react-refresh/only-export-components
export const preloadHomeDetailCards = () =>
  Promise.all([
    preloadIpInfoCard().catch(() => {}),
    preloadClashInfoCard().catch(() => {}),
    preloadSystemInfoCard().catch(() => {}),
  ])

const cardFallback = <Skeleton variant="rectangular" height={200} />

/**
 * Everything the dashboard used to carry beyond its five cards: traffic totals,
 * IP and core identity, the privileged-service state, and the proxy mode
 * switcher. Kept on its own tab so the dashboard stays fixed without losing the
 * service install/repair controls.
 */
const HomeDetailsPage = () => {
  const { t } = useTranslation()

  return (
    <BasePage title={t('home.page.tabs.details')} contentStyle={{ padding: 2 }}>
      <Grid container spacing={1.5} columns={{ xs: 6, sm: 6, md: 12 }}>
        <Grid size={12}>
          <EnhancedCard
            title={t('home.page.cards.trafficStats')}
            icon={<SpeedOutlined />}
            iconColor="secondary"
          >
            <EnhancedTrafficStats />
          </EnhancedCard>
        </Grid>
        <Grid size={6}>
          <Suspense fallback={cardFallback}>
            <LazyIpInfoCard />
          </Suspense>
        </Grid>
        <Grid size={6}>
          <Suspense fallback={cardFallback}>
            <LazyClashInfoCard />
          </Suspense>
        </Grid>
        <Grid size={6}>
          <Suspense fallback={cardFallback}>
            <LazySystemInfoCard />
          </Suspense>
        </Grid>
        <Grid size={6}>
          <EnhancedCard
            title={t('home.page.cards.proxyMode')}
            icon={<RouterOutlined />}
            iconColor="info"
            action={null}
          >
            <ClashModeCard />
          </EnhancedCard>
        </Grid>
      </Grid>
    </BasePage>
  )
}

export default HomeDetailsPage
