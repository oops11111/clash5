# clash5 plugin Host ABI

This document is the author-facing contract for third-party `.c5p` plugins. The
runtime contract is intentionally small: a plugin receives one JSON invocation,
returns one JSON patch, and has no host imports.

## Package layout

A package is a ZIP archive with exactly these four files:

```text
manifest.json
module.wasm
manifest.minisig
signer.pub
```

`manifest.minisig` signs the exact bytes of `manifest.json`. The
`module_sha256` field is the lowercase SHA-256 digest of the exact
`module.wasm` bytes. The package signer is shown to the user and is persisted
only after explicit consent; a signature does not establish publisher identity.

The manifest schema is:

```json
{
  "schema_version": 1,
  "id": "example.plugin",
  "version": "1.0.0",
  "name": "Example plugin",
  "description": "A short description shown in the plugin UI.",
  "abi_version": 1,
  "permissions": ["config.read", "config.patch"],
  "writable_paths": ["/tun/mtu"],
  "module_sha256": "<64 lowercase hexadecimal characters>"
}
```

The runtime accepts only `config.read` and `config.patch` permissions. Every
plugin must declare `config.patch`; every returned patch path must be covered by
one of the manifest's `writable_paths` prefixes. The `/rules` subtree is never
writable by a plugin.

## WASM exports

The module must export:

```text
memory: linear memory
apply(ptr: i32, len: i32) -> i64
```

The module must not import functions, memories, tables, globals, or any other
host capability. There is no network, filesystem, clock, or environment API.

Before calling `apply`, the Host serializes this UTF-8 JSON value and appends it
after the module's initial linear-memory data:

```json
{
  "configuration": { "...": "..." },
  "binding": { "...": "..." }
}
```

`configuration` is the effective configuration snapshot when `config.read` is
declared; otherwise it is `{}`. `binding` is the user-owned binding object. The
Host passes the appended buffer pointer and byte length as `ptr` and `len`.
Plugins must treat the input as read-only and may return a pointer into any
valid region of their own linear memory.

The `i64` return value packs the output pointer and byte length:

```text
low 32 bits  = pointer to UTF-8 JSON patch
high 32 bits = patch length in bytes
```

The returned bytes must decode to a JSON object. An empty patch is valid:
`{}`. Invalid JSON, an out-of-bounds pointer, a patch larger than 1 MiB, or a
path outside `writable_paths` fails the configuration generation and the
previous configuration remains in effect.

## Limits and failure behavior

The Host enforces these limits per invocation:

| Resource | Limit |
| --- | ---: |
| Enabled bindings in one generation | 32 |
| WASM module | 8 MiB |
| Invocation JSON | 1 MiB |
| Returned patch | 1 MiB |
| Linear memory | 32 MiB, one memory |
| Tables | one table, 1,024 elements |
| Fuel | 10,000,000 units |
| Wall-clock/epoch deadline | 3 seconds |

Fuel exhaustion, epoch interruption, or the wall-clock deadline produces a
timeout. Other traps produce an execution failure. A plugin failure is audited
and aborts that configuration generation; it does not partially apply patches.

Patches are merged after the normal Merge/Script and chain generation stages.
Higher binding priority wins a conflicting path, with binding ID as the stable
tie-breaker. The effective configuration is then passed through the existing
Mihomo validation before it is written or activated.

## Minimal author workflow

The [`plugin-template`](plugin-template/) directory contains a no-dependency
WAT module and manifest skeleton. The module in `module.wat` returns a
`/tun/mtu` patch and deliberately ignores the input, making it suitable for
smoke tests and ABI experiments.

1. Compile WAT to WASM with a tool such as `wat2wasm`.
2. Compute the exact module digest: `sha256sum module.wasm` (or the platform
   equivalent) and copy it into `manifest.json`.
3. Generate a minisign key pair outside the repository.
4. Sign the exact manifest bytes with `minisign -Sm manifest.json`.
5. Put the public key in `signer.pub` and create the ZIP with only the four
   required files.

Never commit the private signing key. The repository fixture under
`scripts/fixtures/` is test-only and its ephemeral key is not a production
trust anchor.
