//! `vhd_bridge::log_sink` — install the bridge as the global `log`
//! crate sink with a bounded ring-buffer queue, drop-oldest policy, and
//! a serial writer task that forwards each event to the bridge worker
//! (Requirements 18.1, 18.4, 18.5, 18.6, 18.8, 18.9).
//!
//! Task 10.1 wires up:
//!   * `OnceLock`-protected install: `vhd_bridge::start` calls `install`
//!     exactly once; a second call is a silent no-op.
//!   * A bounded ring buffer (cap 4096 events / 4 MiB total), guarded by
//!     a `std::sync::Mutex`. `Log::log` is synchronous and never blocks
//!     across `.await`; the lock is held for a single push / pop.
//!   * A single `tokio::spawn(log_writer_task(rx))` task drains the
//!     ring buffer and serialises writes through the bridge worker.
//!   * `Bridge_State` ∉ {`Connected`, `Authorized`} ⇒ drop silently and
//!     increment `LOG_DROP` (Requirement 18.5 / 18.8 / 18.9). No
//!     fall-back to local files, stderr, or Windows Event Log.
//!
//! Task 10.2 wires in [`redact_message`] (in-place replacement of any
//! sensitive `field=value` / `field: value` token with `"***"`) and a
//! [`redact_controller_id`] helper used by §19 / §10 logging paths
//! (Requirements 10.1, 10.2, 12.3, 18.7, 19.9). Field truncation is
//! still done by [`truncate_to_bytes`] introduced in task 10.1; tasks
//! 10.4 / 10.5 will exercise this module with property tests.
//!
//! The submodule is feature-gated by its parent (`mod log_sink;` in
//! `mod.rs` lives under `cfg(all(target_os = "windows", feature =
//! "vhd-bridge"))`), so no extra `cfg` attribute is needed here. When
//! the feature is off, `vhd_bridge::install_log_sink()` resolves to the
//! `mod.rs` no-op stub and the existing `flexi_logger` / `env_logger`
//! sinks remain in place (Requirement 18.9).

#![allow(dead_code)] // wired up by task 14.1 (install_log_sink delegate),
                     // task 10.2 (redact_message), and tasks 10.4 / 10.5
                     // (property tests).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use hbb_common::log;
use hbb_common::tokio::sync::Notify;

use super::worker;
use super::BridgeState;

/// Bounded queue capacity by event count. Per Requirement 18.5 / 18.10
/// (and design.md §"Log Sink"): 4096 events.
const LOG_QUEUE_CAPACITY_EVENTS: usize = 4096;

/// Bounded queue capacity by aggregate payload bytes (best-effort sum
/// of `target.len() + message.len()` across all queued events). Per
/// Requirement 18.10: 4 MiB hard ceiling on memory pressure.
const LOG_QUEUE_CAPACITY_BYTES: usize = 4 * 1024 * 1024;

/// Hard upper bounds applied during enqueue, before counting toward
/// `LOG_QUEUE_CAPACITY_BYTES`. Per Requirement 18.7 / design §"Log
/// Sink": `target` ≤ 256 bytes, `message` ≤ 4 KiB. Truncation runs
/// after redaction (`redact_message`) so we never split a UTF-8
/// sequence in a way that would leak partial sensitive text.
const TARGET_MAX_BYTES: usize = 256;
const MESSAGE_MAX_BYTES: usize = 4 * 1024;

/// Atomic counter for dropped log events. Increments every time:
///   * the bounded ring buffer is full and an oldest event is evicted;
///   * the bridge state is not in {`Connected`, `Authorized`}; or
///   * the writer task has not been installed yet.
///
/// Surfaced in `BridgeStateSnapshot.log_drop_count` via
/// `worker::log_drop_count_inc` (called by the writer-side eviction
/// path, see [`enqueue`]).
static LOG_DROP: AtomicU64 = AtomicU64::new(0);

/// One forwarded `log` crate event. Already redacted / truncated by
/// the producer (`Log::log`) before the ring buffer push, so the
/// writer task can serialise it as a `Log_Frame` without further
/// validation. The struct is `pub(super)` because task 7.2 will hand
/// `LogEvent`s to the bridge worker through this module's writer task.
#[derive(Debug, Clone)]
pub(super) struct LogEvent {
    pub level: log::Level,
    pub target: String,
    pub message: String,
    pub timestamp_ms: u64,
}

impl LogEvent {
    /// Best-effort byte cost used by the byte-cap accounting in
    /// [`enqueue`]. Constants like `level` and `timestamp_ms` are
    /// fixed-size so the variable cost reduces to the two strings.
    fn approx_bytes(&self) -> usize {
        self.target.len().saturating_add(self.message.len())
    }
}

