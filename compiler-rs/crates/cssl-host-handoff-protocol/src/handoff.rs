//! § handoff
//! ════════════════════════════════════════════════════════════════
//! Single handoff record + transition validation.
//!
//! Valid transition matrix (DM-hub topology) :
//!   DM ↔ GM            ✓
//!   DM ↔ Collaborator  ✓
//!   DM ↔ Coder         ✓
//!   GM ↔ Collaborator  ✓
//!   GM ↔ Coder         ✗   (must hop through DM)
//!   Collab ↔ Coder     ✗   (must hop through DM)
//!   X ↔ X              ✓   (loopback — controlled by policy layer)
//!
//! `from == to` represents an intra-role checkpoint and is permitted
//! at the transition layer ; the policy layer can disallow it.

use serde::{Deserialize, Serialize};

use crate::role::Role;

/// Maximum payload size (1 MiB). Larger payloads must use out-of-band
/// references — keeps audit-log replay practical.
pub const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

/// A single inter-role handoff event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handoff {
    /// Originating role.
    pub from: Role,
    /// Receiving role.
    pub to: Role,
    /// Human-readable reason — surfaces in audit log.
    pub reason: String,
    /// Opaque payload bytes (intent, plan-fragment, etc.).
    pub payload: Vec<u8>,
    /// Timestamp in microseconds-since-epoch (caller-supplied for
    /// determinism + replay).
    pub ts_micros: u64,
    /// True iff the originating role acted under sovereign-attestation
    /// (consent-recorded user override). Required for some Coder paths
    /// per `HandoffPolicy::require_sovereign_for_coder`.
    pub sovereign_used: bool,
}

/// Errors surfaced during handoff validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandoffErr {
    /// A role attempted to use a cap that does not belong to it.
    CrossRoleCapBleed {
        /// The role that attempted the bleed.
        attempted: Role,
        /// The bit it tried to wield.
        with_bit: u32,
    },
    /// The (from, to) pair is not in the valid transition matrix.
    InvalidTransition(Role, Role),
    /// Payload exceeded `MAX_PAYLOAD_BYTES`.
    PayloadTooLarge,
}

impl std::fmt::Display for HandoffErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CrossRoleCapBleed { attempted, with_bit } => {
                write!(f, "cross-role-cap-bleed: {attempted} attempted bit 0x{with_bit:x}")
            }
            Self::InvalidTransition(a, b) => {
                write!(f, "invalid transition: {a} → {b} not in handoff matrix")
            }
            Self::PayloadTooLarge => f.write_str("payload exceeds MAX_PAYLOAD_BYTES"),
        }
    }
}

impl std::error::Error for HandoffErr {}

impl Handoff {
    /// Construct a new handoff. No validation — call [`Handoff::validate`].
    #[must_use]
    pub fn new(
        from: Role,
        to: Role,
        reason: String,
        payload: Vec<u8>,
        ts_micros: u64,
        sovereign_used: bool,
    ) -> Self {
        Self { from, to, reason, payload, ts_micros, sovereign_used }
    }

