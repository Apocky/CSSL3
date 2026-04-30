//! § camera — Camera struct + ray-generation + foveation-mask.
//!
//! ## Role
//! The CFER iterator's render-readout step (spec § 36 § ALGORITHM step 3)
//! generates one ray per pixel, resolves the cell along the ray, and
//! decompresses the cell's KAN-band into a tone-mapped pixel value. The
//! `Camera` here is the per-frame projection state ; the [`FoveationMask`]
//! drives adaptive-detail on Quest 3 / Arc-AVR targets.
//!
//! ## Math
//! Per spec § 36 § PERFORMANCE-TARGETS the foveation cuts peripheral pixels'
//! sample-budget by ~4× ; we model it as a per-pixel `priority ∈ [0,1]` that
//! the evidence-driver multiplies into the per-cell budget.

use thiserror::Error;

/// Error class for camera setup / ray-generation failures.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CameraError {
    /// Resolution is zero in one or both axes.
    #[error("resolution must be non-zero ; got ({0}, {1})")]
    ZeroResolution(u32, u32),
    /// Aspect-ratio is non-positive (camera mis-set).
    #[error("aspect ratio must be > 0 ; got {0}")]
    BadAspect(f32),
    /// Field-of-view out of [1°, 179°] range.
    #[error("fov_y_degrees out of [1,179] ; got {0}")]
    BadFov(f32),
    /// Pixel index out of bounds.
    #[error("pixel ({0},{1}) out of bounds ({2}x{3})")]
    PixelOutOfBounds(u32, u32, u32, u32),
}

/// 3-vector (lightweight ; we don't depend on glam in this crate).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const FORWARD: Self = Self {
        x: 0.0,
        y: 0.0,
        z: -1.0,
    };
    pub const UP: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const RIGHT: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let l = self.length().max(1e-9);
        Self {
            x: self.x / l,
            y: self.y / l,
            z: self.z / l,
        }
    }

    #[inline]
    pub fn cross(self, b: Self) -> Self {
        Self {
            x: self.y * b.z - self.z * b.y,
            y: self.z * b.x - self.x * b.z,
            z: self.x * b.y - self.y * b.x,
        }
    }

    #[inline]
    pub fn dot(self, b: Self) -> f32 {
        self.x * b.x + self.y * b.y + self.z * b.z
    }

    #[inline]
    pub fn add(self, b: Self) -> Self {
        Self {
            x: self.x + b.x,
            y: self.y + b.y,
            z: self.z + b.z,
        }
    }

    #[inline]
    pub fn scale(self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }
}

/// Camera ray.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    /// Wavelength sample (nm) — used by `decompress_kan_band` to sample the
    /// per-cell KAN-band at a specific frequency.
    pub wavelength_nm: f32,
}

/// Camera : pinhole projection + foveation-mask.
#[derive(Debug, Clone)]
pub struct Camera {
    /// World-space position of the eye.
    pub position: Vec3,
    /// World-space forward direction (normalized).
    pub forward: Vec3,
    /// World-space up direction (normalized).
    pub up: Vec3,
    /// Vertical field of view in degrees.
    pub fov_y_degrees: f32,
    /// Render width in pixels.
    pub width: u32,
    /// Render height in pixels.
    pub height: u32,
    /// Scalar exposure for the tonemap pass.
    pub exposure: f32,
    /// Sampling-wavelength for default rays (nm). Visible-mid = 555nm.
    pub default_wavelength_nm: f32,
    /// Foveation mask (None = uniform full-detail).
    pub foveation: Option<FoveationMask>,
}

impl Camera {
    /// Construct a default-pinhole camera at origin facing -Z.
    pub fn new(width: u32, height: u32) -> Result<Self, CameraError> {
        if width == 0 || height == 0 {
            return Err(CameraError::ZeroResolution(width, height));
        }
        Ok(Self {
            position: Vec3::ZERO,
            forward: Vec3::FORWARD,
            up: Vec3::UP,
            fov_y_degrees: 60.0,
            width,
            height,
            exposure: 1.0,
            default_wavelength_nm: 555.0,
            foveation: None,
        })
    }

    /// Aspect ratio (width / height).
    pub fn aspect(&self) -> f32 {
        (self.width as f32) / (self.height as f32)
    }

    /// Validate the camera state. Returns Ok(()) when ready for rendering.
    pub fn validate(&self) -> Result<(), CameraError> {
        if self.width == 0 || self.height == 0 {
            return Err(CameraError::ZeroResolution(self.width, self.height));
        }
        let asp = self.aspect();
        if asp <= 0.0 {
            return Err(CameraError::BadAspect(asp));
        }
        if !(1.0..=179.0).contains(&self.fov_y_degrees) {
            return Err(CameraError::BadFov(self.fov_y_degrees));
        }
        Ok(())
    }

