# 机台鉴权链路（Machine Authentication）

本文档描述 VHD Mounter 服务端（VHDSelectServer）与机台客户端（VHDMounter / VHDMounter_Maimoller）之间端到端的鉴权链路，覆盖：

- 鉴权角色与信任根
- 注册证书（registration certificate）的生成与分发
- 机台 RSA 密钥（TPM-protected）的生成与首次注册
- 机台请求签名协议（部署接口）
- 机台日志 WebSocket 三步握手 + 会话密钥派生
- EVHD 密码下发的 RSA 信封
- 管理员侧的审批 / 吊销 / 可信证书管理接口

读者应配合 `AGENTS.md` 与 `documents/admin-guide.md` 一起阅读。

---

## 1. 角色与信任关系

| 角色 | 凭证 / 身份 | 服务端验证手段 |
|------|------------|----------------|
| 管理员（Admin） | 用户名 + 密码 + TOTP | bcrypt 校验 + `express-session` + OTP step-up |
| 机台（Machine） | X.509 注册证书（首次） + 机台 RSA 公钥（之后） | 注册证书指纹白名单 + RSA-SHA256 签名 |
| 部署 / 日志通道 | 机台已注册 RSA + 一次性 bootstrap | 在已注册 RSA 之上叠加 ECDH/HKDF/AES-GCM |

信任关系：

```
管理员
  └─ 在服务端初始化时配置可信注册证书清单 (trustedRegistrationCertificates)
        ↓
注册证书 (.pfx, RSA 3072, 自签 X.509)
  └─ 签名机台首次提交的公钥注册请求
        ↓
机台 RSA 公钥 (TPM 保护，2048 位)
  └─ 签名机台所有后续请求 / 解密 RSA 信封
```

机台一旦完成审批，注册证书的作用就退居"应急换密钥时的再签名"地位，常规请求只用机台自身的 RSA 公私钥。

---

## 2. 注册证书（Registration Certificate）

注册证书是"管理员预先签发并下发给机台的 X.509 自签证书"，机台用它来证明自己是被授权的机台进行第一次公钥注册。

### 2.1 由 `VHDMountAdminTools` 生成

`VHDMountAdminTools/MainWindow.xaml.cs` 中的 `GenerateRegistrationBundle` 离线生成下列文件：

```
<bundleName>.pfx              # RSA 3072 私钥 + 自签证书
<bundleName>.pem              # 仅证书部分（用于服务端可信清单）
<bundleName>.trust.json       # 给服务端导入的元数据
<bundleName>.client-config.ini # 给机台 vhdmonter_config.ini 的片段
```

证书参数：

- 算法：RSA 3072 + SHA-256
- 主题：`CN=<subjectCommonName>`（管理员自定义）
- 扩展：
  - `X509BasicConstraintsExtension(false, false, 0, false)` —— 非 CA
  - `X509KeyUsageExtension(DigitalSignature, false)` —— 仅签名
  - `X509SubjectKeyIdentifierExtension(...)`
- 有效期：`notBefore = UtcNow - 5min`，`notAfter = notBefore + validDays`
- PFX 密码：管理员设定，长度 ≥ 8

`trust.json` 结构（供服务端可信清单导入）：

```json
{
  "name": "<bundleName>",
  "subject": "CN=...",
  "fingerprint256": "<HEX, 大写, 无冒号>",
  "validFrom": "2025-01-01T00:00:00Z",
  "validTo": "2026-01-01T00:00:00Z",
  "certificatePem": "-----BEGIN CERTIFICATE-----\n...",
  "addedAt": "..."
}
```

### 2.2 在服务端注册可信清单

可信注册证书在服务端 **初始化时一次性写入**（`POST /api/init/complete` 中的 `trustedRegistrationCertificates` 字段），随后由这两个接口动态维护：

| 方法 | 路径 | 鉴权 | 说明 |
|------|------|------|------|
| `GET` | `/api/security/trusted-certificates` | requireAuth + OTP | 列出可信注册证书 |
| `POST` | `/api/security/trusted-certificates` | requireAuth + OTP | 新增（body: `certificatePem`, `name?`） |
| `DELETE` | `/api/security/trusted-certificates/:fingerprint` | requireAuth + OTP | 按 SHA-256 指纹删除 |

实现：`VHDSelectServer/securityStore.js`

