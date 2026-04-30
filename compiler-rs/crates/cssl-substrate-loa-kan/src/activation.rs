//! § ParametricActivation — per-cell KAN-edge activation function.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Where a standard MLP fixes its activation function (ReLU / Tanh /
//!   GELU) at architecture-time, a KAN replaces per-neuron activation with
//!   per-edge spline functions. This module encodes a **per-cell**
//!   activation specialization : each Sovereign-claimed cell can carry its
//!   own activation function, enabling region-specific behavior modulation
//!   on the same downstream KanMaterial / KanNetwork instance without
//!   minting one network per cell.
//!
//! § DESIGN — fixed-shape parameter buffer
//!   Activation parameters are stored in a fixed `[f32; ACTIVATION_PARAM_MAX]`
//!   buffer, tagged with [`ActivationKind`] to discriminate the parameter
//!   semantics. This keeps the per-cell overlay-cell at a known compile-time
//!   size (8B kind+pad + 64B params = 72B-aligned), matching the std430-
//!   discipline established by the canonical FieldCell.
//!
//! § PARAMETER-SEMANTICS (per kind)
//!   - `BSplineEdge`     : 16 control points (params[0..16])
//!   - `Sigmoid`         : 2 params : (gain, bias)
//!   - `Tanh`            : 2 params : (gain, bias)
//!   - `Gaussian`        : 3 params : (mean, sigma, amplitude)
//!   - `RadialBasis`     : 4 params : (center_x, center_y, sigma, amplitude)
//!   - `Polynomial`      : up to 8 params : a0..a7 for `Σ ai * x^i`
//!   - `Identity`        : no params (default for unclaimed cells)
//!
//! § SPEC
//!   - `specs/33_F1_F6_LANGUAGE_FEATURES.csl` § F1.1 — Jet<T,N> for higher-
//!     order forward-mode AD. ParametricActivation::derivative is the
//!     primal+1st-derivative pair used by the gaze-saccade prediction +
//!     spectral-BRDF gradient-step paths.

use cssl_substrate_kan::SplineBasis;

/// § Maximum number of activation parameters carried per cell. Sized for
///   the BSplineEdge variant which uses 16 control points. Smaller variants
///   (Sigmoid, Tanh) leave the unused slots zero-filled.
pub const ACTIVATION_PARAM_MAX: usize = 16;

/// § Discriminator for the per-cell activation function. The parameter
///   semantics depend on the kind (see module docs).
///
/// § STABILITY
///   The discriminant values are FROZEN — used as the byte-stable tag in
///   the LoaKanOverlay storage. Reordering = ABI break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ActivationKind {
    /// Identity : `f(x) = x`. The default for unclaimed cells.
    #[default]
    Identity = 0,
    /// Sigmoid : `f(x) = 1 / (1 + exp(-(gain * x + bias)))`.
    Sigmoid = 1,
    /// Tanh : `f(x) = tanh(gain * x + bias)`.
    Tanh = 2,
    /// Gaussian : `f(x) = amplitude * exp(-((x - mean)^2) / (2 * sigma^2))`.
    Gaussian = 3,
    /// Radial basis : 2-D RBF, `f(x, y) = amplitude * exp(-((x-cx)^2 + (y-cy)^2) / (2 * sigma^2))`.
    /// For 1-D inputs, `y` is taken from the genome-embedding's first axis.
    RadialBasis = 4,
    /// Polynomial : `f(x) = Σ a_i * x^i` for i in 0..8.
    Polynomial = 5,
    /// B-spline edge function (KAN-canonical). 16 control points + global
    /// knot grid from the parent network.
    BSplineEdge = 6,
}

impl ActivationKind {
    /// § All variants in canonical order.
    #[must_use]
    pub const fn all() -> [ActivationKind; 7] {
        [
            Self::Identity,
            Self::Sigmoid,
            Self::Tanh,
            Self::Gaussian,
            Self::RadialBasis,
            Self::Polynomial,
            Self::BSplineEdge,
        ]
    }

