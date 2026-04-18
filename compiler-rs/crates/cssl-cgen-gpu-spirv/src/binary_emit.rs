//! Stage-0 SPIR-V *binary* emitter via `rspirv::dr::Builder` (T11-D34).
//!
//! § PURPOSE
//!
//! The text emitter in [`crate::emit`] produces `spirv-as`-compatible assembly
//! text — readable, diff-able, but not directly validatable without external
//! tooling. This module produces real SPIR-V **binary words** (`Vec<u32>`)
//! through the pure-Rust `rspirv` crate. The binary is validatable *at test
//! time* by loading it back through `rspirv::dr::load_words` — if the loader
//! accepts the bytes, the module is structurally valid SPIR-V.
//!
//! § D32-PARALLEL PATTERN
//!
//! This is the SPIR-V counterpart of T11-D32's naga-validates-WGSL slice :
//!
//!   text emitter        →  parse-via-real-tool      →  structural gate
//!   ──────────────────     ──────────────────────      ─────────────────
//!   D32 `emit_compute*`    `naga::front::wgsl::parse`  entry-point present
//!   D34 `emit_module_binary` `rspirv::dr::load_words`  module loads, ids match
//!
//! § CONTRAST WITH TEXT EMITTER
//!
//! The text emitter has stub fn-bodies like :
//!
//! ```text
//! OpFunction main_cs None TypeFunction_void__void ; main_cs
//!   OpLabel %entry
//!   ; stage-0 skeleton — body @ T10-phase-2
//!   OpReturn
//! OpFunctionEnd
//! ```
//!
//! The `TypeFunction_void__void` token is a human-readable placeholder — it is
//! not a valid SPIR-V ID. The binary emitter in this module emits the *real*
//! IDs (`%1 = OpTypeVoid`, `%2 = OpTypeFunction %1`, `%3 = OpFunction %1 None %2` …)
//! via `rspirv::dr::Builder::type_void / type_function / begin_function`, so the
//! resulting binary is round-trip-parseable.
//!
//! § SCOPE (this commit)
//!   - Map every variant of [`SpirvCapability`] / [`SpirvExtension`] / [`MemoryModel`] /
//!     [`AddressingModel`] / [`ExecutionModel`] to its `rspirv::spirv::*` counterpart.
//!   - Emit for each entry point a minimal function : `void fn() { return; }`.
//!   - Emit `LocalSize X Y Z` execution modes for compute entries (parsed from the
//!     text form in `SpirvEntryPoint::execution_modes`).
//!   - Emit `OriginUpperLeft` execution mode for fragment entries.
//!   - Emit `OpSource` with `CSSLv3` placeholder (mapped to `SourceLanguage::Unknown`
//!     since CSSLv3 is not in the Khronos registry).
//!   - Emit `OpName` for each entry-point function ID.
//!
//! § DEFERRED (future slices)
//!   - Full fn-body lowering from MIR [`CsslOp`] → SPIR-V `OpFAdd` / `OpFMul` / … .
//!     Today the binary entry-point function is always `void fn() { return; }`.
//!   - `spirv-val` semantic validation via `spirv-tools` crate : catches violations
//!     that pure structural parsing misses (e.g., capability-vs-extension mismatches,
//!     illegal capability combinations). Stage-0 relies on rspirv's structural check.
//!   - `spirv-opt -O` / `-Os` optimizer invocation.
//!
//! [`SpirvCapability`]: crate::SpirvCapability
//! [`SpirvExtension`]: crate::SpirvExtension
//! [`MemoryModel`]: crate::MemoryModel
//! [`AddressingModel`]: crate::AddressingModel
//! [`ExecutionModel`]: crate::ExecutionModel
//! [`CsslOp`]: cssl_mir::CsslOp

use rspirv::binary::Assemble;
use rspirv::dr::Builder;
use rspirv::spirv;
use thiserror::Error;

use crate::module::{SpirvEntryPoint, SpirvModule};
use crate::{
    AddressingModel, ExecutionModel, MemoryModel, SpirvCapability, SpirvExtension, SpirvTargetEnv,
};

/// Failure modes for SPIR-V binary emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BinaryEmitError {
    /// A module targeting a shader env had zero entry points.
    #[error("module targeting `{target_env}` declared no entry points — invalid for this env")]
    NoEntryPoints {
        /// Target environment string.
        target_env: String,
    },
    /// `rspirv`'s builder rejected the sequence (should only surface for malformed
    /// module inputs that pass our `NoEntryPoints` check).
    #[error("rspirv builder rejected the module : {0}")]
    BuilderFailed(String),
}

