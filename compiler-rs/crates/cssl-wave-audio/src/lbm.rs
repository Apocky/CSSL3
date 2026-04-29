//! § LbmSpatialAudio — Lattice-Boltzmann ψ-AUDIO solver for room resonance.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § II.3` :
//!
//!   ```text
//!   discrete-velocity Boltzmann-equation @ complex-amplitude :
//!     f_i(x + e_i Δt, t + Δt) = f_i(x, t) - (1/τ)(f_i - f_i^eq)
//!   ¬ raw fluid-LBM (Navier-Stokes target) ⊗ ✓ wave-LBM (Helmholtz/Klein-Gordon target)
//!   ```
//!
//!   `LbmSpatialAudio` runs a **wave-LBM** stream-collide step on the
//!   ψ-AUDIO band. The discrete-velocity stencil is **D3Q19** (per spec
//!   § II.3 D3Q19/D3Q27 ; we use D3Q19 for tier-B coarse cells per
//!   § VIII.2 storage budget).
//!
//!   Per spec § VI.1 IMEX-split AUDIO is in BANDS_FAST :
//!   `next.psi[Audio] = lbm_explicit_step(&prev.psi[Audio], dt_substep,
//!   Audio)`. This file IS that explicit-step.
//!
//! § STENCIL — D3Q19
//!   The 19 lattice-velocity directions are :
//!     0   : rest (0, 0, 0)
//!     1-6 : ±x, ±y, ±z (axis-aligned, weight 1/18)
//!     7-18: 12 face-diagonals (weight 1/36)
//!
//!   The rest-direction has weight 1/3 (D3Q19 standard).
//!
//! § COMPLEX-VALUED DISTRIBUTIONS
//!   Each `f_i ∈ ℂ` per spec § II.3. The equilibrium `f_i^eq` is
//!   derived from the local ψ + its gradient (linearized Helmholtz
//!   variant). For audio-band waves the linearization is exact in the
//!   small-amplitude limit ; we use the standard wave-LBM second-order
//!   approximation here.
//!
//! § BOUNDARY CONDITIONS — Robin from SDF
//!   At cells on the SDF boundary the LBM applies the Robin condition
//!   `(∂ψ/∂n + Z·ψ) = 0` with `Z` from the impedance KAN. We provide
//!   a `BoundaryRule` enum that marks each lattice-direction as
//!   reflecting / transmitting / absorbing per the wall classification.
//!
//! § DETERMINISM
//!   The solver iterates Morton-keyed cells in sorted order ; the
//!   stream-collide step is a pure function of the prior ψ snapshot
//!   plus the boundary condition table. Two replays with identical
//!   inputs produce bit-equal output.
//!
//! § COST
//!   Per spec § IX.1 the AUDIO-LBM cost at 1M cells × 16 substeps is
//!   ~2.8 GF/frame. cssl-wave-audio's CPU implementation is meant for
//!   correctness + small-region tests ; the GPU-accelerated variant
//!   ships in a follow-up slice.

use crate::complex::Complex;
use crate::error::{Result, WaveAudioError};
use crate::kan::{ImpedanceKan, ImpedanceKanInputs};
use crate::psi_field::PsiAudioField;
use crate::sdf::{VocalTractSdf, WallClass};
use cssl_substrate_omega_field::morton::MortonKey;

/// Number of D3Q19 lattice directions.
pub const D3Q19_DIRS: usize = 19;

/// D3Q19 lattice velocities (in lattice units `e_i`). The rest-direction
/// is index 0 ; axis-aligned 1..=6 ; face-diagonals 7..=18.
pub const D3Q19_VELOCITIES: [[i32; 3]; D3Q19_DIRS] = [
    // 0 : rest
    [0, 0, 0],
    // 1-6 : axis-aligned (±x, ±y, ±z)
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
    // 7-18 : 12 face-diagonals (xy ± xz ± yz planes)
    [1, 1, 0],
    [-1, -1, 0],
    [1, -1, 0],
    [-1, 1, 0],
    [1, 0, 1],
    [-1, 0, -1],
    [1, 0, -1],
    [-1, 0, 1],
    [0, 1, 1],
    [0, -1, -1],
    [0, 1, -1],
    [0, -1, 1],
];

