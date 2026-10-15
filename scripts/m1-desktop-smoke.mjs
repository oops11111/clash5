import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { mkdir } from 'node:fs/promises'
import { resolve } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'

import {
  cleanupWdioSession,
  createTauriCapabilities,
  startWdioSession,
} from '@wdio/tauri-service'
import { dump, load } from 'js-yaml'

const binary = process.argv[2]
if (!binary)
  throw new Error('Usage: node scripts/m1-desktop-smoke.mjs <app-binary>')
const full = process.argv.includes('--full')
const persisted = process.argv.includes('--persisted')
if (full && persisted) throw new Error('Choose either --full or --persisted')

const capabilities = createTauriCapabilities(resolve(binary), {
  driverProvider: 'external',
  autoInstallTauriDriver: true,
  logLevel: 'debug',
  startTimeout: 120_000,
  commandTimeout: 60_000,
})
let browser
let originalClipboard
let testError
const screenshot = resolve(
  persisted ? 'target/m1-desktop-persisted.png' : 'target/m1-desktop-smoke.png',
)
const fixtures = []
let phase = 'WebDriver startup'
const watchdog = setTimeout(() => {
  process.stderr.write(`Desktop smoke timed out during ${phase}\n`)
  process.exit(124)
}, 8 * 60_000)

function startFixture(port, mode) {
  const child = spawn(
    process.execPath,
    [resolve('scripts/m1-probe-fixture.mjs'), String(port), mode],
    { stdio: ['ignore', 'pipe', 'pipe'] },
  )
  let output = ''
  for (const stream of [child.stdout, child.stderr]) {
    stream.on('data', (chunk) => {
      output += chunk.toString()
    })
  }
  fixtures.push(child)
  return { child, output: () => output }
}

async function fixtureReady(port, fixture) {
  const url = `http://127.0.0.1:${port}/__stats`
  for (let attempt = 0; attempt < 80; attempt += 1) {
    if (fixture.child.exitCode !== null)
      throw new Error(`Fixture exited early: ${fixture.output()}`)
    try {
      if ((await fetch(url, { signal: AbortSignal.timeout(1_000) })).ok) return
    } catch {}
    await sleep(250)
  }
  throw new Error(`Fixture did not become ready: ${fixture.output()}`)
}

async function fixtureStats(port) {
  return (
    await fetch(`http://127.0.0.1:${port}/__stats`, {
      signal: AbortSignal.timeout(2_000),
    })
  ).json()
}

async function startFixtures() {
  await Promise.all(
    [
      [18080, 'forward'],
      [18081, 'limit'],
      [18082, 'limit'],
    ].map(([port, mode]) => fixtureReady(port, startFixture(port, mode))),
  )
}

async function invoke(command, args) {
  const result = await browser.execute(
    async (name, input) => {
      try {
        return { value: await window.__TAURI_INTERNALS__.invoke(name, input) }
      } catch (error) {
        return { error: String(error) }
      }
    },
    command,
    args,
  )
  if (result.error) throw new Error(`${command}: ${result.error}`)
  return result.value
}

async function waitButtonReady(labels) {
  await browser.waitUntil(
    () =>
      browser.execute(
        (values) =>
          [...document.querySelectorAll('.base-page button')].some(
            (button) =>
              values.includes(button.textContent?.trim()) && !button.disabled,
          ),
        labels,
      ),
    {
      timeout: 20_000,
      interval: 250,
      timeoutMsg: `${labels[0]} stayed disabled`,
    },
  )
}

async function clickButton(labels) {
  await waitButtonReady(labels)
  const clicked = await browser.execute((values) => {
    const button = [...document.querySelectorAll('.base-page button')].find(
      (item) => values.includes(item.textContent?.trim()) && !item.disabled,
    )
    button?.click()
    return Boolean(button)
  }, labels)
  if (!clicked) throw new Error(`${labels[0]} button disappeared`)
}

