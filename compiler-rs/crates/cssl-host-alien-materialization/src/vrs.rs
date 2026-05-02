//! § vrs — Variable-Rate Pixel Sampling + Temporal Reprojection.
//!
//! § T11-W18-I · canonical-companion : pixel_field.rs (full-rate path).
//!
//! § Why
//!
//! Apocky-spec : 2560×1440 @ 144Hz stable. Full-rate substrate-resonance
//! resolves every pixel every frame · O(W·H·RAY_SAMPLES·Σ-crystals-near).
//! At 4K-2K-pixels-per-frame · 144 Hz · that's 530M pixel-resolves/s in
//! the substrate-resonance hot loop. Even with W18-B SIMD/SoA wins, raw
//! pixel-count alone is the wall.
//!
//! § What
//!
//! Variable-Rate Pixel Sampling (VRS-style · borrowed from VRS-shading
//! but applied to substrate-resonance pixel evaluation, not raster
//! shading) :
//!
//!   - Tile the framebuffer into 16×16 tiles.
//!   - Each tile gets a VrsTier ∈ {Full, Half, Quarter, Eighth}.
//!     - Full    : sample every pixel · 16×16=256 resolves
//!     - Half    : sample every 2nd pixel both axes · 8×8=64 (1/4 work)
//!     - Quarter : sample every 4th pixel both axes · 4×4=16 (1/16)
//!     - Eighth  : sample every 8th pixel both axes · 2×2=4 (1/64)
//!   - Skipped pixels are filled by :
//!     1. spatial bilinear interpolation from same-frame samples, OR
//!     2. temporal-reproject from `ring.buffers[1]` (last frame) using
//!        observer-delta motion-vector reprojection.
//!
//! § Tier-distribution policy
//!
//! Stage-0 static center-falloff :
//!   - Tile distance-from-screen-center ∈ [0, max-radius]
//!   - 0..1/4 max-radius → Full
//!   - 1/4..1/2          → Half
//!   - 1/2..3/4          → Quarter
//!   - 3/4..1            → Eighth
//!
//! § Adaptive
//!
//! `VrsConfig.adaptive` : if last-frame-time > target-frame-budget-ms,
//! shift the entire tier-LUT down by 1 (Full→Half · Half→Quarter etc.).
//! `notify_frame_time` updates the rolling state.
//!
//! § Temporal-reprojection
//!
//! For Half/Quarter/Eighth tiles, skipped pixels copy from the prior
//! frame's PixelField (passed via `prior_frame: Option<&PixelField>`)
//! at the reprojected source-pixel coord. Stage-0 uses observer-yaw +
//! pos delta to compute a per-tile motion-vector (mv-x, mv-y in pixels)
//! and shifts the source coord. If the source coord is out-of-bounds
//! the pixel falls back to spatial-bilinear-from-current-tile-samples.
//!
//! § Determinism
//!
//! Same observer + crystals + VrsConfig + prior-frame ⇒ same output.
//! No FP · all integer math · tier-LUT precomputed once per resize.

use cssl_host_crystallization::aspect::{aspect_idx, silhouette_at_angle};
use cssl_host_crystallization::hdc::{bundle, HdcVec256};
use cssl_host_crystallization::spectral::{project_to_srgb, SpectralLut};
use cssl_host_crystallization::Crystal;

use crate::observer::ObserverCoord;
use crate::pixel_field::{PixelField, ResonanceFrame};
use crate::ray::{crystals_near, pixel_direction, walk_ray, RAY_SAMPLES};

/// Tile size (px). 16 = matches W18-B SIMD-tile + most VRS-style hardware.
pub const VRS_TILE_PX: u32 = 16;

/// Per-tile sampling tier.
///
/// Step-px is the pixel-stride between resolved samples in both axes.
/// Pixel work per tile is `(VRS_TILE_PX/step)^2`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VrsTier {
    /// Sample every pixel (1× cost, 256 resolves/tile).
    Full = 1,
    /// Sample 1/2 axes (1/4 cost, 64 resolves/tile).
    Half = 2,
    /// Sample 1/4 axes (1/16 cost, 16 resolves/tile).
    Quarter = 4,
    /// Sample 1/8 axes (1/64 cost, 4 resolves/tile).
    Eighth = 8,
}

