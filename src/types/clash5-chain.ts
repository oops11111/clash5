import type { Clash5NodeRecord } from './clash5-node'

export interface ChainNodeRef {
  source: Clash5NodeRecord['source']
  stable_id: string
}

export interface Clash5Chain {
  schema_version: 1
  id: string
  name: string
  profile_id: string | null
  front: ChainNodeRef
  landing: ChainNodeRef
  enabled: boolean
  exit_policy: { mode: 'warn' } | { mode: 'switch'; fallback_chain_id: string }
}

export interface ChainNodeOption {
  reference: ChainNodeRef
  display_name: string
  kind: string
  server: string
  port: number
  source_name: string | null
}

export interface ChainWorkspace {
  profile_id: string | null
  chains: Clash5Chain[]
  nodes: ChainNodeOption[]
  policy_groups: string[]
}

interface ExitObservation {
  ip: string
  elapsed_ms: number
}

export type ExitIpResult =
  | { status: 'disabled' }
  | { status: 'healthy'; observation: ExitObservation }
  | { status: 'warning'; code: string }
  | {
      status: 'switched'
      fallback_chain_id: string
      observation: ExitObservation
      selection_persisted: boolean
    }
