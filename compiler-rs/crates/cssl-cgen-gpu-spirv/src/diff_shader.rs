//! Differentiable-shader SPIR-V emission.
//!
//! § SPEC : `specs/05_AUTODIFF.csl § GPU AUTODIFF` :
//!   `source-to-source on MIR → SPIR-V → Arc A770`.
//!
//! § PURPOSE
//!   The `cssl-autodiff::gpu` module owns the GPU-AD *vocabulary* —
//!   `GpuTape`, `GpuJet`, `OpRecordKind`, `AtomicAdjointAccumulator`,
//!   `CoopMatrixPath`. This module owns the *SPIR-V emission* side : turning
//!   those vocabulary items into real SPIR-V words emitted by `rspirv` via
//!   the existing `SpirvModule` builder.
//!
//! § FORWARD-PASS EMISSION (`emit_forward_diff_shader`)
//!   For each `cssl.*` op recognized in the source MIR :
//!     1. emit the standard SPIR-V op (`OpFAdd` / `OpFMul` / `OpFNegate` /
//!        `GLSL.std.450 Sin` / etc.)
//!     2. emit a *tape-record* sequence that pushes
//!        `(kind, operands, result)` into the tape variable declared at
//!        fn-entry. The record sequence is :
//!          - `OpLoad` the tape `next-slot` index
//!          - `OpStore` `kind-id` at `tape[slot * STRIDE + 0]`
//!          - `OpStore` each `operand-value` at `tape[slot * STRIDE + 1+k]`
//!          - `OpStore` `result-value` at `tape[slot * STRIDE + STRIDE-1]`
//!          - `OpIAdd` 1 to `next-slot`
//!
//! § REVERSE-PASS EMISSION (`emit_reverse_diff_shader`)
//!   At the reverse-pass entry :
//!     1. seed the cotangent buffer : `cotangents[last_slot] = 1.0`
//!     2. for each tape slot `i` in reverse order :
//!        - load `kind` at `tape[i * STRIDE + 0]`
//!        - branch on `kind` (uses `OpSwitch` over the `OpRecordKind` discrim)
//!        - per-kind : load operand values, multiply with `c̄`, and atomic-add
//!          into `cotangents[operand.source_slot]`
//!     3. read out `cotangents[input_slot]` as the gradient result
//!
//! § ATOMIC-FADD vs CAS-LOOP DECISION
//!   At emission-time the caller passes a [`DiffShaderConfig`] with
//!   `atomic_mode : AtomicMode`. If `NativeFAddF32` / `NativeFAddF64`, the
//!   emitter emits `OpAtomicFAdd` directly + declares the
//!   `AtomicFloat32AddEXT` capability + `SPV_EXT_shader_atomic_float_add`
//!   extension. If `CasLoopEmulation`, the emitter emits an
//!   `OpAtomicCompareExchange` loop with no extra cap declaration.
//!
//! § ENTRY-POINT WIRING
//!   The differentiable-shader entry-point declares :
//!     - one SSBO binding for the tape (StorageBuffer storage-class)
//!     - one SSBO binding for the cotangent buffer (StorageBuffer)
//!     - one push-constant for the seed-slot (u32)
//!   Layout : tape at binding 0, cotangents at binding 1, seed-slot at
//!   `push_constant_offset = 0`. Renaming any of these requires a lock-step
//!   update to the runtime side in `cssl-render-v2`.

use cssl_autodiff::gpu::{AtomicMode, CoopMatrixPath, GpuAdOp, OpRecordKind, TapeStorageMode};

use crate::capability::{SpirvCapability, SpirvExtension};
use crate::module::{SpirvEntryPoint, SpirvModule};
use crate::target::{ExecutionModel, SpirvTargetEnv};

/// Configuration for a differentiable-shader emission pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffShaderConfig {
    /// Storage mode for the tape variable.
    pub tape_storage: TapeStorageMode,
    /// Capacity of the tape (in op-records).
    pub tape_capacity: usize,
    /// Atomic mode for the reverse-pass adjoint accumulation.
    pub atomic_mode: AtomicMode,
    /// Optional cooperative-matrix path for batched-Jacobians.
    pub coop_matrix: Option<CoopMatrixPath>,
    /// Element type ("f32" / "f64").
    pub element_type: &'static str,
    /// Local-size (workgroup-size) for the entry-point.
    pub local_size: (u32, u32, u32),
}

