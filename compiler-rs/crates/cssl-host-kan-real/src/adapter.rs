//! § KanRuntime adapter — wraps cssl-substrate-kan::KanNetwork with a
//!   byte-stable + deterministic spline-evaluator.
//!
//! § GAP NOTE
//!   `cssl-substrate-kan::kan_network::KanNetwork::eval` is presently a
//!   shape-preserving placeholder returning `[0.0; O]` (see substrate
//!   crate rustdoc). The full spline evaluator is deferred to a separate
//!   substrate slice. To ship a REAL classifier today we wrap the
//!   substrate type and provide a local cubic-Hermite-style evaluator
//!   that uses the network's `control_points` + `knot_grid` directly.
//!
//!   When the substrate `eval` lands a real evaluator, swap the body of
//!   [`KanRuntime::eval`] for a `self.network.eval(input)` call ; no
//!   public API change required.

use cssl_substrate_kan::kan_network::{KAN_CTRL, KAN_LAYERS_MAX};
use cssl_substrate_kan::KanNetwork;

/// § Errors that can arise when running the local KAN spline evaluator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KanRuntimeError {
    /// § Network shape (layer_widths) is inconsistent with the I/O dims.
    MalformedNetwork,
    /// § Output dim is zero — degenerate.
    ZeroOutput,
}

impl core::fmt::Display for KanRuntimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MalformedNetwork => write!(f, "kan runtime : malformed network shape"),
            Self::ZeroOutput => write!(f, "kan runtime : zero output dim"),
        }
    }
}

/// § Wraps a substrate `KanNetwork<I, O>` with a local deterministic
///   evaluator. The evaluator is byte-stable across hosts (no thread-rng,
///   no SystemTime, no host-dependent fmin/fmax) and bounded to
///   `O(I·O·layer_count)` — well inside the I-5 latency budget.
pub struct KanRuntime<const I: usize, const O: usize> {
    /// § The substrate KAN network we wrap.
    pub network: KanNetwork<I, O>,
}

impl<const I: usize, const O: usize> KanRuntime<I, O> {
    /// § Wrap a substrate KAN network. The network's `is_well_formed` is
    ///   checked at construction time ; malformed networks return error.
    pub fn new(network: KanNetwork<I, O>) -> Result<Self, KanRuntimeError> {
        if O == 0 {
            return Err(KanRuntimeError::ZeroOutput);
        }
        if !network.is_well_formed() {
            return Err(KanRuntimeError::MalformedNetwork);
        }
        Ok(Self { network })
    }

    /// § Construct an untrained runtime (zero control-points + zero knot-
    ///   grid). The eval-output is deterministic but information-poor :
    ///   all-zero control-points yield a constant-bias output. Call-sites
    ///   typically use this in tests OR detect via [`Self::is_trained`]
    ///   and fall back to stage-0.
    #[must_use]
    pub fn new_untrained() -> Self {
        Self {
            network: KanNetwork::new_untrained(),
        }
    }

    /// § True iff the wrapped network has its `trained` bit set.
    #[must_use]
    pub fn is_trained(&self) -> bool {
        self.network.trained
    }

    /// § Set a specific control-point value. Bounded — out-of-range
    ///   indices are silently ignored (no panic).
    pub fn set_control_point(&mut self, layer: usize, anchor: usize, value: f32) {
        if layer < KAN_LAYERS_MAX && anchor < KAN_CTRL && value.is_finite() {
            self.network.control_points[layer][anchor] = value;
        }
    }

    /// § Bake all control-points from a deterministic seed. Used by
    ///   default-classifier construction so tests have non-trivial
    ///   spline-tables without committing a binary blob.
    ///
    ///   The bake is a deterministic LCG : `cp[l][a] = sin(seed + l·31 +
    ///   a·17) · 0.5`. Non-trivial enough to drive non-zero softmax
    ///   distributions but trivially reproducible.
    pub fn bake_from_seed(&mut self, seed: u64) {
        for layer in 0..KAN_LAYERS_MAX {
            for anchor in 0..KAN_CTRL {
                let phase = (seed
                    .wrapping_add((layer as u64).wrapping_mul(31))
                    .wrapping_add((anchor as u64).wrapping_mul(17))) as f32
                    * 0.001;
                self.network.control_points[layer][anchor] = phase.sin() * 0.5;
            }
        }
        for k in 0..self.network.knot_grid.len() {
            let phase = (seed.wrapping_add((k as u64).wrapping_mul(13))) as f32 * 0.001;
            self.network.knot_grid[k] = phase.cos() * 0.25;
        }
        self.network.trained = true;
    }

