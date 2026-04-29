//! Higher-order forward-mode automatic differentiation via [`Jet<T, N>`].
//!
//! § SPEC : `specs/17_JETS.csl` § TYPE-DEFINITION + § ARITHMETIC-RULES + §
//!          INTEGRATION-WITH-OTHER-FEATURES.
//!
//! § THESIS
//!   A jet is a Taylor-truncation viewed as a number-system. `Jet<T, N>` stores
//!   `N` consecutive terms of a Taylor-series expansion of a univariate
//!   function around a working point :
//!     terms[0] = primal  f(x)
//!     terms[1] = 1st-order  f'(x)
//!     terms[2] = 2nd-order  f''(x)
//!     …
//!     terms[N-1] = (N-1)th-order  f^(N-1)(x)
//!
//!   Arithmetic on jets propagates the chain-rule to all stored orders ; the
//!   transcendentals (sin / cos / exp / log / sqrt / pow) compose via the
//!   coefficient-table form of Faà di Bruno's formula. The result is that any
//!   differentiable expression evaluated on `Jet<T, N>` yields its Taylor
//!   expansion to order `N - 1` *in the value type itself* — no recursive
//!   transformation, no duplication of code, no rederivation by hand.
//!
//! § STORAGE-CONVENTION (stable-Rust without `feature(generic_const_exprs)`)
//!   The const-generic `N` here is the *storage size* (number of terms held),
//!   not the maximum order. Order-`k` jet ↔ `Jet<T, k + 1>`. The convenience
//!   constant [`Jet::ORDER`] reports `N.saturating_sub(1)`. The spec uses
//!   `Jet<T, N>` interchangeably with "N-th order jet" ; the off-by-one is a
//!   pragmatic concession to stable Rust : `[T; N]` is allowed, `[T; N + 1]`
//!   demands an unstable feature gate. This is the same trick Ceres-Solver
//!   uses internally (their `N` = "number of independent variables" = number
//!   of partial derivatives carried *in addition* to the primal — so their
//!   storage is also `N + 1` floats but typed as a struct with `N` partials
//!   plus one primal scalar). We chose the dual storage layout for arithmetic
//!   uniformity.
//!
//! § OPERATIONS
//!   - [`Jet::lift`]            : constant-jet (zero higher-order terms)
//!   - [`Jet::promote`]         : variable-jet (1st-order term = 1, others = 0)
//!   - [`Jet::primal`]          : extract `terms[0]`
//!   - [`Jet::nth_deriv`]       : extract `terms[k]`
//!   - [`Jet::project`]         : truncate to a smaller-order jet
//!   - [`Jet::compose`]         : Faà di Bruno general chain-rule
//!   - core arithmetic          : `+`, `-`, `*`, `/`, unary `-`, scalar ops
//!   - transcendentals          : `sin / cos / exp / log / sqrt / pow`
//!   - higher-order utilities   : [`Jet::hessian_vector_product`] (for
//!                                Hessian-vector-products in optimizer loops)
//!
//! § INTEGRATION-WITH-EXISTING-AD
//!   - The first-order forward-mode pass implemented by [`crate::apply_fwd`]
//!     is semantically equivalent to evaluating the function on `Jet<f32, 2>`
//!     (storage = 2, order = 1 : primal + 1st derivative).
//!   - The KAN-runtime (T11-D115) calls [`Jet::sin`] / [`Jet::cos`] / [`Jet::pow`]
//!     to obtain spline-coefficient 2nd-derivatives without manual derivation
//!     of the Bochner expansion.
//!   - Hyperspectral-KAN-BRDF (T11-D118) uses [`Jet::compose`] to chain
//!     spectral-basis functions with parameter-jets.
//!   - Inverse-rendering (T11-D116 + T11-D117) uses Hessian-vector-products
//!     for second-order optimizer steps via [`Jet::hessian_vector_product`].
//!
//! § REFERENCE
//!   Ceres-Solver `Jet<T, N>` (Google) — header-only C++ template ;
//!   <https://github.com/ceres-solver/ceres-solver/blob/master/include/ceres/jet.h>
//!   We follow Ceres' algebraic identities verbatim where they apply ; the
//!   chain-rule for higher-order transcendentals is computed via the
//!   coefficient-table form (no symbolic expansion).
//!
//! § FUTURE-WORK (parked here so MIR-level emission can pick them up later)
//!   - MIR-op `cssl.jet.construct<N>` / `cssl.jet.primal` / `cssl.jet.nth_deriv`
//!     emission alongside this runtime-typed implementation.
//!   - GPU-jet (Arc A770) : `Jet<f32, N>` packed in registers for `N ≤ 4` ;
//!     larger `N` spilled to shared-memory / SSBO per spec § GPU-JET.
//!   - Lazy `LazyJet<T>` : co-inductive stream for analytic functions per
//!     spec § LAZY (deferred — needs `{NoUnbounded}` effect-discipline).

#![allow(clippy::needless_range_loop)] // Jet uses idiomatic indexed loops.
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::many_single_char_names)] // f, g, k, n, j are textbook names.
#![allow(clippy::manual_memcpy)] // explicit-loop preserves intent in project().
#![allow(clippy::suspicious_arithmetic_impl)] // Div uses recip — intentional.
#![allow(clippy::float_cmp)] // Jet:: API exposes exact-equal in unit-tests.
#![allow(clippy::suboptimal_flops)] // textbook expressions, FMA not relevant.
#![allow(clippy::assertions_on_constants)]

use core::fmt;
use core::ops::{Add, Div, Mul, Neg, Sub};

// ─────────────────────────────────────────────────────────────────────────
// § Trait : `JetField` — algebraic operations a jet element-type must support.
// ─────────────────────────────────────────────────────────────────────────

/// The minimum algebraic vocabulary the [`Jet<T, N>`] type expects of `T`.
///
/// Any implementation provides : associative addition / subtraction /
/// multiplication / division by-value, an additive zero, a multiplicative one,
/// scalar-by-`f64` multiplication (for chain-rule coefficients : `1/2 * sqrt(x)`,
/// `cos(x)` factors of `n!`, etc.), and the standard transcendentals required
/// by the per-primitive rule table.
///
/// `f32` and `f64` implement the trait directly. Vector-types can be added
/// later via `JetField for vec<f32, K>` once the autodiff-vector-types land
/// (T11-D115 KAN-runtime work).
pub trait JetField:
    Copy
    + PartialEq
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + fmt::Debug
{
    /// Additive identity.
    fn zero() -> Self;
    /// Multiplicative identity.
    fn one() -> Self;
    /// Multiply by an `f64` scalar (for chain-rule coefficients).
    #[must_use]
    fn scale_f64(self, k: f64) -> Self;
    /// Reciprocal `1 / self`.
    #[must_use]
    fn recip(self) -> Self;
    /// Square root.
    #[must_use]
    fn jet_sqrt(self) -> Self;
    /// Sine (radians).
    #[must_use]
    fn jet_sin(self) -> Self;
    /// Cosine (radians).
    #[must_use]
    fn jet_cos(self) -> Self;
    /// Natural exponential.
    #[must_use]
    fn jet_exp(self) -> Self;
    /// Natural logarithm.
    #[must_use]
    fn jet_ln(self) -> Self;
    /// Generic real-power `self.powf(k)`.
    #[must_use]
    fn jet_powf(self, k: f64) -> Self;
    /// Convert the `f64` literal to the field-type. Used by the chain-rule
    /// table to mint coefficients (`0.5`, `n!`, `1 / n`, …).
    fn from_f64(x: f64) -> Self;
    /// Convert back to `f64` for diagnostics + scalar comparisons. The result
    /// must round-trip `from_f64` for representable values.
    fn to_f64(self) -> f64;
}