/// Ring buffer + total byte counter, guarded by a single
/// `std::sync::Mutex`. Sync mutex (rather than `tokio::sync::Mutex`)
/// is correct here: every critical section is bounded — push one,
/// pop one, evict oldest — and never spans an `.await` (per
/// AGENTS.md "Tokio Rules").
struct LogQueue {
    events: VecDeque<LogEvent>,
    bytes: usize,
}

impl LogQueue {
    fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(LOG_QUEUE_CAPACITY_EVENTS),
            bytes: 0,
        }
    }
}

/// Process-singleton ring buffer. Set by the first `install()` call.
/// Subsequent installs are silent no-ops (Requirement 18.1 last
/// clause).
static LOG_QUEUE: OnceLock<Mutex<LogQueue>> = OnceLock::new();

/// Wakes the writer task whenever a new event is enqueued. `Notify`
/// is used (instead of `mpsc::channel`) so the producer can implement
/// drop-oldest semantics: `mpsc::Sender::try_send` returns `Full` but
/// gives no way to evict the oldest pending message. With a
/// hand-rolled ring buffer + `Notify`, the producer is the sole owner
/// of the eviction policy.
static LOG_NOTIFY: OnceLock<Notify> = OnceLock::new();

/// `log::Log` implementation that turns each `log::Record` into a
/// `LogEvent` and tries to enqueue it. Synchronous, never blocks,
/// never panics.
struct VhdBridgeLogger;

impl log::Log for VhdBridgeLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // Filter is global, set via `log::set_max_level` in `install()`.
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        // Drop silently when the bridge is not ready. The check is a
        // single watch read (lock-free) so it adds negligible cost to
        // the call site (Requirement 18.5 / 18.8).
        let snapshot = worker::current_snapshot();
        if !matches!(
            snapshot.state,
            BridgeState::Connected | BridgeState::Authorized
        ) {
            LOG_DROP.fetch_add(1, Ordering::Relaxed);
            worker::log_drop_count_inc(1);
            return;
        }

        // If the writer side has not been installed (sink off), drop
        // silently. This branch is unreachable in normal flow because
        // `install()` initialises both `LOG_QUEUE` and `LOG_NOTIFY`
        // before registering the logger, but we guard defensively.
        let (queue, notify) = match (LOG_QUEUE.get(), LOG_NOTIFY.get()) {
            (Some(q), Some(n)) => (q, n),
            _ => {
                LOG_DROP.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };

        let mut target = record.target().to_owned();
        truncate_to_bytes(&mut target, TARGET_MAX_BYTES);

        let mut message = format!("{}", record.args());
        // Redact sensitive `field=value` / `field: value` substrings
        // before the byte-length truncation so we never leak partial
        // password / proof / mac / hwid bytes (Requirement 18.7).
        redact_message(&mut message);
        truncate_to_bytes(&mut message, MESSAGE_MAX_BYTES);

        let event = LogEvent {
            level: record.level(),
            target,
            message,
            timestamp_ms: now_unix_ms(),
        };

        enqueue(queue, event);
        notify.notify_one();
    }

    fn flush(&self) {
        // No-op: the writer task drains on every `notify` wake-up.
    }
}

/// Push `event` into the ring buffer, evicting oldest entries while
/// either the event-count or byte-count cap would be exceeded. Each
/// eviction increments `LOG_DROP` and, indirectly, the publicly
/// observable `log_drop_count` field of `BridgeStateSnapshot`.
fn enqueue(queue: &Mutex<LogQueue>, event: LogEvent) {
    let event_bytes = event.approx_bytes();

    // `Mutex::lock` only fails on poisoning. Per AGENTS.md "Rust
    // Rules", treating a poisoned mutex as a normal control-flow
    // outcome is one of the two sanctioned `unwrap` exceptions.
    let mut q = match queue.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };

    let mut dropped: u64 = 0;
    while q.events.len() >= LOG_QUEUE_CAPACITY_EVENTS
        || q.bytes.saturating_add(event_bytes) > LOG_QUEUE_CAPACITY_BYTES
    {
        match q.events.pop_front() {
            Some(old) => {
                q.bytes = q.bytes.saturating_sub(old.approx_bytes());
                dropped = dropped.saturating_add(1);
            }
            None => break, // ring is empty but new event still over byte cap.
        }
    }

    q.bytes = q.bytes.saturating_add(event_bytes);
    q.events.push_back(event);
    drop(q);

    if dropped > 0 {
        LOG_DROP.fetch_add(dropped, Ordering::Relaxed);
        worker::log_drop_count_inc(dropped);
    }
}

/// Pop one event from the ring buffer. Returns `None` if empty.
/// Called only from inside the writer task.
fn try_pop_one() -> Option<LogEvent> {
    let queue = LOG_QUEUE.get()?;
    let mut q = match queue.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let ev = q.events.pop_front()?;
    q.bytes = q.bytes.saturating_sub(ev.approx_bytes());
    Some(ev)
}

