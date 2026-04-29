//! § cssl-render — renderer foundation : SceneGraph + PBR Material + RenderGraph + per-host backend abstraction
//! ════════════════════════════════════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The renderer-foundation slice (R1, T11-D105). Provides the
//!   substrate-canonical scene-graph + material model + render-graph + draw
//!   submission pipeline. Sits between the per-host backend crates
//!   (`cssl-host-vulkan`, `cssl-host-d3d12`, `cssl-host-metal`,
//!   `cssl-host-webgpu`) and the consumer-side scene-authoring layer.
//!
//! § SPEC ANCHOR
//!   - `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS — the per-host adapter
//!     surface that this crate's backend-trait abstracts over.
//!   - `specs/30_SUBSTRATE.csl` § PROJECTIONS — the camera + frustum +
//!     LoD pipeline that the renderer integrates via the
//!     [`projections`] re-export.
//!
//! § SURFACE SUMMARY
//!   - **Scene graph** ([`scene::SceneGraph`], [`scene::SceneNode`],
//!     [`scene::NodeId`]) — flat-arena tree with depth-first transform
//!     propagation. Multiple roots support disjoint world segments.
//!   - **Mesh + materials** ([`mesh::Mesh`], [`material::Material`],
//!     [`material::MaterialModel`]) — vertex/index buffers + PBR /
//!     Lambert / Phong shading models. Asset-database-friendly handle
//!     references (renderer never owns GPU bytes).
//!   - **Lights** ([`light::Light`]) — Directional / Point / Spot / Area
//!     primitives with photometric units + shadow flags.
//!   - **Render graph** ([`graph::RenderGraph`], [`graph::RenderPass`],
//!     [`graph::PassKind`]) — declarative pass DAG with topo-sort. Ships a
//!     substrate-canonical default forward pipeline.
//!   - **Render queue** ([`queue::RenderQueue`], [`queue::DrawCall`]) —
//!     per-camera draw list with frustum culling + opaque/translucent
//!     bucketing + sort.
//!   - **Backend trait** ([`backend::RenderBackend`], [`backend::NullBackend`])
//!     — abstraction the per-host crates implement. Backend-agnostic
//!     surface : no Vulkan / D3D12 / Metal types in public API.
//!   - **Submit entry-point** ([`submit::submit`], [`submit::RenderContext`],
//!     [`submit::RenderEffect`]) — per-frame submission driver. The
//!     `{Render}` effect-row marker witnesses render-fn participation.
//!
//! § HANDEDNESS + DEPTH CONVENTIONS
//!   - Right-handed, Y-up. View-space forward is `-Z`.
//!   - Reverse-Z perspective by default. Pair with depth-cleared-to-0 +
//!     `GREATER` depth-test on the host backend.
//!   - NDC-Z range `[0, 1]` (Vulkan / D3D12 / WebGPU canonical).
//!   - Column-major Mat4 storage matches GLSL upload buffers.
//!   These match `cssl-substrate-projections` (H3) so the renderer +
//!   projections + math triangle is convention-consistent.
//!
//! § LANDMINES (MATH + ASSET in-flight)
//!   - **MATH (M1)** : `cssl-math` is sibling-in-flight. `crate::math`
//!     provides the canonical surface stubs the renderer needs locally
//!     until M1 lands. Conversion helpers (`Vec3::to_proj` / `from_proj`)
//!     bridge to the projections crate's embedded math types.
//!   - **ASSET (N1)** : same pattern via `crate::asset`. Texture / Sampler
//!     placeholder types + opaque `AssetHandle<T>` typed handles.
//!   When MATH + ASSET land, `crate::math` + `crate::asset` shrink to
//!   re-export wrappers. Consumer API unchanged.
//!
//! § PRIME-DIRECTIVE
//!   - The renderer stores world data ; nothing observer-specific. Per-
//!     camera frustum culling lives in [`queue`] so multiple observers
//!     coexist for split-screen / debug / mini-map. This matches the
//!     substrate-projections H3 ObserverFrame design.
//!   - Shadow + visibility data are scene-graph-local — no telemetry
//!     escape-paths.
//!
//! § STAGE-0 LIMITATIONS / DEFERRED
//!   - **Shader bytecode** : per-backend hand-written test shaders only.
//!     CSSLv3-source-to-shader emission is a future Phase-D wiring slice.
//!   - **Per-frame dirty-tracking** : transform propagation is O(N) per
//!     frame ; incremental + dirty-flag-based propagation deferred.
//!   - **Async-compute pass parallelism** : RenderGraph topo-sort is
//!     single-queue ; multi-queue scheduling deferred.
//!   - **Live Vulkan submit** : the NullBackend covers the trait surface ;
//!     real Vulkan-backend driving Arc A770 readback is a follow-up that
//!     wires `cssl-host-vulkan` into [`backend::RenderBackend`].

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — tighten @ post-R1 stabilization slice.
#![allow(clippy::similar_names)] // Mathematical / RGB-channel short names read clearer.
#![allow(clippy::many_single_char_names)] // Matrix-element math reads better with i/j/k/m/n.
#![allow(clippy::float_cmp)] // Identity-matrix / round-trip tests want exact zero-drift checks.
#![allow(clippy::cast_precision_loss)] // u32→f32 casts for viewport / counts well-defined for typical sizes.
#![allow(clippy::neg_multiply)] // -1.0 * x reads clearer than -(x) in some derivations.
#![allow(clippy::suboptimal_flops)] // mul_add already used on hot paths ; literal-tuple tests keep simple form.
#![allow(clippy::struct_excessive_bools)] // Material flags read clearly as named bools.
#![allow(clippy::missing_const_for_fn)] // Many fn bodies could be const but readability + Rust 1.75 const-eval limits apply.
#![allow(clippy::match_same_arms)] // Format-byte tables intentionally enumerate variants for clarity.
#![allow(clippy::field_reassign_with_default)] // Test setups read clearer with let-mut + per-field assigns.
#![allow(clippy::map_unwrap_or)] // map(...).unwrap_or(...) is readable in scene-graph linkage walks.
#![allow(clippy::return_self_not_must_use)] // Builder-pattern setters on RenderPass intentionally unmarked — caller may chain or not.
#![allow(clippy::unnecessary_literal_bound)] // NullBackend::description returns 'static literal — lifetime is intentional.
#![allow(clippy::imprecise_flops)] // hypot vs sqrt of squares — readability over float precision in stage-0 area-light bound.
#![allow(clippy::let_underscore_must_use)] // PassContext field probes used for self-doc + future-extension witness.

