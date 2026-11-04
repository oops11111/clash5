export const STORAGE_PREFIX = 'clash5:v1:'

const getStorage = (): Storage | null => {
  if (typeof window === 'undefined') return null
  try {
    return window.localStorage
  } catch {
    return null
  }
}

const namespacedKey = (key: string) =>
  key.startsWith(STORAGE_PREFIX) ? key : `${STORAGE_PREFIX}${key}`

export const canUseAppStorage = () => getStorage() !== null

/**
 * Application storage is namespaced to avoid colliding with other WebViews or
 * older Verge instances. A legacy key is read once and moved into the v1
 * namespace so existing user preferences are not silently discarded.
 */
export const appStorage = {
  getItem(key: string): string | null {
    const storage = getStorage()
    if (!storage) return null

    const keyV1 = namespacedKey(key)
    const current = storage.getItem(keyV1)
    if (current !== null || key.startsWith(STORAGE_PREFIX)) return current

    const legacy = storage.getItem(key)
    if (legacy === null) return null
    try {
      storage.setItem(keyV1, legacy)
      storage.removeItem(key)
    } catch {
      // Keep the legacy value if storage is full or becomes unavailable.
    }
    return legacy
  },

  setItem(key: string, value: string) {
    const storage = getStorage()
    if (!storage) return

    const keyV1 = namespacedKey(key)
    storage.setItem(keyV1, value)
    if (!key.startsWith(STORAGE_PREFIX)) storage.removeItem(key)
  },

  removeItem(key: string) {
    const storage = getStorage()
    if (!storage) return

    storage.removeItem(namespacedKey(key))
    if (!key.startsWith(STORAGE_PREFIX)) storage.removeItem(key)
  },
}
