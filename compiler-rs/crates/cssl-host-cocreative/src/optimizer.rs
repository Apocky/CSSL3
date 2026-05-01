// ══════════════════════════════════════════════════════════════════════════════
// § optimizer.rs · CocreativeOptimizer — observe → step → checkpoint
// ══════════════════════════════════════════════════════════════════════════════
// § Spec : paradigm-6 · "θ updated · serializable session checkpoints".
//
// § Update rule · vanilla SGD with classical momentum :
//
//     g_t   = ∇θ L(θ_t ; φ_t , ℓ_t)        ← finite-diff central gradient
//     v_t   = β · v_{t-1} + g_t            ← momentum buffer (β = mom_decay)
//     θ_t+1 = θ_t − η · v_t                ← parameter update (η = lr)
//
//   The momentum-decay default is `DEFAULT_MOMENTUM_DECAY = 0.9`. After every
//   update we clip θ to the unit ball so spontaneous-condensation seed-cells
//   see a bounded bias-vector regardless of feedback magnitude.
//
// § Append-only history · every observed `FeedbackEvent` is retained for
//   replay / debugging. The `step()` method pops semantically by advancing
//   the internal step-cursor — events are NEVER deleted ; checkpointing the
//   optimizer captures the full feedback-trace.

use serde::{Deserialize, Serialize};

use crate::bias::BiasVector;
use crate::feedback::FeedbackEvent;
use crate::loss::{finite_diff_grad, linear_loss, LossErr};
use crate::{DEFAULT_EPS, DEFAULT_MOMENTUM_DECAY};

/// Per-step diagnostic report returned by `CocreativeOptimizer::step`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepReport {
    /// Linear loss at θ_t before the update.
    pub loss: f32,
    /// L2 norm of the gradient applied at this step.
    pub grad_l2: f32,
    /// L2 norm of θ AFTER the update (post-clip).
    pub theta_l2: f32,
    /// Monotonic step-counter (number of steps taken so far, including this one).
    pub step_idx: u64,
}

/// Sixth-paradigm co-creative optimizer.
///
/// Maintains a `BiasVector` θ ∈ ℝ^D, a momentum buffer v ∈ ℝ^D, and an
/// append-only history of `FeedbackEvent`. Each call to `step()` consumes
/// the next un-processed feedback event and applies a momentum-SGD update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CocreativeOptimizer {
    theta: BiasVector,
    lr: f32,
    momentum: Vec<f32>,
    mom_decay: f32,
    eps: f32,
    history: Vec<FeedbackEvent>,
    /// Index of the next un-processed event in `history`.
    cursor: usize,
    step_count: u64,
}

impl CocreativeOptimizer {
    /// Construct a fresh optimizer with zero θ, zero momentum, empty history.
    ///
    /// `dim` is the bias-vector dimensionality (paradigm-6 spec recommends
    /// D ∈ [16, 32]). `lr` is the learning rate ; pass `DEFAULT_LR` if unsure.
    #[must_use]
    pub fn new(dim: usize, lr: f32) -> Self {
        Self {
            theta: BiasVector::new(dim),
            lr,
            momentum: vec![0.0; dim],
            mom_decay: DEFAULT_MOMENTUM_DECAY,
            eps: DEFAULT_EPS,
            history: Vec::new(),
            cursor: 0,
            step_count: 0,
        }
    }

    /// Override the momentum-decay coefficient (default = 0.9).
    pub fn set_momentum_decay(&mut self, decay: f32) {
        self.mom_decay = decay;
    }

    /// Override the finite-difference epsilon (default = 1e-3).
    pub fn set_eps(&mut self, eps: f32) {
        self.eps = eps;
    }

    /// Append a feedback event to the history.
    ///
    /// Does NOT trigger a parameter update — call `step()` to consume it.
    pub fn observe(&mut self, event: FeedbackEvent) {
        self.history.push(event);
    }

