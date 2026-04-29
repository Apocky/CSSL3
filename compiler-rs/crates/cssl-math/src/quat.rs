//! § Quat — unit quaternion for orientation
//!
//! Storage : `(x, y, z, w)` with `w` as the scalar (Hamilton convention).
//! This matches `glm` / Vulkan / `glam` / `cgmath` / Unity / Unreal — the
//! dominant convention in graphics codebases. The alternative `(w, x, y, z)`
//! is more common in robotics / control theory and is intentionally NOT
//! used here. Switching = MAJOR-VERSION bump.
//!
//! § HAMILTON PRODUCT
//!   `q1 * q2` applies `q2` first then `q1` (right-to-left composition,
//!   matching how matrix products read). The Vec3 rotation is
//!   `v' = q * (0, v) * q⁻¹` — expanded into the standard triple-cross-
//!   product form for efficiency in [`Quat::rotate`].
//!
//! § FAST vs. SAFE
//!   - [`Quat::slerp`] is the IEEE-correct spherical interpolation that
//!     gives constant angular velocity along the great-circle arc. The
//!     dot-product threshold falls back to a `nlerp` near-collinear
//!     branch to avoid the `1 / sin(theta)` division blowing up.
//!   - [`Quat::nlerp`] is the cheap normalize-after-lerp variant. NOT
//!     constant velocity but adequate for animation blending where the
//!     blend duration is short relative to the angular distance.
//!   - The shortest-arc fix-up (negating one quaternion if their dot
//!     product is negative) is applied in BOTH paths so both produce the
//!     visually-correct result for opposing-hemisphere inputs.

use core::ops::{Mul, Neg};

use crate::scalar::{lerp, EPSILON_F32, SMALL_EPSILON_F32};
use crate::vec3::Vec3;

