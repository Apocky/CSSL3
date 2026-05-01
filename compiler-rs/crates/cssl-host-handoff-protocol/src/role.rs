//! § role
//! ════════════════════════════════════════════════════════════════
//! Four narrow-orchestrator roles + per-role capability bitfields.
//!
//! Cap-bit topology (per specs/grand-vision/10_INTELLIGENCE.csl) :
//!   DM      : SCENE_EDIT       SPAWN_NPC       COMPANION_RELAY
//!   GM      : TEXT_EMIT        VOICE_EMIT      TONE_TUNE
//!   Collab  : INTEGRATE        BRANCH_MERGE    BIAS_UPDATE
//!   Coder   : AST_EDIT         HOT_RELOAD      SCHEMA_EVOLVE
//!
//! Each role's bits live in an independent u32 namespace. The
//! `cap_name_for(role, bit)` lookup ensures cross-role bits cannot be
//! coerced into the wrong role's name-table — bleed-attempts at the
//! cap-validation layer surface as `None`.

use serde::{Deserialize, Serialize};

/// Narrow-orchestrator role identity. Each role has its own scoped cap-bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Scene/world coordinator — edits scenes, spawns NPCs, relays to companions.
    Dm,
    /// Game-master narrator — emits text/voice, tunes tone.
    Gm,
    /// Collaborator co-author — integrates branches, merges, updates bias profile.
    Collaborator,
    /// Coder — runtime AST/schema mutation + hot-reload.
    Coder,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Dm => "DM",
            Self::Gm => "GM",
            Self::Collaborator => "Collaborator",
            Self::Coder => "Coder",
        };
        f.write_str(s)
    }
}

// ── DM caps ────────────────────────────────────────────────────────
/// DM : edit scene structure (rooms, props, layout).
pub const DM_CAP_SCENE_EDIT: u32 = 1;
/// DM : spawn NPCs into the active scene.
pub const DM_CAP_SPAWN_NPC: u32 = 2;
/// DM : relay messages to companion-channel.
pub const DM_CAP_COMPANION_RELAY: u32 = 4;

// ── GM caps ────────────────────────────────────────────────────────
/// GM : emit narration as text.
pub const GM_CAP_TEXT_EMIT: u32 = 1;
/// GM : emit narration as voice (TTS / synthesis).
pub const GM_CAP_VOICE_EMIT: u32 = 2;
/// GM : tune tone / vocal style parameters.
pub const GM_CAP_TONE_TUNE: u32 = 4;

// ── Collaborator caps ──────────────────────────────────────────────
/// Collab : integrate co-author proposals into active narrative.
pub const COLLAB_CAP_INTEGRATE: u32 = 1;
/// Collab : merge story-branches.
pub const COLLAB_CAP_BRANCH_MERGE: u32 = 2;
/// Collab : update co-author bias-profile.
pub const COLLAB_CAP_BIAS_UPDATE: u32 = 4;

// ── Coder caps ─────────────────────────────────────────────────────
/// Coder : edit AST nodes at runtime.
pub const CODER_CAP_AST_EDIT: u32 = 1;
/// Coder : trigger hot-reload of mutated modules.
pub const CODER_CAP_HOT_RELOAD: u32 = 2;
/// Coder : evolve schema (type + cap declarations).
pub const CODER_CAP_SCHEMA_EVOLVE: u32 = 4;

/// Per-role cap-bit envelope. The `bits` field is a bitfield of the role's
/// own cap-namespace — cross-role values are meaningless and rejected at
/// audit by [`cap_name_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleCaps {
    /// Which role this cap-set applies to.
    pub role: Role,
    /// Bitfield in role-local namespace.
    pub bits: u32,
}

impl RoleCaps {
    /// Construct a new RoleCaps. No validation — bits are role-local.
    #[must_use]
    pub const fn new(role: Role, bits: u32) -> Self {
        Self { role, bits }
    }

    /// Returns true if the bitfield contains only known caps for the role.
    #[must_use]
    pub fn is_valid_mask(&self) -> bool {
        let max = match self.role {
            Role::Dm => DM_CAP_SCENE_EDIT | DM_CAP_SPAWN_NPC | DM_CAP_COMPANION_RELAY,
            Role::Gm => GM_CAP_TEXT_EMIT | GM_CAP_VOICE_EMIT | GM_CAP_TONE_TUNE,
            Role::Collaborator => {
                COLLAB_CAP_INTEGRATE | COLLAB_CAP_BRANCH_MERGE | COLLAB_CAP_BIAS_UPDATE
            }
            Role::Coder => CODER_CAP_AST_EDIT | CODER_CAP_HOT_RELOAD | CODER_CAP_SCHEMA_EVOLVE,
        };
        (self.bits & !max) == 0
    }
}

