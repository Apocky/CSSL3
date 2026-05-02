//! § wired_cocreative — wrapper around `cssl-host-cocreative`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the gradient-descent feedback optimizer + bias-vector types
//!   so MCP tools can probe the optimizer dimensionality + observe
//!   feedback events without each call-site reaching across the path-dep.
//!
//! § Q-12 RESOLVED 2026-05-01 (Apocky-canonical) :
//!   verbatim : "Sovereign choice."
//!   binding-matrix : 6 archetypes × 4 roles (Collaborator-cell sovereign-revocable)
//!   default-fallback = Phantasia (archetype_id = 0) if-no-cap-set
//!   spec : Labyrinth of Apocalypse/systems/draconic_choice.csl
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

// § T11-W11-GM-DM-DEEPEN ------------------------------------------------
// Convert a persona-axes byte-vector into a co-author bias-vector seed.

#[must_use]
pub fn persona_axes_to_bias_seed(axes: [i8; 8]) -> [f32; 8] {
    let mut out = [0.0_f32; 8];
    for (i, &a) in axes.iter().enumerate() {
        out[i] = f32::from(a) / 100.0;
    }
    out
}

// ─── § Q-12 · Draconic-archetype binding-cap (Collaborator cell) ──────────
// Apocky 2026-05-01 verbatim : "Sovereign choice."

/// Default-fallback archetype-id for the Collaborator role · per Q-12.
pub const COCREATIVE_ARCHETYPE_FALLBACK: u8 = 0; // Phantasia

/// Resolve `archetype_id` to a valid archetype for Collaborator cell · falls
/// back to Phantasia(0) per Q-12 sovereign-choice.
#[must_use]
pub fn cocreative_resolve_archetype(archetype_id: u8) -> u8 {
    if archetype_id < crate::wired_dm::DRACONIC_ARCHETYPE_COUNT {
        archetype_id
    } else {
        COCREATIVE_ARCHETYPE_FALLBACK
    }
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

    // § T11-W11-GM-DM-DEEPEN
    #[test]
    fn persona_axes_to_bias_seed_normalizes() {
        let axes = [100i8, -100, 50, -50, 0, 25, -75, 10];
        let seed = persona_axes_to_bias_seed(axes);
        assert!((seed[0] - 1.0).abs() < 1e-6);
        assert!((seed[1] - (-1.0)).abs() < 1e-6);
        assert!((seed[2] - 0.5).abs() < 1e-6);
        assert!((seed[4] - 0.0).abs() < 1e-6);
        assert!((seed[5] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn persona_axes_zero_yields_zero_seed() {
        let axes = [0i8; 8];
        let seed = persona_axes_to_bias_seed(axes);
        assert_eq!(seed, [0.0_f32; 8]);
    }
}
