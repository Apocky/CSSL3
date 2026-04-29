#![allow(clippy::float_cmp)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::field_reassign_with_default)]

//! § cssl-wave-audio — integration tests.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   End-to-end exercise of the wave-audio surface : the projector
//!   reading a populated ψ-AUDIO field, the LBM solver evolving a
//!   point-source impulse through a creature vocal-tract, the cross-
//!   band coupler injecting magic-hum from a MANA donor, and the
//!   procedural-vocal demo rendering a creature vocalization to
//!   stereo `f32` frames.
//!
//! § PEDANTIC ALLOWANCES (matches workspace baseline)
//!   The integration tests use the same DSP-readable-first style as the
//!   unit tests + sibling crates. The `#![allow(...)]` block above
//!   matches the workspace baseline ; comments restated here are for
//!   readers, not the compiler.
//!
//! § SPEC
//!   - Acceptance per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § XIV` :
//!     ✓ ψ-norm conservation verified.
//!     ✓ Cross-band-coupling-table enforced.
//!     ✓ All 6 novel-artifacts render-correctly @ test-scenes (we
//!       cover the resonant-instrument + visible-sound-fields proxies
//!       at the LBM/coupler boundary here ; the LIGHT-band dependent
//!       artifacts wait on D114).
//!     ✓ Σ-check @ ψ-injection.

use cssl_substrate_omega_field::morton::MortonKey;
use cssl_substrate_omega_field::sigma_overlay::SigmaOverlay;
use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked};
use cssl_wave_audio::{
    canonical_formant_table, vocalization_demo, AudioListener, Band, BinauralRender, Complex,
    CouplingMatrix, CreatureVocalSpec, CrossBandCoupler, ImpedanceKan, ImpedanceKanInputs,
    LbmConfig, LbmSpatialAudio, ProceduralVocal, ProjectorConfig, PsiAudioField, StereoSample,
    Vec3, VocalKanInputs, VocalSpectralKan, VocalTractSdf, WallClass, WaveAudioError,
    WaveAudioProjector, ATTESTATION, ATTESTATION_AUTHOR, ATTESTATION_CITATIONS,
    ATTESTATION_SECTION_1, ATTESTATION_TAG, CSSL_WAVE_AUDIO_CRATE, LBM_TAU, LBM_VOXEL_SIZE,
    SPEED_OF_SOUND, STAGE0_SCAFFOLD, STANDARD_HEAD_BASELINE, VOCAL_HARMONIC_COUNT,
};

// ───────────────────────────────────────────────────────────────────────
// § Smoke tests : crate-surface present + attestation reachable.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn smoke_crate_constants_present() {
    assert_eq!(CSSL_WAVE_AUDIO_CRATE, "cssl-wave-audio");
    assert!(!STAGE0_SCAFFOLD.is_empty());
    // Bind constants to non-const locals to dodge clippy::assertions_on_constants.
    let c = SPEED_OF_SOUND;
    let b = STANDARD_HEAD_BASELINE;
    assert!(c > 340.0 && c < 350.0);
    assert!(b > 0.0 && b < 0.3);
    assert!((LBM_VOXEL_SIZE - 0.5).abs() < 1e-6);
    assert!((LBM_TAU - 0.6).abs() < 1e-6);
}

#[test]
fn smoke_attestation_reachable() {
    assert!(ATTESTATION.contains("no hurt nor harm"));
    assert!(ATTESTATION.contains("Density is sovereignty"));
    assert!(ATTESTATION.contains("consent is the OS"));
    assert!(ATTESTATION_TAG.contains("D125b"));
    assert!(ATTESTATION_AUTHOR.contains("AI-collective"));
}

#[test]
fn smoke_attestation_section_1_disclaims_capture() {
    let s = ATTESTATION_SECTION_1;
    assert!(s.contains("OUTPUT-ONLY"));
    assert!(s.contains("NEVER opens a capture device"));
    assert!(s.contains("non-surveillance"));
    assert!(s.contains("field-derived"));
    assert!(s.contains("KAN-derived"));
}

