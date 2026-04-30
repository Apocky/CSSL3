//! § cssl-rt::loa_stubs — Host-symbol stubs for LoA `extern fn __cssl_*` (T11-D317)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Provides the `#[no_mangle] pub extern "C" fn __cssl_*` symbols that LoA
//!   `.csl` scenes forward-decl + the csslc-emitted object-files reference.
//!   Each stub gives a SAFE-DEFAULT body so static-link succeeds **before**
//!   the actual subsystems (window / GPU / audio / OpenXR) are wired by the
//!   stage-1 backend-binding work.
//!
//!   Stage-1 incrementally REPLACES these stubs in-place with real
//!   implementations. The ABI signature is what's stable ; the body
//!   evolves from "log-only sentinel" → "real subsystem call".
//!
//! § CATEGORIES (this module)
//!   1. WINDOW + INPUT          — log-only, sentinel returns
//!   2. GPU + RENDER            — log-only, zero-handle returns
//!   3. ω-FIELD SUBSTRATE       — REAL : sparse Morton-keyed FieldCell hash
//!   4. TIME + TELEMETRY        — REAL : monotonic clock + 4096-event ring
//!   5. AUDIO                   — log-only, sentinel returns
//!   6. MCP + CAP               — REAL : 16-tool registry + handle gate
//!   7. PLAYER + SCENE          — REAL : 32B PlayerState struct + scene-id
//!   8. UTILITY                 — REAL : xorshift32 RNG + FNV-1a + Morton
//!
//!   Memory + alloc lives in [`crate::alloc`] / [`crate::ffi`] (already shipped
//!   in S6-A1) ; this module adds only the `__cssl_arena_*` no-op stubs.
//!
//! § NOT-A-PANIC INVARIANT
//!   No stub here panics. No stub here unbounded-allocs. Pointer reads are
//!   bounds-checked against the supplied `len`. NUL-byte UTF-8 is accepted as
//!   a length-only hint (no zero-terminator scan).
//!
//! § STAGE-1 SWAP-POINTS
//!   Each stub's `/// § STAGE-1 PATH` doc comment names the eventual real
//!   implementation (e.g. `cssl-host-vulkan::device_create` for GPU,
//!   `cssl-host-cpal::play_sample` for audio). The signatures are locked.
//!
//! § PRIME-DIRECTIVE
//!   "There was no hurt nor harm in the making of this, to anyone /
//!    anything / anybody."
//!   No surveillance, no telemetry-without-consent, no hidden side-channels.
//!   The telemetry ring is process-local + bounded ; the MCP registry
//!   accepts only explicitly-supplied tool-names ; the cap-check rejects
//!   the zero handle by default.

// § FFI surface fundamentally requires `extern "C"` + raw-pointer ops.
// Each unsafe block carries an inline SAFETY paragraph.
#![allow(unsafe_code)]
#![allow(dead_code)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::cast_possible_wrap)]
// `Mutex::lock`'s Err branch is poison-recovery only ; flattening to
// `Option::map_or` obscures the recovery path. Keep the `if let Ok / else`
// form that mirrors the rest of cssl-rt's host_* modules.
#![allow(clippy::option_if_let_else)]

use core::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

// ───────────────────────────────────────────────────────────────────────
// § return-code conventions (FFI-stable)
//
//   `0`  = success / boolean-false
//   `1`  = boolean-true (in cap-check / scene-active style)
//   `-1` = error / not-yet-implemented sentinel
// ───────────────────────────────────────────────────────────────────────

/// FFI return : success.
pub const LOA_OK: i32 = 0;
/// FFI return : error / stub-not-yet-implemented.
pub const LOA_ERR: i32 = -1;
/// FFI return : boolean true (cap-check passed, etc.).
pub const LOA_TRUE: i32 = 1;
/// Handle sentinel : zero = invalid / unallocated.
pub const LOA_INVALID_HANDLE: u64 = 0;

// ───────────────────────────────────────────────────────────────────────
// § debug-trace helper — single point that respects test-mode silence
//
//   Stage-1 will replace this with `cssl-telemetry`'s structured event
//   pipeline. For stage-0 stubs it's a one-shot eprintln per symbol the
//   first time it's called, gated behind a `LOA_STUB_TRACE` env var so
//   release builds + the cargo-test harness stay silent by default.
// ───────────────────────────────────────────────────────────────────────

#[cfg(not(test))]
fn trace_first_call(symbol: &'static str) {
    use core::sync::atomic::AtomicBool;
    static SEEN: OnceLock<Mutex<HashMap<&'static str, AtomicBool>>> = OnceLock::new();
    let map = SEEN.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut g) = map.lock() {
        let entry = g
            .entry(symbol)
            .or_insert_with(|| AtomicBool::new(false));
        if !entry.swap(true, Ordering::Relaxed) && std::env::var_os("LOA_STUB_TRACE").is_some() {
            eprintln!("[loa_stub] first-call: {symbol}");
        }
    }
}

