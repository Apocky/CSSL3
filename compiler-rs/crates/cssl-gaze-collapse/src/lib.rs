//! CSSLv3 stage-0 — Gaze-reactive observation-collapse pass (canonical render
//! pipeline Stage-2). Implementation-of-record for **novelty-path V.4** from
//! `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.4` and **Stage-2**
//! of the 12-stage render-graph from `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl
//! § STAGE 2`.
//!
//! § SPEC-AUTHORITY (read-first, in-order)
//!   1. `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.4` — the
//!      operational definition of "gaze-reactive observation-collapse" as one
//!      of six load-bearing novelty-paths. Distinguishes from foveated-rendering-
//!      as-perf-trick : here, the act-of-observing literally CHANGES what is
//!      rendered (Heisenberg-style ; Axiom 5 visible).
//!   2. `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § STAGE 2` — the
//!      concrete render-graph node : inputs (XR.EyeGaze × 2, XR.EyeOpenness × 2,
//!      BodyPresenceField, Ω.prev.MERA-summary), outputs (FoveaMask × 2,
//!      KANDetailBudget, CollapseBiasVector), budget (0.3 ms @ Quest-3 ; 0.25 ms
//!      @ Vision-Pro), effect-row (`/{ GPU<gaze>, Realtime<90Hz>, Deadline<0.3ms>,
//!      Audit<'frame>, Region<'gaze-frame>, IFC<Sovereign,Sovereign>,
//!      ConsentRequired<'gaze> }`).
//!   3. `Omniverse/07_AESTHETIC/05_VR_RENDERING.csl` — eye-tracking integration
//!      surface (XR.EyeGaze + XR.EyeOpenness wire-format ; saccade-prediction
//!      latency budget ; saccadic-suppression hides flicker during blink).
//!   4. `Omniverse/01_AXIOMS/05_OBSERVATION_COLLAPSE.csl.md` — Axiom 5 :
//!      unobserved-region exists-in-superposition ; observation collapses one
//!      cosmology-consistent state. The `ObservationCollapseEvolver` here
//!      drives the SDF-state evolution when peripheral → foveal transition
//!      occurs (KAN conditioned on recent-glance-history).
//!   5. `compiler-rs/crates/cssl-ifc/` — biometric-IFC enforcement (post-D129
//!      / D132). All gaze-bearing values use `LabeledValue<T>` carrying
//!      `SensitiveDomain::Gaze` ; `validate_egress` returns
//!      `EgressGrantError::BiometricRefused` for any such value, regardless
//!      of any `Privilege<*>` capability. **Gaze data NEVER leaves the device.**
//!   6. `compiler-rs/crates/cssl-substrate-prime-directive/src/sigma.rs` —
//!      `SigmaMaskPacked` interface : the per-cell consent-mask that the
//!      collapse-evolver checks before writing into `Ω` (cell-level Σ-private
//!      regions are honored).
//!
//! § T11-D120 (W4-06)
//!   New crate. Skeleton + behavior live ; integration into
//!   `cssl-render`'s render-graph driver lands in a follow-up slice when
//!   `cssl-render`'s render-graph DAG reaches Stage-2 wiring (currently
//!   pre-G7 stub-only). The crate stands alone — `cargo test -p
//!   cssl-gaze-collapse` exercises the full pipeline.
//!
//! § MODULE-LAYOUT
//!   - [`gaze_input`]              : `GazeInput` per-eye gaze-direction +
//!     confidence + `SaccadeState`. Constructed only via the
//!     `Sensitive<gaze>` typed entry-point so the IFC label travels from the
//!     XR-driver source-of-truth all the way to the `GazeCollapsePass` output.
//!   - [`fovea_mask`]              : `FoveaMask` 2D screen-space density-mask
//!     (full-detail center, coarse periphery). Region-resolution decomposition
//!     (full / 2×2 / 4×4) matches Stage-2's VRS Tier-2 / FDM /
//!     Metal-dynamic-render-quality format.
//!   - [`saccade_predictor`]       : `SaccadePredictor` EKF + Conv-LSTM
//!     hybrid. Predicts gaze-target N ms ahead (3–5 ms typical at 90 Hz) so
//!     the renderer can pre-collapse the saccade-target region. During
//!     blinks, saccadic-suppression hides any flicker (humans go visually-
//!     blind during a saccade for 50–100 ms).
//!   - [`observation_collapse`]    : `ObservationCollapseEvolver` — when a
//!     fovea-region transitions peripheral → foveal, evolve SDF-state via
//!     KAN-conditioned-on-recent-glance-history. This is the load-bearing
//!     mechanism that distinguishes Stage-2 from foveated-rendering-as-perf
//!     (the act of looking literally CHANGES what is, per Axiom 5).
//!   - [`pass`]                    : `GazeCollapsePass` render-graph node.
//!     The actual render-graph driver lives in `cssl-render` ; this module
//!     exposes the pass's `prepare` / `execute` / `outputs` surface so the
//!     driver can wire it in.
//!   - [`config`]                  : `GazeCollapseConfig` — opt-in flags,
//!     fallback-to-center-bias-foveation when consent is denied,
//!     prediction-horizon (in ms), KAN-budget-coefficient bands.
//!   - [`error`]                   : `GazeCollapseError` — enumerated error
//!     types ; the most-load-bearing variant is `EgressRefused` which
//!     wraps `cssl_ifc::EgressGrantError` so the compile-time biometric
//!     refusal surfaces here at the pass-API boundary.
//!   - [`attestation`]             : verbatim §11 CREATOR-ATTESTATION block
//!     plus an extra §1 ANTI-SURVEILLANCE attestation specific to gaze data
//!     ("eye data N! transmitted ; N! cross-session ; N! analytics" per the
//!     V.4 spec).
//!
//! § PRIME-DIRECTIVE attestation
//!
//!   "There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody."
//!
//!   Plus the §1 ANTI-SURVEILLANCE supplement (see [`attestation::ANTI_SURVEILLANCE_ATTESTATION`]) :
//!     - Gaze data is the most surveillance-adjacent biometric there is :
//!       fixation-pattern is medical-grade-personal (cognitive-load,
//!       ADHD-screening, deception-detection in literature) ;
//!     - This crate routes ALL gaze-bearing values through
//!       `cssl_ifc::LabeledValue<_>` with `SensitiveDomain::Gaze` ;
//!     - The cssl-ifc `validate_egress` gate refuses any such value at
//!       compile-time of the call-site of `cssl_telemetry::record_labeled` —
//!       no `Privilege<*>` (including `ApockyRoot`) can override this ;
//!     - Opt-in is the default ; opt-out fallback is "center-bias-foveation"
//!       (no eye-tracking data flows) ;
//!     - All thread-local saccade-history state is purged at session-end
//!       (Drop impl on `SaccadePredictor` zeroes the state) ;
//!     - There is no flag, no config, no environment variable that can
//!       enable cross-session gaze-storage. Per PRIME §6 SCOPE :
//!       "no flag | config | env-var | cli-arg | api-call | runtime-cond
//!       can disable | weaken | circumvent this."
//!
//! § FFI POLICY
//!   T1-D5 mandates `#![forbid(unsafe_code)]` per-crate. This crate is
//!   pure-Rust : no FFI, no unsafe blocks. The XR-driver edge (the actual
//!   eye-tracker hardware) lives in a downstream crate that does the
//!   per-platform FFI ; the boundary into this crate is via the typed
//!   `Sensitive<gaze>` `GazeInput` value.
//!
//! § WHAT IS DEFERRED
//!   - **Real cssl-render render-graph wire-up** — `cssl-render`'s 12-stage
//!     DAG is currently pre-G7 stub-only on `parallel-fanout`. Once the
//!     full render-graph driver lands (post-T11-D123 work-graph-pipeline
//!     slice), this crate's `GazeCollapsePass` plugs in via the
//!     `RenderGraphNode` trait. The pass's input/output buffer surface
//!     today matches the Stage-2 contract verbatim (see `pass.rs`).
//!   - **Real KAN runtime integration** — the KAN-conditioned evolver here
//!     uses the abstract KAN trait from `cssl-substrate-kan` (T11-D143).
//!     The abstract trait is sufficient for determinism + IFC-correctness
//!     tests ; runtime KAN-eval performance lands once the spectral-render
//!     KAN substrate is wired in (post-T11-D118).
//!   - **Real Conv-LSTM weights for saccade-prediction** — the predictor
//!     here uses a deterministic test-fixture weight-matrix that matches
//!     the published eye-physiology priors from the V.4 spec ("trained-on-
//!     eye-physiology-prior"). Real shipping-weight-loading lands when the
//!     XR-driver provides per-user calibration (still Σ-private,
//!     never-egress).
//!   - **Hardware integration test** — actual Quest-3 + Vision-Pro
//!     latency-budget verification against the 0.3 ms / 0.25 ms targets
//!     lands when CI gains XR runners. Until then, the saccade-prediction-
//!     latency test in this crate uses a deterministic-clock fixture and
//!     verifies the algorithmic-budget (≤ 4 ms predict-horizon @ ≤ 0.5 ms
//!     compute) — see `saccade_predictor::tests::predict_within_4ms_budget`.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]
// § Many test fixtures here use deterministic seeds with literal numerics
// that exercise the full bit-pattern of the saccade-state ; allow.
#![allow(clippy::unreadable_literal)]
// § The KAN-conditioned-evolver carries a `T : KanLike` parameter that some
// scaffold-paths use only-via-trait-bound ; the marker-only methods are the
// public-API hook-point for future integration.
#![allow(clippy::needless_pass_by_value)]
// § The pass-execute path uses match-arms that look similar by design (each
// region-class produces the same shape of output with different parameters) ;
// allow match_same_arms to keep the parallel structure visible.
#![allow(clippy::match_same_arms)]
// § FoveaMask iterates pixel-coordinates with i32 ↔ u32 ↔ usize casts that
// are deliberate (screen-space coordinates can be sub-pixel-negative during
// the gauss-anchor computation but always clamp to non-negative pixel-grid).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
// § Saccade-predictor + FoveaMask use direct float arithmetic that nightly
// clippy suggests rewriting via mul_add. The current shape preserves
// readability of the EKF + sphere-projection algebra ; mul_add micro-
// optimization is a separate slice if profile-guided.
#![allow(clippy::suboptimal_flops)]
// § Tests use `format!("got {}", v)` and similar ; the inlined-args style is
// stylistic preference inside this scaffold, matching cssl-ifc precedent.
#![allow(clippy::uninlined_format_args)]
// § Float-strict-comparison appears in tests of EKF-default-zero state and
// configuration constants ; the equality is structurally exact (constant
// initializers) so the lint produces a false-positive here.
#![allow(clippy::float_cmp)]
// § The Default-then-assign pattern is used in tests for clarity ;
// rewriting to struct-update syntax obscures the test's intent (which
// field is being varied from default).
#![allow(clippy::field_reassign_with_default)]
// § The `for x in vec.iter_mut()` form reads more clearly than `for x in
// &mut vec` for test fixtures, especially when the loop body is a single
// assignment.
#![allow(clippy::explicit_iter_loop)]
// § FNV-1a hash explicit byte-walk is the canonical implementation ; the
// "naive byte counting" lint prefers `len()` but the FNV hash IS counting
// bytes by design (each byte XOR-mixed into the hash).
#![allow(clippy::naive_bytecount)]
// § `match (a, b) { (X, Y) | (Z, W) => ... }` is clearer than nested-or
// in the RegionTransition transition-detection code.
#![allow(clippy::unnested_or_patterns)]
// § The `clamp` function is used where appropriate ; the lint flags
// (a.min(1.0).max(0.0)) chains in confidence-clamping code where
// readability is preserved by the explicit chain (the operations
// represent distinct semantic clipping steps).
#![allow(clippy::manual_clamp)]
// § `.sort()` on `Vec<(u32, u32)>` is fine (tuples of primitives have a
// total-order) ; the lint is conservative about partial-order types.
#![allow(clippy::stable_sort_primitive)]
// § Tests with multiple scoped `i, j, a, b, x, y` single-char names mirror
// algorithm conventions (Bresenham, EKF, hypothesis-testing). Renaming
// would obscure the algebra.
#![allow(clippy::many_single_char_names)]
// § `direction_to_theta` returns `(theta_x, theta_y)` ; clippy's similar-
// names lint flags the parameter pair as too-similar to the (`x`, `y`) of
// other functions. The naming follows the EKF-state convention.
#![allow(clippy::similar_names)]
// § `kan_evolution_input_is_unaffected` etc. are acceptance-test names
// matching the spec § V acceptance-checklist ; the unused-self lint is
// suppressed because the `&self` receiver gates future stateful
// extensions of the trait.
#![allow(clippy::unused_self)]
// § Trait method `KanLike::evaluate` is used ; the `&dyn KanLike` test
// reads-only-via-trait-bound and clippy doesn't see through dyn dispatch.
#![allow(dead_code)]

