//! § cssl-render::material — PBR + Lambert + Phong material parameters
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Material parameters that drive the shader. The renderer ships three
//!   shading models :
//!   - [`MaterialModel::Pbr`]     — substrate canonical (metallic-roughness)
//!   - [`MaterialModel::Lambert`] — diffuse-only fallback
//!   - [`MaterialModel::Phong`]   — classic specular-exponent model
//!
//!   The PBR model is the default ; Lambert + Phong are kept for legacy
//!   asset compatibility + as cheaper variants for stylized rendering.
//!
//! § PBR PARAMETER PACKING
//!   Substrate canonical : metallic-roughness PBR (glTF 2.0 + Disney +
//!   Filament). Per-pixel inputs from textures or per-material constants :
//!   - **base_color**   : Rgba8UnormSrgb albedo (sRGB-decoded → linear)
//!   - **normal_map**   : Rgba8Unorm tangent-space normal (xy stored,
//!                        z reconstructed in shader for BC5 compatibility)
//!   - **metallic_rough**: Rg8Unorm packed map — `r` = metallic, `g` = roughness
//!   - **emissive**     : Rgba16Float HDR emission
//!   - **occlusion**    : R8Unorm ambient-occlusion mask
//!   These are STAGE-0 ; PBR-clearcoat / sheen / transmission deferred.
//!
//! § GAMMA-CORRECTNESS
//!   The renderer + shaders work in LINEAR space throughout. Albedo /
//!   emissive textures stored as sRGB get gamma-decoded at fetch time
//!   (TextureFormat::Rgba8UnormSrgb). The final tonemap pass
//!   (RenderGraph TonemapPass) re-encodes to gamma-2.2 / sRGB at
//!   swapchain-output. Per-material `base_color_factor` is in linear
//!   space — don't pre-encode to sRGB on the CPU.

use crate::asset::{SamplerHandle, TextureHandle};
use crate::math::{Vec3, Vec4};

// ════════════════════════════════════════════════════════════════════════════
// § MaterialModel — discriminator for shading model
// ════════════════════════════════════════════════════════════════════════════

/// Shading model selector. Determines which shader-variant the renderer
/// dispatches per draw call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialModel {
    /// Substrate canonical : metallic-roughness PBR. Energy-conserving,
    /// IBL-friendly, glTF-compatible.
    Pbr,
    /// Pure diffuse Lambert. Cheap, no specular response. Useful for
    /// stylized rendering + secondary objects where PBR fidelity is
    /// overkill.
    Lambert,
    /// Classic Blinn-Phong : diffuse + specular with shininess exponent.
    /// Legacy model retained for compatibility with non-PBR assets.
    Phong,
}

impl Default for MaterialModel {
    fn default() -> Self {
        Self::Pbr
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § AlphaMode — opacity / cutout / blend
// ════════════════════════════════════════════════════════════════════════════

/// How alpha is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    /// Fully opaque. Alpha channel ignored. Standard depth-test path.
    Opaque,
    /// Cutout : alpha-test against [`Material::alpha_cutoff`]. Pixels with
    /// `albedo.a < alpha_cutoff` are discarded. No blending.
    Mask,
    /// Standard alpha-blended translucency. Sorted back-to-front per-pass.
    Blend,
}

impl Default for AlphaMode {
    fn default() -> Self {
        Self::Opaque
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § MaterialBinding — texture + sampler pair
// ════════════════════════════════════════════════════════════════════════════

/// A texture reference paired with the sampler used to read it. Either may
/// be invalid — the shader falls back to the constant factor parameter when
/// the texture handle is invalid (e.g. `Material::base_color_factor` in
/// place of the albedo texture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MaterialBinding {
    pub texture: TextureHandle,
    pub sampler: SamplerHandle,
}

impl MaterialBinding {
    /// Empty binding : no texture, no sampler. Shader uses the per-material
    /// constant factor for this slot.
    pub const NONE: Self = Self {
        texture: TextureHandle::INVALID,
        sampler: SamplerHandle::INVALID,
    };

    /// Construct an explicit binding.
    #[must_use]
    pub const fn new(texture: TextureHandle, sampler: SamplerHandle) -> Self {
        Self { texture, sampler }
    }

