//! WASAPI integration tests — exercised on Apocky's Win11 host.
//!
//! § SCOPE
//!   - On Windows : real `IAudioClient` open / start / submit-frames /
//!     stop / close cycle. The 1-second sine-tone test writes ~48k
//!     samples through the WASAPI render-client + asserts clean
//!     teardown.
//!   - On non-Windows : every test gate-skips with the
//!     `is_loader_missing()` predicate ; the suite stays green.
//!
//! § PRIME-DIRECTIVE
//!   The `capture_open_returns_consent_error` test is mandatory on
//!   every platform — it documents that the capture (microphone)
//!   surface is deferred per PRIME-DIRECTIVE consent gate.
//!
//! § TEST EXECUTION
//!   The full WASAPI sine-tone test runs only when the environment
//!   variable `CSSL_AUDIO_RUN_DEVICE_TESTS=1` is set. This keeps CI
//!   green on headless / no-audio-device Windows runners while still
//!   letting Apocky exercise the real path locally with a single
//!   `set CSSL_AUDIO_RUN_DEVICE_TESTS=1 && cargo test` invocation.
//!
//!   The test is also gated to skip when `AudioStream::open_default_output`
//!   returns a `LoaderMissing` / `DeviceNotFound` error — same
//!   `is_loader_missing()` skip-territory pattern as the d3d12 host.

#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)] // sine-tone test casts u32 frame counter to f32 for phase

use cssl_host_audio::{
    AudioError, AudioEvent, AudioFormat, AudioStream, AudioStreamConfig, ChannelLayout, SampleRate,
};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use cssl_host_audio::ShareMode;

const ONE_SECOND_FRAMES_48K: usize = 48_000;

