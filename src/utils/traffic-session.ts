export interface TrafficSessionSample {
  up: number
  down: number
  upTotal: number
  downTotal: number
}

export class TrafficSessionTotals {
  private mode: string | null = null
  private baseline: Pick<TrafficSessionSample, 'upTotal' | 'downTotal'> | null =
    null

  observeMode(mode: string) {
    if (mode === this.mode) return false

    this.mode = mode
    this.baseline = null
    return true
  }

  normalize(sample: TrafficSessionSample): TrafficSessionSample {
    if (this.mode === 'NotRunning') {
      return { ...sample, upTotal: 0, downTotal: 0 }
    }

    if (!this.baseline) {
      this.baseline = {
        upTotal: sample.upTotal,
        downTotal: sample.downTotal,
      }
    }

    return {
      ...sample,
      upTotal: Math.max(0, sample.upTotal - this.baseline.upTotal),
      downTotal: Math.max(0, sample.downTotal - this.baseline.downTotal),
    }
  }
}

export const trafficSessionTotals = new TrafficSessionTotals()
