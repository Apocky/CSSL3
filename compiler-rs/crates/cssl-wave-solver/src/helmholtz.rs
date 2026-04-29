//! § Steady-state complex-Helmholtz residual + Jacobi-style iterator.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS (Wave-Unity §II.2)
//!   At steady-state the wave field obeys :
//!
//!     `∇²ψ + k²(x) ψ = source(x)`
//!
//!   with the boundary conditions described in [`crate::bc`]. The
//!   complex wavenumber is `k(x) = ω / c(x) + i α(x)` where `α` is the
//!   absorption coefficient.
//!
//!   At Stage-0 we expose two utilities :
//!     1. [`helmholtz_residual`] — compute the L² residual of the
//!        Helmholtz equation given a candidate `ψ` field. Used by
//!        Phase-6 entropy-book to detect numerical drift.
//!     2. [`helmholtz_steady_iterate`] — one Jacobi-style relaxation
//!        substep toward the steady-state. Used by the standing-wave
//!        + sound-caustic test scenes ; not part of the per-frame
//!        omega_step pipeline (the LBM/IMEX kernels are the canonical
//!        transient solvers).
//!
//! § RESIDUAL DEFINITION
//!   `R(x) = ∇²ψ(x) + k²(x) ψ(x) - source(x)` per cell.
//!   The L² norm is `√( Σ |R(x)|² · Δx³ )`.
//!
//! § JACOBI ITERATOR
//!   `ψ_new(x) = (1 - ω) ψ(x) + ω · ψ_jacobi(x)` where
//!   `ψ_jacobi(x) = (mean(ψ_neighbours) - source(x) / 6) / (1 - k²(x) Δx² / 6)`
//!   is the Gauss-Jacobi point-update for the discrete Helmholtz
//!   equation. `ω ∈ (0, 1]` is the relaxation parameter.

use crate::complex::C32;
use crate::psi_field::WaveField;

#[cfg(test)]
use crate::band::Band;

use cssl_substrate_omega_field::MortonKey;