async function importLink(link, count, expectedName) {
  await invoke('plugin:clipboard-manager|write_text', { text: link })
  await clickButton(['Import from clipboard', '从剪贴板导入'])
  await browser.waitUntil(
    async () => {
      const nodes = await invoke('clash5_list_nodes', { includeInactive: true })
      return (
        nodes.length === count &&
        nodes.some((node) => node.display_name === expectedName)
      )
    },
    {
      timeout: 45_000,
      interval: 500,
      timeoutMsg: `Manual import did not yield ${count} nodes`,
    },
  )
}

async function verifyImportAndProbe() {
  try {
    originalClipboard = await invoke('plugin:clipboard-manager|read_text')
  } catch {
    originalClipboard = null
  }
  await startFixtures()

  await importLink('http://127.0.0.1:18080/#M1-forward', 1, 'M1-forward')
  await importLink('http://127.0.0.1:18080/#M1-renamed', 1, 'M1-renamed')
  await importLink('http://127.0.0.1:18081/#M1-limit', 2, 'M1-limit')
  const nodes = await invoke('clash5_list_nodes', { includeInactive: true })
  if (
    nodes.length !== 2 ||
    !nodes.every((node) => node.source.kind === 'manual') ||
    !nodes.some((node) => node.display_name === 'M1-renamed')
  ) {
    throw new Error(
      `Manual import did not preserve source-scoped deduplication: ${JSON.stringify(nodes)}`,
    )
  }

  await clickButton(['Probe nodes', '探测节点'])
  await browser.waitUntil(
    async () => {
      const current = await invoke('clash5_list_nodes', {
        includeInactive: true,
      })
      return current.length === 2 && current.every((node) => node.quality)
    },
    {
      timeout: 60_000,
      interval: 500,
      timeoutMsg: 'Probe results did not reach the node inventory',
    },
  )
  const probed = await invoke('clash5_list_nodes', { includeInactive: true })
  const success = probed.find((node) => node.port === 18080)
  const failure = probed.find((node) => node.port === 18081)
  if (
    !(success?.quality?.short_delay_ms > 0) ||
    success.quality.status === 'failed' ||
    failure?.quality?.status !== 'failed'
  ) {
    throw new Error(
      `Probe did not isolate the limited node: ${JSON.stringify(probed)}`,
    )
  }
  const before = await Promise.all([fixtureStats(18080), fixtureStats(18081)])
  if (before.some((stats) => stats.requests + stats.tunnels === 0)) {
    throw new Error(
      `Probe did not reach both fixture proxies: ${JSON.stringify(before)}`,
    )
  }
  await waitButtonReady(['Probe nodes', '探测节点'])
  const cached = await invoke('clash5_probe_nodes')
  const after = await Promise.all([fixtureStats(18080), fixtureStats(18081)])
  if (
    cached.cached !== 2 ||
    cached.probed !== 0 ||
    before.some(
      (stats, index) =>
        stats.requests !== after[index].requests ||
        stats.tunnels !== after[index].tunnels,
    )
  ) {
    throw new Error(
      `Immediate retry bypassed the probe cache: ${JSON.stringify({ cached, before, after })}`,
    )
  }
  const rows = await browser.execute(
    () => document.querySelectorAll('.base-page button[aria-pressed]').length,
  )
  if (rows !== 2)
    throw new Error(`Node workspace rendered ${rows} rows after import`)
}

