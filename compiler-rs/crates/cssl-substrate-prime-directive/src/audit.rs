//! Audit-chain integration for the H6 enforcement layer.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` § AUDIT-CHAIN + `specs/30_SUBSTRATE.csl`
//!   § PRIME_DIRECTIVE-ALIGNMENT § AUDIT-CHAIN-INTEGRATION + R18 telemetry-ring
//!   invariant.
//!
//! § DESIGN
//!   - [`EnforcementAuditBus`] wraps a [`cssl_telemetry::AuditChain`] and
//!     exposes typed `record_*` methods that produce stable tag-strings +
//!     deterministic message bodies.
//!   - Every cap-grant + revoke + violation emits an entry. Per `specs/22_TELEMETRY.csl`
//!     the chain is APPEND-ONLY ; we never mutate or remove entries.
//!   - The bus also receives orphan-drop events from
//!     [`crate::cap::CapToken`]'s `Drop` impl via a private process-wide
//!     channel. Tests can intercept by grabbing the
//!     `audit_chain_for_test` handle (feature-gated `test-bypass`).
//!
//! § INVARIANTS
//!   - Audit-append failure ⇒ panic (`record_*` panics on internal allocation
//!     failures so the chain is never silently dropped). Per `specs/22_TELEMETRY.csl`
//!     § PRIME-DIRECTIVE : `audit-append-failure → process-abort`.
//!   - Tag strings are stable across builds (we test for them).
//!   - Entry order matches issuance order (chain seq is monotonic).

use std::sync::Mutex;

use cssl_telemetry::audit::AuditChain;
use cssl_telemetry::audit::AuditEntry;

use crate::cap::{CapTokenId, SubstrateCap};
use crate::consent::ConsentScope;
use crate::halt::HaltReason;
use crate::harm::Prohibition;
use crate::sigma::SigmaMaskPacked;

/// Kinds of enforcement events that can land on the audit-chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    /// A cap-grant was issued (interactive or test-bypass).
    GrantIssued {
        token_id: CapTokenId,
        cap: SubstrateCap,
        principal: String,
        purpose: String,
        test_bypass: bool,
    },
    /// A cap-grant attempt was denied.
    GrantDenied {
        cap: SubstrateCap,
        principal: String,
        purpose: String,
        reason: String,
    },
    /// A cap was revoked (token consumed by `caps_revoke`).
    Revoked {
        token_id: CapTokenId,
        cap: SubstrateCap,
    },
    /// A [`crate::cap::CapToken`] was dropped without being consumed. Visible
    /// (not fatal) — the bus records the leak so it can be inspected.
    OrphanDrop {
        token_id: CapTokenId,
        cap: SubstrateCap,
    },
    /// A harm-prevention check fired and rejected a Substrate op.
    HarmCheckFailed {
        prohibition: Prohibition,
        site: String,
    },
    /// The kill-switch [`crate::halt::substrate_halt`] was invoked.
    Halted {
        reason: HaltReason,
        outstanding_steps: u32,
    },
    /// Attestation drift was detected (the per-fn ATTESTATION constant
    /// did not match the canonical hash).
    AttestationDrift { site: String },
    /// A per-cell Σ-mask was mutated. The before/after packed-u128 forms are
    /// recorded so the chain can be replayed cell-by-cell.
    /// Per `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT` § II + § VII.
    SigmaMaskMutated {
        site: String,
        before_packed: u128,
        after_packed: u128,
        audit_seq_before: u16,
        audit_seq_after: u16,
    },
}

