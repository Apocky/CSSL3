//! § cssl-wavelet — wavelet basis + MERA tensor-network primitives
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Multi-resolution analysis substrate for CSSLv3. Provides the canonical
//!   wavelet bases (`Haar`, `Daubechies<N>` for N ∈ {2, 4, 6, 8}, `MexicanHat`)
//!   together with the MERA tensor-network templates (disentanglers,
//!   isometries, layered pyramids) that Axiom-10 § III calls for. The
//!   radiance-cascade GI subsystem (07_AESTHETIC/02) and the Ω-field
//!   MERA-summary tier (Axiom-10 § III usage list) consume this crate.
//!
//! § SPEC ANCHOR
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl § III` — MERA primitive,
//!     disentangler + isometry + layer-iteration ≡ RG-flow.
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl § IV` — radiance-cascades probe
//!     hierarchy, naturally consumes a wavelet-style multi-band split.
//!   - `Omniverse/07_AESTHETIC/02_RADIANCE_CASCADE_GI.csl` — five-band
//!     cascade (LIGHT/HEAT/MANA/SCENT/AUDIO) integrates with multi-band
//!     wavelet probes from this crate.
//!   - Daubechies (1988) — "Orthonormal bases of compactly supported
//!     wavelets" : N-tap construction with N/2 vanishing moments.
//!   - Vidal (2007/2008) — "Entanglement renormalization" + "Class of
//!     quantum many-body states that can be efficiently simulated" :
//!     MERA tensor-network primitives.
//!   - Ricker (1953) — "The form and laws of propagation of seismic
//!     wavelets" : Mexican-hat continuous wavelet.
//!
//! § THE FIVE PIECES
//!   - **Haar** : the simplest orthonormal wavelet. 2-tap filter,
//!     piecewise-constant. The reference oracle for orthonormality +
//!     perfect-reconstruction tests in this crate.
//!   - **Daubechies\<N\>** : N-tap orthonormal compactly-supported
//!     wavelets with N/2 vanishing moments. N ∈ {2, 4, 6, 8} ; N=2 is
//!     equivalent to Haar (rescaled), so the canonical Daubechies family
//!     really starts at N=4 (often called "db2" in the standard naming
//!     because the index there counts vanishing-moment pairs). This crate
//!     uses the *filter-tap-count* convention so `Daubechies::<4>` has 4
//!     filter taps and 2 vanishing moments.
//!   - **MexicanHat** (Ricker) : continuous wavelet
//!     `ψ(t) = (1 − t²) · exp(−t² / 2)` (up to normalization). Used for
//!     scale-space analysis and edge detection ; not a discrete-tap filter
//!     so the multi-resolution decomposition path uses fixed-scale sampling.
//!   - **Multi-Resolution Analysis (MRA)** : the `decompose(signal, levels)`
//!     and `reconstruct` paths that build the standard wavelet pyramid
//!     of approximation + detail coefficients. Hooks for radiance-cascade
//!     style multi-band probe pyramids live in the `cascade` module.
//!   - **MERA tensor-network templates** : `Disentangler` (2-input/2-output
//!     unitary that strips local short-range entanglement), `Isometry`
//!     (N-input/1-output coarse-graining map), `MeraPyramid<L>` (L-level
//!     binary-tree composition for a 1D MERA layout). The Ω-field
//!     `cells-storage` path uses these as the LOD-summary tier.
//!
//! § STORAGE LAYOUT
//!   Filters are stored as `&'static [f32]` slices ; `Vec<f32>` is reserved
//!   for the multi-resolution coefficient outputs. The MERA primitive types
//!   (`Disentangler`, `Isometry`) hold their tensor data in a `Vec<f32>`
//!   plus a small static shape descriptor — this matches the natural
//!   layout for the GPU upload path that consumes the same data via
//!   `cssl-substrate-omega-tensor` integration in a follow-up slice.
//!
//! § NUMERIC STABILITY DISCIPLINE
//!   All forward + inverse transforms are TOTAL on finite inputs. Boundary
//!   handling at the ends of a finite-length signal uses the canonical
//!   *periodic-extension* convention by default (matches the standard
//!   reference implementation), with the `BoundaryMode` enum exposing
//!   `Periodic`, `Symmetric`, and `Zero` for callers that need the other
//!   conventional choices. Energy-conserving + perfect-reconstruction
//!   are part of the test surface — `recon(decompose(x))` returns the
//!   original signal to within `1e-5` (f32) / `1e-12` (f64) on every
//!   wavelet basis defined in this crate.
//!
//! § f32 / f64 VARIANTS
//!   The default surface is `f32`-only to match game-engine + renderer hot
//!   paths. The `f64` feature enables the parallel `*_f64` surface for
//!   scientific-precision workloads (long signal lengths where roundoff
//!   accumulates, audio-band analysis, etc.). Both variants share the
//!   same filter-tap tables — the f64 path simply runs the same arithmetic
//!   in 64-bit precision.
//!
//! § Ω-FIELD INTEGRATION
//!   The Ω-field `MERA-summary-tier` (Axiom-10 § III usage list) is the
//!   primary downstream consumer outside the renderer. The tier consumes
//!   `MeraPyramid<L>` to coarse-grain the cells-storage hierarchy ; the
//!   `summary_at(scale: usize)` accessor returns the isometry-projected
//!   summary for that LOD level.
//!
//! § PRIME-DIRECTIVE
//!   Pure compute. No I/O, no logging, no allocation outside the
//!   coefficient `Vec<f32>` outputs that callers explicitly request.
//!   Behavior is what it appears to be — total, deterministic, transparent.
//!   Energy-conservation + perfect-reconstruction are property-tested
//!   against the orthonormality of the underlying filter banks.
//!
//! § SIMD-AWARENESS
//!   Filter-bank convolutions use `f32::mul_add` for FMA paths so the
//!   compiler auto-vectorizes the natural loops on x86_64-v3 / AArch64-NEON.
//!   Hand-rolled SSE/AVX paths are deferred to the perf slice that lands
//!   when the renderer's hot loops profile this crate.
//!
//! § CSL3 NATIVE
//!   This crate's reasoning is in CSL3 ; English prose only on rustdoc
//!   pub-items + onboarding text per ~/.claude/CLAUDE.md notation default.

