# VHDMount ⇄ RustDesk_Controlled 桥接协议规范

> 状态：**草案** —— 与 RustDesk 的 `vhd-machine-auth-bridge` feature 同步产出。
> 权威来源：本文与 `.kiro/specs/vhd-machine-auth-bridge/`（requirements / design）必须**字节级一致**。如果实现层暴露出不一致，两边在同一个 PR 内更新（Requirement 16.7）。

本文档是同一台 Windows 主机上两个组件之间的契约：

- `RustDesk_Controlled` —— 启用 `vhd-bridge` cargo feature 编出的 RustDesk 构建，作为命名管道的**客户端**。
- `VHDMount` —— 独立分发的 C# 程序，掌控 TPM 访问以及与 `VHDSelectServer` 的全部通信，作为命名管道的**服务端**。

`VHDMount` 与 `VHDSelectServer` 团队**必须**能够在不读 RustDesk Rust 源码的前提下完成自己一侧的实现与评审。

---

## 1. Overview & Scope · 概述与范围

`RustDesk_Controlled` 在 TPM 绑定的机器身份这件事上是**观察者**，不是守门人：

- 远程控制接受策略由 RustDesk 既有的 `validate_password` 加上一次次级 `Peer_Approval_Request` 往返决定；命名管道不可用时退化为「密码正确 = 允许」（Requirement 8 / 19）。
- 所有 TPM 操作、所有到 `VHDSelectServer` 的 HTTPS 调用、所有签名都在 `VHDMount` 内部完成。RustDesk 永远看不到 TPM 密钥材料，永远不向服务端发起 HTTPS，也永远不持久化注册证书。
- 管道上承载四个请求帧（Handshake / Report / Log / PeerApproval）、对应的三种响应形态、以及一个服务端发起的 Revocation 帧（见 §5–§9）。

本文档范围：

- 传输、组帧、HMAC 构造、帧 schema、响应形态、错误码、时间窗、防重放、版本兼容、往返示例、2FA 禁用。

不在范围：

- TPM 配置、`VHDSelectServer` HTTP API、`VHDMount` 内部存储、注册 UX。这些由 `VHDMount` 仓库与 `machine-auth.md` spec 负责。

### 上游 RustDesk 服务器（`VHD_Self_Hosted_Server_Repo`）

`RustDesk_Controlled` 的 rendezvous（`hbbs`）与 relay（`hbbr`）服务器来自上游：

- `https://github.com/rustdesk/rustdesk-server`

本仓库（RustDesk 客户端）**不**附带也不重写该服务端代码。运维方自托管 hbbs / hbbr 时直接使用上游仓库，并通过下文 §1.1 描述的 `Custom_Server_Injection` 路径在 RustDesk 客户端构建期注入 host / key / relay / api（Requirement 17.1, 17.2）。

本文描述的 `VHD_Bridge` 命名管道协议与 hbbs/hbbr 选择**正交**。Bridge 不读 `custom-rendezvous-server`、不读 `relay-server` / `api-server`，也不会主动向 hbbs / hbbr 发起任何网络调用（Requirement 17.5）。

### 1.1 Custom_Server_Injection —— RustDesk 既有的服务端注入路径

RustDesk 有一套已存在的稳定机制，可以在打包时把 `hbbs` / `hbbr` / `api-server` 烤进 Windows 二进制。它位于 `src/custom_server.rs`，对外暴露：

```rust
pub struct CustomServer {
    pub key:   String, // hbbs public key (base64)
    pub host:  String, // hbbs host[:port[-port2]]
    pub api:   String, // optional api-server URL
    pub relay: String, // hbbr host[:port]
}
```

支持两种等价的注入形式（在本 feature 之前就已经实现）：

1. **文件名后缀形式** —— 在二进制名 `.exe` 之前追加逗号分隔的键值列表：

   ```text
   rustdesk-host=<hbbs-host>,key=<hbbs-pubkey>,relay=<hbbr-host>,api=<api-url>.exe
   ```

   `host=` **必须**第一个出现；后续字段可选。Windows 重命名重复文件（如 `rustdesk (1).exe`）由 parser 容忍。

2. **签名 base64 license 字符串形式** —— 一段 base64 编码的 JSON `CustomServer` blob，用打包期的 license 私钥签名，由 `get_license_from_string` 解码并对照内嵌的 ed25519 公钥验签。

`RustDesk_Controlled` 的 CI 构建**应**填入 `host` / `key` / `relay` / `api` 中部署实际用到的子集，且**必须不**在产物中嵌入任何 license **私**签名材料（Requirement 17.7）。

注入的值由 RustDesk 既有运行时通过 `Config::get_rendezvous_servers()`、`Config::get_option("relay-server")`、`Config::get_option("api-server")`、以及 `src/rendezvous_mediator.rs::get_relay_server` 消费。`VHD_Bridge` 不引入任何新的服务端选择路径。

### 1.2 共享密钥注入（`RustDeskClientSharedSecret`）与版本号

两端管道对帧的认证统一用 32 字节密钥 `RustDeskClientSharedSecret` 上的 HMAC-SHA256。该密钥**编译期**注入 RustDesk 二进制并通过带外渠道下发给 `VHDMount`；它**永远不**走线，也**永远不**出现在任何日志中。

RustDesk CI 构建按以下优先级（高优先级在前）解析密钥：

1. `VHD_BRIDGE_SECRET_HEX` 环境变量 —— 64 个十六进制字符，解码后正好 32 字节。
2. `VHD_BRIDGE_SECRET_B64` 环境变量 —— 标准 base64，解码后正好 32 字节。
3. 仓库根目录的 `vhd_bridge_secret.bin` 文件 —— 正好 32 字节裸数据。
4. 仓库根目录的 `secret.sec` 文件中 `VHDMount Key: <hex>` 行 —— 64 个十六进制字符，解码后正好 32 字节。

`VHD_BRIDGE_SECRET_HEX` 与 `VHD_BRIDGE_SECRET_B64` **必须不**同时设置；冲突、缺源、长度不匹配、解码失败时 `build.rs` 以非零状态退出（Requirement 3.1–3.6, 14.1–14.5）。

版本号 `VHD_BRIDGE_SECRET_VERSION`（`u32`，默认 `1`）先从 env 解析，再从 `secret.sec` 的 `VHDMount Key Version: <decimal>` 行解析。它内嵌进每帧的 `secretVersion` 字段，让两端能够检测密钥轮换，且是密钥相关的**唯一**允许出现在产物元数据中的字段（Requirement 14.6, 14.8）。

#### 推荐的轮换流程

1. 运维生成新的 32 字节密钥（例如 `openssl rand -hex 32`）。
2. 运维把 `VHD_BRIDGE_SECRET_VERSION` 加到 `N+1`。
3. CI 用新的 `_HEX` / `_B64` / `.bin` 与新的版本号重建 `RustDesk_Controlled`。
4. `VHDMount` 在独立的发行通道上同步使用同一个新密钥 + 版本号。
5. 任一端仍跑旧版本时会收到 `HandshakeResponse { ok: false, reason: "secret_outdated" }`（§5），进入永久 `Failed`，直到二进制更新为止。设计上**不存在**带内的轮换协商 —— `VHDMount` 是「当前接受的版本号」的唯一权威。

`vhd_bridge_secret.bin` 与 `secret.sec` 都在 gitignore 中。这两个文件**不允许**出现在 CI runner 上；CI **只**通过 env 注入。

---

## 2. Transport & Endpoint Definition · 传输与端点定义

### 2.1 端点

