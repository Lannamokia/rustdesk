//! vhd-machine-auth-bridge §21.3 — SMOKE: feature-off parity.
//!
//! Validates: Requirements 1.7, 13.1.
//!
//! ## What this SMOKE pins
//!
//! When **both** `vhd-bridge` and `controlled-only` features are off,
//! the resulting binary is what ships as `RustDesk_Controller` /
//! `RustDesk_RelayServer` (and the research-only "controlled without
//! bridge" flavor that Requirement 1.7 explicitly carves out for
//! engineering rollback). The contract this file defends is that the
//! `vhd_bridge::*` public surface is purely no-op stubs, and the
//! cropped 2FA / trusted-devices guards installed by tasks 18.1 / 18.2
//! / 18.3 are **not** active — i.e. the build is byte-compatible with
//! a default upstream RustDesk build at the public-API level.
//!
//! Per design.md `tests/smoke/feature_off_parity.rs`, this is a SMOKE
//! test (Requirement 1.7's "启动行为与未集成桥接时一致"), not a
//! property test — input space is enumerable and reads/writes against
//! a static `Config` are deterministic.
//!
//! ## How to run
//!
//! Default features, no bridge:
//!
//!     cargo test --test feature_off_parity --target x86_64-pc-windows-msvc
//!
//! Cross-platform (no Windows-only items in this file):
//!
//!     cargo test --test feature_off_parity
//!
//! The whole file is gated `cfg(not(any(feature = "vhd-bridge",
//! feature = "controlled-only")))`. Building with either feature on
//! compiles to an empty integration-test crate and exits clean — by
//! design, because the parity contract this file describes only
//! applies to the feature-off binary flavor.

#![cfg(not(any(feature = "vhd-bridge", feature = "controlled-only")))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use hbb_common::message_proto::LoginRequest;
use librustdesk::vhd_bridge::{
    self, peer_approval, ApprovalOutcome, BridgeState, BridgeStateSnapshot, ConnectionType,
};

// ---------------------------------------------------------------------------
// vhd_bridge public-API surface — no-op stubs only
// ---------------------------------------------------------------------------

/// §21.3 — `vhd_bridge::current_state()` MUST return the constant
/// `Disabled` snapshot (Requirement 4.6: `Disabled` is the only state
/// visible when the feature is off). The IPC `vhd-bridge-state`
/// observability key (task 13.2) consumes this directly, so the
/// Controller / Relay flavors broadcast a stable shape with no
/// runtime plumbing.
#[test]
fn current_state_is_constant_disabled_snapshot() {
    let snap = vhd_bridge::current_state();
    let expected = BridgeStateSnapshot::disabled();

    assert_eq!(snap.state, BridgeState::Disabled);
    assert_eq!(snap.state, expected.state);
    assert!(snap.last_reason.is_none());
    assert_eq!(snap.secret_version, 0);
    assert_eq!(snap.log_drop_count, 0);
    assert_eq!(snap.last_change_at_ms, 0);
    assert!(snap.error_code.is_none());
}

/// §21.3 — `active_session_count()` is the source of truth for the
/// `Maintenance_Overlay` IPC key. With the bridge compiled out, the
/// feature-off stub returns `0` unconditionally (mod.rs line 295).
/// No counter to bump, no observability::inc/dec to call.
#[test]
fn active_session_count_is_zero() {
    assert_eq!(vhd_bridge::active_session_count(), 0);
}

/// §21.3 — `vhd_bridge::start(rt)` is a no-op in the feature-off
/// build (mod.rs line 178). The signature still requires a Tokio
/// `Handle` so call-sites in `core_main` / `start_server` do not need
/// `cfg(...)` of their own; we provide one via `#[tokio::test]`.
#[tokio::test]
async fn start_is_a_noop() {
    let handle = tokio::runtime::Handle::current();
    vhd_bridge::start(&handle);
    // Idempotent — second call is the same no-op.
    vhd_bridge::start(&handle);

    // After "starting", state stays Disabled. There is no worker to
    // spawn, no hook to register, no `STARTED` OnceLock to flip.
    assert_eq!(vhd_bridge::current_state().state, BridgeState::Disabled);
}

/// §21.3 — `vhd_bridge::install_log_sink()` is a no-op (mod.rs line
/// 258). Logically: there is no `log_sink` submodule compiled in, so
/// the global `log` crate sink is whatever the host process set up
/// (typically `env_logger` or RustDesk's own logger), unmodified.
#[test]
fn install_log_sink_is_a_noop() {
    vhd_bridge::install_log_sink();
    // Idempotent.
    vhd_bridge::install_log_sink();
}

/// §21.3 — `vhd_bridge::reset()` is a no-op (mod.rs line 243). No
/// peer-approval cache to clear, no `RESET_SIGNAL` to pulse. State
/// remains `Disabled` after reset, exactly as it was before.
#[test]
fn reset_is_a_noop() {
    vhd_bridge::reset();
    assert_eq!(vhd_bridge::current_state().state, BridgeState::Disabled);
    // Idempotent.
    vhd_bridge::reset();
    assert_eq!(vhd_bridge::current_state().state, BridgeState::Disabled);
}

