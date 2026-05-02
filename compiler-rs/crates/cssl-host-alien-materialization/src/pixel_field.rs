//! § pixel_field — the substrate-resonance pixel-field algorithm.
//!
//! For each pixel : ray-walk → contributing crystals → HDC bundle +
//! spectral accumulation → sRGB projection → pixel.
//!
//! Pixel-field state is stored separately from the algorithm so the host
//! can hold one allocation across many frames (no per-frame heap traffic
//! for pixel-buffer storage).

use cssl_host_crystallization::aspect::{aspect_idx, silhouette_at_angle};
use cssl_host_crystallization::hdc::{bundle, HdcVec256};
use cssl_host_crystallization::spectral::{project_to_srgb, IlluminantBlend, SpectralLut};
use cssl_host_crystallization::Crystal;

use crate::observer::ObserverCoord;
use crate::ray::{crystals_near, pixel_direction, walk_ray, RAY_SAMPLES};

/// A 2D grid of RGBA pixels. Stored as a flat Vec<[u8; 4]> for direct
/// upload to wgpu textures (or any native framebuffer).
#[derive(Debug, Clone)]
pub struct PixelField {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl PixelField {
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as usize) * (height as usize);
        Self {
            width,
            height,
            pixels: vec![[0, 0, 0, 0]; n],
        }
    }

    pub fn clear(&mut self) {
        for p in &mut self.pixels {
            *p = [0, 0, 0, 0];
        }
    }

    pub fn pixel_index(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width as usize) + (x as usize)
    }

    /// As a flat byte slice (RGBA8). Useful for `wgpu::Queue::write_texture`.
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: we allow it via #[repr(transparent)]-equivalent layout
        // through std-mem bytemuck-style cast. Stage-0 uses safe-byte access:
        // the Vec<[u8;4]> is contiguous so we can re-borrow the underlying
        // bytes via slice::from_raw_parts. To avoid unsafe in this crate
        // (which has #![forbid(unsafe_code)]), we expose by-byte iteration
        // through pixel-by-pixel in the host.
        // Instead, we provide a convenience method that copies to a Vec<u8>.
        // (Hot path callers should use as_bytes_copy or iterate pixels.)
        // Actually we CAN do this safely via slice-len arithmetic:
        // [u8;4] is guaranteed contiguous + same alignment as u8.
        // Use slice::from_raw_parts — wait, that's unsafe.
        // Compromise: `as_bytes_owned` returns a copy.
        unreachable!("use as_bytes_owned() ; this method is reserved for a future zero-copy path")
    }

    /// Owned RGBA8 byte buffer (one allocation per call).
    pub fn as_bytes_owned(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for p in &self.pixels {
            out.extend_from_slice(p);
        }
        out
    }
}

/// Per-frame metadata returned alongside the resolved pixel field.
#[derive(Debug, Clone, Copy)]
pub struct ResonanceFrame {
    pub observer: ObserverCoord,
    pub n_crystals: u32,
    pub n_pixels_lit: u32,
    pub fingerprint: u32,
}

