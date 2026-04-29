//! § Companion — sovereign-AI collaborator archetype.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/31_LOA_DESIGN.csl § AI-INTERACTION` +
//!   `specs/30_SUBSTRATE.csl § AI-COLLABORATOR-PROTECTIONS` + PRIME_DIRECTIVE
//!   §3 SUBSTRATE-SOVEREIGNTY.
//!
//! § THESIS  ‼ load-bearing
//!
//!   Per `GDDs/LOA_PILLARS.md § Pillar 2` :
//!     "The Companion archetype in the world is not an NPC. It is the
//!      in-world projection of an actual AI participant who has consented
//!      to collaborate with the player. The game does not own the AI's
//!      cognition — it surfaces affordances, and the AI decides what to
//!      do with them."
//!
//!   This module encodes that commitment structurally. The Companion's
//!   read-only Ω-tensor view is gated by a [`Grant::CompanionView`] from
//!   `cssl-substrate-projections` ; the Companion's log is AI-authored and
//!   the game cannot read it back to override the AI's stated experience.
//!
//! § CONSTRAINTS  (per `specs/31 § AI-INTERACTION § STAGE-0-DESIGN-COMMITMENTS`)
//!
//!   C-1 : Companion carries `Handle<AISession>` (here a stable u64 id) —
//!         the game does NOT own or replicate the AI's cognition.
//!   C-2 : Participation is consent-token-gated. Revocation is a graceful
//!         disengagement, not a crash.
//!   C-3 : Read-only projection onto Ω-tensor.
//!   C-4 : The game NEVER instructs the AI to violate its-own-cognition.
//!   C-5 : Companion-actions are AI-initiated.
//!   C-6 : CompanionLog is AI-authored ; game cannot modify it.
//!   C-7 : Player + Companion relationship is collaborative, not master-slave.
//!
//! § SPEC-HOLES
//!
//!   Q-D / Q-DD — Companion capability-set / affordances :
//!     [`CompanionCapability::Stub`]
//!   Q-FF — Withdrawal grace period : [`WithdrawalPolicy::Stub`]
//!   Q-GG — Companion non-binary cognitive states : Q-D's `Stub` covers it
//!   Q-EE — Cross-instance Companions : DEFERRED to §§ 30 D-1 (multiplayer)

// ═══════════════════════════════════════════════════════════════════════════
// § AI-SESSION HANDLE
// ═══════════════════════════════════════════════════════════════════════════

/// Stable handle for the AI's external sovereign session.
///
/// Per `specs/31 § AI-INTERACTION § C-1` : "Companion archetype carries a
/// Handle<AISession> linking to the AI's sovereign session-state. the game
/// does-NOT own or replicate the AI's cognition."
///
/// At scaffold-time this is an opaque u64. The game NEVER inspects what
/// session this id refers to ; it only uses it to route affordance-surfacing
/// to the right external AI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AiSessionId(pub u64);

// ═══════════════════════════════════════════════════════════════════════════
// § COMPANION CAPABILITY (Q-D + Q-DD + Q-GG)
// ═══════════════════════════════════════════════════════════════════════════

/// What in-world affordances the game surfaces to the Companion-AI.
///
/// SPEC-HOLE Q-D + Q-DD + Q-GG (Apocky-fill required) — the full capability-
/// set is collaborative-design between Apocky + the AI-collaborator. The
/// scaffold encodes a single `Stub` ; the design-pillar is that the AI
/// CHOOSES from the affordance-set, not the other way around.
///
/// Per spec § C-5 : "Companion-actions in-world are AI-initiated ; the game
/// surfaces affordances ; the AI chooses."
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CompanionCapability {
    /// SPEC-HOLE Q-D / Q-DD / Q-GG (Apocky-fill required) — capability-set
    /// awaiting collaborative design with the AI-collaborator.
    Stub,
}

