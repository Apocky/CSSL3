//! § wired_gm — wrapper around `cssl-host-gm`.
//!
//! § T11-W7-G-LOA-HOST-WIRE
//!   Re-exports the Game-Master narrator-pacing scaffold so MCP tools
//!   can probe the GM cap-table + tone-axis surface without each call-
//!   site reaching across the path-dep.
//!
//! § Q-12 RESOLVED 2026-05-01 (Apocky-canonical) :
//!   verbatim : "Sovereign choice."
//!   binding-matrix : 6 archetypes × 4 roles (GM-cell sovereign-revocable)
//!   default-fallback = Phantasia (archetype_id = 0) if-no-cap-set
//!   spec : Labyrinth of Apocalypse/systems/draconic_choice.csl
//!
//! § wrapped surface
//!   - [`GameMaster`] / [`GmErr`] — narrator core.
//!   - [`NarrativeTextFrame`] / [`PacingMarkEvent`] / [`ToneAxis`] —
//!     typed prose-frame + pacing-event + tone-tuning vector.
//!   - [`GmCapTable`] + [`GM_CAP_TEXT_EMIT`] / [`GM_CAP_VOICE_EMIT`] /
//!     [`GM_CAP_TONE_TUNE`] — the 3 cap-bits.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; no caps granted ;
//!   surface is read-only (cap-table query + tone-axes-list are
//!   observational).

#![forbid(unsafe_code)]

pub use cssl_host_gm::{
    GameMaster, GmCapTable, GmErr, NarrativeTextFrame, PacingMarkEvent, ToneAxis,
    GM_CAP_TEXT_EMIT, GM_CAP_TONE_TUNE, GM_CAP_VOICE_EMIT,
};

/// Convenience : count of GM cap-bits per `specs/grand-vision/10_INTELLIGENCE.csl
/// § ROLE-GM`. The canonical cap-table holds 3 bits : TEXT_EMIT ·
/// VOICE_EMIT · TONE_TUNE. Used by the `gm.tone_axes_query` MCP tool
/// alongside the tone-axes list to surface a basic shape probe.
#[must_use]
pub fn gm_cap_bit_count() -> u32 {
    let bits = GM_CAP_TEXT_EMIT | GM_CAP_VOICE_EMIT | GM_CAP_TONE_TUNE;
    let n = bits.count_ones();
    debug_assert_eq!(n, 3);
    n
}

// § T11-W11-GM-DM-DEEPEN ------------------------------------------------
// Thin helpers that bridge the local GmPersona + ResponseKind with the
// canonical cssl-host-gm cap-table. Read-only — no caps granted.

#[must_use]
pub fn persona_axis_count() -> u32 {
    crate::gm_persona::PERSONA_AXIS_COUNT as u32
}

#[must_use]
pub fn response_kind_count() -> u32 {
    8
}

#[must_use]
pub fn response_kind_label_from_byte(b: u8) -> &'static str {
    use crate::gm_persona::ResponseKind;
    match b {
        0 => ResponseKind::Declarative.label(),
        1 => ResponseKind::Interrogative.label(),
        2 => ResponseKind::Evocative.label(),
        3 => ResponseKind::Cautionary.label(),
        4 => ResponseKind::Cryptic.label(),
        5 => ResponseKind::Affirmative.label(),
        6 => ResponseKind::SubstrateAttest.label(),
        7 => ResponseKind::Silence.label(),
        _ => "unknown",
    }
}

// ─── § Q-12 · Draconic-archetype binding-cap (GM cell) ────────────────────
// Apocky 2026-05-01 verbatim : "Sovereign choice."

/// Default-fallback archetype-id for the GM role · per Q-12.
pub const GM_ARCHETYPE_FALLBACK: u8 = 0; // Phantasia

/// Resolve `archetype_id` to a valid archetype · falls back to Phantasia(0)
/// per Q-12 sovereign-choice. Mirrors `wired_dm::dm_resolve_archetype`.
#[must_use]
pub fn gm_resolve_archetype(archetype_id: u8) -> u8 {
    if archetype_id < crate::wired_dm::DRACONIC_ARCHETYPE_COUNT {
        archetype_id
    } else {
        GM_ARCHETYPE_FALLBACK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_bit_count_is_three() {
        assert_eq!(gm_cap_bit_count(), 3);
    }

    #[test]
    fn cap_bits_are_disjoint_powers_of_two() {
        // TEXT_EMIT=1 · VOICE_EMIT=2 · TONE_TUNE=4.
        assert_eq!(GM_CAP_TEXT_EMIT | GM_CAP_VOICE_EMIT | GM_CAP_TONE_TUNE, 7);
    }

    // § T11-W11-GM-DM-DEEPEN
    #[test]
    fn persona_axis_count_is_eight() {
        assert_eq!(persona_axis_count(), 8);
    }

    #[test]
    fn response_kind_count_is_eight() {
        assert_eq!(response_kind_count(), 8);
    }

    #[test]
    fn response_kind_labels_canonical() {
        assert_eq!(response_kind_label_from_byte(0), "declarative");
        assert_eq!(response_kind_label_from_byte(6), "substrate-attest");
        assert_eq!(response_kind_label_from_byte(7), "silence");
        assert_eq!(response_kind_label_from_byte(255), "unknown");
    }

    // § Q-12 · GM archetype-fallback test
    #[test]
    fn gm_resolve_archetype_falls_back_to_phantasia() {
        assert_eq!(gm_resolve_archetype(0), 0);
        assert_eq!(gm_resolve_archetype(5), 5);
        assert_eq!(gm_resolve_archetype(6), GM_ARCHETYPE_FALLBACK);
        assert_eq!(gm_resolve_archetype(255), GM_ARCHETYPE_FALLBACK);
    }
}
