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
import { useSearchParams } from 'react-router'

import { BasePage } from '@/components/base'
import {
  SettingList,
  type SettingSection,
} from '@/components/setting/mods/setting-comp'
import { PerformancePresetSettings } from '@/components/setting/performance-preset-settings'
import SettingClash from '@/components/setting/setting-clash'
import SettingSystem from '@/components/setting/setting-system'
import SettingVergeAdvanced from '@/components/setting/setting-verge-advanced'
import SettingVergeBasic from '@/components/setting/setting-verge-basic'
import { showNotice } from '@/services/notice-service'
import { openExternalUrl } from '@/utils/open-external-url'

import DiagnosticsPage from './diagnostics'

const settingCardSx = {
  borderRadius: '14px',
  border: '1px solid',
  borderColor: 'divider',
  backgroundColor: 'background.paper',
  boxShadow: '0 1px 3px rgba(26, 35, 50, 0.04)',
}

type DiagnosticTab = 'connections' | 'logs' | 'unlock' | 'dns'
type SettingsGroup = SettingSection | 'diagnostics'

const groups: SettingsGroup[] = [
  'appearance',
  'system',
  'clash',
  'basic',
  'diagnostics',
  'backup',
  'advanced',
]

const isDiagnosticsView = (
  view: string | null,
): view is 'diagnostics' | DiagnosticTab =>
  view === 'diagnostics' ||
  view === 'connections' ||
  view === 'logs' ||
  view === 'unlock' ||
  view === 'dns'

const SettingPage = () => {
  const { t } = useTranslation()
  const [searchParams, setSearchParams] = useSearchParams()
  const requestedView = searchParams.get('view')
  const diagnosticsView = isDiagnosticsView(requestedView)
  const initialDiagnosticTab: DiagnosticTab | undefined =
    requestedView === 'connections' ||
    requestedView === 'logs' ||
    requestedView === 'unlock' ||
    requestedView === 'dns'
      ? requestedView
      : undefined
  const [selectedGroup, setSelectedGroup] = useState<SettingsGroup>(() =>
    diagnosticsView ? 'diagnostics' : 'appearance',
  )
  const active = diagnosticsView
    ? 'diagnostics'
    : selectedGroup === 'diagnostics'
      ? 'appearance'
      : selectedGroup

  const selectGroup = (group: SettingsGroup) => {
    setSelectedGroup(group)
    setSearchParams(
      (current) => {
        const currentView = current.get('view')
        if (group === 'diagnostics') {
          if (isDiagnosticsView(currentView)) return current
          const next = new URLSearchParams(current)
          next.set('view', 'diagnostics')
          return next
        }
        if (!isDiagnosticsView(currentView)) return current
        const next = new URLSearchParams(current)
        next.delete('view')
        return next
      },
      { replace: true },
    )
  }

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

  const renderGroup = (section: SettingSection) => {
    switch (section) {
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
        return <SettingVergeAdvanced section={section} />
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
      {active !== 'diagnostics' && <PerformancePresetSettings />}
      <Box
        sx={{
          display: 'grid',
          gap: 1.5,
          gridTemplateColumns: { xs: '1fr', md: '200px minmax(0, 1fr)' },
          alignItems: 'stretch',
          ...(active === 'diagnostics'
            ? {
                height: 'min(700px, calc(100vh - 160px))',
                minHeight: '420px',
              }
            : {}),
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
                onClick={() => selectGroup(group)}
                sx={{ borderRadius: '10px' }}
              >
                <ListItemText primary={t(`settings.page.groups.${group}`)} />
              </ListItemButton>
            ))}
          </List>
        </Box>
        <Box
          sx={{
            ...settingCardSx,
            minWidth: 0,
            display: active === 'diagnostics' ? 'flex' : undefined,
            flexDirection: active === 'diagnostics' ? 'column' : undefined,
            overflow: active === 'diagnostics' ? 'hidden' : undefined,
          }}
        >
          {active === 'diagnostics' ? (
            <DiagnosticsPage
              key={initialDiagnosticTab ?? 'connections'}
              initialView={initialDiagnosticTab}
            />
          ) : (
            <SettingList title={t(`settings.page.groups.${active}`)}>
              {renderGroup(active)}
            </SettingList>
          )}
        </Box>
      </Box>
    </BasePage>
  )
}

export default SettingPage