- 解析 PEM → `crypto.X509Certificate`
- 取规范化指纹：`fingerprint256.replace(/:/g, '').toUpperCase()`
- 持久化到 `server-security.json` 的 `trustedRegistrationCertificates` 数组
- 不存私钥，仅存 `certificatePem` + 元数据

### 2.3 在机台侧加载

机台 `vhdmonter_config.ini`：

```ini
RegistrationCertificatePath=registration.pfx
RegistrationCertificatePassword=<pfx-password>
```

加载位置：`MachineKeyRegistration.LoadRegistrationCertificate`

```csharp
new X509Certificate2(
    resolvedCertificatePath,
    password,
    X509KeyStorageFlags.Exportable | X509KeyStorageFlags.EphemeralKeySet);
```

要求私钥存在，否则注册流程报"注册证书缺少私钥"。

---

## 3. 机台 RSA 密钥（TPM）

每台机台拥有一对 **不可导出的** 2048 位 RSA 密钥，由 Windows TPM 通过 `Microsoft Platform Crypto Provider` 持有。

实现：`VHDManager.EnsureOrCreateTpmRsa(machineId)` (`src/VHDMounter/VHDManager.cs`)

- 密钥名：`VHDMounterKey_{machineId}`
- Provider：`CngProvider.MicrosoftPlatformCryptoProvider`
- 创建参数：
  - `KeyUsage = AllUsages`
  - `ExportPolicy = None`（私钥不可导出，只有 TPM 可签名 / 解密）
  - `Length = 2048`
- 错误处理：仅 `NTE_BAD_KEYSET (0x80090016)` 视为"密钥不存在"并新建；TPM 不可用 / 权限不足等错误必须上抛，避免被掩盖成 NRE

公钥导出格式：`SubjectPublicKeyInfo` (SPKI) → Base64 → PEM `-----BEGIN PUBLIC KEY-----`，由 `VHDManager.ExportPublicKeyPem` 完成，并以 `NormalizePemText`（仅 `Trim()`）规范化。

机台 `keyId` 命名约定：`VHDMounterKey_{machineId}`，与 TPM 密钥同名。

---

## 4. 机台首次注册

### 4.1 接口

```
POST /api/machines/:machineId/keys
```

中间件链：`machineRegistrationLimiter` → `requireInitialized` → `requireDatabase` → `verifySignedRegistrationRequest`

请求体（JSON）：

| 字段 | 类型 | 说明 |
|------|------|------|
| `keyId` | string | 机台 RSA 公钥 ID，约定为 `VHDMounterKey_{machineId}` |
| `keyType` | string | `RSA` |
| `pubkeyPem` | string | 机台 RSA 公钥（SPKI / PEM） |
| `registrationCertificatePem` | string | 注册证书 PEM（仅证书，不含私钥） |
| `signature` | string | Base64 编码的 RSA-SHA256-PKCS1 签名（注册证书私钥签的） |
| `timestamp` | number | Unix 毫秒时间戳 |
| `nonce` | string | 16+ 位十六进制随机串 |

签名输入（**服务端与机台必须按完全相同的规则构造**）：

```
VHDMounterRegistrationV1
<machineId>
<keyId>
<keyType (大写)>
<sha256(pubkeyPem) hex 小写>
<timestamp>
<nonce>
```

各行用 `\n`（LF）连接。规范化：

- `machineId / keyId / nonce`：`Trim()`
- `keyType`：`Trim().ToUpperInvariant()`
- `pubkeyPem`：`NormalizePemText`（仅 `Trim()`，**不要重排换行**）
- `pubkeyPem` 哈希：UTF-8 → SHA-256 → 小写 hex

实现引用：

- 客户端：`VHDManager.BuildRegistrationSigningPayload`（`src/VHDMounter/VHDManager.cs`）
- 服务端：`registrationAuth.buildRegistrationSigningPayload`（`VHDSelectServer/registrationAuth.js`）

### 4.2 服务端校验流程

`registrationAuth.verifySignedRegistrationRequest`：

1. **可信清单非空**：否则返回 503，`服务端尚未配置可信注册证书`
2. **解析证书**：`new crypto.X509Certificate(registrationCertificatePem)`
3. **指纹白名单匹配**：与 `trustedRegistrationCertificates[*].fingerprint256` 严格相等
4. **新鲜度检查**：`assertFreshTimestampAndNonce`
   - `nonce` 长度 ≥ 16
   - `|now - timestamp| ≤ 5 min`
   - `nonce` 不在最近 10 min 的缓存中（缓存键：`runtime.registrationNonceCache`）
