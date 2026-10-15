import ExtensionOutlinedIcon from '@mui/icons-material/ExtensionOutlined'
import { Paper, Stack, Tab, Tabs, Typography } from '@mui/material'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'

type PluginTab = 'installed' | 'market' | 'binding' | 'settings'

const tabs: PluginTab[] = ['installed', 'market', 'binding', 'settings']

const PluginsPage = () => {
  const { t } = useTranslation()
  const [tab, setTab] = useState<PluginTab>('installed')

  return (
    <BasePage title={t('layout.components.navigation.tabs.plugins')}>
      <Stack spacing={2}>
        <Tabs
          value={tab}
          onChange={(_, value: PluginTab) => setTab(value)}
          variant="scrollable"
          scrollButtons="auto"
          aria-label={t('layout.components.navigation.tabs.plugins')}
        >
          {tabs.map((item) => (
            <Tab
              key={item}
              value={item}
              label={t(`layout.components.navigation.pluginTabs.${item}`)}
            />
          ))}
        </Tabs>
        <Paper variant="outlined" sx={{ p: 4, borderRadius: 3 }}>
          <Stack spacing={1} sx={{ alignItems: 'center', textAlign: 'center' }}>
            <ExtensionOutlinedIcon color="primary" fontSize="large" />
            <Typography variant="h6">
              {t(`layout.components.navigation.empty.${tab}Title`)}
            </Typography>
            <Typography color="text.secondary">
              {t(`layout.components.navigation.empty.${tab}Description`)}
            </Typography>
          </Stack>
        </Paper>
      </Stack>
    </BasePage>
  )
}

export default PluginsPage
