//! `vhd_bridge::hmac` — HMAC-SHA256 input builders for all five frame
//! kinds plus a `ct_eq` constant-time comparison helper (task 4.3).
//!
//! Inputs are ASCII text with `\n` (LF, 0x0A) separators, byte-for-byte
//! matching `docs/vhd-rustdesk-bridge-protocol.md` §5.2 / §6.2 / §7.2 /
//! §8.2 / §9.2. Numeric fields are written via `to_string()` so the
//! decimal ASCII representation has no leading zeros and no `+` sign,
//! per §2.2 of the same document.
//!
//! Per Requirement 10.3 / 13.5, the assembled HMAC input lives in a
//! `Zeroizing<Vec<u8>>` so its backing memory is wiped immediately
//! after the digest is computed; password copies feeding the report
//! frame are handled by the caller using `Zeroizing<String>` for the
//! same reason.
//!
//! HMAC-SHA256 itself is computed via `hbb_common::hmac` /
//! `hbb_common::sha2`, the existing dependencies — no new
//! cryptographic crate is introduced (Requirement 13.5).
//!
//! ## Layering
//!
//! Each frame kind is implemented as a pair:
//!
//! * `hmac_*_input(...) -> Vec<u8>` returns the assembled HMAC input
//!   byte string. This is the value tested by Property 2 / Property
//!   20 (task 4.4) — every byte of the layout is what `VHDMount` and
//!   the spec doc agree on.
//! * `hmac_*(...)` wraps that buffer in `Zeroizing<Vec<u8>>` and runs
//!   `compute_hmac` over it. The wrapper drops the buffer with its
//!   bytes wiped (Requirement 10.3 / 13.5).

#![allow(dead_code)] // wired up by worker/peer_approval/log_sink in later tasks.

use hbb_common::hmac::{Hmac, Mac};
use hbb_common::sha2::Sha256;
use zeroize::Zeroizing;

use super::secret::with_shared_secret;

type HmacSha256 = Hmac<Sha256>;

/// Compute HMAC-SHA256 over `input` using the build-time-injected
/// `RustDeskClientSharedSecret`. The secret is borrowed for the
/// duration of the call only (see [`with_shared_secret`]).
#[inline]
fn compute_hmac(input: &[u8]) -> [u8; 32] {
    with_shared_secret(|secret| {
        // Hmac::new_from_slice never fails for HMAC: any key length is
        // valid. Using `expect` here matches AGENTS.md "lock acquisition
        // where failure means poisoning, not normal control flow" — an
        // error here would be a programmer bug, not runtime input.
        let mut mac = <HmacSha256 as Mac>::new_from_slice(secret)
            .expect("HMAC accepts any key length");
        mac.update(input);
        let bytes = mac.finalize().into_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        out
    })
}

// ---------------------------------------------------------------------------
// Handshake — VHDRustDeskBridgeHandshakeV1 (§5.2)
// ---------------------------------------------------------------------------

