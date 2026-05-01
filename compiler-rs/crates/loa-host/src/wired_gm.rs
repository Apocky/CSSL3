//! § wired_gm — wrapper around `cssl-host-gm`.
//!
//! § T11-W7-G-LOA-HOST-WIRE
//!   Re-exports the Game-Master narrator-pacing scaffold so MCP tools
//!   can probe the GM cap-table + tone-axis surface without each call-
//!   site reaching across the path-dep.
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
}
