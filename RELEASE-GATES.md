# clash5 Release Gates

This file is the authoritative release-gate ledger. `OPEN` means the gate has
not been proven; `BLOCKED` means required environment or credentials are not
available; `PASS` is reserved for a gate with current automated evidence and,
where required, completed manual evidence. A green registry-format check does
not mean the release is ready.

The reviewed scope comes from the
[release-hardening issue](https://github.com/oops11111/clash5/issues/68) and its
linked backlog. Historical gate ranges stay grouped where the source does not
define their individual meanings; this ledger does not guess those meanings.

| Gate | Scope from reviewed backlog | Required evidence | State |
|---|---|---|---|
| G-01 | P0-01: invalid route target degrades to REJECT | Per-target route tests and Windows acceptance | OPEN |
| G-02 | P0-03: only promote a release candidate to a published release | Release-tag and candidate-origin checks | OPEN |
| G-03 | P0-02: cross-profile isolation and materialized nodes | Data-layer isolation tests | OPEN |
| G-04 | P0-04: WebView CSP and filesystem/asset scopes | CI config guard; dialog-flow and local-worker acceptance | OPEN |
| G-05 | P0-05: trusted plugin signer list | Signer trust, tamper, rotation, and rollback tests | OPEN |
| G-06 | P0-06: backup export traversal | Rust tests `export_rejects_traversal` and `export_rejects_absolute` | OPEN |
| G-07 | P0-18: conditional browser-extension naming gate | Conditional scope review recorded in issue #68 | OPEN |
| G-08 | P0-18: takeover wording and frozen driver scope | Forbidden-copy check and product review | OPEN |
| G-09 | P0-07: system proxy cleanup and dirty-exit recovery | Windows proxy lifecycle tests | OPEN |
| G-10 | P0-08: IPv6 and DNS privacy defaults | Default configuration tests | OPEN |
| G-11 | P0-09: uninstall cleanup | Windows install/uninstall acceptance | OPEN |
| G-12 | P0-10: prevent subscription names from exposing tokens | Subscription-name redaction regression tests | OPEN |
| G-13 | P0-11: subscription update channel and User-Agent | Update policy tests and request-header verification | OPEN |
| G-14 | P0-14: migration snapshot, atomicity, and old-version guard | Migration transaction and compatibility tests | OPEN |
| G-15 | P0-15: preserve data when configuration is corrupt | Corrupt-configuration preservation tests | OPEN |
| G-16 | P0-16: signing and first-release updater behavior | Updater-off regression tests; platform signing evidence remains required | BLOCKED |
| P0-12-CORE-UPDATE | P0-12: core updates ship only with clash5 installers | Command-surface check and desktop installer acceptance | OPEN |
| G-17…G-22 | R11 additions covering WebDAV, deep links, User-Agent, and related decisions | Individual R11 acceptance evidence; see P0-13 and P1-36 | OPEN |
| G-23…G-27 | R12 additions | Individual R12 acceptance evidence; see P0-11 and P1-38…P1-40 | OPEN |
| G-28…G-31 | R13 additions | Individual R13 acceptance evidence; see P0-05 and P1-41 | OPEN |
| G-32…G-34 | R14 additions | Individual R14 UI/accessibility acceptance evidence; see P1-42 | OPEN |
| R19-LINUX-KEYSTORE | P1-48: Linux credential storage uses the system keystore | Linux keystore integration test and platform acceptance | OPEN |
| R19-NO-CREDENTIALS | P1-49: manual-node list responses contain no credentials | Response serialization regression test | OPEN |

## CI contract

The Release Candidate Gate workflow validates this registry on pull requests
and before inspecting a draft release. The validator rejects missing,
duplicate, malformed, or reordered gate rows and unknown state values. It does
not promote open or blocked gates to passing; a release is not cleared until
all applicable rows have passing evidence and required manual evidence is
recorded.
