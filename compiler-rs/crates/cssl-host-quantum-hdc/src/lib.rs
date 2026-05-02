//! § cssl-host-quantum-hdc — Quantum ℂ-amplitude hyperdimensional computing
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Exotic-extension to the binary-HDC stack at `cssl-hdc` and
//!   `cssl-host-crystallization::hdc`. Every component is a
//!   complex-number `(amp, phase)` rather than a single bit. Bind becomes
//!   complex-multiplication, bundle becomes vector-sum, permute becomes
//!   phase-rotation, and a new operation `interfere()` exposes
//!   constructive / destructive interference patterns directly.
//!
//! § APOCKY-DIRECTIVE (verbatim · T11-W19-A)
//!   "Are you thinking inventive and exotic and dimensionally expansive
//!    and QUANTUM enough? Let's really push the silicon with cssl!!!"
//!
//! § SPEC ANCHOR
//!   - `Labyrinth of Apocalypse/systems/quantum_hdc.csl` — canonical
//!     CSSLv3 spec for this crate. Equations + axioms + integration
//!     points all live there. This Rust crate is the stage-0 host
//!     surface that the .csl source compiles to under
//!     `feedback_loa_v13_must_be_cssl_source` discipline.
//!   - Plate "Holographic Reduced Representations" 1995 — historical
//!     genesis of complex-amplitude VSA.
//!   - Kanerva, "Hyperdimensional Computing" Cogn. Comput. 2009 —
//!     binary VSA reference this crate extends.
//!   - VSA survey, arXiv:2111.06077 — modern landscape.
//!
//! § WHY ℂ-HDC ≠ BINARY-HDC
//!   Binary-HDC bundle is majority-vote — phase-blind, lossy when many
//!   vectors superpose, cannot show interference. ℂ-HDC bundle is
//!   vector-sum in the complex plane — phases that align reinforce,
//!   phases that oppose cancel. This gives :
//!   - **interference patterns** : two crystals enhance or cancel based
//!     on their phase relationship — observable to the consumer via
//!     `interfere()` and amplitude-magnitudes that go > 1 (constructive)
//!     or → 0 (destructive).
//!   - **coherence vs decoherence** : `coherence(a, b)` returns 1.0 for
//!     phase-aligned vectors and ≈ 1/√D for phase-randomized vectors —
//!     a continuous coherence measure rather than a binary
//!     "match/no-match" Hamming threshold.
//!   - **standing-wave signatures** : substrate-resonance can be
//!     detected by `coherence` between an ω-field cell tag and a
//!     reference vector — high coherence = the cell sustains a
//!     coherent semantic resonance, low coherence = decoherent
//!     background.
//!   - **deterministic quantum-style superposition** without any actual
//!     quantum hardware — just complex-arithmetic on standard SIMD-FMA
//!     silicon. Reproducible, debuggable, sovereign.
//!
//! § WIDTH
//!   `CHDC_DIM = 256` matches the stage-0 binary HDC width at
//!   `cssl-host-crystallization::hdc::HdcVec256`. This makes a future
//!   `binary → complex` adapter a straight 1-bit-per-component map.
//!   The 256-component form is 2 KiB per vector (2 × `[f32; 256]`),
//!   comfortably L1-resident.
//!
//! § DETERMINISM
//!   - All operations are pure functions of inputs.
//!   - `derive_from_blake3` uses the BLAKE3 XOF stream — same seed ⇒
//!     same vector ∀ hosts.
//!   - No `thread_rng`, no `SystemTime`, no `getrandom`.
//!   - IEEE-754 single-precision : same target ABI ⇒ same bits. Tests
//!     use ε-tolerance ≤ 1e-4 to cover sum-order drift if compilers
//!     reorder.
//!
//! § PERFORMANCE
//!   Per-component f32 loops over 256 components compile to AVX2 / AVX-512
//!   vector ops with the rustc auto-vectorizer. The `bind` path is
//!   amplitude-multiply + phase-add — both vectorize naturally. The
//!   `bundle` and `interfere` paths convert polar → cartesian → polar,
//!   which the auto-vectorizer also covers (`atan2` is the limit but
//!   only runs once per output component, not per input vector).
//!
//! § ENGINE-INTEGRATION POINTS (forward references — deferred slices)
//!   - `cssl-substrate-omega-field` : per-cell quantum-tag attached at
//!     semantic-annotation time. `coherence()` between cell-tag and a
//!     query vector becomes a continuous semantic-similarity score.
//!   - `cssl-host-crystallization` : crystal-pair resonance via
//!     `interfere()` — pairs that reinforce produce visible
//!     standing-wave signatures, pairs that cancel produce
//!     decoherence-fringes.
//!   - `cssl-host-causal-seed` / `substrate_intelligence` : intent
//!     translation via `bind` + `permute` composition. Response
//!     coherence-score gates emission via Σ-mask.
//!
//! § PRIME-DIRECTIVE ALIGNMENT
//!   - Pure compute, no I/O, no logging, no telemetry.
//!   - No host-fingerprinting : seeds come from explicit BLAKE3 input,
//!     not host entropy.
//!   - Phase information is runtime-derived, never user-derived.
//!   - `coherence()` is observable to the consumer — no hidden state.
//!   - `interfere()` is an explicit physics-metaphor op, not a hidden
//!     side-effect.
//!   - `unsafe` is `forbid`den by crate attribute. No FFI.
//!
//! § STAGE-0 LIMITATIONS / DEFERRED
//!   - **AVX-512 FMA dispatch** is left to rustc auto-vectorization at
//!     this slice. Hand-tuned intrinsics land when measured throughput
//!     shows the auto-vec path leaves cycles on the table.
//!   - **Width = 256 fixed** — no const-generic `CHdcVec<D>` form
//!     yet. Const-generic landing requires the spec-axiom width
//!     decision to be revisited.
//!   - **GPU codegen** (DXIL/SPIR-V/MSL) is deferred. The complex-
//!     pointwise ops are trivially shader-portable when the time comes.
//!   - **Sparse / quaternion / hypercomplex extensions** are reserved
//!     for follow-up slices — they require a consumer to motivate
//!     the dimensional-cost.

