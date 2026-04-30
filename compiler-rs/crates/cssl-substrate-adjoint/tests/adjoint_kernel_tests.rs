//! § Integration tests for the adjoint-method kernel.
//!
//! Coverage (≥ 8 tests · per W-S-CORE-4 mandate) :
//!   1. forward-correctness          — alpha-contraction converges to 0
//!   2. finite-diff-vs-adjoint       — analytic adjoint matches FD oracle
//!   3. checkpoint-gradient-equivalence — different strides give same gradient
//!   4. parameter-update             — Adam descent reduces loss over N steps
//!   5. convergence                  — SGD on quadratic loss reaches near-zero
//!   6. multi-parameter-routing      — gradients route to correct param-id
//!   7. frozen-parameter-respected   — gradient computed but not applied
//!   8. checkpoint-recompute-correct — recomputed state == direct forward
//!   9. checkpoint-memory-bound      — store size respects √MAX_ITER policy
//!  10. zero-loss-zero-gradient      — converged target → zero gradient

use cssl_substrate_adjoint::{
    AdamConfig, AdamOptimizer, AdjointConfig, AdjointState, CheckpointPolicy, LossFn, LrSchedule,
    Parameter, ParameterId, ParameterSet, SgdConfig, SgdOptimizer,
};

// Toy substrate : single scalar state x ; per-iteration step x ← α·x.
//
// Forward over K iterations with x_0 = 1 produces x_K = α^K.
// Loss : ½(x_K - target)².
// Analytic gradient : d Loss / d α = (x_K - target) · K · α^{K-1}.
//
// This is small enough to validate analytically + against finite-diff.

fn step_alpha(state: &[f32], params: &ParameterSet, _k: u32) -> Vec<f32> {
    let a = params.params()[0].values[0];
    state.iter().map(|x| a * x).collect()
}

/// VJP w.r.t. state for x ← α·x : Jᵀ · v = α · v (scalar Jacobian = α).
fn vjp_state_alpha(_state: &[f32], params: &ParameterSet, _k: u32, adjoint: &[f32]) -> Vec<f32> {
    let a = params.params()[0].values[0];
    adjoint.iter().map(|v| a * v).collect()
}

/// VJP w.r.t. α for x ← α·x : Jᵀ_α · v = sum_i x_i · v_i.
fn vjp_params_alpha(
    state: &[f32],
    _params: &ParameterSet,
    _k: u32,
    adjoint: &[f32],
    _id: ParameterId,
) -> Vec<f32> {
    let mut g = 0.0_f32;
    for (s, a) in state.iter().zip(adjoint.iter()) {
        g += s * a;
    }
    vec![g]
}

fn make_alpha_set(alpha: f32) -> (ParameterSet, ParameterId) {
    let mut s = ParameterSet::new();
    let mut p = Parameter::material_coefs(1, "alpha");
    p.values[0] = alpha;
    let id = s.register(p).unwrap();
    (s, id)
}

fn forward_only(alpha: f32, x0: f32, k_iters: u32) -> f32 {
    let mut x = x0;
    for _ in 0..k_iters {
        x *= alpha;
    }
    x
}

#[test]
fn t1_forward_correctness_alpha_contraction() {
    let (params, _) = make_alpha_set(0.5);
    let mut adj = AdjointState::new(AdjointConfig {
        max_iter: 32,
        forward_tol: 1e-12,
        checkpoint: CheckpointPolicy::standard(),
    });
    adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
    let traj = adj.trajectory().unwrap();
    // After 32 iterations of 0.5^k → vanishingly small.
    assert!(traj.final_state[0].abs() < 1e-6);
}

#[test]
fn t2_finite_diff_vs_adjoint_matches() {
    // Analytic forward : x_K = α^K, K=8, x_0=1.
    let alpha = 0.7_f32;
    let target = 0.5_f32;
    let k_iters = 8_u32;

    // Run adjoint.
    let (mut params, id) = make_alpha_set(alpha);
    let mut adj = AdjointState::new(AdjointConfig {
        max_iter: k_iters,
        forward_tol: 0.0, // force full K iterations
        checkpoint: CheckpointPolicy::standard(),
    });
    adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
    let report = adj
        .backward_pass(
            &LossFn::mse(),
            &[target],
            &mut params,
            step_alpha,
            vjp_state_alpha,
            vjp_params_alpha,
        )
        .unwrap();

    let analytic_grad = params.gradient(id).unwrap()[0];

    // Finite-difference oracle.
    let h = 1e-3_f32;
    let mse = |a: f32| {
        let xk = forward_only(a, 1.0, k_iters);
        let d = xk - target;
        0.5 * d * d
    };
    let fd_grad = (mse(alpha + h) - mse(alpha - h)) / (2.0 * h);

    let abs_err = (analytic_grad - fd_grad).abs();
    let rel_err = abs_err / (fd_grad.abs().max(1e-6));

    assert!(
        rel_err < 5e-2,
        "rel-err {rel_err}: analytic={analytic_grad} fd={fd_grad}; loss={}",
        report.loss_value
    );
}