5. **证书时效**：当前时间在证书 `validFrom / validTo` 范围内
6. **签名验证**：`crypto.createVerify('RSA-SHA256').verify(certificate.publicKey, signatureBytes)`

校验通过后，服务端写入 `machines` 表：

```sql
key_id                          -- 机台公钥 ID
key_type                        -- 'RSA'
pubkey_pem                      -- 机台公钥 PEM
approved = false                -- 默认未审批
revoked  = false
last_seen                       -- 当前时间
registration_cert_fingerprint   -- 注册证书指纹
registration_cert_subject       -- 注册证书 Subject
```

返回：`HTTP 202 Accepted`，提示"机台公钥已注册，待管理员审批"。

### 4.3 退避与限流

- 服务端：`machineRegistrationLimiter`，10 分钟 / 20 次 / 每 machineId（不基于 IP，避免 NAT 误伤）。超限返回 429 + `retryAfterSeconds`
- 客户端：`MachineKeyRegistration` 解析 429 响应或 `Retry-After` 头部，写入 `_nextRegistrationAttempt`，下次循环阻塞到对应时间点
- 永久错误（证书文件缺失 / 缺私钥 / 配置错）不进入退避，避免循环递增

### 4.4 状态轮询

机台启动后阻塞执行 `EnsureRegisteredAsync`，使用 `/api/machine-log-bootstrap` 作为"廉价探针"：

| 服务端响应 | 机台判定 |
|-----------|----------|
| 200 | 已审批通过，进入正常运行 |
| 400 + `errorCode = MACHINE_NOT_REGISTERED` 或文案命中`未注册公钥` | 未注册，触发 `SubmitRegistrationAsync` |
| 400（其他 / `EVHD` 类） | 已提交但尚未审批，继续轮询 |
| 403（已吊销 / 未审批） | 视为已提交，继续轮询 |
| 429 | 退避等待 `retryAfterSeconds` 后再试 |

轮询间隔 2 s，状态变更通过 `MachineKeyRegistration.CurrentState` 暴露给上层 UI。

---

## 5. 管理员审批与吊销

| 方法 | 路径 | 鉴权 | 行为 |
|------|------|------|------|
| `POST` | `/api/machines/:machineId/approve` | requireAuth + OTP | `approved = body.approved`（默认 `true`），写 `approved_at` |
| `POST` | `/api/machines/:machineId/revoke` | requireAuth + OTP | 重置机台密钥（`revoked = true` 或清空 `pubkey_pem` 视实现） |

只有 `approved = true && revoked = false && pubkey_pem != NULL` 三件齐全的机台，机台侧 / 部署侧 / 日志侧的请求才会被认证通过；缺一个就在对应中间件抛出：

- `404`：机台不存在
- `403`：机台密钥已吊销 / 机台密钥未审批
- `400`：机台未注册公钥

---

## 6. 机台请求签名（部署接口）

注册完成后，机台调用部署接口时使用机台 RSA 私钥（TPM）做请求级签名。

### 6.1 适用接口

机台命中 `requireVerifiedMachineRequest`（`VHDSelectServer/deploymentRoutes.js`）的端点：

| 方法 | 路径 |
|------|------|
| `GET` | `/api/machines/:machineId/deployments/pending` |
| `POST` | `/api/machines/:machineId/deployments/:taskId/status` |
| `POST` | `/api/machines/:machineId/deployments/sync` |

附加约束：

- `User-Agent` 必须以 `VHDMount/` 开头（仅用于下载接口 `downloadPackage / downloadSignature`）
- 每机台 30 次 / 分钟（`deploymentMachineLimiter`）

### 6.2 请求头

| 头名 | 含义 |
|------|------|
| `X-VHDM-KeyId` | 机台公钥 ID（必须等于数据库中已注册的 `key_id`） |
| `X-VHDM-Timestamp` | Unix 毫秒时间戳，与服务器时差 ≤ 5 min |
| `X-VHDM-Nonce` | 16+ 位十六进制随机串，机台维度唯一 |
| `X-VHDM-Signature` | Base64 编码的 RSA-SHA256-PKCS1 签名 |

