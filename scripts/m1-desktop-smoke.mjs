import { mkdir } from 'node:fs/promises'
import { resolve } from 'node:path'

import {
  cleanupWdioSession,
  createTauriCapabilities,
  startWdioSession,
} from '@wdio/tauri-service'

const binary = process.argv[2]
if (!binary)
  throw new Error('Usage: node scripts/m1-desktop-smoke.mjs <app-binary>')

const capabilities = createTauriCapabilities(resolve(binary), {
  driverProvider: 'external',
  autoInstallTauriDriver: true,
  startTimeout: 120_000,
  commandTimeout: 60_000,
})
let browser

try {
  browser = await startWdioSession(capabilities)
  let inventory
  await browser.waitUntil(
    async () => {
      try {
        const result = await browser.executeAsync((done) => {
          const invoke = window.__TAURI_INTERNALS__?.invoke
          if (!invoke) {
            done({ error: 'Tauri bridge is unavailable' })
            return
          }
          invoke('clash5_list_nodes', { includeInactive: true }).then(
            (value) => done({ value }),
            (error) => done({ error: String(error) }),
          )
        })
        inventory = result.value
        return Array.isArray(inventory)
      } catch {
        return false
      }
    },
    {
      timeout: 60_000,
      interval: 1_000,
      timeoutMsg: 'clash5_list_nodes did not become available',
    },
  )
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

  await mkdir(resolve('target'), { recursive: true })
  await browser.saveScreenshot(resolve('target/m1-desktop-smoke.png'))
  process.stdout.write('Native Tauri bridge and empty node workspace passed\n')
} finally {
  if (browser) await cleanupWdioSession(browser)
}
