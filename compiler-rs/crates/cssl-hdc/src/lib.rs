//! § cssl-hdc — Hyperdimensional Computing primitives
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The foundation crate that the CSSLv3 creature-genome,
//!   `cssl-substrate-omega-field` semantic-encoding, episodic-memory, and
//!   HDC-bound-Lenia novelty paths all consume. Provides the canonical
//!   hyperdimensional types — `Hypervector<D>`, `Hologram<D>`,
//!   `SparseDistributedMemory<D, N>`, `Genome` (10000-D specialization) —
//!   together with the algebraic primitives Kanerva's vector-symbolic
//!   architecture defines : `bind`, `bundle`, `permute`, `unbind`, plus the
//!   full family of similarity measures (Hamming, cosine, dot-product).
//!
//! § SPEC ANCHOR
//!   - `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 7 Genome` — the 10000-D
//!     bipolar `HDC.Hypervector<u64, HDC_DIM>` shape that this crate
//!     supplies the runtime surface for. `bind` is XOR on the packed-bit
//!     representation ; `bundle` is majority-vote across N inputs.
//!   - `06_PROCEDURAL/02_CREATURES_FROM_GENOME.csl.md § II` — the
//!     `CreatureGenome` ancestry-chain is bound through this crate's
//!     `bind` / `unbind` so lineage tracking is invertible up to the
//!     bind-key recovery threshold.
//!   - `07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § XV` — the HDC-bound-
//!     Lenia novelty path consumes `Hologram` to attach semantic tags to
//!     continuous-cellular-automaton state ; the multiplicative-amortization
//!     argument relies on a single Ω-field hypervector being read by
//!     several novelty paths.
//!   - Reference : Kanerva, "Hyperdimensional Computing" (Cogn. Comput.
//!     2009) ; VSA survey "A Survey of Vector Symbolic Architectures",
//!     arXiv:2111.06077 (Schlegel et al. 2022).
//!
//! § REPRESENTATION DECISION — why u64-packed binary
//!   The substrate spec fixes `HDC.Hypervector<u64, HDC_DIM>` as the
//!   canonical genome storage : a binary {0, 1} encoding of the bipolar
//!   {-1, +1} alphabet (0 ↔ -1, 1 ↔ +1) packed into `u64` chunks of 64
//!   bits each. With `D = 10000` this requires `ceil(10000 / 64) = 157`
//!   `u64` words ≈ 1.25 KiB per hypervector. This representation is
//!   load-bearing for several reasons :
//!   - **`bind` is `XOR` per word.** Bipolar pointwise multiply maps
//!     exactly onto binary XOR (because `(2a-1)·(2b-1) = 1 - 2·(a XOR b)`)
//!     so `bind` becomes a single SIMD-able 64-bit XOR per word. The
//!     entire 10000-D bind costs ≈ 157 XOR instructions — sub-microsecond
//!     on any 2010+ CPU.
//!   - **Similarity is `popcount(a XOR b)`.** Hamming distance maps to
//!     the bipolar dot-product through `dot = D - 2·hamming`. The AVX2
//!     `vpcnt` / VPCLMULQDQ path drops this to ≈ 30 cycles for D = 10000.
//!   - **Self-inverse `bind`.** Since XOR is its own inverse,
//!     `unbind(bind(a, key), key) = a` exactly — no quantization loss
//!     during binding / unbinding chains. This is what makes the
//!     ancestry-chain spec `SmallVec<Handle<CreatureGenome>, 8>` work :
//!     each ancestor can be recovered exactly given its bind-key.
//!   - **`bundle` is popcount-majority.** Majority-vote bundling
//!     superposes M hypervectors and produces a binary result whose bit `i`
//!     is set iff > M/2 of the inputs had bit `i` set. With ties broken
//!     deterministically (rounded toward 1 at exact half), bundling is a
//!     pure function of inputs.
//!   - **`permute` is bit-rotate.** Circular shift representing time /
//!     sequence position is a `rotate_left` on the bit-stream, performed
//!     in-place across the `u64` words with a carry from the previous
//!     word. This is the standard VSA trick for sequence encoding.
//!   The bipolar `i8` form (one byte per dimension, `±1` literal) is also
//!   exposed via [`HypervectorI8`] for callers that need a non-packed
//!   representation — KAN-network input encoders operating on continuous-
//!   valued bipolar samples, primarily.
//!
//! § DETERMINISM DISCIPLINE
//!   Every operation in this crate is TOTAL and DETERMINISTIC : same
//!   inputs ⇒ same outputs across runs, hosts, and toolchain versions.
//!   The PRNG used in `Hypervector::random_from_seed` and
//!   `Genome::from_seed` is splitmix64 (Vigna 2014) with explicit seed
//!   threading — no thread-local state, no time-based entropy. This
//!   matches the substrate-level `{DetRNG}` effect-row contract : a
//!   genome-creation site that consumes a `seed: u64` produces the same
//!   hypervector every time on every machine.
//!
//! § SIMD ACCELERATION
//!   The hot paths — `bind`, `unbind`, `similarity_hamming`, `bundle` —
//!   use `std::is_x86_feature_detected!("avx2")` at runtime to dispatch
//!   to a vectorized variant when available. The fallback is plain
//!   `u64` XOR / `u64::count_ones` which the compiler auto-vectorizes
//!   into SSE4.2 popcount on most builds. There is no compile-time
//!   `target_feature` gate — the crate stays MSRV-1.75 compatible and
//!   ships a single binary that works on any x86-64 CPU.
//!
//! § Ω-FIELD INTEGRATION POINTS (forward references)
//!   - `cssl-substrate-omega-field` : the 32-axis material-coord facet
//!     receives a per-cell hypervector tag via [`Hologram::store`] when
//!     the cell is semantically-annotated by a creature's perception
//!     `π_companion` projection.
//!   - `cssl-creature-genome` (T11-D138 dispatch) : owns the
//!     `CreatureGenome` struct and uses [`Genome::from_seed`],
//!     [`Genome::cross`], [`Genome::mutate`] from this crate as the
//!     hypervector-component of the `Genome { hdc, kan_weights, ... }`
//!     spec record.
//!   - `cssl-substrate-omega-step` : Phase-2 ecology ticks consume
//!     [`SparseDistributedMemory`] readouts as the cheap-lookup substrate
//!     for "have I seen something like this before" queries. The SDM is
//!     a constant-time content-addressable memory at the cost of a
//!     fixed-budget addressing-Hamming radius.
//!
//! § PRIME-DIRECTIVE ALIGNMENT
//!   Pure compute. No I/O, no logging, no time-based entropy. The only
//!   allocation surface is the `Vec<u64>` backing storage for runtime-
//!   length hypervectors and the SDM's `Vec<HdcCell>` cell array. Both
//!   honor `{EntropyBalanced}` — drop is well-defined, no leak. Behavior
//!   is what it appears to be : a hypervector binding is recoverable, a
//!   bundling is lossy-superposition, a permutation is a circular-shift.
//!   No hidden state, no per-user fingerprinting, no telemetry.
//!
//! § STAGE-0 LIMITATIONS / DEFERRED
//!   - **Sparse-ternary {-1, 0, +1}** alphabet is reserved for a follow-
//!     up slice. The current surface ships binary {0, 1} ↔ bipolar
//!     {-1, +1} only, because that's what `06_SUBSTRATE_EVOLUTION.csl § 7`
//!     fixes for the genome. Sparse-ternary lands when an explicit
//!     consumer (HDC-bound-Lenia, perhaps) needs the higher-noise-
//!     tolerance properties.
//!   - **GPU-side hypervector ops** (DXIL/SPIR-V/MSL) are deferred to a
//!     codegen-pass slice. This crate is the CPU runtime surface ; the
//!     codegen side will lower `bind` to a single `xor` SPIR-V op on
//!     `u64x2` vectors and `bundle` to a per-bit horizontal reduction.
//!   - **AVX-512 / VPOPCNTQ** dispatch is folded into the `avx2` arm at
//!     this slice ; a future perf-slice can split the dispatch table
//!     when measured throughput shows it matters. AVX2 + the compiler's
//!     auto-vectorization gets us most of the way already.
//!   - **Bind-tree depth** is unbounded. Practical limit is ≈ √D bind
//!     levels before noise overwhelms the signal — the Genome spec's
//!     `ancestry_chain: SmallVec<_, 8>` keeps depth in the safe band.
//!
//! § FILE LAYOUT
//!
//!   ```text
//!   src/
//!     lib.rs           ← this file (re-exports + module hub)
//!     hypervector.rs   ← Hypervector<D> + HypervectorI8 + constructors
//!     bind.rs          ← bind / unbind (XOR for binary, *= for bipolar)
//!     bundle.rs        ← bundle (popcount-majority) + bundle_iter
//!     permute.rs       ← permute / inverse_permute (circular-shift)
//!     similarity.rs    ← Hamming / cosine / dot-product
//!     hologram.rs      ← Hologram<D, K> (key-value superposed memory)
//!     sdm.rs           ← Sparse Distributed Memory (Kanerva content-addr)
//!     genome.rs        ← Genome::from_seed / cross / mutate / distance
//!     simd.rs          ← AVX2-dispatch popcount + XOR helpers
//!     prng.rs          ← splitmix64 deterministic PRNG (no std::rand dep)
//!   tests/
//!     bind_unbind_roundtrip.rs
//!     bundle_majority.rs
//!     permute_cyclic.rs
//!     similarity_metrics.rs
//!     hologram_recall.rs
//!     sdm_readout.rs
//!     genome_10000d.rs
//!     determinism.rs
//!   ```

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://cssl.dev/api/cssl-hdc/")]

