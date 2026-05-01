// ══════════════════════════════════════════════════════════════════════════════
// § cssl-host-cocreative · sixth-paradigm bias-vector optimizer
// ══════════════════════════════════════════════════════════════════════════════
// § Spec : specs/grand-vision/01_PARADIGMS.csl § paradigm-6 = "co-creative"
//
// § Thesis · the substrate adjusts toward what the player LIKES rather than
//   what was authored. A continuous bias-vector θ ∈ ℝ^D (D ≈ 16-32) modulates
//   spontaneous-condensation seed-cells. Player feedback events
//   (thumbs-up · thumbs-down · scalar score · comment) carry an implicit-loss
//   signal ; gradient-descent via finite-differences updates θ ; subsequent
//   spontaneous-condensation queries the updated bias.
//
// § Invariants
//   • NO autodiff dependency · finite-difference central-gradient only
//   • NO ndarray / nalgebra · `Vec<f32>` storage only
//   • All loss/gradient paths NaN-guarded
//   • #![forbid(unsafe_code)] · NO panics in library code (errors via Result)
//   • All public types serde-roundtrippable
//
// § Modules
//   • bias       · `BiasVector` — θ storage + dot/norm/clip primitives
//   • feedback   · `FeedbackEvent` + `FeedbackKind` + implicit-loss extraction
//   • loss       · linear-loss surrogate + finite-diff gradient
//   • optimizer  · `CocreativeOptimizer` — observe → step → checkpoint
//
// § Wire-up · workspace glob auto-discovers ; loa-host wiring deferred to wave-5

#![forbid(unsafe_code)]
#![doc = "Co-creative bias-vector optimizer for paradigm-6 spontaneous-condensation seed bias."]

pub mod bias;
pub mod feedback;
pub mod loss;
pub mod optimizer;

// re-exports : flat top-level surface for downstream callers
pub use bias::{BiasErr, BiasVector};
pub use feedback::{FeedbackEvent, FeedbackKind};
pub use loss::{finite_diff_grad, linear_loss, LossErr};
pub use optimizer::{CocreativeOptimizer, StepReport};

/// Default finite-difference epsilon for central-gradient computation.
pub const DEFAULT_EPS: f32 = 1.0e-3;

/// Default learning-rate for the co-creative optimizer.
pub const DEFAULT_LR: f32 = 1.0e-2;

/// Default momentum-decay coefficient.
pub const DEFAULT_MOMENTUM_DECAY: f32 = 0.9;
