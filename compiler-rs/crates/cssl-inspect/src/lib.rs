// § cssl-inspect — Runtime introspection for the CSSLv3 substrate (L3 diagnostic-infra).
//
// § T11-D162 (W-Jη-1) : MVP slice. Read-only world-state inspection.
// Foundation for MCP inspect_cell / inspect_entity tools.
//
// § Phase-J spec : `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 2 (L3).
//
// § Scope of this slice
//   - SceneGraphSnapshot (read-only view over a scene)
//   - EntitySnapshot     (one entity ; body-omnoid layer summaries)
//   - FieldCellSnapshot  (Σ-mask-gated field-cell read)
//   - pause / resume / step time-control
//   - capture_frame      (PNG/EXR/spectral-bin format-tags ; mock-impl)
//
// § Σ-mask gate
//   The phase-J spec mandates EVERY inspector method that touches a cell,
//   entity, or KAN net funnel through a SigmaOverlay::at(key) check. In this
//   MVP slice the upstream cssl-substrate-omega-field crate may not yet have
//   landed in the workspace ; we MOCK the SigmaOverlay surface via the
//   `mock_substrate` module below. The mock's discriminator is a string-tag :
//   any cell whose tag contains "biometric" returns a refusal ; everything
//   else permits Observe. When the real substrate crate lands the swap is
//   one line : replace the mock_substrate re-exports with the real ones.
//
// § ATTESTATION (PRIME_DIRECTIVE.md § 11)
//   There was no hurt nor harm in the making of this, to anyone, anything,
//   or anybody.

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]
#![warn(missing_docs)]
#![doc = "Runtime introspection (L3) for the CSSLv3 substrate."]

pub mod capture;
pub mod mock_substrate;
pub mod snapshot;
pub mod time_control;

pub use capture::{capture_frame, CaptureFormat, CaptureHandle};
pub use mock_substrate::{
    Cap, ConsentBit, DevMode, MortonKey, SigmaConsentBits, SigmaOverlay, TelemetryEgress,
};
pub use snapshot::{EntityId, EntitySnapshot, FieldCellSnapshot, MaterialView, SceneGraphSnapshot};
pub use time_control::{TimeControl, TimeMode};

/// § ATTESTATION (PRIME_DIRECTIVE.md § 11)
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Slice identifier.
pub const SLICE_ID: &str = "T11-D162 (W-Jη-1) cssl-inspect";

/// Crate-wide error type.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum InspectError {
    /// Σ-mask refused the read.
    #[error("consent denied : {reason}")]
    ConsentDenied {
        /// Why the gate refused.
        reason: String,
    },
    /// Cell / entity not found.
    #[error("not found : {what}")]
    NotFound {
        /// What was not found.
        what: String,
    },
    /// Time-control transition refused.
    #[error("time-control transition refused : {reason}")]
    TimeControlRefused {
        /// Why the transition is refused.
        reason: String,
    },
    /// Capture-format unsupported.
    #[error("capture format unsupported : {tag}")]
    CaptureFormatUnsupported {
        /// The unsupported format tag.
        tag: String,
    },
    /// Capability missing.
    #[error("capability missing : {needed}")]
    CapabilityMissing {
        /// Human-readable name of the missing capability.
        needed: String,
    },
    /// Mock / scaffolding error.
    #[error("mock-substrate scaffolding error : {detail}")]
    MockScaffolding {
        /// Detail of the scaffolding-only error.
        detail: String,
    },
}

/// The top-level inspector handle.
#[derive(Debug)]
pub struct Inspector {
    cap_dev: Cap<DevMode>,
    scene: SceneGraphSnapshot,
    time: TimeControl,
    audit_seq: u64,
}

impl Inspector {
    /// Attach an inspector to a (mock) scene.
    ///
    /// # Errors
    /// Returns `CapabilityMissing` without dev-mode capability.
    pub fn attach(cap_dev: Cap<DevMode>, scene: SceneGraphSnapshot) -> Result<Self, InspectError> {
        if !cap_dev.permits_dev_mode() {
            return Err(InspectError::CapabilityMissing {
                needed: "Cap<DevMode>".into(),
            });
        }
        Ok(Self {
            cap_dev,
            scene,
            time: TimeControl::new(),
            audit_seq: 0,
        })
    }

