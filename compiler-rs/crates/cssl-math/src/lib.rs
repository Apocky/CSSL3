//! § cssl-math — canonical 3D math library
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The foundation crate that the CSSLv3 renderer, physics, and animation
//!   subsystems all consume. Provides the canonical numeric types — `Vec2`,
//!   `Vec3`, `Vec4`, `Quat`, `Mat3`, `Mat4`, `Transform`, `Aabb`, `Sphere`,
//!   `Plane`, `Ray` — together with the interpolation helpers (`lerp`,
//!   `slerp`, `smoothstep`, `clamp`, etc.) and angle conversions every
//!   downstream system needs.
//!
//! § SPEC ANCHOR
//!   - `specs/30_SUBSTRATE.csl § PROJECTIONS` — handedness + depth + NDC
//!     conventions inherited from the H-track design.
//!   - `specs/30_SUBSTRATE.csl § Ω-TENSOR` — batch-op interop (when the
//!     `omega-batch` feature is enabled).
//!   - `specs/18_VECTOR.csl` — reserved slot for the source-level vector
//!     surface ; this crate supplies the Rust runtime surface that the
//!     emitted source-level surface will lower onto.
//!
//! § CONVENTIONS — locked at this slice ; switching = MAJOR-VERSION bump
//!   - **Right-handed, Y-up** coordinate system. View-space forward is
//!     `-Z`. Matches `cssl-substrate-projections` + Vulkan / OpenGL spatial
//!     convention. The host-backend layer applies the per-target NDC fixup.
//!   - **Column-major** matrices. `Mat3::cols[i][j]` is row `j`, column
//!     `i` ; `Mat4` uses the same. `m * v` post-multiplies the column
//!     vector. This matches Vulkan / WebGPU / GLSL upload conventions —
//!     the `to_cols_array()` flatteners drop straight into a shader-uniform
//!     buffer with no transpose.
//!   - **Reverse-Z** depth pairing. The projection-matrix constructors live
//!     in `cssl-substrate-projections`, NOT in this crate ; this crate
//!     supplies the Mat4 building blocks they compose. Reverse-Z is the
//!     substrate canonical pairing — depth-clear `0.0` + `GREATER`.
//!   - **Quaternion `(x, y, z, w)`** storage with `w` as the scalar
//!     component (Hamilton convention). Matches `glm` / Vulkan / `glam` /
//!     `cgmath` / Unity / Unreal. This is the dominant convention in
//!     graphics codebases ; the alternative `(w, x, y, z)` is more common
//!     in robotics / control theory.
//!
//! § STORAGE LAYOUT
//!   Every vector type is `#[repr(C)]` so a `&[Vec3]` slice can be cast
//!   directly to a `&[f32]` of three times the length for SIMD or GPU
//!   upload paths. The matrix types are similarly `#[repr(C)]`. The `Quat`
//!   layout matches `Vec4` (xyzw) so a quaternion slice can be uploaded
//!   the same way.
//!
//! § NUMERIC STABILITY DISCIPLINE
//!   All operations are TOTAL — never produce NaN or infinity for valid
//!   finite inputs. Degenerate cases (normalize of zero, perspective-divide
//!   by zero, slerp of opposite quaternions) return a sensible sentinel
//!   rather than propagating a NaN. This matches the substrate-level
//!   discipline established by `cssl-substrate-projections::vec` and is
//!   essential for upstream determinism guarantees from `{DetRNG}`-tagged
//!   effect rows. Where a "fast" variant is available with weaker
//!   stability, both are exposed (e.g. `Quat::slerp` + `Quat::nlerp`).
//!
//! § Ω-TENSOR INTEROP (feature `omega-batch`)
//!   With the `omega-batch` feature enabled, `&[Vec3]` slices convert into
//!   `OmegaTensor<f32, 2>` of shape `[N, 3]` for batch operations (skinning,
//!   broadphase, particle systems). The conversion is a copy at this slice
//!   to maintain ownership clarity ; a zero-copy view path lands when the
//!   tensor surface gains stride-permutation views in a later H-slice.
//!
//! § PRIME-DIRECTIVE
//!   Pure compute. No I/O, no logging, no allocation (the math types are
//!   all stack-resident `Copy` types). The Ω-tensor batch-op path is the
//!   only allocation-touching surface and is feature-gated. Behavior is
//!   what it appears to be — total, deterministic, transparent.
//!
//! § STAGE-0 LIMITATIONS / DEFERRED
//!   - SIMD intrinsics are deferred to a follow-up perf slice. Operations
//!     are written in plain Rust with `mul_add` for FMA-able paths ; the
//!     compiler auto-vectorizes the simple cases. SSE/NEON/AVX hand-tuned
//!     paths land when the renderer's hot loops need them.
//!   - `f64` variants of `Vec3` / `Mat4` etc. are NOT in this slice ; the
//!     foundation surface is `f32`-only to match game-engine hot paths.
//!     Planet-scale rendering or scientific-precision workloads will get
//!     `Vec3F64` etc. in a follow-up. The slerp / lerp helpers are written
//!     to be const-generic-friendly so the f64 path is mechanical.
//!   - Source-level CSSLv3 syntax for these types is part of the
//!     reserved `specs/18_VECTOR.csl` slot ; this crate is the runtime
//!     target the source-level surface will lower onto.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — same set as cssl-substrate-projections so the
// foundation surface reads consistently. Tighten in a polish slice once
// the API stabilizes.
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::neg_multiply)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::approx_constant)]
#![allow(clippy::too_many_arguments)]

pub mod aabb;
pub mod mat3;
pub mod mat4;
pub mod plane;
pub mod quat;
pub mod ray;
pub mod scalar;
pub mod sphere;
pub mod transform;
pub mod vec2;
pub mod vec3;
pub mod vec4;

#[cfg(feature = "omega-batch")]
pub mod omega_batch;

pub use aabb::Aabb;
pub use mat3::Mat3;
pub use mat4::Mat4;
pub use plane::Plane;
pub use quat::Quat;
pub use ray::Ray;
pub use scalar::{
    clamp, lerp, smoothstep, step, to_degrees, to_radians, wrap_angle, EPSILON_F32,
    SMALL_EPSILON_F32,
};
pub use sphere::Sphere;
pub use transform::Transform;
pub use vec2::Vec2;
pub use vec3::Vec3;
pub use vec4::Vec4;

/// Crate version exposed for scaffold verification — mirrors the same
/// constant pattern used elsewhere in the workspace.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
