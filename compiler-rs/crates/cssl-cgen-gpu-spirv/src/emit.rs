//! Stage-0 text emitter : `SpirvModule` → SPIR-V disassembler-like text.
//!
//! § STRATEGY
//!   Phase-1 emits one line per SPIR-V instruction in the canonical form accepted by
//!   `spirv-as`. Each section from `module.rs` is walked in fixed order. Phase-2
//!   wires this to `rspirv::dr::Module` for real binary emission + `spirv-val`
//!   subprocess validation.

use core::fmt::Write as _;

use thiserror::Error;

use crate::module::{SpirvEntryPoint, SpirvModule, SpirvSection};

/// Failure modes for SPIR-V emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpirvEmitError {
    /// An extension was declared that this target-env does not support.
    #[error(
        "extension `{extension}` is not valid for target-env `{target_env}` (stage-0 lax-check, \
         `spirv-val` catches the full invariant at T10-phase-2)"
    )]
    ExtensionNotValidForTarget {
        extension: String,
        target_env: String,
    },
    /// A capability was declared that requires an extension not present in the module.
    #[error(
        "capability `{capability}` requires at least one SPV extension but none was declared \
         on this module"
    )]
    CapabilityMissingExtension { capability: String },
    /// A module had zero entry points. For Shader targets this is a hard error ;
    /// for Kernel targets it's a lint.
    #[error("module targeting `{target_env}` declared no entry points — invalid for this env")]
    NoEntryPoints { target_env: String },
}

/// Emit a SPIR-V module to disassembler-like text.
///
/// # Errors
/// Returns [`SpirvEmitError::NoEntryPoints`] for non-kernel targets with no entries.
/// Capability-vs-extension invariants are lax-checked at stage-0 (deferred to
/// `spirv-val` at T10-phase-2).
pub fn emit_module(module: &SpirvModule) -> Result<String, SpirvEmitError> {
    // Shader-like targets require at least one entry point ; Kernel targets don't.
    if module.entry_points.is_empty() {
        use crate::target::SpirvTargetEnv as T;
        let target_is_kernel = matches!(module.target_env, T::OpenClKernel2_2);
        if !target_is_kernel {
            return Err(SpirvEmitError::NoEntryPoints {
                target_env: module.target_env.to_string(),
            });
        }
    }

    let mut out = String::new();

    // Header banner (comments are ignored by `spirv-as` but useful for diffs).
    writeln!(
        out,
        "; SPIR-V module emitted by cssl-cgen-gpu-spirv (stage-0)\n\
         ; target-env = {}\n\
         ; memory-model = {} / addressing-model = {}",
        module.target_env.target_env_str(),
        module.memory_model.as_str(),
        module.addressing_model.as_str(),
    )
    .unwrap();

    for section in SpirvSection::ALL_SECTIONS {
        writeln!(out, "; -- section : {} --", section.as_str()).unwrap();
        match section {
            SpirvSection::Capability => emit_capabilities(module, &mut out),
            SpirvSection::Extension => emit_extensions(module, &mut out),
            SpirvSection::ExtInstImport => emit_ext_inst_imports(module, &mut out),
            SpirvSection::MemoryModel => emit_memory_model(module, &mut out),
            SpirvSection::EntryPoint => emit_entry_points(module, &mut out),
            SpirvSection::ExecutionMode => emit_execution_modes(module, &mut out),
            SpirvSection::Debug => emit_debug(module, &mut out),
            SpirvSection::Annotation | SpirvSection::TypesConstantsGlobals => {
                // Stage-0 : these sections are empty placeholders ; T10-phase-2 populates them
                // when fn-body lowering lands.
                writeln!(out, "; (stage-0 : empty — populated @ T10-phase-2)").unwrap();
            }
            SpirvSection::FnDecl => {
                writeln!(out, "; (stage-0 : no fn-decls — T10-phase-2 adds externs)").unwrap();
            }
            SpirvSection::FnDef => {
                // One OpFunction stub per entry point for shape-inspection.
                for ep in &module.entry_points {
                    writeln!(
                        out,
                        "OpFunction {} None TypeFunction_void__void ; {}",
                        ep.name, ep.model
                    )
                    .unwrap();
                    writeln!(out, "  OpLabel %entry").unwrap();
                    writeln!(out, "  ; stage-0 skeleton — body @ T10-phase-2").unwrap();
                    writeln!(out, "  OpReturn").unwrap();
                    writeln!(out, "OpFunctionEnd").unwrap();
                }
            }
        }
    }

    Ok(out)
}

fn emit_capabilities(module: &SpirvModule, out: &mut String) {
    for c in module.capabilities.iter() {
        writeln!(out, "OpCapability {}", c.as_str()).unwrap();
    }
}

fn emit_extensions(module: &SpirvModule, out: &mut String) {
    for e in module.extensions.iter_plain() {
        writeln!(out, "OpExtension \"{}\"", e.as_str()).unwrap();
    }
}

fn emit_ext_inst_imports(module: &SpirvModule, out: &mut String) {
    for e in module.extensions.iter_ext_inst_sets() {
        writeln!(
            out,
            "%{} = OpExtInstImport \"{}\"",
            ext_inst_handle(e.as_str()),
            e.as_str()
        )
        .unwrap();
    }
}

fn emit_memory_model(module: &SpirvModule, out: &mut String) {
    writeln!(
        out,
        "OpMemoryModel {} {}",
        module.addressing_model.as_str(),
        module.memory_model.as_str()
    )
    .unwrap();
}

