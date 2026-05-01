//! § policy
//! ════════════════════════════════════════════════════════════════
//! Higher-level handoff policy : layered above pure topology.
//!
//! Policy axes :
//!   • allow_loopback                — permit `from == to` self-loops.
//!   • max_chain_length              — cap consecutive handoffs.
//!   • require_sovereign_for_coder   — any handoff INTO Coder requires
//!     `sovereign_used == true` on the candidate.

use serde::{Deserialize, Serialize};

use crate::handoff::Handoff;
use crate::role::Role;
use crate::state_machine::HandoffStateMachine;

/// Configurable policy applied on top of topology validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffPolicy {
    /// If false, `from == to` candidates are flagged.
    pub allow_loopback: bool,
    /// Maximum length of recent-history chain before deny.
    pub max_chain_length: u32,
    /// If true, any handoff with `to == Coder` requires sovereign.
    pub require_sovereign_for_coder: bool,
}

/// Decision returned by [`HandoffPolicy::evaluate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Candidate handoff is permitted unconditionally.
    Allow,
    /// Candidate is permitted but flagged with an explanatory tag.
    AllowWithFlag(String),
    /// Candidate is denied with an explanatory reason.
    Deny(String),
}

/// Default policy : loopback ALLOWED with flag, max-chain=8,
/// sovereign required for Coder.
#[must_use]
pub fn default_policy() -> HandoffPolicy {
    HandoffPolicy {
        allow_loopback: true,
        max_chain_length: 8,
        require_sovereign_for_coder: true,
    }
}

impl HandoffPolicy {
    /// Evaluate a candidate handoff against the current state-machine.
    ///
    /// Note : topological validity is the caller's responsibility
    /// (the state-machine validates before invoking policy). This
    /// function focuses on the *policy* axes only.
    #[must_use]
    pub fn evaluate(
        &self,
        sm: &HandoffStateMachine,
        candidate: &Handoff,
    ) -> PolicyDecision {
        // 1) Coder-sovereign gate.
        if self.require_sovereign_for_coder
            && candidate.to == Role::Coder
            && !candidate.sovereign_used
        {
            return PolicyDecision::Deny(format!(
                "handoff to Coder requires sovereign attestation (from={})",
                candidate.from
            ));
        }

        // 2) Loopback.
        if candidate.from == candidate.to {
            if !self.allow_loopback {
                return PolicyDecision::Deny(format!(
                    "loopback {} → {} disallowed",
                    candidate.from, candidate.to
                ));
            }
            return PolicyDecision::AllowWithFlag(format!(
                "loopback {}→{}",
                candidate.from, candidate.to
            ));
        }

        // 3) Max-chain (history depth — counts retained entries).
        let chain = u32::try_from(sm.history().len()).unwrap_or(u32::MAX);
        if chain >= self.max_chain_length {
            return PolicyDecision::Deny(format!(
                "max_chain_length={} reached",
                self.max_chain_length
            ));
        }

        PolicyDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(from: Role, to: Role, sovereign: bool) -> Handoff {
        Handoff::new(from, to, "policy-test".to_string(), vec![], 1, sovereign)
    }

    #[test]
    fn default_allows_dm_to_gm() {
        let p = default_policy();
        let sm = HandoffStateMachine::new(Role::Dm, 8);
        let c = cand(Role::Dm, Role::Gm, false);
        assert!(matches!(p.evaluate(&sm, &c), PolicyDecision::Allow));
    }

    #[test]
    fn default_denies_dm_to_coder_without_sovereign() {
        let p = default_policy();
        let sm = HandoffStateMachine::new(Role::Dm, 8);
        let c = cand(Role::Dm, Role::Coder, false);
        match p.evaluate(&sm, &c) {
            PolicyDecision::Deny(msg) => assert!(msg.contains("sovereign")),
            other => panic!("expected Deny, got {other:?}"),
        }
        // sovereign-attested candidate is allowed
        let c2 = cand(Role::Dm, Role::Coder, true);
        assert!(matches!(p.evaluate(&sm, &c2), PolicyDecision::Allow));
    }

    #[test]
    fn loopback_flagged_when_allowed_and_denied_when_not() {
        let p_allow = default_policy();
        let sm = HandoffStateMachine::new(Role::Gm, 8);
        let c = cand(Role::Gm, Role::Gm, false);
        match p_allow.evaluate(&sm, &c) {
            PolicyDecision::AllowWithFlag(tag) => assert!(tag.contains("loopback")),
            other => panic!("expected AllowWithFlag, got {other:?}"),
        }
        let p_deny = HandoffPolicy { allow_loopback: false, ..default_policy() };
        match p_deny.evaluate(&sm, &c) {
            PolicyDecision::Deny(msg) => assert!(msg.contains("loopback")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn max_chain_deny() {
        let p = HandoffPolicy { max_chain_length: 2, ..default_policy() };
        let mut sm = HandoffStateMachine::new(Role::Dm, 16);
        // fill history to chain-cap
        sm.handoff(Role::Gm, "a".into(), vec![], 1, false).unwrap();
        sm.handoff(Role::Dm, "b".into(), vec![], 2, false).unwrap();
        let c = cand(Role::Dm, Role::Gm, false);
        match p.evaluate(&sm, &c) {
            PolicyDecision::Deny(msg) => assert!(msg.contains("max_chain")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn sovereign_bypass_coder_path() {
        let p = default_policy();
        let sm = HandoffStateMachine::new(Role::Dm, 8);
        let c = cand(Role::Dm, Role::Coder, true);
        assert!(matches!(p.evaluate(&sm, &c), PolicyDecision::Allow));

        // policy with sovereign-not-required also allows
        let p2 = HandoffPolicy { require_sovereign_for_coder: false, ..default_policy() };
        let c2 = cand(Role::Dm, Role::Coder, false);
        assert!(matches!(p2.evaluate(&sm, &c2), PolicyDecision::Allow));
    }
}
