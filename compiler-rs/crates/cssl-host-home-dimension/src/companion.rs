//! Companion subsystem : add / dismiss / converse-stub for befriended NPCs.
//!
//! Companions are NPCs the player has befriended elsewhere in the game and
//! invited to live in their Home (spec/16 § Home-features NPC-COMPANIONS).
//! Disposition is a coarse three-bucket gauge ; `cssl-host-npc-bt` drives
//! detailed behaviour-tree state outside this crate.

use crate::archetype::ArchetypeId;
use crate::ids::Pubkey;
use serde::{Deserialize, Serialize};

/// Coarse disposition bucket for a companion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CompanionDisposition {
    /// Reserved / wary : freshly-added companion.
    Reserved,
    /// Friendly : default after a few interactions.
    Friendly,
    /// Devoted : explicitly bonded over time.
    Devoted,
}

impl Default for CompanionDisposition {
    fn default() -> Self {
        Self::Reserved
    }
}

/// One companion living in a Home.
///
/// `archetype` here is *the companion's preferred archetype hint* — used by
/// the renderer to pick idle-animations + ambient lines that match the Home's
/// vibe. It is **not** the Home's archetype.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Companion {
    /// Pubkey-style identifier for the companion (deterministic-orderable).
    pub id: Pubkey,
    /// Companion's preferred-archetype hint for renderer.
    pub archetype_hint: ArchetypeId,
    /// Greeting line spoken when the owner (or any visitor) enters.
    pub greeting: String,
    /// Current disposition.
    pub disposition: CompanionDisposition,
}

impl Companion {
    /// Build a fresh companion @ default Reserved disposition.
    #[must_use]
    pub fn new(id: Pubkey, archetype_hint: ArchetypeId, greeting: impl Into<String>) -> Self {
        Self {
            id,
            archetype_hint,
            greeting: greeting.into(),
            disposition: CompanionDisposition::Reserved,
        }
    }

    /// Stub for future cssl-host-npc-bt-driven dialogue.
    ///
    /// Returns a deterministic short reply that nudges the disposition up
    /// by one bucket each call (Reserved → Friendly → Devoted → Devoted).
    pub fn converse(&mut self, prompt: &str) -> String {
        self.disposition = match self.disposition {
            CompanionDisposition::Reserved => CompanionDisposition::Friendly,
            CompanionDisposition::Friendly | CompanionDisposition::Devoted => {
                CompanionDisposition::Devoted
            }
        };
        // deterministic stub-reply — no RNG, no wall-clock
        format!("[{}] heard: {}", self.archetype_hint.code(), prompt)
    }
}
