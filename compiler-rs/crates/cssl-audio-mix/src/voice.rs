//! Per-voice playback state.
//!
//! § DESIGN
//!   A `MixerVoice` is a single playback instance — sound + position +
//!   velocity + volume + fade. The mixer holds a `BTreeMap<VoiceId,
//!   MixerVoice>` so iteration is deterministic + voice ids are stable
//!   across calls.
//!
//!   Voices are created via `Mixer::play(sound, params)` ; `params`
//!   captures the per-instance settings the caller wants to vary
//!   (position, volume, pitch, fade-in).
//!
//! § VOICE LIFECYCLE
//!   ```text
//!     Idle    — newly-allocated, awaiting first render
//!     Playing — render emits samples each tick
//!     Fading  — volume ramping toward target ; on completion -> Playing
//!                or Stopped depending on the fade target
//!     Paused  — render emits silence ; sample cursor frozen
//!     Stopped — terminal ; mixer reaps on next sweep
//!   ```
//!
//! § DETERMINISM
//!   `VoiceId` is monotone-ascending. The mixer iterates voices by id,
//!   so two replays produce bit-equal mix output given identical voice
//!   creation order.

use crate::sound::{PcmSource, SoundHandle};

/// 3D vector — position / velocity / orientation. Stage-0 uses bare
/// `f32` triples ; future slices may swap in cssl-substrate's
/// math vector when M1 lands.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    /// X axis (right).
    pub x: f32,
    /// Y axis (up).
    pub y: f32,
    /// Z axis (forward — listener-relative + 0 = at listener).
    pub z: f32,
}

impl Vec3 {
    /// Origin (0,0,0).
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Forward unit vector (0, 0, -1) using right-handed convention.
    pub const FORWARD: Self = Self {
        x: 0.0,
        y: 0.0,
        z: -1.0,
    };

    /// Up unit vector (0, 1, 0).
    pub const UP: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    /// Construct.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    /// Vector difference (`self - rhs`).
    #[must_use]
    pub fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }

    /// Vector addition.
    #[must_use]
    pub fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }

    /// Scalar multiplication.
    #[must_use]
    pub fn scale(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }

    /// Squared magnitude.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Euclidean magnitude.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Normalize ; returns `Vec3::ZERO` when length ≤ epsilon to avoid
    /// NaN propagation onto the audio thread (PRIME-DIRECTIVE : every
    /// failure mode is explicit).
    #[must_use]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len < 1e-12 {
            return Self::ZERO;
        }
        self.scale(1.0 / len)
    }

    /// Cross product (right-handed).
    #[must_use]
    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }
}

/// Stable identifier for a registered voice. Issued in monotone-ascending
/// order by `Mixer::play()`. Stable for the voice's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(pub u64);

/// Voice runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    /// Newly allocated, awaiting first render.
    Idle,
    /// Producing samples.
    Playing,
    /// Volume ramping toward target.
    Fading,
    /// Sample cursor frozen ; render emits silence.
    Paused,
    /// Terminal — reaped on next sweep.
    Stopped,
}

impl VoiceState {
    /// Whether the voice is contributing samples this frame.
    #[must_use]
    pub const fn is_audible(self) -> bool {
        matches!(self, Self::Playing | Self::Fading)
    }
}

/// Fade direction + sample-count bookkeeping.
///
/// § STAGE-0 FORM
///   Linear ramp from `start_volume` to `target_volume` over
///   `total_frames` frames. `frames_remaining` counts down each render.
///   Fade-in (start = 0, target = 1) and fade-out (start = 1, target = 0)
///   are the canonical forms ; arbitrary start/target values are
///   permitted for cross-fades + ducking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fade {
    /// Initial volume at start of fade.
    pub start_volume: f32,
    /// Target volume at end of fade.
    pub target_volume: f32,
    /// Total frames over which the ramp occurs.
    pub total_frames: u32,
    /// Frames remaining until ramp completes.
    pub frames_remaining: u32,
    /// What to do when the ramp finishes — keep playing or stop.
    pub on_complete: FadeMode,
}