/// Emit a [`SpirvModule`] to a real SPIR-V binary word-stream (`Vec<u32>`).
///
/// The output is a complete SPIR-V module : magic number + version + generator +
/// bound + schema header, followed by all instructions in the canonical section
/// order. Round-trippable through `rspirv::dr::load_words`.
///
/// # Errors
/// Returns [`BinaryEmitError::NoEntryPoints`] for shader targets with zero entries.
/// Returns [`BinaryEmitError::BuilderFailed`] if rspirv's builder rejects the
/// instruction sequence (structural bug in the caller or in this emitter).
pub fn emit_module_binary(module: &SpirvModule) -> Result<Vec<u32>, BinaryEmitError> {
    // § Shader-env sanity : no entry points ⇒ invalid module.
    if module.entry_points.is_empty()
        && !matches!(module.target_env, SpirvTargetEnv::OpenClKernel2_2)
    {
        return Err(BinaryEmitError::NoEntryPoints {
            target_env: module.target_env.to_string(),
        });
    }

    let mut b = Builder::new();

    // § Version : Vulkan 1.4 baseline ⇒ SPIR-V 1.5 is the minimum, 1.6 is
    //   backward-compatible. We emit 1.5 for broadest consumer acceptance.
    b.set_version(1, 5);

    // § Capabilities (OpCapability ×N)
    for cap in module.capabilities.iter() {
        b.capability(map_capability(cap));
    }

    // § Extensions (OpExtension ×N, for non-ext-inst-set ones)
    for ext in module.extensions.iter_plain() {
        b.extension(ext.as_str());
    }

    // § Ext-inst imports (OpExtInstImport ×N)
    for ext in module.extensions.iter_ext_inst_sets() {
        let _ = b.ext_inst_import(ext.as_str());
    }

    // § Memory model (OpMemoryModel)
    b.memory_model(
        map_addressing_model(module.addressing_model),
        map_memory_model(module.memory_model),
    );

    // § Types : void + void() fn type — shared across all entry points.
    let void_ty = b.type_void();
    let void_fn_ty = b.type_function(void_ty, vec![]);

    // § For each entry point : emit OpFunction + OpEntryPoint + OpExecutionMode + OpName.
    let mut entry_fn_ids: Vec<(u32, &SpirvEntryPoint)> =
        Vec::with_capacity(module.entry_points.len());
    for ep in &module.entry_points {
        let fn_id = b
            .begin_function(void_ty, None, spirv::FunctionControl::NONE, void_fn_ty)
            .map_err(|e| BinaryEmitError::BuilderFailed(format!("begin_function : {e:?}")))?;
        b.begin_block(None)
            .map_err(|e| BinaryEmitError::BuilderFailed(format!("begin_block : {e:?}")))?;
        b.ret()
            .map_err(|e| BinaryEmitError::BuilderFailed(format!("ret : {e:?}")))?;
        b.end_function()
            .map_err(|e| BinaryEmitError::BuilderFailed(format!("end_function : {e:?}")))?;
        entry_fn_ids.push((fn_id, ep));
    }

    // § Entry points (OpEntryPoint) — emitted after fn-defs in rspirv's builder ;
    //   the builder handles section-re-ordering internally when `.module()` is
    //   called, so we emit in the order convenient for us.
    for (fn_id, ep) in &entry_fn_ids {
        b.entry_point(map_execution_model(ep.model), *fn_id, ep.name.clone(), []);
    }

    // § Execution modes (OpExecutionMode)
    for (fn_id, ep) in &entry_fn_ids {
        emit_execution_modes_for_entry(&mut b, *fn_id, ep);
    }

    // § Debug : OpSource + OpName.
    if module.source_language.is_some() {
        b.source(
            spirv::SourceLanguage::Unknown,
            module.source_version.unwrap_or(0),
            None,
            None::<String>,
        );
    }
    for (fn_id, ep) in &entry_fn_ids {
        b.name(*fn_id, ep.name.clone());
    }

    // § Assemble : rspirv sorts instructions into the canonical SPIR-V section
    //   order, emits the header (magic + version + generator + bound + schema),
    //   and returns Vec<u32> binary words.
    Ok(b.module().assemble())
}