#![forbid(unsafe_code)]
// Wavelet computations are intrinsically lossy (multi-resolution f32 cascades,
// integer-grid → real conversions, signed-index boundary arithmetic). Per
// `04_OMEGA_FIELD/05_DENSITY_BUDGET.csl` the precision-loss + cast-wrap +
// multiply-add are domain-justified — tests pin specific dyadic values where
// exactness matters ; production accepts the f32 bounds.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::match_same_arms,
    clippy::float_cmp
)]

pub mod boundary;
pub mod cascade;
pub mod daubechies;
pub mod haar;
pub mod mera;
pub mod mexican_hat;
pub mod mra;
pub mod qmf;

#[cfg(feature = "f64")]
pub mod f64_path;

pub use boundary::{extend_periodic, extend_symmetric, extend_zero, BoundaryMode};
pub use cascade::{CascadeBand, CascadeProbePyramid, ProbeCoarsen};
pub use daubechies::{Daubechies, DAUB2_LO, DAUB4_LO, DAUB6_LO, DAUB8_LO};
pub use haar::Haar;
pub use mera::{Disentangler, Isometry, MeraLayer, MeraPyramid};
pub use mexican_hat::{mexican_hat, MexicanHat, MexicanHatScale};
pub use mra::{MraCoeffs, MultiResolution};
pub use qmf::{Qmf, QmfPair};

/// § The wavelet-basis trait. Every wavelet defined in this crate
/// implements `forward_1d` + `inverse_1d` ; orthonormal wavelets in addition
/// implement `is_orthonormal()` and exhibit perfect-reconstruction such
/// that `inverse_1d(forward_1d(x)) ≈ x` to within numerical precision.
///
/// The transform splits a signal of length `2n` into an approximation half
/// (`a[0..n]`) and a detail half (`d[0..n]`), packed in that order in the
/// returned vector. The inverse interleaves the two halves back into the
/// original signal length.
pub trait WaveletBasis {
    /// Compute the forward 1D discrete wavelet transform on `signal`.
    /// Returns `[approx; detail]` of length `signal.len()`. The signal length
    /// must be a positive even number ; an empty or odd-length signal returns
    /// the input cloned.
    fn forward_1d(&self, signal: &[f32], boundary: BoundaryMode) -> Vec<f32>;

    /// Compute the inverse 1D discrete wavelet transform from the packed
    /// `[approx; detail]` representation. Returns the reconstructed signal
    /// of the same length.
    fn inverse_1d(&self, coeffs: &[f32], boundary: BoundaryMode) -> Vec<f32>;

    /// Whether this wavelet basis is orthonormal (i.e. perfect-reconstruction
    /// holds and the analysis filters equal the synthesis filters up to
    /// time-reversal). Haar + every Daubechies-N here is orthonormal ;
    /// MexicanHat is a continuous wavelet and reports `false`.
    fn is_orthonormal(&self) -> bool;

    /// Number of filter taps. For continuous wavelets, returns `usize::MAX`
    /// (no finite tap count) ; the MRA / decompose path then routes to the
    /// continuous-evaluation branch instead of the QMF convolution branch.
    fn tap_count(&self) -> usize;

