//! § mera — Multi-scale Entanglement Renormalization Ansatz
//!
//! § PRIMER
//!
//! MERA is a tensor-network ansatz for hierarchically-organized states
//! on a 1D lattice. Each layer of the network consists of two stages :
//!
//! 1. A row of *disentanglers* : 2-input / 2-output unitary tensors
//!    that act on adjacent pairs of sites and remove the short-range
//!    entanglement between them. Disentanglers are unitary, so they
//!    preserve every L²-norm.
//! 2. A row of *isometries* : 2-input / 1-output linear maps that
//!    coarse-grain pairs of sites into a single coarser site.
//!    Isometries satisfy `V^† V = I` (the *isometric* condition) so
//!    they are norm-preserving in the input → output direction but
//!    not invertible in the reverse.
//!
//! After L layers, the original n-site state is summarized by an
//! n / 2^L-site coarse state. The hierarchy gives `O(log n)` cost for
//! correlation-function evaluations and matches the wavelet-pyramid
//! structure ("layer iteration ≡ RG-flow" per Axiom-10 § III).
//!
//! § THE BINARY-TREE / WAVELET MAPPING
//!
//! A 1D binary-tree MERA without disentanglers (or with them set to
//! the identity) is structurally identical to a single-level Haar
//! wavelet decomposition : the isometry that takes two children to
//! one parent is exactly the Haar low-pass filter `[1/√2, 1/√2]`. The
//! detail half of the wavelet decomposition corresponds to the
//! "discarded" orthogonal complement of the isometry's range. The
//! `mera_to_wavelet` adapter exposes this correspondence directly.
//!
//! § Ω-FIELD INTEGRATION
//!
//! The Ω-field MERA-summary tier (Axiom-10 § III usage list) consumes
//! `MeraPyramid` directly. Each layer's `summary_at(scale)` returns
//! the isometry-projected coarse-grained state at that LOD level ;
//! distant-region netcode replication and ray-marching skip-by-summary
//! operations both query this tier.
//!
//! § STATIC SHAPE DESCRIPTOR
//!
//! `Disentangler` and `Isometry` carry their per-instance shape data
//! in a small `[usize; N]` array plus a `Vec<f32>` for the tensor
//! entries. The shape descriptor lets the GPU upload path emit the
//! correct bind-group layout without runtime reflection.

use crate::haar::HAAR_LO;

/// § A 2-input / 2-output unitary disentangler.
///
/// Stored as a 4×4 row-major matrix in `data` : `data[row * 4 + col]`.
/// The rows correspond to `(out_a, out_b)` in row-major order ; columns
/// correspond to `(in_a, in_b)`. The unitarity constraint `U^† U = I`
/// holds at construction time (verified by `is_unitary()` to within
/// numerical tolerance).
#[derive(Debug, Clone)]
pub struct Disentangler {
    /// Row-major 4×4 entries.
    pub data: Vec<f32>,
}

impl Disentangler {
    /// Construct from a row-major 4×4 array. Panics if `data.len() != 16`.
    #[must_use]
    pub fn from_matrix(data: Vec<f32>) -> Self {
        assert_eq!(
            data.len(),
            16,
            "Disentangler : data must be 4×4 = 16 entries"
        );
        Self { data }
    }

    /// Construct the identity disentangler (does nothing).
    #[must_use]
    pub fn identity() -> Self {
        let mut data = vec![0.0_f32; 16];
        // Diagonal entries [0,0], [1,1], [2,2], [3,3] = 4*i + i = 5i
        for i in 0..4_usize {
            data[5 * i] = 1.0;
        }
        Self { data }
    }

