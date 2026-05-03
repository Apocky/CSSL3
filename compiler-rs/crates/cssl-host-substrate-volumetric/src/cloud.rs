//! § cloud — `build_voxel_cloud` and friends.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-L5-VOXEL
//!
//! The volumetric paradigm's central API. Given a slice of `Crystal` (the
//! substrate-state for one frame), produce a sparse VOLUMETRIC POINT-CLOUD
//! that the GPU splat-shader will project onto the framebuffer.
//!
//! § HOW EACH CRYSTAL EMITS CELLS
//!
//! Each crystal contributes a Halton-jittered cluster of cells inside its
//! bounding extent. The cell-count scales with `crystal.extent_mm` so an
//! Environment crystal (32 m extent) emits an order-of-magnitude more cells
//! than an Object crystal (2 m extent). Each cell's :
//!
//!   - position : world_pos + jittered offset within extent
//!   - emission : crystal's spectral-LUT projected through observer's
//!                illuminant blend, modulated by the cell's local-position
//!                inside the crystal (gradient → glowing core)
//!   - hdc fingerprint : low 4 bytes of crystal.hdc.words[0] (replay-stable)
//!   - sigma snapshot : crystal.sigma_mask at emission time
//!
//! § DENSITY BUDGET
//!
//! `MAX_CLOUD_POINTS` caps the total cell-count so a 1000-crystal scene
//! cannot blow up the GPU buffer. If the budget is exceeded, per-crystal
//! cell-counts are scaled down PROPORTIONALLY so every visible crystal
//! still appears (no silent culling).
//!
//! § Σ-MASK GATING
//!
//! If `crystal.aspect_permitted(0)` is false (silhouette revoked), the
//! crystal contributes ZERO cells. The same applies if an `ObserverCoord`
//! is supplied AND its `permits_aspect(0)` is false.

use cssl_host_alien_materialization::observer::ObserverCoord;
use cssl_host_crystallization::spectral::{project_to_srgb, IlluminantBlend};
use cssl_host_crystallization::{Crystal, CrystalClass};

use crate::voxel::{VoxelEmission, VoxelPoint};

/// Maximum cells in a single cloud (host-side budget). 1 MiB at 16 B/point
/// = 65536 ; we choose `MAX_CLOUD_POINTS = 65536` so the cloud fits in a
/// single 2 MiB GPU buffer including 32 B/point GPU pack.
pub const MAX_CLOUD_POINTS: usize = 65_536;

/// Default cells emitted per Object/Entity/Behavior/Event/Recipe/Inherit
/// crystal. Picked to give a visible-but-not-overwhelming cluster at 2 m
/// extent.
pub const DEFAULT_CRYSTAL_DENSITY: u32 = 64;

/// Default cells emitted per Environment crystal (32 m extent).
/// 16× density for 16× extent : matches density-per-volume.
pub const DEFAULT_ENV_DENSITY: u32 = 1024;

/// Default cells emitted per Aura crystal (volumetric atmosphere).
pub const DEFAULT_AURA_DENSITY: u32 = 256;

/// Sentinel fingerprint returned by `build_voxel_cloud(&[])`. Distinct from
/// any real cloud-fingerprint so callers can detect the empty case without
/// inspecting `points.len()`.
pub const EMPTY_CLOUD_FINGERPRINT: u32 = 0xFFFF_FFFF;

/// Per-cloud statistics for telemetry + tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CloudStats {
    /// Number of cells actually emitted into the cloud.
    pub cells_emitted: u32,
    /// Number of crystals on input.
    pub crystals_in: u32,
    /// Number of crystals dropped by Σ-mask gating.
    pub crystals_gated_out: u32,
    /// Number of crystals contributing cells.
    pub crystals_contributing: u32,
    /// Density-budget scale factor in q1.31 fixed-point. 1.0 = full density.
    pub density_scale_q31: u32,
}