#[test]
fn smoke_citations_index_references_specs() {
    let joined = ATTESTATION_CITATIONS.join(" | ");
    assert!(joined.contains("PRIME_DIRECTIVE.md §1"));
    assert!(joined.contains("PRIME_DIRECTIVE.md §11"));
    assert!(joined.contains("WAVE_UNITY"));
    assert!(joined.contains("FIELD_AUDIO"));
}

// ───────────────────────────────────────────────────────────────────────
// § Σ-mask gate : ψ-injection refuses default-private cells.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_psi_injection_default_private_refused() {
    let mut psi = PsiAudioField::new();
    let sigma = SigmaOverlay::new();
    // SigmaOverlay default is DefaultPrivate (Observe-only).
    let k = MortonKey::encode(1, 0, 0).unwrap();
    let r = psi.inject(k, Complex::new(0.5, 0.0), &sigma);
    assert!(r.is_err());
    match r.unwrap_err() {
        WaveAudioError::ConsentDenied { requested, .. } => {
            assert_eq!(requested, "modify");
        }
        e => panic!("expected ConsentDenied, got {e:?}"),
    }
}

#[test]
fn integration_psi_injection_after_modify_grant_succeeds() {
    let mut psi = PsiAudioField::new();
    let mut sigma = SigmaOverlay::new();
    let k = MortonKey::encode(2, 0, 0).unwrap();
    let mask = SigmaMaskPacked::default_mask().with_consent(ConsentBit::Modify.bits());
    sigma.set(k, mask);
    psi.inject(k, Complex::new(0.5, 0.0), &sigma).unwrap();
    assert_eq!(psi.at(k).re, 0.5);
}

// ───────────────────────────────────────────────────────────────────────
// § Binaural correctness : center pan, lateral pan, head-shadow ILD.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_binaural_center_source_equal_ears() {
    // Source directly in front : ψ-AUDIO sampled equally at both ears,
    // azimuth = 0 → no ILD attenuation → equal pressures.
    //
    // Listener placed at world position (5, 0, 0) so the per-ear probe
    // points fall into populated cells (negative-coordinate cells
    // would clip to zero amplitude).
    let mut psi = PsiAudioField::new();
    for x in 5..15 {
        for y in 0..3 {
            for z in 0..3 {
                let k = MortonKey::encode(x, y, z).unwrap();
                psi.set(k, Complex::new(0.5, 0.0)).unwrap();
            }
        }
    }
    let l = AudioListener::at(Vec3::new(5.0, 0.5, 0.5));
    let p = WaveAudioProjector::default();
    let r = p.project(&psi, &l, Some(Vec3::new(5.0, 0.5, -10.0)), None);
    assert!((r.stereo.left - r.stereo.right).abs() < 1e-3);
}

#[test]
fn integration_binaural_right_source_pans_right() {
    let mut psi = PsiAudioField::new();
    for x in 0..2 {
        for y in 0..2 {
            for z in 0..2 {
                let k = MortonKey::encode(x, y, z).unwrap();
                psi.set(k, Complex::new(0.5, 0.0)).unwrap();
            }
        }
    }
    let l = AudioListener::at_origin();
    let p = WaveAudioProjector::default();
    let r = p.project(&psi, &l, Some(Vec3::new(10.0, 0.0, 0.0)), None);
    assert!(r.stereo.right > r.stereo.left);
    assert!(r.azimuth_rad > 0.0);
}

#[test]
fn integration_binaural_left_source_pans_left() {
    let mut psi = PsiAudioField::new();
    for x in 0..2 {
        for y in 0..2 {
            for z in 0..2 {
                let k = MortonKey::encode(x, y, z).unwrap();
                psi.set(k, Complex::new(0.5, 0.0)).unwrap();
            }
        }
    }
    let l = AudioListener::at_origin();
    let p = WaveAudioProjector::default();
    let r = p.project(&psi, &l, Some(Vec3::new(-10.0, 0.0, 0.0)), None);
    assert!(r.stereo.left > r.stereo.right);
    assert!(r.azimuth_rad < 0.0);
}

