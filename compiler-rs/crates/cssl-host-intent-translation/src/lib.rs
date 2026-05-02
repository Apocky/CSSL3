//! § cssl-host-intent-translation — STAGE-0-BOOTSTRAP-SHIM for intent_translation.csl
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-C-INTENT · canonical-impl : `Labyrinth of Apocalypse/systems/intent_translation.csl`
//!
//! § THESIS  (verbatim from .csl spec)
//!
//!   "I want to be able to describe things in text or voice and they crystalize
//!    from the substrate novel and exotic rendering techniques or something even
//!    more alien than 'rendering' entirely!"
//!
//! Player utterance is not "describing what to render" — player utterance IS
//! the substrate-mutation-request.
//!
//! § STAGE-0 PIPELINE
//!
//!   text-input (UTF-8)  ──→ tokenize::Tokenizer  ──→ Token-stream
//!                            (BLAKE3-derive vocab-id ; deterministic)
//!                                 ↓
//!                       intent_category::classify_tokens
//!                            (10-category keyword/axis classifier)
//!                                 ↓
//!                       resolution::IntentResolution::from_classification
//!                            (text + dispatch fingerprints)
//!                                 ↓
//!                       resolution.spawn() / spawn_at(pos)
//!                            ↓
//!                       Crystal { class · curves · spectral · hdc · sigma_mask }
//!
//! § AXIOMS  (inherited from intent_translation.csl + PRIME_DIRECTIVE)
//!
//!   t∞: voice-recognition LOCAL-only · ¬ external-ASR-API · ¬ network-call
//!   t∞: text-input LOCAL-parsed · BLAKE3-keyed-derivation ¬ external-tokenizer
//!   t∞: intent-translation Σ-mask-gated · denied-cap = silent-no-translate
//!   t∞: deterministic translate · same-utterance + same-seed ⇒ same-fingerprint
//!   t∞: NO external LLM · NO transformer · NO neural classifier
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody. All tokenization is local. All classification is local.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod intent_category;
pub mod resolution;
pub mod tokenize;

pub use intent_category::{Classification, IntentCategory, KeywordCodebook};
pub use resolution::{compute_dispatch_fingerprint, compute_text_fingerprint, IntentResolution};
pub use tokenize::{Token, TokenIter, TokenOwned, Tokenizer};

pub use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};

// ══════════════════════════════════════════════════════════════════════════
// § Constants — match `intent_translation.csl` const-decls
// ══════════════════════════════════════════════════════════════════════════

pub const INPUT_MODE_TEXT: u32 = 0;
pub const INPUT_MODE_VOICE: u32 = 1;
pub const INPUT_MODE_HYBRID: u32 = 2;
pub const INPUT_MODE_INVALID: u32 = 255;

pub const HDC_DIM: u32 = 10000;
pub const PHONEME_VOCAB_SIZE: u32 = 64;
pub const TEXT_TOKEN_VOCAB: u32 = 30000;
pub const KAN_CONF_THRESHOLD: u32 = 70;

// ══════════════════════════════════════════════════════════════════════════
// § Headline pure-Rust API
// ══════════════════════════════════════════════════════════════════════════

/// Translate a UTF-8 input string into a fully-populated IntentResolution.
///
/// Same `(text, seed)` pair always produces the same `IntentResolution.
/// dispatch_fingerprint` AND (when `.spawn()`d) the same `Crystal.fingerprint`.
pub fn translate_text(text: &str, seed: u64) -> IntentResolution {
    let codebook = KeywordCodebook::build();
    translate_text_with_codebook(text, seed, &codebook)
}

