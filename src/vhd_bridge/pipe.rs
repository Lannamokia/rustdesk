//! `vhd_bridge::pipe` — `open_and_verify`: connect to the VHDMount
//! named pipe via `tokio::net::windows::named_pipe::ClientOptions` and
//! verify the peer process image is one of the accepted VHDMounter
//! binaries.
//!
//! Layering (design.md §"命名管道层（pipe.rs）"):
//!
//!  1. `tokio::time::timeout(Duration::from_millis(timeout_ms), ...)`
//!     bounds the entire connect attempt — including the
//!     `ERROR_PIPE_BUSY` retry loop the standard tokio named-pipe
//!     client idiom requires (`ClientOptions::open` is synchronous).
//!     The caller in `worker.rs` translates `ConnectError::Timeout`
//!     and `ConnectError::Io(_)` into the §9.1 fixed-interval reconnect
//!     path.
//!  2. Once connected we resolve the *server* PID with
//!     `GetNamedPipeServerProcessId` and the executable path with
//!     `QueryFullProcessImageNameW`, comparing the file name against
//!     the accepted set: `VHDMounter.exe` plus the suffixed family
//!     `VHDMounter_<tag>.exe` (e.g. `VHDMounter_LE2025.exe`),
//!     ASCII-case-insensitively (Requirement 10.5; rule encoded in
//!     `is_expected_peer_image`).
//!  3. On image mismatch the client is `shutdown()` (literal spec
//!     wording) and dropped — `NamedPipeClient` closes its handle on
//!     drop; the explicit `shutdown` is a flush + half-close marker
//!     so that any TODO future write would not hit a stale handle.
//!     The worker translates `PeerNotVhdMount` into a permanent
//!     `Failed` state per Requirement 9.5 / 10.5.
//!
//! This module deliberately performs the peer-image check **only** on
//! the path that has actually opened a pipe; observability paths
//! (`current_state()`, `vhd-bridge-state`) MUST NOT trigger any extra
//! `OpenProcess` syscall (Requirement 10.5 last clause).
//!
//! No new IPC crate is introduced: connect uses
//! `hbb_common::tokio::net::windows::named_pipe::ClientOptions`, the
//! same primitive task 7.x reads/writes through the worker
//! (Requirements 13.2, 13.3).

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::time::Duration;

use hbb_common::log;
use hbb_common::tokio::io::AsyncWriteExt;
use hbb_common::tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use hbb_common::tokio::time;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Win32 `ERROR_PIPE_BUSY`. Returned by `CreateFileW` / `ClientOptions::open`
/// while the named-pipe server has reached its instance count and is
/// waiting for an existing client to close. The standard tokio
/// `ClientOptions` example loops with `time::sleep(50ms)` until either
/// the pipe becomes available or the outer `timeout` fires.
const ERROR_PIPE_BUSY: i32 = 231;

/// Per-busy-tick sleep before retrying `ClientOptions::open`. Matches
/// the official tokio doc example. Bounded by the outer `timeout` so
/// busy-spinning cannot escape `request_timeout_ms`.
const PIPE_BUSY_RETRY: Duration = Duration::from_millis(50);

/// Peer-image acceptance rule.  Matches both the canonical name
/// `VHDMounter.exe` and the suffix family `VHDMounter_<tag>.exe`
/// (e.g. `VHDMounter_LE2025.exe`) ASCII-case-insensitively.  The
/// `_<tag>` part MUST be non-empty so that bare `VHDMounter_.exe`
/// or anything that just *starts with* `VHDMounter` (e.g.
/// `VHDMounterMalicious.exe`) still gets rejected.
///
/// Windows file systems are case-insensitive, so the comparison is
/// ASCII-case-insensitive across the whole string.
fn is_expected_peer_image(file_name: &str) -> bool {
    const PREFIX: &str = "VHDMounter";
    const SUFFIX: &str = ".exe";

    if file_name.len() < PREFIX.len() + SUFFIX.len() {
        return false;
    }
    let lower = file_name.to_ascii_lowercase();
    let prefix_lower = PREFIX.to_ascii_lowercase();
    let suffix_lower = SUFFIX.to_ascii_lowercase();

    if !lower.starts_with(&prefix_lower) || !lower.ends_with(&suffix_lower) {
        return false;
    }
    // Carve out the middle segment between PREFIX and SUFFIX.
    let middle = &lower[prefix_lower.len()..lower.len() - suffix_lower.len()];
    // Two acceptable shapes: empty (= VHDMounter.exe) or `_<non-empty>`.
    middle.is_empty() || (middle.starts_with('_') && middle.len() > 1)
}

