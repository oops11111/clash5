export const performancePresets = [
  { value: 'balanced', intervalMinutes: 5 },
  { value: 'performance', intervalMinutes: 2 },
  { value: 'power_saving', intervalMinutes: 15 },
] as const

export type PerformancePresetOption = (typeof performancePresets)[number]

export const performancePresetPatch = (preset: PerformancePresetOption) => ({
  performance_preset: preset.value,
  auto_delay_detection_interval_minutes: preset.intervalMinutes,
})

export const applyPerformancePreset = (
  config: IVergeConfig,
  preset: PerformancePresetOption,
): IVergeConfig => ({ ...config, ...performancePresetPatch(preset) })
