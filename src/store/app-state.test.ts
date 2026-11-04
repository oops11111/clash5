import { describe, expect, it } from 'vitest'

import type { RunState } from '@/services/cmds'

import { appStoreReducer, initialAppStoreState } from './app-state'

const runState = (mode: RunState['mode']) => ({ mode }) as RunState

describe('traffic session revision', () => {
  it('advances when the observed core mode starts or changes', () => {
    const first = appStoreReducer(initialAppStoreState, {
      type: 'runState/loaded',
      runState: runState('Service'),
    })
    const same = appStoreReducer(first, {
      type: 'runState/loaded',
      runState: runState('Service'),
    })
    const stopped = appStoreReducer(same, {
      type: 'runState/loaded',
      runState: runState('NotRunning'),
    })
    const restarted = appStoreReducer(stopped, {
      type: 'runState/loaded',
      runState: runState('Service'),
    })

    expect(first.trafficSession).toBe(1)
    expect(same.trafficSession).toBe(1)
    expect(stopped.trafficSession).toBe(2)
    expect(restarted.trafficSession).toBe(3)
  })
})
