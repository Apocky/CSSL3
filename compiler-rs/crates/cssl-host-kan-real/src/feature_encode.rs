//! § feature_encode — utterance / history → ≥32-D feature-vector encoder.
//!
//! § DESIGN
//!   Deterministic byte-hash + RFF-style sin/cos projection. The encoder
//!   is :
//!
//!     1. Tokenize : split-whitespace, lowercase, alphanum-only.
//!     2. Per-token : FNV-1a-32 fingerprint → seed.
//!     3. Seeded sin/cos at multiple frequencies projected into the
//!        feature-vec.
//!     4. Length / count-of-tokens written into trailing slots.
//!
//!   This is NOT a true Random-Fourier-Features (RFF) embedding — it is
//!   inspired by RFF's deterministic-projection trick and is sufficient
//!   for the stage-1 KAN classifier head. A true RFF embedding requires
//!   sampled-from-Gaussian projection vectors, which would need a binary
//!   blob committed to the repo. The seeded-sin path is byte-stable
//!   across hosts and trivially reproducible.
//!
//! § DETERMINISM
//!   No `SystemTime`, no `thread_rng`. Output is purely a function of the
//!   input string. Equal strings ⇒ equal feature-vecs (I-1 invariant).

use crate::audit::fnv1a_64;

/// § Default feature-dim. Matches the KAN classifier head's `I = 32`.
pub const FEATURE_DIM: usize = 32;

/// § Feature-encoder configuration.
#[derive(Debug, Clone, Copy)]
pub struct FeatureEncodeConfig {
    /// § Per-instance seed mixed into the projection. Defaults to `0`.
    pub seed: u64,
    /// § Maximum tokens to consider. Excess tokens are ignored (bounded
    ///   compute). Defaults to 64.
    pub max_tokens: usize,
}

impl Default for FeatureEncodeConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            max_tokens: 64,
        }
    }
}

/// § Encode an utterance / arbitrary string into a `FEATURE_DIM`-D
///   feature-vec. The output is bounded in `[-1, 1]^FEATURE_DIM` and
///   deterministic.
///
/// § PIPELINE
///   1. Tokenize (whitespace + alphanum-only ; lowercase).
///   2. Seed an accumulator from the config.seed.
///   3. For each token, hash it → use the hash as a per-token phase ;
///      sum sin/cos of (phase + i·step) into the feature-vec at index `i`.
///   4. Encode token-count + total-length into the last 2 slots.
///   5. Tanh-clamp the whole vector for boundedness.
#[must_use]
pub fn encode_features(text: &str, config: FeatureEncodeConfig) -> [f32; FEATURE_DIM] {
    let mut feats = [0.0_f32; FEATURE_DIM];

    // Step 1 + 2 : tokenize with case-fold + alphanum-filter.
    let mut tokens_seen = 0_u32;
    let mut total_len = 0_u32;
    for raw in text.split_whitespace().take(config.max_tokens) {
        // Strip non-alphanumeric ; lowercase.
        let normalized: String = raw
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        if normalized.is_empty() {
            continue;
        }
        tokens_seen = tokens_seen.saturating_add(1);
        total_len = total_len.saturating_add(normalized.len() as u32);

        // Mix seed into the per-token hash via a multiplicative + xor
        // perturbation (large prime) so distinct seeds yield linearly-
        // independent phase trajectories — a simple add wraps modulo 2^64
        // and can produce numerically-identical phases after the float
        // cast at small phase scales.
        let h = fnv1a_64(normalized.as_bytes())
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(config.seed.wrapping_mul(0x100_0000_01b3));
        // Project into the feature-vec.
        for i in 0..(FEATURE_DIM - 2) {
            let phase = (h.wrapping_add((i as u64).wrapping_mul(2654435761))) as f32 * 1e-3;
            // Even slots = sin ; odd slots = cos.
            let component = if i % 2 == 0 { phase.sin() } else { phase.cos() };
            feats[i] += component;
        }
    }

    // Step 4 : encode token-count + total-len into the last 2 slots.
    feats[FEATURE_DIM - 2] = (tokens_seen as f32) * 0.1;
    feats[FEATURE_DIM - 1] = (total_len as f32) * 0.01;

    // Step 5 : tanh-clamp for boundedness.
    for f in &mut feats {
        if !f.is_finite() {
            *f = 0.0;
        } else {
            *f = f.tanh();
        }
    }
    feats
}

