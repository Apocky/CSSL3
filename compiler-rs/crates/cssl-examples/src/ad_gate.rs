// § Module-level allowances for the `AnalyticExpr` algebra :
//   * `clippy::float_cmp` : exact-0.0/1.0 comparison is precisely how a
//     constant-fold simplifier recognizes neutral elements — approximate
//     comparison would break simplification idempotence.
//   * `clippy::should_implement_trait` : `neg` / `add` / `sub` / `mul` / `div`
//     on [`AnalyticExpr`] are constructor helpers that read more naturally
//     than the `Neg` / `Add` / etc. trait form for building trees by hand.
//   * `clippy::cast_precision_loss` : the `usize → f64` casts in sample-env
//     generation are deliberately low-resolution (single-digit indices).
//   * `clippy::redundant_closure` / `clippy::useless_format` / `clippy::needless_pass_by_value`
//     / `clippy::redundant_clone` / `clippy::single_char_pattern`
//     / `clippy::map_unwrap_or` — pragmatic allowances in the test-fixture
//     builders + SMT-text formatter ; tightening them would bloat the code
//     without improving correctness.
#![allow(clippy::float_cmp)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::useless_format)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::redundant_closure_for_method_calls)]

//! T7-phase-2c KILLER-APP GATE — AD gradient equivalence verifier.
//!
//! § SPEC : `specs/05_AUTODIFF.csl` § SDF-NORMAL (killer-app) + § INTEGRATIONS.
//!         `HANDOFF_SESSION_2.csl` § GATES § F1-correctness-gate.
//!
//! § GOAL
//!   Prove that for every canonical scalar primal function `f(x₁, ..., xₙ)`, the
//!   reverse-mode AD-generated gradient `∇_MIR f` emitted by
//!   [`cssl_autodiff::apply_bwd`] is **semantically equivalent** to the
//!   handwritten analytic gradient `∇_analytic f`. This is the PUBLISHABLE
//!   F1-correctness claim : any third-party auditor can reproduce the gate
//!   from the source + this crate and observe the match.
//!
//! § APPROACH
//!   1. Build a MIR primal fn (directly, bypassing lex/parse — the tests are
//!      independent of the surface-syntax state).
//!   2. Run `apply_bwd` to produce the real adjoint-carrying MIR body + the
//!      `cssl.diff.bwd_return` terminator whose operands ARE the gradient
//!      SSA values (post-accumulation).
//!   3. Walk the bwd body with [`MirAdjointInterpreter`] to reconstruct an
//!      [`AnalyticExpr`] tree for each adjoint-out operand.
//!   4. Compare against a handwritten `∇_analytic` [`AnalyticExpr`] via :
//!      - symbolic simplification (constant-fold, neutral-element elimination)
//!      - sampling-based numeric evaluation across a deterministic test-range
//!   5. Report a [`GradientCase`] with per-param match verdicts.
//!
//! § WHY NOT SMT (yet)
//!   The Z3 / CVC5 integration lives in `cssl_smt`, but binding MIR-adjoint ops
//!   to SMT-LIB expressions requires a HIR-direct translator (T9-phase-2c).
//!   This phase-2c slice closes the **structural** equivalence claim ; bit-
//!   exact runtime float equivalence (per §§ 23 TESTING differential-backend)
//!   is a CI-side concern that composes on top of this module.
//!
//! § WHAT THIS GATES
//!   ✓ FAdd / FSub / FMul / FDiv / FNeg scalar gradient correctness
//!   ✓ Sqrt / Sin / Cos / Exp / Log chain-rule correctness
//!   ✓ Sphere-SDF scalar surrogate `sphere_sdf(p, r) = p - r`
//!   ✓ Composed chain (e.g., `f(x, r) = (x - r) * (x - r)` → ∇_x = 2(x-r))
//!   ○ Vector-SDF `length(p) - r` (requires T6 vec-op lowering ; phase-2d)
//!   ○ Scene-SDF union / min (requires monomorphization ; phase-2d)
//!   ○ Runtime bit-exact float comparison (CI-side ; composes with R18 audit-chain)

use std::collections::HashMap;

use cssl_autodiff::{apply_bwd, DiffRuleTable};
use cssl_mir::{FloatWidth, MirFunc, MirOp, MirType, ValueId};

// ─────────────────────────────────────────────────────────────────────────
// § Symbolic expression algebra for gradients.
// ─────────────────────────────────────────────────────────────────────────

/// Symbolic expression tree for representing primal-fn bodies and their
/// gradients. Stage-0 scalar-only ; vector / tensor extension is phase-2d.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalyticExpr {
    /// Numeric literal.
    Const(f64),
    /// Named variable — typically a primal param (e.g., "x") or the seed
    /// adjoint ("d_y").
    Var(String),
    /// Unary negation : `-a`.
    Neg(Box<AnalyticExpr>),
    /// Binary sum : `a + b`.
    Add(Box<AnalyticExpr>, Box<AnalyticExpr>),
    /// Binary difference : `a - b`.
    Sub(Box<AnalyticExpr>, Box<AnalyticExpr>),
    /// Binary product : `a * b`.
    Mul(Box<AnalyticExpr>, Box<AnalyticExpr>),
    /// Binary division : `a / b`.
    Div(Box<AnalyticExpr>, Box<AnalyticExpr>),
    /// Square-root : `√a`.
    Sqrt(Box<AnalyticExpr>),
    /// Sine : `sin(a)`.
    Sin(Box<AnalyticExpr>),
    /// Cosine : `cos(a)`.
    Cos(Box<AnalyticExpr>),
    /// Exponential : `exp(a)`.
    Exp(Box<AnalyticExpr>),
    /// Natural log : `log(a)`.
    Log(Box<AnalyticExpr>),
    /// Uninterpreted function-call (for unrecognized ops). Carries the callee
    /// name and arg list. Evaluation falls back to NaN — these branches don't
    /// pass gradient-equivalence checks.
    Uninterpreted(String, Vec<AnalyticExpr>),
}

impl AnalyticExpr {
    /// Numeric constant.
    #[must_use]
    pub fn c(v: f64) -> Self {
        Self::Const(v)
    }

    /// Named variable.
    #[must_use]
    pub fn v(name: impl Into<String>) -> Self {
        Self::Var(name.into())
    }

    /// `-a`
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

    /// `a * b`
    #[must_use]
    pub fn mul(a: Self, b: Self) -> Self {
        Self::Mul(Box::new(a), Box::new(b))
    }

    /// `a / b`
    #[must_use]
    pub fn div(a: Self, b: Self) -> Self {
        Self::Div(Box::new(a), Box::new(b))
    }

