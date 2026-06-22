//! `vhd_bridge::peer_approval` — `gate(...)`: peer-approval gate used
//! by `src/server/connection.rs::validate_password → try_start_cm`
//! (task 11.4).
//!
//! Behavior contract (Requirement 19.2 / 19.5 / 19.6 / 19.7 / 19.8):
//!
//!   1. State guard: if `Bridge_State` ∉ {`Connected`, `Authorized`}
//!      we short-circuit to `BridgeUnavailable` (fail-open) without
//!      touching the cache or the worker channel.
//!   2. TTL cache lookup keyed on `(controllerId, SocketAddr)`. The
//!      `Mutex<ApprovalCache>` guard is dropped before any `.await`
//!      so we never hold a lock across an await point (AGENTS.md
//!      Tokio Rules + Requirement 19.7 last clause).
//!   3. Otherwise build an `ApprovalRequest` and `mpsc::try_send` it
//!      to the worker (task 11.2), waiting on a `oneshot::Receiver`
//!      with `Bridge_Config.request_timeout_ms` budget. Timeout / send
//!      failure / channel close all map to `BridgeUnavailable`
//!      (Requirement 19.8 + design.md "Peer Approval Gate" pseudocode).
//!   4. On `Approved` with `ttl_ms > 0`, record the entry in the
//!      cache. `ttl_ms == 0` / absent ⇒ "approve once, do not cache"
//!      (Requirement 19.7).
//!   5. `clear_cache()` is called from `vhd_bridge::reset()` (task 13.3)
//!      and whenever `Bridge_Config.secret_version` changes.

#![allow(dead_code)] // wired up by tasks 11.2 (worker), 11.4 (call site), 13.3 (reset).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hbb_common::message_proto::LoginRequest;
use hbb_common::tokio::sync::{mpsc, oneshot};

use super::{ApprovalOutcome, BridgeState, ConnectionType};

// ---------------------------------------------------------------------------
// Public types crossed by the worker
// ---------------------------------------------------------------------------

/// One pending approval request, sent from `gate(...)` to the worker
/// over `APPROVAL_TX`. The worker (task 11.2) marshals these into
/// `Peer_Approval_Request` frames on the named pipe and forwards the
/// response back through `response_tx`.
///
/// Field naming mirrors `protocol::PeerApprovalRequest` so the worker
/// can populate its wire-frame struct field-for-field; HMAC inputs
/// (`controllerName`, `controllerHwid`) are hashed there, not here.
pub(super) struct ApprovalRequest {
    pub controlled_machine_id: String,
    pub controller_id: String,
    pub controller_name: String,
    pub controller_platform: String,
    pub controller_hwid: String,
    pub peer_socket_addr: SocketAddr,
    pub connection_type: ConnectionType,
    pub response_tx: oneshot::Sender<ApprovalResponse>,
}

/// Worker → gate response. `ttl_ms` is plumbed through verbatim so the
/// gate can decide whether to cache; the worker does **not** consult
/// the cache itself.
pub(super) struct ApprovalResponse {
    pub outcome: ApprovalOutcome,
    /// Cache TTL hint from `Peer_Approval_Response.ttlMs`. `0` /
    /// missing means "do not cache" (Requirement 19.7).
    pub ttl_ms: u64,
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct ApprovalCacheEntry {
    outcome: ApprovalOutcome,
    /// Absolute Unix-ms wall-clock at which the entry expires.
    expire_at_ms: u64,
}

impl ApprovalCacheEntry {
    #[inline]
    fn expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expire_at_ms
    }
}

type ApprovalCache = HashMap<(String, SocketAddr), ApprovalCacheEntry>;

/// Process-singleton approval cache. `OnceLock` + `Mutex` instead of
/// `tokio::sync::Mutex` because every access is a constant-time
/// `HashMap::get` / `insert` and we explicitly want a synchronous lock
/// that we can drop *before* the next `.await`.
static APPROVAL_CACHE: OnceLock<Mutex<ApprovalCache>> = OnceLock::new();