    /// § Stable canonical name for telemetry + audit.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Sigmoid => "sigmoid",
            Self::Tanh => "tanh",
            Self::Gaussian => "gaussian",
            Self::RadialBasis => "radial_basis",
            Self::Polynomial => "polynomial",
            Self::BSplineEdge => "bspline_edge",
        }
    }

    /// § Number of meaningful parameters for this kind.
    #[must_use]
    pub const fn param_count(self) -> usize {
        match self {
            Self::Identity => 0,
            Self::Sigmoid => 2,
            Self::Tanh => 2,
            Self::Gaussian => 3,
            Self::RadialBasis => 4,
            Self::Polynomial => 8,
            Self::BSplineEdge => 16,
        }
    }

    /// § Decode from a u8 ; unknown discriminants clamp to Identity.
    #[must_use]
    pub const fn from_u8(v: u8) -> ActivationKind {
        match v {
            0 => Self::Identity,
            1 => Self::Sigmoid,
            2 => Self::Tanh,
            3 => Self::Gaussian,
            4 => Self::RadialBasis,
            5 => Self::Polynomial,
            6 => Self::BSplineEdge,
            _ => Self::Identity,
        }
    }

    /// § Pack to u8.
    #[must_use]
    pub const fn to_u8(self) -> u8 {
        self as u8
    }
}

/// § Per-cell parametric activation : the kind-tag + a fixed-size parameter
///   buffer. Total size = 1B kind + 7B pad + 64B params = 72B (std430-aligned
///   on the parent overlay's u64-stride storage).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParametricActivation {
    /// § Kind-tag — discriminates parameter-buffer semantics.
    pub kind: ActivationKind,
    /// § Spline basis for BSplineEdge variants. Ignored for non-spline kinds.
    pub spline_basis: SplineBasis,
    /// § Fixed-size parameter buffer. Only the first `kind.param_count()`
    ///   entries are meaningful ; remainder MUST be zero-filled.
    pub params: [f32; ACTIVATION_PARAM_MAX],
}

impl ParametricActivation {
    /// § Construct an Identity activation (no parameters).
    #[must_use]
    pub const fn identity() -> ParametricActivation {
        ParametricActivation {
            kind: ActivationKind::Identity,
            spline_basis: SplineBasis::BSpline,
            params: [0.0; ACTIVATION_PARAM_MAX],
        }
    }

    /// § Construct a Sigmoid activation.
    #[must_use]
    pub fn sigmoid(gain: f32, bias: f32) -> ParametricActivation {
        let mut p = [0.0_f32; ACTIVATION_PARAM_MAX];
        p[0] = gain;
        p[1] = bias;
        ParametricActivation {
            kind: ActivationKind::Sigmoid,
            spline_basis: SplineBasis::BSpline,
            params: p,
        }
    }

    /// § Construct a Tanh activation.
    #[must_use]
    pub fn tanh(gain: f32, bias: f32) -> ParametricActivation {
        let mut p = [0.0_f32; ACTIVATION_PARAM_MAX];
        p[0] = gain;
        p[1] = bias;
        ParametricActivation {
            kind: ActivationKind::Tanh,
            spline_basis: SplineBasis::BSpline,
            params: p,
        }
    }

    /// § Construct a Gaussian activation.
    #[must_use]
    pub fn gaussian(mean: f32, sigma: f32, amplitude: f32) -> ParametricActivation {
        let mut p = [0.0_f32; ACTIVATION_PARAM_MAX];
        p[0] = mean;
        p[1] = sigma;
        p[2] = amplitude;
        ParametricActivation {
            kind: ActivationKind::Gaussian,
            spline_basis: SplineBasis::BSpline,
            params: p,
        }
    }

    /// § Construct a Polynomial activation from coefficient slice.
    #[must_use]
    pub fn polynomial(coeffs: &[f32]) -> ParametricActivation {
        let mut p = [0.0_f32; ACTIVATION_PARAM_MAX];
        let n = coeffs.len().min(8);
        p[..n].copy_from_slice(&coeffs[..n]);
        ParametricActivation {
            kind: ActivationKind::Polynomial,
            spline_basis: SplineBasis::BSpline,
            params: p,
        }
    }

