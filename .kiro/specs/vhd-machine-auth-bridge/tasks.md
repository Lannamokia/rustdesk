# Implementation Plan: vhd-machine-auth-bridge

## Overview

按"先打基础（Cargo features + build.rs + 配置项）→ 再立协议层（帧 / HMAC / 密钥）→ 再叠运行时（pipe + worker + 触发 + 日志 + 观测）→ 再接 RustDesk 既有调用点（凭据触发钩子 + 批准门控）→ 再做 UI 与裁剪（Maintenance_Overlay + controlled-only + 2FA 禁用）→ 最后协议文档与一致性测试"的顺序推进。每个父任务都把"实现"放在 `_Requirements:` 之后必做的 sub-task，把单元 / 属性 / 集成测试放在 `*` 标记的可选 sub-task；属性测试每条对应 design.md 的 Property N，并显式标注其覆盖的 Requirement 子条款。

实现语言由 design 直接指定：Rust（`src/vhd_bridge/` + `libs/hbb_common/src/config.rs` + `build.rs` + `src/ipc.rs` 钩子）与 Dart（`flutter/lib/desktop/widgets/maintenance_overlay.dart` 及配套 widget 测试）。本任务列表 SHALL NOT 引入新的 IPC / 密码学 crate，SHALL 复用 `tokio::net::windows::named_pipe`、`hmac` / `sha2` / `rand`，仅在 `feature = "vhd-bridge"` 启用时新增 `subtle` / `zeroize` 两个轻量 crate。

## Tasks

- [x] 1. 建立 Cargo features 与编译期密钥注入骨架
  - [x] 1.1 在 `Cargo.toml` 与 `libs/hbb_common/Cargo.toml` 声明 `vhd-bridge` 与 `controlled-only` features
    - 在根 `Cargo.toml` 的 `[features]` 段新增 `vhd-bridge = ["dep:subtle", "dep:zeroize"]` 与 `controlled-only = []`，并把 `subtle = { version = "2", optional = true }` / `zeroize = { version = "1", optional = true }` 加入 `[dependencies]`，仅在 `vhd-bridge` 启用时编译进依赖图
    - 在 `libs/hbb_common/Cargo.toml` 中通过 features pass-through 暴露 `vhd-bridge`（不引入新 crate，仅复用既有 `hmac` / `sha2` / `rand`）
    - 不修改 `default` features 集合，使 `RustDesk_Controller` / `RustDesk_RelayServer` 形态默认即不启用
    - _Requirements: 1.1, 1.2, 4.7, 13.3, 13.5, 14.6, 20.1, 20.10_

  - [x] 1.2 扩展 `build.rs` 为 `vhd-bridge` 形态生成 `vhd_bridge_secret.rs` 与版本号
    - 在 `build.rs` 顶层用 `#[cfg(feature = "vhd-bridge")]` 包住一段逻辑：按 `VHD_BRIDGE_SECRET_HEX` > `VHD_BRIDGE_SECRET_B64` > `vhd_bridge_secret.bin` > `secret.sec` 中 `VHDMount Key` 行的优先级解析共享密钥，校验解码后字节长度严格等于 32
    - `_HEX` 与 `_B64` 同时设置时 `eprintln!` 互斥提示并以非零退出码终止；任意来源缺失 / 长度不匹配 / 解码失败时同样非零退出，错误信息中 SHALL NOT 出现密钥取值或文件内容
    - 写出 `${OUT_DIR}/vhd_bridge_secret.rs`，内容形如 `[0xNN; 32]` 字面量，供 `secret.rs` 用 `include!()` 嵌入
    - 解析 `VHD_BRIDGE_SECRET_VERSION`（缺省回退到 `secret.sec` 中 `VHDMount Key Version` 行；若该行也缺失再回退到默认 `1`）写入第二个 include 文件，并打印 `cargo:rerun-if-env-changed=VHD_BRIDGE_SECRET_HEX` / `_B64` / `_VERSION`、`cargo:rerun-if-changed=vhd_bridge_secret.bin` 与 `cargo:rerun-if-changed=secret.sec`
    - 在 `cfg(not(feature = "vhd-bridge"))` 路径下 SHALL NOT 读取 `_HEX` / `_B64` env 与 `vhd_bridge_secret.bin`；但 `secret.sec` 解析器仍允许复用以驱动 §1.2a 的 `Build_Prereq_Vars` 校验门
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.12, 3.13, 14.1, 14.2, 14.3, 14.5, 14.8_

  - [x] 1.2a 实现 `secret.sec` 解析器（双冒号等价 + 容错忽略未识别行）
    - 在 `build.rs` 中新增一个共享解析函数 `parse_secret_sec(path: &Path) -> SecretSecMap`：按行读取 UTF-8 文本；用"找到第一个 `:` 或 `：` 的字节位置"切分 `<Name>` / `<value>`；ASCII `:` 与全角 `：` SHALL 被视为字节级等价的同一分隔符语义；trim `<Name>` 与 `<value>` 两侧的 ASCII 空白；行名大小写敏感
    - 可识别行名严格限于 `HBBS Key` / `HBBS Host` / `HBBR Host` / `VHDMount Key` / `VHDMount Key Version` 五项；未识别行、空行、文件不存在均不报错（仅返回空 `SecretSecMap`），由各自前置校验门决定是否因缺失而失败
    - 解析器 SHALL NOT 通过 `println!` / `eprintln!` 输出取值本体或文件正文片段；I/O 错误以"reason 而非 path/value"形式抛出
    - 该函数同时被 §1.2 的 `RustDeskClientSharedSecret` 解析路径与 §1.2b 的 `Build_Prereq_Vars` 前置校验门复用
    - _Requirements: 3.5, 3.12, 3.13, 14.8, 22.5, 22.7, 22.10_

  - [x] 1.2b 实现 `Build_Prereq_Vars` 前置校验门（HBBS_KEY / HBBS_HOST / HBBR_HOST，无条件运行）
    - 在 `build.rs` 顶层（**不**包在 `cfg(feature = "vhd-bridge")` 下）新增一段逻辑：按 `HBBS_KEY` env > `secret.sec` 中 `HBBS Key` 行的优先级解析 `HBBS_Key`，按 `HBBS_HOST` env > `secret.sec` 中 `HBBS Host` 行的优先级解析 `HBBS_Host`，按 `HBBR_HOST` env > `secret.sec` 中 `HBBR Host` 行的优先级解析 `HBBR_Host`
    - 校验：`HBBS_Key` 是合法 base64 且 decode 后字节长度严格等于 32；`HBBS_Host` 非空且可解析为 `host[:port[-port2]]`（端口在 `[1, 65535]`，区间需 `port1 ≤ port2`）；`HBBR_Host` 非空且可解析为 `host[:port]`
    - 任一项缺失或非法时以非零退出码终止编译，错误信息列出缺失/非法的项名（`HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST`）、每项已查过的来源（环境变量名 + `secret.sec` 路径）、期望形态；SHALL NOT 出现取值本体或文件正文片段
    - 把校验通过的取值注入到 RustDesk 既有的编译期常量槽：`HBBS_Key` 解码后 32 字节注入 `hbb_common::config::RS_PUB_KEY`（或其等价编译期注入点，例如以 `cargo:rustc-env=RS_PUB_KEY=<base64>` + `option_env!` 现有读取路径）；`HBBS_Host` 注入 `RENDEZVOUS_SERVERS` 默认列表的编译期源；`HBBR_Host` 注入既有 `relay-server` option 默认值的编译期源
    - 同时打印 `cargo:rerun-if-env-changed=HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST` 与 `cargo:rerun-if-changed=secret.sec`
    - 该校验门 SHALL 在三种部署形态（`RustDesk_Controlled` / `RustDesk_Controller` / `RustDesk_RelayServer`）上同等运行，SHALL NOT 受 `feature = "vhd-bridge"` / `controlled-only` 等任何 cargo feature 控制
    - _Requirements: 22.1, 22.2, 22.3, 22.4, 22.6, 22.8, 22.9, 22.10, 14.7, 14.8, 17.3, 17.4_

  - [x] 1.2c 为 `secret.sec` 解析器写属性测试
    - **Property 21: secret.sec parser equivalence**
    - 用 `proptest` 生成任意 (a) 任意可识别 / 未识别行子集；(b) 行内 `:` 与 `：` 任意组合；(c) 行间任意空白 / 空行 / 注释行；(d) `<value>` 两侧任意 ASCII 空白；断言：解析后已识别键值集合只取决于"行名 + value 内容"，与冒号字节形态无关；未识别行 / 空行 / 文件不存在 SHALL NOT 让解析器返回 Err；缺失某一已识别键 SHALL NOT 由解析器报错（仅可能由后续前置校验门或 `RustDeskClientSharedSecret` 解析器报错）；同一行多次出现的 `:` / `：` 仅以最左侧分隔符切分
    - **Validates: Requirements 3.12, 22.5, 22.10**
    - _Requirements: 3.12, 22.5, 22.10_


  - [x] 1.3 为 `build.rs` 密钥注入路径与 `Build_Prereq_Vars` 校验门写表驱动 EXAMPLE 测试
    - 在 `build_script_tests/secret_priority.rs`（或等效集成测试目录）中以表驱动方式覆盖共享密钥用例：仅 HEX / 仅 B64 / 仅 `vhd_bridge_secret.bin` / 仅 `secret.sec` 中 `VHDMount Key` 行 / HEX+B64 同时设置 / 长度错误 / 全部缺失 / 非法字符；断言四级优先级 `_HEX` env > `_B64` env > `vhd_bridge_secret.bin` > `secret.sec` 与 §22.7 一致
    - 同一文件再加一组 `Build_Prereq_Vars` 校验门用例：（a）`HBBS_KEY` env 缺失 ∧ `secret.sec` 中 `HBBS Key` 行缺失 → 非零退出且错误信息包含 `HBBS_KEY` 项名；（b）`HBBS_HOST` env 缺失 ∧ `secret.sec` 中 `HBBS Host` 行缺失 → 非零退出且错误信息包含 `HBBS_HOST` 项名；（c）`HBBR_HOST` env 缺失 ∧ `secret.sec` 中 `HBBR Host` 行缺失 → 非零退出且错误信息包含 `HBBR_HOST` 项名；（d）`HBBS_KEY` 取值非合法 base64 → 非零退出；（e）`HBBS_KEY` decode 后长度 ≠ 32 → 非零退出；（f）`HBBS_HOST` / `HBBR_HOST` 取值含非法端口或无法解析 → 非零退出；（g）`secret.sec` 中行用全角 `：` 与 ASCII `:` 混排 → 与全部 ASCII `:` 等价通过校验；（h）`secret.sec` 中含未识别行（如注释 `# foo` / `Random: bar`）+ 全部已识别项 → 通过校验；（i）`secret.sec` 文件不存在 ∧ 三个 env 均设置且合法 → 通过校验；（j）`VHD_BRIDGE_SECRET_VERSION` env 设置 ∧ `secret.sec` 中 `VHDMount Key Version` 也存在 → env 胜出
    - 断言每条非合法用例都得到非零退出码，错误信息中绝不出现密钥取值、`HBBS_Key` 字节、`secret.sec` 文件正文片段
    - **Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.12, 3.13, 14.1, 22.2, 22.3, 22.4, 22.5, 22.7, 22.9, 22.10**
    - _Requirements: 3.1-3.4, 3.12, 3.13, 14.1, 22.2-22.5, 22.7, 22.9, 22.10_

  - [x] 1.4 在被控端版本元数据中暴露 `secret_version`
    - 在 `RustDesk_Controlled` 形态的 `--version` 输出 / 资源段中追加一行 `vhd-bridge-secret-version=<n>`，取自 `build.rs` 注入的常量
    - `vhd-bridge` 关闭时取值固定为 `0`，与启用形态出现位置一致，便于发布工程师 diff
    - SHALL NOT 暴露密钥本体或派生值
    - _Requirements: 3.7, 3.11, 14.4_

