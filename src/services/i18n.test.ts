import { describe, expect, it } from 'vitest'

import { FALLBACK_LANGUAGE, resolvePreferredLanguage } from './i18n'

describe('language preference', () => {
  it('follows the system until a manual language is selected', () => {
    expect(resolvePreferredLanguage('system', 'en-US')).toBe('en')
    expect(resolvePreferredLanguage('system', 'zh-CN')).toBe('zh')
    expect(resolvePreferredLanguage('en', 'zh-CN')).toBe('en')
    expect(resolvePreferredLanguage(null, 'en-US')).toBe('en')
    expect(resolvePreferredLanguage('system', 'xx-XX')).toBe('en')
  })

  it('maps Chinese system locale variants to Chinese and other locales to English', () => {
    for (const locale of ['zh', 'zh-CN', 'zh_TW', 'zh-Hant-HK']) {
      expect(resolvePreferredLanguage('system', locale)).toBe('zh')
    }

    for (const locale of ['ru-RU', 'fr-FR', 'xx-XX']) {
      expect(resolvePreferredLanguage('system', locale)).toBe('en')
    }

    expect(resolvePreferredLanguage('system', '')).toBe('en')
  })

  it('keeps an explicit supported language authoritative', () => {
    expect(resolvePreferredLanguage('ru', 'zh-CN')).toBe('ru')
    expect(resolvePreferredLanguage('zhtw', 'en-US')).toBe('zhtw')
  })

  it('uses English for i18next fallback resources', () => {
    expect(FALLBACK_LANGUAGE).toBe('en')
  })
})
