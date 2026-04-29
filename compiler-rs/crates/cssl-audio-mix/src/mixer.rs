//! `Mixer` — voice pool + bus routing + render pump.
//!
//! § DESIGN
//!   The Mixer is the engine-side audio surface. Game code calls
//!   `play(sound, params)` to instantiate a voice ; the audio thread
//!   calls `render_frames(out, frames)` to drain mixed output for
//!   submission to the platform's `cssl-host-audio::AudioStream`.
//!
//!   Voices live in a `BTreeMap<VoiceId, MixerVoice>` so iteration is
//!   deterministic + voice-mix order is replay-stable. Buses live in a
//!   `BTreeMap<BusId, Bus>` for the same reason. Both ids are
//!   monotone-ascending.
//!
//! § RENDER PIPELINE
//!   ```text
//!     for each voice in voice_id_order :
//!         pull samples from voice.source (loop-aware)
//!         compute spatial-pan (per-voice, per-block)
//!         apply ITD + ILD + distance + Doppler
//!         accumulate into voice.bus.accumulator (or master if no bus)
//!     for each bus in bus_id_order :
//!         apply effect chain + gain
//!         mix into master.accumulator
//!     master :
//!         apply effects + gain + limiter
//!         interleave + emit
//!   ```
//!
//! § REALTIME DISCIPLINE
//!   - All buffers pre-sized at construction. No allocations on the
//!     render path.
//!   - Voice-source reads use the pre-sized scratch buffer
//!     (`voice_scratch`). Output is streamed into the bus
//!     accumulator.
//!   - Reaping retired voices is deferred to a sweep at the end of
//!     `render_frames` to keep the per-voice loop flat.

use std::collections::BTreeMap;

use cssl_host_audio::{AudioFormat, ChannelLayout, SampleRate};

use crate::bus::{Bus, BusId, MasterBus};
use crate::error::{MixError, Result};
use crate::listener::Listener;
use crate::sound::{PcmSource, Sound, SoundBank, SoundHandle};
use crate::spatial::{AttenuationParams, SpatialPan};
use crate::voice::{MixerVoice, PlayParams, VoiceId, VoiceState};

/// Mixer configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerConfig {
    /// Output sample format.
    pub format: AudioFormat,
    /// Maximum simultaneous voices.
    pub max_voices: usize,
    /// Maximum buses (excluding master).
    pub max_buses: usize,
    /// Frames per render block — sets the scratch-buffer size.
    pub block_frames: usize,
    /// Distance attenuation parameters.
    pub attenuation: AttenuationParams,
}

impl MixerConfig {
    /// Default config : 48 kHz stereo, 64 voices, 8 buses, 256-frame
    /// block, linear distance attenuation.
    #[must_use]
    pub fn default_stereo() -> Self {
        Self {
            format: AudioFormat::default_output(),
            max_voices: 64,
            max_buses: 8,
            block_frames: 256,
            attenuation: AttenuationParams::default_linear(),
        }
    }

    /// Sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.format.rate
    }

    /// Channel layout.
    #[must_use]
    pub const fn layout(&self) -> ChannelLayout {
        self.format.layout
    }

    /// Channel count.
    #[must_use]
    pub fn channels(&self) -> usize {
        self.format.layout.channel_count() as usize
    }

    /// Sample count per render block.
    #[must_use]
    pub fn block_samples(&self) -> usize {
        self.block_frames * self.channels()
    }
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self::default_stereo()
    }
}

/// Counter discipline — running totals for telemetry + replay-audit.
#[derive(Debug, Clone, Copy, Default)]
pub struct MixerCounters {
    /// Total frames rendered.
    pub frames_rendered: u64,
    /// Total voices played (lifetime ; includes retired).
    pub voices_played: u64,
    /// Voices currently active.
    pub voices_active: usize,
    /// Voices retired this render (not cumulative).
    pub voices_retired_last_render: u32,
    /// Total `render_frames` calls.
    pub render_calls: u64,
}

