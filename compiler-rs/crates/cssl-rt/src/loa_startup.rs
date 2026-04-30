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
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

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
pub fn log_event(level: &str, source: &str, msg: &str) {
    let line = format!("[{}] [{}] [{}] {}\n", iso_utc_now(), level, source, msg);
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(f) = guard.as_mut() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
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
}