/// What to do when a fade ramp completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FadeMode {
    /// Continue playing after the ramp finishes.
    Hold,
    /// Stop the voice when the ramp finishes (fade-out + retire).
    StopOnComplete,
}

impl Fade {
    /// Construct a fade-in ramp from 0 to `target` over `frames`.
    #[must_use]
    pub fn fade_in(target: f32, frames: u32) -> Self {
        Self {
            start_volume: 0.0,
            target_volume: target.clamp(0.0, 1.0),
            total_frames: frames.max(1),
            frames_remaining: frames.max(1),
            on_complete: FadeMode::Hold,
        }
    }

    /// Construct a fade-out ramp from `start` to 0 over `frames` ; the
    /// voice retires when the ramp completes.
    #[must_use]
    pub fn fade_out(start: f32, frames: u32) -> Self {
        Self {
            start_volume: start.clamp(0.0, 1.0),
            target_volume: 0.0,
            total_frames: frames.max(1),
            frames_remaining: frames.max(1),
            on_complete: FadeMode::StopOnComplete,
        }
    }

    /// Cross-fade between two volume levels.
    #[must_use]
    pub fn ramp(start: f32, target: f32, frames: u32) -> Self {
        Self {
            start_volume: start.clamp(0.0, 1.0),
            target_volume: target.clamp(0.0, 1.0),
            total_frames: frames.max(1),
            frames_remaining: frames.max(1),
            on_complete: FadeMode::Hold,
        }
    }

    /// Sample the ramp at `frame_offset` from start ; returns the
    /// interpolated volume.
    #[must_use]
    pub fn sample(&self, frame_offset: u32) -> f32 {
        if self.total_frames == 0 {
            return self.target_volume;
        }
        let t = (frame_offset.min(self.total_frames) as f32) / (self.total_frames as f32);
        self.start_volume + (self.target_volume - self.start_volume) * t
    }

    /// Advance the ramp by `frames`. Returns `true` when the ramp is
    /// complete.
    pub fn advance(&mut self, frames: u32) -> bool {
        self.frames_remaining = self.frames_remaining.saturating_sub(frames);
        self.frames_remaining == 0
    }

    /// Whether the ramp completes the voice (fade-out + retire).
    #[must_use]
    pub const fn retires_voice(&self) -> bool {
        matches!(self.on_complete, FadeMode::StopOnComplete)
    }
}

/// Per-instance playback parameters. Not stored on the voice — the
/// mixer's `play()` builds the voice from these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayParams {
    /// Source position in 3D space ; `None` = positionless (UI / music).
    pub position: Option<Vec3>,
    /// Source velocity for Doppler shift ; `None` = static (no shift).
    pub velocity: Option<Vec3>,
    /// Pitch multiplier (1.0 = unchanged ; 2.0 = octave up ; 0.5 = down).
    pub pitch: f32,
    /// Voice volume (0.0 ‥ 1.0).
    pub volume: f32,
    /// Whether to loop on EOF.
    pub looping: bool,
    /// Optional initial fade-in.
    pub fade: Option<Fade>,
    /// Bus to route this voice into. `None` = master bus.
    pub bus: Option<crate::bus::BusId>,
}

impl Default for PlayParams {
    fn default() -> Self {
        Self {
            position: None,
            velocity: None,
            pitch: 1.0,
            volume: 1.0,
            looping: false,
            fade: None,
            bus: None,
        }
    }
}

impl PlayParams {
    /// Build params for an unpositioned (UI / music) voice.
    #[must_use]
    pub fn ui(volume: f32) -> Self {
        Self {
            volume: volume.clamp(0.0, 1.0),
            ..Self::default()
        }
    }

    /// Build params for a 3D-positioned voice.
    #[must_use]
    pub fn positioned(position: Vec3, volume: f32) -> Self {
        Self {
            position: Some(position),
            volume: volume.clamp(0.0, 1.0),
            ..Self::default()
        }
    }

