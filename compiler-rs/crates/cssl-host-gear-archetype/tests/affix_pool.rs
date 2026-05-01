//! § affix_pool tests — 24 prefixes + 24 suffixes per GDD enumeration.

use cssl_host_gear_archetype::{AffixKind, Prefix, Suffix};

#[test]
fn prefix_pool_has_24_distinct_entries() {
    let all = Prefix::all();
    assert_eq!(all.len(), 24, "prefix pool must have 24 entries");
    // Distinct check via HashSet of Debug-formatted variant names.
    let mut seen = std::collections::HashSet::new();
    for p in all {
        assert!(seen.insert(p), "duplicate prefix {p:?}");
    }
}

#[test]
fn suffix_pool_has_24_distinct_entries() {
    let all = Suffix::all();
    assert_eq!(all.len(), 24, "suffix pool must have 24 entries");
    let mut seen = std::collections::HashSet::new();
    for s in all {
        assert!(seen.insert(s), "duplicate suffix {s:?}");
    }
}

#[test]
fn every_prefix_descriptor_marked_prefix() {
    for p in Prefix::all() {
        let d = p.descriptor();
        assert_eq!(d.kind, AffixKind::Prefix, "prefix {p:?} not Prefix-kind");
        let (lo, hi) = d.range;
        // Featherlight has negative range (-weight) ; assert lo ≤ hi only.
        assert!(lo <= hi, "prefix {p:?} range invalid : ({lo}, {hi})");
    }
}

#[test]
fn every_suffix_descriptor_marked_suffix_with_positive_range() {
    for s in Suffix::all() {
        let d = s.descriptor();
        assert_eq!(d.kind, AffixKind::Suffix, "suffix {s:?} not Suffix-kind");
        let (lo, hi) = d.range;
        assert!(lo <= hi, "suffix {s:?} range invalid : ({lo}, {hi})");
        assert!(lo >= 0.0, "suffix {s:?} should not have negative-low ({lo})");
    }
}
