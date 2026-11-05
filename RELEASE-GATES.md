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
| G-13 | P0-11 | partial | - | Desktop transport/UA evidence, direct-connection UX, and certificate-exception acceptance |
| G-14 | P0-14 | blocked | - | Pre-migration snapshot, atomic v11 and downgrade rejection |
| G-15 | P0-15 | blocked | - | Corrupt config preservation and write protection |
| G-16 | P0-16 | partial | scripts/check-privacy-defaults.mjs | Updater disabled in config, frontend and backend; signing and RC artifact policy still unverified |
| P0-12 | P0-12 | partial | scripts/check-privacy-defaults.mjs | Upgrade UI, command and downloader removed; packaged-core service hash acceptance remains |
| P0-13 | P0-13 | partial | - | Strict TLS and bounded staged restore implemented; invalid-cert server acceptance, optional encryption and CA/pin UX remain |
| P0-17 | P0-17 | partial | scripts/check-release-gates.mjs | Complete executable gate coverage, evidence and later-round reconciliation |

## Local implementation evidence (2026-11-04)

- P1-28：`scripts/prebuild.mjs` 将 mihomo 实际字节写入 `resources/core-hashes.json`；服务安装前读取该清单并重新计算核心 SHA-256，清单缺失仅允许开发会话，清单存在但哈希不匹配时拒绝安装。Windows NSIS 仍使用同一构建摘要调用服务安装器。静态门禁：`npm run check:core-hash-gate`；Rust 单测覆盖精确字节接受、篡改拒绝及开发会话无清单。
- P1-29：远程规则集在配置写入和运行时生成两处拒绝非 HTTPS/超长 URL；生成的 mihomo `rule-providers` 强制走 `clash5-default` 并设置 10 MiB `size-limit`。静态门禁：`npm run check:rule-provider-policy`；Rust 单测覆盖代理字段、大小限制和 HTTPS URL 策略。
- P1-30：前端剪贴板读写统一使用 Tauri clipboard-manager 插件，能力文件保留最小读写权限；未声明 create-webview/window create 能力，源码无 `navigator.clipboard`。静态门禁：`npm run check:window-clipboard-policy`。
- P1-31：Boa 自定义脚本设置 128 层递归、4096 VM 栈、1000 万次循环、5 秒超时和最多 2 个并发执行槽；超时后槽位由阻塞任务持有至实际结束。静态门禁：`npm run check:script-safety`；Rust 单测覆盖限制常量。
- P1-32：移除 macOS 默认绕过中的 `*.crashlytics.com`；日志保留本地 panic hook，诊断导出经 `clash_verge_logging::redact` 后复制到本机剪贴板，无自动上报。静态门禁：`npm run check:local-diagnostics`。
- P1-33：新增 `scripts/perf/redline.json`，将流量图三个阶段、主进程/WebView 的 CPU 均值和 RSS P50 固定为 20% 最大回归；`npm run perf:gate` 要求基线和候选各至少 3 次独立 macOS 运行且使用不同构建。当前 Windows 环境不能运行 macOS WKWebView 采样，RC 前仍需上传真实基线/候选报告作为验收证据。
- P1-34：新增 `appStorage`，所有应用端浏览器存储键统一写入 `clash5:v1:`，首次读取旧键时迁移并删除旧键；窗口初始化脚本和桌面 smoke fixture 使用同一命名空间。静态门禁：`npm run check:storage-namespace`；Vitest 覆盖迁移、读写删除和无窗口执行。
- P1-36：Windows 仅注册 `Clash5`，Linux 仅维护 `clash5` MIME handler，解析器拒绝旧 `clash://` / `clash-verge://`；已有且明确指向当前可执行文件的旧 Windows 注册项会被清理。静态门禁：`npm run check:deep-link-scheme`；运行时仍需补“显示订阅主机名并由用户确认后导入”的桌面验收。

- P0-04：生产/开发 CSP、Monaco schema 请求关闭及 `$APPDATA/icons/**` asset scope 通过静态门禁；迁移 capability 不再预置递归 `$APPDATA/**` 文件系统 scope，本地 YAML 导入继续由系统文件选择器/拖放路径触发。仍需在桌面壳中验证选择器授予范围、图标回退及越界读写拒绝。

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

## P1-10 session TUN evidence (issue #72)

TUN generation now projects the saved preference onto the current session
capability. Service loss no longer writes `enable_tun_mode: false`; a later
Service-backed generation restores the saved preference. The projection test,
the 16 Run State/TUN capability tests, formatting, and Windows library compile
check passed locally. Live service handoff and real TUN re-enable remain desktop
acceptance work.

