//! CSSLv3 stage0 — WebGPU Shading Language (WGSL) emitter.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — WGSL path + `specs/14_BACKEND.csl`
//!         § OWNED WGSL EMITTER.
//!
//! § STRATEGY
//!   Phase-1 emits WGSL source text directly — WGSL is a textual shading language,
//!   so no intermediate binary form + no `tint` FFI needed. Phase-2 adds a `naga`-based
//!   round-trip validator for CI regression-detection.
//!
//! § SCOPE (T10-phase-1 / this commit)
//!   - [`WebGpuStage`]         — vertex / fragment / compute.
//!   - [`WgslLimits`]          — WebGPU min/max limits relevant to codegen
//!     (`max_bind_groups` / `max_workgroup_size` / `max_storage_buffers_per_stage`).
//!   - [`WgslTargetProfile`]   — stage + limits + feature-set bundle.
//!   - [`WgslModule`] / [`WgslStatement`] — skeletal WGSL source builder.
//!   - [`emit_wgsl`]           — `MirModule` → WGSL text.
//!
//! § T10-phase-2 DEFERRED
//!   - Full MIR body → WGSL statement lowering.
//!   - `naga` round-trip validation subprocess (pure-Rust but pulls many deps).
//!   - Ray-query extension emission (WebGPU v2 — experimental ; spec-current does not expose).
//!   - Subgroup-op extension emission (chrome flag).
//!   - `@must_use` / `@group` / `@binding` auto-generation from effect-row.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod emit;
pub mod target;
pub mod wgsl;

pub use emit::{emit_wgsl, WgslError};
pub use target::{WebGpuFeature, WebGpuStage, WgslLimits, WgslTargetProfile};
pub use wgsl::{WgslModule, WgslStatement};

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
