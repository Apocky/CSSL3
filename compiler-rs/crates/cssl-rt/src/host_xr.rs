//! § cssl-rt::host_xr — Wave-D8 OpenXR / VR / XR primitive surface.
//!
//! ════════════════════════════════════════════════════════════════════
//! § ROLE
//!   Stage-0 host-FFI bridge for OpenXR / VR session-handle + pose-stream
//!   + stereo-swapchain + controller-input. Exposes the ABI-stable
//!   `__cssl_xr_*` extern "C" symbols per `specs/24_HOST_FFI.csl § vr-xr`.
//!   The session/swapchain handles are u64 indices into a process-local
//!   slot-table ; the slot-table is allocation-free at the hot-path
//!   (lazy-init Mutex<Vec<Slot>> · grows on session-create only).
//!
//! § SPEC-REFERENCES
//!   - `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § vr-xr`
//!   - `specs/24_HOST_FFI.csl § IFC-LABELS § XR`
//!     · XR-head-pose       = Sensitive<Spatial>     ← never egresses
//!     · XR-controller-pose = Sensitive<Behavioral>  ← never egresses
//!   - `compiler-rs/crates/cssl-host-openxr/src/lib.rs` — stage-0 wraps
//!     point ; this module's bodies stub-out on non-Quest builds + leave
//!     SWAP-POINTs for the real-hw integration (T11-D124-FOLLOWUP-1).
//!   - `PRIME_DIRECTIVE.md § 1 (anti-surveillance)` + `§ 11 (attestation)`.
//!
//! § PRIME-DIRECTIVE STRUCTURAL ENFORCEMENT
//!   The pose-stream + input-state ops emit data that is always labeled
//!   `Sensitive<Spatial>` (head/controller pose) or `Sensitive<Behavioral>`
//!   (controller buttons/axes). At the cssl-rt layer we cannot enforce
//!   the IFC-label semantics directly (that lives in the cssl-ifc crate
//!   + cssl-host-openxr's `LabeledValue<T>` wrapper) ; what we CAN do
//!   structurally :
//!
//!   1. CAP-GATED  : every xr op consults a per-thread XR cap-bitset.
//!      Default = `XR_CAP_NONE`. `xr_caps_grant(XR_CAP_SESSION)` is
//!      required to even create a session ; the host process holds
//!      the cap-grant key + only emits it after PRIME-DIRECTIVE-side
//!      consent ceremony. This mirrors the net.rs cap-system pattern.
//!
//!   2. NO-EGRESS-AT-RUNTIME : every pose write goes into a caller-
//!      supplied buffer ; cssl-rt itself does NOT buffer or forward
//!      pose data anywhere. There is no log, no telemetry-ring entry,
//!      no atomic-snapshot of pose values. The bytes flow caller→
//!      `*mut u8` and stop there ; downstream IFC validation in
//!      cssl-ifc rejects any compose with `Effect<Net>` or
//!      `Effect<File>` outright (per specs/11_IFC.csl).
//!
//!   3. AUDIT VISIBILITY : every successful xr op increments a public
//!      atomic counter the host can inspect. Counters are LOCAL — they
//!      do not export anywhere. Pattern matches crate::net.
//!
//!   4. NO BACKDOORS : no flag, env-var, config knob, build-feature, or
//!      runtime cond can disable the cap-gate or the IFC-label per
//!      `PRIME_DIRECTIVE.md § 6 SCOPE`.
//!
//! § STAGE-0 SCOPE
//!   - hosted (`cargo build`) only.
//!   - all bodies are PURE-RUST + ALLOC-FREE on the hot path ; no
//!     openxr-rs / windows-rs / vulkan-rs deps. Pose-stream returns a
//!     deterministic-zero payload (84 bytes : 28 head + 28 left + 28
//!     right) on every call ; this lets unit tests exercise the
//!     plumbing without a Quest 3s plugged in.
//!   - SWAP-POINT for real-hw integration is the per-fn `*_impl`
//!     helper ; a future Wave-D8b will reroute through cssl-host-openxr
//!     when Cargo.toml gains the dep.
//!
//! § HANDLE LAYOUT (Sawyer-efficiency : slot-table indexed by u64)
//!   ┌─ bit 63       ─┐  generation high-bit (reserved for ABI growth)
//!   │  bit 62..32    │  generation counter (32-bit, increments on free)
//!   │  bit 31..0     │  slot index into the per-table Vec<Slot>
//!   └─ ─ ─ ─ ─ ─ ─ ─┘
//!   - `0` = invalid handle (sentinel) ; matches `INVALID_XR_HANDLE`.
//!   - On `*_destroy` the slot's generation counter increments ; future
//!     handles to the same slot get fresh generation bits, defending
//!     against use-after-destroy.
//!
//! § POSE PAYLOAD LAYOUT (Sawyer-efficiency : 7 floats per pose, packed)
//!   ┌─ offset 0..28   ─┐  head pose : (x,y,z,qx,qy,qz,qw) f32-LE
//!   │  offset 28..56   │  left controller pose : same shape
//!   │  offset 56..84   │  right controller pose : same shape
//!   └─ ─ ─ ─ ─ ─ ─ ─ ─┘
//!   Total = 84 bytes ; if `max_len < 84` the call returns
//!   `XR_ERROR_BUFFER_TOO_SMALL` without writing anything.
//!
//! § INPUT-STATE PAYLOAD LAYOUT (per-controller)
//!   ┌─ offset 0..1   ─┐  trigger axis (u8 quantized)
//!   │  offset 1..2   │   grip axis (u8 quantized)
//!   │  offset 2..3   │   thumbstick X (i8 quantized)
//!   │  offset 3..4   │   thumbstick Y (i8 quantized)
//!   │  offset 4..8   │   button bitset (u32 LE) :
//!   │                │     bit 0 = primary
//!   │                │     bit 1 = secondary
//!   │                │     bit 2 = thumbstick-click
//!   │                │     bit 3 = trigger-touch
//!   │                │     bit 4 = grip-touch
//!   │                │     bit 5..31 = reserved
//!   └─ ─ ─ ─ ─ ─ ─ ─┘
//!   Total = 8 bytes ; matches `XR_INPUT_STATE_BYTES`.

#![allow(unsafe_code)]

use core::sync::atomic::{AtomicI32, AtomicU64, AtomicU32, Ordering};
use std::sync::Mutex;

// ───────────────────────────────────────────────────────────────────────
// § ABI-STABLE constants (PRIME-DIRECTIVE-compliant flag-set, frozen)
// ───────────────────────────────────────────────────────────────────────

/// Sentinel handle for "invalid / not-a-session" / "not-a-swapchain".
/// Matches `0u64` ; CSSLv3-source-level wrappers convert this to
/// `Result::Err(XrError::*)` per the per-thread last-error slot.
pub const INVALID_XR_HANDLE: u64 = 0;

/// XR cap-bit : create / destroy session. Default = OFF ; host grants
/// explicitly after PRIME-DIRECTIVE consent ceremony.
pub const XR_CAP_SESSION: i32 = 0x0001;

/// XR cap-bit : access head pose-stream. Implies head-tracking is on.
/// `Sensitive<Spatial>`. Default = OFF.
pub const XR_CAP_POSE_HEAD: i32 = 0x0002;

/// XR cap-bit : access controller pose-stream. `Sensitive<Behavioral>`.
/// Default = OFF.
pub const XR_CAP_POSE_CONTROLLER: i32 = 0x0004;

