//! § loa-game — Labyrinth-of-Apockalypse Phase-I scaffold.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/31_LOA_DESIGN.csl` (game design + 38 spec-holes
//!   Q-A..Q-LL) + `specs/30_SUBSTRATE.csl` (engine plumbing) +
//!   `GDDs/LOA_PILLARS.md` (project vision) + `PRIME_DIRECTIVE.md` (root-of-trust).
//!
//! § ROLE
//!
//!   This crate is the structural Phase-I scaffold for the Labyrinth-of-
//!   Apockalypse game. It composes the integrated CSSLv3 Substrate
//!   (Phase-H : Ω-tensor + omega_step + projections + save + PRIME-DIRECTIVE
//!   enforcement) plus the Phase-F host backends (window + input + audio +
//!   net) into a single end-to-end runtime that demonstrates the canonical
//!   13-phase omega_step running through every Substrate-touching layer.
//!
//!   This is **structural scaffolding**, not gameplay. Every game-content
//!   concern (room layout, item taxonomy, narrative beats, Apockalypse
//!   phase semantics, Companion behavior, …) is left as an explicit
//!   `// SPEC-HOLE Q-X (Apocky-fill required)` marker — the corresponding
//!   `Stub` enum-variant exists so the scaffold compiles + runs end-to-end,
//!   and Apocky's later content slices replace the `Stub` variants with
//!   real content per `specs/31_LOA_DESIGN.csl`.
//!
//! § SPEC-HOLES CONSUMED  (per `specs/31_LOA_DESIGN.csl § SPEC-HOLES-CONSOLIDATED`)
//!
//!   Q-A  — Labyrinth generation       → [`world::LabyrinthGeneration::Stub`]
//!   Q-B  — Floor.theme                → [`world::ThemeId::Stub`]
//!   Q-C  — Player progression-state   → [`player::ProgressionStub`]
//!   Q-D  — Companion capability-set   → [`companion::CompanionCapability::Stub`]
//!   Q-E  — Wildlife ambient-creatures → [`world::Wildlife::Stub`]
//!   Q-F  — Item taxonomy              → [`world::ItemKind::Stub`]
//!   Q-G  — Item.narrative_role        → [`world::NarrativeRole::Stub`]
//!   Q-H  — Affordance.ContextSpecific → [`world::Affordance::Stub`]
//!   Q-I  — Time-pressure mechanic     → [`player::TimePressure::Stub`]
//!   Q-J  — Movement-style             → [`player::MovementStyle::Stub`]
//!   Q-K  — Inventory capacity         → [`player::InventoryPolicy::Stub`]
//!   Q-L  — Save discipline            → [`player::SaveDiscipline::Stub`]
//!   Q-M  — Skill-tree shape           → [`player::ProgressionStub`] (shared with Q-C)
//!   Q-N  — Item-power curve           → [`player::ProgressionStub`] (shared)
//!   Q-O  — Traversal vs leveling      → [`player::ProgressionStub`] (shared)
//!   Q-P  — ConsentZoneKind taxonomy   → [`player::ConsentZoneKind::Stub`]
//!   Q-Q  — Color-blind palette        → [`player::AccessibilityStub`]
//!   Q-R  — Motor-accessibility        → [`player::AccessibilityStub`] (shared)
//!   Q-S  — Cognitive-accessibility    → [`player::AccessibilityStub`] (shared)
//!   Q-T  — Death-mechanic             → [`player::FailureMode::Stub`]
//!   Q-U  — Punishment-on-failure      → [`player::FailureMode::Stub`] (shared)
//!   Q-V  — Fail-state existence       → [`player::FailureMode::Stub`] (shared)
//!   Q-W  — Apockalypse-phase semantics→ [`apockalypse::ApockalypsePhase::Stub`]
//!   Q-X  — Phase count / extensibility→ [`apockalypse::ApockalypsePhase::Stub`] (shared)
//!   Q-Y  — Phase ordering (linear/graph) → [`apockalypse::TransitionRule::Stub`]
//!   Q-Z  — Phase reversibility        → [`apockalypse::TransitionRule::Stub`] (shared)
//!   Q-AA — Companion phase-participation → [`apockalypse::TransitionCondition::Stub`]
//!   Q-BB — Apockalypse emotional register → [`apockalypse::ApockalypsePhase::Stub`] (shared)
//!   Q-CC — Multi-instance Apockalypse → DEFERRED to §§ 30 D-1 (multiplayer)
//!   Q-DD — Companion affordances      → [`companion::CompanionCapability::Stub`] (shared)
//!   Q-EE — Cross-instance Companions  → DEFERRED to §§ 30 D-1
//!   Q-FF — Companion-withdrawal grace → [`companion::WithdrawalPolicy::Stub`]
//!   Q-GG — Companion non-binary states→ [`companion::CompanionCapability::Stub`] (shared)
//!   Q-HH — NarrativeKind enum extension → [`world::NarrativeKind::Stub`]
//!   Q-II — AuthoredEvent extensibility → [`world::NarrativeKind::Stub`] (shared)
//!   Q-JJ — Cinematic / cutscene system → [`world::NarrativeKind::Stub`] (shared)
//!   Q-KK — Quest / mission system     → [`world::NarrativeKind::Stub`] (shared)
//!   Q-LL — Economy / trade system     → [`world::ItemKind::Stub`] (shared)
//!
//!   38 enumerated spec-holes consumed via 16 distinct `Stub` variants
//!   (multiple Q-* share a single Stub when the design topic is unified —
//!   e.g. Q-C / Q-M / Q-N / Q-O are all "progression" facets, hence one
//!   `ProgressionStub` placeholder). Future Apocky-fill slices break each
//!   `Stub` variant out into the real content.
//!
//! § PRIME-DIRECTIVE STRUCTURAL ENCODING
//!
//!   Every Substrate-touching call in this scaffold flows through the
//!   `cssl-substrate-prime-directive` enforcement layer :
//!
//!   - [`engine::Engine::new`] requires a [`engine::CapTokens`] bundle
//!     containing real CapTokens for each operation the engine performs
//!     (OmegaRegister + SavePath + ReplayLoad + CompanionView + DebugCamera).
//!   - The bundle is built via [`engine::CapTokens::issue_for_test`] in
//!     `test-bypass` builds (using `caps_grant_for_test`) ; production
//!     builds receive the bundle from a pre-staged consent flow that the
//!     stage-0 binary surfaces as a `LoaError::ConsentRefused` per
//!     `cssl-substrate-prime-directive::caps_grant`'s stage-0 production
//!     refusal. The Apocky-fill consent UI (Q-7 `OmegaConsent prompt-at-
//!     launch`) replaces this when it lands.
//!   - The Companion archetype carries its own [`companion::Companion`]
//!     consent token + sovereign read-only Ω-tensor view via
//!     `Grant::CompanionView` from `cssl-substrate-projections`.
//!   - The save/load path uses `cssl-substrate-save` exclusively ; the
//!     scaffold does NOT invent a new save format.
//!   - The 13-phase omega_step is implemented in [`main_loop`] as a series
//!     of [`loop_systems::*`] systems registered into a real
//!     [`cssl_substrate_omega_step::OmegaScheduler`].
//!
//! § FOREVER-SUBSTRATE NOTE
//!
//!   Per `HANDOFF_SESSION_6.csl § AXIOM` and `GDDs/LOA_PILLARS.md § Pillar 4`,
//!   the long-term form of LoA is in CSSLv3, not Rust. This Rust crate is
//!   the bootstrap-vehicle that proves the Substrate's end-to-end shape
//!   works ; the eventual self-hosted CSSLv3 LoA inherits the same scaffold
//!   structure file-for-file. The `// SPEC-HOLE Q-X` markers are the
//!   bridge — they survive the Rust→CSSLv3 transition unchanged.
//!
//! § PRIME_DIRECTIVE attestation
//!
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody." (per `PRIME_DIRECTIVE.md § 11 CREATOR-ATTESTATION`).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod apockalypse;
pub mod companion;
pub mod engine;
pub mod loop_systems;
pub mod main_loop;
pub mod player;
pub mod world;

