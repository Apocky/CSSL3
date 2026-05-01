//! CSSLv3 stage0 — Trait-bridge between LoA classifiers and pluggable backends.
//!
//! § T11-W6-KAN-BRIDGE (cssl/session-15/W-W6-kan-bridge)
//!
//! § PURPOSE
//!   LoA's intelligence-role pipeline (DM / GM / Collaborator / Coder per
//!   `specs/grand-vision/10_INTELLIGENCE.csl`) classifies intents, scores
//!   cocreative-bias feature-vectors, and emits spontaneous-condensation
//!   seed-cells. Today these run on stage-0 keyword + dot-product
//!   heuristics. Stage-1+ swaps the implementations to KAN-substrate
//!   classifiers WITHOUT touching call-sites — this crate is the BRIDGE.
//!
//!   Three trait abstractions (one per role) + a registry that owns
//!   `Box<dyn Trait>` instances. The host constructs the registry once
//!   (stage-0 OR stage-1-stub) and passes it through; downstream code
//!   never sees the concrete impl.
//!
//! § MODULE LAYOUT
//!   - [`intent_classifier`]     — `IntentClassifier` trait + `Stage0HeuristicClassifier` + `Stage1KanStubClassifier`
//!   - [`cocreative_classifier`] — `CocreativeClassifier` trait + `Stage0DotProductClassifier` + `Stage1KanStubClassifier`
//!   - [`seed_classifier`]       — `SeedCellClassifier` trait + `Stage0KeywordSeedClassifier` + `Stage1KanStubSeedClassifier`
//!   - [`registry`]              — `ClassifierRegistry` + `default_stage0` / `default_stage1_with_stubs`
//!
//! § GUARANTEES
//!   - `#![forbid(unsafe_code)]` ; no `unsafe` blocks.
//!   - No panics in library code : every fallible path is total.
//!   - All traits are object-safe (verified by `Box<dyn Trait>` storage in registry).
//!   - Stage-1 stubs carry an opaque handle string + fall through to a
//!     stage-0 fallback when the handle is `None` ; they do NOT invoke
//!     any actual KAN backend (wave-7 wires the real KAN delegate).
//!   - `serde` round-trip stable for `IntentClass` + `SeedCell`.

#![forbid(unsafe_code)]

pub mod cocreative_classifier;
pub mod intent_classifier;
pub mod registry;
pub mod seed_classifier;

pub use cocreative_classifier::{
    CocreativeClassifier, Stage0DotProductClassifier,
    Stage1KanStubClassifier as Stage1CocreativeKanStubClassifier,
};
pub use intent_classifier::{
    IntentClass, IntentClassifier, KeywordRule, Stage0HeuristicClassifier,
    Stage1KanStubClassifier as Stage1IntentKanStubClassifier,
};
pub use registry::{ClassifierRegistry, default_stage0, default_stage1_with_stubs};
pub use seed_classifier::{
    SeedCell, SeedCellClassifier, Stage0KeywordSeedClassifier,
    Stage1KanStubSeedClassifier,
};
