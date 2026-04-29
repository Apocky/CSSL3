//! § Mat3 — 3x3 column-major float matrix
//!
//! Used for orthonormal rotation bases (the linear part of a rigid-body
//! transform), inertia tensors, and the inverse-transpose of `Mat4`'s
//! upper-left for normal-vector transforms.
//!
//! § STORAGE — column-major, `cols[col][row]` indexing. `m * v`
//! post-multiplies the column vector. Identical layout convention to
//! [`crate::Mat4`] and `cssl-substrate-projections::Mat4` so transposes
//! never appear at upload-time to the GPU.

use core::ops::Mul;

use crate::quat::Quat;
use crate::scalar::SMALL_EPSILON_F32;
use crate::vec3::Vec3;

/// 3x3 column-major float matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Mat3 {
    /// Column-major storage. `cols[i][j]` is row `j`, column `i`.
    pub cols: [[f32; 3]; 3],
}

impl Default for Mat3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat3 {
    /// 3x3 identity matrix.
    pub const IDENTITY: Self = Self {
        cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    /// 3x3 zero matrix.
    pub const ZERO: Self = Self {
        cols: [[0.0; 3]; 3],
    };

    /// Construct from three column vectors (in column-order).
    #[must_use]
    pub const fn from_cols(c0: Vec3, c1: Vec3, c2: Vec3) -> Self {
        Self {
            cols: [[c0.x, c0.y, c0.z], [c1.x, c1.y, c1.z], [c2.x, c2.y, c2.z]],
        }
    }

    /// Construct from a column-major flat array of 9 floats.
    #[must_use]
    pub const fn from_cols_array(arr: [f32; 9]) -> Self {
        Self {
            cols: [
                [arr[0], arr[1], arr[2]],
                [arr[3], arr[4], arr[5]],
                [arr[6], arr[7], arr[8]],
            ],
        }
    }

    /// Flatten to a column-major `[f32; 9]`.
    #[must_use]
    pub const fn to_cols_array(self) -> [f32; 9] {
        let c = self.cols;
        [
            c[0][0], c[0][1], c[0][2], //
            c[1][0], c[1][1], c[1][2], //
            c[2][0], c[2][1], c[2][2], //
        ]
    }

    /// Indexed read. `(row, col)` order matches mathematical convention.
    #[must_use]
    pub const fn get(self, row: usize, col: usize) -> f32 {
        self.cols[col][row]
    }

    /// Indexed write. `(row, col)` order matches mathematical convention.
    pub fn set(&mut self, row: usize, col: usize, v: f32) {
        self.cols[col][row] = v;
    }

    /// Uniform scale matrix.
    #[must_use]
    pub const fn from_scale(s: Vec3) -> Self {
        Self {
            cols: [[s.x, 0.0, 0.0], [0.0, s.y, 0.0], [0.0, 0.0, s.z]],
        }
    }

    /// Construct from a unit quaternion. The two paths agree :
    /// `Mat3::from_quat(q).mul_vec3(v) == q.rotate(v)` to within float
    /// precision.
    #[must_use]
    pub fn from_quat(q: Quat) -> Self {
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
                [1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy)],
                [2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx)],
                [2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy)],
            ],
        }
    }

    /// Transpose of this matrix.
    #[must_use]
    pub const fn transpose(self) -> Self {
        let c = self.cols;
        Self {
            cols: [
                [c[0][0], c[1][0], c[2][0]],
                [c[0][1], c[1][1], c[2][1]],
                [c[0][2], c[1][2], c[2][2]],
            ],
        }
    }

    /// Determinant via cofactor expansion along the first column.
    #[must_use]
    pub fn determinant(self) -> f32 {
        let c = self.cols;
        c[0][0] * (c[1][1] * c[2][2] - c[1][2] * c[2][1])
            - c[1][0] * (c[0][1] * c[2][2] - c[0][2] * c[2][1])
            + c[2][0] * (c[0][1] * c[1][2] - c[0][2] * c[1][1])
    }

    /// Inverse via cofactor / adjugate. Returns `None` for singular
    /// matrices (determinant near zero).
    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        let c = self.cols;
        // Build the cofactor matrix.
        let m00 = c[1][1] * c[2][2] - c[1][2] * c[2][1];
        let m01 = -(c[0][1] * c[2][2] - c[0][2] * c[2][1]);
        let m02 = c[0][1] * c[1][2] - c[0][2] * c[1][1];

        let m10 = -(c[1][0] * c[2][2] - c[1][2] * c[2][0]);
        let m11 = c[0][0] * c[2][2] - c[0][2] * c[2][0];
        let m12 = -(c[0][0] * c[1][2] - c[0][2] * c[1][0]);

        let m20 = c[1][0] * c[2][1] - c[1][1] * c[2][0];
        let m21 = -(c[0][0] * c[2][1] - c[0][1] * c[2][0]);
        let m22 = c[0][0] * c[1][1] - c[0][1] * c[1][0];

        let det = c[0][0] * m00 + c[1][0] * m01 + c[2][0] * m02;
        if det.abs() < SMALL_EPSILON_F32 {
            return None;
        }
        let inv_det = det.recip();
        // Inverse = adjugate / det = transpose(cofactor) / det.
        Some(Self {
            cols: [
                [m00 * inv_det, m01 * inv_det, m02 * inv_det],
                [m10 * inv_det, m11 * inv_det, m12 * inv_det],
                [m20 * inv_det, m21 * inv_det, m22 * inv_det],
            ],
        })
    }

    /// Apply this matrix to a `Vec3`, returning the transformed vector.
    #[must_use]
    pub fn mul_vec3(self, v: Vec3) -> Vec3 {
        let c = self.cols;
        Vec3::new(
            c[0][0].mul_add(v.x, c[1][0].mul_add(v.y, c[2][0] * v.z)),
            c[0][1].mul_add(v.x, c[1][1].mul_add(v.y, c[2][1] * v.z)),
            c[0][2].mul_add(v.x, c[1][2].mul_add(v.y, c[2][2] * v.z)),
        )
    }

    /// Compose left-to-right : `self.compose(rhs)` returns `self * rhs`.
    #[must_use]
    pub fn compose(self, rhs: Self) -> Self {
        let mut out = Self::ZERO;
        for col in 0..3 {
            for row in 0..3 {
                let mut sum = 0.0_f32;
                for k in 0..3 {
                    sum = self.cols[k][row].mul_add(rhs.cols[col][k], sum);
                }
                out.cols[col][row] = sum;
            }
        }
        out
    }
}

