//! CSSLv3 stage0 — LOCAL voice ASR pipeline.
//!
//! § T11-W18-VOICE-ASR · canonical-Rust-shim mirroring
//! `Labyrinth of Apocalypse/systems/intent_translation.csl` § VOICE-INPUT pipeline.
//!
//! § APOCKY-VISION (verbatim 2026-05-02)
//!   "I want to be able to describe things in text or voice and they
//!    crystalize from the substrate"
//!
//! § THESIS
//!   Voice utterance → phoneme-stream → HDC-bind → intent-vector →
//!   substrate-crystallization. THIS CRATE owns the voice → phoneme-stream
//!   stage. Downstream stages (HDC-bind / intent-classify / dispatch /
//!   crystallize) live in sibling crates and are wired via the
//!   `cssl-host-intent-translation` orchestrator.
//!
//! § BACKENDS
//!   - [`Backend::Mock`] — deterministic phoneme-stream derived from the
//!     capture-handle seed. Always-works · platform-independent · primary
//!     stage-0 default. Fits the "same-input-same-output" replay axiom.
//!   - [`Backend::WindowsSapi`] — Windows Speech API via the OS-level local
//!     speech-recognition service. Real-Windows-runtime only (gated by
//!     `cfg(target_os="windows")`). Stage-0 stub returns confidence=0 and
//!     defers to mock until a sibling slice wires the real SAPI dispatch
//!     (windows-crate Win32_Media_Speech feature + COM init).
//!     This avoids dragging the Win32-Speech feature-bloat into stage-0.
//!
//! § PRIME-DIRECTIVE alignment
//!   - LOCAL-only · no network egress · no cloud-ASR · no Anthropic/OpenAI/
//!     Whisper-cloud. The mock backend is by-construction local. The SAPI
//!     scaffold dispatches into the local OS service · no remote service.
//!   - Σ-mask cap-gate : `voice_capture_start(cap_token)` returns handle 0
//!     when `cap_token == 0`. Higher-level cap-revocation maps to
//!     cap-token=0 by the orchestrator.
//!   - Deterministic : mock mode is pure-fn of (cap_token, handle-counter).
//!   - Sovereign : every handle is finalize-able · returns confidence-only ·
//!     no audio bytes ever leave this process even with SAPI scaffold.
//!
//! § ABI surface (matches `intent_translation.csl` extern "C" decls)
//!   - [`voice_capture_start`]          → handle u32 (0=denied)
//!   - [`voice_capture_phoneme_count`]  → u32
//!   - [`voice_capture_phoneme_at`]     → phoneme-id u32 (255 = end-of-buffer)
//!   - [`voice_capture_finalize`]       → confidence-percent u32 (0..=100)
//!
//! § DETERMINISM CONTRACT
//!   Given identical (cap_token, handle-allocation-order) the mock backend
//!   yields bit-identical phoneme-streams across hosts. This satisfies the
//!   spec § DETERMINISM AUDIT axiom for replay-mismatch detection. The
//!   SAPI backend is non-deterministic by nature (live-audio) and is
//!   marked as such in the [`CaptureMeta::deterministic`] field.
//!
//! § FORBIDDEN BEHAVIOURS
//!   - silent mic activation                   → cap_token=0 → handle 0
//!   - audio-bytes leaving the process         → no API surface exists
//!   - external-ASR-API / cloud / Anthropic    → not a dependency
//!   - phoneme-id out-of-vocab                 → clamped to PHONEME_VOCAB_SIZE
//!   - panic on bad input                      → all paths return u32 codes

// § FFI : the four `extern "C"` decls require an `unsafe extern "C"` ABI ;
// we permit unsafe-blocks for those decls only. Body is safe-Rust ; module
// `mock` and `sapi` are forbid(unsafe_code).
#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
// Registry mutex is held for the entirety of small atomic ops by design ;
// the `let reg = registry().lock()...` binding makes the lock-scope explicit
// for review. Tightening would inline the call and harm readability.
#![allow(clippy::significant_drop_tightening)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::sync::Mutex;

pub mod mock;
pub mod sapi;

// ─────────────────────────────────────────────────────────────────────
// § canonical constants — mirror `intent_translation.csl` § HDC-VECTOR
// ─────────────────────────────────────────────────────────────────────

/// Number of distinct phoneme-ids (English ≈ 44 + extension slack).
/// Mirrors `PHONEME_VOCAB_SIZE` in `intent_translation.csl`.
pub const PHONEME_VOCAB_SIZE: u32 = 64;

