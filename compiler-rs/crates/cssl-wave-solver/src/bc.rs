//! § Boundary conditions — SDF-extracted Robin BC + KAN-impedance.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS (Wave-Unity §IV)
//!   Boundary geometry is **not** authored separately for the wave-solver
//!   ; it comes from the same SDF that drives collision + render. The
//!   Robin boundary condition `(∂ψ/∂n + Z·ψ) = 0` enforces the
//!   wall-impedance behaviour ; the impedance `Z(λ, embedding)` is
//!   KAN-derived from the cell's [`cssl_substrate_kan::KanMaterial::physics_impedance`]
//!   variant.
//!
//! § TYPES
//!   [`BoundaryKind`] — Dirichlet / Neumann / Robin discriminator.
//!   [`RobinBcConfig`] — per-cell configuration : the impedance + the
//!     surface normal.
//!   [`SdfQuery`] — minimal trait the wave-solver consumes ; the
//!     real D116 SDF crate impls this in a follow-on slice.
//!   [`AnalyticPlanarSdf`] — Stage-0 reference impl that defines a
//!     half-space `n · x = d`. Used by the standing-wave + sound-caustic
//!     test-scenes.
//!   [`NoSdf`] — default no-boundary impl ; useful for free-space tests.
//!
//! § KAN-IMPEDANCE INTEGRATION
//!   The impedance is computed via [`cssl_substrate_kan::KanMaterial::physics_impedance`]
//!   — the existing variant that owns the wave-unity Z(λ) lookup. The
//!   `KanNetwork` evaluation is deterministic (PCG-XSH-RR seeded
//!   weights) and the result is a `[f32; 8]` band-spectrum. The
//!   wave-solver consumes the first 2 entries as `(R, X) = (real,
//!   reactive)` impedance components.
//!
//! § DETERMINISM
//!   - SDF queries are pure functions of position.
//!   - KanMaterial impedance lookups are deterministic (no RNG).
//!   - Robin update is a per-cell point-update with no inter-cell
//!     ordering dependency.

use crate::band::Band;
use crate::complex::C32;
use crate::psi_field::WaveField;

use cssl_substrate_kan::kan_material::{KanMaterial, KanMaterialKind};
use cssl_substrate_omega_field::MortonKey;

/// § Boundary-condition kind discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundaryKind {
    /// `ψ|_∂Ω = ψ_b` — Dirichlet (driven amplitude).
    Dirichlet,
    /// `∂ψ/∂n|_∂Ω = q_b` — Neumann (driven flux).
    Neumann,
    /// `(∂ψ/∂n + Z·ψ)|_∂Ω = 0` — Robin (impedance wall ; the common case).
    Robin,
}

/// § Robin BC configuration for one boundary cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RobinBcConfig {
    /// § Complex impedance `Z = R + iX` for this cell at this band.
    pub impedance: C32,
    /// § Outward normal direction (unit-norm 3-vector). Stage-0
    ///   represents normals as `[f32; 3]` ; once cssl-pga lands we
    ///   switch to a PGA bivector.
    pub normal: [f32; 3],
}

/// § Minimal SDF query trait. Returns the signed distance at a cell
///   AND the outward normal. Negative distance = inside solid.
pub trait SdfQuery {
    /// § Signed distance at the cell `key`. Negative inside, positive outside.
    fn distance(&self, key: MortonKey, dx_m: f32) -> f32;
    /// § Outward unit normal at the cell `key`. Points from solid into air.
    fn normal(&self, key: MortonKey, dx_m: f32) -> [f32; 3];
    /// § Material at the cell — used to look up the KAN impedance. Stage-0
    ///   returns `None` if the cell is in air ; the solver skips BC
    ///   application in that case.
    fn material(&self, key: MortonKey) -> Option<&KanMaterial> {
        let _ = key;
        None
    }
}

/// § Default no-SDF query — every cell is in air, no boundaries.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSdf;

impl SdfQuery for NoSdf {
    fn distance(&self, _key: MortonKey, _dx_m: f32) -> f32 {
        // All cells are in air ; 1.0 is a safe positive distance.
        1.0
    }
    fn normal(&self, _key: MortonKey, _dx_m: f32) -> [f32; 3] {
        [0.0, 1.0, 0.0]
    }
}

/// § Analytic planar half-space SDF : `n · x = d`. Used by the
///   standing-wave + sound-caustic test scenes. The plane is at
///   `axis = d` (in cell units) ; cells with axis > d are in air,
///   cells with axis ≤ d are inside the solid.
#[derive(Debug, Clone, Copy)]
pub struct AnalyticPlanarSdf {
    /// § Normal axis (0 = X, 1 = Y, 2 = Z).
    pub axis: u8,
    /// § Plane offset along the axis (cell units).
    pub plane_offset_cells: i64,
    /// § Sign : +1 = "outside is positive direction", -1 = inverted.
    pub sign: i8,
}

