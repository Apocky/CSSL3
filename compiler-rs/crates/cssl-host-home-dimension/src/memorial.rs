//! Memorial-Wall : visitor-spore-ascriptions on public Homes.
//!
//! When a Home is shared publicly (M3 PublicListed or M4 RandomDropin),
//! visitors may leave spore-ascriptions — short text remembrances of
//! fallen characters or shared moments (spec/16 § Home-features
//! MEMORIAL-WALL). Posting requires `HM_CAP_MEMORIAL_PIN` ; the wall is a
//! `BTreeMap`-keyed deterministic record so federated mirrors converge.

use crate::ids::{Pubkey, Timestamp};
use serde::{Deserialize, Serialize};

/// One visitor-supplied ascription on a memorial entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorialAscription {
    /// Pubkey of the visitor who left the ascription.
    pub author: Pubkey,
    /// Timestamp of the ascription (caller-supplied).
    pub at: Timestamp,
    /// Free-form short text — sketches/songs are referenced via asset-ref tag.
    pub text: String,
}

impl MemorialAscription {
    /// Build a fresh ascription.
    #[must_use]
    pub fn new(author: Pubkey, at: Timestamp, text: impl Into<String>) -> Self {
        Self {
            author,
            at,
            text: text.into(),
        }
    }
}

/// One memorial-wall entry — typically a fallen character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorialEntry {
    /// Caller-allocated id (map key on the Home).
    pub id: u64,
    /// Name / handle of the fallen character.
    pub fallen_char: String,
    /// Cause of fall (free-form ; "to a Nemesis named Yrgrim").
    pub cause: String,
    /// Time of the fall (caller-supplied).
    pub at: Timestamp,
    /// Visitor-supplied ascriptions, in insertion order.
    pub ascriptions: Vec<MemorialAscription>,
}

impl MemorialEntry {
    /// Build a fresh entry with no ascriptions.
    #[must_use]
    pub fn new(
        id: u64,
        fallen_char: impl Into<String>,
        cause: impl Into<String>,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            fallen_char: fallen_char.into(),
            cause: cause.into(),
            at,
            ascriptions: Vec::new(),
        }
    }
}
