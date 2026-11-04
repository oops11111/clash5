// Warn when a locale is missing keys from the English baseline.
// Existing locale packs are intentionally allowed to lag; i18next uses English
// as the runtime fallback while translators catch up.
import { promises as fs } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const localeRoot = path.join(root, 'src', 'locales')

const isObject = (value) =>
  value !== null && typeof value === 'object' && !Array.isArray(value)

const flatten = (value, prefix = '') =>
  Object.entries(value).flatMap(([key, child]) => {
    const next = prefix ? `${prefix}.${key}` : key
    return isObject(child) ? flatten(child, next) : [next]
  })

const readLocale = async (language, file) =>
  JSON.parse(await fs.readFile(path.join(localeRoot, language, file), 'utf8'))

const files = (await fs.readdir(path.join(localeRoot, 'en')))
  .filter((file) => file.endsWith('.json'))
  .sort()
const baseline = new Map()
for (const file of files) baseline.set(file, new Set(flatten(await readLocale('en', file))))

const languages = (await fs.readdir(localeRoot, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .filter((language) => language !== 'en')
  .sort()

let warningCount = 0
for (const language of languages) {
  const missing = []
  for (const [file, keys] of baseline) {
    const localeFile = path.join(localeRoot, language, file)
    try {
      const actual = new Set(flatten(JSON.parse(await fs.readFile(localeFile, 'utf8'))))
      for (const key of keys) if (!actual.has(key)) missing.push(`${file}:${key}`)
    } catch (error) {
      missing.push(`${file} (${error instanceof Error ? error.message : String(error)})`)
    }
  }

  if (missing.length) {
    warningCount += missing.length
    console.warn(
      `::warning file=src/locales/${language}::${language} is missing ${missing.length} English translation key(s); runtime fallback is en`,
    )
  } else {
    console.log(`ok   ${language} matches the English translation key set`)
  }
}

console.log(`Checked ${languages.length + 1} locale packs; ${warningCount} missing-key warning(s).`)