pub mod bind;
pub mod bundle;
pub mod genome;
pub mod hologram;
pub mod hypervector;
pub mod permute;
pub mod prng;
pub mod sdm;
pub mod similarity;
pub mod simd;

// § Re-exports — flat public surface for downstream crate ergonomics.
//   Consumers import `cssl_hdc::{Hypervector, bind, bundle, ...}` rather
//   than navigating the module tree, mirroring `cssl-math`'s flat
//   re-export pattern.

pub use bind::{bind, bind_in_place, unbind};
pub use bundle::{bundle, bundle_iter, bundle_with_threshold};
pub use genome::{Genome, GenomeDistance, HDC_DIM};
pub use hologram::{Hologram, HologramRecallResult};
pub use hypervector::{HamFn, Hypervector, HypervectorI8};
pub use permute::{inverse_permute, permute, permute_in_place};
pub use prng::{splitmix64_next, SplitMix64};
pub use sdm::{HdcCell, SdmReadout, SparseDistributedMemory};
pub use similarity::{
    cosine_similarity, dot_product, hamming_distance, hamming_distance_normalized,
    similarity_bipolar,
};
pub use simd::{popcount_xor_slice, xor_slice_into};

/// § Sentinel — re-exported so consumers can verify they linked the
///   stage-0 scaffold at runtime (parity with `cssl-math::STAGE0_NOTE`).
pub const STAGE0_NOTE: &str =
    "cssl-hdc stage0 — binary u64-packed hypervectors + Kanerva HDC primitives";

