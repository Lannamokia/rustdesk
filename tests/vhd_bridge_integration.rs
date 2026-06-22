//! Task 22.1: end-to-end integration test for the `vhd_bridge` worker
//! against a real Windows named-pipe server hosted inside the test
//! binary.
//!
//! ## What this test exercises
//!
//! The current worker (after tasks 7.x / 11.2) drives a state machine
//! whose **observable surface from `tests/`** is:
//!
//!   * Connect + handshake outcomes (`ok` / `deny` / `rate_limited` /
//!     `invalid_proof` / `secret_outdated`) → `Bridge_State`
//!     transitions per design.md §"状态机".
//!   * `peer_approval::gate(...)` → `Peer_Approval_Request` /
//!     `Peer_Approval_Response` round-trip per design.md §"Peer
//!     Approval Gate".
//!   * Reconnect-with-jitter (Requirement 9.1) — the test asserts
//!     that after a server-side disconnect mid-handshake the worker
//!     reconnects within a bounded window.
//!
//! ## What this test deliberately does NOT exercise
//!
//! Per the inline doc-comment at `worker.rs::run`, the post-handshake
//! session loop today is a thin placeholder that only multiplexes
//! `peer_approval` requests over the pipe. The following are owned
//! by tasks 8.x / 10.x and are NOT yet wired into the worker:
//!
//!   * `Report_Frame` writes (startup / id_change / password_change /
//!     rotation / heartbeat) — task 8.x. So `Bridge_State` cannot
//!     yet reach `Authorized` from a real handshake — `Authorized`
//!     requires `ReportAck.accepted`, which requires `Report_Frame`
//!     writes that the worker does not yet emit.
//!   * `Log_Frame` writes — task 10.x.
//!   * Heartbeat-timeout failure detection — depends on report
//!     frames, task 8.x.
//!   * Bad-MAC detection on a *Report*Ack — same.
//!
//! The task 22.1 prompt asks for "happy path → Authorized", "bad MAC
//! → invalid_mac", and "heartbeat timeout" scenarios. Those are
//! mechanically unreachable against today's worker; the property
//! tests in `worker::tests` (task 7.6, Property 7) and
//! `triggers::tests` (tasks 8.3 / 8.4) cover the equivalent state-
//! machine and timer correctness in isolation. Once tasks 8.x / 10.x
//! land the missing scenarios will be added in this file.
//!
//! ## Test-only production-side hooks
//!
//! Two `pub` entry points on `librustdesk::vhd_bridge`, both gated
//! by `cfg(debug_assertions)` so they are never compiled into a
//! release binary, give this integration test the minimum surface
//! it needs:
//!
//!   * [`vhd_bridge::test_set_skip_peer_check`] flips the peer-image
//!     check in `pipe::open_and_verify` so the worker tolerates a
//!     server whose process image is the test binary itself
//!     (not `VHDMount.exe`).
//!   * [`vhd_bridge::test_set_pipe_name`] retargets the worker to a
//!     per-process pipe path via the same
//!     `try_apply_bridge_option(...)` validator the production IPC
//!     config-sync uses.
//!
//! ## Why one test, sequential scenarios
//!
//! `vhd_bridge::start` is `OnceLock`-guarded — the bridge worker is
//! a single process-lifetime task. Cargo runs `#[test]` functions in
//! parallel threads of the same process, which would race on that
//! `OnceLock`. To keep the test deterministic, this file ships a
//! single `#[test]` entry point that drives multiple scenarios in
//! sequence over the same worker.

#![cfg(all(target_os = "windows", feature = "vhd-bridge", debug_assertions))]

use std::time::{Duration, Instant};

use hbb_common::tokio::io::AsyncWriteExt;
use hbb_common::tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use hbb_common::tokio::sync::oneshot;
use hbb_common::tokio::{self, time};

// `librustdesk` is the cdylib/staticlib/rlib crate name set in the
// root Cargo.toml `[lib]` section. Integration tests in `tests/`
// link against that name, not the `[[bin]]` target.
use librustdesk::vhd_bridge;

// ---------------------------------------------------------------------------
// Mock pipe-server fixture
//
// One `unique_pipe_name()` per scenario keeps reruns / parallel test
// processes isolated. The `ServerOptions::create` first instance
// must call `first_pipe_instance(true)` so the kernel-level name
// reservation matches the production server's expectation.
// ---------------------------------------------------------------------------

/// Build a per-scenario pipe path that survives reruns of the same
/// binary and concurrent `cargo test` processes. `\\.\pipe\` is a
/// global namespace; collisions surface as `ERROR_ACCESS_DENIED` on
/// the second `ServerOptions::create`.
fn unique_pipe_name(suffix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        r"\\.\pipe\rustdesk-vhd-bridge-integration-{}-{}-{}",
        std::process::id(),
        nanos,
        suffix,
    )
}