/// Sentinel returned by [`voice_capture_phoneme_at`] when `idx` ≥ count.
pub const PHONEME_END_OF_BUFFER: u32 = 255;

/// Maximum phoneme-stream length per capture-handle. Bounded to keep
/// memory deterministic (no unbounded growth · ring-overwrite-safe).
/// 4096 phonemes ≈ 30s of speech at average ~10 phonemes/sec. Plenty for
/// stage-0 single-utterance flows.
pub const MAX_PHONEMES_PER_CAPTURE: u32 = 4096;

/// Cap-token value indicating denied microphone-record permission.
/// Per spec § VOICE-INPUT pipeline, the orchestrator translates a
/// revoked cap-microphone-record bit to this sentinel before calling.
pub const CAP_TOKEN_DENIED: u32 = 0;

/// Crate-version constant for scaffold verification + audit-event tagging.
pub const STAGE0_VOICE_ASR: &str = env!("CARGO_PKG_VERSION");

// ─────────────────────────────────────────────────────────────────────
// § backend selection
// ─────────────────────────────────────────────────────────────────────

/// Backend pick at runtime. Stage-0 default = [`Backend::Mock`] for
/// deterministic-replay + zero-feature-bloat. Switch via
/// [`set_backend`] before any [`voice_capture_start`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    /// Deterministic phoneme-stream from capture-handle seed. Always-works.
    #[default]
    Mock,
    /// Windows-SAPI scaffold. Real-Windows-runtime only.
    /// Stage-0 returns confidence=0 + empty stream until sibling wires COM.
    WindowsSapi,
}

// ─────────────────────────────────────────────────────────────────────
// § capture-handle registry · process-local · Σ-mask-attested
// ─────────────────────────────────────────────────────────────────────

/// Metadata associated with each open capture-handle.
#[derive(Debug, Clone)]
pub struct CaptureMeta {
    /// Cap-token recorded at start. Audit-replay can re-verify.
    pub cap_token: u32,
    /// Backend used for this capture.
    pub backend: Backend,
    /// Pre-computed deterministic phoneme-stream (mock) OR live ring (SAPI).
    pub phonemes: Vec<u32>,
    /// Whether the capture was finalized (handle invalidated).
    pub finalized: bool,
    /// Confidence-percent computed at finalize-time (0..=100).
    pub confidence: u32,
    /// True if backend produces deterministic output (mock=true · SAPI=false).
    pub deterministic: bool,
}

/// Internal registry — Mutex-guarded · process-local · no globals leak.
struct Registry {
    next_handle: u32,
    backend: Backend,
    captures: Vec<Option<CaptureMeta>>,
}

impl Registry {
    const fn new() -> Self {
        Self {
            next_handle: 1, // 0 is reserved for "denied"
            backend: Backend::Mock,
            captures: Vec::new(),
        }
    }
}

/// SAFETY-NOTE : Mutex<Registry> is `Send + Sync` ; the global is reserved
/// for the host process and serializes all FFI calls. No re-entrancy
/// hazard — handlers only mutate the registry inside the lock.
fn registry() -> &'static Mutex<Registry> {
    use std::sync::OnceLock;
    static REG: OnceLock<Mutex<Registry>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(Registry::new()))
}

/// Test-helper · resets the global registry. Tests use this to ensure
/// independence. Production code must NOT call this.
#[doc(hidden)]
pub fn _reset_registry_for_tests() {
    let mut reg = registry().lock().expect("registry mutex");
    *reg = Registry::new();
}

/// Pick the backend used for subsequent [`voice_capture_start`] calls.
/// Existing handles continue with their original backend.
pub fn set_backend(b: Backend) {
    let mut reg = registry().lock().expect("registry mutex");
    reg.backend = b;
}

/// Read the currently-configured backend.
pub fn current_backend() -> Backend {
    registry().lock().expect("registry mutex").backend
}

// ─────────────────────────────────────────────────────────────────────
// § safe-Rust API (used by FFI shims AND direct-Rust callers e.g. tests
//   and the in-process orchestrator).
// ─────────────────────────────────────────────────────────────────────