### 6.3 签名输入

```
VHDMountDeploymentRequestV1
<machineId>
<keyId>
<METHOD 大写>
<request.path>            -- 服务端使用 req.path（不含 query）
<host 不含端口>            -- 客户端用 RequestUri.Host.Split(':')[0]
<timestamp>
<nonce>
<sha256(body) hex 小写>
```

`bodyHash` 取舍（**机端与服务端必须一致**）：

- 服务端：若 `req.rawBody` 存在则用原始字符串，否则若 `req.body` 非空则用 `JSON.stringify(req.body)`，再否则取空串的 SHA-256
- 机端：使用机端构造 / 即将发送的 JSON 字符串（`bodyJson`），GET 请求传空串

实现：

- 机端：`SoftwareDeploy/DeployRequestSigner.cs`
- 服务端：`deploymentRoutes.js` 中 `buildMachineRequestSigningPayload` + `requireVerifiedMachineRequest`

### 6.4 防重放

- `runtime.deploymentRequestNonceCache`：`Map<"${machineId}:${nonce}", ts>`
- 窗口：`MACHINE_REQUEST_SIGNATURE_WINDOW_MS = 5 * 60 * 1000`
- 命中重复 → `409 机台签名 nonce 重复`
- 命中过期 → `401 机台签名已过期`

---

## 7. 机台日志通道（WebSocket）

机台日志走独立 WebSocket：`/ws/machine-log`，与 HTTP 共享端口、通过 `upgrade` 事件分流。

握手三步：bootstrap（HTTP）→ client_hello / server_hello → client_finish / server_finish。

### 7.1 第一步：Bootstrap（HTTP）

```
GET /api/machine-log-bootstrap?machineId=<id>
```

服务端要求该 `machineId` 已注册公钥、已审批、未吊销，否则返回相应 4xx。返回体：

```json
{
  "success": true,
  "machineId": "...",
  "approved": true,
  "revoked": false,
  "keyId": "...",
  "keyType": "RSA",
  "logChannelBootstrapId": "boot_<uuid-no-dash>",
  "logChannelBootstrapCiphertext": "<base64>",
  "logChannelBootstrapExpiresAt": "<ISO8601>"
}
```

`bootstrapCiphertext` 用机台公钥 `RSA-OAEP-SHA1` 加密以下明文：

```json
{
  "bootstrapSecret": "<32 bytes base64>",
  "bootstrapId": "...",
  "expiresAt": "..."
}
```

机台用 TPM 私钥解密得到 `bootstrapSecret`（32 字节随机数），后续会和 ECDH 共享密钥拼接进入 HKDF。

服务端缓存：`runtime.machineLogBootstrapCache`，TTL = 15 分钟（`MACHINE_LOG_BOOTSTRAP_TTL_MS`）。

### 7.2 第二步：client_hello → server_hello

机端发送（明文 JSON）：

```json
{
  "type": "client_hello",
  "protocolVersion": "machine-log-ws-v1",
  "machineId": "...",
  "keyId": "...",
  "sessionId": "...",
  "bootstrapId": "...",
  "timestamp": 1700000000000,
  "nonce": "<32 hex chars>",
  "clientEcdhPublicKey": "<base64 SECG 未压缩点 0x04||X||Y, P-256>",
  "signature": "<base64 RSA-SHA256-PKCS1>"
}
```

签名输入 `VHDMounterMachineLogHelloV1`：

```
VHDMounterMachineLogHelloV1
<protocolVersion>
<machineId>
<keyId>
<sessionId>
<bootstrapId>
<timestamp>
<nonce>
<clientEcdhPublicKey>
```

服务端验证：

1. `protocolVersion === "machine-log-ws-v1"`
2. 机台已注册 / 已审批 / 未吊销
3. `bootstrapId` 命中缓存且 `machineId` 一致且未过期
4. `verifySignedMachineLogHello`：使用机台已注册公钥验证签名 + 时间窗 + nonce 防重（`runtime.machineLogRequestNonceCache`）
5. 服务端生成 `prime256v1` ECDH 密钥对，`computeSecret(clientEcdhPublicKey)`

服务端回 `server_hello`（明文）：

