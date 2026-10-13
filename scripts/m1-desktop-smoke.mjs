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
  logLevel: 'debug',
  startTimeout: 120_000,
  commandTimeout: 60_000,
})
let browser
const screenshot = resolve('target/m1-desktop-smoke.png')

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

  await capture()
  process.stdout.write('Native Tauri bridge and empty node workspace passed\n')
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
  if (browser) await cleanupWdioSession(browser)
}