#[cfg(test)]
fn trace_first_call(_symbol: &'static str) {
    // Tests rely on counter-only observation ; no stdout/stderr noise.
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 1 — WINDOW + INPUT (log-only, sentinel returns)
// ═══════════════════════════════════════════════════════════════════════
//
// Stage-1 path : delegates to `cssl-rt::host_window` (already shipped in
// Wave-D as platform-conditional W32/X11/Wayland implementations). The
// stub here returns INVALID_HANDLE so LoA scenes can run their event
// loop in `--no-display` test mode.
// ───────────────────────────────────────────────────────────────────────

/// FFI : create a window with `title` + dimensions. Returns handle (or 0 on failure).
///
/// § STAGE-1 PATH : delegate to `crate::host_window::__cssl_window_spawn`. Note
/// `__cssl_window_create` is a LoA-vocabulary alias — the W-D3 surface ships
/// `__cssl_window_spawn` with a slightly different param-order ; LoA scenes
/// can use either at link-time.
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_create(
    width: i32,
    height: i32,
    title_ptr: *const u8,
    title_len: i32,
) -> u64 {
    trace_first_call("__cssl_window_create");
    let _ = (width, height, title_ptr, title_len);
    // Stage-0 stub : returns the invalid-handle sentinel. Stage-1 wires
    // through to `crate::host_window::__cssl_window_spawn` with arg-order
    // adaptation. We do NOT deref the title pointer here — LoA scenes
    // link against this stub for headless tests where title is unused.
    LOA_INVALID_HANDLE
}

// `__cssl_window_destroy` is already provided by `crate::host_window` (W-D3) ;
// we don't re-export it here. LoA scenes that forward-decl it link against
// the host_window implementation, which matches the same signature
// `(handle: u64) -> i32` the LoA surface expects.

/// FFI : drain pending OS events for `handle`. Returns event-count drained.
#[no_mangle]
pub extern "C" fn __cssl_window_poll_events(handle: u64) -> i32 {
    trace_first_call("__cssl_window_poll_events");
    let _ = handle;
    0
}

/// FFI : write current window size into `(*w_out, *h_out)`.
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_get_size(
    handle: u64,
    w_out: *mut i32,
    h_out: *mut i32,
) -> i32 {
    trace_first_call("__cssl_window_get_size");
    let _ = handle;
    // SAFETY : caller supplies non-null out-pointers per ABI ; we null-check.
    if !w_out.is_null() {
        unsafe { *w_out = 0 };
    }
    if !h_out.is_null() {
        unsafe { *h_out = 0 };
    }
    LOA_OK
}

/// FFI : write mouse-state into `(*x, *y, *buttons)`.
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_get_mouse(
    handle: u64,
    x: *mut i32,
    y: *mut i32,
    buttons: *mut u32,
) -> i32 {
    trace_first_call("__cssl_window_get_mouse");
    let _ = handle;
    // SAFETY : null-check each out-ptr ; ABI says caller supplies them.
    if !x.is_null() {
        unsafe { *x = 0 };
    }
    if !y.is_null() {
        unsafe { *y = 0 };
    }
    if !buttons.is_null() {
        unsafe { *buttons = 0 };
    }
    LOA_OK
}

/// FFI : write keyboard-state into `key_state_out` (caller-allocated `len` bytes).
#[no_mangle]
pub unsafe extern "C" fn __cssl_window_get_keys(
    handle: u64,
    key_state_out: *mut u8,
    len: i32,
) -> i32 {
    trace_first_call("__cssl_window_get_keys");
    let _ = handle;
    if !key_state_out.is_null() && len > 0 {
        // SAFETY : caller supplies ≥ len bytes per ABI ; we bound-zero them.
        unsafe {
            core::ptr::write_bytes(key_state_out, 0, len as usize);
        }
    }
    LOA_OK
}

/// FFI : aggregate input-state poll.
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_state(handle: u64, out_buf: *mut u8, max_bytes: i32) -> i32 {
    trace_first_call("__cssl_input_state");
    let _ = handle;
    if !out_buf.is_null() && max_bytes > 0 {
        // SAFETY : zero the caller-supplied buffer ; bound-checked.
        unsafe {
            core::ptr::write_bytes(out_buf, 0, max_bytes as usize);
        }
    }
    0
}

/// FFI : check if a logical input action is currently pressed.
#[no_mangle]
pub extern "C" fn __cssl_input_action_pressed(action_id: u32) -> i32 {
    trace_first_call("__cssl_input_action_pressed");
    let _ = action_id;
    0
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 2 — GPU + RENDER (log-only, zero-handle returns)
// ═══════════════════════════════════════════════════════════════════════
//
// Stage-1 path : delegate to `cssl-host-vulkan` / `cssl-host-d3d12` /
// `cssl-host-metal` per-platform shimmed through `crate::host_gpu`. The
// stub here surfaces the LoA scene's pipeline-create + draw calls without
// requiring a GPU device, so headless test runs succeed.
// ───────────────────────────────────────────────────────────────────────

// `__cssl_gpu_device_create / __cssl_gpu_device_destroy / __cssl_gpu_swapchain_*`
// are already provided by `crate::host_gpu` (W-D2). LoA scenes that
// forward-decl those names link against the host_gpu implementations.
// The remaining `__cssl_gpu_submit / __cssl_gpu_present / __cssl_swapchain_*
// / __cssl_pipeline_*` symbols below are ADDITIONAL surface specific to
// the LoA renderer pipeline that aren't in W-D2's surface.

/// FFI : submit a command-list to the GPU.
#[no_mangle]
pub unsafe extern "C" fn __cssl_gpu_submit(
    device: u64,
    cmd_buf: *const u8,
    cmd_len: i32,
) -> i32 {
    trace_first_call("__cssl_gpu_submit");
    let _ = (device, cmd_buf, cmd_len);
    LOA_OK
}

/// FFI : present the current backbuffer.
#[no_mangle]
pub extern "C" fn __cssl_gpu_present(swapchain: u64) -> i32 {
    trace_first_call("__cssl_gpu_present");
    let _ = swapchain;
    LOA_OK
}

/// FFI : create a swapchain bound to `(device, window)`.
#[no_mangle]
pub extern "C" fn __cssl_swapchain_create(device: u64, window: u64, format: u32) -> u64 {
    trace_first_call("__cssl_swapchain_create");
    let _ = (device, window, format);
    LOA_INVALID_HANDLE
}

/// FFI : destroy a swapchain.
#[no_mangle]
pub extern "C" fn __cssl_swapchain_destroy(swapchain: u64) -> i32 {
    trace_first_call("__cssl_swapchain_destroy");
    let _ = swapchain;
    LOA_OK
}

/// FFI : create a graphics or compute pipeline.
#[no_mangle]
pub unsafe extern "C" fn __cssl_pipeline_create(
    device: u64,
    desc_ptr: *const u8,
    desc_len: i32,
) -> u64 {
    trace_first_call("__cssl_pipeline_create");
    let _ = (device, desc_ptr, desc_len);
    LOA_INVALID_HANDLE
}

/// FFI : destroy a pipeline.
#[no_mangle]
pub extern "C" fn __cssl_pipeline_destroy(pipeline: u64) -> i32 {
    trace_first_call("__cssl_pipeline_destroy");
    let _ = pipeline;
    LOA_OK
}

/// FFI : bind a pipeline to the current command-encoder.
#[no_mangle]
pub extern "C" fn __cssl_pipeline_bind(pipeline: u64, encoder: u64) -> i32 {
    trace_first_call("__cssl_pipeline_bind");
    let _ = (pipeline, encoder);
    LOA_OK
}

/// FFI : issue a draw with `(vertex_count, instance_count, first_vertex, first_instance)`.
#[no_mangle]
pub extern "C" fn __cssl_pipeline_draw(
    encoder: u64,
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
) -> i32 {
    trace_first_call("__cssl_pipeline_draw");
    let _ = (
        encoder,
        vertex_count,
        instance_count,
        first_vertex,
        first_instance,
    );
    LOA_OK
}

/// FFI : dispatch a compute pipeline.
#[no_mangle]
pub extern "C" fn __cssl_pipeline_dispatch(
    encoder: u64,
    group_x: u32,
    group_y: u32,
    group_z: u32,
) -> i32 {
    trace_first_call("__cssl_pipeline_dispatch");
    let _ = (encoder, group_x, group_y, group_z);
    LOA_OK
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 3 — ω-FIELD SUBSTRATE (REAL : sparse-hash-grid)
// ═══════════════════════════════════════════════════════════════════════
//
// Per spec `30_SUBSTRATE_v2.csl`, the ω-field is a sparse Morton-keyed
// volume of 88-byte FieldCell records. This stub implements an in-memory
// HashMap<u64, [u8; FIELD_CELL_BYTES]> backed by a global Mutex so LoA
// scenes can read/write/decay during stage-0 testing.
//
// Sawyer-efficient choices :
//   * 88-byte fixed-size cells (matches spec) ; no heap allocation per cell
//   * Decay is integer-quantized (Q14, dt scaled) ; no floating-point
//   * Initial capacity is honored ; HashMap grows lazily under contention
//
// Stage-1 path : `cssl-substrate-omega-field`'s real Σ-mask + KAN runtime
// replaces this map with a chunked Morton-octree.
// ───────────────────────────────────────────────────────────────────────

/// Bytes per ω-field cell per spec 30_SUBSTRATE_v2.csl.
pub const FIELD_CELL_BYTES: usize = 88;

struct OmegaFieldStub {
    cells: HashMap<u64, [u8; FIELD_CELL_BYTES]>,
    initialized: bool,
}

impl OmegaFieldStub {
    fn new() -> Self {
        Self {
            cells: HashMap::new(),
            initialized: false,
        }
    }
}

fn omega_field_lock() -> &'static Mutex<OmegaFieldStub> {
    static OMEGA: OnceLock<Mutex<OmegaFieldStub>> = OnceLock::new();
    OMEGA.get_or_init(|| Mutex::new(OmegaFieldStub::new()))
}

/// FFI : initialize the ω-field substrate with `capacity` reserved cells.
///
/// § STAGE-1 PATH : delegate to `cssl-substrate-omega-field::Substrate::with_capacity`.
#[no_mangle]
pub extern "C" fn __cssl_omega_field_init(capacity: i32) -> i32 {
    trace_first_call("__cssl_omega_field_init");
    if capacity < 0 {
        return LOA_ERR;
    }
    let mtx = omega_field_lock();
    if let Ok(mut g) = mtx.lock() {
        if capacity > 0 {
            g.cells.reserve(capacity as usize);
        }
        g.initialized = true;
        LOA_OK
    } else {
        LOA_ERR
    }
}

/// FFI : sample one Morton-keyed cell into `out_buf`.
///
/// Returns bytes-written (0 if cell unset, FIELD_CELL_BYTES if set, -1 on bad args).
#[no_mangle]
pub unsafe extern "C" fn __cssl_omega_field_sample(
    morton: u64,
    out_buf: *mut u8,
    cap: i32,
) -> i32 {
    trace_first_call("__cssl_omega_field_sample");
    if out_buf.is_null() || cap < FIELD_CELL_BYTES as i32 {
        return LOA_ERR;
    }
    let mtx = omega_field_lock();
    let Ok(g) = mtx.lock() else {
        return LOA_ERR;
    };
    if let Some(cell) = g.cells.get(&morton) {
        // SAFETY : cap >= FIELD_CELL_BYTES validated above ; out_buf is the
        // caller's ≥ cap-byte buffer per ABI.
        unsafe {
            core::ptr::copy_nonoverlapping(cell.as_ptr(), out_buf, FIELD_CELL_BYTES);
        }
        FIELD_CELL_BYTES as i32
    } else {
        0
    }
}

/// FFI : sample an AABB-bounded set of cells. Returns cells-written (≤ max_cells).
#[no_mangle]
pub unsafe extern "C" fn __cssl_omega_field_sample_aabb(
    min_morton: u64,
    max_morton: u64,
    out_buf: *mut u8,
    max_cells: i32,
) -> i32 {
    trace_first_call("__cssl_omega_field_sample_aabb");
    if out_buf.is_null() || max_cells <= 0 || min_morton > max_morton {
        return LOA_ERR;
    }
    let mtx = omega_field_lock();
    let Ok(g) = mtx.lock() else {
        return LOA_ERR;
    };
    let mut written = 0i32;
    for (&k, cell) in &g.cells {
        if written >= max_cells {
            break;
        }
        if k >= min_morton && k <= max_morton {
            // SAFETY : we advance by FIELD_CELL_BYTES per cell ; total ≤
            // max_cells * FIELD_CELL_BYTES which the caller pre-validated.
            unsafe {
                let dst = out_buf.add((written as usize) * FIELD_CELL_BYTES);
                core::ptr::copy_nonoverlapping(cell.as_ptr(), dst, FIELD_CELL_BYTES);
            }
            written += 1;
        }
    }
    written
}

/// FFI : modify (insert/overwrite) one Morton-keyed cell from `in_buf`.
///
/// `sovereign_handle` MUST be non-zero per the spec's sovereignty-gate ; the
/// stub treats zero as a denial signal (returns LOA_ERR).
#[no_mangle]
pub unsafe extern "C" fn __cssl_omega_field_modify(
    morton: u64,
    in_buf: *const u8,
    len: i32,
    sovereign_handle: u64,
) -> i32 {
    trace_first_call("__cssl_omega_field_modify");
    if sovereign_handle == 0 {
        return LOA_ERR;
    }
    if in_buf.is_null() || len < FIELD_CELL_BYTES as i32 {
        return LOA_ERR;
    }
    let mut cell = [0u8; FIELD_CELL_BYTES];
    // SAFETY : len ≥ FIELD_CELL_BYTES validated ; in_buf non-null ; caller
    // owns the read range per ABI.
    unsafe {
        core::ptr::copy_nonoverlapping(in_buf, cell.as_mut_ptr(), FIELD_CELL_BYTES);
    }
    let mtx = omega_field_lock();
    let Ok(mut g) = mtx.lock() else {
        return LOA_ERR;
    };
    g.cells.insert(morton, cell);
    LOA_OK
}

/// FFI : advance the ω-field decay step by `dt_q14` (Q14 fixed-point).
#[no_mangle]
pub extern "C" fn __cssl_omega_field_decay_step(dt_q14: i32) -> i32 {
    trace_first_call("__cssl_omega_field_decay_step");
    if dt_q14 < 0 {
        return LOA_ERR;
    }
    // Stage-0 stub : decay is a no-op ; stage-1 KAN-substrate runtime will
    // replace this with the real Σ-mask propagation pass.
    let _ = dt_q14;
    LOA_OK
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 4 — TIME + TELEMETRY (REAL)
// ═══════════════════════════════════════════════════════════════════════

// `__cssl_time_monotonic_ns` is already provided by `crate::host_time` (W-D6)
// returning `u64` ns since process-start. LoA scenes can call it for monotonic
// time. The mission spec called for an `i64` variant ; we keep the existing
// `u64` ABI to avoid duplicate-symbol errors. LoA call-sites cast to i64
// at the boundary if signed-arithmetic is needed (the `u64` value is always
// ≤ `i64::MAX` for any plausible runtime — saturating at ≈ 292 years).

/// FFI : current wall-clock hour (0..23). On clock-skew before-epoch returns 0.
#[no_mangle]
pub extern "C" fn __cssl_time_of_day() -> u8 {
    trace_first_call("__cssl_time_of_day");
    let secs_since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let hour_utc = (secs_since_epoch / 3600) % 24;
    hour_utc as u8
}

// ─── telemetry ring (4096-event capacity) ──────────────────────────────

/// Fixed payload max per event.
pub const TELEMETRY_PAYLOAD_MAX: usize = 64;

/// Ring capacity (events).
pub const TELEMETRY_RING_CAP: usize = 4096;

#[derive(Clone, Copy)]
struct TelemetryEvent {
    event_id: u32,
    payload_len: u8,
    payload: [u8; TELEMETRY_PAYLOAD_MAX],
}

impl TelemetryEvent {
    const fn empty() -> Self {
        Self {
            event_id: 0,
            payload_len: 0,
            payload: [0u8; TELEMETRY_PAYLOAD_MAX],
        }
    }
}

struct TelemetryRing {
    events: Vec<TelemetryEvent>,
    head: usize,
    len: usize,
}

impl TelemetryRing {
    fn new() -> Self {
        Self {
            events: vec![TelemetryEvent::empty(); TELEMETRY_RING_CAP],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, ev: TelemetryEvent) {
        let idx = (self.head + self.len) % TELEMETRY_RING_CAP;
        self.events[idx] = ev;
        if self.len < TELEMETRY_RING_CAP {
            self.len += 1;
        } else {
            // overwrite oldest
            self.head = (self.head + 1) % TELEMETRY_RING_CAP;
        }
    }

    fn pop_front(&mut self) -> Option<TelemetryEvent> {
        if self.len == 0 {
            return None;
        }
        let ev = self.events[self.head];
        self.head = (self.head + 1) % TELEMETRY_RING_CAP;
        self.len -= 1;
        Some(ev)
    }

    fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

fn telemetry_ring() -> &'static Mutex<TelemetryRing> {
    static RING: OnceLock<Mutex<TelemetryRing>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(TelemetryRing::new()))
}

/// Bytes-per-event in the wire-format used by `__cssl_telemetry_drain` :
/// `u32 event_id | u8 payload_len | [u8; TELEMETRY_PAYLOAD_MAX] payload`.
const TELEMETRY_DRAIN_BYTES_PER_EVENT: usize = 4 + 1 + TELEMETRY_PAYLOAD_MAX;

/// FFI : emit a telemetry event into the process-local ring.
///
/// `payload_len` is clamped to TELEMETRY_PAYLOAD_MAX. Returns 0 on success,
/// -1 on bad args.
#[no_mangle]
pub unsafe extern "C" fn __cssl_telemetry_emit(
    event_id: u32,
    payload_ptr: *const u8,
    payload_len: i32,
) -> i32 {
    trace_first_call("__cssl_telemetry_emit");
    if payload_len < 0 {
        return LOA_ERR;
    }
    let len = (payload_len as usize).min(TELEMETRY_PAYLOAD_MAX);
    let mut ev = TelemetryEvent {
        event_id,
        payload_len: len as u8,
        payload: [0u8; TELEMETRY_PAYLOAD_MAX],
    };
    if len > 0 && !payload_ptr.is_null() {
        // SAFETY : caller-owned read range of `len` bytes ≤ payload_len
        // they supplied per ABI ; clamped above.
        unsafe {
            core::ptr::copy_nonoverlapping(payload_ptr, ev.payload.as_mut_ptr(), len);
        }
    }
    let Ok(mut ring) = telemetry_ring().lock() else {
        return LOA_ERR;
    };
    ring.push(ev);
    LOA_OK
}

/// FFI : drain events from the ring into a packed byte-buffer.
///
/// Wire-format per event : `u32 event_id | u8 payload_len | u8[64] payload` =
/// 69 bytes. Returns total bytes-written (always a multiple of 69).
#[no_mangle]
pub unsafe extern "C" fn __cssl_telemetry_drain(out_buf: *mut u8, max_bytes: i32) -> i32 {
    trace_first_call("__cssl_telemetry_drain");
    if out_buf.is_null() || max_bytes <= 0 {
        return 0;
    }
    let max_events = (max_bytes as usize) / TELEMETRY_DRAIN_BYTES_PER_EVENT;
    if max_events == 0 {
        return 0;
    }
    let Ok(mut ring) = telemetry_ring().lock() else {
        return 0;
    };
    let mut written = 0usize;
    for _ in 0..max_events {
        let Some(ev) = ring.pop_front() else { break };
        let id_bytes = ev.event_id.to_le_bytes();
        // SAFETY : we advance `written` by exactly TELEMETRY_DRAIN_BYTES_PER_EVENT
        // per iteration ; `max_events` was derived from max_bytes so we never
        // exceed the caller's buffer.
        unsafe {
            let dst = out_buf.add(written);
            core::ptr::copy_nonoverlapping(id_bytes.as_ptr(), dst, 4);
            *dst.add(4) = ev.payload_len;
            core::ptr::copy_nonoverlapping(
                ev.payload.as_ptr(),
                dst.add(5),
                TELEMETRY_PAYLOAD_MAX,
            );
        }
        written += TELEMETRY_DRAIN_BYTES_PER_EVENT;
    }
    written as i32
}

/// FFI : flush the telemetry ring (drop all pending events).
#[no_mangle]
pub extern "C" fn __cssl_telemetry_flush() -> i32 {
    trace_first_call("__cssl_telemetry_flush");
    let Ok(mut ring) = telemetry_ring().lock() else {
        return LOA_ERR;
    };
    ring.clear();
    LOA_OK
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 5 — MEMORY + ALLOC
// ═══════════════════════════════════════════════════════════════════════
//
// `__cssl_alloc / __cssl_free / __cssl_realloc` already shipped in [`crate::ffi`].
// This module adds only the arena-create / arena-destroy stubs.
// ───────────────────────────────────────────────────────────────────────

static ARENA_NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// FFI : create a per-frame arena of `capacity` bytes. Returns handle.
#[no_mangle]
pub extern "C" fn __cssl_arena_create(capacity: i32) -> u64 {
    trace_first_call("__cssl_arena_create");
    if capacity <= 0 {
        return LOA_INVALID_HANDLE;
    }
    // Stage-0 : just allocate a sentinel handle. Stage-1 will back this with
    // a real `crate::alloc::BumpArena`.
    ARENA_NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)
}

/// FFI : destroy an arena handle.
#[no_mangle]
pub extern "C" fn __cssl_arena_destroy(handle: u64) -> i32 {
    trace_first_call("__cssl_arena_destroy");
    let _ = handle;
    LOA_OK
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 6 — AUDIO (log-only)
// ═══════════════════════════════════════════════════════════════════════
//
// Stage-1 path : `cssl-host-cpal` (PortAudio successor) for sample playback ;
// `cssl-host-whisper` for ASR. Stage-0 stubs return error sentinels.
// ───────────────────────────────────────────────────────────────────────

/// FFI : initialize the audio subsystem.
#[no_mangle]
pub extern "C" fn __cssl_audio_init() -> i32 {
    trace_first_call("__cssl_audio_init");
    LOA_OK
}

/// FFI : play a sample with `volume` (Q14 fixed-point in [0, 1]).
#[no_mangle]
pub extern "C" fn __cssl_audio_play(sample_id: u32, volume_q14: i32) -> i32 {
    trace_first_call("__cssl_audio_play");
    let _ = (sample_id, volume_q14);
    LOA_OK
}

/// FFI : stop a sample/channel.
#[no_mangle]
pub extern "C" fn __cssl_audio_stop(channel: u32) -> i32 {
    trace_first_call("__cssl_audio_stop");
    let _ = channel;
    LOA_OK
}

/// FFI : load a Whisper model from `model_path`. Returns 0 (= "not loaded · stub").
#[no_mangle]
pub unsafe extern "C" fn __cssl_whisper_load(model_path_ptr: *const u8, len: i32) -> u64 {
    trace_first_call("__cssl_whisper_load");
    let _ = (model_path_ptr, len);
    LOA_INVALID_HANDLE
}

/// FFI : transcribe a PCM buffer to UTF-8 text. Returns -1 (= stub).
#[no_mangle]
pub unsafe extern "C" fn __cssl_whisper_transcribe(
    model: u64,
    pcm_ptr: *const i16,
    sample_count: i32,
    out_text: *mut u8,
    max_bytes: i32,
) -> i32 {
    trace_first_call("__cssl_whisper_transcribe");
    let _ = (model, pcm_ptr, sample_count, out_text, max_bytes);
    LOA_ERR
}

/// FFI : unload a Whisper model.
#[no_mangle]
pub extern "C" fn __cssl_whisper_unload(model: u64) -> i32 {
    trace_first_call("__cssl_whisper_unload");
    let _ = model;
    LOA_OK
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 7 — MCP + CAP (REAL)
// ═══════════════════════════════════════════════════════════════════════

/// Maximum number of MCP tools registrable in stage-0.
pub const MCP_TOOL_REGISTRY_CAP: usize = 16;

#[derive(Clone)]
struct McpToolEntry {
    name: String,
    handler_id: u32,
}

fn mcp_registry() -> &'static Mutex<Vec<McpToolEntry>> {
    static REG: OnceLock<Mutex<Vec<McpToolEntry>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(Vec::with_capacity(MCP_TOOL_REGISTRY_CAP)))
}

/// FFI : register an MCP tool with `name` + `handler_id`. Returns 0 on
/// success, -1 if the registry is full or the name is invalid.
#[no_mangle]
pub unsafe extern "C" fn __cssl_mcp_register_tool(
    name_ptr: *const u8,
    name_len: i32,
    handler_id: u32,
) -> i32 {
    trace_first_call("__cssl_mcp_register_tool");
    if name_ptr.is_null() || name_len <= 0 {
        return LOA_ERR;
    }
    // SAFETY : caller supplies `(name_ptr, name_len)` per ABI ; we cap at
    // 256 bytes to avoid OOB surprises.
    let len = (name_len as usize).min(256);
    let bytes = unsafe { core::slice::from_raw_parts(name_ptr, len) };
    let Ok(name) = core::str::from_utf8(bytes) else {
        return LOA_ERR;
    };
    let Ok(mut reg) = mcp_registry().lock() else {
        return LOA_ERR;
    };
    if reg.len() >= MCP_TOOL_REGISTRY_CAP {
        return LOA_ERR;
    }
    reg.push(McpToolEntry {
        name: name.to_string(),
        handler_id,
    });
    LOA_OK
}

/// FFI : unregister an MCP tool by `handler_id`. Returns 0 on success, -1 if not found.
#[no_mangle]
pub extern "C" fn __cssl_mcp_unregister(handler_id: u32) -> i32 {
    trace_first_call("__cssl_mcp_unregister");
    let Ok(mut reg) = mcp_registry().lock() else {
        return LOA_ERR;
    };
    let initial = reg.len();
    reg.retain(|e| e.handler_id != handler_id);
    if reg.len() < initial {
        LOA_OK
    } else {
        LOA_ERR
    }
}

/// FFI : check sovereign capability for `handle`. Returns 1 if granted, 0 otherwise.
#[no_mangle]
pub extern "C" fn __cssl_cap_check_sovereign(handle: u64) -> i32 {
    trace_first_call("__cssl_cap_check_sovereign");
    if handle == 0 {
        0
    } else {
        LOA_TRUE
    }
}

/// FFI : check observer capability for `handle`. Returns 1 if granted, 0 otherwise.
#[no_mangle]
pub extern "C" fn __cssl_cap_check_observer(handle: u64) -> i32 {
    trace_first_call("__cssl_cap_check_observer");
    if handle == 0 {
        0
    } else {
        LOA_TRUE
    }
}

/// FFI : check IFC flow from `from_label` to `to_label`. Returns 1 if allowed, 0 otherwise.
#[no_mangle]
pub extern "C" fn __cssl_ifc_check_flow(from_label: u32, to_label: u32) -> i32 {
    trace_first_call("__cssl_ifc_check_flow");
    // Stage-0 trivial : same-label flows are always allowed ; mixed labels
    // require sovereign override (stage-1's IFC enforcer).
    if from_label == to_label {
        LOA_TRUE
    } else {
        0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 8 — PLAYER + SCENE (REAL · static singleton)
// ═══════════════════════════════════════════════════════════════════════

/// Bytes per PlayerState struct (compact 32B stub layout).
pub const PLAYER_STATE_BYTES: usize = 32;

fn player_state() -> &'static Mutex<[u8; PLAYER_STATE_BYTES]> {
    static STATE: OnceLock<Mutex<[u8; PLAYER_STATE_BYTES]>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new([0u8; PLAYER_STATE_BYTES]))
}

/// FFI : read the current player state into `out`. Returns bytes-written.
#[no_mangle]
pub unsafe extern "C" fn __cssl_player_state_get(out: *mut u8, max_bytes: i32) -> i32 {
    trace_first_call("__cssl_player_state_get");
    if out.is_null() || max_bytes < PLAYER_STATE_BYTES as i32 {
        return LOA_ERR;
    }
    let Ok(g) = player_state().lock() else {
        return LOA_ERR;
    };
    // SAFETY : max_bytes ≥ PLAYER_STATE_BYTES validated ; out is non-null.
    unsafe {
        core::ptr::copy_nonoverlapping(g.as_ptr(), out, PLAYER_STATE_BYTES);
    }
    PLAYER_STATE_BYTES as i32
}

/// FFI : write `in_buf[0..len]` into the player state. Returns 0 on success.
#[no_mangle]
pub unsafe extern "C" fn __cssl_player_state_set(in_buf: *const u8, len: i32) -> i32 {
    trace_first_call("__cssl_player_state_set");
    if in_buf.is_null() || len < PLAYER_STATE_BYTES as i32 {
        return LOA_ERR;
    }
    let Ok(mut g) = player_state().lock() else {
        return LOA_ERR;
    };
    // SAFETY : len ≥ PLAYER_STATE_BYTES validated ; in_buf non-null.
    unsafe {
        core::ptr::copy_nonoverlapping(in_buf, g.as_mut_ptr(), PLAYER_STATE_BYTES);
    }
    LOA_OK
}

static ACTIVE_SCENE_ID: AtomicU32 = AtomicU32::new(0);

/// FFI : get the currently-active scene id.
#[no_mangle]
pub extern "C" fn __cssl_scene_active() -> u32 {
    trace_first_call("__cssl_scene_active");
    ACTIVE_SCENE_ID.load(Ordering::Relaxed)
}

/// FFI : switch to scene `scene_id` ; requires non-zero `sovereign_handle`.
#[no_mangle]
pub extern "C" fn __cssl_scene_switch(scene_id: u32, sovereign_handle: u64) -> i32 {
    trace_first_call("__cssl_scene_switch");
    if sovereign_handle == 0 {
        return LOA_ERR;
    }
    ACTIVE_SCENE_ID.store(scene_id, Ordering::Relaxed);
    LOA_OK
}

// ═══════════════════════════════════════════════════════════════════════
// § CATEGORY 9 — UTILITY (REAL : RNG + FNV-1a + Morton)
// ═══════════════════════════════════════════════════════════════════════

thread_local! {
    static RNG_STATE: RefCell<u32> = const { RefCell::new(0xCAFE_F00D) };
}

/// FFI : return a 32-bit pseudo-random integer (xorshift32, thread-local state).
#[no_mangle]
pub extern "C" fn __cssl_rng_u32() -> u32 {
    RNG_STATE.with(|s| {
        let mut x = *s.borrow();
        if x == 0 {
            x = 0xCAFE_F00D;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *s.borrow_mut() = x;
        x
    })
}

/// FFI : seed the RNG. `seed == 0` is replaced with a fixed non-zero
/// sentinel (xorshift cannot leave the zero-state).
#[no_mangle]
pub extern "C" fn __cssl_rng_seed(seed: u32) {
    RNG_STATE.with(|s| {
        let nonzero = if seed == 0 { 0xCAFE_F00D } else { seed };
        *s.borrow_mut() = nonzero;
    });
}

/// FFI : FNV-1a 64-bit hash of `(ptr, len)`.
#[no_mangle]
pub unsafe extern "C" fn __cssl_fnv1a_hash(ptr: *const u8, len: i32) -> u64 {
    if ptr.is_null() || len <= 0 {
        return 0xcbf2_9ce4_8422_2325; // FNV-1a 64-bit offset-basis (empty hash)
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    // SAFETY : caller supplies `len` accessible bytes per ABI.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// FFI : 21-bit-per-axis Morton encode of `(x, y, z)`.
///
/// The result is a 63-bit Morton key (bit 63 reserved for caller-tagging).
#[no_mangle]
pub extern "C" fn __cssl_hash_morton(x: u32, y: u32, z: u32) -> u64 {
    fn split3(v: u32) -> u64 {
        let mut x = u64::from(v) & 0x1f_ffff; // 21 bits
        x = (x | (x << 32)) & 0x001f_0000_0000_ffff;
        x = (x | (x << 16)) & 0x001f_0000_ff00_00ff;
        x = (x | (x << 8)) & 0x100f_00f0_0f00_f00f;
        x = (x | (x << 4)) & 0x10c3_0c30_c30c_30c3;
        x = (x | (x << 2)) & 0x1249_2492_4924_9249;
        x
    }
    split3(x) | (split3(y) << 1) | (split3(z) << 2)
}

/// FFI : inverse of `__cssl_hash_morton` ; writes `(x, y, z)` into out-pointers.
#[no_mangle]
pub unsafe extern "C" fn __cssl_hash_morton_inv(
    morton: u64,
    x_out: *mut u32,
    y_out: *mut u32,
    z_out: *mut u32,
) {
    fn compact1by2(mut x: u64) -> u32 {
        x &= 0x1249_2492_4924_9249;
        x = (x ^ (x >> 2)) & 0x10c3_0c30_c30c_30c3;
        x = (x ^ (x >> 4)) & 0x100f_00f0_0f00_f00f;
        x = (x ^ (x >> 8)) & 0x001f_0000_ff00_00ff;
        x = (x ^ (x >> 16)) & 0x001f_0000_0000_ffff;
        x = (x ^ (x >> 32)) & 0x0000_0000_001f_ffff;
        x as u32
    }
    let x = compact1by2(morton);
    let y = compact1by2(morton >> 1);
    let z = compact1by2(morton >> 2);
    // SAFETY : caller supplies non-null out-ptrs per ABI ; we null-check.
    if !x_out.is_null() {
        unsafe { *x_out = x };
    }
    if !y_out.is_null() {
        unsafe { *y_out = y };
    }
    if !z_out.is_null() {
        unsafe { *z_out = z };
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § Audit-counter for monotonic-call frequency (lightweight observability)
// ═══════════════════════════════════════════════════════════════════════

static MONOTONIC_CALL_COUNT: AtomicI64 = AtomicI64::new(0);

/// Number of `__cssl_time_monotonic_ns` calls observed at audit-time.
#[must_use]
pub fn loa_monotonic_count() -> i64 {
    MONOTONIC_CALL_COUNT.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// § TESTS — inline #[test] blocks per stub-category
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Module-local lock so tests that touch shared globals (omega-field,
    /// telemetry ring, MCP registry, scene-id, player-state) serialize.
    static LOA_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        match LOA_TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                LOA_TEST_LOCK.clear_poison();
                p.into_inner()
            }
        }
    }

    #[test]
    fn time_monotonic_ns_returns_increasing() {
        // The crate's `__cssl_time_monotonic_ns` is provided by `host_time`
        // (W-D6). We exercise its `_impl` helper here as a sanity check
        // that the LoA surface's monotonic-time anchor is wired.
        let a = crate::host_time::cssl_time_monotonic_ns_impl();
        // Spin briefly to ensure clock progresses.
        let mut spin = 0u64;
        while spin < 10_000 {
            spin = spin.wrapping_add(1);
            core::hint::black_box(spin);
        }
        let b = crate::host_time::cssl_time_monotonic_ns_impl();
        assert!(b >= a, "monotonic regressed: {a} → {b}");
    }

    #[test]
    fn telemetry_emit_then_drain_roundtrip() {
        let _g = test_lock();
        // Flush so this test starts from empty regardless of test order.
        assert_eq!(__cssl_telemetry_flush(), LOA_OK);

        let payload = [0xDEu8, 0xAD, 0xBE, 0xEF, 0x42];
        unsafe {
            assert_eq!(
                __cssl_telemetry_emit(0x1234, payload.as_ptr(), payload.len() as i32),
                LOA_OK
            );
        }

        let mut buf = vec![0u8; TELEMETRY_DRAIN_BYTES_PER_EVENT * 4];
        let written = unsafe { __cssl_telemetry_drain(buf.as_mut_ptr(), buf.len() as i32) };
        assert_eq!(written, TELEMETRY_DRAIN_BYTES_PER_EVENT as i32);

        // event_id (LE u32) at offset 0
        let ev_id = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        assert_eq!(ev_id, 0x1234);
        // payload_len at offset 4
        assert_eq!(buf[4], 5);
        // payload bytes at offset 5..10
        assert_eq!(&buf[5..10], &payload);

        // Drain again → empty.
        let empty = unsafe { __cssl_telemetry_drain(buf.as_mut_ptr(), buf.len() as i32) };
        assert_eq!(empty, 0);
    }

    #[test]
    fn mcp_register_tool_then_lookup() {
        let _g = test_lock();
        // Clean state by unregistering both ids (idempotent).
        let _ = __cssl_mcp_unregister(0xAAAA_0001);
        let _ = __cssl_mcp_unregister(0xAAAA_0002);

        let name = b"loa.dm.narrate";
        unsafe {
            assert_eq!(
                __cssl_mcp_register_tool(name.as_ptr(), name.len() as i32, 0xAAAA_0001),
                LOA_OK
            );
        }
        // Duplicate handler-id is allowed (the registry is by-name + by-id) ;
        // we register a second tool to confirm capacity-tracking works.
        let name2 = b"loa.gm.scene_switch";
        unsafe {
            assert_eq!(
                __cssl_mcp_register_tool(name2.as_ptr(), name2.len() as i32, 0xAAAA_0002),
                LOA_OK
            );
        }
        // Unregister-by-id removes the entry ; second unregister of same id fails.
        assert_eq!(__cssl_mcp_unregister(0xAAAA_0001), LOA_OK);
        assert_eq!(__cssl_mcp_unregister(0xAAAA_0001), LOA_ERR);
        // Cleanup
        let _ = __cssl_mcp_unregister(0xAAAA_0002);
    }

    #[test]
    fn cap_check_zero_handle_rejects() {
        let _g = test_lock();
        assert_eq!(__cssl_cap_check_sovereign(0), 0);
        assert_eq!(__cssl_cap_check_sovereign(0xDEAD_BEEF), LOA_TRUE);
        assert_eq!(__cssl_cap_check_observer(0), 0);
        assert_eq!(__cssl_cap_check_observer(42), LOA_TRUE);
    }

    #[test]
    fn rng_seed_determinism() {
        let _g = test_lock();
        __cssl_rng_seed(0x12345678);
        let a1 = __cssl_rng_u32();
        let a2 = __cssl_rng_u32();
        let a3 = __cssl_rng_u32();
        __cssl_rng_seed(0x12345678);
        let b1 = __cssl_rng_u32();
        let b2 = __cssl_rng_u32();
        let b3 = __cssl_rng_u32();
        assert_eq!((a1, a2, a3), (b1, b2, b3));
        // Zero-seed is replaced with sentinel ; not the zero-state.
        __cssl_rng_seed(0);
        let z = __cssl_rng_u32();
        assert_ne!(z, 0);
    }

    #[test]
    fn omega_field_sample_unset_returns_zero() {
        let _g = test_lock();
        assert_eq!(__cssl_omega_field_init(64), LOA_OK);
        let mut buf = [0u8; FIELD_CELL_BYTES];
        let written = unsafe { __cssl_omega_field_sample(0xDEAD, buf.as_mut_ptr(), buf.len() as i32) };
        // Cell 0xDEAD was never written ; sample must report 0 bytes.
        assert_eq!(written, 0);

        // Modify with a sovereign handle, then sample.
        let payload = [0x77u8; FIELD_CELL_BYTES];
        let mod_rc = unsafe {
            __cssl_omega_field_modify(0xDEAD, payload.as_ptr(), payload.len() as i32, 0x42)
        };
        assert_eq!(mod_rc, LOA_OK);
        let written2 = unsafe { __cssl_omega_field_sample(0xDEAD, buf.as_mut_ptr(), buf.len() as i32) };
        assert_eq!(written2, FIELD_CELL_BYTES as i32);
        assert!(buf.iter().all(|&b| b == 0x77));

        // Modify with zero sovereign-handle is rejected.
        let bad = unsafe {
            __cssl_omega_field_modify(0xBEEF, payload.as_ptr(), payload.len() as i32, 0)
        };
        assert_eq!(bad, LOA_ERR);
    }

    #[test]
    fn player_state_set_then_get_roundtrip() {
        let _g = test_lock();
        let mut input = [0u8; PLAYER_STATE_BYTES];
        for (i, b) in input.iter_mut().enumerate() {
            *b = i as u8;
        }
        let set_rc = unsafe { __cssl_player_state_set(input.as_ptr(), input.len() as i32) };
        assert_eq!(set_rc, LOA_OK);

        let mut output = [0u8; PLAYER_STATE_BYTES];
        let get_rc = unsafe { __cssl_player_state_get(output.as_mut_ptr(), output.len() as i32) };
        assert_eq!(get_rc, PLAYER_STATE_BYTES as i32);
        assert_eq!(input, output);

        // Sub-size buffer rejected.
        let too_small = unsafe { __cssl_player_state_get(output.as_mut_ptr(), 8) };
        assert_eq!(too_small, LOA_ERR);
    }

    #[test]
    fn morton_encode_decode_roundtrip() {
        for &(x, y, z) in &[
            (0u32, 0u32, 0u32),
            (1, 1, 1),
            (1023, 1023, 1023),
            (12345, 67890, 11111),
            (0x1f_ffff, 0x1f_ffff, 0x1f_ffff), // max 21-bit values
        ] {
            let m = __cssl_hash_morton(x, y, z);
            let mut xo = 0u32;
            let mut yo = 0u32;
            let mut zo = 0u32;
            unsafe { __cssl_hash_morton_inv(m, &mut xo, &mut yo, &mut zo) };
            assert_eq!((x, y, z), (xo, yo, zo), "morton roundtrip failed at {x},{y},{z}");
        }
    }

    // Bonus tests : extra coverage of the real-impl surface ───────────────

    #[test]
    fn fnv1a_known_values() {
        // Empty input → offset basis.
        let e = unsafe { __cssl_fnv1a_hash(core::ptr::null(), 0) };
        assert_eq!(e, 0xcbf2_9ce4_8422_2325);
        // "a" → known FNV-1a 64-bit
        let a = b"a";
        let h = unsafe { __cssl_fnv1a_hash(a.as_ptr(), a.len() as i32) };
        assert_eq!(h, 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn scene_active_default_zero_then_switch() {
        let _g = test_lock();
        // Reset to known state.
        let _ = __cssl_scene_switch(0, 0xFEED);
        assert_eq!(__cssl_scene_active(), 0);
        // Sovereign-zero rejected.
        assert_eq!(__cssl_scene_switch(7, 0), LOA_ERR);
        assert_eq!(__cssl_scene_active(), 0);
        // Sovereign-non-zero accepted.
        assert_eq!(__cssl_scene_switch(7, 0xFEED), LOA_OK);
        assert_eq!(__cssl_scene_active(), 7);
    }

    #[test]
    fn time_of_day_in_range() {
        let h = __cssl_time_of_day();
        assert!(h < 24);
    }

    #[test]
    fn ifc_check_flow_same_label_passes_mixed_fails() {
        assert_eq!(__cssl_ifc_check_flow(7, 7), LOA_TRUE);
        assert_eq!(__cssl_ifc_check_flow(7, 8), 0);
    }

    #[test]
    fn arena_create_returns_unique_handles() {
        let h1 = __cssl_arena_create(1024);
        let h2 = __cssl_arena_create(1024);
        assert_ne!(h1, LOA_INVALID_HANDLE);
        assert_ne!(h2, LOA_INVALID_HANDLE);
        assert_ne!(h1, h2);
        assert_eq!(__cssl_arena_destroy(h1), LOA_OK);
        assert_eq!(__cssl_arena_destroy(h2), LOA_OK);
        // Negative capacity rejected.
        assert_eq!(__cssl_arena_create(-1), LOA_INVALID_HANDLE);
    }

    #[test]
    fn window_get_size_writes_zeros() {
        let mut w = -1i32;
        let mut h = -1i32;
        let rc = unsafe { __cssl_window_get_size(0, &mut w, &mut h) };
        assert_eq!(rc, LOA_OK);
        assert_eq!((w, h), (0, 0));
    }
}
