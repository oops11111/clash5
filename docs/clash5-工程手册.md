# clash5 工程手册

> 产品名：**clash5**
> 基线：Fork [clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev) · GPL-3.0 · 上游 `dev` · 同步 **merge（S2）**
> 配套：`M0-M6-任务拆解.md`（本目录）

---

## 1. 开发规范

### 1.1 语言与目录

| 层 | 技术 | 约定 |
|----|------|------|
| 桌面壳 | Tauri 2 + Rust | 业务逻辑、插件 Host、SQLite、内核调度放 Rust |
| 前端 | 现有 Verge 栈（TS + React/Vue 以仓库为准） | 新页面走 i18n；禁止硬编码中文文案 |
| 主内核 | Mihomo | 配置仍以 YAML + provider 为主 |
| 侧车 | sing-box（按需） | JSON outbound；仅本地回环；不抢 TUN |
| 插件 | WASM 优先；原生仅官方/白名单（I1） | Manifest + 签名 |

### 1.2 Git 与分支

- `main` 可发版；`dev` 日常集成；`feature/<topic>` 单主题。
- Commit：英文祈使句或中文简述二选一，**同一 PR 内统一**；需签名提交（跟上游习惯）。
- PR：小步、可审；必须能说明「解决什么 / 如何验证」；关联 issue（若有）。
- 上游：定期 **merge** `upstream/dev` 进 `dev`，冲突优先保 clash5 模块（插件、链路、SQLite）。

### 1.3 代码风格

- Rust：`rustfmt` + `clippy`（CI 必过）；公共 API 要有简短注释。
- TS：项目既有 ESLint/Prettier；新文件不引入第二种风格。
- 错误：对外（CLI/IPC/日志）用结构化错误码；不对用户弹原始堆栈。
- 日志：默认 `info`；节点凭证、完整订阅 URL **不得**进普通日志。

### 1.4 测试（T2）

必须测：

- `stable_id` 稳定性与凭证归一化
- 订阅 diff → 继承 / 新增 / **orphan（A2）**
- 绑定冲突合并（Override / MostRestrictive 等）
- 热更新失败回滚（D2）
- 配置生成快照（dialer-proxy / 中转端口）

不测：为覆盖率堆的空测；真连机场；无机器时的注入（标 skip + 手测清单）。

### 1.5 安全与权限

- 插件默认拒绝；权限见已定枚举；G3 分级确认。
- WASM：32MB / 3s / 无网无盘（除非白名单 hosts）。
- `process.inject` 仅白名单原生；Windows 注入默认关（M1）。
- 诊断「一键复制」必须脱敏。

### 1.6 UI

- 新交互对齐已产出的设计提示词（玻璃拟态 + Verge 侧栏）。
- 语言：zh-CN + en；默认跟随系统（V1）。
- 性能三档（R1）；高内存自动缓解（Q3）。

---

## 2. 硬约束（不可破）

1. **订阅更新不得覆盖**用户规则、手动节点、链路定义；只刷新订阅线路。
2. **节点身份用 `stable_id`**，不用显示名；名称匹配仅高级开关且默认关。
3. **链式主路径**：Mihomo `dialer-proxy` 单进程；禁止默认多进程拼接。
4. **TUN 仅 Mihomo**；sing-box 不启用 TUN；专属节点走 **F1 本地中转**。
5. **注入与 TUN**：已注入进程 **注入优先（N1）**，禁止双重接管。
6. **热更新失败必须回滚并明确通知（D2）**，服务不中断。
7. **第三方插件只能 WASM（I1）**；危险权限不出白名单。
8. **许可**：宿主 GPL-3.0；分发须满足 GPL；不引入与 GPL 冲突的闭源核心依赖。
9. **仓库**：不把完整订阅、用户节点凭证提交进 git。
10. **云端改代码**：非琐碎仓库工作交给 Cursor Cloud Agent；本机不随意 clone 大仓库排障（见第 5 节）。

---

## 3. 路线图（摘要）

| 里程碑 | 主题 | 退出标准 |
|--------|------|----------|
| **M0** | Fork、CI、stable_id、SQLite、orphan | 订阅更新不丢绑定；孤儿可恢复 |
| **M1** | 节点池、解耦、去重、探测 P2 | 节点池可演示 |
| **M2** | 链路编排、dialer-proxy、出口 IP | 可视化建链可用 |
| **M3** | 插件 Host、绑定、生效配置 UI | 绑定可回滚、来源可追溯 |
| **M4** | sing-box 侧车 E3/F1 | 特定协议可走侧车 |
| **M5** | Windows L6b 注入 | 指定进程树走链路 |
| **M6** | 玻璃 UI、性能档、Release+更新检查 | clash5 测试版可装 |

