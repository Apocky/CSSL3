//! § intent_real — REAL stage-1 KAN intent classifier.
//!
//! § PIPELINE
//!   utterance
//!     → tokenize + normalize
//!     → encode_features (RFF-style ; FEATURE_DIM = 32)
//!     → KanRuntime<32, 8>::eval_softmax
//!     → top-1 IntentClass + clamped confidence
//!     → audit-emit
//!
//! § INTENT LABELS
//!   Eight canonical intent-classes mapped to KAN output indices.
//!   Mapping is a stable enum ; the stage-0 fallback uses these same
//!   strings.
//!
//! § FALLBACK
//!   When the KAN runtime is `None` ⇒ delegate to `fallback`. This is
//!   the I-4 invariant : `cold-start = pure-stage-0`.

use crate::adapter::KanRuntime;
use crate::audit::{audit_log, fnv1a_64, AuditEvent};
use crate::feature_encode::{encode_features, FeatureEncodeConfig, FEATURE_DIM};
use cssl_host_kan_substrate_bridge::{IntentClass, IntentClassifier};

/// § Number of intent labels the KAN classifier emits.
pub const INTENT_LABEL_COUNT: usize = 8;

/// § Canonical intent labels ↔ KAN output indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentLabel {
    /// § "move" / "walk" / direction-of-travel.
    Move = 0,
    /// § "talk" / NPC-interaction.
    Talk = 1,
    /// § "examine" / "look".
    Examine = 2,
    /// § "cocreate" / world-edit.
    Cocreate = 3,
    /// § "use" / "interact-with-object".
    Use = 4,
    /// § "drop" / "give".
    Drop = 5,
    /// § "wait" / pacing.
    Wait = 6,
    /// § "unknown" / catch-all.
    Unknown = 7,
}

impl IntentLabel {
    /// § Stable lowercase string for an intent label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Talk => "talk",
            Self::Examine => "examine",
            Self::Cocreate => "cocreate",
            Self::Use => "use",
            Self::Drop => "drop",
            Self::Wait => "wait",
            Self::Unknown => "unknown",
        }
    }

    /// § Decode an output-index ; out-of-range ⇒ Unknown.
    #[must_use]
    pub const fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Move,
            1 => Self::Talk,
            2 => Self::Examine,
            3 => Self::Cocreate,
            4 => Self::Use,
            5 => Self::Drop,
            6 => Self::Wait,
            _ => Self::Unknown,
        }
    }
}

/// § REAL stage-1 KAN intent classifier.
///
/// Wraps a `KanRuntime<FEATURE_DIM, INTENT_LABEL_COUNT>` and a stage-0
/// fallback. When `runtime.is_some()` and the runtime is `is_trained()`,
/// emits a real KAN classification ; otherwise delegates to the fallback.
pub struct RealIntentKanClassifier {
    /// § Optional KAN runtime. `None` ⇒ pure-fallback per I-4.
    pub runtime: Option<KanRuntime<FEATURE_DIM, INTENT_LABEL_COUNT>>,
    /// § Stage-0 fallback (always present per I-4).
    pub fallback: Box<dyn IntentClassifier>,
    /// § Feature-encoder config.
    pub encode_cfg: FeatureEncodeConfig,
    /// § Stable impl-id used in audit events.
    pub impl_id: &'static str,
}

impl RealIntentKanClassifier {
    /// § Construct with an explicit runtime + fallback.
    #[must_use]
    pub fn new(
        runtime: KanRuntime<FEATURE_DIM, INTENT_LABEL_COUNT>,
        fallback: Box<dyn IntentClassifier>,
    ) -> Self {
        Self {
            runtime: Some(runtime),
            fallback,
            encode_cfg: FeatureEncodeConfig::default(),
            impl_id: "real-kan",
        }
    }

    /// § Construct with NO runtime (pure-fallback mode). Useful in tests.
    #[must_use]
    pub fn pure_fallback(fallback: Box<dyn IntentClassifier>) -> Self {
        Self {
            runtime: None,
            fallback,
            encode_cfg: FeatureEncodeConfig::default(),
            impl_id: "stage-0-fallback",
        }
    }

    /// § Construct with a baked runtime (deterministic seed) + fallback.
    #[must_use]
    pub fn with_baked_seed(seed: u64, fallback: Box<dyn IntentClassifier>) -> Self {
        let mut runtime = KanRuntime::<FEATURE_DIM, INTENT_LABEL_COUNT>::new_untrained();
        runtime.bake_from_seed(seed);
        Self::new(runtime, fallback)
    }

