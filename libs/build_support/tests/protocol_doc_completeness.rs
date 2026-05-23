// vhd-machine-auth-bridge: protocol-doc completeness EXAMPLE test
// (task §20.4).
//
// Asserts that `docs/vhd-rustdesk-bridge-protocol.md` carries every
// required section heading and every required protocol literal
// produced by tasks §20.1 / §20.2 / §20.3, plus the redaction
// placeholder mandated by Requirement 16.5. A failure here means the
// document drifted from `.kiro/specs/vhd-machine-auth-bridge/design.md`
// or from the implementation in `src/vhd_bridge/` — both sides must be
// updated in the same PR (Requirement 16.7).
//
// Validates: Requirements 16.2, 16.5
//
// Hosted in the dependency-free `build_support` crate so that
// `cargo test -p build_support` exercises this without dragging in the
// full RustDesk Windows / vcpkg toolchain.

use std::path::PathBuf;

/// Resolve the protocol doc path relative to this crate's manifest dir
/// (`libs/build_support`). The workspace layout puts the doc at
/// `<workspace_root>/docs/vhd-rustdesk-bridge-protocol.md`, two levels
/// up from `CARGO_MANIFEST_DIR`.
fn protocol_doc_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("..")
        .join("..")
        .join("docs")
        .join("vhd-rustdesk-bridge-protocol.md")
}

fn read_protocol_doc() -> String {
    let path = protocol_doc_path();
    std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "docs/vhd-rustdesk-bridge-protocol.md must exist (looked up at {}): {}",
            path.display(),
            err
        )
    })
}

// ---------------------------------------------------------------------------
// §20.4 — required section headings
// ---------------------------------------------------------------------------

#[test]
fn protocol_doc_has_all_numbered_sections() {
    let content = read_protocol_doc();

    // Each entry is a `(label, prefix)` pair. We match by H2 heading
    // *prefix* rather than full title to absorb the descriptive tail
    // ("— `VHDRustDeskBridge…V1`", "& Nonce Anti-Replay", etc.) that
    // the doc adds for readability while still locking down the
    // numbering and the anchor word.
    let required_prefixes: &[(&str, &str)] = &[
        ("§1 Overview & Scope", "## 1. Overview"),
        ("§2 Transport", "## 2. Transport"),
        ("§3 HMAC-SHA256 Construction Rules", "## 3. HMAC-SHA256"),
        ("§4 Frame Catalog", "## 4. Frame Catalog"),
        ("§5 Handshake Frame", "## 5. Handshake Frame"),
        ("§6 Report Frame", "## 6. Report Frame"),
        ("§7 Log Frame", "## 7. Log Frame"),
        ("§8 Peer Approval Frame", "## 8. Peer Approval Frame"),
        ("§9 Revocation Frame", "## 9. Revocation Frame"),
        ("§10 Timing Window", "## 10. Timing Window"),
        ("§11 Error Codes", "## 11. Error Codes"),
        ("§12 Compatibility", "## 12. Compatibility"),
        ("§13 Round-trip Examples", "## 13. Round-trip"),
        ("§14 2FA disablement", "## 14. 2FA"),
    ];

    let missing: Vec<&str> = required_prefixes
        .iter()
        .filter_map(|(label, prefix)| {
            if heading_with_prefix_present(&content, prefix) {
                None
            } else {
                Some(*label)
            }
        })
        .collect();

    assert!(
        missing.is_empty(),
        "protocol doc is missing required sections: {:?}",
        missing
    );
}

