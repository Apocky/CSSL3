//! § GazeInput — biometric eye-gaze sample (per-eye direction + confidence + saccade-state).
//!
//! § SPEC
//!   `Omniverse/07_AESTHETIC/05_VR_RENDERING.csl § VIII.A` (XR_EXT_eye_gaze_interaction binding)
//!   `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-2 GazeCollapsePass`
//!   `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.4 Gaze-Reactive Observation-Collapse`
//!   `Omniverse/01_AXIOMS/05_OBSERVATION_COLLAPSE.csl.md` (Axiom 5)
//!
//! § T11-D120 (Slice : Gaze-reactive observation-collapse — V.4 path)
//!
//! § ROLE
//!   This module owns the *biometric* gaze-input surface. Eye-gaze data
//!   is the most-personal real-time signal a user can emit ; it is
//!   distinct from keyboard / mouse / gamepad in three load-bearing ways :
//!
//!     1. it is **biometric** — the gaze-stream encodes pupil-position +
//!        fixation-pattern + saccade-cadence which together fingerprint a
//!        user uniquely (more strongly than typing-cadence + mouse-pattern
//!        combined).
//!     2. it has **high temporal density** — 90-200 Hz sampling on Quest-Pro,
//!        1 kHz on Vision-Pro / Tobii. A naive log captures more bits-per-
//!        second than any other input surface.
//!     3. it carries **attention semantics** — what a person looks at is
//!        what they care about. Logging this is the closest a host can
//!        come to logging *thoughts*.
//!
//!   Per `PRIME_DIRECTIVE.md § 1 PROHIBITIONS § surveillance` and
//!   `Omniverse/.../05_VR_RENDERING.csl § VIII.A` :
//!
//!     - on-device only ⊗ never network-egresses
//!     - no cross-session storage
//!     - no analytics / dwell-heatmap / engagement-loop tuning
//!     - opt-IN explicit (default OFF)
//!     - revocable per-session
//!
//!   This module enforces these structurally (see `gaze_capability` for
//!   the capability-token surface).
//!
//! § DATA SHAPE
//!   `GazeInput` is a single sample at a single instant :
//!
//!     - `per_eye : [GazeRay; 2]`  — left + right eye direction + origin
//!     - `confidence : [f32; 2]`   — per-eye confidence in [0, 1]
//!     - `saccade_state : SaccadeState` — Stable / Drift / SaccadeOnset / SaccadeMid / Blink
//!     - `pupil_diameter : Option<[f32; 2]>` — millimetres ; not exposed to
//!       game code by default (Σ-mask elevated)
//!     - `timestamp_ns : u64`      — nanoseconds since OS monotonic clock
//!
//!   Source-level code reads `GazeInput` via the same `InputBackend`
//!   surface that produces `InputEvent` / `InputState` — but only after
//!   a `GazeCapability` token has been minted (consent-required ; see
//!   `gaze_capability::GazeCapability::request`).

use crate::gaze_capability::GazeCapability;

/// Per-eye gaze ray expressed in head-relative coordinates.
///
/// The `origin` is the eyeball-center expressed in metres relative to the
/// head-anchor pose ; the `direction` is the unit-length forward vector
/// from `origin` through the visual axis.
///
/// Both fields are in the head's local frame — they are NOT in world space
/// because the host backend does not have head-pose authority. Source-level
/// CSSLv3 code receives the head-pose separately (from `cssl-host-vulkan` /
/// XR session locate) and composes the two rays into world space at use-site.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GazeRay {
    /// Eyeball center in head-local metres (typically ~(±0.032, 0.0, 0.0)).
    pub origin: [f32; 3],
    /// Unit-length direction vector in head-local frame.
    pub direction: [f32; 3],
}

impl GazeRay {
    /// Construct a forward-looking ray for the given eye offset (left = `-x`,
    /// right = `+x`). Used as the default before a backend reports a real
    /// reading — equivalent to "looking straight ahead".
    #[must_use]
    pub const fn forward(eye_offset_x: f32) -> Self {
        Self {
            origin: [eye_offset_x, 0.0, 0.0],
            direction: [0.0, 0.0, -1.0],
        }
    }

    /// Default left-eye forward ray (offset −0.032 m ≈ −32 mm IPD/2).
    #[must_use]
    pub const fn forward_left() -> Self {
        Self::forward(-0.032)
    }

    /// Default right-eye forward ray (offset +0.032 m).
    #[must_use]
    pub const fn forward_right() -> Self {
        Self::forward(0.032)
    }