    /// True if the binding has a valid texture (sampler defaults are OK).
    #[must_use]
    pub const fn has_texture(self) -> bool {
        self.texture.is_valid()
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Material — the renderer-side material struct
// ════════════════════════════════════════════════════════════════════════════

/// Material : shading-model + per-channel constants + texture bindings.
///
/// Stage-0 ships the PBR + Lambert + Phong unions in a single struct ; the
/// shader-variant dispatcher reads `model` and ignores the irrelevant
/// fields. A more compact discriminated-union representation is a future
/// slice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    /// Shading model selector.
    pub model: MaterialModel,
    /// Alpha mode : opaque / cutout / blend.
    pub alpha_mode: AlphaMode,
    /// Alpha cutoff threshold in `[0, 1]` for `AlphaMode::Mask`.
    pub alpha_cutoff: f32,
    /// True if this material's surface renders both sides of the geometry.
    /// Disables backface culling for this draw.
    pub double_sided: bool,

    // ─ PBR (Pbr) ─
    /// PBR : base color (linear-space RGBA). Modulates the albedo texture.
    pub base_color_factor: Vec4,
    /// PBR : albedo / base color texture. sRGB-encoded at storage,
    /// linear-decoded at fetch.
    pub base_color: MaterialBinding,
    /// PBR : metallic factor in `[0, 1]`. `0` = dielectric, `1` = conductor.
    pub metallic_factor: f32,
    /// PBR : roughness factor in `[0, 1]`. `0` = mirror, `1` = fully diffuse.
    pub roughness_factor: f32,
    /// PBR : packed metallic-roughness texture. `r` = metallic, `g` = roughness.
    pub metallic_roughness: MaterialBinding,
    /// PBR : tangent-space normal map. Substrate canonical encoding stores
    /// XY in `r`/`g`, Z reconstructed from `sqrt(1 - x*x - y*y)` in the
    /// shader (BC5-compatible).
    pub normal_map: MaterialBinding,
    /// PBR : ambient-occlusion mask. Multiplied into indirect lighting.
    pub occlusion: MaterialBinding,
    /// PBR : emissive linear-space RGB factor. Modulates the emissive texture.
    pub emissive_factor: Vec3,
    /// PBR : HDR emissive texture (Rgba16Float canonical).
    pub emissive: MaterialBinding,

    // ─ Lambert ─
    /// Lambert : diffuse color (linear). Used when `model == Lambert`.
    pub lambert_diffuse_factor: Vec3,
    /// Lambert : optional diffuse texture.
    pub lambert_diffuse: MaterialBinding,

    // ─ Phong ─
    /// Phong : diffuse color (linear). Used when `model == Phong`.
    pub phong_diffuse_factor: Vec3,
    /// Phong : optional diffuse texture.
    pub phong_diffuse: MaterialBinding,
    /// Phong : specular color (linear).
    pub phong_specular_factor: Vec3,
    /// Phong : shininess exponent. Higher = tighter specular highlight.
    /// Typical range `[1, 256]`.
    pub phong_shininess: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self::DEFAULT_PBR
    }
}

impl Material {
    /// Default PBR material : white albedo, fully rough, dielectric, no
    /// emission. A reasonable starting point for new materials.
    pub const DEFAULT_PBR: Self = Self {
        model: MaterialModel::Pbr,
        alpha_mode: AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        double_sided: false,
        base_color_factor: Vec4::ONE,
        base_color: MaterialBinding::NONE,
        metallic_factor: 0.0,
        roughness_factor: 1.0,
        metallic_roughness: MaterialBinding::NONE,
        normal_map: MaterialBinding::NONE,
        occlusion: MaterialBinding::NONE,
        emissive_factor: Vec3::ZERO,
        emissive: MaterialBinding::NONE,
        lambert_diffuse_factor: Vec3::ONE,
        lambert_diffuse: MaterialBinding::NONE,
        phong_diffuse_factor: Vec3::ONE,
        phong_diffuse: MaterialBinding::NONE,
        phong_specular_factor: Vec3::ONE,
        phong_shininess: 32.0,
    };

