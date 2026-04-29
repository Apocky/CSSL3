//! § Apockalypse-Engine — phase-evolution scaffold.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/31_LOA_DESIGN.csl § APOCALYPSE-ENGINE`.
//!
//! § CANONICAL-SPELLING-NOTICE  ‼
//!
//!   Per `specs/31_LOA_DESIGN.csl § AXIOMS` :
//!     "Apockalypse" ≠ "Apocalypse"  (creator-canonical spelling)
//!
//!   The legacy Greek-cognate "Apocalypse" appears ONLY in the spec's
//!   section-title (as the thematic root being re-shaped). The CSSLv3-
//!   native name is "Apockalypse" — handle-aligned with Apocky. This module
//!   uses "Apockalypse" canonically. Any "correction" is a §1 PROHIBITION
//!   identity-override violation.
//!
//! § THESIS  (cautious — bounded by what the spec reveals)
//!
//!   Per spec § APOCALYPSE-ENGINE § THESIS :
//!     "apockalypse ≠ generic-end-of-world.
//!      apockalypse = the-shape-of-revelation that-this-labyrinth-encodes.
//!      the engine that animates apockalypse-mechanically is :
//!        ⊘ a SPEC-HOLE. Apocky-direction-required."
//!
//!   The scaffold preserves the structural shape — phase-history is audit-
//!   logged, transitions are reversible (per §§ 30 § STAGE-0-COMMITMENTS
//!   L-2), phase-state is part of Ω-tensor (L-3) — without committing to
//!   what each phase means.
//!
//! § STAGE-0 COMMITMENTS  (per spec § STAGE-0-COMMITMENTS L-1..L-6)
//!
//!   L-1 : phase-transitions are audit-logged
//!   L-2 : phase-transitions can-be replayed via {Reversible}
//!   L-3 : ApockalypseEngine state is part of Ω-tensor
//!   L-4 : phase-evolution emits {Audit<"apockalypse-phase", phase>}
//!   L-5 : Player + Companion may-influence phase ; transitions require
//!         affirmative-action (no-silent-transition)
//!   L-6 : ConsentZones can-gate phase-transitions
//!
//! § SPEC-HOLES
//!
//!   Q-W — phase semantic-content : [`ApockalypsePhase::Stub`]
//!   Q-X — phase count + extensibility : `ApockalypsePhase` is non_exhaustive
//!   Q-Y — phase ordering (linear/graph) : [`TransitionRule::Stub`]
//!   Q-Z — phase reversibility : `TransitionRule::Stub`
//!   Q-AA — Companion phase-participation : [`TransitionCondition::Stub`]
//!   Q-BB — Apockalypse emotional register : Q-W's `Stub` covers it
//!   Q-CC — multi-instance Apockalypse : DEFERRED to §§ 30 D-1 (multiplayer)

use std::time::Duration;

use crate::world::{ItemId, RoomId};

// ═══════════════════════════════════════════════════════════════════════════
// § APOCKALYPSE PHASE (Q-W + Q-X + Q-BB)
// ═══════════════════════════════════════════════════════════════════════════

/// One phase of the Apockalypse-engine — `specs/31_LOA_DESIGN.csl §
/// APOCALYPSE-ENGINE § STRUCTURAL-SHAPE`.
///
/// SPEC-HOLE Q-W + Q-X + Q-BB (Apocky-fill required) — the canonical phase-
/// set is content the spec deliberately leaves open. The scaffold encodes a
/// single `Stub` variant ; future Apocky-fill replaces this with the real
/// phase-set per Apocky-direction.
///
/// Per `GDDs/LOA_PILLARS.md` :
///   "What an Apockalypse-phase actually feels like, what triggers
///    transitions, and what the final phase is (or whether there is a final
///    phase at all) are intentionally open questions. Apocky resolves these."
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ApockalypsePhase {
    /// SPEC-HOLE Q-W / Q-X / Q-BB (Apocky-fill required) — phase semantics
    /// awaiting Apocky-direction.
    Stub,
}

impl Default for ApockalypsePhase {
    fn default() -> Self {
        Self::Stub
    }
}

