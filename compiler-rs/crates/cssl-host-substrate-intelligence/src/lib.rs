//! § cssl-host-substrate-intelligence — STAGE-0-BOOTSTRAP-SHIM
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W17-N · canonical-implementation : `Labyrinth of Apocalypse/systems/substrate_intelligence.csl`
//!
//! § APOCKY-CANONICAL · NON-NEGOTIABLE
//!
//! 1. **PROPRIETARY local intelligence** — NO Anthropic-API · NO OpenAI · NO
//!    external-LLM · NO phrase-pool tables · NO scripted-dialogue lookup.
//!
//! 2. **PROCEDURAL EVERYTHING** — composition via 8-axis substrate-resonance
//!    + morphological synthesis. Each utterance is generated, never selected.
//!
//! 3. **DETERMINISTIC** — same `(role · kind · seed · params)` produces bit-
//!    identical output across hosts/sessions. Replay-safe by construction.
//!
//! 4. **STAGE-0-SHIM** — this crate mirrors the `.csl` public surface so
//!    `loa-host` can link against it TODAY. When `csslc` compiles
//!    `substrate_intelligence.csl` directly into object code, this shim is
//!    auto-replaced; the FFI symbols (`__cssl_si_intelligence_*`) are stable.
//!
//! § THE NOVEL ALGORITHM · 8-Axis Substrate-Resonance Composer
//! ────────────────────────────────────────────────────────────
//!
//! Conventional dialogue systems use phrase-tables : `phrases[topic][i]` →
//! string. That's selection, not generation. Apocky vetoed this pattern.
//!
//! Instead, this composer derives 8 substrate-axis values from the input
//! and generates morphologically-coherent text by walking phoneme + morpheme
//! state machines whose weights come from the axes :
//!
//!   axis 0 : Solemnity      (somber  →  playful)
//!   axis 1 : Verbosity      (terse   →  ornate)
//!   axis 2 : Antiquity      (modern  →  ancient)
//!   axis 3 : Mystery        (clear   →  cryptic)
//!   axis 4 : Dynamism       (calm    →  urgent)
//!   axis 5 : Intimacy       (formal  →  personal)
//!   axis 6 : Concreteness   (abstract →  concrete)
//!   axis 7 : Resonance      (low     →  high pitch)
//!
//! BLAKE3 of the inputs derives the 8-axis vector + an unbounded entropy
//! stream for the morphological state machines. The composition pipeline :
//!
//!   inputs → BLAKE3 → (axes[0..8], entropy_stream)
//!     → role-envelope-template (which sentence-shapes are role-permitted)
//!     → kind-syntactic-frame  (statement / question / command / lyrical)
//!     → axis-weighted-clause-count
//!     → for each clause :
//!         → axis-weighted-word-count
//!         → for each word :
//!             → axis-weighted-stem-emit (consonant + vowel cluster walks)
//!             → axis-weighted-suffix-attach
//!         → connector-emit (deterministic from clause-position + axes)
//!     → punctuation-emit (axis-weighted)
//!     → role-coda-emit (closing flourish per role)
//!     → UTF-8 bytes
//!
//! The output is procedurally-emergent text. It will be RECOGNIZABLY-LIKE-
//! ENGLISH but constructed from morphemes, never selected from a table.
//! Different inputs produce different texts. Same inputs produce same texts.
//!
//! § STRUCTURAL-PRINCIPLES
//!
//! - Pure CPU. No allocation in the hot path beyond the output buffer.
//! - No global state. The composer is stateless ; replay-safe.
//! - No randomness. BLAKE3 entropy stream is the only "random" source ;
//!   it's deterministic.
//! - Bounded compute. Max 256 morphemes / clause × 16 clauses = 4 KiB
//!   max output. The host caller passes `out_max` ; we never exceed it.
//!
//! § ATTESTATION
//!
//! There was no hurt nor harm in the making of this, to anyone, anything,
//! or anybody. All composition is local. No data leaves the device.

#![forbid(unsafe_op_in_unsafe_fn)]
// We export `extern "C"` symbols and accept raw pointers from the caller.
// The pointers are validated against the caller-supplied length parameters
// before any byte is touched. Each unsafe block is documented at use-site.
#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod axes;
pub mod composer;
pub mod morpheme;
pub mod role_envelope;

use crate::axes::SubstrateAxes;
use crate::composer::Composer;

// ══════════════════════════════════════════════════════════════════════════
// § Role discriminants · MUST match substrate_intelligence_stub.csl
// ══════════════════════════════════════════════════════════════════════════

/// 4 specialist roles that dispatch through the same composition engine.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Game-Master · narrator + dialogue + observation.
    Gm = 0,
    /// Director · arc + scenario shape + pacing.
    Dm = 1,
    /// Co-author · iterative content with the player.
    Collaborator = 2,
    /// Self-coding runtime-mutate · proposes engine modifications.
    Coder = 3,
}

