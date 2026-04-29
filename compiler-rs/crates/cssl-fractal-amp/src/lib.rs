//! § cssl-fractal-amp — Stage-7 Sub-Pixel Fractal-Tessellation Amplifier
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The Stage-7 of the canonical 12-stage render-pipeline declared in
//!   `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-7`. The
//!   amplifier turns coarse-SDF samples (from Stage-5 raymarch ; D116) into
//!   infinite-detail SDF via per-fragment KAN-spline-network evaluation,
//!   per the V.3 novelty-path declared in
//!   `07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.3`.
//!
//!   ‼ NO-LOD-popping ⊗ NO-mipmap-bias ⊗ NO-Nanite-cluster-swap.
//!   ‼ NO-ASSET-ON-DISK ⊗ detail emerges-purely-from-KAN-evaluation.
//!   ‼ DETERMINISTIC ⊗ amplifier ⊗ fn-of-position (no temporal hash) ⇒ no
//!     flicker between frames at fixed camera.
//!
//! § FOUR-COMPONENT SURFACE
//!   The crate's public surface is four cooperating types :
//!
//!   - [`FractalAmplifier`] — the per-fragment evaluator. Wraps three KAN
//!     networks (`KAN_micro_displacement`, `KAN_micro_roughness`,
//!     `KAN_micro_color_perturbation`) and packs the seven-input vector
//!     `[pos.xyz | view.xy_proj | grad.norm_2D]` per
//!     `07_KAN_RUNTIME_SHADING § IX § canonical-call-site-signature`. The
//!     evaluator is a `fn(world_pos, view_dir, base_sdf_grad,
//!     detail_budget) -> AmplifiedFragment` over a borrowed
//!     `KanNetwork<7, _>` triple.
//!
//!   - [`DetailBudget`] — per-pixel KAN-detail-budget conditioned on
//!     (a) FoveaMask from D120 gaze-collapse,
//!     (b) view-distance to the surface (PGA-derived per-pixel-projected-
//!         area trigger),
//!     (c) KAN-confidence (the second output of the recursion-truncation
//!         KAN ; rises with surface curvature, falls with atmospheric
//!         attenuation).
//!     The budget is consumed by the recursion driver to decide how deep
//!     the fractal recursion descends. The peripheral-skip / 2×2-mid /
//!     full-fovea branching is encoded here, matching the
//!     `06_RENDERING_PIPELINE § Stage-7` compute table verbatim.
//!
//!   - [`RecursiveDetailLOD`] — fractal-tessellation depth controller.
//!     Wraps a `SmallVec<DetailLevel, 5>` stack. Peripheral fragments
//!     unwind to depth 0 (pure base-SDF, no amplifier evaluation). Mid
//!     fragments to depth 1-2. Foveal fragments evaluate 3-5 levels,
//!     each level halving the spatial-step and feeding the previous
//!     level's micro-displacement back into the next-level's input
//!     vector — this is the fractal-self-similarity property declared in
//!     `06_RENDERING_PIPELINE § Stage-7 § fractal-property`.
//!
//!   - [`SdfRaymarchAmplifier`] — the integration trait that ties this
//!     crate to `cssl-render-v2` (D116). The trait declares a single
//!     entry point `amplify_at_hit(&hit) -> AmplifiedFragment` that the
//!     raymarcher's bisection-refine step calls when ray-march-step
//!     approaches HIT_EPSILON. By naming the trait HERE rather than in
//!     render-v2, this crate stays buildable when D116 has not landed —
//!     callers simply implement the trait against their own RayHit
//!     equivalent. A reference [`MockSdfHit`] is provided for unit tests
//!     to exercise the amplifier in isolation.
//!
//! § DETERMINISM CONTRACT — load-bearing
//!   Per `07_AESTHETIC/00_EXOTICISM_PRINCIPLES § V.3 (d) reversibility` and
//!   `06_RENDERING_PIPELINE § Stage-7 § effect-row { Pure }`, the amplifier
//!   MUST be a pure function of `(world_pos, view_dir, base_sdf_grad,
//!   detail_budget, kan_weights)`. There is :
//!
//!   - NO temporal hash (no frame-counter, no time-since-boot in inputs)
//!   - NO RNG (no pseudo-random sampling for sub-pixel jitter)
//!   - NO globally-mutable cache (the optional thread-local last-N
//!     evaluations cache from `07_AESTHETIC/01 § V.3 (c)` is a SCRATCH
//!     speed-up that is REQUIRED to be content-keyed ; same input always
//!     produces same output ; cache is per-thread to honor the V.3 (d)
//!     anti-surveil row "amplifier-cache thread-local ⊗ ¬ user-tracked").
//!   - NO seed-by-pixel-id (the input vector is the sole determiner ; if
//!     two pixels share `(world_pos, view_dir, grad)` they MUST share
//!     output, which is the natural consequence of the SDF's analytic
//!     structure).
//!
//!   The `flicker_stability_across_frames` test in `tests/`, the
//!   `amplifier_determinism` test, and the `kan_confidence_budget`
//!   test pin this contract.
//!
//! § BUDGET — 1.2 ms per-frame @ Quest-3
//!   `06_RENDERING_PIPELINE § Stage-7` declares the budget as 1.2 ms per
//!   frame at Quest-3-class hardware (1.0 ms @ Vision-Pro). The KAN-runtime
//!   spec `07_KAN_RUNTIME_SHADING § VII` shows the per-fragment cost
//!   decomposition :
//!
//!     - foveal-pixels (~25% of screen ≈ 520k @ 1080p) × ~50 ns/eval
//!       (CoopMatrix tier) × trigger-density 5%
//!       = 0.42 ms — well under the 1.2 ms budget at full-fovea.
//!     - mid-region pixels × half-amplitude (cheaper KAN) at 2×2
//!       resolution × ~50 ns/eval → another ~0.4 ms.
//!     - peripheral pixels — amplifier SKIPPED entirely, 0 ms.
//!
//!   The total Stage-7 budget thus has comfortable headroom on Quest-3
//!   even before persistent-tile residency (`07_KAN § V`) is applied. The
//!   crate's `cost_model.rs` re-establishes the same budget
//!   compile-time : if the configured detail-budget exceeds 1.2 ms it
//!   triggers an `AmplifierError::BudgetExceeded`.
//!
//! § INTEGRATION POINTS
//!   - **D116 (cssl-render-v2-sdf-raymarch)** — implements
//!     [`SdfRaymarchAmplifier`] for its `RayHit` type and calls
//!     `amplify_at_hit` from the bisection-refine path. When D116 has
//!     not landed, the trait's `MockSdfHit` reference impl exercises the
//!     amplifier against synthetic hits.
//!
//!   - **D118 (cssl-spectral-render / KAN-BRDF)** — consumes the
//!     amplifier's `micro_color_perturbation` output as a small additive
//!     spectral tint that feeds into the BRDF evaluator's M-coord
//!     pre-shift. This is the "shared-substrate" amortization claim from
//!     `07_AESTHETIC/00_EXOTICISM § VI § amortization-table` :
//!     paths-2+3 share the M-coord pipeline so the amplifier's tint
//!     emerges from the same KAN-network family that the BRDF queries.
//!
//!   - **D120 (gaze-collapse)** — produces the FoveaMask that this
//!     crate's [`DetailBudget`] consumes. The mask is one of three
//!     conditioning factors on the budget.
//!
//! § PRIME-DIRECTIVE POSTURE — `00_EXOTICISM § V.3 (d)`
//!   - **Transparency** : amplifier is deterministic ⊗ no-hidden-state.
//!     Every fragment's output is reproducible from the input alone.
//!   - **Consent** : N/A (geometry-detail is-not-Σ-state) per spec.
//!   - **Anti-surveil** : amplifier-cache thread-local ⊗ ¬ user-tracked.
//!     The optional speed-up cache key is `(quantized_pos, view_dir_proj)`
//!     ; never includes player-Φ.
//!   - **Reversibility** : same-input → same-output ⊗ frame-deterministic.
//!     Pinned by the `flicker_stability_across_frames` test.
//!   - **Sovereignty** : amplifier ¬ blurs-other-Sovereign's body @
//!     Σ-private-region. The Σ-mask consultation in
//!     `FractalAmplifier::amplify` enforces this before any KAN-eval
//!     fires : Σ-private fragments fall back to base-SDF.