/// Variant that accepts a caller-cached codebook for hot loops.
pub fn translate_text_with_codebook(
    text: &str,
    seed: u64,
    codebook: &KeywordCodebook,
) -> IntentResolution {
    let tokenizer = Tokenizer::new(text);
    let mut token_count: u32 = 0;
    let classification = {
        let mut hits = [0u32; 10];
        for tok in tokenizer.iter() {
            token_count = token_count.saturating_add(1);
            for (idx, cat) in IntentCategory::ALL.iter().enumerate() {
                if codebook.is_keyword(*cat, tok.vocab_id) {
                    hits[idx] = hits[idx].saturating_add(1);
                }
            }
        }
        if token_count == 0 {
            Classification {
                category: IntentCategory::Invalid,
                confidence_pct: 0,
            }
        } else {
            let mut best_idx = 0usize;
            let mut best_hits = hits[0];
            for i in 1..10 {
                if hits[i] > best_hits {
                    best_hits = hits[i];
                    best_idx = i;
                }
            }
            if best_hits == 0 {
                Classification {
                    category: IntentCategory::Ambiguous,
                    confidence_pct: 0,
                }
            } else {
                let confidence_raw = (best_hits.saturating_mul(100)) / token_count;
                let confidence_pct = confidence_raw.min(100) as u8;
                let category = if confidence_pct < KAN_CONF_THRESHOLD as u8 {
                    IntentCategory::Ambiguous
                } else {
                    IntentCategory::ALL[best_idx]
                };
                Classification {
                    category,
                    confidence_pct,
                }
            }
        }
    };
    let text_fingerprint = compute_text_fingerprint(text);
    IntentResolution::from_classification(classification, seed, token_count, text_fingerprint)
}

// ══════════════════════════════════════════════════════════════════════════
// § extern "C" surface — mirrors intent_translation.csl FFI decls
// ══════════════════════════════════════════════════════════════════════════

pub fn pack_classification(c: Classification, token_count: u32) -> u32 {
    let cat_byte = (c.category.as_u32() & 0xFF) as u32;
    let conf_byte = u32::from(c.confidence_pct) & 0xFF;
    let tok_low = token_count & 0xFFFF;
    (cat_byte << 24) | (conf_byte << 16) | tok_low
}

pub const fn unpack_category(packed: u32) -> u32 {
    (packed >> 24) & 0xFF
}

pub const fn unpack_confidence(packed: u32) -> u32 {
    (packed >> 16) & 0xFF
}

pub const fn unpack_token_count(packed: u32) -> u32 {
    packed & 0xFFFF
}

#[no_mangle]
pub extern "C" fn intent_pack_category(cat: u32) -> u32 {
    cat
}

#[no_mangle]
pub extern "C" fn intent_extract_category(packed: u32) -> u32 {
    packed
}