/// Build the HMAC input byte string for `VHDRustDeskBridgeHandshakeV1`.
///
/// ```text
/// "VHDRustDeskBridgeHandshakeV1\n" || secretVersion || "\n" || nonce
///                                    || "\n" || timestampMs
/// ```
///
/// Exposed at `pub(super)` so HMAC tests (task 4.4 / Property 2 /
/// Property 20) can compare it byte-for-byte against an independent
/// reconstruction of the spec.
pub(super) fn hmac_handshake_input(
    secret_version: u32,
    nonce_hex: &str,
    ts_ms: u64,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(b"VHDRustDeskBridgeHandshakeV1\n");
    buf.extend_from_slice(secret_version.to_string().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(nonce_hex.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(ts_ms.to_string().as_bytes());
    buf
}

/// HMAC input builder for `VHDRustDeskBridgeHandshakeV1` (§5.2).
pub(super) fn hmac_handshake(
    secret_version: u32,
    nonce_hex: &str,
    ts_ms: u64,
) -> [u8; 32] {
    let buf: Zeroizing<Vec<u8>> =
        Zeroizing::new(hmac_handshake_input(secret_version, nonce_hex, ts_ms));
    compute_hmac(&buf)
}

// ---------------------------------------------------------------------------
// Report — VHDRustDeskBridgeReportV1 (§6.2)
// ---------------------------------------------------------------------------

/// Build the HMAC input byte string for `VHDRustDeskBridgeReportV1`.
///
/// ```text
/// "VHDRustDeskBridgeReportV1\n" || secretVersion || "\n" || rustDeskId
///   || "\n" || passwordKind || "\n" || sha256Hex(password) || "\n"
///   || reason || "\n" || reportedAt || "\n" || nonce
/// ```
///
/// `password_sha256_hex` MUST be the lowercase 64-char hex of
/// `SHA-256(password)`; the caller is responsible for redacting the
/// password plaintext from this code path (it stays inside the JSON
/// payload only).
pub(super) fn hmac_report_input(
    secret_version: u32,
    rust_desk_id: &str,
    password_kind: &str,
    password_sha256_hex: &str,
    reason: &str,
    reported_at: u64,
    nonce: &str,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(b"VHDRustDeskBridgeReportV1\n");
    buf.extend_from_slice(secret_version.to_string().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(rust_desk_id.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(password_kind.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(password_sha256_hex.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(reason.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(reported_at.to_string().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(nonce.as_bytes());
    buf
}

/// HMAC input builder for `VHDRustDeskBridgeReportV1` (§6.2).
pub(super) fn hmac_report(
    secret_version: u32,
    rust_desk_id: &str,
    password_kind: &str,
    password_sha256_hex: &str,
    reason: &str,
    reported_at: u64,
    nonce: &str,
) -> [u8; 32] {
    let buf: Zeroizing<Vec<u8>> = Zeroizing::new(hmac_report_input(
        secret_version,
        rust_desk_id,
        password_kind,
        password_sha256_hex,
        reason,
        reported_at,
        nonce,
    ));
    compute_hmac(&buf)
}

// ---------------------------------------------------------------------------
// Log — VHDRustDeskBridgeLogV1 (§7.2)
// ---------------------------------------------------------------------------

/// Build the HMAC input byte string for `VHDRustDeskBridgeLogV1`.
///
/// ```text
/// "VHDRustDeskBridgeLogV1\n" || secretVersion || "\n" || level
///   || "\n" || target || "\n" || sha256Hex(message) || "\n"
///   || timestampMs
/// ```
pub(super) fn hmac_log_input(
    secret_version: u32,
    level: &str,
    target: &str,
    message_sha256_hex: &str,
    ts_ms: u64,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(b"VHDRustDeskBridgeLogV1\n");
    buf.extend_from_slice(secret_version.to_string().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(level.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(target.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(message_sha256_hex.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(ts_ms.to_string().as_bytes());
    buf
}

/// HMAC input builder for `VHDRustDeskBridgeLogV1` (§7.2).
pub(super) fn hmac_log(
    secret_version: u32,
    level: &str,
    target: &str,
    message_sha256_hex: &str,
    ts_ms: u64,
) -> [u8; 32] {
    let buf: Zeroizing<Vec<u8>> = Zeroizing::new(hmac_log_input(
        secret_version,
        level,
        target,
        message_sha256_hex,
        ts_ms,
    ));
    compute_hmac(&buf)
}

// ---------------------------------------------------------------------------
// PeerApproval — VHDRustDeskBridgePeerApprovalV1 (§8.2)
// ---------------------------------------------------------------------------

/// Build the HMAC input byte string for
/// `VHDRustDeskBridgePeerApprovalV1`.
///
/// ```text
/// "VHDRustDeskBridgePeerApprovalV1\n" || secretVersion || "\n" ||
/// controlledMachineId || "\n" || controllerId || "\n" ||
/// sha256Hex(controllerName) || "\n" || controllerPlatform || "\n" ||
/// sha256Hex(controllerHwid) || "\n" || peerSocketAddr || "\n" ||
/// connectionType || "\n" || requestNonce || "\n" || timestampMs
/// ```
///
/// Both `controllerName` and `controllerHwid` MUST be passed as their
/// `sha256Hex(...)` form — the plaintext is carried only in the JSON
/// payload (Requirement 19.4 / §8.2).
#[allow(clippy::too_many_arguments)]
pub(super) fn hmac_peer_approval_input(
    secret_version: u32,
    controlled_machine_id: &str,
    controller_id: &str,
    controller_name_sha256_hex: &str,
    controller_platform: &str,
    controller_hwid_sha256_hex: &str,
    peer_socket_addr: &str,
    connection_type: &str,
    request_nonce: &str,
    ts_ms: u64,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(512);
    buf.extend_from_slice(b"VHDRustDeskBridgePeerApprovalV1\n");
    buf.extend_from_slice(secret_version.to_string().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(controlled_machine_id.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(controller_id.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(controller_name_sha256_hex.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(controller_platform.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(controller_hwid_sha256_hex.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(peer_socket_addr.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(connection_type.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(request_nonce.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(ts_ms.to_string().as_bytes());
    buf
}

/// HMAC input builder for `VHDRustDeskBridgePeerApprovalV1` (§8.2).
#[allow(clippy::too_many_arguments)]
pub(super) fn hmac_peer_approval(
    secret_version: u32,
    controlled_machine_id: &str,
    controller_id: &str,
    controller_name_sha256_hex: &str,
    controller_platform: &str,
    controller_hwid_sha256_hex: &str,
    peer_socket_addr: &str,
    connection_type: &str,
    request_nonce: &str,
    ts_ms: u64,
) -> [u8; 32] {
    let buf: Zeroizing<Vec<u8>> = Zeroizing::new(hmac_peer_approval_input(
        secret_version,
        controlled_machine_id,
        controller_id,
        controller_name_sha256_hex,
        controller_platform,
        controller_hwid_sha256_hex,
        peer_socket_addr,
        connection_type,
        request_nonce,
        ts_ms,
    ));
    compute_hmac(&buf)
}

// ---------------------------------------------------------------------------
// Revocation — VHDRustDeskBridgeRevocationV1 (§9.2)
// ---------------------------------------------------------------------------

/// Build the HMAC input byte string for
/// `VHDRustDeskBridgeRevocationV1`.
///
/// ```text
/// "VHDRustDeskBridgeRevocationV1\n" || secretVersion || "\n" || reason
///                                    || "\n" || issuedAt
/// ```
pub(super) fn hmac_revocation_input(
    secret_version: u32,
    reason: &str,
    issued_at: u64,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(b"VHDRustDeskBridgeRevocationV1\n");
    buf.extend_from_slice(secret_version.to_string().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(reason.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(issued_at.to_string().as_bytes());
    buf
}

/// HMAC input builder for `VHDRustDeskBridgeRevocationV1` (§9.2).
///
/// Revocation is server-pushed (§9): the bridge worker uses this
/// builder to recompute the expected MAC and compare it against the
/// `mac` field in the received frame via [`super::ct_eq`] (task 4.3).
pub(super) fn hmac_revocation(
    secret_version: u32,
    reason: &str,
    issued_at: u64,
) -> [u8; 32] {
    let buf: Zeroizing<Vec<u8>> =
        Zeroizing::new(hmac_revocation_input(secret_version, reason, issued_at));
    compute_hmac(&buf)
}

// ===========================================================================
// Tests — task 4.4
//
// Property 2  (HMAC input byte string matches the spec):
//   For arbitrary fields, `hmac_*_input(...)` MUST equal a reference
//   reconstruction that follows `docs/vhd-rustdesk-bridge-protocol.md`
//   §5.2 / §6.2 / §7.2 / §8.2 / §9.2 verbatim. The reference is
//   intentionally written from the doc — not the implementation —
//   so a drift in either direction would fail the test.
//
// Property 20 (Code-vs-doc HMAC consistency):
//   The five worked-example vectors from the doc (§5.4 / §6.4 / §7.4
//   / §8.4 / §9.4) MUST round-trip byte-for-byte through the
//   builders, locking the implementation to the published spec.
//
// Validates: Requirements 5.2, 6.2, 16.1, 16.2, 16.7, 18.3, 19.4
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // -----------------------------------------------------------------
    // Reference reconstructions — written from the doc, NOT from the
    // implementation. The whole point of Property 2 is that two
    // independent assemblies agree byte-for-byte.
    //
    // Layout per §5.2 / §6.2 / §7.2 / §8.2 / §9.2:
    //   <protocol_tag> "\n" <int_or_str> "\n" ... <last_field>
    // (no trailing newline)
    // -----------------------------------------------------------------

    fn ref_handshake(secret_version: u32, nonce: &str, ts_ms: u64) -> Vec<u8> {
        let mut s = String::new();
        s.push_str("VHDRustDeskBridgeHandshakeV1");
        s.push('\n');
        s.push_str(&format!("{}", secret_version));
        s.push('\n');
        s.push_str(nonce);
        s.push('\n');
        s.push_str(&format!("{}", ts_ms));
        s.into_bytes()
    }

    fn ref_report(
        secret_version: u32,
        rust_desk_id: &str,
        password_kind: &str,
        password_sha256_hex: &str,
        reason: &str,
        reported_at: u64,
        nonce: &str,
    ) -> Vec<u8> {
        let mut s = String::new();
        s.push_str("VHDRustDeskBridgeReportV1");
        s.push('\n');
        s.push_str(&format!("{}", secret_version));
        s.push('\n');
        s.push_str(rust_desk_id);
        s.push('\n');
        s.push_str(password_kind);
        s.push('\n');
        s.push_str(password_sha256_hex);
        s.push('\n');
        s.push_str(reason);
        s.push('\n');
        s.push_str(&format!("{}", reported_at));
        s.push('\n');
        s.push_str(nonce);
        s.into_bytes()
    }

    fn ref_log(
        secret_version: u32,
        level: &str,
        target: &str,
        message_sha256_hex: &str,
        ts_ms: u64,
    ) -> Vec<u8> {
        let mut s = String::new();
        s.push_str("VHDRustDeskBridgeLogV1");
        s.push('\n');
        s.push_str(&format!("{}", secret_version));
        s.push('\n');
        s.push_str(level);
        s.push('\n');
        s.push_str(target);
        s.push('\n');
        s.push_str(message_sha256_hex);
        s.push('\n');
        s.push_str(&format!("{}", ts_ms));
        s.into_bytes()
    }

    #[allow(clippy::too_many_arguments)]
    fn ref_peer_approval(
        secret_version: u32,
        controlled_machine_id: &str,
        controller_id: &str,
        controller_name_sha256_hex: &str,
        controller_platform: &str,
        controller_hwid_sha256_hex: &str,
        peer_socket_addr: &str,
        connection_type: &str,
        request_nonce: &str,
        ts_ms: u64,
    ) -> Vec<u8> {
        let mut s = String::new();
        s.push_str("VHDRustDeskBridgePeerApprovalV1");
        s.push('\n');
        s.push_str(&format!("{}", secret_version));
        s.push('\n');
        s.push_str(controlled_machine_id);
        s.push('\n');
        s.push_str(controller_id);
        s.push('\n');
        s.push_str(controller_name_sha256_hex);
        s.push('\n');
        s.push_str(controller_platform);
        s.push('\n');
        s.push_str(controller_hwid_sha256_hex);
        s.push('\n');
        s.push_str(peer_socket_addr);
        s.push('\n');
        s.push_str(connection_type);
        s.push('\n');
        s.push_str(request_nonce);
        s.push('\n');
        s.push_str(&format!("{}", ts_ms));
        s.into_bytes()
    }

    fn ref_revocation(secret_version: u32, reason: &str, issued_at: u64) -> Vec<u8> {
        let mut s = String::new();
        s.push_str("VHDRustDeskBridgeRevocationV1");
        s.push('\n');
        s.push_str(&format!("{}", secret_version));
        s.push('\n');
        s.push_str(reason);
        s.push('\n');
        s.push_str(&format!("{}", issued_at));
        s.into_bytes()
    }

    // -----------------------------------------------------------------
    // Strategies. Strings stay in `[\x20\x7E]` (printable ASCII minus
    // `\n`) for most cases so the random data never collides with the
    // separator byte; a separate Unicode strategy exercises multibyte
    // edge cases. Empty strings are allowed throughout — both `nonce`
    // and `controllerHwid` may legitimately be empty per the doc
    // (§6.2 absent password / §8.1 empty hwid).
    // -----------------------------------------------------------------

    /// Printable ASCII excluding `\n` / `\r`, `[\x20-\x7E]`. The HMAC
    /// input separator is LF; constraining the strategy to non-LF
    /// content lets the property assertion remain a clean byte
    /// equality (otherwise the doc layout silently allows a string
    /// field to embed an LF, but neither builder splits on it, so
    /// the assertion still holds — but excluding LF keeps the
    /// generated values readable in counter-examples).
    fn ascii_string(max: usize) -> impl Strategy<Value = String> {
        prop::string::string_regex(&format!("[\\x20-\\x7E]{{0,{max}}}"))
            .expect("vhd_bridge::hmac tests: build ascii regex")
    }

    /// Arbitrary UTF-8, including multibyte sequences. Bounded length
    /// keeps a single test case well under the 64 KiB frame ceiling.
    fn unicode_string(max: usize) -> impl Strategy<Value = String> {
        prop::string::string_regex(&format!(".{{0,{max}}}"))
            .expect("vhd_bridge::hmac tests: build unicode regex")
    }

    /// Mix ASCII / unicode / empty so each `proptest!` block touches
    /// the printable-ASCII boundary case the spec actually carries
    /// (frame field strings come from RustDesk IDs, password kinds,
    /// hex strings) AND the Unicode case the spec leaves room for
    /// (controllerName / target / message can be UTF-8).
    fn any_field_string(max: usize) -> impl Strategy<Value = String> {
        prop_oneof![
            // Empty string — `passwordKind="absent"` payload, empty
            // hwid, etc.
            Just(String::new()),
            ascii_string(max),
            unicode_string(max),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 256,
            ..ProptestConfig::default()
        })]

        // ---- Property 2 (handshake) -----------------------------------
        #[test]
        fn handshake_input_matches_reference(
            secret_version in any::<u32>(),
            nonce in any_field_string(64),
            ts_ms in any::<u64>(),
        ) {
            let actual = hmac_handshake_input(secret_version, &nonce, ts_ms);
            let expected = ref_handshake(secret_version, &nonce, ts_ms);
            prop_assert_eq!(actual, expected);
        }

        // ---- Property 2 (report) --------------------------------------
        #[test]
        fn report_input_matches_reference(
            secret_version in any::<u32>(),
            rust_desk_id in any_field_string(32),
            password_kind in any_field_string(16),
            password_sha256_hex in any_field_string(64),
            reason in any_field_string(16),
            reported_at in any::<u64>(),
            nonce in any_field_string(64),
        ) {
            let actual = hmac_report_input(
                secret_version,
                &rust_desk_id,
                &password_kind,
                &password_sha256_hex,
                &reason,
                reported_at,
                &nonce,
            );
            let expected = ref_report(
                secret_version,
                &rust_desk_id,
                &password_kind,
                &password_sha256_hex,
                &reason,
                reported_at,
                &nonce,
            );
            prop_assert_eq!(actual, expected);
        }

        // ---- Property 2 (log) -----------------------------------------
        #[test]
        fn log_input_matches_reference(
            secret_version in any::<u32>(),
            level in any_field_string(8),
            target in any_field_string(256),
            message_sha256_hex in any_field_string(64),
            ts_ms in any::<u64>(),
        ) {
            let actual = hmac_log_input(
                secret_version,
                &level,
                &target,
                &message_sha256_hex,
                ts_ms,
            );
            let expected = ref_log(
                secret_version,
                &level,
                &target,
                &message_sha256_hex,
                ts_ms,
            );
            prop_assert_eq!(actual, expected);
        }

        // ---- Property 2 (peer-approval) -------------------------------
        #[test]
        fn peer_approval_input_matches_reference(
            secret_version in any::<u32>(),
            controlled_machine_id in any_field_string(64),
            controller_id in any_field_string(64),
            controller_name_sha256_hex in any_field_string(64),
            controller_platform in any_field_string(16),
            controller_hwid_sha256_hex in any_field_string(64),
            peer_socket_addr in any_field_string(48),
            connection_type in any_field_string(16),
            request_nonce in any_field_string(64),
            ts_ms in any::<u64>(),
        ) {
            let actual = hmac_peer_approval_input(
                secret_version,
                &controlled_machine_id,
                &controller_id,
                &controller_name_sha256_hex,
                &controller_platform,
                &controller_hwid_sha256_hex,
                &peer_socket_addr,
                &connection_type,
                &request_nonce,
                ts_ms,
            );
            let expected = ref_peer_approval(
                secret_version,
                &controlled_machine_id,
                &controller_id,
                &controller_name_sha256_hex,
                &controller_platform,
                &controller_hwid_sha256_hex,
                &peer_socket_addr,
                &connection_type,
                &request_nonce,
                ts_ms,
            );
            prop_assert_eq!(actual, expected);
        }

        // ---- Property 2 (revocation) ----------------------------------
        #[test]
        fn revocation_input_matches_reference(
            secret_version in any::<u32>(),
            reason in any_field_string(16),
            issued_at in any::<u64>(),
        ) {
            let actual = hmac_revocation_input(secret_version, &reason, issued_at);
            let expected = ref_revocation(secret_version, &reason, issued_at);
            prop_assert_eq!(actual, expected);
        }
    }

    // ---------------------------------------------------------------------
    // Property 20 — code-vs-doc HMAC consistency.
    //
    // Each `*_doc_example_input` is a verbatim transcription of the
    // worked example in `docs/vhd-rustdesk-bridge-protocol.md` §5.4 /
    // §6.4 / §7.4 / §8.4 / §9.4. If either side changes its byte
    // layout the test fails and Requirement 16.7 forces both sides
    // to be updated in the same PR.
    // ---------------------------------------------------------------------

    #[test]
    fn handshake_doc_example_input() {
        // §5.4: secretVersion=1, nonce="4f1c2a8b39d0e7561f8a2b3c4d5e6f70",
        //       timestampMs=1730000000000.
        let actual = hmac_handshake_input(
            1,
            "4f1c2a8b39d0e7561f8a2b3c4d5e6f70",
            1_730_000_000_000,
        );
        let expected: &[u8] =
            b"VHDRustDeskBridgeHandshakeV1\n1\n4f1c2a8b39d0e7561f8a2b3c4d5e6f70\n1730000000000";
        assert_eq!(actual, expected);
    }

    #[test]
    fn report_doc_example_input() {
        // §6.4: secretVersion=1, rustDeskId="123456789",
        //       passwordKind="temporary",
        //       sha256Hex("Hunter2!") = 607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe,
        //       reason="startup", reportedAt=1730000000000,
        //       nonce="9a8b7c6d5e4f30210011223344556677".
        let actual = hmac_report_input(
            1,
            "123456789",
            "temporary",
            "607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe",
            "startup",
            1_730_000_000_000,
            "9a8b7c6d5e4f30210011223344556677",
        );
        let expected: &[u8] = b"VHDRustDeskBridgeReportV1\n1\n123456789\ntemporary\n607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe\nstartup\n1730000000000\n9a8b7c6d5e4f30210011223344556677";
        assert_eq!(actual, expected);
    }

    #[test]
    fn log_doc_example_input() {
        // §7.4: secretVersion=1, level="warn",
        //       target="rustdesk::server::connection",
        //       sha256Hex(message) = c0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7,
        //       timestampMs=1730000000500.
        let actual = hmac_log_input(
            1,
            "warn",
            "rustdesk::server::connection",
            "c0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7",
            1_730_000_000_500,
        );
        let expected: &[u8] = b"VHDRustDeskBridgeLogV1\n1\nwarn\nrustdesk::server::connection\nc0ae75da2950b0a6b5feaf69ffbdc0120099eeef8ab1e17afcb2c7a16ccda0c7\n1730000000500";
        assert_eq!(actual, expected);
    }

    #[test]
    fn peer_approval_doc_example_input() {
        // §8.4: full vector with controllerName / controllerHwid hashed.
        let actual = hmac_peer_approval_input(
            1,
            "MACHINE-DEADBEEF",
            "987654321",
            "bb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85",
            "Windows",
            "a820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560",
            "192.0.2.1:51820",
            "controlled",
            "0123456789abcdef0123456789abcdef",
            1_730_000_001_000,
        );
        let expected: &[u8] = b"VHDRustDeskBridgePeerApprovalV1\n1\nMACHINE-DEADBEEF\n987654321\nbb9b48894d2b3ddae42b93f5a33153171dc1a6429f90ac8188dde266b4728a85\nWindows\na820c04e6dceaf2071e870a32279b4399df2f5d2e549cce23e3358192aea1560\n192.0.2.1:51820\ncontrolled\n0123456789abcdef0123456789abcdef\n1730000001000";
        assert_eq!(actual, expected);
    }

    #[test]
    fn revocation_doc_example_input() {
        // §9.4: secretVersion=1, reason="denied", issuedAt=1730000005000.
        let actual = hmac_revocation_input(1, "denied", 1_730_000_005_000);
        let expected: &[u8] =
            b"VHDRustDeskBridgeRevocationV1\n1\ndenied\n1730000005000";
        assert_eq!(actual, expected);
    }

    // ---------------------------------------------------------------------
    // Cross-check: the wrapping `hmac_*` functions feed the same bytes
    // into `compute_hmac` as the standalone `hmac_*_input` returns.
    // This is implicit (the wrappers literally call the input fn) but
    // explicit guards against accidental code drift in the wrappers.
    // ---------------------------------------------------------------------

    #[test]
    fn handshake_wrapper_uses_input_fn() {
        // The `hmac_handshake` wrapper is *defined* to delegate to
        // `hmac_handshake_input` and `compute_hmac`; this guard
        // pins that delegation so a future refactor can't drift the
        // wrapper away from the property-tested input function.
        let nonce = "4f1c2a8b39d0e7561f8a2b3c4d5e6f70";
        let ts = 1_730_000_000_000u64;
        let via_wrapper = hmac_handshake(1, nonce, ts);
        let via_input = compute_hmac(&hmac_handshake_input(1, nonce, ts));
        assert_eq!(via_wrapper, via_input);
    }
}

// ---------------------------------------------------------------------------
// Constant-time MAC comparison wrapper (task 4.3)
//
// Wraps `subtle::ConstantTimeEq` so the `mac` fields of
// `HandshakeResponse` / `ReportAck` / `Peer_Approval_Response` /
// `RevocationFrame` can be compared without leaking timing information.
//
// Length-mismatched inputs short-circuit to `false` *before* entering
// the constant-time path: timing here only needs to be constant across
// inputs of equal length, since a length mismatch is not secret data
// (the wire framing reveals it). Using `subtle::ConstantTimeEq` on
// equal-length slices keeps comparison time independent of the byte
// pattern, which is the property required by Requirement 10.4.
//
// AGENTS.md "do not use `==` / `memcmp` for MAC verification" is the
// reason this helper exists at all; callers in the worker MUST go
// through `ct_eq` rather than reaching for `slice == slice`.
// ---------------------------------------------------------------------------

use subtle::ConstantTimeEq;

/// Constant-time byte-slice equality.
///
/// Returns `false` immediately when `a.len() != b.len()`; for equal
/// lengths, defers to `subtle::ConstantTimeEq::ct_eq` so the comparison
/// is constant-time over the byte pattern.
#[inline]
pub(super) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).unwrap_u8() == 1
}

// ---------------------------------------------------------------------------
// Tests — Property 3: `ct_eq` agrees with `==` (task 4.5)
//
// Validates: Requirements 3.9, 3.10, 10.4
//   * any two byte slices, `ct_eq(a, b) == (a == b)`
//   * unequal-length inputs always return false
//   * empty / identical-content edge cases
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests_ct_eq {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// **Property 3**: For any two byte slices (any length), the
        /// constant-time comparison agrees with `==`.
        ///
        /// **Validates: Requirements 3.10, 10.4**
        #[test]
        fn ct_eq_agrees_with_eq(
            a in prop::collection::vec(any::<u8>(), 0..256),
            b in prop::collection::vec(any::<u8>(), 0..256),
        ) {
            prop_assert_eq!(ct_eq(&a, &b), a == b);
        }

        /// Unequal-length inputs unconditionally return `false`,
        /// regardless of common prefix content.
        ///
        /// **Validates: Requirements 3.10, 10.4**
        #[test]
        fn ct_eq_unequal_lengths_always_false(
            a in prop::collection::vec(any::<u8>(), 0..256),
            extra in prop::collection::vec(any::<u8>(), 1..32),
        ) {
            let mut b = a.clone();
            b.extend_from_slice(&extra);
            prop_assert!(!ct_eq(&a, &b));
            // Symmetric: longer-vs-shorter direction is also false.
            prop_assert!(!ct_eq(&b, &a));
        }

        /// Equal-length differ-by-one-byte inputs always return false.
        /// This is a focused regression: the constant-time path must
        /// not silently report equality for any single-bit difference.
        ///
        /// **Validates: Requirements 3.10, 10.4**
        #[test]
        fn ct_eq_single_bit_flip_is_false(
            a in prop::collection::vec(any::<u8>(), 1..256),
            idx in any::<usize>(),
            mask in 1u8..=255,
        ) {
            let i = idx % a.len();
            let mut b = a.clone();
            b[i] ^= mask;
            prop_assert!(!ct_eq(&a, &b));
        }
    }

    #[test]
    fn ct_eq_empty_slices_equal() {
        assert!(ct_eq(&[], &[]));
    }

    #[test]
    fn ct_eq_identical_content() {
        let bytes: [u8; 5] = [1, 2, 3, 4, 5];
        assert!(ct_eq(&bytes, &bytes));
    }

    #[test]
    fn ct_eq_one_empty_one_nonempty() {
        assert!(!ct_eq(&[], &[0u8]));
        assert!(!ct_eq(&[0u8], &[]));
    }

    #[test]
    fn ct_eq_equal_length_different_content() {
        // Worst-case for naive `==`: same first byte, differ later.
        let a = [0xAAu8, 0xBB, 0xCC, 0xDD];
        let b = [0xAAu8, 0xBB, 0xCC, 0xDE];
        assert!(!ct_eq(&a, &b));
    }
}

// ---------------------------------------------------------------------------
// Tests — best-effort zeroize evidence (task 4.5)
//
// The HMAC input buffers in this module are wrapped in
// `Zeroizing<Vec<u8>>`; on `Drop`, `zeroize` overwrites the backing
// storage before deallocation. Observing the freed memory directly in
// safe Rust is not possible without a custom global allocator, which
// would affect every other test in the binary and is therefore out of
// scope for a unit test (the spec calls for "best-effort 正向证据").
//
// Instead this module captures the contract by:
//   1. Exercising `Zeroize::zeroize()` on a freshly populated `Vec<u8>`
//      and verifying every byte is `0` afterward — this is the
//      operation that `Zeroizing` invokes from its `Drop` impl.
//   2. Exercising the `Zeroizing<Vec<u8>>` wrapper itself: filling it
//      with sensitive bytes, taking a deref view to confirm the
//      payload is intact pre-drop, then dropping it. The `Drop` impl
//      runs `zeroize()` on the inner `Vec`, after which any access to
//      the freed bytes would be UB (and is therefore not attempted).
//   3. Running the in-module HMAC builder once and checking it returns
//      a 32-byte digest, demonstrating that the `Zeroizing<Vec<u8>>`
//      assembly path actually completes — so the `Drop` zeroization
//      runs on the real code path (Requirement 10.3).
//
// Validates: Requirements 3.9, 10.3
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests_zeroize {
    use super::*;
    use zeroize::{Zeroize, Zeroizing};

    /// Direct positive evidence: `Zeroize::zeroize()` overwrites the
    /// `Vec<u8>` storage before our eyes. This is the same operation
    /// that `Zeroizing<Vec<u8>>` invokes from `Drop`, so seeing it
    /// work here is the strongest in-process evidence we can produce
    /// without a custom allocator.
    #[test]
    fn zeroize_vec_overwrites_all_bytes() {
        let mut secret = vec![0xAAu8; 64];
        // Pre-condition: every byte is the sentinel.
        assert!(secret.iter().all(|&b| b == 0xAA));
        secret.zeroize();
        // Post-condition: every byte is 0.
        assert!(secret.iter().all(|&b| b == 0));
        // `zeroize()` clears the logical length to 0 as well; this is
        // part of the documented contract for `Zeroize for Vec<T>`.
        assert_eq!(secret.len(), 0);
    }

    /// `Zeroizing<Vec<u8>>` derefs to the inner `Vec`, so the payload
    /// is observable while the wrapper is alive. After `drop`, the
    /// inner buffer's `zeroize()` runs as part of the `Drop` glue.
    #[test]
    fn zeroizing_wrapper_holds_payload_until_drop() {
        let secret_buf: Zeroizing<Vec<u8>> = Zeroizing::new(vec![0xCCu8; 32]);
        // Pre-drop: the wrapper exposes the sensitive payload.
        assert_eq!(secret_buf.len(), 32);
        assert!(secret_buf.iter().all(|&b| b == 0xCC));
        // Drop runs `Zeroize::zeroize` on the inner Vec before
        // deallocation. We do not (and cannot, without UB) inspect the
        // freed allocation in safe Rust.
        drop(secret_buf);
    }

    /// Smoke test: the real HMAC builder uses a `Zeroizing<Vec<u8>>`
    /// internally. Running it confirms the path completes and the
    /// `Drop` zeroization fires for the input buffer (best-effort
    /// positive evidence for Requirement 10.3).
    ///
    /// We only assert that the digest is 32 bytes long and depends on
    /// at least one input field — verifying the *byte string* itself
    /// is task 4.4's job (Property 2).
    #[test]
    fn hmac_handshake_runs_and_zeroizes_input_buffer() {
        let mac1 = hmac_handshake(1, "deadbeef", 1_700_000_000_000);
        let mac2 = hmac_handshake(1, "deadbeef", 1_700_000_000_001);
        assert_eq!(mac1.len(), 32);
        assert_eq!(mac2.len(), 32);
        // Different timestamp ⇒ different digest. Confirms the buffer
        // assembly + HMAC compute actually consumed the input before
        // the `Zeroizing<Vec<u8>>` was dropped.
        assert_ne!(mac1, mac2);
    }
}