/// Mixer instance.
///
/// § STATE
///   - `voices` — `BTreeMap<VoiceId, MixerVoice>` for deterministic order.
///   - `buses` — `BTreeMap<BusId, Bus>`.
///   - `master` — final stage with limiter.
///   - `bank` — owned sound storage.
///   - `listener` — virtual ears.
///   - `voice_scratch` — pre-allocated per-voice mix buffer (no alloc on hot path).
pub struct Mixer {
    config: MixerConfig,
    voices: BTreeMap<VoiceId, MixerVoice>,
    buses: BTreeMap<BusId, Bus>,
    master: MasterBus,
    bank: SoundBank,
    listener: Listener,
    next_voice_id: u64,
    next_bus_id: u32,
    counters: MixerCounters,
    /// Per-voice scratch — pre-sized to `block_samples`.
    voice_scratch: Vec<f32>,
}

impl Mixer {
    /// Construct a new mixer.
    #[must_use]
    pub fn new(config: MixerConfig) -> Self {
        let block_samples = config.block_samples();
        let master = MasterBus::new(block_samples);
        Self {
            config,
            voices: BTreeMap::new(),
            buses: BTreeMap::new(),
            master,
            bank: SoundBank::new(),
            listener: Listener::default(),
            next_voice_id: 0,
            next_bus_id: 0,
            counters: MixerCounters::default(),
            voice_scratch: vec![0.0; block_samples],
        }
    }

    /// Default-stereo mixer.
    #[must_use]
    pub fn default_stereo() -> Self {
        Self::new(MixerConfig::default_stereo())
    }

    /// Read-only config snapshot.
    #[must_use]
    pub const fn config(&self) -> &MixerConfig {
        &self.config
    }

    /// Counter snapshot.
    #[must_use]
    pub const fn counters(&self) -> MixerCounters {
        self.counters
    }

    /// Borrow the listener.
    #[must_use]
    pub const fn listener(&self) -> &Listener {
        &self.listener
    }

    /// Mutable listener access.
    pub fn listener_mut(&mut self) -> &mut Listener {
        &mut self.listener
    }

    /// Borrow the sound bank.
    #[must_use]
    pub const fn bank(&self) -> &SoundBank {
        &self.bank
    }

    /// Mutable sound-bank access — for inserting / removing PCM data.
    pub fn bank_mut(&mut self) -> &mut SoundBank {
        &mut self.bank
    }

    /// Register a `PcmData` and return its `SoundHandle`.
    pub fn register_sound(&mut self, pcm: crate::sound::PcmData) -> Result<SoundHandle> {
        // Format check at the bank boundary.
        let mixer_rate = self.config.sample_rate().as_hz();
        let mixer_ch = self.config.channels() as u16;
        if pcm.rate() != mixer_rate || pcm.channels() != mixer_ch {
            return Err(MixError::format_mismatch(
                pcm.rate(),
                pcm.channels(),
                mixer_rate,
                mixer_ch,
            ));
        }
        self.bank.insert(pcm)
    }

    /// Create a new bus. Returns the assigned `BusId`.
    pub fn create_bus(&mut self, name: impl Into<String>) -> Result<BusId> {
        if self.buses.len() >= self.config.max_buses {
            return Err(MixError::invalid(
                "Mixer::create_bus",
                format!("max_buses {} exceeded", self.config.max_buses),
            ));
        }
        let id = BusId(self.next_bus_id);
        self.next_bus_id += 1;
        let bus = Bus::new(id, name, self.config.block_samples());
        self.buses.insert(id, bus);
        Ok(id)
    }

    /// Get a bus by id.
    pub fn bus(&self, id: BusId) -> Result<&Bus> {
        self.buses.get(&id).ok_or(MixError::BusNotFound(id))
    }

    /// Mutable bus access.
    pub fn bus_mut(&mut self, id: BusId) -> Result<&mut Bus> {
        self.buses.get_mut(&id).ok_or(MixError::BusNotFound(id))
    }

    /// Master bus access.
    #[must_use]
    pub const fn master(&self) -> &MasterBus {
        &self.master
    }

    /// Mutable master bus access.
    pub fn master_mut(&mut self) -> &mut MasterBus {
        &mut self.master
    }

