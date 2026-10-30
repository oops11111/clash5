import { describe, expect, test } from 'vitest'

import { parseTextRules, previewTextRules } from './text-rules'

describe('text rules', () => {
  test('reads valid lines and counts the rest as invalid', () => {
    const parsed = parseTextRules(
      [
        '# comment',
        'domain-suffix,ads.example.com',
        'IP-CIDR,10.0.0.0/8,no-resolve',
        'UNKNOWN,x',
        'DOMAIN-KEYWORD',
        '',
      ].join('\n'),
    )
    expect(parsed.entries).toEqual([
      { kind: 'DOMAIN-SUFFIX', payload: 'ads.example.com' },
      { kind: 'IP-CIDR', payload: '10.0.0.0/8' },
    ])
    expect(parsed.invalid).toBe(2)
  })

  test('previews new and merged entries against existing rules', () => {
    const parsed = parseTextRules(
      [
        'DOMAIN-SUFFIX,github.com',
        'DOMAIN-SUFFIX,ads.example.com',
        'DOMAIN-SUFFIX,ads.example.com',
        'DOMAIN-KEYWORD,tracker',
      ].join('\n'),
    )
    const preview = previewTextRules(parsed, [
      { kind: 'DOMAIN-SUFFIX', payload: 'github.com' },
    ])
    expect(preview).toEqual({ valid: 4, invalid: 0, added: 2, merged: 2 })
  })
})
