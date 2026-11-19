import { describe, expect, test } from 'vitest'

import { TrafficSessionTotals } from './traffic-session'

const traffic = (upTotal: number, downTotal: number, up = 2, down = 3) => ({
  up,
  down,
  upTotal,
  downTotal,
})

describe('TrafficSessionTotals', () => {
  test('resets cumulative totals when the core starts a new session', () => {
    const session = new TrafficSessionTotals()

    expect(session.normalize(traffic(100, 200))).toMatchObject({
      upTotal: 100,
      downTotal: 200,
    })
    expect(session.normalize(traffic(130, 260))).toMatchObject({
      upTotal: 130,
      downTotal: 260,
    })
    expect(session.normalize(traffic(4, 7))).toMatchObject({
      upTotal: 0,
      downTotal: 0,
    })
    expect(session.normalize(traffic(10, 17))).toMatchObject({
      upTotal: 6,
      downTotal: 10,
    })
  })

  test('keeps live rates unchanged across a session reset', () => {
    const session = new TrafficSessionTotals()
    session.normalize(traffic(100, 200, 11, 13))

    expect(session.normalize(traffic(2, 5, 17, 19))).toMatchObject({
      up: 17,
      down: 19,
      upTotal: 0,
      downTotal: 0,
    })
  })
})