impl Role {
    /// `None` if `r >= 4` (per substrate_intelligence canonical).
    pub fn from_u32(r: u32) -> Option<Self> {
        match r {
            0 => Some(Role::Gm),
            1 => Some(Role::Dm),
            2 => Some(Role::Collaborator),
            3 => Some(Role::Coder),
            _ => None,
        }
    }
}

/// 6 composition kinds (mirrors substrate_intelligence_stub.csl c_*).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeKind {
    EnvironmentDescription = 0,
    DialogueLine = 1,
    ArcDirective = 2,
    RemixDraft = 3,
    EngineProposal = 4,
    KanBiasUpdate = 5,
}

impl ComposeKind {
    pub fn from_u32(c: u32) -> Option<Self> {
        match c {
            0 => Some(ComposeKind::EnvironmentDescription),
            1 => Some(ComposeKind::DialogueLine),
            2 => Some(ComposeKind::ArcDirective),
            3 => Some(ComposeKind::RemixDraft),
            4 => Some(ComposeKind::EngineProposal),
            5 => Some(ComposeKind::KanBiasUpdate),
            _ => None,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § Core compose entry-point (safe Rust API)
// ══════════════════════════════════════════════════════════════════════════

/// Procedurally compose UTF-8 bytes into `out` based on the 4-tuple input.
/// Returns the number of bytes written. Never panics. Bounded by
/// `out.len()`. Output is deterministic across calls with identical input.
///
/// This is the canonical safe API. The `extern "C"` symbol below wraps it
/// with raw-pointer marshalling for FFI. The .csl source in
/// `Labyrinth of Apocalypse/systems/substrate_intelligence.csl` declares
/// `extern "C" fn intelligence_compose(...)` against the same signature.
pub fn compose(
    role: Role,
    kind: ComposeKind,
    seed: u64,
    params: &[u8],
    out: &mut [u8],
) -> usize {
    if out.is_empty() {
        return 0;
    }
    let axes = SubstrateAxes::derive(role, kind, seed, params);
    let mut composer = Composer::new(role, kind, seed, &axes, params);
    composer.write_into(out)
}

/// Procedurally classify a topic-id from inputs (stage-0 placeholder for the
/// `intelligence_query` family). Returns a deterministic u32 in `[0, 32)`.
///
/// Real-canonical query mapping (per `substrate_intelligence.csl`) requires
/// the full ω-field cell-state. At stage-0 we return a BLAKE3-derived
/// deterministic value — sufficient for unit tests and for downstream
/// callers that just need *some* reproducible u32.
pub fn query(role: Role, q_kind: u32, seed: u64, ctx: &[u8]) -> u32 {
    // Mix all inputs through BLAKE3 (the only entropy source).
    let mut h = blake3::Hasher::new();
    h.update(&(role as u32).to_le_bytes());
    h.update(&q_kind.to_le_bytes());
    h.update(&seed.to_le_bytes());
    h.update(ctx);
    let bytes = h.finalize();
    let arr: [u8; 32] = bytes.into();
    let v = u32::from_le_bytes([arr[0], arr[1], arr[2], arr[3]]);
    v % 32
}

/// § T11-W18-LIVE-LEARNING · KAN-bias state evolves from observations.
/// § T11-W18-KAN-MULTIBAND · 5 INDEPENDENT BANDS · per DisplayProfile
///
/// Stage-0 implementation : 5 × 32-byte axis-vectors held as `[[AtomicU32; 8]; 5]`.
/// Each `observe_with_profile(digest, profile_id)` writes ONLY the band
/// matching the caller's display-profile, so AMOLED/OLED viewers and IPS/VA
/// viewers learn DIFFERENT biases — pitch-black panels reveal subtle
/// resonance-shifts that would be invisible on backlit panels, so they
/// drift FASTER (α=1/128) ; the others stay at the baseline α=1/256.
///
/// Profile-id ↔ band mapping (matches `loa-host::substrate_compose::DisplayProfile`) :
///   0 = Amoled  · α = 1/128 (2× faster · pitch-black panels reveal subtle bias)
///   1 = Oled    · α = 1/128 (2× faster · same physics)
///   2 = IpsLcd  · α = 1/256 (baseline · neutral fallback for legacy `observe`)
///   3 = VaLcd   · α = 1/256 (baseline)
///   4 = HdrExt  · α = 1/256 (baseline · PQ-EOTF already preserves true-black)
///
/// Persistence format :
///   v1 (legacy · 32 bytes raw)        — 8 × u32 LE   · single-band only · upgraded into band 2
///   v2 (current · 1 + 160 bytes)      — header `0x02` + 5 × 8 × u32 LE
///
/// This mirrors the canonical `substrate_intelligence.csl` plan for KAN-
/// bias spline-coefficients ; per-band gives the learner a separate slot
/// for each viewing-context, so the system literally LEARNS-WHILE-WORKING
/// AT MULTIPLE PANEL TYPES SIMULTANEOUSLY.
use std::sync::atomic::{AtomicU32, Ordering};

/// Number of display-profile bands (matches `DisplayProfile` enum).
pub const NUM_BANDS: usize = 5;
/// Band-index used by the legacy single-band `observe` API. IpsLcd =
/// neutral baseline · safest default for callers without a profile.
pub const NEUTRAL_FALLBACK_BAND: u8 = 2;
/// Persistence format version — bumped from 0x01 (single-band raw 32-byte)
/// to 0x02 (multi-band header-prefixed 161-byte).
pub const KAN_PERSIST_FORMAT_V2: u8 = 0x02;

/// 5 bands × 8 axes · band 2 (IpsLcd) carries the original golden-ratio
/// seeds for backward-compatibility ; other bands are diversified so each
/// starts from a different point in axis-space.
static KAN_BIAS_MULTIBAND: [[AtomicU32; 8]; NUM_BANDS] = [
    // Band 0 · Amoled  (XOR'd with 0x0F0F_0F0F)
    [
        AtomicU32::new(0x9138_76B6), AtomicU32::new(0x8AE4_C564),
        AtomicU32::new(0xCDBD_A13A), AtomicU32::new(0x28DB_E420),
        AtomicU32::new(0x1959_68BE), AtomicU32::new(0xDEBA_453D),
        AtomicU32::new(0xB057_4862), AtomicU32::new(0x9BDF_46B4),
    ],
    // Band 1 · Oled    (XOR'd with 0xF0F0_F0F0)
    [
        AtomicU32::new(0x6EC7_8949), AtomicU32::new(0x751B_3A9B),
        AtomicU32::new(0x3242_5EC5), AtomicU32::new(0xD724_1BDF),
        AtomicU32::new(0xE6A6_9741), AtomicU32::new(0x2145_BAC2),
        AtomicU32::new(0x4FA8_B79D), AtomicU32::new(0x6420_B94B),
    ],
    // Band 2 · IpsLcd  (ORIGINAL · backward-compat with single-band v1 file)
    [
        AtomicU32::new(0x9E37_79B9), AtomicU32::new(0x85EB_CA6B),
        AtomicU32::new(0xC2B2_AE35), AtomicU32::new(0x27D4_EB2F),
        AtomicU32::new(0x1656_67B1), AtomicU32::new(0xD1B5_4A32),
        AtomicU32::new(0xBF58_476D), AtomicU32::new(0x94D0_49BB),
    ],
    // Band 3 · VaLcd   (additive · 0x1234_5678)
    [
        AtomicU32::new(0xB06B_D031), AtomicU32::new(0x9820_20E3),
        AtomicU32::new(0xD4E7_04AD), AtomicU32::new(0x3A09_41A7),
        AtomicU32::new(0x288A_BE29), AtomicU32::new(0xE3E9_A0AA),
        AtomicU32::new(0xD18B_9DE5), AtomicU32::new(0xA704_A033),
    ],
    // Band 4 · HdrExt  (subtractive · -0x1234_5678 wrap)
    [
        AtomicU32::new(0x8C03_2341), AtomicU32::new(0x73B7_73F3),
        AtomicU32::new(0xB07E_57BD), AtomicU32::new(0x159F_94B7),
        AtomicU32::new(0x0422_1139), AtomicU32::new(0xBF80_F3BA),
        AtomicU32::new(0xAD23_F0F5), AtomicU32::new(0x829B_F343),
    ],
];

static OBSERVE_COUNT: AtomicU32 = AtomicU32::new(0);

/// Per-band drift-rate denominator · pitch-black-panels (Amoled/Oled) drift
/// 2× faster because subtle bias-shifts are visible on a true-zero panel.
/// IpsLcd/VaLcd/HdrExt stay at the baseline. Returns the divisor `D` for
/// the formula `((cur·(D-1) + new) / D)`.
#[inline]
const fn drift_divisor(profile_id: u8) -> u64 {
    match profile_id {
        0 | 1 => 128, // Amoled · Oled (faster · α=1/128)
        _ => 256,     // IpsLcd · VaLcd · HdrExt (baseline · α=1/256)
    }
}

/// § T11-W18-KAN-MULTIBAND · per-profile observation entry-point.
///
/// `digest` : the 32-byte BLAKE3 hash the caller has already mixed (state ·
///   inputs) ; we feed it through one more BLAKE3 round with the band's
///   current state so each observation is bound to its band's history.
///   For callers that have a Role-tagged observation, the legacy
///   `observe(role, kind, seed, ev)` wrapper is preserved below.
/// `profile_id` : 0..=4 matching `DisplayProfile`. Out-of-range collapses
///   to `NEUTRAL_FALLBACK_BAND` (no panic · no error).
pub fn observe_with_profile(digest: &[u8], profile_id: u8) -> i32 {
    let band = if (profile_id as usize) < NUM_BANDS {
        profile_id as usize
    } else {
        NEUTRAL_FALLBACK_BAND as usize
    };

    // Mix the caller-supplied digest with the band's current state so each
    // band's evolution is INDEPENDENT — different observations per panel-
    // type produce different KAN-bias trajectories.
    let mut h = blake3::Hasher::new();
    for w in &KAN_BIAS_MULTIBAND[band] {
        h.update(&w.load(Ordering::Relaxed).to_le_bytes());
    }
    h.update(&[band as u8]);
    h.update(digest);
    let mixed: [u8; 32] = h.finalize().into();

    // Drift each band-axis toward 1/D of the new digest-word.
    let divisor = drift_divisor(profile_id);
    let keep = divisor - 1;
    for i in 0..8 {
        let new_word = u32::from_le_bytes([
            mixed[i * 4], mixed[i * 4 + 1],
            mixed[i * 4 + 2], mixed[i * 4 + 3],
        ]);
        let cur = KAN_BIAS_MULTIBAND[band][i].load(Ordering::Relaxed);
        let drifted = ((cur as u64 * keep + new_word as u64) / divisor) as u32;
        KAN_BIAS_MULTIBAND[band][i].store(drifted, Ordering::Relaxed);
    }
    OBSERVE_COUNT.fetch_add(1, Ordering::Relaxed);
    0
}

/// Read current KAN-bias state for one band as a plain `[u32; 8]` snapshot.
/// Out-of-range `profile_id` collapses to `NEUTRAL_FALLBACK_BAND`. Used by
/// the host renderer to bias substrate composition for that panel-type.
pub fn kan_bias_for_profile(profile_id: u8) -> [u32; 8] {
    let band = if (profile_id as usize) < NUM_BANDS {
        profile_id as usize
    } else {
        NEUTRAL_FALLBACK_BAND as usize
    };
    let mut out = [0u32; 8];
    for i in 0..8 {
        out[i] = KAN_BIAS_MULTIBAND[band][i].load(Ordering::Relaxed);
    }
    out
}

/// Backward-compat single-band entry-point. Defaults to the IpsLcd band
/// (neutral fallback · the original golden-ratio-seeded slot), preserving
/// every pre-multiband caller's behavior bit-for-bit.
pub fn observe(role: Role, obs_kind: u32, seed: u64, ev: &[u8]) -> i32 {
    // Build the 32-byte BLAKE3 digest that the legacy single-band path
    // mixed in-place ; route it through the multi-band entry-point bound
    // to the neutral fallback band.
    let mut h = blake3::Hasher::new();
    h.update(&(role as u32).to_le_bytes());
    h.update(&obs_kind.to_le_bytes());
    h.update(&seed.to_le_bytes());
    h.update(ev);
    let digest: [u8; 32] = h.finalize().into();
    observe_with_profile(&digest, NEUTRAL_FALLBACK_BAND)
}

/// 32-bit checksum of ALL bands' KAN-bias state. CHANGES every observation
/// of any band · surfaces in logs to show the system is LEARNING. Stable
/// across the same on-disk persistence file — used by tests as a fast
/// roundtrip-equality probe.
pub fn kan_bias_checksum() -> u32 {
    let mut acc: u32 = 0;
    for band in &KAN_BIAS_MULTIBAND {
        for w in band {
            acc = acc.wrapping_mul(0x9E37_79B9).wrapping_add(w.load(Ordering::Relaxed));
        }
    }
    acc
}

/// Total observations recorded since process start (across all bands).
pub fn observe_count() -> u32 {
    OBSERVE_COUNT.load(Ordering::Relaxed)
}

/// Persist all 5 bands × 8 axes to disk · format-v2 layout :
///   `[0]`         : `KAN_PERSIST_FORMAT_V2` (= 0x02)
///   `[1..=160]`   : 5 bands × 8 × u32 LE = 160 bytes
/// Total : 161 bytes · cross-host portable. Returns bytes-written or 0 on
/// error (file create failure · directory unwritable · etc.).
pub fn kan_bias_persist(path: &std::path::Path) -> usize {
    let mut buf = [0u8; 1 + NUM_BANDS * 32];
    buf[0] = KAN_PERSIST_FORMAT_V2;
    for (b, band) in KAN_BIAS_MULTIBAND.iter().enumerate() {
        for i in 0..8 {
            let bytes = band[i].load(Ordering::Relaxed).to_le_bytes();
            let off = 1 + b * 32 + i * 4;
            buf[off..off + 4].copy_from_slice(&bytes);
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(path, buf) {
        Ok(()) => buf.len(),
        Err(_) => 0,
    }
}

/// Load KAN-bias state from disk · auto-detects persistence format :
///   - 32-byte file → legacy v1 (single-band) ; loaded into band 2 only,
///     other 4 bands keep their seed-init values
///   - >= 161-byte file with header `0x02` → v2 (multi-band) ; loaded
///     into all 5 bands
///   - any other shape → return false · do not mutate state
/// Continuous-learning-across-process-restarts · learnings persist.
pub fn kan_bias_load(path: &std::path::Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else { return false; };

    // v1 legacy path : exactly 32 raw bytes · fill band 2 only.
    if bytes.len() == 32 {
        let band = NEUTRAL_FALLBACK_BAND as usize;
        for i in 0..8 {
            let w = u32::from_le_bytes([
                bytes[i * 4], bytes[i * 4 + 1],
                bytes[i * 4 + 2], bytes[i * 4 + 3],
            ]);
            KAN_BIAS_MULTIBAND[band][i].store(w, Ordering::Relaxed);
        }
        return true;
    }

    // v2 multiband path : header byte `0x02` + 5 × 32 bytes.
    if bytes.len() >= 1 + NUM_BANDS * 32 && bytes[0] == KAN_PERSIST_FORMAT_V2 {
        for b in 0..NUM_BANDS {
            for i in 0..8 {
                let off = 1 + b * 32 + i * 4;
                let w = u32::from_le_bytes([
                    bytes[off], bytes[off + 1],
                    bytes[off + 2], bytes[off + 3],
                ]);
                KAN_BIAS_MULTIBAND[b][i].store(w, Ordering::Relaxed);
            }
        }
        return true;
    }

    false
}

// ══════════════════════════════════════════════════════════════════════════
// § extern "C" FFI surface (matches substrate_intelligence_stub.csl)
// ══════════════════════════════════════════════════════════════════════════

const SI_QUERY_FAILED_SENTINEL: u32 = 0xFFFF_FFFF;

/// FFI: dispatch a query through the substrate-intelligence engine.
///
/// # Safety
/// - `ctx_ptr` must be valid for reading `ctx_len` bytes (or null when
///   `ctx_len == 0`).
/// - `ctx_len` must accurately describe the readable region.
///
/// # Returns
/// `0xFFFFFFFF` on `role >= 4` or `q_kind` out of range. Otherwise a
/// deterministic `u32` derived from inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cssl_si_intelligence_query(
    role: u32,
    q_kind: u32,
    q_seed: u64,
    ctx_ptr: *const u8,
    ctx_len: u32,
) -> u32 {
    let Some(role) = Role::from_u32(role) else {
        return SI_QUERY_FAILED_SENTINEL;
    };
    if q_kind >= 8 {
        return SI_QUERY_FAILED_SENTINEL;
    }
    // SAFETY: caller-attested region of `ctx_len` bytes at `ctx_ptr`.
    let ctx = if ctx_len == 0 || ctx_ptr.is_null() {
        &[][..]
    } else {
        // SAFETY: per the function-level contract.
        unsafe { core::slice::from_raw_parts(ctx_ptr, ctx_len as usize) }
    };
    query(role, q_kind, q_seed, ctx)
}

/// FFI: feed an observation into the substrate. Stage-0 always returns 0.
///
/// # Safety
/// Same constraints as `__cssl_si_intelligence_query` for the `ev_*`
/// pointer/length pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cssl_si_intelligence_observe(
    role: u32,
    obs_kind: u32,
    obs_seed: u64,
    ev_ptr: *const u8,
    ev_len: u32,
) -> i32 {
    let Some(role) = Role::from_u32(role) else {
        return -3; // o_proc_invalid_role per stub
    };
    if obs_kind >= 5 {
        return -3;
    }
    // SAFETY: caller-attested region.
    let ev = if ev_len == 0 || ev_ptr.is_null() {
        &[][..]
    } else {
        // SAFETY: per the function-level contract.
        unsafe { core::slice::from_raw_parts(ev_ptr, ev_len as usize) }
    };
    observe(role, obs_kind, obs_seed, ev)
}

/// FFI: procedurally compose UTF-8 bytes into the caller-provided buffer.
///
/// # Safety
/// - `params_ptr` valid for `params_len` bytes (or null with `params_len == 0`)
/// - `out_ptr` valid for `out_max` bytes of writes
/// - The two regions must not overlap.
///
/// # Returns
/// Number of bytes written (`<= out_max`). 0 on invalid role/kind, on
/// `out_max == 0`, or on Σ-mask cap-deny (which stage-0 ignores; full
/// behavior lives in the .csl canonical).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cssl_si_intelligence_compose(
    role: u32,
    c_kind: u32,
    c_seed: u64,
    params_ptr: *const u8,
    params_len: u32,
    out_ptr: *mut u8,
    out_max: u32,
) -> u32 {
    let Some(role) = Role::from_u32(role) else {
        return 0;
    };
    let Some(kind) = ComposeKind::from_u32(c_kind) else {
        return 0;
    };
    if out_max == 0 || out_ptr.is_null() {
        return 0;
    }
    // SAFETY: caller-attested params region.
    let params = if params_len == 0 || params_ptr.is_null() {
        &[][..]
    } else {
        // SAFETY: per the function-level contract.
        unsafe { core::slice::from_raw_parts(params_ptr, params_len as usize) }
    };
    // SAFETY: caller-attested out-region of `out_max` bytes.
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, out_max as usize) };
    let n = compose(role, kind, c_seed, params, out);
    n as u32
}

