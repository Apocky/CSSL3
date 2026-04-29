//! Register-packed Jet for GPU evaluation.
//!
//! § SPEC : `specs/17_JETS.csl § GPU JET (Arc A770)` :
//!   `Jet<f32, N> fits in register-file for small N (N ≤ 4 typical)`
//!   `larger N : shared-memory or SSBO-spill`.
//!
//! § DESIGN
//!   `Jet<T, N>` for `N ≤ 4` packs cleanly into 4-component vector
//!   registers (vec4 / `vector<float, 4>` / `simd_float4`). For `N > 4` the
//!   tape spills to either shared-memory or SSBO. This type is the
//!   register-packed view ; the spill/reload helpers translate to/from a
//!   slice in the chosen storage-mode.
//!
//! § INTEGRATION
//!   The GPU evaluation pipeline takes the Jet from the calling MIR-fn,
//!   converts it to a `GpuJet`, threads it through the recorded ops on the
//!   tape, and reads out the gradient at the reverse-pass exit. The
//!   register-packed form means each per-thread Jet is a *single value-type
//!   slot* in SPIR-V, not a `OpVariable` array.
//!
//! § FALLBACK
//!   If the kernel's storage budget exceeds the register-file (per the
//!   walker's density estimate in `storage::OperationDensity`), the Jet is
//!   spilled to shared-memory at the workgroup boundary. Because the on-tape
//!   format is invariant across modes, this is purely a SPIR-V emission
//!   detail — the algebra in this module operates on the packed form
//!   regardless.

use crate::Jet;
use crate::JetField;

/// Inline factorial helper (mirrors the private factorial in `crate::jet`).
#[inline]
fn factorial_f64(n: usize) -> f64 {
    let mut acc = 1.0_f64;
    for k in 2..=n {
        acc *= k as f64;
    }
    acc
}

/// Maximum N for which `Jet<T, N>` fits entirely in a 4-wide register.
pub const GPU_JET_REGISTER_LIMIT: usize = 4;

/// Errors the GPU-jet API can surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuJetError {
    /// Caller attempted to construct a register-packed Jet beyond the limit.
    OrderExceedsRegisterLimit { requested: usize, limit: usize },
    /// Caller attempted to load from a shared-memory slice that's too short.
    SharedSliceTooShort { needed: usize, actual: usize },
}

impl core::fmt::Display for GpuJetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OrderExceedsRegisterLimit { requested, limit } => write!(
                f,
                "Jet of storage-size {requested} exceeds GPU register-pack limit {limit}"
            ),
            Self::SharedSliceTooShort { needed, actual } => write!(
                f,
                "shared-memory slice of len {actual} too short ; needed {needed}"
            ),
        }
    }
}

impl std::error::Error for GpuJetError {}

/// Register-packed Jet for the GPU forward-pass.
///
/// The `Jet<T, N>` underneath is reused verbatim — the GPU-jet wrapper just
/// gates against the register-pack limit + offers spill / reload helpers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuJet<T: JetField, const N: usize> {
    inner: Jet<T, N>,
}

impl<T: JetField, const N: usize> GpuJet<T, N> {
    /// Construct from a CPU-side `Jet<T, N>`. Errors if `N > 4`.
    pub fn pack(j: Jet<T, N>) -> Result<Self, GpuJetError> {
        if N > GPU_JET_REGISTER_LIMIT {
            return Err(GpuJetError::OrderExceedsRegisterLimit {
                requested: N,
                limit: GPU_JET_REGISTER_LIMIT,
            });
        }
        Ok(Self { inner: j })
    }

    /// Bypass the register-pack check (used internally by spill/reload).
    #[must_use]
    pub const fn pack_unchecked(j: Jet<T, N>) -> Self {
        Self { inner: j }
    }

    /// Underlying CPU Jet view.
    #[must_use]
    pub const fn inner(&self) -> &Jet<T, N> {
        &self.inner
    }

    /// Mutable access (used by tape recorders).
    pub fn inner_mut(&mut self) -> &mut Jet<T, N> {
        &mut self.inner
    }

    /// Primal extraction.
    #[must_use]
    pub fn primal(&self) -> T {
        self.inner.primal()
    }

    /// k-th derivative extraction.
    #[must_use]
    pub fn nth_deriv(&self, k: usize) -> T {
        self.inner.nth_deriv(k)
    }

    /// Spill to a shared-memory slice. Each Jet element occupies one slot ;
    /// caller pre-sizes `dest` to at least `N` elements.
    pub fn spill_to_shared(&self, dest: &mut [T]) -> Result<(), GpuJetError> {
        if dest.len() < N {
            return Err(GpuJetError::SharedSliceTooShort {
                needed: N,
                actual: dest.len(),
            });
        }
        for (k, slot) in dest.iter_mut().enumerate().take(N) {
            *slot = self.inner.nth_deriv(k);
        }
        Ok(())
    }

