//! `FoveaMask` : 2D screen-space density-mask (full-detail center, coarse
//! periphery).
//!
//! § DESIGN
//!   The FoveaMask is a per-eye 2D bitmap classifying every pixel into one of
//!   three [`FoveaResolution`] classes : `Full`, `Half`, `Quarter`. The
//!   classification is anchored at the gaze-projected screen-space point
//!   with a Gaussian density-falloff modeled on actual human visual acuity :
//!     - foveal cone (5° full-acuity)         → Full
//!     - para-foveal cone (15° mid-acuity)    → Half (2×2 shading-rate)
//!     - peripheral cone (>30° low-acuity)    → Quarter (4×4 shading-rate)
//!   Per `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § STAGE 2.compute`.
//!
//!   The mask format matches the canonical Stage-2 output : VRS Tier-2 / FDM
//!   / Metal-dynamic-render-quality. Each cell is one of the three classes
//!   and the downstream stages (5 raymarch, 6 BRDF, 7 amplifier) consume the
//!   mask to throttle their per-pixel work-rate.
//!
//! § COMPUTATION
//!   The angular cones project onto the render-target via the camera
//!   field-of-view. For Quest-3 (~110° horizontal FOV per-eye) at
//!   1832×1920 per-eye, 5° foveal projects to ~83 px radius, 15° to
//!   ~250 px, 30° to ~500 px (linear approximation good enough for VRS).
//!
//! § DETERMINISM
//!   `FoveaMask::compute` is pure : same gaze + same FOV → same mask.
//!   No RNG, no allocation outside the initial buffer.
//!
//! § Σ-MASK-AWARENESS
//!   When the gaze ray-cast resolves into a Σ-private cell (per
//!   `cssl_substrate_prime_directive::SigmaMaskPacked`), the FoveaMask
//!   computation falls back to the previous-frame anchor rather than
//!   committing to the new gaze. This is checked by the caller before
//!   invoking [`FoveaMask::compute_at`] — see [`crate::pass`].

use crate::config::GazeCollapseConfig;
use crate::error::GazeCollapseError;
use crate::gaze_input::GazeInput;

/// Per-pixel resolution-class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FoveaResolution {
    /// Full-detail (1×1 shading-rate).
    Full,
    /// Half-detail (2×2 shading-rate).
    Half,
    /// Quarter-detail (4×4 shading-rate).
    Quarter,
}

impl FoveaResolution {
    /// Linear coarseness factor (1.0 = full, 0.5 = half, 0.25 = quarter).
    #[must_use]
    pub const fn coarseness(self) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::Half => 0.5,
            Self::Quarter => 0.25,
        }
    }

    /// Pack into a u8 wire-format : 0 = Full, 1 = Half, 2 = Quarter.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Full => 0,
            Self::Half => 1,
            Self::Quarter => 2,
        }
    }

    /// Decode from u8 wire-format ; unknown values clamp to Quarter (most
    /// conservative — peripheral resolution always safe).
    #[must_use]
    pub const fn from_u8(b: u8) -> Self {
        match b {
            0 => Self::Full,
            1 => Self::Half,
            _ => Self::Quarter,
        }
    }
}

/// 2D NxM shading-rate ratio — alias for the canonical wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadingRate {
    /// Horizontal shading-rate (1, 2, 4).
    pub x: u8,
    /// Vertical shading-rate (1, 2, 4).
    pub y: u8,
}

impl ShadingRate {
    /// Shading-rate from a FoveaResolution.
    #[must_use]
    pub const fn from_resolution(res: FoveaResolution) -> Self {
        match res {
            FoveaResolution::Full => Self { x: 1, y: 1 },
            FoveaResolution::Half => Self { x: 2, y: 2 },
            FoveaResolution::Quarter => Self { x: 4, y: 4 },
        }
    }
}

/// A logical region in the FoveaMask described by its gaze-anchor + cone
/// half-angle + resolution-class. Used by [`crate::ObservationCollapseEvolver`]
/// to detect peripheral-→-foveal transitions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoveaRegion {
    /// Anchor pixel-x (screen-space).
    pub anchor_x: i32,
    /// Anchor pixel-y (screen-space).
    pub anchor_y: i32,
    /// Cone half-angle in degrees.
    pub half_angle_deg: f32,
    /// Resolution-class for this region.
    pub resolution: FoveaResolution,
}

