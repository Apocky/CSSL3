//! § cssl-rt::events — structured-event observability for FFI entry-points
//! ═════════════════════════════════════════════════════════════════════════
//!
//! § T11-W19-β-FS-EVENT-JSONL
//!
//! § ROLE
//!   Emit one canonical JSONL event per FFI entry/exit/branch/skip/error
//!   into `%TEMP%\cssl_events.jsonl` (Windows) or `/tmp/cssl_events.jsonl`
//!   (Unix). Append-only · process-survival · audit-friendly.
//!
//! § SCHEMA
//!   ```jsonc
//!   {
//!     "ts_ns": 1777918507138123456,        // SystemTime::now → UNIX_EPOCH ns
//!     "src":   "cssl-rt::host_window",     // module that emitted
//!     "op":    "window.spawn",             // <domain>.<verb> taxonomy
//!     "kind":  "entry",                    // entry|exit|branch|skip|error
//!     "args":  { ... },                    // call-site args (always object)
//!     "result": null | { ... },            // exit/error : the produced value
//!     "latency_ns": null | u64,            // exit/error : ns from matching entry
//!     "note":  null | "string"             // skip/branch : human reason
//!   }
//!   ```
//!
//! § BACKWARDS-COMPAT
//!   Every JSONL emit ALSO writes a one-line `cssl_trace.log` summary so
//!   existing trace consumers ¬→ break. The text format mirrors the
//!   pre-T11-W19-β `host_window::fs_trace` shape.
//!
//! § PRIME-DIRECTIVE
//!   No data egresses cross-process — both sinks are local files. The
//!   events carry FFI-call shape (handles, lengths, error-codes), never
//!   the raw payload bytes (e.g. audio samples, IR blobs, network bytes
//!   are NOT logged ; only their lengths + handles).
//!
//! § SAWYER-EFFICIENCY
//!   Each emit opens, appends, sync_data, closes. No background flusher
//!   thread, no buffering layer that would lose data on `__cssl_abort`.
//!   The trade-off : ~one syscall per FFI call. At stage-0 game loop
//!   rates (60 Hz × ~5 calls/frame ≈ 300 emits/s) this is well under
//!   the noise floor of the calls being measured.
#![allow(dead_code, unreachable_pub)]

use serde_json::Value;
use std::io::Write as _;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// Returns the absolute path to the JSONL event sink.
///
/// `%TEMP%\cssl_events.jsonl` on Windows ; `/tmp/cssl_events.jsonl` on
/// Unix. Created on first append.
fn jsonl_path() -> std::path::PathBuf {
    std::env::temp_dir().join("cssl_events.jsonl")
}

/// Returns the absolute path to the legacy text-trace sink (kept for
/// backwards compat with the pre-T11-W19-β `host_window::fs_trace` log
/// consumer scripts).
fn trace_path() -> std::path::PathBuf {
    std::env::temp_dir().join("cssl_trace.log")
}

/// Wall-clock nanoseconds since UNIX epoch. Saturates at 0 on pre-epoch
/// host clocks (which should not exist in practice).
fn now_unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

/// Wall-clock milliseconds since UNIX epoch (for the legacy `cssl_trace.log`
/// text format ; matches the historical `host_window::fs_trace` shape).
fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Append one canonical JSONL event to `cssl_events.jsonl` AND a
/// human-readable text summary to `cssl_trace.log`.
///
/// `src` should be the module path ("cssl-rt::host_gpu", etc.). `op` is
/// the `<domain>.<verb>` taxonomy ("gpu.device_create", etc.). `kind` is
/// one of `entry`/`exit`/`branch`/`skip`/`error`.
///
/// All filesystem errors are silently ignored — the event sink is a
/// best-effort observability channel and MUST NOT alter FFI semantics.
pub fn fs_event_jsonl(
    src: &str,
    op: &str,
    kind: &str,
    args: Value,
    result: Option<Value>,
    latency_ns: Option<u64>,
    note: Option<&str>,
) {
    let ts_ns = now_unix_ns();
    let result_v = result.clone().unwrap_or(Value::Null);
    let latency_v = latency_ns.map_or(Value::Null, |n| Value::from(n));
    let note_v = note.map_or(Value::Null, |s| Value::String(s.to_string()));
    let ev = serde_json::json!({
        "ts_ns":      ts_ns,
        "src":        src,
        "op":         op,
        "kind":       kind,
        "args":       args,
        "result":     result_v,
        "latency_ns": latency_v,
        "note":       note_v,
    });
    // 1. JSONL sink — one line per event.
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(jsonl_path())
    {
        if let Ok(serialized) = serde_json::to_string(&ev) {
            let _ = writeln!(f, "{}", serialized);
            let _ = f.sync_data();
        }
    }
    // 2. Legacy text sink — same data in the historical fs_trace shape.
    //    Format: `<unix_ms> <src> <op> <kind> <args>[ result=<r>][ lat=<ns>ns][ note=<n>]`
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_path())
    {
        let mut line = format!("{} {} {} {} args={}", now_unix_ms(), src, op, kind, args);
        if let Some(r) = &result {
            line.push_str(&format!(" result={}", r));
        }
        if let Some(l) = latency_ns {
            line.push_str(&format!(" lat={}ns", l));
        }
        if let Some(n) = note {
            line.push_str(&format!(" note={}", n));
        }
        let _ = writeln!(f, "{}", line);
        let _ = f.sync_data();
    }
}

