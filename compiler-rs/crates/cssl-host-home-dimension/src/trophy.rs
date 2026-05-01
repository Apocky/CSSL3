//! Trophy-case subsystem : pin / unpin / list of Ascended-items + Coherence shrines.
//!
//! Trophies commemorate accomplishments (spec/16 § Home-features TROPHY-CASE).
//! They are **not** cap-gated for *display* — the Home owner pins/unpins them
//! freely — but are recorded in the audit-log so visitors can verify provenance.

use crate::ids::Timestamp;
use serde::{Deserialize, Serialize};

/// Discriminator for the kind of accomplishment a trophy represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TrophyKind {
    /// An Ascended-item (Coherence-Engine epithet item ; spec/13).
    AscendedItem,
    /// A Coherence-Score shrine (numeric milestone).
    CoherenceShrine,
    /// A defeated Multiversal-Nemesis spore-trophy.
    NemesisSpore,
    /// A discovered alchemy-recipe certificate.
    RecipeCertificate,
    /// A biography fragment (spec/16 § Home-features Biography-readers).
    Biography,
    /// Any community-attested accomplishment not covered above.
    Other,
}

/// One pinned trophy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trophy {
    /// Caller-supplied unique id (map-key into the Home's trophy collection).
    pub id: u64,
    /// Kind discriminator.
    pub kind: TrophyKind,
    /// Time the trophy was acquired (caller-supplied timestamp).
    pub acquired_at: Timestamp,
    /// Free-form short note (e.g. "Defeated Nemesis Yrgrim @ NG+3").
    pub note: String,
}

impl Trophy {
    /// Build a fresh trophy.
    #[must_use]
    pub fn new(id: u64, kind: TrophyKind, acquired_at: Timestamp, note: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            acquired_at,
            note: note.into(),
        }
    }
}