    /// Validate the (from, to) edge + payload size. Cap-bit-bleed is
    /// detected at the higher layer (state-machine + policy) — at this
    /// level we enforce the topology only.
    pub fn validate(&self) -> Result<(), HandoffErr> {
        if self.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(HandoffErr::PayloadTooLarge);
        }
        if Self::is_valid_edge(self.from, self.to) {
            Ok(())
        } else {
            Err(HandoffErr::InvalidTransition(self.from, self.to))
        }
    }

    /// Pure topology check : true iff (from, to) is in the matrix.
    #[must_use]
    pub fn is_valid_edge(from: Role, to: Role) -> bool {
        // self-loops permitted at topology layer ; policy may forbid.
        if from == to {
            return true;
        }
        // DM hub : reaches all three other roles bidirectionally.
        // GM ↔ Collaborator (narrative ↔ co-author bridge).
        // Everything else (incl. GM↔Coder + Collab↔Coder) must hop through DM.
        matches!(
            (from, to),
            (Role::Dm, Role::Gm | Role::Collaborator | Role::Coder)
                | (Role::Gm | Role::Collaborator | Role::Coder, Role::Dm)
                | (Role::Gm, Role::Collaborator)
                | (Role::Collaborator, Role::Gm)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(from: Role, to: Role, ts: u64) -> Handoff {
        Handoff::new(from, to, "test".to_string(), vec![1, 2, 3], ts, false)
    }

    #[test]
    fn valid_dm_to_gm() {
        assert!(h(Role::Dm, Role::Gm, 1).validate().is_ok());
        assert!(h(Role::Gm, Role::Dm, 2).validate().is_ok());
    }

    #[test]
    fn valid_dm_to_coder() {
        assert!(h(Role::Dm, Role::Coder, 1).validate().is_ok());
        assert!(h(Role::Coder, Role::Dm, 2).validate().is_ok());
    }

    #[test]
    fn invalid_gm_to_coder_rejected() {
        let r = h(Role::Gm, Role::Coder, 1).validate();
        assert!(matches!(r, Err(HandoffErr::InvalidTransition(Role::Gm, Role::Coder))));
        let r2 = h(Role::Coder, Role::Gm, 2).validate();
        assert!(matches!(r2, Err(HandoffErr::InvalidTransition(Role::Coder, Role::Gm))));
    }

    #[test]
    fn invalid_collab_to_coder_rejected() {
        let r = h(Role::Collaborator, Role::Coder, 1).validate();
        assert!(matches!(
            r,
            Err(HandoffErr::InvalidTransition(Role::Collaborator, Role::Coder))
        ));
        let r2 = h(Role::Coder, Role::Collaborator, 2).validate();
        assert!(matches!(
            r2,
            Err(HandoffErr::InvalidTransition(Role::Coder, Role::Collaborator))
        ));
    }

    #[test]
    fn payload_cap_enforced() {
        let big = vec![0u8; MAX_PAYLOAD_BYTES + 1];
        let ho = Handoff::new(Role::Dm, Role::Gm, "big".into(), big, 1, false);
        assert!(matches!(ho.validate(), Err(HandoffErr::PayloadTooLarge)));
        // exactly at cap is allowed
        let exact = vec![0u8; MAX_PAYLOAD_BYTES];
        let ok = Handoff::new(Role::Dm, Role::Gm, "exact".into(), exact, 2, false);
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn ts_monotonic_preserved() {
        // The struct itself simply records a u64. Monotonicity is a
        // state-machine property — we verify the field round-trips here.
        let a = h(Role::Dm, Role::Gm, 100);
        let b = h(Role::Dm, Role::Gm, 200);
        assert!(a.ts_micros < b.ts_micros);
    }

    #[test]
    fn sovereign_recorded() {
        let s = Handoff::new(
            Role::Dm,
            Role::Coder,
            "schema-evolve under sovereign attestation".into(),
            vec![],
            1,
            true,
        );
        assert!(s.sovereign_used);
        assert!(s.validate().is_ok());

        let ns = Handoff::new(
            Role::Dm,
            Role::Coder,
            "non-sovereign DM→Coder".into(),
            vec![],
            2,
            false,
        );
        assert!(!ns.sovereign_used);
        // topology-valid even without sovereign — policy gates that.
        assert!(ns.validate().is_ok());
    }

    #[test]
    fn handoff_serde_roundtrip() {
        let ho = Handoff::new(
            Role::Collaborator,
            Role::Gm,
            "branch-merge narrative".into(),
            vec![0xde, 0xad, 0xbe, 0xef],
            999_999,
            false,
        );
        let s = serde_json::to_string(&ho).expect("ser");
        let back: Handoff = serde_json::from_str(&s).expect("de");
        assert_eq!(ho, back);

        // Error variant roundtrip
        let err = HandoffErr::CrossRoleCapBleed { attempted: Role::Gm, with_bit: 0x4 };
        let es = serde_json::to_string(&err).expect("ser err");
        let eback: HandoffErr = serde_json::from_str(&es).expect("de err");
        assert_eq!(err, eback);
    }
}
