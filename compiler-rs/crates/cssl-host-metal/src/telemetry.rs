//! Telemetry-ring placeholder hooks for the Metal host.
//!
//! § SPEC : `specs/22_TELEMETRY.csl § R18` (full integration is a later slice ;
//!   this module wires command-buffer begin/end + GPU-time samples through a
//!   caller-supplied `cssl_telemetry::TelemetryRing`).
//!
//! § DESIGN
//!   - [`MetalTelemetryProbe`] holds a `TelemetryRing` reference + emits one
//!     `TelemetrySlot` per command-buffer commit (kind = `Sample`,
//!     scope = `HostSubmit`).
//!   - On non-Apple hosts the probe still functions ; samples carry the stub
//!     command-buffer id rather than a real GPU timestamp. This lets parallel
//!     CSSLv3 builds share the same telemetry-shape regardless of host.

use thiserror::Error;

use cssl_telemetry::{RingError, TelemetryKind, TelemetryRing, TelemetryScope, TelemetrySlot};

/// Failure modes for [`MetalTelemetryProbe`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TelemetryEmitError {
    /// The telemetry-ring rejected the slot (full ring + overflow saturated).
    #[error("telemetry ring rejected slot: {detail}")]
    Ring {
        /// Underlying ring-error display.
        detail: String,
    },
}

impl From<RingError> for TelemetryEmitError {
    fn from(e: RingError) -> Self {
        Self::Ring {
            detail: format!("{e}"),
        }
    }
}

/// Telemetry probe that samples Metal command-buffer events into a
/// [`TelemetryRing`].
#[derive(Debug)]
pub struct MetalTelemetryProbe {
    /// Ring slots are pushed into. Caller owns the ring ; the probe holds
    /// a reference so multiple Metal sessions can share one ring.
    ring: *const TelemetryRing,
    /// Number of samples this probe has emitted.
    pub samples_emitted: u64,
    /// Number of samples this probe has dropped due to ring-overflow.
    pub samples_dropped: u64,
}

impl MetalTelemetryProbe {
    /// Construct a probe over the supplied ring. The caller MUST keep the
    /// ring alive for the probe's lifetime.
    ///
    /// # Safety
    ///
    /// The probe stores a raw pointer to `ring` ; the caller must not drop
    /// `ring` while the probe is alive. This is the same contract used in
    /// `cssl-host-level-zero::live_telemetry::LiveTelemetryProbe`.
    #[must_use]
    pub fn new(ring: &TelemetryRing) -> Self {
        Self {
            ring: ring as *const TelemetryRing,
            samples_emitted: 0,
            samples_dropped: 0,
        }
    }

    /// Emit a command-buffer-commit sample.
    pub fn emit_command_buffer_commit(
        &mut self,
        cb_id: u64,
        timestamp_ns: u64,
    ) -> Result<(), TelemetryEmitError> {
        // SAFETY: contract from `Self::new` requires the ring to outlive this probe.
        let ring = unsafe { &*self.ring };
        let mut payload = [0u8; 40];
        let cb_bytes = cb_id.to_le_bytes();
        payload[..8].copy_from_slice(&cb_bytes);
        let slot = TelemetrySlot::new(
            timestamp_ns,
            TelemetryScope::DispatchLatency,
            TelemetryKind::Sample,
        )
        .with_inline_payload(&payload);
        match ring.push(slot) {
            Ok(()) => {
                self.samples_emitted += 1;
                Ok(())
            }
            Err(e) => {
                self.samples_dropped += 1;
                Err(e.into())
            }
        }
    }

    /// Emit a GPU-time sample (encoder begin / end pair).
    pub fn emit_gpu_time(
        &mut self,
        cb_id: u64,
        timestamp_ns: u64,
        gpu_duration_ns: u64,
    ) -> Result<(), TelemetryEmitError> {
        // SAFETY: contract from `Self::new`.
        let ring = unsafe { &*self.ring };
        let mut payload = [0u8; 40];
        payload[..8].copy_from_slice(&cb_id.to_le_bytes());
        payload[8..16].copy_from_slice(&gpu_duration_ns.to_le_bytes());
        let slot = TelemetrySlot::new(
            timestamp_ns,
            TelemetryScope::DispatchLatency,
            TelemetryKind::Sample,
        )
        .with_inline_payload(&payload);
        match ring.push(slot) {
            Ok(()) => {
                self.samples_emitted += 1;
                Ok(())
            }
            Err(e) => {
                self.samples_dropped += 1;
                Err(e.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use cssl_telemetry::TelemetryRing;

    use super::MetalTelemetryProbe;

    #[test]
    fn probe_emits_command_buffer_commit_into_ring() {
        let ring = TelemetryRing::new(8);
        let mut probe = MetalTelemetryProbe::new(&ring);
        probe.emit_command_buffer_commit(42, 1_000).unwrap();
        assert_eq!(probe.samples_emitted, 1);
        assert_eq!(probe.samples_dropped, 0);
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn probe_emits_gpu_time_with_duration_payload() {
        let ring = TelemetryRing::new(8);
        let mut probe = MetalTelemetryProbe::new(&ring);
        probe.emit_gpu_time(7, 2_000, 500).unwrap();
        assert_eq!(probe.samples_emitted, 1);
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn probe_records_drops_on_full_ring() {
        let ring = TelemetryRing::new(2);
        let mut probe = MetalTelemetryProbe::new(&ring);
        probe.emit_command_buffer_commit(1, 100).unwrap();
        probe.emit_command_buffer_commit(2, 200).unwrap();
        // Third push must overflow.
        let r = probe.emit_command_buffer_commit(3, 300);
        assert!(r.is_err());
        assert_eq!(probe.samples_emitted, 2);
        assert_eq!(probe.samples_dropped, 1);
    }
}