impl VrsTier {
    pub fn step_px(self) -> u32 {
        self as u32
    }

    /// Downgrade one level (called by adaptive on budget-overrun).
    pub fn downgrade(self) -> Self {
        match self {
            VrsTier::Full => VrsTier::Half,
            VrsTier::Half => VrsTier::Quarter,
            VrsTier::Quarter => VrsTier::Eighth,
            VrsTier::Eighth => VrsTier::Eighth,
        }
    }

    /// Upgrade one level (when frame-budget restores headroom).
    pub fn upgrade(self) -> Self {
        match self {
            VrsTier::Full => VrsTier::Full,
            VrsTier::Half => VrsTier::Full,
            VrsTier::Quarter => VrsTier::Half,
            VrsTier::Eighth => VrsTier::Quarter,
        }
    }
}

/// Configuration for the VRS pipeline.
#[derive(Debug, Clone, Copy)]
pub struct VrsConfig {
    /// Default tier when the framerate is healthy.
    pub default_tier: VrsTier,
    /// True ⇒ downscale on frame-time overrun.
    pub adaptive: bool,
    /// Target per-frame budget in microseconds (e.g., 6944 for 144 Hz).
    pub target_frame_budget_us: u32,
    /// Apply temporal reprojection (else spatial-only fill).
    pub temporal_reproject: bool,
    /// Internal : current adaptive offset (0 = no degradation, +1 = one
    /// downgrade applied to all tiers, etc.). Capped at 3 (max Eighth).
    pub adaptive_offset: u8,
}

impl Default for VrsConfig {
    fn default() -> Self {
        Self {
            default_tier: VrsTier::Half,
            adaptive: true,
            target_frame_budget_us: 6_944, // 144 Hz
            temporal_reproject: true,
            adaptive_offset: 0,
        }
    }
}

impl VrsConfig {
    /// Update the adaptive offset based on observed frame-time.
    /// `last_frame_us` : the time the previous frame took.
    /// Returns the new effective default tier (after applying offset).
    pub fn notify_frame_time(&mut self, last_frame_us: u32) -> VrsTier {
        if !self.adaptive {
            return self.default_tier;
        }
        let budget = self.target_frame_budget_us.max(1);
        if last_frame_us > budget {
            // Overrun → downgrade by one (capped at 3).
            if self.adaptive_offset < 3 {
                self.adaptive_offset += 1;
            }
        } else if last_frame_us < budget * 3 / 4 && self.adaptive_offset > 0 {
            // Healthy headroom → upgrade by one.
            self.adaptive_offset -= 1;
        }
        self.effective_default_tier()
    }

    /// The default-tier with the adaptive-offset applied.
    pub fn effective_default_tier(&self) -> VrsTier {
        let mut t = self.default_tier;
        for _ in 0..self.adaptive_offset {
            t = t.downgrade();
        }
        t
    }
}

/// Per-tile tier table (row-major · `tiles_x * tiles_y`).
#[derive(Debug, Clone)]
pub struct TierMap {
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub tiers: Vec<VrsTier>,
}

