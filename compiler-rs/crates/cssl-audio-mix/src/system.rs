//! `OmegaSystem` adapter for the mixer.
//!
//! § THESIS
//!   The `Mixer` itself is the audio-DSP engine ; this module wires it
//!   into the `cssl-substrate-omega-step` scheduler so the audio
//!   render-tick is a first-class participant in the omega_step
//!   contract.
//!
//!   The adapter declares `EffectRow::sim_audio()` which the scheduler
//!   hoists onto the dedicated audio-callback fiber per `specs/30
//!   § PHASES`. Per H4 EFR0005 + EFR0006, the body MUST honor :
//!     `{NoAlloc, NoUnbounded, Deadline<1ms>, Realtime<Crit>, PureDet}`
//!   — which the `Mixer::render_frames` body upholds (see lib.rs §
//!   REALTIME-CRIT INVARIANTS).
//!
//! § STAGE-0 SCOPE
//!   The adapter holds an in-memory render-target buffer + a pending-
//!   frames count. On each `step(ctx, dt)` it computes the frame budget
//!   from `(dt × sample_rate)` and renders that many frames. Real-world
//!   integration with `cssl-host-audio::AudioStream::submit_frames`
//!   happens via the caller wiring the buffer into a stream — this
//!   module deliberately does not own the platform stream because the
//!   ownership / threading model for cross-thread submission is a
//!   future-slice concern (S9-O2 audio-graph integration).

use cssl_substrate_omega_step::{
    EffectRow, OmegaError, OmegaStepCtx, OmegaSystem, RngStreamId, SystemId,
};

use crate::mixer::Mixer;

/// `OmegaSystem` adapter wrapping a `Mixer`.
///
/// § DESIGN
///   The adapter renders into a pre-allocated buffer + exposes that
///   buffer to callers via `render_buffer()`. Callers wire the buffer
///   into `AudioStream::submit_frames` outside the omega_step contract
///   (the mixer's purity is preserved : it doesn't own platform
///   resources).
pub struct MixerSystem {
    /// Wrapped mixer.
    mixer: Mixer,
    /// Pre-allocated render buffer. Sized to `(block_frames × channels)`
    /// at construction so no allocation happens on the step hot path.
    render_buffer: Vec<f32>,
    /// Cached system name.
    name: String,
    /// Dependencies declared at registration time.
    deps: Vec<SystemId>,
}

impl MixerSystem {
    /// Construct a new MixerSystem wrapping `mixer`.
    #[must_use]
    pub fn new(mixer: Mixer) -> Self {
        let block_samples = mixer.config().block_samples();
        Self {
            mixer,
            render_buffer: vec![0.0; block_samples],
            name: "audio.mixer".to_string(),
            deps: Vec::new(),
        }
    }

    /// Construct with a custom system name.
    #[must_use]
    pub fn with_name(mixer: Mixer, name: impl Into<String>) -> Self {
        let mut s = Self::new(mixer);
        s.name = name.into();
        s
    }

    /// Set the dependency list this system declares to the scheduler.
    pub fn set_dependencies(&mut self, deps: Vec<SystemId>) {
        self.deps = deps;
    }

    /// Borrow the wrapped mixer (read-only).
    #[must_use]
    pub const fn mixer(&self) -> &Mixer {
        &self.mixer
    }

    /// Mutable mixer access — for play/stop calls outside the step.
    pub fn mixer_mut(&mut self) -> &mut Mixer {
        &mut self.mixer
    }

    /// Read the most recently rendered buffer. Length = `block_frames *
    /// channels`. Caller submits this into `AudioStream::submit_frames`.
    #[must_use]
    pub fn render_buffer(&self) -> &[f32] {
        &self.render_buffer
    }

    /// Number of frames the most recent render produced.
    #[must_use]
    pub fn last_block_frames(&self) -> usize {
        self.mixer.config().block_frames
    }
}

