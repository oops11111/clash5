# clash5 工程手册（r2 草案）

> 产品名：**clash5**
> 基线：Fork [clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev) · GPL-3.0 · 上游 tag 跟踪（见 §1.2）
> 配套：[`clash5-M0-M6-任务拆解.md`](clash5-M0-M6-任务拆解.md)、[`clash5-风险登记.md`](clash5-风险登记.md)、[`clash5-AI协作规范.md`](clash5-AI协作规范.md)
> 状态：r2 修订稿，待整体评审冻结。M0 原验收基线已合并；新增标准的实现差距见下文，不视为已通过验收。身份迁移方案由 PR #8 确认，代码迁移、fixture 与 Windows 桌面验收已由 [PR #23](https://github.com/oops11111/clash5/pull/23) 合并通过；R-25 已缓解。

## 变更说明（相对 r1）

- 补入此前只存在于决策速查表中的规则，使本文件可独立阅读（§2、§6）。
- 新增身份稳定性、链路标识、本地中转鉴权、TUN 与注入协调、补丁权限等规范（§4 扩写）。
- 删除 AI 协作流程（原 §5），迁至 `clash5-AI协作规范.md`。
- 增加与 verge-rev 共存、数据迁移、发布签名、上游同步策略（§6–§9）。
- 测试清单补充 IPC 契约、迁移、日志脱敏、内核重启、CI 矩阵（§1.4）。

---

## 1. 开发规范

### 1.1 语言与目录

| 层 | 技术 | 约定 |
| --- | --- | --- |
| 桌面壳 | Tauri 2 + Rust | 业务逻辑、插件 Host、SQLite、内核调度放 Rust |
| 前端 | TypeScript + React 19 + Vite + MUI（M0.1 已核对仓库） | 新页面走 i18n；禁止硬编码中文文案 |
| 主内核 | Mihomo | 配置仍以 YAML + provider 为主 |
| 侧车 | sing-box（按需） | JSON outbound；仅本地回环；不启用 TUN |
| 插件 | WASM 优先；原生仅官方/白名单（I1） | Manifest + 签名 |

clash5 新增代码尽量集中于独立模块，减少上游 merge 冲突。现有 M0/M1 实现位于 `crates/clash5-data/`、`src-tauri/src/cmd/clash5_data.rs` 与 `src/pages/node-pool.tsx`，并修改了订阅刷新接线；这些是现状，不应为迎合建议目录而搬迁。后续新增接触点须在 PR 中说明。

### 1.2 Git、分支与上游

- `main` 可发版；`dev` 日常集成；`feature/<topic>` 单主题。
- Commit 遵循仓库 `AGENTS.md`：英文 Conventional Commits；需签名提交。
- PR 小步可审，说明「解决什么 / 如何验证」，并关联预先建立的 issue。
- **上游同步**：默认跟随上游 **release tag**，而非 `dev` 分支头；每次同步开独立 PR，分支名 `sync/upstream-<tag>`。
- 同步冲突优先保 clash5 模块（插件、链路、SQLite）；冲突解决需附在 PR 描述中列出每个冲突文件的取舍理由。
- 上游 `dev` 仅用于提前评估破坏性变更，不作为合并基线。

### 1.3 代码风格

- Rust：`rustfmt` + `clippy`（CI 必过，`-D warnings`）；公共 API 需简短注释。
- TS：沿用项目既有 ESLint/Prettier；新文件不引入第二种风格。
- 错误：对外（CLI/IPC/日志）使用结构化错误码，格式 `C5-<领域>-<编号>`；不向用户弹出原始堆栈。
- 日志：默认 `info`。以下内容**不得**进入普通日志：节点凭证、完整订阅 URL、Mihomo external-controller secret、本地中转鉴权令牌。日志写入前经统一脱敏函数，该函数须有单测。

### 1.4 测试

**必须测**（单元/集成，CI 中运行）：

- `stable_id` 稳定性与凭证归一化（含 §4.1 的版本迁移用例）
- 订阅 diff → 继承 / 新增 / 孤儿（A2）；孤儿恢复、保留期提醒与用户确认后的清理
- 插件补丁冲突合并：高优先级优先、同优先级按 `binding_id` 决胜，并验证手动规则不被覆盖（§4.3）
- 热更新失败回滚（D2）
- 内核异常退出与重启上限（K2）
- 配置生成快照（dialer-proxy / 中转端口）
- SQLite 迁移：空库初始化、版本号升级、迁移失败回退
- IPC 契约：每个 Tauri command 的入参/出参 schema 快照
- 日志脱敏：输入含凭证的样本，断言输出不含原文

