import {
  DnsOutlined,
  HelpOutlineRounded,
  HistoryEduOutlined,
  RouterOutlined,
  SettingsOutlined,
  SpeedOutlined,
} from '@mui/icons-material'
import {
  Box,
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  FormGroup,
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
import { Suspense, lazy, useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import { ClashModeCard } from '@/components/home/clash-mode-card'
import { CurrentProxyCard } from '@/components/home/current-proxy-card'
import { EnhancedCard } from '@/components/home/enhanced-card'
import { EnhancedTrafficStats } from '@/components/home/enhanced-traffic-stats'
import { HomeProfileCard } from '@/components/home/home-profile-card'
import { ProxyTunCard } from '@/components/home/proxy-tun-card'
import { ThemeModeSwitch } from '@/components/setting/mods/theme-mode-switch'
import ProxyControlSwitches from '@/components/shared/proxy-control-switches'
import { useProfiles } from '@/hooks/use-profiles'
import { useVerge } from '@/hooks/use-verge'
import { navigationItems } from '@/pages/_navigation-meta'
import { entry_lightweight_mode } from '@/services/cmds'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'
import { openExternalUrl } from '@/utils/open-external-url'

const preloadTestCard = () =>
  import('@/components/home/test-card').then((module) => ({
    default: module.TestCard,
  }))
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

const LazyTestCard = lazy(preloadTestCard)
const LazyIpInfoCard = lazy(preloadIpInfoCard)
const LazyClashInfoCard = lazy(preloadClashInfoCard)
const LazySystemInfoCard = lazy(preloadSystemInfoCard)

// Used by bootstrap to initiate optional card imports without blocking render.
// eslint-disable-next-line react-refresh/only-export-components
export const preloadHomePageCards = () =>
  Promise.all([
    preloadTestCard().catch(() => {}),
    preloadIpInfoCard().catch(() => {}),
    preloadClashInfoCard().catch(() => {}),
    preloadSystemInfoCard().catch(() => {}),
  ])

// 定义首页卡片设置接口
interface HomeCardsSettings {
  profile: boolean
  proxy: boolean
  network: boolean
  mode: boolean
  traffic: boolean
  info: boolean
  clashinfo: boolean
  systeminfo: boolean
  test: boolean
  ip: boolean
  [key: string]: boolean
}

const DEFAULT_HOME_CARDS: HomeCardsSettings = {
  info: false,
  profile: true,
  proxy: true,
  network: true,
  mode: true,
  traffic: true,
  clashinfo: true,
  systeminfo: true,
  test: true,
  ip: false,
}

const serializeCardFlags = (cards: HomeCardsSettings) =>
  Object.keys(cards)
    .sort()
    .map((key) => `${key}:${cards[key] ? 1 : 0}`)
    .join('|')

// 首页设置对话框组件接口
interface HomeSettingsDialogProps {
  onClose: () => void
  homeCards: HomeCardsSettings
}

// 首页设置对话框组件
// Accent colors offered in the home settings; the choice is saved as the theme's primary color.
const ACCENT_COLORS = ['#2F6BFF', '#8B5CF6', '#22C55E', '#F59E0B']

const HomeSettingsDialog = ({
  onClose,
  homeCards,
}: HomeSettingsDialogProps) => {
  const { t } = useTranslation()
  const [cards, setCards] = useState<HomeCardsSettings>(homeCards)
  const { verge, patchVerge } = useVerge()
  const accent = verge?.theme_setting?.primary_color

  const handleToggle = (key: string) => {
    setCards((prev: HomeCardsSettings) => ({
      ...prev,
      [key]: !prev[key],
    }))
  }

  const handleSave = async () => {
    await patchVerge({ home_cards: cards })
    onClose()
  }

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
          {t('home.page.settings.cardsTitle')}
        </Typography>
        <FormGroup>
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.profile || false}
                onChange={() => handleToggle('profile')}
              />
            }
            label={t('home.page.settings.cards.profile')}
          />
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.proxy || false}
                onChange={() => handleToggle('proxy')}
              />
            }
            label={t('home.page.settings.cards.currentProxy')}
          />
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.network || false}
                onChange={() => handleToggle('network')}
              />
            }
            label={t('home.page.settings.cards.network')}
          />
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.mode || false}
                onChange={() => handleToggle('mode')}
              />
            }
            label={t('home.page.settings.cards.proxyMode')}
          />
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.traffic || false}
                onChange={() => handleToggle('traffic')}
              />
            }
            label={t('home.page.settings.cards.traffic')}
          />
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.test || false}
                onChange={() => handleToggle('test')}
              />
            }
            label={t('home.page.settings.cards.tests')}
          />
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.ip || false}
                onChange={() => handleToggle('ip')}
              />
            }
            label={t('home.page.settings.cards.ip')}
          />
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.clashinfo || false}
                onChange={() => handleToggle('clashinfo')}
              />
            }
            label={t('home.page.settings.cards.clashInfo')}
          />
          <FormControlLabel
            control={
              <Checkbox
                checked={cards.systeminfo || false}
                onChange={() => handleToggle('systeminfo')}
              />
            }
            label={t('home.page.settings.cards.systemInfo')}
          />
        </FormGroup>
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
            {Object.values(navigationItems).map((page) => (
              <MenuItem key={page.path} value={page.path}>
                {t(page.label)}
              </MenuItem>
            ))}
          </Select>
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t('shared.actions.cancel')}</Button>
        <Button onClick={handleSave} color="primary">
          {t('shared.actions.save')}
        </Button>
      </DialogActions>
    </Dialog>
  )
}