- **OS**：Windows。
- **管道路径**（UTF-16）：`\\.\pipe\VHDMount.RustDeskBridge`。
- **服务端**：`VHDMount`。它持有管道，使用 `CreateNamedPipeW` 创建实例。
- **客户端**：`RustDesk_Controlled`。使用 `tokio::net::windows::named_pipe::ClientOptions` 连接。
- **方向**：全双工，请求/响应。客户端发送 Handshake → Report / Log / PeerApproval；服务端发送响应，并**可**主动推送 `Revocation`（§9）。
- **进程身份校验**：`CreateFile` 成功后，客户端调用 `GetNamedPipeServerProcessId` 解析服务端进程映像路径；如果映像不是预期的 `VHDMount` 二进制，客户端按永久错误（`peer_not_vhdmount`）处理，**进程不重启就不再重试**（Requirement 5.6, 11.2）。

### 2.2 字节序与编码

- 线上的多字节整数字段（帧长度前缀）一律**小端序**。
- 所有 JSON 载荷一律 **UTF-8**（不带 BOM）。
- HMAC 输入一律 **ASCII** 文本；字段分隔符是 `\n`（0x0A，**仅 LF** —— 不带 CR）。HMAC 输入中的十进制整数**不带前导零、不带 `+` 号**（例如 `1730000000000`，不是 `+1730000000000` 也不是 `01730000000000`）。

### 2.3 帧编码

管道上的每一帧 —— 请求或响应 —— 都使用同一个外壳：

```text
+----------------+---------------------------------------------+
| 4 bytes  (LE)  | N bytes JSON payload                        |
| u32 length=N   | UTF-8, no BOM, no trailing NUL              |
+----------------+---------------------------------------------+
```

- 4 字节长度前缀是 JSON 载荷的字节长度，小端编码。
- `MAX_FRAME_BYTES = 64 KiB`（65 536）。长度前缀超过此值**必须**被拒绝：接收方按 `InvalidData` 关闭会话，客户端侧带退避回到 `Initializing`（Requirement 13.4，design §"帧编解码"）。
- 外壳层**不带**消息级校验和 —— 帧完整性由 JSON 载荷里每帧自带的 HMAC（§3）端到端保证。
- 传输层**不带**保活机制。活性由应用级的 `heartbeat` Report（§6）每 30s 一次观察。

### 2.4 连接生命周期（信息性）

```text
client                                          server
  | --- CreateFile \\.\pipe\VHDMount.RustDeskBridge -->
  | <-- pipe connected --
  | -- GetNamedPipeServerProcessId, verify image --
  | --- Handshake_Frame (request) ----------------->
  | <-- HandshakeResponse --------------------------
  | --- Report_Frame   reason=startup ------------->
  | <-- ReportAck                                   --
  | --- Report_Frame   reason=heartbeat (every 30s) ->
  | <-- ReportAck                                   --
  | --- Log_Frame      (as-needed)                 ->
  | <-- (no response) -- (Log frames are fire-and-forget per design)
  | --- Peer_Approval_Request (per inbound login) ->
  | <-- Peer_Approval_Response                     --
  |                                                 |
  | <== Revocation (server-pushed, any time) ======|
```

详细的状态转换与错误分类表见 design §"BridgeWorker 状态机" —— 此处**不**重复，避免漂移。

---

## 3. HMAC-SHA256 Construction Rules · HMAC-SHA256 构造规则

> 由 **task 20.2**（每帧具体输入）与 **task 20.3**（测试向量）填充。

适用于全部四种帧的通用规则：

- **算法**：固定 HMAC-SHA256。**不**协商算法。未来迁移通过新的 `protocol` 字符串表达（如 `…V2`），不在本版本范围。
- **密钥**：32 字节的 `RustDeskClientSharedSecret`（§1.2）。两端持有相同密钥。
- **输入串编码**：ASCII 字节；字段分隔符为单个 `\n`（0x0A）。
- HMAC 输入中的**整数**为十进制 ASCII，无前导零，无 `+` 号。
- **`secretVersion`** 作为协议标签后的第一个整数字段，进入每个 HMAC 输入，使版本不一致的两端即使其它字段全部一致也得到不匹配的 MAC。
- HMAC 输入中的 **`sha256Hex(x)`** 是 `SHA-256(x as UTF-8 bytes)` 的小写十六进制（64 字符）。用于那些明文出现在 JSON 载荷中、但**不能**出现在 HMAC 字符串里的字段（密码、控制者显示名、hwid）—— 见 §6 / §8 的精确清单。
- **MAC 在线上的编码**：得到的 32 字节摘要以标准 base64（带 `=` padding）写入 JSON 的 `mac` / `proof` 字段。

速查 —— 各帧的 HMAC 输入串（`\n` 分隔，无尾随换行）：

| 帧 | HMAC 输入（`\n` = 0x0A） |
| --- | --- |
| `VHDRustDeskBridgeHandshakeV1` | `"VHDRustDeskBridgeHandshakeV1\n" \|\| secretVersion \|\| "\n" \|\| nonce \|\| "\n" \|\| timestampMs` |
| `VHDRustDeskBridgeReportV1` | `"VHDRustDeskBridgeReportV1\n" \|\| secretVersion \|\| "\n" \|\| rustDeskId \|\| "\n" \|\| passwordKind \|\| "\n" \|\| sha256Hex(password) \|\| "\n" \|\| reason \|\| "\n" \|\| reportedAt \|\| "\n" \|\| nonce` |
| `VHDRustDeskBridgeLogV1` | `"VHDRustDeskBridgeLogV1\n" \|\| secretVersion \|\| "\n" \|\| level \|\| "\n" \|\| target \|\| "\n" \|\| sha256Hex(message) \|\| "\n" \|\| timestampMs` |
| `VHDRustDeskBridgePeerApprovalV1` | `"VHDRustDeskBridgePeerApprovalV1\n" \|\| secretVersion \|\| "\n" \|\| controlledMachineId \|\| "\n" \|\| controllerId \|\| "\n" \|\| sha256Hex(controllerName) \|\| "\n" \|\| controllerPlatform \|\| "\n" \|\| sha256Hex(controllerHwid) \|\| "\n" \|\| peerSocketAddr \|\| "\n" \|\| connectionType \|\| "\n" \|\| requestNonce \|\| "\n" \|\| timestampMs` |

每条输入的字节级完整构造在 §5 / §6 / §7 / §8 中与该帧的 JSON schema 一并原样重列。Revocation 帧（§9）有自己的 `mac` 字段及不同输入 —— 详见 §9。

---

## 4. Frame Catalog · 帧目录

| § | 帧                                  | 方向                     | 响应                          |
| - | ---------------------------------- | ------------------------ | ---------------------------- |
| 5 | `VHDRustDeskBridgeHandshakeV1`     | Controlled → VHDMount    | `HandshakeResponse`          |
| 6 | `VHDRustDeskBridgeReportV1`        | Controlled → VHDMount    | `ReportAck`                  |
| 7 | `VHDRustDeskBridgeLogV1`           | Controlled → VHDMount    | （无 —— fire-and-forget）   |
| 8 | `VHDRustDeskBridgePeerApprovalV1`  | Controlled → VHDMount    | `Peer_Approval_Response`     |
| 9 | `VHDRustDeskBridgeRevocationV1`    | VHDMount → Controlled    | （无 —— 服务端推送）        |

---

## 5. Handshake Frame — `VHDRustDeskBridgeHandshakeV1`

`RustDesk_Controlled` 在 `CreateFile` 成功并完成服务端映像校验（§2.1, §2.4）后作为**第一帧**发送。权威 schema：design §"Handshake_Frame / VHDRustDeskBridgeHandshakeV1"；接受准则：Requirements §5。

### 5.1 JSON schema

```json
{
  "protocol":      "VHDRustDeskBridgeHandshakeV1",
  "secretVersion": 1,
  "nonce":         "<32 hex chars, 16 random bytes>",
  "timestampMs":   1730000000000,
  "clientKind":    "rustdesk",
  "clientVersion": "1.4.6",
  "proof":         "<Base64(HMAC-SHA256)>"
}
```