impl AuditEvent {
    /// The stable tag string used for the chain entry. Renaming = ABI bump.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::GrantIssued { .. } => "h6.grant.issued",
            Self::GrantDenied { .. } => "h6.grant.denied",
            Self::Revoked { .. } => "h6.revoke",
            Self::OrphanDrop { .. } => "h6.orphan-drop",
            Self::HarmCheckFailed { .. } => "h6.harm.failed",
            Self::Halted { .. } => "h6.halt",
            Self::AttestationDrift { .. } => "h6.attestation.drift",
            Self::SigmaMaskMutated { .. } => "h6.sigma.mutated",
        }
    }

    /// Render the event as a deterministic UTF-8 message body. The body
    /// content is what the BLAKE3 chain-hash covers ; we intentionally
    /// keep it short + structured.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::GrantIssued {
                token_id,
                cap,
                principal,
                purpose,
                test_bypass,
            } => format!(
                "issued {token_id} {cap} principal={principal} purpose={purpose} bypass={test_bypass}"
            ),
            Self::GrantDenied {
                cap,
                principal,
                purpose,
                reason,
            } => format!(
                "denied {cap} principal={principal} purpose={purpose} reason={reason}"
            ),
            Self::Revoked { token_id, cap } => format!("revoked {token_id} {cap}"),
            Self::OrphanDrop { token_id, cap } => format!("orphan-drop {token_id} {cap}"),
            Self::HarmCheckFailed { prohibition, site } => {
                format!("harm-check {} site={site}", prohibition.code())
            }
            Self::Halted {
                reason,
                outstanding_steps,
            } => format!(
                "halt reason={} outstanding_steps={outstanding_steps}",
                reason.canonical_name()
            ),
            Self::AttestationDrift { site } => format!("attestation-drift site={site}"),
            Self::SigmaMaskMutated {
                site,
                before_packed,
                after_packed,
                audit_seq_before,
                audit_seq_after,
            } => format!(
                "sigma-mask-mutated site={site} before={before_packed:#034x} after={after_packed:#034x} seq={audit_seq_before}->{audit_seq_after}"
            ),
        }
    }
}

/// Audit-bus owned by a [`crate::consent::ConsentStore`].
///
/// § DESIGN
///   - Wraps an [`AuditChain`] (from `cssl-telemetry`).
///   - Every event is appended with a deterministic timestamp (caller-supplied
///     or `0`) so unit-tests can produce byte-identical chains.
#[derive(Debug)]
pub struct EnforcementAuditBus {
    chain: AuditChain,
    /// Monotonic synthetic timestamp used when no explicit timestamp is
    /// passed. Real production builds use OS-clock per `specs/22_TELEMETRY.csl`
    /// § ring-implementation.
    next_ts: u64,
}

impl EnforcementAuditBus {
    /// Construct a bus with a stub-signed empty chain.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chain: AuditChain::new(),
            next_ts: 0,
        }
    }

    /// Construct a bus around an existing (possibly signed) chain.
    #[must_use]
    pub fn from_chain(chain: AuditChain) -> Self {
        Self { chain, next_ts: 0 }
    }

    /// Inspect the inner chain (read-only).
    #[must_use]
    pub const fn chain(&self) -> &AuditChain {
        &self.chain
    }

    /// Number of entries currently in the chain.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.chain.len()
    }

    /// Iterate entries in chain order. Borrowed from the underlying chain.
    pub fn iter(&self) -> impl Iterator<Item = &AuditEntry> {
        self.chain.iter()
    }

    /// Append `event` to the chain. Internal — every public `record_*`
    /// method routes here.
    fn append(&mut self, event: &AuditEvent) {
        let ts = self.next_ts;
        self.next_ts = self.next_ts.saturating_add(1);
        self.chain
            .append(event.tag().to_string(), event.message(), ts);
    }

    // ── Public typed recorders ──────────────────────────────────────────

    pub fn record_grant_issued(
        &mut self,
        token_id: CapTokenId,
        cap: SubstrateCap,
        scope: &ConsentScope,
        test_bypass: bool,
    ) {
        let event = AuditEvent::GrantIssued {
            token_id,
            cap,
            principal: scope.principal.clone(),
            purpose: scope.purpose.clone(),
            test_bypass,
        };
        self.append(&event);
    }

    pub fn record_grant_denied(
        &mut self,
        cap: SubstrateCap,
        scope: &ConsentScope,
        reason: impl Into<String>,
    ) {
        let event = AuditEvent::GrantDenied {
            cap,
            principal: scope.principal.clone(),
            purpose: scope.purpose.clone(),
            reason: reason.into(),
        };
        self.append(&event);
    }

    pub fn record_revoke(&mut self, token_id: CapTokenId, cap: SubstrateCap) {
        let event = AuditEvent::Revoked { token_id, cap };
        self.append(&event);
    }

    pub fn record_orphan_drop(&mut self, token_id: CapTokenId, cap: SubstrateCap) {
        let event = AuditEvent::OrphanDrop { token_id, cap };
        self.append(&event);
    }

    pub fn record_harm_check_failed(&mut self, prohibition: Prohibition, site: impl Into<String>) {
        let event = AuditEvent::HarmCheckFailed {
            prohibition,
            site: site.into(),
        };
        self.append(&event);
    }

    pub fn record_halted(&mut self, reason: HaltReason, outstanding_steps: u32) {
        let event = AuditEvent::Halted {
            reason,
            outstanding_steps,
        };
        self.append(&event);
    }

    pub fn record_attestation_drift(&mut self, site: impl Into<String>) {
        let event = AuditEvent::AttestationDrift { site: site.into() };
        self.append(&event);
    }

    /// Record a Σ-mask mutation : before / after packed-u128 + audit-seq
    /// transition. Called from [`crate::sigma::SigmaMaskPacked::mutate`].
    pub fn record_sigma_mask_mutated(
        &mut self,
        before: SigmaMaskPacked,
        after: SigmaMaskPacked,
        site: impl Into<String>,
    ) {
        let event = AuditEvent::SigmaMaskMutated {
            site: site.into(),
            before_packed: before.to_u128(),
            after_packed: after.to_u128(),
            audit_seq_before: before.audit_seq(),
            audit_seq_after: after.audit_seq(),
        };
        self.append(&event);
    }
}

