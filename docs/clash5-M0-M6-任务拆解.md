# clash5 开发任务拆解（M0–M6）

> 基于已锁定决策。上游 merge（S2）。产品名 **clash5**。
> **模型标注**：`[Opus]` = 架构/高风险/复杂并发与安全，须用 Opus（或同级）Cloud Agent；`[标准]` = Composer/标准编码模型即可；`[人工]` = 需你本机或账号操作。

---

## 模型选用总则

| 标签 | 何时用 |
|------|--------|
| **[Opus]** | 跨模块设计落地、插件沙箱/权限、冲突合并、双内核调度、注入引擎、热更新回滚、数据模型与订阅 diff |
| **[标准]** | UI 页面、i18n、CI yaml、文档、简单 IPC 接线、样式 |
| **[人工]** | GitHub fork 授权、代码签名、杀软加白、真机注入验收 |

**调用方式**：`[Opus]` 任务在 Cloud Agent launch 的 prompt 首行写：`Model requirement: use the strongest available coding model (Opus-class).`
Codex 同理：选 GPT‑5.2 / o 系列高推理档，勿用轻量档硬扛架构题。

---

## M0 · Fork 基建与数据骨架

### M0.1 仓库与工程
- [ ] `[人工]` Fork 上游并建 `clash5` 远程仓库，把地址发给协调者
- [ ] `[标准]` 品牌占位、README、upstream merge 说明（S2）
- [ ] `[标准]` CI T2：lint + typecheck + cargo test + 快照；nightly 工件

### M0.2 稳定身份与存储
- [ ] `[Opus]` `stable_id` 指纹与凭证归一化设计+实现+单测
- [ ] `[Opus]` SQLite：bindings / chains / quality / audit / orphan；手动 vs 订阅（C2）
- [ ] `[Opus]` 订阅 diff：继承 / 新增 / orphan 重绑（A2）；名称匹配默认关（B）
- [ ] `[标准]` 导出/导入包、孤儿清理入口（可跟 Opus PR 同开或二期）

### M0.3 验收
- [ ] `[标准]` 空壳可运行 + 单测绿

**本阶段 Opus 必做**：M0.2 全部核心逻辑。

---

## M1 · 节点池与探测

- [ ] `[Opus]` 订阅/规则解耦数据流（provider 只更线路）
- [ ] `[标准]` 节点池 UI、剪贴板导入交互、过滤角标
- [ ] `[Opus]` 去重指纹 + 短延迟/长握手探测管线与节流（P2）
- [ ] `[标准]` 探测结果展示着色/排序

**Opus 必做**：解耦后端、去重与探测管线。

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

