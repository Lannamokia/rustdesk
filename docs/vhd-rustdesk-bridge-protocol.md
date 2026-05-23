# VHDMount ⇄ RustDesk_Controlled Bridge Protocol Specification

> Status: **Draft** — produced alongside RustDesk feature `vhd-machine-auth-bridge`.
> Authoritative spec: this document and `.kiro/specs/vhd-machine-auth-bridge/` (requirements / design) MUST stay byte-level consistent. If implementation reveals a mismatch, both sides are updated in the same PR (Requirement 16.7).

This document is the contract between two same-host components running on a Windows machine:

- `RustDesk_Controlled` — a build of RustDesk with the `vhd-bridge` cargo feature enabled, acting as **client** of the named pipe.
- `VHDMount` — a separately-shipped C# program owning TPM access and all communication with `VHDSelectServer`, acting as **server** of the named pipe.

`VHDMount` and `VHDSelectServer` teams MUST be able to implement and review their side without reading RustDesk Rust source.

---

## 1. Overview & Scope

`RustDesk_Controlled` is an **observer**, not a gatekeeper, with respect to TPM-backed machine identity:

- Remote-control accept policy is decided by RustDesk's existing `validate_password` plus a second-factor `Peer_Approval_Request` round-trip; the named pipe being unavailable falls back to "password-correct = allow" (Requirement 8 / 19).
- All TPM operations, all HTTPS calls to `VHDSelectServer`, and all signing happen inside `VHDMount`. RustDesk never sees TPM key material, never speaks HTTPS to the server, and never persists a registration certificate.
- The pipe carries four request frames (Handshake / Report / Log / PeerApproval), three matching response shapes, and one server-initiated Revocation frame (see §5–§9).

Scope of this document:

- Transport, framing, HMAC construction, frame schemas, response shapes, error codes, timing window, anti-replay, version compatibility, round-trip examples, 2FA disablement.

Out of scope:

- TPM provisioning, `VHDSelectServer` HTTP API, `VHDMount` internal storage, registration UX. Those live in the `VHDMount` repo and the `machine-auth.md` spec.

### Upstream RustDesk server (`VHD_Self_Hosted_Server_Repo`)

`RustDesk_Controlled`'s rendezvous (`hbbs`) and relay (`hbbr`) servers come from the upstream repo:

- `https://github.com/rustdesk/rustdesk-server`

This repository (the RustDesk client) does **not** vendor or re-implement that server. Operators self-host hbbs / hbbr from the upstream repo and inject host / key / relay / api at RustDesk client build time using the existing `Custom_Server_Injection` path documented in §1.1 below (Requirement 17.1, 17.2).

The `VHD_Bridge` named-pipe protocol described in this document is **orthogonal** to hbbs/hbbr selection. The bridge does not read `custom-rendezvous-server`, does not read `relay-server` / `api-server`, and does not initiate any network call to hbbs / hbbr (Requirement 17.5).

### 1.1 Custom_Server_Injection — RustDesk's existing server-injection path

RustDesk has a stable, pre-existing way to bake `hbbs` / `hbbr` / `api-server` into a Windows build at packaging time. It lives in `src/custom_server.rs` and exposes:

```rust
pub struct CustomServer {
    pub key:   String, // hbbs public key (base64)
    pub host:  String, // hbbs host[:port[-port2]]
    pub api:   String, // optional api-server URL
    pub relay: String, // hbbr host[:port]
}
```

Two equivalent injection forms are supported (both implemented today, before this feature):

1. **Filename-suffix form** — append a comma-separated key-value list to the binary name before `.exe`:

   ```text
   rustdesk-host=<hbbs-host>,key=<hbbs-pubkey>,relay=<hbbr-host>,api=<api-url>.exe
   ```

   `host=` MUST come first; later fields are optional. Windows duplicate-rename (e.g. `rustdesk (1).exe`) is tolerated by the parser.

2. **Signed base64 license string form** — a base64-encoded JSON `CustomServer` blob signed with the build-time license private key, decoded by `get_license_from_string` and verified against an embedded ed25519 public key.

CI builds for `RustDesk_Controlled` SHALL fill in whichever subset of `host` / `key` / `relay` / `api` the deployment actually uses, and MUST NOT embed any license **private** signing material in the artifact (Requirement 17.7).

The injected values are consumed by RustDesk's existing runtime via `Config::get_rendezvous_servers()`, `Config::get_option("relay-server")`, `Config::get_option("api-server")`, and `src/rendezvous_mediator.rs::get_relay_server`. `VHD_Bridge` does not introduce any new server-selection path.

### 1.2 Shared secret injection (`RustDeskClientSharedSecret`) and version

Both pipe peers authenticate frames with HMAC-SHA256 over a 32-byte secret called `RustDeskClientSharedSecret`. The secret is **compile-time-injected** into the RustDesk binary and provisioned out-of-band into `VHDMount`; it never travels over the wire and never appears in any log.

CI build of RustDesk resolves the secret in this priority order (highest first):

1. `VHD_BRIDGE_SECRET_HEX` env — 64 hex chars, decodes to exactly 32 bytes.
2. `VHD_BRIDGE_SECRET_B64` env — standard base64, decodes to exactly 32 bytes.
3. `vhd_bridge_secret.bin` file at the repo root — exactly 32 raw bytes.
4. `secret.sec` file at the repo root, line `VHDMount Key: <hex>` — 64 hex chars, decodes to exactly 32 bytes.

`VHD_BRIDGE_SECRET_HEX` and `VHD_BRIDGE_SECRET_B64` MUST NOT be set simultaneously; `build.rs` exits non-zero on conflict, missing source, length mismatch, or decode failure (Requirement 3.1–3.6, 14.1–14.5).

The version number `VHD_BRIDGE_SECRET_VERSION` (a `u32`, default `1`) is resolved from env first, then from `secret.sec` line `VHDMount Key Version: <decimal>`. It is embedded in every frame's `secretVersion` field, lets the two peers detect rotation, and is the only field of the secret allowed to appear in artifact metadata (Requirement 14.6, 14.8).

#### Recommended rotation flow

1. Operator generates a new 32-byte secret (e.g. `openssl rand -hex 32`).
2. Operator increments `VHD_BRIDGE_SECRET_VERSION` to `N+1`.
3. CI rebuilds `RustDesk_Controlled` with the new `_HEX` / `_B64` / `.bin` and the new version.
4. `VHDMount` is updated in lock-step (separate ship channel) with the same new secret + version.
5. Peers running the old version on either side will receive `HandshakeResponse { ok: false, reason: "secret_outdated" }` (§5) and enter permanent `Failed` until the binary is updated. There is no in-band rotation negotiation by design — `VHDMount` is the source of truth for "currently accepted versions".

