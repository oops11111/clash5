/**
 * DNS leak check.
 *
 * The card asks a public resolver-echo endpoint which DNS server performed the
 * lookup, then looks that server up. Comparing the resolver's country with the
 * proxy exit's country is a heuristic, not proof: a leak shows up as a resolver
 * in a different region, but a CDN resolver in a neighbouring country can also
 * look "different". The verdict is therefore advisory and the card always shows
 * both sides so the user can judge.
 */
export type DnsLeakVerdict = 'ok' | 'mismatch' | 'unknown'

export interface DnsLeakInput {
  exitCountryCode?: string
  resolverIp?: string
  resolverCountryCode?: string
}

export interface ResolverLocation {
  countryCode: string
  country: string
}

export const evaluateDnsLeak = ({
  exitCountryCode,
  resolverIp,
  resolverCountryCode,
}: DnsLeakInput): DnsLeakVerdict => {
  if (!resolverIp || !exitCountryCode || !resolverCountryCode) return 'unknown'
  return exitCountryCode.trim().toUpperCase() ===
    resolverCountryCode.trim().toUpperCase()
    ? 'ok'
    : 'mismatch'
}

const asRecord = (value: unknown): Record<string, unknown> | null =>
  typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null

const asString = (value: unknown): string =>
  typeof value === 'string' ? value.trim() : ''

/**
 * `https://edns.ip-api.com/json` answers with `{ dns: { ip, geo }, edns: … }`
 * where `dns.ip` is the resolver that reached the service.
 */
export const parseResolverIp = (payload: unknown): string => {
  const dns = asRecord(asRecord(payload)?.dns)
  return asString(dns?.ip)
}

/**
 * `https://api.ip.sb/geoip/<ip>` answers with `{ country_code, country, … }`.
 * Some providers answer a failure body instead; anything without a country code
 * is treated as "unknown" rather than as a location.
 */
export const parseResolverLocation = (payload: unknown): ResolverLocation => {
  const record = asRecord(payload)
  if (!record || record.success === false) {
    return { countryCode: '', country: '' }
  }
  return {
    countryCode: asString(record.country_code),
    country: asString(record.country),
  }
}
