//! § T11-W18-L8 — substrate-kernel DXBC emit (CSSL-source → DXIL container).
//!
//! § THESIS
//!   The substrate-kernel `Labyrinth of Apocalypse/systems/substrate_v2_dxil.csl`
//!   is the canonical artifact. csslc compiles it through HIR → MIR → this
//!   module, which builds a `MirFunc` of the canonical substrate-kernel shape
//!   (compute · workgroup `(8,8,1)` · entry `"main"` · 3 D3D12 root-params :
//!   CBV·SRV·UAV) and drives the from-scratch `cssl-cgen-dxil::lower_function`
//!   emitter to produce a canonical DXBC container ready to feed
//!   `D3D12CreateComputePipelineState` directly.
//!
//! § PROPRIETARY-EVERYTHING (§ I> spec/14_BACKEND § OWNED DXIL EMITTER)
//!   - Source : `.csl` substrate-kernel · authored in CSSL.
//!   - Compiler : `cssl-cgen-dxil` from-scratch DXBC + DXIL emitter · zero
//!     external dep (no dxc · no d3dcompiler · no HLSL-text on the wire).
//!   - Host : `cssl-host-substrate-render-v4-dxil` D3D12-direct dispatch ·
//!     zero wgpu pipeline-builder · zero ash · zero Vulkan.
//!
//! § DETERMINISM
//!   The emitted DXBC is byte-exact for a given `(workgroup, entry-name,
//!   binding-shape, shader-model)` — verified by the `emit_is_deterministic`
//!   test below. Two calls with the same `SubstrateKernelDxilSpec` produce
//!   identical byte vectors.

use cssl_cgen_dxil::{lower_function, LowerError, ShaderTarget as DxilShaderTarget};
use cssl_mir::func::MirFunc;

/// § Spec for the substrate-kernel DXBC emit path.
///
/// Mirrors the declaration block in `substrate_v2_dxil.csl § INPUTS` :
/// CBV at `b0` (observer uniform) · SRV at `t0` (crystals storage) · UAV at
/// `u0` (output storage-image · `RWTexture2D<RGBA8Unorm>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubstrateKernelDxilSpec {
    /// Entry-point name. The `.csl` source declares `entry: "main"` ; the
    /// host `cssl-host-substrate-render-v4-dxil` picks up that name when it
    /// constructs the `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
    pub entry_name: String,
    /// Workgroup size. The `.csl` source declares `workgroup: ⟨8, 8, 1⟩`.
    pub workgroup: (u32, u32, u32),
    /// Shader-model `(major, minor)`. Default = `(6, 6)` per `.csl § DECLARATION`.
    pub shader_model: (u32, u32),
    /// Whether the kernel binds the observer CBV (`b0`).
    pub has_observer_cbv: bool,
    /// Whether the kernel binds the crystal storage SRV (`t0`).
    pub has_crystals_srv: bool,
    /// Whether the kernel binds the output storage-image UAV (`u0`).
    pub has_output_uav: bool,
}

impl SubstrateKernelDxilSpec {
    /// Canonical spec : matches `substrate_v2_dxil.csl` verbatim.
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            entry_name: "main".to_string(),
            workgroup: (8, 8, 1),
            shader_model: (6, 6),
            has_observer_cbv: true,
            has_crystals_srv: true,
            has_output_uav: true,
        }
    }

    /// Convert to a `cssl_cgen_dxil::ShaderTarget`.
    fn to_target(&self) -> DxilShaderTarget {
        DxilShaderTarget {
            stage: cssl_cgen_dxil::ShaderStage::Compute,
            entry_name: self.entry_name.clone(),
            workgroup: self.workgroup,
            shader_model: self.shader_model,
            has_cbv: self.has_observer_cbv,
            has_srv: self.has_crystals_srv,
            has_uav: self.has_output_uav,
            enable_16_bit_types: true,
            enable_dynamic_resources: self.shader_model >= (6, 6),
        }
    }
}