    /// Recursively apply algebraic simplifications :
    ///   - `const op const` → const
    ///   - `0 + x` / `x + 0` → x
    ///   - `x - 0` → x
    ///   - `0 - x` → -x
    ///   - `1 * x` / `x * 1` → x
    ///   - `0 * x` / `x * 0` → 0
    ///   - `x / 1` → x
    ///   - `-(-x)` → x
    ///   - `-(const)` → -const
    #[must_use]
    pub fn simplify(&self) -> Self {
        match self {
            Self::Const(_) | Self::Var(_) => self.clone(),
            Self::Neg(inner) => {
                let s = inner.simplify();
                match s {
                    Self::Const(c) => Self::Const(-c),
                    Self::Neg(x) => *x,
                    other => Self::Neg(Box::new(other)),
                }
            }
            Self::Add(a, b) => {
                let sa = a.simplify();
                let sb = b.simplify();
                if let Self::Const(x) = sa {
                    if x == 0.0 {
                        return sb;
                    }
                    if let Self::Const(y) = sb {
                        return Self::Const(x + y);
                    }
                    return Self::Add(Box::new(Self::Const(x)), Box::new(sb));
                }
                if let Self::Const(y) = sb {
                    if y == 0.0 {
                        return sa;
                    }
                }
                Self::Add(Box::new(sa), Box::new(sb))
            }
            Self::Sub(a, b) => {
                let sa = a.simplify();
                let sb = b.simplify();
                if let Self::Const(y) = sb {
                    if y == 0.0 {
                        return sa;
                    }
                    if let Self::Const(x) = sa {
                        return Self::Const(x - y);
                    }
                }
                if let Self::Const(x) = sa {
                    if x == 0.0 {
                        return Self::Neg(Box::new(sb)).simplify();
                    }
                    return Self::Sub(Box::new(Self::Const(x)), Box::new(sb));
                }
                Self::Sub(Box::new(sa), Box::new(sb))
            }
            Self::Mul(a, b) => {
                let sa = a.simplify();
                let sb = b.simplify();
                if let Self::Const(x) = sa {
                    if x == 0.0 {
                        return Self::Const(0.0);
                    }
                    if x == 1.0 {
                        return sb;
                    }
                    if let Self::Const(y) = sb {
                        return Self::Const(x * y);
                    }
                    return Self::Mul(Box::new(Self::Const(x)), Box::new(sb));
                }
                if let Self::Const(y) = sb {
                    if y == 0.0 {
                        return Self::Const(0.0);
                    }
                    if y == 1.0 {
                        return sa;
                    }
                }
                Self::Mul(Box::new(sa), Box::new(sb))
            }
            Self::Div(a, b) => {
                let sa = a.simplify();
                let sb = b.simplify();
                if let Self::Const(y) = sb {
                    if y == 1.0 {
                        return sa;
                    }
                    if y != 0.0 {
                        if let Self::Const(x) = sa {
                            return Self::Const(x / y);
                        }
                    }
                }
                Self::Div(Box::new(sa), Box::new(sb))
            }
            Self::Sqrt(x) => Self::Sqrt(Box::new(x.simplify())),
            Self::Sin(x) => Self::Sin(Box::new(x.simplify())),
            Self::Cos(x) => Self::Cos(Box::new(x.simplify())),
            Self::Exp(x) => Self::Exp(Box::new(x.simplify())),
            Self::Log(x) => Self::Log(Box::new(x.simplify())),
            Self::Uninterpreted(name, args) => {
                Self::Uninterpreted(name.clone(), args.iter().map(Self::simplify).collect())
            }
        }
    }

    /// Numerically evaluate the expression under a named-variable environment.
    ///
    /// Returns `f64::NAN` if an [`AnalyticExpr::Uninterpreted`] node is
    /// encountered or a variable is missing from `env`.
    #[must_use]
    pub fn evaluate(&self, env: &HashMap<String, f64>) -> f64 {
        match self {
            Self::Const(v) => *v,
            Self::Var(name) => env.get(name).copied().unwrap_or(f64::NAN),
            Self::Neg(a) => -a.evaluate(env),
            Self::Add(a, b) => a.evaluate(env) + b.evaluate(env),
            Self::Sub(a, b) => a.evaluate(env) - b.evaluate(env),
            Self::Mul(a, b) => a.evaluate(env) * b.evaluate(env),
            Self::Div(a, b) => a.evaluate(env) / b.evaluate(env),
            Self::Sqrt(a) => a.evaluate(env).sqrt(),
            Self::Sin(a) => a.evaluate(env).sin(),
            Self::Cos(a) => a.evaluate(env).cos(),
            Self::Exp(a) => a.evaluate(env).exp(),
            Self::Log(a) => a.evaluate(env).ln(),
            Self::Uninterpreted(_, _) => f64::NAN,
        }
    }

    /// Check equivalence with `other` by evaluating both over a set of
    /// deterministic sample environments and comparing within `tolerance`.
    /// Returns `true` iff every **defined** sample matches AND at least one
    /// sample produced a finite comparison.
    ///
    /// § Why sampling-based ?
    ///   Stage-0 doesn't embed a full symbolic CAS. Two gradient expressions
    ///   can be structurally different yet mathematically equal (e.g., `a*b`
    ///   vs `b*a`, or `x / x` vs `1`). Sampling across a wide range detects
    ///   mismatches with high probability ; for the scalar primitive rules we
    ///   care about, the default 11-point sampling is sufficient.
    ///
    /// § NaN / Inf handling
    ///   - Both sides produce NaN (e.g. sqrt of negative input) → sample is
    ///     *inconclusive* (skipped ; not counted as match or mismatch).
    ///   - Exactly one side NaN → mismatch (indicates domain disagreement).
    ///   - Both sides ±Inf with the same sign → match.
    ///   - Finite values → match iff `|a - b| ≤ tolerance`.
    ///
    ///   At least one sample MUST produce a finite match — an all-NaN result
    ///   returns `false` to avoid falsely declaring equivalence for
    ///   ill-defined expressions.
    #[must_use]
    pub fn equivalent_by_sampling(
        &self,
        other: &Self,
        samples: &[HashMap<String, f64>],
        tolerance: f64,
    ) -> bool {
        let mut conclusive_matches = 0_u32;
        for env in samples {
            let a = self.evaluate(env);
            let b = other.evaluate(env);
            if a.is_nan() && b.is_nan() {
                continue; // inconclusive — both undefined at this sample
            }
            if a.is_nan() || b.is_nan() {
                return false; // domain disagreement
            }
            if !a.is_finite() || !b.is_finite() {
                if a != b {
                    return false;
                }
                conclusive_matches += 1;
                continue;
            }
            if (a - b).abs() > tolerance {
                return false;
            }
            conclusive_matches += 1;
        }
        conclusive_matches > 0
    }

    /// Emit the expression in SMT-LIB real-arithmetic syntax (Z3 / CVC5
    /// compatible). Stretch path : compose with `cssl_smt::Query` for a Z3
    /// unsat-verdict equivalence proof. Used by [`GradientCase::smt_query_text`].
    #[must_use]
    pub fn to_smt(&self) -> String {
        match self {
            Self::Const(v) => format_smt_real(*v),
            Self::Var(name) => name.clone(),
            Self::Neg(a) => format!("(- {})", a.to_smt()),
            Self::Add(a, b) => format!("(+ {} {})", a.to_smt(), b.to_smt()),
            Self::Sub(a, b) => format!("(- {} {})", a.to_smt(), b.to_smt()),
            Self::Mul(a, b) => format!("(* {} {})", a.to_smt(), b.to_smt()),
            Self::Div(a, b) => format!("(/ {} {})", a.to_smt(), b.to_smt()),
            Self::Sqrt(a) => format!("(sqrt_uf {})", a.to_smt()),
            Self::Sin(a) => format!("(sin_uf {})", a.to_smt()),
            Self::Cos(a) => format!("(cos_uf {})", a.to_smt()),
            Self::Exp(a) => format!("(exp_uf {})", a.to_smt()),
            Self::Log(a) => format!("(log_uf {})", a.to_smt()),
            Self::Uninterpreted(name, args) => {
                if args.is_empty() {
                    name.clone()
                } else {
                    let mut s = format!("({name}");
                    for a in args {
                        s.push(' ');
                        s.push_str(&a.to_smt());
                    }
                    s.push(')');
                    s
                }
            }
        }
    }

    /// Collect every distinct `Var` name in the expression tree.
    #[must_use]
    pub fn free_vars(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_vars(&mut out);
        out.sort();
        out.dedup();
        out
    }

    fn collect_vars(&self, out: &mut Vec<String>) {
        match self {
            Self::Const(_) => {}
            Self::Var(n) => out.push(n.clone()),
            Self::Neg(a)
            | Self::Sqrt(a)
            | Self::Sin(a)
            | Self::Cos(a)
            | Self::Exp(a)
            | Self::Log(a) => a.collect_vars(out),
            Self::Add(a, b) | Self::Sub(a, b) | Self::Mul(a, b) | Self::Div(a, b) => {
                a.collect_vars(out);
                b.collect_vars(out);
            }
            Self::Uninterpreted(_, args) => {
                for a in args {
                    a.collect_vars(out);
                }
            }
        }
    }
}

