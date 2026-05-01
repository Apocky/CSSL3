//! § material — surface-material registry for the diagnostic-dense renderer.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-RICH-RENDER (W-LOA-rich-render-overhaul)
//!
//! § ROLE
//!   16-entry GPU-uploadable Look-Up-Table of surface materials. Each Vertex
//!   in the room mesh carries a `material_id: u32` indexing into this table ;
//!   the uber-shader (`scene.wgsl`) reads { albedo · roughness · metallic ·
//!   alpha · emissive } and combines with the per-vertex pattern color.
//!
//! § LAYOUT
//!   Each `Material` is 32 bytes (8 × f32) for 16-byte alignment :
//!     - albedo    : vec3<f32> + roughness     : f32   (16 B)
//!     - emissive  : vec3<f32> + metallic_alpha: f32   (16 B)  [packed]
//!   Total LUT = 16 entries × 32 B = 512 bytes (well under the 16 KiB UBO limit).
//!
//! § PRESETS (8+ named entries · ids 0..7+ are stable)
//!   0  MATTE_GREY        — calibration-reference matte 50% grey
//!   1  VERMILLION_LACQUER— canonical "default plinth" red
//!   2  GOLD_LEAF         — high-roughness gold cap
//!   3  BRUSHED_STEEL     — semi-rough steel (architectural)
//!   4  IRIDESCENT        — view-angle-dependent rainbow film
//!   5  EMISSIVE_CYAN     — full-bright cyan glow (HUD label material)
//!   6  TRANSPARENT_GLASS — alpha=0.35, low-roughness, slight tint
//!   7  HOLOGRAPHIC       — sparkle + dichroic time-driven
//!   8  HAIRY_FUR         — anisotropic-rough off-white
//!   9  DICHROIC_VIOLET   — thin-film violet
//!   10 NEON_MAGENTA      — emissive magenta (stress-object accent)
//!   11 DEEP_INDIGO       — matte deep-blue-violet
//!   12 OFF_WHITE         — limestone-toned wall reference
//!   13 WARM_SKY          — sky-tinted near-white (ceiling)
//!   14 GRADIENT_RED      — saturation-marker red
//!   15 PINK_NOISE_VOL    — soft pink, used for noise-volume cube
//!
//! § STAGE-1 PATH
//!   Once cssl-substrate-omega-field's KAN-BRDF lands, this LUT is replaced
//!   by per-cell ω-field samples + spectral upsample. Stage-0 keeps the table
//!   as a static set so the renderer is operational while the substrate-side
//!   spectral path matures.

#![allow(clippy::cast_precision_loss)]

use bytemuck::{Pod, Zeroable};

/// GPU-uploadable material entry. 48 bytes · 16-byte aligned.
///
/// std140-style packing (vec3+f32 occupies one 16-byte slot each) :
///   - `albedo[0..3]` + `roughness`   (16 B)  — vec4 slot
///   - `emissive[0..3]` + `metallic`  (16 B)  — vec4 slot
///   - `alpha` + `_pad[0..3]`         (16 B)  — vec4 slot
///   = 48 bytes total · 16-aligned · 16 entries × 48 = 768 bytes ≤ 16 KiB.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq)]
pub struct Material {
    /// Linear-RGB albedo (multiplies with pattern color).
    pub albedo: [f32; 3],
    /// 0..1 roughness. 0 = mirror · 1 = pure-lambert.
    pub roughness: f32,
    /// Linear-RGB additive emission (full-bright when nonzero).
    pub emissive: [f32; 3],
    /// 0..1 metallic factor (modulates fresnel highlight).
    pub metallic: f32,
    /// 0..1 alpha (uber-shader emits `vec4(rgb, alpha)`).
    pub alpha: f32,
    /// Padding to align the 48-byte slot to 16 bytes.
    pub _pad: [f32; 3],
}

impl Material {
    /// Construct an opaque dielectric.
    #[must_use]
    pub const fn opaque(albedo: [f32; 3], roughness: f32) -> Self {
        Self {
            albedo,
            roughness,
            emissive: [0.0; 3],
            metallic: 0.0,
            alpha: 1.0,
            _pad: [0.0; 3],
        }
    }

    /// Construct an opaque metallic.
    #[must_use]
    pub const fn metallic(albedo: [f32; 3], roughness: f32) -> Self {
        Self {
            albedo,
            roughness,
            emissive: [0.0; 3],
            metallic: 1.0,
            alpha: 1.0,
            _pad: [0.0; 3],
        }
    }

