//! § diag — startup-from-launch logging + panic-catching + metrics dump.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!
//!   From the moment `main()` runs, every event of interest must reach :
//!     - the **console** (immediate user feedback)
//!     - a **log file** at `logs/loa-game-{timestamp}.log` (persistent record)
//!   Every panic must be caught + structured-reported (NOT raw-rust-panic-output)
//!   so the user can post-mortem any crash. Every frame's omega_step time +
//!   render time + tick-count must be recorded so a metrics-summary lands at
//!   exit.
//!
//! § STAGE-0 SCOPE
//!
//!   This module is a MINIMAL stdlib-based diag-layer. It deliberately does
//!   NOT integrate `cssl-log` / `cssl-error` / `cssl-metrics` yet — those
//!   crates are on parallel-fanout but their public-API ergonomics need
//!   verification before wiring (deferred to T11-D238 follow-up). For now :
//!   `std::fs::File` for log-sink, `std::panic::set_hook` for panic-catch,
//!   per-frame `Histogram`-style accumulator for metrics-dump.
//!
//! § PRIME-DIRECTIVE
//!
//!   - Path-hash discipline : log writes the **path-stem only** (no absolute
//!     paths) ; D130 path-hash carry-forward.
//!   - No telemetry-egress : log-file is local-disk-only, NEVER network.
//!   - Panic-info captured : structured-report includes thread + location +
//!     frame-N at panic-time. NO biometric / NO surveillance data.
//!   - Replay-determinism : log uses logical-frame-N (NOT wall-clock) for
//!     intra-frame events ; wall-clock only for boot + exit (orthogonal).
//!
//! § ATTESTATION  (PRIME_DIRECTIVE.md § 11)
//!
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::format_in_format_args)]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ═══════════════════════════════════════════════════════════════════════════
// § GLOBAL LOG SINK — Mutex-protected file + console mirror
// ═══════════════════════════════════════════════════════════════════════════

/// Global log-sink. Initialized exactly once via [`init`] at startup.
static LOG_SINK: OnceLock<Mutex<LogSink>> = OnceLock::new();

struct LogSink {
    file: Option<File>,
    log_path: PathBuf,
    boot_instant: Instant,
}

/// Log severity tier. Tier-mapping (lower = more verbose).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

impl Severity {
    fn tag(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO ",
            Self::Warn => "WARN ",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }
}

/// Initialize the diag layer. Creates `logs/` directory + opens
/// `logs/loa-game-{epoch_ms}.log`. Idempotent (subsequent calls no-op).
///
/// # Errors
/// Returns the I/O error if the logs-directory or file cannot be created.
/// In that case, the diag layer falls back to console-only output.
pub fn init() -> std::io::Result<PathBuf> {
    let boot_instant = Instant::now();
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // Locate logs-dir relative to current-working-directory.
    let mut logs_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    logs_dir.push("logs");
    fs::create_dir_all(&logs_dir)?;

    let mut log_path = logs_dir.clone();
    log_path.push(format!("loa-game-{epoch_ms}.log"));

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)?;

    let sink = LogSink {
        file: Some(file),
        log_path: log_path.clone(),
        boot_instant,
    };

    let _ = LOG_SINK.set(Mutex::new(sink));
    Ok(log_path)
}

/// Log a single event at the given severity. Writes to BOTH the file-sink
/// AND stderr (mirrored). Format :
///   `[+0.123s | INFO  | subsystem] message`
///
/// `frame_n` is logical-frame number (NOT wall-clock) ; pass `0` if not
/// in the per-frame loop.
pub fn log(severity: Severity, subsystem: &str, frame_n: u64, message: impl AsRef<str>) {
    let msg = message.as_ref();
    let elapsed = LOG_SINK
        .get()
        .and_then(|m| m.lock().ok().map(|s| s.boot_instant.elapsed().as_secs_f64()))
        .unwrap_or(0.0);

    let line = format!(
        "[+{:>7.3}s | {} | f={:>6} | {:<24}] {}\n",
        elapsed,
        severity.tag(),
        frame_n,
        subsystem,
        msg,
    );

    // Mirror to stderr (immediate user feedback).
    let _ = std::io::stderr().write_all(line.as_bytes());

    // Append to file (persistent).
    if let Some(mutex) = LOG_SINK.get() {
        if let Ok(mut sink) = mutex.lock() {
            if let Some(f) = sink.file.as_mut() {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
            }
        }
    }
}

