# Requirements Document

## Introduction

`vhd-machine-auth-bridge` 在 RustDesk 受控端进程中嵌入一条对接同机 `VHDMount` 主程序的本机桥接通道，把当前受控端的远控 ID 与登录密码定向上报给 `VHDMount`，再由后者使用其已经持有的 TPM RSA 私钥按 `machine-auth.md` 既有签名规范上送 `VHDSelectServer`。RustDesk 一侧因此**不再**承担机台身份认证、TPM 密钥管理、注册证书加载、服务端 URL 选择或任何 HTTPS 重试逻辑——这些职责全部由 `VHDMount` 主程序代为完成。

本仓库构建出的二进制可以以三种**部署角色**运行（沿用 RustDesk 现有架构）：

- `RustDesk_Controlled`（被控端）：仅在 Windows 上构建并部署，运行时持有远控 ID 与登录密码，等待被远控。本特性的全部桥接代码与被控端 UI 遮罩代码**只在该形态下编译启用**。
- `RustDesk_Controller`（主控端）：跨平台（Windows / macOS / Linux / Android / iOS），仅作为远控发起方，不持有受控状态。本特性**不在主控端启用任何代码**。
- `RustDesk_RelayServer`（转发 / 中继服务）：跨平台，仅承担辅助网络连接、不直接处理凭据。本特性**不在中继服务启用任何代码**。

本桥接模块的目标是：

1. 在 `RustDesk_Controlled` 进程内提供一个轻量的 Windows 命名管道客户端，握手到同机 `VHDMount` 主程序。
2. 通过编译期硬编码的 `RustDeskClientSharedSecret` 在握手阶段以 HMAC-SHA256 方式向 `VHDMount` 证明本进程是合法的 RustDesk 客户端，避免同机其它进程伪装上报凭据；密钥注入流程同时支持本地文件与 CI 环境变量两种来源。
3. 在远控 ID / 密码生成或轮换时，把 `(RustDesk_Remote_ID, RustDesk_Remote_Password, reason)` 打包为带 HMAC 的上报帧，以异步方式投递到 `VHDMount` 的本机端点。
4. 把 `RustDesk_Controlled` 进程产生的全部日志通过同一根命名管道以独立的日志帧转发给 `VHDMount` 处理，自身不维护任何本地日志文件；命名管道不可用时静默退避，不回退到 stderr 或磁盘文件。
5. 复用 RustDesk 既有的密码校验流程作为远控接受门控，并在密码校验通过后把主控端登录请求中的 `lr.my_id` 通过命名管道交给 `VHDMount`，由其向 `VHDSelectServer` 询问该主控端是否被批准；命名管道不可用时回退到 RustDesk 既有的"密码正确即放行"行为。
6. 在 `RustDesk_Controlled` 没有任何活动远控会话时彻底隐身，进入"被控"或"被观察"状态时则在所有显示器上覆盖全屏遮罩，提示本机正在被远程维护并提供活体动画指示。
7. 自建服务端基于上游仓库 `https://github.com/rustdesk/rustdesk-server`（hbbs / hbbr）部署，通过 RustDesk 既有的 `src/custom_server.rs` 注入机制接入；本桥接模块不重复实现自定义服务端选择逻辑，仅声明 `RustDesk_Controlled` 与该接入路径的兼容性。
8. 与 RustDesk 实现一同交付一份独立的命名管道交互规范文档，供 `VHDMount` 与 `VHDSelectServer` 团队对接和评审。

## Glossary

- **RustDesk_Controlled**: 本仓库构建出的"被控端"形态二进制，仅在 Windows 上部署，持有远控 ID 与登录密码，是本特性所有桥接行为与 UI 遮罩行为的唯一宿主。
- **RustDesk_Controller**: 本仓库构建出的"主控端"形态二进制（跨平台），仅作为远控发起方使用。本特性范围外。
- **RustDesk_RelayServer**: 本仓库构建出的"转发 / 中继"形态二进制（跨平台，对应 hbbs / hbbr 或其封装形态），不直接处理凭据。本特性范围外。
- **VHD_Bridge**: 本特性新增的桥接模块，运行在 `RustDesk_Controlled` 进程内，是一个独立逻辑组件，仅持有指向同机 `VHDMount` 的 Windows 命名管道客户端能力。
- **VHDMount**: 本机已经存在的 C# 主程序，由独立仓库交付，承担机台身份认证、TPM 私钥访问、向 `VHDSelectServer` 的所有 HTTPS 调用与签名工作。
- **VHDMount_Bridge_Endpoint**: `VHDMount` 暴露给同机 `RustDesk_Controlled` 的 Windows 命名管道端点，默认路径 `\\.\pipe\VHDMount.RustDeskBridge`。
- **VHDSelectServer**: 由 `machine-auth.md` 描述的远端服务端。本规格中 `RustDesk_Controlled` **不**直接与之通信。
- **RustDeskClientSharedSecret**: 一段 32 字节的随机串，由发行流程在编译期注入 RustDesk 二进制（`build.rs` 从 `vhd_bridge_secret.bin` 读取后嵌入），用于在 `VHDMount_Bridge_Endpoint` 上向 `VHDMount` 证明发起方是合法的 RustDesk 客户端。
- **Shared_Secret_Version**: 当前 `RustDeskClientSharedSecret` 对应的版本号（无符号整数），用于配合 `VHDMount` 与 `VHDSelectServer` 的远程吊销机制。
- **VHDRustDeskBridgeHandshakeV1**: 桥接握手阶段使用的版本字符串与 HMAC 协议名。
- **VHDRustDeskBridgeReportV1**: 桥接上报阶段使用的版本字符串与帧名。
- **Handshake_Frame**: `RustDesk_Controlled` 在 IPC 连接建立后首先发送的 JSON 帧，包含 `Shared_Secret_Version`、随机 nonce、毫秒时间戳与 HMAC 证明值。
- **Report_Frame**: 握手通过后由 `RustDesk_Controlled` 发送的上报帧，包含 `RustDesk_Remote_ID`、`RustDesk_Remote_Password`、`reason` 等字段以及对帧整体的 HMAC。
- **RustDesk_Remote_ID**: 受控端的远控 ID，由 `Config::get_id()` 返回。
- **RustDesk_Remote_Password**: 受控端的远控登录密码，包含临时密码（`temporary_password`）与永久密码两类。
- **Bridge_Config**: 本特性引入的配置项集合，归属 `libs/hbb_common/src/config.rs` 的现有配置体系。
- **Bridge_State**: `VHD_Bridge` 暴露给 `RustDesk_Controlled` 其它模块的运行时状态机，取值集合为 `{Disabled, Initializing, Connected, Authorized, Denied, Failed}`，**仅作为可观测性信号**，不参与远控连接的接受 / 拒绝决策。
- **Last_Reported_Snapshot**: 最近一次被 `VHDMount` 接受（`accepted`）的上报快照，用于幂等去重。
- **Active_Remote_Session**: `RustDesk_Controlled` 当前正在进行的、来自任意主控端发起方的远控会话，包含两类：
    - **Controlled_Session**（完整被控）: 发起方既可观看屏幕也可下发输入。
    - **View_Only_Session**（被观察）: 发起方仅观看屏幕，不下发输入；对应 RustDesk 现有的 `view-only` 概念。
- **Maintenance_Overlay**: `RustDesk_Controlled` 在存在至少一个 `Active_Remote_Session` 时，于所有显示器上展示的全屏遮罩窗口，承载维护文案与活体动画指示。
- **Liveness_Indicator**: `Maintenance_Overlay` 上持续运行的活体动画元素（例如旋转加载圈或来回滑动指针），用于让旁观者区分"远程维护中"与"机台死机"。
- **Bridge_Protocol_Spec_Doc**: 本特性交付清单中独立的 Markdown 文档（默认路径 `docs/vhd-rustdesk-bridge-protocol.md`），描述命名管道交互协议，供 `VHDMount` 与 `VHDSelectServer` 团队对接。
- **VHD_Self_Hosted_Server_Repo**: 自建 RustDesk 服务端的上游 Git 仓库，地址 `https://github.com/rustdesk/rustdesk-server`，包含 hbbs（rendezvous）与 hbbr（relay）两份二进制；维护方按需克隆并打补丁，本仓库不内联其源码。
- **Custom_Server_Injection**: RustDesk 既有的自定义服务端注入机制，由 `src/custom_server.rs::CustomServer { host, key, api, relay }` 实现，支持文件名后缀（`host=...,key=...,relay=...,api=...`）与签名 base64 license 两种形态；运行时落到 `Config::get_rendezvous_server()` / `Config::get_option("relay-server")` / `Config::get_option("api-server")` 等现有键。
- **Log_Frame**: 通过 `VHDMount_Bridge_Endpoint` 由 `RustDesk_Controlled` 投递给 `VHDMount` 的日志帧，协议名 `VHDRustDeskBridgeLogV1`；帧负载承载已脱敏的日志事件，由 `VHDMount` 负责落盘 / 留存。
- **VHDRustDeskBridgeLogV1**: 日志帧使用的版本字符串与 HMAC 协议名。
- **Peer_Approval_Request**: `RustDesk_Controlled` 在密码校验通过后、连接进入授权阶段前，通过命名管道向 `VHDMount` 发送的主控端身份批准请求帧，协议名 `VHDRustDeskBridgePeerApprovalV1`。
- **Peer_Approval_Response**: `VHDMount` 对 `Peer_Approval_Request` 的响应，包含 `approved` / `rejected` 与可选的批准缓存 `ttlMs`。
- **VHDRustDeskBridgePeerApprovalV1**: 主控端批准帧使用的版本字符串与 HMAC 协议名。
- **Controller_Id**: 主控端登录请求 `LoginRequest.my_id` 字段携带的远控 ID，由 `RustDesk_Controlled` 透传给 `VHDMount` 用于服务端侧批准判定。
- **Controller_Approval_Wait_Msgbox**: `RustDesk_Controller` 在 `VHDMount` 批准查询尚未返回时向用户展示的进度提示，复用 RustDesk 既有 `interface.msgbox(...)` 路径，新增 `wait-vhd-approval` 类 msgtype 与若干 `LOGIN_MSG_VHD_*` 常量。
- **LOGIN_MSG_VHD_APPROVAL_PENDING**: `RustDesk_Controlled` 在 `Peer_Approval_Request` 已发出但尚未收到响应时，主动通过登录响应通道下发给 `RustDesk_Controller` 的进度信号常量字面量；与 `crate::client::LOGIN_MSG_*` 同族，文案语义为"正在等待 VHDMount 验证您的身份"。
- **LOGIN_MSG_VHD_APPROVAL_REJECTED**: `RustDesk_Controlled` 在收到 `Peer_Approval_Response.result = "rejected"` 后，与"密码错误"等价的对外错误码字面量，文案语义为"您的身份未被 VHDMount 批准"。
- **Controlled_Initiator_Stripped**: `RustDesk_Controlled` 在编译期被裁剪掉的"作为远控发起方"的全部代码路径与 UI 入口（含最近列表 / 收藏 / 地址簿 / 局域网发现 / `--connect` 等 CLI 子命令）。
- **Build_Prereq_Vars**: 一组在 `build.rs` 编译期被前置校验的 RustDesk 服务端注入变量集合，由 `HBBS_Key` / `HBBS_Host` / `HBBR_Host` 三项构成；任意一项缺失或格式非法 SHALL 直接以非零退出码终止编译，与 `RustDeskClientSharedSecret` 共享同一套"环境变量优先、`Dev_Secret_File` 回退"的解析策略。
- **Dev_Secret_File**: 仓库根目录下的本地开发期回退文件 `secret.sec`，UTF-8 纯文本，按行分隔，每条行形如 `<Name>: <value>` 或 `<Name>：<value>`（ASCII 冒号 `:` 与全角中文冒号 `：` SHALL 被视为字节级等价的同一分隔符语义）；可识别的 `<Name>` 限于 `HBBS Key` / `HBBS Host` / `HBBR Host` / `VHDMount Key` / `VHDMount Key Version` 五项；空行与未识别行 SHALL 被静默忽略；该文件与 `vhd_bridge_secret.bin` 同等地列入 `.gitignore`，SHALL NOT 进入版本控制，CI 构建中 SHALL NOT 出现该文件。
- **HBBS_Key**: 自建 hbbs rendezvous 服务（`VHD_Self_Hosted_Server_Repo` 中的 `hbbs` 二进制）使用的 ed25519 公钥，base64 编码后等价 32 字节裸字节；编译期通过环境变量 `HBBS_KEY` 或 `Dev_Secret_File` 中 `HBBS Key` 行注入到 RustDesk 既有的 `hbb_common::config::RS_PUB_KEY` 编译期常量槽，作为 RustDesk 默认 rendezvous 服务公钥。
- **HBBS_Host**: 自建 hbbs rendezvous 服务的网络端点，形如 `host[:port[-port2]]`（例如 `home.271104.xyz:21115-21116`）；编译期通过环境变量 `HBBS_HOST` 或 `Dev_Secret_File` 中 `HBBS Host` 行注入到 RustDesk 既有 `RENDEZVOUS_SERVERS` 默认列表。
- **HBBR_Host**: 自建 hbbr relay 服务的网络端点，形如 `host[:port]`（例如 `home.271104.xyz:21117`）；编译期通过环境变量 `HBBR_HOST` 或 `Dev_Secret_File` 中 `HBBR Host` 行注入为 RustDesk 既有 `relay-server` 默认 option。