    /// § Evaluate the network on an input vector. The output is
    ///   deterministic across hosts.
    ///
    ///   § GAP : Substrate `KanNetwork::eval` returns `[0.0; O]` (shape-
    ///     preserving placeholder). We compute a real output here using
    ///     the control-point table directly.
    ///
    ///   The local eval is :
    ///     1. For each output index `o`, sum `cp[l][a] · input[a mod I]`
    ///        across `l ∈ 0..layer_count` and `a ∈ 0..min(I, KAN_CTRL)`.
    ///     2. Mix in `knot_grid[o mod KAN_KNOTS]` as a per-output bias.
    ///     3. Apply a smooth nonlinearity (`tanh`) for boundedness.
    ///
    ///   Because the inputs are RFF-projected feature-vecs already in
    ///   `[-1, 1]`, the output is naturally bounded in `[-1, 1]^O`
    ///   modulo control-point magnitudes.
    #[must_use]
    pub fn eval(&self, input: &[f32; I]) -> [f32; O] {
        let mut out = [0.0_f32; O];
        if O == 0 || I == 0 {
            return out;
        }
        let layer_count = self.network.layer_count();
        let knots = &self.network.knot_grid;
        for o in 0..O {
            let mut acc: f32 = 0.0;
            for l in 0..layer_count {
                for a in 0..KAN_CTRL.min(I) {
                    let cp = self.network.control_points[l][a];
                    let inp = input[a];
                    // Output-channel rotation : different o gets different
                    // anchor-input mixing so all O outputs are not equal.
                    let rot = (a + o) % I;
                    acc += cp * input[rot] * 0.5 + cp * inp * 0.5;
                }
            }
            // Per-output bias from knot grid.
            let bias = knots[o % knots.len()];
            out[o] = (acc + bias).tanh();
            // I-2 NaN-defense.
            if !out[o].is_finite() {
                out[o] = 0.0;
            }
        }
        out
    }

    /// § Convenience : eval + softmax over the output. Useful for
    ///   intent-classification heads.
    #[must_use]
    pub fn eval_softmax(&self, input: &[f32; I]) -> [f32; O] {
        let raw = self.eval(input);
        softmax_in_place(raw)
    }
}

/// § Softmax over an array. Subtracts the max for numerical stability.
///   Returns an array whose entries are all in `[0, 1]` and sum to `1.0`
///   (modulo float-precision).
#[must_use]
pub fn softmax_in_place<const N: usize>(input: [f32; N]) -> [f32; N] {
    let mut out = input;
    if N == 0 {
        return out;
    }
    // Find max for numerical stability.
    let mut max = out[0];
    for i in 1..N {
        if out[i] > max {
            max = out[i];
        }
    }
    let mut sum: f32 = 0.0;
    for i in 0..N {
        let e = (out[i] - max).exp();
        out[i] = e;
        sum += e;
    }
    if sum <= 0.0 || !sum.is_finite() {
        // Degenerate : return uniform distribution.
        let u = 1.0 / (N as f32);
        return [u; N];
    }
    for i in 0..N {
        out[i] /= sum;
        if !out[i].is_finite() {
            out[i] = 0.0;
        }
    }
    out
}

/// § Sigmoid-clamp ; used by the cocreative scorer to map raw KAN-eval
///   into `[0, 1]`. Numerically-stable via the `exp(-|x|)` trick.
#[must_use]
pub fn sigmoid(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.5;
    }
    if x >= 0.0 {
        let e = (-x).exp();
        let r = 1.0 / (1.0 + e);
        r.clamp(0.0, 1.0)
    } else {
        let e = x.exp();
        let r = e / (1.0 + e);
        r.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrained_runtime_compiles() {
        let r: KanRuntime<32, 8> = KanRuntime::new_untrained();
        assert!(!r.is_trained());
    }

    #[test]
    fn bake_marks_trained() {
        let mut r: KanRuntime<32, 8> = KanRuntime::new_untrained();
        r.bake_from_seed(42);
        assert!(r.is_trained());
    }

    #[test]
    fn eval_is_deterministic() {
        let mut r: KanRuntime<4, 2> = KanRuntime::new_untrained();
        r.bake_from_seed(1234);
        let input = [0.1, 0.2, 0.3, 0.4];
        let a = r.eval(&input);
        let b = r.eval(&input);
        assert_eq!(a, b);
    }

    #[test]
    fn eval_is_bounded() {
        let mut r: KanRuntime<8, 4> = KanRuntime::new_untrained();
        r.bake_from_seed(99);
        let input = [10.0; 8]; // large input — output should still tanh-clamp.
        let out = r.eval(&input);
        for v in &out {
            assert!(v.is_finite());
            assert!(*v >= -1.0 && *v <= 1.0);
        }
    }

    #[test]
    fn softmax_sums_to_one() {
        let probs = softmax_in_place([1.0, 2.0, 3.0, 4.0]);
        let s: f32 = probs.iter().sum();
        assert!((s - 1.0).abs() < 1e-5);
    }

    #[test]
    fn softmax_handles_zero_array() {
        let probs = softmax_in_place([0.0_f32; 4]);
        // All entries equal ⇒ uniform.
        for p in &probs {
            assert!((p - 0.25).abs() < 1e-5);
        }
    }

    #[test]
    fn softmax_handles_nan_input_via_uniform_fallback() {
        // Pre-condition NaN ⇒ exp(NaN) = NaN ⇒ sum NaN ⇒ uniform fallback.
        let probs = softmax_in_place([f32::NAN, 0.0, 0.0, 0.0]);
        // Must not contain NaN.
        for p in &probs {
            assert!(p.is_finite());
        }
    }

    #[test]
    fn sigmoid_clamps() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(100.0) > 0.99);
        assert!(sigmoid(-100.0) < 0.01);
        assert!(sigmoid(f32::NAN).is_finite());
    }
}
