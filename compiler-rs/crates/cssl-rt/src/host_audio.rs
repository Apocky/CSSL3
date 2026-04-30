//! § cssl-rt host_audio — Wave-D6 (S7-host-FFI / specs/24_HOST_FFI.csl § audio).
//!
//! § ROLE
//!   Stage-0 implementation of the four ABI-stable `__cssl_audio_*` extern
//!   symbols documented in `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § audio` :
//!     `__cssl_audio_stream_open(flags, sample_rate, channels, fmt) -> u64`
//!     `__cssl_audio_stream_write(stream, buf, len)               -> i64`
//!     `__cssl_audio_stream_read (stream, buf, len)               -> i64`
//!     `__cssl_audio_stream_close(stream)                         -> i32`
//!
//!   Mirrors the `crate::net` 3-layer pattern : platform layer (cssl-host-
//!   audio cpal-based ; NOT directly imported here per task constraint
//!   "DO NOT modify Cargo.toml"), this module (slot-table + format-LUT +
//!   flags-bitset + last-error slot + counter discipline), and the
//!   `extern "C"` symbols at the bottom of this file. Each FFI symbol
//!   delegates to a Rust-side `*_impl` helper for testability per the
//!   `crate::ffi` pattern (S6-A1 / T11-D52).
//!
//! § AUDIOSTREAM HANDLES — slot-table (Sawyer-efficiency)
//!   `u64` handle : low-32 = slot index (0..MAX_STREAMS) , high-32 =
//!   generation counter (bumped on FREE→OPEN transition ; defends against
//!   use-after-close). `0` is the INVALID_STREAM sentinel ; never produced
//!   by a successful open.
//!
//! § AUDIO FORMAT — LUT (no String formatting on hot paths)
//!   `fmt` packed `u32` : 1=i16 · 2=i24 · 3=i32 · 4=f32 · 5=f64. Other
//!   values rejected with `last_error_kind = INVALID_INPUT`.
//!
//! § FLAGS — bitset
//!   bit 0 : OUTPUT (mutually exclusive with bit 1)
//!   bit 1 : INPUT  (microphone — DEFAULT-DENIED per PRIME-DIRECTIVE)
//!   bit 2 : SHARED (default OS mix-format)
//!   bit 3 : EXCLUSIVE (Win-only ; mutually exclusive with bit 2)
//!   bit 4 : NONBLOCK (write returns WOULD_BLOCK on full-ring)
//!
//! § PRIME-DIRECTIVE — microphone INPUT default-DENIED  (W! read-first)
//!   Per `specs/24_HOST_FFI.csl § IFC-LABELS` `Audio-mic-stream :
//!   Sensitive<Voice> ← default-DENIED capability` AND
//!   `PRIME_DIRECTIVE.md § PROHIBITIONS § surveillance` ("silent microphone
//!   activation is a BUG class"). At THIS layer we enforce a STRUCTURAL
//!   bottom-default : `STREAM_INPUT` returns `0` (open failure) with
//!   `last_error_kind = INPUT_DENIED` UNLESS `audio_caps_grant(
//!   AUDIO_CAP_INPUT)` has been called. The mic-cap-grant itself is an
//!   UPSTREAM gate (`Cap<Audio>` witness from §§ 12 capability machinery)
//!   — this runtime layer is the SAFETY NET that ensures even-if-upstream-
//!   is-bypassed there is a default-DENIED structural barrier. There is
//!   no PRIME-DIRECTIVE override.
//!
//! § INTEGRATION_NOTE  (per Wave-D6 dispatch directive)
//!   Module is NEW ; `cssl-rt/src/lib.rs` is intentionally NOT modified
//!   per task constraint. A future cssl-rt refactor will (1) add
//!   `pub mod host_audio;` to `lib.rs`, (2) add `cssl-host-audio =
//!   { path = "../cssl-host-audio" }` to the `cssl-rt` Cargo.toml,
//!   (3) replace the slot-table back-end internals here with a thin
//!   delegation to `cssl_host_audio::AudioStream` (slot-table front-end
//!   stays identical so the FFI handle-shape never changes), (4) wire
//!   `audio_caps_grant` / `audio_caps_revoke` to the §§ 12 cap machinery.
//!   Until then the slot-table is self-contained ; tests cover the entire
//!   surface without going through cssl-host-audio.

#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § ABI-stable constants — flag bitset
// ───────────────────────────────────────────────────────────────────────