/// § Compute the L² residual of `∇²ψ + k²ψ - source` over the active
///   region of `band` in `field`. Used by Phase-6 entropy-book.
///
///   The Laplacian uses a 7-point central-difference stencil
///   (face-cardinals only). `dx_m` is the cell size in metres ;
///   `k` is the complex wavenumber `(ω/c, α)` packed as a [`C32`].
///   `source` is a closure returning the source at each cell.
#[must_use]
pub fn helmholtz_residual<const C: usize>(
    field: &WaveField<C>,
    band_idx: usize,
    dx_m: f32,
    k_complex: C32,
    source: impl Fn(MortonKey) -> C32,
) -> f32 {
    if band_idx >= field.band_count() {
        return 0.0;
    }
    let inv_dx2 = 1.0 / (dx_m * dx_m);
    let mut sum_sq = 0.0_f32;
    let cells: Vec<(MortonKey, C32)> = field.cells_in_band(band_idx).collect();
    let k_sq = k_complex * k_complex;
    for (k, psi_here) in &cells {
        // Compute Laplacian using face-cardinal neighbours.
        let (x, y, z) = k.decode();
        let mut lap = C32::ZERO;
        let mut count = 0_i32;
        for (dx, dy, dz) in [
            (1i64, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            let nx = x as i64 + dx;
            let ny = y as i64 + dy;
            let nz = z as i64 + dz;
            if nx < 0 || ny < 0 || nz < 0 {
                continue;
            }
            let nk = match MortonKey::encode(nx as u64, ny as u64, nz as u64) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let psi_n = field.at(band_idx, nk);
            lap += psi_n - *psi_here;
            count += 1;
        }
        if count > 0 {
            lap = lap.scale(inv_dx2);
        }
        let res = lap + (*psi_here * k_sq) - source(*k);
        sum_sq += res.norm_sqr();
    }
    sum_sq.sqrt()
}

/// § One Jacobi-style relaxation substep toward steady-state Helmholtz.
///
/// `omega_relax ∈ (0, 1]` is the under-relaxation factor. Returns the
/// number of cells touched.
pub fn helmholtz_steady_iterate<const C: usize>(
    prev: &WaveField<C>,
    next: &mut WaveField<C>,
    band_idx: usize,
    dx_m: f32,
    k_complex: C32,
    omega_relax: f32,
    source: impl Fn(MortonKey) -> C32,
) -> usize {
    if band_idx >= prev.band_count() {
        return 0;
    }
    let cells: Vec<(MortonKey, C32)> = prev.cells_in_band(band_idx).collect();
    let touched = cells.len();
    let k_sq = k_complex * k_complex;
    for (k, psi_here) in &cells {
        let (x, y, z) = k.decode();
        let mut sum = C32::ZERO;
        let mut count = 0_i32;
        for (dx, dy, dz) in [
            (1i64, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            let nx = x as i64 + dx;
            let ny = y as i64 + dy;
            let nz = z as i64 + dz;
            if nx < 0 || ny < 0 || nz < 0 {
                continue;
            }
            let nk = match MortonKey::encode(nx as u64, ny as u64, nz as u64) {
                Ok(v) => v,
                Err(_) => continue,
            };
            sum += prev.at(band_idx, nk);
            count += 1;
        }
        let n = count as f32;
        if n < 1e-9 {
            next.set(band_idx, *k, *psi_here);
            continue;
        }
        let mean_n = sum.scale(1.0 / n);
        // ψ_jacobi = (mean - dx² · source) / (1 - k² · dx² / n).
        let denom = C32::ONE - (k_sq.scale(dx_m * dx_m / n));
        let dn = denom.norm_sqr();
        if dn < 1e-12 {
            next.set(band_idx, *k, *psi_here);
            continue;
        }
        let psi_jacobi = (mean_n - source(*k).scale(dx_m * dx_m / n)) / denom;
        // ψ_new = (1 - ω) ψ + ω ψ_jacobi.
        let psi_new = psi_here.scale(1.0 - omega_relax) + psi_jacobi.scale(omega_relax);
        if psi_new.is_finite() {
            next.set(band_idx, *k, psi_new);
        } else {
            next.set(band_idx, *k, *psi_here);
        }
    }
    touched
}

/// § Identify steady-state standing-wave modes : cells where the
///   amplitude is bounded above the threshold AND the time-derivative
///   approximation `(ψ_t - ψ_{t-Δt}) / Δt` is below a tolerance.
///   Used by the standing-wave detection test (V.3).
#[must_use]
pub fn detect_standing_wave_cells<const C: usize>(
    field_t: &WaveField<C>,
    field_tm1: &WaveField<C>,
    band_idx: usize,
    amplitude_threshold: f32,
    derivative_threshold: f32,
) -> Vec<MortonKey> {
    let mut found = Vec::new();
    if band_idx >= field_t.band_count() {
        return found;
    }
    for (k, v) in field_t.cells_in_band(band_idx) {
        if v.norm() > amplitude_threshold {
            let prev = field_tm1.at(band_idx, k);
            let dphi = (v - prev).norm();
            if dphi < derivative_threshold {
                found.push(k);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(x: u64, y: u64, z: u64) -> MortonKey {
        MortonKey::encode(x, y, z).unwrap()
    }

    #[test]
    fn residual_zero_for_zero_field() {
        let field = WaveField::<5>::with_default_bands();
        let r = helmholtz_residual(&field, 0, 0.5, C32::new(1.0, 0.0), |_| C32::ZERO);
        assert!(r.abs() < 1e-9);
    }

    #[test]
    fn residual_non_zero_when_source_unbalanced() {
        let mut field = WaveField::<5>::with_default_bands();
        field.set_band(Band::AudioSubKHz, key(5, 5, 5), C32::new(1.0, 0.0));
        // Source = 0 ; the bare amplitude with no neighbours should
        // produce a non-zero residual.
        let r = helmholtz_residual(&field, 0, 0.5, C32::new(1.0, 0.0), |_| C32::ZERO);
        assert!(r > 0.0);
    }

    #[test]
    fn jacobi_iterate_runs_without_panic() {
        // Setting a cell to ZERO removes it from the WaveField — the
        // canonical zero-cull pattern. We verify only the non-zero
        // count survives.
        let mut prev = WaveField::<5>::with_default_bands();
        prev.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(1.0, 0.0));
        prev.set_band(Band::AudioSubKHz, key(1, 0, 0), C32::new(0.5, 0.0));
        prev.set_band(Band::AudioSubKHz, key(2, 0, 0), C32::new(1.0, 0.0));
        let mut next = WaveField::<5>::with_default_bands();
        let n = helmholtz_steady_iterate(&prev, &mut next, 0, 0.5, C32::new(0.1, 0.0), 0.5, |_| {
            C32::ZERO
        });
        assert_eq!(n, 3);
    }

    #[test]
    fn jacobi_omega_zero_is_identity() {
        let mut prev = WaveField::<5>::with_default_bands();
        prev.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(2.0, 0.5));
        let mut next = WaveField::<5>::with_default_bands();
        helmholtz_steady_iterate(&prev, &mut next, 0, 0.5, C32::new(0.1, 0.0), 0.0, |_| {
            C32::ZERO
        });
        let v = next.at_band(Band::AudioSubKHz, key(0, 0, 0));
        // omega = 0 ⇒ no change.
        assert!((v.re - 2.0).abs() < 1e-6);
        assert!((v.im - 0.5).abs() < 1e-6);
    }

    #[test]
    fn detect_standing_wave_finds_high_amplitude_zero_drift() {
        let mut t = WaveField::<5>::with_default_bands();
        let mut tm1 = WaveField::<5>::with_default_bands();
        // High-amplitude cell, no drift.
        t.set_band(Band::AudioSubKHz, key(5, 5, 5), C32::new(2.0, 0.0));
        tm1.set_band(Band::AudioSubKHz, key(5, 5, 5), C32::new(2.0, 0.0));
        // High-amplitude cell with drift.
        t.set_band(Band::AudioSubKHz, key(6, 6, 6), C32::new(2.0, 0.0));
        tm1.set_band(Band::AudioSubKHz, key(6, 6, 6), C32::new(0.0, 0.0));
        let found = detect_standing_wave_cells(&t, &tm1, 0, 1.5, 0.1);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], key(5, 5, 5));
    }

    #[test]
    fn detect_standing_wave_skips_low_amplitude() {
        let mut t = WaveField::<5>::with_default_bands();
        let mut tm1 = WaveField::<5>::with_default_bands();
        t.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(0.001, 0.0));
        tm1.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(0.001, 0.0));
        let found = detect_standing_wave_cells(&t, &tm1, 0, 0.1, 0.1);
        assert!(found.is_empty());
    }

    #[test]
    fn residual_oob_band_returns_zero() {
        let field = WaveField::<5>::with_default_bands();
        let r = helmholtz_residual(&field, 99, 0.5, C32::new(1.0, 0.0), |_| C32::ZERO);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn jacobi_oob_band_returns_zero_touched() {
        let prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let n =
            helmholtz_steady_iterate(&prev, &mut next, 99, 0.5, C32::new(1.0, 0.0), 0.5, |_| {
                C32::ZERO
            });
        assert_eq!(n, 0);
    }
}
