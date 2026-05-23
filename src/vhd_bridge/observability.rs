//! `vhd_bridge::observability` — `BridgeStateSnapshot` watch channel,
//! `REASON_*` constants, `ALLOWED_ERROR_CODES`, and `LOG_DROP` counter.
//!
//! Task 3.2 chose the spec's "或 `mod.rs`" placement for the public
//! data types `BridgeState` / `BridgeStateSnapshot` / `ApprovalOutcome`
//! so that a single declaration is visible to both feature flavors
//! (Windows + `vhd-bridge` and the no-op fallback). This module
//! re-exports them for the convenience of feature-on submodules.
//!
//! Task 3.3 added the `REASON_*` discrete reason-code constants and
//! the `ALLOWED_ERROR_CODES` whitelist (Requirements 12.2, 12.5).
//!
//! Task 13.1 owns the `tokio::sync::watch::Sender<BridgeStateSnapshot>`
//! and centralises every write to it inside three helpers:
//! [`transition_to`], [`record_accepted`], and [`log_drop_count_inc`].
//! `transition_to_failed` exists as a guarded variant of `transition_to`
//! so the "permanent `Failed` is sticky inside one startup cycle"
//! contract (Requirements 9.5 / 9.8) lives next to the `Sender`.
//! Visibility is `pub(super)` because `mod observability;` in `mod.rs`
//! is private; the only crossings are sibling submodules (`worker`,
//! `log_sink`, ...) and the public `current_state()` / `reset()`
//! façades on `mod.rs`.

#![allow(dead_code)] // wired up by tasks 7.2-7.4, 8.x, 10.1, 11.2, 13.x.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use hbb_common::tokio::sync::watch;

#[allow(unused_imports)]
pub(super) use super::{ApprovalOutcome, BridgeState, BridgeStateSnapshot};
use super::secret::SHARED_SECRET_VERSION;

// ---------------------------------------------------------------------------
// Discrete reason codes (Requirement 12.2)
//
// These strings are the *only* values that may appear in the `reason`
// field of a `BridgeStateSnapshot` / `vhd-bridge-state` IPC payload, in
// `Bridge_State` transition log entries, and as the `last_reason`
// argument to `compute_retry_delay` (task 7.3). Free-form / raw error
// text is forbidden — see design.md §"Bridge_State 离散原因码".
// ---------------------------------------------------------------------------

pub(super) const REASON_DENY: &str = "deny";
pub(super) const REASON_RATE_LIMITED: &str = "rate_limited";
pub(super) const REASON_INVALID_PROOF: &str = "invalid_proof";
pub(super) const REASON_INVALID_MAC: &str = "invalid_mac";
pub(super) const REASON_SECRET_OUTDATED: &str = "secret_outdated";
pub(super) const REASON_PIPE_CLOSED: &str = "pipe_closed";
pub(super) const REASON_PIPE_TIMEOUT: &str = "pipe_timeout";
pub(super) const REASON_PEER_NOT_VHDMOUNT: &str = "peer_not_vhdmount";
pub(super) const REASON_VERSION_MISMATCH: &str = "version_mismatch";

// ---------------------------------------------------------------------------
// Stable, locale-independent error codes (Requirement 12.5)
//
// Exposed via the `vhd-bridge-state` IPC key whenever `Bridge_State` is
// `Failed` or `Denied`. UI / monitoring code MAY switch on these strings
// without parsing free-form text; the i18n key mapping lives in
// design.md §"Bridge_State 离散原因码". Property test 15 (task 13.4)
// asserts that every `errorCode` emitted by the snapshot publisher is
// a member of this set.
// ---------------------------------------------------------------------------

pub(super) const ALLOWED_ERROR_CODES: &[&str] = &[
    "vhd.bridge.failed.secret_outdated",
    "vhd.bridge.failed.peer_not_vhdmount",
    "vhd.bridge.failed.version_mismatch",
    "vhd.bridge.denied.deny",
    "vhd.bridge.denied.rate_limited",
    "vhd.bridge.denied.invalid_proof",
    "vhd.bridge.denied.invalid_mac",
];

// ---------------------------------------------------------------------------
// Watch channel (Requirements 12.1, 12.4)
// ---------------------------------------------------------------------------