impl JetField for f32 {
    #[inline]
    fn zero() -> Self {
        0.0
    }
    #[inline]
    fn one() -> Self {
        1.0
    }
    #[inline]
    fn scale_f64(self, k: f64) -> Self {
        self * k as f32
    }
    #[inline]
    fn recip(self) -> Self {
        1.0 / self
    }
    #[inline]
    fn jet_sqrt(self) -> Self {
        self.sqrt()
    }
    #[inline]
    fn jet_sin(self) -> Self {
        self.sin()
    }
    #[inline]
    fn jet_cos(self) -> Self {
        self.cos()
    }
    #[inline]
    fn jet_exp(self) -> Self {
        self.exp()
    }
    #[inline]
    fn jet_ln(self) -> Self {
        self.ln()
    }
    #[inline]
    fn jet_powf(self, k: f64) -> Self {
        self.powf(k as f32)
    }
    #[inline]
    fn from_f64(x: f64) -> Self {
        x as f32
    }
    #[inline]
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
}

impl JetField for f64 {
    #[inline]
    fn zero() -> Self {
        0.0
    }
    #[inline]
    fn one() -> Self {
        1.0
    }
    #[inline]
    fn scale_f64(self, k: f64) -> Self {
        self * k
    }
    #[inline]
    fn recip(self) -> Self {
        1.0 / self
    }
    #[inline]
    fn jet_sqrt(self) -> Self {
        self.sqrt()
    }
    #[inline]
    fn jet_sin(self) -> Self {
        self.sin()
    }
    #[inline]
    fn jet_cos(self) -> Self {
        self.cos()
    }
    #[inline]
    fn jet_exp(self) -> Self {
        self.exp()
    }
    #[inline]
    fn jet_ln(self) -> Self {
        self.ln()
    }
    #[inline]
    fn jet_powf(self, k: f64) -> Self {
        self.powf(k)
    }
    #[inline]
    fn from_f64(x: f64) -> Self {
        x
    }
    #[inline]
    fn to_f64(self) -> f64 {
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § The Jet type.
// ─────────────────────────────────────────────────────────────────────────

/// Maximum supported jet storage-size (= maximum order + 1).
///
/// The per-order-`k` chain-rule for transcendentals scales as `O(k²)` due to
/// Faà-di-Bruno coefficient enumeration ; orders 1–8 are all that real GPU /
/// KAN-runtime workloads need. We cap at 16 storage-slots (= order-15) for
/// the public surface so incidental misuse won't accidentally allocate
/// 1024-entry coefficient tables on the stack.
pub const MAX_JET_ORDER_PLUS_ONE: usize = 16;

/// `Jet<T, N>` : a Taylor-series truncation with `N` stored terms.
///
/// `terms[k]` is the `k`-th *Taylor coefficient* of the expansion :
/// `terms[k] = f^(k)(x) / k!`. This matches Ceres-Solver's internal storage
/// convention and makes the multiplicative-product law plain Cauchy
/// convolution (no binomial factor needed). User-facing accessors :
///
///   - [`Jet::primal`]    — returns `terms[0]` = `f(x)`.
///   - [`Jet::nth_deriv`] — returns `terms[k] · k!` = `f^(k)(x)` (the raw
///                          derivative, what most user-code wants to inspect).
///   - [`Jet::nth_taylor_coeff`] — returns `terms[k]` directly (for users
///                                 who already think in Taylor-coefficient
///                                 space, e.g. inverse-rendering optimizer
///                                 implementers + KAN-spline coefficient
///                                 readers).
#[derive(Clone, Copy, PartialEq)]
pub struct Jet<T: JetField, const N: usize> {
    /// `terms[0]` = primal `f(x)`,
    /// `terms[k]` = `k`-th Taylor coefficient `f^(k)(x) / k!` for `k ≥ 1`.
    pub terms: [T; N],
}

impl<T: JetField, const N: usize> fmt::Debug for Jet<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Jet")
            .field("order", &Self::ORDER)
            .field("terms", &self.terms)
            .finish()
    }
}

impl<T: JetField, const N: usize> Jet<T, N> {
    /// Maximum derivative-order representable : `N - 1` (saturates at 0 for
    /// `N == 0` or `N == 1`).
    pub const ORDER: usize = if N == 0 { 0 } else { N - 1 };

    /// Number of stored terms (= primal + every retained derivative).
    pub const STORAGE: usize = N;

    /// Construct a jet from explicit term-array.
    #[must_use]
    pub const fn new(terms: [T; N]) -> Self {
        Self { terms }
    }

    /// `Jet::lift(c)` — constant jet : `terms[0] = c`, every other term = 0.
    /// Use for *parameters that are not being differentiated against*.
    #[must_use]
    pub fn lift(c: T) -> Self {
        let mut terms = [T::zero(); N];
        if N > 0 {
            terms[0] = c;
        }
        Self { terms }
    }

    /// `Jet::promote(x)` — variable jet : `terms[0] = x`, `terms[1] = 1`,
    /// every higher-order term = 0. Use for *the input variable being
    /// differentiated against*. Note that `terms[1]` storing `1` corresponds
    /// to `f'(x) / 1! = 1` ⇒ `f(u) = u` ⇒ `f'(u) = 1` (identity-fn
    /// derivative).
    ///
    /// Equivalent to "promote `x` to be the active variable of the jet
    /// computation" — the higher-order terms emerge naturally from the
    /// chain-rule applied during arithmetic.
    #[must_use]
    pub fn promote(x: T) -> Self {
        let mut terms = [T::zero(); N];
        if N > 0 {
            terms[0] = x;
        }
        if N > 1 {
            terms[1] = T::one();
        }
        Self { terms }
    }

    /// Alias for [`Self::promote`] used by spec-side prose ("promote the
    /// active variable to a jet").
    #[must_use]
    pub fn promote_active(x: T) -> Self {
        Self::promote(x)
    }

    /// Zero jet : all terms 0.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            terms: [T::zero(); N],
        }
    }

    /// One jet : `terms[0] = 1`, all higher 0.
    #[must_use]
    pub fn one() -> Self {
        let mut t = [T::zero(); N];
        if N > 0 {
            t[0] = T::one();
        }
        Self { terms: t }
    }

    /// Extract the primal value.
    #[must_use]
    pub fn primal(self) -> T {
        if N == 0 {
            T::zero()
        } else {
            self.terms[0]
        }
    }

    /// Extract the `k`-th derivative `f^(k)(x)`. Internally stored as
    /// `terms[k] · k!` (Taylor-coefficient convention), so this multiplies
    /// by `k!` before returning. Returns zero for `k >= N`.
    #[must_use]
    pub fn nth_deriv(self, k: usize) -> T {
        if k < N {
            self.terms[k].scale_f64(Self::factorial(k))
        } else {
            T::zero()
        }
    }

    /// Extract the `k`-th raw Taylor coefficient `f^(k)(x) / k!` (the
    /// stored value). Use this when working in coefficient-space directly
    /// (e.g. inverse-rendering loss-functions, KAN-spline coefficients
    /// already expressed in Taylor form). Returns zero for `k >= N`.
    #[must_use]
    pub fn nth_taylor_coeff(self, k: usize) -> T {
        if k < N {
            self.terms[k]
        } else {
            T::zero()
        }
    }

    /// Truncate this jet to a smaller storage-size `M`. Terms beyond `M` are
    /// dropped. Terms that don't exist in the source (for `M > N`) are zero.
    #[must_use]
    pub fn project<const M: usize>(self) -> Jet<T, M> {
        let mut t = [T::zero(); M];
        let copy_len = if N < M { N } else { M };
        for i in 0..copy_len {
            t[i] = self.terms[i];
        }
        Jet { terms: t }
    }

    /// Element-wise-equal to within `tol` on every term (for tests).
    #[must_use]
    pub fn approx_eq(self, other: Self, tol: f64) -> bool {
        for i in 0..N {
            let a = self.terms[i].to_f64();
            let b = other.terms[i].to_f64();
            if (a - b).abs() > tol {
                return false;
            }
        }
        true
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Helpers : binomial coefficients + factorial — pre-tabulated up to
    // MAX_JET_ORDER_PLUS_ONE for chain-rule expansion.
    // ─────────────────────────────────────────────────────────────────────

    /// Binomial coefficient `C(n, k)` as `f64` (saturates to 0 for `k > n`).
    #[must_use]
    pub fn binom(n: usize, k: usize) -> f64 {
        if k > n {
            return 0.0;
        }
        let k = k.min(n - k);
        let mut acc: f64 = 1.0;
        for i in 0..k {
            acc = acc * ((n - i) as f64) / ((i + 1) as f64);
        }
        acc
    }

    /// Factorial as `f64` (saturates to `f64::INFINITY` past `n = 170`).
    #[must_use]
    pub fn factorial(n: usize) -> f64 {
        let mut acc: f64 = 1.0;
        for i in 2..=n {
            acc *= i as f64;
        }
        acc
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Arithmetic : Jet + Jet ; Jet - Jet ; Jet * Jet ; Jet / Jet ; -Jet.
// ─────────────────────────────────────────────────────────────────────────

impl<T: JetField, const N: usize> Add for Jet<T, N> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let mut t = [T::zero(); N];
        for i in 0..N {
            t[i] = self.terms[i] + rhs.terms[i];
        }
        Self { terms: t }
    }
}