并行：M5 可与 M4 后期重叠；M6 在 M3/M4 稳定后收口。

发版：GitHub Release + 应用内检查；稳定/测试通道，测试默认关（U2/W1）。

---

## 4. 关键类型设计（Rust 侧概念）

> 名称可按落地微调；职责不可漂。

### 4.1 身份与存储

```text
StableId        = 节点指纹哈希（type+server+port+凭证归一化）
ChainId         = hash(front_ref + landing_stable_id) 或 UUID+显示名
NodeSource      = Manual | Subscription { provider_id }
PluginBinding   = { id, plugin_id, target_type, target_id, priority, enabled, config, source_aware }
OrphanBinding   = PluginBinding + disappeared_at + last_stable_id
```

```text
trait NodeIdentity {
  fn stable_id(&self) -> StableId;
}

struct BindingStore { /* SQLite */ }
impl BindingStore {
  fn upsert_binding(...)
  fn list_by_target(...)
  fn mark_orphan_on_diff(...)      // A2
  fn reattach_orphans(...)         // stable_id 恢复
  fn purge_orphans(...)
}
```

### 4.2 订阅 diff

```text
struct ProviderDiff {
  matched: Vec<(old, new)>,   // 继承绑定与质量
  added: Vec<Node>,
  removed: Vec<StableId>,     // → orphan
}

trait SubscriptionService {
  fn refresh_provider(id) -> ProviderDiff;
}
```

### 4.3 链路与补丁

```text
struct ProxyChain { id: ChainId, front: FrontRef, landing: StableId, plugins: Vec<BindingId> }

struct PluginPatch {
  target_type, target_id,
  operations: Vec<{ path, op: Set|Delete|Merge, value }>,
  metadata: { plugin_id, binding_id, priority }
}

trait PatchMerger {
  fn merge(patches: Vec<PluginPatch>) -> EffectiveConfig; // 冲突表已定
}

trait KernelAdapter {
  fn apply_patches(cfg: EffectiveConfig) -> Result<(), HotReloadError>;
  fn rollback_last_good();
}
```

### 4.4 插件 Host

```text
struct PluginManifest { id, version, permissions[], runtime: Wasm|Native, entry, signature }

trait PluginRuntime {
  fn load(pkg) -> Result<PluginHandle>;
  fn call_hook(handle, hook, ctx) -> Result<Vec<PluginPatch>>;
  fn unload(handle);
}

struct PluginHost {
  registry, permissions, audit, sandboxes
}
```

### 4.5 内核调度

```text
enum KernelKind { Mihomo, SingBox }

trait KernelScheduler {
  fn ensure_mihomo();
  fn ensure_singbox_if_needed();  // 懒启动；空闲退出 E3
  fn route_node(node) -> KernelKind;
  fn local_relay_to_singbox(...); // F1
}
```

### 4.6 进程级（Windows）

```text
trait ProcessProxyEngine {
  fn attach(exe, opts: { include_tree: true }) -> Result<()>; // O1 默认树
  fn detach(exe);
  // 注入流量 → 本地入站 → 指定 Chain；与 TUN 互斥按 N1
}
```

### 4.7 前端 IPC（Tauri commands 示例）

- `nodes/list` · `bindings/bind` · `bindings/unbind`
- `chains/save` · `chains/test` · `exit_ip/detect`
- `plugins/install` · `plugins/effective_config`
- `settings/performance_preset` · `update/check`

---

## 5. 子 Agent / Codex 调用规范

### 5.1 角色分工

| 角色 | 谁 | 做什么 |
|------|-----|--------|
| **协调者** | 本助手（pidan / Grok Bot） | 拆任务、定验收、汇总决策、对用户说话、发起/跟进 Cloud Agent |
| **实现者** | Cursor **Cloud Agent** | 在远程环境改仓库、开 PR、跑测试 |
| **Codex / 其它编码 Agent** | 仅当用户明确要求，或 Cloud Agent 不可用时的备选 | 同一套「问题+验收」提示词；仍由协调者写 brief |
| **浏览器/桌面** | computerUse | 只做 GUI/站点操作，不改仓库 |
| **本地 executor** | 脚本、下载、文档、非仓库杂活 | 不 clone 大仓库做主开发 |

### 5.2 何时必须丢给 Cloud Agent

- 实现功能、修 bug、重构、查清仓库真实行为（非「读一个远程文件」这种窄查询）
- **禁止**在 box / 用户电脑上 clone 仓库做主开发（除非用户明确要求本地 clone）

