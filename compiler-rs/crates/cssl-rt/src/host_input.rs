//! § cssl-rt::host_input — host-input FFI surface (Wave-D4 / S5 ↳ § 24 HOST_FFI).
//!
//! § ROLE
//!   The platform-neutral input surface that CSSLv3-emitted code calls
//!   into via the `__cssl_input_*` ABI-stable extern "C" symbols. This
//!   module is the cssl-rt-side interlock between :
//!     a) source-level CSSLv3 code that pulls keyboard / mouse / gamepad
//!        snapshots through the §§ 04 `Input` effect-row + §§ 12
//!        `Cap<Input>` capability-witness ;
//!     b) the per-OS host-input back-ends (Win32 raw-input + XInput,
//!        Linux evdev + libudev, macOS IOKit HID Manager) that produce
//!        the actual snapshot bytes.
//!
//!   Per `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § input` the four
//!   symbols this slice exposes are :
//!
//!   ```text
//!   __cssl_input_keyboard_state(handle : u64,
//!                                out_ptr : *mut u8, max_len : usize) -> i32
//!   __cssl_input_mouse_state(handle : u64,
//!                              x_out : *mut i32, y_out : *mut i32,
//!                              btns_out : *mut u32) -> i32
//!   __cssl_input_mouse_delta(handle : u64,
//!                              dx_out : *mut i32, dy_out : *mut i32) -> i32
//!   __cssl_input_gamepad_state(idx : u32,
//!                                out_ptr : *mut u8, max_len : usize) -> i32
//!   ```
//!
//!   Renaming any of these symbols = major-version-bump per
//!   `specs/24_HOST_FFI.csl § DESIGN-PRINCIPLES P1`.
//!
//! § PRIME-DIRECTIVE attestation  (W! read-first per § 24 + § 11)
//!   Input is the most surveillance-adjacent surface in the runtime
//!   stack — every byte is the user's most personal real-time signal.
//!   This module enforces FOUR structural safeguards that mirror the
//!   `crate::net` discipline :
//!
//!   ```text
//!   1. CAPABILITY-GATED : every input op consults the per-thread
//!      `INPUT_CAPS` bitset. The default is INPUT_CAP_NONE — no
//!      keyboard / mouse / gamepad reads succeed until the host
//!      explicitly grants `INPUT_CAP_KEYBOARD` / `INPUT_CAP_MOUSE`
//!      / `INPUT_CAP_GAMEPAD`. Source-level code asks for caps via
//!      `__cssl_input_caps_grant` (host-managed signed-token in
//!      stage-1 ; dev-mode bypass via `cssl-rt-dev-cap-grant` env
//!      var per `cssl-rt::net` precedent).
//!
//!   2. SENSITIVE<BEHAVIORAL> : per `specs/24_HOST_FFI.csl § IFC-LABELS`,
//!      mouse-delta data is `Sensitive<Behavioral>` and NEVER
//!      egresses cross-process. The §§ 11 IFC machinery (see
//!      specs/11_IFC.csl) rejects compositions that compose
//!      `Sensitive<Behavioral>` with `Net` outright at compile-time ;
//!      this module's runtime guard is a backstop that records every
//!      mouse-delta read into the local audit counters but does NOT
//!      export bytes (counters only).
//!
//!   3. NO COVERT CHANNELS : the FFI surface accepts raw output
//!      pointers + max-len caps ; over-reads are rejected with
//!      [`net_error_code::INVALID_INPUT`]-class err codes mapped onto
//!      the `INPUT_ERR_*` namespace below. There is no IO-redirect
//!      / no key-injection / no mouse-warp surface in stage-0.
//!
//!   4. AUDIT VISIBILITY : every successful read increments a public
//!      atomic counter (`keyboard_read_count` / `mouse_read_count` /
//!      `mouse_delta_read_count` / `gamepad_read_count`). Counters
//!      are LOCAL — nothing escapes the process.
//!   ```
//!
//!   What this module does NOT do : keystroke logging, surveillance,
//!   exfiltration, hidden side-channels. The surface is intentionally
//!   minimal + auditable. No flag / config / env-var / cli-arg /
//!   api-call / runtime-cond can disable these protections, per
//!   `PRIME_DIRECTIVE.md § 6 SCOPE`.
//!
//! § STAGE-0 SCOPE  (Wave-D4 / this slice)
//!   - ABI-stable symbol surface only. Stage-0 stores LOCAL last-known
//!     snapshots in atomics + per-handle slot tables (no real OS pump).
//!   - Stage-1 wraps `cssl-host-input` properly via the `pump_into`
//!     interlock once `cssl-rt → cssl-host-input` Cargo edge lands
//!     (deferred per Wave-D4 task constraint : no Cargo.toml edits).
//!   - Telemetry-ring integration deferred (matches `crate::net` /
//!     `crate::io` pattern at S6-B5 / S7-F4).
//!
//! § SAWYER-EFFICIENCY
//!   - Keyboard-state = 256-bit bitset = `[u8; 32]` packed. Source-level
//!     code reads via FFI memcpy ; max-len enforces 32-byte ceiling.
//!   - Mouse-state = single 16-byte struct (`x : i32`, `y : i32`,
//!     `btns : u32`, `pad : u32`). Serialized to `[u8; 16]` via
//!     little-endian byte-order (cross-host portable).
//!   - Mouse-delta = two `i32`s only ; NO heap, NO struct serialization,
//!     direct write through the caller's `dx_out` / `dy_out` pointers.
//!   - Per-handle slot table = fixed 16-slot ring (matches XInput's
//!     4-controller cap × 4 windows for typical multi-monitor setups).
//!     Slot lookup is a linear-scan over 16 entries — branch-friendly +
//!     cache-resident in a single 64-byte cache line.
//!   - All counters are atomic ⇒ thread-safe from day-1.

#![allow(unsafe_code)]

use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § Public sizing constants
// ───────────────────────────────────────────────────────────────────────

/// Keyboard-state bitset width in bytes = 32 (256 bits).
///
/// Each bit i ∈ [0, 256) corresponds to the host-input `KeyCode`
/// discriminant ordinal i (see `cssl-host-input::event::KeyCode`).
/// 256 = 2^8 ; matches the hardware-ish upper bound for HID keycodes
/// (Win32 vk_codes max ≈ 254, Linux KEY_MAX ≈ 255).
pub const KEYBOARD_STATE_BYTES: usize = 32;

/// Mouse-state serialized width in bytes = 16.
///
/// Layout (little-endian per host) :
/// - bytes [0..4]   : `x : i32`         (cursor x-coord, host px)
/// - bytes [4..8]   : `y : i32`         (cursor y-coord, host px)
/// - bytes [8..12]  : `btns : u32`      (button-mask bit-i = btn-i held)
/// - bytes [12..16] : reserved (zero)   (future scroll-x/y or modifier)
pub const MOUSE_STATE_BYTES: usize = 16;

