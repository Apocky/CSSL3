//! § KanNetwork<I, O> — Kolmogorov-Arnold spline-net evaluator (skeleton)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The `02_CSSL/06_SUBSTRATE_EVOLUTION.csl § 4` shape :
//!
//!   ```cssl
//!   @layout(soa) @axiom(10) @differentiable
//!   type KanNetwork<const I: usize, const O: usize> = {
//!     layer_widths:   SmallVec<u16, 8>,
//!     spline_basis:   SplineBasis,
//!     control_points: [[f32; KAN_CTRL]; KAN_LAYERS_MAX],
//!     knot_grid:      [f32; KAN_KNOTS],
//!     edge_activations: [BSplineFn; KAN_EDGE_MAX],
//!     trained:        bool,
//!   }
//!   ```
//!
//!   This crate provides the **minimal eval-skeleton** : enough surface for
//!   `Pattern::stamp(genome, weights, tag)` to fingerprint a KAN net,
//!   `KanMaterial` variants to be instantiated, and the spec-mandated test
//!   coverage. The full training / autodiff / spline-evaluator path lives in
//!   the upstream slice that lands `cssl-kan` (T11-D115 / wave-3β-04 horizon)
//!   ; this crate only needs the byte-stable representation and the
//!   shape-typed eval entry point.
//!
//! § DESIGN — fixed-size storage with const-generic shape
//!   The substrate spec uses `SmallVec<u16, 8>` for layer widths. To keep
//!   this crate dependency-light and `repr(C)`-stable for the upcoming
//!   `cssl-substrate-save` crate's wire format, we use a fixed `[u16; 8]`
//!   array with a `layer_count` field. The semantics are identical : at
//!   most 8 layers, each with a `u16` width. SmallVec<u16, 8>'s on-stack
//!   representation IS this layout.

use crate::handle::{Handle, HandleResolveError};

/// § Maximum number of layers in a KAN network. Matches the spec's
///   `SmallVec<u16, 8>` declaration.
pub const KAN_LAYERS_MAX: usize = 8;

/// § Maximum number of control points per edge. Sized for the prototype
///   spline evaluator ; the full evaluator may bump this in a follow-up
///   slice.
pub const KAN_CTRL: usize = 16;

/// § Number of knots in the global grid.
pub const KAN_KNOTS: usize = 32;

/// § Maximum number of edge-activation functions per network.
pub const KAN_EDGE_MAX: usize = 64;

/// § The spline basis for a KAN edge function. Matches spec `§ 4`
///   declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplineBasis {
    /// § B-spline basis. The canonical KAN choice — spec default.
    #[default]
    BSpline,
    /// § Catmull-Rom basis. Smoother cardinal spline ; useful when the
    ///   training data has natural breakpoints.
    CatmullRom,
    /// § Cubic Hermite. Lowest-order option ; used in the prototype paths.
    Cubic,
}

/// § A KAN network with `I` inputs and `O` outputs.
///
/// Kolmogorov-Arnold Networks replace the per-neuron activation function
/// with per-edge spline functions. The network evaluates an input vector
/// `x ∈ R^I` to an output vector `y ∈ R^O` by composing spline-edges
/// across `layer_count` layers.
///
/// This crate provides the storage shape ; full training + autodiff lives
/// in `cssl-kan` (separate slice).
#[derive(Debug, Clone)]
pub struct KanNetwork<const I: usize, const O: usize> {
    /// § Fixed-size buffer of layer widths. Only the first `layer_count`
    ///   entries are meaningful. The first entry MUST equal `I`, the last
    ///   MUST equal `O` ; intermediate entries are hidden-layer widths.
    pub layer_widths: [u16; KAN_LAYERS_MAX],
    /// § Number of meaningful entries in `layer_widths`.
    pub layer_count: u8,
    /// § The spline basis used for every edge function.
    pub spline_basis: SplineBasis,
    /// § Per-edge control-point grid. The first dimension is the layer
    ///   index ; the second is the per-edge anchor index.
    pub control_points: Box<[[f32; KAN_CTRL]; KAN_LAYERS_MAX]>,
    /// § Global knot grid.
    pub knot_grid: [f32; KAN_KNOTS],
    /// § Whether the network has been trained. Untrained networks emit
    ///   identity-like output ; this is intentional for prototype paths.
    pub trained: bool,
}