async function verifySubscriptionRefresh() {
  const url = 'http://127.0.0.1:18080/subscription.yaml'
  await invoke('plugin:clipboard-manager|write_text', { text: url })
  await clickButton(['Import from clipboard', '从剪贴板导入'])

  let profile
  await browser.waitUntil(
    async () => {
      const profiles = await invoke('get_profiles')
      profile = profiles.items?.find((item) => item.url === url)
      return Boolean(profile?.uid)
    },
    {
      timeout: 45_000,
      interval: 500,
      timeoutMsg: 'Clipboard subscription import did not create a profile',
    },
  )
  const uid = profile.uid
  await browser.waitUntil(
    async () => {
      const nodes = await invoke('clash5_list_nodes', { includeInactive: true })
      return nodes.some(
        (node) =>
          node.source.kind === 'subscription' &&
          node.source.provider_id === uid &&
          node.display_name === 'M1-inline' &&
          node.port === 18080 &&
          node.active,
      )
    },
    {
      timeout: 45_000,
      interval: 500,
      timeoutMsg: 'Imported subscription did not populate the node inventory',
    },
  )

  const original = load(await invoke('read_profile_file', { index: uid }))
  const group = original['proxy-groups']?.find(
    (item) => item.name === 'M1-selected',
  )
  if (!Array.isArray(original.rules) || !Array.isArray(group?.proxies)) {
    throw new Error('Fixture subscription is missing its rule or group')
  }
  original.rules.unshift('DOMAIN,local.example,DIRECT')
  group.proxies.push('DIRECT')
  const saved = await invoke('save_profile_file', {
    index: uid,
    fileData: dump(original, { lineWidth: -1 }),
  })
  if (saved.status !== 'valid') {
    throw new Error(
      `Local subscription edit was rejected: ${JSON.stringify(saved)}`,
    )
  }

  const switched = await invoke('patch_profiles_config', {
    profiles: { current: uid },
  })
  if (switched.status !== 'valid') {
    throw new Error(
      `Fixture profile switch failed: ${JSON.stringify(switched)}`,
    )
  }
  const phase = await fetch('http://127.0.0.1:18080/__phase/2', {
    method: 'POST',
    signal: AbortSignal.timeout(2_000),
  })
  if (phase.status !== 204) throw new Error('Fixture phase switch failed')
  await invoke('update_profile', { index: uid, option: profile.option })

  const refreshed = load(await invoke('read_profile_file', { index: uid }))
  const refreshedGroup = refreshed['proxy-groups']?.find(
    (item) => item.name === 'M1-selected',
  )
  if (
    !refreshed.rules?.includes('DOMAIN,local.example,DIRECT') ||
    !refreshedGroup?.proxies?.includes('DIRECT') ||
    refreshed.proxies?.find((item) => item.name === 'M1-inline')?.port !== 18081
  ) {
    throw new Error('Subscription refresh discarded a local rule or group edit')
  }

  try {
    await browser.waitUntil(
      async () => {
        const nodes = await invoke('clash5_list_nodes', {
          includeInactive: true,
        })
        return nodes.some(
          (node) =>
            node.source.provider_id === `proxy-provider:${uid}:M1-remote` &&
            node.display_name === 'M1-provider',
        )
      },
      {
        timeout: 60_000,
        interval: 2_000,
        timeoutMsg: 'External proxy-provider cache did not populate',
      },
    )
  } catch (error) {
    const stats = await fixtureStats(18080).catch(() => null)
    const runtime = await invoke('get_runtime_config').catch(() => null)
    const view = await invoke('get_proxy_view').catch(() => null)
    const coreLogs = await invoke('get_clash_logs').catch(() => [])
    const diagnostics = {
      fixture: stats,
      runtimeProvider: runtime?.['proxy-providers']?.['M1-remote']
        ? {
            type: runtime['proxy-providers']['M1-remote'].type,
            path: runtime['proxy-providers']['M1-remote'].path,
          }
        : null,
      runtimeNames: Object.values(view?.records ?? {}).map((node) => ({
        name: node.name,
        source: node.source,
      })),
      coreProviderLogs: coreLogs
        .filter((line) => /provider/i.test(line))
        .slice(-10)
        .map((line) => line.replace(/https?:\/\/[^\s]+/gi, '[redacted-uri]')),
    }
    throw new Error(
      `${String(error)}; diagnostics: ${JSON.stringify(diagnostics)}`,
    )
  }
  const nodes = await invoke('clash5_list_nodes', { includeInactive: true })
  const inline = nodes.filter(
    (node) =>
      node.source.provider_id === uid && node.display_name === 'M1-inline',
  )
  const provider = nodes.find(
    (node) =>
      node.source.provider_id === `proxy-provider:${uid}:M1-remote` &&
      node.display_name === 'M1-provider',
  )
  const manual = nodes.filter((node) => node.source.kind === 'manual')
  if (
    manual.length !== 2 ||
    !manual.every((node) => node.active) ||
    inline.length !== 2 ||
    !inline.some((node) => node.port === 18080 && !node.active) ||
    !inline.some((node) => node.port === 18081 && node.active) ||
    !provider?.active ||
    provider.port !== 18082
  ) {
    throw new Error(
      `Subscription refresh lost source isolation or orphan state: ${JSON.stringify(nodes)}`,
    )
  }
  const beforeProviderProbe = await fixtureStats(18082)
  const providerBatch = await invoke('clash5_probe_nodes')
  const afterProviderProbe = await fixtureStats(18082)
  const probedProvider = (
    await invoke('clash5_list_nodes', {
      includeInactive: true,
    })
  ).find(
    (node) =>
      node.source.provider_id === `proxy-provider:${uid}:M1-remote` &&
      node.display_name === 'M1-provider',
  )
  if (
    providerBatch.probed < 1 ||
    probedProvider?.quality?.status !== 'failed' ||
    afterProviderProbe.requests + afterProviderProbe.tunnels <=
      beforeProviderProbe.requests + beforeProviderProbe.tunnels
  ) {
    throw new Error(
      `Provider node was not probed through its runtime proxy: ${JSON.stringify({ providerBatch, probedProvider, beforeProviderProbe, afterProviderProbe })}`,
    )
  }
  await clickButton(['Refresh list', '刷新列表'])
  await browser.waitUntil(
    () =>
      browser.execute(
        () =>
          document.querySelectorAll('.base-page button[aria-pressed]')
            .length === 4,
      ),
    {
      timeout: 20_000,
      interval: 500,
      timeoutMsg: 'Node workspace did not show four active source-scoped rows',
    },
  )
}

