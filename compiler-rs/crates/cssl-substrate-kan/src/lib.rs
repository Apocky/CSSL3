//! § cssl-substrate-kan — KAN substrate runtime + Φ-pattern-pool
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The wiring crate that materializes the substrate-spec
//!   `02_CSSL/06_SUBSTRATE_EVOLUTION.csl` surface fragments :
//!
//!   - `KanNetwork<I, O>`         (§§ 4)  — minimal eval-skeleton (BSpline edge fns)
//!   - `KanGenomeWeights`         (§§ 7)  — tri-net package (body + cognitive + capability)
//!   - `KanMaterial`              (§§ 5)  — multi-variant : spectral_brdf<N> + single_band
//!                                          + physics_impedance + creature_morphology
//!   - `Pattern` (Phi'Pattern)    (§§ 1)  — substrate-invariant identity carrier
//!   - `AppendOnlyPool<Pattern>`  (§§ 3)  — `phi_table` storage in `OmegaField`
//!   - `Handle<Phi>`              (§§ 1)  — packed (generation u32, index u32) = u64
//!
//!   This is the Φ-FACET layer : the runtime surface that lets `FieldCell`'s
//!   `pattern_handle` field point into a stable, append-only pool of
//!   substrate-invariant patterns ; and that lets `KanMaterial::translate_to_substrate`
//!   preserve the 256-bit Pattern fingerprint across substrate-class boundaries
//!   (Axiom-2 substrate-relativity invariant + `08_BODY/03_DIMENSIONAL_TRAVEL.csl §V`
//!   round-trip test).
//!
//! § SPEC ANCHORS
//!   - `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 1`  — FieldCell.pattern_handle field
//!   - `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 3`  — `phi_table : PhiTable = AppendOnlyPool<Phi'Pattern>`
//!   - `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 4`  — KanNetwork<I, O> spec
//!   - `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 5`  — KanMaterial spec (BRDF + IOR + emission + thermal + acoustic)
//!   - `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 7`  — KanGenomeWeights tri-net
//!   - `08_BODY/03_DIMENSIONAL_TRAVEL.csl § V`   — Pattern fingerprint round-trip preservation
//!   - `Omniverse/01_AXIOMS/02_SUBSTRATE_RELATIVITY` — Pattern-invariance theorem
//!
//! § Φ-PATTERN-POOL DESIGN — append-only with stable handle
//!   The substrate spec is explicit : `phi_table` is `AppendOnlyPool<Pattern>`
//!   with stable-handle-indexed access. The append-only discipline guarantees :
//!
//!   - **Stable handles.** A `Handle<Pattern>` minted at stamp-time stays valid
//!     for the lifetime of the pool. No compaction, no reallocation that
//!     invalidates outstanding handles. This matches the `FieldCell.pattern_handle`
//!     contract — a 64-bit field stored inline in a 72-byte std430 cell, dereferenced
//!     by every substrate-translation, every audit-chain extension, every
//!     consent-check that touches the cell. Handle invalidation would be a
//!     soundness bug at the runtime level.
//!
//!   - **Immutability post-stamp.** Once `Pattern::stamp(genome, weights, tag)`
//!     returns a `Handle`, the pattern data is read-only. The 256-bit blake3
//!     fingerprint is computed eagerly at stamp-time and cached in the `Pattern`
//!     record itself ; subsequent `Pattern::resolve(handle).fingerprint()` is
//!     a 32-byte memcpy, not a recomputation. This is what lets the
//!     dimensional-travel round-trip test compare fingerprints by value-equality
//!     in O(1) without trusting any caller to recompute.
//!
//!   - **Generation-tagged collision-free indices.** `Handle = (gen: u32, idx: u32)`
//!     packed into a `u64`. The pool tracks per-slot generation counters that
//!     advance each time a slot is stamped (single advancement at append-time
//!     since this is append-only, but the generation tag is reserved against
//!     future de-duplication paths — see `RESERVED FOR DEDUP` rustdoc on
//!     `AppendOnlyPool::stamp_with_dedup_hint`). The collision-free guarantee
//!     is checked by `Handle::is_live(pool)` ; a stale handle returned across
//!     a hypothetical pool-recycle would mismatch generations and refuse
//!     resolve.
//!
//! § KanMaterial MULTI-VARIANT INSTANTIATION
//!   The substrate spec specifies a single `KanMaterial` shape with five
//!   internal `KanNetwork<32, _>` fields (BRDF, IOR, emission, thermal, acoustic).
//!   But downstream callers want different specializations for different paths :
//!
//!   - **`spectral_brdf<N_BANDS>`** — the hyperspectral renderer wants
//!     N_BANDS-spectrum BRDF where N_BANDS is a const-generic. This variant
//!     is the canonical render-path entry-point ; KAN-network output dim
//!     scales with N_BANDS.
//!
//!   - **`single_band_brdf`** — the prototype renderer (and most unit tests)
//!     uses a single-band scalar BRDF. This is the smallest viable shape :
//!     `KanNetwork<32, 1>` for albedo, no spectral splitting. Useful for
//!     non-render consumers (e.g. cohomology tests that just need a Pattern
//!     fingerprint and don't care about BRDF correctness).
//!
//!   - **`physics_impedance`** — the wave-unity solver wants `Z(λ)` (impedance
//!     as a function of wavelength) but doesn't need BRDF / IOR / emission.
//!     This variant is `KanNetwork<32, 4>` (4 bands of complex impedance =
//!     8 floats) — much narrower than the full KanMaterial.
//!
//!   - **`creature_morphology`** — the body-omnoid SDF builder wants a
//!     KAN-net that maps `genome_embedding : vec'32 → SDF_params : vec'16`,
//!     the morphological coefficients that `crystallize::<S>(&phi, target)`
//!     in `08_BODY/03_DIMENSIONAL_TRAVEL.csl § II` consumes to re-derive the
//!     Bone + Flesh layers in the target substrate. This is `KanNetwork<32, 16>`.
//!
//!   The four variants share the same `KanMaterial` envelope (32-D embedding
//!   + fingerprint + optional `pattern_link`) but instantiate the inner
//!   networks differently. The variant tag is preserved through
//!   `translate_to_substrate` so a `physics_impedance` material translated
//!   into a substrate that only supports `single_band_brdf` falls back to a
//!   well-defined degraded form rather than panic'ing.
//!
//! § PRIME-DIRECTIVE ALIGNMENT
//!   The Pattern type IS the spec's "Sovereign-Φ link" carrier — it IS the
//!   thing whose `Sovereignty` (at tier L4+) is consent-protected. This crate
//!   surface MUST :
//!   - **Be deterministic** — `Pattern::stamp(genome, weights, tag)` produces
//!     identical fingerprints on every host, every toolchain, every run.
//!     No `SystemTime::now()`, no `thread_rng()`, no per-host fingerprinting.
//!   - **Preserve information** — `translate_to_substrate` re-derives surface
//!     properties via the target substrate's laws but never modifies the
//!     Pattern fingerprint. The fingerprint is the substrate-invariant.
//!   - **Refuse silent overwrites** — the append-only discipline means a
//!     handle minted by one Sovereign cannot be aliased / recycled to point
//!     at a different Pattern. Sovereignty over a Φ-handle is monotone.