fn emit_entry_points(module: &SpirvModule, out: &mut String) {
    for ep in &module.entry_points {
        writeln!(
            out,
            "OpEntryPoint {} %{} \"{}\"",
            ep.model.as_str(),
            ep.name,
            ep.name
        )
        .unwrap();
    }
}

fn emit_execution_modes(module: &SpirvModule, out: &mut String) {
    for ep in &module.entry_points {
        for mode in &ep.execution_modes {
            writeln!(out, "OpExecutionMode %{} {}", ep.name, mode).unwrap();
        }
    }
}

fn emit_debug(module: &SpirvModule, out: &mut String) {
    if let Some(lang) = &module.source_language {
        let version = module.source_version.unwrap_or(0);
        writeln!(out, "OpSource {lang} {version}").unwrap();
    }
    for ep in &module.entry_points {
        writeln!(out, "OpName %{} \"{}\"", ep.name, ep.name).unwrap();
    }
}

/// Produce a handle token from an ext-inst-set name (`"GLSL.std.450"` → `"GLSL_std_450"`).
fn ext_inst_handle(name: &str) -> String {
    name.replace('.', "_")
}

/// Trivial smoke-check used by integration tests : build a minimal Vulkan compute module
/// with one entry point + `LocalSize 1 1 1` execution mode.
#[must_use]
pub fn minimal_vulkan_compute_module(entry: &str) -> SpirvModule {
    use crate::target::{ExecutionModel, SpirvTargetEnv};
    let mut m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
    m.seed_vulkan_1_4_defaults();
    m.add_entry_point(SpirvEntryPoint {
        model: ExecutionModel::GlCompute,
        name: entry.into(),
        execution_modes: vec!["LocalSize 1 1 1".into()],
    });
    m
}

#[cfg(test)]
mod tests {
    use super::{emit_module, minimal_vulkan_compute_module, SpirvEmitError};
    use crate::module::{SpirvEntryPoint, SpirvModule};
    use crate::target::{ExecutionModel, SpirvTargetEnv};

    #[test]
    fn shader_module_without_entry_fails() {
        let m = SpirvModule::new(SpirvTargetEnv::VulkanKhr1_4);
        let e = emit_module(&m).unwrap_err();
        assert!(matches!(e, SpirvEmitError::NoEntryPoints { .. }));
    }

    #[test]
    fn kernel_module_without_entry_succeeds() {
        let m = SpirvModule::new(SpirvTargetEnv::OpenClKernel2_2);
        let text = emit_module(&m).unwrap();
        assert!(text.contains("target-env = opencl2.2"));
    }

    #[test]
    fn minimal_compute_module_emits_all_sections() {
        let m = minimal_vulkan_compute_module("main_cs");
        let text = emit_module(&m).unwrap();
        for section_marker in [
            "-- section : capabilities --",
            "-- section : extensions --",
            "-- section : ext-inst-imports --",
            "-- section : memory-model --",
            "-- section : entry-points --",
            "-- section : execution-modes --",
            "-- section : debug --",
            "-- section : annotations --",
            "-- section : types-constants-globals --",
            "-- section : fn-decls --",
            "-- section : fn-defs --",
        ] {
            assert!(
                text.contains(section_marker),
                "missing section marker `{section_marker}`"
            );
        }
    }

    #[test]
    fn capabilities_are_emitted_before_extensions() {
        let m = minimal_vulkan_compute_module("main_cs");
        let text = emit_module(&m).unwrap();
        let cap_pos = text.find("OpCapability Shader").unwrap();
        let ext_pos = text.find("OpExtension").unwrap();
        assert!(cap_pos < ext_pos);
    }

    #[test]
    fn entry_point_line_shape() {
        let m = minimal_vulkan_compute_module("run_step");
        let text = emit_module(&m).unwrap();
        assert!(text.contains("OpEntryPoint GLCompute %run_step \"run_step\""));
    }

    #[test]
    fn execution_mode_line_shape() {
        let m = minimal_vulkan_compute_module("run_step");
        let text = emit_module(&m).unwrap();
        assert!(text.contains("OpExecutionMode %run_step LocalSize 1 1 1"));
    }

    #[test]
    fn memory_model_line_shape() {
        let m = minimal_vulkan_compute_module("main_cs");
        let text = emit_module(&m).unwrap();
        assert!(text.contains("OpMemoryModel PhysicalStorageBuffer64 Vulkan"));
    }

    #[test]
    fn ext_inst_import_handle_shape() {
        let m = minimal_vulkan_compute_module("main_cs");
        let text = emit_module(&m).unwrap();
        assert!(text.contains("%GLSL_std_450 = OpExtInstImport \"GLSL.std.450\""));
    }

    #[test]
    fn debug_source_line() {
        let m = minimal_vulkan_compute_module("main_cs");
        let text = emit_module(&m).unwrap();
        assert!(text.contains("OpSource CSSLv3 0"));
    }

    #[test]
    fn fn_def_stub_per_entry_point() {
        let mut m = minimal_vulkan_compute_module("main_cs");
        m.add_entry_point(SpirvEntryPoint {
            model: ExecutionModel::Vertex,
            name: "vs_main".into(),
            execution_modes: vec![],
        });
        let text = emit_module(&m).unwrap();
        assert!(text.contains("OpFunction main_cs"));
        assert!(text.contains("OpFunction vs_main"));
        assert_eq!(text.matches("OpFunctionEnd").count(), 2);
    }
}