/// Install the bridge log sink as the global `log` logger. Called
/// once from `vhd_bridge::start` (task 14.1 will wire the delegate).
/// A second call is a silent no-op (Requirement 18.1).
///
/// Spawns the writer task on the existing Tokio runtime — never
/// creates a nested runtime (AGENTS.md "Tokio Rules").
pub(super) fn install() {
    // First call wins; subsequent installs short-circuit before they
    // touch the global `log` registration so the logger boxing is
    // also performed exactly once.
    if LOG_QUEUE.set(Mutex::new(LogQueue::new())).is_err() {
        return;
    }
    // `LOG_NOTIFY` mirrors `LOG_QUEUE`: if `LOG_QUEUE.set` succeeded,
    // this set is also the first one; we ignore the (unreachable)
    // already-set case for symmetry.
    let _ = LOG_NOTIFY.set(Notify::new());

    hbb_common::tokio::spawn(log_writer_task());

    // `set_boxed_logger` may fail if another logger has already been
    // registered by some earlier code path; per the task spec we
    // silently ignore that — the bridge runs alongside whatever sink
    // is already in place. The max-level filter follows the same
    // best-effort policy.
    let _ = log::set_boxed_logger(Box::new(VhdBridgeLogger));
    log::set_max_level(level_filter_from_env());
}

/// Read the current `LOG_DROP` counter for inclusion in
/// `BridgeStateSnapshot.log_drop_count` snapshots assembled outside
/// this module. Internal callers prefer reading the snapshot directly.
pub(super) fn current_log_drop() -> u64 {
    LOG_DROP.load(Ordering::Relaxed)
}

/// Background task that drains the ring buffer and forwards each
/// event to the bridge worker as a `Log_Frame`. The drain is
/// `Notify`-driven so producers never block: every successful
/// `enqueue` calls `notify.notify_one()` to wake this task, which
/// then pops every available event and ships it to the worker via
/// [`worker::publish_log_event`]. The worker's session loop owns the
/// actual pipe write (Requirement 18.4: pipe I/O lives off the
/// caller's thread).
///
/// `worker::publish_log_event` is non-blocking: a full downstream
/// queue increments `log_drop_count` in lieu of back-pressuring this
/// task. That keeps the producer side free of latency dependencies on
/// the named pipe — exactly the property Requirement 18.6 calls for.
async fn log_writer_task() {
    // Take an `&'static Notify` reference so the await below is
    // tied to the process-singleton primitive rather than a local.
    let notify = match LOG_NOTIFY.get() {
        Some(n) => n,
        None => return, // install() failed before spawning; bail out.
    };

    loop {
        notify.notified().await;
        // Drain everything we can without giving up the runtime
        // tick; the lock is acquired and released for each pop so
        // producers are never blocked behind the writer.
        while let Some(event) = try_pop_one() {
            worker::publish_log_event(event);
        }
    }
}

/// Determine the maximum log level based on `RUST_LOG`, falling back
/// to `Info`. Bridge-specific config keys do not gate logging level
/// (Requirement 4.2: no extra runtime switches).
fn level_filter_from_env() -> log::LevelFilter {
    std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(log::LevelFilter::Info)
}

/// Truncate `s` to at most `max_bytes`, preserving UTF-8 boundaries.
fn truncate_to_bytes(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    // Walk back to the nearest char boundary so we never split a
    // multi-byte UTF-8 sequence. `is_char_boundary(0)` is always
    // true so the loop terminates.
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    s.truncate(idx);
}

// ---------------------------------------------------------------------------
// Redaction helpers (Requirements 10.1, 10.2, 12.3, 18.7, 19.9)
// ---------------------------------------------------------------------------

/// Field names whose values must be replaced with the constant string
/// `"***"` before a log line leaves this process. Match is performed
/// case-insensitively against the field name. The list mirrors
/// design.md §"Log Sink" and Requirements 10.1 / 18.7 / 19.9:
///
///   * `password` / `temporary_password` — Requirement 10.1
///   * `proof` / `mac` — Requirement 10.2 (HMAC body / handshake proof)
///   * `secret` — covers any `RustDeskClientSharedSecret` derivative;
///     the byte literal itself never appears in user-format strings
///     because no code path stringifies it (Requirement 3.7), but the
///     name guard is here as a defence-in-depth net.
///   * `hwid` — Requirement 19.9 (controllerHwid plain text forbidden)
///   * `controllerName` / `controllerHwid` — Requirement 19.9
///
/// Both `camelCase` and `snake_case` spellings are listed because the
/// surrounding RustDesk code base is mixed: protocol JSON uses the
/// camelCase spelling (`controllerName`) and Rust struct fields land
/// in `snake_case` (`controller_name`) when `Debug`-formatted.
const SENSITIVE_FIELDS: &[&str] = &[
    "password",
    "temporary_password",
    "temporaryPassword",
    "mac",
    "proof",
    "secret",
    "hwid",
    "controllerName",
    "controller_name",
    "controllerHwid",
    "controller_hwid",
];