/// Emit an `f64` in Z3-compatible `(/ num den)` or `decimal` form.
fn format_smt_real(v: f64) -> String {
    if v.is_nan() {
        "nan".to_string()
    } else if v == v.trunc() && v.abs() < 1e15 {
        // Integer-valued — emit as integer literal (`1.0` etc.).
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § MIR-adjoint interpreter : bwd-variant MIR → AnalyticExpr for each
//   bwd_return operand.
// ─────────────────────────────────────────────────────────────────────────

/// Walks the body of a reverse-mode MIR variant (produced by
/// [`cssl_autodiff::apply_bwd`]) and reconstructs an [`AnalyticExpr`] for
/// every operand of the terminal `cssl.diff.bwd_return` op.
///
/// The interpreter maintains two parallel symbol tables :
///
///   * `primal_exprs` — maps primal SSA values to their symbolic form. Seeded
///     with `Var(param_name)` for each primal param. Updated as the walk
///     encounters ops with `diff_role != "adjoint"` (i.e., the preserved
///     primal computation).
///   * `adjoint_exprs` — maps adjoint SSA values to their symbolic form.
///     Seeded with `Var("d_y")` for the adjoint-in param. Updated as the
///     walk encounters ops with `diff_role == "adjoint"` — including the
///     `arith.constant 0.0 → %zero_d_*` zero-init ops emitted at bwd-start
///     (those are mapped to `Const(0.0)` before the reverse walk begins).
pub struct MirAdjointInterpreter<'a> {
    /// The reverse-mode variant being interpreted.
    pub bwd_fn: &'a MirFunc,
    /// Names for the original primal-params in positional order.
    pub primal_param_names: Vec<String>,
    /// Tolerance for sampling-based equivalence checks that downstream
    /// [`verify_gradient_case`] performs.
    pub tolerance: f64,
    primal_exprs: HashMap<ValueId, AnalyticExpr>,
    adjoint_exprs: HashMap<ValueId, AnalyticExpr>,
}

impl<'a> MirAdjointInterpreter<'a> {
    /// Build an interpreter over `bwd_fn` where the first `primal_param_names.len()`
    /// entry-block args correspond to the listed primal param names.
    #[must_use]
    pub fn new(bwd_fn: &'a MirFunc, primal_param_names: Vec<String>) -> Self {
        let mut me = Self {
            bwd_fn,
            primal_param_names: primal_param_names.clone(),
            tolerance: 1e-9,
            primal_exprs: HashMap::new(),
            adjoint_exprs: HashMap::new(),
        };
        me.seed_params();
        me
    }

    fn seed_params(&mut self) {
        if let Some(entry) = self.bwd_fn.body.entry() {
            let n = self.primal_param_names.len();
            for (i, arg) in entry.args.iter().enumerate() {
                if i < n {
                    self.primal_exprs.insert(
                        arg.id,
                        AnalyticExpr::Var(self.primal_param_names[i].clone()),
                    );
                } else {
                    // Trailing adjoint-in param(s) — seed as Var("d_y") + "d_y_1", etc.
                    let name = if i == n {
                        "d_y".to_string()
                    } else {
                        format!("d_y_{}", i - n)
                    };
                    self.adjoint_exprs.insert(arg.id, AnalyticExpr::Var(name));
                }
            }
        }
    }

    /// Interpret the bwd-variant body and return one [`AnalyticExpr`] per
    /// `cssl.diff.bwd_return` operand, in order. The list is empty if the
    /// terminator is missing.
    pub fn compute_adjoint_outs(&mut self) -> Vec<AnalyticExpr> {
        let Some(entry) = self.bwd_fn.body.entry() else {
            return Vec::new();
        };
        let mut return_operands: Vec<ValueId> = Vec::new();
        for op in &entry.ops {
            if op.name == "cssl.diff.bwd_return" {
                return_operands = op.operands.clone();
                continue;
            }
            self.interpret_op(op);
        }
        return_operands
            .iter()
            .map(|id| self.resolve_adjoint(*id).simplify())
            .collect()
    }

    /// Dispatch a single MIR op into the matching expression-table update.
    fn interpret_op(&mut self, op: &MirOp) {
        let is_adjoint = op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "adjoint");
        let Some(first_result) = op.results.first() else {
            return;
        };
        let rid = first_result.id;
        let expr = self.compute_op_expr(op, is_adjoint);
        if is_adjoint {
            self.adjoint_exprs.insert(rid, expr);
        } else {
            self.primal_exprs.insert(rid, expr);
        }
    }

    fn compute_op_expr(&self, op: &MirOp, is_adjoint: bool) -> AnalyticExpr {
        match op.name.as_str() {
            "arith.constant" => {
                let v = op
                    .attributes
                    .iter()
                    .find(|(k, _)| k == "value")
                    .and_then(|(_, v)| parse_const_value(v))
                    .unwrap_or(0.0);
                AnalyticExpr::Const(v)
            }
            "arith.addf" => {
                let a = self.resolve_operand(op.operands.first().copied(), is_adjoint);
                let b = self.resolve_operand(op.operands.get(1).copied(), is_adjoint);
                AnalyticExpr::add(a, b)
            }
            "arith.subf" => {
                let a = self.resolve_operand(op.operands.first().copied(), is_adjoint);
                let b = self.resolve_operand(op.operands.get(1).copied(), is_adjoint);
                AnalyticExpr::sub(a, b)
            }
            "arith.mulf" => {
                let a = self.resolve_operand(op.operands.first().copied(), is_adjoint);
                let b = self.resolve_operand(op.operands.get(1).copied(), is_adjoint);
                AnalyticExpr::mul(a, b)
            }
            "arith.divf" => {
                let a = self.resolve_operand(op.operands.first().copied(), is_adjoint);
                let b = self.resolve_operand(op.operands.get(1).copied(), is_adjoint);
                AnalyticExpr::div(a, b)
            }
            "arith.negf" => {
                let a = self.resolve_operand(op.operands.first().copied(), is_adjoint);
                AnalyticExpr::neg(a)
            }
            "func.call" => {
                let callee = op
                    .attributes
                    .iter()
                    .find(|(k, _)| k == "callee")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("?");
                let a = self.resolve_operand(op.operands.first().copied(), is_adjoint);
                match callee {
                    "sqrt" => AnalyticExpr::Sqrt(Box::new(a)),
                    "sin" => AnalyticExpr::Sin(Box::new(a)),
                    "cos" => AnalyticExpr::Cos(Box::new(a)),
                    "exp" => AnalyticExpr::Exp(Box::new(a)),
                    "log" | "ln" => AnalyticExpr::Log(Box::new(a)),
                    other => AnalyticExpr::Uninterpreted(other.to_string(), vec![a]),
                }
            }
            "func.return" => AnalyticExpr::Uninterpreted("return".to_string(), Vec::new()),
            other => AnalyticExpr::Uninterpreted(other.to_string(), Vec::new()),
        }
    }

    /// Resolve a single operand in the current op's context :
    ///   - Adjoint op → `adjoint_exprs[id]` primary ; fall back to
    ///     `primal_exprs[id]` (for primal-value references like `b` in
    ///     `contrib_a = d_y * b`).
    ///   - Primal op → `primal_exprs[id]` only.
    fn resolve_operand(&self, id: Option<ValueId>, is_adjoint: bool) -> AnalyticExpr {
        let Some(id) = id else {
            return AnalyticExpr::Uninterpreted(format!("?missing_operand"), Vec::new());
        };
        if is_adjoint {
            if let Some(e) = self.adjoint_exprs.get(&id) {
                return e.clone();
            }
            if let Some(e) = self.primal_exprs.get(&id) {
                return e.clone();
            }
            AnalyticExpr::Uninterpreted(format!("?v{}", id.0), Vec::new())
        } else {
            self.primal_exprs
                .get(&id)
                .cloned()
                .unwrap_or_else(|| AnalyticExpr::Uninterpreted(format!("?v{}", id.0), Vec::new()))
        }
    }

    fn resolve_adjoint(&self, id: ValueId) -> AnalyticExpr {
        self.adjoint_exprs
            .get(&id)
            .cloned()
            .unwrap_or_else(|| AnalyticExpr::Uninterpreted(format!("?a{}", id.0), Vec::new()))
    }
}

