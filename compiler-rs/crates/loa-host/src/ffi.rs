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
use std::sync::atomic::{AtomicU32, Ordering};

use cssl_rt::loa_startup::log_event;

use crate::material::{material_name, MATERIAL_LUT_LEN};
use crate::pattern::{pattern_name, PATTERN_LUT_LEN};

// ───────────────────────────────────────────────────────────────────────
// § Live render-control state (live-text-input control plane)
// ───────────────────────────────────────────────────────────────────────
//
// These atomics let pure-CSSL programs (and the MCP control plane via
// `mcp_tools::render_set_*`) override the per-wall, per-floor-quadrant,
// and per-quad material/pattern at runtime. Reads from the renderer hot
// path are lock-free.

/// One slot per wall (4) + one slot per floor quadrant (4) + 16 free quad
/// slots = 24. Each holds a packed (material << 16) | pattern ; sentinel
/// 0xFFFF = use-default.
static RENDER_CONTROL_SLOTS: [AtomicU32; 24] = [
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
    AtomicU32::new(SENTINEL),
];

const SENTINEL: u32 = 0xFFFF_FFFF;

/// Counter for stress-object spawn ids (returned to caller).
static SPAWN_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Read the current pattern override for a wall (0..3) ; returns u32::MAX
/// if no override is active.
#[must_use]
pub fn wall_pattern_override(wall_id: u32) -> Option<u32> {
    if wall_id >= 4 {
        return None;
    }
    let v = RENDER_CONTROL_SLOTS[wall_id as usize].load(Ordering::Relaxed);
    if v == SENTINEL {
        None
    } else {
        Some(v & 0xFFFF)
    }
}

/// Read the current pattern override for a floor quadrant (0..3).
#[must_use]
pub fn floor_pattern_override(quadrant_id: u32) -> Option<u32> {
    if quadrant_id >= 4 {
        return None;
    }
    let v = RENDER_CONTROL_SLOTS[(4 + quadrant_id) as usize].load(Ordering::Relaxed);
    if v == SENTINEL {
        None
    } else {
        Some(v & 0xFFFF)
    }
}

/// Read the current material override for a quad (0..15).
#[must_use]
pub fn quad_material_override(quad_id: u32) -> Option<u32> {
    if quad_id >= 16 {
        return None;
    }
    let v = RENDER_CONTROL_SLOTS[(8 + quad_id) as usize].load(Ordering::Relaxed);
    if v == SENTINEL {
        None
    } else {
        Some(v >> 16)
    }
}

// Sovereign-cap that mutating FFI calls require. Same value as MCP layer.
const SOVEREIGN_CAP_U64: u64 = 0xCAFE_BABE_DEAD_BEEF;

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
// § Live render-control FFI surface
// ───────────────────────────────────────────────────────────────────────
//
// These symbols let pure-CSSL programs (or the MCP control plane) drive
// the live diagnostic-dense renderer at runtime. They are pure setters /
// getters into the global atomic state above ; the renderer reads it on
// each frame.

/// Set the procedural pattern for a wall (0=N · 1=S · 2=E · 3=W).
/// Returns 0 on success · -1 if `wall_id` out-of-range · -2 if cap-rejected.
#[no_mangle]
pub extern "C" fn __cssl_render_set_wall_pattern(
    wall_id: u32,
    pattern_id: u32,
    sovereign_cap: u64,
) -> i32 {
    if sovereign_cap != SOVEREIGN_CAP_U64 {
        log_event(
            "WARN",
            "loa-host/ffi",
            "__cssl_render_set_wall_pattern · sovereign_cap mismatch",
        );
        return -2;
    }
    if wall_id >= 4 || pattern_id >= PATTERN_LUT_LEN as u32 {
        return -1;
    }
    // Pattern in low 16 bits, material in high (preserve high bits if set).
    let prior = RENDER_CONTROL_SLOTS[wall_id as usize].load(Ordering::Relaxed);
    let mat = if prior == SENTINEL { 0 } else { prior >> 16 };
    let new = (mat << 16) | (pattern_id & 0xFFFF);
    RENDER_CONTROL_SLOTS[wall_id as usize].store(new, Ordering::Relaxed);
    log_event(
        "INFO",
        "loa-host/ffi",
        &format!(
            "render.set_wall_pattern · wall={wall_id} pattern={} ({})",
            pattern_id,
            pattern_name(pattern_id)
        ),
    );
    0
}

