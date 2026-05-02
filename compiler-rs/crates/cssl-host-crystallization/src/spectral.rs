//! § spectral — 16-band × 4-illuminant spectral lookup table.
//!
//! § REPLACES TEXTURES
//!
//! A conventional engine maps `Material → AlbedoMap → uv-sample → RGB`.
//! LoA replaces this with `Crystal → SpectralLUT → illuminant-project → sRGB`.
//!
//! The LUT stores reflectance per (16 wavelength bands × 4 canonical
//! illuminants). At render time we project the spectrum through the
//! current scene's illuminant cohort (which itself can be composed from
//! multiple light-sources, e.g., 30% sunlight + 60% moonlight + 10% torch
//! = a 0.3/0.6/0.1 weighted illuminant).
//!
//! Output is sRGB. Stage-0 uses a fixed sRGB primary matrix.
//!
//! § STORAGE
//!
//! 16 bands × 4 illuminants × 1 byte = 64 bytes per crystal. (Compare to a
//! conventional 256×256 RGBA8 texture = 256 KiB · 4000× larger.)
//!
//! § DETERMINISM
//!
//! The LUT is derived from the crystal's allocation digest + class so the
//! same crystal always projects to the same color under the same illuminant.

/// 4 canonical illuminants. The render scene composes a weighted blend.
pub const ILLUMINANT_SUN: usize = 0;
pub const ILLUMINANT_MOON: usize = 1;
pub const ILLUMINANT_TORCH: usize = 2;
pub const ILLUMINANT_AMBIENT: usize = 3;
pub const ILLUMINANT_COUNT: usize = 4;

/// 16 wavelength bands · approximately 380nm..780nm in 25nm steps.
pub const SPECTRAL_BANDS: usize = 16;

/// Per-illuminant + per-band reflectance. 64 bytes per crystal.
#[derive(Debug, Clone, Copy)]
pub struct SpectralLut {
    /// `data[illuminant][band]` = reflectance in 0..=255.
    pub data: [[u8; SPECTRAL_BANDS]; ILLUMINANT_COUNT],
}

impl SpectralLut {
    /// Derive from the allocation digest + class.
    pub fn derive(digest: &[u8; 32], class: crate::CrystalClass) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"spectral-derive-v1");
        h.update(digest);
        h.update(&(class as u32).to_le_bytes());
        let mut xof = h.finalize_xof();
        let mut buf = [0u8; 64];
        xof.fill(&mut buf);

        let mut data = [[0u8; SPECTRAL_BANDS]; ILLUMINANT_COUNT];
        for il in 0..ILLUMINANT_COUNT {
            for b in 0..SPECTRAL_BANDS {
                data[il][b] = buf[il * SPECTRAL_BANDS + b];
            }
        }
        Self { data }
    }
}

/// Illuminant-blend weights (must sum to 255 across all 4).
#[derive(Debug, Clone, Copy)]
pub struct IlluminantBlend {
    pub w: [u8; ILLUMINANT_COUNT],
}

impl IlluminantBlend {
    pub const fn new(sun: u8, moon: u8, torch: u8, ambient: u8) -> Self {
        Self { w: [sun, moon, torch, ambient] }
    }

    /// Sun-dominant (daytime).
    pub const fn day() -> Self {
        Self::new(220, 0, 0, 35)
    }

    /// Moon-dominant (night).
    pub const fn night() -> Self {
        Self::new(0, 200, 0, 55)
    }

    /// Torch-dominant (dungeon).
    pub const fn dungeon() -> Self {
        Self::new(0, 30, 200, 25)
    }

    /// Mixed dawn/dusk.
    pub const fn dusk() -> Self {
        Self::new(120, 80, 30, 25)
    }
}