    /// Process the next un-processed feedback event :
    /// 1. Compute loss + finite-diff gradient against current θ.
    /// 2. Update momentum buffer and θ.
    /// 3. Clip θ to unit ball.
    ///
    /// # Errors
    /// * `LossErr::DimensionMismatch` if the event's `scene_features` length
    ///   differs from θ's dimension.
    /// * `LossErr::NaN` if any intermediate value is non-finite.
    /// * `LossErr::ZeroEps` should be impossible if `set_eps` is not abused.
    ///
    /// Returns `Ok(None)` (wrapped as `Err(EmptyHistory)`) when there is
    /// nothing to consume — represented here as `LossErr::DimensionMismatch
    /// { theta_dim: 0, features_dim: 0 }` would be misleading, so we use a
    /// sentinel via `step_optional` for the no-op case. The public `step()`
    /// returns `Err(LossErr::NaN)` is reserved for actual numerical issues ;
    /// the empty-history case is reported as `Err(LossErr::DimensionMismatch
    /// { theta_dim: dim, features_dim: 0 })` because there are no features.
    pub fn step(&mut self) -> Result<StepReport, LossErr> {
        let dim = self.theta.dim();
        if self.cursor >= self.history.len() {
            // No event to consume. Report as a dimension-0 mismatch since we
            // have no features to align against. Callers should use
            // `pending_events()` to gate before calling step().
            return Err(LossErr::DimensionMismatch {
                theta_dim: dim,
                features_dim: 0,
            });
        }
        // Find the next event with quantitative implicit-loss. Skip
        // comment-only events (which carry no scalar signal).
        loop {
            if self.cursor >= self.history.len() {
                return Err(LossErr::DimensionMismatch {
                    theta_dim: dim,
                    features_dim: 0,
                });
            }
            let event = &self.history[self.cursor];
            if event.scene_features.len() != dim {
                return Err(LossErr::DimensionMismatch {
                    theta_dim: dim,
                    features_dim: event.scene_features.len(),
                });
            }
            match event.implicit_loss() {
                Some(ell) => {
                    let phi = event.scene_features.clone();
                    self.cursor += 1;
                    return self.apply_update(&phi, ell);
                }
                None => {
                    // Skip this comment-only event and advance the cursor.
                    self.cursor += 1;
                }
            }
        }
    }

    /// Internal helper : apply one momentum-SGD update.
    fn apply_update(&mut self, phi: &[f32], ell: f32) -> Result<StepReport, LossErr> {
        let theta_slice = self.theta.theta();
        let loss = linear_loss(theta_slice, phi, ell)?;
        let grad = finite_diff_grad(theta_slice, phi, ell, self.eps)?;
        // Update momentum + θ in-place
        let mut grad_sq_sum = 0.0_f32;
        for (m, g) in self.momentum.iter_mut().zip(grad.iter()) {
            *m = self.mom_decay.mul_add(*m, *g);
            grad_sq_sum += g * g;
        }
        let grad_l2 = grad_sq_sum.sqrt();
        {
            let theta_mut = self.theta.theta_mut();
            for (t, m) in theta_mut.iter_mut().zip(self.momentum.iter()) {
                let new_val = (-self.lr).mul_add(*m, *t);
                if !new_val.is_finite() {
                    return Err(LossErr::NaN);
                }
                *t = new_val;
            }
        }
        self.theta.clip_to_unit();
        let theta_l2 = self.theta.norm_l2();
        self.step_count += 1;
        Ok(StepReport {
            loss,
            grad_l2,
            theta_l2,
            step_idx: self.step_count,
        })
    }

    /// Number of feedback events still un-processed.
    #[must_use]
    pub fn pending_events(&self) -> usize {
        self.history.len().saturating_sub(self.cursor)
    }

    /// Read-only view of θ.
    #[must_use]
    pub fn theta(&self) -> &[f32] {
        self.theta.theta()
    }

    /// Read-only view of the bias-vector struct.
    #[must_use]
    pub fn bias_vector(&self) -> &BiasVector {
        &self.theta
    }

    /// Read-only view of the full append-only feedback history.
    #[must_use]
    pub fn history(&self) -> &[FeedbackEvent] {
        &self.history
    }

