import { Box, Button, Stack, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'
import { useNavigate, useRouteError } from 'react-router'

/**
 * Full-page fallback for errors that escape the layout's error boundary
 * (e.g. thrown while rendering the layout itself). Retry re-runs the
 * current route; Home navigates away from the failing route.
 */
export const RouteErrorPage = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const error = useRouteError()
  const errorMessage = error instanceof Error ? error.message : String(error)

  return (
    <Box role="alert" sx={{ padding: 2 }}>
      <Typography variant="h6">
        {t('shared.feedback.errors.unexpected')}
      </Typography>
      <Typography variant="body2" sx={{ whiteSpace: 'pre-wrap' }}>
        {errorMessage}
      </Typography>
      <Stack direction="row" spacing={1} sx={{ marginTop: 2 }}>
        <Button variant="contained" onClick={() => navigate(0)}>
          {t('shared.actions.retry')}
        </Button>
        <Button variant="outlined" onClick={() => navigate('/')}>
          {t('layout.components.navigation.tabs.home')}
        </Button>
      </Stack>
    </Box>
  )
}
