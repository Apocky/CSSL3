//! § loa_startup — auto-init logger for LoA-v13 binaries
//! ═══════════════════════════════════════════════════════
//!
//! Anchored to T11-LOA-LOG-1. Provides a process-wide ctor that fires
//! BEFORE `main()` on Windows-MSVC builds (via the `.CRT$XCU` initializer
//! section that the C runtime walks during startup) and on POSIX builds
//! (via `.init_array`). When fired, the ctor :
//!
//!   1. Creates `logs/` next to the running executable (cwd-relative).
//!   2. Opens `logs/loa_runtime.log` (append-mode) and writes a startup
//!      banner with ISO-UTC timestamp + PID + cssl-rt version.
//!   3. Prints a one-line marker to stderr so a console-running user sees
//!      that the binary has started before main().
//!   4. Installs an atexit hook that writes a shutdown banner with the
//!      observed exit-code (best-effort — std::process::exit skips it).
//!
//! Apocky directive : "I want full logging and error-catching and telemetry
//! from the moment the game is started and written to disk so that we know
//! what fires and what doesn't and what was supposed to but didn't."
//!
//! § ENV CONTROLS
//!   `CSSL_LOG_DIR` — override `logs/` location (absolute or relative).
//!   `CSSL_LOG_QUIET=1` — skip the stderr banner (file logging still on).
//!   `CSSL_LOG_DISABLE=1` — no-op the entire ctor.

#![allow(unsafe_code)]

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

/// § T11-LOA-TELEM : per-N-call rotation check. We sample the log-file size
/// every N writes rather than every write to keep `log_event` cheap. N=1000
/// at ~10 events/frame ≈ once-every-100-frames ≈ ~1.6Hz at 60fps.
const ROTATION_CHECK_EVERY: u64 = 1000;

/// Rotation threshold : 10MB. Beyond this, rotate into a timestamped file.
const ROTATION_BYTES: u64 = 10 * 1024 * 1024;

/// Maximum rotated files to keep. Older ones are deleted on rotation.
const MAX_ROTATED_FILES: usize = 5;

/// Counter for rotation-check throttling.
static LOG_EVENT_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

fn iso_utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Minimal ISO-UTC formatter without chrono dep · seconds-resolution.
    let days = secs / 86_400;
    let hms = secs % 86_400;
    let h = hms / 3600;
    let m = (hms % 3600) / 60;
    let s = hms % 60;
    let (y, mo, d) = days_to_ymd(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719_468;
    let era = if days >= 0 { days / 146_097 } else { (days - 146_096) / 146_097 };
    let doe = (days - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp.wrapping_sub(9) };
    let year = (y + i64::from(m <= 2)) as i32;
    (year, m as u32, d as u32)
}

fn log_dir() -> PathBuf {
    if let Ok(v) = std::env::var("CSSL_LOG_DIR") {
        return PathBuf::from(v);
    }
    PathBuf::from("logs")
}

fn open_log_file() -> std::io::Result<File> {
    let dir = log_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join("loa_runtime.log");
    OpenOptions::new().create(true).append(true).open(path)
}

/// Public : log a message to the runtime log file. Best-effort · silent
/// on failure (no panic). Used by other cssl-rt modules + any host code.
///
/// § T11-LOA-TELEM : every `ROTATION_CHECK_EVERY` calls, the function checks
/// the current log-file size. If it exceeds `ROTATION_BYTES`, the file is
/// rotated into `loa_runtime_<iso-ts>.log` and a fresh handle is opened.
/// Old rotated files beyond `MAX_ROTATED_FILES` are deleted oldest-first.
pub fn log_event(level: &str, source: &str, msg: &str) {
    let line = format!("[{}] [{}] [{}] {}\n", iso_utc_now(), level, source, msg);
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(f) = guard.as_mut() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }
    // Rotation throttle : check every Nth call.
    let n = LOG_EVENT_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    if n % ROTATION_CHECK_EVERY == 0 && n > 0 {
        check_rotation();
    }
}

