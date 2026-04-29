//! § Transform — canonical scene-graph node transform
//!
//! `{ translation, rotation, scale }` triple — the SRT (or TRS in
//! application order) decomposition that scene graphs and animation
//! systems use as their canonical local-space representation. The
//! corresponding 4x4 matrix is computed on demand via
//! [`Transform::to_mat4`].
//!
//! § COMPOSITION
//!   [`Transform::compose`] : `parent.compose(child)` returns the
//!   world-space transform of `child` when `child` is expressed in
//!   `parent`'s local space. The standard scene-graph traversal
//!   pattern.
//!
//! § INVERSE
//!   [`Transform::inverse`] is exact for uniform-scale transforms —
//!   transposes the rotation, inverts the scale, and composes with
//!   `-translation`. For non-uniform scale the inverse is NOT
//!   representable as a TRS triple ; callers should fall back to
//!   `self.to_mat4().inverse()` for the general case. The doc-comment
//!   on [`Transform::inverse`] explains the algebraic reason.

use crate::mat4::Mat4;
use crate::quat::Quat;
use crate::scalar::SMALL_EPSILON_F32;
use crate::vec3::Vec3;

/// Canonical scene-graph node transform.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Transform {
    /// Translation in the parent space.
    pub translation: Vec3,
    /// Rotation expressed as a unit quaternion.
    pub rotation: Quat,
    /// Per-axis scale. Negative values flip ; zero-scale is degenerate
    /// and produces a non-invertible transform.
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Identity transform — origin, no rotation, unit scale.
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    /// Construct from translation only ; identity rotation + unit scale.
    #[must_use]
    pub const fn from_translation(t: Vec3) -> Self {
        Self {
            translation: t,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    /// Construct from rotation only.
    #[must_use]
    pub const fn from_rotation(r: Quat) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: r,
            scale: Vec3::ONE,
        }
    }

    /// Construct from scale only.
    #[must_use]
    pub const fn from_scale(s: Vec3) -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: s,
        }
    }

    /// Construct from all three components.
    #[must_use]
    pub const fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Lower to a 4x4 affine matrix `T * R * S`. Equivalent to
    /// `Mat4::from_translation_rotation_scale`.
    #[must_use]
    pub fn to_mat4(self) -> Mat4 {
        Mat4::from_translation_rotation_scale(self.translation, self.rotation, self.scale)
    }

    /// Compose : `self.compose(child)` returns the world transform of a
    /// child node expressed in self's local space. Equivalent to
    /// `(self.to_mat4() * child.to_mat4())` but stays in
    /// `{translation, rotation, scale}` form.
    ///
    /// The composition rule for two TRS transforms is :
    ///   - new translation = `self.translation + self.rotation.rotate(self.scale * child.translation)`
    ///   - new rotation = `self.rotation * child.rotation`
    ///   - new scale = `self.scale * child.scale` (componentwise)
    ///
    /// This rule is exact when neither transform contains shear (which
    /// `Transform` cannot represent — shear requires a general `Mat4`).
    #[must_use]
    pub fn compose(self, child: Self) -> Self {
        let scaled_t = self.scale.mul_componentwise(child.translation);
        let rotated_t = self.rotation.rotate(scaled_t);
        Self {
            translation: self.translation + rotated_t,
            rotation: self.rotation * child.rotation,
            scale: self.scale.mul_componentwise(child.scale),
        }
    }

    /// Multiplicative inverse — only EXACT for uniform-scale transforms.
    /// Returns `None` if any scale component is near-zero (would produce
    /// a non-invertible transform) OR if the scale is non-uniform.
    ///
    /// Derivation : the linear part `L = R * S` has inverse
    /// `S^-1 * R^T`. For this inverse to fit back into a `T * R * S`
    /// form we'd need `S^-1 * R^T == R' * S'` for some orthonormal `R'`
    /// and diagonal `S'`. That holds only when `S` is a uniform scale
    /// (`s_x = s_y = s_z`), because then `S^-1 = (1/s) * I` commutes
    /// with the rotation. For non-uniform scales the inverse is NOT
    /// representable as a TRS triple ; callers should fall back to
    /// `self.to_mat4().inverse()` for the general case.
    ///
    /// For uniform scale `s`, the inverse is :
    ///   - `S' = (1/s, 1/s, 1/s)`
    ///   - `R' = R^T` (the conjugate of `rotation`)
    ///   - `T' = -R' * S' * T`
    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        if self.scale.x.abs() < SMALL_EPSILON_F32
            || self.scale.y.abs() < SMALL_EPSILON_F32
            || self.scale.z.abs() < SMALL_EPSILON_F32
        {
            return None;
        }
        // Reject non-uniform scales — see the doc-comment for why.
        let sx = self.scale.x;
        if (self.scale.y - sx).abs() > SMALL_EPSILON_F32
            || (self.scale.z - sx).abs() > SMALL_EPSILON_F32
        {
            return None;
        }
        let inv_rotation = self.rotation.conjugate();
        let inv_s = sx.recip();
        let inv_scale = Vec3::splat(inv_s);
        // -R' * S' * T = -R'.rotate(inv_s * T).
        let scaled = self.translation * inv_s;
        let inv_translation = -inv_rotation.rotate(scaled);
        Some(Self {
            translation: inv_translation,
            rotation: inv_rotation,
            scale: inv_scale,
        })
    }

    /// Apply this transform to a point — `T R S p`.
    #[must_use]
    pub fn transform_point(self, p: Vec3) -> Vec3 {
        let scaled = self.scale.mul_componentwise(p);
        let rotated = self.rotation.rotate(scaled);
        rotated + self.translation
    }

    /// Apply this transform to a direction (translation has no effect).
    /// Note : if the scale is non-uniform, the direction's length is
    /// scaled too. Use [`Self::transform_normal`] for normal vectors
    /// where you want only the rotation applied.
    #[must_use]
    pub fn transform_vector(self, v: Vec3) -> Vec3 {
        let scaled = self.scale.mul_componentwise(v);
        self.rotation.rotate(scaled)
    }

    /// Apply this transform's rotation to a normal vector. The scale is
    /// NOT applied — for non-uniform scales this is the inverse-transpose
    /// of the upper-3x3 of the matrix. For unit-scale transforms this is
    /// just `rotation.rotate(n)`.
    ///
    /// For non-uniform-scale TRS the correct formula is
    /// `rotation.rotate(n / scale).normalize()` — we apply the
    /// inverse-scale to the normal before rotating, which is the
    /// behavior most engines expect for normal-mapping shading.
    #[must_use]
    pub fn transform_normal(self, n: Vec3) -> Vec3 {
        let inv_scale = Vec3::new(
            if self.scale.x.abs() > SMALL_EPSILON_F32 {
                self.scale.x.recip()
            } else {
                0.0
            },
            if self.scale.y.abs() > SMALL_EPSILON_F32 {
                self.scale.y.recip()
            } else {
                0.0
            },
            if self.scale.z.abs() > SMALL_EPSILON_F32 {
                self.scale.z.recip()
            } else {
                0.0
            },
        );
        self.rotation.rotate(inv_scale.mul_componentwise(n)).normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::Transform;
    use crate::quat::Quat;
    use crate::vec3::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec_approx(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps) && approx(a.z, b.z, eps)
    }

    #[test]
    fn transform_identity_is_no_op() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Transform::IDENTITY.transform_point(v), v);
        assert_eq!(Transform::IDENTITY.transform_vector(v), v);
    }

    #[test]
    fn transform_point_applies_trs_in_order() {
        let r = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let xform = Transform::from_trs(
            Vec3::new(10.0, 0.0, 0.0),
            r,
            Vec3::new(2.0, 2.0, 2.0),
        );
        // X axis : scale 2x ⇒ (2,0,0). Rotate 90 around Y ⇒ (0,0,-2).
        // Translate +10 X ⇒ (10, 0, -2).
        let out = xform.transform_point(Vec3::X);
        assert!(vec_approx(out, Vec3::new(10.0, 0.0, -2.0), 1e-5));
    }

    #[test]
    fn transform_vector_ignores_translation() {
        let xform = Transform::from_translation(Vec3::new(99.0, 99.0, 99.0));
        assert_eq!(xform.transform_vector(Vec3::X), Vec3::X);
    }

    #[test]
    fn transform_inverse_round_trip_is_identity_uniform_scale() {
        let r = Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 0.7);
        let xform = Transform::from_trs(
            Vec3::new(5.0, -3.0, 2.0),
            r,
            Vec3::splat(2.0),
        );
        let inv = xform.inverse().expect("non-zero uniform scale");
        let v = Vec3::new(1.0, 2.0, 3.0);
        let round_trip = inv.transform_point(xform.transform_point(v));
        assert!(vec_approx(round_trip, v, 1e-5));
    }

    #[test]
    fn transform_inverse_non_uniform_scale_returns_none() {
        // Non-uniform scale : inverse is NOT representable as TRS, so
        // we explicitly return None and require the caller to drop into
        // the general Mat4::inverse path.
        let xform = Transform::from_trs(
            Vec3::new(5.0, -3.0, 2.0),
            Quat::IDENTITY,
            Vec3::new(2.0, 1.5, 0.8),
        );
        assert!(xform.inverse().is_none());
    }

    #[test]
    fn transform_inverse_zero_scale_returns_none() {
        let xform = Transform::from_trs(Vec3::ZERO, Quat::IDENTITY, Vec3::new(0.0, 1.0, 1.0));
        assert!(xform.inverse().is_none());
    }

    #[test]
    fn transform_compose_is_associative() {
        let a = Transform::from_trs(
            Vec3::new(1.0, 0.0, 0.0),
            Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_4),
            Vec3::ONE,
        );
        let b = Transform::from_trs(
            Vec3::new(0.0, 1.0, 0.0),
            Quat::from_axis_angle(Vec3::X, core::f32::consts::FRAC_PI_4),
            Vec3::ONE,
        );
        let c = Transform::from_trs(
            Vec3::new(0.0, 0.0, 1.0),
            Quat::from_axis_angle(Vec3::Z, core::f32::consts::FRAC_PI_4),
            Vec3::ONE,
        );
        let v = Vec3::new(1.0, 2.0, 3.0);
        let lhs = a.compose(b).compose(c).transform_point(v);
        let rhs = a.compose(b.compose(c)).transform_point(v);
        assert!(vec_approx(lhs, rhs, 1e-5));
    }

    #[test]
    fn transform_compose_matches_mat4_compose() {
        let a = Transform::from_trs(
            Vec3::new(1.0, 0.0, 0.0),
            Quat::from_axis_angle(Vec3::Y, 0.5),
            Vec3::new(2.0, 2.0, 2.0),
        );
        let b = Transform::from_trs(
            Vec3::new(0.0, 1.0, 0.0),
            Quat::from_axis_angle(Vec3::X, 0.3),
            Vec3::new(1.5, 1.5, 1.5),
        );
        let v = Vec3::new(1.0, 2.0, 3.0);
        // Via Transform.compose
        let xform = a.compose(b);
        let p_xform = xform.transform_point(v);
        // Via Mat4 composition.
        let m = a.to_mat4() * b.to_mat4();
        let p_mat = m.transform_point3(v);
        assert!(vec_approx(p_xform, p_mat, 1e-4));
    }

    #[test]
    fn transform_to_mat4_round_trips_through_point_application() {
        let xform = Transform::from_trs(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_3),
            Vec3::new(2.0, 2.0, 2.0),
        );
        let v = Vec3::new(0.5, 0.5, 0.5);
        let p_xform = xform.transform_point(v);
        let p_mat = xform.to_mat4().transform_point3(v);
        assert!(vec_approx(p_xform, p_mat, 1e-5));
    }

    #[test]
    fn transform_normal_unit_scale_matches_rotation() {
        let xform = Transform::from_rotation(Quat::from_axis_angle(Vec3::Y, 0.5));
        let n = Vec3::new(0.0, 1.0, 0.0);
        let n_t = xform.transform_normal(n);
        let n_r = xform.rotation.rotate(n);
        assert!(vec_approx(n_t, n_r, 1e-5));
    }
}