    /// § Construct a BSplineEdge activation from a control-point slice +
    ///   spline basis.
    #[must_use]
    pub fn bspline_edge(control_points: &[f32], basis: SplineBasis) -> ParametricActivation {
        let mut p = [0.0_f32; ACTIVATION_PARAM_MAX];
        let n = control_points.len().min(ACTIVATION_PARAM_MAX);
        p[..n].copy_from_slice(&control_points[..n]);
        ParametricActivation {
            kind: ActivationKind::BSplineEdge,
            spline_basis: basis,
            params: p,
        }
    }

    /// § Apply the activation to a scalar input. Uses the kind-tag to
    ///   dispatch to the correct evaluator.
    ///
    /// § DESIGN-NOTE : BSplineEdge eval uses a simple linear-interp between
    ///   control points (canonical KAN prototype eval) ; full spline-evaluator
    ///   lives in cssl-kan + composes with this surface via the parent
    ///   KanNetwork's knot_grid. The prototype path is sufficient for the
    ///   substrate-S12 milestone ; trained weights fall through to the
    ///   prototype eval until the training-loop lands.
    #[must_use]
    pub fn apply(&self, x: f32) -> f32 {
        match self.kind {
            ActivationKind::Identity => x,
            ActivationKind::Sigmoid => {
                let gain = self.params[0];
                let bias = self.params[1];
                let z = gain * x + bias;
                1.0 / (1.0 + (-z).exp())
            }
            ActivationKind::Tanh => {
                let gain = self.params[0];
                let bias = self.params[1];
                (gain * x + bias).tanh()
            }
            ActivationKind::Gaussian => {
                let mean = self.params[0];
                let sigma = self.params[1].max(1e-6);
                let amplitude = self.params[2];
                let dx = x - mean;
                amplitude * (-(dx * dx) / (2.0 * sigma * sigma)).exp()
            }
            ActivationKind::RadialBasis => {
                let cx = self.params[0];
                let cy = self.params[1];
                let sigma = self.params[2].max(1e-6);
                let amplitude = self.params[3];
                // For 1-D input, y = 0 (caller threads embedding-axis if 2-D).
                let dx = x - cx;
                let dy = -cy;
                amplitude * (-(dx * dx + dy * dy) / (2.0 * sigma * sigma)).exp()
            }
            ActivationKind::Polynomial => {
                // Σ params[i] * x^i for i in 0..8 (Horner's rule).
                let mut acc = 0.0_f32;
                for i in (0..8).rev() {
                    acc = acc * x + self.params[i];
                }
                acc
            }
            ActivationKind::BSplineEdge => {
                // § Prototype linear-interp between 16 control points on
                //   uniform knot grid x ∈ [0, 1]. Full spline evaluator
                //   lives in cssl-kan ; this is the fallback path.
                let n = ACTIVATION_PARAM_MAX;
                let xc = x.clamp(0.0, 1.0);
                let pos = xc * ((n - 1) as f32);
                let lo = pos.floor() as usize;
                let hi = (lo + 1).min(n - 1);
                let t = pos - (lo as f32);
                self.params[lo] * (1.0 - t) + self.params[hi] * t
            }
        }
    }

