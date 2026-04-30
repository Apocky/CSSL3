//! § optimizer — SGD + Adam.
//!
//! Both optimizers consume gradients accumulated in `ParameterSet` (one
//! buffer per parameter) and write back updated values, respecting the
//! Σ-mask freeze flag.
//!
//! ## SGD
//! ```pseudo
//! v_t = momentum * v_{t-1} + grad
//! θ_t = θ_{t-1} - lr * lr_scale(kind) * v_t
//! ```
//!
//! ## Adam (Kingma+Ba 2015) — minor adaptation : per-parameter-kind lr_scale.
//! ```pseudo
//! m_t = β1 * m_{t-1} + (1-β1) * grad
//! v_t = β2 * v_{t-1} + (1-β2) * grad²
//! m̂   = m_t / (1 - β1^t)
//! v̂   = v_t / (1 - β2^t)
//! θ_t = θ_{t-1} - lr * lr_scale(kind) * m̂ / (sqrt(v̂) + ε)
//! ```
//!
//! Both optimizers also support a learning-rate schedule via
//! `lr_at(step)` ; the default is constant.

use crate::parameter::{ParameterError, ParameterId, ParameterSet};
use thiserror::Error;

/// Errors surfaced by the optimizers.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum OptimizerError {
    #[error(transparent)]
    Parameter(#[from] ParameterError),
    #[error("optimizer state-dimension mismatch on parameter {0}: expected {1}, got {2}")]
    StateDimensionMismatch(u32, u32, u32),
}

/// Report from one optimizer step : number of parameters updated, number
/// frozen-and-skipped, post-update gradient norm.
#[derive(Clone, Debug)]
pub struct StepReport {
    /// Number of parameters whose values were updated.
    pub updated: u32,
    /// Number of parameters skipped because they were frozen.
    pub frozen_skipped: u32,
    /// Pre-step gradient L2-norm (after accumulation).
    pub gradient_norm: f32,
    /// Optimizer step counter (post-step).
    pub step_index: u64,
}

/// Schedule for learning-rate as a function of step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LrSchedule {
    /// Constant learning rate.
    Constant,
    /// Exponential decay : `lr * decay.powi(step)`.
    Exponential { decay: f32 },
    /// Cosine annealing over `total` steps : lr * 0.5 * (1 + cos(π * step/total)).
    Cosine { total: u32 },
    /// Step-wise decay : `lr * gamma.powi(step / step_size)`.
    StepDecay { step_size: u32, gamma: f32 },
}

impl LrSchedule {
    /// Apply the schedule to `base_lr` for `step`.
    #[must_use]
    pub fn lr_at(self, base_lr: f32, step: u64) -> f32 {
        match self {
            Self::Constant => base_lr,
            Self::Exponential { decay } => base_lr * decay.powi(step as i32),
            Self::Cosine { total } => {
                if total == 0 {
                    return base_lr;
                }
                let frac = (step as f32 / total as f32).min(1.0);
                base_lr * 0.5 * (1.0 + (core::f32::consts::PI * frac).cos())
            }
            Self::StepDecay { step_size, gamma } => {
                if step_size == 0 {
                    return base_lr;
                }
                base_lr * gamma.powi((step as u32 / step_size) as i32)
            }
        }
    }
}

// ───────────────────────────────────────────── SGD ─────────────────────

/// SGD configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SgdConfig {
    /// Base learning rate.
    pub lr: f32,
    /// Momentum coefficient (0 = pure SGD, 0.9 typical).
    pub momentum: f32,
    /// Optional weight-decay (L2 regularization on parameter values).
    pub weight_decay: f32,
    /// Learning-rate schedule.
    pub schedule: LrSchedule,
}

impl Default for SgdConfig {
    fn default() -> Self {
        Self {
            lr: 0.01,
            momentum: 0.9,
            weight_decay: 0.0,
            schedule: LrSchedule::Constant,
        }
    }
}

/// SGD optimizer with per-parameter velocity buffers.
#[derive(Clone, Debug)]
pub struct SgdOptimizer {
    cfg: SgdConfig,
    /// One velocity buffer per parameter, indexed by `ParameterId`.
    velocity: Vec<Vec<f32>>,
    step: u64,
}

impl SgdOptimizer {
    /// New optimizer initialized for `set`'s current parameter shapes.
    #[must_use]
    pub fn new(cfg: SgdConfig, set: &ParameterSet) -> Self {
        let velocity = set
            .params()
            .iter()
            .map(|p| vec![0.0_f32; p.shape.count as usize])
            .collect();
        Self {
            cfg,
            velocity,
            step: 0,
        }
    }