`vhd_bridge_secret.bin` and `secret.sec` are gitignored. Neither file is permitted on CI runners; CI uses env-based injection exclusively.

---

## 2. Transport & Endpoint Definition

### 2.1 Endpoint

- **OS**: Windows.
- **Pipe path** (UTF-16): `\\.\pipe\VHDMount.RustDeskBridge`.
- **Server**: `VHDMount`. It owns the pipe; it creates instances with `CreateNamedPipeW`.
- **Client**: `RustDesk_Controlled`. It connects with `tokio::net::windows::named_pipe::ClientOptions`.
- **Direction**: full-duplex, request/response. The client sends Handshake → Report / Log / PeerApproval; the server sends responses and may push `Revocation` unsolicited (§9).
- **Process identity check**: after `CreateFile` succeeds, the client calls `GetNamedPipeServerProcessId` and resolves the server image path; if the image is not the expected `VHDMount` binary, the client treats it as a permanent error (`peer_not_vhdmount`) and never retries until the process restarts (Requirement 5.6, 11.2).

### 2.2 Byte order & encoding

- Multi-byte integer fields on the wire (frame length prefix) are **little-endian**.
- All JSON payloads are **UTF-8** (no BOM).
- HMAC inputs are **ASCII** text; `\n` (0x0A, LF only — no CR) is the field separator. Decimal integers in HMAC input are written **without leading zeros and without a `+` sign** (e.g. `1730000000000`, never `+1730000000000` or `01730000000000`).

### 2.3 Frame encoding

Every frame on the pipe — request or response — has the same wrapper:

```text
+----------------+---------------------------------------------+
| 4 bytes  (LE)  | N bytes JSON payload                        |
| u32 length=N   | UTF-8, no BOM, no trailing NUL              |
+----------------+---------------------------------------------+
```

- The 4-byte length prefix is the byte length of the JSON payload, encoded little-endian.
- `MAX_FRAME_BYTES = 64 KiB` (65 536). A length prefix exceeding this MUST be rejected: the receiver closes the session as `InvalidData` and (on the client side) returns to `Initializing` with backoff (Requirement 13.4, design §"帧编解码").
- There is no message-level checksum on the wrapper — frame integrity is provided end-to-end by the per-frame HMAC inside the JSON payload (§3).
- There is no framing keepalive at the transport layer. Liveness is observed via the application-level `heartbeat` Report (§6) every 30 s.

### 2.4 Connection lifecycle (informative)

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

Detailed state transitions and error-classification table live in design §"BridgeWorker 状态机" — they are *not* duplicated here to avoid drift.

---

## 3. HMAC-SHA256 Construction Rules

> Filled by **task 20.2** (concrete per-frame inputs) and **task 20.3** (test vectors).

General rules that apply to all four frame kinds:

- **Algorithm**: HMAC-SHA256, fixed. There is no algorithm negotiation. A future migration would be expressed by a new frame `protocol` string (e.g. `…V2`) and is out of scope for this version.
- **Key**: the 32-byte `RustDeskClientSharedSecret` (§1.2). Both peers hold the same secret.
- **Encoding of the input string**: ASCII bytes; field separator is a single `\n` (0x0A).
- **Integers** in HMAC inputs are decimal ASCII, no leading zeros, no `+` sign.
- **`secretVersion`** is included in every HMAC input as the first integer field after the protocol tag, so version-mismatched peers will produce non-matching MACs even when all other fields are identical.
- **`sha256Hex(x)`** in HMAC inputs is the lowercase hex (64 chars) of `SHA-256(x as UTF-8 bytes)`. Used for fields whose plaintext appears in the JSON payload but must not appear in the HMAC string (passwords, controller display names, hwid) — see §6 / §8 for the exact list.
- **MAC encoding on the wire**: the resulting 32-byte digest is base64-encoded (standard alphabet, with `=` padding) into the JSON `mac` / `proof` field.

For quick reference, the per-frame HMAC input strings (LF separators, no trailing newline) are:

| Frame | HMAC input (`\n` = 0x0A) |
| --- | --- |
| `VHDRustDeskBridgeHandshakeV1` | `"VHDRustDeskBridgeHandshakeV1\n" \|\| secretVersion \|\| "\n" \|\| nonce \|\| "\n" \|\| timestampMs` |
| `VHDRustDeskBridgeReportV1` | `"VHDRustDeskBridgeReportV1\n" \|\| secretVersion \|\| "\n" \|\| rustDeskId \|\| "\n" \|\| passwordKind \|\| "\n" \|\| sha256Hex(password) \|\| "\n" \|\| reason \|\| "\n" \|\| reportedAt \|\| "\n" \|\| nonce` |
| `VHDRustDeskBridgeLogV1` | `"VHDRustDeskBridgeLogV1\n" \|\| secretVersion \|\| "\n" \|\| level \|\| "\n" \|\| target \|\| "\n" \|\| sha256Hex(message) \|\| "\n" \|\| timestampMs` |
| `VHDRustDeskBridgePeerApprovalV1` | `"VHDRustDeskBridgePeerApprovalV1\n" \|\| secretVersion \|\| "\n" \|\| controlledMachineId \|\| "\n" \|\| controllerId \|\| "\n" \|\| sha256Hex(controllerName) \|\| "\n" \|\| controllerPlatform \|\| "\n" \|\| sha256Hex(controllerHwid) \|\| "\n" \|\| peerSocketAddr \|\| "\n" \|\| connectionType \|\| "\n" \|\| requestNonce \|\| "\n" \|\| timestampMs` |

The full byte-level construction of each input is reproduced verbatim in §5 / §6 / §7 / §8 alongside the JSON schema for that frame. The Revocation frame (§9) carries its own `mac` field with a different input — see §9 for details.

---

## 4. Frame Catalog

| § | Frame                              | Direction                | Response                     |
| - | ---------------------------------- | ------------------------ | ---------------------------- |
| 5 | `VHDRustDeskBridgeHandshakeV1`     | Controlled → VHDMount    | `HandshakeResponse`          |
| 6 | `VHDRustDeskBridgeReportV1`        | Controlled → VHDMount    | `ReportAck`                  |
| 7 | `VHDRustDeskBridgeLogV1`           | Controlled → VHDMount    | (none — fire-and-forget)     |
| 8 | `VHDRustDeskBridgePeerApprovalV1`  | Controlled → VHDMount    | `Peer_Approval_Response`     |
| 9 | `VHDRustDeskBridgeRevocationV1`    | VHDMount → Controlled    | (none — server-pushed)       |

