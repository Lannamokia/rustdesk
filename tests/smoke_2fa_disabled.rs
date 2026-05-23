//! vhd-machine-auth-bridge §18.4 — SMOKE: 2FA / Trusted Devices are inert
//! under `controlled-only` / `vhd-bridge` builds.
//!
//! Validates: Requirements 21.2, 21.4, 21.5, 21.6.
//!
//! ## Why an in-process integration test, not a PowerShell IPC script
//!
//! The user-visible IPC behavior at the keys this task asks about,
//!
//!     Data::Config(("2fa",              None))      // read 2FA TOTP
//!     Data::Config(("2fa",              Some(_)))   // write 2FA TOTP
//!     Data::Config(("enable-trusted-devices", Some(_))) // toggle TD
//!     Data::Config(("trusted-devices",   None))     // read TD list
//!
//! is wired straight to `hbb_common::config::Config::{get_option,
//! set_option, get_trusted_devices, get_trusted_devices_json,
//! add_trusted_device}` — see `src/ipc.rs` lines 827..904 for the
//! read/write arms and `libs/hbb_common/src/config.rs` for the
//! controlled-only / vhd-bridge guards installed by tasks 18.1, 18.2,
//! 18.3. Driving those entry-points directly here gives CI a single
//! place to assert the inert behavior without bringing up
//! `RustDesk_Controlled` and a named pipe — and proves that the IPC
//! "2fa" / "trusted-devices" wire-keys cannot leak a value through any
//! source-of-truth path. A real-pipe SMOKE script (option 1 / 2 in the
//! task) would only re-test the wire framing; the source of truth
//! lives here.
//!
//! ## How to run
//!
//! Windows MSVC matrix used by CI (task 21.1):
//!
//!     cargo test --test smoke_2fa_disabled \
//!         --features vhd-bridge,controlled-only \
//!         --target x86_64-pc-windows-msvc
//!
//! For local cross-platform runs (no Windows-only bits in this file):
//!
//!     cargo test --test smoke_2fa_disabled \
//!         --features vhd-bridge,controlled-only
//!
//! The whole file is `cfg`-gated: `cargo test` without either feature
//! compiles to an empty crate and exits clean, by design.

#![cfg(any(feature = "controlled-only", feature = "vhd-bridge"))]

use hbb_common::bytes::Bytes;
use hbb_common::config::{keys, Config, TrustedDevice};

/// §18.1 / §21.1, §21.2 surrogate — the in-process source feeding
/// `crate::auth_2fa::get_2fa(None)`. The cropped build's `get_2fa`
/// short-circuits to `None`; the orthogonal guarantee this SMOKE pins
/// is that the underlying `Config::get_option("2fa")` slot is empty,
/// matching the IPC `Data::Config(("2fa", None))` read which also has
/// no arm in `src/ipc.rs` and falls through to `value = None`.
#[test]
fn read_back_2fa_option_is_empty() {
    assert_eq!(
        Config::get_option("2fa"),
        "",
        "controlled-only/vhd-bridge: `Config::get_option(\"2fa\")` must \
         be the constant empty string so `auth_2fa::get_2fa` and the \
         IPC `2fa` read arm both observe \"no TOTP secret\""
    );
}

/// §18.2 / §21.3 — write to `"2fa"` via the single `set_option`
/// entry-point used by IPC `Data::Config(("2fa", Some(_)))`, CLI flags,
/// `HARD_SETTINGS` and config-sync pushes. The §18.2 guard SHALL drop
/// the value before it reaches `CONFIG2.options`; the read-back stays
/// empty.
#[test]
fn write_2fa_via_set_option_is_refused() {
    Config::set_option("2fa".to_owned(), "JBSWY3DPEHPK3PXP".to_owned());
    assert_eq!(
        Config::get_option("2fa"),
        "",
        "controlled-only/vhd-bridge: a write to \"2fa\" must be dropped \
         at the `set_option` entry-point so the IPC \"2fa\" key cannot \
         be re-enabled behind `get_2fa`'s `None` collapse"
    );
}

