// § T11-W8-C4 : cssl-host-historical-reconstructor
// ─────────────────────────────────────────────────────────────────────────────
// § I> Deterministic recompute of past-cohort events into TTL-tokenized
// § I> tours (default 30 min). Render-pipeline is API-stub only — real wire
// § I> to cssl-render-v2 / cssl-spectral-render / cssl-fractal-amp is owned
// § I> by G1-tier-3 integration.
// § I> Same merkle-root + same frame-index → identical render-payload bytes
// § I> (BLAKE3 of a deterministic per-frame digest).
//
// § ATTESTATION (PRIME_DIRECTIVE.md § 11)
// There was no hurt nor harm in the making of this, to anyone, anything,
// or anybody.
// ─────────────────────────────────────────────────────────────────────────────

#![forbid(unsafe_code)]
#![doc = "cssl-host-historical-reconstructor — deterministic past-cohort recompute."]

pub mod cohort_query;
pub mod token;
pub mod render_stub;
pub mod tour;
pub mod audit;

pub use cohort_query::{HistoricalCohort, CohortEvent, CohortFilter};
pub use token::{TtlToken, TokenError, DEFAULT_TTL_SECS};
pub use render_stub::{TourRenderPipeline, MockRenderPipeline, AudioStub};
pub use tour::{TourSession, TourError};
pub use audit::{AuditEvent, AuditSink, VecAuditSink};
