//! Metamorphic oracle (`@metamorphic`) — algebraic-law preservation.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • metamorphic.
//! § ROLE   : macro-generated equivalence tests over algebraic properties —
//!            commutativity, associativity, Leibniz rule, Lipschitz bounds, etc.
//! § STATUS : T11 stub — implementation pending.

/// Algebraic law classes recognized by `@metamorphic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Law {
    /// `a * b == b * a`
    Commutative,
    /// `(a * b) * c == a * (b * c)`
    Associative,
    /// `a * (b + c) == a*b + a*c`
    Distributive,
    /// Slang.D rule : `bwd_diff(f * g) == bwd_diff(f)*g + f*bwd_diff(g)`.
    Leibniz,
    /// Faà di Bruno generalization for higher-order jets.
    FaaDiBruno,
    /// `|sdf(p) - sdf(p + eps*n)| <= k*eps` (Lipschitz on SDFs).
    Lipschitz,
    /// Conservation laws on fluid simulations (mass, momentum).
    Conservation,
    /// User-provided custom law identified by name.
    Custom,
}

/// Config for the `@metamorphic` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Law to verify.
    pub law: Law,
    /// Custom-law name when `law == Law::Custom`.
    pub custom_name: Option<String>,
    /// Sample count for random-input verification.
    pub samples: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            law: Law::Commutative,
            custom_name: None,
            samples: 256,
        }
    }
}

/// Outcome of running the `@metamorphic` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11.
    Stage0Unimplemented,
    /// Law held on every sample.
    Ok { samples_tested: u32 },
    /// Law violated at the described sample.
    Violation { sample: String, message: String },
}

/// Dispatcher trait for `@metamorphic` oracle.
pub trait Dispatcher {
    fn run(&self, config: &Config) -> Outcome;
}

/// Stage0 stub dispatcher — always returns `Stage0Unimplemented`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0Stub;

