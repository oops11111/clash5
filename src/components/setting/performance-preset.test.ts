import { describe, expect, it } from 'vitest'

import {
  applyPerformancePreset,
  performancePresetPatch,
  performancePresets,
} from './performance-preset'

describe('performance preset switching', () => {
  it('updates the saved preset and current-node check interval together', () => {
    expect(performancePresets.map(performancePresetPatch)).toEqual([
      {
        performance_preset: 'balanced',
        auto_delay_detection_interval_minutes: 5,
      },
      {
        performance_preset: 'performance',
        auto_delay_detection_interval_minutes: 2,
      },
      {
        performance_preset: 'power_saving',
        auto_delay_detection_interval_minutes: 15,
      },
    ])
  })

  it('switches between presets without changing other settings', () => {
    const original: IVergeConfig = { theme_mode: 'dark' }
    const fast = applyPerformancePreset(original, performancePresets[1])
    const saving = applyPerformancePreset(fast, performancePresets[2])

    expect(fast).toMatchObject({
      theme_mode: 'dark',
      performance_preset: 'performance',
      auto_delay_detection_interval_minutes: 2,
    })
    expect(saving).toMatchObject({
      theme_mode: 'dark',
      performance_preset: 'power_saving',
      auto_delay_detection_interval_minutes: 15,
    })
    expect(original).toEqual({ theme_mode: 'dark' })
  })
})