    /// Default Lambert material : pure white diffuse, no texture.
    pub const DEFAULT_LAMBERT: Self = Self {
        model: MaterialModel::Lambert,
        ..Self::DEFAULT_PBR
    };

    /// Default Phong material : white diffuse + white specular + shininess 32.
    pub const DEFAULT_PHONG: Self = Self {
        model: MaterialModel::Phong,
        ..Self::DEFAULT_PBR
    };

    /// Construct a PBR material with explicit albedo factor + metallic +
    /// roughness. Convenience for common cases.
    #[must_use]
    pub const fn pbr(base_color: Vec4, metallic: f32, roughness: f32) -> Self {
        Self {
            base_color_factor: base_color,
            metallic_factor: metallic,
            roughness_factor: roughness,
            ..Self::DEFAULT_PBR
        }
    }

    /// True if this material uses translucency.
    #[must_use]
    pub fn is_translucent(&self) -> bool {
        matches!(self.alpha_mode, AlphaMode::Blend)
    }

    /// True if the material's emissive contribution is non-zero (factor or
    /// bound emissive texture).
    #[must_use]
    pub fn has_emission(&self) -> bool {
        self.emissive_factor.length_squared() > f32::EPSILON || self.emissive.has_texture()
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_default_is_pbr() {
        assert_eq!(Material::default().model, MaterialModel::Pbr);
    }

    #[test]
    fn material_default_pbr_is_opaque_white_dielectric_rough() {
        let m = Material::DEFAULT_PBR;
        assert_eq!(m.alpha_mode, AlphaMode::Opaque);
        assert_eq!(m.base_color_factor, Vec4::ONE);
        assert_eq!(m.metallic_factor, 0.0);
        assert_eq!(m.roughness_factor, 1.0);
        assert!(!m.double_sided);
        assert!(!m.is_translucent());
    }

    #[test]
    fn material_default_lambert_carries_pbr_constants() {
        let m = Material::DEFAULT_LAMBERT;
        assert_eq!(m.model, MaterialModel::Lambert);
        // Lambert default reuses PBR's base_color_factor as a sentinel.
        assert_eq!(m.lambert_diffuse_factor, Vec3::ONE);
    }

    #[test]
    fn material_default_phong_uses_shininess_32() {
        let m = Material::DEFAULT_PHONG;
        assert_eq!(m.model, MaterialModel::Phong);
        assert_eq!(m.phong_shininess, 32.0);
    }

    #[test]
    fn material_pbr_constructor() {
        let m = Material::pbr(Vec4::new(1.0, 0.5, 0.0, 1.0), 0.8, 0.2);
        assert_eq!(m.model, MaterialModel::Pbr);
        assert_eq!(m.base_color_factor, Vec4::new(1.0, 0.5, 0.0, 1.0));
        assert_eq!(m.metallic_factor, 0.8);
        assert_eq!(m.roughness_factor, 0.2);
    }

    #[test]
    fn material_alpha_mode_default_opaque() {
        assert_eq!(AlphaMode::default(), AlphaMode::Opaque);
    }

    #[test]
    fn material_translucency_check() {
        let mut m = Material::DEFAULT_PBR;
        assert!(!m.is_translucent());
        m.alpha_mode = AlphaMode::Mask;
        // Mask is NOT translucent — it's binary alpha-test.
        assert!(!m.is_translucent());
        m.alpha_mode = AlphaMode::Blend;
        assert!(m.is_translucent());
    }

    #[test]
    fn material_emission_check() {
        let mut m = Material::DEFAULT_PBR;
        assert!(!m.has_emission());
        m.emissive_factor = Vec3::new(1.0, 0.5, 0.0);
        assert!(m.has_emission());
    }

    #[test]
    fn material_binding_none_invalid() {
        let b = MaterialBinding::NONE;
        assert!(!b.has_texture());
    }

    #[test]
    fn material_binding_with_texture() {
        let b = MaterialBinding::new(TextureHandle::new(7), SamplerHandle::new(2));
        assert!(b.has_texture());
        assert_eq!(b.texture.id, 7);
        assert_eq!(b.sampler.id, 2);
    }
}
