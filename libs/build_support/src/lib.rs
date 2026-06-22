// vhd-machine-auth-bridge: pure helpers shared by `build.rs` (compile-time
// secret injection) and `tests/build_script_tests.rs` (table-driven
// EXAMPLE tests for the priority chain & error contract).
//
// Everything in this file MUST be:
//   * side-effect-free in terms of process control (no `die()` /
//     `std::process::exit` / panics on user input — return `Result<_, String>`),
//   * free of `eprintln!` / `println!` (the build script wraps these helpers
//     and is responsible for emitting cargo directives & error messages),
//   * free of feature gates and target-os gates (so it can be compiled by the
//     test harness regardless of whether `feature = "vhd-bridge"` is on).
//
// Error strings produced by these helpers SHALL NOT include the actual secret
// values, the contents of `vhd_bridge_secret.bin`, or any line of `secret.sec`.
// They reference only item names ("VHD_BRIDGE_SECRET_HEX", "secret.sec
// 'VHDMount Key' line", …), expected shapes, and offsets.

use std::path::Path;

// ---------------------------------------------------------------------------
// secret.sec parser (covers Requirements 3.12, 3.13, 22.5, 22.10).
// ---------------------------------------------------------------------------

/// Parsed view of a `secret.sec` development fallback file.
///
/// Lines that don't match a recognized name (empty lines, comments, anything
/// else) are silently dropped — `parse_secret_sec` never raises errors for
/// "unknown line" / "missing file"; downstream gates decide whether a missing
/// entry is fatal.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct SecretSecMap {
    pub hbbs_key: Option<String>,
    pub hbbs_host: Option<String>,
    pub hbbr_host: Option<String>,
    pub vhdmount_key: Option<String>,
    pub vhdmount_key_version: Option<String>,
}

