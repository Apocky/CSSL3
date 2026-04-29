//! § Mat4 — 4x4 column-major float matrix
//!
//! The general affine + projective transform type. `cols[col][row]`
//! indexing ; `m * v` post-multiplies the column vector. Identical
//! convention to [`crate::Mat3`] and `cssl-substrate-projections::Mat4`
//! so the host backends upload uniforms with no transpose.
//!
//! § PROJECTION CONSTRUCTORS
//!   This crate intentionally does NOT include `perspective_*` /
//!   `ortho_*` constructors. Those live in `cssl-substrate-projections`
//!   so the projection-matrix surface stays a single source of truth
//!   for the substrate canonical reverse-Z + RH conventions. This crate
//!   supplies `from_translation` / `from_scale` / `from_rotation` and
//!   the `look_at` / `inverse` / `transpose` infrastructure that the
//!   projections crate composes.

use core::ops::Mul;

use crate::mat3::Mat3;
use crate::quat::Quat;
use crate::scalar::SMALL_EPSILON_F32;
use crate::vec3::Vec3;
use crate::vec4::Vec4;

/// 4x4 column-major float matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Mat4 {
    /// Column-major storage. `cols[i][j]` is row `j`, column `i`.
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
    /// layout is the same as Vulkan / GLSL `mat4` upload buffers.
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

    /// Pure translation matrix.
    #[must_use]
    pub const fn from_translation(t: Vec3) -> Self {
        Self {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [t.x, t.y, t.z, 1.0],
            ],
        }
    }

    /// Pure scale matrix.
    #[must_use]
    pub const fn from_scale(s: Vec3) -> Self {
        Self {
            cols: [
                [s.x, 0.0, 0.0, 0.0],
                [0.0, s.y, 0.0, 0.0],
                [0.0, 0.0, s.z, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Pure rotation matrix from a unit quaternion.
    #[must_use]
    pub fn from_rotation(q: Quat) -> Self {
        let m = Mat3::from_quat(q);
        Self {
            cols: [
                [m.cols[0][0], m.cols[0][1], m.cols[0][2], 0.0],
                [m.cols[1][0], m.cols[1][1], m.cols[1][2], 0.0],
                [m.cols[2][0], m.cols[2][1], m.cols[2][2], 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Combined translation + rotation + scale, applied right-to-left
    /// `T * R * S`. Equivalent to `Transform { translation, rotation,
    /// scale }.to_mat4()`.
    #[must_use]
    pub fn from_translation_rotation_scale(t: Vec3, r: Quat, s: Vec3) -> Self {
        let m = Mat3::from_quat(r);
        Self {
            cols: [
                [m.cols[0][0] * s.x, m.cols[0][1] * s.x, m.cols[0][2] * s.x, 0.0],
                [m.cols[1][0] * s.y, m.cols[1][1] * s.y, m.cols[1][2] * s.y, 0.0],
                [m.cols[2][0] * s.z, m.cols[2][1] * s.z, m.cols[2][2] * s.z, 0.0],
                [t.x, t.y, t.z, 1.0],
            ],
        }
    }

    /// View matrix : look-at, RH. The returned matrix transforms world
    /// space to view space — `eye` is at the view-space origin, `target`
    /// is on the negative-Z axis, `up` is positive-Y (after the
    /// orthogonalization step).
    #[must_use]
    pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        // RH look-at : the forward basis vector points FROM eye TO target,
        // and the view-space negative-Z is forward — so the third row of
        // the view matrix is the negation of (target - eye).normalize().
        let f = (target - eye).normalize(); // world-space forward.
        let r = f.cross(up).normalize(); // world-space right.
        let u = r.cross(f); // re-orthogonalized up.
        // View basis : view-space x = r, view-space y = u, view-space z = -f.
        // The view matrix's rows are these basis vectors ; the translation
        // is `-(R^T * eye)` to land the eye at the origin.
        Self {
            cols: [
                [r.x, u.x, -f.x, 0.0],
                [r.y, u.y, -f.y, 0.0],
                [r.z, u.z, -f.z, 0.0],
                [-r.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
            ],
        }
    }

    /// Transpose of this matrix.
    #[must_use]
    pub const fn transpose(self) -> Self {
        let c = self.cols;
        Self {
            cols: [
                [c[0][0], c[1][0], c[2][0], c[3][0]],
                [c[0][1], c[1][1], c[2][1], c[3][1]],
                [c[0][2], c[1][2], c[2][2], c[3][2]],
                [c[0][3], c[1][3], c[2][3], c[3][3]],
            ],
        }
    }

    /// Determinant via Laplace expansion along the first row. Used by
    /// [`Self::inverse`] but also exposed publicly for users who need
    /// to detect singularity / sign-flip in their own code.
    #[must_use]
    pub fn determinant(self) -> f32 {
        // Build the matrix as 4 row vectors `r0..r3` where `r_i[j]` is
        // the element at row `i`, column `j`. With our column-major
        // storage `cols[col][row]`, `r_i[j] = cols[j][i]`.
        let c = self.cols;
        // Helper : 2x2 minor from 4 elements, columns (a, b) of rows 2 & 3.
        let det2 = |a0: f32, a1: f32, b0: f32, b1: f32| a0 * b1 - a1 * b0;
        // Cofactors of row 0 — each a 3x3 determinant of the lower 3 rows
        // with one column removed.
        let m00 = c[1][1] * det2(c[2][2], c[2][3], c[3][2], c[3][3])
            - c[2][1] * det2(c[1][2], c[1][3], c[3][2], c[3][3])
            + c[3][1] * det2(c[1][2], c[1][3], c[2][2], c[2][3]);
        let m01 = c[0][1] * det2(c[2][2], c[2][3], c[3][2], c[3][3])
            - c[2][1] * det2(c[0][2], c[0][3], c[3][2], c[3][3])
            + c[3][1] * det2(c[0][2], c[0][3], c[2][2], c[2][3]);
        let m02 = c[0][1] * det2(c[1][2], c[1][3], c[3][2], c[3][3])
            - c[1][1] * det2(c[0][2], c[0][3], c[3][2], c[3][3])
            + c[3][1] * det2(c[0][2], c[0][3], c[1][2], c[1][3]);
        let m03 = c[0][1] * det2(c[1][2], c[1][3], c[2][2], c[2][3])
            - c[1][1] * det2(c[0][2], c[0][3], c[2][2], c[2][3])
            + c[2][1] * det2(c[0][2], c[0][3], c[1][2], c[1][3]);
        // Expansion along row 0 with alternating signs.
        c[0][0] * m00 - c[1][0] * m01 + c[2][0] * m02 - c[3][0] * m03
    }

    /// General inverse via cofactor / adjugate. Returns `None` for
    /// singular matrices. For rigid-body transforms (no shear) consider
    /// [`Self::inverse_rigid_scaled`] which is much cheaper.
    ///
    /// Implementation : compute every 3x3 minor explicitly, build the
    /// cofactor matrix, transpose to get the adjugate, and divide by
    /// the determinant. This is the textbook approach — clear, correct,
    /// and the constant-factor overhead is negligible for a 4x4 matrix.
    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        let c = self.cols;
        // Helper : 2x2 determinant.
        let det2 = |a0: f32, a1: f32, b0: f32, b1: f32| a0 * b1 - a1 * b0;
        // Helper : 3x3 determinant of a sub-matrix indexed by row/col
        // exclusions. We compute each 3x3 minor inline below.

        // Build the 16 cofactors. cof_ij = (-1)^(i+j) * det(M_ij) where
        // M_ij is the 3x3 minor obtained by deleting row i and column j.
        // For column-major storage, m[i][j] is element at row j, col i.
        let m = |i: usize, j: usize| c[i][j]; // (row j, col i)

        // The 3 remaining rows and 3 remaining columns after deleting row i, col j.
        // For row deletion : keep rows ∈ {0,1,2,3} \ {i}. For col deletion : keep
        // cols ∈ {0,1,2,3} \ {j}. We hand-roll all 16 minors.

        let cof = |skip_row: usize, skip_col: usize| -> f32 {
            let rows: [usize; 3] = match skip_row {
                0 => [1, 2, 3],
                1 => [0, 2, 3],
                2 => [0, 1, 3],
                _ => [0, 1, 2],
            };
            let cols: [usize; 3] = match skip_col {
                0 => [1, 2, 3],
                1 => [0, 2, 3],
                2 => [0, 1, 3],
                _ => [0, 1, 2],
            };
            // 3x3 determinant via cofactor expansion along the first row.
            let a = m(cols[0], rows[0]);
            let b = m(cols[1], rows[0]);
            let cc = m(cols[2], rows[0]);
            let minor_a = det2(
                m(cols[1], rows[1]),
                m(cols[2], rows[1]),
                m(cols[1], rows[2]),
                m(cols[2], rows[2]),
            );
            let minor_b = det2(
                m(cols[0], rows[1]),
                m(cols[2], rows[1]),
                m(cols[0], rows[2]),
                m(cols[2], rows[2]),
            );
            let minor_c = det2(
                m(cols[0], rows[1]),
                m(cols[1], rows[1]),
                m(cols[0], rows[2]),
                m(cols[1], rows[2]),
            );
            let det3 = a * minor_a - b * minor_b + cc * minor_c;
            // Cofactor sign : (-1)^(skip_row + skip_col).
            if (skip_row + skip_col) % 2 == 0 {
                det3
            } else {
                -det3
            }
        };

        // Determinant : Laplace expansion along row 0.
        let det = m(0, 0) * cof(0, 0) + m(1, 0) * cof(0, 1) + m(2, 0) * cof(0, 2) + m(3, 0) * cof(0, 3);
        if det.abs() < SMALL_EPSILON_F32 {
            return None;
        }
        let inv_det = det.recip();

        // Inverse = adjugate / det. The adjugate is the transpose of
        // the cofactor matrix, so adjugate_(i,j) = cofactor_(j,i).
        // For element (i, j) of the output (row i, column j), in our
        // column-major storage `out.cols[j][i]`, we want cof(j, i)/det.
        let mut out = Self::ZERO;
        for i in 0..4 {
            for j in 0..4 {
                out.cols[j][i] = cof(j, i) * inv_det;
            }
        }
        Some(out)
    }

    /// Fast inverse for a rigid-body transform `T * R * S`. Decomposes
    /// the 3x3 block into rotation × scale, transposes the rotation
    /// (which equals its inverse for orthonormal R), inverts the scale,
    /// and composes the result.
    ///
    /// Caller must guarantee the input is `T * R * S` (no shear). This
    /// is the common case for scene-graph node transforms ; the cost is
    /// O(constant) instead of the determinant + cofactor expansion of
    /// [`Self::inverse`]. Returns `None` if the scale has a near-zero
    /// component (would produce a singular inverse).
    ///
    /// Derivation : let `M = T * R * S` with `S = diag(s)`. The 3x3
    /// linear part is `L = R * S` ; column i of `L` is `s_i * R_col_i`,
    /// length `s_i`. So `R_col_i = M_col_i / s_i` and `L^-1 = diag(1/s)
    /// * R^T`. Element (i, j) of `L^-1` is `(1/s_i) * R^T(i,j) =
    /// (1/s_i) * R(j,i) = (1/s_i) * (M(j,i) / s_i) = M(j,i) / s_i^2`.
    /// In column-major storage `cols(c, r)` is row r, column c, so
    /// `out.cols(j, i) = cols(i, j) / sq_i`. The translation row is
    /// `-L^-1 * t_input`.
    #[must_use]
    pub fn inverse_rigid_scaled(self) -> Option<Self> {
        let c = self.cols;
        let sx_sq = c[0][0] * c[0][0] + c[0][1] * c[0][1] + c[0][2] * c[0][2];
        let sy_sq = c[1][0] * c[1][0] + c[1][1] * c[1][1] + c[1][2] * c[1][2];
        let sz_sq = c[2][0] * c[2][0] + c[2][1] * c[2][1] + c[2][2] * c[2][2];
        if sx_sq < SMALL_EPSILON_F32 || sy_sq < SMALL_EPSILON_F32 || sz_sq < SMALL_EPSILON_F32 {
            return None;
        }
        let inv_sx_sq = sx_sq.recip();
        let inv_sy_sq = sy_sq.recip();
        let inv_sz_sq = sz_sq.recip();
        // L^-1 row i = M^T_row_i / sq_i. Equivalently, output.cols[j][i] = c[i][j] / sq_i.
        // Build L^-1 columns directly : output.cols[j] for j ∈ {0, 1, 2}.
        //   out.cols[0][i] = c[i][0] * inv_sq_i
        //   out.cols[1][i] = c[i][1] * inv_sq_i
        //   out.cols[2][i] = c[i][2] * inv_sq_i
        let l_inv_col0 = [
            c[0][0] * inv_sx_sq,
            c[1][0] * inv_sy_sq,
            c[2][0] * inv_sz_sq,
        ];
        let l_inv_col1 = [
            c[0][1] * inv_sx_sq,
            c[1][1] * inv_sy_sq,
            c[2][1] * inv_sz_sq,
        ];
        let l_inv_col2 = [
            c[0][2] * inv_sx_sq,
            c[1][2] * inv_sy_sq,
            c[2][2] * inv_sz_sq,
        ];
        // Translation : -L^-1 * t.
        let tx = c[3][0];
        let ty = c[3][1];
        let tz = c[3][2];
        let t_inv_x = -(l_inv_col0[0] * tx + l_inv_col1[0] * ty + l_inv_col2[0] * tz);
        let t_inv_y = -(l_inv_col0[1] * tx + l_inv_col1[1] * ty + l_inv_col2[1] * tz);
        let t_inv_z = -(l_inv_col0[2] * tx + l_inv_col1[2] * ty + l_inv_col2[2] * tz);
        Some(Self {
            cols: [
                [l_inv_col0[0], l_inv_col0[1], l_inv_col0[2], 0.0],
                [l_inv_col1[0], l_inv_col1[1], l_inv_col1[2], 0.0],
                [l_inv_col2[0], l_inv_col2[1], l_inv_col2[2], 0.0],
                [t_inv_x, t_inv_y, t_inv_z, 1.0],
            ],
        })
    }

    /// Apply this matrix to a `Vec4`.
    #[must_use]
    pub fn mul_vec4(self, v: Vec4) -> Vec4 {
        let c = self.cols;
        Vec4::new(
            c[0][0].mul_add(
                v.x,
                c[1][0].mul_add(v.y, c[2][0].mul_add(v.z, c[3][0] * v.w)),
            ),
            c[0][1].mul_add(
                v.x,
                c[1][1].mul_add(v.y, c[2][1].mul_add(v.z, c[3][1] * v.w)),
            ),
            c[0][2].mul_add(
                v.x,
                c[1][2].mul_add(v.y, c[2][2].mul_add(v.z, c[3][2] * v.w)),
            ),
            c[0][3].mul_add(
                v.x,
                c[1][3].mul_add(v.y, c[2][3].mul_add(v.z, c[3][3] * v.w)),
            ),
        )
    }

    /// Transform a point (implicit `w = 1`) and return the post-translate
    /// `Vec3`. Convenience wrapper around `mul_vec4(Vec4::from_vec3(v, 1))`.
    #[must_use]
    pub fn transform_point3(self, v: Vec3) -> Vec3 {
        self.mul_vec4(Vec4::from_vec3(v, 1.0)).xyz()
    }

    /// Transform a direction (implicit `w = 0`) — translation has no
    /// effect. Convenience wrapper around `mul_vec4(Vec4::from_vec3(v, 0))`.
    #[must_use]
    pub fn transform_vector3(self, v: Vec3) -> Vec3 {
        self.mul_vec4(Vec4::from_vec3(v, 0.0)).xyz()
    }

    /// Compose left-to-right : `self.compose(rhs)` returns `self * rhs`.
    #[must_use]
    pub fn compose(self, rhs: Self) -> Self {
        let mut out = Self::ZERO;
        for col in 0..4 {
            for row in 0..4 {
                let mut sum = 0.0_f32;
                for k in 0..4 {
                    sum = self.cols[k][row].mul_add(rhs.cols[col][k], sum);
                }
                out.cols[col][row] = sum;
            }
        }
        out
    }
}

impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        self.compose(rhs)
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;
    fn mul(self, rhs: Vec4) -> Vec4 {
        self.mul_vec4(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::Mat4;
    use crate::quat::Quat;
    use crate::vec3::Vec3;
    use crate::vec4::Vec4;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec3_approx(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps) && approx(a.z, b.z, eps)
    }
    fn mat4_approx(a: Mat4, b: Mat4, eps: f32) -> bool {
        for i in 0..4 {
            for j in 0..4 {
                if !approx(a.get(i, j), b.get(i, j), eps) {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn mat4_identity_compose_identity() {
        assert_eq!(Mat4::IDENTITY.compose(Mat4::IDENTITY), Mat4::IDENTITY);
    }

    #[test]
    fn mat4_translation_translates_points() {
        let t = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let v = Vec4::new(1.0, 2.0, 3.0, 1.0);
        assert_eq!(t.mul_vec4(v), Vec4::new(11.0, 22.0, 33.0, 1.0));
    }

    #[test]
    fn mat4_translation_does_not_move_directions() {
        let t = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let v = Vec4::new(1.0, 2.0, 3.0, 0.0);
        assert_eq!(t.mul_vec4(v), Vec4::new(1.0, 2.0, 3.0, 0.0));
    }

    #[test]
    fn mat4_scale_scales_points() {
        let s = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(
            s.mul_vec4(Vec4::new(1.0, 1.0, 1.0, 1.0)),
            Vec4::new(2.0, 3.0, 4.0, 1.0)
        );
    }

    #[test]
    fn mat4_from_rotation_matches_quat_rotate() {
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_3);
        let m = Mat4::from_rotation(q);
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert!(vec3_approx(m.transform_vector3(v), q.rotate(v), 1e-5));
    }

    #[test]
    fn mat4_trs_composes_translation_rotation_scale() {
        let t = Vec3::new(1.0, 2.0, 3.0);
        let r = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let s = Vec3::new(2.0, 2.0, 2.0);
        let m = Mat4::from_translation_rotation_scale(t, r, s);
        // T*R*S applied to e_x should : scale to (2,0,0), rotate to (0,0,-2),
        // translate to (1, 2, 1).
        let v = Vec3::X;
        let out = m.transform_point3(v);
        assert!(vec3_approx(out, Vec3::new(1.0, 2.0, 1.0), 1e-5));
    }

    #[test]
    fn mat4_look_at_rh_eye_at_origin() {
        // eye at +Z, looking at origin, up = Y. Forward in world = -Z.
        let view = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            Vec3::Y,
        );
        // The eye should land at view-space origin.
        let eye_view = view.transform_point3(Vec3::new(0.0, 0.0, 5.0));
        assert!(vec3_approx(eye_view, Vec3::ZERO, 1e-5));
        // The target (world origin) should land on negative-Z in view-space.
        let target_view = view.transform_point3(Vec3::ZERO);
        assert!(target_view.z < 0.0);
        assert!(approx(target_view.z, -5.0, 1e-5));
    }

    #[test]
    fn mat4_transpose_swaps_rows_cols() {
        let m = Mat4::from_cols_array([
            1.0, 2.0, 3.0, 4.0, //
            5.0, 6.0, 7.0, 8.0, //
            9.0, 10.0, 11.0, 12.0, //
            13.0, 14.0, 15.0, 16.0,
        ]);
        let t = m.transpose();
        assert!(approx(t.get(0, 0), 1.0, 0.0));
        assert!(approx(t.get(0, 1), 2.0, 0.0));
        assert!(approx(t.get(1, 0), 5.0, 0.0));
        assert!(approx(t.get(3, 3), 16.0, 0.0));
    }

    #[test]
    fn mat4_determinant_identity_is_one() {
        assert!(approx(Mat4::IDENTITY.determinant(), 1.0, 1e-6));
    }

    #[test]
    fn mat4_determinant_translation_is_one() {
        // Translation has det = 1 (preserves volumes).
        let t = Mat4::from_translation(Vec3::new(7.0, -3.0, 9.0));
        assert!(approx(t.determinant(), 1.0, 1e-5));
    }

    #[test]
    fn mat4_determinant_scale_is_volume_factor() {
        let s = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
        assert!(approx(s.determinant(), 24.0, 1e-4));
    }

    #[test]
    fn mat4_inverse_round_trip_identity() {
        let q = Quat::from_axis_angle(Vec3::new(1.0, 1.0, 0.0).normalize(), 0.7);
        let m = Mat4::from_translation_rotation_scale(
            Vec3::new(1.0, 2.0, 3.0),
            q,
            Vec3::new(2.0, 3.0, 4.0),
        );
        let inv = m.inverse().expect("non-singular");
        let prod = m * inv;
        assert!(mat4_approx(prod, Mat4::IDENTITY, 1e-4));
    }

    #[test]
    fn mat4_inverse_singular_returns_none() {
        // Zero matrix is singular.
        assert_eq!(Mat4::ZERO.inverse(), None);
    }

    #[test]
    fn mat4_inverse_rigid_scaled_matches_general_inverse() {
        // For a TRS matrix the rigid-scaled inverse should equal the
        // general inverse to within float precision.
        let q = Quat::from_axis_angle(Vec3::Y, 0.7);
        let m = Mat4::from_translation_rotation_scale(
            Vec3::new(1.0, 2.0, 3.0),
            q,
            Vec3::new(2.0, 2.0, 2.0),
        );
        let inv_general = m.inverse().expect("non-singular");
        let inv_rigid = m.inverse_rigid_scaled().expect("non-singular");
        assert!(mat4_approx(inv_general, inv_rigid, 1e-4));
    }

    #[test]
    fn mat4_inverse_rigid_scaled_round_trip() {
        let q = Quat::from_axis_angle(Vec3::X, 1.1);
        let m = Mat4::from_translation_rotation_scale(
            Vec3::new(5.0, -3.0, 2.0),
            q,
            Vec3::new(1.5, 1.5, 1.5),
        );
        let inv = m.inverse_rigid_scaled().expect("non-singular");
        let prod = m * inv;
        assert!(mat4_approx(prod, Mat4::IDENTITY, 1e-4));
    }

    #[test]
    fn mat4_inverse_rigid_zero_scale_returns_none() {
        let m = Mat4::from_scale(Vec3::new(0.0, 1.0, 1.0));
        assert!(m.inverse_rigid_scaled().is_none());
    }

    #[test]
    fn mat4_cols_array_round_trip() {
        let arr = [
            1.0, 2.0, 3.0, 4.0, //
            5.0, 6.0, 7.0, 8.0, //
            9.0, 10.0, 11.0, 12.0, //
            13.0, 14.0, 15.0, 16.0,
        ];
        let m = Mat4::from_cols_array(arr);
        assert_eq!(m.to_cols_array(), arr);
    }

    #[test]
    fn mat4_repr_c_layout() {
        // 16 floats * 4 bytes = 64 bytes, aligned to 4.
        assert_eq!(core::mem::size_of::<Mat4>(), 64);
        assert_eq!(core::mem::align_of::<Mat4>(), 4);
    }
}