    /// Take one step : read gradients from `set`, write updates back.
    pub fn step(&mut self, set: &mut ParameterSet) -> Result<StepReport, OptimizerError> {
        let grad_norm = set.gradient_norm();
        let lr = self.cfg.schedule.lr_at(self.cfg.lr, self.step);
        let mut updated = 0_u32;
        let mut frozen = 0_u32;

        // Iterate by id rather than by &-borrow to avoid double-borrowing
        // `set`. Take a snapshot of (id, kind, frozen) to drive updates.
        let summary: Vec<(ParameterId, f32, bool, u32)> = set
            .params()
            .iter()
            .map(|p| (p.id, p.kind.default_lr_scale(), p.frozen, p.shape.count))
            .collect();

        for (id, lr_scale, is_frozen, count) in summary {
            if is_frozen {
                frozen += 1;
                continue;
            }
            let idx = id.raw() as usize;
            if self.velocity[idx].len() != count as usize {
                return Err(OptimizerError::StateDimensionMismatch(
                    id.raw(),
                    count,
                    self.velocity[idx].len() as u32,
                ));
            }

            // Compute update buffer in-place.
            let mut update = vec![0.0_f32; count as usize];
            {
                let g = set.gradient(id)?;
                let v = &mut self.velocity[idx];
                let p_vals = &set.get(id)?.values;
                for i in 0..count as usize {
                    let mut grad = g[i];
                    if self.cfg.weight_decay != 0.0 {
                        grad += self.cfg.weight_decay * p_vals[i];
                    }
                    v[i] = self.cfg.momentum * v[i] + grad;
                    update[i] = -lr * lr_scale * v[i];
                }
            }

            set.apply_update(id, &update)?;
            updated += 1;
        }

        self.step += 1;
        Ok(StepReport {
            updated,
            frozen_skipped: frozen,
            gradient_norm: grad_norm,
            step_index: self.step,
        })
    }

    /// Current step counter.
    #[must_use]
    pub fn step_index(&self) -> u64 {
        self.step
    }
}

// ───────────────────────────────────────────── Adam ────────────────────

/// Adam configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdamConfig {
    /// Base learning rate (typical 1e-3).
    pub lr: f32,
    /// β1 — first-moment decay (typical 0.9).
    pub beta1: f32,
    /// β2 — second-moment decay (typical 0.999).
    pub beta2: f32,
    /// ε — denominator stability (typical 1e-8).
    pub epsilon: f32,
    /// Weight-decay (L2 reg).
    pub weight_decay: f32,
    /// Learning-rate schedule.
    pub schedule: LrSchedule,
}

impl Default for AdamConfig {
    fn default() -> Self {
        Self {
            lr: 1e-3,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            weight_decay: 0.0,
            schedule: LrSchedule::Constant,
        }
    }
}

/// Adam optimizer with per-parameter (m, v) buffers.
#[derive(Clone, Debug)]
pub struct AdamOptimizer {
    cfg: AdamConfig,
    /// First-moment buffers.
    m: Vec<Vec<f32>>,
    /// Second-moment buffers.
    v: Vec<Vec<f32>>,
    step: u64,
}

impl AdamOptimizer {
    /// New optimizer initialized for `set`.
    #[must_use]
    pub fn new(cfg: AdamConfig, set: &ParameterSet) -> Self {
        let m = set
            .params()
            .iter()
            .map(|p| vec![0.0_f32; p.shape.count as usize])
            .collect();
        let v = set
            .params()
            .iter()
            .map(|p| vec![0.0_f32; p.shape.count as usize])
            .collect();
        Self { cfg, m, v, step: 0 }
    }

    /// Take one Adam step.
    pub fn step(&mut self, set: &mut ParameterSet) -> Result<StepReport, OptimizerError> {
        let grad_norm = set.gradient_norm();
        self.step += 1;
        let t = self.step as i32;
        let lr = self.cfg.schedule.lr_at(self.cfg.lr, self.step);
        let bc1 = 1.0 - self.cfg.beta1.powi(t);
        let bc2 = 1.0 - self.cfg.beta2.powi(t);
        let mut updated = 0_u32;
        let mut frozen = 0_u32;

        let summary: Vec<(ParameterId, f32, bool, u32)> = set
            .params()
            .iter()
            .map(|p| (p.id, p.kind.default_lr_scale(), p.frozen, p.shape.count))
            .collect();

        for (id, lr_scale, is_frozen, count) in summary {
            if is_frozen {
                frozen += 1;
                continue;
            }
            let idx = id.raw() as usize;
            if self.m[idx].len() != count as usize {
                return Err(OptimizerError::StateDimensionMismatch(
                    id.raw(),
                    count,
                    self.m[idx].len() as u32,
                ));
            }

            let mut update = vec![0.0_f32; count as usize];
            {
                let g = set.gradient(id)?;
                let p_vals = &set.get(id)?.values;
                let m = &mut self.m[idx];
                let v = &mut self.v[idx];
                for i in 0..count as usize {
                    let mut grad = g[i];
                    if self.cfg.weight_decay != 0.0 {
                        grad += self.cfg.weight_decay * p_vals[i];
                    }
                    m[i] = self.cfg.beta1 * m[i] + (1.0 - self.cfg.beta1) * grad;
                    v[i] = self.cfg.beta2 * v[i] + (1.0 - self.cfg.beta2) * grad * grad;
                    let m_hat = m[i] / bc1;
                    let v_hat = v[i] / bc2;
                    update[i] = -lr * lr_scale * m_hat / (v_hat.sqrt() + self.cfg.epsilon);
                }
            }
            set.apply_update(id, &update)?;
            updated += 1;
        }

        Ok(StepReport {
            updated,
            frozen_skipped: frozen,
            gradient_norm: grad_norm,
            step_index: self.step,
        })
    }

