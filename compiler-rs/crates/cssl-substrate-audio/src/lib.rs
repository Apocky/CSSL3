//! § cssl-substrate-audio — CSSLv3 ADCS audio-pillar substrate
//! ════════════════════════════════════════════════════════════════════════
//!
//! § SPEC : `specs/30_SUBSTRATE_v3.csl § PILLAR-3 render § audio sub-pillar` +
//!          `specs/04_EFFECTS.csl § BUILT-IN EFFECTS § Realtime<p>` +
//!          `specs/22_TELEMETRY.csl § AUDIO-OPS` +
//!          `specs/24_HOST_FFI.csl § audio` (cssl-rt host_audio FFI,
//!           Wave-D6 / commit 6678c88-shared).
//!
//! § ROLE
//!   The ADCS render-pillar audio substrate. Sits between gameplay /
//!   companion-AI / wave-coupling sound producers and the platform
//!   audio FFI (`cssl-rt::host_audio` or `cssl-host-audio::AudioStream`).
//!
//!   Four canonical sub-systems, each in its own module :
//!
//!   - [`hrtf`]        — Head-Related Transfer Function spatializer.
//!                        Per-ear convolution against an azimuth-elevation
//!                        impulse-response set ; inter-aural phase
//!                        difference (IPD) for low-frequency cues ;
//!                        Doppler-shift via listener-relative velocity ;
//!                        inverse-square distance attenuation.
//!
//!   - [`synth`]       — Procedural synthesizer driven by a KAN
//!                        spline-network. Polyphonic oscillators
//!                        (sine / saw / square / triangle) ; ADSR
//!                        envelope per voice ; LFO modulation chain
//!                        (pitch / amp / filter) ; KAN-driven
//!                        spectral-coefficient surface for timbre.
//!
//!   - [`reverb`]      — Reverberation : (a) small-IR convolution-reverb
//!                        for short impulse responses ;
//!                        (b) Schroeder-Moorer Feedback-Delay-Network
//!                        parametric reverb with explicit RT60 +
//!                        diffusion + pre-delay knobs.
//!
//!   - [`spatializer`] — 3D-positional audio over HRTF/reverb mix.
//!                        Room-geometry occlusion via simple AABB
//!                        ray-cast attenuation ; direct-vs-reverberant
//!                        balance from listener-source distance + room
//!                        size ; per-source send-bus weights.
//!
//! § REALTIME-CRIT INVARIANTS  ‼ load-bearing
//!   Per `04_EFFECTS § Realtime<Crit>` the audio-callback fiber MUST
//!   honor `{NoAlloc}` + `{NoUnbounded}` + `{Deadline<1ms>}` + `{PureDet}`.
//!
//! § PRIME-DIRECTIVE — OUTPUT-ONLY
//!   Per `PRIME_DIRECTIVE.md § PROHIBITIONS § surveillance` ("silent
//!   microphone activation is a BUG class") this crate **NEVER**
//!   constructs an input stream, **NEVER** routes the post-render
//!   signal to a recordable sink, **NEVER** provides a "record what's
//!   playing" surface. The HRTF impulse-response data is **synthetic**
//!   (analytical spherical-head model + frequency-dependent shadowing
//!   + simulated pinna notch) — there is no recorded-from-human IR
//!   data shipped by this crate.
//!
//! § ABI STABILITY
//!   The public surface (HrtfSpatializer, KanSynth, ConvolutionReverb,
//!   FdnReverb, Spatializer3D, their `process_frames` methods) is
//!   stage-0 STABLE per the T11-D76 ABI lock precedent.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::similar_names)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::useless_let_if_seq)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::single_match)]
#![allow(clippy::if_not_else)]
#![allow(clippy::float_cmp)]
#![allow(clippy::while_float)]
#![allow(clippy::type_complexity)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::assertions_on_constants)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

// pub mod hrtf;        // W-T11-W11-AGGR · src/hrtf.rs orphaned during cross-branch shuffle · pending re-author
// pub mod synth;       // W-T11-W11-AGGR · src/synth.rs orphaned during cross-branch shuffle · pending re-author
// pub mod reverb;      // W-S-AUD-1 rate-limit-truncated · file not yet authored · pending post-CSSL-greenlight
// pub mod spatializer; // W-S-AUD-1 rate-limit-truncated · file not yet authored · pending post-CSSL-greenlight

