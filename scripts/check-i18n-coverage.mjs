import { readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

/**
 * Guards translation coverage for the newest namespaces (P0-8).
 *
 *   node scripts/check-i18n-coverage.mjs [--write-baseline]
 *
 * For each of chains, nodePool, settings and layout, every non-English
 * locale must not have more values identical to English than the committed
 * baseline in scripts/i18n-coverage-baseline.json, and must have no missing
 * keys. Language-neutral values (no lowercase letters, e.g. "IP", "KB")
 * do not count as untranslated. A locale not present in the baseline must
 * stay under 10% identical values.
 *
 * The baseline records the state at the time the gate was introduced: most
 * locales are far from translated for these namespaces. Translating a
 * namespace means updating the baseline down with --write-baseline after
 * verifying the new counts; the gate only ever allows the numbers to fall.
 * Removing a language from the settings picker is a separate product
 * decision and is not done here.
 */

const ROOT = path.resolve(import.meta.dirname, '..')
const LOCALES_DIR = path.join(ROOT, 'src/locales')
const BASELINE_PATH = path.join(ROOT, 'scripts/i18n-coverage-baseline.json')
const NAMESPACES = ['chains', 'nodePool', 'settings', 'layout']
const NEW_LOCALE_THRESHOLD = 0.1

const isNeutral = (value) => !/[a-z]/.test(value)

const flatten = (data, prefix = '', out = {}) => {
  for (const [key, value] of Object.entries(data)) {
    const pathKey = prefix ? `${prefix}.${key}` : key
    if (value && typeof value === 'object') flatten(value, pathKey, out)
    else if (typeof value === 'string') out[pathKey] = value
  }
  return out
}

const readLocale = async (lang, namespace) =>
  flatten(
    JSON.parse(
      await readFile(path.join(LOCALES_DIR, lang, `${namespace}.json`), 'utf8'),
    ),
  )

const failures = []
const baseline = {}
for (const namespace of NAMESPACES) {
  const base = await readLocale('en', namespace)
  const total = Object.keys(base).length
  baseline[namespace] = {}
  for (const lang of [
    'ar',
    'de',
    'es',
    'fa',
    'id',
    'jp',
    'ko',
    'pt',
    'ru',
    'tr',
    'tt',
    'zh',
    'zhtw',
  ]) {
    const locale = await readLocale(lang, namespace)
    let identical = 0
    let missing = 0
    for (const [key, value] of Object.entries(base)) {
      if (!(key in locale)) missing += 1
      else if (locale[key] === value && !isNeutral(value)) identical += 1
    }
    baseline[namespace][lang] = { identical, missing, total }
    if (missing > 0) {
      failures.push(`${namespace}/${lang}: ${missing} missing keys`)
    }
  }
}

if (process.argv.includes('--write-baseline')) {
  await writeFile(BASELINE_PATH, `${JSON.stringify(baseline, null, 2)}\n`)
  console.log('Baseline written to scripts/i18n-coverage-baseline.json.')
} else {
  const committed = JSON.parse(await readFile(BASELINE_PATH, 'utf8'))
  for (const namespace of NAMESPACES) {
    for (const [lang, counts] of Object.entries(baseline[namespace])) {
      const previous = committed[namespace]?.[lang]
      if (!previous) {
        if (counts.identical / counts.total > NEW_LOCALE_THRESHOLD) {
          failures.push(
            `${namespace}/${lang}: ${counts.identical}/${counts.total} values identical to English (new locale, limit 10%)`,
          )
        }
      } else if (counts.identical > previous.identical) {
        failures.push(
          `${namespace}/${lang}: identical-to-English rose ${previous.identical} -> ${counts.identical}`,
        )
      }
    }
  }
}

if (failures.length > 0) {
  failures.forEach((message) => {
    console.error(`error: ${message}`)
  })
  process.exit(1)
}
console.log('i18n coverage guard passed.')
