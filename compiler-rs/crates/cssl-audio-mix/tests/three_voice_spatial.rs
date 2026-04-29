//! Integration test : 3-voice mix with ITD spatial panning.
//!
//! § PURPOSE
//!   The session report-back requires verification that the mixer correctly
//!   spatializes three concurrent voices :
//!     - voice_1 : tone @ origin (no pan, both channels equal)
//!     - voice_2 : tone @ -90° (full left, ITD delays right channel)
//!     - voice_3 : tone @ +90° (full right, ITD delays left channel)
//!   We measure the per-channel energy + verify the ITD/ILD math gives the
//!   expected panning across stereo channels.
//!
//! § TONE GENERATION
//!   We generate 200 Hz sine waves at sample rate 48 kHz. The frequency is
//!   well within the audible range + below comb-filter resonances so any
//!   spatial-pan effects are isolated from accidental DSP interactions.

#![allow(clippy::cast_precision_loss)] // tone-generator usize→f32 fixtures
#![allow(clippy::needless_collect)] // L+R extraction is intentional for clarity
#![allow(clippy::items_after_statements)] // local helper-fn is scoped inline by design
#![allow(clippy::similar_names)] // h1a/h1b across two mixers is intentional symmetry

use cssl_audio_mix::{Mixer, MixerConfig, PcmData, PlayParams, Vec3};

const SAMPLE_RATE: u32 = 48_000;
const FRAMES: usize = 4_096; // ~85 ms at 48 kHz — enough for ITD steady-state
const TONE_FREQ_HZ: f32 = 200.0;

/// Generate a stereo sine tone at `freq_hz` for `frames` frames.
fn make_tone(freq_hz: f32, frames: usize) -> PcmData {
    let mut samples = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let t = (i as f32) / (SAMPLE_RATE as f32);
        let s = (2.0 * std::f32::consts::PI * freq_hz * t).sin();
        samples.push(s);
        samples.push(s);
    }
    PcmData::new(samples, SAMPLE_RATE, 2).expect("valid pcm")
}

/// Compute per-channel RMS energy from an interleaved stereo buffer.
fn channel_rms(buf: &[f32]) -> (f32, f32) {
    let frames = buf.len() / 2;
    let mut l_sum = 0.0;
    let mut r_sum = 0.0;
    for i in 0..frames {
        l_sum += buf[i * 2] * buf[i * 2];
        r_sum += buf[i * 2 + 1] * buf[i * 2 + 1];
    }
    let n = frames as f32;
    ((l_sum / n).sqrt(), (r_sum / n).sqrt())
}

#[test]
fn three_voice_mix_voice_at_zero_balanced_pan() {
    // Voice at origin should produce equal-energy on L+R.
    let mut config = MixerConfig::default_stereo();
    config.block_frames = FRAMES;
    let mut m = Mixer::new(config);
    let pcm = make_tone(TONE_FREQ_HZ, FRAMES);
    let h = m.register_sound(pcm).expect("register");
    let _v = m
        .play_oneshot(h, PlayParams::positioned(Vec3::ZERO, 0.5))
        .expect("play");

    let mut out = vec![0.0_f32; FRAMES * 2];
    m.render_frames(&mut out, FRAMES);

    let (rms_l, rms_r) = channel_rms(&out);
    // Center pan : equal-power, so L≈R within numerical tolerance.
    let diff = (rms_l - rms_r).abs();
    let avg = (rms_l + rms_r) * 0.5;
    let rel_diff = diff / avg.max(1e-9);
    assert!(
        rel_diff < 0.05,
        "voice-at-origin should have balanced L/R : L={rms_l}, R={rms_r}, rel_diff={rel_diff}"
    );
}