| 字段 | 类型 | 约束 |
| --- | --- | --- |
| `protocol` | string | **必须**严格等于字面量 `"VHDRustDeskBridgeHandshakeV1"`。其它任何值**必须**被 `VHDMount` 拒绝。 |
| `secretVersion` | u32 | 当前注入的 `VHD_BRIDGE_SECRET_VERSION`（§1.2）。十进制。 |
| `nonce` | string | 小写十六进制，正好 32 字符，编码 16 字节密码学随机数。同一 `secretVersion` 的任意 5 分钟窗口内**必须不**重用（Requirement 5.3）。 |
| `timestampMs` | u64 | 帧构造时刻的 Unix 毫秒。服务端有效窗口为 `|now - timestampMs| ≤ 300000`（Requirement 5.4）。 |
| `clientKind` | string | **必须**严格等于 `"rustdesk"`。 |
| `clientVersion` | string | RustDesk 产品版本字符串，例如 `"1.4.6"`。自由格式 ASCII；**不**进入 HMAC 输入。 |
| `proof` | string | §5.2 定义的 32 字节 HMAC-SHA256 摘要的标准字母表 base64（`=` padding）。 |

### 5.2 HMAC 输入

```
"VHDRustDeskBridgeHandshakeV1\n" || secretVersion || "\n" || nonce || "\n" || timestampMs
```

`secretVersion` 与 `timestampMs` 写为十进制 ASCII，无前导零、无 `+` 号。`\n` 仅 LF（0x0A） —— 不带 CR。**无**尾随换行。

注意 `clientKind`、`clientVersion` 以及 `proof` 自身**不**进入 HMAC 输入。`clientKind` / `clientVersion` 只是给 `VHDMount` 审计日志用的咨询性元数据；篡改它们不会让 `proof` 失效，但 `VHDMount` 可自由拒绝 `clientKind != "rustdesk"` 的帧。

### 5.3 `HandshakeResponse`（VHDMount → Controlled）

```json
{ "ok": true }
```

```json
{ "ok": false, "reason": "deny" }
{ "ok": false, "reason": "rate_limited" }
{ "ok": false, "reason": "invalid_proof" }
{ "ok": false, "reason": "secret_outdated" }
```

`reason` 语义（依 design §"BridgeWorker 状态机"）：

| `reason` | `Bridge_State` 转换 | 恢复 |
| --- | --- | --- |
| （缺省 / `ok: true`） | `Initializing → Connected` | 继续发送 Report |
| `deny` | `Initializing → Denied` | 固定重连间隔后重试 |
| `rate_limited` | `Initializing → Denied` | 固定间隔 + 60s 后重试 |
| `invalid_proof` | `Initializing → Denied` | 固定间隔后重试（多半是时钟漂移或密钥注入 bug）|
| `secret_outdated` | `Initializing → Failed`（永久） | 需要二进制或 `secret_version` 更新；**不带**带内重试 |

既无 `ok: true` 又无可识别 `reason` 的响应**必须**作为协议错误处理（带退避回到 `Initializing`）。

### 5.4 完整示例

> 本例使用密钥 `RustDeskClientSharedSecret = "<32 random bytes>"`（REDACTED —— 真实值**永远不**应出现在本文档中，依 Requirement 16.5）。其它字段值是具体的。

输入：

- `secretVersion = 1`
- `nonce = "4f1c2a8b39d0e7561f8a2b3c4d5e6f70"`（16 字节随机数，小写十六进制）
- `timestampMs = 1730000000000`

HMAC 输入（裸字节，`\n` 显示为 `\n`；无尾随换行）：

```
VHDRustDeskBridgeHandshakeV1\n1\n4f1c2a8b39d0e7561f8a2b3c4d5e6f70\n1730000000000
```

便于校验方使用的 Python 字面量：

```python
b"VHDRustDeskBridgeHandshakeV1\n1\n4f1c2a8b39d0e7561f8a2b3c4d5e6f70\n1730000000000"
```

得到的线上 JSON 载荷（下面显示的 `proof` 是用占位/REDACTED 密钥算的，**不权威** —— **必须**用实际注入的 `RustDeskClientSharedSecret` 重算）：

```json
{
  "protocol":      "VHDRustDeskBridgeHandshakeV1",
  "secretVersion": 1,
  "nonce":         "4f1c2a8b39d0e7561f8a2b3c4d5e6f70",
  "timestampMs":   1730000000000,
  "clientKind":    "rustdesk",
  "clientVersion": "1.4.6",
  "proof":         "<Base64(HMAC-SHA256(RustDeskClientSharedSecret, <input above>))>"
}
```

线上的帧使用 §2.3 的标准外壳：上面 JSON 的 UTF-8 编码的 4 字节小端长度前缀，紧跟该 JSON。

---

## 6. Report Frame — `VHDRustDeskBridgeReportV1`

`RustDesk_Controlled` 在 `Bridge_State ∈ {Connected, Authorized}` 期间发送，把当前 `(rustDeskId, password)` 推给 `VHDMount`。权威 schema：design §"Report_Frame / VHDRustDeskBridgeReportV1"；接受准则：Requirements §6。

### 6.1 JSON schema

```json
{
  "protocol":      "VHDRustDeskBridgeReportV1",
  "secretVersion": 1,
  "rustDeskId":    "123456789",
  "passwordKind":  "temporary",
  "password":      "Hunter2!",
  "reason":        "startup",
  "reportedAt":    1730000000000,
  "nonce":         "9a8b7c6d5e4f30210011223344556677",
  "mac":           "<Base64(HMAC-SHA256)>"
}
```

| 字段 | 类型 | 约束 |
| --- | --- | --- |
| `protocol` | string | **必须**严格等于 `"VHDRustDeskBridgeReportV1"`。 |
| `secretVersion` | u32 | 十进制。与本连接最近一次成功握手所用值相同。 |
| `rustDeskId` | string | `Config::get_id()` 返回的 RustDesk peer ID（数字或主机名样式）。 |
| `passwordKind` | enum string | `"temporary"` / `"permanent"` / `"preset"` / `"absent"` 之一。 |
| `password` | string | UTF-8 明文密码。`passwordKind == "absent"` 时为**空字符串**。明文交付给 `VHDMount`，因为 `VHDMount` 是负责重新签名并转发到 `VHDSelectServer` 的实体；明文**必须不**在 RustDesk 端被记日志（Requirement 18.7）。 |
| `reason` | enum string | `"startup"` / `"id_change"` / `"password_change"` / `"rotation"` / `"heartbeat"` 之一。 |
| `reportedAt` | u64 | 帧构造时刻的 Unix 毫秒。 |
| `nonce` | string | 小写十六进制，正好 32 字符（16 字节随机）。**必须**在单次连接会话内唯一（Requirement 6.3）。与握手 `nonce` 不同。 |
| `mac` | string | §6.2 定义的 32 字节 HMAC-SHA256 摘要的标准字母表 base64。 |

### 6.2 HMAC 输入

```
"VHDRustDeskBridgeReportV1\n" || secretVersion || "\n" || rustDeskId || "\n" ||
passwordKind || "\n" || sha256Hex(password) || "\n" || reason || "\n" ||
reportedAt || "\n" || nonce
```

关键：**明文** `password` 只出现在 JSON 载荷中。HMAC 输入采用 `sha256Hex(password)` —— 即 `SHA-256(password as UTF-8 bytes)` 的小写 64 字符十六进制。这样即便 HMAC 输入在审计追踪中被复现，密码也不会留在 HMAC 日志里。Requirement 6.2 强制此构造。

`secretVersion` 与 `reportedAt` 写为十进制 ASCII（无前导零、无 `+`）。`\n` 仅 LF。

