// Packs and signs a clash5 WASM plugin as a .c5p package, using the minisign layout the app verifies.
//
//   node scripts/plugin-package.mjs keygen <secret-out> <public-out>
//   node scripts/plugin-package.mjs pack <plugin-dir> <secret-key> <out.c5p> [--timestamp <unix>]
//
// <plugin-dir> holds manifest.template.json (without module_sha256) and module.wasm. The secret key
// is the JSON written by keygen; keep it out of the repository.
import {
  createHash,
  createPrivateKey,
  createPublicKey,
  randomBytes,
  sign,
} from 'node:crypto'
import { chmodSync, existsSync, readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import zlib from 'node:zlib'

const ED25519_PKCS8_PREFIX = Buffer.from(
  '302e020100300506032b657004220420',
  'hex',
)

function privateKeyFrom(seed) {
  return createPrivateKey({
    key: Buffer.concat([ED25519_PKCS8_PREFIX, seed]),
    format: 'der',
    type: 'pkcs8',
  })
}

function publicBytesFrom(privateKey) {
  return createPublicKey(privateKey)
    .export({ format: 'der', type: 'spki' })
    .subarray(-32)
}

function keygen(secretOut, publicOut) {
  const seed = randomBytes(32)
  const keyId = randomBytes(8)
  const publicBytes = publicBytesFrom(privateKeyFrom(seed))
  const keyIdHex = keyId.toString('hex').toUpperCase()
  writeFileSync(
    secretOut,
    `${JSON.stringify({ keyId: keyIdHex, seed: seed.toString('base64') }, null, 2)}\n`,
    { mode: 0o600 },
  )
  chmodSync(secretOut, 0o600)
  const publicLine = Buffer.concat([
    Buffer.from('Ed'),
    keyId,
    publicBytes,
  ]).toString('base64')
  writeFileSync(
    publicOut,
    `untrusted comment: minisign public key ${keyIdHex}\n${publicLine}\n`,
  )
  console.log(`wrote ${secretOut} (keep private) and ${publicOut}`)
}

function loadSecret(file) {
  const parsed = JSON.parse(readFileSync(file, 'utf8'))
  const seed = Buffer.from(parsed.seed, 'base64')
  const keyId = Buffer.from(parsed.keyId, 'hex')
  if (seed.length !== 32 || keyId.length !== 8)
    throw new Error('invalid secret key file')
  const privateKey = privateKeyFrom(seed)
  return { keyId, privateKey, publicBytes: publicBytesFrom(privateKey) }
}

// Signature layout: "ED" + key id + Ed25519 over BLAKE2b-512(message), as minisign writes prehashed files.
function signManifest(manifest, key, timestamp) {
  const digest = createHash('blake2b512').update(manifest).digest()
  const signature = sign(null, digest, key.privateKey)
  const trustedComment = `timestamp:${timestamp}\tfile:manifest.json\tprehashed`
  const globalSignature = sign(
    null,
    Buffer.concat([signature, Buffer.from(trustedComment)]),
    key.privateKey,
  )
  return [
    'untrusted comment: signature from minisign secret key',
    Buffer.concat([Buffer.from('ED'), key.keyId, signature]).toString('base64'),
    `trusted comment: ${trustedComment}`,
    globalSignature.toString('base64'),
    '',
  ].join('\n')
}

// Minimal ZIP writer with stored entries, enough for the four package files.
function zip(entries) {
  const locals = []
  const centrals = []
  let offset = 0
  for (const [name, data] of entries) {
    const nameBuffer = Buffer.from(name)
    const crc = zlib.crc32(data)
    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(20, 4)
    local.writeUInt16LE(0x21, 12)
    local.writeUInt32LE(crc, 14)
    local.writeUInt32LE(data.length, 18)
    local.writeUInt32LE(data.length, 22)
    local.writeUInt16LE(nameBuffer.length, 26)
    const localRecord = Buffer.concat([local, nameBuffer, data])
    const central = Buffer.alloc(46)
    central.writeUInt32LE(0x02014b50, 0)
    central.writeUInt16LE(20, 4)
    central.writeUInt16LE(20, 6)
    central.writeUInt16LE(0x21, 14)
    central.writeUInt32LE(crc, 16)
    central.writeUInt32LE(data.length, 20)
    central.writeUInt32LE(data.length, 24)
    central.writeUInt16LE(nameBuffer.length, 28)
    central.writeUInt32LE(offset, 42)
    centrals.push(Buffer.concat([central, nameBuffer]))
    locals.push(localRecord)
    offset += localRecord.length
  }
  const centralDirectory = Buffer.concat(centrals)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(entries.length, 8)
  end.writeUInt16LE(entries.length, 10)
  end.writeUInt32LE(centralDirectory.length, 12)
  end.writeUInt32LE(offset, 16)
  return Buffer.concat([...locals, centralDirectory, end])
}

function pack(pluginDir, secretPath, out, timestamp) {
  const key = loadSecret(secretPath)
  const template = JSON.parse(
    readFileSync(path.join(pluginDir, 'manifest.template.json'), 'utf8'),
  )
  const module = readFileSync(path.join(pluginDir, 'module.wasm'))
  const manifest = Buffer.from(
    `${JSON.stringify({ ...template, module_sha256: createHash('sha256').update(module).digest('hex') })}\n`,
  )
  const signature = Buffer.from(signManifest(manifest, key, timestamp))
  const publicKeyLine = Buffer.concat([
    Buffer.from('Ed'),
    key.keyId,
    key.publicBytes,
  ]).toString('base64')
  const signerFile = Buffer.from(
    `untrusted comment: minisign public key ${key.keyId.toString('hex').toUpperCase()}\n${publicKeyLine}\n`,
  )
  writeFileSync(
    out,
    zip([
      ['manifest.json', manifest],
      ['module.wasm', module],
      ['manifest.minisig', signature],
      ['signer.pub', signerFile],
    ]),
  )
  console.log(`wrote ${out}`)
}

const [command, ...args] = process.argv.slice(2)
if (command === 'keygen' && args.length === 2) {
  keygen(args[0], args[1])
} else if (command === 'pack' && args.length >= 3) {
  const timestampIndex = args.indexOf('--timestamp')
  const timestamp =
    timestampIndex >= 0
      ? args[timestampIndex + 1]
      : String(Math.floor(Date.now() / 1000))
  if (!existsSync(args[0]))
    throw new Error(`plugin directory not found: ${args[0]}`)
  pack(args[0], args[1], args[2], timestamp)
} else {
  console.error(
    'usage: plugin-package.mjs keygen <secret-out> <public-out> | pack <plugin-dir> <secret> <out.c5p>',
  )
  process.exit(2)
}