    /// Generate a ray through the (px, py) pixel center.
    pub fn ray_through(&self, px: u32, py: u32) -> Result<Ray, CameraError> {
        if px >= self.width || py >= self.height {
            return Err(CameraError::PixelOutOfBounds(
                px, py, self.width, self.height,
            ));
        }
        let aspect = self.aspect();
        let fov_rad = self.fov_y_degrees.to_radians();
        let half_h = (fov_rad * 0.5).tan();
        let half_w = half_h * aspect;

        // pixel center in NDC : (-1, +1) along x, (+1, -1) along y (image-space).
        let u = (((px as f32) + 0.5) / (self.width as f32)) * 2.0 - 1.0;
        let v = 1.0 - (((py as f32) + 0.5) / (self.height as f32)) * 2.0;

        let f = self.forward.normalize();
        let r = f.cross(self.up).normalize();
        let upn = r.cross(f).normalize();

        let dir = f.add(r.scale(u * half_w)).add(upn.scale(v * half_h));
        Ok(Ray {
            origin: self.position,
            direction: dir.normalize(),
            wavelength_nm: self.default_wavelength_nm,
        })
    }

    /// Per-pixel priority from the foveation-mask in [0,1].
    pub fn priority_at(&self, px: u32, py: u32) -> f32 {
        match &self.foveation {
            None => 1.0,
            Some(mask) => mask.priority_at(px, py, self.width, self.height),
        }
    }
}

/// Foveation mask : radial Gaussian peaked at the gaze-center.
///
/// Per spec § 36 § PERFORMANCE-TARGETS aggressive foveation cuts peripheral
/// sample-budget. The mask is a per-pixel [0,1] priority that the
/// evidence-driver multiplies into the cell-iteration budget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoveationMask {
    /// Gaze-center NDC x ∈ [-1,1] ; 0 = screen-center.
    pub gaze_x_ndc: f32,
    /// Gaze-center NDC y ∈ [-1,1] ; 0 = screen-center.
    pub gaze_y_ndc: f32,
    /// Foveal radius in NDC units (≈ 0.2 = 20% of half-screen).
    pub foveal_radius: f32,
    /// Peripheral floor priority (0.05 = 5% of foveal-rate).
    pub peripheral_floor: f32,
}

impl FoveationMask {
    /// Default Quest-3-like fovea : center, 20% radius, 25% peripheral floor.
    pub const QUEST_3: Self = Self {
        gaze_x_ndc: 0.0,
        gaze_y_ndc: 0.0,
        foveal_radius: 0.2,
        peripheral_floor: 0.25,
    };

    /// Per-pixel priority.
    pub fn priority_at(&self, px: u32, py: u32, w: u32, h: u32) -> f32 {
        let u = ((px as f32) + 0.5) / (w as f32) * 2.0 - 1.0;
        let v = 1.0 - ((py as f32) + 0.5) / (h as f32) * 2.0;
        let dx = u - self.gaze_x_ndc;
        let dy = v - self.gaze_y_ndc;
        let r = (dx * dx + dy * dy).sqrt();
        let sigma = self.foveal_radius.max(1e-3);
        let g = (-(r * r) / (2.0 * sigma * sigma)).exp();
        // map g ∈ (0,1] to priority ∈ [floor, 1]
        self.peripheral_floor + (1.0 - self.peripheral_floor) * g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_default_validates() {
        let c = Camera::new(640, 480).unwrap();
        assert!(c.validate().is_ok());
        assert_eq!(c.width, 640);
        assert_eq!(c.height, 480);
    }

    #[test]
    fn camera_zero_res_errors() {
        let c = Camera::new(0, 480);
        assert!(matches!(c, Err(CameraError::ZeroResolution(0, 480))));
    }

    #[test]
    fn camera_validate_bad_fov() {
        let mut c = Camera::new(640, 480).unwrap();
        c.fov_y_degrees = 200.0;
        assert!(matches!(c.validate(), Err(CameraError::BadFov(_))));
    }

    #[test]
    fn ray_center_pixel_points_forward() {
        let c = Camera::new(640, 480).unwrap();
        let cx = 320;
        let cy = 240;
        let r = c.ray_through(cx, cy).unwrap();
        // Forward is -Z ; center pixel ray ≈ -Z (within sub-pixel jitter).
        assert!(r.direction.z < 0.0);
        // Magnitude is unit.
        assert!((r.direction.length() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn ray_left_pixel_points_left() {
        let c = Camera::new(640, 480).unwrap();
        let r = c.ray_through(0, 240).unwrap();
        // Left edge → ray direction has -X component.
        assert!(r.direction.x < 0.0);
    }

    #[test]
    fn ray_oob_errors() {
        let c = Camera::new(64, 64).unwrap();
        assert!(c.ray_through(64, 0).is_err());
        assert!(c.ray_through(0, 64).is_err());
    }

    #[test]
    fn foveation_center_full_priority() {
        let mask = FoveationMask::QUEST_3;
        let p = mask.priority_at(320, 240, 640, 480);
        assert!(p > 0.99);
    }

    #[test]
    fn foveation_corner_floor_priority() {
        let mask = FoveationMask::QUEST_3;
        let p = mask.priority_at(0, 0, 640, 480);
        // Corner is far from center ; should be ≈ peripheral_floor.
        assert!(p < 0.5);
        assert!(p >= mask.peripheral_floor);
    }

    #[test]
    fn camera_priority_uniform_when_no_foveation() {
        let c = Camera::new(64, 64).unwrap();
        assert_eq!(c.priority_at(0, 0), 1.0);
        assert_eq!(c.priority_at(63, 63), 1.0);
    }
}