/// Per-eye FoveaMask : a flat row-major buffer of `FoveaResolution` values.
#[derive(Debug, Clone)]
pub struct FoveaMask {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major buffer of resolution-classes (length = width × height).
    /// Stored as `Vec<u8>` to match the canonical wire-format directly.
    pub data: Vec<u8>,
    /// Anchor used to compute this mask (for diagnostic-overlay).
    pub anchor: (i32, i32),
}

impl FoveaMask {
    /// Construct a mask with all cells set to `Quarter` (the safe default —
    /// uniform peripheral coarseness, no foveation focus).
    #[must_use]
    pub fn quarter_uniform(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![FoveaResolution::Quarter.as_u8(); (width * height) as usize],
            anchor: ((width / 2) as i32, (height / 2) as i32),
        }
    }

    /// Construct a center-bias mask : foveal-region anchored at screen-center
    /// with the standard cone-angle decomposition. This is the spec-canonical
    /// fallback per V.4(d) when consent is denied or hardware fails.
    #[must_use]
    pub fn center_bias(config: &GazeCollapseConfig) -> Self {
        let cx = (config.render_target_width / 2) as i32;
        let cy = (config.render_target_height / 2) as i32;
        Self::compute_at_anchor(cx, cy, config)
    }

    /// Compute the FoveaMask given a gaze input + config.
    ///
    /// Projects the gaze direction onto the screen-space anchor via the
    /// configured FOV, then applies the cone-angle decomposition.
    pub fn compute(
        input: &GazeInput,
        config: &GazeCollapseConfig,
    ) -> Result<Self, GazeCollapseError> {
        config.validate()?;
        let (anchor_x, anchor_y) = project_gaze_to_screen(input, config);
        Ok(Self::compute_at_anchor(anchor_x, anchor_y, config))
    }

    /// Compute at an explicit anchor (used by center-bias-fallback +
    /// last-known-gaze fallback).
    #[must_use]
    pub fn compute_at_anchor(anchor_x: i32, anchor_y: i32, config: &GazeCollapseConfig) -> Self {
        let w = config.render_target_width;
        let h = config.render_target_height;
        let foveal_radius =
            compute_pixel_radius_for_angle(FOVEAL_HALF_ANGLE_DEG, w, DEFAULT_HORIZONTAL_FOV_DEG);
        let para_radius = compute_pixel_radius_for_angle(
            PARA_FOVEAL_HALF_ANGLE_DEG,
            w,
            DEFAULT_HORIZONTAL_FOV_DEG,
        );
        let mut data = vec![FoveaResolution::Quarter.as_u8(); (w * h) as usize];
        // Iterate in row-major order ; for each pixel compute distance²
        // from the anchor and classify.
        let foveal_r2 = (foveal_radius * foveal_radius) as i64;
        let para_r2 = (para_radius * para_radius) as i64;
        for y in 0..(h as i32) {
            let dy = y - anchor_y;
            let dy2 = (dy as i64) * (dy as i64);
            for x in 0..(w as i32) {
                let dx = x - anchor_x;
                let dx2 = (dx as i64) * (dx as i64);
                let d2 = dx2 + dy2;
                let res = if d2 <= foveal_r2 {
                    FoveaResolution::Full
                } else if d2 <= para_r2 {
                    FoveaResolution::Half
                } else {
                    FoveaResolution::Quarter
                };
                let idx = (y as u32 * w + x as u32) as usize;
                data[idx] = res.as_u8();
            }
        }
        Self {
            width: w,
            height: h,
            data,
            anchor: (anchor_x, anchor_y),
        }
    }

    /// Read resolution-class at pixel (x, y).
    ///
    /// Returns `None` if (x, y) is outside the mask.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<FoveaResolution> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y * self.width + x) as usize;
        Some(FoveaResolution::from_u8(self.data[idx]))
    }

    /// Total foveal-pixel count (for budget-pulldown decisions).
    #[must_use]
    pub fn foveal_pixel_count(&self) -> u32 {
        self.data
            .iter()
            .filter(|&&b| b == FoveaResolution::Full.as_u8())
            .count() as u32
    }

    /// Total para-foveal-pixel count.
    #[must_use]
    pub fn para_foveal_pixel_count(&self) -> u32 {
        self.data
            .iter()
            .filter(|&&b| b == FoveaResolution::Half.as_u8())
            .count() as u32
    }

    /// Total peripheral-pixel count.
    #[must_use]
    pub fn peripheral_pixel_count(&self) -> u32 {
        self.data
            .iter()
            .filter(|&&b| b == FoveaResolution::Quarter.as_u8())
            .count() as u32
    }

    /// Decompose the mask into the three logical regions (foveal,
    /// para-foveal, peripheral) for the collapse-evolver to test
    /// peripheral-→-foveal transitions on.
    #[must_use]
    pub fn regions(&self) -> [FoveaRegion; 3] {
        [
            FoveaRegion {
                anchor_x: self.anchor.0,
                anchor_y: self.anchor.1,
                half_angle_deg: FOVEAL_HALF_ANGLE_DEG,
                resolution: FoveaResolution::Full,
            },
            FoveaRegion {
                anchor_x: self.anchor.0,
                anchor_y: self.anchor.1,
                half_angle_deg: PARA_FOVEAL_HALF_ANGLE_DEG,
                resolution: FoveaResolution::Half,
            },
            FoveaRegion {
                anchor_x: self.anchor.0,
                anchor_y: self.anchor.1,
                half_angle_deg: 90.0,
                resolution: FoveaResolution::Quarter,
            },
        ]
    }

    /// Apply diagnostic-overlay : draw a 1-pixel-thick foveal-circle outline
    /// to the mask's debug-channel. The drawn pixels are returned as a
    /// separate vec of (x, y) coordinates so the caller's render-target
    /// can composite them on top.
    #[must_use]
    pub fn diagnostic_circle_pixels(&self) -> Vec<(u32, u32)> {
        let foveal_radius = compute_pixel_radius_for_angle(
            FOVEAL_HALF_ANGLE_DEG,
            self.width,
            DEFAULT_HORIZONTAL_FOV_DEG,
        );
        bresenham_circle_pixels(
            self.anchor.0,
            self.anchor.1,
            foveal_radius,
            self.width,
            self.height,
        )
    }
}