#[test]
fn t3_checkpoint_gradient_equivalence_across_strides() {
    let alpha = 0.6_f32;
    let target = 0.4_f32;
    let k_iters = 12_u32;

    let mut grads = Vec::new();
    for stride in [1_u32, 4, 6, 12] {
        let (mut params, id) = make_alpha_set(alpha);
        let mut adj = AdjointState::new(AdjointConfig {
            max_iter: k_iters,
            forward_tol: 0.0,
            checkpoint: CheckpointPolicy {
                stride,
                capacity: 32,
            },
        });
        adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
        adj.backward_pass(
            &LossFn::mse(),
            &[target],
            &mut params,
            step_alpha,
            vjp_state_alpha,
            vjp_params_alpha,
        )
        .unwrap();
        grads.push(params.gradient(id).unwrap()[0]);
    }
    // All strides should give identical gradient (up to floating-point order).
    let g0 = grads[0];
    for g in &grads {
        assert!((*g - g0).abs() < 1e-4, "stride mismatch: {grads:?}");
    }
}

#[test]
fn t4_parameter_update_adam_reduces_loss() {
    let alpha_init = 0.95_f32;
    let target = 0.0_f32; // drive x_K → 0 ⇒ α should shrink.
    let k_iters = 8_u32;

    let (mut params, id) = make_alpha_set(alpha_init);
    let mut opt = AdamOptimizer::new(
        AdamConfig {
            lr: 0.1,
            ..AdamConfig::default()
        },
        &params,
    );

    let initial_xk = forward_only(alpha_init, 1.0, k_iters);
    let initial_loss = 0.5 * initial_xk * initial_xk;

    for _ in 0..30 {
        params.clear_gradients();
        let mut adj = AdjointState::new(AdjointConfig {
            max_iter: k_iters,
            forward_tol: 0.0,
            checkpoint: CheckpointPolicy::standard(),
        });
        adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
        adj.backward_pass(
            &LossFn::mse(),
            &[target],
            &mut params,
            step_alpha,
            vjp_state_alpha,
            vjp_params_alpha,
        )
        .unwrap();
        opt.step(&mut params).unwrap();
    }

    let alpha_final = params.get(id).unwrap().values[0];
    let final_xk = forward_only(alpha_final, 1.0, k_iters);
    let final_loss = 0.5 * final_xk * final_xk;

    assert!(
        final_loss < initial_loss * 0.5,
        "Adam failed to reduce loss: init={initial_loss} final={final_loss} α: {alpha_init} → {alpha_final}"
    );
}

#[test]
fn t5_sgd_convergence_on_quadratic_loss() {
    // We want x_K = α^K → 0.5 with K=4. Optimal α = 0.5^{1/4} ≈ 0.8409.
    let target = 0.5_f32;
    let k_iters = 4_u32;
    let alpha_init = 0.99_f32;

    let (mut params, id) = make_alpha_set(alpha_init);
    let mut opt = SgdOptimizer::new(
        SgdConfig {
            lr: 0.5, // higher rate ; lr_scale 0.5 for MaterialCoefs makes effective lr=0.25
            momentum: 0.0,
            weight_decay: 0.0,
            schedule: LrSchedule::Constant,
        },
        &params,
    );

    let mut last_loss = f32::INFINITY;
    for _step in 0..50 {
        params.clear_gradients();
        let mut adj = AdjointState::new(AdjointConfig {
            max_iter: k_iters,
            forward_tol: 0.0,
            checkpoint: CheckpointPolicy::standard(),
        });
        adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
        let r = adj
            .backward_pass(
                &LossFn::mse(),
                &[target],
                &mut params,
                step_alpha,
                vjp_state_alpha,
                vjp_params_alpha,
            )
            .unwrap();
        last_loss = r.loss_value;
        opt.step(&mut params).unwrap();
    }
    let alpha_final = params.get(id).unwrap().values[0];
    let optimal = 0.5_f32.powf(0.25);
    assert!(last_loss < 1e-3, "SGD did not converge: loss={last_loss}");
    assert!(
        (alpha_final - optimal).abs() < 0.05,
        "α final {alpha_final} not close to optimal {optimal}"
    );
}

