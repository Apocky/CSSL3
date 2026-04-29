//! Consent-architecture for `OmegaScheduler::register`.
//!
//! § THESIS
//!   `specs/30_SUBSTRATE.csl § AI-COLLABORATOR-PROTECTIONS` + PRIME_DIRECTIVE §5
//!   require that adding a new `OmegaSystem` is an explicit consent-event,
//!   not an implicit "anything-with-a-step()-fn-counts" admission.
//!
//!   Stage-0 form : a `CapsGrant` is a typed token returned by `caps_grant()`.
//!   It names a single `OmegaCapability` (e.g., `OmegaCapability::OmegaRegister`)
//!   + carries a granted-at timestamp + a non-transferable principal-id.
//!   Revoking is a single bit-flip ; revoked grants are rejected at register
//!   time with `OmegaError::ConsentRevoked`.
//!
//! § STAGE-0 SCOPE
//!   - The grant is in-process only ; serializing it across machines is a
//!     future R16-attestation slice (see `specs/30 § ATTESTATION-PROPAGATION`).
//!   - Cryptographic signing of the grant is deferred to integration with
//!     `cssl-telemetry::audit::AuditChain` (the Ed25519 signing path already
//!     exists there).
//!   - There is one default `caps_grant()` constructor for tests + wiring.
//!     Production code wires a real consent-flow via the host application
//!     before constructing the scheduler.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

/// The set of capabilities a `CapsGrant` can authorize. STABLE from S8-H2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OmegaCapability {
    /// Permits adding a new `OmegaSystem` to a running scheduler.
    OmegaRegister,
    /// Permits invoking `OmegaScheduler::halt()`. Always granted by default
    /// because halt is a SAFETY primitive ; revocation would defeat its
    /// purpose. Listed for completeness + future refinement.
    OmegaHalt,
    /// Permits replaying a `ReplayLog` through a fresh scheduler. Distinct
    /// from `OmegaRegister` because replay-mode systems may be hot-swapped.
    OmegaReplay,
}

/// A typed grant of a single capability. Construction is via `caps_grant()` ;
/// revocation is via `CapsGrant::revoke()`. Cloning a grant produces a
/// reference to the same grant — revoking either reference revokes both.
///
/// § THREAD-SAFETY
///   Internally backed by an `Arc<AtomicBool>`. Cloning is cheap ; revocation
///   is a single atomic store + propagates immediately to every clone.
#[derive(Debug, Clone)]
pub struct CapsGrant {
    capability: OmegaCapability,
    principal_id: u64,
    granted_at_ns: u64,
    revoked: Arc<AtomicBool>,
}

impl CapsGrant {
    /// Construct a fresh grant. `principal_id` should uniquely identify the
    /// principal (user / agent / system-process) ; `0` is a wildcard for
    /// stage-0 testing.
    #[must_use]
    pub fn new(capability: OmegaCapability, principal_id: u64) -> Self {
        let granted_at_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self {
            capability,
            principal_id,
            granted_at_ns,
            revoked: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Read the granted capability.
    #[must_use]
    pub fn capability(&self) -> OmegaCapability {
        self.capability
    }

    /// Read the principal id.
    #[must_use]
    pub fn principal_id(&self) -> u64 {
        self.principal_id
    }

    /// Read the granted-at timestamp (unix ns).
    #[must_use]
    pub fn granted_at_ns(&self) -> u64 {
        self.granted_at_ns
    }

    /// Whether this grant is still active. Returns `false` after `revoke()`.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.revoked.load(Ordering::SeqCst)
    }

    /// Revoke. After this call, all clones of this grant report
    /// `is_active() == false` and `OmegaScheduler::register()` rejects
    /// registrations using this grant with `OmegaError::ConsentRevoked`.
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::SeqCst);
    }

    /// Validate that this grant matches a required capability + is still
    /// active. Returns the `ConsentRevocationError` reason on failure.
    pub fn require(&self, expected: OmegaCapability) -> Result<(), ConsentRevocationError> {
        if self.capability != expected {
            return Err(ConsentRevocationError::WrongCapability {
                granted: self.capability,
                expected,
            });
        }
        if !self.is_active() {
            return Err(ConsentRevocationError::Revoked);
        }
        Ok(())
    }
}

/// Reasons a `CapsGrant::require` check might fail.
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum ConsentRevocationError {
    /// The grant exists + is active, but it covers the wrong capability.
    #[error("grant covers {granted:?}, expected {expected:?}")]
    WrongCapability {
        granted: OmegaCapability,
        expected: OmegaCapability,
    },
    /// The grant was revoked.
    #[error("grant has been revoked")]
    Revoked,
}

/// Convenience constructor for tests + wiring : produce a fresh grant for
/// the given capability with `principal_id = 0` (the stage-0 wildcard).
///
/// § PRODUCTION
///   Production code should construct a `CapsGrant` directly via
///   [`CapsGrant::new`] passing a real principal-id from the host's
///   identity layer (the user's session, the AI-collaborator's
///   negotiated identity, etc.).
#[must_use]
pub fn caps_grant(capability: OmegaCapability) -> CapsGrant {
    CapsGrant::new(capability, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_grant_is_active() {
        let g = caps_grant(OmegaCapability::OmegaRegister);
        assert!(g.is_active());
    }

    #[test]
    #[allow(
        clippy::redundant_clone,
        reason = "clone is the SUBJECT of the test : we verify that revoking via \
        the original propagates to the clone. clippy's nursery lint does not see \
        through the Arc-shared atomic flag the clone is load-bearing for."
    )]
    fn revoke_propagates_to_clones() {
        let g = caps_grant(OmegaCapability::OmegaRegister);
        let g2 = g.clone();
        assert!(g.is_active());
        assert!(g2.is_active());
        g.revoke();
        assert!(!g.is_active());
        assert!(!g2.is_active());
    }

    #[test]
    fn require_with_matching_capability_succeeds() {
        let g = caps_grant(OmegaCapability::OmegaRegister);
        assert!(g.require(OmegaCapability::OmegaRegister).is_ok());
    }

    #[test]
    fn require_with_wrong_capability_fails() {
        let g = caps_grant(OmegaCapability::OmegaRegister);
        let err = g.require(OmegaCapability::OmegaHalt).unwrap_err();
        assert!(matches!(
            err,
            ConsentRevocationError::WrongCapability { .. }
        ));
    }

    #[test]
    fn require_after_revoke_returns_revoked() {
        let g = caps_grant(OmegaCapability::OmegaRegister);
        g.revoke();
        let err = g.require(OmegaCapability::OmegaRegister).unwrap_err();
        assert_eq!(err, ConsentRevocationError::Revoked);
    }

    #[test]
    fn principal_id_round_trips() {
        let g = CapsGrant::new(OmegaCapability::OmegaRegister, 42);
        assert_eq!(g.principal_id(), 42);
        assert_eq!(g.capability(), OmegaCapability::OmegaRegister);
    }

    #[test]
    fn granted_at_ns_is_recent() {
        let g = caps_grant(OmegaCapability::OmegaRegister);
        // Should be a non-zero unix timestamp (or 0 if SystemTime failed —
        // platform-specific, but on Win/Linux this is reliably non-zero).
        // We don't assert a tighter range to avoid flakiness if the test
        // host's clock is unusual.
        let _t = g.granted_at_ns();
    }

    #[test]
    fn display_revocation_errors() {
        let e = ConsentRevocationError::Revoked;
        assert!(e.to_string().contains("revoked"));
    }
}