## Requirements

### Requirement 1: 部署角色与编译启用范围

**User Story:** 作为发布工程师，我希望桥接代码与被控端 UI 遮罩代码只出现在 `RustDesk_Controlled` 形态的产物中，所以主控端与中继服务的产物不会因为本特性多出任何攻击面或代码膨胀。

#### Acceptance Criteria

1. THE 编译特性 `vhd-bridge` SHALL 仅在 `RustDesk_Controlled` 形态的构建配置中默认启用，且 SHALL 在 `RustDesk_Controller` 与 `RustDesk_RelayServer` 形态的构建配置中默认关闭。
2. WHERE 当前构建目标为 `RustDesk_Controller` 或 `RustDesk_RelayServer`, THE 桥接相关 Rust 模块（`src/vhd_bridge/` 等）、`Maintenance_Overlay` 相关 Flutter 代码以及 `RustDeskClientSharedSecret` 的嵌入逻辑 SHALL 全部被条件编译剔除，且产物中 SHALL NOT 包含任何 HMAC、共享密钥或 `VHDRustDeskBridgeHandshakeV1` / `VHDRustDeskBridgeReportV1` 字符串字面量。
3. WHERE 当前构建目标为 `RustDesk_Controlled`, THE 构建目标 SHALL 仅为 Windows（x86_64 与 aarch64），且 SHALL NOT 在 Linux、macOS、Android、iOS 上提供 `RustDesk_Controlled` 形态的发行产物。
4. THE Bridge_Config 字段 SHALL 仅在 `RustDesk_Controlled` 产物中作为可读写的运行时配置暴露；WHERE 当前产物形态为 `RustDesk_Controller` 或 `RustDesk_RelayServer`, THE Bridge_Config 字段 SHALL 要么完全不暴露，要么仅以只读 stub 形式存在并 SHALL NOT 触发任何运行期桥接行为或被纳入 IPC 配置同步消息。
5. THE VHD_Bridge SHALL 仅使用 Windows 命名管道作为唯一的 `VHDMount_Bridge_Endpoint` 形态，且本规格 SHALL NOT 包含任何针对 Unix 域套接字、Linux、macOS、Android 或 iOS 的桥接条款。
6. WHERE 当前构建目标为 `RustDesk_Controlled`, THE 构建配置 SHALL 移除"主动作为远控发起方"的全部代码路径，使该形态产物 SHALL NOT 通过 RustDesk 既有 UI、CLI、IPC 或 URI scheme（如 `rustdesk:` 协议）发起到任何其它机台的远控连接、文件传输、端口转发或终端会话；具体来说 THE 构建配置 SHALL 在被控端形态中移除 / 抑制以下入口：远控会话发起按钮、最近连接列表（`PeerConfig::peers` / `main_load_recent_peers`）、收藏列表（`get_fav` / `main_load_fav_peers`）、地址簿入口（`Ab::load` / `main_load_recent_peers_for_ab`）、局域网发现（`crate::lan::*` / `main_load_lan_peers`）以及命令行 `--connect` / `--play` / `--port-forward` 等子命令的可执行路径。
7. WHEN 构建 `RustDesk_Controlled` 产物时编译特性 `vhd-bridge` 被显式关闭, THE RustDesk_Controlled SHALL 表现得与未集成桥接时完全一致，包括启动时间、内存占用与既有协议交互；该回退分支仅用于研发期排障，SHALL NOT 用于生产发布。

### Requirement 2: 本机命名管道客户端与连接管理

**User Story:** 作为 RustDesk 维护者，我希望桥接模块以一个本机命名管道客户端的形态接入 `VHDMount`，所以我不需要在 RustDesk 内部维护 HTTPS、证书、服务端 URL 或类 Unix 平台分支等远端通信细节。

#### Acceptance Criteria

1. THE VHD_Bridge SHALL 通过 `Bridge_Config.pipe_name` 指向的 `VHDMount_Bridge_Endpoint` 与同机 `VHDMount` 建立单向客户端连接，且 SHALL 使用 `tokio::net::windows::named_pipe::ClientOptions` 完成连接。
2. THE VHD_Bridge SHALL 在每次连接建立请求中应用 `Bridge_Config.request_timeout_ms` 作为连接超时上限，且 SHALL 在超时后视为本次连接失败并按 §9 的固定间隔重试策略安排下一次连接尝试。
3. THE VHD_Bridge SHALL 同一时间最多保持一个活动 IPC 会话，且 SHALL 在新触发到来时复用该会话而不是为每次触发新建连接。
4. WHEN 命名管道被对端关闭、被操作系统重置或读写返回 EOF, THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Initializing` 并 SHALL 调度一次重连。
5. WHILE 处于断线重连阶段, THE VHD_Bridge SHALL 使用 `Bridge_Config.retry_interval_ms`（默认 2000 毫秒）作为固定重连间隔，附加 0–200 毫秒的均匀分布抖动以错峰，SHALL 在连续 5 次连接失败后将日志级别提升到 `warn`，且 SHALL NOT 使用任何形式的指数退避或上限放大策略——本机 IPC 不需要保护远端服务。
6. THE VHD_Bridge SHALL NOT 在自身代码路径中持有任何 HTTPS 客户端实例，且 SHALL NOT 通过 IPC 发起任何指向 `VHDSelectServer` 的网络请求。
7. THE VHD_Bridge SHALL 在写入命名管道时使用"4 字节小端长度前缀 + JSON 负载"的按帧分隔协议，把 `Handshake_Frame` 与 `Report_Frame` 与同机其它字节流明确隔离。

### Requirement 3: 编译期共享密钥与运行期使用

**User Story:** 作为安全负责人，我希望 `RustDesk_Controlled` 对 `VHDMount` 的身份证明只依赖编译期注入的密钥，所以同机其它进程或调试器不能从配置文件、IPC 同步通道或日志里把它读出来。

#### Acceptance Criteria

1. THE RustDeskClientSharedSecret SHALL 由 `build.rs` 在编译期注入二进制，注入来源 SHALL 按下列优先级唯一确定：环境变量 `VHD_BRIDGE_SECRET_HEX`（64 字符 hex，等价 32 字节）> 环境变量 `VHD_BRIDGE_SECRET_B64`（44 字符 Base64，含或不含尾随 `=`，等价 32 字节）> 相对路径 `vhd_bridge_secret.bin`（32 字节裸字节）；注入后 SHALL 以 `&'static [u8; 32]` 形式可见于运行期。
2. IF `VHD_BRIDGE_SECRET_HEX` 与 `VHD_BRIDGE_SECRET_B64` 同时被设置, THEN THE 编译流程 SHALL 失败并 SHALL 在错误信息中提示二者互斥。
3. WHERE 上述两个环境变量都未设置且 `vhd_bridge_secret.bin` 不存在或长度不等于 32 字节, THE 编译流程 SHALL 立即失败并以非零退出码终止，错误信息 SHALL 同时提示三种支持的注入方式与期望长度（32 字节）；THE 编译流程 SHALL NOT 通过插入零字节填充、随机字节占位或任何其它方式让构建在共享密钥缺失时静默继续，使 `RustDesk_Controlled` 在运行期才落入 `Failed` 状态成为不可能事件。
4. THE 编译流程 SHALL 校验最终解码后的字节长度严格等于 32，且 SHALL 在长度不匹配、Hex / Base64 解析失败或环境变量取值含非法字符时立即失败并以非零退出码终止，与 §3.3 同等严格。
5. THE 编译流程 SHALL NOT 把 `VHD_BRIDGE_SECRET_HEX` / `VHD_BRIDGE_SECRET_B64` 的取值或 `vhd_bridge_secret.bin` 的内容输出到任何编译日志、构建产物的字符串字面量、`.rlib` / `.exe` 元数据或 panic 信息中。
6. THE Bridge_Config.secret_version 默认值 SHALL 优先取自环境变量 `VHD_BRIDGE_SECRET_VERSION`（无符号整数），WHERE 该环境变量未设置 THE 默认值 SHALL 回退到 §4.1 声明的默认值；运行期 `Bridge_Config` 的覆盖 SHALL 仍然有效。
7. THE RustDeskClientSharedSecret SHALL NOT 被序列化到 `Bridge_Config`、`hbb_common` 配置文件、IPC 配置同步消息或任何位于磁盘的运行期文件。
8. THE VHD_Bridge SHALL 仅在 HMAC 计算函数内部以最小作用域读取 `RustDeskClientSharedSecret`，且 SHALL NOT 通过任何公共 API（包括但不限于 `vhd_bridge::current_state()` 与 IPC `vhd-bridge-state`）暴露密钥本体或派生值。
9. WHEN HMAC 计算结束, THE VHD_Bridge SHALL 立即对承载中间状态（包括拼接缓冲区与 HMAC 输出）的可变缓冲区执行覆写清零。
10. THE VHD_Bridge SHALL 使用恒定时间比较（例如 `subtle::ConstantTimeEq` 或等价实现）对 `VHDMount` 返回的任何 MAC 字段进行校验，SHALL NOT 使用 `==` 或 `memcmp` 等可被时序侧信道利用的比较方式。
11. THE Bridge_Config.secret_version SHALL 与 `RustDeskClientSharedSecret` 的实际版本号一致，且 SHALL 是日志中允许出现的唯一与共享密钥相关的字段。
12. WHERE 环境变量 `VHD_BRIDGE_SECRET_HEX` / `VHD_BRIDGE_SECRET_B64` 与文件 `vhd_bridge_secret.bin` 全部缺失但仓库根目录存在 `Dev_Secret_File` (`secret.sec`), THE 编译流程 SHALL 把该文件中名为 `VHDMount Key` 的行（hex，64 字符，等价 32 字节，与 `VHD_BRIDGE_SECRET_HEX` 含义一致）作为 `RustDeskClientSharedSecret` 的回退来源；THE 注入优先级 SHALL 严格为 `VHD_BRIDGE_SECRET_HEX` env > `VHD_BRIDGE_SECRET_B64` env > `vhd_bridge_secret.bin` 文件 > `Dev_Secret_File` 中的 `VHDMount Key` 行；`Dev_Secret_File` 仅作为本地开发期回退，CI 构建中 SHALL NOT 依赖该文件。§3.2 关于"`_HEX` 与 `_B64` 同时设置即编译失败"的互斥约束 SHALL 保持不变。
13. WHERE `RustDeskClientSharedSecret` 由 §3.12 的 `Dev_Secret_File` 回退路径解析得到, THE Bridge_Config.secret_version 默认值 SHALL 由 `Dev_Secret_File` 中名为 `VHDMount Key Version` 的行（十进制无符号整数，与 `VHD_BRIDGE_SECRET_VERSION` 含义一致）提供；WHEN 环境变量 `VHD_BRIDGE_SECRET_VERSION` 已显式设置, THE 环境变量取值 SHALL 仍然胜出；WHERE `Dev_Secret_File` 中 `VHDMount Key Version` 行缺失或不可解析为无符号整数且环境变量也未设置, THE 编译流程 SHALL 回退到 §3.6 原本声明的默认值。`VHDMount Key Version` 行 SHALL 与 `VHDMount Key` 行配套提供，二者解析过程 SHALL NOT 把数值之外的任何文本输出到编译日志或产物中。

