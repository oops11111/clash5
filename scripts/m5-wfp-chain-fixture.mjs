import assert from 'node:assert/strict'
import { execFileSync, spawn } from 'node:child_process'
import { once } from 'node:events'
import { existsSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import net from 'node:net'
import { join, resolve } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import { startChainFixture } from './m2-mihomo-smoke.mjs'

if (process.platform !== 'win32' || process.argv.length !== 4) {
  throw new Error(
    'Usage on Windows: node scripts/m5-wfp-chain-fixture.mjs <mihomo-binary> <scratch-directory>',
  )
}

const repo = resolve(fileURLToPath(new URL('..', import.meta.url)))
const binary = resolve(process.argv[2])
const scratch = resolve(process.argv[3])
const readyPath = join(scratch, 'm5-wfp-chain-ready.json')
const donePath = join(scratch, 'm5-wfp-chain-done.txt')
const resultPath = join(scratch, 'm5-wfp-chain-result.txt')
const token = 'a'.repeat(64)
const group = 'clash5-chain-18dc0b8a-7485-49bc-8a91-f48bdc736846'

async function freePort() {
  const server = net.createServer().listen(0, '127.0.0.1')
  await once(server, 'listening')
  const port = server.address().port
  await new Promise((done) => server.close(done))
  return port
}

for (const path of [readyPath, donePath, resultPath]) {
  if (existsSync(path)) await rm(path)
}

let fixture
let core
let directory
let output = ''
try {
  fixture = await startChainFixture()
  const proxyPort = await freePort()
  const controllerPort = await freePort()
  directory = await mkdtemp(join(scratch, 'm5-wfp-chain-'))
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
      ...[
        fixture.frontPort,
        fixture.landingPort,
        proxyPort,
        controllerPort,
      ].map(String),
    ],
    { cwd: repo, encoding: 'utf8' },
  )
  assert(
    yaml.endsWith(`  proxy: ${group}\n`),
    'listener is not pinned to the selected Chain',
  )
  const config = join(directory, 'config.yaml')
  await writeFile(
    config,
    `${yaml}  users:\n  - username: clash5\n    password: ${token}\n`,
  )
  core = spawn(binary, ['-d', directory, '-f', config], {
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  for (const stream of [core.stdout, core.stderr])
    stream.on('data', (chunk) => {
      output += chunk
    })
  let ready = false
  for (let i = 0; i < 100; i += 1) {
    if (core.exitCode !== null) throw new Error(`Mihomo exited: ${output}`)
    try {
      const response = await fetch(
        `http://127.0.0.1:${controllerPort}/version`,
        {
          headers: { Authorization: 'Bearer fixture-controller-secret' },
          signal: AbortSignal.timeout(1_000),
        },
      )
      if (response.ok) {
        ready = true
        break
      }
    } catch {}
    await sleep(200)
  }
  assert(ready, `Mihomo did not become ready: ${output}`)
  await writeFile(
    readyPath,
    JSON.stringify({ proxyPort, targetPort: fixture.targetPort, token }),
  )
  process.stdout.write(`fixture_ready=${proxyPort}/${fixture.targetPort}\n`)
  const deadline = Date.now() + 90_000
  while (!existsSync(donePath) && Date.now() < deadline) {
    if (core.exitCode !== null) throw new Error(`Mihomo exited: ${output}`)
    await sleep(100)
  }
  assert(existsSync(donePath), 'WFP probe did not finish')
  assert(fixture.frontStats.length > 0, 'the front proxy was bypassed')
  assert(fixture.landingStats.length > 0, 'the landing proxy was bypassed')
  assert(
    fixture.frontStats.every(
      (target) => target === `127.0.0.1:${fixture.landingPort}`,
    ),
  )
  assert(
    fixture.landingStats.every(
      (target) => target === `127.0.0.1:${fixture.targetPort}`,
    ),
  )
  assert(
    fixture.requests.includes('/ip'),
    'the target did not receive the expected request',
  )
  await writeFile(
    resultPath,
    `pass front=${fixture.frontStats.length} landing=${fixture.landingStats.length}\n`,
  )
  process.stdout.write(
    'WFP target traversed the selected two-hop Mihomo Chain.\n',
  )
} catch (error) {
  await writeFile(resultPath, `failure=${error.stack ?? error}\n`)
  throw error
} finally {
  if (core && core.exitCode === null) {
    const stopped = once(core, 'close')
    core.kill()
    await stopped
  }
  await fixture?.close()
  if (directory) {
    assert(
      directory.startsWith(scratch + '\\'),
      'temporary cleanup escaped the scratch directory',
    )
    await rm(directory, { recursive: true, force: true })
  }
}
