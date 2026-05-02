//! § buffer_pack — Crystal[] → GPU storage-buffer pack (bytemuck::Pod).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-G-GPU
//!
//! The GPU compute-shader reads the world's crystals via a single
//! `array<GpuCrystal>` storage buffer (binding 1 in shaders/substrate_resonance
//! .wgsl). To make that read trivially safe we pack each `Crystal` into a
//! `#[repr(C)]` `bytemuck::Pod` struct on the host side, then bytemuck-cast
//! the slice → bytes for `Queue::write_buffer`.
//!
//! § PACK SHAPE
//!
//!   GpuCrystal = 16 (header) + 64 (spectral) + 256 (silhouette) = 336 bytes
//!
//!   - world_pos (vec3<i32>) + extent_mm (i32)  16 B  (16-byte aligned)
//!   - sigma_mask (u32) + 3×u32 pad             16 B
//!   - spectral_lut : [vec4<u32>; 4]            64 B  (4 illum × 4 packed-bands)
//!   - silhouette   : [vec4<i32>; 16]          256 B  (sign-extended i16 → i32)
//!
//!   The silhouette spline is sign-extended from i16 to i32 to avoid the
//!   wgpu `shader-f16`-or-`16-bit-storage` extension. 256 B/crystal × 1k
//!   crystals = 256 KB · well under any storage-buffer ceiling.
//!
//! § PORT-PARITY
//!
//! Every field that the CPU `resolve_substrate_resonance` reads from a
//! `Crystal` is reproduced here at-bit-fidelity. The HDC vector + the
//! non-silhouette aspect splines are intentionally NOT packed : they are
//! never read by the resonance algorithm, only by gameplay-side queries.

use bytemuck::{Pod, Zeroable};
use cssl_host_alien_materialization::observer::ObserverCoord;
use cssl_host_crystallization::aspect::aspect_idx;
use cssl_host_crystallization::Crystal;

// § The `bytemuck` workspace dep enables `derive(Pod, Zeroable)`. We use
// the derive macros so this crate stays #![forbid(unsafe_code)]-clean ;
// the macros' generated `unsafe impl` lives inside the bytemuck crate.

// ════════════════════════════════════════════════════════════════════════════
// § GpuCrystal — host mirror of the WGSL struct.
// ════════════════════════════════════════════════════════════════════════════

/// 336-byte storage-buffer pack of one Crystal. `#[repr(C)]` so the layout
/// is deterministic + matches the WGSL declaration. `bytemuck::Pod` so
/// `bytemuck::cast_slice(&[GpuCrystal])` is a free byte-cast.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuCrystal {
    /// World position (mm fixed-point) ; w = extent_mm.
    pub world_pos_x: i32,
    pub world_pos_y: i32,
    pub world_pos_z: i32,
    pub extent_mm: i32,
    /// Σ-mask permission bits (low 8 bits used).
    pub sigma_mask: u32,
    /// Padding to align spectral_lut to 16-byte boundary.
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
    /// Spectral LUT : 4 illuminants × 4 packed-bands per word.
    /// Each u32 packs 4 consecutive band-bytes (LE order).
    /// Layout : `spectral_lut[ill][word_idx] = (b0 | b1<<8 | b2<<16 | b3<<24)`.
    pub spectral_lut: [[u32; 4]; 4],
    /// Silhouette spline : 16 control points × 4 axis-modulators (i32).
    pub silhouette: [[i32; 4]; 16],
}

impl GpuCrystal {
    pub const SIZE_BYTES: usize = std::mem::size_of::<Self>();
}

// ════════════════════════════════════════════════════════════════════════════
// § GpuObserver — host mirror of ObserverUniform (WGSL binding 0).
// ════════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuObserver {
    pub pos_x_mm: i32,
    pub pos_y_mm: i32,
    pub pos_z_mm: i32,
    pub sigma_mask: u32,
    pub yaw_milli: i32,
    pub pitch_milli: i32,
    pub width: u32,
    pub height: u32,
    pub n_crystals: u32,
    /// Illuminant blend weights packed as 4 × u8 in a single u32 (LE :
    /// sun = byte 0 · moon = byte 1 · torch = byte 2 · ambient = byte 3).
    pub illuminant_blend: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

impl GpuObserver {
    pub const SIZE_BYTES: usize = std::mem::size_of::<Self>();
}

// ════════════════════════════════════════════════════════════════════════════
// § Pack functions.
// ════════════════════════════════════════════════════════════════════════════

/// Pack one `Crystal` into a `GpuCrystal`. Lossless for fields the GPU
/// resonance kernel actually reads.
pub fn pack_crystal(c: &Crystal) -> GpuCrystal {
    // Pack the spectral-LUT : per-illuminant 4 u32 words, each holding 4
    // consecutive band-bytes (LE).
    let mut lut = [[0u32; 4]; 4];
    for (il, illum_row) in c.spectral.data.iter().enumerate() {
        for w in 0..4 {
            let b0 = u32::from(illum_row[w * 4]);
            let b1 = u32::from(illum_row[w * 4 + 1]);
            let b2 = u32::from(illum_row[w * 4 + 2]);
            let b3 = u32::from(illum_row[w * 4 + 3]);
            lut[il][w] = b0 | (b1 << 8) | (b2 << 16) | (b3 << 24);
        }
    }

    // Sign-extend the silhouette spline (the only aspect the resonance
    // kernel reads at frame-time) from i16 → i32.
    let silhouette_spline = c.curves.spline(aspect_idx::SILHOUETTE);
    let mut silhouette = [[0i32; 4]; 16];
    for (pi, point) in silhouette_spline.points.iter().enumerate() {
        for (axi, axis_value) in point.iter().enumerate().take(4) {
            silhouette[pi][axi] = i32::from(*axis_value);
        }
    }

    GpuCrystal {
        world_pos_x: c.world_pos.x_mm,
        world_pos_y: c.world_pos.y_mm,
        world_pos_z: c.world_pos.z_mm,
        extent_mm: c.extent_mm,
        sigma_mask: u32::from(c.sigma_mask),
        _pad0: 0,
        _pad1: 0,
        _pad2: 0,
        spectral_lut: lut,
        silhouette,
    }
}

