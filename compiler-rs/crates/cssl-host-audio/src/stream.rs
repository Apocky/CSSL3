//! AudioStream surface — push-mode output stream backed by a platform layer.
//!
//! § DESIGN
//!   `AudioStream` is the user-facing surface. Internally it holds :
//!     - an [`crate::ring::RingBuffer`] for pending samples,
//!     - a platform-specific stream handle (WASAPI / ALSA / CoreAudio),
//!     - counter discipline (frames_submitted / frames_dropped /
//!       underrun_count / sample_clock),
//!     - an event queue ([`AudioEvent`]) for underruns + diagnostic
//!       messages.
//!
//!   The platform layer is pluggable via [`AudioBackend`]. Stage-0 has
//!   a single backend selected by `cfg(target_os = ...)` ; future
//!   slices may add multiple backends per platform (e.g., DirectSound
//!   on Windows in addition to WASAPI).
//!
//! § PUSH MODE LIFECYCLE
//!   1. [`AudioStream::open_default_output`] — opens the platform's
//!      default output device, negotiates format, allocates ring.
//!   2. [`AudioStream::start`] — starts the platform stream.
//!   3. Render loop — caller invokes [`AudioStream::submit_frames`]
//!      with `&[f32]` interleaved buffers ; the stream pushes into
//!      the ring + the platform layer drains the ring into the device.
//!   4. [`AudioStream::stop`] — stops the platform stream.
//!   5. drop — closes the platform stream + frees resources.
//!
//! § COUNTER DISCIPLINE
//!   - `frames_submitted` — total frames the caller has pushed.
//!   - `frames_dropped`   — frames lost to a full ring (back-pressure).
//!   - `underrun_count`   — times the device drained faster than refill.
//!   - `sample_clock`     — frame-position of the next sample to submit
//!                          (monotonic). Used by tone generators to
//!                          phase-track across submit calls.
//!
//!   These mirror the d3d12 telemetry counter precedent (T11-D66).

use crate::error::{AudioError, Result};
use crate::format::AudioFormat;
use crate::platform::active as backend;
use crate::ring::{RingBuffer, RingBufferConfig};

/// Sharing mode for the output stream.
///
/// On WASAPI this maps to `AUDCLNT_SHAREMODE_SHARED` /
/// `AUDCLNT_SHAREMODE_EXCLUSIVE`. On ALSA / CoreAudio shared is the only
/// supported mode (devices are already mixed by the OS) ; passing
/// `Exclusive` on those platforms returns `NotSupported`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShareMode {
    /// Shared mode — the OS audio engine mixes us with other streams.
    /// Default on every platform. Format must match the device mix-format
    /// or be convertible at the boundary.
    Shared,
    /// Exclusive mode — we own the device end-to-end. Lowest latency
    /// but no other process can play audio while we hold the stream.
    /// Windows only.
    Exclusive,
}

impl ShareMode {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Exclusive => "exclusive",
        }
    }
}

impl Default for ShareMode {
    fn default() -> Self {
        Self::Shared
    }
}

/// Configuration for opening an `AudioStream`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioStreamConfig {
    /// Desired audio format (rate + layout).
    pub format: AudioFormat,
    /// Ring buffer config (latency budget).
    pub ring: RingBufferConfig,
    /// Sharing mode (Shared default).
    pub share_mode: ShareMode,
    /// Set to `true` if the caller has already initialized COM /
    /// platform-global state and wants us to skip the RAII guard.
    /// Stage-0 only meaningful on Windows.
    pub coinit_managed: bool,
}

impl AudioStreamConfig {
    /// Default config : 48 kHz stereo, 256-frame ring, shared mode,
    /// internal COM init.
    #[must_use]
    pub fn default_output() -> Self {
        Self {
            format: AudioFormat::default_output(),
            ring: RingBufferConfig::default_latency(),
            share_mode: ShareMode::Shared,
            coinit_managed: false,
        }
    }
}

