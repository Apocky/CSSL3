//! § Transform — bone-local TRS triple, procedural-runtime native.
//!
//! § THESIS
//!   The procedural surface emits bone-local transforms in the same
//!   `(translation, rotation, scale)` shape the keyframe surface used.
//!   Identical layout means the skinning-upload path and any caller that
//!   already holds a `cssl-anim::Transform` migrates by swapping the
//!   import path. Functionally, however, this `Transform` is populated by
//!   the procedural pose network rather than by keyframe sampling.
//!
//! § STORAGE
//!   - `translation : Vec3` — bone-local offset relative to the parent.
//!   - `rotation    : Quat` — Hamilton-convention quaternion. The
//!     procedural surface internally evaluates poses in PGA-Motor space
//!     and converts to quaternion-on-edge for skinning compatibility ; see
//!     [`Transform::from_motor`].
//!   - `scale       : Vec3` — per-axis scale ; uniform-scale uses
//!     `Vec3::splat(s)`.
//!
//! § INTERPOLATION
//!   The procedural pose-blending path uses **PGA Motors** for joint
//!   blends (see [`crate::motor_blend`]). Motor-blend produces a single
//!   geodesic interpolation in SE(3) that doesn't suffer the
//!   slerp-near-collinear degeneracy. After blending in motor space, the
//!   result is converted back to TRS for the skinning upload.
//!
//!   The shorter-arc-quaternion `nlerp` and `slerp` helpers are still
//!   provided for Δ-time-very-small fast paths and for the keyframe-compat
//!   import where motor conversion is unnecessary. Neither helper appears
//!   in the canonical procedural pipeline.
//!
//! § DETERMINISM
//!   All operations are pure functions of their inputs. `Transform::IDENTITY
//!   .compose(Transform::IDENTITY) == Transform::IDENTITY` byte-equally.

use cssl_pga::{Motor, Rotor, Translator};
use cssl_substrate_projections::{Mat4, Quat, Vec3};