/// Buffer size for `QueryFullProcessImageNameW`. Matches
/// `src/platform/windows.rs::get_process_executable_path`. 32 KiB
/// comfortably covers `MAX_PATH` (260) and long-path forms (≈ 32767).
const IMAGE_PATH_BUFFER_LEN: usize = 32 * 1024;

/// Connect-time error, mapped by the worker (task 7.2) to either a
/// fixed-interval reconnect or a permanent `Failed`:
///
///  * `Io(_)` and `Timeout` → `Initializing` + retry (Requirements 9.1, 9.7).
///  * `PeerNotVhdMount`     → permanent `Failed` (Requirements 9.5, 10.5).
#[derive(Debug)]
pub(super) enum ConnectError {
    /// Underlying `io::Error` — pipe missing, peer reset, GetLastError
    /// surface from a Windows API call, etc.
    Io(io::Error),
    /// `ClientOptions::open` did not return within `timeout_ms`,
    /// including time spent in the `ERROR_PIPE_BUSY` retry loop.
    Timeout,
    /// Peer pipe-server image is not in the accepted VHDMounter set
    /// (`VHDMounter.exe` or `VHDMounter_<tag>.exe`).  Treated as
    /// permanent — re-verifying immediately would just race the same
    /// process again.
    PeerNotVhdMount,
}

impl From<io::Error> for ConnectError {
    fn from(e: io::Error) -> Self {
        ConnectError::Io(e)
    }
}

/// Open the named pipe at `pipe_name` (e.g.
/// `\\.\pipe\VHDMount.RustDeskBridge`) and return a connected
/// [`NamedPipeClient`] only after verifying the server process is
/// an accepted `VHDMounter*.exe` binary (see `is_expected_peer_image`).
///
/// `timeout_ms` is the upper bound on the *entire* connect attempt,
/// including any `ERROR_PIPE_BUSY` retries. The worker passes
/// `Bridge_Config.request_timeout_ms` (default 5000 ms, design §"Bridge_Config").
///
/// On any non-success path the partially-opened pipe handle (if any) is
/// closed before the function returns.
pub(super) async fn open_and_verify(
    pipe_name: &str,
    timeout_ms: u32,
) -> Result<NamedPipeClient, ConnectError> {
    // Tokio's `ClientOptions::open` is synchronous; the standard
    // pattern wraps it in an async retry loop on ERROR_PIPE_BUSY and
    // bounds the whole loop with `time::timeout`. We follow that
    // verbatim so transient busy windows during VHDMount restarts do
    // not look like permanent connect failures to the worker.
    let pipe_name_owned = pipe_name.to_owned();
    let connect = async move {
        loop {
            match ClientOptions::new().open(&pipe_name_owned) {
                Ok(client) => return Ok::<NamedPipeClient, io::Error>(client),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    time::sleep(PIPE_BUSY_RETRY).await;
                }
                Err(e) => return Err(e),
            }
        }
    };

    let mut client =
        match time::timeout(Duration::from_millis(timeout_ms as u64), connect).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => return Err(ConnectError::Io(e)),
            Err(_elapsed) => return Err(ConnectError::Timeout),
        };

    // Debug-only test hook: integration tests in `tests/` host the
    // pipe server inside the test binary itself, whose image basename
    // never matches the accepted VHDMounter set. The end-to-end test
    // (task 22.1) flips this gate so the worker can complete a real
    // handshake against a mock server. The whole branch is
    // `#[cfg(debug_assertions)]`
    // so release builds (whose `[profile.release]` defaults
    // `debug-assertions = false`) never even compile the check —
    // production cannot disable Requirement 10.5.
    #[cfg(debug_assertions)]
    if test_skip_peer_check() {
        return Ok(client);
    }

    // Peer-process identity check. We do this exactly once, on the
    // path that just opened a pipe; the only-on-open invariant from
    // Requirement 10.5 is satisfied because no other call site of this
    // function exists.
    //
    // Each branch logs the rejection reason at `log::warn!` so
    // operators chasing "VHDMount sees connect-then-clean-EOF" symptoms
    // can tell which of the three sub-checks tripped without having to
    // re-instrument and rebuild.  Production releases historically had
    // no log at all for the success-rejected path; an unexpected
    // peer-image basename was indistinguishable from `OpenProcess`
    // access-denied.
    match peer_server_pid(&client).and_then(peer_image_file_name) {
        Ok(name) if is_expected_peer_image(&name) => Ok(client),
        Ok(name) => {
            // Image mismatch is the §10.5 permanent-error case. Spec
            // wording is "立即 shutdown() 并返回 ConnectError::PeerNotVhdMount";
            // `NamedPipeClient` closes its underlying HANDLE on drop,
            // so the explicit `shutdown()` here is a half-close marker
            // (and a flush of any future write buffer) rather than the
            // sole release path — drop still does the real cleanup.
            log::warn!(
                "vhd_bridge: peer pipe-server image basename {:?} \
                 is not in the accepted set; closing pipe",
                name
            );
            let _ = client.shutdown().await;
            Err(ConnectError::PeerNotVhdMount)
        }
        Err(e) => {
            // PID resolution or image-path query failed.  This is
            // the path that fires on `OpenProcess` access-denied,
            // `GetNamedPipeServerProcessId` failure, or stale-PID
            // races where the peer has already died by the time we
            // probe it.  Logging here distinguishes that case from
            // the basename-rejection branch above.
            log::warn!(
                "vhd_bridge: peer image probe failed; closing pipe: {:?}",
                e
            );
            let _ = client.shutdown().await;
            Err(e)
        }
    }
}

