# 分软件代理：真机验收前置条件

本文列出「attach / 改链 / TUN 互斥 / 杀进程恢复」这四项验收所依赖的环境条件，以及当前代码侧仍缺的东西。目的是让验收从"装不装得上"变成可执行的清单，而不是反复试。

结论先说：**这台机器目前不满足条件。服务身份冲突已通过改名消除，剩下的唯一硬缺口是 WFP 驱动（构建、签名、打包、安装均未完成）。**

## 一、attach 需要两层门槛，缺一不可

`clash5_attach_process_proxy` 的实现路径是：

```
attach(chain_id, pid)
  → service::active_service_session()      ← 门槛 1：必须已有 owner 会话
  → ChainListener + TcpRedirectRelay       ← 本地回环中继
  → service: attach_process_proxy(IOCTL)   ← 门槛 2：服务经 WFP 驱动重定向
```

### 门槛 1：owner 会话

`ACTIVE_SERVICE_SESSION` 只在**应用通过服务启动内核**时写入（`src-tauri/src/core/service.rs`）。也就是说：

- 运行模式必须是**服务模式**，且服务是 **clash5 自己的** `clash5-service.exe`；
- 若回落到 sidecar（用户模式），该会话永远为空，`attach` 直接返回 `C5-PROXY-005: service owner session is not active`。

**这台机器的实际情况**：`clash_verge_service` 已注册并在运行，但 `ImagePath` 指向
`C:\Program Files\Clash Verge\resources\clash-verge-service.exe` —— 那是**上游 Clash Verge 的服务**，不是 clash5 的。

改名之前两边都叫 `clash_verge_service`，`Get-Service` 看起来"有服务"，实际握手对不上。现在 clash5 注册为 `clash5_service`，两个产品可以共存，不再抢同一个名字；但这台机器上只有上游的服务，clash5 自己的服务还没装，所以仍然回落到 sidecar。

判定方法：`Get-Service clash5_service` 存在且 `ImagePath` 指向 clash5 安装目录下的 `clash5-service.exe`，才算通过门槛 1。

### 门槛 2：WFP 驱动

`drivers/windows/wfp/README.md` 明确写着：

> This is an unintegrated kernel component for M5, not an installable driver. … The application does not package, load, or invoke it.

即：驱动能编译出 `clash5wfp.sys`，但**仓库不打包、不加载、不调用它**；且 "Release driver signing, packaging, and installation are still pending"。

本机 `C:\Windows\System32\drivers\` 下无 `clash5wfp.sys`，也无 `clash5wfp` 内核服务。

服务侧的策略下发**已经实现**：`clash5-service-ipc` 的 `src/core/process_proxy/wfp.rs` 会创建 provider、sublayer、callout 与 filter，并在会话结束时回收。所以缺的不是策略代码，而是驱动本身能不能被签名、打包、装上。

## 二、前置条件清单（按顺序，均需管理员）

> 以下命令**未在本机执行**。全部需要管理员权限；建议在专用验收机上做，因为第 2、4 步会改动系统启动配置并装载内核驱动。

### 1. 准备驱动构建工具链

`build.ps1` 要求 WDK/SDK CPP `10.0.26100.1`、NuGet 包 `Microsoft.Windows.WDK.x64 10.0.26100.6584`、VS2022 x64 MSVC。

```powershell
cd drivers\windows\wfp
.\build.ps1 -PackagesRoot <NuGet packages 目录> `
            -MsvcRoot <VC\Tools\MSVC\<version>> `
            -OutputDirectory <输出目录>
```

预期：产出 `clash5wfp.sys`（未签名），无编译告警（脚本已开 warnings-as-errors + MSVC analyze）。

### 2. 测试签名并开启测试模式（仅实验环境）

正式发布需要 EV 证书 + 微软 attestation 签名；实验机上用测试证书即可：

```powershell
# 以管理员运行
New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=clash5-wfp-test" `
  -CertStoreLocation Cert:\LocalMachine\My
# 用该证书签名 clash5wfp.sys，并把证书装入 受信任的根证书颁发机构 与 受信任的发布者
signtool sign /v /s My /n "clash5-wfp-test" /fd SHA256 <输出目录>\clash5wfp.sys
bcdedit /set testsigning on
# 重启使 testsigning 生效
```

预期：重启后 `bcdedit /enum {current}` 显示 `testsigning Yes`，桌面右下角出现"测试模式"水印。

### 3. 安装 clash5 自己的服务（不需要动上游服务）

改名后 clash5 注册为 `clash5_service`，与上游 Clash Verge 的 `clash_verge_service` 互不冲突，两者可以共存。

```powershell
# 以管理员运行
& "<应用目录>\resources\clash5-service-install.exe"
sc.exe qc clash5_service
```

预期：`SERVICE_NAME: clash5_service`，`BINARY_PATH_NAME` 指向安装目录下的 `clash5-service.exe`。

> 安装器会顺带清理**旧命名**的注册，但只在旧服务的 `ImagePath` 指向 `<安装目录>\clash-verge-service.exe`（即确实是我们自己的旧安装）时才 stop + delete；指向别处的（例如上游 Clash Verge）一律不动。这一步不再需要手工 `sc.exe delete`。

### 4. 注册并启动驱动

```powershell
Copy-Item <输出目录>\clash5wfp.sys C:\Windows\System32\drivers\clash5wfp.sys
sc.exe create clash5wfp type= kernel start= demand binPath= C:\Windows\System32\drivers\clash5wfp.sys
sc.exe start clash5wfp
sc.exe query clash5wfp
```

预期：`STATE` 为 `RUNNING`。

> 注意：驱动只提供重定向能力，WFP provider/callout/filter 由服务创建（已实现）。驱动起来但服务未建立会话时，不会拦截任何流量——这是预期行为，不要据此判定驱动失效。

### 5. 让应用以服务模式启动

启动 clash5，确认运行模式为**服务模式**（不是 sidecar）。判定方法：`attach` 不再返回 `C5-PROXY-005`。

### 6. 就绪自检

```powershell
powershell -File check-readiness.ps1 -AppDir <应用目录> -RepoRoot <仓库目录>
```

全部 PASS 后再进入运行时验收（attach → 改链 → 开 TUN → 杀进程恢复）。

## 三、即使环境齐备，这些代码缺口仍可能导致验收失败

摘自 `drivers/windows/wfp/README.md`（原文列举的 M5 前必须完成项）：

| 缺口 | 影响 |
|---|---|
| filter 生命周期 | 服务未创建/清理 WFP provider、callout、filter，重定向不生效 |
| relay record handling | 中继连接记录不完整 |
| TUN exclusion | TUN 与分软件代理共存时无排除逻辑 |
| crash recovery（服务看门狗） | 服务崩溃后残留 filter 无人清理，状态会停在 `recovery_required` |
| UDP | 当前仅 TCP 被重定向，UDP 流量直连 |
| target discovery | 驱动不发现子进程；进程树由应用侧 `process_tree.rs` 提供 |

因此本次验收里「attach 成功」「改链生效」「杀进程恢复」这三项，**在补齐驱动集成之前无法判定为通过**，只能判定为"被环境与代码共同阻塞"。

## 四、当前验收可以做到哪一步

已完成（`outputs/perapp-acceptance-20261116/`）：

- 应用构建、首页与分软件代理卡片渲染、空态与有数据态区分 —— 通过
- 进程枚举（221/221 拿到镜像路径）、配置读写 —— 通过
- `status` / `attach` / `detach` 的失败路径返回正确错误码且不 panic —— 通过

被阻塞：attach 成功路径、改链交互、TUN 互斥、杀进程恢复。