    /// 2D forward transform : tensor-product of 1D transforms applied
    /// row-major then column-major. `width` * `height` must equal
    /// `signal.len()`. Both dimensions must be even.
    fn forward_2d(
        &self,
        signal: &[f32],
        width: usize,
        height: usize,
        boundary: BoundaryMode,
    ) -> Vec<f32> {
        assert_eq!(
            signal.len(),
            width * height,
            "cssl-wavelet : signal length must equal width*height for forward_2d"
        );
        assert!(
            width % 2 == 0 && height % 2 == 0,
            "cssl-wavelet : both 2D dimensions must be even"
        );
        let mut row_passed = vec![0.0_f32; signal.len()];
        for y in 0..height {
            let row_in = &signal[y * width..(y + 1) * width];
            let row_out = self.forward_1d(row_in, boundary);
            row_passed[y * width..(y + 1) * width].copy_from_slice(&row_out);
        }
        let mut out = vec![0.0_f32; signal.len()];
        let mut col_buf = vec![0.0_f32; height];
        let mut col_out_buf = vec![0.0_f32; height];
        for x in 0..width {
            for y in 0..height {
                col_buf[y] = row_passed[y * width + x];
            }
            let col_out = self.forward_1d(&col_buf, boundary);
            col_out_buf.copy_from_slice(&col_out);
            for y in 0..height {
                out[y * width + x] = col_out_buf[y];
            }
        }
        out
    }

    /// 2D inverse transform : the dual of `forward_2d`. Inverts the
    /// column-pass first, then the row-pass.
    fn inverse_2d(
        &self,
        coeffs: &[f32],
        width: usize,
        height: usize,
        boundary: BoundaryMode,
    ) -> Vec<f32> {
        assert_eq!(
            coeffs.len(),
            width * height,
            "cssl-wavelet : coeff length must equal width*height for inverse_2d"
        );
        assert!(
            width % 2 == 0 && height % 2 == 0,
            "cssl-wavelet : both 2D dimensions must be even"
        );
        let mut col_inverted = vec![0.0_f32; coeffs.len()];
        let mut col_buf = vec![0.0_f32; height];
        let mut col_out_buf = vec![0.0_f32; height];
        for x in 0..width {
            for y in 0..height {
                col_buf[y] = coeffs[y * width + x];
            }
            let col_out = self.inverse_1d(&col_buf, boundary);
            col_out_buf.copy_from_slice(&col_out);
            for y in 0..height {
                col_inverted[y * width + x] = col_out_buf[y];
            }
        }
        let mut out = vec![0.0_f32; coeffs.len()];
        for y in 0..height {
            let row_in = &col_inverted[y * width..(y + 1) * width];
            let row_out = self.inverse_1d(row_in, boundary);
            out[y * width..(y + 1) * width].copy_from_slice(&row_out);
        }
        out
    }

    /// 3D forward transform : tensor-product of 1D transforms applied
    /// along each of the three axes. `width` * `height` * `depth` must equal
    /// `signal.len()`. All three dimensions must be even.
    fn forward_3d(
        &self,
        signal: &[f32],
        width: usize,
        height: usize,
        depth: usize,
        boundary: BoundaryMode,
    ) -> Vec<f32> {
        assert_eq!(
            signal.len(),
            width * height * depth,
            "cssl-wavelet : signal length must equal width*height*depth for forward_3d"
        );
        assert!(
            width % 2 == 0 && height % 2 == 0 && depth % 2 == 0,
            "cssl-wavelet : all 3D dimensions must be even"
        );
        // X-pass
        let mut x_passed = vec![0.0_f32; signal.len()];
        for z in 0..depth {
            for y in 0..height {
                let base = (z * height + y) * width;
                let row_in = &signal[base..base + width];
                let row_out = self.forward_1d(row_in, boundary);
                x_passed[base..base + width].copy_from_slice(&row_out);
            }
        }
        // Y-pass
        let mut y_passed = vec![0.0_f32; signal.len()];
        let mut col_buf = vec![0.0_f32; height];
        for z in 0..depth {
            for x in 0..width {
                for y in 0..height {
                    col_buf[y] = x_passed[(z * height + y) * width + x];
                }
                let col_out = self.forward_1d(&col_buf, boundary);
                for y in 0..height {
                    y_passed[(z * height + y) * width + x] = col_out[y];
                }
            }
        }
        // Z-pass
        let mut out = vec![0.0_f32; signal.len()];
        let mut zcol_buf = vec![0.0_f32; depth];
        for y in 0..height {
            for x in 0..width {
                for z in 0..depth {
                    zcol_buf[z] = y_passed[(z * height + y) * width + x];
                }
                let zcol_out = self.forward_1d(&zcol_buf, boundary);
                for z in 0..depth {
                    out[(z * height + y) * width + x] = zcol_out[z];
                }
            }
        }
        out
    }