#![forbid(unsafe_code)]
// § Style allowances for substrate-runtime numerical code :
// - many_single_char_names : test fixtures using x/y/z/p/v/g/etc as natural-domain bindings
// - suboptimal_flops : explicit dot-products vs sum-of-products is a precision/portability concern
//   that we don't want clippy second-guessing in this layer.
// - float_cmp : float fixtures use exact-equality where the comparison is intentional
//   (e.g. checking that `EPSILON_DISP` saturation produces an exact +EPSILON_DISP).
//   The amplifier's determinism contract REQUIRES bit-exact equality across calls, so
//   exact float comparison is the right tool.
// - manual_range_contains : range-contains pattern in the budget.rs validation is more
//   readable than `!(0.0..=1.0).contains(...)` for the multi-clause guard.
// - should_implement_trait : `MicroColor::add` and similar take Self-by-value with
//   explicit naming for readability ; the std::ops::Add impl would force a different
//   call shape that doesn't match the rest of the API.
// - assertions_on_constants : compile-time-evaluated assertions document the invariant
//   even when clippy can constant-fold them.
// - items_after_statements : we sometimes inline `use` statements close to their use
//   site for readability in tests.
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::float_cmp)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::items_after_statements)]
// § cast_possible_wrap / cast_precision_loss : the recursion depth is u8-bounded
//   to MAX_RECURSION_DEPTH = 5, so the i32 cast cannot overflow ; the cost-model
//   N as f32 in tests is a small-N count bounded by the budget so precision-loss
//   is not material.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::imprecise_flops)]