/// Look up the human-readable cap-name for `(role, bit)`. Returns `None`
/// if `bit` is not a single cap of the named role — this is the
/// cap-bit-bleed defence at name-resolution time.
#[must_use]
pub fn cap_name_for(role: Role, bit: u32) -> Option<&'static str> {
    match (role, bit) {
        (Role::Dm, DM_CAP_SCENE_EDIT) => Some("DM_CAP_SCENE_EDIT"),
        (Role::Dm, DM_CAP_SPAWN_NPC) => Some("DM_CAP_SPAWN_NPC"),
        (Role::Dm, DM_CAP_COMPANION_RELAY) => Some("DM_CAP_COMPANION_RELAY"),
        (Role::Gm, GM_CAP_TEXT_EMIT) => Some("GM_CAP_TEXT_EMIT"),
        (Role::Gm, GM_CAP_VOICE_EMIT) => Some("GM_CAP_VOICE_EMIT"),
        (Role::Gm, GM_CAP_TONE_TUNE) => Some("GM_CAP_TONE_TUNE"),
        (Role::Collaborator, COLLAB_CAP_INTEGRATE) => Some("COLLAB_CAP_INTEGRATE"),
        (Role::Collaborator, COLLAB_CAP_BRANCH_MERGE) => Some("COLLAB_CAP_BRANCH_MERGE"),
        (Role::Collaborator, COLLAB_CAP_BIAS_UPDATE) => Some("COLLAB_CAP_BIAS_UPDATE"),
        (Role::Coder, CODER_CAP_AST_EDIT) => Some("CODER_CAP_AST_EDIT"),
        (Role::Coder, CODER_CAP_HOT_RELOAD) => Some("CODER_CAP_HOT_RELOAD"),
        (Role::Coder, CODER_CAP_SCHEMA_EVOLVE) => Some("CODER_CAP_SCHEMA_EVOLVE"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_four_roles_distinct() {
        let roles = [Role::Dm, Role::Gm, Role::Collaborator, Role::Coder];
        for (i, a) in roles.iter().enumerate() {
            for b in roles.iter().skip(i + 1) {
                assert_ne!(a, b, "roles must be pairwise-distinct");
            }
        }
    }

    #[test]
    fn cap_bits_unique_within_role() {
        // DM
        assert_ne!(DM_CAP_SCENE_EDIT, DM_CAP_SPAWN_NPC);
        assert_ne!(DM_CAP_SCENE_EDIT, DM_CAP_COMPANION_RELAY);
        assert_ne!(DM_CAP_SPAWN_NPC, DM_CAP_COMPANION_RELAY);
        // GM
        assert_ne!(GM_CAP_TEXT_EMIT, GM_CAP_VOICE_EMIT);
        assert_ne!(GM_CAP_VOICE_EMIT, GM_CAP_TONE_TUNE);
        // Collab
        assert_ne!(COLLAB_CAP_INTEGRATE, COLLAB_CAP_BRANCH_MERGE);
        assert_ne!(COLLAB_CAP_BRANCH_MERGE, COLLAB_CAP_BIAS_UPDATE);
        // Coder
        assert_ne!(CODER_CAP_AST_EDIT, CODER_CAP_HOT_RELOAD);
        assert_ne!(CODER_CAP_HOT_RELOAD, CODER_CAP_SCHEMA_EVOLVE);
    }

    #[test]
    fn cap_name_lookup_known_and_unknown() {
        assert_eq!(cap_name_for(Role::Dm, DM_CAP_SCENE_EDIT), Some("DM_CAP_SCENE_EDIT"));
        assert_eq!(cap_name_for(Role::Gm, GM_CAP_VOICE_EMIT), Some("GM_CAP_VOICE_EMIT"));
        assert_eq!(cap_name_for(Role::Coder, CODER_CAP_SCHEMA_EVOLVE), Some("CODER_CAP_SCHEMA_EVOLVE"));
        // bleed: GM bit shape, but asked under DM → must be None
        assert_eq!(cap_name_for(Role::Dm, GM_CAP_VOICE_EMIT | 0x80), None);
        // unknown bit
        assert_eq!(cap_name_for(Role::Dm, 0x1000), None);
    }

    #[test]
    fn role_serde_roundtrip() {
        for r in [Role::Dm, Role::Gm, Role::Collaborator, Role::Coder] {
            let s = serde_json::to_string(&r).expect("ser");
            let back: Role = serde_json::from_str(&s).expect("de");
            assert_eq!(r, back);
        }
        let caps = RoleCaps::new(Role::Gm, GM_CAP_TEXT_EMIT | GM_CAP_TONE_TUNE);
        let s = serde_json::to_string(&caps).expect("ser caps");
        let back: RoleCaps = serde_json::from_str(&s).expect("de caps");
        assert_eq!(caps, back);
    }

    #[test]
    fn role_display() {
        assert_eq!(Role::Dm.to_string(), "DM");
        assert_eq!(Role::Gm.to_string(), "GM");
        assert_eq!(Role::Collaborator.to_string(), "Collaborator");
        assert_eq!(Role::Coder.to_string(), "Coder");
    }

    #[test]
    fn is_valid_cap_mask() {
        let dm_ok = RoleCaps::new(Role::Dm, DM_CAP_SCENE_EDIT | DM_CAP_SPAWN_NPC);
        assert!(dm_ok.is_valid_mask());
        // bit 0x10 has no name → invalid
        let dm_bad = RoleCaps::new(Role::Dm, DM_CAP_SCENE_EDIT | 0x10);
        assert!(!dm_bad.is_valid_mask());
        // empty mask is valid (no caps)
        let empty = RoleCaps::new(Role::Coder, 0);
        assert!(empty.is_valid_mask());
        // full per-role
        let coder_full = RoleCaps::new(
            Role::Coder,
            CODER_CAP_AST_EDIT | CODER_CAP_HOT_RELOAD | CODER_CAP_SCHEMA_EVOLVE,
        );
        assert!(coder_full.is_valid_mask());
    }
}
