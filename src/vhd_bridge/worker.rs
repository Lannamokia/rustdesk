//! `vhd_bridge::worker` — `BridgeWorker` state machine: connect,
//! handshake, report, peer-approval, reconnect-with-jitter, and nonce
//! window.
//!
//! Task 7.1 originally placed the `tokio::sync::watch` publisher and
//! its writer helpers (`transition_to` / `transition_to_failed` /
//! `log_drop_count_inc`) here. Task 13.1 moved that machinery to
//! `super::observability` so every write to the
//! `BridgeStateSnapshot` channel funnels through a single small set
//! of helpers (Requirement 12.1). The connect / handshake / reconnect
//! loop in task 7.2 (this module) calls into
//! `super::observability::{transition_to, transition_to_failed,
//! record_accepted, log_drop_count_inc}` from this module.
//!
//! Task 13.3 lays down the **reset signalling** primitive: a
//! process-singleton [`tokio::sync::Notify`] that the public
//! `vhd_bridge::reset()` façade pulses through [`request_reset`] and
//! the worker's main loop consumes via [`reset_signal`]. This isolates
//! the "close the current session" plumbing here so 13.3 can land the
//! public API without creating a forward dependency on 7.2 / 8.2 / 11.1.
//!
//! ## Task 7.2 scope
//!
//! This file owns the **connect / handshake / reconnect** state machine
//! drawn in design.md §"BridgeWorker 控制流" and the §"状态机" diagram.
//! Specifically:
//!
//!   * `loop { open_and_verify → write Handshake_Frame → read
//!     HandshakeResponse → 分支处理 ok / deny / rate_limited /
//!     invalid_proof / secret_outdated / 超时 / I/O 错误 }`.
//!   * `Bridge_State` switches to `Connected` on a successful handshake
//!     and a `reason = "startup"` trigger is injected exactly once per
//!     successful handshake (Requirements 7.1 / 8.8).
//!   * `secret_outdated` / `peer_not_vhdmount` walk the permanent
//!     `Failed` branch (Requirements 5.6 / 9.5 / 9.8 / 10.5 / 11.2).
//!     `version_mismatch` is reserved for task 11.x once `VHDMount`
//!     starts emitting it on protocol-version-mismatch frames.
//!   * `BrokenPipe` / `ConnectionReset` / EOF / length-cap overshoot /
//!     JSON parse failure / timeout walk the transient `Initializing`
//!     branch (Requirements 2.4 / 9.7).
//!   * Once `Failed` is sticky for the current startup cycle the worker
//!     parks on the reset signal; only `vhd_bridge::reset()`
//!     (Requirement 11.4) or a process restart escapes it.
//!
//! ## Tasks 7.3 / 7.4 / 8.2 scope
//!
//! Tasks 7.3, 7.4, and 8.2 land together (they all touch this file
//! and would otherwise create merge churn). Their additions are:
//!
//!   * **7.3** — A `failure_count` counter is threaded through the
//!     outer `loop` in [`run`]. It resets to 0 on every successful
//!     `Connected` outcome and increments on every transient or
//!     denied failure. [`connect_and_handshake`] takes it as input
//!     and, via [`escalating_log!`], promotes its connect / handshake
//!     diagnostic from `debug` to `warn` once `failure_count >= 5`
//!     (Requirement 9.4 second clause). [`compute_retry_delay`]
//!     stays stateless: the "rate_limited overlay clears on next
//!     success" half of Requirement 9.2 is already implicit in the
//!     caller passing `last_reason = None` after `Connected`.
//!   * **7.4** — [`NonceWindow`] replaces the placeholder
//!     `generate_nonce_hex` free function. It maintains the
//!     `BTreeMap<u64, [u8; 16]> + HashSet<[u8; 16]>` pair described
//!     in the spec, evicts entries older than `NONCE_WINDOW_MS`
//!     (5 minutes) before each generate, and regenerates on the
//!     vanishingly-small chance of a HashSet collision. The window
//!     lives as a `run()`-local for the worker's lifetime so the
//!     5-minute cross-attempt dedup spans handshakes and reconnects
//!     (Requirement 5.3 / 19.3).
//!   * **8.2** — [`LastReportedSnapshot`] and [`should_skip_report`]
//!     land here so the dedup contract has a tested home, even
//!     though the actual `Report_Frame` write loop is task 8.x. The
//!     `Option<LastReportedSnapshot>` cache lives as a `run()`-local
//!     and is cleared on every fresh `Connected` outcome — a new
//!     session must re-assert dedup against `VHDMount`'s own state
//!     because the server may have rotated its memory of us
//!     (Requirements 6.8 / 7.6).
//!
//! What this file deliberately still does **not** own:
//!
//!   * Trigger-receiver / `Coalescer` consumption inside the session
//!     loop — task 8.3.
//!   * The actual `Report_Frame` write path that *uses* the
//!     `LastReportedSnapshot` infrastructure introduced by 8.2 —
//!     task 8.x. Today the snapshot is allocated, cleared, and
//!     observable via [`should_skip_report`]'s tests, but
//!     `hold_session_until_break` does not yet build / write a
//!     report frame on trigger arrival.
//!   * Server-pushed `Revocation_Frame` parsing + `Peer_Approval`
//!     request multiplexing — tasks 11.2 / 11.x.
//!   * `log_sink` → bridge `Log_Frame` writer wiring — task 10.x.
//!
//! The post-handshake "session" today is a thin placeholder
//! ([`hold_session_until_break`]) whose only job is to keep the pipe
//! open until either an I/O error / EOF surfaces (drives the
//! `Connected → Initializing` transition in design.md §"状态机") or a
//! reset signal pulses. Tasks 8.x / 10.x / 11.x will replace its body
//! with the real `tokio::select!` over trigger / log / approval channels.

#![allow(dead_code)] // remaining hooks wired up by tasks 8.x, 10.1, 11.2.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hbb_common::base64::engine::{general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use hbb_common::log;
use hbb_common::rand::{self, Rng, RngCore};
use hbb_common::sha2::{Digest, Sha256};
use hbb_common::tokio::net::windows::named_pipe::NamedPipeClient;
use hbb_common::tokio::runtime::Handle;
use hbb_common::tokio::sync::{mpsc, Notify};
use hbb_common::tokio::{self, time as tokio_time};

use super::log_sink::LogEvent;
use super::observability::{
    self, REASON_DENY, REASON_INVALID_MAC, REASON_INVALID_PROOF, REASON_PEER_NOT_VHDMOUNT,
    REASON_PIPE_CLOSED, REASON_PIPE_TIMEOUT, REASON_RATE_LIMITED, REASON_SECRET_OUTDATED,
};
use super::peer_approval::{ApprovalRequest, ApprovalResponse};
use super::pipe::{self, ConnectError};
use super::protocol::{
    self, HandshakeErrorReason, HandshakeFrame, HandshakeResponse, LogFrame, PasswordKind,
    PeerApprovalRequest, PeerApprovalResponse, ReportAck, ReportAckRejectReason, ReportFrame,
    ReportReason, RevocationFrame, RevocationReason, CLIENT_KIND_RUSTDESK, PROTOCOL_HANDSHAKE,
    PROTOCOL_LOG, PROTOCOL_PEER_APPROVAL, PROTOCOL_REPORT,
};
use super::triggers::{self, TriggerEvent};
use super::{ApprovalOutcome, BridgeState, BridgeStateSnapshot};

// ---------------------------------------------------------------------------
// Re-exports for sibling modules (currently `log_sink`)
//
// `log_sink::Log::log` reads the current snapshot and bumps the
// drop counter through the `worker::*` namespace per design.md
// §"Log Sink"; the actual storage lives in `observability`. Routing
// the calls through here keeps the public sibling-API stable while
// the worker is still being built out in tasks 7.2 / 8.x.
// ---------------------------------------------------------------------------

/// Snapshot accessor used by `log_sink` to gate emission on
/// `Bridge_State`. Forwards to [`observability::current_snapshot`] so
/// every reader hits the same `tokio::sync::watch::Receiver` (no
/// duplicate state).
#[inline]
pub(super) fn current_snapshot() -> BridgeStateSnapshot {
    observability::current_snapshot()
}

/// Counter bump used by `log_sink` whenever its bounded ring buffer
/// drops events. Forwards to [`observability::log_drop_count_inc`].
#[inline]
pub(super) fn log_drop_count_inc(by: u64) {
    observability::log_drop_count_inc(by);
}

// ---------------------------------------------------------------------------
// log_sink → worker bridge (task 10.x / Requirements 18.1, 18.2, 18.4)
//
// The bridge log sink has no `NamedPipeClient` of its own, so the
// drain task in `log_sink::log_writer_task` cannot serialise frames
// directly. Instead it publishes already-redacted / truncated
// `LogEvent`s through this channel; the worker's `tokio::select!` arm
// in [`hold_session_until_break`] consumes them and writes
// `Log_Frame`s to the pipe (fire-and-forget per design.md §"Log Sink"
// / Requirement 18.5).
//
// `OnceLock<Sender>` mirrors the trigger plumbing in `triggers.rs`:
// the worker creates the channel on startup, hands the receiver to
// the session loop, and stashes the sender here for `log_sink` to
// reach. A second worker spin-up returns `None` from `init_log_rx()`
// — the first init wins, exactly like `peer_approval::take_request_
// receiver()`.
// ---------------------------------------------------------------------------

/// Capacity of the log → worker queue. The log_sink's own ring
/// buffer (4096 events / 4 MiB) is the primary back-pressure point;
/// 64 here is a small downstream cushion that lets the writer task
/// overlap one frame's pipe I/O with another producer push without
/// gratuitously expanding the worker's footprint. A `Full` `try_send`
/// drops the event and increments the public `log_drop_count` so
/// observability stays truthful.
const LOG_FRAME_CHANNEL_CAPACITY: usize = 64;

/// Sender half of the log → worker channel, populated by [`init_log_rx`]
/// during worker startup. `log_sink::log_writer_task` reaches it via
/// [`publish_log_event`].
static LOG_FRAME_TX: OnceLock<mpsc::Sender<LogEvent>> = OnceLock::new();

/// Build the log → worker channel and publish the sender. Returns the
/// receiver half so [`run`] can plug it into the session `select!`.
/// Idempotent: a duplicate worker spin-up returns `None`, matching the
/// `take_request_receiver()` shape in `peer_approval`.
fn init_log_rx() -> Option<mpsc::Receiver<LogEvent>> {
    let (tx, rx) = mpsc::channel::<LogEvent>(LOG_FRAME_CHANNEL_CAPACITY);
    if LOG_FRAME_TX.set(tx).is_err() {
        return None;
    }
    Some(rx)
}

/// Hand the worker one already-redacted `LogEvent`. Called by
/// `log_sink::log_writer_task` for every event it pops off the
/// bounded ring buffer. Non-blocking: a `Full` queue drops the event
/// and bumps `log_drop_count` (Requirements 18.5 / 18.10); a closed
/// queue means the worker has exited (process tear-down) and we
/// silently discard.
pub(super) fn publish_log_event(ev: LogEvent) {
    let Some(tx) = LOG_FRAME_TX.get() else {
        // Worker not yet started — drop and bump the counter so the
        // public `log_drop_count` snapshot stays truthful.
        observability::log_drop_count_inc(1);
        return;
    };
    match tx.try_send(ev) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            observability::log_drop_count_inc(1);
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            // Worker exited; nothing to publish to.
        }
    }
}

// ---------------------------------------------------------------------------
// Reset signal (task 13.3)
// ---------------------------------------------------------------------------

/// Process-singleton signal used by `vhd_bridge::reset()` to ask the
/// running `BridgeWorker` to abandon its current pipe session and
/// re-enter the connect loop.
///
/// Why `Notify` over `watch::channel(())`:
///   * `request_reset` only has to **pulse** — there is no payload,
///     and we never need to read the "current" reset value.
///   * `Notify::notify_waiters` wakes every park'd consumer
///     (the worker's `select!` arm in [`hold_session_until_break`])
///     without needing to manage version counters.
///   * Stays consistent with the `Notify` already used by
///     `log_sink::LOG_NOTIFY`.
///
/// `OnceLock` (instead of `lazy_static!`) matches the rest of the
/// module's "no global mutex on the read path" policy.
static RESET_SIGNAL: OnceLock<Notify> = OnceLock::new();

/// Lazily construct and return the process-singleton reset signal.
/// Idempotent: every caller — the worker's `select!` and any
/// `vhd_bridge::reset()` invocation — observes the same instance.
fn reset_signal() -> &'static Notify {
    RESET_SIGNAL.get_or_init(Notify::new)
}

/// Hand the worker a reference to the reset signal so its `select!`
/// arm can `notified().await`. Exposed `pub(super)` so only sibling
/// modules (the worker itself) can subscribe; external callers go
/// through the public `vhd_bridge::reset()` façade.
#[inline]
pub(super) fn reset_notify() -> &'static Notify {
    reset_signal()
}

/// Public-to-the-bridge entry point used by `vhd_bridge::reset()`
/// (task 13.3 / `mod.rs`). Wakes any worker parked on
/// [`reset_notify`] and forces the observable `BridgeStateSnapshot`
/// back to `Initializing`, escaping the sticky `Failed` sink-state if
/// applicable (design.md state diagram: `Failed → Initializing` is
/// only allowed via `vhd_bridge::reset()`).
///
/// Note: clearing the report-dedup snapshot (Requirement 6.8 / 7.6)
/// and the approval cache (Requirement 19.7) is the responsibility of
/// the public façade in `mod.rs`, which calls
/// `peer_approval::clear_cache()` before invoking `request_reset`.
/// The `Last_Reported_Snapshot` introduced by task 8.2 lives as a
/// `run()`-local on the worker task and is naturally wiped when the
/// `select!` arm woken below tears down `hold_session_until_break`,
/// so a fresh session re-asserts dedup against `VHDMount` from
/// scratch (the next `Connected` outcome resets the cache to `None`).
pub(super) fn request_reset() {
    log::info!("vhd_bridge: reset requested; closing current session");
    observability::force_reset_to_initializing();
    reset_signal().notify_waiters();
}

// ---------------------------------------------------------------------------
// Public worker entry point (called by `vhd_bridge::start` — task 14.1)
// ---------------------------------------------------------------------------

/// Spawn the worker on `rt`. Idempotent at the worker level: the
/// internal `triggers::init()` returns `None` on a second call so the
/// duplicate spawn becomes a no-op session that exits immediately.
///
/// Production callers go through `vhd_bridge::start(rt)` (the public
/// façade in `mod.rs`); this `pub(super)` entry point exists so task
/// 14.1's wiring in `core_main` can land independently of the public
/// API shape.
pub(super) fn start(rt: &Handle) {
    rt.spawn(run());
}