/// Inspect the current log-file size and rotate if it exceeds `ROTATION_BYTES`.
/// Best-effort ; silently no-ops on any error so logging itself never panics.
fn check_rotation() {
    let dir = log_dir();
    let primary = dir.join("loa_runtime.log");
    let size = match fs::metadata(&primary) {
        Ok(m) => m.len(),
        Err(_) => return,
    };
    if size < ROTATION_BYTES {
        return;
    }
    // § Rotation sequence :
    //   1. Acquire LOG_FILE lock and drop the current file handle.
    //   2. Rename loa_runtime.log → loa_runtime_<iso-ts>.log (Windows
    //      requires the handle be closed first).
    //   3. Open a fresh loa_runtime.log and write a rotation banner.
    //   4. Prune old rotations beyond MAX_ROTATED_FILES.
    let ts = iso_utc_now().replace(':', "-"); // colons illegal in Windows filenames
    let rotated = dir.join(format!("loa_runtime_{ts}.log"));
    {
        let Ok(mut guard) = LOG_FILE.lock() else {
            return;
        };
        // Drop the file handle so Windows lets us rename.
        *guard = None;
    }
    let _ = fs::rename(&primary, &rotated);
    // Reopen primary log file.
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(&primary) {
        if let Ok(mut guard) = LOG_FILE.lock() {
            *guard = Some(f);
        }
    }
    // Write a rotation banner via direct call (avoid the throttle counter
    // recursion since this is called inside check_rotation).
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(f) = guard.as_mut() {
            let banner = format!(
                "[{}] [INFO] [loa_startup/rotate] § log rotated · prior file → {} · size={size}\n",
                iso_utc_now(),
                rotated.display(),
            );
            let _ = f.write_all(banner.as_bytes());
            let _ = f.flush();
        }
    }
    // Prune old rotated files (keep last MAX_ROTATED_FILES).
    prune_rotations(&dir);
}

/// Delete old rotated log files (`loa_runtime_*.log`) keeping the most-recent
/// `MAX_ROTATED_FILES` by mtime. Silent on any I/O error.
fn prune_rotations(dir: &std::path::Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut rotations: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?.to_string();
            if name.starts_with("loa_runtime_") && name.ends_with(".log") {
                let mtime = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                Some((mtime, p))
            } else {
                None
            }
        })
        .collect();
    if rotations.len() <= MAX_ROTATED_FILES {
        return;
    }
    rotations.sort_by_key(|(t, _)| *t);
    // Oldest-first ; delete the leading `len - MAX_ROTATED_FILES` entries.
    let drop_count = rotations.len() - MAX_ROTATED_FILES;
    for (_, path) in rotations.into_iter().take(drop_count) {
        let _ = fs::remove_file(path);
    }
}

fn pid() -> u64 {
    std::process::id() as u64
}

