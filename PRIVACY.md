# Privacy Policy

**Applies to:** clash5, a desktop client for the mihomo proxy core, built from
this repository (Windows, macOS and Linux).

**Last updated:** 2026-10-23

clash5 is a fork of Clash Verge Rev. This document describes clash5's own
behavior and defaults. Where clash5 still inherits upstream behavior, that is
stated below and tracked in the issues linked from each section.

## 1. Summary

- clash5 has **no telemetry, analytics, crash reporting or accounts**, and it
  does not send data to the maintainers or to any server operated by this
  project.
- Your subscriptions, proxy credentials and settings stay on your device.
- The application contacts only the endpoints you configure, the proxy
  servers you use, and the third-party services listed in section 4. Some of
  those are enabled by default; section 4 says which, and how to turn each one
  off.

## 2. Who is responsible

The clash5 maintainers, reachable through the issue tracker of this repository.
The project has no data controller relationship with users and holds no user
data.

## 3. Data stored on your device

All application data is kept in the application data directory of your system,
named after the application identifier `io.github.oops11111.clash5`. It
contains:

- **Settings** (`verge` settings). WebDAV URL, user name and password are
  stored encrypted with the application's local key. Anyone who can read the
  application data directory as your user account can also read that key.
- **Profiles and subscriptions** (`profiles/`, `profiles.yaml`). These usually
  include subscription URLs, which often embed a personal token, and proxy
  server addresses and credentials.
- **Node inventory** (SQLite database). Node identities, display names,
  server addresses, ports, bindings and probe results. Node credentials are not
  stored in this database. Identity keys for node fingerprints are kept in the
  operating system credential store.
- **Plugin binding settings.** Encrypted with the application's local key, as
  for WebDAV settings (tracked in [#58](https://github.com/oops11111/clash5/issues/58)).
- **Logs.** Application and core logs can contain requested domain names,
  IP addresses and connection metadata, depending on the log level. Logs are
  deleted after a retention period you choose under
  *Settings → Miscellaneous → Auto Log Clean*. The default is 7 days.
- **Runtime files** used by the core, such as its cache and GeoIP/GeoSite
  databases.

Data leaves the device only when you enable WebDAV backup, export a backup, or
attach files to a bug report. Review logs and configuration before sharing
them: they can reveal proxy servers, tokens and visited domains.

## 4. Network connections

### 4.1 Proxy traffic

Traffic routed through clash5 goes to the proxy servers you configured. clash5
does not inspect, record or forward it. Your proxy operator can see this
traffic as any proxy operator can.

### 4.2 Subscription downloads

You add subscriptions by URL. clash5 downloads them when you import or refresh
them, and again on the interval you set for automatic updates. Requests carry
the User-Agent `clash-verge/v<version>`, inherited from upstream and tracked in
[#53](https://github.com/oops11111/clash5/issues/53). The subscription provider
sees your IP address and this request.

### 4.3 Application update check: **off by default**

When enabled, clash5 checks for updates on startup. The default is off
(`auto_check_update: false`). The updater endpoint list in this build is empty,
so no update request is sent until a release channel is configured.

### 4.4 Proxy core update: user-initiated

Updating the mihomo core downloads it from the mihomo GitHub releases page,
only when you start the update.

### 4.5 Home page IP card: **off by default, opt-in**

The Home page card shows your apparent public IP address and its approximate
location. It is off by default; enable it in the Home page card settings. A
saved setting from an earlier version is kept. While the card is visible and
the window is in use, clash5 refreshes it every 5 minutes. Each refresh asks one
service, chosen at random, from:

`api.ip.sb`, `ipapi.co`, `api.ipapi.is`, `ipwho.is`, `ip.api.skk.moe`,
`get.geojs.io`.

These services are independent third parties with their own privacy policies.
Requests go through clash5's local mixed port, so the service sees the address
of the route the running core selects for them. If that route is direct, the
service sees your direct address. Disable the card in the Home page card
settings to stop these requests.

### 4.6 Exit IP detection: user-initiated

Detection is started from the chain editor. clash5 starts a temporary listener
on `127.0.0.1` pinned to the chain you test, and asks the configured service
(by default `https://api64.ipify.org`) for your address through that listener.
No node credentials or subscription URL are sent. Detection runs only when you
start it; there is no background polling.

### 4.7 Latency tests and probes: user-initiated

Testing a proxy or group sends HTTP requests through that proxy to
`http://cp.cloudflare.com/generate_204` or to the test URL you configure. Node
probes in the node pool send requests through the running core to the same
kind of test URL, and run only when you start them.

### 4.8 Streaming availability test: user-initiated

The unlock test sends requests through your selected proxy to the streaming and
AI services it tests. It runs only when you start it.

### 4.9 DNS

The default DNS configuration for new installations uses encrypted DNS over
HTTPS to `doh.pub` and `dns.alidns.com`. Only the names of these two DNS
services are resolved in plain DNS, through the fixed addresses `223.6.6.6` and
`2400:3200::1`. Those are the only plain-DNS requests in the default
configuration.

Resolvers see the domain names you look up. Existing DNS configuration files
are not changed by an upgrade; they keep the values you saved.

The DNS listener for new installations binds `127.0.0.1` only, so other devices
cannot query it. Existing DNS files keep the address they were saved with.

### 4.10 Controller

The mihomo controller listens on `127.0.0.1` only. Each run generates a new
random secret, which the application uses to talk to the controller. The
secret is not a constant and is not stored in the source tree
([#50](https://github.com/oops11111/clash5/issues/50)). Other programs running
as your user account can still connect if they can read the runtime
configuration.

### 4.11 WebDAV backup: disabled until you configure it

If you configure WebDAV, clash5 uploads backup archives to the server you
entered, and only when you start a backup or enable scheduled backup.

### 4.12 System notifications

Desktop notifications use generic text (for example, that the system proxy was
turned on). They never include node names, addresses or domains.

### 4.13 Plugin catalogs: user-initiated

Adding or refreshing a plugin catalog requests, from the catalog's https address:
its index, its signature, its signer key, and, when you choose to install from
it, the plugin package. Requests go through clash5's local mixed port, so they
leave on the route the core selects. They carry no identifiers beyond the
standard request headers, and clash5 sends no subscription, credential or node
data. Catalog contents are verified against the pinned signer before use.

## 5. Plugins

Plugins run in a sandbox with no network access, no file system access and no
host imports. A plugin can only change the fields its manifest declares, and
its audit records contain identifiers, permissions and results, not binding
settings or configuration values.

## 6. What clash5 does not do

- It does not run analytics, telemetry or crash uploads.
- It does not contact the project's servers.
- It does not send your subscriptions, credentials or node lists anywhere.

## 7. Turning things off

| Behavior | Default | How to turn it off |
| --- | --- | --- |
| Update check | Off | *Settings → Miscellaneous → Auto Check Update* |
| Home IP card | Off | Keep it disabled, or change it in Home page card settings |
| Exit IP detection | Only when started | Do not start it, or change the service in the chain editor |
| WebDAV backup | Off until configured | Remove the WebDAV settings |
| Log retention | 7 days | *Settings → Miscellaneous → Auto Log Clean* |

Uninstalling clash5 and deleting its application data directory removes
everything it stored.
