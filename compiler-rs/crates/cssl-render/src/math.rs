//! § cssl-render::math — canonical 3D math surface (local stub for in-flight cssl-math)
//! ════════════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Local canonical-math surface that cssl-render needs to operate. The
//!   cssl-math crate (M1, slice S9-MATH) is sibling-in-flight at the time of
//!   the R1 slice : it exists in the `.claude/worktrees/MATH/` worktree but
//!   has not landed on `parallel-fanout` yet. Per the wave-7 G-axis pattern,
//!   this module defines the surface cssl-render needs locally so the slice
//!   compiles + tests against current parallel-fanout HEAD.
//!
//! § FUTURE — when cssl-math lands
//!   This module shrinks to a re-export wrapper :
//!   ```text
//!   pub use cssl_math::{Vec3, Vec4, Quat, Mat4, Transform, Aabb, Sphere};
//!   ```
//!   Consumers (cssl-render::scene / mesh / queue / graph) see zero API
//!   change. Field layouts + RH Y-up convention + column-major Mat4 +
//!   reverse-Z handedness lock are designed to match cssl-math's expected
//!   shape, which itself conforms to `cssl-substrate-projections::vec` /
//!   `::mat` types so the projections-renderer-math triangle is consistent.
//!
//! § HANDEDNESS + DEPTH CONVENTIONS — substrate canonical (locked by H3 + M1)
//!   - **Right-handed, Y-up.** View-space forward is `-Z`.
//!   - **Reverse-Z perspective** by default. Near plane → `z = 1.0` in clip
//!     space, far plane → `z = 0.0`. Pair on host side with depth-buffer
//!     cleared to `0.0` + `GREATER` depth-test.
//!   - **NDC-Z range `[0, 1]`** (Vulkan / D3D12 / WebGPU canonical).
//!   - **Column-major Mat4.** `cols[i][j]` is row `j`, column `i`. The
//!     `Mat4::to_cols_array()` flattening matches Vulkan / GLSL `mat4`
//!     upload buffers directly without transpose.
//!
//! § SCOPE (this slice — cssl-math::canonical-stub)
//!   - [`Vec3`] / [`Vec4`]   — 3D + homogeneous float vectors
//!   - [`Quat`]              — unit quaternion (Hamilton convention)
//!   - [`Mat4`]              — 4x4 column-major float matrix
//!   - [`Transform`]         — TRS composite (translation + rotation + scale)
//!   - [`Aabb`]              — axis-aligned bounding box
//!   - [`Sphere`]            — bounding sphere (center + radius)
//!
//! § INTERCHANGE WITH SUBSTRATE PROJECTIONS
//!   `cssl_substrate_projections` carries its own embedded math types with
//!   the same shape. `to_projections_*` helpers convert when an observer-
//!   frame is built from renderer state. The conversion is field-wise
//!   memcpy-equivalent — both crates use the same semantic conventions,
//!   they just don't share a type identity (yet, until cssl-math lands).
//!
//! § STAGE-0 LIMITATIONS (vs eventual cssl-math)
//!   - All floats are `f32`. f64 promotion deferred until precision-sensitive
//!     workloads (e.g. planet-scale rendering) land.
//!   - No SIMD intrinsics : readability + portability over peak throughput.
//!   - Hand-rolled minimal API : not the full glam / nalgebra surface.
//!     Only the operations the renderer needs at R1 stage.

use core::ops::{Add, Mul, Neg, Sub};

// ════════════════════════════════════════════════════════════════════════════
// § Vec2 — 2D float vector (UVs, screen-space, particle 2D state)
// ════════════════════════════════════════════════════════════════════════════

/// 2-component float vector. Primarily used for UV coordinates + screen-space
/// quantities. Minimal API at R1 ; more ops lift in as consumers need them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    /// Origin / all-zero.
    pub const ZERO: Self = Self::new(0.0, 0.0);
    /// All-ones.
    pub const ONE: Self = Self::new(1.0, 1.0);

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Splat a scalar across both components.
    #[must_use]
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v)
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x.mul_add(other.x, self.y * other.y)
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Vec3 — 3D float vector
// ════════════════════════════════════════════════════════════════════════════