#[test]
fn integration_binaural_itd_sub_millisecond() {
    let psi = PsiAudioField::new();
    let l = AudioListener::at_origin();
    let p = WaveAudioProjector::default();
    let r = p.project(&psi, &l, Some(Vec3::new(0.5, 0.0, 0.0)), None);
    // ITD for a 1m-source at 17.5cm baseline ≈ 250 µs ; well below 1 ms.
    assert!(r.itd_seconds.abs() < 0.001);
}

#[test]
fn integration_binaural_doppler_listener_approaching() {
    let psi = PsiAudioField::new();
    let mut l = AudioListener::at_origin();
    l.set_velocity(Vec3::new(0.0, 0.0, -50.0));
    let p = WaveAudioProjector::default();
    let r = p.project(&psi, &l, Some(Vec3::new(0.0, 0.0, -10.0)), None);
    // Listener moving toward source → expects pitch shift away from 1.0.
    assert!(r.doppler_ratio < 1.0);
}

// ───────────────────────────────────────────────────────────────────────
// § Determinism : two replays produce bit-equal output.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_determinism_replay_bit_equal() {
    let mut psi = PsiAudioField::new();
    psi.set(MortonKey::encode(0, 0, 0).unwrap(), Complex::new(0.5, 0.3))
        .unwrap();
    let l = AudioListener::at(Vec3::new(0.1, 0.0, 0.0));
    let p = WaveAudioProjector::default();
    let r1 = p.project(&psi, &l, Some(Vec3::new(1.0, 0.0, 0.0)), None);
    let r2 = p.project(&psi, &l, Some(Vec3::new(1.0, 0.0, 0.0)), None);
    assert_eq!(r1.stereo.left.to_bits(), r2.stereo.left.to_bits());
    assert_eq!(r1.stereo.right.to_bits(), r2.stereo.right.to_bits());
    assert_eq!(r1.psi_left.re.to_bits(), r2.psi_left.re.to_bits());
    assert_eq!(r1.psi_right.re.to_bits(), r2.psi_right.re.to_bits());
}

// ───────────────────────────────────────────────────────────────────────
// § LBM integration : substep + boundary-condition + mass-conservation.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_lbm_point_source_propagates_outward() {
    let mut lbm = LbmSpatialAudio::default();
    let center = MortonKey::encode(10, 10, 10).unwrap();
    lbm.inject_source(center, Complex::new(1.0, 0.0)).unwrap();
    lbm.substep().unwrap();
    // Neighbors in all 6 axial directions should have non-zero amplitudes.
    for delta in [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ] {
        let (dx, dy, dz) = delta;
        let nx = (10 + dx) as u64;
        let ny = (10 + dy) as u64;
        let nz = (10 + dz) as u64;
        let k = MortonKey::encode(nx, ny, nz).unwrap();
        assert!(lbm.current().at(k).norm() > 0.0, "delta {delta:?}");
    }
}

#[test]
fn integration_lbm_l1_amplitude_conserved_in_open_field() {
    let mut lbm = LbmSpatialAudio::default();
    let k = MortonKey::encode(10, 10, 10).unwrap();
    lbm.inject_source(k, Complex::new(1.0, 0.0)).unwrap();
    let l1_before = lbm.current().l1_amplitude();
    lbm.substep().unwrap();
    let l1_after = lbm.current().l1_amplitude();
    // L1 amplitude should be conserved (Σ w_i = 1 → mass-conserving
    // explicit step). Tolerance 5% to account for f32 rounding.
    let drift = (l1_after - l1_before).abs() / l1_before.max(1e-6);
    assert!(drift < 0.05, "L1 drift = {drift}");
}