**不测**：为覆盖率堆砌的空测；真连机场；无机器时的注入（标 `skip` 并附手测清单）。

**CI 矩阵**（T2 的定义）：Linux、Windows、macOS 各跑 `cargo test`、`clippy`、前端 lint 与单测；Windows 与 macOS 允许仅编译不跑集成测试，但必须编译通过。

### 1.5 安全与权限

- 插件默认拒绝；权限见 §4.4 枚举；敏感权限按 G3 分级确认。
- WASM：32MB 内存 / 3s 执行 / 默认无网络、无文件系统（白名单 hosts 与白名单路径除外）。
- `process.inject` 仅白名单原生插件可用；Windows 注入默认关（M1）。
- 诊断「一键复制」必须经脱敏后输出，并在界面标明已脱敏范围。
- 本地中转端口（F1）必须随机分配并启用令牌认证，见 §4.5。

### 1.6 UI

- 新交互对齐已确认的[浅色桌面 UI 基准](clash5-ui-design.md)与工作区外层 `src/ui-hifi` 原型；若与现有组件库冲突，以 M0.1 的评估结论为准。原型中的模拟业务数据不得进入正式页面。
- 语言：zh-CN + en；默认跟随系统（V1）。
- 性能三档（R1）；高内存自动缓解（Q3）。

---

## 2. 硬约束（不可破）

1. **订阅更新不得覆盖**用户规则、手动节点、链路定义；只刷新订阅线路。
2. **节点身份用 `stable_id`**，不用显示名；名称匹配仅高级开关，默认关（B）。
3. **链式主路径**：Mihomo `dialer-proxy` 单进程；禁止默认多进程拼接。
4. **TUN 仅 Mihomo**；sing-box 不启用 TUN；专属节点走 F1 本地中转。
5. **注入与 TUN 互斥协调**：已被注入的进程流量优先走注入链路（N1），且 TUN 必须排除注入目标（见 §4.6），禁止双重接管。
6. **热更新失败必须回滚并明确通知（D2）**，服务不中断。
7. **第三方插件只能 WASM（I1）**；危险权限不出白名单。
8. **许可**：宿主 GPL-3.0；分发须满足 GPL；不引入与 GPL 冲突的闭源核心依赖；sing-box 分发前核对其许可证附加条款（见风险登记）。
9. **仓库**：不把完整订阅、用户节点凭证、签名私钥提交进 git。
10. **协作流程**见 `clash5-AI协作规范.md`；仓库操作同时遵守 `AGENTS.md`。开发环境由任务与项目负责人决定。

---

## 3. 路线图与退出标准

| 里程碑 | 主题 | 退出标准（可验证） |
| --- | --- | --- |
| **M0** | Fork、CI、stable_id、SQLite、orphan、共存与迁移骨架 | ① 订阅更新后，绑定保留率 = 100%（fixture 测试）；② 人为删除节点后孤儿可在保留期内恢复并重绑；③ 新旧版本可同机共存（独立 identifier 与目录） |
| **M1** | 节点池、解耦、去重、探测 P2 | 节点池页面可导入、去重、并发 16 探测，单次探测失败不影响其它节点（集成测试 + 演示脚本） |
| **M2** | 链路编排、dialer-proxy、出口 IP | 在 fixture 环境中，前置+落地两跳配置生成正确；出口 IP 检测结果与 L2 策略一致 |
| **M3** | 插件 Host、绑定、生效配置 UI | 绑定可回滚（D2 测试通过）；生效配置视图可显示每个字段的来源插件 |
| **M4** | sing-box 侧车 E3/F1 | 指定协议经本地中转走 sing-box；侧车退出后 Mihomo 无残留路由 |
| **M5** | Windows L6b 注入 | 指定进程树的流量经链路出站；关闭注入后系统路由恢复（自动化或手测清单） |
| **M6** | 玻璃 UI、性能档、Release + 更新检查 | 测试通道安装包可在 Win/macOS 安装、启动并完成一次更新检查；签名验证通过 |

