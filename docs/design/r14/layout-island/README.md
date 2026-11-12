# layout-island · 横向五 Tab + 浮岛卡片

## 已定五标签（左→右）
**首页 · 代理 · 订阅 · 插件 · 设置**  
文案取自 `locales/zh/layout.json` → `layout.components.navigation.tabs.*`。
**不新造第六标签。**

## 源码事实（只读）
- 实施前 `src/pages/_navigation.tsx` 的 `navItems` 为九个独立入口；实施后固定为五个主入口。
- `_navigation-meta.ts` 另定义 connections / logs 文案键，但不在 navItems 中

## 合并归属（已写入决策）
| 原入口 | 并入 | 说明 |
|---|---|---|
| 节点 (`nodePool`) | **订阅** | 与 U2 A 左来源右节点表一致：订阅页管来源+节点 |
| 规则 (`rules`) | **代理**（页内可达） | 代理页内保留规则集/进程路由入口；落实 2A 规则页能力不丢 |
| 链路 (`chains`) | **首页** | 首页浮岛展示当前链路 / 新建入口 |
| 诊断 (`diagnostics`) | **设置 → 诊断** | 设置内诊断分区 |
| 连接 (`connections`) | **设置 → 诊断** | 与诊断同区 Tab/子页 |
| 日志 (`logs`) | **设置 → 诊断** | 同上 |
| 测试/解锁 (`unlock`) | **设置 → 诊断**（首页 DNS 卡片可深链） | U1 首页保留 DNS 泄漏检测；解锁走诊断 |

## 布局
标题栏 → 其下横向五 Tab → 主内容区（无左侧纵栏主导）。内容卡片：细边框 + 阴影浮岛。

## 本目录
- `0x-*.svg/png`：线框草稿
- `renders/`：浅色 + 暗色高保真 PNG
- `src/`：渲染用 HTML

全局确认、错误、空、骨架、按钮加载与焦点状态参考见 `../interaction-states/`。
