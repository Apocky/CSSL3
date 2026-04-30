//! § loss — common loss-functions over rendered Ω-field state.
//!
//! Implements scalar `forward(prediction, target) → f32` and gradient
//! `backward(prediction, target) → ∂L/∂prediction` for each loss kind.
//! The adjoint kernel calls `backward` to seed `∂L/∂L_final` before running
//! the reverse iteration.
//!
//! Available loss-kinds (per specs/36 § Use-cases) :
//!   - `MSE`        : ½‖p - t‖² ; standard scene-fitting loss
//!   - `L1`         : ‖p - t‖₁ ; robust to outliers (e.g. specular highlights)
//!   - `Perceptual` : weighted-channel MSE ; lets callers up-weight luminance
//!                    or chroma bands per the `PerceptualWeights` table
//!   - `Custom`     : caller supplies forward + backward closures (boxed)

use thiserror::Error;

/// Per-channel weights for `Perceptual` loss.
///
/// Defaults match the standard luminance-emphasis prior :
///   - luminance Y : 0.6
///   - chroma   Cb : 0.2
///   - chroma   Cr : 0.2
/// Callers can rebalance via constructor for spectral / specular emphasis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerceptualWeights {
    /// Luminance band weight.
    pub luma: f32,
    /// Chroma-Cb band weight.
    pub chroma_cb: f32,
    /// Chroma-Cr band weight.
    pub chroma_cr: f32,
}

impl Default for PerceptualWeights {
    fn default() -> Self {
        Self {
            luma: 0.6,
            chroma_cb: 0.2,
            chroma_cr: 0.2,
        }
    }
}

impl PerceptualWeights {
    /// Custom weights ; renormalized so the components sum to 1.
    #[must_use]
    pub fn new(luma: f32, chroma_cb: f32, chroma_cr: f32) -> Self {
        let s = (luma + chroma_cb + chroma_cr).max(f32::EPSILON);
        Self {
            luma: luma / s,
            chroma_cb: chroma_cb / s,
            chroma_cr: chroma_cr / s,
        }
    }
}

/// Loss-function discriminator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LossKind {
    /// Mean-squared-error : ½‖p - t‖² / N
    Mse,
    /// L1 / mean-absolute-error : ‖p - t‖₁ / N
    L1,
    /// Channel-weighted MSE. Length of (p, t) must be a multiple of 3
    /// (interpreted as interleaved YCbCr).
    Perceptual,
    /// Caller-supplied loss (used by tests + experimental losses).
    Custom,
}

/// Errors surfaced by loss evaluation.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LossError {
    #[error("dimension-mismatch: prediction {prediction} vs target {target}")]
    DimensionMismatch { prediction: u32, target: u32 },
    #[error("perceptual loss requires multiple-of-3 length, got {0}")]
    PerceptualBadLen(u32),
    #[error("empty prediction vector")]
    Empty,
}

/// Report from a single loss evaluation : scalar value + gradient buffer.
#[derive(Clone, Debug)]
pub struct LossReport {
    /// Scalar loss value.
    pub value: f32,
    /// ∂L/∂prediction — same length as the prediction vector.
    pub gradient: Vec<f32>,
}

/// Boxed forward-loss closure type used by `LossKind::Custom`.
type CustomForwardFn = Box<dyn Fn(&[f32], &[f32]) -> f32 + Send + Sync>;
/// Boxed backward-gradient closure type used by `LossKind::Custom`.
type CustomBackwardFn = Box<dyn Fn(&[f32], &[f32]) -> Vec<f32> + Send + Sync>;

/// Loss-function descriptor.
///
/// `Custom` carries optional closures ; `Mse` / `L1` / `Perceptual` are
/// implemented analytically.
pub struct LossFn {
    kind: LossKind,
    perceptual_weights: PerceptualWeights,
    custom_forward: Option<CustomForwardFn>,
    custom_backward: Option<CustomBackwardFn>,
}

