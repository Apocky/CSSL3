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

#[cfg(test)]
mod tests {
    use super::{Config, Dispatcher, Outcome, Stage0Stub};

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }
}
