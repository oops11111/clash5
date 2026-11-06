import {
  DnsOutlined,
  HelpOutlineRounded,
  HistoryEduOutlined,
  RouterOutlined,
  SettingsOutlined,
} from '@mui/icons-material'
import {
  Box,
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Grid,
  IconButton,
  MenuItem,
  Select,
  Skeleton,
  Stack,
  ToggleButton,
  ToggleButtonGroup,
  Tooltip,
  Typography,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import { Suspense, lazy, useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSearchParams } from 'react-router'

import { BasePage } from '@/components/base'
import { HomeChainCard } from '@/components/home/chain-card'
import { ClashModeCard } from '@/components/home/clash-mode-card'
import { CurrentProxyCard } from '@/components/home/current-proxy-card'
import { EnhancedCard } from '@/components/home/enhanced-card'
import { FirstRunEmptyState } from '@/components/home/first-run-empty-state'
import { HomeProfileCard } from '@/components/home/home-profile-card'
import { ProxyTunCard } from '@/components/home/proxy-tun-card'
import { ThemeModeSwitch } from '@/components/setting/mods/theme-mode-switch'
import ProxyControlSwitches from '@/components/shared/proxy-control-switches'
import { useProfiles } from '@/hooks/use-profiles'
import { useVerge } from '@/hooks/use-verge'
import { primaryNavigationItems } from '@/pages/_navigation-meta'
import { entry_lightweight_mode, listClash5Nodes } from '@/services/cmds'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'
import { openExternalUrl } from '@/utils/open-external-url'

import ChainsPage from './chains'

const LazyDnsCheckCard = lazy(() =>
  import('@/components/home/dns-check-card').then((module) => ({
    default: module.DnsCheckCard,
  })),
)

// Used by bootstrap to initiate optional card imports without blocking render.
// eslint-disable-next-line react-refresh/only-export-components
export const preloadHomePageCards = () =>
  import('@/components/home/dns-check-card').catch(() => {})

// 首页设置对话框组件接口
interface HomeSettingsDialogProps {
  onClose: () => void
}

// 首页设置对话框组件
// Accent colors offered in the home settings; the choice is saved as the theme's primary color.
const ACCENT_COLORS = ['#2F6BFF', '#8B5CF6', '#22C55E', '#F59E0B']

const HomeSettingsDialog = ({
  onClose,
}: HomeSettingsDialogProps) => {
  const { t } = useTranslation()
  const { verge, patchVerge } = useVerge()
  const accent = verge?.theme_setting?.primary_color

  return (
    <Dialog open onClose={onClose} maxWidth="xs" fullWidth>
      <DialogTitle>{t('home.page.settings.title')}</DialogTitle>
      <DialogContent>
        <Typography variant="overline" color="text.secondary">
          {t('home.page.settings.appearance')}
        </Typography>
        <Stack spacing={1} sx={{ mb: 2 }}>
          <ThemeModeSwitch
            value={verge?.theme_mode}
            onChange={(mode) => void patchVerge({ theme_mode: mode })}
          />
          <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
            <Typography variant="body2">
              {t('home.page.settings.accent')}
            </Typography>
            {ACCENT_COLORS.map((color) => (
              <IconButton
                key={color}
                size="small"
                aria-label={t('home.page.settings.accent')}
                onClick={() =>
                  void patchVerge({
                    theme_setting: {
                      ...verge?.theme_setting,
                      primary_color: color,
                    },
                  })
                }
                sx={{
                  width: 24,
                  height: 24,
                  bgcolor: color,
                  border: accent === color ? '2px solid' : 'none',
                  borderColor: 'text.primary',
                  '&:hover': { bgcolor: color },
                }}
              />
            ))}
          </Stack>
        </Stack>
        <Typography variant="overline" color="text.secondary">
          {t('home.page.settings.refresh.title')}
        </Typography>
        <Stack spacing={1.5} sx={{ mb: 2 }}>
          <Stack
            direction="row"
            spacing={1}
            sx={{ alignItems: 'center', justifyContent: 'space-between' }}
          >
            <Stack>
              <Typography variant="body2">
                {t('home.page.settings.refresh.trafficInterval')}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                {t('home.page.settings.refresh.trafficIntervalHint')}
              </Typography>
            </Stack>
            <ToggleButtonGroup
              exclusive
              size="small"
              aria-label={t('home.page.settings.refresh.trafficInterval')}
              value={verge?.traffic_refresh_seconds ?? 2}
              onChange={(_, seconds: number | null) => {
                if (seconds !== null)
                  void patchVerge({ traffic_refresh_seconds: seconds })
              }}
            >
              {[1, 2, 5].map((seconds) => (
                <ToggleButton key={seconds} value={seconds}>
                  {t('home.page.settings.refresh.seconds', { count: seconds })}
                </ToggleButton>
              ))}
            </ToggleButtonGroup>
          </Stack>
          <Stack
            direction="row"
            spacing={1}
            sx={{ alignItems: 'center', justifyContent: 'space-between' }}
          >
            <Stack>
              <Typography variant="body2">
                {t('home.page.settings.refresh.ipAuto')}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                {t('home.page.settings.refresh.ipAutoHint')}
              </Typography>
            </Stack>
            <ToggleButtonGroup
              exclusive
              size="small"
              aria-label={t('home.page.settings.refresh.ipAuto')}
              value={verge?.ip_auto_refresh_minutes ?? 0}
              onChange={(_, minutes: number | null) => {
                if (minutes !== null)
                  void patchVerge({ ip_auto_refresh_minutes: minutes })
              }}
            >
              <ToggleButton value={0}>
                {t('home.page.settings.refresh.off')}
              </ToggleButton>
              {[5, 30].map((minutes) => (
                <ToggleButton key={minutes} value={minutes}>
                  {t('home.page.settings.refresh.minutes', { count: minutes })}
                </ToggleButton>
              ))}
            </ToggleButtonGroup>
          </Stack>
        </Stack>
        <Typography variant="overline" color="text.secondary">
          {t('home.page.settings.startup.title')}
        </Typography>
        <Stack
          direction="row"
          spacing={1}
          sx={{ alignItems: 'center', justifyContent: 'space-between' }}
        >
          <Stack>
            <Typography variant="body2">
              {t('home.page.settings.startup.startPage')}
            </Typography>
            <Typography variant="caption" color="text.secondary">
              {t('home.page.settings.startup.startPageHint')}
            </Typography>
          </Stack>
          <Select
            size="small"
            value={verge?.start_page ?? '/'}
            onChange={(e) => void patchVerge({ start_page: e.target.value })}
            sx={{ minWidth: 140, '> div': { py: '7.5px' } }}
          >
            {Object.values(primaryNavigationItems).map((page) => (
              <MenuItem key={page.path} value={page.path}>
                {t(page.label)}
              </MenuItem>
            ))}
          </Select>
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t('shared.actions.cancel')}</Button>
      </DialogActions>
    </Dialog>
  )
}

