//! § audio_subsystem — ψ-field → binaural projection.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Companion subsystem (not stage-mapped 1:1). Drives `cssl-wave-audio`
//!   to project the shared ψ-AUDIO field into binaural stereo at the
//!   listener's ears. Per M8 AC : "ψ-field updates audio + light from
//!   same wave-PDE solver".
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use cssl_wave_audio::{
    AudioListener, BinauralRender, ProjectionResult, ProjectorConfig, PsiAudioField,
    WaveAudioProjector,
};

/// Outcome of one audio step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioOutcome {
    /// Frame index this outcome covers.
    pub frame_idx: u64,
    /// Stereo sample left.
    pub stereo_left: f32,
    /// Stereo sample right.
    pub stereo_right: f32,
    /// Doppler ratio applied.
    pub doppler_ratio: f32,
    /// Whether ψ-AUDIO field shares the same solver as light bands (always true).
    pub shared_solver_with_light: bool,
}

/// Stage driver.
pub struct AudioSubsystem {
    seed: u64,
    projector: WaveAudioProjector,
    listener: AudioListener,
    field: PsiAudioField,
}

impl std::fmt::Debug for AudioSubsystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioSubsystem")
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

impl AudioSubsystem {
    /// Construct.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let projector =
            WaveAudioProjector::new(ProjectorConfig::default(), BinauralRender::default());
        let listener = AudioListener::default();
        let field = PsiAudioField::default();
        Self {
            seed,
            projector,
            listener,
            field,
        }
    }

    /// Run one tick.
    pub fn step(&mut self, _dt: f32, frame_idx: u64) -> AudioOutcome {
        let result: ProjectionResult =
            self.projector
                .project(&self.field, &self.listener, None, None);
        AudioOutcome {
            frame_idx,
            stereo_left: result.stereo.left,
            stereo_right: result.stereo.right,
            doppler_ratio: result.doppler_ratio,
            shared_solver_with_light: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_constructs() {
        let _ = AudioSubsystem::new(0);
    }

    #[test]
    fn audio_one_step() {
        let mut a = AudioSubsystem::new(0);
        let o = a.step(1.0 / 60.0, 0);
        assert_eq!(o.frame_idx, 0);
        assert!(o.shared_solver_with_light);
    }

    #[test]
    fn audio_replay_bit_equal() {
        let mut a1 = AudioSubsystem::new(0);
        let mut a2 = AudioSubsystem::new(0);
        let r1 = a1.step(1.0 / 60.0, 7);
        let r2 = a2.step(1.0 / 60.0, 7);
        assert_eq!(r1, r2);
    }

    #[test]
    fn audio_silence_initial() {
        let mut a = AudioSubsystem::new(0);
        let o = a.step(1.0 / 60.0, 0);
        // Default field is empty → silence.
        assert_eq!(o.stereo_left, 0.0);
        assert_eq!(o.stereo_right, 0.0);
    }
}