impl Mul for Mat3 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        self.compose(rhs)
    }
}

impl Mul<Vec3> for Mat3 {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Vec3 {
        self.mul_vec3(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::Mat3;
    use crate::quat::Quat;
    use crate::vec3::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec_approx(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps) && approx(a.z, b.z, eps)
    }

    #[test]
    fn mat3_identity_compose_identity() {
        let i = Mat3::IDENTITY;
        assert_eq!(i.compose(i), Mat3::IDENTITY);
    }

    #[test]
    fn mat3_identity_mul_vec3_is_identity() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Mat3::IDENTITY.mul_vec3(v), v);
    }

    #[test]
    fn mat3_scale_scales_vec3() {
        let m = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
        let v = Vec3::new(1.0, 1.0, 1.0);
        assert_eq!(m * v, Vec3::new(2.0, 3.0, 4.0));
    }

    #[test]
    fn mat3_from_quat_matches_quat_rotate() {
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_3);
        let m = Mat3::from_quat(q);
        let v = Vec3::new(1.0, 0.5, 0.25);
        assert!(vec_approx(m * v, q.rotate(v), 1e-5));
    }

    #[test]
    fn mat3_transpose_swaps_rows_and_columns() {
        let m = Mat3::from_cols_array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
        let t = m.transpose();
        assert!(approx(t.get(0, 0), 1.0, 0.0));
        assert!(approx(t.get(0, 1), 2.0, 0.0));
        assert!(approx(t.get(1, 0), 4.0, 0.0));
        assert!(approx(t.get(2, 2), 9.0, 0.0));
    }

    #[test]
    fn mat3_determinant_identity_is_one() {
        assert!(approx(Mat3::IDENTITY.determinant(), 1.0, 1e-6));
    }

    #[test]
    fn mat3_determinant_scale_is_product_of_factors() {
        let m = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
        assert!(approx(m.determinant(), 24.0, 1e-5));
    }

    #[test]
    fn mat3_determinant_known_matrix() {
        // [1 2 3]
        // [0 1 4]   det = 1*(1*0 - 4*5) - 0 + 0 = -20.
        // [5 6 0]
        let m = Mat3::from_cols_array([
            1.0, 0.0, 5.0, // col0
            2.0, 1.0, 6.0, // col1
            3.0, 4.0, 0.0, // col2
        ]);
        assert!(approx(m.determinant(), 1.0, 0.5)); // wait — recompute
                                                    // Actually determinant = 1*(1*0 - 4*6) - 2*(0*0 - 4*5) + 3*(0*6 - 1*5)
                                                    //                     = 1*(-24) - 2*(-20) + 3*(-5)
                                                    //                     = -24 + 40 - 15 = 1.
                                                    // Above assertion is in fact correct (1.0).
    }

    #[test]
    fn mat3_inverse_round_trip_is_identity() {
        let q = Quat::from_axis_angle(Vec3::new(1.0, 1.0, 0.0).normalize(), 0.7);
        let m = Mat3::from_quat(q);
        let inv = m.inverse().expect("rotation matrix is invertible");
        let prod = m * inv;
        // m * m^-1 = I within float precision.
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    approx(prod.get(i, j), expected, 1e-5),
                    "({i}, {j}) = {} expected {expected}",
                    prod.get(i, j)
                );
            }
        }
    }

    #[test]
    fn mat3_inverse_singular_returns_none() {
        // All-zero column ⇒ det = 0 ⇒ singular.
        let m = Mat3::from_cols(Vec3::ZERO, Vec3::Y, Vec3::Z);
        assert_eq!(m.inverse(), None);
    }

    #[test]
    fn mat3_compose_known_matrices() {
        // Identity composes are identity.
        let m = Mat3::from_quat(Quat::from_axis_angle(Vec3::Y, 0.5));
        let i = Mat3::IDENTITY;
        assert_eq!(m * i, m);
        assert_eq!(i * m, m);
    }

    #[test]
    fn mat3_cols_array_round_trip() {
        let arr = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let m = Mat3::from_cols_array(arr);
        assert_eq!(m.to_cols_array(), arr);
    }

    #[test]
    fn mat3_repr_c_layout() {
        // 9 floats * 4 bytes = 36 bytes, aligned to 4.
        assert_eq!(core::mem::size_of::<Mat3>(), 36);
        assert_eq!(core::mem::align_of::<Mat3>(), 4);
    }
}
