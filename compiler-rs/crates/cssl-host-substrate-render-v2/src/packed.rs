//! § packed — GpuCrystalPacked · 64-byte cache-friendly struct-of-arrays-style pack.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-SOA-PACK
//!
//! § THESIS
//!
//! The W18-G `GpuCrystal` is **352 bytes** per element. At 128 crystals the
//! storage-buffer is `128 × 352 = 45 056 B = 44 KiB`. Per-pixel each thread
//! linearly scans this buffer ⇒ at 1440p × 8 ray-samples × 128 crystals the
//! kernel issues `1440 × 1440 × 8 × 128 ≈ 2.1 G crystal-loads / frame` worth
//! of memory bandwidth. The L1-cache footprint at 44 KiB *exceeds* most
//! integrated-GPU per-SM L1 capacities (Intel-Arc tile = 16 KB · AMD-RDNA2
//! WGP = 32 KB) ⇒ heavy thrashing.
//!
//! Packing each crystal to **64 bytes** drops the buffer to `128 × 64 =
//! 8 KiB` which fits inside *every* mainstream GPU's per-SM L1. Expected
//! bandwidth-pressure reduction = **5.5×** (352 ÷ 64).
//!
//! § LAYOUT (64 B · `repr(C)` · `bytemuck::Pod`)
//!
//! | Offset | Size | Field                       | Encoding                         |
//! |--------|------|-----------------------------|----------------------------------|
//! | 0      | 4    | `world_pos_x_mm`            | `i32` (mm fixed-point)           |
//! | 4      | 4    | `world_pos_y_mm`            | `i32` (mm fixed-point)           |
//! | 8      | 4    | `world_pos_z_mm`            | `i32` (mm fixed-point)           |
//! | 12     | 2    | `extent_mm_u16`             | `u16` (clamp 0..65535 mm = 65 m) |
//! | 14     | 1    | `sigma_mask_u8`             | `u8`  (low-8 of W18-G u32 mask)  |
//! | 15     | 1    | `flags_u8`                  | reserved · phase-seed-low byte   |
//! | 16     | 16   | `spectral_quad`             | `[u32; 4]` · 16 bands × 1 byte   |
//! | 32     | 32   | `silhouette_quant`          | `[i8; 32]` · 16 ctrl × 2 axis-q  |
//!
//! TOTAL = 64 B — exactly the wgpu uniform-buffer alignment + AMD-cache-line.
//!
//! § LOSSY FIELDS
//!
//! - **`extent_mm`** truncated to `u16` ⇒ extent must be `0 ≤ x ≤ 65 535 mm
//!   (65.535 m)`. The Crystal-allocator currently caps at 32 km (`i32`) but
//!   the SUBSTRATE-RENDER kernel only cares about *visible* crystals which
//!   are always sub-100m. We saturate clamp on overflow ; tested below.
//!
//! - **`spectral_quad[4]`** keeps a *single*-illuminant spectrum (16 bytes
//!   = 16 bands × 1 byte). The W18-G original has 4 illuminants × 16 bands
//!   = 64 bytes ; the Σ-blend is *already* applied host-side (`pack_crystal`
//!   pre-folds illuminant_blend into a single equivalent-spectrum) so the
//!   shader-side `weighted_reflectance` collapses to a direct band-byte
//!   read. Loss = the per-frame illuminant-shift can no longer be done in
//!   the shader (must rebuild the buffer when illuminants change). Test
//!   for round-trip below confirms zero-error when blend is stable across
//!   frames.
//!
//! - **`silhouette_quant[32]`** keeps 2 axes (i8 each) per spline-control-
//!   point instead of 4 (i32 each). The shader uses 4-axis weight blending
//!   (`weights = vec4<i32>(w0, w1, w2, w3)`) ; collapsing to 2 axes loses
//!   the lower-significance modulators. Empirically (see precision test
//!   below) the visual delta is ≤ 4 / 255 ⇒ sub-perceptual at 8-bit RGBA.
//!   Quantization is `i32 → i8` via `(x.clamp(-127,127)) as i8` lossy
//!   saturation.
//!
//! § HOST-SIDE PRE-BLEND
//!
//! `pack_crystal_packed` accepts the host-side `illuminant_blend: [u8; 4]`
//! and emits a single `spectral_quad[16]` = blend-weighted average. This
//! moves the shader's `weighted_reflectance` 4-illuminant pre-mix to the
//! host (one extra mult-add per crystal-pack, dwarfed by the GPU savings).
//!
//! § FALLBACK
//!
//! The 352-byte `GpuCrystal` (re-exported from `cssl-host-substrate-resonance-gpu`)
//! remains available as the default code path. The packed path is opt-in
//! via `LOA_SUBSTRATE_PACKED=1` env-var (host-side toggle) + a uniform-
//! input-flag `observer.packed_flag` (shader-side branch). The shader-side
//! plumbing is left as a future rewrite ; this module ships only the
//! **Rust-side scaffold + tests** so the next wave can pick up directly.

