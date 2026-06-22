// vhd-machine-auth-bridge: table-driven EXAMPLE tests for the build.rs
// secret-injection path and (placeholder for §1.2b) the Build_Prereq_Vars
// gate.
//
// The test consumes the same `build_support` crate that `build.rs` itself
// uses, so every assertion exercises the production resolver — no
// re-implementation, no duplicated parser, no recursive `cargo build`.
//
// Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.12, 3.13, 14.1, 22.2, 22.3,
//            22.4, 22.5, 22.7, 22.9, 22.10
//
// Cases marked `#[ignore]` cover the Build_Prereq_Vars gate (HBBS_KEY /
// HBBS_HOST / HBBR_HOST) which is described in tasks §1.2b but not yet
// implemented; once the gate lands, those tests can be flipped on by
// removing `#[ignore]` and wiring up the new resolver entry points.
//
// Conventions:
//   * No `tempfile` dependency — tests build a per-test directory under
//     `std::env::temp_dir()` keyed by PID + an atomic counter, then clean
//     it up explicitly.
//   * Tests touch only files inside the per-test directory.  They never
//     mutate process-wide environment variables; the resolver takes
//     env values as `Option<&str>` arguments.

use build_support::{
    decode_b64_secret, decode_hex_secret, parse_secret_sec, resolve_build_prereq_vars,
    resolve_secret_version, resolve_shared_secret, BuildPrereqInputs, SecretInputs, SecretSecMap,
    SecretSource,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Test scaffolding
// ---------------------------------------------------------------------------

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TmpDir {
    path: PathBuf,
}

impl TmpDir {
    fn new(label: &str) -> Self {
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("rustdesk-build-tests-{}-{}-{}", pid, n, label));
        std::fs::create_dir_all(&path).expect("create temp dir");
        TmpDir { path }
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

const FIXTURE_HEX: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
// 32 bytes: 0x00,0x01,...,0x1f — distinct from the hex fixture so we can
// assert which source the resolver picked.
const FIXTURE_BIN: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
const FIXTURE_HEX_FROM_BIN: &str =
    "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const FIXTURE_B64: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="; // == FIXTURE_BIN
const FIXTURE_SEC_HEX: &str =
    "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100";

/// Reusable forbidden-leak assertion. Error messages must never echo back
/// the secret values, the bytes of `vhd_bridge_secret.bin`, or any line of
/// `secret.sec`.
fn assert_no_leak(err: &str, banned: &[&str]) {
    for b in banned {
        assert!(
            !err.contains(b),
            "error string leaked sensitive content {:?}; full error = {:?}",
            b,
            err
        );
    }
}

// ---------------------------------------------------------------------------
// Shared-secret priority chain (covers Req 3.1, 3.12, 22.7).
// ---------------------------------------------------------------------------

#[test]
fn priority_a_only_hex_env() {
    let empty = SecretSecMap::default();
    let inputs = SecretInputs {
        hex_env: Some(FIXTURE_HEX),
        b64_env: None,
        bin_bytes: None,
        sec_map: &empty,
    };
    let (bytes, src) = resolve_shared_secret(&inputs).expect("hex env resolves");
    assert_eq!(src, SecretSource::HexEnv);
    assert_eq!(bytes.len(), 32);
}

#[test]
fn priority_b_only_b64_env() {
    let empty = SecretSecMap::default();
    let inputs = SecretInputs {
        hex_env: None,
        b64_env: Some(FIXTURE_B64),
        bin_bytes: None,
        sec_map: &empty,
    };
    let (bytes, src) = resolve_shared_secret(&inputs).expect("b64 env resolves");
    assert_eq!(src, SecretSource::B64Env);
    assert_eq!(bytes, FIXTURE_BIN);
}

#[test]
fn priority_c_only_bin_file() {
    let empty = SecretSecMap::default();
    let inputs = SecretInputs {
        hex_env: None,
        b64_env: None,
        bin_bytes: Some(&FIXTURE_BIN),
        sec_map: &empty,
    };
    let (bytes, src) = resolve_shared_secret(&inputs).expect("bin file resolves");
    assert_eq!(src, SecretSource::BinFile);
    assert_eq!(bytes, FIXTURE_BIN);
}

#[test]
fn priority_d_only_secret_sec_line() {
    let mut sec = SecretSecMap::default();
    sec.vhdmount_key = Some(FIXTURE_SEC_HEX.to_string());
    let inputs = SecretInputs {
        hex_env: None,
        b64_env: None,
        bin_bytes: None,
        sec_map: &sec,
    };
    let (bytes, src) = resolve_shared_secret(&inputs).expect("secret.sec line resolves");
    assert_eq!(src, SecretSource::SecretSecLine);
    assert_eq!(bytes.len(), 32);
}

#[test]
fn priority_chain_hex_env_beats_b64_env_beats_bin_beats_secret_sec() {
    // §22.7 four-level priority: _HEX > _B64 > .bin > secret.sec.
    let mut sec = SecretSecMap::default();
    sec.vhdmount_key = Some(FIXTURE_SEC_HEX.to_string());

    // (i) HEX present + everything else present ⇒ HEX wins.
    {
        // HEX must be resolved without conflicting with B64 — exercise the
        // "HEX wins over .bin and secret.sec" subset.
        let inputs = SecretInputs {
            hex_env: Some(FIXTURE_HEX),
            b64_env: None,
            bin_bytes: Some(&FIXTURE_BIN),
            sec_map: &sec,
        };
        let (_, src) = resolve_shared_secret(&inputs).expect("hex wins");
        assert_eq!(src, SecretSource::HexEnv);
    }

    // (ii) No HEX, B64 + .bin + secret.sec all present ⇒ B64 wins.
    {
        let inputs = SecretInputs {
            hex_env: None,
            b64_env: Some(FIXTURE_B64),
            bin_bytes: Some(&FIXTURE_BIN),
            sec_map: &sec,
        };
        let (_, src) = resolve_shared_secret(&inputs).expect("b64 wins");
        assert_eq!(src, SecretSource::B64Env);
    }

    // (iii) No env, .bin + secret.sec ⇒ .bin wins.
    {
        let inputs = SecretInputs {
            hex_env: None,
            b64_env: None,
            bin_bytes: Some(&FIXTURE_BIN),
            sec_map: &sec,
        };
        let (_, src) = resolve_shared_secret(&inputs).expect("bin wins");
        assert_eq!(src, SecretSource::BinFile);
    }

    // (iv) Nothing but secret.sec ⇒ secret.sec wins.
    {
        let inputs = SecretInputs {
            hex_env: None,
            b64_env: None,
            bin_bytes: None,
            sec_map: &sec,
        };
        let (_, src) = resolve_shared_secret(&inputs).expect("secret.sec wins");
        assert_eq!(src, SecretSource::SecretSecLine);
    }
}

// ---------------------------------------------------------------------------
// Error cases (covers Req 3.2, 3.3, 3.4, 3.5).
// ---------------------------------------------------------------------------

#[test]
fn case_e_hex_and_b64_simultaneously_set_is_rejected() {
    let empty = SecretSecMap::default();
    let inputs = SecretInputs {
        hex_env: Some(FIXTURE_HEX),
        b64_env: Some(FIXTURE_B64),
        bin_bytes: None,
        sec_map: &empty,
    };
    let err = resolve_shared_secret(&inputs).unwrap_err();
    assert!(
        err.contains("VHD_BRIDGE_SECRET_HEX") && err.contains("VHD_BRIDGE_SECRET_B64"),
        "expected mutual-exclusion message to name both env vars; got {:?}",
        err
    );
    assert_no_leak(&err, &[FIXTURE_HEX, FIXTURE_B64]);
}

#[test]
fn case_f_length_error_hex_too_short_is_rejected() {
    let empty = SecretSecMap::default();
    let inputs = SecretInputs {
        hex_env: Some("deadbeef"), // 8 chars, must be 64
        b64_env: None,
        bin_bytes: None,
        sec_map: &empty,
    };
    let err = resolve_shared_secret(&inputs).unwrap_err();
    assert!(err.contains("VHD_BRIDGE_SECRET_HEX"));
    assert!(err.contains("64") || err.contains("32 bytes"));
    assert_no_leak(&err, &["deadbeef"]);
}

#[test]
fn case_f_length_error_bin_wrong_size_is_rejected() {
    let empty = SecretSecMap::default();
    let bad: [u8; 16] = [0u8; 16];
    let inputs = SecretInputs {
        hex_env: None,
        b64_env: None,
        bin_bytes: Some(&bad),
        sec_map: &empty,
    };
    let err = resolve_shared_secret(&inputs).unwrap_err();
    assert!(err.contains("vhd_bridge_secret.bin"));
    assert!(err.contains("32 bytes"));
}

#[test]
fn case_f_length_error_b64_short_is_rejected() {
    let empty = SecretSecMap::default();
    let inputs = SecretInputs {
        hex_env: None,
        b64_env: Some("AAA="),
        bin_bytes: None,
        sec_map: &empty,
    };
    let err = resolve_shared_secret(&inputs).unwrap_err();
    assert!(err.contains("VHD_BRIDGE_SECRET_B64"));
    assert!(err.contains("43") || err.contains("44"));
}

#[test]
fn case_g_all_missing_is_rejected() {
    let empty = SecretSecMap::default();
    let inputs = SecretInputs {
        hex_env: None,
        b64_env: None,
        bin_bytes: None,
        sec_map: &empty,
    };
    let err = resolve_shared_secret(&inputs).unwrap_err();
    // Error must list the three supported injection sources and the 32-byte
    // expected length so the operator knows where to fix it (Req 3.3).
    assert!(err.contains("VHD_BRIDGE_SECRET_HEX"));
    assert!(err.contains("VHD_BRIDGE_SECRET_B64"));
    assert!(err.contains("vhd_bridge_secret.bin"));
    assert!(err.contains("32 bytes"));
}

#[test]
fn case_h_illegal_chars_in_hex_is_rejected() {
    let empty = SecretSecMap::default();
    // 64 chars but contains 'g' which is not hex.
    let bad_hex = "g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert_eq!(bad_hex.len(), 64);
    let inputs = SecretInputs {
        hex_env: Some(bad_hex),
        b64_env: None,
        bin_bytes: None,
        sec_map: &empty,
    };
    let err = resolve_shared_secret(&inputs).unwrap_err();
    assert!(err.contains("VHD_BRIDGE_SECRET_HEX"));
    assert!(err.contains("non-hex"));
    assert_no_leak(&err, &[bad_hex]);
}

#[test]
fn case_h_illegal_chars_in_b64_is_rejected() {
    let empty = SecretSecMap::default();
    let mut bad_b64 = String::from(FIXTURE_B64);
    // Replace the first character with a '!' — not in the base64 alphabet.
    bad_b64.replace_range(0..1, "!");
    let inputs = SecretInputs {
        hex_env: None,
        b64_env: Some(&bad_b64),
        bin_bytes: None,
        sec_map: &empty,
    };
    let err = resolve_shared_secret(&inputs).unwrap_err();
    assert!(err.contains("VHD_BRIDGE_SECRET_B64"));
    assert!(err.contains("non-base64"));
    assert_no_leak(&err, &[bad_b64.as_str()]);
}

// ---------------------------------------------------------------------------
// secret.sec parser equivalence cases (covers Req 3.12, 22.5, 22.10).
// ---------------------------------------------------------------------------

fn write_sec_file(label: &str, contents: &str) -> (TmpDir, PathBuf) {
    let dir = TmpDir::new(label);
    let path = dir.path.join("secret.sec");
    std::fs::write(&path, contents).expect("write secret.sec fixture");
    (dir, path)
}

#[test]
fn secret_sec_full_width_and_ascii_colons_are_equivalent() {
    let ascii = "\
HBBS Key: keypayload\n\
HBBS Host: host.example:21116\n\
HBBR Host: host.example:21117\n\
VHDMount Key: ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100\n\
VHDMount Key Version: 7\n";
    let mixed = "\
HBBS Key： keypayload\n\
HBBS Host: host.example:21116\n\
HBBR Host：host.example:21117\n\
VHDMount Key ： ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100\n\
VHDMount Key Version：7\n";
    let (_d1, p1) = write_sec_file("colons-ascii", ascii);
    let (_d2, p2) = write_sec_file("colons-mixed", mixed);
    let m1 = parse_secret_sec(&p1);
    let m2 = parse_secret_sec(&p2);
    assert_eq!(m1, m2, "ascii ':' and full-width '：' must be equivalent");
    assert_eq!(m1.vhdmount_key_version.as_deref(), Some("7"));
    // host:port lines must keep their inner ':' (only first colon splits).
    assert_eq!(m1.hbbs_host.as_deref(), Some("host.example:21116"));
    assert_eq!(m1.hbbr_host.as_deref(), Some("host.example:21117"));
}

#[test]
fn secret_sec_unrecognized_lines_are_silently_ignored() {
    // Comments, blank lines, junk identifiers — none of these may produce
    // an error or leak into the recognized-key map.
    let contents = "\
# this is a comment line\n\
\n\
Random: bar\n\
HBBS Key: only-this-line-counts\n\
HBBS Host = wrong-syntax-no-colon\n\
\n\
VHDMount Key Version: 3\n";
    let (_d, p) = write_sec_file("unrecognized-lines", contents);
    let m = parse_secret_sec(&p);
    assert_eq!(m.hbbs_key.as_deref(), Some("only-this-line-counts"));
    assert_eq!(m.vhdmount_key_version.as_deref(), Some("3"));
    // Lines without ':' / '：' (HBBS Host = …) are dropped silently.
    assert!(m.hbbs_host.is_none());
    assert!(m.hbbr_host.is_none());
    assert!(m.vhdmount_key.is_none());
}

#[test]
fn secret_sec_missing_file_yields_empty_map_not_error() {
    let dir = TmpDir::new("missing-sec");
    let path = dir.path.join("does-not-exist.sec");
    let m = parse_secret_sec(&path);
    assert_eq!(m, SecretSecMap::default());
}

#[test]
fn secret_sec_only_first_colon_splits_the_line() {
    // Pathological line: "HBBS Host: a:b:c" must yield value = "a:b:c".
    let contents = "HBBS Host: a:b:c\n";
    let (_d, p) = write_sec_file("first-colon", contents);
    let m = parse_secret_sec(&p);
    assert_eq!(m.hbbs_host.as_deref(), Some("a:b:c"));
}

// ---------------------------------------------------------------------------
// secret_version resolver (covers Req 3.6, 3.13).
// ---------------------------------------------------------------------------

#[test]
fn case_j_version_env_wins_over_secret_sec_line() {
    let mut sec = SecretSecMap::default();
    sec.vhdmount_key_version = Some("99".to_string());
    let v = resolve_secret_version(Some("42"), &sec).expect("env parses");
    assert_eq!(v, 42, "VHD_BRIDGE_SECRET_VERSION env must win over secret.sec");
}

#[test]
fn version_secret_sec_line_used_when_env_unset() {
    let mut sec = SecretSecMap::default();
    sec.vhdmount_key_version = Some("17".to_string());
    let v = resolve_secret_version(None, &sec).expect("line parses");
    assert_eq!(v, 17);
}

#[test]
fn version_defaults_to_one_when_both_missing() {
    let sec = SecretSecMap::default();
    let v = resolve_secret_version(None, &sec).expect("default");
    assert_eq!(v, 1);
}

#[test]
fn version_unparseable_secret_sec_line_falls_back_to_default() {
    // §3.13: when secret.sec line is unparseable AND env is unset, default = 1.
    let mut sec = SecretSecMap::default();
    sec.vhdmount_key_version = Some("not-a-number".to_string());
    let v = resolve_secret_version(None, &sec).expect("falls back");
    assert_eq!(v, 1);
}

#[test]
fn version_unparseable_env_is_fatal() {
    // §3.6: env was set but is non-integer ⇒ build must fail. The resolver
    // returns Err; the build script translates that into a non-zero exit.
    let sec = SecretSecMap::default();
    let err = resolve_secret_version(Some("v2"), &sec).unwrap_err();
    assert!(err.contains("VHD_BRIDGE_SECRET_VERSION"));
}

// ---------------------------------------------------------------------------
// Decoder spot checks (covers Req 3.4 case-insensitive hex etc.).
// ---------------------------------------------------------------------------

#[test]
fn hex_decoder_accepts_uppercase_and_lowercase() {
    let upper = "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
    let lower = upper.to_ascii_lowercase();
    let a = decode_hex_secret(upper, "VHD_BRIDGE_SECRET_HEX").expect("upper");
    let b = decode_hex_secret(&lower, "VHD_BRIDGE_SECRET_HEX").expect("lower");
    assert_eq!(a, b);
}

#[test]
fn b64_decoder_accepts_unpadded_and_padded() {
    let padded = FIXTURE_B64;
    let unpadded = padded.trim_end_matches('=');
    assert_eq!(unpadded.len(), 43);
    let a = decode_b64_secret(padded, "VHD_BRIDGE_SECRET_B64").expect("padded");
    let b = decode_b64_secret(unpadded, "VHD_BRIDGE_SECRET_B64").expect("unpadded");
    assert_eq!(a, b);
    assert_eq!(a, FIXTURE_BIN);
}

#[test]
fn hex_from_secret_sec_decodes_to_same_bytes_as_explicit_hex() {
    // Sanity: secret.sec 'VHDMount Key' line is hex with the same semantics
    // as VHD_BRIDGE_SECRET_HEX.
    let hex_bytes = decode_hex_secret(FIXTURE_HEX_FROM_BIN, "VHD_BRIDGE_SECRET_HEX").unwrap();
    assert_eq!(hex_bytes, FIXTURE_BIN);
}

// ---------------------------------------------------------------------------
// §1.2b Build_Prereq_Vars gate — real cases.
//
// Validates the resolver's contract: env > secret.sec, base64-32 for
// HBBS_KEY, host[:port[-port2]] for HBBS_HOST, host[:port] for HBBR_HOST,
// reason-only error messages.  The gate's leniency activation policy
// ("only fail when ops attempted injection") is enforced by the build
// script wrapper, not the resolver itself; the resolver always returns
// Err on missing/invalid input so that wrapper has something to gate on.
// ---------------------------------------------------------------------------

const VALID_HBBS_KEY_B64: &str = FIXTURE_B64; // 32 bytes ⇒ 44-char base64
const VALID_HBBS_HOST: &str = "rs.example.com:21116";
const VALID_HBBR_HOST: &str = "relay.example.com:21117";

fn empty_inputs<'a>(sec: &'a SecretSecMap) -> BuildPrereqInputs<'a> {
    BuildPrereqInputs {
        hbbs_key_env: None,
        hbbs_host_env: None,
        hbbr_host_env: None,
        sec_map: sec,
    }
}

