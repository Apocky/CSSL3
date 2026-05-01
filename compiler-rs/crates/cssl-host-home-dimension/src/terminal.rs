//! Mycelial-Terminal : opt-in flag + stub for cssl-host-mycelium aggregate-view.
//!
//! The terminal is the player's interface to view aggregate-contributions
//! and toggle opt-in for cross-user nutrient-exchange (spec/16 §
//! Home-features MYCELIAL-TERMINAL + § MYCELIAL-NUTRIENT-EXCHANGE). The
//! actual aggregate-fetch lives in `cssl-host-mycelium` ; this crate
//! tracks only the per-Home opt-in flag and an audit-emit hook.

use crate::ids::Timestamp;
use serde::{Deserialize, Serialize};

/// Per-Home mycelial-terminal state.
///
/// `opted_in` defaults to `false` — opt-in is **always** explicit (axiom A-3
/// of spec/16) and revocable. `last_toggle_at` is recorded for audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MycelialTerminal {
    /// Whether the player has opted in to mycelial nutrient-exchange.
    pub opted_in: bool,
    /// Timestamp of the most-recent opt-in/opt-out toggle.
    pub last_toggle_at: Timestamp,
    /// Number of times the opt-flag has been toggled (sovereignty footprint).
    pub toggle_count: u32,
}

impl MycelialTerminal {
    /// Fresh-default (opted out, never-toggled).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            opted_in: false,
            last_toggle_at: Timestamp::zero(),
            toggle_count: 0,
        }
    }

    /// Toggle the opt-flag and record the timestamp.
    pub fn toggle(&mut self, at: Timestamp) {
        self.opted_in = !self.opted_in;
        self.last_toggle_at = at;
        self.toggle_count = self.toggle_count.saturating_add(1);
    }
}
