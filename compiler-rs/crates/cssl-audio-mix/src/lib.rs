//! § cssl-audio-mix — CSSLv3 stage0 audio mixer + 3D spatial DSP
//! ════════════════════════════════════════════════════════════════════════
//!
//! § SPEC : `specs/04_EFFECTS.csl § BUILT-IN EFFECTS § Realtime<p>` +
//!          `specs/22_TELEMETRY.csl § AUDIO-OPS` +
//!          `specs/30_SUBSTRATE.csl § PHASES § audio-fiber`.
//!
//! § ROLE
//!   Engine-side **mix bus** sitting between the per-system sound emitters
//!   (gameplay code, music synth, UI feedback) and the platform-specific
//!   output device exposed by `cssl-host-audio`. Per-voice playback
//!   discipline + 3D spatial panning + DSP effect chains live here.
//!
//!   The crate's hot path is :
//!     `(voices, listener) -> per_voice_render -> spatial_pan ->
//!      bus_sum -> effect_chain -> master -> AudioStream::submit_frames`.
//!
//! § SURFACE  (stage-0 STABLE)
//!   ```text
//!   struct Mixer { voices, listener, master_bus, sub_buses, master_clip }
//!     fn new(format: AudioFormat) -> Self
//!     fn play(sound: SoundHandle, params: PlayParams) -> VoiceId
//!     fn stop(VoiceId)
//!     fn render_frames(out: &mut [f32], frames: usize)
//!     impl OmegaSystem        // mixing-per-tick
//!
//!   enum Sound  { OneShot(PcmData), Looping(PcmData), Streaming(BufStream) }
//!   struct PcmData { sample_rate, channels, samples : Vec<f32> }
//!
//!   struct MixerVoice {
//!       sound, position?, velocity?, pitch, volume, looping, fade
//!   }
//!   struct Listener { position, orientation, velocity }
//!
//!   mod dsp {
//!       biquad : LowPass / HighPass / BandPass
//!       reverb : Schroeder all-pass + comb network
//!       delay  : configurable taps + feedback
//!       dynamics : Compressor / Limiter (attack/release/threshold/ratio)
//!   }
//!
//!   mod spatial {
//!       ITD + ILD-based panning ; Doppler-shift via velocity ;
//!       distance attenuation (inverse-square or linear-rolloff).
//!   }
//!   ```
//!
//! § REALTIME-CRIT INVARIANTS  ‼ load-bearing
//!   The mixer's `render_frames` is called from the audio-callback fiber
//!   (per `specs/30 § PHASES § audio`). Per `04_EFFECTS § Realtime<Crit>`
//!   the body MUST honor :
//!     - `{NoAlloc}`     — no `Vec::push`, no `String::new`, no `Box::new`
//!                          on the hot path. Buffers are pre-sized at
//!                          construction.
//!     - `{NoUnbounded}` — every loop has a static or buffer-bounded count.
//!     - `{Deadline<1ms>}` (advisory, EFR0006) — per-frame compute MUST fit
//!                          within the audio-buffer-deadline (e.g.
//!                          `256 frames @ 48 kHz = 5.33 ms`).
//!     - `{PureDet}`     — voice-mix order is sorted by `VoiceId` so two
//!                          mixers seeded identically with identical voice
//!                          submission order produce bit-equal output.
//!
//! § SAFETY ENVELOPE — PRIME-DIRECTIVE
//!   - **OUTPUT-ONLY**. The mixer NEVER opens a capture device, NEVER
//!     routes the post-effect signal to a recordable sink, NEVER
//!     provides a "record what's playing" surface. Per PRIME_DIRECTIVE §1
//!     surveillance, audio-loopback to capture is structurally forbidden ;
//!     it is impossible to construct via this surface.
//!   - **OMEGA-FIBER GATING**. The `OmegaSystem for Mixer` impl declares
//!     `EffectRow::sim_audio()` which the scheduler hoists onto the
//!     dedicated audio-callback fiber per `specs/30 § PHASES`. A `{Sim}`-
//!     only fiber MUST NOT call `Mixer::render_frames` ; per H4 EFR0005
//!     `{Audio} ⊎ {Sim}` on the same fiber is forbidden.
//!   - **NO HRTF**. Head-related-transfer-function spatialization is a
//!     deferred slice. The stage-0 spatial path is ITD + ILD only ; this
//!     keeps the hot path within the deadline + avoids shipping a HRTF
//!     database that future-licensing-questions can attach to.
//!
//! § DETERMINISM  ‼ load-bearing
//!   - Voice-mix order is deterministic — voices are kept in a `BTreeMap<
//!     VoiceId, MixerVoice>` so iteration order is monotone-ascending.
//!     Two replays produce bit-identical mixed output given identical
//!     voice submissions.
//!   - DSP filter state is owned + serializable per filter ; the
//!     `replay_state` accessor lets the scheduler snapshot + restore
//!     filter-internals at frame boundaries.
//!   - No `thread_rng()`, no `Instant::now()`, no platform clock reads on
//!     the hot path.
//!
//! § ABI STABILITY
//!   The public surface above is **stage-0 STABLE** per the T11-D76 ABI
//!   lock precedent. Renaming any item (Mixer, Sound, MixerVoice, Listener,
//!   biquad/reverb/delay/dynamics modules) is a major-version-bump event.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)] // f32 audio synthesis : usize→f32 fixtures intentional
#![allow(clippy::cast_possible_truncation)] // sample-index → u32 truncation intentional
#![allow(clippy::cast_sign_loss)]
// distance-attenuation guards against negative values explicitly
// § Pedantic-bucket allowances — DSP code is hot-path-readable-first.
//   - suboptimal_flops : `a*b + c` is the textbook DSP algebra ; `mul_add` is
//                         a micro-optimization that obscures the per-line math.
//                         The compiler typically emits FMA where the target ABI permits it.
//   - cognitive_complexity : DSP coefficient blocks naturally have many branches.
//   - similar_names : ITD pairs (gain_left/gain_right, delay_left_samples/delay_right_samples)
//                      are intentionally symmetric ; renaming would harm clarity.
//   - explicit_iter_loop / needless_range_loop : `for i in 0..frames` reads as
//                      "for each frame index" — clearer than iterator chains for DSP.
//   - field_reassign_with_default / useless_let_if_seq / useless_conversion :
//                      builder-style reassign reads cleaner than spread syntax.
//   - manual_let_else : the explicit match form preserves error context on Mixer paths.
//   - derivable_impls : Default impls hand-rolled to document defaults inline.
//   - should_implement_trait : `Vec3::add/sub` are CSSLv3 method-style, not std::ops.
//   - unnecessary_cast : explicit casts in algebra preserve audit-trace at the
//                         exact site (sample-rate conversions are load-bearing).
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
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod bus;
pub mod dsp;
pub mod error;
pub mod listener;
pub mod mixer;
pub mod sound;
pub mod spatial;
pub mod system;
pub mod voice;

pub use bus::{Bus, BusId, MasterBus};
pub use error::{MixError, Result};
pub use listener::{Listener, Orientation};
pub use mixer::{Mixer, MixerConfig, MixerCounters};
pub use sound::{PcmData, PcmDataBuilder, Sound, SoundBank, SoundHandle, SoundSource};
pub use voice::{Fade, FadeMode, MixerVoice, PlayParams, Vec3, VoiceId, VoiceState};

/// Crate version — exposed for scaffold-verification (matches the
/// `STAGE0_SCAFFOLD` pattern in sibling crates).
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation literal. The mixer's audit-walker
/// verifies this string is reachable from the binary so the build was
/// assembled under the consent-as-OS axiom.
///
/// ≡ "There was no hurt nor harm in the making of this, to anyone /
///   anything / anybody."
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }
}
