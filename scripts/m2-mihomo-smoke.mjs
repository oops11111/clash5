import assert from 'node:assert/strict'
import { spawn, execFileSync } from 'node:child_process'
import { once } from 'node:events'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import http from 'node:http'
import net from 'node:net'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

const servers = []
const sockets = new Set()
let directory
let core
let output = ''
const controllerSecret = 'fixture-controller-secret'
const listenerToken = 'a'.repeat(64)
const listenerAuthorization = `Basic ${Buffer.from(`clash5:${listenerToken}`).toString('base64')}`

async function listen(server, port = 0) {
  servers.push(server)
  server.on('connection', (socket) => {
    sockets.add(socket)
    socket.on('close', () => sockets.delete(socket))
  })
  server.listen(port, '127.0.0.1')
  await once(server, 'listening')
  return server.address().port
}

async function freePort() {
  const server = net.createServer()
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const port = server.address().port
  await new Promise((done) => server.close(done))
  return port
}

async function proxy(allowedPort, stats, port = 0) {
  const server = http.createServer((_, response) =>
    response.writeHead(502).end(),
  )
  server.on('connect', (request, client, head) => {
    stats.push(request.url)
    if (request.url !== `127.0.0.1:${allowedPort}`) {
      client.end('HTTP/1.1 403 Forbidden\r\n\r\n')
      return
    }
    const remote = net.connect(allowedPort, '127.0.0.1')
    sockets.add(remote)
    remote.on('close', () => sockets.delete(remote))
    remote.on('connect', () => {
      client.write('HTTP/1.1 200 Connection Established\r\n\r\n')
      if (head.length) remote.write(head)
      client.pipe(remote).pipe(client)
    })
    remote.on('error', () => client.destroy())
    client.on('error', () => remote.destroy())
    client.on('close', () => remote.destroy())
  })
  return listen(server, port)
}

function viaProbe(port, url) {
  return new Promise((done, reject) => {
    const request = http.get(
      {
        hostname: '127.0.0.1',
        port,
        path: url,
        headers: {
          Host: new URL(url).host,
          'Proxy-Authorization': listenerAuthorization,
        },
        timeout: 10_000,
      },
      (response) => {
        let body = ''
        response.on('data', (chunk) => {
          body += chunk
        })
        response.on('end', () => done({ status: response.statusCode, body }))
      },
    )
    request.on('timeout', () => request.destroy(new Error('probe timed out')))
    request.on('error', reject)
  })
}

function viaConnect(port, target, authorized) {
  return new Promise((done, reject) => {
    const request = http.request({
      hostname: '127.0.0.1',
      port,
      method: 'CONNECT',
      path: target,
      headers: authorized
        ? { 'Proxy-Authorization': listenerAuthorization }
        : {},
      timeout: 10_000,
    })
    request.on('connect', (response, socket) => {
      if (response.statusCode !== 200) {
        socket.destroy()
        done({ status: response.statusCode, body: '' })
        return
      }
      let body = ''
      socket.setTimeout(10_000, () =>
        socket.destroy(new Error('tunnel timeout')),
      )
      socket.on('data', (chunk) => {
        body += chunk
        if (body.includes('203.0.113.27')) {
          socket.destroy()
          done({ status: response.statusCode, body })
        }
      })
      socket.on('error', reject)
      socket.write(
        `GET /ip HTTP/1.1\r\nHost: ${target}\r\nConnection: close\r\n\r\n`,
      )
    })
    request.on('response', (response) =>
      done({ status: response.statusCode, body: '' }),
    )
    request.on('timeout', () => request.destroy(new Error('CONNECT timeout')))
    request.on('error', reject)
    request.end()
  })
}

export async function startChainFixture(fixed = false) {
  const requests = []
  const targetPort = await listen(
    http.createServer((request, response) => {
      requests.push(request.url)
      if (request.url === '/ip') response.writeHead(200).end('203.0.113.27')
      else if (request.url === '/unavailable')
        response.writeHead(503).end('unavailable')
      else response.writeHead(204).end()
    }),
    fixed ? 18090 : 0,
  )
  const frontStats = []
  const landingStats = []
  const landingPort = await proxy(targetPort, landingStats, fixed ? 18091 : 0)
  const frontPort = await proxy(landingPort, frontStats, fixed ? 18092 : 0)
  return {
    targetPort,
    landingPort,
    frontPort,
    requests,
    frontStats,
    landingStats,
    close: async () => {
      for (const socket of sockets) socket.destroy()
      await Promise.all(
        servers.map((server) => new Promise((done) => server.close(done))),
      )
    },
  }
}

