//! § tokenize — shared tokenizer mirroring build.rs's exact transform so that
//! runtime queries hash to the *same* `u32` keys as the build-time corpus.
//!
//! § rules
//!   * lowercase
//!   * strip the punctuation set `[],{}()<>"'\`,.;:!?`
//!   * whitespace-split
//!   * keep only tokens with `≥ 3` `char`s
//!   * dedupe (deterministic, BTreeSet)
//!   * cap at 256 tokens (matches build.rs)
//!   * blake3 → first-4-bytes-LE ⇒ `u32`
//!   * sort + dedupe the resulting `u32` set so jaccard merge is O(n+m)

use std::collections::BTreeSet;

const STRIPS: &[char] = &[
    '[', ']', '{', '}', '(', ')', '<', '>', '"', '\'', '`', ',', '.', ';', ':', '!', '?',
];

/// Tokenize `s` into the same `u32`-hash set the build.rs corpus used.
///
/// Returns sorted + deduped `Vec<u32>` ready for `jaccard` over another
/// build-time bag.
pub fn tokenize(s: &str) -> Vec<u32> {
    let mut seen = BTreeSet::new();
    let mut hashes = Vec::new();
    for raw in s.split_whitespace() {
        let cleaned: String = raw
            .chars()
            .filter(|c| !STRIPS.contains(c))
            .collect::<String>()
            .to_lowercase();
        if cleaned.chars().count() < 3 {
            continue;
        }
        if !seen.insert(cleaned.clone()) {
            continue;
        }
        let h = blake3::hash(cleaned.as_bytes());
        let b = h.as_bytes();
        hashes.push(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if hashes.len() >= 256 {
            break;
        }
    }
    hashes.sort_unstable();
    hashes.dedup();
    hashes
}