    /// Reload from a shared-memory slice ; mirror of [`Self::spill_to_shared`].
    ///
    /// The slice is expected to hold the *raw nth-derivative values* (output
    /// of [`Self::spill_to_shared`]) ; each `slice[k]` holds `f^(k)(x)`. The
    /// internal Jet stores `terms[k] = f^(k)(x) / k!`, so the reload divides
    /// by `k!` to round-trip cleanly with [`Self::spill_to_shared`].
    pub fn reload_from_shared(slice: &[T]) -> Result<Self, GpuJetError> {
        if slice.len() < N {
            return Err(GpuJetError::SharedSliceTooShort {
                needed: N,
                actual: slice.len(),
            });
        }
        let mut terms = [T::zero(); N];
        for k in 0..N {
            let kf = factorial_f64(k);
            terms[k] = slice[k].scale_f64(1.0 / kf);
        }
        let inner = Jet::new(terms);
        Ok(Self { inner })
    }

    /// True iff this Jet fits in a 4-wide vector register.
    #[must_use]
    pub const fn fits_in_register(&self) -> bool {
        N <= GPU_JET_REGISTER_LIMIT
    }
}

impl<T: JetField, const N: usize> From<Jet<T, N>> for GpuJet<T, N> {
    /// Direct-conversion ; bypasses the register-pack check via
    /// `pack_unchecked`. Use [`Self::pack`] for a checked construction.
    fn from(value: Jet<T, N>) -> Self {
        Self::pack_unchecked(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_succeeds_for_jet2() {
        let j: Jet<f32, 2> = Jet::lift(3.0);
        assert!(GpuJet::pack(j).is_ok());
    }

    #[test]
    fn pack_succeeds_for_jet4() {
        let j: Jet<f32, 4> = Jet::lift(3.0);
        assert!(GpuJet::pack(j).is_ok());
    }

    #[test]
    fn pack_fails_for_jet5() {
        let j: Jet<f32, 5> = Jet::lift(3.0);
        let err = GpuJet::pack(j).unwrap_err();
        match err {
            GpuJetError::OrderExceedsRegisterLimit { requested, limit } => {
                assert_eq!(requested, 5);
                assert_eq!(limit, 4);
            }
            other => panic!("wrong error : {other:?}"),
        }
    }

    #[test]
    fn primal_round_trip() {
        let j: Jet<f32, 2> = Jet::promote(2.5);
        let g = GpuJet::pack(j).unwrap();
        assert!((g.primal() - 2.5).abs() < 1e-7);
    }

    #[test]
    fn nth_deriv_picks_first_derivative() {
        let j: Jet<f32, 2> = Jet::promote(0.0);
        let g = GpuJet::pack(j).unwrap();
        assert!((g.nth_deriv(1) - 1.0).abs() < 1e-7);
    }

    #[test]
    fn spill_and_reload_round_trip() {
        let j: Jet<f64, 3> = Jet::promote(1.5);
        let g = GpuJet::pack(j).unwrap();
        let mut shared = vec![0.0; 3];
        g.spill_to_shared(&mut shared).unwrap();
        let h: GpuJet<f64, 3> = GpuJet::reload_from_shared(&shared).unwrap();
        assert!((h.primal() - g.primal()).abs() < 1e-12);
        assert!((h.nth_deriv(1) - g.nth_deriv(1)).abs() < 1e-12);
        assert!((h.nth_deriv(2) - g.nth_deriv(2)).abs() < 1e-12);
    }

    #[test]
    fn spill_short_slice_errors() {
        let j: Jet<f32, 4> = Jet::promote(1.0);
        let g = GpuJet::pack(j).unwrap();
        let mut shared = vec![0.0; 2];
        let err = g.spill_to_shared(&mut shared).unwrap_err();
        match err {
            GpuJetError::SharedSliceTooShort { needed, actual } => {
                assert_eq!(needed, 4);
                assert_eq!(actual, 2);
            }
            other => panic!("wrong error : {other:?}"),
        }
    }

    #[test]
    fn fits_in_register_below_limit() {
        let j: Jet<f32, 4> = Jet::lift(1.0);
        let g = GpuJet::pack(j).unwrap();
        assert!(g.fits_in_register());
    }

    #[test]
    fn from_jet_unchecked_constructor() {
        let j: Jet<f32, 8> = Jet::lift(1.0);
        let g: GpuJet<f32, 8> = j.into();
        assert!(!g.fits_in_register());
    }
}