- [x] 2. 在 `libs/hbb_common/src/config.rs` 中加入 Bridge_Config 与 IPC 观测键
  - [x] 2.1 新增 `BridgeConfig` 结构与四个可调字段
    - 在 `libs/hbb_common/src/config.rs` 内 `cfg(all(windows, feature = "vhd-bridge"))` 下定义 `BridgeConfig { pipe_name, secret_version, request_timeout_ms, retry_interval_ms }` 与默认值（pipe_name 默认 `"\\\\.\\pipe\\VHDMount.RustDeskBridge"`、`request_timeout_ms = 5000`、`retry_interval_ms = 2000`、`secret_version` 取自 `build.rs` 注入的常量）
    - SHALL NOT 包含 `enabled` / `registration_certificate_path` 等被显式禁止的字段
    - 在 `keys` 模块内新增 `VHD_BRIDGE_PIPE_NAME` / `VHD_BRIDGE_SECRET_VERSION` / `VHD_BRIDGE_REQUEST_TIMEOUT` / `VHD_BRIDGE_RETRY_INTERVAL` / `VHD_BRIDGE_STATE` 五个常量
    - _Requirements: 4.1, 4.2, 13.6_

  - [x] 2.2 实现 `try_apply_bridge_option` 守卫与字段回退
    - 增加 `pub(crate) fn try_apply_bridge_option(name: &str, value: &str)`：对 `vhd-bridge-pipe-name` 空值或非法字符回退到默认并 `log::warn!`；对 `vhd-bridge-request-timeout-ms` / `-retry-interval-ms` 在 `[1, 60_000]` 之外回退默认 + warn；对任何"关闭桥接"语义的 key（包括 `vhd-bridge-enabled` / `enable-vhd-bridge`）忽略并 `log::warn!`
    - 实现 `BridgeConfig::resolve_pipe_name(&self) -> Cow<'_, str>` 在使用点回退默认；SHALL NOT 因为字段被覆写而切到 `Disabled` / `Failed`
    - `vhd-bridge-state` 写入请求一律拒绝（只读键）
    - _Requirements: 4.4, 4.5, 4.6_

  - [x] 2.3 为 `try_apply_bridge_option` 写属性测试
    - **Property 11: Configuration writes never disable the bridge**
    - 用 `proptest` 生成任意 `(key, value)` 序列调用 `try_apply_bridge_option`，断言：(a) 不会把 `Bridge_State` 推到 `Disabled` / `Failed`；(b) 任何"kill switch" key 被丢弃且 `BridgeConfig` 字节级不变；(c) 非法 `pipe_name` 在 `resolve_pipe_name()` 处回退默认
    - **Validates: Requirements 4.2, 4.4, 4.5, 4.6**
    - _Requirements: 4.2, 4.4, 4.5, 4.6_


  - [x] 2.4 为 `BridgeConfig` 序列化写属性测试
    - **Property 13: BridgeConfig serialization never includes the shared secret**
    - 生成任意 `BridgeConfig` 实例，断言 `serde_json::to_string` 与 `src/ipc.rs` 走 bincode 的同源序列化结果中绝不出现 32 字节共享密钥（含 hex / base64 形式），唯一被暴露的密码学字段是 `secret_version`
    - **Validates: Requirements 3.7, 10.6, 13.6**
    - _Requirements: 3.7, 10.6, 13.6_

- [x] 3. 创建 `src/vhd_bridge/` 模块骨架与公共 API 占位
  - [x] 3.1 新建 `src/vhd_bridge/mod.rs` 与子模块文件
    - 创建目录 `src/vhd_bridge/`，在 `mod.rs` 内 `#[cfg(all(target_os = "windows", feature = "vhd-bridge"))] mod ...;` 声明 `config` / `secret` / `frame` / `protocol` / `hmac` / `pipe` / `worker` / `triggers` / `log_sink` / `peer_approval` / `observability` 子模块；为非 Windows / 关闭特性形态提供完整的 no-op 替身（`pub fn start(_)`、`pub fn current_state()` 返回常量 `Disabled` 等）
    - 在 `src/lib.rs` 或 `src/main.rs` 加 `pub mod vhd_bridge;`，使外部调用方在裁剪形态下无需 `cfg!` 散落
    - 该 sub-task 仅生成空 stub，便于后续 sub-task 填实
    - _Requirements: 1.2, 1.5, 14.6_

  - [x] 3.2 定义 `BridgeStateSnapshot` / `BridgeState` / `ApprovalOutcome` 等公共数据类型
    - 在 `vhd_bridge::observability`（或 `mod.rs`）声明 `pub enum BridgeState { Disabled, Initializing, Connected, Authorized, Denied, Failed }`、`pub struct BridgeStateSnapshot { state, last_reason: Option<&'static str>, secret_version: u32, log_drop_count: u64, last_change_at_ms: u64, error_code: Option<&'static str> }`、`pub enum ApprovalOutcome { Approved, Rejected, BridgeUnavailable }`
    - 在 `mod.rs` 内 `pub fn start(rt)` / `pub fn current_state()` / `pub fn reset()` / `pub fn install_log_sink()` / `pub mod triggers { pub fn notify_id_change(); pub fn notify_password_change(); pub fn notify_rotation(); }` / `pub mod peer_approval { pub async fn gate(...) }` 全部 stub 编译通过
    - _Requirements: 8.1, 12.1, 12.4, 12.5_

  - [x] 3.3 定义 `REASON_*` 常量与 `ALLOWED_ERROR_CODES` 集合
    - 在 `vhd_bridge::observability` 声明 `pub const REASON_DENY` / `REASON_RATE_LIMITED` / `REASON_INVALID_PROOF` / `REASON_INVALID_MAC` / `REASON_SECRET_OUTDATED` / `REASON_PIPE_CLOSED` / `REASON_PIPE_TIMEOUT` / `REASON_PEER_NOT_VHDMOUNT` / `REASON_VERSION_MISMATCH`
    - 定义 `ALLOWED_ERROR_CODES` 静态数组列出 `vhd.bridge.failed.secret_outdated` / `vhd.bridge.failed.peer_not_vhdmount` / `vhd.bridge.failed.version_mismatch` / `vhd.bridge.denied.deny` / `vhd.bridge.denied.rate_limited` / `vhd.bridge.denied.invalid_proof` / `vhd.bridge.denied.invalid_mac`
    - _Requirements: 12.2, 12.5_