当 `passwordKind == "absent"` 时，password 字段为空串 `""`，`sha256Hex("")` 是众所周知的常量 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`。

### 6.3 `ReportAck`（VHDMount → Controlled）

```json
{ "result": "accepted" }
```

```json
{ "result": "rejected", "reason": "deny" }
{ "result": "rejected", "reason": "rate_limited" }
{ "result": "rejected", "reason": "secret_outdated" }
{ "result": "rejected", "reason": "invalid_mac" }
```

`reason` 语义：

| `result` / `reason` | `Bridge_State` 影响 | 恢复 |
| --- | --- | --- |
| `"accepted"`（首次） | `Connected → Authorized` | 继续 heartbeat / 事件驱动报告 |
| `"accepted"`（后续） | 不变（保持 `Authorized`） | 刷新 `Last_Reported_Snapshot` 缓存 |
| `rejected` / `deny` | `→ Denied` | 固定间隔后重试 |
| `rejected` / `rate_limited` | `→ Denied` | 固定间隔 + 60s 后重试 |
| `rejected` / `invalid_mac` | `→ Denied` | 固定间隔后重试（暗示注入 / 时钟 / 比特翻转） |
| `rejected` / `secret_outdated` | `→ Failed`（永久） | 需要二进制或 `secret_version` 更新 |

JSON 既不匹配 accepted 也不匹配 rejected 形态的 `ReportAck` 视为协议错误，重置会话（带退避回到 `Initializing`）。

### 6.4 完整示例

> 密钥：`RustDeskClientSharedSecret = "<32 random bytes>"`（REDACTED，依 Requirement 16.5）。

输入：

- `secretVersion = 1`
- `rustDeskId = "123456789"`
- `passwordKind = "temporary"`
- `password = "Hunter2!"`（仅明文出现在线上）
- `sha256Hex("Hunter2!") = 607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe`
- `reason = "startup"`
- `reportedAt = 1730000000000`
- `nonce = "9a8b7c6d5e4f30210011223344556677"`

HMAC 输入（裸字节；`\n` 显示为 `\n`；无尾随换行）：

```
VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nstartup\n1730000000000\n9a8b7c6d5e4f30210011223344556677
```

Python 字面量：

```python
b"VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nstartup\n1730000000000\n9a8b7c6d5e4f30210011223344556677"
```

线上 JSON 载荷（下面显示的 `mac` 是占位 —— `VHDMount` **必须**用实际注入的密钥重算）：

```json
{
  "protocol":      "VHDRustDeskBridgeReportV1",
  "secretVersion": 1,
  "rustDeskId":    "123456789",
  "passwordKind":  "temporary",
  "password":      "Hunter2!",
  "reason":        "startup",
  "reportedAt":    1730000000000,
  "nonce":         "9a8b7c6d5e4f30210011223344556677",
  "mac":           "<Base64(HMAC-SHA256(RustDeskClientSharedSecret, <input above>))>"
}
```

---

## 7. Log Frame — `VHDRustDeskBridgeLogV1`

由 `RustDesk_Controlled` 以 fire-and-forget 方式发送，把已经做过脱敏的 `log` crate 事件转发给 `VHDMount` 集中存储。**无**逐帧响应，**无**重试：管道不可用时帧静默丢弃，并递增 `logDropCount` 计数（Requirement 18.5，通过 `vhd-bridge-state` IPC 暴露）。权威 schema：design §"Log_Frame / VHDRustDeskBridgeLogV1"；接受准则：Requirements §18。

### 7.1 JSON schema

```json
{
  "protocol":      "VHDRustDeskBridgeLogV1",
  "secretVersion": 1,
  "level":         "warn",
  "target":        "rustdesk::server::connection",
  "message":       "controlled login from 192.0.2.1, password ok",
  "timestampMs":   1730000000500,
  "mac":           "<Base64(HMAC-SHA256)>"
}
```

| 字段 | 类型 | 约束 |
| --- | --- | --- |
| `protocol` | string | **必须**严格等于 `"VHDRustDeskBridgeLogV1"`。 |
| `secretVersion` | u32 | 十进制。 |
| `level` | enum string | `"error"` / `"warn"` / `"info"` / `"debug"` / `"trace"` 之一。 |
| `target` | string | `log` crate 的 target（例如 `"rustdesk::server::connection"`）。自由格式 ASCII / UTF-8。 |
| `message` | string | UTF-8 文本，**生产端已脱敏**：密码呈现为 `"***"`，控制者显示名与 `hwid` 被剥离或哈希（Requirement 18.7，design Property 12）。长度 ≤ 4 KiB。 |
| `timestampMs` | u64 | log 事件发生时刻的 Unix 毫秒。 |
| `mac` | string | §7.2 定义的 32 字节 HMAC-SHA256 摘要的标准字母表 base64。 |

整个帧和其它所有帧一样受 `MAX_FRAME_BYTES = 64 KiB` 限制（§2.3）。生产端如有必要会先截断 `message` —— 截断**先于** `mac` 计算。

### 7.2 HMAC 输入

```
"VHDRustDeskBridgeLogV1\n" || secretVersion || "\n" || level || "\n" || target || "\n" ||
sha256Hex(message) || "\n" || timestampMs
```

HMAC 输入里 `message` 取 `sha256Hex` —— 明文留在 JSON 载荷里因为 `VHDMount` 是存储系统，需要可读文本。HMAC 输入里放哈希意味着未来审计回放无需再次处理消息字节即可校验 MAC。

`secretVersion` 与 `timestampMs` 为十进制 ASCII（无前导零、无 `+`）。仅 LF 分隔符。

### 7.3 无响应

`VHDMount` **必须不**对 Log 帧发送任何响应。管道方向保持空闲，留给下一个请求帧（通常是下一个 Log 帧，或一个 Report / PeerApproval）。如管道写返回 `BrokenPipe` 或 `ConnectionReset`，客户端按标准传输错误路径处理（design §"BridgeWorker 状态机"）—— 这与是否有 ack 无关。

丢帧语义，依 Requirement 18.5 / 18.10 概述：

- 在管道不可用期间（`Bridge_State ∈ {Disabled, Initializing, Denied, Failed}` 或写错误），Log 帧从有界 mpsc 队列中静默丢弃。
- 每次丢帧递增通过 `vhd-bridge-state` IPC 键暴露的 `logDropCount`。
- 被丢的事件**必须不**写入本地文件、stderr、syslog、或 Windows Event Log。

### 7.4 完整示例

> 密钥：`RustDeskClientSharedSecret = "REDACTED"`（占位，依 Requirement 16.5）。

输入：

- `secretVersion = 1`
- `level = "warn"`
- `target = "rustdesk::server::connection"`
- `message = "controlled login from 192.0.2.1, password ok"`（已脱敏）
- `sha256Hex(message) = c0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7`
- `timestampMs = 1730000000500`

HMAC 输入（裸字节；`\n` 显示为 `\n`；无尾随换行）：

```
VHDRustDeskBridgeLogV1\n1\nwarn\nrustdesk::server::connection\nc0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7\n1730000000500
```

Python 字面量：

```python
b"VHDRustDeskBridgeLogV1\n1\nwarn\nrustdesk::server::connection\nc0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7\n1730000000500"
```

线上 JSON 载荷（下面显示的 `mac` 是占位）：

```json
{
  "protocol":      "VHDRustDeskBridgeLogV1",
  "secretVersion": 1,
  "level":         "warn",
  "target":        "rustdesk::server::connection",
  "message":       "controlled login from 192.0.2.1, password ok",
  "timestampMs":   1730000000500,
  "mac":           "<Base64(HMAC-SHA256(RustDeskClientSharedSecret, <input above>))>"
}
```

---

## 8. Peer Approval Frame — `VHDRustDeskBridgePeerApprovalV1`

`RustDesk_Controlled` 在 `validate_password` 成功之后、`try_start_cm(.., authorized=true)` 之前发送（Requirement 19.2）。它向 `VHDMount` 询问由 `controllerId` 标识的控制者**当下**是否被允许控制本机。权威 schema：design §"Peer_Approval_Request / VHDRustDeskBridgePeerApprovalV1"；接受准则：Requirements §19。

### 8.1 JSON schema

```json
{
  "protocol":             "VHDRustDeskBridgePeerApprovalV1",
  "secretVersion":        1,
  "controlledMachineId":  "MACHINE-DEADBEEF",
  "controllerId":         "987654321",
  "controllerName":       "admin@ops",
  "controllerPlatform":   "Windows",
  "controllerHwid":       "aabbccddeeff00112233445566778899",
  "peerSocketAddr":       "192.0.2.1:51820",
  "connectionType":       "controlled",
  "requestNonce":         "0123456789abcdef0123456789abcdef",
  "timestampMs":          1730000001000,
  "mac":                  "<Base64(HMAC-SHA256)>"
}
```

| 字段 | 类型 | 约束 |
| --- | --- | --- |
| `protocol` | string | **必须**严格等于 `"VHDRustDeskBridgePeerApprovalV1"`。 |
| `secretVersion` | u32 | 十进制。 |
| `controlledMachineId` | string | 本机的 `machineId`。 |
| `controllerId` | string | 来自 `LoginRequest.my_id`。 |
| `controllerName` | string | 来自 `LoginRequest.my_name`。**线上明文**，HMAC 输入中哈希（Requirement 19.4）。 |
| `controllerPlatform` | string | 来自 `LoginRequest.my_platform`（如 `"Windows"` / `"Linux"` / `"Mac"` / `"Android"`）。 |
| `controllerHwid` | string | 来自 `LoginRequest.hwid`；**可**为空串。**线上明文**，HMAC 输入中哈希。 |
| `peerSocketAddr` | string | 对端 socket 地址，按 Rust `SocketAddr::to_string()` 显示 —— 即 IPv4 形如 `IP:port`（例如 `"192.0.2.1:51820"`），IPv6 形如 `[IP]:port`（例如 `"[2001:db8::1]:51820"`）。 |
| `connectionType` | enum string | `"controlled"` / `"view-only"` / `"file-transfer"` / `"port-forward"` / `"terminal"` 之一。 |

| `requestNonce` | string | 小写十六进制，正好 32 字符（16 字节随机）。**必须**在单次连接会话内唯一。 |
| `timestampMs` | u64 | 帧构造时刻的 Unix 毫秒。 |
| `mac` | string | §8.2 定义的 32 字节 HMAC-SHA256 摘要的标准字母表 base64。 |

### 8.2 HMAC 输入

```
"VHDRustDeskBridgePeerApprovalV1\n" || secretVersion || "\n" ||
controlledMachineId || "\n" || controllerId || "\n" ||
sha256Hex(controllerName) || "\n" || controllerPlatform || "\n" ||
sha256Hex(controllerHwid) || "\n" || peerSocketAddr || "\n" ||
connectionType || "\n" || requestNonce || "\n" || timestampMs
```

关键：

- `controllerName` 与 `controllerHwid` **明文出现在 JSON 载荷**（让 `VHDMount` 能在审计日志渲染它们），但 HMAC 输入中归约为 `sha256Hex(...)`。
- 当 `controllerHwid` 为空时，`sha256Hex("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`。
- 其它字段在 HMAC 输入中按原样出现。`secretVersion` 与 `timestampMs` 为十进制 ASCII（无前导零、无 `+`）。仅 LF 分隔符。

`RustDesk_Controlled` 一侧，`controllerName` 与 `controllerHwid` **必须不**出现在任何本地日志行；只允许 `controllerId[..3]` + `***` 的形式（Requirement 19.9）。

### 8.3 `Peer_Approval_Response`（VHDMount → Controlled）

```json
{ "result": "approved", "ttlMs": 60000 }
```

```json
{ "result": "rejected" }
{ "result": "rejected", "reason": "<short string>" }
```

语义：

| `result` | `ttlMs` | RustDesk_Controlled 行为 |
| --- | --- | --- |
| `"approved"` | `> 0` | 继续 `try_start_cm(.., authorized=true)`；以给定 TTL 把 `(controllerId, peerSocketAddr) → Approved` 缓存进内存 `ApprovalCache` |
| `"approved"` | `0` 或缺省 | 仅本次放行，**不**入缓存（Requirement 19.7） |
| `"rejected"`（任何 `reason`） | n/a | 返回与 `LOGIN_MSG_PASSWORD_WRONG` 等价的错误，关闭连接，**不**触发 `Maintenance_Overlay`（Requirement 19.6）；具体的 `reason` **必须不**透露给控制者 |

JSON 既不匹配两种形态、或者未在 `Bridge_Config.request_timeout_ms` 内到达的 `Peer_Approval_Response` 视为「桥不可用」，回退至 §19.8 的「密码正确 = 允许」路径。关键的是：此回退**不**改变 `Bridge_State`（保持原值 —— 可能仍是 `Authorized`）—— 只是单次请求的判定降级。

### 8.4 完整示例

> 密钥：`RustDeskClientSharedSecret = "<32 random bytes>"`（REDACTED，依 Requirement 16.5）。

输入：

- `secretVersion = 1`
- `controlledMachineId = "MACHINE-DEADBEEF"`
- `controllerId = "987654321"`
- `controllerName = "admin@ops"`（线上明文）
- `sha256Hex("admin@ops") = bb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85`
- `controllerPlatform = "Windows"`
- `controllerHwid = "aabbccddeeff00112233445566778899"`（线上明文）
- `sha256Hex("aabbccddeeff00112233445566778899") = a820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560`
- `peerSocketAddr = "192.0.2.1:51820"`
- `connectionType = "controlled"`
- `requestNonce = "0123456789abcdef0123456789abcdef"`
- `timestampMs = 1730000001000`

HMAC 输入（裸字节；`\n` 显示为 `\n`；无尾随换行）：

```
VHDRustDeskBridgePeerApprovalV1\n1\nMACHINE-DEADBEEF\n987654321\nbb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85\nWindows\na820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560\n192.0.2.1:51820\ncontrolled\n0123456789abcdef0123456789abcdef\n1730000001000
```

Python 字面量：

```python
b"VHDRustDeskBridgePeerApprovalV1\n1\nMACHINE-DEADBEEF\n987654321\nbb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85\nWindows\na820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560\n192.0.2.1:51820\ncontrolled\n0123456789abcdef0123456789abcdef\n1730000001000"
```

线上 JSON 载荷（下面显示的 `mac` 是占位）：

```json
{
  "protocol":             "VHDRustDeskBridgePeerApprovalV1",
  "secretVersion":        1,
  "controlledMachineId":  "MACHINE-DEADBEEF",
  "controllerId":         "987654321",
  "controllerName":       "admin@ops",
  "controllerPlatform":   "Windows",
  "controllerHwid":       "aabbccddeeff00112233445566778899",
  "peerSocketAddr":       "192.0.2.1:51820",
  "connectionType":       "controlled",
  "requestNonce":         "0123456789abcdef0123456789abcdef",
  "timestampMs":          1730000001000,
  "mac":                  "<Base64(HMAC-SHA256(RustDeskClientSharedSecret, <input above>))>"
}
```

---

## 9. Revocation Frame — `VHDRustDeskBridgeRevocationV1`

服务端推送（`VHDMount → Controlled`）。和 §5–§8 帧不同，Revocation 由 `VHDMount` 在握手成功后**任何时刻**主动发起，迫使被控端从 `Authorized` 退出，即便此刻没有任何客户端请求在路上。接受准则：Requirements §11.7；状态机影响：design §"BridgeWorker 状态机" 错误分类表。

### 9.1 JSON schema

```json
{
  "protocol":      "VHDRustDeskBridgeRevocationV1",
  "secretVersion": 1,
  "reason":        "denied",
  "issuedAt":      1730000005000,
  "mac":           "<Base64(HMAC-SHA256)>"
}
```

| 字段 | 类型 | 约束 |
| --- | --- | --- |
| `protocol` | string | **必须**严格等于 `"VHDRustDeskBridgeRevocationV1"`。 |
| `secretVersion` | u32 | 十进制。**必须**等于本连接已接受的 `secretVersion`。 |
| `reason` | enum string | `"denied"` / `"secret_outdated"` 之一。 |
| `issuedAt` | u64 | `VHDMount` 构造帧时刻的 Unix 毫秒。接收方**可**应用 §10 用于握手的同样 5 分钟有效窗口，防御跨重连重放旧 Revocation。 |
| `mac` | string | §9.2 定义的 32 字节 HMAC-SHA256 摘要的标准字母表 base64。 |

帧使用 §2.3 的标准外壳（4 字节小端长度前缀 + JSON 载荷），共享同样的 `MAX_FRAME_BYTES = 64 KiB` 上限，在已经承载客户端请求响应的同一全双工管道上读取。

### 9.2 HMAC 输入

```
"VHDRustDeskBridgeRevocationV1\n" || secretVersion || "\n" || reason || "\n" || issuedAt
```

`secretVersion` 与 `issuedAt` 为十进制 ASCII（无前导零、无 `+`）。仅 LF 分隔符。无尾随换行。

HMAC 密钥与所有其它帧相同，都是 `RustDeskClientSharedSecret`；Revocation 的 MAC 由 `RustDesk_Controlled` 在应用任何状态转换**之前**校验。`mac` 不合法的 Revocation **必须**忽略，且**应**作为 `level = "warn"` 的 Log 帧记录（受 §7.1 脱敏规则约束）。

### 9.3 客户端无响应

线上**没有**确认。效果是接收方单方面的 `Bridge_State` 转换：

| `reason` | `Bridge_State` 影响 | 恢复 |
| --- | --- | --- |
| `"denied"` | `Authorized → Denied`（或 `Connected → Denied`，或依 Requirement 11.7 从 `Disabled → Denied`）| 标准固定重连间隔后重试握手 |
| `"secret_outdated"` | `→ Failed`（永久） | 需要 `secret_version` 变更或进程重启 |

这与 `HandshakeResponse` / `ReportAck` 携带相同 `reason` 时的目标状态与恢复规则一致。依 Requirement 11.7，`RustDesk_Controlled` 在 `Bridge_State == Disabled` 时**也必须**接受并应用此转换，避免后续重新启用时悄悄回到 `Authorized`。

请求飞行中（例如恰在 `Peer_Approval_Response` 之前）到达的 Revocation **必须**立即生效。任何在转换之后无法被匹配的飞行中请求被丢弃，IPC 会话关闭；下次重连尝试将从头重新握手。

### 9.4 完整示例

> 密钥：`RustDeskClientSharedSecret = "REDACTED"`（占位，依 Requirement 16.5）。

输入：

- `secretVersion = 1`
- `reason = "denied"`
- `issuedAt = 1730000005000`

HMAC 输入（裸字节；`\n` 显示为 `\n`；无尾随换行）：

```
VHDRustDeskBridgeRevocationV1\n1\ndenied\n1730000005000
```

Python 字面量：

```python
b"VHDRustDeskBridgeRevocationV1\n1\ndenied\n1730000005000"
```

线上 JSON 载荷（下面显示的 `mac` 是占位）：

```json
{
  "protocol":      "VHDRustDeskBridgeRevocationV1",
  "secretVersion": 1,
  "reason":        "denied",
  "issuedAt":      1730000005000,
  "mac":           "<Base64(HMAC-SHA256(RustDeskClientSharedSecret, <input above>))>"
}
```

---

## 10. Timing Window & Nonce Anti-Replay · 时间窗与 Nonce 防重放

本节是 §3（HMAC）与 §5–§9（每帧 `nonce` / `timestampMs` / `reportedAt` / `issuedAt`）的接收方补充。接受准则：Requirements §5.3 / §5.4 / §6.3 / §11。

### 10.1 握手有效窗口

`VHDMount` **必须**拒绝 `timestampMs` 不满足下式的任何 `VHDRustDeskBridgeHandshakeV1`：

```
|now - timestampMs| ≤ 300_000   // 5 分钟，毫秒
```

其中 `now` 是 `VHDMount` 在帧到达时的墙钟。窗口外的帧以 `HandshakeResponse { ok: false, reason: "invalid_proof" }`（与 MAC 不匹配同码 —— 线上故意做到无法区分，依 Requirement 5.4 / §11）拒绝。

### 10.2 Nonce 唯一性与重放窗口

| 帧 | `nonce` 字段 | 唯一性范围 | 重放窗口 |
| --- | --- | --- | --- |
| `Handshake` | `nonce` | 同一 `secretVersion` 内唯一 | 5 分钟（与 §10.1 时间窗一致） |
| `Report` | `nonce` | 单次连接会话内唯一（per `(rustDeskId, connection)`） | 会话生命周期 |
| `Peer_Approval_Request` | `requestNonce` | 单次连接会话内唯一 | 会话生命周期 |
| `Log` | （无 nonce） | n/a —— fire-and-forget，由 `MAX_FRAME_BYTES` / `timestampMs` 限定 | n/a |
| `Revocation` | （无 nonce） | n/a —— `issuedAt` **可**与 §9.1 的 5 分钟窗口对照 | 5 分钟 |

握手 nonce 窗口是唯一**必须跨连接存活**的：崩溃后重连**必须不**能在 5 分钟内复用之前见过的 nonce，否则攻击者可从其它进程对 `VHDMount` 重放捕获的 Handshake 帧。Report / PeerApproval 的 nonce 范围限定到一次 `(handshake → connection-close)` 生命周期，因为 HMAC 也覆盖 `secretVersion`，且连接本身已由 Handshake 认证。

### 10.3 时钟漂移容忍

§10.1 的 5 分钟窗口意在吸收同一台 Windows 主机上 `VHDMount` 与 `RustDesk_Controlled` 之间的现实时钟漂移。两端**不**要求 NTP。运维方建议保持两侧时钟相差 1 分钟以内，为握手重试与休眠/恢复后的管道重连留出余量。

### 10.4 推荐的 `VHDMount` 端 nonce 缓存大小

`VHDMount` **应**维护一个内存中的 LRU 缓存 `(secretVersion, nonce) → first_seen_at`，用于拒绝重放的握手。建议：

- 每个 RustDesk_Controlled 实例最坏情况握手速率受重连退避（Requirement 13.2 / 13.3）限制。每客户端持续低于每秒 1 次重连时，约 300 条/客户端可覆盖完整 5 分钟窗口并留有余量。
- 一个 `VHDMount` 实例若服务 `N` 台被控主机（典型部署是 1:1，但运维**可**整合），按相同 5 分钟驱逐规则配置 `300 × N` 条。
- 比 `first_seen_at` 早 5 分钟以上的条目**必须**驱逐（无论 LRU 是否满），保证重放拒绝窗口在时间和大小两个维度都有界。

Report / PeerApproval 的 nonce 唯一性仅对每会话 `HashSet` 检查，**不**进入握手 LRU。

---

## 11. Error Codes & Reasons · 错误码与原因

本节汇总四种请求帧 + Revocation 可产生的全部 `reason` 值、它们在 `RustDesk_Controlled` 上引发的 `Bridge_State` 转换、以及推荐的本地恢复行为。语义**必须**与 design §"BridgeWorker 状态机" 保持一致 —— 若两边漂移，**design 文档为权威**，本表在同一个 PR 中更新（Requirement 16.7）。

### 11.1 完整原因表

| 来源帧 | 字段 | 值 | `Bridge_State` 影响 | 恢复 |
| --- | --- | --- | --- | --- |
| `HandshakeResponse` | `reason` | `deny` | `Initializing → Denied` | 固定重连间隔后重试 |
| `HandshakeResponse` | `reason` | `rate_limited` | `Initializing → Denied` | 固定间隔 + 60s 后重试 |
| `HandshakeResponse` | `reason` | `invalid_proof` | `Initializing → Denied` | 固定间隔后重试（暗示时钟漂移或密钥注入问题） |
| `HandshakeResponse` | `reason` | `secret_outdated` | `Initializing → Failed`（永久） | 需要二进制或 `secret_version` 更新 |
| `ReportAck` | `reason` | `deny` | `→ Denied` | 固定间隔后重试 |
| `ReportAck` | `reason` | `rate_limited` | `→ Denied` | 固定间隔 + 60s 后重试 |
| `ReportAck` | `reason` | `invalid_mac` | `→ Denied` | 固定间隔后重试（暗示注入 / 比特翻转） |
| `ReportAck` | `reason` | `secret_outdated` | `→ Failed`（永久） | 需要二进制或 `secret_version` 更新 |
| `Peer_Approval_Response` | `reason` | （任意） | **不**影响 `Bridge_State` | 仅本次请求拒绝：连接以 `LOGIN_MSG_VHD_APPROVAL_REJECTED` 关闭，**不**触发 `Maintenance_Overlay`（Requirement 19.6） |
| `Revocation` | `reason` | `denied` | `→ Denied`（来自任意状态，包括 `Disabled`） | 固定间隔后重试 |
| `Revocation` | `reason` | `secret_outdated` | `→ Failed`（永久） | 需要二进制或 `secret_version` 更新 |

### 11.2 隐私 / 最小化要求

`VHDMount` **必须不**在任何 `reason` 值中携带超出上述枚举的敏感细节。特别是：TPM 错误码、`VHDSelectServer` HTTP 状态、控制者显示名、机器标识符**必须不**出现在 `reason` 中。逐请求的 `Peer_Approval_Response.reason` 对控制者也不透明 —— `RustDesk_Controlled` 在响应入站登录前丢弃它（Requirement 19.6）。

### 11.3 IPC 映射（`vhd-bridge-state` → `errorCode`）

`RustDesk_Controlled` 通过 `vhd-bridge-state` IPC 键暴露 `(Bridge_State, latest reason)` 到稳定 `errorCode` 字符串的固定映射，让 UI / 安装器 / 监控代码可基于该码 switch 而无需解析自由文本：

- `vhd.bridge.failed.secret_outdated`
- `vhd.bridge.failed.peer_not_vhdmount`
- `vhd.bridge.failed.version_mismatch`
- `vhd.bridge.denied.deny`
- `vhd.bridge.denied.rate_limited`
- `vhd.bridge.denied.invalid_proof`
- `vhd.bridge.denied.invalid_mac`

`vhd.bridge.failed.peer_not_vhdmount` 在 §2.1 的 `GetNamedPipeServerProcessId` 映像路径校验失败时产生。`vhd.bridge.failed.version_mismatch` 保留给未来使用：当某帧承载的 `protocol` 字面量不被本构建理解时（§12.2）。

---

## 12. Compatibility & Versioning · 兼容性与版本管理

本节定义协议的哪些变更是前向兼容的、哪些需要新的 `protocol` 字面量。接受准则：Requirements §14.4 / §14.6 / §16.7。

### 12.1 `secretVersion` 不匹配

两端各自在构建 / 配置期注入 `secretVersion`（§1.2）。值不一致时：

- `VHDMount` 拒绝：`HandshakeResponse { ok: false, reason: "secret_outdated" }`，或 `ReportAck { result: "rejected", reason: "secret_outdated" }`，或推送 `Revocation { reason: "secret_outdated" }`，取决于哪个帧最先暴露不匹配。
- `RustDesk_Controlled` 转入 `Bridge_State == Failed`（永久），通过 IPC 暴露 `vhd.bridge.failed.secret_outdated`（§11.3）。**不**重试。

`secretVersion` **没有**带内协商。运维方**必须**协调 §1.2 描述的轮换流程；恢复需要落后一侧重新构建 / 重配置。

### 12.2 `protocol` 字段不匹配

每帧（Handshake / Report / Log / PeerApproval / Revocation）的 `protocol` 字段是固定字面量 —— `VHDRustDeskBridge<Kind>V1`。接收方看到不识别的字面量时：

- 接收方**必须**拒绝该帧。
- 接收方**应**关闭会话（客户端侧走标准 `Initializing → Initializing` 重连路径；服务端侧立即关闭管道）。
- 接收方**必须不**在协议版本之间自动转换，**必须不**静默降级。

未来的 `…V2` 字面量将是带独立专属章节、独立 HMAC 输入串的独立帧 schema；它**不**搭乘 V1 的 `mac` / `proof` 字段。

### 12.3 前向兼容的可加性 JSON 字段

向既有 V1 帧添加新 JSON 字段是前向兼容的，**当且仅当**：

- 生产端继续发出 V1 接收方已经理解的全部字段，类型与语义不变。
- 如果新字段属于 HMAC 输入定义，生产端**必须**在加入新字段后重算 `mac` / `proof`。在 V1 输入字节上算 HMAC 的旧接收方将看到不匹配并拒绝 —— 此时本来就需要新的 `protocol` 字面量（即这其实是破坏性变更）。
- 如果新字段**不**属于 HMAC 输入定义（仅作为咨询性元数据 —— 既有的 `clientVersion` 是例子），接收方忽略不识别字段，HMAC 仍然有效。

换言之：在不改 HMAC 输入的前提下加咨询性字段是前向兼容的。加参与认证的字段不是。

### 12.4 破坏性变更

以下变更是破坏性的，需要新的 `protocol` 字面量（如 `VHDRustDeskBridgeReportV2`）以及本文档中的对应 schema 章节：

- 重命名字段。
- 删除字段。
- 更改 HMAC 输入顺序、分隔符、或 `sha256Hex(...)` 包裹。
- 更改 HMAC 算法（依 §3，算法在 V1 内**故意不**协商）。
- 更改 `reason` 枚举值的含义，或重新利用既有字段。

引入 `…V2` 帧时，两端**必须**先理解它再让该字面量上线。**没有**单端同时讲 V1 与 V2 同种帧的过渡期。

### 12.5 `secretVersion` 暴露

`secretVersion` 是密钥相关字段中**唯一**允许出现在产物元数据、审计日志、IPC `errorCode` 载荷中的字段（Requirement 14.4 / 14.6）。32 字节密钥本身**永远不**出现在本文档、任何提交文件、任何帧、任何日志行中。

---

## 13. Round-trip Examples (test vectors) · 往返示例（测试向量）

本节复现一个具体的转录，覆盖全部四个请求帧加服务端推送的 Revocation，使用跨帧一致的字段值，让校验方能够把它当作一个连续会话逐步推演。所有 HMAC 输入按占位密钥 `RustDeskClientSharedSecret = "<32 random bytes>"` 计算，依 Requirement 16.5 —— 线上每个 `proof` / `mac` 字段**必须**用真实注入密钥重算。

跨转录共享值：

- `secretVersion = 1`
- `rustDeskId = "123456789"`
- `controlledMachineId = "MACHINE-DEADBEEF"`
- `controllerId = "987654321"`
- 时间戳从 `1730000000000` 起每步 + 500 ms

此处 500 ms 节奏仅作示例 —— 真实 heartbeat 每 30s 一次（§6 / Requirement 6.4），并非由握手完成驱动。

### 13.1 Step 1 —— `Handshake`（Controlled → VHDMount）

线上 JSON 载荷：

```json
{
  "protocol":      "VHDRustDeskBridgeHandshakeV1",
  "secretVersion": 1,
  "nonce":         "4f1c2a8b39d0e7561f8a2b3c4d5e6f70",
  "timestampMs":   1730000000000,
  "clientKind":    "rustdesk",
  "clientVersion": "1.4.6",
  "proof":         "<Base64(HMAC-SHA256(<32 random bytes>, <input below>))>"
}
```

HMAC 输入（Python 字面量）：

```python
b"VHDRustDeskBridgeHandshakeV1\n1\n4f1c2a8b39d0e7561f8a2b3c4d5e6f70\n1730000000000"
```

### 13.2 Step 2 —— `HandshakeResponse`（VHDMount → Controlled）

```json
{ "ok": true }
```

`Bridge_State`：`Initializing → Connected`。

### 13.3 Step 3 —— `Report` startup（Controlled → VHDMount）

JSON 载荷：

```json
{
  "protocol":      "VHDRustDeskBridgeReportV1",
  "secretVersion": 1,
  "rustDeskId":    "123456789",
  "passwordKind":  "temporary",
  "password":      "Hunter2!",
  "reason":        "startup",
  "reportedAt":    1730000000500,
  "nonce":         "9a8b7c6d5e4f30210011223344556677",
  "mac":           "<Base64(HMAC-SHA256(<32 random bytes>, <input below>))>"
}
```

HMAC 输入（`sha256Hex("Hunter2!") = 607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe`）：

```python
b"VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nstartup\n1730000000500\n9a8b7c6d5e4f30210011223344556677"
```

### 13.4 Step 4 —— `ReportAck` accepted（VHDMount → Controlled）

```json
{ "result": "accepted" }
```

`Bridge_State`：`Connected → Authorized`。

### 13.5 Step 5 —— `Report` heartbeat（Controlled → VHDMount）

JSON 载荷：

```json
{
  "protocol":      "VHDRustDeskBridgeReportV1",
  "secretVersion": 1,
  "rustDeskId":    "123456789",
  "passwordKind":  "temporary",
  "password":      "Hunter2!",
  "reason":        "heartbeat",
  "reportedAt":    1730000001000,
  "nonce":         "1122334455667788aabbccddeeff0011",
  "mac":           "<Base64(HMAC-SHA256(<32 random bytes>, <input below>))>"
}
```

HMAC 输入：

```python
b"VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nheartbeat\n1730000001000\n1122334455667788aabbccddeeff0011"
```

### 13.6 Step 6 —— `ReportAck` accepted（VHDMount → Controlled）

```json
{ "result": "accepted" }
```

`Bridge_State` 保持 `Authorized`（后续 accept；仅刷新缓存）。

### 13.7 Step 7 —— `Log`（Controlled → VHDMount，fire-and-forget）

JSON 载荷：

```json
{
  "protocol":      "VHDRustDeskBridgeLogV1",
  "secretVersion": 1,
  "level":         "warn",
  "target":        "rustdesk::server::connection",
  "message":       "controlled login from 192.0.2.1, password ok",
  "timestampMs":   1730000001500,
  "mac":           "<Base64(HMAC-SHA256(<32 random bytes>, <input below>))>"
}
```

HMAC 输入（`sha256Hex(message) = c0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7`）：

```python
b"VHDRustDeskBridgeLogV1\n1\nwarn\nrustdesk::server::connection\nc0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7\n1730000001500"
```

线上无响应（§7.3）。

### 13.8 Step 8 —— `Peer_Approval_Request`（Controlled → VHDMount）

JSON 载荷：

```json
{
  "protocol":             "VHDRustDeskBridgePeerApprovalV1",
  "secretVersion":        1,
  "controlledMachineId":  "MACHINE-DEADBEEF",
  "controllerId":         "987654321",
  "controllerName":       "admin@ops",
  "controllerPlatform":   "Windows",
  "controllerHwid":       "aabbccddeeff00112233445566778899",
  "peerSocketAddr":       "192.0.2.1:51820",
  "connectionType":       "controlled",
  "requestNonce":         "0123456789abcdef0123456789abcdef",
  "timestampMs":          1730000002000,
  "mac":                  "<Base64(HMAC-SHA256(<32 random bytes>, <input below>))>"
}
```

HMAC 输入（`sha256Hex("admin@ops") = bb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85`；`sha256Hex("aabbccddeeff00112233445566778899") = a820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560`）：

```python
b"VHDRustDeskBridgePeerApprovalV1\n1\nMACHINE-DEADBEEF\n987654321\nbb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85\nWindows\na820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560\n192.0.2.1:51820\ncontrolled\n0123456789abcdef0123456789abcdef\n1730000002000"
```

### 13.9 Step 9 —— `Peer_Approval_Response` approved（VHDMount → Controlled）

```json
{ "result": "approved", "ttlMs": 60000 }
```

`RustDesk_Controlled` 继续 `try_start_cm(.., authorized=true)`，并把 `(controllerId, peerSocketAddr) → Approved` 缓存 60s。

### 13.10 Step 10 —— `Revocation` denied（VHDMount → Controlled，服务端推送）

JSON 载荷：

```json
{
  "protocol":      "VHDRustDeskBridgeRevocationV1",
  "secretVersion": 1,
  "reason":        "denied",
  "issuedAt":      1730000002500,
  "mac":           "<Base64(HMAC-SHA256(<32 random bytes>, <input below>))>"
}
```

HMAC 输入：

```python
b"VHDRustDeskBridgeRevocationV1\n1\ndenied\n1730000002500"
```

`Bridge_State`：`Authorized → Denied`。IPC 会话关闭；下次重连尝试从 §13.1 重新握手（用新的 `nonce` 和更新的 `timestampMs`）。

---

## 14. 2FA / Trusted-Devices Disabled · 2FA / 受信设备禁用（`RustDesk_Controlled` 一侧）

本节对 `VHDMount` / `VHDSelectServer` 评审者起信息性作用。实际的代码裁剪由 RustDesk 的 `controlled-only` cargo feature 强制（Requirement 21）；本文档**不**重复 feature flag 矩阵。

### 14.1 被禁用的流程

`RustDesk_Controlled` 构建（启用 `controlled-only` feature 时）禁用 RustDesk 内置的次因素与受信设备流程：

- `LoginRequest.tfa.code` 收到时被忽略；`Connection::require_2fa()` 总返回 `None`。
- 登录响应通道**不应**承载 `LOGIN_MSG_2FA_*` 字面量（来自上游 2FA 路径的 `LOGIN_MSG_OFFLINE`、`REQUIRE_2FA`、`LOGIN_MSG_2FA_WRONG` 等）。
- `Trusted_Devices` 列表硬编码为空：`Config::get_trusted_devices()` 返回 `vec![]`，对应的设置 UI 在编译期被剥除。
- 「邮箱验证」/ 一次性码提示在本构建中是不可达代码路径。

### 14.2 由 `Peer_Approval_Request` 提供身份证明

入站控制者的身份证明专门由 §8 提供 —— 即向 `VHDMount` 的 `Peer_Approval_Request` 往返。RustDesk_Controlled **不**执行以下任意一项：

- TOTP / 一次性码生成或验证
- 受信设备指纹本地持久化
- 邮箱验证往返

`VHDMount` 是「该控制者（`controllerId`，可结合 `peerSocketAddr` / `controllerHwid`）此刻**是否**可控制本机」的唯一权威。桥不可达时，§19.8 「密码正确 = 允许」回退路径生效 —— 那是唯一另一条身份检查路径，**故意**比桥路径弱。

### 14.3 强制参考

- 代码层强制：Requirement 21 描述的 `controlled-only` cargo feature 把 `RustDesk_Controlled` 二进制中相关的 2FA / 受信设备模块完全剥除；缺失是结构性的，不是运行时开关。
- 线层强制：`VHDMount` / `VHDSelectServer` 评审者**可**通过观察断言：从 `RustDesk_Controlled` 构建发出的、出现在线上的任何 `LOGIN_MSG_2FA_*` 字面量都是 bug。
- 审计日志强制：`VHDMount` 审计日志**不应**在从 `RustDesk_Controlled` 转发的 `Log` 帧（§7）中包含任何 `LOGIN_MSG_2FA_*` 字面量；出现一次表示要么是误构建（feature flag 未应用），要么是 RustDesk 端的脱敏管道 bug。

---

## 交叉引用

- RustDesk requirements：`.kiro/specs/vhd-machine-auth-bridge/requirements.md` §5、§6、§8、§11、§16、§17、§18、§19。
- RustDesk design：`.kiro/specs/vhd-machine-auth-bridge/design.md` §"协议帧 schema"、§"BridgeWorker 状态机"。
- 上游服务器：`https://github.com/rustdesk/rustdesk-server`。
- 自定义服务端注入：`src/custom_server.rs`。
