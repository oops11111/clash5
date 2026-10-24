# M5 Windows process proxy architecture proposal

Status: proposal; requires the Opus-class design review specified by Issue #24 and Windows driver distribution acceptance.

## Decision

Use Windows Filtering Platform (WFP) ALE connect redirection with a signed kernel callout and a user-mode redirect relay. Do not inject DLLs or hooks into target processes. WFP's connect-redirect layer can redirect TCP and UDP connect requests to a local proxy, but local redirection requires a callout driver, a redirect handle, redirect-loop checks, and a target PID for the local service. The relay retrieves the original destination and opaque redirect records and applies the record to its new socket toward the HTTP listener. Mihomo then creates the final outbound socket, so propagation across that boundary remains unresolved and external redirector coexistence is not supported yet.

The relay is a required protocol adapter between WFP and clash5's selected Chain. It accepts redirected sockets, extracts the original destination and redirect context, then forwards the flow to a session-authenticated Mihomo listener pinned to the selected generated chain group. A direct redirect to Mihomo's global SOCKS port is insufficient because it does not pin a flow to the selected Chain. clash5 already creates temporary authenticated, chain-pinned HTTP listeners for exit-IP checks; the process-proxy session can build on that runtime-only pattern. TCP can use HTTP CONNECT to a pinned HTTP listener. UDP needs a pinned SOCKS listener with UDP enabled and its own WFP redirect-record handling. Neither transport is supported until its redirect context, chain selection, and teardown are verified end to end.

## Target identity and process tree

Do not use executable path alone as the active target selector: it can capture unrelated instances of the same executable. The user-mode service should own an immutable target identity containing PID, process creation time, canonical executable path, and file identity. Capture file identity from `FILE_ID_INFO`: its volume serial is 64-bit and its file ID is 128-bit. Treat a failed identity query as an unavailable target, rather than substituting a path or narrowing the ID to the legacy 64-bit file index. Parent links must include the parent's creation time, not only its PID. Accept a child-start event only while that exact parent instance is active, belongs to the selected tree, and was created no later than the child; after a parent exits, retain already tracked descendants but reject later events attributed to the exited instance. A stale exit or child event must never remove or extend a reused PID. The service maintains the selected root and descendants, and sends the current PID/creation-time set to the callout. The callout consults process-ID metadata at ALE connect classification and redirects only active identities. A child starting between initial enumeration and watcher registration, and service restart, must fail closed for the target session until the process tree has been reconciled.

The initial supported target contract remains an executable process tree, included by default. Supporting arbitrary child processes requires a documented and tested process notification source; WFP's ALE app identity condition is a normalized executable path, not a per-instance process-tree identity. UWP/AppContainer targets need separate validation before being advertised.

## TUN coordination

The KernelScheduler remains the single owner of TUN and injection coordination. Before installing redirect filters, it must prepare an exclusion for the relay and proxy core and verify that the TUN configuration actually applies it. If the active TUN backend cannot express the exclusion, injection cannot start while TUN is active. The exclusion must be part of the same attach transaction as the WFP filters; detach removes only clash5-owned filters and restores the prior TUN setting.

The current Mihomo TUN schema documents UID inclusion/exclusion as Linux-only and does not document a Windows process-exclusion setting. A `PROCESS-NAME` routing rule is not proof that the Windows TUN backend excludes a process from capture. Until a tested mechanism prevents proxy-core re-entry, attach must fail closed while TUN is active; do not silently disable TUN or claim N1 coexistence.

## Chain adapter and redirect-record boundary

The chain-pinned listener must be created in runtime configuration with a random session credential, bound to loopback, and removed on every detach or failed attach. Its `proxy` field must name the selected generated chain group. This reuses the existing `exit_ip::probe_listener` pattern without persisting session credentials or listeners.

WFP redirect records are opaque. The relay transfers the record to its socket toward the chain-pinned HTTP listener before connecting, which passed a Windows 10 test with a fixture listener. Mihomo owns the subsequent socket toward the final destination, so clash5's relay cannot attach the record there. Coexistence with other WFP redirectors therefore remains unproven; the relay and Mihomo must be excluded from clash5's own target filters, and external redirector coexistence must not be claimed until tested on Windows.

Connected UDP is a separate compatibility gate. Microsoft documents a failure mode when a redirected local proxy uses connected UDP with redirect records; the documented workaround uses connectionless `sendto`. The relay must either implement and test that path or report UDP as unsupported while preserving TCP functionality.

## Lifecycle and failure policy

Injection stays off after install and after every restart. Attach requires explicit user confirmation and administrator-capability validation. The service creates a per-session authenticated control channel and dynamic WFP policy, then publishes the persistent active state. The WFP session owns its filters and releases them when its engine handle closes, including after a service crash. Any failed stage rolls back completed stages in reverse order. Detach, application shutdown, sidecar failure, service restart, or stale-state recovery removes the session filters and releases TUN exclusions before reporting idle. If cleanup cannot be verified, do not start a new injection session and show a recovery-required state.

