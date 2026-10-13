# r2 node identity migration decision

Issue: [#7](https://github.com/oops11111/clash5/issues/7). This document fixes the recovery and migration behavior before changing identity code. The accepted M0 baseline remains valid as `v1`; the M1 draft adds SQLite schema v2 but has not yet merged. The implementation must rebase on the merged schema and must not relabel old IDs as new IDs.

## Key ownership and recovery

- Generate one random 256-bit identity key per clash5 installation and store it in the operating system credential store under the app identifier. Never put the key in SQLite, the app data directory, a diagnostic bundle, or a log. The existing `.encryption_key` used for configuration encryption is a different key and must not be reused for identity HMAC.
- Store only a non-secret key identifier in SQLite: the first 16 bytes of `SHA-256("clash5-identity-key-id-v1" || key)`, hex encoded. An `identity_keys` metadata table records generations, identifiers and the active generation; each node records its generation. On startup, compare the credential-store key for the active generation with this identifier before any identity write.
- If a database contains HMAC identities and the key is missing, locked, or mismatched, enter identity recovery mode. Keep nodes and bindings readable; refuse refresh, reimport, deduplication, automatic reattachment, and identity migration. Never generate a replacement key silently.
- Recovery is an explicit reset. Explain that previous HMAC IDs cannot be reconstructed without the old key. After the user confirms reset, stage a key under a new generation in the credential store, commit that generation as active in SQLite, refresh available sources into new identities, and leave old bindings as orphans with their original IDs and generation. The user must choose a new target for each binding. Do not guess from a display name or a cross-provider endpoint. A cancel leaves the database and key state unchanged.
- A planned rotation while the old key is available may compute old and new IDs from the same source definitions and move exact matches transactionally. Retain the old key until every eligible record has migrated or the user has explicitly accepted unresolved orphans. Key loss uses the explicit reset path, never this automatic rotation path.
- Credential-store writes and SQLite cannot share a transaction. Stage the new generation's key first, verify it can be read back, then commit SQLite metadata and identity moves. A crash before the SQLite commit leaves an unreferenced staged key that can be cleaned up later; a crash after the commit leaves a readable active key. Never overwrite or delete the old generation before the database commit.
- If the credential store is unavailable on a fresh install, report a recoverable setup error and do not create HMAC identities or a fallback key file. Existing `v1` data stays readable.

## `v2` identity bytes

The ID is `v2:` followed by lowercase hex HMAC-SHA256. Its payload is a length-delimited, version-tagged encoding of normalized protocol, server, port, transport, SNI, and a credential fingerprint. Use a fixed field order and reject conflicting aliases rather than silently choosing one. Display name, source/provider ID and plugin state are not part of the ID; source remains a separate SQLite scope.

| Field | Canonical value |
| --- | --- |
| Protocol | Trimmed ASCII lowercase type, using the existing supported-type validation. |
| Server | Existing `v1` IP/IDNA normalization, including IPv6 brackets and removal of one trailing DNS dot. |
| Port | Numeric `u16`, nonzero. |
| Transport | Lowercase Mihomo `network` value. Missing and explicit `tcp` are equivalent for TCP-based protocols; protocol-native transports use the defaults below. A supplied value must be a nonempty ASCII token and pass core configuration validation. |
| SNI | Normalize the single effective top-level `sni`, top-level `servername`, or nested `tls.servername` value as an IP or IDNA DNS name. Missing is empty; conflicting nonempty aliases fail validation. |
| Credential fingerprint | HMAC-SHA256 with a distinct domain tag over sorted, length-delimited credential keys and normalized values. UUID normalization preserves the accepted `v1` behavior. |

Transport defaults for an absent `network` field are `tcp` for `ss`, `ssr`, `trojan`, `vless`, `vmess`, `anytls`, `socks5`, `http`, `ssh`, `shadowtls`; `quic` for `hysteria`, `hysteria2`, `tuic`; and `wireguard` for `wireguard`. An explicit `network` equal to that default is the same identity. For protocol-native transports, an explicit different `network` is invalid. Network-specific options such as a WebSocket path remain configuration of the same node in `v2`; changing the protocol transport type or effective SNI changes identity. The credential field list is the accepted `v1` list, sorted by UTF-8 bytes for `v2` and encoded with an unambiguous length-delimited format. These choices need fixed-key golden fixtures before implementation is merged.

The outer HMAC uses a distinct `clash5-node-v2` domain tag. Store `stable_id_version = 2` and the key generation with each node, and key identifiers in database metadata; no raw credential or credential fingerprint is stored. Changes to any canonical field or its normalization require a new ID version and migration plan.

## Database and source migration

1. Add version, key-generation and normalized transport/SNI columns, key metadata, and pending-rebind records through one SQLite transaction. Advance `PRAGMA user_version` only in that transaction. A failed migration leaves the prior schema and data intact. Do not replace `PRAGMA user_version` with a second schema-version authority.
2. Opening an old database changes only its schema. SQLite has no credentials with which to recompute `v1` IDs. Retain all existing IDs, bindings, quality rows and orphan state until raw definitions are available again.
3. On each source refresh or manual reimport, calculate both the accepted `v1` ID and the new keyed `v2` ID from the same raw definition. An exact `v1` match may be moved to `v2` within that source. Insert the new node, retarget its bindings and quality row, then remove the old node in one transaction; foreign keys must remain enabled. A conflicting existing `v2` row aborts the batch unless its exact identity and scope are proven equal. Unavailable historical definitions remain `v1` orphans for explicit rebinding.
4. Credential rotation produces a new `v2` ID. Search only inactive identities from the same subscription provider whose normalized protocol, server, port, transport and SNI match. Exactly one candidate may transfer bindings automatically. Zero candidates leave a new node with no inherited binding; multiple candidates create a pending choice and leave all old bindings orphaned. Manual nodes and different providers are never automatic candidates.
5. All source diff, identity changes and binding moves commit atomically. Audit records contain event names and counts only. A failed source parse, missing key, collision or database error leaves the previous source snapshot and bindings intact.

## Required evidence before completion

Fixtures must cover exact `v1` to `v2` migration, a removed historical node, key missing/mismatched/locked, explicit reset cancellation, planned key rotation, credential rotation with one and multiple candidates, different providers, manual nodes, same endpoint with different transport or SNI, and migration rollback. Verify that credentials and keys are absent from SQLite and logs. A desktop check must demonstrate the recovery and pending-rebind choices without auto-selecting an ambiguous target.
