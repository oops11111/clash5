# M5 Windows process proxy architecture proposal

Status: proposal; requires the Opus-class design review specified by Issue #24 and Windows driver distribution acceptance.

## Decision

Use Windows Filtering Platform (WFP) ALE connect redirection with a signed kernel callout and a user-mode redirect relay. Do not inject DLLs or hooks into target processes. WFP's connect-redirect layer can redirect TCP and UDP connect requests to a local proxy, but local redirection requires a callout driver, a redirect handle, redirect-loop checks, and a target PID for the local service. The relay must retrieve the original destination and opaque redirect records. Windows expects a redirect record on the proxy socket that connects toward the final destination for cooperation with other WFP redirectors. Because this design delegates that socket to Mihomo, record propagation is unresolved and external redirector coexistence is not supported by the proposal yet.

The relay is a required protocol adapter between WFP and clash5's selected Chain. It accepts redirected sockets, extracts the original destination and redirect context, then forwards the flow to a session-authenticated Mihomo listener pinned to the selected generated chain group. A direct redirect to Mihomo's global SOCKS port is insufficient because it does not pin a flow to the selected Chain. clash5 already creates temporary authenticated, chain-pinned HTTP listeners for exit-IP checks; the process-proxy session can build on that runtime-only pattern. TCP can use HTTP CONNECT to a pinned HTTP listener. UDP needs a pinned SOCKS listener with UDP enabled and its own WFP redirect-record handling. Neither transport is supported until its redirect context, chain selection, and teardown are verified end to end.

## Target identity and process tree

Do not use executable path alone as the active target selector: it can capture unrelated instances of the same executable. The user-mode service should own an immutable target identity containing PID, process creation time, canonical executable path, and file identity. Capture file identity from `FILE_ID_INFO`: its volume serial is 64-bit and its file ID is 128-bit. Treat a failed identity query as an unavailable target, rather than substituting a path or narrowing the ID to the legacy 64-bit file index. Parent links must include the parent's creation time, not only its PID. Accept a child-start event only while that exact parent instance is active, belongs to the selected tree, and was created no later than the child; after a parent exits, retain already tracked descendants but reject later events attributed to the exited instance. A stale exit or child event must never remove or extend a reused PID. The service maintains the selected root and descendants, and sends the current PID/creation-time set to the callout. The callout consults process-ID metadata at ALE connect classification and redirects only active identities. A child starting between initial enumeration and watcher registration, and service restart, must fail closed for the target session until the process tree has been reconciled.

The initial supported target contract remains an executable process tree, included by default. Supporting arbitrary child processes requires a documented and tested process notification source; WFP's ALE app identity condition is a normalized executable path, not a per-instance process-tree identity. UWP/AppContainer targets need separate validation before being advertised.

## TUN coordination

The KernelScheduler remains the single owner of TUN and injection coordination. Before installing redirect filters, it must prepare an exclusion for the relay and proxy core and verify that the TUN configuration actually applies it. If the active TUN backend cannot express the exclusion, injection cannot start while TUN is active. The exclusion must be part of the same attach transaction as the WFP filters; detach removes only clash5-owned filters and restores the prior TUN setting.

The current Mihomo TUN schema documents UID inclusion/exclusion as Linux-only and does not document a Windows process-exclusion setting. A `PROCESS-NAME` routing rule is not proof that the Windows TUN backend excludes a process from capture. Until a tested mechanism prevents proxy-core re-entry, attach must fail closed while TUN is active; do not silently disable TUN or claim N1 coexistence.

## Chain adapter and redirect-record boundary

The chain-pinned listener must be created in runtime configuration with a random session credential, bound to loopback, and removed on every detach or failed attach. Its `proxy` field must name the selected generated chain group. This reuses the existing `exit_ip::probe_listener` pattern without persisting session credentials or listeners.

WFP redirect records are opaque and normally need to be transferred to the socket that the proxy service opens toward the final destination. In this design Mihomo owns that final socket, so clash5's relay cannot itself attach the record to it. Coexistence with other WFP redirectors therefore remains unproven; the relay and Mihomo must be excluded from clash5's own target filters, and external redirector coexistence must not be claimed until tested on Windows.