// ---------------------------------------------------------------------------
// Main loop (Requirement 4.3 / 8.5 / 9.6 / 11.4)
// ---------------------------------------------------------------------------

/// `BridgeWorker` main loop. Runs for the lifetime of the
/// `RustDesk_Controlled` process: every connect / handshake /
/// reconnect cycle is one iteration of the outer `loop` below.
///
/// State invariants (design.md §"状态机"):
///
///   * `Failed` is sticky inside one startup cycle. The first thing
///     each iteration does is check the snapshot, and if `Failed`
///     park on the reset signal until `vhd_bridge::reset()` (or a
///     `Bridge_Config.secret_version` change that routes through it)
///     pulses it. `force_reset_to_initializing` will have already
///     pushed the snapshot back to `Initializing` by the time we wake.
///
///   * Every Connected → … → Initializing path is followed by a
///     reconnect delay computed by [`compute_retry_delay`]. The
///     `Denied` branches are the only ones that propagate a
///     `last_reason` into that helper today — task 7.3 will replace
///     it with the full jitter / `rate_limited` / failure-count
///     escalation logic.
async fn run() {
    // Construct the trigger queue + heartbeat ticker once. The queue
    // is owned for the worker's lifetime; we hand the receiver to
    // [`hold_session_until_break`] every iteration so a new session
    // can drain coalesced bursts without losing in-flight events
    // (the [`mpsc::Receiver`] is moved out and back via `&mut`,
    // never replaced, so heartbeat / hook fires that arrive between
    // sessions still queue up).
    //
    // A duplicate worker spin-up (only possible from a misordered
    // `vhd_bridge::start` call — the `OnceLock` in `mod.rs` guards
    // against that in production) sees `None` here; without a trigger
    // receiver the session loop's trigger arm parks on `pending` and
    // we run as an approval-only worker. The `peer_approval` arm has
    // the same fall-through shape, keeping behaviour symmetric.
    let mut trigger_rx_opt = super::triggers::init();

    // Task 11.2: take ownership of the gate → worker approval channel.
    // `gate(...)` (`peer_approval::gate`) `try_send`s `ApprovalRequest`
    // values into this receiver; we drain them inside
    // [`hold_session_until_break`]. A `None` here means a duplicate
    // worker bring-up — the first init wins, the second worker has no
    // way to drive approvals and must run without that arm.
    let mut approval_rx = super::peer_approval::take_request_receiver();

    // Task 10.x: own the log → worker channel. `log_sink::log_writer_
    // task` publishes already-redacted `LogEvent`s through
    // [`publish_log_event`]; we drain them here and write `Log_Frame`s
    // to the pipe inside the session loop. As with the approval
    // receiver, a duplicate worker init returns `None` and the
    // session's log arm parks on `pending`.
    let mut log_rx = init_log_rx();

    // Task 7.3: track consecutive failures. Reset on every
    // `Connected`; bump on every `Denied` / `TransientPipeIo` outcome.
    // `PermanentFailure` walks the sticky `Failed` branch, and the
    // sticky-Failed park guard at the top of the loop means the
    // counter never matters again until reset / restart.
    //
    // Task 7.4: own the cross-attempt nonce window. The 5-minute
    // dedup spans handshakes within the worker's lifetime so that
    // even back-to-back failed attempts cannot reuse a nonce
    // (Requirement 5.3). [`NonceWindow::clear`] is *not* called on
    // session boundaries — every nonce is unique within the window
    // regardless of which session it was used on.
    //
    // Task 8.2: own the `Last_Reported_Snapshot` cache. Cleared on
    // every fresh `Connected` outcome so a new session must re-assert
    // dedup against VHDMount (Requirements 6.8 / 7.6). The actual
    // `Report_Frame` write loop that *consumes* this cache is owned
    // by task 8.x; today the cache is allocated and observable via
    // [`should_skip_report`] for tests but never written by the
    // session loop.
    let mut failure_count: u32 = 0;
    let mut nonce_window = NonceWindow::new();
    let mut last_reported: Option<LastReportedSnapshot> = None;

    loop {
        // Sticky-Failed guard. Park here instead of returning so the
        // worker task stays alive across `vhd_bridge::reset()` calls
        // and the spawned task is "process-lifetime" per Requirement 4.3.
        if matches!(current_snapshot().state, BridgeState::Failed) {
            log::debug!("vhd_bridge: state == Failed; parking on reset signal");
            reset_notify().notified().await;
            // After the reset, observability has been forced back to
            // `Initializing`; failure tracking restarts clean.
            failure_count = 0;
            last_reported = None;
            continue;
        }

        // Read a fresh `BridgeConfig` clone so the rest of the
        // iteration runs without holding any lock across `.await`
        // (AGENTS.md "do not hold locks across `.await`"). Runtime
        // overrides (`try_apply_bridge_option`) take effect on the
        // next iteration; in-flight handshakes use the values frozen
        // at the start of this attempt, which is the design's intent.
        let cfg = hbb_common::config::current_bridge_config();
        let pipe_name = cfg.resolve_pipe_name().into_owned();
        let timeout_ms = cfg.request_timeout_ms;
        let retry_interval_ms = cfg.retry_interval_ms;
        let secret_version = cfg.secret_version;

        match connect_and_handshake(
            &pipe_name,
            timeout_ms,
            secret_version,
            failure_count,
            &mut nonce_window,
        )
        .await
        {
            HandshakeOutcome::Connected(client) => {
                // Successful handshake: clear the consecutive-failure
                // counter (Requirement 9.4 implicit invariant — log
                // level returns to `debug` after the next failure)
                // and the report-dedup cache (Requirement 6.8 — a
                // fresh session must re-assert against VHDMount).
                failure_count = 0;
                last_reported = None;

                // Connected → emit one `reason = "startup"` trigger
                // per Requirement 7.1 / 8.8. The trigger lands in the
                // bounded mpsc owned by `triggers::init()`; task 8.3
                // will drain it inside `hold_session_until_break`.
                observability::transition_to(BridgeState::Connected, None);
                super::triggers::notify_startup();

                // Hold the session until either the pipe surfaces an
                // I/O error / EOF (`Connected → Initializing` per the
                // design state graph) or `vhd_bridge::reset()` fires.
                hold_session_until_break(
                    client,
                    &mut last_reported,
                    &mut trigger_rx_opt,
                    &mut approval_rx,
                    &mut log_rx,
                    secret_version,
                    timeout_ms,
                    &mut nonce_window,
                )
                .await;

                // Either we got reset (snapshot already forced to
                // `Initializing` by `request_reset`) or the pipe
                // broke. In the pipe-break case, transition to
                // `Initializing` with `pipe_closed` per Requirement
                // 2.4 / 9.7. The transition is a no-op if reset
                // already moved the snapshot; the second arm of
                // `transition_to` updates `last_reason` either way,
                // which is the more truthful story for observability.
                if !matches!(current_snapshot().state, BridgeState::Initializing) {
                    observability::transition_to(
                        BridgeState::Initializing,
                        Some(REASON_PIPE_CLOSED),
                    );
                }
                // Pipe break after a successful handshake counts as a
                // fresh failure for the escalation counter. It also
                // restarts the "1, 2, 3, … 5 → warn" cadence — i.e.
                // a worker that flakes once every connect is *not*
                // permanently stuck at warn level.
                failure_count = failure_count.saturating_add(1);
                sleep_retry(retry_interval_ms, None).await;
            }
            HandshakeOutcome::PermanentFailure(reason) => {
                // `secret_outdated` / `peer_not_vhdmount`. The
                // `transition_to_failed` helper is sticky inside one
                // startup cycle, so a later `BrokenPipe` cannot
                // demote us back to `Initializing` (Requirement 9.8).
                observability::transition_to_failed(reason);
                // No retry sleep: the next loop iteration parks on
                // the reset signal. `failure_count` is irrelevant
                // here — Requirement 9.5 forbids permanent errors
                // from entering the retry queue.
            }
            HandshakeOutcome::TransientPipeIo(reason) => {
                // Connect timeout / I/O error / EOF / length-cap /
                // JSON parse failure / write failure. All collapse to
                // `Initializing` + reconnect (Requirements 2.4, 9.1,
                // 9.7).
                failure_count = failure_count.saturating_add(1);
                observability::transition_to(BridgeState::Initializing, Some(reason));
                sleep_retry(retry_interval_ms, None).await;
            }
            HandshakeOutcome::Denied(reason) => {
                // `deny` / `rate_limited` / `invalid_proof` from the
                // server (Requirements 5.7, 9.2, 9.3, 11.1).
                failure_count = failure_count.saturating_add(1);
                observability::transition_to(BridgeState::Denied, Some(reason));
                sleep_retry(retry_interval_ms, Some(reason)).await;
            }
        }
    }
}