/// Parse the `value` attribute of an `arith.constant` op. Returns `None` for
/// unrecognized forms (including the phase-2a `"stage0_int"` / `"stage0_float"`
/// placeholders, which cleanly surface as unresolved at gate-time).
fn parse_const_value(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed == "stage0_int" || trimmed == "stage0_float" {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

// ─────────────────────────────────────────────────────────────────────────
// § Gradient-equivalence verifier.
// ─────────────────────────────────────────────────────────────────────────

/// Per-param equivalence result.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamCheck {
    /// Primal param name (e.g., `"x"`).
    pub name: String,
    /// Handwritten analytic gradient expression.
    pub analytic: AnalyticExpr,
    /// MIR-derived gradient expression (post-simplify).
    pub mir_derived: AnalyticExpr,
    /// `true` iff the two are equivalent over the sample set.
    pub matches: bool,
}

/// Top-level gradient-equivalence result for one `(primal, analytic-grad)` pair.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientCase {
    /// Human-readable case name (e.g., `"f(x,r) = x * r"`).
    pub name: String,
    /// Primal param names in positional order.
    pub param_names: Vec<String>,
    /// Per-param equivalence verdicts.
    pub params: Vec<ParamCheck>,
    /// `true` iff every param matches.
    pub all_match: bool,
}

impl GradientCase {
    /// Short summary line.
    #[must_use]
    pub fn summary(&self) -> String {
        let matched = self.params.iter().filter(|p| p.matches).count();
        format!(
            "{} : {}/{} gradient-component(s) match{}",
            self.name,
            matched,
            self.params.len(),
            if self.all_match { " ✓" } else { " ✗" }
        )
    }

    /// SMT-LIB equivalence query expressing `∀ vars : mir_derived == analytic`.
    /// Z3 / CVC5 should return `unsat` for the NEGATION if the gradients are
    /// equivalent. Stretch-path : wire this through `cssl_smt::Query` and a
    /// real solver binary.
    ///
    /// Always declares every primal param + the adjoint seed `d_y`, even when
    /// the gradient simplifies to a form that doesn't reference them — this
    /// keeps the SMT query shape stable across cases.
    #[must_use]
    pub fn smt_query_text(&self) -> String {
        let mut vars: Vec<String> = self.param_names.clone();
        vars.push("d_y".to_string());
        for p in &self.params {
            for v in p.analytic.free_vars() {
                vars.push(v);
            }
            for v in p.mir_derived.free_vars() {
                vars.push(v);
            }
        }
        vars.sort();
        vars.dedup();
        let mut out = String::from("(set-logic QF_UFNRA)\n");
        for v in &vars {
            out.push_str(&format!("(declare-fun {v} () Real)\n"));
        }
        // Uninterpreted transcendental fns (for sqrt/sin/cos/exp/log).
        out.push_str("(declare-fun sqrt_uf (Real) Real)\n");
        out.push_str("(declare-fun sin_uf (Real) Real)\n");
        out.push_str("(declare-fun cos_uf (Real) Real)\n");
        out.push_str("(declare-fun exp_uf (Real) Real)\n");
        out.push_str("(declare-fun log_uf (Real) Real)\n");
        // Negation of the equivalence claim — Z3 `unsat` proves equivalence.
        out.push_str("(assert (not (and\n");
        for p in &self.params {
            out.push_str(&format!(
                "  (= {mir} {ana})\n",
                mir = p.mir_derived.to_smt(),
                ana = p.analytic.to_smt()
            ));
        }
        out.push_str(")))\n(check-sat)\n");
        out
    }
}

/// Verify that the reverse-mode AD-generated gradient of `primal` matches the
/// provided analytic gradients, one entry per primal param.
///
/// `primal.params.len() == analytic_gradients.len()` must hold ; each
/// `analytic_gradients[i]` is the handwritten `∂(primal)/∂(param_i)`,
/// expressed symbolically in terms of the primal param names + the seed
/// adjoint `d_y`.
///
/// The verifier :
///   1. Runs `apply_bwd` on `primal` to get the reverse-mode variant.
///   2. Walks its body via [`MirAdjointInterpreter`] to reconstruct each
///      adjoint-out's [`AnalyticExpr`].
///   3. Compares analytic vs MIR-derived by sampling across a deterministic
///      set of input environments.
///
/// Returns a [`GradientCase`] with per-param verdicts.
#[must_use]
pub fn verify_gradient_case(
    name: &str,
    primal: &MirFunc,
    param_names: Vec<String>,
    analytic_gradients: Vec<AnalyticExpr>,
) -> GradientCase {
    assert_eq!(
        param_names.len(),
        analytic_gradients.len(),
        "one analytic gradient per primal param required"
    );
    let (bwd_variant, _, _) = apply_bwd(primal, &DiffRuleTable::canonical());
    let mut interp = MirAdjointInterpreter::new(&bwd_variant, param_names.clone());
    let mir_grads = interp.compute_adjoint_outs();
    // Pad if MIR produced fewer operands than expected (e.g., a non-float param).
    let samples = default_samples(&param_names);
    let mut params = Vec::with_capacity(param_names.len());
    for (i, analytic) in analytic_gradients.iter().enumerate() {
        let mir = mir_grads.get(i).cloned().unwrap_or_else(|| {
            AnalyticExpr::Uninterpreted("?missing_mir_gradient".to_string(), Vec::new())
        });
        let analytic_simple = analytic.simplify();
        let mir_simple = mir.simplify();
        let matches = analytic_simple.equivalent_by_sampling(&mir_simple, &samples, 1e-9);
        params.push(ParamCheck {
            name: param_names[i].clone(),
            analytic: analytic_simple,
            mir_derived: mir_simple,
            matches,
        });
    }
    let all_match = params.iter().all(|p| p.matches);
    GradientCase {
        name: name.to_string(),
        param_names,
        params,
        all_match,
    }
}

