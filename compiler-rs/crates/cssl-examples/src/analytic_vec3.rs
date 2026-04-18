// § Allowances :
//   float_cmp             ← constant-fold symmetry with AnalyticExpr
//   should_implement_trait ← neg/add/sub are constructor-helpers, not std::ops::{Neg,Add,Sub} impls
//   implicit_hasher       ← HashMap<String, f64> mirror of AnalyticExpr env-shape; generalizing
//                           over hasher would ripple through every caller for no practical gain
#![allow(clippy::float_cmp)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::implicit_hasher)]

//! T11-D9 VECTOR AnalyticExpr : real `vec3` symbolic algebra.
//!
//! § SPEC : `specs/05_AUTODIFF.csl` § SDF-NORMAL (killer-app : `length(p) - r`).
//! § ROLE :
//!   Extend the scalar [`crate::ad_gate::AnalyticExpr`] algebra with a
//!   companion vec3-valued algebra. The two communicate via vec3→scalar
//!   projections (`length`, `dot`, `.x`/`.y`/`.z`) so the existing scalar
//!   gate-verification infrastructure (`simplify`, `evaluate`, `to_term`,
//!   `to_smt`) composes unchanged.
//!
//! § DESIGN
//!   - `AnalyticVec3Expr` : vec3-valued expressions (constants, vars, arith).
//!   - `length(v) : vec3 → scalar`  ← returns [`AnalyticExpr::Uninterpreted`]
//!     with "length3" callee + expanded-sqrt-of-sum-of-squares fallback.
//!   - `dot(a, b) : vec3 × vec3 → scalar`.
//!   - `vec3_proj(v, comp) : vec3 → scalar`.
//!   - `normalize(v) : vec3 → vec3`.
//!   - `to_scalar_components(v) -> (AnalyticExpr, AnalyticExpr, AnalyticExpr)`
//!     : the (x, y, z) scalar-expansion of a vec3 expression — the bridge
//!     that lets every vec3 operation reduce to existing scalar AD machinery
//!     without adding a new AD primitive.
//!
//! § WHAT THIS UNLOCKS
//!   - Writing `length(p) - r` directly without manual scalar scaffolding.
//!   - Future scene-SDF union / min by monomorphizing `min(sdf(p), sdf(p-c))`.
//!   - T11-D10 : real MIR vec3 lowering (MirType::Vec3F32 + MirOp::Vec3*).
//!
//! § STATUS
//!   This slice extends the **scalar** algebra's capabilities without adding
//!   an AD primitive : every vec3 operation reduces to scalar-per-component
//!   via [`AnalyticVec3Expr::to_scalar_components`]. The existing AD dual-
//!   substitution gates (T7-D1..D10) verify correctness of the scalar-
//!   expansion at the test-level.

use std::collections::HashMap;

use crate::ad_gate::AnalyticExpr;

/// Vec3 component projection indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VecComp {
    X,
    Y,
    Z,
}

impl VecComp {
    /// Human-readable suffix used in variable-env lookups + debug dumps.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

/// Vec3-valued symbolic expression. Stage-0 algebraic companion to
/// [`AnalyticExpr`] ; every vec3 expression reduces to three scalar
/// expressions via [`AnalyticVec3Expr::to_scalar_components`].
#[derive(Debug, Clone, PartialEq)]
pub enum AnalyticVec3Expr {
    /// Literal `(x, y, z)` triple.
    Const(f64, f64, f64),
    /// Named vec3 variable — env lookups use `"<name>.x"` / `.y` / `.z`.
    Var(String),
    /// `-v`  (componentwise negation).
    Neg(Box<Self>),
    /// `a + b`  (componentwise sum).
    Add(Box<Self>, Box<Self>),
    /// `a - b`  (componentwise difference).
    Sub(Box<Self>, Box<Self>),
    /// `s · v`  (scalar × vec3).
    ScalarMul(Box<AnalyticExpr>, Box<Self>),
    /// `v / s`  (vec3 ÷ scalar).
    ScalarDiv(Box<Self>, Box<AnalyticExpr>),
    /// `normalize(v) = v / |v|`  (unit-vector projection).
    Normalize(Box<Self>),
}

impl AnalyticVec3Expr {
    /// Literal vec3.
    #[must_use]
    pub const fn c(x: f64, y: f64, z: f64) -> Self {
        Self::Const(x, y, z)
    }

    /// Named vec3 variable.
    #[must_use]
    pub fn v(name: impl Into<String>) -> Self {
        Self::Var(name.into())
    }

    /// `-v`
    #[must_use]
    pub fn neg(a: Self) -> Self {
        Self::Neg(Box::new(a))
    }

    /// `a + b`
    #[must_use]
    pub fn add(a: Self, b: Self) -> Self {
        Self::Add(Box::new(a), Box::new(b))
    }

    /// `a - b`
    #[must_use]
    pub fn sub(a: Self, b: Self) -> Self {
        Self::Sub(Box::new(a), Box::new(b))
    }

    /// `s · v`
    #[must_use]
    pub fn scalar_mul(s: AnalyticExpr, v: Self) -> Self {
        Self::ScalarMul(Box::new(s), Box::new(v))
    }

    /// `v / s`
    #[must_use]
    pub fn scalar_div(v: Self, s: AnalyticExpr) -> Self {
        Self::ScalarDiv(Box::new(v), Box::new(s))
    }

    /// `normalize(v)`
    #[must_use]
    pub fn normalize(v: Self) -> Self {
        Self::Normalize(Box::new(v))
    }