#[test]
fn build_prereq_a_hbbs_key_missing_is_rejected() {
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_host_env = Some(VALID_HBBS_HOST);
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(
        err.contains("HBBS_KEY"),
        "error must name the missing item; got {:?}",
        err
    );
    assert!(err.contains("env HBBS_KEY"));
    assert!(err.contains("secret.sec 'HBBS Key' line"));
    assert!(err.contains("32 bytes"));
}

#[test]
fn build_prereq_b_hbbs_host_missing_is_rejected() {
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_key_env = Some(VALID_HBBS_KEY_B64);
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(err.contains("HBBS_HOST"));
    assert!(err.contains("env HBBS_HOST"));
    assert!(err.contains("secret.sec 'HBBS Host' line"));
    assert!(err.contains("host[:port[-port2]]"));
}

#[test]
fn build_prereq_c_hbbr_host_missing_is_rejected() {
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_key_env = Some(VALID_HBBS_KEY_B64);
    inputs.hbbs_host_env = Some(VALID_HBBS_HOST);
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(err.contains("HBBR_HOST"));
    assert!(err.contains("env HBBR_HOST"));
    assert!(err.contains("secret.sec 'HBBR Host' line"));
    assert!(err.contains("host[:port]"));
}

#[test]
fn build_prereq_d_hbbs_key_invalid_base64_is_rejected() {
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    let bad = "!!!not-base64!!!";
    inputs.hbbs_key_env = Some(bad);
    inputs.hbbs_host_env = Some(VALID_HBBS_HOST);
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(err.contains("HBBS_KEY"));
    assert_no_leak(&err, &[bad]);
}