    /// Build params for a moving voice (Doppler shift).
    #[must_use]
    pub fn moving(position: Vec3, velocity: Vec3, volume: f32) -> Self {
        Self {
            position: Some(position),
            velocity: Some(velocity),
            volume: volume.clamp(0.0, 1.0),
            ..Self::default()
        }
    }

    /// Set looping flag.
    #[must_use]
    pub const fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Set pitch multiplier.
    #[must_use]
    pub const fn with_pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    /// Set initial fade.
    #[must_use]
    pub const fn with_fade(mut self, fade: Fade) -> Self {
        self.fade = Some(fade);
        self
    }

    /// Route to a specific bus.
    #[must_use]
    pub const fn with_bus(mut self, bus: crate::bus::BusId) -> Self {
        self.bus = Some(bus);
        self
    }
}

/// A single playback instance.
///
/// § FIELDS
///   - `id`        — stable monotonic identifier ; used for ordering.
///   - `source`    — the playback source (PCM cursor or streaming).
///   - `state`     — runtime state machine.
///   - `position`  — 3D position when spatialized ; `None` for UI.
///   - `velocity`  — 3D velocity for Doppler shift ; `None` for static.
///   - `volume`    — 0.0‥1.0.
///   - `pitch`     — playback rate multiplier.
///   - `looping`   — wrap on EOF when true.
///   - `fade`      — optional active ramp.
///   - `bus`       — routing target ; `None` = master.
///   - `original_handle` — the SoundHandle this voice was created from
///                          (kept for replay-introspection).
pub struct MixerVoice {
    /// Stable monotonic id.
    pub id: VoiceId,
    /// Playback source — owned by the voice for the voice's lifetime.
    pub(crate) source: VoiceSource,
    /// Runtime state.
    pub state: VoiceState,
    /// 3D position ; `None` = unpositioned (UI).
    pub position: Option<Vec3>,
    /// 3D velocity ; `None` = static.
    pub velocity: Option<Vec3>,
    /// Voice volume.
    pub volume: f32,
    /// Pitch multiplier.
    pub pitch: f32,
    /// Whether to loop on EOF.
    pub looping: bool,
    /// Active fade, if any.
    pub fade: Option<Fade>,
    /// Routing target ; `None` = master.
    pub bus: Option<crate::bus::BusId>,
    /// SoundHandle this voice was instantiated from (for replay).
    pub original_handle: Option<SoundHandle>,
}

/// Internal — owned playback source. Either a PCM cursor (OneShot /
/// Looping) or a streaming source.
pub(crate) enum VoiceSource {
    /// PCM-backed cursor.
    Pcm(PcmSource),
    /// Streaming source.
    Stream(Box<dyn crate::sound::SoundSource>),
}

impl core::fmt::Debug for VoiceSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Pcm(p) => f
                .debug_struct("Pcm")
                .field("frames", &p.frames())
                .field("cursor", &p.cursor())
                .finish(),
            Self::Stream(_) => f
                .debug_struct("Stream")
                .field("source", &"<dyn SoundSource>")
                .finish(),
        }
    }
}

impl VoiceSource {
    pub(crate) fn rate(&self) -> u32 {
        match self {
            Self::Pcm(p) => p.rate(),
            Self::Stream(s) => s.rate(),
        }
    }

    pub(crate) fn channels(&self) -> u16 {
        match self {
            Self::Pcm(p) => p.channels(),
            Self::Stream(s) => s.channels(),
        }
    }

    pub(crate) fn read(&mut self, out: &mut [f32], looping: bool) -> usize {
        match self {
            Self::Pcm(p) => {
                if looping {
                    p.read_frames_looping(out)
                } else {
                    p.read_frames(out)
                }
            }
            Self::Stream(s) => s.next_frames(out),
        }
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        match self {
            Self::Pcm(p) => p.is_exhausted(),
            Self::Stream(s) => !s.has_more(),
        }
    }
}