/// Gamepad-state serialized width in bytes = 32.
///
/// Layout (little-endian per host) :
/// - bytes [0..2]    : `buttons : u16`   (16-bit button bitmap)
/// - bytes [2..4]    : `flags   : u16`   (bit-0 = connected)
/// - bytes [4..28]   : `axes    : [i16; 12]`  (LX, LY, RX, RY, LT, RT, +6 reserved)
/// - bytes [28..32]  : reserved
pub const GAMEPAD_STATE_BYTES: usize = 32;

/// Maximum number of input-handle slots tracked simultaneously = 16.
///
/// Per the spec § window § input : one input-handle per window-handle ;
/// 16 = 4-window × 4-monitor headroom for Apocky's primary host (Win11
/// + Arc A770) + future split-screen layouts.
pub const INPUT_HANDLE_SLOT_COUNT: usize = 16;

/// Maximum number of gamepad slots = 4 (XInput hard cap).
///
/// Per `specs/24_HOST_FFI.csl § input` + `cssl-host-input::win32`
/// landmines : XInput 1.4's 4-controller cap is the lowest-common
/// denominator across host back-ends. Linux + macOS expose more, but
/// the FFI surface mirrors the cap so source-level code is portable.
pub const GAMEPAD_SLOT_COUNT: usize = 4;

// ───────────────────────────────────────────────────────────────────────
// § FFI return-code namespace
// ───────────────────────────────────────────────────────────────────────

/// Successful read.
pub const INPUT_OK: i32 = 0;

/// Output buffer too small (e.g. `max_len < KEYBOARD_STATE_BYTES`).
pub const INPUT_ERR_BUFFER_TOO_SMALL: i32 = -1;

/// Null output pointer.
pub const INPUT_ERR_NULL_OUT: i32 = -2;

/// Unknown handle / handle never registered / slot table exhausted.
pub const INPUT_ERR_INVALID_HANDLE: i32 = -3;

/// Gamepad index out-of-range (≥ [`GAMEPAD_SLOT_COUNT`]).
pub const INPUT_ERR_INVALID_INDEX: i32 = -4;

/// Gamepad disconnected (or never connected) — output is filled with
/// zeros + `flags & 1 == 0`.
pub const INPUT_ERR_DISCONNECTED: i32 = -5;

/// Capability not granted for this op.
pub const INPUT_ERR_CAP_DENIED: i32 = -6;

// ───────────────────────────────────────────────────────────────────────
// § Capability bitset
//
// Mirrors `cssl-rt::net::NET_CAP_*` discipline. Default = INPUT_CAP_NONE ;
// host code grants caps via `set_input_caps_for_tests` (stage-0) or via
// signed-token machinery (stage-1+). Per PRIME-DIRECTIVE there is NO
// override path that disables this machinery.
// ───────────────────────────────────────────────────────────────────────

/// No caps granted (default).
pub const INPUT_CAP_NONE: i32 = 0;

/// Keyboard reads allowed.
pub const INPUT_CAP_KEYBOARD: i32 = 1 << 0;

/// Mouse-state reads allowed (cursor position + button mask).
pub const INPUT_CAP_MOUSE: i32 = 1 << 1;

/// Mouse-delta reads allowed (Sensitive<Behavioral> per § 24 IFC).
pub const INPUT_CAP_MOUSE_DELTA: i32 = 1 << 2;

/// Gamepad-state reads allowed.
pub const INPUT_CAP_GAMEPAD: i32 = 1 << 3;

/// Mask of all defined cap bits.
pub const INPUT_CAP_MASK: i32 =
    INPUT_CAP_KEYBOARD | INPUT_CAP_MOUSE | INPUT_CAP_MOUSE_DELTA | INPUT_CAP_GAMEPAD;

static INPUT_CAPS: AtomicI32 = AtomicI32::new(INPUT_CAP_NONE);

/// Returns the current per-process input-cap bitset.
///
/// Per the §§ 12 capability machinery sketch in `specs/24_HOST_FFI.csl`
/// stage-1 will widen this to a per-thread bitset ; stage-0 uses one
/// process-wide AtomicI32 for simplicity.
#[must_use]
pub fn caps_current() -> i32 {
    INPUT_CAPS.load(Ordering::Acquire)
}

/// OR-grants `bits` into the cap bitset. Returns the new bitset.
///
/// Stage-1 will reject bits not present in a host-signed token ;
/// stage-0 trusts the caller (matching `cssl-rt::net::caps_grant`).
pub fn caps_grant(bits: i32) -> i32 {
    let masked = bits & INPUT_CAP_MASK;
    INPUT_CAPS.fetch_or(masked, Ordering::AcqRel) | masked
}

/// AND-NOT-removes `bits` from the cap bitset. Returns the new bitset.
pub fn caps_revoke(bits: i32) -> i32 {
    let masked = bits & INPUT_CAP_MASK;
    INPUT_CAPS.fetch_and(!masked, Ordering::AcqRel) & !masked
}

/// Returns true if `bits` are all currently granted.
#[must_use]
pub fn caps_satisfied(bits: i32) -> bool {
    let cur = caps_current();
    (cur & bits) == bits
}

// ───────────────────────────────────────────────────────────────────────
// § Audit counters
//
// Mirrors `crate::net` + `crate::io` discipline : LOCAL atomic counters
// every successful read increments. Nothing escapes the process.
// ───────────────────────────────────────────────────────────────────────

static KEYBOARD_READ_COUNT: AtomicU64 = AtomicU64::new(0);
static MOUSE_READ_COUNT: AtomicU64 = AtomicU64::new(0);
static MOUSE_DELTA_READ_COUNT: AtomicU64 = AtomicU64::new(0);
static GAMEPAD_READ_COUNT: AtomicU64 = AtomicU64::new(0);
static INPUT_ERROR_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_INPUT_ERROR: AtomicI32 = AtomicI32::new(INPUT_OK);

/// Public count of successful keyboard-state reads.
#[must_use]
pub fn keyboard_read_count() -> u64 {
    KEYBOARD_READ_COUNT.load(Ordering::Acquire)
}

/// Public count of successful mouse-state reads.
#[must_use]
pub fn mouse_read_count() -> u64 {
    MOUSE_READ_COUNT.load(Ordering::Acquire)
}

/// Public count of successful mouse-delta reads. Per § 24 IFC this
/// counter increments but no delta-bytes egress.
#[must_use]
pub fn mouse_delta_read_count() -> u64 {
    MOUSE_DELTA_READ_COUNT.load(Ordering::Acquire)
}

/// Public count of successful gamepad-state reads.
#[must_use]
pub fn gamepad_read_count() -> u64 {
    GAMEPAD_READ_COUNT.load(Ordering::Acquire)
}

/// Public count of all input-FFI errors.
#[must_use]
pub fn input_error_count() -> u64 {
    INPUT_ERROR_COUNT.load(Ordering::Acquire)
}

/// Public last-error-code from the most-recent failing FFI call (or
/// `INPUT_OK` if none has occurred yet).
#[must_use]
pub fn last_input_error() -> i32 {
    LAST_INPUT_ERROR.load(Ordering::Acquire)
}

