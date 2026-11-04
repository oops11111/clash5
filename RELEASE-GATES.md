# Release gates

This is the release-blocking registry for the [2026-11-04 reviewed backlog](docs/clash5/CLASH5-BACKLOG-ALL-ROUNDS-2026-11-04.md).
An implementation or a static check alone is not a passed gate. Every row must be
`passed`, have an executable check, and link acceptance evidence before RC approval.
`node scripts/check-release-gates.mjs --release` rejects incomplete rows.
The later-round G-17 through G-34 acceptance details must also be reconciled from
the original review records before release; the present registry is a skeleton.

| Gate | Backlog | Status | Check | Remaining evidence |
| --- | --- | --- | --- | --- |
| G-01 | P0-01 | blocked | - | Missing targets reject only their application; route health and UI |
| G-02 | P0-03 | blocked | - | RC-only publication and environment approval |
| G-03 | P0-02 | blocked | - | Profile isolation and retained materialized nodes |
| G-04 | P0-04 | partial | scripts/check-webview-security.mjs | Desktop IPC, Monaco, dialog import, icons and denied out-of-scope reads |
| G-05 | P0-05 | blocked | - | Trusted signer lifecycle and catalog rollback protection |
| G-06 | P0-06 | partial | - | Backup path tests pass locally; native save dialog and denied destination runtime checks remain |
| G-07 | P0-18 | blocked | - | First release excludes browser companion resources |
| G-08 | P0-18 | blocked | - | Frozen driver artifacts and accurate product claims |
| G-09 | P0-07 | partial | - | Sidecar exit cleanup, persisted proxy lease and bounded recovery implemented; OS acceptance and service policy alignment remain |
| G-10 | P0-08 | partial | scripts/check-privacy-defaults.mjs | Rust and frontend default tests pass; new-install snapshot and live DNS routing acceptance remain |
| G-11 | P0-09 | blocked | - | Uninstall proxy ownership and scheduled-task cleanup |
| G-12 | P0-10 | blocked | - | Token-safe subscription naming and migration |
| G-13 | P0-11 | blocked | - | Explicit subscription transport policy and UA |
| G-14 | P0-14 | blocked | - | Pre-migration snapshot, atomic v11 and downgrade rejection |
| G-15 | P0-15 | blocked | - | Corrupt config preservation and write protection |
| G-16 | P0-16 | partial | scripts/check-privacy-defaults.mjs | Updater disabled in config, frontend and backend; signing and RC artifact policy still unverified |
| P0-12 | P0-12 | partial | scripts/check-privacy-defaults.mjs | Upgrade UI, command and downloader removed; packaged-core service hash acceptance remains |
| P0-13 | P0-13 | partial | - | Strict TLS and bounded staged restore implemented; invalid-cert server acceptance, optional encryption and CA/pin UX remain |
| P0-17 | P0-17 | partial | scripts/check-release-gates.mjs | Complete executable gate coverage, evidence and later-round reconciliation |

## Local implementation evidence (2026-11-04)

Issue: https://github.com/oops11111/clash5/issues/68.
`tsc --noEmit`, Vite production build, and offline Rust library `cargo check`
passed. The standalone backup path test binary passed all three Windows tests;
the Unix symlink-escape test is included for CI and has not run locally.
The WebView guard passed on current configuration and rejected isolated fixtures
with null CSP and unrestricted filesystem scope. Registry validation passed;
`--release` failed as intended because acceptance is incomplete.

The Windows native save dialog, out-of-scope IPC rejection, icon loading and
Monaco workers still require desktop acceptance. WebDAV now uses the default
certificate verifier, but invalid-certificate server acceptance has not run.
Restore now uses a private staging directory, an explicit file whitelist,
64 MiB archive/entry limits, a 512 MiB expanded limit and at most 10,000 entries.
Config files and referenced profile paths are checked before publication; SQLite
snapshots require a valid header and integrity check. File replacements are atomic
per file, with rollback on publication/database errors; this is not a crash-atomic
transaction across all config files and OS effects. Failed database restoration
retains recovery snapshots. Eleven Windows archive/database regression tests passed;
the Unix destination-link test is wired into CI but has not run locally.
All 75 data-layer tests passed. Clippy passed for the backend library and the
backup restore integration test with warnings denied.
Optional encryption and custom CA/pin controls remain unimplemented.
Backup creation shares the restore limits and excludes unexpected profile files;
WebDAV restore downloads are streamed into a random temporary filename with the
same archive-size cap, rather than joining the remote name into the app directory.
Core upgrade code was removed and the generated command contract
was regenerated. The frontend test verifies that an empty release channel never
invokes the updater plugin; updater permissions and artifact generation are off.
No installer was built,
no system security settings changed, and no release was published.

## Proxy lifecycle implementation (issue #70)

Sidecar termination now invalidates only its captured readiness generation and
clears the recorded proxy target before scheduling recovery. Persisted leases
cover global proxy and PAC writes and retain cleanup failures. Startup compares
the enabled OS endpoints against that lease; unrelated targets are not cleared,
and mixed ownership fails without an OS write. An unrelated active proxy blocks
automatic recovery/re-apply; explicitly disabling clash5 retires the stale lease
without switching that unrelated proxy off. Each OS apply uses the same frozen
target as the lease. A cancelled manual operation or replacement generation
invalidates the previous recovery request.

Mihomo sidecar recovery and sing-box use the same three-attempt rolling
five-minute policy, with separate supervisor budgets. The pinned service dependency still has its own watchdog
(`max_restarts: 10`, 600-second window); its attempts are not governed by the new
app budget. This policy must be reconciled before G-09 can pass.
Eleven isolated ownership/marker/budget tests passed; no live OS proxy was changed
by those tests. Linux reads HTTP, HTTPS and SOCKS separately rather than using the
merged endpoint getter. Live GNOME/KDE acceptance, macOS helper cleanup after app
termination and actual Windows process-kill/startup recovery remain unverified.

Using the existing `clippy` test feature, all 44 Core manager tests and the
sing-box budget test passed on Windows, along with 11 archive recovery tests.
That feature embeds the Common Controls v6 manifest needed by the library test
harness; the default-feature harness fails before test entry with missing
`TaskDialogIndirect`, which does not establish an unsupported Windows version.
`cargo clippy --all-targets --all-features -- -D warnings` passed locally after
using an unchanged copy of `tauri-plugin-devtools 2.2.2` in the workspace cache:
its build script needs to create/delete permissions in its source directory,
which the global registry cache does not permit here. The temporary Cargo patch
was only for verification; Cargo.lock was restored and no dependency override is
part of the committed implementation. Rust formatting and static security gates
also passed. Remote CI and desktop acceptance are still outstanding.