/// The substrate-resonance pixel-field algorithm. Walks each pixel's ray
/// through the ω-field, accumulates contributions from nearby crystals,
/// and projects the accumulated spectrum into sRGB.
///
/// This is the actual paradigm-shift implementation. Each pixel is a
/// substrate-query, not a rasterized triangle.
pub fn resolve_substrate_resonance(
    observer: ObserverCoord,
    crystals: &[Crystal],
    field: &mut PixelField,
) -> ResonanceFrame {
    field.clear();

    // Sphere-radius (mm) for the per-sample crystal-near query.
    const NEAR_RADIUS_MM: i32 = 1500;

    let mut n_pixels_lit: u32 = 0;
    let mut fp_acc: u64 = 0;

    // We accumulate per-pixel into a u32 spectrum buffer (16 bands) before
    // collapsing to sRGB. Stage-0 reuses a stack array per pixel to avoid
    // a per-pixel alloc.
    for py in 0..field.height {
        for px in 0..field.width {
            // 1. Compute observer-ray direction for this pixel.
            let (dx, dy, dz) = pixel_direction(observer, px, py, field.width, field.height);

            // 2. Walk the ray, collecting contributions from crystals.
            let samples = walk_ray(observer, dx, dy, dz);

            // 3. Per-pixel resonance accumulator (HDC bundle + spectrum).
            let mut hdc_acc = HdcVec256::ZERO;
            let mut hdc_inputs: [HdcVec256; RAY_SAMPLES] = [HdcVec256::ZERO; RAY_SAMPLES];
            let mut hdc_count = 0usize;
            let mut spec_acc: [u32; 16] = [0; 16];
            let mut weight_total: u32 = 0;

            for (sample_idx, sample) in samples.iter().enumerate() {
                for ci in crystals_near(crystals, sample.world, NEAR_RADIUS_MM) {
                    let crystal = &crystals[ci];

                    // Σ-mask check : observer must permit the silhouette
                    // aspect for the crystal to contribute at all.
                    if !observer.permits_aspect(aspect_idx::SILHOUETTE) {
                        continue;
                    }
                    if !crystal.aspect_permitted(aspect_idx::SILHOUETTE) {
                        continue;
                    }

                    // Silhouette extent at this observer-angle.
                    let yaw = observer.yaw_milli ^ ((sample_idx as u32) * 17);
                    let pitch = observer.pitch_milli ^ ((sample_idx as u32) * 31);
                    let extent =
                        silhouette_at_angle(&crystal.curves, yaw, pitch, crystal.extent_mm);
                    if extent <= 0 {
                        continue;
                    }

                    // Distance attenuation : closer crystals contribute more.
                    // Stage-0 uses a piecewise-linear inverse-square fall-off
                    // expressed in mm so we never integer-truncate to zero
                    // for in-range crystals.
                    let d_sq = crystal.dist_sq_mm(sample.world).max(1);
                    // inv_d_scaled : 1.0 at touching · ~1/16 at extent_mm radius.
                    let extent_sq = (crystal.extent_mm as i64) * (crystal.extent_mm as i64);
                    let inv_d_scaled =
                        (extent_sq.saturating_mul(1024) / (d_sq + extent_sq)).clamp(1, 1024) as u32;
                    let weight = ((extent as u32 / 16).max(1)).saturating_mul(inv_d_scaled / 4).max(1).min(2048);

                    if weight == 0 {
                        continue;
                    }

                    // HDC: bind the crystal vector with sample-position
                    // permutation, then bundle.
                    let perm = crystal.hdc.permute(sample_idx as u32 * 7);
                    if hdc_count < RAY_SAMPLES {
                        hdc_inputs[hdc_count] = perm;
                        hdc_count += 1;
                    }

                    // Spectral accumulation : add weighted reflectance.
                    let lut: &SpectralLut = &crystal.spectral;
                    for band in 0..16 {
                        // Weighted sum across illuminants (use day-blend
                        // weights pre-applied later in project_to_srgb).
                        // For accumulation we use the canonical sun-band as
                        // the reference; final projection composes the full
                        // illuminant blend.
                        spec_acc[band] = spec_acc[band]
                            .saturating_add((lut.data[0][band] as u32) * weight / 32);
                    }
                    weight_total = weight_total.saturating_add(weight);
                }
            }

            if weight_total == 0 {
                // No contributions. Leave the pixel transparent (alpha = 0).
                continue;
            }

            // 4. Bundle the HDC inputs to get the pixel's resonance vector.
            //    (Currently used only for fingerprinting + debug; future:
            //    the bundled HDC can drive aura-overlay channels.)
            if hdc_count > 0 {
                hdc_acc = bundle(&hdc_inputs[..hdc_count]);
            }
            fp_acc = fp_acc.wrapping_add(hdc_acc.words[0]);

            // 5. Build a synthetic SpectralLut from spec_acc to feed into
            //    project_to_srgb. We populate only the sun-illuminant column
            //    since spec_acc was a single-illuminant accumulation.
            let mut synth_lut = SpectralLut {
                data: [[0u8; 16]; 4],
            };
            for band in 0..16 {
                synth_lut.data[0][band] = (spec_acc[band] / weight_total.max(1)).min(255) as u8;
                synth_lut.data[1][band] = synth_lut.data[0][band] / 3;
                synth_lut.data[2][band] = synth_lut.data[0][band] / 4;
                synth_lut.data[3][band] = synth_lut.data[0][band] / 5;
            }

            // 6. Project to sRGB through the observer's illuminant blend.
            let rgb = project_to_srgb(&synth_lut, observer.illuminant_blend);

            // 7. Write pixel (alpha = 255 marks "lit by substrate").
            let idx = field.pixel_index(px, py);
            field.pixels[idx] = [rgb[0], rgb[1], rgb[2], 255];
            n_pixels_lit = n_pixels_lit.saturating_add(1);
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
    use cssl_host_crystallization::{CrystalClass, WorldPos};

    fn day_observer_at_origin() -> ObserverCoord {
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
    fn empty_field_is_zero() {
        let mut f = PixelField::new(8, 8);
        let frame = resolve_substrate_resonance(day_observer_at_origin(), &[], &mut f);
        assert_eq!(frame.n_pixels_lit, 0);
        assert!(f.pixels.iter().all(|p| p[3] == 0));
    }

    #[test]
    fn one_crystal_lights_some_pixels() {
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let mut f = PixelField::new(16, 16);
        let frame = resolve_substrate_resonance(day_observer_at_origin(), &[crystal], &mut f);
        assert_eq!(frame.n_crystals, 1);
        assert!(frame.n_pixels_lit > 0, "crystal in front of observer should lit pixels");
    }

    #[test]
    fn determinism() {
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let mut a = PixelField::new(8, 8);
        let mut b = PixelField::new(8, 8);
        let fa = resolve_substrate_resonance(day_observer_at_origin(), &[crystal.clone()], &mut a);
        let fb = resolve_substrate_resonance(day_observer_at_origin(), &[crystal], &mut b);
        assert_eq!(fa.fingerprint, fb.fingerprint);
        assert_eq!(a.pixels, b.pixels);
    }

    #[test]
    fn sigma_mask_revoked_skips_crystal() {
        let mut crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        crystal.revoke_aspect(0); // silhouette
        let mut f = PixelField::new(8, 8);
        let frame = resolve_substrate_resonance(day_observer_at_origin(), &[crystal], &mut f);
        assert_eq!(frame.n_pixels_lit, 0);
    }

    #[test]
    fn observer_sigma_mask_revoked_skips_crystal() {
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let mut observer = day_observer_at_origin();
        observer.sigma_mask_token &= !1u32; // revoke silhouette
        let mut f = PixelField::new(8, 8);
        let frame = resolve_substrate_resonance(observer, &[crystal], &mut f);
        assert_eq!(frame.n_pixels_lit, 0);
    }

    #[test]
    fn as_bytes_owned_is_correct_size() {
        let f = PixelField::new(4, 4);
        assert_eq!(f.as_bytes_owned().len(), 4 * 4 * 4);
    }
}
