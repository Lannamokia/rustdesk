//! vhd_bridge: machine-auth bridge between RustDesk_Controlled and the
//! VHDMount sidecar over a Windows named pipe.
//!
//! This module is feature-gated on `cfg(all(target_os = "windows",
//! feature = "vhd-bridge"))`. On any other platform / feature combination,
//! the public API resolves to no-op stubs so that callers can invoke
//! `vhd_bridge::*` without scattering `#[cfg(...)]` at every call site.
//!
//! Task 3.1 lays down the module skeleton only. Subsequent tasks fill in
//! the real bodies:
//!   * 4.x  — secret access, HMAC inputs, constant-time compare
//!   * 5.x  — frame codec / wire JSON
//!   * 6.x  — pipe + peer-process verification
//!   * 7.x  — `BridgeWorker` state machine, reconnect, nonce window
//!   * 8.x  — triggers, coalescing, heartbeat, dedup
//!   * 10.x — log sink
//!   * 11.x — peer-approval gate
//!   * 13.x — `current_state()` watch + `reset()` signal

// Real submodules: only compiled on Windows with the bridge feature on.
// They start as empty stubs (`//!` doc + TODO) and get filled in by the
// tasks listed above.
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod config;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod frame;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod hmac;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod log_sink;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod observability;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod pipe;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod protocol;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod secret;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
mod worker;

#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
pub mod peer_approval;
#[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
pub mod triggers;

// ---------------------------------------------------------------------------
// Public data types
//
// Defined unconditionally so external callers (IPC observability, server
// connection path, login error map, ...) can name them in either build
// flavor. Task 3.2 chose the spec's "或 `mod.rs`" placement: a single
// declaration here is visible to both the feature-on submodules and the
// feature-off fallback, avoiding two divergent copies that would have
// to stay byte-compatible for IPC serialization. The feature-on
// `observability` submodule re-exports these types for ergonomics.
// ---------------------------------------------------------------------------

/// Bridge runtime state. `Disabled` is the only state visible on
/// non-Windows targets or when the `vhd-bridge` feature is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde_derive::Serialize, serde_derive::Deserialize)]
pub enum BridgeState {
    Disabled,
    Initializing,
    Connected,
    Authorized,
    Denied,
    Failed,
}

/// Snapshot of bridge state suitable for the `vhd-bridge-state` IPC
/// observability key. See design.md §"Components and Interfaces".
///
/// `last_reason` and `error_code` are owned `String`s rather than
/// `&'static str` so this struct is `Deserialize` (Task 13.2 wires it
/// into the IPC `Data` enum, whose `serde_json::from_str` round-trip
/// can't reborrow static lifetimes). The `observability` writers still
/// receive `&'static str` constants and convert at the boundary.
#[derive(Debug, Clone, serde_derive::Serialize, serde_derive::Deserialize)]
pub struct BridgeStateSnapshot {
    pub state: BridgeState,
    pub last_reason: Option<String>,
    pub secret_version: u32,
    pub log_drop_count: u64,
    pub last_change_at_ms: u64,
    pub error_code: Option<String>,
}

impl BridgeStateSnapshot {
    /// The snapshot returned when the bridge is compiled out or the
    /// worker has not been started yet. Not `const fn` because
    /// `Option<String>` allocates only when `Some(...)`, but the type
    /// itself isn't `const`-constructible.
    pub fn disabled() -> Self {
        Self {
            state: BridgeState::Disabled,
            last_reason: None,
            secret_version: 0,
            log_drop_count: 0,
            last_change_at_ms: 0,
            error_code: None,
        }
    }
}

/// Outcome of `peer_approval::gate(...)`. `BridgeUnavailable` is the
/// fail-open value used when the bridge cannot reach VHDMount or is
/// compiled out entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved,
    Rejected,
    BridgeUnavailable,
}

/// Connection-type tag passed into `peer_approval::gate(...)`. Mirrors the
/// `connectionType` field of the `Peer_Approval_Request` JSON frame
/// (Requirement 19.3) but lives on the public bridge API so callers in
/// `src/server/connection.rs` can name it without depending on the
/// internal `protocol` submodule. Defined unconditionally so the
/// feature-off fallback `peer_approval::gate(...)` keeps a stable
/// signature with the feature-on path (task 11.4 wires the call site).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Controlled,
    ViewOnly,
    FileTransfer,
    PortForward,
    Terminal,
}