/// Set the floor-quadrant pattern (0=NE · 1=NW · 2=SW · 3=SE).
/// Returns 0 on success · -1 if quadrant_id out-of-range · -2 if cap-rejected.
#[no_mangle]
pub extern "C" fn __cssl_render_set_floor_pattern(
    quadrant_id: u32,
    pattern_id: u32,
    sovereign_cap: u64,
) -> i32 {
    if sovereign_cap != SOVEREIGN_CAP_U64 {
        return -2;
    }
    if quadrant_id >= 4 || pattern_id >= PATTERN_LUT_LEN as u32 {
        return -1;
    }
    let slot = (4 + quadrant_id) as usize;
    let prior = RENDER_CONTROL_SLOTS[slot].load(Ordering::Relaxed);
    let mat = if prior == SENTINEL { 0 } else { prior >> 16 };
    let new = (mat << 16) | (pattern_id & 0xFFFF);
    RENDER_CONTROL_SLOTS[slot].store(new, Ordering::Relaxed);
    log_event(
        "INFO",
        "loa-host/ffi",
        &format!(
            "render.set_floor_pattern · q={quadrant_id} pattern={} ({})",
            pattern_id,
            pattern_name(pattern_id)
        ),
    );
    0
}

/// Set a quad's material id (overrides the per-quad default).
/// `quad_id` is 0..15. Returns 0 on success · -1 if id out-of-range.
#[no_mangle]
pub extern "C" fn __cssl_render_set_material(
    quad_id: u32,
    material_id: u32,
    sovereign_cap: u64,
) -> i32 {
    if sovereign_cap != SOVEREIGN_CAP_U64 {
        return -2;
    }
    if quad_id >= 16 || material_id >= MATERIAL_LUT_LEN as u32 {
        return -1;
    }
    let slot = (8 + quad_id) as usize;
    let prior = RENDER_CONTROL_SLOTS[slot].load(Ordering::Relaxed);
    let pat = if prior == SENTINEL { 0 } else { prior & 0xFFFF };
    let new = ((material_id & 0xFFFF) << 16) | pat;
    RENDER_CONTROL_SLOTS[slot].store(new, Ordering::Relaxed);
    log_event(
        "INFO",
        "loa-host/ffi",
        &format!(
            "render.set_material · quad={quad_id} mat={} ({})",
            material_id,
            material_name(material_id)
        ),
    );
    0
}

/// Spawn a stress object at world coordinates (x,y,z).
/// `kind` is 0..13 (see geometry::stress_object_name). Returns the new
/// object id (≥1) on success or 0 on failure.
#[no_mangle]
pub extern "C" fn __cssl_render_spawn_stress_object(
    kind: u32,
    x: f32,
    y: f32,
    z: f32,
    sovereign_cap: u64,
) -> u32 {
    if sovereign_cap != SOVEREIGN_CAP_U64 || kind >= 14 {
        return 0;
    }
    let id = SPAWN_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    log_event(
        "INFO",
        "loa-host/ffi",
        &format!(
            "render.spawn_stress · id={id} kind={kind} ({}) at ({x:.2},{y:.2},{z:.2})",
            crate::geometry::stress_object_name(kind),
        ),
    );
    id
}

/// Despawn a stress object by id. Stage-0 stub : logs the call but the
/// actual ECS will land in a follow-up slice.
/// Returns 0 on success · -1 if id was already despawned · -2 if cap-rejected.
#[no_mangle]
pub extern "C" fn __cssl_render_despawn(object_id: u32, sovereign_cap: u64) -> i32 {
    if sovereign_cap != SOVEREIGN_CAP_U64 {
        return -2;
    }
    log_event(
        "INFO",
        "loa-host/ffi",
        &format!("render.despawn · id={object_id}"),
    );
    0
}

/// Returns the size of the material+pattern palette (encoded as
/// `(materials << 16) | patterns`).
#[no_mangle]
pub extern "C" fn __cssl_render_palette_size() -> u32 {
    let m = MATERIAL_LUT_LEN as u32;
    let p = PATTERN_LUT_LEN as u32;
    (m << 16) | p
}

// ───────────────────────────────────────────────────────────────────────
// § T11-LOA-ROOMS · room enumeration + teleport FFI surface
// ───────────────────────────────────────────────────────────────────────

/// Latest "teleport requested" room id ; the renderer reads this on the
/// next frame and snaps the camera to that room's spawn position. SENTINEL
/// = no pending teleport.
static TELEPORT_PENDING: AtomicU32 = AtomicU32::new(SENTINEL);

/// Total number of rooms in the diagnostic test-suite. Stable contract :
/// CSSL programs may rely on this returning 5 for the foreseeable future.
#[no_mangle]
pub extern "C" fn __cssl_room_count() -> u32 {
    crate::room::ROOM_COUNT
}

