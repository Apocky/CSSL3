// § T11-WAVE3-IT-GOLDEN : cssl-host-golden crate root
// ══════════════════════════════════════════════════════════════════
// § I> golden-image visual-regression for LoA frame snapshots
// § I> 4 sub-modules : snapshot · diff · golden · report
// § I> stdlib-only · ¬ unsafe · ¬ panics
//
// § Pipeline
//   1. caller → from_rgba(label, raw, w, h) → Snapshot
//   2. GoldenStore::compare(label, snap, tol) → GoldenCompare
//   3. CampaignReport aggregates ∀ comparisons → render_text
//
// § Spec-traceability
//   – T11-WAVE3-IT-GOLDEN
//   – /specs/22_TELEMETRY (regression-detection lineage)
//   – /specs/23_TESTING   (oracle-mode + differential-backend testing)

#![forbid(unsafe_code)]
// § precision-loss : f64 → f32 narrowing in diff.rs is intentional ; report
// fields are publicly typed `f32` per spec ; pixel counters are bounded by
// width*height which never exceeds 2^32 in any LoA snapshot (max 65k×65k).
#![allow(clippy::cast_precision_loss)]

pub mod diff;
pub mod golden;
pub mod report;
pub mod snapshot;

pub use diff::{diff_rgb, diff_rgba, DiffErr, DiffReport};
pub use golden::{GoldenCompare, GoldenStore, GoldenVerdict};
pub use report::{render_text, CampaignReport, RunResult};
pub use snapshot::{from_rgba, SnapErr, Snapshot};