/// Unit quaternion. Stores `(x, y, z, w)` with `w` scalar (Hamilton).
///
/// Most consumers should construct via [`Quat::from_axis_angle`] /
/// [`Quat::from_euler_yxz`] / [`Quat::from_mat3`] rather than the raw
/// component constructor — those guarantee unit-length output.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Quat {
    /// X imaginary component.
    pub x: f32,
    /// Y imaginary component.
    pub y: f32,
    /// Z imaginary component.
    pub z: f32,
    /// W scalar component.
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    /// Identity quaternion — represents zero rotation.
    pub const IDENTITY: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    /// Construct from raw components. Caller must ensure unit-length ;
    /// most users should prefer [`Self::from_axis_angle`].
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Construct from a unit-length axis and an angle in radians (RH).
    /// The axis is normalized internally — passing a non-unit axis is
    /// safe but allocates a normalize.
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle_rad: f32) -> Self {
        let half = angle_rad * 0.5;
        let s = half.sin();
        let c = half.cos();
        let ax = axis.normalize();
        Self::new(ax.x * s, ax.y * s, ax.z * s, c)
    }

    /// Construct from Euler angles in radians using `Y-X-Z` (yaw-pitch-roll)
    /// intrinsic order. This is the convention most game-engine inspectors
    /// surface : `yaw` around Y first, then `pitch` around the rotated X,
    /// then `roll` around the twice-rotated Z.
    #[must_use]
    pub fn from_euler_yxz(yaw: f32, pitch: f32, roll: f32) -> Self {
        // Compose : Q = Q_y * Q_x * Q_z.
        let qy = Self::from_axis_angle(Vec3::Y, yaw);
        let qx = Self::from_axis_angle(Vec3::X, pitch);
        let qz = Self::from_axis_angle(Vec3::Z, roll);
        qy * qx * qz
    }

    /// Squared magnitude. A unit quaternion has `length_squared() == 1`
    /// modulo accumulated float drift.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.x.mul_add(
            self.x,
            self.y
                .mul_add(self.y, self.z.mul_add(self.z, self.w * self.w)),
        )
    }

    /// Euclidean magnitude.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Renormalize to unit length. Returns identity if degenerate (zero
    /// length) for totality.
    #[must_use]
    pub fn normalize(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq > EPSILON_F32 {
            let inv = len_sq.sqrt().recip();
            Self::new(self.x * inv, self.y * inv, self.z * inv, self.w * inv)
        } else {
            Self::IDENTITY
        }
    }

    /// Conjugate : negate the imaginary part. For unit quaternions this
    /// equals the inverse.
    #[must_use]
    pub const fn conjugate(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    /// Multiplicative inverse. For unit quaternions this is the conjugate ;
    /// for non-unit quaternions it's `conjugate / length_squared`. Returns
    /// identity for the zero quaternion (totality).
    #[must_use]
    pub fn inverse(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq > EPSILON_F32 {
            let inv = len_sq.recip();
            Self::new(-self.x * inv, -self.y * inv, -self.z * inv, self.w * inv)
        } else {
            Self::IDENTITY
        }
    }

    /// Dot product. For unit quaternions, the value is `cos(theta/2)`
    /// where `theta` is the angle between the two represented rotations.
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x.mul_add(
            other.x,
            self.y
                .mul_add(other.y, self.z.mul_add(other.z, self.w * other.w)),
        )
    }

    /// Rotate a `Vec3` by this quaternion.
    ///
    /// Implementation : the standard `v + 2 * cross(q.xyz, cross(q.xyz, v) +
    /// q.w * v)` form, which is equivalent to `q * (0, v) * q⁻¹` but avoids
    /// constructing the intermediate quaternion.
    #[must_use]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = Vec3::new(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }

    /// Hamilton product `self * other`. Composition reads right-to-left :
    /// `(a * b).rotate(v) == a.rotate(b.rotate(v))`.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        let (lx, ly, lz, lw) = (self.x, self.y, self.z, self.w);
        let (rx, ry, rz, rw) = (other.x, other.y, other.z, other.w);
        Self::new(
            lw * rx + lx * rw + ly * rz - lz * ry,
            lw * ry - lx * rz + ly * rw + lz * rx,
            lw * rz + lx * ry - ly * rx + lz * rw,
            lw * rw - lx * rx - ly * ry - lz * rz,
        )
    }

    /// Spherical linear interpolation. Constant angular velocity along
    /// the great-circle arc on the unit-quaternion 3-sphere. `t` is NOT
    /// clamped — values outside `[0, 1]` extrapolate.
    ///
    /// Falls back to [`Self::nlerp`] when the two quaternions are
    /// near-collinear (`|dot| > 0.9995`) — at that point `1/sin(theta)`
    /// blows up and the linear path is visually indistinguishable.
    ///
    /// Applies the shortest-arc fixup : if `dot < 0`, the second
    /// quaternion is negated so the interpolation takes the short path
    /// through the represented-rotation space.
    #[must_use]
    pub fn slerp(self, other: Self, t: f32) -> Self {
        let mut other = other;
        let mut d = self.dot(other);
        if d < 0.0 {
            other = -other;
            d = -d;
        }
        // Near-collinear : fall back to nlerp to avoid the 1/sin division.
        if d > 0.9995 {
            return self.nlerp(other, t);
        }
        let theta = d.clamp(-1.0, 1.0).acos();
        let sin_theta = theta.sin();
        if sin_theta.abs() < SMALL_EPSILON_F32 {
            // Defensive — should be unreachable given the 0.9995 guard.
            return self.nlerp(other, t);
        }
        let inv_sin = sin_theta.recip();
        let s_self = ((1.0 - t) * theta).sin() * inv_sin;
        let s_other = (t * theta).sin() * inv_sin;
        Self::new(
            s_self.mul_add(self.x, s_other * other.x),
            s_self.mul_add(self.y, s_other * other.y),
            s_self.mul_add(self.z, s_other * other.z),
            s_self.mul_add(self.w, s_other * other.w),
        )
    }

    /// Normalized linear interpolation. Cheap alternative to slerp ;
    /// applies the shortest-arc fixup. Component-wise lerp followed by
    /// renormalize. NOT constant angular velocity but adequate for short
    /// blends and visually identical near `t = 0` / `t = 1`.
    #[must_use]
    pub fn nlerp(self, other: Self, t: f32) -> Self {
        let mut other = other;
        if self.dot(other) < 0.0 {
            other = -other;
        }
        Self::new(
            lerp(self.x, other.x, t),
            lerp(self.y, other.y, t),
            lerp(self.z, other.z, t),
            lerp(self.w, other.w, t),
        )
        .normalize()
    }

    /// Construct from a 3x3 rotation matrix. The matrix is assumed to
    /// be orthonormal (a proper rotation) — non-orthonormal inputs
    /// produce undefined results. See [`crate::Mat3::from_quat`] for
    /// the inverse path.
    ///
    /// Uses Shepperd's method to choose the largest diagonal element
    /// for numerical stability, avoiding the catastrophic cancellation
    /// that the naive `w = sqrt(1 + trace) / 2` form suffers near
    /// 180-degree rotations.
    #[must_use]
    pub fn from_mat3(m: &crate::mat3::Mat3) -> Self {
        let m00 = m.get(0, 0);
        let m11 = m.get(1, 1);
        let m22 = m.get(2, 2);
        let trace = m00 + m11 + m22;
        if trace > 0.0 {
            let s = (trace + 1.0).sqrt() * 2.0;
            let inv_s = s.recip();
            Self::new(
                (m.get(2, 1) - m.get(1, 2)) * inv_s,
                (m.get(0, 2) - m.get(2, 0)) * inv_s,
                (m.get(1, 0) - m.get(0, 1)) * inv_s,
                0.25 * s,
            )
        } else if m00 > m11 && m00 > m22 {
            let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
            let inv_s = s.recip();
            Self::new(
                0.25 * s,
                (m.get(0, 1) + m.get(1, 0)) * inv_s,
                (m.get(0, 2) + m.get(2, 0)) * inv_s,
                (m.get(2, 1) - m.get(1, 2)) * inv_s,
            )
        } else if m11 > m22 {
            let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
            let inv_s = s.recip();
            Self::new(
                (m.get(0, 1) + m.get(1, 0)) * inv_s,
                0.25 * s,
                (m.get(1, 2) + m.get(2, 1)) * inv_s,
                (m.get(0, 2) - m.get(2, 0)) * inv_s,
            )
        } else {
            let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
            let inv_s = s.recip();
            Self::new(
                (m.get(0, 2) + m.get(2, 0)) * inv_s,
                (m.get(1, 2) + m.get(2, 1)) * inv_s,
                0.25 * s,
                (m.get(1, 0) - m.get(0, 1)) * inv_s,
            )
        }
    }

    /// Lower to a 3x3 rotation matrix. Inverse of [`Self::from_mat3`]
    /// for unit quaternions.
    #[must_use]
    pub fn to_mat3(self) -> crate::mat3::Mat3 {
        crate::mat3::Mat3::from_quat(self)
    }
}