/// Foveal half-angle in degrees per V.4 spec (5°).
pub const FOVEAL_HALF_ANGLE_DEG: f32 = 5.0;
/// Para-foveal half-angle in degrees per V.4 spec (15°).
pub const PARA_FOVEAL_HALF_ANGLE_DEG: f32 = 15.0;
/// Default horizontal FOV (degrees) — Quest-3 per-eye monocular.
pub const DEFAULT_HORIZONTAL_FOV_DEG: f32 = 110.0;

/// Convert a cone half-angle (deg) into a pixel radius given screen-width +
/// horizontal-FOV.
fn compute_pixel_radius_for_angle(half_angle_deg: f32, width: u32, fov_deg: f32) -> u32 {
    if fov_deg <= 0.0 || width == 0 {
        return 0;
    }
    let ratio = half_angle_deg / (fov_deg * 0.5);
    let pixels = ratio * (width as f32 * 0.5);
    pixels.max(0.0) as u32
}

/// Project a gaze input into screen-space pixel coordinates.
///
/// The cyclopean direction is treated as the dominant axis ; we project the
/// (x, y) components onto the screen plane assuming the head's forward axis
/// passes through screen-center.
fn project_gaze_to_screen(input: &GazeInput, config: &GazeCollapseConfig) -> (i32, i32) {
    let dir = input.cyclopean_direction();
    let cx = (config.render_target_width / 2) as i32;
    let cy = (config.render_target_height / 2) as i32;
    // Avoid singular projection : if z is near-zero the user is looking 90° off-axis.
    if dir.z.abs() < 0.05 {
        return (cx, cy);
    }
    // Tangent-space projection : x_screen = (dir.x / dir.z) × (W / 2) / tan(fov/2).
    let half_fov_tan = (DEFAULT_HORIZONTAL_FOV_DEG * 0.5).to_radians().tan();
    let x_off = (dir.x / dir.z) * (config.render_target_width as f32 * 0.5) / half_fov_tan;
    // Aspect-ratio aware vertical projection.
    let aspect = config.render_target_height as f32 / config.render_target_width as f32;
    let y_off =
        -(dir.y / dir.z) * (config.render_target_height as f32 * 0.5) / (half_fov_tan * aspect);
    let x = cx + x_off as i32;
    let y = cy + y_off as i32;
    (
        x.clamp(0, config.render_target_width as i32 - 1),
        y.clamp(0, config.render_target_height as i32 - 1),
    )
}