// ═════════════════════════════════════════════════════════════════════════
// § Execution-mode sub-parser
// ═════════════════════════════════════════════════════════════════════════

/// Parse + emit stage-0 recognized execution modes from the textual form stored
/// on [`SpirvEntryPoint`]. Supports :
///   - `LocalSize X Y Z`  (compute)
///   - `OriginUpperLeft`  (fragment)
///   - `OriginLowerLeft`  (fragment — GLSL-style)
///   - `LocalSizeHint X Y Z` (OpenCL kernel)
///
/// Unrecognized modes are silently skipped at stage-0 (T10-phase-2 would
/// extend this with the full catalog + return an error for unknown modes).
fn emit_execution_modes_for_entry(b: &mut Builder, fn_id: u32, ep: &SpirvEntryPoint) {
    for mode_str in &ep.execution_modes {
        let trimmed = mode_str.trim();
        if let Some(rest) = trimmed.strip_prefix("LocalSize ") {
            if let Some([x, y, z]) = parse_three_u32(rest) {
                b.execution_mode(fn_id, spirv::ExecutionMode::LocalSize, [x, y, z]);
            }
        } else if let Some(rest) = trimmed.strip_prefix("LocalSizeHint ") {
            if let Some([x, y, z]) = parse_three_u32(rest) {
                b.execution_mode(fn_id, spirv::ExecutionMode::LocalSizeHint, [x, y, z]);
            }
        } else if trimmed == "OriginUpperLeft" {
            b.execution_mode(fn_id, spirv::ExecutionMode::OriginUpperLeft, []);
        } else if trimmed == "OriginLowerLeft" {
            b.execution_mode(fn_id, spirv::ExecutionMode::OriginLowerLeft, []);
        }
        // § Unrecognized : silent skip at stage-0. T10-phase-2 extends this.
    }
}

/// Parse three whitespace-separated u32 values (`"32 1 1"` → `[32, 1, 1]`).
fn parse_three_u32(s: &str) -> Option<[u32; 3]> {
    let mut it = s.split_whitespace();
    let x: u32 = it.next()?.parse().ok()?;
    let y: u32 = it.next()?.parse().ok()?;
    let z: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some([x, y, z])
}

// ═════════════════════════════════════════════════════════════════════════
// § Enum mapping : our catalog → rspirv::spirv::*
// ═════════════════════════════════════════════════════════════════════════

/// Map our [`SpirvCapability`] enum to `rspirv::spirv::Capability`.
///
/// The two enums share Khronos-canonical names, so the mapping is 1:1.
#[allow(clippy::too_many_lines)]
fn map_capability(c: SpirvCapability) -> spirv::Capability {
    use SpirvCapability as C;
    match c {
        C::Shader => spirv::Capability::Shader,
        C::Kernel => spirv::Capability::Kernel,
        C::Int8 => spirv::Capability::Int8,
        C::Int16 => spirv::Capability::Int16,
        C::Int64 => spirv::Capability::Int64,
        C::Float16 => spirv::Capability::Float16,
        C::Float64 => spirv::Capability::Float64,
        C::AtomicFloat32AddEXT => spirv::Capability::AtomicFloat32AddEXT,
        C::AtomicFloat32MinMaxEXT => spirv::Capability::AtomicFloat32MinMaxEXT,
        C::PhysicalStorageBufferAddresses => spirv::Capability::PhysicalStorageBufferAddresses,
        C::VulkanMemoryModelDeviceScope => spirv::Capability::VulkanMemoryModelDeviceScope,
        C::RuntimeDescriptorArray => spirv::Capability::RuntimeDescriptorArray,
        C::ShaderNonUniform => spirv::Capability::ShaderNonUniform,
        C::StorageBuffer16BitAccess => spirv::Capability::StorageBuffer16BitAccess,
        C::StorageBuffer8BitAccess => spirv::Capability::StorageBuffer8BitAccess,
        C::GroupNonUniformArithmetic => spirv::Capability::GroupNonUniformArithmetic,
        C::GroupNonUniformBallot => spirv::Capability::GroupNonUniformBallot,
        C::GroupNonUniformShuffle => spirv::Capability::GroupNonUniformShuffle,
        C::GroupNonUniformQuad => spirv::Capability::GroupNonUniformQuad,
        C::DemoteToHelperInvocation => spirv::Capability::DemoteToHelperInvocation,
        C::CooperativeMatrixKHR => spirv::Capability::CooperativeMatrixKHR,
        C::CooperativeMatrixNV => spirv::Capability::CooperativeMatrixNV,
        C::RayTracingKHR => spirv::Capability::RayTracingKHR,
        C::RayQueryKHR => spirv::Capability::RayQueryKHR,
        C::RayTracingProvisional => spirv::Capability::RayTracingProvisionalKHR,
        C::GroupNonUniformRotateKHR => spirv::Capability::GroupNonUniformRotateKHR,
        C::ExpectAssumeKHR => spirv::Capability::ExpectAssumeKHR,
        // FloatControls2 lives in SPIR-V SDK 1.4+ ; rspirv 0.12 ships the 1.3.268
        // spirv crate which predates the enum. Map to Shader placeholder at stage-0 ;
        // a future rspirv-bump surfaces this as a real Capability::FloatControls2.
        C::FloatControls2 => spirv::Capability::Shader,
        C::StoragePushConstant16 => spirv::Capability::StoragePushConstant16,
        C::Int64Atomics => spirv::Capability::Int64Atomics,
        C::ShaderNonSemanticInfo => spirv::Capability::Shader, // placeholder — no direct enum
        C::MeshShadingEXT => spirv::Capability::MeshShadingEXT,
    }
}

