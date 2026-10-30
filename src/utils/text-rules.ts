// Mirrors the rule types and line format accepted by the backend (enhance/rules.rs).
const RULE_TYPES = new Set([
  'DOMAIN',
  'DOMAIN-SUFFIX',
  'DOMAIN-KEYWORD',
  'DOMAIN-REGEX',
  'GEOSITE',
  'GEOIP',
  'IP-CIDR',
  'IP-CIDR6',
  'SRC-IP-CIDR',
  'SRC-PORT',
  'DST-PORT',
  'PROCESS-NAME',
  'PROCESS-PATH',
  'RULE-SET',
])

// Runtime rules report their type as a string or as `{ Unknown: string }`.
export const ruleKind = (type: unknown): string => {
  if (typeof type === 'string') return type.toUpperCase()
  if (type && typeof type === 'object' && 'Unknown' in type) {
    return String((type as { Unknown: unknown }).Unknown).toUpperCase()
  }
  return ''
}

export interface TextRuleEntry {
  kind: string
  payload: string
}

export interface ParsedTextRules {
  entries: TextRuleEntry[]
  invalid: number
}

export const parseTextRules = (content: string): ParsedTextRules => {
  const result: ParsedTextRules = { entries: [], invalid: 0 }
  for (const raw of content.split('\n')) {
    const line = raw.trim()
    if (!line || line.startsWith('#')) continue

    const parts = line.split(',').map((part) => part.trim())
    const kind = parts[0].toUpperCase()
    const payload = parts[1] ?? ''
    const flagOk =
      parts.length === 2 ||
      (parts.length === 3 && parts[2].toLowerCase() === 'no-resolve')

    if (flagOk && payload && RULE_TYPES.has(kind)) {
      result.entries.push({ kind, payload })
    } else {
      result.invalid += 1
    }
  }
  return result
}

export interface TextRulePreview {
  valid: number
  invalid: number
  added: number
  merged: number
}

// Entries are compared by type and payload; the policy is set by the rule set's target.
export const previewTextRules = (
  parsed: ParsedTextRules,
  existing: Iterable<TextRuleEntry>,
): TextRulePreview => {
  const present = new Set(
    Array.from(existing, (entry) => `${entry.kind},${entry.payload}`),
  )
  const unique = new Set(
    parsed.entries.map((entry) => `${entry.kind},${entry.payload}`),
  )
  let added = 0
  for (const key of unique) {
    if (!present.has(key)) added += 1
  }
  return {
    valid: parsed.entries.length,
    invalid: parsed.invalid,
    added,
    merged: parsed.entries.length - added,
  }
}
