export const DNS_PRIVACY_DEFAULTS = {
  ipv6: false,
  'respect-rules': true,
} as const

export function resolveDnsPrivacySettings(
  config: { ipv6?: boolean; 'respect-rules'?: boolean } = {},
) {
  return {
    ipv6: config.ipv6 ?? DNS_PRIVACY_DEFAULTS.ipv6,
    respectRules:
      config['respect-rules'] ?? DNS_PRIVACY_DEFAULTS['respect-rules'],
  }
}