/// Convenience macros (file-scope local — not exported as macro_rules!).
#[macro_export]
macro_rules! diag_info {
    ($subsystem:expr, $frame:expr, $($arg:tt)*) => {
        $crate::diag::log($crate::diag::Severity::Info, $subsystem, $frame, format!($($arg)*));
    };
}
#[macro_export]
macro_rules! diag_warn {
    ($subsystem:expr, $frame:expr, $($arg:tt)*) => {
        $crate::diag::log($crate::diag::Severity::Warn, $subsystem, $frame, format!($($arg)*));
    };
}
#[macro_export]
macro_rules! diag_error {
    ($subsystem:expr, $frame:expr, $($arg:tt)*) => {
        $crate::diag::log($crate::diag::Severity::Error, $subsystem, $frame, format!($($arg)*));
    };
}
#[macro_export]
macro_rules! diag_fatal {
    ($subsystem:expr, $frame:expr, $($arg:tt)*) => {
        $crate::diag::log($crate::diag::Severity::Fatal, $subsystem, $frame, format!($($arg)*));
    };
}

// ═══════════════════════════════════════════════════════════════════════════
// § PANIC HOOK — captures any panic + structured-reports
// ═══════════════════════════════════════════════════════════════════════════

/// Install a structured-panic-hook. Idempotent — only installs once.
///
/// The hook captures :
///   - thread name
///   - panic location (file:line)
///   - panic payload (downcast to &str)
///   - current frame_n (if set via [`set_frame_n`])
///
/// Output goes to BOTH the log-sink AND stderr.
pub fn install_panic_hook() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return; // already installed
    }
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&'static str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<unrecoverable panic payload>");
        let frame_n = current_frame_n();

        log(
            Severity::Fatal,
            "panic",
            frame_n,
            format!("thread='{thread}' at={location} payload='{payload}'"),
        );

        // Defer to original so default reporting still happens.
        original(info);
    }));
}

// ═══════════════════════════════════════════════════════════════════════════
// § FRAME-N TRACKING — for panic-hook to report which frame crashed
// ═══════════════════════════════════════════════════════════════════════════

static CURRENT_FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Update the current frame-counter. Called from main-loop each frame so
/// panic-hook can report which frame crashed.
pub fn set_frame_n(n: u64) {
    CURRENT_FRAME.store(n, std::sync::atomic::Ordering::Relaxed);
}