impl<const I: usize, const O: usize> KanNetwork<I, O> {
    /// § Construct an untrained (zero-weight) KAN network with the
    ///   default basis (BSpline) and a layer schedule of `[I, O]` (no
    ///   hidden layers — the minimal viable shape).
    #[must_use]
    pub fn new_untrained() -> Self {
        let mut layer_widths = [0u16; KAN_LAYERS_MAX];
        layer_widths[0] = I as u16;
        layer_widths[1] = O as u16;
        Self {
            layer_widths,
            layer_count: 2,
            spline_basis: SplineBasis::BSpline,
            control_points: Box::new([[0.0; KAN_CTRL]; KAN_LAYERS_MAX]),
            knot_grid: [0.0; KAN_KNOTS],
            trained: false,
        }
    }

    /// § Construct with an explicit layer-width schedule. The first entry
    ///   MUST be `I` and the last entry MUST be `O` ; this is checked at
    ///   runtime (returns `None` on mismatch).
    #[must_use]
    pub fn with_layers(widths: &[u16]) -> Option<Self> {
        if widths.len() < 2 || widths.len() > KAN_LAYERS_MAX {
            return None;
        }
        if widths[0] as usize != I || widths[widths.len() - 1] as usize != O {
            return None;
        }
        let mut layer_widths = [0u16; KAN_LAYERS_MAX];
        for (i, w) in widths.iter().enumerate() {
            layer_widths[i] = *w;
        }
        Some(Self {
            layer_widths,
            layer_count: widths.len() as u8,
            spline_basis: SplineBasis::BSpline,
            control_points: Box::new([[0.0; KAN_CTRL]; KAN_LAYERS_MAX]),
            knot_grid: [0.0; KAN_KNOTS],
            trained: false,
        })
    }

    /// § The number of inputs (compile-time constant).
    #[must_use]
    pub const fn input_dim() -> usize {
        I
    }

    /// § The number of outputs (compile-time constant).
    #[must_use]
    pub const fn output_dim() -> usize {
        O
    }

    /// § Number of layers in the schedule.
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layer_count as usize
    }

    /// § True iff `layer_widths[0..layer_count]` is a valid schedule that
    ///   starts with `I` and ends with `O`.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        let n = self.layer_count();
        if !(2..=KAN_LAYERS_MAX).contains(&n) {
            return false;
        }
        self.layer_widths[0] as usize == I && self.layer_widths[n - 1] as usize == O
    }

    /// § Hash the network's byte-stable parts into a 32-byte blake3 digest.
    ///   Used by `Pattern::stamp` to fingerprint the KAN-weight contribution.
    ///
    ///   Hashed fields :
    ///   - `I`, `O` (input/output dims)
    ///   - `layer_count`
    ///   - `layer_widths[0..layer_count]`
    ///   - `spline_basis` discriminant
    ///   - `control_points` (every layer × every anchor)
    ///   - `knot_grid`
    ///   - `trained` bit
    #[must_use]
    pub fn fingerprint_bytes(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(&(I as u64).to_le_bytes());
        h.update(&(O as u64).to_le_bytes());
        h.update(&[self.layer_count]);
        for w in &self.layer_widths[..self.layer_count()] {
            h.update(&w.to_le_bytes());
        }
        let basis_tag: u8 = match self.spline_basis {
            SplineBasis::BSpline => 0,
            SplineBasis::CatmullRom => 1,
            SplineBasis::Cubic => 2,
        };
        h.update(&[basis_tag]);
        for layer in self.control_points.iter() {
            for v in layer {
                h.update(&v.to_le_bytes());
            }
        }
        for k in &self.knot_grid {
            h.update(&k.to_le_bytes());
        }
        h.update(&[u8::from(self.trained)]);
        let digest = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(digest.as_bytes());
        out
    }

    /// § Evaluate the network on an input vector. The reference
    ///   implementation in this crate is a SHAPE-PRESERVING IDENTITY-LIKE
    ///   eval : it returns `[0.0; O]` for an untrained network and a
    ///   shape-correct deterministic output for a trained one. The full
    ///   spline-evaluator lives in `cssl-kan` (T11-D115).
    ///
    ///   This entry-point exists so call-sites that only need to verify
    ///   shape contracts (FieldCell wiring, KanMaterial round-trip,
    ///   `Pattern` integration tests) can compile and exercise the path
    ///   without taking a dep on `cssl-kan`.
    #[must_use]
    pub fn eval(&self, _input: &[f32; I]) -> [f32; O] {
        // § Shape-preserving placeholder — see rustdoc.
        [0.0; O]
    }
}

impl<const I: usize, const O: usize> Default for KanNetwork<I, O> {
    fn default() -> Self {
        Self::new_untrained()
    }
}

/// § A typed handle into a pool of KAN networks. Used by the upcoming
///   `KanRegistry` (separate slice) to look up networks by stable handle.
pub type KanHandle<const I: usize, const O: usize> = Handle<KanNetwork<I, O>>;

