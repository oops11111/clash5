import { spawn } from 'node:child_process'
import { randomBytes, randomUUID } from 'node:crypto'
import { once } from 'node:events'
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import http from 'node:http'
import net from 'node:net'
import { join, resolve } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'

import {
  cleanupWdioSession,
  createTauriCapabilities,
  startWdioSession,
} from '@wdio/tauri-service'
import AdmZip from 'adm-zip'
import { dump, load } from 'js-yaml'

import { startChainFixture } from './m2-mihomo-smoke.mjs'

const binary = process.argv[2]
if (!binary)
  throw new Error('Usage: node scripts/m1-desktop-smoke.mjs <app-binary>')
const full = process.argv.includes('--full')
const persisted = process.argv.includes('--persisted')
const recoveryCancel = process.argv.includes('--recovery-cancel')
if ([full, persisted, recoveryCancel].filter(Boolean).length > 1) {
  throw new Error('Choose only one desktop smoke mode')
}

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
let m3AppDirectory
let m3StagingPath
let m3FixtureInstalled = false
let m3BindingIds = []
let m4Fixture
const screenshot = resolve(
  persisted
    ? 'target/m1-desktop-persisted.png'
    : recoveryCancel
      ? 'target/m1-recovery-cancel.png'
      : 'target/m1-desktop-smoke.png',
)
const fixtures = []
let chainFixture
let phase = 'WebDriver startup'
const watchdog = setTimeout(
  () => {
    process.stderr.write(`Desktop smoke timed out during ${phase}\n`)
    process.exit(124)
  },
  (full ? 15 : 8) * 60_000,
)

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

async function freeLoopbackPort() {
  const server = net.createServer()
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const port = server.address().port
  await new Promise((done) => server.close(done))
  return port
}

