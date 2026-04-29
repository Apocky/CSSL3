//! Minimal vector / quaternion math used by the projections crate.
//!
//! § SPEC ANCHOR : `specs/30_SUBSTRATE.csl § PROJECTIONS § ObserverFrame`
//!   references `vec3`, `vec3'unit`, `mat4`. The full source-level vector
//!   surface (per the reserved `specs/18_VECTOR.csl` slot in the H-track design)
//!   is not yet emitted — this module supplies the stage-0 host-side runtime
//!   surface needed for capability-gated projection math.
//!
//! § HANDEDNESS
//!   The Substrate canonical convention is **right-handed, Y-up**, matching
//!   Vulkan's clip-space conventions (Z forward = -Z in view-space). All
//!   constructors (`Camera::look_at`, `ProjectionMatrix::perspective`, etc.)
//!   assume RH. The host-backend layer (`cssl-host-d3d12`) is responsible for
//!   the Y-flip + winding-order swap when targeting D3D12's LH default ; that
//!   layering keeps this crate substrate-target-agnostic.
//!
//! § STAGE-0 CHOICES
//!   - All floats are f32 ; f64 promotion is a future slice if precision-sensitive
//!     workloads (e.g. planet-scale rendering) land.
//!   - `Quat` stores `(x, y, z, w)` with `w` as the scalar component (Hamilton
//!     convention) — matches glam / cgmath / Unity's storage order.
//!   - No SIMD intrinsics : stage-0 prioritizes readability + portability over
//!     peak throughput. The host-backend layer is free to substitute SIMD
//!     accelerated paths once the surface stabilizes.
//!   - `Mat4` is column-major (Vulkan / GLSL convention) : `m[col][row]` indexing,
//!     `m * v` post-multiplies the column-vector. This matches the GPU shader
//!     interpretation directly, avoiding transpose at upload-time.

use core::ops::{Add, Mul, Neg, Sub};

/// 3-component float vector. Used for positions, directions, and unit-axes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// All-zero vector — the origin in world / view space.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    /// Positive X axis.
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// Positive Y axis (Substrate canonical "up").
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// Positive Z axis. RH convention : view-space forward is `-Z`.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Splat a single scalar across all three components.
    #[must_use]
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v, v)
    }

    /// Dot product. Returns a scalar.
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x
            .mul_add(other.x, self.y.mul_add(other.y, self.z * other.z))
    }

    /// Cross product, RH convention. Returns a `Vec3` perpendicular to both.
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y.mul_add(other.z, -(self.z * other.y)),
            self.z.mul_add(other.x, -(self.x * other.z)),
            self.x.mul_add(other.y, -(self.y * other.x)),
        )
    }

    /// Squared magnitude. Avoids the sqrt when only ordering / threshold
    /// comparisons are needed (e.g. distance-based LoD selection).
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Euclidean magnitude.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Normalized copy. Returns `Vec3::ZERO` for the zero vector rather than
    /// producing NaN — substrate-level math must be total to keep determinism
    /// invariants from upstream {DetRNG} effect-rows.
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