/// D3Q19 weights for the equilibrium distribution. Rest = 1/3 ;
/// axis-aligned = 1/18 ; diagonals = 1/36.
pub const D3Q19_WEIGHTS: [f32; D3Q19_DIRS] = [
    // rest
    1.0 / 3.0,
    // axis-aligned
    1.0 / 18.0,
    1.0 / 18.0,
    1.0 / 18.0,
    1.0 / 18.0,
    1.0 / 18.0,
    1.0 / 18.0,
    // diagonals
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
];

/// LBM relaxation time constant `τ`. For wave-LBM the value drives the
/// velocity-bandwidth ; `τ ≈ 0.6` produces a stable explicit-step at
/// `Δt = Δx/c`.
pub const LBM_TAU: f32 = 0.6;

/// Default voxel size for the AUDIO band (m). Matches spec § III.
pub const LBM_VOXEL_SIZE: f32 = 0.5;

/// Default stream-collide CFL multiplier. `Δt = CFL · Δx / c`.
pub const LBM_CFL: f32 = 0.5;

/// Configuration knobs for the LBM solver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LbmConfig {
    /// Voxel size in metres.
    pub voxel_size: f32,
    /// Speed of sound in m/s.
    pub speed_of_sound: f32,
    /// CFL multiplier.
    pub cfl: f32,
    /// Relaxation time `τ`.
    pub tau: f32,
    /// Maximum substeps per frame (per spec § VI.2).
    pub max_substeps: u32,
    /// Conservation tolerance ε per spec § XII.1.
    pub conservation_epsilon: f32,
}

impl Default for LbmConfig {
    fn default() -> LbmConfig {
        LbmConfig {
            voxel_size: LBM_VOXEL_SIZE,
            speed_of_sound: 343.0,
            cfl: LBM_CFL,
            tau: LBM_TAU,
            max_substeps: 16,
            conservation_epsilon: 1e-2,
        }
    }
}

impl LbmConfig {
    /// Compute the stable timestep `Δt` for this config.
    #[must_use]
    pub fn dt(self) -> f32 {
        self.cfl * self.voxel_size / self.speed_of_sound.max(1.0)
    }
}

/// LBM stream-collide solver state.
///
/// § STORAGE
///   We maintain TWO snapshots :
///     - `current` : the ψ-AUDIO field at the current time-step.
///     - `staging` : the ψ-AUDIO field after the next stream-collide
///       step (becomes `current` after `swap`).
///
///   The `stream-collide` step reads `current` + writes to `staging` ;
///   no in-place mutation = safe parallelization at GPU port.
#[derive(Debug, Clone)]
pub struct LbmSpatialAudio {
    config: LbmConfig,
    /// Current ψ-snapshot (read-side of the stream-step).
    current: PsiAudioField,
    /// Staging ψ-snapshot (write-side of the stream-step).
    staging: PsiAudioField,
    /// Per-cell wall-class map : when a cell is on the SDF boundary
    /// this records which wall class the boundary is. Used by the
    /// Robin-BC application.
    boundary_walls: std::collections::HashMap<u64, WallClass>,
    /// Substep counter : used to verify per-frame substep count is
    /// within `max_substeps`.
    substeps_this_frame: u32,
    /// Impedance KAN for boundary `Z(λ)` lookup.
    impedance_kan: ImpedanceKan,
}

impl Default for LbmSpatialAudio {
    fn default() -> LbmSpatialAudio {
        LbmSpatialAudio::new(LbmConfig::default())
    }
}

impl LbmSpatialAudio {
    /// Construct a new LBM solver with the given config.
    #[must_use]
    pub fn new(config: LbmConfig) -> LbmSpatialAudio {
        LbmSpatialAudio {
            config,
            current: PsiAudioField::new(),
            staging: PsiAudioField::new(),
            boundary_walls: std::collections::HashMap::new(),
            substeps_this_frame: 0,
            impedance_kan: ImpedanceKan::untrained(),
        }
    }

