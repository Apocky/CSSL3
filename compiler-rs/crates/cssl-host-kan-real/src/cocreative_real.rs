//! § cocreative_real — REAL stage-1 KAN cocreative-bias scorer.
//!
//! § PIPELINE
//!   features (≥ FEATURE_DIM-D feature-vec)
//!     → encode_existing (zero-pad / truncate / clamp to FEATURE_DIM)
//!     → KanRuntime<FEATURE_DIM, 1>::eval
//!     → sigmoid → f32 ∈ [0,1] preference-weight
//!     → audit-emit
//!
//! § FALLBACK
//!   When the KAN runtime is `None` ⇒ delegate to the stage-0 fallback
//!   (typically `Stage0DotProductClassifier`).

use crate::adapter::{sigmoid, KanRuntime};
use crate::audit::{audit_log, fnv1a_64, AuditEvent};
use crate::feature_encode::{encode_existing, FEATURE_DIM};
use cssl_host_kan_substrate_bridge::cocreative_classifier::CocreativeClassifier;

/// § REAL stage-1 KAN cocreative-bias classifier.
pub struct RealCocreativeKanClassifier {
    /// § Optional KAN runtime. `None` ⇒ pure-fallback per I-4.
    pub runtime: Option<KanRuntime<FEATURE_DIM, 1>>,
    /// § Stage-0 fallback.
    pub fallback: Box<dyn CocreativeClassifier>,
    /// § Stable impl-id for audit events.
    pub impl_id: &'static str,
}

impl RealCocreativeKanClassifier {
    /// § Construct with an explicit runtime + fallback.
    #[must_use]
    pub fn new(
        runtime: KanRuntime<FEATURE_DIM, 1>,
        fallback: Box<dyn CocreativeClassifier>,
    ) -> Self {
        Self {
            runtime: Some(runtime),
            fallback,
            impl_id: "real-kan",
        }
    }

    /// § Construct with NO runtime (pure-fallback).
    #[must_use]
    pub fn pure_fallback(fallback: Box<dyn CocreativeClassifier>) -> Self {
        Self {
            runtime: None,
            fallback,
            impl_id: "stage-0-fallback",
        }
    }

    /// § Construct with a baked runtime (deterministic seed) + fallback.
    #[must_use]
    pub fn with_baked_seed(seed: u64, fallback: Box<dyn CocreativeClassifier>) -> Self {
        let mut runtime = KanRuntime::<FEATURE_DIM, 1>::new_untrained();
        runtime.bake_from_seed(seed);
        Self::new(runtime, fallback)
    }

    /// § Internal : run the KAN forward-pass on a feature-vec and emit
    ///   the [0, 1] preference-weight.
    fn kan_forward(&self, features: &[f32; FEATURE_DIM]) -> f32 {
        let runtime = match self.runtime.as_ref() {
            Some(r) => r,
            None => return 0.0,
        };
        let raw = runtime.eval(features);
        sigmoid(raw[0])
    }
}

impl CocreativeClassifier for RealCocreativeKanClassifier {
    fn name(&self) -> &'static str {
        "stage1-kan-real-cocreative"
    }

    fn score_features(&self, features: &[f32]) -> f32 {
        // I-1 : deterministic encoding.
        let encoded = encode_existing(features);
        // FNV over the byte-rep of the input vec for the audit event.
        let mut buf = Vec::with_capacity(features.len() * 4);
        for f in features {
            buf.extend_from_slice(&f.to_le_bytes());
        }
        let in_hash = fnv1a_64(&buf);

        let raw_score = match self.runtime.as_ref() {
            Some(r) if r.is_trained() => self.kan_forward(&encoded),
            _ => self.fallback.score_features(features),
        };

        // I-2 : clamp + NaN defense.
        let safe = if raw_score.is_finite() {
            raw_score.clamp(0.0, 1.0)
        } else {
            0.0
        };

        // I-3 : audit-emit.
        audit_log(AuditEvent {
            sp_id: "cocreative_score",
            impl_id: self.impl_id,
            in_hash,
            out_hash: fnv1a_64(&safe.to_le_bytes()),
        });

        safe
    }

    fn rank_candidates(&self, candidates: &[Vec<f32>]) -> Vec<usize> {
        let mut indexed: Vec<(usize, f32)> = candidates
            .iter()
            .enumerate()
            .map(|(i, x)| (i, self.score_features(x)))
            .collect();
        // Stable sort : descending by score, ties preserve original order.
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        indexed.into_iter().map(|(i, _)| i).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_kan_substrate_bridge::cocreative_classifier::Stage0DotProductClassifier;

    #[test]
    fn real_kan_score_in_unit_interval() {
        let fb = Box::new(Stage0DotProductClassifier::default_dim4());
        let c = RealCocreativeKanClassifier::with_baked_seed(123, fb);
        let s = c.score_features(&[0.5, 0.5, 0.5, 0.5]);
        assert!(s >= 0.0 && s <= 1.0);
    }

    #[test]
    fn real_kan_deterministic() {
        let fb1 = Box::new(Stage0DotProductClassifier::default_dim4());
        let c1 = RealCocreativeKanClassifier::with_baked_seed(123, fb1);
        let fb2 = Box::new(Stage0DotProductClassifier::default_dim4());
        let c2 = RealCocreativeKanClassifier::with_baked_seed(123, fb2);
        let s1 = c1.score_features(&[0.5, 0.5, 0.5, 0.5]);
        let s2 = c2.score_features(&[0.5, 0.5, 0.5, 0.5]);
        assert_eq!(s1, s2);
    }

    #[test]
    fn real_kan_handles_huge_input() {
        let fb = Box::new(Stage0DotProductClassifier::default_dim4());
        let c = RealCocreativeKanClassifier::with_baked_seed(7, fb);
        let huge: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let s = c.score_features(&huge);
        assert!(s.is_finite());
        assert!(s >= 0.0 && s <= 1.0);
    }

    #[test]
    fn pure_fallback_uses_stage0() {
        let fb = Box::new(Stage0DotProductClassifier::new(vec![1.0, 1.0]));
        let c = RealCocreativeKanClassifier::pure_fallback(fb);
        // Stage-0 dot([1,1], [3,4]) = 7. But we clamp to [0,1] so = 1.0.
        let s = c.score_features(&[3.0, 4.0]);
        // Score is clamped to [0, 1] post-classification ; raw was 7.0.
        assert!((s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn nan_input_safe() {
        let fb = Box::new(Stage0DotProductClassifier::new(vec![1.0, 1.0]));
        let c = RealCocreativeKanClassifier::with_baked_seed(7, fb);
        let s = c.score_features(&[f32::NAN, f32::NAN]);
        assert!(s.is_finite());
    }

    #[test]
    fn rank_returns_indices() {
        let fb = Box::new(Stage0DotProductClassifier::default_dim4());
        let c = RealCocreativeKanClassifier::with_baked_seed(11, fb);
        let cands = vec![
            vec![1.0, 1.0, 1.0, 1.0],
            vec![0.1, 0.1, 0.1, 0.1],
            vec![0.5, 0.5, 0.5, 0.5],
        ];
        let r = c.rank_candidates(&cands);
        assert_eq!(r.len(), 3);
        // All original indices appear exactly once.
        let mut sorted = r.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2]);
    }
}
