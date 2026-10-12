# M1 node pool integration

The `/nodes` workspace reads the clash5 SQLite inventory through Tauri. It shows source-scoped identities, duplicate counts, inactive subscription entries and persisted probe results. The compact left list and 320px source table follow the local `src/ui-hifi/src/pages/Nodes.tsx` prototype and [`clash5-ui-design.md`](../clash5-ui-design.md); no sample node, fake latency, plugin count, or conflict badge is inserted into the desktop app. The right pane labels its currently available base fields as source configuration, since full merged effective configuration and provenance are later milestones.

On a remote profile refresh, only `proxies` and `proxy-providers` are replaced from the downloaded YAML. Existing rules, groups and other committed profile settings keep their values. The semantic YAML rewrite does not retain comments in that profile file. A new import still saves the original downloaded profile. The independent user rule, merge and script files remain under their existing profile IDs.

`clash5-data` schema v2 adds non-secret protocol and endpoint fields to the node inventory. Migration keeps all v1 identities, bindings, chains and quality rows. A refresh folds identical stable fingerprints within one provider, chooses the lexicographically first display name, and reports the count; records from different sources remain separate. Credentials are used only to derive the fingerprint and are not stored in SQLite.

The on-demand probe tests only a uniquely named inline node in the active subscription. It sends a short HTTP delay test (1.5 seconds) followed by an HTTPS test with a longer handshake window (10 seconds) through Mihomo. Results are classified as healthy, slow or failed and cached per source and stable identity for 600 seconds. A batch has at most 16 concurrent nodes. The value is an end-to-end Mihomo URL test; it is not a raw TCP handshake measurement. Unloaded or ambiguous names are reported as unavailable, without guessing a runtime target.

The clipboard action currently accepts HTTP(S) subscription URLs and uses the existing import command. Standalone proxy links, external `proxy-providers` maintained solely by Mihomo, and manual-node runtime injection still need their own identity-to-runtime mapping before they can be shown or probed safely. They remain open M1 work.

Verification:

```sh
cargo fmt --manifest-path crates/clash5-data/Cargo.toml --check
cargo test --manifest-path crates/clash5-data/Cargo.toml --locked --offline
pnpm typecheck
pnpm lint
pnpm format:check
```