impl TierMap {
    /// Static center-falloff tier-distribution.
    ///
    /// Distance (Chebyshev — max of dx,dy in tile-units) from screen-center
    /// → tier. Bands of [0..R/4 → Full · R/4..R/2 → Half ·
    /// R/2..3R/4 → Quarter · ≥3R/4 → Eighth].
    ///
    /// Adaptive : the whole map is then degraded by `cfg.adaptive_offset`
    /// downgrade-steps so the center stays sharp longer than the edges
    /// when budget tightens.
    pub fn build_static_falloff(width: u32, height: u32, cfg: &VrsConfig) -> Self {
        let tiles_x = (width + VRS_TILE_PX - 1) / VRS_TILE_PX;
        let tiles_y = (height + VRS_TILE_PX - 1) / VRS_TILE_PX;
        let mut tiers = Vec::with_capacity((tiles_x * tiles_y) as usize);
        let cx = (tiles_x as i32) / 2;
        let cy = (tiles_y as i32) / 2;
        let max_r = cx.max(cy).max(1);
        for ty in 0..tiles_y as i32 {
            for tx in 0..tiles_x as i32 {
                let dx = (tx - cx).abs();
                let dy = (ty - cy).abs();
                let r = dx.max(dy);
                let band_q = (r * 4) / max_r;
                let mut base = match band_q {
                    0 => VrsTier::Full,
                    1 => VrsTier::Half,
                    2 => VrsTier::Quarter,
                    _ => VrsTier::Eighth,
                };
                // If the user-default is coarser than the falloff band,
                // honor that as the upper bound (so cfg.default_tier =
                // Half effectively caps the center at Half).
                if (cfg.default_tier as u8) > (base as u8) {
                    base = cfg.default_tier;
                }
                // Apply adaptive offset.
                for _ in 0..cfg.adaptive_offset {
                    base = base.downgrade();
                }
                tiers.push(base);
            }
        }
        Self { tiles_x, tiles_y, tiers }
    }

    pub fn at(&self, tx: u32, ty: u32) -> VrsTier {
        let i = (ty * self.tiles_x + tx) as usize;
        self.tiers[i]
    }
}

/// Compute one pixel's substrate-resonance value (returns RGBA8 + lit-flag).
///
/// Extracted from `resolve_substrate_resonance` to be invokable per-VRS-key-pixel.
fn resolve_one_pixel(
    observer: ObserverCoord,
    crystals: &[Crystal],
    px: u32,
    py: u32,
    width: u32,
    height: u32,
) -> (([u8; 4], u32), u64) {
    const NEAR_RADIUS_MM: i32 = 1500;

    let (dx, dy, dz) = pixel_direction(observer, px, py, width, height);
    let samples = walk_ray(observer, dx, dy, dz);

    let mut hdc_acc = HdcVec256::ZERO;
    let mut hdc_inputs: [HdcVec256; RAY_SAMPLES] = [HdcVec256::ZERO; RAY_SAMPLES];
    let mut hdc_count = 0usize;
    let mut spec_acc: [u32; 16] = [0; 16];
    let mut weight_total: u32 = 0;

    for (sample_idx, sample) in samples.iter().enumerate() {
        for ci in crystals_near(crystals, sample.world, NEAR_RADIUS_MM) {
            let crystal = &crystals[ci];
            if !observer.permits_aspect(aspect_idx::SILHOUETTE) {
                continue;
            }
            if !crystal.aspect_permitted(aspect_idx::SILHOUETTE) {
                continue;
            }
            let yaw = observer.yaw_milli ^ ((sample_idx as u32) * 17);
            let pitch = observer.pitch_milli ^ ((sample_idx as u32) * 31);
            let extent = silhouette_at_angle(&crystal.curves, yaw, pitch, crystal.extent_mm);
            if extent <= 0 {
                continue;
            }
            let d_sq = crystal.dist_sq_mm(sample.world).max(1);
            let extent_sq = (crystal.extent_mm as i64) * (crystal.extent_mm as i64);
            let inv_d_scaled =
                (extent_sq.saturating_mul(1024) / (d_sq + extent_sq)).clamp(1, 1024) as u32;
            let weight = ((extent as u32 / 16).max(1))
                .saturating_mul(inv_d_scaled / 4)
                .max(1)
                .min(2048);
            if weight == 0 {
                continue;
            }
            let perm = crystal.hdc.permute(sample_idx as u32 * 7);
            if hdc_count < RAY_SAMPLES {
                hdc_inputs[hdc_count] = perm;
                hdc_count += 1;
            }
            let lut: &SpectralLut = &crystal.spectral;
            for band in 0..16 {
                spec_acc[band] = spec_acc[band]
                    .saturating_add((lut.data[0][band] as u32) * weight / 32);
            }
            weight_total = weight_total.saturating_add(weight);
        }
    }

    if weight_total == 0 {
        return (([0, 0, 0, 0], 0), 0);
    }
    if hdc_count > 0 {
        hdc_acc = bundle(&hdc_inputs[..hdc_count]);
    }
    let mut synth_lut = SpectralLut { data: [[0u8; 16]; 4] };
    for band in 0..16 {
        synth_lut.data[0][band] = (spec_acc[band] / weight_total.max(1)).min(255) as u8;
        synth_lut.data[1][band] = synth_lut.data[0][band] / 3;
        synth_lut.data[2][band] = synth_lut.data[0][band] / 4;
        synth_lut.data[3][band] = synth_lut.data[0][band] / 5;
    }
    let rgb = project_to_srgb(&synth_lut, observer.illuminant_blend);
    (([rgb[0], rgb[1], rgb[2], 255], 1), hdc_acc.words[0])
}

