#![allow(clippy::suboptimal_flops)] // textbook expressions used in test analytics.
#![allow(clippy::float_cmp)] // bit-exact recorded values, exact compare ok.

//! Integration tests for the GPU-AD tape + Jet + atomic + KAN surfaces.
//!
//! These tests exercise the full record-replay pipeline as a single
//! differentiable computation (mirroring how the SPIR-V emitter wires the
//! forward + reverse pass on the GPU). Each test :
//!     1. constructs a tape in the appropriate storage mode
//!     2. records a forward-pass op-sequence (mirroring what the SPIR-V
//!        body-emitter would emit for a `@differentiable {GPU}` MIR fn)
//!     3. seeds the cotangent buffer + replays
//!     4. compares the recovered gradient against the analytic answer
//!
//! § COMMITMENT-CHECK
//!   These tests are the validation contract between the CPU-side simulator
//!   in `cssl_autodiff::gpu` and the SPIR-V emission in
//!   `cssl_cgen_gpu_spirv::diff_shader`. If the algebraic invariants here
//!   hold, the SPIR-V emitter that lays down the same op-sequence is
//!   correct-by-construction (modulo SPIR-V validation, which the existing
//!   `cssl-cgen-gpu-spirv` round-trip test handles separately).

use cssl_autodiff::gpu::{
    select_storage_mode, AtomicAdjointAccumulator, AtomicMode, CoopMatrixPath, CoopMatrixVendor,
    GpuJet, GpuTape, KanGpuForward, KanVariant, OpRecordKind, OperationDensity, RecordedOperand,
    TapeStorageMode,
};
use cssl_autodiff::Jet;

#[test]
fn lds_to_workgroup_to_ssbo_storage_promotion_chain() {
    // Tiny op-count → LDS.
    let small = OperationDensity {
        op_count: 8,
        avg_operand_count: 2,
        workgroup_size: 32,
        ..Default::default()
    };
    assert_eq!(select_storage_mode(small), TapeStorageMode::ThreadLocalLds);

    // Medium → workgroup.
    let mid = OperationDensity {
        op_count: 200,
        ..small
    };
    assert_eq!(select_storage_mode(mid), TapeStorageMode::WorkgroupShared);

    // Large or atomic → SSBO.
    let big = OperationDensity {
        op_count: 4096,
        ..small
    };
    assert_eq!(select_storage_mode(big), TapeStorageMode::GlobalSsbo);
}

#[test]
fn forward_then_reverse_round_trip_on_polynomial() {
    // y = a² + 3·a + 1 ; dy/da = 2a + 3
    let mut tape = GpuTape::new(TapeStorageMode::WorkgroupShared);

    let a_val = 2.0_f64;
    let a = tape
        .record(
            OpRecordKind::Load,
            vec![RecordedOperand::input(a_val)],
            a_val,
        )
        .unwrap();

    let aa = tape
        .record(
            OpRecordKind::FMul,
            vec![
                RecordedOperand::from_slot(a, a_val),
                RecordedOperand::from_slot(a, a_val),
            ],
            a_val * a_val,
        )
        .unwrap();

    let three_a = tape
        .record(
            OpRecordKind::FMul,
            vec![
                RecordedOperand::from_slot(a, a_val),
                RecordedOperand::input(3.0),
            ],
            3.0 * a_val,
        )
        .unwrap();

    let sum1 = tape
        .record(
            OpRecordKind::FAdd,
            vec![
                RecordedOperand::from_slot(aa, a_val * a_val),
                RecordedOperand::from_slot(three_a, 3.0 * a_val),
            ],
            a_val * a_val + 3.0 * a_val,
        )
        .unwrap();

    let _y = tape
        .record(
            OpRecordKind::FAdd,
            vec![
                RecordedOperand::from_slot(sum1, a_val * a_val + 3.0 * a_val),
                RecordedOperand::input(1.0),
            ],
            a_val * a_val + 3.0 * a_val + 1.0,
        )
        .unwrap();

    let mut cot = vec![0.0; tape.len()];
    let last = tape.len() - 1;
    cot[last] = 1.0;
    tape.replay_into(&mut cot).unwrap();

    let expected = 2.0 * a_val + 3.0;
    assert!(
        (cot[a as usize] - expected).abs() < 1e-12,
        "got {} expected {}",
        cot[a as usize],
        expected
    );
}

