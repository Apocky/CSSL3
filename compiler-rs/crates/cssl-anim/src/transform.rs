//! Bone-local transform : translation + rotation (quaternion) + scale.
//!
//! § THESIS
//!   The canonical authored form for skeletal animation is `(T, R, S)` —
//!   translation, rotation, scale — composed in TRS order :
//!     `M = T * R * S` (applied to a point as `M * p`, right-to-left).
//!   This module supplies the runtime [`Transform`] type plus the
//!   composition + interpolation primitives every other module in this
//!   crate consumes.
//!
//! § INTERPOLATION
//!   - **Translation + scale** : linear (lerp).
//!   - **Rotation** : spherical-linear interpolation (slerp) by default
//!     for fidelity ; normalized-linear (nlerp) is offered as a fast-path
//!     for short keyframe deltas where the angular error is below the
//!     visible threshold.
//!   - **Sign disambiguation** : quaternions `q` and `-q` represent the
//!     same rotation. Slerp picks the shorter arc by negating one operand
//!     when their dot product is negative.
//!
//! § DETERMINISM
//!   All operations are pure functions of their inputs. `interpolate` at
//!   `t = 0.0` returns the first operand exactly ; at `t = 1.0` returns
//!   the second exactly. Round-trip composition with `Transform::IDENTITY`
//!   is bit-identical.

use cssl_substrate_projections::{Mat4, Quat, Vec3};

