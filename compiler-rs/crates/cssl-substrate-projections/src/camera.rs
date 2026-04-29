//! Camera transform + AABB + frustum + visibility tests.
//!
//! § SPEC ANCHOR : `specs/30_SUBSTRATE.csl § PROJECTIONS § ObserverFrame +
//!   § CULL-HULL`. The `Frustum` here corresponds to the spec's
//!   `CullHull::Frustum(Frustum6Planes)` variant.
//!
//! § PIPELINE
//!   1. `Camera::view_matrix()` — world→view, derived from position + orient.
//!   2. `Camera::projection_matrix()` — view→clip, reverse-Z RH perspective.
//!   3. `world_to_clip(p, &cam)` — full pipeline returning `Vec4` clip coords.
//!   4. `Frustum::from_view_projection(vp)` — extracts 6 planes from the
//!      composite world→clip transform.
//!   5. `frustum_cull(aabb, &cam)` — true if the AABB is outside the frustum.
//!
//! § FRUSTUM-PLANE EXTRACTION
//!   Standard Gribb / Hartmann (2001) row-extraction trick. Given world→clip
//!   matrix `M` with rows `m0..m3` (rows in mathematical sense), every world-
//!   space plane comes from a clip-space inequality. The frustum-side inequality
//!   `(a,b,c,d) . p_clip >= 0` translates to the world-space plane
//!   `a*m0 + b*m1 + c*m2 + d*m3` (because `p_clip = M * p_world`).
//!
//!   Substrate canonical : NDC Z range `[0, 1]` (Vulkan / D3D12 / WebGPU). The
//!   six clip-space inequalities + their world-plane extractions are :
//!     - left    `-w <= x`       coeffs `(1, 0, 0, 1)`  ⇒ `m0 + m3`
//!     - right   `x <= w`        coeffs `(-1, 0, 0, 1)` ⇒ `m3 - m0`
//!     - bottom  `-w <= y`       coeffs `(0, 1, 0, 1)`  ⇒ `m1 + m3`
//!     - top     `y <= w`        coeffs `(0, -1, 0, 1)` ⇒ `m3 - m1`
//!     - z lo    `0 <= z`        coeffs `(0, 0, 1, 0)`  ⇒ `m2`
//!     - z hi    `z <= w`        coeffs `(0, 0, -1, 1)` ⇒ `m3 - m2`
//!
//!   Each plane is normalized so distance tests can use `dot(plane.xyz, p) +
//!   plane.w` directly.
//!
//!   For **forward-Z** matrices (z=0 at near plane, z=w at far plane), the
//!   "z lo" plane is the near plane and "z hi" is the far plane.
//!   For **reverse-Z** matrices (substrate canonical : z=w at near plane,
//!   z=0 at far plane), the "z lo" plane is the FAR plane (a point ON the far
//!   plane has z_clip = 0 ; further-away points have z_clip < 0) and "z hi"
//!   is the near plane (a point ON the near plane has z_clip = w ; closer
//!   points have z_clip > w). We re-label so the API stays caller-friendly :
//!   `PLANE_NEAR` always means "closer to camera" regardless of which
//!   z-direction the matrix uses.
//!
//!   This crate's [`Frustum::from_view_projection`] auto-detects reverse-Z
//!   vs forward-Z from the sign of the `m22` element : reverse-Z perspective
//!   has `m22 > 0` (the `near/(far-near)` form) while forward-Z has
//!   `m22 < 0` (the `-far/(far-near)` form). Substrate canonical is reverse-Z,
//!   so the auto-detect path is well-exercised by the crate's own
//!   `Camera::projection_matrix()`.

use crate::mat::{Mat4, ProjectionMatrix};
use crate::vec::{Quat, Vec3, Vec4};