async function startM4SingboxFixture() {
  const ssPort = await freeLoopbackPort()
  const password = randomBytes(24).toString('base64')
  const child = spawn(
    process.platform === 'win32' ? 'sing-box.exe' : 'sing-box',
    ['run', '-c', 'stdin'],
    { stdio: ['pipe', 'ignore', 'ignore'], windowsHide: true },
  )
  let startupError
  child.once('error', (error) => {
    startupError = error
  })
  child.stdin.on('error', (error) => {
    startupError ??= error
  })
  fixtures.push(child)
  child.stdin.end(
    JSON.stringify({
      log: { level: 'error' },
      inbounds: [
        {
          type: 'shadowsocks',
          tag: 'm4-test-in',
          listen: '127.0.0.1',
          listen_port: ssPort,
          method: 'aes-128-gcm',
          password,
        },
      ],
      outbounds: [{ type: 'direct', tag: 'direct' }],
      route: { final: 'direct' },
    }),
  )

  let ssReady = false
  for (let attempt = 0; attempt < 80; attempt += 1) {
    if (startupError)
      throw new Error(
        `M4 sing-box fixture could not start: ${startupError.message}`,
      )
    if (child.exitCode !== null)
      throw new Error(
        `M4 sing-box fixture exited early with code ${child.exitCode}`,
      )
    try {
      await new Promise((done, reject) => {
        const socket = net.connect(ssPort, '127.0.0.1')
        socket.once('connect', () => {
          socket.destroy()
          done()
        })
        socket.once('error', reject)
      })
      ssReady = true
      break
    } catch {
      await sleep(250)
    }
  }
  if (!ssReady) throw new Error('M4 sing-box fixture did not start')

  const frontStats = []
  const sockets = new Set()
  const front = http.createServer((_, response) =>
    response.writeHead(502).end(),
  )
  front.on('connection', (socket) => {
    sockets.add(socket)
    socket.once('close', () => sockets.delete(socket))
  })
  front.on('connect', (request, client, head) => {
    frontStats.push(request.url)
    if (request.url !== `127.0.0.1:${ssPort}`) {
      client.end('HTTP/1.1 403 Forbidden\r\n\r\n')
      return
    }
    const upstream = net.connect(ssPort, '127.0.0.1')
    sockets.add(upstream)
    upstream.once('close', () => sockets.delete(upstream))
    upstream.once('connect', () => {
      client.write('HTTP/1.1 200 Connection Established\r\n\r\n')
      if (head.length) upstream.write(head)
      client.pipe(upstream).pipe(client)
    })
    upstream.once('error', () => client.destroy())
    client.once('error', () => upstream.destroy())
    client.once('close', () => upstream.destroy())
  })
  front.listen(0, '127.0.0.1')
  await once(front, 'listening')
  return {
    frontPort: front.address().port,
    frontStats,
    landingPort: ssPort,
    password,
    close: async () => {
      for (const socket of sockets) socket.destroy()
      await new Promise((done) => front.close(done))
    },
  }
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
        return {
          error:
            typeof error === 'object' ? JSON.stringify(error) : String(error),
        }
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

async function captureClipboardImportFlow() {
  await browser.execute(() => {
    const internals = window.__TAURI_INTERNALS__
    const flow = {
      installed: false,
      installError: null,
      invokeDescriptor: null,
      clipboardReads: [],
      importCalls: [],
      importErrors: [],
    }
    const publish = () => {
      document.documentElement.dataset.clash5SmokeImportFlow =
        JSON.stringify(flow)
    }
    publish()
    if (!internals) {
      flow.installError = 'Tauri internals are unavailable'
      publish()
      return
    }

    const invoke = internals.invoke
    const descriptor = Object.getOwnPropertyDescriptor(internals, 'invoke')
    flow.invokeDescriptor = descriptor
      ? {
          configurable: descriptor.configurable,
          writable: descriptor.writable ?? null,
          hasGetter: typeof descriptor.get === 'function',
          hasSetter: typeof descriptor.set === 'function',
        }
      : null
    const wrappedInvoke = async function (...args) {
      try {
        const result = await invoke.apply(this, args)
        if (args[0] === 'plugin:clipboard-manager|read_text') {
          flow.clipboardReads.push(
            typeof result === 'string' ? result.slice(0, 240) : result,
          )
        }
        if (args[0] === 'clash5_import_manual_proxies') {
          flow.importCalls.push({
            proxyCount: args[1]?.proxies?.length ?? null,
            imported: result?.imported ?? null,
          })
        }
        return result
      } catch (error) {
        if (args[0] === 'clash5_import_manual_proxies') {
          let detail = error?.detail
          if (typeof error === 'string') {
            try {
              detail = JSON.parse(error)?.detail ?? error
            } catch {
              detail = error
            }
          }
          flow.importErrors.push({
            code: error?.code,
            detail: typeof detail === 'string' ? detail.slice(0, 240) : null,
          })
        }
        throw error
      } finally {
        publish()
      }
    }

    try {
      try {
        Object.defineProperty(internals, 'invoke', {
          configurable: true,
          writable: true,
          value: wrappedInvoke,
        })
      } catch {
        internals.invoke = wrappedInvoke
      }
      flow.installed = internals.invoke === wrappedInvoke
      if (!flow.installed)
        flow.installError = 'Tauri invoke hook was not installed'
    } catch (error) {
      flow.installError = String(error).slice(0, 240)
    }
    publish()
  })
}

async function importLink(link, count, expectedName) {
  await invoke('plugin:clipboard-manager|write_text', { text: link })
  const clipboard = await invoke('plugin:clipboard-manager|read_text')
  if (clipboard !== link) {
    throw new Error(
      `Clipboard fixture did not round-trip: ${JSON.stringify(clipboard)}`,
    )
  }
  await captureClipboardImportFlow()
  const clipboardAfterCapture = await invoke(
    'plugin:clipboard-manager|read_text',
  )
  if (clipboardAfterCapture !== link) {
    throw new Error('Clipboard fixture changed before the UI import click')
  }
  await clickButton(['Import from clipboard', '从剪贴板导入'])
  try {
    await browser.waitUntil(
      async () => {
        const nodes = await invoke('clash5_list_nodes', {
          includeInactive: true,
        })
        const importFinished = await browser.execute(() =>
          [...document.querySelectorAll('.MuiAlert-root')].some((alert) =>
            /Imported 1 standalone proxy link|已导入 1 个独立节点链接/.test(
              alert.textContent ?? '',
            ),
          ),
        )
        return (
          importFinished &&
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
  } catch (error) {
    const [nodes, alerts, importFlow, importFailure, currentClipboard] =
      await Promise.all([
        invoke('clash5_list_nodes', { includeInactive: true }),
        browser.execute(() =>
          [...document.querySelectorAll('.MuiAlert-root')].map((alert) =>
            alert.textContent?.trim(),
          ),
        ),
        browser.execute(() => {
          const flow = document.documentElement.dataset.clash5SmokeImportFlow
          return flow ? JSON.parse(flow) : null
        }),
        browser.execute(() => {
          const failure =
            document.documentElement.dataset.clash5ClipboardImportFailure
          return failure ? JSON.parse(failure) : null
        }),
        invoke('plugin:clipboard-manager|read_text').catch(
          (readError) => `read failed: ${String(readError)}`,
        ),
      ])
    const details = {
      error: String(error),
      nodes,
      alerts,
      clipboard: currentClipboard,
      importFlow,
      importFailure,
    }
    throw new Error(
      `Manual import acceptance failed: ${JSON.stringify(details)}`,
      { cause: error },
    )
  }
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

  const originalNode = (
    await invoke('clash5_list_nodes', { includeInactive: true })
  ).find(
    (node) =>
      node.source.kind === 'subscription' &&
      node.source.provider_id === uid &&
      node.display_name === 'M1-inline' &&
      node.active,
  )
  if (!originalNode) throw new Error('Initial subscription node is missing')
  await invoke('clash5_bind_node', {
    binding: {
      id: 'm1-ambiguous-rebind',
      plugin_id: 'desktop-smoke',
      target_id: originalNode.stable_id,
      source: { kind: 'subscription', provider_id: uid },
      priority: 10,
      enabled: true,
      config: {},
    },
  })

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
      { cause: error },
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
            .length === 6,
      ),
    {
      timeout: 20_000,
      interval: 500,
      timeoutMsg: 'Node workspace did not show six active source-scoped rows',
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

async function prepareM3PluginFixture() {
  if (
    (await invoke('clash5_list_plugins')).some(
      (plugin) => plugin.id === 'm3.smoke.plugin',
    )
  ) {
    throw new Error(
      'Refusing to replace a pre-existing m3.smoke.plugin installation',
    )
  }
  const fixturePath = resolve('scripts/fixtures/m3-acceptance-plugin.c5p')
  const packageArchive = new AdmZip(await readFile(fixturePath))
  const signer = packageArchive.readAsText('signer.pub').trim()
  m3AppDirectory = await invoke('get_app_dir')
  const stagingId = randomBytes(16).toString('base64url').slice(0, 21)
  const stagingDirectory = join(m3AppDirectory, 'plugins', '.staging')
  await mkdir(stagingDirectory, { recursive: true })
  m3StagingPath = join(stagingDirectory, `${stagingId}.c5p`)
  await writeFile(m3StagingPath, await readFile(fixturePath))
  const installed = await invoke('clash5_install_staged_plugin', {
    stagingId,
    confirmedSigner: signer,
  })
  if (installed.id !== 'm3.smoke.plugin')
    throw new Error(
      `Unexpected M3 fixture plugin: ${JSON.stringify(installed)}`,
    )
  m3FixtureInstalled = true

  const node = (
    await invoke('clash5_list_nodes', { includeInactive: true })
  ).find((item) => item.active && item.source.kind === 'manual')
  if (!node)
    throw new Error('M3 desktop test requires an active stored manual node')

  const suffix = randomBytes(8).toString('hex')
  const target = {
    kind: 'node',
    source: node.source,
    stable_id: node.stable_id,
  }
  m3BindingIds = [`m3-smoke-${suffix}-high`, `m3-smoke-${suffix}-low`]
  await invoke('clash5_save_plugin_binding', {
    binding: {
      id: m3BindingIds[0],
      plugin_id: installed.id,
      target,
      priority: 80,
      enabled: true,
      config: { acceptance: true },
    },
  })
  await invoke('clash5_save_plugin_binding', {
    binding: {
      id: m3BindingIds[1],
      plugin_id: installed.id,
      target,
      priority: 20,
      enabled: true,
      config: { acceptance: true },
    },
  })

  const effective = await invoke('clash5_get_plugin_effective_config')
  const source = effective.provenance['/tun/mtu']
  if (
    effective.config?.tun?.mtu !== 1360 ||
    source?.kind !== 'plugin' ||
    source.plugin_id !== installed.id ||
    source.binding_id !== m3BindingIds[0] ||
    !effective.conflicts.some(
      (conflict) =>
        conflict.path === '/tun/mtu' &&
        conflict.winner_binding_id === m3BindingIds[0] &&
        conflict.loser_binding_id === m3BindingIds[1],
    )
  ) {
    throw new Error(
      `M3 plugin merge or provenance failed: ${JSON.stringify(effective)}`,
    )
  }
  const audit = await invoke('clash5_list_plugin_audit_events', { limit: 50 })
  if (
    !audit.some((event) => event.event === 'plugin.install') ||
    !audit.some(
      (event) =>
        event.event === 'plugin.apply' &&
        event.details.executed?.some(
          (entry) =>
            entry.plugin_id === installed.id && entry.permissions?.length > 0,
        ),
    )
  ) {
    throw new Error(
      `M3 plugin audit events are incomplete: ${JSON.stringify(audit)}`,
    )
  }
}

async function clickBindingAction(bindingId, labels) {
  const priority = bindingId.endsWith('-high') ? '80' : '20'
  const priorityLabels = [
    `Priority ${priority}`,
    `优先级 ${priority}`,
    `優先級 ${priority}`,
  ]
  const findEnabledBindingAction = ({
    buttonLabels,
    bindingPriorityLabels,
  }) => {
    const cards = [...document.querySelectorAll('.base-page .MuiPaper-root')]
      .filter(
        (card) =>
          card.textContent?.includes('m3.smoke.plugin') &&
          [...card.querySelectorAll('.MuiChip-root')].some((chip) =>
            bindingPriorityLabels.includes(chip.textContent?.trim()),
          ) &&
          [...card.querySelectorAll('button')].some((button) =>
            buttonLabels.includes(button.textContent?.trim()),
          ),
      )
      .sort((left, right) => left.textContent.length - right.textContent.length)
    const button = [...(cards[0]?.querySelectorAll('button') ?? [])].find(
      (item) =>
        buttonLabels.includes(item.textContent?.trim()) && !item.disabled,
    )
    return Boolean(button)
  }
  const args = { bindingPriorityLabels: priorityLabels, buttonLabels: labels }
  await browser.waitUntil(
    () => browser.execute(findEnabledBindingAction, args).then(Boolean),
    {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: `${labels[0]} did not become enabled for ${bindingId}`,
    },
  )
  const clicked = await browser.execute(
    ({ buttonLabels, bindingPriorityLabels }) => {
      const cards = [...document.querySelectorAll('.base-page .MuiPaper-root')]
        .filter(
          (card) =>
            card.textContent?.includes('m3.smoke.plugin') &&
            [...card.querySelectorAll('.MuiChip-root')].some((chip) =>
              bindingPriorityLabels.includes(chip.textContent?.trim()),
            ),
        )
        .sort(
          (left, right) => left.textContent.length - right.textContent.length,
        )
      const button = [...(cards[0]?.querySelectorAll('button') ?? [])].find(
        (item) =>
          buttonLabels.includes(item.textContent?.trim()) && !item.disabled,
      )
      button?.click()
      return Boolean(button)
    },
    args,
  )
  if (!clicked) throw new Error(`Could not find ${labels[0]} for ${bindingId}`)
}

async function verifyM3PluginUi() {
  await browser.waitUntil(
    () =>
      browser.execute(() => {
        const text = document.querySelector('.base-page')?.textContent ?? ''
        const priorities = [
          ...document.querySelectorAll('.base-page .MuiChip-root'),
        ].map((chip) => chip.textContent?.trim())
        return (
          text.includes('m3.smoke.plugin') &&
          text.includes('/tun/mtu') &&
          text.includes('1360') &&
          ['Priority 80', '优先级 80', '優先級 80'].some((label) =>
            priorities.includes(label),
          ) &&
          ['Priority 20', '优先级 20', '優先級 20'].some((label) =>
            priorities.includes(label),
          )
        )
      }),
    {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: 'Stored M3 plugin bindings did not render',
    },
  )
  const pageText = await browser.execute(
    () => document.querySelector('.base-page')?.textContent ?? '',
  )
  if (
    !pageText.includes('m3.smoke.plugin') ||
    !pageText.includes('/tun/mtu') ||
    !pageText.includes('1360')
  ) {
    throw new Error(
      `Plugins page did not render the stored plugin state: ${pageText}`,
    )
  }
  await browser.saveScreenshot(resolve('target/m3-desktop-plugin-binding.png'))

  await browser.execute(() => {
    const settings = [
      ...document.querySelectorAll('.base-page [role="tab"]'),
    ].find((tab) =>
      ['Settings', '设置', '設定'].includes(tab.textContent?.trim()),
    )
    settings?.click()
  })
  await browser.waitUntil(
    () =>
      browser.execute(() => {
        const text = document.querySelector('.base-page')?.textContent ?? ''
        return ['Recent plugin activity', '近期插件活动', '近期外掛活動'].some(
          (title) => text.includes(title),
        )
      }),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Plugin audit panel did not render',
    },
  )
  const settingsText = await browser.execute(
    () => document.querySelector('.base-page')?.textContent ?? '',
  )
  if (!settingsText.includes('m3.smoke.plugin'))
    throw new Error(`Plugin audit activity did not render: ${settingsText}`)

  await browser.execute(() => {
    const binding = [
      ...document.querySelectorAll('.base-page [role="tab"]'),
    ].find((tab) =>
      ['Binding', '绑定', '綁定'].includes(tab.textContent?.trim()),
    )
    binding?.click()
  })
  await clickBindingAction(m3BindingIds[1], ['Disable', '停用'])
  await browser.waitUntil(
    async () =>
      (await invoke('clash5_list_plugin_bindings')).find(
        (item) => item.id === m3BindingIds[1],
      )?.enabled === false,
    {
      timeout: 20_000,
      interval: 250,
      timeoutMsg: 'Plugin binding did not disable from the UI',
    },
  )
  await clickBindingAction(m3BindingIds[0], ['Remove', '移除'])
  await browser.waitUntil(
    async () =>
      !(await invoke('clash5_list_plugin_bindings')).some(
        (item) => item.id === m3BindingIds[0],
      ),
    {
      timeout: 20_000,
      interval: 250,
      timeoutMsg: 'Plugin binding did not remove from the UI',
    },
  )
  await clickBindingAction(m3BindingIds[1], ['Remove', '移除'])
  await browser.waitUntil(
    async () =>
      !(await invoke('clash5_list_plugin_bindings')).some((item) =>
        m3BindingIds.includes(item.id),
      ),
    {
      timeout: 20_000,
      interval: 250,
      timeoutMsg: 'Disabled plugin binding did not remove from the UI',
    },
  )
  process.stdout.write(
    'M3 signed plugin install, SQLite bindings, deterministic merge, provenance, audit, UI disable/remove passed\n',
  )
}

async function cleanupM3PluginFixture() {
  if (!m3AppDirectory) return
  if (browser && m3BindingIds.length) {
    const remaining = await invoke('clash5_list_plugin_bindings').catch(
      () => [],
    )
    for (const binding of remaining.filter((item) =>
      m3BindingIds.includes(item.id),
    )) {
      await invoke('clash5_delete_plugin_binding', {
        target: binding.target,
        bindingId: binding.id,
      }).catch(() => {})
    }
  }
  if (m3StagingPath) await rm(m3StagingPath, { force: true }).catch(() => {})
  if (m3FixtureInstalled) {
    const pluginDirectory = join(m3AppDirectory, 'plugins', 'm3.smoke.plugin')
    await rm(pluginDirectory, { recursive: true, force: true })
  }
}

async function verifyNavigation({ includePluginFixture = true } = {}) {
  if (includePluginFixture) await prepareM3PluginFixture()
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
        document.querySelector('.the-logo img[alt="clash5"]'),
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
  if (includePluginFixture) {
    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            document
              .querySelector('.base-page')
              ?.textContent?.includes('M3 Acceptance Plugin') ?? false,
        ),
      {
        timeout: 20_000,
        interval: 250,
        timeoutMsg: 'The signed M3 plugin was not listed in the installed page',
      },
    )
  }
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
  if (includePluginFixture) await verifyM3PluginUi()
}

async function verifyThemeScreenshots() {
  const settingsOpened = await browser.execute(() => {
    const settings = [
      ...document.querySelectorAll('.the-menu .MuiListItemButton-root'),
    ].find((item) =>
      ['Settings', '设置', '設定'].includes(item.textContent?.trim()),
    )
    settings?.click()
    return Boolean(settings)
  })
  if (!settingsOpened) throw new Error('Settings navigation is missing')

  await browser.waitUntil(
    () =>
      browser.execute(() =>
        ['Settings', '设置', '設定'].includes(
          document
            .querySelector('.base-page > header .MuiTypography-root')
            ?.textContent?.trim() ?? '',
        ),
      ),
    { timeout: 10_000, interval: 250, timeoutMsg: 'Settings did not render' },
  )

  const selectTheme = async (labels, mode, canvasColor) => {
    const clicked = await browser.execute((values) => {
      const button = [...document.querySelectorAll('.base-page button')].find(
        (item) => values.includes(item.textContent?.trim()),
      )
      button?.click()
      return Boolean(button)
    }, labels)
    if (!clicked) throw new Error(`${mode} theme control is missing`)
    await browser.waitUntil(
      async () =>
        (await invoke('get_verge_config')).theme_mode === mode &&
        (await browser.execute(() =>
          document.documentElement.style
            .getPropertyValue('--background-color')
            .trim(),
        )) === canvasColor,
      {
        timeout: 10_000,
        interval: 250,
        timeoutMsg: `${mode} theme did not apply`,
      },
    )
  }

  await selectTheme(['Light', '浅色', '淺色'], 'light', '#F0F4FA')
  await browser.saveScreenshot(resolve('target/m6-settings-light.png'))
  await selectTheme(['Dark', '深色'], 'dark', '#2E303D')
  await browser.saveScreenshot(resolve('target/m6-settings-dark.png'))
  await selectTheme(['Light', '浅色', '淺色'], 'light', '#F0F4FA')
}

async function verifyIdentityRotationAndAmbiguousRebind() {
  phase = 'identity rotation cancellation'
  const beforeCancel = {
    status: await invoke('clash5_identity_status'),
    nodes: (await invoke('clash5_list_nodes', { includeInactive: true })).map(
      (node) => [node.source, node.stable_id, node.active],
    ),
    bindings: await invoke('clash5_list_bindings', { source: null }),
  }
  const clickButtonIn = async (labels, selector = '.base-page') => {
    const clicked = await browser.execute(
      (values, rootSelector) => {
        const roots = [...document.querySelectorAll(rootSelector)].filter(
          (root) => {
            const surface = root.matches('.MuiDialog-root')
              ? root.querySelector('[role="dialog"]')
              : root
            if (!surface) return false
            const style = getComputedStyle(surface)
            const bounds = surface.getBoundingClientRect()
            return (
              style.visibility !== 'hidden' &&
              bounds.width > 0 &&
              bounds.height > 0
            )
          },
        )
        const button = roots
          .flatMap((root) => [...root.querySelectorAll('button')])
          .find((item) => values.includes(item.textContent?.trim()))
        button?.click()
        return Boolean(button)
      },
      labels,
      selector,
    )
    if (!clicked) throw new Error(`Button is missing: ${labels[0]}`)
  }
  const rotateLabels = ['Rotate identity key', '轮换身份密钥', '輪替身分金鑰']
  const cancelLabels = ['Cancel', '取消']
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
      timeoutMsg: 'Plugin binding tab did not render before identity recovery',
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
      browser.execute(
        (labels) =>
          [...document.querySelectorAll('.base-page button')].some((button) =>
            labels.includes(button.textContent?.trim()),
          ),
        rotateLabels,
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Identity controls did not render on the plugin binding tab',
    },
  )
  await clickButtonIn(rotateLabels)
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        [...document.querySelectorAll('.MuiDialogTitle-root')].some((title) =>
          [
            'Rotate the node identity key?',
            '轮换节点身份密钥？',
            '輪替節點身分金鑰？',
          ].includes(title.textContent?.trim()),
        ),
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Identity rotation confirmation did not open',
    },
  )
  await clickButtonIn(cancelLabels, '.MuiDialog-root')
  await browser.waitUntil(
    () =>
      browser.execute(
        () => document.querySelectorAll('.MuiDialog-root').length === 0,
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Identity rotation dialog did not close after cancellation',
    },
  )
  const afterCancel = {
    status: await invoke('clash5_identity_status'),
    nodes: (await invoke('clash5_list_nodes', { includeInactive: true })).map(
      (node) => [node.source, node.stable_id, node.active],
    ),
    bindings: await invoke('clash5_list_bindings', { source: null }),
  }
  if (JSON.stringify(beforeCancel) !== JSON.stringify(afterCancel)) {
    throw new Error('Cancelling identity rotation changed persisted data')
  }

  phase = 'ambiguous manual rebind'
  const pending = await invoke('clash5_list_pending_identity_rebinds', {
    source: null,
  })
  const item = pending.find((candidate) => candidate.candidate_ids.length === 2)
  if (!item)
    throw new Error('Subscription refresh did not create two rebind candidates')
  const oldBinding = (
    await invoke('clash5_list_bindings', { source: null })
  ).find((binding) => binding.id === 'm1-ambiguous-rebind')
  if (!oldBinding?.orphan || oldBinding.target_id !== item.old_stable_id) {
    throw new Error('Rotation did not preserve the old binding as an orphan')
  }
  const nodes = await invoke('clash5_list_nodes', { includeInactive: true })
  const candidateNames = item.candidate_ids.map(
    (id) => nodes.find((node) => node.stable_id === id)?.display_name,
  )
  if (
    candidateNames.length !== 2 ||
    candidateNames.some((name) => !name?.startsWith('M1-candidate-'))
  ) {
    throw new Error(
      `Unexpected rebind candidates: ${JSON.stringify(candidateNames)}`,
    )
  }

  const oldId = item.old_stable_id
  const opened = await browser.execute((id) => {
    const card = [
      ...document.querySelectorAll('.base-page .MuiPaper-root'),
    ].find(
      (paper) =>
        paper.textContent?.includes(id) &&
        paper.querySelector('[role="combobox"]'),
    )
    const select = card?.querySelector('[role="combobox"]')
    // MUI Select opens on mousedown; HTMLElement.click() dispatches only click.
    select?.dispatchEvent(
      new MouseEvent('mousedown', {
        bubbles: true,
        cancelable: true,
        view: window,
        button: 0,
      }),
    )
    return Boolean(select)
  }, oldId)
  if (!opened) throw new Error('Ambiguous rebind candidate selector is missing')
  const rebindInitiallyDisabled = await browser.execute((id) => {
    const card = [
      ...document.querySelectorAll('.base-page .MuiPaper-root'),
    ].find(
      (paper) =>
        paper.textContent?.includes(id) &&
        paper.querySelector('[role="combobox"]'),
    )
    return [...(card?.querySelectorAll('button') ?? [])].some(
      (button) =>
        button.disabled &&
        ['Rebind', '重新绑定', '重新綁定'].includes(button.textContent?.trim()),
    )
  }, oldId)
  if (!rebindInitiallyDisabled) {
    throw new Error('Ambiguous rebind was enabled without an explicit choice')
  }
  await browser.waitUntil(
    () =>
      browser.execute(
        (id) =>
          [...document.querySelectorAll('[role="option"]')].some(
            (option) => option.textContent?.trim() === id,
          ),
        item.candidate_ids[0],
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Ambiguous candidate options did not open',
    },
  )
  await browser.execute((id) => {
    ;[...document.querySelectorAll('[role="option"]')]
      .find((option) => option.textContent?.trim() === id)
      ?.click()
  }, item.candidate_ids[0])
  const rebindEnabled = await browser.execute((id) => {
    const card = [
      ...document.querySelectorAll('.base-page .MuiPaper-root'),
    ].find(
      (paper) =>
        paper.textContent?.includes(id) &&
        paper.querySelector('[role="combobox"]'),
    )
    return [...(card?.querySelectorAll('button') ?? [])].some(
      (button) =>
        !button.disabled &&
        ['Rebind', '重新绑定', '重新綁定'].includes(button.textContent?.trim()),
    )
  }, oldId)
  if (!rebindEnabled) throw new Error('Rebind stayed disabled after selection')
  await browser.execute((id) => {
    const card = [
      ...document.querySelectorAll('.base-page .MuiPaper-root'),
    ].find(
      (paper) =>
        paper.textContent?.includes(id) &&
        paper.querySelector('[role="combobox"]'),
    )
    ;[...(card?.querySelectorAll('button') ?? [])]
      .find((button) =>
        ['Rebind', '重新绑定', '重新綁定'].includes(button.textContent?.trim()),
      )
      ?.click()
  }, oldId)
  await browser.waitUntil(
    async () => (await invoke('clash5_identity_status')).pending_rebinds === 0,
    {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: 'Explicit rebind choice was not saved',
    },
  )
  const selectedBinding = (
    await invoke('clash5_list_bindings', { source: null })
  ).find((binding) => binding.id === 'm1-ambiguous-rebind')
  if (
    selectedBinding?.target_id !== item.candidate_ids[0] ||
    selectedBinding.orphan
  ) {
    throw new Error(
      `Explicit rebind was not applied: ${JSON.stringify(selectedBinding)}`,
    )
  }
  const selectedNode = nodes.find(
    (node) => node.stable_id === item.candidate_ids[0],
  )
  if (!selectedNode) throw new Error('Selected rebind target is missing')

  phase = 'identity rotation'
  await browser.execute(() => {
    const root = document.documentElement
    const key = 'clash5SmokeIdentityRotation'
    root.dataset[key] = 'pending'
    window.__TAURI_INTERNALS__
      .invoke('clash5_rotate_identity')
      .then(() => {
        root.dataset[key] = 'success'
      })
      .catch((error) => {
        root.dataset[key] =
          `error:${typeof error === 'string' ? error : JSON.stringify(error)}`
      })
  })
  await browser.waitUntil(
    () =>
      browser.execute(
        () =>
          document.documentElement.dataset.clash5SmokeIdentityRotation !==
          'pending',
      ),
    {
      timeout: 180_000,
      interval: 250,
      timeoutMsg: 'Identity rotation command did not finish within 180 seconds',
    },
  )
  const rotationResult = await browser.execute(
    () => document.documentElement.dataset.clash5SmokeIdentityRotation,
  )
  if (rotationResult !== 'success') {
    throw new Error(`Identity rotation command failed: ${rotationResult}`)
  }
  let rotatedTarget
  try {
    await browser.waitUntil(
      async () => {
        const currentNodes = await invoke('clash5_list_nodes', {
          includeInactive: true,
        })
        rotatedTarget = currentNodes.find(
          (node) =>
            node.source.kind === selectedNode.source.kind &&
            node.source.provider_id === selectedNode.source.provider_id &&
            node.display_name === selectedNode.display_name &&
            node.active,
        )
        return Boolean(
          rotatedTarget && rotatedTarget.stable_id !== selectedNode.stable_id,
        )
      },
      {
        timeout: 60_000,
        interval: 500,
        timeoutMsg: 'Identity rotation did not finish',
      },
    )
  } catch (error) {
    const currentNodes = await invoke('clash5_list_nodes', {
      includeInactive: true,
    })
    const diagnostics = currentNodes.map((node) => ({
      source: node.source,
      stable_id: node.stable_id,
      display_name: node.display_name,
      port: node.port,
      active: node.active,
    }))
    throw new Error(
      `${String(error)}; identity inventory: ${JSON.stringify(diagnostics)}`,
      { cause: error },
    )
  }
  const rotatedNodes = await invoke('clash5_list_nodes', {
    includeInactive: true,
  })
  rotatedTarget = rotatedNodes.find(
    (node) =>
      node.source.kind === selectedNode.source.kind &&
      node.source.provider_id === selectedNode.source.provider_id &&
      node.display_name === selectedNode.display_name &&
      node.active,
  )
  const rotatedBinding = (
    await invoke('clash5_list_bindings', { source: null })
  ).find((binding) => binding.id === 'm1-ambiguous-rebind')
  if (
    !rotatedTarget ||
    rotatedTarget.stable_id === selectedNode.stable_id ||
    rotatedBinding?.target_id !== rotatedTarget.stable_id ||
    rotatedBinding.orphan
  ) {
    throw new Error(
      `Identity rotation did not preserve the explicit binding: ${JSON.stringify({ rotatedTarget, rotatedBinding })}`,
    )
  }
}