#[inline]
fn record_input_error(code: i32) -> i32 {
    INPUT_ERROR_COUNT.fetch_add(1, Ordering::AcqRel);
    LAST_INPUT_ERROR.store(code, Ordering::Release);
    code
}

// ───────────────────────────────────────────────────────────────────────
// § Per-handle snapshot store (stage-0 in-process state)
//
// Stage-0 keeps a fixed-size slot table : (handle, keyboard_bits,
// mouse_state_bytes, last_mouse_pos). Slot[0] is the "global" slot
// returned for handle == 0 (which the spec § 24 input documents as
// "active window" ; stage-1 binds handle ↔ host-window).
// ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct HandleSlot {
    handle: u64,                                      // 0 = unused-slot sentinel ; 0 = "global" too
    in_use: bool,
    keyboard: [u8; KEYBOARD_STATE_BYTES],
    mouse: [u8; MOUSE_STATE_BYTES],
    last_mouse_x: i32,
    last_mouse_y: i32,
    pending_dx: i32,
    pending_dy: i32,
}

impl HandleSlot {
    const fn empty() -> Self {
        Self {
            handle: 0,
            in_use: false,
            keyboard: [0; KEYBOARD_STATE_BYTES],
            mouse: [0; MOUSE_STATE_BYTES],
            last_mouse_x: 0,
            last_mouse_y: 0,
            pending_dx: 0,
            pending_dy: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct GamepadSlot {
    bytes: [u8; GAMEPAD_STATE_BYTES],
    connected: bool,
}

impl GamepadSlot {
    const fn empty() -> Self {
        Self {
            bytes: [0; GAMEPAD_STATE_BYTES],
            connected: false,
        }
    }
}

// § Spinlock-guarded slot tables. Stage-0 uses a Mutex (std-available
// since cssl-rt is hosted-only this slice). Stage-1 will swap to a
// fixed-capacity lock-free table per the cssl-host-input lock-free
// queue pattern in `cssl-host-input::win32`.
use std::sync::Mutex;

static HANDLE_SLOTS: Mutex<[HandleSlot; INPUT_HANDLE_SLOT_COUNT]> =
    Mutex::new([HandleSlot::empty(); INPUT_HANDLE_SLOT_COUNT]);

static GAMEPAD_SLOTS: Mutex<[GamepadSlot; GAMEPAD_SLOT_COUNT]> =
    Mutex::new([GamepadSlot::empty(); GAMEPAD_SLOT_COUNT]);

// § Atomic pending-mouse-delta counters for the global handle (handle=0)
// so the most-common code path (single-window, no slot-table contention)
// can run lock-free. Used as the source-of-truth for handle 0 ; per-handle
// slots take the slow path through HANDLE_SLOTS.
static GLOBAL_PENDING_DX: AtomicI32 = AtomicI32::new(0);
static GLOBAL_PENDING_DY: AtomicI32 = AtomicI32::new(0);
static GLOBAL_MOUSE_X: AtomicI32 = AtomicI32::new(0);
static GLOBAL_MOUSE_Y: AtomicI32 = AtomicI32::new(0);
static GLOBAL_MOUSE_BTNS: AtomicU32 = AtomicU32::new(0);

#[inline]
fn poison_safe_lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => {
            m.clear_poison();
            p.into_inner()
        }
    }
}

/// Find slot for `handle`. Returns `Some(idx)` if found OR if a free slot
/// is available (slot-acquired on first write). `None` if no slot can be
/// allocated.
fn find_or_acquire_slot(slots: &mut [HandleSlot; INPUT_HANDLE_SLOT_COUNT], handle: u64) -> Option<usize> {
    // Linear scan : look for matching handle first.
    for (i, slot) in slots.iter().enumerate() {
        if slot.in_use && slot.handle == handle {
            return Some(i);
        }
    }
    // Acquire free slot.
    for (i, slot) in slots.iter_mut().enumerate() {
        if !slot.in_use {
            slot.handle = handle;
            slot.in_use = true;
            return Some(i);
        }
    }
    None
}

fn find_existing_slot(slots: &[HandleSlot; INPUT_HANDLE_SLOT_COUNT], handle: u64) -> Option<usize> {
    for (i, slot) in slots.iter().enumerate() {
        if slot.in_use && slot.handle == handle {
            return Some(i);
        }
    }
    None
}

// ───────────────────────────────────────────────────────────────────────
// § Stage-0 host-side state injection (used by host pump + tests)
//
// Real OS-event-pump implementations (cssl-host-input::win32 et al.)
// will call these to update the stage-0 cache before source-level code
// reads it back via the FFI. Stage-0 = no real pump ; tests call these
// directly to simulate input events. Per `crate::net` precedent the
// stage-1 wrapping point will route the host-input crate's snapshot
// updates through these setters.
// ───────────────────────────────────────────────────────────────────────

/// Inject keyboard-bitset state for `handle`. Bytes beyond `bits.len()`
/// keep their previous value ; bytes in `bits` overwrite. `bits.len()`
/// must be `≤ KEYBOARD_STATE_BYTES`.
pub fn set_keyboard_state(handle: u64, bits: &[u8]) -> i32 {
    if bits.len() > KEYBOARD_STATE_BYTES {
        return INPUT_ERR_BUFFER_TOO_SMALL;
    }
    let mut slots = poison_safe_lock(&HANDLE_SLOTS);
    let idx = match find_or_acquire_slot(&mut slots, handle) {
        Some(i) => i,
        None => return INPUT_ERR_INVALID_HANDLE,
    };
    slots[idx].keyboard[..bits.len()].copy_from_slice(bits);
    INPUT_OK
}

/// Inject mouse-state for `handle`. The `(x, y, btns)` triple overwrites
/// the cached mouse-state ; the pending-delta accumulator records
/// `(x - last_x, y - last_y)` so subsequent `__cssl_input_mouse_delta`
/// calls observe the movement.
pub fn set_mouse_state(handle: u64, x: i32, y: i32, btns: u32) -> i32 {
    if handle == 0 {
        // Global fast-path : update atomics.
        let prev_x = GLOBAL_MOUSE_X.load(Ordering::Acquire);
        let prev_y = GLOBAL_MOUSE_Y.load(Ordering::Acquire);
        let dx = x.wrapping_sub(prev_x);
        let dy = y.wrapping_sub(prev_y);
        GLOBAL_MOUSE_X.store(x, Ordering::Release);
        GLOBAL_MOUSE_Y.store(y, Ordering::Release);
        GLOBAL_MOUSE_BTNS.store(btns, Ordering::Release);
        GLOBAL_PENDING_DX.fetch_add(dx, Ordering::AcqRel);
        GLOBAL_PENDING_DY.fetch_add(dy, Ordering::AcqRel);
        return INPUT_OK;
    }
    let mut slots = poison_safe_lock(&HANDLE_SLOTS);
    let idx = match find_or_acquire_slot(&mut slots, handle) {
        Some(i) => i,
        None => return INPUT_ERR_INVALID_HANDLE,
    };
    let dx = x.wrapping_sub(slots[idx].last_mouse_x);
    let dy = y.wrapping_sub(slots[idx].last_mouse_y);
    slots[idx].last_mouse_x = x;
    slots[idx].last_mouse_y = y;
    slots[idx].pending_dx = slots[idx].pending_dx.wrapping_add(dx);
    slots[idx].pending_dy = slots[idx].pending_dy.wrapping_add(dy);
    // Serialize to bytes : x | y | btns | reserved (16 bytes).
    slots[idx].mouse[0..4].copy_from_slice(&x.to_le_bytes());
    slots[idx].mouse[4..8].copy_from_slice(&y.to_le_bytes());
    slots[idx].mouse[8..12].copy_from_slice(&btns.to_le_bytes());
    slots[idx].mouse[12..16].copy_from_slice(&0u32.to_le_bytes());
    INPUT_OK
}

/// Inject gamepad-state for `idx`. `bytes.len()` ≤ `GAMEPAD_STATE_BYTES`.
/// `connected = true` flips bit-0 of the `flags` field on injection ;
/// disconnected pads are not callable (the FFI returns
/// `INPUT_ERR_DISCONNECTED`).
pub fn set_gamepad_state(idx: u32, bytes: &[u8], connected: bool) -> i32 {
    if (idx as usize) >= GAMEPAD_SLOT_COUNT {
        return INPUT_ERR_INVALID_INDEX;
    }
    if bytes.len() > GAMEPAD_STATE_BYTES {
        return INPUT_ERR_BUFFER_TOO_SMALL;
    }
    let mut slots = poison_safe_lock(&GAMEPAD_SLOTS);
    let slot = &mut slots[idx as usize];
    slot.bytes[..bytes.len()].copy_from_slice(bytes);
    // Force flags bit-0 = connected.
    let flags_bytes = if bytes.len() >= 4 {
        let mut f = u16::from_le_bytes([slot.bytes[2], slot.bytes[3]]);
        if connected {
            f |= 1;
        } else {
            f &= !1;
        }
        f
    } else if connected {
        1
    } else {
        0
    };
    slot.bytes[2..4].copy_from_slice(&flags_bytes.to_le_bytes());
    slot.connected = connected;
    INPUT_OK
}

/// Reset all stage-0 caches to default. Used by tests.
pub fn reset_for_tests() {
    let mut slots = poison_safe_lock(&HANDLE_SLOTS);
    *slots = [HandleSlot::empty(); INPUT_HANDLE_SLOT_COUNT];
    drop(slots);
    let mut g = poison_safe_lock(&GAMEPAD_SLOTS);
    *g = [GamepadSlot::empty(); GAMEPAD_SLOT_COUNT];
    drop(g);
    GLOBAL_PENDING_DX.store(0, Ordering::Release);
    GLOBAL_PENDING_DY.store(0, Ordering::Release);
    GLOBAL_MOUSE_X.store(0, Ordering::Release);
    GLOBAL_MOUSE_Y.store(0, Ordering::Release);
    GLOBAL_MOUSE_BTNS.store(0, Ordering::Release);
    KEYBOARD_READ_COUNT.store(0, Ordering::Release);
    MOUSE_READ_COUNT.store(0, Ordering::Release);
    MOUSE_DELTA_READ_COUNT.store(0, Ordering::Release);
    GAMEPAD_READ_COUNT.store(0, Ordering::Release);
    INPUT_ERROR_COUNT.store(0, Ordering::Release);
    LAST_INPUT_ERROR.store(INPUT_OK, Ordering::Release);
    INPUT_CAPS.store(INPUT_CAP_NONE, Ordering::Release);
}

// ───────────────────────────────────────────────────────────────────────
// § Internal _impl helpers (pure-Rust, callable from tests + FFI).
//
// Mirrors `crate::net::*` 3-layer pattern : FFI → _impl → state-store.
// _impl fns are safe + return `i32` ; FFI fns are unsafe + delegate.
// ───────────────────────────────────────────────────────────────────────

fn keyboard_state_impl(handle: u64, out: &mut [u8]) -> i32 {
    if !caps_satisfied(INPUT_CAP_KEYBOARD) {
        return record_input_error(INPUT_ERR_CAP_DENIED);
    }
    if out.len() < KEYBOARD_STATE_BYTES {
        return record_input_error(INPUT_ERR_BUFFER_TOO_SMALL);
    }
    let slots = poison_safe_lock(&HANDLE_SLOTS);
    if let Some(idx) = find_existing_slot(&slots, handle) {
        out[..KEYBOARD_STATE_BYTES].copy_from_slice(&slots[idx].keyboard);
    } else {
        // Unknown handle : zero the buffer (cleanest "no input" state).
        out[..KEYBOARD_STATE_BYTES].fill(0);
    }
    KEYBOARD_READ_COUNT.fetch_add(1, Ordering::AcqRel);
    INPUT_OK
}

fn mouse_state_impl(handle: u64, x_out: &mut i32, y_out: &mut i32, btns_out: &mut u32) -> i32 {
    if !caps_satisfied(INPUT_CAP_MOUSE) {
        return record_input_error(INPUT_ERR_CAP_DENIED);
    }
    if handle == 0 {
        *x_out = GLOBAL_MOUSE_X.load(Ordering::Acquire);
        *y_out = GLOBAL_MOUSE_Y.load(Ordering::Acquire);
        *btns_out = GLOBAL_MOUSE_BTNS.load(Ordering::Acquire);
    } else {
        let slots = poison_safe_lock(&HANDLE_SLOTS);
        if let Some(idx) = find_existing_slot(&slots, handle) {
            let mb = &slots[idx].mouse;
            *x_out = i32::from_le_bytes([mb[0], mb[1], mb[2], mb[3]]);
            *y_out = i32::from_le_bytes([mb[4], mb[5], mb[6], mb[7]]);
            *btns_out = u32::from_le_bytes([mb[8], mb[9], mb[10], mb[11]]);
        } else {
            *x_out = 0;
            *y_out = 0;
            *btns_out = 0;
        }
    }
    MOUSE_READ_COUNT.fetch_add(1, Ordering::AcqRel);
    INPUT_OK
}

fn mouse_delta_impl(handle: u64, dx_out: &mut i32, dy_out: &mut i32) -> i32 {
    if !caps_satisfied(INPUT_CAP_MOUSE_DELTA) {
        return record_input_error(INPUT_ERR_CAP_DENIED);
    }
    if handle == 0 {
        // Atomic swap-with-zero : reads current pending and zeros it
        // atomically. This is the canonical "consume-pending-delta"
        // pattern for input handlers (matches how Win32 raw-input
        // backends drain `RAWMOUSE.lLastX/Y` per WM_INPUT message).
        *dx_out = GLOBAL_PENDING_DX.swap(0, Ordering::AcqRel);
        *dy_out = GLOBAL_PENDING_DY.swap(0, Ordering::AcqRel);
    } else {
        let mut slots = poison_safe_lock(&HANDLE_SLOTS);
        if let Some(idx) = find_existing_slot(&slots, handle) {
            *dx_out = slots[idx].pending_dx;
            *dy_out = slots[idx].pending_dy;
            slots[idx].pending_dx = 0;
            slots[idx].pending_dy = 0;
        } else {
            *dx_out = 0;
            *dy_out = 0;
        }
    }
    // § Sensitive<Behavioral> per § 24 IFC : counter increments but
    // nothing about the delta-bytes egresses.
    MOUSE_DELTA_READ_COUNT.fetch_add(1, Ordering::AcqRel);
    INPUT_OK
}

fn gamepad_state_impl(idx: u32, out: &mut [u8]) -> i32 {
    if !caps_satisfied(INPUT_CAP_GAMEPAD) {
        return record_input_error(INPUT_ERR_CAP_DENIED);
    }
    if (idx as usize) >= GAMEPAD_SLOT_COUNT {
        return record_input_error(INPUT_ERR_INVALID_INDEX);
    }
    if out.len() < GAMEPAD_STATE_BYTES {
        return record_input_error(INPUT_ERR_BUFFER_TOO_SMALL);
    }
    let slots = poison_safe_lock(&GAMEPAD_SLOTS);
    let slot = &slots[idx as usize];
    if !slot.connected {
        // Per § 24 input : disconnected pads return zeroed buffer + the
        // disconnected error-code so the caller sees both "shape valid"
        // (output is well-formed all-zero) AND "no signal" (errcode).
        out[..GAMEPAD_STATE_BYTES].fill(0);
        return record_input_error(INPUT_ERR_DISCONNECTED);
    }
    out[..GAMEPAD_STATE_BYTES].copy_from_slice(&slot.bytes);
    GAMEPAD_READ_COUNT.fetch_add(1, Ordering::AcqRel);
    INPUT_OK
}

// ───────────────────────────────────────────────────────────────────────
// § FFI : __cssl_input_*  (ABI-stable extern "C" symbols)
//
// ‼ Renaming any of these symbols is a major-version-bump event per
//   `specs/24_HOST_FFI.csl § DESIGN-PRINCIPLES P1`. CSSLv3-emitted code
//   references them by exact name.
// ───────────────────────────────────────────────────────────────────────

/// FFI : write the 256-bit keyboard-state bitset for `handle` into the
/// caller-provided `out_ptr` buffer (must be at least
/// [`KEYBOARD_STATE_BYTES`] = 32 bytes). Returns [`INPUT_OK`] on success
/// or a negative [`INPUT_ERR_*`] code on failure.
///
/// # Safety
/// Caller must ensure :
/// - `out_ptr` is non-null + valid for `max_len` writable bytes,
/// - `max_len ≥ KEYBOARD_STATE_BYTES` (else returns `INPUT_ERR_BUFFER_TOO_SMALL`
///   without writing).
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_keyboard_state(
    handle: u64,
    out_ptr: *mut u8,
    max_len: usize,
) -> i32 {
    if out_ptr.is_null() {
        return record_input_error(INPUT_ERR_NULL_OUT);
    }
    if max_len < KEYBOARD_STATE_BYTES {
        return record_input_error(INPUT_ERR_BUFFER_TOO_SMALL);
    }
    // SAFETY : `out_ptr` non-null + `max_len` ≥ KEYBOARD_STATE_BYTES per
    // contract above. Lifetime of the slice ends before this fn returns.
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, max_len) };
    keyboard_state_impl(handle, out)
}