/// Open the stream as an OUTPUT device (default ; speaker / headphones).
pub const STREAM_OUTPUT: u32 = 0x0000_0001;
/// Open the stream as an INPUT device (microphone). PRIME-DIRECTIVE :
/// default-DENIED ; requires [`AUDIO_CAP_INPUT`] to be granted.
pub const STREAM_INPUT: u32 = 0x0000_0002;
/// Shared-mode device sharing (default ; OS mix-format ; multi-process).
pub const STREAM_SHARED: u32 = 0x0000_0004;
/// Exclusive-mode (Windows-only ; lowest latency ; excludes other procs).
pub const STREAM_EXCLUSIVE: u32 = 0x0000_0008;
/// Non-blocking I/O — write/read returns WOULD_BLOCK on full-ring.
pub const STREAM_NONBLOCK: u32 = 0x0000_0010;

/// Mask of recognized stream-flag bits. Other bits = rejected.
pub const STREAM_FLAG_MASK: u32 =
    STREAM_OUTPUT | STREAM_INPUT | STREAM_SHARED | STREAM_EXCLUSIVE | STREAM_NONBLOCK;

// ───────────────────────────────────────────────────────────────────────
// § ABI-stable constants — format LUT (renaming = major-version-bump)
// ───────────────────────────────────────────────────────────────────────

/// `i16` interleaved (2 bytes per sample per channel).
pub const FMT_I16: u32 = 0x0000_0001;
/// `i24` interleaved (3 bytes per sample per channel).
pub const FMT_I24: u32 = 0x0000_0002;
/// `i32` interleaved (4 bytes per sample per channel).
pub const FMT_I32: u32 = 0x0000_0003;
/// `f32` interleaved (4 bytes per sample per channel ; canonical
/// stage-0 format ; matches `cssl-host-audio` AudioFormat).
pub const FMT_F32: u32 = 0x0000_0004;
/// `f64` interleaved (8 bytes per sample per channel).
pub const FMT_F64: u32 = 0x0000_0005;

/// Lowest valid format-code (inclusive).
pub const FMT_MIN: u32 = FMT_I16;
/// Highest valid format-code (inclusive).
pub const FMT_MAX: u32 = FMT_F64;

// ───────────────────────────────────────────────────────────────────────
// § ABI-stable constants — capability bitset
// ───────────────────────────────────────────────────────────────────────

/// Default audio-cap bitset — output-only ; mic-input default-DENIED.
pub const AUDIO_CAP_DEFAULT: u32 = AUDIO_CAP_OUTPUT;
/// Output-stream capability (granted by default at process start).
pub const AUDIO_CAP_OUTPUT: u32 = 0x0000_0001;
/// Input-stream / microphone capability — Sensitive<Voice>. Default-DENIED.
pub const AUDIO_CAP_INPUT: u32 = 0x0000_0002;
/// Mask of recognized cap-bits.
pub const AUDIO_CAP_MASK: u32 = AUDIO_CAP_OUTPUT | AUDIO_CAP_INPUT;

// ───────────────────────────────────────────────────────────────────────
// § ABI-stable constants — sentinels
// ───────────────────────────────────────────────────────────────────────

/// Sentinel "invalid stream" handle — returned by failed
/// `__cssl_audio_stream_open` ; rejected by every other op.
pub const INVALID_STREAM: u64 = 0;

/// Maximum simultaneous open audio-streams in this stage-0 slot-table.
/// 32 covers every realistic CSSL-game use case (BGM + SFX + voice-out).
pub const MAX_STREAMS: usize = 32;

// ───────────────────────────────────────────────────────────────────────
// § audio_error_code — stable error-kind values
// ───────────────────────────────────────────────────────────────────────

/// Stable error-kind values reported by [`last_audio_error_kind`].
pub mod audio_error_code {
    /// No prior error recorded since [`super::reset_audio_for_tests`].
    pub const OK: i32 = 0;
    /// `flags` / `fmt` / `sample_rate` / `channels` argument validation failed.
    pub const INVALID_INPUT: i32 = 1;
    /// Microphone-input requested but [`super::AUDIO_CAP_INPUT`] not granted.
    /// The structural PRIME-DIRECTIVE safety net.
    pub const INPUT_DENIED: i32 = 2;
    /// All [`super::MAX_STREAMS`] slots are in use.
    pub const SLOT_TABLE_FULL: i32 = 3;
    /// Handle invalid OR generation stale (use-after-close).
    pub const INVALID_HANDLE: i32 = 4;
    /// Non-blocking op on full-ring : would have to wait.
    pub const WOULD_BLOCK: i32 = 5;
    /// Format / rate / channels combo not supported by platform layer.
    pub const NOT_SUPPORTED: i32 = 6;
    /// Generic platform failure (cpal / WASAPI / ALSA returned an error).
    pub const PLATFORM_ERROR: i32 = 7;
}