/// Characters that terminate a redacted value. ASCII whitespace plus a
/// short list of structural delimiters that commonly close a value in
/// `Display` / `Debug` output (`,`, `;`, `}`, `)`, `"`, `'`, `>`).
fn is_value_terminator(c: char) -> bool {
    c.is_ascii_whitespace()
        || matches!(c, ',' | ';' | '}' | ')' | '"' | '\'' | '>')
}

/// True when `prev` is part of a longer identifier that the field name
/// happens to be a suffix of. Used to skip false matches like
/// `user_password=...` matching the `password` keyword: redacting that
/// substring would still leak the leading bytes of the *intended*
/// (different) identifier.
#[inline]
fn is_identifier_continuation(prev: char) -> bool {
    prev.is_ascii_alphanumeric() || prev == '_'
}

/// Redact every `field=value` / `field: value` occurrence in `msg` in
/// place, replacing the value with `"***"`.
///
/// Matching rules:
///
///   * The field name match is case-insensitive; only ASCII case is
///     folded so non-ASCII text inside `msg` is preserved verbatim.
///   * A match must start at a byte offset where the preceding
///     character is **not** an identifier continuation (`[A-Za-z0-9_]`),
///     so `user_password=...` only matches `user_password` and not the
///     trailing `password`.
///   * After the field name, optional ASCII whitespace is skipped, then
///     a single `=` or `:` separator is required. (`":="`-style pairs
///     are not in this protocol, so a single delimiter is sufficient.)
///     Optional ASCII whitespace after the separator is also skipped.
///   * The value extends from the post-separator byte offset up to the
///     first character matched by [`is_value_terminator`] or the end
///     of the string, whichever comes first.
///
/// All redactions are computed first (under an immutable borrow of
/// `msg`) then applied in reverse byte order so earlier indices stay
/// valid after each `replace_range`.
pub(super) fn redact_message(msg: &mut String) {
    if msg.is_empty() {
        return;
    }

    // Pre-compute lowercase view once. We only fold ASCII case: that
    // is sufficient to match every entry in `SENSITIVE_FIELDS` (all of
    // which are ASCII) and keeps byte offsets aligned with `msg` so we
    // can index into both strings interchangeably.
    let lower = msg.as_bytes().to_ascii_lowercase();

    let mut redactions: Vec<(usize, usize)> = Vec::new();

    for &field in SENSITIVE_FIELDS {
        let needle = field.to_ascii_lowercase();
        let needle_bytes = needle.as_bytes();
        if needle_bytes.is_empty() {
            continue;
        }

        let mut search_from = 0;
        while search_from + needle_bytes.len() <= lower.len() {
            let slice = &lower[search_from..];
            let Some(rel) = find_subsequence(slice, needle_bytes) else {
                break;
            };
            let abs = search_from + rel;

            // Reject matches that sit inside a longer identifier on
            // either side (left side: `user_password`; right side:
            // `password_id` — the trailing identifier byte means the
            // token is a different field name and we must not consume
            // its value as if it were the configured one).
            let prev_char = msg[..abs].chars().next_back().unwrap_or(' ');
            let after_field = abs + needle_bytes.len();
            let next_char = msg[after_field..].chars().next();
            if is_identifier_continuation(prev_char)
                || matches!(next_char, Some(c) if is_identifier_continuation(c))
            {
                search_from = abs + needle_bytes.len();
                continue;
            }

            // Locate the `=` or `:` separator after optional ASCII
            // whitespace. Anything else means this is a bare mention
            // of the field name (e.g. inside narrative prose) and the
            // value is not present, so leave it alone.
            let sep = find_separator(msg, after_field);
            let Some(value_start) = sep else {
                search_from = after_field;
                continue;
            };

            // Skip ASCII whitespace immediately after the separator.
            let value_start = msg[value_start..]
                .char_indices()
                .find(|(_, c)| !c.is_ascii_whitespace())
                .map(|(i, _)| value_start + i)
                .unwrap_or(msg.len());

            let value_end = msg[value_start..]
                .char_indices()
                .find(|(_, c)| is_value_terminator(*c))
                .map(|(i, _)| value_start + i)
                .unwrap_or(msg.len());

            if value_end > value_start {
                redactions.push((value_start, value_end));
            }
            search_from = value_end;
        }
    }

    if redactions.is_empty() {
        return;
    }

    // Apply in reverse so earlier offsets remain valid.
    redactions.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    let mut last_start = usize::MAX;
    for (start, end) in redactions {
        // Defensive: skip overlapping ranges produced by two field
        // names sharing a prefix (e.g. `password` and
        // `temporary_password`). With the boundary check above this is
        // not currently reachable, but the guard makes the loop
        // robust against future additions to `SENSITIVE_FIELDS`.
        if start >= last_start {
            continue;
        }
        last_start = start;
        msg.replace_range(start..end, "***");
    }
}