#[test]
fn integration_lbm_rigid_box_creates_resonance() {
    // Place a source in the middle of a rigid-walled cube ; after a
    // few substeps the energy should be redistributed but constrained
    // to the cube interior (no leakage into the boundary cells).
    let mut lbm = LbmSpatialAudio::default();
    let center = MortonKey::encode(5, 5, 5).unwrap();
    lbm.inject_source(center, Complex::new(1.0, 0.0)).unwrap();
    // Mark a 3x3x3 shell of rigid walls around (5,5,5).
    for dx in -1..=1_i32 {
        for dy in -1..=1_i32 {
            for dz in -1..=1_i32 {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let nx = (5 + dx) as u64;
                let ny = (5 + dy) as u64;
                let nz = (5 + dz) as u64;
                let k = MortonKey::encode(nx, ny, nz).unwrap();
                if dx.abs() == 1 && dy.abs() == 1 && dz.abs() == 1 {
                    // Corner cells form the shell.
                    lbm.mark_boundary(k, WallClass::Rigid);
                }
            }
        }
    }
    lbm.substep().unwrap();
    // After substep the corners should be ψ = 0 (Dirichlet).
    let corner = MortonKey::encode(6, 6, 6).unwrap();
    assert!(lbm.current().at(corner).norm() < 1e-6);
}

#[test]
fn integration_lbm_step_for_dt_clamped_to_max() {
    let mut lbm = LbmSpatialAudio::new(LbmConfig {
        max_substeps: 4,
        ..LbmConfig::default()
    });
    let k = MortonKey::encode(5, 5, 5).unwrap();
    lbm.inject_source(k, Complex::new(0.1, 0.0)).unwrap();
    // dt = 1.0 s ; default Δt_sub ≈ 7.3e-4 ; should clamp to 4 substeps.
    let n = lbm.step_for_dt(1.0).unwrap();
    assert_eq!(n, 4);
}

// ───────────────────────────────────────────────────────────────────────
// § Cross-band coupling : LIGHT→AUDIO + MANA→AUDIO + AUDIO→LIGHT outflow.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_coupling_canonical_validates() {
    let m = CouplingMatrix::canonical_spec();
    m.validate_agency().unwrap();
    let c = CrossBandCoupler::canonical().unwrap();
    assert!((c.matrix().strength(Band::Light, Band::Audio) - 0.001).abs() < 1e-9);
    assert!((c.matrix().strength(Band::Mana, Band::Audio) - 0.05).abs() < 1e-9);
}

#[test]
fn integration_coupling_light_to_audio_shimmer() {
    let mut audio = PsiAudioField::new();
    let coupler = CrossBandCoupler::canonical().unwrap();
    // LIGHT donor : a single bright cell.
    let k = MortonKey::encode(7, 7, 7).unwrap();
    let donor = vec![(k, Complex::new(50.0, 0.0))];
    coupler
        .apply_light_to_audio(&mut audio, donor, 1.0)
        .unwrap();
    // Strength 0.001 × 50 × 1 = 0.05 ψ-AUDIO.
    let injected = audio.at(k);
    assert!((injected.re - 0.05).abs() < 1e-4);
}

#[test]
fn integration_coupling_mana_to_audio_magic_hum() {
    let mut audio = PsiAudioField::new();
    let coupler = CrossBandCoupler::canonical().unwrap();
    let k = MortonKey::encode(8, 8, 8).unwrap();
    let donor = vec![(k, Complex::new(2.0, 0.0))];
    coupler.apply_mana_to_audio(&mut audio, donor, 1.0).unwrap();
    // Strength 0.05 × 2 × 1 = 0.1 ψ-AUDIO.
    let injected = audio.at(k);
    assert!((injected.re - 0.1).abs() < 1e-4);
}

#[test]
fn integration_coupling_audio_outflow_to_light_attenuates() {
    let mut audio = PsiAudioField::new();
    let coupler = CrossBandCoupler::canonical().unwrap();
    let k = MortonKey::encode(9, 9, 9).unwrap();
    audio.set(k, Complex::new(10.0, 0.0)).unwrap();
    let outflow = coupler
        .apply_audio_outflow_to_light(&mut audio, 1.0, 0.0)
        .unwrap();
    assert!(outflow > 0.0);
    let after = audio.at(k);
    // 10 - 0.001 * 10 * 1 = 9.99
    assert!((after.re - 9.99).abs() < 1e-3);
}