/// The output of `build_voxel_cloud`. A flat `Vec<VoxelPoint>` plus
/// statistics + a fingerprint (BLAKE3-truncated) for replay-equality.
#[derive(Debug, Clone)]
pub struct VoxelCloudHandle {
    pub points: Vec<VoxelPoint>,
    pub stats: CloudStats,
    /// BLAKE3-truncated u32 of all (crystal-fingerprint || cell-positions).
    /// Replay-equality on this hash. `EMPTY_CLOUD_FINGERPRINT` for empty.
    pub fingerprint: u32,
}

/// Build a voxel-cloud from a crystal-slice. No observer-gating ; each
/// crystal's own Σ-mask is the only filter.
pub fn build_voxel_cloud(crystals: &[Crystal]) -> VoxelCloudHandle {
    build_voxel_cloud_inner(crystals, None)
}

/// Build a voxel-cloud from a crystal-slice + observer. The observer's
/// Σ-mask is AND-ed with each crystal's silhouette permission ; observers
/// with `permits_aspect(0) == false` see ZERO cells (privacy default).
pub fn build_voxel_cloud_with_observer(
    crystals: &[Crystal],
    observer: &ObserverCoord,
) -> VoxelCloudHandle {
    build_voxel_cloud_inner(crystals, Some(observer))
}

fn build_voxel_cloud_inner(
    crystals: &[Crystal],
    observer: Option<&ObserverCoord>,
) -> VoxelCloudHandle {
    if crystals.is_empty() {
        return VoxelCloudHandle {
            points: Vec::new(),
            stats: CloudStats::default(),
            fingerprint: EMPTY_CLOUD_FINGERPRINT,
        };
    }

    // Observer-side gate : if the observer's silhouette aspect is denied,
    // nothing is visible. Privacy axiom — no bypass.
    let observer_permits = observer.map_or(true, |o| o.permits_aspect(0));
    if !observer_permits {
        return VoxelCloudHandle {
            points: Vec::new(),
            stats: CloudStats {
                crystals_in: crystals.len() as u32,
                crystals_gated_out: crystals.len() as u32,
                ..CloudStats::default()
            },
            fingerprint: EMPTY_CLOUD_FINGERPRINT,
        };
    }

    // Use observer illuminant if supplied ; otherwise day default.
    let blend = observer.map_or(IlluminantBlend::day(), |o| o.illuminant_blend);

    // Pass 1 : compute per-crystal cell counts + total budget.
    let mut per_crystal_count: Vec<u32> = Vec::with_capacity(crystals.len());
    let mut total: u64 = 0;
    let mut gated_out: u32 = 0;
    for c in crystals {
        if !c.aspect_permitted(0) {
            per_crystal_count.push(0);
            gated_out += 1;
            continue;
        }
        let n = density_for_class(c.class);
        per_crystal_count.push(n);
        total += u64::from(n);
    }

    // If we'd exceed the budget, scale every count proportionally.
    let (counts, density_scale_q31) = if total > MAX_CLOUD_POINTS as u64 {
        let scale_q31 =
            ((MAX_CLOUD_POINTS as u128) << 31) / (total as u128).max(1);
        let scale_q31_u32 = scale_q31.min(u32::MAX as u128) as u32;
        let scaled: Vec<u32> = per_crystal_count
            .iter()
            .map(|n| {
                let scaled =
                    (u64::from(*n) * u64::from(scale_q31_u32)) >> 31;
                scaled as u32
            })
            .collect();
        (scaled, scale_q31_u32)
    } else {
        (per_crystal_count, 0x8000_0000_u32) // 1.0 in q1.31
    };

    // Pass 2 : emit cells.
    let mut points: Vec<VoxelPoint> = Vec::with_capacity(MAX_CLOUD_POINTS.min(total as usize));
    let mut crystals_contributing = 0u32;
    for (c, &n) in crystals.iter().zip(counts.iter()) {
        if n == 0 {
            continue;
        }
        crystals_contributing += 1;
        emit_crystal_cells(c, n, blend, &mut points);
    }

    // Pass 3 : fingerprint (replay-determinism contract).
    let fingerprint = compute_fingerprint(&points, crystals);
    let cells_emitted = points.len() as u32;

    VoxelCloudHandle {
        points,
        stats: CloudStats {
            cells_emitted,
            crystals_in: crystals.len() as u32,
            crystals_gated_out: gated_out,
            crystals_contributing,
            density_scale_q31,
        },
        fingerprint,
    }
}