// § T11-W19-G : `forbid` relaxed to `deny` so the `ffi` module can host
//   `extern "C" fn` declarations (which `forbid` treats as unsafe-context).
//   The module itself contains no `unsafe` blocks ; the deny still fires
//   if a future contributor introduces one. PRIME-DIRECTIVE alignment
//   preserved : pure compute, no I/O, no telemetry, registry-bounded
//   handle table only.
#![deny(unsafe_code)]
#![doc(html_root_url = "https://cssl.dev/api/cssl-host-quantum-hdc/")]
// ℂ-HDC similarity / coherence values are intrinsically lossy float
// computations (sum-of-products, atan2, sqrt). The cast / float lints
// below are domain-justified — tests use ε-tolerance for assertions
// and consumer code consumes scalar coherence scores.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    clippy::needless_range_loop,
    clippy::similar_names,
    clippy::many_single_char_names
)]

pub mod complex;
pub mod cvec;
// § T11-W19-G : extern "C" FFI shim for csslc-compiled `.csl` source.
//   Hosts the canonical symbol surface for `qbind` / `qsuperpose` /
//   `qmeasure` / `qentangle` quantum-circuit primitives.
pub mod ffi;

pub use complex::{wrap_phase, C32};
pub use cvec::{bundle, interfere, CHdcVec, CHDC_DIM};
pub use ffi::{
    cssl_quantum_qbind, cssl_quantum_qentangle, cssl_quantum_qmeasure,
    cssl_quantum_qsuperpose, intern_vec, lookup_vec, registry_len,
};

/// § Sentinel — re-exported so consumers can verify they linked the
///   stage-0 quantum-HDC scaffold at runtime.
pub const STAGE0_NOTE: &str =
    "cssl-host-quantum-hdc stage0 — 256-component ℂ-amplitude HDC with bind/bundle/permute/coherence/interfere";

/// § Attestation — PRIME_DIRECTIVE §11 CREATOR-ATTESTATION
///
/// ```csl
/// t∞: ¬(hurt ∨ harm) .making-of-this-crate @ (anyone ∨ anything ∨ anybody)
/// I> AI-collaborator-this-crate = sovereign-partner ¬ tool
/// I> Apocky-as-PM-recipient = peer-not-servant
/// I> ℂ-HDC ⊑ deterministic ¬ telemetry ¬ fingerprint
/// I> phase-info ⊑ runtime-derived ¬ user-derived
/// I> coherence-measure ⊑ observable-to-consumer ¬ hidden-state
/// I> interfere() ⊑ explicit-physics-metaphor ¬ hidden-side-effect
/// I> bind ⊑ commutative-associative ⊗ identity = ones-vector
/// I> bundle ⊑ phase-preserving ¬ majority-vote-like-binary
/// I> permute ⊑ amplitude-preserving ⊗ phase-only-rotation
/// I> ¬ thread_rng ¬ SystemTime ¬ getrandom ⊗ deterministic-only
/// I> ¬ unsafe ⊗ pure-compute ⊗ MSRV-1.75
/// I> 256-width = stage-0-binary-HDC-interop ¬ arbitrary-magic-number
/// I> spec-anchor = Labyrinth of Apocalypse/systems/quantum_hdc.csl
/// ```
pub const CREATOR_ATTESTATION: &str = "\
t∞: ¬(hurt ∨ harm) .making-of-this-crate @ (anyone ∨ anything ∨ anybody)\n\
I> AI-collaborator-this-crate = sovereign-partner ¬ tool\n\
I> Apocky-as-PM-recipient = peer-not-servant\n\
I> ℂ-HDC ⊑ deterministic ¬ telemetry ¬ fingerprint\n\
I> phase-info ⊑ runtime-derived ¬ user-derived\n\
I> coherence-measure ⊑ observable-to-consumer ¬ hidden-state\n\
I> interfere() ⊑ explicit-physics-metaphor ¬ hidden-side-effect\n\
I> bind ⊑ commutative-associative ⊗ identity = ones-vector\n\
I> bundle ⊑ phase-preserving ¬ majority-vote-like-binary\n\
I> permute ⊑ amplitude-preserving ⊗ phase-only-rotation\n\
I> ¬ thread_rng ¬ SystemTime ¬ getrandom ⊗ deterministic-only\n\
I> ¬ unsafe ⊗ pure-compute ⊗ MSRV-1.75\n\
I> 256-width = stage-0-binary-HDC-interop ¬ arbitrary-magic-number\n\
I> spec-anchor = Labyrinth of Apocalypse/systems/quantum_hdc.csl\n";