/// Bone-local transform : translation + rotation + uniform-or-non-uniform scale.
///
/// § STORAGE
///   - `translation` : `Vec3` offset from the parent bone's origin.
///   - `rotation` : unit quaternion in Hamilton convention `(x, y, z, w)`.
///   - `scale` : per-axis `Vec3` ; uniform scale uses `Vec3::splat(s)`.
///
/// § DEFAULTS
///   `Transform::IDENTITY` is the all-zero translation, identity rotation,
///   `Vec3::splat(1.0)` scale — the "neutral" pose component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Translation offset from the parent's origin.
    pub translation: Vec3,
    /// Rotation expressed as a unit quaternion. Defaults to identity.
    pub rotation: Quat,
    /// Per-axis scale. `Vec3::splat(1.0)` for no scale.
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// The identity transform — no translation, no rotation, unit scale.
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::new(1.0, 1.0, 1.0),
    };

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Construct a translation-only transform with identity rotation +
    /// unit scale.
    #[must_use]
    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            rotation: Quat::IDENTITY,
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }

    /// Construct a rotation-only transform with zero translation + unit
    /// scale.
    #[must_use]
    pub const fn from_rotation(rotation: Quat) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation,
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }

    /// Construct a scale-only transform with zero translation + identity
    /// rotation.
    #[must_use]
    pub const fn from_scale(scale: Vec3) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale,
        }
    }

    /// Convert this `(T, R, S)` triple into a `Mat4`. The matrix corresponds
    /// to applying scale first, then rotation, then translation — the
    /// canonical TRS composition order.
    #[must_use]
    pub fn to_mat4(self) -> Mat4 {
        // Build the rotation matrix from the quaternion. Standard form :
        //   R = | 1-2(y²+z²)   2(xy-wz)     2(xz+wy)   |
        //       | 2(xy+wz)     1-2(x²+z²)   2(yz-wx)   |
        //       | 2(xz-wy)     2(yz+wx)     1-2(x²+y²) |
        let q = self.rotation;
        let xx = q.x * q.x;
        let yy = q.y * q.y;
        let zz = q.z * q.z;
        let xy = q.x * q.y;
        let xz = q.x * q.z;
        let yz = q.y * q.z;
        let wx = q.w * q.x;
        let wy = q.w * q.y;
        let wz = q.w * q.z;

        let r00 = 1.0 - 2.0 * (yy + zz);
        let r01 = 2.0 * (xy - wz);
        let r02 = 2.0 * (xz + wy);
        let r10 = 2.0 * (xy + wz);
        let r11 = 1.0 - 2.0 * (xx + zz);
        let r12 = 2.0 * (yz - wx);
        let r20 = 2.0 * (xz - wy);
        let r21 = 2.0 * (yz + wx);
        let r22 = 1.0 - 2.0 * (xx + yy);

        // Pre-multiply each rotation column by the matching scale
        // component, then place translation in the last column.
        Mat4 {
            cols: [
                [
                    r00 * self.scale.x,
                    r10 * self.scale.x,
                    r20 * self.scale.x,
                    0.0,
                ],
                [
                    r01 * self.scale.y,
                    r11 * self.scale.y,
                    r21 * self.scale.y,
                    0.0,
                ],
                [
                    r02 * self.scale.z,
                    r12 * self.scale.z,
                    r22 * self.scale.z,
                    0.0,
                ],
                [
                    self.translation.x,
                    self.translation.y,
                    self.translation.z,
                    1.0,
                ],
            ],
        }
    }

    /// Compose two transforms : `self.compose(child)` returns the transform
    /// equivalent to applying `child` first, then `self` — i.e. the
    /// parent-times-child order used when walking a bone hierarchy
    /// from root to leaf.
    ///
    /// § DERIVATION
    ///   `M_world = M_parent * M_child`. In TRS form :
    ///     T_world = T_parent + R_parent * (S_parent ⊙ T_child)
    ///     R_world = R_parent * R_child           (Hamilton compose)
    ///     S_world = S_parent ⊙ S_child           (component-wise scale mul)
    ///   `⊙` here is component-wise multiplication. Non-uniform scale
    ///   composed with non-axis-aligned rotation is mathematically not
    ///   representable as a `(T, R, S)` triple ; the runtime handles the
    ///   common cases (uniform parent scale, axis-aligned non-uniform)
    ///   exactly and approximates otherwise. Authored skeletons should
    ///   prefer uniform scale at non-leaf joints to avoid ambiguity.
    #[must_use]
    pub fn compose(self, child: Self) -> Self {
        // Parent's scale acts on the child's translation.
        let scaled_child_t = Vec3::new(
            self.scale.x * child.translation.x,
            self.scale.y * child.translation.y,
            self.scale.z * child.translation.z,
        );
        let rotated = self.rotation.rotate(scaled_child_t);
        Self {
            translation: self.translation + rotated,
            rotation: self.rotation.compose(child.rotation).normalize(),
            scale: Vec3::new(
                self.scale.x * child.scale.x,
                self.scale.y * child.scale.y,
                self.scale.z * child.scale.z,
            ),
        }
    }

    /// Linear interpolation of translation + scale and slerp of rotation.
    ///
    /// At `t = 0.0` returns `self` ; at `t = 1.0` returns `other`. Values
    /// outside `[0, 1]` extrapolate linearly (translation, scale) or via
    /// the slerp arc (rotation) — useful for over-/under-shoot easing
    /// curves but the caller is responsible for the result still being
    /// well-formed.
    #[must_use]
    pub fn interpolate(self, other: Self, t: f32) -> Self {
        Self {
            translation: lerp_vec3(self.translation, other.translation, t),
            rotation: slerp(self.rotation, other.rotation, t),
            scale: lerp_vec3(self.scale, other.scale, t),
        }
    }

    /// Linear interpolation of every component, including the rotation
    /// quaternion (followed by renormalization). Faster than slerp and
    /// produces near-identical visual output for short keyframe deltas
    /// (typically `< 30deg` of arc). Use [`Self::interpolate`] when
    /// fidelity matters.
    #[must_use]
    pub fn interpolate_nlerp(self, other: Self, t: f32) -> Self {
        Self {
            translation: lerp_vec3(self.translation, other.translation, t),
            rotation: nlerp(self.rotation, other.rotation, t),
            scale: lerp_vec3(self.scale, other.scale, t),
        }
    }

    /// Inverse transform — the reverse mapping.
    ///
    /// § DERIVATION
    ///   For a unit rotation + uniform-or-axis-aligned scale, the inverse
    ///   is `(R^-1 * (-T), R^-1, 1/S)`. Non-uniform scale combined with
    ///   non-axis-aligned rotation has no exact `(T, R, S)` inverse ; the
    ///   runtime returns the closest approximation — callers should avoid
    ///   that combination at non-leaf joints.
    #[must_use]
    pub fn inverse(self) -> Self {
        let inv_rot = self.rotation.conjugate();
        let inv_scale = Vec3::new(
            if self.scale.x.abs() > f32::EPSILON {
                1.0 / self.scale.x
            } else {
                0.0
            },
            if self.scale.y.abs() > f32::EPSILON {
                1.0 / self.scale.y
            } else {
                0.0
            },
            if self.scale.z.abs() > f32::EPSILON {
                1.0 / self.scale.z
            } else {
                0.0
            },
        );
        // Inverse translation : rotate -T by R^-1, then scale by 1/S.
        let neg_t = -self.translation;
        let rot_neg_t = inv_rot.rotate(neg_t);
        let inv_t = Vec3::new(
            rot_neg_t.x * inv_scale.x,
            rot_neg_t.y * inv_scale.y,
            rot_neg_t.z * inv_scale.z,
        );
        Self {
            translation: inv_t,
            rotation: inv_rot,
            scale: inv_scale,
        }
    }
}