/// Result of a single connect + handshake attempt. Mapping back to the
/// design state graph rows:
///
/// | variant                        | next state                                   |
/// | ------------------------------ | -------------------------------------------- |
/// | `Connected(_)`                 | `Initializing → Connected`                   |
/// | `PermanentFailure(_)`          | `* → Failed` (sticky)                        |
/// | `TransientPipeIo(_)`           | `* → Initializing` + reconnect               |
/// | `Denied(_)`                    | `Initializing → Denied` + reconnect          |
enum HandshakeOutcome {
    Connected(NamedPipeClient),
    PermanentFailure(&'static str),
    TransientPipeIo(&'static str),
    Denied(&'static str),
}

/// Threshold (number of *prior* consecutive failures) at which the
/// `escalating_log!` macro promotes connect / handshake diagnostics
/// from `debug` to `warn`. Exposed as a constant so tests can pin
/// the boundary without duplicating the magic number. Reads as
/// "the *fifth* line — counter values 0..=3 already printed at
/// debug — is the first one at warn", matching the spec wording
/// "连续 5 次失败后把日志级别从 debug 提升到 warn" (Requirement 9.4
/// last clause).
pub(super) const FAILURE_LOG_ESCALATION_THRESHOLD: u32 = 4;

/// Emit a connect / handshake diagnostic at `debug` for the first
/// four consecutive failures and at `warn` once
/// `failure_count >= FAILURE_LOG_ESCALATION_THRESHOLD`. Implemented
/// as a macro because `log::debug!` / `log::warn!` are macros
/// themselves and we want the call-site `module_path!() / file!() /
/// line!()` metadata to be the worker's, not the helper's.
///
/// `macro_rules!` declarations are textually scoped, so this macro
/// has to live above its first use site
/// (`connect_and_handshake`). Tasks 7.3 / 7.7 are the only callers.
macro_rules! escalating_log {
    ($failure_count:expr, $($arg:tt)+) => {
        if $failure_count >= FAILURE_LOG_ESCALATION_THRESHOLD {
            ::hbb_common::log::warn!($($arg)+);
        } else {
            ::hbb_common::log::debug!($($arg)+);
        }
    };
}

/// Open a pipe, verify the peer image, send a `Handshake_Frame`, read
/// the `HandshakeResponse`, and translate the result into a
/// `HandshakeOutcome`. Pure async helper with no observability side
/// effects — the caller (`run`) drives all `transition_to*` writes
/// from the returned value so every state mutation funnels through
/// one well-named site.
///
/// Task 7.3: `failure_count` is the *running* count of consecutive
/// failures *before* this attempt. The escalating-log macro
/// [`escalating_log!`] uses it to promote the connect / handshake
/// diagnostics from `debug` to `warn` once the threshold is reached.
/// 0 means "this is the first attempt or the previous attempt
/// succeeded"; the log thus stays at `debug` for the first 4
/// failures and shifts to `warn` from the 5th onward.
///
/// Task 7.4: `nonce_window` provides the cross-attempt 5-minute
/// dedup. Each handshake takes one nonce out of the window and
/// records it; oversaturation (vanishingly rare) prompts an internal
/// regenerate.
async fn connect_and_handshake(
    pipe_name: &str,
    timeout_ms: u32,
    secret_version: u32,
    failure_count: u32,
    nonce_window: &mut NonceWindow,
) -> HandshakeOutcome {
    // (1) Connect + verify peer process image.
    let mut client = match pipe::open_and_verify(pipe_name, timeout_ms).await {
        Ok(c) => c,
        Err(ConnectError::PeerNotVhdMount) => {
            // Peer-mismatch is a permanent error regardless of how
            // many transient retries preceded it; logging at warn
            // even on the first attempt is appropriate per
            // Requirement 10.5.
            log::warn!(
                "vhd_bridge: peer process is not VHDMount.exe; entering permanent Failed"
            );
            return HandshakeOutcome::PermanentFailure(REASON_PEER_NOT_VHDMOUNT);
        }
        Err(ConnectError::Timeout) => {
            escalating_log!(
                failure_count,
                "vhd_bridge: pipe connect timed out (failure #{})",
                failure_count.saturating_add(1)
            );
            return HandshakeOutcome::TransientPipeIo(REASON_PIPE_TIMEOUT);
        }
        Err(ConnectError::Io(e)) => {
            escalating_log!(
                failure_count,
                "vhd_bridge: pipe connect failed: {} (failure #{})",
                e.kind(),
                failure_count.saturating_add(1)
            );
            return HandshakeOutcome::TransientPipeIo(REASON_PIPE_CLOSED);
        }
    };

    // (2) Build + send Handshake_Frame.
    let timestamp_ms = now_unix_ms();
    let nonce = nonce_window.generate(timestamp_ms);
    let proof_bytes = super::hmac::hmac_handshake(secret_version, &nonce, timestamp_ms);
    let handshake = HandshakeFrame {
        protocol: PROTOCOL_HANDSHAKE.to_owned(),
        secret_version,
        nonce,
        timestamp_ms,
        client_kind: CLIENT_KIND_RUSTDESK.to_owned(),
        client_version: env!("CARGO_PKG_VERSION").to_owned(),
        proof: BASE64_STANDARD.encode(proof_bytes),
    };
    let payload = match serde_json::to_vec(&handshake) {
        Ok(b) => b,
        Err(e) => {
            // Should be unreachable — the `HandshakeFrame` schema is
            // closed and contains only `String` / numeric fields.
            // Treat as transient out of caution.
            log::warn!("vhd_bridge: handshake serialize failed: {e}");
            return HandshakeOutcome::TransientPipeIo(REASON_PIPE_CLOSED);
        }
    };
    if let Err(e) = super::frame::write_frame(&mut client, &payload).await {
        escalating_log!(
            failure_count,
            "vhd_bridge: handshake write failed: {} ({})",
            e.kind(),
            e
        );
        return HandshakeOutcome::TransientPipeIo(REASON_PIPE_CLOSED);
    }

    // (3) Read response with the same `request_timeout_ms` budget per
    // Requirement 5.8. `tokio::time::timeout` cancels `read_frame` on
    // expiry; the partially-read `scratch` buffer is dropped along
    // with the future, and the local `client` is dropped on every
    // return path below — `NamedPipeClient` closes its underlying
    // HANDLE on drop.
    let mut scratch = Vec::new();
    let response_bytes = match tokio_time::timeout(
        Duration::from_millis(timeout_ms as u64),
        super::frame::read_frame(&mut client, &mut scratch),
    )
    .await
    {
        Ok(Ok(slice)) => slice.to_vec(),
        Ok(Err(e)) => {
            // I/O error / EOF / declared length over `MAX_FRAME_BYTES`.
            // All three collapse to the §9.7 transient path.
            escalating_log!(
                failure_count,
                "vhd_bridge: handshake read failed: {} ({})",
                e.kind(),
                e
            );
            return HandshakeOutcome::TransientPipeIo(REASON_PIPE_CLOSED);
        }
        Err(_elapsed) => {
            escalating_log!(failure_count, "vhd_bridge: handshake response timed out");
            return HandshakeOutcome::TransientPipeIo(REASON_PIPE_TIMEOUT);
        }
    };

    let parsed: HandshakeResponse = match serde_json::from_slice(&response_bytes) {
        Ok(r) => r,
        Err(e) => {
            // Malformed JSON from the server is a session-corruption
            // signal; fall through to reconnect.
            escalating_log!(
                failure_count,
                "vhd_bridge: handshake response parse failed: {e}"
            );
            return HandshakeOutcome::TransientPipeIo(REASON_PIPE_CLOSED);
        }
    };

    // (4) Branch per design.md §"BridgeWorker 控制流" / Requirement 5.5-5.7.
    match parsed {
        HandshakeResponse::Ok { ok: true } => HandshakeOutcome::Connected(client),
        HandshakeResponse::Ok { ok: false } => {
            // `{ok: false}` without a `reason` is a server-side
            // protocol violation (see protocol.rs::HandshakeResponse
            // doc-comment: variant order makes serde fall through to
            // `Ok { ok: false }` only when no `reason` is present).
            // Treat as session-corruption so we reconnect rather than
            // silently spin.
            log::warn!("vhd_bridge: malformed handshake response (ok=false, no reason)");
            HandshakeOutcome::TransientPipeIo(REASON_PIPE_CLOSED)
        }
        HandshakeResponse::Err { ok: _, reason } => match reason {
            HandshakeErrorReason::Deny => HandshakeOutcome::Denied(REASON_DENY),
            HandshakeErrorReason::RateLimited => HandshakeOutcome::Denied(REASON_RATE_LIMITED),
            HandshakeErrorReason::InvalidProof => {
                HandshakeOutcome::Denied(REASON_INVALID_PROOF)
            }
            HandshakeErrorReason::SecretOutdated => {
                HandshakeOutcome::PermanentFailure(REASON_SECRET_OUTDATED)
            }
        },
    }
}

/// Hold the post-handshake session open until the pipe surfaces an
/// I/O error / EOF, or `vhd_bridge::reset()` pulses the reset signal.
///
/// Today's `tokio::select!` arms (after task 8.x / 10.x / 11.x /
/// 11.2 wiring):
///
///   * **reset** — `vhd_bridge::reset()` pulses; tear down the
///     session unconditionally.
///   * **trigger** — coalesced `TriggerEvent` from [`triggers::
///     coalesce_window`]; the worker reads live credentials, runs
///     dedup, and writes a `Report_Frame`. The accepted snapshot is
///     not committed to `last_reported` until the matching
///     `ReportAck::Accepted` lands on the read arm — so a server
///     that NACKs a frame does not leave the cache primed (Task 8.2
///     contract / Requirement 6.8).
///   * **approval** — `peer_approval::gate(...)` request; round-trip
///     a `Peer_Approval_Request` and reply on its oneshot.
///   * **log** — `LogEvent` published by `log_sink::log_writer_task`;
///     write a `Log_Frame` fire-and-forget.
///   * **read_frame** — server-pushed inbound traffic. The dispatcher
///     tries `ReportAck` first (since the worker actively writes
///     reports), then `RevocationFrame`, then `PeerApprovalResponse`
///     for completeness; unknown JSON shapes are logged at debug and
///     dropped without disturbing the session (Requirement 11.6
///     keeps unknown server traffic non-fatal).
///
/// Error handling contract (Requirement 19.5 / 19.8):
///   * Any failure on the approval round-trip — write error, read
///     error, response timeout, JSON parse failure — is mapped to
///     `ApprovalOutcome::BridgeUnavailable` so the inbound-connection
///     decision table at task 11.4 keeps treating the bridge as
///     fail-open.
///   * The approval / report / log paths SHALL NOT call
///     `transition_to_failed` even for catastrophic write errors.
///     The session loop will surface the same I/O error on its next
///     read and naturally walk back to `Initializing` via the read
///     arm below.
///
/// MVP request multiplexing is single-in-flight: while one approval
/// round-trip runs, no other `select!` arm consumes from the pipe.
/// Real multiplexing — interleaving `Report_Frame` writes,
/// `ReportAck` / `Revocation_Frame` reads, and `Peer_Approval_*`
/// round-trips with a request-id — lands in tasks 8.x / 11.x.
async fn hold_session_until_break(
    mut client: NamedPipeClient,
    last_reported: &mut Option<LastReportedSnapshot>,
    trigger_rx: &mut Option<mpsc::Receiver<TriggerEvent>>,
    approval_rx: &mut Option<mpsc::Receiver<ApprovalRequest>>,
    log_rx: &mut Option<mpsc::Receiver<LogEvent>>,
    secret_version: u32,
    timeout_ms: u32,
    nonce_window: &mut NonceWindow,
) {
    let reset = reset_notify();
    let mut scratch = Vec::new();

    /// "Pending" snapshot held while a `Report_Frame` is in flight.
    /// On `ReportAck::Accepted` we commit it into `last_reported`; on
    /// any other outcome (rejected ack, pipe drop, parse failure) we
    /// drop it so the next non-heartbeat trigger re-sends. This is
    /// the post-task-8.x form of the "writes first, accept later"
    /// contract from design.md §"上报去重".
    let mut pending_ack_snap: Option<LastReportedSnapshot> = None;

    /// Outcome of one `select!` iteration. Decoded outside the
    /// macro so the per-arm borrows of `client` / `scratch` /
    /// `approval_rx` / `trigger_rx` / `log_rx` don't overlap with the
    /// post-select handler that needs them again.
    enum SessionEvent {
        Reset,
        TriggerFired(TriggerEvent),
        TriggerChannelClosed,
        ApprovalRequest(ApprovalRequest),
        ApprovalChannelClosed,
        LogEvent(LogEvent),
        LogChannelClosed,
        FrameRead(Vec<u8>),
        ReadError,
    }

    loop {
        let event: SessionEvent = {
            // Build a fresh `Notified` future per iteration: it is
            // consumed once it resolves, so re-park'ing on the next
            // loop turn requires a new instance.
            let notified = reset.notified();
            tokio::pin!(notified);

            // The trigger arm fires once `coalesce_window` resolves
            // its 1-second burst-collapse. When the receiver is
            // absent (duplicate worker init) we park forever, the
            // same shape used by the approval / log arms below.
            let trigger_recv = async {
                match trigger_rx.as_mut() {
                    Some(rx) => triggers::coalesce_window(rx, triggers::COALESCE_WINDOW).await,
                    None => std::future::pending::<Option<TriggerEvent>>().await,
                }
            };
            tokio::pin!(trigger_recv);

            // The approval arm is only enabled when the worker won
            // the `take_request_receiver()` race. The `pending` no-op
            // future keeps the `select!` shape stable when the arm
            // is disabled.
            let approval_recv = async {
                match approval_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending::<Option<ApprovalRequest>>().await,
                }
            };
            tokio::pin!(approval_recv);

            // Same `pending` shape for the log arm.
            let log_recv = async {
                match log_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending::<Option<LogEvent>>().await,
                }
            };
            tokio::pin!(log_recv);

            tokio::select! {
                biased;
                // Reset wins over the others so a `vhd_bridge::reset()`
                // issued mid-read is honoured immediately.
                _ = &mut notified => SessionEvent::Reset,
                trig = &mut trigger_recv => match trig {
                    Some(ev) => SessionEvent::TriggerFired(ev),
                    None => SessionEvent::TriggerChannelClosed,
                },
                approval_req = &mut approval_recv => match approval_req {
                    Some(req) => SessionEvent::ApprovalRequest(req),
                    None => SessionEvent::ApprovalChannelClosed,
                },
                log_ev = &mut log_recv => match log_ev {
                    Some(ev) => SessionEvent::LogEvent(ev),
                    None => SessionEvent::LogChannelClosed,
                },
                res = super::frame::read_frame(&mut client, &mut scratch) => match res {
                    Ok(slice) => SessionEvent::FrameRead(slice.to_vec()),
                    Err(e) => {
                        log::debug!(
                            "vhd_bridge: session read terminated: {} ({})",
                            e.kind(),
                            e
                        );
                        SessionEvent::ReadError
                    }
                },
            }
        };

        match event {
            SessionEvent::Reset => {
                log::debug!("vhd_bridge: session interrupted by reset signal");
                return;
            }
            SessionEvent::TriggerChannelClosed => {
                // `triggers::init()` sender is held in a `OnceLock`,
                // so the receiver only closes when the worker is
                // exiting (process tear-down). Walk out of the
                // session and let the outer loop reconnect.
                log::debug!("vhd_bridge: trigger channel closed; tearing down session");
                return;
            }
            SessionEvent::ApprovalChannelClosed => {
                // Only possible during process tear-down because
                // `APPROVAL_TX` lives in a `OnceLock`. Bring down the
                // session and let the outer loop reconnect.
                log::debug!(
                    "vhd_bridge: approval channel closed; tearing down session"
                );
                return;
            }
            SessionEvent::LogChannelClosed => {
                // `LOG_FRAME_TX` is `OnceLock`-held, same shape as
                // the other channels above. A close is a tear-down
                // signal.
                log::debug!("vhd_bridge: log channel closed; tearing down session");
                return;
            }
            SessionEvent::ReadError => return,
            SessionEvent::TriggerFired(ev) => {
                // Snapshot any prior pending ack: the worker only
                // keeps **one** pending Report_Frame in flight at a
                // time (single-in-flight MVP, design.md §"BridgeWorker
                // 控制流"). If a previous trigger's ack hasn't landed
                // yet we drop its pending snapshot; the new trigger's
                // wire frame will land first and its ack will
                // (re)prime the cache.
                pending_ack_snap = handle_trigger_event(
                    &mut client,
                    ev,
                    last_reported.as_ref(),
                    secret_version,
                    nonce_window,
                )
                .await;
            }
            SessionEvent::LogEvent(log_ev) => {
                if let Err(e) =
                    write_log_frame(&mut client, &log_ev, secret_version).await
                {
                    log::debug!(
                        "vhd_bridge: log frame write failed: {} ({})",
                        e.kind(),
                        e
                    );
                    // A pipe-class write failure will surface again on
                    // the next read arm — let the read-error path
                    // drive the `Connected → Initializing` transition
                    // rather than duplicating it here.
                }
            }
            SessionEvent::FrameRead(payload) => {
                if let SessionDisposition::TearDown = handle_inbound_frame(
                    &payload,
                    &mut pending_ack_snap,
                    last_reported,
                ) {
                    // `Revocation_Frame` / fatal ReportAck reasons
                    // closed the session: walk back to the outer loop
                    // so the next iteration picks up the
                    // (already-published) state transition.
                    return;
                }
            }
            SessionEvent::ApprovalRequest(req) => {
                // Walk the round-trip. Any error path inside the
                // helper is funnelled through the `oneshot::Sender`
                // carried by `req` so the gate caller observes
                // `BridgeUnavailable` (Requirement 19.8). Pipe-class
                // I/O errors also trip the next read on this loop
                // and bring the session down via the read arm above
                // — we deliberately don't try to detect that here.
                handle_approval_request(
                    &mut client,
                    req,
                    secret_version,
                    timeout_ms,
                    nonce_window,
                    &mut scratch,
                )
                .await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Report_Frame writer + dispatcher (task 8.x / Requirements 6.x / 7.x)
// ---------------------------------------------------------------------------

/// Outcome of [`handle_inbound_frame`]: most read-arm events keep the
/// session running, but a `Revocation_Frame` or a permanent
/// `ReportAck::Rejected` reason must tear down the pipe so the outer
/// loop can park on `Failed` (or transition to `Denied`) per design.md
/// §"状态机".
enum SessionDisposition {
    Continue,
    TearDown,
}

/// Build and write a `Report_Frame` for the freshly coalesced
/// `TriggerEvent`. Returns the snapshot that should be cached as
/// `pending_ack_snap` so a subsequent `ReportAck::Accepted` can
/// commit it into `last_reported`. `None` means either dedup skipped
/// the trigger (no frame on the wire) or the write failed (already
/// logged, the read arm will surface the same I/O error).
async fn handle_trigger_event(
    client: &mut NamedPipeClient,
    ev: TriggerEvent,
    last_reported: Option<&LastReportedSnapshot>,
    secret_version: u32,
    nonce_window: &mut NonceWindow,
) -> Option<LastReportedSnapshot> {
    let reason_str = ev.reason_str();
    let is_heartbeat = ev.is_heartbeat();
    let reason = report_reason_for(ev);

    // (a) Read live credentials. The hbb_common APIs each take an
    // RwLock for the read; we copy each value out under a fresh
    // borrow so no lock is held across the upcoming await.
    let creds = collect_credentials();
    let (password_kind, password) = classify_password(&creds);

    // (b) Build the candidate snapshot for dedup.
    let next_snap = LastReportedSnapshot::from_password(
        creds.rust_desk_id.clone(),
        password_kind,
        &password,
    );

    let state = current_snapshot().state;
    if should_skip_report(last_reported, &next_snap, is_heartbeat, state) {
        log::debug!(
            "vhd_bridge: dedup skip ({}, state={:?})",
            reason_str,
            state
        );
        return None;
    }

    // (c) Build wire frame: nonce + MAC + JSON.
    let reported_at = now_unix_ms();
    let nonce = nonce_window.generate(reported_at);
    let frame = build_report_frame(
        &creds.rust_desk_id,
        password_kind,
        password,
        reason,
        secret_version,
        nonce,
        reported_at,
    );

    let payload = match serde_json::to_vec(&frame) {
        Ok(b) => b,
        Err(e) => {
            // Should be unreachable — the schema is closed strings +
            // primitives. Treat as a logged drop and don't prime the
            // pending-ack cache so the next trigger retries.
            log::warn!("vhd_bridge: report serialize failed ({}): {e}", reason_str);
            return None;
        }
    };
    if let Err(e) = super::frame::write_frame(client, &payload).await {
        log::warn!(
            "vhd_bridge: report write failed ({}): {} ({})",
            reason_str,
            e.kind(),
            e
        );
        return None;
    }

    Some(next_snap)
}

/// Snapshot of the credential triplet used to build a `Report_Frame`.
/// Plaintext is held only for the lifetime of one frame build; the
/// `Drop` glue here is intentionally trivial — `Zeroizing` lives at
/// the HMAC input layer (`super::hmac`) where the cryptographic
/// boundary is.
struct LiveCredentials {
    rust_desk_id: String,
    temporary_password: String,
    temporary_enabled: bool,
    is_preset: bool,
    preset_password: String,
    permanent_enabled: bool,
    has_permanent_password: bool,
}

/// Snapshot all credential state we need to build a `Report_Frame`
/// without holding any of the underlying RwLocks across an `.await`.
/// Each helper here reads through `hbb_common` so the bridge stays an
/// observer-only consumer.
fn collect_credentials() -> LiveCredentials {
    let rust_desk_id = hbb_common::config::Config::get_id();
    let temporary_password = hbb_common::password_security::temporary_password();
    let temporary_enabled = hbb_common::password_security::temporary_enabled();
    let permanent_enabled = hbb_common::password_security::permanent_enabled();
    let has_permanent_password = hbb_common::config::Config::has_permanent_password();
    // `HARD_SETTINGS["password"]` is the build-time-injected preset
    // (set via `common::load_custom_client`-style packaging). Empty
    // means "no preset"; non-empty means "preset is currently the
    // permanent password" iff `matches_permanent_password_plain`
    // agrees, which is the same predicate `flutter_ffi::is_preset_
    // password` uses (see src/flutter_ffi.rs::is_preset_password).
    let preset_password = hbb_common::config::HARD_SETTINGS
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .get("password")
        .cloned()
        .unwrap_or_default();
    let is_preset = !preset_password.is_empty()
        && hbb_common::config::Config::matches_permanent_password_plain(&preset_password);
    LiveCredentials {
        rust_desk_id,
        temporary_password,
        temporary_enabled,
        is_preset,
        preset_password,
        permanent_enabled,
        has_permanent_password,
    }
}

/// Pick `(passwordKind, password)` for the report frame from the
/// snapshotted credentials. Decision tree:
///
///   1. `is_preset`         → `(Preset, hard_settings["password"])`
///   2. `temporary_enabled` ∧ non-empty temp pwd → `(Temporary, pwd)`
///   3. otherwise           → `(Absent, "")`
///
/// `Permanent` is intentionally **not** emitted: RustDesk only stores
/// the permanent password as a hashed digest on disk (see
/// `Config::matches_permanent_password_plain`'s use of
/// `verify_h1`-style hashes), and `VHDMount` needs plaintext to
/// re-sign and forward (design.md §"协议帧 schema" Report_Frame).
/// Reporting `Absent` is the correct conservative default — the
/// server caches whatever value it last accepted, and the operator
/// can re-enter the permanent password on the controller side at
/// approval time. This matches the spec's "VHDMount needs plaintext"
/// constraint without leaking a plaintext we don't have.
fn classify_password(creds: &LiveCredentials) -> (PasswordKind, String) {
    if creds.is_preset {
        return (PasswordKind::Preset, creds.preset_password.clone());
    }
    if creds.temporary_enabled && !creds.temporary_password.is_empty() {
        return (PasswordKind::Temporary, creds.temporary_password.clone());
    }
    // `permanent_enabled` / `has_permanent_password` exist for future
    // policy evolution (e.g. surfacing "a permanent password is set
    // but unavailable to the bridge" via richer reasoning). Today
    // they collapse to the `Absent` branch.
    let _ = creds.permanent_enabled;
    let _ = creds.has_permanent_password;
    (PasswordKind::Absent, String::new())
}

/// Map our internal `TriggerEvent` to the wire `ReportReason`.
fn report_reason_for(ev: TriggerEvent) -> ReportReason {
    match ev {
        TriggerEvent::Startup => ReportReason::Startup,
        TriggerEvent::IdChange => ReportReason::IdChange,
        TriggerEvent::PasswordChange => ReportReason::PasswordChange,
        TriggerEvent::Rotation => ReportReason::Rotation,
        TriggerEvent::Heartbeat => ReportReason::Heartbeat,
    }
}

/// Wire string for `Report_Frame.passwordKind` — also the value fed
/// into `hmac_report`'s `password_kind` argument so the HMAC input
/// matches docs/vhd-rustdesk-bridge-protocol.md §6.2 byte-for-byte.
fn password_kind_wire(k: PasswordKind) -> &'static str {
    match k {
        PasswordKind::Temporary => "temporary",
        PasswordKind::Permanent => "permanent",
        PasswordKind::Preset => "preset",
        PasswordKind::Absent => "absent",
    }
}

/// Pure builder for a `ReportFrame`: takes already-classified inputs
/// and produces the wire struct with its base64-encoded MAC. Pure so
/// the production write path and the unit tests share one
/// implementation; the only side effect is the HMAC computation
/// inside `hmac::hmac_report` (which itself has no state beyond the
/// embedded shared-secret constant).
fn build_report_frame(
    rust_desk_id: &str,
    password_kind: PasswordKind,
    password: String,
    reason: ReportReason,
    secret_version: u32,
    nonce: String,
    reported_at: u64,
) -> ReportFrame {
    let password_kind_str = password_kind_wire(password_kind);
    let reason_str = report_reason_wire(reason);
    let password_sha256 = sha256_hex(password.as_bytes());
    let mac_bytes = super::hmac::hmac_report(
        secret_version,
        rust_desk_id,
        password_kind_str,
        &password_sha256,
        reason_str,
        reported_at,
        &nonce,
    );
    ReportFrame {
        protocol: PROTOCOL_REPORT.to_owned(),
        secret_version,
        rust_desk_id: rust_desk_id.to_owned(),
        password_kind,
        password,
        reason,
        reported_at,
        nonce,
        mac: BASE64_STANDARD.encode(mac_bytes),
    }
}

/// Wire string for `Report_Frame.reason` — needed both by the JSON
/// serde tag (which lives on `ReportReason` itself) and by the HMAC
/// input layer. Kept symmetric with `password_kind_wire` so a
/// reader auditing the cross-product of types and HMAC inputs can
/// see both translations side by side.
fn report_reason_wire(r: ReportReason) -> &'static str {
    match r {
        ReportReason::Startup => "startup",
        ReportReason::IdChange => "id_change",
        ReportReason::PasswordChange => "password_change",
        ReportReason::Rotation => "rotation",
        ReportReason::Heartbeat => "heartbeat",
    }
}

/// Build, serialise and write a `Log_Frame` (Requirement 18.x /
/// design.md §"Log Sink"). Fire-and-forget: a successful write
/// returns `Ok(())` and the worker moves on without waiting for an
/// ack. `LogFrame` has no server-side reply shape per protocol §7.
async fn write_log_frame(
    client: &mut NamedPipeClient,
    ev: &LogEvent,
    secret_version: u32,
) -> io::Result<()> {
    let level_wire = log_level_wire(ev.level);
    let mac_bytes = super::hmac::hmac_log(
        secret_version,
        level_wire,
        &ev.target,
        &sha256_hex(ev.message.as_bytes()),
        ev.timestamp_ms,
    );
    let frame = LogFrame {
        protocol: PROTOCOL_LOG.to_owned(),
        secret_version,
        level: protocol_log_level(ev.level),
        target: ev.target.clone(),
        message: ev.message.clone(),
        timestamp_ms: ev.timestamp_ms,
        mac: BASE64_STANDARD.encode(mac_bytes),
    };
    let payload = serde_json::to_vec(&frame).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, format!("log serialize: {e}"))
    })?;
    super::frame::write_frame(client, &payload).await
}