impl<T: JetField, const N: usize> Sub for Jet<T, N> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        let mut t = [T::zero(); N];
        for i in 0..N {
            t[i] = self.terms[i] - rhs.terms[i];
        }
        Self { terms: t }
    }
}

impl<T: JetField, const N: usize> Neg for Jet<T, N> {
    type Output = Self;
    fn neg(self) -> Self {
        let mut t = [T::zero(); N];
        for i in 0..N {
            t[i] = -self.terms[i];
        }
        Self { terms: t }
    }
}

impl<T: JetField, const N: usize> Mul for Jet<T, N> {
    type Output = Self;
    /// Cauchy / convolution product of Taylor-coefficient arrays.
    ///
    /// In Taylor-coefficient space `terms[k] = f^(k)(x) / k!`, the product
    /// `(f · g)` satisfies plain convolution :
    /// ```text
    ///   (fg)_k  =  Σ_{j = 0}^{k}  f_j · g_{k - j}
    /// ```
    /// — the binomial factor that appears in the *raw-derivative* Leibniz
    /// identity is absorbed into the factorial divisors of each Taylor
    /// coefficient, leaving a plain convolution. This is the form used
    /// internally by Ceres-Solver + every standard Jet implementation.
    fn mul(self, rhs: Self) -> Self {
        let mut t = [T::zero(); N];
        for k in 0..N {
            let mut acc = T::zero();
            for j in 0..=k {
                acc = acc + self.terms[j] * rhs.terms[k - j];
            }
            t[k] = acc;
        }
        Self { terms: t }
    }
}