    /// Current step counter.
    #[must_use]
    pub fn step_index(&self) -> u64 {
        self.step
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameter::Parameter;

    #[test]
    fn lr_schedule_constant_returns_base() {
        let s = LrSchedule::Constant;
        for k in 0..10 {
            assert_eq!(s.lr_at(0.5, k), 0.5);
        }
    }

    #[test]
    fn lr_schedule_exponential_decays() {
        let s = LrSchedule::Exponential { decay: 0.5 };
        assert!((s.lr_at(1.0, 0) - 1.0).abs() < 1e-6);
        assert!((s.lr_at(1.0, 1) - 0.5).abs() < 1e-6);
        assert!((s.lr_at(1.0, 2) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn lr_schedule_cosine_at_endpoints() {
        let s = LrSchedule::Cosine { total: 10 };
        assert!((s.lr_at(1.0, 0) - 1.0).abs() < 1e-5);
        assert!(s.lr_at(1.0, 10).abs() < 1e-5);
    }

    #[test]
    fn lr_schedule_step_decay_drops_at_boundary() {
        let s = LrSchedule::StepDecay {
            step_size: 5,
            gamma: 0.1,
        };
        assert!((s.lr_at(1.0, 0) - 1.0).abs() < 1e-6);
        assert!((s.lr_at(1.0, 4) - 1.0).abs() < 1e-6);
        assert!((s.lr_at(1.0, 5) - 0.1).abs() < 1e-6);
        assert!((s.lr_at(1.0, 10) - 0.01).abs() < 1e-6);
    }

    #[test]
    fn sgd_step_decreases_loss_directionally() {
        let mut set = ParameterSet::new();
        // Use kan_cell_weights : zero-init values + lr_scale 1.0 → easy to predict.
        let id = set.register(Parameter::kan_cell_weights(2, "x")).unwrap();
        // grad = -1 ; with lr=0.1 + lr_scale=1.0 + momentum=0 we expect
        // update = -lr * lr_scale * v = -0.1 * 1.0 * (-1) = +0.1 ; θ_new = 0+0.1 = 0.1.
        set.accumulate(id, &[-1.0, -1.0]).unwrap();
        let mut opt = SgdOptimizer::new(
            SgdConfig {
                lr: 0.1,
                momentum: 0.0,
                weight_decay: 0.0,
                schedule: LrSchedule::Constant,
            },
            &set,
        );
        let r = opt.step(&mut set).unwrap();
        assert_eq!(r.updated, 1);
        assert_eq!(r.frozen_skipped, 0);
        let v = &set.get(id).unwrap().values;
        assert!((v[0] - 0.1).abs() < 1e-6, "v[0] = {}", v[0]);
        assert!((v[1] - 0.1).abs() < 1e-6, "v[1] = {}", v[1]);
    }

    #[test]
    fn sgd_skips_frozen_parameter() {
        let mut set = ParameterSet::new();
        let id = set.register(Parameter::material_coefs(2, "x")).unwrap();
        set.get_mut(id).unwrap().freeze();
        set.accumulate(id, &[-1.0, -1.0]).unwrap();
        let mut opt = SgdOptimizer::new(SgdConfig::default(), &set);
        let r = opt.step(&mut set).unwrap();
        assert_eq!(r.updated, 0);
        assert_eq!(r.frozen_skipped, 1);
        // Values unchanged.
        let v = &set.get(id).unwrap().values;
        assert_eq!(v, &vec![0.5, 0.5]);
    }

    #[test]
    fn adam_step_moves_in_descent_direction() {
        let mut set = ParameterSet::new();
        let id = set.register(Parameter::material_coefs(1, "x")).unwrap();
        set.accumulate(id, &[1.0]).unwrap();
        let mut opt = AdamOptimizer::new(
            AdamConfig {
                lr: 0.1,
                beta1: 0.9,
                beta2: 0.999,
                epsilon: 1e-8,
                weight_decay: 0.0,
                schedule: LrSchedule::Constant,
            },
            &set,
        );
        let v0 = set.get(id).unwrap().values[0];
        opt.step(&mut set).unwrap();
        let v1 = set.get(id).unwrap().values[0];
        // Positive gradient → value should DECREASE.
        assert!(v1 < v0, "expected descent, v0={v0} v1={v1}");
    }
}
