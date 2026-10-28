// Writes module.wasm for the IPv6 and DNS guard preset. Equivalent WAT:
//
//   (module
//     (memory (export "memory") 1)
//     (data (i32.const 128) "{\"ipv6\":false,\"dns\":{\"ipv6\":false}}")
//     (func (export "apply") (param i32 i32) (result i64)
//       (i64.const <len << 32 | 128>)))
//
// Run from the repository root: node plugins/presets/ipv6-dns-guard/build-module.mjs
import { writeFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const PATCH = '{"ipv6":false,"dns":{"ipv6":false}}'
const PATCH_OFFSET = 128

function unsignedLeb(value) {
  const bytes = []
  let rest = BigInt(value)
  do {
    let byte = Number(rest & 0x7fn)
    rest >>= 7n
    if (rest !== 0n) byte |= 0x80
    bytes.push(byte)
  } while (rest !== 0n)
  return bytes
}

function signedLeb(value) {
  const bytes = []
  let rest = BigInt(value)
  for (;;) {
    const byte = Number(rest & 0x7fn)
    rest >>= 7n
    const done =
      (rest === 0n && (byte & 0x40) === 0) ||
      (rest === -1n && (byte & 0x40) !== 0)
    bytes.push(done ? byte : byte | 0x80)
    if (done) return bytes
  }
}

function section(id, content) {
  return [id, ...unsignedLeb(content.length), ...content]
}

function name(text) {
  const bytes = Buffer.from(text)
  return [...unsignedLeb(bytes.length), ...bytes]
}

const patchBytes = Buffer.from(PATCH)
const returned = (BigInt(patchBytes.length) << 32n) | BigInt(PATCH_OFFSET)

const types = section(1, [1, 0x60, 2, 0x7f, 0x7f, 1, 0x7e])
const functions = section(3, [1, 0])
const memory = section(5, [1, 0, 1])
const exports = section(7, [2, ...name('memory'), 2, 0, ...name('apply'), 0, 0])
const body = [0, 0x42, ...signedLeb(returned), 0x0b]
const code = section(10, [1, ...unsignedLeb(body.length), ...body])
const data = section(11, [
  1,
  0,
  0x41,
  ...signedLeb(PATCH_OFFSET),
  0x0b,
  ...unsignedLeb(patchBytes.length),
  ...patchBytes,
])

const module = Buffer.from([
  0x00,
  0x61,
  0x73,
  0x6d,
  0x01,
  0x00,
  0x00,
  0x00,
  ...types,
  ...functions,
  ...memory,
  ...exports,
  ...code,
  ...data,
])
const directory = path.dirname(fileURLToPath(import.meta.url))
writeFileSync(path.join(directory, 'module.wasm'), module)
console.log(`wrote module.wasm (${module.length} bytes)`)
