//! § T11-W18-L7 — substrate-kernel SPIR-V emit (CSSL-source → SPIR-V binary).
//!
//! § THESIS
//!   The substrate-kernel `Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl`
//!   is the canonical artifact. csslc compiles it through HIR → MIR → this
//!   module, which builds a `MirFunc` of the canonical substrate-kernel shape
//!   (compute · workgroup `(8,8,1)` · entry `"main"` · 3 resource bindings) and
//!   drives the from-scratch `cssl-cgen-spirv::lower_function` emitter to
//!   produce canonical SPIR-V 1.5 words.
//!
//! § PROPRIETARY-EVERYTHING (§ I> spec/14_BACKEND § OWNED SPIR-V EMITTER)
//!   - Source : `.csl` substrate-kernel · authored in CSSL.
//!   - Compiler : `cssl-cgen-spirv` from-scratch SPIR-V binary emitter · zero
//!     external dep (no rspirv on this path · no naga · no WGSL).
//!   - Host : `cssl-host-substrate-render-v3` ash-direct vulkan-1.3 dispatch ·
//!     zero wgpu pipeline-builder.
//!
//! § DETERMINISM
//!   The emitted SPIR-V is byte-exact for a given `(workgroup, entry-name,
//!   binding-shape)` — the lowering driver is deterministic + the type-cache
//!   is order-stable. Two calls with the same `SubstrateKernelSpec` produce
//!   identical word-vectors.

use cssl_cgen_spirv::{lower_function, LowerError, ShaderTarget};
use cssl_mir::func::MirFunc;

/// § Spec for the substrate-kernel emit path. Mirrors the declaration block
/// in `substrate_v2_kernel.csl` § INPUTS.
#[derive(Debug, Clone)]
pub struct SubstrateKernelSpec {
    /// Entry-point name. The `.csl` source declares `entry: "main"` ; the
    /// host `cssl-host-substrate-render-v3` picks up that name when it
    /// constructs the `VkPipelineShaderStageCreateInfo`.
    pub entry_name: String,
    /// Workgroup size. The `.csl` source declares `workgroup: ⟨8, 8, 1⟩`.
    pub workgroup: (u32, u32, u32),
    /// Whether the kernel binds the observer uniform (`set=0, binding=0`).
    /// Always `true` for the substrate-kernel ; exposed as a flag so callers
    /// can construct a stripped-down probe kernel for tests.
    pub has_observer_uniform: bool,
    /// Whether the kernel binds the crystal storage buffer (`set=0, binding=1`).
    pub has_crystals_storage: bool,
    /// Whether the kernel binds the output storage-image (`set=0, binding=2`).
    /// `cssl-cgen-spirv::ShaderTarget` exposes a `sampled_image` flag (the
    /// `UniformConstant` storage class) which we hijack here as the proxy for
    /// "this kernel needs an image binding". The full storage-image lowering
    /// will follow as `cssl-cgen-spirv` grows its image table ; for now the
    /// canonical substrate-kernel is emitted as a compute-only entry-point
    /// + uniform + storage-buffer ; the host attaches the storage-image
    /// descriptor itself per `Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl`
    /// § ASH-DIRECT-DISPATCH.
    pub has_output_storage_image: bool,
}

impl SubstrateKernelSpec {
    /// Canonical spec : matches `substrate_v2_kernel.csl` verbatim.
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            entry_name: "main".to_string(),
            workgroup: (8, 8, 1),
            has_observer_uniform: true,
            has_crystals_storage: true,
            has_output_storage_image: true,
        }
    }
}

