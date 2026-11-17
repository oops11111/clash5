import { expect, test } from 'vitest'

import { HOME_CARD_DEFAULTS, resolveHomeCards } from './home-cards'

test('a missing config yields every card enabled', () => {
  expect(resolveHomeCards(null)).toEqual(HOME_CARD_DEFAULTS)
  expect(resolveHomeCards(undefined)).toEqual(HOME_CARD_DEFAULTS)
})

test('a card added after the user saved settings stays visible', () => {
  // Saved before perAppProxy existed: the key is absent from the stored object.
  const stored = {
    profile: true,
    proxy: false,
    network: true,
    mode: true,
    traffic: true,
    clashinfo: true,
    systeminfo: true,
    test: true,
    ip: true,
  }

  expect(resolveHomeCards(stored).perAppProxy).toBe(true)
})

test('an explicit false survives the merge', () => {
  expect(resolveHomeCards({ perAppProxy: false }).perAppProxy).toBe(false)
  expect(resolveHomeCards({ proxy: false }).proxy).toBe(false)
})

test('a malformed value falls back to the default instead of hiding the card', () => {
  expect(resolveHomeCards({ perAppProxy: 'no' }).perAppProxy).toBe(true)
  expect(resolveHomeCards({ proxy: 0 }).proxy).toBe(true)
})

test('non-object values are ignored', () => {
  expect(resolveHomeCards('enabled')).toEqual(HOME_CARD_DEFAULTS)
  expect(resolveHomeCards(['profile'])).toEqual(HOME_CARD_DEFAULTS)
})

test('unknown keys are preserved so a downgrade does not lose them', () => {
  expect(resolveHomeCards({ legacyCard: false }).legacyCard).toBe(false)
})
