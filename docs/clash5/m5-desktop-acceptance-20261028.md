# Limited Windows desktop acceptance (2026-10-28)

This record comes from the separate M5 injection branch, tested at `dcd1f775d`.
It does not imply that its WFP implementation is integrated into this branch.

The desktop ran on Windows 10 Enterprise 22H2 x64 without administrator
privileges. TUN was disabled. Windows Defender real-time protection was off,
and Huorong plus other third-party WFP drivers were present. The active service
was the upstream Clash Verge service, not the pinned clash5 service. The tested
build did not contain or install `clash5wfp.sys`.

The run confirmed default-off behavior, sidecar cleanup after forced app
termination, and sidecar startup after relaunch. Injection, TUN coordination,
and WFP cleanup were not verified because the product UI, matching privileged
service, and driver were unavailable. WFP state could not be read without
elevation. Killing the Mihomo sidecar left it stopped for at least 90 seconds.
Issue #24 does not require automatic core restart; cleanup after sidecar failure
could not be tested without an active WFP session. A system reboot was not
attempted. WebView2 did not render the app content, so screenshots do not prove
UI behavior.

This was a failed, limited desktop smoke test, not M5 acceptance. The private
environment report and evidence archive remain with the test operator. These
results do not prove process-tree routing, driver lifecycle, network restoration,
or release readiness.