/// FFI : write the current mouse cursor x/y + button-mask for `handle`
/// into the caller-provided out-pointers. Returns [`INPUT_OK`] on success
/// or a negative [`INPUT_ERR_*`] code on failure.
///
/// # Safety
/// Caller must ensure all three out-pointers are non-null + writable.
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_mouse_state(
    handle: u64,
    x_out: *mut i32,
    y_out: *mut i32,
    btns_out: *mut u32,
) -> i32 {
    if x_out.is_null() || y_out.is_null() || btns_out.is_null() {
        return record_input_error(INPUT_ERR_NULL_OUT);
    }
    let mut x = 0i32;
    let mut y = 0i32;
    let mut btns = 0u32;
    let rc = mouse_state_impl(handle, &mut x, &mut y, &mut btns);
    if rc == INPUT_OK {
        // SAFETY : non-null check above ; lifetimes end at fn-return.
        unsafe {
            *x_out = x;
            *y_out = y;
            *btns_out = btns;
        }
    }
    rc
}

/// FFI : write the pending mouse-delta for `handle` into the caller-
/// provided out-pointers, draining the accumulator atomically.
///
/// § PRIME-DIRECTIVE — `Sensitive<Behavioral>` per § 24 IFC :
/// the delta-read counter increments but nothing about the delta-bytes
/// egresses cross-process from cssl-rt.
///
/// Returns [`INPUT_OK`] on success or a negative [`INPUT_ERR_*`] code.
///
/// # Safety
/// Caller must ensure both out-pointers are non-null + writable.
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_mouse_delta(
    handle: u64,
    dx_out: *mut i32,
    dy_out: *mut i32,
) -> i32 {
    if dx_out.is_null() || dy_out.is_null() {
        return record_input_error(INPUT_ERR_NULL_OUT);
    }
    let mut dx = 0i32;
    let mut dy = 0i32;
    let rc = mouse_delta_impl(handle, &mut dx, &mut dy);
    if rc == INPUT_OK {
        // SAFETY : non-null check above ; lifetimes end at fn-return.
        unsafe {
            *dx_out = dx;
            *dy_out = dy;
        }
    }
    rc
}

