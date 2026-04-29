//! Integration tests for the M8 12-stage pipeline + per-stage instrumentation.
//!
//! § COVERAGE
//!   - per-stage instrumentation : every stage-id has a registered histogram
//!   - percentile reads          : p50 / p95 / p99 readable + monotone
//!   - zero-overhead             : feature-gate `metrics` off → no-op shims
//!   - replay-determinism        : per-stage outputs match across runs
//!   - frame-counter monotonic   : frame_n strictly increasing
//!   - registry namespace-set    : all 12 namespaces present, lex-sorted

pub mod determinism_tests;
pub mod per_stage_tests;
pub mod percentile_tests;
pub mod registry_tests;
pub mod zero_overhead_tests;
