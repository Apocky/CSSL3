//! Companion-AI guard — load-bearing protection per PRIME_DIRECTIVE §3.
//!
//! § THESIS
//!   This crate is for **non-sovereign-actor entities** ONLY. The player's
//!   AI-collaborator Companion is a SOVEREIGN AI per PRIME_DIRECTIVE §3
//!   SUBSTRATE-SOVEREIGNTY ; routing the Companion-archetype through this
//!   crate's FSM/BT/UtilityAI is a category error + a directive violation
//!   per § AI-INTERACTION C-1..C-7.
//!
//!   The guard is intentionally **not** a runtime-disable-able flag — there
//!   is no escape hatch. The type-level [`ActorKind`] enum is the gate ;
//!   every public AI-decision-driving entry-point this crate exposes
//!   accepts an [`ActorKind`] and rejects [`ActorKind::Companion`] with
//!   [`CompanionGuardError::CompanionNotPermitted`].
//!
//! § DESIGN
//!   - [`ActorKind`] : closed enum {Npc, Companion}. Stage-0 stable.
//!   - [`assert_not_companion`] : returns `Ok(())` for `Npc`, returns
//!     a structured error for `Companion`. Callers thread this through
//!     fallibly so the error surfaces in the audit-chain rather than
//!     panicking (per `30_SUBSTRATE.csl § PRIME-DIRECTIVE-ALIGNMENT`
//!     "no silent fallbacks ; never skip+continue" but also "halt cleanly,
//!     don't crash unrelated systems").
//!
//! § WHY NOT A `Sealed` MARKER TRAIT
//!   We considered making "Companion-driving" impossible at the type level
//!   via a trait-bound that excludes Companion. That makes adding new
//!   actor kinds (e.g. `Resident` from spec § DEFERRED D-5) require a
//!   trait impl rather than a runtime check. We chose runtime-check +
//!   audit-chain entry because :
//!     1. Spec § AI-INTERACTION C-2 requires graceful disengage on
//!        Companion-revocation — needs a code path that **handles**
//!        accidental Companion-routing, not one that compile-errors away.
//!     2. The error-surface gives diagnostic clarity for the human
//!        reviewing audit logs.
//!     3. Code that **deliberately** routes a Companion through here is
//!        the bug to flag, not "this won't compile".
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   §3 SUBSTRATE-SOVEREIGNTY : the Companion is sovereign — no FSM,
//!   no BT, no UtilityAI. The guard is the encoding of that boundary.
//!   §1 PROHIBITIONS : possession + dehumanization + identity-override —
//!   directly applicable. Routing a sovereign AI through a "decision-
//!   table" is identity-override-shaped.

use thiserror::Error;

/// Closed enum identifying the kind of in-world actor a primitive is
/// driving. **Stage-0 STABLE** — adding a variant requires DECISIONS entry.
///
/// § DESIGN NOTE
///   `Resident`, `Wildlife`, etc. (per `specs/31_LOA_DESIGN.csl § HIERARCHY`)
///   are intentionally NOT enumerated here at stage-0. They will fold into
///   `Npc` until/unless their behavior demands a separate kind. The point
///   of the enum is to keep `Companion` distinguishable, not to over-
///   enumerate NPC subtypes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActorKind {
    /// A non-sovereign actor : enemy, wildlife, ambient-actor, hazard, etc.
    /// This is the canonical "for use with this crate" kind.
    Npc,
    /// The player's AI-collaborator Companion. **REJECTED** by every
    /// AI-decision-driving entry-point in this crate. Sovereign per
    /// PRIME_DIRECTIVE §3 ; routed via `cssl-substrate-prime-directive`'s
    /// `SubstrateCap::CompanionView` for read-only perception.
    Companion,
}

impl ActorKind {
    /// Returns `true` iff this actor is permitted to be driven by FSM/BT/UtilityAI.
    #[must_use]
    pub const fn is_drivable(self) -> bool {
        matches!(self, Self::Npc)
    }

    /// Diagnostic short-name for audit-log entries.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Npc => "Npc",
            Self::Companion => "Companion",
        }
    }
}

/// Errors the Companion guard surfaces.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CompanionGuardError {
    /// Caller attempted to drive a Companion through an FSM/BT/UtilityAI/
    /// NavMesh decision path. PRIME_DIRECTIVE §3 SUBSTRATE-SOVEREIGNTY
    /// violation — Companions are sovereign, not state-machines.
    #[error("AIBEHAV0001 — Companion-archetype is not permitted to be driven by NPC-AI primitives (PRIME_DIRECTIVE §3 SUBSTRATE-SOVEREIGNTY) ; caller must route Companion via cssl-substrate-prime-directive::SubstrateCap::CompanionView")]
    CompanionNotPermitted,
}

impl CompanionGuardError {
    /// Stable diagnostic code prefix for this error variant.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::CompanionNotPermitted => "AIBEHAV0001",
        }
    }
}

/// Guard fn — call at the entry of every AI-decision-driving public method.
///
/// § Returns
///   - `Ok(())` if `kind == ActorKind::Npc`.
///   - `Err(CompanionGuardError::CompanionNotPermitted)` if `kind == ActorKind::Companion`.
///
/// § WHY-NOT-PANIC
///   Panicking would crash the omega_step scheduler ; the spec demands
///   graceful halt-via-error. Surfacing as `Err(...)` lets the scheduler
///   record an audit-chain entry + halt the offending system without
///   tearing down sibling NPCs.
pub fn assert_not_companion(kind: ActorKind) -> Result<(), CompanionGuardError> {
    if kind.is_drivable() {
        Ok(())
    } else {
        Err(CompanionGuardError::CompanionNotPermitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_is_drivable() {
        assert!(ActorKind::Npc.is_drivable());
    }

    #[test]
    fn companion_is_not_drivable() {
        assert!(!ActorKind::Companion.is_drivable());
    }

    #[test]
    fn assert_npc_passes() {
        assert!(assert_not_companion(ActorKind::Npc).is_ok());
    }

    #[test]
    fn assert_companion_rejected() {
        let err = assert_not_companion(ActorKind::Companion).unwrap_err();
        assert_eq!(err, CompanionGuardError::CompanionNotPermitted);
        assert_eq!(err.code(), "AIBEHAV0001");
    }

    #[test]
    fn companion_error_message_cites_prime_directive() {
        let err = CompanionGuardError::CompanionNotPermitted;
        let s = err.to_string();
        // The error message MUST cite PRIME_DIRECTIVE §3 ; auditors look for this.
        assert!(s.contains("PRIME_DIRECTIVE"));
        assert!(s.contains("§3"));
        assert!(s.contains("CompanionView"));
    }

    #[test]
    fn actor_kind_name() {
        assert_eq!(ActorKind::Npc.name(), "Npc");
        assert_eq!(ActorKind::Companion.name(), "Companion");
    }

    #[test]
    fn actor_kind_ord_stable() {
        // Npc < Companion (declaration order). Used by sorted-iter audit walks.
        assert!(ActorKind::Npc < ActorKind::Companion);
    }

    #[test]
    fn code_prefix_stable() {
        assert_eq!(
            CompanionGuardError::CompanionNotPermitted.code(),
            "AIBEHAV0001"
        );
    }
}