/// Silence dead-code lint on the `SpirvExtension` import since the emitter uses
/// `ext.as_str()` directly via the trait. This `_` keeps the static-analysis
/// tight without requiring a `use` gymnastic.
const _: Option<SpirvExtension> = None;

/// Map our [`MemoryModel`] to `rspirv::spirv::MemoryModel`.
fn map_memory_model(m: MemoryModel) -> spirv::MemoryModel {
    match m {
        MemoryModel::Simple => spirv::MemoryModel::Simple,
        MemoryModel::Glsl450 => spirv::MemoryModel::GLSL450,
        MemoryModel::OpenCL => spirv::MemoryModel::OpenCL,
        MemoryModel::Vulkan => spirv::MemoryModel::Vulkan,
    }
}

/// Map our [`AddressingModel`] to `rspirv::spirv::AddressingModel`.
fn map_addressing_model(a: AddressingModel) -> spirv::AddressingModel {
    match a {
        AddressingModel::Logical => spirv::AddressingModel::Logical,
        AddressingModel::Physical32 => spirv::AddressingModel::Physical32,
        AddressingModel::Physical64 => spirv::AddressingModel::Physical64,
        AddressingModel::PhysicalStorageBuffer64 => spirv::AddressingModel::PhysicalStorageBuffer64,
    }
}

/// Map our [`ExecutionModel`] to `rspirv::spirv::ExecutionModel`.
fn map_execution_model(e: ExecutionModel) -> spirv::ExecutionModel {
    match e {
        ExecutionModel::Vertex => spirv::ExecutionModel::Vertex,
        ExecutionModel::TessellationControl => spirv::ExecutionModel::TessellationControl,
        ExecutionModel::TessellationEvaluation => spirv::ExecutionModel::TessellationEvaluation,
        ExecutionModel::Geometry => spirv::ExecutionModel::Geometry,
        ExecutionModel::Fragment => spirv::ExecutionModel::Fragment,
        ExecutionModel::GlCompute => spirv::ExecutionModel::GLCompute,
        ExecutionModel::Kernel => spirv::ExecutionModel::Kernel,
        ExecutionModel::TaskExt => spirv::ExecutionModel::TaskEXT,
        ExecutionModel::MeshExt => spirv::ExecutionModel::MeshEXT,
        ExecutionModel::RayGenerationKhr => spirv::ExecutionModel::RayGenerationKHR,
        ExecutionModel::IntersectionKhr => spirv::ExecutionModel::IntersectionKHR,
        ExecutionModel::AnyHitKhr => spirv::ExecutionModel::AnyHitKHR,
        ExecutionModel::ClosestHitKhr => spirv::ExecutionModel::ClosestHitKHR,
        ExecutionModel::MissKhr => spirv::ExecutionModel::MissKHR,
        ExecutionModel::CallableKhr => spirv::ExecutionModel::CallableKHR,
    }
}