impl<T: JetField, const N: usize> Div for Jet<T, N> {
    type Output = Self;
    /// Series-division : compute `1 / rhs` via [`Jet::recip`] then multiply.
    /// Closed-form chain-rule for division would also work but goes via the
    /// reciprocal as the most numerically-stable path.
    fn div(self, rhs: Self) -> Self {
        self * rhs.recip()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Scalar mixing : Jet ± T, Jet * T, T * Jet, etc.
// ─────────────────────────────────────────────────────────────────────────

impl<T: JetField, const N: usize> Jet<T, N> {
    /// Add a scalar (only affects the primal).
    #[must_use]
    pub fn add_scalar(self, s: T) -> Self {
        let mut out = self;
        if N > 0 {
            out.terms[0] = out.terms[0] + s;
        }
        out
    }

    /// Multiply every term by a scalar (chain-rule for affine reweighting).
    #[must_use]
    pub fn scale(self, s: T) -> Self {
        let mut out = self;
        for i in 0..N {
            out.terms[i] = out.terms[i] * s;
        }
        out
    }

    /// Reciprocal of a jet : `g(x) = 1 / f(x)`.
    /// In Taylor-coefficient space `f · g = 1` ⇒ `Σ_{j=0}^{k} f_j g_{k-j} = δ_{k,0}`,
    /// which gives the recurrence :
    ///   g_0 = 1 / f_0
    ///   g_k = -(1 / f_0) · Σ_{j=1}^{k} f_j · g_{k - j}    (k ≥ 1)
    /// — a plain Cauchy-convolution division. No binomial factor is needed
    /// because both operands are stored in Taylor-coefficient form.
    #[must_use]
    pub fn recip(self) -> Self {
        if N == 0 {
            return Self::zero();
        }
        let f0_inv = self.terms[0].recip();
        let mut g = [T::zero(); N];
        g[0] = f0_inv;
        for k in 1..N {
            let mut acc = T::zero();
            for j in 1..=k {
                acc = acc + self.terms[j] * g[k - j];
            }
            g[k] = (T::zero() - acc) * f0_inv;
        }
        Self { terms: g }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Transcendentals : Faà di Bruno via per-primitive analytic recursion.
// ─────────────────────────────────────────────────────────────────────────
//
// For every univariate analytic function `phi(u)`, given a jet
// `f = (f_0, f_1, …, f_{N-1})`, the composition `g = phi(f)` has
// derivatives that satisfy the recurrence form of Faà di Bruno's formula :
//
//   g_n  =  Σ_{k=1}^{n}  phi^(k)(f_0) · B_{n,k}(f_1, …, f_{n-k+1})
//
// where `B_{n,k}` are the partial Bell polynomials. For each transcendental
// we use a closed-form recurrence specific to that function — these are
// numerically stable and avoid the combinatorial explosion of general
// Bell-polynomial enumeration.

impl<T: JetField, const N: usize> Jet<T, N> {
    /// Natural-log of a jet using the standard Taylor-coefficient recurrence
    ///   g_0 = ln(f_0)
    ///   g_n = (1 / f_0) · ( f_n − (1 / n) · Σ_{k=1}^{n-1} k · f_{n-k} · g_k )
    /// derived from `f · g' = f'` followed by Cauchy-product identification
    /// with `f' Taylor coeff #(n-1) = n · f_n` (shift-rule for derivative
    /// in coefficient-space). See Knuth TAOCP Vol 2 § 4.7 ; or Brent & Kung
    /// "Fast Algorithms for Manipulating Formal Power Series" (1978) for the
    /// derivation of this exact form.
    #[must_use]
    pub fn ln(self) -> Self {
        if N == 0 {
            return Self::zero();
        }
        let f0_inv = self.terms[0].recip();
        let mut g = [T::zero(); N];
        g[0] = self.terms[0].jet_ln();
        for n in 1..N {
            let mut acc = self.terms[n];
            for k in 1..n {
                let coeff: f64 = k as f64 / n as f64;
                acc = acc - self.terms[n - k].scale_f64(coeff) * g[k];
            }
            g[n] = acc * f0_inv;
        }
        Self { terms: g }
    }

    /// Exponential of a jet using the recurrence
    ///   g_0 = exp(f_0)
    ///   g_n = (1 / n) · Σ_{k=1}^{n} k · f_k · g_{n - k}
    /// derived from `g'(x) = f'(x) · g(x)` followed by the Leibniz identity.
    #[must_use]
    pub fn exp(self) -> Self {
        if N == 0 {
            return Self::zero();
        }
        let mut g = [T::zero(); N];
        g[0] = self.terms[0].jet_exp();
        for n in 1..N {
            let mut acc = T::zero();
            for k in 1..=n {
                let w: f64 = k as f64 / n as f64;
                acc = acc + self.terms[k].scale_f64(w) * g[n - k];
            }
            g[n] = acc;
        }
        Self { terms: g }
    }

    /// Sine of a jet : computed jointly with [`Jet::cos`] via the coupled
    /// recurrence below. We just wrap [`Self::sin_cos`] and discard the
    /// cosine output.
    #[must_use]
    pub fn sin(self) -> Self {
        self.sin_cos().0
    }

    /// Cosine of a jet — see [`Self::sin_cos`].
    #[must_use]
    pub fn cos(self) -> Self {
        self.sin_cos().1
    }

    /// Sine-and-cosine pair, using the well-known coupled recurrence
    ///   s_0 = sin(f_0), c_0 = cos(f_0)
    ///   s_n = (1/n) · Σ_{k=1}^{n} k · f_k · c_{n-k}
    ///   c_n = (1/n) · Σ_{k=1}^{n} k · f_k · (-s_{n-k})
    /// derived from `(sin f)' = cos(f) · f'` and `(cos f)' = -sin(f) · f'`
    /// followed by Leibniz.
    #[must_use]
    pub fn sin_cos(self) -> (Self, Self) {
        if N == 0 {
            return (Self::zero(), Self::zero());
        }
        let mut s = [T::zero(); N];
        let mut c = [T::zero(); N];
        s[0] = self.terms[0].jet_sin();
        c[0] = self.terms[0].jet_cos();
        for n in 1..N {
            let mut s_acc = T::zero();
            let mut c_acc = T::zero();
            for k in 1..=n {
                let w: f64 = k as f64 / n as f64;
                let scaled_fk = self.terms[k].scale_f64(w);
                s_acc = s_acc + scaled_fk * c[n - k];
                c_acc = c_acc - scaled_fk * s[n - k];
            }
            s[n] = s_acc;
            c[n] = c_acc;
        }
        (Self { terms: s }, Self { terms: c })
    }

    /// Square-root of a jet using the recurrence
    ///   g_0 = sqrt(f_0)
    ///   g_n = (1 / (2 g_0)) · ( f_n - Σ_{k=1}^{n-1} g_k · g_{n - k} )
    /// derived from `g(x) = sqrt(f(x)) ⇒ 2 g(x) g'(x) = f'(x) ⇒ Leibniz`.
    #[must_use]
    pub fn sqrt(self) -> Self {
        if N == 0 {
            return Self::zero();
        }
        let mut g = [T::zero(); N];
        g[0] = self.terms[0].jet_sqrt();
        let two_g0_inv = g[0].scale_f64(2.0).recip();
        for n in 1..N {
            let mut acc = self.terms[n];
            for k in 1..n {
                acc = acc - g[k] * g[n - k];
            }
            g[n] = acc * two_g0_inv;
        }
        Self { terms: g }
    }

    /// Generic real-power : `self ^ k`. Implemented as `exp(k * ln(self))`
    /// for stability across `k ∈ ℝ`. For integer `k` this matches the
    /// closed-form direct-power recurrence ; for fractional `k` it
    /// generalizes correctly because Faà-di-Bruno coefficients of `ln`
    /// composed with `exp` reproduce the binomial-series.
    #[must_use]
    pub fn powf(self, k: f64) -> Self {
        if N == 0 {
            return Self::zero();
        }
        // Optimization : k = 1 is identity ; k = 0 is one ; k = 2 is square.
        if k == 0.0 {
            return Self::one();
        }
        if k == 1.0 {
            return self;
        }
        if k == 2.0 {
            return self * self;
        }
        // Generic path : exp(k · ln self).
        let log = self.ln();
        let scaled = log.scale(T::from_f64(k));
        scaled.exp()
    }

    /// Integer-power `self ^ p` via repeated multiplication (exact, no
    /// log/exp). For `p < 0` it inverts via [`Self::recip`] then squares.
    #[must_use]
    pub fn powi(self, p: i32) -> Self {
        if p == 0 {
            return Self::one();
        }
        let (base, mut p) = if p < 0 { (self.recip(), -p) } else { (self, p) };
        let mut result = Self::one();
        let mut sq = base;
        while p > 0 {
            if p & 1 != 0 {
                result = result * sq;
            }
            p >>= 1;
            if p > 0 {
                sq = sq * sq;
            }
        }
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Composition : Faà di Bruno via the explicit jet-of-jet trick.
// ─────────────────────────────────────────────────────────────────────────
//
// Given `g : T → T` evaluated as a closure on jets, and an inner jet `f`,
// the composition `(g ∘ f)` has derivatives that follow Faà di Bruno's
// formula. Rather than enumerate Bell polynomials, we lean on the algebra
// already implemented : if a callable `phi : Jet<T, N> → Jet<T, N>`
// represents `g` *acting on jets*, then `phi(f)` is exactly the answer.
// This is the standard way Ceres + Slang compose Jet-typed expressions :
// the callable IS the composition operator. We expose [`Jet::compose`] as
// a thin wrapper for code that wants explicit composition syntax.

impl<T: JetField, const N: usize> Jet<T, N> {
    /// `Jet::compose(phi, f)` ≡ `phi(f)`.
    ///
    /// `phi` is expected to be a jet-aware closure (i.e., one that already
    /// implements the chain-rule for whatever ground operation it represents).
    /// In practice user-code calls jet-arithmetic methods directly — this
    /// function exists for documentation + parity with the spec-prose.
    #[must_use]
    pub fn compose<F>(phi: F, f: Self) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        phi(f)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Hessian-vector-product : per spec § HESSIAN-VECTOR-PRODUCT (lines 72–81).
// ─────────────────────────────────────────────────────────────────────────

/// Compute the second-order *directional derivative* `f''(x) · v²` of a
/// univariate `f : R → R` at the working point `x` along the seed `v`,
/// using a `Jet<T, 3>` internally (storage = 3, order = 2). Per spec §
/// HESSIAN-VECTOR-PRODUCT (lines 72–81), this is the universal building
/// block for Hessian-vector products in optimizer loops :
///
/// - For *univariate* `f : R → R` : returns `f''(x) · v²`. Pass `v = 1` to
///   get the bare second derivative `f''(x)`.
/// - For *multivariate* `f : R^n → R` (when this function is generalized
///   over vector-valued jets, T11-D115 follow-up) : returns the bilinear
///   form `v^T · H · v`. The full `H · v` vector is obtained by `n`
///   separate calls with directional seeds `v = e_i` per coordinate, or
///   by composing forward-over-reverse mode (one reverse-pass + one
///   forward-pass) — see `cssl-stdlib::math::optim::quasi_newton`.
///
/// The Taylor-coefficient storage convention internally divides by `2!`
/// when stored ; [`Jet::nth_deriv`] multiplies it back, so the returned
/// value is in raw-derivative units regardless of internal convention.
///
/// § COMPLEXITY : O(N²) at `Jet<T, 3>` ⇒ O(9) flops + transcendental cost.
#[must_use]
pub fn hessian_vector_product_1d<T: JetField, F>(f: F, x: T, v: T) -> T
where
    F: Fn(Jet<T, 3>) -> Jet<T, 3>,
{
    let mut t = [T::zero(); 3];
    t[0] = x;
    t[1] = v;
    let j = Jet::<T, 3> { terms: t };
    let r = f(j);
    r.nth_deriv(2)
}

/// Alias of [`hessian_vector_product_1d`] used in optimizer-loop code that
/// reads as "evaluate the i-th coordinate's directional 2nd derivative
/// along seed `v_i`". For full `n`-dimensional HVP, dispatch this `n`
/// times. Used by `cssl-stdlib::math::optim` second-order methods.
#[must_use]
pub fn hvp_axis<T: JetField, F>(f: F, x: T, v: T) -> T
where
    F: Fn(Jet<T, 3>) -> Jet<T, 3>,
{
    hessian_vector_product_1d(f, x, v)
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — 50+ tests covering arithmetic, transcendentals, composition,
// KAN-spline 2nd-derivative, and HVP correctness.
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{hessian_vector_product_1d, hvp_axis, Jet, JetField, MAX_JET_ORDER_PLUS_ONE};

    const EPS_F32: f64 = 1e-5;
    const EPS: f64 = 1e-10;

    // ── Constants + smoke tests ──────────────────────────────────────────

    #[test]
    fn order_alias_equals_n_minus_one() {
        assert_eq!(Jet::<f64, 1>::ORDER, 0);
        assert_eq!(Jet::<f64, 2>::ORDER, 1);
        assert_eq!(Jet::<f64, 3>::ORDER, 2);
        assert_eq!(Jet::<f64, 4>::ORDER, 3);
    }

    #[test]
    fn storage_alias_equals_n() {
        assert_eq!(Jet::<f64, 1>::STORAGE, 1);
        assert_eq!(Jet::<f64, 8>::STORAGE, 8);
    }

    #[test]
    fn max_order_const_is_accessible() {
        assert!(MAX_JET_ORDER_PLUS_ONE >= 16);
    }

    #[test]
    fn jetfield_f64_round_trips() {
        let x = 3.5_f64;
        assert_eq!(JetField::to_f64(x), x);
        let y = f64::from_f64(2.71);
        assert_eq!(y, 2.71_f64);
    }

    #[test]
    fn jetfield_f32_round_trips() {
        let x = 3.5_f32;
        assert!(((JetField::to_f64(x)) - 3.5_f64).abs() < 1e-6);
    }

    // ── Construction ─────────────────────────────────────────────────────

    #[test]
    fn new_constructs_jet_with_explicit_terms() {
        // Taylor-coefficient convention : terms[k] = f^(k)(x) / k!
        // So [1.0, 2.0, 3.0] means primal=1, f'=2, f''=6 (=3·2!).
        let j: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0]);
        assert_eq!(j.primal(), 1.0);
        assert_eq!(j.nth_deriv(1), 2.0);
        assert_eq!(j.nth_deriv(2), 6.0); // 3.0 · 2! = 6.0
        assert_eq!(j.nth_taylor_coeff(2), 3.0); // raw stored value
    }

    #[test]
    fn lift_zeros_higher_order_terms() {
        let j: Jet<f64, 4> = Jet::lift(7.5);
        assert_eq!(j.primal(), 7.5);
        for k in 1..4 {
            assert_eq!(j.nth_deriv(k), 0.0);
        }
    }

    #[test]
    fn promote_sets_first_derivative_to_one() {
        let j: Jet<f64, 4> = Jet::promote(3.0);
        assert_eq!(j.primal(), 3.0);
        assert_eq!(j.nth_deriv(1), 1.0);
        assert_eq!(j.nth_deriv(2), 0.0);
        assert_eq!(j.nth_deriv(3), 0.0);
    }

    #[test]
    fn promote_active_alias_matches_promote() {
        let a: Jet<f64, 3> = Jet::promote(2.5);
        let b: Jet<f64, 3> = Jet::promote_active(2.5);
        assert!(a.approx_eq(b, 0.0));
    }

    #[test]
    fn one_jet_matches_lift_one() {
        let a: Jet<f64, 4> = Jet::one();
        let b: Jet<f64, 4> = Jet::lift(1.0);
        assert!(a.approx_eq(b, 0.0));
    }

    #[test]
    fn zero_jet_is_zero_everywhere() {
        let j: Jet<f64, 5> = Jet::zero();
        for k in 0..5 {
            assert_eq!(j.nth_deriv(k), 0.0);
        }
    }

    #[test]
    fn nth_deriv_out_of_range_returns_zero() {
        let j: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0]);
        assert_eq!(j.nth_deriv(99), 0.0);
    }

    // ── Project (truncation + zero-pad) ──────────────────────────────────

    #[test]
    fn project_truncates_higher_terms() {
        let j: Jet<f64, 5> = Jet::new([1.0, 2.0, 3.0, 4.0, 5.0]);
        let p: Jet<f64, 3> = j.project();
        assert_eq!(p.terms, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn project_zero_pads_when_growing() {
        let j: Jet<f64, 2> = Jet::new([1.0, 2.0]);
        let p: Jet<f64, 5> = j.project();
        assert_eq!(p.terms, [1.0, 2.0, 0.0, 0.0, 0.0]);
    }

    // ── Arithmetic : add / sub / mul / div ───────────────────────────────

    #[test]
    fn jet_addition_componentwise() {
        let a: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0]);
        let b: Jet<f64, 3> = Jet::new([10.0, 20.0, 30.0]);
        let c = a + b;
        assert_eq!(c.terms, [11.0, 22.0, 33.0]);
    }

    #[test]
    fn jet_subtraction_componentwise() {
        let a: Jet<f64, 3> = Jet::new([10.0, 20.0, 30.0]);
        let b: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0]);
        let c = a - b;
        assert_eq!(c.terms, [9.0, 18.0, 27.0]);
    }

    #[test]
    fn jet_negation_flips_sign_of_every_term() {
        let a: Jet<f64, 4> = Jet::new([1.0, -2.0, 3.0, -4.0]);
        let b = -a;
        assert_eq!(b.terms, [-1.0, 2.0, -3.0, 4.0]);
    }

    #[test]
    fn jet_multiplication_first_order_matches_product_rule() {
        // f(x) = x · x · x at x=2 ⇒ 8 ; f'=12 ; f''=12 ; f'''=6.
        let x = 2.0_f64;
        let xj: Jet<f64, 4> = Jet::promote(x);
        let p = xj * xj * xj;
        assert!((p.primal() - 8.0).abs() < EPS);
        assert!((p.nth_deriv(1) - 12.0).abs() < EPS);
        assert!((p.nth_deriv(2) - 12.0).abs() < EPS);
        assert!((p.nth_deriv(3) - 6.0).abs() < EPS);
    }

    #[test]
    fn jet_multiplication_constant_pulls_through() {
        let a: Jet<f64, 3> = Jet::new([3.0, 1.0, 0.0]);
        let c: Jet<f64, 3> = Jet::lift(5.0);
        let r = a * c;
        // Multiplying by a constant scales every term by 5 and leaves the
        // higher-order *constant terms* zero.
        assert!(r.approx_eq(Jet::new([15.0, 5.0, 0.0]), EPS));
    }

    #[test]
    fn jet_recip_first_order_correct_at_x_eq_2() {
        // f(x) = x, recip(x) = 1/x ⇒ d/dx = -1/x² = -0.25 at x=2.
        // 2nd deriv = 2/x³ = 0.25 at x=2.
        let f: Jet<f64, 3> = Jet::promote(2.0);
        let r = f.recip();
        assert!((r.primal() - 0.5).abs() < EPS);
        assert!((r.nth_deriv(1) - (-0.25)).abs() < EPS);
        assert!((r.nth_deriv(2) - 0.25).abs() < EPS);
    }

    #[test]
    fn jet_division_quotient_rule_at_x_eq_3() {
        // f(x)/g(x) where f = x², g = x ⇒ result = x ⇒ d/dx = 1, d²/dx² = 0.
        let x = 3.0_f64;
        let xj: Jet<f64, 3> = Jet::promote(x);
        let f = xj * xj;
        let g = xj;
        let q = f / g;
        assert!((q.primal() - 3.0).abs() < EPS);
        assert!((q.nth_deriv(1) - 1.0).abs() < EPS);
        assert!(q.nth_deriv(2).abs() < EPS);
    }

    #[test]
    fn add_scalar_only_changes_primal() {
        let a: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0]);
        let b = a.add_scalar(10.0);
        assert_eq!(b.terms, [11.0, 2.0, 3.0]);
    }

    #[test]
    fn scale_multiplies_all_terms() {
        let a: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0]);
        let b = a.scale(4.0);
        assert_eq!(b.terms, [4.0, 8.0, 12.0]);
    }

    // ── Transcendentals : exp / ln / sin / cos / sqrt / pow ──────────────

    #[test]
    fn exp_first_order_matches_analytic_at_x_eq_0() {
        // d/dx exp(x) = exp(x), exp(0) = 1.
        let f: Jet<f64, 3> = Jet::promote(0.0);
        let e = f.exp();
        assert!((e.primal() - 1.0).abs() < EPS);
        assert!((e.nth_deriv(1) - 1.0).abs() < EPS);
        assert!((e.nth_deriv(2) - 1.0).abs() < EPS);
    }

    #[test]
    fn exp_higher_order_matches_self_at_x_eq_0() {
        // exp^(k)(0) = 1 for all k.
        let f: Jet<f64, 5> = Jet::promote(0.0);
        let e = f.exp();
        for k in 0..5 {
            assert!(
                (e.nth_deriv(k) - 1.0).abs() < EPS,
                "k={k} ⇒ {}",
                e.nth_deriv(k)
            );
        }
    }

    #[test]
    fn ln_first_order_matches_analytic_at_x_eq_2() {
        // d/dx ln(x) = 1/x = 0.5 at x=2 ; d²/dx² = -1/x² = -0.25 at x=2.
        let f: Jet<f64, 3> = Jet::promote(2.0);
        let l = f.ln();
        assert!((l.primal() - (2.0_f64).ln()).abs() < EPS);
        assert!((l.nth_deriv(1) - 0.5).abs() < EPS);
        assert!((l.nth_deriv(2) - (-0.25)).abs() < EPS);
    }

    #[test]
    fn ln_third_order_matches_analytic_at_x_eq_2() {
        // d³/dx³ ln(x) = 2/x³ = 0.25 at x=2.
        let f: Jet<f64, 4> = Jet::promote(2.0);
        let l = f.ln();
        assert!((l.nth_deriv(3) - 0.25).abs() < EPS);
    }

    #[test]
    fn ln_then_exp_is_identity() {
        let f: Jet<f64, 4> = Jet::promote(1.7);
        let l = f.ln().exp();
        assert!(l.approx_eq(f, 1e-12));
    }

    #[test]
    fn sin_at_zero_yields_basis_first_derivative() {
        // sin(0) = 0, cos(0) = 1, -sin(0) = 0, -cos(0) = -1
        let f: Jet<f64, 4> = Jet::promote(0.0);
        let s = f.sin();
        assert!(s.primal().abs() < EPS);
        assert!((s.nth_deriv(1) - 1.0).abs() < EPS);
        assert!(s.nth_deriv(2).abs() < EPS);
        assert!((s.nth_deriv(3) - (-1.0)).abs() < EPS);
    }

    #[test]
    fn cos_at_zero_yields_basis_first_derivative() {
        let f: Jet<f64, 4> = Jet::promote(0.0);
        let c = f.cos();
        assert!((c.primal() - 1.0).abs() < EPS);
        assert!(c.nth_deriv(1).abs() < EPS);
        assert!((c.nth_deriv(2) - (-1.0)).abs() < EPS);
        assert!(c.nth_deriv(3).abs() < EPS);
    }

    #[test]
    fn sin_squared_plus_cos_squared_equals_one_in_jet_space() {
        let f: Jet<f64, 4> = Jet::promote(1.234);
        let s2 = f.sin() * f.sin();
        let c2 = f.cos() * f.cos();
        let total = s2 + c2;
        assert!(total.approx_eq(Jet::lift(1.0), 1e-10));
    }

    #[test]
    fn sqrt_first_order_at_x_eq_4() {
        // d/dx sqrt(x) = 1/(2 sqrt(x)) = 0.25 at x=4.
        // d²/dx² sqrt(x) = -1/(4 x^{3/2}) = -1/32 ≈ -0.03125 at x=4.
        let f: Jet<f64, 3> = Jet::promote(4.0);
        let r = f.sqrt();
        assert!((r.primal() - 2.0).abs() < EPS);
        assert!((r.nth_deriv(1) - 0.25).abs() < EPS);
        assert!((r.nth_deriv(2) - (-1.0 / 32.0)).abs() < EPS);
    }

    #[test]
    fn sqrt_squared_is_identity() {
        let f: Jet<f64, 4> = Jet::promote(7.0);
        let s = f.sqrt();
        let back = s * s;
        assert!(back.approx_eq(f, 1e-10));
    }

    #[test]
    fn powf_with_k_eq_zero_is_one() {
        let f: Jet<f64, 3> = Jet::promote(2.5);
        let p = f.powf(0.0);
        assert!(p.approx_eq(Jet::lift(1.0), EPS));
    }

    #[test]
    fn powf_with_k_eq_one_is_identity() {
        let f: Jet<f64, 3> = Jet::promote(2.5);
        let p = f.powf(1.0);
        assert!(p.approx_eq(f, EPS));
    }

    #[test]
    fn powf_with_k_eq_two_matches_self_squared() {
        let f: Jet<f64, 3> = Jet::promote(1.7);
        let p2 = f.powf(2.0);
        let pp = f * f;
        assert!(p2.approx_eq(pp, 1e-10));
    }

    #[test]
    fn powf_x_to_three_first_order_at_x_eq_2() {
        // x³ ⇒ derivative = 3x² = 12 ; 2nd = 6x = 12 at x=2.
        let f: Jet<f64, 3> = Jet::promote(2.0);
        let p = f.powf(3.0);
        assert!((p.primal() - 8.0).abs() < 1e-10);
        assert!((p.nth_deriv(1) - 12.0).abs() < 1e-10);
        assert!((p.nth_deriv(2) - 12.0).abs() < 1e-9);
    }

    #[test]
    fn powi_three_matches_explicit_cube() {
        let f: Jet<f64, 4> = Jet::promote(1.5);
        let pi = f.powi(3);
        let manual = f * f * f;
        assert!(pi.approx_eq(manual, 1e-12));
    }

    #[test]
    fn powi_zero_is_one() {
        let f: Jet<f64, 3> = Jet::promote(99.0);
        let p = f.powi(0);
        assert!(p.approx_eq(Jet::lift(1.0), 0.0));
    }

    #[test]
    fn powi_negative_inverts() {
        let f: Jet<f64, 3> = Jet::promote(2.0);
        let p = f.powi(-1);
        let r = f.recip();
        assert!(p.approx_eq(r, 1e-12));
    }

    #[test]
    fn powi_negative_two_matches_explicit_recip_squared() {
        let f: Jet<f64, 3> = Jet::promote(2.0);
        let p = f.powi(-2);
        let r = f.recip() * f.recip();
        assert!(p.approx_eq(r, 1e-12));
    }

    // ── Composition (Faà di Bruno via callable) ──────────────────────────

    #[test]
    fn compose_identity_is_input() {
        let f: Jet<f64, 3> = Jet::promote(1.5);
        let g = Jet::compose(|j| j, f);
        assert!(g.approx_eq(f, 0.0));
    }

    #[test]
    fn compose_sin_of_identity_matches_sin() {
        let f: Jet<f64, 3> = Jet::promote(0.5);
        let g = Jet::compose(Jet::sin, f);
        assert!(g.approx_eq(f.sin(), 1e-12));
    }

    #[test]
    fn compose_chained_exp_ln_is_identity() {
        let f: Jet<f64, 4> = Jet::promote(2.7);
        let g = Jet::compose(|j: Jet<f64, 4>| j.ln().exp(), f);
        assert!(g.approx_eq(f, 1e-10));
    }

    #[test]
    fn compose_sin_of_squared_matches_chain_rule() {
        // h(x) = sin(x²). h'(x) = 2x cos(x²). h''(x) = 2 cos(x²) - 4 x² sin(x²).
        let x = 1.0_f64;
        let f: Jet<f64, 3> = Jet::promote(x);
        let h = Jet::compose(Jet::sin, f * f);
        let expected_p = (x * x).sin();
        let expected_d1 = 2.0 * x * (x * x).cos();
        let expected_d2 = 2.0 * (x * x).cos() - 4.0 * x * x * (x * x).sin();
        assert!((h.primal() - expected_p).abs() < EPS);
        assert!((h.nth_deriv(1) - expected_d1).abs() < EPS);
        assert!((h.nth_deriv(2) - expected_d2).abs() < EPS);
    }

    // ── KAN-runtime spline 2nd-derivative correctness ────────────────────

    /// Small-cubic-spline fragment of the form `B(x) = a + b·x + c·x² + d·x³`.
    /// The KAN-runtime uses fragments like this in its activation tables — we
    /// gate that the 2nd derivative `B''(x) = 2c + 6d·x` matches the
    /// jet-derived value bit-stably (within `1e-12`).
    fn kan_cubic<const N: usize>(x: Jet<f64, N>, a: f64, b: f64, c: f64, d: f64) -> Jet<f64, N> {
        let x2 = x * x;
        let x3 = x2 * x;
        Jet::lift(a) + x.scale(b) + x2.scale(c) + x3.scale(d)
    }

    #[test]
    fn kan_spline_second_derivative_matches_analytic() {
        // Coefficients chosen for non-trivial output ; eval at x=0.7.
        let (a, b, c, d) = (1.0, -2.0, 3.0, 0.5);
        let x = 0.7_f64;
        let xj: Jet<f64, 3> = Jet::promote(x);
        let bj = kan_cubic(xj, a, b, c, d);
        // Analytic : B'(x) = b + 2c x + 3d x² ; B''(x) = 2c + 6d x.
        let expected_d1 = b + 2.0 * c * x + 3.0 * d * x * x;
        let expected_d2 = 2.0 * c + 6.0 * d * x;
        assert!((bj.nth_deriv(1) - expected_d1).abs() < EPS);
        assert!((bj.nth_deriv(2) - expected_d2).abs() < EPS);
    }

    #[test]
    fn kan_spline_third_derivative_matches_analytic() {
        // For B = a + bx + cx² + dx³, B'''(x) = 6d (constant).
        let (a, b, c, d) = (0.0, 1.0, 2.0, 3.0);
        let x = 0.4_f64;
        let xj: Jet<f64, 4> = Jet::promote(x);
        let bj = kan_cubic(xj, a, b, c, d);
        assert!((bj.nth_deriv(3) - 6.0 * d).abs() < EPS);
    }

    #[test]
    fn kan_spline_fourth_derivative_is_zero_for_cubic() {
        // d⁴/dx⁴ of a cubic is identically zero.
        let xj: Jet<f64, 5> = Jet::promote(0.55);
        let bj = kan_cubic(xj, 0.1, 0.2, 0.3, 0.4);
        assert!(bj.nth_deriv(4).abs() < EPS);
    }

    // ── Hessian-vector-product (univariate slice) ────────────────────────

    #[test]
    fn hvp_of_x_squared_yields_two_v_squared() {
        // f(x) = x², f''(x) = 2 ⇒ univariate-HVP = f''(x) · v² = 2 · v².
        let f = |j: Jet<f64, 3>| j * j;
        for v in [0.0, 1.0, 2.5, -3.0] {
            let r = hessian_vector_product_1d(f, 7.0, v);
            assert!((r - 2.0 * v * v).abs() < EPS, "v={v}");
        }
    }

    #[test]
    fn hvp_of_sin_yields_minus_sin_x_times_v_squared() {
        // f(x) = sin(x), f''(x) = -sin(x), univariate-HVP = -sin(x) · v².
        let f = |j: Jet<f64, 3>| j.sin();
        let x = 0.7_f64;
        let v = 0.3_f64;
        let r = hessian_vector_product_1d(f, x, v);
        let expected = -x.sin() * v * v;
        assert!((r - expected).abs() < EPS);
    }

    #[test]
    fn hvp_axis_alias_matches_underlying() {
        let f = |j: Jet<f64, 3>| j * j;
        let r1 = hessian_vector_product_1d(f, 1.5, 2.0);
        let r2 = hvp_axis(f, 1.5, 2.0);
        assert!((r1 - r2).abs() < EPS);
    }

    #[test]
    fn hvp_of_exp_yields_exp_x_times_v_squared() {
        // f(x) = exp(x), f''(x) = exp(x), univariate-HVP = exp(x) · v².
        let f = |j: Jet<f64, 3>| j.exp();
        let x = 0.4_f64;
        let v = 1.7_f64;
        let r = hessian_vector_product_1d(f, x, v);
        let expected = x.exp() * v * v;
        assert!((r - expected).abs() < EPS);
    }

    // ── Cross-checks vs first-order-only AD (DiffMode::Fwd) ──────────────

    #[test]
    fn order_2_jet_subsumes_first_order_fwd_mode() {
        // First-order forward-mode AD is mathematically equivalent to
        // running a Jet<T, 2> evaluation. We stage that here by computing
        // the same expression two ways and asserting agreement on the
        // 1st-order term — guarding the "Fwd ⊑ Jet<T, 2>" claim from spec
        // line 22.
        // Expression : f(x) = (x + 1)² · sin(x), eval @ x = 0.5.
        let x = 0.5_f64;
        let xj: Jet<f64, 2> = Jet::promote(x);
        let f = (xj + Jet::lift(1.0)) * (xj + Jet::lift(1.0)) * xj.sin();
        // Analytic : f'(x) = 2(x+1) sin(x) + (x+1)² cos(x).
        let expected = 2.0 * (x + 1.0) * x.sin() + (x + 1.0) * (x + 1.0) * x.cos();
        assert!((f.nth_deriv(1) - expected).abs() < EPS);
    }

    #[test]
    fn binom_reference_values() {
        type J = Jet<f64, 1>;
        assert_eq!(J::binom(0, 0), 1.0);
        assert_eq!(J::binom(5, 0), 1.0);
        assert_eq!(J::binom(5, 5), 1.0);
        assert_eq!(J::binom(5, 2), 10.0);
        assert_eq!(J::binom(7, 3), 35.0);
        assert_eq!(J::binom(2, 5), 0.0);
    }

    #[test]
    fn factorial_reference_values() {
        type J = Jet<f64, 1>;
        assert_eq!(J::factorial(0), 1.0);
        assert_eq!(J::factorial(1), 1.0);
        assert_eq!(J::factorial(5), 120.0);
        assert_eq!(J::factorial(10), 3_628_800.0);
    }

    // ── f32 spot-checks (ensure JetField for f32 is wired) ───────────────

    #[test]
    fn f32_jet_basic_arithmetic() {
        let a: Jet<f32, 2> = Jet::promote(2.0);
        let b: Jet<f32, 2> = Jet::lift(3.0);
        let c = a + b;
        assert!((c.primal() - 5.0_f32).abs() < EPS_F32 as f32);
        assert!((c.nth_deriv(1) - 1.0_f32).abs() < EPS_F32 as f32);
    }

    #[test]
    fn f32_jet_exp_first_order_matches_analytic_at_zero() {
        let a: Jet<f32, 2> = Jet::promote(0.0);
        let e = a.exp();
        assert!((e.primal() - 1.0_f32).abs() < EPS_F32 as f32);
        assert!((e.nth_deriv(1) - 1.0_f32).abs() < EPS_F32 as f32);
    }

    // ── High-N sanity (orders 4–8) ───────────────────────────────────────

    #[test]
    fn order_4_sin_at_zero_pattern() {
        // sin / cos / -sin / -cos / sin pattern at x=0 ⇒ 0,1,0,-1,0
        let f: Jet<f64, 5> = Jet::promote(0.0);
        let s = f.sin();
        assert!(s.nth_deriv(0).abs() < EPS);
        assert!((s.nth_deriv(1) - 1.0).abs() < EPS);
        assert!(s.nth_deriv(2).abs() < EPS);
        assert!((s.nth_deriv(3) - (-1.0)).abs() < EPS);
        assert!(s.nth_deriv(4).abs() < EPS);
    }

    #[test]
    fn order_6_exp_at_zero_is_all_ones() {
        let f: Jet<f64, 7> = Jet::promote(0.0);
        let e = f.exp();
        for k in 0..7 {
            assert!((e.nth_deriv(k) - 1.0).abs() < EPS, "k={k}");
        }
    }

    #[test]
    fn order_8_polynomial_kth_derivative_matches_factorial() {
        // For p(x) = x⁷, p^(k)(x) = (7! / (7-k)!) x^(7-k) for k ≤ 7, else 0.
        // At x = 1, p^(k)(1) = 7! / (7-k)!.
        let xj: Jet<f64, 9> = Jet::promote(1.0);
        let p = xj.powi(7);
        for k in 0..=7 {
            // 7! / (7-k)!
            let expected = (8 - 1..=7).take(k).product::<usize>().max(1) as f64;
            // ↑ degenerate for k=0: empty product = 1 ; otherwise product of
            // `7, 6, ..., 8-k`.
            let ref_expected: f64 = if k == 0 {
                1.0
            } else {
                let mut acc = 1.0_f64;
                for i in 0..k {
                    acc *= (7 - i) as f64;
                }
                acc
            };
            assert!(
                (p.nth_deriv(k) - ref_expected).abs() < 1e-9,
                "k={k} got={} want={ref_expected} (sanity-{expected})",
                p.nth_deriv(k)
            );
        }
        // p^(8) is identically zero for a degree-7 polynomial.
        assert!(p.nth_deriv(8).abs() < 1e-9);
    }

    #[test]
    fn order_4_division_matches_quotient_rule() {
        // f(x) = (x² + 1) / x at x=2 ⇒ 5/2 = 2.5
        // f'(x) = (x² - 1)/x² ⇒ 3/4 = 0.75
        // f''(x) = 2/x³ ⇒ 2/8 = 0.25
        let xj: Jet<f64, 4> = Jet::promote(2.0);
        let num = xj * xj + Jet::lift(1.0);
        let q = num / xj;
        assert!((q.primal() - 2.5).abs() < EPS);
        assert!((q.nth_deriv(1) - 0.75).abs() < EPS);
        assert!((q.nth_deriv(2) - 0.25).abs() < EPS);
    }

    // ── Composition stress + jet-of-jet usage ────────────────────────────

    #[test]
    fn deep_composition_sin_exp_ln_is_consistent() {
        // sin(exp(ln(x))) at x = 0.9 ⇒ sin(x) (since exp ∘ ln = id).
        let xj: Jet<f64, 4> = Jet::promote(0.9);
        let h = xj.ln().exp().sin();
        let direct = xj.sin();
        assert!(h.approx_eq(direct, 1e-10));
    }

    #[test]
    fn jet_lift_constant_passes_through_arithmetic_cleanly() {
        // c + (x · y) should preserve the c only on the primal of the result.
        let xj: Jet<f64, 3> = Jet::promote(2.0);
        let yj: Jet<f64, 3> = Jet::lift(3.0);
        let r = xj * yj + Jet::lift(7.0);
        assert!((r.primal() - 13.0).abs() < EPS);
        assert!((r.nth_deriv(1) - 3.0).abs() < EPS); // d/dx (3x + 7) = 3
        assert!(r.nth_deriv(2).abs() < EPS);
    }

    #[test]
    fn debug_format_includes_order_and_terms() {
        let j: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0]);
        let s = format!("{j:?}");
        assert!(s.contains("Jet"));
        assert!(s.contains("order"));
        assert!(s.contains("terms"));
    }

    #[test]
    fn approx_eq_is_symmetric() {
        let a: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0]);
        let b: Jet<f64, 3> = Jet::new([1.0, 2.0, 3.0001]);
        assert!(a.approx_eq(b, 1e-3));
        assert!(b.approx_eq(a, 1e-3));
        assert!(!a.approx_eq(b, 1e-5));
    }

    // ── Numerical-stability gates (no NaN / no overflow on ordinary ops) ─

    #[test]
    fn small_argument_log_no_nan() {
        let xj: Jet<f64, 4> = Jet::promote(1e-3);
        let l = xj.ln();
        for k in 0..4 {
            assert!(l.nth_deriv(k).is_finite(), "k={k}");
        }
    }

    #[test]
    fn nonzero_division_no_nan() {
        let xj: Jet<f64, 4> = Jet::promote(0.1);
        let yj: Jet<f64, 4> = Jet::lift(1.0);
        let q = yj / xj;
        for k in 0..4 {
            assert!(q.nth_deriv(k).is_finite(), "k={k}");
        }
    }

    // ── Spec-claim gate : Jet<T, 1> ≡ T (storage = primal-only) ───────────

    #[test]
    fn jet_of_storage_one_acts_like_scalar_primal() {
        let j: Jet<f64, 1> = Jet::lift(3.5);
        assert_eq!(j.primal(), 3.5);
        // Order-0 jet : nth_deriv(k) for k>=1 is implicit zero.
        assert_eq!(j.nth_deriv(1), 0.0);
        assert_eq!(j.nth_deriv(2), 0.0);
        // Arithmetic falls through to scalar-arith on the primal.
        let k: Jet<f64, 1> = Jet::lift(7.0);
        let s = j + k;
        assert_eq!(s.primal(), 10.5);
        let p = j * k;
        assert_eq!(p.primal(), 24.5);
    }

    // ── Mixed multi-fn pipelines (matches KAN-runtime usage shape) ───────

    #[test]
    fn kan_activation_pipeline_silu_2nd_derivative() {
        // SiLU(x) = x · σ(x) = x / (1 + exp(-x)) where σ is the logistic
        // sigmoid. Reference values at x=0 :
        //   SiLU(0)  =  0
        //   SiLU'(0) =  σ(0)  + 0 · σ'(0)         =  0.5
        //   SiLU''(0) = 2·σ'(0) + 0·σ''(0)         =  2 · 0.25 = 0.5
        // We compute via a Jet<f64, 3>.
        let xj: Jet<f64, 3> = Jet::promote(0.0);
        let neg_x = -xj;
        let one_plus_exp = Jet::lift(1.0) + neg_x.exp();
        let silu = xj / one_plus_exp;
        assert!(silu.primal().abs() < EPS);
        assert!((silu.nth_deriv(1) - 0.5).abs() < EPS);
        assert!((silu.nth_deriv(2) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn product_of_three_jets_matches_iterated_chain() {
        // f(x) = x · sin(x) · exp(x), Manual expansion of the 2nd derivative
        // is cumbersome ; we verify via two equivalent groupings of the
        // associative product.
        let xj: Jet<f64, 4> = Jet::promote(0.6);
        let p1 = xj * (xj.sin() * xj.exp());
        let p2 = (xj * xj.sin()) * xj.exp();
        assert!(p1.approx_eq(p2, 1e-12));
    }

    #[test]
    fn distribution_of_mul_over_add_holds_in_jet_space() {
        // a · (b + c) == a·b + a·c
        let aj: Jet<f64, 3> = Jet::promote(1.3);
        let bj: Jet<f64, 3> = Jet::lift(2.1);
        let cj: Jet<f64, 3> = Jet::lift(0.7);
        let lhs = aj * (bj + cj);
        let rhs = aj * bj + aj * cj;
        assert!(lhs.approx_eq(rhs, 1e-12));
    }

    #[test]
    fn additive_inverse_in_jet_space() {
        // a + (-a) == zero
        let aj: Jet<f64, 4> = Jet::promote(2.0);
        let z = aj + (-aj);
        assert!(z.approx_eq(Jet::zero(), 0.0));
    }

    #[test]
    fn multiplicative_inverse_in_jet_space() {
        // a · (1/a) == one
        let aj: Jet<f64, 4> = Jet::promote(2.0);
        let r = aj * aj.recip();
        assert!(r.approx_eq(Jet::one(), 1e-12));
    }

    // ── Higher-N HVP sanity (using order-3 storage = order-2 jet) ────────

    #[test]
    fn hvp_of_x_cubed_yields_six_x_v_squared() {
        // f(x) = x³ ⇒ f''(x) = 6x ⇒ univariate-HVP = 6x · v².
        let f = |j: Jet<f64, 3>| j * j * j;
        for x in [-2.0, 0.0, 1.5, 4.7] {
            for v in [-1.0, 0.0, 0.5, 3.0] {
                let r = hessian_vector_product_1d(f, x, v);
                let expected = 6.0 * x * v * v;
                assert!((r - expected).abs() < 1e-10, "x={x} v={v}");
            }
        }
    }

    #[test]
    fn hvp_of_log_yields_minus_v_squared_over_x_squared() {
        // f(x) = ln(x) ⇒ f''(x) = -1/x² ⇒ univariate-HVP = -v² / x².
        let f = |j: Jet<f64, 3>| j.ln();
        for x in [0.5_f64, 1.0, 2.0, 4.0] {
            for v in [-1.0_f64, 0.5, 2.0] {
                let r = hessian_vector_product_1d(f, x, v);
                let expected = -(v * v) / (x * x);
                assert!((r - expected).abs() < 1e-10, "x={x} v={v}");
            }
        }
    }
}