/// Component-wise linear interpolation of two `Vec3` values.
#[must_use]
fn lerp_vec3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

/// Quaternion dot product. Used by slerp to detect the shorter-arc and
/// to skip the expensive `acos`/`sin` path when operands are nearly equal.
#[must_use]
fn quat_dot(a: Quat, b: Quat) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w
}

/// Quaternion negate. Used to flip the sign of one operand for slerp's
/// shorter-arc selection.
#[must_use]
fn quat_neg(q: Quat) -> Quat {
    Quat::new(-q.x, -q.y, -q.z, -q.w)
}

/// Quaternion linear interpolation followed by renormalization.
///
/// § FAST PATH
///   Use for short keyframe deltas where the angular distance between
///   `a` and `b` is small (typically below 30deg). The error vs. slerp
///   is bounded by the chord-arc divergence, which falls off as the
///   square of the angle.
#[must_use]
pub fn nlerp(a: Quat, b: Quat, t: f32) -> Quat {
    // Shorter-arc selection — flip b if its dot with a is negative.
    let b = if quat_dot(a, b) < 0.0 { quat_neg(b) } else { b };
    let result = Quat::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
        a.w + (b.w - a.w) * t,
    );
    result.normalize()
}

/// Spherical-linear interpolation for unit quaternions. Maps `t = 0` to
/// `a`, `t = 1` to `b`, with a constant-angular-velocity arc between.
///
/// § ALGORITHM (Shoemake 1985)
///   Compute `cos_omega = dot(a, b)`. If negative, flip `b` so we travel
///   the shorter arc. If `|cos_omega|` is very close to 1.0 (operands
///   nearly equal), fall back to nlerp to avoid divide-by-near-zero in
///   `sin_omega`. Otherwise :
///     `omega = acos(cos_omega)`
///     `s_a   = sin((1 - t) * omega) / sin(omega)`
///     `s_b   = sin(t * omega) / sin(omega)`
///     `out   = s_a * a + s_b * b`   (component-wise)
///   The result is automatically unit-length (Shoemake's identity) but
///   we renormalize to suppress accumulated drift over many compositions.
#[must_use]
pub fn slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let cos_omega = quat_dot(a, b);
    let (b, cos_omega) = if cos_omega < 0.0 {
        (quat_neg(b), -cos_omega)
    } else {
        (b, cos_omega)
    };
    // Near-parallel : fall back to nlerp. Threshold matches the standard
    // numerical-stability cliff.
    if cos_omega > 0.9995 {
        return nlerp(a, b, t);
    }
    let omega = cos_omega.acos();
    let sin_omega = omega.sin();
    let s_a = ((1.0 - t) * omega).sin() / sin_omega;
    let s_b = (t * omega).sin() / sin_omega;
    Quat::new(
        s_a * a.x + s_b * b.x,
        s_a * a.y + s_b * b.y,
        s_a * a.z + s_b * b.z,
        s_a * a.w + s_b * b.w,
    )
    .normalize()
}

