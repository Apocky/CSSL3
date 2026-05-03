//! § voxel — host-side `VoxelPoint` + `VoxelEmission` types.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-L5-VOXEL
//!
//! A `VoxelPoint` is the host's normalized representation of one ω-field cell
//! that contributes to the volumetric voxel-cloud. It is a 3D position +
//! emission spectrum + HDC-resonance signature (compressed). The cloud is
//! `Vec<VoxelPoint>` ; per-frame the host rebuilds it from the visible
//! crystals.
//!
//! § DESIGN
//!
//! - Position : i32 millimeters (matches `WorldPos`). Replay-stable.
//! - Emission : 16-band spectrum compressed to a 4-byte sRGB-pre-projection.
//!   Stage-0 keeps a per-cell `[u8; 3]` (sRGB) + `u8` alpha-glow.
//! - HDC signature : 4 bytes of the crystal's HDC vector for future
//!   resonance-bind pass.
//!
//! `VoxelPoint` is the "wide" host-side type. The GPU mirror is `GpuVoxelPoint`
//! (in `splat.rs`) which is identical in size + layout but with `bytemuck::Pod`
//! derive for storage-buffer copy.

/// Per-cell emission packet. Stage-0 = pre-projected sRGB + alpha-glow ; future
/// iterations keep the full 16-band spectrum for spectral-direct displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VoxelEmission {
    /// sRGB color of this cell (already projected through current illuminant).
    pub rgb: [u8; 3],
    /// Alpha-glow : how much the cell contributes per-pixel post-splat.
    /// Encodes (intensity × HDC-resonance) in a single byte.
    pub alpha: u8,
}

/// One cell of the voxel-cloud. Cheap (16 bytes) so a 65k-cell cloud is 1 MiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoxelPoint {
    /// World-position (mm fixed-point).
    pub world_x_mm: i32,
    pub world_y_mm: i32,
    pub world_z_mm: i32,
    /// Source-crystal handle (for debug + replay attestation).
    pub source_crystal: u32,
    /// Emission packet.
    pub emission: VoxelEmission,
    /// HDC fingerprint (4 bytes of the source crystal's HDC vector).
    pub hdc_fingerprint: u32,
    /// Cell's position-within-crystal index (0..N for per-crystal samples).
    /// Used by the splat shader for phase-coherent illumination.
    pub local_index: u16,
    /// Σ-mask snapshot at emission time (for attestation).
    pub sigma_mask: u8,
    pub _pad: u8,
}

/// Size of a `VoxelPoint` in bytes (load-bearing for buffer-pack tests).
pub const VOXEL_POINT_BYTES: usize = std::mem::size_of::<VoxelPoint>();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voxel_point_size() {
        // 4×i32 (16) + emission (4) + hdc (4) + local (2) + sigma (1) + pad (1) = 28 ;
        // alignment to 4 ⇒ size 28. We document this so the GPU mirror can
        // diverge if it needs to (it pads to 32 bytes for vec4 alignment).
        assert!(VOXEL_POINT_BYTES <= 32);
    }

    #[test]
    fn voxel_emission_default() {
        let e = VoxelEmission::default();
        assert_eq!(e.rgb, [0, 0, 0]);
        assert_eq!(e.alpha, 0);
    }
}
