//! `vhd_bridge::protocol` — `serde` structs for the four request frames
//! (`Handshake_Frame` / `Report_Frame` / `Log_Frame` /
//! `Peer_Approval_Request`), the server-pushed `Revocation_Frame`, and
//! the three matching response shapes (`HandshakeResponse` /
//! `ReportAck` / `Peer_Approval_Response`).
//!
//! The wire format is byte-level pinned by:
//!   * `docs/vhd-rustdesk-bridge-protocol.md`  §5 / §6 / §7 / §8 / §9
//!   * `.kiro/specs/vhd-machine-auth-bridge/design.md` §"Data Models →
//!     协议帧 schema"
//!
//! Both documents are kept byte-level consistent (Requirement 16.7);
//! this module is the authoritative Rust mirror of those JSON schemas.
//!
//! Layering rules:
//!   * This module defines **shape only**. It never inspects field
//!     contents, never computes HMAC, never validates the
//!     `protocol` discriminator. Those checks live in `super::hmac`
//!     and `super::worker`.
//!   * The `protocol` field stays a free-form `String`; the per-frame
//!     `PROTOCOL_*` constants below are the canonical literals callers
//!     compare against (Requirement 5.1, 6.1, 18.2, 19.3).
//!   * Visibility is `pub(super)` — the bridge surface is private to
//!     `crate::vhd_bridge`; only `mod.rs`'s public API leaks out.
//!
//! Task 5.2 deliverable. Subsequent tasks consume these types:
//!   * 5.3 — round-trip property tests
//!   * 5.4 — `frame::write_frame` callers
//!   * 6.x — HMAC input construction
//!   * 7.x — `BridgeWorker` request/response state machine
//!   * 11.x — `peer_approval::gate` request/response handling

use serde_derive::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Protocol literals
//
// Each frame's `protocol` field MUST be exactly one of these strings.
// `…V1` is the only version this crate accepts; a future migration is
// expressed by introducing `…V2` rather than negotiating in-band
// (protocol doc §3).
// ---------------------------------------------------------------------------

pub(super) const PROTOCOL_HANDSHAKE: &str = "VHDRustDeskBridgeHandshakeV1";
pub(super) const PROTOCOL_REPORT: &str = "VHDRustDeskBridgeReportV1";
pub(super) const PROTOCOL_LOG: &str = "VHDRustDeskBridgeLogV1";
pub(super) const PROTOCOL_PEER_APPROVAL: &str = "VHDRustDeskBridgePeerApprovalV1";
pub(super) const PROTOCOL_REVOCATION: &str = "VHDRustDeskBridgeRevocationV1";

/// Fixed `clientKind` value carried by every `Handshake_Frame`.
/// Required by protocol doc §5.1; non-`"rustdesk"` values are rejected
/// by `VHDMount`.
pub(super) const CLIENT_KIND_RUSTDESK: &str = "rustdesk";

// ---------------------------------------------------------------------------
// Enum-shaped reason / kind fields
//
// Every fixed string set on the wire is modeled as an enum so that
// (a) typos at producer sites turn into compile errors, (b) the
// serde wire form is enforced by `rename_all` rather than by free-form
// strings sprinkled across the worker.
// ---------------------------------------------------------------------------

/// `HandshakeResponse.reason` — the four rejection reasons defined in
/// protocol doc §5.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HandshakeErrorReason {
    Deny,
    RateLimited,
    InvalidProof,
    SecretOutdated,
}

/// `ReportAck.reason` (only set when `result == "rejected"`) — the
/// four rejection reasons defined in protocol doc §6.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReportAckRejectReason {
    Deny,
    RateLimited,
    SecretOutdated,
    InvalidMac,
}

/// `Report_Frame.passwordKind` — protocol doc §6.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PasswordKind {
    Temporary,
    Permanent,
    Preset,
    Absent,
}

/// `Report_Frame.reason` — protocol doc §6.1, the five trigger
/// classifications consumed by `triggers.rs` (task 8.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReportReason {
    Startup,
    IdChange,
    PasswordChange,
    Rotation,
    Heartbeat,
}

