import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'

const capability = fs.readFileSync('src-tauri/capabilities/migrated.json', 'utf8')
const windowsCapability = fs.readFileSync('src-tauri/capabilities/desktop-windows.json', 'utf8')
const sourceRoot = 'src'

const frontendFiles = []
const visit = (directory) => {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name)
    if (entry.isDirectory()) visit(file)
    else if (/\.(tsx?|jsx?)$/.test(entry.name)) frontendFiles.push(file)
  }
}
visit(sourceRoot)

assert.doesNotMatch(capability, /allow-create(?:-webview(?:-window)?)?/)
assert.doesNotMatch(windowsCapability, /create-webview|allow-create/)
assert.equal(
  frontendFiles.some((file) => /navigator\.clipboard/.test(fs.readFileSync(file, 'utf8'))),
  false,
)
assert.match(capability, /clipboard-manager:allow-read-text/)
assert.match(capability, /clipboard-manager:allow-write-text/)

console.log('window and clipboard policy audit passed')