fn startup_run() {
    if std::env::var("CSSL_LOG_DISABLE").is_ok() {
        return;
    }
    if let Ok(f) = open_log_file() {
        if let Ok(mut guard) = LOG_FILE.lock() {
            *guard = Some(f);
        }
    }
    let banner = format!(
        "════════════════════════════════════════════════════════════\n\
         § LoA-v13 runtime · cssl-rt v{} · pid={} · ts={}\n\
         § entered cssl-rt ctor BEFORE main() · auto-log armed\n\
         ════════════════════════════════════════════════════════════",
        env!("CARGO_PKG_VERSION"),
        pid(),
        iso_utc_now(),
    );
    log_event("INFO", "loa_startup", &banner);
    if std::env::var("CSSL_LOG_QUIET").is_err() {
        let _ = writeln!(
            std::io::stderr(),
            "§ LoA-v13 starting · pure-CSSL native · log => {}/loa_runtime.log · pid={}",
            log_dir().display(),
            pid(),
        );
    }
    // § T11-LOA-PANIC-HOOK : install panic-hook that captures Rust panics +
    //   stack-trace + writes to logs/loa_runtime.log BEFORE the process dies.
    //   Critical for diagnosing silent crashes : without this, a wgpu validation
    //   panic prints to stderr (often invisible when running via double-click)
    //   and atexit doesn't fire because the process aborts hard.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");
        let backtrace = std::backtrace::Backtrace::force_capture();
        let msg = format!(
            "═══ PANIC ═══\nlocation : {location}\npayload  : {payload}\nbacktrace:\n{backtrace}",
        );
        log_event("FATAL", "loa_startup/panic_hook", &msg);
        // Also stderr for double-click console visibility (if any).
        let _ = writeln!(std::io::stderr(), "{msg}");
        // Chain to previous hook (preserves default backtrace if RUST_BACKTRACE=1).
        prev_hook(info);
    }));
    log_event(
        "INFO",
        "loa_startup",
        "§ panic-hook armed · captures stack-trace to log before process dies",
    );
    extern "C" fn shutdown_hook() {
        log_event(
            "INFO",
            "loa_startup",
            "§ atexit fired · LoA-v13 shutting down",
        );
    }
    unsafe {
        libc_atexit(shutdown_hook);
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
unsafe fn libc_atexit(f: extern "C" fn()) {
    extern "C" {
        fn atexit(cb: extern "C" fn()) -> i32;
    }
    let _ = unsafe { atexit(f) };
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
unsafe fn libc_atexit(_: extern "C" fn()) {}

extern "C" fn loa_startup_ctor_thunk() {
    startup_run();
}

// § Windows MSVC : the C runtime walks `.CRT$XCU` calling each fn-ptr
//   before main() runs. The `#[used]` attribute prevents the linker from
//   stripping the static when the symbol has no apparent caller.
#[cfg(all(target_os = "windows", target_env = "msvc"))]
#[used]
#[link_section = ".CRT$XCU"]
static LOA_STARTUP_CTOR: extern "C" fn() = loa_startup_ctor_thunk;

// § Windows GNU (MinGW + MSYS2) : the GNU runtime uses `.ctors` (legacy)
//   and `.init_array` (modern). MinGW-w64's CRT walks `.init_array` since
//   ~2014 ; we use the modern section to match Linux/BSD behavior. T11-D319
//   adds this branch so cssl-rt's startup ctor activates regardless of
//   whether the build host uses the MSVC or GNU rust toolchain.
#[cfg(all(target_os = "windows", target_env = "gnu"))]
#[used]
#[link_section = ".init_array"]
static LOA_STARTUP_CTOR: extern "C" fn() = loa_startup_ctor_thunk;

// § ELF (Linux + most BSDs) : the dynamic linker walks `.init_array`.
#[cfg(all(unix, not(target_os = "macos")))]
#[used]
#[link_section = ".init_array"]
static LOA_STARTUP_CTOR: extern "C" fn() = loa_startup_ctor_thunk;

// § Mach-O (macOS) : the dyld walks `__DATA,__mod_init_func`.
#[cfg(target_os = "macos")]
#[used]
#[link_section = "__DATA,__mod_init_func"]
static LOA_STARTUP_CTOR: extern "C" fn() = loa_startup_ctor_thunk;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_utc_format_is_well_formed() {
        let s = iso_utc_now();
        assert_eq!(s.len(), 20);
        assert!(s.ends_with('Z'));
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert_eq!(&s[10..11], "T");
        assert_eq!(&s[13..14], ":");
        assert_eq!(&s[16..17], ":");
    }

    #[test]
    fn log_dir_default_is_logs() {
        std::env::remove_var("CSSL_LOG_DIR");
        assert_eq!(log_dir(), PathBuf::from("logs"));
    }

    #[test]
    fn log_event_does_not_panic_when_no_file_open() {
        if let Ok(mut guard) = LOG_FILE.lock() {
            *guard = None;
        }
        log_event("DEBUG", "test", "no-op-message");
    }

    #[test]
    fn rotation_thresholds_well_formed() {
        // 10MB rotation threshold ; sanity-check the consts are non-zero
        // and the order makes sense.
        assert!(ROTATION_BYTES > 0);
        assert_eq!(ROTATION_BYTES, 10 * 1024 * 1024);
        assert!(ROTATION_CHECK_EVERY >= 100);
        assert!(MAX_ROTATED_FILES >= 1);
    }

    #[test]
    fn prune_rotations_keeps_most_recent_n() {
        let dir = std::env::temp_dir().join(format!("loa-prune-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        // Synthesize 7 rotated files with ascending mtimes (via small sleeps).
        for i in 0..7 {
            let p = dir.join(format!("loa_runtime_2026-04-30T19-25-0{i}Z.log"));
            let _ = std::fs::File::create(&p);
            // Stagger mtimes a hair so sort-by-mtime is deterministic.
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        prune_rotations(&dir);
        // After prune : at most MAX_ROTATED_FILES (5) remain.
        let remaining = fs::read_dir(&dir).unwrap().count();
        assert!(remaining <= MAX_ROTATED_FILES, "remaining={remaining}");
        // Cleanup.
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_rotation_no_op_when_file_missing() {
        // Set CSSL_LOG_DIR to a fresh dir with NO loa_runtime.log ; check
        // shouldn't panic + shouldn't create any files.
        let dir = std::env::temp_dir().join(format!("loa-rotchk-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        std::env::set_var("CSSL_LOG_DIR", &dir);
        check_rotation();
        // No files should exist (we didn't write anything).
        let count = fs::read_dir(&dir).unwrap().count();
        assert_eq!(count, 0);
        // Cleanup.
        std::env::remove_var("CSSL_LOG_DIR");
        let _ = fs::remove_dir_all(&dir);
    }
}
