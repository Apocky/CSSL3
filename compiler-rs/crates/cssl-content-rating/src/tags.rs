//! § tags — 16-bit player-selectable tag bitset.
//!
//! Stage-0 narrow taxonomy. Every tag is positive-or-neutral framing — there
//! is no "bad" tag (cosmetic-axiom + welcoming-default) ; absence of a
//! positive tag is the only signal that a rater chose not to applaud-it.
//! Trending-pages aggregate the proportion-of-raters-who-tagged-X — never
//! a one-shot can sway rank.

use serde::{Deserialize, Serialize};

/// § TAG_TOTAL — number of tags in the bitset.
pub const TAG_TOTAL: usize = 16;

/// § TAG_NAMES — canonical ordering ; index = bit-position.
pub const TAG_NAMES: [&str; TAG_TOTAL] = [
    "fun",                  // bit 0
    "balanced",             // bit 1
    "creative",             // bit 2
    "accessible",           // bit 3
    "sovereign-respectful", // bit 4
    "remix-worthy",         // bit 5
    "documentation-clear",  // bit 6
    "runtime-stable",       // bit 7
    "audio-quality",        // bit 8
    "visual-polish",        // bit 9
    "narrative-depth",      // bit 10
    "educational",          // bit 11
    "welcoming",            // bit 12
    "novel",                // bit 13
    "meditative",           // bit 14
    "tense",                // bit 15
];

/// § tag_index — name → bit-position. `None` if not a known tag.
#[must_use]
pub fn tag_index(name: &str) -> Option<usize> {
    TAG_NAMES.iter().position(|&n| n == name)
}

/// § TagBitset — 16-bit bitset over the canonical tag-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TagBitset(u16);

impl TagBitset {
    /// § EMPTY — no tags set.
    pub const EMPTY: Self = Self(0);

    /// § from_bits — wrap a raw u16. No validation (extra bits beyond the
    /// taxonomy are u16-representable but wasted ; tests assert no setter
    /// goes above bit 15).
    #[must_use]
    pub fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// § bits — unwrap to raw u16.
    #[must_use]
    pub fn bits(&self) -> u16 {
        self.0
    }

    /// § set — set the bit at position `idx`. Panics if idx ≥ TAG_TOTAL.
    pub fn set(&mut self, idx: usize) {
        assert!(idx < TAG_TOTAL, "tag idx {idx} out of range");
        self.0 |= 1u16 << idx;
    }

    /// § clear — clear the bit at position `idx`.
    pub fn clear(&mut self, idx: usize) {
        assert!(idx < TAG_TOTAL, "tag idx {idx} out of range");
        self.0 &= !(1u16 << idx);
    }

    /// § contains — true if bit `idx` is set.
    #[must_use]
    pub fn contains(&self, idx: usize) -> bool {
        idx < TAG_TOTAL && (self.0 & (1u16 << idx)) != 0
    }

    /// § count_ones — population count.
    #[must_use]
    pub fn count_ones(&self) -> u32 {
        self.0.count_ones()
    }

    /// § iter_set — iterate (idx, name) over set bits.
    pub fn iter_set(&self) -> impl Iterator<Item = (usize, &'static str)> + '_ {
        (0..TAG_TOTAL).filter_map(move |i| {
            if self.contains(i) {
                Some((i, TAG_NAMES[i]))
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_total_is_16() {
        assert_eq!(TAG_TOTAL, 16);
        assert_eq!(TAG_NAMES.len(), 16);
    }

    #[test]
    fn tag_names_are_distinct() {
        let mut sorted: Vec<&str> = TAG_NAMES.to_vec();
        sorted.sort_unstable();
        for w in sorted.windows(2) {
            assert_ne!(w[0], w[1], "duplicate tag : {}", w[0]);
        }
    }

    #[test]
    fn tag_index_lookup_finds_each_canonical_name() {
        for (i, n) in TAG_NAMES.iter().enumerate() {
            assert_eq!(tag_index(n), Some(i));
        }
        assert_eq!(tag_index("not-a-real-tag"), None);
    }

    #[test]
    fn bitset_set_clear_contains_roundtrip() {
        let mut b = TagBitset::EMPTY;
        b.set(0);
        b.set(15);
        assert!(b.contains(0));
        assert!(b.contains(15));
        assert!(!b.contains(7));
        b.clear(0);
        assert!(!b.contains(0));
        assert!(b.contains(15));
    }

    #[test]
    fn bitset_count_ones_matches_bits_set() {
        let mut b = TagBitset::EMPTY;
        b.set(1);
        b.set(5);
        b.set(11);
        assert_eq!(b.count_ones(), 3);
    }

    #[test]
    fn bitset_iter_set_yields_set_pairs() {
        let mut b = TagBitset::EMPTY;
        b.set(tag_index("fun").unwrap());
        b.set(tag_index("welcoming").unwrap());
        let collected: Vec<(usize, &'static str)> = b.iter_set().collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.iter().any(|&(_, n)| n == "fun"));
        assert!(collected.iter().any(|&(_, n)| n == "welcoming"));
    }
}
