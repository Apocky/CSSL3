//! § adjoint — forward + backward driver for CFER iteration.
//!
//! Architecture (per specs/36 § DIFFERENTIABILITY) :
//!
//! Forward pass :
//! ```text
//!   L^{(0)} = L_init
//!   for k in 0..MAX_ITER {
//!       L^{(k+1)} = step(L^{(k)}, params, k)            # CFER iteration
//!       if ‖L^{(k+1)} - L^{(k)}‖ < tol { break; }       # convergence
//!       checkpoint at every stride steps
//!   }
//! ```
//!
//! Backward pass (adjoint) :
//! ```text
//!   ∂L/∂L^{(K)} = ∂loss/∂L_final                        # seed from loss-fn
//!   for k in (0..K).rev() {
//!       L^{(k)} = recompute_from_nearest_checkpoint(k)   # √MAX_ITER memory
//!       J_state, J_params = jacobian(step at L^{(k)}, params)
//!       ∂L/∂L^{(k)}     = J_stateᵀ · ∂L/∂L^{(k+1)}
//!       ∂L/∂params     += J_paramsᵀ · ∂L/∂L^{(k+1)}
//!   }
//! ```
//!
//! The kernel is **substrate-agnostic** : callers supply
//! `step_fn` (forward iteration) + `vjp_state` (state-vector-Jacobian-product)
//! + `vjp_params` (param-vector-Jacobian-product) callbacks. This keeps the
//! adjoint testable in isolation against a finite-difference oracle — see
//! the `tests/` integration suite.
//!
//! For CFER specifically the callbacks wrap :
//!   - `step_fn`     : one CFER iteration over an `OmegaField10D` slice
//!   - `vjp_state`   : KAN per-cell update Jacobian × adjoint-state
//!   - `vjp_params`  : KAN per-cell update Jacobian × parameter
//!
//! Wave-S-CORE-3 (cssl-substrate-loa-kan) supplies the analytic Jacobians ;
//! this kernel only orchestrates the iteration, checkpointing, and
//! gradient accumulation.

use crate::checkpoint::{CheckpointError, CheckpointPolicy, CheckpointStore};
use crate::loss::{LossError, LossFn, LossReport};
use crate::parameter::{ParameterError, ParameterId, ParameterSet};
use thiserror::Error;

/// Configuration for one adjoint job.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdjointConfig {
    /// Maximum forward iterations. Per specs/36 : 16-64 typical.
    pub max_iter: u32,
    /// Convergence tolerance for forward pass : ‖L^{(k+1)} - L^{(k)}‖ < tol.
    pub forward_tol: f32,
    /// Checkpoint policy for memory-bounded backward.
    pub checkpoint: CheckpointPolicy,
}

impl Default for AdjointConfig {
    fn default() -> Self {
        Self {
            max_iter: crate::DEFAULT_MAX_ITER,
            forward_tol: crate::DEFAULT_FORWARD_TOL,
            checkpoint: CheckpointPolicy::standard(),
        }
    }
}

