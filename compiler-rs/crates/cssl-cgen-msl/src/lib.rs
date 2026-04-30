//! CSSLv3 stage-0 — direct Metal Shading Language source-string emitter.
//!
//! § SPEC : `specs/14_BACKEND.csl` § OWNED MSL EMITTER + `specs/07_CODEGEN.csl`
//!         § GPU BACKEND — MSL path.
//!
//! § ROLE — W-G3 (T11-D269) · LoA-v13 macOS / iOS GPU path.
//!         Emits Metal Shading Language source-strings (Apple Metal 2.4+
//!         compatible) directly from `cssl_mir::MirModule`. Apple's Metal
//!         driver compiles source-strings online at runtime via
//!         `MTLDevice.makeLibrary(source:options:)` ; this crate does NOT
//!         depend on any external transpiler (no spirv-cross, no
//!         metal-shaderconverter — see `cssl-cgen-gpu-msl` for those paths).
//!
//! § DISTINCT FROM `cssl-cgen-gpu-msl`
//!   - `cssl-cgen-gpu-msl` carries the legacy spirv-cross-shim adapter +
//!     a partial body emitter inherited from the SPIR-V → MSL pipeline.
//!   - This crate (`cssl-cgen-msl`) is the canonical Apple-native path :
//!     types ∈ MSL native ; no SPIR-V intermediate ; designed to run in the
//!     LoA-v13 GPU resource pipeline at packaging time and at runtime.
//!
//! § LAYOUT
//!   - `types.rs` — MSL type-system : scalar / vector / matrix / texture /
//!     sampler / address-space / bind attribute.
//!   - `emit.rs` — text emitter : `MslSourceModule` → string.
//!   - `lower.rs` — driver : `MirModule + LowerOptions → MslSourceModule`.
//!
//! § COVERAGE
//!   - Compute (kernel)   : `[[kernel]]` + `[[thread_position_in_grid]]`
//!   - Vertex             : `[[vertex]]` + `[[vertex_id]]` / `[[stage_in]]`
//!   - Fragment           : `[[fragment]]` + `[[position]]` / `[[stage_in]]`
//!   - Resources          : `[[buffer(N)]]` / `[[texture(N)]]` / `[[sampler(N)]]`
//!   - Address-spaces     : `device` / `constant` / `threadgroup` / `thread`
//!
//! § PRIME DIRECTIVE
//!   This crate emits **shader source code only** ; it does not execute
//!   shaders, does not interact with the user, does not collect telemetry.
//!   No PRIME-DIRECTIVE-relevant capabilities are exercised here.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::option_if_let_else)]

pub mod emit;
pub mod lower;
pub mod types;

pub use emit::{MslDecl, MslField, MslParam, MslSourceModule, StageAttr};
pub use lower::{
    lower_module, lower_to_source, BindingLayout, LowerOptions, MslLowerError, ResourceBinding,
};
pub use types::{
    float_to_msl, int_to_msl, mir_type_to_msl, AddressSpace, BindAttr, MslScalar, MslType,
    TextureAccess, TextureKind,
};

/// Crate-version string surfaced for scaffold-verification tests.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