```json
{
  "type": "server_hello",
  "protocolVersion": "machine-log-ws-v1",
  "connectionId": "conn_<uuid-no-dash>",
  "bootstrapId": "...",
  "timestamp": <ms>,
  "nonce": "<32 hex>",
  "serverEcdhPublicKey": "<base64 SECG 未压缩点>",
  "heartbeatSeconds": 15,
  "heartbeatTimeoutSeconds": 45,
  "reconnectBaseMs": 1000,
  "reconnectMaxMs": 30000,
  "resumeWindowSeconds": 300,
  "acknowledgedSeq": <已落库的最大 seq>
}
```

### 7.3 会话密钥派生

```
ikm  = sharedSecret || base64decode(bootstrapSecret)
salt = utf8(clientNonce || serverNonce)

authKey    = HKDF-SHA256(ikm, salt, "machine-log-ws-auth-v1", 32)
sessionKey = HKDF-SHA256(ikm, salt, "machine-log-ws-data-v1", 32)
```

`authKey` 用于握手 MAC，`sessionKey` 用于业务帧 AES-256-GCM。

### 7.4 第三步：client_finish / server_finish

握手 transcript（按行 LF 连接）：

```
VHDMounterMachineLogTranscriptV1
<protocolVersion>
<machineId>
<keyId>
<sessionId>
<bootstrapId>
<timestamp>
<nonce>                                 -- client nonce
<clientEcdhPublicKey>
<connectionId>
<server timestamp>
<nonce>                                 -- server nonce
<serverEcdhPublicKey>
<heartbeatSeconds>
<heartbeatTimeoutSeconds>
<reconnectBaseMs>
<reconnectMaxMs>
<resumeWindowSeconds>
<acknowledgedSeq>
```

`transcriptHash = SHA-256(transcript)`

```
clientFinishMac = base64( HMAC-SHA256(authKey, transcriptHash || "client_finish") )
serverFinishMac = base64( HMAC-SHA256(authKey, transcriptHash || "server_finish") )
```

机端发：

```json
{ "type": "client_finish", "mac": "<base64>" }
```

服务端校验通过后，会主动关闭同 machineId 的旧连接（`1012 superseded`），然后回：

```json
{ "type": "server_finish", "mac": "<base64>" }
```

机端用 `CryptographicOperations.FixedTimeEquals` 校验，避免计时侧信道。

### 7.5 业务帧（握手后）

所有业务帧形如：

```json
{
  "type": "encrypted_frame",
  "seq": 1,
  "ack": 0,
  "iv": "<base64 12 bytes>",
  "ciphertext": "<base64>",
  "tag": "<base64 16 bytes>"
}
```

- AES-256-GCM，`sessionKey` 32 字节，IV 12 字节随机，tag 16 字节
- 明文是 UTF-8 JSON：`log_batch / heartbeat / resume / rekey / close / ack`

服务端限流（按 `MACHINE_LOG_*` 环境变量可调）：

- `MACHINE_LOG_MAX_FRAME_BYTES = 512 KB`
- `MACHINE_LOG_MAX_BATCH_SIZE = 200`
- `MACHINE_LOG_MAX_MACHINE_FRAMES_PER_MINUTE = 120`
- `MACHINE_LOG_MAX_IP_FRAMES_PER_MINUTE = 240`
- `MACHINE_LOG_MAX_BYTES_PER_DAY = 64 MB`

非加密帧（除 `client_hello / client_finish` 外的明文消息）一律 `1008 policy` 关闭。

实现：`VHDSelectServer/machineLogServer.js`、`src/VHDMounter/MachineLogRealtimeChannel.cs`

---

## 8. EVHD 密码下发（RSA 信封）

```
GET /api/evhd-envelope?machineId=<id>
```

要求机台已注册 / 已审批 / 未吊销 / 已写入 `evhd_password`。返回：

```json
{
  "success": true,
  "machineId": "...",
  "approved": true,
  "revoked": false,
  "keyId": "...",
  "keyType": "RSA",
  "ciphertext": "<base64 RSA-OAEP-SHA1(机台公钥, evhdPassword)>"
}
```

机端用 TPM 私钥 `rsa.Decrypt(cipher, RSAEncryptionPadding.OaepSHA1)` 取得明文密码（仅在内存中使用），用于挂载 EVHD。