/// Errors surfaced by the adjoint driver.
#[derive(Debug, Error)]
pub enum AdjointError {
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
    #[error(transparent)]
    Loss(#[from] LossError),
    #[error(transparent)]
    Parameter(#[from] ParameterError),
    #[error("forward pass diverged at iteration {iter}: residual {residual}")]
    Diverged { iter: u32, residual: f32 },
    #[error("VJP callback returned wrong-length vector: param {param_id} expected {expected}, got {actual}")]
    VjpDimensionMismatch {
        param_id: u32,
        expected: u32,
        actual: u32,
    },
}

/// Per-iteration state recorded during forward pass.
#[derive(Clone, Debug)]
pub struct ForwardTrajectory {
    /// Final state vector (last iteration before convergence-stop).
    pub final_state: Vec<f32>,
    /// Total iterations actually run.
    pub iters_run: u32,
    /// Residual at convergence (or at max-iter cap).
    pub final_residual: f32,
    /// Whether the forward pass converged within tol.
    pub converged: bool,
    /// Checkpoint store (used to recompute intermediate states during backward).
    pub checkpoints: CheckpointStore,
}

/// Report from a forward pass.
#[derive(Clone, Debug)]
pub struct ForwardReport {
    /// Iterations run.
    pub iters_run: u32,
    /// Final residual (‖L^{(K)} - L^{(K-1)}‖).
    pub final_residual: f32,
    /// Converged flag.
    pub converged: bool,
    /// Number of checkpoints stored.
    pub checkpoints_stored: u32,
    /// Memory used by checkpoint store (bytes).
    pub checkpoint_memory_bytes: u32,
}

/// Report from a backward pass.
#[derive(Clone, Debug)]
pub struct BackwardReport {
    /// Loss value at the end of forward pass.
    pub loss_value: f32,
    /// Iterations run during backward pass (matches `forward.iters_run`).
    pub backward_iters: u32,
    /// Final L2-norm of the parameter-set gradient vector.
    pub gradient_norm: f32,
    /// Number of parameters with nonzero gradient.
    pub active_parameters: u32,
}

/// Adjoint driver — owns the iteration loop and checkpoint store but
/// delegates the actual step + VJPs to callbacks.
///
/// The state vector type is `Vec<f32>` (caller chooses semantics). Adjoint
/// state has the same shape as the forward state.
pub struct AdjointState {
    cfg: AdjointConfig,
    /// Forward trajectory recorded by the most recent `forward_pass` call.
    /// Reset on every forward.
    trajectory: Option<ForwardTrajectory>,
}

impl core::fmt::Debug for AdjointState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AdjointState")
            .field("cfg", &self.cfg)
            .field("has_trajectory", &self.trajectory.is_some())
            .finish()
    }
}

impl AdjointState {
    /// New driver with the given config.
    #[must_use]
    pub fn new(cfg: AdjointConfig) -> Self {
        Self { cfg, trajectory: None }
    }

    /// Whether a forward trajectory has been recorded (and is available
    /// for backward).
    #[must_use]
    pub fn has_trajectory(&self) -> bool {
        self.trajectory.is_some()
    }

    /// Borrow the recorded trajectory (None if no forward has run yet).
    #[must_use]
    pub fn trajectory(&self) -> Option<&ForwardTrajectory> {
        self.trajectory.as_ref()
    }

    /// Run forward pass.
    ///
    /// `step_fn(state, params, k) → next_state` advances one CFER iteration ;
    /// must produce a vector of the same length as `state`.
    ///
    /// Convergence : forward stops when ‖next - state‖₂ < `cfg.forward_tol`.
    /// Divergence  : if the residual exceeds 1e6 we abort with `Diverged`.
    pub fn forward_pass<F>(
        &mut self,
        initial_state: &[f32],
        params: &ParameterSet,
        mut step_fn: F,
    ) -> Result<ForwardReport, AdjointError>
    where
        F: FnMut(&[f32], &ParameterSet, u32) -> Vec<f32>,
    {
        let policy = self.cfg.checkpoint;
        let mut store = CheckpointStore::new(policy)?;
        let mut state = initial_state.to_vec();

        // Save iter=0 if policy says so.
        if store.should_checkpoint(0) {
            store.push(0, state.clone())?;
        }

        let mut residual = f32::INFINITY;
        let mut converged = false;
        let mut iters_run = 0;
        for k in 0..self.cfg.max_iter {
            let next = step_fn(&state, params, k);
            // Compute residual (L2 of next - state).
            residual = l2_diff(&state, &next);
            if !residual.is_finite() || residual > 1e6 {
                return Err(AdjointError::Diverged {
                    iter: k,
                    residual,
                });
            }
            state = next;
            iters_run = k + 1;
            // Save checkpoint at iter=k+1 if policy says so.
            if store.should_checkpoint(iters_run) {
                store.push(iters_run, state.clone())?;
            }
            if residual < self.cfg.forward_tol {
                converged = true;
                break;
            }
        }

        let cps = store.len() as u32;
        let mem = store.memory_bytes() as u32;
        self.trajectory = Some(ForwardTrajectory {
            final_state: state,
            iters_run,
            final_residual: residual,
            converged,
            checkpoints: store,
        });

        Ok(ForwardReport {
            iters_run,
            final_residual: residual,
            converged,
            checkpoints_stored: cps,
            checkpoint_memory_bytes: mem,
        })
    }