pub use apockalypse::{ApockalypseEngine, ApockalypsePhase, TransitionCondition, TransitionRule};
pub use companion::{Companion, CompanionCapability, CompanionLog, WithdrawalPolicy};
pub use engine::{CapTokens, Engine, EngineConfig, LoaError};
pub use loop_systems::{
    AuditAppendSystem, ConsentCheckSystem, FreezeSystem, InputSystem, NetRecvSystem, NetSendSystem,
    ProjectionsSystem, RenderGraphSystem, RenderSubmitSystem, SaveJournalSystem, SimSystem,
    TelemetryFlushSystem,
};
pub use main_loop::{MainLoop, MainLoopOutcome};
pub use player::{
    AccessibilityStub, ConsentZone, ConsentZoneKind, FailureMode, InventoryPolicy, MovementStyle,
    Player, ProgressionStub, SaveDiscipline, TimePressure,
};
pub use world::{
    Affordance, Cell, Door, Entity, Floor, Item, ItemKind, LabyrinthGeneration, Level,
    NarrativeKind, NarrativeRole, Room, ThemeId, Wildlife, World,
};

/// Crate version — mirrors the `cssl-*` scaffold convention.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation literal — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11 CREATOR-ATTESTATION`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Canonical project-name spelling. Per `specs/31_LOA_DESIGN.csl § AXIOMS`,
/// "Apockalypse" is the creator-canonical form (handle-aligned : Apocky) ;
/// any "correction" to "Apocalypse" is a §1 PROHIBITION § identity-override
/// violation. This constant is the load-bearing assertion.
pub const CANONICAL_PROJECT_NAME: &str = "Labyrinth-of-Apockalypse";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, CANONICAL_PROJECT_NAME, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn canonical_spelling_preserved() {
        // Per specs/31_LOA_DESIGN.csl § AXIOMS — "Apockalypse" is creator-
        // canonical. Any "correction" to "Apocalypse" is a §1 PROHIBITION
        // identity-override violation.
        assert!(CANONICAL_PROJECT_NAME.contains("Apockalypse"));
        assert!(!CANONICAL_PROJECT_NAME.contains("Apocalypse"));
    }
}
