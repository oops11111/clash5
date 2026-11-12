# P0-07 release gate evidence

This records the current evidence for [issue #70](https://github.com/oops11111/clash5/issues/70).
The complete cross-project release-gate registry remains tracked by P0-17.

| Gate | Backlog | Status | Check | Remaining evidence |
| --- | --- | --- | --- | --- |
| G-09 | P0-07 | partial | `node scripts/check-proxy-recovery.mjs`; Rust proxy recovery tests | Live Windows, Linux and macOS crash/restart acceptance remains outstanding |

The app persists ownership leases before global proxy or PAC changes, compares enabled system endpoints with the lease at startup, and preserves unrelated proxy settings. Sidecar, sing-box and the pinned service watchdog use a three-attempt rolling five-minute policy. The service dependency is pinned to commit `6cc9a8aee1dcc4e40fdd22469a42b31cc4cc77b4` in [PR #2](https://github.com/oops11111/clash5-service-ipc/pull/2), which remains under review.

The 11 isolated proxy lease and recovery tests passed, and the service cleanup-error visibility test passed. The service watchdog default-policy test passed. Its Windows crash-loop integration test could not initialize the test-owner ACL locally (`Access denied`); dependency CI still needs to verify it. The main app's `cargo check --locked` passed. No live OS proxy was changed during these checks.

Live GNOME/KDE acceptance, macOS helper cleanup after app termination and actual Windows process-kill/startup recovery remain unverified.

## P1-10 TUN session suspension

| Backlog | Status | Evidence | Remaining evidence |
| --- | --- | --- | --- |
| [Issue #72](https://github.com/oops11111/clash5/issues/72) | OPEN | Run State suspension, Sidecar runtime config, and service-return handoff regression tests | Current-head CI; no live driver or network mutation is in scope |
