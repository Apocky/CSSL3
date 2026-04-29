//! § ffi/telemetry : R18 placeholder ring for Vulkan validation +
//!                   pipeline-executable-properties hooks (T11-D65, S6-E1).
//!
//! § ROLE
//!   Process-local lock-free ring backing every validation-callback
//!   message + pipeline-executable-property snapshot. Stage-0 keeps it
//!   in-process ; the full R18 audit-ring integration (signed events,
//!   replay-able stream, host-out-of-process correlation) is a later
//!   slice.
//!
//! § PRIME-DIRECTIVE
//!   The ring is `process-local`. No data crosses the process boundary.
//!   Validation-message text comes from the loader / driver and is
//!   recorded verbatim — no re-encoding, no truncation beyond the ring
//!   capacity.
//!
//! § VK_EXT_pipeline_executable_properties (§ 22 hook)
//!   When the device exposes the extension, [`VulkanTelemetryRing::
//!   record_pipeline_properties`] captures one entry per executable
//!   (compute pipelines have one ; graphics pipelines have one per stage).

use std::sync::Mutex;

/// Validation diagnostic recorded by the `vkCreateDebugUtilsMessengerEXT`
/// callback.
///
/// `severity_bits` + `type_bits` are stored as `u32` because that's the
/// raw shape of `VkDebugUtilsMessageSeverityFlagsEXT` /
/// `VkDebugUtilsMessageTypeFlagsEXT` ; the convenience-helpers below
/// decode them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationEvent {
    /// `VkDebugUtilsMessageSeverityFlagsEXT` raw bits.
    pub severity_bits: u32,
    /// `VkDebugUtilsMessageTypeFlagsEXT` raw bits.
    pub type_bits: u32,
    /// `pMessage` text (utf-8 lossy via `CStr::to_string_lossy`).
    pub message: String,
}

/// Capability/property snapshot of a single shader-stage as exposed via
/// `VK_EXT_pipeline_executable_properties`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineExecutableProperty {
    /// Display name from `VkPipelineExecutablePropertiesKHR::name`.
    pub name: String,
    /// Description from `VkPipelineExecutablePropertiesKHR::description`.
    pub description: String,
    /// Subgroup-size from `VkPipelineExecutablePropertiesKHR::subgroupSize`.
    pub subgroup_size: u32,
    /// Stage-flags raw bits (mirrors `VkShaderStageFlags`).
    pub stage_bits: u32,
}

/// Snapshot of telemetry ring contents (test-friendly clone of state).
#[derive(Debug, Clone, Default)]
pub struct TelemetrySnapshot {
    /// Recorded validation events (newest last).
    pub validation_events: Vec<ValidationEvent>,
    /// Recorded pipeline executable properties.
    pub pipeline_properties: Vec<PipelineExecutableProperty>,
}

/// Lock-protected ring buffer for validation events + pipeline-property
/// snapshots.
#[derive(Debug)]
pub struct VulkanTelemetryRing {
    state: Mutex<TelemetrySnapshot>,
    /// Maximum entries before older ones are dropped.
    capacity: usize,
}

impl VulkanTelemetryRing {
    /// Create a ring with the default capacity (1024 entries per stream).
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }

    /// Create a ring with explicit capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            state: Mutex::new(TelemetrySnapshot::default()),
            capacity,
        }
    }

    /// Record a validation event ; oldest entries dropped if past
    /// capacity.
    pub fn record(&self, event: ValidationEvent) {
        let mut state = self.state.lock().expect("telemetry ring poisoned");
        state.validation_events.push(event);
        if state.validation_events.len() > self.capacity {
            // Drop from front to keep newest entries.
            let n = state.validation_events.len() - self.capacity;
            state.validation_events.drain(0..n);
        }
    }

    /// Record a pipeline executable property snapshot.
    pub fn record_pipeline_properties(&self, prop: PipelineExecutableProperty) {
        let mut state = self.state.lock().expect("telemetry ring poisoned");
        state.pipeline_properties.push(prop);
        if state.pipeline_properties.len() > self.capacity {
            let n = state.pipeline_properties.len() - self.capacity;
            state.pipeline_properties.drain(0..n);
        }
    }

    /// Snapshot current ring contents.
    #[must_use]
    pub fn snapshot(&self) -> TelemetrySnapshot {
        self.state.lock().expect("telemetry ring poisoned").clone()
    }

    /// Clear all events.
    pub fn clear(&self) {
        let mut state = self.state.lock().expect("telemetry ring poisoned");
        state.validation_events.clear();
        state.pipeline_properties.clear();
    }

    /// Total events recorded (validation + pipeline).
    #[must_use]
    pub fn total_events(&self) -> usize {
        let state = self.state.lock().expect("telemetry ring poisoned");
        state.validation_events.len() + state.pipeline_properties.len()
    }

    /// Capacity per stream.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for VulkanTelemetryRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(msg: &str) -> ValidationEvent {
        ValidationEvent {
            severity_bits: 0x0000_0010_u32,
            type_bits: 0x0000_0001_u32,
            message: msg.to_string(),
        }
    }

    #[test]
    fn ring_records_validation_events_in_order() {
        let r = VulkanTelemetryRing::new();
        r.record(ev("first"));
        r.record(ev("second"));
        let snap = r.snapshot();
        assert_eq!(snap.validation_events.len(), 2);
        assert_eq!(snap.validation_events[0].message, "first");
        assert_eq!(snap.validation_events[1].message, "second");
    }

    #[test]
    fn ring_caps_at_capacity_dropping_oldest() {
        let r = VulkanTelemetryRing::with_capacity(2);
        r.record(ev("a"));
        r.record(ev("b"));
        r.record(ev("c"));
        let snap = r.snapshot();
        assert_eq!(snap.validation_events.len(), 2);
        assert_eq!(snap.validation_events[0].message, "b");
        assert_eq!(snap.validation_events[1].message, "c");
    }

    #[test]
    fn ring_records_pipeline_properties() {
        let r = VulkanTelemetryRing::new();
        let p = PipelineExecutableProperty {
            name: "main".into(),
            description: "compute".into(),
            subgroup_size: 32,
            stage_bits: 0x20,
        };
        r.record_pipeline_properties(p);
        let snap = r.snapshot();
        assert_eq!(snap.pipeline_properties.len(), 1);
        assert_eq!(snap.pipeline_properties[0].subgroup_size, 32);
    }

    #[test]
    fn ring_clear_resets_both_streams() {
        let r = VulkanTelemetryRing::new();
        r.record(ev("x"));
        r.clear();
        assert_eq!(r.total_events(), 0);
    }

    #[test]
    fn ring_capacity_is_default_1024() {
        let r = VulkanTelemetryRing::default();
        assert_eq!(r.capacity(), 1024);
    }
}