/// Parse `secret.sec` if it exists.
///
/// Recognized line names (case-sensitive):
///   * `HBBS Key`
///   * `HBBS Host`
///   * `HBBR Host`
///   * `VHDMount Key`
///   * `VHDMount Key Version`
///
/// ASCII `:` and the full-width Chinese colon `：` (U+FF1A) are treated as
/// byte-level equivalent line separators; the line is split at the first
/// occurrence of either.  A second `:` / `：` later in the value is part of
/// the value (e.g. host:port lines split correctly).  `<Name>` and `<value>`
/// have ASCII whitespace trimmed from both ends.
///
/// Missing / unreadable file ⇒ empty map.  This function never echoes file
/// content via `println!` / `eprintln!`.
pub fn parse_secret_sec(path: &Path) -> SecretSecMap {
    let mut map = SecretSecMap::default();
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return map,
    };
    for line in content.lines() {
        // First ':' (ASCII) or '：' (U+FF1A), whichever comes earliest.
        let mut split: Option<(usize, usize)> = None;
        for (idx, ch) in line.char_indices() {
            if ch == ':' || ch == '\u{FF1A}' {
                split = Some((idx, idx + ch.len_utf8()));
                break;
            }
        }
        let Some((name_end, value_start)) = split else {
            continue;
        };
        let name = line[..name_end].trim_matches(|c: char| c.is_ascii_whitespace());
        let value = line[value_start..].trim_matches(|c: char| c.is_ascii_whitespace());
        match name {
            "HBBS Key" => map.hbbs_key = Some(value.to_string()),
            "HBBS Host" => map.hbbs_host = Some(value.to_string()),
            "HBBR Host" => map.hbbr_host = Some(value.to_string()),
            "VHDMount Key" => map.vhdmount_key = Some(value.to_string()),
            "VHDMount Key Version" => map.vhdmount_key_version = Some(value.to_string()),
            _ => {}
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Hex / Base64 decoders for the 32-byte shared secret
// (covers Requirements 3.1, 3.4 — strict length & character validation).
// ---------------------------------------------------------------------------

/// Decode a 64-character ASCII hex string into 32 bytes.
///
/// Errors mention only `source_name` and offsets — never the input itself.
pub fn decode_hex_secret(s: &str, source_name: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if s.len() != 64 {
        return Err(format!(
            "{} must be exactly 64 hex characters (32 bytes); got a different length",
            source_name
        ));
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(32);
    let mut i = 0usize;
    while i < 64 {
        let hi = match bytes[i] {
            b'0'..=b'9' => bytes[i] - b'0',
            b'a'..=b'f' => bytes[i] - b'a' + 10,
            b'A'..=b'F' => bytes[i] - b'A' + 10,
            _ => {
                return Err(format!(
                    "{} contains a non-hex character at offset {}",
                    source_name, i
                ));
            }
        };
        let lo = match bytes[i + 1] {
            b'0'..=b'9' => bytes[i + 1] - b'0',
            b'a'..=b'f' => bytes[i + 1] - b'a' + 10,
            b'A'..=b'F' => bytes[i + 1] - b'A' + 10,
            _ => {
                return Err(format!(
                    "{} contains a non-hex character at offset {}",
                    source_name, i + 1
                ));
            }
        };
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

/// Decode a 43- or 44-character standard Base64 string into 32 bytes.
pub fn decode_b64_secret(s: &str, source_name: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if !(s.len() == 43 || s.len() == 44) {
        return Err(format!(
            "{} must decode to exactly 32 bytes (43 or 44 base64 characters)",
            source_name
        ));
    }
    let mut out = Vec::with_capacity(33);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for (i, &c) in s.as_bytes().iter().enumerate() {
        if c == b'=' {
            break; // padding only valid at end; we stop accumulating
        }
        let v: u8 = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => {
                return Err(format!(
                    "{} contains a non-base64 character at offset {}",
                    source_name, i
                ));
            }
        };
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    if out.len() != 32 {
        return Err(format!(
            "{} did not decode to exactly 32 bytes",
            source_name
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Shared-secret resolver — pure orchestration for table-driven tests
// (covers Requirements 3.1, 3.2, 3.3, 3.4, 3.12, 14.1, 22.7).
// ---------------------------------------------------------------------------

/// Where the resolved 32-byte shared secret came from. Tests assert the
/// four-level priority `_HEX env > _B64 env > .bin > secret.sec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretSource {
    HexEnv,
    B64Env,
    BinFile,
    SecretSecLine,
}

/// Inputs to the resolver.  `bin_bytes = Some(_)` simulates the presence of
/// `vhd_bridge_secret.bin`; `None` simulates "file does not exist".
pub struct SecretInputs<'a> {
    pub hex_env: Option<&'a str>,
    pub b64_env: Option<&'a str>,
    pub bin_bytes: Option<&'a [u8]>,
    pub sec_map: &'a SecretSecMap,
}

/// Resolve the 32-byte shared secret with the documented priority order:
///   1) `VHD_BRIDGE_SECRET_HEX` env  (64 hex chars)
///   2) `VHD_BRIDGE_SECRET_B64` env  (43–44 base64 chars)
///   3) `vhd_bridge_secret.bin`      (32 raw bytes)
///   4) `secret.sec` `VHDMount Key`  (64 hex chars)
///
/// Errors:
///   * `_HEX` and `_B64` env both set         ⇒ Err (mutual-exclusion violation)
///   * any decode / length failure            ⇒ Err
///   * all four sources missing               ⇒ Err
///
/// Error strings reference only item names and expected shapes.
pub fn resolve_shared_secret(
    input: &SecretInputs<'_>,
) -> Result<(Vec<u8>, SecretSource), String> {
    if input.hex_env.is_some() && input.b64_env.is_some() {
        return Err(
            "VHD_BRIDGE_SECRET_HEX and VHD_BRIDGE_SECRET_B64 are mutually exclusive; \
             set only one of them"
                .to_string(),
        );
    }

    if let Some(hex) = input.hex_env {
        return decode_hex_secret(hex, "VHD_BRIDGE_SECRET_HEX")
            .and_then(|b| ensure_32(b, "VHD_BRIDGE_SECRET_HEX"))
            .map(|b| (b, SecretSource::HexEnv));
    }

    if let Some(b64) = input.b64_env {
        return decode_b64_secret(b64, "VHD_BRIDGE_SECRET_B64")
            .and_then(|b| ensure_32(b, "VHD_BRIDGE_SECRET_B64"))
            .map(|b| (b, SecretSource::B64Env));
    }

    if let Some(bytes) = input.bin_bytes {
        if bytes.len() != 32 {
            return Err("vhd_bridge_secret.bin must be exactly 32 bytes".to_string());
        }
        return Ok((bytes.to_vec(), SecretSource::BinFile));
    }

    if let Some(line) = input.sec_map.vhdmount_key.as_deref() {
        return decode_hex_secret(line, "secret.sec 'VHDMount Key' line")
            .and_then(|b| ensure_32(b, "secret.sec 'VHDMount Key' line"))
            .map(|b| (b, SecretSource::SecretSecLine));
    }

    Err(
        "no shared secret was provided. Set one of: \
         VHD_BRIDGE_SECRET_HEX (64 hex chars), \
         VHD_BRIDGE_SECRET_B64 (43-44 base64 chars), \
         vhd_bridge_secret.bin (32 raw bytes in workspace root), \
         or a 'VHDMount Key: <hex>' line in secret.sec. \
         Decoded length must be exactly 32 bytes"
            .to_string(),
    )
}

fn ensure_32(b: Vec<u8>, source_name: &str) -> Result<Vec<u8>, String> {
    if b.len() == 32 {
        Ok(b)
    } else {
        Err(format!(
            "{} did not decode to exactly 32 bytes",
            source_name
        ))
    }
}

// ---------------------------------------------------------------------------
// secret_version resolver — env wins over secret.sec line, default = 1
// (covers Requirements 3.6, 3.13, 14.2).
// ---------------------------------------------------------------------------

/// Resolve `Bridge_Config.secret_version` with the documented priority:
///   1) `VHD_BRIDGE_SECRET_VERSION` env  (must parse as u32)
///   2) `secret.sec` `VHDMount Key Version` line  (u32, unparseable ⇒ default)
///   3) default = 1
///
/// Returns `Err` only when the env var is set but doesn't parse as u32 — that
/// case is fatal per Requirement 3.6.  An unparseable secret.sec line, when
/// the env is unset, falls back to default per Requirement 3.13.
pub fn resolve_secret_version(
    env_value: Option<&str>,
    sec_map: &SecretSecMap,
) -> Result<u32, String> {
    if let Some(v) = env_value {
        return v
            .trim()
            .parse::<u32>()
            .map_err(|_| "VHD_BRIDGE_SECRET_VERSION must be an unsigned 32-bit integer".to_string());
    }
    if let Some(line) = sec_map.vhdmount_key_version.as_deref() {
        return Ok(line.trim().parse::<u32>().unwrap_or(1));
    }
    Ok(1)
}

// ---------------------------------------------------------------------------
// Build_Prereq_Vars resolver — HBBS_KEY / HBBS_HOST / HBBR_HOST gate
// (covers Requirements 22.1-22.10).
//
// Runs unconditionally in `build.rs` (NOT gated by any cargo feature) so the
// three deployment forms (Controlled / Controller / Relay) share the same
// default rendezvous / relay configuration.  The resolver itself is a pure
// `Result<BuildPrereqValues, String>`; the build script wraps it with the
// activation policy ("only fail when ops intends to inject — i.e. any env
// is set OR secret.sec is present").
// ---------------------------------------------------------------------------

/// Successfully resolved Build_Prereq_Vars.  All three values are non-empty
/// and have passed shape validation.  `hbbs_key_b64_canonical` is a freshly
/// re-encoded standard padded base64 string of the 32 decoded bytes — this
/// is what ends up in the `RS_PUB_KEY` slot, so callers don't have to worry
/// about whether the operator typed unpadded base64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildPrereqValues {
    pub hbbs_key_bytes: [u8; 32],
    pub hbbs_key_b64_canonical: String,
    pub hbbs_host: String,
    pub hbbr_host: String,
}

/// Inputs to the Build_Prereq_Vars resolver.
///
/// Each `*_env` is `Some` iff the corresponding environment variable is set
/// (the build script wraps `std::env::var(...)` in `.ok()`).  `sec_map` is
/// the parsed view of `secret.sec`; an empty default-constructed map means
/// "file does not exist or has no recognized lines".
pub struct BuildPrereqInputs<'a> {
    pub hbbs_key_env: Option<&'a str>,
    pub hbbs_host_env: Option<&'a str>,
    pub hbbr_host_env: Option<&'a str>,
    pub sec_map: &'a SecretSecMap,
}

/// Resolve the three Build_Prereq_Vars (HBBS_KEY / HBBS_HOST / HBBR_HOST).
///
/// Per Requirement 22.2, env vars win over `secret.sec` fallback lines.  Per
/// Requirement 22.3, each value is shape-checked:
///   * `HBBS_KEY` SHALL be base64 that decodes to exactly 32 bytes
///   * `HBBS_HOST` SHALL be `host[:port[-port2]]` with ports in `[1, 65535]`
///     and `port1 ≤ port2`
///   * `HBBR_HOST` SHALL be `host[:port]` with port in `[1, 65535]`
///
/// On failure, the returned `Err` lists every offending item: its name
/// (`HBBS_KEY` / `HBBS_HOST` / `HBBR_HOST`), the sources that were checked
/// (env name + `secret.sec` line name), and the expected shape — but never
/// echoes the rejected value.  Callers are responsible for translating this
/// `Err` into a non-zero process exit (Requirement 22.4).
pub fn resolve_build_prereq_vars(
    input: &BuildPrereqInputs<'_>,
) -> Result<BuildPrereqValues, String> {
    let mut errors: Vec<String> = Vec::new();

    // HBBS_KEY ----------------------------------------------------------
    let hbbs_key_pair: Option<([u8; 32], String)> = match resolve_one(
        input.hbbs_key_env,
        input.sec_map.hbbs_key.as_deref(),
    ) {
        Some(raw) => match validate_hbbs_key(raw) {
            Ok(v) => Some(v),
            Err(e) => {
                errors.push(format!(
                    "HBBS_KEY is invalid: {}. Sources checked: env HBBS_KEY, \
                     secret.sec 'HBBS Key' line. Expected shape: standard \
                     base64 that decodes to exactly 32 bytes",
                    e
                ));
                None
            }
        },
        None => {
            errors.push(
                "HBBS_KEY is missing. Sources checked: env HBBS_KEY, \
                 secret.sec 'HBBS Key' line. Expected shape: standard \
                 base64 that decodes to exactly 32 bytes"
                    .to_string(),
            );
            None
        }
    };

    // HBBS_HOST ---------------------------------------------------------
    let hbbs_host: Option<String> = match resolve_one(
        input.hbbs_host_env,
        input.sec_map.hbbs_host.as_deref(),
    ) {
        Some(raw) => match validate_host_with_optional_port_range(raw) {
            Ok(()) => Some(raw.to_string()),
            Err(e) => {
                errors.push(format!(
                    "HBBS_HOST is invalid: {}. Sources checked: env \
                     HBBS_HOST, secret.sec 'HBBS Host' line. Expected \
                     shape: host[:port[-port2]] with ports in [1, 65535] \
                     and port1 <= port2",
                    e
                ));
                None
            }
        },
        None => {
            errors.push(
                "HBBS_HOST is missing. Sources checked: env HBBS_HOST, \
                 secret.sec 'HBBS Host' line. Expected shape: \
                 host[:port[-port2]] with ports in [1, 65535] and \
                 port1 <= port2"
                    .to_string(),
            );
            None
        }
    };

    // HBBR_HOST ---------------------------------------------------------
    let hbbr_host: Option<String> = match resolve_one(
        input.hbbr_host_env,
        input.sec_map.hbbr_host.as_deref(),
    ) {
        Some(raw) => match validate_host_with_optional_port(raw) {
            Ok(()) => Some(raw.to_string()),
            Err(e) => {
                errors.push(format!(
                    "HBBR_HOST is invalid: {}. Sources checked: env \
                     HBBR_HOST, secret.sec 'HBBR Host' line. Expected \
                     shape: host[:port] with port in [1, 65535]",
                    e
                ));
                None
            }
        },
        None => {
            errors.push(
                "HBBR_HOST is missing. Sources checked: env HBBR_HOST, \
                 secret.sec 'HBBR Host' line. Expected shape: \
                 host[:port] with port in [1, 65535]"
                    .to_string(),
            );
            None
        }
    };

    match (hbbs_key_pair, hbbs_host, hbbr_host) {
        (Some((hbbs_key_bytes, hbbs_key_b64_canonical)), Some(hbbs_host), Some(hbbr_host))
            if errors.is_empty() =>
        {
            Ok(BuildPrereqValues {
                hbbs_key_bytes,
                hbbs_key_b64_canonical,
                hbbs_host,
                hbbr_host,
            })
        }
        _ => Err(errors.join("; ")),
    }
}

/// env > secret.sec fallback, with both sources trimmed and treated as
/// "absent" when empty after trim.
fn resolve_one<'a>(env_val: Option<&'a str>, sec_line: Option<&'a str>) -> Option<&'a str> {
    if let Some(v) = env_val {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t);
        }
    }
    if let Some(v) = sec_line {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t);
        }
    }
    None
}

fn validate_hbbs_key(raw: &str) -> Result<([u8; 32], String), String> {
    let bytes = decode_b64_secret(raw, "HBBS_KEY")?;
    if bytes.len() != 32 {
        return Err("base64 decoded length is not 32 bytes".to_string());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    let canonical = encode_b64_padded(&arr);
    Ok((arr, canonical))
}

/// Encode 32 bytes as standard padded base64 (44 chars). Self-contained so
/// this crate stays dependency-free and matches Requirement 22.6's contract
/// of writing the canonical form into `RS_PUB_KEY`.
fn encode_b64_padded(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let b0 = input[i] as u32;
        let b1 = input[i + 1] as u32;
        let b2 = input[i + 2] as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        out.push(ALPHABET[(triple & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let b0 = input[i] as u32;
        let triple = b0 << 16;
        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b0 = input[i] as u32;
        let b1 = input[i + 1] as u32;
        let triple = (b0 << 16) | (b1 << 8);
        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

/// Validate `host[:port[-port2]]`.
///
/// Empty string / leading colon ⇒ Err. The host part itself is not deeply
/// validated (operators may use IP literals, hostnames, or even bracketed
/// IPv6 forms supported elsewhere in RustDesk); we only enforce the
/// *port* contract here, which is what Requirement 22.3 names explicitly.
pub fn validate_host_with_optional_port_range(s: &str) -> Result<(), String> {
    if s.trim().is_empty() {
        return Err("value is empty".to_string());
    }
    // Find the rightmost ':' separating host from port spec, while ignoring
    // colons inside `[...]` IPv6 literals.
    let port_spec = match split_off_port_spec(s) {
        Some((host, port_spec)) => {
            if host.is_empty() {
                return Err("host part is empty".to_string());
            }
            port_spec
        }
        None => return Ok(()), // host with no port, accepted
    };
    // port_spec is "p" or "p1-p2".
    if let Some((p1, p2)) = port_spec.split_once('-') {
        let p1: u32 = p1
            .parse()
            .map_err(|_| format!("port '{}' is not a number", p1))?;
        let p2: u32 = p2
            .parse()
            .map_err(|_| format!("port '{}' is not a number", p2))?;
        validate_port_in_range(p1)?;
        validate_port_in_range(p2)?;
        if p1 > p2 {
            return Err("port range port1 > port2".to_string());
        }
        Ok(())
    } else {
        let p: u32 = port_spec
            .parse()
            .map_err(|_| format!("port '{}' is not a number", port_spec))?;
        validate_port_in_range(p)
    }
}

/// Validate `host[:port]`.  Only one port is accepted (no `-port2` form).
pub fn validate_host_with_optional_port(s: &str) -> Result<(), String> {
    if s.trim().is_empty() {
        return Err("value is empty".to_string());
    }
    match split_off_port_spec(s) {
        Some((host, port_spec)) => {
            if host.is_empty() {
                return Err("host part is empty".to_string());
            }
            if port_spec.contains('-') {
                return Err("port range is not allowed in HBBR_HOST".to_string());
            }
            let p: u32 = port_spec
                .parse()
                .map_err(|_| format!("port '{}' is not a number", port_spec))?;
            validate_port_in_range(p)
        }
        None => Ok(()),
    }
}

fn validate_port_in_range(p: u32) -> Result<(), String> {
    if (1..=65535).contains(&p) {
        Ok(())
    } else {
        Err(format!("port {} is out of [1, 65535]", p))
    }
}

/// Split a host[:port[...]] form into (host, port_spec) at the *last* colon
/// that is not inside `[...]` IPv6 brackets.  Returns `None` when there is
/// no port at all.
fn split_off_port_spec(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut last_colon: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b':' if depth == 0 => last_colon = Some(i),
            _ => {}
        }
    }
    last_colon.map(|i| (&s[..i], &s[i + 1..]))
}