/// 3-component float vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// All-zero vector — origin in world / view space.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    /// All-ones vector.
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    /// Positive X axis.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// Positive Y axis (substrate canonical "up").
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// Positive Z axis. RH convention : view-space forward is `-Z`.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Splat a scalar across all three components.
    #[must_use]
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v, v)
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x
            .mul_add(other.x, self.y.mul_add(other.y, self.z * other.z))
    }

    /// Cross product, RH convention.
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y.mul_add(other.z, -(self.z * other.y)),
            self.z.mul_add(other.x, -(self.x * other.z)),
            self.x.mul_add(other.y, -(self.y * other.x)),
        )
    }

    /// Squared magnitude.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Euclidean magnitude.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Normalized copy. Returns `Vec3::ZERO` for zero-length input rather
    /// than NaN — substrate-totality discipline.
    #[must_use]
    pub fn normalize(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq > f32::EPSILON {
            let inv = len_sq.sqrt().recip();
            Self::new(self.x * inv, self.y * inv, self.z * inv)
        } else {
            Self::ZERO
        }
    }

    /// Component-wise minimum.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self::new(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
        )
    }

    /// Component-wise maximum.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self::new(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
        )
    }

    /// Linear interpolation : `self + t * (other - self)`. `t = 0` returns
    /// self ; `t = 1` returns other. No clamping — caller responsibility.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}
impl Mul<Vec3> for Vec3 {
    /// Component-wise (Hadamard) product. Useful for material-color modulation.
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Vec4 — homogeneous 4-vector
// ════════════════════════════════════════════════════════════════════════════

/// 4-component float vector. Used for clip-space + homogeneous coordinates +
/// RGBA color (when adopted as a color type).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    /// All-zero 4-vector.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);
    /// All-ones 4-vector — equivalent to RGBA white.
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0, 1.0);

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Lift a `Vec3` to a `Vec4` with explicit `w`. `1.0` for points,
    /// `0.0` for directions.
    #[must_use]
    pub const fn from_vec3(v: Vec3, w: f32) -> Self {
        Self::new(v.x, v.y, v.z, w)
    }

    /// Drop `w` and return the `xyz` triplet.
    #[must_use]
    pub const fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    /// Perspective divide — divide `xyz` by `w`. Returns `Vec3::ZERO` for
    /// near-zero `w` rather than NaN / infinity.
    #[must_use]
    pub fn perspective_divide(self) -> Vec3 {
        if self.w.abs() > f32::EPSILON {
            let inv = self.w.recip();
            Vec3::new(self.x * inv, self.y * inv, self.z * inv)
        } else {
            Vec3::ZERO
        }
    }
}

impl Add for Vec4 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(
            self.x + rhs.x,
            self.y + rhs.y,
            self.z + rhs.z,
            self.w + rhs.w,
        )
    }
}
impl Mul<f32> for Vec4 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs, self.w * rhs)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Quat — unit quaternion
// ════════════════════════════════════════════════════════════════════════════