### Requirement 4: 配置项与强制启用

**User Story:** 作为运维管理员，我希望桥接模块在 `RustDesk_Controlled` 形态下默认且强制启用、不能被运行期开关或配置覆盖关闭，所以这台生产机台不会因为人为误操作或运行期配置注入失去与 `VHDMount` 的连接，但少量必要的运行参数仍然可调。

#### Acceptance Criteria

1. THE Bridge_Config SHALL 仅在 `libs/hbb_common/src/config.rs` 中暴露下列字段: `pipe_name (string, 默认 "\\\\.\\pipe\\VHDMount.RustDeskBridge")`、`secret_version (u32, 默认值由编译期 §3.6 决定)`、`request_timeout_ms (u32, 默认 5000)`、`retry_interval_ms (u32, 默认 2000)`。
2. THE Bridge_Config SHALL NOT 包含 `enabled`、`registration_certificate_path`、`registration_certificate_password`、`server_base_url`、`server_public_key_pem`、`tpm_key_name`、`allow_software_key_fallback`、`gate_remote_access_until_authorized` 字段；THE VHD_Bridge SHALL 在编译特性 `vhd-bridge` 启用时把"是否启用"硬编码为 `true`，SHALL NOT 提供任何运行期开关、CLI 参数、IPC 命令或环境变量将其切换为 `false`。
3. WHEN `RustDesk_Controlled` 启动且编译特性 `vhd-bridge` 启用, THE VHD_Bridge SHALL 立即进入 `Initializing` 状态并 SHALL 开始 §2 描述的连接流程，且 SHALL 在 `RustDesk_Controlled` 进程整个生命周期内保持桥接任务为活动状态。
4. IF `Bridge_Config.pipe_name` 在运行期被设置为空字符串或解析失败, THEN THE VHD_Bridge SHALL 回退到 §4.1 声明的默认值并 SHALL 以 `warn` 级别记录该回退；THE VHD_Bridge SHALL NOT 因为该字段被覆写而进入 `Disabled` 或 `Failed` 状态。
5. THE Bridge_Config 字段 SHALL 与 §13.6 一致，仅作为运行期参数微调存在，运行期 IPC 配置同步消息 SHALL NOT 携带任何用于"关闭桥接"语义的字段；任何上游来源（命令行、配置文件、IPC 同步、UI 动作）尝试关闭桥接的请求 SHALL 被忽略并 SHALL 以 `warn` 级别记录拒绝原因。
6. THE Bridge_State 取值 `Disabled` SHALL 仅由"编译特性 `vhd-bridge` 关闭"这一构建期事实触发，运行期 SHALL NOT 出现该状态；§7、§8、§11 等条款中涉及 `Disabled` 的运行期分支 SHALL 视为对编译特性关闭场景的描述，与启用场景互斥。
7. WHERE 编译特性 `vhd-bridge` 被关闭, THE Bridge_Config 字段与桥接相关代码路径 SHALL 全部被条件编译剔除，与 §1.6 一致；该编译特性 SHALL 由发布构建配置控制，SHALL NOT 通过环境变量或运行期开关在已发布产物上切换。

### Requirement 5: 握手协议（VHDRustDeskBridgeHandshakeV1）

**User Story:** 作为 VHDMount 的开发者，我希望任何通过 `VHDMount_Bridge_Endpoint` 上报远控凭据的发起方都先证明自己是合法的 RustDesk 客户端，所以我可以拒绝同机的伪造进程。

#### Acceptance Criteria

1. WHEN IPC 连接建立成功, THE VHD_Bridge SHALL 在第一帧发送 `Handshake_Frame`，其 JSON 结构 SHALL 至少包含 `protocol = "VHDRustDeskBridgeHandshakeV1"`、`secretVersion (u32)`、`nonce (32 hex chars, 16 字节随机)`、`timestampMs (u64, Unix 毫秒)`、`clientKind = "rustdesk"`、`clientVersion (string)`、`proof (Base64)`。
2. THE VHD_Bridge SHALL 按 `proof = HMAC-SHA256(RustDeskClientSharedSecret, "VHDRustDeskBridgeHandshakeV1\n" || secretVersion || "\n" || nonce || "\n" || timestampMs)` 计算证明值，所有非二进制字段 SHALL 以 ASCII 文本表示，行分隔符 SHALL 为 `\n`（LF）。
3. THE VHD_Bridge SHALL 为每次握手生成新的 16 字节 `nonce`，且 SHALL NOT 在 5 分钟内对同一 `secretVersion` 重复使用同一 `nonce`。
4. THE VHD_Bridge SHALL 把 `timestampMs` 设为当前 Unix 毫秒时间戳，且 SHALL 把握手有效窗口视为 5 分钟（即 `VHDMount` 端预期 `|now - timestampMs| ≤ 300000`）。
5. WHEN `VHDMount` 返回 `HandshakeResponse { ok: true }`, THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Connected` 并 SHALL 准备进入上报阶段。
6. IF `VHDMount` 返回 `HandshakeResponse { ok: false, reason: "secret_outdated" }`, THEN THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Failed` 并 SHALL NOT 自动重新握手直到 `Bridge_Config.secret_version` 或二进制本体被更换。
7. IF `VHDMount` 返回 `HandshakeResponse { ok: false, reason: "deny" | "rate_limited" | "invalid_proof" }`, THEN THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Denied` 并 SHALL 在按 §9 的固定间隔（必要时叠加 §9.2 / §9.3 描述的额外延迟）后再尝试重新握手。
8. WHEN `VHDMount` 在 `Bridge_Config.request_timeout_ms` 内未返回任何握手响应, THE VHD_Bridge SHALL 视为握手失败，关闭 IPC 会话并按 §9 的固定间隔重连。

### Requirement 6: 上报帧（VHDRustDeskBridgeReportV1）

**User Story:** 作为 VHDMount 的开发者，我希望从 RustDesk 收到的每一份远控凭据都能被独立验真且与单次握手绑定，所以我可以放心地用 TPM 私钥代签后转发给 `VHDSelectServer`。

#### Acceptance Criteria

1. WHILE `Bridge_State` 等于 `Connected` 或 `Authorized`, THE VHD_Bridge SHALL 通过同一会话发送 `Report_Frame`，其 JSON 结构 SHALL 至少包含 `protocol = "VHDRustDeskBridgeReportV1"`、`rustDeskId (string)`、`passwordKind ("temporary" | "permanent" | "preset" | "absent")`、`password (string, UTF-8 明文，passwordKind = "absent" 时为空串)`、`reason ("startup" | "id_change" | "password_change" | "rotation" | "heartbeat")`、`reportedAt (u64, Unix 毫秒)`、`nonce (32 hex chars, 16 字节随机)`、`mac (Base64)`。
2. THE VHD_Bridge SHALL 按 `mac = HMAC-SHA256(RustDeskClientSharedSecret, "VHDRustDeskBridgeReportV1\n" || secretVersion || "\n" || rustDeskId || "\n" || passwordKind || "\n" || sha256Hex(password) || "\n" || reason || "\n" || reportedAt || "\n" || nonce)` 计算帧 MAC，所有字段 SHALL 与 §6.1 中的最终序列化值完全一致。
3. THE VHD_Bridge SHALL 为每帧生成新的 16 字节 `nonce`，且 SHALL NOT 在同一会话内复用 `nonce`。
4. THE VHD_Bridge SHALL 在每次发送 `Report_Frame` 后等待 `VHDMount` 返回 `ReportAck { result: "accepted" | "rejected", reason?: string }`，并 SHALL 在 `Bridge_Config.request_timeout_ms` 超时后视为发送失败、按 §9 的固定间隔重连后重试。
5. WHEN `ReportAck.result = "accepted"` 首次出现, THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Authorized`，该切换 SHALL 仅作为可观测性信号，与 §8.4 一致 SHALL NOT 用作远控连接接受与否的前置门控。
6. IF `ReportAck.result = "rejected"` 且 `reason = "deny"`, THEN THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Denied` 并 SHALL 在按 §9 的固定间隔（必要时叠加 §9.2 / §9.3 描述的额外延迟）后再尝试重新握手 + 上报。
7. IF `ReportAck.result = "rejected"` 且 `reason = "secret_outdated"`, THEN THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Failed`，与 §5.6 行为一致。
8. WHEN `ReportAck.result = "accepted"` 返回, THE VHD_Bridge SHALL 把 `(rustDeskId, passwordKind, sha256Hex(password))` 缓存为 `Last_Reported_Snapshot`，且 SHALL 在该快照与下一次触发的内容一致时跳过发送（心跳触发除外）。

### Requirement 7: 上报触发时机

**User Story:** 作为运维管理员，我希望远控凭据在任何会导致它变化的事件后都能尽快被推到 VHDMount，所以我不会因为 RustDesk 轮换了密码而被锁在外面。

#### Acceptance Criteria

1. WHEN `Bridge_State` 切换到 `Connected`, THE VHD_Bridge SHALL 触发一次 `reason = "startup"` 的上报投递。
2. WHEN `Config::set_id` 被调用且新值与 `Last_Reported_Snapshot.rustDeskId` 不同, THE VHD_Bridge SHALL 在 1 秒内尝试触发一次 `reason = "id_change"` 的上报投递；IF 系统负载或 IPC 不可用阻止 1 秒内完成调度, THEN THE VHD_Bridge SHALL 把该事件保留在合并队列中，待 IPC 可用时再发送，且 SHALL NOT 因为延迟而丢弃事件。
3. WHEN `password::update_temporary_password` 被调用并产生新的 `temporary_password`, THE VHD_Bridge SHALL 在 1 秒内尝试触发一次 `reason = "password_change"` 的上报投递；IF 上报因 IPC 不可用或限流被推迟, THEN THE VHD_Bridge SHALL 在重试间隔结束后用最新的密码值替换队列中尚未发送的同类事件，避免发送过期密码。
4. WHEN 永久密码被设置或被清除（`set_permanent_password_with_ack` 成功或永久密码被关闭）, THE VHD_Bridge SHALL 按 §7.3 相同的延迟与合并语义触发一次 `reason = "password_change"` 的上报投递。
5. WHEN `RustDesk_Controlled` 因为认证失败次数达到阈值而旋转临时密码（`check_update_temporary_password` 触发）, THE VHD_Bridge SHALL 触发一次 `reason = "rotation"` 的上报投递。
6. WHILE `Bridge_State` 等于 `Connected` 或 `Authorized`, THE VHD_Bridge SHALL 至少每 30 分钟做一次 `reason = "heartbeat"` 的对账上报，使用当前最新的 `(rustDeskId, password)` 内容，确保 `VHDMount` 一侧不会因为漏丢消息而长时间持有过期凭据。
7. WHEN `Bridge_State` 在心跳调度期间从 `Connected` / `Authorized` 暂时切换到 `Denied` / `Initializing` 又恢复, THE VHD_Bridge SHALL 保持心跳定时器持续运行（即定时器不重置），并 SHALL 在每次心跳触发时根据当时的 `Bridge_State` 决定是否真正发送（仅当处于 `Connected` 或 `Authorized` 时发送，否则跳过本次发送但保留下一次的调度）。
8. IF 触发条件在 1 秒窗口内被多次激发, THEN THE VHD_Bridge SHALL 合并为单次上报并使用最近一次的 `reason`，避免对 `VHDMount` 造成放大流量。
9. THE VHD_Bridge SHALL 通过 `tokio::sync::mpsc` 等异步通道接收触发事件，并 SHALL NOT 在调用方线程中阻塞执行 HMAC 计算 / IPC 写入等耗时操作。