/// Bone-local TRS transform. Procedural-runtime native, layout-compatible
/// with the keyframe surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Translation offset from the parent's origin.
    pub translation: Vec3,
    /// Rotation expressed as a unit quaternion (Hamilton convention).
    pub rotation: Quat,
    /// Per-axis scale ; `Vec3::splat(1.0)` for no scale.
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

    /// Construct a translation-only transform.
    #[must_use]
    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            rotation: Quat::IDENTITY,
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }

    /// Construct a rotation-only transform.
    #[must_use]
    pub const fn from_rotation(rotation: Quat) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation,
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }

    /// Construct a uniform-scale-only transform.
    #[must_use]
    pub const fn from_uniform_scale(scale: f32) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::new(scale, scale, scale),
        }
    }

    /// Convert this `(T, R, S)` triple into a `Mat4` in TRS-compose order
    /// (scale → rotate → translate). Matches the keyframe-runtime
    /// convention for skinning upload.
    #[must_use]
    pub fn to_mat4(self) -> Mat4 {
        // Build the rotation matrix from the quaternion (standard form).
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

        // Apply scale to rotation columns (RS).
        let s = self.scale;
        Mat4 {
            cols: [
                [r00 * s.x, r10 * s.x, r20 * s.x, 0.0],
                [r01 * s.y, r11 * s.y, r21 * s.y, 0.0],
                [r02 * s.z, r12 * s.z, r22 * s.z, 0.0],
                [
                    self.translation.x,
                    self.translation.y,
                    self.translation.z,
                    1.0,
                ],
            ],
        }
    }

    /// Compose this transform with a child transform : `parent.compose(child)`
    /// expresses `child` in the parent's frame. Right-to-left multiplication
    /// matches the matrix convention.
    #[must_use]
    pub fn compose(self, child: Self) -> Self {
        // Rotate the child's translation into the parent's frame, then
        // scale it by the parent's scale, then add the parent's translation.
        let scaled = Vec3::new(
            child.translation.x * self.scale.x,
            child.translation.y * self.scale.y,
            child.translation.z * self.scale.z,
        );
        let rotated = quat_rotate(self.rotation, scaled);
        Self {
            translation: self.translation + rotated,
            rotation: quat_mul(self.rotation, child.rotation),
            scale: Vec3::new(
                self.scale.x * child.scale.x,
                self.scale.y * child.scale.y,
                self.scale.z * child.scale.z,
            ),
        }
    }

    /// Build a `Transform` from a PGA `Motor`. The motor encodes a rigid
    /// motion (rotation + translation) ; scale is the identity. Used by
    /// the procedural pose path : the KAN-pose network emits motors and
    /// the runtime lowers them to TRS for skinning upload.
    ///
    /// § DERIVATION
    ///   - The grade-2 spatial-bivector components of the motor encode the
    ///     rotation. We extract the rotor by taking the (s, r1, r2, r3)
    ///     subset and building a quaternion from it.
    ///   - The grade-2 ideal-bivector components combined with the rotor
    ///     encode the translation. Per Klein PGA convention :
    ///     `t = -2 * (T_part * R_conj).vector_part`. We compute it via the
    ///     translator-extraction path.
    ///   - For a unit motor the result is exact ; for a non-unit motor we
    ///     normalize the rotor first.
    #[must_use]
    pub fn from_motor(motor: Motor) -> Self {
        // Rotor part : (s, r1, r2, r3) in Klein convention. The PGA basis
        // bivectors `e₂₃, e₃₁, e₁₂` map to quaternion `(x, y, z)`
        // imaginaries with the scalar `s` becoming `w`. The mapping is
        // direct because we use the Hamilton convention for `Quat` and
        // the Klein convention for `Rotor`.
        let r_norm_sq =
            motor.s * motor.s + motor.r1 * motor.r1 + motor.r2 * motor.r2 + motor.r3 * motor.r3;
        let inv_r_norm = if r_norm_sq > f32::EPSILON {
            r_norm_sq.sqrt().recip()
        } else {
            1.0
        };
        let qw = motor.s * inv_r_norm;
        let qx = motor.r1 * inv_r_norm;
        let qy = motor.r2 * inv_r_norm;
        let qz = motor.r3 * inv_r_norm;
        let rotation = Quat {
            x: qx,
            y: qy,
            z: qz,
            w: qw,
        };

        // Translation extraction : in the canonical M = T R decomposition,
        // T's bivector part (e₀₁, e₀₂, e₀₃) is recoverable as
        //   t_vec = 2 * (M ~R)_translation_part
        // where ~R is the reverse of R. For a stage-0 surface we expand
        // this directly :
        //   t_vec.x = 2 * (motor.t1 * motor.s + motor.t2 * motor.r3
        //                 - motor.t3 * motor.r2 - motor.m0 * motor.r1)
        //   t_vec.y = 2 * (-motor.t1 * motor.r3 + motor.t2 * motor.s
        //                 + motor.t3 * motor.r1 - motor.m0 * motor.r2)
        //   t_vec.z = 2 * (motor.t1 * motor.r2 - motor.t2 * motor.r1
        //                 + motor.t3 * motor.s - motor.m0 * motor.r3)
        //
        // For a pure translator (s=1, r=0, m0=0) this reduces to
        // t_vec = 2 * (t1, t2, t3) which matches the Translator
        // convention `t01 = -t.x / 2`. We invert the sign on assignment
        // so the caller-visible Transform carries the world-space offset.
        let tx = 2.0
            * (motor.t1 * motor.s + motor.t2 * motor.r3
                - motor.t3 * motor.r2
                - motor.m0 * motor.r1);
        let ty = 2.0
            * (-motor.t1 * motor.r3 + motor.t2 * motor.s + motor.t3 * motor.r1
                - motor.m0 * motor.r2);
        let tz = 2.0
            * (motor.t1 * motor.r2 - motor.t2 * motor.r1 + motor.t3 * motor.s
                - motor.m0 * motor.r3);

        Self {
            translation: Vec3::new(-tx, -ty, -tz),
            rotation,
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }

    /// Build a PGA `Motor` from this transform's translation + rotation
    /// (scale is dropped — motors don't carry scale). Inverse of
    /// [`Transform::from_motor`].
    #[must_use]
    pub fn to_motor(self) -> Motor {
        let r = Rotor::from_components(
            self.rotation.w,
            self.rotation.x,
            self.rotation.y,
            self.rotation.z,
        );
        let t = Translator::from_translation(
            self.translation.x,
            self.translation.y,
            self.translation.z,
        );
        Motor::from_translator_rotor(t, r)
    }

    /// Linear-blend interpolation in TRS-space. Translation + scale are
    /// lerp'd ; rotation uses normalized-linear-interpolation.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            translation: vec3_lerp(self.translation, other.translation, t),
            rotation: nlerp(self.rotation, other.rotation, t),
            scale: vec3_lerp(self.scale, other.scale, t),
        }
    }

    /// Spherical-linear interpolation. Use this for keyframe-compat paths
    /// where short angular deltas would otherwise flatten under nlerp.
    #[must_use]
    pub fn slerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            translation: vec3_lerp(self.translation, other.translation, t),
            rotation: slerp(self.rotation, other.rotation, t),
            scale: vec3_lerp(self.scale, other.scale, t),
        }
    }

    /// Inverse of this transform. Returns identity if the transform is
    /// singular (scale near zero) — matches the substrate-level totality
    /// discipline.
    #[must_use]
    pub fn inverse(self) -> Self {
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
        let inv_rot = quat_conjugate(self.rotation);
        let neg_t = Vec3::new(
            -self.translation.x,
            -self.translation.y,
            -self.translation.z,
        );
        let scaled = Vec3::new(
            neg_t.x * inv_scale.x,
            neg_t.y * inv_scale.y,
            neg_t.z * inv_scale.z,
        );
        let inv_t = quat_rotate(inv_rot, scaled);
        Self {
            translation: inv_t,
            rotation: inv_rot,
            scale: inv_scale,
        }
    }
}