/// § Errors thrown when emitting the substrate-kernel SPIR-V.
#[derive(Debug, thiserror::Error)]
pub enum SubstrateKernelEmitError {
    /// The from-scratch SPIR-V backend rejected the lowering.
    #[error("substrate-kernel lowering failed : {0}")]
    Lower(#[from] LowerError),
}

/// § Emit canonical substrate-kernel SPIR-V words.
///
/// This advances `cssl-cgen-gpu-spirv` to be the orchestrator that drives the
/// from-scratch `cssl-cgen-spirv` backend along the substrate-kernel-shape
/// declared in `substrate_v2_kernel.csl`. The output is a `Vec<u32>` of
/// canonical SPIR-V 1.5 words ready to feed `vkCreateShaderModule` directly
/// (no naga · no WGSL · no wgpu in the chain).
///
/// The MIR fn body is empty in this slice — the substrate-kernel-shape
/// (capabilities + types + entry-point + interface + bindings) is what the
/// host renderer cares about for L7's "csslc emits SPIR-V binary directly"
/// directive. Body-op coverage is iterated post-L7 as the kernel logic moves
/// out of the WGSL legacy and into MIR-emitted SPIR-V.
///
/// § ERRORS
///   Returns [`SubstrateKernelEmitError::Lower`] if the from-scratch SPIR-V
///   backend rejects the lowering (e.g. entry-name mismatch).
pub fn emit_substrate_kernel_spirv(
    spec: &SubstrateKernelSpec,
) -> Result<Vec<u32>, SubstrateKernelEmitError> {
    // 1. Build a substrate-kernel-shaped MirFunc. The fn signature is
    //    `() -> ()` ; resource bindings are declared via the ShaderTarget
    //    flags (cssl-cgen-spirv synthesizes the OpVariable + OpDecorate
    //    instructions from those flags).
    let func = MirFunc::new(spec.entry_name.clone(), Vec::new(), Vec::new());

    // 2. Build the per-stage target descriptor.
    let target = ShaderTarget {
        stage: cssl_cgen_spirv::ShaderStage::Compute,
        local_size: spec.workgroup,
        entry_name: spec.entry_name.clone(),
        uniform_buffer: spec.has_observer_uniform,
        push_constant: false,
        sampled_image: spec.has_output_storage_image,
        storage_buffer: spec.has_crystals_storage,
    };

    // 3. Drive the from-scratch SPIR-V backend.
    let bin = lower_function(&func, &target)?;

    // 4. Finalize header + instructions into a single u32 stream ready for
    //    `vkCreateShaderModule(pCode = ptr, codeSize = bytes_len)`.
    Ok(bin.finalize())
}

/// § Emit canonical substrate-kernel SPIR-V as a little-endian byte vector.
///
/// Same content as [`emit_substrate_kernel_spirv`] but already serialized to
/// bytes. Useful for hashing / on-disk caching / `vkCreateShaderModule`-via-
/// `pCode = bytes.as_ptr() as *const u32`.
pub fn emit_substrate_kernel_spirv_bytes(
    spec: &SubstrateKernelSpec,
) -> Result<Vec<u8>, SubstrateKernelEmitError> {
    let words = emit_substrate_kernel_spirv(spec)?;
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    Ok(out)
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests — substrate-kernel emit integrity.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// SPIR-V magic number from Khronos § 2.3.
    const SPIRV_MAGIC: u32 = 0x0723_0203;

    #[test]
    fn canonical_spec_emits_nonempty_spirv() {
        let spec = SubstrateKernelSpec::canonical();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        assert!(words.len() > 5, "must emit header (5 words) + instructions");
        assert_eq!(
            words[0], SPIRV_MAGIC,
            "first word must be SPIR-V magic 0x07230203",
        );
    }

    #[test]
    fn emitted_bytes_are_4_aligned() {
        let spec = SubstrateKernelSpec::canonical();
        let bytes = emit_substrate_kernel_spirv_bytes(&spec).expect("byte emit must succeed");
        assert!(bytes.len() >= 20, "header alone is 5 × 4 = 20 bytes");
        assert_eq!(
            bytes.len() % 4,
            0,
            "SPIR-V is u32-stream ; byte-len must be 4-aligned",
        );
        // Bytes 0..4 little-endian = SPIRV_MAGIC.
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        assert_eq!(magic, SPIRV_MAGIC);
    }

    #[test]
    fn emit_is_deterministic() {
        let spec = SubstrateKernelSpec::canonical();
        let a = emit_substrate_kernel_spirv(&spec).unwrap();
        let b = emit_substrate_kernel_spirv(&spec).unwrap();
        assert_eq!(
            a, b,
            "same spec must produce byte-identical SPIR-V across calls",
        );
    }

    #[test]
    fn workgroup_change_changes_emit() {
        let canonical = SubstrateKernelSpec::canonical();
        let mut alt = canonical.clone();
        alt.workgroup = (16, 16, 1);
        let a = emit_substrate_kernel_spirv(&canonical).unwrap();
        let c = emit_substrate_kernel_spirv(&alt).unwrap();
        assert_ne!(a, c, "different workgroup → different LocalSize words");
    }
}
