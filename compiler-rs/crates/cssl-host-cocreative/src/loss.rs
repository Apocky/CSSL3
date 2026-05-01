// ══════════════════════════════════════════════════════════════════════════════
// § loss.rs · linear-loss surrogate + central-difference numerical gradient
// ══════════════════════════════════════════════════════════════════════════════
// § Spec : paradigm-6 · "loss · gradient via finite-differences (no autodiff
//   dep) · θ updated".
//
// § Linear-loss surrogate
//   Given θ ∈ ℝ^D · features φ ∈ ℝ^D · implicit-loss ℓ ∈ ℝ ,
//     L(θ ; φ , ℓ) = -ℓ · (θ · φ)
//
//   • Thumbs-up  ℓ = -1.0  →  L = +(θ·φ) · gradient pushes θ toward -φ
//                              ✗ wrong direction for "more like this"
//
//   Re-derivation : we want θ to predict the player's preference. Treat ℓ
//   as the *desired* dot-product output ; the loss is the squared-error or
//   linear-prediction error. We use the linear surrogate
//
//     L(θ ; φ , ℓ) = -(−ℓ) · (θ · φ) = ℓ · (θ · φ)   ← negate-ℓ-then-multiply
//
//   so that for thumbs-up (ℓ = -1.0) we get L = -(θ·φ) · gradient ∂L/∂θ = -φ
//   · standard SGD step θ ← θ - η·∇L = θ + η·φ · pushes θ TOWARD φ.
//   For thumbs-down (ℓ = +1.0) we get L = +(θ·φ) · ∇L = +φ · θ moves AWAY.
//   This matches the intended "co-creative" semantics : θ accumulates toward
//   feature-vectors associated with reward and away from those associated
//   with penalty.
//
// § finite-diff central gradient
//   ∂L/∂θ_i ≈ (L(θ + ε·e_i) − L(θ − ε·e_i)) / (2ε)
//   For our linear L this exactly recovers ∂L/∂θ_i = ℓ · φ_i ; the test
//   `grad_matches_analytic_for_linear` verifies this within fp tolerance.

/// Errors emitted by `linear_loss` / `finite_diff_grad`.
#[derive(Debug, Clone, PartialEq)]
pub enum LossErr {
    /// `theta` and `features` have differing dimensions.
    DimensionMismatch {
        /// Length of `theta`.
        theta_dim: usize,
        /// Length of `features`.
        features_dim: usize,
    },
    /// A computation produced a non-finite value (NaN or ±∞).
    NaN,
    /// `eps` was zero or non-finite.
    ZeroEps,
}

impl std::fmt::Display for LossErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DimensionMismatch { theta_dim, features_dim } => write!(
                f,
                "loss dimension mismatch : theta_dim={theta_dim} · features_dim={features_dim}"
            ),
            Self::NaN => write!(f, "loss / gradient produced a non-finite value"),
            Self::ZeroEps => write!(f, "finite-difference epsilon must be > 0 and finite"),
        }
    }
}

impl std::error::Error for LossErr {}

/// Linear loss surrogate L(θ ; φ , ℓ) = ℓ · (θ · φ).
///
/// • `theta`         · current bias-vector
/// • `features`      · scene feature-vector φ (must match `theta` length)
/// • `implicit_loss` · ℓ extracted from `FeedbackEvent::implicit_loss()`
///
/// # Errors
/// * `LossErr::DimensionMismatch` if `theta.len() != features.len()`.
/// * `LossErr::NaN` if any input is non-finite or the result is non-finite.
pub fn linear_loss(theta: &[f32], features: &[f32], implicit_loss: f32) -> Result<f32, LossErr> {
    if theta.len() != features.len() {
        return Err(LossErr::DimensionMismatch {
            theta_dim: theta.len(),
            features_dim: features.len(),
        });
    }
    if !implicit_loss.is_finite() {
        return Err(LossErr::NaN);
    }
    let mut dot = 0.0_f32;
    for i in 0..theta.len() {
        let a = theta[i];
        let b = features[i];
        if !a.is_finite() || !b.is_finite() {
            return Err(LossErr::NaN);
        }
        dot += a * b;
    }
    let l = implicit_loss * dot;
    if !l.is_finite() {
        return Err(LossErr::NaN);
    }
    Ok(l)
}