/// `Log_Frame.level` — protocol doc §7.1, mirroring the `log` crate's
/// five severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// `Peer_Approval_Request.connectionType` — protocol doc §8.1. The
/// three multi-word variants serialise with `-` separators
/// (`view-only`, `file-transfer`, `port-forward`) so the enum uses
/// `kebab-case`; single-word variants (`controlled`, `terminal`) are
/// unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ConnectionType {
    Controlled,
    ViewOnly,
    FileTransfer,
    PortForward,
    Terminal,
}

/// `Revocation_Frame.reason` — protocol doc §9.1 / §9.3. Only two
/// values are valid: a transient `denied` and the permanent
/// `secret_outdated`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RevocationReason {
    Denied,
    SecretOutdated,
}

// ---------------------------------------------------------------------------
// Request frame structs
//
// Field naming is camelCase on the wire (protocol doc §5.1 / §6.1 /
// §7.1 / §8.1) and snake_case in Rust; serde converts via
// `rename_all = "camelCase"`. The `protocol` field is the literal
// discriminator listed in `PROTOCOL_*` above; producers set it from
// the matching constant and consumers compare it before any other
// validation.
// ---------------------------------------------------------------------------

/// `VHDRustDeskBridgeHandshakeV1` — protocol doc §5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HandshakeFrame {
    /// MUST equal [`PROTOCOL_HANDSHAKE`].
    pub protocol: String,
    pub secret_version: u32,
    /// 32 lowercase hex chars (16 random bytes); doc §5.1 / §10.
    pub nonce: String,
    pub timestamp_ms: u64,
    /// MUST equal [`CLIENT_KIND_RUSTDESK`].
    pub client_kind: String,
    /// RustDesk product version string; advisory only, not part of the
    /// HMAC input (doc §5.2).
    pub client_version: String,
    /// Standard-alphabet base64 of the HMAC-SHA256 digest defined in
    /// doc §5.2.
    pub proof: String,
}

/// `VHDRustDeskBridgeReportV1` — protocol doc §6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReportFrame {
    /// MUST equal [`PROTOCOL_REPORT`].
    pub protocol: String,
    pub secret_version: u32,
    pub rust_desk_id: String,
    pub password_kind: PasswordKind,
    /// UTF-8 plaintext password. Empty string when
    /// `password_kind == PasswordKind::Absent`. Plaintext is required
    /// for `VHDMount` to re-sign and forward; it MUST NOT be logged on
    /// the RustDesk side (Requirement 18.7) and is replaced by
    /// `sha256Hex(...)` in the HMAC input.
    pub password: String,
    pub reason: ReportReason,
    pub reported_at: u64,
    /// 32 lowercase hex chars; unique within a single connected
    /// session (doc §6.1, Requirement 6.3).
    pub nonce: String,
    /// Standard-alphabet base64 of the HMAC-SHA256 digest defined in
    /// doc §6.2.
    pub mac: String,
}

/// `VHDRustDeskBridgeLogV1` — protocol doc §7.
///
/// Sender is responsible for redacting `message` before this struct is
/// constructed (Requirement 18.7, design Property 12); this module
/// does not enforce or inspect the contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LogFrame {
    /// MUST equal [`PROTOCOL_LOG`].
    pub protocol: String,
    pub secret_version: u32,
    pub level: LogLevel,
    pub target: String,
    /// Already-redacted UTF-8 text, ≤ 4 KiB after truncation
    /// (doc §7.1).
    pub message: String,
    pub timestamp_ms: u64,
    /// Standard-alphabet base64 of the HMAC-SHA256 digest defined in
    /// doc §7.2.
    pub mac: String,
}

/// `VHDRustDeskBridgePeerApprovalV1` — protocol doc §8.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PeerApprovalRequest {
    /// MUST equal [`PROTOCOL_PEER_APPROVAL`].
    pub protocol: String,
    pub secret_version: u32,
    pub controlled_machine_id: String,
    pub controller_id: String,
    /// Plaintext on the wire; hashed in the HMAC input (doc §8.2).
    /// MUST NOT appear in any local log line (Requirement 19.9).
    pub controller_name: String,
    pub controller_platform: String,
    /// Plaintext on the wire (may be empty); hashed in the HMAC input
    /// (doc §8.2). MUST NOT appear in any local log line.
    pub controller_hwid: String,
    /// `SocketAddr::to_string()` form: `IP:port` for IPv4,
    /// `[IP]:port` for IPv6 (doc §8.1).
    pub peer_socket_addr: String,
    pub connection_type: ConnectionType,
    /// 32 lowercase hex chars; unique within a single connected
    /// session.
    pub request_nonce: String,
    pub timestamp_ms: u64,
    /// Standard-alphabet base64 of the HMAC-SHA256 digest defined in
    /// doc §8.2.
    pub mac: String,
}

