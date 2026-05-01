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
}