/// Central-difference numerical gradient of `linear_loss` w.r.t. θ.
///
/// Returns ∇θ L ∈ ℝ^D where D = `theta.len()`. For the linear surrogate this
/// is analytically `ℓ · φ`, but we compute it numerically so the same code
/// path can be reused if a non-linear surrogate is plugged in later.
///
/// `eps` is the perturbation magnitude (default `DEFAULT_EPS = 1e-3`).
///
/// # Errors
/// * `LossErr::ZeroEps` if `eps <= 0` or non-finite.
/// * `LossErr::DimensionMismatch` (propagated from `linear_loss`).
/// * `LossErr::NaN` if any intermediate non-finite value appears.
pub fn finite_diff_grad(
    theta: &[f32],
    features: &[f32],
    implicit_loss: f32,
    eps: f32,
) -> Result<Vec<f32>, LossErr> {
    if !eps.is_finite() || eps <= 0.0 {
        return Err(LossErr::ZeroEps);
    }
    if theta.len() != features.len() {
        return Err(LossErr::DimensionMismatch {
            theta_dim: theta.len(),
            features_dim: features.len(),
        });
    }
    let d = theta.len();
    let mut grad = vec![0.0_f32; d];
    let mut th_plus = theta.to_vec();
    let mut th_minus = theta.to_vec();
    let two_eps = 2.0 * eps;
    for i in 0..d {
        th_plus[i] = theta[i] + eps;
        th_minus[i] = theta[i] - eps;
        let l_plus = linear_loss(&th_plus, features, implicit_loss)?;
        let l_minus = linear_loss(&th_minus, features, implicit_loss)?;
        let g = (l_plus - l_minus) / two_eps;
        if !g.is_finite() {
            return Err(LossErr::NaN);
        }
        grad[i] = g;
        // restore for next iteration
        th_plus[i] = theta[i];
        th_minus[i] = theta[i];
    }
    Ok(grad)
}

// ══════════════════════════════════════════════════════════════════════════════
// § tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// θ = 0 → L = 0 regardless of features / implicit-loss.
    #[test]
    fn zero_theta_zero_loss() {
        let theta = vec![0.0_f32; 4];
        let phi = vec![0.5, -0.5, 0.25, -0.25];
        let l = linear_loss(&theta, &phi, 1.0).unwrap();
        assert!(l.abs() < 1.0e-6);
        let l2 = linear_loss(&theta, &phi, -1.0).unwrap();
        assert!(l2.abs() < 1.0e-6);
    }

    /// thumbs-down on θ aligned-with φ → positive loss.
    #[test]
    fn positive_loss_on_disagreement() {
        let theta = vec![1.0_f32, 0.0, 0.0];
        let phi = vec![1.0_f32, 0.0, 0.0];
        // ℓ = +1.0 (thumbs-down) · θ·φ = 1.0 · L = +1.0
        let l = linear_loss(&theta, &phi, 1.0).unwrap();
        assert!(l > 0.0);
        // ℓ = -1.0 (thumbs-up) · L = -1.0
        let l2 = linear_loss(&theta, &phi, -1.0).unwrap();
        assert!(l2 < 0.0);
    }

    /// φ = 0 → gradient = 0 (no signal).
    #[test]
    fn zero_grad_at_zero_features() {
        let theta = vec![0.1_f32, 0.2, 0.3];
        let phi = vec![0.0_f32; 3];
        let g = finite_diff_grad(&theta, &phi, 1.0, 1.0e-3).unwrap();
        for v in g {
            assert!(v.abs() < 1.0e-5);
        }
    }

    /// finite-diff gradient matches analytic ∇L = ℓ · φ.
    #[test]
    fn grad_matches_analytic_for_linear() {
        let theta = vec![0.1_f32, -0.2, 0.3, 0.4];
        let phi = vec![0.5_f32, 0.6, -0.7, 0.8];
        let ell = -1.0_f32; // thumbs-up
        let g = finite_diff_grad(&theta, &phi, ell, 1.0e-3).unwrap();
        for i in 0..phi.len() {
            let analytic = ell * phi[i];
            assert!(
                (g[i] - analytic).abs() < 1.0e-3,
                "i={i} numeric={} analytic={analytic}",
                g[i]
            );
        }
    }

    /// eps = 0 (or negative / NaN) → ZeroEps error, not panic.
    #[test]
    fn zero_eps_rejected() {
        let theta = vec![0.0_f32; 3];
        let phi = vec![0.0_f32; 3];
        assert!(matches!(finite_diff_grad(&theta, &phi, 1.0, 0.0), Err(LossErr::ZeroEps)));
        assert!(matches!(finite_diff_grad(&theta, &phi, 1.0, -1.0e-3), Err(LossErr::ZeroEps)));
        assert!(matches!(finite_diff_grad(&theta, &phi, 1.0, f32::NAN), Err(LossErr::ZeroEps)));
    }

    /// dimension mismatch surfaces as Result::Err, not panic.
    #[test]
    fn dim_mismatch_rejected() {
        let theta = vec![0.0_f32; 3];
        let phi = vec![0.0_f32; 4];
        assert!(matches!(linear_loss(&theta, &phi, 1.0), Err(LossErr::DimensionMismatch { .. })));
        assert!(matches!(
            finite_diff_grad(&theta, &phi, 1.0, 1.0e-3),
            Err(LossErr::DimensionMismatch { .. })
        ));
    }

    /// NaN in inputs surfaces as LossErr::NaN, not panic.
    #[test]
    fn nan_in_inputs_rejected() {
        let theta = vec![f32::NAN, 0.0, 0.0];
        let phi = vec![0.5_f32, 0.5, 0.5];
        assert!(matches!(linear_loss(&theta, &phi, 1.0), Err(LossErr::NaN)));

        let theta_ok = vec![0.0_f32; 3];
        assert!(matches!(linear_loss(&theta_ok, &phi, f32::INFINITY), Err(LossErr::NaN)));
    }
}