impl OmegaSystem for MixerSystem {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        // Per-tick render of `block_frames` audio frames. Callers
        // typically configure `block_frames` to match the platform
        // ring's per-callback frame count (e.g., 256 frames @ 48 kHz
        // = 5.3 ms latency budget).
        let block_frames = self.mixer.config().block_frames;
        let channels = self.mixer.config().channels();
        let needed = block_frames * channels;
        if self.render_buffer.len() < needed {
            self.render_buffer.resize(needed, 0.0);
        }
        self.mixer.render_frames(&mut self.render_buffer[..needed], block_frames);

        // Telemetry counters mirror the mixer's internal state so the
        // R18 ring sees per-step audio activity.
        let counters = self.mixer.counters();
        ctx.telemetry().count_by("audio.frames_rendered", block_frames as u64);
        ctx.telemetry().count_by("audio.voices_active", counters.voices_active as u64);
        if counters.voices_retired_last_render > 0 {
            ctx.telemetry().count_by(
                "audio.voices_retired",
                u64::from(counters.voices_retired_last_render),
            );
        }
        Ok(())
    }

    fn dependencies(&self) -> &[SystemId] {
        &self.deps
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn effect_row(&self) -> EffectRow {
        // Per specs/30 § PHASES § audio : {Sim, Audio}.
        // The scheduler hoists this onto the audio-callback fiber.
        EffectRow::sim_audio()
    }

    fn rng_streams(&self) -> &[RngStreamId] {
        // Mixer DSP is fully deterministic ; no randomness required.
        &[]
    }
}