// ---------------------------------------------------------------------------
// Top-level public API
//
// All four functions are unconditional no-ops in task 3.1. Tasks 7.x /
// 10.x / 13.x replace their bodies with delegations into `worker`,
// `observability`, and `log_sink`. The signatures stay stable so that
// callers in `core_main` / `ipc.rs` can be wired up without further
// `cfg!` plumbing.
// ---------------------------------------------------------------------------

/// Start the bridge worker on the given Tokio runtime handle.
///
/// Wiring done here, in order:
///
///   1. Register the `Config::set_id` and
///      `password_security::update_temporary_password` observation
///      hooks installed by tasks 9.1 / 9.2 with the corresponding
///      `triggers::notify_*` entry points. Done **before** the
///      worker is spawned so that any cred-write that races the
///      first scheduler tick still feeds into the trigger queue
///      (the `triggers::notify_*` callbacks themselves tolerate
///      being invoked before `triggers::init()` has run — they
///      warn-drop). `notify_rotation` is wired at its sole
///      call-site (`server::connection::check_update_temporary_password`)
///      directly via `cfg(all(target_os = "windows", feature =
///      "vhd-bridge"))`, so no separate hook registration is needed
///      here.
///   2. Spawn `BridgeWorker::run` on `rt` via [`worker::start`].
///      `triggers::init` (the heartbeat ticker + the bounded mpsc
///      receiver) runs lazily inside `worker::run` on the spawned
///      task.
///
/// Idempotency: a process-lifetime `OnceLock` short-circuits every
/// invocation past the first, so the recursive / re-entrant
/// `start_server` paths in `src/server.rs` do not spawn duplicate
/// workers. The `OnceLock::set(...)` on the hbb_common hooks is
/// itself idempotent — a second call has no effect — but
/// `worker::start` would otherwise `tokio::spawn` a second
/// `BridgeWorker::run` task that would race the first on the same
/// pipe.
///
/// Task 14.1 wires this entry point into the controlled-server
/// process's `start_server` async fn, immediately after `#[tokio::main]`
/// opens the runtime. SHALL NOT create a nested runtime; the caller
/// passes a `Handle` borrowed from the ambient runtime
/// (Requirement 13.3 / AGENTS.md "Tokio Rules").
pub fn start(_rt: &hbb_common::tokio::runtime::Handle) {
    #[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
    {
        use std::sync::OnceLock;
        static STARTED: OnceLock<()> = OnceLock::new();
        if STARTED.set(()).is_err() {
            // Already started: the recursive `start_server` calls in
            // `src/server.rs` (e.g. `--server` retry path) reach this
            // entry point on every iteration. Subsequent invocations
            // are silent no-ops by design.
            return;
        }

        hbb_common::config::set_id_hook(triggers::notify_id_change);
        hbb_common::password_security::password_change_hook(
            triggers::notify_password_change,
        );

        worker::start(_rt);
    }
}

/// Return a snapshot of the current bridge state.
///
/// Feature-on builds delegate to the `tokio::sync::watch` channel
/// owned by `observability` (task 13.1). Feature-off / non-Windows
/// builds always return `BridgeStateSnapshot::disabled()` so the
/// `vhd-bridge-state` IPC key (task 13.2) and external callers see a
/// stable, well-typed value with no plumbing.
pub fn current_state() -> BridgeStateSnapshot {
    #[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
    {
        observability::current_snapshot()
    }
    #[cfg(not(all(target_os = "windows", feature = "vhd-bridge")))]
    {
        BridgeStateSnapshot::disabled()
    }
}