### Requirement 8: 状态机语义与远控连接策略解耦

**User Story:** 作为运维管理员，我希望 `RustDesk_Controlled` 不论桥接是否在线、是否被 VHDMount 接受，都按 RustDesk 既有逻辑独立处理远控连接，所以一台机台不会因为桥接侧的瞬时故障而被锁死。

#### Acceptance Criteria

1. THE Bridge_State SHALL 仅在 `{Disabled, Initializing, Connected, Authorized, Denied, Failed}` 集合内取值，且 SHALL 由 §2 / §4 / §5 / §6 中描述的事件唯一驱动。
2. THE Bridge_State 取值 `Authorized` SHALL 仅表示"`VHDMount` 已接受过至少一次本会话内的上报"，**不**表示远控可用的前置条件已满足。
3. THE Bridge_State 取值 `Denied` SHALL 表示 `VHDMount` 在某次握手 / 上报中显式拒绝，仅作为可观测性信号，且 SHALL 在新策略下属于罕见情况；THE Bridge_State 取值 `Failed` SHALL 仅由永久型错误（共享密钥不匹配 / 编译期密钥缺失 / `Bridge_Config` 字段不可解析等）触发。
4. THE RustDesk_Controlled SHALL NOT 因 `Bridge_State` 取任何值（包括 `Disabled` / `Initializing` / `Connected` / `Authorized` / `Denied` / `Failed`）而拒绝来自远控发起方的入向连接；远控接受策略 SHALL 由 RustDesk 既有密码校验逻辑（§19.1）与 `VHDMount` 主控端批准（§19.2-§19.6）共同决定，且 WHERE `Bridge_State` 不等于 `Connected` 或 `Authorized` THE 决策 SHALL 退化为 §19.8 定义的"密码正确即放行"兜底行为。
5. WHEN `RustDesk_Controlled` 启动且 `Bridge_Config.enabled` 为 `true`, THE VHD_Bridge SHALL 把 `Bridge_State` 初始化为 `Initializing` 并 SHALL 立即开始 §2 的连接流程。
6. WHEN IPC 握手成功（§5.5）, THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Connected`；WHEN 收到首个 `ReportAck.result = "accepted"`（§6.5）, THE VHD_Bridge SHALL 切换到 `Authorized`。
7. WHEN `VHDMount` 主动通过 IPC 推送 `Revocation { reason: "denied" | "secret_outdated" }`, THE VHD_Bridge SHALL 切换到对应的 `Denied` / `Failed` 状态，与握手 / 上报阶段返回相同 `reason` 时的语义一致；THE VHD_Bridge SHALL 在当前 `Bridge_State` 等于 `Disabled` 时同样接受并应用该状态切换，使 `VHDMount` 后续重新接通时不会立刻再次进入 `Authorized`。
8. WHEN `Bridge_State` 由非 `Authorized` 切换到 `Authorized`, THE VHD_Bridge SHALL 在 1 秒内触发一次 `reason = "startup"` 的上报投递（与 §7.1 一致），用于覆盖切换瞬间可能漏发的事件。

### Requirement 9: 失败处理与重试

**User Story:** 作为运维管理员，我希望 `RustDesk_Controlled` 在 IPC 不可用或被 VHDMount 限流时按本机 IPC 的固定间隔安静地重试，所以本机调试与重启 VHDMount 时不会出现一旦失败就长时间不可用的情况。

#### Acceptance Criteria

1. WHEN IPC 连接建立失败（端点不存在 / 拒绝连接 / 超时）, THE VHD_Bridge SHALL 使用 `Bridge_Config.retry_interval_ms`（默认 2000 毫秒）作为固定重连间隔，附加 0–200 毫秒的均匀分布抖动以错峰，SHALL NOT 使用任何形式的指数退避或上限放大策略。
2. WHEN `VHDMount` 返回 `HandshakeResponse { ok: false, reason: "rate_limited" }` 或 `ReportAck { result: "rejected", reason: "rate_limited" }`, THE VHD_Bridge SHALL 在 §9.1 的固定重试间隔之上叠加 60 秒额外延迟，且 SHALL 仅在收到下一次成功响应后才把"叠加延迟"的状态清零。
3. WHEN `VHDMount` 返回 `HandshakeResponse { ok: false, reason: "deny" }` 或 `ReportAck { result: "rejected", reason: "deny" }`, THE VHD_Bridge SHALL 沿用 §9.1 的固定重试间隔，且 SHALL NOT 把固定间隔放大或转为指数退避。
4. THE VHD_Bridge SHALL NOT 实现任何 HTTP 状态码相关的重试 / 退避逻辑（包括但不限于 429 / 503 / 400），且 SHALL 不持有任何"服务端是否启用了远控凭据接口"的判断状态——这些 SHALL 全部由 `VHDMount` 处理。
5. IF 错误属于"永久型"（编译期共享密钥缺失会在 §3 / §14 中由编译期阻塞，不会进入运行期；其它如 `Bridge_Config.pipe_name` 解析失败按 §4.4 回退而不进入永久型错误；运行期识别到的永久型错误仅包括"`Peer_Approval_Request` 协议被 `VHDMount` 标记为不再支持的版本不匹配"等明确不可恢复信号）, THEN THE VHD_Bridge SHALL 进入 `Failed` 状态而不进入重试队列；THE VHD_Bridge SHALL 在同一启动周期内同时检测到多个永久型错误时仅切换一次 `Failed` 状态，SHALL NOT 引入额外的聚合 / 上报路径。
6. WHILE `Bridge_State` 等于 `Failed`, THE VHD_Bridge SHALL 仅在 `Bridge_Config.secret_version` 变更或 `RustDesk_Controlled` 重启后才重新尝试初始化；§9 SHALL NOT 通过 `Bridge_Config.enabled` 这类"运行期开关"恢复 `Failed`，因为按 §4 该字段已被移除。
7. WHEN IPC 写入返回 `BrokenPipe` / `ConnectionReset` 或任何其它 IPC I/O 错误, THE VHD_Bridge SHALL 立即关闭当前会话并按 §2.4 的语义切换到 `Initializing`，按 §9.1 的固定间隔重试，且 SHALL NOT 因为 IPC I/O 错误本身进入 `Failed` 状态。
8. WHEN 同一交互回合内 `VHDMount` 已经返回 `secret_outdated` 而后续 IPC 读取又出现 `BrokenPipe` / `ConnectionReset`, THE VHD_Bridge SHALL 优先按 §5.6 / §11.2 进入 `Failed` 状态，SHALL NOT 因后到的 I/O 错误把 `Bridge_State` 改回 `Initializing`。

### Requirement 10: 安全属性

**User Story:** 作为安全负责人，我希望桥接模块带来的额外攻击面是受控的，所以我可以放心地把这条本机通道开到生产环境。

#### Acceptance Criteria

1. THE VHD_Bridge SHALL NOT 把 `RustDesk_Remote_Password` 的明文写入磁盘、日志、IPC 同步配置消息或崩溃转储，所有日志中出现的密码字段 SHALL 以固定字符串 `"***"` 替换。
2. THE VHD_Bridge SHALL NOT 把 `RustDeskClientSharedSecret`、`Handshake_Frame.proof` 或 `Report_Frame.mac` 的本体值输出到日志、IPC 消息或崩溃转储；可被记录的相关字段 SHALL 仅限于 `Bridge_Config.secret_version`。
3. THE VHD_Bridge SHALL 在每次完成 HMAC 计算或上报后立即对承载该次密码副本的可变缓冲区执行覆写清零，且 SHALL NOT 通过 `String::clone` / `Vec::clone` 等方式制造长生命周期的副本。
4. THE VHD_Bridge SHALL 使用恒定时间比较来校验 `VHDMount` 返回的任何 MAC / proof 字段，与 §3.6 的要求一致。
5. THE VHD_Bridge SHALL 在每次打开命名管道时通过 Windows 平台 API 校验对端进程是否为 `VHDMount`（例如 `GetNamedPipeServerProcessId` + 进程映像路径校验），且 SHALL 在校验失败时立即关闭会话并按 §9.5 视为永久型错误进入 `Failed`；THE VHD_Bridge SHALL NOT 在没有打开命名管道的代码路径上额外触发对端校验。
6. THE VHD_Bridge SHALL NOT 通过任何 IPC 配置同步消息把 `Bridge_Config.pipe_name` 之外的桥接相关秘密字段同步出去，且 SHALL 在 IPC 配置同步消息中明确剔除 `RustDeskClientSharedSecret` 与 `Bridge_Config.secret_version` 之外的派生值。

### Requirement 11: 凭据生命周期与吊销响应

**User Story:** 作为运维管理员，我希望我可以远程吊销一台机台的桥接凭据或要求它重新走握手流程，所以我能快速处置疑似被泄露的机台；与此同时，桥接吊销 SHALL NOT 直接影响远控连接是否被接受。

#### Acceptance Criteria

1. WHEN `VHDMount` 在任意阶段返回 `reason = "deny"`, THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Denied`，停止主动上报，且 SHALL 在 §9.3 描述的固定间隔重试后再尝试握手。
2. WHEN `VHDMount` 在任意阶段返回 `reason = "secret_outdated"`, THE VHD_Bridge SHALL 把 `Bridge_State` 切换到 `Failed`，且 SHALL NOT 在不更换 `Bridge_Config.secret_version` 与二进制本体的前提下自动恢复。
3. WHILE `Bridge_State` 等于 `Denied`, THE VHD_Bridge SHALL NOT 上报任何远控凭据，且 SHALL NOT 主动把 `Bridge_State` 切换到 `Authorized`——必须经历一次完整的握手 + 上报往返。
4. WHEN 用户通过 RustDesk UI 或 CLI 触发"重置桥接状态"操作, THE VHD_Bridge SHALL 关闭当前会话、清空 `Last_Reported_Snapshot` 并把 `Bridge_State` 切换到 `Initializing`，重新进入 §2 的连接流程。
5. WHEN `Bridge_Config.secret_version` 在运行时被重新加载为不同值, THE VHD_Bridge SHALL 关闭当前会话并按 §11.4 的语义重新初始化。
6. THE 任意 `Bridge_State` 切换（包括 `Denied`、`Failed`、`Disabled`）SHALL NOT 触发对入向远控连接的拒绝，与 §8.4 一致。

### Requirement 12: 可观测性与诊断

**User Story:** 作为支持工程师，我希望我能从一台 `RustDesk_Controlled` 的日志或运行时面板上看到桥接当前的状态，所以排障时不需要 attach 调试器。

#### Acceptance Criteria