---

## 5. Handshake Frame — `VHDRustDeskBridgeHandshakeV1`

Sent by `RustDesk_Controlled` as the first frame after `CreateFile` succeeds and the server image has been verified (§2.1, §2.4). Authoritative schema: design §"Handshake_Frame / VHDRustDeskBridgeHandshakeV1"; acceptance criteria: Requirements §5.

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

| Field | Type | Constraint |
| --- | --- | --- |
| `protocol` | string | MUST be exactly the literal `"VHDRustDeskBridgeHandshakeV1"`. Any other value MUST be rejected by `VHDMount`. |
| `secretVersion` | u32 | The currently-injected `VHD_BRIDGE_SECRET_VERSION` (§1.2). Decimal. |
| `nonce` | string | Lowercase hex, exactly 32 characters, encoding 16 cryptographically random bytes. MUST NOT be reused for the same `secretVersion` within any 5-minute window (Requirement 5.3). |
| `timestampMs` | u64 | Unix milliseconds at the moment the frame is built. Validity window on the server side is `|now - timestampMs| ≤ 300000` (Requirement 5.4). |
| `clientKind` | string | MUST be exactly `"rustdesk"`. |
| `clientVersion` | string | RustDesk product version string, e.g. `"1.4.6"`. Free-form ASCII; not part of the HMAC input. |
| `proof` | string | Standard-alphabet base64 (`=`-padded) of the 32-byte HMAC-SHA256 digest defined in §5.2. |

### 5.2 HMAC input

```
"VHDRustDeskBridgeHandshakeV1\n" || secretVersion || "\n" || nonce || "\n" || timestampMs
```

`secretVersion` and `timestampMs` are written as decimal ASCII, no leading zeros, no `+` sign. `\n` is LF (0x0A) only — no CR. There is no trailing newline.

Note that `clientKind`, `clientVersion`, and `proof` itself are **not** part of the HMAC input. `clientKind` / `clientVersion` are advisory metadata for `VHDMount` audit logs; tampering with them does not invalidate the proof, but `VHDMount` is free to reject frames whose `clientKind != "rustdesk"`.

### 5.3 `HandshakeResponse` (VHDMount → Controlled)

```json
{ "ok": true }
```

```json
{ "ok": false, "reason": "deny" }
{ "ok": false, "reason": "rate_limited" }
{ "ok": false, "reason": "invalid_proof" }
{ "ok": false, "reason": "secret_outdated" }
```

`reason` semantics (per design §"BridgeWorker 状态机"):

| `reason` | `Bridge_State` transition | Recovery |
| --- | --- | --- |
| (absent / `ok: true`) | `Initializing → Connected` | proceed to send Report |
| `deny` | `Initializing → Denied` | retry after fixed reconnect interval |
| `rate_limited` | `Initializing → Denied` | retry after fixed interval + 60 s |
| `invalid_proof` | `Initializing → Denied` | retry after fixed interval (likely a clock-skew or secret-injection bug) |
| `secret_outdated` | `Initializing → Failed` (permanent) | requires binary or `secret_version` update; no in-band retry |

A response missing both `ok: true` and a recognised `reason` MUST be treated as a protocol error (`Initializing` with backoff).

### 5.4 Worked example

> The secret used for this example is `RustDeskClientSharedSecret = "<32 random bytes>"` (REDACTED — real values MUST NEVER appear in this document, per Requirement 16.5). All other field values are concrete.

Inputs:

- `secretVersion = 1`
- `nonce = "4f1c2a8b39d0e7561f8a2b3c4d5e6f70"` (16 random bytes, lowercase hex)
- `timestampMs = 1730000000000`

HMAC input (raw bytes, with `\n` shown as `\n`; no trailing newline):

```
VHDRustDeskBridgeHandshakeV1\n1\n4f1c2a8b39d0e7561f8a2b3c4d5e6f70\n1730000000000
```

As a Python literal for verifiers:

```python
b"VHDRustDeskBridgeHandshakeV1\n1\n4f1c2a8b39d0e7561f8a2b3c4d5e6f70\n1730000000000"
```

Resulting on-the-wire JSON payload (the `proof` shown is computed with the redacted/placeholder secret and is not authoritative — it MUST be recomputed from the actual injected `RustDeskClientSharedSecret`):

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

The wire frame is the standard wrapper from §2.3: 4-byte little-endian length prefix of the UTF-8 encoding of the JSON above, followed by that JSON.

---

## 6. Report Frame — `VHDRustDeskBridgeReportV1`

Sent by `RustDesk_Controlled` while `Bridge_State ∈ {Connected, Authorized}` to push the current `(rustDeskId, password)` pair to `VHDMount`. Authoritative schema: design §"Report_Frame / VHDRustDeskBridgeReportV1"; acceptance criteria: Requirements §6.

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

| Field | Type | Constraint |
| --- | --- | --- |
| `protocol` | string | MUST be exactly `"VHDRustDeskBridgeReportV1"`. |
| `secretVersion` | u32 | Decimal. Same value as the most recent successful handshake on this connection. |
| `rustDeskId` | string | RustDesk peer ID as returned by `Config::get_id()` (digits or hostname-style). |
| `passwordKind` | enum string | One of `"temporary"`, `"permanent"`, `"preset"`, `"absent"`. |
| `password` | string | UTF-8 plaintext password. **Empty string** when `passwordKind == "absent"`. The plaintext is delivered to `VHDMount` because `VHDMount` is the entity that re-signs and forwards it to `VHDSelectServer`; the plaintext MUST NOT be logged on the RustDesk side (Requirement 18.7). |
| `reason` | enum string | One of `"startup"`, `"id_change"`, `"password_change"`, `"rotation"`, `"heartbeat"`. |
| `reportedAt` | u64 | Unix milliseconds at the moment the frame is built. |
| `nonce` | string | Lowercase hex, exactly 32 characters (16 random bytes). MUST be unique within a single connected session (Requirement 6.3). Distinct from the handshake `nonce`. |
| `mac` | string | Standard-alphabet base64 of the 32-byte HMAC-SHA256 digest defined in §6.2. |

### 6.2 HMAC input

```
"VHDRustDeskBridgeReportV1\n" || secretVersion || "\n" || rustDeskId || "\n" ||
passwordKind || "\n" || sha256Hex(password) || "\n" || reason || "\n" ||
reportedAt || "\n" || nonce
```

Critical: the **plaintext** `password` only appears in the JSON payload. The HMAC input takes `sha256Hex(password)` — the lowercase 64-char hex of `SHA-256(password as UTF-8 bytes)`. This keeps the password out of the HMAC log even when the HMAC input is reproduced in audit traces. Requirement 6.2 mandates this construction.

