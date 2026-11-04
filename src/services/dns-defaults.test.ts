import { describe, expect, it } from 'vitest'

import {
  DEFAULT_DNS_CONFIG,
  resolveDnsBooleanDefaults,
} from './dns-defaults'

describe('DNS privacy defaults', () => {
  it('disables IPv6 and routes ordinary DNS according to rules by default', () => {
    expect(DEFAULT_DNS_CONFIG.ipv6).toBe(false)
    expect(DEFAULT_DNS_CONFIG['respect-rules']).toBe(true)
    expect(DEFAULT_DNS_CONFIG['proxy-server-nameserver']).toEqual([
      'https://doh.pub/dns-query',
      'https://dns.alidns.com/dns-query',
    ])
  })

  it('does not turn an explicit existing opt-in off', () => {
    expect(resolveDnsBooleanDefaults({ ipv6: true, 'respect-rules': false })).toEqual({
      ipv6: true,
      respectRules: false,
    })
  })

  it('uses privacy-safe values when legacy keys are missing', () => {
    expect(resolveDnsBooleanDefaults()).toEqual({
      ipv6: false,
      respectRules: true,
    })
  })
})
