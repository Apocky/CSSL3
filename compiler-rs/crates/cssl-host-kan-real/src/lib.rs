//! § cssl-host-kan-real — REAL stage-1 KAN classifier crate.
//! ════════════════════════════════════════════════════════════════════
//!
//! § T11-W7-A-KAN-REAL (cssl/session-6/parallel-fanout)
//!
//! § ROLE
//!   Wave-6 landed `cssl-host-kan-substrate-bridge` with TRAIT abstractions
//!   plus stage-0 reference impls and stage-1 STUB impls (opaque
//!   `kan_handle = String` carrier ; canned mocked output). This crate
//!   replaces the stage-1 STUBs with REAL substrate-driven classifiers :
//!
//!     - [`intent_real::RealIntentKanClassifier`]     — implements
//!       `IntentClassifier` against a baked KAN spline-table (intent-head
//!       I=32 → O=8) ; pipeline = utterance → tokenize → encode-as-feature-
//!       vec (≥32-D RFF-style) → KAN spline-eval → softmax → top-1
//!       IntentClass + clamped confidence.
//!
//!     - [`cocreative_real::RealCocreativeKanClassifier`] — implements
//!       `CocreativeClassifier` against a baked KAN scorer (I=feature-dim
//!       → O=1) ; pipeline = history-buffer + bias-axis → KAN scorer →
//!       sigmoid-clamped f32 ∈ [0,1] preference-weight.
//!
//!     - [`seed_real::RealSeedCellKanClassifier`] — implements
//!       `SeedCellClassifier` against a baked KAN seeder (I=zone-summary
//!       → O=N_CELLS×CELL_DIM) ; pipeline = ω-field-summary + zone-id +
//!       cap-table → KAN seeder → bounded `Vec<SeedCell>` (N≤16).
//!
//!     - [`feature_encode`] — utterance → feature-vec encoder
//!       (deterministic byte-hash + RFF-style sin/cos projection · seeded).
//!
//!     - [`canary::CanaryGate`] — 10% session-id-hash gate per spec
//!       `§ A/B-PROTOCOL` ; emits structured `DisagreementKind` for the
//!       rollback trigger T-2.
//!
//! § INVARIANTS PER `specs/grand-vision/11_KAN_RIDE.csl § INVARIANTS`
//!   - I-1 determinism : seed+input → output bit-equal across runs.
//!   - I-2 confidence ∈ [0.0, 1.0] · NaN banned · clamp-on-violate-then-audit.
//!   - I-3 audit-emit : every classify-call hits [`audit::audit_log`]
//!     (default no-op ; host wires `cssl-host-attestation` via
//!     [`audit::set_audit_sink`] at registry-construction time).
//!   - I-4 fallback-on-missing-handle : if the baked KAN spline-table is
//!     `None` ⇒ delegate to stage-0 fallback ; never panic.
//!   - I-5 latency-bound : impl is bounded by O(I·O·KAN_LAYERS) per call —
//!     well inside the 2× stage-0 budget the spec mandates.
//!   - I-6 never-refuse : every classify path returns SOMETHING — the
//!     unknown / fallback / clamped paths are explicitly enumerated.
//!
//! § GAP : cssl-substrate-kan::KanNetwork::eval is presently a shape-
//!   preserving placeholder returning `[0.0; O]` (see
//!   `cssl-substrate-kan::kan_network::KanNetwork::eval` rustdoc). This
//!   crate therefore wraps the substrate type with [`adapter::KanRuntime`]
//!   which carries the substrate `KanNetwork` AND a local control-point-
//!   driven cubic-Hermite eval that is byte-stable + deterministic. When
//!   the substrate `eval` lands a real spline evaluator, swap the body of
//!   `KanRuntime::eval` for a substrate-call ; no API change required.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::float_cmp)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::case_sensitive_file_extension_comparisons)]

pub mod adapter;
pub mod audit;
pub mod canary;
pub mod cocreative_real;
pub mod feature_encode;
pub mod intent_real;
pub mod seed_real;

// § Re-export the trait + stage-0 fallback surface from the bridge crate
//   so call-sites can pin a single dep on `cssl-host-kan-real` and get the
//   full registry-shape.
pub use cssl_host_kan_substrate_bridge::{
    cocreative_classifier::{CocreativeClassifier, Stage0DotProductClassifier},
    intent_classifier::{IntentClass, IntentClassifier, Stage0HeuristicClassifier},
    seed_classifier::{SeedCell, SeedCellClassifier, Stage0KeywordSeedClassifier},
    ClassifierRegistry,
};

pub use adapter::{KanRuntime, KanRuntimeError};
pub use audit::{audit_log, AuditEvent, AuditSink};
pub use canary::{CanaryGate, DisagreementKind};
pub use cocreative_real::RealCocreativeKanClassifier;
pub use feature_encode::{encode_features, FeatureEncodeConfig, FEATURE_DIM};
pub use intent_real::{IntentLabel, RealIntentKanClassifier, INTENT_LABEL_COUNT};
pub use seed_real::{RealSeedCellKanClassifier, MAX_SEED_CELLS};

/// § Crate version sentinel — bumped when the public surface contract
///   changes in a way that invalidates downstream registry-construction.
pub const KAN_REAL_SURFACE_VERSION: u32 = 1;
