// § tests/tokenize.rs — pure-fn tokenizer invariants.

use cssl_host_substrate_knowledge::tokenize;

#[test]
fn tokenize_strips_punctuation() {
    // "(foo)," and "foo" must hash to same u32 ⇒ resulting set has exactly 1 elem.
    let a = tokenize("(foo),");
    let b = tokenize("foo");
    assert_eq!(a, b, "punctuation should not affect hashing");
}

#[test]
fn tokenize_lowercases() {
    let a = tokenize("Substrate");
    let b = tokenize("SUBSTRATE");
    let c = tokenize("substrate");
    assert_eq!(a, b);
    assert_eq!(b, c);
}

#[test]
fn tokenize_filters_short() {
    // Single 2-char and 1-char tokens dropped ; only "foo" survives.
    let r = tokenize("a bb foo");
    assert_eq!(r.len(), 1, "expected only 'foo' to survive, got {r:?}");
}

#[test]
fn tokenize_dedupes() {
    let r = tokenize("substrate substrate substrate");
    assert_eq!(r.len(), 1, "duplicates must collapse");
}

#[test]
fn tokenize_empty() {
    let r = tokenize("");
    assert!(r.is_empty());
    let r2 = tokenize("   \t\n  ");
    assert!(r2.is_empty());
}

#[test]
fn tokenize_unicode_safe() {
    // Must not panic on multibyte input ; should produce ≥ 1 token here.
    let r = tokenize("σ-chain mycélium 共有 ω-field");
    // We don't assert exact count — char-class behavior on punct is platform-stable
    // but unicode tokens still pass len-≥-3 by char-count.
    assert!(!r.is_empty(), "got empty for unicode input");
    // Ensure sorted+dedup invariant.
    for w in r.windows(2) {
        assert!(w[0] < w[1], "tokens must be sorted+deduped");
    }
}
