import type { Traffic } from 'tauri-plugin-mihomo-api'

type Totals = Pick<Traffic, 'upTotal' | 'downTotal'>

const total = (value: number) => (Number.isFinite(value) && value >= 0 ? value : 0)

/**
 * Keeps cumulative traffic totals scoped to the current Mihomo process.
 * Mihomo resets both counters when its core starts; a decrease therefore marks
 * a new session while instantaneous rates and graph samples remain untouched.
 */
export class TrafficSessionTotals {
  private previous: Totals | undefined
  private baseline: Totals = { upTotal: 0, downTotal: 0 }

  normalize(traffic: Traffic): Traffic {
    const current = {
      upTotal: total(traffic.upTotal),
      downTotal: total(traffic.downTotal),
    }
    if (
      this.previous &&
      (current.upTotal < this.previous.upTotal ||
        current.downTotal < this.previous.downTotal)
    ) {
      this.baseline = current
    }
    this.previous = current

    return {
      ...traffic,
      upTotal: Math.max(0, current.upTotal - this.baseline.upTotal),
      downTotal: Math.max(0, current.downTotal - this.baseline.downTotal),
    }
  }

  reset() {
    this.previous = undefined
    this.baseline = { upTotal: 0, downTotal: 0 }
  }
}