pub mod asset;
pub mod backend;
pub mod graph;
pub mod light;
pub mod material;
pub mod math;
pub mod mesh;
pub mod queue;
pub mod scene;
pub mod submit;

/// Re-export of `cssl-substrate-projections` types under a stable namespace
/// for renderer consumers. Hides the dependency-path detail so consumers
/// can switch to a different projection layer without touching their imports.
pub mod projections {
    pub use cssl_substrate_projections::{
        frustum_cull, world_to_clip, Aabb, Camera, CameraError, Frustum, Mat4, ObserverFrame,
        Plane, ProjectionMatrix, Quat, Vec3, Vec4, Viewport,
    };
}

// ═══════════════════════════════════════════════════════════════════════════
// § Top-level re-exports — the canonical "use cssl_render::*" surface
// ═══════════════════════════════════════════════════════════════════════════

pub use asset::{
    AssetHandle, FilterMode, Sampler, SamplerHandle, Texture, TextureFormat, TextureHandle,
    WrapMode,
};
pub use backend::{
    BackendCommand, FrameStats, NullBackend, PassContext, RenderBackend, RenderError,
};
pub use graph::{
    AttachmentId, GraphError, PassId, PassKind, RenderGraph, RenderPass, MAX_ATTACHMENTS_PER_PASS,
};
pub use light::{Light, LightCommon};
pub use material::{AlphaMode, Material, MaterialBinding, MaterialModel};
pub use math::{Aabb, Mat4, Quat, Sphere, Transform, Vec2, Vec3, Vec4};
pub use mesh::{
    AttributeFormat, AttributeSemantic, IndexFormat, Mesh, MeshBuffer, SkinVertex, StandardVertex,
    Topology, VertexAttribute, VertexAttributeLayout, MAX_ATTRIBUTES,
};
pub use queue::{DrawCall, QueueStats, RenderQueue};
pub use scene::{NodeId, SceneChildIter, SceneError, SceneGraph, SceneNode};
pub use submit::{submit, RenderContext, RenderEffect};

/// Crate version exposed for scaffold verification — mirrors the same
/// constant pattern as the rest of the workspace.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
