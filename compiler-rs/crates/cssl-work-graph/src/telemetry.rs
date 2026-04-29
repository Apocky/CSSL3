//! Dispatch telemetry.
//!
//! § DESIGN
//!   `DispatchTelemetry` records per-frame measurements of the work-graph :
//!     - actual wall-clock cost (us) per node
//!     - dispatch-group counts
//!     - mesh-output bandwidth
//!
//!   Aggregated per-frame, smoothed over a rolling window for the 30-frame
//!   hysteresis required by `density_budget § VI` mode-auto-switch.
//!
//! § PRIME-DIRECTIVE
//!   Telemetry is process-local and never leaves the device unless the user
//!   explicitly enables remote ingestion (cite `06_RENDERING_PIPELINE.csl`
//!   § XII : eye-data + body-data Σ-elevated, never transmits without
//!   consent). This module is data-only ; transmission is the caller's
//!   responsibility.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use std::collections::HashMap;

use crate::node::NodeId;

/// Per-node measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeMeasurement {
    /// Actual GPU-time in microseconds.
    pub gpu_us: u32,
    /// Dispatch groups actually issued.
    pub groups: u64,
    /// Mesh-output vertices emitted (0 for compute-only nodes).
    pub mesh_vertices: u64,
}

impl NodeMeasurement {
    /// Construct.
    #[must_use]
    pub const fn new(gpu_us: u32, groups: u64, mesh_vertices: u64) -> Self {
        Self {
            gpu_us,
            groups,
            mesh_vertices,
        }
    }

    /// Empty.
    #[must_use]
    pub const fn empty() -> Self {
        Self::new(0, 0, 0)
    }
}

/// Per-frame dispatch telemetry.
#[derive(Debug, Clone, Default)]
pub struct DispatchTelemetry {
    /// Measurements per node.
    nodes: HashMap<NodeId, NodeMeasurement>,
    /// Total wall-clock cost (us).
    pub total_us: u32,
    /// Frame number this telemetry corresponds to.
    pub frame_index: u64,
}

impl DispatchTelemetry {
    /// Construct.
    #[must_use]
    pub fn new(frame_index: u64) -> Self {
        Self {
            nodes: HashMap::new(),
            total_us: 0,
            frame_index,
        }
    }

    /// Record one node measurement.
    pub fn record(&mut self, id: NodeId, m: NodeMeasurement) {
        self.total_us = self.total_us.saturating_add(m.gpu_us);
        self.nodes.insert(id, m);
    }

    /// Number of nodes measured.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Look-up one measurement.
    #[must_use]
    pub fn get(&self, id: &NodeId) -> Option<&NodeMeasurement> {
        self.nodes.get(id)
    }

    /// Mean per-node cost (us).
    #[must_use]
    pub fn mean_us(&self) -> u32 {
        if self.nodes.is_empty() {
            return 0;
        }
        let sum: u64 = self.nodes.values().map(|m| u64::from(m.gpu_us)).sum();
        (sum / self.nodes.len() as u64) as u32
    }

    /// Aggregate dispatch groups across all nodes.
    #[must_use]
    pub fn total_groups(&self) -> u64 {
        self.nodes.values().map(|m| m.groups).sum()
    }

    /// Aggregate mesh-output vertices across all nodes.
    #[must_use]
    pub fn total_mesh_vertices(&self) -> u64 {
        self.nodes.values().map(|m| m.mesh_vertices).sum()
    }
}

/// Rolling-window aggregator for the 30-frame hysteresis required by
/// `density_budget § VI`.
#[derive(Debug, Clone)]
pub struct RollingTelemetry {
    window: Vec<DispatchTelemetry>,
    capacity: usize,
}

