//! § cssl-substrate-audio — STUB (skeleton-only ; full impl per T11-D330).
//!
//! § This file is a temporary stub so the workspace builds while
//! § cssl-substrate-audio awaits its full landing per T11-D330 (W-S-AUD-1).
//! § The real HRTF + procedural-synth-via-KAN + reverb-FDN + 3D-spatializer
//! § land when their source-of-truth slice merges.
//!
//! § This stub was emitted by the parallel-fanout rescue in T11-D301
//! § (W-S-CORE-2 cssl-substrate-light) so the workspace cargo build
//! § resolves while D330 lands its surface in parallel.

#![allow(dead_code)]

/// § Crate version sentinel — bumped on full T11-D330 landing.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// § Slice-id placeholder for the in-flight T11-D330 landing.
pub const SLICE_ID: &str = "T11-D330 (PENDING)";