    /// Inspect a single field-cell.
    ///
    /// # Errors
    /// `ConsentDenied` if Σ-mask refuses ; `NotFound` if missing.
    pub fn inspect_cell(&mut self, key: MortonKey) -> Result<FieldCellSnapshot, InspectError> {
        self.audit_seq = self.audit_seq.saturating_add(1);
        let cell = self
            .scene
            .cell_by_key(key)
            .ok_or_else(|| InspectError::NotFound {
                what: format!("morton-key 0x{:x}", key.raw()),
            })?
            .clone();
        let sigma = SigmaOverlay::at(&cell.tag);
        if !sigma.permits(ConsentBit::Observe) {
            return Err(InspectError::ConsentDenied {
                reason: format!("Σ-mask refused observe on tag '{}'", cell.tag),
            });
        }
        Ok(cell.with_audit_seq(self.audit_seq))
    }

    /// Inspect a single entity by id.
    ///
    /// # Errors
    /// `ConsentDenied` if entity refuses ; `NotFound` if missing.
    pub fn inspect_entity(&mut self, id: EntityId) -> Result<EntitySnapshot, InspectError> {
        self.audit_seq = self.audit_seq.saturating_add(1);
        let entity = self
            .scene
            .entity_by_id(id)
            .ok_or_else(|| InspectError::NotFound {
                what: format!("entity-id {}", id.raw()),
            })?
            .clone();
        let sigma = SigmaOverlay::at(&entity.tag);
        if !sigma.permits(ConsentBit::Observe) {
            return Err(InspectError::ConsentDenied {
                reason: format!("Σ-mask refused observe on entity tag '{}'", entity.tag),
            });
        }
        Ok(entity.with_audit_seq(self.audit_seq))
    }

    /// Read-only access to the scene graph.
    #[must_use]
    pub fn scene(&self) -> &SceneGraphSnapshot {
        &self.scene
    }

    /// Current monotone audit-sequence value.
    #[must_use]
    pub fn audit_seq(&self) -> u64 {
        self.audit_seq
    }

    /// Pause the engine. Idempotent.
    ///
    /// # Errors
    /// Currently never errors.
    pub fn pause(&mut self) -> Result<TimeMode, InspectError> {
        self.time.pause()
    }

    /// Resume the engine. Idempotent.
    ///
    /// # Errors
    /// Currently never errors.
    pub fn resume(&mut self) -> Result<TimeMode, InspectError> {
        self.time.resume()
    }

    /// Step the engine `n_frames` then return to paused.
    ///
    /// # Errors
    /// `TimeControlRefused` if `n_frames == 0` or not paused.
    pub fn step(&mut self, n_frames: u32) -> Result<TimeMode, InspectError> {
        self.time.step(n_frames)
    }

    /// Capture a frame in the requested format.
    ///
    /// # Errors
    /// `CapabilityMissing` if `egress` does not grant telemetry-egress ;
    /// `CaptureFormatUnsupported` if format invalid.
    pub fn capture_frame(
        &mut self,
        egress: &Cap<TelemetryEgress>,
        format: CaptureFormat,
    ) -> Result<CaptureHandle, InspectError> {
        self.audit_seq = self.audit_seq.saturating_add(1);
        capture_frame(egress, format, self.audit_seq)
    }

    /// Current time-mode.
    #[must_use]
    pub fn time_mode(&self) -> TimeMode {
        self.time.mode()
    }

    /// Current time-control object.
    #[must_use]
    pub fn time_control(&self) -> &TimeControl {
        &self.time
    }

    /// Capability handle.
    #[must_use]
    pub fn cap_dev(&self) -> &Cap<DevMode> {
        &self.cap_dev
    }
}

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn attestation_const_is_canonical() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn slice_id_starts_with_t11() {
        assert!(SLICE_ID.starts_with("T11-D162"));
    }

    #[test]
    fn attach_refuses_non_dev_cap() {
        let bad = Cap::<DevMode>::synthetic_nondev_for_tests();
        let scene = SceneGraphSnapshot::empty();
        assert!(matches!(
            Inspector::attach(bad, scene),
            Err(InspectError::CapabilityMissing { .. })
        ));
    }

    #[test]
    fn attach_with_real_dev_cap() {
        let cap = Cap::<DevMode>::dev_for_tests();
        let scene = SceneGraphSnapshot::empty();
        assert!(Inspector::attach(cap, scene).is_ok());
    }
}
