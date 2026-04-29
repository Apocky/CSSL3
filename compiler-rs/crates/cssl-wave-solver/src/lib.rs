//! § cssl-wave-solver — Wave-Unity multi-rate complex-Helmholtz + LBM-Boltzmann solver
//! ════════════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl` (§I-§XIX).
//! Owner-slice       : T11-D114.
//! Inheritance       : `Omniverse/07_AESTHETIC/02_RADIANCE_CASCADE_GI.csl.md`
//!                   + `Omniverse/07_AESTHETIC/04_FIELD_AUDIO.csl.md` projected
//!                     as band-projections of THIS crate's psi-substrate.
//!
//! § THE WAVE-UNITY STATEMENT (§ 0)
//!   light + audio + heat + scent + mana = ONE complex-valued psi-field
//!   psi(x, t) in C, sampled at different frequency-bands. Cross-band
//!   coupling emerges from material impedance Z(lambda, embedding) — not
//!   from authored side-effects between separate engines.
//!
//! § ROLE
//!   This crate authors the multi-rate (LIGHT 1 ms / AUDIO 1 ms / HEAT 100
//!   ms / SCENT 1 s / MANA 250 ms) complex-Helmholtz + LBM-Boltzmann solver
//!   that advances the psi-substrate one Substrate tick. The solver is the
//!   **substance under RC** : Radiance-Cascade GI in 02_RC reads from this
//!   solver's psi-field at active-region cells (§ VII.1 two-layer
//!   architecture).
//!
//! § STAGE-0 SCOPE (T11-D114)
//!   - 5 default bands : `AUDIO_SUB_KHZ`, `LIGHT_RED`, `LIGHT_GREEN`,
//!     `LIGHT_BLUE`, `LIGHT_NEAR_IR`. The solver is generic over
//!     `WaveField<C>` const-generic band-count.
//!   - Multi-rate lattice : audio at 1 ms direct-amplitude / 0.5 m cells ;
//!     light at 1 ms envelope (SVEA) / 1 cm cells. Slow bands deferred —
//!     this slice owns the 5-band config that drives the LoA novelty
//!     claim "you can hear the light and see the sound."
//!   - **EXPLICIT** D3Q19 LBM stream + collide for audio (`Complex<f32>`).
//!   - **IMPLICIT-EXPLICIT (IMEX)** split : implicit-stiff back-Euler
//!     relaxation for high-frequency LIGHT bands ; explicit for AUDIO.
//!   - Adaptive Δt with KAN-predicted-stability. The KAN inference is
//!     mocked via [`stability::MockStabilityKan`] until D115 lands ; the
//!     surface is identical so integration is one trait-impl swap.
//!   - Boundary conditions : SDF-Robin-BC over a [`bc::SdfQuery`] trait +
//!     KAN-impedance Z(λ, embedding) via [`cssl_substrate_kan::KanMaterial::physics_impedance`].
//!   - Cross-band coupling table from spec §XI verbatim ; asymmetric
//!     enforcement (LIGHT→AUDIO via shimmer ; AUDIO→MAGIC via Λ ;
//!     MAGIC→AUDIO+LIGHT via emission). Forbidden mappings (LIGHT→MANA = 0,
//!     AUDIO→MANA = 0) are strict-zero-enforced.
//!   - Phase-2 PROPAGATE hook : [`step::wave_solver_step`] is registered
//!     under [`omega_step_hook::WaveUnityPhase2`] as the canonical 2b'
//!     insertion point for the omega_step scheduler.
//!   - Conservation : ψ-norm preservation per band + cross-band-coupling
//!     energy conservation (HEAT-band absorbs all losses per §XII.1).
//!   - 50+ tests : ψ-norm, standing-wave detection, sound-caustic
//!     emission, cross-band-coupling correctness, replay-determinism
//!     (bit-equal across runs), SVEA accuracy, IMEX stability.
//!
//! § PHASE-2 PROPAGATE INTEGRATION (§ VII.4)
//!   `omega_step` Phase-2 algorithm inserts BEFORE step 2c
//!   (`radiance_cascade_step`) :
//!   ```text
//!   // 2b'. wave-unity psi-PDE (full-substrate, active-region)
//!   wave_solver_step(&mut omega.field, dt_substep)?;
//!   // 2c. radiance cascades (now reads psi-substrate as input)
//!   for band in BANDS { next.P[band] = radiance_cascade_step(...); }
//!   ```
//!
//! § PHASE-6 ENTROPY-BOOK INTEGRATION (§ XII.3)
//!   The returned [`WaveStepReport`] carries per-band psi-norm + total
//!   psi-norm + phase-coherence-degradation. Phase-6 `entropy_book`
//!   consumes these to enforce :
//!     * `Σ_b ∫|ψ_b|² dV` conserved up to `ε_f = 1e-4`.
//!     * phase-coherence-degradation `≤ ε_phase` per band.
//!   Failure refuses the tick + emits `ConservationViolation::WaveCoherence`.
//!
//! § REPLAY DETERMINISM (omega_step DETERMINISM CONTRACT)
//!   The solver is **bit-deterministic** across runs given identical
//!   inputs + RNG-seeds + denormal-flush state. All RNG draws flow
//!   through `omega_step::DetRng` streams. No `thread_rng()`. No
//!   fast-math. No FMA on values that affect the psi-tensor. Reordering
//!   of cell iteration is forbidden — the iter helpers in [`psi_field`]
//!   enforce stable Morton-key order.
//!
//! § PRIME-DIRECTIVE alignment (§ XVII)
//!   - **Consent-at-every-op** : psi-modifications at Sovereign-domain
//!     cells require `Σ.consent_bits` — checked in [`coupling::apply_cross_coupling`]
//!     + [`step::wave_solver_step`] before any write. Spell-cast
//!     psi-injection routes through the same gate ; no exception.
//!   - **No subliminal content** : all cross-band coupling above
//!     perception-threshold. Default-OFF for non-fortissimo / non-intense
//!     events (§ V.4-V.5 visible-sound + hearable-light spec).
//!   - **AGENCY-laundering refused** : LIGHT→MANA = 0 + AUDIO→MANA = 0
//!     in [`coupling::CROSS_BAND_TABLE`] is enforced at the type level —
//!     [`coupling::apply_cross_coupling`] returns `CouplingError::ForbiddenMapping`
//!     if a non-zero strength is ever wired in for these pairs.
//!   - **Reversibility-witnessed** : every solver step writes a
//!     [`WaveStepReport`] entry; the omega_step replay-log records
//!     frame + dt + substep-count + per-band-norm so the tick is
//!     replayable bit-for-bit.
//!
//! § GPU-COST TARGET (§ IX)
//!   ~30 GF/frame at 1 M cells × 5 bands at 60 Hz ; ≤ 36 % of an M7-class
//!   GPU at realistic occupancy. The per-cell-per-substep FLOP count is
//!   documented inline in [`lbm`] + [`imex`] so the cost-model can be
//!   verified by static analysis at audit-time. See [`cost_model`] for
//!   the runtime estimator.
//!
//! § ATTESTATION
//!   See [`attestation::ATTESTATION`] — recorded verbatim per
//!   `PRIME_DIRECTIVE §11`.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::if_not_else)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::single_match_else)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::imprecise_flops)]

