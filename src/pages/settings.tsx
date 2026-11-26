import { Box, Grid } from '@mui/material'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import { PerformancePresetSettings } from '@/components/setting/performance-preset-settings'
import SettingClash from '@/components/setting/setting-clash'
import SettingSystem from '@/components/setting/setting-system'
import SettingVergeAdvanced from '@/components/setting/setting-verge-advanced'
import SettingVergeBasic from '@/components/setting/setting-verge-basic'
import { showNotice } from '@/services/notice-service'

const settingCardSx = {
  borderRadius: '14px',
  border: '1px solid',
  borderColor: 'divider',
  backgroundColor: 'background.paper',
  boxShadow: '0 1px 3px rgba(26, 35, 50, 0.04)',
}

const SettingPage = () => {
  const { t } = useTranslation()

  const onError = (err: any) => {
    showNotice.error(err)
  }

  return (
    <BasePage title={t('settings.page.title')}>
      <PerformancePresetSettings />
      <Grid container spacing={1.5} columns={{ xs: 6, sm: 6, md: 12 }}>
        <Grid size={6}>
          <Box
            sx={{
              ...settingCardSx,
              marginBottom: 1.5,
            }}
          >
            <SettingSystem onError={onError} />
          </Box>
          <Box sx={settingCardSx}>
            <SettingClash onError={onError} />
          </Box>
        </Grid>
        <Grid size={6}>
          <Box
            sx={{
              ...settingCardSx,
              marginBottom: 1.5,
            }}
          >
            <SettingVergeBasic onError={onError} />
          </Box>
          <Box sx={settingCardSx}>
            <SettingVergeAdvanced onError={onError} />
          </Box>
        </Grid>
      </Grid>
    </BasePage>
  )
}

export default SettingPage