impl MixerVoice {
    /// Build a voice with a PCM source. Internal — use `Mixer::play`.
    pub(crate) fn from_pcm(
        id: VoiceId,
        source: PcmSource,
        params: &PlayParams,
        original_handle: Option<SoundHandle>,
    ) -> Self {
        Self {
            id,
            source: VoiceSource::Pcm(source),
            state: VoiceState::Idle,
            position: params.position,
            velocity: params.velocity,
            volume: params.volume.clamp(0.0, 1.0),
            pitch: params.pitch.max(0.0),
            looping: params.looping,
            fade: params.fade,
            bus: params.bus,
            original_handle,
        }
    }

    /// Build a voice with a streaming source. Internal — used when the
    /// caller hands a `Sound::Streaming(box)` to `Mixer::play_stream`.
    pub(crate) fn from_stream(
        id: VoiceId,
        source: Box<dyn crate::sound::SoundSource>,
        params: &PlayParams,
    ) -> Self {
        Self {
            id,
            source: VoiceSource::Stream(source),
            state: VoiceState::Idle,
            position: params.position,
            velocity: params.velocity,
            volume: params.volume.clamp(0.0, 1.0),
            pitch: params.pitch.max(0.0),
            looping: params.looping,
            fade: params.fade,
            bus: params.bus,
            original_handle: None,
        }
    }

    /// Whether the voice is contributing samples this frame.
    #[must_use]
    pub const fn is_audible(&self) -> bool {
        self.state.is_audible() || matches!(self.state, VoiceState::Idle)
    }

    /// Whether the voice has terminated and should be reaped.
    #[must_use]
    pub const fn is_stopped(&self) -> bool {
        matches!(self.state, VoiceState::Stopped)
    }

    /// Pause the voice. Render emits silence ; cursor frozen.
    pub const fn pause(&mut self) {
        self.state = VoiceState::Paused;
    }

    /// Resume from pause.
    pub const fn resume(&mut self) {
        self.state = VoiceState::Playing;
    }

    /// Stop the voice. Mixer reaps on next sweep.
    pub const fn stop(&mut self) {
        self.state = VoiceState::Stopped;
    }

    /// Sample-rate of the underlying source.
    pub fn source_rate(&self) -> u32 {
        self.source.rate()
    }

    /// Channel count of the underlying source.
    pub fn source_channels(&self) -> u16 {
        self.source.channels()
    }
}

