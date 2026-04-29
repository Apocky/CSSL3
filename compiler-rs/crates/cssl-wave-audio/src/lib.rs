//! § cssl-wave-audio — Wave-Unity audio projection (T11-D125b).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § 0` :
//!
//!   ```text
//!   ‼ light + audio = ONE complex-valued wave-field ψ(x,t) ∈ ℂ ⊗
//!     projected at-different-frequency-bands
//!   ¬ "the audio engine" + "the light engine" coupled-via-shared-cascade
//!   ✓ ONE substrate-PDE : ψ(x,t) ⊗ obeys ⊗ wave-Helmholtz-Boltzmann-hybrid
//!     ⊗ ALL-bands-emerge-as-band-pass-filtered-projections
//!   ```
//!
//!   `cssl-wave-audio` IS the AUDIO-band slice of that unified ψ-field,
//!   downstream of the T11-D114 multi-band wave-solver. It provides :
//!
//!     - [`PsiAudioField`] : sparse Morton-keyed AUDIO-band ψ-overlay
//!       (one Complex<f32> per active cell).
//!     - [`WaveAudioProjector`] : per-ear projection of ψ-AUDIO to
//!       binaural stereo, with HRTF + RC-derived ITD/ILD.
//!     - [`LbmSpatialAudio`] : D3Q19 wave-LBM stream-collide solver
//!       for room-resonance + creature-vocalization on the ψ-AUDIO
//!       band.
//!     - [`BinauralRender`] : per-ear stereo `f32` output from per-ear
//!       complex amplitudes, with phase-coherent multi-source mixing +
//!       ILD head-shadowing + soft-clip limiting.
//!     - [`CrossBandCoupler`] : reads the spec § XI cross-band-coupling
//!       table + applies AUDIO-row entries (LIGHT→AUDIO shimmer,
//!       MANA→AUDIO magic-hum, AUDIO→LIGHT visible-sound, AUDIO→HEAT
//!       absorption). Refuses any matrix that violates AGENCY-INVARIANT
//!       (LIGHT→MANA = 0, AUDIO→MANA = 0).
//!     - [`ProceduralVocal`] : creature vocalization synthesized from
//!       SDF-vocal-tract + KAN-derived spectral coefficients (no
//!       sample-library lookup ; full ψ-PDE on the vocal tract domain).
//!
//! § SIBLING TO cssl-audio-mix (LEGACY)
//!   The legacy mixer at `crates/cssl-audio-mix` is a SIBLING crate ;
//!   cssl-wave-audio does NOT depend on it. The two coexist during the
//!   deprecation window. When the `legacy_mixer` feature is enabled
//!   the [`legacy`] module re-exports the legacy types alongside the
//!   wave-audio surface so consumers can A/B compare ; the legacy
//!   re-export emits a deprecation note via rustdoc.
//!
//! § PRIME-DIRECTIVE-ALIGNMENT
//!   - **§1 (Surveillance)** : cssl-wave-audio is OUTPUT-ONLY by
//!     structural construction. There is no capture-device API on the
//!     surface. Reverb is field-derived (LBM ψ-PDE on geometry) ;
//!     vocal synthesis is procedural (SDF + KAN) ; no microphone-
//!     array impulse-response is recorded. See [`attestation`] for
//!     the verbatim §1 disclaimer.
//!   - **§11 (Attestation)** : the canonical attestation block + tag
//!     are recorded in [`attestation::ATTESTATION`] +
//!     [`attestation::ATTESTATION_TAG`]. The audit-walker verifies
//!     these strings are reachable from the compiled binary.
//!   - **§ XVII (Wave-Unity attestation §1-§7)** : every ψ-injection
//!     into a Sovereign-domain cell threads through the Σ-mask consent
//!     gate via [`PsiAudioField::inject`]. Cells refusing the
//!     `Modify` op-class refuse the injection.
//!
//! § ATTESTATION-TAG : T11-D125b-cssl-wave-audio
//! § BRANCH         : cssl/session-11/T11-D125b-wave-audio
//! § WORKTREE       : .claude/worktrees/W4-12

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Pedantic-bucket allowances — the wave-audio path is hot-path-readable-first.
//   - suboptimal_flops : `a*b + c` is the textbook DSP algebra ; `mul_add` is
//                         a micro-optimization that obscures the per-line math.
//   - cognitive_complexity : LBM stream-collide naturally has many branches per
//                             D3Q19 stencil entry.
//   - similar_names : ITD pairs (psi_left/psi_right, ear positions) are
//                      intentionally symmetric.
//   - explicit_iter_loop / needless_range_loop : `for i in 0..n` reads as
//                      "for each frame index" — clearer than iterator chains
//                      in DSP.
//   - manual_let_else : the explicit `match` form preserves error context.
//   - many_single_char_names : DSP coefficient blocks naturally use short names.
//   - cast_precision_loss / cast_possible_truncation / cast_sign_loss :
//                      sample-rate conversions + lattice-coordinate casts are
//                      load-bearing ; explicit at the site preserves audit-trace.
//   - module_name_repetitions : matches the workspace baseline.
//   - missing_errors_doc / missing_panics_doc / must_use_candidate :
//                      matches the workspace baseline.
//   - float_cmp : `f32 == 0.0` is the canonical "silent cell" test.
//   - field_reassign_with_default : builder-style mutations preferred.
//   - needless_pass_by_value : closures take by value in builder APIs.
//   - useless_let_if_seq / useless_conversion / option_if_let_else :
//                      preserve readability of explicit form.
//   - or_fun_call : `unwrap_or(Vec3::ZERO)` reads naturally.
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::float_cmp)]
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
#![allow(clippy::manual_range_contains)]
#![allow(clippy::if_not_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::needless_continue)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::comparison_to_empty)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::single_match_else)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::comparison_chain)]