并行：M5 可与 M4 后期重叠；M6 在 M3/M4 稳定后收口。

发版：GitHub Release + 应用内检查更新；稳定/测试通道，测试默认关（U2/W1）。

### r2 与现有实现的差距（2026-10-17）

M0 的原验收已完成并合并至 `dev`。以下是本草案新增或收紧的要求，需作为后续改进单独验收，不追认成 M0 的已完成项：

| 范围 | 已核对现状 | r2 待补齐 |
| --- | --- | --- |
| 身份与孤儿 | PR #23 已将身份迁移为本机密钥 HMAC，包含版本/密钥代次、同来源凭证轮换候选重绑、多候选确认、密钥异常恢复，以及 WAL/schema v4 和孤儿保留期/清理 | R-01/R-02/R-25 的迁移与回滚 fixture、CI 和 Windows 桌面验收均通过并已合并；导出/导入包仍是任务拆解中的独立未决项 |
| SQLite | PR #23 已将 SQLite schema 升至 v4，启用 WAL、分代元数据与逐级迁移 | 迁移失败回滚及孤儿保留期测试通过 CI/Windows 验收；PR #23 已合并 |
| 共存与导入 | PR #27 已通过 Windows 同机安装/升级/启动/独立卸载及上游配置目录 SHA-256 前后比对，Windows/macOS/Linux 标识审计通过；PR #26 已只读导入 verge-rev 数据 | R-18 等 PR #27 合入 dev；R-19 的源目录完整性和 Windows 桌面验收已通过并随 PR #26 合并。macOS/Linux 安装运行测试为后续覆盖 |
| 安全与契约 | M0 IPC 返回结构化错误；PR #31 稳定 SQLite 错误码，PR #32 已完成节点数据命令 `C5-*` 统一且当前 CI/Windows 桌面验收通过，均待依序集成；当前契约生成器由 CI 校验 | `dev` 注册 108 个 Tauri 命令；事件契约覆盖 11 个事件 payload 与 34 个通知状态。PR #34 已实现全部命令的参数、成功返回与错误类型快照，并通过注册表覆盖及前端类型校验；完整桌面验收和合并仍待完成。日志/诊断脱敏仍未完成验收；R-20 保持开放 |
| 上游同步 | M0 文档及仓库基线来自上游 `dev` | 本草案将后续同步改为 release tag 的独立 PR；首次采用前核对目标 tag 与当前基线关系 |
| M1 节点池 | PR #4 已实现库存、导入、手动节点运行时映射、受限探测与连续失败退避；[Server 2022 原生验收](https://github.com/oops11111/clash5/actions/runs/38647386824)通过页面交互、外部 provider 健康检查、筛选、搜索、质量排序与重启身份检查，附桌面截图 | M1 已达到退出标准并合并至 `dev`。截图覆盖默认 940 × 700 窗口，未单独截图验证 1600 × 900 布局；完整合并配置/插件来源属于后续里程碑。Mihomo 接口未暴露上游 HTTP 状态及 `Retry-After`，当前为连续失败通用退避（R-11/R-26） |

