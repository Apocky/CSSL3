//! § AudioListener — wave-unity ψ-AUDIO sampling probe.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § XV ANTI-PATTERNS` :
//!
//!   ```text
//!   | Audio-listener as separate query-path | violates §0 same-substrate ⊗ AudioListener samples ψ |
//!   ```
//!
//!   The wave-unity AudioListener is NOT a microphone-array, NOT a HRTF
//!   sampling abstraction, NOT a separate query-path. It is a **probe
//!   into the ψ-AUDIO band** : a position, an orientation, an ear-baseline
//!   vector, and a velocity for Doppler. The `WaveAudioProjector` reads
//!   the ψ-field at the per-ear sample positions to recover binaural
//!   pressure.
//!
//! § PER-EAR PROBES
//!   Unlike the legacy mixer's listener (one position + ITD synthesized
//!   from pan), the wave-unity listener carries TWO probe positions —
//!   the left ear and the right ear, separated by the head-baseline
//!   (default ≈ 17.5 cm). Each ear samples ψ_AUDIO independently ; ITD
//!   and ILD emerge from the SDF + LBM propagation, not from a synthesis
//!   formula.
//!
//! § DOPPLER VIA Ω.S-FACET
//!   Per spec § XIII the AudioListener velocity feeds the Doppler shift
//!   computation. The shift is APPLIED AT SAMPLE-TIME in the projector,
//!   not pre-baked into the field — this preserves the ψ-substrate's
//!   "no separate query path" invariant.

use crate::vec3::Vec3;

/// Forward + up basis pair encoding the listener's head orientation.
/// Identical convention to the legacy mixer's [`Orientation`] but lives
/// here so cssl-wave-audio is link-independent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orientation {
    /// Forward direction the head is facing (default `-Z`).
    pub forward: Vec3,
    /// Up direction (default `+Y`).
    pub up: Vec3,
}

impl Orientation {
    /// Standard "looking down -Z, up = +Y" right-handed orientation.
    pub const STANDARD: Orientation = Orientation {
        forward: Vec3::FORWARD,
        up: Vec3::UP,
    };

    /// Construct a new orientation. Inputs are normalized at construction
    /// so callers don't have to.
    #[must_use]
    pub fn new(forward: Vec3, up: Vec3) -> Orientation {
        Orientation {
            forward: forward.normalize(),
            up: up.normalize(),
        }
    }

    /// Right-vector derived from `forward × up`, normalized.
    /// `Vec3::ZERO` when forward + up are degenerate.
    #[must_use]
    pub fn right(self) -> Vec3 {
        self.forward.cross(self.up).normalize()
    }
}

impl Default for Orientation {
    fn default() -> Orientation {
        Orientation::STANDARD
    }
}

/// Standard adult head-baseline (left-ear-to-right-ear distance) in
/// metres ≈ 17.5 cm. Matches the legacy mixer's `2 × HEAD_RADIUS` and
/// the median anatomical ear-spacing measurement.
pub const STANDARD_HEAD_BASELINE: f32 = 0.175;

/// Default speed of sound at room temperature (m/s @ 20°C). Matches
/// the legacy mixer's `SPEED_OF_SOUND` constant verbatim. This is the
/// value plugged into Doppler + LBM stream-velocity by default ; the
/// solver may shift it per-cell when temperature varies (per spec §
/// V.2 sound-caustics RG-time-of-day modulation).
pub const SPEED_OF_SOUND: f32 = 343.0;

/// AudioListener — a probe pair sampling ψ_AUDIO at the left + right
/// ear positions.
///
/// § FIELDS
///   - `position` : world-space head-center.
///   - `orientation` : forward + up basis. Right is derived.
///   - `velocity` : head-center velocity in m/s ; powers Doppler.
///   - `head_baseline` : left-ear-to-right-ear distance (m). Default
///     [`STANDARD_HEAD_BASELINE`].
///   - `gain` : master post-projection gain (1.0 = unity).
///
/// § DETERMINISM
///   The struct is `Copy + PartialEq` ; per-ear positions are derived
///   functions of the fields above, so two listeners constructed with
///   the same field values produce bit-identical ear-probes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioListener {
    /// World-space head-center position.
    pub position: Vec3,
    /// Forward + up orientation.
    pub orientation: Orientation,
    /// Head-center velocity (m/s) for Doppler.
    pub velocity: Vec3,
    /// Left-to-right ear baseline distance in metres.
    pub head_baseline: f32,
    /// Master post-projection gain.
    pub gain: f32,
}

impl AudioListener {
    /// Construct a listener at the world origin with standard
    /// orientation, zero velocity, default baseline + unity gain.
    #[must_use]
    pub const fn at_origin() -> AudioListener {
        AudioListener {
            position: Vec3::ZERO,
            orientation: Orientation::STANDARD,
            velocity: Vec3::ZERO,
            head_baseline: STANDARD_HEAD_BASELINE,
            gain: 1.0,
        }
    }

    /// Construct a listener at `pos` with standard orientation.
    #[must_use]
    pub const fn at(pos: Vec3) -> AudioListener {
        AudioListener {
            position: pos,
            orientation: Orientation::STANDARD,
            velocity: Vec3::ZERO,
            head_baseline: STANDARD_HEAD_BASELINE,
            gain: 1.0,
        }
    }

    /// World-space position of the LEFT ear : head-center minus half
    /// the baseline along the right axis.
    #[must_use]
    pub fn left_ear(&self) -> Vec3 {
        let half = self.head_baseline * 0.5;
        self.position.sub(self.orientation.right().scale(half))
    }

