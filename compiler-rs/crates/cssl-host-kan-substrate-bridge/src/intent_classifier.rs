//! Intent-classification trait abstraction + stage-0 heuristic + stage-1 KAN-stub impls.
//!
//! § T11-W6-KAN-BRIDGE — module 1/4
//!
//! § ROLE
//!   LoA's `intent_router` consumes free-form text from the player /
//!   collaborator and emits a typed `IntentClass` (e.g. `move`, `talk`,
//!   `examine`, `co-create`). Stage-0 today uses keyword-rules ; stage-1+
//!   replaces this with a KAN classifier that reads from the ω-field +
//!   Σ-mask substrate. The trait below pins the call-site contract so
//!   the swap is a registry-edit, not a refactor.

use serde::{Deserialize, Serialize};

/// Trait for any intent-classification backend.
///
/// Object-safe : the registry stores `Box<dyn IntentClassifier>` so a
/// stage-0 implementation and a stage-1 stub can be swapped at runtime
/// without touching call-sites.
pub trait IntentClassifier: Send + Sync {
    /// Stable identifier for this backend (e.g. `"stage0-heuristic"`).
    fn name(&self) -> &str;

    /// Classify a free-form text utterance into a typed `IntentClass`.
    ///
    /// Implementations MUST NOT panic on any `&str` input ; they should
    /// return an `IntentClass` with `kind = "unknown"` and `confidence =
    /// 0.0` for inputs they cannot recognize.
    fn classify(&self, text: &str) -> IntentClass;

    /// Self-reported aggregate confidence across the backend's training
    /// or rule-set (0.0..=1.0). Used by the host as a quality-gate hint.
    fn confidence(&self) -> f32;
}

/// Typed classification outcome.
///
/// `args` is a flat key-value list (e.g. `[("target", "altar"),
/// ("verb", "examine")]`) the call-site interprets per `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentClass {
    pub kind: String,
    pub confidence: f32,
    pub args: Vec<(String, String)>,
}

impl IntentClass {
    /// Construct the canonical `unknown` intent (kind=`"unknown"` ;
    /// confidence=0.0 ; no args).
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            kind: String::from("unknown"),
            confidence: 0.0,
            args: Vec::new(),
        }
    }
}

/// Stage-0 keyword-rule classifier.
///
/// Walks `rules` in order ; the first rule whose keyword case-folds-equal
/// any whitespace-split token of `text` wins. `confidence` is the rule's
/// own weight clamped to 0.0..=1.0.
pub struct Stage0HeuristicClassifier {
    pub rules: Vec<KeywordRule>,
}

/// One keyword → intent-kind mapping with a confidence weight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordRule {
    pub keyword: String,
    pub intent_kind: String,
    pub weight: f32,
}

impl Stage0HeuristicClassifier {
    /// Construct from a list of rules.
    #[must_use]
    pub fn new(rules: Vec<KeywordRule>) -> Self {
        Self { rules }
    }

    /// Construct with a default rule-set covering the four canonical
    /// LoA intent-kinds (`move` / `talk` / `examine` / `cocreate`).
    #[must_use]
    pub fn default_rules() -> Self {
        Self::new(vec![
            KeywordRule {
                keyword: String::from("move"),
                intent_kind: String::from("move"),
                weight: 0.9,
            },
            KeywordRule {
                keyword: String::from("walk"),
                intent_kind: String::from("move"),
                weight: 0.85,
            },
            KeywordRule {
                keyword: String::from("talk"),
                intent_kind: String::from("talk"),
                weight: 0.9,
            },
            KeywordRule {
                keyword: String::from("examine"),
                intent_kind: String::from("examine"),
                weight: 0.9,
            },
            KeywordRule {
                keyword: String::from("look"),
                intent_kind: String::from("examine"),
                weight: 0.85,
            },
            KeywordRule {
                keyword: String::from("create"),
                intent_kind: String::from("cocreate"),
                weight: 0.9,
            },
        ])
    }
}

impl IntentClassifier for Stage0HeuristicClassifier {
    fn name(&self) -> &str {
        "stage0-heuristic"
    }

    fn classify(&self, text: &str) -> IntentClass {
        let lowered = text.to_lowercase();
        for rule in &self.rules {
            for token in lowered.split_whitespace() {
                if token == rule.keyword.to_lowercase() {
                    return IntentClass {
                        kind: rule.intent_kind.clone(),
                        confidence: rule.weight.clamp(0.0, 1.0),
                        args: vec![(String::from("matched_keyword"), rule.keyword.clone())],
                    };
                }
            }
        }
        IntentClass::unknown()
    }

    fn confidence(&self) -> f32 {
        if self.rules.is_empty() {
            0.0
        } else {
            // Aggregate confidence = mean rule-weight clamped.
            let total: f32 = self.rules.iter().map(|r| r.weight.clamp(0.0, 1.0)).sum();
            (total / self.rules.len() as f32).clamp(0.0, 1.0)
        }
    }
}

