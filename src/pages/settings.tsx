import { GitHub, HelpOutlineRounded, Telegram } from '@mui/icons-material'
import {
  Box,
  ButtonGroup,
  IconButton,
  List,
  ListItemButton,
  ListItemText,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import {
  SettingList,
  SettingSection,
} from '@/components/setting/mods/setting-comp'
import { PerformancePresetSettings } from '@/components/setting/performance-preset-settings'
import SettingClash from '@/components/setting/setting-clash'
import SettingSystem from '@/components/setting/setting-system'
import SettingVergeAdvanced from '@/components/setting/setting-verge-advanced'
import SettingVergeBasic from '@/components/setting/setting-verge-basic'
import { showNotice } from '@/services/notice-service'
import { openExternalUrl } from '@/utils/open-external-url'

const settingCardSx = {
  borderRadius: '14px',
  border: '1px solid',
  borderColor: 'divider',
  backgroundColor: 'background.paper',
  boxShadow: '0 1px 3px rgba(26, 35, 50, 0.04)',
}

const groups: SettingSection[] = [
  'appearance',
  'system',
  'clash',
  'basic',
  'backup',
  'advanced',
]

const SettingPage = () => {
  const { t } = useTranslation()
  const [active, setActive] = useState<SettingSection>('appearance')

  const onError = (err: any) => {
    showNotice.error(err)
  }

  const toGithubRepo = useLockFn(() =>
    openExternalUrl('https://github.com/clash-verge-rev/clash-verge-rev').catch(
      onError,
    ),
  )

  const toGithubDoc = useLockFn(() =>
    openExternalUrl('https://clash-verge-rev.github.io/index.html').catch(
      onError,
    ),
  )

  const toTelegramChannel = useLockFn(() =>
    openExternalUrl('https://t.me/clash_verge_re').catch(onError),
  )

  const renderGroup = () => {
    switch (active) {
      case 'appearance':
        return <SettingVergeBasic section="appearance" onError={onError} />
      case 'system':
        return (
          <>
            <SettingSystem section="system" onError={onError} />
            <SettingClash section="system" onError={onError} />
          </>
        )
      case 'clash':
        return <SettingClash section="clash" onError={onError} />
      case 'basic':
        return <SettingVergeBasic section="basic" onError={onError} />
      case 'backup':
      case 'advanced':
        return <SettingVergeAdvanced section={active} />
    }
  }

  return (
    <BasePage
      title={t('settings.page.title')}
      header={
        <ButtonGroup
          variant="contained"
          aria-label={t('settings.page.actionsGroupLabel')}
        >
          <IconButton
            size="medium"
            color="inherit"
            title={t('settings.page.actions.manual')}
            onClick={toGithubDoc}
          >
            <HelpOutlineRounded fontSize="inherit" />
          </IconButton>
          <IconButton
            size="medium"
            color="inherit"
            title={t('settings.page.actions.telegram')}
            onClick={toTelegramChannel}
          >
            <Telegram fontSize="inherit" />
          </IconButton>

          <IconButton
            size="medium"
            color="inherit"
            title={t('settings.page.actions.github')}
            onClick={toGithubRepo}
          >
            <GitHub fontSize="inherit" />
          </IconButton>
        </ButtonGroup>
      }
    >
      <PerformancePresetSettings />
      <Box
        sx={{
          display: 'grid',
          gap: 1.5,
          gridTemplateColumns: { xs: '1fr', md: '200px minmax(0, 1fr)' },
          alignItems: 'start',
        }}
      >
        <Box
          component="nav"
          aria-label={t('settings.page.groupsLabel')}
          sx={{ ...settingCardSx, p: 1 }}
        >
          <List dense disablePadding>
            {groups.map((group) => (
              <ListItemButton
                key={group}
                selected={active === group}
                onClick={() => setActive(group)}
                sx={{ borderRadius: '10px' }}
              >
                <ListItemText primary={t(`settings.page.groups.${group}`)} />
              </ListItemButton>
            ))}
          </List>
        </Box>
        <Box sx={{ ...settingCardSx, minWidth: 0 }}>
          <SettingList title={t(`settings.page.groups.${active}`)}>
            {renderGroup()}
          </SettingList>
        </Box>
      </Box>
    </BasePage>
  )
}

export default SettingPage