#[test]
fn forward_reverse_pipeline_recovers_sqrt_derivative() {
    // y = sqrt(a) ; dy/da = 1 / (2 sqrt(a))
    let mut tape = GpuTape::new(TapeStorageMode::ThreadLocalLds);
    let a_val = 4.0_f64;
    let a = tape
        .record(
            OpRecordKind::Load,
            vec![RecordedOperand::input(a_val)],
            a_val,
        )
        .unwrap();
    tape.record(
        OpRecordKind::Sqrt,
        vec![RecordedOperand::from_slot(a, a_val)],
        a_val.sqrt(),
    )
    .unwrap();

    let mut cot = vec![0.0; tape.len()];
    let last = tape.len() - 1;
    cot[last] = 1.0;
    tape.replay_into(&mut cot).unwrap();

    let expected = 1.0 / (2.0 * a_val.sqrt());
    assert!((cot[a as usize] - expected).abs() < 1e-12);
}

#[test]
fn forward_reverse_pipeline_recovers_exp_derivative() {
    // y = exp(a) ; dy/da = exp(a)
    let mut tape = GpuTape::new(TapeStorageMode::ThreadLocalLds);
    let a_val = 1.5_f64;
    let a = tape
        .record(
            OpRecordKind::Load,
            vec![RecordedOperand::input(a_val)],
            a_val,
        )
        .unwrap();
    tape.record(
        OpRecordKind::Exp,
        vec![RecordedOperand::from_slot(a, a_val)],
        a_val.exp(),
    )
    .unwrap();

    let mut cot = vec![0.0; tape.len()];
    let last = tape.len() - 1;
    cot[last] = 1.0;
    tape.replay_into(&mut cot).unwrap();

    let expected = a_val.exp();
    assert!((cot[a as usize] - expected).abs() < 1e-12);
}

#[test]
fn forward_reverse_pipeline_recovers_log_derivative() {
    // y = log(a) ; dy/da = 1/a
    let mut tape = GpuTape::new(TapeStorageMode::ThreadLocalLds);
    let a_val = 2.5_f64;
    let a = tape
        .record(
            OpRecordKind::Load,
            vec![RecordedOperand::input(a_val)],
            a_val,
        )
        .unwrap();
    tape.record(
        OpRecordKind::Log,
        vec![RecordedOperand::from_slot(a, a_val)],
        a_val.ln(),
    )
    .unwrap();

    let mut cot = vec![0.0; tape.len()];
    let last = tape.len() - 1;
    cot[last] = 1.0;
    tape.replay_into(&mut cot).unwrap();

    let expected = 1.0 / a_val;
    assert!((cot[a as usize] - expected).abs() < 1e-12);
}

#[test]
fn atomic_adjoint_accumulates_concurrent_writes() {
    // Simulate 8 lanes each writing a different partial-delta into a shared
    // parameter cotangent ; final value should be the sum.
    let acc = AtomicAdjointAccumulator::f32();
    for k in 0..8 {
        acc.add(f64::from(k), AtomicMode::NativeFAddF32);
    }
    // 0 + 1 + 2 + ... + 7 = 28
    assert!((acc.read() - 28.0).abs() < 1e-5);
}

#[test]
fn atomic_adjoint_cas_loop_matches_native_for_single_thread() {
    let cas = AtomicAdjointAccumulator::f64();
    let nat = AtomicAdjointAccumulator::f64();
    for k in 0..50 {
        let d = (k as f64) * 0.1;
        cas.add(d, AtomicMode::CasLoopEmulation);
        nat.add(d, AtomicMode::NativeFAddF64);
    }
    let v_cas = cas.read();
    let v_nat = nat.read();
    assert!((v_cas - v_nat).abs() < 1e-12);
}

#[test]
fn kan_forward_pass_uses_workgroup_tape() {
    let kan = KanGpuForward::new(KanVariant::SpectralBrdf);
    let mut tape = GpuTape::new(TapeStorageMode::WorkgroupShared);
    let input: GpuJet<f32, 2> = GpuJet::pack(Jet::promote(0.3)).unwrap();
    let output = kan.eval(input, 1.25_f32, &mut tape).unwrap();
    let analytic_primal = (0.3 * 0.3 + 0.3_f32.sin()) * 1.25;
    assert!((output.primal() - analytic_primal).abs() < 1e-5);
    assert!(!tape.is_empty());
}

