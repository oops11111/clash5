import { Box, Button, Typography } from '@mui/material'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useMemoryMitigation } from '@/hooks/use-memory-mitigation'
import { useSystemState } from '@/hooks/use-system-state'
import { useVerge } from '@/hooks/use-verge'
import { showNotice } from '@/services/notice-service'

import {
  applyPerformancePreset,
  performancePresetPatch,
  performancePresets,
  type PerformancePresetOption,
} from './performance-preset'

export const PerformancePresetSettings = () => {
  const { t } = useTranslation()
  const { verge, mutateVerge, patchVerge } = useVerge()
  const memoryMitigation = useMemoryMitigation()
  const { runningMode } = useSystemState()
  const [saving, setSaving] = useState(false)
  const selected = verge?.performance_preset ?? 'balanced'
  const selectedPreset = performancePresets.find(
    (preset) => preset.value === selected,
  )
  const customInterval = verge?.auto_delay_detection_interval_minutes

  const selectPreset = async (preset: PerformancePresetOption) => {
    if (!verge || saving || selected === preset.value) return
    const patch = performancePresetPatch(preset)
    mutateVerge(applyPerformancePreset(verge, preset), false)
    setSaving(true)
    try {
      await patchVerge(patch)
    } catch (error) {
      mutateVerge(verge, false)
      showNotice.error(error)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Box
      sx={{
        borderRadius: 2,
        backgroundColor: 'background.paper',
        px: 2,
        py: 1.5,
        mb: 1.5,
      }}
    >
      <Typography sx={{ fontWeight: 700, mb: 1.5 }}>
        {t('settings.components.verge.performance.title')}
      </Typography>
      <Box
        role="group"
        aria-label={t('settings.components.verge.performance.title')}
        sx={{
          display: 'grid',
          gridTemplateColumns: { xs: '1fr', md: 'repeat(3, minmax(0, 1fr))' },
          gap: 1.5,
        }}
      >
        {performancePresets.map((preset) => (
          <Button
            key={preset.value}
            variant={selected === preset.value ? 'contained' : 'outlined'}
            aria-pressed={selected === preset.value}
            disabled={!verge || saving}
            onClick={() => void selectPreset(preset)}
            sx={{
              display: 'block',
              textTransform: 'none',
              textAlign: 'left',
              minHeight: 88,
              py: 1.25,
            }}
          >
            <Typography variant="body2" sx={{ fontWeight: 700 }}>
              {t(`settings.components.verge.performance.${preset.value}`)}
            </Typography>
            <Typography
              variant="caption"
              component="span"
              sx={{ display: 'block', mt: 0.5 }}
            >
              {t(
                `settings.components.verge.performance.${preset.value}Description`,
              )}
            </Typography>
          </Button>
        ))}
      </Box>
      {memoryMitigation?.active && (
        <Typography
          variant="caption"
          color="warning.main"
          sx={{ display: 'block', mt: 1 }}
        >
          {t('settings.components.verge.performance.memoryProtectionActive')}
        </Typography>
      )}
      {runningMode !== 'NotRunning' &&
        memoryMitigation &&
        !memoryMitigation.monitoring && (
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ display: 'block', mt: 1 }}
          >
            {t(
              'settings.components.verge.performance.memoryTelemetryUnavailable',
            )}
          </Typography>
        )}
      {customInterval != null &&
        customInterval !== selectedPreset?.intervalMinutes && (
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ display: 'block', mt: 1 }}
          >
            {t('settings.components.verge.performance.manualInterval', {
              minutes: customInterval,
            })}
          </Typography>
        )}
    </Box>
  )
}