async function verifyWorkspaceControls() {
  phase = 'source filters'
  const nodes = await invoke('clash5_list_nodes', { includeInactive: true })
  const active = nodes.filter((node) => node.active)
  const sourceCounts = new Map()
  for (const node of active) {
    sourceCounts.set(
      node.stable_id,
      (sourceCounts.get(node.stable_id) ?? 0) + 1,
    )
  }

  async function rows() {
    return browser.execute(() =>
      [...document.querySelectorAll('.base-page button[aria-pressed]')].map(
        (button) =>
          button.querySelector('.MuiTypography-root')?.textContent?.trim(),
      ),
    )
  }

  async function selectFilter(labels, expected) {
    const clicked = await browser.execute((values) => {
      const chip = [
        ...document.querySelectorAll('.base-page .MuiChip-clickable'),
      ].find((item) => values.includes(item.textContent?.trim()))
      chip?.click()
      return Boolean(chip)
    }, labels)
    if (!clicked) throw new Error(`${labels[0]} filter is missing`)
    const names = expected.map((node) => node.display_name).sort()
    await browser.waitUntil(
      async () =>
        JSON.stringify((await rows()).sort()) === JSON.stringify(names),
      {
        timeout: 10_000,
        interval: 250,
        timeoutMsg: `${labels[0]} filter did not show ${JSON.stringify(names)}`,
      },
    )
  }

  await selectFilter(
    ['Manual', '手动'],
    active.filter((node) => node.source.kind === 'manual'),
  )
  await selectFilter(
    ['Subscription', '订阅'],
    active.filter((node) => node.source.kind === 'subscription'),
  )
  await selectFilter(
    ['Duplicates', '重复'],
    active.filter(
      (node) =>
        node.duplicate_count > 1 || (sourceCounts.get(node.stable_id) ?? 0) > 1,
    ),
  )
  await selectFilter(
    ['Healthy', '高质量'],
    active.filter((node) => node.quality?.status === 'healthy'),
  )
  await selectFilter(
    ['Removed', '已移除'],
    nodes.filter((node) => !node.active),
  )
  await selectFilter(['All', '全部'], active)

  phase = 'search controls'
  const search = await browser.$('.base-page input[placeholder]')
  await search.setValue('M1-provider')
  await browser.waitUntil(
    async () =>
      JSON.stringify(await rows()) === JSON.stringify(['M1-provider']),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Node search did not isolate the provider row',
    },
  )
  await search.click()
  await browser.keys(['Control', 'a'])
  await browser.keys('Backspace')
  await browser.waitUntil(async () => (await rows()).length === active.length, {
    timeout: 10_000,
    interval: 250,
    timeoutMsg: 'Clearing node search did not restore active rows',
  })

  phase = 'sort menu opening'
  const sortFound = await browser.execute(() => {
    const sort = document.querySelector('.base-page [role="combobox"]')
    if (!sort) return false
    sort.dispatchEvent(
      new MouseEvent('mousedown', {
        bubbles: true,
        cancelable: true,
        button: 0,
      }),
    )
    return true
  })
  if (!sortFound) throw new Error('Node sort control is missing')
  phase = 'sort option lookup'
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        [...document.querySelectorAll('li[role="option"]')].some((item) =>
          ['Sort by probe result', '按探测结果排序'].includes(
            item.textContent?.trim(),
          ),
        ),
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Quality sort option is missing',
    },
  )
  await browser.execute(() => {
    const option = [...document.querySelectorAll('li[role="option"]')].find(
      (item) =>
        ['Sort by probe result', '按探测结果排序'].includes(
          item.textContent?.trim(),
        ),
    )
    option?.click()
  })
  phase = 'sort ranking'
  const rank = (node) => {
    if (!node.quality) return 2_000_000
    if (node.quality.status === 'failed') return 1_000_000
    return (
      (node.quality.status === 'slow' ? 100_000 : 0) +
      (node.quality.handshake_delay_ms ?? 0)
    )
  }
  const rankedNames = [...active]
    .sort((a, b) => rank(a) - rank(b) || a.stable_id.localeCompare(b.stable_id))
    .map((node) => node.display_name)
  await browser.waitUntil(
    async () => JSON.stringify(await rows()) === JSON.stringify(rankedNames),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: `Quality sort did not show ${JSON.stringify(rankedNames)}`,
    },
  )
}