/// Locate the first `=` or `:` separator in `msg[from..]`, returning
/// the byte offset *after* the separator. Optional ASCII whitespace
/// between the field name and the separator is skipped. Returns `None`
/// if any other non-whitespace character is encountered first.
fn find_separator(msg: &str, from: usize) -> Option<usize> {
    for (i, ch) in msg[from..].char_indices() {
        if ch.is_ascii_whitespace() {
            continue;
        }
        if ch == '=' || ch == ':' {
            return Some(from + i + ch.len_utf8());
        }
        return None;
    }
    None
}

/// Naive byte-level subsequence search. Used instead of `str::find`
/// because we are searching the lowercase byte buffer and want strict
/// byte equality after the case fold.
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let last = haystack.len() - needle.len();
    (0..=last).find(|&i| &haystack[i..i + needle.len()] == needle)
}

/// Return the redacted form of a `controllerId`: the first three
/// (Unicode) characters followed by `***`. For inputs of three
/// characters or fewer, the entire input is returned with `***`
/// appended so we never silently disclose all of a short id.
///
/// Used by §19 / §10 logging paths (Requirement 19.9): logs may only
/// expose the `controllerId` prefix, never `controllerName` /
/// `controllerHwid` plain text.
pub(super) fn redact_controller_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len().min(16) + 3);
    let mut taken = 0usize;
    for ch in id.chars() {
        if taken == 3 {
            break;
        }
        out.push(ch);
        taken += 1;
    }
    out.push_str("***");
    out
}

#[inline]
fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Test-only helpers (tasks 10.4 / 10.5)
// ---------------------------------------------------------------------------