/// Compute integer pixel-space motion vector from prior to current observer.
/// Stage-0 : decompose yaw-delta + position-delta along screen X/Y.
///
/// The MV represents : "the pixel at (px+mv.0, py+mv.1) in the prior frame
/// corresponds (approximately) to (px, py) now". So when filling a skipped
/// pixel, we read from `prior[px+mv.0, py+mv.1]`.
fn motion_vector(
    prev: ObserverCoord,
    curr: ObserverCoord,
    width: u32,
    height: u32,
) -> (i32, i32) {
    // Yaw delta (milliradians) → horizontal pixel shift via FOV ≈ 90° = 1571 mrad.
    // mv_x_px = (yaw_delta_milli / FOV_milli) × width
    let dyaw = (curr.yaw_milli as i32).wrapping_sub(prev.yaw_milli as i32);
    let dpitch = (curr.pitch_milli as i32).wrapping_sub(prev.pitch_milli as i32);
    let fov_milli: i32 = 1571;
    let mv_x = (-dyaw * (width as i32)) / fov_milli;
    let mv_y = (dpitch * (height as i32)) / fov_milli;
    // Ignore translation : position-delta projects through ray-walk anyway,
    // and integer-mm deltas of <100mm map to <1 pixel at typical FOV/distance.
    (mv_x, mv_y)
}

/// Fetch a pixel from the prior-frame at (sx, sy). Out-of-bounds → None.
fn sample_prior(prior: &PixelField, sx: i32, sy: i32) -> Option<[u8; 4]> {
    if sx < 0 || sy < 0 {
        return None;
    }
    let (sxu, syu) = (sx as u32, sy as u32);
    if sxu >= prior.width || syu >= prior.height {
        return None;
    }
    let idx = prior.pixel_index(sxu, syu);
    let p = prior.pixels[idx];
    if p[3] == 0 {
        // Prior was un-lit at this coord; no useful temporal data.
        return None;
    }
    Some(p)
}

