import { readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const changelogPath = path.join(root, 'Changelog.md')
const original = (await readFile(changelogPath, 'utf8')).replace(/\r\n/g, '\n')
const sourceLines = original.replace(/\n$/, '').split('\n')
const order = [
  'Windows',
  'macOS',
  'Linux',
  'Windows/macOS',
  'Windows/Linux',
  'macOS/Linux',
]

function platformOf(line) {
  const platforms = ['Windows', 'macOS', 'Linux'].filter((platform) =>
    line.includes(platform),
  )
  return platforms.length === 1 || platforms.length === 2
    ? platforms.join('/')
    : ''
}

function heading(platforms) {
  return `**${platforms
    .replaceAll('Windows', '🖥️')
    .replaceAll('macOS', '🍎')
    .replaceAll('Linux', '🐧')} ${platforms}**`
}

function regroup(lines) {
  const output = []
  for (let index = 0; index < lines.length; ) {
    if (lines[index] !== '<details>') {
      output.push(lines[index++])
      continue
    }

    const start = index
    let end = index
    while (end < lines.length && lines[end] !== '</details>') end++
    if (end === lines.length) {
      output.push(...lines.slice(start))
      break
    }

    output.push(lines[start])
    let contentStart = start + 1
    if (lines[contentStart]?.startsWith('<summary>')) {
      output.push(lines[contentStart++])
    }
    output.push('')

    const groups = new Map()
    for (const line of lines.slice(contentStart, end)) {
      if (!line.startsWith('- ')) continue
      const platform = platformOf(line)
      const entries = groups.get(platform) ?? []
      entries.push(line)
      groups.set(platform, entries)
    }

    output.push(...(groups.get('') ?? []))
    for (const platform of order) {
      const entries = groups.get(platform)
      if (!entries?.length) continue
      output.push('', heading(platform), '', ...entries)
    }
    output.push('', lines[end])
    index = end + 1
  }
  return `${output.join('\n')}\n`
}

const result = regroup(sourceLines)
if (process.argv.includes('--check')) {
  if (result !== original) {
    process.stderr.write(
      'Misplaced platform entries detected in Changelog.md\n',
    )
    process.exitCode = 1
  } else {
    process.stdout.write('OK: platform grouping is up to date\n')
  }
} else {
  const countEntries = (text) => (text.match(/^- /gm) ?? []).length
  if (countEntries(result) !== countEntries(original)) {
    throw new Error('Regrouping would drop Changelog entries')
  }
  await writeFile(changelogPath, result)
  process.stdout.write('Regrouped platform entries in Changelog.md\n')
}
