import { expect, it, vi } from 'vitest'

const check = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/plugin-updater', () => ({ check }))

import { checkUpdateSafe, UPDATER_AVAILABLE } from './update'

it('does not invoke the updater when the release channel is disabled', async () => {
  expect(UPDATER_AVAILABLE).toBe(false)
  expect(await checkUpdateSafe()).toBeNull()
  expect(check).not.toHaveBeenCalled()
})
