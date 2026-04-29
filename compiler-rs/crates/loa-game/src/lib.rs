//! LoA-game integration crate — M8 render-pipeline + per-stage timing.
//!
//! § T11-D158 (W-Jζ-2) : Per-stage frame-time instrumentation across the
//! 12 canonical render-pipeline stages.
//!
//! § ARCH
//!   - [`m8_integration`]            — pipeline orchestrator + 12 stage-passes
//!   - [`metrics_mock`]              — mock cssl-metrics surface (T11-D157
//!     not-yet-merged) ; trait `MetricsRegistry` permits swap-in once landed
//!   - per-stage namespace           — `pipeline.stage_N_<name>.frame_time_ms`
//!   - p50/p95/p99 via              — [`metrics_mock::Histogram`] aggregation
//!   - cross-frame trend             — last N=1024 frames per stage
//!   - feature-gate `metrics`        — zero-overhead-when-off
//!   - replay-determinism            — timing observe-only (¬state-mutation)
//!
//! § STAGES (canonical order, frozen-set)
//!   1.  embodiment              — body-tracking + IK retargeting
//!   2.  gaze_collapse           — saccade-driven Ω-collapse bias
//!   3.  omega_field_update      — Ω-field tier-resolution + Σ-mask
//!   4.  wave_solver             — ψ-evolution + spectral propagate
//!   5.  sdf_raymarch            — sphere-trace + foveated pixel-march
//!   6.  kan_brdf                — KAN spectral-BRDF eval per-fragment
//!   7.  fractal_amplifier       — RC-fractal detail injection
//!   8.  companion_semantic      — companion-perspective per-character
//!   9.  mise_en_abyme           — recursion-depth witnessed amplification
//!   10. tonemap                 — HDR → display ; fovea-tier compose
//!   11. motion_vec              — frame-N→N+1 motion vectors @ AppSW
//!   12. compose_xr_layers       — quad/cyl/cube layer compose per-eye
//!
//! § SPEC-CITE
//!   - DIAGNOSTIC_INFRA_PLAN.md § 3.3                         (per-stage timing inventory)
//!   - DIAGNOSTIC_INFRA_PLAN.md § 3.3.3 render-pipeline       (10 metrics, frozen-set)
//!   - DENSITY_BUDGET §V                                      (frame-time budgets per mode)
//!   - PRIME_DIRECTIVE.md § 11                                (attestation-discipline)

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// ── lint allowances : intentional patterns for instrumentation + tests ──
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
// `#[inline(always)]` on Timer + Histogram::record is load-bearing :
// it is a hard requirement for zero-overhead-when-feature-off ; the
// compiler must elide call-sites entirely. Override the clippy-perf
// suggestion which is unaware of this ABI commitment.
#![allow(clippy::inline_always)]
// Tests instantiate near-identical pass-pairs (p1a/p1b) by design to
// validate replay-determinism ; clippy-similar-names is noise here.
#![allow(clippy::similar_names)]
// Tests use `format!("{}-{}", a, b)` style ; uninlined-format-args is
// nursery-level and the format-style is stable + readable.
#![allow(clippy::uninlined_format_args)]
// Test-helpers that look like clones of trivial calls are intentional
// to validate Arc-shared state visible across handles.
#![allow(clippy::redundant_clone)]
// Synthetic-workload calculations include f32-precision drops that are
// scoped to test-fixtures ; not load-bearing for production timing.
#![allow(clippy::suboptimal_flops)]
// Some math expressions in synthetic workloads use cast_lossless that
// is correct but flagged ; suppress at crate level.
#![allow(clippy::cast_lossless)]

pub mod m8_integration;
pub mod metrics_mock;

pub use m8_integration::pipeline::Pipeline;
pub use m8_integration::Pass;
pub use metrics_mock::{Histogram, MetricsRegistry, Timer};

/// Crate version exposed for scaffold-verification + diff-test reads.
pub const D158_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");