#[test]
fn kan_forward_pass_atomic_reduces_per_layer_grad() {
    // Forward through KAN, then accumulate a per-layer gradient via atomic.
    let kan = KanGpuForward::new(KanVariant::BrdfParams);
    let mut tape = GpuTape::new(TapeStorageMode::WorkgroupShared);
    let input: GpuJet<f32, 2> = GpuJet::pack(Jet::promote(0.5)).unwrap();
    let _output = kan.eval(input, 1.0_f32, &mut tape).unwrap();

    let mut cot = vec![0.0_f64; tape.len()];
    let last = tape.len() - 1;
    cot[last] = 1.0;
    tape.replay_into(&mut cot).unwrap();

    // Atomic-reduce all per-slot cotangents into a single accumulator.
    let acc = AtomicAdjointAccumulator::f64();
    for c in &cot {
        acc.add(*c, AtomicMode::NativeFAddF64);
    }
    // The first cotangent is the input-grad ; the sum should be > 0 since
    // the polynomial is increasing at a = 0.5.
    let total = acc.read();
    assert!(total.is_finite());
    assert!(cot[0] > 0.0); // dy/da > 0 at a = 0.5
}

#[test]
fn coop_matrix_tile_shape_matches_vendor_for_arc() {
    let path = CoopMatrixPath::for_vendor(CoopMatrixVendor::IntelArcXmx);
    let tile = path.tile.expect("tile present for Arc");
    assert_eq!((tile.m, tile.n, tile.k), (8, 16, 16));
    assert_eq!(path.element_format, "f16");
    assert!(path.uses_matrix_engine());
}

#[test]
fn coop_matrix_scalar_fallback_no_capability_required() {
    let path = CoopMatrixPath::for_vendor(CoopMatrixVendor::Scalar);
    assert!(!path.uses_matrix_engine());
    assert_eq!(path.required_capability(), None);
}

#[test]
fn jet_round_trips_through_gpu_pack_and_back() {
    let j: Jet<f32, 4> = Jet::promote(1.7);
    let g = GpuJet::pack(j).unwrap();
    assert!((g.primal() - 1.7).abs() < 1e-6);
    assert!((g.nth_deriv(1) - 1.0).abs() < 1e-6);
}

#[test]
fn jet5_rejected_by_gpu_register_pack() {
    let j: Jet<f32, 5> = Jet::promote(1.0);
    assert!(GpuJet::pack(j).is_err());
}

#[test]
fn cross_workgroup_visible_forces_ssbo() {
    let d = OperationDensity {
        op_count: 4,
        cross_workgroup_visible: true,
        ..Default::default()
    };
    assert_eq!(select_storage_mode(d), TapeStorageMode::GlobalSsbo);
}

#[test]
fn full_pipeline_kan_jet_atomic_coop_matrix_smoke() {
    // Smoke-test the full GPU-AD pipeline :
    //   - configure a workgroup-shared tape
    //   - run a KAN forward pass
    //   - replay the tape
    //   - reduce gradients via atomic
    //   - confirm a coop-matrix path is selected for Arc

    let kan = KanGpuForward::new(KanVariant::BrdfParams);
    let mut tape = GpuTape::new(TapeStorageMode::WorkgroupShared);
    let input: GpuJet<f32, 2> = GpuJet::pack(Jet::promote(0.4)).unwrap();
    let output = kan.eval(input, 0.9_f32, &mut tape).unwrap();
    assert!(output.primal().is_finite());

    let mut cot = vec![0.0_f64; tape.len()];
    let last = tape.len() - 1;
    cot[last] = 1.0;
    tape.replay_into(&mut cot).unwrap();
    assert!(cot[0].is_finite());
    assert!(cot[0] > 0.0); // first slot is input ; positive grad on this kernel @ a=0.4

    let acc = AtomicAdjointAccumulator::f32();
    acc.add(cot[0], AtomicMode::NativeFAddF32);
    let v = acc.read();
    assert!(v.is_finite());

    let path = CoopMatrixPath::for_vendor(CoopMatrixVendor::IntelArcXmx);
    assert!(path.uses_matrix_engine());
    assert_eq!(path.tile.unwrap().m, 8);
}