No test fixture can establish that Windows route state was restored. Release acceptance must inspect WFP state and TUN route/config snapshots before attach, while attached, after detach, after forced app/service termination, and after next-start recovery.

## Controlled service IPC

M5 does not depend on a future upstream service-protocol release. clash5 pins its
own `clash5-service-ipc` fork to a reviewed commit. The fork exposes an authenticated,
capability-gated `process-proxy/session` API for attach, status, and detach. It accepts
only the active owner session, a claimed relay PID and port, and exact targets with a
creation timestamp. The privileged service now checks that the claimed relay PID
owns listeners on both IPv4 and IPv6 loopback at the requested port before attach.
This check does not keep the listeners alive after attach; the app must own their
lifecycle and react to relay failure before product activation. The chain
credential stays outside the privileged service boundary.

The service persists a pending recovery intent before driver attach, installs IPv4
and IPv6 TCP redirect filters in a dynamic WFP transaction after driver attach,
and marks the record active only after both stages succeed. Status reads back both
WFP filter IDs and confirms that the claimed relay still owns both loopback
listeners before reporting an active session. Status also compares target PIDs
and creation times; if every target has exited, it detaches the recorded session
before reporting idle. Normal core stop detaches the
recorded WFP session before stopping the core; a core crash also detaches it
before the watchdog attempts a restart, and cleanup failure stops recovery.
The service treats an interrupted, foreign, unreadable, or uncleared record as
recovery-required. At service startup it
detaches a surviving recorded driver session before normal work resumes. A filter
permits traffic when its kernel callout is unregistered, avoiding a machine-wide
TCP outage if the driver unloads while the service is still running. This remains
a control boundary: an elevated Windows 10 test installed and read back both
dynamic filters through an independent WFP engine connection, then verified
that both disappeared after closing the session. A separate elevated IPv4 TCP
probe attached an exact child PID and creation time through the service's
driver IOCTL, installed the WFP filters, and observed its `C5!` payload at
a loopback listener owned by a different process. The listener recovered
the original TEST-NET destination 198.51.100.10:443 and matching session ID
from the driver context, and read a nonempty WFP redirect record. The probe
removed the WFP session, temporary driver service, and one-day test certificate.
The fork now also provides a session-bound dual-stack TCP relay. It rejects
connections without matching WFP context, reads the original destination and
redirect record, applies the record to its socket toward an authenticated HTTP
listener, and forwards traffic after CONNECT succeeds. On the Windows 10 test
host, an exact target sent `C5!` through WFP and this relay to a fixture HTTP
listener, then received `OK!` back; the temporary driver service and certificate
were removed and independently confirmed absent. This does not establish
app-owned relay lifecycle, a Mihomo Chain binding, TUN exclusion, or release
readiness.

A separate Windows smoke test starts the bundled Mihomo core with a listener
pinned to the generated two-hop Chain. It rejects anonymous CONNECT with 407;
an authenticated CONNECT reaches the target through both configured hops. This
checks the listener and Chain adapter independently of WFP, not the complete
process-proxy route.

## Controlled Windows driver validation

On 2026-10-23, the protocol-v2 driver built with WDK 26100/MSVC x64 static analysis
and loaded as a demand-start kernel service on Windows 10 in test-signing mode.
An elevated probe rejected a stale process creation time, attached the exact live
PID and creation time, rejected a duplicate attach and a different session's
detach, then passed matching and idempotent detach. The test service and one-day
certificate were removed afterward. Authenticode verification passed; SignTool's
Microsoft-root `/kp` check failed for the self-signed test certificate, so this
does not establish release-signing readiness.

The WFP policy was separately installed, read back through an independent engine
connection, and removed on the same host. Live IPv4 and IPv6 TCP probes read the
driver context and redirect record, passed the record to the relay's HTTP proxy
socket, and completed a bidirectional transfer through a fixture listener. The
same elevated probe then used the normal core-stop path and confirmed that the
WFP filters and recovery record disappeared. It attached a second session and
ran startup reconciliation against its persisted record; the WFP filters and
record disappeared again. A third exact target exited, after which a status
query returned idle and removed its WFP filters and record. This cleanup still
depends on a status query; it does not establish a background watcher, forced
service crash, or watchdog restart. These probes do not validate
UDP traffic, the final Mihomo outbound
socket's redirect record, Chain forwarding, TUN coordination, or a selected
process tree. The feature remains unavailable to product users.

## Remaining gates

- Package and manage the signed WFP callout driver and privileged service lifecycle.
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