impl core::fmt::Debug for LossFn {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LossFn")
            .field("kind", &self.kind)
            .field("perceptual_weights", &self.perceptual_weights)
            .field("custom_forward", &self.custom_forward.is_some())
            .field("custom_backward", &self.custom_backward.is_some())
            .finish()
    }
}

impl LossFn {
    /// MSE loss.
    #[must_use]
    pub fn mse() -> Self {
        Self {
            kind: LossKind::Mse,
            perceptual_weights: PerceptualWeights::default(),
            custom_forward: None,
            custom_backward: None,
        }
    }

    /// L1 loss.
    #[must_use]
    pub fn l1() -> Self {
        Self {
            kind: LossKind::L1,
            perceptual_weights: PerceptualWeights::default(),
            custom_forward: None,
            custom_backward: None,
        }
    }

    /// Perceptual (channel-weighted) MSE.
    #[must_use]
    pub fn perceptual(weights: PerceptualWeights) -> Self {
        Self {
            kind: LossKind::Perceptual,
            perceptual_weights: weights,
            custom_forward: None,
            custom_backward: None,
        }
    }

    /// Caller-supplied loss. Both `forward` and `backward` closures are
    /// required ; `backward(p,t)` must return a gradient of the same length
    /// as `p`.
    pub fn custom<F, B>(forward: F, backward: B) -> Self
    where
        F: Fn(&[f32], &[f32]) -> f32 + Send + Sync + 'static,
        B: Fn(&[f32], &[f32]) -> Vec<f32> + Send + Sync + 'static,
    {
        Self {
            kind: LossKind::Custom,
            perceptual_weights: PerceptualWeights::default(),
            custom_forward: Some(Box::new(forward)),
            custom_backward: Some(Box::new(backward)),
        }
    }

    /// Loss kind discriminator.
    #[must_use]
    pub fn kind(&self) -> LossKind {
        self.kind
    }

    /// Evaluate scalar loss + gradient. Single pass per call.
    pub fn evaluate(&self, prediction: &[f32], target: &[f32]) -> Result<LossReport, LossError> {
        if prediction.is_empty() {
            return Err(LossError::Empty);
        }
        if prediction.len() != target.len() {
            return Err(LossError::DimensionMismatch {
                prediction: prediction.len() as u32,
                target: target.len() as u32,
            });
        }
        match self.kind {
            LossKind::Mse => Ok(eval_mse(prediction, target)),
            LossKind::L1 => Ok(eval_l1(prediction, target)),
            LossKind::Perceptual => {
                if prediction.len() % 3 != 0 {
                    return Err(LossError::PerceptualBadLen(prediction.len() as u32));
                }
                Ok(eval_perceptual(prediction, target, self.perceptual_weights))
            }
            LossKind::Custom => {
                // Both closures are guaranteed by the constructor.
                let value = self
                    .custom_forward
                    .as_ref()
                    .map(|f| f(prediction, target))
                    .unwrap_or(0.0);
                let gradient = self
                    .custom_backward
                    .as_ref()
                    .map(|b| b(prediction, target))
                    .unwrap_or_else(|| vec![0.0; prediction.len()]);
                Ok(LossReport { value, gradient })
            }
        }
    }
}

fn eval_mse(p: &[f32], t: &[f32]) -> LossReport {
    let n = p.len() as f32;
    let mut sum_sq = 0.0_f64;
    let mut grad = Vec::with_capacity(p.len());
    for (pi, ti) in p.iter().zip(t.iter()) {
        let d = pi - ti;
        sum_sq += (d as f64) * (d as f64);
        // d/d pi of ½(pi-ti)²/N = (pi-ti)/N
        grad.push(d / n);
    }
    let value = (0.5 * sum_sq / n as f64) as f32;
    LossReport { value, gradient: grad }
}