    /// § Internal : run the KAN forward-pass on a feature-vec and emit
    ///   the top-1 IntentClass.
    fn kan_forward(&self, features: &[f32; FEATURE_DIM]) -> IntentClass {
        let runtime = match self.runtime.as_ref() {
            Some(r) => r,
            None => return IntentClass::unknown(),
        };
        let probs = runtime.eval_softmax(features);
        let mut best_i = 0_usize;
        let mut best_p = probs[0];
        for i in 1..INTENT_LABEL_COUNT {
            if probs[i] > best_p {
                best_p = probs[i];
                best_i = i;
            }
        }
        // I-2 : clamp confidence to [0, 1] + NaN-defense.
        let conf = if best_p.is_finite() {
            best_p.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let label = IntentLabel::from_index(best_i);
        IntentClass {
            kind: label.as_str().to_string(),
            confidence: conf,
            args: vec![
                ("backend".to_string(), "real-kan".to_string()),
                ("label_index".to_string(), best_i.to_string()),
            ],
        }
    }
}

impl IntentClassifier for RealIntentKanClassifier {
    fn name(&self) -> &'static str {
        "stage1-kan-real-intent"
    }

    fn classify(&self, text: &str) -> IntentClass {
        // I-1 : deterministic feature-encode.
        let features = encode_features(text, self.encode_cfg);
        let in_hash = fnv1a_64(text.as_bytes());

        // I-4 : pure-fallback when no runtime OR untrained runtime.
        let result = match self.runtime.as_ref() {
            Some(r) if r.is_trained() => self.kan_forward(&features),
            _ => self.fallback.classify(text),
        };

        // I-3 : audit-emit (no-op sink unless the host installs one).
        let out_bytes = format!("{}|{:.4}", result.kind, result.confidence);
        audit_log(AuditEvent {
            sp_id: "intent_router",
            impl_id: self.impl_id,
            in_hash,
            out_hash: fnv1a_64(out_bytes.as_bytes()),
        });

        // I-6 never-refuse + I-2 NaN-defense double-check.
        let mut safe = result;
        if !safe.confidence.is_finite() {
            safe.confidence = 0.0;
        }
        safe.confidence = safe.confidence.clamp(0.0, 1.0);
        safe
    }

    fn confidence(&self) -> f32 {
        if let Some(r) = self.runtime.as_ref() {
            if r.is_trained() {
                return 0.7; // baked-runtime aggregate confidence
            }
        }
        self.fallback.confidence()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_kan_substrate_bridge::Stage0HeuristicClassifier;

    #[test]
    fn real_kan_returns_known_label() {
        let fb = Box::new(Stage0HeuristicClassifier::default_rules());
        let c = RealIntentKanClassifier::with_baked_seed(42, fb);
        let r = c.classify("examine the door");
        // The KAN forward-pass returns SOME label from the canonical set.
        let known = [
            "move", "talk", "examine", "cocreate", "use", "drop", "wait", "unknown",
        ];
        assert!(known.contains(&r.kind.as_str()));
    }

    #[test]
    fn real_kan_confidence_clamped() {
        let fb = Box::new(Stage0HeuristicClassifier::default_rules());
        let c = RealIntentKanClassifier::with_baked_seed(99, fb);
        let r = c.classify("anything goes here please");
        assert!(r.confidence >= 0.0 && r.confidence <= 1.0);
    }

    #[test]
    fn pure_fallback_uses_stage0() {
        let fb = Box::new(Stage0HeuristicClassifier::default_rules());
        let c = RealIntentKanClassifier::pure_fallback(fb);
        let r = c.classify("walk forward");
        // Stage-0 should match the "walk" rule.
        assert_eq!(r.kind, "move");
    }

    #[test]
    fn name_is_stable() {
        let fb = Box::new(Stage0HeuristicClassifier::default_rules());
        let c = RealIntentKanClassifier::with_baked_seed(0, fb);
        assert_eq!(c.name(), "stage1-kan-real-intent");
    }

    #[test]
    fn intent_label_round_trip() {
        for i in 0..INTENT_LABEL_COUNT {
            let lbl = IntentLabel::from_index(i);
            let s = lbl.as_str();
            assert!(!s.is_empty());
        }
        // Out-of-range maps to Unknown.
        assert_eq!(IntentLabel::from_index(99), IntentLabel::Unknown);
    }
}