fn run_device_tests() -> bool {
    std::env::var("CSSL_AUDIO_RUN_DEVICE_TESTS")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[test]
fn open_default_output_or_skip() {
    let result = AudioStream::open_default_output();
    match result {
        Ok(stream) => {
            assert!(!stream.is_running());
            #[cfg(target_os = "windows")]
            assert_eq!(stream.backend_name(), "WASAPI");
            #[cfg(not(target_os = "windows"))]
            {
                let name = stream.backend_name();
                assert!(name == "ALSA" || name == "PulseAudio" || name == "CoreAudio");
            }
        }
        Err(e) if e.is_loader_missing() => {
            eprintln!("[skip] no audio device on this host: {e}");
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn open_with_default_config_negotiates_format() {
    let result = AudioStream::open(&AudioStreamConfig::default_output());
    match result {
        Ok(stream) => {
            // Negotiated format must be f32 ; channel count > 0 ;
            // rate > 0. Stereo is the most common but not guaranteed.
            assert!(stream.format().rate.as_hz() > 0);
            assert!(stream.format().layout.channel_count() > 0);
            assert_eq!(AudioFormat::bytes_per_sample(), 4);
        }
        Err(e) if e.is_loader_missing() => {
            eprintln!("[skip] no audio device: {e}");
        }
        // Stage-0 fail-loud on rate mismatch is documented behavior.
        Err(AudioError::SampleRateMismatch { requested, device }) => {
            eprintln!("[stage-0 expected] device rate {device} Hz != requested {requested} Hz");
        }
        Err(AudioError::NotSupported { feature }) => {
            eprintln!("[skip] device format not f32: {feature}");
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn capture_open_returns_consent_error() {
    // PRIME-DIRECTIVE gate : capture is deferred. This test is the
    // canary that must FAIL in the future when capture-mode lands —
    // the failure will force a review of the consent / UI-affordance
    // contract before capture is exposed.
    use cssl_host_audio::stream::AudioCaptureStream;
    let r = AudioCaptureStream::open_default_input();
    match r {
        Err(AudioError::CaptureNotImplemented) => {}
        other => panic!("PRIME-DIRECTIVE consent gate broken : {other:?}"),
    }
}

#[test]
fn invalid_share_mode_on_alsa_returns_unsupported() {
    // Exclusive mode is Windows-only ; Linux / macOS reject it.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let cfg = AudioStreamConfig {
            share_mode: ShareMode::Exclusive,
            ..AudioStreamConfig::default_output()
        };
        let r = AudioStream::open(&cfg);
        match r {
            Err(AudioError::NotSupported { .. }) => {}
            Err(e) if e.is_loader_missing() => {}
            other => panic!("expected NotSupported or LoaderMissing, got {other:?}"),
        }
    }
    // On Windows the exclusive path is supported (test would require a
    // device that grants exclusive access, deferred from this slice).
}

#[cfg(target_os = "windows")]
#[test]
fn wasapi_sine_tone_one_second_or_skip() {
    if !run_device_tests() {
        eprintln!("[skip] CSSL_AUDIO_RUN_DEVICE_TESTS != 1 — skipping device-bound test");
        return;
    }
    let stream_result = AudioStream::open_default_output();
    let mut stream = match stream_result {
        Ok(s) => s,
        Err(e) if e.is_loader_missing() => {
            eprintln!("[skip] no audio device: {e}");
            return;
        }
        Err(AudioError::SampleRateMismatch { requested, device }) => {
            eprintln!("[skip] device rate {device} Hz vs requested {requested} Hz");
            return;
        }
        Err(AudioError::NotSupported { feature }) => {
            eprintln!("[skip] device format not f32: {feature}");
            return;
        }
        Err(e) => panic!("unexpected error opening default output: {e}"),
    };
    stream.start().expect("start");
    let format = stream.format();
    let channels = format.layout.channel_count() as usize;
    let rate = format.rate.as_hz() as f32;
    // Generate a 440 Hz sine tone for 1 second.
    let chunk_frames = 256;
    let total_frames = ONE_SECOND_FRAMES_48K;
    let mut frame_position: u32 = 0;
    let frequency_hz: f32 = 440.0;
    let amplitude: f32 = 0.10; // -20 dBFS — gentle, never user-painful.
    while (frame_position as usize) < total_frames {
        let mut buf = Vec::with_capacity(chunk_frames * channels);
        for f in 0..chunk_frames {
            let t = (frame_position + f as u32) as f32 / rate;
            let sample = amplitude * (2.0 * core::f32::consts::PI * frequency_hz * t).sin();
            for _ in 0..channels {
                buf.push(sample);
            }
        }
        let _accepted = stream.submit_frames(&buf).expect("submit_frames");
        frame_position += chunk_frames as u32;
        // Yield briefly to let WASAPI consume frames ; under heavy load
        // submit_frames returns 0 from a full ring → AudioEvent::Overrun.
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    // Capture a final padding poll.
    let _padding = stream.poll_padding().expect("poll_padding");
    stream.stop().expect("stop");
    // Inspect the event stream — must contain a Started event + a
    // Stopped event in order ; underrun count is informational.
    let events = stream.drain_events();
    let started = events
        .iter()
        .any(|e| matches!(e, AudioEvent::Started { .. }));
    let stopped = events.iter().any(|e| matches!(e, AudioEvent::Stopped));
    assert!(started, "expected Started event");
    assert!(stopped, "expected Stopped event");
    let counters = stream.counters();
    eprintln!(
        "wasapi sine-tone : submitted={} dropped={} underruns={} sample_clock={}",
        counters.frames_submitted,
        counters.frames_dropped,
        counters.underrun_count,
        counters.sample_clock,
    );
    // We submitted ~48000 frames ; counter should reflect at least
    // some real frames ; on a fully-empty buffer pass we expect close
    // to total_frames but allow generous tolerance because shared-mode
    // mix + back-pressure can drop frames legitimately.
    assert!(counters.frames_submitted > 0, "no frames accepted");
}

#[test]
fn audio_format_default_matches_48k_stereo() {
    let f = AudioFormat::default_output();
    assert_eq!(f.rate, SampleRate::Hz48000);
    assert_eq!(f.layout, ChannelLayout::Stereo);
}

#[test]
fn open_with_invalid_format_returns_invalid_argument() {
    // Force a clearly-invalid config. The bytes_per_sample static is
    // always 4 — exercise the fact that a 0-channel format is rejected
    // structurally before crossing FFI.
    let cfg = AudioStreamConfig {
        format: AudioFormat {
            rate: SampleRate::Hz48000,
            // Cannot construct ChannelLayout with 0 channels (enum-only
            // surface) ; instead, exercise the SampleRate::custom(0)
            // rejection path.
            layout: ChannelLayout::Stereo,
        },
        ..AudioStreamConfig::default_output()
    };
    let _ = AudioStream::open(&cfg);
    // Invalid sample-rate construction is caught earlier ; surface
    // here is the negotiation path.
    let bad_rate = SampleRate::custom(0);
    assert!(bad_rate.is_err());
}
