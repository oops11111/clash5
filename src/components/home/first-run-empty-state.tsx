import {
  AddLinkRounded,
  DnsOutlined,
  ExtensionOutlined,
} from '@mui/icons-material'
import { Button, Paper, Stack, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

export const FirstRunEmptyState = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()

  return (
    <Paper
      role="region"
      aria-labelledby="first-run-empty-title"
      data-testid="first-run-empty-state"
      variant="outlined"
      sx={{
        p: { xs: 3, sm: 5 },
        borderRadius: 3,
        textAlign: 'center',
        bgcolor: 'background.paper',
      }}
    >
      <Stack spacing={2.5} sx={{ alignItems: 'center' }}>
        <Stack spacing={1} sx={{ alignItems: 'center', maxWidth: 560 }}>
          <Typography
            id="first-run-empty-title"
            variant="h5"
            sx={{ fontWeight: 700 }}
          >
            {t('home.page.firstRun.title')}
          </Typography>
          <Typography color="text.secondary">
            {t('home.page.firstRun.description')}
          </Typography>
        </Stack>
        <Stack
          direction={{ xs: 'column', sm: 'row' }}
          spacing={1.5}
          useFlexGap
          sx={{ justifyContent: 'center', flexWrap: 'wrap' }}
        >
          <Button
            data-testid="first-run-import-subscription"
            variant="contained"
            startIcon={<AddLinkRounded />}
            onClick={() => navigate('/subscriptions?section=sources')}
          >
            {t('home.page.firstRun.importSubscription')}
          </Button>
          <Button
            data-testid="first-run-add-node"
            variant="outlined"
            startIcon={<DnsOutlined />}
            onClick={() => navigate('/subscriptions?section=nodes')}
          >
            {t('home.page.firstRun.addNode')}
          </Button>
          <Button
            data-testid="first-run-open-plugins"
            variant="text"
            startIcon={<ExtensionOutlined />}
            onClick={() => navigate('/plugins')}
          >
            {t('home.page.firstRun.plugins')}
          </Button>
        </Stack>
      </Stack>
    </Paper>
  )
}
