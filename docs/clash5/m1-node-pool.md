# M1 node pool integration

The `/nodes` workspace reads the clash5 SQLite inventory through Tauri. It shows source-scoped identities, duplicate counts, inactive subscription entries and persisted probe results. The compact left list and 320px source table follow the local `src/ui-hifi/src/pages/Nodes.tsx` prototype and [`clash5-ui-design.md`](../clash5-ui-design.md); no sample node, fake latency, plugin count, or conflict badge is inserted into the desktop app. The right pane labels its currently available base fields as source configuration, since full merged effective configuration and provenance are later milestones.

On a remote profile refresh, only `proxies` and `proxy-providers` are replaced from the downloaded YAML. Existing rules, groups and other committed profile settings keep their values. The semantic YAML rewrite does not retain comments in that profile file. A new import still saves the original downloaded profile. The independent user rule, merge and script files remain under their existing profile IDs.

`clash5-data` schema v2 adds non-secret protocol and endpoint fields to the node inventory. Migration keeps all v1 identities, bindings, chains and quality rows. A refresh folds identical stable fingerprints within one provider, chooses the lexicographically first display name, and reports the count; records from different sources remain separate. Credentials are used only to derive the fingerprint and are not stored in SQLite.

The on-demand probe tests uniquely named inline nodes in the active subscription and remote `proxy-providers` from the applied runtime cache. Provider identities are scoped to the active profile and provider name; cache files that are missing or invalid are left untouched. It sends a short HTTP delay test (1.5 seconds) followed by an HTTPS test with a longer handshake window (10 seconds) through Mihomo. Results are classified as healthy, slow or failed and cached per source and stable identity for at least 600 seconds. Consecutive failures extend the retry interval to 20, then 40, then at most 60 minutes; a healthy or slow result resets it to 10 minutes. A batch has at most 16 concurrent nodes. The value is an end-to-end Mihomo URL test; it is not a raw TCP handshake measurement. Unloaded or ambiguous names are reported as unavailable, without guessing a runtime target. The Mihomo delay endpoint does not expose upstream HTTP 429 status to this command, so this is failure-based backoff rather than provider-specific rate-limit handling.

The clipboard action accepts HTTP(S) subscription URLs and standalone proxy URI links supported by the existing URI parser. Standalone links now store their complete proxy definitions in a separate app-owned file, with the proxy payload encrypted by the existing AES-GCM configuration helper. The generated runtime adds these proxies under deterministic names derived from stable IDs; provider refresh cannot overwrite them. After successful runtime validation, the manual identities are committed to SQLite in one transaction. A rejected configuration restores the previous manual file and runtime config. The probe maps manual identities to those runtime names; older identity-only manual records remain unavailable until their links are reimported. Credentials never enter the SQLite inventory. The existing encryption key is stored in the app data directory, so the r2 keyring requirement remains open.

Verification:

```sh
cargo fmt --manifest-path crates/clash5-data/Cargo.toml --check
cargo test --manifest-path crates/clash5-data/Cargo.toml --locked --offline
pnpm typecheck
pnpm lint
pnpm format:check
```
