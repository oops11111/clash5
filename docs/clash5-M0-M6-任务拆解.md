# clash5 开发任务拆解（M0–M6）

> 基于 [`clash5-工程手册.md`](clash5-工程手册.md) r2 修订稿（待冻结）。后续上游同步拟按 release tag 的独立 merge PR（S2 r2）；产品名 **clash5**。
> **模型标注**：`[Opus]` = 架构/高风险/复杂并发与安全，使用 Opus-class 或同级高推理模型；`[标准]` = 常规编码模型；`[人工]` = 需账号、设备或真机验收。
> **进度口径**：原版 M0 已按验收合并至 `dev`；r2 的新增要求仍逐项补齐。M1 节点池及九入口导航已通过桌面验收并分别合并（PR #4、#16）；M0.2 身份恢复已由 PR #23 合并。M2 实现及 Windows Server 2022 原生验收已通过，PR #18 正在同步 `dev`，尚未合并。

---

## 模型选用总则

| 标签 | 何时用 |
|------|--------|
| **[Opus]** | 跨模块设计落地、插件沙箱/权限、冲突合并、双内核调度、注入引擎、热更新回滚、数据模型与订阅 diff |
| **[标准]** | UI 页面、i18n、CI yaml、文档、简单 IPC 接线、样式 |
| **[人工]** | GitHub fork 授权、发布证书、杀软加白、真机注入验收 |

**调用方式**：`[Opus]` brief 明确 `Model requirement: Opus-class`；具体工具按项目负责人指定和可用性选择。协作格式见 [`clash5-AI协作规范.md`](clash5-AI协作规范.md)。

---

## M0 · Fork 基建与数据骨架

### M0.1 仓库与工程
- [x] `[人工]` Fork 上游并建立 `oops11111/clash5` 仓库
- [x] `[标准]` 原版 M0 品牌占位、README、上游同步说明
- [x] `[标准]` 原版 M0 CI 与 shell smoke 基础检查；r2 矩阵扩展见下方差距

### M0.2 稳定身份与存储
- [x] `[Opus]` 原版 stable_id 指纹与凭证归一化、测试；r2 HMAC 与版本迁移另列
- [x] `[Opus]` 原版 SQLite bindings / chains / quality / audit / orphan，手动与订阅隔离（C2）
- [x] `[Opus]` 原版订阅 diff：同身份继承 / 新增 / orphan 恢复（A2），名称匹配默认关（B）
- [ ] `[标准]` 导出/导入包
- [x] `[标准]` 孤儿保留期配置、清理预览、确认与到期清理（PR #23 已合并；CI 与 Windows 桌面恢复/持久化验收通过）

### M0.3 验收
- [x] `[标准]` 原版 M0 空壳与数据层验收，已合并至 `dev`

### M0 r2 增量差距（不能按已完成项勾选）
- [x] `[Opus]` R-01/R-02/R-25：按[身份迁移决定](clash5/r2-identity-migration.md) 完成本机密钥 HMAC、版本/代次迁移、密钥丢失恢复、同来源唯一候选重绑和多候选确认（PR #23 已合并；数据、Clippy 与 Windows 桌面验收通过）
- [x] `[Opus]` SQLite WAL 与版本迁移：schema v4 逐级迁移，孤儿默认保留 30 天、可设 1–365 天，并提供清理预览、确认和到期清理；迁移回滚与保留期数据测试、PR #23 CI 和 Windows 桌面验收均通过，已合并
- [ ] `[标准]` R-18：服务/包名隔离及同机安装、启动、卸载共存验收（PR #27 的 CI/安装器隔离检查通过；完整同机共存验收与合并仍待完成）
- [x] `[标准]` R-19：只读导入 Verge Rev 配置并验证源目录导入前后 SHA-256 一致（PR #26 已合并，fixture 与 Windows 桌面验收通过）
- [ ] `[Opus]` R-20：统一日志/诊断脱敏；凭证、完整订阅 URL、controller secret、relay token 的泄露回归测试
- [ ] `[标准]` 对外 `C5-*` 错误码、IPC 契约快照与 r2 CI 矩阵证据
- [ ] `[标准]` 首次 release-tag 同步前核对 M0 所用上游 `dev` 基线，单独 PR 记录取舍

**本阶段 Opus 必做**：M0.2 全部核心逻辑。

---

## M1 · 节点池与探测

- [x] `[Opus]` 订阅/规则解耦数据流（provider 只更线路）
- [x] `[标准]` 节点池 UI、剪贴板导入交互、过滤角标
- [x] `[Opus]` 去重指纹 + 短延迟/长握手探测管线与节流（P2）
- [x] `[标准]` 探测结果展示着色/排序
- [x] `[Opus]` 手动节点加入运行时后可安全探测；限流自动退避及 fixture 验证（R-11）
- [x] `[标准]` 对照工作区外层 `src/ui-hifi` 的已确认 UI 做桌面验收，提供 r2 所需的集成测试与演示步骤