impl Default for AudioStreamConfig {
    fn default() -> Self {
        Self::default_output()
    }
}

/// Events emitted by the audio stream — surfaced via
/// [`AudioStream::drain_events`].
#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    /// Stream has started (platform start succeeded).
    Started {
        /// Negotiated format (may differ from requested if shared-mode
        /// mix-format conversion happened).
        format: AudioFormat,
    },
    /// Stream has stopped.
    Stopped,
    /// Buffer underrun — the device drained `frames_lost` frames of
    /// silence because the ring was empty. Counter incremented.
    Underrun {
        /// Frame-position at which the underrun occurred.
        frame_position: u64,
        /// How many frames of silence were inserted.
        frames_lost: u64,
    },
    /// Buffer overrun — the caller pushed faster than the device
    /// could drain ; `frames_dropped` were rejected at the ring's
    /// capacity boundary. Counter incremented.
    Overrun {
        /// Frame-position at which the overrun occurred.
        frame_position: u64,
        /// How many frames were rejected.
        frames_dropped: u64,
    },
    /// Telemetry-relevant diagnostic (string-tagged).
    Diagnostic {
        /// Free-form tag (e.g., `"format-negotiation-fallback"`).
        tag: String,
        /// Free-form detail.
        detail: String,
    },
}

/// Counter discipline — incremented by stream operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioCounters {
    /// Total frames the caller has pushed via `submit_frames`.
    pub frames_submitted: u64,
    /// Frames dropped because the ring was full (back-pressure).
    pub frames_dropped: u64,
    /// Number of underrun events recorded.
    pub underrun_count: u64,
    /// Sample-clock — frame-position of the next sample to be submitted.
    pub sample_clock: u64,
}

/// Backend trait — the platform-specific layer implements this.
///
/// Each platform impl in `crate::platform::*` exposes a `BackendStream`
/// type that implements this trait. The trait deliberately stays tiny :
/// open / start / stop / submit_frames / poll_padding / close.
///
/// At stage-0 the backend layer holds its own internal ring + the
/// AudioStream owns the user-facing ring. A future slice will collapse
/// these into a single shared ring with atomic cursors so the platform
/// callback (CoreAudio) + the producer thread can run lock-free.
pub trait AudioBackend {
    /// Open the platform's default output device with `config`.
    /// Returns the negotiated format (may differ from the requested
    /// format on shared-mode platforms).
    fn open(config: &AudioStreamConfig) -> Result<(Self, AudioFormat)>
    where
        Self: Sized;
    /// Start the stream. After this call, `submit_frames` produces
    /// audible output.
    fn start(&mut self) -> Result<()>;
    /// Stop the stream. Buffers may still be drained on the platform
    /// side ; subsequent `submit_frames` calls return `Err`.
    fn stop(&mut self) -> Result<()>;
    /// Submit `samples` (interleaved f32) to the platform. Returns the
    /// number of *frames* accepted. The platform may apply back-pressure ;
    /// callers are responsible for retrying or routing the rejected
    /// frames into an `Overrun` event.
    fn submit_frames(&mut self, samples: &[f32]) -> Result<usize>;
    /// Returns the number of *frames* currently buffered in the platform
    /// (used to detect underruns + monitor latency).
    fn poll_padding(&mut self) -> Result<u64>;
    /// Close the stream + release resources.
    fn close(&mut self) -> Result<()>;
    /// Backend-specific human-readable identifier (e.g., `"WASAPI"`,
    /// `"ALSA"`, `"CoreAudio"`, `"stub"`).
    fn name(&self) -> &'static str;
}

/// Push-mode audio output stream.
pub struct AudioStream {
    /// Negotiated format (after open).
    format: AudioFormat,
    /// Configured ring buffer. Mirrors what the platform layer holds ;
    /// at stage-0 used as a counter / fallback cache.
    #[allow(dead_code)]
    ring: RingBuffer,
    /// Platform backend instance.
    backend: backend::BackendStream,
    /// Counter discipline.
    counters: AudioCounters,
    /// Pending events to surface to the caller.
    events: Vec<AudioEvent>,
    /// `true` between `start` + `stop`.
    running: bool,
}