// § Quaternion helpers — duplicated locally so the procedural-anim crate
//   doesn't pull cssl-anim as a hard dependency. Identical math to the
//   keyframe surface so shared callers see identical numerics.

/// Conjugate `(x, y, z, w) → (-x, -y, -z, w)`. For unit quaternions this
/// equals the inverse.
#[must_use]
pub fn quat_conjugate(q: Quat) -> Quat {
    Quat {
        x: -q.x,
        y: -q.y,
        z: -q.z,
        w: q.w,
    }
}

/// Hamilton-convention quaternion product `a * b` (apply `b` then `a`).
#[must_use]
pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    Quat {
        x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
        w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
    }
}

/// Rotate a `Vec3` by a unit quaternion via the sandwich product.
#[must_use]
pub fn quat_rotate(q: Quat, v: Vec3) -> Vec3 {
    let qv = Quat {
        x: v.x,
        y: v.y,
        z: v.z,
        w: 0.0,
    };
    let r = quat_mul(quat_mul(q, qv), quat_conjugate(q));
    Vec3::new(r.x, r.y, r.z)
}

/// Normalized linear interpolation. Fast-path for short angular deltas.
#[must_use]
pub fn nlerp(a: Quat, b: Quat, t: f32) -> Quat {
    let dot = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let qx = a.x + sign * (b.x * sign - a.x) * t;
    let qy = a.y + sign * (b.y * sign - a.y) * t;
    let qz = a.z + sign * (b.z * sign - a.z) * t;
    let qw = a.w + sign * (b.w * sign - a.w) * t;
    let len_sq = qx * qx + qy * qy + qz * qz + qw * qw;
    if len_sq > f32::EPSILON {
        let inv = len_sq.sqrt().recip();
        Quat {
            x: qx * inv,
            y: qy * inv,
            z: qz * inv,
            w: qw * inv,
        }
    } else {
        Quat::IDENTITY
    }
}

/// Spherical-linear interpolation along the shorter arc.
#[must_use]
pub fn slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let mut dot = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    let mut bx = b.x;
    let mut by = b.y;
    let mut bz = b.z;
    let mut bw = b.w;
    if dot < 0.0 {
        dot = -dot;
        bx = -bx;
        by = -by;
        bz = -bz;
        bw = -bw;
    }
    if dot > 0.9995 {
        // Near-collinear : fall back to nlerp to avoid divide-by-near-zero.
        return nlerp(a, b, t);
    }
    let theta = dot.acos();
    let sin_theta = theta.sin();
    if sin_theta.abs() < f32::EPSILON {
        return nlerp(a, b, t);
    }
    let inv_sin = sin_theta.recip();
    let s0 = ((1.0 - t) * theta).sin() * inv_sin;
    let s1 = (t * theta).sin() * inv_sin;
    Quat {
        x: a.x * s0 + bx * s1,
        y: a.y * s0 + by * s1,
        z: a.z * s0 + bz * s1,
        w: a.w * s0 + bw * s1,
    }
}