`secretVersion` and `reportedAt` are written as decimal ASCII (no leading zeros, no `+`). `\n` is LF only.

For `passwordKind == "absent"`, the password field is the empty string `""` and `sha256Hex("")` is the well-known constant `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

### 6.3 `ReportAck` (VHDMount → Controlled)

```json
{ "result": "accepted" }
```

```json
{ "result": "rejected", "reason": "deny" }
{ "result": "rejected", "reason": "rate_limited" }
{ "result": "rejected", "reason": "secret_outdated" }
{ "result": "rejected", "reason": "invalid_mac" }
```

`reason` semantics:

| `result` / `reason` | `Bridge_State` effect | Recovery |
| --- | --- | --- |
| `"accepted"` (first time) | `Connected → Authorized` | continue heartbeats / event-driven reports |
| `"accepted"` (subsequent) | no change (stays `Authorized`) | refresh `Last_Reported_Snapshot` cache |
| `rejected` / `deny` | `→ Denied` | retry after fixed interval |
| `rejected` / `rate_limited` | `→ Denied` | retry after fixed interval + 60 s |
| `rejected` / `invalid_mac` | `→ Denied` | retry after fixed interval (suggests injection / clock / bit-flip) |
| `rejected` / `secret_outdated` | `→ Failed` (permanent) | requires binary or `secret_version` update |

A `ReportAck` whose JSON does not match either accepted or rejected shape is treated as a protocol error and the session is reset (`→ Initializing` with backoff).

### 6.4 Worked example

> Secret: `RustDeskClientSharedSecret = "<32 random bytes>"` (REDACTED, Requirement 16.5).

Inputs:

- `secretVersion = 1`
- `rustDeskId = "123456789"`
- `passwordKind = "temporary"`
- `password = "Hunter2!"` (plaintext on the wire only)
- `sha256Hex("Hunter2!") = 607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe`
- `reason = "startup"`
- `reportedAt = 1730000000000`
- `nonce = "9a8b7c6d5e4f30210011223344556677"`

HMAC input (raw bytes; `\n` shown as `\n`; no trailing newline):

```
VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nstartup\n1730000000000\n9a8b7c6d5e4f30210011223344556677
```

As a Python literal:

```python
b"VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nstartup\n1730000000000\n9a8b7c6d5e4f30210011223344556677"
```

JSON payload on the wire (the `mac` shown is a placeholder — `VHDMount` MUST recompute it from the actual injected secret):

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

Sent fire-and-forget by `RustDesk_Controlled` to forward already-redacted `log` crate events to `VHDMount` for centralised storage. There is no per-frame response and no retry: when the pipe is unavailable, frames are silently dropped and a `logDropCount` counter is incremented (Requirement 18.5, exposed via `vhd-bridge-state` IPC). Authoritative schema: design §"Log_Frame / VHDRustDeskBridgeLogV1"; acceptance criteria: Requirements §18.

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

| Field | Type | Constraint |
| --- | --- | --- |
| `protocol` | string | MUST be exactly `"VHDRustDeskBridgeLogV1"`. |
| `secretVersion` | u32 | Decimal. |
| `level` | enum string | One of `"error"`, `"warn"`, `"info"`, `"debug"`, `"trace"`. |
| `target` | string | The `log` crate target (e.g. `"rustdesk::server::connection"`). Free-form ASCII / UTF-8. |
| `message` | string | UTF-8 message text, **already redacted** by the producer: passwords appear as `"***"`, controller display names and `hwid` are stripped or hashed (Requirement 18.7, design Property 12). Length ≤ 4 KiB. |
| `timestampMs` | u64 | Unix milliseconds when the log event was emitted. |
| `mac` | string | Standard-alphabet base64 of the 32-byte HMAC-SHA256 digest defined in §7.2. |

The whole frame, like every other frame, is bounded by `MAX_FRAME_BYTES = 64 KiB` (§2.3). Producers truncate `message` to fit if necessary — the truncation is applied **before** `mac` is computed.

### 7.2 HMAC input

```
"VHDRustDeskBridgeLogV1\n" || secretVersion || "\n" || level || "\n" || target || "\n" ||
sha256Hex(message) || "\n" || timestampMs
```

`message` is hashed (`sha256Hex`) in the HMAC input — the plaintext stays in the JSON payload because `VHDMount` is the storage system and needs the human-readable text. The hash inside the HMAC input means a future audit-replay does not have to re-handle the message bytes to verify the MAC.

`secretVersion` and `timestampMs` are decimal ASCII (no leading zeros, no `+`). LF separators only.

### 7.3 No response

`VHDMount` MUST NOT send any response to a Log frame. The pipe direction stays clear for whatever the next request frame is (typically the next Log frame, or a Report / PeerApproval). If the pipe write returns `BrokenPipe` or `ConnectionReset`, the client side handles it via the standard transport-error path (design §"BridgeWorker 状态机") — that is independent of any acknowledgement.

Drop semantics, summarised from Requirement 18.5 / 18.10:

- While the pipe is unavailable (`Bridge_State ∈ {Disabled, Initializing, Denied, Failed}` or write error), Log frames are silently dropped from the bounded mpsc queue.
- Each drop increments `logDropCount` exposed via the `vhd-bridge-state` IPC key.
- The dropped events MUST NOT be written to local files, stderr, syslog, or Windows Event Log.

### 7.4 Worked example

> Secret: `RustDeskClientSharedSecret = "REDACTED"` (placeholder, Requirement 16.5).

Inputs:

- `secretVersion = 1`
- `level = "warn"`
- `target = "rustdesk::server::connection"`
- `message = "controlled login from 192.0.2.1, password ok"` (already redacted)
- `sha256Hex(message) = c0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7`
- `timestampMs = 1730000000500`

HMAC input (raw bytes; `\n` shown as `\n`; no trailing newline):

```
VHDRustDeskBridgeLogV1\n1\nwarn\nrustdesk::server::connection\nc0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7\n1730000000500
```

As a Python literal:

```python
b"VHDRustDeskBridgeLogV1\n1\nwarn\nrustdesk::server::connection\nc0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7\n1730000000500"
```

JSON payload on the wire (the `mac` shown is a placeholder):

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

Sent by `RustDesk_Controlled` after a successful `validate_password` and before `try_start_cm(.., authorized=true)` (Requirement 19.2). It asks `VHDMount` whether the controller identified by `controllerId` is allowed to control this machine right now. Authoritative schema: design §"Peer_Approval_Request / VHDRustDeskBridgePeerApprovalV1"; acceptance criteria: Requirements §19.

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

| Field | Type | Constraint |
| --- | --- | --- |
| `protocol` | string | MUST be exactly `"VHDRustDeskBridgePeerApprovalV1"`. |
| `secretVersion` | u32 | Decimal. |
| `controlledMachineId` | string | The local machine's `machineId`. |
| `controllerId` | string | From `LoginRequest.my_id`. |
| `controllerName` | string | From `LoginRequest.my_name`. **Plaintext on the wire**, hashed in the HMAC input (Requirement 19.4). |
| `controllerPlatform` | string | From `LoginRequest.my_platform` (e.g. `"Windows"`, `"Linux"`, `"Mac"`, `"Android"`). |
| `controllerHwid` | string | From `LoginRequest.hwid`; MAY be empty string. **Plaintext on the wire**, hashed in the HMAC input. |
| `peerSocketAddr` | string | The peer's socket address as displayed by Rust's `SocketAddr::to_string()` — i.e. `IP:port` for IPv4 (e.g. `"192.0.2.1:51820"`) and `[IP]:port` for IPv6 (e.g. `"[2001:db8::1]:51820"`). |
| `connectionType` | enum string | One of `"controlled"`, `"view-only"`, `"file-transfer"`, `"port-forward"`, `"terminal"`. |
| `requestNonce` | string | Lowercase hex, exactly 32 characters (16 random bytes). MUST be unique within a single connected session. |
| `timestampMs` | u64 | Unix milliseconds at the moment the frame is built. |
| `mac` | string | Standard-alphabet base64 of the 32-byte HMAC-SHA256 digest defined in §8.2. |

### 8.2 HMAC input

```
"VHDRustDeskBridgePeerApprovalV1\n" || secretVersion || "\n" ||
controlledMachineId || "\n" || controllerId || "\n" ||
sha256Hex(controllerName) || "\n" || controllerPlatform || "\n" ||
sha256Hex(controllerHwid) || "\n" || peerSocketAddr || "\n" ||
connectionType || "\n" || requestNonce || "\n" || timestampMs
```

Critical:

- `controllerName` and `controllerHwid` appear **plaintext in the JSON payload** (so `VHDMount` can render them in audit logs) but are reduced to `sha256Hex(...)` in the HMAC input.
- For an empty `controllerHwid`, `sha256Hex("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
- All other fields appear verbatim in the HMAC input. `secretVersion` and `timestampMs` are decimal ASCII (no leading zeros, no `+`). LF separators only.

On the RustDesk-Controlled side, `controllerName` and `controllerHwid` MUST NOT appear in any local log line; only `controllerId[..3]` followed by `***` is permitted (Requirement 19.9).

### 8.3 `Peer_Approval_Response` (VHDMount → Controlled)

```json
{ "result": "approved", "ttlMs": 60000 }
```

```json
{ "result": "rejected" }
{ "result": "rejected", "reason": "<short string>" }
```

Semantics:

| `result` | `ttlMs` | RustDesk_Controlled action |
| --- | --- | --- |
| `"approved"` | `> 0` | proceed to `try_start_cm(.., authorized=true)`; cache `(controllerId, peerSocketAddr) → Approved` with the given TTL in the in-memory `ApprovalCache` |
| `"approved"` | `0` or absent | proceed once but do NOT cache (Requirement 19.7) |
| `"rejected"` (any `reason`) | n/a | return `LOGIN_MSG_PASSWORD_WRONG`-equivalent error, close the connection, do NOT trigger `Maintenance_Overlay` (Requirement 19.6); the specific `reason` MUST NOT be exposed to the controller |

A `Peer_Approval_Response` whose JSON does not match either shape, or that does not arrive within `Bridge_Config.request_timeout_ms`, is treated as "bridge unavailable" and falls back to the §19.8 "password-correct = allow" path. Crucially, this fallback does NOT change `Bridge_State` (it stays whatever it was — possibly still `Authorized`) — only the per-request decision is degraded.

### 8.4 Worked example

> Secret: `RustDeskClientSharedSecret = "<32 random bytes>"` (REDACTED, Requirement 16.5).

Inputs:

- `secretVersion = 1`
- `controlledMachineId = "MACHINE-DEADBEEF"`
- `controllerId = "987654321"`
- `controllerName = "admin@ops"` (plaintext on the wire)
- `sha256Hex("admin@ops") = bb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85`
- `controllerPlatform = "Windows"`
- `controllerHwid = "aabbccddeeff00112233445566778899"` (plaintext on the wire)
- `sha256Hex("aabbccddeeff00112233445566778899") = a820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560`
- `peerSocketAddr = "192.0.2.1:51820"`
- `connectionType = "controlled"`
- `requestNonce = "0123456789abcdef0123456789abcdef"`
- `timestampMs = 1730000001000`

HMAC input (raw bytes; `\n` shown as `\n`; no trailing newline):

```
VHDRustDeskBridgePeerApprovalV1\n1\nMACHINE-DEADBEEF\n987654321\nbb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85\nWindows\na820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560\n192.0.2.1:51820\ncontrolled\n0123456789abcdef0123456789abcdef\n1730000001000
```

As a Python literal:

```python
b"VHDRustDeskBridgePeerApprovalV1\n1\nMACHINE-DEADBEEF\n987654321\nbb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85\nWindows\na820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560\n192.0.2.1:51820\ncontrolled\n0123456789abcdef0123456789abcdef\n1730000001000"
```

JSON payload on the wire (the `mac` shown is a placeholder):

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

Server-pushed (`VHDMount → Controlled`). Unlike §5–§8 frames, Revocation is initiated by `VHDMount` at any time after a successful handshake to force the controlled side out of `Authorized` even when no client request is in flight. Acceptance criteria: Requirements §11.7; state-machine effect: design §"BridgeWorker 状态机" error-classification table.

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

| Field | Type | Constraint |
| --- | --- | --- |
| `protocol` | string | MUST be exactly `"VHDRustDeskBridgeRevocationV1"`. |
| `secretVersion` | u32 | Decimal. MUST equal the `secretVersion` accepted on the current connection. |
| `reason` | enum string | One of `"denied"`, `"secret_outdated"`. |
| `issuedAt` | u64 | Unix milliseconds at the moment `VHDMount` built the frame. The receiver MAY apply the same 5-minute validity window used for handshake (§10) to defend against replay of an old Revocation across a reconnect. |
| `mac` | string | Standard-alphabet base64 of the 32-byte HMAC-SHA256 digest defined in §9.2. |

The frame uses the standard wrapper from §2.3 (4-byte LE length prefix + JSON payload), shares the same `MAX_FRAME_BYTES = 64 KiB` ceiling, and is read on the same full-duplex pipe that already carries responses to client requests.

### 9.2 HMAC input

```
"VHDRustDeskBridgeRevocationV1\n" || secretVersion || "\n" || reason || "\n" || issuedAt
```

`secretVersion` and `issuedAt` are decimal ASCII (no leading zeros, no `+`). LF separators only. No trailing newline.

The HMAC key is the same `RustDeskClientSharedSecret` used by every other frame; the Revocation MAC is verified by `RustDesk_Controlled` before any state transition is applied. A Revocation with an invalid `mac` MUST be ignored and SHOULD be logged as a Log frame with `level = "warn"` (subject to the redaction rules of §7.1).

### 9.3 No client response

There is no acknowledgement on the wire. The effect is a unilateral `Bridge_State` transition on the receiver:

| `reason` | `Bridge_State` effect | Recovery |
| --- | --- | --- |
| `"denied"` | `Authorized → Denied` (or `Connected → Denied`, or `Disabled → Denied` per Requirement 11.7) | retry handshake after the standard fixed reconnect interval |
| `"secret_outdated"` | `→ Failed` (permanent) | requires `secret_version` change or process restart |

These are the same destinations and recovery rules as `HandshakeResponse`/`ReportAck` returning the corresponding `reason`. Per Requirement 11.7, `RustDesk_Controlled` MUST also accept and apply the transition when `Bridge_State == Disabled`, so that a later re-enable does not silently bounce back into `Authorized`.

A Revocation arriving while a request is in flight (e.g. just before a `Peer_Approval_Response`) MUST take effect immediately. Any in-flight request whose response cannot be matched after the transition is discarded, and the IPC session is closed; the next reconnect attempt re-handshakes from scratch.

### 9.4 Worked example

> Secret: `RustDeskClientSharedSecret = "REDACTED"` (placeholder, Requirement 16.5).

Inputs:

- `secretVersion = 1`
- `reason = "denied"`
- `issuedAt = 1730000005000`

HMAC input (raw bytes; `\n` shown as `\n`; no trailing newline):

```
VHDRustDeskBridgeRevocationV1\n1\ndenied\n1730000005000
```

As a Python literal:

```python
b"VHDRustDeskBridgeRevocationV1\n1\ndenied\n1730000005000"
```

JSON payload on the wire (the `mac` shown is a placeholder):

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

## 10. Timing Window & Nonce Anti-Replay

This section is the receiver-side complement to §3 (HMAC) and §5–§9 (per-frame `nonce` / `timestampMs` / `reportedAt` / `issuedAt`). Acceptance criteria: Requirements §5.3 / §5.4 / §6.3 / §11.

### 10.1 Handshake validity window

`VHDMount` MUST reject any `VHDRustDeskBridgeHandshakeV1` whose `timestampMs` does not satisfy

```
|now - timestampMs| ≤ 300_000   // 5 minutes, in milliseconds
```

where `now` is `VHDMount`'s wall clock at frame receipt. A frame outside the window is rejected with `HandshakeResponse { ok: false, reason: "invalid_proof" }` (the same code used for MAC-mismatch — the on-the-wire failure is deliberately indistinguishable per Requirement 5.4 and §11).

### 10.2 Nonce uniqueness & replay window

| Frame | `nonce` field | Uniqueness scope | Replay window |
| --- | --- | --- | --- |
| `Handshake` | `nonce` | unique per `secretVersion` | 5 minutes (matches §10.1 timestamp window) |
| `Report` | `nonce` | unique within a single connected session (per `(rustDeskId, connection)`) | session lifetime |
| `Peer_Approval_Request` | `requestNonce` | unique within a single connected session | session lifetime |
| `Log` | (no nonce) | n/a — frames are fire-and-forget and bounded by `MAX_FRAME_BYTES` / `timestampMs` | n/a |
| `Revocation` | (no nonce) | n/a — `issuedAt` MAY be checked against the same 5-minute window per §9.1 | 5 minutes |

The handshake-nonce window is the only one that needs to outlive a single connection: a reconnect after a crash MUST NOT be able to re-use a previously-seen nonce within 5 minutes, otherwise an attacker could replay a captured Handshake frame against `VHDMount` from a different process. Report / PeerApproval nonces are scoped to one `(handshake → connection-close)` lifetime because the HMAC also covers `secretVersion` and the connection itself is authenticated by the Handshake.

### 10.3 Clock-skew tolerance

The 5-minute window in §10.1 is intended to absorb realistic clock skew between `VHDMount` and `RustDesk_Controlled` running on the same Windows host. There is no NTP requirement on either side. Operators are advised to keep both clocks within 1 minute of each other to leave headroom for handshake retries and pipe reconnects after suspend / resume.

### 10.4 Recommended `VHDMount`-side nonce cache sizing

`VHDMount` SHOULD maintain an in-memory LRU cache of `(secretVersion, nonce) → first_seen_at` entries used to reject replayed handshakes. Sizing guideline:

- Per RustDesk_Controlled instance, the worst-case handshake rate is bounded by the reconnect backoff (Requirement 13.2 / 13.3). At a sustained rate of less than 1 reconnect per second per client, ~300 entries per client cover the full 5-minute window with margin.
- For a fleet of `N` controlled hosts pointing at one `VHDMount` instance (the typical deployment is 1:1, but operators MAY consolidate), provision `300 × N` entries with the same 5-minute eviction rule.
- Entries older than 5 minutes (relative to their `first_seen_at`) MUST be evicted whether or not the LRU is full, so that the replay-rejection window is bounded in time as well as in size.

Report / PeerApproval nonce uniqueness is checked against the per-session `HashSet` only and does not contribute to the handshake LRU.

---

## 11. Error Codes & Reasons

This section consolidates every `reason` value the four request frames + Revocation can produce, the resulting `Bridge_State` transition on `RustDesk_Controlled`, and the recommended local recovery action. The semantics here MUST stay aligned with design §"BridgeWorker 状态机" — if the two diverge, the design document is authoritative and this table is updated in the same PR (Requirement 16.7).

### 11.1 Full reason table

| Source frame | Field | Value | `Bridge_State` effect | Recovery |
| --- | --- | --- | --- | --- |
| `HandshakeResponse` | `reason` | `deny` | `Initializing → Denied` | retry after fixed reconnect interval |
| `HandshakeResponse` | `reason` | `rate_limited` | `Initializing → Denied` | retry after fixed interval + 60 s |
| `HandshakeResponse` | `reason` | `invalid_proof` | `Initializing → Denied` | retry after fixed interval (suggests clock-skew or secret-injection issue) |
| `HandshakeResponse` | `reason` | `secret_outdated` | `Initializing → Failed` (permanent) | binary or `secret_version` update required |
| `ReportAck` | `reason` | `deny` | `→ Denied` | retry after fixed interval |
| `ReportAck` | `reason` | `rate_limited` | `→ Denied` | retry after fixed interval + 60 s |
| `ReportAck` | `reason` | `invalid_mac` | `→ Denied` | retry after fixed interval (suggests injection / wire bit-flip) |
| `ReportAck` | `reason` | `secret_outdated` | `→ Failed` (permanent) | binary or `secret_version` update required |
| `Peer_Approval_Response` | `reason` | (any) | not applied to `Bridge_State` | per-request rejection only: connection closed with `LOGIN_MSG_VHD_APPROVAL_REJECTED`, no `Maintenance_Overlay` (Requirement 19.6) |
| `Revocation` | `reason` | `denied` | `→ Denied` (from any state, including `Disabled`) | retry after fixed interval |
| `Revocation` | `reason` | `secret_outdated` | `→ Failed` (permanent) | binary or `secret_version` update required |

### 11.2 Privacy / minimisation requirement

`VHDMount` MUST NOT include sensitive details in any `reason` value beyond the documented enums above. In particular: TPM error codes, `VHDSelectServer` HTTP status, controller display names, and machine identifiers MUST NOT appear in `reason`. Per-request `Peer_Approval_Response.reason` is also opaque to the controller — `RustDesk_Controlled` discards it before responding to the inbound login (Requirement 19.6).

### 11.3 IPC mapping (`vhd-bridge-state` → `errorCode`)

`RustDesk_Controlled` exposes a fixed mapping from `(Bridge_State, latest reason)` to a stable `errorCode` string via the `vhd-bridge-state` IPC key, so that UI / installer / monitoring code can switch on the code without parsing free-form text:

- `vhd.bridge.failed.secret_outdated`
- `vhd.bridge.failed.peer_not_vhdmount`
- `vhd.bridge.failed.version_mismatch`
- `vhd.bridge.denied.deny`
- `vhd.bridge.denied.rate_limited`
- `vhd.bridge.denied.invalid_proof`
- `vhd.bridge.denied.invalid_mac`

`vhd.bridge.failed.peer_not_vhdmount` is produced when the `GetNamedPipeServerProcessId` image-path check from §2.1 fails. `vhd.bridge.failed.version_mismatch` is reserved for future use when the `protocol` literal carried in a frame does not match the version this build understands (§12.2).

---

## 12. Compatibility & Versioning

This section defines what changes to the protocol are forward-compatible and which require a new `protocol` literal. Acceptance criteria: Requirements §14.4 / §14.6 / §16.7.

### 12.1 `secretVersion` mismatch

Both peers carry a `secretVersion` (§1.2) that is independently injected at build / provisioning time. When the values do not match:

- `VHDMount` rejects with `HandshakeResponse { ok: false, reason: "secret_outdated" }`, or `ReportAck { result: "rejected", reason: "secret_outdated" }`, or pushes `Revocation { reason: "secret_outdated" }`, depending on which frame surfaces the mismatch first.
- `RustDesk_Controlled` transitions to `Bridge_State == Failed` (permanent) and surfaces `vhd.bridge.failed.secret_outdated` over IPC (§11.3). It does NOT retry.

There is no in-band negotiation of `secretVersion`. Operators MUST coordinate the rotation flow described in §1.2; recovery requires a new build / re-provisioning on whichever side is behind.

### 12.2 `protocol` field mismatch

The `protocol` field in every frame (Handshake / Report / Log / PeerApproval / Revocation) is a fixed literal — `VHDRustDeskBridge<Kind>V1`. When the receiver sees an unrecognised literal:

- The receiver MUST reject the frame.
- The receiver SHOULD close the session (the standard `Initializing → Initializing` reconnect path on the client side; an immediate pipe close on the server side).
- Receivers MUST NOT auto-translate between protocol versions, and MUST NOT silently downgrade.

A future `…V2` literal would be a separate frame schema with its own dedicated section in this document and a separate HMAC input string; it would not piggy-back on the V1 `mac` / `proof` field.

### 12.3 Forward-compatible additive JSON fields

Adding a new JSON field to an existing V1 frame is forward-compatible IF AND ONLY IF:

- Producers continue to emit all the fields V1 receivers already understand, with the same types and semantics.
- If the new field is part of the HMAC input definition, producers SHALL recompute `mac` / `proof` after adding it. Older receivers that compute the HMAC over the V1 input bytes will then see a mismatch and reject — at which point a new `protocol` literal is required (i.e. it was actually a breaking change).
- If the new field is NOT part of the HMAC input definition (advisory metadata only — `clientVersion` is the existing example), receivers ignore unrecognised fields and the HMAC stays valid.

In other words: adding an advisory field to the JSON without changing the HMAC input is forward-compatible. Adding a field that participates in authentication is not.

### 12.4 Breaking changes

The following changes are breaking and require a new `protocol` literal (e.g. `VHDRustDeskBridgeReportV2`) plus an accompanying schema section in this document:

- Renaming a field.
- Removing a field.
- Changing the HMAC input order, separator, or `sha256Hex(...)` wrapping.
- Changing the HMAC algorithm (per §3, the algorithm is intentionally not negotiable within V1).
- Changing the meaning of a `reason` enum value, or repurposing an existing field.

When a `…V2` frame is introduced, both peers MUST be updated to understand it before the literal is shipped on the wire. There is no transitional period during which a single peer speaks both V1 and V2 of the same frame kind.

### 12.5 `secretVersion` exposure

`secretVersion` is the ONLY field of the secret allowed to appear in artifact metadata, audit logs, or the IPC `errorCode` payload (Requirement 14.4 / 14.6). The 32-byte secret bytes themselves never appear in this document, in any committed file, in any frame, or in any log line.

---

## 13. Round-trip Examples (test vectors)

This section reproduces a single concrete transcript covering all four request frames plus the server-pushed Revocation, using consistent field values across frames so a verifier can step through it as one continuous session. All HMAC inputs are computed against the placeholder secret `RustDeskClientSharedSecret = "<32 random bytes>"` per Requirement 16.5 — every `proof` / `mac` field on the wire MUST be recomputed against the real injected secret.

Shared values across the transcript:

- `secretVersion = 1`
- `rustDeskId = "123456789"`
- `controlledMachineId = "MACHINE-DEADBEEF"`
- `controllerId = "987654321"`
- timestamps step by 500 ms starting at `1730000000000`

The 500 ms cadence here is illustrative — real-world heartbeats fire every 30 s (§6 / Requirement 6.4) and are not driven by handshake completion.

### 13.1 Step 1 — `Handshake` (Controlled → VHDMount)

JSON payload on the wire:

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

HMAC input (Python literal):

```python
b"VHDRustDeskBridgeHandshakeV1\n1\n4f1c2a8b39d0e7561f8a2b3c4d5e6f70\n1730000000000"
```

### 13.2 Step 2 — `HandshakeResponse` (VHDMount → Controlled)

```json
{ "ok": true }
```

`Bridge_State`: `Initializing → Connected`.

### 13.3 Step 3 — `Report` startup (Controlled → VHDMount)

JSON payload:

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

HMAC input (`sha256Hex("Hunter2!") = 607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe`):

```python
b"VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nstartup\n1730000000500\n9a8b7c6d5e4f30210011223344556677"
```

### 13.4 Step 4 — `ReportAck` accepted (VHDMount → Controlled)

```json
{ "result": "accepted" }
```

`Bridge_State`: `Connected → Authorized`.

### 13.5 Step 5 — `Report` heartbeat (Controlled → VHDMount)

JSON payload:

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

HMAC input:

```python
b"VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nheartbeat\n1730000001000\n1122334455667788aabbccddeeff0011"
```

### 13.6 Step 6 — `ReportAck` accepted (VHDMount → Controlled)

```json
{ "result": "accepted" }
```

`Bridge_State` stays `Authorized` (subsequent-accept; cache refresh only).

### 13.7 Step 7 — `Log` (Controlled → VHDMount, fire-and-forget)

JSON payload:

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

HMAC input (`sha256Hex(message) = c0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7`):

```python
b"VHDRustDeskBridgeLogV1\n1\nwarn\nrustdesk::server::connection\nc0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7\n1730000001500"
```

No response on the wire (§7.3).

### 13.8 Step 8 — `Peer_Approval_Request` (Controlled → VHDMount)

JSON payload:

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

HMAC input (`sha256Hex("admin@ops") = bb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85`; `sha256Hex("aabbccddeeff00112233445566778899") = a820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560`):

```python
b"VHDRustDeskBridgePeerApprovalV1\n1\nMACHINE-DEADBEEF\n987654321\nbb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85\nWindows\na820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560\n192.0.2.1:51820\ncontrolled\n0123456789abcdef0123456789abcdef\n1730000002000"
```

### 13.9 Step 9 — `Peer_Approval_Response` approved (VHDMount → Controlled)

```json
{ "result": "approved", "ttlMs": 60000 }
```

`RustDesk_Controlled` proceeds to `try_start_cm(.., authorized=true)` and caches `(controllerId, peerSocketAddr) → Approved` for 60 s.

### 13.10 Step 10 — `Revocation` denied (VHDMount → Controlled, server-pushed)

JSON payload:

```json
{
  "protocol":      "VHDRustDeskBridgeRevocationV1",
  "secretVersion": 1,
  "reason":        "denied",
  "issuedAt":      1730000002500,
  "mac":           "<Base64(HMAC-SHA256(<32 random bytes>, <input below>))>"
}
```

HMAC input:

```python
b"VHDRustDeskBridgeRevocationV1\n1\ndenied\n1730000002500"
```

`Bridge_State`: `Authorized → Denied`. The IPC session is closed; the next reconnect attempt re-handshakes from §13.1 (with a fresh `nonce` and updated `timestampMs`).

---

## 14. 2FA / Trusted-Devices Disabled (`RustDesk_Controlled` side)

This section is informative for `VHDMount` / `VHDSelectServer` reviewers. The actual code-stripping is enforced by RustDesk's `controlled-only` cargo feature flag (Requirement 21); this document does not duplicate the feature-flag matrix.

### 14.1 Disabled flows

`RustDesk_Controlled` builds (with the `controlled-only` feature) disable RustDesk's built-in second-factor and trusted-device flows:

- `LoginRequest.tfa.code` is ignored on receipt; `Connection::require_2fa()` always returns `None`.
- The login response channel SHALL NOT carry the `LOGIN_MSG_2FA_*` literals (`LOGIN_MSG_OFFLINE`, `REQUIRE_2FA`, `LOGIN_MSG_2FA_WRONG`, etc. originating from the upstream 2FA path).
- The `Trusted_Devices` list is hardcoded empty: `Config::get_trusted_devices()` returns `vec![]`, and the corresponding settings UI is removed at compile time.
- The "Verify via email" / one-time-code prompts are unreachable code paths in this build.

### 14.2 Identity is provided by `Peer_Approval_Request` instead

Identity proof of inbound controllers is provided exclusively by §8 — the `Peer_Approval_Request` round-trip to `VHDMount`. RustDesk_Controlled does not perform any of:

- TOTP / one-time-code generation or validation
- Trusted-device fingerprint persistence on disk
- E-mail verification round-trips

`VHDMount` is the single source of truth for "may this controller (`controllerId`, optionally combined with `peerSocketAddr` / `controllerHwid`) control this machine right now". When the bridge is unreachable, the §19.8 "password-correct = allow" fallback applies — that fallback is the only other identity check, and it is intentionally weaker than the bridge path.

### 14.3 Enforcement reference

- Code-level enforcement: the `controlled-only` cargo feature flag described in Requirement 21 strips the relevant 2FA / trusted-devices modules from the `RustDesk_Controlled` binary entirely; the absence is structural, not a runtime toggle.
- Wire-level enforcement: `VHDMount` / `VHDSelectServer` reviewers MAY assert by inspection that any `LOGIN_MSG_2FA_*` literal observed on the wire from a `RustDesk_Controlled` build is a bug.
- Audit-log enforcement: `VHDMount` audit logs SHOULD NOT contain any `LOGIN_MSG_2FA_*` literal in `Log` frames forwarded from `RustDesk_Controlled` (§7); appearance of one indicates either a misbuild (feature flag not applied) or a redaction-pipeline bug on the RustDesk side.

---

## Cross-references

- RustDesk requirements: `.kiro/specs/vhd-machine-auth-bridge/requirements.md` §5, §6, §8, §11, §16, §17, §18, §19.
- RustDesk design: `.kiro/specs/vhd-machine-auth-bridge/design.md` §"协议帧 schema", §"BridgeWorker 状态机".
- Upstream server: `https://github.com/rustdesk/rustdesk-server`.
- Custom server injection: `src/custom_server.rs`.