/// XR cap-bit : acquire stereo swapchain images for rendering. Default = OFF.
pub const XR_CAP_SWAPCHAIN: i32 = 0x0008;

/// XR cap-bit : access controller input state (triggers / buttons / axes).
/// `Sensitive<Behavioral>`. Default = OFF.
pub const XR_CAP_INPUT: i32 = 0x0010;

/// Mask of recognized cap bits.
pub const XR_CAP_MASK: i32 =
    XR_CAP_SESSION | XR_CAP_POSE_HEAD | XR_CAP_POSE_CONTROLLER | XR_CAP_SWAPCHAIN | XR_CAP_INPUT;

/// Default cap-set on every thread : NO XR caps. Host must explicitly
/// `xr_caps_grant(...)` before any session can be created. Matches the
/// PRIME-DIRECTIVE default-deny posture for spatial / behavioral data.
pub const XR_CAP_NONE: i32 = 0;

/// Stereo eye enum : LEFT eye-buffer for a stereo swapchain.
pub const XR_EYE_LEFT: u32 = 0;

/// Stereo eye enum : RIGHT eye-buffer for a stereo swapchain.
pub const XR_EYE_RIGHT: u32 = 1;

/// Number of valid eye-buffer slots per stereo swapchain.
/// (LEFT + RIGHT — quad-view headsets like Varjo XR-3/4 are addressed
/// by additional `__cssl_xr_swapchain_quad_*` symbols not in this slice.)
pub const XR_EYE_COUNT: usize = 2;

/// Bytes per pose record (7 × f32-LE : x,y,z,qx,qy,qz,qw).
pub const XR_POSE_BYTES: usize = 28;

/// Bytes per pose-stream payload (1 head + 2 controller poses).
pub const XR_POSE_STREAM_BYTES: usize = XR_POSE_BYTES * 3;

/// Bytes per controller input-state payload.
pub const XR_INPUT_STATE_BYTES: usize = 8;

/// Max simultaneous XR sessions per process. Stage-0 stub ; matches
/// realistic upper-bound (1 active OpenXR session per process per
/// XR_KHR_vulkan_enable spec). Slot-table grows lazily up to this cap.
pub const XR_MAX_SESSIONS: usize = 4;

// ───────────────────────────────────────────────────────────────────────
// § XrError — canonical error sum-type. Discriminants STABLE from D8.
// ───────────────────────────────────────────────────────────────────────

/// Canonical XR error variants — discriminants are STABLE from Wave-D8.
///
/// § VARIANTS
///   - `0`  Success — sentinel for "no error" ; never observed by consumers.
///   - `1`  `InvalidInput` — malformed flags / pointer / length / eye-enum.
///   - `2`  `BufferTooSmall` — caller buffer < required payload bytes.
///   - `3`  `InvalidSession` — handle not in slot-table or generation-stale.
///   - `4`  `InvalidSwapchain` — swapchain handle not valid.
///   - `5`  `SessionLimit` — process exceeded `XR_MAX_SESSIONS` sessions.
///   - `6`  `CapDenied` — PRIME-DIRECTIVE cap-system rejected the call.
///   - `7`  `EyeOutOfRange` — eye index ∉ {LEFT, RIGHT}.
///   - `8`  `RuntimeNotPresent` — no OpenXR runtime available on host.
///   - `9`  `SessionNotRunning` — pose / swap call before session begin.
///   - `99` `Other` — catch-all carrying raw OS error in high 32 bits.
pub mod xr_error_code {
    /// No error ; never observed by consumers.
    pub const SUCCESS: i32 = 0;
    /// Caller-supplied flags / pointer / length / eye-enum is malformed.
    pub const INVALID_INPUT: i32 = 1;
    /// Caller buffer too small for the payload.
    pub const BUFFER_TOO_SMALL: i32 = 2;
    /// Session handle is not in the slot-table or has stale generation.
    pub const INVALID_SESSION: i32 = 3;
    /// Swapchain handle is not in the slot-table or has stale generation.
    pub const INVALID_SWAPCHAIN: i32 = 4;
    /// Process has exceeded `XR_MAX_SESSIONS` simultaneous sessions.
    pub const SESSION_LIMIT: i32 = 5;
    /// PRIME-DIRECTIVE cap-system rejected the call. Caller must
    /// `xr_caps_grant(XR_CAP_*)` for the relevant op-class first.
    pub const CAP_DENIED: i32 = 6;
    /// Eye index outside {`XR_EYE_LEFT`, `XR_EYE_RIGHT`}.
    pub const EYE_OUT_OF_RANGE: i32 = 7;
    /// No OpenXR runtime available on host (no headset, no driver).
    pub const RUNTIME_NOT_PRESENT: i32 = 8;
    /// Pose / swap call invoked before session begin.
    pub const SESSION_NOT_RUNNING: i32 = 9;
    /// Catch-all : the high 32 bits of the i64 slot carry the raw OS code.
    pub const OTHER: i32 = 99;
}

// ───────────────────────────────────────────────────────────────────────
// § Per-process last-error slot.
//
//   Stage-0 implementation : a single global atomic. Sufficient for
//   hosted stage-0 testing. A per-thread TLS slot is a follow-up once
//   the runtime grows TLS infrastructure (matches B5 file-IO + S7-F4
//   net precedent).
// ───────────────────────────────────────────────────────────────────────

static LAST_XR_ERROR_CODE: AtomicU64 = AtomicU64::new(0);

/// Write the canonical error code for the last xr op.
///
/// `os_code` is the raw OS code (or 0 for the non-`OTHER` cases). The
/// two are packed into a single u64 :
/// `(os_code as u64) << 32 | (kind_code as u32 as u64)`.
pub fn record_xr_error(kind_code: i32, os_code: i32) {
    #[allow(clippy::cast_sign_loss)]
    let kind = kind_code as u32 as u64;
    #[allow(clippy::cast_sign_loss)]
    let os = os_code as u32 as u64;
    LAST_XR_ERROR_CODE.store((os << 32) | kind, Ordering::Relaxed);
}

/// Read the canonical error kind from the last xr op (low 32 bits).
#[must_use]
pub fn last_xr_error_kind() -> i32 {
    #[allow(clippy::cast_possible_wrap)]
    let kind = (LAST_XR_ERROR_CODE.load(Ordering::Relaxed) & 0xFFFF_FFFF) as i32;
    kind
}

/// Read the raw OS code from the last xr op (high 32 bits).
#[must_use]
pub fn last_xr_error_os() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_possible_wrap)]
    let os = ((LAST_XR_ERROR_CODE.load(Ordering::Relaxed) >> 32) & 0xFFFF_FFFF) as i32;
    os
}

