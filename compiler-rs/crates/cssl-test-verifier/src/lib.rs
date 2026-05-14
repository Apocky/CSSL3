// § T11-W19-β-VERIFIER : event-trace verifier library surface
// ══════════════════════════════════════════════════════════════════
// § I> stage0 = Rust-hosted ← per parent-Cargo.toml § stage0
// § I> post-stage0 → migrate-to-CSSL-source ← per §§ 01_BOOTSTRAP
// § I> manifest-format = line-oriented stage0-pragmatic ← see manifest.rs
// § I> verifier-comparator-axes :
//     ✓ required-ops .with count-tolerance (exact | range | ge | le)
//     ✓ ordering A→B (before / only-after)
//     ✓ result-value predicates
//     ✓ latency bounds (per-op max-ns)
//     ✓ missing-required detection
//     ✓ silent-fallback alarm (skip-events ∉ allowlist)
//     ✓ unexpected-event report
// ══════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]

pub mod events;
pub mod manifest;
pub mod manifest_csl;
pub mod output;
pub mod verify;

pub use events::{Event, EventKind};
pub use manifest::{
    AllowedSkip, CountSpec, LatencyBound, Manifest, OrderConstraint, ResultPredicate, Required,
};
pub use verify::{
    verify, Failure, SilentFallback, UnexpectedEvent, VerificationReport,
};