async function verifyRecoveryResetCancellation() {
  phase = 'identity recovery reset cancellation'
  const before = {
    status: await invoke('clash5_identity_status'),
    nodes: await invoke('clash5_list_nodes', { includeInactive: true }),
    bindings: await invoke('clash5_list_bindings', { source: null }),
  }
  if (!before.status.recovery_mode) {
    throw new Error('Desktop smoke did not start in identity recovery mode')
  }
  const clicked = await browser.execute(() => {
    const button = [...document.querySelectorAll('.base-page button')].find(
      (item) =>
        ['Reset identity key', '重置身份密钥', '重設身分金鑰'].includes(
          item.textContent?.trim(),
        ),
    )
    button?.click()
    return Boolean(button)
  })
  if (!clicked) throw new Error('Identity recovery reset action is missing')
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        [...document.querySelectorAll('.MuiDialogTitle-root')].some((title) =>
          [
            'Reset node identity key?',
            '重置节点身份密钥？',
            '重設節點身分金鑰？',
          ].includes(title.textContent?.trim()),
        ),
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Identity reset confirmation did not open',
    },
  )
  const cancelled = await browser.execute(() => {
    const button = [
      ...document.querySelectorAll('.MuiDialog-root button'),
    ].find((item) => ['Cancel', '取消'].includes(item.textContent?.trim()))
    button?.click()
    return Boolean(button)
  })
  if (!cancelled)
    throw new Error('Identity reset cancellation action is missing')
  await browser.waitUntil(
    () =>
      browser.execute(
        () => document.querySelectorAll('.MuiDialog-root').length === 0,
      ),
    {
      timeout: 10_000,
      interval: 250,
      timeoutMsg: 'Identity reset dialog did not close after cancellation',
    },
  )
  const after = {
    status: await invoke('clash5_identity_status'),
    nodes: await invoke('clash5_list_nodes', { includeInactive: true }),
    bindings: await invoke('clash5_list_bindings', { source: null }),
  }
  if (
    !after.status.recovery_mode ||
    JSON.stringify(before) !== JSON.stringify(after)
  ) {
    throw new Error('Cancelling identity reset changed persisted data')
  }
}

