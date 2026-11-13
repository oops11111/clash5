import { describe, expect, it } from 'vitest'

import {
  DNS_PRIVACY_DEFAULTS,
  resolveDnsPrivacySettings,
} from './dns-privacy-defaults'

describe('DNS privacy defaults', () => {
  it('disables IPv6 and routes DNS by rules by default', () => {
    expect(DNS_PRIVACY_DEFAULTS).toEqual({
      ipv6: false,
      'respect-rules': true,
    })
    expect(resolveDnsPrivacySettings()).toEqual({
      ipv6: false,
      respectRules: true,
    })
  })

  it('preserves explicit user choices, including false values', () => {
    expect(
      resolveDnsPrivacySettings({ ipv6: true, 'respect-rules': false }),
    ).toEqual({ ipv6: true, respectRules: false })
    expect(
      resolveDnsPrivacySettings({ ipv6: false, 'respect-rules': true }),
    ).toEqual({ ipv6: false, respectRules: true })
  })
})