/// Match an H2 heading prefix on a line whose first non-whitespace
/// characters are exactly the supplied prefix. We require the prefix
/// to start at column 0 (markdown H2 convention) and to be followed
/// either by EOL or by additional title text — never by a different
/// numeric segment (so `## 1.` does not accidentally match `## 11.`).
fn heading_with_prefix_present(content: &str, prefix: &str) -> bool {
    for line in content.lines() {
        if !line.starts_with(prefix) {
            continue;
        }
        // Anything immediately after the prefix must not extend the
        // numeric segment we just matched. e.g. when prefix = "## 1.",
        // a line "## 11. ..." would also start with "## 1" — but the
        // prefix already includes the trailing dot, so this is a
        // non-issue. We additionally require either end-of-line or a
        // space / tab separator after the prefix to avoid weird false
        // positives like "## 2. TransportSomething" (cannot occur in
        // current doc, but cheap to guard).
        let tail = &line[prefix.len()..];
        if tail.is_empty() || tail.starts_with(' ') || tail.starts_with('\t') {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// §20.4 — required protocol literals (frame names)
// ---------------------------------------------------------------------------

#[test]
fn protocol_doc_mentions_all_frame_literals() {
    let content = read_protocol_doc();

    let required_literals = [
        "VHDRustDeskBridgeHandshakeV1",
        "VHDRustDeskBridgeReportV1",
        "VHDRustDeskBridgeLogV1",
        "VHDRustDeskBridgePeerApprovalV1",
        "VHDRustDeskBridgeRevocationV1",
    ];

    let missing: Vec<&str> = required_literals
        .iter()
        .copied()
        .filter(|lit| !content.contains(lit))
        .collect();

    assert!(
        missing.is_empty(),
        "protocol doc is missing required frame literals: {:?}",
        missing
    );
}

// ---------------------------------------------------------------------------
// §20.4 — transport / framing invariants are documented
// ---------------------------------------------------------------------------

#[test]
fn protocol_doc_documents_transport_invariants() {
    let content = read_protocol_doc();

    // 64 KiB frame cap (Requirement 13.4 / §2.3).
    assert!(
        content.contains("MAX_FRAME_BYTES") || content.contains("64 KiB"),
        "frame size cap (MAX_FRAME_BYTES = 64 KiB) is not documented"
    );

    // HMAC algorithm pin (§3 / Requirement 16.2).
    assert!(
        content.contains("HMAC-SHA256"),
        "HMAC algorithm (HMAC-SHA256) is not documented"
    );

    // Pipe path (§2.1 / Requirement 16.3).
    assert!(
        content.contains(r"\\.\pipe\VHDMount.RustDeskBridge"),
        "named-pipe endpoint path is not documented"
    );
}

// ---------------------------------------------------------------------------
// §20.4 — secret redaction (Requirement 16.5)
// ---------------------------------------------------------------------------

#[test]
fn protocol_doc_uses_secret_placeholder_only() {
    let content = read_protocol_doc();

    // At least one explicit placeholder MUST appear in worked examples.
    assert!(
        content.contains("<32 random bytes>") || content.contains("REDACTED"),
        "doc must use a placeholder for `RustDeskClientSharedSecret`, not a real secret"
    );

    // Never embed a real 32-byte secret as 64 hex chars on the same
    // line as `RustDeskClientSharedSecret =` / `proof =` / `mac =`.
    // We scan for any line that pairs one of those tokens with a
    // contiguous run of ≥ 64 hex chars, which would be a strong
    // indicator of a leaked literal. The legitimate occurrences in
    // the doc are either placeholders (`<32 random bytes>` /
    // `REDACTED`) or hex strings used as `nonce` / `sha256Hex(...)`
    // which never sit on the same line as the secret-token names.
    for (idx, line) in content.lines().enumerate() {
        if !(line.contains("RustDeskClientSharedSecret =")
            || line.contains("\"proof\":") && line.contains("=")
            || line.contains("\"mac\":") && line.contains("="))
        {
            continue;
        }
        let hex_run = longest_lowercase_hex_run(line);
        assert!(
            hex_run < 64,
            "line {} appears to leak a 64-hex-char literal next to a secret token: {:?}",
            idx + 1,
            line
        );
    }
}

/// Return the length, in characters, of the longest contiguous run of
/// lowercase hex characters (`0-9a-f`) in `line`.
fn longest_lowercase_hex_run(line: &str) -> usize {
    let mut best = 0usize;
    let mut current = 0usize;
    for ch in line.chars() {
        if ch.is_ascii_digit() || matches!(ch, 'a'..='f') {
            current += 1;
            if current > best {
                best = current;
            }
        } else {
            current = 0;
        }
    }
    best
}