/// Open a capture-handle. Returns 0 if `cap_token == 0` (Σ-mask-denied).
/// Otherwise returns a non-zero handle that may be queried via the
/// other API surface and must be finalized to release resources.
pub fn capture_start(cap_token: u32) -> u32 {
    if cap_token == CAP_TOKEN_DENIED {
        return 0;
    }
    let mut reg = registry().lock().expect("registry mutex");
    let backend = reg.backend;
    let handle = reg.next_handle;
    // Roll over before u32::MAX to avoid wrap-to-0 (which equals "denied").
    reg.next_handle = handle.checked_add(1).filter(|&n| n != 0).unwrap_or(1);

    let (phonemes, deterministic, conf) = match backend {
        Backend::Mock => {
            let stream = mock::generate_phoneme_stream(cap_token, handle);
            let conf = mock::confidence_for(cap_token, handle);
            (stream, true, conf)
        }
        Backend::WindowsSapi => {
            // Stage-0 SAPI scaffold returns empty + 0 confidence. A sibling
            // slice wires real Win32_Media_Speech dispatch under
            // `cfg(target_os = "windows")`. Until then we explicitly do
            // NOT silently fall through to mock — caller picked SAPI.
            let stream = sapi::collect_phoneme_stream(cap_token, handle);
            let conf = sapi::confidence_for(cap_token, handle);
            (stream, false, conf)
        }
    };

    let meta = CaptureMeta {
        cap_token,
        backend,
        phonemes,
        finalized: false,
        confidence: conf,
        deterministic,
    };

    // Slot allocation : index = handle as usize ; grow vec as needed.
    let idx = handle as usize;
    if idx >= reg.captures.len() {
        reg.captures.resize(idx + 1, None);
    }
    reg.captures[idx] = Some(meta);
    handle
}

/// Number of phonemes available on `handle`. Returns 0 for invalid /
/// finalized / denied handles.
pub fn capture_phoneme_count(handle: u32) -> u32 {
    if handle == 0 {
        return 0;
    }
    let reg = registry().lock().expect("registry mutex");
    reg.captures
        .get(handle as usize)
        .and_then(|opt| opt.as_ref())
        .filter(|m| !m.finalized)
        .map_or(0, |m| u32::try_from(m.phonemes.len()).unwrap_or(u32::MAX))
}

/// Phoneme-id at `idx`. Returns [`PHONEME_END_OF_BUFFER`] (255) past the end
/// or for invalid handles. Phoneme-id is clamped into 0..PHONEME_VOCAB_SIZE.
pub fn capture_phoneme_at(handle: u32, idx: u32) -> u32 {
    if handle == 0 {
        return PHONEME_END_OF_BUFFER;
    }
    let reg = registry().lock().expect("registry mutex");
    let Some(meta) = reg
        .captures
        .get(handle as usize)
        .and_then(|opt| opt.as_ref())
        .filter(|m| !m.finalized)
    else {
        return PHONEME_END_OF_BUFFER;
    };
    let i = idx as usize;
    if i >= meta.phonemes.len() {
        return PHONEME_END_OF_BUFFER;
    }
    let raw = meta.phonemes[i];
    if raw >= PHONEME_VOCAB_SIZE {
        PHONEME_END_OF_BUFFER
    } else {
        raw
    }
}

/// Finalize `handle` · returns confidence-percent (0..=100) · invalidates
/// the handle. Subsequent queries return zeroes / EOB.
/// Invalid handle returns 0.
pub fn capture_finalize(handle: u32) -> u32 {
    if handle == 0 {
        return 0;
    }
    let mut reg = registry().lock().expect("registry mutex");
    let Some(slot) = reg.captures.get_mut(handle as usize) else {
        return 0;
    };
    let Some(meta) = slot.as_mut() else {
        return 0;
    };
    if meta.finalized {
        return 0;
    }
    meta.finalized = true;
    let conf = meta.confidence.min(100);
    // Drop the buffer to free memory ; keep meta for replay-audit.
    meta.phonemes.clear();
    meta.phonemes.shrink_to_fit();
    conf
}

// ─────────────────────────────────────────────────────────────────────
// § FFI surface · matches `intent_translation.csl` extern "C" decls
// ─────────────────────────────────────────────────────────────────────
// SAFETY : these are the canonical extern "C" exports declared in
// `Labyrinth of Apocalypse/systems/intent_translation.csl`. csslc-emitted
// callers expect the four names below with u32 → u32 ABI. We forward
// every call to the safe-Rust API above ; no raw-pointer args are used,
// no panics escape (all paths return a u32 sentinel). The functions
// take and return u32-by-value only — there is no UB surface here.

/// FFI export · cap_token=0 returns 0 (denied) ; otherwise opens handle.
#[no_mangle]
pub extern "C" fn voice_capture_start(cap_token: u32) -> u32 {
    capture_start(cap_token)
}

/// FFI export · count of phonemes on `handle`.
#[no_mangle]
pub extern "C" fn voice_capture_phoneme_count(handle: u32) -> u32 {
    capture_phoneme_count(handle)
}