    /// Run backward pass.
    ///
    /// Inputs :
    ///   - `loss_fn`   : the loss to differentiate.
    ///   - `target`    : reference state (same length as `final_state`).
    ///   - `params`    : the parameter set ; gradients are accumulated into
    ///                   `params.gradient_mut(id)` for each id.
    ///   - `step_fn`   : the SAME forward step used in `forward_pass`.
    ///                   We need it to recompute intermediate states from
    ///                   checkpoints during the reverse iteration.
    ///   - `vjp_state` : `(state, params, k, adjoint) → adjoint_prev` —
    ///                   computes the vector-Jacobian-product against the
    ///                   STATE Jacobian. Returned vector matches `state.len()`.
    ///   - `vjp_params`: `(state, params, k, adjoint, param_id) → grad` —
    ///                   computes the VJP against the PARAMETER Jacobian for
    ///                   the given parameter. Returned vector length matches
    ///                   `params.get(param_id).values.len()`.
    ///
    /// Returns `BackwardReport`.
    pub fn backward_pass<F, V, P>(
        &mut self,
        loss_fn: &LossFn,
        target: &[f32],
        params: &mut ParameterSet,
        mut step_fn: F,
        mut vjp_state: V,
        mut vjp_params: P,
    ) -> Result<BackwardReport, AdjointError>
    where
        F: FnMut(&[f32], &ParameterSet, u32) -> Vec<f32>,
        V: FnMut(&[f32], &ParameterSet, u32, &[f32]) -> Vec<f32>,
        P: FnMut(&[f32], &ParameterSet, u32, &[f32], ParameterId) -> Vec<f32>,
    {
        let traj = self
            .trajectory
            .as_ref()
            .ok_or(AdjointError::Checkpoint(CheckpointError::Empty))?;

        // Seed adjoint from loss : ∂L/∂L_final.
        let LossReport {
            value: loss_value,
            gradient: mut adjoint,
        } = loss_fn.evaluate(&traj.final_state, target)?;

        let iters_run = traj.iters_run;
        let snapshot_step = step_fn_snapshot(&mut step_fn);

        // Reverse iteration : k = iters_run-1, iters_run-2, ..., 0.
        // At iteration k we have the adjoint AT STEP k+1 ; we want adjoint at step k.
        for k_rev in (0..iters_run).rev() {
            // Recompute state at iteration k_rev (input to step k_rev → step k_rev + 1).
            let state_at_k =
                traj.checkpoints
                    .recompute_to(k_rev, |s, ki| (snapshot_step.borrow_mut())(s, params, ki))?;

            // 1. Accumulate parameter-gradients : grad_θ += Jᵀ_θ(step) · adjoint.
            // Snapshot ids first to avoid borrow conflicts.
            let ids: Vec<ParameterId> = params.params().iter().map(|p| p.id).collect();
            for id in ids {
                let expected = params.get(id)?.values.len() as u32;
                let g = vjp_params(&state_at_k, params, k_rev, &adjoint, id);
                if g.len() as u32 != expected {
                    return Err(AdjointError::VjpDimensionMismatch {
                        param_id: id.raw(),
                        expected,
                        actual: g.len() as u32,
                    });
                }
                params.accumulate(id, &g)?;
            }

            // 2. Backprop adjoint : adjoint_new = Jᵀ_state(step) · adjoint.
            let new_adjoint = vjp_state(&state_at_k, params, k_rev, &adjoint);
            // The adjoint must remain the same length as state.
            if new_adjoint.len() != adjoint.len() {
                return Err(AdjointError::VjpDimensionMismatch {
                    param_id: u32::MAX,
                    expected: adjoint.len() as u32,
                    actual: new_adjoint.len() as u32,
                });
            }
            adjoint = new_adjoint;
        }

        let gradient_norm = params.gradient_norm();
        let active_parameters = params
            .params()
            .iter()
            .filter_map(|p| {
                params.gradient(p.id).ok().and_then(|g| {
                    if g.iter().any(|x| x.abs() > 0.0) {
                        Some(())
                    } else {
                        None
                    }
                })
            })
            .count() as u32;

        Ok(BackwardReport {
            loss_value,
            backward_iters: iters_run,
            gradient_norm,
            active_parameters,
        })
    }

    /// Reset recorded trajectory (frees the checkpoint store).
    pub fn reset(&mut self) {
        self.trajectory = None;
    }