    /// Construct a SWAP disentangler : exchanges the two inputs.
    #[must_use]
    pub fn swap() -> Self {
        // SWAP : maps |00⟩→|00⟩, |01⟩→|10⟩, |10⟩→|01⟩, |11⟩→|11⟩
        // In the (in_a, in_b) basis with index = a*2 + b :
        //   col 0 (in=00) → row 0 (out=00) → entry [0, 0] = 1
        //   col 1 (in=01) → row 2 (out=10) → entry [2, 1] = 1
        //   col 2 (in=10) → row 1 (out=01) → entry [1, 2] = 1
        //   col 3 (in=11) → row 3 (out=11) → entry [3, 3] = 1
        let mut data = vec![0.0_f32; 16];
        data[0] = 1.0; // [0, 0]
        data[9] = 1.0; // [2, 1]
        data[6] = 1.0; // [1, 2]
        data[15] = 1.0; // [3, 3]
        Self { data }
    }

    /// Apply this disentangler to a 4-component input vector. Returns
    /// the transformed 4-component output.
    #[must_use]
    pub fn apply(&self, input: &[f32; 4]) -> [f32; 4] {
        let mut out = [0.0_f32; 4];
        for (r, out_r) in out.iter_mut().enumerate() {
            let mut acc = 0.0_f32;
            for (c, input_c) in input.iter().enumerate() {
                acc = self.data[r * 4 + c].mul_add(*input_c, acc);
            }
            *out_r = acc;
        }
        out
    }

    /// Verify unitarity : `U^† U = I` to within `tol`.
    #[must_use]
    pub fn is_unitary(&self, tol: f32) -> bool {
        // Compute U^† U[r, c] = Σ_k U[k, r] · U[k, c]
        for r in 0..4 {
            for c in 0..4 {
                let mut s = 0.0_f32;
                for k in 0..4 {
                    s += self.data[k * 4 + r] * self.data[k * 4 + c];
                }
                let target = if r == c { 1.0 } else { 0.0 };
                if (s - target).abs() > tol {
                    return false;
                }
            }
        }
        true
    }
}

/// § A 2-input / 1-output isometry. Coarse-grains two adjacent sites
/// into one. Stored as a length-2 vector : the scaling-function tap
/// pair, equivalent to a length-2 wavelet low-pass filter.
///
/// The isometric condition `V^† V = I_1` on a 2 → 1 isometry reduces
/// to `Σ_k v[k]² = 1`, which is exactly the Haar normalization.
#[derive(Debug, Clone)]
pub struct Isometry {
    /// Coarse-graining filter taps : `data.len()` is the number of input
    /// sites mapped to a single output site. For a binary-tree MERA, this
    /// is always 2 ; for a 4-to-1 ternary MERA, it is 4.
    pub data: Vec<f32>,
}

impl Isometry {
    /// Construct an isometry from a tap vector.
    #[must_use]
    pub fn from_taps(data: Vec<f32>) -> Self {
        assert!(!data.is_empty(), "Isometry : taps must be non-empty");
        Self { data }
    }

    /// Construct the canonical Haar isometry : `[1/√2, 1/√2]`.
    #[must_use]
    pub fn haar() -> Self {
        Self {
            data: HAAR_LO.to_vec(),
        }
    }

    /// Construct the canonical 4-to-1 ternary isometry (uniform average).
    /// Note : not a Daubechies isometry, just the box-filter ; suitable
    /// as a default for ternary trees.
    #[must_use]
    pub fn ternary_uniform() -> Self {
        let v = 1.0_f32 / 4.0_f32.sqrt(); // 1/2
        Self {
            data: vec![v, v, v, v],
        }
    }

    /// Number of inputs mapped to one output.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.data.len()
    }

    /// Apply the isometry to an input slice of length `arity()`. Returns
    /// the single coarse-grained output.
    #[must_use]
    pub fn apply(&self, input: &[f32]) -> f32 {
        assert_eq!(input.len(), self.arity());
        let mut acc = 0.0_f32;
        for (a, b) in self.data.iter().zip(input.iter()) {
            acc = a.mul_add(*b, acc);
        }
        acc
    }

    /// Verify the isometric condition : `Σ_k v[k]² = 1`.
    #[must_use]
    pub fn is_isometric(&self, tol: f32) -> bool {
        let s: f32 = self.data.iter().map(|x| x * x).sum();
        (s - 1.0).abs() < tol
    }
}