/// §21.3 — `peer_approval::gate(...)` returns `BridgeUnavailable`,
/// the fail-open value the Requirement-19 inbound-connection decision
/// table treats as "accept" (Requirement 19.6, design.md §"Property
/// 14"). This guarantees that a Controller / Relay binary, which by
/// Requirement 1.6 never accepts inbound remote-control sessions
/// anyway, still presents the same gate signature to any code path
/// that links it — call-sites in `src/server/connection.rs` need no
/// `cfg(...)` plumbing.
#[tokio::test]
async fn peer_approval_gate_returns_bridge_unavailable() {
    let lr = LoginRequest::new();
    let peer_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 21118);

    for ct in [
        ConnectionType::Controlled,
        ConnectionType::ViewOnly,
        ConnectionType::FileTransfer,
        ConnectionType::PortForward,
        ConnectionType::Terminal,
    ] {
        assert_eq!(
            peer_approval::gate(&lr, peer_addr, ct).await,
            ApprovalOutcome::BridgeUnavailable,
            "feature-off `peer_approval::gate` MUST always return \
             BridgeUnavailable (fail-open) for connection_type = {:?}",
            ct,
        );
    }
}

// ---------------------------------------------------------------------------
// Cropped 2FA / trusted-devices guards are NOT active
//
// Tasks 18.1 / 18.2 / 18.3 install guards at `auth_2fa::get_2fa`,
// `Config::set_option`, `Config::get_trusted_devices` etc. that
// short-circuit to `None` / refuse-to-write / empty-Vec under the
// `controlled-only` or `vhd-bridge` features. The parity contract for
// §21.3 is the **converse**: with neither feature set, those guards
// MUST NOT trigger. We can't reach `auth_2fa::get_2fa` from an
// integration test (the module is `mod auth_2fa;` — private), so we
// pin the orthogonal observable: the `Config::set_option` /
// `Config::get_option` round-trip for the `"2fa"` key is allowed.
// ---------------------------------------------------------------------------

/// §21.3 / Requirement 13.1 — `Config::set_option("2fa", _)` MUST
/// persist the value in the feature-off build, mirroring the upstream
/// RustDesk behavior. The cropped guard at `config.rs` line 1305
/// (`is_2fa_disabled_option_key` short-circuit) lives behind
/// `cfg(any(feature = "controlled-only", feature = "vhd-bridge"))` and
/// therefore SHALL NOT be compiled here. We round-trip a sentinel
/// value and restore the prior config slot so the test is idempotent
/// against the developer's persisted RustDesk options.
#[test]
fn config_set_option_2fa_round_trips() {
    use hbb_common::config::Config;

    // Snapshot prior state so we can restore it on exit.
    let prior = Config::get_option("2fa");

    // Sentinel that does not collide with any real TOTP secret format.
    const SENTINEL: &str = "feature-off-parity-21.3-sentinel";
    Config::set_option("2fa".to_owned(), SENTINEL.to_owned());

    let read_back = Config::get_option("2fa");

    // Restore before asserting so a panic-on-fail still cleans up.
    Config::set_option("2fa".to_owned(), prior.clone());

    assert_eq!(
        read_back, SENTINEL,
        "feature-off build: writes to the `\"2fa\"` config slot MUST \
         persist (the cropped `is_2fa_disabled_option_key` guard is \
         not compiled in). Got `{}`, expected `{}`.",
        read_back, SENTINEL,
    );

    // Sanity: post-restore read returns the prior value.
    assert_eq!(Config::get_option("2fa"), prior);
}

/// §21.3 / Requirement 13.1 — `Config::get_trusted_devices()` MUST
/// hit the real read path in the feature-off build (`config.rs` line
/// 1631..1641), not the `Vec::new()` short-circuit at line 1625. We
/// don't make claims about the *contents* of the developer's trusted-
/// devices list (it could legitimately be empty), only that the call
/// site is the upstream one. The orthogonal observable is the
/// `add_trusted_device` round-trip: under `controlled-only` /
/// `vhd-bridge` it's a `let _ = device; return;` no-op; under default
/// features the new device joins the in-memory cache. We add and
/// remove a sentinel device, asserting the cache reflects the writes.
#[test]
fn config_trusted_devices_writes_round_trip() {
    use hbb_common::bytes::Bytes;
    use hbb_common::config::{Config, TrustedDevice};

    let sentinel_hwid = Bytes::from_static(b"\xfe\xed\xfa\xce-21.3");
    // Make sure we start clean with respect to our sentinel.
    Config::remove_trusted_devices(&vec![sentinel_hwid.clone()]);

    let device = TrustedDevice {
        hwid: sentinel_hwid.clone(),
        time: hbb_common::get_time(),
        id: "feature-off-parity-21.3".to_owned(),
        name: "feature-off-parity-21.3".to_owned(),
        platform: "feature-off-parity-21.3".to_owned(),
    };
    Config::add_trusted_device(device);

    let after_add = Config::get_trusted_devices();
    let found = after_add.iter().any(|d| d.hwid == sentinel_hwid);

    // Restore before asserting so a panic-on-fail still cleans up.
    Config::remove_trusted_devices(&vec![sentinel_hwid.clone()]);

    assert!(
        found,
        "feature-off build: `Config::add_trusted_device` MUST reach \
         the upstream write path (the `controlled-only` / `vhd-bridge` \
         no-op branch is not compiled in). The sentinel hwid was not \
         visible via `get_trusted_devices` after `add_trusted_device`."
    );

    let post_restore = Config::get_trusted_devices();
    assert!(
        post_restore.iter().all(|d| d.hwid != sentinel_hwid),
        "test cleanup: sentinel hwid still present after \
         `remove_trusted_devices`",
    );
}