M0 原始实现说明见 [`clash5/implementation.md`](clash5/implementation.md)；M1 已合入 `dev`；M2 链路变更及限制见[PR #18](https://github.com/oops11111/clash5/pull/18)。

---

## 4. 关键类型与设计约束

> 名称可按落地微调；职责与下列约束不可漂。

### 4.1 身份与存储

```text
StableId        = HMAC-SHA256(key_local, canonical(type, server, port, transport, sni, credential_fingerprint))
                  - canonical 字段清单为本节唯一定义，变更须升版本
                  - stable_id_version 随记录存储；版本升级时提供重算迁移，不得静默改变已存 ID
                  - credential_fingerprint 为凭证归一化后的单向摘要（同样带 key_local），不得可逆
                  - key_local 存于系统密钥库，不入库、不入日志
ChainId         = UUID v4（不由 front/landing 派生）；链路记录中保存 front_ref 与 landing_stable_id 作为普通字段
NodeSource      = Manual | Subscription { provider_id }
PluginBinding   = { id, plugin_id, target_type, target_id, priority, enabled, config, source_aware }
OrphanBinding   = PluginBinding + disappeared_at + last_stable_id + candidates[]
```

**设计决定**：

- 凭证轮换（UUID/密码变化）导致的 stable_id 变化，视为「同一节点的新身份」。`reattach` 仅在**同一订阅来源内**对失活身份按 type、server、port、transport、SNI 匹配：候选唯一时自动重绑，多候选时进入人工确认；手动节点与不同 provider 不跨来源自动重绑。
- 本机密钥丢失、锁定或不匹配时，保持现有节点与绑定可读，暂停刷新和身份写入；须由用户确认重置后逐项显式重绑，不得生成替代密钥后静默迁移。计划内轮换在旧密钥仍可用时才可精确迁移。
- r2 的密钥分代、`v1`→`v2` 迁移、规范字段及崩溃恢复顺序以 [`clash5/r2-identity-migration.md`](clash5/r2-identity-migration.md) 为准；代码与 fixture 验收已随 [PR #23](https://github.com/oops11111/clash5/pull/23) 完成并合并（R-01/R-02/R-25）。
- 同机同端口不同 transport/SNI 的节点，因 canonical 字段包含 transport 与 sni，不会碰撞；若仍碰撞，diff 阶段报错而非静默合并。

**孤儿策略（A2 补充）**：

- 保留期默认 30 天，可在设置中修改（范围 1–365 天）。
- 超期节点进入待清理状态；界面提示后用户可选择「保留」。无人确认时不得静默清除绑定。
- 一个孤儿有多个候选时，不自动选择，保留为孤儿并列出候选。

```text
trait NodeIdentity { fn stable_id(&self) -> StableId; }

struct BindingStore { /* SQLite, WAL 模式 */ }
impl BindingStore {
  fn upsert_binding(...)
  fn list_by_target(...)
  fn mark_orphan_on_diff(...)      // A2
  fn reattach_orphans(...)         // 候选唯一自动绑定；多候选返回待确认
  fn purge_orphans(...)            // 按保留期
}
```

**存储约束**：

- 数据库继续使用 `PRAGMA user_version` 作为唯一 schema 版本标记；r2 迁移与版本号推进在单事务内执行，失败整体回滚，不另建第二套版本权威。
- YAML 配置与 SQLite 为双数据源：SQLite 为绑定关系的唯一事实来源；YAML 为每次生成的派生产物，不回写。
- 含凭证的字段（如插件 `config` 中的密钥类字段）使用系统密钥库或 AES-GCM 加密后存储，密钥同样存于系统密钥库。

### 4.2 订阅 diff

```text
struct ProviderDiff {
  matched: Vec<(old, new)>,   // 继承绑定与质量
  added: Vec<Node>,
  removed: Vec<StableId>,     // → orphan
  ambiguous: Vec<(old, Vec<new>)>,  // 多候选，待确认（§4.1）
}

trait SubscriptionService {
  fn refresh_provider(id) -> ProviderDiff;
}
```

订阅拉取失败（网络/解析错误）时，**不产生任何 diff**，不改变现有绑定与孤儿状态。

### 4.3 链路、补丁与合并

```text
struct ProxyChain { id: ChainId, front: FrontRef, landing: StableId, plugins: Vec<BindingId> }

struct PluginPatch {
  target_type, target_id,
  operations: Vec<{ path, op: Set|Delete|Merge, value }>,   // path 必须命中白名单前缀
  metadata: { plugin_id, binding_id, priority }
}

trait PatchMerger {
  fn merge(patches: Vec<PluginPatch>) -> Result<EffectiveConfig, MergeError>;
}

trait KernelAdapter {
  fn apply_patches(cfg: EffectiveConfig) -> Result<(), HotReloadError>;
  fn rollback_last_good();
}
```

**补丁约束**：

- 每个插件的 manifest 声明可写路径前缀（如 `proxies.*`、`rules`、`dns.nameserver`）；超出声明的 path 在合并前拒绝，整个补丁失效。
- 所有补丁先经 schema 校验（对应 Mihomo 配置的结构约束），校验失败的补丁整体丢弃并记录。
- 合并顺序：先执行 verge-rev 原有的 Merge/Script 增强，再执行 clash5 插件补丁，最后生成 `EffectiveConfig`。插件补丁不可覆盖用户的手动规则（硬约束 1）。
- 合并冲突解决：同一 path 多个补丁时，按 `priority` 高者胜；同优先级按 `binding_id` 字典序；冲突表以本节为准，并在 `EffectiveConfig` 中记录每个字段的来源。
- 任一补丁应用失败，整个 `apply_patches` 回滚到 `rollback_last_good`（D2）。

### 4.4 插件 Host 与权限枚举

```text
struct PluginManifest {
  id, version, runtime: Wasm|Native, entry, signature,
  permissions: [Permission],
  writable_paths: [String]      // 补丁路径前缀白名单
}

enum Permission {
  ConfigRead,                   // 只读生效配置（低）
  ConfigPatch,                  // 提交补丁（中）
  NetHost(hostname),            // 指定域名出站（高，需 G3 确认）
  FsRead(path_prefix),          // 白名单路径读（高）
  FsWrite(path_prefix),         // 白名单路径写（高）
  ProcessInject(exe),           // 仅原生白名单（高，M1 默认关）
}

trait PluginRuntime {
  fn load(pkg) -> Result<PluginHandle>;
  fn call_hook(handle, hook, ctx) -> Result<Vec<PluginPatch>>;
  fn unload(handle);
}

struct PluginHost { registry, permissions, audit, sandboxes }
```

- 权限枚举为封闭集合，新增权限需修订本文件。
- 安装插件时逐项展示权限并按 G3 分级确认；审计日志记录每次授予与使用。

### 4.5 内核调度与本地中转（F1）

```text
enum KernelKind { Mihomo, SingBox }

trait KernelScheduler {
  fn ensure_mihomo();
  fn ensure_singbox_if_needed();  // 懒启动；空闲退出 E3
  fn route_node(node) -> KernelKind;
  fn local_relay_to_singbox(...); // F1
}
```

- **本地中转**：监听 `127.0.0.1` 随机高位端口；每次启动生成会话令牌，中转入口要求令牌（SOCKS5 用户名/密码或首包头部校验）；令牌不写日志、不写磁盘。
- **Mihomo external-controller**：绑定回环地址，启用 secret，secret 由应用生成并存入系统密钥库。
- **内核异常**：按 K2 在 10 分钟窗口内最多重启 3 次，超限进入降级态并通知用户，不再自动重启。
- **sing-box 前置/落地**：M4 仅支持 sing-box 作为落地（出口）；sing-box 作为前置的场景不在本版本范围内，写入风险登记。

### 4.6 进程级注入（Windows，M5）

```text
trait ProcessProxyEngine {
  fn attach(exe, opts: { include_tree: true }) -> Result<()>;  // O1 默认树
  fn detach(exe);
  // 注入流量 → 本地入站 → 指定 Chain
}
```

- **与 TUN 的协调**：注入启用时，TUN 的路由规则必须排除注入流量的本地入站端口与内核自身进程（按进程或端口排除）；否则进入回环。该排除规则由 `KernelScheduler` 统一下发，不由插件决定。
- 注入默认关（M1）；启用需二次确认，并在界面持续显示「注入中」状态。
- 退出或崩溃时必须执行 `detach` 并恢复路由；启动时检测残留注入状态并清理。

### 4.7 前端 IPC（Tauri commands）

- `nodes/list` · `bindings/bind` · `bindings/unbind`
- `chains/save` · `chains/test` · `exit_ip/detect`
- `plugins/install` · `plugins/effective_config`
- `settings/performance_preset` · `update/check`

每个 command 的参数与返回 schema 纳入 §1.4 的契约快照测试。

---

## 5. 决策速查（r2 修订稿，待冻结）

每条决策附理由与影响范围，供评审与日后回溯。

| 代号 | 结论 | 理由 | 影响 |
| --- | --- | --- | --- |
| A2 | 消失节点绑定变孤儿，恢复后重绑 | 机场临时摘节点很常见，直接删除会丢用户配置 | §4.1、§4.2 |
| B | 名称匹配默认关 | 显示名不稳定，误绑风险高 | §2-2 |
| C2 | 手动/订阅逻辑隔离 | 保证订阅更新不影响用户手动数据 | §2-1 |
| D2 | 热更失败回滚并通知 | 避免配置半生效导致断网 | §4.3 |
| E3 | sing-box 默认可退出，可改常驻 | 减少不用时的资源占用 | §4.5 |
| F1 | sing-box 走本地中转 | 避免 sing-box 抢占 TUN，且中转可统一鉴权 | §4.5 |
| G3 | 敏感权限分级确认 | 插件权限可能涉及凭证与文件 | §4.4 |
| H | WASM 32MB / 3s / 默认无网无盘 | 限制插件资源与越权面 | §1.5 |
| I1 | 第三方仅 WASM | 原生代码无法沙箱化 | §2-7 |
| J2 | 死连接 120s / 30s | 与内核健康检查配合 | §4.5 |
| K2 | 内核 10 分钟最多重启 3 次 | 防止崩溃循环 | §4.5 |
| L2 | 出口 IP 失败默认告警，可自动切 | 避免误切导致链路抖动 | M2 |
| M1 | 注入默认关 | 注入影响面大、误报风险高 | §4.6、§1.5 |
| N1 | 注入优先于 TUN，TUN 排除注入目标 | 防止双重接管与回环 | §2-5、§4.6 |
| O1 | 默认进程树 | 子进程常被遗漏 | §4.6 |
| P2 | 探测并发 16 / 间隔 600s | 控制对机场的请求压力 | M1 |
| Q3 | 高内存默认自动缓解 | 降低低配设备崩溃概率 | §1.6 |
| R1 | 三档性能预设 | 兼顾低配与高配 | §1.6 |
| S2（r2 修订） | 后续上游同步以 release tag 为基线，走独立 merge PR | r1 使用 `upstream/dev`；为降低 dev 分支不稳定性而修订，首次同步须核对现有基线 | §1.2 |
| T2 | 标准 CI 矩阵 | 见 §1.4 | §1.4 |
| U2 | Release + 应用内检查更新 | 用户无需手动下载 | §3 |
| V1 | 语言跟随系统 | 默认体验 | §1.6 |
| W1 | 测试通道默认关 | 避免普通用户误装不稳定版本 | §3 |
| 品牌 | **clash5** | — | — |

---

## 6. 与 verge-rev 共存与数据迁移

- **标识隔离**：独立的 app identifier、配置目录、系统服务名、协议名（如 `clash5://` 替代 `clash://`），确保与 verge-rev 同机可共存。
- **数据迁移**：提供一次性导入向导，读取 verge-rev 的订阅列表与基础配置；不迁移插件数据（clash5 新增）。导入失败不影响 verge-rev 原数据，且不修改其目录。
- **迁移范围**须在 M0 退出前列出清单，并附测试。

---

## 7. 发布、签名与密钥管理

- **Tauri updater 签名密钥**：私钥仅存放于 CI 的加密密钥库，不入仓库；由指定负责人保管，并记录轮换流程。
- **平台签名**：Windows 使用代码签名证书，macOS 进行 Developer ID 签名与公证。证书来源与费用在 M6 前确定。
- **发布流程**：tag → CI 构建 → 签名 → 生成 Release 草稿 → 人工确认后发布；测试通道与稳定通道使用不同的更新端点。
- **回滚**：若某版本出现严重问题，撤回 Release 并在更新端点指回上一稳定版本。

---

## 8. 隐私与第三方服务

- **出口 IP 检测**：默认使用的服务在 M2 前确定并写入本节；用户可在设置中关闭检测，或更换服务地址。
- 检测请求不附带节点凭证、订阅 URL 或用户标识。
- 探测（P2）对机场的请求频率受并发与间隔限制；若机场返回限流响应，探测自动退避，不重试风暴。
- 应用不内置任何遥测；若未来增加，需单独设计并经用户明确同意。

---

## 9. 上游同步

- 同步周期：每个上游 release tag 评估一次，不设固定日期。
- 同步步骤：开 `sync/upstream-<tag>` 分支 → merge → 运行全部 CI → 对 clash5 模块做回归 → 开 PR。
- 同步 PR 需附「上游变更摘要」与「clash5 受影响点」两部分。
- 若上游改动了 §1.1 所列的接触点，须同步评估该接触点是否需要调整。

---

*维护：重大决策变更时同步改本文件与 `clash5-M0-M6-任务拆解.md`。决策代号变更需在 §5 注明原因。*