impl RollingTelemetry {
    /// 30-frame default per `density_budget § VI`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(30)
    }

    /// Custom capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            window: Vec::with_capacity(capacity),
            capacity: capacity.max(1),
        }
    }

    /// Push a frame.
    pub fn push(&mut self, frame: DispatchTelemetry) {
        if self.window.len() == self.capacity {
            self.window.remove(0);
        }
        self.window.push(frame);
    }

    /// Length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Mean total-us across the window.
    #[must_use]
    pub fn rolling_mean_us(&self) -> u32 {
        if self.window.is_empty() {
            return 0;
        }
        let sum: u64 = self.window.iter().map(|f| u64::from(f.total_us)).sum();
        (sum / self.window.len() as u64) as u32
    }

    /// p99 total-us across the window (approximate ; sorts a copy).
    #[must_use]
    pub fn rolling_p99_us(&self) -> u32 {
        if self.window.is_empty() {
            return 0;
        }
        let mut samples: Vec<u32> = self.window.iter().map(|f| f.total_us).collect();
        samples.sort_unstable();
        let idx = ((samples.len() as f32) * 0.99).ceil() as usize - 1;
        samples[idx.min(samples.len() - 1)]
    }
}

impl Default for RollingTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{DispatchTelemetry, NodeMeasurement, RollingTelemetry};
    use crate::node::NodeId;

    #[test]
    fn record_accumulates_total_us() {
        let mut t = DispatchTelemetry::new(0);
        t.record(NodeId::new("A"), NodeMeasurement::new(1_000, 4, 0));
        t.record(NodeId::new("B"), NodeMeasurement::new(2_500, 16, 0));
        assert_eq!(t.total_us, 3_500);
        assert_eq!(t.node_count(), 2);
    }

    #[test]
    fn mean_zero_when_empty() {
        let t = DispatchTelemetry::new(0);
        assert_eq!(t.mean_us(), 0);
    }

    #[test]
    fn mean_us_correct() {
        let mut t = DispatchTelemetry::new(0);
        t.record(NodeId::new("A"), NodeMeasurement::new(1_000, 0, 0));
        t.record(NodeId::new("B"), NodeMeasurement::new(3_000, 0, 0));
        assert_eq!(t.mean_us(), 2_000);
    }

    #[test]
    fn total_groups_sum() {
        let mut t = DispatchTelemetry::new(0);
        t.record(NodeId::new("A"), NodeMeasurement::new(0, 100, 0));
        t.record(NodeId::new("B"), NodeMeasurement::new(0, 200, 0));
        assert_eq!(t.total_groups(), 300);
    }

    #[test]
    fn total_mesh_vertices_sum() {
        let mut t = DispatchTelemetry::new(0);
        t.record(NodeId::new("M"), NodeMeasurement::new(0, 0, 1_000));
        t.record(NodeId::new("M2"), NodeMeasurement::new(0, 0, 500));
        assert_eq!(t.total_mesh_vertices(), 1_500);
    }

    #[test]
    fn rolling_default_30_frames() {
        let r: RollingTelemetry = RollingTelemetry::new();
        assert_eq!(r.capacity(), 30);
    }

    #[test]
    fn rolling_evicts_oldest() {
        let mut r = RollingTelemetry::with_capacity(3);
        for i in 0..5 {
            let mut f = DispatchTelemetry::new(i);
            f.total_us = (i as u32) * 100;
            r.push(f);
        }
        assert_eq!(r.len(), 3);
        // Window is [200, 300, 400] ⇒ mean = 300.
        assert_eq!(r.rolling_mean_us(), 300);
    }

    #[test]
    fn rolling_p99_is_high_sample() {
        let mut r = RollingTelemetry::with_capacity(10);
        for i in 0..10 {
            let mut f = DispatchTelemetry::new(i);
            f.total_us = (i as u32) * 100;
            r.push(f);
        }
        // Samples : 0, 100, 200, ... 900. p99 = ceil(0.99 × 10) - 1 = 9 → 900.
        assert_eq!(r.rolling_p99_us(), 900);
    }
}