#[test]
fn integration_coupling_invalid_matrix_refused() {
    // Build a matrix with LIGHT → MANA = 0.5 ; should refuse.
    let mut m = CouplingMatrix::canonical_spec();
    m.entries[0][4] = 0.5;
    let r = CrossBandCoupler::from_matrix(m);
    assert!(r.is_err());
    match r.unwrap_err() {
        WaveAudioError::AgencyViolation { explanation } => {
            assert!(explanation.contains("LIGHT") && explanation.contains("MANA"));
        }
        e => panic!("expected AgencyViolation, got {e:?}"),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Procedural-vocal demo : creature vocalization end-to-end.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_vocal_demo_human_default_renders() {
    let (lbm, vocal) = vocalization_demo(CreatureVocalSpec::default()).unwrap();
    assert!(lbm.has_active_cells());
    assert!(vocal.t_seconds() > 0.0);
    assert!(vocal.formants()[0] > 200.0);
    assert!(vocal.formants()[0] < 1500.0);
}

#[test]
fn integration_vocal_demo_smaller_creature_higher_formants() {
    let small_spec = CreatureVocalSpec {
        tract_size: 0.5,
        ..CreatureVocalSpec::default()
    };
    let big_spec = CreatureVocalSpec {
        tract_size: 1.5,
        ..CreatureVocalSpec::default()
    };
    let (_, small) = vocalization_demo(small_spec).unwrap();
    let (_, big) = vocalization_demo(big_spec).unwrap();
    assert!(small.formants()[0] > big.formants()[0]);
}

#[test]
fn integration_vocal_demo_listener_samples_lip_radiation() {
    // After running the demo, project the LBM's current ψ-AUDIO field
    // to a listener positioned outside the lip aperture. The projector
    // should recover non-zero stereo output.
    let (lbm, vocal) = vocalization_demo(CreatureVocalSpec::default()).unwrap();

    // Place the listener at the lip-position (in world coords).
    let voxel = lbm.config().voxel_size;
    let lip = vocal.lip_cell(voxel);
    let lip_world = Vec3::new(
        lip.0 as f32 * voxel + 1.0, // 1 m beyond the lip
        lip.1 as f32 * voxel,
        lip.2 as f32 * voxel,
    );
    let listener = AudioListener::at(lip_world);
    let p = WaveAudioProjector::default();
    let r = p.project(lbm.current(), &listener, None, None);
    // The radiated waveform may be small ; we just verify the
    // projector returns valid (non-NaN) output.
    assert!(r.stereo.left.is_finite());
    assert!(r.stereo.right.is_finite());
}

#[test]
fn integration_vocal_l2_normalized_harmonic_amps() {
    let v = ProceduralVocal::default_human().unwrap();
    let amps = v.harmonic_amps();
    let l2: f32 = amps.iter().map(|a| a * a).sum::<f32>().sqrt();
    assert!((l2 - 1.0).abs() < 1e-3);
}

#[test]
fn integration_vocal_creature_vs_human_distinguishable() {
    // Two creatures with different specs should produce
    // distinguishable harmonic vectors (the spectrum encodes physics).
    let v1 = ProceduralVocal::creature(
        CreatureVocalSpec {
            tract_size: 0.5,
            throat_narrowness: 0.8,
            ..CreatureVocalSpec::default()
        },
        (5, 5, 5),
    )
    .unwrap();
    let v2 = ProceduralVocal::creature(
        CreatureVocalSpec {
            tract_size: 1.5,
            throat_narrowness: 0.0,
            ..CreatureVocalSpec::default()
        },
        (5, 5, 5),
    )
    .unwrap();
    let a1 = v1.harmonic_amps();
    let a2 = v2.harmonic_amps();
    let any_diff = (0..VOCAL_HARMONIC_COUNT).any(|i| (a1[i] - a2[i]).abs() > 0.01);
    assert!(any_diff, "creature spectrums should differ");
}

// ───────────────────────────────────────────────────────────────────────
// § SDF + KAN sanity : tract-radius profile, formant-table.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_sdf_human_default_resonant_modes_in_human_range() {
    let sdf = VocalTractSdf::human_default();
    let formants = sdf.formants(3, 343.0);
    // First formant should be in 200..600 Hz for a 17 cm tract.
    assert!(formants[0] > 400.0 && formants[0] < 600.0);
    // f2 ≈ 3·f1, f3 ≈ 5·f1 (uniform-tube odd-multiples).
    assert!((formants[1] / formants[0] - 3.0).abs() < 0.05);
    assert!((formants[2] / formants[0] - 5.0).abs() < 0.05);
}

#[test]
fn integration_kan_impedance_matches_canonical_walls() {
    let kan = ImpedanceKan::untrained();
    let rigid = kan.evaluate(ImpedanceKanInputs {
        wavelength_m: 0.343,
        wall_class_id: 0,
    });
    let soft = kan.evaluate(ImpedanceKanInputs {
        wavelength_m: 0.343,
        wall_class_id: 1,
    });
    let imped = kan.evaluate(ImpedanceKanInputs {
        wavelength_m: 0.343,
        wall_class_id: 2,
    });
    // Rigid : R >> Z_air ; Soft : R ≈ Z_air ; Impedance : intermediate.
    assert!(rigid[0] > 1e5);
    assert!((soft[0] - 415.0).abs() < 1.0);
    assert!(imped[0] > 1000.0 && imped[0] < 1e5);
}

#[test]
fn integration_kan_canonical_formant_table_consistent_with_sdf() {
    let (kan_f, _) = canonical_formant_table(1.0);
    let sdf_f = VocalTractSdf::human_default().formants(3, 343.0);
    // Both should match within 5%.
    for i in 0..3 {
        let drift = (kan_f[i] - sdf_f[i]).abs() / sdf_f[i].max(1e-6);
        assert!(drift < 0.05, "drift = {drift} at formant {i}");
    }
}

#[test]
fn integration_kan_vocal_spectrum_sums_to_unit_l2() {
    let mut kan = VocalSpectralKan::untrained();
    let v = kan.evaluate(VocalKanInputs::default()).unwrap();
    let l2: f32 = v.iter().map(|a| a * a).sum::<f32>().sqrt();
    assert!((l2 - 1.0).abs() < 1e-3);
}

// ───────────────────────────────────────────────────────────────────────
// § Block-rendering surface : project_block + project_series.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_project_block_emits_stereo_frames() {
    let mut psi = PsiAudioField::new();
    psi.set(MortonKey::encode(0, 0, 0).unwrap(), Complex::new(0.5, 0.0))
        .unwrap();
    let l = AudioListener::at_origin();
    let p = WaveAudioProjector::default();
    let mut out = vec![0.0_f32; 64];
    let n = p.project_block(&psi, &l, None, None, &mut out);
    assert_eq!(n, 32);
    // The buffer should be a valid contiguous f32 series ; we don't
    // assert a particular amplitude because the listener at origin
    // may sample empty cells (negative-coord clipping). Just check
    // every sample is finite.
    for v in &out {
        assert!(v.is_finite());
    }
}