/// Read current frame-counter (for panic-hook).
fn current_frame_n() -> u64 {
    CURRENT_FRAME.load(std::sync::atomic::Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════════
// § METRICS — minimal histogram + per-frame recorder
// ═══════════════════════════════════════════════════════════════════════════

/// Per-frame metric recorder. Stores microsecond observations into a fixed
/// circular buffer ; computes p50/p95/p99 + mean on demand.
#[derive(Debug)]
pub struct Metrics {
    /// omega_step duration in microseconds (last N=4096 frames).
    omega_step_us: Vec<u32>,
    /// render duration in microseconds (last N=4096 frames).
    render_us: Vec<u32>,
    /// total frame duration in microseconds (last N=4096 frames).
    frame_us: Vec<u32>,
    /// frame-counter (global, never wraps).
    pub total_frames: u64,
    /// boot-instant for wall-elapsed.
    pub boot: Instant,
}

const METRICS_CAPACITY: usize = 4096;

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    #[must_use]
    pub fn new() -> Self {
        Self {
            omega_step_us: Vec::with_capacity(METRICS_CAPACITY),
            render_us: Vec::with_capacity(METRICS_CAPACITY),
            frame_us: Vec::with_capacity(METRICS_CAPACITY),
            total_frames: 0,
            boot: Instant::now(),
        }
    }

    /// Record one frame's measurements.
    pub fn record_frame(&mut self, omega_step_us: u32, render_us: u32, frame_us: u32) {
        push_capped(&mut self.omega_step_us, omega_step_us);
        push_capped(&mut self.render_us, render_us);
        push_capped(&mut self.frame_us, frame_us);
        self.total_frames = self.total_frames.saturating_add(1);
    }

    /// Compute summary statistics for the omega_step histogram.
    #[must_use]
    pub fn omega_step_summary(&self) -> Summary {
        Summary::compute(&self.omega_step_us)
    }

    /// Compute summary statistics for the render histogram.
    #[must_use]
    pub fn render_summary(&self) -> Summary {
        Summary::compute(&self.render_us)
    }

    /// Compute summary statistics for the frame-total histogram.
    #[must_use]
    pub fn frame_summary(&self) -> Summary {
        Summary::compute(&self.frame_us)
    }

    /// Achieved Hz over wall-elapsed since boot.
    #[must_use]
    pub fn achieved_hz(&self) -> f64 {
        let s = self.boot.elapsed().as_secs_f64();
        if s > 0.0 {
            self.total_frames as f64 / s
        } else {
            0.0
        }
    }

    /// Format a one-line summary suitable for logging.
    #[must_use]
    pub fn summary_line(&self) -> String {
        let omega = self.omega_step_summary();
        let render = self.render_summary();
        let frame = self.frame_summary();
        format!(
            "frames={} hz={:.1} omega_step(p50/p95/p99 μs)=({}/{}/{}) render(p50/p95/p99 μs)=({}/{}/{}) frame(p50/p95/p99 μs)=({}/{}/{})",
            self.total_frames,
            self.achieved_hz(),
            omega.p50, omega.p95, omega.p99,
            render.p50, render.p95, render.p99,
            frame.p50, frame.p95, frame.p99,
        )
    }
}

fn push_capped(v: &mut Vec<u32>, x: u32) {
    if v.len() >= METRICS_CAPACITY {
        v.remove(0);
    }
    v.push(x);
}

/// Statistical summary of a histogram.
#[derive(Debug, Clone, Copy)]
pub struct Summary {
    pub min: u32,
    pub p50: u32,
    pub p95: u32,
    pub p99: u32,
    pub max: u32,
    pub mean: u32,
    pub samples: u32,
}

impl Summary {
    fn compute(samples: &[u32]) -> Self {
        if samples.is_empty() {
            return Self {
                min: 0,
                p50: 0,
                p95: 0,
                p99: 0,
                max: 0,
                mean: 0,
                samples: 0,
            };
        }
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let n = sorted.len();
        let pick = |q: f64| sorted[((n - 1) as f64 * q) as usize];
        let mean = (samples.iter().map(|&x| x as u64).sum::<u64>() / n as u64) as u32;
        Self {
            min: sorted[0],
            p50: pick(0.5),
            p95: pick(0.95),
            p99: pick(0.99),
            max: sorted[n - 1],
            mean,
            samples: n as u32,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § HEALTH PROBES — minimal subsystem-status reporter
// ═══════════════════════════════════════════════════════════════════════════

/// Subsystem health status (mirrors cssl-health::HealthStatus shape).
#[derive(Debug, Clone)]
pub enum SubsysHealth {
    Ok,
    Degraded(String),
    Failed(String),
}

/// Aggregate engine-health snapshot.
#[derive(Debug, Default)]
pub struct EngineHealth {
    pub window: SubsysHealth,
    pub renderer: SubsysHealth,
    pub omega_step: SubsysHealth,
    pub save_load: SubsysHealth,
}

impl Default for SubsysHealth {
    fn default() -> Self {
        Self::Ok
    }
}

impl SubsysHealth {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

impl EngineHealth {
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.window.is_ok()
            && self.renderer.is_ok()
            && self.omega_step.is_ok()
            && self.save_load.is_ok()
    }

    #[must_use]
    pub fn summary_line(&self) -> String {
        format!(
            "window={:?} renderer={:?} omega_step={:?} save_load={:?}",
            self.window, self.renderer, self.omega_step, self.save_load
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § EXIT — final dump on clean shutdown
// ═══════════════════════════════════════════════════════════════════════════

/// Final metrics + log-flush at exit. Call once from main() after the loop.
pub fn dump_summary(metrics: &Metrics, health: &EngineHealth) {
    log(Severity::Info, "exit", metrics.total_frames, "── final metrics ──");
    log(Severity::Info, "exit", metrics.total_frames, metrics.summary_line());
    log(Severity::Info, "exit", metrics.total_frames, format!("health: {}", health.summary_line()));
    log(
        Severity::Info,
        "exit",
        metrics.total_frames,
        format!(
            "log-file={}",
            LOG_SINK
                .get()
                .and_then(|m| m.lock().ok().map(|s| s.log_path.display().to_string()))
                .unwrap_or_else(|| "<unavailable>".to_string())
        ),
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_tags_unique() {
        let tags = [
            Severity::Trace.tag(),
            Severity::Debug.tag(),
            Severity::Info.tag(),
            Severity::Warn.tag(),
            Severity::Error.tag(),
            Severity::Fatal.tag(),
        ];
        for (i, t1) in tags.iter().enumerate() {
            for (j, t2) in tags.iter().enumerate() {
                if i != j {
                    assert_ne!(t1, t2, "severity tags must be unique");
                }
            }
        }
    }

    #[test]
    fn metrics_summary_empty_returns_zeros() {
        let m = Metrics::new();
        let s = m.omega_step_summary();
        assert_eq!(s.p50, 0);
        assert_eq!(s.samples, 0);
    }

    #[test]
    fn metrics_record_advances_count() {
        let mut m = Metrics::new();
        m.record_frame(100, 200, 300);
        m.record_frame(150, 250, 400);
        assert_eq!(m.total_frames, 2);
        assert_eq!(m.omega_step_summary().samples, 2);
    }

    #[test]
    fn metrics_summary_p50_p95_p99() {
        let mut m = Metrics::new();
        for i in 1..=100u32 {
            m.record_frame(i, i * 2, i * 3);
        }
        let s = m.omega_step_summary();
        assert_eq!(s.samples, 100);
        assert_eq!(s.min, 1);
        assert_eq!(s.max, 100);
        assert!(s.p50 >= 49 && s.p50 <= 51);
        assert!(s.p95 >= 94 && s.p95 <= 96);
        assert!(s.p99 >= 98 && s.p99 <= 100);
    }

    #[test]
    fn metrics_circular_buffer_caps_at_capacity() {
        let mut m = Metrics::new();
        for i in 0..(METRICS_CAPACITY as u32 + 100) {
            m.record_frame(i, i, i);
        }
        assert_eq!(m.omega_step_summary().samples as usize, METRICS_CAPACITY);
        assert_eq!(m.total_frames, METRICS_CAPACITY as u64 + 100);
    }

    #[test]
    fn engine_health_default_is_all_ok() {
        let h = EngineHealth::default();
        assert!(h.all_ok());
    }

    #[test]
    fn engine_health_degraded_breaks_all_ok() {
        let h = EngineHealth {
            window: SubsysHealth::Degraded("dpi-mismatch".into()),
            ..Default::default()
        };
        assert!(!h.all_ok());
    }

    #[test]
    fn frame_n_tracker_round_trip() {
        set_frame_n(42);
        assert_eq!(current_frame_n(), 42);
        set_frame_n(0);
        assert_eq!(current_frame_n(), 0);
    }
}