/// Bresenham-circle outline pixel generation for diagnostic-overlay.
fn bresenham_circle_pixels(cx: i32, cy: i32, radius: u32, w: u32, h: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    if radius == 0 {
        return out;
    }
    let r = radius as i32;
    let mut x = r;
    let mut y = 0i32;
    let mut decision = 1 - x;
    while x >= y {
        for &(dx, dy) in &[
            (x, y),
            (-x, y),
            (x, -y),
            (-x, -y),
            (y, x),
            (-y, x),
            (y, -x),
            (-y, -x),
        ] {
            let px = cx + dx;
            let py = cy + dy;
            if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                out.push((px as u32, py as u32));
            }
        }
        y += 1;
        if decision <= 0 {
            decision += 2 * y + 1;
        } else {
            x -= 1;
            decision += 2 * (y - x) + 1;
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::{
        compute_pixel_radius_for_angle, FoveaMask, FoveaRegion, FoveaResolution, ShadingRate,
        DEFAULT_HORIZONTAL_FOV_DEG, FOVEAL_HALF_ANGLE_DEG, PARA_FOVEAL_HALF_ANGLE_DEG,
    };
    use crate::config::GazeCollapseConfig;
    use crate::gaze_input::GazeInput;

    fn small_config() -> GazeCollapseConfig {
        let mut cfg = GazeCollapseConfig::quest3_opted_in();
        cfg.render_target_width = 256;
        cfg.render_target_height = 256;
        cfg
    }

    #[test]
    fn fovea_resolution_coarseness_matches_spec() {
        assert!((FoveaResolution::Full.coarseness() - 1.0).abs() < 1e-6);
        assert!((FoveaResolution::Half.coarseness() - 0.5).abs() < 1e-6);
        assert!((FoveaResolution::Quarter.coarseness() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn fovea_resolution_u8_roundtrip() {
        for res in [
            FoveaResolution::Full,
            FoveaResolution::Half,
            FoveaResolution::Quarter,
        ] {
            assert_eq!(FoveaResolution::from_u8(res.as_u8()), res);
        }
    }

    #[test]
    fn fovea_resolution_unknown_u8_clamps_to_quarter() {
        for b in [3u8, 4, 99, 255] {
            assert_eq!(FoveaResolution::from_u8(b), FoveaResolution::Quarter);
        }
    }

    #[test]
    fn shading_rate_from_resolution_correct() {
        assert_eq!(
            ShadingRate::from_resolution(FoveaResolution::Full),
            ShadingRate { x: 1, y: 1 }
        );
        assert_eq!(
            ShadingRate::from_resolution(FoveaResolution::Half),
            ShadingRate { x: 2, y: 2 }
        );
        assert_eq!(
            ShadingRate::from_resolution(FoveaResolution::Quarter),
            ShadingRate { x: 4, y: 4 }
        );
    }

    #[test]
    fn pixel_radius_for_angle_matches_proportional() {
        let r5 = compute_pixel_radius_for_angle(5.0, 1832, 110.0);
        let r15 = compute_pixel_radius_for_angle(15.0, 1832, 110.0);
        // 15° should be ~3× wider than 5°
        assert!(r15 > r5 * 2);
        assert!(r15 < r5 * 4);
        // Sanity-check scale : at 1832 px / 110° FOV, 5° ≈ 83 px.
        assert!((50..=120).contains(&(r5 as i32)), "got r5={}", r5);
    }

    #[test]
    fn pixel_radius_zero_for_zero_fov() {
        assert_eq!(compute_pixel_radius_for_angle(5.0, 1024, 0.0), 0);
    }

    #[test]
    fn pixel_radius_zero_for_zero_width() {
        assert_eq!(compute_pixel_radius_for_angle(5.0, 0, 110.0), 0);
    }

    #[test]
    fn quarter_uniform_is_all_quarter() {
        let m = FoveaMask::quarter_uniform(64, 64);
        assert_eq!(m.foveal_pixel_count(), 0);
        assert_eq!(m.para_foveal_pixel_count(), 0);
        assert_eq!(m.peripheral_pixel_count(), 64 * 64);
    }

    #[test]
    fn center_bias_anchor_at_center() {
        let cfg = small_config();
        let m = FoveaMask::center_bias(&cfg);
        assert_eq!(m.anchor, (128, 128));
        // Center pixel must be foveal.
        assert_eq!(m.get(128, 128), Some(FoveaResolution::Full));
        // Far corner must be peripheral.
        assert_eq!(m.get(0, 0), Some(FoveaResolution::Quarter));
    }

    #[test]
    fn compute_with_forward_gaze_is_centered() {
        let cfg = small_config();
        let input = GazeInput::center_bias_fallback(0);
        let mask = FoveaMask::compute(&input, &cfg).unwrap();
        // Forward gaze → anchor at center.
        assert_eq!(mask.anchor, (128, 128));
        assert_eq!(mask.get(128, 128), Some(FoveaResolution::Full));
    }

    #[test]
    fn compute_off_center_gaze_shifts_anchor() {
        let cfg = small_config();
        let mut input = GazeInput::center_bias_fallback(0);
        // Right + slightly down (in head-relative space).
        let s = (1.0_f32 / 3.0).sqrt();
        input.left_direction = crate::gaze_input::GazeDirection::new(s, -s, s).unwrap();
        input.right_direction = input.left_direction;
        let mask = FoveaMask::compute(&input, &cfg).unwrap();
        // Anchor should NOT be at center.
        assert_ne!(mask.anchor, (128, 128));
        // Anchor should be right-of-center + below-center.
        assert!(mask.anchor.0 > 128, "anchor.x = {}", mask.anchor.0);
        assert!(mask.anchor.1 > 128, "anchor.y = {}", mask.anchor.1);
    }

    #[test]
    fn fovea_pixel_counts_sum_to_total() {
        let cfg = small_config();
        let m = FoveaMask::center_bias(&cfg);
        let total =
            m.foveal_pixel_count() + m.para_foveal_pixel_count() + m.peripheral_pixel_count();
        assert_eq!(total, cfg.render_target_width * cfg.render_target_height);
    }

    #[test]
    fn fovea_pixel_count_decreases_outward() {
        let cfg = small_config();
        let m = FoveaMask::center_bias(&cfg);
        // At 256×256 with 110° FOV, 5° foveal radius is about 6 px,
        // 15° para is about 17 px — peripheral dwarfs both.
        assert!(m.foveal_pixel_count() < m.para_foveal_pixel_count());
        assert!(m.para_foveal_pixel_count() < m.peripheral_pixel_count());
    }

    #[test]
    fn regions_three_classes() {
        let cfg = small_config();
        let m = FoveaMask::center_bias(&cfg);
        let regs: [FoveaRegion; 3] = m.regions();
        assert_eq!(regs[0].resolution, FoveaResolution::Full);
        assert_eq!(regs[1].resolution, FoveaResolution::Half);
        assert_eq!(regs[2].resolution, FoveaResolution::Quarter);
        assert!((regs[0].half_angle_deg - FOVEAL_HALF_ANGLE_DEG).abs() < 1e-6);
        assert!((regs[1].half_angle_deg - PARA_FOVEAL_HALF_ANGLE_DEG).abs() < 1e-6);
    }

    #[test]
    fn diagnostic_circle_emits_pixels() {
        let cfg = small_config();
        let m = FoveaMask::center_bias(&cfg);
        let pixels = m.diagnostic_circle_pixels();
        // At 256×256 with 110° FOV, foveal-radius is ~6 px ; circumference
        // is ~38 px, Bresenham yields between ~32 and ~50 unique pixels.
        assert!(pixels.len() >= 8, "circle pixels = {}", pixels.len());
        assert!(pixels.len() <= 100, "circle pixels = {}", pixels.len());
    }

    #[test]
    fn deterministic_compute_same_input_same_output() {
        let cfg = small_config();
        let input = GazeInput::center_bias_fallback(0);
        let m1 = FoveaMask::compute(&input, &cfg).unwrap();
        let m2 = FoveaMask::compute(&input, &cfg).unwrap();
        assert_eq!(m1.data, m2.data);
        assert_eq!(m1.anchor, m2.anchor);
    }

    #[test]
    fn default_horizontal_fov_realistic() {
        // Quest-3 per-eye horizontal FOV is documented as ~106-110° ; spec
        // table cites 110°. Sanity-check our constant matches.
        assert!((DEFAULT_HORIZONTAL_FOV_DEG - 110.0).abs() < 1e-6);
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let m = FoveaMask::quarter_uniform(64, 64);
        assert_eq!(m.get(63, 63), Some(FoveaResolution::Quarter));
        assert_eq!(m.get(64, 0), None);
        assert_eq!(m.get(0, 64), None);
        assert_eq!(m.get(100, 100), None);
    }
}
