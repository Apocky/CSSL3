//! Integration tests for the differentiable-shader SPIR-V emission path.
//!
//! These tests exercise [`emit_forward_diff_shader`] +
//! [`emit_reverse_diff_shader`] against a [`SpirvModule`] and check the
//! resulting capability + extension set + entry-point + section-ordering
//! against the spec invariants.

use cssl_autodiff::gpu::{
    AtomicMode, CoopMatrixPath, CoopMatrixVendor, OpRecordKind, TapeStorageMode,
};
use cssl_cgen_gpu_spirv::{
    declare_diff_shader_caps, emit_forward_diff_shader, emit_reverse_diff_shader,
    recognize_gpu_ad_op_name, reverse_partial_rule, supports_diff_shader, DiffShaderConfig,
    PartialFactor, PartialRule, SpirvCapability, SpirvExtension, SpirvModule, SpirvTargetEnv,
};

#[test]
fn forward_emit_lands_atomic_fadd_ext_for_native_mode() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let cfg = DiffShaderConfig::default_forward();
    let stream = [OpRecordKind::FAdd, OpRecordKind::FMul];
    let report = emit_forward_diff_shader(&mut m, &cfg, "fwd_main", &stream).unwrap();
    assert_eq!(report.records_emitted, 2);
    assert!(m
        .capabilities
        .contains(SpirvCapability::AtomicFloat32AddEXT));
    assert!(m
        .extensions
        .contains(SpirvExtension::ExtShaderAtomicFloatAdd));
}

#[test]
fn forward_emit_no_atomic_caps_for_cas_mode() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let mut cfg = DiffShaderConfig::default_forward();
    cfg.atomic_mode = AtomicMode::CasLoopEmulation;
    let stream = [OpRecordKind::FAdd];
    emit_forward_diff_shader(&mut m, &cfg, "fwd_cas", &stream).unwrap();
    assert!(!m
        .capabilities
        .contains(SpirvCapability::AtomicFloat32AddEXT));
}

#[test]
fn coop_matrix_path_lands_capability() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let mut cfg = DiffShaderConfig::default_forward();
    cfg.coop_matrix = Some(CoopMatrixPath::for_vendor(CoopMatrixVendor::IntelArcXmx));
    let stream = [OpRecordKind::FMul];
    emit_forward_diff_shader(&mut m, &cfg, "coop", &stream).unwrap();
    assert!(m
        .capabilities
        .contains(SpirvCapability::CooperativeMatrixKHR));
    assert!(m.extensions.contains(SpirvExtension::KhrCooperativeMatrix));
}

#[test]
fn forward_emit_workgroup_storage_class_string() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let cfg = DiffShaderConfig::default_forward();
    let report = emit_forward_diff_shader(&mut m, &cfg, "main", &[]).unwrap();
    assert_eq!(report.tape_storage_class, "Workgroup");
}

#[test]
fn forward_emit_thread_local_storage_class_string() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let mut cfg = DiffShaderConfig::default_forward();
    cfg.tape_storage = TapeStorageMode::ThreadLocalLds;
    let report = emit_forward_diff_shader(&mut m, &cfg, "main", &[]).unwrap();
    assert_eq!(report.tape_storage_class, "Function");
}

#[test]
fn forward_emit_global_ssbo_storage_class_string() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let mut cfg = DiffShaderConfig::default_forward();
    cfg.tape_storage = TapeStorageMode::GlobalSsbo;
    let report = emit_forward_diff_shader(&mut m, &cfg, "main", &[]).unwrap();
    assert_eq!(report.tape_storage_class, "StorageBuffer");
}

#[test]
fn reverse_emit_replay_steps_match_op_count() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let cfg = DiffShaderConfig::default_forward();
    let stream = [OpRecordKind::FAdd, OpRecordKind::FMul, OpRecordKind::Sin];
    let report = emit_reverse_diff_shader(&mut m, &cfg, "rev", &stream).unwrap();
    assert_eq!(report.replay_steps, stream.len());
}