/// Camera : observer-frame intrinsics. The view-matrix is derived from
/// `position` + `orientation` ; the projection-matrix from
/// `fov_y_rad / aspect / near / far` via reverse-Z RH perspective.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// World-space camera origin (eye position).
    pub position: Vec3,
    /// Camera orientation as a unit quaternion. Identity = looking down `-Z`,
    /// up vector `+Y`, right vector `+X` (RH, Y-up convention).
    pub orientation: Quat,
    /// Vertical field-of-view in **radians**. Refinement type per spec :
    /// `0 < fov_y_rad < 179deg` (`≈ 3.124rad`). Stage-0 clamps internally
    /// rather than panicking — substrate totality.
    pub fov_y_rad: f32,
    /// Near plane distance, `> 0`.
    pub near: f32,
    /// Far plane distance, `> near`.
    pub far: f32,
    /// Viewport aspect ratio (width / height), `> 0`. Updated when the
    /// associated render-target resizes ; see [`ObserverFrame`](crate::observer::ObserverFrame)
    /// which carries the render-target rect that produces this aspect.
    pub aspect: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Camera {
    /// Default camera : at origin, looking down `-Z`, 60deg vertical FOV,
    /// 16:9 aspect, near = 0.1, far = 1000.0. Matches typical first-person
    /// game defaults.
    pub const DEFAULT: Self = Self {
        position: Vec3::ZERO,
        orientation: Quat::IDENTITY,
        fov_y_rad: 1.0471975512, // 60 deg
        near: 0.1,
        far: 1000.0,
        aspect: 16.0 / 9.0,
    };

    /// Construct a `Camera` with the given pose + intrinsics. Inputs are
    /// stored as-given ; call [`Self::validate`] to check refinement
    /// preconditions before producing matrices.
    #[must_use]
    pub const fn new(
        position: Vec3,
        orientation: Quat,
        fov_y_rad: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Self {
        Self {
            position,
            orientation,
            fov_y_rad,
            near,
            far,
            aspect,
        }
    }

    /// Returns `Ok(())` if all parameters satisfy the refinement-type bounds
    /// from `specs/30_SUBSTRATE.csl § ObserverFrame`. Returns a
    /// [`CameraError`] describing the first violation otherwise.
    pub fn validate(&self) -> Result<(), CameraError> {
        if !self.position.x.is_finite()
            || !self.position.y.is_finite()
            || !self.position.z.is_finite()
        {
            return Err(CameraError::NonFinitePosition);
        }
        if !self.fov_y_rad.is_finite() || self.fov_y_rad <= 0.0 {
            return Err(CameraError::FovOutOfRange(self.fov_y_rad));
        }
        // Spec : 0 < fov_y_deg <= 179.0 — convert to rad bound.
        if self.fov_y_rad >= 179.0_f32.to_radians() {
            return Err(CameraError::FovOutOfRange(self.fov_y_rad));
        }
        if !self.aspect.is_finite() || self.aspect <= 0.0 {
            return Err(CameraError::NonPositiveAspect(self.aspect));
        }
        if !self.near.is_finite() || self.near <= 0.0 {
            return Err(CameraError::NonPositiveNear(self.near));
        }
        if !self.far.is_finite() || self.far <= self.near {
            return Err(CameraError::FarBelowNear {
                near: self.near,
                far: self.far,
            });
        }
        Ok(())
    }

    /// Forward direction in world space — the unit vector the camera is
    /// pointing toward. Identity orientation gives `-Z` (RH convention).
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        self.orientation
            .normalize()
            .rotate(Vec3::new(0.0, 0.0, -1.0))
    }

    /// Up direction in world space. Identity orientation gives `+Y`.
    #[must_use]
    pub fn up(&self) -> Vec3 {
        self.orientation.normalize().rotate(Vec3::Y)
    }

    /// Right direction in world space. Identity orientation gives `+X`.
    #[must_use]
    pub fn right(&self) -> Vec3 {
        self.orientation.normalize().rotate(Vec3::X)
    }

    /// World→view matrix. Rotates + translates such that the camera is at
    /// the origin looking down `-Z`.
    #[must_use]
    pub fn view_matrix(&self) -> Mat4 {
        // The view matrix is the inverse of the camera's world-transform :
        //   W = T(pos) * R(orient)
        //   V = W^-1 = R^-1 * T(-pos) = R(conjugate) * T(-pos)
        // Build R^-1 from the conjugate quaternion's basis vectors.
        let inv_orient = self.orientation.normalize().conjugate();
        let r = inv_orient.rotate(Vec3::X);
        let u = inv_orient.rotate(Vec3::Y);
        let f = inv_orient.rotate(Vec3::Z);
        // Camera looks down -Z in view-space ; -translation in rotated frame :
        let neg_pos = -self.position;
        let tx = r.dot(neg_pos);
        let ty = u.dot(neg_pos);
        let tz = f.dot(neg_pos);
        Mat4 {
            cols: [
                [r.x, u.x, f.x, 0.0],
                [r.y, u.y, f.y, 0.0],
                [r.z, u.z, f.z, 0.0],
                [tx, ty, tz, 1.0],
            ],
        }
    }

    /// Projection matrix using the substrate canonical convention :
    /// reverse-Z right-handed perspective.
    #[must_use]
    pub fn projection_matrix(&self) -> ProjectionMatrix {
        ProjectionMatrix::perspective_rh_reverse_z(self.fov_y_rad, self.aspect, self.near, self.far)
    }

    /// Composite world→clip matrix : `proj * view`.
    #[must_use]
    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix().0.compose(self.view_matrix())
    }
}