#[test]
fn build_prereq_e_hbbs_key_decoded_length_not_32_is_rejected() {
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    // 16 bytes encoded ⇒ 24 chars, fails the 43/44 length precheck.
    let too_short = "AAECAwQFBgcICQoLDA0ODw==";
    inputs.hbbs_key_env = Some(too_short);
    inputs.hbbs_host_env = Some(VALID_HBBS_HOST);
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(err.contains("HBBS_KEY"));
    assert!(err.contains("32 bytes"));
}

#[test]
fn build_prereq_f_host_with_invalid_port_is_rejected() {
    // Port out of [1, 65535].
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_key_env = Some(VALID_HBBS_KEY_B64);
    inputs.hbbs_host_env = Some("rs.example.com:99999");
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(err.contains("HBBS_HOST"));
    assert!(err.contains("[1, 65535]"));

    // port1 > port2.
    inputs.hbbs_host_env = Some("rs.example.com:30000-20000");
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(err.contains("HBBS_HOST"));

    // Non-numeric port.
    inputs.hbbs_host_env = Some("rs.example.com:abc");
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(err.contains("HBBS_HOST"));

    // HBBR_HOST does not allow a port range.
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_key_env = Some(VALID_HBBS_KEY_B64);
    inputs.hbbs_host_env = Some(VALID_HBBS_HOST);
    inputs.hbbr_host_env = Some("relay.example.com:21117-21118");
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert!(err.contains("HBBR_HOST"));
}