#[test]
fn t6_multi_parameter_routing() {
    // Two parameters : α (used by step) + β (unused).
    // β's gradient must be 0 ; α's gradient must match single-param run.
    let mut params = ParameterSet::new();
    let mut a = Parameter::material_coefs(1, "alpha");
    a.values[0] = 0.5;
    let id_a = params.register(a).unwrap();
    let mut b = Parameter::material_coefs(1, "beta");
    b.values[0] = 1.234;
    let id_b = params.register(b).unwrap();

    let target = 0.0_f32;
    let k_iters = 6_u32;

    let mut adj = AdjointState::new(AdjointConfig {
        max_iter: k_iters,
        forward_tol: 0.0,
        checkpoint: CheckpointPolicy::standard(),
    });
    adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
    adj.backward_pass(
        &LossFn::mse(),
        &[target],
        &mut params,
        step_alpha,
        vjp_state_alpha,
        |state, p, _k, adjoint, id| {
            if id == id_a {
                let mut g = 0.0_f32;
                for (s, a) in state.iter().zip(adjoint.iter()) {
                    g += s * a;
                }
                vec![g]
            } else {
                vec![0.0; p.get(id).unwrap().values.len()]
            }
        },
    )
    .unwrap();

    let g_a = params.gradient(id_a).unwrap()[0];
    let g_b = params.gradient(id_b).unwrap()[0];
    assert!(g_a.abs() > 1e-6, "α should have nonzero gradient ; got {g_a}");
    assert!(g_b.abs() < 1e-9, "β should have zero gradient ; got {g_b}");
}

#[test]
fn t7_frozen_parameter_respected_by_optimizer() {
    let (mut params, id) = make_alpha_set(0.5);
    params.get_mut(id).unwrap().freeze();
    let mut opt = SgdOptimizer::new(SgdConfig::default(), &params);

    let mut adj = AdjointState::new(AdjointConfig {
        max_iter: 4,
        forward_tol: 0.0,
        checkpoint: CheckpointPolicy::standard(),
    });
    adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
    adj.backward_pass(
        &LossFn::mse(),
        &[0.0],
        &mut params,
        step_alpha,
        vjp_state_alpha,
        vjp_params_alpha,
    )
    .unwrap();

    // Gradient should be present (computed) but step should NOT apply it.
    let g = params.gradient(id).unwrap()[0];
    assert!(g.abs() > 1e-6, "gradient should still be computed for frozen param");

    let report = opt.step(&mut params).unwrap();
    assert_eq!(report.frozen_skipped, 1);
    assert_eq!(report.updated, 0);
    assert_eq!(params.get(id).unwrap().values[0], 0.5, "frozen value unchanged");
}

#[test]
fn t8_checkpoint_recompute_correct() {
    // Run forward with stride=4 ; recompute states at iter=2,5,7 and compare to
    // direct forward.
    let (params, _) = make_alpha_set(0.7);
    let mut adj = AdjointState::new(AdjointConfig {
        max_iter: 10,
        forward_tol: 0.0,
        checkpoint: CheckpointPolicy {
            stride: 4,
            capacity: 16,
        },
    });
    adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
    let traj = adj.trajectory().unwrap();
    let store = &traj.checkpoints;

    for target_iter in [2_u32, 5, 7, 9] {
        let recomputed = store
            .recompute_to(target_iter, |s, k| step_alpha(s, &params, k))
            .unwrap();
        let direct = forward_only(0.7, 1.0, target_iter);
        assert!(
            (recomputed[0] - direct).abs() < 1e-4,
            "iter {target_iter}: recomputed={} direct={direct}",
            recomputed[0]
        );
    }
}

#[test]
fn t9_checkpoint_memory_bound_respects_capacity() {
    let (params, _) = make_alpha_set(0.5);
    let mut adj = AdjointState::new(AdjointConfig {
        max_iter: 64,
        forward_tol: 0.0,
        checkpoint: CheckpointPolicy {
            stride: 8,
            capacity: 4,
        },
    });
    let report = adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
    // capacity is 4 ; never exceed that even though 64/8 = 8 candidate save points.
    assert!(report.checkpoints_stored <= 4);
}

#[test]
fn t10_zero_loss_zero_gradient_at_optimum() {
    // x_0 = 1, α = 0.5, K = 2 → x_K = 0.25. Set target = 0.25 ⇒ loss=0.
    let (mut params, id) = make_alpha_set(0.5);
    let mut adj = AdjointState::new(AdjointConfig {
        max_iter: 2,
        forward_tol: 0.0,
        checkpoint: CheckpointPolicy::standard(),
    });
    adj.forward_pass(&[1.0], &params, step_alpha).unwrap();
    let report = adj
        .backward_pass(
            &LossFn::mse(),
            &[0.25],
            &mut params,
            step_alpha,
            vjp_state_alpha,
            vjp_params_alpha,
        )
        .unwrap();
    assert!(report.loss_value < 1e-6);
    assert!(params.gradient(id).unwrap()[0].abs() < 1e-4);
}