1. THE VHD_Bridge SHALL 暴露一个只读 API `vhd_bridge::current_state()`，返回 `Bridge_State` 与最近一次错误的简短描述。
2. THE VHD_Bridge SHALL 在状态切换时以 `log::info!` 记录形如 `vhd_bridge: state {old} -> {new} ({reason})` 的日志条目，其中 `reason` SHALL 仅包含来自 `VHDMount` 的离散原因码（如 `deny / rate_limited / invalid_proof / secret_outdated / pipe_closed`）。
3. THE VHD_Bridge SHALL 在每次上报被 `VHDMount` 接受时以 `log::info!` 记录 `rustDeskId` 的脱敏前缀（前 3 位 + `***`）、`passwordKind` 与 `Bridge_Config.secret_version`，且 SHALL NOT 记录密码、密文、HMAC 本体或共享密钥本体。
4. THE VHD_Bridge SHALL 通过 RustDesk 现有的 IPC 通道（`src/ipc.rs`）暴露名为 `vhd-bridge-state` 的只读配置键，使 Flutter UI 可以读取当前状态用于展示，键值 SHALL 仅包含 `Bridge_State` 与 §12.2 规定的离散原因码。
5. WHEN `Bridge_State` 变成 `Failed` 或 `Denied`, THE VHD_Bridge SHALL 始终在对应的 `vhd-bridge-state` IPC 消息中包含一条供 UI 展示的本地化错误码，且 SHALL NOT 因 UI 是否主动查询而决定是否携带该错误码；错误码集合 SHALL 在设计阶段定义并稳定。

### Requirement 13: 与 RustDesk 既有体系的集成约束

**User Story:** 作为 RustDesk 维护者，我希望本特性不会破坏现有的连接路径或配置体系，所以现有用户在不开启桥接时感受不到任何差异。

#### Acceptance Criteria

1. WHERE `Bridge_Config.enabled` 为 `false` 或编译特性 `vhd-bridge` 被关闭, THE RustDesk_Controlled SHALL 表现得与未集成桥接时完全一致，包括启动时间、内存占用与既有协议交互。
2. THE VHD_Bridge SHALL 通过现有 Tokio 运行时调度异步任务，且 SHALL NOT 创建嵌套 Tokio 运行时或调用 `Runtime::block_on` 于 async 上下文中。
3. THE VHD_Bridge SHALL 优先复用 `tokio::net::windows::named_pipe` 执行 IPC，SHALL NOT 引入新的 IPC crate（如 `interprocess`、`parity-tokio-ipc` 等），且 SHALL 通过现有 Tokio 运行时驱动所有 IPC I/O，SHALL NOT 通过同步阻塞 API 或独立的非 Tokio 异步运行时绕开运行时复用要求。
4. THE VHD_Bridge SHALL 与 `src/ipc.rs` 现有的帧编码风格保持一致（4 字节小端长度前缀 + JSON / bincode 负载，按既有模块的选择），SHALL NOT 在桥接专属帧上引入与 `src/ipc.rs` 不一致的分隔符或编码风格。
5. WHERE 桥接需要 HMAC-SHA256 / 随机数能力, THE VHD_Bridge SHALL 优先复用 `hbb_common` 已有的或可由其暴露的密码学依赖（如 `hmac`、`sha2`、`rand`），SHALL NOT 新增重复实现的密码学库。
6. THE Bridge_Config 字段 SHALL 通过 `libs/hbb_common/src/config.rs` 已有的 keys 机制暴露，使其可以与其它配置项一致地被 IPC 同步、UI 读取与 CLI 覆盖。
7. THE VHD_Bridge SHALL NOT 修改 RustDesk 现有的 ID / 密码生成逻辑（`Config::set_id` / `password::update_temporary_password` / `set_permanent_password_with_ack`），仅以"观察者"形式在它们之后被通知。

### Requirement 14: 构建产物约束

**User Story:** 作为发布工程师，我希望桥接特性的构建产物清晰、可审计，所以我可以判断一份发行版是否真的具备这条链路、且主控端 / 中继服务的产物绝不会意外携带 HMAC 或共享密钥相关字面量。

#### Acceptance Criteria

1. WHEN 构建 `RustDesk_Controlled` 形态产物且编译特性 `vhd-bridge` 启用, THE 构建流程 SHALL 通过 `build.rs` 按 §3.1 的优先级解析共享密钥（`VHD_BRIDGE_SECRET_HEX` > `VHD_BRIDGE_SECRET_B64` > `vhd_bridge_secret.bin`），并在编译期校验最终字节长度严格等于 32。
2. WHEN 构建 `RustDesk_Controller` 或 `RustDesk_RelayServer` 形态产物, THE 构建流程 SHALL NOT 读取 `vhd_bridge_secret.bin`，SHALL NOT 读取 `VHD_BRIDGE_SECRET_HEX` / `VHD_BRIDGE_SECRET_B64` / `VHD_BRIDGE_SECRET_VERSION` 环境变量，且产物中 SHALL NOT 包含任何与桥接相关的代码段或 HMAC、共享密钥相关字符串字面量（包括 `VHDRustDeskBridgeHandshakeV1` / `VHDRustDeskBridgeReportV1` / `VHDRustDeskBridgeLogV1` / `VHDRustDeskBridgePeerApprovalV1` / `RustDeskClientSharedSecret` 等标识符的字符串实例）。
3. WHEN 构建 `RustDesk_Controlled` 形态产物且编译特性 `vhd-bridge` 关闭, THE 构建流程 SHALL NOT 读取上述任一密钥来源，且产物中 SHALL NOT 包含任何与桥接相关的代码段或字符串字面量；THE 构建流程 SHALL 仍然在产物的版本元数据中暴露 `Bridge_Config.secret_version`（取值固定为 `0` 或编译变量，与启用时一致），用于发布工程师比对。
4. THE 构建流程 SHALL 在 `RustDesk_Controlled` 产物的版本元数据（如 `--version` 输出或资源段）中暴露 `Bridge_Config.secret_version`，使发布工程师可以快速比对 `VHDMount` 与 RustDesk 的密钥版本一致性，且 SHALL NOT 暴露密钥本体。
5. THE CI 配置 SHALL 把 `VHD_BRIDGE_SECRET_HEX` / `VHD_BRIDGE_SECRET_B64` / `VHD_BRIDGE_SECRET_VERSION` 列为 secret 类输入（GitHub Actions 的 `secrets`、GitLab 的 masked variable、Azure DevOps 的 secret variable 等），SHALL NOT 在工作流文件中以明文方式出现密钥取值；本地构建用的 `vhd_bridge_secret.bin` SHALL 列入构建系统的 secret 输入清单与 `.gitignore`，SHALL NOT 被提交到版本控制。
6. THE 桥接相关的 Rust 模块 SHALL 集中在新增子目录（例如 `src/vhd_bridge/`）下，且 SHALL NOT 直接修改 `src/server/`、`libs/scrap/`、`libs/enigo/` 等与远控核心通路无关的模块；仅 `Config::set_id` / `password::*` / `validate_password` 调用点的"观察者"通知与 §19 定义的批准门控钩子 SHALL 以最小必要修改方式接入。
7. THE CI 配置 SHALL 把 `HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST` 三个 `Build_Prereq_Vars` 与既有 `VHD_BRIDGE_SECRET_HEX` / `VHD_BRIDGE_SECRET_B64` / `VHD_BRIDGE_SECRET_VERSION` 一同列为 secret 类输入（GitHub Actions 的 `secrets`、GitLab 的 masked variable、Azure DevOps 的 secret variable 等），SHALL NOT 在工作流文件中以明文方式出现这六个变量的取值，且 SHALL NOT 把这些取值打印到 CI 任务的标准输出 / 标准错误 / 构建产物日志。
8. THE 仓库 SHALL 把 `Dev_Secret_File`（`secret.sec`）与 `vhd_bridge_secret.bin` 列入 `.gitignore`，SHALL NOT 提交到版本控制；THE `build.rs` SHALL NOT 把 `Build_Prereq_Vars` 任一变量的取值或 `Dev_Secret_File` 任一行的取值输出到编译日志、构建产物的字符串字面量、`.rlib` / `.exe` 元数据或 panic 信息；唯一允许出现在产物元数据中的与本组变量相关的字段 SHALL 为 `Bridge_Config.secret_version`（与 §3.5 / §14.4 一致）。

### Requirement 15: 被控端 UI 隐身与维护遮罩

**User Story:** 作为运维管理员，我希望 `RustDesk_Controlled` 在没有任何活动远控会话时对本机用户完全隐身，而在有人正在远程维护时则在屏幕上显著提示并保留活体动画，所以现场旁观者既不会随手关掉服务、也不会把"维护中"误认成"机台死机"。

#### Acceptance Criteria

1. WHILE `RustDesk_Controlled` 没有任何 `Active_Remote_Session`（既不在 `Controlled_Session` 也不在 `View_Only_Session` 中）, THE RustDesk_Controlled SHALL NOT 显示任何窗口、任何托盘图标、任何任务栏图标与任何浮窗，且 SHALL NOT 让普通用户在不查看进程列表的情况下感知到 RustDesk 正在运行。
2. WHEN 一个或多个远控发起方完成密码校验且通过 §19 定义的"主控端身份批准"流程后进入 `Controlled_Session` 或 `View_Only_Session`, THE RustDesk_Controlled SHALL 在所有显示器上同时显示一层 `Maintenance_Overlay`，覆盖整个桌面与任务栏，且 `Maintenance_Overlay` SHALL 始终置顶（topmost）、不可被拖动、不可被最小化。
3. THE Maintenance_Overlay SHALL NOT 在 §19 流程返回 `rejected` 或仍未到达接受路径之前被实例化或显示，使被拒绝的连接尝试对本机用户保持完全无感。
4. THE Maintenance_Overlay SHALL 在屏幕中央显示一段醒目的提示文案，文案 SHALL 提供中英双语（且 SHALL 通过 RustDesk 现有的 i18n 文件配置），文案语义 SHALL 等价于"该机台正在被管理者远程控制以进行维护操作，请不要断开网络或电源连接"。
5. THE Maintenance_Overlay SHALL 同时显示一个持续动画的 `Liveness_Indicator`（例如旋转加载圈或来回滑动指针），且 `Liveness_Indicator` SHALL 在主线程被阻塞或 GPU 不可用的极端情况下仍然以不低于每秒 1 帧的频率刷新，使旁观者能够区分"远程维护中"与"机台死机"。
6. WHEN 当前 `Active_Remote_Session` 数量从大于等于 1 降为 0, THE RustDesk_Controlled SHALL 在 1 秒内移除所有显示器上的 `Maintenance_Overlay` 并恢复到 §15.1 描述的隐身状态。
7. THE Maintenance_Overlay SHALL 阻止本地物理键鼠对桌面的有意义交互（除系统级快捷键如 `Ctrl+Alt+Del` 等无法被应用拦截的组合键外），且 SHALL NOT 阻止远控发起方通过 RustDesk 协议下发的输入事件被传递到 `RustDesk_Controlled` 的现有输入路径。
8. THE Maintenance_Overlay SHALL 在 Flutter 桌面 UI（`flutter/lib/desktop/`）实现，且 SHALL 通过 RustDesk 现有的 IPC 通道（`src/ipc.rs`）从 Rust 进程读取"当前 `Active_Remote_Session` 数量"与"是否包含 `View_Only_Session`"两类状态。
9. WHERE 当前进程是 `RustDesk_Controller` 或 `RustDesk_RelayServer`, THE Maintenance_Overlay 相关代码 SHALL 不被加载、不被链接、不被实例化（与 §1.2 一致）。

### Requirement 16: 交付物——命名管道交互规范文档

**User Story:** 作为 `VHDMount` 与 `VHDSelectServer` 团队，我希望 RustDesk 侧实现交付时同时给出一份独立、自洽的协议规范文档，所以我可以在不读 Rust 源码的情况下完成对端实现与评审。

#### Acceptance Criteria

