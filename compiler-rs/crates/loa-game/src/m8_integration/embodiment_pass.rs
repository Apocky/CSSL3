//! § embodiment_pass — Stage 1 : XR-input → body-presence-field.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 1 of the canonical 12-stage pipeline. Reads (mock) XR-input pose
//!   data + composes a [`BodyPresenceField`] keyed by Morton-encoded cell
//!   indices. In the M8 vertical-slice this stage is intentionally MOCK
//!   (no real XR-driver wiring) ; the real OpenXR head-pose / hand-tracking
//!   integration lands in M9 via `cssl-host-openxr`.
//!
//! § DETERMINISM
//!   Per-frame body-presence is a pure function of `(seed, frame_idx)` so
//!   the M8 replay-determinism contract holds even for the mock path.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::m8_integration::pipeline::PipelineError;

/// Mocked XR-input snapshot. In M9 this is replaced by real
/// `cssl-host-openxr::FrameLoop` outputs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmbodimentInputs {
    /// Frame index this snapshot covers.
    pub frame_idx: u64,
    /// Mock head-pose translation (meters in world-space).
    pub head_translation: [f32; 3],
    /// Mock head-pose forward-vector (unit).
    pub head_forward: [f32; 3],
    /// Mock left-hand position.
    pub hand_left: [f32; 3],
    /// Mock right-hand position.
    pub hand_right: [f32; 3],
}

impl EmbodimentInputs {
    /// Construct a deterministic default snapshot for the given frame.
    /// The orbital head motion is a slow pure-function of frame_idx so
    /// the test-replay produces a small but non-trivial motion stream.
    #[must_use]
    pub fn deterministic_default(frame_idx: u64) -> Self {
        let phase = (frame_idx as f32) * 0.01;
        Self {
            frame_idx,
            head_translation: [phase.cos() * 0.05, 1.6, phase.sin() * 0.05],
            head_forward: [0.0, 0.0, -1.0],
            hand_left: [-0.25, 1.2, -0.3],
            hand_right: [0.25, 1.2, -0.3],
        }
    }

    /// Hash for determinism comparisons (input-side).
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        // f32 isn't Hash-able — go via to_bits.
        for v in [
            self.head_translation,
            self.head_forward,
            self.hand_left,
            self.hand_right,
        ]
        .iter()
        .flat_map(|v| v.iter())
        {
            v.to_bits().hash(&mut h);
        }
        h.finish()
    }
}

/// Body-presence field — sparse Morton-keyed presence buffer.
///
/// In production the buffer is the SparseMortonGrid of FieldCell values
/// near the player+companion. At M8 vertical-slice it's a small
/// deterministic summary keyed by frame.
#[derive(Debug, Clone)]
pub struct BodyPresenceField {
    /// Frame this field was generated for.
    pub frame_idx: u64,
    /// Cell coordinates (sparse — populated near body).
    pub cells: Vec<[i16; 3]>,
    /// Per-cell AURA-Λ density in [0, 1].
    pub aura_density: Vec<f32>,
    /// Mock SDF handle reference (frame-monotonic id).
    pub sdf_handle: u64,
}

impl BodyPresenceField {
    /// Hash for determinism comparison.
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        for c in &self.cells {
            c.hash(&mut h);
        }
        for a in &self.aura_density {
            a.to_bits().hash(&mut h);
        }
        self.sdf_handle.hash(&mut h);
        h.finish()
    }

    /// Number of populated cells.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
}

/// Stage 1 driver.
#[derive(Debug, Clone, Copy)]
pub struct EmbodimentPass {
    seed: u64,
}

impl EmbodimentPass {
    /// Construct.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Run Stage 1 — synthesize a body-presence field from the XR-input.
    ///
    /// # Errors
    /// Returns [`PipelineError::ViewDimensionMismatch`] reserved for the
    /// future real-XR-extension path. The mock path always succeeds.
    pub fn run(&self, inputs: &EmbodimentInputs) -> Result<BodyPresenceField, PipelineError> {
        // Sample a small 8-cell ring around the head. Determinism is
        // preserved because all positions are derived from `frame_idx`.
        let mut cells = Vec::with_capacity(8);
        let mut aura = Vec::with_capacity(8);
        let head_x = (inputs.head_translation[0] / 0.5) as i16;
        let head_y = (inputs.head_translation[1] / 0.5) as i16;
        let head_z = (inputs.head_translation[2] / 0.5) as i16;
        for dx in -1..=1_i16 {
            for dz in -1..=1_i16 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                cells.push([head_x + dx, head_y, head_z + dz]);
                let r = ((dx * dx + dz * dz) as f32).sqrt();
                aura.push((1.0 / (1.0 + r)).clamp(0.0, 1.0));
            }
        }
        let sdf_handle = self.seed.wrapping_add(inputs.frame_idx);
        Ok(BodyPresenceField {
            frame_idx: inputs.frame_idx,
            cells,
            aura_density: aura,
            sdf_handle,
        })
    }

    /// Master seed.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embodiment_inputs_deterministic_per_frame() {
        let a = EmbodimentInputs::deterministic_default(7);
        let b = EmbodimentInputs::deterministic_default(7);
        assert_eq!(a, b);
    }

    #[test]
    fn embodiment_inputs_different_frames_differ() {
        let a = EmbodimentInputs::deterministic_default(7);
        let b = EmbodimentInputs::deterministic_default(8);
        assert_ne!(a, b);
    }

    #[test]
    fn embodiment_pass_produces_body_field() {
        let p = EmbodimentPass::new(0xCAFE_BABE);
        let inp = EmbodimentInputs::deterministic_default(0);
        let f = p.run(&inp).unwrap();
        assert_eq!(f.frame_idx, 0);
        assert_eq!(f.cell_count(), 8);
    }

    #[test]
    fn embodiment_pass_replay_bit_equal() {
        let p1 = EmbodimentPass::new(0xCAFE_BABE);
        let p2 = EmbodimentPass::new(0xCAFE_BABE);
        let inp = EmbodimentInputs::deterministic_default(11);
        let a = p1.run(&inp).unwrap();
        let b = p2.run(&inp).unwrap();
        assert_eq!(a.determinism_hash(), b.determinism_hash());
    }

    #[test]
    fn different_seeds_produce_different_sdf_handles() {
        let p1 = EmbodimentPass::new(1);
        let p2 = EmbodimentPass::new(2);
        let inp = EmbodimentInputs::deterministic_default(5);
        let a = p1.run(&inp).unwrap();
        let b = p2.run(&inp).unwrap();
        assert_ne!(a.sdf_handle, b.sdf_handle);
    }
}