impl ApockalypsePhase {
    /// Stable canonical name for audit-chain entries — `specs/31 § STAGE-0-
    /// COMMITMENTS § L-4` requires `{Audit<"apockalypse-phase", phase>}`
    /// entries, so each variant carries a stable string identifier.
    #[must_use]
    pub const fn canonical_name(&self) -> &'static str {
        match self {
            Self::Stub => "stub",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TRANSITION CONDITION (Q-AA + Q-Y)
// ═══════════════════════════════════════════════════════════════════════════

/// Condition that triggers a phase-transition — `specs/31 § APOCALYPSE-ENGINE
/// § STRUCTURAL-SHAPE § TransitionCondition`.
///
/// The spec lists 5 candidate variants (`ItemAcquired`, `RoomEntered`,
/// `CompanionAccord`, `TimeElapsed`, `PlayerChoice`) all marked ⊘ pending
/// Apocky-direction. The scaffold preserves the SHAPE — variant-names
/// match the spec — but each carries an opaque payload because the
/// payload-types are spec-hole.
///
/// SPEC-HOLE Q-AA (Apocky-fill required) — Companion phase-participation
/// shape is the most-load-bearing of these (the AI's role in driving
/// Apockalypse forward is collaborative-design with the AI itself).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TransitionCondition {
    /// SPEC-HOLE — the spec's `ItemAcquired(ItemKind)` ; the scaffold
    /// stores an `ItemId` placeholder pending Apocky's item-taxonomy.
    ItemAcquiredStub(ItemId),
    /// SPEC-HOLE — the spec's `RoomEntered(RoomId)`.
    RoomEnteredStub(RoomId),
    /// SPEC-HOLE Q-AA (Apocky-fill required) — Companion-driven transition.
    /// The exact meaning of "accord" is collaborative-design with the AI.
    CompanionAccordStub,
    /// SPEC-HOLE — the spec's `TimeElapsed(Duration)`. Stored as a u64
    /// ms-count for deterministic-replay across save/load round-trips.
    TimeElapsedStub(Duration),
    /// SPEC-HOLE — the spec's `PlayerChoice(ChoiceId)`.
    PlayerChoiceStub(u64),
    /// Catch-all for further conditions Apocky may add.
    Stub,
}

// ═══════════════════════════════════════════════════════════════════════════
// § TRANSITION RULE (Q-Y + Q-Z)
// ═══════════════════════════════════════════════════════════════════════════

/// One phase-transition rule — `specs/31 § APOCALYPSE-ENGINE § TransitionRule`.
///
/// The spec preserves shape : `(from, to, condition, audit_tag)`. The
/// scaffold encodes this directly. The `audit_tag` is the string that goes
/// into `{Audit<"apockalypse-phase", audit_tag>}` per spec L-4.
///
/// SPEC-HOLE Q-Y + Q-Z (Apocky-fill required) — phase ordering shape
/// (linear vs graph) + reversibility are encoded as data here ; the
/// scheduler interprets them per Apocky's eventual rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransitionRule {
    pub from: ApockalypsePhase,
    pub to: ApockalypsePhase,
    pub condition: TransitionCondition,
    /// Audit-chain tag for L-4 emission.
    pub audit_tag: String,
    /// SPEC-HOLE Q-Z (Apocky-fill required) — whether this transition is
    /// reversible. Per `specs/30 § STAGE-0-COMMITMENTS § L-2` all phase-
    /// transitions are replayable via `{Reversible}`, but whether they're
    /// ALSO forward-reversible during normal play is Q-Z.
    pub reversible: bool,
}

