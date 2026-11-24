import { Box, Button, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { BasePage } from '@/components/base'

export const NotFoundPage = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()

  return (
    <BasePage title={t('shared.feedback.errors.pageNotFound')}>
      <Box>
        <Typography>
          {t('shared.feedback.errors.pageNotFoundDescription')}
        </Typography>
        <Button
          variant="contained"
          onClick={() => navigate('/')}
          sx={{ marginTop: 2 }}
        >
          {t('layout.components.navigation.tabs.home')}
        </Button>
      </Box>
    </BasePage>
  )
}
