# Read-only Clash Verge Rev import

The profile import action accepts a Clash Verge Rev data directory containing
`profiles.yaml` and a `profiles/` folder. It imports cached `local` and
`remote` profile snapshots as new local clash5 profiles. A source profile must
reference a regular `.yaml` or `.yml` file directly inside `profiles/` and its
content must be a YAML mapping.

Import does not copy subscription URLs, current-profile selection, proxy
selections, settings, credentials, scripts, merge templates, or other
application state. Profiles with external `proxy-providers` or
`rule-providers` that declare a `path` or `url` are skipped because their data
is not self-contained. Missing, invalid, oversized, and unsupported profile
entries are reported as skipped. Imported snapshots are local profiles and
must be refreshed or edited by the user if they need later changes.

Before import, clash5 asks the user to back up the selected source directory.
The importer hashes every relative entry path and regular-file content in the
entire selected tree with SHA-256 before reading profiles, after reading, and
after import. Symlinks are recorded by path and link target but never followed.
If the source changes while it is being read or imported, the command fails.
The source and clash5 data directories may not overlap. To recover, the user
can remove the newly imported local profiles from clash5; the original source
remains the recovery copy.

The IPC failure family is `C5-IMPORT-001` through `C5-IMPORT-007`: directory
separation/inspection, source parsing, manifest verification, concurrent
source changes, target write failure, and post-import source preservation.