#![forbid(unsafe_code)]

pub mod handle;
pub mod kan_genome_weights;
pub mod kan_material;
pub mod kan_network;
pub mod pattern;
pub mod phi_table;
pub mod pool;

// § Top-level re-exports for the canonical surface — match the
//   `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § IV` REQUIRED NEW TYPES list.
pub use handle::{Handle, HandleResolveError, NULL_HANDLE};
pub use kan_genome_weights::{
    KanGenomeWeights, BODY_PARAMS, CAP_PARAMS, COG_PARAMS,
};
pub use kan_material::{
    KanMaterial, KanMaterialKind, MaterialFingerprint, BRDF_OUT_DIM,
    EMBEDDING_DIM, IMPEDANCE_BANDS, MORPHOLOGY_PARAMS,
};
pub use kan_network::{KanNetwork, SplineBasis, KAN_LAYERS_MAX};
pub use pattern::{Pattern, PatternFingerprint, PatternStampError, SubstrateClassTag};
pub use phi_table::PhiTable;
pub use pool::{AppendOnlyPool, PoolError, PoolIter};

/// § Crate version sentinel — bumped when the public surface contract changes
///   in a way that invalidates downstream `pattern_handle` handles.
pub const SUBSTRATE_KAN_SURFACE_VERSION: u32 = 1;

/// § Maximum number of patterns admissible in a single `phi_table`. The spec
///   declares `Handle<Phi'Pattern>` as a packed u64 with 32-bit index, so the
///   theoretical maximum is `2^32 - 1` patterns per pool. The runtime
///   enforces a soft cap at `2^30` to leave headroom for generation-tag
///   advances under future de-duplication. A single substrate's Φ-pool will
///   not realistically exceed `~1e6` patterns ; the cap is defensive only.
pub const MAX_PATTERNS_PER_POOL: usize = 1 << 30;