/// Process-singleton watch channel for the current `BridgeStateSnapshot`.
/// Constructed lazily on the first write, read by
/// `vhd_bridge::current_state()` synchronously via `borrow().clone()`,
/// and consumed by the `vhd-bridge-state` IPC handler (task 13.2).
///
/// Using `OnceLock<watch::Sender>` (instead of `lazy_static` + `Mutex`)
/// gives us:
///   - Lock-free `Receiver::borrow()` reads.
///   - No `Mutex<BridgeState>` anywhere on the read path
///     (Requirement 12.1 last clause: "SHALL NOT 在 `.await` 上持锁").
///   - Single-init guarantee — concurrent `start()` / first
///     `transition_to` calls converge on the same channel.
static STATE_TX: OnceLock<watch::Sender<BridgeStateSnapshot>> = OnceLock::new();
static STATE_RX: OnceLock<watch::Receiver<BridgeStateSnapshot>> = OnceLock::new();

/// Construct the channel and store both halves. Idempotent: callers
/// after the first one observe the existing `Sender`, which is what
/// the worker (task 7.x) and the IPC observability handler (task 13.2)
/// both want.
fn ensure_channel() -> &'static watch::Sender<BridgeStateSnapshot> {
    STATE_TX.get_or_init(|| {
        let initial = initial_snapshot();
        let (tx, rx) = watch::channel(initial);
        // OnceLock::set fails if already set; we use `let _ =` to
        // ignore the second-init case (the first init wins).
        let _ = STATE_RX.set(rx);
        tx
    })
}

/// Construct the snapshot the worker publishes before its first
/// `transition_to`. `state = Initializing` because the public API
/// contract is "feature-on builds always start initialising"; the
/// `Disabled` value is reserved for the no-op fallback in mod.rs
/// (Requirement 4.6 / design §"Bridge_State 取值集合").
fn initial_snapshot() -> BridgeStateSnapshot {
    BridgeStateSnapshot {
        state: BridgeState::Initializing,
        last_reason: None,
        secret_version: SHARED_SECRET_VERSION,
        log_drop_count: 0,
        last_change_at_ms: now_unix_ms(),
        error_code: None,
    }
}

/// Read the current snapshot. Called by `vhd_bridge::current_state()`
/// (mod.rs façade) and the `vhd-bridge-state` IPC handler
/// (task 13.2). Synchronous, lock-free.
pub(super) fn current_snapshot() -> BridgeStateSnapshot {
    if let Some(rx) = STATE_RX.get() {
        rx.borrow().clone()
    } else {
        // Channel not yet initialised (only possible between binary
        // entry and the worker's first publish). Surface the
        // initial-state shape so callers don't see uninitialised data
        // and the `secret_version` field stays truthful.
        initial_snapshot()
    }
}

// ---------------------------------------------------------------------------
// Centralised writers (Requirement 12.1)
//
// Per task 13.1: "把 `tokio::sync::watch::Sender<BridgeStateSnapshot>`
// 的写入集中到 `transition_to`、`record_accepted`、`log_drop_count_inc`
// 等少数路径". Every other module reaches the watch channel only
// through these helpers.
// ---------------------------------------------------------------------------

/// Transition the worker to `new_state`, optionally tagging the
/// reason. Updates `last_change_at_ms` and `error_code`, then
/// publishes via the watch channel.
///
/// Failed is a sink state inside one startup cycle: once
/// `Bridge_State == Failed`, this helper is a no-op so that transient
/// I/O errors arriving after a permanent `secret_outdated` /
/// `peer_not_vhdmount` / `version_mismatch` cannot demote the state
/// back to `Initializing` (Requirements 9.5 / 9.8). The only escape
/// from `Failed` is `vhd_bridge::reset()` (task 13.3).
pub(super) fn transition_to(new_state: BridgeState, reason: Option<&'static str>) {
    let tx = ensure_channel();
    if matches!(tx.borrow().state, BridgeState::Failed) {
        return;
    }
    let mut snapshot = tx.borrow().clone();
    snapshot.state = new_state;
    snapshot.last_reason = reason.map(|r| r.to_owned());
    snapshot.last_change_at_ms = now_unix_ms();
    snapshot.error_code = reason_to_error_code(new_state, reason).map(|c| c.to_owned());
    let _ = tx.send(snapshot);
}