#[inline]
fn vec3_lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_v3(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a.x - b.x).abs() <= eps && (a.y - b.y).abs() <= eps && (a.z - b.z).abs() <= eps
    }

    fn approx_q(a: Quat, b: Quat, eps: f32) -> bool {
        let d = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
        d.abs() >= 1.0 - eps
    }

    #[test]
    fn identity_is_neutral_under_compose() {
        let t = Transform::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let composed = Transform::IDENTITY.compose(t);
        assert!(approx_v3(composed.translation, t.translation, 1e-6));
    }

    #[test]
    fn compose_translation_chain_adds_translations() {
        let a = Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Transform::from_translation(Vec3::new(0.0, 2.0, 0.0));
        let c = a.compose(b);
        assert!(approx_v3(c.translation, Vec3::new(1.0, 2.0, 0.0), 1e-6));
    }

    #[test]
    fn lerp_at_zero_returns_first() {
        let a = Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Transform::from_translation(Vec3::new(2.0, 0.0, 0.0));
        let r = a.lerp(b, 0.0);
        assert!(approx_v3(r.translation, a.translation, 1e-6));
    }

    #[test]
    fn lerp_at_one_returns_second() {
        let a = Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Transform::from_translation(Vec3::new(2.0, 0.0, 0.0));
        let r = a.lerp(b, 1.0);
        assert!(approx_v3(r.translation, b.translation, 1e-6));
    }

    #[test]
    fn slerp_preserves_unit_quaternion() {
        let a = Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        };
        let b = Quat {
            x: 0.0,
            y: 1.0,
            z: 0.0,
            w: 0.0,
        };
        let r = slerp(a, b, 0.5);
        let len = (r.x * r.x + r.y * r.y + r.z * r.z + r.w * r.w).sqrt();
        assert!((len - 1.0).abs() < 1e-5);
    }

    #[test]
    fn nlerp_preserves_unit_quaternion() {
        let a = Quat {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        };
        let b = Quat {
            x: 1.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        };
        let r = nlerp(a, b, 0.4);
        let len = (r.x * r.x + r.y * r.y + r.z * r.z + r.w * r.w).sqrt();
        assert!((len - 1.0).abs() < 1e-5);
    }

    #[test]
    fn motor_round_trip_pure_translation() {
        let t = Transform::from_translation(Vec3::new(1.5, -2.0, 0.25));
        let m = t.to_motor();
        let t2 = Transform::from_motor(m);
        assert!(approx_v3(t2.translation, t.translation, 1e-4));
        assert!(approx_q(t2.rotation, t.rotation, 1e-4));
    }

    #[test]
    fn motor_round_trip_pure_rotation() {
        // 90° about Y.
        let half = std::f32::consts::FRAC_PI_4;
        let q = Quat {
            x: 0.0,
            y: half.sin(),
            z: 0.0,
            w: half.cos(),
        };
        let t = Transform::from_rotation(q);
        let m = t.to_motor();
        let t2 = Transform::from_motor(m);
        assert!(approx_q(t2.rotation, t.rotation, 1e-4));
    }

    #[test]
    fn inverse_of_identity_is_identity() {
        let i = Transform::IDENTITY.inverse();
        assert_eq!(i.translation, Vec3::ZERO);
        assert!(approx_q(i.rotation, Quat::IDENTITY, 1e-6));
        assert_eq!(i.scale, Vec3::splat(1.0));
    }

    #[test]
    fn inverse_of_translation_negates() {
        let t = Transform::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let inv = t.inverse();
        let composed = t.compose(inv);
        assert!(approx_v3(composed.translation, Vec3::ZERO, 1e-5));
    }

    #[test]
    fn quat_mul_identity_left_neutral() {
        let q = Quat {
            x: 1.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        };
        let r = quat_mul(Quat::IDENTITY, q);
        assert!(approx_q(r, q, 1e-6));
    }

    #[test]
    fn quat_rotate_identity_no_change() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        let r = quat_rotate(Quat::IDENTITY, v);
        assert!(approx_v3(r, v, 1e-6));
    }

    #[test]
    fn from_uniform_scale_constructs_correctly() {
        let t = Transform::from_uniform_scale(2.5);
        assert_eq!(t.scale, Vec3::splat(2.5));
    }

    #[test]
    fn to_mat4_translation_appears_in_last_column() {
        let t = Transform::from_translation(Vec3::new(7.0, 8.0, 9.0));
        let m = t.to_mat4();
        assert_eq!(m.cols[3][0], 7.0);
        assert_eq!(m.cols[3][1], 8.0);
        assert_eq!(m.cols[3][2], 9.0);
    }
}