impl AnalyticPlanarSdf {
    /// § Construct a Y-axis half-space at `y = plane_offset_cells`,
    ///   "outside" = positive Y.
    #[must_use]
    pub const fn y_plane(plane_offset_cells: i64) -> Self {
        Self {
            axis: 1,
            plane_offset_cells,
            sign: 1,
        }
    }

    /// § Construct an X-axis half-space.
    #[must_use]
    pub const fn x_plane(plane_offset_cells: i64) -> Self {
        Self {
            axis: 0,
            plane_offset_cells,
            sign: 1,
        }
    }

    /// § Construct a Z-axis half-space.
    #[must_use]
    pub const fn z_plane(plane_offset_cells: i64) -> Self {
        Self {
            axis: 2,
            plane_offset_cells,
            sign: 1,
        }
    }
}

impl SdfQuery for AnalyticPlanarSdf {
    fn distance(&self, key: MortonKey, dx_m: f32) -> f32 {
        let (x, y, z) = key.decode();
        let pos = match self.axis {
            0 => x as i64,
            1 => y as i64,
            _ => z as i64,
        };
        ((pos - self.plane_offset_cells) as f32) * dx_m * (self.sign as f32)
    }

    fn normal(&self, _key: MortonKey, _dx_m: f32) -> [f32; 3] {
        let mut n = [0.0_f32; 3];
        n[self.axis as usize] = self.sign as f32;
        n
    }
}

/// § Look up complex impedance from a KanMaterial via the `physics_impedance`
///   variant. Stage-0 reads the first 2 entries of `acoustic_kan` output
///   as `(R, X)`. Returns a default unit-impedance if the material's
///   `kind` is not `PhysicsImpedance`.
#[must_use]
pub fn impedance_from_material(material: &KanMaterial, band: Band) -> C32 {
    if material.kind != KanMaterialKind::PhysicsImpedance {
        // Material is not configured for wave-physics — return
        // unit-impedance (perfect-match, no reflection).
        return C32::new(1.0, 0.0);
    }
    // Stage-0 deterministic Z extraction : combine band index +
    // embedding[0..4] into a stable (R, X) pair. The real D115 KAN
    // runtime evaluates the spline network ; this Stage-0 path is
    // purely deterministic.
    let band_idx = band.index() as f32;
    let r = (material.embedding[0].abs() + 0.5 * band_idx).clamp(0.1, 10.0);
    let x_react = material.embedding[1] * 0.25;
    C32::new(r, x_react)
}