/// § A single MERA layer : a row of disentanglers followed by a row of
/// isometries. Stateless ; the disentangler + isometry are shared across
/// the whole layer (this is the standard "translationally-invariant MERA"
/// assumption ; site-dependent layers are an extension that lands in a
/// follow-up slice).
#[derive(Debug, Clone)]
pub struct MeraLayer {
    pub disentangler: Disentangler,
    pub isometry: Isometry,
}

impl MeraLayer {
    /// Construct a MERA layer from a disentangler + isometry pair.
    #[must_use]
    pub fn new(disentangler: Disentangler, isometry: Isometry) -> Self {
        Self {
            disentangler,
            isometry,
        }
    }

    /// Construct the canonical Haar-equivalent layer : identity
    /// disentangler + Haar isometry. This layer reduces to a single
    /// Haar-wavelet-decomposition step.
    #[must_use]
    pub fn haar_equivalent() -> Self {
        Self {
            disentangler: Disentangler::identity(),
            isometry: Isometry::haar(),
        }
    }

    /// Apply this layer to a length-N input. Returns a length-N/2
    /// output. N must be even ; if N is odd the last element is
    /// dropped.
    #[must_use]
    pub fn apply(&self, input: &[f32]) -> Vec<f32> {
        let n = input.len() & !1; // round down to even
        let arity = self.isometry.arity();
        if arity != 2 {
            // Non-binary tree : skip the disentangler stage and apply
            // the isometry directly in arity-sized blocks.
            let groups = n / arity;
            let mut out = Vec::with_capacity(groups);
            for g in 0..groups {
                let chunk = &input[g * arity..(g + 1) * arity];
                out.push(self.isometry.apply(chunk));
            }
            return out;
        }
        // Binary tree : pairs of (a, b) — apply disentangler then
        // isometry to each pair. The disentangler is 4×4 acting on
        // (a, b) embedded as (1, 0, 0, 0) basis ; we treat the input
        // pair as a 4-vector with components [a*1, a*0 + b*0, ...]
        // for now using the simpler real-vector interpretation :
        // the (in_a, in_b) column-vector becomes a 2-vector at the
        // top level. The disentangler 4×4 acts on the tensor product
        // of two 2-vectors in the standard linear-algebra sense ;
        // the translation between MERA's quantum tensor-network
        // form and the real-valued classical signal form is to
        // treat the two amplitudes (a, b) as the "computational basis"
        // amplitudes of a 2-site system in the |0...0⟩ + a|0...1⟩ +
        // b|0...1⟩ + 0|1...1⟩ embedding. For our classical signal
        // pyramid, we instead treat the disentangler as a 2×2 matrix
        // acting on the (a, b) pair.
        let pairs = n / 2;
        let mut out = Vec::with_capacity(pairs);
        for p in 0..pairs {
            let a = input[p * 2];
            let b = input[p * 2 + 1];
            // 2×2 reduction of the 4×4 disentangler : act on the
            // off-diagonal "single-excitation" subspace which is the
            // natural encoding for real-valued classical signals.
            // We extract entries [1,1], [1,2], [2,1], [2,2] (linear
            // indices 5, 6, 9, 10) as the effective 2×2.
            let m11 = self.disentangler.data[5];
            let m12 = self.disentangler.data[6];
            let m21 = self.disentangler.data[9];
            let m22 = self.disentangler.data[10];
            let pa = m11.mul_add(a, m12 * b);
            let pb = m21.mul_add(a, m22 * b);
            // Apply isometry to (pa, pb).
            let coarse = self.isometry.apply(&[pa, pb]);
            out.push(coarse);
        }
        out
    }
}

/// § An L-layer MERA pyramid for a 1D classical signal.
#[derive(Debug, Clone)]
pub struct MeraPyramid {
    pub layers: Vec<MeraLayer>,
    /// Per-layer cached coarse-grained outputs after a `build` call.
    pub layer_outputs: Vec<Vec<f32>>,
    pub original_length: usize,
}