/// Per-class default density. Environment is 16× because volumetric ; Aura
/// is 4× because atmospheric ; rest are baseline.
fn density_for_class(class: CrystalClass) -> u32 {
    match class {
        CrystalClass::Environment => DEFAULT_ENV_DENSITY,
        CrystalClass::Aura => DEFAULT_AURA_DENSITY,
        _ => DEFAULT_CRYSTAL_DENSITY,
    }
}

/// Emit `n` cells for one crystal into the cloud. Each cell is positioned
/// inside the crystal's extent via Halton-jitter (deterministic, low-discrepancy).
fn emit_crystal_cells(
    crystal: &Crystal,
    n: u32,
    blend: IlluminantBlend,
    points: &mut Vec<VoxelPoint>,
) {
    let base_rgb = project_to_srgb(&crystal.spectral, blend);
    // HDC fingerprint = low 32 bits of word 0 (stable per-crystal).
    let hdc_fingerprint = (crystal.hdc.words[0] & 0xFFFF_FFFF) as u32;

    // Halton-jitter requires a stable per-crystal seed ; we derive it from
    // the crystal fingerprint so re-running on the same crystal yields the
    // same point cluster.
    let seed = crystal.fingerprint;

    let extent = crystal.extent_mm;
    let half_ext_f32 = (extent as f32) / 2.0;
    for i in 0..n {
        // Halton-2/3/5 in [0, 1) projected to [-extent/2, extent/2]^3.
        let h2 = halton(i, 2);
        let h3 = halton(i, 3);
        let h5 = halton(i, 5);

        let dx = ((h2 * 2.0 - 1.0) * half_ext_f32) as i32;
        let dy = ((h3 * 2.0 - 1.0) * half_ext_f32) as i32;
        let dz = ((h5 * 2.0 - 1.0) * half_ext_f32) as i32;

        // Distance-from-center (normalized in 0..1) → glow falloff.
        let d2 = (h2 - 0.5).powi(2) + (h3 - 0.5).powi(2) + (h5 - 0.5).powi(2);
        let glow = (1.0 - (d2 * 4.0).min(1.0)).clamp(0.0, 1.0);
        let alpha = (glow * 255.0) as u8;

        // Per-cell color = base_rgb modulated by glow (denser cells brighter).
        let rgb = [
            ((base_rgb[0] as f32) * (0.5 + 0.5 * glow)) as u8,
            ((base_rgb[1] as f32) * (0.5 + 0.5 * glow)) as u8,
            ((base_rgb[2] as f32) * (0.5 + 0.5 * glow)) as u8,
        ];

        let local_index = (i & 0xFFFF) as u16;
        // local_seed_mix = seed XOR i is purely informational ; we use it
        // to keep the alpha varied across local-index without rng.
        let alpha = alpha.wrapping_add(((seed ^ i) & 0x1F) as u8);

        points.push(VoxelPoint {
            world_x_mm: crystal.world_pos.x_mm.saturating_add(dx),
            world_y_mm: crystal.world_pos.y_mm.saturating_add(dy),
            world_z_mm: crystal.world_pos.z_mm.saturating_add(dz),
            source_crystal: crystal.handle,
            emission: VoxelEmission { rgb, alpha },
            hdc_fingerprint,
            local_index,
            sigma_mask: crystal.sigma_mask,
            _pad: 0,
        });
    }
}

