//! CSSLv3 — from-scratch DXBC/DXIL container backend (zero external deps).
//!
//! § T11-W18-L8 — `cssl-cgen-dxil` crate · the L8 sibling of L7's
//! `cssl-cgen-spirv`. Emits canonical DXBC containers carrying DXIL bitcode
//! directly from MIR — no `dxc`, no `d3dcompiler.dll`, no HLSL-text-on-the-
//! wire, no rspirv-equivalent third-party.
//!
//! § SPEC : `specs/07_CODEGEN.csl § GPU BACKEND` + `specs/14_BACKEND.csl
//! § OWNED DXIL EMITTER` + Microsoft DXBC container format (publicly
//! reverse-engineered) + LLVM-3.7-bitcode subset (DXIL inner format).
//!
//! § DESIGN
//!   The DXBC container is a chunked binary blob :
//!
//!   - header (8 u32 words = 32 bytes) :
//!       magic       = `"DXBC"` (0x43_42_58_44 little-endian)
//!       hash[0..4]  = 16-byte fingerprint (deterministic from body)
//!       version     = ⟨1, 0⟩ (2 u32 words)
//!       size        = total container bytes (1 u32)
//!       chunk_count = number of chunks (1 u32)
//!   - chunk-offsets : `chunk_count` × u32 (offset from container-start)
//!   - chunks : each chunk = `[fourcc:u32][size:u32][body:u8 × size]`
//!
//!   The canonical L8 substrate-kernel container ships these chunks :
//!
//!   - `SFI0` — shader-feature-info bitfield (SM6.6 + 16-bit + dynamic-resources)
//!   - `ISG1` — input-signature (compute = no inputs · empty signature record)
//!   - `OSG1` — output-signature (compute = no outputs · empty signature record)
//!   - `PSV0` — pipeline-state-validation stub (entry-point-name + stage)
//!   - `DXIL` — DXIL program-header + LLVM-bitcode wrapper-magic
//!
//!   Three shader stages are supported in this slice :
//!     - Compute (`cs_6_6` · DXIL-program-version 1.6 · stage 5)
//!     - Vertex (`vs_6_6` · stage 1)
//!     - Pixel (`ps_6_6` · stage 0)
//!
//! § PROPRIETARY-EVERYTHING THESIS
//!   This crate is the canonical DXIL emit path the LoA-v13 GPU stack
//!   ships when targeting D3D12. The companion `cssl-cgen-gpu-dxil`
//!   crate (T10) wraps the HLSL→dxc fallback ; LoA scenes compiled for
//!   production go through THIS path so that no external crate sits
//!   in the source-of-truth chain.
//!
//! § DETERMINISM
//!   Same `(MirFunc, ShaderTarget)` ⇒ byte-identical container. The hash
//!   field is computed deterministically from the chunk-bytes (FNV-1a
//!   over the body) so two compiles of the same kernel match exactly —
//!   verified by the `lower_is_deterministic` test in this crate.
//!
//! § PRIME-DIRECTIVE
//!   This is renderer-side codegen ; consent + IFC enforcement happens
//!   upstream (cssl-mir `BiometricEgressCheck` + `EnforcesSigmaAtCellTouches`).
//!   Ops reaching `lower::lower_function` are already consent-clean. No
//!   identity claims encoded.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
// Spec-mirroring large match tables.
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::missing_errors_doc)]

pub mod container;
pub mod lower;

pub use container::{
    DxbcChunk, DxbcContainer, DxilProgramHeader, FourCc, DXBC_MAGIC, DXIL_BITCODE_MAGIC,
};
pub use lower::{lower_function, LowerError, ShaderStage, ShaderTarget};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
