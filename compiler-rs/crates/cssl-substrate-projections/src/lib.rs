//! § cssl-substrate-projections — viewport / camera / observer-frame for the Substrate
//! ════════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The Substrate's perception layer : how observers (cameras / user-frame /
//!   debug-views) project the Ω-tensor's state into renderable + display-ready
//!   data. This crate is consumed by the host backends (cssl-host-vulkan,
//!   cssl-host-d3d12, future cssl-host-metal / cssl-host-webgpu / cssl-host-
//!   level-zero) for actual rendering ; this layer is target-agnostic + does
//!   no GPU resource binding itself.
//!
//! § SPEC ANCHOR
//!   `specs/30_SUBSTRATE.csl § PROJECTIONS` (the H-track design's canonical
//!   Projections section). Stage-0 surfaces a runtime-friendly subset of the
//!   spec ; the parts deferred to a future slice are listed in § DEFERRED
//!   below.
//!
//! § SURFACE SUMMARY
//!   - **Math primitives** ([`Vec3`], [`Vec4`], [`Quat`], [`Mat4`]) — minimal
//!     RH Y-up f32 column-major math sufficient for projection-matrix work.
//!     The full source-level vector surface (per the reserved
//!     `specs/18_VECTOR.csl` slot) is a separate slice.
//!   - **Projection matrices** ([`ProjectionMatrix`]) — perspective + ortho
//!     constructors with **reverse-Z by default** + forward-Z alternative.
//!     NDC-Z range `[0, 1]` (Vulkan / D3D12 / WebGPU canonical).
//!   - **Camera + frustum** ([`Camera`], [`Aabb`], [`Frustum`],
//!     [`Plane`], [`world_to_clip`], [`frustum_cull`]) — the world→view→clip
//!     pipeline + AABB-vs-frustum culling via the Gribb / Hartmann
//!     row-extraction trick.
//!   - **Observer frame + viewport + LoD** ([`ObserverFrame`], [`Viewport`],
//!     [`LodPolicy`], [`select_lod`], [`ProjectionTarget`], [`ProjectionId`])
//!     — the canonical "what does this observer see" envelope, with
//!     distance-based LoD selection. Multiple observer-frames coexist for
//!     split-screen / stereo / mini-map / debug-introspect.
//!   - **Capability gating** ([`CapsToken`], [`Grant`], [`caps_grant`],
//!     [`caps_grant_debug_camera`], [`set_telemetry_hook`]) — the
//!     PRIME_DIRECTIVE-aligned access control gate. Projections cannot read
//!     the Ω-tensor without `OmegaTensorAccess` ; debug-cams require
//!     `DebugCamera` AND emit telemetry events ; cross-observer sharing
//!     requires `ObserverShare`.
//!
//! § HANDEDNESS + DEPTH CONVENTIONS — substrate canonical
//!   - **Right-handed, Y-up.** View-space forward is `-Z`. Matches Vulkan +
//!     OpenGL spatial convention.
//!   - **Reverse-Z perspective** by default. Near plane → `z = 1.0` in clip
//!     space, far plane → `z = 0.0`. Pair on the host side with depth-buffer
//!     cleared to `0.0` + `GREATER` depth-test for uniform precision across
//!     the entire frustum.
//!   - **NDC-Z range `[0, 1]`** (Vulkan / D3D12 / WebGPU). OpenGL's `[-1, 1]`
//!     Z range is NOT supported directly — host-backend wrappers can post-
//!     compose a remap if they target a `KHR_clip_control`-less GL pipeline.
//!   - **Column-major matrices.** `cols[i][j]` is row `j`, column `i`. The
//!     `Mat4::to_cols_array()` flattening matches Vulkan / GLSL `mat4` upload
//!     buffers directly.
//!
//! § HOST-BACKEND INTEGRATION
//!   The host crates consume this crate as a non-FFI dep. Each host applies
//!   the target-specific NDC / handedness fixups in its own bridging code :
//!   - **`cssl-host-vulkan`** : Vulkan's NDC has `Y-down`. The host post-
//!     multiplies a `Mat4::scale((1, -1, 1))` flip after this crate's
//!     projection matrix to land in Vulkan-canonical NDC.
//!   - **`cssl-host-d3d12`** : D3D12 also has NDC Z `[0, 1]` and Y-up by
//!     default ; minimal fixup ; winding-order convention may need a flip
//!     depending on the front-face declaration.
//!   - **`cssl-host-metal`** : NDC Z `[0, 1]`, Y-up ; matches substrate
//!     canonical directly.
//!   - **`cssl-host-webgpu`** : matches substrate canonical Z `[0, 1]` + Y-up.
//!   - **`cssl-host-level-zero`** : compute-only ; projections are NA.
//!
//! § PRIME-DIRECTIVE
//!   This crate is on the highest-risk surveillance vector in the substrate
//!   (`specs/30_SUBSTRATE.csl § SUBSTRATE-PRIME-DIRECTIVE-ALIGNMENT`). Every
//!   surveillance-relevant accessor is capability-gated :
//!   - Reading Ω-tensor state requires [`Grant::OmegaTensorAccess`].
//!   - Minting / using a debug-camera requires [`Grant::DebugCamera`] AND
//!     emits a [`DebugCamEvent`] regardless of grant outcome.
//!   - Sharing rendered output across observers requires [`Grant::ObserverShare`].
//!
//! § STAGE-0 LIMITATIONS / DEFERRED
//!   - **Source-level surface** : there is no source-level CSSLv3 syntax for
//!     `Camera` / `ObserverFrame` / `CapsToken` yet. Stage-0 hosts construct
//!     them via the Rust API directly. The source-level surface lands once
//!     `specs/18_VECTOR.csl` + the trait-resolve infra stabilizes.
//!   - **Hysteresis on LoD** : the spec's `LoDSchema` adds hysteresis +
//!     detail-mask + screen-pixel-target. Stage-0 surfaces only the distance
//!     threshold slice ; the rest land in a follow-up.
//!   - **CullHull variants** : the spec has `CullHull::{Frustum, AABB, Sphere,
//!     Compound}`. Stage-0 surfaces `Frustum + AABB` only ; sphere + compound
//!     are deferred.
//!   - **IfcMask + ConsentTokenSet integration** : stage-0 expresses
//!     capability gating through the simpler [`CapsToken`] / [`Grant`] pair.
//!     Full `{IfcMask, max_label, can_decl}` tuple-typing lands once the IFC
//!     surface gains source-level types in `cssl-effects::ifc`.
//!   - **Ω-tensor handle threading** : this crate exposes the gate but does
//!     NOT carry an Ω-tensor reference. The H1 slice (Ω-tensor) wires the
//!     `OmegaTensorAccess` grant into actual tensor reads.
//!
//! § STABILITY CONTRACT
//!   - [`Grant`] bit positions are STABLE from S8-H3 — see
//!     [`Grant::bit`] doc-comment.
//!   - [`Mat4`] column-major layout is STABLE — uploaded directly to shaders.
//!   - [`Camera::DEFAULT`] field values may change ; downstream code should
//!     not depend on the specific 60-deg / 16:9 / 0.1-1000 numbers.
//!   - Reverse-Z is the substrate canonical default ; switching the default
//!     to forward-Z would be a major-version bump.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — tighten @ S8-H3-phase-2 stabilization.
#![allow(clippy::similar_names)] // r/u/f basis vectors + tx/ty/tz translation read clearer with short names.
#![allow(clippy::many_single_char_names)] // matrix-element math reads more naturally with mathematical-style i/j/k/m/n.
#![allow(clippy::neg_multiply)] // -1.0 * x is occasionally clearer than -(x) in projection-matrix derivations.
#![allow(clippy::suboptimal_flops)]
// mul_add is used in production paths ; literal-tuple test fixtures keep the simple form for readability.
#![allow(clippy::float_cmp)] // exact equality is intentional in identity-matrix / round-trip tests where we want zero drift.
#![allow(clippy::cast_precision_loss)] // u32 viewport dimensions cast to f32 for aspect ratio is well-defined for typical screen sizes.
#![allow(clippy::excessive_precision)] // Camera::DEFAULT carries the literal 60-deg-radian constant at full f32 precision intentionally.
#![allow(clippy::approx_constant)] // tests use FRAC_PI_3 etc. directly via core::f32::consts ; clippy false-positives where the form already matches.

pub mod camera;
pub mod caps;
pub mod mat;
pub mod observer;
pub mod vec;

pub use camera::{
    frustum_cull, world_to_clip, Aabb, Camera, CameraError, Frustum, Plane, PLANE_BOTTOM,
    PLANE_FAR, PLANE_LEFT, PLANE_NEAR, PLANE_RIGHT, PLANE_TOP,
};
pub use caps::{
    caps_grant, caps_grant_debug_camera, clear_telemetry_hook, set_telemetry_hook,
    telemetry_event_count, CapsError, CapsToken, DebugCamEvent, Grant, TelemetryHook,
};
pub use mat::{Mat4, ProjectionMatrix};
pub use observer::{
    select_lod, LodError, LodPolicy, ObserverFrame, ProjectionId, ProjectionTarget, Viewport,
};
pub use vec::{Quat, Vec3, Vec4};

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
