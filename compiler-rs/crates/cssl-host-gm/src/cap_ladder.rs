//! § cap_ladder.rs — GM cap-bit definitions + table.
//!
//! Bit-flag cap-namespace for the GM role only. Cross-role caps
//! (CODER_CAP_*, DM_CAP_*, COCREATE_CAP_*) live in their own crates ;
//! the GM cannot exercise them and never imports them.

use serde::{Deserialize, Serialize};

/// `GM_CAP_TEXT_EMIT` — permit narrative-text emission.
pub const GM_CAP_TEXT_EMIT: u32 = 1;
/// `GM_CAP_VOICE_EMIT` — permit synthesized-speech emission (deferred).
pub const GM_CAP_VOICE_EMIT: u32 = 2;
/// `GM_CAP_TONE_TUNE` — permit cocreative-bias-tone adjustment.
pub const GM_CAP_TONE_TUNE: u32 = 4;

/// Granted-cap mask for one GM instance.
///
/// Stored as a `u32` bit-mask. The constants above are mutually
/// exclusive single-bit values so the mask composes as a bitwise-OR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GmCapTable {
    pub granted_bits: u32,
}

impl GmCapTable {
    /// Construct an empty (no-caps-granted) table.
    #[must_use]
    pub fn empty() -> Self {
        Self { granted_bits: 0 }
    }

    /// Construct from a raw bit-mask.
    #[must_use]
    pub fn new(granted_bits: u32) -> Self {
        Self { granted_bits }
    }

    /// Construct with all GM caps granted (for tests / dev-mode).
    #[must_use]
    pub fn all() -> Self {
        Self {
            granted_bits: GM_CAP_TEXT_EMIT | GM_CAP_VOICE_EMIT | GM_CAP_TONE_TUNE,
        }
    }

    /// Check whether `cap_bit` is granted.
    #[must_use]
    pub fn has(&self, cap_bit: u32) -> bool {
        (self.granted_bits & cap_bit) == cap_bit && cap_bit != 0
    }

    /// Add a cap-bit (returns a new table — `self` is `Copy`).
    #[must_use]
    pub fn with(self, cap_bit: u32) -> Self {
        Self {
            granted_bits: self.granted_bits | cap_bit,
        }
    }

    /// Revoke a cap-bit (returns a new table).
    #[must_use]
    pub fn without(self, cap_bit: u32) -> Self {
        Self {
            granted_bits: self.granted_bits & !cap_bit,
        }
    }
}

impl Default for GmCapTable {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_grants_nothing() {
        let t = GmCapTable::empty();
        assert!(!t.has(GM_CAP_TEXT_EMIT));
        assert!(!t.has(GM_CAP_VOICE_EMIT));
        assert!(!t.has(GM_CAP_TONE_TUNE));
    }

    #[test]
    fn all_table_grants_everything() {
        let t = GmCapTable::all();
        assert!(t.has(GM_CAP_TEXT_EMIT));
        assert!(t.has(GM_CAP_VOICE_EMIT));
        assert!(t.has(GM_CAP_TONE_TUNE));
    }

    #[test]
    fn with_and_without_compose() {
        let t = GmCapTable::empty().with(GM_CAP_TEXT_EMIT);
        assert!(t.has(GM_CAP_TEXT_EMIT));
        assert!(!t.has(GM_CAP_VOICE_EMIT));
        let t2 = t.without(GM_CAP_TEXT_EMIT);
        assert!(!t2.has(GM_CAP_TEXT_EMIT));
    }

    #[test]
    fn cap_bits_are_unique() {
        // canonical sanity : the three bits are pairwise-disjoint.
        assert_eq!(GM_CAP_TEXT_EMIT & GM_CAP_VOICE_EMIT, 0);
        assert_eq!(GM_CAP_TEXT_EMIT & GM_CAP_TONE_TUNE, 0);
        assert_eq!(GM_CAP_VOICE_EMIT & GM_CAP_TONE_TUNE, 0);
    }

    #[test]
    fn has_zero_bit_is_false() {
        // Defensive : `has(0)` should not be true for any table.
        let t = GmCapTable::all();
        assert!(!t.has(0));
    }
}