/// Read one frame off the connected server side and return its JSON
/// payload. Mirrors the codec in `src/vhd_bridge/frame.rs` (4-byte LE
/// length prefix + payload). Caller is responsible for parsing JSON.
async fn server_read_frame(server: &mut NamedPipeServer) -> std::io::Result<Vec<u8>> {
    use hbb_common::tokio::io::AsyncReadExt;
    let mut len_buf = [0u8; 4];
    server.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    server.read_exact(&mut buf).await?;
    Ok(buf)
}

/// Write a JSON-encoded frame onto the server side using the same
/// 4-byte LE length prefix + payload codec.
async fn server_write_frame(
    server: &mut NamedPipeServer,
    payload: &[u8],
) -> std::io::Result<()> {
    let len = (payload.len() as u32).to_le_bytes();
    server.write_all(&len).await?;
    server.write_all(payload).await?;
    server.flush().await?;
    Ok(())
}

/// Scripted server side for one connection. The closure `script`
/// owns the freshly-connected `NamedPipeServer` and runs whatever
/// scenario the test step needs. When the closure returns the
/// pipe is dropped, which closes the kernel handle.
///
/// `ready_tx` fires after `connect()` resolves so the test step can
/// observe that the worker has just opened the pipe. The signal is
/// not strictly required — the worker drives the conversation —
/// but it lets the test correlate timeline events.
async fn scripted_server<F, Fut>(pipe_name: String, ready_tx: oneshot::Sender<()>, script: F)
where
    F: FnOnce(NamedPipeServer) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&pipe_name)
        .expect("vhd_bridge integration: ServerOptions::create failed");
    server
        .connect()
        .await
        .expect("vhd_bridge integration: server-side connect failed");
    let _ = ready_tx.send(());
    script(server).await;
}

/// Drain the worker's outbound `HandshakeFrame` and reply with the
/// caller-supplied raw JSON. Common to every scenario.
async fn read_handshake_then_reply(server: &mut NamedPipeServer, reply_json: &[u8]) {
    let _request = server_read_frame(server)
        .await
        .expect("server: failed to read Handshake_Frame");
    server_write_frame(server, reply_json)
        .await
        .expect("server: failed to write HandshakeResponse");
}

// ---------------------------------------------------------------------------
// State observation helpers
// ---------------------------------------------------------------------------

