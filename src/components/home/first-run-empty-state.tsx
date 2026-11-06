import {
  AddLinkRounded,
  CloudDownloadOutlined,
  ExtensionOutlined,
} from '@mui/icons-material'
import { Box, Button, Paper, Stack, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

export const FirstRunEmptyState = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()

  return (
    <Box
      sx={{
        minHeight: '100%',
        display: 'grid',
        placeItems: 'center',
        p: 2,
        boxSizing: 'border-box',
      }}
    >
      <Paper
        variant="outlined"
        sx={{
          width: '100%',
          maxWidth: 720,
          px: { xs: 2, sm: 4 },
          py: { xs: 3, sm: 4 },
          borderRadius: 3,
          textAlign: 'center',
          boxShadow: '0 4px 16px rgba(47, 107, 255, 0.06)',
        }}
      >
        <CloudDownloadOutlined
          aria-hidden="true"
          sx={{ fontSize: 44, color: 'primary.main', mb: 1 }}
        />
        <Typography variant="h6" sx={{ fontWeight: 700 }}>
          {t('home.page.emptyState.title')}
        </Typography>
        <Typography color="text.secondary" sx={{ mt: 0.75 }}>
          {t('home.page.emptyState.description')}
        </Typography>
        <Stack
          direction={{ xs: 'column', sm: 'row' }}
          spacing={1}
          sx={{ mt: 2.5, justifyContent: 'center' }}
        >
          <Button
            variant="contained"
            startIcon={<CloudDownloadOutlined />}
            onClick={() => navigate('/profile')}
          >
            {t('home.page.emptyState.importSubscription')}
          </Button>
          <Button
            variant="outlined"
            startIcon={<AddLinkRounded />}
            onClick={() => navigate('/profile?view=nodes')}
          >
            {t('home.page.emptyState.addNode')}
          </Button>
          <Button
            variant="outlined"
            startIcon={<ExtensionOutlined />}
            onClick={() => navigate('/plugins')}
          >
            {t('home.page.emptyState.openPlugins')}
          </Button>
        </Stack>
      </Paper>
    </Box>
  )
}
