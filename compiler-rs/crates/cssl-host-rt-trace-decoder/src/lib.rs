//! # cssl-host-rt-trace-decoder — analysis layer over `cssl-host-rt-trace`
//!
//! § T11-W6-RT-TRACE-DECODER : the W5a ring emits compact 32-byte
//! [`RtEvent`](cssl_host_rt_trace::RtEvent) records into a bounded ring.
//! This crate consumes drained snapshots + the matching
//! [`LabelInterner`](cssl_host_rt_trace::LabelInterner) and produces
//! structured analysis :
//!
//! - [`pair`]            — match `MarkBegin`/`MarkEnd` into [`MarkPair`] structs
//!   with depth + duration tracking.
//! - [`flame`]           — build a hierarchical [`FlameNode`] tree + render
//!   to the flat-collapsed format consumed by Brendan Gregg's `flamegraph.pl`.
//! - [`chrome_tracing`]  — emit a chrome://tracing JSON document
//!   (`X` complete-events) suitable for the browser-native viewer at
//!   `chrome://tracing` or Perfetto.
//! - [`summary`]         — compute per-label aggregate statistics
//!   (count · total-µs · mean · p50 · p99) and a one-page text report.
//!
//! ## Design contract
//!
//! - **Pure analysis.** No side-effects · no I/O · no panics in library paths.
//! - **Bounded memory.** Output sizes scale linearly with input ; no
//!   internal unbounded buffering.
//! - **Stable across drains.** The same input slice always produces the same
//!   output (deterministic ordering via `BTreeMap` for label tables).
//! - **`#![forbid(unsafe_code)]`.** Stage-0 correctness > stage-1 perf.
//!
//! ## Usage sketch
//!
//! ```ignore
//! use cssl_host_rt_trace::{LabelInterner, RtRing, scoped_mark};
//! use cssl_host_rt_trace_decoder::{pair_marks, build_flame_graph, summarize};
//!
//! let ring = RtRing::new(4096);
//! let mut interner = LabelInterner::default();
//! let frame_idx = interner.intern("frame");
//!
//! // ... emit events into the ring via scoped_mark!(...) ...
//!
//! let events = ring.drain();
//! let pairs  = pair_marks(&events, &interner);
//! let flame  = build_flame_graph(&pairs);
//! let stats  = summarize(&events, &interner, &pairs);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod chrome_tracing;
pub mod flame;
pub mod pair;
pub mod summary;

pub use chrome_tracing::{
    marks_to_chrome_tracing, render_json, ChromeTracingDoc, ChromeTracingEvent,
};
pub use flame::{build_flame_graph, render_flat_collapsed, FlameNode};
pub use pair::{pair_marks, unmatched_begins, unmatched_ends, MarkPair};
pub use summary::{render_text, summarize, LabelStats, TraceSummary};