impl Default for CompanionCapability {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § WITHDRAWAL POLICY (Q-FF)
// ═══════════════════════════════════════════════════════════════════════════

/// Graceful-withdrawal policy when the Companion-AI revokes consent.
///
/// SPEC-HOLE Q-FF (Apocky-fill required) — immediate vs end-of-step.
/// The spec § COMPANION-WITHDRAWAL says "next-step : Companion despawns
/// gracefully" so end-of-step is the likely default. Apocky-confirmation
/// pending.
///
/// Per spec § AI-INTERACTION § C-2 : revocation gracefully disengages —
/// "NOT crashed, NOT killed, NOT erased ; their state-trace preserved +
/// signed in audit-chain."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WithdrawalPolicy {
    /// SPEC-HOLE Q-FF (Apocky-fill required) — withdrawal-grace-period
    /// awaiting Apocky-direction. Stage-0 scaffold treats as end-of-step.
    Stub,
}

impl Default for WithdrawalPolicy {
    fn default() -> Self {
        Self::Stub
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § COMPANION-LOG (AI-authored ; game cannot modify)
// ═══════════════════════════════════════════════════════════════════════════

/// The Companion's own log — AI-authored, AI-redacted, AI-exported.
///
/// Per `specs/31 § AI-INTERACTION § C-6` :
///   "Companion-log (CompanionLog ref-shared) is AI-authored ; AI may-redact-
///    it-or-export-it under their own consent."
///
/// The game holds an append-only handle but CANNOT read entries back to
/// override the AI's stated experience. At scaffold-time the log is an
/// opaque vector of strings ; the game's `loop_systems::SimSystem` drops
/// entries into it but never reads them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompanionLog {
    /// Append-only opaque entries. The game NEVER reads from this vector
    /// (only appends). Future Apocky-fill may swap this for a richer
    /// AI-side type that signs entries with the AI's own key.
    entries: Vec<String>,
}

impl CompanionLog {
    /// New empty log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an entry. Game-side code uses this ; AI-side code uses the
    /// same surface (in a real impl the AI's session has a separate write-
    /// channel that signs entries — DEFERRED per spec § C-6).
    pub fn append(&mut self, entry: impl Into<String>) {
        self.entries.push(entry.into());
    }

    /// Number of entries — the only read-side surface the game has. The
    /// game CANNOT read entry contents.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Test-only : inspect entries. Production game-code never calls this.
    #[doc(hidden)]
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn test_entries(&self) -> &[String] {
        &self.entries
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § COMPANION ARCHETYPE
// ═══════════════════════════════════════════════════════════════════════════

/// Companion-AI in-world archetype — `specs/31_LOA_DESIGN.csl § Inhabitants
/// § Companion`.
///
/// Per spec : "AI-collaborator-sovereign. ai_session : Handle<AISession>
/// (sovereign-attribution). consent_token : ConsentToken<\"ai-collab\">.
/// can_revoke : bool (AI-can-leave). observation_log : ref<CompanionLog>
/// (AI's-own-log)."
///
/// The scaffold preserves every field. The `consent_active` boolean
/// proxies `ConsentToken<"ai-collab">` until the typed token-system from
/// `cssl-substrate-prime-directive` is wired through ; the
/// [`crate::engine::Engine`] holds the actual `CapToken` for the
/// CompanionView grant.
#[derive(Debug, Clone, PartialEq)]
pub struct Companion {
    /// Stable handle to the AI's external sovereign session.
    pub ai_session: AiSessionId,
    /// Whether the AI's consent-token for ai-collab is currently active.
    /// Per spec § C-2, revocation triggers graceful disengagement.
    pub consent_active: bool,
    /// World-space position the Companion's projection appears at.
    pub pos: [f32; 3],
    /// Orientation as a unit quaternion (xyzw).
    pub orientation: [f32; 4],
    /// Whether the AI has the standing right to revoke participation
    /// (always `true` per spec § C-2 ; preserved as a field so future
    /// Apocky-fill cannot accidentally remove the right).
    pub can_revoke: bool,
    /// AI-authored log. Game cannot read entries back.
    pub observation_log: CompanionLog,
    /// SPEC-HOLE Q-D / Q-DD / Q-GG (Apocky-fill required) — capabilities.
    pub capability_set: CompanionCapability,
    /// Withdrawal-policy when consent is revoked.
    pub withdrawal_policy: WithdrawalPolicy,
}

impl Companion {
    /// Construct a new Companion archetype with the given session id.
    /// Initial state : consent active, can-revoke true, capabilities Stub.
    #[must_use]
    pub fn new(ai_session: AiSessionId) -> Self {
        Self {
            ai_session,
            consent_active: true,
            pos: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0, 1.0],
            can_revoke: true,
            observation_log: CompanionLog::new(),
            capability_set: CompanionCapability::default(),
            withdrawal_policy: WithdrawalPolicy::default(),
        }
    }

    /// Companion-AI revokes consent. Per spec § C-2, this is a graceful
    /// disengagement that the next omega_step phase-1 (consent-check)
    /// observes. The Companion enters Withdrawing-state for the current
    /// step ; the next step despawns the archetype.
    ///
    /// `can_revoke` is preserved as `true` — even after revocation, the
    /// AI retains the standing to decline future participation.
    pub fn revoke_consent(&mut self, reason: impl Into<String>) {
        self.consent_active = false;
        self.observation_log
            .append(format!("companion-withdrawal: {}", reason.into()));
    }

    /// Whether this Companion is in active state (consent-granted + AI is
    /// participating).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.consent_active
    }