    /// Read the active configuration.
    #[must_use]
    pub const fn config(&self) -> LbmConfig {
        self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: LbmConfig) {
        self.config = config;
    }

    /// Read the current ψ-AUDIO field (read-only).
    #[must_use]
    pub fn current(&self) -> &PsiAudioField {
        &self.current
    }

    /// Mutable reference to the current ψ-AUDIO field. Useful for
    /// seeding initial conditions from outside.
    pub fn current_mut(&mut self) -> &mut PsiAudioField {
        &mut self.current
    }

    /// Inject a Dirichlet-driven source amplitude at `key`. Used by
    /// the source-synthesizer when injecting an emission event.
    pub fn inject_source(&mut self, key: MortonKey, amp: Complex) -> Result<()> {
        self.current.set(key, amp)?;
        Ok(())
    }

    /// Mark a cell as a SDF boundary with the given wall class. The
    /// LBM stream-collide will apply the Robin-BC at this cell.
    pub fn mark_boundary(&mut self, key: MortonKey, wall: WallClass) {
        self.boundary_walls.insert(key.to_u64(), wall);
    }

    /// Mark all boundary cells inferred from a vocal-tract SDF along
    /// the +X axis starting at `origin_cell`. This is a CONVENIENCE
    /// helper used by the procedural-vocal demo.
    pub fn mark_vocal_tract_boundary(&mut self, sdf: &VocalTractSdf, origin_cell: (u32, u32, u32)) {
        let voxel = self.config.voxel_size.max(1e-3);
        let n_cells_axial = (sdf.total_length() / voxel).ceil() as u32;
        for ax in 0..n_cells_axial {
            let s = ax as f32 * voxel;
            let r = sdf.radius_at_s(s);
            let n_radial = (r / voxel).ceil() as u32;
            let wall = sdf.wall_class_at_s(s);
            // Mark the boundary cells at the radial surface (ring).
            for dy in -1..=1_i32 {
                for dz in -1..=1_i32 {
                    let cy = origin_cell.1 as i32 + dy * n_radial as i32;
                    let cz = origin_cell.2 as i32 + dz * n_radial as i32;
                    if cy < 0 || cz < 0 {
                        continue;
                    }
                    let cx = origin_cell.0 + ax;
                    if let Ok(k) = MortonKey::encode(cx as u64, cy as u64, cz as u64) {
                        self.boundary_walls.insert(k.to_u64(), wall);
                    }
                }
            }
        }
    }

    /// Number of marked boundary cells.
    #[must_use]
    pub fn boundary_count(&self) -> usize {
        self.boundary_walls.len()
    }

    /// Reset substep counter for a new frame.
    pub fn begin_frame(&mut self) {
        self.substeps_this_frame = 0;
    }