/// RAII helper : emits an `entry` event on construction, then `exit` /
/// `error` / `skip` on the matching method.
///
/// § USAGE
/// ```ignore
/// let mut sc = EventScope::new(
///     "cssl-rt::host_gpu",
///     "gpu.device_create",
///     serde_json::json!({"adapter_idx": adapter_idx, "flags": flags}),
/// );
/// // … work …
/// sc.success(serde_json::json!({"handle": handle}));
/// ```
pub struct EventScope {
    src: &'static str,
    op: &'static str,
    args: Value,
    start: Instant,
    /// Set once the scope has been finalized via `success`/`error`/`skip`
    /// to avoid double-emission if the caller forgets to call one of them
    /// AND the scope is dropped (Drop emits a synthetic skip).
    closed: bool,
}

impl EventScope {
    /// Construct a scope + emit the `entry` event immediately. The
    /// returned handle's `success` / `error` / `skip` methods finalize
    /// the scope ; if none is called the `Drop` impl emits a fallback
    /// `skip` with note `"scope-dropped-without-finalize"` (defensive
    /// against future refactors that miss a return path).
    pub fn new(src: &'static str, op: &'static str, args: Value) -> Self {
        fs_event_jsonl(src, op, "entry", args.clone(), None, None, None);
        Self {
            src,
            op,
            args,
            start: Instant::now(),
            closed: false,
        }
    }

    /// Emit an `exit` event with the given result + measured latency.
    pub fn success(mut self, result: Value) {
        let lat = self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        fs_event_jsonl(
            self.src,
            self.op,
            "exit",
            self.args.clone(),
            Some(result),
            Some(lat),
            None,
        );
        self.closed = true;
    }

    /// Emit an `error` event with the given result-sentinel + latency.
    pub fn error(mut self, result: Value, note: Option<&str>) {
        let lat = self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        fs_event_jsonl(
            self.src,
            self.op,
            "error",
            self.args.clone(),
            Some(result),
            Some(lat),
            note,
        );
        self.closed = true;
    }

    /// Emit a `skip` event for a silent-fallback path (e.g. real-driver
    /// returned None ; we fell through to stub).
    pub fn skip(mut self, note: &str) {
        let lat = self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        fs_event_jsonl(
            self.src,
            self.op,
            "skip",
            self.args.clone(),
            None,
            Some(lat),
            Some(note),
        );
        self.closed = true;
    }

    /// Emit a `branch` event for an internal decision-point WITHOUT
    /// finalizing the scope. The matching `success`/`error`/`skip` still
    /// must fire to close the entry/exit pair.
    pub fn branch(&self, note: &str) {
        fs_event_jsonl(
            self.src,
            self.op,
            "branch",
            self.args.clone(),
            None,
            None,
            Some(note),
        );
    }
}

impl Drop for EventScope {
    fn drop(&mut self) {
        if !self.closed {
            // Defensive fallback : a scope that was created but never
            // explicitly closed gets a `skip` with a diagnostic note. This
            // surfaces forgotten-return-path bugs in the instrumentation
            // itself rather than silently dropping events.
            let lat = self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            fs_event_jsonl(
                self.src,
                self.op,
                "skip",
                self.args.clone(),
                None,
                Some(lat),
                Some("scope-dropped-without-finalize"),
            );
        }
    }
}

/// § spec-70 § item-05 (A05.2) : register an observable side-effect for the
/// run-and-observe gate. Tests call this to opt-in as "alive" even when they
/// don't otherwise touch the FFI surface (e.g. pure compute / unit logic).
///
/// Emits a single JSONL `branch` event with op = `test.observe` and
/// `args = {"name": <name>}`. The verifier (`cssl-test-verifier::verify`)
/// counts these toward the `require-observable` gate.
///
/// This is an opt-in mechanism : tests that ARE expected to be silent (e.g.
/// idle-cycle benchmarks per OQ.03) simply omit the `require-observable`
/// line in their manifest and never call this function.
pub fn cssl_test_observe(name: &str) {
    fs_event_jsonl(
        "cssl-rt::test_harness",
        "test.observe",
        "branch",
        serde_json::json!({"name": name}),
        None,
        None,
        Some("cssl_test_observe"),
    );
}
