import { afterEach, describe, expect, it, vi } from 'vitest'

import { appStorage, STORAGE_PREFIX } from './storage'

const createStorage = () => {
  const values = new Map<string, string>()
  return {
    values,
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  }
}

afterEach(() => vi.unstubAllGlobals())

describe('namespaced app storage', () => {
  it('migrates a legacy value to the v1 namespace on first read', () => {
    const storage = createStorage()
    storage.values.set('legacy-key', 'legacy-value')
    vi.stubGlobal('window', { localStorage: storage })

    expect(appStorage.getItem('legacy-key')).toBe('legacy-value')
    expect(storage.values.get(`${STORAGE_PREFIX}legacy-key`)).toBe(
      'legacy-value',
    )
    expect(storage.values.has('legacy-key')).toBe(false)
  })

  it('writes and removes only namespaced application keys', () => {
    const storage = createStorage()
    vi.stubGlobal('window', { localStorage: storage })

    appStorage.setItem('setting', 'value')
    expect(storage.values.get(`${STORAGE_PREFIX}setting`)).toBe('value')
    expect(storage.values.has('setting')).toBe(false)

    appStorage.removeItem('setting')
    expect(storage.values.size).toBe(0)
  })

  it('does not access browser storage during non-window execution', () => {
    expect(appStorage.getItem('server-only')).toBeNull()
  })
})