#[test]
fn diff_shader_unsupported_on_webgpu() {
    let mut m = SpirvModule::new(SpirvTargetEnv::WebGpu);
    let cfg = DiffShaderConfig::default_forward();
    assert!(declare_diff_shader_caps(&mut m, &cfg).is_err());
}

#[test]
fn diff_shader_supported_on_vulkan_1_3_and_1_4() {
    assert!(supports_diff_shader(SpirvTargetEnv::VulkanKhr1_3));
    assert!(supports_diff_shader(SpirvTargetEnv::VulkanKhr1_4));
}

#[test]
fn forward_then_reverse_share_caps() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let cfg = DiffShaderConfig::default_forward();
    let stream = [OpRecordKind::FAdd];
    emit_forward_diff_shader(&mut m, &cfg, "fwd", &stream).unwrap();
    emit_reverse_diff_shader(&mut m, &cfg, "rev", &stream).unwrap();

    // Both entry-points landed.
    assert_eq!(m.entry_points.len(), 2);
    assert_eq!(m.entry_points[0].name, "fwd");
    assert_eq!(m.entry_points[1].name, "rev");

    // Caps consolidated (set semantics — duplicates don't grow).
    assert!(m
        .capabilities
        .contains(SpirvCapability::AtomicFloat32AddEXT));
    assert!(m.capabilities.contains(SpirvCapability::Shader));
}

#[test]
fn reverse_partial_rule_table_covers_all_kinds() {
    let kinds = [
        OpRecordKind::FAdd,
        OpRecordKind::FSub,
        OpRecordKind::FMul,
        OpRecordKind::FDiv,
        OpRecordKind::FNeg,
        OpRecordKind::Sqrt,
        OpRecordKind::Sin,
        OpRecordKind::Cos,
        OpRecordKind::Exp,
        OpRecordKind::Log,
        OpRecordKind::Load,
        OpRecordKind::Store,
    ];
    for k in kinds {
        let rules = reverse_partial_rule(k);
        assert!(!rules.is_empty(), "kind {k:?} produced empty rule");
        for r in rules {
            match r {
                PartialRule::OperandTimes { idx, .. } => {
                    assert!(*idx < 2, "operand idx out of range : {idx}");
                }
            }
        }
    }
}

#[test]
fn recognize_gpu_ad_op_name_for_alloc() {
    use cssl_autodiff::gpu::GpuAdOp;
    let op = recognize_gpu_ad_op_name("cssl.diff.gpu_tape_alloc").unwrap();
    assert_eq!(op, GpuAdOp::Alloc);
}

#[test]
fn fmul_partial_rule_uses_other_operand_for_both_partials() {
    let rules = reverse_partial_rule(OpRecordKind::FMul);
    assert_eq!(rules.len(), 2);
    let factors: Vec<_> = rules
        .iter()
        .map(|r| match r {
            PartialRule::OperandTimes { factor, .. } => *factor,
        })
        .collect();
    assert!(matches!(factors[0], PartialFactor::OtherOperand { idx: 1 }));
    assert!(matches!(factors[1], PartialFactor::OtherOperand { idx: 0 }));
}

#[test]
fn fdiv_partial_rule_uses_quotient_neg_squared() {
    let rules = reverse_partial_rule(OpRecordKind::FDiv);
    let second = match rules[1] {
        PartialRule::OperandTimes { factor, .. } => factor,
    };
    assert_eq!(second, PartialFactor::QuotientNegSquared);
}

#[test]
fn supported_target_envs_include_kernel_for_level_zero() {
    assert!(supports_diff_shader(SpirvTargetEnv::OpenClKernel2_2));
}

#[test]
fn invalid_config_zero_capacity_rejected_by_emitter() {
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    let mut cfg = DiffShaderConfig::default_forward();
    cfg.tape_capacity = 0;
    assert!(emit_forward_diff_shader(&mut m, &cfg, "bad", &[]).is_err());
}