/// Convenience entry point for the permanent `Failed` transition.
/// Routed through [`transition_to`] so the sink-state guard above
/// also catches the "two distinct permanent errors back-to-back"
/// race: only the first reason wins, and that is the one persisted
/// on the wire-visible `error_code`.
pub(super) fn transition_to_failed(reason: &'static str) {
    transition_to(BridgeState::Failed, Some(reason));
}

/// Force the snapshot back to a fresh `Initializing` state, bypassing
/// the sink-state guard in [`transition_to`]. This is the **only**
/// sanctioned escape from `Failed` (design.md §"状态机":
/// `Failed → Initializing: secret_version 变更 / 进程重启 /
/// vhd_bridge::reset()`); routine I/O errors must continue to use
/// [`transition_to`] so they cannot accidentally clear a permanent
/// `secret_outdated` / `peer_not_vhdmount` / `version_mismatch`.
///
/// Called from `vhd_bridge::reset()` (task 13.3) via
/// [`super::worker::request_reset`]. `last_reason` and `error_code`
/// are cleared so the caller observes a clean restart and the
/// next `transition_to_failed(...)` (if it lands again) records its
/// own reason from scratch.
pub(super) fn force_reset_to_initializing() {
    let tx = ensure_channel();
    let mut snapshot = tx.borrow().clone();
    snapshot.state = BridgeState::Initializing;
    snapshot.last_reason = None;
    snapshot.last_change_at_ms = now_unix_ms();
    snapshot.error_code = None;
    let _ = tx.send(snapshot);
}

/// Record a successful `ReportAck.accepted`: the first one promotes
/// the bridge to `Authorized` (Requirement 6.5 / 12.1); subsequent
/// accepteds are deduped here so the watch channel does not emit a
/// new frame for every heartbeat report. This keeps the IPC
/// observability key calm on long-lived steady-state sessions.
///
/// `error_code` is intentionally not touched: it is only meaningful in
/// `Failed` / `Denied` states per Requirement 12.5 and `transition_to`
/// already cleared it on the way through `Connected → Authorized`.
pub(super) fn record_accepted() {
    let tx = ensure_channel();
    if matches!(tx.borrow().state, BridgeState::Authorized) {
        return;
    }
    transition_to(BridgeState::Authorized, None);
}

/// Increment `log_drop_count` by `by` and republish. Called from the
/// log sink (task 10.1) when its bounded queue overflows. The counter
/// is monotone (Requirement 18.10) — `saturating_add` only matters in
/// the pathological u64-overflow case, but using it costs nothing.
pub(super) fn log_drop_count_inc(by: u64) {
    if by == 0 {
        return;
    }
    let tx = ensure_channel();
    let mut snapshot = tx.borrow().clone();
    snapshot.log_drop_count = snapshot.log_drop_count.saturating_add(by);
    let _ = tx.send(snapshot);
}

// ---------------------------------------------------------------------------
// Active remote-session counter (Task 15.7 / Requirements 15.2, 15.3, 15.6)
//
// Drives the `active-session-count` IPC observability key consumed by
// the Flutter `Maintenance_Overlay` (design.md §"Maintenance_Overlay").
// Held outside the `BridgeStateSnapshot` watch channel on purpose:
//   - Property 15 (design.md line 922) freezes the snapshot to exactly
//     six keys; adding a seventh would break that contract.
//   - The overlay's source of truth, per design.md line 458, is the
//     existing `Data::ControlledSessionCount` IPC variant, not a new
//     bridge-state field.
//   - Updates happen on the *authorized*-connection lifecycle owned by
//     `src/server/connection.rs`, which has no access to a Tokio
//     runtime in its `Drop` impl — `AtomicUsize` keeps both the
//     increment hook (`on_remote_authorized`) and the decrement hook
//     (RAII drop) lock-free and async-free.
//
// Increments only land for connections that survived the §11.4 peer-
// approval gate (which calls `try_start_cm(.., authorized=true)`
// followed by `on_remote_authorized`). Rejected connections do not
// reach `on_remote_authorized`, so they cannot bump the counter and
// therefore cannot trigger the overlay (Requirement 15.3). Decrements
// fire from the RAII guard's `Drop`, covering both normal teardown
// and abnormal close paths.
// ---------------------------------------------------------------------------

