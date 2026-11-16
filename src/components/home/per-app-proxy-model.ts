import type {
  PerAppProxyMapping,
  ProcessProxyStatus,
} from '@/services/tauri-commands.generated'

/** Runtime state shown on a per-app block. */
export type PerAppRuntimeStatus =
  | 'idle'
  | 'active'
  | 'failed'
  | 'recovery_required'
  | 'unavailable'

/** One block on the per-app proxy card. */
export interface PerAppEntry {
  processName: string
  processPath: string
  /** Latest connection start time for this process, 0 when only a mapping exists. */
  lastSeen: number
  connectionCount: number
  mapping?: PerAppProxyMapping
  runtimeStatus: PerAppRuntimeStatus
  chainId?: string
  errorDetail?: string
}

/** Why a block cannot be toggled right now. */
export type PerAppGateReason =
  | 'nonWindows'
  | 'noService'
  | 'tunActive'
  | 'singleSession'
  | 'unavailable'
  | 'busy'

export interface DeriveEntriesInput {
  connections: IConnectionsItem[]
  mappings: PerAppProxyMapping[]
  status: ProcessProxyStatus
}

export interface GateEntryInput {
  entry: PerAppEntry
  /** Result of getSystem(); anything but 'windows' is unsupported. */
  platform: string
  serviceUsable: boolean
  tunActive: boolean
  /** True when some process proxy session is live. */
  activeSession: boolean
  /** True while any attach/detach call is in flight. */
  busy: boolean
}

/** Stable identity for a mapping or entry: name plus path when known. */
export const entryKey = (entry: {
  processName: string
  processPath?: string
}): string =>
  entry.processPath
    ? `${entry.processName}|${entry.processPath}`
    : entry.processName

const runtimeStatusFor = (
  mapping: PerAppProxyMapping | undefined,
  status: ProcessProxyStatus,
): { runtimeStatus: PerAppRuntimeStatus; errorDetail?: string } => {
  if (status.status === 'unavailable') {
    return { runtimeStatus: 'unavailable' }
  }
  if (status.status === 'recovery_required') {
    return { runtimeStatus: 'recovery_required', errorDetail: status.detail }
  }
  if (status.status === 'active') {
    // Only the mapping whose chain matches the live session is being proxied;
    // the backend keeps a single session at a time.
    const matchesLiveChain =
      mapping?.enabled === true && status.chain_id === mapping.chainId
    return { runtimeStatus: matchesLiveChain ? 'active' : 'idle' }
  }
  return { runtimeStatus: 'idle' }
}

/**
 * Merge connection metadata, stored mappings and live session status into the
 * rows shown on the card. Processes with recent traffic and mappings without
 * traffic both appear; a mapping never hides traffic from another process.
 */
export const derivePerAppEntries = ({
  connections,
  mappings,
  status,
}: DeriveEntriesInput): PerAppEntry[] => {
  const byKey = new Map<string, PerAppEntry>()
  const mappingByKey = new Map<string, PerAppProxyMapping>()
  for (const mapping of mappings) {
    mappingByKey.set(entryKey(mapping), mapping)
  }

  for (const connection of connections) {
    const processName = connection.metadata.process?.trim()
    if (!processName) continue

    const processPath = connection.metadata.processPath?.trim() ?? ''
    const key = entryKey({ processName, processPath })
    const startTime = Date.parse(connection.start) || 0
    const existing = byKey.get(key)

    if (existing) {
      existing.connectionCount += 1
      if (startTime > existing.lastSeen) existing.lastSeen = startTime
      continue
    }

    const mapping = mappingByKey.get(key)
    const { runtimeStatus, errorDetail } = runtimeStatusFor(mapping, status)
    byKey.set(key, {
      processName,
      processPath,
      lastSeen: startTime,
      connectionCount: 1,
      mapping,
      runtimeStatus,
      chainId: mapping?.chainId,
      errorDetail,
    })
  }

  // Mappings with no recent traffic still need a block, otherwise a configured
  // app would silently vanish from the card.
  for (const mapping of mappings) {
    const key = entryKey(mapping)
    if (byKey.has(key)) continue

    const { runtimeStatus, errorDetail } = runtimeStatusFor(mapping, status)
    byKey.set(key, {
      processName: mapping.processName,
      processPath: mapping.processPath ?? '',
      lastSeen: 0,
      connectionCount: 0,
      mapping,
      runtimeStatus,
      chainId: mapping.chainId,
      errorDetail,
    })
  }

  return Array.from(byKey.values()).sort((left, right) => {
    const leftEnabled = left.mapping?.enabled === true
    const rightEnabled = right.mapping?.enabled === true
    if (leftEnabled !== rightEnabled) return leftEnabled ? -1 : 1
    return right.lastSeen - left.lastSeen
  })
}

/**
 * Decide whether a block can be toggled. Returning a reason keeps the UI from
 * offering an action the backend would refuse.
 */
export const gateEntry = ({
  entry,
  platform,
  serviceUsable,
  tunActive,
  activeSession,
  busy,
}: GateEntryInput): PerAppGateReason | null => {
  if (platform !== 'windows') return 'nonWindows'
  if (!serviceUsable) return 'noService'
  if (tunActive) return 'tunActive'
  if (activeSession && entry.runtimeStatus !== 'active') return 'singleSession'
  if (entry.runtimeStatus === 'unavailable') return 'unavailable'
  if (busy) return 'busy'
  return null
}

/**
 * A stored chain is only "missing" once the chain list actually loaded; a chain
 * service outage must not relabel every block as invalid.
 */
export const isChainMissing = (
  entry: Pick<PerAppEntry, 'chainId'>,
  knownChainIds: ReadonlySet<string>,
  chainsLoaded: boolean,
): boolean =>
  chainsLoaded &&
  entry.chainId !== undefined &&
  entry.chainId !== '' &&
  !knownChainIds.has(entry.chainId)