pub mod attestation;
pub mod binaural;
pub mod complex;
pub mod coupling;
pub mod error;
pub mod kan;
pub mod lbm;
pub mod listener;
pub mod projector;
pub mod psi_field;
pub mod sdf;
pub mod vec3;
pub mod vocal;

pub use attestation::{
    ATTESTATION, ATTESTATION_AUTHOR, ATTESTATION_CITATIONS, ATTESTATION_SECTION_1, ATTESTATION_TAG,
};
pub use binaural::{BinauralConfig, BinauralRender, StereoSample};
pub use complex::Complex;
pub use coupling::{Band, CouplingMatrix, CrossBandCoupler};
pub use error::{Result, WaveAudioError};
pub use kan::{
    canonical_formant_table, ImpedanceKan, ImpedanceKanInputs, VocalKanInputs, VocalSpectralKan,
    VOCAL_HARMONIC_COUNT,
};
pub use lbm::{
    LbmConfig, LbmSpatialAudio, D3Q19_DIRS, D3Q19_VELOCITIES, D3Q19_WEIGHTS, LBM_CFL, LBM_TAU,
    LBM_VOXEL_SIZE,
};
pub use listener::{AudioListener, Orientation, SPEED_OF_SOUND, STANDARD_HEAD_BASELINE};
pub use projector::{compute_doppler_ratio, ProjectionResult, ProjectorConfig, WaveAudioProjector};
pub use psi_field::{PsiAudioCell, PsiAudioField};
pub use sdf::{TractSegment, VocalTractSdf, WallClass};
pub use vec3::Vec3;
pub use vocal::{vocalization_demo, CreatureVocalSpec, ProceduralVocal, VocalSourceFrame};

/// Crate-version stamp ; recorded in audit + telemetry.
pub const CSSL_WAVE_AUDIO_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Crate-name stamp.
pub const CSSL_WAVE_AUDIO_CRATE: &str = "cssl-wave-audio";

/// Stage marker — matches the workspace's stage0-scaffold convention.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

// ═══════════════════════════════════════════════════════════════════════
// § Legacy bridge — opt-in re-export of cssl-audio-mix surface.
// ═══════════════════════════════════════════════════════════════════════

