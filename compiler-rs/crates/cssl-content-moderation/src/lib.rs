//! § cssl-content-moderation — community-flag · cap-required · k-anon
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W12-MODERATION (POD-3-W12-11) : community-driven content-moderation
//! that respects PRIME-DIRECTIVE absolutely. ¬ shadowban EVER · ¬ algorithmic-
//! suppression · transparency-at-every-stage · sovereign-revocable.
//!
//! § ARCHITECTURE
//!   ┌────────────────┐    submit-flag (cap-flagger)
//!   │ flagger        │ ──────────────────────────►  ┌────────────────┐
//!   │ (community)    │                              │ FlagRecord     │
//!   └────────────────┘                              │  (32-byte pack)│
//!                                                   └────────┬───────┘
//!                                                            │ insert
//!                                                            ▼
//!   ┌────────────────┐    aggregate (k-anon)         ┌────────────────┐
//!   │ ModerationStore│ ◄───────────────────────────  │ Aggregate      │
//!   │  (per-content) │                               │  (T2 floor)    │
//!   └────────┬───────┘                               └────────────────┘
//!            │                                                ▲
//!            │ T3 needs-review · ≥10-distinct + weight ≥75    │
//!            ▼                                                │
//!   ┌────────────────┐    cap-curate                          │
//!   │ Curator(A or B)│ ─────────────►  ┌────────────────┐    │
//!   │  (≥3 quorum)   │                 │ Decision       │    │
//!   └────────────────┘                 │  Σ-Chain-anchor│    │
//!                                      └────────┬───────┘    │
//!                                               │            │
//!                                               │ public     │ author-visible
//!                                               ▼            │
//!                                      ┌────────────────────┐│
//!                                      │ /transparency/:slug├┘
//!                                      └────────────────────┘
//!
//! § SOVEREIGNTY GUARANTEES
//!   1. ¬ shadowban : ALL flag-counts visible-to-author @ T2 (≥3 flags)
//!   2. ¬ algorithmic-suppression : curator-decision is the ONLY path
//!   3. author-appeal ALWAYS-available · 30-day-window
//!   4. auto-restore @ 7-days-no-decision (T5)
//!   5. sovereign-revoke wins UNCONDITIONALLY (even mid-review)
//!   6. Σ-Chain-anchor on every curator-decision (immutable trail)
//!   7. flagger can revoke own-flag any-stage
//!   8. determinism : replay-stable record-pack/unpack
//!
//! § PRIME-DIRECTIVE
//!   `#![forbid(unsafe_code)]`. ¬ surveillance. ¬ coercion. ¬ secret-state.
//!
//! § PARENT spec : `Labyrinth of Apocalypse/systems/content_moderation.csl`

#![forbid(unsafe_code)]
#![doc(html_no_source)]

pub mod aggregate;
pub mod appeal;
pub mod cap;
pub mod decision;
pub mod ffi;
pub mod record;
pub mod store;

pub use aggregate::{ModerationAggregate, K_AUTHOR_AGGREGATE_FLOOR, K_NEEDS_REVIEW_DISTINCT, K_NEEDS_REVIEW_WEIGHTED};
pub use appeal::{Appeal, AppealError, T_AUTO_RESTORE_DAYS, T_APPEAL_WINDOW_DAYS, K_APPEAL_CURATOR_QUORUM};
pub use cap::{CapClass, CapPolicy, MOD_CAP_FLAG_SUBMIT, MOD_CAP_APPEAL, MOD_CAP_CURATE_A, MOD_CAP_CURATE_B, MOD_CAP_CHAIN_ANCHOR, MOD_CAP_AGGREGATE_READ};
pub use decision::{CuratorDecision, DecisionKind, DecisionError};
pub use record::{FlagRecord, FlagKind, RecordError};
pub use store::{ModerationStore, StoreError};

/// § PRIME_DIRECTIVE attestation — compiled-in · queryable.
/// Returns the canonical attestation-string verifying ¬ shadowban + ¬ algo-
/// suppression + sovereign-revoke + Σ-Chain-anchor + transparency.
pub fn prime_directive_attestation() -> &'static str {
    "cssl-content-moderation : NO-shadowban + NO-algo-suppression + \
     sovereign-revoke-wins + Sigma-Chain-anchor + author-transparent + \
     flagger-revocable + 30d-appeal-window + 7d-auto-restore"
}