/// Stage-1 KAN-stub classifier.
///
/// Carries an opaque `kan_handle` (identifier-only ; the real KAN handle
/// type lands in stage-2 alongside cssl-substrate-kan integration). When
/// `kan_handle` is `Some`, returns a canned mocked classification ; when
/// `None`, delegates to `fallback`. This lets call-sites exercise the
/// stage-1 trait-object today and validate the registry-swap without
/// any actual KAN dependency.
pub struct Stage1KanStubClassifier {
    pub fallback: Box<dyn IntentClassifier>,
    pub kan_handle: Option<String>,
}

impl Stage1KanStubClassifier {
    /// Construct with a fallback + no KAN handle (will always delegate).
    #[must_use]
    pub fn new(fallback: Box<dyn IntentClassifier>) -> Self {
        Self {
            fallback,
            kan_handle: None,
        }
    }

    /// Construct with a fallback + a mock KAN handle (returns canned).
    #[must_use]
    pub fn with_handle(fallback: Box<dyn IntentClassifier>, handle: String) -> Self {
        Self {
            fallback,
            kan_handle: Some(handle),
        }
    }
}

impl IntentClassifier for Stage1KanStubClassifier {
    fn name(&self) -> &str {
        "stage1-kan-stub"
    }

    fn classify(&self, text: &str) -> IntentClass {
        if self.kan_handle.is_some() {
            // Mocked KAN response : echoes input length as confidence-proxy.
            // Stage-2 replaces this with a real KAN forward-pass.
            IntentClass {
                kind: String::from("kan_mocked"),
                confidence: 0.5,
                args: vec![
                    (String::from("backend"), String::from("kan-mock")),
                    (String::from("input_len"), text.len().to_string()),
                ],
            }
        } else {
            self.fallback.classify(text)
        }
    }

    fn confidence(&self) -> f32 {
        if self.kan_handle.is_some() {
            0.5
        } else {
            self.fallback.confidence()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage0_classifies_known_keyword() {
        let c = Stage0HeuristicClassifier::default_rules();
        let r = c.classify("please move forward");
        assert_eq!(r.kind, "move");
        assert!(r.confidence > 0.0);
        assert_eq!(r.args[0].0, "matched_keyword");
    }

    #[test]
    fn stage0_falls_through_to_unknown() {
        let c = Stage0HeuristicClassifier::default_rules();
        let r = c.classify("zzzzzz nonsense gibberish");
        assert_eq!(r.kind, "unknown");
        assert_eq!(r.confidence, 0.0);
    }

    #[test]
    fn stage1_no_kan_falls_through() {
        let stage0 = Box::new(Stage0HeuristicClassifier::default_rules());
        let s1 = Stage1KanStubClassifier::new(stage0);
        let r = s1.classify("walk north");
        // Should match stage0's "walk" rule.
        assert_eq!(r.kind, "move");
    }

    #[test]
    fn stage1_with_kan_mocked_returns_canned() {
        let stage0 = Box::new(Stage0HeuristicClassifier::default_rules());
        let s1 = Stage1KanStubClassifier::with_handle(stage0, String::from("kan-v0-mock"));
        let r = s1.classify("anything at all");
        assert_eq!(r.kind, "kan_mocked");
        assert_eq!(r.confidence, 0.5);
        assert_eq!(r.args[0], (String::from("backend"), String::from("kan-mock")));
    }

    #[test]
    fn trait_object_safe() {
        // Compile-time check : if `IntentClassifier` is not object-safe
        // this won't compile.
        let _v: Vec<Box<dyn IntentClassifier>> = vec![
            Box::new(Stage0HeuristicClassifier::default_rules()),
            Box::new(Stage1KanStubClassifier::new(Box::new(
                Stage0HeuristicClassifier::default_rules(),
            ))),
        ];
        assert_eq!(_v.len(), 2);
    }

    #[test]
    fn classifier_name() {
        let s0 = Stage0HeuristicClassifier::default_rules();
        assert_eq!(s0.name(), "stage0-heuristic");
        let s1 = Stage1KanStubClassifier::new(Box::new(Stage0HeuristicClassifier::default_rules()));
        assert_eq!(s1.name(), "stage1-kan-stub");
    }

    #[test]
    fn args_roundtrip() {
        let intent = IntentClass {
            kind: String::from("examine"),
            confidence: 0.75,
            args: vec![
                (String::from("target"), String::from("altar")),
                (String::from("verb"), String::from("inspect")),
            ],
        };
        assert_eq!(intent.args.len(), 2);
        assert_eq!(intent.args[0].0, "target");
        assert_eq!(intent.args[1].1, "inspect");
    }

    #[test]
    fn serde_round_trip_intent_class() {
        let intent = IntentClass {
            kind: String::from("cocreate"),
            confidence: 0.85,
            args: vec![(String::from("payload"), String::from("seed"))],
        };
        let s = serde_json::to_string(&intent).expect("serialize");
        let back: IntentClass = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(intent, back);
    }
}