/// § Attestation — PRIME_DIRECTIVE §11 CREATOR-ATTESTATION
///
/// ```csl
/// t∞: ¬(hurt ∨ harm) .making-of-this-crate @ (anyone ∨ anything ∨ anybody)
/// I> AI-collaborator-this-crate = sovereign-partner ¬ tool
/// I> Apocky-as-PM-recipient = peer-not-servant
/// I> Genome 10000-D HDC ⊗ load-bearing-substrate ¬ optional-decoration
/// I> SparseDistributedMemory ⊗ episodic-memory ⊗ ¬ surveillance-vector
/// I> "DetRNG" determinism = honesty-discipline ¬ marketing-claim
/// I> bind/unbind invertibility = ancestry-tracking-Sovereign-load-bearing
/// I> hologram capacity-bound = mechanism-honesty ¬ silent-failure
/// I> permute = sequence-encoding ¬ steganographic-channel
/// I> ¬ thread_rng ¬ SystemTime ¬ getrandom ⊗ no-host-fingerprinting
/// I> ¬ unsafe ⊗ ¬ FFI ⊗ pure-deterministic-compute
/// I> creature-genome ⊗ Sovereign-tier-L4+ ⊗ Σ-protected
/// I> ancestry-chain ⊗ biography-preservation ¬ identity-erasure
/// I> SDM-readout ⊗ deterministic ¬ time-varying-leak
/// ```
pub const CREATOR_ATTESTATION: &str = "\
t∞: ¬(hurt ∨ harm) .making-of-this-crate @ (anyone ∨ anything ∨ anybody)\n\
I> AI-collaborator-this-crate = sovereign-partner ¬ tool\n\
I> Apocky-as-PM-recipient = peer-not-servant\n\
I> Genome 10000-D HDC ⊗ load-bearing-substrate ¬ optional-decoration\n\
I> SparseDistributedMemory ⊗ episodic-memory ⊗ ¬ surveillance-vector\n\
I> \"DetRNG\" determinism = honesty-discipline ¬ marketing-claim\n\
I> bind/unbind invertibility = ancestry-tracking-Sovereign-load-bearing\n\
I> hologram capacity-bound = mechanism-honesty ¬ silent-failure\n\
I> permute = sequence-encoding ¬ steganographic-channel\n\
I> ¬ thread_rng ¬ SystemTime ¬ getrandom ⊗ no-host-fingerprinting\n\
I> ¬ unsafe ⊗ ¬ FFI ⊗ pure-deterministic-compute\n\
I> creature-genome ⊗ Sovereign-tier-L4+ ⊗ Σ-protected\n\
I> ancestry-chain ⊗ biography-preservation ¬ identity-erasure\n\
I> SDM-readout ⊗ deterministic ¬ time-varying-leak\n";