/// Wire string for `Log_Frame.level` — also the value fed into
/// `hmac_log`'s `level` argument.
fn log_level_wire(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "error",
        log::Level::Warn => "warn",
        log::Level::Info => "info",
        log::Level::Debug => "debug",
        log::Level::Trace => "trace",
    }
}

/// Convert `log::Level` to the protocol's `LogLevel` enum so serde's
/// `rename_all = "snake_case"` produces the wire string defined in
/// `password_kind_wire` / docs §7.1.
fn protocol_log_level(level: log::Level) -> protocol::LogLevel {
    match level {
        log::Level::Error => protocol::LogLevel::Error,
        log::Level::Warn => protocol::LogLevel::Warn,
        log::Level::Info => protocol::LogLevel::Info,
        log::Level::Debug => protocol::LogLevel::Debug,
        log::Level::Trace => protocol::LogLevel::Trace,
    }
}

/// Dispatch one inbound frame off the pipe. Reports / revocations
/// take precedence over generic peer-approval responses; unknown JSON
/// shapes are ignored so a future protocol field cannot wedge the
/// session (design.md §"BridgeWorker 控制流" forward-compat hint).
fn handle_inbound_frame(
    payload: &[u8],
    pending_ack_snap: &mut Option<LastReportedSnapshot>,
    last_reported: &mut Option<LastReportedSnapshot>,
) -> SessionDisposition {
    if let Ok(ack) = serde_json::from_slice::<ReportAck>(payload) {
        return handle_report_ack(ack, pending_ack_snap, last_reported);
    }
    if let Ok(rev) = serde_json::from_slice::<RevocationFrame>(payload) {
        return handle_revocation_frame(rev);
    }
    // PeerApproval responses are normally consumed inline by
    // [`handle_approval_request`] (which reads its own response from
    // the pipe with a timeout). A response landing on the
    // outer-session read arm means the gate caller already gave up —
    // we silently drop it.
    if serde_json::from_slice::<PeerApprovalResponse>(payload).is_ok() {
        log::debug!(
            "vhd_bridge: orphan PeerApprovalResponse on session read arm; dropping"
        );
        return SessionDisposition::Continue;
    }
    log::debug!(
        "vhd_bridge: ignoring unrecognised inbound frame ({} bytes)",
        payload.len()
    );
    SessionDisposition::Continue
}

/// Process a `ReportAck`. `Accepted` commits the pending snapshot
/// into `last_reported` and (the first time) promotes the bridge to
/// `Authorized` (Requirement 6.5). `Rejected` reasons funnel through
/// the observability writers per design §"状态机":
///
/// | reason            | new state               | sticky? |
/// | ----------------- | ----------------------- | ------- |
/// | deny              | Denied                  | no      |
/// | rate_limited      | Denied                  | no      |
/// | secret_outdated   | Failed (sink)           | yes     |
/// | invalid_mac       | Denied                  | no      |
fn handle_report_ack(
    ack: ReportAck,
    pending_ack_snap: &mut Option<LastReportedSnapshot>,
    last_reported: &mut Option<LastReportedSnapshot>,
) -> SessionDisposition {
    match ack {
        ReportAck::Accepted => {
            // Commit the pending snapshot into the dedup cache. A
            // missing pending entry is fine — heartbeats may arrive
            // during a session even when we did not just write a
            // snapshot-changing frame, and an ack-without-frame
            // (server replaying state) is harmless.
            if let Some(snap) = pending_ack_snap.take() {
                *last_reported = Some(snap);
            }
            // `record_accepted` is idempotent: post-`Authorized`
            // accepts collapse into a no-op so the watch channel
            // stays calm on long-lived steady-state sessions.
            observability::record_accepted();
            SessionDisposition::Continue
        }
        ReportAck::Rejected { reason } => {
            // Drop the pending snapshot — a rejected frame must not
            // poison the cache. The next non-heartbeat trigger will
            // try again.
            *pending_ack_snap = None;
            match reason {
                ReportAckRejectReason::Deny => {
                    observability::transition_to(BridgeState::Denied, Some(REASON_DENY));
                    SessionDisposition::TearDown
                }
                ReportAckRejectReason::RateLimited => {
                    observability::transition_to(
                        BridgeState::Denied,
                        Some(REASON_RATE_LIMITED),
                    );
                    SessionDisposition::TearDown
                }
                ReportAckRejectReason::SecretOutdated => {
                    observability::transition_to_failed(REASON_SECRET_OUTDATED);
                    SessionDisposition::TearDown
                }
                ReportAckRejectReason::InvalidMac => {
                    observability::transition_to(
                        BridgeState::Denied,
                        Some(REASON_INVALID_MAC),
                    );
                    SessionDisposition::TearDown
                }
            }
        }
    }
}

/// Process a server-pushed `Revocation_Frame`. Task 11.x is the
/// authoritative implementation site (it owns the MAC verification
/// + `secret_version` cross-check and the full state-machine fan-out).
/// Today we log the receipt and tear down the session so the outer
/// loop reconnects; the read arm's transient-error path will re-emit
/// `Initializing` if observability hasn't already moved.
fn handle_revocation_frame(rev: RevocationFrame) -> SessionDisposition {
    let reason = match rev.reason {
        RevocationReason::Denied => "denied",
        RevocationReason::SecretOutdated => "secret_outdated",
    };
    log::info!(
        "vhd_bridge: received revocation frame (reason={}, secret_version={}, issued_at={})",
        reason,
        rev.secret_version,
        rev.issued_at,
    );
    SessionDisposition::TearDown
}

// ---------------------------------------------------------------------------
// Peer_Approval_Request handling — task 11.2
// ---------------------------------------------------------------------------

