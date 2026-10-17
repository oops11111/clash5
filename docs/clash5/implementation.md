# clash5 implementation

Repository: https://github.com/oops11111/clash5. Baseline: upstream `dev` at `27efaf34f457f357bf73daa242df909fef1fa9ee`.

The package display name and Tauri identifier are `clash5` and `io.github.oops11111.clash5`. All desktop platform overrides and the application data resolver use that identifier, so this fork keeps its SQLite database and profiles away from Clash Verge Rev's data directory. Existing upstream attribution remains in README and the GPL license is retained.

The updater endpoint list is empty and automatic checks default to off until clash5 has its own signing key and release metadata. This prevents a development build from offering an upstream Clash Verge installer or repeatedly checking an unavailable endpoint. The original upstream public key is inert without endpoints and must be replaced when clash5 release signing is configured.

Desktop deep links register the `clash5` scheme, so installing this fork does not take over Clash Verge Rev's `clash` and `clash-verge` links. Linux package metadata and service binary names are still inherited from upstream and must be isolated before distributing installers.

## First slice: M0 foundation

The fork lacks clash5's persistent, source-scoped identity and binding model. `crates/clash5-data` implements that model as an independent Rust library, preserving the GPL-3.0 license and keeping tests independent of Tauri platform dependencies. The earlier local TypeScript prototype remains outside this checkout as a behavioral reference.

The library stores node identities and display names, bindings, chain JSON, a quality-cache schema and aggregate audit events. It does not persist node connection credentials. Manual membership and each subscription provider use distinct composite keys, even for identical stable IDs and binding IDs. A complete provider snapshot updates only that provider. Removed nodes become inactive and their bindings become orphan records; returning identities restore bindings while preserving enabled state. Orphan bindings are excluded from effective queries. SQLite uses WAL and `PRAGMA user_version` migrations through schema v5, unifying the M2 identity-retention tables with M3 plugin bindings. Existing M3 schema-v3 and M2 schema-v4 databases upgrade without discarding data. Orphan retention defaults to 30 days and is configurable from 1 to 365 days; the preview API counts retained and expired records, and explicit cleanup only deletes records past the configured age. Display-name matching is disabled.

The accepted `v1` IDs preserve the original versioned SHA-256 encoding. New IDs use installation-scoped HMAC-SHA256 (`v2`) over normalized protocol, server, port, transport, SNI and a separately keyed credential fingerprint. UUID spelling is normalized; passwords retain case and whitespace. Key material lives in the OS credential store, while SQLite stores only a key ID and generation. Caller-supplied binding and chain JSON must not contain credentials. Tests use synthetic subscriptions only.

The data crate has its own lock file and is excluded from the desktop workspace so its offline tests do not resolve the entire Tauri dependency graph. The Tauri backend now links it through a path dependency. On import, refresh and deletion, the existing profile file is persisted first, then the backend reconciles its inline proxies into SQLite using the profile UID as the provider ID. Proxy-provider-only profiles have no inline snapshot and leave these rows untouched. A failed parse or database write raises an existing update-failure notice; the already committed profile and its runtime update remain authoritative. The next successful refresh reconciles the database. These independent stores are not an atomic file/SQLite transaction. SQLite rollback is not Mihomo hot-reload rollback.

IPC operations register manual identities, create or update a binding, list and rebind orphans, preview orphan cleanup, configure retention, and purge expired orphan bindings. The Plugins → Binding view shows read-only recovery mode, explicit key reset, planned key rotation, pending credential-rebind choices, orphan targets, retention settings and a confirmation before permanent cleanup. Errors expose structured codes without raw SQLite or credential text. Identity recovery blocks mutations while leaving stored nodes and bindings readable.

IPC source values use `{ "kind": "manual" }` or `{ "kind": "subscription", "provider_id": "<profile UID>" }`. Binding input omits `orphan` and `disappeared_at`; those fields are derived by the store and appear in listings.

## Verification

After `cargo fetch --manifest-path crates/clash5-data/Cargo.toml --locked`, run:

```sh
cargo fmt --manifest-path crates/clash5-data/Cargo.toml --check
cargo clippy --manifest-path crates/clash5-data/Cargo.toml --locked --offline --all-targets -- -D warnings
cargo test --manifest-path crates/clash5-data/Cargo.toml --locked --offline
```

Targeted tests cover rename and credential changes, credential/server normalization, manual/provider isolation, orphan recovery and both enabled states, filtered purge, SQLite reopen, WAL mode, schema migration rollback, retention bounds and expired-only cleanup after an actual SQL write failure. The identity snapshot must match the previous TypeScript identity encoding. The dedicated M0 workflow provisions dependencies, then runs the same offline checks. Existing frontend lint/typecheck and desktop CI are retained. Mihomo configuration snapshots require the later configuration generator; this slice only snapshots node identity.

The Windows shell smoke workflow provisions real upstream desktop resources, builds the frontend and desktop executable, then confirms the process stays alive for 15 seconds. Its scheduled run uploads a test shell artifact. It does not publish or bundle an installer while clash5's service names and release signing remain unfinished.

## Upstream synchronization

`origin` is the owner's fork; `upstream` is `https://github.com/clash-verge-rev/clash-verge-rev.git`. M0 started from an upstream `dev` commit. The r2 engineering manual proposes following upstream release tags for future synchronization through separate merge PRs; the first such PR must reconcile the existing `dev` baseline with the chosen tag. Do not rewrite shared history. Preserve upstream GPL attribution.

M0 passed its original acceptance and was merged into `dev`; M1 node inventory and the nine-entry clash5 navigation were subsequently merged as PRs #4 and #16. The r2 follow-up adds HMAC identity migration, credential-rotation reattachment, planned key rotation, read-only recovery, explicit orphan rebinding, and configurable orphan retention/cleanup. PR #23 was merged after its CI and Windows desktop key-recovery/rotation acceptance. M4 sing-box work (PR #22) and M5 process-proxy work (PR #29) remain drafts; PR #29 currently records an architecture proposal and test-only process-tree model, not working traffic interception. UI polish, Plugin Host, signing/release automation and real Mihomo hot loading remain outside this foundation slice.

The owner authorized implementation without additional routine approval. Issues is enabled; [issue #1](https://github.com/oops11111/clash5/issues/1) tracks the original M0 integration and [issue #7](https://github.com/oops11111/clash5/issues/7) tracks the r2 identity migration and recovery requirements.