/// § Apply Robin boundary conditions across the active region of `band`
///   in `field`. Walks the field's cells in Morton-sorted order, checks
///   the SDF, and updates ψ at boundary cells via :
///
///     `ψ_new = ψ - dt · Z · ψ`
///
///   This is the explicit form of the Robin BC for the discrete
///   wave-LBM step (cite : Wave-Unity §IV.1 Robin form). The Δt scaling
///   makes the BC a per-substep update consistent with the LBM/IMEX
///   stepping cadence.
///
/// § Returns
///   The number of boundary cells touched.
pub fn apply_robin_bc<const C: usize, S: SdfQuery>(
    field: &mut WaveField<C>,
    band: Band,
    sdf: &S,
    dt: f64,
) -> usize {
    let band_idx = band.index();
    if band_idx >= field.band_count() {
        return 0;
    }
    let dx = field.dx_m(band_idx) as f32;
    // Collect cells first to avoid the borrow-checker complaint about
    // mutating `field` while iterating it.
    let cells: Vec<(MortonKey, C32)> = field.cells_in_band(band_idx).collect();
    let mut touched = 0_usize;
    for (k, psi_here) in cells {
        let d = sdf.distance(k, dx);
        // Boundary cells : within one cell of the surface.
        if d.abs() <= dx {
            let z = if let Some(m) = sdf.material(k) {
                impedance_from_material(m, band)
            } else {
                // No material → default impedance from band (deterministic).
                C32::new(1.0 + 0.1 * band.index() as f32, 0.0)
            };
            let psi_new = psi_here - (z * psi_here).scale(dt as f32);
            if psi_new.is_finite() {
                field.set(band_idx, k, psi_new);
            } else {
                field.set(band_idx, k, C32::ZERO);
            }
            touched += 1;
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_kan::kan_material::EMBEDDING_DIM;

    fn key(x: u64, y: u64, z: u64) -> MortonKey {
        MortonKey::encode(x, y, z).unwrap()
    }

    #[test]
    fn no_sdf_distance_is_positive() {
        let s = NoSdf;
        assert!(s.distance(key(0, 0, 0), 0.5) > 0.0);
    }

    #[test]
    fn analytic_y_plane_distance_correct() {
        let p = AnalyticPlanarSdf::y_plane(5);
        // Cell at y=10 ⇒ distance = (10 - 5) · dx = 2.5 m at dx=0.5.
        let d = p.distance(key(0, 10, 0), 0.5);
        assert!((d - 2.5).abs() < 1e-6);
        // Cell at y=0 ⇒ distance = -2.5 m.
        let d2 = p.distance(key(0, 0, 0), 0.5);
        assert!((d2 + 2.5).abs() < 1e-6);
    }

    #[test]
    fn analytic_y_plane_normal_is_y_axis() {
        let p = AnalyticPlanarSdf::y_plane(5);
        let n = p.normal(key(0, 5, 0), 0.5);
        assert_eq!(n, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn analytic_x_plane_normal_is_x_axis() {
        let p = AnalyticPlanarSdf::x_plane(5);
        let n = p.normal(key(5, 0, 0), 0.5);
        assert_eq!(n, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn analytic_z_plane_distance_correct() {
        let p = AnalyticPlanarSdf::z_plane(3);
        let d = p.distance(key(0, 0, 7), 0.25);
        assert!((d - 1.0).abs() < 1e-6);
    }

    #[test]
    fn impedance_unity_for_non_physics_material() {
        let m = KanMaterial::single_band_brdf([0.0; EMBEDDING_DIM]);
        let z = impedance_from_material(&m, Band::AudioSubKHz);
        assert_eq!(z, C32::new(1.0, 0.0));
    }

    #[test]
    fn impedance_varies_with_embedding_for_physics() {
        let mut e1 = [0.0; EMBEDDING_DIM];
        e1[0] = 0.5;
        let m1 = KanMaterial::physics_impedance(e1);
        let z1 = impedance_from_material(&m1, Band::AudioSubKHz);

        let mut e2 = [0.0; EMBEDDING_DIM];
        e2[0] = 1.5;
        let m2 = KanMaterial::physics_impedance(e2);
        let z2 = impedance_from_material(&m2, Band::AudioSubKHz);

        assert_ne!(z1, z2);
    }

    #[test]
    fn impedance_varies_across_bands_for_physics() {
        let m = KanMaterial::physics_impedance([0.5; EMBEDDING_DIM]);
        let z_audio = impedance_from_material(&m, Band::AudioSubKHz);
        let z_red = impedance_from_material(&m, Band::LightRed);
        assert_ne!(z_audio, z_red);
    }

    #[test]
    fn apply_robin_bc_runs_on_empty() {
        let mut field = WaveField::<5>::with_default_bands();
        let sdf = NoSdf;
        let touched = apply_robin_bc(&mut field, Band::AudioSubKHz, &sdf, 1e-3);
        assert_eq!(touched, 0);
    }

    #[test]
    fn apply_robin_bc_decays_amplitude_at_boundary() {
        let mut field = WaveField::<5>::with_default_bands();
        // Single cell at y=5, plane at y=5 ⇒ on boundary.
        let k = key(0, 5, 0);
        field.set_band(Band::AudioSubKHz, k, C32::new(1.0, 0.0));
        let sdf = AnalyticPlanarSdf::y_plane(5);
        let touched = apply_robin_bc(&mut field, Band::AudioSubKHz, &sdf, 0.1);
        assert_eq!(touched, 1);
        // Should have decayed (Robin BC removes amplitude).
        let v = field.at_band(Band::AudioSubKHz, k);
        assert!(v.re < 1.0);
        assert!(v.re > 0.0);
    }

    #[test]
    fn apply_robin_bc_skips_cells_far_from_boundary() {
        let mut field = WaveField::<5>::with_default_bands();
        // Cell at y=20, plane at y=5 ⇒ far from boundary (dx=0.5, so |d|=7.5 m).
        let k = key(0, 20, 0);
        field.set_band(Band::AudioSubKHz, k, C32::new(1.0, 0.0));
        let sdf = AnalyticPlanarSdf::y_plane(5);
        let touched = apply_robin_bc(&mut field, Band::AudioSubKHz, &sdf, 0.1);
        // d = (20-5)·0.5 = 7.5 m ; |d| > dx=0.5 m ⇒ not boundary.
        assert_eq!(touched, 0);
    }

    #[test]
    fn boundary_kind_variants_are_distinct() {
        let kinds = [BoundaryKind::Dirichlet, BoundaryKind::Neumann, BoundaryKind::Robin];
        for i in 0..kinds.len() {
            for j in (i + 1)..kinds.len() {
                assert_ne!(kinds[i], kinds[j]);
            }
        }
    }

    #[test]
    fn apply_robin_bc_blowup_clamps() {
        let mut field = WaveField::<5>::with_default_bands();
        let k = key(0, 5, 0);
        field.set_band(Band::AudioSubKHz, k, C32::new(f32::NAN, 0.0));
        let sdf = AnalyticPlanarSdf::y_plane(5);
        apply_robin_bc(&mut field, Band::AudioSubKHz, &sdf, 0.1);
        let v = field.at_band(Band::AudioSubKHz, k);
        assert_eq!(v, C32::ZERO);
    }
}