/// FFI : write the gamepad-state for slot `idx` into the caller-provided
/// `out_ptr` buffer (must be at least [`GAMEPAD_STATE_BYTES`] = 32 bytes).
///
/// Disconnected pads zero the buffer + return [`INPUT_ERR_DISCONNECTED`]
/// (-5). The cssl-host-input layer flags slot.connected = false when the
/// OS reports the controller is unplugged ; the error-code lets source-
/// level code skip update logic without re-checking the bytes.
///
/// # Safety
/// Caller must ensure `out_ptr` is non-null + valid for `max_len`
/// writable bytes.
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_gamepad_state(
    idx: u32,
    out_ptr: *mut u8,
    max_len: usize,
) -> i32 {
    if out_ptr.is_null() {
        return record_input_error(INPUT_ERR_NULL_OUT);
    }
    if max_len < GAMEPAD_STATE_BYTES {
        return record_input_error(INPUT_ERR_BUFFER_TOO_SMALL);
    }
    // SAFETY : `out_ptr` non-null + `max_len` ≥ GAMEPAD_STATE_BYTES.
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, max_len) };
    gamepad_state_impl(idx, out)
}

// ───────────────────────────────────────────────────────────────────────
// § FFI : __cssl_input_caps_*  (T11-W19-β-RT-DELEG-INPUT new symbols)
//
//   Stage-0 stdlib::input::caps_grant was a no-op stub returning the
//   bits unchanged ; cssl-rt's INPUT_CAPS bitset stayed at NONE so every
//   __cssl_input_keyboard_state / mouse_state / mouse_delta / gamepad
//   call returned ERR_CAP_DENIED. Adding these three FFI-locked symbols
//   gives stdlib + source-level CSSLv3 a real path to grant input caps.
//
//   Symbol-name discipline matches the rest of the cssl-rt FFI surface :
//   ABI-stable from this commit forward. Renaming any of the three is
//   a major-version-bump event per spec/24 § DESIGN-PRINCIPLES P1.
//
//   Stage-1 will accept a host-signed token instead of trusting the
//   caller ; stage-0 mirrors `cssl-rt::net::caps_grant_impl` shape.
// ───────────────────────────────────────────────────────────────────────

