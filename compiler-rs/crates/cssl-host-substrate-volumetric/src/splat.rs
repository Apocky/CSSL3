//! § splat — host-side GPU buffer pack for the voxel-cloud splat shader.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-L5-VOXEL
//!
//! Mirrors the WGSL declarations in `shaders/volumetric_splat.wgsl`. Every
//! `#[repr(C)]` field-order here MUST match the WGSL struct exactly.
//!
//! § PACK SHAPE
//!
//!   GpuVoxelPoint = 32 bytes :
//!     - world_pos : vec3<i32>     (12 B)
//!     - rgba_pack : u32           ( 4 B)  // RGBA8 packed
//!     - source_crystal : u32      ( 4 B)
//!     - hdc_fingerprint : u32     ( 4 B)
//!     - local_index : u32         ( 4 B)  // expanded from u16 for vec4 align
//!     - sigma_mask : u32          ( 4 B)  // expanded from u8 for vec4 align
//!
//! § CAMERA UNIFORM
//!
//!   GpuVoxelCameraUniform = 64 bytes (one cache-line, 16-aligned).

use bytemuck::{Pod, Zeroable};

use crate::cloud::VoxelCloudHandle;

#[cfg(feature = "runtime")]
use cssl_host_alien_materialization::observer::ObserverCoord;

// ════════════════════════════════════════════════════════════════════════════
// § GpuVoxelPoint — host mirror of the WGSL struct.
// ════════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuVoxelPoint {
    pub world_x_mm: i32,
    pub world_y_mm: i32,
    pub world_z_mm: i32,
    /// RGBA8 packed : (r | g<<8 | b<<16 | a<<24).
    pub rgba_pack: u32,
    pub source_crystal: u32,
    pub hdc_fingerprint: u32,
    pub local_index: u32,
    pub sigma_mask: u32,
}

impl GpuVoxelPoint {
    pub const SIZE_BYTES: usize = std::mem::size_of::<Self>();
}

// ════════════════════════════════════════════════════════════════════════════
// § GpuVoxelCameraUniform — host mirror of the WGSL camera-uniform.
// ════════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuVoxelCameraUniform {
    /// Observer position (mm fixed-point).
    pub pos_x_mm: i32,
    pub pos_y_mm: i32,
    pub pos_z_mm: i32,
    /// Σ-mask token (low 8 bits = aspects 0..8).
    pub sigma_mask: u32,
    pub yaw_milli: i32,
    pub pitch_milli: i32,
    pub width: u32,
    pub height: u32,
    pub n_points: u32,
    /// Splat radius in pixels (default 2).
    pub splat_radius_px: u32,
    /// Reserved padding (16-byte boundary fill).
    pub _pad0: u32,
    pub _pad1: u32,
    /// Reserved future-use (e.g., depth-fog start, camera FOV in radians q24).
    pub _reserved0: u32,
    pub _reserved1: u32,
    pub _reserved2: u32,
    pub _reserved3: u32,
}

impl GpuVoxelCameraUniform {
    pub const SIZE_BYTES: usize = std::mem::size_of::<Self>();
}

// ════════════════════════════════════════════════════════════════════════════
// § Pack functions.
// ════════════════════════════════════════════════════════════════════════════

/// Pack one `VoxelCloudHandle` into a `Vec<GpuVoxelPoint>`. Lossless for
/// fields the GPU splat-shader actually reads.
pub fn pack_voxel_cloud(cloud: &VoxelCloudHandle) -> Vec<GpuVoxelPoint> {
    let mut out = Vec::with_capacity(cloud.points.len());
    for p in &cloud.points {
        let rgba = u32::from(p.emission.rgb[0])
            | (u32::from(p.emission.rgb[1]) << 8)
            | (u32::from(p.emission.rgb[2]) << 16)
            | (u32::from(p.emission.alpha) << 24);
        out.push(GpuVoxelPoint {
            world_x_mm: p.world_x_mm,
            world_y_mm: p.world_y_mm,
            world_z_mm: p.world_z_mm,
            rgba_pack: rgba,
            source_crystal: p.source_crystal,
            hdc_fingerprint: p.hdc_fingerprint,
            local_index: u32::from(p.local_index),
            sigma_mask: u32::from(p.sigma_mask),
        });
    }
    out
}

/// Pack `ObserverCoord` + framebuffer dims into the GPU camera-uniform.
#[cfg(feature = "runtime")]
pub fn pack_camera_uniform(
    observer: &ObserverCoord,
    width: u32,
    height: u32,
    n_points: u32,
) -> GpuVoxelCameraUniform {
    GpuVoxelCameraUniform {
        pos_x_mm: observer.x_mm,
        pos_y_mm: observer.y_mm,
        pos_z_mm: observer.z_mm,
        sigma_mask: observer.sigma_mask_token,
        yaw_milli: observer.yaw_milli as i32,
        pitch_milli: observer.pitch_milli as i32,
        width,
        height,
        n_points,
        splat_radius_px: 2,
        _pad0: 0,
        _pad1: 0,
        _reserved0: 0,
        _reserved1: 0,
        _reserved2: 0,
        _reserved3: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::build_voxel_cloud;
    use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};

    #[test]
    fn gpu_voxel_point_is_32_bytes() {
        assert_eq!(GpuVoxelPoint::SIZE_BYTES, 32);
    }

    #[test]
    fn gpu_camera_uniform_is_64_bytes() {
        assert_eq!(GpuVoxelCameraUniform::SIZE_BYTES, 64);
    }

    #[test]
    fn pack_voxel_cloud_matches_input_size() {
        let crystals = vec![Crystal::allocate(
            CrystalClass::Object,
            1,
            WorldPos::new(0, 0, 1500),
        )];
        let cloud = build_voxel_cloud(&crystals);
        let packed = pack_voxel_cloud(&cloud);
        assert_eq!(packed.len(), cloud.points.len());
    }

    #[test]
    fn pack_voxel_cloud_preserves_rgba() {
        let crystals = vec![Crystal::allocate(
            CrystalClass::Object,
            1,
            WorldPos::new(0, 0, 1500),
        )];
        let cloud = build_voxel_cloud(&crystals);
        let packed = pack_voxel_cloud(&cloud);
        for (host, gpu) in cloud.points.iter().zip(packed.iter()) {
            let r = (gpu.rgba_pack & 0xFF) as u8;
            let g = ((gpu.rgba_pack >> 8) & 0xFF) as u8;
            let b = ((gpu.rgba_pack >> 16) & 0xFF) as u8;
            let a = ((gpu.rgba_pack >> 24) & 0xFF) as u8;
            assert_eq!([r, g, b], host.emission.rgb);
            assert_eq!(a, host.emission.alpha);
        }
    }

    #[test]
    fn empty_cloud_pack_is_empty() {
        let cloud = build_voxel_cloud(&[]);
        let packed = pack_voxel_cloud(&cloud);
        assert_eq!(packed.len(), 0);
    }
}