- [x] 4. 实现 secret 访问、HMAC 输入构造与恒定时间比较
  - [x] 4.1 在 `secret.rs` 嵌入共享密钥并暴露最小作用域访问器
    - `include!(concat!(env!("OUT_DIR"), "/vhd_bridge_secret.rs"))` 拿到 `[u8; 32]` 常量，包装为 `pub(super) const SHARED_SECRET: [u8; 32]` 与 `pub(super) const SHARED_SECRET_VERSION: u32`
    - 实现 `pub(super) fn with_shared_secret<R>(f: impl FnOnce(&[u8; 32]) -> R) -> R`，仅在 HMAC 计算函数内部调用；外部任何路径 SHALL NOT 直接看到 `SHARED_SECRET`
    - 整个模块 `#[cfg(all(windows, feature = "vhd-bridge"))]`
    - _Requirements: 3.1, 3.7, 3.8, 3.11_

  - [x] 4.2 在 `hmac.rs` 实现四种协议的 HMAC 输入 builder
    - 实现 `hmac_handshake(secret_version, nonce_hex, ts_ms) -> [u8; 32]`、`hmac_report(secret_version, rust_desk_id, password_kind, password_sha256_hex, reason, reported_at, nonce) -> [u8; 32]`、`hmac_log(secret_version, level, target, message_sha256_hex, ts_ms) -> [u8; 32]`、`hmac_peer_approval(secret_version, controlled_machine_id, controller_id, controller_name_sha256_hex, controller_platform, controller_hwid_sha256_hex, peer_socket_addr, connection_type, request_nonce, ts_ms) -> [u8; 32]`
    - 拼接缓冲区按 ASCII 文本 + `\n` LF 分隔，所有数值用 `to_string()` 写出（无前导零、无 `+`），与 docs/vhd-rustdesk-bridge-protocol.md 字节级一致
    - 缓冲区在 HMAC 输出后立即 `Zeroizing<Vec<u8>>` drop；密码副本同样用 `Zeroizing<String>` 包裹
    - HMAC 计算复用 `hbb_common::hmac` / `hbb_common::sha2`，SHALL NOT 引入新密码学 crate
    - _Requirements: 5.2, 6.2, 10.3, 13.5, 18.3, 19.4_

  - [x] 4.3 实现 `ct_eq(a, b) -> bool` 恒定时间比较包装
    - 用 `subtle::ConstantTimeEq` 实现 `pub(super) fn ct_eq(a: &[u8], b: &[u8]) -> bool`：长度不等直接返回 false，长度相等走 `subtle::ConstantTimeEq::ct_eq(...).unwrap_u8() == 1`
    - 用于校验 `HandshakeResponse` / `ReportAck` / `Peer_Approval_Response` 中可能携带的对端 MAC 字段
    - SHALL NOT 用 `==` 或 `memcmp`
    - _Requirements: 3.10, 10.4_

  - [x] 4.4 为 HMAC 输入字符串写属性测试 + 文档一致性测试
    - **Property 2: HMAC input byte string matches the spec** + **Property 20: Code-vs-doc HMAC consistency**
    - 用 `proptest` 生成各协议帧的随机字段元组（含 ASCII 边界字符、空字符串、Unicode），调用 `hmac::hmac_*` 与一份独立的"按 design.md / docs 文本重构"的参考实现对比，断言 HMAC 输入字节串完全相等；同时把 `docs/vhd-rustdesk-bridge-protocol.md` 中的示例向量作为 ground truth 跑一次 round-trip
    - **Validates: Requirements 5.2, 6.2, 16.1, 16.2, 16.7, 18.3, 19.4**
    - _Requirements: 5.2, 6.2, 16.7, 18.3, 19.4_


  - [x] 4.5 为 `ct_eq` 与 zeroize 路径写属性 / 单元测试
    - **Property 3: Constant-time MAC comparison agrees with `==`**
    - 生成任意两组等长 / 不等长 byte slices，断言 `ct_eq(a, b) == (a == b)`，且不等长输入恒返回 false
    - 同时用一个自定义 allocator EXAMPLE 测试抓 dealloc 字节快照，证明 HMAC 计算后密码副本 buffer 已被覆写（best-effort 正向证据）
    - **Validates: Requirements 3.9, 3.10, 10.3, 10.4**
    - _Requirements: 3.9, 3.10, 10.3, 10.4_

- [x] 5. 实现帧编解码与协议 JSON 结构
  - [x] 5.1 在 `frame.rs` 实现 4 字节小端长度前缀 + JSON 帧 codec
    - 实现 `pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R, scratch: &mut Vec<u8>) -> io::Result<&[u8]>` 与 `pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, payload: &[u8]) -> io::Result<()>`
    - 定义 `const MAX_FRAME_BYTES: usize = 64 * 1024`；`read_frame` 在长度字段超过该上限时返回 `io::Error::new(InvalidData, "frame too large")`
    - 与 `src/ipc.rs` 现有帧风格一致（4 字节小端长度前缀），SHALL NOT 引入新 IPC crate
    - _Requirements: 2.7, 13.4, 18.2, 19.3_

  - [x] 5.2 在 `protocol.rs` 定义四种帧 + 三种响应的 serde 结构
    - 用 `#[derive(Serialize, Deserialize)]` 定义 `HandshakeFrame` / `HandshakeResponse` / `ReportFrame` / `ReportAck` / `LogFrame` / `PeerApprovalRequest` / `PeerApprovalResponse` / `RevocationFrame`，字段命名严格按 design §Data Models 中的 JSON schema（含 `protocol`、`secretVersion`、`nonce`、`timestampMs`、`mac` 等），用 `#[serde(rename_all = "camelCase")]` 与显式 `rename` 控制
    - `HandshakeResponse` / `ReportAck` / `PeerApprovalResponse` 用 `#[serde(tag = "result"|"ok", rename_all = "snake_case")]` 或 untagged enum 对齐 design 中的 `{ ok: true } / { ok: false, reason: ... }` 与 `{ result: "accepted" } / { result: "rejected", reason: ... }`
    - `passwordKind` / `reason` / `connectionType` 用 enum + `#[serde(rename_all = "snake_case")]` 暴露固定取值集合
    - _Requirements: 5.1, 6.1, 8.7, 18.2, 19.3_

  - [x] 5.3 为帧 codec 与 JSON schema 写 round-trip 属性测试
    - **Property 1: Frame round-trip preservation**
    - 用 `proptest` 生成任意 `HandshakeFrame` / `ReportFrame` / `LogFrame` / `PeerApprovalRequest` 与对应响应，断言 `serde_json::to_vec` → `frame::write_frame` → `frame::read_frame` → `serde_json::from_slice` 结构相等
    - 同时生成长度字段超过 `MAX_FRAME_BYTES` 的字节流，断言被拒为 `io::Error::InvalidData`
    - **Validates: Requirements 2.7, 5.1, 6.1, 13.4, 18.2, 19.3**
    - _Requirements: 2.7, 5.1, 6.1, 13.4, 18.2, 19.3_


- [x] 6. 实现命名管道层与对端进程身份校验
  - [x] 6.1 在 `pipe.rs` 实现 `open_and_verify`
    - 用 `tokio::time::timeout(Duration::from_millis(timeout_ms), tokio::net::windows::named_pipe::ClientOptions::new().open(pipe_name))` 控制连接耗时；超时翻译成 `ConnectError::Timeout`
    - 连接成功后立刻调用 `windows::Win32::System::Pipes::GetNamedPipeServerProcessId` 拿对端 PID，再用 `windows::Win32::System::Threading::QueryFullProcessImageNameW` 拿映像路径，校验文件名属于接受集合 `{ VHDMounter.exe, VHDMounter_<tag>.exe }`（其中 `<tag>` 非空，case-insensitive；判定函数 `is_expected_peer_image`）
    - 校验失败立即 `shutdown()` 并返回 `ConnectError::PeerNotVhdMount`，不在没有打开管道的代码路径上额外触发对端校验
    - SHALL 使用现有 Tokio 运行时；SHALL NOT 引入 `interprocess` / `parity-tokio-ipc` 等 crate
    - _Requirements: 2.1, 2.2, 2.3, 10.5, 13.2, 13.3_

  - [x] 6.2 为 `open_and_verify` 写错误分类单元测试
    - 用一个本地 `CreateNamedPipeW` 测试 server 模拟"端点不存在 / 立即 EOF / 假冒进程"三种场景，断言分别得到 `ConnectError::Io(NotFound)` / `ConnectError::Io(BrokenPipe)` / `ConnectError::PeerNotVhdMount`
    - 验证 `PeerNotVhdMount` 在 worker 中翻译为永久型 `Failed`、`Io` / `Timeout` 翻译为 `Initializing`
    - _Requirements: 2.4, 9.7, 10.5_

