import { describe, expect, it } from 'vitest'

import { resolvePreferredLanguage } from './i18n'

describe('language preference', () => {
  it('follows the system until a manual language is selected', () => {
    expect(resolvePreferredLanguage('system', 'en-US')).toBe('en')
    expect(resolvePreferredLanguage('system', 'zh-CN')).toBe('zh')
    expect(resolvePreferredLanguage('en', 'zh-CN')).toBe('en')
    expect(resolvePreferredLanguage(null, 'en-US')).toBe('en')
    expect(resolvePreferredLanguage('system', 'xx-XX')).toBe('en')
  })

  it('uses simplified Chinese for every Chinese system locale', () => {
    expect(resolvePreferredLanguage('system', 'zh-TW')).toBe('zh')
    expect(resolvePreferredLanguage('system', 'zh-HK')).toBe('zh')
    expect(resolvePreferredLanguage('system', 'zh-Hans-CN')).toBe('zh')
  })

  it('keeps an explicit language choice authoritative', () => {
    expect(resolvePreferredLanguage('zhtw', 'zh-CN')).toBe('zhtw')
    expect(resolvePreferredLanguage('jp', 'zh-CN')).toBe('jp')
  })
})