/// Resolve the peer pipe-server PID through `GetNamedPipeServerProcessId`.
///
/// `NamedPipeClient::as_raw_handle()` returns the underlying Win32
/// `HANDLE` for the pipe; the existing `src/ipc/auth.rs::peer_pid`
/// uses the same shape (one rung lower in the protocol stack — server
/// vs client process id query) so this stays idiomatic to the
/// codebase.
fn peer_server_pid(client: &NamedPipeClient) -> Result<u32, ConnectError> {
    let raw = client.as_raw_handle();
    if raw.is_null() {
        return Err(ConnectError::Io(io::Error::new(
            io::ErrorKind::Other,
            "vhd_bridge: NamedPipeClient exposed a null raw handle",
        )));
    }
    let mut pid: u32 = 0;
    // SAFETY: `raw` is the live raw handle owned by `client`; the
    // borrow of `client` keeps the handle alive for the whole call.
    // `pid` is a stack-local writable u32. Per windows-rs 0.61
    // `GetNamedPipeServerProcessId` returns `windows::core::Result<()>`;
    // we map any error into a generic `io::Error::Other` so the worker
    // can route it through the §9.1 reconnect path without leaking
    // GetLastError text into log frames it has not yet redacted.
    unsafe { GetNamedPipeServerProcessId(HANDLE(raw), &mut pid as *mut u32) }.map_err(
        |e| {
            ConnectError::Io(io::Error::new(
                io::ErrorKind::Other,
                format!("GetNamedPipeServerProcessId failed: {e}"),
            ))
        },
    )?;
    if pid == 0 {
        return Err(ConnectError::Io(io::Error::new(
            io::ErrorKind::Other,
            "vhd_bridge: GetNamedPipeServerProcessId returned pid 0",
        )));
    }
    Ok(pid)
}