/// Poll `vhd_bridge::current_state()` until `predicate` returns true
/// or `timeout` elapses. Returns the final snapshot.
async fn wait_until_state<F>(predicate: F, timeout: Duration) -> vhd_bridge::BridgeStateSnapshot
where
    F: Fn(&vhd_bridge::BridgeStateSnapshot) -> bool,
{
    let start = Instant::now();
    loop {
        let s = vhd_bridge::current_state();
        if predicate(&s) {
            return s;
        }
        if start.elapsed() >= timeout {
            return s;
        }
        time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------
// One-time worker bootstrap
//
// The bridge worker is a process-singleton; bring it up exactly
// once before the first scenario and reuse it across the rest.
// ---------------------------------------------------------------------------

/// Start the bridge worker on the ambient runtime if it has not
/// been started yet. Idempotent at the `vhd_bridge::start`
/// level — the second call is a no-op short-circuit.
fn ensure_worker_started() {
    let handle = tokio::runtime::Handle::current();
    vhd_bridge::start(&handle);
}

// ---------------------------------------------------------------------------
// Top-level test
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vhd_bridge_end_to_end_against_real_named_pipe() {
    // Flip the debug-only test hooks before any worker activity. The
    // peer-image check is process-wide, so this single call covers
    // every scenario below.
    vhd_bridge::test_set_skip_peer_check(true);

    // Tighten retry interval so reconnect-driven scenarios don't
    // spend the default 2000 ms + 200 ms jitter idle between
    // attempts. Floor is 1 ms (per `try_apply_bridge_option`'s
    // [1, 60_000] clamp).
    hbb_common::config::test_apply_bridge_option(
        hbb_common::config::keys::VHD_BRIDGE_RETRY_INTERVAL,
        "100",
    );
    hbb_common::config::test_apply_bridge_option(
        hbb_common::config::keys::VHD_BRIDGE_REQUEST_TIMEOUT,
        "2000",
    );

    // ----- Scenario 1: handshake OK → Connected ------------------------
    //
    // Validates: Requirements 5.5 / 8.6 / design §"BridgeWorker 状态机"
    // row "Initializing → Connected: handshake ok".
    {
        let pipe = unique_pipe_name("scenario1-ok");
        vhd_bridge::test_set_pipe_name(&pipe);

        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe.clone();
        let server_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                // Reply OK and then keep the pipe alive long enough
                // for the test to observe `Connected`. The worker
                // holds the session open via
                // `hold_session_until_break`; we drop `srv` at the
                // end of the closure to surface a `BrokenPipe` on
                // the worker's read arm, which moves it back to
                // `Initializing`.
                read_handshake_then_reply(&mut srv, br#"{"ok":true}"#).await;
                time::sleep(Duration::from_millis(150)).await;
                // Drop = close pipe; worker will reconnect on next loop.
            },
        ));

        // Bring up the worker now that the server is listening. The
        // second / third / nth scenarios reuse this same worker.
        ensure_worker_started();

        let snap = wait_until_state(
            |s| matches!(s.state, vhd_bridge::BridgeState::Connected),
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(snap.state, vhd_bridge::BridgeState::Connected),
            "expected Connected, got {:?} (reason={:?})",
            snap.state,
            snap.last_reason
        );
        // `error_code` MUST be None for non-error states (Requirement 12.5).
        assert!(
            snap.error_code.is_none(),
            "Connected snapshot must not carry error_code"
        );

        // Drain the server task so its assertions fire if any.
        let _ = server_handle.await;
    }

    // ----- Scenario 2: handshake rejected with `secret_outdated` → Failed
    //
    // Validates: Requirements 5.6 / 9.5 / 11.2 / design §"状态机" row
    // "Initializing → Failed: secret_outdated". Permanent failure
    // sticks until `vhd_bridge::reset()` (Requirement 11.4).
    //
    // Note: the worker reconnects on the post-Scenario-1 BrokenPipe.
    // We start the new server first, then redirect the worker to
    // the new pipe path. The worker re-reads the config on every
    // iteration of its outer loop, so the next attempt picks the
    // updated pipe name up.
    {
        let pipe = unique_pipe_name("scenario2-secret-outdated");

        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe.clone();
        let server_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                read_handshake_then_reply(
                    &mut srv,
                    br#"{"ok":false,"reason":"secret_outdated"}"#,
                )
                .await;
                time::sleep(Duration::from_millis(150)).await;
            },
        ));

        vhd_bridge::test_set_pipe_name(&pipe);

        let snap = wait_until_state(
            |s| matches!(s.state, vhd_bridge::BridgeState::Failed),
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(snap.state, vhd_bridge::BridgeState::Failed),
            "expected Failed after secret_outdated, got {:?} (reason={:?})",
            snap.state,
            snap.last_reason
        );
        assert_eq!(snap.last_reason.as_deref(), Some("secret_outdated"));
        // `error_code` MUST be a member of `ALLOWED_ERROR_CODES`
        // (design §"Bridge_State 离散原因码", Requirement 12.5).
        assert_eq!(
            snap.error_code.as_deref(),
            Some("vhd.bridge.failed.secret_outdated"),
        );

        let _ = server_handle.await;
    }

    // ----- Scenario 3: vhd_bridge::reset() escapes Failed --------------
    //
    // Validates: Requirement 11.4 / design §"状态机" row "Failed →
    // Initializing: vhd_bridge::reset()". Pulse the reset signal and
    // confirm the snapshot leaves `Failed`. The worker will then go
    // looking for the pipe again; we point it at a fresh OK server
    // so the post-reset transition lands on `Connected` without
    // racing the previous scenario's closed pipe.
    {
        let pipe = unique_pipe_name("scenario3-after-reset");

        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe.clone();
        let server_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                read_handshake_then_reply(&mut srv, br#"{"ok":true}"#).await;
                time::sleep(Duration::from_millis(200)).await;
            },
        ));

        vhd_bridge::test_set_pipe_name(&pipe);

        // Issue the reset.
        vhd_bridge::reset();

        // Worker MUST leave `Failed` first, then walk back through
        // `Initializing → Connected` against the new server.
        let snap = wait_until_state(
            |s| matches!(s.state, vhd_bridge::BridgeState::Connected),
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(snap.state, vhd_bridge::BridgeState::Connected),
            "post-reset expected Connected, got {:?} (reason={:?}, error_code={:?})",
            snap.state,
            snap.last_reason,
            snap.error_code
        );

        let _ = server_handle.await;
    }

    // ----- Scenario 3b: secret_outdated `Failed` is sticky against
    //                    transient I/O (Task 22.2 part 1) -------------
    //
    // Validates: Requirements 5.6 / 9.5 / 9.6 / 9.8 / 11.2 / design
    // §"状态机" rows "Initializing → Failed: secret_outdated" and
    // "Failed → Failed: 任意 I/O 错误均忽略". Once the snapshot has
    // been driven into `Failed { reason: secret_outdated }`, no
    // amount of reconnect activity — `BrokenPipe` from a closed
    // pipe, a timeout against a non-existent endpoint, or a
    // freshly-OK server appearing on the configured pipe path —
    // may demote the state back to `Initializing` / `Connected`.
    // The only sanctioned escape is `vhd_bridge::reset()` (or a
    // `secret_version` change, which is structurally equivalent
    // because `worker::run()` re-reads `BridgeConfig` on every
    // outer-loop iteration; testing the latter would require
    // re-spawning the worker, which is impossible against a
    // process-singleton).
    //
    // Witness for the `transition_to`'s sink-state guard
    // (`observability.rs::transition_to` short-circuits when
    // `state == Failed`): we sample `last_change_at_ms` before the
    // stickiness check and reassert it is unchanged afterwards.
    // A regression that drops the guard would re-publish the
    // snapshot on every reconnect attempt and bump
    // `last_change_at_ms` forward.
    {
        // (a) Drive into Failed { secret_outdated } using the same
        //     mock as Scenario 2.
        let pipe_outdated = unique_pipe_name("scenario3b-secret-outdated");
        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe_outdated.clone();
        let outdated_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                read_handshake_then_reply(
                    &mut srv,
                    br#"{"ok":false,"reason":"secret_outdated"}"#,
                )
                .await;
                time::sleep(Duration::from_millis(150)).await;
            },
        ));
        vhd_bridge::test_set_pipe_name(&pipe_outdated);

        let failed_snap = wait_until_state(
            |s| matches!(s.state, vhd_bridge::BridgeState::Failed),
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(failed_snap.state, vhd_bridge::BridgeState::Failed),
            "scenario 3b prelude expected Failed, got {:?}",
            failed_snap.state,
        );
        assert_eq!(failed_snap.last_reason.as_deref(), Some("secret_outdated"));
        assert_eq!(
            failed_snap.error_code.as_deref(),
            Some("vhd.bridge.failed.secret_outdated"),
        );
        let _ = outdated_handle.await;

        // (b) Stand up a *new*, *OK*-replying server on a different
        //     pipe path and redirect the worker at it. A worker
        //     that ignored §9.8 / §11.2 would happily walk
        //     `Failed → Initializing → Connected` here. The sticky
        //     `Failed` sink-state must keep that from happening.
        let pipe_ok = unique_pipe_name("scenario3b-bait-ok-server");
        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe_ok.clone();
        let bait_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                // Best-effort: if the worker ever does try to
                // connect (which would itself be a regression),
                // serve a perfectly valid handshake reply so the
                // diagnostic in the test failure pinpoints the
                // sticky-Failed regression rather than a confusing
                // pipe-protocol mismatch downstream. Tolerate EOF
                // because the *expected* path is "worker stays in
                // Failed and never connects" — in which case the
                // server task will sit on `read_frame` until the
                // test drops the pipe handle.
                if let Ok(_) = server_read_frame(&mut srv).await {
                    let _ = server_write_frame(&mut srv, br#"{"ok":true}"#).await;
                }
                time::sleep(Duration::from_millis(150)).await;
            },
        ));
        vhd_bridge::test_set_pipe_name(&pipe_ok);

        // The worker parks on `reset_notify().notified().await` at
        // the top of `run()` while in `Failed`, so the only writes
        // the snapshot may receive in this window are
        // `transition_to` calls — all of which are short-circuited
        // by the sink-state guard. Sample for 500 ms (≫ retry
        // interval + jitter). Each sample MUST report `Failed` with
        // unchanged `last_change_at_ms`.
        let pinned_change_at_ms = failed_snap.last_change_at_ms;
        let watch_until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < watch_until {
            let s = vhd_bridge::current_state();
            assert!(
                matches!(s.state, vhd_bridge::BridgeState::Failed),
                "permanent Failed must stick; saw {:?} (reason={:?}, error_code={:?})",
                s.state,
                s.last_reason,
                s.error_code,
            );
            assert_eq!(
                s.last_reason.as_deref(),
                Some("secret_outdated"),
                "Failed `last_reason` must remain secret_outdated",
            );
            assert_eq!(
                s.error_code.as_deref(),
                Some("vhd.bridge.failed.secret_outdated"),
                "Failed `error_code` must remain vhd.bridge.failed.secret_outdated",
            );
            assert_eq!(
                s.last_change_at_ms, pinned_change_at_ms,
                "sink-state guard violated: snapshot was republished while in Failed",
            );
            time::sleep(Duration::from_millis(40)).await;
        }

        // (c) `vhd_bridge::reset()` is the only escape. After it,
        //     the worker must walk `Failed → Initializing →
        //     Connected` against the bait OK server. This restores
        //     the worker to Connected so the next scenario starts
        //     from a clean slate (parity with Scenario 3's exit).
        vhd_bridge::reset();
        let post_reset = wait_until_state(
            |s| matches!(s.state, vhd_bridge::BridgeState::Connected),
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(post_reset.state, vhd_bridge::BridgeState::Connected),
            "post-reset expected Connected, got {:?} (reason={:?})",
            post_reset.state,
            post_reset.last_reason,
        );
        assert!(
            post_reset.last_change_at_ms > pinned_change_at_ms,
            "post-reset snapshot must advance last_change_at_ms",
        );

        let _ = bait_handle.await;
    }

    // ----- Scenario 3c: peer_not_vhdmount → permanent Failed
    //                    (Task 22.2 part 2) ------------------------
    //
    // Validates: Requirements 9.5 / 10.5 / 11.2 / design §"状态机"
    // row "Initializing → Failed: peer_not_vhdmount". The peer
    // process-image check in `pipe::open_and_verify` rejects any
    // server whose image basename is not `VHDMount.exe`; the worker
    // translates that into `HandshakeOutcome::PermanentFailure
    // (REASON_PEER_NOT_VHDMOUNT)`, which routes through
    // `transition_to_failed` exactly once thanks to the sink-state
    // guard ("只切一次", task prompt second bullet).
    //
    // We exercise the production peer-check path by toggling
    // `test_set_skip_peer_check(false)` for the duration of this
    // scenario. The test binary's image basename
    // (`vhd_bridge_integration-<hash>.exe`) categorically is not
    // `VHDMount.exe`, so any successful pipe connect MUST be
    // rejected with `ConnectError::PeerNotVhdMount`.
    //
    // Note: this scenario must run *after* Scenario 3b's
    // `vhd_bridge::reset()` so the worker is back in `Connected`
    // (or `Initializing` mid-reconnect) — sticky `Failed` from 3b
    // would mask whatever 3c is trying to assert.
    {
        // Re-engage the production peer-check. `pipe::open_and_verify`
        // reads the atomic on every connect attempt, so the toggle
        // takes effect on the next outer-loop iteration of
        // `worker::run()` regardless of where the worker currently
        // sits in its session. We restore the skip back to `true`
        // at the end of this scenario so Scenarios 4–6 keep their
        // existing semantics.
        vhd_bridge::test_set_skip_peer_check(false);

        // Stand up a server with a *valid* protocol reply. The
        // worker MUST reject it on the peer-image check before
        // ever sending a `Handshake_Frame`, so the script never
        // sees a request frame and can simply hold the pipe open
        // briefly before dropping it. We still write a valid
        // handshake reply just in case a regression bypasses the
        // peer-image check — that way the resulting state would be
        // `Connected`, not silently `Initializing`, making the
        // failure mode unambiguous.
        let pipe = unique_pipe_name("scenario3c-peer-not-vhdmount");
        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe.clone();
        let server_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                // Best-effort: if the peer-check is somehow
                // bypassed, reply OK so the regression surfaces as
                // an unexpected `Connected` rather than a hung
                // pipe. Tolerate EOF (the expected path: the
                // worker rejects the connection on peer-image
                // mismatch and never sends a handshake).
                if let Ok(_) = server_read_frame(&mut srv).await {
                    let _ = server_write_frame(&mut srv, br#"{"ok":true}"#).await;
                }
                time::sleep(Duration::from_millis(150)).await;
            },
        ));
        vhd_bridge::test_set_pipe_name(&pipe);

        // The worker MUST land in `Failed { peer_not_vhdmount }`
        // on the very first connect attempt against this pipe.
        let snap = wait_until_state(
            |s| {
                matches!(s.state, vhd_bridge::BridgeState::Failed)
                    && s.last_reason.as_deref() == Some("peer_not_vhdmount")
            },
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(snap.state, vhd_bridge::BridgeState::Failed),
            "expected Failed after peer_not_vhdmount, got {:?} (reason={:?})",
            snap.state,
            snap.last_reason,
        );
        assert_eq!(snap.last_reason.as_deref(), Some("peer_not_vhdmount"));
        assert_eq!(
            snap.error_code.as_deref(),
            Some("vhd.bridge.failed.peer_not_vhdmount"),
        );

        // Stickiness witness: same as Scenario 3b. With the worker
        // parked in `Failed`, sample `current_state()` for 500 ms
        // and assert nothing about `last_change_at_ms` shifts. This
        // simultaneously catches "切只一次" regressions (a
        // hypothetical re-emission of `transition_to_failed
        // (REASON_PEER_NOT_VHDMOUNT)` would still be a no-op under
        // the sink-state guard, but `last_change_at_ms` would not
        // be touched either way).
        let pinned_change_at_ms = snap.last_change_at_ms;
        let watch_until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < watch_until {
            let s = vhd_bridge::current_state();
            assert!(
                matches!(s.state, vhd_bridge::BridgeState::Failed),
                "peer_not_vhdmount Failed must stick; saw {:?}",
                s.state,
            );
            assert_eq!(s.last_reason.as_deref(), Some("peer_not_vhdmount"));
            assert_eq!(
                s.error_code.as_deref(),
                Some("vhd.bridge.failed.peer_not_vhdmount"),
            );
            assert_eq!(
                s.last_change_at_ms, pinned_change_at_ms,
                "sink-state guard violated under peer_not_vhdmount",
            );
            time::sleep(Duration::from_millis(40)).await;
        }

        let _ = server_handle.await;

        // (Recovery) Re-enable the test peer-check skip and reset
        // back into Connected, mirroring Scenario 3's exit shape.
        // Without this, Scenario 4 would also fail the peer-image
        // check and never reach its mid-handshake-close path.
        vhd_bridge::test_set_skip_peer_check(true);
        let pipe = unique_pipe_name("scenario3c-after-reset");
        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe.clone();
        let server_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                read_handshake_then_reply(&mut srv, br#"{"ok":true}"#).await;
                time::sleep(Duration::from_millis(200)).await;
            },
        ));
        vhd_bridge::test_set_pipe_name(&pipe);
        vhd_bridge::reset();
        let post_reset = wait_until_state(
            |s| matches!(s.state, vhd_bridge::BridgeState::Connected),
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(post_reset.state, vhd_bridge::BridgeState::Connected),
            "post-reset expected Connected, got {:?}",
            post_reset.state,
        );
        let _ = server_handle.await;
    }

    // ----- Scenario 4: server closes pipe mid-handshake → Initializing -
    //
    // Validates: Requirements 2.4 / 9.7 / design §"状态机" row
    // "Initializing → Initializing: connect fail / timeout 重试".
    // The server accepts the connection, reads the Handshake_Frame,
    // and then drops the pipe without responding. The worker MUST
    // surface a transient I/O error and remain in `Initializing`.
    {
        let pipe = unique_pipe_name("scenario4-mid-handshake");

        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe.clone();
        let server_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                // Accept and consume the worker's handshake, then
                // close without replying. Worker will hit EOF on the
                // response read and translate it into the §9.7
                // transient path.
                let _ = server_read_frame(&mut srv).await;
                let _ = srv.shutdown().await;
                drop(srv);
            },
        ));

        vhd_bridge::test_set_pipe_name(&pipe);

        // The worker should sit in `Initializing` (with the
        // pipe-closed reason) for at least one full retry cycle.
        let snap = wait_until_state(
            |s| {
                matches!(s.state, vhd_bridge::BridgeState::Initializing)
                    && matches!(
                        s.last_reason.as_deref(),
                        Some("pipe_closed") | Some("pipe_timeout")
                    )
            },
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(snap.state, vhd_bridge::BridgeState::Initializing),
            "expected Initializing after mid-handshake close, got {:?}",
            snap.state
        );
        assert!(
            matches!(
                snap.last_reason.as_deref(),
                Some("pipe_closed") | Some("pipe_timeout")
            ),
            "unexpected reason after mid-handshake close: {:?}",
            snap.last_reason
        );
        // Transient `Initializing` MUST NOT carry an error_code
        // (Requirement 12.5: errorCode is only set for Failed/Denied).
        assert!(
            snap.error_code.is_none(),
            "Initializing snapshot must not carry error_code (got {:?})",
            snap.error_code
        );

        let _ = server_handle.await;
    }

    // ----- Scenario 5: handshake rejected with `rate_limited` → Denied -
    //
    // Validates: Requirements 5.7 / 9.2 / design §"状态机" row
    // "Initializing → Denied: rate_limited". Confirms the centralised
    // state-publish path attaches the right `error_code` for the UI.
    {
        let pipe = unique_pipe_name("scenario5-rate-limited");

        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe.clone();
        let server_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                read_handshake_then_reply(
                    &mut srv,
                    br#"{"ok":false,"reason":"rate_limited"}"#,
                )
                .await;
                time::sleep(Duration::from_millis(150)).await;
            },
        ));

        vhd_bridge::test_set_pipe_name(&pipe);

        let snap = wait_until_state(
            |s| matches!(s.state, vhd_bridge::BridgeState::Denied),
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(snap.state, vhd_bridge::BridgeState::Denied),
            "expected Denied after rate_limited, got {:?}",
            snap.state
        );
        assert_eq!(snap.last_reason.as_deref(), Some("rate_limited"));
        assert_eq!(
            snap.error_code.as_deref(),
            Some("vhd.bridge.denied.rate_limited"),
        );

        let _ = server_handle.await;
    }

    // ----- Scenario 6: `rate_limited` adds a 60 s back-off -------------
    //
    // Validates: Requirement 9.2 / design §"BridgeWorker 状态机" row
    // "Denied → Initializing: 重连，rate_limited 叠加 60 s" /
    // `worker.rs::compute_retry_delay`'s 60_000 ms overlay.
    //
    // Scenario 5 left the worker in `Denied { reason: rate_limited }`
    // and immediately entered
    // `sleep_retry(retry_interval_ms, Some(REASON_RATE_LIMITED))`.
    // The worker is therefore mid-sleep for
    // `retry_interval_ms + 60_000 + jitter[0..=200]` ms before its
    // next handshake attempt. Under §22.1's `retry_interval_ms = 100`
    // hook, the next handshake MUST land inside
    // `[t_denied + 60_100, t_denied + 60_300] ms`, modulo pipe-connect
    // and handshake round-trip overhead.
    //
    // The 60 s overlay is observable purely from
    // `BridgeStateSnapshot::last_change_at_ms`, which
    // `observability::transition_to` writes from `now_unix_ms()` on
    // every state change. We anchor `t_denied` from the post-
    // scenario-5 snapshot, redirect the worker to a fresh OK-
    // replying server, and assert the wall-clock delta to the next
    // `Connected` transition lies in the spec'd window.
    //
    // Note on `tokio::time::pause()` / `advance()`: the
    // accelerate-time path is not viable here because (a) the test
    // runtime is `flavor = "multi_thread"` and `start_paused = true`
    // is only honoured under `flavor = "current_thread"`, and (b)
    // the bridge worker is a process-singleton (`OnceLock`-guarded
    // `vhd_bridge::start`) shared with scenarios 1–5, so it cannot
    // be re-spawned onto a paused runtime mid-test. The wall-clock
    // ~60 s cost is the price of exercising the actual production
    // sleep path against a real named pipe; `worker::tests` Property
    // 6 already pins `compute_retry_delay`'s envelope without I/O.
    {
        // Re-fetch the current snapshot to anchor `t_denied`. Scenario
        // 5's last assertion left it in Denied { rate_limited }; the
        // re-fetch is defensive against any future inter-scenario
        // change.
        let denied_snap = vhd_bridge::current_state();
        assert!(
            matches!(denied_snap.state, vhd_bridge::BridgeState::Denied),
            "scenario 6 prelude expected Denied carryover from scenario 5, got {:?}",
            denied_snap.state,
        );
        assert_eq!(
            denied_snap.last_reason.as_deref(),
            Some("rate_limited"),
            "scenario 6 prelude expected last_reason == rate_limited",
        );
        let t_denied_ms = denied_snap.last_change_at_ms;

        // Stand up an OK-replying server on a fresh pipe path. The
        // worker is currently sleeping its 60 s retry overlay against
        // scenario 5's (now-closed) pipe; redirecting the pipe name
        // now means the next iteration of `worker::run()` (after the
        // overlay expires) reads the updated config (Requirement 4.4)
        // and lands on this server.
        let pipe = unique_pipe_name("scenario6-rate-limited-overlay");
        let (ready_tx, _ready_rx) = oneshot::channel::<()>();
        let server_pipe = pipe.clone();
        let server_handle = tokio::spawn(scripted_server(
            server_pipe,
            ready_tx,
            |mut srv| async move {
                read_handshake_then_reply(&mut srv, br#"{"ok":true}"#).await;
                // Hold long enough for the worker's `transition_to(
                // Connected, None)` to publish before we drop the
                // pipe. The exact value is not load-bearing — the
                // test polls `current_state()` every 20 ms.
                time::sleep(Duration::from_millis(300)).await;
            },
        ));
        vhd_bridge::test_set_pipe_name(&pipe);

        // (a) Worker MUST NOT reconnect within `retry_interval_ms`
        //     (Requirement 9.2 first clause: 60 s overlay applies
        //     before any retry attempt). 500 ms is comfortably above
        //     `retry_interval_ms + max_jitter` (100 + 200 = 300 ms)
        //     yet four orders of magnitude below the 60 s overlay
        //     floor, so a regression that drops the overlay would
        //     surface here as a premature `Connected`.
        time::sleep(Duration::from_millis(500)).await;
        let mid = vhd_bridge::current_state();
        assert!(
            !matches!(mid.state, vhd_bridge::BridgeState::Connected),
            "worker reconnected inside retry_interval_ms — 60 s overlay missing; \
             snapshot = {:?}",
            mid,
        );

        // (b) Worker MUST reconnect after the overlay expires.
        //     Upper bound: 65 s = 60 s overlay + 100 ms base +
        //     200 ms jitter + ~5 s headroom for pipe-connect,
        //     handshake round-trip, and multi-thread scheduling
        //     jitter. A regression that inflates the overlay
        //     above 60 s surfaces in the lower / upper bound check
        //     immediately below.
        let connected_snap = wait_until_state(
            |s| matches!(s.state, vhd_bridge::BridgeState::Connected),
            Duration::from_secs(65),
        )
        .await;
        assert!(
            matches!(connected_snap.state, vhd_bridge::BridgeState::Connected),
            "expected Connected after rate_limited 60 s overlay; got {:?}",
            connected_snap,
        );

        // (c) Wall-clock delta between the two transitions MUST lie
        //     inside [retry_interval_ms + 60_000,
        //     retry_interval_ms + 60_200 + slack] ms. Slack absorbs
        //     pipe-connect + 4-byte length prefix + JSON write/read
        //     + `tokio::time::sleep` rounding under multi-thread
        //     scheduling. The test pins "60 s overlay was applied",
        //     not "`compute_retry_delay`'s micro-bounds match" —
        //     `worker::tests::Property 6` owns the latter.
        let delta_ms = connected_snap
            .last_change_at_ms
            .saturating_sub(t_denied_ms);
        let lo = 100u64 + 60_000;
        let hi = 100u64 + 60_200 + 5_000;
        assert!(
            (lo..=hi).contains(&delta_ms),
            "rate_limited reconnect delta {} ms not in [{},{}] ms — \
             expected 100 ms base + 60_000 ms overlay + ≤200 ms jitter + scheduling slack",
            delta_ms,
            lo,
            hi,
        );

        // (d) "Overlay clears on next success" half of Requirement
        //     9.2 is implicit in the worker's call site: `run()`
        //     passes `last_reason = None` for every non-`Denied`
        //     outcome, so the next `sleep_retry` call after this
        //     scenario can no longer carry the rate_limited overlay
        //     until a fresh `rate_limited` `Denied` fires. Pinning
        //     that explicitly would require provoking another
        //     transient I/O failure here and timing the next retry,
        //     which adds another ~retry_interval_ms of wall-clock
        //     for a property already covered by
        //     `worker::tests::reconnect_envelope_is_independent_of_failure_count`.

        let _ = server_handle.await;
    }

    // ----- End-of-test cleanup -----------------------------------------
    //
    // The worker keeps running after this function returns (it is a
    // spawned process-lifetime task, per Requirement 4.3). That is
    // intentional and matches production: cargo's test harness
    // tears the process down after the test completes, killing the
    // worker along with it.
    //
    // Restore the peer-check guard so any further test that links
    // against this binary observes the production behaviour.
    vhd_bridge::test_set_skip_peer_check(false);
}

