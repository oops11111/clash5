type Clash5NodeSource =
  | { kind: 'manual' }
  | { kind: 'subscription'; provider_id: string }

export interface Clash5NodeRecord {
  stable_id: string
  source: Clash5NodeSource
  display_name: string
  kind: string
  server: string
  port: number
  active: boolean
  duplicate_count: number
  quality: {
    status: 'healthy' | 'slow' | 'failed'
    short_delay_ms: number | null
    handshake_delay_ms: number | null
    checked_at_ms: number
    failure_streak: number
  } | null
}
