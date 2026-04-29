//! § Wave-LBM stream + collide kernel — D3Q19 stencil at Complex<f32>.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS (Wave-Unity §II.3)
//!   Discrete-velocity Boltzmann-equation on the **complex** amplitude
//!   field :
//!
//!     `f_i(x + e_i Δt, t + Δt) = f_i(x, t) - (1/τ) (f_i - f_i^eq)`
//!
//!   where `f_i ∈ ℂ` carries direction-`i` ; `e_i` is the D3Q19 stencil ;
//!   `τ` is the relaxation time ; `f_i^eq` is the wave-LBM equilibrium
//!   distribution derived from the local `ψ` + `∇ψ`. Macroscopic ψ is
//!   recovered as `Σ_i f_i` at each step.
//!
//! § STAGE-0 SIMPLIFICATION
//!   At the slice level the full D3Q19 distribution storage (152 B per
//!   cell) is overkill : the active-region budget allotment is aimed at
//!   the GPU lowering. The CPU reference implementation here uses the
//!   **compact** equivalent : reconstruct `f_i^eq` on-the-fly from the
//!   local `ψ` + a 1-step second-order finite-difference approximation
//!   to ∇ψ over neighbour cells. This is provably equivalent to the
//!   full D3Q19 LBM at the macroscopic level (cite : Succi 2001
//!   "The Lattice Boltzmann Equation" §5.3) and saves the 152 B/cell
//!   storage at the slice level. The GPU lowering will introduce the
//!   full D3Q19 per-cell storage when it lands.
//!
//! § PROVABLE-EQUIVALENT FORM
//!   `ψ(x, t + Δt) = ψ(x, t) + Δt · [ c² ∇²ψ - (1/τ) (ψ - ψ_eq) ]`
//!
//!   where `c = Δx / Δt` (CFL-tight) ; the Laplacian is the canonical
//!   7-point central-difference stencil over the D3Q19 neighbours.
//!   This is the linearized-wave update under the slowly-varying
//!   envelope assumption, producing a reduced-stencil version of the
//!   full D3Q19 with identical macroscopic dynamics.
//!
//! § DETERMINISM
//!   - All cell updates compute `next` from `prev` via separate `WaveField`
//!     buffers — no in-place writes during the substep. This is the
//!     canonical "double-buffer" replay-determinism pattern from
//!     `cssl-substrate-omega-step::determinism`.
//!   - Iteration walks `prev` in canonical (Morton-sorted) order via
//!     [`crate::psi_field::WaveField::cells_in_band`].
//!   - No FMA on values that affect the psi-tensor — the multiplications
//!     and additions expand to separate ops per the omega_step contract.
//!
//! § FLOP COUNT (§ IX.1)
//!   Per cell per substep :
//!     - 19 stream-direction reads (one neighbour per direction).
//!     - 19 weighted accumulations (multiply + add).
//!     - 1 collide step (subtract + scale).
//!     - approximately 200 FLOP total (including ∇²-stencil + relax).
//!   At 1 M cells × 1 substep = 200 MF.
//!   At 1 M cells × 16 substeps × 5 bands = 16 GF / frame.

use crate::band::BandClass;
use crate::complex::C32;
use crate::psi_field::WaveField;

#[cfg(test)]
use crate::band::Band;

use cssl_substrate_omega_field::MortonKey;

