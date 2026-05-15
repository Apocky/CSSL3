#![forbid(unsafe_code)]
#![doc = "cssl-grades — graded-modal-type kernel.\n\n\
Spec: `specs/Upgrade/impl/IMPL_02_FOUNDATION.csl` § cssl-grades. \
Provides the abstract `Semiring` trait + several initial grade-instances \
(`Linear`, `Affine`, `Unrestricted`, `Nat`, `Privacy`, `Trust`). The substructural \
discipline of bindings is recovered as a property of which semiring annotates them."]

/// Algebraic semiring with `0`, `1`, `⊕`, `⊗`.
///
/// Implementations MUST satisfy :
/// - `(plus, zero)` is a commutative monoid
/// - `(times, one)` is a monoid
/// - `times` distributes over `plus` from both sides
/// - `zero` annihilates `times`
pub trait Semiring: Clone + PartialEq + Eq {
    /// Additive identity.
    fn zero() -> Self;
    /// Multiplicative identity.
    fn one() -> Self;
    /// Additive operation `⊕`.
    fn plus(self, other: Self) -> Self;
    /// Multiplicative operation `⊗`.
    fn times(self, other: Self) -> Self;
}

/// A value annotated with a grade.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Graded<G: Semiring, T> {
    pub grade: G,
    pub value: T,
}

impl<G: Semiring, T> Graded<G, T> {
    /// Construct a graded value.
    pub const fn new(grade: G, value: T) -> Self {
        Self { grade, value }
    }
}

// ─── Initial grade-instances ─────────────────────────────────────────────────

/// Linear-affine semiring : `{0, 1, ω}`.
///
/// `1 ⊕ 1 = ω` (more than one use → unrestricted).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Linear {
    Zero,
    One,
    Many,
}

impl Semiring for Linear {
    fn zero() -> Self { Self::Zero }
    fn one() -> Self { Self::One }
    fn plus(self, other: Self) -> Self {
        use Linear::*;
        match (self, other) {
            (Zero, x) | (x, Zero) => x,
            (One, One) => Many,
            _ => Many,
        }
    }
    fn times(self, other: Self) -> Self {
        use Linear::*;
        match (self, other) {
            (Zero, _) | (_, Zero) => Zero,
            (One, x) | (x, One) => x,
            (Many, Many) => Many,
        }
    }
}

/// Affine semiring : `{0, 1, ω}` ; identical algebra to `Linear` but conceptually
/// "may drop" — distinct type so the elaborator can dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Affine {
    Zero,
    One,
    Many,
}

impl Semiring for Affine {
    fn zero() -> Self { Self::Zero }
    fn one() -> Self { Self::One }
    fn plus(self, other: Self) -> Self {
        use Affine::*;
        match (self, other) {
            (Zero, x) | (x, Zero) => x,
            _ => Many,
        }
    }
    fn times(self, other: Self) -> Self {
        use Affine::*;
        match (self, other) {
            (Zero, _) | (_, Zero) => Zero,
            (One, x) | (x, One) => x,
            (Many, Many) => Many,
        }
    }
}

/// Unrestricted (classical) semiring : a single inhabitant that absorbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Unrestricted;

impl Semiring for Unrestricted {
    fn zero() -> Self { Self }
    fn one() -> Self { Self }
    fn plus(self, _: Self) -> Self { Self }
    fn times(self, _: Self) -> Self { Self }
}

/// Natural-number semiring : exact-count multiplicities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Nat(pub u64);

impl Semiring for Nat {
    fn zero() -> Self { Self(0) }
    fn one() -> Self { Self(1) }
    fn plus(self, other: Self) -> Self { Self(self.0.saturating_add(other.0)) }
    fn times(self, other: Self) -> Self { Self(self.0.saturating_mul(other.0)) }
}

/// Privacy budget : ε-differential-privacy.
///
/// NOT a `Semiring` because non-negative-finite-`ε` lacks an annihilator under
/// "+" composition. Provided as a distinct type with explicit `compose_parallel`
/// (= `max`) and `compose_sequential` (= `+`) methods. The full tropical-semiring
/// formulation (with `-∞` as additive identity) is deferred to a follow-up wave.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Privacy(pub f64);

impl Eq for Privacy {}

impl Privacy {
    /// Zero-budget (no privacy cost incurred).
    pub const ZERO: Self = Self(0.0);