impl Default for EnforcementAuditBus {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § PROCESS-BUS — orphan-drop sink.
//
// `CapToken::drop` does not have access to a `&mut ConsentStore`. We give
// it a process-wide bus that aggregates orphan-drops. Tests can swap this
// out via [`audit_chain_for_test`].
// ───────────────────────────────────────────────────────────────────────

static PROCESS_BUS: Mutex<Option<EnforcementAuditBus>> = Mutex::new(None);

/// Record an orphan-drop on the process-wide bus. Called from
/// [`crate::cap::CapToken::drop`]. Lazy-initializes the bus on first use.
pub(crate) fn record_orphan_drop(token_id: CapTokenId, cap: SubstrateCap) {
    let mut guard = PROCESS_BUS.lock().expect("process audit bus lock poisoned");
    if guard.is_none() {
        *guard = Some(EnforcementAuditBus::new());
    }
    if let Some(bus) = guard.as_mut() {
        bus.record_orphan_drop(token_id, cap);
    }
}

/// Test-only handle to inspect the process-wide audit-bus.
///
/// Returns the entry count + a vector of (tag, message) pairs since the
/// last call. The bus retains entries (it is append-only) ; this helper
/// exposes them for assertion. Tests using this should use
/// `--test-threads=1` to avoid cross-test interference.
#[cfg(any(test, feature = "test-bypass"))]
#[must_use]
pub fn audit_chain_for_test() -> Vec<(String, String)> {
    let guard = PROCESS_BUS.lock().expect("process audit bus lock poisoned");
    guard.as_ref().map_or_else(Vec::new, |bus| {
        bus.iter()
            .map(|e| (e.tag.clone(), e.message.clone()))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{AuditEvent, EnforcementAuditBus};
    use crate::cap::{CapTokenId, SubstrateCap};
    use crate::consent::ConsentScope;
    use crate::halt::HaltReason;
    use crate::harm::Prohibition;

    #[test]
    fn audit_event_tags_are_stable_strings() {
        // Pin tag strings ; downstream consumers (telemetry exporters) rely
        // on these. Renaming = ABI break per `specs/22_TELEMETRY.csl`.
        assert_eq!(
            AuditEvent::GrantIssued {
                token_id: CapTokenId(0),
                cap: SubstrateCap::OmegaRegister,
                principal: "p".into(),
                purpose: "u".into(),
                test_bypass: false,
            }
            .tag(),
            "h6.grant.issued"
        );
        assert_eq!(
            AuditEvent::GrantDenied {
                cap: SubstrateCap::OmegaRegister,
                principal: "p".into(),
                purpose: "u".into(),
                reason: "r".into(),
            }
            .tag(),
            "h6.grant.denied"
        );
        assert_eq!(
            AuditEvent::Revoked {
                token_id: CapTokenId(0),
                cap: SubstrateCap::OmegaRegister
            }
            .tag(),
            "h6.revoke"
        );
        assert_eq!(
            AuditEvent::OrphanDrop {
                token_id: CapTokenId(0),
                cap: SubstrateCap::OmegaRegister
            }
            .tag(),
            "h6.orphan-drop"
        );
        assert_eq!(
            AuditEvent::HarmCheckFailed {
                prohibition: Prohibition::Harm,
                site: "x".into()
            }
            .tag(),
            "h6.harm.failed"
        );
        assert_eq!(
            AuditEvent::Halted {
                reason: HaltReason::User,
                outstanding_steps: 0
            }
            .tag(),
            "h6.halt"
        );
        assert_eq!(
            AuditEvent::AttestationDrift { site: "y".into() }.tag(),
            "h6.attestation.drift"
        );
    }

    #[test]
    fn bus_record_grant_issued_appends_one_entry() {
        let mut bus = EnforcementAuditBus::new();
        let scope = ConsentScope::for_purpose("test", "system");
        bus.record_grant_issued(CapTokenId(7), SubstrateCap::ObserverShare, &scope, true);
        assert_eq!(bus.entry_count(), 1);
        let entries: Vec<_> = bus.iter().collect();
        assert_eq!(entries[0].tag, "h6.grant.issued");
        assert!(entries[0].message.contains("cap-token#7"));
        assert!(entries[0].message.contains("observer_share"));
    }

    #[test]
    fn bus_record_grant_denied_appends_with_reason() {
        let mut bus = EnforcementAuditBus::new();
        let scope = ConsentScope::for_purpose("p", "system");
        bus.record_grant_denied(SubstrateCap::SavePath, &scope, "denied-by-policy");
        assert_eq!(bus.entry_count(), 1);
        let e = bus.iter().next().unwrap();
        assert!(e.message.contains("denied-by-policy"));
    }

    #[test]
    fn bus_record_revoke_appends_with_token() {
        let mut bus = EnforcementAuditBus::new();
        bus.record_revoke(CapTokenId(99), SubstrateCap::DebugCamera);
        assert_eq!(bus.entry_count(), 1);
        let e = bus.iter().next().unwrap();
        assert!(e.message.contains("cap-token#99"));
        assert!(e.message.contains("debug_camera"));
    }

    #[test]
    fn bus_chain_verifies_after_many_appends() {
        let mut bus = EnforcementAuditBus::new();
        let scope = ConsentScope::for_purpose("p", "system");
        for i in 0..50 {
            bus.record_grant_issued(CapTokenId(i), SubstrateCap::OmegaRegister, &scope, false);
        }
        bus.chain().verify_chain().expect("chain must verify");
        assert_eq!(bus.entry_count(), 50);
    }

    #[test]
    fn bus_records_halt_event() {
        let mut bus = EnforcementAuditBus::new();
        bus.record_halted(HaltReason::User, 3);
        let e = bus.iter().next().unwrap();
        assert_eq!(e.tag, "h6.halt");
        assert!(e.message.contains("user"));
        assert!(e.message.contains("outstanding_steps=3"));
    }

    #[test]
    fn bus_records_harm_check_with_pd_code() {
        let mut bus = EnforcementAuditBus::new();
        bus.record_harm_check_failed(Prohibition::Surveillance, "omega_step.read_sensor");
        let e = bus.iter().next().unwrap();
        assert!(e.message.contains("PD0004"));
        assert!(e.message.contains("omega_step.read_sensor"));
    }

    #[test]
    fn audit_event_messages_are_deterministic() {
        let event_a = AuditEvent::Revoked {
            token_id: CapTokenId(5),
            cap: SubstrateCap::SavePath,
        };
        let event_b = AuditEvent::Revoked {
            token_id: CapTokenId(5),
            cap: SubstrateCap::SavePath,
        };
        assert_eq!(event_a.message(), event_b.message());
    }
}
