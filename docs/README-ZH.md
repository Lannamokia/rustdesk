<p align="center">
  <img src="../res/logo-header.svg" alt="RustDesk - Your remote desktop"><br>
  <b>RustDesk &mdash; <code>Lannamokia</code> fork（VHD 机器认证桥接版）</b><br>
  <a href="#fork-状态">Fork 状态</a> &bull;
  <a href="#本-fork-新增了什么">新增内容</a> &bull;
  <a href="#编译">编译</a> &bull;
  <a href="#密钥与-ci">密钥与 CI</a> &bull;
  <a href="#许可证与署名">许可证</a><br>
  [<a href="../README.md">English</a>] | [<a href="README-UA.md">Українська</a>] | [<a href="README-CS.md">česky</a>] | [<a href="README-HU.md">Magyar</a>] | [<a href="README-ES.md">Español</a>] | [<a href="README-FA.md">فارسی</a>] | [<a href="README-FR.md">Français</a>] | [<a href="README-DE.md">Deutsch</a>] | [<a href="README-PL.md">Polski</a>] | [<a href="README-ID.md">Indonesian</a>] | [<a href="README-FI.md">Suomi</a>] | [<a href="README-ML.md">മലയാളം</a>] | [<a href="README-JP.md">日本語</a>] | [<a href="README-NL.md">Nederlands</a>] | [<a href="README-IT.md">Italiano</a>] | [<a href="README-RU.md">Русский</a>] | [<a href="README-PTBR.md">Português (Brasil)</a>] | [<a href="README-EO.md">Esperanto</a>] | [<a href="README-KR.md">한국어</a>] | [<a href="README-AR.md">العربي</a>] | [<a href="README-VN.md">Tiếng Việt</a>] | [<a href="README-DA.md">Dansk</a>] | [<a href="README-GR.md">Ελληνικά</a>] | [<a href="README-TR.md">Türkçe</a>] | [<a href="README-NO.md">Norsk</a>] | [<a href="README-RO.md">Română</a>]
</p>