// ───────────────────────────────────────────────────────────────────────
// § format LUT — bytes-per-sample-per-channel (no String fmt)
// ───────────────────────────────────────────────────────────────────────

/// Bytes-per-sample-per-channel for the given format-code. Returns `0`
/// for unrecognized format. Branch-order : f32 first (canonical),
/// i16 second (most common device-native), rest in declaration order.
#[must_use]
pub const fn format_bytes_per_sample(fmt: u32) -> u32 {
    match fmt {
        FMT_F32 => 4,
        FMT_I16 => 2,
        FMT_I24 => 3,
        FMT_I32 => 4,
        FMT_F64 => 8,
        _ => 0,
    }
}

/// Predicate : is `fmt` a recognized format-code ?
#[must_use]
pub const fn is_valid_format(fmt: u32) -> bool {
    matches!(fmt, FMT_I16 | FMT_I24 | FMT_I32 | FMT_F32 | FMT_F64)
}

// ───────────────────────────────────────────────────────────────────────
// § slot-table — fixed-size open-addressed table of audio streams
// ───────────────────────────────────────────────────────────────────────

const STATE_FREE: u32 = 0;
const STATE_OPEN: u32 = 1;

#[derive(Debug, Default)]
struct AudioSlot {
    state: AtomicU32,
    /// Bumped on every FREE→OPEN transition. Handles whose embedded
    /// generation does NOT match the slot's current generation =
    /// REJECTED by every op.
    generation: AtomicU32,
    fmt: AtomicU32,
    sample_rate: AtomicU32,
    channels: AtomicU32,
    flags: AtomicU32,
    bytes_written: AtomicU64,
    bytes_read: AtomicU64,
}

const SLOT_DEFAULT: AudioSlot = AudioSlot {
    state: AtomicU32::new(STATE_FREE),
    generation: AtomicU32::new(0),
    fmt: AtomicU32::new(0),
    sample_rate: AtomicU32::new(0),
    channels: AtomicU32::new(0),
    flags: AtomicU32::new(0),
    bytes_written: AtomicU64::new(0),
    bytes_read: AtomicU64::new(0),
};

static SLOT_TABLE: [AudioSlot; MAX_STREAMS] = [SLOT_DEFAULT; MAX_STREAMS];

// ───────────────────────────────────────────────────────────────────────
// § handle-encoding helpers
// ───────────────────────────────────────────────────────────────────────

/// Encode `(slot_idx, generation)` into a `u64` handle.
#[must_use]
pub const fn encode_handle(slot_idx: u32, generation: u32) -> u64 {
    ((generation as u64) << 32) | (slot_idx as u64)
}

/// Decode a `u64` handle into `(slot_idx, generation)`.
#[must_use]
pub const fn decode_handle(handle: u64) -> (u32, u32) {
    let slot_idx = (handle & 0xFFFF_FFFF) as u32;
    let generation = (handle >> 32) as u32;
    (slot_idx, generation)
}

// ───────────────────────────────────────────────────────────────────────
// § global counters + last-error slot
// ───────────────────────────────────────────────────────────────────────

static OPEN_COUNT: AtomicU64 = AtomicU64::new(0);
static CLOSE_COUNT: AtomicU64 = AtomicU64::new(0);
static BYTES_WRITTEN_TOTAL: AtomicU64 = AtomicU64::new(0);
static BYTES_READ_TOTAL: AtomicU64 = AtomicU64::new(0);
static FORMAT_DISPATCH_COUNT: AtomicU64 = AtomicU64::new(0);
static MIC_DENIED_COUNT: AtomicU64 = AtomicU64::new(0);

static LAST_ERROR_KIND: AtomicI32 = AtomicI32::new(audio_error_code::OK);
static AUDIO_CAPS: AtomicU32 = AtomicU32::new(AUDIO_CAP_DEFAULT);

