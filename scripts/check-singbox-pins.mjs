import { readFile } from 'node:fs/promises'

const [prebuild, resolver, workflow, smoke] = await Promise.all([
  readFile('scripts/prebuild.mjs', 'utf8'),
  readFile('src-tauri/src/core/singbox_binary.rs', 'utf8'),
  readFile('.github/workflows/clash5-shell-smoke.yml', 'utf8'),
  readFile('scripts/m1-desktop-smoke.mjs', 'utf8'),
])

const version = prebuild.match(/const SINGBOX_VERSION = '([^']+)'/)?.[1]
if (!version) throw new Error('SINGBOX_VERSION must be explicit')
if (!/SINGBOX_RELEASE_BASE/.test(prebuild)) {
  throw new Error('sing-box must use an explicit release base URL')
}

const targets = [
  'win32-x64',
  'win32-ia32',
  'win32-arm64',
  'darwin-x64',
  'darwin-arm64',
  'linux-x64',
  'linux-ia32',
  'linux-arm64',
  'linux-arm',
  'linux-riscv64',
  'linux-loong64',
]
for (const target of targets) {
  const entry = new RegExp(`['"]${target}['"]\\s*:\\s*\\{[\\s\\S]{0,260}sha256:\\s*['"]([0-9a-f]{64})['"]`)
  if (!entry.test(prebuild)) {
    throw new Error(`${target} is missing an explicit sing-box archive SHA-256`)
  }
}

if (/var_os\("PATH"\)|path_dirs|on PATH|PATH \(/.test(resolver)) {
  throw new Error('sing-box resolver still accepts a PATH fallback')
}
if (/Provision pinned sing-box|GITHUB_PATH/.test(workflow)) {
  throw new Error('CI must consume the prebuild sing-box resource, not PATH')
}
if (!/src-tauri['"]?, ['"]resources['"]/.test(smoke)) {
  throw new Error('desktop smoke must resolve the packaged sing-box resource')
}

console.log(`sing-box pin audit passed (${version})`)