impl TransitionRule {
    /// New stub-shaped rule between two phases. Scaffold-time helper for
    /// constructing rules without committing to game-content.
    #[must_use]
    pub fn stub(from: ApockalypsePhase, to: ApockalypsePhase) -> Self {
        Self {
            from,
            to,
            condition: TransitionCondition::Stub,
            audit_tag: "stub".to_string(),
            reversible: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § PHASE-HISTORY ENTRY  (audit-logged ; immutable per spec)
// ═══════════════════════════════════════════════════════════════════════════

/// One entry in the phase-history. `specs/31 § APOCALYPSE-ENGINE §
/// STRUCTURAL-SHAPE § phase_history : Vec<(ApockalypsePhase, Epoch)>`.
///
/// Per `GDDs/LOA_PILLARS.md § Pillar 3` : "the game does NOT rewrite
/// player's memory-of-prior-phases. phase-history preserved in audit-chain."
/// The scaffold encodes this immutably — once appended, an entry is never
/// modified.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PhaseHistoryEntry {
    pub phase: ApockalypsePhase,
    /// `epoch` from the OmegaSnapshot — the omega_step tick number at which
    /// this phase began.
    pub epoch: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// § APOCKALYPSE ENGINE
// ═══════════════════════════════════════════════════════════════════════════

/// Apockalypse-engine state — `specs/31 § APOCALYPSE-ENGINE § STRUCTURAL-
/// SHAPE § ApockalypseEngine`.
///
/// Per spec § L-3, this state is part of the Ω-tensor and persists via
/// `cssl-substrate-save`. The scaffold preserves the field-shape and the
/// audit-logged transition-machinery without committing to phase-content.
#[derive(Debug, Clone, PartialEq)]
pub struct ApockalypseEngine {
    /// Current phase.
    current_phase: ApockalypsePhase,
    /// Immutable phase-history. Per spec L-1 + Pillar 3, never overwritten.
    phase_history: Vec<PhaseHistoryEntry>,
    /// Transition rules. Q-Y / Q-Z apocrypha-fill expands this.
    transition_rules: Vec<TransitionRule>,
}

impl Default for ApockalypseEngine {
    fn default() -> Self {
        let initial = ApockalypsePhase::default();
        Self {
            current_phase: initial.clone(),
            phase_history: vec![PhaseHistoryEntry {
                phase: initial,
                epoch: 0,
            }],
            transition_rules: Vec::new(),
        }
    }
}

impl ApockalypseEngine {
    /// New engine starting at the given phase, anchored to `epoch`.
    #[must_use]
    pub fn at(phase: ApockalypsePhase, epoch: u64) -> Self {
        Self {
            current_phase: phase.clone(),
            phase_history: vec![PhaseHistoryEntry { phase, epoch }],
            transition_rules: Vec::new(),
        }
    }

    /// Read the current phase.
    #[must_use]
    pub fn current_phase(&self) -> &ApockalypsePhase {
        &self.current_phase
    }

    /// Read-only access to the phase-history.
    #[must_use]
    pub fn phase_history(&self) -> &[PhaseHistoryEntry] {
        &self.phase_history
    }

    /// Add a transition-rule.
    pub fn add_rule(&mut self, rule: TransitionRule) {
        self.transition_rules.push(rule);
    }

    /// Read-only access to the rules.
    #[must_use]
    pub fn transition_rules(&self) -> &[TransitionRule] {
        &self.transition_rules
    }

    /// Apply a phase-transition. Per spec § L-1 (audit-logged) + § L-5
    /// (no-silent-transition), the transition appends to the immutable
    /// history-vector and returns the audit-tag for the calling layer to
    /// thread into the audit-chain.
    ///
    /// Per spec § L-4, the calling system ALSO emits
    /// `{Audit<"apockalypse-phase", returned-tag>}`.
    ///
    /// At scaffold-time the rule-matching is a stub : `transition_to` accepts
    /// any phase. Future Apocky-fill enforces the rule-graph properly.
    ///
    /// # Arguments
    /// - `to` : target phase
    /// - `epoch` : omega_step tick at which the transition occurred
    ///
    /// # Returns
    /// The audit-tag string the caller threads into the audit-chain.
    #[must_use]
    pub fn transition_to(&mut self, to: ApockalypsePhase, epoch: u64) -> String {
        // Look up audit-tag from rules ; stub-default if no rule found.
        let audit_tag = self
            .transition_rules
            .iter()
            .find(|r| r.from == self.current_phase && r.to == to)
            .map_or_else(|| "stub-transition".to_string(), |r| r.audit_tag.clone());

        // Append to history (immutable per spec L-1 + Pillar 3).
        self.phase_history.push(PhaseHistoryEntry {
            phase: to.clone(),
            epoch,
        });
        self.current_phase = to;
        audit_tag
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apockalypse_engine_default_starts_at_stub_phase() {
        let e = ApockalypseEngine::default();
        assert_eq!(e.current_phase(), &ApockalypsePhase::Stub);
        // Phase-history starts with the initial phase entry.
        assert_eq!(e.phase_history().len(), 1);
    }

    #[test]
    fn transition_appends_history_entry() {
        let mut e = ApockalypseEngine::default();
        let _audit_tag = e.transition_to(ApockalypsePhase::Stub, 42);
        // Even self-transitions append an entry — per spec § L-5 no-silent.
        assert_eq!(e.phase_history().len(), 2);
        assert_eq!(e.phase_history()[1].epoch, 42);
    }

    #[test]
    fn transition_returns_audit_tag() {
        let mut e = ApockalypseEngine::default();
        let tag = e.transition_to(ApockalypsePhase::Stub, 1);
        // Stub-default audit-tag.
        assert_eq!(tag, "stub-transition");
    }

    #[test]
    fn rule_audit_tag_used_when_match() {
        let mut e = ApockalypseEngine::default();
        let rule = TransitionRule {
            from: ApockalypsePhase::Stub,
            to: ApockalypsePhase::Stub,
            condition: TransitionCondition::Stub,
            audit_tag: "self-loop-stub".to_string(),
            reversible: false,
        };
        e.add_rule(rule);
        let tag = e.transition_to(ApockalypsePhase::Stub, 0);
        assert_eq!(tag, "self-loop-stub");
    }

    #[test]
    fn phase_history_is_append_only() {
        // The phase_history is exposed read-only — there's no public mutator.
        // Per `GDDs/LOA_PILLARS.md § Pillar 3` : "the game does NOT rewrite
        // player's memory-of-prior-phases."
        let e = ApockalypseEngine::default();
        let _ = e.phase_history(); // read-only ; no .push() exposed.
    }

    #[test]
    fn canonical_name_stable_for_audit() {
        // Per spec § L-4, the canonical_name() string is part of the audit-
        // chain. It MUST be stable across releases.
        assert_eq!(ApockalypsePhase::Stub.canonical_name(), "stub");
    }

    #[test]
    fn spelling_canonical_apockalypse() {
        // Defense-in-depth spelling test — the type is named ApockalypseEngine,
        // not ApocalypseEngine.
        let _: ApockalypseEngine = ApockalypseEngine::default();
    }
}