async function runSmoke(binary) {
  if (!binary)
    throw new Error('Usage: node scripts/m2-mihomo-smoke.mjs <mihomo-binary>')
  let fixture
  try {
    fixture = await startChainFixture()
    const {
      targetPort,
      landingPort,
      frontPort,
      requests,
      frontStats,
      landingStats,
    } = fixture
    const probePort = await freePort()
    const controllerPort = await freePort()
    directory = await mkdtemp(join(tmpdir(), 'clash5-m2-mihomo-'))
    const yaml = execFileSync(
      'cargo',
      [
        'run',
        '--quiet',
        '--locked',
        '--manifest-path',
        'crates/clash5-data/Cargo.toml',
        '--example',
        'm2-fixture',
        '--',
        ...[frontPort, landingPort, probePort, controllerPort].map(String),
      ],
      { encoding: 'utf8' },
    )
    const config = join(directory, 'config.yaml')
    const group = 'clash5-chain-18dc0b8a-7485-49bc-8a91-f48bdc736846'
    assert(yaml.endsWith(`  proxy: ${group}\n`))
    await writeFile(
      config,
      `${yaml}  users:\n  - username: clash5\n    password: ${listenerToken}\n`,
    )
    core = spawn(resolve(binary), ['-d', directory, '-f', config], {
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    for (const stream of [core.stdout, core.stderr])
      stream.on('data', (chunk) => {
        output += chunk
      })
    const controller = `http://127.0.0.1:${controllerPort}`
    const until = Date.now() + 20_000
    let ready = false
    while (Date.now() < until) {
      if (core.exitCode !== null) throw new Error(`Mihomo exited: ${output}`)
      try {
        const response = await fetch(`${controller}/version`, {
          headers: { Authorization: `Bearer ${controllerSecret}` },
          signal: AbortSignal.timeout(1_000),
        })
        if (response.ok) {
          ready = true
          break
        }
      } catch {}
      await sleep(200)
    }
    assert(ready, `Mihomo did not become ready: ${output}`)
    const ip = await viaProbe(probePort, `http://127.0.0.1:${targetPort}/ip`)
    assert.deepEqual(ip, { status: 200, body: '203.0.113.27' })
    const target = `127.0.0.1:${targetPort}`
    assert.equal((await viaConnect(probePort, target, false)).status, 407)
    const tunnel = await viaConnect(probePort, target, true)
    assert.equal(tunnel.status, 200)
    assert(tunnel.body.includes('203.0.113.27'))
    assert(frontStats.length > 0, 'front proxy was bypassed')
    assert(landingStats.length > 0, 'landing proxy was bypassed')
    assert(frontStats.every((target) => target === `127.0.0.1:${landingPort}`))
    assert(landingStats.every((target) => target === `127.0.0.1:${targetPort}`))
    const failed = await viaProbe(
      probePort,
      `http://127.0.0.1:${targetPort}/unavailable`,
    )
    assert.equal(failed.status, 503)
    const delayUrl = new URL(`${controller}/proxies/${group}/delay`)
    delayUrl.searchParams.set('url', `http://127.0.0.1:${targetPort}/health`)
    delayUrl.searchParams.set('timeout', '5000')
    const delay = await fetch(delayUrl, {
      headers: { Authorization: `Bearer ${controllerSecret}` },
    })
    assert(delay.ok, await delay.text())
    assert(
      requests.includes('/ip') &&
        requests.includes('/unavailable') &&
        requests.includes('/health'),
    )
    process.stdout.write(
      `M2 generated two-hop chain, pinned exit IP and failure response passed (${frontStats.length} front / ${landingStats.length} landing tunnels)\n`,
    )
  } finally {
    if (core && core.exitCode === null) {
      const stopped = once(core, 'close')
      core.kill()
      await stopped
    }
    await fixture?.close()
    if (directory) await rm(directory, { recursive: true, force: true })
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await runSmoke(process.argv[2])
}