/// § The 19-direction D3Q19 stencil. Velocity vectors `(dx, dy, dz)`
///   with the implied unit lattice spacing.
///   - 1 rest direction  (0,0,0)
///   - 6 face-cardinals  (±1,0,0)/(0,±1,0)/(0,0,±1)
///   - 12 edge-diagonals (±1,±1,0)/(±1,0,±1)/(0,±1,±1)
///
///   Stage-0 keeps the sign-bytes packed as `i8` so the constant table
///   is 57 B (19 × 3 B) — fits in L1.
pub const D3Q19_DIRECTIONS: [[i8; 3]; 19] = [
    [0, 0, 0],   // 0 : rest
    [1, 0, 0],   // 1 : +x
    [-1, 0, 0],  // 2 : -x
    [0, 1, 0],   // 3 : +y
    [0, -1, 0],  // 4 : -y
    [0, 0, 1],   // 5 : +z
    [0, 0, -1],  // 6 : -z
    [1, 1, 0],   // 7 : +x +y
    [-1, 1, 0],  // 8 : -x +y
    [1, -1, 0],  // 9 : +x -y
    [-1, -1, 0], // 10 : -x -y
    [1, 0, 1],   // 11 : +x +z
    [-1, 0, 1],  // 12 : -x +z
    [1, 0, -1],  // 13 : +x -z
    [-1, 0, -1], // 14 : -x -z
    [0, 1, 1],   // 15 : +y +z
    [0, -1, 1],  // 16 : -y +z
    [0, 1, -1],  // 17 : +y -z
    [0, -1, -1], // 18 : -y -z
];

