//! § wired_dm — wrapper around `cssl-host-dm`.
//!
//! § T11-W7-G-LOA-HOST-WIRE
//!   Re-exports the Director-Master scene-arbiter scaffold so MCP tools
//!   can probe the DM cap-table without each call-site reaching across
//!   the path-dep.
//!
//! § wrapped surface
//!   - [`DirectorMaster`] / [`DmDecision`] / [`DmErr`] — orchestrator core.
//!   - [`SceneEditOp`] / [`SpawnOrder`] / [`CompanionPrompt`] — typed effects.
//!   - [`DmCapTable`] + [`DM_CAP_SCENE_EDIT`] / [`DM_CAP_SPAWN_NPC`] /
//!     [`DM_CAP_COMPANION_RELAY`] — the 3 cap-bits.
//!
//! § Q-12 RESOLVED 2026-05-01 (Apocky-canonical) :
//!   verbatim : "Sovereign choice."
//!   binding-matrix : 6 archetypes × 4 roles = 24-cell sovereign-revocable
//!   archetypes : Phantasia(0) · EbonyMimic(1) · Phoenix(2) · Octarine(3)
//!                · Paradox(4) · Fossil(5)
//!   default-fallback = Phantasia (archetype_id = 0) if-no-cap-set
//!   spec : Labyrinth of Apocalypse/systems/draconic_choice.csl
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; no caps granted ;
//!   surface is read-only (cap-table query is observational).

#![forbid(unsafe_code)]

pub use cssl_host_dm::{
    CompanionPrompt, DirectorMaster, DmCapTable, DmDecision, DmErr, SceneEditOp, SpawnOrder,
    DM_CAP_ALL, DM_CAP_COMPANION_RELAY, DM_CAP_SCENE_EDIT, DM_CAP_SPAWN_NPC,
};

/// Convenience : count of DM cap-bits defined by spec § ROLE-DM. The
/// canonical cap-table holds 3 bits : SCENE_EDIT · SPAWN_NPC ·
/// COMPANION_RELAY. Used by the `dm.cap_table_query` MCP tool to surface
/// a basic shape probe.
#[must_use]
pub fn dm_cap_bit_count() -> u32 {
    // popcount of DM_CAP_ALL — must equal 3 per spec.
    let n = DM_CAP_ALL.count_ones();
    debug_assert_eq!(n, 3);
    n
}

// § T11-W11-GM-DM-DEEPEN ------------------------------------------------
// Thin helpers that bridge the new local DmArc with the canonical
// cssl-host-dm cap-table. Read-only — no caps granted.

/// Translate a 5-phase arc-byte into its canonical label.
#[must_use]
pub fn arc_phase_label_from_byte(b: u8) -> &'static str {
    match crate::dm_arc::ArcPhase::from_index(b) {
        Some(p) => p.label(),
        None => "unknown",
    }
}

/// Convenience : count of arc-phases defined locally · always 5.
#[must_use]
pub fn arc_phase_count() -> u32 {
    crate::dm_arc::ARC_PHASE_COUNT as u32
}

// ─── § Q-12 · Draconic-archetype binding-cap ──────────────────────────────
// Apocky 2026-05-01 verbatim : "Sovereign choice."
//
// `archetype_id` is the 6-archetype ordinal :
//   0 = Phantasia (default-fallback) · 1 = EbonyMimic · 2 = Phoenix
//   3 = Octarine · 4 = Paradox · 5 = Fossil
// Out-of-range u8 ⇒ Phantasia fallback.

/// Default-fallback archetype-id for the DM role · per Q-12.
pub const DM_ARCHETYPE_FALLBACK: u8 = 0; // Phantasia

/// Total archetype-count per Q-12 (canonical 6).
pub const DRACONIC_ARCHETYPE_COUNT: u8 = 6;

/// Archetype label resolver · Phantasia for any out-of-range id.
#[must_use]
pub fn archetype_label_from_byte(b: u8) -> &'static str {
    match b {
        0 => "Phantasia",
        1 => "EbonyMimic",
        2 => "Phoenix",
        3 => "Octarine",
        4 => "Paradox",
        5 => "Fossil",
        _ => "Phantasia", // fallback per Q-12
    }
}

/// Resolve `archetype_id` to a valid archetype · falls back to Phantasia(0)
/// per Q-12 sovereign-choice + default-fallback discipline.
#[must_use]
pub fn dm_resolve_archetype(archetype_id: u8) -> u8 {
    if archetype_id < DRACONIC_ARCHETYPE_COUNT { archetype_id } else { DM_ARCHETYPE_FALLBACK }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_bit_count_is_three() {
        assert_eq!(dm_cap_bit_count(), 3);
    }

    #[test]
    fn cap_bits_are_disjoint_powers_of_two() {
        // SCENE_EDIT=1 · SPAWN_NPC=2 · COMPANION_RELAY=4.
        assert_eq!(DM_CAP_SCENE_EDIT | DM_CAP_SPAWN_NPC | DM_CAP_COMPANION_RELAY, 7);
        assert_eq!(DM_CAP_ALL, 7);
    }

    // § T11-W11-GM-DM-DEEPEN
    #[test]
    fn arc_phase_count_is_five() {
        assert_eq!(arc_phase_count(), 5);
    }

    #[test]
    fn arc_phase_labels_canonical() {
        assert_eq!(arc_phase_label_from_byte(0), "Discovery");
        assert_eq!(arc_phase_label_from_byte(1), "Tension");
        assert_eq!(arc_phase_label_from_byte(2), "Crisis");
        assert_eq!(arc_phase_label_from_byte(3), "Catharsis");
        assert_eq!(arc_phase_label_from_byte(4), "Quiet");
        assert_eq!(arc_phase_label_from_byte(99), "unknown");
    }

    // § Q-12 · Draconic-archetype binding-cap tests
    #[test]
    fn archetype_count_is_six() {
        assert_eq!(DRACONIC_ARCHETYPE_COUNT, 6);
    }

    #[test]
    fn archetype_labels_canonical() {
        assert_eq!(archetype_label_from_byte(0), "Phantasia");
        assert_eq!(archetype_label_from_byte(1), "EbonyMimic");
        assert_eq!(archetype_label_from_byte(2), "Phoenix");
        assert_eq!(archetype_label_from_byte(3), "Octarine");
        assert_eq!(archetype_label_from_byte(4), "Paradox");
        assert_eq!(archetype_label_from_byte(5), "Fossil");
        assert_eq!(archetype_label_from_byte(99), "Phantasia"); // fallback
    }

    #[test]
    fn dm_resolve_archetype_falls_back_to_phantasia() {
        assert_eq!(dm_resolve_archetype(0), 0);
        assert_eq!(dm_resolve_archetype(5), 5);
        assert_eq!(dm_resolve_archetype(6), DM_ARCHETYPE_FALLBACK); // out-of-range
        assert_eq!(dm_resolve_archetype(255), DM_ARCHETYPE_FALLBACK);
    }
}