async function verifyM2Chains() {
  chainFixture = await startChainFixture(true)
  m4Fixture = await startM4SingboxFixture()
  const fileData = dump({
    proxies: [
      {
        name: 'M2-front',
        type: 'http',
        server: '127.0.0.1',
        port: chainFixture.frontPort,
      },
      {
        name: 'M4-front',
        type: 'http',
        server: '127.0.0.1',
        port: m4Fixture.frontPort,
      },
    ],
    'proxy-providers': {
      'M2-provider': {
        type: 'inline',
        payload: [
          {
            name: 'M2-landing',
            type: 'http',
            server: '127.0.0.1',
            port: chainFixture.landingPort,
          },
          {
            name: 'M4-landing',
            type: 'ss',
            server: '127.0.0.1',
            port: m4Fixture.landingPort,
            cipher: 'aes-128-gcm',
            password: m4Fixture.password,
          },
        ],
        override: { udp: false },
      },
    },
    'proxy-groups': [
      {
        name: 'M2-selected',
        type: 'select',
        proxies: ['DIRECT', 'M2-front'],
        use: ['M2-provider'],
      },
    ],
    rules: ['MATCH,M2-selected'],
  })
  await invoke('create_profile', {
    item: { type: 'local', name: 'M2-local-chain-fixture' },
    fileData,
  })
  const profiles = await invoke('get_profiles')
  const profile = profiles.items.find(
    (item) => item.name === 'M2-local-chain-fixture',
  )
  if (!profile?.uid) throw new Error('M2 fixture profile was not created')
  const switched = await invoke('patch_profiles_config', {
    profiles: { current: profile.uid },
  })
  if (switched.status !== 'valid')
    throw new Error('M2 fixture profile was rejected')
  const activeProfiles = await invoke('get_profiles')
  const activeRuntime = await invoke('get_runtime_config')
  if (activeProfiles.current !== profile.uid)
    throw new Error('M2 fixture profile did not become active')
  if (!activeRuntime.proxies?.some((proxy) => proxy.name === 'M2-front'))
    throw new Error(
      `M2 fixture proxy was not generated: ${activeRuntime.proxies?.map((proxy) => proxy.name).join(', ')}`,
    )
  const backendWorkspace = await invoke('clash5_chain_workspace')
  if (!backendWorkspace.nodes.some((node) => node.display_name === 'M2-front'))
    throw new Error(
      `M2 front node was absent from workspace: ${backendWorkspace.nodes.map((node) => node.display_name).join(', ')}`,
    )
  await browser.execute(() => {
    localStorage.setItem('clash5-exit-ip-url', 'http://127.0.0.1:18090/ip')
    window.history.pushState({}, '', '/chains')
    window.dispatchEvent(new PopStateEvent('popstate'))
  })
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        Boolean(
          document.querySelector('[data-testid="chains-workspace"] input'),
        ),
      ),
    { timeout: 20_000 },
  )
  await browser
    .$('[data-testid="chains-workspace"] input')
    .setValue('M2 desktop chain')
  for (const [index, name] of [
    [0, 'M2-front'],
    [1, 'M2-landing'],
  ]) {
    const control = await browser
      .$$('[data-testid="chains-workspace"] [role="combobox"]')
      .then((controls) => controls[index])
    await control.click()
    await browser.waitUntil(
      () =>
        browser.execute(
          (prefix) =>
            [...document.querySelectorAll('[role="option"]')].some((option) =>
              option.textContent?.startsWith(prefix),
            ),
          name,
        ),
      { timeout: 5_000 },
    )
    await browser.execute(
      (prefix) =>
        [...document.querySelectorAll('[role="option"]')]
          .find((option) => option.textContent?.startsWith(prefix))
          ?.click(),
      name,
    )
    await browser.waitUntil(
      () => browser.execute(() => !document.querySelector('[role="listbox"]')),
      { timeout: 5_000 },
    )
  }
  await clickButton(['Save chain', '保存链路', '儲存鏈路'])
  let workspace
  await browser.waitUntil(
    async () => {
      workspace = await invoke('clash5_chain_workspace')
      return workspace.chains.some((chain) => chain.name === 'M2 desktop chain')
    },
    { timeout: 45_000, timeoutMsg: 'The desktop chain was not saved' },
  )
  const chain = workspace.chains.find(
    (chain) => chain.name === 'M2 desktop chain',
  )
  const landingHop = chain.hops.at(-1)
  if (
    landingHop?.kind !== 'node' ||
    !landingHop.node.source.provider_id?.endsWith(':M2-provider')
  )
    throw new Error('Chain did not retain its external provider source')
  const tested = await invoke('clash5_test_chain', {
    id: chain.id,
    testUrl: 'http://127.0.0.1:18090/health',
  })
  if (
    typeof tested.delay_ms !== 'number' ||
    tested.group_name !== `clash5-chain-${chain.id}`
  )
    throw new Error('Chain connectivity bridge returned an invalid result')
  await waitButtonReady(['Test chain', '测试链路', '測試鏈路'])
  await clickButton(['Detect exit IP', '检测出口 IP', '偵測出口 IP'])
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        [...document.querySelectorAll('.MuiAlert-root')].some((alert) =>
          alert.textContent?.includes('203.0.113.27'),
        ),
      ),
    {
      timeout: 45_000,
      timeoutMsg: 'Exit IP did not render through the real chain',
    },
  )
  const activation = await invoke('clash5_activate_chain', {
    id: chain.id,
    policyGroup: 'M2-selected',
  })
  if (!activation.selection_persisted)
    throw new Error('Chain selection did not persist')
  const runtime = await invoke('get_runtime_config')
  const original = runtime.proxies.find((proxy) => proxy.name === 'M2-front')
  const lastHop = `clash5-chain-${chain.id}-h${chain.hops.length - 1}`
  const landing = runtime.proxies.find((proxy) => proxy.name === lastHop)
  if (
    original['dialer-proxy'] ||
    landing['dialer-proxy'] !== `clash5-chain-${chain.id}-h0`
  )
    throw new Error(
      'Generated chain changed the original or reversed hop direction',
    )
  let legacyRejected = false
  try {
    await invoke('update_proxy_chain_config_in_runtime', {
      proxyChainConfig: null,
    })
  } catch {
    legacyRejected = true
  }
  if (!legacyRejected)
    throw new Error(
      'Legacy chain mutation was allowed to overwrite a managed chain',
    )
  if (
    runtime.listeners?.some((listener) =>
      listener.name.startsWith('clash5-exit-probe-'),
    )
  )
    throw new Error('Exit-IP probe listener leaked into the runtime')
  const warning = await invoke('clash5_detect_exit_ip', {
    id: chain.id,
    serviceUrl: 'http://127.0.0.1:18090/unavailable',
  })
  if (warning.status !== 'warning')
    throw new Error('Default L2 policy did not warn on service failure')
  const afterWarning = await invoke('get_proxy_view')
  if (
    afterWarning.groups.find((group) => group.name === 'M2-selected')?.now !==
    `clash5-chain-${chain.id}`
  )
    throw new Error('Warning unexpectedly switched the policy group')
  const disabled = await invoke('clash5_detect_exit_ip', {
    id: chain.id,
    serviceUrl: null,
  })
  if (disabled.status !== 'disabled')
    throw new Error('Disabled detection was not respected')
  let rejected = false
  try {
    await invoke('clash5_save_chain', {
      chain: {
        ...chain,
        hops: [
          ...chain.hops.slice(0, -1),
          {
            kind: 'node',
            node: {
              ...chain.hops.at(-1).node,
              stable_id: 'fixture-missing-node',
            },
          },
        ],
      },
    })
  } catch {
    rejected = true
  }
  if (!rejected) throw new Error('Missing chain node was accepted')
  workspace = await invoke('clash5_chain_workspace')
  if (
    JSON.stringify(workspace.chains.find((saved) => saved.id === chain.id)) !==
    JSON.stringify(chain)
  )
    throw new Error('Rejected chain edit did not restore its saved definition')
  const limited = workspace.nodes.find(
    (node) => node.reference.source.kind === 'manual' && node.port === 18081,
  )
  if (!limited)
    throw new Error(
      'Rate-limited manual node is unavailable to the chain fixture',
    )
  const failing = {
    ...chain,
    id: '28dc0b8a-7485-49bc-8a91-f48bdc736846',
    name: 'M2 verified fallback policy',
    hops: [{ kind: 'node', node: limited.reference }, ...chain.hops.slice(1)],
    exit_policy: { mode: 'switch', fallback_chain_id: chain.id },
  }
  await invoke('clash5_save_chain', { chain: failing })
  const fallback = await invoke('clash5_detect_exit_ip', {
    id: failing.id,
    serviceUrl: 'http://127.0.0.1:18090/ip',
  })
  if (
    fallback.status !== 'switched' ||
    fallback.fallback_chain_id !== chain.id ||
    fallback.observation.ip !== '203.0.113.27'
  )
    throw new Error(`Verified fallback failed: ${JSON.stringify(fallback)}`)
  const afterFallback = await invoke('get_proxy_view')
  if (
    afterFallback.groups.find(
      (group) => group.name === `clash5-chain-${failing.id}`,
    )?.now !== `clash5-chain-${chain.id}`
  )
    throw new Error('Verified fallback was not selected')
  if (!chainFixture.frontStats.length || !chainFixture.landingStats.length)
    throw new Error('Desktop exit-IP request bypassed a hop')
  const layout = await browser.execute(() => {
    const content = document.querySelector('.base-content')
    return {
      clientWidth: content?.clientWidth ?? 0,
      scrollWidth: content?.scrollWidth ?? 0,
    }
  })
  if (layout.scrollWidth > layout.clientWidth + 1)
    throw new Error(
      `Chains page overflows its content area: ${JSON.stringify(layout)}`,
    )
  await browser.saveScreenshot(resolve('target/m2-desktop-chains.png'))
  process.stdout.write(
    'M2 desktop save, provider chain, exit-IP, warning, rollback and verified fallback passed\n',
  )
}