补充：管理员可通过 `GET /api/evhd-password/plain?machineId=...&reason=...`（requireAuth + OTP step-up）取明文，仅用于人工排障，所有访问写审计日志（`audit type = machine.evhd-password.read`）。

---

## 9. 部署任务的密钥下发（RSA + AES-CTR）

部署任务 ZIP 不在传输中明文。流程：

1. 机台调用 `GET /api/machines/:machineId/deployments/pending`（带 RSA 请求签名）
2. 服务端为每条 task 现场生成两组独立 AES 参数：

   ```
   packageAesKey   (32 random bytes)
   packageIv       = 8 random bytes || 8 zero bytes   -- CTR 起始 counter = 0
   signatureAesKey (32 random bytes)
   signatureIv     = 8 random bytes || 8 zero bytes
   ```

3. 用机台公钥 RSA-OAEP-SHA1 加密 `base64(aesKey)`，得到 `keyCipher` / `signatureKeyCipher`
4. AES key + IV 落表 `deployment_tokens`（migration 005），下载令牌一次性消费
5. 机台收到 `keyCipher / iv / signatureKeyCipher / signatureIv`，TPM 私钥解密得到 AES key
6. 机台 `GET /api/deployments/packages/:id/download?token=...` / `signature?token=...`
   - User-Agent 必须以 `VHDMount/` 开头
   - 服务端按 token 取出对应 AES key/IV，用 `aes-256-ctr` 在文件流上做动态加密
   - 支持 HTTP Range：服务端按 `start` 计算起始 counter（`block_offset = start / 16`，块内偏移 `start % 16` 用空 padding 喂入 cipher）
7. 机端 .NET 自实现 `AesCtrTransform`（无内置 CTR），ECB-加密 counter 后与数据 XOR
8. 完整性最终由 ZIP 包外侧的 RSA 数字签名校验（`DeployVerifier`），AES-CTR 仅承担机密性

ZIP 与签名 **必须用不同的 AES 参数**，避免 keystream 在两个不同明文之间复用。

---

## 10. 管理员鉴权（参考）

机台鉴权之外，管理员侧通过 `requireAuth + requireOtpStepUp` 控制敏感操作。

- 登录：`POST /api/login`，bcrypt 比对密码 → 写 session（`isAuthenticated = true`）
- OTP step-up：`POST /api/otp/verify`，TOTP 校验通过后写 `session.otpVerifiedUntil = now + N min`
- 多 TOTP 密钥：`SecurityStore.totpKeys`（旧版 `totpSecret` 在 `migrateToMultiKey` 时自动迁移到数组）
- 限流：
  - `apiLimiter`：60 s / 240 次（默认）
  - `loginLimiter`：10 min / 20 次
  - `sensitiveLimiter`：10 min / 40 次
  - `machineRegistrationLimiter`：10 min / 20 次 / machineId

涉及机台密钥状态变更的接口（审批 / 吊销 / 可信证书 / 删除机台 / 部署管理 / 日志导出）一律要 `OTP step-up`。

---

## 11. 数据库相关字段（Postgres）

machines 表关键列（来自 `001_initial_schema.sql` + `002_machine_security_columns.sql`）：

| 列 | 说明 |
|----|------|
| `machine_id` | 机台 ID，唯一 |
| `key_id` | 机台公钥 ID（`VHDMounterKey_<machineId>`） |
| `key_type` | `RSA` |
| `pubkey_pem` | 机台公钥 PEM |
| `approved` / `approved_at` | 审批状态 |
| `revoked` / `revoked_at` | 吊销状态 |
| `last_seen` | 任意机台调用都会刷新 |
| `registration_cert_fingerprint` | 注册时使用的注册证书 SHA-256 指纹 |
| `registration_cert_subject` | 注册证书 Subject |
| `evhd_password` | 用于 EVHD 信封的明文密码（仅在受控的 sensitive endpoint 暴露） |

部署 token 表（`004 + 005`）额外字段：

- `aes_key`、`aes_iv`：单次有效，下载完成后 `validateAndConsumeToken` 一次性消费

---

## 12. 故障与排查指引