/// Halton low-discrepancy sequence ; deterministic + replay-stable.
fn halton(index: u32, base: u32) -> f32 {
    let mut f = 1.0f32;
    let mut r = 0.0f32;
    let mut i = index + 1; // Halton is 1-indexed.
    let base_f = base as f32;
    while i > 0 {
        f /= base_f;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

/// Compute a stable fingerprint over (per-crystal-fingerprint ‖ per-cell-pos).
/// BLAKE3-truncated u32 ; NEVER returns the EMPTY sentinel for non-empty
/// clouds (we hash one extra byte to disambiguate).
fn compute_fingerprint(points: &[VoxelPoint], crystals: &[Crystal]) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"voxel-cloud-v1");
    for c in crystals {
        h.update(&c.fingerprint.to_le_bytes());
        h.update(&c.handle.to_le_bytes());
    }
    h.update(&(points.len() as u32).to_le_bytes());
    for p in points {
        h.update(&p.world_x_mm.to_le_bytes());
        h.update(&p.world_y_mm.to_le_bytes());
        h.update(&p.world_z_mm.to_le_bytes());
        h.update(&p.emission.rgb);
        h.update(&[p.emission.alpha]);
        h.update(&p.hdc_fingerprint.to_le_bytes());
    }
    let digest: [u8; 32] = h.finalize().into();
    let mut fp = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
    // Avoid EMPTY sentinel collision (1 in 4 billion).
    if fp == EMPTY_CLOUD_FINGERPRINT {
        fp = fp.wrapping_sub(1);
    }
    fp
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::WorldPos;

    #[test]
    fn density_for_class_environment_largest() {
        assert!(density_for_class(CrystalClass::Environment) > density_for_class(CrystalClass::Object));
        assert!(density_for_class(CrystalClass::Aura) > density_for_class(CrystalClass::Object));
    }

    #[test]
    fn halton_first_term_is_inv_base() {
        // Halton(0, 2) = 1/2 ; Halton(0, 3) = 1/3
        let h2 = halton(0, 2);
        let h3 = halton(0, 3);
        assert!((h2 - 0.5).abs() < 1e-5);
        assert!((h3 - 1.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn halton_in_unit_interval() {
        for i in 0..256 {
            let h = halton(i, 2);
            assert!(h >= 0.0 && h < 1.0, "halton({i}, 2) = {h} not in [0,1)");
        }
    }

    #[test]
    fn observer_revoke_zeros_cloud() {
        let crystals = vec![Crystal::allocate(
            CrystalClass::Object,
            1,
            WorldPos::new(0, 0, 1500),
        )];
        let mut obs = ObserverCoord::default();
        // Strip the silhouette bit.
        obs.sigma_mask_token = 0xFFFF_FFFE;
        let cloud = build_voxel_cloud_with_observer(&crystals, &obs);
        assert_eq!(cloud.points.len(), 0);
        // Crystals all gated-out from observer side.
        assert_eq!(cloud.stats.crystals_gated_out, 1);
    }

    #[test]
    fn cloud_respects_max_budget() {
        // Fill the cloud past MAX_CLOUD_POINTS by spawning enough Environment
        // crystals (each 1024 cells) — 100 × 1024 = 102_400 > 65_536.
        let mut crystals = Vec::new();
        for i in 0..100 {
            crystals.push(Crystal::allocate(
                CrystalClass::Environment,
                i,
                WorldPos::new(0, 0, 1500 + (i as i32) * 100),
            ));
        }
        let cloud = build_voxel_cloud(&crystals);
        assert!(
            cloud.points.len() <= MAX_CLOUD_POINTS,
            "cloud over-budget: {}",
            cloud.points.len()
        );
    }

    #[test]
    fn empty_fingerprint_is_sentinel() {
        let cloud = build_voxel_cloud(&[]);
        assert_eq!(cloud.fingerprint, EMPTY_CLOUD_FINGERPRINT);
    }

    #[test]
    fn nonempty_fingerprint_not_sentinel() {
        let crystals = vec![Crystal::allocate(
            CrystalClass::Object,
            1,
            WorldPos::new(0, 0, 1500),
        )];
        let cloud = build_voxel_cloud(&crystals);
        assert_ne!(cloud.fingerprint, EMPTY_CLOUD_FINGERPRINT);
    }
}