- [x] 7. 实现 `BridgeWorker` 状态机与重连策略
  - [x] 7.1 在 `worker.rs` 实现状态机骨架与 `tokio::sync::watch` 状态广播
    - 用 `tokio::sync::watch::channel(BridgeStateSnapshot::initial())` 作为 worker 持有的状态发布通道；`current_state()` 读 `watch::Receiver::borrow()` 同步返回，避免任何 `Mutex<BridgeState>`
    - 状态切换时调用 `transition_to(new_state, reason)`：先更新 `last_change_at_ms` / `error_code`，再 `watch::Sender::send`；`transition_to_failed(reason)` 内部 `if state == Failed { return; }` 实现"同一启动周期仅切一次 Failed"的去重
    - SHALL NOT 在 `.await` 上持锁
    - _Requirements: 8.1, 8.2, 8.3, 9.5, 12.1, 13.2_

  - [x] 7.2 实现连接 / 握手 / 重连主循环
    - `loop { open_and_verify → write Handshake_Frame → read HandshakeResponse → 分支处理 ok / deny / rate_limited / invalid_proof / secret_outdated / 超时 / I/O 错误 }`，按 design 状态图驱动转移
    - `Bridge_State` 切换到 `Connected` 后立刻投递一次 `reason = "startup"` 的内部 trigger（覆盖 §7.1 与 §8.8）
    - `secret_outdated` / `peer_not_vhdmount` / `version_mismatch` 走永久型 `Failed`；`BrokenPipe` / `ConnectionReset` / EOF / 长度错误 / JSON 解析失败 走 `Initializing`
    - `secret_outdated` 命中后即使后续读到 `BrokenPipe` 也不能把状态改回 `Initializing`（§9.8）
    - _Requirements: 2.4, 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 5.7, 5.8, 8.5, 8.6, 8.8, 9.5, 9.6, 9.7, 9.8, 10.5, 11.1, 11.2, 11.3_


  - [x] 7.3 实现固定间隔 + 抖动 + rate_limited 60s 叠加的重连延迟
    - 重试函数 `compute_retry_delay(retry_interval_ms: u32, last_reason: Option<&str>) -> Duration`：基础延迟 = `retry_interval_ms`，附加 0–200 ms 均匀分布抖动（用 `rand::thread_rng().gen_range(0..=200)`），若 `last_reason == REASON_RATE_LIMITED` 则再叠加 60_000 ms；只有收到下一次成功响应后才把"叠加延迟"标志清零
    - SHALL NOT 使用任何指数退避 / 上限放大策略；连续 5 次失败后把日志级别从 `debug` 提升到 `warn`
    - 永久型错误（`Failed`）SHALL NOT 进入重试队列
    - _Requirements: 2.5, 9.1, 9.2, 9.3, 9.4, 9.5, 9.6_

  - [x] 7.4 实现 nonce 生成 + 5 分钟窗口去重
    - 维护一个 `BTreeMap<u64 /* timestamp_ms */, [u8; 16]>` 与 `HashSet<[u8; 16]>` 双结构：每次生成 nonce 用 `rand::thread_rng().fill_bytes` + `hex::encode`，写入前清掉已经超过 5 分钟的旧条目，再断言新值不在集合内（极小概率冲突时立即重生成）
    - Report / PeerApproval 的 session 内 nonce 同样不复用，每会话开始时清空集合
    - _Requirements: 5.3, 6.3, 19.3_

  - [x] 7.5 为 nonce 生成写属性测试
    - **Property 4: Nonce non-reuse**
    - 用 `tokio::time::pause()` + `advance()` 模拟 5 分钟窗口，生成 N=200 次握手 nonce，断言全部 distinct；同样针对 session 内 M 条 Report/PeerApproval nonce 断言 distinct
    - **Validates: Requirements 5.3, 6.3, 19.3**
    - _Requirements: 5.3, 6.3, 19.3_

  - [x] 7.6 为状态机轨迹写 model-based 属性测试
    - **Property 7: State-machine integrity**
    - 用 `proptest-state-machine` 生成 (compile-time + runtime) 事件序列驱动 worker，比对实现状态轨迹与参考状态机；断言 (a) 取值集合 (b) 转移与 design 状态图一致 (c) `Disabled` 仅由编译期触发 (d) 永久型 `Failed` 不被后到的 I/O 错误抹掉 (e) `BrokenPipe` 永远走 `Initializing` (f) 首个 `accepted` 后切 `Authorized` 并 1s 内触发一次 `startup` report
    - **Validates: Requirements 2.4, 4.6, 5.5, 5.6, 5.7, 5.8, 6.4, 6.5, 6.6, 6.7, 7.1, 7.7, 8.1-8.8, 9.5-9.8, 10.5, 11.1-11.6**
    - _Requirements: 2.4, 4.6, 5.5-5.8, 6.4-6.7, 7.1, 7.7, 8.1-8.8, 9.5-9.8, 10.5, 11.1-11.6_

  - [x] 7.7 为重连延迟写属性测试
    - **Property 6: Reconnect delay is bounded fixed-interval + jitter (no exponential backoff)**
    - 给 `compute_retry_delay` 喂任意 K 次连续失败序列，断言相邻两次 delay 落在 `[retry_interval_ms, retry_interval_ms+200] ms`（无 rate_limited）或 `[retry_interval_ms+60_000, retry_interval_ms+60_200] ms`（有 rate_limited），且 delay 不随 K 增长
    - **Validates: Requirements 2.5, 9.1, 9.2, 9.3**
    - _Requirements: 2.5, 9.1, 9.2, 9.3_


- [x] 8. 实现触发器、合并窗口、心跳与上报去重
  - [x] 8.1 在 `triggers.rs` 实现 mpsc + 1s Coalescer + 30 分钟心跳
    - 用 `tokio::sync::mpsc::channel::<TriggerEvent>(32)` 维护 trigger 队列；`pub fn notify_id_change()` / `notify_password_change()` / `notify_rotation()` 全部走 `try_send`，队列满时丢**最旧** + `log::warn!`，绝不阻塞调用方
    - Coalescer：`tokio::time::interval(Duration::from_secs(1))` 推进窗口；窗口内取最后一条非 `heartbeat` 的 reason；heartbeat 与其它 reason 共存时丢 heartbeat
    - 心跳定时器 `tokio::time::interval(Duration::from_secs(30 * 60))` 始终运行；每次 tick 投递 `internal_heartbeat`，由 worker 在写入瞬间根据 `Bridge_State` 决定真发送或跳过；状态切换 SHALL NOT 重置该 interval
    - _Requirements: 7.1-7.9, 13.2_

  - [x] 8.2 在 `worker.rs` 实现 `Last_Reported_Snapshot` 去重
    - 每次收到 `ReportAck.accepted` 写 `LAST_REPORTED_SNAPSHOT = (rust_desk_id, password_kind, sha256Hex(password))`
    - 下一次触发到达：若 `reason != "heartbeat"` 且 `next_snapshot == last_reported_snapshot` 且 `state == Authorized` 则 `log::debug!("vhd_bridge: dedup skip")` 跳过；heartbeat 永远发送
    - SHALL NOT 把密码明文写入日志，使用 `sha256Hex(password)` 作为快照键
    - _Requirements: 6.8, 7.6_

  - [x] 8.3 为合并窗口写属性测试
    - **Property 8: 1-second coalescing window**
    - 用 `tokio::time::pause()` 给 Coalescer 喂任意 burst 序列，断言 1s 窗口内只 emit 一帧，`reason` 为最后一条非 heartbeat 值；纯 heartbeat burst emit 一帧 heartbeat；混合 burst emit 一帧非 heartbeat
    - **Validates: Requirements 7.2, 7.3, 7.4, 7.5, 7.8**
    - _Requirements: 7.2, 7.3, 7.4, 7.5, 7.8_

  - [x] 8.4 为心跳节奏写属性测试
    - **Property 9: Heartbeat cadence**
    - 用 `tokio::time::pause()` 模拟任意 `T` 时长，断言 heartbeat 触发数落在 `{floor(T/30min), ceil(T/30min)}`；状态短暂切到 `Denied` / `Initializing` 再恢复时定时器不被重置（仅 worker 在写入瞬间跳过）
    - **Validates: Requirements 7.6, 7.7**
    - _Requirements: 7.6, 7.7_

  - [x] 8.5 为去重逻辑写属性测试
    - **Property 5: Snapshot-identical reports are deduped (heartbeat exempted)**
    - 给 worker 喂任意触发序列（含连续相同 snapshot 的 id_change / password_change 与穿插 heartbeat），断言写入帧数 = `(# 与上次 accepted snapshot 不同的非 heartbeat 触发) + (# heartbeat 触发)`
    - **Validates: Requirements 6.8, 7.6**
    - _Requirements: 6.8, 7.6_

  - [x] 8.6 为 notify / log 非阻塞写属性测试
    - **Property 10: Caller-side notify / log is non-blocking**
    - 在 worker 持续阻塞 IPC 写入的场景下任意调用 `triggers::notify_*` 与 `log::*`，断言每次调用墙钟耗时 < 1 ms；HMAC / IPC 写入不发生在调用线程
    - **Validates: Requirements 7.9, 18.4**
    - _Requirements: 7.9, 18.4_


- [x] 9. 在 RustDesk 既有凭据生成路径上挂"观察者"通知
  - [x] 9.1 在 `Config::set_id` 写入完成后调用 `vhd_bridge::triggers::notify_id_change`
    - 在 `libs/hbb_common/src/config.rs::Config::set_id` 写入完成那一行之后追加 `#[cfg(all(target_os = "windows", feature = "vhd-bridge"))] vhd_bridge::triggers::notify_id_change();`
    - SHALL NOT 修改既有 ID 生成 / 持久化逻辑，仅以观察者形态接入
    - _Requirements: 7.2, 13.7, 14.6_

  - [x] 9.2 在密码生成路径加 `notify_password_change` / `notify_rotation`
    - `src/password_security.rs::update_temporary_password` 写入完成后调用 `notify_password_change()`
    - `src/ipc.rs` 或 `src/ui_interface.rs` 中 `set_permanent_password_with_ack` 成功路径调用 `notify_password_change()`
    - `check_update_temporary_password`（认证失败次数达到阈值时旋转）路径调用 `notify_rotation()`
    - 全部以 `cfg(all(windows, feature = "vhd-bridge"))` 包裹，每处仅追加一行
    - _Requirements: 7.3, 7.4, 7.5, 13.7, 14.6_