async function capture() {
  await mkdir(resolve('target'), { recursive: true })
  await browser.saveScreenshot(screenshot)
}

async function verifyNavigation() {
  const sidebar = await browser.execute(() => {
    const menu = document.querySelector('.the-menu')
    const items = [...menu.querySelectorAll('.MuiListItemButton-root')]
    const last = items.at(-1)?.getBoundingClientRect()
    const viewport = menu.getBoundingClientRect()
    return {
      labels: items.map((item) => item.textContent?.trim()),
      lastVisible: Boolean(
        last && last.top >= viewport.top && last.bottom <= viewport.bottom + 1,
      ),
      logoVisible: Boolean(
        document.querySelector('.the-logo .MuiSvgIcon-root'),
      ),
    }
  })
  const expected = [
    [
      'Home',
      'Nodes',
      'Subscriptions',
      'Chains',
      'Rules',
      'Plugins',
      'Connections',
      'Logs',
      'Settings',
    ],
    ['首页', '节点', '订阅', '链路', '规则', '插件', '连接', '日志', '设置'],
    ['首頁', '節點', '訂閱', '鏈路', '規則', '插件', '連線', '日誌', '設定'],
  ]
  if (
    !sidebar.logoVisible ||
    !sidebar.lastVisible ||
    !expected.some(
      (labels) => JSON.stringify(labels) === JSON.stringify(sidebar.labels),
    )
  ) {
    throw new Error(`Unexpected clash5 sidebar: ${JSON.stringify(sidebar)}`)
  }

  await browser.execute(() => {
    const plugins = [
      ...document.querySelectorAll('.the-menu .MuiListItemButton-root'),
    ].find((item) => ['Plugins', '插件'].includes(item.textContent?.trim()))
    plugins?.click()
  })
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        [...document.querySelectorAll('.base-page [role="tab"]')].some((tab) =>
          ['Binding', '绑定', '綁定'].includes(tab.textContent?.trim()),
        ),
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Plugin binding tab did not render',
    },
  )
  await browser.execute(() => {
    const binding = [
      ...document.querySelectorAll('.base-page [role="tab"]'),
    ].find((tab) =>
      ['Binding', '绑定', '綁定'].includes(tab.textContent?.trim()),
    )
    binding?.click()
  })
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        [...document.querySelectorAll('.base-page [role="tab"]')].some(
          (tab) =>
            tab.getAttribute('aria-selected') === 'true' &&
            ['Binding', '绑定', '綁定'].includes(tab.textContent?.trim()),
        ),
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Plugin binding tab did not activate',
    },
  )
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        [...document.querySelectorAll('.base-page label')].some((label) =>
          ['Keep orphan bindings (days)', '孤儿绑定保留天数'].includes(
            label.textContent?.trim(),
          ),
        ),
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Orphan retention controls did not render',
    },
  )
  const retentionSummaryVisible = await browser.execute(() => {
    const text = document.querySelector('.base-page')?.textContent ?? ''
    return /orphan bindings|孤儿绑定/.test(text)
  })
  if (!retentionSummaryVisible) {
    throw new Error('Orphan retention summary did not render')
  }
  await browser.saveScreenshot(resolve('target/nav-plugins-binding.png'))
}