async function verifyM4SingboxRoute() {
  phase = 'M4 real sing-box route activation'
  const workspace = await invoke('clash5_chain_workspace')
  const original = workspace.chains.find(
    (chain) => chain.name === 'M2 desktop chain',
  )
  const front = workspace.nodes.find((node) => node.display_name === 'M4-front')
  const landing = workspace.nodes.find(
    (node) => node.display_name === 'M4-landing',
  )
  if (!original || !front || !landing)
    throw new Error('M4 local Shadowsocks chain fixture was not generated')
  const chain = {
    ...original,
    id: randomUUID(),
    name: 'M4 real sing-box route',
    hops: [
      { kind: 'node', node: front.reference },
      { kind: 'node', node: landing.reference },
    ],
  }
  await invoke('clash5_save_chain', { chain })
  await invoke('patch_verge_config', { payload: { singbox_persistent: true } })
  const persistentConfig = await invoke('get_verge_config')
  if (persistentConfig.singbox_persistent !== true)
    throw new Error(
      `M4 keep-running preference did not persist: ${JSON.stringify(persistentConfig.singbox_persistent)}`,
    )
  const activation = await invoke('clash5_activate_chain', {
    id: chain.id,
    policyGroup: 'M2-selected',
  })
  if (!activation.selection_persisted)
    throw new Error('M4 chain selection did not persist')

  const landingName = `clash5-chain-${chain.id}-h${chain.hops.length - 1}`
  let status = await invoke('clash5_singbox_route_status', { landingName })
  if (
    !status.active ||
    status.degraded ||
    status.mihomo_proxy_type !== 'Socks5'
  )
    throw new Error(
      `M4 sidecar route was not active: ${JSON.stringify(status)}`,
    )

  const observation = await invoke('clash5_detect_exit_ip', {
    id: chain.id,
    serviceUrl: 'http://127.0.0.1:18090/ip',
  })
  if (
    observation.status !== 'healthy' ||
    observation.observation.ip !== '203.0.113.27' ||
    !m4Fixture.frontStats.includes(`127.0.0.1:${m4Fixture.landingPort}`)
  )
    throw new Error(
      `M4 traffic did not traverse the real sing-box route: ${JSON.stringify(observation)}`,
    )

  phase = 'M4 persistent sidecar behavior'
  const persistentDeadline = Date.now() + 135_000
  let routeDriftRecovered = false
  while (Date.now() < persistentDeadline) {
    await sleep(Math.min(5_000, persistentDeadline - Date.now()))
    status = await invoke('clash5_singbox_route_status', { landingName })
    if (
      status.active &&
      !status.degraded &&
      status.mihomo_proxy_type === 'Socks5'
    )
      continue

    const recoveryDeadline = Date.now() + 45_000
    while (
      Date.now() < recoveryDeadline &&
      (!status.active ||
        status.degraded ||
        status.mihomo_proxy_type !== 'Socks5')
    ) {
      await sleep(2_000)
      status = await invoke('clash5_singbox_route_status', { landingName })
    }
    if (
      !status.active ||
      status.degraded ||
      status.mihomo_proxy_type !== 'Socks5'
    )
      throw new Error(
        `Persistent M4 route drift was not recovered: ${JSON.stringify(status)}`,
      )
    routeDriftRecovered = true
  }
  const persistentConfigAfterIdle = await invoke('get_verge_config')
  if (
    persistentConfigAfterIdle.singbox_persistent !== true ||
    !status.active ||
    status.mihomo_proxy_type !== 'Socks5'
  )
    throw new Error(
      `Persistent M4 route changed unexpectedly: ${JSON.stringify({ persistent: persistentConfigAfterIdle.singbox_persistent, ...status })}`,
    )
  if (routeDriftRecovered)
    process.stdout.write(
      'M4 supervisor recovered Mihomo route drift while persistent mode was enabled\n',
    )

  phase = 'M4 idle sidecar shutdown and Mihomo rollback'
  await invoke('patch_verge_config', { payload: { singbox_persistent: false } })
  let stopped = false
  for (let attempt = 0; attempt < 24; attempt += 1) {
    await sleep(5_000)
    status = await invoke('clash5_singbox_route_status', { landingName })
    if (!status.active) {
      stopped = true
      break
    }
  }
  if (!stopped || status.mihomo_proxy_type !== 'Shadowsocks' || status.degraded)
    throw new Error(
      `Idle M4 cleanup did not restore the original Mihomo landing proxy: ${JSON.stringify(status)}`,
    )
  process.stdout.write(
    'M4 real sing-box traffic, persistent mode, idle shutdown, and Mihomo Shadowsocks rollback passed\n',
  )
  await browser.saveScreenshot(resolve('target/m4-desktop-route.png'))
}