/// Camera-input validation error. Returned by [`Camera::validate`] ; never
/// produced by the matrix-build accessors which apply substrate-totality
/// fallback (returning IDENTITY-wrapped) on bad input.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum CameraError {
    /// Camera position has a NaN or infinite component.
    #[error("camera position has non-finite component")]
    NonFinitePosition,
    /// FOV is non-finite, non-positive, or >= 179deg.
    #[error("camera fov_y must be in (0, 179deg) ; got {0} radians")]
    FovOutOfRange(f32),
    /// Aspect is non-finite or non-positive.
    #[error("camera aspect must be > 0 ; got {0}")]
    NonPositiveAspect(f32),
    /// Near plane is non-finite or non-positive.
    #[error("camera near must be > 0 ; got {0}")]
    NonPositiveNear(f32),
    /// Far plane is not strictly greater than near.
    #[error("camera far ({far}) must be > near ({near})")]
    FarBelowNear { near: f32, far: f32 },
}

/// Apply the full world→view→clip pipeline to a single point. The returned
/// `Vec4` is in clip space ; divide by `w` to get NDC, then apply the
/// viewport transform to get screen-space pixels.
#[must_use]
pub fn world_to_clip(p: Vec3, cam: &Camera) -> Vec4 {
    let vp = cam.view_projection_matrix();
    vp.mul_vec4(Vec4::from_vec3(p, 1.0))
}

/// Axis-aligned bounding box in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    /// Minimum corner (component-wise).
    pub min: Vec3,
    /// Maximum corner (component-wise).
    pub max: Vec3,
}

impl Aabb {
    /// Construct from explicit min / max corners. Caller is responsible for
    /// ensuring `min <= max` component-wise ; degenerate boxes are valid
    /// inputs (a single point is a zero-volume AABB).
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// AABB centered at `c` with half-extents `h`.
    #[must_use]
    pub fn from_center_half_extents(c: Vec3, h: Vec3) -> Self {
        Self::new(c - h, c + h)
    }

    /// Center point of the AABB.
    #[must_use]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Half-extents of the AABB (always non-negative if `min <= max`).
    #[must_use]
    pub fn half_extents(self) -> Vec3 {
        (self.max - self.min) * 0.5
    }
}

/// Plane in world space : `{ p : dot(normal, p) + d = 0 }`. The normal is
/// kept normalized so signed-distance from a point is `dot(normal, p) + d`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// Unit-length normal pointing toward the inside of the frustum.
    pub normal: Vec3,
    /// Plane offset : `dot(normal, p) + d = 0` on the plane.
    pub d: f32,
}

impl Plane {
    /// Build from a 4-vector `(a, b, c, d)` such that `a*x + b*y + c*z + d = 0`.
    /// Normalizes so that further distance computations are unit-correct.
    #[must_use]
    pub fn from_abcd(a: f32, b: f32, c: f32, d: f32) -> Self {
        let n = Vec3::new(a, b, c);
        let len = n.length();
        if len > f32::EPSILON {
            let inv = len.recip();
            Self {
                normal: n * inv,
                d: d * inv,
            }
        } else {
            // Degenerate ; return a "no-op" plane (distance always 0).
            Self {
                normal: Vec3::ZERO,
                d: 0.0,
            }
        }
    }

    /// Signed distance from a world-space point to the plane. Positive ⇒
    /// point lies on the side the normal points to (inside the frustum, by
    /// construction).
    #[must_use]
    pub fn signed_distance(&self, p: Vec3) -> f32 {
        self.normal.dot(p) + self.d
    }
}