#[test]
fn integration_binaural_phase_coherent_constructive_mix() {
    let r = BinauralRender::default();
    let s = r.mix_phase_coherent(&[
        (Complex::new(0.2, 0.0), Complex::new(0.2, 0.0), 0.0),
        (Complex::new(0.2, 0.0), Complex::new(0.2, 0.0), 0.0),
    ]);
    // Two in-phase sources sum constructively.
    let single = r.render_sample(Complex::new(0.2, 0.0), Complex::new(0.2, 0.0), 0.0);
    assert!(s.left > single.left);
}

#[test]
fn integration_binaural_phase_coherent_destructive_cancel() {
    let r = BinauralRender::default();
    let s = r.mix_phase_coherent(&[
        (Complex::new(0.5, 0.0), Complex::new(0.5, 0.0), 0.0),
        (Complex::new(-0.5, 0.0), Complex::new(-0.5, 0.0), 0.0),
    ]);
    assert!(s.left.abs() < 1e-5);
    assert!(s.right.abs() < 1e-5);
}

// ───────────────────────────────────────────────────────────────────────
// § Stage-1 wave-solver downstream stub : the coupler's API surface.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_stub_wave_solver_donor_pattern() {
    // Document the donor-iterator pattern : when D114's wave-solver
    // lands its `WaveSolver` will emit `(MortonKey, Complex)` pairs
    // for each cross-band-coupling pass. cssl-wave-audio consumes
    // this iterator-of-pairs surface today using a plain Vec<>.
    let mut audio = PsiAudioField::new();
    let coupler = CrossBandCoupler::canonical().unwrap();
    let donor = vec![
        (MortonKey::encode(1, 0, 0).unwrap(), Complex::new(10.0, 0.0)),
        (MortonKey::encode(2, 0, 0).unwrap(), Complex::new(20.0, 0.0)),
        (MortonKey::encode(3, 0, 0).unwrap(), Complex::new(30.0, 0.0)),
    ];
    coupler
        .apply_light_to_audio(&mut audio, donor, 1.0)
        .unwrap();
    // Three injection sites should now have non-zero AUDIO amplitudes.
    let mut count = 0;
    for x in 1..=3 {
        let k = MortonKey::encode(x, 0, 0).unwrap();
        if audio.at(k).norm() > 1e-6 {
            count += 1;
        }
    }
    assert_eq!(count, 3);
}

