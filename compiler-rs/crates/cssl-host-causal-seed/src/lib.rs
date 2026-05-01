// § T11-W4-CAUSAL : cssl-host-causal-seed — root module
// ════════════════════════════════════════════════════════════════════
// § I> story-as-physics : intents impose causal-force ; world-state bends along DAG
// § I> ⟨specs/grand-vision/01_PARADIGMS.csl⟩ paradigm-5 ⊆ CAUSAL
// § I> exports : node ⊔ edge ⊔ dag ⊔ integrate
// § I> determinism : same-input → bit-identical-output (HashMap-iter sorted on serialize)
// § I> safety : forbid(unsafe_code) · no panics in lib · all-failures via Result
// ════════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]
#![doc = include_str!("../Cargo.toml")]

pub mod node;
pub mod edge;
pub mod dag;
pub mod integrate;

pub use node::{CausalNode, NodeKind};
pub use edge::{CausalEdge, EdgeKind, EdgeErr};
pub use dag::{CausalDag, DagErr};
pub use integrate::{CausalEffect, CausalIntegrator, LinearEffect, WorldVector};

/// Crate-level metadata banner ← attests § PRIME-DIRECTIVE structurally.
///
/// § I> consent=OS · violation=bug · no-override-exists
/// § I> reads-only intents from caller ; writes-only world-vector ; no surveillance
pub const PRIME_DIRECTIVE_BANNER: &str =
    "consent=OS • violation=bug • no-override-exists";

/// Crate version (matches Cargo.toml) — surfaced for serialized seed-kit headers.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn prime_directive_banner_nonempty() {
        assert!(!PRIME_DIRECTIVE_BANNER.is_empty());
        assert!(PRIME_DIRECTIVE_BANNER.contains("consent=OS"));
    }

    #[test]
    fn version_matches_cargo() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }
}