    /// § First-derivative of the activation at `x`. Used by the gaze-
    ///   saccade prediction + spectral-BRDF gradient-step paths (per
    ///   specs/33 § CASE-1).
    #[must_use]
    pub fn derivative(&self, x: f32) -> f32 {
        match self.kind {
            ActivationKind::Identity => 1.0,
            ActivationKind::Sigmoid => {
                // d/dx [σ(z)] = σ(z) * (1 - σ(z)) * gain
                let s = self.apply(x);
                let gain = self.params[0];
                gain * s * (1.0 - s)
            }
            ActivationKind::Tanh => {
                let gain = self.params[0];
                let bias = self.params[1];
                let z = (gain * x + bias).tanh();
                gain * (1.0 - z * z)
            }
            ActivationKind::Gaussian => {
                // d/dx [A exp(-((x-μ)/σ)²/2)] = -((x-μ)/σ²) * f(x)
                let mean = self.params[0];
                let sigma = self.params[1].max(1e-6);
                -(x - mean) / (sigma * sigma) * self.apply(x)
            }
            ActivationKind::RadialBasis => {
                let cx = self.params[0];
                let sigma = self.params[2].max(1e-6);
                -(x - cx) / (sigma * sigma) * self.apply(x)
            }
            ActivationKind::Polynomial => {
                // d/dx [Σ a_i x^i] = Σ i*a_i*x^(i-1)
                let mut acc = 0.0_f32;
                for i in (1..8).rev() {
                    acc = acc * x + (i as f32) * self.params[i];
                }
                acc
            }
            ActivationKind::BSplineEdge => {
                // Numerical finite-difference for prototype path.
                let h = 1e-4_f32;
                (self.apply(x + h) - self.apply(x - h)) / (2.0 * h)
            }
        }
    }

    /// § True iff the activation is Identity (no behavior-change applied).
    ///   Used by the overlay-emit path to avoid storing identity-default cells.
    #[must_use]
    pub const fn is_identity(&self) -> bool {
        matches!(self.kind, ActivationKind::Identity)
    }

    /// § Validate that unused-tail parameters are zero-filled per the
    ///   parameter-buffer discipline.
    #[must_use]
    pub fn unused_tail_zeroed(&self) -> bool {
        let n = self.kind.param_count();
        for i in n..ACTIVATION_PARAM_MAX {
            if self.params[i] != 0.0 {
                return false;
            }
        }
        true
    }
}

