import { afterEach, expect, it, vi } from 'vitest'

import {
  getSnapshotNotices,
  hideNotice,
  NOTICE_MERGE_WINDOW_MS,
  showNotice,
} from './notice-service'

afterEach(() => {
  getSnapshotNotices().forEach(({ id }) => hideNotice(id))
  vi.useRealTimers()
})

it('merges identical notices during the sixty-second window', () => {
  vi.useFakeTimers()

  const first = showNotice.info('same notice', 0)
  const second = showNotice.info('same notice', 0)

  expect(second).toBe(first)
  expect(getSnapshotNotices()).toHaveLength(1)
  expect(getSnapshotNotices()[0].repeatCount).toBe(2)
})

it('keeps identical notices separate after the merge window', () => {
  vi.useFakeTimers()

  showNotice.warning('same notice', 0)
  vi.advanceTimersByTime(NOTICE_MERGE_WINDOW_MS)
  showNotice.warning('same notice', 0)

  expect(getSnapshotNotices()).toHaveLength(2)
})

it('does not merge notices with different severities', () => {
  showNotice.info('same notice', 0)
  showNotice.error('same notice', 0)

  expect(getSnapshotNotices()).toHaveLength(2)
})