pub mod amplifier;
pub mod budget;
pub mod cost_model;
pub mod determinism;
pub mod fragment;
pub mod recursion;
pub mod sdf_trait;
pub mod sigma_mask;

// § Top-level re-exports — the canonical public surface for downstream
//   consumers (D116 raymarch + D118 spectral-render + D120 gaze).
pub use amplifier::{
    AmplifierError, FractalAmplifier, KAN_AMPLIFIER_INPUT_DIM, MICRO_COLOR_OUTPUT_DIM,
    MICRO_DISPLACEMENT_OUTPUT_DIM, MICRO_ROUGHNESS_OUTPUT_DIM,
};
pub use budget::{DetailBudget, FoveaTier, BUDGET_FULL, BUDGET_MID_HALF, BUDGET_PERIPHERAL_SKIP};
pub use cost_model::{CostModel, COST_BUDGET_QUEST3_MS, COST_BUDGET_VISION_PRO_MS};
pub use determinism::{DeterminismCheck, DeterminismError};
pub use fragment::{AmplifiedFragment, MicroColor};
pub use recursion::{DetailLevel, RecursionError, RecursiveDetailLOD, MAX_RECURSION_DEPTH};
pub use sdf_trait::{MockSdfHit, SdfHitInfo, SdfRaymarchAmplifier};
pub use sigma_mask::{SigmaMaskCheck, SigmaPrivacy};

/// § Crate version sentinel — bumped when the trait surface OR the KAN-
///   network shape contract changes in a way that invalidates D116 / D118 /
///   D120 callers.
pub const FRACTAL_AMP_SURFACE_VERSION: u32 = 1;

/// § The canonical micro-displacement KAN-network input dimension. Per
///   `07_KAN_RUNTIME_SHADING § II § variant-table` the micro-displacement
///   variant is `KanNetwork<7, 1>` (input = `[pos.xyz | view.xy_proj |
///   grad.norm_2D]`, output = scalar displacement).
pub const KAN_AMP_INPUT_DIM: usize = 7;

/// § Budget slack-factor : the runtime-budget allows up to this fraction
///   of the configured per-frame budget to be consumed before
///   `AmplifierError::BudgetExceeded` fires. Set to 1.0 by default ; the
///   budget is exhausted when `cumulative_cost_ms >= configured_budget_ms`.
///   A degraded-mode fallback could relax this to 1.2 with a
///   visible-telemetry frame-marker, per `07_KAN § VII § degraded-mode`.
pub const BUDGET_SLACK_FACTOR: f32 = 1.0;