#[no_mangle]
pub extern "C" fn intent_dispatch_fingerprint_ffi(
    category: u32,
    seed_lo: u32,
    seed_hi: u32,
    token_count: u32,
    text_fingerprint: u32,
) -> u32 {
    let seed = u64::from(seed_lo) | (u64::from(seed_hi) << 32);
    let cat = IntentCategory::from_u32(category);
    compute_dispatch_fingerprint(cat, seed, token_count, text_fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_empty_is_invalid() {
        let r = translate_text("", 0);
        assert_eq!(r.category, IntentCategory::Invalid);
        assert_eq!(r.token_count, 0);
        assert!(r.spawn().is_none());
    }

    #[test]
    fn translate_describe_object() {
        let r2 = translate_text("sword shield armor", 1);
        assert_eq!(r2.category, IntentCategory::DescribeObject);
        assert_eq!(r2.confidence_pct, 100);
        let c = r2.spawn().expect("describe-object should spawn");
        assert_eq!(c.class, CrystalClass::Object);
    }

    #[test]
    fn translate_is_deterministic() {
        let a = translate_text("a forest at dusk", 42);
        let b = translate_text("a forest at dusk", 42);
        assert_eq!(a, b);
    }

    #[test]
    fn translate_varies_with_text() {
        let a = translate_text("sword", 0);
        let b = translate_text("forest", 0);
        assert_ne!(a.text_fingerprint, b.text_fingerprint);
        assert_ne!(a.dispatch_fingerprint, b.dispatch_fingerprint);
    }

    #[test]
    fn translate_varies_with_seed() {
        let a = translate_text("sword", 0);
        let b = translate_text("sword", 1);
        assert_eq!(a.category, b.category);
        assert_eq!(a.text_fingerprint, b.text_fingerprint);
        assert_ne!(a.dispatch_fingerprint, b.dispatch_fingerprint);
    }

    #[test]
    fn spawn_is_replay_safe() {
        let r1 = translate_text("sword shield armor", 7);
        let r2 = translate_text("sword shield armor", 7);
        let c1 = r1.spawn().unwrap();
        let c2 = r2.spawn().unwrap();
        assert_eq!(c1.fingerprint, c2.fingerprint);
        assert_eq!(c1.handle, c2.handle);
    }

    #[test]
    fn spawn_at_pos_uses_pos() {
        let r = translate_text("forest cathedral void", 0);
        assert_eq!(r.category, IntentCategory::DescribeEnv);
        let c = r.spawn_at(WorldPos::new(1, 2, 3)).unwrap();
        assert_eq!(c.world_pos, WorldPos::new(1, 2, 3));
        assert_eq!(c.class, CrystalClass::Environment);
    }

    #[test]
    fn all_10_categories_classifiable() {
        let cases = [
            (IntentCategory::DescribeObject, "sword shield armor"),
            (IntentCategory::DescribeEntity, "sage knight wizard"),
            (IntentCategory::DescribeEnv, "forest cathedral void"),
            (IntentCategory::DescribeBehavior, "responds reacts triggers"),
            (IntentCategory::InvokeAction, "open approach speak"),
            (IntentCategory::QueryState, "what where who"),
            (IntentCategory::RevisePrior, "more less smaller"),
            (IntentCategory::RevokeCrystallize, "remove undo delete"),
            (IntentCategory::NarrateAuthor, "narrate compose write"),
            (IntentCategory::DebugInspect, "debug inspect trace"),
        ];
        for (expected, text) in cases {
            let r = translate_text(text, 0);
            assert_eq!(
                r.category, expected,
                "text {text:?} should classify to {expected:?} but got {:?}",
                r.category
            );
        }
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let c = Classification {
            category: IntentCategory::DescribeEnv,
            confidence_pct: 85,
        };
        let packed = pack_classification(c, 7);
        assert_eq!(unpack_category(packed), 2);
        assert_eq!(unpack_confidence(packed), 85);
        assert_eq!(unpack_token_count(packed), 7);
    }

    #[test]
    fn ffi_dispatch_fingerprint_matches_pure_api() {
        let pure = compute_dispatch_fingerprint(IntentCategory::DescribeObject, 0xCAFE_BABE_1234_5678, 5, 0xDEAD_BEEF);
        let ffi = intent_dispatch_fingerprint_ffi(
            IntentCategory::DescribeObject.as_u32(),
            0x1234_5678,
            0xCAFE_BABE,
            5,
            0xDEAD_BEEF,
        );
        assert_eq!(pure, ffi);
    }

    #[test]
    fn ffi_pack_extract_identity() {
        assert_eq!(intent_pack_category(3), 3);
        assert_eq!(intent_extract_category(7), 7);
    }

    #[test]
    fn const_surface_matches_csl() {
        assert_eq!(INPUT_MODE_TEXT, 0);
        assert_eq!(INPUT_MODE_VOICE, 1);
        assert_eq!(INPUT_MODE_HYBRID, 2);
        assert_eq!(INPUT_MODE_INVALID, 255);
        assert_eq!(HDC_DIM, 10000);
        assert_eq!(PHONEME_VOCAB_SIZE, 64);
        assert_eq!(TEXT_TOKEN_VOCAB, 30000);
        assert_eq!(KAN_CONF_THRESHOLD, 70);
    }

    #[test]
    fn translate_with_codebook_matches_translate() {
        let cb = KeywordCodebook::build();
        let a = translate_text("a sword that opens portals", 42);
        let b = translate_text_with_codebook("a sword that opens portals", 42, &cb);
        assert_eq!(a, b);
    }
}
