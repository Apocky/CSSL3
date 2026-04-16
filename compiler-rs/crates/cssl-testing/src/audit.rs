//! Audit-chain oracle (`@audit_test`) — §§ 11 IFC + §§ 22 TELEMETRY cross-cut.
//!
//! § SPEC   : `specs/23_TESTING.csl` § audit-tests + `specs/22_TELEMETRY.csl` audit-chain.
//! § ROLE   : verify `{Audit<dom>}` events emitted correctly, audit-chain Ed25519-signature
//!            is valid, all declass-events recorded, PRIME-DIRECTIVE violations
//!            trigger expected compile-error (negative tests).
//! § GATE   : T25 + T11 theorem discharge.
//! § STATUS : T11 stub — implementation pending.

/// Config for the `@audit_test` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Audit domain filter (empty = all domains).
    pub domain_filter: Option<String>,
    /// If `true`, run negative-tests verifying PRIME-DIRECTIVE violations compile-error.
    pub check_negative_cases: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            domain_filter: None,
            check_negative_cases: true,
        }
    }
}

/// Outcome of running the `@audit_test` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11 (requires `cssl-ifc` + `cssl-telemetry`).
    Stage0Unimplemented,
    /// Audit chain intact; all required events present; negative cases compile-error.
    Ok { events_verified: u64 },
    /// Audit chain tampering detected.
    ChainTampered { first_broken_index: u64 },
    /// Expected audit event missing.
    EventMissing {
        expected_domain: String,
        expected_kind: String,
    },
    /// Negative test unexpectedly compiled (PRIME-DIRECTIVE violation slipped through).
    NegativeCaseCompiled { case: String },
}

/// Dispatcher trait for `@audit_test` oracle.
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