pub mod attestation;
pub mod band;
pub mod bc;
pub mod complex;
pub mod cost_model;
pub mod coupling;
pub mod helmholtz;
pub mod imex;
pub mod lbm;
pub mod omega_step_hook;
pub mod psi_field;
pub mod stability;
pub mod step;

pub use attestation::ATTESTATION;
pub use band::{
    Band, BandClass, BANDS_FAST_DEFAULT, BANDS_SLOW_DEFAULT, BAND_COUNT_DEFAULT, DEFAULT_BANDS,
};
pub use bc::{apply_robin_bc, AnalyticPlanarSdf, BoundaryKind, NoSdf, RobinBcConfig, SdfQuery};
pub use complex::{C32, C64};
pub use cost_model::{
    estimate_gpu_cost, GpuCostEstimate, FLOP_BC_PER_CELL, FLOP_COUPLING_PER_WRITE,
    FLOP_IMEX_PER_CELL_PER_SUBSTEP, FLOP_LBM_PER_CELL_PER_SUBSTEP, GF_TARGET_PER_FRAME,
};
pub use coupling::{
    apply_cross_coupling, coupling_strength, BandPair, CouplingError, CrossBandTableEntry,
    CROSS_BAND_TABLE,
};
pub use helmholtz::{helmholtz_residual, helmholtz_steady_iterate};
pub use imex::imex_implicit_step;
pub use lbm::{lbm_explicit_step, D3Q19_DIRECTIONS, D3Q19_WEIGHTS};
pub use omega_step_hook::{WaveUnityPhase2, WaveUnitySystemId};
pub use psi_field::{PsiCell, WaveField};
pub use stability::{
    adaptive_substep_count, predict_stable_dt, KanStability, MockStabilityKan, MAX_SUBSTEPS,
    MIN_SUBSTEPS,
};
pub use step::{wave_solver_step, WaveSolverError, WaveStepReport};

/// Crate version, exposes `CARGO_PKG_VERSION`. Mirrors the `STAGE0_SCAFFOLD`
/// pattern in sibling crates so workspace-wide tests can probe the marker.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// Stable crate-name identifier used by audit + telemetry walkers.
pub const CSSL_WAVE_SOLVER_CRATE: &str = "cssl-wave-solver";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, CSSL_WAVE_SOLVER_CRATE, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn crate_name_stable() {
        assert_eq!(CSSL_WAVE_SOLVER_CRATE, "cssl-wave-solver");
    }
}