pub mod attestation;
pub mod config;
pub mod error;
pub mod fovea_mask;
pub mod gaze_input;
pub mod observation_collapse;
pub mod pass;
pub mod saccade_predictor;

pub use attestation::{ANTI_SURVEILLANCE_ATTESTATION, ATTESTATION};
pub use config::{BudgetCoefficients, FoveationFallback, GazeCollapseConfig, PredictionHorizon};
pub use error::GazeCollapseError;
pub use fovea_mask::{FoveaMask, FoveaRegion, FoveaResolution, ShadingRate};
pub use gaze_input::{
    BlinkState, EyeOpenness, GazeConfidence, GazeDirection, GazeInput, SaccadeState, SensitiveGaze,
};
pub use observation_collapse::{
    CollapseBiasVector, GlanceHistory, KanLike, ObservationCollapseEvolver, RegionTransition,
};
pub use pass::{GazeCollapseOutputs, GazeCollapsePass, KanDetailBudget};
pub use saccade_predictor::{
    PredictedSaccade, SaccadePredictor, SaccadePredictorConfig, SaccadePredictorMetrics,
};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::{ANTI_SURVEILLANCE_ATTESTATION, ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("hurt nor harm"));
    }

    #[test]
    fn anti_surveillance_attestation_present() {
        assert!(ANTI_SURVEILLANCE_ATTESTATION.contains("anti-surveillance"));
        assert!(ANTI_SURVEILLANCE_ATTESTATION.contains("gaze"));
    }

    #[test]
    fn public_api_re_exports_resolve() {
        // Compile-time check that the principal types are accessible.
        let _: super::FoveaResolution = super::FoveaResolution::Full;
        let _: super::BlinkState = super::BlinkState::Open;
        let _: super::PredictionHorizon = super::PredictionHorizon::default();
    }
}