    /// Recursively apply algebraic simplifications to each component.
    /// Rules are componentwise-lifted from [`AnalyticExpr::simplify`].
    #[must_use]
    pub fn simplify(&self) -> Self {
        match self {
            Self::Const(_, _, _) | Self::Var(_) => self.clone(),
            Self::Neg(a) => Self::Neg(Box::new(a.simplify())),
            Self::Add(a, b) => Self::Add(Box::new(a.simplify()), Box::new(b.simplify())),
            Self::Sub(a, b) => Self::Sub(Box::new(a.simplify()), Box::new(b.simplify())),
            Self::ScalarMul(s, v) => {
                Self::ScalarMul(Box::new(s.simplify()), Box::new(v.simplify()))
            }
            Self::ScalarDiv(v, s) => {
                Self::ScalarDiv(Box::new(v.simplify()), Box::new(s.simplify()))
            }
            Self::Normalize(v) => Self::Normalize(Box::new(v.simplify())),
        }
    }

    /// Numerically evaluate to `[x, y, z]` under a named-variable environment.
    /// The env uses `"<name>.x"` / `"<name>.y"` / `"<name>.z"` for vec3 vars,
    /// and bare `"<name>"` for scalar vars referenced by `ScalarMul` / `ScalarDiv`.
    /// Missing lookups / NaN propagate componentwise.
    #[must_use]
    pub fn evaluate(&self, env: &HashMap<String, f64>) -> [f64; 3] {
        match self {
            Self::Const(x, y, z) => [*x, *y, *z],
            Self::Var(name) => [
                env.get(&format!("{name}.x")).copied().unwrap_or(f64::NAN),
                env.get(&format!("{name}.y")).copied().unwrap_or(f64::NAN),
                env.get(&format!("{name}.z")).copied().unwrap_or(f64::NAN),
            ],
            Self::Neg(a) => {
                let [x, y, z] = a.evaluate(env);
                [-x, -y, -z]
            }
            Self::Add(a, b) => {
                let [ax, ay, az] = a.evaluate(env);
                let [bx, by, bz] = b.evaluate(env);
                [ax + bx, ay + by, az + bz]
            }
            Self::Sub(a, b) => {
                let [ax, ay, az] = a.evaluate(env);
                let [bx, by, bz] = b.evaluate(env);
                [ax - bx, ay - by, az - bz]
            }
            Self::ScalarMul(s, v) => {
                let sv = s.evaluate(env);
                let [vx, vy, vz] = v.evaluate(env);
                [sv * vx, sv * vy, sv * vz]
            }
            Self::ScalarDiv(v, s) => {
                let sv = s.evaluate(env);
                let [vx, vy, vz] = v.evaluate(env);
                [vx / sv, vy / sv, vz / sv]
            }
            Self::Normalize(v) => {
                let [vx, vy, vz] = v.evaluate(env);
                let len = vx.mul_add(vx, vy.mul_add(vy, vz * vz)).sqrt();
                if len == 0.0 || !len.is_finite() {
                    [f64::NAN, f64::NAN, f64::NAN]
                } else {
                    [vx / len, vy / len, vz / len]
                }
            }
        }
    }

