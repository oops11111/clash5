import { spawn } from 'node:child_process'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { once } from 'node:events'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'

const binary = process.argv[2]
if (!binary)
  throw new Error('Usage: node scripts/m1-mihomo-smoke.mjs <mihomo-binary>')

const controller = 'http://127.0.0.1:19090'
const fixture = (port) => `http://127.0.0.1:${port}`
const children = []
let directory

function start(command, args) {
  const child = spawn(command, args, { stdio: ['ignore', 'pipe', 'pipe'] })
  let output = ''
  for (const stream of [child.stdout, child.stderr]) {
    stream.on('data', (chunk) => {
      output += chunk.toString()
    })
  }
  children.push(child)
  return { child, output: () => output }
}

async function ready(url, process, timeoutMs = 20_000) {
  const until = Date.now() + timeoutMs
  while (Date.now() < until) {
    if (process.child.exitCode !== null) {
      throw new Error(`Process exited early: ${process.output()}`)
    }
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(1_000) })
      if (response.ok) return
    } catch {}
    await sleep(250)
  }
  throw new Error(`Timed out waiting for ${url}: ${process.output()}`)
}

async function delay(name, url) {
  const endpoint = new URL(
    `/proxies/${encodeURIComponent(name)}/delay`,
    controller,
  )
  endpoint.searchParams.set('url', url)
  endpoint.searchParams.set('timeout', '7000')
  const response = await fetch(endpoint, {
    signal: AbortSignal.timeout(10_000),
  })
  const body = await response.json()
  return { ok: response.ok, body }
}

try {
  directory = await mkdtemp(join(tmpdir(), 'clash5-m1-mihomo-'))
  const forward = start(process.execPath, [
    resolve('scripts/m1-probe-fixture.mjs'),
    '18080',
    'forward',
  ])
  const limited = start(process.execPath, [
    resolve('scripts/m1-probe-fixture.mjs'),
    '18081',
    'limit',
  ])
  await Promise.all([
    ready(`${fixture(18080)}/__stats`, forward),
    ready(`${fixture(18081)}/__stats`, limited),
  ])

  const config = join(directory, 'config.yaml')
  await writeFile(
    config,
    `mixed-port: 0
allow-lan: false
mode: rule
log-level: warning
external-controller: 127.0.0.1:19090
proxies:
  - {name: M1-forward, type: http, server: 127.0.0.1, port: 18080}
  - {name: M1-limit, type: http, server: 127.0.0.1, port: 18081}
proxy-groups:
  - {name: M1-test, type: select, proxies: [M1-forward, M1-limit]}
rules:
  - MATCH,M1-test
`,
  )
  const mihomo = start(resolve(binary), ['-d', directory, '-f', config])
  await ready(`${controller}/version`, mihomo)

  for (const url of [
    'http://cp.cloudflare.com/generate_204',
    'https://cp.cloudflare.com/generate_204',
  ]) {
    const success = await delay('M1-forward', url)
    if (!success.ok || !(success.body.delay > 0)) {
      const stats = await (await fetch(`${fixture(18080)}/__stats`)).json()
      throw new Error(
        `Forward probe failed for ${url}: ${JSON.stringify({ success, stats, mihomo: mihomo.output() })}`,
      )
    }
    const failure = await delay('M1-limit', url)
    if (failure.ok && failure.body.delay > 0) {
      throw new Error(`Rate-limited probe unexpectedly succeeded for ${url}`)
    }
  }
  const forwardStats = await (await fetch(`${fixture(18080)}/__stats`)).json()
  const limitStats = await (await fetch(`${fixture(18081)}/__stats`)).json()
  if (forwardStats.tunnels < 2 || limitStats.tunnels < 2) {
    throw new Error(
      `Unexpected fixture traffic: ${JSON.stringify({ forwardStats, limitStats })}`,
    )
  }
  process.stdout.write('Mihomo HTTP/HTTPS probe and 429 failure smoke passed\n')
} finally {
  for (const child of children.reverse()) {
    if (child.exitCode === null) {
      const stopped = once(child, 'close')
      child.kill()
      await stopped
    }
  }
  if (directory) await rm(directory, { recursive: true, force: true })
}