/// Reset the process-singleton log queue and `LOG_DROP` counter back
/// to a clean slate. Only callable from `#[cfg(test)]` code in this
/// module's child `tests` submodule. Property tests serialise via a
/// dedicated `Mutex<()>` lock so concurrent runs cannot observe a
/// half-cleared state.
///
/// Note: the `LOG_QUEUE` / `LOG_NOTIFY` `OnceLock`s are intentionally
/// not torn down — `OnceLock` has no `take` and the bound mutex /
/// notify cannot be re-set anyway. We mutate the queue *contents*
/// instead, which is what the property tests need.
#[cfg(test)]
pub(super) fn reset_log_state_for_tests() {
    // Initialise the OnceLocks if a previous test never ran the
    // production `install()` path, so the queue is reachable from the
    // first `enqueue` call.
    let queue = LOG_QUEUE.get_or_init(|| Mutex::new(LogQueue::new()));
    let _ = LOG_NOTIFY.get_or_init(Notify::new);

    let mut q = match queue.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    q.events.clear();
    q.bytes = 0;
    drop(q);

    LOG_DROP.store(0, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Tests (task 10.2)
//
// Property tests for the redaction / truncation paths live in tasks
// 10.4 / 10.5 (`Property 12` / `Property 19`). The unit tests below
// cover the example shapes called out in design.md §"Log Sink" and
// the field name list from Requirements 10.1 / 10.2 / 19.9.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn redact(input: &str) -> String {
        let mut s = input.to_owned();
        redact_message(&mut s);
        s
    }

    #[test]
    fn redacts_password_with_equals() {
        let out = redact("user logged in password=hunter2 ok");
        assert_eq!(out, "user logged in password=*** ok");
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn redacts_password_with_colon_and_space() {
        let out = redact("password: hunter2 next field");
        assert_eq!(out, "password: *** next field");
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn redacts_temporary_password_snake_and_camel_case() {
        let out_snake = redact("temporary_password=abc123 end");
        assert_eq!(out_snake, "temporary_password=*** end");
        let out_camel = redact("temporaryPassword: abc123, end");
        assert_eq!(out_camel, "temporaryPassword: ***, end");
    }

    #[test]
    fn redacts_proof_and_mac_fields() {
        let out = redact("frame mac=AAAA proof=BBBB end");
        assert!(out.contains("mac=***"));
        assert!(out.contains("proof=***"));
        assert!(!out.contains("AAAA"));
        assert!(!out.contains("BBBB"));
    }

    #[test]
    fn redacts_controller_name_and_hwid_fields() {
        let out = redact("auth controllerName=alice controllerHwid=Z9 done");
        assert!(out.contains("controllerName=***"));
        assert!(out.contains("controllerHwid=***"));
        assert!(!out.contains("alice"));
        assert!(!out.contains("Z9"));
    }

    #[test]
    fn redaction_is_case_insensitive_on_field_name() {
        let out = redact("PASSWORD=hunter2 PaSsWoRd: nope");
        assert!(out.starts_with("PASSWORD=***"));
        assert!(out.contains("PaSsWoRd: ***"));
        assert!(!out.contains("hunter2"));
        assert!(!out.contains("nope"));
    }

    #[test]
    fn does_not_match_substrings_inside_other_identifiers() {
        // The `password` keyword appears inside `user_password_id`,
        // which is a *different* identifier. We must not consume the
        // tail (`_id=42`) as if it were the password value.
        let out = redact("user_password_id=42 trailing");
        assert!(out.contains("user_password_id=42"));
        assert!(out.contains("trailing"));
    }

    #[test]
    fn does_not_match_when_field_name_is_followed_by_more_identifier_bytes() {
        // `passwordless` is not the `password` field even though it
        // shares the prefix.
        let out = redact("passwordless=true other=ok");
        assert_eq!(out, "passwordless=true other=ok");
    }

    #[test]
    fn leaves_message_unchanged_when_no_separator_follows_field_name() {
        let out = redact("the password was rotated successfully");
        assert_eq!(out, "the password was rotated successfully");
    }

    #[test]
    fn redacts_value_until_structural_terminator() {
        let out = redact("{password=hunter2,next:1}");
        assert!(out.contains("password=***"));
        assert!(out.contains(",next:"));
        // Note: `next:1` is not in SENSITIVE_FIELDS so it stays.
    }

    #[test]
    fn empty_string_is_a_noop() {
        let mut s = String::new();
        redact_message(&mut s);
        assert!(s.is_empty());
    }

    #[test]
    fn redacts_multiple_occurrences_in_one_message() {
        let out = redact(
            "first password=alpha second password=beta third password=gamma",
        );
        assert_eq!(
            out,
            "first password=*** second password=*** third password=***"
        );
        assert!(!out.contains("alpha"));
        assert!(!out.contains("beta"));
        assert!(!out.contains("gamma"));
    }

    #[test]
    fn redact_controller_id_short_input_appends_stars() {
        assert_eq!(redact_controller_id(""), "***");
        assert_eq!(redact_controller_id("1"), "1***");
        assert_eq!(redact_controller_id("12"), "12***");
        assert_eq!(redact_controller_id("123"), "123***");
    }

    #[test]
    fn redact_controller_id_long_input_truncates_to_three_chars() {
        assert_eq!(redact_controller_id("123456789"), "123***");
    }

    #[test]
    fn redact_controller_id_handles_multibyte_chars() {
        // The first three characters of "你好世界" should be the
        // first three Unicode characters, not the first three bytes
        // (which would split a UTF-8 sequence).
        let out = redact_controller_id("你好世界");
        assert_eq!(out, "你好世***");
    }

    #[test]
    fn truncate_to_bytes_preserves_utf8_boundary() {
        let mut s = "héllo".to_owned(); // 'é' is 2 bytes (0xC3 0xA9)
        truncate_to_bytes(&mut s, 2);
        // We must not stop in the middle of `é`; expected fall back
        // to the byte before the multi-byte char (offset 1).
        assert!(s.is_char_boundary(s.len()));
        assert!(s == "h" || s == "hé");
    }

    #[test]
    fn truncate_to_bytes_is_noop_when_already_short() {
        let mut s = "abc".to_owned();
        truncate_to_bytes(&mut s, 256);
        assert_eq!(s, "abc");
    }

    // -----------------------------------------------------------------
    // Property tests (tasks 10.4 / 10.5)
    //
    // `LOG_QUEUE` / `LOG_DROP` / `LOG_NOTIFY` are process-global
    // `OnceLock`s; concurrent property test cases must not interleave
    // their writes. A single `Mutex<()>` serialises every case across
    // all three proptest functions in this module.
    // -----------------------------------------------------------------

    use proptest::prelude::*;

    /// Process-singleton serialisation lock for log-sink proptests.
    /// Delegates to `super::observability::shared_test_lock()` so
    /// these tests share a single critical section with sibling
    /// modules that also mutate the `STATE_TX` watch channel
    /// (notably `observability::log_drop_count_inc_is_monotone` and
    /// `worker::worker_state_invariants_*`). Without this delegation
    /// the log-sink Property 19 test bumps `log_drop_count` via
    /// `worker::log_drop_count_inc` while a concurrent observability
    /// test reads it, racing against the monotonicity assertion.
    #[allow(non_snake_case)]
    fn LOG_TEST_LOCK() -> &'static std::sync::Mutex<()> {
        super::super::observability::shared_test_lock()
    }

    proptest! {
        // Property 19 (task 10.4)
        // Validates: Requirements 18.5, 18.6, 18.10
        #[test]
        fn log_queue_caps_events_at_4096_and_drop_count_matches(
            events_to_push in 1usize..=8192usize,
        ) {
            let _g = LOG_TEST_LOCK()
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            reset_log_state_for_tests();

            // After `reset_log_state_for_tests` the OnceLock is set,
            // so `get()` is unconditionally `Some`.
            let queue = LOG_QUEUE
                .get()
                .expect("LOG_QUEUE initialised by reset_log_state_for_tests");

            let before_drop = LOG_DROP.load(Ordering::Relaxed);
            let start = std::time::Instant::now();
            for i in 0..events_to_push {
                let event = LogEvent {
                    level: log::Level::Info,
                    target: format!("t{}", i),
                    message: format!("m{}", i),
                    timestamp_ms: i as u64,
                };
                enqueue(queue, event);
            }
            let elapsed = start.elapsed();

            // (1) Caller never blocks on back-pressure: 8192 enqueues
            // of small events must finish well within the property
            // test's per-case budget. 5 s is generous against any CI
            // host; a true back-pressure bug would take many seconds.
            prop_assert!(
                elapsed.as_secs() < 5,
                "enqueues took too long: {:?}",
                elapsed
            );

            // (2) Queue size and bytes never exceed their caps.
            let q = queue.lock().unwrap_or_else(|p| p.into_inner());
            prop_assert!(
                q.events.len() <= LOG_QUEUE_CAPACITY_EVENTS,
                "events.len()={} exceeds {}-event cap",
                q.events.len(),
                LOG_QUEUE_CAPACITY_EVENTS
            );
            prop_assert!(
                q.bytes <= LOG_QUEUE_CAPACITY_BYTES,
                "bytes={} exceeds {}-byte cap",
                q.bytes,
                LOG_QUEUE_CAPACITY_BYTES
            );

            // (3) `LOG_DROP` is monotone and matches the deterministic
            // expected number of evictions. With short `t{i}` /
            // `m{i}` payloads (≤ ~10 bytes/event), the byte cap never
            // triggers within 8192 events, so the only constraint is
            // the event-count cap.
            let after_drop = LOG_DROP.load(Ordering::Relaxed);
            prop_assert!(
                after_drop >= before_drop,
                "LOG_DROP went backwards: {} -> {}",
                before_drop,
                after_drop
            );
            let actual_dropped = after_drop - before_drop;
            let expected_dropped = events_to_push
                .saturating_sub(LOG_QUEUE_CAPACITY_EVENTS) as u64;
            prop_assert_eq!(
                actual_dropped,
                expected_dropped,
                "drop count mismatch (pushed={}, kept_cap={})",
                events_to_push,
                LOG_QUEUE_CAPACITY_EVENTS
            );

            // (4) Oldest-drop policy: when overflow, the surviving
            // events are exactly the LAST `LOG_QUEUE_CAPACITY_EVENTS`
            // pushed, so the front-most event identifies the cutoff
            // index unambiguously.
            if events_to_push > LOG_QUEUE_CAPACITY_EVENTS {
                let first_kept = q
                    .events
                    .front()
                    .expect("queue non-empty after overflow");
                let expected_first_kept_idx =
                    events_to_push - LOG_QUEUE_CAPACITY_EVENTS;
                let expected_target = format!("t{}", expected_first_kept_idx);
                prop_assert_eq!(
                    first_kept.target.as_str(),
                    expected_target.as_str(),
                    "oldest-drop policy violated"
                );
            }
        }
    }

    proptest! {
        // Property 12 (task 10.5)
        // Validates: Requirements 3.5, 3.7, 3.11, 10.1, 10.2, 10.3,
        //            10.6, 12.3, 18.7, 19.9
        //
        // Each generated value starts with a digit so it cannot equal
        // any name in `SENSITIVE_FIELDS` (all of which are pure
        // alpha/underscore). Charsets exclude value-terminators
        // (`,` `;` `}` `)` `"` `'` `>` whitespace) so the entire
        // generated value is consumed by the redactor — otherwise a
        // post-terminator suffix could survive in the output.
        #[test]
        fn redact_message_eliminates_sensitive_field_values(
            password in r"[1-9][A-Za-z0-9!#$%&+./=?@_~\-]{3,31}",
            proof_b64 in r"[1-9][A-Za-z0-9+/]{39,79}",
            mac_b64 in r"[1-9][A-Za-z0-9+/]{39,79}",
            hwid in r"[a-f0-9]{16,32}",
            controller_name in r"[1-9][A-Za-z0-9_@.]{2,31}",
        ) {
            let mut msg = format!(
                "auth password={} proof={} mac={} hwid={} controllerName={} done",
                password, proof_b64, mac_b64, hwid, controller_name
            );
            redact_message(&mut msg);

            // (1) No literal sensitive value survives in the output.
            prop_assert!(!msg.contains(&password), "password leaked: {}", msg);
            prop_assert!(!msg.contains(&proof_b64), "proof leaked: {}", msg);
            prop_assert!(!msg.contains(&mac_b64), "mac leaked: {}", msg);
            prop_assert!(!msg.contains(&hwid), "hwid leaked: {}", msg);
            prop_assert!(
                !msg.contains(&controller_name),
                "controllerName leaked: {}",
                msg
            );

            // (2) Each field is positively replaced with `***` at its
            //     `key=` position — the redaction actually fired.
            prop_assert!(
                msg.contains("password=***"),
                "password not replaced: {}",
                msg
            );
            prop_assert!(
                msg.contains("proof=***"),
                "proof not replaced: {}",
                msg
            );
            prop_assert!(msg.contains("mac=***"), "mac not replaced: {}", msg);
            prop_assert!(
                msg.contains("hwid=***"),
                "hwid not replaced: {}",
                msg
            );
            prop_assert!(
                msg.contains("controllerName=***"),
                "controllerName not replaced: {}",
                msg
            );
        }

        #[test]
        fn bridge_state_snapshot_serialization_only_exposes_secret_version(
            // Cover all six BridgeState variants discriminated by index
            // so we don't have to import each into scope as a generator.
            state_idx in 0usize..6,
            secret_version in any::<u32>(),
            log_drop_count in any::<u64>(),
            last_change_at_ms in any::<u64>(),
            // last_reason / error_code values are constrained to
            // digit-prefixed digit/underscore/dot patterns so they
            // cannot accidentally form English substrings that match
            // a forbidden field name (e.g. `mac` inside `machine`).
            // The structural property still covers the case the spec
            // cares about: the snapshot's *field names and values*
            // never include sensitive terms.
            last_reason in prop::option::of(r"[1-9][0-9_]{2,29}"),
            error_code in prop::option::of(r"[1-9][0-9_.]{7,39}"),
        ) {
            use super::super::{BridgeState, BridgeStateSnapshot};

            let state = match state_idx {
                0 => BridgeState::Disabled,
                1 => BridgeState::Initializing,
                2 => BridgeState::Connected,
                3 => BridgeState::Authorized,
                4 => BridgeState::Denied,
                _ => BridgeState::Failed,
            };
            let snap = BridgeStateSnapshot {
                state,
                last_reason,
                secret_version,
                log_drop_count,
                last_change_at_ms,
                error_code,
            };
            let json = serde_json::to_string(&snap)
                .expect("BridgeStateSnapshot must serialize");

            // No SENSITIVE_FIELDS substring may appear, case-insensitive.
            // The list mirrors the user-facing forbidden tokens from
            // §10 / §19; `secret_version` is *integer* and the field
            // *name* itself contains "secret" but never "shared_secret".
            let lower = json.to_ascii_lowercase();
            for forbidden in &[
                "password",
                "proof",
                "shared_secret",
                "rustdeskclientsharedsecret",
                "hwid",
                "controllername",
                "controllerhwid",
            ] {
                prop_assert!(
                    !lower.contains(forbidden),
                    "snapshot JSON leaked '{}': {}",
                    forbidden,
                    json
                );
            }
            // `mac` is a 3-letter substring that could collide with
            // common words in extended payloads; the only place it
            // could possibly appear in this snapshot's serialized
            // bytes is inside `last_reason` / `error_code` (whose
            // generators above forbid letters) or as part of a
            // hypothetical sensitive payload (which the type does
            // not carry). Keep the check explicit:
            prop_assert!(
                !lower.contains("mac"),
                "snapshot JSON leaked 'mac': {}",
                json
            );

            // The integer `secret_version` IS allowed and the field
            // MUST be present (positive confirmation that it was
            // emitted). Accept either snake_case or camelCase, since
            // serde-derive naming is configurable.
            prop_assert!(
                json.contains(r#""secret_version""#)
                    || json.contains(r#""secretVersion""#),
                "snapshot JSON missing secret_version field: {}",
                json
            );
        }

        #[test]
        fn redact_controller_id_only_exposes_first_three_chars(
            id in r"[A-Za-z0-9]{4,30}",
        ) {
            let red = redact_controller_id(&id);
            // Structural property from Requirement 19.9 / 12.3:
            // the redacted form is exactly the first three characters
            // followed by the literal sentinel `***`.
            let prefix: String = id.chars().take(3).collect();
            let expected = format!("{}***", prefix);
            prop_assert_eq!(red, expected);
        }
    }
}