try {
  if (persisted) {
    await startFixtures()
    chainFixture = await startChainFixture(true)
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
    throw new Error(`${String(error)}; last error: ${lastError ?? 'none'}`, {
      cause: error,
    })
  }
  const memoryStatus = await invoke('clash5_memory_mitigation_status')
  if (
    typeof memoryStatus?.active !== 'boolean' ||
    typeof memoryStatus.monitoring !== 'boolean' ||
    (memoryStatus.availablePercent != null &&
      (!Number.isInteger(memoryStatus.availablePercent) ||
        memoryStatus.availablePercent < 0 ||
        memoryStatus.availablePercent > 100))
  ) {
    throw new Error(
      `Invalid memory mitigation status: ${JSON.stringify(memoryStatus)}`,
    )
  }
  if (!persisted && !recoveryCancel && inventory.length !== 0) {
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
    (!persisted && !recoveryCancel && page.rows !== 0)
  ) {
    throw new Error(`Unexpected node workspace: ${JSON.stringify(page)}`)
  }

  if (full) {
    phase = 'manual import and probe'
    await verifyImportAndProbe()
    phase = 'subscription refresh and provider probe'
    await verifySubscriptionRefresh()
    await verifyWorkspaceControls()
  } else if (recoveryCancel) {
    await verifyNavigation({ includePluginFixture: false })
    await verifyRecoveryResetCancellation()
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
    phase = 'M6 light and dark settings appearance'
    await verifyThemeScreenshots()
    phase = 'M2 chain editor and exit-IP policy'
    await verifyM2Chains()
    await verifyIdentityRotationAndAmbiguousRebind()
    await verifyM4SingboxRoute()
  } else if (persisted) {
    const savedTheme = await invoke('get_verge_config')
    if (savedTheme.theme_mode !== 'light')
      throw new Error('Selected light theme did not survive desktop restart')
    await invoke('patch_verge_config', { payload: { theme_mode: 'system' } })
    const workspace = await invoke('clash5_chain_workspace')
    const original = workspace.chains.find(
      (chain) => chain.name === 'M2 desktop chain',
    )
    const fallback = workspace.chains.find(
      (chain) => chain.id === '28dc0b8a-7485-49bc-8a91-f48bdc736846',
    )
    if (!original || !fallback)
      throw new Error('M2 chains did not survive a desktop restart')
    const runtime = await invoke('get_runtime_config')
    if (
      !workspace.chains.every((chain) =>
        runtime['proxy-groups']?.some(
          (group) => group.name === `clash5-chain-${chain.id}`,
        ),
      )
    )
      throw new Error('Saved M2 chains were not regenerated after restart')
    const view = await invoke('get_proxy_view')
    if (
      view.groups.find((group) => group.name === `clash5-chain-${fallback.id}`)
        ?.now !== `clash5-chain-${original.id}`
    )
      throw new Error(
        'Verified fallback selection did not survive desktop restart',
      )
    process.stdout.write(
      'M2 stable chains survived restart and regenerated their runtime groups\n',
    )
  }
  process.stdout.write(
    persisted
      ? 'Manual nodes and their stable identities survived a desktop restart\n'
      : recoveryCancel
        ? 'Identity reset cancellation preserved recovery state and data\n'
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
  await cleanupM3PluginFixture().catch((error) => {
    process.stderr.write(`M3 fixture cleanup failed: ${String(error)}\n`)
  })
  if (browser)
    await invoke('patch_verge_config', {
      payload: { singbox_persistent: false },
    }).catch(() => {})
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
  await m4Fixture?.close()
  await chainFixture?.close()
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
