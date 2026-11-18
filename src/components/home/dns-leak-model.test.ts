import { expect, test } from 'vitest'

import {
  evaluateDnsLeak,
  parseResolverIp,
  parseResolverLocation,
} from './dns-leak-model'

test('no resolver means the check cannot conclude anything', () => {
  expect(
    evaluateDnsLeak({
      exitCountryCode: 'JP',
      resolverCountryCode: 'JP',
      resolverIp: '',
    }),
  ).toBe('unknown')
})

test('a missing country on either side is not a leak', () => {
  expect(evaluateDnsLeak({ resolverIp: '1.1.1.1' })).toBe('unknown')
  expect(
    evaluateDnsLeak({
      resolverIp: '1.1.1.1',
      exitCountryCode: 'JP',
    }),
  ).toBe('unknown')
  expect(
    evaluateDnsLeak({
      resolverIp: '1.1.1.1',
      resolverCountryCode: 'JP',
    }),
  ).toBe('unknown')
})

test('resolver and exit in the same country is not a leak', () => {
  expect(
    evaluateDnsLeak({
      exitCountryCode: 'JP',
      resolverIp: '203.0.113.9',
      resolverCountryCode: 'JP',
    }),
  ).toBe('ok')
})

test('country codes compare case-insensitively', () => {
  expect(
    evaluateDnsLeak({
      exitCountryCode: 'jp',
      resolverIp: '203.0.113.9',
      resolverCountryCode: 'JP',
    }),
  ).toBe('ok')
})

test('a resolver in another country is reported as a possible leak', () => {
  expect(
    evaluateDnsLeak({
      exitCountryCode: 'JP',
      resolverIp: '198.51.100.7',
      resolverCountryCode: 'CN',
    }),
  ).toBe('mismatch')
})

test('the resolver ip is read from the dns object', () => {
  expect(
    parseResolverIp({
      dns: { ip: '203.0.113.9', geo: '🇯🇵 Japan' },
      edns: { ip: '203.0.113.10' },
    }),
  ).toBe('203.0.113.9')
})

test('a malformed resolver payload yields an empty ip', () => {
  expect(parseResolverIp(null)).toBe('')
  expect(parseResolverIp('nope')).toBe('')
  expect(parseResolverIp({})).toBe('')
  expect(parseResolverIp({ dns: { ip: 42 } })).toBe('')
})

test('the resolver location is read from the lookup response', () => {
  expect(
    parseResolverLocation({
      success: true,
      country_code: 'JP',
      country: 'Japan',
    }),
  ).toEqual({ countryCode: 'JP', country: 'Japan' })
})

test('a failed lookup does not become a location', () => {
  expect(
    parseResolverLocation({ success: false, message: 'reserved range' }),
  ).toEqual({ countryCode: '', country: '' })
  expect(parseResolverLocation(undefined)).toEqual({
    countryCode: '',
    country: '',
  })
})