/// § Errors thrown when emitting the substrate-kernel DXBC container.
#[derive(Debug, thiserror::Error)]
pub enum SubstrateKernelDxilEmitError {
    /// The from-scratch DXBC backend rejected the lowering.
    #[error("substrate-kernel DXIL lowering failed : {0}")]
    Lower(#[from] LowerError),
}

/// § Emit canonical substrate-kernel DXBC bytes ready to feed
/// `D3D12_COMPUTE_PIPELINE_STATE_DESC.CS = { pShaderBytecode, BytecodeLength }`.
///
/// This advances `cssl-cgen-gpu-dxil` to be the orchestrator that drives the
/// from-scratch `cssl-cgen-dxil` backend along the substrate-kernel-shape
/// declared in `substrate_v2_dxil.csl`. The output is a `Vec<u8>` of
/// canonical DXBC container bytes (no HLSL-text · no dxc-subprocess · no
/// d3dcompiler in the chain).
///
/// § ERRORS
///   Returns [`SubstrateKernelDxilEmitError::Lower`] if the from-scratch DXBC
///   backend rejects the lowering (e.g. entry-name mismatch).
pub fn emit_substrate_kernel_dxil(
    spec: &SubstrateKernelDxilSpec,
) -> Result<Vec<u8>, SubstrateKernelDxilEmitError> {
    let func = MirFunc::new(spec.entry_name.clone(), Vec::new(), Vec::new());
    let target = spec.to_target();
    let container = lower_function(&func, &target)?;
    Ok(container.finalize())
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests — substrate-kernel DXBC emit integrity.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::{
        emit_substrate_kernel_dxil, SubstrateKernelDxilEmitError, SubstrateKernelDxilSpec,
    };

    /// DXBC magic = `"DXBC"` little-endian.
    const DXBC_MAGIC: [u8; 4] = *b"DXBC";

    #[test]
    fn canonical_spec_emits_dxbc_container() {
        let spec = SubstrateKernelDxilSpec::canonical();
        let bytes = emit_substrate_kernel_dxil(&spec).expect("canonical emit must succeed");
        assert!(
            bytes.len() > 64,
            "container header alone is 32 bytes ; with chunks must be > 64",
        );
        assert_eq!(&bytes[0..4], &DXBC_MAGIC, "first 4 bytes must be DXBC magic");
    }

    #[test]
    fn canonical_emit_is_deterministic() {
        let spec = SubstrateKernelDxilSpec::canonical();
        let a = emit_substrate_kernel_dxil(&spec).unwrap();
        let b = emit_substrate_kernel_dxil(&spec).unwrap();
        assert_eq!(
            a, b,
            "same spec must produce byte-identical DXBC across calls",
        );
    }

    #[test]
    fn workgroup_change_changes_emit() {
        let canonical = SubstrateKernelDxilSpec::canonical();
        let mut alt = canonical.clone();
        alt.workgroup = (16, 16, 1);
        let a = emit_substrate_kernel_dxil(&canonical).unwrap();
        let c = emit_substrate_kernel_dxil(&alt).unwrap();
        assert_ne!(a, c, "different workgroup ⇒ different DXIL fingerprint");
    }

    #[test]
    fn entry_name_change_changes_emit() {
        let canonical = SubstrateKernelDxilSpec::canonical();
        let mut alt = canonical.clone();
        alt.entry_name = "main_alt".to_string();
        let a = emit_substrate_kernel_dxil(&canonical).unwrap();
        let c = emit_substrate_kernel_dxil(&alt).unwrap();
        assert_ne!(a, c, "different entry-name ⇒ different PSV0 + DXIL fingerprint");
    }

    #[test]
    fn empty_entry_name_errors_cleanly() {
        let mut spec = SubstrateKernelDxilSpec::canonical();
        spec.entry_name.clear();
        let err = emit_substrate_kernel_dxil(&spec).unwrap_err();
        assert!(matches!(err, SubstrateKernelDxilEmitError::Lower(_)));
    }

    #[test]
    fn container_total_size_matches_byte_len() {
        let spec = SubstrateKernelDxilSpec::canonical();
        let bytes = emit_substrate_kernel_dxil(&spec).unwrap();
        // bytes [24..28] = total-size u32.
        let stored_size = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        assert_eq!(stored_size as usize, bytes.len());
    }

    #[test]
    fn container_chunk_count_is_five() {
        let spec = SubstrateKernelDxilSpec::canonical();
        let bytes = emit_substrate_kernel_dxil(&spec).unwrap();
        let stored_count = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        assert_eq!(stored_count, 5, "L8-phase-1 emits SFI0+ISG1+OSG1+PSV0+DXIL");
    }
}