/// 4-component float vector. Used for clip-space positions + homogeneous coords.
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

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Lift a `Vec3` to a `Vec4` with explicit `w` (typically `1.0` for points,
    /// `0.0` for directions).
    #[must_use]
    pub const fn from_vec3(v: Vec3, w: f32) -> Self {
        Self::new(v.x, v.y, v.z, w)
    }

    /// Drop `w` and return the `xyz` triplet.
    #[must_use]
    pub const fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    /// Perspective divide — divide `xyz` by `w`, returning normalized device
    /// coordinates. Returns the zero vector if `w` is near-zero rather than
    /// producing NaN / infinity (substrate-level totality discipline).
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

/// Unit quaternion for orientation. Stores `(x, y, z, w)` with `w` scalar.
///
/// § CONVENTION
///   - Hamilton product (right-to-left composition : `q1 * q2` applies `q2`
///     first then `q1`).
///   - RH coordinate system : the rotation axis follows the right-hand rule.
///   - Normalize after every composition for numerical stability ; the
///     `Camera::orient` accessor returns a renormalized copy on every read so
///     downstream consumers always see a unit quaternion.
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
    /// Identity quaternion — represents zero rotation. `w = 1.0`, `xyz = 0`.
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// Construct from explicit components. Caller must ensure unit length ;
    /// most users should prefer [`Self::from_axis_angle`].
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Construct from an axis (assumed unit-length) and an angle in radians.
    /// RH convention — positive angle rotates counter-clockwise when looking
    /// along the axis from its tip toward the origin.
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle_rad: f32) -> Self {
        let half = angle_rad * 0.5;
        let s = half.sin();
        let c = half.cos();
        let ax = axis.normalize();
        Self::new(ax.x * s, ax.y * s, ax.z * s, c)
    }

    /// Squared magnitude. A unit quaternion has `length_squared() == 1.0`
    /// modulo float drift.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.x.mul_add(
            self.x,
            self.y
                .mul_add(self.y, self.z.mul_add(self.z, self.w * self.w)),
        )
    }

    /// Renormalize to unit length ; returns identity if degenerate (zero
    /// length) to maintain totality.
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

    /// Conjugate : negate the imaginary part. For unit quaternions this equals
    /// the inverse — used for rotating a vector "backward" through the orientation.
    #[must_use]
    pub const fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    /// Rotate a `Vec3` by this quaternion. RH convention.
    ///
    /// Implementation : `v' = q * (0, v) * q^-1`, expanded into the standard
    /// triple-cross-product form for efficiency :
    /// `v' = v + 2 * cross(q.xyz, cross(q.xyz, v) + q.w * v)`.
    #[must_use]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }

    /// Hamilton product : `self * other`. Composition reads right-to-left
    /// (apply `other` first, then `self`).
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

#[cfg(test)]
mod tests {
    use super::{Quat, Vec3, Vec4};

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn vec3_approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx_eq(a.x, b.x, eps) && approx_eq(a.y, b.y, eps) && approx_eq(a.z, b.z, eps)
    }

    #[test]
    fn vec3_basic_ops() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(b - a, Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(-a, Vec3::new(-1.0, -2.0, -3.0));
        assert!(approx_eq(a.dot(b), 1.0 * 4.0 + 2.0 * 5.0 + 3.0 * 6.0, 1e-6));
    }

    #[test]
    fn vec3_cross_obeys_right_hand_rule() {
        // X cross Y = Z in RH.
        assert!(vec3_approx_eq(Vec3::X.cross(Vec3::Y), Vec3::Z, 1e-6));
        // Y cross Z = X.
        assert!(vec3_approx_eq(Vec3::Y.cross(Vec3::Z), Vec3::X, 1e-6));
        // Z cross X = Y.
        assert!(vec3_approx_eq(Vec3::Z.cross(Vec3::X), Vec3::Y, 1e-6));
        // Anti-symmetry : Y cross X = -Z.
        assert!(vec3_approx_eq(Vec3::Y.cross(Vec3::X), -Vec3::Z, 1e-6));
    }

    #[test]
    fn vec3_normalize_zero_returns_zero() {
        // Substrate totality : zero-length vector must not produce NaN.
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
        // And nonzero vectors normalize to unit length.
        let n = Vec3::new(3.0, 4.0, 0.0).normalize();
        assert!(approx_eq(n.length(), 1.0, 1e-6));
    }

    #[test]
    fn vec4_perspective_divide_total() {
        // w == 0 must not crash or NaN ; substrate totality.
        let v = Vec4::new(1.0, 2.0, 3.0, 0.0);
        assert_eq!(v.perspective_divide(), Vec3::ZERO);
        // Normal case.
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
        // 90deg rotation around Y axis : X should map to -Z.
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        assert!(vec3_approx_eq(q.rotate(Vec3::X), -Vec3::Z, 1e-6));
        // 180deg around Z : X -> -X.
        let q = Quat::from_axis_angle(Vec3::Z, core::f32::consts::PI);
        assert!(vec3_approx_eq(q.rotate(Vec3::X), -Vec3::X, 1e-5));
    }

    #[test]
    fn quat_compose_is_associative_for_rotations() {
        // Compose two 45deg rotations around Y should equal one 90deg.
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
    fn quat_conjugate_is_inverse_for_unit() {
        let q = Quat::from_axis_angle(Vec3::new(1.0, 1.0, 1.0).normalize(), 0.7);
        let v = Vec3::new(1.0, 0.5, 0.25);
        let rotated = q.rotate(v);
        let restored = q.conjugate().rotate(rotated);
        assert!(vec3_approx_eq(restored, v, 1e-5));
    }
}