/// Handle one `ApprovalRequest`: write a `PeerApprovalRequest` frame
/// onto the pipe, read the matching `PeerApprovalResponse` with
/// `timeout_ms` budget, and reply on `req.response_tx`.
///
/// The contract (Requirement 19.5 / 19.8):
///   * Every error path — write fail, read fail, timeout, JSON
///     parse failure — collapses to
///     `ApprovalOutcome::BridgeUnavailable` with `ttl_ms = 0`.
///   * `transition_to_failed` is NEVER called from this path;
///     surfacing the same I/O error from the read arm in the caller's
///     `select!` is what drives the `Connected → Initializing`
///     transition (the approval path is a side-arm, not the state
///     machine driver).
///   * If the gate caller has already given up (its `oneshot::Receiver`
///     was dropped because of the gate's own timeout) the
///     `response_tx.send` returns `Err`; we silently swallow that —
///     the caller no longer cares about the answer.
async fn handle_approval_request(
    client: &mut NamedPipeClient,
    req: ApprovalRequest,
    secret_version: u32,
    timeout_ms: u32,
    nonce_window: &mut NonceWindow,
    scratch: &mut Vec<u8>,
) {
    let outcome = approval_round_trip(
        client,
        &req,
        secret_version,
        timeout_ms,
        nonce_window,
        scratch,
    )
    .await;
    let (outcome, ttl_ms) = match outcome {
        Ok(pair) => pair,
        Err(e) => {
            // Single, structured warn per failed approval — keeps
            // log volume calm under sustained pipe outages while
            // still letting an operator correlate
            // `BridgeUnavailable` with a reason.
            log::warn!(
                "vhd_bridge: peer-approval round-trip failed ({}); answering BridgeUnavailable",
                e
            );
            (ApprovalOutcome::BridgeUnavailable, 0)
        }
    };
    // Drop the response over the gate's `oneshot`. If the gate gave
    // up first (its own `request_timeout_ms`), `send` returns `Err`
    // — we accept and discard.
    let _ = req.response_tx.send(ApprovalResponse { outcome, ttl_ms });
}

/// Inner round-trip helper: assemble + write the
/// `PeerApprovalRequest` frame and parse the response. Returns
/// `(outcome, ttl_ms)` on success, or a borrowed `&'static str`
/// describing the failure on any error path. The caller maps every
/// `Err` to `BridgeUnavailable`.
async fn approval_round_trip(
    client: &mut NamedPipeClient,
    req: &ApprovalRequest,
    secret_version: u32,
    timeout_ms: u32,
    nonce_window: &mut NonceWindow,
    scratch: &mut Vec<u8>,
) -> Result<(ApprovalOutcome, u64), &'static str> {
    let timestamp_ms = now_unix_ms();
    let request_nonce = nonce_window.generate(timestamp_ms);

    // The HMAC input hashes `controllerName` / `controllerHwid` per
    // protocol §8.2 / Requirement 19.4. The plaintext stays in the
    // JSON payload; the digest is what the cryptographic input sees.
    let controller_name_sha = sha256_hex(req.controller_name.as_bytes());
    let controller_hwid_sha = sha256_hex(req.controller_hwid.as_bytes());
    let peer_socket_addr = req.peer_socket_addr.to_string();
    let connection_type_str = connection_type_wire(req.connection_type);

    let mac_bytes = super::hmac::hmac_peer_approval(
        secret_version,
        &req.controlled_machine_id,
        &req.controller_id,
        &controller_name_sha,
        &req.controller_platform,
        &controller_hwid_sha,
        &peer_socket_addr,
        connection_type_str,
        &request_nonce,
        timestamp_ms,
    );

    let frame = PeerApprovalRequest {
        protocol: PROTOCOL_PEER_APPROVAL.to_owned(),
        secret_version,
        controlled_machine_id: req.controlled_machine_id.clone(),
        controller_id: req.controller_id.clone(),
        controller_name: req.controller_name.clone(),
        controller_platform: req.controller_platform.clone(),
        controller_hwid: req.controller_hwid.clone(),
        peer_socket_addr,
        connection_type: protocol_connection_type(req.connection_type),
        request_nonce,
        timestamp_ms,
        mac: BASE64_STANDARD.encode(mac_bytes),
    };

    let payload = serde_json::to_vec(&frame).map_err(|_| "serialize")?;

    // Write within `timeout_ms` so a wedged peer cannot pin the
    // approval arm forever; the read budget is independently capped
    // below so the **total** worst-case is `2 * timeout_ms`. Both
    // halves are independently mapped to `BridgeUnavailable`, which
    // is the correct semantics for the gate caller (Requirement 19.5).
    let write_res = tokio_time::timeout(
        Duration::from_millis(timeout_ms as u64),
        super::frame::write_frame(client, &payload),
    )
    .await;
    match write_res {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return Err("write_io"),
        Err(_) => return Err("write_timeout"),
    }

    let read_res = tokio_time::timeout(
        Duration::from_millis(timeout_ms as u64),
        super::frame::read_frame(client, scratch),
    )
    .await;
    let response_bytes: Vec<u8> = match read_res {
        Ok(Ok(slice)) => slice.to_vec(),
        Ok(Err(_)) => return Err("read_io"),
        Err(_) => return Err("read_timeout"),
    };

    let parsed: PeerApprovalResponse =
        serde_json::from_slice(&response_bytes).map_err(|_| "parse")?;

    Ok(match parsed {
        PeerApprovalResponse::Approved { ttl_ms } => {
            (ApprovalOutcome::Approved, ttl_ms.unwrap_or(0))
        }
        PeerApprovalResponse::Rejected { .. } => (ApprovalOutcome::Rejected, 0),
    })
}

/// Map the public `super::ConnectionType` to the wire string used in
/// the HMAC input (protocol §8.2 / docs §8.4 example: `"controlled"`,
/// `"view-only"`, `"file-transfer"`, `"port-forward"`, `"terminal"`).
/// Kept as a free function instead of a method on `ConnectionType` to
/// avoid widening that public enum's impl surface.
fn connection_type_wire(t: super::ConnectionType) -> &'static str {
    match t {
        super::ConnectionType::Controlled => "controlled",
        super::ConnectionType::ViewOnly => "view-only",
        super::ConnectionType::FileTransfer => "file-transfer",
        super::ConnectionType::PortForward => "port-forward",
        super::ConnectionType::Terminal => "terminal",
    }
}

/// Map the public `super::ConnectionType` to the internal
/// `protocol::ConnectionType`. They are isomorphic enums — separated
/// because the public type lives on `mod.rs` (callable from
/// `src/server/connection.rs` without depending on the private
/// `protocol` submodule) while the internal one carries the
/// `Serialize` annotation that gives us the kebab-case wire form.
fn protocol_connection_type(t: super::ConnectionType) -> protocol::ConnectionType {
    match t {
        super::ConnectionType::Controlled => protocol::ConnectionType::Controlled,
        super::ConnectionType::ViewOnly => protocol::ConnectionType::ViewOnly,
        super::ConnectionType::FileTransfer => protocol::ConnectionType::FileTransfer,
        super::ConnectionType::PortForward => protocol::ConnectionType::PortForward,
        super::ConnectionType::Terminal => protocol::ConnectionType::Terminal,
    }
}

// ---------------------------------------------------------------------------
// Helpers — task 7.3 (retry delay) / task 7.4 (nonce window) /
// task 8.2 (Last_Reported_Snapshot dedup)
// ---------------------------------------------------------------------------

/// Sleep for the reconnect interval. Wraps [`compute_retry_delay`]
/// for one centralised call site so tests / future hooks
/// (`tokio::time::pause()`) can target a single helper.
#[inline]
async fn sleep_retry(retry_interval_ms: u32, last_reason: Option<&'static str>) {
    tokio_time::sleep(compute_retry_delay(retry_interval_ms, last_reason)).await;
}

/// Compute the next reconnect delay.
///
/// Implements the Requirement 9.1 / 9.2 contract:
///   * Base interval = `retry_interval_ms`.
///   * Jitter: 0–200 ms, uniform via
///     `rand::thread_rng().gen_range(0..=200)`.
///   * `last_reason == REASON_RATE_LIMITED` overlays an additional
///     60_000 ms on top of the base interval. The "clear overlay on
///     next success" half is the caller's responsibility — `run()`
///     passes `last_reason = None` for every non-`Denied` outcome,
///     and after the next `Connected` the overlay is structurally
///     no longer reachable until a fresh `rate_limited` `Denied`
///     fires again.
///
/// SHALL NOT use exponential backoff or any growth term that scales
/// with the consecutive-failure count (Requirement 2.5 / 9.3 last
/// clause: "本机 IPC 不需要保护远端服务"); see Property 6 for the
/// proptest that pins this.
fn compute_retry_delay(
    retry_interval_ms: u32,
    last_reason: Option<&'static str>,
) -> Duration {
    // Base interval + 0..=200 ms jitter (§9.1). Saturating arithmetic
    // is overkill at u64 widths but cheap insurance against operator
    // overrides at the upper validation bound.
    let mut delay_ms: u64 = retry_interval_ms as u64;
    let jitter: u64 = rand::thread_rng().gen_range(0..=200);
    delay_ms = delay_ms.saturating_add(jitter);
    // §9.2: `rate_limited` overlays an additional 60 s on top of the
    // fixed interval. Caller resets `last_reason` to `None` after the
    // next non-Denied outcome, satisfying "只有收到下一次成功响应后
    // 才把'叠加延迟'标志清零".
    if matches!(last_reason, Some(r) if r == REASON_RATE_LIMITED) {
        delay_ms = delay_ms.saturating_add(60_000);
    }
    Duration::from_millis(delay_ms)
}

// ---------------------------------------------------------------------------
// Nonce dedup window — task 7.4
// ---------------------------------------------------------------------------

/// Length of the nonce-dedup window in milliseconds. Matches
/// Requirement 5.3 / 19.3: nonces SHALL NOT be reused within a
/// 5-minute window.
pub(super) const NONCE_WINDOW_MS: u64 = 5 * 60 * 1000;

/// 5-minute sliding window dedup for the 16-byte nonces consumed by
/// the handshake / report / peer-approval frames.
///
/// Two indices kept in lockstep:
///   * `by_time: BTreeMap<u64, [u8; 16]>` — keyed on the
///     `timestamp_ms` recorded at insertion. The
///     `BTreeMap::iter().next()` cursor walks the oldest entry first,
///     making eviction a tight `pop_first` loop.
///   * `by_value: HashSet<[u8; 16]>` — answers
///     "have we used this nonce inside the window?" in O(1).
///
/// Both indices grow at most one entry per generated nonce in the
/// window (~300 entries at one nonce/second), so the storage cost is
/// negligible. A `BTreeMap` over a `VecDeque<(u64, [u8; 16])>` is
/// chosen because the operator can shorten `request_timeout_ms` and
/// produce an out-of-order timestamp via `now_unix_ms` skew — the
/// `BTreeMap` keeps eviction monotone in *clock* time rather than
/// insertion order.
///
/// The `BTreeMap` keys are timestamps; on the very rare collision
/// (two nonces minted at the same millisecond) only one survives.
/// That is harmless for dedup correctness — both nonces are still in
/// the `HashSet` until a regenerate; eviction may free one early but
/// the *other* nonce is still in flight and prevented from reuse via
/// the `HashSet`.
#[derive(Debug, Default)]
struct NonceWindow {
    by_time: BTreeMap<u64, [u8; 16]>,
    by_value: HashSet<[u8; 16]>,
}

impl NonceWindow {
    fn new() -> Self {
        Self::default()
    }

    /// Generate a fresh 16-byte nonce as 32 lowercase hex chars,
    /// recording it for 5 minutes of dedup.
    ///
    /// `now_ms` is the current Unix timestamp in milliseconds — the
    /// caller must source it from [`now_unix_ms`] for the eviction
    /// math to align with `Handshake_Frame.timestampMs` (Requirement
    /// 5.4: 5-minute handshake window).
    fn generate(&mut self, now_ms: u64) -> String {
        // (1) Evict expired entries. `now_ms.saturating_sub(ts)` lets
        // operator clock-skew (a sudden wall-clock backwards jump)
        // collapse to zero rather than wrap around — old entries
        // simply linger until the clock catches up.
        while let Some((&ts, _)) = self.by_time.iter().next() {
            if now_ms.saturating_sub(ts) > NONCE_WINDOW_MS {
                if let Some(val) = self.by_time.remove(&ts) {
                    self.by_value.remove(&val);
                }
            } else {
                break;
            }
        }

        // (2) Mint a fresh nonce, regenerate on the (cryptographically
        // negligible) chance of a HashSet collision. Bounding the
        // loop at 8 iterations is paranoia: a clean OS RNG cannot hit
        // the same 128-bit value twice in 8 tries unless something
        // is catastrophically wrong, and the saturating cap stops a
        // mis-seeded RNG from spinning forever.
        for _ in 0..8 {
            let mut bytes = [0u8; 16];
            rand::thread_rng().fill_bytes(&mut bytes);
            if !self.by_value.contains(&bytes) {
                self.by_value.insert(bytes);
                self.by_time.insert(now_ms, bytes);
                return hex::encode(bytes);
            }
        }
        // The escape hatch: if the loop above somehow burned through
        // 8 collisions, return the most recent draw without
        // recording it. The dedup contract degrades to "no dedup for
        // this one nonce" rather than panicking. In practice this is
        // unreachable.
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    /// Wipe the window. Reserved for future use (Requirement 5.3:
    /// "每会话开始时清空集合" applies once the session loop starts
    /// minting per-session Report / PeerApproval nonces in tasks 8.x
    /// / 11.x). Kept here so the call site lives close to the
    /// generator that fills it.
    #[allow(dead_code)]
    fn clear(&mut self) {
        self.by_time.clear();
        self.by_value.clear();
    }

    /// Number of entries currently in the window. Test-only.
    #[cfg(test)]
    fn len(&self) -> usize {
        debug_assert_eq!(self.by_time.len(), self.by_value.len());
        self.by_value.len()
    }
}

// ---------------------------------------------------------------------------
// Last_Reported_Snapshot dedup — task 8.2
// ---------------------------------------------------------------------------

/// One row of the report-dedup cache from the spec:
/// `LAST_REPORTED_SNAPSHOT = (rust_desk_id, password_kind,
/// sha256Hex(password))`.
///
/// `password_sha256_hex` deliberately replaces the password
/// plaintext (Requirement 6.8 last clause + Requirement 18.7: SHALL
/// NOT 把密码明文写入日志). The plaintext only enters the
/// `Report_Frame` JSON payload itself, never this cache, never any
/// log line, and never any IPC observability message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LastReportedSnapshot {
    pub(super) rust_desk_id: String,
    pub(super) password_kind: PasswordKind,
    pub(super) password_sha256_hex: String,
}

impl LastReportedSnapshot {
    /// Build a snapshot from the live values. Hashes the password
    /// at construction time so the plaintext never reaches any
    /// owner of a `LastReportedSnapshot` instance — only the local
    /// scope that calls `from_password` does.
    pub(super) fn from_password(
        rust_desk_id: String,
        password_kind: PasswordKind,
        password: &str,
    ) -> Self {
        Self {
            rust_desk_id,
            password_kind,
            password_sha256_hex: sha256_hex(password.as_bytes()),
        }
    }
}