impl AudioStream {
    /// Open the platform's default output device with default config.
    pub fn open_default_output() -> Result<Self> {
        Self::open(&AudioStreamConfig::default_output())
    }

    /// Open the platform's default output device with the given config.
    pub fn open(config: &AudioStreamConfig) -> Result<Self> {
        // Validate config before crossing the FFI boundary.
        if config.format.layout.channel_count() == 0 {
            return Err(AudioError::invalid(
                "AudioStream::open",
                "format has 0 channels",
            ));
        }
        let (mut backend_stream, negotiated_format) = backend::BackendStream::open(config)?;
        // Surface a diagnostic if the platform negotiated a different
        // format than we asked for.
        let mut events = Vec::new();
        if negotiated_format != config.format {
            events.push(AudioEvent::Diagnostic {
                tag: "format-negotiation".to_string(),
                detail: format!("requested {:?}, got {:?}", config.format, negotiated_format),
            });
        }
        let ring = RingBuffer::new(negotiated_format, config.ring).map_err(|e| {
            // Best-effort cleanup — close the backend we just opened.
            let _ = backend_stream.close();
            e
        })?;
        Ok(Self {
            format: negotiated_format,
            ring,
            backend: backend_stream,
            counters: AudioCounters::default(),
            events,
            running: false,
        })
    }

    /// Start the stream — audio flows after this returns.
    pub fn start(&mut self) -> Result<()> {
        if self.running {
            return Err(AudioError::invalid(
                "AudioStream::start",
                "stream already running",
            ));
        }
        self.backend.start()?;
        self.running = true;
        self.events.push(AudioEvent::Started {
            format: self.format,
        });
        Ok(())
    }

    /// Stop the stream.
    pub fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        self.backend.stop()?;
        self.running = false;
        self.events.push(AudioEvent::Stopped);
        Ok(())
    }

    /// Submit `samples` (interleaved f32) to the stream. Returns the
    /// number of *frames* accepted. If the ring is full, the rejected
    /// frames are recorded as an [`AudioEvent::Overrun`] and counted
    /// in `frames_dropped`.
    ///
    /// Panics if `samples.len() % channels != 0`.
    pub fn submit_frames(&mut self, samples: &[f32]) -> Result<usize> {
        if !self.running {
            return Err(AudioError::invalid(
                "AudioStream::submit_frames",
                "stream not started — call start() first",
            ));
        }
        let channels = self.format.layout.channel_count() as usize;
        assert!(
            samples.len() % channels == 0,
            "submit_frames : samples.len() {} not divisible by channels {}",
            samples.len(),
            channels
        );
        let requested_frames = samples.len() / channels;
        let accepted = self.backend.submit_frames(samples)?;
        self.counters.frames_submitted += accepted as u64;
        self.counters.sample_clock += accepted as u64;
        if accepted < requested_frames {
            let dropped = (requested_frames - accepted) as u64;
            self.counters.frames_dropped += dropped;
            self.events.push(AudioEvent::Overrun {
                frame_position: self.counters.sample_clock,
                frames_dropped: dropped,
            });
        }
        Ok(accepted)
    }

    /// Poll platform-side padding (frames still buffered on the device).
    /// If padding is zero while running, treat as an underrun candidate.
    pub fn poll_padding(&mut self) -> Result<u64> {
        let padding = self.backend.poll_padding()?;
        if self.running && padding == 0 && self.counters.frames_submitted > 0 {
            // Record an underrun event ; padding == 0 means the device
            // drained everything we gave it. The platform layer is
            // responsible for inserting silence ; we just record the
            // observation.
            self.counters.underrun_count += 1;
            self.events.push(AudioEvent::Underrun {
                frame_position: self.counters.sample_clock,
                frames_lost: 0, // platform fills with silence ; count via polling delta
            });
        }
        Ok(padding)
    }

    /// Drain accumulated events. Returns + clears the internal queue.
    pub fn drain_events(&mut self) -> Vec<AudioEvent> {
        core::mem::take(&mut self.events)
    }

    /// Read-only counter snapshot.
    #[must_use]
    pub const fn counters(&self) -> AudioCounters {
        self.counters
    }

    /// The negotiated format for this stream.
    #[must_use]
    pub const fn format(&self) -> AudioFormat {
        self.format
    }

    /// Is the stream currently running ?
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Backend identifier (e.g., `"WASAPI"`).
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        if self.running {
            let _ = self.backend.stop();
        }
        let _ = self.backend.close();
    }
}