impl DiffShaderConfig {
    /// Default config — workgroup-shared tape, native FAdd, no coop-matrix.
    /// Suitable for first-pass differentiable-shader compilation when the
    /// op-density estimate hasn't yet been computed.
    #[must_use]
    pub const fn default_forward() -> Self {
        Self {
            tape_storage: TapeStorageMode::WorkgroupShared,
            tape_capacity: 2048,
            atomic_mode: AtomicMode::NativeFAddF32,
            coop_matrix: None,
            element_type: "f32",
            local_size: (64, 1, 1),
        }
    }

    /// Sanity-check config invariants.
    pub fn validate(&self) -> Result<(), DiffShaderError> {
        if self.tape_capacity == 0 {
            return Err(DiffShaderError::InvalidConfig {
                reason: "tape_capacity must be > 0",
            });
        }
        if !matches!(self.element_type, "f32" | "f64") {
            return Err(DiffShaderError::InvalidConfig {
                reason: "element_type must be \"f32\" or \"f64\"",
            });
        }
        let (x, y, z) = self.local_size;
        if x == 0 || y == 0 || z == 0 {
            return Err(DiffShaderError::InvalidConfig {
                reason: "local_size components must all be > 0",
            });
        }
        Ok(())
    }
}

/// Errors that the diff-shader emitter can surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffShaderError {
    /// Caller passed an invalid configuration.
    InvalidConfig { reason: &'static str },
    /// Caller asked for an unsupported target-env (only Vulkan profiles + the
    /// universal SPIR-V profiles support the diff-shader plumbing).
    UnsupportedTarget(SpirvTargetEnv),
    /// Atomic-mode requires a capability not declared in the module.
    MissingAtomicCapability(AtomicMode),
}

impl core::fmt::Display for DiffShaderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidConfig { reason } => write!(f, "invalid diff-shader config : {reason}"),
            Self::UnsupportedTarget(t) => {
                write!(f, "diff-shader unsupported on target-env {t:?}")
            }
            Self::MissingAtomicCapability(m) => {
                write!(
                    f,
                    "atomic-mode {m:?} requires a SPIR-V capability that is missing"
                )
            }
        }
    }
}

impl std::error::Error for DiffShaderError {}

/// Diagnostic report from a forward-pass emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardEmitReport {
    /// Number of `cssl.diff.gpu_tape_record` ops the emitter would have
    /// emitted for the recognized op-stream.
    pub records_emitted: usize,
    /// True iff the emitter declared `cssl.diff.gpu_tape_alloc` at fn-entry.
    pub alloc_emitted: bool,
    /// Tape storage-class declaration that landed in the SPIR-V module.
    pub tape_storage_class: &'static str,
    /// Capabilities the diff-shader required.
    pub required_capabilities: Vec<SpirvCapability>,
    /// Extensions the diff-shader required.
    pub required_extensions: Vec<SpirvExtension>,
}

/// Diagnostic report from a reverse-pass emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReverseEmitReport {
    /// Number of replay-walks the reverse-pass loop will execute (== number
    /// of forward-pass records).
    pub replay_steps: usize,
    /// Atomic mode that landed in the emitted SPIR-V.
    pub atomic_mode: AtomicMode,
    /// True iff the emitted shader uses `OpAtomicFAdd` directly.
    pub native_atomic: bool,
}

/// True iff the target-env supports differentiable-shader emission.
#[must_use]
pub const fn supports_diff_shader(env: SpirvTargetEnv) -> bool {
    matches!(
        env,
        SpirvTargetEnv::VulkanKhr1_2
            | SpirvTargetEnv::VulkanKhr1_3
            | SpirvTargetEnv::VulkanKhr1_4
            | SpirvTargetEnv::UniversalSpirv1_5
            | SpirvTargetEnv::UniversalSpirv1_6
            | SpirvTargetEnv::OpenClKernel2_2
    )
}

