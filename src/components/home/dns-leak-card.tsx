import { RadarOutlined, RefreshOutlined } from '@mui/icons-material'
import {
  Box,
  Chip,
  CircularProgress,
  IconButton,
  Tooltip,
  Typography,
} from '@mui/material'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { getIpInfo } from '@/services/api'

import {
  evaluateDnsLeak,
  parseResolverIp,
  parseResolverLocation,
  type DnsLeakVerdict,
} from './dns-leak-model'
import { EnhancedCard } from './enhanced-card'

const REQUEST_TIMEOUT_MS = 8_000
// Echoes the resolver that reached the service, not the requester's address.
const RESOLVER_ECHO_URL = 'https://edns.ip-api.com/json'
const LOOKUP_URL = 'https://ipwho.is/'

interface DnsLeakSample {
  exitIp: string
  exitCountry: string
  exitCountryCode: string
  resolverIp: string
  resolverCountry: string
  resolverCountryCode: string
  checkedAt: number
}

const fetchJson = async (url: string): Promise<unknown> => {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS)
  try {
    const response = await fetch(url, { signal: controller.signal })
    if (!response.ok) {
      throw new Error(`${url} responded ${response.status}`)
    }
    return (await response.json()) as unknown
  } finally {
    clearTimeout(timer)
  }
}

const Row = ({ label, value }: { label: string; value: string }) => (
  <Box sx={{ display: 'flex', alignItems: 'baseline', gap: 1, mb: 0.5 }}>
    <Typography
      variant="body2"
      color="text.secondary"
      sx={{ flexShrink: 0, minWidth: 84 }}
    >
      {label}
    </Typography>
    <Typography
      variant="body2"
      sx={{ wordBreak: 'break-all', fontFamily: 'monospace' }}
    >
      {value}
    </Typography>
  </Box>
)

export const DnsLeakCard = () => {
  const { t } = useTranslation()
  const [sample, setSample] = useState<DnsLeakSample | null>(null)
  const [loading, setLoading] = useState(false)
  const [failed, setFailed] = useState(false)
  // Guards against a slow check landing after unmount or after a newer one.
  const requestIdRef = useRef(0)

  const runCheck = useCallback(async () => {
    const id = requestIdRef.current + 1
    requestIdRef.current = id
    setLoading(true)
    setFailed(false)
    try {
      const exit = await getIpInfo()
      const resolverIp = parseResolverIp(await fetchJson(RESOLVER_ECHO_URL))
      if (!resolverIp) throw new Error('resolver echo carried no address')
      const resolver = parseResolverLocation(
        await fetchJson(`${LOOKUP_URL}${resolverIp}`),
      )
      if (requestIdRef.current !== id) return
      setSample({
        exitIp: exit.ip,
        exitCountry: exit.country,
        exitCountryCode: exit.country_code,
        resolverIp,
        resolverCountry: resolver.country,
        resolverCountryCode: resolver.countryCode,
        checkedAt: Date.now(),
      })
    } catch {
      if (requestIdRef.current === id) setFailed(true)
    } finally {
      if (requestIdRef.current === id) setLoading(false)
    }
  }, [])

  useEffect(() => {
    void runCheck()
    return () => {
      // Invalidate any in-flight check so it cannot set state after unmount.
      requestIdRef.current += 1
    }
  }, [runCheck])

  const verdict: DnsLeakVerdict = sample
    ? evaluateDnsLeak({
        exitCountryCode: sample.exitCountryCode,
        resolverIp: sample.resolverIp,
        resolverCountryCode: sample.resolverCountryCode,
      })
    : 'unknown'

  const verdictColor =
    verdict === 'ok'
      ? 'success'
      : verdict === 'mismatch'
        ? 'warning'
        : 'default'

  return (
    <EnhancedCard
      title={t('home.components.dnsLeak.title')}
      icon={<RadarOutlined />}
      iconColor="info"
      action={
        <Tooltip title={t('home.components.dnsLeak.actions.refresh')}>
          <span>
            <IconButton
              size="small"
              onClick={() => void runCheck()}
              disabled={loading}
              aria-label={t('home.components.dnsLeak.actions.refresh')}
            >
              {loading ? (
                <CircularProgress size={16} />
              ) : (
                <RefreshOutlined fontSize="small" />
              )}
            </IconButton>
          </span>
        </Tooltip>
      }
    >
      {failed ? (
        <Box sx={{ py: 1 }}>
          <Typography variant="body2" color="error.main">
            {t('home.components.dnsLeak.errors.unavailable')}
          </Typography>
          <Typography variant="caption" color="text.secondary">
            {t('home.components.dnsLeak.errors.hint')}
          </Typography>
        </Box>
      ) : sample ? (
        <Box sx={{ py: 0.5 }}>
          <Row
            label={t('home.components.dnsLeak.fields.exitIp')}
            value={sample.exitIp || t('home.components.dnsLeak.fields.unknown')}
          />
          <Row
            label={t('home.components.dnsLeak.fields.exitRegion')}
            value={
              sample.exitCountry || t('home.components.dnsLeak.fields.unknown')
            }
          />
          <Row
            label={t('home.components.dnsLeak.fields.resolverIp')}
            value={sample.resolverIp}
          />
          <Row
            label={t('home.components.dnsLeak.fields.resolverRegion')}
            value={
              sample.resolverCountry ||
              t('home.components.dnsLeak.fields.unknown')
            }
          />
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mt: 1 }}>
            <Chip
              size="small"
              color={verdictColor}
              label={t(`home.components.dnsLeak.verdict.${verdict}`)}
            />
            <Typography variant="caption" color="text.secondary">
              {t(`home.components.dnsLeak.hints.${verdict}`)}
            </Typography>
          </Box>
        </Box>
      ) : (
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, py: 2 }}>
          <CircularProgress size={16} />
          <Typography variant="body2" color="text.secondary">
            {t('home.components.dnsLeak.checking')}
          </Typography>
        </Box>
      )}
    </EnhancedCard>
  )
}