#[test]
fn build_prereq_g_mixed_full_width_and_ascii_colons_pass() {
    // secret.sec with mixed colons feeds the same parser (already covered
    // at the parser level by `secret_sec_full_width_and_ascii_colons_are_equivalent`).
    // Here we additionally assert the *gate* accepts the parsed result.
    let mixed = "\
HBBS Key： AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\n\
HBBS Host：rs.example.com:21116\n\
HBBR Host : relay.example.com:21117\n";
    let (_d, p) = write_sec_file("gate-mixed-colons", mixed);
    let sec = parse_secret_sec(&p);
    let inputs = empty_inputs(&sec);
    let v = resolve_build_prereq_vars(&inputs).expect("mixed colons accepted");
    assert_eq!(v.hbbs_host, "rs.example.com:21116");
    assert_eq!(v.hbbr_host, "relay.example.com:21117");
    assert_eq!(v.hbbs_key_b64_canonical, FIXTURE_B64);
}

#[test]
fn build_prereq_h_unrecognized_lines_do_not_break_validation() {
    let with_extras = "\
# random comment\n\
HBBS Key: AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\n\
Random: extra-line\n\
HBBS Host: rs.example.com:21116\n\
HBBR Host: relay.example.com:21117\n";
    let (_d, p) = write_sec_file("gate-extra-lines", with_extras);
    let sec = parse_secret_sec(&p);
    let inputs = empty_inputs(&sec);
    let v = resolve_build_prereq_vars(&inputs).expect("extra lines tolerated");
    assert_eq!(v.hbbs_host, "rs.example.com:21116");
}

