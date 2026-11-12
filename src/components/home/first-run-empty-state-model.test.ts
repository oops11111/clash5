import { describe, expect, it } from 'vitest'

import { shouldShowFirstRunEmptyState } from './first-run-empty-state-model'

describe('first-run empty state', () => {
  it('waits for the node inventory before deciding', () => {
    expect(shouldShowFirstRunEmptyState(null, false)).toBe(false)
  })

  it('does not present onboarding when the inventory failed to load', () => {
    expect(shouldShowFirstRunEmptyState([], true)).toBe(false)
  })

  it('shows onboarding when there are no active nodes', () => {
    expect(shouldShowFirstRunEmptyState([], false)).toBe(true)
    expect(shouldShowFirstRunEmptyState([{ active: false }], false)).toBe(true)
  })

  it('keeps the normal home page when an active node exists', () => {
    expect(
      shouldShowFirstRunEmptyState(
        [{ active: false }, { active: true }],
        false,
      ),
    ).toBe(false)
  })
})