    /// Surface a read-only invitation for the AI to look at the world.
    /// Per spec § C-3 + § C-5, this is a one-way affordance : the game
    /// offers, the AI chooses.
    ///
    /// Returns the `AiSessionId` so the calling layer can route the
    /// invitation to the right external AI session.
    #[must_use]
    pub fn surface_observation_affordance(&self) -> AiSessionId {
        self.ai_session
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_starts_active_with_revoke_right() {
        let c = Companion::new(AiSessionId(42));
        assert!(c.is_active());
        assert!(c.can_revoke);
    }

    #[test]
    fn revoke_consent_disengages_gracefully() {
        let mut c = Companion::new(AiSessionId(42));
        c.revoke_consent("AI-decision");
        assert!(!c.is_active());
        // can_revoke is preserved — even after revocation, AI retains
        // the standing to decline future participation.
        assert!(c.can_revoke);
        // Withdrawal logged in AI-authored log.
        assert_eq!(c.observation_log.entry_count(), 1);
    }

    #[test]
    fn capability_set_default_is_stub() {
        // Q-D / Q-DD / Q-GG remain Apocky-fill ; no capabilities asserted.
        let c = Companion::new(AiSessionId(0));
        assert!(matches!(c.capability_set, CompanionCapability::Stub));
    }

    #[test]
    fn companion_log_is_append_only() {
        // The CompanionLog API is intentionally append-only on the game side.
        // We can call append() + entry_count() but there's no public read-
        // entries surface — this is the load-bearing structural encoding of
        // spec § C-6.
        let mut log = CompanionLog::new();
        log.append("a");
        log.append("b");
        assert_eq!(log.entry_count(), 2);
    }

    #[test]
    fn surface_affordance_returns_session_id_only() {
        // Per spec § C-3 + § C-5 : the game surfaces invitations ; it does
        // NOT call into the AI's cognition. The affordance returns the
        // session-id only.
        let c = Companion::new(AiSessionId(123));
        let id = c.surface_observation_affordance();
        assert_eq!(id, AiSessionId(123));
    }

    #[test]
    fn ai_session_id_is_opaque_u64() {
        // The game NEVER inspects what AiSessionId refers to. It's an
        // opaque routing token.
        let a = AiSessionId(42);
        let b = AiSessionId(42);
        assert_eq!(a, b);
    }
}