- [x] 10. 实现日志 Sink：log crate 接管 + bounded mpsc + 脱敏
  - [x] 10.1 在 `log_sink.rs` 实现 `install_log_sink`
    - 用 `OnceCell<mpsc::Sender<LogEvent>>` 在进程入口由 `vhd_bridge::start` 调用一次安装；同名第二次调用 `set_boxed_logger().ok()` 静默忽略
    - `tokio::spawn(log_writer_task(rx))` 串行写入命名管道；`Log::log` 内 `try_send` 队列满时 `LOG_DROP.fetch_add(1, Ordering::Relaxed)` 并丢最旧（用一个内部 ring buffer 实现 bounded 容量 4096 / 4 MiB）
    - `Bridge_State` ∉ {`Connected`, `Authorized`} 时静默丢弃日志事件，并递增 `LOG_DROP`，绝不回写本地文件 / stderr / Windows Event Log
    - 编译特性 `vhd-bridge` 关闭时整个 sink 不安装，沿用既有 `flexi_logger` / `env_logger`
    - _Requirements: 18.1, 18.4, 18.5, 18.6, 18.8, 18.9_

  - [x] 10.2 实现 `redact_message` 与字段截断
    - 实现 `pub(super) fn redact_message(msg: &mut String)`：覆盖 `password` / `temporary_password` / `mac` / `proof` / `secret` / `hwid` / `controllerName` / `controllerHwid` 等字段名后紧跟 `=` 或 `:` 直到下一个空白或终止符为止的取值，替换为 `***`
    - `target` 截断到 256 字节、`message` 截断到 4 KiB
    - SHALL NOT 输出 `RustDeskClientSharedSecret` / `proof` / `mac` / 密码本体；唯一允许的密码学相关字段是 `secret_version`
    - 实现 `controllerId` 的"前 3 位 + `***`"脱敏 helper，供 §19 / §10 处使用
    - _Requirements: 10.1, 10.2, 12.3, 18.7, 19.9_


  - [x] 10.3 抑制既有本地文件日志 sink
    - 在桥接初始化完成后，把 RustDesk 既有 `Config::log_path()` 下的本地 `*.log` 写入路径条件编译为 no-op（仅 `vhd-bridge` 启用时生效），使日志只去往命名管道
    - SHALL NOT 降低既有日志级别过滤策略
    - _Requirements: 18.1, 18.8_

  - [x] 10.4 为日志队列写属性测试
    - **Property 19: Bounded log queue, oldest-drop, monotone counter**
    - 用 `proptest` 喂任意 N 条日志事件，断言队列容量 ≤ 4096 条 / 4 MiB；满时丢最旧；`LOG_DROP` 单调递增且等于丢弃数；调用方永不被反压阻塞
    - **Validates: Requirements 18.5, 18.6, 18.10**
    - _Requirements: 18.5, 18.6, 18.10_

  - [x] 10.5 为脱敏写属性测试
    - **Property 12: Log / IPC / config redaction**
    - 用 `proptest` 生成任意含密码 / proof / mac / controllerName / controllerHwid / 共享密钥派生值的 `LogEvent` / `BridgeStateSnapshot` / config-sync entry，断言序列化字节中 SHALL NOT 出现这些敏感字段的本体值；密码必须被替换为 `"***"`；`controllerId` 仅以前 3 位 + `***` 出现；唯一允许的密码学相关字段是整数 `secret_version`
    - **Validates: Requirements 3.5, 3.7, 3.11, 10.1, 10.2, 10.3, 10.6, 12.3, 18.7, 19.9**
    - _Requirements: 3.5, 3.7, 3.11, 10.1, 10.2, 10.3, 10.6, 12.3, 18.7, 19.9_

- [x] 11. 实现 Peer Approval Gate 与 LOGIN_MSG_VHD_* 注册
  - [x] 11.1 在 `peer_approval.rs` 实现 `gate(&lr, peer_addr, conn_type) -> ApprovalOutcome`
    - 状态守卫：`Bridge_State` ∉ {`Connected`, `Authorized`} 时立即返回 `ApprovalOutcome::BridgeUnavailable`
    - 缓存查询：`OnceCell<Mutex<HashMap<(String /* controllerId */, SocketAddr), ApprovalCacheEntry>>>`，`ttlMs` 来自上一次 `Peer_Approval_Response.ttlMs`，为 0 / 未设置则不写缓存；查询作用域只持锁到 `lookup` 完成立即 drop 再 await
    - 通过 `mpsc::Sender<ApprovalRequest>` 把请求送给 worker；`oneshot::Receiver` 等待 `Peer_Approval_Response`，超时 / 通道断开映射成 `BridgeUnavailable`
    - `Bridge_Config.secret_version` 变更或 `vhd_bridge::reset()` 时 `clear()` 缓存
    - SHALL NOT 跨 `.await` 持 `Mutex<ApprovalCache>` 锁
    - _Requirements: 19.2, 19.3, 19.4, 19.5, 19.6, 19.7, 19.8_

  - [x] 11.2 在 `worker.rs` 处理 Peer_Approval_Request 帧
    - 在 worker `tokio::select!` 多路事件中加 approval 分支：收到 `ApprovalRequest` 后写 `PeerApprovalRequest` 帧到管道、读 `PeerApprovalResponse`、把结果通过 `oneshot` 回送给 `gate()`
    - 写入耗时 / 读取错误一律映射成 `BridgeUnavailable`，绝不在 `gate()` 路径上把 `Bridge_State` 推到 `Failed`
    - _Requirements: 19.2, 19.5, 19.8_


  - [x] 11.3 在 `src/client.rs` 注册 `LOGIN_MSG_VHD_*` 常量与 `LOGIN_ERROR_MAP` 项
    - 与既有 `LOGIN_MSG_*` 同源在 `src/client.rs` 声明 `pub const LOGIN_MSG_VHD_APPROVAL_PENDING: &str = "VHD Approval Pending";` 与 `pub const LOGIN_MSG_VHD_APPROVAL_REJECTED: &str = "VHD Approval Rejected";`
    - 在 `LOGIN_ERROR_MAP` 中 insert `LOGIN_MSG_VHD_APPROVAL_PENDING -> LoginErrorMsgBox { msgtype: "wait-vhd-approval", title: "Verifying", text: "Waiting for VHDMount to verify your identity", link: "", try_again: true }` 与 `LOGIN_MSG_VHD_APPROVAL_REJECTED -> LoginErrorMsgBox { msgtype: "re-input-password", title: LOGIN_MSG_VHD_APPROVAL_REJECTED, text: "Your identity is not approved by the operator. Please contact ops", link: "", try_again: true }`
    - 这两个常量在 `controlled-only` 与 `vhd-bridge` 关闭形态下也定义；编译特性关闭时 SHALL NOT 被任何代码路径使用
    - _Requirements: 19.12, 19.13, 19.14, 19.15_

  - [x] 11.4 在 `src/server/connection.rs::validate_password → try_start_cm` 路径注入批准钩子
    - 在 `validate_password()` 返回 `true` 之后、`try_start_cm(.., authorized=true)` 之前用 `cfg(all(windows, feature = "vhd-bridge"))` 包裹一段：派生 `connection_type`、读 `peer_addr`、`spawn_pending_pump()` 每 1s 推送一次 `LOGIN_MSG_VHD_APPROVAL_PENDING` 进度信号、`vhd_bridge::peer_approval::gate(...)` 拿 `ApprovalOutcome`、`pending_pump.stop()`
    - `Approved` / `BridgeUnavailable` 走既有 `try_start_cm(.., authorized=true)` 路径；`Rejected` 走 `send_login_error(LOGIN_MSG_VHD_APPROVAL_REJECTED)` + `try_start_cm(.., authorized=false)` 后 return（与"密码错误"等价的 `re-input-password` msgtype），且 SHALL NOT 触发 §15 的 `Maintenance_Overlay`
    - 仅以最小钩子接入，SHALL NOT 修改 `validate_password` / `validate_password_plain` / `verify_h1` 实现
    - _Requirements: 14.6, 19.1, 19.2, 19.6, 19.10, 19.11_

  - [x] 11.5 实现 `spawn_pending_pump`
    - `tokio::spawn` + `tokio::time::interval(Duration::from_secs(1))` 在等待 `Peer_Approval_Response` 期间通过登录响应通道每秒推送一次 `LOGIN_MSG_VHD_APPROVAL_PENDING`（不关闭连接，仅作为进度信号）
    - 通过 `oneshot::Sender<()>` + `tokio::select!` 让调用方 `stop()` 时立即结束
    - _Requirements: 19.11, 19.13_

  - [x] 11.6 为入向连接决策表写属性测试
    - **Property 14: Inbound-connection decision is decoupled from `Bridge_State`**
    - 用 `proptest` 生成任意 `(Bridge_State, validate_password_outcome, peer_approval_outcome, lr.hwid, lr.tfa.code)` 元组，断言决策完全等于：(a) password=false → reject `LOGIN_MSG_PASSWORD_WRONG`；(b) password=true ∧ approval ∈ {Approved, BridgeUnavailable} → `try_start_cm(authorized=true)`；(c) password=true ∧ approval=Rejected → reject `LOGIN_MSG_VHD_APPROVAL_REJECTED`；`Bridge_State` / `lr.hwid` / `lr.tfa.code` 在 `controlled-only` 下绝不影响决策
    - **Validates: Requirements 8.4, 11.6, 19.6, 19.8, 21.1, 21.2, 21.3, 21.5, 21.6**
    - _Requirements: 8.4, 11.6, 19.6, 19.8, 21.1, 21.2, 21.3, 21.5, 21.6_


- [x] 12. 检查点：核心 Rust 桥接路径的实现 + 测试通过
  - Ensure all tests pass, ask the user if questions arise.