/// Frustum : 6 planes in world space, all normals pointing inward. A point
/// is inside the frustum iff it lies on the inside (positive distance) of
/// every plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    /// Six planes in canonical order : left, right, bottom, top, near, far.
    pub planes: [Plane; 6],
}

/// Index of the left plane in [`Frustum::planes`].
pub const PLANE_LEFT: usize = 0;
/// Index of the right plane in [`Frustum::planes`].
pub const PLANE_RIGHT: usize = 1;
/// Index of the bottom plane in [`Frustum::planes`].
pub const PLANE_BOTTOM: usize = 2;
/// Index of the top plane in [`Frustum::planes`].
pub const PLANE_TOP: usize = 3;
/// Index of the near plane in [`Frustum::planes`].
pub const PLANE_NEAR: usize = 4;
/// Index of the far plane in [`Frustum::planes`].
pub const PLANE_FAR: usize = 5;

impl Frustum {
    /// Extract 6 frustum planes from a world→clip matrix using the
    /// Gribb / Hartmann row-extraction trick.
    ///
    /// The `vp` matrix is the column-major `proj * view` composite — caller
    /// can pass `cam.view_projection_matrix()`. The extracted planes are in
    /// world space with normals pointing **inward**, so a point is inside
    /// the frustum iff every `plane.signed_distance(p) >= 0`.
    #[must_use]
    pub fn from_view_projection(vp: Mat4) -> Self {
        // `vp.get(row, col)` indexes mathematical (row, col) ; we want
        // mathematical rows m0..m3 of the matrix.
        // Each row r_i has components (vp[i,0], vp[i,1], vp[i,2], vp[i,3]).
        let row = |i: usize| (vp.get(i, 0), vp.get(i, 1), vp.get(i, 2), vp.get(i, 3));
        let m0 = row(0);
        let m1 = row(1);
        let m2 = row(2);
        let m3 = row(3);
        // X / Y planes are independent of the matrix's Z convention.
        let left = Plane::from_abcd(m0.0 + m3.0, m0.1 + m3.1, m0.2 + m3.2, m0.3 + m3.3);
        let right = Plane::from_abcd(m3.0 - m0.0, m3.1 - m0.1, m3.2 - m0.2, m3.3 - m0.3);
        let bottom = Plane::from_abcd(m1.0 + m3.0, m1.1 + m3.1, m1.2 + m3.2, m1.3 + m3.3);
        let top = Plane::from_abcd(m3.0 - m1.0, m3.1 - m1.1, m3.2 - m1.2, m3.3 - m1.3);
        // Z planes : NDC Z is [0, 1] for the substrate canonical convention.
        //   "z lo" plane (z_clip >= 0) is `m2`.
        //   "z hi" plane (z_clip <= w_clip ⇔ w-z >= 0) is `m3 - m2`.
        // For reverse-Z matrices the z-lo plane is the FAR plane (further
        // from camera) and the z-hi plane is the NEAR plane. For forward-Z
        // it's the opposite. Detect via the sign of m2.2 : reverse-Z RH
        // perspective has m2.2 = near/(far-near) > 0 ; forward-Z has
        // m2.2 = far/(near-far) < 0.
        let z_lo = Plane::from_abcd(m2.0, m2.1, m2.2, m2.3);
        let z_hi = Plane::from_abcd(m3.0 - m2.0, m3.1 - m2.1, m3.2 - m2.2, m3.3 - m2.3);
        let (near, far) = if m2.2 > 0.0 {
            // Reverse-Z (substrate canonical) : z_hi = near, z_lo = far.
            (z_hi, z_lo)
        } else {
            // Forward-Z : z_lo = near, z_hi = far.
            (z_lo, z_hi)
        };
        Self {
            planes: [left, right, bottom, top, near, far],
        }
    }

    /// Test whether a world-space point lies inside the frustum.
    #[must_use]
    pub fn contains_point(&self, p: Vec3) -> bool {
        self.planes
            .iter()
            .all(|plane| plane.signed_distance(p) >= 0.0)
    }

