//! § cssl-render::asset — asset-handle surface (local stub for in-flight cssl-asset)
//! ════════════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Local placeholder asset-handle types that cssl-render needs to operate
//!   without a hard dep on the in-flight cssl-asset (N1) crate. Per the same
//!   wave-7 G-axis pattern as `crate::math`, this module defines just enough
//!   surface area for the renderer to reference textures + meshes + GPU
//!   buffers + samplers without baking in any specific asset-database
//!   strategy.
//!
//! § FUTURE — when cssl-asset lands
//!   This module shrinks to a re-export wrapper. The handle types defined
//!   here (`AssetHandle<T>`, `TextureHandle`, `MeshHandle`) are designed to
//!   match the eventual cssl-asset surface.
//!
//! § DESIGN — handles, not data
//!   The renderer references assets via opaque handles, never owning the
//!   underlying GPU resources directly. The asset crate (or whoever the
//!   asset crate's eventual owner is) holds the storage ; the renderer
//!   passes handles to the backend, which translates them to
//!   backend-specific bindings (Vulkan VkImageView + VkSampler, D3D12
//!   D3D12_GPU_DESCRIPTOR_HANDLE, etc.). This keeps the renderer
//!   substrate-agnostic + lets multiple backends share the same scene.
//!
//! § FORMAT FAMILIES (stage-0 minimum)
//!   - [`TextureFormat::R8Unorm`] / `Rg8Unorm` / `Rgba8Unorm` / `Rgba8UnormSrgb` —
//!     standard 8-bit per channel paths
//!   - [`TextureFormat::R16Float`] / `R32Float` / `Rgba16Float` / `Rgba32Float` —
//!     HDR / linear-data paths (e.g. roughness-metallic packed maps,
//!     normal-encoded buffers, environment cubemaps)
//!   - [`TextureFormat::Depth32Float`] — depth attachment for substrate's
//!     reverse-Z pipeline. Cleared to `0.0` + `GREATER` depth-test.
//!   - [`TextureFormat::Bc1Rgba` ..] — block-compressed paths (BC1/BC3/BC5/
//!     BC7) deferred to N1's compression slice.

use core::marker::PhantomData;

// ════════════════════════════════════════════════════════════════════════════
// § AssetHandle — phantom-typed opaque handle
// ════════════════════════════════════════════════════════════════════════════

/// Opaque handle to an asset. Phantom-typed so the renderer can distinguish
/// `AssetHandle<Texture>` from `AssetHandle<Mesh>` at the type level. The
/// underlying `id` is an arbitrary 32-bit identifier whose interpretation is
/// owned by the asset database (cssl-asset), not the renderer.
///
/// `Self::INVALID` is the canonical "no asset" sentinel — handles default to
/// invalid so callers MUST explicitly bind a real asset before submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetHandle<T> {
    /// Asset-database identifier. `u32::MAX` is reserved for [`Self::INVALID`].
    pub id: u32,
    /// Phantom type marker — distinguishes texture handles from mesh handles
    /// from sampler handles at the type level.
    _marker: PhantomData<T>,
}

impl<T> AssetHandle<T> {
    /// Sentinel value : no asset bound. Renderer skips draws referencing
    /// invalid handles rather than panicking — substrate totality.
    pub const INVALID: Self = Self {
        id: u32::MAX,
        _marker: PhantomData,
    };

    /// Construct from an explicit id. The asset-database owns the id-space.
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// True if this handle points to a real asset (i.e. is not `INVALID`).
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.id != u32::MAX
    }
}

impl<T> Default for AssetHandle<T> {
    fn default() -> Self {
        Self::INVALID
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Texture — placeholder asset type
// ════════════════════════════════════════════════════════════════════════════

/// Placeholder for the eventual cssl-asset texture type. Holds the format +
/// dimensions + mip-count needed for shader binding. The actual pixel data
/// lives in the asset database, not in this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Texture {
    /// Texture format (color space + bit depth).
    pub format: TextureFormat,
    /// Width in texels.
    pub width: u32,
    /// Height in texels. `1` for 1D textures.
    pub height: u32,
    /// Depth in texels for 3D textures, or array-layer count for 2D arrays.
    /// `1` for plain 2D.
    pub depth_or_array_layers: u32,
    /// Number of mipmap levels. `1` for un-mipped.
    pub mip_levels: u32,
    /// Texture dimensionality.
    pub dimension: TextureDimension,
}

impl Texture {
    /// Construct a 2D texture spec.
    #[must_use]
    pub const fn texture_2d(format: TextureFormat, width: u32, height: u32) -> Self {
        Self {
            format,
            width,
            height,
            depth_or_array_layers: 1,
            mip_levels: 1,
            dimension: TextureDimension::Tex2d,
        }
    }

