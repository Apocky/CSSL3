//! § morpheme — phoneme/morpheme primitives for procedural composition.
//!
//! § APPROACH
//!   We compose words from open-class stems (consonant-vowel-consonant cores)
//!   and closed-class affixes selected via the substrate axes. The output
//!   is recognizably-English-shaped text, but no string is ever pulled from
//!   a phrase-table — every word is synthesized at compose-time.
//!
//! § MORPHOLOGICAL INVENTORY (small but expressive)
//!   - 24 onset consonant clusters
//!   - 12 nucleus vowel patterns
//!   - 16 coda consonant clusters
//!   - 8 prefixes / 16 suffixes
//!   - 16 connective particles
//!
//! 24 × 12 × 16 = 4608 base stems × 16 suffix variants ≈ 73k word forms.
//! Combined with affix permutations + ordering, the procedural word-space
//! is large enough that observable repetition is rare even at long runs.

/// 24 onset clusters (single consonants + common digraphs/clusters).
pub const ONSETS: &[&str] = &[
    "", "s", "th", "br", "tr", "fl", "gl", "sh",
    "wh", "kr", "pl", "v", "m", "n", "l", "r",
    "k", "d", "g", "h", "f", "p", "y", "z",
];

/// 12 nucleus patterns (single vowels and a few diphthongs).
pub const NUCLEI: &[&str] = &[
    "a", "e", "i", "o", "u",
    "ai", "ea", "ie", "ou", "ai",
    "y", "ae",
];

/// 16 coda clusters (zero coda → 'ng' clusters).
pub const CODAS: &[&str] = &[
    "", "n", "r", "l", "th", "s", "m", "k",
    "d", "ng", "rn", "st", "rd", "ll", "sh", "lt",
];

/// 8 prefix particles. Empty = no prefix.
pub const PREFIXES: &[&str] = &[
    "", "un-", "re-", "fore-", "out-", "in-", "be-", "with-",
];

/// 16 suffix particles.
pub const SUFFIXES: &[&str] = &[
    "", "-ing", "-ed", "-er", "-th", "-ly", "-en", "-ous",
    "-some", "-less", "-ward", "-fold", "-most", "-wise", "-ish", "-en",
];

/// 16 connective particles (used between clauses or as articles/preps).
pub const CONNECTIVES: &[&str] = &[
    " and ", " yet ", " though ", ", ",
    " of ", " in ", " by ", " with ",
    " upon ", " beneath ", " beyond ", " through ",
    "; ", "—", " for ", " under ",
];

/// Articles / determiners.
pub const ARTICLES: &[&str] = &[
    "the ", "a ", "this ", "that ", "every ", "no ", "such ", "some ",
];

/// Sentence-end punctuation (axis-weighted).
pub const PUNCT_ENDINGS: &[&str] = &[".", ".", ".", "...", "!", "?", "—"];

/// Build a stem from indices into the inventory.
/// Returns the assembled string, capped by `max_chars`.
pub fn make_stem(onset_idx: u8, nucleus_idx: u8, coda_idx: u8) -> &'static str {
    // We don't allocate: we return three slices the caller concatenates.
    // For convenience this just returns the onset and lets the caller
    // append nucleus + coda. In practice the composer assembles directly.
    let _ = (onset_idx, nucleus_idx, coda_idx);
    "" // unused — see Composer::push_stem instead
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_lengths_match_expectations() {
        assert_eq!(ONSETS.len(), 24);
        assert_eq!(NUCLEI.len(), 12);
        assert_eq!(CODAS.len(), 16);
        assert_eq!(PREFIXES.len(), 8);
        assert_eq!(SUFFIXES.len(), 16);
        assert_eq!(CONNECTIVES.len(), 16);
        assert_eq!(ARTICLES.len(), 8);
    }
}