/// Unit quaternion for orientation. Stores `(x, y, z, w)` with `w` scalar.
///
/// § CONVENTION
///   - Hamilton product (right-to-left composition).
///   - RH coordinate system : rotation axis follows right-hand rule.
///   - `Quat::IDENTITY` represents zero rotation : looking down `-Z`, up `+Y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    /// Identity quaternion : zero rotation. `w = 1.0`, `xyz = 0`.
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// Construct from explicit components. Caller must ensure unit length ;
    /// most users prefer [`Self::from_axis_angle`].
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Construct from axis (assumed unit-length) + angle in radians.
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle_rad: f32) -> Self {
        let half = angle_rad * 0.5;
        let s = half.sin();
        let c = half.cos();
        let ax = axis.normalize();
        Self::new(ax.x * s, ax.y * s, ax.z * s, c)
    }

    /// Squared magnitude — `1.0` for unit quaternions modulo float drift.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.x.mul_add(
            self.x,
            self.y
                .mul_add(self.y, self.z.mul_add(self.z, self.w * self.w)),
        )
    }

    /// Renormalize to unit length ; returns identity if degenerate.
    #[must_use]
    pub fn normalize(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq > f32::EPSILON {
            let inv = len_sq.sqrt().recip();
            Self::new(self.x * inv, self.y * inv, self.z * inv, self.w * inv)
        } else {
            Self::IDENTITY
        }
    }

    /// Conjugate : negate the imaginary part. For unit quaternions this is
    /// the inverse rotation.
    #[must_use]
    pub const fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    /// Rotate a `Vec3` by this quaternion. RH convention.
    /// `v' = v + 2 * cross(q.xyz, cross(q.xyz, v) + q.w * v)`
    #[must_use]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }

    /// Hamilton product : `self * other`. Reads right-to-left.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        Self::new(
            self.w.mul_add(
                other.x,
                self.x.mul_add(other.w, self.y * other.z - self.z * other.y),
            ),
            self.w.mul_add(
                other.y,
                self.y.mul_add(other.w, self.z * other.x - self.x * other.z),
            ),
            self.w.mul_add(
                other.z,
                self.z.mul_add(other.w, self.x * other.y - self.y * other.x),
            ),
            self.w.mul_add(
                other.w,
                -(self.x * other.x + self.y * other.y + self.z * other.z),
            ),
        )
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Mat4 — 4x4 column-major float matrix
// ════════════════════════════════════════════════════════════════════════════