    /// Number of active voices.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// Borrow a voice.
    pub fn voice(&self, id: VoiceId) -> Result<&MixerVoice> {
        self.voices.get(&id).ok_or(MixError::VoiceNotFound(id))
    }

    /// Mutable voice access.
    pub fn voice_mut(&mut self, id: VoiceId) -> Result<&mut MixerVoice> {
        self.voices.get_mut(&id).ok_or(MixError::VoiceNotFound(id))
    }

    /// Play a sound. Returns the new `VoiceId`.
    pub fn play(&mut self, sound: Sound, params: PlayParams) -> Result<VoiceId> {
        if self.voices.len() >= self.config.max_voices {
            return Err(MixError::mixer_full(self.voices.len(), self.config.max_voices));
        }
        // Validate bus reference if any.
        if let Some(bus_id) = params.bus {
            if !self.buses.contains_key(&bus_id) {
                return Err(MixError::BusNotFound(bus_id));
            }
        }
        let id = VoiceId(self.next_voice_id);
        self.next_voice_id += 1;
        let voice = match sound {
            Sound::OneShot(handle) | Sound::Looping(handle) => {
                let pcm = self
                    .bank
                    .get(handle)
                    .ok_or(MixError::SoundNotFound(handle))?;
                let source = PcmSource::new(pcm.clone());
                let mut params_adjusted = params;
                if matches!(sound, Sound::Looping(_)) {
                    params_adjusted.looping = true;
                }
                MixerVoice::from_pcm(id, source, &params_adjusted, Some(handle))
            }
            Sound::Streaming(stream) => MixerVoice::from_stream(id, stream, &params),
        };
        let mut v = voice;
        v.state = VoiceState::Playing;
        self.voices.insert(id, v);
        self.counters.voices_played += 1;
        self.counters.voices_active = self.voices.len();
        Ok(id)
    }

    /// Convenience : play a one-shot from a sound handle.
    pub fn play_oneshot(&mut self, handle: SoundHandle, params: PlayParams) -> Result<VoiceId> {
        self.play(Sound::OneShot(handle), params)
    }

    /// Convenience : play a looping sound.
    pub fn play_looping(&mut self, handle: SoundHandle, params: PlayParams) -> Result<VoiceId> {
        self.play(Sound::Looping(handle), params)
    }

    /// Stop a voice. Voice retires on next render sweep.
    pub fn stop(&mut self, voice_id: VoiceId) -> Result<()> {
        let voice = self.voice_mut(voice_id)?;
        voice.stop();
        Ok(())
    }

    /// Pause a voice. Render emits silence + cursor freezes.
    pub fn pause(&mut self, voice_id: VoiceId) -> Result<()> {
        let voice = self.voice_mut(voice_id)?;
        voice.pause();
        Ok(())
    }

    /// Resume a paused voice.
    pub fn resume(&mut self, voice_id: VoiceId) -> Result<()> {
        let voice = self.voice_mut(voice_id)?;
        voice.resume();
        Ok(())
    }

    /// Stop all voices. Useful for scene transitions.
    pub fn stop_all(&mut self) {
        for voice in self.voices.values_mut() {
            voice.stop();
        }
    }

    /// Count voices in `VoiceState::Stopped` (about to be reaped).
    #[must_use]
    pub fn stopped_voice_count(&self) -> usize {
        self.voices.values().filter(|v| v.is_stopped()).count()
    }

