export const DEFAULT_DNS_CONFIG = {
  enable: true,
  listen: '127.0.0.1:53',
  'enhanced-mode': 'fake-ip' as 'fake-ip' | 'redir-host',
  'fake-ip-range': '198.18.0.1/16',
  'fake-ip-range6': 'fdfe:dcba:9876::1/64',
  'fake-ip-filter-mode': 'blacklist' as 'blacklist' | 'whitelist',
  'prefer-h3': false,
  'respect-rules': true,
  'use-hosts': false,
  'use-system-hosts': false,
  ipv6: false,
  'fake-ip-filter': [
    '*.lan',
    '*.local',
    '*.arpa',
    'time.*.com',
    'ntp.*.com',
    'time.*.com',
    '+.market.xiaomi.com',
    'localhost.ptlogin2.qq.com',
    '*.msftncsi.com',
    'www.msftconnecttest.com',
  ],
  'default-nameserver': ['223.6.6.6', '2400:3200::1'],
  nameserver: ['https://doh.pub/dns-query', 'https://dns.alidns.com/dns-query'],
  fallback: [],
  'nameserver-policy': {},
  // Node hostnames use the domestic encrypted resolvers before rule routing.
  'proxy-server-nameserver': [
    'https://doh.pub/dns-query',
    'https://dns.alidns.com/dns-query',
  ],
  'direct-nameserver': [],
  'direct-nameserver-follow-policy': false,
  'fallback-filter': {
    geoip: true,
    'geoip-code': 'CN',
    ipcidr: ['240.0.0.0/4', '0.0.0.0/32'],
    domain: ['+.google.com', '+.facebook.com', '+.youtube.com'],
  },
}

export function resolveDnsBooleanDefaults(dnsConfig: {
  'respect-rules'?: boolean | null
  ipv6?: boolean | null
} = {}) {
  return {
    respectRules:
      dnsConfig['respect-rules'] ?? DEFAULT_DNS_CONFIG['respect-rules'],
    ipv6: dnsConfig.ipv6 ?? DEFAULT_DNS_CONFIG.ipv6,
  }
}