/// 4x4 column-major float matrix.
///
/// § STORAGE
///   `cols[i][j]` is the value at row `j`, column `i`. Matrix-vector multiply
///   `m * v` post-multiplies the column vector. `to_cols_array()` flattening
///   matches Vulkan / GLSL upload buffers directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// Column-major storage.
    pub cols: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    /// 4x4 identity matrix.
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Zero matrix.
    pub const ZERO: Self = Self {
        cols: [[0.0; 4]; 4],
    };

    /// Construct from a column-major flat array of 16 floats. The slice
    /// layout matches GLSL `mat4` upload buffers.
    #[must_use]
    pub const fn from_cols_array(arr: [f32; 16]) -> Self {
        Self {
            cols: [
                [arr[0], arr[1], arr[2], arr[3]],
                [arr[4], arr[5], arr[6], arr[7]],
                [arr[8], arr[9], arr[10], arr[11]],
                [arr[12], arr[13], arr[14], arr[15]],
            ],
        }
    }

    /// Flatten to a column-major `[f32; 16]` ready for shader-uniform upload.
    #[must_use]
    pub const fn to_cols_array(self) -> [f32; 16] {
        let c = self.cols;
        [
            c[0][0], c[0][1], c[0][2], c[0][3], //
            c[1][0], c[1][1], c[1][2], c[1][3], //
            c[2][0], c[2][1], c[2][2], c[2][3], //
            c[3][0], c[3][1], c[3][2], c[3][3], //
        ]
    }

    /// Indexed read. `(row, col)` order matches mathematical convention.
    #[must_use]
    pub const fn get(self, row: usize, col: usize) -> f32 {
        self.cols[col][row]
    }

    /// Indexed write.
    pub fn set(&mut self, row: usize, col: usize, v: f32) {
        self.cols[col][row] = v;
    }

    /// Translation matrix `T(tx, ty, tz)`. World-space translation moving
    /// points by the given offset.
    #[must_use]
    pub fn translation(t: Vec3) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[3][0] = t.x;
        m.cols[3][1] = t.y;
        m.cols[3][2] = t.z;
        m
    }

    /// Scale matrix `S(sx, sy, sz)`. Non-uniform scale on each axis.
    #[must_use]
    pub fn scale(s: Vec3) -> Self {
        let mut m = Self::ZERO;
        m.cols[0][0] = s.x;
        m.cols[1][1] = s.y;
        m.cols[2][2] = s.z;
        m.cols[3][3] = 1.0;
        m
    }

    /// Rotation matrix from a unit quaternion. RH convention.
    #[must_use]
    pub fn rotation(q: Quat) -> Self {
        let q = q.normalize();
        let xx = q.x * q.x;
        let yy = q.y * q.y;
        let zz = q.z * q.z;
        let xy = q.x * q.y;
        let xz = q.x * q.z;
        let yz = q.y * q.z;
        let wx = q.w * q.x;
        let wy = q.w * q.y;
        let wz = q.w * q.z;

        Self {
            cols: [
                [1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy), 0.0],
                [2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx), 0.0],
                [2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy), 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Matrix-matrix product `self * rhs`. Reads right-to-left.
    #[must_use]
    pub fn mul_mat(self, rhs: Self) -> Self {
        let mut out = Self::ZERO;
        for col in 0..4 {
            for row in 0..4 {
                let mut s = 0.0_f32;
                for k in 0..4 {
                    s = self.cols[k][row].mul_add(rhs.cols[col][k], s);
                }
                out.cols[col][row] = s;
            }
        }
        out
    }

    /// Matrix-vector product `self * v` treating `v` as a column vector with
    /// implicit homogeneous component `w = 1.0` (point semantics).
    #[must_use]
    pub fn mul_point(self, v: Vec3) -> Vec3 {
        let v4 = self.mul_vec4(Vec4::from_vec3(v, 1.0));
        if v4.w.abs() > f32::EPSILON && (v4.w - 1.0).abs() > f32::EPSILON {
            v4.perspective_divide()
        } else {
            v4.xyz()
        }
    }

    /// Matrix-vector product `self * v` treating `v` as a direction (`w = 0`,
    /// translation column does not apply).
    #[must_use]
    pub fn mul_dir(self, v: Vec3) -> Vec3 {
        let v4 = self.mul_vec4(Vec4::from_vec3(v, 0.0));
        v4.xyz()
    }

    /// Matrix-vector product on full Vec4.
    #[must_use]
    pub fn mul_vec4(self, v: Vec4) -> Vec4 {
        let c = &self.cols;
        Vec4::new(
            c[0][0] * v.x + c[1][0] * v.y + c[2][0] * v.z + c[3][0] * v.w,
            c[0][1] * v.x + c[1][1] * v.y + c[2][1] * v.z + c[3][1] * v.w,
            c[0][2] * v.x + c[1][2] * v.y + c[2][2] * v.z + c[3][2] * v.w,
            c[0][3] * v.x + c[1][3] * v.y + c[2][3] * v.z + c[3][3] * v.w,
        )
    }

    /// Transpose. Useful for converting column-major ↔ row-major when
    /// interfacing with row-major external libraries (e.g. some D3D APIs).
    #[must_use]
    pub fn transpose(self) -> Self {
        let c = &self.cols;
        Self {
            cols: [
                [c[0][0], c[1][0], c[2][0], c[3][0]],
                [c[0][1], c[1][1], c[2][1], c[3][1]],
                [c[0][2], c[1][2], c[2][2], c[3][2]],
                [c[0][3], c[1][3], c[2][3], c[3][3]],
            ],
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Transform — TRS composite
// ════════════════════════════════════════════════════════════════════════════

/// TRS composite : translation + rotation + scale. Applied right-to-left as
/// `M = T * R * S`. `position` translates to the world origin ; `orientation`
/// rotates ; `scale` scales each axis. Default is identity (zero translation,
/// identity rotation, unit scale).
///
/// § COMPOSITION SEMANTICS
///   When [`SceneNode`](crate::scene::SceneNode) propagates parent → child,
///   the child's local transform composes onto the parent's accumulated
///   world transform via `parent_world.compose(child_local)`. This matches
///   Unity / Unreal scene-graph conventions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    pub orientation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Identity transform : zero translation, identity rotation, unit scale.
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        orientation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(position: Vec3, orientation: Quat, scale: Vec3) -> Self {
        Self {
            position,
            orientation,
            scale,
        }
    }

    /// Construct a translation-only transform.
    #[must_use]
    pub const fn from_position(position: Vec3) -> Self {
        Self {
            position,
            orientation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    /// Lift to a 4x4 column-major matrix : `T * R * S`.
    #[must_use]
    pub fn to_matrix(self) -> Mat4 {
        let t = Mat4::translation(self.position);
        let r = Mat4::rotation(self.orientation);
        let s = Mat4::scale(self.scale);
        t.mul_mat(r).mul_mat(s)
    }

    /// Compose two transforms : `self * child`. Used for parent → child
    /// propagation in the scene graph. Semantics : `child` is interpreted in
    /// `self`'s local space.
    ///
    /// Note : non-uniform scale + non-axis-aligned rotation does NOT compose
    /// exactly via TRS — the result might require shear in general. For the
    /// scene-graph slice we accept the approximation (most real scenes use
    /// uniform scale on rotated nodes) ; consumers needing exact non-uniform
    /// scale composition should drop to `Mat4::mul_mat` directly.
    #[must_use]
    pub fn compose(self, child: Self) -> Self {
        let position = self.position + self.orientation.rotate(child.position * self.scale.x);
        let orientation = self.orientation.compose(child.orientation);
        let scale = self.scale * child.scale;
        Self::new(position, orientation, scale)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Aabb — axis-aligned bounding box
// ════════════════════════════════════════════════════════════════════════════

/// Axis-aligned bounding box in some coordinate space (typically world or
/// model). Used by frustum culling + spatial broad-phase + LoD distance
/// estimation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    /// Minimum corner (inclusive).
    pub min: Vec3,
    /// Maximum corner (inclusive).
    pub max: Vec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Aabb {
    /// Empty AABB : `min = +inf`, `max = -inf`. Adding any point produces a
    /// valid box. `contains_point` always returns false on the empty box.
    pub const EMPTY: Self = Self {
        min: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
        max: Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
    };

    /// Construct from explicit min / max corners. No validation — caller's
    /// responsibility to ensure `min <= max` component-wise.
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Construct from center + half-extents.
    #[must_use]
    pub fn from_center_extents(center: Vec3, half_extents: Vec3) -> Self {
        Self::new(center - half_extents, center + half_extents)
    }

    /// Center point.
    #[must_use]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Half-extents vector.
    #[must_use]
    pub fn half_extents(self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// Expand to include a point.
    #[must_use]
    pub fn expand_point(self, p: Vec3) -> Self {
        Self::new(self.min.min(p), self.max.max(p))
    }

    /// Union with another AABB.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self::new(self.min.min(other.min), self.max.max(other.max))
    }

    /// True if the box has positive volume on all axes (i.e. is non-empty).
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }

    /// True if the AABB contains the point (inclusive).
    #[must_use]
    pub fn contains_point(self, p: Vec3) -> bool {
        self.is_valid()
            && p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }

    /// Transform this AABB by a 4x4 matrix. Computes the bounding box of the
    /// 8 transformed corners. Conservative — the result may be larger than
    /// the tight bound of the rotated box.
    #[must_use]
    pub fn transform(self, m: Mat4) -> Self {
        if !self.is_valid() {
            return self;
        }
        let corners = [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ];
        let mut out = Self::EMPTY;
        for c in corners {
            out = out.expand_point(m.mul_point(c));
        }
        out
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Sphere — bounding sphere
// ════════════════════════════════════════════════════════════════════════════

/// Bounding sphere (center + radius). Cheaper culling primitive than AABB but
/// looser. Used for billboards + particle systems + light-influence-volumes
/// where rotation-invariance matters.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}

impl Sphere {
    /// Construct from explicit center + radius.
    #[must_use]
    pub const fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    /// True if the sphere contains the point.
    #[must_use]
    pub fn contains_point(self, p: Vec3) -> bool {
        (p - self.center).length_squared() <= self.radius * self.radius
    }

    /// Tight AABB enclosing this sphere.
    #[must_use]
    pub fn to_aabb(self) -> Aabb {
        Aabb::from_center_extents(self.center, Vec3::splat(self.radius))
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Substrate-projections interchange
// ════════════════════════════════════════════════════════════════════════════
//
// § ROLE : `cssl_substrate_projections` carries its own minimal Vec3/Quat/Mat4
//   surface. Until cssl-math lands and unifies the two, these helpers convert
//   the renderer's local types to / from the projections types so consumers
//   that build an `ObserverFrame` from renderer state don't have to hand-
//   marshal field-by-field.

use cssl_substrate_projections as proj;

impl Vec3 {
    /// Convert to `cssl_substrate_projections::Vec3`.
    #[must_use]
    pub const fn to_proj(self) -> proj::Vec3 {
        proj::Vec3::new(self.x, self.y, self.z)
    }

    /// Convert from `cssl_substrate_projections::Vec3`.
    #[must_use]
    pub const fn from_proj(v: proj::Vec3) -> Self {
        Self::new(v.x, v.y, v.z)
    }
}

impl Quat {
    /// Convert to `cssl_substrate_projections::Quat`.
    #[must_use]
    pub const fn to_proj(self) -> proj::Quat {
        proj::Quat::new(self.x, self.y, self.z, self.w)
    }

    /// Convert from `cssl_substrate_projections::Quat`.
    #[must_use]
    pub const fn from_proj(q: proj::Quat) -> Self {
        Self::new(q.x, q.y, q.z, q.w)
    }
}

impl Mat4 {
    /// Convert to `cssl_substrate_projections::Mat4` (column-major shape is
    /// identical between the two types — direct field copy).
    #[must_use]
    pub const fn to_proj(self) -> proj::Mat4 {
        proj::Mat4 { cols: self.cols }
    }

    /// Convert from `cssl_substrate_projections::Mat4`.
    #[must_use]
    pub const fn from_proj(m: proj::Mat4) -> Self {
        Self { cols: m.cols }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn vec3_approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx_eq(a.x, b.x, eps) && approx_eq(a.y, b.y, eps) && approx_eq(a.z, b.z, eps)
    }

    #[test]
    fn vec2_basic_ops() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 4.0);
        assert_eq!(a + b, Vec2::new(4.0, 6.0));
        assert_eq!(b - a, Vec2::new(2.0, 2.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert!(approx_eq(a.dot(b), 1.0 * 3.0 + 2.0 * 4.0, 1e-6));
    }

    #[test]
    fn vec2_constants() {
        assert_eq!(Vec2::ZERO, Vec2::new(0.0, 0.0));
        assert_eq!(Vec2::ONE, Vec2::new(1.0, 1.0));
        assert_eq!(Vec2::splat(3.0), Vec2::new(3.0, 3.0));
    }

    #[test]
    fn vec3_basic_ops() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(b - a, Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(-a, Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(a * b, Vec3::new(4.0, 10.0, 18.0));
        assert!(approx_eq(a.dot(b), 1.0 * 4.0 + 2.0 * 5.0 + 3.0 * 6.0, 1e-6));
    }

    #[test]
    fn vec3_constants_match_axes() {
        assert_eq!(Vec3::ZERO, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(Vec3::ONE, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(Vec3::X, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(Vec3::Y, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(Vec3::Z, Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn vec3_cross_obeys_right_hand_rule() {
        assert!(vec3_approx_eq(Vec3::X.cross(Vec3::Y), Vec3::Z, 1e-6));
        assert!(vec3_approx_eq(Vec3::Y.cross(Vec3::Z), Vec3::X, 1e-6));
        assert!(vec3_approx_eq(Vec3::Z.cross(Vec3::X), Vec3::Y, 1e-6));
        assert!(vec3_approx_eq(Vec3::Y.cross(Vec3::X), -Vec3::Z, 1e-6));
    }

    #[test]
    fn vec3_normalize_zero_returns_zero() {
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
        let n = Vec3::new(3.0, 4.0, 0.0).normalize();
        assert!(approx_eq(n.length(), 1.0, 1e-6));
    }

    #[test]
    fn vec3_min_max_componentwise() {
        let a = Vec3::new(1.0, 5.0, 3.0);
        let b = Vec3::new(4.0, 2.0, 6.0);
        assert_eq!(a.min(b), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(a.max(b), Vec3::new(4.0, 5.0, 6.0));
    }

    #[test]
    fn vec3_lerp_endpoints() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert!(vec3_approx_eq(
            a.lerp(b, 0.5),
            Vec3::new(2.5, 3.5, 4.5),
            1e-6
        ));
    }

    #[test]
    fn vec4_perspective_divide_total() {
        let v = Vec4::new(1.0, 2.0, 3.0, 0.0);
        assert_eq!(v.perspective_divide(), Vec3::ZERO);
        let v = Vec4::new(2.0, 4.0, 6.0, 2.0);
        assert!(vec3_approx_eq(
            v.perspective_divide(),
            Vec3::new(1.0, 2.0, 3.0),
            1e-6
        ));
    }

    #[test]
    fn quat_identity_rotates_to_self() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert!(vec3_approx_eq(Quat::IDENTITY.rotate(v), v, 1e-6));
    }

    #[test]
    fn quat_axis_angle_rotates_correctly() {
        // 90deg around Y : X -> -Z.
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        assert!(vec3_approx_eq(q.rotate(Vec3::X), -Vec3::Z, 1e-6));
        // 180deg around Z : X -> -X.
        let q = Quat::from_axis_angle(Vec3::Z, core::f32::consts::PI);
        assert!(vec3_approx_eq(q.rotate(Vec3::X), -Vec3::X, 1e-5));
    }

    #[test]
    fn quat_compose_associative_for_rotations() {
        let q45 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_4);
        let q90 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let combined = q45.compose(q45);
        let v = Vec3::X;
        assert!(vec3_approx_eq(combined.rotate(v), q90.rotate(v), 1e-5));
    }

    #[test]
    fn quat_normalize_preserves_unit_quaternion() {
        let q = Quat::from_axis_angle(Vec3::Y, 1.234);
        let n = q.normalize();
        assert!(approx_eq(n.length_squared(), 1.0, 1e-6));
    }

    #[test]
    fn quat_conjugate_inverse_for_unit() {
        let q = Quat::from_axis_angle(Vec3::new(1.0, 1.0, 1.0).normalize(), 0.7);
        let v = Vec3::new(1.0, 0.5, 0.25);
        let rotated = q.rotate(v);
        let restored = q.conjugate().rotate(rotated);
        assert!(vec3_approx_eq(restored, v, 1e-5));
    }

    #[test]
    fn mat4_identity_round_trip() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Mat4::IDENTITY.mul_point(v), v);
        assert_eq!(Mat4::IDENTITY.mul_dir(v), v);
    }

    #[test]
    fn mat4_translation_moves_point_not_direction() {
        let t = Mat4::translation(Vec3::new(10.0, 20.0, 30.0));
        let p = Vec3::new(1.0, 2.0, 3.0);
        // Point gets translated.
        assert_eq!(t.mul_point(p), Vec3::new(11.0, 22.0, 33.0));
        // Direction does NOT get translated (w = 0).
        assert_eq!(t.mul_dir(p), p);
    }

    #[test]
    fn mat4_scale_scales_point_and_direction() {
        let s = Mat4::scale(Vec3::new(2.0, 3.0, 4.0));
        let v = Vec3::new(1.0, 1.0, 1.0);
        assert_eq!(s.mul_point(v), Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(s.mul_dir(v), Vec3::new(2.0, 3.0, 4.0));
    }

    #[test]
    fn mat4_rotation_matches_quaternion() {
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let r = Mat4::rotation(q);
        assert!(vec3_approx_eq(r.mul_dir(Vec3::X), q.rotate(Vec3::X), 1e-5));
        assert!(vec3_approx_eq(r.mul_dir(Vec3::Y), q.rotate(Vec3::Y), 1e-5));
        assert!(vec3_approx_eq(r.mul_dir(Vec3::Z), q.rotate(Vec3::Z), 1e-5));
    }

    #[test]
    fn mat4_compose_associative() {
        let a = Mat4::translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Mat4::translation(Vec3::new(0.0, 2.0, 0.0));
        let c = Mat4::translation(Vec3::new(0.0, 0.0, 3.0));
        let abc1 = a.mul_mat(b).mul_mat(c);
        let abc2 = a.mul_mat(b.mul_mat(c));
        // Translations compose by addition.
        let p = Vec3::ZERO;
        assert_eq!(abc1.mul_point(p), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(abc2.mul_point(p), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn mat4_transpose_is_involution() {
        let m = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(m.transpose().transpose(), m);
    }

    #[test]
    fn mat4_to_from_cols_array_round_trip() {
        let arr = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        ];
        let m = Mat4::from_cols_array(arr);
        assert_eq!(m.to_cols_array(), arr);
    }

    #[test]
    fn transform_identity_to_matrix_is_identity() {
        assert_eq!(Transform::IDENTITY.to_matrix(), Mat4::IDENTITY);
    }

    #[test]
    fn transform_translation_only() {
        let t = Transform::from_position(Vec3::new(5.0, 6.0, 7.0));
        let m = t.to_matrix();
        assert_eq!(m.mul_point(Vec3::ZERO), Vec3::new(5.0, 6.0, 7.0));
    }

    #[test]
    fn transform_compose_translations_add() {
        let a = Transform::from_position(Vec3::new(1.0, 0.0, 0.0));
        let b = Transform::from_position(Vec3::new(0.0, 2.0, 0.0));
        let composed = a.compose(b);
        // Composed translation should equal sum (no rotation, unit scale).
        assert_eq!(composed.position, Vec3::new(1.0, 2.0, 0.0));
    }

    #[test]
    fn transform_compose_scales_multiply() {
        let a = Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(2.0));
        let b = Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(3.0));
        let composed = a.compose(b);
        assert_eq!(composed.scale, Vec3::splat(6.0));
    }

    #[test]
    fn aabb_empty_contains_no_points() {
        assert!(!Aabb::EMPTY.contains_point(Vec3::ZERO));
        assert!(!Aabb::EMPTY.is_valid());
    }

    #[test]
    fn aabb_unit_cube_contains_origin() {
        let cube = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        assert!(cube.is_valid());
        assert!(cube.contains_point(Vec3::ZERO));
        assert!(cube.contains_point(Vec3::splat(1.0)));
        assert!(cube.contains_point(Vec3::splat(-1.0)));
        assert!(!cube.contains_point(Vec3::splat(2.0)));
    }

    #[test]
    fn aabb_expand_point_grows_box() {
        let mut box_ = Aabb::EMPTY;
        box_ = box_.expand_point(Vec3::new(1.0, 2.0, 3.0));
        box_ = box_.expand_point(Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(box_.min, Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(box_.max, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn aabb_union() {
        let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb::new(Vec3::splat(2.0), Vec3::splat(3.0));
        let u = a.union(b);
        assert_eq!(u.min, Vec3::ZERO);
        assert_eq!(u.max, Vec3::splat(3.0));
    }

    #[test]
    fn aabb_center_extents() {
        let box_ = Aabb::from_center_extents(Vec3::new(5.0, 6.0, 7.0), Vec3::ONE);
        assert_eq!(box_.center(), Vec3::new(5.0, 6.0, 7.0));
        assert_eq!(box_.half_extents(), Vec3::ONE);
    }

    #[test]
    fn aabb_transform_translation() {
        let cube = Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0));
        let t = Mat4::translation(Vec3::new(5.0, 0.0, 0.0));
        let moved = cube.transform(t);
        assert_eq!(moved.min, Vec3::new(4.0, -1.0, -1.0));
        assert_eq!(moved.max, Vec3::new(6.0, 1.0, 1.0));
    }

    #[test]
    fn sphere_contains_point() {
        let s = Sphere::new(Vec3::ZERO, 2.0);
        assert!(s.contains_point(Vec3::ZERO));
        assert!(s.contains_point(Vec3::new(1.0, 1.0, 1.0)));
        assert!(!s.contains_point(Vec3::new(3.0, 0.0, 0.0)));
    }

    #[test]
    fn sphere_to_aabb_envelopes() {
        let s = Sphere::new(Vec3::new(1.0, 2.0, 3.0), 0.5);
        let bb = s.to_aabb();
        assert_eq!(bb.min, Vec3::new(0.5, 1.5, 2.5));
        assert_eq!(bb.max, Vec3::new(1.5, 2.5, 3.5));
    }

    // ─ projections-interchange round-trips ─

    #[test]
    fn vec3_proj_round_trip() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Vec3::from_proj(v.to_proj()), v);
    }

    #[test]
    fn quat_proj_round_trip() {
        let q = Quat::from_axis_angle(Vec3::Y, 1.234);
        let proj = q.to_proj();
        let back = Quat::from_proj(proj);
        assert_eq!(back, q);
    }

    #[test]
    fn mat4_proj_round_trip() {
        let m = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(Mat4::from_proj(m.to_proj()), m);
    }
}