Connected UDP is a separate compatibility gate. Microsoft documents a failure mode when a redirected local proxy uses connected UDP with redirect records; the documented workaround uses connectionless `sendto`. The relay must either implement and test that path or report UDP as unsupported while preserving TCP functionality.

## Lifecycle and failure policy

Injection stays off after install and after every restart. Attach requires explicit user confirmation and administrator-capability validation. The service creates a per-session authenticated control channel and dynamic WFP policy, records only clash5-owned filter identifiers, then publishes the persistent active state. Any failed stage rolls back completed stages in reverse order. Detach, application shutdown, sidecar failure, service restart, or stale-state recovery removes the session filters and releases TUN exclusions before reporting idle. If cleanup cannot be verified, do not start a new injection session and show a recovery-required state.

No test fixture can establish that Windows route state was restored. Release acceptance must inspect WFP state and TUN route/config snapshots before attach, while attached, after detach, after forced app/service termination, and after next-start recovery.

## Controlled service IPC

M5 does not depend on a future upstream service-protocol release. clash5 pins its
own `clash5-service-ipc` fork to a reviewed commit. The fork exposes an authenticated,
capability-gated `process-proxy/session` API for attach, status, and detach. It accepts
only the active owner session, a loopback relay PID and port, and exact targets with a
creation timestamp. The chain credential stays inside the relay and never crosses the
privileged service boundary.

The service records an accepted session before reporting it active and treats a missing,
foreign, unreadable, or uncleared record as recovery-required. At service startup it
detaches a surviving recorded driver session before normal work resumes. This supplies
the control and recovery boundary only: it does not establish WFP filters, relay
behavior, TUN exclusion, driver packaging, or release readiness.

## Controlled Windows driver validation

On 2026-10-22, the current callout binary was built with the WDK 26100 toolchain
and exercised on Windows 10 with test signing enabled. A short-lived local test
certificate signed the binary. The `c5wfptest` demand-start kernel service loaded
successfully, an authenticated administrator probe attached a session, rejected a
detach request with a different session ID, and detached the original session.
The service and test certificate were removed after the probe.

This verifies the driver entry path and its control-device session ownership
boundary only. It did not install WFP policy, redirect live traffic, start a
relay, change TUN configuration, or validate a target process tree. The feature
therefore remains unavailable to product users until the gates below are met.
## Gating questions before implementation

- Choose and package the signed WFP callout driver and privileged service; this repository currently has no driver project or installer lifecycle for one.
- Verify supported Windows versions, signing requirements, Secure Boot behavior, upgrade/uninstall cleanup, and antivirus/anti-cheat observations.
- Prove exact-instance process-tree tracking, including race-free child discovery and PID reuse handling.
- Prove how the active Windows Mihomo TUN backend avoids core re-entry; until then, keep injection unavailable while TUN is active rather than silently changing the user's TUN setting.
- Validate TCP, UDP, IPv4, IPv6, loopback, SOCKS/HTTP chain-pinned listeners, and other WFP redirector coexistence on real Windows hosts.
- Confirm how WFP redirect records are handled when Mihomo, rather than the relay, opens the final destination socket.

Until these gates pass, the UI must report injection as unavailable and must not claim it is active.

## References

- Microsoft, [Using Bind or Connect Redirection](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/using-bind-or-connect-redirection)
- Microsoft, [Application Layer Enforcement](https://learn.microsoft.com/en-us/windows/win32/fwp/application-layer-enforcement--ale-)
- Microsoft, [SIO_SET_WFP_CONNECTION_REDIRECT_RECORDS](https://learn.microsoft.com/en-us/windows/win32/winsock/sio-set-wfp-connection-redirect-records)
- Microsoft, [FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)
- Microsoft, [Failed to redirect connected UDP traffic to a local proxy service](https://learn.microsoft.com/en-us/troubleshoot/windows-hardware/drivers/redirection-connected-udp-traffic-local-proxy-fail)
- MetaCubeX, [Mihomo TUN configuration](https://wiki.metacubex.one/en/config/inbound/tun/)
- MetaCubeX, [Mihomo listener common fields](https://wiki.metacubex.one/en/config/inbound/listeners/)