> [!Important]
> 本仓库是 [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk) 的下游 fork。完整英文文档见 [`../README.md`](../README.md)。
> 上游著作权、商标及 AGPL-3.0 许可证保持不变 &mdash; 详见[许可证与署名](#许可证与署名)。

> [!Caution]
> **免责声明：** 上游 RustDesk 的开发者及本 fork 的维护者不纵容或支持任何不道德或非法的软件使用行为。未经授权的访问、控制或侵犯隐私等滥用行为严格违反使用准则。作者对应用程序的任何滥用行为概不负责。

---

## Fork 状态

| | |
|---|---|
| **上游** | [`rustdesk/rustdesk`](https://github.com/rustdesk/rustdesk)（在 git 中作为 `upstream` remote） |
| **本 fork** | [`Lannamokia/rustdesk`](https://github.com/Lannamokia/rustdesk) |
| **活跃分支** | `feature/vhd-machine-auth-bridge` |
| **子模块** | `libs/hbb_common` &rarr; [`Lannamokia/hbb_common`](https://github.com/Lannamokia/hbb_common)，同名分支 |
| **许可证** | AGPL-3.0（与上游一致，见 [`LICENCE`](../LICENCE)） |
| **目标** | 让 RustDesk 受控端作为外部 VHDMount 代理的 sidecar，通过经过认证、机器绑定的桥接信道协同工作。 |

当 `vhd-bridge` 功能**关闭**时，构建产物在行为上与上游 RustDesk 完全等价 &mdash; 该约束由 `tests/feature_off_parity.rs` 自动验证。

## 本 fork 新增了什么

引入了一个内聚的子系统 &mdash; **VHD 机器认证桥接（VHD machine-auth bridge）**，由两个**默认关闭**的 Cargo features 控制：

- **`vhd-bridge`** &mdash; 编入桥接 worker、IPC 通道、维护遮罩 UI 与 smoke 测试。
- **`controlled-only`** &mdash; 剥离主控端（initiator）UI 与代码路径，使产物仅能被控制；与 `vhd-bridge` 组合用于生产 sidecar 构建。

未启用任一 feature 时，原有 `cargo run` 与上游构建流程完全不变。

### 主要改动

- **`src/vhd_bridge/`** &ndash; 命名管道 worker，状态机 `Identify &rarr; Authenticate &rarr; PeerSet &rarr; Heartbeat &rarr; Approval`，HMAC-SHA256 用编译期注入的 32 字节共享密钥；含重连退避、结构化可观测性、对密钥脱敏的日志接收器。
- **`src/server/connection.rs`** &ndash; 接受连入对等方前先咨询桥接的机器认证 peer 集合的准入门。
- **`src/auth_2fa.rs`** &ndash; 桥接介入认证时强制禁用 2FA（由 `tests/smoke_2fa_disabled.rs` 验证）。
- **`flutter/lib/desktop/widgets/maintenance_overlay.dart`** &ndash; 反映桥接状态（`active / starting / lost`）的维护遮罩 UI。
- **`libs/build_support/`** &ndash; 由 `build.rs` 与 CI 共用的辅助 crate：包含严格的前置变量校验门、`secret.sec` 的容错解析器、与协议文档一致性检查。
- **`docs/vhd-rustdesk-bridge-protocol.md`** &ndash; 线协议参考文档。
- **`scripts/check_bridge_strings.ps1`** &ndash; 构建后泄漏扫描器，确保 `HBBS Key` / `VHDMount Key` 明文不会进入产物。
- **`.github/workflows/vhd-bridge.yml`** &ndash; 编译 feature-on / feature-off / controlled-only 三个 Windows flavour 的 CI 矩阵。

完整设计文档见 [`.kiro/specs/vhd-machine-auth-bridge/`](../.kiro/specs/vhd-machine-auth-bridge)。

## 克隆

本 fork 修改了 `libs/hbb_common` 子模块的 URL，需使用递归克隆：

```sh
git clone --recursive https://github.com/Lannamokia/rustdesk.git
cd rustdesk
git checkout feature/vhd-machine-auth-bridge
git submodule update --init --recursive
```

如果你之前用上游 `.gitmodules` 克隆过，执行 `git submodule sync && git submodule update --init --recursive`。

## 编译

### 上游构建（不启用桥接）

不启用任一 feature 时，本 fork 是上游的严格超集，**直接套用上游构建说明**即可。完整依赖与命令请见 [`../README.md`](../README.md)。

### 启用桥接（Windows MSVC，推荐）

桥接当前仅支持 Windows（命名管道传输与 VHDMount 代理的依赖所致）。

所需环境变量：

```text
VCPKG_ROOT             = C:\src\vcpkg
VCPKG_DEFAULT_TRIPLET  = x64-windows-static
VCPKGRS_DYNAMIC        = 0
LIBCLANG_PATH          = <LLVM\x64\bin 路径>
```

随后填好开发版 `secret.sec`（见[密钥与 CI](#密钥与-ci)）或将相关变量以环境变量方式传入，然后：

```sh
# 生产 sidecar 构建（启用桥接 + 剥离主控端）
cargo build --release --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc

# 仅启用桥接（保留主控端 UI 用于开发）
cargo build --features vhd-bridge --target x86_64-pc-windows-msvc
```

### 验证

```sh
cargo check --lib --features vhd-bridge,controlled-only --target x86_64-pc-windows-msvc
cargo test  -p rustdesk --lib   --features vhd-bridge,controlled-only
cargo test  --test smoke_2fa_disabled --features vhd-bridge,controlled-only
cargo test  --test feature_off_parity
cargo test  -p build_support
```

最近一次本分支结果：0 错误 / 189 单元测试通过 / 6 + 8 集成测试 / 38 + 4 build_support 测试。

## 密钥与 CI

桥接需要 5 个编译期输入：

| 变量 | 用途 | 格式 |
|---|---|---|
| `HBBS_KEY` | RustDesk rendezvous server 公钥（覆盖 `RS_PUB_KEY`） | base64，解码后 32 字节 |
| `HBBS_HOST` | rendezvous 服务器地址 | `host[:port[-port2]]` |
| `HBBR_HOST` | relay 服务器地址 | `host[:port]` |
| `VHD_BRIDGE_SECRET_HEX`（或 `_B64`） | 32 字节 HMAC 共享密钥 | 64 hex 字符 / 44 base64 字符 |
| `VHD_BRIDGE_SECRET_VERSION` | 单调递增的密钥轮换版本号 | 非负整数 |

两种供给方式：

1. **本地开发** &mdash; 在仓库根目录创建 `secret.sec`，写入 `HBBS Key:` / `HBBS Host:` / `HBBR Host:` / `VHDMount Key:` / `VHDMount Key Version:`。该文件已被 [`.gitignore`](../.gitignore) 忽略。
2. **CI** &mdash; 在 `Settings &rarr; Secrets and variables &rarr; Actions` 中以同名仓库密钥配置；[`.github/workflows/vhd-bridge.yml`](../.github/workflows/vhd-bridge.yml) 通过受掩码的环境变量注入。**`secret.sec` 不会被写入 CI runner**。

`secret.sec` 与 `vhd_bridge_secret.bin` 均已加入 `.gitignore`，**严禁提交**。`scripts/check_bridge_strings.ps1` 是构建后兜底扫描，确保产物中无明文密钥泄漏。

## 被控端部署

`controlled-windows` workflow 的产物 `rustdesk-controlled-windows-x86_64` 是用 `--features vhd-bridge,controlled-only` 编出来的独立 `rustdesk.exe`。它仍然支持上游 RustDesk 受控端的全部 CLI（`--service` / `--server` / `--install-service` / `--cm` / `--tray`），但**任何发起方子命令**（`--connect` / `--port-forward` 等）在启动时直接拒绝。

### 一次性安装（推荐）

在目标 Windows 机器上以**管理员身份**打开 PowerShell：

```powershell
# 1. 把二进制放到一个稳定路径（不要放 %TEMP%，也不要放含 `host=` / `licensed-`
#    子串的目录 —— 见 src/custom_server.rs，否则会被误解析为自定义服务端配置）。
$dst = "C:\Program Files\RustDeskControlled"
New-Item -ItemType Directory -Force -Path $dst | Out-Null
Copy-Item .\rustdesk-controlled-windows-x86_64\rustdesk.exe $dst\rustdesk.exe -Force

# 2. 注册 Windows 服务。这条命令会调用 platform::install_service() ——
#    与官方 MSI 安装器走完全相同的路径：服务名 `RustDesk`、ImagePath
#    `<dst>\rustdesk.exe --service`、启动类型 Automatic、崩溃自动重启。
& "$dst\rustdesk.exe" --install-service

# 3.（可选）确认服务已注册并已启动。
sc.exe query RustDesk
sc.exe qc RustDesk
```

装好之后，每次开机：

1. `services.exe` 以 **LocalSystem** 启动 `rustdesk.exe --service`。
2. `--service` 进程在当前活跃用户会话里 spawn `--server`（覆盖 UAC 提权画面、锁屏、用户切换场景）。
3. `--server` 进程持有：到 `VHDMount` 的命名管道客户端（如有）、到你 hbbs 的 rendezvous 连接、以及音频 / 视频 / 剪贴板 / 输入服务。

### 保活机制

服务**自带崩溃自动重启**（`install_service` 注册时就配好了 `failure` recovery actions）—— 不需要外部看门狗。如果想确认或加严策略：

```powershell
# 查看当前 recovery actions
sc.exe qfailure RustDesk
# 加严：前 24 小时内每次崩溃延迟 5 秒后重启
sc.exe failure RustDesk reset= 86400 actions= restart/5000/restart/5000/restart/5000
```

### 对接自托管 hbbs / hbbr

编译期注入（推荐方式，详见[密钥与 CI](#密钥与-ci)）会把 host / 端口 / 公钥写进二进制，新装机开箱即连。如果你部署的产物在打包时正确填了 `HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST` 三个 GitHub Actions Secret，被控端**不需要**任何手动设置。

如果需要把已经装好的实例指向另一个 rendezvous 服务器，可以编辑 `%APPDATA%\RustDesk\config\RustDesk2.toml`（当前活跃用户会话 `--server` 进程的 per-user 配置），或者通过 IPC 下发：

```powershell
& "C:\Program Files\RustDeskControlled\rustdesk.exe" --option custom-rendezvous-server <host:port>
sc.exe stop RustDesk
sc.exe start RustDesk
```

### 健康检查

```powershell
# 服务状态
sc.exe query RustDesk

# 实时日志 —— LocalSystem 模式下日志写在 %ProgramData%\RustDesk\log
Get-Content "$env:ProgramData\RustDesk\log\service.log" -Tail 50 -Wait

# 桥接状态（仅 vhd-bridge build 暴露，需要 VHDMount 在跑）
# 读取 `vhd-bridge-state` IPC 键 —— 错误码字典见 docs/vhd-rustdesk-bridge-protocol.md §11.3
& "C:\Program Files\RustDeskControlled\rustdesk.exe" --get-option vhd-bridge-state

# 由 hbbs 分配的当前 peer ID
& "C:\Program Files\RustDeskControlled\rustdesk.exe" --get-id
```

### 卸载

```powershell
& "C:\Program Files\RustDeskControlled\rustdesk.exe" --uninstall-service
Remove-Item -Recurse -Force "C:\Program Files\RustDeskControlled"
Remove-Item -Recurse -Force "$env:ProgramData\RustDesk" -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force "$env:APPDATA\RustDesk"     -ErrorAction SilentlyContinue
```

### 关于 `vhd-bridge` 命名管道对端

启用 `vhd-bridge` 特性时（`controlled-windows` 产物默认启用），`rustdesk.exe --server` 会去找 `VHDMounter.exe`（或任意 `VHDMounter_<tag>.exe` 分支变体 —— 例如 `VHDMounter_LE2025.exe`）监听的 `\\.\pipe\VHDMount.RustDeskBridge`（详见 [docs/vhd-rustdesk-bridge-protocol.md](vhd-rustdesk-bridge-protocol.md) §2.1）。如果接受集合内的任何 VHDMounter 都没有运行：

- 桥接 worker 停留在 `Bridge_State == Initializing` 并按退避重试（Requirement 13.2 / 13.3）。
- 入站连接**仍然可用** —— 走 §19.8 的「密码正确 = 允许」回退路径（无桥接侧批准）。
- `vhd-bridge-state` IPC 键报 `vhd.bridge.failed.peer_not_vhdmount`，监控可据此告警。

`VHDMount` 自身的配置 / 打包**不属于本仓库范围** —— 它独立分发。完整跨产品规范见 `machine-auth.md` 与 `.kiro/specs/vhd-machine-auth-bridge/`。

## 许可证与署名

本 fork 沿用上游 RustDesk 的许可证：**GNU Affero General Public License v3.0（AGPL-3.0）**。完整条款见 [`../LICENCE`](../LICENCE)，本 fork **不修改**许可证文本。

- 上游 RustDesk 代码的著作权归上游 RustDesk 作者及贡献者所有，参见 <https://github.com/rustdesk/rustdesk>。
- 本 fork 引入的修改（`vhd-bridge` / `controlled-only` 两个 feature 及其支持代码）同样以 AGPL-3.0 发布；下游用户保留 AGPL-3.0 授予的全部权利，包括对网络部署索取对应源代码的权利。
- "RustDesk" 名称及 logo 归上游项目所有，本 fork 仅用于标识被修改的代码基础，符合自由软件 fork 项目对商标的合理使用惯例。
- 通过 vcpkg 引入的第三方库（`libvpx`、`libyuv`、`opus`、`aom`）以及 Sciter SDK、Flutter 依赖等，各自保留原始许可证。

使用本 fork 即表示同意 AGPL-3.0 条款及顶部的**免责声明**。
