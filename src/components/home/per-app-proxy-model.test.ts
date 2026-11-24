import { expect, test } from 'vitest'

import type {
  PerAppProxyMapping,
  ProcessProxyStatus,
} from '@/services/tauri-commands.generated'

import {
  applyFailures,
  derivePerAppEntries,
  entryKey,
  gateEntry,
  isChainMissing,
  type PerAppEntry,
} from './per-app-proxy-model'

const connection = (
  process: string,
  path: string,
  start: string,
): IConnectionsItem =>
  ({
    id: `${process}-${start}`,
    metadata: {
      network: 'tcp',
      type: 'HTTP',
      host: 'example.com',
      sourceIP: '127.0.0.1',
      sourcePort: '5000',
      destinationPort: '443',
      process,
      processPath: path,
    },
    upload: 0,
    download: 0,
    start,
    chains: [],
    rule: '',
    rulePayload: '',
  }) as IConnectionsItem

const mapping = (
  process: string,
  path: string,
  chainId: string,
  enabled = false,
): PerAppProxyMapping => ({
  processName: process,
  processPath: path,
  chainId,
  enabled,
  order: 0,
})

const idle: ProcessProxyStatus = { status: 'idle' }

const entryOf = (over: Partial<PerAppEntry> = {}): PerAppEntry => ({
  processName: 'chrome.exe',
  processPath: 'C:\\Chrome\\chrome.exe',
  lastSeen: 0,
  connectionCount: 1,
  runtimeStatus: 'idle',
  ...over,
})

// --- derivation -------------------------------------------------------------

test('merges connections of the same process and keeps the latest start', () => {
  const entries = derivePerAppEntries({
    connections: [
      connection(
        'chrome.exe',
        'C:\\Chrome\\chrome.exe',
        '2026-11-16T01:00:00Z',
      ),
      connection(
        'chrome.exe',
        'C:\\Chrome\\chrome.exe',
        '2026-11-16T03:00:00Z',
      ),
    ],
    mappings: [],
    status: idle,
  })

  expect(entries).toHaveLength(1)
  expect(entries[0].connectionCount).toBe(2)
  expect(entries[0].lastSeen).toBe(Date.parse('2026-11-16T03:00:00Z'))
})

test('same process name with different paths stays two blocks', () => {
  const entries = derivePerAppEntries({
    connections: [
      connection('app.exe', 'C:\\one\\app.exe', '2026-11-16T01:00:00Z'),
      connection('app.exe', 'C:\\two\\app.exe', '2026-11-16T02:00:00Z'),
    ],
    mappings: [],
    status: idle,
  })

  expect(entries.map(entryKey)).toEqual([
    'app.exe|C:\\two\\app.exe',
    'app.exe|C:\\one\\app.exe',
  ])
})

test('a mapping without traffic still produces a block', () => {
  const entries = derivePerAppEntries({
    connections: [],
    mappings: [mapping('notepad.exe', 'C:\\notepad.exe', 'chain-a')],
    status: idle,
  })

  expect(entries).toHaveLength(1)
  expect(entries[0]).toMatchObject({
    processName: 'notepad.exe',
    connectionCount: 0,
    lastSeen: 0,
    chainId: 'chain-a',
    runtimeStatus: 'idle',
  })
})

test('enabled mappings sort before idle ones, then by recency', () => {
  const entries = derivePerAppEntries({
    connections: [
      connection('recent.exe', 'C:\\recent.exe', '2026-11-16T05:00:00Z'),
      connection('older.exe', 'C:\\older.exe', '2026-11-16T01:00:00Z'),
    ],
    mappings: [mapping('pinned.exe', 'C:\\pinned.exe', 'chain-a', true)],
    status: idle,
  })

  expect(entries.map((entry) => entry.processName)).toEqual([
    'pinned.exe',
    'recent.exe',
    'older.exe',
  ])
})

// --- status mapping ---------------------------------------------------------

test('only the block whose chain matches the live session is active', () => {
  const status: ProcessProxyStatus = {
    status: 'active',
    chain_id: 'chain-a',
    relay_port: 1234,
    target_count: 1,
  }
  const entries = derivePerAppEntries({
    connections: [
      connection('a.exe', 'C:\\a.exe', '2026-11-16T01:00:00Z'),
      connection('b.exe', 'C:\\b.exe', '2026-11-16T01:00:00Z'),
    ],
    mappings: [
      mapping('a.exe', 'C:\\a.exe', 'chain-a', true),
      mapping('b.exe', 'C:\\b.exe', 'chain-b', true),
    ],
    status,
  })

  const byName = new Map(entries.map((entry) => [entry.processName, entry]))
  expect(byName.get('a.exe')?.runtimeStatus).toBe('active')
  expect(byName.get('b.exe')?.runtimeStatus).toBe('idle')
})

test('a disabled mapping never reports active even if its chain matches', () => {
  const entries = derivePerAppEntries({
    connections: [connection('a.exe', 'C:\\a.exe', '2026-11-16T01:00:00Z')],
    mappings: [mapping('a.exe', 'C:\\a.exe', 'chain-a', false)],
    status: {
      status: 'active',
      chain_id: 'chain-a',
      relay_port: 1234,
      target_count: 1,
    },
  })

  expect(entries[0].runtimeStatus).toBe('idle')
})

