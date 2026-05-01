//! § ffi — pure-CSSL engine entry-point FFI surface
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-PURE-CSSL (W-LOA-pure-cssl-engine)
//!
//! § ROLE
//!   Stable extern "C" symbol exported from the loa-host staticlib that
//!   `Labyrinth of Apocalypse/main.cssl` reaches for via :
//!   ```cssl
//!   extern "C" fn __cssl_engine_run() -> i32
//!   fn main() -> i32 { __cssl_engine_run() }
//!   ```
//!
//! § ABI CONTRACT
//!   Returns 0 on clean window-close, non-zero on engine-startup error
//!   (e.g. no event-loop · no GPU · winit failure). The pure-CSSL caller
//!   propagates this as the process exit code. Treats this symbol like
//!   a C `main()` : zero-args, single-i32 return, no panic across the
//!   boundary (we catch + return non-zero on panic).
//!
//! § BUILD MODES
//!   - `runtime` feature : opens window + runs render-loop + ticks DM +
//!     serves MCP. Returns 0 on clean exit.
//!   - default (catalog) : logs + returns 0 immediately. Lets csslc
//!     produce a hello-world-shaped LoA.exe that links cleanly even when
//!     the runtime-feature staticlib hasn't been built yet (useful for
//!     parallel-fanout dev workflows).
//!
//! § PRIME-DIRECTIVE
//!   Engine launch is consent-architected : the user invoked the binary,
//!   they get the window + capture + audio. Esc opens the menu (NOT exits)
//!   so the user retains agency over when the session ends. No telemetry
//!   leak ; no off-machine relay.

#![allow(unsafe_code)] // extern "C" exports require #[no_mangle] which is unsafe attr

use std::panic::{self, AssertUnwindSafe};

use cssl_rt::loa_startup::log_event;

/// Pure-CSSL engine entry-point.
///
/// § ABI
///   `extern "C" fn() -> i32` : zero-args, single-i32 return.
///
/// § BEHAVIOR
///   1. Logs the entry to `logs/loa_runtime.log` so the user knows the
///      engine fired before main() returned.
///   2. Spawns the engine event loop via `crate::run_engine()`. With the
///      `runtime` feature this opens a winit window + runs until close ;
///      catalog-mode logs + exits.
///   3. Catches any panic via `panic::catch_unwind` so we never unwind
///      across the FFI boundary into the CSSL caller (CSSL stage-0 has
///      no rustpanic-runtime ; an unwound panic across `extern "C"` is
///      undefined behavior per §§ 22 TELEMETRY).
///   4. Returns 0 on clean exit, 1 on `run_engine` IO error, 2 on panic.
///
/// § Stage-1 path
///   When the engine event-loop is rewritten in pure CSSL (winit-bindings
///   via cssl-host-window FFI · wgpu-bindings via cssl-host-gpu FFI), this
///   function shrinks to a no-op + main.cssl drives the loop directly.
///   The symbol stays as an ABI anchor for backward-compat.
#[no_mangle]
pub extern "C" fn __cssl_engine_run() -> i32 {
    log_event(
        "INFO",
        "loa-host/ffi",
        "__cssl_engine_run · pure-CSSL entry · delegating to run_engine",
    );

    // Catch panics so we never unwind across the FFI boundary into the
    // CSSL caller. Stage-0 CSSL has no rustpanic-runtime ; an unwound
    // panic across `extern "C"` is UB. Wrap in AssertUnwindSafe : the
    // engine state machine is internally panic-safe (every Mutex is
    // poison-tolerant per W-LOA-host-mcp), so a panic here is recoverable.
    let r = panic::catch_unwind(AssertUnwindSafe(crate::run_engine));

    match r {
        Ok(Ok(())) => {
            log_event(
                "INFO",
                "loa-host/ffi",
                "__cssl_engine_run · clean exit · returning 0",
            );
            0
        }
        Ok(Err(e)) => {
            log_event(
                "ERROR",
                "loa-host/ffi",
                &format!("__cssl_engine_run · IO error : {e} · returning 1"),
            );
            1
        }
        Err(_) => {
            log_event(
                "ERROR",
                "loa-host/ffi",
                "__cssl_engine_run · panic caught at FFI boundary · returning 2",
            );
            2
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Catalog-mode (no runtime feature) : the FFI fn returns 0 cleanly.
    /// This proves the symbol is reachable + the panic-catch wrap doesn't
    /// degrade clean-exit semantics. Skipped under `runtime` because that
    /// would actually try to open a window.
    #[cfg(not(feature = "runtime"))]
    #[test]
    fn engine_run_catalog_returns_clean() {
        let code = __cssl_engine_run();
        assert_eq!(code, 0);
    }

    /// Symbol reachability : the function exists at the linker-visible
    /// surface. We can't introspect the export-symbol table from a unit
    /// test (no runtime-reflection), but we CAN take its address as a
    /// `extern "C" fn() -> i32` and prove the ABI tag is correct.
    #[test]
    fn engine_run_has_correct_abi_signature() {
        let f: extern "C" fn() -> i32 = __cssl_engine_run;
        // Verify the function-pointer round-trips through a void* (the
        // shape csslc-emitted code uses to call us).
        let p = f as *const ();
        assert!(!p.is_null());
    }
}
