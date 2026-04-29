//! 4x4 column-major float matrix + canonical projection-matrix constructors.
//!
//! § SPEC ANCHOR : `specs/30_SUBSTRATE.csl § PROJECTIONS § PERSPECTIVE-TRANSFORMS`.
//!
//! § STORAGE : column-major. `cols[i]` is column `i` ; `cols[i][j]` is the
//!   element at row `j`, column `i`. Matrix-vector product `m * v` post-
//!   multiplies the column vector. This matches Vulkan / WebGPU / GLSL upload
//!   conventions and avoids transpose on host-to-GPU upload.
//!
//! § HANDEDNESS : right-handed, Y-up — the Substrate canonical convention.
//!   Camera looks down `-Z` in view space ; `+X` is right ; `+Y` is up.
//!   Vulkan host requires post-multiplying a Y-flip ; D3D12 host requires
//!   a Y-flip + winding-order swap. Both are host-backend layer concerns.
//!
//! § REVERSE-Z : default. `perspective` puts the near plane at `z = 1.0` in
//!   clip space and the far plane at `z = 0.0`. Combined with a depth buffer
//!   cleared to `0.0` and `GREATER` as the depth comparison, this gives
//!   uniformly-distributed depth precision across the entire frustum, removing
//!   the classic `1/z` precision cliff at the far plane. This matches the
//!   reverse-Z best-practice from id Tech 6 / Frostbite / Unreal 4.18+ /
//!   Unity HDRP. The host backends MUST clear depth to `0.0` and use
//!   `VK_COMPARE_OP_GREATER` / `D3D12_COMPARISON_FUNC_GREATER`.
//!
//! § NDC SPACE : Substrate canonical Z range is `[0, 1]` (Vulkan / D3D12
//!   convention). With reverse-Z, the near plane maps to `z = 1` and the far
//!   plane maps to `z = 0`. OpenGL's `[-1, 1]` Z range is NOT supported
//!   directly — a host-backend wrapper would post-compose the appropriate
//!   remap, but this is outside the substrate-level surface.

use crate::vec::{Vec3, Vec4};