/// FFI : OR-grant `bits` into the input-cap bitset. Returns the new bitset.
///
/// # Safety
/// Always safe ; the `unsafe` qualifier is only for `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_caps_grant(bits: i32) -> i32 {
    caps_grant(bits)
}

/// FFI : AND-NOT-revoke `bits` from the input-cap bitset. Returns new bitset.
///
/// # Safety
/// Always safe ; the `unsafe` qualifier is only for `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_caps_revoke(bits: i32) -> i32 {
    caps_revoke(bits)
}

/// FFI : return the current input-cap bitset.
///
/// # Safety
/// Always safe ; the `unsafe` qualifier is only for `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_caps_current() -> i32 {
    caps_current()
}

// ───────────────────────────────────────────────────────────────────────
// § cssl-host-input integration hook
//
//   This is the seam where cssl-host-window's WNDPROC will eventually
//   feed real WM_INPUT data into the cssl-rt cache. The function reads
//   the active backend's `current_state()` snapshot and writes its
//   keyboard / mouse / gamepad fields through the `set_*_state` setters
//   already present in this module.
//
//   Today : cssl-host-window's WNDPROC does NOT yet route raw-input
//   messages to a cssl_host_input::Win32Backend instance — that's the
//   F2-input-integration deferred slice noted in the cssl-host-input
//   docstring. So `flush_host_input_state` is a NO-OP at runtime
//   (zero events generated) but the symbol exists + the integration
//   path is laid down. When the WNDPROC integration lands, the body
//   here gets implemented + the engine starts seeing real keyboard /
//   mouse events without further FFI-shape changes.
//
//   Per task constraint : do NOT modify cssl-host-window's WNDPROC in
//   this slice (that's separate work). The hook below documents the
//   delegation seam.
// ───────────────────────────────────────────────────────────────────────

/// Stage-0 placeholder for the cssl-host-input → cssl-rt cache flush.
///
/// Returns 0 (no events flushed). When the F2-input-integration slice
/// lands, this fn will :
///   1. Acquire the active `cssl_host_input::Win32Backend` (from a
///      thread-local cell populated at window-spawn).
///   2. Call `backend.tick()` to pump WM_INPUT / XInput.
///   3. Call `backend.current_state()` + serialize keyboard / mouse /
///      gamepad fields through `set_keyboard_state` / `set_mouse_state`
///      / `set_gamepad_state` setters.
///   4. Return the number of events flushed.
///
/// # Safety
/// Always safe ; the `unsafe` qualifier is only for `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_input_flush_host_state() -> i32 {
    // SWAP-POINT : cssl-host-input::Win32Backend::tick() + current_state()
    // serialization. Today no real backend is wired (F2-input-integration
    // pending) ; the FFI symbol exists so source-level CSSL code can
    // call it without later re-link discipline.
    let _ = cssl_host_input::api::BackendKind::current();
    0
}