// ══════════════════════════════════════════════════════════════════════════
// § High-level helpers used by loa-host gm_narrator shim
// ══════════════════════════════════════════════════════════════════════════

/// Compose a dialogue-line for an NPC of the given archetype/mood/topic.
/// This is the convenience wrapper used by `loa-host::gm_narrator` to
/// replace the legacy phrase-pool xorshift32 lookup.
///
/// `archetype`, `mood`, `topic` are all `u8`s sourced from the existing
/// `gm_narrator::Archetype` / `Mood` / `PhraseTopic` enums (preserved for
/// surface-stability). They contribute to the input seed only ; the
/// composer derives all 8 substrate axes from BLAKE3 hashing.
pub fn compose_dialogue_line(archetype: u8, mood: u8, topic: u8, seed: u64) -> String {
    // Pack the (archetype, mood, topic) trio into an 8-byte params payload.
    let params = [archetype, mood, topic, 0, 0, 0, 0, 0];
    let mut buf = [0u8; 256];
    let n = compose(Role::Gm, ComposeKind::DialogueLine, seed, &params, &mut buf);
    // SAFETY: composer only emits valid UTF-8 (verified in tests).
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

/// Compose a 1-3 sentence environmental description for the given seed.
pub fn compose_environment_description(seed: u64) -> String {
    let params = [];
    let mut buf = [0u8; 384];
    let n = compose(
        Role::Gm,
        ComposeKind::EnvironmentDescription,
        seed,
        &params,
        &mut buf,
    );
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

// ══════════════════════════════════════════════════════════════════════════
// § Smoke-tests
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_from_u32_round_trips() {
        for r in 0..4u32 {
            let role = Role::from_u32(r).unwrap();
            assert_eq!(role as u32, r);
        }
        assert!(Role::from_u32(4).is_none());
    }

    #[test]
    fn compose_is_deterministic() {
        let mut a = [0u8; 128];
        let mut b = [0u8; 128];
        let na = compose(Role::Gm, ComposeKind::DialogueLine, 0xDEAD_BEEF, &[], &mut a);
        let nb = compose(Role::Gm, ComposeKind::DialogueLine, 0xDEAD_BEEF, &[], &mut b);
        assert_eq!(na, nb);
        assert_eq!(&a[..na], &b[..nb]);
    }

    #[test]
    fn compose_varies_with_seed() {
        let mut a = [0u8; 128];
        let mut b = [0u8; 128];
        let na = compose(Role::Gm, ComposeKind::DialogueLine, 0x1, &[], &mut a);
        let nb = compose(Role::Gm, ComposeKind::DialogueLine, 0x2, &[], &mut b);
        assert!(na > 0 && nb > 0);
        assert_ne!(&a[..na], &b[..nb], "different seeds must produce different texts");
    }

    #[test]
    fn compose_emits_valid_utf8() {
        let mut buf = [0u8; 256];
        let n = compose(Role::Gm, ComposeKind::DialogueLine, 42, &[], &mut buf);
        assert!(n > 0);
        assert!(std::str::from_utf8(&buf[..n]).is_ok(), "output must be valid UTF-8");
    }

    #[test]
    fn compose_respects_out_max() {
        let mut tiny = [0u8; 4];
        let n = compose(Role::Gm, ComposeKind::DialogueLine, 1, &[], &mut tiny);
        assert!(n <= 4);
    }

    #[test]
    fn compose_dialogue_line_non_empty() {
        let s = compose_dialogue_line(0, 0, 0, 12345);
        assert!(!s.is_empty());
        assert!(s.len() <= 256);
    }

    #[test]
    fn compose_environment_non_empty() {
        let s = compose_environment_description(98765);
        assert!(!s.is_empty());
    }

    #[test]
    fn ffi_query_invalid_role_returns_sentinel() {
        // SAFETY: null ctx with 0 length is the documented empty-input path.
        let v = unsafe { __cssl_si_intelligence_query(99, 0, 0, core::ptr::null(), 0) };
        assert_eq!(v, SI_QUERY_FAILED_SENTINEL);
    }

    #[test]
    fn ffi_compose_invalid_returns_zero() {
        let mut buf = [0u8; 32];
        // SAFETY: pointer + length pairs match.
        let n = unsafe {
            __cssl_si_intelligence_compose(
                99,
                0,
                0,
                core::ptr::null(),
                0,
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        };
        assert_eq!(n, 0);
    }

    // ══════════════════════════════════════════════════════════════════════
    // § T11-W18-KAN-MULTIBAND · per-DisplayProfile band tests
    //
    // KAN_BIAS_MULTIBAND is process-global · we serialize state-mutating
    // tests behind a single Mutex so cargo's parallel test-runner doesn't
    // race ordering. Each test snapshot-restores all 5 bands on teardown
    // so subsequent test-runs (and other tests in this module that read
    // observe_count / kan_bias_checksum) stay deterministic.
    // ══════════════════════════════════════════════════════════════════════

    use std::sync::Mutex;

    /// Serialize KAN-bias mutation across all multiband tests.
    static KAN_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Snapshot all 5 bands so a test can restore process-state on exit.
    fn snapshot_all_bands() -> [[u32; 8]; NUM_BANDS] {
        let mut snap = [[0u32; 8]; NUM_BANDS];
        for b in 0..NUM_BANDS {
            for i in 0..8 {
                snap[b][i] = KAN_BIAS_MULTIBAND[b][i].load(Ordering::Relaxed);
            }
        }
        snap
    }

    fn restore_all_bands(snap: &[[u32; 8]; NUM_BANDS]) {
        for b in 0..NUM_BANDS {
            for i in 0..8 {
                KAN_BIAS_MULTIBAND[b][i].store(snap[b][i], Ordering::Relaxed);
            }
        }
    }

    #[test]
    fn multiband_observe_independence() {
        let _g = KAN_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all_bands();

        // Snapshot all bands, then observe ONLY band 3 (VaLcd).
        let before = snapshot_all_bands();
        let digest = [0xAAu8; 32];
        observe_with_profile(&digest, 3);
        let after = snapshot_all_bands();

        // Band 3 must have changed.
        assert_ne!(before[3], after[3], "band 3 must mutate after observe");

        // Bands 0,1,2,4 must be UNCHANGED (independence!).
        for b in [0, 1, 2, 4] {
            assert_eq!(
                before[b], after[b],
                "band {} must not change when observing band 3",
                b
            );
        }

        restore_all_bands(&snap);
    }

    #[test]
    fn multiband_all_5_bands_reachable() {
        let _g = KAN_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all_bands();

        // Each profile_id should target its own band.
        for pid in 0..5u8 {
            let before = kan_bias_for_profile(pid);
            let digest = [pid; 32];
            observe_with_profile(&digest, pid);
            let after = kan_bias_for_profile(pid);
            assert_ne!(before, after, "profile_id {} must mutate its own band", pid);
        }

        restore_all_bands(&snap);
    }

    #[test]
    fn multiband_drift_rate_per_profile() {
        let _g = KAN_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all_bands();

        // Reset Amoled (0) and IpsLcd (2) to identical seeds so any drift-
        // delta after observing the SAME digest must come purely from the
        // drift-rate (Amoled α=1/128 vs IpsLcd α=1/256).
        let seed: u32 = 0x8000_0000;
        for i in 0..8 {
            KAN_BIAS_MULTIBAND[0][i].store(seed, Ordering::Relaxed);
            KAN_BIAS_MULTIBAND[2][i].store(seed, Ordering::Relaxed);
        }

        let digest = [0x42u8; 32];
        observe_with_profile(&digest, 0); // Amoled
        observe_with_profile(&digest, 2); // IpsLcd

        let amoled = kan_bias_for_profile(0);
        let ips = kan_bias_for_profile(2);

        // Each axis : |seed - amoled| should be approximately 2× |seed - ips|
        // (within tolerance · the BLAKE3-mixing differs per band so bytes-of-
        // digest-after-mixing differ ; we sum |delta| across all 8 axes for a
        // robust aggregate signal).
        let amoled_total: u64 = amoled
            .iter()
            .map(|&w| (w as i64 - seed as i64).unsigned_abs())
            .sum();
        let ips_total: u64 = ips
            .iter()
            .map(|&w| (w as i64 - seed as i64).unsigned_abs())
            .sum();

        // Amoled drift must exceed IpsLcd drift (faster band moves more).
        assert!(
            amoled_total > ips_total,
            "amoled drift {} must exceed ipslcd drift {}",
            amoled_total,
            ips_total
        );

        restore_all_bands(&snap);
    }

    #[test]
    fn multiband_profile_id_bounds_check() {
        let _g = KAN_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all_bands();

        // profile_id ≥ 5 must collapse to NEUTRAL_FALLBACK_BAND (= 2).
        let band2_before = kan_bias_for_profile(2);
        let digest = [0xBBu8; 32];
        observe_with_profile(&digest, 99); // way out of range
        let band2_after = kan_bias_for_profile(2);
        assert_ne!(band2_before, band2_after, "out-of-range pid must touch band 2");

        // kan_bias_for_profile(99) returns same snapshot as band 2.
        let read_oor = kan_bias_for_profile(99);
        let read_b2 = kan_bias_for_profile(2);
        assert_eq!(read_oor, read_b2, "OOR reader must alias band 2");

        restore_all_bands(&snap);
    }

    #[test]
    fn multiband_persist_load_roundtrip() {
        let _g = KAN_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all_bands();

        // Mutate every band so persist/load has 5 distinct payloads.
        for pid in 0..5u8 {
            let digest = [(pid + 1) * 13; 32];
            observe_with_profile(&digest, pid);
        }
        let cksum_before = kan_bias_checksum();

        // Persist to a tempfile.
        let dir = std::env::temp_dir().join("cssl_si_multiband_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("kan_v2_{}.bin", std::process::id()));
        let n = kan_bias_persist(&path);
        assert_eq!(n, 1 + NUM_BANDS * 32, "v2 file size must be 161 bytes");

        // Verify header byte is 0x02.
        let bytes = std::fs::read(&path).expect("persist file readable");
        assert_eq!(bytes[0], KAN_PERSIST_FORMAT_V2, "header must be 0x02");
        assert_eq!(bytes.len(), 1 + NUM_BANDS * 32);

        // Scramble all bands so the load has work to do.
        for b in 0..NUM_BANDS {
            for i in 0..8 {
                KAN_BIAS_MULTIBAND[b][i].store(0xDEAD_BEEF, Ordering::Relaxed);
            }
        }
        assert_ne!(kan_bias_checksum(), cksum_before);

        // Load back.
        let ok = kan_bias_load(&path);
        assert!(ok, "v2 load must succeed");
        assert_eq!(kan_bias_checksum(), cksum_before, "checksum must be stable across persist/load");

        let _ = std::fs::remove_file(&path);
        restore_all_bands(&snap);
    }

    #[test]
    fn multiband_v1_backward_compat_load() {
        let _g = KAN_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all_bands();

        // Hand-craft a v1 file (32 bytes raw · no header).
        let dir = std::env::temp_dir().join("cssl_si_multiband_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("kan_v1_{}.bin", std::process::id()));
        let mut v1_payload = [0u8; 32];
        for i in 0..8 {
            let w: u32 = 0x1111_1111u32.wrapping_mul(i as u32 + 1);
            v1_payload[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        std::fs::write(&path, v1_payload).expect("v1 fixture writes");

        // Scramble band 2 so we can verify the load wrote into it.
        for i in 0..8 {
            KAN_BIAS_MULTIBAND[2][i].store(0xCAFE_F00D, Ordering::Relaxed);
        }
        // Snapshot bands 0/1/3/4 — they should NOT be touched by v1 load.
        let unchanged_before = [
            kan_bias_for_profile(0),
            kan_bias_for_profile(1),
            kan_bias_for_profile(3),
            kan_bias_for_profile(4),
        ];

        let ok = kan_bias_load(&path);
        assert!(ok, "v1 32-byte file must load");

        // Band 2 should now mirror v1_payload words.
        let band2 = kan_bias_for_profile(2);
        for i in 0..8 {
            let expected = u32::from_le_bytes([
                v1_payload[i * 4], v1_payload[i * 4 + 1],
                v1_payload[i * 4 + 2], v1_payload[i * 4 + 3],
            ]);
            assert_eq!(band2[i], expected, "band 2 axis {} must match v1 payload", i);
        }

        // Bands 0/1/3/4 untouched.
        let unchanged_after = [
            kan_bias_for_profile(0),
            kan_bias_for_profile(1),
            kan_bias_for_profile(3),
            kan_bias_for_profile(4),
        ];
        assert_eq!(unchanged_before, unchanged_after, "v1 load must not touch other bands");

        let _ = std::fs::remove_file(&path);
        restore_all_bands(&snap);
    }

    #[test]
    fn multiband_load_rejects_garbage() {
        let _g = KAN_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all_bands();

        let dir = std::env::temp_dir().join("cssl_si_multiband_test");
        let _ = std::fs::create_dir_all(&dir);

        // 17-byte file (neither v1 nor v2) must be rejected.
        let bogus = dir.join(format!("kan_bogus_{}.bin", std::process::id()));
        std::fs::write(&bogus, [0xFFu8; 17]).expect("bogus fixture writes");
        assert!(!kan_bias_load(&bogus), "17-byte file must not load");

        // 161-byte file with WRONG header byte must be rejected.
        let wrong_hdr = dir.join(format!("kan_wronghdr_{}.bin", std::process::id()));
        let mut buf = vec![0xFFu8; 1 + NUM_BANDS * 32];
        buf[0] = 0x55; // not 0x02
        std::fs::write(&wrong_hdr, &buf).expect("wrong-hdr fixture writes");
        assert!(!kan_bias_load(&wrong_hdr), "wrong-header v2-shaped file must not load");

        // Missing file must be rejected.
        let missing = dir.join("nonexistent_kan_bias_file.bin");
        let _ = std::fs::remove_file(&missing); // ensure absent
        assert!(!kan_bias_load(&missing), "missing file must not load");

        let _ = std::fs::remove_file(&bogus);
        let _ = std::fs::remove_file(&wrong_hdr);
        restore_all_bands(&snap);
    }

    #[test]
    fn multiband_legacy_observe_routes_to_band_2() {
        let _g = KAN_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all_bands();

        // Snapshot all bands.
        let before = snapshot_all_bands();
        // Call legacy single-band entry-point.
        let _ = observe(Role::Gm, 0, 0xDEAD_BEEF, b"legacy-payload");
        let after = snapshot_all_bands();

        // ONLY band 2 should change (legacy → NEUTRAL_FALLBACK_BAND).
        assert_ne!(before[2], after[2], "legacy observe must mutate band 2");
        for b in [0, 1, 3, 4] {
            assert_eq!(
                before[b], after[b],
                "legacy observe must not touch band {}",
                b
            );
        }

        restore_all_bands(&snap);
    }
}