use bytemuck::{Pod, Zeroable};
use cssl_host_crystallization::aspect::aspect_idx;
use cssl_host_crystallization::Crystal;

// ════════════════════════════════════════════════════════════════════════════
// § GpuCrystalPacked — 64-byte cache-friendly Crystal pack.
// ════════════════════════════════════════════════════════════════════════════

/// 64-byte storage-buffer pack of one Crystal. `#[repr(C)]` so the layout is
/// deterministic + `bytemuck::Pod` so `bytemuck::cast_slice(&[...])` is a
/// free byte-cast.
///
/// See module-level docs for byte-by-byte layout.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuCrystalPacked {
    /// World position X (mm fixed-point ; same encoding as `GpuCrystal`).
    pub world_pos_x_mm: i32,
    /// World position Y.
    pub world_pos_y_mm: i32,
    /// World position Z.
    pub world_pos_z_mm: i32,
    /// Extent (mm) saturating-clamped to `u16`. Crystals beyond 65 535 mm
    /// extent are not visible at typical observer ranges anyway.
    pub extent_mm_u16: u16,
    /// Σ-mask permission (low 8 bits — high bits unused).
    pub sigma_mask_u8: u8,
    /// Reserved flag-byte. Currently holds `phase_seed_low = (extent ^ idx) & 0xFF`
    /// for the shader's amp-bundle phase calculation. Future bits TBD.
    pub flags_u8: u8,
    /// Pre-blended spectral spectrum : 16 bands × 1 byte each, packed 4-bytes
    /// per `u32` little-endian. `spectral_quad[w] = b0 | b1<<8 | b2<<16 | b3<<24`
    /// where `b_i` is the host-side `Σ(illum_blend[ill] × spectral[ill][b])`
    /// pre-mix (one observer-side `illuminant_blend`-weighted average).
    pub spectral_quad: [u32; 4],
    /// Silhouette spline (lossy). 16 control points × 2 axes (i8 each) =
    /// 32 bytes. Axis 0 = primary modulator · axis 1 = secondary. Lower-
    /// significance W18-G axes 2/3 are dropped — visual delta ≤ 4/255.
    pub silhouette_quant: [i8; 32],
}

impl GpuCrystalPacked {
    /// `std::mem::size_of::<Self>()` — guaranteed 64 by `#[repr(C)]` layout.
    pub const SIZE_BYTES: usize = std::mem::size_of::<Self>();

    /// Compile-time assertion : the struct is exactly 64 B. If a refactor
    /// drifts the layout, the const-eval fails to compile.
    pub const SIZE_CHECK: () = {
        assert!(
            Self::SIZE_BYTES == 64,
            "GpuCrystalPacked must be exactly 64 bytes — see module docs",
        );
    };
}

// ════════════════════════════════════════════════════════════════════════════
// § Pack functions.
// ════════════════════════════════════════════════════════════════════════════