try {
  if (persisted) {
    await startFixtures()
  }
  browser = await startWdioSession(capabilities)
  phase = 'Tauri bridge'
  let inventory
  let lastError
  try {
    await browser.waitUntil(
      async () => {
        try {
          const result = await browser.execute(async () => {
            const invoke = window.__TAURI_INTERNALS__?.invoke
            if (!invoke) {
              return { error: 'Tauri bridge is unavailable' }
            }
            try {
              const value = await invoke('clash5_list_nodes', {
                includeInactive: true,
              })
              return { value }
            } catch (error) {
              return { error: String(error) }
            }
          })
          inventory = result.value
          lastError = result.error
          return Array.isArray(inventory)
        } catch (error) {
          lastError = String(error)
          return false
        }
      },
      {
        timeout: 60_000,
        interval: 1_000,
        timeoutMsg: 'clash5_list_nodes did not become available',
      },
    )
  } catch (error) {
    throw new Error(`${String(error)}; last error: ${lastError ?? 'none'}`)
  }
  if (!persisted && inventory.length !== 0) {
    throw new Error(`Fresh desktop session contains ${inventory.length} nodes`)
  }
  const persistedManual = inventory.filter(
    (node) => node.source.kind === 'manual',
  )
  const persistedQuality = persistedManual.find(
    (node) => node.display_name === 'M1-renamed',
  )?.quality
  if (
    persisted &&
    (persistedManual.length !== 2 ||
      !persistedQuality ||
      !persistedManual.every((node) => node.active) ||
      !persistedManual.some(
        (node) => node.display_name === 'M1-renamed' && node.port === 18080,
      ) ||
      !persistedManual.some(
        (node) => node.display_name === 'M1-limit' && node.port === 18081,
      ))
  ) {
    throw new Error(
      `Manual nodes did not survive restart: ${JSON.stringify(persistedManual)}`,
    )
  }

  await browser.execute(() => {
    window.history.pushState({}, '', '/nodes')
    window.dispatchEvent(new PopStateEvent('popstate'))
  })
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        Boolean(document.querySelector('.base-page input[placeholder]')),
      ),
    {
      timeout: 20_000,
      interval: 500,
      timeoutMsg: 'Node workspace did not render',
    },
  )

  const page = await browser.execute(() => ({
    title: document.querySelector('.base-page header')?.textContent?.trim(),
    rows: document.querySelectorAll('.base-page button[aria-pressed]').length,
  }))
  if (
    !page.title ||
    page.title.includes('nodePool.') ||
    (!persisted && page.rows !== 0)
  ) {
    throw new Error(`Unexpected node workspace: ${JSON.stringify(page)}`)
  }

  if (full) {
    phase = 'manual import and probe'
    await verifyImportAndProbe()
    phase = 'subscription refresh and provider probe'
    await verifySubscriptionRefresh()
    await verifyWorkspaceControls()
  } else if (persisted) {
    phase = 'manual node persistence'
    originalClipboard = await invoke(
      'plugin:clipboard-manager|read_text',
    ).catch(() => null)
    await importLink(
      'http://127.0.0.1:18080/#M1-renamed',
      inventory.length,
      'M1-renamed',
    )
    await browser.waitUntil(
      () =>
        browser.execute(() =>
          [...document.querySelectorAll('.base-page .MuiAlert-root')].some(
            (alert) =>
              /Imported 1 standalone proxy link|已导入 1 个独立节点链接/.test(
                alert.textContent ?? '',
              ),
          ),
        ),
      {
        timeout: 30_000,
        interval: 250,
        timeoutMsg: 'Persisted manual link was not reimported through the page',
      },
    )
    const reimported = (
      await invoke('clash5_list_nodes', { includeInactive: true })
    ).filter((node) => node.source.kind === 'manual')
    if (
      reimported.length !== 2 ||
      reimported.find((node) => node.display_name === 'M1-renamed')?.quality
        ?.checked_at_ms !== persistedQuality.checked_at_ms ||
      persistedManual.some(
        (before) =>
          !reimported.some(
            (after) =>
              after.stable_id === before.stable_id &&
              after.display_name === before.display_name,
          ),
      )
    ) {
      throw new Error(
        `Manual identity changed after restart: ${JSON.stringify(reimported)}`,
      )
    }
  }
  phase = 'screenshot capture'
  await capture()
  if (full) {
    phase = 'sidebar and plugin binding navigation'
    await verifyNavigation()
  }
  process.stdout.write(
    persisted
      ? 'Manual nodes and their stable identities survived a desktop restart\n'
      : full
        ? 'Native Tauri import, probe cache, subscription refresh, and source isolation passed\n'
        : 'Native Tauri bridge and empty node workspace passed\n',
  )
} catch (error) {
  testError = error
  if (browser) {
    await capture().catch((captureError) => {
      process.stderr.write(
        `Desktop screenshot failed: ${String(captureError)}\n`,
      )
    })
  }
  throw error
} finally {
  phase = 'WebDriver cleanup'
  if (browser && typeof originalClipboard === 'string') {
    await invoke('plugin:clipboard-manager|write_text', {
      text: originalClipboard,
    }).catch(() => {})
  }
  let forceExit = false
  if (browser) {
    if (process.platform === 'win32' && process.env.CI) {
      // The upstream Windows driver stop blocks the Node event loop after DELETE.
      await browser.deleteSession()
      const driverKill = spawn(
        'taskkill',
        ['/F', '/T', '/IM', 'tauri-driver.exe'],
        { stdio: 'ignore', windowsHide: true },
      )
      await once(driverKill, 'exit', {
        signal: AbortSignal.timeout(5_000),
      }).catch((error) => {
        process.stderr.write(`Driver process cleanup failed: ${error}\n`)
      })
      forceExit = true
    } else {
      await cleanupWdioSession(browser)
    }
  }
  phase = 'fixture cleanup'
  for (const child of fixtures.reverse()) {
    if (child.exitCode === null && child.signalCode === null) {
      const exited = once(child, 'exit', {
        signal: AbortSignal.timeout(5_000),
      })
      child.kill()
      await exited
    }
    child.stdout?.destroy()
    child.stderr?.destroy()
  }
  clearTimeout(watchdog)
  if (forceExit) {
    if (testError) process.stderr.write(`${testError.stack ?? testError}\n`)
    process.exit(testError ? 1 : 0)
  }
}
