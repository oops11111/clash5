import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { access, readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const document = await readFile(path.join(root, 'RELEASE-GATES.md'), 'utf8')
const rows = document
  .split('\n')
  .filter((line) => /^\| (G-\d+|P0-\d+) \|/.test(line))
const gates = new Map()
for (const row of rows) {
  const [id, backlog, status, check, evidence] = row
    .split('|')
    .slice(1, -1)
    .map((cell) => cell.trim())
  assert.ok(!gates.has(id), `Duplicate gate ${id}`)
  assert.match(backlog, /^P0-\d+$/)
  assert.ok(
    ['blocked', 'partial', 'passed'].includes(status),
    `${id}: invalid status`,
  )
  assert.ok(evidence, `${id}: missing evidence`)
  if (check !== '-') {
    assert.match(check, /^scripts\/[a-z0-9/-]+\.mjs$/)
    assert.ok(!check.includes('..'))
    await access(path.join(root, check))
  }
  if (status === 'passed')
    assert.notEqual(check, '-', `${id}: missing executable check`)
  gates.set(id, { status, check })
}
for (let n = 1; n <= 16; n++)
  assert.ok(gates.has(`G-${String(n).padStart(2, '0')}`))
for (const id of ['P0-12', 'P0-13', 'P0-17']) assert.ok(gates.has(id))
const runChecks = () => {
  for (const check of new Set([...gates.values()].map((gate) => gate.check))) {
    if (check === '-' || check === 'scripts/check-release-gates.mjs') continue
    execFileSync(process.execPath, [path.join(root, check)], {
      cwd: root,
      stdio: 'inherit',
    })
  }
}

if (process.argv.includes('--release')) {
  const incomplete = [...gates]
    .filter(([, gate]) => gate.status !== 'passed')
    .map(([id]) => id)
  assert.equal(
    incomplete.length,
    0,
    `Release blocked: ${incomplete.join(', ')}`,
  )
}
runChecks()
console.log(
  `Validated ${gates.size} gate records. Release approval remains blocked.`,
)