#[test]
fn build_prereq_i_no_secret_sec_but_three_envs_set_passes() {
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_key_env = Some(VALID_HBBS_KEY_B64);
    inputs.hbbs_host_env = Some(VALID_HBBS_HOST);
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let v = resolve_build_prereq_vars(&inputs).expect("envs-only path");
    assert_eq!(v.hbbs_host, VALID_HBBS_HOST);
    assert_eq!(v.hbbr_host, VALID_HBBR_HOST);
}

#[test]
fn build_prereq_env_wins_over_secret_sec_line() {
    // Both env and secret.sec set HBBS_HOST; env value is what comes through.
    let mut sec = SecretSecMap::default();
    sec.hbbs_key = Some("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=".to_string());
    sec.hbbs_host = Some("from-file.example:21116".to_string());
    sec.hbbr_host = Some("relay-from-file.example:21117".to_string());
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_host_env = Some("from-env.example:21116");
    let v = resolve_build_prereq_vars(&inputs).expect("env wins over file");
    assert_eq!(v.hbbs_host, "from-env.example:21116");
    assert_eq!(v.hbbr_host, "relay-from-file.example:21117");
}

#[test]
fn build_prereq_no_leakage_in_error_messages() {
    // For every Err path: error string must not contain the rejected value
    // (nor the still-fine values from other inputs that happened to be
    // present).  Reuse `assert_no_leak` to flag any accidental echo.
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    let bad_key = "shouldnotappearinerror";
    let bad_host = "shouldalsoneverappear:21116";
    inputs.hbbs_key_env = Some(bad_key);
    inputs.hbbs_host_env = Some(VALID_HBBS_HOST);
    inputs.hbbr_host_env = Some(bad_host); // also bad — port ok but we'll spoil
                                            // by picking a port > 65535 below.
    inputs.hbbr_host_env = Some("h:99999");
    let err = resolve_build_prereq_vars(&inputs).unwrap_err();
    assert_no_leak(&err, &[bad_key, "h:99999"]);
}