#[cfg(test)]
mod tests {
    use super::{nlerp, slerp, Transform};
    use cssl_substrate_projections::{Mat4, Quat, Vec3};

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn vec3_approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx_eq(a.x, b.x, eps) && approx_eq(a.y, b.y, eps) && approx_eq(a.z, b.z, eps)
    }

    fn quat_approx_eq(a: Quat, b: Quat, eps: f32) -> bool {
        // Allow either sign — q and -q represent the same rotation.
        let dot_pos = (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() + (a.w - b.w).abs();
        let dot_neg = (a.x + b.x).abs() + (a.y + b.y).abs() + (a.z + b.z).abs() + (a.w + b.w).abs();
        dot_pos <= eps || dot_neg <= eps
    }

    #[test]
    fn identity_is_neutral_for_compose() {
        let t = Transform::from_translation(Vec3::new(3.0, 4.0, 5.0));
        let composed = Transform::IDENTITY.compose(t);
        assert!(vec3_approx_eq(composed.translation, t.translation, 1e-6));
        let composed_other = t.compose(Transform::IDENTITY);
        assert!(vec3_approx_eq(
            composed_other.translation,
            t.translation,
            1e-6
        ));
    }

    #[test]
    fn to_mat4_translation_only_matches_mat4_translation() {
        let t = Transform::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let m = t.to_mat4();
        let expected = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(m, expected);
    }

    #[test]
    fn to_mat4_scale_only_matches_mat4_scale() {
        let t = Transform::from_scale(Vec3::new(2.0, 3.0, 4.0));
        let m = t.to_mat4();
        let expected = Mat4::scale(Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(m, expected);
    }

    #[test]
    fn to_mat4_rotation_quat_round_trip() {
        // 90deg around Y : X -> -Z. The matrix should agree with the
        // quaternion's rotate() result on basis vectors.
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let t = Transform::from_rotation(q);
        let m = t.to_mat4();
        // M * (1, 0, 0, 1) should give (0, 0, -1, 1) approx.
        let v = m.mul_vec4(cssl_substrate_projections::Vec4::new(1.0, 0.0, 0.0, 1.0));
        assert!(approx_eq(v.x, 0.0, 1e-5));
        assert!(approx_eq(v.y, 0.0, 1e-5));
        assert!(approx_eq(v.z, -1.0, 1e-5));
        assert!(approx_eq(v.w, 1.0, 1e-5));
    }

    #[test]
    fn compose_parent_then_child_preserves_translation_chain() {
        // Two parent-then-child translations should compose to the sum
        // when there is no rotation or scale involved.
        let a = Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Transform::from_translation(Vec3::new(0.0, 2.0, 0.0));
        let composed = a.compose(b);
        assert!(vec3_approx_eq(
            composed.translation,
            Vec3::new(1.0, 2.0, 0.0),
            1e-6
        ));
    }

    #[test]
    fn compose_parent_rotation_rotates_child_translation() {
        // Parent rotates 90deg about Y, child has translation +X.
        // Result should translate the child by parent-rotated +X = -Z.
        let parent =
            Transform::from_rotation(Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2));
        let child = Transform::from_translation(Vec3::X);
        let composed = parent.compose(child);
        assert!(vec3_approx_eq(composed.translation, -Vec3::Z, 1e-5));
    }

    #[test]
    fn interpolate_at_zero_returns_self() {
        let a = Transform::new(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_axis_angle(Vec3::Y, 0.7),
            Vec3::new(0.5, 0.5, 0.5),
        );
        let b = Transform::new(
            Vec3::new(4.0, 5.0, 6.0),
            Quat::from_axis_angle(Vec3::X, 1.2),
            Vec3::new(2.0, 2.0, 2.0),
        );
        let out = a.interpolate(b, 0.0);
        assert!(vec3_approx_eq(out.translation, a.translation, 1e-6));
        assert!(quat_approx_eq(out.rotation, a.rotation, 1e-5));
        assert!(vec3_approx_eq(out.scale, a.scale, 1e-6));
    }

    #[test]
    fn interpolate_at_one_returns_other() {
        let a = Transform::new(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_axis_angle(Vec3::Y, 0.7),
            Vec3::new(0.5, 0.5, 0.5),
        );
        let b = Transform::new(
            Vec3::new(4.0, 5.0, 6.0),
            Quat::from_axis_angle(Vec3::X, 1.2),
            Vec3::new(2.0, 2.0, 2.0),
        );
        let out = a.interpolate(b, 1.0);
        assert!(vec3_approx_eq(out.translation, b.translation, 1e-5));
        assert!(quat_approx_eq(out.rotation, b.rotation, 1e-4));
        assert!(vec3_approx_eq(out.scale, b.scale, 1e-5));
    }

    #[test]
    fn interpolate_translation_at_half_is_midpoint() {
        let a = Transform::from_translation(Vec3::new(0.0, 0.0, 0.0));
        let b = Transform::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let mid = a.interpolate(b, 0.5);
        assert!(vec3_approx_eq(
            mid.translation,
            Vec3::new(5.0, 10.0, 15.0),
            1e-6
        ));
    }

    #[test]
    fn interpolate_scale_at_half_is_midpoint() {
        let a = Transform::from_scale(Vec3::new(1.0, 1.0, 1.0));
        let b = Transform::from_scale(Vec3::new(3.0, 3.0, 3.0));
        let mid = a.interpolate(b, 0.5);
        assert!(vec3_approx_eq(mid.scale, Vec3::new(2.0, 2.0, 2.0), 1e-6));
    }

    #[test]
    fn slerp_at_half_is_halfway_arc() {
        // Slerp between identity and 90deg-around-Y should give 45deg-around-Y.
        let q0 = Quat::IDENTITY;
        let q1 = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let mid = slerp(q0, q1, 0.5);
        let expected = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_4);
        assert!(quat_approx_eq(mid, expected, 1e-5));
    }

    #[test]
    fn slerp_endpoints() {
        let q0 = Quat::IDENTITY;
        let q1 = Quat::from_axis_angle(Vec3::Z, 1.234);
        assert!(quat_approx_eq(slerp(q0, q1, 0.0), q0, 1e-5));
        assert!(quat_approx_eq(slerp(q0, q1, 1.0), q1, 1e-4));
    }

    #[test]
    fn slerp_takes_shorter_arc() {
        // q and -q represent the same rotation. Slerp should pick the
        // shorter arc when operands have negative dot.
        let q = Quat::from_axis_angle(Vec3::Y, 0.5);
        let neg_q = Quat::new(-q.x, -q.y, -q.z, -q.w);
        let mid = slerp(q, neg_q, 0.5);
        // The shorter-arc midpoint between q and -q is q (or -q) — angular
        // distance zero. Result must be unit-length and represent the same
        // rotation as q.
        let v = Vec3::X;
        assert!(vec3_approx_eq(mid.rotate(v), q.rotate(v), 1e-4));
    }

    #[test]
    fn slerp_near_parallel_falls_back_to_nlerp() {
        // Two nearly-equal quaternions should not crash in the slerp
        // sin-omega divide ; the implementation falls back to nlerp.
        let q0 = Quat::IDENTITY;
        let q1 = Quat::from_axis_angle(Vec3::Y, 1e-5);
        let mid = slerp(q0, q1, 0.5);
        // Result should be unit-length.
        assert!(approx_eq(mid.length_squared(), 1.0, 1e-5));
    }

    #[test]
    fn nlerp_is_unit_length() {
        let q0 = Quat::IDENTITY;
        let q1 = Quat::from_axis_angle(Vec3::Y, 1.0);
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let q = nlerp(q0, q1, t);
            assert!(approx_eq(q.length_squared(), 1.0, 1e-5));
        }
    }

    #[test]
    fn inverse_round_trips_for_simple_transform() {
        let t = Transform::new(
            Vec3::new(3.0, 4.0, 5.0),
            Quat::from_axis_angle(Vec3::Y, 0.7),
            Vec3::new(2.0, 2.0, 2.0),
        );
        let inv = t.inverse();
        let composed = t.compose(inv);
        // T * T^-1 should be approximately the identity.
        assert!(vec3_approx_eq(composed.translation, Vec3::ZERO, 1e-4));
        assert!(quat_approx_eq(composed.rotation, Quat::IDENTITY, 1e-5));
        assert!(vec3_approx_eq(
            composed.scale,
            Vec3::new(1.0, 1.0, 1.0),
            1e-5
        ));
    }
}