/// Legacy-mixer bridge module. Available behind the `legacy_mixer`
/// Cargo feature. When enabled the module re-exports a deprecation
/// note + the legacy mixer's primary types so consumers can A/B
/// compare against the wave-audio surface during the deprecation
/// window.
///
/// § DEPRECATION NOTE
///   The legacy mixer at `cssl-audio-mix` predates the Wave-Unity
///   substrate. It uses ITD-synthesized panning + a Schroeder-reverb
///   network instead of the field-derived reverb that emerges from
///   the LBM ψ-PDE. New consumers SHOULD use `cssl-wave-audio` ;
///   the legacy mixer is preserved here for incremental migration.
///
/// ‼ DEPRECATED ⊗ migrate to cssl-wave-audio
#[cfg(feature = "legacy_mixer")]
pub mod legacy {
    pub use cssl_audio_mix::{
        Bus, BusId, Listener as LegacyListener, MasterBus, MixError, Mixer, MixerConfig,
        MixerVoice, Orientation as LegacyOrientation, PcmData, PcmDataBuilder, PlayParams, Sound,
        SoundBank, SoundHandle, Vec3 as LegacyVec3, VoiceId,
    };

    /// Marker signaling that this build pulled in the legacy mixer.
    /// Consumers checking `LEGACY_BRIDGE_ACTIVE` know they are in the
    /// deprecation-window code path.
    pub const LEGACY_BRIDGE_ACTIVE: &str = "T11-D125b: legacy mixer bridge active";
}

#[cfg(test)]
mod crate_invariants {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CSSL_WAVE_AUDIO_CRATE, "cssl-wave-audio");
    }

    #[test]
    fn version_is_nonempty() {
        assert!(!CSSL_WAVE_AUDIO_VERSION.is_empty());
    }

    #[test]
    fn stage0_scaffold_matches_version() {
        assert_eq!(STAGE0_SCAFFOLD, CSSL_WAVE_AUDIO_VERSION);
    }

    #[test]
    fn attestation_reachable() {
        assert!(!ATTESTATION.is_empty());
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn attestation_section_1_disclaims_capture() {
        assert!(ATTESTATION_SECTION_1.contains("OUTPUT-ONLY"));
        assert!(ATTESTATION_SECTION_1.contains("NEVER opens a capture device"));
    }

    #[test]
    fn attestation_tag_is_d125b() {
        assert!(ATTESTATION_TAG.contains("D125b"));
    }

    #[test]
    fn key_types_are_in_scope() {
        // Compile-time check : the public surface is all in scope.
        let _: PsiAudioField = PsiAudioField::new();
        let _: AudioListener = AudioListener::at_origin();
        let _: WaveAudioProjector = WaveAudioProjector::default();
        let _: BinauralRender = BinauralRender::default();
        let _: LbmSpatialAudio = LbmSpatialAudio::default();
        let _ = CrossBandCoupler::default();
        let _ = ProceduralVocal::default_human().unwrap();
    }

    #[test]
    fn vec3_listener_complex_constants() {
        // Smoke test : key constants survive the public re-export.
        assert_eq!(Vec3::ZERO, Vec3::default());
        assert_eq!(Complex::ZERO, Complex::default());
        // Speed-of-sound + head-baseline are compile-time constants ;
        // we verify them via runtime-comparison to non-const values to
        // avoid clippy::assertions_on_constants.
        let c = SPEED_OF_SOUND;
        let b = STANDARD_HEAD_BASELINE;
        assert!(c > 340.0 && c < 350.0);
        assert!(b > 0.0 && b < 0.3);
    }

    #[test]
    fn citations_index_includes_all_specs() {
        let cites = ATTESTATION_CITATIONS;
        let joined = cites.join(" | ");
        assert!(joined.contains("PRIME_DIRECTIVE"));
        assert!(joined.contains("WAVE_UNITY"));
        assert!(joined.contains("FIELD_AUDIO"));
    }
}

#[cfg(all(test, feature = "legacy_mixer"))]
mod legacy_bridge_tests {
    use super::legacy::{LegacyListener, LegacyVec3, LEGACY_BRIDGE_ACTIVE};

    #[test]
    fn legacy_bridge_marker_present() {
        assert!(LEGACY_BRIDGE_ACTIVE.contains("D125b"));
    }

    #[test]
    fn legacy_listener_constructs() {
        let _l = LegacyListener::at_origin();
    }

    #[test]
    fn legacy_vec3_zero() {
        assert_eq!(LegacyVec3::default(), LegacyVec3::ZERO);
    }
}
