import type { Clash5NodeRecord } from './clash5-node'

export type Clash5PluginBindingTarget =
  | {
      kind: 'node'
      source: Clash5NodeRecord['source']
      stable_id: string
    }
  | { kind: 'chain'; chain_id: string }

export interface Clash5PluginBinding {
  id: string
  plugin_id: string
  target: Clash5PluginBindingTarget
  priority: number
  enabled: boolean
  orphan: boolean
  disappeared_at: string | null
}

export interface Clash5PluginBindingInput {
  id: string
  plugin_id: string
  target: Clash5PluginBindingTarget
  priority: number
  enabled: boolean
  config: Record<string, unknown>
}

export type Clash5PluginBindingBatchInput = Clash5PluginBindingInput[]

type Clash5PluginProvenance = Record<
  string,
  | { kind: 'profile' }
  | {
      kind: 'plugin'
      plugin_id: string
      binding_id: string
      priority: number
    }
>

interface Clash5PluginConflict {
  path: string
  winner_plugin_id: string
  winner_binding_id: string
  winner_priority: number
  loser_plugin_id: string
  loser_binding_id: string
  loser_priority: number
}

export interface Clash5PluginEffectiveConfig {
  config: Record<string, unknown> | null
  provenance: Clash5PluginProvenance
  conflicts: Clash5PluginConflict[]
}

export interface Clash5PluginSummary {
  id: string
  version: string
  name: string
  description: string
  permissions: string[]
  writable_paths: string[]
  signer_fingerprint: string
  enabled: boolean
  has_previous: boolean
}

export interface Clash5PluginAuditEvent {
  id: number
  event: string
  created_at: string
  details: Record<string, unknown>
}

export interface Clash5PluginPackagePreview {
  staging_id: string
  id: string
  version: string
  name: string
  description: string
  permissions: string[]
  writable_paths: string[]
  signer_fingerprint: string
  signer: string
}

interface Clash5CatalogEntry {
  id: string
  version: string
  name: string
  description: string
  file: string
  sha256: string
}

export interface Clash5CatalogView {
  id: string
  name: string
  url: string
  signer_fingerprint: string
  plugins: Clash5CatalogEntry[]
}

export interface Clash5CatalogPreview {
  url: string
  name: string
  signer: string
  signer_fingerprint: string
  plugin_count: number
}
