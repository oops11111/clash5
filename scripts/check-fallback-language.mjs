import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url))
const rootDirectory = path.resolve(scriptDirectory, '..')
const reportPath = path.join(
  os.tmpdir(),
  `clash5-i18n-fallback-${process.pid}.json`,
)

try {
  const result = spawnSync(
    process.execPath,
    [
      path.join(scriptDirectory, 'cleanup-unused-i18n.mjs'),
      '--baseline',
      'en',
      '--report',
      reportPath,
    ],
    { cwd: rootDirectory, encoding: 'utf8' },
  )

  process.stdout.write(result.stdout ?? '')
  process.stderr.write(result.stderr ?? '')
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`i18n inventory failed with exit code ${result.status}`)
  }

  const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'))
  const frontend = report.results.filter(
    (result) => result.group === 'frontend',
  )
  const english = frontend.find((result) => result.locale === 'en')
  if (!english)
    throw new Error('English frontend fallback locale was not checked')

  const missing = english.missingSourceKeys ?? []
  if (missing.length) {
    throw new Error(
      `English fallback is missing ${missing.length} frontend key(s): ${missing.join(', ')}`,
    )
  }

  console.log(
    'English fallback covers every referenced frontend translation key.',
  )
} catch (error) {
  console.error(error instanceof Error ? error.message : error)
  process.exitCode = 1
} finally {
  fs.rmSync(reportPath, { force: true })
}