**验收结果**：PR #4 的 SQLite 库存、剪贴板导入、订阅刷新与规则保留、手动节点运行时映射、外部 provider 探测、最多 16 并发、600 秒缓存和连续失败退避已通过数据层与真实 Mihomo fixture 测试。[Windows Server 2022 原生验收](https://github.com/oops11111/clash5/actions/runs/38647386824)通过 Tauri 页面导入、独立节点与 429 代理探测、缓存、来源/状态筛选、搜索、质量排序，以及桌面重启后的稳定身份检查；截图与范围见 [`M1 桌面演示步骤`](clash5/m1-acceptance.md)。M1 已通过 PR #4 合并至 `dev`。Windows Server 2025 的 WebDriver 问题仍由[上游 runner 问题](https://github.com/actions/runner-images/issues/14738)追踪。Mihomo 延迟接口不提供上游 HTTP 状态或 `Retry-After`，所以限流缓解是连续失败通用退避，风险 R-11 仍需观察。

---

## M2 · 链路编排

- [x] `[Opus]` `dialer-proxy` 生成、override、与 provider 协同
- [x] `[标准]` 链路编排画布 UI、工具栏
- [x] `[Opus]` 出口 IP 检测与 L2 告警/可选切换策略
- [x] `[标准]` 链路详情三栏布局壳

**验收结果**：PR #18 的两跳生成、provider override 与原配置保留、保存/测试/应用、出口 IP 检测、默认告警、经验证的备用切换、失败回滚及旧命令保护已通过 fixture 与 Windows Server 2022 桌面验收。[原生验收](https://github.com/oops11111/clash5/actions/runs/38701837292)通过默认 940 × 700 窗口的页面无横向溢出检查、Tauri 桥接与桌面重启后的链路恢复。M2 代码与桌面验收已完成，待 PR #18 合并。插件输出和绑定仍属于 M3。

---

## M3 · 插件系统

- [ ] `[Opus]` Plugin Host、WASM 沙箱、权限 G3、Manifest/签名校验（I1/H）
- [ ] `[Opus]` Patch 合并引擎 + 热更新失败回滚通知（D2）
- [ ] `[Opus]` 节点/链路绑定模型与优先级冲突表
- [ ] `[标准]` 插件中心 / 绑定弹窗 / 批量绑定 UI
- [ ] `[Opus]` `getEffectiveConfig` + 来源链数据 API
- [ ] `[标准]` 生效配置表、来源链弹窗、冲突徽章 UI
- [ ] `[标准]` 示例插件文档；`[Opus]` 示例 WASM 钩子实现

**本阶段几乎整段后端标 Opus；UI 标准。**

---

## M4 · sing-box 侧车

- [ ] `[Opus]` 懒启动/退出（E3）、端口与 DNS 隔离、F1 中转
- [ ] `[Opus]` 配置映射 detour、调度与降级
- [ ] `[Opus]` 死连接 J2、内核重启 K2
- [ ] `[标准]` 设置项「常驻/自动退出」、状态展示

**Opus 必做**：双内核生命周期与中转正确性。

---

## M5 · Windows 注入 L6b

- [ ] `[Opus]` 注入引擎架构、进程树 O1、与 TUN 互斥 N1、默认关 M1
- [ ] `[Opus]` 流量进本地入站再绑 Chain/插件
- [ ] `[标准]` 按需开启 UI、非 Windows 降级文案
- [ ] `[人工]` 管理员权限、杀软、真机进程树验收

**Opus 必做**：全部注入核心；验收人工。

---

## M6 · 体验与发版

- [ ] `[标准]` 玻璃主题/换肤、i18n V1、性能三档 R1
- [ ] `[Opus]` 高内存自动缓解策略接线（Q3）— 策略逻辑用 Opus，UI 标准
- [ ] `[标准]` Release + 应用内更新检查（U2/W1）
- [ ] `[人工]` 签名/公证（若发正式安装包）

---

## 建议 Agent 批次（可直接开）

| ID | 内容 | 模型 |
|----|------|------|
| **CA-1** | Fork 后 CI + 品牌占位 | 标准 |
| **CA-2** | stable_id + BindingStore + orphan diff | **Opus** |
| **CA-3** | 节点池空壳路由 + i18n | 标准 |
| **CA-4** | 订阅解耦 + 探测管线 | **Opus** |
| **CA-5** | dialer-proxy + 出口 IP | **Opus** |
| **CA-6** | Plugin Host + PatchMerger + 回滚 | **Opus** |
| **CA-7** | 插件/生效配置 UI | 标准 |
| **CA-8** | sing-box 侧车 | **Opus** |
| **CA-9** | Windows 注入 | **Opus** |
| **CA-10** | 发版与主题收口 | 标准为主 |

Brief 首行示例（Opus 任务）：

```text
Model requirement: Opus-class / strongest coding model.
Repo: <fork-url>
Milestone: M0.2
…
```