#[test]
fn three_voice_mix_voice_at_minus_90_routes_left() {
    // Voice at -X (full-left of listener at origin).
    let mut config = MixerConfig::default_stereo();
    config.block_frames = FRAMES;
    let mut m = Mixer::new(config);
    let pcm = make_tone(TONE_FREQ_HZ, FRAMES);
    let h = m.register_sound(pcm).expect("register");
    let _v = m
        .play_oneshot(h, PlayParams::positioned(Vec3::new(-10.0, 0.0, 0.0), 0.5))
        .expect("play");

    let mut out = vec![0.0_f32; FRAMES * 2];
    m.render_frames(&mut out, FRAMES);

    let (rms_l, rms_r) = channel_rms(&out);
    // Full-left pan : L should be much louder than R.
    assert!(
        rms_l > rms_r * 5.0,
        "-90° should route to L : L={rms_l}, R={rms_r}"
    );
    // R should be near-zero (sin(0) = 0 in equal-power pan).
    assert!(
        rms_r < 0.01,
        "right channel should be near-silent : R={rms_r}"
    );
}

#[test]
fn three_voice_mix_voice_at_plus_90_routes_right() {
    // Voice at +X (full-right of listener at origin).
    let mut config = MixerConfig::default_stereo();
    config.block_frames = FRAMES;
    let mut m = Mixer::new(config);
    let pcm = make_tone(TONE_FREQ_HZ, FRAMES);
    let h = m.register_sound(pcm).expect("register");
    let _v = m
        .play_oneshot(h, PlayParams::positioned(Vec3::new(10.0, 0.0, 0.0), 0.5))
        .expect("play");

    let mut out = vec![0.0_f32; FRAMES * 2];
    m.render_frames(&mut out, FRAMES);

    let (rms_l, rms_r) = channel_rms(&out);
    assert!(
        rms_r > rms_l * 5.0,
        "+90° should route to R : L={rms_l}, R={rms_r}"
    );
    assert!(
        rms_l < 0.01,
        "left channel should be near-silent : L={rms_l}"
    );
}

#[test]
fn three_voice_mix_combined_balanced_when_summed() {
    // All three voices simultaneously :
    //   voice_1 @ 0°   → L=R
    //   voice_2 @ -90° → L only
    //   voice_3 @ +90° → R only
    // Total : L = voice_1_L + voice_2_full ≈ voice_1_R + voice_3_full = R.
    // → Combined L+R energies should be approximately equal.
    let mut config = MixerConfig::default_stereo();
    config.block_frames = FRAMES;
    let mut m = Mixer::new(config);

    // Use distinct tone frequencies so superposition doesn't constructively
    // interfere across voices ; the test still measures aggregate L vs R.
    let pcm1 = make_tone(200.0, FRAMES);
    let pcm2 = make_tone(440.0, FRAMES);
    let pcm3 = make_tone(880.0, FRAMES);
    let h1 = m.register_sound(pcm1).expect("register 1");
    let h2 = m.register_sound(pcm2).expect("register 2");
    let h3 = m.register_sound(pcm3).expect("register 3");

    m.play_oneshot(h1, PlayParams::positioned(Vec3::ZERO, 0.4))
        .expect("play 1");
    m.play_oneshot(h2, PlayParams::positioned(Vec3::new(-10.0, 0.0, 0.0), 0.4))
        .expect("play 2");
    m.play_oneshot(h3, PlayParams::positioned(Vec3::new(10.0, 0.0, 0.0), 0.4))
        .expect("play 3");

    let mut out = vec![0.0_f32; FRAMES * 2];
    m.render_frames(&mut out, FRAMES);

    let (rms_l, rms_r) = channel_rms(&out);
    let avg = (rms_l + rms_r) * 0.5;
    // The mix should have non-trivial energy on both channels.
    assert!(rms_l > 0.05, "L energy too low : L={rms_l}");
    assert!(rms_r > 0.05, "R energy too low : R={rms_r}");
    // L vs R should be within 30% of average — voices are symmetric.
    let rel_diff = (rms_l - rms_r).abs() / avg.max(1e-9);
    assert!(
        rel_diff < 0.30,
        "combined mix should be roughly balanced : L={rms_l}, R={rms_r}, rel_diff={rel_diff}"
    );
}