    /// Number of update-steps taken since construction.
    #[must_use]
    pub fn step_count(&self) -> u64 {
        self.step_count
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// § tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::{FeedbackEvent, FeedbackKind};

    /// step() on empty history returns Err, not panic, and does NOT modify θ.
    #[test]
    fn empty_history_no_step() {
        let mut opt = CocreativeOptimizer::new(4, 0.01);
        let before = opt.theta().to_vec();
        let r = opt.step();
        assert!(r.is_err());
        let after = opt.theta().to_vec();
        assert_eq!(before, after);
        assert_eq!(opt.step_count(), 0);
    }

    /// A single thumbs-up moves θ toward φ (cosine similarity increases).
    #[test]
    fn single_feedback_updates_theta() {
        let dim = 4;
        let mut opt = CocreativeOptimizer::new(dim, 0.1);
        let phi = vec![1.0_f32, 0.0, 0.0, 0.0];
        opt.observe(FeedbackEvent::thumbs_up(0, "scene:liked", phi));
        let report = opt.step().unwrap();
        // Loss before update : θ=0 · φ = 0 · ℓ=-1 → L = 0
        assert!(report.loss.abs() < 1.0e-5);
        // After update θ should be moved in the +φ direction
        let theta = opt.theta();
        assert!(theta[0] > 0.0, "theta[0]={} should be positive after thumbs-up on +x", theta[0]);
        assert!(theta[1].abs() < 1.0e-4);
        assert_eq!(opt.step_count(), 1);
    }

    /// Thumbs-down on a feature pushes θ AWAY from it.
    #[test]
    fn negative_feedback_pushes_away() {
        let dim = 3;
        let mut opt = CocreativeOptimizer::new(dim, 0.1);
        let phi = vec![1.0_f32, 0.0, 0.0];
        opt.observe(FeedbackEvent::thumbs_down(0, "scene:disliked", phi));
        opt.step().unwrap();
        let theta = opt.theta();
        assert!(theta[0] < 0.0, "theta[0]={} should be negative after thumbs-down on +x", theta[0]);
    }

    /// Repeated thumbs-up on the SAME feature accumulates momentum.
    #[test]
    fn momentum_accumulates() {
        let dim = 3;
        let mut opt = CocreativeOptimizer::new(dim, 0.05);
        let phi = vec![1.0_f32, 0.0, 0.0];
        opt.observe(FeedbackEvent::thumbs_up(0, "x", phi.clone()));
        opt.observe(FeedbackEvent::thumbs_up(1, "x", phi));
        let r1 = opt.step().unwrap();
        let theta_after_1 = opt.theta()[0];
        let r2 = opt.step().unwrap();
        let theta_after_2 = opt.theta()[0];
        // theta should have grown more per-step due to momentum carry-over
        let delta_1 = theta_after_1; // 0 → theta_after_1
        let delta_2 = theta_after_2 - theta_after_1;
        assert!(delta_2 > delta_1 * 0.99, "momentum should make step-2 movement >= step-1 (d1={delta_1} d2={delta_2})");
        assert!(r1.grad_l2 > 0.0);
        assert!(r2.grad_l2 > 0.0);
    }

    /// lr = 0 → θ never changes regardless of feedback.
    #[test]
    fn lr_zero_no_change() {
        let dim = 4;
        let mut opt = CocreativeOptimizer::new(dim, 0.0);
        let phi = vec![0.5_f32, -0.5, 0.5, -0.5];
        opt.observe(FeedbackEvent::thumbs_up(0, "x", phi.clone()));
        opt.observe(FeedbackEvent::thumbs_down(1, "y", phi));
        let _ = opt.step().unwrap();
        let _ = opt.step().unwrap();
        for &v in opt.theta() {
            assert!(v.abs() < 1.0e-7, "lr=0 must keep theta at zero, got {v}");
        }
    }

    /// Multiple thumbs-up on the same feature drives θ toward unit-φ.
    #[test]
    fn multiple_feedback_converges_toward_features() {
        let dim = 3;
        let mut opt = CocreativeOptimizer::new(dim, 0.1);
        let phi = vec![1.0_f32, 0.0, 0.0];
        for k in 0..30 {
            opt.observe(FeedbackEvent::thumbs_up(k, "x", phi.clone()));
            opt.step().unwrap();
        }
        let theta = opt.theta();
        // After clipping to unit-ball, θ should align with φ
        assert!(theta[0] > 0.5, "after 30 👍 on +x, theta[0]={} should be > 0.5", theta[0]);
        assert!(theta[1].abs() < 1.0e-2);
        assert!(theta[2].abs() < 1.0e-2);
        // norm capped at 1
        let n: f32 = theta.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(n <= 1.0 + 1.0e-5);
    }

    /// Comment-only events are skipped without producing a step.
    #[test]
    fn comment_only_event_skipped() {
        let dim = 3;
        let mut opt = CocreativeOptimizer::new(dim, 0.1);
        opt.observe(FeedbackEvent {
            ts_micros: 0,
            kind: FeedbackKind::Comment("nice".into()),
            target_label: "x".into(),
            scene_features: vec![1.0, 0.0, 0.0],
        });
        // Only a comment in history → step() consumes it but produces nothing,
        // returning the empty-history sentinel.
        let r = opt.step();
        assert!(r.is_err());
        assert_eq!(opt.step_count(), 0);
        // pending_events should now be 0 (comment was consumed)
        assert_eq!(opt.pending_events(), 0);
    }

    /// Optimizer state survives serde roundtrip verbatim.
    #[test]
    fn serde_roundtrip() {
        let mut opt = CocreativeOptimizer::new(4, 0.05);
        let phi = vec![0.5_f32, 0.5, 0.0, 0.0];
        opt.observe(FeedbackEvent::thumbs_up(0, "x", phi));
        opt.step().unwrap();
        let s = serde_json::to_string(&opt).unwrap();
        let r: CocreativeOptimizer = serde_json::from_str(&s).unwrap();
        assert_eq!(opt.theta(), r.theta());
        assert_eq!(opt.history(), r.history());
        assert_eq!(opt.step_count(), r.step_count());
    }

    /// Dimension mismatch in scene_features surfaces as Err.
    #[test]
    fn dim_mismatch_in_event_rejected() {
        let mut opt = CocreativeOptimizer::new(3, 0.1);
        let phi_wrong = vec![1.0_f32, 0.0]; // dim 2 vs expected 3
        opt.observe(FeedbackEvent::thumbs_up(0, "x", phi_wrong));
        let r = opt.step();
        assert!(matches!(r, Err(LossErr::DimensionMismatch { .. })));
    }
}
