//! CSSLv3 stage0 — DXIL emitter via DirectXShaderCompiler shim.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — DXIL path + `specs/14_BACKEND.csl`.
//!
//! § STRATEGY
//!   Phase-1 emits HLSL-textual source at stage-0 + wraps the `dxc` (DirectX Shader
//!   Compiler) CLI binary as an optional subprocess to convert HLSL → DXIL. This
//!   mirrors the T6-D1 MLIR-text-CLI fallback + T9-D1 Z3-CLI subprocess pattern —
//!   no `windows-rs` / `windows-sys` FFI is needed at stage-0 for DXIL codegen.
//!
//! § SCOPE (T10-phase-1 / this commit)
//!   - [`ShaderModel`]         — SM 6.0 / 6.1 / 6.2 / 6.3 / 6.4 / 6.5 / 6.6 / 6.7 / 6.8.
//!   - [`ShaderStage`]         — VS / PS / CS / GS / HS / DS / MS / AS / Lib / RayGen /
//!     ClosestHit / AnyHit / Miss / Intersection / Callable.
//!   - [`HlslProfile`]         — combined (stage + shader-model) profile string like `"cs_6_6"`.
//!   - [`DxilTargetProfile`]   — profile + feature-flags + root-signature-version bundle.
//!   - [`HlslModule`]          — skeletal HLSL source builder.
//!   - [`emit_hlsl`]           — `MirModule` → HLSL text.
//!   - [`DxcCliInvoker`]       — `dxc.exe` subprocess adapter (returns error if binary
//!     not on PATH ; stage-0 CI skips the validation-pass unless the DXC binary is installed).
//!   - [`DxilError`]           — error enum.
//!
//! § T10-phase-2 DEFERRED
//!   - Real DXIL binary-emission via `windows-rs` + IDxc* COM interfaces (Windows-only,
//!     pulls MSVC ; deferred per T1-D7).
//!   - Root-signature auto-generation from effect-row + layout attributes.
//!   - Shader-model-6.8 mesh-shader / RT / cooperative-matrix intrinsics emission.
//!   - Signature-compatibility matching vs D3D12 root-signature v1.1+.
//!   - `dxc` invocation with proper `-T`/`-E`/`-HV` arg plumbing + `-spirv` round-trip.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod dxc;
pub mod emit;
pub mod hlsl;
pub mod target;

pub use dxc::{DxcCliInvoker, DxcInvocation, DxcOutcome};
pub use emit::{emit_hlsl, DxilError};
pub use hlsl::{HlslModule, HlslStatement};
pub use target::{DxilTargetProfile, HlslProfile, RootSignatureVersion, ShaderModel, ShaderStage};

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