// ───────────────────────────────────────────────────────────────────────
// § Tests
//
// Coverage matrix (per Wave-D4 dispatch directive, ≥ 8 tests) :
//   - keyboard-state-shape : sized buffer, bit-pattern roundtrip, too-small reject
//   - mouse-state-roundtrip : x/y/btns triple preserved, null-out reject
//   - mouse-delta-zero-on-no-movement : drain-after-no-update reads (0,0)
//   - mouse-delta-accumulates : two writes accumulate before drain
//   - mouse-delta-drains-after-read : second read after drain reads (0,0)
//   - gamepad-disconnected-handling : disconnected slot returns ERR + zero bytes
//   - gamepad-connected-roundtrip : bytes preserved
//   - gamepad-out-of-range : idx ≥ 4 rejected
//   - cap-denied : no-cap → ERR_CAP_DENIED
//   - cap-grant-revoke : grant + revoke + grant cycle works
//   - audit-counter-increments : every successful op bumps its counter
//   - constants-shape : sizing constants are stable
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    // § One module-level test lock — input tests touch globals.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = match TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                TEST_LOCK.clear_poison();
                p.into_inner()
            }
        };
        reset_for_tests();
        g
    }

    fn grant_all() {
        caps_grant(INPUT_CAP_MASK);
    }

    // ── constants-shape ─────────────────────────────────────────────────

    #[test]
    fn constants_have_canonical_sizes() {
        let _g = lock_and_reset();
        assert_eq!(KEYBOARD_STATE_BYTES, 32, "256-bit keyboard bitset = 32 bytes");
        assert_eq!(MOUSE_STATE_BYTES, 16, "mouse-state struct = 16 bytes");
        assert_eq!(GAMEPAD_STATE_BYTES, 32, "gamepad-state struct = 32 bytes");
        assert_eq!(GAMEPAD_SLOT_COUNT, 4, "XInput cap = 4");
        assert!(INPUT_HANDLE_SLOT_COUNT >= 4, "handle slots ≥ 4");
    }

    // ── keyboard-state-shape ───────────────────────────────────────────

    #[test]
    fn keyboard_state_shape_roundtrip() {
        let _g = lock_and_reset();
        grant_all();
        // Simulate keys A (bit 65), space (bit 32), and Esc (bit 27) held.
        let mut bits = [0u8; KEYBOARD_STATE_BYTES];
        bits[27 / 8] |= 1 << (27 % 8);
        bits[32 / 8] |= 1 << (32 % 8);
        bits[65 / 8] |= 1 << (65 % 8);
        let h = 0x1234u64;
        let rc = set_keyboard_state(h, &bits);
        assert_eq!(rc, INPUT_OK);
        let mut out = [0u8; KEYBOARD_STATE_BYTES];
        let rc = unsafe { __cssl_input_keyboard_state(h, out.as_mut_ptr(), out.len()) };
        assert_eq!(rc, INPUT_OK);
        assert_eq!(out, bits, "keyboard bits roundtrip exact");
    }

    #[test]
    fn keyboard_state_buffer_too_small_rejected() {
        let _g = lock_and_reset();
        grant_all();
        let mut tiny = [0u8; 8];
        let rc = unsafe { __cssl_input_keyboard_state(0, tiny.as_mut_ptr(), tiny.len()) };
        assert_eq!(rc, INPUT_ERR_BUFFER_TOO_SMALL);
        assert_eq!(last_input_error(), INPUT_ERR_BUFFER_TOO_SMALL);
    }

    #[test]
    fn keyboard_state_null_out_rejected() {
        let _g = lock_and_reset();
        grant_all();
        let rc = unsafe { __cssl_input_keyboard_state(0, core::ptr::null_mut(), 32) };
        assert_eq!(rc, INPUT_ERR_NULL_OUT);
    }

    #[test]
    fn keyboard_state_unknown_handle_zero_filled() {
        let _g = lock_and_reset();
        grant_all();
        // Never set state for handle 0xDEAD : reads back as zeros.
        let mut out = [0xFFu8; KEYBOARD_STATE_BYTES];
        let rc = unsafe { __cssl_input_keyboard_state(0xDEAD, out.as_mut_ptr(), out.len()) };
        assert_eq!(rc, INPUT_OK);
        assert_eq!(out, [0u8; KEYBOARD_STATE_BYTES]);
    }

    // ── mouse-state-roundtrip ──────────────────────────────────────────

    #[test]
    fn mouse_state_roundtrip_via_global() {
        let _g = lock_and_reset();
        grant_all();
        set_mouse_state(0, 100, 200, 0b101);
        let mut x = 0i32;
        let mut y = 0i32;
        let mut btns = 0u32;
        let rc = unsafe { __cssl_input_mouse_state(0, &mut x, &mut y, &mut btns) };
        assert_eq!(rc, INPUT_OK);
        assert_eq!(x, 100);
        assert_eq!(y, 200);
        assert_eq!(btns, 0b101);
    }

    #[test]
    fn mouse_state_roundtrip_via_handle() {
        let _g = lock_and_reset();
        grant_all();
        let h = 0x42u64;
        set_mouse_state(h, -50, 75, 0b10);
        let mut x = 0i32;
        let mut y = 0i32;
        let mut btns = 0u32;
        let rc = unsafe { __cssl_input_mouse_state(h, &mut x, &mut y, &mut btns) };
        assert_eq!(rc, INPUT_OK);
        assert_eq!(x, -50);
        assert_eq!(y, 75);
        assert_eq!(btns, 0b10);
    }

    #[test]
    fn mouse_state_null_out_rejected() {
        let _g = lock_and_reset();
        grant_all();
        let mut x = 0i32;
        let mut y = 0i32;
        let rc = unsafe { __cssl_input_mouse_state(0, &mut x, &mut y, core::ptr::null_mut()) };
        assert_eq!(rc, INPUT_ERR_NULL_OUT);
    }

    // ── mouse-delta-zero-on-no-movement ────────────────────────────────

    #[test]
    fn mouse_delta_zero_on_no_movement() {
        let _g = lock_and_reset();
        grant_all();
        let mut dx = 99i32;
        let mut dy = 99i32;
        let rc = unsafe { __cssl_input_mouse_delta(0, &mut dx, &mut dy) };
        assert_eq!(rc, INPUT_OK);
        assert_eq!(dx, 0, "no movement injected → dx = 0");
        assert_eq!(dy, 0, "no movement injected → dy = 0");
    }

    #[test]
    fn mouse_delta_accumulates_then_drains() {
        let _g = lock_and_reset();
        grant_all();
        // Two movements : (0,0) → (5,3) → (10,8). Net delta from both
        // events = (10, 8) ; drain reads (10, 8) ; second drain reads (0, 0).
        set_mouse_state(0, 5, 3, 0);
        set_mouse_state(0, 10, 8, 0);
        let mut dx = 0i32;
        let mut dy = 0i32;
        let rc = unsafe { __cssl_input_mouse_delta(0, &mut dx, &mut dy) };
        assert_eq!(rc, INPUT_OK);
        assert_eq!(dx, 10, "accumulated delta x = 5 + 5 = 10");
        assert_eq!(dy, 8, "accumulated delta y = 3 + 5 = 8");
        // Second drain : pending was zeroed by first drain.
        let rc = unsafe { __cssl_input_mouse_delta(0, &mut dx, &mut dy) };
        assert_eq!(rc, INPUT_OK);
        assert_eq!(dx, 0, "drain consumed prior delta");
        assert_eq!(dy, 0);
    }

    #[test]
    fn mouse_delta_per_handle_isolated() {
        let _g = lock_and_reset();
        grant_all();
        let h1 = 0x111u64;
        let h2 = 0x222u64;
        // Inject independent movement on each.
        set_mouse_state(h1, 1, 2, 0);
        set_mouse_state(h2, 100, 200, 0);
        let mut dx1 = 0i32;
        let mut dy1 = 0i32;
        let mut dx2 = 0i32;
        let mut dy2 = 0i32;
        unsafe { __cssl_input_mouse_delta(h1, &mut dx1, &mut dy1) };
        unsafe { __cssl_input_mouse_delta(h2, &mut dx2, &mut dy2) };
        assert_eq!((dx1, dy1), (1, 2));
        assert_eq!((dx2, dy2), (100, 200));
    }

    #[test]
    fn mouse_delta_null_out_rejected() {
        let _g = lock_and_reset();
        grant_all();
        let rc = unsafe { __cssl_input_mouse_delta(0, core::ptr::null_mut(), core::ptr::null_mut()) };
        assert_eq!(rc, INPUT_ERR_NULL_OUT);
    }

    // ── gamepad-disconnected-handling ──────────────────────────────────

    #[test]
    fn gamepad_disconnected_returns_disconnected_err_and_zero_bytes() {
        let _g = lock_and_reset();
        grant_all();
        // Slot 0 is disconnected by default.
        let mut out = [0xAAu8; GAMEPAD_STATE_BYTES];
        let rc = unsafe { __cssl_input_gamepad_state(0, out.as_mut_ptr(), out.len()) };
        assert_eq!(rc, INPUT_ERR_DISCONNECTED);
        assert_eq!(out, [0u8; GAMEPAD_STATE_BYTES], "disconnected → zero buffer");
    }

    #[test]
    fn gamepad_connected_roundtrip_preserves_bytes() {
        let _g = lock_and_reset();
        grant_all();
        let mut payload = [0u8; GAMEPAD_STATE_BYTES];
        // buttons = 0xABCD.
        payload[0] = 0xCD;
        payload[1] = 0xAB;
        // axes : LX = 0x4000.
        payload[4] = 0x00;
        payload[5] = 0x40;
        let rc = set_gamepad_state(2, &payload, true);
        assert_eq!(rc, INPUT_OK);
        let mut out = [0u8; GAMEPAD_STATE_BYTES];
        let rc = unsafe { __cssl_input_gamepad_state(2, out.as_mut_ptr(), out.len()) };
        assert_eq!(rc, INPUT_OK);
        // Confirm injected bytes preserved (flags bit-0 is forced on by injector).
        assert_eq!(out[0], 0xCD);
        assert_eq!(out[1], 0xAB);
        assert_eq!(out[4], 0x00);
        assert_eq!(out[5], 0x40);
        assert_eq!(out[2] & 1, 1, "flags bit-0 = connected");
    }

    #[test]
    fn gamepad_out_of_range_rejected() {
        let _g = lock_and_reset();
        grant_all();
        let mut out = [0u8; GAMEPAD_STATE_BYTES];
        let rc = unsafe { __cssl_input_gamepad_state(99, out.as_mut_ptr(), out.len()) };
        assert_eq!(rc, INPUT_ERR_INVALID_INDEX);
    }

    #[test]
    fn gamepad_buffer_too_small_rejected() {
        let _g = lock_and_reset();
        grant_all();
        let mut tiny = [0u8; 8];
        let rc = unsafe { __cssl_input_gamepad_state(0, tiny.as_mut_ptr(), tiny.len()) };
        assert_eq!(rc, INPUT_ERR_BUFFER_TOO_SMALL);
    }

    // ── caps + audit counters ─────────────────────────────────────────

    #[test]
    fn no_cap_granted_means_cap_denied() {
        let _g = lock_and_reset();
        // No caps_grant call.
        let mut out = [0u8; KEYBOARD_STATE_BYTES];
        let rc = unsafe { __cssl_input_keyboard_state(0, out.as_mut_ptr(), out.len()) };
        assert_eq!(rc, INPUT_ERR_CAP_DENIED);
        assert_eq!(input_error_count(), 1);
        assert_eq!(last_input_error(), INPUT_ERR_CAP_DENIED);
    }

    #[test]
    fn caps_grant_revoke_cycle() {
        let _g = lock_and_reset();
        assert_eq!(caps_current(), INPUT_CAP_NONE);
        let new = caps_grant(INPUT_CAP_KEYBOARD);
        assert_eq!(new & INPUT_CAP_KEYBOARD, INPUT_CAP_KEYBOARD);
        assert!(caps_satisfied(INPUT_CAP_KEYBOARD));
        assert!(!caps_satisfied(INPUT_CAP_MOUSE));
        let after = caps_revoke(INPUT_CAP_KEYBOARD);
        assert_eq!(after & INPUT_CAP_KEYBOARD, 0);
        assert!(!caps_satisfied(INPUT_CAP_KEYBOARD));
    }

    #[test]
    fn audit_counters_increment_on_success() {
        let _g = lock_and_reset();
        grant_all();
        // Pre-populate state for a clean read.
        let bits = [0u8; KEYBOARD_STATE_BYTES];
        set_keyboard_state(0, &bits);
        set_mouse_state(0, 0, 0, 0);
        set_gamepad_state(1, &[0u8; GAMEPAD_STATE_BYTES], true);
        let mut buf = [0u8; KEYBOARD_STATE_BYTES];
        let mut x = 0i32;
        let mut y = 0i32;
        let mut btns = 0u32;
        let mut dx = 0i32;
        let mut dy = 0i32;
        let mut gp = [0u8; GAMEPAD_STATE_BYTES];
        unsafe {
            assert_eq!(__cssl_input_keyboard_state(0, buf.as_mut_ptr(), buf.len()), INPUT_OK);
            assert_eq!(__cssl_input_mouse_state(0, &mut x, &mut y, &mut btns), INPUT_OK);
            assert_eq!(__cssl_input_mouse_delta(0, &mut dx, &mut dy), INPUT_OK);
            assert_eq!(__cssl_input_gamepad_state(1, gp.as_mut_ptr(), gp.len()), INPUT_OK);
        }
        assert_eq!(keyboard_read_count(), 1);
        assert_eq!(mouse_read_count(), 1);
        assert_eq!(mouse_delta_read_count(), 1);
        assert_eq!(gamepad_read_count(), 1);
        assert_eq!(input_error_count(), 0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § INTEGRATION_NOTE  (Wave-D4 / S5 ↳ § 24 HOST_FFI ↳ host_input)
// ═══════════════════════════════════════════════════════════════════════
//
// This module is delivered as a NEW file. Per the Wave-D4 dispatch
// constraint "DO NOT modify any lib.rs / Cargo.toml ; add INTEGRATION_NOTE
// at file ends" the following wiring is INTENTIONALLY NOT applied here ;
// the next slice in the W4-fanout (Wave-D5 wiring) MUST :
//
//   1. In `compiler-rs/crates/cssl-rt/src/lib.rs` (existing file) add :
//        ```
//        pub mod host_input;
//        ```
//      After the existing `pub mod net;` line (column-aligned with siblings).
//
//   2. In `compiler-rs/crates/cssl-rt/src/lib.rs` extend the public re-export
//      block with :
//        ```
//        pub use host_input::{
//            __cssl_input_keyboard_state, __cssl_input_mouse_state,
//            __cssl_input_mouse_delta, __cssl_input_gamepad_state,
//            keyboard_read_count, mouse_read_count, mouse_delta_read_count,
//            gamepad_read_count, input_error_count, last_input_error,
//            caps_current, caps_grant, caps_revoke, caps_satisfied,
//            INPUT_OK, INPUT_ERR_BUFFER_TOO_SMALL, INPUT_ERR_NULL_OUT,
//            INPUT_ERR_INVALID_HANDLE, INPUT_ERR_INVALID_INDEX,
//            INPUT_ERR_DISCONNECTED, INPUT_ERR_CAP_DENIED,
//            INPUT_CAP_KEYBOARD, INPUT_CAP_MOUSE, INPUT_CAP_MOUSE_DELTA,
//            INPUT_CAP_GAMEPAD, INPUT_CAP_MASK, INPUT_CAP_NONE,
//            KEYBOARD_STATE_BYTES, MOUSE_STATE_BYTES,
//            GAMEPAD_STATE_BYTES, GAMEPAD_SLOT_COUNT,
//        };
//        ```
//
//   3. In `compiler-rs/crates/cssl-rt/src/lib.rs::test_helpers::lock_and_reset_all`
//      add `crate::host_input::reset_for_tests();` so the existing
//      cross-module test serializer also resets input globals.
//
//   4. In `compiler-rs/crates/cssl-rt/Cargo.toml` IF the stage-1 wrap is
//      desired add a `cssl-host-input = { path = "../cssl-host-input" }`
//      dep so that the OS pump can call this module's `set_keyboard_state` /
//      `set_mouse_state` / `set_gamepad_state` setters from the per-OS
//      backend's event-loop. Until that stage-1 cargo edit lands, the
//      stage-0 surface here uses the in-process state-cache only.
//
// § PRIME-DIRECTIVE INVARIANT (W! lock-step with § 24 IFC + § 11 PRIME-DIRECTIVE)
//   The four FFI symbols' signatures are LOCKED at first commit. Any
//   future widening (e.g. a 5th out-pointer) MUST mint a NEW symbol
//   (e.g. `__cssl_input_mouse_state_v2`) per `specs/24_HOST_FFI.csl §
//   DESIGN-PRINCIPLES P1`. Renaming or reshape = link-time UB ⇒ debug-
//   stage CSSLv3 binaries would crash on first input read.
//
// § ATTESTATION  (PRIME_DIRECTIVE.md § 11 ; carried-forward landmine)
//   "There was no hurt nor harm in the making of this, to anyone /
//   anything / anybody." Every byte of input data stays in-process.
//   Mouse-delta is `Sensitive<Behavioral>` per § 24 IFC — counters
//   only, never delta-bytes. Keyboard-state is `Sensitive<Behavioral>`
//   for non-game-key events ; the FFI surface has no logging /
//   network / fs side-channel.