/// Process-wide count of currently authorized remote sessions.
/// `Relaxed` ordering is sufficient: callers consume this value via a
/// 1-second IPC poll (`tray.rs::start_query_session_count`), so we do
/// not need to synchronize with any other memory location.
static ACTIVE_SESSION_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Increment the active remote-session counter. Called from
/// `Connection::on_remote_authorized` after the §11.4 peer-approval
/// gate returns `Approved` or `BridgeUnavailable`.
pub(super) fn inc_active_session_count() {
    ACTIVE_SESSION_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Decrement the active remote-session counter. Called from the RAII
/// drop guard owned by `Connection`; bottoms out at 0 via
/// `saturating_sub` so a duplicate decrement (e.g. a future refactor)
/// cannot wrap to `usize::MAX` and leave the overlay stuck visible.
pub(super) fn dec_active_session_count() {
    // `fetch_update` lets us implement a saturating decrement with a
    // single CAS loop. The closure is pure, so the loop terminates in
    // practice after at most a handful of iterations under contention.
    let _ = ACTIVE_SESSION_COUNT.fetch_update(
        Ordering::Relaxed,
        Ordering::Relaxed,
        |n| Some(n.saturating_sub(1)),
    );
}

/// Read the current active remote-session count. Consumed by the IPC
/// `Data::ControlledSessionCount` query handler in `src/ipc.rs` so the
/// Flutter overlay can decide visibility.
pub(super) fn active_session_count() -> usize {
    ACTIVE_SESSION_COUNT.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a `(state, reason)` pair to a stable, locale-independent
/// `errorCode` string. Returns `None` for non-error states or when no
/// recognised reason is attached. The output is always a member of
/// [`ALLOWED_ERROR_CODES`] (asserted in debug builds and by Property
/// test 15 / task 13.4).
///
/// `REASON_PIPE_CLOSED` and `REASON_PIPE_TIMEOUT` are intentionally
/// not mapped: pipe-close / timeout are transient `Initializing`
/// events, not user-facing `Failed` / `Denied` states.
fn reason_to_error_code(
    state: BridgeState,
    reason: Option<&'static str>,
) -> Option<&'static str> {
    let r = reason?;
    let code = match (state, r) {
        (BridgeState::Failed, REASON_SECRET_OUTDATED) => "vhd.bridge.failed.secret_outdated",
        (BridgeState::Failed, REASON_PEER_NOT_VHDMOUNT) => "vhd.bridge.failed.peer_not_vhdmount",
        (BridgeState::Failed, REASON_VERSION_MISMATCH) => "vhd.bridge.failed.version_mismatch",
        (BridgeState::Denied, REASON_DENY) => "vhd.bridge.denied.deny",
        (BridgeState::Denied, REASON_RATE_LIMITED) => "vhd.bridge.denied.rate_limited",
        (BridgeState::Denied, REASON_INVALID_PROOF) => "vhd.bridge.denied.invalid_proof",
        (BridgeState::Denied, REASON_INVALID_MAC) => "vhd.bridge.denied.invalid_mac",
        _ => return None,
    };
    debug_assert!(
        ALLOWED_ERROR_CODES.contains(&code),
        "reason_to_error_code produced an unexpected code: {}",
        code
    );
    Some(code)
}

#[inline]
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Sibling-only test hook
// ---------------------------------------------------------------------------

/// Reset the watch channel back to the initial snapshot. Used only by
/// in-module tests; production code reaches the worker reset via the
/// public `vhd_bridge::reset()` façade (task 13.3).
#[cfg(test)]
pub(super) fn reset_for_tests() {
    if let Some(tx) = STATE_TX.get() {
        let _ = tx.send(initial_snapshot());
    }
}

/// Shared `Mutex` used by every test that mutates the
/// process-singleton `STATE_TX` watch channel. Tests in this module
/// AND in sibling modules (notably `worker::tests` for the Property 7
/// state-machine proptest) acquire this lock so a parallel test
/// runner cannot interleave their snapshot mutations.
#[cfg(test)]
pub(super) fn shared_test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The OnceLock-backed channel is process-global. Serialise the
    /// tests that mutate it so they do not interleave.
    ///
    /// **Why a `fn` rather than a `static`**: this critical section
    /// must be shared with sibling-module tests that drive the same
    /// `STATE_TX` watch channel (notably
    /// `vhd_bridge::worker::tests::worker_state_invariants_*`).
    /// Returning the process-singleton from `shared_test_lock()`
    /// guarantees a single point of serialisation; defining a
    /// private static here would let parallel test runners
    /// interleave a sibling module's `transition_to` writes with
    /// this module's reads.
    #[allow(non_snake_case)]
    fn TEST_LOCK() -> &'static Mutex<()> {
        shared_test_lock()
    }

    #[test]
    fn initial_snapshot_carries_secret_version() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        let s = initial_snapshot();
        assert_eq!(s.state, BridgeState::Initializing);
        assert_eq!(s.secret_version, SHARED_SECRET_VERSION);
        assert!(s.last_reason.is_none());
        assert!(s.error_code.is_none());
        assert_eq!(s.log_drop_count, 0);
    }

    #[test]
    fn transition_to_publishes_state_and_clears_error_code_on_success_states() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        reset_for_tests();
        transition_to(BridgeState::Connected, None);
        let s = current_snapshot();
        assert_eq!(s.state, BridgeState::Connected);
        assert!(s.error_code.is_none());
    }

    #[test]
    fn transition_to_failed_attaches_error_code_from_reason_table() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        reset_for_tests();
        transition_to_failed(REASON_SECRET_OUTDATED);
        let s = current_snapshot();
        assert_eq!(s.state, BridgeState::Failed);
        assert_eq!(s.last_reason.as_deref(), Some(REASON_SECRET_OUTDATED));
        assert_eq!(
            s.error_code.as_deref(),
            Some("vhd.bridge.failed.secret_outdated")
        );
        assert!(ALLOWED_ERROR_CODES.contains(&s.error_code.as_deref().unwrap()));
    }

    #[test]
    fn transition_to_failed_is_sticky_within_one_startup_cycle() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        reset_for_tests();
        transition_to_failed(REASON_SECRET_OUTDATED);
        // Subsequent transient I/O errors must not flip back.
        transition_to(BridgeState::Initializing, Some(REASON_PIPE_CLOSED));
        transition_to_failed(REASON_PEER_NOT_VHDMOUNT);
        let s = current_snapshot();
        assert_eq!(s.state, BridgeState::Failed);
        assert_eq!(s.last_reason.as_deref(), Some(REASON_SECRET_OUTDATED));
        assert_eq!(
            s.error_code.as_deref(),
            Some("vhd.bridge.failed.secret_outdated")
        );
    }

    #[test]
    fn force_reset_to_initializing_escapes_failed_sink_state() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        reset_for_tests();
        transition_to_failed(REASON_SECRET_OUTDATED);
        assert_eq!(current_snapshot().state, BridgeState::Failed);

        // The only sanctioned escape from `Failed`.
        force_reset_to_initializing();
        let s = current_snapshot();
        assert_eq!(s.state, BridgeState::Initializing);
        assert!(s.last_reason.is_none());
        assert!(s.error_code.is_none());

        // After reset, ordinary transitions resume normally.
        transition_to(BridgeState::Connected, None);
        assert_eq!(current_snapshot().state, BridgeState::Connected);
    }

    #[test]
    fn record_accepted_promotes_to_authorized_and_is_idempotent() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        reset_for_tests();
        transition_to(BridgeState::Connected, None);
        record_accepted();
        let first_change = current_snapshot().last_change_at_ms;
        assert_eq!(current_snapshot().state, BridgeState::Authorized);
        // Second call is a no-op — last_change_at_ms must not advance.
        std::thread::sleep(std::time::Duration::from_millis(2));
        record_accepted();
        let s = current_snapshot();
        assert_eq!(s.state, BridgeState::Authorized);
        assert_eq!(s.last_change_at_ms, first_change);
    }

    #[test]
    fn log_drop_count_inc_is_monotone() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        reset_for_tests();
        let before = current_snapshot().log_drop_count;
        log_drop_count_inc(3);
        log_drop_count_inc(0); // ignored
        log_drop_count_inc(7);
        let after = current_snapshot().log_drop_count;
        assert_eq!(after - before, 10);
    }

    #[test]
    fn active_session_count_inc_dec_round_trip() {
        // Counter is process-global; serialize against the other
        // observability tests so a parallel runner cannot interleave.
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        let baseline = active_session_count();

        inc_active_session_count();
        inc_active_session_count();
        assert_eq!(active_session_count(), baseline + 2);

        dec_active_session_count();
        assert_eq!(active_session_count(), baseline + 1);

        dec_active_session_count();
        assert_eq!(active_session_count(), baseline);
    }

    #[test]
    fn active_session_count_dec_saturates_at_zero() {
        // Double-decrement must not wrap to usize::MAX, otherwise a
        // bug in the RAII guard would leave the overlay stuck visible.
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        // Drain to zero first; any leftover state from prior tests is
        // bounded since each test pairs its inc/dec, but be defensive.
        while active_session_count() > 0 {
            dec_active_session_count();
        }
        dec_active_session_count();
        dec_active_session_count();
        assert_eq!(active_session_count(), 0);
    }

    #[test]
    fn reason_to_error_code_only_emits_allowed_codes() {
        for &(state, reason) in &[
            (BridgeState::Failed, REASON_SECRET_OUTDATED),
            (BridgeState::Failed, REASON_PEER_NOT_VHDMOUNT),
            (BridgeState::Failed, REASON_VERSION_MISMATCH),
            (BridgeState::Denied, REASON_DENY),
            (BridgeState::Denied, REASON_RATE_LIMITED),
            (BridgeState::Denied, REASON_INVALID_PROOF),
            (BridgeState::Denied, REASON_INVALID_MAC),
        ] {
            let code = reason_to_error_code(state, Some(reason)).unwrap();
            assert!(ALLOWED_ERROR_CODES.contains(&code), "{} not allowed", code);
        }
        // Non-error states / unknown reasons / pipe-class reasons map to None.
        assert_eq!(reason_to_error_code(BridgeState::Connected, None), None);
        assert_eq!(
            reason_to_error_code(BridgeState::Initializing, Some(REASON_PIPE_CLOSED)),
            None
        );
        assert_eq!(
            reason_to_error_code(BridgeState::Initializing, Some(REASON_PIPE_TIMEOUT)),
            None
        );
    }

    // -----------------------------------------------------------------
    // Task 13.4 / Property 15: BridgeStateSnapshot is well-formed
    //
    // Drives `transition_to` / `transition_to_failed` / `record_accepted`
    // / `log_drop_count_inc` with arbitrary sequences and checks that
    // the published snapshot satisfies the four shape invariants the
    // `vhd-bridge-state` IPC observability key promises to consumers.
    //
    // **Validates: Requirements 12.1, 12.2, 12.4, 12.5, 18.10**
    //
    // (a) `state` is always one of the six declared `BridgeState`
    //     variants — enum exhaustiveness pins the domain; the explicit
    //     `matches!` makes the contract grep-able.
    // (b) `error_code` is `Some(_)` iff `state ∈ {Failed, Denied}`,
    //     and any present code is a member of `ALLOWED_ERROR_CODES`.
    // (c) `log_drop_count` is monotonically non-decreasing.
    // (d) `last_change_at_ms` is monotonically non-decreasing.
    // -----------------------------------------------------------------

    use proptest::prelude::*;

    /// Discrete operations driving the centralised observability
    /// writers. Each variant maps 1:1 to a single public `pub(super)`
    /// entry point so the property test exercises exactly the surface
    /// the rest of `vhd_bridge` is allowed to use.
    #[derive(Debug, Clone)]
    enum Op {
        TransitionInitializing,
        TransitionConnected,
        TransitionAuthorized,
        TransitionDenied(&'static str),
        TransitionFailed(&'static str),
        RecordAccepted,
        LogDrop(u64),
    }

    fn apply(op: Op) {
        match op {
            Op::TransitionInitializing => transition_to(BridgeState::Initializing, None),
            Op::TransitionConnected => transition_to(BridgeState::Connected, None),
            Op::TransitionAuthorized => transition_to(BridgeState::Authorized, None),
            Op::TransitionDenied(reason) => transition_to(BridgeState::Denied, Some(reason)),
            Op::TransitionFailed(reason) => transition_to_failed(reason),
            Op::RecordAccepted => record_accepted(),
            Op::LogDrop(n) => log_drop_count_inc(n),
        }
    }

    /// Generator covering every legal `Op`. The denied / failed
    /// variants are constrained to the discrete reason codes the
    /// `reason_to_error_code` table recognises so the (b) iff side
    /// of Property 15 is reachable; sending an unrecognised reason
    /// would be a separate bug, covered by the `reason_to_error_code`
    /// example test above.
    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            Just(Op::TransitionInitializing),
            Just(Op::TransitionConnected),
            Just(Op::TransitionAuthorized),
            Just(Op::TransitionDenied(REASON_DENY)),
            Just(Op::TransitionDenied(REASON_RATE_LIMITED)),
            Just(Op::TransitionDenied(REASON_INVALID_PROOF)),
            Just(Op::TransitionDenied(REASON_INVALID_MAC)),
            Just(Op::TransitionFailed(REASON_SECRET_OUTDATED)),
            Just(Op::TransitionFailed(REASON_PEER_NOT_VHDMOUNT)),
            Just(Op::TransitionFailed(REASON_VERSION_MISMATCH)),
            Just(Op::RecordAccepted),
            (1u64..100u64).prop_map(Op::LogDrop),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 256,
            ..ProptestConfig::default()
        })]

        #[test]
        fn state_invariants_under_arbitrary_transitions(
            ops in proptest::collection::vec(op_strategy(), 0..32),
        ) {
            // OnceLock-backed channel is process-global; serialise
            // against the other observability tests in this module.
            let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
            reset_for_tests();

            let mut prev_log_drop: u64 = 0;
            // After `reset_for_tests` the snapshot's `last_change_at_ms`
            // is whatever the last writer wrote; seed `prev_change_ms`
            // from the post-reset value so the (d) assertion is anchored
            // to the actual baseline rather than 0.
            let mut prev_change_ms: u64 = current_snapshot().last_change_at_ms;

            for op in ops {
                apply(op);
                let s = current_snapshot();

                // (a) state in legal set.
                let state_ok = matches!(
                    s.state,
                    BridgeState::Disabled
                        | BridgeState::Initializing
                        | BridgeState::Connected
                        | BridgeState::Authorized
                        | BridgeState::Denied
                        | BridgeState::Failed
                );
                prop_assert!(state_ok, "unexpected state variant: {:?}", s.state);

                // (b) error_code presence/membership iff state ∈ {Failed, Denied}.
                match s.state {
                    BridgeState::Failed | BridgeState::Denied => {
                        prop_assert!(
                            s.error_code.is_some(),
                            "{:?} state must carry Some(error_code) (reason={:?})",
                            s.state,
                            s.last_reason
                        );
                        let code = s.error_code.as_deref().unwrap();
                        prop_assert!(
                            ALLOWED_ERROR_CODES.contains(&code),
                            "error_code {:?} not in ALLOWED_ERROR_CODES",
                            code
                        );
                    }
                    _ => {
                        prop_assert!(
                            s.error_code.is_none(),
                            "non-Failed/Denied state {:?} carries error_code: {:?}",
                            s.state,
                            s.error_code
                        );
                    }
                }

                // (c) log_drop_count monotonically non-decreasing.
                prop_assert!(
                    s.log_drop_count >= prev_log_drop,
                    "log_drop_count regressed: {} < {}",
                    s.log_drop_count,
                    prev_log_drop
                );
                prev_log_drop = s.log_drop_count;

                // (d) last_change_at_ms monotonically non-decreasing.
                prop_assert!(
                    s.last_change_at_ms >= prev_change_ms,
                    "last_change_at_ms regressed: {} < {}",
                    s.last_change_at_ms,
                    prev_change_ms
                );
                prev_change_ms = s.last_change_at_ms;
            }
        }
    }
}