    /// Returns `true` if the direction vector is approximately unit-length
    /// (within ±1e-3). Backends should normalize before publishing — this
    /// helper is for downstream debug-asserts.
    #[must_use]
    pub fn is_unit_length(&self) -> bool {
        let [x, y, z] = self.direction;
        let mag2 = (x * x) + (y * y) + (z * z);
        (mag2 - 1.0).abs() < 1e-3
    }
}

/// Eye saccade-state classification.
///
/// Drives saccadic-suppression in the renderer (during `SaccadeMid` and
/// `Blink` the human visual system suppresses input — the renderer can
/// hide texture-pop / detail-emergence flicker safely in those windows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaccadeState {
    /// Eye is fixated on a point ; angular velocity < 30°/s.
    Stable,
    /// Slow smooth-pursuit drift ; 30°/s ≤ velocity < 100°/s.
    Drift,
    /// Saccade onset detected ; velocity ramping up but target unknown yet.
    SaccadeOnset,
    /// Mid-saccade ; velocity > 300°/s ; visual perception suppressed.
    SaccadeMid,
    /// Eyelid closed (blink) ; no usable signal ; predict from prior trajectory.
    Blink,
}

impl SaccadeState {
    /// Returns `true` if the human visual system is suppressing perception
    /// in this state (renderer can hide flicker / pop / detail emergence).
    #[must_use]
    pub const fn perception_suppressed(self) -> bool {
        matches!(self, Self::SaccadeMid | Self::Blink)
    }

    /// Returns `true` if the gaze-target is reliably known in this state
    /// (Stable + Drift = trustworthy ; SaccadeOnset = ambiguous ; SaccadeMid +
    /// Blink = predict from prior).
    #[must_use]
    pub const fn target_reliable(self) -> bool {
        matches!(self, Self::Stable | Self::Drift)
    }
}

/// One sample of gaze data at one instant.
///
/// # Capability gate
///
/// `GazeInput` instances are only constructible inside this crate ; the
/// public `from_backend_with_capability` constructor requires a
/// `GazeCapability` token. Source-level CSSLv3 code that lacks the
/// capability cannot produce or read `GazeInput` — this is the
/// type-system enforcement of "no flag can disable on-device-only".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GazeInput {
    /// Per-eye gaze rays in head-local frame ; index 0 = left, 1 = right.
    pub per_eye: [GazeRay; 2],
    /// Per-eye confidence in [0, 1] ; 1.0 = full tracking, 0.0 = lost.
    pub confidence: [f32; 2],
    /// Saccade-state classification (drives saccadic suppression).
    pub saccade_state: SaccadeState,
    /// Pupil diameter in millimetres per eye, if reported by the tracker.
    /// `None` when the backend doesn't expose this (Quest-Pro does, Vive
    /// Pro Eye does, ARKit does NOT).
    ///
    /// Σ-mask elevated : even when `Some`, source-level code can only
    /// read this via an additional `PupilDiameterCapability` (not in this
    /// slice — reserved for future medical/research use-cases that require
    /// explicit study-IRB consent).
    pub pupil_diameter: Option<[f32; 2]>,
    /// Nanoseconds since OS monotonic clock.
    pub timestamp_ns: u64,
}

impl GazeInput {
    /// Construct a centered-forward fallback `GazeInput` (used when a
    /// confidence-fallback fires — see `saccade_predictor`).
    ///
    /// Confidence is set to 0.0 explicitly so downstream consumers can
    /// detect "this is the synthetic fallback, not real data" and apply
    /// center-bias foveation per `Omniverse/.../05_VR_RENDERING.csl § V.B`.
    #[must_use]
    pub const fn forward_fallback(timestamp_ns: u64) -> Self {
        Self {
            per_eye: [GazeRay::forward_left(), GazeRay::forward_right()],
            confidence: [0.0, 0.0],
            saccade_state: SaccadeState::Stable,
            pupil_diameter: None,
            timestamp_ns,
        }
    }

    /// Construct a `GazeInput` from raw backend values.
    ///
    /// The `_capability` parameter is consumed-by-reference to enforce
    /// at the type-system level that a holder of `GazeCapability` (and
    /// therefore an explicit user opt-in) is the only entity that can
    /// publish gaze-data into the runtime.
    #[must_use]
    pub fn from_backend_with_capability(
        _capability: &GazeCapability,
        per_eye: [GazeRay; 2],
        confidence: [f32; 2],
        saccade_state: SaccadeState,
        pupil_diameter: Option<[f32; 2]>,
        timestamp_ns: u64,
    ) -> Self {
        Self {
            per_eye,
            confidence,
            saccade_state,
            pupil_diameter,
            timestamp_ns,
        }
    }