impl core::fmt::Debug for MixerVoice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MixerVoice")
            .field("id", &self.id)
            .field("state", &self.state)
            .field("volume", &self.volume)
            .field("pitch", &self.pitch)
            .field("looping", &self.looping)
            .field("position", &self.position)
            .field("velocity", &self.velocity)
            .field("fade", &self.fade)
            .field("bus", &self.bus)
            .field("source", &self.source)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn vec3_zero_constants() {
        assert_eq!(Vec3::ZERO, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(Vec3::FORWARD, Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(Vec3::UP, Vec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn vec3_dot_product() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, -5.0, 6.0);
        assert_eq!(a.dot(b), 1.0 * 4.0 + 2.0 * -5.0 + 3.0 * 6.0);
    }

    #[test]
    fn vec3_sub_add() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a.sub(b), Vec3::new(-3.0, -3.0, -3.0));
        assert_eq!(a.add(b), Vec3::new(5.0, 7.0, 9.0));
    }

    #[test]
    fn vec3_length_pythagoras() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(v.length(), 5.0);
    }

    #[test]
    fn vec3_normalize_unit_length() {
        let v = Vec3::new(3.0, 4.0, 0.0).normalize();
        assert!((v.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vec3_normalize_zero_safe() {
        let v = Vec3::ZERO.normalize();
        assert_eq!(v, Vec3::ZERO);
    }

    #[test]
    fn vec3_cross_right_handed() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let y = Vec3::new(0.0, 1.0, 0.0);
        let z = x.cross(y);
        assert_eq!(z, Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn voice_id_ord() {
        let a = VoiceId(1);
        let b = VoiceId(2);
        assert!(a < b);
    }

    #[test]
    fn voice_state_is_audible() {
        assert!(VoiceState::Playing.is_audible());
        assert!(VoiceState::Fading.is_audible());
        assert!(!VoiceState::Paused.is_audible());
        assert!(!VoiceState::Stopped.is_audible());
        assert!(!VoiceState::Idle.is_audible());
    }

    #[test]
    fn fade_in_starts_at_zero_targets_volume() {
        let f = Fade::fade_in(0.7, 100);
        assert_eq!(f.start_volume, 0.0);
        assert_eq!(f.target_volume, 0.7);
        assert_eq!(f.total_frames, 100);
        assert_eq!(f.frames_remaining, 100);
        assert_eq!(f.on_complete, FadeMode::Hold);
    }

    #[test]
    fn fade_out_starts_at_volume_targets_zero_retires() {
        let f = Fade::fade_out(0.8, 50);
        assert_eq!(f.start_volume, 0.8);
        assert_eq!(f.target_volume, 0.0);
        assert_eq!(f.on_complete, FadeMode::StopOnComplete);
        assert!(f.retires_voice());
    }

    #[test]
    fn fade_ramp_at_zero_returns_start() {
        let f = Fade::ramp(0.2, 0.8, 100);
        assert_eq!(f.sample(0), 0.2);
    }

    #[test]
    fn fade_ramp_at_full_returns_target() {
        let f = Fade::ramp(0.2, 0.8, 100);
        assert_eq!(f.sample(100), 0.8);
    }

    #[test]
    fn fade_ramp_at_half_returns_midpoint() {
        let f = Fade::ramp(0.0, 1.0, 100);
        assert!((f.sample(50) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn fade_advance_decrements() {
        let mut f = Fade::fade_in(1.0, 100);
        let done = f.advance(50);
        assert!(!done);
        assert_eq!(f.frames_remaining, 50);
    }

    #[test]
    fn fade_advance_completes() {
        let mut f = Fade::fade_in(1.0, 100);
        let done = f.advance(150);
        assert!(done);
    }

    #[test]
    fn fade_advance_saturates() {
        let mut f = Fade::fade_in(1.0, 100);
        f.advance(200);
        assert_eq!(f.frames_remaining, 0);
    }

    #[test]
    fn play_params_default_is_full_volume_no_loop() {
        let p = PlayParams::default();
        assert!(p.position.is_none());
        assert_eq!(p.volume, 1.0);
        assert_eq!(p.pitch, 1.0);
        assert!(!p.looping);
        assert!(p.fade.is_none());
        assert!(p.bus.is_none());
    }

    #[test]
    fn play_params_ui_is_unpositioned() {
        let p = PlayParams::ui(0.5);
        assert!(p.position.is_none());
        assert_eq!(p.volume, 0.5);
    }

    #[test]
    fn play_params_positioned_keeps_position() {
        let pos = Vec3::new(1.0, 2.0, 3.0);
        let p = PlayParams::positioned(pos, 0.7);
        assert_eq!(p.position, Some(pos));
        assert!(p.velocity.is_none());
    }

    #[test]
    fn play_params_moving_includes_velocity() {
        let p = PlayParams::moving(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 1.0);
        assert!(p.velocity.is_some());
    }

    #[test]
    fn play_params_with_looping() {
        let p = PlayParams::default().with_looping(true);
        assert!(p.looping);
    }

    #[test]
    fn play_params_with_pitch() {
        let p = PlayParams::default().with_pitch(2.0);
        assert_eq!(p.pitch, 2.0);
    }

    #[test]
    fn play_params_with_fade() {
        let f = Fade::fade_in(1.0, 100);
        let p = PlayParams::default().with_fade(f);
        assert!(p.fade.is_some());
    }

    #[test]
    fn play_params_volume_clamped_to_one() {
        let p = PlayParams::ui(2.0);
        assert_eq!(p.volume, 1.0);
    }

    #[test]
    fn play_params_volume_clamped_below_zero() {
        let p = PlayParams::ui(-0.5);
        assert_eq!(p.volume, 0.0);
    }
}
