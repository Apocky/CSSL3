//! CSSLv3 stage0 — Metal Shading Language emitter.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — MSL path + `specs/14_BACKEND.csl`
//!         § OWNED MSL EMITTER.
//!
//! § STRATEGY
//!   Phase-1 emits MSL source text directly from MIR (stage-0 skeletons only).
//!   The `spirv-cross --msl` legacy path is retained as an optional subprocess
//!   (see [`SpirvCrossInvoker`]) for validation round-trips. `metal-shaderconverter`
//!   is Apple-only and only wired in host-platform builds.
//!
//! § SCOPE (T10-phase-1 / this commit)
//!   - [`MslVersion`]          — MSL 2.0 / 2.1 / 2.2 / 2.3 / 2.4 / 3.0 / 3.1 / 3.2.
//!   - [`MetalStage`]          — vertex / fragment / kernel / object (mesh-task) /
//!     mesh / tile (Metal-3 tile-shaders) / visible-fn (intersection / callable).
//!   - [`MslTargetProfile`]    — version + platform (macOS / iOS) + fast-math + argument-buffer-tier.
//!   - [`MslModule`] / [`MslStatement`] — skeletal MSL source builder.
//!   - [`emit_msl`]            — `MirModule` → MSL text.
//!   - [`SpirvCrossInvoker`]   — `spirv-cross --msl` subprocess adapter.
//!
//! § T10-phase-2 DEFERRED
//!   - Full MIR body → MSL statement lowering.
//!   - Argument-buffer auto-generation from bindings.
//!   - Metal-3 mesh-shader + ray-tracing + cooperative-matrix intrinsics.
//!   - Metal-fn-constants for specialization.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod emit;
pub mod msl;
pub mod spirv_cross;
pub mod target;

pub use emit::{emit_msl, MslError};
pub use msl::{MslModule, MslStatement};
pub use spirv_cross::{SpirvCrossInvocation, SpirvCrossInvoker, SpirvCrossOutcome};
pub use target::{ArgumentBufferTier, MetalPlatform, MetalStage, MslTargetProfile, MslVersion};

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
