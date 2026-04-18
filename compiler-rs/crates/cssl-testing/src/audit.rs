//! Audit-chain oracle (`@audit_test`) — §§ 11 IFC + §§ 22 TELEMETRY cross-cut.
//!
//! § SPEC   : `specs/23_TESTING.csl` § audit-tests + `specs/22_TELEMETRY.csl` audit-chain.
//! § ROLE   : verify `{Audit<dom>}` events emitted correctly, audit-chain Ed25519-signature
//!            is valid, all declass-events recorded, PRIME-DIRECTIVE violations
//!            trigger expected compile-error (negative tests).
//! § GATE   : T25 + T11 theorem discharge.
//! § STATUS : T11-phase-2b live (chain-invariant verification via `cssl_telemetry::AuditChain`) ;
//!            domain-filter + negative-test harness deferred to T11-phase-2c
//!            (requires `cssl-ifc` + `cssl-macros` for compile-error assertions).

use cssl_telemetry::audit::{AuditChain, AuditError};

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

// ─────────────────────────────────────────────────────────────────────────
// § Live verifier : validates AuditChain structural invariants + sig-chain.
// ─────────────────────────────────────────────────────────────────────────

/// Verify the chain-invariant of `chain` : hash-linkage + sequence-monotonicity +
/// signature-validity (if a signing-key is attached). `config.domain_filter`
/// restricts required-event-checks to entries whose `tag` starts with the given
/// prefix ; an empty filter (default) verifies all entries.
///
/// § MAPPING :
///   - chain.verify_chain() == Ok  → `Ok { events_verified }`
///   - ChainBreak / GenesisPrevNonZero → `ChainTampered { first_broken_index }`
///   - InvalidSequence / SignatureInvalid → `ChainTampered` (same bucket : tampered-or-corrupted)
///
/// `required_events` : optional list of `(domain_prefix, kind_substring)` pairs
/// that MUST appear somewhere in the chain (post-filter). Missing triples
/// produce `EventMissing`.
pub fn run_audit_verify(
    config: &Config,
    chain: &AuditChain,
    required_events: &[(&str, &str)],
) -> Outcome {
    if let Err(e) = chain.verify_chain() {
        let idx = match e {
            AuditError::GenesisPrevNonZero | AuditError::SignatureInvalid => 0,
            AuditError::ChainBreak { seq } | AuditError::InvalidSequence { actual: seq, .. } => seq,
        };
        return Outcome::ChainTampered {
            first_broken_index: idx,
        };
    }

    let filter = config.domain_filter.as_deref().unwrap_or("");
    let filtered: Vec<_> = chain
        .iter()
        .filter(|e| filter.is_empty() || e.tag.starts_with(filter))
        .collect();

    for (domain, kind) in required_events {
        let found = filtered
            .iter()
            .any(|e| e.tag.starts_with(domain) && e.message.contains(kind));
        if !found {
            return Outcome::EventMissing {
                expected_domain: (*domain).to_string(),
                expected_kind: (*kind).to_string(),
            };
        }
    }

    Outcome::Ok {
        events_verified: filtered.len() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::{run_audit_verify, Config, Dispatcher, Outcome, Stage0Stub};
    use cssl_telemetry::audit::AuditChain;

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }

    #[test]
    fn valid_chain_verifies_with_no_required_events() {
        let mut chain = AuditChain::new();
        chain.append("declass.audit", "user=alice action=read", 100);
        chain.append("declass.audit", "user=alice action=write", 101);
        let outcome = run_audit_verify(&Config::default(), &chain, &[]);
        match outcome {
            Outcome::Ok { events_verified } => assert_eq!(events_verified, 2),
            other => panic!("expected Ok(2), got {other:?}"),
        }
    }

    #[test]
    fn required_events_found_reports_ok() {
        let mut chain = AuditChain::new();
        chain.append("declass", "user=alice action=read", 100);
        chain.append("power", "watts=42 breach=false", 101);
        let outcome = run_audit_verify(
            &Config::default(),
            &chain,
            &[("declass", "alice"), ("power", "watts")],
        );
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn missing_required_event_reports_event_missing() {
        let mut chain = AuditChain::new();
        chain.append("declass", "user=alice", 100);
        let outcome = run_audit_verify(&Config::default(), &chain, &[("power", "breach")]);
        match outcome {
            Outcome::EventMissing {
                expected_domain,
                expected_kind,
            } => {
                assert_eq!(expected_domain, "power");
                assert_eq!(expected_kind, "breach");
            }
            other => panic!("expected EventMissing, got {other:?}"),
        }
    }

    #[test]
    fn domain_filter_restricts_verification() {
        let mut chain = AuditChain::new();
        chain.append("declass.sensitive", "user=alice", 100);
        chain.append("power.telemetry", "watts=42", 101);
        chain.append("declass.audit", "user=bob", 102);
        let config = Config {
            domain_filter: Some("declass".to_string()),
            check_negative_cases: false,
        };
        let outcome = run_audit_verify(&config, &chain, &[]);
        match outcome {
            Outcome::Ok { events_verified } => assert_eq!(events_verified, 2),
            other => panic!("expected Ok(2), got {other:?}"),
        }
    }

    #[test]
    fn empty_chain_verifies_to_zero_events() {
        let chain = AuditChain::new();
        let outcome = run_audit_verify(&Config::default(), &chain, &[]);
        match outcome {
            Outcome::Ok { events_verified } => assert_eq!(events_verified, 0),
            other => panic!("expected Ok(0), got {other:?}"),
        }
    }

    #[test]
    fn chain_with_signing_key_verifies_real_signatures() {
        use cssl_telemetry::audit::SigningKey;
        let key = SigningKey::from_seed([7u8; 32]);
        let mut chain = AuditChain::with_signing_key(key);
        chain.append("declass", "user=alice", 100);
        chain.append("declass", "user=bob", 101);
        chain.append("declass", "user=carol", 102);
        let outcome = run_audit_verify(&Config::default(), &chain, &[]);
        match outcome {
            Outcome::Ok { events_verified } => assert_eq!(events_verified, 3),
            other => panic!("expected Ok(3), got {other:?}"),
        }
    }
}