/// Request a camera-teleport to room `room_id`. The pure-Rust renderer
/// reads the pending-id on the next frame and snaps the player camera to
/// the room's center. Returns 0 on success, -1 if `room_id` out-of-range,
/// -2 if `sovereign_cap` mismatch.
#[no_mangle]
pub extern "C" fn __cssl_room_teleport(room_id: u32, sovereign_cap: u64) -> i32 {
    if sovereign_cap != SOVEREIGN_CAP_U64 {
        log_event(
            "WARN",
            "loa-host/ffi",
            "__cssl_room_teleport · sovereign_cap mismatch · denied",
        );
        return -2;
    }
    if room_id >= crate::room::ROOM_COUNT {
        return -1;
    }
    TELEPORT_PENDING.store(room_id, Ordering::Relaxed);
    let room_name = crate::room::Room::from_u32(room_id).map_or("?", |r| r.name());
    log_event(
        "INFO",
        "loa-host/ffi",
        &format!("room.teleport · room_id={room_id} ({room_name})"),
    );
    0
}

/// Read + clear any pending teleport request. Returns the room id or
/// `Option::None` if none pending. Used by the render loop / window event
/// handler to apply the camera move.
#[must_use]
pub fn take_pending_teleport() -> Option<u32> {
    let v = TELEPORT_PENDING.swap(SENTINEL, Ordering::Relaxed);
    if v == SENTINEL {
        None
    } else {
        Some(v)
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
        let p = f as *const ();
        assert!(!p.is_null());
    }

    #[test]
    fn ffi_set_wall_pattern_updates_uniform() {
        // sovereign-cap matches → success
        let rc = __cssl_render_set_wall_pattern(0, 4, SOVEREIGN_CAP_U64);
        assert_eq!(rc, 0);
        assert_eq!(wall_pattern_override(0), Some(4));

        // out-of-range wall_id → -1
        let rc2 = __cssl_render_set_wall_pattern(99, 0, SOVEREIGN_CAP_U64);
        assert_eq!(rc2, -1);

        // wrong cap → -2
        let rc3 = __cssl_render_set_wall_pattern(1, 0, 0xDEAD);
        assert_eq!(rc3, -2);
    }

    #[test]
    fn ffi_set_floor_pattern_updates() {
        let rc = __cssl_render_set_floor_pattern(2, 5, SOVEREIGN_CAP_U64);
        assert_eq!(rc, 0);
        assert_eq!(floor_pattern_override(2), Some(5));
    }

    #[test]
    fn ffi_set_material_updates() {
        let rc = __cssl_render_set_material(3, 7, SOVEREIGN_CAP_U64);
        assert_eq!(rc, 0);
        assert_eq!(quad_material_override(3), Some(7));
    }

    #[test]
    fn ffi_spawn_returns_increasing_ids() {
        let id1 = __cssl_render_spawn_stress_object(0, 1.0, 1.0, 1.0, SOVEREIGN_CAP_U64);
        let id2 = __cssl_render_spawn_stress_object(1, 2.0, 1.0, 1.0, SOVEREIGN_CAP_U64);
        assert!(id1 > 0);
        assert!(id2 > id1);
    }

    #[test]
    fn ffi_palette_size_is_packed_correctly() {
        let v = __cssl_render_palette_size();
        let m = v >> 16;
        let p = v & 0xFFFF;
        assert!(m >= 8, "≥ 8 materials");
        assert!(p >= 12, "≥ 12 patterns");
    }

    // § T11-LOA-ROOMS · FFI surface tests
    #[test]
    fn ffi_room_count_is_five() {
        assert_eq!(__cssl_room_count(), 5);
    }

    #[test]
    fn ffi_room_teleport_accepts_valid_id() {
        // Drain any prior pending teleport request so this test is order-independent.
        let _ = take_pending_teleport();
        let rc = __cssl_room_teleport(2, SOVEREIGN_CAP_U64); // PatternRoom
        assert_eq!(rc, 0);
        assert_eq!(take_pending_teleport(), Some(2));
        // After consume, no pending request.
        assert_eq!(take_pending_teleport(), None);
    }

    #[test]
    fn ffi_room_teleport_rejects_out_of_range() {
        let rc = __cssl_room_teleport(99, SOVEREIGN_CAP_U64);
        assert_eq!(rc, -1);
    }

    #[test]
    fn ffi_room_teleport_rejects_wrong_cap() {
        let rc = __cssl_room_teleport(0, 0xDEAD);
        assert_eq!(rc, -2);
    }
}