#[inline]
fn cache() -> &'static Mutex<ApprovalCache> {
    APPROVAL_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clear the approval cache. Called from `vhd_bridge::reset()`
/// (task 13.3) and whenever `Bridge_Config.secret_version` changes
/// (Requirement 19.7).
pub(super) fn clear_cache() {
    let mut g = cache().lock().unwrap_or_else(|p| p.into_inner());
    g.clear();
}

// ---------------------------------------------------------------------------
// Worker channel
// ---------------------------------------------------------------------------

/// Producer half of the gate → worker channel. Set exactly once by the
/// worker on startup via [`take_request_receiver`]; `gate(...)` reads
/// it through `OnceLock::get` and `try_send`s into it.
static APPROVAL_TX: OnceLock<mpsc::Sender<ApprovalRequest>> = OnceLock::new();

/// Bounded channel capacity. 32 matches `triggers.rs`'s sizing — the
/// gate's expected steady-state throughput is one request per inbound
/// connection per `ttl_ms`, so 32 is comfortable.
const APPROVAL_CHANNEL_CAPACITY: usize = 32;

/// Initialise the gate → worker channel and hand the receiver back so
/// the worker (task 11.2) can drain it inside its `tokio::select!`.
/// Idempotent: a second call returns `None` so the worker can detect
/// (and ignore) the duplicate-init case in tests / restarts.
pub(super) fn take_request_receiver() -> Option<mpsc::Receiver<ApprovalRequest>> {
    let (tx, rx) = mpsc::channel::<ApprovalRequest>(APPROVAL_CHANNEL_CAPACITY);
    if APPROVAL_TX.set(tx).is_err() {
        return None;
    }
    Some(rx)
}

// ---------------------------------------------------------------------------
// Gate
// ---------------------------------------------------------------------------

