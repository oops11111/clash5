import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import vm from 'node:vm'
import { buildRules, normalizeRules, profileForHost } from './policy.mjs'

test('specific site override excludes parent script and header rules', () => {
  const rules = normalizeRules([
    { host: 'example.com', profile: 'new_york' },
    { host: 'private.example.com', profile: 'unchanged' },
  ])
  assert.equal(profileForHost('www.example.com', rules), 'new_york')
  assert.equal(profileForHost('private.example.com', rules), 'unchanged')
  const { scripts, headers } = buildRules(rules)
  assert.equal(scripts.length, 1)
  assert.ok(scripts[0].excludeMatches.includes('*://private.example.com/*'))
  assert.ok(headers[0].condition.excludedRequestDomains.includes('private.example.com'))
})

test('host validation rejects broad or malformed rules', () => {
  for (const host of ['', '*', 'https://example.com', 'example.com/path', 'bad..example', 'example.com:8080']) {
    assert.throws(() => normalizeRules([{ host, profile: 'tokyo' }]))
  }
  assert.throws(() => normalizeRules([{ host: 'example.com', profile: 'unknown' }]))
})

test('page profile keeps locale, time zone, and Date local clock aligned across DST', () => {
  const context = vm.createContext({})
  vm.runInContext('globalThis.window = globalThis; globalThis.Navigator = class {}; globalThis.navigator = new Navigator()', context)
  vm.runInContext(readFileSync(new URL('./page-shim.js', import.meta.url), 'utf8'), context)
  vm.runInContext(readFileSync(new URL('./profile-new_york.js', import.meta.url), 'utf8'), context)
  assert.equal(vm.runInContext('navigator.language', context), 'en-US')
  assert.equal(vm.runInContext('Intl.DateTimeFormat().resolvedOptions().timeZone', context), 'America/New_York')
  assert.equal(vm.runInContext('new Date("2026-07-01T12:00:00Z").getHours()', context), 8)
  assert.equal(vm.runInContext('new Date("2026-07-01T12:00:00Z").getTimezoneOffset()', context), 240)
  assert.equal(vm.runInContext('new Date("2026-01-01T12:00:00Z").getHours()', context), 7)
  assert.equal(vm.runInContext('new Date("2026-01-01T12:00:00Z").getTimezoneOffset()', context), 300)
})
