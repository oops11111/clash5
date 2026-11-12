import { describe, expect, test } from 'vitest'

import { TrafficSessionTotals } from './traffic-session'

describe('traffic totals per core session', () => {
  test('resets totals at stop and start while preserving live rates', () => {
    const totals = new TrafficSessionTotals()
    expect(totals.observeMode('Service')).toBe(true)

    expect(
      totals.normalize({ up: 12, down: 24, upTotal: 500, downTotal: 900 }),
    ).toEqual({ up: 12, down: 24, upTotal: 0, downTotal: 0 })
    expect(
      totals.normalize({ up: 14, down: 30, upTotal: 540, downTotal: 980 }),
    ).toEqual({ up: 14, down: 30, upTotal: 40, downTotal: 80 })

    expect(totals.observeMode('NotRunning')).toBe(true)
    expect(
      totals.normalize({ up: 0, down: 0, upTotal: 540, downTotal: 980 }),
    ).toEqual({ up: 0, down: 0, upTotal: 0, downTotal: 0 })

    expect(totals.observeMode('Service')).toBe(true)
    expect(
      totals.normalize({ up: 7, down: 11, upTotal: 3, downTotal: 6 }),
    ).toEqual({ up: 7, down: 11, upTotal: 0, downTotal: 0 })
    expect(
      totals.normalize({ up: 8, down: 13, upTotal: 10, downTotal: 20 }),
    ).toEqual({ up: 8, down: 13, upTotal: 7, downTotal: 14 })
  })

  test('does not reset a session on repeated run-state snapshots', () => {
    const totals = new TrafficSessionTotals()
    totals.observeMode('Sidecar')
    totals.normalize({ up: 1, down: 2, upTotal: 100, downTotal: 200 })

    expect(totals.observeMode('Sidecar')).toBe(false)
    expect(
      totals.normalize({ up: 3, down: 4, upTotal: 110, downTotal: 220 }),
    ).toEqual({ up: 3, down: 4, upTotal: 10, downTotal: 20 })
  })
})