// pub use hrtf::{EarChannel, HrtfConfig, HrtfImpulseSet, HrtfSpatializer};   // pending W-T11-W11-AGGR re-author
// pub use reverb::{ConvolutionReverb, FdnReverb, ReverbConfig};   // pending W-S-AUD-1
// pub use spatializer::{Listener3D, OcclusionAabb, RoomGeometry, SoundSource3D, Spatializer3D};   // pending W-S-AUD-1
// pub use synth::{
//     Adsr, AdsrStage, KanSynth, Lfo, LfoTarget, Oscillator, OscillatorWaveform, SynthVoice,
// };  // pending W-T11-W11-AGGR re-author of synth.rs

/// Speed of sound in air at 20 °C, sea level (m/s).
pub const SPEED_OF_SOUND_MPS: f32 = 343.0;

/// Inter-aural distance for the synthetic spherical-head HRTF model.
pub const INTER_AURAL_DISTANCE_M: f32 = 0.18;

/// Default sample rate (Hz). Matches the canonical AudioFormat used by
/// `cssl-host-audio` + the `cssl-rt::host_audio` FFI.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Maximum HRTF impulse-response length (samples).
pub const HRTF_IR_LEN: usize = 128;

/// Maximum simultaneous synth voices.
pub const MAX_SYNTH_VOICES: usize = 32;

/// Crate version sentinel.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// Slice id (kept for compatibility with the prior stub).
pub const SLICE_ID: &str = "T11-D330";

/// PRIME-DIRECTIVE attestation literal.
///
/// ≡ "There was no hurt nor harm in the making of this, to anyone /
///   anything / anybody."
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// Top-level error type for the audio substrate.
#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum AudioSubstrateError {
    /// HRTF azimuth or elevation outside the `[-180, 180] / [-90, 90]` envelope.
    #[error("HRTF angle out of range : azimuth={azimuth_deg}, elevation={elevation_deg}")]
    HrtfAngleOutOfRange { azimuth_deg: f32, elevation_deg: f32 },

    /// ADSR stage parameter (attack/decay/release) was negative or non-finite.
    #[error("ADSR parameter invalid : {field}={value}")]
    AdsrParamInvalid { field: &'static str, value: f32 },

    /// LFO rate out of range (must be `> 0` and `<= sample_rate / 2`).
    #[error("LFO rate {rate_hz} Hz out of range for sample_rate {sample_rate} Hz")]
    LfoRateOutOfRange { rate_hz: f32, sample_rate: u32 },

    /// Reverb RT60 / pre-delay outside of physically-plausible envelope.
    #[error("Reverb parameter out of range : {field}={value}")]
    ReverbParamOutOfRange { field: &'static str, value: f32 },

    /// Voice slot exhausted — hit `MAX_SYNTH_VOICES` polyphony cap.
    #[error("Voice slot exhausted : MAX_SYNTH_VOICES={limit}")]
    VoiceSlotExhausted { limit: usize },

    /// Spatializer source position contains NaN or infinity.
    #[error("Source position non-finite : ({x}, {y}, {z})")]
    SourcePositionNonFinite { x: f32, y: f32, z: f32 },
}

/// Result alias used throughout the crate.
pub type Result<T> = core::result::Result<T, AudioSubstrateError>;

/// 3D vector — minimal embedded type to avoid adding a `cssl-math` dep.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// Construct.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Origin (0,0,0).
    #[must_use]
    pub const fn origin() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Squared length.
    #[must_use]
    pub fn length_sq(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Length.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    /// Difference (a - b).
    #[must_use]
    pub fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    /// Returns `true` iff every component is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

#[cfg(test)]
mod scaffold_tests {
    use super::*;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn vec3_basic_geometry() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(v.length_sq(), 25.0);
        assert_eq!(v.length(), 5.0);
        assert!(v.is_finite());
    }

    #[test]
    fn vec3_subtract() {
        let a = Vec3::new(5.0, 7.0, 9.0);
        let b = Vec3::new(2.0, 3.0, 4.0);
        let d = a.sub(b);
        assert_eq!(d, Vec3::new(3.0, 4.0, 5.0));
    }
}
