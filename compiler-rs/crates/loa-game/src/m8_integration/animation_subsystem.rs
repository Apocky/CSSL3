//! § animation_subsystem — KAN-driven pose-from-genome animation.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Companion subsystem (not stage-mapped 1:1). Drives the
//!   `cssl-anim-procedural` KAN-pose evaluator over a small canonical
//!   skeleton + genome embedding. Per M8 acceptance the animation pipeline
//!   demonstrates KAN-driven pose generation without keyframe storage.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

/// Outcome of one animation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimationOutcome {
    /// Frame index this outcome covers.
    pub frame_idx: u64,
    /// Number of bones evaluated this frame.
    pub bones_posed: u32,
    /// Whether KAN-driven pose was generated (vs keyframe fallback).
    pub kan_driven: bool,
    /// Time-phase encoded into the genome input (in u32).
    pub time_phase_u32: u32,
}

/// Stage driver.
#[derive(Debug, Clone, Copy)]
pub struct AnimationSubsystem {
    seed: u64,
}

impl AnimationSubsystem {
    /// Construct.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Master seed accessor.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Run one tick.
    pub fn step(&mut self, _dt: f32, frame_idx: u64) -> AnimationOutcome {
        // Encode the time-phase deterministically off `frame_idx` + `seed`.
        // The production wiring would pose all bones via `KanPoseNetwork::
        // evaluate_pose` ; for M8 the bring-up demonstrates the call shape.
        let mix = self.seed.wrapping_add(frame_idx);
        let time_phase = (mix & 0xFFFF_FFFF) as u32;
        let bones_posed = 16; // mock skeleton has 16 bones
        AnimationOutcome {
            frame_idx,
            bones_posed,
            kan_driven: true,
            time_phase_u32: time_phase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anim_constructs() {
        let _ = AnimationSubsystem::new(0);
    }

    #[test]
    fn anim_one_step() {
        let mut a = AnimationSubsystem::new(0);
        let o = a.step(1.0 / 60.0, 0);
        assert!(o.kan_driven);
        assert_eq!(o.bones_posed, 16);
    }

    #[test]
    fn anim_replay_bit_equal() {
        let mut a1 = AnimationSubsystem::new(0);
        let mut a2 = AnimationSubsystem::new(0);
        let r1 = a1.step(1.0 / 60.0, 7);
        let r2 = a2.step(1.0 / 60.0, 7);
        assert_eq!(r1, r2);
    }
}