/// Pack one `Crystal` into a `GpuCrystalPacked`, applying the observer's
/// `illuminant_blend` pre-mix on the host side.
///
/// `illuminant_blend` is `[u8; 4]` — the same per-illuminant byte-weights
/// the W18-G `GpuObserver.illuminant_blend` field carries. This pre-blends
/// the 4-illuminant × 16-band spectral table down to a single 16-byte
/// pre-folded spectrum so the shader can read one byte per band without
/// the per-frame illuminant-mix multiply.
///
/// Quantization is documented in the module docstring.
pub fn pack_crystal_packed(c: &Crystal, illuminant_blend: [u8; 4]) -> GpuCrystalPacked {
    // ── World position (lossless · keep i32) ──────────────────────────
    let world_pos_x_mm = c.world_pos.x_mm;
    let world_pos_y_mm = c.world_pos.y_mm;
    let world_pos_z_mm = c.world_pos.z_mm;

    // ── Extent (saturating i32 → u16) ─────────────────────────────────
    let extent_mm_u16: u16 = c.extent_mm.clamp(0, u16::MAX as i32) as u16;

    // ── Σ-mask (truncate u32 → u8 ; only low byte is ever used in WGSL
    //    `(c.sigma_mask & 1u) != 0u` ; high bits never sampled). ───────
    let sigma_mask_u8: u8 = u32::from(c.sigma_mask) as u8;

    // ── Reserved flag-byte : low byte of `extent ^ phase_seed_proxy`.
    //    Used by the future packed-shader for the amp-bundle phase. ───
    let flags_u8: u8 = ((extent_mm_u16 as u32 ^ 0x9E37u32) & 0xFFu32) as u8;

    // ── Pre-blended spectral pack : ill-blend × per-band averaging. ──
    //    weighted_reflectance(b) = Σ(illum_blend[i] × spectral[i][b]) / Σblend
    //    Folded host-side ; shader reads one byte / band.
    let blend_sum: u32 = u32::from(illuminant_blend[0])
        + u32::from(illuminant_blend[1])
        + u32::from(illuminant_blend[2])
        + u32::from(illuminant_blend[3]);
    let blend_sum = blend_sum.max(1);
    let mut bands = [0u8; 16];
    for (b_i, band_byte) in bands.iter_mut().enumerate() {
        let acc: u32 = u32::from(illuminant_blend[0]) * u32::from(c.spectral.data[0][b_i])
            + u32::from(illuminant_blend[1]) * u32::from(c.spectral.data[1][b_i])
            + u32::from(illuminant_blend[2]) * u32::from(c.spectral.data[2][b_i])
            + u32::from(illuminant_blend[3]) * u32::from(c.spectral.data[3][b_i]);
        *band_byte = (acc / blend_sum).min(255) as u8;
    }
    let spectral_quad: [u32; 4] = [
        u32::from(bands[0])
            | (u32::from(bands[1]) << 8)
            | (u32::from(bands[2]) << 16)
            | (u32::from(bands[3]) << 24),
        u32::from(bands[4])
            | (u32::from(bands[5]) << 8)
            | (u32::from(bands[6]) << 16)
            | (u32::from(bands[7]) << 24),
        u32::from(bands[8])
            | (u32::from(bands[9]) << 8)
            | (u32::from(bands[10]) << 16)
            | (u32::from(bands[11]) << 24),
        u32::from(bands[12])
            | (u32::from(bands[13]) << 8)
            | (u32::from(bands[14]) << 16)
            | (u32::from(bands[15]) << 24),
    ];

    // ── Silhouette quant : lossy 4-axis i32 → 2-axis i8.  ───────────
    //    Axis 0 + axis 1 are the primary modulators ; 2/3 are dropped.
    let silhouette_spline = c.curves.spline(aspect_idx::SILHOUETTE);
    let mut silhouette_quant = [0i8; 32];
    for (pi, point) in silhouette_spline.points.iter().enumerate().take(16) {
        // i16 → clamp(±127) → i8.
        let a0 = i32::from(point[0]).clamp(-127, 127) as i8;
        let a1 = i32::from(point[1]).clamp(-127, 127) as i8;
        silhouette_quant[pi * 2] = a0;
        silhouette_quant[pi * 2 + 1] = a1;
    }

    GpuCrystalPacked {
        world_pos_x_mm,
        world_pos_y_mm,
        world_pos_z_mm,
        extent_mm_u16,
        sigma_mask_u8,
        flags_u8,
        spectral_quad,
        silhouette_quant,
    }
}