| 症状 | 可能原因 | 排查路径 |
|------|----------|----------|
| 机台一直停在"等待注册结果回传" | 服务端无可信证书 / 证书指纹未匹配 / `pubkeyPem` 在传输中被改了换行 | 看 `server-audit.log` 中 `machine.registration` 失败原因；确认 `NormalizePemText` 没被覆写 |
| `400 机台未注册公钥` | DB `pubkey_pem` 为空 | 注册请求未通过签名校验，再看上一条 |
| `401 机台签名已过期` | 机台时间不准 / 通信延迟 | NTP 同步；签名窗口 5 min |
| `409 机台签名 nonce 重复` | 机端 nonce 生成器熵不足 / 重发 | `RandomNumberGenerator.GetBytes(16)` 必须每次新生成 |
| 部署下载 403 `User-Agent 校验失败` | UA 没以 `VHDMount/` 开头 | 检查 `HttpClient.DefaultRequestHeaders.UserAgent` |
| WebSocket 1008 `policy` | 协议帧不合法 / 频率限流 | `machineLogServer.js` 抛 `RegistrationAuthError` 都会以 1008 关 |
| WebSocket `clientEcdhPublicKey 无效` | 机端 SECG 未压缩点格式错误 | 长度必须为 65，首字节 `0x04` |
| `bootstrapId 已过期` | 距离 bootstrap > 15 min | 重新请求 `/api/machine-log-bootstrap` |

---

## 13. 关键签名规范汇总（速查）

| 版本字符串 | 用途 | 字段 |
|-----------|------|------|
| `VHDMounterRegistrationV1` | 注册证书签的"机台首次提交公钥" | `machineId / keyId / keyType / sha256(pubkeyPem) / timestamp / nonce` |
| `VHDMountDeploymentRequestV1` | 机台 RSA 签的"部署 / 状态 / sync 请求" | `machineId / keyId / METHOD / path / host / timestamp / nonce / sha256(body)` |
| `VHDMounterMachineLogHelloV1` | 机台 RSA 签的 WS `client_hello` | `protocolVersion / machineId / keyId / sessionId / bootstrapId / timestamp / nonce / clientEcdhPublicKey` |
| `VHDMounterMachineLogTranscriptV1` | WS 握手 transcript（HMAC 输入） | 见 §7.4 |

所有签名：

- 算法：`RSASSA-PKCS1-v1_5 + SHA-256`
- 编码：UTF-8、行分隔符 `\n`、Base64 密文
- 规范化：`Trim()` 字符串字段；`keyType` 上加 `ToUpperInvariant`；`pubkeyPem` 仅 `Trim`，不重排换行

所有 RSA 加密：

- `RSA-OAEP + SHA-1`（保持 .NET / Node 默认互通；如要改 SHA-256 需双端同步升级）

ECDH：`prime256v1 (P-256)`，公钥使用 SECG 未压缩点 `0x04 || X(32) || Y(32)`，Base64 编码。

KDF：`HKDF-SHA256`，Info 串：

- `"machine-log-ws-auth-v1"` → 握手 MAC
- `"machine-log-ws-data-v1"` → 业务帧 AES-256-GCM

业务帧加密：`AES-256-GCM`，IV 12 B 随机，tag 16 B。

---

## 14. 代码索引

机台侧（`src/VHDMounter/`）：

- `MachineKeyRegistration.cs` —— 注册流程、状态机、限流退避
- `VHDManager.cs` —— TPM RSA、注册签名 payload 构造、注册证书加载、`/api/machine-log-bootstrap` 探针消费
- `MachineLogRealtimeChannel.cs` —— WebSocket 三步握手、HKDF、AES-GCM
- `SoftwareDeploy/DeployRequestSigner.cs` —— `X-VHDM-*` 头部签名

服务端（`VHDSelectServer/`）：

- `registrationAuth.js` —— 各类签名 payload + 验签 + nonce 缓存
- `securityStore.js` —— 可信注册证书 / TOTP 多密钥 / 安全配置持久化
- `server.js` —— REST 路由、限流、审计、`encryptWithPublicKeyRSA` 信封
- `machineLogServer.js` —— `/ws/machine-log`、bootstrap 缓存、限流、加密帧编解码
- `deploymentRoutes.js` —— `requireVerifiedMachineRequest`、AES-CTR 动态加密、token 一次性消费
- `migrations/002_machine_security_columns.sql` / `005_deployment_encryption.sql` —— schema

管理工具（`VHDMountAdminTools/`）：

- `MainWindow.xaml.cs::GenerateRegistrationBundle` —— 注册证书 PFX / PEM / trust.json 生成
