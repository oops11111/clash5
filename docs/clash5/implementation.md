# clash5 implementation

Repository: https://github.com/oops11111/clash5. Baseline: upstream `dev` at `27efaf34f457f357bf73daa242df909fef1fa9ee`.

## First slice: M0 foundation

The fork lacks clash5's persistent, source-scoped identity and binding model. `crates/clash5-data` implements that model as an independent Rust library, preserving the GPL-3.0 license and keeping tests independent of Tauri platform dependencies. The earlier local TypeScript prototype remains outside this checkout as a behavioral reference.

The library stores node identities and display names, bindings, chain JSON, a quality-cache schema and aggregate audit events. It does not persist node connection credentials. Manual membership and each subscription provider use distinct composite keys, even for identical stable IDs and binding IDs. A complete provider snapshot updates only that provider. Removed nodes become inactive and their bindings become orphan records; returning identities restore bindings while preserving enabled state. Orphan bindings are excluded from effective queries. Purge supports all orphans or one source. Display-name matching is disabled.

Stable IDs are versioned SHA-256 hashes of a structural JSON tuple containing normalized protocol, server, port and sorted credential pairs. UUID spelling is normalized; passwords retain case and whitespace. Protocol aliases are not inferred. Caller-supplied binding and chain JSON must not contain credentials. No real subscription is used in tests.

The data crate has its own lock file and is excluded from the desktop workspace so its offline tests do not resolve the entire Tauri dependency graph. The Tauri backend now links it through a path dependency. On import, refresh and deletion, the existing profile file is persisted first, then the backend reconciles its inline proxies into SQLite using the profile UID as the provider ID. Proxy-provider-only profiles have no inline snapshot and leave these rows untouched. A failed parse or database write raises an existing update-failure notice; the already committed profile and its runtime update remain authoritative. The next successful refresh reconciles the database. These independent stores are not an atomic file/SQLite transaction. SQLite rollback is not Mihomo hot-reload rollback.

Four initial IPC commands register a manual identity, create or update a binding, list bindings, and purge orphan bindings. Their errors expose structured codes without raw SQLite or credential text. The registration command records identity only; the existing profile configuration remains the source of the actual proxy definition. There is no UI for these commands yet.

## Verification

After `cargo fetch --manifest-path crates/clash5-data/Cargo.toml --locked`, run:

```sh
cargo fmt --manifest-path crates/clash5-data/Cargo.toml --check
cargo clippy --manifest-path crates/clash5-data/Cargo.toml --locked --offline --all-targets -- -D warnings
cargo test --manifest-path crates/clash5-data/Cargo.toml --locked --offline
```

Targeted tests cover rename and credential changes, credential/server normalization, manual/provider isolation, orphan recovery and both enabled states, filtered purge, SQLite reopen and rollback after an actual SQL write failure. The identity snapshot must match the previous TypeScript identity encoding. The dedicated M0 workflow provisions dependencies, then runs the same offline checks. Existing frontend lint/typecheck and desktop CI are retained. Mihomo configuration snapshots require the later configuration generator; this slice only snapshots node identity.

## Upstream synchronization

`origin` is the owner's fork; `upstream` is `https://github.com/clash-verge-rev/clash-verge-rev.git`. Merge `upstream/dev` into clash5 `dev`, resolve conflicts without discarding source isolation or bindings, and run the affected checks. Do not rewrite shared history. Preserve upstream GPL attribution.

M0 completion still requires successful desktop checks and a runnable shell. M1 and subsequent milestones follow only after M0 integration is verified. UI polish, sing-box, injection, Plugin Host, signing/release automation and real Mihomo hot loading are outside this slice.

The owner authorized implementation without additional routine approval. Issues was initially disabled on the fork; it is now enabled and [issue #1](https://github.com/oops11111/clash5/issues/1) tracks M0 integration. The first data-library slice was validated before enabling issue tracking; desktop integration follows that issue.
