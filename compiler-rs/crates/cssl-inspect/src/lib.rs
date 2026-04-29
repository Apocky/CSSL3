// § cssl-inspect — Runtime introspection for the CSSLv3 substrate (L3 diagnostic-infra).
//
// § T11-D162 (W-Jη-1) : MVP slice. Read-only world-state inspection.
// Foundation for MCP inspect_cell / inspect_entity tools.
//
// § Phase-J spec : `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 2 (L3).
//
// § Scope of this slice
//   ∋ SceneGraphSnapshot (read-only view over a scene)
//   ∋ EntitySnapshot     (one entity ; body-omnoid layer summaries)
//   ∋ FieldCellSnapshot  (Σ-mask-gated field-cell read)
//   ∋ pause / resume / step time-control
//   ∋ capture_frame      (PNG/EXR/spectral-bin format-tags ; mock-impl)
//   ∌ KAN-eval inspector       (deferred to W-Jη-2)
//   ∌ ψ-field inspector        (deferred to W-Jη-2)
//   ∌ replay-record extension  (deferred to W-Jη-3)
//   ∌ live-tweak / hot-reload  (L4 ; deferred to W-Jη-4)
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
// § Read-only API discipline
//   No method on Inspector takes `&mut self` for a state-of-the-world read.
//   Time-control methods (pause/step/resume) DO take `&mut self` because
//   they mutate the inspector's local time-control state machine — but they
//   do NOT mutate world state ; they request the host engine to honour the
//   pause/step/resume mode. In MVP that request is a no-op state-machine.
//
// § PRIME_DIRECTIVE compliance
//   ∀ inspect-method M ⊑ Inspector :
//     M(key) ≡ {
//       let sigma = SigmaOverlay::at(key);
//       if ¬ sigma.permits(Observe):
//         return Err(ConsentDenied);
//       Ok(snapshot)
//     }
//   No biometric class data is producible by this slice — biometric tags
//   refuse at the gate. Real D138 MIR-pass integration is a follow-up
//   slice ; this slice supplies the runtime-side gate stub.
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
///
/// Asserted at slice-authoring time : "There was no hurt nor harm in the
/// making of this, to anyone, anything, or anybody." Embedded as a const
/// so every consumer can `assert_eq!` against it in their own attestation
/// chain — this is the substrate-invariant signature that propagates.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Slice identifier — used in audit-log entries when the inspector is
/// initialised. Bumping this on every slice means the audit chain records
/// which inspector revision produced each snapshot.
pub const SLICE_ID: &str = "T11-D162 (W-Jη-1) cssl-inspect";

/// Crate-wide error type. Every fallible inspector method returns this.
///
/// The variants map 1:1 to phase-J spec § 2.7 error categorisation : consent
/// denials are SEPARATE from internal errors so MCP-tool authors can present
/// "this read was refused for consent reasons" differently from "the
/// inspector crashed". The discriminant is stable.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum InspectError {
    /// Σ-mask refused the read. The `reason` is a human-readable string
    /// (e.g. "biometric class refused" or "Observe consent absent").
    #[error("consent denied : {reason}")]
    ConsentDenied {
        /// Why the gate refused.
        reason: String,
    },
    /// Cell / entity not found in the scene graph.
    #[error("not found : {what}")]
    NotFound {
        /// What was not found (e.g. "morton-key 0xdeadbeef").
        what: String,
    },
    /// Time-control transition refused (e.g. step() while engine is running).
    #[error("time-control transition refused : {reason}")]
    TimeControlRefused {
        /// Why the transition is refused.
        reason: String,
    },
    /// Capture-format unsupported (slice-level mock returns this for
    /// everything except the three documented format tags).
    #[error("capture format unsupported : {tag}")]
    CaptureFormatUnsupported {
        /// The unsupported format tag.
        tag: String,
    },
    /// Capability missing — operation requires Cap<DevMode> or
    /// Cap<TelemetryEgress> not held by the caller.
    #[error("capability missing : {needed}")]
    CapabilityMissing {
        /// Human-readable name of the missing capability.
        needed: String,
    },
    /// Mock / scaffolding error (will not appear in production builds once
    /// the real substrate crates land).
    #[error("mock-substrate scaffolding error : {detail}")]
    MockScaffolding {
        /// Detail of the scaffolding-only error.
        detail: String,
    },
}