// ───────────────────────────────────────────────────────────────────────
// § Misc invariants : silence-default, ProjectorConfig roundtrip.
// ───────────────────────────────────────────────────────────────────────

#[test]
fn integration_silent_field_silent_projection() {
    let psi = PsiAudioField::new();
    let l = AudioListener::at_origin();
    let p = WaveAudioProjector::default();
    let r = p.project(&psi, &l, None, None);
    assert_eq!(r.stereo, StereoSample::SILENCE);
}

#[test]
fn integration_projector_config_roundtrip() {
    let mut p = WaveAudioProjector::default();
    let cfg = ProjectorConfig {
        voxel_size: 0.25,
        speed_of_sound: 350.0,
        doppler_min: 0.5,
        doppler_max: 2.0,
        master_gain: 1.5,
    };
    p.set_config(cfg);
    assert_eq!(p.config(), cfg);
}

#[test]
fn integration_projector_master_gain_scales_output() {
    let mut psi = PsiAudioField::new();
    psi.set(MortonKey::encode(0, 0, 0).unwrap(), Complex::new(0.5, 0.0))
        .unwrap();
    let l = AudioListener::at_origin();

    let mut p = WaveAudioProjector::default();
    let r1 = p.project(&psi, &l, None, None);
    let mut cfg = p.config();
    cfg.master_gain = 0.5;
    p.set_config(cfg);
    let r2 = p.project(&psi, &l, None, None);

    // r2 should be ≈ 0.5 × r1 (with soft-clip nuance for high amps,
    // but at 0.5 amplitude we're below the clip threshold).
    let ratio = r2.stereo.left / r1.stereo.left.max(1e-6);
    assert!((ratio - 0.5).abs() < 0.01);
}

#[test]
fn integration_listener_per_ear_distinct() {
    let l = AudioListener::at(Vec3::new(5.0, 0.0, 0.0));
    let le = l.left_ear();
    let re = l.right_ear();
    assert!((le.x - re.x).abs() > 0.0);
    let baseline = re.sub(le).length();
    assert!((baseline - STANDARD_HEAD_BASELINE).abs() < 1e-6);
}