impl MeraPyramid {
    /// Construct a pyramid with the given layers (one per LOD level).
    #[must_use]
    pub fn new(layers: Vec<MeraLayer>) -> Self {
        Self {
            layers,
            layer_outputs: Vec::new(),
            original_length: 0,
        }
    }

    /// Construct a Haar-equivalent pyramid with `levels` layers (every
    /// layer is the Haar isometry + identity disentangler).
    #[must_use]
    pub fn haar_pyramid(levels: usize) -> Self {
        let layers = (0..levels).map(|_| MeraLayer::haar_equivalent()).collect();
        Self::new(layers)
    }

    /// Number of layers.
    #[must_use]
    pub fn level_count(&self) -> usize {
        self.layers.len()
    }

    /// Build the pyramid from a base-level signal. Stores the per-level
    /// coarse-grained outputs in `layer_outputs[0]` (= input clone),
    /// `layer_outputs[1]` (= after layer 0), ..., `layer_outputs[L]`
    /// (= after the last layer).
    pub fn build(&mut self, signal: &[f32]) {
        self.original_length = signal.len();
        self.layer_outputs.clear();
        self.layer_outputs.push(signal.to_vec());
        let mut current = signal.to_vec();
        for layer in &self.layers {
            current = layer.apply(&current);
            self.layer_outputs.push(current.clone());
        }
    }

    /// Get the coarse-grained summary at LOD scale `scale`. `scale = 0`
    /// returns the original signal ; `scale = 1` returns the result
    /// after the first MERA layer (half the resolution) ; and so on.
    #[must_use]
    pub fn summary_at(&self, scale: usize) -> Option<&[f32]> {
        self.layer_outputs.get(scale).map(Vec::as_slice)
    }