// ---------------------------------------------------------------------------
// Future scenarios (deferred — task references inline)
//
// Once tasks 8.x (Report_Frame writes + heartbeat dispatch) and 10.x
// (Log_Frame sink wiring) land, this file SHALL gain the following
// scenarios — listed here as a forward-looking checklist so the
// integration coverage tracks production code as it ships:
//
//   1. Happy path → Authorized
//      Server: HandshakeResponse{ok:true}, then ReportAck{accepted}
//      on the first Report_Frame. Asserts `Bridge_State` reaches
//      `Authorized` and that `record_accepted` fires.
//      Validates: Requirement 6.5, 8.6.
//
//   2. ReportAck rejected with `invalid_mac` → Denied + error_code
//      Validates: Requirement 6.6, 12.5.
//
//   3. Heartbeat tick produces a `Report_Frame` with reason="heartbeat"
//      after 30 minutes (driven via tokio::time::pause + advance,
//      hosted in `worker::tests` rather than `tests/` so the paused
//      clock doesn't leak between scenarios).
//      Validates: Requirement 7.6, 7.7.
//
//   4. LOGIN_MSG_VHD_APPROVAL_PENDING progress signal — needs
//      `connection.rs::spawn_pending_pump` plumbing observable from
//      a test. Likely lives in `src/server/connection.rs::tests`
//      rather than here.
//
//   5. `version_mismatch` end-to-end (Task 22.2 deferred half).
//      `protocol::HandshakeErrorReason` does not yet emit
//      `version_mismatch` on the wire — it only carries `deny`,
//      `rate_limited`, `invalid_proof`, `secret_outdated`. The
//      symbolic state-machine path (`transition_to_failed
//      (REASON_VERSION_MISMATCH)` is sticky and routes to
//      `vhd.bridge.failed.version_mismatch`) is already covered
//      by `observability::tests::transition_to_failed_is_sticky_
//      within_one_startup_cycle` and Property 7 in
//      `worker::tests`. A real end-to-end `version_mismatch`
//      scenario lights up once `VHDMount` (and the matching wire
//      enum here) starts emitting it — at that point this file
//      SHALL gain a third stickiness-witness scenario alongside
//      3b / 3c.
//      Validates: Requirements 5.6 / 9.5 / 11.2 (version_mismatch
//      branch).
//
// Each of those scenarios already has a property-test cousin in
// the corresponding submodule's `#[cfg(test)] mod tests` (Property 7
// for the state-machine, Property 9 for heartbeat cadence, etc.);
// the integration version just confirms the pieces compose
// end-to-end against a real pipe.
// ---------------------------------------------------------------------------
