# Minimal clash5 plugin template

This directory is a source template, not an installable package. It contains a
small WAT module and a manifest skeleton that match ABI version 1. The module
exports `memory` and `apply(ptr, len) -> i64`, ignores the invocation, and
returns the JSON patch `{ "tun": { "mtu": 1360 } }`.

## Build

```text
wat2wasm module.wat -o module.wasm
sha256sum module.wasm
```

Replace the placeholder `module_sha256` in `manifest.json` with the digest of
the resulting bytes. Then sign the manifest with a locally protected minisign
key and assemble exactly:

```text
manifest.json
module.wasm
manifest.minisig
signer.pub
```

See [`../plugin-host-abi.md`](../plugin-host-abi.md) for the complete contract
and limits.