    /// 3D inverse transform : the dual of `forward_3d`. Inverts the
    /// Z-pass, then Y-pass, then X-pass.
    fn inverse_3d(
        &self,
        coeffs: &[f32],
        width: usize,
        height: usize,
        depth: usize,
        boundary: BoundaryMode,
    ) -> Vec<f32> {
        assert_eq!(
            coeffs.len(),
            width * height * depth,
            "cssl-wavelet : coeff length must equal width*height*depth for inverse_3d"
        );
        assert!(
            width % 2 == 0 && height % 2 == 0 && depth % 2 == 0,
            "cssl-wavelet : all 3D dimensions must be even"
        );
        // Z-inverse
        let mut z_inverted = vec![0.0_f32; coeffs.len()];
        let mut zcol_buf = vec![0.0_f32; depth];
        for y in 0..height {
            for x in 0..width {
                for z in 0..depth {
                    zcol_buf[z] = coeffs[(z * height + y) * width + x];
                }
                let zcol_out = self.inverse_1d(&zcol_buf, boundary);
                for z in 0..depth {
                    z_inverted[(z * height + y) * width + x] = zcol_out[z];
                }
            }
        }
        // Y-inverse
        let mut y_inverted = vec![0.0_f32; coeffs.len()];
        let mut col_buf = vec![0.0_f32; height];
        for z in 0..depth {
            for x in 0..width {
                for y in 0..height {
                    col_buf[y] = z_inverted[(z * height + y) * width + x];
                }
                let col_out = self.inverse_1d(&col_buf, boundary);
                for y in 0..height {
                    y_inverted[(z * height + y) * width + x] = col_out[y];
                }
            }
        }
        // X-inverse
        let mut out = vec![0.0_f32; coeffs.len()];
        for z in 0..depth {
            for y in 0..height {
                let base = (z * height + y) * width;
                let row_in = &y_inverted[base..base + width];
                let row_out = self.inverse_1d(row_in, boundary);
                out[base..base + width].copy_from_slice(&row_out);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq_slice(a: &[f32], b: &[f32], tol: f32) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < tol)
    }

    #[test]
    fn lib_smoke_haar_roundtrip_2d() {
        let h = Haar::new();
        let img: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let fwd = h.forward_2d(&img, 4, 4, BoundaryMode::Periodic);
        let recon = h.inverse_2d(&fwd, 4, 4, BoundaryMode::Periodic);
        assert!(
            approx_eq_slice(&img, &recon, 1e-4),
            "Haar 2D roundtrip failed : got {recon:?}"
        );
    }

    #[test]
    fn lib_smoke_haar_roundtrip_3d() {
        let h = Haar::new();
        let vol: Vec<f32> = (0..64).map(|i| i as f32 * 0.1).collect();
        let fwd = h.forward_3d(&vol, 4, 4, 4, BoundaryMode::Periodic);
        let recon = h.inverse_3d(&fwd, 4, 4, 4, BoundaryMode::Periodic);
        assert!(
            approx_eq_slice(&vol, &recon, 1e-4),
            "Haar 3D roundtrip failed"
        );
    }

    #[test]
    fn lib_haar_is_orthonormal() {
        let h = Haar::new();
        assert!(h.is_orthonormal());
        assert_eq!(h.tap_count(), 2);
    }

    #[test]
    fn lib_daub2_equals_haar_up_to_scale() {
        // db1 (the Daubechies-2 family in the standard naming) ≡ Haar
        // up to filter normalization. This crate's `Daubechies::<2>` is
        // the haar-equivalent member.
        let d = Daubechies::<2>::new();
        assert!(d.is_orthonormal());
        assert_eq!(d.tap_count(), 2);
    }

    #[test]
    fn lib_daub4_orthonormal_signal_roundtrip() {
        let d = Daubechies::<4>::new();
        let sig: Vec<f32> = (0..16).map(|i| (i as f32 * 0.5).sin()).collect();
        let fwd = d.forward_1d(&sig, BoundaryMode::Periodic);
        let recon = d.inverse_1d(&fwd, BoundaryMode::Periodic);
        assert!(
            approx_eq_slice(&sig, &recon, 1e-4),
            "Daubechies-4 roundtrip failed"
        );
    }

    #[test]
    fn lib_mexican_hat_continuous_eval() {
        let mh = MexicanHat::new(1.0);
        // Mexican hat at t=0 should be 1 (in unnormalized form), positive peak
        let v0 = mh.evaluate(0.0);
        assert!(v0 > 0.0, "Mexican hat at origin should be positive : {v0}");
        // Should have zeros at t = ±1 (where 1 - t² = 0)
        let v1 = mh.evaluate(1.0);
        assert!(v1.abs() < 1e-5, "Mexican hat at t=1 should be ~0 : {v1}");
    }
}