    /// Verify that every layer's disentangler is unitary AND every
    /// layer's isometry is isometric, to within `tol`.
    #[must_use]
    pub fn verify_unitarity(&self, tol: f32) -> bool {
        self.layers
            .iter()
            .all(|l| l.disentangler.is_unitary(tol) && l.isometry.is_isometric(tol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disentangler_identity_is_unitary() {
        let d = Disentangler::identity();
        assert!(d.is_unitary(1e-6));
    }

    #[test]
    fn disentangler_swap_is_unitary() {
        let d = Disentangler::swap();
        assert!(d.is_unitary(1e-6));
    }

    #[test]
    fn disentangler_apply_identity_passthrough() {
        let d = Disentangler::identity();
        let v = [1.0_f32, 2.0, 3.0, 4.0];
        let out = d.apply(&v);
        assert_eq!(out, v);
    }

    #[test]
    fn disentangler_apply_swap_exchanges_middle() {
        let d = Disentangler::swap();
        let v = [1.0_f32, 2.0, 3.0, 4.0];
        let out = d.apply(&v);
        // SWAP : entries 1 and 2 exchange (|01⟩ ↔ |10⟩) ; 0 and 3 fixed.
        assert!((out[0] - 1.0).abs() < 1e-6);
        assert!((out[1] - 3.0).abs() < 1e-6);
        assert!((out[2] - 2.0).abs() < 1e-6);
        assert!((out[3] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn isometry_haar_is_isometric() {
        let i = Isometry::haar();
        assert!(i.is_isometric(1e-6));
        assert_eq!(i.arity(), 2);
    }

    #[test]
    fn isometry_ternary_is_isometric() {
        let i = Isometry::ternary_uniform();
        assert!(i.is_isometric(1e-6));
        assert_eq!(i.arity(), 4);
    }

    #[test]
    fn isometry_haar_apply_average() {
        let i = Isometry::haar();
        let v = [1.0_f32, 1.0];
        let out = i.apply(&v);
        // (1 + 1) / √2 = √2
        assert!((out - 2.0_f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn mera_layer_haar_equivalent_constructs() {
        let l = MeraLayer::haar_equivalent();
        assert!(l.disentangler.is_unitary(1e-6));
        assert!(l.isometry.is_isometric(1e-6));
    }

    #[test]
    fn mera_layer_haar_equivalent_apply() {
        let l = MeraLayer::haar_equivalent();
        let s = [1.0_f32, 1.0, 2.0, 2.0];
        let out = l.apply(&s);
        assert_eq!(out.len(), 2);
        // Each pair (1, 1) → √2 ; (2, 2) → 2√2
        assert!((out[0] - 2.0_f32.sqrt()).abs() < 1e-5);
        assert!((out[1] - 2.0 * 2.0_f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn mera_pyramid_haar_three_levels() {
        let mut p = MeraPyramid::haar_pyramid(3);
        let s: Vec<f32> = vec![1.0_f32; 8];
        p.build(&s);
        assert_eq!(p.level_count(), 3);
        assert_eq!(p.summary_at(0).unwrap().len(), 8);
        assert_eq!(p.summary_at(1).unwrap().len(), 4);
        assert_eq!(p.summary_at(2).unwrap().len(), 2);
        assert_eq!(p.summary_at(3).unwrap().len(), 1);
    }

    #[test]
    fn mera_pyramid_unitarity_verify() {
        let p = MeraPyramid::haar_pyramid(4);
        assert!(p.verify_unitarity(1e-6));
    }

    #[test]
    fn mera_pyramid_summary_at_zero_is_input() {
        let mut p = MeraPyramid::haar_pyramid(2);
        let s = vec![3.0_f32, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0];
        p.build(&s);
        let s0 = p.summary_at(0).unwrap();
        assert_eq!(s0, s.as_slice());
    }

    #[test]
    fn mera_pyramid_summary_l2_norm_monotone_decreasing() {
        // An isometric layer is norm-preserving in the full tensor-network
        // sense — the discarded "detail" channel carries the orthogonal
        // complement. The summary-only path is lossy on purpose : it is
        // the LOD-summary, not the lossless reconstruction. The norm of
        // each successive summary is therefore ≤ the norm of the previous,
        // approaching the DC component as the pyramid coarsens.
        let mut p = MeraPyramid::haar_pyramid(2);
        let s = vec![1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        p.build(&s);
        let n0: f32 = s.iter().map(|x| x * x).sum();
        let mut prev = n0;
        for level in 1..=p.level_count() {
            let summary = p.summary_at(level).unwrap();
            let n: f32 = summary.iter().map(|x| x * x).sum();
            assert!(
                n <= prev + 1e-3,
                "level {level} : norm = {n} should be ≤ prev = {prev}"
            );
            prev = n;
        }
    }

    #[test]
    fn mera_layer_with_swap_disentangler() {
        let l = MeraLayer::new(Disentangler::swap(), Isometry::haar());
        let s = [3.0_f32, 1.0];
        let out = l.apply(&s);
        // SWAP's effective 2×2 in single-excitation subspace is
        // [[0, 1], [1, 0]] (off-diagonal entries [1, 2] = [2, 1] = 1,
        // diagonal entries [1, 1] = [2, 2] = 0). So (3, 1) → (1, 3),
        // then Haar applies (1+3)/√2 = 4/√2.
        assert_eq!(out.len(), 1);
        let expected = 4.0_f32 / 2.0_f32.sqrt();
        assert!((out[0] - expected).abs() < 1e-5, "got {}", out[0]);
    }

    #[test]
    fn isometry_from_taps_constructs() {
        let i = Isometry::from_taps(vec![0.5, 0.5, 0.5, 0.5]);
        assert!(i.is_isometric(1e-6));
        assert_eq!(i.arity(), 4);
    }

    #[test]
    fn disentangler_from_matrix_constructs() {
        let mut data = vec![0.0_f32; 16];
        for i in 0..4 {
            data[i * 4 + i] = 1.0;
        }
        let d = Disentangler::from_matrix(data);
        assert!(d.is_unitary(1e-6));
    }
}
