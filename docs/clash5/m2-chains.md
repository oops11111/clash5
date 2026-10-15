# M2 chain orchestration

Implementation for [Issue #17](https://github.com/oops11111/clash5/issues/17). Desktop and real-core acceptance is pending; this document describes the current implementation, not a completed milestone.

Each chain has a UUID v4, a profile scope, two source-scoped stable node references, an enabled flag and an exit-IP failure policy. SQLite stores these references without proxy credentials. The existing opaque M0 chain records remain readable through their original API.

After upstream merge/script enhancement and provider path conflict resolution, the generator resolves current inline, manual and provider definitions. It creates private front and landing copies, resets the copied front's inherited dialer, and sets the copied landing's `dialer-proxy` to the copied front. The original definitions, provider overrides and rules remain intact. A stable chain selector is added to existing select groups. Missing nodes, conflicting definitions with the same identity, generated name collisions and fallback cycles reject the candidate configuration atomically.

Provider identities are calculated from their raw definitions before supported provider overrides are applied to the generated copy. External provider payloads must already be cached below the application data directory; unavailable or out-of-root files are not read. The saved chain is rebuilt from current definitions on profile refresh and application startup. A chain is applied only in its owning profile.

Saving and deleting a chain rebuilds and validates the runtime. Rejection restores the previous SQLite definition and confirms the previous runtime. Changing the selected policy group uses the existing backend selection persistence path.

Exit-IP detection creates a temporary HTTP listener bound to `127.0.0.1` on an available port and pins it to the chain selector. It then requests the configured service through that listener and removes that listener. The request sends no node credentials or subscription URL to the service, rejects redirects, has a 12-second timeout and limits the response to 4 KiB. Plain IP and JSON `ip` responses are supported. The default endpoint is [ipify's IPv4/IPv6 service](https://www.ipify.org/). Detection can be disabled or pointed at another HTTPS service; HTTP is accepted only for loopback fixtures.

The default L2 policy warns and keeps the selected route. An explicitly selected fallback is probed first and selected only on success. Detection is user-triggered; no background polling is installed. Failure to remove a temporary listener is a distinct error and does not trigger a fallback switch.

The editor follows the approved prototype's front-to-landing canvas, toolbar, chain list, template and three-column details. Plugin output remains an explicit pending state until M3. Current M2 data tests cover generated configuration, source isolation, renames, ambiguous identities, name conflicts, disabled definitions, SQLite reopen, absent credentials and fallback graph validation.

Official behavior references: [Mihomo dialer-proxy](https://wiki.metacubex.one/en/config/proxies/dialer-proxy/), [provider overrides](https://wiki.metacubex.one/en/config/proxy-providers/), and [listener proxy routing](https://wiki.metacubex.one/en/config/inbound/listeners/).
