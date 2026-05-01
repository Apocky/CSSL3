//! DM cap-ladder : the bit-flag set every DM-emission is gated against.
//!
//! § SPEC : `specs/grand-vision/10_INTELLIGENCE.csl` § ROLE-DM § CAP-GATING
//!
//! Cross-role-cap-bleed is structurally prevented at the call-site : the DM
//! only ever consults [`DmCapTable`], never a `GmCapTable` / `CollabCapTable`
//! / `CoderCapTable`. Inter-role transitions = explicit `HandoffEvent`s.

use serde::{Deserialize, Serialize};

/// Cap-bit : stamp seed-cells via `SubstrateMutateCap`-zone.
pub const DM_CAP_SCENE_EDIT: u32 = 1;

/// Cap-bit : NPC instantiation via `Sovereign-claim-grant`.
pub const DM_CAP_SPAWN_NPC: u32 = 2;

/// Cap-bit : forward text-hash via `NetPostWriteCap` (vercel companion-proxy).
pub const DM_CAP_COMPANION_RELAY: u32 = 4;

/// Mask of every well-known DM cap-bit. Useful for pre-grant-all in tests.
pub const DM_CAP_ALL: u32 =
    DM_CAP_SCENE_EDIT | DM_CAP_SPAWN_NPC | DM_CAP_COMPANION_RELAY;

/// DM cap-table. `granted_bits` is a bit-set : cap is held iff the bit is 1.
///
/// The table is **revocable** at any time via [`DmCapTable::revoke`] ; the
/// `cap-revoked-mid → DROP+user-feedback` failure-mode is implemented at the
/// call-site by checking [`DmCapTable::has`] before every emission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmCapTable {
    /// Bit-set of granted DM cap-bits.
    pub granted_bits: u32,
}

impl DmCapTable {
    /// Empty cap-table (no caps granted).
    #[must_use]
    pub fn empty() -> Self {
        Self { granted_bits: 0 }
    }

    /// All-DM-caps-granted table (test convenience).
    #[must_use]
    pub fn all_granted() -> Self {
        Self {
            granted_bits: DM_CAP_ALL,
        }
    }

    /// Construct from a literal bit-set.
    #[must_use]
    pub fn from_bits(granted_bits: u32) -> Self {
        Self { granted_bits }
    }

    /// Returns `true` iff every bit in `cap_bit` is granted.
    ///
    /// `cap_bit` may be a single bit (`DM_CAP_SCENE_EDIT`) or a mask
    /// (`DM_CAP_SCENE_EDIT | DM_CAP_SPAWN_NPC`) ; the check is set-inclusion.
    #[must_use]
    pub fn has(&self, cap_bit: u32) -> bool {
        (self.granted_bits & cap_bit) == cap_bit
    }

    /// Grant additional bits (idempotent).
    pub fn grant(&mut self, cap_bit: u32) {
        self.granted_bits |= cap_bit;
    }

    /// Revoke bits (idempotent).
    pub fn revoke(&mut self, cap_bit: u32) {
        self.granted_bits &= !cap_bit;
    }
}

impl Default for DmCapTable {
    /// Default = empty (no caps). Forces explicit grant before any emission.
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_holds_no_caps() {
        let t = DmCapTable::empty();
        assert!(!t.has(DM_CAP_SCENE_EDIT));
        assert!(!t.has(DM_CAP_SPAWN_NPC));
        assert!(!t.has(DM_CAP_COMPANION_RELAY));
    }

    #[test]
    fn all_granted_holds_every_known_cap() {
        let t = DmCapTable::all_granted();
        assert!(t.has(DM_CAP_SCENE_EDIT));
        assert!(t.has(DM_CAP_SPAWN_NPC));
        assert!(t.has(DM_CAP_COMPANION_RELAY));
        assert!(t.has(DM_CAP_ALL));
    }

    #[test]
    fn grant_revoke_round_trips() {
        let mut t = DmCapTable::empty();
        t.grant(DM_CAP_SCENE_EDIT);
        assert!(t.has(DM_CAP_SCENE_EDIT));
        t.revoke(DM_CAP_SCENE_EDIT);
        assert!(!t.has(DM_CAP_SCENE_EDIT));
    }

    #[test]
    fn has_supports_mask_set_inclusion() {
        let t = DmCapTable::from_bits(DM_CAP_SCENE_EDIT | DM_CAP_COMPANION_RELAY);
        // Mask requires both bits — middle bit missing.
        assert!(!t.has(DM_CAP_SCENE_EDIT | DM_CAP_SPAWN_NPC));
        // But each-individual present-bit holds.
        assert!(t.has(DM_CAP_SCENE_EDIT));
        assert!(t.has(DM_CAP_COMPANION_RELAY));
        assert!(!t.has(DM_CAP_SPAWN_NPC));
    }

    #[test]
    fn cap_bits_are_disjoint_singletons() {
        // Each canonical bit is a distinct power-of-two.
        assert_eq!(DM_CAP_SCENE_EDIT, 1);
        assert_eq!(DM_CAP_SPAWN_NPC, 2);
        assert_eq!(DM_CAP_COMPANION_RELAY, 4);
        assert_eq!(DM_CAP_ALL, 0b111);
    }
}