/// Total successful audio_stream_open calls since process start.
#[must_use]
pub fn open_count() -> u64 { OPEN_COUNT.load(Ordering::Relaxed) }
/// Total successful audio_stream_close calls since process start.
#[must_use]
pub fn close_count() -> u64 { CLOSE_COUNT.load(Ordering::Relaxed) }
/// Total bytes successfully written via audio_stream_write.
#[must_use]
pub fn bytes_written_total() -> u64 { BYTES_WRITTEN_TOTAL.load(Ordering::Relaxed) }
/// Total bytes successfully read via audio_stream_read.
#[must_use]
pub fn bytes_read_total() -> u64 { BYTES_READ_TOTAL.load(Ordering::Relaxed) }
/// Total format-LUT dispatches (sanity counter for hot-path tests).
#[must_use]
pub fn format_dispatch_count() -> u64 { FORMAT_DISPATCH_COUNT.load(Ordering::Relaxed) }
/// Total mic-INPUT requests rejected by the structural cap-gate.
/// Audit-visible counter — never egresses.
#[must_use]
pub fn mic_denied_count() -> u64 { MIC_DENIED_COUNT.load(Ordering::Relaxed) }
/// Last audio-error kind (per `audio_error_code` constants).
#[must_use]
pub fn last_audio_error_kind() -> i32 { LAST_ERROR_KIND.load(Ordering::Relaxed) }
/// Current per-process audio capability bitset.
#[must_use]
pub fn audio_caps_current() -> u32 { AUDIO_CAPS.load(Ordering::Relaxed) }

/// Grant a subset of `AUDIO_CAP_*` capabilities. Returns the new caps
/// bitset. Caps not in [`AUDIO_CAP_MASK`] are silently dropped.
pub fn audio_caps_grant(bits: u32) -> u32 {
    AUDIO_CAPS.fetch_or(bits & AUDIO_CAP_MASK, Ordering::SeqCst);
    AUDIO_CAPS.load(Ordering::Relaxed)
}

/// Revoke a subset of `AUDIO_CAP_*` capabilities. Returns the new caps bitset.
pub fn audio_caps_revoke(bits: u32) -> u32 {
    AUDIO_CAPS.fetch_and(!(bits & AUDIO_CAP_MASK), Ordering::SeqCst);
    AUDIO_CAPS.load(Ordering::Relaxed)
}

/// Test-only : reset every audio counter + last-error slot + slot-table.
#[doc(hidden)]
pub fn reset_audio_for_tests() {
    OPEN_COUNT.store(0, Ordering::SeqCst);
    CLOSE_COUNT.store(0, Ordering::SeqCst);
    BYTES_WRITTEN_TOTAL.store(0, Ordering::SeqCst);
    BYTES_READ_TOTAL.store(0, Ordering::SeqCst);
    FORMAT_DISPATCH_COUNT.store(0, Ordering::SeqCst);
    MIC_DENIED_COUNT.store(0, Ordering::SeqCst);
    LAST_ERROR_KIND.store(audio_error_code::OK, Ordering::SeqCst);
    AUDIO_CAPS.store(AUDIO_CAP_DEFAULT, Ordering::SeqCst);
    for slot in SLOT_TABLE.iter() {
        slot.state.store(STATE_FREE, Ordering::SeqCst);
        slot.generation.store(0, Ordering::SeqCst);
        slot.fmt.store(0, Ordering::SeqCst);
        slot.sample_rate.store(0, Ordering::SeqCst);
        slot.channels.store(0, Ordering::SeqCst);
        slot.flags.store(0, Ordering::SeqCst);
        slot.bytes_written.store(0, Ordering::SeqCst);
        slot.bytes_read.store(0, Ordering::SeqCst);
    }
}