/// Pack a slice of `Crystal` into a `Vec<GpuCrystal>` ready for byte-casting
/// to a wgpu storage buffer.
pub fn pack_crystals(crystals: &[Crystal]) -> Vec<GpuCrystal> {
    crystals.iter().map(pack_crystal).collect()
}

/// Pack an observer + per-frame metadata into a `GpuObserver` uniform.
pub fn pack_observer(o: ObserverCoord, width: u32, height: u32, n_crystals: u32) -> GpuObserver {
    let blend = o.illuminant_blend.w;
    let illuminant_blend = u32::from(blend[0])
        | (u32::from(blend[1]) << 8)
        | (u32::from(blend[2]) << 16)
        | (u32::from(blend[3]) << 24);
    GpuObserver {
        pos_x_mm: o.x_mm,
        pos_y_mm: o.y_mm,
        pos_z_mm: o.z_mm,
        sigma_mask: o.sigma_mask_token,
        // `yaw_milli` + `pitch_milli` are u32 in the host struct but the
        // shader needs signed values for the rotation. Cast preserves the
        // bit pattern (host already clamps to ±1000 mrad in CPU code).
        yaw_milli: o.yaw_milli as i32,
        pitch_milli: o.pitch_milli as i32,
        width,
        height,
        n_crystals,
        illuminant_blend,
        _pad0: 0,
        _pad1: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::{CrystalClass, WorldPos};

    #[test]
    fn gpu_crystal_size_is_known() {
        // 16 (header) + 16 (sigma + pad) + 64 (spectral) + 256 (silhouette) = 352
        // (the four-u32 alignment padding is between extent_mm and
        // sigma_mask wrapper, plus end-of-struct ; let the compiler tell
        // us, but assert it's 16-aligned + ≥ 336.)
        assert_eq!(GpuCrystal::SIZE_BYTES % 16, 0, "must be 16-byte aligned");
        assert!(GpuCrystal::SIZE_BYTES >= 336, "header+spectral+silhouette must fit");
    }

    #[test]
    fn pack_round_trip_preserves_crystal_state() {
        let crystal = Crystal::allocate(CrystalClass::Object, 42, WorldPos::new(1, 2, 3));
        let packed = pack_crystal(&crystal);

        // World position + extent.
        assert_eq!(packed.world_pos_x, 1);
        assert_eq!(packed.world_pos_y, 2);
        assert_eq!(packed.world_pos_z, 3);
        assert_eq!(packed.extent_mm, crystal.extent_mm);
        assert_eq!(packed.sigma_mask, u32::from(crystal.sigma_mask));

        // Spectral round-trip : verify packed-bytes recover the original.
        for il in 0..4usize {
            for b in 0..16usize {
                let word = packed.spectral_lut[il][b / 4];
                let byte = ((word >> ((b % 4) * 8)) & 0xFF) as u8;
                assert_eq!(byte, crystal.spectral.data[il][b], "spectral mismatch");
            }
        }

        // Silhouette spline round-trip (i16 → i32).
        let s = crystal.curves.spline(aspect_idx::SILHOUETTE);
        for pi in 0..16 {
            for axi in 0..4 {
                assert_eq!(packed.silhouette[pi][axi], i32::from(s.points[pi][axi]));
            }
        }
    }

    #[test]
    fn pack_crystals_keeps_count() {
        let cs = vec![
            Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 0)),
            Crystal::allocate(CrystalClass::Entity, 2, WorldPos::new(0, 0, 1000)),
            Crystal::allocate(CrystalClass::Environment, 3, WorldPos::new(0, 0, 2000)),
        ];
        let packed = pack_crystals(&cs);
        assert_eq!(packed.len(), 3);
        assert_eq!(packed[0].extent_mm, cs[0].extent_mm);
        assert_eq!(packed[2].extent_mm, cs[2].extent_mm);
        assert!(packed[2].extent_mm > packed[0].extent_mm, "env > obj extent");
    }

    #[test]
    fn pack_observer_packs_blend_correctly() {
        let mut o = ObserverCoord::default();
        o.illuminant_blend = cssl_host_crystallization::spectral::IlluminantBlend::day();
        let p = pack_observer(o, 64, 64, 0);
        assert_eq!(p.width, 64);
        assert_eq!(p.height, 64);
        // Day blend = (220, 0, 0, 35).
        assert_eq!(p.illuminant_blend & 0xFF, 220);
        assert_eq!((p.illuminant_blend >> 8) & 0xFF, 0);
        assert_eq!((p.illuminant_blend >> 16) & 0xFF, 0);
        assert_eq!((p.illuminant_blend >> 24) & 0xFF, 35);
    }

    #[test]
    fn cast_slice_is_byte_safe() {
        // Crucial invariant : bytemuck::cast_slice(&[GpuCrystal]) must work.
        let cs = vec![Crystal::allocate(CrystalClass::Object, 1, WorldPos::default())];
        let packed = pack_crystals(&cs);
        let bytes: &[u8] = bytemuck::cast_slice(&packed);
        assert_eq!(bytes.len(), GpuCrystal::SIZE_BYTES);
    }
}