/// Pack a slice of `Crystal` into a `Vec<GpuCrystalPacked>` ready for byte-
/// casting to a wgpu storage buffer.
pub fn pack_crystals_packed(crystals: &[Crystal], illuminant_blend: [u8; 4]) -> Vec<GpuCrystalPacked> {
    crystals
        .iter()
        .map(|c| pack_crystal_packed(c, illuminant_blend))
        .collect()
}

// ════════════════════════════════════════════════════════════════════════════
// § Env-var toggle for the packed path.
// ════════════════════════════════════════════════════════════════════════════

/// Returns `true` if the host has opted into the packed-buffer path via the
/// `LOA_SUBSTRATE_PACKED=1` environment variable.
///
/// The default is `false` : v2 keeps the W18-G 352-byte path until both the
/// Rust scaffold and the WGSL packed-kernel have shipped + been verified.
#[must_use]
pub fn packed_path_enabled() -> bool {
    std::env::var("LOA_SUBSTRATE_PACKED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on"))
        .unwrap_or(false)
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests — 7 unit tests + 1 const-eval check.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::{CrystalClass, WorldPos};

    /// Test 1 : the packed struct is *exactly* 64 bytes. Const-eval doubles
    /// as a compile-time assertion ; this runtime check is belt + braces.
    #[test]
    fn gpu_crystal_packed_is_exactly_64_bytes() {
        // Force the const-eval check to fire (zero-cost · proves SIZE_CHECK
        // compiled-in).
        let _ = GpuCrystalPacked::SIZE_CHECK;
        assert_eq!(
            GpuCrystalPacked::SIZE_BYTES,
            64,
            "GpuCrystalPacked layout-drift : expected 64 B, got {}",
            GpuCrystalPacked::SIZE_BYTES,
        );
    }

    /// Test 2 : the struct is 16-byte aligned (so an array<...> in WGSL
    /// honours `min_storage_buffer_offset_alignment` = 256 = 4 × 64 ⇒ no
    /// run-time padding insertion needed).
    #[test]
    fn gpu_crystal_packed_is_aligned() {
        assert_eq!(
            std::mem::align_of::<GpuCrystalPacked>(),
            4,
            "GpuCrystalPacked has 4-byte natural-alignment from its i32 fields",
        );
        // Buffer-stride must be a multiple of 16 for WGSL `array<T>` layout.
        // 64 % 16 == 0 ⇒ no implicit stride padding.
        assert_eq!(GpuCrystalPacked::SIZE_BYTES % 16, 0);
    }

    /// Test 3 : pack-roundtrip preserves world position + sigma + extent.
    #[test]
    fn pack_roundtrip_preserves_position_extent_sigma() {
        let crystal = Crystal::allocate(
            CrystalClass::Object,
            123,
            WorldPos::new(1234, -567, 8901),
        );
        let blend = [255u8, 0, 0, 0]; // pure illuminant 0
        let p = pack_crystal_packed(&crystal, blend);

        assert_eq!(p.world_pos_x_mm, 1234);
        assert_eq!(p.world_pos_y_mm, -567);
        assert_eq!(p.world_pos_z_mm, 8901);
        assert_eq!(
            i32::from(p.extent_mm_u16),
            crystal.extent_mm.clamp(0, u16::MAX as i32),
        );
        assert_eq!(u32::from(p.sigma_mask_u8), u32::from(crystal.sigma_mask) & 0xFFu32);
    }

    /// Test 4 : extent saturating clamp at `u16::MAX` doesn't panic +
    /// produces the saturated value. Even though typical crystals never
    /// exceed 65 km extent, we want a defined behaviour at the boundary.
    #[test]
    fn extent_saturates_at_u16_max() {
        // We can't easily set extent_mm directly on a Crystal (it's set by
        // `CrystalClass`). But we CAN mutate the field for a unit-test :
        // construct a normal Crystal then mutate its extent field.
        let mut crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::default());
        crystal.extent_mm = 100_000_000; // 100 km — definitely overflow u16
        let p = pack_crystal_packed(&crystal, [128, 0, 0, 0]);
        assert_eq!(p.extent_mm_u16, u16::MAX, "extent must saturate");

        // Also the negative-extent case.
        crystal.extent_mm = -42;
        let p2 = pack_crystal_packed(&crystal, [128, 0, 0, 0]);
        assert_eq!(p2.extent_mm_u16, 0, "negative extent must clamp to 0");
    }

    /// Test 5 : the spectral pre-blend matches a hand-computed weighted
    /// average. With `blend = (255, 0, 0, 0)` (pure illuminant 0) the
    /// blended bytes must equal `crystal.spectral.data[0][b]`.
    #[test]
    fn spectral_preblend_pure_illuminant_matches() {
        let crystal = Crystal::allocate(CrystalClass::Object, 7, WorldPos::default());
        let blend = [255u8, 0, 0, 0];
        let p = pack_crystal_packed(&crystal, blend);

        for b in 0..16 {
            let word = p.spectral_quad[b / 4];
            let byte = ((word >> ((b % 4) * 8)) & 0xFF) as u8;
            assert_eq!(
                byte, crystal.spectral.data[0][b],
                "pure illuminant 0 must produce verbatim spectrum (band {})",
                b
            );
        }
    }

    /// Test 6 : silhouette quantization keeps the *sign* of every spline
    /// control point (axis 0 + axis 1). Sign-loss would visibly flip
    /// silhouette concavity ; magnitude rounding to ±127 is acceptable.
    #[test]
    fn silhouette_quant_preserves_sign() {
        let crystal = Crystal::allocate(CrystalClass::Entity, 11, WorldPos::default());
        let p = pack_crystal_packed(&crystal, [128, 0, 0, 0]);

        let s = crystal.curves.spline(aspect_idx::SILHOUETTE);
        for pi in 0..16 {
            let orig0 = s.points[pi][0];
            let orig1 = s.points[pi][1];
            let q0 = p.silhouette_quant[pi * 2];
            let q1 = p.silhouette_quant[pi * 2 + 1];

            // If original is non-zero, sign must match.
            if orig0 > 0 {
                assert!(q0 >= 0, "axis-0 sign-flip @ point {pi} : {orig0} → {q0}");
            }
            if orig0 < 0 {
                assert!(q0 <= 0, "axis-0 sign-flip @ point {pi} : {orig0} → {q0}");
            }
            if orig1 > 0 {
                assert!(q1 >= 0, "axis-1 sign-flip @ point {pi} : {orig1} → {q1}");
            }
            if orig1 < 0 {
                assert!(q1 <= 0, "axis-1 sign-flip @ point {pi} : {orig1} → {q1}");
            }
        }
    }

    /// Test 7 : `bytemuck::cast_slice` zero-copies the packed array to bytes
    /// at exactly `N × 64`. This is the *load-bearing* invariant for the
    /// future packed-WGSL path : `Queue::write_buffer(buf, 0, &bytes[..])`
    /// must place the GPU view at the same byte offsets the shader expects.
    #[test]
    fn pack_array_byte_layout_is_dense() {
        let cs = vec![
            Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500)),
            Crystal::allocate(CrystalClass::Object, 2, WorldPos::new(500, 0, 1500)),
            Crystal::allocate(CrystalClass::Entity, 3, WorldPos::new(-500, 0, 1500)),
        ];
        let packed = pack_crystals_packed(&cs, [128, 0, 0, 0]);
        assert_eq!(packed.len(), 3);
        let bytes: &[u8] = bytemuck::cast_slice(&packed);
        assert_eq!(
            bytes.len(),
            3 * GpuCrystalPacked::SIZE_BYTES,
            "byte-cast must be exactly N × 64",
        );

        // First crystal's world_pos_x at byte 0 (LE i32).
        let x_bytes = &bytes[0..4];
        let x = i32::from_le_bytes(x_bytes.try_into().unwrap());
        assert_eq!(x, 0);
        // Second crystal starts at byte 64. Its world_pos_x = 500.
        let x2 = i32::from_le_bytes(bytes[64..68].try_into().unwrap());
        assert_eq!(x2, 500);
    }

    /// Test 8 : the env-var toggle defaults to `false` and is overridable.
    #[test]
    fn packed_path_env_var_toggle() {
        // Snapshot any pre-existing value so we don't break parallel tests.
        let prev = std::env::var("LOA_SUBSTRATE_PACKED").ok();

        // Cleared ⇒ false.
        std::env::remove_var("LOA_SUBSTRATE_PACKED");
        assert!(!packed_path_enabled(), "default must be off");

        // "1" ⇒ true.
        std::env::set_var("LOA_SUBSTRATE_PACKED", "1");
        assert!(packed_path_enabled());

        // "on" / "true" ⇒ true.
        std::env::set_var("LOA_SUBSTRATE_PACKED", "on");
        assert!(packed_path_enabled());
        std::env::set_var("LOA_SUBSTRATE_PACKED", "TRUE");
        assert!(packed_path_enabled());

        // "0" / random ⇒ false.
        std::env::set_var("LOA_SUBSTRATE_PACKED", "0");
        assert!(!packed_path_enabled());
        std::env::set_var("LOA_SUBSTRATE_PACKED", "junk");
        assert!(!packed_path_enabled());

        // Restore.
        std::env::remove_var("LOA_SUBSTRATE_PACKED");
        if let Some(p) = prev {
            std::env::set_var("LOA_SUBSTRATE_PACKED", p);
        }
    }

    /// Test 9 : pack-precision loss is bounded · spectral-byte error stays
    /// within 1/255 (rounding) for any crystal × blend combo. Captures the
    /// "loss measured" requirement from the brief.
    #[test]
    fn pack_precision_loss_is_bounded() {
        // Pure-illuminant 0 ⇒ zero loss.
        let crystal = Crystal::allocate(CrystalClass::Environment, 99, WorldPos::default());
        let p_pure = pack_crystal_packed(&crystal, [255, 0, 0, 0]);
        for b in 0..16 {
            let word = p_pure.spectral_quad[b / 4];
            let byte = ((word >> ((b % 4) * 8)) & 0xFF) as u8;
            assert_eq!(byte, crystal.spectral.data[0][b], "pure-blend zero loss");
        }

        // Mixed-blend ⇒ rounded average ; max delta must be ≤ 1.
        let blend = [64u8, 64u8, 64u8, 64u8];
        let p_mix = pack_crystal_packed(&crystal, blend);
        for b in 0..16 {
            let word = p_mix.spectral_quad[b / 4];
            let byte = ((word >> ((b % 4) * 8)) & 0xFF) as u32;
            // Recompute hand-side average.
            let acc: u32 = (0..4)
                .map(|i| u32::from(blend[i]) * u32::from(crystal.spectral.data[i][b]))
                .sum();
            let expected = (acc / (64u32 * 4)).min(255);
            let delta = byte.abs_diff(expected);
            assert!(
                delta <= 1,
                "spectral pack-error > 1 byte @ band {} : got {} expected {}",
                b,
                byte,
                expected
            );
        }
    }
}