fn record_audio_error(kind: i32) {
    LAST_ERROR_KIND.store(kind, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § validation helpers
// ───────────────────────────────────────────────────────────────────────

/// Validate the `flags` argument for [`stream_open_impl`]. Returns
/// `Some(error_code)` if invalid, `None` if accepted.
#[must_use]
pub fn validate_open_flags(flags: u32) -> Option<i32> {
    if flags & !STREAM_FLAG_MASK != 0 {
        return Some(audio_error_code::INVALID_INPUT);
    }
    let has_out = (flags & STREAM_OUTPUT) != 0;
    let has_in = (flags & STREAM_INPUT) != 0;
    if has_out && has_in {
        return Some(audio_error_code::INVALID_INPUT);
    }
    let has_shared = (flags & STREAM_SHARED) != 0;
    let has_exclusive = (flags & STREAM_EXCLUSIVE) != 0;
    if has_shared && has_exclusive {
        return Some(audio_error_code::INVALID_INPUT);
    }
    None
}

/// Validate the `(sample_rate, channels)` pair. Boundaries match
/// cssl-host-audio's `SampleRate::custom` validator (rate 1..=768000) +
/// 1..=8 channel envelope (mono through 7.1).
#[must_use]
pub fn validate_open_format(sample_rate: u32, channels: u32) -> Option<i32> {
    if sample_rate == 0 || sample_rate > 768_000 {
        return Some(audio_error_code::INVALID_INPUT);
    }
    if channels == 0 || channels > 8 {
        return Some(audio_error_code::INVALID_INPUT);
    }
    None
}

/// Decode + validate a `u64` handle against the slot-table.
fn validate_handle(handle: u64) -> Result<usize, i32> {
    if handle == INVALID_STREAM {
        return Err(audio_error_code::INVALID_HANDLE);
    }
    let (slot_idx, gen_in_handle) = decode_handle(handle);
    let idx = slot_idx as usize;
    if idx >= MAX_STREAMS {
        return Err(audio_error_code::INVALID_HANDLE);
    }
    let slot = &SLOT_TABLE[idx];
    if slot.state.load(Ordering::Acquire) != STATE_OPEN {
        return Err(audio_error_code::INVALID_HANDLE);
    }
    if slot.generation.load(Ordering::Acquire) != gen_in_handle {
        return Err(audio_error_code::INVALID_HANDLE);
    }
    Ok(idx)
}

// ───────────────────────────────────────────────────────────────────────
// § *_impl helpers — Rust-side, testable without FFI
// ───────────────────────────────────────────────────────────────────────

/// Rust-side impl of `__cssl_audio_stream_open`. Returns the encoded
/// handle on success, [`INVALID_STREAM`] (0) on failure.
///
/// § PRIME-DIRECTIVE
///   `STREAM_INPUT` is REJECTED unless [`AUDIO_CAP_INPUT`] is granted.
///   The structural safety net ; upstream §§ 12 `Cap<Audio>` is the
///   primary gate.
pub fn stream_open_impl(flags: u32, sample_rate: u32, channels: u32, fmt: u32) -> u64 {
    if let Some(err) = validate_open_flags(flags) {
        record_audio_error(err);
        return INVALID_STREAM;
    }
    if let Some(err) = validate_open_format(sample_rate, channels) {
        record_audio_error(err);
        return INVALID_STREAM;
    }
    if !is_valid_format(fmt) {
        record_audio_error(audio_error_code::INVALID_INPUT);
        return INVALID_STREAM;
    }
    FORMAT_DISPATCH_COUNT.fetch_add(1, Ordering::Relaxed);

    if (flags & STREAM_INPUT) != 0 {
        let caps = AUDIO_CAPS.load(Ordering::Acquire);
        if (caps & AUDIO_CAP_INPUT) == 0 {
            MIC_DENIED_COUNT.fetch_add(1, Ordering::Relaxed);
            record_audio_error(audio_error_code::INPUT_DENIED);
            return INVALID_STREAM;
        }
    }

    for (idx, slot) in SLOT_TABLE.iter().enumerate() {
        if slot
            .state
            .compare_exchange(STATE_FREE, STATE_OPEN, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let new_gen = slot.generation.fetch_add(1, Ordering::AcqRel) + 1;
            slot.fmt.store(fmt, Ordering::Release);
            slot.sample_rate.store(sample_rate, Ordering::Release);
            slot.channels.store(channels, Ordering::Release);
            slot.flags.store(flags, Ordering::Release);
            slot.bytes_written.store(0, Ordering::Release);
            slot.bytes_read.store(0, Ordering::Release);
            OPEN_COUNT.fetch_add(1, Ordering::Relaxed);
            record_audio_error(audio_error_code::OK);
            return encode_handle(idx as u32, new_gen);
        }
    }
    record_audio_error(audio_error_code::SLOT_TABLE_FULL);
    INVALID_STREAM
}

/// Rust-side impl of `__cssl_audio_stream_write`. Returns bytes-accepted
/// on success, `-1` on error. `len == 0` is a valid no-op (returns 0).
///
/// At stage-0 the platform layer is not wired (§ INTEGRATION_NOTE) ; this
/// layer tallies bytes against the slot's `bytes_written` counter + the
/// global `BYTES_WRITTEN_TOTAL`. When the cssl-host-audio integration
/// lands the body becomes a delegation to `cssl_host_audio::AudioStream::
/// submit_frames` with the byte-count contract preserved.
///
/// # Safety
/// Caller must ensure `buf` is valid for `len` bytes (or `len == 0`).
pub fn stream_write_impl(stream: u64, _buf: *const u8, len: usize) -> i64 {
    let idx = match validate_handle(stream) {
        Ok(i) => i,
        Err(e) => {
            record_audio_error(e);
            return -1;
        }
    };
    if len == 0 {
        record_audio_error(audio_error_code::OK);
        return 0;
    }
    let slot = &SLOT_TABLE[idx];
    let flags = slot.flags.load(Ordering::Acquire);
    if (flags & STREAM_INPUT) != 0 {
        record_audio_error(audio_error_code::INVALID_INPUT);
        return -1;
    }
    let len_u64 = len as u64;
    slot.bytes_written.fetch_add(len_u64, Ordering::Relaxed);
    BYTES_WRITTEN_TOTAL.fetch_add(len_u64, Ordering::Relaxed);
    record_audio_error(audio_error_code::OK);
    len as i64
}

/// Rust-side impl of `__cssl_audio_stream_read`. Returns bytes-filled
/// on success, `-1` on error. Reads against OUTPUT streams REJECTED.
///
/// # Safety
/// Caller must ensure `buf` is valid for `len` bytes (or `len == 0`).
pub fn stream_read_impl(stream: u64, _buf: *mut u8, len: usize) -> i64 {
    let idx = match validate_handle(stream) {
        Ok(i) => i,
        Err(e) => {
            record_audio_error(e);
            return -1;
        }
    };
    let slot = &SLOT_TABLE[idx];
    let flags = slot.flags.load(Ordering::Acquire);
    if (flags & STREAM_INPUT) == 0 {
        record_audio_error(audio_error_code::INVALID_INPUT);
        return -1;
    }
    if len == 0 {
        record_audio_error(audio_error_code::OK);
        return 0;
    }
    let len_u64 = len as u64;
    slot.bytes_read.fetch_add(len_u64, Ordering::Relaxed);
    BYTES_READ_TOTAL.fetch_add(len_u64, Ordering::Relaxed);
    record_audio_error(audio_error_code::OK);
    len as i64
}

/// Rust-side impl of `__cssl_audio_stream_close`. Returns `0` on
/// success, `-1` on invalid handle. Idempotent : double-close on the
/// same handle returns `-1` on the second call (state-check rejects).
pub fn stream_close_impl(stream: u64) -> i32 {
    let idx = match validate_handle(stream) {
        Ok(i) => i,
        Err(e) => {
            record_audio_error(e);
            return -1;
        }
    };
    SLOT_TABLE[idx].state.store(STATE_FREE, Ordering::Release);
    CLOSE_COUNT.fetch_add(1, Ordering::Relaxed);
    record_audio_error(audio_error_code::OK);
    0
}

// ───────────────────────────────────────────────────────────────────────
// § ABI-stable extern "C" symbols — the four __cssl_audio_* primitives
//
//   ‼ Symbol names LOCKED FOREVER. Renaming = major-version-bump.
//     cssl-mir + cssl-cgen-cpu-cranelift must lock-step.
// ───────────────────────────────────────────────────────────────────────

/// FFI : open a new audio stream + return its handle. Returns `0` on
/// failure ; consult [`last_audio_error_kind`] for the failure reason.
///
/// # Safety
/// Pure validation + slot-table CAS — no raw-pointer deref.
#[no_mangle]
pub unsafe extern "C" fn __cssl_audio_stream_open(
    flags: u32,
    sample_rate: u32,
    channels: u32,
    fmt: u32,
) -> u64 {
    stream_open_impl(flags, sample_rate, channels, fmt)
}

/// FFI : write `len` bytes from `buf` into the stream's playback queue.
/// Returns bytes-accepted on success, `-1` on error.
///
/// # Safety
/// Caller must ensure `buf` is valid for `len` bytes (or `len == 0`).
#[no_mangle]
pub unsafe extern "C" fn __cssl_audio_stream_write(
    stream: u64,
    buf: *const u8,
    len: usize,
) -> i64 {
    stream_write_impl(stream, buf, len)
}

/// FFI : read up to `len` bytes from the stream's capture queue.
/// Returns bytes-filled on success, `-1` on error. Reads against OUTPUT
/// streams REJECTED.
///
/// # Safety
/// Caller must ensure `buf` is valid for `len` bytes (or `len == 0`).
#[no_mangle]
pub unsafe extern "C" fn __cssl_audio_stream_read(
    stream: u64,
    buf: *mut u8,
    len: usize,
) -> i64 {
    stream_read_impl(stream, buf, len)
}

/// FFI : close an audio stream, releasing its slot. Returns `0` on
/// success, `-1` on invalid handle. Idempotent.
///
/// # Safety
/// Pure slot-table mutation — no raw-pointer deref.
#[no_mangle]
pub unsafe extern "C" fn __cssl_audio_stream_close(stream: u64) -> i32 {
    stream_close_impl(stream)
}

// ───────────────────────────────────────────────────────────────────────
// § tests — 12 unit tests covering open · write · read · close · cap-gate
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = match TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                TEST_LOCK.clear_poison();
                p.into_inner()
            }
        };
        reset_audio_for_tests();
        g
    }

    #[test]
    fn open_returns_valid_handle_for_canonical_format() {
        let _g = lock_and_reset();
        let h = stream_open_impl(STREAM_OUTPUT | STREAM_SHARED, 48_000, 2, FMT_F32);
        assert_ne!(h, INVALID_STREAM);
        let (slot_idx, gen) = decode_handle(h);
        assert!((slot_idx as usize) < MAX_STREAMS);
        assert!(gen >= 1);
        assert_eq!(open_count(), 1);
        assert_eq!(last_audio_error_kind(), audio_error_code::OK);
    }

    #[test]
    fn open_rejects_invalid_arguments() {
        let _g = lock_and_reset();
        // unknown flag bit
        assert_eq!(
            stream_open_impl(0x8000_0000, 48_000, 2, FMT_F32),
            INVALID_STREAM
        );
        assert_eq!(last_audio_error_kind(), audio_error_code::INVALID_INPUT);
        // OUTPUT + INPUT conflict
        assert_eq!(
            stream_open_impl(STREAM_OUTPUT | STREAM_INPUT, 48_000, 2, FMT_F32),
            INVALID_STREAM
        );
        // SHARED + EXCLUSIVE conflict
        assert_eq!(
            stream_open_impl(STREAM_SHARED | STREAM_EXCLUSIVE, 48_000, 2, FMT_F32),
            INVALID_STREAM
        );
        // rate = 0 + rate > 768000
        assert_eq!(
            stream_open_impl(STREAM_OUTPUT, 0, 2, FMT_F32),
            INVALID_STREAM
        );
        assert_eq!(
            stream_open_impl(STREAM_OUTPUT, 1_000_000, 2, FMT_F32),
            INVALID_STREAM
        );
        // channels = 0 + channels > 8
        assert_eq!(
            stream_open_impl(STREAM_OUTPUT, 48_000, 0, FMT_F32),
            INVALID_STREAM
        );
        assert_eq!(
            stream_open_impl(STREAM_OUTPUT, 48_000, 9, FMT_F32),
            INVALID_STREAM
        );
        // unknown format
        assert_eq!(
            stream_open_impl(STREAM_OUTPUT, 48_000, 2, 0),
            INVALID_STREAM
        );
        assert_eq!(
            stream_open_impl(STREAM_OUTPUT, 48_000, 2, FMT_MAX + 1),
            INVALID_STREAM
        );
    }

    #[test]
    fn format_lut_dispatches_correctly() {
        let _g = lock_and_reset();
        assert_eq!(format_bytes_per_sample(FMT_I16), 2);
        assert_eq!(format_bytes_per_sample(FMT_I24), 3);
        assert_eq!(format_bytes_per_sample(FMT_I32), 4);
        assert_eq!(format_bytes_per_sample(FMT_F32), 4);
        assert_eq!(format_bytes_per_sample(FMT_F64), 8);
        assert_eq!(format_bytes_per_sample(0xDEAD_BEEF), 0);
        assert!(is_valid_format(FMT_F32));
        assert!(!is_valid_format(0));
        assert!(!is_valid_format(FMT_MAX + 1));
    }

    #[test]
    fn write_returns_bytes_written_and_bumps_counters() {
        let _g = lock_and_reset();
        let h = stream_open_impl(STREAM_OUTPUT, 48_000, 2, FMT_F32);
        assert_ne!(h, INVALID_STREAM);
        let buf = vec![0u8; 1024];
        // SAFETY : buf valid for 1024 bytes ; impl never derefs.
        let n = stream_write_impl(h, buf.as_ptr(), 1024);
        assert_eq!(n, 1024);
        assert_eq!(bytes_written_total(), 1024);
        // zero-len no-op
        let n0 = stream_write_impl(h, buf.as_ptr(), 0);
        assert_eq!(n0, 0);
        assert_eq!(bytes_written_total(), 1024);
    }

    #[test]
    fn write_rejects_invalid_handle() {
        let _g = lock_and_reset();
        assert_eq!(stream_write_impl(INVALID_STREAM, std::ptr::null(), 0), -1);
        assert_eq!(last_audio_error_kind(), audio_error_code::INVALID_HANDLE);
        let bogus = encode_handle(0, 9999);
        assert_eq!(stream_write_impl(bogus, std::ptr::null(), 1), -1);
        let way_out = encode_handle((MAX_STREAMS + 5) as u32, 1);
        assert_eq!(stream_write_impl(way_out, std::ptr::null(), 1), -1);
    }

    #[test]
    fn close_is_idempotent_second_call_fails() {
        let _g = lock_and_reset();
        let h = stream_open_impl(STREAM_OUTPUT, 48_000, 2, FMT_F32);
        assert_ne!(h, INVALID_STREAM);
        assert_eq!(stream_close_impl(h), 0);
        assert_eq!(close_count(), 1);
        assert_eq!(last_audio_error_kind(), audio_error_code::OK);
        // Second close fails — slot is FREE.
        assert_eq!(stream_close_impl(h), -1);
        assert_eq!(last_audio_error_kind(), audio_error_code::INVALID_HANDLE);
        assert_eq!(close_count(), 1);
    }

    #[test]
    fn mic_input_is_default_denied_per_prime_directive() {
        let _g = lock_and_reset();
        // Caps default = OUTPUT only.
        assert_eq!(audio_caps_current(), AUDIO_CAP_DEFAULT);
        assert_eq!(audio_caps_current() & AUDIO_CAP_INPUT, 0);
        // INPUT open rejected.
        let h = stream_open_impl(STREAM_INPUT, 48_000, 2, FMT_F32);
        assert_eq!(h, INVALID_STREAM);
        assert_eq!(last_audio_error_kind(), audio_error_code::INPUT_DENIED);
        assert_eq!(mic_denied_count(), 1);
        // Granting AUDIO_CAP_INPUT lets the open succeed.
        let after = audio_caps_grant(AUDIO_CAP_INPUT);
        assert_eq!(after & AUDIO_CAP_INPUT, AUDIO_CAP_INPUT);
        let h2 = stream_open_impl(STREAM_INPUT, 48_000, 2, FMT_F32);
        assert_ne!(h2, INVALID_STREAM);
        // Revoking restores default-DENIED.
        let revoked = audio_caps_revoke(AUDIO_CAP_INPUT);
        assert_eq!(revoked & AUDIO_CAP_INPUT, 0);
    }

    #[test]
    fn handle_encoding_round_trips() {
        for slot in [0u32, 1, 7, 31] {
            for gen in [1u32, 2, 100, 0xDEAD_BEEF] {
                let (s, g) = decode_handle(encode_handle(slot, gen));
                assert_eq!((s, g), (slot, gen));
            }
        }
    }

    #[test]
    fn read_rejects_output_streams() {
        let _g = lock_and_reset();
        let h = stream_open_impl(STREAM_OUTPUT, 48_000, 2, FMT_F32);
        assert_ne!(h, INVALID_STREAM);
        let mut buf = vec![0u8; 64];
        let n = stream_read_impl(h, buf.as_mut_ptr(), 64);
        assert_eq!(n, -1);
        assert_eq!(last_audio_error_kind(), audio_error_code::INVALID_INPUT);
    }

    #[test]
    fn write_rejects_input_streams() {
        let _g = lock_and_reset();
        audio_caps_grant(AUDIO_CAP_INPUT);
        let h = stream_open_impl(STREAM_INPUT, 48_000, 2, FMT_F32);
        assert_ne!(h, INVALID_STREAM);
        let buf = vec![0u8; 64];
        let n = stream_write_impl(h, buf.as_ptr(), 64);
        assert_eq!(n, -1);
        assert_eq!(last_audio_error_kind(), audio_error_code::INVALID_INPUT);
    }

    #[test]
    fn ffi_extern_surfaces_match_impl_results() {
        let _g = lock_and_reset();
        // SAFETY : pure validation paths — no raw-pointer deref.
        let h = unsafe { __cssl_audio_stream_open(STREAM_OUTPUT, 48_000, 2, FMT_F32) };
        assert_ne!(h, INVALID_STREAM);
        let buf = [0u8; 16];
        let n = unsafe { __cssl_audio_stream_write(h, buf.as_ptr(), 16) };
        assert_eq!(n, 16);
        let r = unsafe { __cssl_audio_stream_close(h) };
        assert_eq!(r, 0);
        let r2 = unsafe { __cssl_audio_stream_close(h) };
        assert_eq!(r2, -1);
    }

    #[test]
    fn slot_table_full_surfaces_when_capacity_exhausted() {
        let _g = lock_and_reset();
        let mut handles = Vec::with_capacity(MAX_STREAMS);
        for _ in 0..MAX_STREAMS {
            let h = stream_open_impl(STREAM_OUTPUT, 48_000, 2, FMT_F32);
            assert_ne!(h, INVALID_STREAM);
            handles.push(h);
        }
        let extra = stream_open_impl(STREAM_OUTPUT, 48_000, 2, FMT_F32);
        assert_eq!(extra, INVALID_STREAM);
        assert_eq!(last_audio_error_kind(), audio_error_code::SLOT_TABLE_FULL);
        for h in handles {
            stream_close_impl(h);
        }
    }
}