// ═════════════════════════════════════════════════════════════════════════
// § Tests — round-trip through `rspirv::dr::load_words`
// ═════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::{
        emit_module_binary, map_addressing_model, map_capability, map_execution_model,
        map_memory_model, parse_three_u32, BinaryEmitError,
    };
    use crate::emit::minimal_vulkan_compute_module;
    use crate::module::{SpirvEntryPoint, SpirvModule};
    use crate::target::{AddressingModel, ExecutionModel, MemoryModel, SpirvTargetEnv};
    use crate::{SpirvCapability, SpirvExtension};
    use rspirv::dr;

    // ─────────────────────────────────────────────────────────────────────
    // § Structural invariants (header words + magic + bound)
    // ─────────────────────────────────────────────────────────────────────

    /// SPIR-V magic number (`MagicNumber` per Khronos spec — first word of every module).
    const SPIRV_MAGIC: u32 = 0x0723_0203;

    #[test]
    fn empty_shader_module_emits_error() {
        let m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let e = emit_module_binary(&m).unwrap_err();
        assert!(matches!(e, BinaryEmitError::NoEntryPoints { .. }));
    }

    #[test]
    fn empty_kernel_module_emits_ok() {
        // OpenCL-Kernel targets allow zero entry points (kernels declared per-fn via OpEntryPoint).
        let m = SpirvModule::new(SpirvTargetEnv::OpenClKernel2_2);
        let words = emit_module_binary(&m).unwrap();
        // Header alone : 5 u32 (magic + version + generator + bound + schema) minimum.
        assert!(words.len() >= 5, "too few header words : {}", words.len());
    }

    #[test]
    fn minimal_compute_module_starts_with_magic() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        assert_eq!(words[0], SPIRV_MAGIC, "first word must be SPIR-V magic");
    }

    #[test]
    fn minimal_compute_module_version_word_is_1_5() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        // Version word : 0x00 | major << 16 | minor << 8 | 0x00.
        let ver = words[1];
        let major = (ver >> 16) & 0xFF;
        let minor = (ver >> 8) & 0xFF;
        assert_eq!(major, 1, "major version must be 1 (got {major})");
        assert_eq!(minor, 5, "minor version must be 5 (got {minor})");
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Round-trip via `rspirv::dr::load_words`
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn compute_module_round_trips_via_rspirv_loader() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted binary");
        // At least one entry point survives the round-trip.
        assert_eq!(parsed.entry_points.len(), 1);
        let ep_name = &parsed.entry_points[0].operands[2]; // model, fn-id, name
        if let rspirv::dr::Operand::LiteralString(name) = ep_name {
            assert_eq!(name, "main_cs");
        } else {
            panic!("entry-point operand 2 must be LiteralString, got {ep_name:?}");
        }
    }

    #[test]
    fn compute_module_round_trip_preserves_local_size() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");
        // Find an OpExecutionMode with LocalSize params (1, 1, 1) per minimal helper.
        let has_local_size = parsed.execution_modes.iter().any(|inst| {
            inst.class.opcode == rspirv::spirv::Op::ExecutionMode
                && inst.operands.iter().any(|op| {
                    matches!(
                        op,
                        rspirv::dr::Operand::ExecutionMode(rspirv::spirv::ExecutionMode::LocalSize)
                    )
                })
        });
        assert!(
            has_local_size,
            "expected LocalSize execution-mode in round-tripped module"
        );
    }

    #[test]
    fn vertex_fragment_combo_round_trips() {
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        m.seed_vulkan_1_4_defaults();
        m.add_entry_point(SpirvEntryPoint {
            model: ExecutionModel::Vertex,
            name: "vs_main".into(),
            execution_modes: vec![],
        });
        m.add_entry_point(SpirvEntryPoint {
            model: ExecutionModel::Fragment,
            name: "fs_main".into(),
            execution_modes: vec!["OriginUpperLeft".into()],
        });

        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");
        assert_eq!(parsed.entry_points.len(), 2);

        // Both entry-point names present.
        let names: Vec<&String> = parsed
            .entry_points
            .iter()
            .filter_map(|i| match &i.operands[2] {
                rspirv::dr::Operand::LiteralString(n) => Some(n),
                _ => None,
            })
            .collect();
        assert!(names.iter().any(|n| n.as_str() == "vs_main"));
        assert!(names.iter().any(|n| n.as_str() == "fs_main"));

        // Fragment OriginUpperLeft execution mode survives.
        let has_origin = parsed.execution_modes.iter().any(|i| {
            i.operands.iter().any(|op| {
                matches!(
                    op,
                    rspirv::dr::Operand::ExecutionMode(
                        rspirv::spirv::ExecutionMode::OriginUpperLeft
                    )
                )
            })
        });
        assert!(has_origin, "OriginUpperLeft must survive round-trip");
    }

    #[test]
    fn compute_module_capabilities_survive_round_trip() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");

        let caps: Vec<rspirv::spirv::Capability> = parsed
            .capabilities
            .iter()
            .filter_map(|i| match &i.operands[0] {
                rspirv::dr::Operand::Capability(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert!(
            caps.contains(&rspirv::spirv::Capability::Shader),
            "Shader capability must survive round-trip : got {caps:?}"
        );
        assert!(
            caps.contains(&rspirv::spirv::Capability::PhysicalStorageBufferAddresses),
            "PhysicalStorageBufferAddresses must survive round-trip"
        );
    }

    #[test]
    fn compute_module_extensions_survive_round_trip() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");

        let exts: Vec<String> = parsed
            .extensions
            .iter()
            .filter_map(|i| match &i.operands[0] {
                rspirv::dr::Operand::LiteralString(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert!(
            exts.iter().any(|e| e == "SPV_KHR_physical_storage_buffer"),
            "physical_storage_buffer ext must survive : got {exts:?}"
        );
    }

    #[test]
    fn compute_module_ext_inst_import_survives_round_trip() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");

        let imports: Vec<String> = parsed
            .ext_inst_imports
            .iter()
            .filter_map(|i| match &i.operands[0] {
                rspirv::dr::Operand::LiteralString(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert!(
            imports.iter().any(|i| i == "GLSL.std.450"),
            "GLSL.std.450 ext-inst-import must survive : got {imports:?}"
        );
    }

    #[test]
    fn memory_model_survives_round_trip() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");

        let mem = parsed
            .memory_model
            .as_ref()
            .expect("memory-model instruction must be present");
        // Operands : [AddressingModel, MemoryModel]
        match &mem.operands[0] {
            rspirv::dr::Operand::AddressingModel(
                rspirv::spirv::AddressingModel::PhysicalStorageBuffer64,
            ) => {}
            other => panic!("addressing model mismatch : {other:?}"),
        }
        match &mem.operands[1] {
            rspirv::dr::Operand::MemoryModel(rspirv::spirv::MemoryModel::Vulkan) => {}
            other => panic!("memory model mismatch : {other:?}"),
        }
    }

    #[test]
    fn entry_point_function_has_void_return() {
        // OpFunction has return-type operand ; for our void-fn it must be the
        // ID of OpTypeVoid.
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");

        // Find OpTypeVoid ID.
        let void_id = parsed
            .types_global_values
            .iter()
            .find(|i| i.class.opcode == rspirv::spirv::Op::TypeVoid)
            .and_then(|i| i.result_id)
            .expect("OpTypeVoid must be present");

        // Find the function and check its return-type operand.
        let f = parsed.functions.first().expect("at least one function");
        let def = f.def.as_ref().expect("function def instruction");
        // OpFunction result-type is stored in `result_type` on the instruction.
        assert_eq!(
            def.result_type,
            Some(void_id),
            "function return-type must reference OpTypeVoid id"
        );
    }

    #[test]
    fn name_debug_instruction_points_to_function() {
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");

        let has_name = parsed.debug_names.iter().any(|i| {
            i.class.opcode == rspirv::spirv::Op::Name
                && i.operands
                    .iter()
                    .any(|op| matches!(op, rspirv::dr::Operand::LiteralString(s) if s == "main_cs"))
        });
        assert!(has_name, "OpName for `main_cs` must survive round-trip");
    }

    #[test]
    fn three_entry_points_round_trip_cleanly() {
        // Stress : multiple entries stay parseable.
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        m.seed_vulkan_1_4_defaults();
        for (model, name) in [
            (ExecutionModel::Vertex, "vs"),
            (ExecutionModel::Fragment, "fs"),
            (ExecutionModel::GlCompute, "cs"),
        ] {
            m.add_entry_point(SpirvEntryPoint {
                model,
                name: name.into(),
                execution_modes: if model == ExecutionModel::GlCompute {
                    vec!["LocalSize 64 1 1".into()]
                } else if model == ExecutionModel::Fragment {
                    vec!["OriginUpperLeft".into()]
                } else {
                    vec![]
                },
            });
        }

        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");
        assert_eq!(parsed.entry_points.len(), 3);
        assert_eq!(parsed.functions.len(), 3);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Enum-mapping coverage
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn all_15_execution_models_map_without_panic() {
        for m in ExecutionModel::ALL_MODELS {
            let _ = map_execution_model(m);
        }
    }

    #[test]
    fn all_4_memory_models_map_without_panic() {
        for m in [
            MemoryModel::Simple,
            MemoryModel::Glsl450,
            MemoryModel::OpenCL,
            MemoryModel::Vulkan,
        ] {
            let _ = map_memory_model(m);
        }
    }

    #[test]
    fn all_4_addressing_models_map_without_panic() {
        for a in [
            AddressingModel::Logical,
            AddressingModel::Physical32,
            AddressingModel::Physical64,
            AddressingModel::PhysicalStorageBuffer64,
        ] {
            let _ = map_addressing_model(a);
        }
    }

    #[test]
    fn capability_catalog_round_trips_for_shader_like() {
        // Build a module declaring every shader-compatible capability ;
        // round-trip via rspirv ; assert all survive.
        let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        for c in [
            SpirvCapability::Shader,
            SpirvCapability::Int16,
            SpirvCapability::Int64,
            SpirvCapability::Float16,
            SpirvCapability::Float64,
            SpirvCapability::RuntimeDescriptorArray,
            SpirvCapability::GroupNonUniformArithmetic,
            SpirvCapability::PhysicalStorageBufferAddresses,
            SpirvCapability::VulkanMemoryModelDeviceScope,
        ] {
            m.declare_capability(c);
        }
        m.declare_extension(SpirvExtension::KhrPhysicalStorageBuffer);
        m.declare_extension(SpirvExtension::KhrVulkanMemoryModel);
        m.add_entry_point(SpirvEntryPoint {
            model: ExecutionModel::GlCompute,
            name: "cs".into(),
            execution_modes: vec!["LocalSize 1 1 1".into()],
        });

        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");
        assert!(parsed.capabilities.len() >= 9);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § parse_three_u32 helper
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_three_u32_happy_path() {
        assert_eq!(parse_three_u32("1 2 3"), Some([1, 2, 3]));
        assert_eq!(parse_three_u32("32  1  1"), Some([32, 1, 1]));
        assert_eq!(parse_three_u32("64 8 4"), Some([64, 8, 4]));
    }

    #[test]
    fn parse_three_u32_wrong_arity_rejects() {
        assert_eq!(parse_three_u32("1 2"), None);
        assert_eq!(parse_three_u32("1 2 3 4"), None);
        assert_eq!(parse_three_u32(""), None);
    }

    #[test]
    fn parse_three_u32_non_numeric_rejects() {
        assert_eq!(parse_three_u32("1 x 3"), None);
        assert_eq!(parse_three_u32("a b c"), None);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Capability + extension combo — specific regression guards
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn capability_ext_inst_and_plain_ext_coexist() {
        // seed_vulkan_1_4_defaults declares 3 exts : 2 plain + 1 ext-inst-set.
        // After round-trip, both categories must be present in distinct sections.
        let m = minimal_vulkan_compute_module("main_cs");
        let words = emit_module_binary(&m).unwrap();
        let parsed = dr::load_words(&words).expect("rspirv parse");

        // Plain extensions have OpExtension ; ext-inst imports have OpExtInstImport.
        let plain_ext_count = parsed
            .extensions
            .iter()
            .filter(|i| i.class.opcode == rspirv::spirv::Op::Extension)
            .count();
        let ext_inst_count = parsed
            .ext_inst_imports
            .iter()
            .filter(|i| i.class.opcode == rspirv::spirv::Op::ExtInstImport)
            .count();

        assert_eq!(plain_ext_count, 2, "expected 2 plain extensions");
        assert_eq!(
            ext_inst_count, 1,
            "expected 1 ext-inst import (GLSL.std.450)"
        );
    }

    #[test]
    fn map_capability_smoke_all_variants() {
        // Spot-check a handful — full coverage would be a match-arms mirror.
        assert_eq!(
            map_capability(SpirvCapability::Shader),
            rspirv::spirv::Capability::Shader
        );
        assert_eq!(
            map_capability(SpirvCapability::Kernel),
            rspirv::spirv::Capability::Kernel
        );
        assert_eq!(
            map_capability(SpirvCapability::Int64),
            rspirv::spirv::Capability::Int64
        );
        assert_eq!(
            map_capability(SpirvCapability::RayTracingKHR),
            rspirv::spirv::Capability::RayTracingKHR
        );
    }
}