1. WHEN 本特性的实现工作（`RustDesk_Controlled` 侧 IPC 客户端、`Maintenance_Overlay` UI 与配套测试）完成并通过验收, THE 交付清单 SHALL 同时包含一份独立的 Markdown 文档 `Bridge_Protocol_Spec_Doc`，默认路径 `docs/vhd-rustdesk-bridge-protocol.md`（文件名可由设计阶段微调，但路径 SHALL 在同一仓库内）。
2. THE Bridge_Protocol_Spec_Doc SHALL 至少包含以下章节: 端点定义（命名管道路径、字节序、帧编码格式 4 字节小端长度前缀 + JSON 负载）、完整的握手协议描述（`VHDRustDeskBridgeHandshakeV1` 的字段约束、签名输入字符串构造、HMAC 输入与至少一组示例向量）、完整的上报帧描述（`VHDRustDeskBridgeReportV1` 的字段约束、HMAC 输入与至少一组示例向量）、完整的日志帧描述（`VHDRustDeskBridgeLogV1` 的字段约束、HMAC 输入与至少一组示例向量）、完整的主控端批准帧描述（`VHDRustDeskBridgePeerApprovalV1` 的字段约束、HMAC 输入与至少一组示例向量）、`VHDMount` 应当返回的全部响应类型（`HandshakeResponse` 的 `ok` / `deny` / `rate_limited` / `invalid_proof` / `secret_outdated`，`ReportAck` 的 `accepted` / `rejected` + `reason`，`PeerApprovalResponse` 的 `approved` / `rejected` + `reason` + 可选 `ttlMs`，以及服务器主动推送的 `Revocation` 帧）、协议错误码与建议的本地处理行为、时间窗 / nonce 防重窗口 / HMAC 算法版本 / 共享密钥版本号策略、兼容性策略（`secret_version` 升级、协议版本 `protocol` 升级时双端的行为约定）、至少一组完整的"握手 + 上报 + 心跳 + 日志 + 主控端批准" round-trip 示例（包含字段值，密钥用占位符）。
3. THE Bridge_Protocol_Spec_Doc SHALL 在文档开头记录上游服务端仓库 `https://github.com/rustdesk/rustdesk-server`（即 `VHD_Self_Hosted_Server_Repo`）与 RustDesk 既有的自定义服务端注入路径（`Custom_Server_Injection`），与 §17 一致，使 `VHDMount` 与 `VHDSelectServer` 团队可以在不读 RustDesk 源码的前提下定位服务端入口与配置注入点。
4. THE Bridge_Protocol_Spec_Doc SHALL 描述共享密钥的两种 CI 注入方式（环境变量 `VHD_BRIDGE_SECRET_HEX` / `VHD_BRIDGE_SECRET_B64` 与本地文件 `vhd_bridge_secret.bin`）、版本号 `VHD_BRIDGE_SECRET_VERSION` 的推荐升级流程，与 §3 / §14 一致。
5. THE Bridge_Protocol_Spec_Doc SHALL NOT 暴露任何真实的 `RustDeskClientSharedSecret`，所有示例 SHALL 使用占位符（如 `"<32 random bytes>"` 或 `"REDACTED"`）。
6. THE Bridge_Protocol_Spec_Doc SHALL 与本特性的源代码处于同一 PR 中提交，便于评审者交叉核对协议规范与实现一致性。
7. THE Bridge_Protocol_Spec_Doc 中描述的所有字段、HMAC 输入构造与状态切换语义 SHALL 与本 Requirements Document（§5 / §6 / §8 / §11 / §18 / §19）一致；IF 在实现阶段发现不一致, THEN 实现 SHALL 同时更新 Bridge_Protocol_Spec_Doc 与本 Requirements Document，SHALL NOT 仅修改一侧。

### Requirement 17: 自建 RustDesk 服务端接入方案

**User Story:** 作为运维管理员，我希望桥接特性的需求文档明确写出自建 hbbs / hbbr 的来源仓库与 `RustDesk_Controlled` 的接入路径，所以我可以独立部署服务端、按需修改其源码，而不必反向阅读 RustDesk 客户端代码来理解协议入口。

#### Acceptance Criteria

1. THE 自建服务端 SHALL 来源于上游仓库 `https://github.com/rustdesk/rustdesk-server`（`VHD_Self_Hosted_Server_Repo`），由维护方自行克隆、构建并部署 `hbbs`（rendezvous 服务）与 `hbbr`（relay 服务）；本仓库 SHALL NOT 内联其源码，本特性 SHALL NOT 重新实现服务端二进制。
2. WHEN 维护方需要修改服务端行为（例如增加管控 / 审批接口）, THE 维护流程 SHALL 通过克隆 `VHD_Self_Hosted_Server_Repo` 进行修改与 fork，SHALL NOT 通过在 `RustDesk_Controlled` 中写客户端补丁来绕过协议变更。
3. THE RustDesk_Controlled SHALL 通过 RustDesk 既有的 `Custom_Server_Injection` 机制接入自建服务端：CI 编译流程 SHALL 通过 `src/custom_server.rs` 支持的"二进制文件名后缀"形态（例如 `rustdesk-host=<hbbs-host>,key=<hbbs-pubkey>,relay=<hbbr-host>,api=<api-url>.exe`）或签名后的 base64 license 字符串注入，且 SHALL 同时填入 `host`、`key`、`relay`、`api` 四个字段中实际部署存在的部分。
4. THE 注入后的服务端配置 SHALL 通过 RustDesk 既有 API 生效（如 `Config::get_rendezvous_server()` / `Config::get_rendezvous_servers()` / `Config::get_option("relay-server")` / `Config::get_option("api-server")`），并由 `src/rendezvous_mediator.rs::get_relay_server` 等既有路径消费，本特性 SHALL NOT 引入新的服务端选择路径。
5. THE VHD_Bridge SHALL NOT 直接读取或写入 `Config::set_option("custom-rendezvous-server", ...)` / `Config::get_rendezvous_server()` / `Config::get_option("relay-server")` / `Config::get_option("api-server")` 等键，且 SHALL NOT 直接发起到 hbbs / hbbr 的网络请求；它仅以"接入方案兼容性声明"的形式与既有 `Custom_Server_Injection` 共存。
6. THE Bridge_Protocol_Spec_Doc SHALL 在文档中明示该接入路径（与 §16.3 一致），使 `VHDMount` 与 `VHDSelectServer` 团队理解 `RustDesk_Controlled` 的服务端选择由编译期文件名 / license 决定，而非运行时桥接协议。
7. WHEN CI 编译流程构建 `RustDesk_Controlled` 产物时, THE 构建流程 SHALL 把上述四个字段的注入值（不含密钥本体）记录到产物的发布元数据中，便于发布工程师比对与排障；THE 构建流程 SHALL NOT 把 hbbs `key` 字段（公钥）的字节本体之外的任何私钥或 license 私签材料嵌入产物或日志。

### Requirement 18: 被控端日志转发到 VHDMount

**User Story:** 作为运维管理员，我希望 `RustDesk_Controlled` 把自身的日志全部交给 `VHDMount` 处理，所以一台机台只产生一处日志，便于审计与索引；同时我不希望命名管道临时不可用就让被控端落本地文件污染机台磁盘。

#### Acceptance Criteria

1. THE RustDesk_Controlled SHALL 通过 `VHDMount_Bridge_Endpoint` 以 `Log_Frame` 形式把进程产生的日志事件转发给 `VHDMount` 处理，由 `VHDMount` 负责落盘 / 索引 / 留存；THE RustDesk_Controlled SHALL NOT 维护任何本地日志文件、滚动备份、syslog 或 stderr 输出（除编译特性 `vhd-bridge` 关闭时的回退路径外，参见 §18.9）。
2. THE Log_Frame SHALL 沿用与握手 / 上报帧相同的"4 字节小端长度前缀 + JSON 负载"编码；JSON 字段 SHALL 至少包含 `protocol = "VHDRustDeskBridgeLogV1"`、`secretVersion (u32)`、`level ("error" | "warn" | "info" | "debug" | "trace")`、`target (string, log target)`、`message (string, 已脱敏)`、`timestampMs (u64, Unix 毫秒)`、`mac (Base64)`。
3. THE Log_Frame.mac SHALL 由 `RustDeskClientSharedSecret` 按 `mac = HMAC-SHA256(secret, "VHDRustDeskBridgeLogV1\n" || secretVersion || "\n" || level || "\n" || target || "\n" || sha256Hex(message) || "\n" || timestampMs)` 计算，使同机其它进程无法伪造日志帧。
4. THE RustDesk_Controlled SHALL 通过 `log` crate 的全局 logger 注入一个自定义 sink（例如 `vhd_bridge::log_sink`），把日志事件投递到 `tokio::sync::mpsc` 异步队列，再由桥接 IO 任务串行写入命名管道，且 SHALL NOT 在调用 `log::info!` 等宏的线程上阻塞执行 HMAC / IPC 写入。
5. WHILE 命名管道不可用（未握手 / 重连退避中 / 写入返回 `BrokenPipe` / `ConnectionReset` / `Bridge_State` 等于 `Disabled` / `Initializing` / `Denied` / `Failed`）, THE RustDesk_Controlled SHALL 静默丢弃溢出的日志事件，SHALL 在内部递增"丢弃计数"指标，且 SHALL NOT 把事件回写到本地文件、stderr、syslog、Windows Event Log 或任何其它接收端。
6. THE RustDesk_Controlled SHALL 限制日志投递队列容量，建议上限 4096 条 / 4 MiB；队列满时 SHALL 按"最旧丢弃"策略丢弃，且 SHALL NOT 阻塞写入方线程或主事件循环。
7. THE Log_Frame SHALL NOT 包含 `RustDesk_Remote_Password`、`RustDeskClientSharedSecret`、`Handshake_Frame.proof`、`Report_Frame.mac`、`Peer_Approval_Request.mac` 的本体值；含密码 / 令牌 / HMAC 字段 SHALL 在投递前替换为固定字符串 `"***"`，且 `target` 与 `message` SHALL 被截断到合理长度（建议 `target ≤ 256` 字节、`message ≤ 4 KiB`）以避免单帧过大。
8. THE RustDesk_Controlled SHALL 在桥接初始化完成后取代或抑制 RustDesk 既有的本地文件日志路径（如 `Config::log_path()` 下的 `*.log`），使日志只去往命名管道；THE RustDesk_Controlled SHALL NOT 因为本节条款而降低既有日志的级别过滤策略，过滤策略仍由 RustDesk 既有运行时 / 配置决定。
9. WHERE 编译特性 `vhd-bridge` 关闭, THE RustDesk_Controlled 的日志路径 SHALL 保持 RustDesk 既有行为（落本地文件），且 SHALL NOT 因本节条款被禁用、截流或重定向到命名管道。
10. THE 丢弃计数指标 SHALL 通过 `vhd_bridge::current_state()` 暴露的诊断信息可读，使排障人员能判断"日志缺失"是来自命名管道不可用而非未发生事件；THE 丢弃计数 SHALL NOT 通过 IPC `vhd-bridge-state` 之外的途径暴露明文。

### Requirement 19: 主控端身份校验：密码 + 服务端 ID 批准

**User Story:** 作为运维管理员，我希望 `RustDesk_Controlled` 既保留 RustDesk 既有的密码校验作为第一道门控，又在密码通过之后把主控端 ID 交给 `VHDMount` 让服务端给出批准结论，所以同一台机台既能离线接受运维主控端，也能被服务端集中纳管。

#### Acceptance Criteria