    /// Configuration accessor.
    #[must_use]
    pub fn config(&self) -> AdjointConfig {
        self.cfg
    }
}

/// Helper : return `(a-b) L2 norm`.
fn l2_diff(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut s = 0.0_f64;
    for i in 0..n {
        let d = (a[i] - b[i]) as f64;
        s += d * d;
    }
    s.sqrt() as f32
}

/// Adapter that lets the `recompute_to` closure call a `FnMut`. We need this
/// because `recompute_to` takes a `FnMut` whose captures are &mut, and we
/// must not move the outer `step_fn` into the inner closure. Wrapping it in
/// a `RefCell`-backed cell-like adapter keeps everything sound.
struct StepFnSnapshot<F> {
    inner: core::cell::RefCell<F>,
}

impl<F> StepFnSnapshot<F> {
    fn borrow_mut(&self) -> impl FnMut(&[f32], &ParameterSet, u32) -> Vec<f32> + '_
    where
        F: FnMut(&[f32], &ParameterSet, u32) -> Vec<f32>,
    {
        move |s, p, k| (self.inner.borrow_mut())(s, p, k)
    }
}

fn step_fn_snapshot<F>(step_fn: &mut F) -> StepFnSnapshot<&mut F>
where
    F: FnMut(&[f32], &ParameterSet, u32) -> Vec<f32>,
{
    StepFnSnapshot {
        inner: core::cell::RefCell::new(step_fn),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameter::Parameter;

    /// Toy step : L^{(k+1)} = α * L^{(k)}, so gradient w.r.t. α is well defined.
    /// State is single-component ; param vector is [α].
    fn alpha_step(state: &[f32], params: &ParameterSet, _k: u32) -> Vec<f32> {
        let alpha = params.params()[0].values[0];
        state.iter().map(|x| alpha * x).collect()
    }

    fn make_set(alpha: f32) -> (ParameterSet, ParameterId) {
        let mut s = ParameterSet::new();
        let mut p = Parameter::material_coefs(1, "alpha");
        p.values[0] = alpha;
        let id = s.register(p).unwrap();
        (s, id)
    }

    #[test]
    fn forward_pass_converges_on_contraction_map() {
        let (params, _) = make_set(0.5);
        let mut adj = AdjointState::new(AdjointConfig {
            max_iter: 64,
            forward_tol: 1e-4,
            checkpoint: CheckpointPolicy::standard(),
        });
        let report = adj.forward_pass(&[1.0], &params, alpha_step).unwrap();
        assert!(report.converged, "should converge for α<1");
        // 0.5^k → 0 ; should converge to ~0 quickly.
        let traj = adj.trajectory().unwrap();
        assert!(traj.final_state[0].abs() < 1e-3);
    }

    #[test]
    fn forward_pass_records_checkpoints() {
        let (params, _) = make_set(0.5);
        let mut adj = AdjointState::new(AdjointConfig {
            max_iter: 32,
            forward_tol: 1e-12,
            checkpoint: CheckpointPolicy {
                stride: 4,
                capacity: 16,
            },
        });
        let report = adj.forward_pass(&[1.0], &params, alpha_step).unwrap();
        assert!(report.checkpoints_stored >= 2);
    }

    #[test]
    fn forward_pass_diverges_on_growth() {
        let (params, _) = make_set(10.0);
        let mut adj = AdjointState::new(AdjointConfig {
            max_iter: 8,
            forward_tol: 1e-4,
            checkpoint: CheckpointPolicy::standard(),
        });
        let r = adj.forward_pass(&[1.0], &params, alpha_step);
        assert!(matches!(r, Err(AdjointError::Diverged { .. })));
    }

    #[test]
    fn backward_pass_no_trajectory_errors() {
        let mut adj = AdjointState::new(AdjointConfig::default());
        let (mut params, _) = make_set(0.5);
        let r = adj.backward_pass(
            &LossFn::mse(),
            &[0.0],
            &mut params,
            alpha_step,
            |_, _, _, a| a.to_vec(),
            |_, p, _, _, id| vec![0.0; p.get(id).unwrap().values.len()],
        );
        assert!(matches!(r, Err(AdjointError::Checkpoint(CheckpointError::Empty))));
    }
}
