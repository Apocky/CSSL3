//! # cssl-host-histograms
//!
//! Bounded-memory streaming histograms for telemetry (frame-time, GPU-pass timings,
//! counter distributions). Designed for the iterate-everywhere directive : record once
//! per frame at O(1) cost into a fixed `[u64; 64]` bucket-array, then query P50/P95/P99/P999
//! at any time without retaining the underlying samples.
//!
//! ## Design notes
//!
//! - **HDR-style power-of-2 bucketing** : 64 buckets covering 1 µs to ~3 hours.
//!   Each bucket spans a doubling range of values, so relative percentile error
//!   is bounded by ~50 % regardless of input scale. For frame-time + GPU-pass use,
//!   inputs cluster between 100 µs and 100 ms ; resolution there is ~ms-grain.
//! - **Welford-style stats** : `count`, `sum`, `sum_sq` updated per-record yields
//!   exact mean + numerically-stable variance over u64-µs inputs without a sample
//!   buffer.
//! - **Bounded memory** : per-histogram footprint is `~600 bytes` regardless of
//!   sample count. No allocations on `record` after construction.
//! - **Mergeable** : two histograms over disjoint sample-sets compose by adding
//!   buckets + counts + sums + min/max. Enables map-reduce style aggregation
//!   across worker threads.
//! - **Serializable** : `serde::{Serialize, Deserialize}` derives on `Histogram`
//!   so snapshots flow through telemetry pipelines (JSONL reports, MCP tools,
//!   replay-bundle writers).
//!
//! ## API surface
//!
//! - [`Histogram`] : single-stream recorder (one named distribution).
//! - [`HistogramRegistry`] : map of `name → Histogram` ; `record(name, value)`
//!   does get-or-create.
//! - [`ScopedTimer`] : RAII timer ; records elapsed-µs to a registry-named
//!   histogram on drop. Wrap a code-block to time it without manual stop calls.
//! - [`buckets`] : free functions for the bucket-index ↔ value-range mapping.
//!
//! ## Non-goals
//!
//! - Not a Prometheus client. Reports are plain text + JSONL, suitable for
//!   in-game telemetry overlays + replay-bundle inclusion.
//! - Not a tracing layer. Use `tracing` crate for structured event logs ; this
//!   crate is for distribution-shape telemetry only.
//! - Not concurrent-safe. The registry is `&mut self` for record ; callers
//!   serialize access (one registry per worker, then merge ; or wrap in a
//!   `Mutex`).
//!
//! ## Per § T11-WAVE3-IT-HISTOGRAMS
//!
//! Crate is FILE-DISJOINT from in-flight loa-host merge ; safe to land in
//! parallel with wave-3 fanout.

#![forbid(unsafe_code)]
// § cast-precision is intrinsic to the f64-mean/stddev/percentile-fraction
// design : sums are u128 + counts u64 in the Welford accumulators, but
// downstream stats project onto f64 because that's the consumer-facing
// numeric type for telemetry overlays + JSON. The 52-bit mantissa is
// adequate for typical sample-counts (< 2^53) and µs values (telemetry
// inputs cluster in [10^2, 10^8] which fits cleanly).
#![allow(clippy::cast_precision_loss)]
// § sub-optimal-flops is a clippy nursery-pedantic lint that prefers
// f64::mul_add over `a + b * c`. Our stats are not perf-critical (one
// call per query, not per record) and the mul_add form sacrifices code
// readability for a sub-percent FP-accuracy gain that doesn't matter for
// telemetry summaries.
#![allow(clippy::suboptimal_flops)]

pub mod buckets;
pub mod histogram;
pub mod registry;
pub mod timer;

pub use buckets::{bucket_index, bucket_lower_bound, bucket_upper_bound, BUCKETS};
pub use histogram::Histogram;
pub use registry::HistogramRegistry;
pub use timer::{scoped, ScopedTimer};