1. THE RustDesk_Controlled SHALL 沿用 RustDesk 现有的密码校验流程（参见 `src/server/connection.rs::validate_password` / `validate_password_plain` / `verify_h1`）判断主控端登录请求中的密码是否正确；本特性 SHALL NOT 修改、绕过、扩展或合成新的密码校验路径，SHALL NOT 改动现有的 2FA / hwid trusted-devices / `approve_mode` 行为。
2. WHEN 密码校验返回 `true`, THE VHD_Bridge SHALL 在 `try_start_cm(... authorized=true)` 调用之前作为额外门控插入一次 `Peer_Approval_Request`，把当前 `LoginRequest` 中的 `lr.my_id`（即 `Controller_Id`）以及连接元数据交给 `VHDMount` 进行批准查询。
3. THE Peer_Approval_Request SHALL 沿用与握手 / 上报 / 日志帧相同的"4 字节小端长度前缀 + JSON 负载"编码；JSON 字段 SHALL 至少包含 `protocol = "VHDRustDeskBridgePeerApprovalV1"`、`secretVersion (u32)`、`controlledMachineId (string, 即本机 machineId)`、`controllerId (string, 来自 lr.my_id)`、`controllerName (string, 来自 lr.my_name)`、`controllerPlatform (string, 来自 lr.my_platform)`、`controllerHwid (string, 来自 lr.hwid，可空字符串)`、`peerSocketAddr (string)`、`connectionType ("controlled" | "view-only" | "file-transfer" | "port-forward" | "terminal")`、`requestNonce (32 hex chars, 16 字节随机)`、`timestampMs (u64, Unix 毫秒)`、`mac (Base64)`。
4. THE Peer_Approval_Request.mac SHALL 由 `RustDeskClientSharedSecret` 按 `mac = HMAC-SHA256(secret, "VHDRustDeskBridgePeerApprovalV1\n" || secretVersion || "\n" || controlledMachineId || "\n" || controllerId || "\n" || sha256Hex(controllerName) || "\n" || controllerPlatform || "\n" || sha256Hex(controllerHwid) || "\n" || peerSocketAddr || "\n" || connectionType || "\n" || requestNonce || "\n" || timestampMs)` 计算，所有字段 SHALL 使用 ASCII 文本表示、行分隔符 SHALL 为 `\n`（LF）。
5. THE VHDMount SHALL 在 `Bridge_Config.request_timeout_ms` 内返回 `Peer_Approval_Response { result: "approved" | "rejected", reason?: string, ttlMs?: u64 }`；超时按 §9 的固定间隔重试策略处理，且 SHALL 视为 §19.8 描述的"桥接不可用"场景之一。
6. WHEN `Peer_Approval_Response.result = "approved"`, THE RustDesk_Controlled SHALL 让连接进入既有的 `try_start_cm` / `on_remote_authorized` 流程；IF `Peer_Approval_Response.result = "rejected"`, THEN THE RustDesk_Controlled SHALL 向主控端返回与 `crate::client::LOGIN_MSG_PASSWORD_WRONG` 等价的错误码（即即使密码正确也按"未授权"处理），关闭该连接，SHALL NOT 透露被服务端拒绝的具体原因，且 SHALL NOT 触发 §15 定义的 `Maintenance_Overlay`。
7. WHERE `Peer_Approval_Response` 携带 `ttlMs` 且取值为正整数, THE RustDesk_Controlled SHALL 把 `(controllerId, peerSocketAddr) → result` 写入仅存在于内存中的批准缓存，命中且未过期时直接放行，缓存过期或缺失时再次走 §19.2 流程；THE 批准缓存 SHALL 在进程退出后失效，SHALL NOT 持久化到磁盘，且 SHALL 在 `Bridge_Config.secret_version` 变更或用户触发"重置桥接状态"操作时立即清空。
8. WHILE `Bridge_State` 不等于 `Connected` 或 `Authorized`（即命名管道不可用 / 握手未完成 / VHDMount 离线 / `Peer_Approval_Request` 超时）, THE RustDesk_Controlled SHALL 保持 RustDesk 既有的密码校验决策作为兜底——只要密码正确就允许连接进入；该兜底行为 SHALL 适用于所有桥接不可用场景，与服务端"无论批准与否都接受凭据上报"的策略保持一致。
9. THE VHD_Bridge SHALL NOT 把 `controllerName` 与 `controllerHwid` 的明文值写入日志，日志中只允许出现 `controllerId` 的脱敏前缀（前 3 位 + `***`）与 `connectionType`；这一约束与 §10.1 / §18.7 一致。
10. THE VHD_Bridge SHALL NOT 把 `Peer_Approval_Request` 流程引入到非远控类连接（如 RustDesk 既有的"切边" / 文件传输 IPC 控制通道之外的纯本地控制流），SHALL 仅在 `validate_password` 返回 `true` 之后、`try_start_cm` 之前注入。

11. WHEN `Peer_Approval_Request` 已通过命名管道发出且尚未在 `Bridge_Config.request_timeout_ms` 内收到 `Peer_Approval_Response`, THE RustDesk_Controlled SHALL 通过 RustDesk 既有的登录响应通道向 `RustDesk_Controller` 发送 `LOGIN_MSG_VHD_APPROVAL_PENDING` 错误字面量（不会关闭连接，仅作为进度信号），使主控端可以借此弹出 `Controller_Approval_Wait_Msgbox`；该信号 SHALL 在 `Peer_Approval_Response` 返回前最多每 1 秒重复一次，避免 UI 长时间无反馈。
12. WHEN `Peer_Approval_Response.result = "rejected"`, THE RustDesk_Controlled SHALL 把 §19.6 中"与 `LOGIN_MSG_PASSWORD_WRONG` 等价的错误码"具体化为 `LOGIN_MSG_VHD_APPROVAL_REJECTED`，并 SHALL 把 `LOGIN_MSG_VHD_APPROVAL_REJECTED` 与既有的 `LOGIN_MSG_PASSWORD_WRONG` 加入 `LOGIN_ERROR_MAP` 中走 `re-input-password` 类型的路径，使主控端 UI 在"密码错误"与"未被 VHDMount 批准"之间使用同一外观但承载不同文案。
13. THE RustDesk_Controller SHALL 在 `src/client.rs::LOGIN_ERROR_MAP` 中为 `LOGIN_MSG_VHD_APPROVAL_PENDING` 注册一项 `LoginErrorMsgBox { msgtype: "wait-vhd-approval", title: "Verifying", text: "<等待 VHDMount 完成身份验证的本地化文案>", link: "", try_again: true }`，并 SHALL 在 Flutter 桌面 / 移动 UI 中为 `wait-vhd-approval` msgtype 渲染一个非阻塞、可取消的进度对话框；用户取消 SHALL 关闭该对话框并按 RustDesk 既有"取消连接"路径终止本次连接尝试。
14. THE RustDesk_Controller SHALL 在 `LOGIN_ERROR_MAP` 中为 `LOGIN_MSG_VHD_APPROVAL_REJECTED` 注册一项与 `LOGIN_MSG_PASSWORD_WRONG` 同 `msgtype` 的条目，但 `text` 文案 SHALL 与"密码错误"区分（建议本地化语义为"您的身份未被服务端批准，请联系运维"），且 `try_again` SHALL 为 `true` 以便允许用户在更换 ID / 联系运维后再次尝试。
15. THE 上述 `LOGIN_MSG_VHD_*` 常量 SHALL 与既有 `LOGIN_MSG_*` 常量同源定义于 `src/client.rs`，SHALL NOT 因 `RustDesk_Controlled` 形态裁剪 `Controlled_Initiator_Stripped` 而在 `RustDesk_Controller` 形态产物中缺失；编译特性 `vhd-bridge` 关闭时 THE 这两个字面量 SHALL 仍然定义但 SHALL NOT 被任何代码路径使用，使主控端可以在桥接关闭时也理解该错误码。

### Requirement 20: 被控端发起方功能裁剪

**User Story:** 作为安全负责人，我希望 `RustDesk_Controlled` 形态产物彻底无法主动发起远控连接，所以即便机台系统其它安全措施被攻破、攻击者拿到本地代码执行能力，也无法把这台机台当作内网横向跳板去远控其它设备。

#### Acceptance Criteria

1. THE RustDesk_Controlled 形态构建配置 SHALL 通过 `Cargo.toml` 中独立的 cargo feature（建议命名 `controlled-only`）裁剪掉 RustDesk 既有的"主控端"代码路径，使产物 SHALL NOT 链接发起方所需的 `Client::start` / `Client::reconnect` / `LoginConfigHandler` 主动登录路径、`crate::client::send_login` / `handle_login_from_ui` 等函数符号；这些符号 SHALL 在被控端形态下要么不存在，要么经条件编译收敛为编译期 `unreachable!()` 桩。
2. THE RustDesk_Controlled 形态产物 SHALL NOT 包含 RustDesk 既有的"最近连接列表"持久化与展示路径（`PeerConfig::peers` / `PeerConfig::batch_peers` / `main_load_recent_peers` / `main_load_recent_peers_for_ab` / `main_get_new_stored_peers` / `main_remove_peer`）；该形态 SHALL 在编译期裁剪 `peers/*.toml` 的写入与读取，磁盘上 SHALL NOT 出现该目录。
3. THE RustDesk_Controlled 形态产物 SHALL NOT 包含 RustDesk 既有的"收藏列表"路径（`get_fav` / `store_fav` / `main_get_fav` / `main_store_fav` / `main_load_fav_peers`），SHALL 在 Flutter UI 中移除"收藏"入口与对应路由。
4. THE RustDesk_Controlled 形态产物 SHALL NOT 包含 RustDesk 既有的"地址簿"路径（`config::Ab` / `try_get_password_from_personal_ab` / `main_load_recent_peers_for_ab`），SHALL 在编译期裁剪指向 hbbs `/api/ab/*` 接口的 HTTP 客户端调用与对应的 `LocalConfig` 键。
5. THE RustDesk_Controlled 形态产物 SHALL NOT 包含 RustDesk 既有的"主动局域网发现"路径（`crate::lan::discover` / `send_wol` / `config::LanPeers::store` / `config::LanPeers::load` / `main_load_lan_peers` / `main_get_lan_peers`），SHALL NOT 主动向局域网广播 `PeerDiscovery { cmd: "ping" }`，且 SHALL NOT 通过 `flutter::push_global_event` 推送 `load_lan_peers` 事件。
5a. THE RustDesk_Controlled 形态产物 SHALL 保留 RustDesk 既有的"被动局域网应答"路径（`crate::lan::start_listening`），使本机仍能在收到主控端广播的 `PeerDiscovery { cmd: "ping" }` 时回 `pong`，从而让同局域网的主控端发现并直连本机以走 P2P 直连而非依赖中继服务转发；THE RustDesk_Controlled SHALL 在启动时根据 RustDesk 既有的 `enable-lan-discovery` 选项默认值（`true`）开启该监听器，SHALL NOT 通过本特性条款或 `HARD_SETTINGS` 静默关闭它。
5b. THE RustDesk_Controlled SHALL 在 `start_listening` 回应 `pong` 时仅暴露 RustDesk 既有 `PeerDiscovery` 字段（`id` / `mac` / `hostname` 等），SHALL NOT 把 `Bridge_Config.secret_version`、`Bridge_State`、`controlledMachineId` 或任何与 §17 / §19 相关的桥接元数据写入 `pong` 报文。
6. THE RustDesk_Controlled 形态产物 SHALL NOT 暴露任何能让外部触发"主动连接"的 CLI 子命令或 URI scheme，包括但不限于 `--connect <id>` / `--play <id>` / `--port-forward` / `--file-transfer` / `rustdesk:<id>` 协议处理；THE 入口可执行 SHALL 在解析到上述参数时立即以非零退出码终止并 SHALL 通过 §18 的日志通道记录拒绝原因。
7. THE RustDesk_Controlled 形态产物 SHALL NOT 通过 IPC（`src/ipc.rs`）接受任何"建立新远控会话"语义的命令；既有 `Data::Connect` / `Data::SwitchSidesRequest` 等 IPC 变体 SHALL 在被控端形态下被条件编译为"立即返回错误"的桩，避免攻击者通过同机 IPC 触发主动连接。
8. THE 裁剪 SHALL 在编译期完成，SHALL NOT 仅以"运行期 UI 隐藏"的方式实现；产物中 SHALL NOT 包含与上述发起方功能对应的字符串字面量（如 "Connect"、"Recent Sessions"、"Address Book" 等仅出现于发起方 UI 的 i18n 键名）以避免被反向工程绕过。
9. THE 裁剪 SHALL NOT 影响以下"被发起方"路径：`LoginRequest` 的接收、密码校验、§19 定义的批准查询、`try_start_cm`、Flutter Connection Manager（`flutter/lib/desktop/`）、`Maintenance_Overlay`；这些 SHALL 保留完整功能。
10. WHERE 当前构建目标为 `RustDesk_Controller`, THE Cargo feature `controlled-only` SHALL 默认关闭，且本节裁剪条款 SHALL 不适用——主控端 SHALL 保留全部既有发起方能力以便运维人员使用。