impl core::fmt::Debug for AudioStream {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AudioStream")
            .field("format", &self.format)
            .field("running", &self.running)
            .field("counters", &self.counters)
            .field("backend", &self.backend.name())
            .finish_non_exhaustive()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Capture-mode placeholder — see PRIME-DIRECTIVE.
// ═══════════════════════════════════════════════════════════════════════

/// Reserved type-name for future capture-mode (microphone) streams.
///
/// **Capture is deferred at stage-0** per PRIME-DIRECTIVE consent gate.
/// Calling [`AudioCaptureStream::open_default_input`] always returns
/// [`AudioError::CaptureNotImplemented`].
///
/// When capture lands, this type will require :
/// 1. A `consent: ConsentToken` parameter wired to a UI-affordance.
/// 2. The `{Sensitive<"microphone">}` effect on every fn that touches it.
/// 3. An audit-policy entry in the R18 telemetry-ring per session.
#[derive(Debug)]
pub struct AudioCaptureStream {
    _private: (),
}

impl AudioCaptureStream {
    /// Open the platform's default input device. **Always returns
    /// [`AudioError::CaptureNotImplemented`] at stage-0.**
    pub fn open_default_input() -> Result<Self> {
        Err(AudioError::CaptureNotImplemented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_mode_default_is_shared() {
        assert_eq!(ShareMode::default(), ShareMode::Shared);
    }

    #[test]
    fn share_mode_str_names() {
        assert_eq!(ShareMode::Shared.as_str(), "shared");
        assert_eq!(ShareMode::Exclusive.as_str(), "exclusive");
    }

    #[test]
    fn config_default_is_48k_stereo_256frame_shared() {
        let cfg = AudioStreamConfig::default();
        assert_eq!(cfg.format.rate.as_hz(), 48_000);
        assert_eq!(cfg.format.layout.channel_count(), 2);
        assert_eq!(cfg.ring.frame_capacity, 256);
        assert_eq!(cfg.share_mode, ShareMode::Shared);
        assert!(!cfg.coinit_managed);
    }

    #[test]
    fn audio_event_underrun_carries_position() {
        let e = AudioEvent::Underrun {
            frame_position: 1024,
            frames_lost: 32,
        };
        if let AudioEvent::Underrun {
            frame_position,
            frames_lost,
        } = e
        {
            assert_eq!(frame_position, 1024);
            assert_eq!(frames_lost, 32);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn audio_event_overrun_carries_dropped_count() {
        let e = AudioEvent::Overrun {
            frame_position: 2048,
            frames_dropped: 64,
        };
        if let AudioEvent::Overrun { frames_dropped, .. } = e {
            assert_eq!(frames_dropped, 64);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn audio_counters_default_all_zero() {
        let c = AudioCounters::default();
        assert_eq!(c.frames_submitted, 0);
        assert_eq!(c.frames_dropped, 0);
        assert_eq!(c.underrun_count, 0);
        assert_eq!(c.sample_clock, 0);
    }

    #[test]
    fn capture_stream_returns_consent_error() {
        let r = AudioCaptureStream::open_default_input();
        match r {
            Err(AudioError::CaptureNotImplemented) => {}
            other => panic!("wrong result: {other:?}"),
        }
    }
}