/// § Encode a flat `[(key, value)]` arg-list (the IntentClass.args shape)
///   into a feature-vec. Keys + values are concatenated then re-encoded.
///   Useful for the seed-classifier when it needs to condition on
///   intent-args + zone-id.
#[must_use]
pub fn encode_args(args: &[(String, String)], config: FeatureEncodeConfig) -> [f32; FEATURE_DIM] {
    let mut buf = String::with_capacity(64);
    for (k, v) in args {
        buf.push_str(k);
        buf.push(' ');
        buf.push_str(v);
        buf.push(' ');
    }
    encode_features(&buf, config)
}

/// § Encode an existing `[f32]` feature-vec (e.g. from cocreative's
///   pre-computed bias-axis) by zero-padding / truncating to FEATURE_DIM.
#[must_use]
pub fn encode_existing(features: &[f32]) -> [f32; FEATURE_DIM] {
    let mut out = [0.0_f32; FEATURE_DIM];
    let n = features.len().min(FEATURE_DIM);
    for i in 0..n {
        let v = features[i];
        out[i] = if v.is_finite() { v.tanh() } else { 0.0 };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_same_string() {
        let cfg = FeatureEncodeConfig::default();
        let a = encode_features("walk forward", cfg);
        let b = encode_features("walk forward", cfg);
        assert_eq!(a, b);
    }

    #[test]
    fn different_strings_different_features() {
        let cfg = FeatureEncodeConfig::default();
        let a = encode_features("walk forward", cfg);
        let b = encode_features("examine altar", cfg);
        assert_ne!(a, b);
    }

    #[test]
    fn empty_input_returns_zero_ish() {
        let cfg = FeatureEncodeConfig::default();
        let f = encode_features("", cfg);
        // No tokens ⇒ token-count = 0 ⇒ slots[FEATURE_DIM-2] = 0.
        assert!((f[FEATURE_DIM - 2]).abs() < 1e-6);
    }

    #[test]
    fn output_bounded() {
        let cfg = FeatureEncodeConfig::default();
        let f = encode_features("aaaa bbbb cccc dddd eeee ffff", cfg);
        for v in &f {
            assert!(v.is_finite());
            assert!(*v >= -1.0 && *v <= 1.0);
        }
    }

    #[test]
    fn seed_changes_output() {
        let cfg_a = FeatureEncodeConfig {
            seed: 0,
            max_tokens: 16,
        };
        let cfg_b = FeatureEncodeConfig {
            seed: 1,
            max_tokens: 16,
        };
        let a = encode_features("hello world", cfg_a);
        let b = encode_features("hello world", cfg_b);
        assert_ne!(a, b);
    }

    #[test]
    fn case_folded() {
        let cfg = FeatureEncodeConfig::default();
        let a = encode_features("Hello", cfg);
        let b = encode_features("HELLO", cfg);
        assert_eq!(a, b);
    }

    #[test]
    fn punct_stripped() {
        let cfg = FeatureEncodeConfig::default();
        let a = encode_features("hello!", cfg);
        let b = encode_features("hello", cfg);
        assert_eq!(a, b);
    }

    #[test]
    fn encode_existing_pads_and_clamps() {
        let f = encode_existing(&[100.0, -100.0, 0.5]);
        assert!(f[0] > 0.99);
        assert!(f[1] < -0.99);
        assert!((f[2] - 0.5_f32.tanh()).abs() < 1e-6);
        // Padded zeros for the rest.
        for i in 3..FEATURE_DIM {
            assert!(f[i].abs() < 1e-9);
        }
    }
}
