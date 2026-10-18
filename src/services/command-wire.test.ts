import { expect, test } from 'vitest'

import {
  nodeFromWire,
  profilesFromWire,
  proxyViewFromWire,
  withoutNulls,
} from './command-wire'

test('nullable configuration values become UI optionals without changing populated values', () => {
  const wire = {
    disabled: false,
    zero: 0,
    empty: '',
    optional: null,
    nested: [{ name: null }],
  }
  expect(withoutNulls(wire)).toEqual({
    disabled: false,
    zero: 0,
    empty: '',
    optional: undefined,
    nested: [{ name: undefined }],
  })
  expect(wire.optional).toBeNull()
})

test('profile wire values retain identity while normalizing optional fields', () => {
  const item = {
    uid: 'profile',
    type: 'local',
    name: null,
    file: null,
    updated: null,
    selected: [{ name: null, now: null }],
  }
  expect(profilesFromWire({ current: null, items: [item] })).toEqual({
    current: undefined,
    items: [
      {
        ...item,
        name: undefined,
        file: undefined,
        updated: undefined,
        selected: [{ name: undefined, now: undefined }],
      },
    ],
  })
  expect(() =>
    profilesFromWire({ current: null, items: [{ ...item, uid: null }] }),
  ).toThrow('uid')
})

test('node conversion preserves source identity and missing quality', () => {
  const node = {
    stable_id: 'node',
    source: { kind: 'manual' as const },
    display_name: 'Node',
    kind: 'http',
    server: 'localhost',
    port: 80,
    active: true,
    duplicate_count: 1,
    quality: null,
  }
  expect(nodeFromWire(node)).toEqual(node)
  const source = { kind: 'subscription' as const, provider_id: 'provider' }
  expect(nodeFromWire({ ...node, source }).source).toEqual(source)
  expect(
    nodeFromWire({
      ...node,
      quality: {
        status: 'failed',
        short_delay_ms: null,
        handshake_delay_ms: null,
        checked_at_ms: 1,
      },
    }).quality?.failure_streak,
  ).toBe(0)
})

test('proxy view conversion retains meaningful nulls and rejects unsupported schemas', () => {
  const view = {
    schemaVersion: 1,
    orderSource: 'runtime' as const,
    providerState: 'ready' as const,
    global: null,
    direct: null,
    groups: [],
    records: {},
    standalone: [],
    providers: [
      {
        name: 'provider',
        vehicleType: 'HTTP' as const,
        updatedAt: null,
        subscriptionInfo: null,
        proxyRecordIds: [],
      },
    ],
  }
  expect(proxyViewFromWire(view)).toEqual({
    ...view,
    providers: [
      {
        ...view.providers[0],
        updatedAt: undefined,
        subscriptionInfo: undefined,
      },
    ],
  })
  expect(() => proxyViewFromWire({ ...view, schemaVersion: 2 })).toThrow(
    'Unsupported proxy view schema',
  )
})
