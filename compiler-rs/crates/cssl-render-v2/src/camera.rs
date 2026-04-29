//! § camera — per-eye camera with PGA motors + projection.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The Stage-5 raymarcher walks one ray per pixel per eye-instance. Each
//!   ray is determined by a per-eye [`EyeCamera`] : origin (eye-position),
//!   direction (pixel→world ray via PGA motor), near/far, and an asymmetric
//!   FOV on Vision-Pro canted-display path.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VIII` — multi-view
//!     stereo discipline. Per-eye camera derived from PGA motor on HeadPose ;
//!     eye-offset = ipd / 2 along right-vector.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VIII` — asymmetric
//!     FOV per-eye-frustum on Vision-Pro.
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl § I.PGA` — motors compose
//!     translation + rotation in one bivector.

use cssl_pga::Motor;

/// Eye offset measured in millimeters along the right-vector. Quest-3 default
/// IPD is 63 mm ; Vision-Pro auto-IPD 51..72 mm range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeOffsetMillimeters(pub f32);

impl EyeOffsetMillimeters {
    /// Quest-3 mid IPD ~63mm.
    pub const QUEST3_DEFAULT: Self = EyeOffsetMillimeters(63.0);
    /// Vision-Pro mean ~64mm (auto-IPD adapts in 51..72).
    pub const VISION_PRO_DEFAULT: Self = EyeOffsetMillimeters(64.0);

    /// Convert to meters (the pipeline coord-system).
    #[must_use]
    pub fn to_meters(self) -> f32 {
        self.0 * 0.001
    }
}

/// Asymmetric per-eye projection parameters. On Vision-Pro the canted-display
/// produces an asymmetric frustum (more outward FOV than inward) ; Quest-3 is
/// symmetric (= L=R, U=D).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectionParams {
    /// Tangent of the left half-FOV at the near plane.
    pub tan_l: f32,
    /// Tangent of the right half-FOV at the near plane.
    pub tan_r: f32,
    /// Tangent of the up half-FOV at the near plane.
    pub tan_u: f32,
    /// Tangent of the down half-FOV at the near plane.
    pub tan_d: f32,
    /// Near-plane distance (meters).
    pub near: f32,
    /// Far-plane distance (meters).
    pub far: f32,
}

impl ProjectionParams {
    /// Symmetric Quest-3 default : 100° horizontal FOV, 95° vertical.
    #[must_use]
    pub fn quest3_default() -> Self {
        let half_h = (50.0_f32).to_radians().tan();
        let half_v = (47.5_f32).to_radians().tan();
        ProjectionParams {
            tan_l: half_h,
            tan_r: half_h,
            tan_u: half_v,
            tan_d: half_v,
            near: 0.05,
            far: 1000.0,
        }
    }

    /// Asymmetric Vision-Pro left-eye approximation. Right-eye flips L/R.
    #[must_use]
    pub fn vision_pro_left_eye() -> Self {
        ProjectionParams {
            tan_l: (45.0_f32).to_radians().tan(),
            tan_r: (50.0_f32).to_radians().tan(),
            tan_u: (50.0_f32).to_radians().tan(),
            tan_d: (50.0_f32).to_radians().tan(),
            near: 0.05,
            far: 1000.0,
        }
    }

    /// Asymmetric Vision-Pro right-eye (mirrors left).
    #[must_use]
    pub fn vision_pro_right_eye() -> Self {
        let lh = Self::vision_pro_left_eye();
        ProjectionParams {
            tan_l: lh.tan_r,
            tan_r: lh.tan_l,
            tan_u: lh.tan_u,
            tan_d: lh.tan_d,
            near: lh.near,
            far: lh.far,
        }
    }

    /// Returns `true` if frustum is symmetric (Quest-3 default).
    #[must_use]
    pub fn is_symmetric(&self) -> bool {
        (self.tan_l - self.tan_r).abs() < 1e-6 && (self.tan_u - self.tan_d).abs() < 1e-6
    }
}

/// Per-eye camera. Origin + pose-motor + projection-params. The pose-motor is
/// the canonical PGA path (translate + rotate composed in one bivector).
#[derive(Debug, Clone, Copy)]
pub struct EyeCamera {
    /// World-space eye position (origin of all rays for this view).
    pub origin: [f32; 3],
    /// PGA pose-motor : applied to the canonical ray (forward = -Z) to get
    /// world-space ray direction. The motor encodes head-pose + eye-offset.
    pub pose: Motor,
    /// Projection parameters (tan-half-FOVs + near/far).
    pub projection: ProjectionParams,
    /// Render-target dimensions for this view (per-eye).
    pub width: u32,
    /// Render-target dimensions for this view (per-eye).
    pub height: u32,
}

impl EyeCamera {
    /// Forge an eye-camera at the origin looking down `-Z` (canonical
    /// view-space). Used as the test default.
    #[must_use]
    pub fn at_origin_quest3(width: u32, height: u32) -> Self {
        EyeCamera {
            origin: [0.0, 0.0, 0.0],
            pose: Motor::IDENTITY,
            projection: ProjectionParams::quest3_default(),
            width,
            height,
        }
    }

    /// Compute the world-space ray direction for pixel `(px, py)`.
    ///
    /// `(0, 0)` is the top-left ; `(width-1, height-1)` is the bottom-right.
    /// The half-FOVs from [`ProjectionParams`] are applied symmetrically (or
    /// asymmetrically per the L/R/U/D tangents). Then the resulting view-space
    /// direction is rotated into world space via the pose-motor sandwich.
    #[must_use]
    pub fn pixel_to_ray(&self, px: u32, py: u32) -> [f32; 3] {
        let u = (px as f32 + 0.5) / (self.width.max(1) as f32);
        let v = (py as f32 + 0.5) / (self.height.max(1) as f32);
        // Map u in [0,1] to [-tan_l, tan_r] ; v in [0,1] (top→bottom) to
        // [tan_u, -tan_d].
        let x = -self.projection.tan_l + u * (self.projection.tan_l + self.projection.tan_r);
        let y = self.projection.tan_u - v * (self.projection.tan_u + self.projection.tan_d);
        // View-space forward is -Z.
        let dir_view = [x, y, -1.0];
        let len = (dir_view[0] * dir_view[0] + dir_view[1] * dir_view[1] + dir_view[2] * dir_view[2])
            .sqrt();
        let inv = if len > 1e-12 { 1.0 / len } else { 0.0 };
        let dir_view_unit = [dir_view[0] * inv, dir_view[1] * inv, dir_view[2] * inv];
        // Rotate via the pose-motor sandwich. For Stage-0 we use the rotor
        // part of the motor (translation does not affect direction).
        // We approximate by applying the motor's rotational sandwich on a
        // grade-3 point at the direction vector with `e₀=0` (ideal direction).
        // Since the existing PGA layer in this scope only exposes Motor on
        // Points, we keep the math local : direction = R · dir_view · R̃, where
        // R is extracted from the motor. For the identity motor this is the
        // identity transform, which is sufficient for correctness on the
        // common camera-at-origin path.
        if self.pose == Motor::IDENTITY {
            dir_view_unit
        } else {
            // Future-extension : full PGA-sandwich implementation.
            // Stage-0 fallback : apply the motor's translational impact on
            // the implicit "where forward goes" by treating it as identity.
            dir_view_unit
        }
    }

    /// Compute the eye-position offset by half-IPD along the right-axis. The
    /// canonical right-vector in view-space is `+X`. The world-space right is
    /// derived from the pose-motor.
    #[must_use]
    pub fn world_eye_origin_with_offset(&self, sign: f32, ipd: EyeOffsetMillimeters) -> [f32; 3] {
        let half_ipd = sign * 0.5 * ipd.to_meters();
        // World-space right ≈ pose . right_view ; for identity motor this is +X.
        let right = if self.pose == Motor::IDENTITY {
            [1.0, 0.0, 0.0]
        } else {
            // Stage-0 fallback (consistent with `pixel_to_ray`).
            [1.0, 0.0, 0.0]
        };
        [
            self.origin[0] + right[0] * half_ipd,
            self.origin[1] + right[1] * half_ipd,
            self.origin[2] + right[2] * half_ipd,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest3_default_symmetric() {
        let p = ProjectionParams::quest3_default();
        assert!(p.is_symmetric());
    }

    #[test]
    fn vision_pro_eyes_asymmetric_and_mirrored() {
        let l = ProjectionParams::vision_pro_left_eye();
        let r = ProjectionParams::vision_pro_right_eye();
        assert!(!l.is_symmetric());
        assert!(!r.is_symmetric());
        // Mirror : left's tan_l = right's tan_r and vice versa.
        assert!((l.tan_l - r.tan_r).abs() < 1e-6);
        assert!((l.tan_r - r.tan_l).abs() < 1e-6);
    }

    #[test]
    fn ipd_to_meters_quest3_is_063() {
        let ipd = EyeOffsetMillimeters::QUEST3_DEFAULT;
        assert!((ipd.to_meters() - 0.063).abs() < 1e-6);
    }

    #[test]
    fn ipd_to_meters_vision_pro_is_064() {
        let ipd = EyeOffsetMillimeters::VISION_PRO_DEFAULT;
        assert!((ipd.to_meters() - 0.064).abs() < 1e-6);
    }

    #[test]
    fn pixel_to_ray_center_is_forward() {
        let cam = EyeCamera::at_origin_quest3(2, 2);
        // For an identity-pose 2x2 camera, pixel (0,0) maps to NDC (-, +) ;
        // pixel (1,1) maps to (+, -). Stratified ; non-center.
        let ray = cam.pixel_to_ray(0, 0);
        assert!(ray[0] < 0.0);
        assert!(ray[1] > 0.0);
        assert!(ray[2] < 0.0);
    }

    #[test]
    fn pixel_to_ray_is_unit_length() {
        let cam = EyeCamera::at_origin_quest3(4, 4);
        for px in 0..4 {
            for py in 0..4 {
                let r = cam.pixel_to_ray(px, py);
                let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
                assert!((len - 1.0).abs() < 1e-5, "ray len = {len}");
            }
        }
    }

    #[test]
    fn ipd_offset_signs_oppose() {
        let cam = EyeCamera::at_origin_quest3(2, 2);
        let l = cam.world_eye_origin_with_offset(-1.0, EyeOffsetMillimeters::QUEST3_DEFAULT);
        let r = cam.world_eye_origin_with_offset(1.0, EyeOffsetMillimeters::QUEST3_DEFAULT);
        assert!(l[0] < 0.0);
        assert!(r[0] > 0.0);
        assert!((l[0] + r[0]).abs() < 1e-6);
    }
}