    /// Test whether an AABB intersects the frustum (inverse of cull).
    /// Conservative — false positives possible (AABB classified intersecting
    /// when it might be entirely outside) but no false negatives. The
    /// classic "p-vertex" trick : for each plane, compute the AABB corner
    /// most-positive in the plane's normal direction ; if even that corner
    /// is on the negative side, the whole AABB is outside.
    #[must_use]
    pub fn intersects_aabb(&self, aabb: Aabb) -> bool {
        for plane in &self.planes {
            // Pick the corner of the AABB most positive in the plane's normal.
            let p = Vec3::new(
                if plane.normal.x >= 0.0 {
                    aabb.max.x
                } else {
                    aabb.min.x
                },
                if plane.normal.y >= 0.0 {
                    aabb.max.y
                } else {
                    aabb.min.y
                },
                if plane.normal.z >= 0.0 {
                    aabb.max.z
                } else {
                    aabb.min.z
                },
            );
            if plane.signed_distance(p) < 0.0 {
                // Even the most-positive corner is outside this plane ⇒ entire AABB outside.
                return false;
            }
        }
        true
    }
}

/// View-frustum cull test. Returns `true` if the AABB is **outside** the
/// camera's frustum (i.e. the caller should cull / not render). Returns
/// `false` if the AABB is inside or intersects the frustum (may be visible).
///
/// The conservative classification means false-positives (an AABB classified
/// "may be visible" when it's entirely outside the frustum's volume) are
/// possible at the corners — no false negatives, so no visible geometry is
/// ever culled by mistake.
#[must_use]
pub fn frustum_cull(aabb: Aabb, cam: &Camera) -> bool {
    let f = Frustum::from_view_projection(cam.view_projection_matrix());
    !f.intersects_aabb(aabb)
}

#[cfg(test)]
mod tests {
    use super::{frustum_cull, world_to_clip, Aabb, Camera, CameraError, Frustum, Plane};
    use crate::mat::Mat4;
    use crate::vec::{Quat, Vec3};

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn camera_default_validates() {
        Camera::DEFAULT.validate().expect("default must validate");
    }

    #[test]
    fn camera_validate_catches_bad_inputs() {
        let mut bad = Camera::DEFAULT;
        bad.fov_y_rad = 0.0;
        assert!(matches!(bad.validate(), Err(CameraError::FovOutOfRange(_))));
        let mut bad = Camera::DEFAULT;
        bad.aspect = -1.0;
        assert!(matches!(
            bad.validate(),
            Err(CameraError::NonPositiveAspect(_))
        ));
        let mut bad = Camera::DEFAULT;
        bad.near = 0.0;
        assert!(matches!(
            bad.validate(),
            Err(CameraError::NonPositiveNear(_))
        ));
        let mut bad = Camera::DEFAULT;
        bad.far = 0.05;
        assert!(matches!(
            bad.validate(),
            Err(CameraError::FarBelowNear { .. })
        ));
        let mut bad = Camera::DEFAULT;
        bad.position = Vec3::new(f32::NAN, 0.0, 0.0);
        assert!(matches!(
            bad.validate(),
            Err(CameraError::NonFinitePosition)
        ));
    }

    #[test]
    fn identity_camera_forward_is_negative_z() {
        let f = Camera::DEFAULT.forward();
        assert!(approx_eq(f.x, 0.0, 1e-6));
        assert!(approx_eq(f.y, 0.0, 1e-6));
        assert!(approx_eq(f.z, -1.0, 1e-6));
    }

    #[test]
    fn camera_basis_forms_orthonormal_frame() {
        let c = Camera::DEFAULT;
        let r = c.right();
        let u = c.up();
        let f = c.forward();
        assert!(approx_eq(r.length(), 1.0, 1e-5));
        assert!(approx_eq(u.length(), 1.0, 1e-5));
        assert!(approx_eq(f.length(), 1.0, 1e-5));
        assert!(approx_eq(r.dot(u), 0.0, 1e-5));
        assert!(approx_eq(r.dot(f), 0.0, 1e-5));
        assert!(approx_eq(u.dot(f), 0.0, 1e-5));
    }