    /// Construct an emissive (full-bright additive).
    #[must_use]
    pub const fn emissive(albedo: [f32; 3], emissive: [f32; 3]) -> Self {
        Self {
            albedo,
            roughness: 0.5,
            emissive,
            metallic: 0.0,
            alpha: 1.0,
            _pad: [0.0; 3],
        }
    }

    /// Construct a transparent dielectric.
    #[must_use]
    pub const fn transparent(albedo: [f32; 3], roughness: f32, alpha: f32) -> Self {
        Self {
            albedo,
            roughness,
            emissive: [0.0; 3],
            metallic: 0.0,
            alpha,
            _pad: [0.0; 3],
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Stable material IDs (uber-shader indexes directly into MATERIAL_LUT)
// ──────────────────────────────────────────────────────────────────────────

pub const MAT_MATTE_GREY: u32 = 0;
pub const MAT_VERMILLION_LACQUER: u32 = 1;
pub const MAT_GOLD_LEAF: u32 = 2;
pub const MAT_BRUSHED_STEEL: u32 = 3;
pub const MAT_IRIDESCENT: u32 = 4;
pub const MAT_EMISSIVE_CYAN: u32 = 5;
pub const MAT_TRANSPARENT_GLASS: u32 = 6;
pub const MAT_HOLOGRAPHIC: u32 = 7;
pub const MAT_HAIRY_FUR: u32 = 8;
pub const MAT_DICHROIC_VIOLET: u32 = 9;
pub const MAT_NEON_MAGENTA: u32 = 10;
pub const MAT_DEEP_INDIGO: u32 = 11;
pub const MAT_OFF_WHITE: u32 = 12;
pub const MAT_WARM_SKY: u32 = 13;
pub const MAT_GRADIENT_RED: u32 = 14;
pub const MAT_PINK_NOISE_VOL: u32 = 15;

/// Total entries in the MATERIAL_LUT.
pub const MATERIAL_LUT_LEN: usize = 16;

/// Build the canonical 16-entry material LUT. Order matches `MAT_*` constants.
#[must_use]
pub fn material_lut() -> [Material; MATERIAL_LUT_LEN] {
    [
        // 0 MATTE_GREY — calibration matte
        Material::opaque([0.50, 0.50, 0.50], 0.95),
        // 1 VERMILLION_LACQUER — canonical plinth
        Material::opaque([0.88, 0.27, 0.18], 0.40),
        // 2 GOLD_LEAF — bright-warm metal
        Material::metallic([0.96, 0.78, 0.27], 0.30),
        // 3 BRUSHED_STEEL — gunmetal architectural
        Material::metallic([0.62, 0.64, 0.68], 0.55),
        // 4 IRIDESCENT — base albedo (shader adds view-angle hue)
        Material {
            albedo: [0.85, 0.85, 0.95],
            roughness: 0.20,
            emissive: [0.05, 0.10, 0.15],
            metallic: 0.30,
            alpha: 1.0,
            _pad: [0.0; 3],
        },
        // 5 EMISSIVE_CYAN — full-bright HUD-label
        Material::emissive([0.10, 0.85, 0.95], [0.20, 1.40, 1.60]),
        // 6 TRANSPARENT_GLASS — slight aqua tint
        Material::transparent([0.85, 0.92, 0.95], 0.10, 0.35),
        // 7 HOLOGRAPHIC — base + dichroic-shimmer (shader-driven)
        Material {
            albedo: [0.55, 0.65, 0.85],
            roughness: 0.15,
            emissive: [0.10, 0.15, 0.25],
            metallic: 0.20,
            alpha: 1.0,
            _pad: [0.0; 3],
        },
        // 8 HAIRY_FUR — soft warm anisotropic
        Material::opaque([0.78, 0.62, 0.50], 0.85),
        // 9 DICHROIC_VIOLET — thin-film violet
        Material {
            albedo: [0.45, 0.30, 0.75],
            roughness: 0.25,
            emissive: [0.05, 0.0, 0.10],
            metallic: 0.10,
            alpha: 1.0,
            _pad: [0.0; 3],
        },
        // 10 NEON_MAGENTA — emissive accent
        Material::emissive([0.95, 0.20, 0.70], [1.20, 0.30, 0.90]),
        // 11 DEEP_INDIGO — matte deep blue-violet
        Material::opaque([0.16, 0.18, 0.42], 0.70),
        // 12 OFF_WHITE — limestone wall
        Material::opaque([0.78, 0.76, 0.72], 0.80),
        // 13 WARM_SKY — sky-tinted near-white ceiling
        Material::opaque([0.92, 0.92, 0.95], 0.70),
        // 14 GRADIENT_RED — saturation marker
        Material::opaque([0.85, 0.18, 0.18], 0.60),
        // 15 PINK_NOISE_VOL — soft pink for noise volume
        Material::opaque([0.90, 0.62, 0.72], 0.85),
    ]
}

/// Human-readable material name (for HUD + MCP `render.list_materials`).
#[must_use]
pub const fn material_name(id: u32) -> &'static str {
    match id {
        MAT_MATTE_GREY => "Matte-Grey",
        MAT_VERMILLION_LACQUER => "Vermillion-Lacquer",
        MAT_GOLD_LEAF => "Gold-Leaf",
        MAT_BRUSHED_STEEL => "Brushed-Steel",
        MAT_IRIDESCENT => "Iridescent",
        MAT_EMISSIVE_CYAN => "Emissive-Cyan",
        MAT_TRANSPARENT_GLASS => "Transparent-Glass",
        MAT_HOLOGRAPHIC => "Holographic",
        MAT_HAIRY_FUR => "Hairy-Fur",
        MAT_DICHROIC_VIOLET => "Dichroic-Violet",
        MAT_NEON_MAGENTA => "Neon-Magenta",
        MAT_DEEP_INDIGO => "Deep-Indigo",
        MAT_OFF_WHITE => "Off-White",
        MAT_WARM_SKY => "Warm-Sky",
        MAT_GRADIENT_RED => "Gradient-Red",
        MAT_PINK_NOISE_VOL => "Pink-Noise-Vol",
        _ => "Unknown",
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_lut_has_at_least_8_entries() {
        let lut = material_lut();
        assert!(lut.len() >= 8, "LUT must have ≥ 8 presets");
        assert_eq!(lut.len(), MATERIAL_LUT_LEN);
    }

    #[test]
    fn material_lut_has_16_entries_canonical() {
        let lut = material_lut();
        assert_eq!(lut.len(), 16);
    }

    #[test]
    fn material_struct_size_is_48_bytes() {
        // Critical : the WGSL `struct Material` MUST match this layout
        // (std140-style : 3 vec4 slots).
        assert_eq!(core::mem::size_of::<Material>(), 48);
        assert_eq!(core::mem::align_of::<Material>(), 16);
    }

    #[test]
    fn material_pod_zero_is_valid() {
        let m: Material = bytemuck::Zeroable::zeroed();
        assert_eq!(m.albedo, [0.0, 0.0, 0.0]);
        assert_eq!(m.alpha, 0.0);
    }

    #[test]
    fn matte_grey_is_id_zero() {
        assert_eq!(MAT_MATTE_GREY, 0);
        let lut = material_lut();
        assert!((lut[0].albedo[0] - 0.50).abs() < 1e-6);
    }

    #[test]
    fn material_names_are_unique() {
        use std::collections::HashSet;
        let mut names = HashSet::new();
        for id in 0..MATERIAL_LUT_LEN as u32 {
            names.insert(material_name(id));
        }
        assert_eq!(names.len(), MATERIAL_LUT_LEN);
    }

    #[test]
    fn transparent_glass_has_alpha_lt_one() {
        let lut = material_lut();
        assert!(lut[MAT_TRANSPARENT_GLASS as usize].alpha < 1.0);
    }

    #[test]
    fn emissive_cyan_has_nonzero_emissive() {
        let lut = material_lut();
        let m = lut[MAT_EMISSIVE_CYAN as usize];
        assert!(m.emissive.iter().any(|&v| v > 0.5));
    }

    #[test]
    fn gold_leaf_is_metallic() {
        let lut = material_lut();
        assert!((lut[MAT_GOLD_LEAF as usize].metallic - 1.0).abs() < 1e-6);
    }

    #[test]
    fn material_lut_total_byte_size_under_16kib() {
        let total = MATERIAL_LUT_LEN * core::mem::size_of::<Material>();
        assert!(total <= 16 * 1024, "LUT must fit in 16 KiB UBO budget");
        assert_eq!(total, 768);
    }
}