/// 4x4 column-major float matrix.
///
/// Element layout : `cols[col][row]`. Matrix-vector multiply applies as
/// `m * v` (column vectors on the right). To compose `T * R * S` (translate ×
/// rotate × scale, applied to a vector right-to-left), call
/// `t.compose(r).compose(s)` reading left-to-right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// Column-major storage. `cols[i][j]` is the value at row `j`, column `i`.
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

    /// Zero matrix — no row / column has a unit element.
    pub const ZERO: Self = Self {
        cols: [[0.0; 4]; 4],
    };

    /// Construct from a column-major flat array of 16 floats. The slice layout
    /// is the same as Vulkan / GLSL `mat4` upload buffers.
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

    /// Indexed write. `(row, col)` order matches mathematical convention.
    pub fn set(&mut self, row: usize, col: usize, v: f32) {
        self.cols[col][row] = v;
    }

    /// Translation matrix.
    #[must_use]
    pub const fn translation(t: Vec3) -> Self {
        Self {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [t.x, t.y, t.z, 1.0],
            ],
        }
    }

    /// Uniform scale matrix.
    #[must_use]
    pub const fn scale(s: Vec3) -> Self {
        Self {
            cols: [
                [s.x, 0.0, 0.0, 0.0],
                [0.0, s.y, 0.0, 0.0],
                [0.0, 0.0, s.z, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Compose left-to-right : `self.compose(rhs)` returns `self * rhs`. Reading
    /// left-to-right matches the order operations are conceptually applied to
    /// a vector when written `result = m1.compose(m2).compose(m3) * v` —
    /// `m3` first, then `m2`, then `m1` (still mathematically right-to-left ;
    /// `compose` just lets you chain it in source order).
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

    /// Apply this matrix to a `Vec4`, returning the transformed `Vec4`.
    #[must_use]
    pub fn mul_vec4(self, v: Vec4) -> Vec4 {
        Vec4::new(
            self.cols[0][0].mul_add(
                v.x,
                self.cols[1][0].mul_add(v.y, self.cols[2][0].mul_add(v.z, self.cols[3][0] * v.w)),
            ),
            self.cols[0][1].mul_add(
                v.x,
                self.cols[1][1].mul_add(v.y, self.cols[2][1].mul_add(v.z, self.cols[3][1] * v.w)),
            ),
            self.cols[0][2].mul_add(
                v.x,
                self.cols[1][2].mul_add(v.y, self.cols[2][2].mul_add(v.z, self.cols[3][2] * v.w)),
            ),
            self.cols[0][3].mul_add(
                v.x,
                self.cols[1][3].mul_add(v.y, self.cols[2][3].mul_add(v.z, self.cols[3][3] * v.w)),
            ),
        )
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
}

/// Canonical projection matrix newtype. Wraps a `Mat4` so the projection-matrix
/// surface is a distinct type from generic 4x4 transforms — distance / LoD /
/// frustum-extraction routines accept `ProjectionMatrix` specifically.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProjectionMatrix(pub Mat4);

impl ProjectionMatrix {
    /// Reverse-Z right-handed perspective projection.
    ///
    /// Near plane maps to `z = 1.0` in clip space ; far plane maps to `z = 0.0`.
    /// This is the Substrate canonical depth convention — pair with `clear
    /// depth = 0.0` + `GREATER` depth-test on the host.
    ///
    /// # Parameters
    /// - `fov_y_rad` : vertical field-of-view in **radians**. Common values
    ///   are `60deg` (`PI/3`) for cinematic / first-person, `90deg` (`PI/2`)
    ///   for action / VR. Refinement type from spec : `0 < fov_y < 179deg` ;
    ///   stage-0 enforces `> 0` and clamps the upper bound at `179deg` worth
    ///   of radians (about `3.124`).
    /// - `aspect` : viewport `width / height`. Must be `> 0`.
    /// - `near` : near plane distance, `> 0`. Use the smallest distance the
    ///   scene contains ; reverse-Z makes precision largely insensitive to
    ///   this choice.
    /// - `far` : far plane distance, `> near`. With reverse-Z, increasing
    ///   `far` does NOT degrade depth precision (unlike forward-Z).
    ///
    /// # Panics / errors
    /// Returns `Mat4::IDENTITY`-wrapped if any parameter is invalid (NaN,
    /// non-positive aspect, fov out of range, near >= far) — substrate
    /// totality discipline. Callers needing a reject path should validate
    /// inputs upfront via [`Camera::validate`](crate::camera::Camera::validate).
    #[must_use]
    pub fn perspective_rh_reverse_z(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Self {
        if !fov_y_rad.is_finite()
            || !aspect.is_finite()
            || !near.is_finite()
            || !far.is_finite()
            || fov_y_rad <= 0.0
            || aspect <= 0.0
            || near <= 0.0
            || far <= near
        {
            return Self(Mat4::IDENTITY);
        }
        // Clamp fov to (0, 179deg) ≈ (0, 3.1241rad) per spec refinement.
        let fov = fov_y_rad.min(179.0_f32.to_radians());
        let f = 1.0 / (fov * 0.5).tan();
        // Reverse-Z RH derivation (NDC z range [0, 1] ; near→1, far→0) :
        //   we want : z_view = -near  →  z_clip / w_clip = 1
        //             z_view = -far   →  z_clip / w_clip = 0
        //   with w_clip = -z_view (positive in front), set z_clip = A*z_view + B.
        //     at z_view = -near : -A*near + B = 1 * w_clip = near
        //     at z_view = -far  : -A*far  + B = 0 * w_clip = 0
        //   ⇒ B = A*far ; -A*near + A*far = near ⇒ A*(far - near) = near
        //   ⇒ A = near / (far - near) ;  B = near * far / (far - near).
        let a = near / (far - near);
        let b = near * far / (far - near);
        // Column-major build :
        Self(Mat4 {
            cols: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, a, -1.0],
                [0.0, 0.0, b, 0.0],
            ],
        })
    }

    /// Forward-Z right-handed perspective projection. Near plane → `z = 0`,
    /// far plane → `z = 1`. **Not the Substrate default** — supplied for
    /// callers that explicitly need it (e.g. shadow-map passes that already
    /// have a forward-Z baked into their pipeline).
    #[must_use]
    pub fn perspective_rh_forward_z(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Self {
        if !fov_y_rad.is_finite()
            || !aspect.is_finite()
            || !near.is_finite()
            || !far.is_finite()
            || fov_y_rad <= 0.0
            || aspect <= 0.0
            || near <= 0.0
            || far <= near
        {
            return Self(Mat4::IDENTITY);
        }
        let fov = fov_y_rad.min(179.0_f32.to_radians());
        let f = 1.0 / (fov * 0.5).tan();
        // Standard forward-Z RH : z_clip = far/(near-far) * z_view + near*far/(near-far)
        // Column-major mat :
        Self(Mat4 {
            cols: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, far / (near - far), -1.0],
                [0.0, 0.0, near * far / (near - far), 0.0],
            ],
        })
    }

    /// Right-handed orthographic projection with reverse-Z. NDC Z range
    /// `[0, 1]` ; near plane → `z = 1`, far plane → `z = 0`.
    #[must_use]
    pub fn ortho_rh_reverse_z(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Self {
        if !left.is_finite()
            || !right.is_finite()
            || !bottom.is_finite()
            || !top.is_finite()
            || !near.is_finite()
            || !far.is_finite()
            || (right - left).abs() < f32::EPSILON
            || (top - bottom).abs() < f32::EPSILON
            || (far - near).abs() < f32::EPSILON
        {
            return Self(Mat4::IDENTITY);
        }
        let rcp_width = 1.0 / (right - left);
        let rcp_height = 1.0 / (top - bottom);
        let rcp_depth = 1.0 / (far - near);
        // Reverse-Z RH ortho derivation (NDC z range [0, 1] ; near→1, far→0) :
        //   linear : z_clip = A*z_view + B (no perspective divide, w stays 1).
        //     at z_view = -near : -A*near + B = 1
        //     at z_view = -far  : -A*far  + B = 0
        //   ⇒ B = A*far ; -A*near + A*far = 1 ⇒ A = 1/(far-near) = rcp_depth.
        //   ⇒ B = far*rcp_depth.
        Self(Mat4 {
            cols: [
                [2.0 * rcp_width, 0.0, 0.0, 0.0],
                [0.0, 2.0 * rcp_height, 0.0, 0.0],
                [0.0, 0.0, rcp_depth, 0.0],
                [
                    -(right + left) * rcp_width,
                    -(top + bottom) * rcp_height,
                    far * rcp_depth,
                    1.0,
                ],
            ],
        })
    }

    /// Borrow the underlying matrix.
    #[must_use]
    pub const fn as_mat4(self) -> Mat4 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Mat4, ProjectionMatrix};
    use crate::vec::{Vec3, Vec4};

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn identity_compose_identity_is_identity() {
        let i = Mat4::IDENTITY;
        let out = i.compose(i);
        assert_eq!(out, Mat4::IDENTITY);
    }

    #[test]
    fn translation_then_apply_preserves_offset() {
        let t = Mat4::translation(Vec3::new(10.0, 20.0, 30.0));
        let v = Vec4::new(1.0, 2.0, 3.0, 1.0);
        let out = t.mul_vec4(v);
        assert_eq!(out, Vec4::new(11.0, 22.0, 33.0, 1.0));
    }

    #[test]
    fn translation_does_not_move_directions() {
        // w = 0 ⇒ direction vector ⇒ translation has no effect.
        let t = Mat4::translation(Vec3::new(10.0, 20.0, 30.0));
        let v = Vec4::new(1.0, 2.0, 3.0, 0.0);
        let out = t.mul_vec4(v);
        assert_eq!(out, Vec4::new(1.0, 2.0, 3.0, 0.0));
    }

    #[test]
    fn scale_then_apply_scales_point() {
        let s = Mat4::scale(Vec3::new(2.0, 3.0, 4.0));
        let v = Vec4::new(1.0, 1.0, 1.0, 1.0);
        let out = s.mul_vec4(v);
        assert_eq!(out, Vec4::new(2.0, 3.0, 4.0, 1.0));
    }

    #[test]
    fn cols_array_round_trip_preserves_values() {
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
    fn perspective_rh_reverse_z_maps_near_to_one_far_to_zero() {
        let p = ProjectionMatrix::perspective_rh_reverse_z(
            core::f32::consts::FRAC_PI_3,
            16.0 / 9.0,
            0.1,
            100.0,
        );
        // A point AT the near plane (z_view = -near) should clip to (?, ?, 1, near).
        let near_pt = p.0.mul_vec4(Vec4::new(0.0, 0.0, -0.1, 1.0));
        let ndc_z = near_pt.z / near_pt.w;
        assert!(approx_eq(ndc_z, 1.0, 1e-4));
        // A point AT the far plane (z_view = -far) should clip to (?, ?, 0, far).
        let far_pt = p.0.mul_vec4(Vec4::new(0.0, 0.0, -100.0, 1.0));
        let ndc_z = far_pt.z / far_pt.w;
        assert!(approx_eq(ndc_z, 0.0, 1e-4));
    }

    #[test]
    fn perspective_rh_forward_z_maps_near_to_zero_far_to_one() {
        let p = ProjectionMatrix::perspective_rh_forward_z(
            core::f32::consts::FRAC_PI_3,
            16.0 / 9.0,
            0.1,
            100.0,
        );
        let near_pt = p.0.mul_vec4(Vec4::new(0.0, 0.0, -0.1, 1.0));
        let ndc_z = near_pt.z / near_pt.w;
        assert!(approx_eq(ndc_z, 0.0, 1e-4));
        let far_pt = p.0.mul_vec4(Vec4::new(0.0, 0.0, -100.0, 1.0));
        let ndc_z = far_pt.z / far_pt.w;
        assert!(approx_eq(ndc_z, 1.0, 1e-4));
    }

    #[test]
    fn perspective_rejects_invalid_inputs() {
        // Substrate totality : invalid parameters return IDENTITY-wrapped, never NaN.
        let bad = ProjectionMatrix::perspective_rh_reverse_z(0.0, 1.0, 0.1, 100.0);
        assert_eq!(bad.0, Mat4::IDENTITY);
        let bad = ProjectionMatrix::perspective_rh_reverse_z(1.0, 0.0, 0.1, 100.0);
        assert_eq!(bad.0, Mat4::IDENTITY);
        let bad = ProjectionMatrix::perspective_rh_reverse_z(1.0, 1.0, 100.0, 0.1);
        assert_eq!(bad.0, Mat4::IDENTITY);
        let bad = ProjectionMatrix::perspective_rh_reverse_z(f32::NAN, 1.0, 0.1, 100.0);
        assert_eq!(bad.0, Mat4::IDENTITY);
    }

    #[test]
    fn ortho_rh_reverse_z_maps_near_to_one_far_to_zero() {
        let p = ProjectionMatrix::ortho_rh_reverse_z(-1.0, 1.0, -1.0, 1.0, 0.1, 100.0);
        let near_pt = p.0.mul_vec4(Vec4::new(0.0, 0.0, -0.1, 1.0));
        let ndc_z = near_pt.z / near_pt.w;
        assert!(approx_eq(ndc_z, 1.0, 1e-4));
        let far_pt = p.0.mul_vec4(Vec4::new(0.0, 0.0, -100.0, 1.0));
        let ndc_z = far_pt.z / far_pt.w;
        assert!(approx_eq(ndc_z, 0.0, 1e-4));
    }

    #[test]
    fn transpose_swaps_rows_and_columns() {
        let m = Mat4::from_cols_array([
            1.0, 2.0, 3.0, 4.0, //
            5.0, 6.0, 7.0, 8.0, //
            9.0, 10.0, 11.0, 12.0, //
            13.0, 14.0, 15.0, 16.0,
        ]);
        let t = m.transpose();
        assert!(approx_eq(t.get(0, 0), 1.0, 0.0));
        assert!(approx_eq(t.get(0, 1), 2.0, 0.0));
        assert!(approx_eq(t.get(1, 0), 5.0, 0.0));
        assert!(approx_eq(t.get(3, 3), 16.0, 0.0));
    }
}