### Requirement 21: 2FA 与受信设备的显式禁用

**User Story:** 作为安全负责人与发布工程师，我希望 `RustDesk_Controlled` 形态下 RustDesk 既有的 2FA / 受信设备机制被显式关闭，所以远控接受门控只有"远控密码 + §19 主控端批准"两层，运维流程不会因为额外的 OTP 输入或 hwid 受信缓存而出现意外旁路或锁机。

#### Acceptance Criteria

1. WHERE 当前构建目标为 `RustDesk_Controlled`, THE 构建配置 SHALL 通过条件编译把 `crate::auth_2fa::get_2fa` 收敛为永远返回 `None` 的桩，使 `src/server/connection.rs::Connection::require_2fa` 在该形态下恒为 `None`，且 `send_logon_response_and_keep_alive` 中 `REQUIRE_2FA` 分支 SHALL NOT 被触发。
2. WHERE 当前构建目标为 `RustDesk_Controlled`, THE 构建配置 SHALL NOT 在产物中链接 `crate::auth_2fa` 模块的 TOTP 生成、校验、Telegram bot 推送等代码路径，且产物中 SHALL NOT 出现 `LOGIN_MSG_2FA_WRONG` / `REQUIRE_2FA` 等 2FA 文案的字符串字面量被实际引用的代码。
3. WHEN `RustDesk_Controlled` 启动, THE 启动流程 SHALL 强制把 `Config` 中 `"2fa"` 与 `keys::OPTION_ENABLE_TRUSTED_DEVICES` 两个选项视为常量空值，无论持久化配置文件 / IPC 同步消息 / CLI / `HARD_SETTINGS` 中给出何种取值；任何来自上述来源的写入 SHALL 被忽略并 SHALL 以 `warn` 级别记录拒绝原因（与 §4.5 的"忽略关闭桥接的写入"风格一致）。
4. THE RustDesk_Controlled 形态产物 SHALL NOT 暴露任何"启用 2FA"、"配置受信设备"、"管理受信设备"的 UI 入口；Flutter 桌面 UI（`flutter/lib/desktop/`）中相关菜单与设置面板 SHALL 在编译期被条件剥离，与 §20.8 的字符串字面量约束一致。
5. THE RustDesk_Controlled SHALL NOT 写入或读取 `Config::get_trusted_devices` / `Config::add_trusted_device` 等持久化路径；该形态产物 SHALL 在编译期把这两类调用收敛为空操作或 `unreachable!()`，避免运行期生成 hwid 受信缓存。
6. WHEN 主控端发起的 `LoginRequest` 中携带 `hwid` 与 `tfa.code` 字段, THE RustDesk_Controlled SHALL 完全忽略它们，按 §19 既定流程仅根据"密码 + 服务端 ID 批准"决定接受或拒绝；§19.6 中"与 `LOGIN_MSG_PASSWORD_WRONG` 等价的错误码"在 2FA 字段被携带时 SHALL NOT 被替换或叠加任何 2FA 相关错误码。
7. WHERE 当前构建目标为 `RustDesk_Controller`, THE 2FA / 受信设备相关功能 SHALL 保留 RustDesk 既有行为不变，使主控端在连接非本特性范围内的常规 RustDesk 被控端时仍然支持原有的 2FA 输入路径。
8. THE Bridge_Protocol_Spec_Doc SHALL 在 §16 列出的章节中单独说明：本特性范围内的 `RustDesk_Controlled` 显式禁用 2FA / 受信设备，使 `VHDMount` 与 `VHDSelectServer` 团队理解被控端登录响应通道中 SHALL NOT 出现 `REQUIRE_2FA` / `LOGIN_MSG_2FA_WRONG` 字面量。

### Requirement 22: rustdesk-server 编译期注入与 secret.sec 开发期回退

**User Story:** 作为发布工程师与本地开发者，我希望 `build.rs` 在编译开始前就把 `HBBS_Key` / `HBBS_Host` / `HBBR_Host` 三项 RustDesk 服务端配置以"环境变量优先、`Dev_Secret_File` 回退"的统一策略前置校验并直接注入 RustDesk 既有的 `RS_PUB_KEY` / `RENDEZVOUS_SERVERS` / `relay-server` 编译期常量槽，所以三种部署形态（Controlled / Controller / RelayServer）的产物在没有运行时 `Custom_Server_Injection` 时也能默认连到正确的自建服务端，而 `Dev_Secret_File` 既能给本地开发者一键提供完整密钥包，又绝不污染 CI 与版本控制。

#### Acceptance Criteria

1. WHEN `build.rs` 启动, THE `build.rs` SHALL 在编译任何 RustDesk 源码 crate 之前执行一次 `Build_Prereq_Vars` 前置校验门，校验对象 SHALL 同时覆盖 `HBBS_Key` / `HBBS_Host` / `HBBR_Host` 三项；该前置校验 SHALL 无条件运行，SHALL NOT 依赖编译特性 `vhd-bridge` / `controlled-only` 或任何 cargo feature 的开关，使三种部署形态（`RustDesk_Controlled` / `RustDesk_Controller` / `RustDesk_RelayServer`）共享同一份默认 rendezvous / relay 配置。
2. THE `build.rs` SHALL 按下列优先级唯一确定每一项 `Build_Prereq_Vars` 的取值：环境变量 `HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST` > `Dev_Secret_File` 中对应的 `HBBS Key` / `HBBS Host` / `HBBR Host` 行；同一项目下两类来源都存在时环境变量 SHALL 胜出；CI 构建路径 SHALL 仅依赖环境变量并把它们作为 masked secret 注入，SHALL NOT 依赖 `Dev_Secret_File`。
3. THE `build.rs` SHALL 校验 `HBBS_Key` 取值为合法 base64 字符串且解码后字节长度严格等于 32；THE `build.rs` SHALL 校验 `HBBS_Host` 与 `HBBR_Host` 取值非空且可解析为 `host[:port[-port2]]` / `host[:port]` 形态（端口号若出现 SHALL 在 `[1, 65535]` 范围内，端口区间若出现 SHALL 满足 `port1 ≤ port2`）；任一校验失败 SHALL 视为该项缺失或非法。
4. IF 任一 `Build_Prereq_Vars` 项缺失（环境变量与 `Dev_Secret_File` 中对应行同时缺失）或 §22.3 校验失败, THEN THE `build.rs` SHALL 立即以非零退出码终止编译，错误信息 SHALL 同时列出（a）具体缺失或非法的项名（`HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST`）；（b）每项实际检查过的来源（环境变量名与 `Dev_Secret_File` 路径）；（c）期望的取值形态（base64 32 字节 / `host[:port[-port2]]` / `host[:port]`）；THE 错误信息 SHALL NOT 出现任何被检查值或文件正文片段。
5. THE `Dev_Secret_File` 解析器 SHALL 把 ASCII 冒号 `:` 与全角中文冒号 `：` 视为字节级等价的同一分隔符语义，SHALL 在 `<Name>` 与 `<value>` 之间允许任意 ASCII 空白（空格 / 制表符）并 trim；THE 解析器 SHALL 对未识别的行名（不属于 `HBBS Key` / `HBBS Host` / `HBBR Host` / `VHDMount Key` / `VHDMount Key Version` 五项）与空行静默忽略，SHALL NOT 因此使整体解析失败；THE 解析器 SHALL NOT 在缺失任意一项已识别行时直接报错——只有 §22.4 的前置校验门有权决定何时因缺失而终止编译。
6. THE `build.rs` SHALL 把校验通过的 `HBBS_Key` 解码后 32 字节注入到 RustDesk 既有的 `hbb_common::config::RS_PUB_KEY` 编译期常量槽（或其等价的编译期注入点），SHALL 把 `HBBS_Host` 注入到 RustDesk 既有的 `RENDEZVOUS_SERVERS` 默认列表，SHALL 把 `HBBR_Host` 注入为 RustDesk 既有 `relay-server` option 的默认值；该注入 SHALL 仅作为运行时 `Custom_Server_Injection`（文件名后缀形态或签名 base64 license 形态，由 `src/custom_server.rs` 既有路径消费）未提供值时的兜底来源，运行时一旦存在 `Custom_Server_Injection` 注入值, THE `RustDesk_Controlled` / `RustDesk_Controller` / `RustDesk_RelayServer` SHALL 优先使用运行时注入值，与 §17 既有约定保持一致。
7. THE `build.rs` SHALL 同时保证 `RustDeskClientSharedSecret` 解析路径与 `Build_Prereq_Vars` 解析路径共享同一份 `Dev_Secret_File` 解析器实现，使 `Dev_Secret_File` 单次读入即可同时回退提供 `HBBS Key` / `HBBS Host` / `HBBR Host` / `VHDMount Key` / `VHDMount Key Version` 五个值；`RustDeskClientSharedSecret` 解析的优先级 SHALL 严格为 §3.12 已声明的 `VHD_BRIDGE_SECRET_HEX` env > `VHD_BRIDGE_SECRET_B64` env > `vhd_bridge_secret.bin` 文件 > `Dev_Secret_File` 中 `VHDMount Key` 行，且 §3.2 关于 `_HEX` 与 `_B64` 同时设置即编译失败的互斥约束 SHALL 保持不变。
8. THE `build.rs` SHALL 输出 `cargo:rerun-if-env-changed=HBBS_KEY` / `cargo:rerun-if-env-changed=HBBS_HOST` / `cargo:rerun-if-env-changed=HBBR_HOST` / `cargo:rerun-if-env-changed=VHD_BRIDGE_SECRET_HEX` / `cargo:rerun-if-env-changed=VHD_BRIDGE_SECRET_B64` / `cargo:rerun-if-env-changed=VHD_BRIDGE_SECRET_VERSION`、`cargo:rerun-if-changed=secret.sec` 与 `cargo:rerun-if-changed=vhd_bridge_secret.bin`，使 cargo 在任何上述来源变更时正确重跑 `build.rs`。
9. THE `build.rs` SHALL NOT 把 `Build_Prereq_Vars` 任一项取值、`Dev_Secret_File` 任一行的取值或 `vhd_bridge_secret.bin` 内容输出到编译日志（stdout / stderr）、构建产物的字符串字面量、`.rlib` / `.exe` 元数据或 panic 信息；唯一允许出现在产物元数据 / 编译日志中的与本组变量相关的字段 SHALL 为 `Bridge_Config.secret_version`（与 §3.5 / §3.11 / §14.4 / §14.8 一致）。
10. THE `Build_Prereq_Vars` 校验门 SHALL NOT 因 `Dev_Secret_File` 中存在未识别的额外行而失败，且 SHALL NOT 把"`Dev_Secret_File` 不存在"本身视为错误——只有当环境变量与文件中对应已识别行同时缺失某一项时才触发 §22.4 的失败分支，使本地开发者既可只用环境变量、也可只用 `Dev_Secret_File`、也可二者混合，CI 工作流仅注入环境变量即可通过校验门。
