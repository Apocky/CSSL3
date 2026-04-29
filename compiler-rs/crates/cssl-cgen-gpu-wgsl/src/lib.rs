//! CSSLv3 stage0 — WebGPU Shading Language (WGSL) emitter.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — WGSL path + `specs/14_BACKEND.csl`
//!         § OWNED WGSL EMITTER.
//!
//! § STRATEGY
//!   Emits WGSL source text directly — WGSL is a textual shading language,
//!   so no intermediate binary form + no `tint` FFI needed. The naga
//!   round-trip validator (T11-D32) parses every emitted shader through
//!   `naga::front::wgsl::parse_str` to catch grammar regressions
//!   immediately at test-time.
//!
//! § SCOPE
//!   - [`WebGpuStage`]         — vertex / fragment / compute.
//!   - [`WgslLimits`]          — WebGPU min/max limits relevant to codegen
//!     (`max_bind_groups` / `max_workgroup_size` / `max_storage_buffers_per_stage`).
//!   - [`WgslTargetProfile`]   — stage + limits + feature-set bundle.
//!   - [`WgslModule`] / [`WgslStatement`] — WGSL source builder.
//!   - [`emit_wgsl`]           — `MirModule` → WGSL text.
//!   - [`body_emit::lower_fn_body`] (T11-D75 / S6-D4) — per-MIR-op WGSL
//!     body emission table covering arith / cmp / scf-if / scf-loop /
//!     scf-while / memref / bitcast + reject-list (heap, closures, cf.br).
//!
//! § FANOUT CONTRACT (T11-D75 / S6-D4)
//!   The structured-CFG validator pass (T11-D70 / S6-D5) MUST run before
//!   WGSL emission — the marker `("structured_cfg.validated", "true")` on
//!   the `MirModule` is the fanout contract between D5 and D1..D4. The
//!   emitter rejects unmarked modules with [`WgslError::StructuredCfgMarkerMissing`].
//!   See the `body_emit` module docs for the full op-mapping table.
//!
//! § DEFERRED
//!   - Ray-query extension emission (WebGPU v2 — experimental).
//!   - Subgroup-op extension emission (chrome flag).
//!   - `@must_use` / `@group` / `@binding` auto-generation from effect-row.
//!   - Heap allocation (REJECTED at body-emit per slice handoff).
//!   - Closures (REJECTED at body-emit per slice handoff).
//!   - i64 / f64 native types (narrowed to i32 / f32 at stage-0).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod body_emit;
pub mod emit;
pub mod target;
pub mod wgsl;

pub use body_emit::{lower_fn_body, BodyEmitError};
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
