import { appStorage, canUseAppStorage } from '@/services/storage'

export type WebdavStatus = 'unknown' | 'ready' | 'failed'

interface WebdavStatusCache {
  signature: string
  status: WebdavStatus
  updatedAt: number
}

const WEBDAV_STATUS_KEY = 'webdav_status_cache'

export const buildWebdavSignature = (
  verge?: Pick<
    IVergeConfig,
    'webdav_url' | 'webdav_username' | 'webdav_password'
  > | null,
) => {
  const url = verge?.webdav_url?.trim() ?? ''
  const username = verge?.webdav_username?.trim() ?? ''
  const password = verge?.webdav_password ?? ''

  if (!url && !username && !password) return ''

  return JSON.stringify([url, username, password])
}

export const getWebdavStatus = (signature: string): WebdavStatus => {
  if (!signature || !canUseAppStorage()) return 'unknown'

  const raw = appStorage.getItem(WEBDAV_STATUS_KEY)
  if (!raw) return 'unknown'

  try {
    const data = JSON.parse(raw) as Partial<WebdavStatusCache>
    if (!data || data.signature !== signature) return 'unknown'
    return data.status === 'ready' || data.status === 'failed'
      ? data.status
      : 'unknown'
  } catch {
    return 'unknown'
  }
}

export const setWebdavStatus = (signature: string, status: WebdavStatus) => {
  if (!signature || !canUseAppStorage()) return

  const payload: WebdavStatusCache = {
    signature,
    status,
    updatedAt: Date.now(),
  }

  appStorage.setItem(WEBDAV_STATUS_KEY, JSON.stringify(payload))
}