/// Reset bridge runtime state, clearing the report-dedup snapshot and
/// the approval cache, and asking the worker to close its current
/// session.
///
/// On feature-off / non-Windows builds this is a no-op — there is no
/// worker to signal and no cache to drop, and the constant `Disabled`
/// snapshot returned by [`current_state`] already reflects "no
/// runtime state to clear".
///
/// On Windows with `vhd-bridge` enabled, the call:
///   1. Drops the in-memory peer-approval cache via
///      [`peer_approval::clear_cache`] so a previously-`Approved`
///      controller has to be re-asked after the reset
///      (Requirement 19.7).
///   2. Pulses the worker's `RESET_SIGNAL` via
///      [`worker::request_reset`], which (a) escapes the sticky
///      `Failed` sink-state by forcing the snapshot back to
///      `Initializing`, and (b) wakes the worker's `select!` arm so
///      it tears down the current pipe session and re-enters the
///      connect loop.
///
/// The `Last_Reported_Snapshot` storage is owned by task 8.2 and not
/// yet present; once it lands, `worker::request_reset` will clear it
/// alongside the reset signal so `vhd_bridge::reset()` continues to
/// be the single user-visible entry point.
pub fn reset() {
    #[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
    {
        peer_approval::clear_cache();
        worker::request_reset();
    }
}
/// Install the bridge log sink as the global `log` crate sink.
///
/// Idempotent: [`log_sink::install`] guards itself with a `OnceLock`
/// on its bounded ring buffer so a second call from a re-entrant
/// `start_server` path is a silent no-op. Must be called from inside
/// an active Tokio runtime (it spawns the writer-drain task); task
/// 14.1's call-site in `src/server.rs::start_server` honours that
/// invariant by invoking it from the `#[tokio::main]` async body.
pub fn install_log_sink() {
    #[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
    {
        log_sink::install();
    }
}

/// Return the number of currently authorized remote sessions.
///
/// This is the source of truth for the `active-session-count` IPC
/// observability key consumed by the Flutter `Maintenance_Overlay`
/// (design.md §"Maintenance_Overlay" + line 458 / Requirement 15.2,
/// 15.3, 15.6). Held outside [`BridgeStateSnapshot`] on purpose:
/// Property 15 (design.md line 922) freezes that snapshot to exactly
/// six keys, and design.md explicitly drives the overlay off the
/// existing `Data::ControlledSessionCount` IPC variant rather than a
/// new bridge-state field.
///
/// Increments fire from [`Connection::on_remote_authorized`] only
/// after the §11.4 peer-approval gate returns `Approved` or
/// `BridgeUnavailable`; rejected connections never reach
/// `on_remote_authorized` and therefore cannot bump the counter
/// (Requirement 15.3). Decrements fire from a per-`Connection` RAII
/// guard's `Drop`, covering both normal teardown and abnormal close.
///
/// On feature-off / non-Windows builds this is a no-op stub returning
/// `0`, so the IPC `Data::ControlledSessionCount` handler can ask for
/// the value without scattering `#[cfg(...)]` of its own.
pub fn active_session_count() -> usize {
    #[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
    {
        observability::active_session_count()
    }
    #[cfg(not(all(target_os = "windows", feature = "vhd-bridge")))]
    {
        0
    }
}

/// RAII guard for the active remote-session counter (Task 15.7).
///
/// `inc_active_session_count()` runs in [`arm_remote_session`]; the
/// matching `dec_active_session_count()` runs in `Drop`. Holding the
/// guard on `Connection` rather than calling the decrement explicitly
/// in a teardown function ensures both normal and abnormal close
/// paths (panics, early returns, dropped futures) decrement exactly
/// once.
///
/// Defined in this module so the feature-off fallback is a zero-sized
/// no-op type and call-sites in `src/server/connection.rs` can name
/// it without scattering `cfg!`. Construct via [`arm_remote_session`].
#[must_use = "drop the RemoteSessionGuard to decrement the active-session-count"]
pub struct RemoteSessionGuard {
    // Tracks whether `Drop` should decrement. Always `true` on the
    // feature-on Windows build (constructed only by
    // `arm_remote_session`); always `false` on the feature-off
    // fallback so `Drop` is a no-op and the field carries no runtime
    // cost beyond a single byte. Kept as a real field (rather than a
    // `cfg`-gated one) so the type's layout stays identical across
    // feature flavors and `Connection`'s struct does not gain a
    // feature-conditional size delta.
    armed: bool,
}

impl Drop for RemoteSessionGuard {
    fn drop(&mut self) {
        #[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
        if self.armed {
            observability::dec_active_session_count();
        }
        // On the feature-off fallback `armed` is always `false` and
        // there is no counter to touch — `Drop` is a no-op.
        #[cfg(not(all(target_os = "windows", feature = "vhd-bridge")))]
        let _ = self.armed;
    }
}

/// Increment the active remote-session counter and return a guard
/// that decrements it on drop. Call exactly once per authorized
/// session, from [`Connection::on_remote_authorized`].
///
/// Feature-off / non-Windows builds return a no-op guard whose `Drop`
/// is empty, so the call-site in `connection.rs` does not need any
/// `cfg(...)` of its own.
pub fn arm_remote_session() -> RemoteSessionGuard {
    #[cfg(all(target_os = "windows", feature = "vhd-bridge"))]
    {
        observability::inc_active_session_count();
        RemoteSessionGuard { armed: true }
    }
    #[cfg(not(all(target_os = "windows", feature = "vhd-bridge")))]
    {
        RemoteSessionGuard { armed: false }
    }
}