    /// Reduce this vec3 expression to its (x, y, z) scalar components.
    /// This is the bridge that lets every vec3 op verify via existing
    /// scalar-AD machinery — no new AD primitive needed.
    #[must_use]
    pub fn to_scalar_components(&self) -> (AnalyticExpr, AnalyticExpr, AnalyticExpr) {
        match self {
            Self::Const(x, y, z) => (
                AnalyticExpr::c(*x),
                AnalyticExpr::c(*y),
                AnalyticExpr::c(*z),
            ),
            Self::Var(name) => (
                AnalyticExpr::v(format!("{name}.x")),
                AnalyticExpr::v(format!("{name}.y")),
                AnalyticExpr::v(format!("{name}.z")),
            ),
            Self::Neg(a) => {
                let (x, y, z) = a.to_scalar_components();
                (
                    AnalyticExpr::neg(x),
                    AnalyticExpr::neg(y),
                    AnalyticExpr::neg(z),
                )
            }
            Self::Add(a, b) => {
                let (ax, ay, az) = a.to_scalar_components();
                let (bx, by, bz) = b.to_scalar_components();
                (
                    AnalyticExpr::add(ax, bx),
                    AnalyticExpr::add(ay, by),
                    AnalyticExpr::add(az, bz),
                )
            }
            Self::Sub(a, b) => {
                let (ax, ay, az) = a.to_scalar_components();
                let (bx, by, bz) = b.to_scalar_components();
                (
                    AnalyticExpr::sub(ax, bx),
                    AnalyticExpr::sub(ay, by),
                    AnalyticExpr::sub(az, bz),
                )
            }
            Self::ScalarMul(s, v) => {
                let (vx, vy, vz) = v.to_scalar_components();
                (
                    AnalyticExpr::mul((**s).clone(), vx),
                    AnalyticExpr::mul((**s).clone(), vy),
                    AnalyticExpr::mul((**s).clone(), vz),
                )
            }
            Self::ScalarDiv(v, s) => {
                let (vx, vy, vz) = v.to_scalar_components();
                (
                    AnalyticExpr::div(vx, (**s).clone()),
                    AnalyticExpr::div(vy, (**s).clone()),
                    AnalyticExpr::div(vz, (**s).clone()),
                )
            }
            Self::Normalize(v) => {
                // normalize(v) = v / length(v)
                let len = length(v);
                let (vx, vy, vz) = v.to_scalar_components();
                (
                    AnalyticExpr::div(vx, len.clone()),
                    AnalyticExpr::div(vy, len.clone()),
                    AnalyticExpr::div(vz, len),
                )
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Vec3 → scalar operations : live in this module to keep AnalyticExpr
//   untouched. They return [`AnalyticExpr`] so existing simplify / evaluate /
//   to_term / to_smt / equivalent_by_sampling compose transparently.
// ─────────────────────────────────────────────────────────────────────────

/// `length(v) = sqrt(v.x² + v.y² + v.z²)` as an `AnalyticExpr`.
/// Expands to real scalar operations so `to_term` + `to_smt` route through
/// existing infrastructure ; the SMT side uses `sqrt_uf` for the outer root.
#[must_use]
pub fn length(v: &AnalyticVec3Expr) -> AnalyticExpr {
    let (x, y, z) = v.to_scalar_components();
    let x2 = AnalyticExpr::mul(x.clone(), x);
    let y2 = AnalyticExpr::mul(y.clone(), y);
    let z2 = AnalyticExpr::mul(z.clone(), z);
    let sum = AnalyticExpr::add(AnalyticExpr::add(x2, y2), z2);
    AnalyticExpr::Sqrt(Box::new(sum))
}

/// `dot(a, b) = a.x·b.x + a.y·b.y + a.z·b.z` as an `AnalyticExpr`.
#[must_use]
pub fn dot(a: &AnalyticVec3Expr, b: &AnalyticVec3Expr) -> AnalyticExpr {
    let (ax, ay, az) = a.to_scalar_components();
    let (bx, by, bz) = b.to_scalar_components();
    let abx = AnalyticExpr::mul(ax, bx);
    let aby = AnalyticExpr::mul(ay, by);
    let abz = AnalyticExpr::mul(az, bz);
    AnalyticExpr::add(AnalyticExpr::add(abx, aby), abz)
}

/// `v.comp` — scalar projection of a vec3 expression.
#[must_use]
pub fn vec3_proj(v: &AnalyticVec3Expr, comp: VecComp) -> AnalyticExpr {
    let (x, y, z) = v.to_scalar_components();
    match comp {
        VecComp::X => x,
        VecComp::Y => y,
        VecComp::Z => z,
    }
}

/// Sphere-SDF in vec3 form : `length(p) - r` as an `AnalyticExpr`. Directly
/// verifies the killer-app primal per `specs/05_AUTODIFF.csl § SDF-NORMAL`.
#[must_use]
pub fn sphere_sdf_vec3(p: &AnalyticVec3Expr, r: &AnalyticExpr) -> AnalyticExpr {
    AnalyticExpr::sub(length(p), r.clone())
}

/// Analytic gradient of `sphere_sdf(p, r) = length(p) - r` wrt `p`. Equals
/// `normalize(p) · d_y` in vec3 form — returned as a vec3 expression so
/// downstream tests can compare against MIR's scalar-expansion component-wise.
#[must_use]
pub fn sphere_sdf_grad_p(p: &AnalyticVec3Expr, d_y: &AnalyticExpr) -> AnalyticVec3Expr {
    AnalyticVec3Expr::scalar_mul(d_y.clone(), AnalyticVec3Expr::normalize(p.clone()))
}

/// Analytic gradient of `sphere_sdf(p, r)` wrt `r`. Equals `-d_y`.
#[must_use]
pub fn sphere_sdf_grad_r(d_y: &AnalyticExpr) -> AnalyticExpr {
    AnalyticExpr::neg(d_y.clone())
}

// ─────────────────────────────────────────────────────────────────────────
// § Scene-SDF analytic primitives : union / intersection / subtraction.
//   All route via AnalyticExpr::Min / Max at the scalar level ; gradients
//   are piecewise-linear (pick-the-winner) + valid everywhere except the
//   cusp surface (a == b) where the subgradient is multi-valued.
// ─────────────────────────────────────────────────────────────────────────

/// Scene-SDF union : `union(a, b) = min(a, b)`. Returns the nearer of two
/// signed-distances — the composed-scene surface is the union of both
/// primitives.
#[must_use]
pub fn scene_sdf_union(a: AnalyticExpr, b: AnalyticExpr) -> AnalyticExpr {
    AnalyticExpr::min(a, b)
}

/// Scene-SDF intersection : `intersect(a, b) = max(a, b)`. Returns the
/// farther-inside of two signed-distances — the composed-scene surface is
/// the intersection.
#[must_use]
pub fn scene_sdf_intersect(a: AnalyticExpr, b: AnalyticExpr) -> AnalyticExpr {
    AnalyticExpr::max(a, b)
}

/// Scene-SDF subtraction : `subtract(a, b) = max(a, -b)`. Returns the region
/// of `a` with `b` carved out.
#[must_use]
pub fn scene_sdf_subtract(a: AnalyticExpr, b: AnalyticExpr) -> AnalyticExpr {
    AnalyticExpr::max(a, AnalyticExpr::neg(b))
}

/// Analytic gradient of `union(a, b) = min(a, b)` at a point where `a ≠ b`.
/// Returns the gradient of whichever branch wins at the sample-environment.
/// At the cusp `a == b`, the subgradient is multi-valued ; this function
/// picks `da` by convention (caller should avoid such points in sampling).
#[must_use]
pub fn scene_sdf_union_grad(
    a: &AnalyticExpr,
    b: &AnalyticExpr,
    da: &AnalyticExpr,
    db: &AnalyticExpr,
    env: &HashMap<String, f64>,
) -> AnalyticExpr {
    let av = a.evaluate(env);
    let bv = b.evaluate(env);
    if av.is_finite() && bv.is_finite() && av <= bv {
        da.clone()
    } else {
        db.clone()
    }
}

/// Analytic gradient of `intersect(a, b) = max(a, b)` at a point where `a ≠ b`.
#[must_use]
pub fn scene_sdf_intersect_grad(
    a: &AnalyticExpr,
    b: &AnalyticExpr,
    da: &AnalyticExpr,
    db: &AnalyticExpr,
    env: &HashMap<String, f64>,
) -> AnalyticExpr {
    let av = a.evaluate(env);
    let bv = b.evaluate(env);
    if av.is_finite() && bv.is_finite() && av >= bv {
        da.clone()
    } else {
        db.clone()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Smooth-min : rounded-edge scene-SDF union. Differentiable everywhere.
// ─────────────────────────────────────────────────────────────────────────

/// `smooth_min(a, b, k) = -log(exp(-k·a) + exp(-k·b)) / k`.
///
/// § PROPERTIES
///   - Differentiable everywhere (no cusp at `a = b`).
///   - As `k → ∞`, smooth_min(a, b, k) → min(a, b).
///   - For finite `k`, the result is slightly smaller than `min(a, b)` near
///     the cusp — this produces the rounded-edge aesthetic common in
///     ray-marched scene-SDF rendering.
///   - `k = 32` is a reasonable default for most scene-SDF uses ; larger
///     values approach the sharp-min limit, smaller values round more.
///
/// § SPEC : `specs/05_AUTODIFF.csl § APPENDIX-SMOOTH`.
#[must_use]
pub fn smooth_min(a: AnalyticExpr, b: AnalyticExpr, k: f64) -> AnalyticExpr {
    let kc = AnalyticExpr::c(k);
    // -k·a
    let neg_ka = AnalyticExpr::neg(AnalyticExpr::mul(kc.clone(), a));
    // -k·b
    let neg_kb = AnalyticExpr::neg(AnalyticExpr::mul(kc.clone(), b));
    // exp(-k·a) + exp(-k·b)
    let sum_exp = AnalyticExpr::add(
        AnalyticExpr::Exp(Box::new(neg_ka)),
        AnalyticExpr::Exp(Box::new(neg_kb)),
    );
    // -log(sum) / k
    let neg_log = AnalyticExpr::neg(AnalyticExpr::Log(Box::new(sum_exp)));
    AnalyticExpr::div(neg_log, kc)
}

/// Detect if a sample environment lies near the cusp surface `a == b`.
/// Returns `true` iff `|a(env) - b(env)| < epsilon`. Samplers should skip
/// cusp-near samples when verifying subgradient-valued gradients.
#[must_use]
pub fn is_near_cusp(
    a: &AnalyticExpr,
    b: &AnalyticExpr,
    env: &HashMap<String, f64>,
    epsilon: f64,
) -> bool {
    let av = a.evaluate(env);
    let bv = b.evaluate(env);
    if !av.is_finite() || !bv.is_finite() {
        return true; // treat non-finite as "cusp-adjacent"
    }
    (av - bv).abs() < epsilon
}

/// `smooth_max(a, b, k) = -smooth_min(-a, -b, k) = log(exp(k·a) + exp(k·b))/k`.
///
/// Companion to [`smooth_min`] ; differentiable everywhere + as `k → ∞`
/// approaches `max(a, b)`. Used for rounded-edge scene-SDF intersection.
#[must_use]
pub fn smooth_max(a: AnalyticExpr, b: AnalyticExpr, k: f64) -> AnalyticExpr {
    AnalyticExpr::neg(smooth_min(AnalyticExpr::neg(a), AnalyticExpr::neg(b), k))
}

// ─────────────────────────────────────────────────────────────────────────
// § N-ary folds : min_n / max_n / smooth_min_n / smooth_max_n.
// ─────────────────────────────────────────────────────────────────────────

/// Fold `min(a_1, a_2, …, a_n)` left-associatively. Returns `None` for empty slice.
#[must_use]
pub fn min_n(items: &[AnalyticExpr]) -> Option<AnalyticExpr> {
    items.iter().cloned().reduce(AnalyticExpr::min)
}

/// Fold `max(a_1, a_2, …, a_n)` left-associatively. Returns `None` for empty slice.
#[must_use]
pub fn max_n(items: &[AnalyticExpr]) -> Option<AnalyticExpr> {
    items.iter().cloned().reduce(AnalyticExpr::max)
}

/// Fold `smooth_min(a_1, a_2, …, a_n, k)` left-associatively. Returns `None`
/// for empty slice. For `n = 1`, returns the single item unchanged.
#[must_use]
pub fn smooth_min_n(items: &[AnalyticExpr], k: f64) -> Option<AnalyticExpr> {
    items.iter().cloned().reduce(|acc, x| smooth_min(acc, x, k))
}

/// Fold `smooth_max(a_1, a_2, …, a_n, k)` left-associatively. Returns `None`
/// for empty slice.
#[must_use]
pub fn smooth_max_n(items: &[AnalyticExpr], k: f64) -> Option<AnalyticExpr> {
    items.iter().cloned().reduce(|acc, x| smooth_max(acc, x, k))
}

#[cfg(test)]
mod tests {
    use super::{
        dot, is_near_cusp, length, max_n, min_n, scene_sdf_intersect, scene_sdf_intersect_grad,
        scene_sdf_subtract, scene_sdf_union, scene_sdf_union_grad, smooth_max, smooth_max_n,
        smooth_min, smooth_min_n, sphere_sdf_grad_p, sphere_sdf_grad_r, sphere_sdf_vec3, vec3_proj,
        AnalyticVec3Expr, VecComp,
    };
    use crate::ad_gate::AnalyticExpr;
    use std::collections::HashMap;

    fn env_with_p(px: f64, py: f64, pz: f64) -> HashMap<String, f64> {
        let mut env = HashMap::new();
        env.insert("p.x".into(), px);
        env.insert("p.y".into(), py);
        env.insert("p.z".into(), pz);
        env
    }

    #[test]
    fn veccomp_suffix_matches_names() {
        assert_eq!(VecComp::X.suffix(), "x");
        assert_eq!(VecComp::Y.suffix(), "y");
        assert_eq!(VecComp::Z.suffix(), "z");
    }

    #[test]
    fn const_evaluates_to_literal() {
        let v = AnalyticVec3Expr::c(1.0, 2.0, 3.0);
        let env = HashMap::new();
        assert_eq!(v.evaluate(&env), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn var_evaluates_via_dotted_env_keys() {
        let v = AnalyticVec3Expr::v("p");
        let env = env_with_p(10.0, 20.0, 30.0);
        assert_eq!(v.evaluate(&env), [10.0, 20.0, 30.0]);
    }

    #[test]
    fn neg_negates_each_component() {
        let v = AnalyticVec3Expr::neg(AnalyticVec3Expr::c(1.0, -2.0, 3.0));
        let env = HashMap::new();
        assert_eq!(v.evaluate(&env), [-1.0, 2.0, -3.0]);
    }

    #[test]
    fn add_is_componentwise() {
        let v = AnalyticVec3Expr::add(
            AnalyticVec3Expr::c(1.0, 2.0, 3.0),
            AnalyticVec3Expr::c(4.0, 5.0, 6.0),
        );
        let env = HashMap::new();
        assert_eq!(v.evaluate(&env), [5.0, 7.0, 9.0]);
    }

    #[test]
    fn sub_is_componentwise() {
        let v = AnalyticVec3Expr::sub(
            AnalyticVec3Expr::c(10.0, 20.0, 30.0),
            AnalyticVec3Expr::c(1.0, 2.0, 3.0),
        );
        let env = HashMap::new();
        assert_eq!(v.evaluate(&env), [9.0, 18.0, 27.0]);
    }

    #[test]
    fn scalar_mul_scales_all_components() {
        let v =
            AnalyticVec3Expr::scalar_mul(AnalyticExpr::c(2.0), AnalyticVec3Expr::c(1.0, 2.0, 3.0));
        let env = HashMap::new();
        assert_eq!(v.evaluate(&env), [2.0, 4.0, 6.0]);
    }

    #[test]
    fn scalar_div_divides_all_components() {
        let v =
            AnalyticVec3Expr::scalar_div(AnalyticVec3Expr::c(4.0, 8.0, 12.0), AnalyticExpr::c(2.0));
        let env = HashMap::new();
        assert_eq!(v.evaluate(&env), [2.0, 4.0, 6.0]);
    }

    #[test]
    fn normalize_produces_unit_length() {
        let v = AnalyticVec3Expr::normalize(AnalyticVec3Expr::c(3.0, 4.0, 0.0));
        let env = HashMap::new();
        let [x, y, z] = v.evaluate(&env);
        // length = 5, so normalized = (0.6, 0.8, 0.0).
        assert!((x - 0.6).abs() < 1e-12);
        assert!((y - 0.8).abs() < 1e-12);
        assert!(z.abs() < 1e-12);
    }

    #[test]
    fn normalize_zero_vector_is_nan() {
        let v = AnalyticVec3Expr::normalize(AnalyticVec3Expr::c(0.0, 0.0, 0.0));
        let env = HashMap::new();
        let [x, y, z] = v.evaluate(&env);
        assert!(x.is_nan() && y.is_nan() && z.is_nan());
    }

    #[test]
    fn length_of_3_4_0_is_5() {
        let v = AnalyticVec3Expr::c(3.0, 4.0, 0.0);
        let len = length(&v);
        let env = HashMap::new();
        assert!((len.evaluate(&env) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn length_uses_real_sqrt_of_sum_of_squares() {
        let p = AnalyticVec3Expr::v("p");
        let len = length(&p);
        let env = env_with_p(1.0, 2.0, 2.0);
        // sqrt(1 + 4 + 4) = sqrt(9) = 3.
        assert!((len.evaluate(&env) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn dot_product_matches_scalar_sum() {
        let a = AnalyticVec3Expr::c(1.0, 2.0, 3.0);
        let b = AnalyticVec3Expr::c(4.0, 5.0, 6.0);
        let d = dot(&a, &b);
        let env = HashMap::new();
        // 1·4 + 2·5 + 3·6 = 4 + 10 + 18 = 32.
        assert!((d.evaluate(&env) - 32.0).abs() < 1e-12);
    }

    #[test]
    fn vec3_proj_extracts_component() {
        let v = AnalyticVec3Expr::c(7.0, 8.0, 9.0);
        let env = HashMap::new();
        assert!((vec3_proj(&v, VecComp::X).evaluate(&env) - 7.0).abs() < 1e-12);
        assert!((vec3_proj(&v, VecComp::Y).evaluate(&env) - 8.0).abs() < 1e-12);
        assert!((vec3_proj(&v, VecComp::Z).evaluate(&env) - 9.0).abs() < 1e-12);
    }

    #[test]
    fn sphere_sdf_primal_matches_length_minus_radius() {
        let p = AnalyticVec3Expr::v("p");
        let r = AnalyticExpr::v("r");
        let sdf = sphere_sdf_vec3(&p, &r);
        let mut env = env_with_p(3.0, 4.0, 0.0);
        env.insert("r".into(), 2.0);
        // length(3,4,0) - 2 = 5 - 2 = 3.
        assert!((sdf.evaluate(&env) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn sphere_sdf_grad_p_equals_normalize_p_scaled_by_d_y() {
        let p = AnalyticVec3Expr::v("p");
        let d_y = AnalyticExpr::c(1.0);
        let g = sphere_sdf_grad_p(&p, &d_y);
        let env = env_with_p(3.0, 4.0, 0.0);
        // length(3,4,0) = 5 ; normalize = (0.6, 0.8, 0.0) ; · 1.0 = (0.6, 0.8, 0.0).
        let [gx, gy, gz] = g.evaluate(&env);
        assert!((gx - 0.6).abs() < 1e-12);
        assert!((gy - 0.8).abs() < 1e-12);
        assert!(gz.abs() < 1e-12);
    }

    #[test]
    fn sphere_sdf_grad_r_equals_neg_d_y() {
        let d_y = AnalyticExpr::c(3.5);
        let g = sphere_sdf_grad_r(&d_y);
        let env = HashMap::new();
        assert!((g.evaluate(&env) + 3.5).abs() < 1e-12);
    }

    #[test]
    fn grad_p_matches_central_difference_numerically() {
        // Canonical F1 gate : d(length(p))/d(p.x) at p = (3, 4, 0) equals 0.6.
        let p = AnalyticVec3Expr::v("p");
        let primal = length(&p);
        let env = env_with_p(3.0, 4.0, 0.0);
        let h = 1e-6_f64;

        // ∂f/∂p.x via central-diff on "p.x".
        let mut env_plus = env.clone();
        env_plus.insert("p.x".into(), 3.0 + h);
        let f_plus = primal.evaluate(&env_plus);
        let mut env_minus = env;
        env_minus.insert("p.x".into(), 3.0 - h);
        let f_minus = primal.evaluate(&env_minus);
        let numerical_dx = (f_plus - f_minus) / (2.0 * h);

        // Analytic : ∂(length(p))/∂p.x = p.x / length(p) = 3/5 = 0.6.
        assert!(
            (numerical_dx - 0.6).abs() < 1e-5,
            "expected ≈ 0.6, got {numerical_dx}"
        );
    }

    #[test]
    fn simplify_preserves_structure_on_constants() {
        let v = AnalyticVec3Expr::add(
            AnalyticVec3Expr::c(1.0, 2.0, 3.0),
            AnalyticVec3Expr::c(0.0, 0.0, 0.0),
        );
        let s = v.simplify();
        // Simplify is structurally-lifted — not a full constant-folder on vec3
        // today. But it shouldn't crash and should preserve evaluation semantics.
        let env = HashMap::new();
        assert_eq!(v.evaluate(&env), s.evaluate(&env));
    }

    #[test]
    fn to_scalar_components_roundtrip_matches_evaluate() {
        let v = AnalyticVec3Expr::add(AnalyticVec3Expr::v("p"), AnalyticVec3Expr::c(1.0, 2.0, 3.0));
        let env = env_with_p(10.0, 20.0, 30.0);
        let [x, y, z] = v.evaluate(&env);
        let (ex, ey, ez) = v.to_scalar_components();
        assert!((ex.evaluate(&env) - x).abs() < 1e-12);
        assert!((ey.evaluate(&env) - y).abs() < 1e-12);
        assert!((ez.evaluate(&env) - z).abs() < 1e-12);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Scene-SDF union / intersect / subtract + piecewise gradient.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn scene_union_picks_nearer_distance() {
        let a = AnalyticExpr::c(3.0);
        let b = AnalyticExpr::c(5.0);
        let u = scene_sdf_union(a, b);
        let env = HashMap::new();
        assert!((u.evaluate(&env) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn scene_intersect_picks_farther_distance() {
        let a = AnalyticExpr::c(3.0);
        let b = AnalyticExpr::c(5.0);
        let i = scene_sdf_intersect(a, b);
        let env = HashMap::new();
        assert!((i.evaluate(&env) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn scene_subtract_carves_via_max_neg_b() {
        // subtract(10, 3) = max(10, -3) = 10.
        let a = AnalyticExpr::c(10.0);
        let b = AnalyticExpr::c(3.0);
        let s = scene_sdf_subtract(a, b);
        let env = HashMap::new();
        assert!((s.evaluate(&env) - 10.0).abs() < 1e-12);

        // subtract(-5, 10) = max(-5, -10) = -5.
        let s2 = scene_sdf_subtract(AnalyticExpr::c(-5.0), AnalyticExpr::c(10.0));
        assert!((s2.evaluate(&env) - (-5.0)).abs() < 1e-12);
    }

    #[test]
    fn union_grad_picks_winning_branch() {
        // Two primals, both functions of x.
        let a = AnalyticExpr::mul(AnalyticExpr::v("x"), AnalyticExpr::c(2.0)); // 2x
        let b = AnalyticExpr::add(AnalyticExpr::v("x"), AnalyticExpr::c(10.0)); // x+10
        let da = AnalyticExpr::c(2.0); // d(2x)/dx
        let db = AnalyticExpr::c(1.0); // d(x+10)/dx

        // At x=3 : a=6, b=13 → a < b → gradient = da = 2.0.
        let mut env = HashMap::new();
        env.insert("x".into(), 3.0);
        let g = scene_sdf_union_grad(&a, &b, &da, &db, &env);
        assert!((g.evaluate(&env) - 2.0).abs() < 1e-12);

        // At x=20 : a=40, b=30 → b < a → gradient = db = 1.0.
        env.insert("x".into(), 20.0);
        let g2 = scene_sdf_union_grad(&a, &b, &da, &db, &env);
        assert!((g2.evaluate(&env) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn intersect_grad_picks_max_branch() {
        let a = AnalyticExpr::mul(AnalyticExpr::v("x"), AnalyticExpr::c(2.0));
        let b = AnalyticExpr::add(AnalyticExpr::v("x"), AnalyticExpr::c(10.0));
        let da = AnalyticExpr::c(2.0);
        let db = AnalyticExpr::c(1.0);

        // At x=3 : a=6, b=13 → b > a → gradient = db = 1.0.
        let mut env = HashMap::new();
        env.insert("x".into(), 3.0);
        let g = scene_sdf_intersect_grad(&a, &b, &da, &db, &env);
        assert!((g.evaluate(&env) - 1.0).abs() < 1e-12);

        // At x=20 : a=40, b=30 → a > b → gradient = da = 2.0.
        env.insert("x".into(), 20.0);
        let g2 = scene_sdf_intersect_grad(&a, &b, &da, &db, &env);
        assert!((g2.evaluate(&env) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn scene_union_two_spheres_numerical_gradient_matches_winner() {
        // Scene = union(sphere(p - c_1, r_1), sphere(p - c_2, r_2))
        //   where c_1 = (0,0,0) r_1 = 2 ; c_2 = (10,0,0) r_2 = 3.
        // At sample p = (1, 0, 0) : sphere_1 = |p-c_1| - r_1 = 1 - 2 = -1 (inside).
        //                           sphere_2 = |p-c_2| - r_2 = 9 - 3 =  6 (outside).
        // union = min(-1, 6) = -1 → winner is sphere_1.
        // Analytic grad wrt p = normalize(p - c_1) = (1, 0, 0) (since p-c_1 = (1,0,0)).
        let p = AnalyticVec3Expr::v("p");
        let c1 = AnalyticVec3Expr::c(0.0, 0.0, 0.0);
        let c2 = AnalyticVec3Expr::c(10.0, 0.0, 0.0);
        let r1 = AnalyticExpr::c(2.0);
        let r2 = AnalyticExpr::c(3.0);

        let d1 = sphere_sdf_vec3(&AnalyticVec3Expr::sub(p.clone(), c1), &r1);
        let d2 = sphere_sdf_vec3(&AnalyticVec3Expr::sub(p, c2), &r2);

        let union = scene_sdf_union(d1.clone(), d2.clone());
        let env = env_with_p(1.0, 0.0, 0.0);

        // Primal : union should equal -1.
        assert!((union.evaluate(&env) - (-1.0)).abs() < 1e-10);

        // Gradient via scalar-wise approach : d(union)/d(p.x) picks sphere_1's.
        // At p=(1,0,0), sphere_1 grad wrt p.x = p.x / |p - c_1| = 1/1 = 1.0.
        // sphere_2 grad wrt p.x = (p.x - 10)/|p - c_2| = -9/9 = -1.0.
        let da_px = AnalyticExpr::c(1.0);
        let db_px = AnalyticExpr::c(-1.0);
        let g = scene_sdf_union_grad(&d1, &d2, &da_px, &db_px, &env);
        assert!((g.evaluate(&env) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn min_max_variants_evaluate_symmetrically() {
        let two = AnalyticExpr::c(2.0);
        let three = AnalyticExpr::c(3.0);
        let env = HashMap::new();
        let min_ab = AnalyticExpr::min(two.clone(), three.clone());
        let min_ba = AnalyticExpr::min(three.clone(), two.clone());
        assert!((min_ab.evaluate(&env) - 2.0).abs() < 1e-12);
        assert!((min_ba.evaluate(&env) - 2.0).abs() < 1e-12);
        let max_ab = AnalyticExpr::max(two.clone(), three.clone());
        let max_ba = AnalyticExpr::max(three, two);
        assert!((max_ab.evaluate(&env) - 3.0).abs() < 1e-12);
        assert!((max_ba.evaluate(&env) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn min_max_constant_folds_in_simplify() {
        // simplify should collapse `min(const, const)` into `const`.
        let e = AnalyticExpr::min(AnalyticExpr::c(5.0), AnalyticExpr::c(3.0));
        let s = e.simplify();
        assert_eq!(s, AnalyticExpr::c(3.0));
    }

    #[test]
    fn min_max_smt_emits_uninterpreted_form() {
        let e = AnalyticExpr::min(AnalyticExpr::v("x"), AnalyticExpr::c(0.0));
        assert!(e.to_smt().contains("min_uf"));
        let e2 = AnalyticExpr::max(AnalyticExpr::v("x"), AnalyticExpr::c(0.0));
        assert!(e2.to_smt().contains("max_uf"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Abs / Sign + smooth_min + cusp-detection.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn abs_evaluates_to_magnitude() {
        let env = HashMap::new();
        let e = AnalyticExpr::Abs(Box::new(AnalyticExpr::c(-5.0)));
        assert!((e.evaluate(&env) - 5.0).abs() < 1e-12);
        let e2 = AnalyticExpr::Abs(Box::new(AnalyticExpr::c(3.0)));
        assert!((e2.evaluate(&env) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn abs_constant_folds() {
        let e = AnalyticExpr::Abs(Box::new(AnalyticExpr::c(-7.0)));
        assert_eq!(e.simplify(), AnalyticExpr::c(7.0));
    }

    #[test]
    fn abs_smt_uses_abs_uf() {
        let e = AnalyticExpr::Abs(Box::new(AnalyticExpr::v("x")));
        assert!(e.to_smt().contains("abs_uf"));
    }

    #[test]
    fn sign_returns_minus_zero_plus() {
        let env = HashMap::new();
        assert!(
            (AnalyticExpr::Sign(Box::new(AnalyticExpr::c(5.0))).evaluate(&env) - 1.0).abs() < 1e-12
        );
        assert!(
            (AnalyticExpr::Sign(Box::new(AnalyticExpr::c(-3.0))).evaluate(&env) - (-1.0)).abs()
                < 1e-12
        );
        assert!(
            AnalyticExpr::Sign(Box::new(AnalyticExpr::c(0.0)))
                .evaluate(&env)
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn sign_constant_folds() {
        assert_eq!(
            AnalyticExpr::Sign(Box::new(AnalyticExpr::c(42.0))).simplify(),
            AnalyticExpr::c(1.0)
        );
        assert_eq!(
            AnalyticExpr::Sign(Box::new(AnalyticExpr::c(-17.5))).simplify(),
            AnalyticExpr::c(-1.0)
        );
        assert_eq!(
            AnalyticExpr::Sign(Box::new(AnalyticExpr::c(0.0))).simplify(),
            AnalyticExpr::c(0.0)
        );
    }

    #[test]
    fn sign_smt_uses_sign_uf() {
        let e = AnalyticExpr::Sign(Box::new(AnalyticExpr::v("x")));
        assert!(e.to_smt().contains("sign_uf"));
    }

    #[test]
    fn smooth_min_approaches_min_as_k_grows() {
        let env = HashMap::new();
        // At k=1 : smooth_min(3, 5, 1) ≈ somewhere between 3 and 5 (but closer to 3).
        // At k=100 : smooth_min(3, 5, 100) ≈ 3.0 to high precision.
        let sm_k1 = smooth_min(AnalyticExpr::c(3.0), AnalyticExpr::c(5.0), 1.0).evaluate(&env);
        let sm_k100 = smooth_min(AnalyticExpr::c(3.0), AnalyticExpr::c(5.0), 100.0).evaluate(&env);
        // Both should be ≤ min(3, 5) = 3 (smooth_min is lower bound).
        assert!(sm_k1 <= 3.0);
        assert!(sm_k100 <= 3.0);
        // Higher k should be closer to 3 than lower k.
        assert!((sm_k100 - 3.0).abs() < (sm_k1 - 3.0).abs());
        // k=100 should be very close to 3.
        assert!((sm_k100 - 3.0).abs() < 1e-3);
    }

    #[test]
    fn smooth_min_is_symmetric() {
        let env = HashMap::new();
        let sm_ab = smooth_min(AnalyticExpr::c(2.0), AnalyticExpr::c(7.0), 5.0).evaluate(&env);
        let sm_ba = smooth_min(AnalyticExpr::c(7.0), AnalyticExpr::c(2.0), 5.0).evaluate(&env);
        assert!((sm_ab - sm_ba).abs() < 1e-12);
    }

    #[test]
    fn smooth_min_central_diff_is_continuous_at_cusp() {
        // At a = b = 0, sharp min has a cusp but smooth_min is differentiable.
        // Verify via central-difference : ∂(smooth_min(x, 0, k))/∂x at x=0 should be finite.
        let sm = smooth_min(AnalyticExpr::v("x"), AnalyticExpr::c(0.0), 10.0);
        let mut env = HashMap::new();
        let h = 1e-6;
        env.insert("x".into(), h);
        let fp = sm.evaluate(&env);
        env.insert("x".into(), -h);
        let fm = sm.evaluate(&env);
        let numerical_dx = (fp - fm) / (2.0 * h);
        // At the cusp, the sharp-min subgradient is [0, 1] ; smooth_min picks 0.5
        // (symmetric contribution from both branches).
        assert!((numerical_dx - 0.5).abs() < 1e-3);
    }

    #[test]
    fn is_near_cusp_detects_close_values() {
        let env = HashMap::new();
        assert!(is_near_cusp(
            &AnalyticExpr::c(1.0),
            &AnalyticExpr::c(1.0001),
            &env,
            0.01
        ));
        assert!(!is_near_cusp(
            &AnalyticExpr::c(1.0),
            &AnalyticExpr::c(2.0),
            &env,
            0.01
        ));
    }

    #[test]
    fn is_near_cusp_treats_nan_as_near() {
        let env = HashMap::new();
        let nan = AnalyticExpr::Div(
            Box::new(AnalyticExpr::c(0.0)),
            Box::new(AnalyticExpr::c(0.0)),
        );
        assert!(is_near_cusp(&nan, &AnalyticExpr::c(1.0), &env, 0.01));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § smooth_max + n-ary folds.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn smooth_max_approaches_max_as_k_grows() {
        let env = HashMap::new();
        let sm_k1 = smooth_max(AnalyticExpr::c(3.0), AnalyticExpr::c(5.0), 1.0).evaluate(&env);
        let sm_k100 = smooth_max(AnalyticExpr::c(3.0), AnalyticExpr::c(5.0), 100.0).evaluate(&env);
        // smooth_max is upper bound on max.
        assert!(sm_k1 >= 5.0);
        assert!(sm_k100 >= 5.0);
        // Higher k is closer to 5.
        assert!((sm_k100 - 5.0).abs() < (sm_k1 - 5.0).abs());
        assert!((sm_k100 - 5.0).abs() < 1e-3);
    }

    #[test]
    fn smooth_max_is_negation_of_smooth_min_of_negations() {
        let env = HashMap::new();
        let a = AnalyticExpr::c(2.0);
        let b = AnalyticExpr::c(7.0);
        let k = 10.0;
        let sm_max = smooth_max(a.clone(), b.clone(), k).evaluate(&env);
        let neg_smin_neg =
            -smooth_min(AnalyticExpr::neg(a), AnalyticExpr::neg(b), k).evaluate(&env);
        assert!((sm_max - neg_smin_neg).abs() < 1e-12);
    }

    #[test]
    fn min_n_empty_returns_none() {
        let items: Vec<AnalyticExpr> = Vec::new();
        assert!(min_n(&items).is_none());
    }

    #[test]
    fn min_n_single_item_returns_self() {
        let items = vec![AnalyticExpr::c(42.0)];
        let folded = min_n(&items).unwrap();
        let env = HashMap::new();
        assert!((folded.evaluate(&env) - 42.0).abs() < 1e-12);
    }

    #[test]
    fn min_n_three_items_picks_smallest() {
        let items = vec![
            AnalyticExpr::c(5.0),
            AnalyticExpr::c(2.0),
            AnalyticExpr::c(8.0),
        ];
        let folded = min_n(&items).unwrap();
        let env = HashMap::new();
        assert!((folded.evaluate(&env) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn max_n_three_items_picks_largest() {
        let items = vec![
            AnalyticExpr::c(5.0),
            AnalyticExpr::c(2.0),
            AnalyticExpr::c(8.0),
        ];
        let folded = max_n(&items).unwrap();
        let env = HashMap::new();
        assert!((folded.evaluate(&env) - 8.0).abs() < 1e-12);
    }

    #[test]
    fn smooth_min_n_four_items_converges_to_min_at_high_k() {
        let items = vec![
            AnalyticExpr::c(3.0),
            AnalyticExpr::c(7.0),
            AnalyticExpr::c(1.5),
            AnalyticExpr::c(5.0),
        ];
        let folded = smooth_min_n(&items, 50.0).unwrap();
        let env = HashMap::new();
        assert!((folded.evaluate(&env) - 1.5).abs() < 1e-2);
    }

    #[test]
    fn smooth_max_n_four_items_converges_to_max_at_high_k() {
        let items = vec![
            AnalyticExpr::c(3.0),
            AnalyticExpr::c(7.0),
            AnalyticExpr::c(1.5),
            AnalyticExpr::c(5.0),
        ];
        let folded = smooth_max_n(&items, 50.0).unwrap();
        let env = HashMap::new();
        assert!((folded.evaluate(&env) - 7.0).abs() < 1e-2);
    }

    #[test]
    fn smooth_min_n_single_is_item_itself() {
        let items = vec![AnalyticExpr::c(42.0)];
        let folded = smooth_min_n(&items, 10.0).unwrap();
        let env = HashMap::new();
        assert!((folded.evaluate(&env) - 42.0).abs() < 1e-12);
    }
}
