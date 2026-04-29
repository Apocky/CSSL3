//! `wgpu::Texture` allocation + helpers.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § per-backend adapter-layer.
//!
//! § DESIGN
//!   `WebGpuTexture` wraps `wgpu::Texture` + a derived `wgpu::TextureView`
//!   (the view-shape that compute / render passes actually bind). Stage-0
//!   provides 2D + Rgba8Unorm helpers ; full mipmap / array / 3D / depth
//!   coverage lands as needed.

use crate::device::WebGpuDevice;
use crate::error::WebGpuError;

/// Texture allocation configuration.
#[derive(Debug, Clone)]
pub struct WebGpuTextureConfig {
    /// Friendly label.
    pub label: Option<String>,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Depth or array-layer count (1 = simple 2D).
    pub depth_or_array_layers: u32,
    /// Mipmap level count (1 = no mips).
    pub mip_level_count: u32,
    /// MSAA sample count (1 = no MSAA).
    pub sample_count: u32,
    /// Texel format.
    pub format: wgpu::TextureFormat,
    /// Usage bitset.
    pub usage: wgpu::TextureUsages,
}

impl WebGpuTextureConfig {
    /// 2D Rgba8Unorm render-target with COPY_SRC + RENDER_ATTACHMENT.
    #[must_use]
    pub fn render_target_2d(width: u32, height: u32, label: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            width,
            height,
            depth_or_array_layers: 1,
            mip_level_count: 1,
            sample_count: 1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        }
    }

    /// 2D R32Uint storage-texture (compute-shader output).
    #[must_use]
    pub fn storage_2d_r32u(width: u32, height: u32, label: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            width,
            height,
            depth_or_array_layers: 1,
            mip_level_count: 1,
            sample_count: 1,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        }
    }
}

/// CSSLv3 wgpu-texture handle.
#[derive(Debug)]
pub struct WebGpuTexture {
    raw: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

impl WebGpuTexture {
    /// Allocate the texture + auto-derive a default view.
    pub fn allocate(device: &WebGpuDevice, cfg: &WebGpuTextureConfig) -> Result<Self, WebGpuError> {
        if cfg.width == 0 || cfg.height == 0 {
            return Err(WebGpuError::Buffer("width and height must be > 0".into()));
        }
        let raw = device
            .raw_device()
            .create_texture(&wgpu::TextureDescriptor {
                label: cfg.label.as_deref(),
                size: wgpu::Extent3d {
                    width: cfg.width,
                    height: cfg.height,
                    depth_or_array_layers: cfg.depth_or_array_layers,
                },
                mip_level_count: cfg.mip_level_count,
                sample_count: cfg.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: cfg.format,
                usage: cfg.usage,
                view_formats: &[],
            });
        let view = raw.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            raw,
            view,
            width: cfg.width,
            height: cfg.height,
            format: cfg.format,
        })
    }

    /// Borrow the underlying wgpu texture.
    #[must_use]
    pub fn raw(&self) -> &wgpu::Texture {
        &self.raw
    }

    /// Borrow the default view.
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Width in texels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in texels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Texel format.
    #[must_use]
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
}

#[cfg(test)]
mod tests {
    use super::WebGpuTextureConfig;

    #[test]
    fn render_target_2d_has_render_attachment() {
        let cfg = WebGpuTextureConfig::render_target_2d(800, 600, "rt");
        assert_eq!(cfg.width, 800);
        assert_eq!(cfg.height, 600);
        assert_eq!(cfg.depth_or_array_layers, 1);
        assert_eq!(cfg.format, wgpu::TextureFormat::Rgba8Unorm);
        assert!(cfg.usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT));
        assert!(cfg.usage.contains(wgpu::TextureUsages::COPY_SRC));
    }

    #[test]
    fn storage_2d_r32u_has_storage_binding() {
        let cfg = WebGpuTextureConfig::storage_2d_r32u(64, 64, "compute-out");
        assert_eq!(cfg.format, wgpu::TextureFormat::R32Uint);
        assert!(cfg.usage.contains(wgpu::TextureUsages::STORAGE_BINDING));
    }

    #[test]
    fn config_defaults_to_no_msaa_no_mips() {
        let cfg = WebGpuTextureConfig::render_target_2d(16, 16, "x");
        assert_eq!(cfg.mip_level_count, 1);
        assert_eq!(cfg.sample_count, 1);
    }
}