/// `VHDRustDeskBridgeRevocationV1` — protocol doc §9.
///
/// Server-pushed; consumed by the worker as a unilateral
/// `Bridge_State` transition (Requirement 11.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RevocationFrame {
    /// MUST equal [`PROTOCOL_REVOCATION`].
    pub protocol: String,
    pub secret_version: u32,
    pub reason: RevocationReason,
    pub issued_at: u64,
    /// Standard-alphabet base64 of the HMAC-SHA256 digest defined in
    /// doc §9.2.
    pub mac: String,
}

// ---------------------------------------------------------------------------
// Response shapes
//
// `HandshakeResponse` is keyed on a *bool* (`ok`), so serde's internally
// tagged enum (`#[serde(tag = "...")]`) cannot be used. We model it as
// an untagged enum where the more-specific `Err` variant is listed
// first so serde tries it before falling back to `Ok` — i.e. a payload
// containing `reason` parses as `Err`, anything else as `Ok`.
//
// `ReportAck` and `PeerApprovalResponse` are keyed on a string-valued
// `result` field, which is exactly what `#[serde(tag = "result")]` is
// for; `rename_all = "snake_case"` lowercases the variant names to the
// expected wire literals (`accepted` / `rejected` / `approved`).
// ---------------------------------------------------------------------------

/// Server response to [`HandshakeFrame`]. Protocol doc §5.3:
///
/// ```json
/// { "ok": true }
/// { "ok": false, "reason": "deny" | "rate_limited" | "invalid_proof" | "secret_outdated" }
/// ```
///
/// Variant order matters: with `untagged`, serde tries variants in
/// source order, so [`HandshakeResponse::Err`] (which requires a
/// `reason` field) is tried first; payloads without `reason` then fall
/// through to [`HandshakeResponse::Ok`]. A payload like
/// `{ "ok": false }` (no `reason`) is technically a protocol violation
/// — it deserialises as `Ok { ok: false }`, and the consumer should
/// treat the `ok == false` case as such per design §"BridgeWorker
/// 状态机".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum HandshakeResponse {
    Err {
        ok: bool,
        reason: HandshakeErrorReason,
    },
    Ok {
        ok: bool,
    },
}

/// Server response to [`ReportFrame`]. Protocol doc §6.3:
///
/// ```json
/// { "result": "accepted" }
/// { "result": "rejected", "reason": "deny" | "rate_limited" | "secret_outdated" | "invalid_mac" }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub(super) enum ReportAck {
    Accepted,
    Rejected { reason: ReportAckRejectReason },
}

