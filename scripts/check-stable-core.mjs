import { readFile } from 'node:fs/promises'

const read = (file) => readFile(file, 'utf8')

const [tauri, linux, prebuild, installer, workflow] = await Promise.all([
  read('src-tauri/tauri.conf.json').then(JSON.parse),
  read('src-tauri/tauri.linux.conf.json').then(JSON.parse),
  read('scripts/prebuild.mjs'),
  read('src-tauri/packages/windows/installer.nsi'),
  read('.github/workflows/clash5-shell-smoke.yml'),
])

const stableOnly = (values, label) => {
  if (values.some((value) => value.toLowerCase().includes('alpha'))) {
    throw new Error(`${label} contains a prerelease alpha core`)
  }
}

stableOnly(tauri.bundle.externalBin, 'Tauri bundle externalBin')
stableOnly(linux.bundle.externalBin, 'Linux bundle externalBin')
for (const [label, source] of [
  ['prebuild', prebuild],
  ['Windows installer', installer],
  ['desktop smoke workflow', workflow],
]) {
  if (/verge-mihomo-alpha|clash5-mihomo-alpha|MIHOMO_ALPHA|Prerelease-Alpha/.test(source)) {
    throw new Error(`${label} still references the alpha core`)
  }
}

console.log('Stable Mihomo resource audit passed')