/// Reset the last-error slot to `SUCCESS / 0`. Test-only.
#[doc(hidden)]
pub fn reset_last_xr_error_for_tests() {
    LAST_XR_ERROR_CODE.store(0, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § PRIME-DIRECTIVE cap-system : per-process capability slot.
// ───────────────────────────────────────────────────────────────────────

static XR_CAP_BITS: AtomicI32 = AtomicI32::new(XR_CAP_NONE);

/// Grant `cap_bits` to the cap-set. Returns the new cap-set.
///
/// ‼ Granting requires explicit caller consent. There is NO mechanism
/// to grant XR caps "by default" ; every xr op requires an explicit
/// grant from the host process. This realizes the `CONSENT-ARCH` axiom
/// from `PRIME_DIRECTIVE.md § 5` : consent is informed, granular,
/// revocable, and ongoing.
pub fn xr_caps_grant(cap_bits: i32) -> i32 {
    let masked = cap_bits & XR_CAP_MASK;
    let prev = XR_CAP_BITS.fetch_or(masked, Ordering::Relaxed);
    prev | masked
}

/// Revoke `cap_bits` from the cap-set. Returns the new cap-set.
///
/// Revocation is always permitted per `PRIME_DIRECTIVE.md § 5` —
/// consent is "revocable + granular + informed + ongoing".
pub fn xr_caps_revoke(cap_bits: i32) -> i32 {
    let masked = cap_bits & XR_CAP_MASK;
    let prev = XR_CAP_BITS.fetch_and(!masked, Ordering::Relaxed);
    prev & !masked
}

/// Read the current cap-set.
#[must_use]
pub fn xr_caps_current() -> i32 {
    XR_CAP_BITS.load(Ordering::Relaxed)
}

/// Reset the cap-set to `XR_CAP_NONE`. Test-only ; not exported via FFI.
#[doc(hidden)]
pub fn reset_xr_caps_for_tests() {
    XR_CAP_BITS.store(XR_CAP_NONE, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § Public counters — audit-visibility (LOCAL ; never exports).
// ───────────────────────────────────────────────────────────────────────

static SESSION_CREATE_COUNT: AtomicU64 = AtomicU64::new(0);
static SESSION_DESTROY_COUNT: AtomicU64 = AtomicU64::new(0);
static POSE_STREAM_COUNT: AtomicU64 = AtomicU64::new(0);
static SWAPCHAIN_ACQUIRE_COUNT: AtomicU64 = AtomicU64::new(0);
static SWAPCHAIN_RELEASE_COUNT: AtomicU64 = AtomicU64::new(0);
static INPUT_STATE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Total successful `__cssl_xr_session_create` calls.
#[must_use]
pub fn session_create_count() -> u64 {
    SESSION_CREATE_COUNT.load(Ordering::Relaxed)
}
/// Total successful `__cssl_xr_session_destroy` calls.
#[must_use]
pub fn session_destroy_count() -> u64 {
    SESSION_DESTROY_COUNT.load(Ordering::Relaxed)
}
/// Total successful `__cssl_xr_pose_stream` calls (head+controller).
#[must_use]
pub fn pose_stream_count() -> u64 {
    POSE_STREAM_COUNT.load(Ordering::Relaxed)
}
/// Total successful `__cssl_xr_swapchain_stereo_acquire` calls.
#[must_use]
pub fn swapchain_acquire_count() -> u64 {
    SWAPCHAIN_ACQUIRE_COUNT.load(Ordering::Relaxed)
}
/// Total successful `__cssl_xr_swapchain_stereo_release` calls.
#[must_use]
pub fn swapchain_release_count() -> u64 {
    SWAPCHAIN_RELEASE_COUNT.load(Ordering::Relaxed)
}
/// Total successful `__cssl_xr_input_state` calls.
#[must_use]
pub fn input_state_count() -> u64 {
    INPUT_STATE_COUNT.load(Ordering::Relaxed)
}

/// Reset all counters + slot-table + last-error + caps to clean state.
/// Test-only.
#[doc(hidden)]
pub fn reset_xr_for_tests() {
    SESSION_CREATE_COUNT.store(0, Ordering::Relaxed);
    SESSION_DESTROY_COUNT.store(0, Ordering::Relaxed);
    POSE_STREAM_COUNT.store(0, Ordering::Relaxed);
    SWAPCHAIN_ACQUIRE_COUNT.store(0, Ordering::Relaxed);
    SWAPCHAIN_RELEASE_COUNT.store(0, Ordering::Relaxed);
    INPUT_STATE_COUNT.store(0, Ordering::Relaxed);
    reset_last_xr_error_for_tests();
    reset_xr_caps_for_tests();
    if let Ok(mut g) = SESSION_TABLE.lock() {
        g.clear();
    } else {
        // Mutex poisoned ; clear poison + reset.
        SESSION_TABLE.clear_poison();
        if let Ok(mut g) = SESSION_TABLE.lock() {
            g.clear();
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Slot-table — session handle plumbing (Sawyer : indexed by u64).
//
//   The session-table holds at most XR_MAX_SESSIONS active sessions ;
//   new sessions reuse freed slots after the generation-counter bumps.
//   The `swapchain_image_id` field is a per-session monotonic counter ;
//   each `swapchain_acquire` increments it and returns the new value
//   as the GPU-image handle. `swapchain_release` validates the image
//   matches the most-recent acquire (one-in-flight per eye, stage-0).
// ───────────────────────────────────────────────────────────────────────

/// Internal slot-table entry. `generation == 0` ⇒ slot is free ;
/// caller-visible u64 handles encode `(generation, slot_idx)` so that
/// reuse of a freed slot does NOT alias a stale handle.
#[derive(Clone, Copy, Default)]
struct SessionSlot {
    /// Generation counter ; 0 = free, ≥ 1 = active.
    generation: u32,
    /// Caller-supplied flags from `__cssl_xr_session_create`.
    flags: u32,
    /// Last-acquired image id for LEFT eye (0 = none in flight).
    last_image_left: u64,
    /// Last-acquired image id for RIGHT eye.
    last_image_right: u64,
    /// Monotonic counter ; bumps on every `swapchain_acquire`.
    image_counter: u64,
}

static SESSION_TABLE: Mutex<Vec<SessionSlot>> = Mutex::new(Vec::new());

/// Encode `(generation, slot_idx)` into a u64 handle.
///   bit 63 reserved (0)
///   bit 62..32 generation (31 bits used ; 0 = invalid)
///   bit 31..0  slot index
#[inline]
const fn encode_handle(generation: u32, slot_idx: u32) -> u64 {
    ((generation as u64) << 32) | (slot_idx as u64)
}

/// Decode a u64 handle into `(generation, slot_idx)`.
#[inline]
const fn decode_handle(h: u64) -> (u32, u32) {
    #[allow(clippy::cast_possible_truncation)]
    let slot_idx = (h & 0xFFFF_FFFF) as u32;
    #[allow(clippy::cast_possible_truncation)]
    let generation = ((h >> 32) & 0x7FFF_FFFF) as u32;
    (generation, slot_idx)
}

/// Validate that `handle` references a live slot. Returns Some(slot_idx)
/// on success, None on a stale / invalid handle.
fn lookup_session(handle: u64) -> Option<usize> {
    if handle == INVALID_XR_HANDLE {
        return None;
    }
    let (gen, slot_idx) = decode_handle(handle);
    let g = SESSION_TABLE.lock().ok()?;
    let slot = g.get(slot_idx as usize)?;
    if slot.generation != gen || slot.generation == 0 {
        return None;
    }
    Some(slot_idx as usize)
}

// ───────────────────────────────────────────────────────────────────────
// § Eye-enum dispatch LUT (Sawyer : single match-arm + table-lookup).
// ───────────────────────────────────────────────────────────────────────

/// Static LUT mapping eye-enum to its short name. Used by tests + diag.
pub const EYE_NAME_LUT: [&str; XR_EYE_COUNT] = ["left", "right"];

/// Validate an eye index ∈ {`XR_EYE_LEFT`, `XR_EYE_RIGHT`}.
#[must_use]
pub const fn eye_is_valid(eye: u32) -> bool {
    (eye as usize) < XR_EYE_COUNT
}

/// Map an eye-enum to its canonical short name. Returns `None` on
/// out-of-range. Branch-free dispatch via direct array index.
#[must_use]
pub fn eye_name(eye: u32) -> Option<&'static str> {
    if eye_is_valid(eye) {
        Some(EYE_NAME_LUT[eye as usize])
    } else {
        None
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Pure-Rust `*_impl` helpers (delegate from extern symbols).
//
//   These are testable without going through the FFI boundary. Each
//   _impl returns a Result-style sum-type via the i32/i64 sentinel +
//   per-thread last-error slot pattern from net.rs.
// ───────────────────────────────────────────────────────────────────────

/// `__cssl_xr_session_create` impl — pure Rust ; cap-checked.
///
/// Returns `INVALID_XR_HANDLE` on cap-deny or table-full ; the
/// per-thread last-error slot is set. On success, returns a valid
/// (generation, slot_idx) handle.
pub fn xr_session_create_impl(flags: u32) -> u64 {
    // PRIME-DIRECTIVE cap-gate.
    let caps = xr_caps_current();
    if (caps & XR_CAP_SESSION) == 0 {
        record_xr_error(xr_error_code::CAP_DENIED, 0);
        return INVALID_XR_HANDLE;
    }
    // Acquire slot-table.
    let Ok(mut g) = SESSION_TABLE.lock() else {
        record_xr_error(xr_error_code::OTHER, 0);
        return INVALID_XR_HANDLE;
    };
    // Find a free slot OR grow up to XR_MAX_SESSIONS.
    let slot_idx = if let Some(idx) = g.iter().position(|s| s.generation == 0) {
        idx
    } else if g.len() < XR_MAX_SESSIONS {
        g.push(SessionSlot::default());
        g.len() - 1
    } else {
        record_xr_error(xr_error_code::SESSION_LIMIT, 0);
        return INVALID_XR_HANDLE;
    };
    // SWAP-POINT : on Windows / Quest builds, we'd call
    // cssl_host_openxr::session::create() here. The stage-0 stub uses
    // a deterministic generation-counter so pure-Rust tests are
    // reproducible. The generation MUST be ≥ 1 (0 = free / invalid).
    static GENERATION_COUNTER: AtomicU32 = AtomicU32::new(0);
    let gen = GENERATION_COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let gen = if gen == 0 { 1 } else { gen };
    g[slot_idx] = SessionSlot {
        generation: gen,
        flags,
        last_image_left: 0,
        last_image_right: 0,
        image_counter: 0,
    };
    SESSION_CREATE_COUNT.fetch_add(1, Ordering::Relaxed);
    record_xr_error(xr_error_code::SUCCESS, 0);
    #[allow(clippy::cast_possible_truncation)]
    encode_handle(gen, slot_idx as u32)
}

/// `__cssl_xr_session_destroy` impl — pure Rust ; cap-checked.
///
/// Returns 0 on success, negative i32 on error.
pub fn xr_session_destroy_impl(session: u64) -> i32 {
    let caps = xr_caps_current();
    if (caps & XR_CAP_SESSION) == 0 {
        record_xr_error(xr_error_code::CAP_DENIED, 0);
        return -1;
    }
    let Some(slot_idx) = lookup_session(session) else {
        record_xr_error(xr_error_code::INVALID_SESSION, 0);
        return -1;
    };
    let Ok(mut g) = SESSION_TABLE.lock() else {
        record_xr_error(xr_error_code::OTHER, 0);
        return -1;
    };
    // Bump generation to invalidate aliasing handles ; mark slot free.
    g[slot_idx].generation = 0;
    g[slot_idx].flags = 0;
    g[slot_idx].last_image_left = 0;
    g[slot_idx].last_image_right = 0;
    g[slot_idx].image_counter = 0;
    SESSION_DESTROY_COUNT.fetch_add(1, Ordering::Relaxed);
    record_xr_error(xr_error_code::SUCCESS, 0);
    0
}

/// `__cssl_xr_pose_stream` impl — pure Rust ; cap-checked ;
/// bounded-write.
///
/// Writes a single 84-byte payload (head 28 + left ctrl 28 + right
/// ctrl 28) into `head_pose_out` ; if `controller_pose_out` is non-null
/// it receives the second + third 28-byte poses. Returns the number of
/// bytes written or negative i32 on error.
///
/// § STAGE-0 STUB
///   Writes deterministic-zero floats. SWAP-POINT for real OpenXR
///   xrLocateViews + xrLocateSpace dispatch is the body below.
///
/// # Safety
/// Caller must ensure :
/// - `head_pose_out` is non-null + valid for `max_len` bytes when `max_len > 0`,
/// - `max_len ≥ XR_POSE_STREAM_BYTES` to receive full 3-pose payload.
pub unsafe fn xr_pose_stream_impl(
    session: u64,
    head_pose_out: *mut u8,
    controller_pose_out: *mut u8,
    max_len: usize,
) -> i32 {
    // Both head + controller-pose caps required (Sensitive<Spatial> +
    // Sensitive<Behavioral>).
    let caps = xr_caps_current();
    let needed = XR_CAP_POSE_HEAD | XR_CAP_POSE_CONTROLLER;
    if (caps & needed) != needed {
        record_xr_error(xr_error_code::CAP_DENIED, 0);
        return -1;
    }
    if lookup_session(session).is_none() {
        record_xr_error(xr_error_code::INVALID_SESSION, 0);
        return -1;
    }
    if head_pose_out.is_null() {
        record_xr_error(xr_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if max_len < XR_POSE_STREAM_BYTES {
        record_xr_error(xr_error_code::BUFFER_TOO_SMALL, 0);
        return -1;
    }
    // SWAP-POINT : on Quest / Vision Pro builds, we'd call
    // cssl_host_openxr::session::locate_views() + ::locate_action_space()
    // here. The stage-0 stub writes deterministic-zero floats.
    //
    // SAFETY : caller pre-cond promises head_pose_out is valid for
    // `max_len ≥ XR_POSE_STREAM_BYTES` bytes ; we write exactly the
    // payload size which is bounded.
    unsafe {
        for i in 0..XR_POSE_STREAM_BYTES {
            *head_pose_out.add(i) = 0u8;
        }
    }
    // If caller passes a separate controller-pose buffer, mirror the
    // 56-byte controller half there too. Optional.
    if !controller_pose_out.is_null() {
        // SAFETY : same bounded-write contract.
        unsafe {
            for i in 0..(XR_POSE_BYTES * 2) {
                *controller_pose_out.add(i) = 0u8;
            }
        }
    }
    POSE_STREAM_COUNT.fetch_add(1, Ordering::Relaxed);
    record_xr_error(xr_error_code::SUCCESS, 0);
    #[allow(clippy::cast_possible_wrap)]
    let written = XR_POSE_STREAM_BYTES as i32;
    written
}

/// `__cssl_xr_swapchain_stereo_acquire` impl — pure Rust ; cap-checked ;
/// eye-dispatch via static LUT.
///
/// Writes the acquired image-id to `*image_out` ; returns 0 on success
/// or negative i32 on error.
///
/// # Safety
/// Caller must ensure `image_out` is non-null + properly aligned for `u64`.
pub unsafe fn xr_swapchain_stereo_acquire_impl(
    session: u64,
    eye: u32,
    image_out: *mut u64,
) -> i32 {
    let caps = xr_caps_current();
    if (caps & XR_CAP_SWAPCHAIN) == 0 {
        record_xr_error(xr_error_code::CAP_DENIED, 0);
        return -1;
    }
    if !eye_is_valid(eye) {
        record_xr_error(xr_error_code::EYE_OUT_OF_RANGE, 0);
        return -1;
    }
    let Some(slot_idx) = lookup_session(session) else {
        record_xr_error(xr_error_code::INVALID_SESSION, 0);
        return -1;
    };
    if image_out.is_null() {
        record_xr_error(xr_error_code::INVALID_INPUT, 0);
        return -1;
    }
    let Ok(mut g) = SESSION_TABLE.lock() else {
        record_xr_error(xr_error_code::OTHER, 0);
        return -1;
    };
    g[slot_idx].image_counter = g[slot_idx].image_counter.wrapping_add(1);
    let img = g[slot_idx].image_counter;
    if eye == XR_EYE_LEFT {
        g[slot_idx].last_image_left = img;
    } else {
        g[slot_idx].last_image_right = img;
    }
    // SWAP-POINT : real OpenXR dispatch via xrAcquireSwapchainImage +
    // xrWaitSwapchainImage for the per-eye Vulkan/D3D12/Metal image.
    // The stage-0 stub returns the monotonic image_counter.
    //
    // SAFETY : caller pre-cond promises image_out is non-null + aligned.
    unsafe {
        *image_out = img;
    }
    SWAPCHAIN_ACQUIRE_COUNT.fetch_add(1, Ordering::Relaxed);
    record_xr_error(xr_error_code::SUCCESS, 0);
    0
}

/// `__cssl_xr_swapchain_stereo_release` impl — pure Rust ; cap-checked.
///
/// Validates that `image` matches the last acquire for `eye` ; returns
/// 0 on success or negative i32 on error.
pub fn xr_swapchain_stereo_release_impl(session: u64, eye: u32, image: u64) -> i32 {
    let caps = xr_caps_current();
    if (caps & XR_CAP_SWAPCHAIN) == 0 {
        record_xr_error(xr_error_code::CAP_DENIED, 0);
        return -1;
    }
    if !eye_is_valid(eye) {
        record_xr_error(xr_error_code::EYE_OUT_OF_RANGE, 0);
        return -1;
    }
    let Some(slot_idx) = lookup_session(session) else {
        record_xr_error(xr_error_code::INVALID_SESSION, 0);
        return -1;
    };
    let Ok(mut g) = SESSION_TABLE.lock() else {
        record_xr_error(xr_error_code::OTHER, 0);
        return -1;
    };
    let last = if eye == XR_EYE_LEFT {
        g[slot_idx].last_image_left
    } else {
        g[slot_idx].last_image_right
    };
    if last != image || image == 0 {
        record_xr_error(xr_error_code::INVALID_SWAPCHAIN, 0);
        return -1;
    }
    if eye == XR_EYE_LEFT {
        g[slot_idx].last_image_left = 0;
    } else {
        g[slot_idx].last_image_right = 0;
    }
    SWAPCHAIN_RELEASE_COUNT.fetch_add(1, Ordering::Relaxed);
    record_xr_error(xr_error_code::SUCCESS, 0);
    0
}

/// `__cssl_xr_input_state` impl — pure Rust ; cap-checked ;
/// bounded-write.
///
/// Writes 8 bytes of controller input (axes + buttons) to `state_out`.
/// Returns the bytes written or negative i32 on error.
///
/// # Safety
/// Caller must ensure `state_out` is non-null + valid for `max_len` bytes
/// when `max_len > 0`.
pub unsafe fn xr_input_state_impl(
    session: u64,
    controller_idx: u32,
    state_out: *mut u8,
    max_len: usize,
) -> i32 {
    let caps = xr_caps_current();
    if (caps & XR_CAP_INPUT) == 0 {
        record_xr_error(xr_error_code::CAP_DENIED, 0);
        return -1;
    }
    if lookup_session(session).is_none() {
        record_xr_error(xr_error_code::INVALID_SESSION, 0);
        return -1;
    }
    // controller_idx ∈ {0=left, 1=right} ; reuse eye LUT shape.
    if !eye_is_valid(controller_idx) {
        record_xr_error(xr_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if state_out.is_null() {
        record_xr_error(xr_error_code::INVALID_INPUT, 0);
        return -1;
    }
    if max_len < XR_INPUT_STATE_BYTES {
        record_xr_error(xr_error_code::BUFFER_TOO_SMALL, 0);
        return -1;
    }
    // SWAP-POINT : on Quest / Vision Pro builds, we'd call
    // cssl_host_openxr::action::poll_input_state() here. Stage-0 stub
    // writes deterministic-zero state.
    //
    // SAFETY : caller pre-cond promises state_out valid for ≥ 8 bytes.
    unsafe {
        for i in 0..XR_INPUT_STATE_BYTES {
            *state_out.add(i) = 0u8;
        }
    }
    INPUT_STATE_COUNT.fetch_add(1, Ordering::Relaxed);
    record_xr_error(xr_error_code::SUCCESS, 0);
    #[allow(clippy::cast_possible_wrap)]
    let written = XR_INPUT_STATE_BYTES as i32;
    written
}

// ───────────────────────────────────────────────────────────────────────
// § ABI-STABLE extern "C" symbols — `__cssl_xr_*`.
//
//   Each delegates to its `*_impl` Rust-side counterpart so unit tests
//   exercise behavior without going through the FFI boundary.
// ───────────────────────────────────────────────────────────────────────

/// FFI : create an XR session with the given `flags`.
///
/// Returns the session handle or `INVALID_XR_HANDLE` on error. Caller
/// inspects `__cssl_xr_last_error_kind()` for the failure reason.
///
/// # Safety
/// `extern "C"` ; safe to call from any thread.
#[no_mangle]
pub unsafe extern "C" fn __cssl_xr_session_create(flags: u32) -> u64 {
    xr_session_create_impl(flags)
}

/// FFI : destroy a session previously created via `__cssl_xr_session_create`.
///
/// Returns 0 on success, negative i32 on error.
///
/// # Safety
/// `extern "C"` ; `session` must be a handle returned by
/// `__cssl_xr_session_create` and not yet destroyed.
#[no_mangle]
pub unsafe extern "C" fn __cssl_xr_session_destroy(session: u64) -> i32 {
    xr_session_destroy_impl(session)
}

/// FFI : stream the latest head + controller poses.
///
/// `head_pose_out` MUST be non-null + valid for ≥ `XR_POSE_STREAM_BYTES`
/// bytes. `controller_pose_out` is optional (may be null) ; if non-null
/// it receives a copy of the controller halves (56 bytes).
///
/// Returns the bytes written or negative i32 on error.
///
/// § PRIME-DIRECTIVE LABEL
///   The bytes written are `Sensitive<Spatial>` (head pose) +
///   `Sensitive<Behavioral>` (controller pose). Caller's CSSLv3-side
///   wrapper MUST honor the IFC label ; cssl-rt does not buffer or
///   forward these bytes anywhere else.
///
/// # Safety
/// `extern "C"` ; caller must ensure `head_pose_out` valid for `max_len`
/// bytes (when `max_len > 0`) and that `controller_pose_out` is either
/// null or valid for at least 56 bytes.
#[no_mangle]
pub unsafe extern "C" fn __cssl_xr_pose_stream(
    session: u64,
    head_pose_out: *mut u8,
    controller_pose_out: *mut u8,
    max_len: usize,
) -> i32 {
    // SAFETY : caller pre-cond on the FFI symbol = caller pre-cond on
    // the impl ; the impl's SAFETY block documents the bounded-write.
    unsafe { xr_pose_stream_impl(session, head_pose_out, controller_pose_out, max_len) }
}

/// FFI : acquire a stereo swapchain image for `eye` ∈ {LEFT, RIGHT}.
///
/// Returns 0 on success ; writes the GPU image id to `*image_out`.
///
/// # Safety
/// `extern "C"` ; `image_out` must be non-null + properly aligned for `u64`.
#[no_mangle]
pub unsafe extern "C" fn __cssl_xr_swapchain_stereo_acquire(
    session: u64,
    eye: u32,
    image_out: *mut u64,
) -> i32 {
    // SAFETY : caller pre-cond on FFI symbol = pre-cond on impl.
    unsafe { xr_swapchain_stereo_acquire_impl(session, eye, image_out) }
}

/// FFI : release a stereo swapchain image previously acquired via
/// `__cssl_xr_swapchain_stereo_acquire`.
///
/// Returns 0 on success, negative i32 on error.
///
/// # Safety
/// `extern "C"` ; `session` + `eye` + `image` must match a prior acquire.
#[no_mangle]
pub unsafe extern "C" fn __cssl_xr_swapchain_stereo_release(
    session: u64,
    eye: u32,
    image: u64,
) -> i32 {
    xr_swapchain_stereo_release_impl(session, eye, image)
}

/// FFI : poll the latest controller input state.
///
/// `state_out` MUST be non-null + valid for ≥ `XR_INPUT_STATE_BYTES` bytes.
/// `controller_idx` ∈ {0=left, 1=right}.
///
/// Returns bytes written (8 on success) or negative i32 on error.
///
/// § PRIME-DIRECTIVE LABEL
///   The bytes written are `Sensitive<Behavioral>`. Same egress
///   prohibitions as `__cssl_xr_pose_stream`.
///
/// # Safety
/// `extern "C"` ; caller must ensure `state_out` valid for `max_len`
/// bytes (when `max_len > 0`).
#[no_mangle]
pub unsafe extern "C" fn __cssl_xr_input_state(
    session: u64,
    controller_idx: u32,
    state_out: *mut u8,
    max_len: usize,
) -> i32 {
    // SAFETY : caller pre-cond on FFI symbol = pre-cond on impl.
    unsafe { xr_input_state_impl(session, controller_idx, state_out, max_len) }
}

/// FFI : read the canonical error kind from the last xr op.
#[no_mangle]
pub extern "C" fn __cssl_xr_last_error_kind() -> i32 {
    last_xr_error_kind()
}

/// FFI : read the raw OS code from the last xr op.
#[no_mangle]
pub extern "C" fn __cssl_xr_last_error_os() -> i32 {
    last_xr_error_os()
}

/// FFI : grant cap-bits to the per-process XR cap-set.
#[no_mangle]
pub extern "C" fn __cssl_xr_caps_grant(cap_bits: i32) -> i32 {
    xr_caps_grant(cap_bits)
}

/// FFI : revoke cap-bits from the per-process XR cap-set.
#[no_mangle]
pub extern "C" fn __cssl_xr_caps_revoke(cap_bits: i32) -> i32 {
    xr_caps_revoke(cap_bits)
}

/// FFI : read the current XR cap-set.
#[no_mangle]
pub extern "C" fn __cssl_xr_caps_current() -> i32 {
    xr_caps_current()
}

// ───────────────────────────────────────────────────────────────────────
// § INTEGRATION_NOTE  (per Wave-D8 dispatch directive)
//
//   This module is delivered as a NEW file. Per the task's HARD
//   CONSTRAINTS, `cssl-rt/src/lib.rs` + `cssl-rt/Cargo.toml` are
//   intentionally NOT modified. The `__cssl_xr_*` extern "C" symbols
//   in this file are still fully linkable — `#[no_mangle] pub extern
//   "C"` reaches the final cssl-rt cdylib without needing a
//   `pub mod host_xr;` declaration in lib.rs ; the Rust `cargo build`
//   driver compiles every file under `src/`. (Confirm by inspecting
//   the .rlib symbol table after `cargo build -p cssl-rt`.) However
//   the Rust-side helper fns (`xr_session_create_impl` etc.) are NOT
//   reachable from outside the crate without a `pub mod` line.
//
//   Wave-D8b (follow-up) wires it up :
//     1. Add `pub mod host_xr;` to `cssl-rt/src/lib.rs`.
//     2. Add re-exports for the 5 cap-fns + 6 counter-fns + the
//        ABI constants (XR_CAP_*, XR_EYE_*, XR_*_BYTES, INVALID_XR_HANDLE,
//        xr_error_code) to the top-level public surface.
//     3. Wire `host_xr::reset_xr_for_tests()` into the
//        `test_helpers::lock_and_reset_all` shared-lock-and-reset path.
//     4. Optionally add a `cssl-host-openxr` Cargo dep + reroute the
//        `*_impl` SWAP-POINTs through it for real-hw integration.
//
//   Until Wave-D8b lands the helpers compile + are tested in-place via
//   `#[cfg(test)]` references inside this file. The extern "C" symbols
//   are the ABI-stable surface ; any cgen-emitted code calling them
//   does NOT need the helpers to be re-exported.
//
//   § SWAP-POINT for real-hw integration :
//     - `xr_session_create_impl` : call into `cssl_host_openxr::session::
//       create()` after PRIME-DIRECTIVE consent ceremony.
//     - `xr_pose_stream_impl` : call `cssl_host_openxr::session::locate_
//       views()` + `::locate_action_space()` + serialize 7-float-per-pose.
//     - `xr_swapchain_stereo_acquire_impl` : call
//       `cssl_host_openxr::swapchain::xr_acquire_swapchain_image()`
//       + `xr_wait_swapchain_image()`.
//     - `xr_swapchain_stereo_release_impl` : call `xr_release_swapchain_
//       image()`.
//     - `xr_input_state_impl` : call `cssl_host_openxr::action::poll_
//       input_state()` + serialize 8-byte payload.
//
//   None of these SWAP-POINTs are reachable on non-Windows / non-Quest
//   builds without the `cssl-host-openxr` Cargo dep ; the stub bodies
//   above keep the cssl-rt crate Cargo-dep-clean per the task's
//   "Mock-when-deps-missing" directive.
// ───────────────────────────────────────────────────────────────────────

// ───────────────────────────────────────────────────────────────────────
// § Tests — symbol-arity · session-roundtrip · pose-stream-bounded ·
// stereo-eye-enum-dispatch · cap-deny · slot-table-reuse · LUT lock.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        eye_is_valid, eye_name, last_xr_error_kind, lookup_session, reset_xr_for_tests,
        xr_caps_current, xr_caps_grant, xr_caps_revoke, xr_error_code, xr_input_state_impl,
        xr_pose_stream_impl, xr_session_create_impl, xr_session_destroy_impl,
        xr_swapchain_stereo_acquire_impl, xr_swapchain_stereo_release_impl, EYE_NAME_LUT,
        INVALID_XR_HANDLE, XR_CAP_INPUT, XR_CAP_MASK, XR_CAP_NONE, XR_CAP_POSE_CONTROLLER,
        XR_CAP_POSE_HEAD, XR_CAP_SESSION, XR_CAP_SWAPCHAIN, XR_EYE_COUNT, XR_EYE_LEFT,
        XR_EYE_RIGHT, XR_INPUT_STATE_BYTES, XR_MAX_SESSIONS, XR_POSE_BYTES, XR_POSE_STREAM_BYTES,
    };
    use std::sync::Mutex;

    // ── crate-internal lock so cap + slot-table tests don't race ──────────
    static XR_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = match XR_TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                XR_TEST_LOCK.clear_poison();
                p.into_inner()
            }
        };
        reset_xr_for_tests();
        g
    }

    // ── ABI const sanity (lock to FFI surface) ─────────────────────────

    #[test]
    fn abi_constants_match_canonical_values() {
        assert_eq!(INVALID_XR_HANDLE, 0);
        assert_eq!(XR_CAP_NONE, 0);
        assert_eq!(XR_CAP_SESSION, 0x0001);
        assert_eq!(XR_CAP_POSE_HEAD, 0x0002);
        assert_eq!(XR_CAP_POSE_CONTROLLER, 0x0004);
        assert_eq!(XR_CAP_SWAPCHAIN, 0x0008);
        assert_eq!(XR_CAP_INPUT, 0x0010);
        assert_eq!(
            XR_CAP_MASK,
            XR_CAP_SESSION
                | XR_CAP_POSE_HEAD
                | XR_CAP_POSE_CONTROLLER
                | XR_CAP_SWAPCHAIN
                | XR_CAP_INPUT
        );
        assert_eq!(XR_EYE_LEFT, 0);
        assert_eq!(XR_EYE_RIGHT, 1);
        assert_eq!(XR_EYE_COUNT, 2);
        assert_eq!(XR_POSE_BYTES, 28);
        assert_eq!(XR_POSE_STREAM_BYTES, 28 * 3);
        assert_eq!(XR_INPUT_STATE_BYTES, 8);
    }

    #[test]
    fn cap_bits_are_distinct_powers_of_two() {
        let bits = [
            XR_CAP_SESSION,
            XR_CAP_POSE_HEAD,
            XR_CAP_POSE_CONTROLLER,
            XR_CAP_SWAPCHAIN,
            XR_CAP_INPUT,
        ];
        for (i, &a) in bits.iter().enumerate() {
            assert!((a as u32).is_power_of_two(), "cap {i} = {a:#x} not power-of-two");
            for (j, &b) in bits.iter().enumerate() {
                if i != j {
                    assert_eq!(a & b, 0, "caps {i} + {j} overlap");
                }
            }
        }
    }

    // ── eye-enum LUT dispatch ────────────────────────────────────────

    #[test]
    fn eye_enum_dispatch_left_right_via_lut() {
        assert_eq!(eye_name(XR_EYE_LEFT), Some("left"));
        assert_eq!(eye_name(XR_EYE_RIGHT), Some("right"));
        assert_eq!(eye_name(2), None);
        assert_eq!(eye_name(99), None);
        assert_eq!(EYE_NAME_LUT[0], "left");
        assert_eq!(EYE_NAME_LUT[1], "right");
        assert!(eye_is_valid(0));
        assert!(eye_is_valid(1));
        assert!(!eye_is_valid(2));
    }

    // ── cap-gate enforcement (PRIME-DIRECTIVE) ─────────────────────────

    #[test]
    fn session_create_denied_without_cap_session() {
        let _g = lock_and_reset();
        // No cap granted ; default = XR_CAP_NONE.
        assert_eq!(xr_caps_current(), XR_CAP_NONE);
        let h = xr_session_create_impl(0);
        assert_eq!(h, INVALID_XR_HANDLE);
        assert_eq!(last_xr_error_kind(), xr_error_code::CAP_DENIED);
    }

    #[test]
    fn pose_stream_denied_without_pose_caps() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION);
        let h = xr_session_create_impl(0);
        assert_ne!(h, INVALID_XR_HANDLE);
        // Granted SESSION but not POSE_HEAD / POSE_CONTROLLER.
        let mut buf = [0u8; XR_POSE_STREAM_BYTES];
        let r = unsafe { xr_pose_stream_impl(h, buf.as_mut_ptr(), core::ptr::null_mut(), buf.len()) };
        assert_eq!(r, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::CAP_DENIED);
    }

    // ── session-handle-roundtrip ───────────────────────────────────────

    #[test]
    fn session_create_destroy_roundtrip() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION);
        let h = xr_session_create_impl(0xABCD);
        assert_ne!(h, INVALID_XR_HANDLE);
        // Slot is live ; lookup must succeed.
        assert!(lookup_session(h).is_some());
        let r = xr_session_destroy_impl(h);
        assert_eq!(r, 0);
        // After destroy, the same handle is stale.
        assert!(lookup_session(h).is_none());
        // Double-destroy reports invalid-session.
        let r2 = xr_session_destroy_impl(h);
        assert_eq!(r2, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::INVALID_SESSION);
    }

    #[test]
    fn session_handle_generation_invalidates_after_destroy() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION);
        let h1 = xr_session_create_impl(0);
        assert_ne!(h1, INVALID_XR_HANDLE);
        let _ = xr_session_destroy_impl(h1);
        let h2 = xr_session_create_impl(0);
        assert_ne!(h2, INVALID_XR_HANDLE);
        // h1 must NOT alias h2 even if the slot index is identical —
        // generation counter has bumped.
        assert_ne!(h1, h2, "destroyed handle must not alias new handle");
        assert!(lookup_session(h1).is_none(), "stale handle must not lookup");
        assert!(lookup_session(h2).is_some());
    }

    #[test]
    fn session_table_caps_at_max_sessions() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION);
        let mut handles = Vec::new();
        for _ in 0..XR_MAX_SESSIONS {
            let h = xr_session_create_impl(0);
            assert_ne!(h, INVALID_XR_HANDLE);
            handles.push(h);
        }
        // (XR_MAX_SESSIONS + 1)-th create must fail with SESSION_LIMIT.
        let overflow = xr_session_create_impl(0);
        assert_eq!(overflow, INVALID_XR_HANDLE);
        assert_eq!(last_xr_error_kind(), xr_error_code::SESSION_LIMIT);
        // After freeing one, a new create must succeed (slot reuse).
        let _ = xr_session_destroy_impl(handles[0]);
        let h_reused = xr_session_create_impl(0);
        assert_ne!(h_reused, INVALID_XR_HANDLE);
    }

    // ── pose-stream bounded-write contract ─────────────────────────────

    #[test]
    fn pose_stream_writes_exactly_84_bytes() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_POSE_HEAD | XR_CAP_POSE_CONTROLLER);
        let h = xr_session_create_impl(0);
        assert_ne!(h, INVALID_XR_HANDLE);
        let mut head = [0xFFu8; XR_POSE_STREAM_BYTES];
        let r = unsafe {
            xr_pose_stream_impl(h, head.as_mut_ptr(), core::ptr::null_mut(), head.len())
        };
        assert_eq!(r, XR_POSE_STREAM_BYTES as i32);
        // First 84 bytes overwritten ; deterministic-zero stub.
        for &b in &head {
            assert_eq!(b, 0);
        }
    }

    #[test]
    fn pose_stream_rejects_buffer_too_small() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_POSE_HEAD | XR_CAP_POSE_CONTROLLER);
        let h = xr_session_create_impl(0);
        let mut tiny = [0u8; 16];
        let r = unsafe {
            xr_pose_stream_impl(h, tiny.as_mut_ptr(), core::ptr::null_mut(), tiny.len())
        };
        assert_eq!(r, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::BUFFER_TOO_SMALL);
        // Buffer must NOT be partially-written on rejection.
        for &b in &tiny {
            assert_eq!(b, 0);
        }
    }

    #[test]
    fn pose_stream_rejects_null_head_buffer() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_POSE_HEAD | XR_CAP_POSE_CONTROLLER);
        let h = xr_session_create_impl(0);
        let r = unsafe {
            xr_pose_stream_impl(h, core::ptr::null_mut(), core::ptr::null_mut(), 0)
        };
        assert_eq!(r, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::INVALID_INPUT);
    }

    #[test]
    fn pose_stream_writes_optional_controller_buffer_when_provided() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_POSE_HEAD | XR_CAP_POSE_CONTROLLER);
        let h = xr_session_create_impl(0);
        let mut head = [0xFFu8; XR_POSE_STREAM_BYTES];
        let mut ctrl = [0xAAu8; XR_POSE_BYTES * 2];
        let r = unsafe {
            xr_pose_stream_impl(h, head.as_mut_ptr(), ctrl.as_mut_ptr(), head.len())
        };
        assert_eq!(r, XR_POSE_STREAM_BYTES as i32);
        // Both buffers overwritten by the deterministic-zero stub.
        for &b in &ctrl {
            assert_eq!(b, 0, "controller buffer must be overwritten");
        }
    }

    // ── stereo-swapchain acquire-release roundtrip ─────────────────────

    #[test]
    fn swapchain_acquire_release_roundtrip_left() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_SWAPCHAIN);
        let h = xr_session_create_impl(0);
        let mut img = 0u64;
        let r = unsafe { xr_swapchain_stereo_acquire_impl(h, XR_EYE_LEFT, &mut img) };
        assert_eq!(r, 0);
        assert_ne!(img, 0, "image id must be non-zero on success");
        let r2 = xr_swapchain_stereo_release_impl(h, XR_EYE_LEFT, img);
        assert_eq!(r2, 0);
        // Releasing the same image twice must fail.
        let r3 = xr_swapchain_stereo_release_impl(h, XR_EYE_LEFT, img);
        assert_eq!(r3, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::INVALID_SWAPCHAIN);
    }

    #[test]
    fn swapchain_eye_dispatch_left_and_right_independent() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_SWAPCHAIN);
        let h = xr_session_create_impl(0);
        let mut img_left = 0u64;
        let mut img_right = 0u64;
        let _ = unsafe { xr_swapchain_stereo_acquire_impl(h, XR_EYE_LEFT, &mut img_left) };
        let _ = unsafe { xr_swapchain_stereo_acquire_impl(h, XR_EYE_RIGHT, &mut img_right) };
        assert_ne!(img_left, img_right, "stereo eyes must get distinct image ids");
        // Cross-eye release must fail (image_left under EYE_RIGHT slot ≠ last).
        let r_cross = xr_swapchain_stereo_release_impl(h, XR_EYE_RIGHT, img_left);
        assert_eq!(r_cross, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::INVALID_SWAPCHAIN);
    }

    #[test]
    fn swapchain_rejects_invalid_eye() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_SWAPCHAIN);
        let h = xr_session_create_impl(0);
        let mut img = 0u64;
        let r = unsafe { xr_swapchain_stereo_acquire_impl(h, 99, &mut img) };
        assert_eq!(r, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::EYE_OUT_OF_RANGE);
    }

    // ── input-state bounded-write ──────────────────────────────────────

    #[test]
    fn input_state_writes_exactly_8_bytes() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_INPUT);
        let h = xr_session_create_impl(0);
        let mut state = [0xFFu8; XR_INPUT_STATE_BYTES];
        let r = unsafe {
            xr_input_state_impl(h, 0, state.as_mut_ptr(), state.len())
        };
        assert_eq!(r, XR_INPUT_STATE_BYTES as i32);
        for &b in &state {
            assert_eq!(b, 0);
        }
    }

    #[test]
    fn input_state_rejects_invalid_controller_idx() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_INPUT);
        let h = xr_session_create_impl(0);
        let mut state = [0u8; XR_INPUT_STATE_BYTES];
        let r = unsafe {
            xr_input_state_impl(h, 99, state.as_mut_ptr(), state.len())
        };
        assert_eq!(r, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::INVALID_INPUT);
    }

    #[test]
    fn input_state_rejects_buffer_too_small() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_SESSION | XR_CAP_INPUT);
        let h = xr_session_create_impl(0);
        let mut tiny = [0u8; 4];
        let r = unsafe {
            xr_input_state_impl(h, 0, tiny.as_mut_ptr(), tiny.len())
        };
        assert_eq!(r, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::BUFFER_TOO_SMALL);
    }

    // ── caps revoke removes access ─────────────────────────────────────

    #[test]
    fn caps_revoke_disables_subsequent_ops() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_MASK);
        let h = xr_session_create_impl(0);
        assert_ne!(h, INVALID_XR_HANDLE);
        // Revoke pose caps ; subsequent pose_stream must deny.
        xr_caps_revoke(XR_CAP_POSE_HEAD | XR_CAP_POSE_CONTROLLER);
        let mut buf = [0u8; XR_POSE_STREAM_BYTES];
        let r = unsafe {
            xr_pose_stream_impl(h, buf.as_mut_ptr(), core::ptr::null_mut(), buf.len())
        };
        assert_eq!(r, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::CAP_DENIED);
    }

    // ── invalid-session early-reject ───────────────────────────────────

    #[test]
    fn ops_reject_invalid_session_handle() {
        let _g = lock_and_reset();
        xr_caps_grant(XR_CAP_MASK);
        // Never created any session ; arbitrary u64 must be rejected.
        let mut img = 0u64;
        let r = unsafe { xr_swapchain_stereo_acquire_impl(0xDEAD_BEEF_u64, XR_EYE_LEFT, &mut img) };
        assert_eq!(r, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::INVALID_SESSION);
        let mut state = [0u8; XR_INPUT_STATE_BYTES];
        let r2 = unsafe {
            xr_input_state_impl(0xDEAD_BEEF_u64, 0, state.as_mut_ptr(), state.len())
        };
        assert_eq!(r2, -1);
        assert_eq!(last_xr_error_kind(), xr_error_code::INVALID_SESSION);
    }

    // ── extern-C symbol arity (compile-time witness) ───────────────────

    #[test]
    fn extern_c_symbols_have_expected_arity() {
        // Compile-time : the fn-ptr type-check witnesses the ABI shape.
        // If any symbol's arity drifts from `specs/24_HOST_FFI.csl
        // § ABI-STABLE-SYMBOLS § vr-xr` this test stops compiling.
        let _create: unsafe extern "C" fn(u32) -> u64 = super::__cssl_xr_session_create;
        let _destroy: unsafe extern "C" fn(u64) -> i32 = super::__cssl_xr_session_destroy;
        let _pose: unsafe extern "C" fn(u64, *mut u8, *mut u8, usize) -> i32 =
            super::__cssl_xr_pose_stream;
        let _acq: unsafe extern "C" fn(u64, u32, *mut u64) -> i32 =
            super::__cssl_xr_swapchain_stereo_acquire;
        let _rel: unsafe extern "C" fn(u64, u32, u64) -> i32 =
            super::__cssl_xr_swapchain_stereo_release;
        let _input: unsafe extern "C" fn(u64, u32, *mut u8, usize) -> i32 =
            super::__cssl_xr_input_state;
        let _err_kind: extern "C" fn() -> i32 = super::__cssl_xr_last_error_kind;
        let _err_os: extern "C" fn() -> i32 = super::__cssl_xr_last_error_os;
        let _grant: extern "C" fn(i32) -> i32 = super::__cssl_xr_caps_grant;
        let _revoke: extern "C" fn(i32) -> i32 = super::__cssl_xr_caps_revoke;
        let _current: extern "C" fn() -> i32 = super::__cssl_xr_caps_current;
    }
}