/// Server response to [`PeerApprovalRequest`]. Protocol doc §8.3:
///
/// ```json
/// { "result": "approved", "ttlMs": 60000 }
/// { "result": "approved" }
/// { "result": "rejected" }
/// { "result": "rejected", "reason": "<short string>" }
/// ```
///
/// `ttl_ms` and `reason` are explicitly renamed to camelCase /
/// preserved-lowercase respectively; enum-level `rename_all` only
/// affects variant names, not fields inside the variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub(super) enum PeerApprovalResponse {
    Approved {
        /// Cache TTL hint from `VHDMount`. `None` or `Some(0)` means
        /// "approve once, do not cache" (Requirement 19.7); any
        /// positive value seeds `ApprovalCache` for that many
        /// milliseconds.
        #[serde(default, rename = "ttlMs", skip_serializing_if = "Option::is_none")]
        ttl_ms: Option<u64>,
    },
    Rejected {
        /// Free-form short string for `VHDMount`-side audit logs only;
        /// MUST NOT be exposed to the controller or surfaced in the
        /// RustDesk UI (protocol doc §8.3).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Tests
//
// Task 5.3 (Property 1, schema half): for every frame type defined
// above, the pipeline
//
//     serde_json::to_vec → frame::write_frame → frame::read_frame
//                         → serde_json::from_slice
//
// MUST recover a structurally equal value. The codec half of
// Property 1 (length prefix + 64 KiB cap) lives in
// `frame.rs::tests`; here we cover the JSON shape end-to-end through
// the same codec to exercise the contract together.
//
// Each frame gets its own proptest case so a counter-example shrinks
// to the smallest failing instance per type, matching the "one
// property per `#[test]`" rule from design §"测试约束".
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::frame::{read_frame, write_frame, MAX_FRAME_BYTES};
    use super::*;
    use hbb_common::tokio;
    use proptest::prelude::*;

    // Per-case Tokio runtime — see frame.rs::tests for rationale.
    fn run_blocking<F, T>(fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("vhd_bridge::protocol tests: build current-thread runtime")
            .block_on(fut)
    }

    /// Encode a value as JSON, frame-write it, frame-read it back, and
    /// decode the JSON. Returns the round-tripped value or the first
    /// error encountered. Strings in the schemas can contain any UTF-8,
    /// including embedded `\n`, `"`, control bytes — `serde_json`
    /// handles escaping, so the codec sees only valid JSON.
    fn round_trip<T>(value: &T) -> Result<T, String>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let json = serde_json::to_vec(value).map_err(|e| format!("serialize: {e}"))?;
        // The frame codec caps payloads at MAX_FRAME_BYTES; oversize
        // generators get filtered out before reaching this helper.
        if json.len() > MAX_FRAME_BYTES {
            return Err(format!("payload too large: {}", json.len()));
        }
        let parsed: T = run_blocking(async move {
            let mut buf = Vec::<u8>::new();
            write_frame(&mut buf, &json)
                .await
                .map_err(|e| format!("write_frame: {e}"))?;
            let mut scratch = Vec::new();
            let mut reader: &[u8] = &buf[..];
            let bytes = read_frame(&mut reader, &mut scratch)
                .await
                .map_err(|e| format!("read_frame: {e}"))?;
            serde_json::from_slice(bytes).map_err(|e| format!("deserialize: {e}"))
        })?;
        Ok(parsed)
    }

    // ----- Strategies ------------------------------------------------------
    //
    // Bounded lengths keep generated payloads well under MAX_FRAME_BYTES so
    // the codec layer is exercised without ever tripping its size cap. A
    // single frame's JSON envelope (~200 B) plus a few thousand bytes of
    // fields stays inside the 64 KiB budget with multiple orders of
    // magnitude of headroom.

    /// Arbitrary string up to `max` UTF-8 bytes. Uses proptest's default
    /// Unicode strategy so we exercise multibyte sequences and JSON
    /// escapes (the schema only requires UTF-8).
    fn any_string(max: usize) -> impl Strategy<Value = String> {
        prop::string::string_regex(&format!(".{{0,{max}}}"))
            .expect("vhd_bridge::protocol tests: build string regex")
    }

    fn handshake_error_reason() -> impl Strategy<Value = HandshakeErrorReason> {
        prop_oneof![
            Just(HandshakeErrorReason::Deny),
            Just(HandshakeErrorReason::RateLimited),
            Just(HandshakeErrorReason::InvalidProof),
            Just(HandshakeErrorReason::SecretOutdated),
        ]
    }

    fn report_ack_reject_reason() -> impl Strategy<Value = ReportAckRejectReason> {
        prop_oneof![
            Just(ReportAckRejectReason::Deny),
            Just(ReportAckRejectReason::RateLimited),
            Just(ReportAckRejectReason::SecretOutdated),
            Just(ReportAckRejectReason::InvalidMac),
        ]
    }

    fn password_kind() -> impl Strategy<Value = PasswordKind> {
        prop_oneof![
            Just(PasswordKind::Temporary),
            Just(PasswordKind::Permanent),
            Just(PasswordKind::Preset),
            Just(PasswordKind::Absent),
        ]
    }

    fn report_reason() -> impl Strategy<Value = ReportReason> {
        prop_oneof![
            Just(ReportReason::Startup),
            Just(ReportReason::IdChange),
            Just(ReportReason::PasswordChange),
            Just(ReportReason::Rotation),
            Just(ReportReason::Heartbeat),
        ]
    }

    fn log_level() -> impl Strategy<Value = LogLevel> {
        prop_oneof![
            Just(LogLevel::Error),
            Just(LogLevel::Warn),
            Just(LogLevel::Info),
            Just(LogLevel::Debug),
            Just(LogLevel::Trace),
        ]
    }

    fn connection_type() -> impl Strategy<Value = ConnectionType> {
        prop_oneof![
            Just(ConnectionType::Controlled),
            Just(ConnectionType::ViewOnly),
            Just(ConnectionType::FileTransfer),
            Just(ConnectionType::PortForward),
            Just(ConnectionType::Terminal),
        ]
    }

    fn revocation_reason() -> impl Strategy<Value = RevocationReason> {
        prop_oneof![
            Just(RevocationReason::Denied),
            Just(RevocationReason::SecretOutdated),
        ]
    }

    fn handshake_frame() -> impl Strategy<Value = HandshakeFrame> {
        (
            any::<u32>(),
            any_string(64),
            any::<u64>(),
            any_string(32),
            any_string(128),
        )
            .prop_map(
                |(secret_version, nonce, timestamp_ms, client_version, proof)| HandshakeFrame {
                    protocol: PROTOCOL_HANDSHAKE.to_owned(),
                    secret_version,
                    nonce,
                    timestamp_ms,
                    client_kind: CLIENT_KIND_RUSTDESK.to_owned(),
                    client_version,
                    proof,
                },
            )
    }

    fn report_frame() -> impl Strategy<Value = ReportFrame> {
        (
            any::<u32>(),
            any_string(32),
            password_kind(),
            // 4 KiB upper bound is generous: the protocol caps log
            // messages at 4 KiB but does not impose an explicit cap on
            // password fields beyond the overall MAX_FRAME_BYTES budget.
            any_string(4096),
            report_reason(),
            any::<u64>(),
            any_string(64),
            any_string(128),
        )
            .prop_map(
                |(
                    secret_version,
                    rust_desk_id,
                    password_kind,
                    password,
                    reason,
                    reported_at,
                    nonce,
                    mac,
                )| ReportFrame {
                    protocol: PROTOCOL_REPORT.to_owned(),
                    secret_version,
                    rust_desk_id,
                    password_kind,
                    password,
                    reason,
                    reported_at,
                    nonce,
                    mac,
                },
            )
    }

    fn log_frame() -> impl Strategy<Value = LogFrame> {
        (
            any::<u32>(),
            log_level(),
            any_string(256),
            // Mirror the design's 4 KiB cap on `message` so the
            // generator does not produce values the producer side
            // would otherwise truncate.
            any_string(4096),
            any::<u64>(),
            any_string(128),
        )
            .prop_map(
                |(secret_version, level, target, message, timestamp_ms, mac)| LogFrame {
                    protocol: PROTOCOL_LOG.to_owned(),
                    secret_version,
                    level,
                    target,
                    message,
                    timestamp_ms,
                    mac,
                },
            )
    }

    fn peer_approval_request() -> impl Strategy<Value = PeerApprovalRequest> {
        (
            (any::<u32>(), any_string(64), any_string(64)),
            (any_string(128), any_string(32), any_string(128)),
            (any_string(64), connection_type(), any_string(64)),
            (any::<u64>(), any_string(128)),
        )
            .prop_map(
                |(
                    (secret_version, controlled_machine_id, controller_id),
                    (controller_name, controller_platform, controller_hwid),
                    (peer_socket_addr, connection_type, request_nonce),
                    (timestamp_ms, mac),
                )| PeerApprovalRequest {
                    protocol: PROTOCOL_PEER_APPROVAL.to_owned(),
                    secret_version,
                    controlled_machine_id,
                    controller_id,
                    controller_name,
                    controller_platform,
                    controller_hwid,
                    peer_socket_addr,
                    connection_type,
                    request_nonce,
                    timestamp_ms,
                    mac,
                },
            )
    }

    fn revocation_frame() -> impl Strategy<Value = RevocationFrame> {
        (
            any::<u32>(),
            revocation_reason(),
            any::<u64>(),
            any_string(128),
        )
            .prop_map(|(secret_version, reason, issued_at, mac)| RevocationFrame {
                protocol: PROTOCOL_REVOCATION.to_owned(),
                secret_version,
                reason,
                issued_at,
                mac,
            })
    }

    fn handshake_response() -> impl Strategy<Value = HandshakeResponse> {
        prop_oneof![
            // Variant `Ok`: well-formed payloads always set `ok = true`;
            // see the `ok == false` discussion on `HandshakeResponse`.
            Just(HandshakeResponse::Ok { ok: true }),
            handshake_error_reason()
                .prop_map(|reason| HandshakeResponse::Err { ok: false, reason }),
        ]
    }

    fn report_ack() -> impl Strategy<Value = ReportAck> {
        prop_oneof![
            Just(ReportAck::Accepted),
            report_ack_reject_reason().prop_map(|reason| ReportAck::Rejected { reason }),
        ]
    }

    fn peer_approval_response() -> impl Strategy<Value = PeerApprovalResponse> {
        prop_oneof![
            // `ttlMs` ∈ {None, Some(>0)} per protocol doc §8.3 — `None`
            // and `Some(0)` are both "do not cache". We exercise both.
            prop::option::of(any::<u64>())
                .prop_map(|ttl_ms| PeerApprovalResponse::Approved { ttl_ms }),
            prop::option::of(any_string(128))
                .prop_map(|reason| PeerApprovalResponse::Rejected { reason }),
        ]
    }

    // ----- Round-trip property tests --------------------------------------
    //
    // One #[test] per frame / response type so a shrink reports the
    // smallest failing input for that specific shape.

    proptest! {
        // Feature: vhd-machine-auth-bridge, Property 1 (schema half):
        // For any well-formed value of any of the four protocol frame
        // types or three response shapes, encode-then-decode yields a
        // structurally equal value.
        #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

        #[test]
        fn handshake_frame_round_trip(value in handshake_frame()) {
            let parsed = round_trip(&value).map_err(|e| TestCaseError::fail(e))?;
            prop_assert_eq!(parsed, value);
        }

        #[test]
        fn report_frame_round_trip(value in report_frame()) {
            let parsed = round_trip(&value).map_err(|e| TestCaseError::fail(e))?;
            prop_assert_eq!(parsed, value);
        }

        #[test]
        fn log_frame_round_trip(value in log_frame()) {
            let parsed = round_trip(&value).map_err(|e| TestCaseError::fail(e))?;
            prop_assert_eq!(parsed, value);
        }

        #[test]
        fn peer_approval_request_round_trip(value in peer_approval_request()) {
            let parsed = round_trip(&value).map_err(|e| TestCaseError::fail(e))?;
            prop_assert_eq!(parsed, value);
        }

        #[test]
        fn revocation_frame_round_trip(value in revocation_frame()) {
            let parsed = round_trip(&value).map_err(|e| TestCaseError::fail(e))?;
            prop_assert_eq!(parsed, value);
        }

        #[test]
        fn handshake_response_round_trip(value in handshake_response()) {
            let parsed = round_trip(&value).map_err(|e| TestCaseError::fail(e))?;
            prop_assert_eq!(parsed, value);
        }

        #[test]
        fn report_ack_round_trip(value in report_ack()) {
            let parsed = round_trip(&value).map_err(|e| TestCaseError::fail(e))?;
            prop_assert_eq!(parsed, value);
        }

        #[test]
        fn peer_approval_response_round_trip(value in peer_approval_response()) {
            let parsed = round_trip(&value).map_err(|e| TestCaseError::fail(e))?;
            prop_assert_eq!(parsed, value);
        }
    }

    // ----- Wire-shape spot checks -----------------------------------------
    //
    // These pin the on-the-wire JSON keys / values that the round-trip
    // tests above can't assert (since they only compare Rust values
    // before and after). They guard against silent renames in the
    // serde annotations.

    #[test]
    fn handshake_response_ok_wire_shape() {
        // `{ "ok": true }` MUST parse as the `Ok` variant.
        let parsed: HandshakeResponse = serde_json::from_slice(br#"{"ok":true}"#).unwrap();
        assert!(matches!(parsed, HandshakeResponse::Ok { ok: true }));
    }

    #[test]
    fn handshake_response_err_wire_shape() {
        // `{ "ok": false, "reason": "..." }` MUST parse as the `Err`
        // variant with the matching reason. The four error literals
        // below cover protocol doc §5.3 exhaustively.
        for (raw, expected) in [
            (
                &br#"{"ok":false,"reason":"deny"}"#[..],
                HandshakeErrorReason::Deny,
            ),
            (
                &br#"{"ok":false,"reason":"rate_limited"}"#[..],
                HandshakeErrorReason::RateLimited,
            ),
            (
                &br#"{"ok":false,"reason":"invalid_proof"}"#[..],
                HandshakeErrorReason::InvalidProof,
            ),
            (
                &br#"{"ok":false,"reason":"secret_outdated"}"#[..],
                HandshakeErrorReason::SecretOutdated,
            ),
        ] {
            let parsed: HandshakeResponse = serde_json::from_slice(raw).unwrap();
            match parsed {
                HandshakeResponse::Err { ok: false, reason } => assert_eq!(reason, expected),
                other => panic!("expected Err variant for {raw:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn report_ack_wire_shape() {
        let accepted: ReportAck = serde_json::from_slice(br#"{"result":"accepted"}"#).unwrap();
        assert!(matches!(accepted, ReportAck::Accepted));
        let rejected: ReportAck =
            serde_json::from_slice(br#"{"result":"rejected","reason":"rate_limited"}"#).unwrap();
        assert!(matches!(
            rejected,
            ReportAck::Rejected {
                reason: ReportAckRejectReason::RateLimited,
            }
        ));
    }

    #[test]
    fn peer_approval_response_wire_shape() {
        // ttlMs camelCase rename is the bug-prone spot — assert it
        // explicitly.
        let approved_ttl: PeerApprovalResponse =
            serde_json::from_slice(br#"{"result":"approved","ttlMs":60000}"#).unwrap();
        assert!(matches!(
            approved_ttl,
            PeerApprovalResponse::Approved { ttl_ms: Some(60000) }
        ));
        let approved_no_ttl: PeerApprovalResponse =
            serde_json::from_slice(br#"{"result":"approved"}"#).unwrap();
        assert!(matches!(
            approved_no_ttl,
            PeerApprovalResponse::Approved { ttl_ms: None }
        ));
        let rejected: PeerApprovalResponse =
            serde_json::from_slice(br#"{"result":"rejected"}"#).unwrap();
        assert!(matches!(
            rejected,
            PeerApprovalResponse::Rejected { reason: None }
        ));
        let rejected_with_reason: PeerApprovalResponse =
            serde_json::from_slice(br#"{"result":"rejected","reason":"unknown"}"#).unwrap();
        match rejected_with_reason {
            PeerApprovalResponse::Rejected { reason: Some(s) } => assert_eq!(s, "unknown"),
            other => panic!("expected Rejected with reason, got {other:?}"),
        }
    }

    #[test]
    fn connection_type_kebab_case_wire_shape() {
        // The kebab-case rename for ConnectionType only matters on the
        // wire — the round-trip property tests would not catch a
        // mistaken `snake_case` rename. Pin the expected literals.
        for (variant, expected) in [
            (ConnectionType::Controlled, r#""controlled""#),
            (ConnectionType::ViewOnly, r#""view-only""#),
            (ConnectionType::FileTransfer, r#""file-transfer""#),
            (ConnectionType::PortForward, r#""port-forward""#),
            (ConnectionType::Terminal, r#""terminal""#),
        ] {
            let actual = serde_json::to_string(&variant).unwrap();
            assert_eq!(actual, expected, "ConnectionType::{variant:?} wire form");
        }
    }

    #[test]
    fn handshake_frame_camel_case_wire_shape() {
        // Confirm the four serde_camelCase rename targets that worker
        // code will rely on. We don't compare the whole JSON because
        // serde_json key order is not part of the contract — instead
        // we assert each key substring is present.
        let frame = HandshakeFrame {
            protocol: PROTOCOL_HANDSHAKE.to_owned(),
            secret_version: 7,
            nonce: "abc".to_owned(),
            timestamp_ms: 1,
            client_kind: CLIENT_KIND_RUSTDESK.to_owned(),
            client_version: "1.4.6".to_owned(),
            proof: "p".to_owned(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        for key in [
            r#""protocol":"VHDRustDeskBridgeHandshakeV1""#,
            r#""secretVersion":7"#,
            r#""nonce":"abc""#,
            r#""timestampMs":1"#,
            r#""clientKind":"rustdesk""#,
            r#""clientVersion":"1.4.6""#,
            r#""proof":"p""#,
        ] {
            assert!(
                json.contains(key),
                "expected substring {key:?} in {json}"
            );
        }
    }
}
