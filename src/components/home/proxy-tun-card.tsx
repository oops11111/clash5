import { HelpOutlineRounded } from '@mui/icons-material'
import { Box, Tooltip, Typography, alpha, useTheme } from '@mui/material'
import { FC } from 'react'
import { useTranslation } from 'react-i18next'

import ProxyControlSwitches from '@/components/shared/proxy-control-switches'
import { useSystemState } from '@/hooks/use-system-state'
import { useVerge } from '@/hooks/use-verge'
import { showNotice } from '@/services/notice-service'

// Network card: the virtual network adapter (TUN) only. The system proxy switch is in the home header.
export const ProxyTunCard: FC = () => {
  const { t } = useTranslation()
  const theme = useTheme()
  const { verge } = useVerge()
  const { isTunModeAvailable } = useSystemState()

  const { enable_tun_mode } = verge ?? {}

  const status = !isTunModeAvailable
    ? t('home.components.proxyTun.status.tunModeServiceRequired')
    : enable_tun_mode
      ? t('home.components.proxyTun.status.tunModeEnabled')
      : t('home.components.proxyTun.status.tunModeDisabled')

  return (
    <Box
      sx={{ display: 'flex', flexDirection: 'column', gap: 1, width: '100%' }}
    >
      <Typography
        variant="body2"
        color="text.secondary"
        sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}
      >
        {status}
        <Tooltip title={t('home.components.proxyTun.tooltips.tunMode')} arrow>
          <HelpOutlineRounded
            sx={{ fontSize: 14, opacity: 0.7, flexShrink: 0 }}
          />
        </Tooltip>
      </Typography>

      <Box
        sx={{
          p: 1,
          bgcolor: alpha(theme.palette.primary.main, 0.04),
          borderRadius: 2,
        }}
      >
        <ProxyControlSwitches
          onError={(err) => showNotice.error(err)}
          label={t('settings.sections.system.toggles.tunMode')}
          noRightPadding={true}
        />
      </Box>
    </Box>
  )
}
