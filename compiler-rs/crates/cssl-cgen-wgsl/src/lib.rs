//! CSSLv3 stage0 — WebGPU Shading Language (WGSL) source-string emitter.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — WGSL path +
//!         `specs/14_BACKEND.csl` § OWNED WGSL EMITTER.
//!
//! § T11-D270 (W-G4) · CSL-MANDATE · LoA-v13 GPU + web
//!
//! § POSITIONING vs. cssl-cgen-gpu-wgsl
//!
//!   This crate is the *thin* WGSL emitter on the LoA-v13 web-deployment
//!   path : the browser's WebGPU implementation accepts WGSL source-text
//!   and compiles it itself, so the host-side compiler only needs to emit
//!   well-formed WGSL strings — no FFI, no naga round-trip, no extra
//!   transitive deps. The sibling `cssl-cgen-gpu-wgsl` crate carries the
//!   heavy validator harness and is the desktop-host path.
//!
//!   The split is by *deployment-surface* :
//!
//!   ┌──────────────────────────┬────────────────────────────────────────┐
//!   │ cssl-cgen-gpu-wgsl       │ desktop wgpu ; naga round-trip ;       │
//!   │   (heavy / D32 / D75)    │ structured-CFG-marker contract ;       │
//!   │                          │ winapi-util pin (D154).                │
//!   ├──────────────────────────┼────────────────────────────────────────┤
//!   │ cssl-cgen-wgsl  ← THIS   │ browser/WASM ; minimal deps ; just     │
//!   │   (thin / D270)          │ the source-string + entry-point        │
//!   │                          │ shape (compute / vertex / fragment).   │
//!   └──────────────────────────┴────────────────────────────────────────┘
//!
//! § SCOPE
//!   - [`emit`]   — high-level driver `emit_wgsl_source` : `MirModule` → WGSL text.
//!   - [`lower`]  — per-fn `MIR-shader-fn → WGSL source string` lowering.
//!   - [`types`]  — WGSL type-system (i32 / u32 / f32 / bool / vec / mat /
//!                  array / texture / sampler) + `MirType` ↔ `WgslType` mapping.
//!   - [`shader`] — entry-point shape, builtin-vars, binding-decorators.
//!
//! § ENTRY POINTS
//!   - `@compute @workgroup_size(X, Y, Z)` (default 64,1,1)
//!   - `@vertex`  — outputs `@builtin(position) vec4<f32>`
//!   - `@fragment` — outputs `@location(0) vec4<f32>`
//!
//! § BINDINGS
//!   `@group(N) @binding(M) var<...> name : type;` — group-index follows
//!   MirFunc parameter order (group 0) ; binding-index is the parameter
//!   index. Storage-buffer / uniform-buffer / sampler / texture variants
//!   distinguished by `MirType`.
//!
//! § BUILTINS
//!   `@builtin(position)`, `@builtin(global_invocation_id)`,
//!   `@builtin(local_invocation_id)`, `@builtin(workgroup_id)`,
//!   `@builtin(vertex_index)`, `@builtin(instance_index)`,
//!   `@builtin(num_workgroups)` are available via [`shader::Builtin`].
//!
//! § DEFERRED
//!   - Full body-emission (per-MirOp lowering table) — handled by the
//!     heavy sibling crate ; this crate emits an explicit `// body : <op-count> ops`
//!     comment + a stub return for `@vertex` / `@fragment` so the emitted
//!     source is grammatically valid even with empty bodies.
//!   - Subgroup-op extension (chrome flag).
//!   - Ray-query extension (WebGPU v2).
//!   - i64 / f64 (narrowed to i32 / f32 at stage-0 per §§ 14_BACKEND).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod emit;
pub mod lower;
pub mod shader;
pub mod types;

pub use emit::{emit_wgsl_source, EmitConfig, EmitError};
pub use lower::{lower_fn, FnLowerError};
pub use shader::{Binding, BindingKind, Builtin, EntryPointKind, ShaderHeader};
pub use types::{wgsl_type_for, WgslType, WgslTypeError};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// Default workgroup size for compute entry-points (64,1,1) — matches
/// WebGPU's "good-enough-for-most-pipelines" baseline.
pub const DEFAULT_COMPUTE_WG: (u32, u32, u32) = (64, 1, 1);

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