/// §18.2 / §21.3 — same guard for `OPTION_ENABLE_TRUSTED_DEVICES`. The
/// IPC `Data::Config((OPTION_ENABLE_TRUSTED_DEVICES, Some(_)))` write
/// path falls through `set_option`'s unmatched `else`, but a future
/// PR adding a write arm would still be checked here because the
/// guard is at `set_option` itself.
#[test]
fn write_enable_trusted_devices_via_set_option_is_refused() {
    Config::set_option(
        keys::OPTION_ENABLE_TRUSTED_DEVICES.to_owned(),
        "Y".to_owned(),
    );
    assert_eq!(
        Config::get_option(keys::OPTION_ENABLE_TRUSTED_DEVICES),
        "",
        "controlled-only/vhd-bridge: `{}` write must be dropped",
        keys::OPTION_ENABLE_TRUSTED_DEVICES
    );
}

/// §18.3 / §21.5 — IPC `Data::Config(("trusted-devices", None))` is
/// wired to `Config::get_trusted_devices_json()` (see `src/ipc.rs` at
/// the `name == "trusted-devices"` arm). Pin the literal `"[]"` so the
/// IPC reply is a stable empty JSON array irrespective of any
/// in-memory `TRUSTED_DEVICES` cache state.
#[test]
fn ipc_trusted_devices_read_is_empty_json_array() {
    assert_eq!(
        Config::get_trusted_devices_json(),
        "[]",
        "controlled-only/vhd-bridge: IPC `trusted-devices` read must \
         deterministically yield \"[]\""
    );
    assert!(
        Config::get_trusted_devices().is_empty(),
        "controlled-only/vhd-bridge: in-process trusted-devices view \
         must be empty"
    );
}

/// §18.3 / §21.5 — `add_trusted_device` is reached from the
/// `Auth2FA(tfa)` accept path in `Connection::handle_login_request`.
/// That path is unreachable in this build form because §18.1 keeps
/// `require_2fa` permanently `None`, but we still assert the no-op so
/// any future caller that finds its way here can not seed an hwid
/// trusted-device cache.
#[test]
fn add_trusted_device_is_a_noop() {
    let device = TrustedDevice {
        hwid: Bytes::from_static(b"\x01\x02\x03"),
        time: hbb_common::get_time(),
        id: "smoke-id".to_owned(),
        name: "smoke-name".to_owned(),
        platform: "smoke-platform".to_owned(),
    };
    Config::add_trusted_device(device);

    assert_eq!(Config::get_trusted_devices_json(), "[]");
    assert!(Config::get_trusted_devices().is_empty());
}

/// §21.6 — given a `LoginRequest` carrying `lr.tfa.code = "123456"`,
/// the cropped build SHALL fall back to "password + §19 approval"
/// without entering any 2FA branch. We can't drive `Connection` from a
/// pure integration test (no server runtime), but the invariant the
/// §21.6 decision rests on is mechanically observable: the persisted
/// TOTP slot stays empty even after a write attempt that would
/// normally seed `get_2fa(None)`'s `unwrap_or` argument. With no
/// secret to validate against, `auth_2fa::get_2fa` could not return
/// `Some(TOTP)` even if §18.1's stub were ever reverted — defense in
/// depth.
#[test]
fn no_persisted_totp_secret_so_tfa_code_field_is_inert() {
    Config::set_option("2fa".to_owned(), "JBSWY3DPEHPK3PXP".to_owned());
    let persisted = Config::get_option("2fa");
    assert!(
        persisted.is_empty(),
        "controlled-only/vhd-bridge: a persisted 2FA secret would \
         re-enable `tfa.code` validation via `auth_2fa::get_2fa`. \
         Got `{}`, expected empty.",
        persisted
    );
}