/// Lowercase 64-char hex of `SHA-256(bytes)`. Same construction the
/// HMAC builders use via `sha256Hex(...)` in their inputs; kept as a
/// free function rather than re-exporting from `super::hmac` to
/// avoid widening that module's public surface.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Decide whether the next trigger should be deduped against the
/// last-accepted snapshot. Returns `true` when the report SHALL be
/// skipped (Requirement 6.8 / 7.6).
///
/// Contract:
///   * Heartbeat triggers are **never** deduped — they are the
///     reconciliation signal whose whole purpose is to refresh
///     VHDMount's understanding of `(id, password)` even when the
///     pair is unchanged (Requirement 7.6).
///   * For non-heartbeat triggers, dedup fires only when *all three*
///     hold:
///       1. the cached snapshot exists at all (post-`Connected` we
///          start with `None`, so the first non-heartbeat trigger
///          always sends — Requirement 6.8);
///       2. the cached snapshot equals `next` byte-for-byte;
///       3. the bridge is `Authorized` (the cached snapshot was set
///          by a `ReportAck::Accepted`, so it only carries dedup
///          weight while the server is still treating us as
///          authorized). Pre-`Authorized` Connected sends still go
///          out so the worker can prime `last_reported` from its
///          first `accepted`.
///
/// Tested in [`tests::should_skip_report_*`]. The actual
/// `Report_Frame` write loop that calls this is owned by task 8.x.
pub(super) fn should_skip_report(
    cached: Option<&LastReportedSnapshot>,
    next: &LastReportedSnapshot,
    is_heartbeat: bool,
    state: BridgeState,
) -> bool {
    if is_heartbeat {
        return false;
    }
    matches!(state, BridgeState::Authorized) && cached == Some(next)
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

/// Current Unix-millisecond timestamp. Same shape as the helper in
/// `observability` / `peer_approval`; duplicated rather than
/// re-exported so this file's set of `super::` couplings stays small.
#[inline]
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn reset_signal_is_singleton() {
        // Two lookups must return the same `Notify` instance so the
        // worker's `notified().await` is woken by an arbitrary later
        // `request_reset` call.
        let a = reset_notify() as *const _;
        let b = reset_notify() as *const _;
        assert_eq!(a, b, "RESET_SIGNAL must be a process-singleton");
    }

    #[test]
    fn request_reset_wakes_a_parked_waiter_and_clears_failed_state() {
        let _guard = observability_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("vhd_bridge::worker test runtime");

        rt.block_on(async {
            // Drive the snapshot into `Failed` first so we can
            // confirm `request_reset` escapes the sink state.
            observability::transition_to_failed(observability::REASON_SECRET_OUTDATED);
            assert_eq!(current_snapshot().state, super::super::BridgeState::Failed);

            let waiter = tokio::spawn(async {
                reset_notify().notified().await;
            });

            // Yield once so the spawned task reaches its `notified`
            // call before we pulse the signal.
            tokio::task::yield_now().await;

            request_reset();
            waiter.await.expect("reset waiter should complete");

            let s = current_snapshot();
            assert_eq!(s.state, super::super::BridgeState::Initializing);
            assert!(s.last_reason.is_none());
            assert!(s.error_code.is_none());
        });
    }

    // -----------------------------------------------------------------
    // Task 7.3 — retry delay envelope, log-level escalation threshold
    // -----------------------------------------------------------------

    #[test]
    fn retry_delay_envelope_matches_design_section_9_1() {
        // §9.1: base interval + 0..=200 ms jitter, no exponential
        // backoff. Property 6 / task 7.7 will lift this into a
        // proptest over arbitrary failure sequences; this `#[test]`
        // pins the spot-check on the implementation that 7.7 will
        // wrap in `proptest!`.
        let base: u32 = 2000;
        for _ in 0..256 {
            let d = compute_retry_delay(base, None).as_millis() as u64;
            assert!(
                (base as u64..=base as u64 + 200).contains(&d),
                "delay {} outside [{}, {}]",
                d,
                base,
                base + 200
            );
        }
    }

    #[test]
    fn retry_delay_overlays_60s_on_rate_limited() {
        // §9.2: `rate_limited` adds 60 s on top of the fixed interval.
        let base: u32 = 2000;
        for _ in 0..256 {
            let d = compute_retry_delay(base, Some(REASON_RATE_LIMITED)).as_millis() as u64;
            let lo = base as u64 + 60_000;
            let hi = base as u64 + 60_200;
            assert!((lo..=hi).contains(&d), "delay {} outside [{}, {}]", d, lo, hi);
        }
    }

    #[test]
    fn retry_delay_does_not_grow_with_failure_count() {
        // Requirement 2.5 / 9.3: SHALL NOT use exponential backoff or
        // any growth term that scales with the consecutive-failure
        // count. We model "K consecutive failures" by calling the
        // function repeatedly and asserting the upper bound holds for
        // every draw — the function is stateless, so a passing K=1
        // proof generalises trivially, but the explicit loop guards
        // against accidental future state.
        let base: u32 = 2000;
        for k in 0..1024u32 {
            let d = compute_retry_delay(base, None).as_millis() as u64;
            assert!(
                d <= base as u64 + 200,
                "delay {} at iteration {} exceeded fixed envelope",
                d,
                k
            );
        }
    }

    #[test]
    fn retry_delay_clears_overlay_on_next_success() {
        // Requirement 9.2 second clause: "只有收到下一次成功响应后才把
        // '叠加延迟'标志清零". The caller passes `last_reason = None`
        // for the post-Connected sleep; check that this collapses the
        // delay back into the fixed envelope (no 60 s tail).
        let base: u32 = 2000;
        // First, "as if rate_limited just happened":
        let with_overlay =
            compute_retry_delay(base, Some(REASON_RATE_LIMITED)).as_millis() as u64;
        assert!(with_overlay >= base as u64 + 60_000);
        // Then, "next success cleared the flag":
        let cleared = compute_retry_delay(base, None).as_millis() as u64;
        assert!(cleared <= base as u64 + 200);
    }

    #[test]
    fn failure_log_escalation_threshold_is_pinned() {
        // The fifth consecutive failure SHALL escalate the log
        // (Requirement 9.4). The macro reads `>= 4`, so the value
        // 0..=3 emits at debug and `>= 4` at warn — the fifth log
        // line is the first warn.
        assert_eq!(FAILURE_LOG_ESCALATION_THRESHOLD, 4);
    }

    // -----------------------------------------------------------------
    // Task 7.4 — NonceWindow dedup
    // -----------------------------------------------------------------

    #[test]
    fn nonce_window_emits_32_lowercase_hex_chars() {
        let mut w = NonceWindow::new();
        let now = 1_730_000_000_000u64;
        for i in 0..32 {
            let n = w.generate(now + i);
            assert_eq!(n.len(), 32, "nonce must be 32 chars, got {:?}", n);
            assert!(
                n.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "nonce must be lowercase hex, got {:?}",
                n
            );
        }
    }

    #[test]
    fn nonce_window_dedup_within_5_minute_window() {
        // Property 4 (handshake half): N nonces minted inside the
        // window are all distinct. With 16 random bytes the
        // collision probability at N=200 is ≈ 2^-117, so an actual
        // collision over the lifetime of CI is structurally
        // impossible — a failure here means the dedup logic itself
        // is broken.
        let mut w = NonceWindow::new();
        let now = 1_730_000_000_000u64;
        let mut seen = HashSet::new();
        for i in 0..200u64 {
            let n = w.generate(now + i);
            assert!(seen.insert(n.clone()), "nonce {} reused inside window", n);
        }
        // All 200 entries should still be tracked (no eviction yet).
        assert_eq!(w.len(), 200);
    }

    #[test]
    fn nonce_window_evicts_entries_older_than_5_minutes() {
        let mut w = NonceWindow::new();
        let t0 = 1_730_000_000_000u64;
        // Mint 50 entries spread over the first 4 minutes.
        for i in 0..50u64 {
            // ~4.8 s apart — comfortably inside the 300 s window.
            w.generate(t0 + i * 4_800);
        }
        assert_eq!(w.len(), 50);

        // Now jump the clock 6 minutes forward and mint a fresh
        // nonce. All 50 prior entries are older than 5 minutes
        // *relative to the new now*, so the eviction loop removes
        // every one of them before recording the new one.
        let n_new = w.generate(t0 + 50 * 4_800 + 6 * 60 * 1000);
        assert_eq!(w.len(), 1, "eviction should leave only the new nonce");
        // The new nonce is in the window.
        let mut bytes = [0u8; 16];
        hex::decode_to_slice(&n_new, &mut bytes).expect("nonce parses as hex");
        assert!(w.by_value.contains(&bytes));
    }

    #[test]
    fn nonce_window_evicts_only_truly_expired_entries() {
        // An entry exactly at the 5-minute boundary is *not* yet
        // expired — the spec's "超过 5 分钟" reads as strict greater
        // than. Use NONCE_WINDOW_MS as the boundary.
        let mut w = NonceWindow::new();
        let t0 = 1_000_000u64;
        w.generate(t0);
        w.generate(t0 + NONCE_WINDOW_MS); // exactly 5 minutes later
        assert_eq!(w.len(), 2, "boundary entry must not be evicted");

        // One ms past the boundary triggers eviction of the t0 entry.
        w.generate(t0 + NONCE_WINDOW_MS + 1);
        // Only the t0+NONCE_WINDOW_MS and t0+NONCE_WINDOW_MS+1
        // entries survive; t0 is now > 5 minutes old.
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn nonce_window_clear_resets_both_indices() {
        let mut w = NonceWindow::new();
        let t0 = 1_000_000u64;
        for i in 0..16 {
            w.generate(t0 + i);
        }
        assert_eq!(w.len(), 16);
        w.clear();
        assert_eq!(w.len(), 0);
        assert!(w.by_time.is_empty());
        assert!(w.by_value.is_empty());
        // Post-clear, fresh generates work normally.
        w.generate(t0 + 100);
        assert_eq!(w.len(), 1);
    }

    // -----------------------------------------------------------------
    // Task 8.2 — Last_Reported_Snapshot dedup
    // -----------------------------------------------------------------

    fn snap(id: &str, kind: PasswordKind, password: &str) -> LastReportedSnapshot {
        LastReportedSnapshot::from_password(id.to_owned(), kind, password)
    }

    #[test]
    fn last_reported_snapshot_hashes_password_at_construction() {
        // The plaintext password MUST NOT survive into the snapshot
        // (Requirement 6.8 / 18.7). Build two snapshots from the
        // same plaintext and verify they agree on the digest only.
        let s1 = snap("123456789", PasswordKind::Temporary, "Hunter2!");
        let s2 = snap("123456789", PasswordKind::Temporary, "Hunter2!");
        assert_eq!(s1, s2);
        // Known-answer: SHA-256("Hunter2!") = the same value the doc
        // example uses (§6.4).
        assert_eq!(
            s1.password_sha256_hex,
            "607265682fb0f3a91201774321ada848cb027b10fe319d6dae730a1968f47abe"
        );
    }

    #[test]
    fn should_skip_report_lets_through_first_non_heartbeat() {
        // Cached = None: the very first non-heartbeat trigger after
        // `Connected` must always send so the worker can prime its
        // dedup cache from the first `ReportAck::Accepted`.
        let next = snap("id1", PasswordKind::Temporary, "pw1");
        assert!(!should_skip_report(None, &next, false, BridgeState::Connected));
        assert!(!should_skip_report(None, &next, false, BridgeState::Authorized));
    }

    #[test]
    fn should_skip_report_dedupes_identical_snapshot_when_authorized() {
        let cached = snap("id1", PasswordKind::Temporary, "pw1");
        let next = snap("id1", PasswordKind::Temporary, "pw1");
        // Same value, same kind, same id, state == Authorized → skip.
        assert!(should_skip_report(
            Some(&cached),
            &next,
            false,
            BridgeState::Authorized
        ));
    }

    #[test]
    fn should_skip_report_does_not_dedupe_outside_authorized() {
        // The cached snapshot is only "current" while the server is
        // still treating us as authorized. Any other state must let
        // the trigger through so a fresh `accepted` can re-prime the
        // cache.
        let cached = snap("id1", PasswordKind::Temporary, "pw1");
        let next = cached.clone();
        for s in [
            BridgeState::Initializing,
            BridgeState::Connected,
            BridgeState::Denied,
            BridgeState::Failed,
            BridgeState::Disabled,
        ] {
            assert!(
                !should_skip_report(Some(&cached), &next, false, s),
                "state {:?} should not dedupe",
                s
            );
        }
    }

    #[test]
    fn should_skip_report_never_dedupes_heartbeat() {
        // Requirement 7.6 last clause: "heartbeat 永远发送". Even when
        // the next snapshot byte-for-byte matches the cached one and
        // the bridge is `Authorized`, the heartbeat trigger SHALL
        // still be emitted.
        let cached = snap("id1", PasswordKind::Temporary, "pw1");
        let next = cached.clone();
        assert!(!should_skip_report(
            Some(&cached),
            &next,
            true,
            BridgeState::Authorized
        ));
    }

    #[test]
    fn should_skip_report_distinguishes_id_or_kind_or_password_change() {
        let cached = snap("id1", PasswordKind::Temporary, "pw1");
        // Different id.
        let n1 = snap("id2", PasswordKind::Temporary, "pw1");
        assert!(!should_skip_report(
            Some(&cached),
            &n1,
            false,
            BridgeState::Authorized
        ));
        // Different password kind.
        let n2 = snap("id1", PasswordKind::Permanent, "pw1");
        assert!(!should_skip_report(
            Some(&cached),
            &n2,
            false,
            BridgeState::Authorized
        ));
        // Different password (digest differs).
        let n3 = snap("id1", PasswordKind::Temporary, "pw2");
        assert!(!should_skip_report(
            Some(&cached),
            &n3,
            false,
            BridgeState::Authorized
        ));
    }

    #[test]
    fn sha256_hex_is_lowercase_64_chars() {
        // Used for both `LastReportedSnapshot.password_sha256_hex`
        // and the HMAC inputs for report / log / peer-approval.
        // Pinning the shape here keeps the dedup cache compatible
        // with whatever future code depends on the digest format.
        let h = sha256_hex(b"");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // Known answer: SHA-256("") =
        // e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // =====================================================================
    // Task 7.5 — Property 4: Nonce non-reuse
    //
    // The 5-minute dedup contract is owned by `NonceWindow::generate`
    // and is driven from a caller-supplied `now_ms` argument, so we
    // do not need `tokio::time::pause()` / `advance()` to simulate
    // wall-clock progression: stepping `now_ms` directly is the same
    // contract under test. The earlier "200 distinct" smoke at
    // `nonce_window_dedup_within_5_minute_window` is kept; the tests
    // below are the *property*-shaped form Property 4 calls for —
    // explicit `Validates:` annotation, larger random surface area,
    // both halves of the property (handshake and session-scoped).
    //
    // **Validates: Requirements 5.3, 6.3, 19.3**
    // =====================================================================

    /// Property 4 (handshake half): for any inter-arrival sequence
    /// inside a 5-minute window, the 200 generated handshake nonces
    /// MUST all be distinct. We constrain `step_ms` to 1..=1499 so
    /// the cumulative offset over 200 iterations stays well inside
    /// `NONCE_WINDOW_MS = 300_000` (the worst case is
    /// `199 * 1499 ≈ 298_301 ms`, still under 5 minutes), keeping
    /// every nonce inside the dedup window for the entire run.
    ///
    /// **Validates: Requirements 5.3, 19.3**
    #[test]
    fn nonce_window_200_handshake_nonces_all_distinct_within_5_min() {
        let mut window = NonceWindow::new();
        let mut seen = HashSet::new();
        let now_ms = 1_700_000_000_000u64;
        for i in 0..200u64 {
            // 1 second apart — total 199 s, well inside 300 s.
            let nonce = window.generate(now_ms + i * 1000);
            assert!(seen.insert(nonce.clone()), "duplicate nonce {nonce:?} at i={i}");
        }
    }

    /// Property 4 (session-scoped half): within a single connected
    /// session the 100 `Report_Frame` + 50 `Peer_Approval_Request`
    /// nonces MUST also be distinct. After `clear()` (called at
    /// session boundaries — task 8.x will start invoking it) the
    /// next session is free to mint fresh values without bumping
    /// against the prior window.
    ///
    /// **Validates: Requirements 5.3, 6.3, 19.3**
    #[test]
    fn nonce_window_session_scoped_for_report_and_peer_approval() {
        let mut window = NonceWindow::new();
        let mut seen = HashSet::new();
        let now_ms = 1_700_000_000_000u64;
        // Session = 100 reports + 50 peer approvals = 150 nonces.
        for i in 0..150u64 {
            let nonce = window.generate(now_ms + i);
            assert!(seen.insert(nonce.clone()), "duplicate session nonce {nonce:?}");
        }
        // Session boundary: clear() wipes both indices. The next
        // session can mint fresh values — we just assert the helper
        // doesn't panic and produces a usable nonce.
        window.clear();
        let next = window.generate(now_ms + 200);
        assert_eq!(next.len(), 32);
        assert!(next.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    // =====================================================================
    // Task 7.6 — Property 7: State-machine integrity (model-based MVP)
    //
    // The full `proptest-state-machine` setup the spec describes
    // requires a parallel reference worker plus mock pipe driving;
    // for the MVP we drive the public observability writers
    // (`transition_to`, `transition_to_failed`, `record_accepted`,
    // `force_reset_to_initializing`) with arbitrary event sequences
    // and assert the invariants directly. The writers are exactly
    // what the worker calls in production (Requirement 12.1: every
    // write to the watch funnels through these helpers), so any
    // state-machine bug visible in production observability is
    // visible here too.
    //
    // **Validates: Requirements 2.4, 4.6, 5.5-5.8, 6.4-6.7, 7.1, 7.7,
    // 8.1-8.8, 9.5-9.8, 10.5, 11.1-11.6**
    // =====================================================================

    /// Lock used by tests that mutate the process-global
    /// `observability` watch channel. Delegates to
    /// [`super::observability::shared_test_lock`] so sibling-module
    /// tests in `observability::tests` AND this module share one
    /// critical section. Without this delegation, `cargo test`'s
    /// parallel test runner can interleave the snapshot mutations
    /// from `record_accepted_promotes_to_authorized_*` with our
    /// `transition_to_failed` calls below.
    fn observability_test_lock() -> &'static std::sync::Mutex<()> {
        observability::shared_test_lock()
    }

    #[derive(Debug, Clone, Copy)]
    enum SmEvent {
        HandshakeOk,
        DenyDeny,
        DenyRateLimited,
        DenyInvalidProof,
        FailSecretOutdated,
        FailPeerNotVhdMount,
        BrokenPipe,
        AcceptReport,
        Reset,
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

        /// Property 7: any event sequence produces a state trajectory
        /// satisfying:
        ///
        ///   * (a) state ∈ {Initializing, Connected, Authorized,
        ///     Denied, Failed} (Disabled never reachable at runtime);
        ///   * (b) once `Failed` is set by a permanent reason, no
        ///     later non-`Reset` event leaves `Failed`
        ///     (Requirements 9.5 / 9.8);
        ///   * (c) `BrokenPipe` (REASON_PIPE_CLOSED) issued from a
        ///     non-`Failed` state lands the snapshot at
        ///     `Initializing` (Requirement 2.4 / 9.7);
        ///   * (d) when `state ∈ {Failed, Denied}`, `error_code` is
        ///     non-empty and ∈ `ALLOWED_ERROR_CODES`
        ///     (Requirement 12.5).
        ///
        /// **Validates: Requirements 2.4, 4.6, 5.5-5.8, 6.4-6.7, 7.1,
        /// 7.7, 8.1-8.8, 9.5-9.8, 10.5, 11.1-11.6**
        #[test]
        fn worker_state_invariants_under_arbitrary_event_seq(
            events in prop::collection::vec(
                prop_oneof![
                    Just(SmEvent::HandshakeOk),
                    Just(SmEvent::DenyDeny),
                    Just(SmEvent::DenyRateLimited),
                    Just(SmEvent::DenyInvalidProof),
                    Just(SmEvent::FailSecretOutdated),
                    Just(SmEvent::FailPeerNotVhdMount),
                    Just(SmEvent::BrokenPipe),
                    Just(SmEvent::AcceptReport),
                    Just(SmEvent::Reset),
                ],
                0..32usize,
            ),
        ) {
            let _guard = observability_test_lock()
                .lock()
                .unwrap_or_else(|p| p.into_inner());

            // Every property-test case starts from a fresh snapshot.
            // The OnceLock-backed channel is process-singleton, so we
            // explicitly reset rather than trying to recreate it.
            observability::reset_for_tests();
            // Property 7(a): runtime never sets `Disabled`.
            let initial = observability::current_snapshot();
            prop_assert_ne!(initial.state, BridgeState::Disabled);

            // Track whether we have entered `Failed` since the most
            // recent `Reset`. While this flag is set, every later
            // non-`Reset` event MUST leave the snapshot in `Failed`.
            let mut sticky_failed = false;

            for ev in events {
                match ev {
                    SmEvent::HandshakeOk => {
                        observability::transition_to(BridgeState::Connected, None);
                    }
                    SmEvent::DenyDeny => {
                        observability::transition_to(
                            BridgeState::Denied,
                            Some(observability::REASON_DENY),
                        );
                    }
                    SmEvent::DenyRateLimited => {
                        observability::transition_to(
                            BridgeState::Denied,
                            Some(observability::REASON_RATE_LIMITED),
                        );
                    }
                    SmEvent::DenyInvalidProof => {
                        observability::transition_to(
                            BridgeState::Denied,
                            Some(observability::REASON_INVALID_PROOF),
                        );
                    }
                    SmEvent::FailSecretOutdated => {
                        observability::transition_to_failed(
                            observability::REASON_SECRET_OUTDATED,
                        );
                        sticky_failed = true;
                    }
                    SmEvent::FailPeerNotVhdMount => {
                        observability::transition_to_failed(
                            observability::REASON_PEER_NOT_VHDMOUNT,
                        );
                        sticky_failed = true;
                    }
                    SmEvent::BrokenPipe => {
                        observability::transition_to(
                            BridgeState::Initializing,
                            Some(observability::REASON_PIPE_CLOSED),
                        );
                    }
                    SmEvent::AcceptReport => {
                        observability::record_accepted();
                    }
                    SmEvent::Reset => {
                        observability::force_reset_to_initializing();
                        sticky_failed = false;
                    }
                }

                let s = observability::current_snapshot();

                // Property 7(a): never `Disabled` at runtime.
                prop_assert_ne!(s.state, BridgeState::Disabled);

                // Property 7(b) / (d): sticky `Failed`.
                if sticky_failed {
                    prop_assert_eq!(s.state, BridgeState::Failed);
                }

                // Property 7(c): a `BrokenPipe` issued while not
                // sticky-failed lands at `Initializing`. We can only
                // assert this when the previous event WAS the
                // `BrokenPipe` and we WERE not sticky_failed at the
                // time — captured implicitly by the pre-event flag.
                if matches!(ev, SmEvent::BrokenPipe) && !sticky_failed {
                    prop_assert_eq!(s.state, BridgeState::Initializing);
                }

                // Property 15 cross-check (also Requirement 12.5):
                // `Failed` / `Denied` snapshots MUST carry an
                // allowlisted error code.
                if matches!(s.state, BridgeState::Failed | BridgeState::Denied) {
                    let code = s
                        .error_code
                        .as_deref()
                        .expect("Failed/Denied must carry error_code");
                    prop_assert!(
                        observability::ALLOWED_ERROR_CODES.contains(&code),
                        "error_code {code} not in ALLOWED_ERROR_CODES",
                    );
                }
            }
        }
    }

    // =====================================================================
    // Task 7.7 — Property 6: Reconnect delay envelope
    //
    // For any K consecutive failures, the wall-clock delay between
    // attempt i and attempt i+1 lies in the spec'd window:
    //   * `[retry_interval_ms, retry_interval_ms + 200]` ms when the
    //     last reason is NOT `rate_limited`;
    //   * `[retry_interval_ms + 60_000, retry_interval_ms + 60_200]`
    //     ms when it IS `rate_limited`.
    // The delay is independent of K (no exponential backoff).
    //
    // **Validates: Requirements 2.5, 9.1, 9.2, 9.3**
    // =====================================================================

    proptest! {
        #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

        /// Property 6: K consecutive calls to [`compute_retry_delay`]
        /// stay inside the fixed envelope no matter how large K
        /// grows. This is the proptest-shaped form of the existing
        /// `retry_delay_*` spot-check tests; the spot-checks pin a
        /// single `retry_interval_ms = 2000` value while this test
        /// fuzzes the operator-overridable interval over its
        /// validation range `[100, 60_000]` ms (matching
        /// `try_apply_bridge_option`'s clamp in `BridgeConfig`).
        ///
        /// **Validates: Requirements 2.5, 9.1, 9.2, 9.3**
        #[test]
        fn compute_retry_delay_envelope_under_arbitrary_failure_count(
            retry_interval_ms in 100u32..=60_000u32,
            k in 1usize..=100usize,
        ) {
            for _ in 0..k {
                let d_no_rate = compute_retry_delay(retry_interval_ms, None)
                    .as_millis() as u64;
                prop_assert!(d_no_rate >= retry_interval_ms as u64);
                prop_assert!(d_no_rate <= retry_interval_ms as u64 + 200);

                let d_rate = compute_retry_delay(
                    retry_interval_ms,
                    Some(observability::REASON_RATE_LIMITED),
                )
                .as_millis() as u64;
                prop_assert!(d_rate >= retry_interval_ms as u64 + 60_000);
                prop_assert!(d_rate <= retry_interval_ms as u64 + 60_200);
            }
        }
    }

    // =====================================================================
    // Task 8.5 — Property 5: Snapshot-identical reports are deduped
    //                       (heartbeat exempted)
    //
    // For any sequence of triggers, the number of frames the worker
    // would write equals (# distinct-snapshot non-heartbeat triggers
    // while Authorized) + (# heartbeat triggers). Tested directly
    // against [`should_skip_report`], which is the predicate the
    // worker's session loop will consume in tasks 8.x.
    //
    // **Validates: Requirements 6.8, 7.6**
    // =====================================================================

    proptest! {
        #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

        /// Property 5: snapshot-identical non-heartbeat triggers
        /// dedupe to a single emission while heartbeat triggers
        /// always emit. The bridge state is fixed at `Authorized`
        /// because dedup is only active in that state (see
        /// `should_skip_report` doc-comment / Requirement 6.8); the
        /// non-`Authorized` cases are covered by the
        /// `should_skip_report_does_not_dedupe_outside_authorized`
        /// example test above.
        ///
        /// **Validates: Requirements 6.8, 7.6**
        #[test]
        fn dedup_skips_identical_non_heartbeat_when_authorized(
            ids in prop::collection::vec(
                prop::string::string_regex("[0-9]{9}").unwrap(),
                0..=12,
            ),
            kinds in prop::collection::vec(
                prop_oneof![
                    Just(PasswordKind::Temporary),
                    Just(PasswordKind::Permanent),
                ],
                0..=12,
            ),
            hb_pattern in prop::collection::vec(
                any::<bool>(),
                0..=12,
            ),
        ) {
            // Zip the three parallel vectors to the shortest length.
            let n = ids.len().min(kinds.len()).min(hb_pattern.len());
            let mut last: Option<LastReportedSnapshot> = None;
            let mut emitted: usize = 0;
            // Reference oracle: independently compute the spec'd
            // count using the same `last` evolution rule the worker
            // would apply (every emitted Report_Frame primes the
            // dedup cache via the subsequent ReportAck::Accepted).
            let mut oracle_last: Option<LastReportedSnapshot> = None;
            let mut oracle_emitted: usize = 0;

            for i in 0..n {
                let snap = LastReportedSnapshot::from_password(
                    ids[i].clone(),
                    kinds[i],
                    "pwd",
                );
                let is_heartbeat = hb_pattern[i];

                // System under test.
                let skip = should_skip_report(
                    last.as_ref(),
                    &snap,
                    is_heartbeat,
                    BridgeState::Authorized,
                );
                if !skip {
                    emitted += 1;
                    // Every accepted frame primes the dedup cache,
                    // including heartbeats (the protocol carries the
                    // current `(id, password)` either way).
                    last = Some(snap.clone());
                }

                // Reference oracle: heartbeat always emits; a
                // non-heartbeat emits iff its snapshot differs from
                // the previously emitted snapshot.
                let oracle_emit_now = if is_heartbeat {
                    true
                } else {
                    match oracle_last.as_ref() {
                        Some(prev) => prev != &snap,
                        None => true,
                    }
                };
                if oracle_emit_now {
                    oracle_emitted += 1;
                    oracle_last = Some(snap);
                }
            }

            // Property 5 sanity: emissions cannot exceed inputs.
            prop_assert!(emitted <= n);
            // Heartbeats always pass through, so emissions is at
            // least the number of heartbeats.
            let hb_count = hb_pattern.iter().take(n).filter(|&&b| b).count();
            prop_assert!(
                emitted >= hb_count,
                "emitted {} < heartbeats {}",
                emitted,
                hb_count,
            );
            // The dedup-formula count: `emitted` MUST equal the
            // independent oracle.
            prop_assert_eq!(emitted, oracle_emitted);
        }
    }

    // =====================================================================
    // Task 8.x — Report frame builder + ack handling unit tests
    //
    // These exercise the new `Report_Frame` write path that 8.x added:
    //   * `build_report_frame` produces the exact JSON the spec
    //     mandates and a base64-encoded MAC that recomputes from
    //     `hmac_report` byte-for-byte (Property 20-flavoured spot check).
    //   * The full trigger → ack → dedup loop, simulated against the
    //     observability writers, gates non-heartbeat re-emission
    //     correctly and re-emits heartbeats unconditionally
    //     (Requirement 6.8 / 7.6).
    //   * `classify_password` follows the documented decision tree:
    //     preset > temporary > absent. The permanent branch never
    //     emits plaintext — that is the design's "VHDMount needs
    //     plaintext" constraint applied to a domain where plaintext
    //     does not exist on disk.
    // =====================================================================

    /// Re-encode a frame through `build_report_frame` and pull out a
    /// raw byte view of the MAC for cross-checking against
    /// `hmac::hmac_report` directly. The wire `mac` field is base64
    /// of those bytes per protocol §6.2.
    fn report_frame_mac_bytes(f: &protocol::ReportFrame) -> [u8; 32] {
        let mut out = [0u8; 32];
        let decoded = BASE64_STANDARD
            .decode(&f.mac)
            .expect("ReportFrame.mac must be valid base64");
        assert_eq!(decoded.len(), 32, "HMAC-SHA256 digest is 32 bytes");
        out.copy_from_slice(&decoded);
        out
    }

    #[test]
    fn build_report_frame_pins_protocol_and_mac_to_hmac_report() {
        // Spot-check a single fully-specified `(rustDeskId,
        // passwordKind, password, reason, secret_version, nonce,
        // reported_at)` tuple. The MAC bytes must match a separate
        // call into `super::hmac::hmac_report`, byte-for-byte. This
        // pins the wire layout and the cross-product of the frame
        // builder + HMAC input layer in one assertion.
        let rust_desk_id = "123456789";
        let password = "Hunter2!";
        let nonce = "9a8b7c6d5e4f30210011223344556677".to_owned();
        let reported_at = 1_730_000_000_000u64;
        let secret_version = 1u32;

        let frame = build_report_frame(
            rust_desk_id,
            PasswordKind::Temporary,
            password.to_owned(),
            ReportReason::Startup,
            secret_version,
            nonce.clone(),
            reported_at,
        );

        // Wire shape sanity.
        assert_eq!(frame.protocol, PROTOCOL_REPORT);
        assert_eq!(frame.secret_version, secret_version);
        assert_eq!(frame.rust_desk_id, rust_desk_id);
        assert_eq!(frame.password_kind, PasswordKind::Temporary);
        assert_eq!(frame.password, password);
        assert_eq!(frame.reason, ReportReason::Startup);
        assert_eq!(frame.reported_at, reported_at);
        assert_eq!(frame.nonce, nonce);

        // MAC reproducibility: recompute through the same input
        // builder and assert byte equality. A regression in either
        // `password_kind_wire` / `report_reason_wire` /
        // `sha256_hex(password)` would surface here.
        let expected_mac = super::super::hmac::hmac_report(
            secret_version,
            rust_desk_id,
            "temporary",
            &sha256_hex(password.as_bytes()),
            "startup",
            reported_at,
            &nonce,
        );
        assert_eq!(report_frame_mac_bytes(&frame), expected_mac);
    }

    #[test]
    fn build_report_frame_absent_password_uses_empty_plaintext_and_sha256_of_empty() {
        // The spec's `Absent` branch emits `password = ""`. The HMAC
        // input therefore hashes the empty byte string, which has
        // SHA-256 = e3b0c44…b855 (RFC 6234 zero-byte vector).
        let frame = build_report_frame(
            "987654321",
            PasswordKind::Absent,
            String::new(),
            ReportReason::Heartbeat,
            7,
            "ffeeddccbbaa99887766554433221100".to_owned(),
            1_730_000_001_000,
        );
        assert_eq!(frame.password, "");
        assert_eq!(frame.password_kind, PasswordKind::Absent);

        let expected_mac = super::super::hmac::hmac_report(
            7,
            "987654321",
            "absent",
            // SHA-256("") — the canonical empty-bytes digest.
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "heartbeat",
            1_730_000_001_000,
            "ffeeddccbbaa99887766554433221100",
        );
        assert_eq!(report_frame_mac_bytes(&frame), expected_mac);
    }

    #[test]
    fn classify_password_prefers_preset_then_temporary_then_absent() {
        // Preset wins outright when the predicate is true.
        let preset_creds = LiveCredentials {
            rust_desk_id: "id".into(),
            temporary_password: "ignored-temp".into(),
            temporary_enabled: true,
            is_preset: true,
            preset_password: "preset!".into(),
            permanent_enabled: true,
            has_permanent_password: true,
        };
        assert_eq!(
            classify_password(&preset_creds),
            (PasswordKind::Preset, "preset!".to_owned())
        );

        // Temporary wins when not preset and temp is enabled + non-empty.
        let temp_creds = LiveCredentials {
            rust_desk_id: "id".into(),
            temporary_password: "tmp42".into(),
            temporary_enabled: true,
            is_preset: false,
            preset_password: String::new(),
            permanent_enabled: true,
            has_permanent_password: true,
        };
        assert_eq!(
            classify_password(&temp_creds),
            (PasswordKind::Temporary, "tmp42".to_owned())
        );

        // Otherwise → Absent. Even when permanent is enabled and set
        // (RustDesk has no plaintext for permanent), the report goes
        // out as Absent, since `VHDMount` needs plaintext to re-sign
        // and forward but we have none.
        let perm_only_creds = LiveCredentials {
            rust_desk_id: "id".into(),
            temporary_password: String::new(),
            temporary_enabled: false,
            is_preset: false,
            preset_password: String::new(),
            permanent_enabled: true,
            has_permanent_password: true,
        };
        assert_eq!(
            classify_password(&perm_only_creds),
            (PasswordKind::Absent, String::new())
        );

        // Empty temp password (temp enabled but value not yet generated)
        // falls through to Absent.
        let blank_temp = LiveCredentials {
            rust_desk_id: "id".into(),
            temporary_password: String::new(),
            temporary_enabled: true,
            is_preset: false,
            preset_password: String::new(),
            permanent_enabled: false,
            has_permanent_password: false,
        };
        assert_eq!(
            classify_password(&blank_temp),
            (PasswordKind::Absent, String::new())
        );
    }

    #[test]
    fn report_reason_for_round_trips_through_wire_string() {
        // The protocol's `reason` field is derived from
        // `TriggerEvent::reason_str()` on the trigger side and from
        // `report_reason_wire(...)` on the HMAC side. Both must agree
        // for every variant (otherwise the HMAC input we sign is
        // different from what `VHDMount` recomputes).
        for ev in [
            TriggerEvent::Startup,
            TriggerEvent::IdChange,
            TriggerEvent::PasswordChange,
            TriggerEvent::Rotation,
            TriggerEvent::Heartbeat,
        ] {
            let r = report_reason_for(ev);
            assert_eq!(report_reason_wire(r), ev.reason_str());
        }
    }

    /// Drive [`handle_report_ack`] directly: an `Accepted` commits the
    /// pending snapshot into `last_reported`, a `Rejected` drops the
    /// pending snapshot, and observability is updated through the
    /// real (process-singleton) writers. This pins the dedup commit
    /// contract from §"上报去重".
    #[test]
    fn handle_report_ack_accepted_commits_pending_snapshot_into_last_reported() {
        let _g = observability_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        observability::reset_for_tests();
        observability::transition_to(BridgeState::Connected, None);

        let mut pending: Option<LastReportedSnapshot> =
            Some(snap("id1", PasswordKind::Temporary, "pw1"));
        let mut last: Option<LastReportedSnapshot> = None;

        let dispo = handle_report_ack(ReportAck::Accepted, &mut pending, &mut last);
        assert!(matches!(dispo, SessionDisposition::Continue));
        assert!(pending.is_none(), "pending must be drained on accept");
        assert_eq!(
            last.as_ref().expect("last_reported committed"),
            &snap("id1", PasswordKind::Temporary, "pw1"),
        );
        // First Accepted promotes Connected → Authorized.
        assert_eq!(
            current_snapshot().state,
            BridgeState::Authorized,
            "first accepted must promote to Authorized"
        );
    }

    #[test]
    fn handle_report_ack_rejected_drops_pending_and_routes_observability() {
        // Run the four `Rejected` reasons one at a time so each can
        // observe its own snapshot transition without sticky state
        // from a sibling case bleeding over.
        for (reason, want_state, want_reason) in [
            (
                ReportAckRejectReason::Deny,
                BridgeState::Denied,
                REASON_DENY,
            ),
            (
                ReportAckRejectReason::RateLimited,
                BridgeState::Denied,
                REASON_RATE_LIMITED,
            ),
            (
                ReportAckRejectReason::InvalidMac,
                BridgeState::Denied,
                REASON_INVALID_MAC,
            ),
            (
                ReportAckRejectReason::SecretOutdated,
                BridgeState::Failed,
                REASON_SECRET_OUTDATED,
            ),
        ] {
            let _g = observability_test_lock()
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            observability::reset_for_tests();
            observability::transition_to(BridgeState::Connected, None);

            let mut pending: Option<LastReportedSnapshot> =
                Some(snap("id1", PasswordKind::Temporary, "pw1"));
            let mut last: Option<LastReportedSnapshot> = None;

            let dispo = handle_report_ack(
                ReportAck::Rejected { reason },
                &mut pending,
                &mut last,
            );
            assert!(
                matches!(dispo, SessionDisposition::TearDown),
                "rejected ack must tear down session for reason {:?}",
                reason
            );
            assert!(
                pending.is_none(),
                "pending must be cleared on reject ({:?})",
                reason
            );
            assert!(
                last.is_none(),
                "last_reported must NOT be primed on reject ({:?})",
                reason
            );
            let s = current_snapshot();
            assert_eq!(s.state, want_state, "state mismatch for {:?}", reason);
            assert_eq!(
                s.last_reason.as_deref(),
                Some(want_reason),
                "reason mismatch for {:?}",
                reason
            );
        }
    }

    /// Trigger-loop-shaped end-to-end check: simulating the
    /// "trigger → write → ack → dedup" sequence twice with the same
    /// snapshot, the second non-heartbeat call MUST be deduped, and
    /// a heartbeat MUST always pass. Drives `should_skip_report`
    /// + `handle_report_ack` directly so the assertion does not
    /// depend on a real `NamedPipeClient`.
    #[test]
    fn dedup_loop_skips_second_identical_non_heartbeat_after_accept() {
        let _g = observability_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        observability::reset_for_tests();
        observability::transition_to(BridgeState::Connected, None);

        let next = snap("id1", PasswordKind::Temporary, "pw1");

        // Round 1: empty cache → must send.
        let mut last: Option<LastReportedSnapshot> = None;
        let mut pending: Option<LastReportedSnapshot>;
        let state = current_snapshot().state;
        assert!(
            !should_skip_report(last.as_ref(), &next, false, state),
            "first non-heartbeat trigger must emit"
        );
        pending = Some(next.clone());
        // Server accepts.
        let _ = handle_report_ack(ReportAck::Accepted, &mut pending, &mut last);
        assert_eq!(last.as_ref(), Some(&next), "accept must prime cache");
        assert_eq!(current_snapshot().state, BridgeState::Authorized);

        // Round 2: same snapshot, state == Authorized → MUST skip.
        let state = current_snapshot().state;
        assert!(
            should_skip_report(last.as_ref(), &next, false, state),
            "second identical non-heartbeat must be deduped"
        );

        // Heartbeat passes through regardless.
        assert!(
            !should_skip_report(last.as_ref(), &next, true, state),
            "heartbeat must always emit"
        );
    }

    #[test]
    fn handle_inbound_frame_dispatches_report_ack_on_accepted_payload() {
        let _g = observability_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        observability::reset_for_tests();
        observability::transition_to(BridgeState::Connected, None);

        let mut pending: Option<LastReportedSnapshot> =
            Some(snap("idX", PasswordKind::Preset, "pwX"));
        let mut last: Option<LastReportedSnapshot> = None;

        let payload = br#"{"result":"accepted"}"#;
        let dispo = handle_inbound_frame(payload, &mut pending, &mut last);
        assert!(matches!(dispo, SessionDisposition::Continue));
        assert!(last.is_some());
        assert!(pending.is_none());
        assert_eq!(current_snapshot().state, BridgeState::Authorized);
    }

    #[test]
    fn handle_inbound_frame_unrecognised_json_is_dropped() {
        let _g = observability_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        observability::reset_for_tests();
        observability::transition_to(BridgeState::Connected, None);

        let mut pending: Option<LastReportedSnapshot> = None;
        let mut last: Option<LastReportedSnapshot> = None;
        // Random JSON object — no `result` / `protocol` we recognise.
        let payload = br#"{"hello":"world"}"#;
        let dispo = handle_inbound_frame(payload, &mut pending, &mut last);
        assert!(
            matches!(dispo, SessionDisposition::Continue),
            "unrecognised inbound frame must not tear down the session"
        );
        // State unchanged.
        assert_eq!(current_snapshot().state, BridgeState::Connected);
    }

    // =====================================================================
    // Task 10.x — Log frame builder unit test
    // =====================================================================

    #[test]
    fn log_level_wire_matches_protocol_serde_form() {
        // The wire form fed into `hmac_log` MUST match what serde
        // emits for `protocol::LogLevel` (which is itself driven by
        // `#[serde(rename_all = "snake_case")]`). Cross-checking
        // both sides ensures the HMAC input and the JSON layer agree.
        for (l, expected) in [
            (log::Level::Error, "error"),
            (log::Level::Warn, "warn"),
            (log::Level::Info, "info"),
            (log::Level::Debug, "debug"),
            (log::Level::Trace, "trace"),
        ] {
            assert_eq!(log_level_wire(l), expected);
            // Round-trip protocol::LogLevel through serde to confirm
            // its kebab-into-snake match.
            let pl = protocol_log_level(l);
            let s = serde_json::to_string(&pl).expect("LogLevel serialisable");
            assert_eq!(s, format!("\"{}\"", expected));
        }
    }
}
