//! Classifier registry — owns the three trait-objects + bundles construction.
//!
//! § T11-W6-KAN-BRIDGE — module 4/4
//!
//! § ROLE
//!   The registry is the single point at which the host wires concrete
//!   classifier impls into the LoA call-graph. `default_stage0` returns
//!   a fully-stage-0 registry ; `default_stage1_with_stubs` returns a
//!   registry where each role is a stage-1 stub backed by a stage-0
//!   fallback. `replace_*` methods let downstream tests swap one role
//!   while keeping the others.

use crate::cocreative_classifier::{
    CocreativeClassifier, Stage0DotProductClassifier,
    Stage1KanStubClassifier as Stage1CocreativeKanStubClassifier,
};
use crate::intent_classifier::{
    IntentClassifier, Stage0HeuristicClassifier,
    Stage1KanStubClassifier as Stage1IntentKanStubClassifier,
};
use crate::seed_classifier::{
    SeedCellClassifier, Stage0KeywordSeedClassifier, Stage1KanStubSeedClassifier,
};

/// Bundle of the three classifier trait-objects.
///
/// Constructed once at host startup ; passed by reference into the
/// intent_router / cocreative-optimizer / spontaneous-condensation
/// call-sites (wave-7 wiring).
pub struct ClassifierRegistry {
    pub intent: Box<dyn IntentClassifier>,
    pub cocreative: Box<dyn CocreativeClassifier>,
    pub seed: Box<dyn SeedCellClassifier>,
}

impl ClassifierRegistry {
    /// Replace the intent classifier in-place.
    pub fn replace_intent(&mut self, c: Box<dyn IntentClassifier>) {
        self.intent = c;
    }

    /// Replace the cocreative classifier in-place.
    pub fn replace_cocreative(&mut self, c: Box<dyn CocreativeClassifier>) {
        self.cocreative = c;
    }

    /// Replace the seed classifier in-place.
    pub fn replace_seed(&mut self, c: Box<dyn SeedCellClassifier>) {
        self.seed = c;
    }
}

/// Construct a registry where every role is the stage-0 reference impl.
#[must_use]
pub fn default_stage0() -> ClassifierRegistry {
    ClassifierRegistry {
        intent: Box::new(Stage0HeuristicClassifier::default_rules()),
        cocreative: Box::new(Stage0DotProductClassifier::default_dim4()),
        seed: Box::new(Stage0KeywordSeedClassifier::default_table()),
    }
}

/// Construct a registry where every role is a stage-1 stub backed by
/// the corresponding stage-0 reference impl as fallback.
///
/// No `kan_handle` is set on any stub : behavior is identical to
/// `default_stage0` modulo trait-object identity. This is the
/// shape-validation registry — wave-7 will replace these stubs with
/// real KAN-substrate handles.
#[must_use]
pub fn default_stage1_with_stubs() -> ClassifierRegistry {
    ClassifierRegistry {
        intent: Box::new(Stage1IntentKanStubClassifier::new(Box::new(
            Stage0HeuristicClassifier::default_rules(),
        ))),
        cocreative: Box::new(Stage1CocreativeKanStubClassifier::new(Box::new(
            Stage0DotProductClassifier::default_dim4(),
        ))),
        seed: Box::new(Stage1KanStubSeedClassifier::new(Box::new(
            Stage0KeywordSeedClassifier::default_table(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stage0_classifies() {
        let reg = default_stage0();
        let intent = reg.intent.classify("move forward");
        assert_eq!(intent.kind, "move");
        let score = reg.cocreative.score_features(&[1.0, 1.0, 1.0, 1.0]);
        assert!((score - 2.0).abs() < 1e-6);
        let cells = reg.seed.intent_to_seed_cells("move", &[]);
        assert_eq!(cells.len(), 1);
    }

    #[test]
    fn default_stage1_stub_falls_through() {
        let reg = default_stage1_with_stubs();
        // No kan_handle is set, so all three stubs delegate to stage-0.
        let intent = reg.intent.classify("examine altar");
        assert_eq!(intent.kind, "examine");
        let score = reg.cocreative.score_features(&[1.0, 1.0, 1.0, 1.0]);
        assert!((score - 2.0).abs() < 1e-6);
        let cells = reg.seed.intent_to_seed_cells("cocreate", &[]);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, 4);
    }

    #[test]
    fn replace_takes_effect() {
        let mut reg = default_stage0();
        // Swap the intent classifier for a stage-1 stub with a mock
        // KAN handle ; verify the registry reports the swap.
        let new_intent = Stage1IntentKanStubClassifier::with_handle(
            Box::new(Stage0HeuristicClassifier::default_rules()),
            String::from("kan-test"),
        );
        reg.replace_intent(Box::new(new_intent));
        assert_eq!(reg.intent.name(), "stage1-kan-stub");
        let r = reg.intent.classify("anything");
        assert_eq!(r.kind, "kan_mocked");
    }

    #[test]
    fn all_3_classifiers_named() {
        let reg = default_stage0();
        assert_eq!(reg.intent.name(), "stage0-heuristic");
        assert_eq!(reg.cocreative.name(), "stage0-dot-product");
        assert_eq!(reg.seed.name(), "stage0-keyword-seed");
        let reg1 = default_stage1_with_stubs();
        assert_eq!(reg1.intent.name(), "stage1-kan-stub");
        assert_eq!(reg1.cocreative.name(), "stage1-kan-stub");
        assert_eq!(reg1.seed.name(), "stage1-kan-stub-seed");
    }
}
