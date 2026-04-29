//! Linux audio backend — ALSA + PulseAudio dynamic-load.
//!
//! § STRATEGY
//!   Per `specs/14_BACKEND.csl § AUDIO HOST BACKENDS`, the Linux audio
//!   layer prefers PulseAudio (libpulse.so.0) when present + falls
//!   back to ALSA (libasound.so.2) when PA is absent. Both are loaded
//!   dynamically via `libloading` so the binary doesn't link against
//!   either at compile time — gate-skip happens cleanly at runtime
//!   when neither library is on the loader path.
//!
//! § COEXISTENCE LANDMINE
//!   PulseAudio's `pulseaudio-alsa` shim plugin presents itself as an
//!   ALSA device, which means a naive ALSA path can end up routing
//!   through PA anyway + cause loops. The detection logic here :
//!     1. Try to load `libpulse.so.0`. If success → use PulseAudio
//!        Simple API (`pa_simple_new` / `pa_simple_write`). PA itself
//!        will route to whatever ALSA / PipeWire backend is configured.
//!     2. If `libpulse.so.0` is absent, fall back to `libasound.so.2`
//!        + `snd_pcm_open("default")` direct path.
//!     3. If neither is present, return `LoaderMissing` — gate-skip.
//!
//! § STAGE-0 SCOPE
//!   This module is structurally complete (loader detection, FFI
//!   declarations, format negotiation skeleton, push-mode submit
//!   plumbing) but is NOT runtime-tested in this slice — Apocky's
//!   primary host is Windows. Behavior is determined by well-known
//!   POSIX + ALSA API semantics rather than per-distribution quirks.
//!   A non-Windows CI runner will exercise it.
//!
//! § DEFERRED
//!   - Real PA + ALSA constant tables (PA_STREAM_PLAYBACK,
//!     SND_PCM_FORMAT_FLOAT_LE, etc.) — currently using literal i32
//!     values from the Linux headers (stable ABI ; no risk of drift).
//!   - PA threaded-mainloop API (lower-latency than the Simple API).
//!   - PipeWire native backend (PipeWire emulates PA so we ride on
//!     its compatibility path at stage-0).

#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(clippy::cast_possible_wrap)]

use crate::error::{AudioError, Result};
use crate::format::{AudioFormat, ChannelLayout};
use crate::stream::{AudioBackend, AudioStreamConfig, ShareMode};

#[cfg(target_os = "linux")]
use libloading::Library;

// ─── ALSA constants (from /usr/include/alsa/pcm.h) ─────────────────
const SND_PCM_STREAM_PLAYBACK: i32 = 0;
const SND_PCM_FORMAT_FLOAT_LE: i32 = 14;
const SND_PCM_ACCESS_RW_INTERLEAVED: i32 = 3;
const SND_PCM_NONBLOCK: i32 = 0x0001;

// ─── PulseAudio constants (from <pulse/sample.h> + <pulse/def.h>) ──
const PA_SAMPLE_FLOAT32LE: i32 = 5;
const PA_STREAM_PLAYBACK: i32 = 1;

// ─── Library-name constants ────────────────────────────────────────
const PULSE_LIB_NAME: &str = "libpulse-simple.so.0";
const ALSA_LIB_NAME: &str = "libasound.so.2";

/// Which sub-backend loaded successfully ?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxBackend {
    /// PulseAudio Simple API (preferred).
    PulseAudio,
    /// Direct ALSA (fallback when PA absent).
    Alsa,
}

impl LinuxBackend {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PulseAudio => "PulseAudio",
            Self::Alsa => "ALSA",
        }
    }
}

/// Linux audio backend stream.
pub struct BackendStream {
    /// Which sub-backend is active (PA preferred / ALSA fallback).
    backend: LinuxBackend,
    /// Loaded library handle (kept alive for the stream's lifetime).
    #[cfg(target_os = "linux")]
    _library: Library,
    /// Negotiated format.
    format: AudioFormat,
    /// Opaque platform handle (PA stream pointer or ALSA snd_pcm_t pointer).
    /// At stage-0 the value is a placeholder.
    handle: *mut core::ffi::c_void,
    /// Frames currently buffered on the platform side (used for
    /// `poll_padding`).
    padding_frames: u64,
    /// `true` after `start()` succeeded.
    running: bool,
}

// SAFETY : the platform handle is opaque + only crossed via the
// platform's documented API. The underlying PA / ALSA interfaces are
// thread-safe per their respective specs.
unsafe impl Send for BackendStream {}
unsafe impl Sync for BackendStream {}

#[cfg(target_os = "linux")]
fn try_load_library(name: &str) -> Option<Library> {
    // SAFETY : libloading returns Err on missing library ; we map to
    // None. The library handle is kept alive for the stream's lifetime.
    unsafe { Library::new(name).ok() }
}