// ---------------------------------------------------------------------------
// Debug-only integration-test hooks (task 22.1)
//
// Cargo's `tests/` integration-test files compile as a separate crate
// against `librustdesk`'s public surface, so they cannot reach
// `pub(super)` items in `pipe.rs` / `worker.rs` / `peer_approval.rs`.
// To keep the production code surface minimal we expose **only two**
// `pub` entry points, both gated by `cfg(debug_assertions)` and the
// usual `cfg(all(target_os = "windows", feature = "vhd-bridge"))`:
//
//   * [`test_set_skip_peer_check`] flips an atomic that
//     `pipe::open_and_verify` reads on every connect attempt. The
//     integration test sets it to `true` once at startup so the
//     worker can complete a real handshake against a mock pipe
//     server hosted by the test binary (whose image basename is
//     never `VHDMount.exe`).
//   * [`test_set_pipe_name`] is a public alias for the
//     `pub(crate)` `try_apply_bridge_option(VHD_BRIDGE_PIPE_NAME, _)`
//     plumbing in `hbb_common::config`. Tests need to point the
//     worker at a per-process pipe path so concurrent `cargo test`
//     runs (and re-runs of the same binary) do not collide on
//     `\\.\pipe\VHDMount.RustDeskBridge`.
//
// The default `[profile.release]` in `Cargo.toml` keeps
// `debug-assertions = false`, so neither hook is compiled into a
// release binary — production cannot disable Requirement 10.5 or
// retarget the pipe name even by accident.
// ---------------------------------------------------------------------------

/// Debug-build-only: flip the peer-image-check skip flag consumed by
/// `pipe::open_and_verify`. See `pipe::set_test_skip_peer_check` for
/// the production-side comment.
#[cfg(all(target_os = "windows", feature = "vhd-bridge", debug_assertions))]
pub fn test_set_skip_peer_check(on: bool) {
    pipe::set_test_skip_peer_check(on);
}

/// Debug-build-only: redirect the bridge worker to `pipe_name` on
/// the **next** reconnect cycle. Implemented by routing through the
/// existing `try_apply_bridge_option` validator so the integration
/// test exercises the same `(name, value)` plumbing the IPC config-
/// sync path uses in production. Empty / control-char-bearing
/// strings fall back to the production default per Requirement 4.4.
#[cfg(all(target_os = "windows", feature = "vhd-bridge", debug_assertions))]
pub fn test_set_pipe_name(pipe_name: &str) {
    hbb_common::config::test_apply_bridge_option(
        hbb_common::config::keys::VHD_BRIDGE_PIPE_NAME,
        pipe_name,
    );
}

// ---------------------------------------------------------------------------
// Feature-off / non-Windows fallbacks for the public submodules.
//
// When the bridge is compiled out, these inline modules supply the
// no-op API surface promised above so that call sites do not need any
// `#[cfg(...)]` of their own.
// ---------------------------------------------------------------------------

#[cfg(not(all(target_os = "windows", feature = "vhd-bridge")))]
pub mod triggers {
    //! No-op stub for the trigger API. The real implementation lives in
    //! `triggers.rs` and is filled in by task 8.1.

    /// No-op when the bridge is compiled out.
    pub fn notify_id_change() {}

    /// No-op when the bridge is compiled out.
    pub fn notify_password_change() {}

    /// No-op when the bridge is compiled out.
    pub fn notify_rotation() {}
}

#[cfg(not(all(target_os = "windows", feature = "vhd-bridge")))]
pub mod peer_approval {
    //! No-op stub for the peer-approval gate. The real implementation
    //! lives in `peer_approval.rs` and is filled in by task 11.1.

    use super::{ApprovalOutcome, ConnectionType};
    use hbb_common::message_proto::LoginRequest;

    /// Always returns `ApprovalOutcome::BridgeUnavailable` when the
    /// bridge is compiled out, which the inbound-connection decision
    /// table at task 11.4 treats as fail-open.
    pub async fn gate(
        _lr: &LoginRequest,
        _peer_addr: std::net::SocketAddr,
        _conn_type: ConnectionType,
    ) -> ApprovalOutcome {
        ApprovalOutcome::BridgeUnavailable
    }
}
