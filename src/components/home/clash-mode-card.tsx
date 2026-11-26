import {
  ArrowForwardRounded,
  DirectionsRounded,
  LanguageRounded,
  MultipleStopRounded,
} from '@mui/icons-material'
import { Box, Button, Paper, Stack, Typography } from '@mui/material'
import { type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { useClashMode, useRuntimeConfig } from '@/hooks/use-clash'
import {
  useClashConfigData,
  useCoreDataStatus,
} from '@/providers/app-data-context'
import type { TranslationKey } from '@/types/generated/i18n-keys'

const CLASH_MODES = ['rule', 'global', 'direct'] as const
type ClashMode = (typeof CLASH_MODES)[number]

const isClashMode = (mode: string): mode is ClashMode =>
  (CLASH_MODES as readonly string[]).includes(mode)

const toClashMode = (mode?: string | null) => {
  const normalized = mode?.toLowerCase()
  return normalized && isClashMode(normalized) ? normalized : undefined
}

const MODE_META: Record<
  ClashMode,
  { label: TranslationKey; description: TranslationKey }
> = {
  rule: {
    label: 'home.components.clashMode.labels.rule',
    description: 'home.components.clashMode.descriptions.rule',
  },
  global: {
    label: 'home.components.clashMode.labels.global',
    description: 'home.components.clashMode.descriptions.global',
  },
  direct: {
    label: 'home.components.clashMode.labels.direct',
    description: 'home.components.clashMode.descriptions.direct',
  },
}

const MODE_ICONS: Record<ClashMode, ReactNode> = {
  rule: <MultipleStopRounded fontSize="small" />,
  global: <LanguageRounded fontSize="small" />,
  direct: <DirectionsRounded fontSize="small" />,
}

// Read-only display of the current proxy mode. The only place that changes
// the mode is the proxies page; this card links there instead of editing.
export const ClashModeCard = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { clashConfig } = useClashConfigData()
  const { isCoreDataPending } = useCoreDataStatus()

  const controllerMode = toClashMode(clashConfig?.mode)
  const needFallback = !controllerMode
  const { data: runtimeConfig, isPending: isRuntimeConfigPending } =
    useRuntimeConfig(needFallback)
  const runtimeMode = toClashMode(runtimeConfig?.mode)
  const { data: backendMode, isPending: isBackendModePending } =
    useClashMode(needFallback)
  // Saved config is refreshed on mode changes; runtime config may be stale.
  const fallbackMode = toClashMode(backendMode) ?? runtimeMode

  const currentMode = controllerMode ?? fallbackMode

  const modeDescription = currentMode
    ? t(MODE_META[currentMode].description)
    : isCoreDataPending || isRuntimeConfigPending || isBackendModePending
      ? ' '
      : t('home.components.clashMode.errors.communication')

  const buttonStyles = (mode: ClashMode) => ({
    px: 2,
    py: 1.2,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 1,
    bgcolor: mode === currentMode ? 'primary.main' : 'background.paper',
    color: mode === currentMode ? 'primary.contrastText' : 'text.primary',
    borderRadius: 1.5,
    position: 'relative',
    overflow: 'visible',
    '&::after':
      mode === currentMode
        ? {
            content: '""',
            position: 'absolute',
            bottom: -16,
            left: '50%',
            width: 2,
            height: 16,
            bgcolor: 'primary.main',
            transform: 'translateX(-50%)',
          }
        : {},
  })

  const descriptionStyles = {
    width: '95%',
    textAlign: 'center',
    color: 'text.secondary',
    p: 0.8,
    borderRadius: 1,
    borderColor: 'primary.main',
    borderWidth: 1,
    borderStyle: 'solid',
    backgroundColor: 'background.paper',
    wordBreak: 'break-word',
    hyphens: 'auto',
  }

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', width: '100%' }}>
      <Stack
        direction="row"
        spacing={1}
        sx={{
          display: 'flex',
          justifyContent: 'center',
          py: 1,
          position: 'relative',
          zIndex: 2,
        }}
      >
        {CLASH_MODES.map((mode) => (
          <Paper
            key={mode}
            elevation={mode === currentMode ? 2 : 0}
            sx={buttonStyles(mode)}
          >
            {MODE_ICONS[mode]}
            <Typography
              variant="body2"
              sx={{
                textTransform: 'capitalize',
                fontWeight: mode === currentMode ? 600 : 400,
              }}
            >
              {t(MODE_META[mode].label)}
            </Typography>
          </Paper>
        ))}
      </Stack>

      <Box
        sx={{
          width: '100%',
          my: 1,
          position: 'relative',
          display: 'flex',
          justifyContent: 'center',
          overflow: 'visible',
        }}
      >
        <Typography variant="caption" component="div" sx={descriptionStyles}>
          {modeDescription}
        </Typography>
      </Box>

      <Box sx={{ display: 'flex', justifyContent: 'center' }}>
        <Button
          size="small"
          endIcon={<ArrowForwardRounded fontSize="small" />}
          onClick={() => navigate('/proxies')}
        >
          {t('layout.components.navigation.tabs.proxies')}
        </Button>
      </Box>
    </Box>
  )
}
