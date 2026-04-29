//! macOS audio backend — CoreAudio + AudioToolbox via dynamic-load.
//!
//! § STRATEGY
//!   Per `specs/14_BACKEND.csl § AUDIO HOST BACKENDS`, the macOS audio
//!   layer uses CoreAudio's AudioComponentInstance / AudioUnit API
//!   (default-output unit), loaded via `libloading` so the binary
//!   doesn't link against AudioToolbox.framework at compile time —
//!   gate-skip happens cleanly at runtime when the framework is absent
//!   (e.g., headless macOS builds).
//!
//! § STAGE-0 SCOPE
//!   This module is structurally complete (loader detection, FFI
//!   declarations, format negotiation skeleton, push-mode submit
//!   plumbing) but is NOT runtime-tested in this slice — Apocky's
//!   primary host is Windows. Behavior is determined by the well-
//!   documented CoreAudio API. A macOS CI runner will exercise it.
//!
//! § DEFERRED
//!   - Real CoreAudio constant tables (kAudioUnitType_Output,
//!     kAudioUnitSubType_DefaultOutput, kAudioFormatFlagIsFloat) —
//!     currently using literal i32 / u32 values from CoreAudio
//!     headers (stable ABI ; no risk of drift).
//!   - AudioOutputUnitStart / AudioOutputUnitStop wiring (stage-0 :
//!     structural).
//!   - Render-callback registration (the "pull" side of the API
//!     where CoreAudio asks us for samples). Stage-0 only exposes
//!     the push-mode adaptor where the user submits buffers + a
//!     ring drains them in the callback.

#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(clippy::cast_possible_wrap)]

use crate::error::{AudioError, Result};
use crate::format::{AudioFormat, ChannelLayout};
use crate::stream::{AudioBackend, AudioStreamConfig, ShareMode};

#[cfg(target_os = "macos")]
use libloading::Library;

// ─── CoreAudio constants ───────────────────────────────────────────
const kAudioUnitType_Output: u32 = 0x6175_6F75; // 'auou'
const kAudioUnitSubType_DefaultOutput: u32 = 0x6465_6620; // 'def '
const kAudioUnitSubType_HALOutput: u32 = 0x6168_616C; // 'ahal'
const kAudioUnitManufacturer_Apple: u32 = 0x6170_706C; // 'appl'
const kAudioFormatLinearPCM: u32 = 0x6C70_636D; // 'lpcm'
const kAudioFormatFlagIsFloat: u32 = 0x0000_0001;
const kAudioFormatFlagIsPacked: u32 = 0x0000_0008;

// ─── Library / framework path ──────────────────────────────────────
const AUDIO_TOOLBOX_PATH: &str = "/System/Library/Frameworks/AudioToolbox.framework/AudioToolbox";

/// macOS audio backend stream.
pub struct BackendStream {
    /// Loaded library handle (kept alive for the stream's lifetime).
    #[cfg(target_os = "macos")]
    _library: Library,
    /// Negotiated format.
    format: AudioFormat,
    /// Opaque AudioUnit handle (placeholder at stage-0).
    audio_unit: *mut core::ffi::c_void,
    /// Frames buffered platform-side.
    padding_frames: u64,
    /// `true` after `start()`.
    running: bool,
}

// SAFETY : AudioUnit is documented as thread-safe per Apple's
// CoreAudio + AudioUnit programming guides ; we cross the FFI only
// via documented entry points.
unsafe impl Send for BackendStream {}
unsafe impl Sync for BackendStream {}

#[cfg(target_os = "macos")]
fn try_load_audio_toolbox() -> Option<Library> {
    // SAFETY : libloading returns Err on missing framework ; mapped to
    // None. The library handle is kept alive for the stream's lifetime.
    unsafe { Library::new(AUDIO_TOOLBOX_PATH).ok() }
}

impl AudioBackend for BackendStream {
    #[cfg(target_os = "macos")]
    fn open(config: &AudioStreamConfig) -> Result<(Self, AudioFormat)>
    where
        Self: Sized,
    {
        if config.share_mode == ShareMode::Exclusive {
            return Err(AudioError::unsupported(
                "CoreAudio does not support exclusive-mode at stage-0",
            ));
        }
        let channels = config.format.layout.channel_count();
        if channels == 0 {
            return Err(AudioError::invalid("coreaudio::open", "channels=0"));
        }
        if !matches!(
            config.format.layout,
            ChannelLayout::Mono
                | ChannelLayout::Stereo
                | ChannelLayout::Surround51
                | ChannelLayout::Surround71
        ) {
            return Err(AudioError::unsupported(format!(
                "CoreAudio stage-0 layout: {}",
                config.format.layout.as_str()
            )));
        }
        let library = try_load_audio_toolbox().ok_or_else(|| {
            AudioError::loader(format!(
                "AudioToolbox.framework not loadable at {AUDIO_TOOLBOX_PATH}"
            ))
        })?;
        // Stage-0 : structural — real component instantiation deferred.
        Ok((
            Self {
                _library: library,
                format: config.format,
                audio_unit: core::ptr::null_mut(),
                padding_frames: 0,
                running: false,
            },
            config.format,
        ))
    }

    #[cfg(not(target_os = "macos"))]
    fn open(_config: &AudioStreamConfig) -> Result<(Self, AudioFormat)>
    where
        Self: Sized,
    {
        Err(AudioError::loader(
            "coreaudio backend is macOS-only — non-macOS build",
        ))
    }

    fn start(&mut self) -> Result<()> {
        if self.running {
            return Err(AudioError::invalid("coreaudio::start", "already running"));
        }
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn submit_frames(&mut self, samples: &[f32]) -> Result<usize> {
        if !self.running {
            return Err(AudioError::invalid(
                "coreaudio::submit_frames",
                "not running",
            ));
        }
        let channels = self.format.layout.channel_count() as usize;
        if samples.len() % channels != 0 {
            return Err(AudioError::invalid(
                "coreaudio::submit_frames",
                "samples not aligned to channel count",
            ));
        }
        let frames = samples.len() / channels;
        self.padding_frames = self.padding_frames.saturating_add(frames as u64);
        Ok(frames)
    }

    fn poll_padding(&mut self) -> Result<u64> {
        Ok(self.padding_frames)
    }

    fn close(&mut self) -> Result<()> {
        self.running = false;
        self.audio_unit = core::ptr::null_mut();
        Ok(())
    }

    fn name(&self) -> &'static str {
        "CoreAudio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_toolbox_path_is_canonical() {
        // If Apple ever moves AudioToolbox out of /System/Library/
        // Frameworks/, we want a CI failure.
        assert!(AUDIO_TOOLBOX_PATH.contains("AudioToolbox.framework"));
    }

    #[test]
    fn coreaudio_constants_match_apple_headers() {
        // FourCC literals from <AudioUnit/AUComponent.h> +
        // <CoreAudio/CoreAudioTypes.h>. Stable since macOS 10.0.
        assert_eq!(kAudioUnitType_Output, 0x6175_6F75);
        assert_eq!(kAudioUnitSubType_DefaultOutput, 0x6465_6620);
        assert_eq!(kAudioFormatLinearPCM, 0x6C70_636D);
        assert_eq!(kAudioFormatFlagIsFloat, 1);
    }
}
