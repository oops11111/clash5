# M5 Windows process proxy architecture proposal

Status: proposal; requires the Opus-class design review specified by Issue #24 and Windows driver distribution acceptance.

## Decision

Use Windows Filtering Platform (WFP) ALE connect redirection with a signed kernel callout and a user-mode redirect relay. Do not inject DLLs or hooks into target processes. WFP's connect-redirect layer can redirect TCP and UDP connect requests to a local proxy, but local redirection requires a callout driver, a redirect handle, redirect-loop checks, and a target PID for the local service. The proxy must read the original destination and opaque redirect records, then attach those records to its outbound socket so other WFP redirectors can cooperate.

The relay is a required protocol adapter between WFP and clash5's selected Chain. It accepts redirected sockets, extracts the original destination and redirect context, authenticates the local session, and opens an authenticated SOCKS5 connection to the selected Mihomo inbound. A direct redirect from WFP to Mihomo's ordinary SOCKS listener is not sufficient: that listener does not implement WFP redirect-record/context handoff. UDP must use the WFP redirect-aware socket path and SOCKS5 UDP relay; it must not assume connected UDP redirection is interchangeable with TCP.

## Target identity and process tree

Do not use executable path alone as the active target selector: it can capture unrelated instances of the same executable. The user-mode service should own an immutable target identity containing PID, process creation time, canonical executable path, and file identity. It maintains the selected root and descendants, and sends the current PID set to the callout. The callout consults process-ID metadata at ALE connect classification and redirects only active identities. PID reuse, a child starting between enumeration and watcher registration, and service restart must fail closed for the target session until the process tree has been reconciled.

The initial supported target contract remains an executable process tree, included by default. Supporting arbitrary child processes requires a documented and tested process notification source; WFP's ALE app identity condition is a normalized executable path, not a per-instance process-tree identity. UWP/AppContainer targets need separate validation before being advertised.

## TUN coordination

The KernelScheduler remains the single owner of TUN and injection coordination. Before installing redirect filters, it must prepare an exclusion for the relay and proxy core and verify that the TUN configuration actually applies it. If the active TUN backend cannot express the exclusion, injection cannot start while TUN is active. The exclusion must be part of the same attach transaction as the WFP filters; detach removes only clash5-owned filters and restores the prior TUN setting.

## Lifecycle and failure policy

Injection stays off after install and after every restart. Attach requires explicit user confirmation and administrator-capability validation. The service creates a per-session authenticated control channel and dynamic WFP policy, records only clash5-owned filter identifiers, then publishes the persistent active state. Any failed stage rolls back completed stages in reverse order. Detach, application shutdown, sidecar failure, service restart, or stale-state recovery removes the session filters and releases TUN exclusions before reporting idle. If cleanup cannot be verified, do not start a new injection session and show a recovery-required state.

No test fixture can establish that Windows route state was restored. Release acceptance must inspect WFP state and TUN route/config snapshots before attach, while attached, after detach, after forced app/service termination, and after next-start recovery.

## Gating questions before implementation

- Choose and package the signed WFP callout driver and privileged service; this repository currently has no driver project or installer lifecycle for one.
- Verify supported Windows versions, signing requirements, Secure Boot behavior, upgrade/uninstall cleanup, and antivirus/anti-cheat observations.
- Prove exact-instance process-tree tracking, including race-free child discovery and PID reuse handling.
- Prove the active Mihomo TUN backend can exclude both the relay and core; otherwise define the safe mutual-exclusion behavior.
- Validate TCP, UDP, IPv4, IPv6, loopback, and other WFP redirector coexistence on real Windows hosts.

Until these gates pass, the UI must report injection as unavailable and must not claim it is active.

## References

- Microsoft, [Using Bind or Connect Redirection](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/using-bind-or-connect-redirection)
- Microsoft, [Application Layer Enforcement](https://learn.microsoft.com/en-us/windows/win32/fwp/application-layer-enforcement--ale-)
- Microsoft, [SIO_SET_WFP_CONNECTION_REDIRECT_RECORDS](https://learn.microsoft.com/en-us/windows/win32/winsock/sio-set-wfp-connection-redirect-records)
- Microsoft, [Failed to redirect connected UDP traffic to a local proxy service](https://learn.microsoft.com/en-us/troubleshoot/windows-hardware/drivers/redirection-connected-udp-traffic-local-proxy-fail)