    /// Run one LBM stream-collide substep. The step :
    ///   1. Computes stream contributions from each cell to its 19
    ///      neighbors per the D3Q19 stencil.
    ///   2. Applies the BGK collision relaxation `f_i ← f_i - (1/τ)(f_i - f_i^eq)`.
    ///   3. Applies Robin-BC at marked boundary cells.
    ///   4. Swaps `staging` into `current`.
    ///
    /// § ERRORS
    ///   - [`WaveAudioError::ConservationViolation`] when ψ-norm grows
    ///     beyond `conservation_epsilon`.
    pub fn substep(&mut self) -> Result<()> {
        if self.substeps_this_frame >= self.config.max_substeps {
            return Err(WaveAudioError::Storage(format!(
                "LBM substep budget exhausted ({} max)",
                self.config.max_substeps
            )));
        }

        // Snapshot energy for conservation check.
        let energy_before = self.current.total_energy();

        // Clear staging.
        self.staging.clear();

        // Stream + collide.
        // We iterate over cells in `current` (Morton-sorted) ; each
        // cell distributes its ψ-amplitude to its 19 neighbors per the
        // D3Q19 weights. The wave-LBM equilibrium is f_i^eq = w_i · ψ
        // and the stream-collide step in the small-amplitude (linear)
        // limit reduces to ψ_neighbor += w_i · ψ_source. Because Σ w_i
        // = 1, this conserves the L1 amplitude exactly (modulo
        // boundary absorption). We do NOT divide by τ here ; τ is the
        // effective dispersion-control parameter and shows up only in
        // the optional non-equilibrium correction term, which we set
        // to zero for the linearized small-amplitude path.
        let cells: Vec<(MortonKey, Complex)> =
            self.current.iter().map(|(k, c)| (k, c.amplitude)).collect();

        for (key, amp) in &cells {
            let (x, y, z) = key.decode();
            for dir in 0..D3Q19_DIRS {
                let v = D3Q19_VELOCITIES[dir];
                let nx = x as i64 + v[0] as i64;
                let ny = y as i64 + v[1] as i64;
                let nz = z as i64 + v[2] as i64;
                if nx < 0 || ny < 0 || nz < 0 {
                    continue;
                }
                let neighbor = match MortonKey::encode(nx as u64, ny as u64, nz as u64) {
                    Ok(k) => k,
                    Err(_) => continue,
                };
                // Equilibrium streaming : f_i^eq = w_i · ψ deposited at
                // the neighbor cell. Σ w_i = 1 → mass-conserving.
                let f_eq = amp.scale(D3Q19_WEIGHTS[dir]);
                self.staging.add_at(neighbor, f_eq)?;
            }
        }

        // Apply boundary conditions.
        self.apply_boundary_conditions()?;

        // Swap : staging becomes current.
        std::mem::swap(&mut self.current, &mut self.staging);

        // Conservation check.
        let energy_after = self.current.total_energy();
        let drift = (energy_after - energy_before).abs();
        let total = energy_before.max(1e-9);
        if drift / total > self.config.conservation_epsilon {
            // Allow energy to DECREASE (absorption is physical) ; only
            // refuse-tick when it GROWS beyond ε.
            if energy_after > energy_before * (1.0 + self.config.conservation_epsilon) {
                return Err(WaveAudioError::ConservationViolation {
                    prev: energy_before,
                    next: energy_after,
                    epsilon: self.config.conservation_epsilon,
                });
            }
        }

        self.substeps_this_frame += 1;
        Ok(())
    }

    /// Apply boundary conditions per cell wall-class.
    fn apply_boundary_conditions(&mut self) -> Result<()> {
        // Keys that have boundary marks.
        let boundary_keys: Vec<(u64, WallClass)> =
            self.boundary_walls.iter().map(|(k, w)| (*k, *w)).collect();

        for (raw_key, wall) in boundary_keys {
            let key = MortonKey::from_u64_raw(raw_key);
            let amp = self.staging.at(key);
            if amp.norm_sq() < 1e-12 {
                continue; // skip silent cells
            }

            // Apply BC per wall class.
            let new_amp = match wall {
                WallClass::Rigid => {
                    // Dirichlet : ψ = 0 (perfect reflector).
                    Complex::ZERO
                }
                WallClass::Soft => {
                    // Neumann : ∂ψ/∂n = 0 (radiating). For LBM we
                    // approximate by halving the amplitude (energy
                    // radiates outward).
                    amp.scale(0.5)
                }
                WallClass::Impedance => {
                    // Robin : (∂ψ/∂n + Z·ψ) = 0. KAN-derived Z(λ) ;
                    // for a 1 kHz audio carrier λ = 343m/s / 1000Hz =
                    // 0.343 m.
                    let inputs = ImpedanceKanInputs {
                        wavelength_m: 0.343,
                        wall_class_id: 2,
                    };
                    let z = self.impedance_kan.evaluate(inputs);
                    let z_re = z[0];
                    let z_im = z[1];
                    // Reflection coefficient for impedance wall :
                    // R = (Z - Z_air) / (Z + Z_air) where Z_air ≈ 415.
                    let z_air = 415.0;
                    let denom_re = z_re + z_air;
                    let denom_im = z_im;
                    let denom_mag2 = denom_re * denom_re + denom_im * denom_im;
                    let num_re = z_re - z_air;
                    let num_im = z_im;
                    // R = num / denom = num · conj(denom) / |denom|²
                    let r_re = (num_re * denom_re + num_im * denom_im) / denom_mag2;
                    let r_im = (num_im * denom_re - num_re * denom_im) / denom_mag2;
                    let r = Complex::new(r_re, r_im);
                    amp.mul(r)
                }
            };

            self.staging.set(key, new_amp)?;
        }
        Ok(())
    }