### 5.3 发起任务的写法（给 Cloud Agent / Codex 通用 brief）

每条任务必须包含：

1. **仓库**：`https://github.com/<you>/clash5`（或当前 fork URL）
2. **背景**：一句话 + 指向本手册约束编号
3. **目标**：可验证结果（不是实现步骤清单）
4. **约束**：从第 2 节抄相关硬约束；标明里程碑（如 M0.2）
5. **验收**：命令或检查项（`cargo test …`、手动步骤）
6. **非目标**：本次不要做的事（防范围膨胀）
7. **假设**：标成 *hypothesis*，允许 Agent 否定

**模板：**

```text
Repo: https://github.com/<owner>/clash5
Milestone: M0.2
Goal: Implement stable_id + SQLite BindingStore with orphan reattach on provider refresh.
Hard constraints: handbook §2 items 1–2, 9; decisions A2, B off, C2.
Out of scope: UI polish, sing-box, injection.
Done when:
- unit tests for stable_id stability and orphan reattach pass
- refreshing a fixture provider marks removed nodes orphan and restores bindings when they return
Hypothesis (verify): existing profile module is the right insertion point — change if wrong.
Open a PR against dev; do not force-push main.
```

### 5.4 协调者调用步骤（本助手执行时）

1. 用 `CloudAgent` **launch**（已有仓库填 `repo`；绿地上新仓才 `new_repo`）。
2. 先向用户发一条短确认 + **cursor-agent 卡片**。
3. **不轮询**；完成唤醒后再汇报 PR 链接与摘要。
4. 同一工作流用 **reply** 续作，不新开第二个 Agent 抢同一 PR。
5. 需要纠偏用 steer；用户说停再用 interrupt。

### 5.5 若用户指定「用 Codex」

- 把同一份 brief 交给 Codex（或用户本机 Codex），**验收标准不变**。
- 仍遵守：不绕过 GPL、不提交密钥、不把注入当默认开启。
- Codex 产出 PR/补丁后，协调者负责对照手册做一次约束检查再合并建议。

### 5.6 子任务粒度建议

| 太大 | 合适 |
|------|------|
| 「做完整个插件系统」 | 「M3.1：PatchMerger + 单测 + 热更新回滚」 |
| 「重做全部 UI」 | 「节点详情生效配置表 + getEffectiveConfig IPC」 |

单次 Cloud Agent：**一个里程碑子项或一个垂直切片**，预计 1 个 PR 能审完。

### 5.7 本周可发的第一批 Agent 任务（可直接复制）

**CA-1** Fork 基建：改名 clash5 占位、CI T2 骨架、upstream merge 文档。
**CA-2** `StableId` + `BindingStore` + provider diff/orphan。
**CA-3** 空壳「节点池」路由 + i18n 词条占位（无业务逻辑也可）。

---

## 6. 决策速查（已锁定）

| 代号 | 结论 |
|------|------|
| A2 | 消失节点绑定变孤儿，恢复后重绑 |
| B | 名称匹配默认关 |
| C2 | 手动/订阅逻辑隔离 |
| D2 | 热更失败回滚并通知 |
| E3 | sing-box 默认可退出，可改常驻 |
| F1 | sing-box 走本地中转 |
| G3 | 敏感权限分级确认 |
| H | WASM 32MB/3s |
| I1 | 第三方仅 WASM |
| J2 | 死连接 120s/30s |
| K2 | 内核 10min 最多重启 3 次 |
| L2 | 出口 IP 失败默认告警，可自动切 |
| M1 | 注入默认关 |
| N1 | 注入优先于 TUN |
| O1 | 默认进程树 |
| P2 | 探测并发 16 / interval 600s |
| Q3 | 高内存默认自动缓解 |
| R1 | 三档性能预设 |
| S2 | merge 同步上游 |
| T2 | 标准 CI |
| U2 | Release + 应用内检查更新 |
| V1 | 语言跟随系统 |
| W1 | 测试通道默认关 |
| 品牌 | **clash5** |

---

*维护：重大决策变更时同步改本文件与 `M0-M6-任务拆解.md`。*


## 附录：Opus 任务清单

详见同目录 `M0-M6-任务拆解.md`。凡标 `[Opus]` 的条目，Cloud Agent / Codex 必须使用最强编码模型；brief 首行写 `Model requirement: Opus-class`。

**Opus 批次**：CA-2, CA-4, CA-5, CA-6, CA-8, CA-9（及 M3 后端、M0.2 核心）。
**标准批次**：CA-1, CA-3, CA-7, CA-10。