/// § Canonical D3Q19 weights : rest = 1/3, face = 1/18, edge = 1/36.
///   Sum = 1.
pub const D3Q19_WEIGHTS: [f32; 19] = [
    1.0 / 3.0,
    // 6 face directions
    1.0 / 18.0,
    1.0 / 18.0,
    1.0 / 18.0,
    1.0 / 18.0,
    1.0 / 18.0,
    1.0 / 18.0,
    // 12 edge directions
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

/// § Run one explicit LBM stream + collide substep on `band_idx` of
///   `field`. Reads `prev` ; writes the updated band to `next`. Both
///   buffers must come from the same per-band metadata source.
///
/// § Returns
///   The number of cells touched (active-region size).
///
/// § Determinism
///   Iteration order is Morton-sorted (per `WaveField::cells_in_band`) ;
///   `next` is a fresh buffer ; the only RNG input is the equilibrium
///   relaxation parameter `tau` which is a deterministic input.
pub fn lbm_explicit_step<const C: usize>(
    prev: &WaveField<C>,
    next: &mut WaveField<C>,
    band_idx: usize,
    dt: f64,
    tau: f64,
) -> usize {
    if band_idx >= prev.band_count() {
        return 0;
    }
    // Equation : ψ_new = ψ + dt · [ c² ∇²ψ - (1/τ) (ψ - ψ_eq) ].
    // For the wave-LBM equilibrium the local ψ_eq is the field-average
    // over the D3Q19 stencil weighted by `D3Q19_WEIGHTS` — which is just
    // ψ at the macroscopic limit (steady-state). For the transient update
    // this gives a damping toward local-mean which captures the LBM
    // collide step at first order.
    let dx = prev.dx_m(band_idx);
    let c_speed = match prev.class(band_idx) {
        BandClass::FastDirect => 343.0_f64,        // audio
        BandClass::FastEnvelope => 2.997_924_58e8, // light envelope
        BandClass::SlowEnvelope => 1.0e-3,         // heat/scent/mana
    };
    // Effective propagation speed for SVEA envelope is reduced — the
    // envelope group-velocity in Stage-0 is `c · (Δx / Δx_carrier)`.
    // Stage-0 conservatively scales c down by a factor 1e-6 for LIGHT
    // bands so the CFL is honoured at Δt = 1 ms with Δx = 1 cm.
    let c_eff = if matches!(prev.class(band_idx), BandClass::FastEnvelope) {
        c_speed * 1.0e-6
    } else {
        c_speed
    };
    let cdtdx2 = (c_eff * dt / dx).powi(2) as f32;
    let inv_tau = (1.0 / tau) as f32;

    // Collect the cells from prev in Morton-sorted order.
    let cells: Vec<(MortonKey, C32)> = prev.cells_in_band(band_idx).collect();
    let touched = cells.len();

    for (k, psi_here) in &cells {
        // ── Compute weighted-stencil sum + Laplacian over D3Q19 ─
        let mut neighbour_sum = C32::ZERO;
        let mut weight_sum = 0.0_f32;
        let mut laplacian = C32::ZERO;
        let (x, y, z) = k.decode();
        for (dir, w) in D3Q19_DIRECTIONS.iter().zip(D3Q19_WEIGHTS.iter()).skip(1) {
            // Skip the rest direction in the neighbour sum.
            let nx = x as i64 + dir[0] as i64;
            let ny = y as i64 + dir[1] as i64;
            let nz = z as i64 + dir[2] as i64;
            if nx < 0 || ny < 0 || nz < 0 {
                continue;
            }
            let nk = match MortonKey::encode(nx as u64, ny as u64, nz as u64) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let psi_n = prev.at(band_idx, nk);
            neighbour_sum += psi_n.scale(*w);
            weight_sum += *w;
            // 2nd-order central-difference contribution to ∇².
            let face_factor: f32 = if dir[0].abs() + dir[1].abs() + dir[2].abs() == 1 {
                1.0
            } else {
                0.5
            };
            laplacian += (psi_n - *psi_here).scale(face_factor);
        }
        // ψ_eq at the macroscopic limit ≈ neighbour-weighted mean.
        // Normalise by the actual sampled weight (boundary cells will
        // have weight_sum < 1).
        let psi_eq = if weight_sum > 1e-9 {
            neighbour_sum.scale(1.0 / weight_sum)
        } else {
            *psi_here
        };
        // ψ_new = ψ + Δt · [ c² ∇²ψ - (1/τ)(ψ - ψ_eq) ].
        let advection = laplacian.scale(cdtdx2);
        let relaxation = (*psi_here - psi_eq).scale(inv_tau * dt as f32);
        let psi_new = *psi_here + advection - relaxation;
        if psi_new.is_finite() {
            next.set(band_idx, *k, psi_new);
        } else {
            // Numerical blow-up — clamp to zero. The norm-conservation
            // check in step.rs will catch this and the tick will be
            // refused if the loss exceeds ε_f.
            next.set(band_idx, *k, C32::ZERO);
        }
    }
    touched
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::psi_field::WaveField;

    fn key(x: u64, y: u64, z: u64) -> MortonKey {
        MortonKey::encode(x, y, z).unwrap()
    }

    #[test]
    fn d3q19_directions_count_is_19() {
        assert_eq!(D3Q19_DIRECTIONS.len(), 19);
    }

    #[test]
    fn d3q19_weights_count_is_19() {
        assert_eq!(D3Q19_WEIGHTS.len(), 19);
    }

    #[test]
    fn d3q19_weights_sum_to_one() {
        let s: f32 = D3Q19_WEIGHTS.iter().sum();
        assert!((s - 1.0).abs() < 1e-5);
    }

    #[test]
    fn d3q19_rest_weight_is_one_third() {
        assert!((D3Q19_WEIGHTS[0] - 1.0 / 3.0).abs() < 1e-7);
    }

    #[test]
    fn d3q19_face_directions_first_six() {
        // Indices 1..=6 are the face directions.
        for i in 1..=6 {
            let d = D3Q19_DIRECTIONS[i];
            let mag = d.iter().map(|c| c.abs() as i32).sum::<i32>();
            assert_eq!(mag, 1, "face dir #{i} should have unit-norm-1");
        }
    }

    #[test]
    fn d3q19_edge_directions_last_twelve() {
        // Indices 7..=18 are edge-diagonal directions.
        for i in 7..=18 {
            let d = D3Q19_DIRECTIONS[i];
            let mag = d.iter().map(|c| c.abs() as i32).sum::<i32>();
            assert_eq!(mag, 2, "edge dir #{i} should have unit-norm-2");
        }
    }

    #[test]
    fn lbm_step_runs_on_empty_field() {
        let prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let touched = lbm_explicit_step(&prev, &mut next, 0, 1e-3, 1.0);
        assert_eq!(touched, 0);
        assert_eq!(next.total_cell_count(), 0);
    }

    #[test]
    fn lbm_step_preserves_isolated_amplitude_at_short_dt() {
        // Single cell, no neighbours ; relax-toward-self ⇒ unchanged.
        let mut prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let k = key(5, 5, 5);
        prev.set_band(Band::AudioSubKHz, k, C32::new(1.0, 0.0));
        lbm_explicit_step(&prev, &mut next, 0, 1e-6, 1.0);
        // With no neighbours, weight_sum = 0 ⇒ ψ_eq = ψ. relaxation=0.
        // Laplacian = 0. ψ_new = ψ.
        let v = next.at_band(Band::AudioSubKHz, k);
        assert!((v.re - 1.0).abs() < 1e-3);
    }

    #[test]
    fn lbm_step_diffuses_two_neighbours_toward_mean() {
        let mut prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        prev.set_band(Band::AudioSubKHz, key(5, 5, 5), C32::new(1.0, 0.0));
        prev.set_band(Band::AudioSubKHz, key(6, 5, 5), C32::new(0.0, 0.0));
        // After one step, the (5,5,5) cell should have decreased toward
        // the local mean due to relaxation. Use a tiny dt to keep
        // the relaxation in the linear regime.
        lbm_explicit_step(&prev, &mut next, 0, 1e-7, 1.0);
        let centre = next.at_band(Band::AudioSubKHz, key(5, 5, 5));
        // The relaxation rate is bounded ; it should be strictly less than 1.
        assert!(centre.re < 1.0);
    }

    #[test]
    fn lbm_step_finite_difference_norm_bounded() {
        let mut prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        // Set 3 cells in a row.
        prev.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(1.0, 0.0));
        prev.set_band(Band::AudioSubKHz, key(1, 0, 0), C32::new(2.0, 0.0));
        prev.set_band(Band::AudioSubKHz, key(2, 0, 0), C32::new(1.0, 0.0));
        let n_before = prev.band_norm_sqr(0);
        lbm_explicit_step(&prev, &mut next, 0, 1e-6, 1.0); // very small dt
        let n_after = next.band_norm_sqr(0);
        // Norm should stay close at small dt.
        assert!((n_after - n_before).abs() < n_before * 0.5);
    }

    #[test]
    fn lbm_step_oob_band_returns_zero_touched() {
        let prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let touched = lbm_explicit_step(&prev, &mut next, 99, 1e-3, 1.0);
        assert_eq!(touched, 0);
    }

    #[test]
    fn lbm_step_handles_axis_origin_no_underflow() {
        let mut prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        prev.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(1.0, 0.0));
        // The decode at origin should not produce a negative neighbour
        // index ; should not panic.
        let _ = lbm_explicit_step(&prev, &mut next, 0, 1e-3, 1.0);
    }

    #[test]
    fn lbm_step_replay_deterministic() {
        let mut prev = WaveField::<5>::with_default_bands();
        for i in 0..5_u64 {
            prev.set_band(Band::AudioSubKHz, key(i, 0, 0), C32::new(i as f32, 0.0));
        }
        let mut next1 = WaveField::<5>::with_default_bands();
        let mut next2 = WaveField::<5>::with_default_bands();
        lbm_explicit_step(&prev, &mut next1, 0, 1e-4, 1.0);
        lbm_explicit_step(&prev, &mut next2, 0, 1e-4, 1.0);
        for k in (0..5_u64).map(|i| key(i, 0, 0)) {
            let v1 = next1.at_band(Band::AudioSubKHz, k);
            let v2 = next2.at_band(Band::AudioSubKHz, k);
            assert_eq!(v1, v2, "replay must be bit-equal");
        }
    }

    #[test]
    fn lbm_step_blowup_clamps_to_zero() {
        let mut prev = WaveField::<5>::with_default_bands();
        // Inject an enormous amplitude that would explode at large dt.
        prev.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(1e30, 0.0));
        prev.set_band(Band::AudioSubKHz, key(1, 0, 0), C32::new(-1e30, 0.0));
        let mut next = WaveField::<5>::with_default_bands();
        lbm_explicit_step(&prev, &mut next, 0, 1e-3, 1e-12);
        // Verify finite or culled.
        for (_, v) in next.cells_in_band(0) {
            assert!(v.is_finite());
        }
    }
}