impl Neg for Quat {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, -self.w)
    }
}

impl Mul for Quat {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        self.compose(rhs)
    }
}

impl Mul<Vec3> for Quat {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Vec3 {
        self.rotate(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::Quat;
    use crate::vec3::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec_approx(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps) && approx(a.z, b.z, eps)
    }
    fn quat_approx(a: Quat, b: Quat, eps: f32) -> bool {
        // Two unit quaternions q and -q represent the same rotation —
        // accept either form.
        let direct = approx(a.x, b.x, eps)
            && approx(a.y, b.y, eps)
            && approx(a.z, b.z, eps)
            && approx(a.w, b.w, eps);
        let flipped = approx(a.x, -b.x, eps)
            && approx(a.y, -b.y, eps)
            && approx(a.z, -b.z, eps)
            && approx(a.w, -b.w, eps);
        direct || flipped
    }

    #[test]
    fn quat_identity_rotates_to_self() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert!(vec_approx(Quat::IDENTITY.rotate(v), v, 1e-6));
    }

    #[test]
    fn quat_axis_angle_known_rotations() {
        // 90deg around Y : X -> -Z (RH).
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        assert!(vec_approx(q.rotate(Vec3::X), -Vec3::Z, 1e-5));
        // 180deg around Z : X -> -X.
        let q = Quat::from_axis_angle(Vec3::Z, core::f32::consts::PI);
        assert!(vec_approx(q.rotate(Vec3::X), -Vec3::X, 1e-5));
    }

