//! § wired_cocreative — wrapper around `cssl-host-cocreative`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the gradient-descent feedback optimizer + bias-vector types
//!   so MCP tools can probe the optimizer dimensionality + observe
//!   feedback events without each call-site reaching across the path-dep.
//!
//! § wrapped surface
//!   - [`CocreativeOptimizer`] — observe → step → checkpoint driver.
//!   - [`BiasVector`] — θ ∈ ℝ^D storage + dot/norm/clip primitives.
//!   - [`FeedbackEvent`] / [`FeedbackKind`] — input event shape.
//!   - [`StepReport`] / [`LossErr`] / [`BiasErr`] — result envelopes.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure-math only.

pub use cssl_host_cocreative::{
    finite_diff_grad, linear_loss, BiasErr, BiasVector, CocreativeOptimizer, FeedbackEvent,
    FeedbackKind, LossErr, StepReport, DEFAULT_EPS, DEFAULT_LR, DEFAULT_MOMENTUM_DECAY,
};

/// Convenience : the dimensionality of the optimizer's underlying bias
/// vector. Used by the `cocreative.bias_dim` MCP tool to surface a basic
/// shape probe ; returns 0 for an `Option::None` input (no optimizer wired).
#[must_use]
pub fn optimizer_dim(opt: Option<&CocreativeOptimizer>) -> usize {
    opt.map_or(0, |o| o.bias_vector().dim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_optimizer_yields_zero_dim() {
        assert_eq!(optimizer_dim(None), 0);
    }

    #[test]
    fn fresh_optimizer_dim_matches_constructor() {
        let opt = CocreativeOptimizer::new(16, DEFAULT_LR);
        assert_eq!(optimizer_dim(Some(&opt)), 16);
    }
}