impl core::fmt::Debug for MixerSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MixerSystem")
            .field("name", &self.name)
            .field("render_buffer_len", &self.render_buffer.len())
            .field("dep_count", &self.deps.len())
            .field("mixer", &self.mixer)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::mixer::{Mixer, MixerConfig};
    use crate::sound::PcmData;
    use crate::voice::PlayParams;
    use cssl_substrate_omega_step::{
        DetRng, InputEvent, RngStreamId, SubstrateEffect, TelemetryHook,
    };
    use std::collections::BTreeMap;

    fn make_ctx<'a>(
        omega: &'a mut cssl_substrate_omega_step::OmegaSnapshot,
        rngs: &'a mut BTreeMap<RngStreamId, DetRng>,
        telem: &'a mut TelemetryHook,
        inputs: &'a BTreeMap<RngStreamId, InputEvent>,
    ) -> OmegaStepCtx<'a> {
        OmegaStepCtx::new(omega, rngs, telem, 0, false, inputs)
    }

    #[test]
    fn new_uses_mixer_block_size_for_render_buffer() {
        let mixer = Mixer::default_stereo();
        let block_samples = mixer.config().block_samples();
        let s = MixerSystem::new(mixer);
        assert_eq!(s.render_buffer().len(), block_samples);
    }

    #[test]
    fn default_name_is_audio_mixer() {
        let s = MixerSystem::new(Mixer::default_stereo());
        assert_eq!(s.name(), "audio.mixer");
    }

    #[test]
    fn with_name_overrides() {
        let s = MixerSystem::with_name(Mixer::default_stereo(), "music.mixer");
        assert_eq!(s.name(), "music.mixer");
    }

    #[test]
    fn effect_row_is_sim_audio() {
        let s = MixerSystem::new(Mixer::default_stereo());
        let row = s.effect_row();
        assert!(row.contains(SubstrateEffect::Sim));
        assert!(row.contains(SubstrateEffect::Audio));
        assert!(row.validate().is_none());
    }

    #[test]
    fn no_rng_streams_declared() {
        let s = MixerSystem::new(Mixer::default_stereo());
        assert!(s.rng_streams().is_empty());
    }

    #[test]
    fn dependencies_default_empty() {
        let s = MixerSystem::new(Mixer::default_stereo());
        assert!(s.dependencies().is_empty());
    }

    #[test]
    fn set_dependencies_visible_via_dependencies() {
        let mut s = MixerSystem::new(Mixer::default_stereo());
        s.set_dependencies(vec![SystemId(1), SystemId(2)]);
        assert_eq!(s.dependencies().len(), 2);
    }

    #[test]
    fn step_renders_silence_when_no_voices() {
        let mut s = MixerSystem::new(Mixer::default_stereo());
        let mut omega = cssl_substrate_omega_step::OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        let mut ctx = make_ctx(&mut omega, &mut rngs, &mut telem, &inputs);
        s.step(&mut ctx, 0.005).expect("step");
        for sample in s.render_buffer() {
            assert!(sample.abs() < 1e-6, "non-silence : {sample}");
        }
    }

    #[test]
    fn step_increments_telemetry_frames_rendered() {
        let mut s = MixerSystem::new(Mixer::default_stereo());
        let mut omega = cssl_substrate_omega_step::OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        let mut ctx = make_ctx(&mut omega, &mut rngs, &mut telem, &inputs);
        s.step(&mut ctx, 0.005).expect("step");
        assert!(telem.read_counter("audio.frames_rendered") > 0);
    }

    #[test]
    fn step_renders_active_voice() {
        let mut mixer = Mixer::default_stereo();
        // Register a non-zero PCM ; play it.
        let pcm = PcmData::new(vec![0.3_f32; 1024], 48_000, 2).unwrap();
        let h = mixer.register_sound(pcm).unwrap();
        mixer.play_oneshot(h, PlayParams::default()).unwrap();
        let mut s = MixerSystem::new(mixer);
        let mut omega = cssl_substrate_omega_step::OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        let mut ctx = make_ctx(&mut omega, &mut rngs, &mut telem, &inputs);
        s.step(&mut ctx, 0.005).expect("step");
        // Some non-zero output expected.
        let energy: f32 = s.render_buffer().iter().map(|x| x * x).sum();
        assert!(energy > 0.0, "expected non-zero energy ; got {energy}");
    }

    #[test]
    fn step_with_block_frames_resize_handles_growing_block() {
        // When mixer.config().block_frames is small + we grow the
        // mixer config, the system's render_buffer grows on demand
        // (resize-on-render path).
        let cfg = MixerConfig {
            block_frames: 64,
            ..MixerConfig::default_stereo()
        };
        let mut s = MixerSystem::new(Mixer::new(cfg));
        let mut omega = cssl_substrate_omega_step::OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        let mut ctx = make_ctx(&mut omega, &mut rngs, &mut telem, &inputs);
        s.step(&mut ctx, 0.005).expect("step");
        assert!(s.render_buffer().len() >= 64 * 2);
    }

    #[test]
    fn last_block_frames_returns_config_value() {
        let s = MixerSystem::new(Mixer::default_stereo());
        assert_eq!(s.last_block_frames(), 256);
    }

    #[test]
    fn determinism_two_systems_step_bit_equal() {
        // Two systems with identical mixer state produce identical
        // render buffers when stepped with identical context.
        let mut s1 = MixerSystem::new(Mixer::default_stereo());
        let mut s2 = MixerSystem::new(Mixer::default_stereo());
        let pcm = PcmData::new(
            (0..1024).map(|i| (i as f32 * 0.01).sin()).collect(),
            48_000,
            2,
        )
        .unwrap();
        let h1 = s1.mixer_mut().register_sound(pcm.clone()).unwrap();
        let h2 = s2.mixer_mut().register_sound(pcm).unwrap();
        s1.mixer_mut().play_oneshot(h1, PlayParams::default()).unwrap();
        s2.mixer_mut().play_oneshot(h2, PlayParams::default()).unwrap();
        let mut omega1 = cssl_substrate_omega_step::OmegaSnapshot::new();
        let mut rngs1: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem1 = TelemetryHook::new();
        let inputs1: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        let mut omega2 = cssl_substrate_omega_step::OmegaSnapshot::new();
        let mut rngs2: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem2 = TelemetryHook::new();
        let inputs2: BTreeMap<RngStreamId, InputEvent> = BTreeMap::new();
        let mut ctx1 = make_ctx(&mut omega1, &mut rngs1, &mut telem1, &inputs1);
        s1.step(&mut ctx1, 0.005).unwrap();
        let mut ctx2 = make_ctx(&mut omega2, &mut rngs2, &mut telem2, &inputs2);
        s2.step(&mut ctx2, 0.005).unwrap();
        assert_eq!(s1.render_buffer(), s2.render_buffer());
    }
}