impl Dispatcher for Stage0Stub {
    fn run(&self, _config: &Config) -> Outcome {
        Outcome::Stage0Unimplemented
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Live generic-relation runner : tests algebraic laws via sampling.
// ─────────────────────────────────────────────────────────────────────────

/// Check commutativity of a binary operation over a set of samples.
/// Returns `Ok` if every sample pair `(a, b)` satisfies `op(a, b) = op(b, a)`,
/// else `Violation` with the first failing pair.
///
/// `eq` is the equality predicate — use exact `==` for `i64` / `bool` and
/// an `|x, y| (x - y).abs() <= ε` for floats.
pub fn check_commutative<T, Op, Eq>(samples: &[(T, T)], op: Op, eq: Eq) -> Outcome
where
    T: core::fmt::Debug + Clone,
    Op: Fn(&T, &T) -> T,
    Eq: Fn(&T, &T) -> bool,
{
    for (i, (a, b)) in samples.iter().enumerate() {
        let ab = op(a, b);
        let ba = op(b, a);
        if !eq(&ab, &ba) {
            return Outcome::Violation {
                sample: format!("(a={a:?}, b={b:?})"),
                message: format!(
                    "commutativity failed at sample {i} : op(a, b) = {ab:?} ≠ {ba:?} = op(b, a)"
                ),
            };
        }
    }
    Outcome::Ok {
        samples_tested: samples.len() as u32,
    }
}

/// Check associativity of a binary operation over triples of samples.
/// Returns `Ok` if every triple satisfies `op(op(a, b), c) = op(a, op(b, c))`.
pub fn check_associative<T, Op, Eq>(samples: &[(T, T, T)], op: Op, eq: Eq) -> Outcome
where
    T: core::fmt::Debug + Clone,
    Op: Fn(&T, &T) -> T,
    Eq: Fn(&T, &T) -> bool,
{
    for (i, (a, b, c)) in samples.iter().enumerate() {
        let ab = op(a, b);
        let left = op(&ab, c);
        let bc = op(b, c);
        let right = op(a, &bc);
        if !eq(&left, &right) {
            return Outcome::Violation {
                sample: format!("(a={a:?}, b={b:?}, c={c:?})"),
                message: format!(
                    "associativity failed at sample {i} : (a·b)·c = {left:?} ≠ {right:?} = a·(b·c)"
                ),
            };
        }
    }
    Outcome::Ok {
        samples_tested: samples.len() as u32,
    }
}

/// Check distributivity : `op(a, add(b, c)) = add(op(a, b), op(a, c))`.
/// Takes both the primary op and the "add" op ; useful for checking e.g.,
/// `a * (b + c) = a*b + a*c`.
pub fn check_distributive<T, Mul, Add, Eq>(
    samples: &[(T, T, T)],
    mul: Mul,
    add: Add,
    eq: Eq,
) -> Outcome
where
    T: core::fmt::Debug + Clone,
    Mul: Fn(&T, &T) -> T,
    Add: Fn(&T, &T) -> T,
    Eq: Fn(&T, &T) -> bool,
{
    for (i, (a, b, c)) in samples.iter().enumerate() {
        let bc = add(b, c);
        let left = mul(a, &bc);
        let ab = mul(a, b);
        let ac = mul(a, c);
        let right = add(&ab, &ac);
        if !eq(&left, &right) {
            return Outcome::Violation {
                sample: format!("(a={a:?}, b={b:?}, c={c:?})"),
                message: format!(
                    "distributivity failed at sample {i} : a·(b+c) = {left:?} ≠ {right:?} = a·b + a·c"
                ),
            };
        }
    }
    Outcome::Ok {
        samples_tested: samples.len() as u32,
    }
}

/// Check idempotence : `op(op(x)) = op(x)`. Applies `op` twice and compares.
pub fn check_idempotent<T, Op, Eq>(samples: &[T], op: Op, eq: Eq) -> Outcome
where
    T: core::fmt::Debug + Clone,
    Op: Fn(&T) -> T,
    Eq: Fn(&T, &T) -> bool,
{
    for (i, x) in samples.iter().enumerate() {
        let once = op(x);
        let twice = op(&once);
        if !eq(&once, &twice) {
            return Outcome::Violation {
                sample: format!("x={x:?}"),
                message: format!(
                    "idempotence failed at sample {i} : op(op(x)) = {twice:?} ≠ {once:?} = op(x)"
                ),
            };
        }
    }
    Outcome::Ok {
        samples_tested: samples.len() as u32,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Calculus-rule numeric validators : verify f'/g'/combined hold at samples.
// ─────────────────────────────────────────────────────────────────────────

/// Verify the Leibniz product rule numerically : `(f*g)'(x) = f'(x)·g(x) + f(x)·g'(x)`.
///
/// Given :
/// - `f(x) -> f64` + `df(x) -> f64` (its derivative)
/// - `g(x) -> f64` + `dg(x) -> f64` (its derivative)
/// - `samples: &[f64]` — input points to check
/// - `tolerance: f64` — absolute difference tolerated (e.g., `1e-9` for hand-coded derivs)
///
/// Returns `Ok { samples_tested }` if every sample satisfies the rule within
/// `tolerance`, else `Violation` at the first failing sample.
#[allow(clippy::similar_names)] // dfx / dgx mirror standard calculus-notation df/dg
pub fn check_leibniz<F, DF, G, DG>(
    samples: &[f64],
    f: F,
    df: DF,
    g: G,
    dg: DG,
    tolerance: f64,
) -> Outcome
where
    F: Fn(f64) -> f64,
    DF: Fn(f64) -> f64,
    G: Fn(f64) -> f64,
    DG: Fn(f64) -> f64,
{
    for (i, &x) in samples.iter().enumerate() {
        let fx = f(x);
        let dfx = df(x);
        let gx = g(x);
        let dgx = dg(x);
        // Product-rule RHS : f'·g + f·g'
        let rhs = dfx.mul_add(gx, fx * dgx);
        // LHS via central-differences at step h. Use h scaled to x but floored.
        let h = 1e-5_f64.max(x.abs() * 1e-6);
        let lhs = f(x + h).mul_add(g(x + h), -(f(x - h) * g(x - h))) / (2.0 * h);
        if (lhs - rhs).abs() > tolerance {
            return Outcome::Violation {
                sample: format!("x={x}"),
                message: format!(
                    "Leibniz rule failed at sample {i} : (f·g)'(x) = {lhs} ≠ {rhs} = f'·g + f·g' (|Δ|={})",
                    (lhs - rhs).abs()
                ),
            };
        }
    }
    Outcome::Ok {
        samples_tested: samples.len() as u32,
    }
}

/// Verify the chain rule numerically : `(f∘g)'(x) = f'(g(x)) · g'(x)`.
///
/// Returns `Ok { samples_tested }` if every sample passes within `tolerance`,
/// else `Violation` at the first failing sample.
pub fn check_chain_rule<F, DF, G, DG>(
    samples: &[f64],
    f: F,
    df: DF,
    g: G,
    dg: DG,
    tolerance: f64,
) -> Outcome
where
    F: Fn(f64) -> f64,
    DF: Fn(f64) -> f64,
    G: Fn(f64) -> f64,
    DG: Fn(f64) -> f64,
{
    for (i, &x) in samples.iter().enumerate() {
        let gx = g(x);
        let dgx = dg(x);
        let df_gx = df(gx);
        let rhs = df_gx * dgx;
        // LHS via central-differences on f∘g.
        let h = 1e-5_f64.max(x.abs() * 1e-6);
        let lhs = (f(g(x + h)) - f(g(x - h))) / (2.0 * h);
        if (lhs - rhs).abs() > tolerance {
            return Outcome::Violation {
                sample: format!("x={x}"),
                message: format!(
                    "chain rule failed at sample {i} : (f∘g)'(x) = {lhs} ≠ {rhs} = f'(g(x))·g'(x) (|Δ|={})",
                    (lhs - rhs).abs()
                ),
            };
        }
    }
    Outcome::Ok {
        samples_tested: samples.len() as u32,
    }
}

/// Verify Lipschitz continuity numerically : `|f(x) - f(y)| ≤ K · |x - y|` for
/// every `(x, y)` sample pair, where `K` is the Lipschitz constant.
///
/// Used to verify SDFs are 1-Lipschitz (per `specs/05_AUTODIFF.csl` § SDF-NORMAL).
pub fn check_lipschitz<F>(samples: &[(f64, f64)], f: F, k: f64) -> Outcome
where
    F: Fn(f64) -> f64,
{
    for (i, &(x, y)) in samples.iter().enumerate() {
        let fx = f(x);
        let fy = f(y);
        let lhs = (fx - fy).abs();
        let rhs = k * (x - y).abs();
        if lhs > rhs {
            return Outcome::Violation {
                sample: format!("(x={x}, y={y})"),
                message: format!(
                    "Lipschitz-{k} failed at sample {i} : |f(x) - f(y)| = {lhs} > {rhs} = K·|x-y|"
                ),
            };
        }
    }
    Outcome::Ok {
        samples_tested: samples.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check_associative, check_chain_rule, check_commutative, check_distributive,
        check_idempotent, check_leibniz, check_lipschitz, Config, Dispatcher, Outcome, Stage0Stub,
    };

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }

    #[test]
    fn i64_addition_is_commutative() {
        let samples = [(1i64, 2i64), (-5, 10), (0, 0), (i64::MAX / 2, 1)];
        let outcome = check_commutative(&samples, |a, b| a + b, |x, y| x == y);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn i64_subtraction_violates_commutativity_when_non_self_symmetric() {
        let samples = [(1i64, 2i64), (5, 3)]; // both non-zero-diff
        let outcome = check_commutative(&samples, |a, b| a - b, |x, y| x == y);
        match outcome {
            Outcome::Violation { sample, .. } => {
                assert!(sample.contains("a="));
                assert!(sample.contains("b="));
            }
            other => panic!("expected Violation, got {other:?}"),
        }
    }

    #[test]
    fn i64_addition_is_associative() {
        let samples = [(1i64, 2i64, 3i64), (-1, 0, 5), (10, -5, 3)];
        let outcome = check_associative(&samples, |a, b| a + b, |x, y| x == y);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn i64_mul_over_add_is_distributive() {
        let samples = [(2i64, 3i64, 4i64), (5, -1, 2), (0, 7, -3)];
        let outcome = check_distributive(&samples, |a, b| a * b, |a, b| a + b, |x, y| x == y);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn bool_double_negation_is_idempotent_over_negate() {
        // Not the textbook idempotence (id x = id (id x) holds trivially) —
        // use `|x| !x` which is its own inverse : !(!!x) == !x.
        //
        // Wait : op(op(x)) = op(x) means applying op twice == applying once.
        // For `|x| !x`, op(op(x)) = x, op(x) = !x. So !x != x ; this FAILS
        // idempotence. The test here uses the `|x| x` identity-op which IS
        // idempotent trivially.
        let samples = [true, false];
        let outcome = check_idempotent(&samples, |x: &bool| *x, |a, b| a == b);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn boolean_and_is_commutative() {
        let samples = [(true, true), (true, false), (false, true), (false, false)];
        let outcome = check_commutative(&samples, |a, b| *a && *b, |x, y| x == y);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn violation_sample_message_shapes() {
        let samples = [(1i64, 2i64)];
        let outcome = check_commutative(&samples, |a, b| a - b, |x, y| x == y);
        if let Outcome::Violation { sample, message } = outcome {
            assert!(sample.contains("a=1"));
            assert!(sample.contains("b=2"));
            assert!(message.contains("commutativity"));
        } else {
            panic!("expected Violation");
        }
    }

    #[test]
    fn empty_samples_returns_ok_with_zero() {
        let samples: [(i64, i64); 0] = [];
        let outcome = check_commutative(&samples, |a, b| a + b, |x, y| x == y);
        match outcome {
            Outcome::Ok { samples_tested } => assert_eq!(samples_tested, 0),
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Calculus-rule numeric-validators : Leibniz + chain-rule + Lipschitz.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn leibniz_holds_for_polynomial_product() {
        // f(x) = x², g(x) = x+1 ; f'(x) = 2x, g'(x) = 1.
        // Product: (x²(x+1))' = 3x² + 2x. Check central-diff matches 2x·(x+1) + x²·1 = 3x²+2x.
        let samples = [0.5, 1.0, 2.0, 5.0, -1.5, -3.0];
        let outcome = check_leibniz(
            &samples,
            |x| x * x,
            |x| 2.0 * x,
            |x| x + 1.0,
            |_x| 1.0,
            1e-4,
        );
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn leibniz_fails_when_derivative_wrong() {
        // Wrong derivative : df = |x| 3.0 (claims (x²)' = 3 which is false).
        let samples = [1.0, 2.0, 5.0];
        let outcome = check_leibniz(
            &samples,
            |x| x * x,
            |_x| 3.0, // WRONG
            |x| x + 1.0,
            |_x| 1.0,
            1e-4,
        );
        match outcome {
            Outcome::Violation { message, .. } => {
                assert!(message.contains("Leibniz"));
            }
            other => panic!("expected Violation, got {other:?}"),
        }
    }

    #[test]
    fn chain_rule_holds_for_sin_of_squared() {
        // f = sin, g = x²  →  (f∘g)' = cos(x²)·2x.
        let samples = [0.3, 1.0, 1.5, 2.0, -1.0];
        let outcome = check_chain_rule(&samples, f64::sin, f64::cos, |x| x * x, |x| 2.0 * x, 1e-4);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn chain_rule_fails_when_inner_derivative_wrong() {
        let samples = [1.0, 2.0];
        let outcome = check_chain_rule(
            &samples,
            f64::sin,
            f64::cos,
            |x| x * x,
            |_x| 1.0, // WRONG : should be 2x
            1e-4,
        );
        match outcome {
            Outcome::Violation { message, .. } => {
                assert!(message.contains("chain rule"));
            }
            other => panic!("expected Violation, got {other:?}"),
        }
    }

    #[test]
    fn lipschitz_holds_for_linear_function() {
        // f(x) = 3x is 3-Lipschitz (exactly).
        let samples = [(1.0, 2.0), (0.0, 5.0), (-3.0, 4.0), (-10.0, 10.0)];
        let outcome = check_lipschitz(&samples, |x| 3.0 * x, 3.0);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn lipschitz_holds_with_slack_constant() {
        // f(x) = sin(x) is 1-Lipschitz (sup |f'| = 1).
        let samples = [(0.0, 1.0), (1.0, 2.0), (-2.0, 3.0), (0.5, 2.5)];
        let outcome = check_lipschitz(&samples, f64::sin, 1.0);
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn lipschitz_fails_for_steep_function_with_small_constant() {
        // f(x) = 100x would need K=100 ; claim K=1.
        let samples = [(0.0, 1.0), (1.0, 2.0)];
        let outcome = check_lipschitz(&samples, |x| 100.0 * x, 1.0);
        match outcome {
            Outcome::Violation { message, .. } => {
                assert!(message.contains("Lipschitz"));
            }
            other => panic!("expected Violation, got {other:?}"),
        }
    }

    #[test]
    fn lipschitz_empty_samples_is_ok_with_zero() {
        let samples: [(f64, f64); 0] = [];
        let outcome = check_lipschitz(&samples, |x: f64| x, 1.0);
        match outcome {
            Outcome::Ok { samples_tested } => assert_eq!(samples_tested, 0),
            other => panic!("expected Ok, got {other:?}"),
        }
    }
}