fn eval_l1(p: &[f32], t: &[f32]) -> LossReport {
    let n = p.len() as f32;
    let mut sum = 0.0_f64;
    let mut grad = Vec::with_capacity(p.len());
    for (pi, ti) in p.iter().zip(t.iter()) {
        let d = pi - ti;
        sum += d.abs() as f64;
        // sub-gradient of |x|/N : sign(x)/N (zero gets 0 sub-gradient).
        let g = if d > 0.0 {
            1.0 / n
        } else if d < 0.0 {
            -1.0 / n
        } else {
            0.0
        };
        grad.push(g);
    }
    let value = (sum / n as f64) as f32;
    LossReport { value, gradient: grad }
}

fn eval_perceptual(p: &[f32], t: &[f32], w: PerceptualWeights) -> LossReport {
    let triplets = (p.len() / 3) as f32;
    let mut sum_sq = 0.0_f64;
    let mut grad = vec![0.0_f32; p.len()];
    let weights = [w.luma, w.chroma_cb, w.chroma_cr];
    for (i, ((pi, ti), gi)) in p.iter().zip(t.iter()).zip(grad.iter_mut()).enumerate() {
        let wi = weights[i % 3];
        let d = pi - ti;
        sum_sq += (wi as f64) * (d as f64) * (d as f64);
        *gi = wi * d / triplets;
    }
    let value = (0.5 * sum_sq / triplets as f64) as f32;
    LossReport { value, gradient: grad }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mse_zero_when_equal() {
        let l = LossFn::mse();
        let r = l.evaluate(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]).unwrap();
        assert!(r.value.abs() < 1e-6);
        for g in &r.gradient {
            assert!(g.abs() < 1e-6);
        }
    }

    #[test]
    fn mse_value_and_gradient_correct() {
        let l = LossFn::mse();
        let r = l.evaluate(&[2.0, 0.0], &[1.0, 0.0]).unwrap();
        // ½ * (1² + 0²) / 2 = 0.25
        assert!((r.value - 0.25).abs() < 1e-6, "value = {}", r.value);
        // grad = (p-t)/N = [0.5, 0.0]
        assert!((r.gradient[0] - 0.5).abs() < 1e-6);
        assert!(r.gradient[1].abs() < 1e-6);
    }

    #[test]
    fn l1_value_correct() {
        let l = LossFn::l1();
        let r = l.evaluate(&[2.0, -1.0], &[0.0, 0.0]).unwrap();
        // (2 + 1) / 2 = 1.5
        assert!((r.value - 1.5).abs() < 1e-6);
        // gradient = [+0.5, -0.5]
        assert!((r.gradient[0] - 0.5).abs() < 1e-6);
        assert!((r.gradient[1] + 0.5).abs() < 1e-6);
    }

    #[test]
    fn perceptual_renormalizes() {
        let w = PerceptualWeights::new(2.0, 1.0, 1.0);
        assert!((w.luma + w.chroma_cb + w.chroma_cr - 1.0).abs() < 1e-6);
    }

    #[test]
    fn perceptual_rejects_bad_length() {
        let l = LossFn::perceptual(PerceptualWeights::default());
        // Length 4 isn't a multiple of 3 ; equality check passes BEFORE the
        // multiple-of-3 check, so use matching lengths and trigger the
        // dedicated error.
        let r = l.evaluate(&[0.0, 0.0, 0.0, 0.0], &[0.0, 0.0, 0.0, 0.0]);
        assert!(matches!(r, Err(LossError::PerceptualBadLen(4))));
    }

    #[test]
    fn dimension_mismatch_surfaces() {
        let l = LossFn::mse();
        let r = l.evaluate(&[0.0, 0.0], &[0.0]);
        assert!(matches!(
            r,
            Err(LossError::DimensionMismatch { prediction: 2, target: 1 })
        ));
    }

    #[test]
    fn custom_loss_threads_through() {
        let l = LossFn::custom(
            |_, _| 7.0,
            |p, _| vec![0.5; p.len()],
        );
        let r = l.evaluate(&[1.0, 2.0], &[0.0, 0.0]).unwrap();
        assert_eq!(r.value, 7.0);
        assert_eq!(r.gradient, vec![0.5, 0.5]);
    }
}