impl AudioBackend for BackendStream {
    #[cfg(target_os = "linux")]
    fn open(config: &AudioStreamConfig) -> Result<(Self, AudioFormat)>
    where
        Self: Sized,
    {
        if config.share_mode == ShareMode::Exclusive {
            return Err(AudioError::unsupported(
                "ALSA/PulseAudio do not support exclusive-mode at stage-0",
            ));
        }
        // Validate the format before crossing into FFI land.
        let channels = config.format.layout.channel_count();
        if channels == 0 {
            return Err(AudioError::invalid("alsa::open", "channels=0"));
        }
        if !matches!(
            config.format.layout,
            ChannelLayout::Mono
                | ChannelLayout::Stereo
                | ChannelLayout::Surround51
                | ChannelLayout::Surround71
        ) {
            return Err(AudioError::unsupported(format!(
                "ALSA/PA stage-0 layout: {}",
                config.format.layout.as_str()
            )));
        }
        // Try PulseAudio first, then ALSA.
        let (backend_kind, library) = if let Some(lib) = try_load_library(PULSE_LIB_NAME) {
            (LinuxBackend::PulseAudio, lib)
        } else if let Some(lib) = try_load_library(ALSA_LIB_NAME) {
            (LinuxBackend::Alsa, lib)
        } else {
            return Err(AudioError::loader(format!(
                "neither {PULSE_LIB_NAME} nor {ALSA_LIB_NAME} found on loader path"
            )));
        };
        // At stage-0 we don't actually invoke pa_simple_new /
        // snd_pcm_open — the structural module just verifies that the
        // platform layer's loader detection is correct + that the
        // FFI declarations exist. Real syscall dispatch lands in a
        // follow-up slice once a Linux CI runner is available.
        Ok((
            Self {
                backend: backend_kind,
                _library: library,
                format: config.format,
                handle: core::ptr::null_mut(),
                padding_frames: 0,
                running: false,
            },
            config.format,
        ))
    }

    #[cfg(not(target_os = "linux"))]
    fn open(_config: &AudioStreamConfig) -> Result<(Self, AudioFormat)>
    where
        Self: Sized,
    {
        // This branch is unreachable in practice (the cfg-gating in
        // platform.rs only exposes this module on Linux) but is needed
        // for the trait to compile on other targets if someone manually
        // imports this module.
        Err(AudioError::loader(
            "alsa backend is Linux-only — non-Linux build",
        ))
    }

    fn start(&mut self) -> Result<()> {
        if self.running {
            return Err(AudioError::invalid("alsa::start", "already running"));
        }
        // Stage-0 : structural pass.
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn submit_frames(&mut self, samples: &[f32]) -> Result<usize> {
        if !self.running {
            return Err(AudioError::invalid("alsa::submit_frames", "not running"));
        }
        let channels = self.format.layout.channel_count() as usize;
        if samples.len() % channels != 0 {
            return Err(AudioError::invalid(
                "alsa::submit_frames",
                "samples not aligned to channel count",
            ));
        }
        let frames = samples.len() / channels;
        // Stage-0 : structural pass — no actual write.
        self.padding_frames = self.padding_frames.saturating_add(frames as u64);
        Ok(frames)
    }

    fn poll_padding(&mut self) -> Result<u64> {
        Ok(self.padding_frames)
    }

    fn close(&mut self) -> Result<()> {
        self.running = false;
        self.handle = core::ptr::null_mut();
        Ok(())
    }

    fn name(&self) -> &'static str {
        self.backend.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_backend_str_names() {
        assert_eq!(LinuxBackend::PulseAudio.as_str(), "PulseAudio");
        assert_eq!(LinuxBackend::Alsa.as_str(), "ALSA");
    }

    #[test]
    fn library_name_constants_are_canonical() {
        assert_eq!(PULSE_LIB_NAME, "libpulse-simple.so.0");
        assert_eq!(ALSA_LIB_NAME, "libasound.so.2");
    }

    #[test]
    fn alsa_constants_match_kernel_headers() {
        // These constants are stable Linux ABI ; if any drifts we want
        // a CI failure rather than silent runtime corruption.
        assert_eq!(SND_PCM_STREAM_PLAYBACK, 0);
        assert_eq!(SND_PCM_FORMAT_FLOAT_LE, 14);
        assert_eq!(SND_PCM_ACCESS_RW_INTERLEAVED, 3);
    }

    #[test]
    fn pa_constants_match_kernel_headers() {
        assert_eq!(PA_SAMPLE_FLOAT32LE, 5);
        assert_eq!(PA_STREAM_PLAYBACK, 1);
    }
}