/// FFI export · phoneme-id at `idx` ; returns 255 on out-of-range / invalid.
#[no_mangle]
pub extern "C" fn voice_capture_phoneme_at(handle: u32, idx: u32) -> u32 {
    capture_phoneme_at(handle, idx)
}

/// FFI export · finalize · returns confidence-percent (0..=100).
#[no_mangle]
pub extern "C" fn voice_capture_finalize(handle: u32) -> u32 {
    capture_finalize(handle)
}

// ─────────────────────────────────────────────────────────────────────
// § tests · cap-deny + determinism + bounds + finalize-invalidation +
//          σ-mask-preserved + same-input-same-output
// ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Tests share the global registry ; serialize them to avoid handle-
    /// counter race when running multi-threaded. This is the canonical
    /// pattern for global-state Rust tests.
    fn test_lock() -> &'static Mutex<()> {
        use std::sync::OnceLock;
        static M: OnceLock<Mutex<()>> = OnceLock::new();
        M.get_or_init(|| Mutex::new(()))
    }

    /// Acquire test-serialization-lock + reset registry. Returns the guard
    /// which must remain in scope for the duration of the test.
    fn fresh() -> std::sync::MutexGuard<'static, ()> {
        let guard = test_lock().lock().unwrap_or_else(|p| p.into_inner());
        _reset_registry_for_tests();
        set_backend(Backend::Mock);
        guard
    }

    /// Spec § VOICE-INPUT axiom :
    ///   t∞: voice-recognition LOCAL-only · ¬ external-ASR-API
    ///   t∞: intent-translation Σ-mask-gated · denied-cap = silent-no-translate
    /// Cap-token = 0 means cap-microphone-record is revoked. Must return 0.
    #[test]
    fn cap_token_zero_denies_capture() {
        let _g = fresh();
        let h = voice_capture_start(0);
        assert_eq!(h, 0, "Σ-mask-denied cap_token=0 must return handle 0");
        assert_eq!(voice_capture_phoneme_count(h), 0);
        assert_eq!(voice_capture_phoneme_at(h, 0), PHONEME_END_OF_BUFFER);
        assert_eq!(voice_capture_finalize(h), 0);
    }

    /// Spec § DETERMINISM AUDIT :
    ///   "Given identical (utterance · KAN-state-snapshot · seed) the
    ///    intent_dispatch result must be bit-identical across hosts/sessions."
    /// For mock backend we approximate this with cap_token = utterance-seed
    /// AND handle-allocation-order. Resetting the registry between runs
    /// gives matching handle counters → identical phoneme streams.
    #[test]
    fn mock_backend_returns_deterministic_stream() {
        let _g = fresh();
        let h1 = voice_capture_start(0xDEAD_BEEF);
        let n1 = voice_capture_phoneme_count(h1);
        let stream_a: Vec<u32> = (0..n1).map(|i| voice_capture_phoneme_at(h1, i)).collect();

        // reset within same lock → identical conditions
        _reset_registry_for_tests();
        set_backend(Backend::Mock);
        let h2 = voice_capture_start(0xDEAD_BEEF);
        let n2 = voice_capture_phoneme_count(h2);
        let stream_b: Vec<u32> = (0..n2).map(|i| voice_capture_phoneme_at(h2, i)).collect();

        assert_eq!(h1, h2, "handle counters reset to identical sequence");
        assert_eq!(stream_a, stream_b, "same input → same phoneme stream");
        assert!(!stream_a.is_empty(), "deterministic stream is non-empty");
    }

    /// `voice_capture_phoneme_count` must agree with the number of in-vocab
    /// phonemes returned by `voice_capture_phoneme_at` before EOB.
    #[test]
    fn phoneme_count_matches_iteration_until_eob() {
        let _g = fresh();
        let h = voice_capture_start(42);
        assert_ne!(h, 0);
        let count = voice_capture_phoneme_count(h);
        assert!(count > 0, "non-denied handle has at least one phoneme");

        // iterate until EOB ; count should match
        let mut iter_count = 0u32;
        let mut idx = 0u32;
        loop {
            let p = voice_capture_phoneme_at(h, idx);
            if p == PHONEME_END_OF_BUFFER {
                break;
            }
            assert!(p < PHONEME_VOCAB_SIZE, "phoneme-id in-vocab");
            iter_count += 1;
            idx += 1;
            if idx > MAX_PHONEMES_PER_CAPTURE * 2 {
                panic!("EOB sentinel never reached — buffer overrun");
            }
        }
        assert_eq!(iter_count, count, "count == iteration-until-EOB");
    }

    /// `voice_capture_finalize` must return 0..=100 confidence-percent.
    #[test]
    fn finalize_confidence_in_bounds_and_invalidates_handle() {
        let _g = fresh();
        let h = voice_capture_start(7);
        assert_ne!(h, 0);
        assert!(voice_capture_phoneme_count(h) > 0);

        let conf = voice_capture_finalize(h);
        assert!(conf <= 100, "confidence-percent ≤ 100");

        // Subsequent queries on finalized handle return zeros / EOB.
        assert_eq!(voice_capture_phoneme_count(h), 0);
        assert_eq!(voice_capture_phoneme_at(h, 0), PHONEME_END_OF_BUFFER);
        // Double-finalize returns 0 (no double-discount of confidence).
        assert_eq!(voice_capture_finalize(h), 0);
    }

    /// Σ-mask preservation : after a denied call, a subsequent allowed call
    /// still works AND a subsequent denied call still returns 0.
    /// The cap-state is per-call, never cached / never bypassed.
    #[test]
    fn sigma_mask_state_preserved_across_calls() {
        let _g = fresh();
        // denied
        assert_eq!(voice_capture_start(0), 0);
        // allowed
        let h = voice_capture_start(123);
        assert_ne!(h, 0);
        assert!(voice_capture_phoneme_count(h) > 0);
        // denied again → still 0 even though prior call succeeded
        assert_eq!(voice_capture_start(0), 0);
        // earlier handle still valid
        assert!(voice_capture_phoneme_count(h) > 0);
    }

    /// Same input (same cap_token) under identical registry-reset state
    /// produces same handle, same count, same per-index phoneme. This is
    /// the canonical "replay-determinism" property the spec requires.
    #[test]
    fn same_input_same_output_replay_contract() {
        let _g = fresh();
        let h1 = voice_capture_start(0xCAFE_BABE);
        let n = voice_capture_phoneme_count(h1);
        let conf1 = voice_capture_finalize(h1);

        // reset-and-replay within the same lock — observe deterministic re-emission
        _reset_registry_for_tests();
        set_backend(Backend::Mock);
        let h2 = voice_capture_start(0xCAFE_BABE);
        let n2 = voice_capture_phoneme_count(h2);
        let conf2 = voice_capture_finalize(h2);

        assert_eq!(h1, h2, "handle id reproducible from reset state");
        assert_eq!(n, n2, "phoneme count reproducible");
        assert_eq!(conf1, conf2, "confidence reproducible");
    }

    /// Different cap_token inputs must produce different streams (mock
    /// backend must be input-sensitive — if it ignored the seed every
    /// stream would be identical and the determinism test would pass
    /// trivially while the replay-fingerprint axiom would fail).
    #[test]
    fn different_input_different_stream() {
        let _g = fresh();
        let h1 = voice_capture_start(1);
        let s1: Vec<u32> = (0..voice_capture_phoneme_count(h1))
            .map(|i| voice_capture_phoneme_at(h1, i))
            .collect();

        _reset_registry_for_tests();
        set_backend(Backend::Mock);
        let h2 = voice_capture_start(2);
        let s2: Vec<u32> = (0..voice_capture_phoneme_count(h2))
            .map(|i| voice_capture_phoneme_at(h2, i))
            .collect();

        assert_ne!(s1, s2, "input-sensitive : distinct seeds yield distinct streams");
    }

    /// SAPI backend stage-0 returns empty + 0 confidence and does NOT
    /// silently fall back to mock. Caller picked SAPI · we honor it.
    #[test]
    fn sapi_backend_stage0_empty_no_silent_mock_fallback() {
        let _g = fresh();
        set_backend(Backend::WindowsSapi);
        let h = voice_capture_start(99);
        // Σ-mask still gates · non-zero token allowed
        assert_ne!(h, 0, "SAPI scaffold still respects cap-gate");
        // stage-0 SAPI placeholder returns empty stream
        assert_eq!(
            voice_capture_phoneme_count(h),
            0,
            "stage-0 SAPI returns empty until real wire-up"
        );
        // confidence is 0 under stub
        assert_eq!(voice_capture_finalize(h), 0);
    }

    /// Backend selector round-trips cleanly.
    #[test]
    fn backend_selector_default_and_set() {
        let _g = fresh();
        assert_eq!(current_backend(), Backend::Mock);
        set_backend(Backend::WindowsSapi);
        assert_eq!(current_backend(), Backend::WindowsSapi);
        set_backend(Backend::Mock);
        assert_eq!(current_backend(), Backend::Mock);
    }
}
