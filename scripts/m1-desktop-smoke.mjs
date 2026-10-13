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

const binary = process.argv[2]
if (!binary)
  throw new Error('Usage: node scripts/m1-desktop-smoke.mjs <app-binary>')
const full = process.argv.includes('--full')

const capabilities = createTauriCapabilities(resolve(binary), {
  driverProvider: 'external',
  autoInstallTauriDriver: true,
  logLevel: 'debug',
  startTimeout: 120_000,
  commandTimeout: 60_000,
})
let browser
let originalClipboard
const screenshot = resolve('target/m1-desktop-smoke.png')
const fixtures = []

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
  const forward = startFixture(18080, 'forward')
  const limited = startFixture(18081, 'limit')
  await Promise.all([
    fixtureReady(18080, forward),
    fixtureReady(18081, limited),
  ])

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

async function capture() {
  await mkdir(resolve('target'), { recursive: true })
  await browser.saveScreenshot(screenshot)
}

try {
  browser = await startWdioSession(capabilities)
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
  if (inventory.length !== 0) {
    throw new Error(`Fresh desktop session contains ${inventory.length} nodes`)
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
  if (!page.title || page.title.includes('nodePool.') || page.rows !== 0) {
    throw new Error(`Unexpected empty node workspace: ${JSON.stringify(page)}`)
  }

  if (full) await verifyImportAndProbe()
  await capture()
  process.stdout.write(
    full
      ? 'Native Tauri import, deduplication, probe isolation, and cache passed\n'
      : 'Native Tauri bridge and empty node workspace passed\n',
  )
} catch (error) {
  if (browser) {
    await capture().catch((captureError) => {
      process.stderr.write(
        `Desktop screenshot failed: ${String(captureError)}\n`,
      )
    })
  }
  throw error
} finally {
  if (browser && typeof originalClipboard === 'string') {
    await invoke('plugin:clipboard-manager|write_text', {
      text: originalClipboard,
    }).catch(() => {})
  }
  if (browser) await cleanupWdioSession(browser)
  for (const child of fixtures.reverse()) {
    if (child.exitCode === null) {
      const closed = once(child, 'close')
      child.kill()
      await closed
    }
  }
}
