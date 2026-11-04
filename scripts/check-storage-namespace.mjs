import assert from 'node:assert/strict'
import { readdir, readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sourceRoot = path.join(root, 'src')
const sourceFiles = []
async function collect(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const full = path.join(directory, entry.name)
    if (entry.isDirectory()) await collect(full)
    else if (/\.(ts|tsx)$/.test(entry.name)) sourceFiles.push(full)
  }
}
await collect(sourceRoot)
for (const file of sourceFiles) {
  if (
    file.endsWith(path.join('services', 'storage.ts')) ||
    file.endsWith('.test.ts')
  )
    continue
  const text = await readFile(file, 'utf8')
  assert.ok(
    !/localStorage/.test(text),
    `${path.relative(root, file)} must use appStorage instead of direct browser storage`,
  )
}

for (const relative of [
  'scripts/m1-desktop-smoke.mjs',
  'src-tauri/src/utils/resolve/window_script.rs',
]) {
  const text = await readFile(path.join(root, relative), 'utf8')
  assert.match(text, /clash5:v1:/)
}

const storage = await readFile(
  path.join(sourceRoot, 'services/storage.ts'),
  'utf8',
)
assert.match(storage, /STORAGE_PREFIX = ['"]clash5:v1:/)
assert.match(storage, /window\.localStorage/)
console.log(
  'Application browser storage is routed through the clash5:v1 namespace.',
)