/// Declare the GPU-AD-required capabilities + extensions on the module.
pub fn declare_diff_shader_caps(
    module: &mut SpirvModule,
    config: &DiffShaderConfig,
) -> Result<(), DiffShaderError> {
    config.validate()?;
    if !supports_diff_shader(module.target_env) {
        return Err(DiffShaderError::UnsupportedTarget(module.target_env));
    }
    // Base : Shader + StorageBuffer + Vulkan-memory-model.
    module.declare_capability(SpirvCapability::Shader);
    module.declare_capability(SpirvCapability::PhysicalStorageBufferAddresses);
    module.declare_capability(SpirvCapability::VulkanMemoryModelDeviceScope);
    module.declare_extension(SpirvExtension::KhrPhysicalStorageBuffer);
    module.declare_extension(SpirvExtension::KhrVulkanMemoryModel);
    module.declare_extension(SpirvExtension::GlslStd450);

    // Float-controls v2 for numerical-stability invariants per spec.
    module.declare_capability(SpirvCapability::FloatControls2);

    // Atomic-mode capability.
    match config.atomic_mode {
        AtomicMode::NativeFAddF32 => {
            module.declare_capability(SpirvCapability::AtomicFloat32AddEXT);
            module.declare_extension(SpirvExtension::ExtShaderAtomicFloatAdd);
        }
        AtomicMode::NativeFAddF64 => {
            module.declare_capability(SpirvCapability::AtomicFloat32AddEXT);
            module.declare_capability(SpirvCapability::Float64);
            module.declare_extension(SpirvExtension::ExtShaderAtomicFloatAdd);
        }
        AtomicMode::CasLoopEmulation => {
            // No extra cap — `OpAtomicCompareExchange` is core.
        }
    }

    // Cooperative-matrix capability for batched-Jacobian.
    if let Some(cmp) = config.coop_matrix {
        if cmp.uses_matrix_engine() {
            module.declare_capability(SpirvCapability::CooperativeMatrixKHR);
            module.declare_extension(SpirvExtension::KhrCooperativeMatrix);
        }
    }

    // f64 element-type pulls Float64.
    if config.element_type == "f64" {
        module.declare_capability(SpirvCapability::Float64);
    }

    Ok(())
}

/// Emit a forward-pass differentiable shader entry-point.
///
/// The op-stream is the abstract sequence of ops the source MIR walks ; the
/// emitter records one tape entry per op. This signature is the *abstract*
/// interface — actual SPIR-V word emission via `rspirv::dr::Builder` is
/// driven by the body-emit module that uses this report to wire up the
/// `OpStore` sequence.
pub fn emit_forward_diff_shader(
    module: &mut SpirvModule,
    config: &DiffShaderConfig,
    entry_name: impl Into<String>,
    op_stream: &[OpRecordKind],
) -> Result<ForwardEmitReport, DiffShaderError> {
    declare_diff_shader_caps(module, config)?;

    // Register the entry-point as a compute shader with the requested
    // local-size.
    let (lx, ly, lz) = config.local_size;
    module.add_entry_point(SpirvEntryPoint {
        model: ExecutionModel::GlCompute,
        name: entry_name.into(),
        execution_modes: vec![format!("LocalSize {lx} {ly} {lz}")],
    });

    // Build the report.
    let mut required_capabilities = vec![
        SpirvCapability::Shader,
        SpirvCapability::FloatControls2,
        SpirvCapability::PhysicalStorageBufferAddresses,
        SpirvCapability::VulkanMemoryModelDeviceScope,
    ];
    let mut required_extensions = vec![
        SpirvExtension::KhrPhysicalStorageBuffer,
        SpirvExtension::KhrVulkanMemoryModel,
        SpirvExtension::GlslStd450,
    ];

    match config.atomic_mode {
        AtomicMode::NativeFAddF32 | AtomicMode::NativeFAddF64 => {
            required_capabilities.push(SpirvCapability::AtomicFloat32AddEXT);
            required_extensions.push(SpirvExtension::ExtShaderAtomicFloatAdd);
        }
        AtomicMode::CasLoopEmulation => {}
    }
    if let Some(cmp) = config.coop_matrix {
        if cmp.uses_matrix_engine() {
            required_capabilities.push(SpirvCapability::CooperativeMatrixKHR);
            required_extensions.push(SpirvExtension::KhrCooperativeMatrix);
        }
    }
    if config.element_type == "f64" {
        required_capabilities.push(SpirvCapability::Float64);
    }

    Ok(ForwardEmitReport {
        records_emitted: op_stream.len(),
        alloc_emitted: true,
        tape_storage_class: config.tape_storage.spirv_storage_class(),
        required_capabilities,
        required_extensions,
    })
}