    /// Construct a depth-attachment texture (substrate canonical : Depth32Float).
    #[must_use]
    pub const fn depth(width: u32, height: u32) -> Self {
        Self {
            format: TextureFormat::Depth32Float,
            width,
            height,
            depth_or_array_layers: 1,
            mip_levels: 1,
            dimension: TextureDimension::Tex2d,
        }
    }

    /// True if the texture has a sensible (positive) extent.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.width > 0 && self.height > 0 && self.depth_or_array_layers > 0 && self.mip_levels > 0
    }
}

/// Dimensionality of a texture resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureDimension {
    /// 1D texture — one row of texels. `height` + `depth_or_array_layers` MUST be 1.
    Tex1d,
    /// 2D texture — the common case. `depth_or_array_layers == 1` for
    /// non-array, or `> 1` for 2D-array textures.
    Tex2d,
    /// 3D volumetric texture. `depth_or_array_layers` is the depth.
    Tex3d,
    /// Cubemap (6 faces). `depth_or_array_layers == 6` ; cubemap arrays
    /// use `depth_or_array_layers = 6 * N`.
    Cube,
}

/// Texture pixel format. Stage-0 enumerates the formats the renderer-foundation
/// slice needs ; block-compressed + ASTC + ETC variants land in cssl-asset's
/// compression slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextureFormat {
    /// 8-bit single channel, normalized [0, 1]. Roughness / metallic / AO maps.
    R8Unorm,
    /// 16-bit float single channel — HDR mask data.
    R16Float,
    /// 32-bit float single channel — high-precision linear data.
    R32Float,
    /// 16-bit per channel two-component, normalized — packed normal maps.
    Rg8Unorm,
    /// 16-bit float two-component.
    Rg16Float,
    /// 8-bit per channel RGBA, linear (NOT sRGB).
    Rgba8Unorm,
    /// 8-bit per channel RGBA, sRGB-encoded — albedo / diffuse / UI.
    Rgba8UnormSrgb,
    /// 16-bit float per channel RGBA — HDR scene textures, environment maps.
    Rgba16Float,
    /// 32-bit float per channel RGBA — highest-precision linear data.
    Rgba32Float,
    /// 32-bit float depth, substrate canonical for reverse-Z pipelines.
    Depth32Float,
}

impl TextureFormat {
    /// True if the format includes a depth component (used as depth-buffer).
    #[must_use]
    pub const fn is_depth(self) -> bool {
        matches!(self, Self::Depth32Float)
    }

    /// True if the format is sRGB-encoded — gamma-decode applied on read.
    #[must_use]
    pub const fn is_srgb(self) -> bool {
        matches!(self, Self::Rgba8UnormSrgb)
    }

    /// Bytes per texel — useful for upload-buffer sizing.
    #[must_use]
    pub const fn bytes_per_texel(self) -> u32 {
        match self {
            Self::R8Unorm => 1,
            Self::Rg8Unorm | Self::R16Float => 2,
            Self::Rgba8Unorm
            | Self::Rgba8UnormSrgb
            | Self::Rg16Float
            | Self::R32Float
            | Self::Depth32Float => 4,
            Self::Rgba16Float => 8,
            Self::Rgba32Float => 16,
        }
    }
}

/// Convenience alias for handle to a texture.
pub type TextureHandle = AssetHandle<Texture>;

// ════════════════════════════════════════════════════════════════════════════
// § Sampler — texture-sampling state
// ════════════════════════════════════════════════════════════════════════════

/// Texture-sampling state. Determines how a shader filters the texture
/// (nearest / linear), how it handles UV-out-of-range (clamp / repeat /
/// mirror), + anisotropic filtering level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sampler {
    pub min_filter: FilterMode,
    pub mag_filter: FilterMode,
    pub mipmap_filter: FilterMode,
    pub wrap_u: WrapMode,
    pub wrap_v: WrapMode,
    pub wrap_w: WrapMode,
    /// `1` = no anisotropy. Capped at backend's `max_sampler_anisotropy`.
    pub anisotropy: u8,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::LINEAR_REPEAT
    }
}

impl Sampler {
    /// Linear filter + repeat wrap — typical PBR albedo sampler.
    pub const LINEAR_REPEAT: Self = Self {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        wrap_u: WrapMode::Repeat,
        wrap_v: WrapMode::Repeat,
        wrap_w: WrapMode::Repeat,
        anisotropy: 1,
    };

    /// Linear filter + clamp wrap — UI / 1-tile textures.
    pub const LINEAR_CLAMP: Self = Self {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        wrap_u: WrapMode::ClampToEdge,
        wrap_v: WrapMode::ClampToEdge,
        wrap_w: WrapMode::ClampToEdge,
        anisotropy: 1,
    };