impl Default for ParametricActivation {
    fn default() -> Self {
        Self::identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Kind discriminant + canonical names ─────────────────────────

    #[test]
    fn activation_kind_all_count() {
        assert_eq!(ActivationKind::all().len(), 7);
    }

    #[test]
    fn activation_kind_canonical_names_unique() {
        let names: Vec<&'static str> = ActivationKind::all()
            .iter()
            .map(|k| k.canonical_name())
            .collect();
        let mut s = names.clone();
        s.sort_unstable();
        let original = s.len();
        s.dedup();
        assert_eq!(s.len(), original);
    }

    #[test]
    fn activation_kind_roundtrip_u8() {
        for &k in &ActivationKind::all() {
            let r = ActivationKind::from_u8(k.to_u8());
            assert_eq!(r, k);
        }
    }

    #[test]
    fn unknown_kind_clamps_to_identity() {
        assert_eq!(ActivationKind::from_u8(255), ActivationKind::Identity);
    }

    // ── Parameter counts ────────────────────────────────────────────

    #[test]
    fn kind_param_counts_match_spec() {
        assert_eq!(ActivationKind::Identity.param_count(), 0);
        assert_eq!(ActivationKind::Sigmoid.param_count(), 2);
        assert_eq!(ActivationKind::Tanh.param_count(), 2);
        assert_eq!(ActivationKind::Gaussian.param_count(), 3);
        assert_eq!(ActivationKind::RadialBasis.param_count(), 4);
        assert_eq!(ActivationKind::Polynomial.param_count(), 8);
        assert_eq!(ActivationKind::BSplineEdge.param_count(), 16);
    }

    // ── Activation shape (kind dispatch) ────────────────────────────

    #[test]
    fn identity_apply_passthrough() {
        let act = ParametricActivation::identity();
        assert_eq!(act.apply(2.5), 2.5);
        assert_eq!(act.apply(-1.0), -1.0);
        assert_eq!(act.apply(0.0), 0.0);
        assert!(act.is_identity());
    }

    #[test]
    fn sigmoid_apply_in_unit_range() {
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        assert!((act.apply(0.0) - 0.5).abs() < 1e-6);
        assert!(act.apply(10.0) > 0.99);
        assert!(act.apply(-10.0) < 0.01);
    }

    #[test]
    fn tanh_apply_in_signed_unit_range() {
        let act = ParametricActivation::tanh(1.0, 0.0);
        assert!((act.apply(0.0)).abs() < 1e-6);
        assert!(act.apply(10.0) > 0.99);
        assert!(act.apply(-10.0) < -0.99);
    }

    #[test]
    fn gaussian_peak_at_mean() {
        let act = ParametricActivation::gaussian(2.0, 0.5, 1.0);
        let at_peak = act.apply(2.0);
        let off_peak = act.apply(5.0);
        assert!(at_peak > off_peak);
        assert!((at_peak - 1.0).abs() < 1e-6);
    }

    #[test]
    fn polynomial_evaluates_horner_correctly() {
        // f(x) = 1 + 2x + 3x² ; f(2) = 1 + 4 + 12 = 17.
        let act = ParametricActivation::polynomial(&[1.0, 2.0, 3.0]);
        let v = act.apply(2.0);
        assert!((v - 17.0).abs() < 1e-5);
    }

    #[test]
    fn bspline_edge_interpolates_linearly() {
        let mut cps = [0.0_f32; ACTIVATION_PARAM_MAX];
        for i in 0..ACTIVATION_PARAM_MAX {
            cps[i] = i as f32;
        }
        let act = ParametricActivation::bspline_edge(&cps, SplineBasis::BSpline);
        // x=0 → cp[0]=0 ; x=1 → cp[15]=15 ; x=0.5 → ~7.5
        assert!((act.apply(0.0) - 0.0).abs() < 1e-3);
        assert!((act.apply(1.0) - 15.0).abs() < 1e-3);
        let mid = act.apply(0.5);
        assert!((mid - 7.5).abs() < 0.5);
    }

    // ── Derivatives ─────────────────────────────────────────────────

    #[test]
    fn identity_derivative_is_one() {
        let act = ParametricActivation::identity();
        assert_eq!(act.derivative(42.0), 1.0);
    }

    #[test]
    fn sigmoid_derivative_peak_at_zero() {
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let at_zero = act.derivative(0.0);
        let off_zero = act.derivative(5.0);
        // d/dx σ(0) = 0.25 (σ(0)*(1-σ(0))*1 = 0.5*0.5 = 0.25)
        assert!((at_zero - 0.25).abs() < 1e-3);
        assert!(at_zero > off_zero);
    }

    #[test]
    fn tanh_derivative_peak_at_zero() {
        let act = ParametricActivation::tanh(1.0, 0.0);
        let at_zero = act.derivative(0.0);
        let off_zero = act.derivative(2.0);
        // d/dx tanh(0) = 1.0
        assert!((at_zero - 1.0).abs() < 1e-3);
        assert!(at_zero > off_zero);
    }

    #[test]
    fn polynomial_derivative_correct() {
        // f(x) = 1 + 2x + 3x² ; f'(x) = 2 + 6x ; f'(2) = 14
        let act = ParametricActivation::polynomial(&[1.0, 2.0, 3.0]);
        let v = act.derivative(2.0);
        assert!((v - 14.0).abs() < 1e-3);
    }

    // ── Tail-zero discipline ────────────────────────────────────────

    #[test]
    fn sigmoid_tail_is_zero() {
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        assert!(act.unused_tail_zeroed());
    }

    #[test]
    fn polynomial_full_buffer_uses_all_eight() {
        let coeffs = [0.1_f32; 8];
        let act = ParametricActivation::polynomial(&coeffs);
        // Tail (8..16) must be zero.
        for i in 8..ACTIVATION_PARAM_MAX {
            assert_eq!(act.params[i], 0.0);
        }
        assert!(act.unused_tail_zeroed());
    }

    // ── Default ─────────────────────────────────────────────────────

    #[test]
    fn default_is_identity() {
        let act = ParametricActivation::default();
        assert!(act.is_identity());
    }

    #[test]
    fn identity_kind_default() {
        let k = ActivationKind::default();
        assert_eq!(k, ActivationKind::Identity);
    }
}