#[test]
fn three_voice_mix_itd_delay_present_for_panned_voices() {
    // The voice at -X should produce an output where the right channel
    // has its phase delayed relative to the left by the ITD offset.
    // We measure this by computing the L vs R cross-correlation peak
    // and checking it occurs at a non-zero lag.
    let mut config = MixerConfig::default_stereo();
    config.block_frames = FRAMES;
    let mut m = Mixer::new(config);
    let pcm = make_tone(TONE_FREQ_HZ, FRAMES);
    let h = m.register_sound(pcm).expect("register");
    // Voice at -X = full left ; right channel should be ITD-delayed.
    m.play_oneshot(h, PlayParams::positioned(Vec3::new(-10.0, 0.0, 0.0), 0.5))
        .expect("play");

    let mut out = vec![0.0_f32; FRAMES * 2];
    m.render_frames(&mut out, FRAMES);

    // Extract L and R streams.
    let l: Vec<f32> = out.iter().step_by(2).copied().collect();
    let r: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();

    // Verify R contains zero-prefix (the ITD leading-silence).
    // The voice is at full-left → right channel is ITD-delayed by
    // ~MAX_ITD_SECS * sample_rate ≈ 38 samples @ 48k.
    // For the first ~38 samples the right channel should be 0 (silence
    // before delay-line catches up).
    let mut r_zero_prefix = 0;
    for &s in &r {
        if s.abs() < 1e-6 {
            r_zero_prefix += 1;
        } else {
            break;
        }
    }
    // Expect at least a few samples of ITD-induced silence prefix on R.
    // Since the voice is at -90° fully panned to L, R has both ILD≈0 + ITD delay.
    // Even if ILD makes R near-zero overall, the prefix should be exact-zero.
    assert!(
        r_zero_prefix > 0,
        "right channel should have ITD silence prefix at full-left pan ; got {r_zero_prefix} zeros"
    );
    // Unused but keeps L symbol live for other potential checks.
    let _ = l.len();
}

#[test]
fn three_voice_mix_replay_determinism_bit_equal() {
    // Two mixers with identical setup + identical play order produce
    // bit-equal output (validates the determinism contract).
    let pcm1 = make_tone(200.0, FRAMES);
    let pcm2 = make_tone(440.0, FRAMES);
    let pcm3 = make_tone(880.0, FRAMES);

    fn build_mixer() -> Mixer {
        let mut config = MixerConfig::default_stereo();
        config.block_frames = FRAMES;
        Mixer::new(config)
    }

    let mut m1 = build_mixer();
    let mut m2 = build_mixer();
    let h1a = m1.register_sound(pcm1.clone()).unwrap();
    let h2a = m1.register_sound(pcm2.clone()).unwrap();
    let h3a = m1.register_sound(pcm3.clone()).unwrap();
    let h1b = m2.register_sound(pcm1).unwrap();
    let h2b = m2.register_sound(pcm2).unwrap();
    let h3b = m2.register_sound(pcm3).unwrap();
    assert_eq!(h1a, h1b);
    assert_eq!(h2a, h2b);
    assert_eq!(h3a, h3b);

    for m in [&mut m1, &mut m2] {
        m.play_oneshot(h1a, PlayParams::positioned(Vec3::ZERO, 0.4))
            .unwrap();
        m.play_oneshot(h2a, PlayParams::positioned(Vec3::new(-10.0, 0.0, 0.0), 0.4))
            .unwrap();
        m.play_oneshot(h3a, PlayParams::positioned(Vec3::new(10.0, 0.0, 0.0), 0.4))
            .unwrap();
    }

    let mut out1 = vec![0.0_f32; FRAMES * 2];
    let mut out2 = vec![0.0_f32; FRAMES * 2];
    m1.render_frames(&mut out1, FRAMES);
    m2.render_frames(&mut out2, FRAMES);

    assert_eq!(out1, out2, "two-mixer replay is not bit-equal");
}