/// Project the spectrum through `blend` and convert to sRGB. Stage-0 uses
/// a fixed CIE-XYZ-1931 → sRGB matrix; future iterations can adapt the
/// matrix to player preferences (color-blind modes, gamma · etc.).
pub fn project_to_srgb(lut: &SpectralLut, blend: IlluminantBlend) -> [u8; 3] {
    // Compose a single 16-band spectrum from the 4-illuminant LUT.
    let mut spectrum = [0u32; SPECTRAL_BANDS];
    let wsum: u32 = blend.w.iter().map(|x| *x as u32).sum::<u32>().max(1);
    for il in 0..ILLUMINANT_COUNT {
        let w = blend.w[il] as u32;
        if w == 0 {
            continue;
        }
        for b in 0..SPECTRAL_BANDS {
            spectrum[b] = spectrum[b].saturating_add((lut.data[il][b] as u32) * w);
        }
    }
    // Normalize.
    for b in 0..SPECTRAL_BANDS {
        spectrum[b] /= wsum;
    }

    // Stage-0 16-band → sRGB. We approximate CIE-1931 X/Y/Z by integrating
    // the spectrum against fixed weights (rough red/green/blue band groups).
    // Full color-management lives in cssl-spectral-render ; this stage-0
    // approximation is fast + bounded + deterministic.
    let mut r: u32 = 0;
    let mut g: u32 = 0;
    let mut b: u32 = 0;
    // Bands 0..5 → blue group, 5..10 → green group, 10..16 → red group.
    for i in 0..5 {
        b += spectrum[i];
    }
    for i in 5..10 {
        g += spectrum[i];
    }
    for i in 10..16 {
        r += spectrum[i];
    }
    // Each group's max is ≈ 5 × 255 (or 6 × 255 for red) ; clamp + scale.
    let r_b = ((r / 6).min(255)) as u8;
    let g_b = ((g / 5).min(255)) as u8;
    let b_b = ((b / 5).min(255)) as u8;
    [r_b, g_b, b_b]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CrystalClass;

    #[test]
    fn derive_is_deterministic() {
        let d = [42u8; 32];
        let a = SpectralLut::derive(&d, CrystalClass::Object);
        let b = SpectralLut::derive(&d, CrystalClass::Object);
        assert_eq!(a.data, b.data);
    }

    #[test]
    fn derive_varies_with_class() {
        let d = [42u8; 32];
        let a = SpectralLut::derive(&d, CrystalClass::Object);
        let b = SpectralLut::derive(&d, CrystalClass::Aura);
        assert_ne!(a.data, b.data);
    }

    #[test]
    fn project_returns_valid_rgb() {
        let d = [42u8; 32];
        let lut = SpectralLut::derive(&d, CrystalClass::Object);
        let rgb = project_to_srgb(&lut, IlluminantBlend::day());
        // u8 always valid by construction · just sanity check non-all-zero.
        let total: u32 = rgb.iter().map(|x| *x as u32).sum();
        assert!(total > 0, "should not be all-zero under day illuminant");
    }

    #[test]
    fn project_varies_with_blend() {
        let d = [42u8; 32];
        let lut = SpectralLut::derive(&d, CrystalClass::Object);
        let day = project_to_srgb(&lut, IlluminantBlend::day());
        let night = project_to_srgb(&lut, IlluminantBlend::night());
        let dungeon = project_to_srgb(&lut, IlluminantBlend::dungeon());
        // At least two different.
        let unique = [day, night, dungeon];
        let s = unique.iter().collect::<std::collections::HashSet<_>>();
        assert!(s.len() >= 2);
    }

    #[test]
    fn blend_constants_make_sense() {
        let d = IlluminantBlend::day();
        assert!(d.w[ILLUMINANT_SUN] > d.w[ILLUMINANT_MOON]);
        let n = IlluminantBlend::night();
        assert!(n.w[ILLUMINANT_MOON] > n.w[ILLUMINANT_SUN]);
        let dn = IlluminantBlend::dungeon();
        assert!(dn.w[ILLUMINANT_TORCH] > dn.w[ILLUMINANT_SUN]);
    }
}