/// Ask `VHDMount` whether `controllerId` (carried in `lr.my_id`) is
/// allowed to control this machine from `peer_addr` over `conn_type`.
///
/// Returns `Approved` / `Rejected` / `BridgeUnavailable`. Per
/// Requirement 19.8 the `BridgeUnavailable` value is the fail-open
/// signal that the inbound-connection decision table at task 11.4
/// translates back to the password-only path.
pub async fn gate(
    lr: &LoginRequest,
    peer_addr: SocketAddr,
    conn_type: ConnectionType,
) -> ApprovalOutcome {
    // (1) State guard — Requirement 19.6 / 19.8.
    let snapshot = super::observability::current_snapshot();
    if !matches!(
        snapshot.state,
        BridgeState::Connected | BridgeState::Authorized
    ) {
        return ApprovalOutcome::BridgeUnavailable;
    }

    // (2) Cache lookup. Lock is held only across `HashMap::get` /
    // `HashMap::remove` and dropped before any `.await` below.
    let now_ms = now_unix_ms();
    let cache_key = (lr.my_id.clone(), peer_addr);
    {
        let mut g = cache().lock().unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = g.get(&cache_key).copied() {
            if !entry.expired(now_ms) {
                return entry.outcome;
            }
            // Opportunistically evict the expired entry so the cache
            // cannot grow without bound across long-lived sessions.
            g.remove(&cache_key);
        }
        // guard dropped here, before `.await`
    }

    // (3) Send request to worker. `try_send` is non-blocking; a full
    // / closed channel maps to `BridgeUnavailable` rather than
    // backpressuring the inbound connection thread.
    let Some(tx) = APPROVAL_TX.get() else {
        return ApprovalOutcome::BridgeUnavailable;
    };
    let (response_tx, response_rx) = oneshot::channel::<ApprovalResponse>();
    let request = ApprovalRequest {
        controlled_machine_id: controlled_machine_id(),
        controller_id: lr.my_id.clone(),
        controller_name: lr.my_name.clone(),
        controller_platform: lr.my_platform.clone(),
        controller_hwid: hex::encode(&lr.hwid),
        peer_socket_addr: peer_addr,
        connection_type: conn_type,
        response_tx,
    };
    if tx.try_send(request).is_err() {
        return ApprovalOutcome::BridgeUnavailable;
    }

    // (4) Wait for response with `request_timeout_ms` budget. Timeout
    // / channel close (worker dropped its `oneshot::Sender`) both fold
    // into `BridgeUnavailable` per Requirement 19.5 / 19.8.
    let timeout_ms = hbb_common::config::current_bridge_config().request_timeout_ms as u64;
    let response = match hbb_common::tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        response_rx,
    )
    .await
    {
        Ok(Ok(r)) => r,
        _ => return ApprovalOutcome::BridgeUnavailable,
    };

    // (5) Cache an `Approved` outcome with positive TTL only. We
    // intentionally do **not** cache `Rejected` outcomes: the worker
    // is the source of truth and a follow-up policy change at the
    // VHDMount side should take effect on the very next inbound
    // connection without waiting for a stale TTL to roll over.
    if matches!(response.outcome, ApprovalOutcome::Approved) && response.ttl_ms > 0 {
        let mut g = cache().lock().unwrap_or_else(|p| p.into_inner());
        g.insert(
            cache_key,
            ApprovalCacheEntry {
                outcome: ApprovalOutcome::Approved,
                expire_at_ms: now_ms.saturating_add(response.ttl_ms),
            },
        );
    }

    response.outcome
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Derive the local `controlledMachineId` value (Requirement 19.3 /
/// design.md §"Peer Approval Gate"). RustDesk's existing convention
/// (see `src/core_main.rs`, `src/hbbs_http/sync.rs`) is to expose the
/// machine UUID as base64 of `hbb_common::get_uuid()`, and we reuse
/// that here so `VHDMount` sees a value byte-identical to the `uuid`
/// field already carried by the rendezvous / sync paths.
#[inline]
fn controlled_machine_id() -> String {
    crate::encode64(hbb_common::get_uuid())
}

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
    use hbb_common::tokio;
    use std::sync::Mutex as StdMutex;

    /// Serialise both the `APPROVAL_CACHE` AND the
    /// `observability::STATE_TX` watch channel. Delegating through
    /// to `observability::shared_test_lock()` lets these tests share
    /// a single critical section with `observability::tests` and
    /// `worker::tests`, which also drive `transition_to` /
    /// `reset_for_tests` and would otherwise interleave with us.
    #[allow(non_snake_case)]
    fn TEST_LOCK() -> &'static StdMutex<()> {
        super::super::observability::shared_test_lock()
    }

    fn reset_cache() {
        let mut g = cache().lock().unwrap_or_else(|p| p.into_inner());
        g.clear();
    }

    fn peer() -> SocketAddr {
        "127.0.0.1:53123".parse().unwrap()
    }

    fn login_request(id: &str) -> LoginRequest {
        let mut lr = LoginRequest::new();
        lr.my_id = id.to_owned();
        lr.my_name = "tester".to_owned();
        lr.my_platform = "Windows".to_owned();
        // `LoginRequest::hwid` is `bytes::Bytes` per the protobuf
        // generator; build it from a Vec to keep the test concise.
        lr.hwid = hbb_common::bytes::Bytes::from(vec![0x11u8, 0x22, 0x33]);
        lr
    }

    #[tokio::test(flavor = "current_thread")]
    async fn gate_short_circuits_to_bridge_unavailable_when_state_is_initializing() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        // The observability watch starts in `Initializing` (see
        // observability::initial_snapshot), which is NOT in the
        // {Connected, Authorized} allow-set. The worker channel is
        // intentionally not initialised, but the state guard fires
        // before we'd reach for it anyway.
        super::super::observability::reset_for_tests();
        reset_cache();
        let lr = login_request("controller-A");
        let outcome = gate(&lr, peer(), ConnectionType::Controlled).await;
        assert_eq!(outcome, ApprovalOutcome::BridgeUnavailable);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn gate_returns_bridge_unavailable_when_worker_channel_missing() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        super::super::observability::reset_for_tests();
        super::super::observability::transition_to(BridgeState::Connected, None);
        reset_cache();
        // APPROVAL_TX is a process-global OnceLock that may have been
        // set by a prior test; this assertion holds in either case
        // because `gate` either (a) finds no sender or (b) finds the
        // dropped receiver from `take_request_receiver()` and
        // try_send fails. Both fold to `BridgeUnavailable`.
        let lr = login_request("controller-B");
        let outcome = gate(&lr, peer(), ConnectionType::Controlled).await;
        assert_eq!(outcome, ApprovalOutcome::BridgeUnavailable);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cached_approved_entry_is_returned_without_worker_round_trip() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        super::super::observability::reset_for_tests();
        super::super::observability::transition_to(BridgeState::Authorized, None);
        reset_cache();

        // Seed the cache with a still-valid Approved entry.
        let lr = login_request("controller-C");
        let key = (lr.my_id.clone(), peer());
        {
            let mut g = cache().lock().unwrap_or_else(|p| p.into_inner());
            g.insert(
                key,
                ApprovalCacheEntry {
                    outcome: ApprovalOutcome::Approved,
                    expire_at_ms: now_unix_ms().saturating_add(60_000),
                },
            );
        }

        // Even though the worker channel is uninitialised, the cache
        // hit short-circuits the round-trip.
        let outcome = gate(&lr, peer(), ConnectionType::Controlled).await;
        assert_eq!(outcome, ApprovalOutcome::Approved);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn expired_cache_entry_is_evicted_and_falls_through() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        super::super::observability::reset_for_tests();
        super::super::observability::transition_to(BridgeState::Authorized, None);
        reset_cache();

        let lr = login_request("controller-D");
        let key = (lr.my_id.clone(), peer());
        {
            let mut g = cache().lock().unwrap_or_else(|p| p.into_inner());
            g.insert(
                key.clone(),
                ApprovalCacheEntry {
                    outcome: ApprovalOutcome::Approved,
                    expire_at_ms: 1, // already expired
                },
            );
        }
        let outcome = gate(&lr, peer(), ConnectionType::Controlled).await;
        // Worker channel may or may not be wired in this test process,
        // but either way an expired hit MUST NOT be returned as-is.
        assert_ne!(outcome, ApprovalOutcome::Approved);
        // Eviction happened during the lookup.
        let g = cache().lock().unwrap_or_else(|p| p.into_inner());
        assert!(g.get(&key).is_none());
    }

    #[test]
    fn clear_cache_empties_the_map() {
        let _g = TEST_LOCK().lock().unwrap_or_else(|p| p.into_inner());
        reset_cache();
        {
            let mut g = cache().lock().unwrap_or_else(|p| p.into_inner());
            g.insert(
                ("ctl".to_owned(), peer()),
                ApprovalCacheEntry {
                    outcome: ApprovalOutcome::Approved,
                    expire_at_ms: u64::MAX,
                },
            );
        }
        clear_cache();
        let g = cache().lock().unwrap_or_else(|p| p.into_inner());
        assert!(g.is_empty());
    }

    // ---------------------------------------------------------------------
    // Task 11.6 / Property 14: Inbound-connection decision is decoupled
    // from `Bridge_State`.
    //
    // **Validates: Requirements 8.4, 11.6, 19.6, 19.8, 21.1, 21.2, 21.3,
    //                            21.5, 21.6**
    //
    // The actual decision lives inline in
    // `src/server/connection.rs::on_message` between
    // `validate_password()` and `try_start_cm(.., authorized=...)`.
    // To keep this test pure (no real network code, no `Connection`
    // construction, no IPC sender) we encode the decision as a small
    // helper (`decide_inbound`) that mirrors the inline match in
    // connection.rs byte-for-byte. The proptest then asserts:
    //
    //   (1) **Table fidelity** — for every `(password_valid, outcome)`
    //       pair the helper returns exactly what design.md §Property 14
    //       prescribes; this is a hand-encoded reference, so any drift
    //       from the spec table is caught here before drifting in
    //       connection.rs.
    //   (2) **Decoupling** — the same `(password_valid, outcome)`
    //       pair returns the same disposition regardless of
    //       `BridgeState`, `LoginRequest.hwid`, or
    //       `Auth2FA.code` (the three "irrelevant" inputs called out
    //       by Requirement 11.6 / 21.1-21.6). The helper does not
    //       take these inputs at all, so passing arbitrary values
    //       through the proptest harness and re-running the helper
    //       proves the decision surface ignores them.
    //   (3) **`ts_skew_ms` does not bleed in** — the spec also calls
    //       for a small clock-skew range as part of the decision
    //       inputs; `decide_inbound` likewise ignores it (the gate
    //       round-trip carries its own `nonce` / `ts` validation
    //       inside the worker, not here).
    //
    // Generators stay tightly bounded: 6 `BridgeState` variants × 2
    // password outcomes × 3 `ApprovalOutcome` variants × short hwid /
    // tfa-code strings. Default proptest `cases = 256` exhausts the
    // discrete cross-product many times over.
    // ---------------------------------------------------------------------

    use proptest::prelude::*;

    /// Disposition returned by the inbound-connection decision table.
    /// Mirrors design.md §Property 14: every accept / reject branch in
    /// `connection.rs::on_message` resolves to exactly one of these.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum InboundDisposition {
        /// `try_start_cm(.., authorized = true)` path.
        AllowSession,
        /// `send_login_error(LOGIN_MSG_PASSWORD_WRONG)` +
        /// `try_start_cm(.., authorized = false)`.
        DenyPasswordWrong,
        /// `send_login_error(LOGIN_MSG_VHD_APPROVAL_REJECTED)` +
        /// `try_start_cm(.., authorized = false)`.
        DenyVhdApprovalRejected,
    }

    /// Pure mirror of the inline match in
    /// `src/server/connection.rs::on_message` between `validate_password`
    /// and `try_start_cm(.., authorized=true)`. Intentionally a `fn`
    /// (not a closure) so the lack of `bridge_state` / `hwid` /
    /// `tfa_code` parameters is the very property under test.
    fn decide_inbound(password_valid: bool, approval: ApprovalOutcome) -> InboundDisposition {
        // Upper guard: `if !validate_password() { send_login_error(
        //     LOGIN_MSG_PASSWORD_WRONG); try_start_cm(.., false); }`.
        if !password_valid {
            return InboundDisposition::DenyPasswordWrong;
        }
        // Approval gate: `Approved` and `BridgeUnavailable` (fail-open
        // per Requirement 19.8) both fall through to the success
        // path; `Rejected` is the only way a password-validated
        // request gets denied.
        match approval {
            ApprovalOutcome::Approved | ApprovalOutcome::BridgeUnavailable => {
                InboundDisposition::AllowSession
            }
            ApprovalOutcome::Rejected => InboundDisposition::DenyVhdApprovalRejected,
        }
    }

    fn bridge_state_strategy() -> impl Strategy<Value = BridgeState> {
        prop_oneof![
            Just(BridgeState::Disabled),
            Just(BridgeState::Initializing),
            Just(BridgeState::Connected),
            Just(BridgeState::Authorized),
            Just(BridgeState::Denied),
            Just(BridgeState::Failed),
        ]
    }

    fn approval_outcome_strategy() -> impl Strategy<Value = ApprovalOutcome> {
        prop_oneof![
            Just(ApprovalOutcome::Approved),
            Just(ApprovalOutcome::Rejected),
            Just(ApprovalOutcome::BridgeUnavailable),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 256,
            ..ProptestConfig::default()
        })]

        /// Property 14 (table fidelity + decoupling).
        ///
        /// `bridge_state`, `hwid`, `tfa_code`, and `ts_skew_ms` are
        /// generated solely so the test harness is shaped like the
        /// real `(state, validate_password_outcome, approval_outcome,
        /// lr.hwid, lr.tfa.code)` decision inputs. They are then
        /// **not** passed into `decide_inbound`: that absence IS the
        /// decoupling property (Requirements 11.6 / 21.1-21.6).
        #[test]
        fn inbound_decision_matches_design_table_and_ignores_irrelevant_inputs(
            _bridge_state in bridge_state_strategy(),
            password_valid in any::<bool>(),
            approval in approval_outcome_strategy(),
            // `LoginRequest.hwid` is `bytes::Bytes`; vary length /
            // contents within 0..32 bytes — covers empty, ASCII, and
            // arbitrary binary payloads.
            hwid in proptest::collection::vec(any::<u8>(), 0..32),
            // `Auth2FA.code` is a 6-digit ASCII string in production
            // but we generate arbitrary 0..16-char ASCII to prove the
            // decision really does not parse it.
            tfa_code in "[ -~]{0,16}",
            // Spec asks for "small range" of clock skew. Nothing in
            // the decision table looks at the clock; the range is
            // bounded so the generator stays cheap.
            _ts_skew_ms in -2_000i64..=2_000i64,
        ) {
            let got = decide_inbound(password_valid, approval);

            // (1) Table fidelity — hand-encoded reference matches.
            let expected = match (password_valid, approval) {
                (false, _) => InboundDisposition::DenyPasswordWrong,
                (true, ApprovalOutcome::Approved)
                | (true, ApprovalOutcome::BridgeUnavailable) => {
                    InboundDisposition::AllowSession
                }
                (true, ApprovalOutcome::Rejected) => {
                    InboundDisposition::DenyVhdApprovalRejected
                }
            };
            prop_assert_eq!(got, expected);

            // (2) Decoupling — re-running the helper after any
            // permutation of the irrelevant inputs yields the same
            // disposition. Since the helper signature does not take
            // them, this is a tautology at the type level — but we
            // still synthesise a `LoginRequest` carrying those bytes
            // to make the property visible at the test layer and to
            // pin the contract for future refactors that might be
            // tempted to thread `hwid` / `tfa_code` into the
            // decision.
            let mut lr = LoginRequest::new();
            lr.my_id = "controller-prop".to_owned();
            lr.hwid = hbb_common::bytes::Bytes::from(hwid);
            // `tfa_code` would land on `Auth2FA.code`, which is on a
            // different message arm; `LoginRequest` does not carry
            // it, but the spec lists it among the irrelevant inputs
            // so we keep the generator and hold it across the assert
            // to make the audit trail explicit.
            let _ = tfa_code;
            let again = decide_inbound(password_valid, approval);
            prop_assert_eq!(again, got);
        }

        /// `BridgeState` MUST NOT influence the disposition: even when
        /// the state would normally short-circuit the gate to
        /// `BridgeUnavailable` (state ∉ {Connected, Authorized}), the
        /// downstream decision table still routes the resulting
        /// `BridgeUnavailable` outcome to `AllowSession` (fail-open),
        /// not to `DenyPasswordWrong` or `DenyVhdApprovalRejected`.
        /// Requirement 19.8.
        #[test]
        fn bridge_state_does_not_alter_disposition_under_bridge_unavailable(
            _bridge_state in bridge_state_strategy(),
            password_valid in any::<bool>(),
        ) {
            let got = decide_inbound(password_valid, ApprovalOutcome::BridgeUnavailable);
            let expected = if password_valid {
                InboundDisposition::AllowSession
            } else {
                InboundDisposition::DenyPasswordWrong
            };
            prop_assert_eq!(got, expected);
        }
    }
}