P1-11 issue #73 now maps system Chinese variants to `zh` and all other system
locales to `en`, while manual language choices remain authoritative. The
`i18n:missing-keys` check reports existing non-English pack gaps as CI warnings;
runtime fallback is English and the translation gaps remain follow-up work.

P1-12 issue #74 aligns latency probing at a default concurrency of 10 with an
explicit ceiling of 16 in both the frontend and backend. The cross-layer
`check:probe-concurrency` guard and unit tests cover the default, ceiling, and
legacy-36 regression.

P1-13 issue #75 scopes the traffic subscription cache to the app's observed
core-running session. A stop/start transition advances the session revision,
so stale cumulative totals cannot cross a core restart while live rates and
graph samples continue to use the current stream.

P1-14 issue #76 keeps the advanced settings actions collapsed until explicitly
expanded and places an explicit confirmation in front of the app-exit action.
The disclosure remains keyboard reachable and the existing advanced dialogs are
unchanged once the section is opened.

P1-15 issue #77 adds a single first-run empty-state card on the home profile
surface with exactly three native buttons: Profiles, Node Pool, and Plugins.
The actions use normal buttons (no click-only container), so keyboard traversal
reaches all three entry points.

P1-16 reports migration skip reasons in a persistent warning notice after a
successful Verge import, while retaining the aggregate success report. Reasons
come directly from the backend import report and are not inferred from counts.

## P1-17 home dashboard evidence (issue #79)

The home dashboard no longer exposes user-facing card visibility toggles. After
the first-run profile entry point, it presents four fixed cards: system proxy and
TUN controls, proxy mode/group switching, current-node quality, and DNS leak
checking. The old traffic and diagnostic cards remain available from their
dedicated pages. Frontend type checking, formatting, and the full frontend test
suite are the executable local checks; desktop visual acceptance remains open.

## P1-18 and P1-19 evidence

The node workspace follows the approved high-fidelity layout: a selectable node
list on the left and a fixed-width source/effective-configuration table on the
right, with real inventory records, source filtering, quality sorting and probe
actions. Quality-pool scores now use the median of the latest five delay samples
plus the failure rate in the latest 30 minutes; each latency median is capped at
1000 ms before normalization. Existing caches without history fall back to their
current sample, and the history is bounded to 30 attempts. The data-crate suite
and desktop pool-score tests cover cache compatibility, the median/cap rule and
the time window.

P1-20 notification evidence: non-coded notices now use a stable type/content
key and merge repeated emissions within a 60-second window. The merged item
keeps one dismissal/timer and renders its repeat count; coded failure notices
retain their existing per-code replacement and dialog suppression behavior.
Service-level tests cover the window boundary, severity separation and count.

P1-24 release-resource evidence: `prebuild.mjs` discovers and downloads only the
stable Mihomo release, generates one core digest, and no longer emits alpha
sidecars. Tauri bundle manifests, the Windows installer hook, Linux external-bin
list, development service preparation and shell-smoke placeholders all match
the stable-only inventory. Existing legacy alpha process text remains only in
diagnostic tests so stale processes can still be explained safely.
`node scripts/check-stable-core.mjs` is the executable guard for these resource
lists and passes locally.

## P1-25 geodata pin evidence

`prebuild.mjs` uses the upstream `meta-rules-dat` release tag recorded in
`GEODATA_RELEASE_TAG` and four explicit SHA-256 values for `Country.mmdb`,
`GeoLite2-ASN.mmdb`, `geosite.dat`, and `geoip.dat`. Existing files are checked
against the same pins; a mismatch triggers a fresh download, and a mismatched
download is deleted before the build can continue. The static guard
`node scripts/check-geodata-pins.mjs` passes locally. Updating the upstream tag
requires an intentional digest review and a matching source change.

## P1-26 Windows loopback helper evidence

The prebuild task, Tauri resources, Rust command registry and Settings UI no
longer download, bundle or invoke `enableLoopback.exe`. The helper was an
external elevation surface; Windows users can use the built-in
`CheckNetIsolation.exe LoopbackExempt` commands or TUN mode instead. The static
guard `node scripts/check-no-loopback-helper.mjs` passes locally.

## P1-27 sing-box pin evidence

`prebuild.mjs` downloads sing-box `1.14.2` for every supported desktop target,
checks the official archive SHA-256, extracts the binary into the bundled
resources directory, and records the archive pin in the local build cache. The
resolver accepts only that packaged binary (or the adjacent portable copy) and
never searches `PATH`. CI desktop smoke now uses the same prebuilt resource;
`node scripts/check-singbox-pins.mjs` is the executable guard.