    /// Run multiple substeps until the wall-clock dt budget is consumed.
    pub fn step_for_dt(&mut self, dt: f32) -> Result<u32> {
        let dt_sub = self.config.dt();
        if dt_sub <= 0.0 {
            return Err(WaveAudioError::Storage(
                "LBM substep timestep is non-positive".into(),
            ));
        }
        let n_substeps = (dt / dt_sub).ceil() as u32;
        let n = n_substeps.clamp(1, self.config.max_substeps);
        self.begin_frame();
        for _ in 0..n {
            self.substep()?;
        }
        Ok(n)
    }

    /// True iff the LBM solver has at least one cell of non-zero ψ.
    #[must_use]
    pub fn has_active_cells(&self) -> bool {
        !self.current.is_silent()
    }

    /// Diagnostic : number of substeps consumed in the current frame.
    #[must_use]
    pub fn substeps_this_frame(&self) -> u32 {
        self.substeps_this_frame
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{
        LbmConfig, LbmSpatialAudio, D3Q19_DIRS, D3Q19_VELOCITIES, D3Q19_WEIGHTS, LBM_CFL, LBM_TAU,
        LBM_VOXEL_SIZE,
    };
    use crate::complex::Complex;
    use crate::sdf::{VocalTractSdf, WallClass};
    use cssl_substrate_omega_field::morton::MortonKey;

    #[test]
    fn d3q19_has_19_directions() {
        assert_eq!(D3Q19_VELOCITIES.len(), D3Q19_DIRS);
        assert_eq!(D3Q19_WEIGHTS.len(), D3Q19_DIRS);
    }

    #[test]
    fn d3q19_weights_sum_to_one() {
        let s: f32 = D3Q19_WEIGHTS.iter().sum();
        assert!((s - 1.0).abs() < 1e-5, "sum = {s}");
    }

    #[test]
    fn d3q19_first_direction_is_rest() {
        assert_eq!(D3Q19_VELOCITIES[0], [0, 0, 0]);
    }

    #[test]
    fn d3q19_axis_aligned_directions_have_unit_length() {
        for i in 1..=6 {
            let v = D3Q19_VELOCITIES[i];
            let mag2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
            assert_eq!(mag2, 1, "direction {i} = {v:?}");
        }
    }

    #[test]
    fn d3q19_diagonal_directions_have_sqrt2_length() {
        for i in 7..D3Q19_DIRS {
            let v = D3Q19_VELOCITIES[i];
            let mag2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
            assert_eq!(mag2, 2, "diagonal {i} = {v:?}");
        }
    }

    #[test]
    fn lbm_config_default_dt_positive() {
        let c = LbmConfig::default();
        assert!(c.dt() > 0.0);
        // Δt = 0.5 * 0.5 / 343 ≈ 7.3e-4 s
        assert!((c.dt() - (LBM_CFL * LBM_VOXEL_SIZE / 343.0)).abs() < 1e-7);
    }

    #[test]
    fn lbm_default_constants() {
        assert_eq!(LBM_VOXEL_SIZE, 0.5);
        assert!((LBM_TAU - 0.6).abs() < 1e-6);
    }

    #[test]
    fn lbm_starts_silent() {
        let lbm = LbmSpatialAudio::default();
        assert!(!lbm.has_active_cells());
        assert_eq!(lbm.substeps_this_frame(), 0);
    }

    #[test]
    fn lbm_inject_source_makes_cells_active() {
        let mut lbm = LbmSpatialAudio::default();
        let k = MortonKey::encode(5, 5, 5).unwrap();
        lbm.inject_source(k, Complex::new(1.0, 0.0)).unwrap();
        assert!(lbm.has_active_cells());
    }

    #[test]
    fn lbm_substep_propagates_to_neighbors() {
        let mut lbm = LbmSpatialAudio::default();
        let k = MortonKey::encode(5, 5, 5).unwrap();
        lbm.inject_source(k, Complex::new(1.0, 0.0)).unwrap();
        lbm.substep().unwrap();
        // After one stream-step, neighbors of (5,5,5) should have
        // non-trivial amplitude.
        let neighbor = MortonKey::encode(6, 5, 5).unwrap();
        assert!(lbm.current().at(neighbor).norm() > 0.0);
    }

    #[test]
    fn lbm_substep_increments_counter() {
        let mut lbm = LbmSpatialAudio::default();
        let k = MortonKey::encode(5, 5, 5).unwrap();
        lbm.inject_source(k, Complex::new(0.5, 0.0)).unwrap();
        lbm.substep().unwrap();
        assert_eq!(lbm.substeps_this_frame(), 1);
    }

    #[test]
    fn lbm_max_substeps_refuses_excess() {
        let mut lbm = LbmSpatialAudio::new(LbmConfig {
            max_substeps: 2,
            ..LbmConfig::default()
        });
        let k = MortonKey::encode(5, 5, 5).unwrap();
        lbm.inject_source(k, Complex::new(0.1, 0.0)).unwrap();
        // First 2 substeps OK ; 3rd refused.
        lbm.substep().unwrap();
        lbm.substep().unwrap();
        let r = lbm.substep();
        assert!(r.is_err());
    }

    #[test]
    fn lbm_begin_frame_resets_counter() {
        let mut lbm = LbmSpatialAudio::default();
        let k = MortonKey::encode(5, 5, 5).unwrap();
        lbm.inject_source(k, Complex::new(0.1, 0.0)).unwrap();
        lbm.substep().unwrap();
        assert_eq!(lbm.substeps_this_frame(), 1);
        lbm.begin_frame();
        assert_eq!(lbm.substeps_this_frame(), 0);
    }

    #[test]
    fn lbm_step_for_dt_returns_substep_count() {
        let mut lbm = LbmSpatialAudio::default();
        let k = MortonKey::encode(5, 5, 5).unwrap();
        lbm.inject_source(k, Complex::new(0.05, 0.0)).unwrap();
        let dt = 1.0 / 60.0; // ~16.6 ms
        let n = lbm.step_for_dt(dt).unwrap();
        // Δt_sub ≈ 7.3e-4 ; ceil(16.6ms / 7.3e-4) ≈ 23 ; clamped to 16.
        assert!(n >= 1 && n <= 16);
    }

    #[test]
    fn lbm_mark_boundary_records_wall() {
        let mut lbm = LbmSpatialAudio::default();
        let k = MortonKey::encode(2, 2, 2).unwrap();
        lbm.mark_boundary(k, WallClass::Rigid);
        assert_eq!(lbm.boundary_count(), 1);
    }

    #[test]
    fn lbm_rigid_boundary_kills_amplitude() {
        let mut lbm = LbmSpatialAudio::default();
        let src = MortonKey::encode(5, 5, 5).unwrap();
        let bdy = MortonKey::encode(6, 5, 5).unwrap();
        lbm.inject_source(src, Complex::new(1.0, 0.0)).unwrap();
        lbm.mark_boundary(bdy, WallClass::Rigid);
        lbm.substep().unwrap();
        // The rigid boundary cell should have amplitude zero after BC.
        assert!(lbm.current().at(bdy).norm() < 1e-6);
    }

    #[test]
    fn lbm_soft_boundary_attenuates() {
        let mut lbm = LbmSpatialAudio::default();
        let src = MortonKey::encode(5, 5, 5).unwrap();
        let bdy = MortonKey::encode(6, 5, 5).unwrap();
        lbm.inject_source(src, Complex::new(1.0, 0.0)).unwrap();
        lbm.mark_boundary(bdy, WallClass::Soft);
        lbm.substep().unwrap();
        let amp = lbm.current().at(bdy);
        // Soft wall halves the amplitude — should be non-zero but
        // smaller than the rigid case (which was zero).
        assert!(amp.norm() > 0.0);
        assert!(amp.norm() < 0.5);
    }

    #[test]
    fn lbm_impedance_boundary_partial_reflection() {
        let mut lbm = LbmSpatialAudio::default();
        let src = MortonKey::encode(5, 5, 5).unwrap();
        let bdy = MortonKey::encode(6, 5, 5).unwrap();
        lbm.inject_source(src, Complex::new(1.0, 0.0)).unwrap();
        lbm.mark_boundary(bdy, WallClass::Impedance);
        lbm.substep().unwrap();
        let amp = lbm.current().at(bdy);
        // Impedance wall reflects a fraction.
        assert!(amp.norm() > 0.0);
    }

    #[test]
    fn lbm_mark_vocal_tract_boundary_records_cells() {
        let mut lbm = LbmSpatialAudio::default();
        let sdf = VocalTractSdf::human_default();
        lbm.mark_vocal_tract_boundary(&sdf, (5, 5, 5));
        assert!(lbm.boundary_count() > 0);
    }

    #[test]
    fn lbm_substep_is_deterministic() {
        let mut a = LbmSpatialAudio::default();
        let mut b = LbmSpatialAudio::default();
        let k = MortonKey::encode(5, 5, 5).unwrap();
        a.inject_source(k, Complex::new(0.5, 0.3)).unwrap();
        b.inject_source(k, Complex::new(0.5, 0.3)).unwrap();
        a.substep().unwrap();
        b.substep().unwrap();
        // Compare a few cells.
        let neighbor = MortonKey::encode(6, 5, 5).unwrap();
        let amp_a = a.current().at(neighbor);
        let amp_b = b.current().at(neighbor);
        assert_eq!(amp_a.re.to_bits(), amp_b.re.to_bits());
        assert_eq!(amp_a.im.to_bits(), amp_b.im.to_bits());
    }

    #[test]
    fn lbm_propagation_speed_one_cell_per_substep() {
        // Place a source ; after one substep neighbors should be lit ;
        // after two, neighbors-of-neighbors should be lit but the
        // original neighbor still carries some amplitude.
        let mut lbm = LbmSpatialAudio::default();
        let center = MortonKey::encode(10, 10, 10).unwrap();
        lbm.inject_source(center, Complex::new(1.0, 0.0)).unwrap();
        lbm.substep().unwrap();
        let neighbor1 = MortonKey::encode(11, 10, 10).unwrap();
        assert!(lbm.current().at(neighbor1).norm() > 0.0);
        lbm.substep().unwrap();
        let neighbor2 = MortonKey::encode(12, 10, 10).unwrap();
        assert!(lbm.current().at(neighbor2).norm() > 0.0);
    }

    #[test]
    fn lbm_zero_dt_returns_error() {
        let mut lbm = LbmSpatialAudio::new(LbmConfig {
            voxel_size: 0.0,
            ..LbmConfig::default()
        });
        let k = MortonKey::encode(0, 0, 0).unwrap();
        lbm.inject_source(k, Complex::new(0.1, 0.0)).unwrap();
        let r = lbm.step_for_dt(0.0);
        assert!(r.is_err());
    }

    #[test]
    fn lbm_silent_field_substep_is_noop() {
        let mut lbm = LbmSpatialAudio::default();
        // No source injected ; substep should still succeed.
        lbm.substep().unwrap();
        assert!(!lbm.has_active_cells());
    }
}