    /// Parallel composition (e.g. mechanisms over disjoint data) : `max`.
    #[must_use]
    pub fn compose_parallel(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    /// Sequential composition (e.g. successive queries on the same data) : `+`.
    #[must_use]
    pub fn compose_sequential(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

/// Trust semiring : tiny lattice `{Low, Med, High}`.
///
/// `⊕` = `max` (join : combining trust evidence raises trust), `⊗` = `min` (meet :
/// composing trust contexts lowers to the weaker). `0 = Low` (additive identity ;
/// annihilation under min). `1 = High` (multiplicative identity).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Trust {
    Low,
    Med,
    High,
}

impl Semiring for Trust {
    fn zero() -> Self { Self::Low }
    fn one() -> Self { Self::High }
    fn plus(self, other: Self) -> Self { self.max(other) }
    fn times(self, other: Self) -> Self { self.min(other) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_semiring_axioms<G: Semiring + std::fmt::Debug>(samples: &[G]) {
        for a in samples {
            // identity laws
            assert_eq!(a.clone().plus(G::zero()), a.clone(), "0 ⊕ a = a");
            assert_eq!(G::zero().plus(a.clone()), a.clone(), "a ⊕ 0 = a");
            assert_eq!(a.clone().times(G::one()), a.clone(), "a ⊗ 1 = a");
            assert_eq!(G::one().times(a.clone()), a.clone(), "1 ⊗ a = a");
            // annihilation
            assert_eq!(a.clone().times(G::zero()), G::zero(), "a ⊗ 0 = 0");
            assert_eq!(G::zero().times(a.clone()), G::zero(), "0 ⊗ a = 0");
            for b in samples {
                // commutativity of ⊕
                assert_eq!(
                    a.clone().plus(b.clone()),
                    b.clone().plus(a.clone()),
                    "⊕ commutative"
                );
                for c in samples {
                    // associativity of ⊕ and ⊗
                    assert_eq!(
                        a.clone().plus(b.clone()).plus(c.clone()),
                        a.clone().plus(b.clone().plus(c.clone())),
                        "⊕ associative"
                    );
                    assert_eq!(
                        a.clone().times(b.clone()).times(c.clone()),
                        a.clone().times(b.clone().times(c.clone())),
                        "⊗ associative"
                    );
                    // distributivity
                    assert_eq!(
                        a.clone().times(b.clone().plus(c.clone())),
                        a.clone().times(b.clone()).plus(a.clone().times(c.clone())),
                        "⊗ distributes over ⊕"
                    );
                }
            }
        }
    }

    #[test]
    fn linear_satisfies_semiring_axioms() {
        assert_semiring_axioms(&[Linear::Zero, Linear::One, Linear::Many]);
    }

    #[test]
    fn affine_satisfies_semiring_axioms() {
        assert_semiring_axioms(&[Affine::Zero, Affine::One, Affine::Many]);
    }

    #[test]
    fn nat_satisfies_semiring_axioms_on_small_values() {
        assert_semiring_axioms(&[Nat(0), Nat(1), Nat(2), Nat(3)]);
    }

    #[test]
    fn unrestricted_collapses_to_single_inhabitant() {
        let u = Unrestricted;
        assert_eq!(u.plus(Unrestricted::zero()), u);
        assert_eq!(u.times(Unrestricted::one()), u);
    }

    #[test]
    fn privacy_parallel_is_max_sequential_is_sum() {
        let a = Privacy(1.0);
        let b = Privacy(2.5);
        assert_eq!(a.compose_parallel(b), Privacy(2.5));
        assert_eq!(a.compose_sequential(b), Privacy(3.5));
    }

    #[test]
    fn trust_join_raises_meet_lowers() {
        assert_eq!(Trust::High.plus(Trust::Low), Trust::High,
            "⊕ joins (raises trust)");
        assert_eq!(Trust::Med.times(Trust::High), Trust::Med,
            "⊗ meets (lowers to weaker)");
    }

    #[test]
    fn trust_satisfies_semiring_axioms() {
        assert_semiring_axioms(&[Trust::Low, Trust::Med, Trust::High]);
    }

    #[test]
    fn graded_value_preserves_grade_through_clone() {
        let g = Graded::new(Linear::One, "x".to_string());
        let g2 = g.clone();
        assert_eq!(g.grade, g2.grade);
        assert_eq!(g.value, g2.value);
    }

    #[test]
    fn linear_one_plus_one_is_many() {
        assert_eq!(Linear::One.plus(Linear::One), Linear::Many);
    }
}