/// Deterministic sample environments for equivalence testing. Covers 16 point
/// configurations in `[-3.0, 3.0]` avoiding 0 for division-safe numerics, plus
/// a canonical `d_y = 1.0` seed (gradient scales linearly in the adjoint, so
/// any non-zero seed suffices for equivalence).
fn default_samples(param_names: &[String]) -> Vec<HashMap<String, f64>> {
    let values = [-2.7, -1.5, -0.3, 0.7, 1.1, 1.9, 2.5, 0.01, -0.01, 3.0, -3.0];
    let mut out = Vec::new();
    for (i, v) in values.iter().enumerate() {
        let mut env = HashMap::new();
        for (j, p) in param_names.iter().enumerate() {
            // Spread values so each param gets a different number per sample.
            let offset = j as f64 * 0.37;
            env.insert(p.clone(), *v + offset);
        }
        // Seed adjoint — vary sign every other sample to catch sign-errors.
        let seed = if i % 2 == 0 { 1.0 } else { -1.0 };
        env.insert("d_y".to_string(), seed);
        out.push(env);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// § Primal-fn builders — test fixtures.
// ─────────────────────────────────────────────────────────────────────────

/// Helper : build a single-primitive-scalar primal fn whose body computes
/// `op(param_0, param_1) → ret`. Used by the per-primitive tests.
fn build_binary_primal(name: &str, op_name: &str) -> MirFunc {
    let f32 = MirType::Float(FloatWidth::F32);
    let mut f = MirFunc::new(name, vec![f32.clone(), f32.clone()], vec![f32.clone()]);
    let result_id = f.fresh_value_id();
    f.push_op(
        MirOp::std(op_name)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(result_id, f32.clone()),
    );
    f.push_op(MirOp::std("func.return").with_operand(result_id));
    f
}

fn build_unary_primal(name: &str, op_name: &str) -> MirFunc {
    let f32 = MirType::Float(FloatWidth::F32);
    let mut f = MirFunc::new(name, vec![f32.clone()], vec![f32.clone()]);
    let result_id = f.fresh_value_id();
    f.push_op(
        MirOp::std(op_name)
            .with_operand(ValueId(0))
            .with_result(result_id, f32.clone()),
    );
    f.push_op(MirOp::std("func.return").with_operand(result_id));
    f
}

fn build_transcendental_primal(name: &str, callee: &str) -> MirFunc {
    let f32 = MirType::Float(FloatWidth::F32);
    let mut f = MirFunc::new(name, vec![f32.clone()], vec![f32.clone()]);
    let result_id = f.fresh_value_id();
    f.push_op(
        MirOp::std("func.call")
            .with_operand(ValueId(0))
            .with_result(result_id, f32.clone())
            .with_attribute("callee", callee),
    );
    f.push_op(MirOp::std("func.return").with_operand(result_id));
    f
}

/// Build `f(x, r) = (x - r) * (x - r)` — the chain-rule gate exercise.
/// Analytic : `∂f/∂x = 2(x - r)`, `∂f/∂r = -2(x - r)`.
fn build_chain_primal() -> MirFunc {
    let f32 = MirType::Float(FloatWidth::F32);
    let mut f = MirFunc::new("chain", vec![f32.clone(), f32.clone()], vec![f32.clone()]);
    let d1 = f.fresh_value_id(); // x - r
    let d2 = f.fresh_value_id(); // (x - r) * (x - r)
    f.push_op(
        MirOp::std("arith.subf")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(d1, f32.clone()),
    );
    f.push_op(
        MirOp::std("arith.mulf")
            .with_operand(d1)
            .with_operand(d1)
            .with_result(d2, f32.clone()),
    );
    f.push_op(MirOp::std("func.return").with_operand(d2));
    f
}

// ─────────────────────────────────────────────────────────────────────────
// § Killer-app gate entry-point + report.
// ─────────────────────────────────────────────────────────────────────────

/// Top-level gate report — rolls up every canonical gradient case.
#[derive(Debug, Clone, PartialEq)]
pub struct KillerAppGateReport {
    /// Every verified case in declaration order.
    pub cases: Vec<GradientCase>,
    /// Total cases.
    pub total: usize,
    /// Cases with full-match ✓.
    pub passing: usize,
}

impl KillerAppGateReport {
    /// Summary line.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "T7-phase-2c KILLER-APP GATE : {}/{} cases pass {}",
            self.passing,
            self.total,
            if self.passing == self.total {
                "✓"
            } else {
                "✗"
            }
        )
    }

    /// `true` iff every case passed full-match.
    #[must_use]
    pub fn is_green(&self) -> bool {
        self.passing == self.total
    }
}

