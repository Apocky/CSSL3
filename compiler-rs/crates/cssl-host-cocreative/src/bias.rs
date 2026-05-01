// ══════════════════════════════════════════════════════════════════════════════
// § bias.rs · BiasVector — θ ∈ ℝ^D storage + dot / l2-norm / unit-clip
// ══════════════════════════════════════════════════════════════════════════════
// § Spec : paradigm-6 § "continuous bias-vector that modulates spontaneous
//   condensation seed-cells". Storage is `Vec<f32>` (no ndarray dep). The
//   optimizer-side gradient-descent loop holds the canonical θ ; this module
//   provides the primitive operations.

use serde::{Deserialize, Serialize};

/// Errors emitted by `BiasVector` operations.
///
/// Library code never panics — dimension mismatches surface as `Result::Err`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BiasErr {
    /// `theta` and `x` have differing dimensions.
    DimensionMismatch {
        /// Length of `theta`.
        theta_dim: usize,
        /// Length of the operand slice.
        operand_dim: usize,
    },
}

impl std::fmt::Display for BiasErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DimensionMismatch { theta_dim, operand_dim } => write!(
                f,
                "BiasVector dimension mismatch : theta_dim={theta_dim} · operand_dim={operand_dim}"
            ),
        }
    }
}

impl std::error::Error for BiasErr {}

/// Continuous bias-vector θ ∈ ℝ^D modulating spontaneous-condensation seeds.
///
/// `theta.len() == dim` is an invariant maintained by every public method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BiasVector {
    theta: Vec<f32>,
    dim: usize,
}

impl BiasVector {
    /// Create a new zero-initialized `BiasVector` of dimension `dim`.
    #[must_use]
    pub fn new(dim: usize) -> Self {
        Self { theta: vec![0.0; dim], dim }
    }

    /// Construct from an existing slice. The dimension is inferred.
    #[must_use]
    pub fn from_slice(theta: &[f32]) -> Self {
        Self { theta: theta.to_vec(), dim: theta.len() }
    }

    /// Dimension D of θ.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Read-only view of θ.
    #[must_use]
    pub fn theta(&self) -> &[f32] {
        &self.theta
    }

    /// Mutable view of θ. The caller MUST NOT change `theta.len()`.
    pub fn theta_mut(&mut self) -> &mut [f32] {
        &mut self.theta
    }

    /// Inner product θ · x.
    ///
    /// # Errors
    /// Returns `BiasErr::DimensionMismatch` if `x.len() != self.dim()`.
    pub fn dot(&self, x: &[f32]) -> Result<f32, BiasErr> {
        if x.len() != self.dim {
            return Err(BiasErr::DimensionMismatch {
                theta_dim: self.dim,
                operand_dim: x.len(),
            });
        }
        let mut acc = 0.0_f32;
        for (a, b) in self.theta.iter().zip(x.iter()) {
            acc += a * b;
        }
        Ok(acc)
    }

    /// Euclidean (L2) norm ‖θ‖.
    #[must_use]
    pub fn norm_l2(&self) -> f32 {
        let sum_sq: f32 = self.theta.iter().map(|t| t * t).sum();
        sum_sq.sqrt()
    }

    /// Rescale θ to lie inside the unit ball if ‖θ‖ > 1.
    ///
    /// Stable when ‖θ‖ ≤ 1 (no-op) and when θ is all-zero (no-op).
    /// NaN entries are clamped to zero before rescaling so the bias-vector
    /// can never propagate non-finite values into downstream sampling.
    pub fn clip_to_unit(&mut self) {
        // NaN guard : zero-out non-finite entries (keeps invariant ‖θ‖ finite)
        for v in &mut self.theta {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
        let n = self.norm_l2();
        if n > 1.0 && n.is_finite() {
            let inv = 1.0 / n;
            for v in &mut self.theta {
                *v *= inv;
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// § tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// new(D) yields a length-D zero-vector.
    #[test]
    fn new_zeros() {
        let b = BiasVector::new(8);
        assert_eq!(b.dim(), 8);
        assert_eq!(b.theta().len(), 8);
        assert!(b.theta().iter().all(|&v| v == 0.0));
    }

    /// from_slice copies values + sets dim.
    #[test]
    fn from_slice_roundtrip() {
        let xs = [0.1_f32, -0.2, 0.3];
        let b = BiasVector::from_slice(&xs);
        assert_eq!(b.dim(), 3);
        assert_eq!(b.theta(), &xs);
    }

    /// dot computes Σ θ_i x_i correctly.
    #[test]
    fn dot_product() {
        let b = BiasVector::from_slice(&[1.0, 2.0, 3.0]);
        let x = [4.0_f32, 5.0, 6.0];
        let d = b.dot(&x).unwrap();
        // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
        assert!((d - 32.0).abs() < 1.0e-6);
    }

    /// norm_l2 = sqrt(Σ θ_i²).
    #[test]
    fn l2_norm_correct() {
        let b = BiasVector::from_slice(&[3.0, 4.0]); // → 5.0
        let n = b.norm_l2();
        assert!((n - 5.0).abs() < 1.0e-6);
    }

    /// clip_to_unit rescales when ‖θ‖ > 1 and is no-op when ≤ 1.
    #[test]
    fn clip_rescales() {
        let mut b = BiasVector::from_slice(&[3.0, 4.0]); // norm 5
        b.clip_to_unit();
        let n = b.norm_l2();
        assert!((n - 1.0).abs() < 1.0e-6);

        let mut c = BiasVector::from_slice(&[0.3, 0.4]); // norm 0.5
        let before = c.theta().to_vec();
        c.clip_to_unit();
        // unchanged (within fp tolerance)
        for (a, b) in before.iter().zip(c.theta().iter()) {
            assert!((a - b).abs() < 1.0e-6);
        }
    }

    /// dot rejects mismatched dimensions instead of panicking.
    #[test]
    fn dim_mismatch_rejected() {
        let b = BiasVector::new(3);
        let x = [1.0_f32, 2.0]; // wrong length
        let err = b.dot(&x).unwrap_err();
        match err {
            BiasErr::DimensionMismatch { theta_dim, operand_dim } => {
                assert_eq!(theta_dim, 3);
                assert_eq!(operand_dim, 2);
            }
        }
    }

    /// clip_to_unit zeros NaN entries before rescaling.
    #[test]
    fn clip_nan_guard() {
        let mut b = BiasVector::from_slice(&[f32::NAN, 0.5, 0.0]);
        b.clip_to_unit();
        for &v in b.theta() {
            assert!(v.is_finite());
        }
    }

    /// serde roundtrip preserves theta + dim.
    #[test]
    fn serde_roundtrip() {
        let b = BiasVector::from_slice(&[0.1_f32, -0.2, 0.3]);
        let s = serde_json::to_string(&b).unwrap();
        let r: BiasVector = serde_json::from_str(&s).unwrap();
        assert_eq!(b, r);
    }
}
