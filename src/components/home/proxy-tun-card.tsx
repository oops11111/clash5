import { ArrowForwardRounded } from '@mui/icons-material'
import { Box, Button, Stack, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { useSystemProxyState } from '@/hooks/use-system-proxy-state'
import { useSystemState } from '@/hooks/use-system-state'
import { useVerge } from '@/hooks/use-verge'

// Read-only display of the system-proxy and TUN states. The only place that
// changes them is the settings page; this card links there instead of
// editing. The editable switches stay in ProxyControlSwitches, used by the
// settings page with its explicit `kind` dispatch.
export const ProxyTunCard = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { verge } = useVerge()
  const { isTunModeAvailable } = useSystemState()
  // Display the observed OS state, not the requested configuration.
  const { indicator: systemProxyIndicator } = useSystemProxyState()

  const { enable_tun_mode } = verge ?? {}

  const rows = [
    {
      label: t('settings.sections.system.toggles.systemProxy'),
      status: systemProxyIndicator
        ? t('home.components.proxyTun.status.systemProxyEnabled')
        : t('home.components.proxyTun.status.systemProxyDisabled'),
    },
    {
      label: t('settings.sections.system.toggles.tunMode'),
      status: !isTunModeAvailable
        ? t('home.components.proxyTun.status.tunModeServiceRequired')
        : enable_tun_mode
          ? t('home.components.proxyTun.status.tunModeEnabled')
          : t('home.components.proxyTun.status.tunModeDisabled'),
    },
  ]

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', width: '100%' }}>
      <Stack spacing={1} sx={{ py: 1 }}>
        {rows.map((row) => (
          <Box
            key={row.label}
            sx={{
              display: 'flex',
              alignItems: 'baseline',
              justifyContent: 'space-between',
              gap: 2,
            }}
          >
            <Typography variant="body2" sx={{ fontWeight: 500 }}>
              {row.label}
            </Typography>
            <Typography
              variant="caption"
              color="text.secondary"
              sx={{ textAlign: 'right' }}
            >
              {row.status}
            </Typography>
          </Box>
        ))}
      </Stack>

      <Box sx={{ display: 'flex', justifyContent: 'center' }}>
        <Button
          size="small"
          endIcon={<ArrowForwardRounded fontSize="small" />}
          onClick={() => navigate('/settings')}
        >
          {t('layout.components.navigation.tabs.settings')}
        </Button>
      </Box>
    </Box>
  )
}