/// Run the canonical T7-phase-2c KILLER-APP GATE covering every scalar
/// primitive + sphere-sdf surrogate + chain-rule exercise.
///
/// Returns a [`KillerAppGateReport`] ; callers (CI drivers, tests, attestation
/// pipelines) check [`KillerAppGateReport::is_green`] for the acceptance verdict.
#[must_use]
pub fn run_killer_app_gate() -> KillerAppGateReport {
    let cases = vec![
        // § FAdd : f(x, r) = x + r  →  (∂x, ∂r) = (d_y, d_y)
        verify_gradient_case(
            "f(x, r) = x + r",
            &build_binary_primal("add", "arith.addf"),
            vec!["x".to_string(), "r".to_string()],
            vec![AnalyticExpr::v("d_y"), AnalyticExpr::v("d_y")],
        ),
        // § FSub : f(x, r) = x - r  →  (∂x, ∂r) = (d_y, -d_y)
        verify_gradient_case(
            "f(x, r) = x - r  [sphere-sdf scalar surrogate]",
            &build_binary_primal("sphere_sdf", "arith.subf"),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::v("d_y"),
                AnalyticExpr::neg(AnalyticExpr::v("d_y")),
            ],
        ),
        // § FMul : f(x, r) = x * r  →  (∂x, ∂r) = (r·d_y, x·d_y)
        verify_gradient_case(
            "f(x, r) = x * r",
            &build_binary_primal("mul", "arith.mulf"),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::mul(AnalyticExpr::v("d_y"), AnalyticExpr::v("r")),
                AnalyticExpr::mul(AnalyticExpr::v("d_y"), AnalyticExpr::v("x")),
            ],
        ),
        // § FDiv : f(x, r) = x / r  →  (∂x, ∂r) = (d_y / r, -d_y · x / r²)
        verify_gradient_case(
            "f(x, r) = x / r",
            &build_binary_primal("div", "arith.divf"),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::div(AnalyticExpr::v("d_y"), AnalyticExpr::v("r")),
                AnalyticExpr::neg(AnalyticExpr::div(
                    AnalyticExpr::mul(AnalyticExpr::v("d_y"), AnalyticExpr::v("x")),
                    AnalyticExpr::mul(AnalyticExpr::v("r"), AnalyticExpr::v("r")),
                )),
            ],
        ),
        // § FNeg : f(x) = -x  →  ∂x = -d_y
        verify_gradient_case(
            "f(x) = -x",
            &build_unary_primal("neg", "arith.negf"),
            vec!["x".to_string()],
            vec![AnalyticExpr::neg(AnalyticExpr::v("d_y"))],
        ),
        // § Sqrt : f(x) = √x  →  ∂x = d_y / (2·√x)
        verify_gradient_case(
            "f(x) = sqrt(x)",
            &build_transcendental_primal("sqrtfn", "sqrt"),
            vec!["x".to_string()],
            vec![AnalyticExpr::div(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::mul(
                    AnalyticExpr::c(2.0),
                    AnalyticExpr::Sqrt(Box::new(AnalyticExpr::v("x"))),
                ),
            )],
        ),
        // § Sin : f(x) = sin(x)  →  ∂x = cos(x)·d_y
        verify_gradient_case(
            "f(x) = sin(x)",
            &build_transcendental_primal("sinfn", "sin"),
            vec!["x".to_string()],
            vec![AnalyticExpr::mul(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::Cos(Box::new(AnalyticExpr::v("x"))),
            )],
        ),
        // § Cos : f(x) = cos(x)  →  ∂x = -sin(x)·d_y
        verify_gradient_case(
            "f(x) = cos(x)",
            &build_transcendental_primal("cosfn", "cos"),
            vec!["x".to_string()],
            vec![AnalyticExpr::neg(AnalyticExpr::mul(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::Sin(Box::new(AnalyticExpr::v("x"))),
            ))],
        ),
        // § Exp : f(x) = exp(x)  →  ∂x = exp(x)·d_y
        verify_gradient_case(
            "f(x) = exp(x)",
            &build_transcendental_primal("expfn", "exp"),
            vec!["x".to_string()],
            vec![AnalyticExpr::mul(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::Exp(Box::new(AnalyticExpr::v("x"))),
            )],
        ),
        // § Log : f(x) = log(x)  →  ∂x = d_y / x
        verify_gradient_case(
            "f(x) = log(x)",
            &build_transcendental_primal("logfn", "log"),
            vec!["x".to_string()],
            vec![AnalyticExpr::div(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::v("x"),
            )],
        ),
        // § Chain : f(x, r) = (x - r) * (x - r)  →  (2(x-r)·d_y, -2(x-r)·d_y)
        verify_gradient_case(
            "f(x, r) = (x - r) * (x - r)  [chain-rule exercise]",
            &build_chain_primal(),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::mul(
                    AnalyticExpr::c(2.0),
                    AnalyticExpr::mul(
                        AnalyticExpr::sub(AnalyticExpr::v("x"), AnalyticExpr::v("r")),
                        AnalyticExpr::v("d_y"),
                    ),
                ),
                AnalyticExpr::neg(AnalyticExpr::mul(
                    AnalyticExpr::c(2.0),
                    AnalyticExpr::mul(
                        AnalyticExpr::sub(AnalyticExpr::v("x"), AnalyticExpr::v("r")),
                        AnalyticExpr::v("d_y"),
                    ),
                )),
            ],
        ),
    ];
    let total = cases.len();
    let passing = cases.iter().filter(|c| c.all_match).count();
    KillerAppGateReport {
        cases,
        total,
        passing,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § R18 attestation : sign the killer-app gate report so any third party
//   holding the public key can independently verify the gate verdict.
// ─────────────────────────────────────────────────────────────────────────

use cssl_telemetry::{verify_detached, ContentHash, Signature, SigningKey};

/// Canonical-serialization version tag embedded in every attestation payload.
/// Bump when the serializer format changes so verifiers can reject stale forms.
pub const ATTESTATION_FORMAT: &str = "CSSLv3-R18-KILLER-APP-GATE-v1";

/// An R18 signed attestation of a [`KillerAppGateReport`].
///
/// Produced by [`sign_gate_report`], verified by
/// [`verify_signed_gate_report`]. The `canonical_payload` field is the exact
/// byte-sequence that was hashed + signed — a third-party auditor
/// reconstructs it from the plain-text report via
/// [`canonical_report_bytes`] and re-hashes to confirm the hash hasn't been
/// tampered with.
#[derive(Debug, Clone)]
pub struct SignedKillerAppGateReport {
    /// The attested report.
    pub report: KillerAppGateReport,
    /// Deterministic byte-serialization of the report (what was hashed).
    pub canonical_payload: Vec<u8>,
    /// BLAKE3 hash of `canonical_payload`.
    pub content_hash: ContentHash,
    /// Ed25519 signature over `content_hash.0` (the raw 32 bytes).
    pub signature: Signature,
    /// 32-byte verifying-key (public half of the signing-key) corresponding
    /// to this signature. Bundled so verifiers don't need out-of-band key
    /// distribution — the verification step still has to validate the key
    /// itself against a trusted key-list.
    pub verifying_key: [u8; 32],
    /// Attestation format tag (e.g. `"CSSLv3-R18-KILLER-APP-GATE-v1"`).
    pub format: String,
}

impl SignedKillerAppGateReport {
    /// Short one-line summary including the gate verdict + hash prefix.
    #[must_use]
    pub fn summary(&self) -> String {
        let hash_prefix = &self.content_hash.hex()[..16];
        format!(
            "{} | hash={}… | {} | key={}…",
            self.format,
            hash_prefix,
            self.report.summary(),
            &hex_short(&self.verifying_key, 8)
        )
    }
}

fn hex_short(bytes: &[u8], n: usize) -> String {
    let mut s = String::with_capacity(n * 2);
    for b in bytes.iter().take(n) {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Serialize a [`KillerAppGateReport`] into a deterministic byte-sequence
/// suitable for hashing. The format is line-oriented UTF-8 :
///
/// ```text
/// CSSLv3-R18-KILLER-APP-GATE-v1
/// total=<N>
/// passing=<N>
/// case[<i>]: <name> | match=<true|false> | params=<csv>
/// case[<i>].param[<j>]: <param-name> | match=<b> | analytic=<smt> | mir=<smt>
/// ...
/// end
/// ```
///
/// Every field is in insertion order ; every string is UTF-8 ; every line is
/// newline-terminated. A third-party auditor can reconstruct this exact byte-
/// sequence from the plain-text report and verify the signature.
#[must_use]
pub fn canonical_report_bytes(report: &KillerAppGateReport) -> Vec<u8> {
    let mut out = String::new();
    out.push_str(ATTESTATION_FORMAT);
    out.push('\n');
    out.push_str(&format!("total={}\n", report.total));
    out.push_str(&format!("passing={}\n", report.passing));
    for (i, c) in report.cases.iter().enumerate() {
        out.push_str(&format!(
            "case[{i}]: {name} | match={m} | params={p}\n",
            name = c.name,
            m = c.all_match,
            p = c.param_names.join(","),
        ));
        for (j, p) in c.params.iter().enumerate() {
            out.push_str(&format!(
                "case[{i}].param[{j}]: {name} | match={m} | analytic={ana} | mir={mir}\n",
                name = p.name,
                m = p.matches,
                ana = p.analytic.to_smt(),
                mir = p.mir_derived.to_smt(),
            ));
        }
    }
    out.push_str("end\n");
    out.into_bytes()
}

/// Sign a [`KillerAppGateReport`] under the given key, producing an
/// attestation that any holder of `key.verifying_key_bytes()` can verify.
#[must_use]
pub fn sign_gate_report(
    report: KillerAppGateReport,
    key: &SigningKey,
) -> SignedKillerAppGateReport {
    let canonical_payload = canonical_report_bytes(&report);
    let content_hash = ContentHash::hash(&canonical_payload);
    let signature = Signature::sign(key, &content_hash.0);
    SignedKillerAppGateReport {
        report,
        canonical_payload,
        content_hash,
        signature,
        verifying_key: key.verifying_key_bytes(),
        format: ATTESTATION_FORMAT.to_string(),
    }
}

/// Verification verdict : returned by [`verify_signed_gate_report`] with per-
/// step status. A third-party auditor should require **all four** checks to
/// pass before trusting the gate verdict inside `signed.report`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct AttestationVerdict {
    /// `true` iff the format-tag matches the expected version.
    pub format_matches: bool,
    /// `true` iff recomputed BLAKE3 over `canonical_report_bytes(&report)`
    /// matches the stored `content_hash`. Detects payload tampering.
    pub payload_hash_matches: bool,
    /// `true` iff the Ed25519 signature verifies under the expected
    /// verifying-key. Detects signature forgery / wrong-key attempts.
    pub signature_verifies: bool,
    /// `true` iff the report self-reports full-gate-green (every case matched).
    /// An auditor may require this as an additional acceptance criterion.
    pub gate_is_green: bool,
}

impl AttestationVerdict {
    /// `true` iff all four checks pass.
    #[must_use]
    pub fn is_fully_valid(&self) -> bool {
        self.format_matches
            && self.payload_hash_matches
            && self.signature_verifies
            && self.gate_is_green
    }

    /// `true` iff format + hash + signature pass (ignores gate-green ; useful
    /// when the auditor wants to accept a signed failure-report too).
    #[must_use]
    pub fn cryptographically_valid(&self) -> bool {
        self.format_matches && self.payload_hash_matches && self.signature_verifies
    }
}

/// Verify a signed gate report against an `expected_verifying_key`. Returns a
/// per-step [`AttestationVerdict`] ; the auditor decides whether
/// [`AttestationVerdict::is_fully_valid`] or
/// [`AttestationVerdict::cryptographically_valid`] is the right threshold for
/// their use-case.
#[must_use]
pub fn verify_signed_gate_report(
    signed: &SignedKillerAppGateReport,
    expected_verifying_key: &[u8; 32],
) -> AttestationVerdict {
    let format_matches = signed.format == ATTESTATION_FORMAT;
    let recomputed_payload = canonical_report_bytes(&signed.report);
    let recomputed_hash = ContentHash::hash(&recomputed_payload);
    let payload_hash_matches =
        recomputed_hash == signed.content_hash && recomputed_payload == signed.canonical_payload;
    let signature_verifies = signed.verifying_key == *expected_verifying_key
        && verify_detached(
            &signed.verifying_key,
            &signed.content_hash.0,
            &signed.signature,
        )
        .is_ok();
    let gate_is_green = signed.report.is_green();
    AttestationVerdict {
        format_matches,
        payload_hash_matches,
        signature_verifies,
        gate_is_green,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_binary_primal, build_chain_primal, build_transcendental_primal, build_unary_primal,
        run_killer_app_gate, verify_gradient_case, AnalyticExpr, MirAdjointInterpreter,
    };
    use cssl_autodiff::{apply_bwd, DiffRuleTable};

    fn env2(
        a_name: &str,
        a: f64,
        b_name: &str,
        b: f64,
        d_y: f64,
    ) -> std::collections::HashMap<String, f64> {
        let mut m = std::collections::HashMap::new();
        m.insert(a_name.to_string(), a);
        m.insert(b_name.to_string(), b);
        m.insert("d_y".to_string(), d_y);
        m
    }

    // § AnalyticExpr algebra
    #[test]
    fn analytic_simplify_add_zero_left() {
        let e = AnalyticExpr::add(AnalyticExpr::c(0.0), AnalyticExpr::v("x"));
        assert_eq!(e.simplify(), AnalyticExpr::v("x"));
    }

    #[test]
    fn analytic_simplify_mul_zero() {
        let e = AnalyticExpr::mul(AnalyticExpr::v("x"), AnalyticExpr::c(0.0));
        assert_eq!(e.simplify(), AnalyticExpr::c(0.0));
    }

    #[test]
    fn analytic_simplify_mul_one() {
        let e = AnalyticExpr::mul(AnalyticExpr::c(1.0), AnalyticExpr::v("x"));
        assert_eq!(e.simplify(), AnalyticExpr::v("x"));
    }

    #[test]
    fn analytic_simplify_double_neg() {
        let e = AnalyticExpr::neg(AnalyticExpr::neg(AnalyticExpr::v("x")));
        assert_eq!(e.simplify(), AnalyticExpr::v("x"));
    }

    #[test]
    fn analytic_evaluate_basic() {
        let e = AnalyticExpr::mul(AnalyticExpr::v("x"), AnalyticExpr::v("y"));
        let mut env = std::collections::HashMap::new();
        env.insert("x".to_string(), 2.0);
        env.insert("y".to_string(), 3.0);
        assert!((e.evaluate(&env) - 6.0).abs() < 1e-12);
    }

    #[test]
    fn analytic_equivalent_by_sampling_trivial() {
        let a = AnalyticExpr::add(AnalyticExpr::v("x"), AnalyticExpr::v("y"));
        let b = AnalyticExpr::add(AnalyticExpr::v("y"), AnalyticExpr::v("x")); // commutative
        let samples = vec![
            env2("x", 1.0, "y", 2.0, 1.0),
            env2("x", -1.5, "y", 0.7, -1.0),
        ];
        assert!(a.equivalent_by_sampling(&b, &samples, 1e-12));
    }

    #[test]
    fn analytic_to_smt_shape() {
        let e = AnalyticExpr::mul(AnalyticExpr::v("x"), AnalyticExpr::c(2.0));
        let s = e.to_smt();
        assert_eq!(s, "(* x 2.0)");
    }

    #[test]
    fn analytic_free_vars_distinct_sorted() {
        let e = AnalyticExpr::add(
            AnalyticExpr::mul(AnalyticExpr::v("y"), AnalyticExpr::v("x")),
            AnalyticExpr::v("x"),
        );
        let vars = e.free_vars();
        assert_eq!(vars, vec!["x".to_string(), "y".to_string()]);
    }

    // § MirAdjointInterpreter
    #[test]
    fn interpreter_seeds_primal_and_adjoint_params() {
        let primal = build_binary_primal("add", "arith.addf");
        let (bwd, _, _) = apply_bwd(&primal, &DiffRuleTable::canonical());
        let interp = MirAdjointInterpreter::new(&bwd, vec!["x".to_string(), "r".to_string()]);
        let entry = bwd.body.entry().unwrap();
        // First two entry args are x, r (primal) ; third is d_y (adjoint-in).
        assert_eq!(entry.args.len(), 3);
        assert_eq!(interp.primal_exprs.len(), 2);
        assert_eq!(interp.adjoint_exprs.len(), 1);
    }

    // § Per-primitive gradient equivalence
    #[test]
    fn gate_fadd_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x, r) = x + r",
            &build_binary_primal("add", "arith.addf"),
            vec!["x".to_string(), "r".to_string()],
            vec![AnalyticExpr::v("d_y"), AnalyticExpr::v("d_y")],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_fsub_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x, r) = x - r",
            &build_binary_primal("sub", "arith.subf"),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::v("d_y"),
                AnalyticExpr::neg(AnalyticExpr::v("d_y")),
            ],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_fmul_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x, r) = x * r",
            &build_binary_primal("mul", "arith.mulf"),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::mul(AnalyticExpr::v("d_y"), AnalyticExpr::v("r")),
                AnalyticExpr::mul(AnalyticExpr::v("d_y"), AnalyticExpr::v("x")),
            ],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_fdiv_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x, r) = x / r",
            &build_binary_primal("div", "arith.divf"),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::div(AnalyticExpr::v("d_y"), AnalyticExpr::v("r")),
                AnalyticExpr::neg(AnalyticExpr::div(
                    AnalyticExpr::mul(AnalyticExpr::v("d_y"), AnalyticExpr::v("x")),
                    AnalyticExpr::mul(AnalyticExpr::v("r"), AnalyticExpr::v("r")),
                )),
            ],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_fneg_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x) = -x",
            &build_unary_primal("neg", "arith.negf"),
            vec!["x".to_string()],
            vec![AnalyticExpr::neg(AnalyticExpr::v("d_y"))],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_sqrt_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x) = sqrt(x)",
            &build_transcendental_primal("sqrtfn", "sqrt"),
            vec!["x".to_string()],
            vec![AnalyticExpr::div(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::mul(
                    AnalyticExpr::c(2.0),
                    AnalyticExpr::Sqrt(Box::new(AnalyticExpr::v("x"))),
                ),
            )],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_sin_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x) = sin(x)",
            &build_transcendental_primal("sinfn", "sin"),
            vec!["x".to_string()],
            vec![AnalyticExpr::mul(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::Cos(Box::new(AnalyticExpr::v("x"))),
            )],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_cos_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x) = cos(x)",
            &build_transcendental_primal("cosfn", "cos"),
            vec!["x".to_string()],
            vec![AnalyticExpr::neg(AnalyticExpr::mul(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::Sin(Box::new(AnalyticExpr::v("x"))),
            ))],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_exp_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x) = exp(x)",
            &build_transcendental_primal("expfn", "exp"),
            vec!["x".to_string()],
            vec![AnalyticExpr::mul(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::Exp(Box::new(AnalyticExpr::v("x"))),
            )],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_log_gradient_matches_analytic() {
        let c = verify_gradient_case(
            "f(x) = log(x)",
            &build_transcendental_primal("logfn", "log"),
            vec!["x".to_string()],
            vec![AnalyticExpr::div(
                AnalyticExpr::v("d_y"),
                AnalyticExpr::v("x"),
            )],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    #[test]
    fn gate_chain_rule_matches_analytic() {
        let c = verify_gradient_case(
            "chain (x - r)²",
            &build_chain_primal(),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::mul(
                    AnalyticExpr::c(2.0),
                    AnalyticExpr::mul(
                        AnalyticExpr::sub(AnalyticExpr::v("x"), AnalyticExpr::v("r")),
                        AnalyticExpr::v("d_y"),
                    ),
                ),
                AnalyticExpr::neg(AnalyticExpr::mul(
                    AnalyticExpr::c(2.0),
                    AnalyticExpr::mul(
                        AnalyticExpr::sub(AnalyticExpr::v("x"), AnalyticExpr::v("r")),
                        AnalyticExpr::v("d_y"),
                    ),
                )),
            ],
        );
        assert!(c.all_match, "{}", c.summary());
    }

    // § Top-level gate
    #[test]
    fn killer_app_gate_all_cases_pass() {
        let report = run_killer_app_gate();
        assert!(
            report.is_green(),
            "{}\n{:#?}",
            report.summary(),
            report
                .cases
                .iter()
                .filter(|c| !c.all_match)
                .map(|c| c.summary())
                .collect::<Vec<_>>()
        );
        // The canonical gate covers 11 cases (5 arith + 5 transcendental + 1 chain).
        assert_eq!(report.total, 11);
        assert_eq!(report.passing, 11);
    }

    #[test]
    fn gate_summary_shape() {
        let report = run_killer_app_gate();
        let s = report.summary();
        assert!(s.contains("KILLER-APP GATE"));
        assert!(s.contains("/"));
    }

    #[test]
    fn gate_smt_query_text_contains_declarations_and_assertion() {
        let c = verify_gradient_case(
            "f(x, r) = x + r",
            &build_binary_primal("add", "arith.addf"),
            vec!["x".to_string(), "r".to_string()],
            vec![AnalyticExpr::v("d_y"), AnalyticExpr::v("d_y")],
        );
        let smt = c.smt_query_text();
        assert!(smt.contains("(set-logic QF_UFNRA)"));
        assert!(smt.contains("(declare-fun x () Real)"));
        assert!(smt.contains("(declare-fun r () Real)"));
        assert!(smt.contains("(declare-fun d_y () Real)"));
        assert!(smt.contains("(assert (not"));
        assert!(smt.contains("(check-sat)"));
    }

    #[test]
    fn gate_chain_gradient_numerically_matches_at_point() {
        // Sanity : evaluate both analytic and MIR-derived at (x=3, r=1, d_y=1).
        // analytic : 2(x-r)·d_y = 2·2·1 = 4 ; ∂r = -4.
        let c = verify_gradient_case(
            "chain (x - r)²",
            &build_chain_primal(),
            vec!["x".to_string(), "r".to_string()],
            vec![
                AnalyticExpr::mul(
                    AnalyticExpr::c(2.0),
                    AnalyticExpr::mul(
                        AnalyticExpr::sub(AnalyticExpr::v("x"), AnalyticExpr::v("r")),
                        AnalyticExpr::v("d_y"),
                    ),
                ),
                AnalyticExpr::neg(AnalyticExpr::mul(
                    AnalyticExpr::c(2.0),
                    AnalyticExpr::mul(
                        AnalyticExpr::sub(AnalyticExpr::v("x"), AnalyticExpr::v("r")),
                        AnalyticExpr::v("d_y"),
                    ),
                )),
            ],
        );
        let env = env2("x", 3.0, "r", 1.0, 1.0);
        let dx = c.params[0].mir_derived.evaluate(&env);
        let dr = c.params[1].mir_derived.evaluate(&env);
        assert!((dx - 4.0).abs() < 1e-9, "expected 4, got {dx}");
        assert!((dr - -4.0).abs() < 1e-9, "expected -4, got {dr}");
    }

    // ─────────────────────────────────────────────────────────────────────
    // § R18 ATTESTATION : sign + verify the killer-app gate report.
    // ─────────────────────────────────────────────────────────────────────

    use super::{
        canonical_report_bytes, sign_gate_report, verify_signed_gate_report, AttestationVerdict,
        ATTESTATION_FORMAT,
    };
    use cssl_telemetry::SigningKey;

    fn fixed_seed_key() -> SigningKey {
        // Deterministic 32-byte seed — tests must be reproducible across runs.
        let mut seed = [0u8; 32];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(13);
        }
        SigningKey::from_seed(seed)
    }

    #[test]
    fn attestation_format_tag_is_stable() {
        assert_eq!(ATTESTATION_FORMAT, "CSSLv3-R18-KILLER-APP-GATE-v1");
    }

    #[test]
    fn canonical_bytes_is_deterministic_across_calls() {
        let report = run_killer_app_gate();
        let a = canonical_report_bytes(&report);
        let b = canonical_report_bytes(&report);
        assert_eq!(a, b);
        // Format tag must be the first bytes.
        assert!(a.starts_with(ATTESTATION_FORMAT.as_bytes()));
        // `end` terminator must be the last meaningful line.
        assert!(core::str::from_utf8(&a).unwrap().ends_with("end\n"));
    }

    #[test]
    fn canonical_bytes_contains_every_case() {
        let report = run_killer_app_gate();
        let bytes = canonical_report_bytes(&report);
        let text = core::str::from_utf8(&bytes).unwrap();
        for c in &report.cases {
            assert!(text.contains(&c.name), "missing case `{}`", c.name);
        }
        assert!(text.contains(&format!("total={}", report.total)));
        assert!(text.contains(&format!("passing={}", report.passing)));
    }

    #[test]
    fn sign_then_verify_roundtrip_fully_valid() {
        let report = run_killer_app_gate();
        let key = fixed_seed_key();
        let vk = key.verifying_key_bytes();
        let signed = sign_gate_report(report, &key);
        let verdict = verify_signed_gate_report(&signed, &vk);
        assert_eq!(
            verdict,
            AttestationVerdict {
                format_matches: true,
                payload_hash_matches: true,
                signature_verifies: true,
                gate_is_green: true,
            }
        );
        assert!(verdict.is_fully_valid());
        assert!(verdict.cryptographically_valid());
    }

    #[test]
    fn verify_fails_under_wrong_key() {
        let report = run_killer_app_gate();
        let key = fixed_seed_key();
        let signed = sign_gate_report(report, &key);
        // A different key — should not verify.
        let other_seed = [0x11u8; 32];
        let other_vk = SigningKey::from_seed(other_seed).verifying_key_bytes();
        let verdict = verify_signed_gate_report(&signed, &other_vk);
        assert!(!verdict.signature_verifies);
        assert!(!verdict.is_fully_valid());
        // Format + payload hash still match — tamper detection is about the hash
        // chain ; wrong-key only fails the signature step.
        assert!(verdict.format_matches);
        assert!(verdict.payload_hash_matches);
    }

    #[test]
    fn tampered_report_fails_payload_hash_check() {
        let report = run_killer_app_gate();
        let key = fixed_seed_key();
        let vk = key.verifying_key_bytes();
        let mut signed = sign_gate_report(report, &key);
        // Tamper with the report after signing — the signature is still over the
        // OLD hash, so recomputing the payload hash now mismatches.
        signed.report.total = 99;
        let verdict = verify_signed_gate_report(&signed, &vk);
        assert!(!verdict.payload_hash_matches);
        assert!(!verdict.is_fully_valid());
    }

    #[test]
    fn tampered_format_tag_fails_format_check() {
        let report = run_killer_app_gate();
        let key = fixed_seed_key();
        let vk = key.verifying_key_bytes();
        let mut signed = sign_gate_report(report, &key);
        signed.format = "CSSLv3-OLD-FORMAT-v0".to_string();
        let verdict = verify_signed_gate_report(&signed, &vk);
        assert!(!verdict.format_matches);
        assert!(!verdict.is_fully_valid());
    }

    #[test]
    fn tampered_signature_fails_signature_check() {
        let report = run_killer_app_gate();
        let key = fixed_seed_key();
        let vk = key.verifying_key_bytes();
        let mut signed = sign_gate_report(report, &key);
        // Flip one byte of the signature → must fail.
        signed.signature.0[0] ^= 0x5a;
        let verdict = verify_signed_gate_report(&signed, &vk);
        assert!(!verdict.signature_verifies);
        assert!(!verdict.is_fully_valid());
    }

    #[test]
    fn signed_report_summary_contains_gate_verdict() {
        let report = run_killer_app_gate();
        let key = fixed_seed_key();
        let signed = sign_gate_report(report, &key);
        let s = signed.summary();
        assert!(s.contains(ATTESTATION_FORMAT));
        assert!(s.contains("hash="));
        assert!(s.contains("KILLER-APP GATE"));
        assert!(s.contains("key="));
    }

    #[test]
    fn signing_is_deterministic_under_fixed_seed() {
        let report = run_killer_app_gate();
        let key_a = fixed_seed_key();
        let key_b = fixed_seed_key();
        let signed_a = sign_gate_report(report.clone(), &key_a);
        let signed_b = sign_gate_report(report, &key_b);
        assert_eq!(signed_a.content_hash, signed_b.content_hash);
        assert_eq!(signed_a.verifying_key, signed_b.verifying_key);
        // Ed25519 signatures are deterministic under RFC 8032 ; same key + same
        // message → same signature.
        assert_eq!(signed_a.signature.0, signed_b.signature.0);
    }

    #[test]
    fn cryptographically_valid_accepts_failed_gate_when_hash_and_sig_ok() {
        // Build a hand-rolled failing gate : construct a `KillerAppGateReport`
        // with passing < total, sign it honestly, then verify.
        let full = run_killer_app_gate();
        let mut degraded = full.clone();
        degraded.passing = degraded.total.saturating_sub(1); // pretend one case failed
        let key = fixed_seed_key();
        let vk = key.verifying_key_bytes();
        let signed = sign_gate_report(degraded, &key);
        let verdict = verify_signed_gate_report(&signed, &vk);
        // The signature is valid → cryptographically-valid is true.
        // But the gate isn't green → is_fully_valid is false.
        assert!(verdict.cryptographically_valid());
        assert!(!verdict.gate_is_green);
        assert!(!verdict.is_fully_valid());
    }
}