- [x] 13. 实现 `vhd-bridge-state` IPC 观测键与 `current_state()` API
  - [x] 13.1 在 `observability.rs` 维护 `BridgeStateSnapshot` watch
    - 把 `tokio::sync::watch::Sender<BridgeStateSnapshot>` 的写入集中到 `transition_to`、`record_accepted`、`log_drop_count_inc` 等少数路径
    - `last_change_at_ms` 用 `SystemTime::now().duration_since(UNIX_EPOCH).as_millis()` 写入；`error_code` 仅在状态切到 `Failed` / `Denied` 时由查表函数 `reason_to_error_code(reason)` 给出固定字符串
    - _Requirements: 12.1, 12.2, 12.4, 12.5, 18.10_

  - [x] 13.2 在 `src/ipc.rs` 加 `Data::VhdBridgeState(BridgeStateSnapshot)` 与 `vhd-bridge-state` 配置键消费分支
    - 在 `Data` enum 加新变体 `VhdBridgeState(BridgeStateSnapshot)`（仅 `cfg(all(windows, feature = "vhd-bridge"))`）
    - 在 IPC `Data::Config(("vhd-bridge-state", None))` 查询分支直接读 `watch::Receiver::borrow().clone()` 序列化回复，不阻塞
    - 该键写入请求一律拒绝
    - _Requirements: 4.5, 12.4, 13.4_

  - [x] 13.3 实现 `vhd_bridge::reset()`
    - 清空 `LAST_REPORTED_SNAPSHOT` 与 `APPROVAL_CACHE`，通过 `RESET_SIGNAL_TX.send(())` 通知 worker 关闭当前会话并把状态切回 `Initializing`
    - 在 `Bridge_Config.secret_version` 运行时变更分支也调用一次 `reset()`
    - _Requirements: 11.4, 11.5, 19.7_

  - [x] 13.4 为 observability 快照写属性测试
    - **Property 15: Observability snapshot shape**
    - 用 `proptest` 驱动任意状态序列，断言 `vhd-bridge-state` IPC 输出 (a) 仅含 `{state, reason, secretVersion, logDropCount, lastChangeAtMs, errorCode}` 六个 key；(b) `state` 在合法集合内；(c) `reason` 在 `REASON_*` ∪ {null}；(d) `state ∈ {Failed, Denied}` 时 `errorCode` 必非空且在 `ALLOWED_ERROR_CODES`；(e) `logDropCount` 单调非降
    - **Validates: Requirements 12.2, 12.4, 12.5, 18.10**
    - _Requirements: 12.2, 12.4, 12.5, 18.10_

- [x] 14. 在 `src/core_main.rs` / `src/main.rs` 进程入口启动桥接
  - [x] 14.1 调用 `vhd_bridge::start(rt)` 与 `install_log_sink()`
    - 在 `core_main` 进入 Tokio 运行时之后立即调用 `vhd_bridge::install_log_sink()`（取代或抑制既有 `flexi_logger`）与 `vhd_bridge::start(tokio::runtime::Handle::current())`
    - `start()` 内部 `tokio::spawn` BridgeWorker / Coalescer / 心跳定时器；进程整个生命周期保持桥接任务为活动状态
    - SHALL NOT 创建嵌套 Tokio 运行时；SHALL NOT 在 async 上下文中调用 `Runtime::block_on`
    - _Requirements: 4.3, 13.2, 13.3, 18.4_


- [x] 15. 实现 Maintenance_Overlay Flutter widget 与 IPC 绑定
  - [x] 15.1 新增 `flutter/lib/desktop/widgets/maintenance_overlay.dart`
    - 通过现有 `bind` API 订阅 `active-session-count`（新增由 `Data::ControlledSessionCount` 驱动）与 `vhd-bridge-state` 两个 IPC 键
    - 当 `active_session_count >= 1` 时在所有 `Window.screens` 上同时实例化全屏 topmost overlay：背景为半透明深色 + 模糊；中央显示中英双语提示文案（i18n key `vhd_overlay_title` / `vhd_overlay_detail`），以及 `LivenessIndicator` widget
    - 当 `active_session_count` 由 ≥1 降为 0 时，1 秒内移除所有显示器上的 overlay 并恢复 §15.1 描述的隐身状态
    - 在 `flutter/lib/desktop/screen/desktop_overlay_screen.dart` 路由中加载该 widget；通过 dart 条件 import + `--dart-define VHD_OVERLAY=on` 在 `controlled-only` 形态独占编译
    - _Requirements: 15.1, 15.2, 15.4, 15.6, 15.8, 15.9_

  - [x] 15.2 实现 `LivenessIndicator` 与独立 Isolate 心跳
    - 用 `AnimatedBuilder` + `Ticker` 在 UI Isolate 渲染旋转加载圈
    - 用 `Isolate.spawn` 起一个独立 isolate 跑 `Timer.periodic(Duration(seconds: 1))`，把递增的相位数通过 `SendPort` 发回主 isolate；主 UI 线程被阻塞或 GPU 不可用时仍以 ≥1 fps 推进相位
    - _Requirements: 15.5_

  - [x] 15.3 在 Windows 平台实现 low-level keyboard hook 与本地输入屏蔽
    - `MouseRegion(opaque: true)` + `Focus(autofocus: true)` 吞掉 overlay 区域内的鼠标 / 焦点事件
    - 平台层（Windows）通过 channel 调用 `SetWindowsHookExW(WH_KEYBOARD_LL)` 注册 low-level keyboard hook，吞掉所有按键（除 `Ctrl+Alt+Del` 等 OS 强制保留组合键）；overlay 隐藏 / 销毁时 `UnhookWindowsHookEx`
    - 远程协议输入（`enigo` 路径 / RustDesk 既有 input pipe）SHALL NOT 走该钩子，保证远控发起方下发的输入仍然被传递
    - _Requirements: 15.7_

  - [x] 15.4 i18n 文案补齐 `vhd_overlay_*` 中英 key
    - 在 RustDesk 既有 i18n 文件中新增 `vhd_overlay_title` / `vhd_overlay_detail` / `vhd_bridge_err_*` 等中英文 key（与 design §Data Models 中本地化错误码表一致）
    - _Requirements: 12.5, 15.4_

  - [x] 15.5 为 Maintenance_Overlay 写 widget property-based test（glados）
    - **Property 16: Maintenance_Overlay visibility** + **Property 18: Local input is blocked except system keys; remote input passes through**
    - 用 `glados` 生成任意 `(active_session_count, has_view_only_flag, peer_approval_outcome)` 历史，断言 (a) overlay 可见性等价于 `count(t) >= 1`；(b) 可见时 topmost / non-resizable / non-minimizable 且覆盖工作区 + taskbar；(c) `count: ≥1 → 0` 后 1 秒内不可见；(d) `Rejected` 路径绝不实例化 overlay
    - 同时验证本地按键 ∉ SYSTEM_RESERVED_KEYS 时不被传递到桌面应用栈，远程协议输入路径仍然抵达
    - **Validates: Requirements 15.1, 15.2, 15.3, 15.6, 15.7**
    - _Requirements: 15.1, 15.2, 15.3, 15.6, 15.7_


  - [x] 15.6 为 Liveness_Indicator 写主线程阻塞场景属性测试
    - **Property 17: Liveness indicator ≥ 1 fps under main-thread stalls**
    - 模拟主线程阻塞 D ∈ [0, 5] 秒，断言 isolate 驱动的相位计数在该窗口内推进 ≥ floor(D) 次
    - **Validates: Requirements 15.5**
    - _Requirements: 15.5_

  - [x] 15.7 在 `src/server/connection.rs::on_remote_authorized` 路径递增 `active-session-count`
    - 仅在 §11.4 流程返回 `Approved` 或 `BridgeUnavailable` 后由 `try_start_cm(.., authorized=true)` + `on_remote_authorized` 触发 `Data::ControlledSessionCount(n)` IPC 推送，使被拒绝连接对本机用户保持完全无感
    - 会话数降为 0 时同样推送一次，触发 overlay 隐藏
    - _Requirements: 15.2, 15.3, 15.6, 15.8_

- [x] 16. 检查点：Maintenance_Overlay 集成可见性符合 §15
  - Ensure all tests pass, ask the user if questions arise.

