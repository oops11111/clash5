import HubOutlinedIcon from '@mui/icons-material/HubOutlined'
import { Paper, Stack, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'

const ChainsPage = () => {
  const { t } = useTranslation()

  return (
    <BasePage title={t('layout.components.navigation.tabs.chains')}>
      <Paper variant="outlined" sx={{ p: 4, borderRadius: 3 }}>
        <Stack spacing={1} sx={{ alignItems: 'center', textAlign: 'center' }}>
          <HubOutlinedIcon color="primary" fontSize="large" />
          <Typography variant="h6">
            {t('layout.components.navigation.empty.chainsTitle')}
          </Typography>
          <Typography color="text.secondary">
            {t('layout.components.navigation.empty.chainsDescription')}
          </Typography>
        </Stack>
      </Paper>
    </BasePage>
  )
}

export default ChainsPage