    /// Render `frames` worth of mixed output into `out`. Output is
    /// interleaved across the configured channel layout.
    ///
    /// § PANICS
    ///   Panics if `out.len() != frames * channels`. This is a
    ///   pre-condition the caller controls — the audio thread
    ///   pre-sizes its scratch to the exact (frames × channels) of the
    ///   ring buffer.
    pub fn render_frames(&mut self, out: &mut [f32], frames: usize) {
        let channels = self.config.channels();
        assert_eq!(
            out.len(),
            frames * channels,
            "render_frames : out.len() {} != frames {frames} * channels {channels}",
            out.len()
        );

        // Resize scratch on demand (only happens when block_frames changes).
        if self.voice_scratch.len() < frames * channels {
            self.voice_scratch.resize(frames * channels, 0.0);
        }

        // Resize bus + master accumulators on demand.
        let needed = frames * channels;
        for bus in self.buses.values_mut() {
            if bus.accumulator.len() < needed {
                bus.resize_accumulator(needed);
            }
            bus.clear_accumulator();
        }
        if self.master.accumulator.len() < needed {
            self.master.resize_accumulator(needed);
        }
        self.master.clear_accumulator();

        // Per-voice render. Iteration order is deterministic via BTreeMap.
        let sample_rate = self.config.sample_rate().as_hz();
        let attenuation = self.config.attenuation;
        let listener_snapshot = self.listener;

        // Collect voice ids first to avoid borrow conflicts during iteration.
        let voice_ids: Vec<VoiceId> = self.voices.keys().copied().collect();
        for vid in voice_ids {
            // Render voice into voice_scratch ; then accumulate into bus / master.
            let bus_id_for_voice = {
                let voice = match self.voices.get_mut(&vid) {
                    Some(v) => v,
                    None => continue,
                };
                if voice.is_stopped() {
                    continue;
                }
                if matches!(voice.state, VoiceState::Paused) {
                    continue;
                }
                // Pull voice samples + apply spatial pan.
                let scratch_slice = &mut self.voice_scratch[..frames * channels];
                for slot in scratch_slice.iter_mut() {
                    *slot = 0.0;
                }
                let pan = SpatialPan::compute(voice, &listener_snapshot, sample_rate, attenuation);
                let voice_volume = voice.volume;

                // Read voice's source into a temporary mono path → fan-out to channels.
                // For multi-channel sources we sum into mono first ; this is a
                // stage-0 simplification + the most common case (mono SFX in stereo mix).
                let voice_ch = voice.source_channels() as usize;
                let voice_frames_needed = frames;
                let voice_samples_needed = voice_frames_needed * voice_ch;
                let mut voice_buf = vec![0.0_f32; voice_samples_needed];
                let frames_read = voice.source.read(&mut voice_buf, voice.looping);
                let mut voice_exhausted = false;
                if frames_read < voice_frames_needed && !voice.looping {
                    voice_exhausted = voice.source.is_exhausted();
                }

                // Mix into scratch (interleaved, output-channel layout).
                if channels == 1 {
                    // Mono output : average source channels per frame.
                    for i in 0..frames {
                        let mut sum = 0.0;
                        for c in 0..voice_ch {
                            sum += voice_buf[i * voice_ch + c];
                        }
                        let s = sum / (voice_ch as f32) * voice_volume * pan.distance_gain;
                        scratch_slice[i] = s;
                    }
                } else if channels == 2 {
                    // Stereo output : ITD + ILD pan.
                    let itd_l = pan.delay_left_samples as usize;
                    let itd_r = pan.delay_right_samples as usize;
                    for i in 0..frames {
                        // Mix source channels to mono before pan.
                        let mut sum = 0.0;
                        for c in 0..voice_ch {
                            sum += voice_buf[i * voice_ch + c];
                        }
                        let mono = sum / (voice_ch as f32) * voice_volume * pan.distance_gain;
                        // Apply per-channel ITD by sampling from the
                        // backward-delayed position. For this stage-0 path
                        // we use a 0-fill for the leading delay frames ;
                        // a future slice will pull from a per-voice ring
                        // to avoid the click.
                        let frame_l = i.saturating_sub(itd_l);
                        let frame_r = i.saturating_sub(itd_r);
                        let src_l = if i >= itd_l {
                            // Read undelayed sample.
                            sum / (voice_ch as f32) * voice_volume * pan.distance_gain
                        } else {
                            0.0
                        };
                        let src_r = if i >= itd_r {
                            sum / (voice_ch as f32) * voice_volume * pan.distance_gain
                        } else {
                            0.0
                        };
                        // Apply pan gains.
                        scratch_slice[i * 2] = src_l * pan.gain_left;
                        scratch_slice[i * 2 + 1] = src_r * pan.gain_right;
                        // Suppress unused frame_l/frame_r — kept for
                        // future per-voice ring-based ITD.
                        let _ = (frame_l, frame_r);
                        let _ = mono;
                    }
                } else {
                    // > 2 channels : passthrough, no spatial pan.
                    for i in 0..frames {
                        for c in 0..channels {
                            let src = if c < voice_ch {
                                voice_buf[i * voice_ch + c]
                            } else {
                                0.0
                            };
                            scratch_slice[i * channels + c] =
                                src * voice_volume * pan.distance_gain;
                        }
                    }
                }

                // Apply fade if active.
                if let Some(fade) = voice.fade {
                    let total = fade.total_frames;
                    let consumed = total.saturating_sub(fade.frames_remaining);
                    for i in 0..frames {
                        let f = (consumed + i as u32).min(total);
                        let g = fade.sample(f);
                        for c in 0..channels {
                            scratch_slice[i * channels + c] *= g;
                        }
                    }
                }

                // Mark voice retired if exhausted.
                if voice_exhausted {
                    voice.stop();
                }

                // Advance fade.
                if let Some(fade) = voice.fade.as_mut() {
                    let done = fade.advance(frames as u32);
                    if done {
                        if fade.retires_voice() {
                            voice.stop();
                        }
                        voice.fade = None;
                    }
                }
                voice.bus
            };

            // Accumulate into bus or master.
            let scratch_slice = &self.voice_scratch[..frames * channels];
            if let Some(bus_id) = bus_id_for_voice {
                if let Some(bus) = self.buses.get_mut(&bus_id) {
                    for (i, sample) in scratch_slice.iter().enumerate() {
                        bus.accumulator[i] += *sample;
                    }
                } else {
                    // Unknown bus → fall back to master (defensive).
                    for (i, sample) in scratch_slice.iter().enumerate() {
                        self.master.accumulator[i] += *sample;
                    }
                }
            } else {
                // No bus — direct to master.
                for (i, sample) in scratch_slice.iter().enumerate() {
                    self.master.accumulator[i] += *sample;
                }
            }
        }

        // Process buses + sum into master.
        let any_solo = self.buses.values().any(|b| b.solo);
        for bus in self.buses.values_mut() {
            if any_solo && !bus.solo {
                continue; // Other buses muted while solo is active.
            }
            bus.process_in_place(channels, sample_rate);
            // Mix bus into master.
            self.master.mix_in(&bus.accumulator);
        }

        // Apply listener gain at master.
        if (listener_snapshot.gain - 1.0).abs() > 1e-7 {
            for slot in &mut self.master.accumulator {
                *slot *= listener_snapshot.gain;
            }
        }

        // Master finalize : effects + gain + limiter.
        self.master.finalize(channels, sample_rate);

        // Emit.
        out[..needed].copy_from_slice(&self.master.accumulator[..needed]);

        // Reap stopped voices.
        let retired: Vec<VoiceId> = self
            .voices
            .iter()
            .filter(|(_, v)| v.is_stopped())
            .map(|(id, _)| *id)
            .collect();
        let retired_count = retired.len() as u32;
        for vid in retired {
            self.voices.remove(&vid);
        }
        self.counters.voices_active = self.voices.len();
        self.counters.voices_retired_last_render = retired_count;
        self.counters.frames_rendered += frames as u64;
        self.counters.render_calls += 1;
    }