/// The top-level inspector handle.
///
/// Construction requires a `Cap<DevMode>` token — release builds with no
/// dev-mode capability cannot construct an `Inspector`. The phase-J spec
/// further requires that the `dev-mode` cargo feature gate the inspector
/// out of release binaries entirely ; this slice keeps the construction
/// gate ; the LTO drop-check is a later slice.
///
/// All methods on `Inspector` that read world state funnel through the
/// Σ-mask gate. No method bypasses it. The MVP gate is a string-tag
/// discriminator (see `mock_substrate`) ; it will be replaced with the real
/// `SigmaOverlay::at(key)` call when cssl-substrate-omega-field lands.
#[derive(Debug)]
pub struct Inspector {
    /// Dev-mode capability token. Constructor refuses without one.
    cap_dev: Cap<DevMode>,
    /// Mock-substrate scene used by the read methods.
    scene: SceneGraphSnapshot,
    /// Local time-control state machine.
    time: TimeControl,
    /// Audit-sequence counter — every read bumps this.
    audit_seq: u64,
}

impl Inspector {
    /// Attach an inspector to a (mock) scene with the supplied dev-mode
    /// capability. Production code-path receives a real scene-graph
    /// reference instead of a constructed mock ; that swap is one line and
    /// is documented in `mock_substrate`.
    ///
    /// # Errors
    /// Returns `CapabilityMissing` if the cap-token claims to be a
    /// non-dev-mode capability (the mock surface lets us simulate that).
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
    /// Returns `ConsentDenied` if the Σ-mask gate refuses ; returns
    /// `NotFound` if the morton-key is not present in the scene.
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
    /// Returns `ConsentDenied` if the entity tag refuses Observe ; returns
    /// `NotFound` if the entity-id is not present.
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

    /// Read-only access to the scene graph (Σ-mask still applies on per-cell
    /// reads — the borrow itself is permitted because no biometric data is
    /// reachable through the surface API ; D132 ensures snapshot types do
    /// not contain biometric-class fields at compile time).
    #[must_use]
    pub fn scene(&self) -> &SceneGraphSnapshot {
        &self.scene
    }

    /// Current monotone audit-sequence value. Increments on every read.
    #[must_use]
    pub fn audit_seq(&self) -> u64 {
        self.audit_seq
    }

    /// Pause the engine. Idempotent.
    ///
    /// # Errors
    /// Returns `TimeControlRefused` if the inspector is already pausing in a
    /// non-idempotent way (mock-impl never refuses idempotent transitions).
    pub fn pause(&mut self) -> Result<TimeMode, InspectError> {
        self.time.pause()
    }

    /// Resume the engine. Idempotent.
    ///
    /// # Errors
    /// Currently never errors in mock-impl.
    pub fn resume(&mut self) -> Result<TimeMode, InspectError> {
        self.time.resume()
    }

    /// Step the engine `n_frames` then return to paused state.
    ///
    /// # Errors
    /// Returns `TimeControlRefused` if `n_frames == 0` (a no-op step is a
    /// usage-error per phase-J § 2.6).
    pub fn step(&mut self, n_frames: u32) -> Result<TimeMode, InspectError> {
        self.time.step(n_frames)
    }

    /// Capture a frame in the requested format. The MVP returns a fake
    /// path-hash + format-tag tuple. Real-impl will engage the
    /// render-graph fence + invoke the per-format encoder.
    ///
    /// # Errors
    /// Returns `CapabilityMissing` if `egress` is not a TelemetryEgress
    /// token ; returns `CaptureFormatUnsupported` if the format is unknown
    /// (it never is — the enum is closed — but the variant exists for
    /// forward-compatibility with the real-impl).
    pub fn capture_frame(
        &mut self,
        egress: &Cap<TelemetryEgress>,
        format: CaptureFormat,
    ) -> Result<CaptureHandle, InspectError> {
        self.audit_seq = self.audit_seq.saturating_add(1);
        capture_frame(egress, format, self.audit_seq)
    }

    /// Current time-mode (read-only).
    #[must_use]
    pub fn time_mode(&self) -> TimeMode {
        self.time.mode()
    }

    /// Current time-control object (read-only). Useful for tests.
    #[must_use]
    pub fn time_control(&self) -> &TimeControl {
        &self.time
    }

    /// Capability handle (read-only). Returns the dev-mode capability so
    /// callers can verify the inspector is running in the expected mode.
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
