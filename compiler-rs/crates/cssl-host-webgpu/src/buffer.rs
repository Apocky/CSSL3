//! `wgpu::Buffer` allocation + helpers.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § per-backend adapter-layer §
//!   bind_resources.
//! § §§ 12_CAPABILITIES § ISO-OWNERSHIP : wgpu Buffers map to `iso<gpu-buffer>`
//!   in the CSSLv3 cap-system. Synchronization is via
//!   `Queue.on_submitted_work_done` (see `sync.rs`).
//!
//! § DESIGN
//!   `WebGpuBuffer` is a thin newtype around `wgpu::Buffer` carrying creation
//!   metadata (size, usage, label). Allocation goes through the active
//!   `WebGpuDevice`. Readback from GPU → CPU uses the standard wgpu pattern :
//!   create a `MAP_READ`-able staging buffer, copy GPU buffer to staging via
//!   `CommandEncoder::copy_buffer_to_buffer`, submit, then map_async +
//!   block_on.

use crate::device::WebGpuDevice;
use crate::error::WebGpuError;

/// Buffer allocation configuration.
#[derive(Debug, Clone)]
pub struct WebGpuBufferConfig {
    /// Friendly label for diagnostics.
    pub label: Option<String>,
    /// Size in bytes. Must be > 0 ; wgpu rejects zero-sized buffers.
    pub size: u64,
    /// Usage bitset (Storage / Uniform / Vertex / Index / CopySrc / CopyDst /
    /// MapRead / MapWrite / etc.).
    pub usage: wgpu::BufferUsages,
    /// `mapped_at_creation` — immediately mapped for CPU writes (one-shot
    /// upload). Stage-0 default = false.
    pub mapped_at_creation: bool,
}

impl WebGpuBufferConfig {
    /// Storage-buffer with COPY_SRC + COPY_DST + STORAGE usage (the common
    /// compute-pipeline shape).
    #[must_use]
    pub fn storage(size: u64, label: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }

    /// Staging-buffer (CPU-reads what GPU wrote) : MAP_READ + COPY_DST.
    #[must_use]
    pub fn staging_readback(size: u64, label: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }

    /// Uniform-buffer : UNIFORM + COPY_DST.
    #[must_use]
    pub fn uniform(size: u64, label: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }
}

/// CSSLv3 wgpu-buffer handle.
#[derive(Debug)]
pub struct WebGpuBuffer {
    raw: wgpu::Buffer,
    size: u64,
    usage: wgpu::BufferUsages,
}

impl WebGpuBuffer {
    /// Allocate via the device.
    pub fn allocate(device: &WebGpuDevice, cfg: &WebGpuBufferConfig) -> Result<Self, WebGpuError> {
        if cfg.size == 0 {
            return Err(WebGpuError::Buffer("size must be > 0".into()));
        }
        let raw = device.raw_device().create_buffer(&wgpu::BufferDescriptor {
            label: cfg.label.as_deref(),
            size: cfg.size,
            usage: cfg.usage,
            mapped_at_creation: cfg.mapped_at_creation,
        });
        Ok(Self {
            raw,
            size: cfg.size,
            usage: cfg.usage,
        })
    }

    /// One-shot upload : create a buffer initialized from CPU data via
    /// `mapped_at_creation`. The data length must equal `cfg.size`.
    pub fn allocate_initialized(
        device: &WebGpuDevice,
        cfg: &WebGpuBufferConfig,
        data: &[u8],
    ) -> Result<Self, WebGpuError> {
        if data.len() as u64 != cfg.size {
            return Err(WebGpuError::Buffer(format!(
                "data-length ({}) does not match buffer-size ({})",
                data.len(),
                cfg.size
            )));
        }
        let mut init_cfg = cfg.clone();
        init_cfg.mapped_at_creation = true;
        let buf = Self::allocate(device, &init_cfg)?;
        // copy data into the mapped range, then unmap.
        {
            let mut view = buf.raw.slice(..).get_mapped_range_mut();
            view.copy_from_slice(data);
        }
        buf.raw.unmap();
        Ok(buf)
    }

    /// Borrow the wgpu buffer.
    #[must_use]
    pub fn raw(&self) -> &wgpu::Buffer {
        &self.raw
    }

    /// Size in bytes.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Usage bitset.
    #[must_use]
    pub const fn usage(&self) -> wgpu::BufferUsages {
        self.usage
    }
}

#[cfg(test)]
mod tests {
    use super::WebGpuBufferConfig;

    #[test]
    fn storage_config_has_storage_usage() {
        let cfg = WebGpuBufferConfig::storage(1024, "test");
        assert!(cfg.usage.contains(wgpu::BufferUsages::STORAGE));
        assert!(cfg.usage.contains(wgpu::BufferUsages::COPY_SRC));
        assert!(cfg.usage.contains(wgpu::BufferUsages::COPY_DST));
        assert_eq!(cfg.size, 1024);
        assert_eq!(cfg.label.as_deref(), Some("test"));
    }

    #[test]
    fn staging_readback_config_has_map_read() {
        let cfg = WebGpuBufferConfig::staging_readback(64, "stage");
        assert!(cfg.usage.contains(wgpu::BufferUsages::MAP_READ));
        assert!(cfg.usage.contains(wgpu::BufferUsages::COPY_DST));
        // explicitly NO COPY_SRC + NO STORAGE.
        assert!(!cfg.usage.contains(wgpu::BufferUsages::STORAGE));
    }

    #[test]
    fn uniform_config_has_uniform_usage() {
        let cfg = WebGpuBufferConfig::uniform(256, "u");
        assert!(cfg.usage.contains(wgpu::BufferUsages::UNIFORM));
        assert!(cfg.usage.contains(wgpu::BufferUsages::COPY_DST));
    }

    #[test]
    fn config_default_no_mapped_at_creation() {
        let cfg = WebGpuBufferConfig::storage(16, "x");
        assert!(!cfg.mapped_at_creation);
    }
}
