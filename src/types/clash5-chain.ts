import type { Clash5NodeRecord } from './clash5-node'

export interface ChainNodeRef {
  source: Clash5NodeRecord['source']
  stable_id: string
}

export type ChainHopRef =
  | { kind: 'node'; node: ChainNodeRef }
  | { kind: 'pool'; pool_id: string }

export interface NodePoolDef {
  schema_version: 1
  id: string
  name: string
  members: ChainNodeRef[]
}

export interface Clash5Chain {
  schema_version: 2
  id: string
  name: string
  profile_id: string | null
  hops: ChainHopRef[]
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

export type AppMatcherRef =
  | { kind: 'process_name'; name: string }
  | { kind: 'process_path'; path: string }

export type RouteTargetRef =
  | { kind: 'chain'; chain_id: string }
  | { kind: 'direct' }
  | { kind: 'reject' }

export interface AppRouteDef {
  schema_version: 1
  id: string
  name: string
  matchers: AppMatcherRef[]
  target: RouteTargetRef
  enabled: boolean
}

export interface ChainWorkspace {
  profile_id: string | null
  chains: Clash5Chain[]
  pools: NodePoolDef[]
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