    /// Nearest filter + repeat wrap — pixel-art / explicit-texel sampling.
    pub const NEAREST_REPEAT: Self = Self {
        min_filter: FilterMode::Nearest,
        mag_filter: FilterMode::Nearest,
        mipmap_filter: FilterMode::Nearest,
        wrap_u: WrapMode::Repeat,
        wrap_v: WrapMode::Repeat,
        wrap_w: WrapMode::Repeat,
        anisotropy: 1,
    };
}

/// Texture-filter mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterMode {
    /// Sample the nearest texel.
    Nearest,
    /// Bilinear / trilinear interpolation between texels.
    Linear,
}

/// Texture-wrap mode for UV coordinates outside `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WrapMode {
    /// Tile : `frac(u)`.
    Repeat,
    /// Mirror-tile : `1 - frac(u)` on alternating tiles.
    MirroredRepeat,
    /// Clamp UV to `[0, 1]` then sample edge texel.
    ClampToEdge,
    /// Sample a constant border color (currently always black).
    ClampToBorder,
}

/// Convenience alias for handle to a sampler.
pub type SamplerHandle = AssetHandle<Sampler>;

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_default_is_invalid() {
        let h: TextureHandle = AssetHandle::default();
        assert!(!h.is_valid());
        assert_eq!(h, AssetHandle::INVALID);
    }

    #[test]
    fn handle_new_is_valid_unless_max() {
        let h = AssetHandle::<Texture>::new(0);
        assert!(h.is_valid());
        let h = AssetHandle::<Texture>::new(42);
        assert!(h.is_valid());
        // u32::MAX is reserved for the invalid sentinel even via new().
        let h = AssetHandle::<Texture>::new(u32::MAX);
        assert!(!h.is_valid());
    }

    #[test]
    fn handle_phantom_types_distinct() {
        // Phantom typing : MeshHandle and TextureHandle have different types
        // even with the same underlying id ; this is enforced by the type
        // system at compile time (not a runtime assertion). We exercise the
        // construction here to at least confirm both compile.
        let _t: AssetHandle<Texture> = AssetHandle::new(0);
        let _s: AssetHandle<Sampler> = AssetHandle::new(0);
    }

    #[test]
    fn texture_2d_constructor() {
        let t = Texture::texture_2d(TextureFormat::Rgba8UnormSrgb, 1024, 512);
        assert_eq!(t.width, 1024);
        assert_eq!(t.height, 512);
        assert_eq!(t.depth_or_array_layers, 1);
        assert_eq!(t.mip_levels, 1);
        assert_eq!(t.dimension, TextureDimension::Tex2d);
        assert!(t.is_valid());
    }

    #[test]
    fn texture_depth_constructor_uses_depth32float() {
        let d = Texture::depth(1920, 1080);
        assert_eq!(d.format, TextureFormat::Depth32Float);
        assert!(d.format.is_depth());
        assert!(d.is_valid());
    }

    #[test]
    fn texture_zero_extent_invalid() {
        let t = Texture::texture_2d(TextureFormat::Rgba8Unorm, 0, 100);
        assert!(!t.is_valid());
    }

    #[test]
    fn texture_format_classification() {
        assert!(TextureFormat::Depth32Float.is_depth());
        assert!(!TextureFormat::Rgba8UnormSrgb.is_depth());
        assert!(TextureFormat::Rgba8UnormSrgb.is_srgb());
        assert!(!TextureFormat::Rgba8Unorm.is_srgb());
    }

    #[test]
    fn texture_format_bytes_per_texel() {
        assert_eq!(TextureFormat::R8Unorm.bytes_per_texel(), 1);
        assert_eq!(TextureFormat::R16Float.bytes_per_texel(), 2);
        assert_eq!(TextureFormat::Rgba8Unorm.bytes_per_texel(), 4);
        assert_eq!(TextureFormat::Rgba16Float.bytes_per_texel(), 8);
        assert_eq!(TextureFormat::Rgba32Float.bytes_per_texel(), 16);
        assert_eq!(TextureFormat::Depth32Float.bytes_per_texel(), 4);
    }

    #[test]
    fn sampler_default_is_linear_repeat() {
        let s = Sampler::default();
        assert_eq!(s, Sampler::LINEAR_REPEAT);
    }

    #[test]
    fn sampler_constants_exist() {
        let _ = Sampler::LINEAR_REPEAT;
        let _ = Sampler::LINEAR_CLAMP;
        let _ = Sampler::NEAREST_REPEAT;
    }

    #[test]
    fn sampler_anisotropy_default_is_one() {
        assert_eq!(Sampler::LINEAR_REPEAT.anisotropy, 1);
    }
}