/// Resolve the peer image's *file name* (e.g. `VHDMounter.exe`) given a
/// PID.
///
/// We deliberately return the file name only, not the full path:
///  * Requirement 10.5 specifies `进程映像路径校验` with the comparison
///    target being a member of the accepted VHDMounter set
///    (`VHDMounter.exe` / `VHDMounter_<tag>.exe`) — the install
///    location is not part of the contract, only the executable
///    basename.
///  * Logs that record this value via §12.x are already constrained to
///    discrete reason codes; a full path would risk leaking a username
///    in `C:\Users\<name>\...` if VHDMount were ever sideloaded.
fn peer_image_file_name(pid: u32) -> Result<String, ConnectError> {
    // SAFETY: All Win32 handles we acquire are released on every
    // return path (`CloseHandle` after the inner closure). `OpenProcess`
    // succeeds with a valid `HANDLE` or returns an error. The buffer
    // and length variables are stack-local; `QueryFullProcessImageNameW`
    // writes at most `length` UTF-16 code units into `buffer` and
    // updates `length` to the actual count.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(
            |e| {
                ConnectError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    format!("OpenProcess({pid}) failed: {e}"),
                ))
            },
        )?;

        let result = (|| -> Result<String, ConnectError> {
            let mut buffer = vec![0u16; IMAGE_PATH_BUFFER_LEN];
            let mut length = IMAGE_PATH_BUFFER_LEN as u32;
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
            .map_err(|e| {
                ConnectError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    format!("QueryFullProcessImageNameW failed: {e}"),
                ))
            })?;
            if length == 0 {
                return Err(ConnectError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    "vhd_bridge: QueryFullProcessImageNameW returned empty path",
                )));
            }
            buffer.truncate(length as usize);
            let path = PathBuf::from(OsString::from_wide(&buffer));
            Ok(path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default())
        })();

        // Always close the process handle, even if the closure failed.
        // Following the same `let _ =` pattern as
        // `src/platform/windows.rs::get_process_executable_path`.
        let _ = CloseHandle(process);
        result
    }
}

// ---------------------------------------------------------------------------
// Debug-only test hook for the peer-image check (task 22.1)
//
// The end-to-end integration test in `tests/vhd_bridge_integration.rs`
// hosts the named-pipe server inside the test binary itself, whose
// image basename is whatever cargo named the test executable —
// never matches the accepted VHDMounter set.  Without this hook
// the worker's first connect
// attempt would map every reply to `ConnectError::PeerNotVhdMount`
// and the worker would walk straight into a permanent `Failed`
// state, making the handshake / peer-approval round-trips
// mechanically unreachable from `tests/`.
//
// `cfg(debug_assertions)` is the compile-time gate that erases the
// hook from release builds. The default `[profile.release]` in
// `Cargo.toml` keeps `debug-assertions = false`, so:
//   * `cargo test --features vhd-bridge,controlled-only ...` (which
//     defaults to debug profile) compiles the hook and the test can
//     flip it before exercising the worker.
//   * `cargo build --release --features vhd-bridge,controlled-only`
//     never compiles the hook; production cannot disable Requirement
//     10.5 even by accident.
//
// The setter is `pub` only inside the parent module's `mod.rs`
// (also `cfg(debug_assertions)`); external test crates flip it
// through that single re-exported entry point.
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
static TEST_SKIP_PEER_CHECK: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(debug_assertions)]
#[inline]
fn test_skip_peer_check() -> bool {
    TEST_SKIP_PEER_CHECK.load(std::sync::atomic::Ordering::Relaxed)
}

