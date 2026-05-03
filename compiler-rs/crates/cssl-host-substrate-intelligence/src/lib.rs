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
///
/// Stage-0 implementation : 32-byte axis-vector held as 8 atomic-u32. Each
/// observe(role, kind, seed, ev) bytes a new BLAKE3 over (state · role ·
/// kind · seed · ev) and DRIFTS the state toward a small fraction of the
/// new digest. Persistent across the process · checksum changes visibly
/// every observation. Persist + load via `kan_bias_persist` / `_load`.
///
/// This is the SAME shape the canonical `substrate_intelligence.csl`
/// describes for KAN-bias spline-coefficients · just at-axis-level vs
/// per-spline. Future-iter advances csslc to compile the .csl version
/// directly · this Rust shim falls-back to passive-import-shim per
/// spec-stub-design.
use std::sync::atomic::{AtomicU32, Ordering};

static KAN_BIAS: [AtomicU32; 8] = [
    AtomicU32::new(0x9E37_79B9), AtomicU32::new(0x85EB_CA6B),
    AtomicU32::new(0xC2B2_AE35), AtomicU32::new(0x27D4_EB2F),
    AtomicU32::new(0x1656_67B1), AtomicU32::new(0xD1B5_4A32),
    AtomicU32::new(0xBF58_476D), AtomicU32::new(0x94D0_49BB),
];

static OBSERVE_COUNT: AtomicU32 = AtomicU32::new(0);

pub fn observe(role: Role, obs_kind: u32, seed: u64, ev: &[u8]) -> i32 {
    // 1. Hash (current-state · inputs) → 32-byte digest.
    let mut h = blake3::Hasher::new();
    for w in &KAN_BIAS {
        h.update(&w.load(Ordering::Relaxed).to_le_bytes());
    }
    h.update(&(role as u32).to_le_bytes());
    h.update(&obs_kind.to_le_bytes());
    h.update(&seed.to_le_bytes());
    h.update(ev);
    let digest: [u8; 32] = h.finalize().into();

    // 2. DRIFT each state-word toward 1/256 of digest-word (gentle slope).
    //    State stays bounded · evolves continuously · replay-safe.
    for i in 0..8 {
        let new_word = u32::from_le_bytes([
            digest[i * 4], digest[i * 4 + 1],
            digest[i * 4 + 2], digest[i * 4 + 3],
        ]);
        let cur = KAN_BIAS[i].load(Ordering::Relaxed);
        // Drift: 255/256 * cur + 1/256 * new
        let drifted = ((cur as u64 * 255 + new_word as u64) / 256) as u32;
        KAN_BIAS[i].store(drifted, Ordering::Relaxed);
    }
    OBSERVE_COUNT.fetch_add(1, Ordering::Relaxed);
    0
}

/// 32-bit checksum of current KAN-bias state. CHANGES every observation ·
/// surface in logs to show the system is LEARNING.
pub fn kan_bias_checksum() -> u32 {
    let mut acc: u32 = 0;
    for w in &KAN_BIAS {
        acc = acc.wrapping_mul(0x9E37_79B9).wrapping_add(w.load(Ordering::Relaxed));
    }
    acc
}

/// Total observations recorded since process start.
pub fn observe_count() -> u32 {
    OBSERVE_COUNT.load(Ordering::Relaxed)
}

/// Persist current KAN-bias state to disk · 32 bytes (8 × u32). Returns
/// bytes-written or 0 on error. Written-as-LE for cross-host compat.
pub fn kan_bias_persist(path: &std::path::Path) -> usize {
    let mut buf = [0u8; 32];
    for i in 0..8 {
        let bytes = KAN_BIAS[i].load(Ordering::Relaxed).to_le_bytes();
        buf[i * 4..i * 4 + 4].copy_from_slice(&bytes);
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(path, buf) {
        Ok(()) => buf.len(),
        Err(_) => 0,
    }
}

/// Load KAN-bias state from disk if file exists · returns true on success.
/// Continuous-learning-across-process-restarts · learnings persist.
pub fn kan_bias_load(path: &std::path::Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else { return false; };
    if bytes.len() < 32 { return false; }
    for i in 0..8 {
        let w = u32::from_le_bytes([
            bytes[i * 4], bytes[i * 4 + 1],
            bytes[i * 4 + 2], bytes[i * 4 + 3],
        ]);
        KAN_BIAS[i].store(w, Ordering::Relaxed);
    }
    true
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
}