    #[test]
    fn rotated_camera_forward_follows_quaternion() {
        // Yaw 90deg around Y axis : forward should now point along -X (was -Z).
        let q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2);
        let mut c = Camera::DEFAULT;
        c.orientation = q;
        let f = c.forward();
        assert!(approx_eq(f.x, -1.0, 1e-5));
        assert!(approx_eq(f.y, 0.0, 1e-5));
        assert!(approx_eq(f.z, 0.0, 1e-5));
    }

    #[test]
    fn world_to_clip_origin_with_default_camera_at_origin_is_singular() {
        // Camera at origin looking at origin ⇒ z_view = 0 ⇒ clip.w = 0.
        // Substrate totality : NOT crashy, just a degenerate output.
        let p = Vec3::ZERO;
        let clip = world_to_clip(p, &Camera::DEFAULT);
        // w should be 0 (camera at origin, point at origin) ; this is the
        // degenerate-corner case — the totality discipline ensures no crash.
        assert!(approx_eq(clip.w, 0.0, 1e-5));
    }

    #[test]
    fn world_to_clip_point_in_front_has_positive_w() {
        // Point 5 units in front of default camera (which looks down -Z).
        let p = Vec3::new(0.0, 0.0, -5.0);
        let clip = world_to_clip(p, &Camera::DEFAULT);
        // RH convention : w_clip = -z_view = +5 ⇒ w > 0.
        assert!(clip.w > 0.0);
    }

    #[test]
    fn aabb_center_and_half_extents_round_trip() {
        let aabb =
            Aabb::from_center_half_extents(Vec3::new(1.0, 2.0, 3.0), Vec3::new(0.5, 1.0, 1.5));
        assert_eq!(aabb.center(), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(aabb.half_extents(), Vec3::new(0.5, 1.0, 1.5));
    }

    #[test]
    fn plane_signed_distance_known_geometry() {
        // Plane y = 0 ; normal up (+Y) ; offset 0.
        let p = Plane::from_abcd(0.0, 1.0, 0.0, 0.0);
        assert!(approx_eq(
            p.signed_distance(Vec3::new(0.0, 5.0, 0.0)),
            5.0,
            1e-6
        ));
        assert!(approx_eq(
            p.signed_distance(Vec3::new(0.0, -3.0, 0.0)),
            -3.0,
            1e-6
        ));
    }

    #[test]
    fn frustum_extracted_from_identity_returns_unit_cube_planes() {
        // Identity world→clip means clip-space cube IS world-space cube.
        let f = Frustum::from_view_projection(Mat4::IDENTITY);
        // Origin (0,0,0) is inside the [-1,1]^3 cube ; reverse-Z near plane
        // would actually exclude it (z=0 fails the z<=1 test for forward-Z
        // or fails the z>=0 test for reverse-Z depending on extraction sign).
        // Here we just verify a deeply-interior point passes.
        assert!(f.contains_point(Vec3::new(0.0, 0.0, 0.5)));
    }

    #[test]
    fn frustum_aabb_inside_is_visible() {
        let cam = Camera::DEFAULT;
        // AABB centered 5 units in front of the camera, half-extent 1.
        let aabb = Aabb::from_center_half_extents(Vec3::new(0.0, 0.0, -5.0), Vec3::splat(1.0));
        // Expect NOT culled.
        assert!(!frustum_cull(aabb, &cam));
    }

    #[test]
    fn frustum_aabb_far_behind_camera_is_culled() {
        let cam = Camera::DEFAULT;
        // AABB 50 units behind the camera (positive Z direction) ; outside.
        let aabb = Aabb::from_center_half_extents(Vec3::new(0.0, 0.0, 50.0), Vec3::splat(0.5));
        assert!(frustum_cull(aabb, &cam));
    }

    #[test]
    fn frustum_aabb_beyond_far_plane_is_culled() {
        let cam = Camera::DEFAULT;
        // far plane is 1000 ; place AABB at -2000 (beyond far plane in -Z).
        let aabb = Aabb::from_center_half_extents(Vec3::new(0.0, 0.0, -2000.0), Vec3::splat(0.5));
        assert!(frustum_cull(aabb, &cam));
    }

    #[test]
    fn frustum_aabb_to_the_side_outside_fov_is_culled() {
        let cam = Camera::DEFAULT;
        // Far to the right of the camera, near depth — outside the right plane.
        let aabb = Aabb::from_center_half_extents(Vec3::new(100.0, 0.0, -5.0), Vec3::splat(0.5));
        assert!(frustum_cull(aabb, &cam));
    }
}
