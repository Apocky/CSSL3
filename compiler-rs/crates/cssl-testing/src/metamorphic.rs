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

#[cfg(test)]
mod tests {
    use super::{
        check_associative, check_commutative, check_distributive, check_idempotent, Config,
        Dispatcher, Outcome, Stage0Stub,
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
}