/// § Resolve-error alias for KAN-network lookups. Same shape as
///   [`HandleResolveError`].
pub type KanResolveError = HandleResolveError;

#[cfg(test)]
mod tests {
    use super::*;

    /// § new_untrained produces a 2-layer (I, O) schedule.
    #[test]
    fn new_untrained_shape() {
        let net: KanNetwork<32, 16> = KanNetwork::new_untrained();
        assert_eq!(net.layer_count(), 2);
        assert_eq!(net.layer_widths[0], 32);
        assert_eq!(net.layer_widths[1], 16);
        assert!(!net.trained);
        assert!(net.is_well_formed());
    }

    /// § with_layers refuses mismatched I.
    #[test]
    fn with_layers_refuses_mismatched_input() {
        let bad: Option<KanNetwork<32, 16>> = KanNetwork::with_layers(&[8, 16]);
        assert!(bad.is_none());
    }

    /// § with_layers refuses mismatched O.
    #[test]
    fn with_layers_refuses_mismatched_output() {
        let bad: Option<KanNetwork<32, 16>> = KanNetwork::with_layers(&[32, 8]);
        assert!(bad.is_none());
    }

    /// § with_layers refuses too-short.
    #[test]
    fn with_layers_refuses_too_short() {
        let bad: Option<KanNetwork<32, 16>> = KanNetwork::with_layers(&[32]);
        assert!(bad.is_none());
    }

    /// § with_layers refuses too-long.
    #[test]
    fn with_layers_refuses_too_long() {
        let v = vec![32u16; KAN_LAYERS_MAX + 1];
        let bad: Option<KanNetwork<32, 32>> = KanNetwork::with_layers(&v);
        assert!(bad.is_none());
    }

    /// § with_layers accepts valid hidden-layer schedule.
    #[test]
    fn with_layers_accepts_valid_hidden() {
        let net: KanNetwork<32, 16> = KanNetwork::with_layers(&[32, 64, 64, 16]).unwrap();
        assert_eq!(net.layer_count(), 4);
        assert!(net.is_well_formed());
    }

    /// § input_dim / output_dim are compile-time constants.
    #[test]
    fn dims_are_const() {
        assert_eq!(KanNetwork::<32, 16>::input_dim(), 32);
        assert_eq!(KanNetwork::<32, 16>::output_dim(), 16);
    }

    /// § fingerprint_bytes is deterministic.
    #[test]
    fn fingerprint_deterministic() {
        let a: KanNetwork<32, 16> = KanNetwork::new_untrained();
        let b: KanNetwork<32, 16> = KanNetwork::new_untrained();
        assert_eq!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § fingerprint changes when control-point changes.
    #[test]
    fn fingerprint_sensitive_to_weights() {
        let a: KanNetwork<32, 16> = KanNetwork::new_untrained();
        let mut b: KanNetwork<32, 16> = KanNetwork::new_untrained();
        b.control_points[0][0] = 1.0;
        assert_ne!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § fingerprint changes when trained-bit flips.
    #[test]
    fn fingerprint_sensitive_to_trained_bit() {
        let a: KanNetwork<32, 16> = KanNetwork::new_untrained();
        let mut b: KanNetwork<32, 16> = KanNetwork::new_untrained();
        b.trained = true;
        assert_ne!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § fingerprint changes when basis changes.
    #[test]
    fn fingerprint_sensitive_to_basis() {
        let a: KanNetwork<32, 16> = KanNetwork::new_untrained();
        let mut b: KanNetwork<32, 16> = KanNetwork::new_untrained();
        b.spline_basis = SplineBasis::CatmullRom;
        assert_ne!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § Different I/O instantiations have different fingerprints.
    #[test]
    fn fingerprint_sensitive_to_io_dims() {
        let a: KanNetwork<32, 16> = KanNetwork::new_untrained();
        let b: KanNetwork<32, 8> = KanNetwork::new_untrained();
        assert_ne!(a.fingerprint_bytes(), b.fingerprint_bytes());
    }

    /// § Eval of untrained returns zeros.
    #[test]
    #[allow(clippy::float_cmp)]
    fn eval_untrained_returns_zeros() {
        let net: KanNetwork<4, 2> = KanNetwork::new_untrained();
        let input = [1.0, 2.0, 3.0, 4.0];
        let out = net.eval(&input);
        // § Untrained network eval is the all-zero placeholder per rustdoc ;
        //   exact float equality against `0.0` is intentional.
        assert_eq!(out, [0.0, 0.0]);
    }

    /// § Default is untrained.
    #[test]
    fn default_is_untrained() {
        let net: KanNetwork<32, 16> = KanNetwork::default();
        assert!(!net.trained);
    }
}