/// Debug-build-only test hook: ask `open_and_verify` to skip the
/// `GetNamedPipeServerProcessId` + `QueryFullProcessImageNameW` /
/// VHDMounter-basename check on subsequent connect attempts.
///
/// Production callers SHALL NOT use this — release builds compile
/// `debug_assertions = false`, which strips both this function and
/// the consumer in `open_and_verify`. The integration test in
/// `tests/vhd_bridge_integration.rs` flips it to `true` exactly
/// once before spawning the bridge worker.
#[cfg(debug_assertions)]
pub(crate) fn set_test_skip_peer_check(on: bool) {
    TEST_SKIP_PEER_CHECK.store(on, std::sync::atomic::Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Task 6.2: error-classification unit tests for `open_and_verify`.
//
// Coverage of the three scenarios called out in tasks.md §6.2:
//
//   * "端点不存在"      → `ConnectError::Io(NotFound)`
//                          — exercised by `nonexistent_pipe_maps_to_io_notfound`.
//   * "假冒进程"        → `ConnectError::PeerNotVhdMount`
//                          — exercised by `peer_not_vhdmount_when_server_image_mismatches`,
//                            using a local `tokio::net::windows::named_pipe::ServerOptions`
//                            server (which wraps `CreateNamedPipeW`); the
//                            test binary itself plays the role of the
//                            "fake" peer because its image basename is
//                            never a member of the accepted VHDMounter set.
//   * "立即 EOF"        → `ConnectError::Io(BrokenPipe)`
//                          — see `immediate_eof_maps_to_io_brokenpipe`,
//                            which is `#[ignore]`d. Today's
//                            `open_and_verify` performs no post-connect
//                            read, so the BrokenPipe path is genuinely
//                            unreachable through this entry point. The
//                            real translation lives in the worker read
//                            loop (task 7.2) and is property-asserted
//                            via Property 7 in task 7.6.
//
// The second half of the task — "验证 `PeerNotVhdMount` 在 worker 中翻译
// 为永久型 `Failed`、`Io` / `Timeout` 翻译为 `Initializing`" — is a
// worker-state-machine assertion. `src/vhd_bridge/worker.rs` is still
// a stub at the time this test module is added (tasks 7.1+ are
// pending), so the worker-translation assertion is intentionally
// deferred to Property 7 in task 7.6 (`State-machine integrity`),
// which exercises the full transition table including this row.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use hbb_common::tokio::{self, net::windows::named_pipe::ServerOptions, time};

    use super::{is_expected_peer_image, open_and_verify, ConnectError};

    /// Build a per-test pipe path that is unique across concurrent
    /// `cargo test` runs and across re-runs of the same binary. PID +
    /// nanos + a per-test suffix is plenty: pipe names live in the
    /// kernel's `\Device\NamedPipe\` namespace which is global, and we
    /// must not collide with another in-flight test that is using
    /// `first_pipe_instance(true)`.
    fn unique_pipe_name(suffix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!(
            r"\\.\pipe\rustdesk-vhd-bridge-test-{}-{}-{}",
            std::process::id(),
            nanos,
            suffix,
        )
    }

    /// "端点不存在" — when no server has called `CreateNamedPipeW` for
    /// this name, `ClientOptions::open` surfaces `ERROR_FILE_NOT_FOUND`
    /// from CreateFileW, which `io::Error::from_raw_os_error` maps to
    /// `ErrorKind::NotFound`. `open_and_verify` MUST forward that as
    /// `ConnectError::Io(_)` with the kind preserved, so the worker's
    /// reconnect translator (task 7.2 / Requirement 9.7) can route it
    /// through the §9.1 fixed-interval retry path rather than the
    /// permanent-`Failed` path.
    #[tokio::test]
    async fn nonexistent_pipe_maps_to_io_notfound() {
        let pipe = unique_pipe_name("nonexistent");
        match open_and_verify(&pipe, 500).await {
            Err(ConnectError::Io(e)) => {
                assert_eq!(
                    e.kind(),
                    std::io::ErrorKind::NotFound,
                    "expected NotFound, got {:?} ({})",
                    e.kind(),
                    e,
                );
            }
            other => panic!("expected ConnectError::Io(NotFound), got {:?}", other),
        }
    }

    /// "假冒进程" — a local pipe server is created in-process via
    /// `ServerOptions::create` (which wraps `CreateNamedPipeW`). The
    /// pipe-server PID is therefore the test binary itself, whose
    /// image basename is something like `vhd_bridge-<hash>.exe` or
    /// `rustdesk-<hash>.exe`, never matches the accepted VHDMounter set.
    /// `open_and_verify`
    /// MUST reject with `PeerNotVhdMount` per Requirement 10.5; the
    /// worker (task 7.2) translates that into a permanent `Failed`
    /// state per Requirement 9.5, which is asserted by Property 7 in
    /// task 7.6.
    #[tokio::test]
    async fn peer_not_vhdmount_when_server_image_mismatches() {
        let pipe = unique_pipe_name("peer-mismatch");
        let pipe_for_server = pipe.clone();

        // Create the pipe-server BEFORE spawning the connect attempt
        // so we don't race the client into a `NotFound`. `tokio`'s
        // `ServerOptions::create` is synchronous and returns once the
        // kernel object exists.
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&pipe_for_server)
            .expect("vhd_bridge test: ServerOptions::create failed");

        // Drive the server side concurrently — `connect()` resolves
        // when the client opens the pipe. We don't read or write
        // anything; `open_and_verify` only inspects the peer process
        // identity and never produces traffic on this side.
        let server_task = tokio::spawn(async move {
            let _ = server.connect().await;
            // Hold the server alive until the client side finishes
            // its image-name check. Dropping `server` here would close
            // the kernel handle and could race the client's
            // `GetNamedPipeServerProcessId` call.
            time::sleep(Duration::from_millis(200)).await;
        });

        let result = open_and_verify(&pipe, 2000).await;

        // Always reap the server task so a failed assertion does not
        // leave a dangling tokio task across test boundaries.
        let _ = server_task.await;

        match result {
            Err(ConnectError::PeerNotVhdMount) => {}
            other => panic!(
                "expected ConnectError::PeerNotVhdMount, got {:?}",
                other
            ),
        }
    }

    /// "立即 EOF" — placeholder for the `Io(BrokenPipe)` row of the
    /// worker's reconnect translator.
    ///
    /// Today's `open_and_verify` does not perform any read after the
    /// peer-image check, so an EOF on the server side surfaces only on
    /// the next read in the worker's handshake loop (task 7.2). The
    /// translation `BrokenPipe` → `Initializing` is therefore
    /// asserted at the worker layer by Property 7 in task 7.6
    /// (`State-machine integrity`) rather than here. Re-enable this
    /// test if a future revision of `open_and_verify` inlines the
    /// first protocol read into the connect-time path.
    #[tokio::test]
    #[ignore = "BrokenPipe path lives in the worker read loop (tasks 7.2 / 7.6)"]
    async fn immediate_eof_maps_to_io_brokenpipe() {
        // Intentionally empty — see doc comment above.
    }

    /// Acceptance rule for the peer-image basename — synchronous /
    /// no Win32 calls, so it lives next to `is_expected_peer_image`
    /// itself rather than in the worker integration tests.
    ///
    /// Pins Requirement 10.5 plus the operator-side relaxation that
    /// ships VHDMounter as either a canonical `VHDMounter.exe` or a
    /// suffixed `VHDMounter_<tag>.exe` (e.g. `VHDMounter_LE2025.exe`).
    /// Anything outside this set MUST be rejected so the worker
    /// continues to walk the permanent-`Failed` path through
    /// `ConnectError::PeerNotVhdMount`.
    #[test]
    fn peer_image_acceptance_rule() {
        // Accepted shapes.
        assert!(is_expected_peer_image("VHDMounter.exe"));
        assert!(is_expected_peer_image("VHDMounter_LE2025.exe"));
        assert!(is_expected_peer_image("VHDMounter_x64.exe"));
        assert!(is_expected_peer_image("VHDMounter_v1.exe"));
        // Case-insensitivity (Windows file system convention).
        assert!(is_expected_peer_image("vhdmounter.exe"));
        assert!(is_expected_peer_image("VHDMOUNTER_LE2025.EXE"));
        assert!(is_expected_peer_image("VhdMounter_Tag.Exe"));

        // Rejected: legacy spelling.
        assert!(!is_expected_peer_image("VHDMount.exe"));
        // Rejected: prefix-substring confusables.
        assert!(!is_expected_peer_image("VHDMounterMalicious.exe"));
        assert!(!is_expected_peer_image("EvilVHDMounter.exe"));
        // Rejected: empty suffix tag.
        assert!(!is_expected_peer_image("VHDMounter_.exe"));
        // Rejected: wrong extension.
        assert!(!is_expected_peer_image("VHDMounter.dll"));
        assert!(!is_expected_peer_image("VHDMounter_x64"));
        // Rejected: empty / too short.
        assert!(!is_expected_peer_image(""));
        assert!(!is_expected_peer_image(".exe"));
    }
}