/// VRS-aware substrate-resonance resolve.
///
/// Walks each tile, computes the tier-step-px keyer pixels, fills skipped
/// pixels via temporal-reproject (if `prior_frame` provided + cfg permits)
/// or spatial-bilinear from the keyer pixels in the same tile.
///
/// Returns the same `ResonanceFrame` shape as `resolve_substrate_resonance`,
/// with `n_pixels_lit` counting BOTH freshly-resolved + reprojected pixels
/// (the user sees them all as "lit").
pub fn resolve_substrate_resonance_vrs(
    observer: ObserverCoord,
    crystals: &[Crystal],
    field: &mut PixelField,
    cfg: &VrsConfig,
    tier_map: &TierMap,
    prior_observer: Option<ObserverCoord>,
    prior_frame: Option<&PixelField>,
) -> ResonanceFrame {
    field.clear();
    let mut n_pixels_lit: u32 = 0;
    let mut fp_acc: u64 = 0;

    let mv = match (prior_observer, prior_frame) {
        (Some(po), Some(_)) if cfg.temporal_reproject => {
            motion_vector(po, observer, field.width, field.height)
        }
        _ => (0, 0),
    };

    for ty in 0..tier_map.tiles_y {
        for tx in 0..tier_map.tiles_x {
            let tier = tier_map.at(tx, ty);
            let step = tier.step_px();
            let tx0 = tx * VRS_TILE_PX;
            let ty0 = ty * VRS_TILE_PX;
            let tx1 = (tx0 + VRS_TILE_PX).min(field.width);
            let ty1 = (ty0 + VRS_TILE_PX).min(field.height);

            // 1. Compute keyer pixels (every `step` in both axes, anchored
            //    at the tile-origin).
            for ky in (ty0..ty1).step_by(step as usize) {
                for kx in (tx0..tx1).step_by(step as usize) {
                    let ((rgba, lit), fp) =
                        resolve_one_pixel(observer, crystals, kx, ky, field.width, field.height);
                    let idx = field.pixel_index(kx, ky);
                    field.pixels[idx] = rgba;
                    n_pixels_lit = n_pixels_lit.saturating_add(lit);
                    fp_acc = fp_acc.wrapping_add(fp);
                }
            }

            // 2. Fill skipped pixels.
            if step == 1 {
                continue; // Full tier — nothing to fill.
            }
            for py in ty0..ty1 {
                for px in tx0..tx1 {
                    let is_keyer = ((px - tx0) % step == 0) && ((py - ty0) % step == 0);
                    if is_keyer {
                        continue;
                    }
                    let mut filled = false;

                    // 2a. Temporal reprojection (preferred when available).
                    if cfg.temporal_reproject {
                        if let Some(prior) = prior_frame {
                            let sx = (px as i32) + mv.0;
                            let sy = (py as i32) + mv.1;
                            if let Some(p) = sample_prior(prior, sx, sy) {
                                let idx = field.pixel_index(px, py);
                                field.pixels[idx] = p;
                                n_pixels_lit = n_pixels_lit.saturating_add(1);
                                filled = true;
                            }
                        }
                    }

                    // 2b. Spatial-fill from nearest keyer in this tile.
                    if !filled {
                        let kx = tx0 + ((px - tx0) / step) * step;
                        let ky = ty0 + ((py - ty0) / step) * step;
                        let kidx = field.pixel_index(kx, ky);
                        let p = field.pixels[kidx];
                        if p[3] != 0 {
                            let idx = field.pixel_index(px, py);
                            field.pixels[idx] = p;
                            n_pixels_lit = n_pixels_lit.saturating_add(1);
                        }
                    }
                }
            }
        }
    }

    ResonanceFrame {
        observer,
        n_crystals: crystals.len() as u32,
        n_pixels_lit,
        fingerprint: (fp_acc as u32) ^ (fp_acc >> 32) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::spectral::IlluminantBlend;
    use cssl_host_crystallization::{CrystalClass, WorldPos};

    fn day_observer() -> ObserverCoord {
        ObserverCoord {
            x_mm: 0,
            y_mm: 0,
            z_mm: 0,
            yaw_milli: 0,
            pitch_milli: 0,
            frame_t_milli: 0,
            sigma_mask_token: 0xFFFF_FFFF,
            illuminant_blend: IlluminantBlend::day(),
        }
    }

    #[test]
    fn vrs_tier_step_px_correct() {
        assert_eq!(VrsTier::Full.step_px(), 1);
        assert_eq!(VrsTier::Half.step_px(), 2);
        assert_eq!(VrsTier::Quarter.step_px(), 4);
        assert_eq!(VrsTier::Eighth.step_px(), 8);
    }

    #[test]
    fn vrs_tier_downgrade_upgrade_clamp() {
        assert_eq!(VrsTier::Full.downgrade(), VrsTier::Half);
        assert_eq!(VrsTier::Half.downgrade(), VrsTier::Quarter);
        assert_eq!(VrsTier::Quarter.downgrade(), VrsTier::Eighth);
        assert_eq!(VrsTier::Eighth.downgrade(), VrsTier::Eighth); // clamp
        assert_eq!(VrsTier::Eighth.upgrade(), VrsTier::Quarter);
        assert_eq!(VrsTier::Full.upgrade(), VrsTier::Full); // clamp
    }

    #[test]
    fn tier_map_center_is_full_when_default_full() {
        let cfg = VrsConfig {
            default_tier: VrsTier::Full,
            adaptive: false,
            target_frame_budget_us: 6_944,
            temporal_reproject: true,
            adaptive_offset: 0,
        };
        let map = TierMap::build_static_falloff(160, 160, &cfg);
        // 160 / 16 = 10 tiles per axis · center tile (5,5) should be Full.
        assert_eq!(map.at(5, 5), VrsTier::Full);
        // Far corner should be Eighth.
        assert_eq!(map.at(0, 0), VrsTier::Eighth);
    }

    #[test]
    fn tier_map_default_half_caps_center_to_half() {
        let cfg = VrsConfig::default(); // default_tier = Half
        let map = TierMap::build_static_falloff(160, 160, &cfg);
        // Even at the center the tier cannot be coarser-than Full but
        // build_static_falloff floors to default_tier so Half should hold.
        assert_eq!(map.at(5, 5), VrsTier::Half);
    }

    #[test]
    fn adaptive_downgrades_on_overrun() {
        let mut cfg = VrsConfig::default();
        let before = cfg.effective_default_tier();
        // Simulate 10ms frame · budget ~6.9ms · should downgrade.
        cfg.notify_frame_time(10_000);
        let after = cfg.effective_default_tier();
        assert!(
            (after as u8) > (before as u8),
            "after-tier must be coarser than before"
        );
    }

    #[test]
    fn adaptive_upgrades_on_headroom() {
        let mut cfg = VrsConfig::default();
        cfg.adaptive_offset = 2; // start degraded
        let before = cfg.effective_default_tier();
        cfg.notify_frame_time(2_000); // way under budget
        let after = cfg.effective_default_tier();
        assert!(
            (after as u8) < (before as u8),
            "after-tier must be finer than before"
        );
    }

    #[test]
    fn resolve_vrs_with_no_crystals_returns_zero_lit() {
        let cfg = VrsConfig::default();
        let map = TierMap::build_static_falloff(32, 32, &cfg);
        let mut field = PixelField::new(32, 32);
        let frame =
            resolve_substrate_resonance_vrs(day_observer(), &[], &mut field, &cfg, &map, None, None);
        assert_eq!(frame.n_pixels_lit, 0);
    }

    #[test]
    fn resolve_vrs_one_crystal_lights_pixels() {
        let cfg = VrsConfig {
            default_tier: VrsTier::Full,
            adaptive: false,
            ..VrsConfig::default()
        };
        let map = TierMap::build_static_falloff(32, 32, &cfg);
        let mut field = PixelField::new(32, 32);
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let frame = resolve_substrate_resonance_vrs(
            day_observer(),
            &[crystal],
            &mut field,
            &cfg,
            &map,
            None,
            None,
        );
        assert!(frame.n_pixels_lit > 0);
    }

    #[test]
    fn vrs_half_tier_does_less_unique_resolves_than_full() {
        // Use the SAME tier across the whole map (no falloff variance).
        let cfg_full = VrsConfig {
            default_tier: VrsTier::Full,
            adaptive: false,
            target_frame_budget_us: u32::MAX, // disable downgrades
            ..VrsConfig::default()
        };
        let cfg_half = VrsConfig {
            default_tier: VrsTier::Half,
            adaptive: false,
            target_frame_budget_us: u32::MAX,
            ..VrsConfig::default()
        };
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));

        // Build flat tier-maps at each tier (override the falloff).
        let mut map_full = TierMap::build_static_falloff(64, 64, &cfg_full);
        let mut map_half = TierMap::build_static_falloff(64, 64, &cfg_half);
        for t in &mut map_full.tiers {
            *t = VrsTier::Full;
        }
        for t in &mut map_half.tiers {
            *t = VrsTier::Half;
        }

        let mut f_full = PixelField::new(64, 64);
        let mut f_half = PixelField::new(64, 64);

        // Time both runs (rough wall-clock — not a strict bench guarantee
        // because CI varies, but on any plausible host Half should be
        // strictly faster than Full).
        let t0 = std::time::Instant::now();
        let _ = resolve_substrate_resonance_vrs(
            day_observer(),
            &[crystal.clone()],
            &mut f_full,
            &cfg_full,
            &map_full,
            None,
            None,
        );
        let dur_full = t0.elapsed();

        let t1 = std::time::Instant::now();
        let _ = resolve_substrate_resonance_vrs(
            day_observer(),
            &[crystal],
            &mut f_half,
            &cfg_half,
            &map_half,
            None,
            None,
        );
        let dur_half = t1.elapsed();

        // Half should be strictly faster (1/4 work) on any plausible host.
        // We allow generous slack because CI-jitter can be wild ; the ratio
        // assertion still validates the algorithmic shape.
        let ratio = dur_full.as_nanos() as f64 / dur_half.as_nanos().max(1) as f64;
        assert!(
            ratio >= 1.5,
            "expected Half to be at least 1.5x faster than Full · got ratio={}",
            ratio
        );
    }

    #[test]
    fn temporal_reprojection_fills_from_prior_frame() {
        // Build a prior-frame with known pixels.
        let mut prior = PixelField::new(32, 32);
        for p in &mut prior.pixels {
            *p = [42, 84, 126, 255]; // distinctive marker
        }
        let cfg = VrsConfig {
            default_tier: VrsTier::Half,
            adaptive: false,
            temporal_reproject: true,
            ..VrsConfig::default()
        };
        let mut map = TierMap::build_static_falloff(32, 32, &cfg);
        for t in &mut map.tiers {
            *t = VrsTier::Half;
        }
        let mut field = PixelField::new(32, 32);
        // No crystals · keyer pixels will be transparent · skipped pixels
        // would also be transparent without temporal-reproject. With the
        // prior-frame, the skipped pixels MUST come back as the marker.
        let _ = resolve_substrate_resonance_vrs(
            day_observer(),
            &[],
            &mut field,
            &cfg,
            &map,
            Some(day_observer()),
            Some(&prior),
        );
        // At least one skipped pixel should now equal the prior-marker.
        let marker_count = field
            .pixels
            .iter()
            .filter(|p| **p == [42, 84, 126, 255])
            .count();
        assert!(
            marker_count > 0,
            "expected ≥1 reprojected pixel · got {}",
            marker_count
        );
    }

    #[test]
    fn adaptive_falls_back_to_half_default_on_overrun() {
        let mut cfg = VrsConfig {
            default_tier: VrsTier::Full,
            adaptive: true,
            target_frame_budget_us: 6_944,
            temporal_reproject: true,
            adaptive_offset: 0,
        };
        // First frame overran (10ms) — request adaptive update.
        let new_tier = cfg.notify_frame_time(10_000);
        assert_eq!(new_tier, VrsTier::Half, "Full → Half on first overrun");
        // Subsequent stable frames at budget should keep us at Half.
        for _ in 0..3 {
            cfg.notify_frame_time(6_900);
        }
        assert_eq!(cfg.effective_default_tier(), VrsTier::Half);
    }

    #[test]
    fn vrs_determinism() {
        // Same input → same output.
        let cfg = VrsConfig {
            default_tier: VrsTier::Full,
            adaptive: false,
            ..VrsConfig::default()
        };
        let map = TierMap::build_static_falloff(16, 16, &cfg);
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let mut f1 = PixelField::new(16, 16);
        let mut f2 = PixelField::new(16, 16);
        let r1 = resolve_substrate_resonance_vrs(
            day_observer(),
            &[crystal.clone()],
            &mut f1,
            &cfg,
            &map,
            None,
            None,
        );
        let r2 = resolve_substrate_resonance_vrs(
            day_observer(),
            &[crystal],
            &mut f2,
            &cfg,
            &map,
            None,
            None,
        );
        assert_eq!(r1.fingerprint, r2.fingerprint);
        assert_eq!(f1.pixels, f2.pixels);
    }

    #[test]
    fn motion_vector_zero_delta_is_zero() {
        let mv = motion_vector(day_observer(), day_observer(), 1920, 1080);
        assert_eq!(mv, (0, 0));
    }

    #[test]
    fn tile_count_for_2560x1440_is_160x90() {
        let cfg = VrsConfig::default();
        let map = TierMap::build_static_falloff(2560, 1440, &cfg);
        assert_eq!(map.tiles_x, 160);
        assert_eq!(map.tiles_y, 90);
        assert_eq!(map.tiers.len(), 14_400);
    }
}
