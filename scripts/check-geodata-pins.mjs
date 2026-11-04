import { readFile } from 'node:fs/promises'

const source = await readFile('scripts/prebuild.mjs', 'utf8')

const tagMatch = source.match(/const GEODATA_RELEASE_TAG = '([^']+)'/)
if (!tagMatch?.[1]) {
  throw new Error('GEODATA_RELEASE_TAG must be explicit')
}
if (/meta-rules-dat\/releases\/download\/latest\//.test(source)) {
  throw new Error('geodata URLs must use GEODATA_RELEASE_BASE, not a floating URL')
}

const expected = ['Country.mmdb', 'ASN.mmdb', 'geosite.dat', 'geoip.dat']
for (const file of expected) {
  const escapedFile = file.replace('.', '\\.')
  const entry = new RegExp(`['"]${escapedFile}['"]\\s*:\\s*['"]([0-9a-f]{64})['"]`)
  if (!entry.test(source)) {
    throw new Error(`${file} is missing a 64-character SHA-256 pin`)
  }
  const use = new RegExp(`file: ['"]${escapedFile}['"][\\s\\S]{0,260}sha256: GEODATA_SHA256`)
  if (!use.test(source)) {
    throw new Error(`${file} is not wired to its pinned SHA-256`)
  }
}

console.log(`Geodata pins audit passed (${tagMatch[1]})`)