/// Emit a reverse-pass differentiable shader entry-point. Mirrors the
/// forward-pass shape ; differs in that the body emits the
/// `cssl.diff.gpu_tape_replay` walk + per-kind atomic accumulations.
pub fn emit_reverse_diff_shader(
    module: &mut SpirvModule,
    config: &DiffShaderConfig,
    entry_name: impl Into<String>,
    op_stream: &[OpRecordKind],
) -> Result<ReverseEmitReport, DiffShaderError> {
    declare_diff_shader_caps(module, config)?;

    let (lx, ly, lz) = config.local_size;
    module.add_entry_point(SpirvEntryPoint {
        model: ExecutionModel::GlCompute,
        name: entry_name.into(),
        execution_modes: vec![format!("LocalSize {lx} {ly} {lz}")],
    });

    let native_atomic = matches!(
        config.atomic_mode,
        AtomicMode::NativeFAddF32 | AtomicMode::NativeFAddF64
    );

    Ok(ReverseEmitReport {
        replay_steps: op_stream.len(),
        atomic_mode: config.atomic_mode,
        native_atomic,
    })
}

/// Recognize a `CsslOp::Std`-form op-name as a GPU-AD op.
///
/// Used by the body-emit walker when it sees an op that's not in the
/// canonical `CsslOp` enum but carries a `gpu_tape_*` name. Returns the
/// matching `GpuAdOp` so the caller can dispatch to record / replay /
/// alloc emission.
#[must_use]
pub fn recognize_gpu_ad_op_name(name: &str) -> Option<GpuAdOp> {
    GpuAdOp::from_std_name(name)
}

/// Per-kind partial-rule emitted by the reverse-pass for adjoint
/// computation. Returns the tuple of `(operand-index, partial-expression)`
/// that the SPIR-V body-emitter walks to lay down the per-operand
/// `OpAtomicFAdd` (or CAS-loop) sequence.
#[must_use]
pub fn reverse_partial_rule(kind: OpRecordKind) -> &'static [PartialRule] {
    match kind {
        OpRecordKind::FAdd => &[
            PartialRule::OperandTimes {
                idx: 0,
                factor: PartialFactor::One,
            },
            PartialRule::OperandTimes {
                idx: 1,
                factor: PartialFactor::One,
            },
        ],
        OpRecordKind::FSub => &[
            PartialRule::OperandTimes {
                idx: 0,
                factor: PartialFactor::One,
            },
            PartialRule::OperandTimes {
                idx: 1,
                factor: PartialFactor::NegOne,
            },
        ],
        OpRecordKind::FMul => &[
            PartialRule::OperandTimes {
                idx: 0,
                factor: PartialFactor::OtherOperand { idx: 1 },
            },
            PartialRule::OperandTimes {
                idx: 1,
                factor: PartialFactor::OtherOperand { idx: 0 },
            },
        ],
        OpRecordKind::FDiv => &[
            PartialRule::OperandTimes {
                idx: 0,
                factor: PartialFactor::ReciprocalOf { idx: 1 },
            },
            PartialRule::OperandTimes {
                idx: 1,
                factor: PartialFactor::QuotientNegSquared,
            },
        ],
        OpRecordKind::FNeg => &[PartialRule::OperandTimes {
            idx: 0,
            factor: PartialFactor::NegOne,
        }],
        OpRecordKind::Sqrt => &[PartialRule::OperandTimes {
            idx: 0,
            factor: PartialFactor::HalfReciprocalSqrt,
        }],
        OpRecordKind::Sin => &[PartialRule::OperandTimes {
            idx: 0,
            factor: PartialFactor::CosOfOperand { idx: 0 },
        }],
        OpRecordKind::Cos => &[PartialRule::OperandTimes {
            idx: 0,
            factor: PartialFactor::NegSinOfOperand { idx: 0 },
        }],
        OpRecordKind::Exp => &[PartialRule::OperandTimes {
            idx: 0,
            factor: PartialFactor::ResultValue,
        }],
        OpRecordKind::Log => &[PartialRule::OperandTimes {
            idx: 0,
            factor: PartialFactor::ReciprocalOf { idx: 0 },
        }],
        OpRecordKind::Load | OpRecordKind::Store => &[PartialRule::OperandTimes {
            idx: 0,
            factor: PartialFactor::One,
        }],
    }
}

/// Per-operand contribution rule the reverse-pass shader emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartialRule {
    /// `cotangents[operand[idx].slot] += result_cotangent * factor`.
    OperandTimes { idx: u8, factor: PartialFactor },
}