#[test]
fn build_prereq_accepts_host_only_no_port() {
    // §22.3 says ports are optional. host-only must pass.
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_key_env = Some(VALID_HBBS_KEY_B64);
    inputs.hbbs_host_env = Some("rs.example.com");
    inputs.hbbr_host_env = Some("relay.example.com");
    let v = resolve_build_prereq_vars(&inputs).expect("host-only accepted");
    assert_eq!(v.hbbs_host, "rs.example.com");
    assert_eq!(v.hbbr_host, "relay.example.com");
}

#[test]
fn build_prereq_accepts_port_range_within_bounds() {
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    inputs.hbbs_key_env = Some(VALID_HBBS_KEY_B64);
    inputs.hbbs_host_env = Some("rs.example.com:21116-21120");
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let v = resolve_build_prereq_vars(&inputs).expect("port range accepted");
    assert_eq!(v.hbbs_host, "rs.example.com:21116-21120");
}

#[test]
fn build_prereq_canonical_b64_matches_decoded_bytes() {
    // The resolver re-encodes HBBS_KEY's decoded 32 bytes to canonical
    // padded base64.  Operators may type unpadded base64; the canonical
    // form is what gets injected into RS_PUB_KEY.
    let sec = SecretSecMap::default();
    let mut inputs = empty_inputs(&sec);
    let unpadded = FIXTURE_B64.trim_end_matches('=');
    inputs.hbbs_key_env = Some(unpadded);
    inputs.hbbs_host_env = Some(VALID_HBBS_HOST);
    inputs.hbbr_host_env = Some(VALID_HBBR_HOST);
    let v = resolve_build_prereq_vars(&inputs).expect("unpadded accepted");
    assert_eq!(v.hbbs_key_bytes, FIXTURE_BIN);
    assert_eq!(v.hbbs_key_b64_canonical, FIXTURE_B64);
}
