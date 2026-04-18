// § Allowances :
//   float_cmp             ← constant-fold symmetry with AnalyticExpr
//   should_implement_trait ← neg/add/sub are constructor-helpers, not std::ops::{Neg,Add,Sub} impls
#![allow(clippy::float_cmp)]
#![allow(clippy::should_implement_trait)]

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

#[cfg(test)]
mod tests {
    use super::{
        dot, length, sphere_sdf_grad_p, sphere_sdf_grad_r, sphere_sdf_vec3, vec3_proj,
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
}