const HomePage = () => {
  const { t } = useTranslation()
  const { verge } = useVerge()
  const { current, mutateProfiles } = useProfiles()

  // 设置弹窗的状态
  const [settingsOpen, setSettingsOpen] = useState(false)

  // 卡片显示状态
  const homeCards =
    (verge?.home_cards as HomeCardsSettings | undefined) ?? DEFAULT_HOME_CARDS

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

  const renderCard = useCallback(
    (cardKey: string, component: React.ReactNode, size: number = 6) => {
      if (!homeCards[cardKey]) return null

      return (
        <Grid size={size} key={cardKey}>
          {component}
        </Grid>
      )
    },
    [homeCards],
  )

  const criticalCards = useMemo(
    () => [
      renderCard(
        'profile',
        <HomeProfileCard current={current} onProfileUpdated={mutateProfiles} />,
      ),
      renderCard('proxy', <CurrentProxyCard />),
      renderCard('network', <NetworkSettingsCard />),
      renderCard('mode', <ClashModeEnhancedCard />),
    ],
    [current, mutateProfiles, renderCard],
  )

  const nonCriticalCards = useMemo(
    () => [
      renderCard(
        'traffic',
        <EnhancedCard
          title={t('home.page.cards.trafficStats')}
          icon={<SpeedOutlined />}
          iconColor="secondary"
        >
          <EnhancedTrafficStats />
        </EnhancedCard>,
        12,
      ),
      renderCard(
        'test',
        <Suspense fallback={<Skeleton variant="rectangular" height={200} />}>
          <LazyTestCard />
        </Suspense>,
      ),
      renderCard(
        'ip',
        <Suspense fallback={<Skeleton variant="rectangular" height={200} />}>
          <LazyIpInfoCard />
        </Suspense>,
      ),
      renderCard(
        'clashinfo',
        <Suspense fallback={<Skeleton variant="rectangular" height={200} />}>
          <LazyClashInfoCard />
        </Suspense>,
      ),
      renderCard(
        'systeminfo',
        <Suspense fallback={<Skeleton variant="rectangular" height={200} />}>
          <LazySystemInfoCard />
        </Suspense>,
      ),
    ],
    [t, renderCard],
  )
  return (
    <BasePage
      title={t('home.page.title')}
      contentStyle={{ padding: 2 }}
      header={
        <Box sx={{ display: 'flex', alignItems: 'center' }}>
          <Box sx={{ minWidth: 200, mr: 1 }}>
            <ProxyControlSwitches
              label={t('settings.sections.system.toggles.systemProxy')}
              onError={(err) => showNotice.error(err)}
              noRightPadding
            />
          </Box>
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
      {homeSection(t('home.page.sections.status'), criticalCards)}
      {/* nonCriticalCards starts with the traffic card, the rest are tools. */}
      {homeSection(
        t('home.page.sections.traffic'),
        nonCriticalCards.slice(0, 1),
      )}
      {homeSection(t('home.page.sections.tools'), nonCriticalCards.slice(1))}

      {/* 首页设置弹窗 */}
      {settingsOpen && (
        <HomeSettingsDialog
          key={serializeCardFlags(homeCards)}
          onClose={() => setSettingsOpen(false)}
          homeCards={homeCards}
        />
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
      <ProxyTunCard />
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