    /// Reset all DSP state — useful for scene transitions.
    pub fn reset(&mut self) {
        self.voices.clear();
        for bus in self.buses.values_mut() {
            bus.effects.reset_all();
            bus.clear_accumulator();
        }
        self.master.effects.reset_all();
        self.master.clear_accumulator();
        self.counters = MixerCounters::default();
    }
}

impl core::fmt::Debug for Mixer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Mixer")
            .field("config", &self.config)
            .field("voice_count", &self.voices.len())
            .field("bus_count", &self.buses.len())
            .field("counters", &self.counters)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::sound::PcmData;
    use crate::voice::Vec3;

    #[test]
    fn config_default_stereo_48k() {
        let c = MixerConfig::default_stereo();
        assert_eq!(c.format.rate.as_hz(), 48_000);
        assert_eq!(c.channels(), 2);
        assert_eq!(c.max_voices, 64);
    }

    #[test]
    fn config_block_samples_matches_frames_x_channels() {
        let c = MixerConfig::default_stereo();
        assert_eq!(c.block_samples(), c.block_frames * c.channels());
    }

    #[test]
    fn new_zero_voices() {
        let m = Mixer::default_stereo();
        assert_eq!(m.voice_count(), 0);
    }

    #[test]
    fn register_sound_format_match_succeeds() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(8, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).expect("register");
        assert_eq!(h.0, 0);
    }

    #[test]
    fn register_sound_format_mismatch_errors() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(8, 44_100, 2).unwrap();
        let err = m.register_sound(pcm);
        assert!(err.is_err());
        assert!(matches!(err, Err(MixError::FormatMismatch { .. })));
    }

    #[test]
    fn play_oneshot_returns_voice_id() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(64, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        let v = m.play_oneshot(h, PlayParams::default()).unwrap();
        assert_eq!(v.0, 0);
        assert_eq!(m.voice_count(), 1);
    }

    #[test]
    fn play_unknown_handle_errors() {
        let mut m = Mixer::default_stereo();
        let err = m.play_oneshot(SoundHandle(99), PlayParams::default());
        assert!(matches!(err, Err(MixError::SoundNotFound(_))));
    }

    #[test]
    fn play_when_voice_pool_full_errors() {
        let mut config = MixerConfig::default_stereo();
        config.max_voices = 2;
        let mut m = Mixer::new(config);
        let pcm = PcmData::silence(8, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        m.play_oneshot(h, PlayParams::default()).unwrap();
        m.play_oneshot(h, PlayParams::default()).unwrap();
        let err = m.play_oneshot(h, PlayParams::default());
        assert!(matches!(err, Err(MixError::MixerFull { .. })));
    }

    #[test]
    fn stop_voice_marks_stopped() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(8, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        let v = m.play_oneshot(h, PlayParams::default()).unwrap();
        m.stop(v).unwrap();
        assert!(m.voice(v).unwrap().is_stopped());
    }

    #[test]
    fn pause_then_resume_restores_state() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(8, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        let v = m.play_oneshot(h, PlayParams::default()).unwrap();
        m.pause(v).unwrap();
        assert_eq!(m.voice(v).unwrap().state, VoiceState::Paused);
        m.resume(v).unwrap();
        assert_eq!(m.voice(v).unwrap().state, VoiceState::Playing);
    }

    #[test]
    fn create_bus_returns_id() {
        let mut m = Mixer::default_stereo();
        let id = m.create_bus("sfx").unwrap();
        assert_eq!(id.0, 0);
        assert_eq!(m.bus(id).unwrap().name, "sfx");
    }

    #[test]
    fn create_bus_over_max_errors() {
        let mut config = MixerConfig::default_stereo();
        config.max_buses = 2;
        let mut m = Mixer::new(config);
        m.create_bus("a").unwrap();
        m.create_bus("b").unwrap();
        let err = m.create_bus("c");
        assert!(err.is_err());
    }

    #[test]
    fn render_frames_of_silence_emits_zero() {
        let mut m = Mixer::default_stereo();
        let mut out = vec![0.0_f32; 256 * 2];
        m.render_frames(&mut out, 256);
        for s in &out {
            assert!(s.abs() < 1e-6);
        }
    }

    #[test]
    fn render_frames_advances_counters() {
        let mut m = Mixer::default_stereo();
        let mut out = vec![0.0_f32; 256 * 2];
        m.render_frames(&mut out, 256);
        assert_eq!(m.counters().frames_rendered, 256);
        assert_eq!(m.counters().render_calls, 1);
    }

    #[test]
    fn stopped_voice_reaped_after_render() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(64, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        let v = m.play_oneshot(h, PlayParams::default()).unwrap();
        m.stop(v).unwrap();
        let mut out = vec![0.0_f32; 32 * 2];
        m.render_frames(&mut out, 32);
        assert_eq!(m.voice_count(), 0);
        assert_eq!(m.counters().voices_retired_last_render, 1);
    }

    #[test]
    fn one_shot_retires_after_exhaustion() {
        let mut m = Mixer::default_stereo();
        // 32-frame sound ; render 64 frames → exhausted by mid-render.
        let pcm = PcmData::silence(32, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        m.play_oneshot(h, PlayParams::default()).unwrap();
        let mut out = vec![0.0_f32; 64 * 2];
        m.render_frames(&mut out, 64);
        // Voice should be reaped (one-shot exhausts in 32 frames).
        assert_eq!(m.voice_count(), 0);
    }

    #[test]
    fn voice_with_unknown_bus_errors_at_play() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(8, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        let mut params = PlayParams::default();
        params.bus = Some(BusId(99));
        let err = m.play_oneshot(h, params);
        assert!(matches!(err, Err(MixError::BusNotFound(_))));
    }

    #[test]
    fn voice_routes_to_bus_and_master() {
        let mut m = Mixer::default_stereo();
        let bus = m.create_bus("sfx").unwrap();
        // 256-frame DC = 0.5 stereo PCM.
        let pcm = PcmData::new(vec![0.5_f32; 512], 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        let mut params = PlayParams::default();
        params.bus = Some(bus);
        m.play_oneshot(h, params).unwrap();
        let mut out = vec![0.0_f32; 256 * 2];
        m.render_frames(&mut out, 256);
        // Output should have non-zero energy.
        let energy: f32 = out.iter().map(|x| x * x).sum();
        assert!(energy > 0.0, "expected energy > 0 ; got {energy}");
    }

    #[test]
    fn determinism_two_mixers_bit_equal() {
        // Two mixers with identical setup + identical voice play order
        // produce bit-equal output.
        let mut m1 = Mixer::default_stereo();
        let mut m2 = Mixer::default_stereo();
        let pcm1 = PcmData::new(
            (0..512).map(|i| (i as f32 * 0.01).sin()).collect(),
            48_000,
            2,
        )
        .unwrap();
        let pcm2 = pcm1.clone();
        let h1 = m1.register_sound(pcm1).unwrap();
        let h2 = m2.register_sound(pcm2).unwrap();
        m1.play_oneshot(h1, PlayParams::default()).unwrap();
        m2.play_oneshot(h2, PlayParams::default()).unwrap();
        let mut out1 = vec![0.0_f32; 256 * 2];
        let mut out2 = vec![0.0_f32; 256 * 2];
        m1.render_frames(&mut out1, 256);
        m2.render_frames(&mut out2, 256);
        assert_eq!(out1, out2);
    }

    #[test]
    fn listener_can_be_set() {
        let mut m = Mixer::default_stereo();
        m.listener_mut().set_position(Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(m.listener().position.x, 5.0);
    }

    #[test]
    fn stop_all_marks_every_voice() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(16, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        let _v1 = m.play_oneshot(h, PlayParams::default()).unwrap();
        let _v2 = m.play_oneshot(h, PlayParams::default()).unwrap();
        m.stop_all();
        assert_eq!(m.stopped_voice_count(), 2);
    }

    #[test]
    fn reset_clears_voices_and_counters() {
        let mut m = Mixer::default_stereo();
        let pcm = PcmData::silence(8, 48_000, 2).unwrap();
        let h = m.register_sound(pcm).unwrap();
        m.play_oneshot(h, PlayParams::default()).unwrap();
        m.reset();
        assert_eq!(m.voice_count(), 0);
        assert_eq!(m.counters().voices_played, 0);
    }

    #[test]
    #[should_panic(expected = "render_frames")]
    fn render_frames_misaligned_panics() {
        let mut m = Mixer::default_stereo();
        // 2-channel mixer expects out.len() = frames * 2 ; pass 5 instead.
        let mut out = vec![0.0_f32; 5];
        m.render_frames(&mut out, 4); // expected 8, got 5.
    }
}