    /// World-space position of the RIGHT ear : head-center plus half
    /// the baseline along the right axis.
    #[must_use]
    pub fn right_ear(&self) -> Vec3 {
        let half = self.head_baseline * 0.5;
        self.position.add(self.orientation.right().scale(half))
    }

    /// Update head position. `const` so callers can build a snapshot
    /// listener in a `const fn` context.
    pub const fn set_position(&mut self, pos: Vec3) {
        self.position = pos;
    }

    /// Update orientation ; vectors are re-normalized.
    pub fn set_orientation(&mut self, forward: Vec3, up: Vec3) {
        self.orientation = Orientation::new(forward, up);
    }

    /// Update velocity.
    pub const fn set_velocity(&mut self, vel: Vec3) {
        self.velocity = vel;
    }

    /// Update head-baseline ; clamped to a positive value.
    pub fn set_head_baseline(&mut self, baseline: f32) {
        self.head_baseline = baseline.max(1e-3);
    }

    /// Update master gain ; clamped to non-negative.
    pub fn set_gain(&mut self, g: f32) {
        self.gain = g.max(0.0);
    }
}

impl Default for AudioListener {
    fn default() -> AudioListener {
        AudioListener::at_origin()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{AudioListener, Orientation, STANDARD_HEAD_BASELINE};
    use crate::vec3::Vec3;

    #[test]
    fn orientation_standard_constants() {
        assert_eq!(Orientation::STANDARD.forward, Vec3::FORWARD);
        assert_eq!(Orientation::STANDARD.up, Vec3::UP);
    }

    #[test]
    fn orientation_normalizes_at_construction() {
        let o = Orientation::new(Vec3::new(0.0, 0.0, -2.0), Vec3::new(0.0, 5.0, 0.0));
        assert!((o.forward.length() - 1.0).abs() < 1e-6);
        assert!((o.up.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orientation_right_is_x_under_standard() {
        // (-Z) × (+Y) = (+X) right-handed.
        let r = Orientation::STANDARD.right();
        assert!((r.x - 1.0).abs() < 1e-6);
        assert!(r.y.abs() < 1e-6);
        assert!(r.z.abs() < 1e-6);
    }

    #[test]
    fn listener_at_origin_defaults() {
        let l = AudioListener::at_origin();
        assert_eq!(l.position, Vec3::ZERO);
        assert_eq!(l.velocity, Vec3::ZERO);
        assert_eq!(l.gain, 1.0);
        assert_eq!(l.head_baseline, STANDARD_HEAD_BASELINE);
    }

    #[test]
    fn listener_at_keeps_position() {
        let pos = Vec3::new(1.0, 2.0, 3.0);
        let l = AudioListener::at(pos);
        assert_eq!(l.position, pos);
    }

    #[test]
    fn left_ear_offset_minus_x_under_standard() {
        let l = AudioListener::at_origin();
        let le = l.left_ear();
        // Right axis is +X under standard ; left ear is at -baseline/2 X.
        assert!((le.x - (-STANDARD_HEAD_BASELINE * 0.5)).abs() < 1e-6);
        assert!(le.y.abs() < 1e-6);
        assert!(le.z.abs() < 1e-6);
    }

    #[test]
    fn right_ear_offset_plus_x_under_standard() {
        let l = AudioListener::at_origin();
        let re = l.right_ear();
        assert!((re.x - (STANDARD_HEAD_BASELINE * 0.5)).abs() < 1e-6);
    }

    #[test]
    fn ear_baseline_distance_matches() {
        let l = AudioListener::at(Vec3::new(5.0, 0.0, 0.0));
        let le = l.left_ear();
        let re = l.right_ear();
        assert!((le.sub(re).length() - STANDARD_HEAD_BASELINE).abs() < 1e-6);
    }

    #[test]
    fn set_position_updates() {
        let mut l = AudioListener::at_origin();
        l.set_position(Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(l.position.x, 1.0);
    }

    #[test]
    fn set_orientation_normalizes() {
        let mut l = AudioListener::at_origin();
        l.set_orientation(Vec3::new(0.0, 0.0, -3.0), Vec3::new(0.0, 4.0, 0.0));
        assert!((l.orientation.forward.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn set_velocity_updates() {
        let mut l = AudioListener::at_origin();
        l.set_velocity(Vec3::new(2.5, 0.0, 0.0));
        assert_eq!(l.velocity.x, 2.5);
    }

    #[test]
    fn set_head_baseline_clamps_minimum() {
        let mut l = AudioListener::at_origin();
        l.set_head_baseline(-0.1);
        assert!(l.head_baseline > 0.0);
    }

    #[test]
    fn set_gain_clamps_negative() {
        let mut l = AudioListener::at_origin();
        l.set_gain(-1.0);
        assert_eq!(l.gain, 0.0);
    }

    #[test]
    fn rotated_listener_ears_rotate() {
        // Rotate listener 90° to the right (now facing +X). Right axis
        // becomes +Z. Right ear should be at +Z, left ear at -Z.
        let mut l = AudioListener::at_origin();
        l.set_orientation(Vec3::RIGHT, Vec3::UP);
        let le = l.left_ear();
        let re = l.right_ear();
        // (+X) × (+Y) = (+Z) → right axis is +Z.
        assert!((re.z - (STANDARD_HEAD_BASELINE * 0.5)).abs() < 1e-5);
        assert!((le.z - (-STANDARD_HEAD_BASELINE * 0.5)).abs() < 1e-5);
    }
}