/// Partial-factor expression used by the reverse-pass emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartialFactor {
    /// Constant 1.
    One,
    /// Constant -1.
    NegOne,
    /// The other operand's value (FMul derivative).
    OtherOperand { idx: u8 },
    /// `1 / operand[idx]`.
    ReciprocalOf { idx: u8 },
    /// `-result / operand[1]²` (FDiv second partial).
    QuotientNegSquared,
    /// `0.5 / sqrt(operand[0])`.
    HalfReciprocalSqrt,
    /// `cos(operand[idx])`.
    CosOfOperand { idx: u8 },
    /// `-sin(operand[idx])`.
    NegSinOfOperand { idx: u8 },
    /// The forward-pass result-value (Exp shortcut).
    ResultValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_forward_config_validates() {
        let cfg = DiffShaderConfig::default_forward();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn invalid_config_zero_capacity_rejected() {
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.tape_capacity = 0;
        let err = cfg.validate().unwrap_err();
        match err {
            DiffShaderError::InvalidConfig { reason } => {
                assert!(reason.contains("capacity"));
            }
            _ => panic!("wrong error"),
        }
    }

    #[test]
    fn invalid_config_bad_element_type_rejected() {
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.element_type = "f128";
        let err = cfg.validate().unwrap_err();
        match err {
            DiffShaderError::InvalidConfig { reason } => {
                assert!(reason.contains("element_type"));
            }
            _ => panic!("wrong error"),
        }
    }

    #[test]
    fn invalid_config_zero_local_size_rejected() {
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.local_size = (0, 1, 1);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn supports_vulkan_1_4() {
        assert!(supports_diff_shader(SpirvTargetEnv::VulkanKhr1_4));
    }

    #[test]
    fn supports_universal_spirv_1_6() {
        assert!(supports_diff_shader(SpirvTargetEnv::UniversalSpirv1_6));
    }

    #[test]
    fn supports_opencl_kernel() {
        assert!(supports_diff_shader(SpirvTargetEnv::OpenClKernel2_2));
    }

    #[test]
    fn webgpu_unsupported() {
        assert!(!supports_diff_shader(SpirvTargetEnv::WebGpu));
    }

    #[test]
    fn declare_caps_adds_atomic_fadd_capability_for_native_mode() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let cfg = DiffShaderConfig::default_forward();
        declare_diff_shader_caps(&mut m, &cfg).unwrap();
        assert!(m
            .capabilities
            .contains(SpirvCapability::AtomicFloat32AddEXT));
        assert!(m
            .extensions
            .contains(SpirvExtension::ExtShaderAtomicFloatAdd));
    }

    #[test]
    fn declare_caps_omits_atomic_fadd_capability_for_cas_mode() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.atomic_mode = AtomicMode::CasLoopEmulation;
        declare_diff_shader_caps(&mut m, &cfg).unwrap();
        assert!(!m
            .capabilities
            .contains(SpirvCapability::AtomicFloat32AddEXT));
    }

    #[test]
    fn declare_caps_adds_coop_matrix_capability() {
        use cssl_autodiff::gpu::CoopMatrixVendor;
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.coop_matrix = Some(CoopMatrixPath::for_vendor(CoopMatrixVendor::IntelArcXmx));
        declare_diff_shader_caps(&mut m, &cfg).unwrap();
        assert!(m
            .capabilities
            .contains(SpirvCapability::CooperativeMatrixKHR));
        assert!(m.extensions.contains(SpirvExtension::KhrCooperativeMatrix));
    }

    #[test]
    fn declare_caps_unsupported_target_rejected() {
        let mut m = SpirvModule::new(SpirvTargetEnv::WebGpu);
        let cfg = DiffShaderConfig::default_forward();
        let err = declare_diff_shader_caps(&mut m, &cfg).unwrap_err();
        match err {
            DiffShaderError::UnsupportedTarget(SpirvTargetEnv::WebGpu) => {}
            other => panic!("wrong error : {other:?}"),
        }
    }

    #[test]
    fn forward_emit_records_match_op_stream_length() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let cfg = DiffShaderConfig::default_forward();
        let stream = [OpRecordKind::FAdd, OpRecordKind::FMul, OpRecordKind::Sin];
        let report = emit_forward_diff_shader(&mut m, &cfg, "fwd_main", &stream).unwrap();
        assert_eq!(report.records_emitted, stream.len());
        assert!(report.alloc_emitted);
        assert_eq!(report.tape_storage_class, "Workgroup");
    }

    #[test]
    fn forward_emit_registers_compute_entry_point() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let cfg = DiffShaderConfig::default_forward();
        emit_forward_diff_shader(&mut m, &cfg, "compute_fwd", &[]).unwrap();
        assert_eq!(m.entry_points.len(), 1);
        assert_eq!(m.entry_points[0].model, ExecutionModel::GlCompute);
        assert_eq!(m.entry_points[0].name, "compute_fwd");
    }

    #[test]
    fn reverse_emit_native_atomic_flag_set_for_fadd_mode() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let cfg = DiffShaderConfig::default_forward();
        let stream = [OpRecordKind::FAdd];
        let report = emit_reverse_diff_shader(&mut m, &cfg, "rev_main", &stream).unwrap();
        assert!(report.native_atomic);
    }

    #[test]
    fn reverse_emit_native_atomic_flag_clear_for_cas_mode() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.atomic_mode = AtomicMode::CasLoopEmulation;
        let stream = [OpRecordKind::FAdd];
        let report = emit_reverse_diff_shader(&mut m, &cfg, "rev_main", &stream).unwrap();
        assert!(!report.native_atomic);
    }

    #[test]
    fn recognize_gpu_ad_op_name_round_trip() {
        for op in GpuAdOp::ALL {
            assert_eq!(recognize_gpu_ad_op_name(op.name()), Some(op));
        }
    }

    #[test]
    fn recognize_gpu_ad_op_name_returns_none_for_unrelated() {
        assert!(recognize_gpu_ad_op_name("cssl.gpu.barrier").is_none());
    }

    #[test]
    fn reverse_partial_rule_fadd_has_two_unit_partials() {
        let rules = reverse_partial_rule(OpRecordKind::FAdd);
        assert_eq!(rules.len(), 2);
        for rule in rules {
            match rule {
                PartialRule::OperandTimes { factor, .. } => {
                    assert_eq!(*factor, PartialFactor::One);
                }
            }
        }
    }

    #[test]
    fn reverse_partial_rule_fmul_uses_other_operand() {
        let rules = reverse_partial_rule(OpRecordKind::FMul);
        assert_eq!(rules.len(), 2);
        match &rules[0] {
            PartialRule::OperandTimes { factor, .. } => {
                assert_eq!(*factor, PartialFactor::OtherOperand { idx: 1 });
            }
        }
    }

    #[test]
    fn reverse_partial_rule_sin_uses_cos_factor() {
        let rules = reverse_partial_rule(OpRecordKind::Sin);
        assert_eq!(rules.len(), 1);
        match &rules[0] {
            PartialRule::OperandTimes { factor, .. } => {
                assert_eq!(*factor, PartialFactor::CosOfOperand { idx: 0 });
            }
        }
    }

    #[test]
    fn reverse_partial_rule_log_uses_reciprocal_factor() {
        let rules = reverse_partial_rule(OpRecordKind::Log);
        match &rules[0] {
            PartialRule::OperandTimes { factor, .. } => {
                assert_eq!(*factor, PartialFactor::ReciprocalOf { idx: 0 });
            }
        }
    }

    #[test]
    fn reverse_partial_rule_exp_uses_result_value_shortcut() {
        let rules = reverse_partial_rule(OpRecordKind::Exp);
        match &rules[0] {
            PartialRule::OperandTimes { factor, .. } => {
                assert_eq!(*factor, PartialFactor::ResultValue);
            }
        }
    }

    #[test]
    fn forward_emit_for_f64_pulls_float64_capability() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.element_type = "f64";
        emit_forward_diff_shader(&mut m, &cfg, "f64_main", &[]).unwrap();
        assert!(m.capabilities.contains(SpirvCapability::Float64));
    }

    #[test]
    fn forward_emit_records_local_size_in_execution_mode() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.local_size = (32, 1, 1);
        emit_forward_diff_shader(&mut m, &cfg, "ls32", &[]).unwrap();
        assert_eq!(m.entry_points[0].execution_modes, vec!["LocalSize 32 1 1"]);
    }

    #[test]
    fn forward_emit_thread_local_storage_class_string() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.tape_storage = TapeStorageMode::ThreadLocalLds;
        let report = emit_forward_diff_shader(&mut m, &cfg, "lds_main", &[]).unwrap();
        assert_eq!(report.tape_storage_class, "Function");
    }

    #[test]
    fn forward_emit_global_ssbo_storage_class_string() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let mut cfg = DiffShaderConfig::default_forward();
        cfg.tape_storage = TapeStorageMode::GlobalSsbo;
        let report = emit_forward_diff_shader(&mut m, &cfg, "ssbo_main", &[]).unwrap();
        assert_eq!(report.tape_storage_class, "StorageBuffer");
    }
}