const HomePage = () => {
  const { t } = useTranslation()
  const [searchParams] = useSearchParams()
  const {
    current,
    mutateProfiles,
    profiles,
    error: profilesError,
    isLoading: profilesLoading,
  } = useProfiles()
  const profileCount = profiles?.items?.length
  const [nodeDataStatus, setNodeDataStatus] = useState<
    'loading' | 'empty' | 'available' | 'error'
  >('loading')

  useEffect(() => {
    if (profileCount !== 0) return

    let active = true
    void listClash5Nodes()
      .then((nodes) => {
        if (active) {
          setNodeDataStatus(
            nodes.some((node) => node.active) ? 'available' : 'empty',
          )
        }
      })
      .catch(() => {
        if (active) setNodeDataStatus('error')
      })

    return () => {
      active = false
    }
  }, [profileCount])

  // 设置弹窗的状态
  const [settingsOpen, setSettingsOpen] = useState(false)

  // 文档链接函数
  const toGithubDoc = useLockFn(() =>
    openExternalUrl('https://clash-verge-rev.github.io/index.html').catch(
      showNotice.error,
    ),
  )

  // 新增：打开设置弹窗
  const openSettings = useCallback(() => {
    setSettingsOpen(true)
  }, [])

  const fixedCards = [
    ...(!current
      ? [
          <Grid size={12} key="first-run">
            <HomeProfileCard
              current={current}
              onProfileUpdated={mutateProfiles}
            />
          </Grid>,
        ]
      : []),
    <Grid size={{ xs: 6, md: 3 }} key="network">
      <NetworkSettingsCard />
    </Grid>,
    <Grid size={{ xs: 6, md: 3 }} key="mode">
      <ClashModeEnhancedCard />
    </Grid>,
    <Grid size={{ xs: 6, md: 3 }} key="proxy">
      <CurrentProxyCard />
    </Grid>,
    <Grid size={{ xs: 6, md: 3 }} key="chains">
      <HomeChainCard profileId={current?.uid ?? null} />
    </Grid>,
    <Grid size={{ xs: 6, md: 3 }} key="dns">
      <Suspense fallback={<Skeleton variant="rectangular" height={200} />}>
        <LazyDnsCheckCard />
      </Suspense>
    </Grid>,
  ]

  if (searchParams.get('view') === 'chains') {
    return <ChainsPage />
  }

  const checkingHomeData =
    !profilesError &&
    (profileCount === undefined
      ? profilesLoading
      : profileCount === 0 && nodeDataStatus === 'loading')
  const showFirstRunEmptyState =
    !profilesError && profileCount === 0 && nodeDataStatus === 'empty'

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
          <Tooltip title={t('home.page.tooltips.settings')} arrow>
            <IconButton onClick={openSettings} size="small" color="inherit">
              <SettingsOutlined />
            </IconButton>
          </Tooltip>
        </Box>
      }
    >
      {checkingHomeData ? (
        <Box sx={{ p: 2 }}>
          <Skeleton variant="rounded" height={260} />
        </Box>
      ) : showFirstRunEmptyState ? (
        <FirstRunEmptyState />
      ) : (
        homeSection(t('home.page.sections.status'), fixedCards)
      )}

      {/* 首页设置弹窗 */}
      {settingsOpen && (
        <HomeSettingsDialog onClose={() => setSettingsOpen(false)} />
      )}
    </BasePage>
  )
}

// Renders a titled group of cards, or nothing when every card in it is hidden.
const homeSection = (title: string, cards: React.ReactNode[]) =>
  cards.some(Boolean) ? (
    <Box key={title} sx={{ mb: 2 }}>
      <Typography variant="overline" color="text.secondary">
        {title}
      </Typography>
      <Grid container spacing={1.5} columns={{ xs: 6, sm: 6, md: 12 }}>
        {cards}
      </Grid>
    </Box>
  ) : null

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
      <Stack spacing={1.5}>
        <ProxyControlSwitches
          label={t('settings.sections.system.toggles.systemProxy')}
          onError={(err) => showNotice.error(err)}
          noRightPadding
        />
        <ProxyTunCard />
      </Stack>
    </EnhancedCard>
  )
}

// 增强版 Clash 模式卡片组件
const ClashModeEnhancedCard = () => {
  const { t } = useTranslation()
  return (
    <EnhancedCard
      title={t('home.page.cards.proxyMode')}
      icon={<RouterOutlined />}
      iconColor="info"
      action={null}
    >
      <ClashModeCard />
    </EnhancedCard>
  )
}

export default HomePage
