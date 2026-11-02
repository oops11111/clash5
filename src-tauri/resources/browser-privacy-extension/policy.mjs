export const profiles = Object.freeze({
  unchanged: null,
  new_york: { languages: ['en-US', 'en'], timeZone: 'America/New_York', header: 'en-US,en;q=0.9' },
  tokyo: { languages: ['ja-JP', 'ja'], timeZone: 'Asia/Tokyo', header: 'ja-JP,ja;q=0.9' },
})

export function normalizeHost(input) {
  const value = String(input).trim().toLowerCase()
  if (!value || /[\s/?#@:*]/.test(value)) throw new Error('Enter a hostname without a scheme, port, or path.')
  const host = value.endsWith('.') ? value.slice(0, -1) : value
  if (host.length > 253 || !host.split('.').every((part) =>
    part.length > 0 && part.length <= 63 && /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(part)
  )) throw new Error('Invalid hostname.')
  return host
}

export function normalizeRules(input) {
  if (!Array.isArray(input) || input.length > 100) throw new Error('Maximum 100 site rules.')
  const seen = new Set()
  return input.map((entry) => {
    const host = normalizeHost(entry.host)
    if (!Object.hasOwn(profiles, entry.profile)) throw new Error('Invalid profile.')
    if (seen.has(host)) throw new Error(`Duplicate hostname: ${host}`)
    seen.add(host)
    return { host, profile: entry.profile }
  }).sort((a, b) => b.host.length - a.host.length || a.host.localeCompare(b.host))
}

export function profileForHost(host, rules) {
  const hostname = normalizeHost(host)
  return rules.find((entry) => hostname === entry.host || hostname.endsWith(`.${entry.host}`))?.profile ?? 'unchanged'
}

function matchPatterns(host) {
  return [`*://${host}/*`, `*://*.${host}/*`]
}

export function buildRules(rules) {
  const scripts = []
  const headers = []
  let id = 1
  for (const entry of rules) {
    if (entry.profile === 'unchanged') continue
    const children = rules.filter((other) => other.host !== entry.host && other.host.endsWith(`.${entry.host}`))
    scripts.push({
      id: `clash5-profile-${id}`,
      matches: matchPatterns(entry.host),
      excludeMatches: children.flatMap((other) => matchPatterns(other.host)),
      js: ['page-shim.js', `profile-${entry.profile}.js`],
      runAt: 'document_start',
      world: 'MAIN',
      allFrames: true,
      persistAcrossSessions: true,
    })
    const action = { type: 'modifyHeaders', requestHeaders: [{ header: 'accept-language', operation: 'set', value: profiles[entry.profile].header }] }
    headers.push({ id: id++, priority: entry.host.length, action, condition: {
      requestDomains: [entry.host],
      excludedRequestDomains: children.map((other) => other.host),
      resourceTypes: ['main_frame', 'sub_frame'],
    } })
    headers.push({ id: id++, priority: entry.host.length, action, condition: {
      initiatorDomains: [entry.host],
      excludedInitiatorDomains: children.map((other) => other.host),
      excludedResourceTypes: ['main_frame', 'sub_frame'],
    } })
  }
  return { scripts, headers }
}
