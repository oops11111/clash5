/**
 * Home dashboard card visibility.
 *
 * `home_cards` is stored as an untyped JSON object and rewritten as a whole by
 * the settings dialog. A user who saved the dialog before a card existed has an
 * object without that key, so merging per key—not replacing the object—is what
 * keeps newly added cards visible on upgrade.
 */
export const HOME_CARD_DEFAULTS = {
  profile: true,
  proxy: true,
  network: true,
  mode: true,
  traffic: true,
  perAppProxy: true,
  clashinfo: true,
  systeminfo: true,
  test: true,
  ip: true,
} as const

export type HomeCardsSettings = Record<string, boolean>

export function resolveHomeCards(stored: unknown): HomeCardsSettings {
  const merged: HomeCardsSettings = { ...HOME_CARD_DEFAULTS }
  if (typeof stored !== 'object' || stored === null || Array.isArray(stored)) {
    return merged
  }
  for (const [key, value] of Object.entries(stored)) {
    // Ignore non-boolean junk so a malformed config cannot hide every card.
    if (typeof value === 'boolean') merged[key] = value
  }
  return merged
}
