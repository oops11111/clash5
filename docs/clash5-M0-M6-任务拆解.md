# clash5 开发任务拆解（M0–M6）

> 基于 [`clash5-工程手册.md`](clash5-工程手册.md) r2 修订稿（待冻结）。后续上游同步拟按 release tag 的独立 merge PR（S2 r2）；产品名 **clash5**。
> **模型标注**：`[Opus]` = 架构/高风险/复杂并发与安全，使用 Opus-class 或同级高推理模型；`[标准]` = 常规编码模型；`[人工]` = 需账号、设备或真机验收。
> **进度口径**：M0 已按原版验收并合并至 `dev`；r2 的新增要求是开放差距。M1 已启动，草稿 PR #4 尚未达到退出标准。

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
- [ ] `[标准]` 导出/导入包、孤儿清理入口及 r2 保留期提醒

### M0.3 验收
- [x] `[标准]` 原版 M0 空壳与数据层验收，已合并至 `dev`

### M0 r2 增量差距（不能按已完成项勾选）
- [ ] `[Opus]` R-01/R-02/R-25：按[已合并的身份迁移决定](clash5/r2-identity-migration.md) 无损迁移至本机密钥 HMAC 并存储版本与密钥代次；密钥丢失须显式重绑，凭证轮换仅在同来源按唯一候选自动重绑，多候选人工确认，附迁移 fixture
- [ ] `[Opus]` SQLite WAL 与版本迁移方案：从现有 `PRAGMA user_version` v2 安全迁移，迁移失败整事务回滚；孤儿默认 30 天、可设 1–365 天与清理前提示
- [ ] `[标准]` R-18/R-19：服务与包名隔离、同机共存实测；只读导入 verge-rev 数据并比较源目录前后校验和
- [ ] `[Opus]` R-20：统一日志/诊断脱敏；凭证、完整订阅 URL、controller secret、relay token 的泄露回归测试
- [ ] `[标准]` 对外 `C5-*` 错误码、IPC 契约快照与 r2 CI 矩阵证据
- [ ] `[标准]` 首次 release-tag 同步前核对 M0 所用上游 `dev` 基线，单独 PR 记录取舍

**本阶段 Opus 必做**：M0.2 全部核心逻辑。

---

## M1 · 节点池与探测

- [ ] `[Opus]` 订阅/规则解耦数据流（provider 只更线路）
- [ ] `[标准]` 节点池 UI、剪贴板导入交互、过滤角标
- [ ] `[Opus]` 去重指纹 + 短延迟/长握手探测管线与节流（P2）
- [ ] `[标准]` 探测结果展示着色/排序
- [ ] `[Opus]` 手动节点加入运行时后可安全探测；限流自动退避及 fixture 验证（R-11）
- [ ] `[标准]` 对照 `src/ui-hifi` 的已确认 UI 做桌面验收，提供 r2 所需的集成测试与演示步骤

**当前进度**：草稿 PR #4 已有 SQLite 库存、剪贴板链接导入、外部 provider 缓存、手动节点运行时映射、受限探测、连续失败退避和本地 HTTP 429 fixture。实际 Mihomo 二进制经该夹具的 HTTP/HTTPS 探测已在本机验证，CI 检查已接入；桌面壳的 Tauri 命令与 UI 端到端验收仍缺。当前延迟接口不提供上游 HTTP 状态或 `Retry-After`，因此只能按连续失败通用退避；M1 尚未达到退出标准。

---

## M2 · 链路编排

- [ ] `[Opus]` `dialer-proxy` 生成、override、与 provider 协同
- [ ] `[标准]` 链路编排画布 UI、工具栏
- [ ] `[Opus]` 出口 IP 检测与 L2 告警/可选切换策略
- [ ] `[标准]` 链路详情三栏布局壳

**Opus 必做**：链式配置正确性与出口 IP 策略。

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

