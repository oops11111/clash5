import http from 'node:http'
import net from 'node:net'

const port = Number(process.argv[2] ?? 18080)
const mode = process.argv[3] ?? 'forward'
if (
  !Number.isInteger(port) ||
  port < 1024 ||
  port > 65535 ||
  !['forward', 'limit'].includes(mode)
) {
  throw new Error(
    'Usage: node scripts/m1-probe-fixture.mjs <port> <forward|limit>',
  )
}

const counts = { requests: 0, tunnels: 0, subscriptions: 0, providers: 0 }
let phase = 1
const subscription = () => `proxies:
  - {name: M1-inline, type: http, server: 127.0.0.1, port: ${phase === 1 ? 18080 : 18081}}
proxy-providers:
  M1-remote:
    type: http
    url: http://127.0.0.1:18080/provider.yaml
    path: ./proxy_provider/m1-fixture.yaml
    interval: 3600
    proxy: DIRECT
proxy-groups:
  - {name: M1-selected, type: select, proxies: [M1-inline], use: [M1-remote]}
rules:
  - MATCH,M1-selected
`
const provider = () => `proxies:
  - {name: M1-provider, type: http, server: 127.0.0.1, port: 18082}
`
const reject = (socket, status, extra = '') => {
  socket.end(
    `HTTP/1.1 ${status}\r\nConnection: close\r\n${extra}Content-Length: 0\r\n\r\n`,
  )
}

const server = http.createServer((request, response) => {
  if (request.url === '/__stats') {
    response.setHeader('Content-Type', 'application/json')
    response.end(JSON.stringify({ mode, phase, ...counts }))
    return
  }
  if (request.url === '/__phase/2' && request.method === 'POST') {
    phase = 2
    response.writeHead(204).end()
    return
  }
  if (request.url === '/subscription.yaml' && mode === 'forward') {
    counts.subscriptions += 1
    response.setHeader('Content-Type', 'text/yaml; charset=utf-8')
    response.end(subscription())
    return
  }
  if (request.url === '/provider.yaml' && mode === 'forward') {
    counts.providers += 1
    response.setHeader('Content-Type', 'text/yaml; charset=utf-8')
    response.end(provider())
    return
  }
  counts.requests += 1
  if (mode === 'limit') {
    response.writeHead(429, { 'Retry-After': '1200' })
    response.end()
    return
  }
  let target
  try {
    target = new URL(request.url)
  } catch {
    response.writeHead(400).end()
    return
  }
  if (
    target.protocol !== 'http:' ||
    target.hostname !== 'cp.cloudflare.com' ||
    target.port
  ) {
    response.writeHead(403).end()
    return
  }
  const upstream = http.request(
    target,
    { method: request.method, timeout: 15000 },
    (result) => {
      response.writeHead(result.statusCode, result.headers)
      result.pipe(response)
    },
  )
  upstream.on('error', () => response.destroy())
  request.pipe(upstream)
})

server.on('connect', (request, client, head) => {
  counts.tunnels += 1
  if (mode === 'limit') {
    reject(client, '429 Too Many Requests', 'Retry-After: 1200\r\n')
    return
  }
  if (
    request.url !== 'cp.cloudflare.com:80' &&
    request.url !== 'cp.cloudflare.com:443'
  ) {
    reject(client, '403 Forbidden')
    return
  }
  const upstream = net.connect(
    request.url.endsWith(':80') ? 80 : 443,
    'cp.cloudflare.com',
  )
  upstream.once('connect', () => {
    client.write('HTTP/1.1 200 Connection Established\r\n\r\n')
    if (head.length > 0) upstream.write(head)
    client.pipe(upstream)
    upstream.pipe(client)
  })
  upstream.on('error', () => client.destroy())
  client.on('error', () => upstream.destroy())
})

server.listen(port, '127.0.0.1', () => {
  process.stdout.write(
    `M1 probe fixture ${mode} listening on 127.0.0.1:${port}\n`,
  )
})