    #[test]
    fn quat_compose_associative_with_rotation() {
        let q45 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_4);
        let q90 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let combined = q45 * q45;
        assert!(vec_approx(
            combined.rotate(Vec3::X),
            q90.rotate(Vec3::X),
            1e-5
        ));
    }

    #[test]
    fn quat_normalize_preserves_unit() {
        let q = Quat::from_axis_angle(Vec3::Y, 1.234);
        let n = q.normalize();
        assert!(approx(n.length_squared(), 1.0, 1e-6));
    }

    #[test]
    fn quat_conjugate_inverts_unit_quaternion() {
        let q = Quat::from_axis_angle(Vec3::new(1.0, 1.0, 1.0).normalize(), 0.7);
        let v = Vec3::new(1.0, 0.5, 0.25);
        let rotated = q.rotate(v);
        let restored = q.conjugate().rotate(rotated);
        assert!(vec_approx(restored, v, 1e-5));
    }

    #[test]
    fn quat_inverse_inverts() {
        let q = Quat::from_axis_angle(Vec3::Y, 0.7);
        let v = Vec3::new(1.0, 0.5, 0.25);
        let rotated = q.rotate(v);
        let restored = q.inverse().rotate(rotated);
        assert!(vec_approx(restored, v, 1e-5));
    }

    #[test]
    fn quat_inverse_of_zero_quat_is_identity() {
        let zero = Quat::new(0.0, 0.0, 0.0, 0.0);
        assert_eq!(zero.inverse(), Quat::IDENTITY);
    }

    #[test]
    fn quat_slerp_at_zero_returns_self() {
        let q1 = Quat::from_axis_angle(Vec3::Y, 0.3);
        let q2 = Quat::from_axis_angle(Vec3::Y, 1.7);
        assert!(quat_approx(q1.slerp(q2, 0.0), q1, 1e-5));
    }

    #[test]
    fn quat_slerp_at_one_returns_other() {
        let q1 = Quat::from_axis_angle(Vec3::Y, 0.3);
        let q2 = Quat::from_axis_angle(Vec3::Y, 1.7);
        assert!(quat_approx(q1.slerp(q2, 1.0), q2, 1e-5));
    }

    #[test]
    fn quat_slerp_at_half_is_midpoint_rotation() {
        // Reference test for the dispatch's report-back :
        // Quat::slerp(IDENTITY, q90_y, 0.5) should equal q45_y.
        let q1 = Quat::IDENTITY;
        let q2 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let mid = q1.slerp(q2, 0.5);
        let q45 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_4);
        assert!(quat_approx(mid, q45, 1e-5));
        // And the rotated vector should be the 45deg rotation result.
        let v = Vec3::X;
        let rotated = mid.rotate(v);
        let expected = q45.rotate(v);
        assert!(vec_approx(rotated, expected, 1e-5));
    }

    #[test]
    fn quat_slerp_takes_short_path() {
        // q1 and -q2 represent the same rotation but on the opposite
        // hemisphere. Slerp should take the short path.
        let q1 = Quat::IDENTITY;
        let q90 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let neg_q90 = -q90;
        let mid_short = q1.slerp(q90, 0.5);
        let mid_neg = q1.slerp(neg_q90, 0.5);
        // The mid point should be the same rotation either way (since q
        // and -q represent the same orientation).
        let v = Vec3::X;
        let r_short = mid_short.rotate(v);
        let r_neg = mid_neg.rotate(v);
        assert!(vec_approx(r_short, r_neg, 1e-5));
    }

    #[test]
    fn quat_slerp_collinear_falls_back_to_nlerp() {
        // Identical quaternions : slerp should return identity-ish.
        let q = Quat::from_axis_angle(Vec3::Y, 0.5);
        let mid = q.slerp(q, 0.5);
        assert!(quat_approx(mid, q, 1e-5));
    }

    #[test]
    fn quat_nlerp_endpoints() {
        let q1 = Quat::IDENTITY;
        let q2 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        assert!(quat_approx(q1.nlerp(q2, 0.0), q1, 1e-5));
        assert!(quat_approx(q1.nlerp(q2, 1.0), q2, 1e-5));
    }

    #[test]
    fn quat_euler_yxz_matches_axis_compose() {
        // yaw=0.2, pitch=0.3, roll=0.4 should equal Q_y(0.2) * Q_x(0.3) * Q_z(0.4).
        let q_euler = Quat::from_euler_yxz(0.2, 0.3, 0.4);
        let q_y = Quat::from_axis_angle(Vec3::Y, 0.2);
        let q_x = Quat::from_axis_angle(Vec3::X, 0.3);
        let q_z = Quat::from_axis_angle(Vec3::Z, 0.4);
        let q_compose = q_y * q_x * q_z;
        // Compare via rotation of a probe vector.
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert!(vec_approx(q_euler.rotate(v), q_compose.rotate(v), 1e-5));
    }

    #[test]
    fn quat_mul_vec3_calls_rotate() {
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        assert!(vec_approx(q * Vec3::X, q.rotate(Vec3::X), 1e-6));
    }

    #[test]
    fn quat_repr_c_layout() {
        assert_eq!(core::mem::size_of::<Quat>(), 16);
        assert_eq!(core::mem::align_of::<Quat>(), 4);
    }
}
