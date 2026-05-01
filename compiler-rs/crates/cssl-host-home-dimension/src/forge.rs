//! Forge-node : crafting-bench-instance queue.
//!
//! The forge is the Home's per-private-or-shared-mode crafting bench
//! (spec/16 § Home-features FORGE-NODE). This crate stores **only** the
//! queue + recipe id ; the recipe-graph evaluation lives in
//! `cssl-host-craft-graph`. We model items as a deterministic
//! `BTreeMap<u64, ForgeQueueItem>` so iteration is canonical.

use crate::ids::Timestamp;
use serde::{Deserialize, Serialize};

/// Identifier for a recipe (`cssl-host-craft-graph`-allocated externally).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ForgeRecipeId(pub u64);

/// One queued craft — the forge serves these in queue-id order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeQueueItem {
    /// Caller-supplied queue-id (map key).
    pub queue_id: u64,
    /// Recipe to be evaluated.
    pub recipe: ForgeRecipeId,
    /// Time the item was queued.
    pub queued_at: Timestamp,
    /// Whether the queue-item is still pending (vs cancelled / consumed).
    pub pending: bool,
}

impl ForgeQueueItem {
    /// Build a fresh queue-item @ pending.
    #[must_use]
    pub fn new(queue_id: u64, recipe: ForgeRecipeId, queued_at: Timestamp) -> Self {
        Self {
            queue_id,
            recipe,
            queued_at,
            pending: true,
        }
    }
}