- [x] 17. 实现 controlled-only 形态发起方代码裁剪
  - [x] 17.1 裁剪 CLI 子命令 `--connect` / `--play` / `--port-forward` / `--file-transfer` / `--rdp` / `--view-camera` / `--terminal` 与 `rustdesk:` URI scheme
    - 在 `src/main.rs` / `src/core_main.rs` 子命令解析路径用 `cfg(feature = "controlled-only")` 命中分支替换为 `log::warn!("vhd_bridge: refused initiator subcommand {arg}");` + `std::process::exit(2);`
    - URI scheme 处理路径同样以非零退出 + 日志方式拒绝
    - _Requirements: 1.6, 20.6_

  - [x] 17.2 裁剪发起方核心代码路径
    - 用 `cfg(feature = "controlled-only")` 把 `Client::start` / `Client::reconnect` / `LoginConfigHandler` / `crate::client::send_login` / `handle_login_from_ui` 等函数收敛为编译期 `unreachable!()` 桩或不存在
    - SHALL 保留 §19 / §20.9 列出的"被发起方"路径不变（`LoginRequest` 接收、密码校验、`try_start_cm`、Flutter Connection Manager、`Maintenance_Overlay`）
    - _Requirements: 1.6, 20.1, 20.9_

  - [x] 17.3 裁剪 IPC `Data::Connect` / `Data::SwitchSidesRequest` 等"建立新会话"语义变体
    - 在 `src/ipc.rs` 这些变体的 handler 用 `cfg(feature = "controlled-only")` 立即返回 `Data::CmErr("controlled-only build refuses initiator IPC")` 后断开
    - SHALL NOT 删除变体定义（以免影响共用结构体），仅 handler 走拒绝分支
    - _Requirements: 20.7_

  - [x] 17.4 裁剪最近 / 收藏 / 地址簿 / 主动 LAN 发现持久化与读取路径
    - 在 `cfg(feature = "controlled-only")` 下：`PeerConfig::peers` / `PeerConfig::batch_peers` / `main_load_recent_peers` / `main_load_recent_peers_for_ab` / `main_get_new_stored_peers` / `main_remove_peer` / `get_fav` / `store_fav` / `main_get_fav` / `main_store_fav` / `main_load_fav_peers` / `config::Ab` / `try_get_password_from_personal_ab` / `crate::lan::discover` / `crate::lan::send_wol` / `config::LanPeers::store|load` / `main_load_lan_peers` / `main_get_lan_peers` 全部走 `unreachable!()` 或编译剔除
    - 磁盘上 SHALL NOT 出现 `peers/*.toml` 目录；hbbs `/api/ab/*` 接口 HTTP 客户端调用与对应 `LocalConfig` 键全部裁剪
    - SHALL NOT 通过 `flutter::push_global_event` 推送 `load_lan_peers` 事件
    - _Requirements: 20.2, 20.3, 20.4, 20.5_


  - [x] 17.5 保留被动 LAN 应答 `start_listening` 且不暴露桥接元数据
    - 在 `cfg(feature = "controlled-only")` 下 `crate::lan::start_listening` 保留并按 `enable-lan-discovery` 默认值（`true`）开启监听器
    - `pong` 报文严格只暴露既有 `PeerDiscovery` 字段（`id` / `mac` / `hostname`），SHALL NOT 写入 `Bridge_Config.secret_version` / `Bridge_State` / `controlledMachineId` 或任何 §17 / §19 桥接元数据
    - _Requirements: 20.5a, 20.5b_

  - [x] 17.6 裁剪 Flutter 发起方路由与 i18n 字符串
    - 在 `flutter/lib/...` 通过 dart 条件 import 把"最近列表 / 收藏 / 地址簿 / 局域网发现 / Connect 入口"路由切到 `pages/empty_*.dart` 占位实现，避免发起方 UI 字符串字面量被链入 `controlled-only` 产物（`"Connect"` / `"Recent Sessions"` / `"Address Book"` 等 i18n key 不可被引用）
    - _Requirements: 20.8_

  - [x] 17.7 为 controlled-only 形态写产物 SMOKE 测试脚本
    - 在 `scripts/check_controlled_only.ps1` 中：构建 `RustDesk_Controlled` 形态产物后，`grep` 二进制不应出现 `Client::start` / `LoginConfigHandler` / `main_load_recent_peers` / `main_load_lan_peers` / `Connect` / `Recent Sessions` / `Address Book` 等发起方专属符号 / 字面量
    - CLI 子命令 `--connect <id>` / `--play <id>` / `rustdesk:foo` 立即非零退出
    - **Validates: Requirements 1.6, 20.1, 20.2, 20.3, 20.4, 20.5, 20.6, 20.8**
    - _Requirements: 1.6, 20.1-20.8_

- [x] 18. 实现 2FA / Trusted Devices 显式禁用
  - [x] 18.1 裁剪 `crate::auth_2fa::get_2fa` 与 `Connection::require_2fa`
    - `cfg(feature = "controlled-only")` 下 `crate::auth_2fa::get_2fa` 收敛为永远返回 `None`；其它 TOTP 路径与 Telegram bot 推送代码用 `cfg(not(feature = "controlled-only"))` 包住
    - `Connection::require_2fa` 在该形态下永远 `None`，使 `send_logon_response_and_keep_alive` 中 `REQUIRE_2FA` 分支不可达
    - 产物中 `LOGIN_MSG_2FA_WRONG` / `REQUIRE_2FA` 等 2FA 文案的字符串字面量 SHALL NOT 被实际引用代码段链入
    - _Requirements: 21.1, 21.2_

  - [x] 18.2 在 `Config` 入口拒写 `"2fa"` / `OPTION_ENABLE_TRUSTED_DEVICES`
    - `cfg(feature = "controlled-only")` 下 `Config::set_option` 增加守卫：对 `"2fa"` 与 `keys::OPTION_ENABLE_TRUSTED_DEVICES` 写入直接 `log::warn!("controlled-only: ignoring write to {}", name)` 后丢弃；read 路径恒返回空值
    - 启动流程把这两个选项视为常量空值，无论持久化 / IPC / CLI / `HARD_SETTINGS` 给出何种取值
    - _Requirements: 21.3_

  - [x] 18.3 裁剪 `Config::get_trusted_devices` / `add_trusted_device` 与 UI 入口
    - `cfg(feature = "controlled-only")` 下 `Config::get_trusted_devices` / `add_trusted_device` / `enable_trusted_devices` 收敛为空操作或 `unreachable!()`，避免运行期生成 hwid 受信缓存
    - Flutter 桌面 UI 中"启用 2FA" / "配置受信设备" / "管理受信设备"菜单与设置面板在编译期被条件剥离
    - 在 `LoginRequest.hwid` / `lr.tfa.code` 字段被携带时按 §19 既定流程仅根据"密码 + 服务端 ID 批准"决定接受或拒绝；§19.6 错误码 SHALL NOT 被替换或叠加任何 2FA 错误码
    - _Requirements: 21.4, 21.5, 21.6_


  - [x] 18.4 为 2FA / 受信设备 SMOKE 写测试脚本
    - 在 `scripts/check_no_2fa_strings.ps1` 中 `grep` `RustDesk_Controlled` 形态产物，断言 `LOGIN_MSG_2FA_WRONG` / `REQUIRE_2FA` 等字面量不被实际引用代码段引用；`get_trusted_devices` 等符号未链入
    - 构造一份带 `lr.tfa.code = "123456"` 的 `LoginRequest`，断言被控端忽略该字段、按密码 + 批准决策
    - **Validates: Requirements 21.2, 21.4, 21.5, 21.6**
    - _Requirements: 21.2, 21.4, 21.5, 21.6_

- [x] 19. 检查点：controlled-only 与 2FA 禁用裁剪验证
  - Ensure all tests pass, ask the user if questions arise.

- [x] 20. 编写 Bridge_Protocol_Spec_Doc 协议规范文档
  - [x] 20.1 创建 `docs/vhd-rustdesk-bridge-protocol.md`
    - 文档头部章节：上游服务端仓库 `https://github.com/rustdesk/rustdesk-server`（`VHD_Self_Hosted_Server_Repo`）、RustDesk 既有自定义服务端注入路径（`Custom_Server_Injection`：`src/custom_server.rs::CustomServer { host, key, api, relay }`、文件名后缀与签名 base64 license 两种形态）、共享密钥两种 CI 注入方式（`VHD_BRIDGE_SECRET_HEX` / `VHD_BRIDGE_SECRET_B64` / `vhd_bridge_secret.bin`）与 `VHD_BRIDGE_SECRET_VERSION` 升级流程
    - 端点定义章节：命名管道路径、字节序、4 字节小端长度前缀 + JSON 负载、`MAX_FRAME_BYTES = 64 KiB`
    - _Requirements: 16.1, 16.3, 16.4, 17.6_

  - [x] 20.2 撰写四种帧 + 三种响应 + Revocation 帧的字段约束与 HMAC 输入示例
    - `Handshake_Frame` / `VHDRustDeskBridgeHandshakeV1`：字段约束 + HMAC 输入字符串（"VHDRustDeskBridgeHandshakeV1\n" || secretVersion || "\n" || nonce || "\n" || timestampMs"）+ 至少一组示例向量
    - `Report_Frame` / `VHDRustDeskBridgeReportV1`：字段约束 + HMAC 输入字符串 + 示例向量；明文密码出现在 JSON 负载、`sha256Hex(password)` 出现在 HMAC 输入
    - `Log_Frame` / `VHDRustDeskBridgeLogV1`：字段约束 + HMAC 输入 + 示例向量
    - `Peer_Approval_Request` / `VHDRustDeskBridgePeerApprovalV1`：字段约束 + HMAC 输入 + 示例向量；`controllerName` / `controllerHwid` 走 `sha256Hex(...)` 在 HMAC 输入中、JSON 负载明文
    - `HandshakeResponse` / `ReportAck` / `PeerApprovalResponse` / `Revocation` 全部响应类型与 reason 集合（`ok` / `deny` / `rate_limited` / `invalid_proof` / `secret_outdated` / `accepted` / `rejected` / `invalid_mac` / `approved` / `denied` 等）
    - 共享密钥所有示例 SHALL 用占位符（`"<32 random bytes>"` / `"REDACTED"`），SHALL NOT 暴露真实密钥本体
    - _Requirements: 5.1, 5.2, 6.1, 6.2, 16.2, 16.5, 18.2, 18.3, 19.3, 19.4_

  - [x] 20.3 撰写时间窗 / nonce 防重 / 协议错误码 / 兼容性 / round-trip 示例 / 2FA 禁用声明章节
    - 时间窗：握手 5 分钟有效窗口；nonce 防重 5 分钟窗口；HMAC 算法版本 = HMAC-SHA256；`secret_version` / `protocol` 升级双端行为约定
    - 协议错误码集合与建议本地处理行为表（与 design §Data Models 中本地化错误码表对齐）
    - 至少一组完整 round-trip 示例：握手 + 上报 + 心跳 + 日志 + 主控端批准
    - 单独章节声明：`RustDesk_Controlled` 显式禁用 2FA / 受信设备，登录响应通道中 SHALL NOT 出现 `REQUIRE_2FA` / `LOGIN_MSG_2FA_WRONG` 字面量
    - 与本特性源代码同 PR 提交，便于交叉评审
    - _Requirements: 16.2, 16.5, 16.6, 16.7, 21.8_


  - [x] 20.4 写 markdown 章节齐全性 EXAMPLE 测试
    - 用一个简单 markdown parser 单元测试断言 `docs/vhd-rustdesk-bridge-protocol.md` 含全部必需章节标题（端点定义 / Handshake / Report / Log / PeerApproval / 响应类型 / 错误码 / 兼容性 / round-trip 示例 / 2FA 禁用声明）
    - 同时 grep 确认 SHALL NOT 出现真实密钥（仅占位符）
    - **Validates: Requirements 16.2, 16.5**
    - _Requirements: 16.2, 16.5_