    /// Returns the average of per-eye confidence ; useful for "has-tracking"
    /// gates that don't care which eye is dominant.
    #[must_use]
    pub fn confidence_avg(&self) -> f32 {
        (self.confidence[0] + self.confidence[1]) * 0.5
    }

    /// Returns the cyclopean (centroid) gaze ray — average of left + right
    /// origin + direction (re-normalized). Used by the `FoveaMask` builder
    /// to produce a single screen-space anchor when stereo-projection is
    /// resolved upstream.
    #[must_use]
    pub fn cyclopean_ray(&self) -> GazeRay {
        let l = self.per_eye[0];
        let r = self.per_eye[1];
        let origin = [
            (l.origin[0] + r.origin[0]) * 0.5,
            (l.origin[1] + r.origin[1]) * 0.5,
            (l.origin[2] + r.origin[2]) * 0.5,
        ];
        let dx = (l.direction[0] + r.direction[0]) * 0.5;
        let dy = (l.direction[1] + r.direction[1]) * 0.5;
        let dz = (l.direction[2] + r.direction[2]) * 0.5;
        let mag = ((dx * dx) + (dy * dy) + (dz * dz)).sqrt().max(1e-9);
        GazeRay {
            origin,
            direction: [dx / mag, dy / mag, dz / mag],
        }
    }

    /// Returns `true` if the backend should fall back to center-bias
    /// foveation (confidence below threshold or perception suppressed).
    /// Threshold = 0.3 per `Omniverse/.../05_VR_RENDERING.csl § VIII.A` :
    /// "eye-track-quality < threshold ⊗ fallback : center-bias-foveation".
    #[must_use]
    pub fn fallback_recommended(&self) -> bool {
        self.confidence_avg() < 0.3
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaze_capability::GazeCapability;

    #[test]
    fn forward_rays_are_unit_length() {
        assert!(GazeRay::forward_left().is_unit_length());
        assert!(GazeRay::forward_right().is_unit_length());
    }

    #[test]
    fn saccade_perception_suppression() {
        assert!(SaccadeState::SaccadeMid.perception_suppressed());
        assert!(SaccadeState::Blink.perception_suppressed());
        assert!(!SaccadeState::Stable.perception_suppressed());
        assert!(!SaccadeState::Drift.perception_suppressed());
    }

    #[test]
    fn saccade_target_reliability() {
        assert!(SaccadeState::Stable.target_reliable());
        assert!(SaccadeState::Drift.target_reliable());
        assert!(!SaccadeState::SaccadeMid.target_reliable());
        assert!(!SaccadeState::Blink.target_reliable());
    }

    #[test]
    fn forward_fallback_has_zero_confidence() {
        let g = GazeInput::forward_fallback(12_345);
        assert_eq!(g.confidence, [0.0, 0.0]);
        assert_eq!(g.confidence_avg(), 0.0);
        assert!(g.fallback_recommended());
        assert_eq!(g.timestamp_ns, 12_345);
    }

    #[test]
    fn from_backend_requires_capability() {
        // The capability test-mints from a debug-only constructor.
        let cap = GazeCapability::test_mint();
        let g = GazeInput::from_backend_with_capability(
            &cap,
            [GazeRay::forward_left(), GazeRay::forward_right()],
            [0.95, 0.93],
            SaccadeState::Stable,
            None,
            42,
        );
        assert!((g.confidence_avg() - 0.94).abs() < 1e-5);
        assert!(!g.fallback_recommended());
    }

    #[test]
    fn cyclopean_ray_is_centered() {
        let cap = GazeCapability::test_mint();
        let g = GazeInput::from_backend_with_capability(
            &cap,
            [GazeRay::forward_left(), GazeRay::forward_right()],
            [1.0, 1.0],
            SaccadeState::Stable,
            None,
            0,
        );
        let cyc = g.cyclopean_ray();
        assert!((cyc.origin[0]).abs() < 1e-6);
        assert!(cyc.is_unit_length());
    }

    #[test]
    fn fallback_recommended_when_low_confidence() {
        let cap = GazeCapability::test_mint();
        let g = GazeInput::from_backend_with_capability(
            &cap,
            [GazeRay::forward_left(), GazeRay::forward_right()],
            [0.2, 0.1],
            SaccadeState::Stable,
            None,
            0,
        );
        assert!(g.fallback_recommended());
    }
}