test('recovery_required keeps the detail so the failure stays visible', () => {
  const entries = derivePerAppEntries({
    connections: [connection('a.exe', 'C:\\a.exe', '2026-11-16T01:00:00Z')],
    mappings: [],
    status: {
      status: 'recovery_required',
      detail: 'relay stopped unexpectedly',
    },
  })

  expect(entries[0].runtimeStatus).toBe('recovery_required')
  expect(entries[0].errorDetail).toBe('relay stopped unexpectedly')
})

test('unavailable marks every block unavailable', () => {
  const entries = derivePerAppEntries({
    connections: [connection('a.exe', 'C:\\a.exe', '2026-11-16T01:00:00Z')],
    mappings: [mapping('b.exe', 'C:\\b.exe', 'chain-b', true)],
    status: { status: 'unavailable' },
  })

  expect(entries.map((entry) => entry.runtimeStatus)).toEqual([
    'unavailable',
    'unavailable',
  ])
})

// --- gating -----------------------------------------------------------------

test('non-windows platforms are gated before anything else', () => {
  expect(
    gateEntry({
      entry: entryOf({ runtimeStatus: 'unavailable' }),
      platform: 'macos',
      serviceUsable: false,
      tunActive: true,
      activeSession: true,
      busy: true,
    }),
  ).toBe('nonWindows')
})

test('TUN mode blocks the toggle', () => {
  expect(
    gateEntry({
      entry: entryOf(),
      platform: 'windows',
      serviceUsable: true,
      tunActive: true,
      activeSession: false,
      busy: false,
    }),
  ).toBe('tunActive')
})

test('a missing service blocks the toggle', () => {
  expect(
    gateEntry({
      entry: entryOf(),
      platform: 'windows',
      serviceUsable: false,
      tunActive: false,
      activeSession: false,
      busy: false,
    }),
  ).toBe('noService')
})

test('a live session blocks other blocks but not the active one', () => {
  const base = {
    platform: 'windows',
    serviceUsable: true,
    tunActive: false,
    activeSession: true,
    busy: false,
  }

  expect(
    gateEntry({ ...base, entry: entryOf({ runtimeStatus: 'idle' }) }),
  ).toBe('singleSession')
  expect(
    gateEntry({ ...base, entry: entryOf({ runtimeStatus: 'active' }) }),
  ).toBeNull()
})

test('an in-flight operation blocks every block', () => {
  expect(
    gateEntry({
      entry: entryOf(),
      platform: 'windows',
      serviceUsable: true,
      tunActive: false,
      activeSession: false,
      busy: true,
    }),
  ).toBe('busy')
})

test('a ready block has no gate reason', () => {
  expect(
    gateEntry({
      entry: entryOf(),
      platform: 'windows',
      serviceUsable: true,
      tunActive: false,
      activeSession: false,
      busy: false,
    }),
  ).toBeNull()
})

// --- chain validity ---------------------------------------------------------

test('a deleted chain is reported once the chain list loaded', () => {
  const known = new Set(['chain-a'])
  expect(isChainMissing({ chainId: 'chain-a' }, known, true)).toBe(false)
  expect(isChainMissing({ chainId: 'gone' }, known, true)).toBe(true)
})

test('an unloaded chain list never reports a chain as missing', () => {
  expect(isChainMissing({ chainId: 'gone' }, new Set(), false)).toBe(false)
})

test('an empty or absent chain id is not treated as missing', () => {
  const known = new Set(['chain-a'])
  expect(isChainMissing({ chainId: '' }, known, true)).toBe(false)
  expect(isChainMissing({ chainId: undefined }, known, true)).toBe(false)
})

// --- failure retention ------------------------------------------------------

test('a recorded failure marks only its own block as failed', () => {
  const entries = [
    entryOf({ processName: 'a.exe', processPath: 'C:\\a.exe' }),
    entryOf({ processName: 'b.exe', processPath: 'C:\\b.exe' }),
  ]

  const rows = applyFailures(entries, {
    'a.exe|C:\\a.exe': 'C5-PROXY-006: TUN is active',
  })

  expect(rows[0].runtimeStatus).toBe('failed')
  expect(rows[0].errorDetail).toBe('C5-PROXY-006: TUN is active')
  expect(rows[1].runtimeStatus).toBe('idle')
  expect(rows[1].errorDetail).toBeUndefined()
})

test('a failure survives the next derivation so the block keeps the trace', () => {
  const entries = derivePerAppEntries({
    connections: [connection('a.exe', 'C:\\a.exe', '2026-11-16T01:00:00Z')],
    mappings: [],
    status: idle,
  })

  const once = applyFailures(entries, { 'a.exe|C:\\a.exe': 'boom' })
  const twice = applyFailures(once, { 'a.exe|C:\\a.exe': 'boom' })

  expect(twice[0].runtimeStatus).toBe('failed')
  expect(twice[0].errorDetail).toBe('boom')
})

test('no recorded failures leaves the rows untouched', () => {
  const entries = derivePerAppEntries({
    connections: [connection('a.exe', 'C:\\a.exe', '2026-11-16T01:00:00Z')],
    mappings: [],
    status: idle,
  })

  expect(applyFailures(entries, {})).toBe(entries)
})