- [x] 21. CI 与产物字符串扫描
  - [x] 21.1 在 `.github/workflows/` 加 `vhd-bridge` 矩阵
    - Windows runner 上 `cargo test --features vhd-bridge,controlled-only` 跑全部 PBT + Unit + Integration
    - 把 `HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST`（`Build_Prereq_Vars`，三种部署形态构建均需要）与 `VHD_BRIDGE_SECRET_HEX` / `VHD_BRIDGE_SECRET_B64` / `VHD_BRIDGE_SECRET_VERSION`（仅 `RustDesk_Controlled` 形态构建需要）一同列为 GitHub Actions secrets，工作流文件中 SHALL NOT 以明文出现这六个变量的取值，且 CI 步骤 SHALL NOT 通过 `echo` / `set -x` / `printenv` 等方式把它们打印到任务日志
    - `vhd_bridge_secret.bin` 与 `secret.sec` 列入 `.gitignore` 与 secret 输入清单，CI runner 上 SHALL NOT 出现该文件——`HBBS_*` / `VHD_BRIDGE_SECRET_*` 全部走环境变量路径，由 `build.rs` 的双源 resolver 在 env 命中时直接采用
    - _Requirements: 14.5, 14.7, 14.8, 22.2_

  - [x] 21.2 加 Controller / Relay 形态产物字符串扫描脚本与 RS_PUB_KEY 一致性检查
    - `cargo build --no-default-features --features flutter` 构建 Controller 产物；`scripts/check_bridge_strings.ps1` `grep` 二进制不应出现 `VHDRustDeskBridgeHandshakeV1` / `VHDRustDeskBridgeReportV1` / `VHDRustDeskBridgeLogV1` / `VHDRustDeskBridgePeerApprovalV1` / `RustDeskClientSharedSecret`，命中即 fail-fast
    - 同一脚本对三种形态产物（Controlled / Controller / Relay）跑负向扫描：二进制中 SHALL NOT 出现 `secret.sec` 任意行的明文字面量子串（包括 `VHDMount Key` hex 取值的 ASCII 形态、`HBBS Key` base64 取值字符串本体、`VHDMount Key Version` 的十进制字符串），仅允许 `Bridge_Config.secret_version` 整数出现在 `--version` 资源段；命中即 fail-fast
    - 同一脚本对三种形态产物跑正向校验：从 `RS_PUB_KEY` / `RENDEZVOUS_SERVERS` 默认列表 / `relay-server` 默认值常量段（即 §1.2b 注入到的位置）解出当前编译期值，断言与 `HBBS_KEY` env（CI 路径）或 `secret.sec` 中 `HBBS Key` / `HBBS Host` / `HBBR Host` 行（本地路径）base64-decode 后字节级一致——这是把"secret.sec 的真实落点是 RS_PUB_KEY 等既有常量"作为机械可验证不变量
    - `cargo build --no-default-features` 构建 Relay 产物，同上扫描与正向校验
    - `cargo deny check` / `cargo tree` 守住 `subtle` / `zeroize` 仅在 `vhd-bridge` 启用下进入依赖图
    - _Requirements: 1.2, 14.2, 14.3, 14.7, 14.8, 22.6, 22.9_

  - [x] 21.3 写 `feature_off_parity` SMOKE 测试
    - 构建 `RustDesk_Controlled` 形态产物且 `vhd-bridge` 关闭，断言启动行为与未集成桥接时一致（启动时间 / 内存占用 / 既有协议交互在合理误差内）
    - **Validates: Requirements 1.7, 13.1**
    - _Requirements: 1.7, 13.1_

- [x] 22. 集成测试：真命名管道 end-to-end
  - [x] 22.1 在测试中起一个真实 named pipe server
    - 用 `windows::Win32::System::Pipes::CreateNamedPipeW` 在 Windows runner 上起一个测试 server，按 design §BridgeWorker 控制流序列回复 HandshakeResponse / ReportAck / PeerApprovalResponse / Revocation
    - 跑 `RustDesk_Controlled` 进程的桥接代码完成 1 次 "握手 + startup 上报 + 心跳跳过 + 日志帧 + Peer_Approval_Request → approved + rejected" 完整往返
    - 断言 `Bridge_State` 轨迹与 design 一致；断言 `LOGIN_MSG_VHD_APPROVAL_PENDING` 进度信号每 1s 推送一次直到响应到达
    - _Requirements: 5.5, 6.5, 7.1, 7.6, 18.1, 19.5, 19.6, 19.11_


  - [x] 22.2 集成测试：永久型错误进入 `Failed` 不可恢复
    - 测试 server 回 `HandshakeResponse { ok: false, reason: "secret_outdated" }`，断言 worker 切到 `Failed` 后即使后续 `BrokenPipe` / `ConnectionReset` 也不切回 `Initializing`；只有 `secret_version` 变更或 `vhd_bridge::reset()` 才能恢复
    - 测试 `peer_not_vhdmount`：测试 server 起在 `notepad.exe` 进程下（或假冒映像名），断言 worker 立刻切到 `Failed` 且只切一次
    - _Requirements: 5.6, 9.5, 9.6, 9.8, 10.5, 11.2_

  - [x] 22.3 集成测试：rate_limited 叠加 60s 延迟
    - 测试 server 第一次回 `rate_limited`，第二次回 `ok`；断言两次 attempt 间隔落在 `[retry_interval_ms + 60_000, retry_interval_ms + 60_200] ms`，第二次成功后"叠加延迟"标志清零
    - _Requirements: 9.2_

- [x] 23. 最终检查点：所有测试通过 + 协议文档与代码一致性核对
  - Ensure all tests pass, ask the user if questions arise.
  - 跑一遍 Property 20（doc_consistency）确认 `docs/vhd-rustdesk-bridge-protocol.md` 与实现 HMAC 输入字节级一致
  - 跑一遍产物字符串扫描脚本确认 Controller / Relay 形态零残留
  - 跑一遍 `feature_off_parity` SMOKE 确认 `vhd-bridge` 关闭形态与未集成时一致

## Notes

- 全部 sub-task 已统一为必做（用户显式启用了所有可选任务）；测试 / SMOKE / 文档辅助测试与核心实现等同对待，全部需要在对应 wave 内完成
- 每个 sub-task 都引用具体 Requirement 子条款（细到 N.M），方便 traceability
- 属性测试每条对应 design.md 中的 Property N，标题与 design 一致
- 检查点（任务 12 / 16 / 19 / 23）是阶段性验证锚点，按"先核心 Rust → 再 Flutter UI → 再裁剪 → 最后协议文档与一致性"的顺序推进
- 由于 design 显式选定 Rust + Dart，本任务列表 SHALL NOT 触发"实现语言选择"分支
- 桥接代码集中在 `src/vhd_bridge/`、`build.rs`、`libs/hbb_common/src/config.rs` 与 `src/ipc.rs` 的最小钩子，加上 `src/server/connection.rs::validate_password → try_start_cm` 一处批准门控钩子；其它文件仅靠 `cfg(feature = "controlled-only")` / `cfg(feature = "vhd-bridge")` 守卫做条件编译

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "1.2a", "20.1"] },
    { "id": 1, "tasks": ["1.2b", "1.2c", "1.3", "1.4", "2.1", "3.1", "20.2"] },
    { "id": 2, "tasks": ["2.2", "3.2", "3.3", "4.1", "5.1", "5.2", "20.3"] },
    { "id": 3, "tasks": ["2.3", "2.4", "4.2", "4.3", "5.3", "6.1", "20.4"] },
    { "id": 4, "tasks": ["4.4", "4.5", "6.2", "7.1", "8.1", "10.1", "11.3", "13.1"] },
    { "id": 5, "tasks": ["7.2", "7.3", "7.4", "8.2", "10.2", "10.3", "11.1", "13.2", "13.3"] },
    { "id": 6, "tasks": ["7.5", "7.6", "7.7", "8.3", "8.4", "8.5", "8.6", "10.4", "10.5", "11.2", "13.4", "9.1", "9.2"] },
    { "id": 7, "tasks": ["11.4", "11.5", "14.1", "15.1", "15.2", "15.3", "15.4"] },
    { "id": 8, "tasks": ["11.6", "15.5", "15.6", "15.7", "17.1", "17.2", "17.3", "17.4", "17.5", "17.6", "18.1", "18.2", "18.3"] },
    { "id": 9, "tasks": ["17.7", "18.4", "21.1", "21.2", "22.1"] },
    { "id": 10, "tasks": ["21.3", "22.2", "22.3"] }
  ]
}
```
