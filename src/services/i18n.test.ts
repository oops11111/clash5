import { describe, expect, it } from 'vitest'

import { resolvePreferredLanguage } from './i18n'

describe('language preference', () => {
  it('follows the system until a manual language is selected', () => {
    expect(resolvePreferredLanguage('system', 'en-US')).toBe('en')
    expect(resolvePreferredLanguage('system', 'zh-CN')).toBe('zh')
    expect(resolvePreferredLanguage('en', 'zh-CN')).toBe('en')
    expect(resolvePreferredLanguage(null, 'en-US')).toBe('en')
    expect(resolvePreferredLanguage('system', 'xx-XX')).toBe('zh')
  })
})
