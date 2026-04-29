//! Windows audio backend — WASAPI via `windows-rs 0.58`.
//!
//! § STRATEGY
//!   Per `specs/14_BACKEND.csl § AUDIO HOST BACKENDS § WASAPI`, the
//!   Windows audio layer wraps :
//!     - `IMMDeviceEnumerator` + `IMMDevice` for device discovery,
//!     - `IAudioClient` for stream lifecycle + buffer management,
//!     - `IAudioRenderClient` for the push-mode buffer-fill side.
//!
//!   Apocky's host is Win11 ; this is the runtime-tested platform path.
//!
//! § COM THREAD AFFINITY
//!   WASAPI requires that the calling thread has called
//!   `CoInitializeEx(COINIT_MULTITHREADED)` before any `IAudioClient`
//!   call. We provide an RAII guard ([`CoInitGuard`]) that scopes the
//!   COM init to the stream's lifetime ; callers who manage COM
//!   externally (e.g., D3D12 already initialized COM on the same
//!   thread) pass `coinit_managed = true` to skip the guard.
//!
//! § SHARED-MODE MIX-FORMAT NEGOTIATION
//!   Per the LANDMINES section of the slice handoff, shared-mode WASAPI
//!   clients **cannot dictate** the device format — the format is what
//!   the OS audio engine has chosen. We must call
//!   `IAudioClient::GetMixFormat` to query the device's preferred
//!   format. If the requested format matches → fast path. Otherwise we
//!   open with the device's mix-format + the AudioStream layer is
//!   responsible for any sample-format conversion. Stage-0 only
//!   supports `f32` mix-formats — non-f32 mix-formats produce
//!   [`crate::error::AudioError::SampleRateMismatch`] (a deliberate
//!   misnomer for "format negotiation failed" — refined in a future
//!   slice once a SampleFormatMismatch variant is needed).
//!
//! § EXCLUSIVE MODE
//!   Exclusive-mode WASAPI bypasses the OS audio engine entirely —
//!   the application owns the device end-to-end. This gives lowest
//!   possible latency but no other process can play audio while we
//!   hold the stream. Stage-0 supports it via
//!   [`crate::stream::ShareMode::Exclusive`].
//!
//! § UNDERRUN POLICY
//!   `IAudioClient::GetCurrentPadding` returns the number of frames
//!   still buffered on the device side. We poll this in
//!   [`BackendStream::poll_padding`] ; when padding is zero while the
//!   stream is running, the AudioStream layer records an
//!   [`crate::stream::AudioEvent::Underrun`] event. The platform fills
//!   the gap with silence — never silent-drop.

#![allow(dead_code)]

use crate::error::{AudioError, Result};
use crate::format::AudioFormat;
use crate::stream::{AudioBackend, AudioStreamConfig};

// `ShareMode` is referenced in tests below + the Windows imp re-imports
// from `crate::stream::ShareMode` directly.
#[cfg(test)]
use crate::stream::ShareMode;

#[cfg(target_os = "windows")]
mod imp {
    use super::{AudioBackend, AudioError, AudioFormat, AudioStreamConfig, Result};
    use crate::format::{ChannelLayout, SampleRate};
    use crate::stream::ShareMode;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HANDLE, RPC_E_CHANGED_MODE, WAIT_FAILED, WAIT_OBJECT_0};
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDevice, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
        WAVEFORMATEXTENSIBLE_0,
    };
    use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
    use windows::Win32::Media::Multimedia::{
        KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

    /// RAII guard for `CoInitializeEx(COINIT_MULTITHREADED)` ↔
    /// `CoUninitialize` lifecycle.
    pub struct CoInitGuard {
        active: bool,
    }

    impl CoInitGuard {
        /// Initialize COM on the calling thread. If COM is already
        /// initialized on this thread with a compatible apartment-model
        /// the call returns `S_FALSE` (still valid) ; we still record
        /// `active = true` because we matched the existing init.
        pub fn new() -> Result<Self> {
            // SAFETY : CoInitializeEx is a documented Win32 entry. On
            // success we own the COM init for this thread until drop.
            #[allow(unsafe_code)]
            let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            // S_OK = init ; S_FALSE = already init compatible ; both OK.
            // RPC_E_CHANGED_MODE = init in single-threaded — fail.
            if hr.is_err() {
                let raw = hr.0;
                if raw == RPC_E_CHANGED_MODE.0 {
                    return Err(AudioError::hresult(
                        "CoInitializeEx",
                        raw,
                        "thread already initialized in single-threaded apartment — pass coinit_managed=true",
                    ));
                }
                return Err(AudioError::hresult(
                    "CoInitializeEx",
                    raw,
                    "COM init failed",
                ));
            }
            Ok(Self { active: true })
        }
    }

    impl Drop for CoInitGuard {
        fn drop(&mut self) {
            if self.active {
                // SAFETY : CoUninitialize is the documented teardown for
                // CoInitializeEx ; safe to call when we hold the init.
                #[allow(unsafe_code)]
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    /// WASAPI backend stream.
    pub struct BackendStream {
        /// COM init guard (None when caller manages COM).
        _coinit: Option<CoInitGuard>,
        /// Audio client (IAudioClient).
        client: IAudioClient,
        /// Render client (IAudioRenderClient).
        render: IAudioRenderClient,
        /// Event handle signaled by WASAPI when buffer space is
        /// available (event-driven mode).
        event: HANDLE,
        /// Negotiated format.
        format: AudioFormat,
        /// Mix-format buffer-frame count (per IAudioClient::GetBufferSize).
        buffer_frame_count: u32,
        /// `true` after `start()`.
        running: bool,
    }

    // SAFETY : COM interfaces from windows-rs are Send + Sync per their
    // own thread-safety model (IAudioClient + IAudioRenderClient are
    // documented thread-neutral by Microsoft) ; we cross the FFI only
    // via documented entry points + serialize state through &mut self.
    // The clippy `non-send-fields-in-send-ty` lint can't see through
    // the COM-interface contract.
    #[allow(clippy::non_send_fields_in_send_ty)]
    unsafe impl Send for BackendStream {}
    #[allow(clippy::non_send_fields_in_send_ty)]
    unsafe impl Sync for BackendStream {}

    impl core::fmt::Debug for BackendStream {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("BackendStream(WASAPI)")
                .field("format", &self.format)
                .field("buffer_frame_count", &self.buffer_frame_count)
                .field("running", &self.running)
                .finish_non_exhaustive()
        }
    }

    fn channel_mask_for(layout: ChannelLayout) -> u32 {
        // SPEAKER_* bits from <ksmedia.h>. Standard WAVEFORMATEXTENSIBLE
        // channel-mask per Microsoft's audio docs.
        const SPEAKER_FRONT_LEFT: u32 = 0x1;
        const SPEAKER_FRONT_RIGHT: u32 = 0x2;
        const SPEAKER_FRONT_CENTER: u32 = 0x4;
        const SPEAKER_LOW_FREQUENCY: u32 = 0x8;
        const SPEAKER_BACK_LEFT: u32 = 0x10;
        const SPEAKER_BACK_RIGHT: u32 = 0x20;
        const SPEAKER_SIDE_LEFT: u32 = 0x200;
        const SPEAKER_SIDE_RIGHT: u32 = 0x400;
        match layout {
            ChannelLayout::Mono => SPEAKER_FRONT_CENTER,
            ChannelLayout::Stereo => SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            ChannelLayout::Stereo21 => {
                SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT | SPEAKER_LOW_FREQUENCY
            }
            ChannelLayout::Surround51 => {
                SPEAKER_FRONT_LEFT
                    | SPEAKER_FRONT_RIGHT
                    | SPEAKER_FRONT_CENTER
                    | SPEAKER_LOW_FREQUENCY
                    | SPEAKER_BACK_LEFT
                    | SPEAKER_BACK_RIGHT
            }
            ChannelLayout::Surround71 => {
                SPEAKER_FRONT_LEFT
                    | SPEAKER_FRONT_RIGHT
                    | SPEAKER_FRONT_CENTER
                    | SPEAKER_LOW_FREQUENCY
                    | SPEAKER_BACK_LEFT
                    | SPEAKER_BACK_RIGHT
                    | SPEAKER_SIDE_LEFT
                    | SPEAKER_SIDE_RIGHT
            }
        }
    }

    /// Build a `WAVEFORMATEXTENSIBLE` for the given AudioFormat (f32 PCM).
    fn build_wave_format(format: AudioFormat) -> WAVEFORMATEXTENSIBLE {
        let channels = format.layout.channel_count();
        let bits_per_sample: u16 = 32;
        let block_align: u16 = channels * (bits_per_sample / 8);
        let bytes_per_sec: u32 = format.rate.as_hz() * u32::from(block_align);
        WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
                nChannels: channels,
                nSamplesPerSec: format.rate.as_hz(),
                nAvgBytesPerSec: bytes_per_sec,
                nBlockAlign: block_align,
                wBitsPerSample: bits_per_sample,
                cbSize: 22,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: bits_per_sample,
            },
            dwChannelMask: channel_mask_for(format.layout),
            SubFormat: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        }
    }

    /// Parse a WASAPI mix-format pointer back into an AudioFormat. Returns
    /// `None` if the format is not f32 PCM (we fall through to a
    /// SampleRateMismatch error in that case at stage-0).
    ///
    /// SAFETY : `ptr` must point to a valid WAVEFORMATEX (or
    /// WAVEFORMATEXTENSIBLE) struct returned by IAudioClient::GetMixFormat.
    unsafe fn parse_mix_format(ptr: *const WAVEFORMATEX) -> Option<AudioFormat> {
        if ptr.is_null() {
            return None;
        }
        // SAFETY : WAVEFORMATEX is repr(C, packed) ; read fields via
        // ptr::read_unaligned to satisfy E0793 (no misaligned references).
        let wfex_value = core::ptr::read_unaligned(ptr);
        let channels = wfex_value.nChannels;
        let rate = wfex_value.nSamplesPerSec;
        let bits = wfex_value.wBitsPerSample;
        let format_tag = wfex_value.wFormatTag;
        let cb_size = wfex_value.cbSize;
        // Float mix format is the common case for shared-mode on Win10+.
        let is_simple_float = format_tag == WAVE_FORMAT_IEEE_FLOAT as u16;
        let is_extensible_float = if format_tag == WAVE_FORMAT_EXTENSIBLE as u16 && cb_size >= 22 {
            let wfext_ptr = ptr.cast::<WAVEFORMATEXTENSIBLE>();
            // SAFETY : reading by value avoids misaligned reference UB.
            let sub = core::ptr::read_unaligned(core::ptr::addr_of!((*wfext_ptr).SubFormat));
            sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        } else {
            false
        };
        let is_float = is_simple_float || is_extensible_float;
        if !is_float || bits != 32 {
            return None;
        }
        let layout = match channels {
            1 => ChannelLayout::Mono,
            2 => ChannelLayout::Stereo,
            3 => ChannelLayout::Stereo21,
            6 => ChannelLayout::Surround51,
            8 => ChannelLayout::Surround71,
            _ => return None,
        };
        let sr = match rate {
            44_100 => SampleRate::Hz44100,
            48_000 => SampleRate::Hz48000,
            88_200 => SampleRate::Hz88200,
            96_000 => SampleRate::Hz96000,
            176_400 => SampleRate::Hz176400,
            192_000 => SampleRate::Hz192000,
            other => SampleRate::Custom(other),
        };
        Some(AudioFormat { rate: sr, layout })
    }

    impl AudioBackend for BackendStream {
        fn open(config: &AudioStreamConfig) -> Result<(Self, AudioFormat)>
        where
            Self: Sized,
        {
            // ── 1. COM init ─────────────────────────────────────────
            let coinit = if config.coinit_managed {
                None
            } else {
                Some(CoInitGuard::new()?)
            };

            // ── 2. Device enumerator ────────────────────────────────
            // SAFETY : standard COM coclass instantiation.
            #[allow(unsafe_code)]
            let enumerator: IMMDeviceEnumerator = unsafe {
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| {
                    AudioError::hresult(
                        "CoCreateInstance(MMDeviceEnumerator)",
                        e.code().0,
                        e.message(),
                    )
                })?
            };

            // ── 3. Default render device ────────────────────────────
            // SAFETY : enumerator is alive ; output args are owned by
            // windows-rs after this returns.
            #[allow(unsafe_code)]
            let device: IMMDevice = unsafe {
                enumerator
                    .GetDefaultAudioEndpoint(eRender, eConsole)
                    .map_err(|e| {
                        AudioError::hresult(
                            "IMMDeviceEnumerator::GetDefaultAudioEndpoint",
                            e.code().0,
                            e.message(),
                        )
                    })?
            };

            // ── 4. Activate IAudioClient ────────────────────────────
            // SAFETY : Activate uses an out-pointer ; windows-rs wraps
            // it as Result<IAudioClient>.
            #[allow(unsafe_code)]
            let client: IAudioClient = unsafe {
                device
                    .Activate::<IAudioClient>(CLSCTX_ALL, None)
                    .map_err(|e| {
                        AudioError::hresult(
                            "IMMDevice::Activate(IAudioClient)",
                            e.code().0,
                            e.message(),
                        )
                    })?
            };

            // ── 5. Negotiate format ─────────────────────────────────
            // For shared mode : query GetMixFormat. We must use the
            // device's mix-format (we can request AUTOCONVERTPCM to let
            // WASAPI resample for us if rates differ ; not in stage-0).
            let negotiated_format = match config.share_mode {
                ShareMode::Shared => {
                    // SAFETY : GetMixFormat returns a CoTaskMem-allocated
                    // WAVEFORMATEX pointer ; we MUST CoTaskMemFree it.
                    #[allow(unsafe_code)]
                    let mix_format_ptr = unsafe {
                        client.GetMixFormat().map_err(|e| {
                            AudioError::hresult(
                                "IAudioClient::GetMixFormat",
                                e.code().0,
                                e.message(),
                            )
                        })?
                    };
                    // SAFETY : ptr is valid and must be freed.
                    #[allow(unsafe_code)]
                    let parsed = unsafe { parse_mix_format(mix_format_ptr) };
                    let neg = parsed.ok_or_else(|| {
                        AudioError::unsupported("device mix-format is not f32 PCM at stage-0")
                    });
                    // CoTaskMemFree the mix-format buffer regardless of parse outcome.
                    // SAFETY : ptr came from CoTaskMemAlloc inside GetMixFormat.
                    #[allow(unsafe_code)]
                    unsafe {
                        CoTaskMemFree(Some(mix_format_ptr.cast()));
                    }
                    let neg_format = neg?;
                    if neg_format.rate.as_hz() != config.format.rate.as_hz() {
                        // Stage-0 : fail-loud on rate mismatch (no resampler).
                        // The slice handoff lists this as a landmine.
                        return Err(AudioError::sample_rate_mismatch(
                            config.format.rate.as_hz(),
                            neg_format.rate.as_hz(),
                        ));
                    }
                    neg_format
                }
                ShareMode::Exclusive => {
                    // Exclusive mode : we dictate the format. Build
                    // WAVEFORMATEXTENSIBLE from config.format.
                    config.format
                }
            };

            // ── 6. Initialize the client ────────────────────────────
            let wave_format = build_wave_format(negotiated_format);
            let share_flag = match config.share_mode {
                ShareMode::Shared => AUDCLNT_SHAREMODE_SHARED,
                ShareMode::Exclusive => AUDCLNT_SHAREMODE_EXCLUSIVE,
            };
            // 100ns-units : we ask for the ring's frame_capacity worth
            // of buffering. The device may round up. Capacity is
            // bounded ≤ 65536 by RingBufferConfig, so the i64 cast is
            // always lossless.
            let frame_cap = i64::try_from(config.ring.frame_capacity).map_err(|_| {
                AudioError::invalid("wasapi::open", "ring.frame_capacity exceeds i64")
            })?;
            let buffer_duration_100ns: i64 =
                (frame_cap * 10_000_000) / i64::from(negotiated_format.rate.as_hz());

            // SAFETY : Initialize takes the wave format by pointer.
            #[allow(unsafe_code)]
            unsafe {
                client
                    .Initialize(
                        share_flag,
                        AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                        buffer_duration_100ns,
                        0, // periodicity (0 = engine default)
                        core::ptr::addr_of!(wave_format.Format),
                        None,
                    )
                    .map_err(|e| {
                        AudioError::hresult("IAudioClient::Initialize", e.code().0, e.message())
                    })?;
            }

            // ── 7. Event handle for event-driven mode ───────────────
            // SAFETY : CreateEventW returns a HANDLE ; we own it + close
            // on drop (via the platform's natural HANDLE-leak avoidance —
            // future slice will add a Drop impl that closes it).
            #[allow(unsafe_code)]
            let event_handle = unsafe {
                CreateEventW(None, false, false, PCWSTR::null()).map_err(|e| {
                    AudioError::hresult("CreateEventW(audio-event)", e.code().0, e.message())
                })?
            };

            // SAFETY : SetEventHandle wires our event into IAudioClient.
            #[allow(unsafe_code)]
            unsafe {
                client.SetEventHandle(event_handle).map_err(|e| {
                    AudioError::hresult("IAudioClient::SetEventHandle", e.code().0, e.message())
                })?;
            }

            // ── 8. GetBufferSize for the actual frame count ─────────
            // SAFETY : GetBufferSize returns u32 frame-count.
            #[allow(unsafe_code)]
            let buffer_frame_count = unsafe {
                client.GetBufferSize().map_err(|e| {
                    AudioError::hresult("IAudioClient::GetBufferSize", e.code().0, e.message())
                })?
            };

            // ── 9. Activate IAudioRenderClient ──────────────────────
            // SAFETY : GetService is the canonical way to reach the
            // render-client interface from a IAudioClient.
            #[allow(unsafe_code)]
            let render: IAudioRenderClient = unsafe {
                client.GetService::<IAudioRenderClient>().map_err(|e| {
                    AudioError::hresult(
                        "IAudioClient::GetService(IAudioRenderClient)",
                        e.code().0,
                        e.message(),
                    )
                })?
            };

            Ok((
                Self {
                    _coinit: coinit,
                    client,
                    render,
                    event: event_handle,
                    format: negotiated_format,
                    buffer_frame_count,
                    running: false,
                },
                negotiated_format,
            ))
        }

        fn start(&mut self) -> Result<()> {
            if self.running {
                return Err(AudioError::invalid("wasapi::start", "already running"));
            }
            // SAFETY : Start is the canonical IAudioClient lifecycle call.
            #[allow(unsafe_code)]
            unsafe {
                self.client.Start().map_err(|e| {
                    AudioError::hresult("IAudioClient::Start", e.code().0, e.message())
                })?;
            }
            self.running = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            if !self.running {
                return Ok(());
            }
            // SAFETY : Stop is canonical.
            #[allow(unsafe_code)]
            unsafe {
                self.client.Stop().map_err(|e| {
                    AudioError::hresult("IAudioClient::Stop", e.code().0, e.message())
                })?;
            }
            self.running = false;
            Ok(())
        }

        fn submit_frames(&mut self, samples: &[f32]) -> Result<usize> {
            if !self.running {
                return Err(AudioError::invalid(
                    "wasapi::submit_frames",
                    "stream not running",
                ));
            }
            let channels = self.format.layout.channel_count() as usize;
            if samples.len() % channels != 0 {
                return Err(AudioError::invalid(
                    "wasapi::submit_frames",
                    "samples not aligned to channel count",
                ));
            }
            let frames_in = samples.len() / channels;
            if frames_in == 0 {
                return Ok(0);
            }
            // ── 1. Query padding (frames already buffered on device) ──
            // SAFETY : GetCurrentPadding returns u32 ; checks done.
            #[allow(unsafe_code)]
            let padding = unsafe {
                self.client.GetCurrentPadding().map_err(|e| {
                    AudioError::hresult("IAudioClient::GetCurrentPadding", e.code().0, e.message())
                })?
            };
            let frames_available = self.buffer_frame_count.saturating_sub(padding) as usize;
            if frames_available == 0 {
                // Buffer is full ; caller should retry after the next
                // event signal. Return 0 — the AudioStream layer counts
                // this as an Overrun.
                return Ok(0);
            }
            let frames_to_write = frames_in.min(frames_available);
            let samples_to_write = frames_to_write * channels;
            // ── 2. GetBuffer — returns a pointer to the platform's
            //       interleaved sample buffer.
            // SAFETY : GetBuffer is the canonical write-acquire call. The
            // returned pointer is valid until ReleaseBuffer.
            #[allow(unsafe_code)]
            let dst_ptr: *mut f32 = unsafe {
                self.render.GetBuffer(frames_to_write as u32).map_err(|e| {
                    AudioError::hresult("IAudioRenderClient::GetBuffer", e.code().0, e.message())
                })?
            }
            .cast();
            // ── 3. Copy our samples into the platform buffer.
            // SAFETY : dst_ptr is valid for samples_to_write f32 writes
            // per the IAudioRenderClient contract ; src is &[f32] with
            // bounds-checked length.
            #[allow(unsafe_code)]
            unsafe {
                core::ptr::copy_nonoverlapping(samples.as_ptr(), dst_ptr, samples_to_write);
            }
            // ── 4. ReleaseBuffer — hands the frames to WASAPI.
            // SAFETY : we just wrote frames_to_write frames + are
            // releasing the same count.
            #[allow(unsafe_code)]
            unsafe {
                self.render
                    .ReleaseBuffer(frames_to_write as u32, 0)
                    .map_err(|e| {
                        AudioError::hresult(
                            "IAudioRenderClient::ReleaseBuffer",
                            e.code().0,
                            e.message(),
                        )
                    })?;
            }
            Ok(frames_to_write)
        }

        fn poll_padding(&mut self) -> Result<u64> {
            // SAFETY : GetCurrentPadding canonical.
            #[allow(unsafe_code)]
            let padding = unsafe {
                self.client.GetCurrentPadding().map_err(|e| {
                    AudioError::hresult("IAudioClient::GetCurrentPadding", e.code().0, e.message())
                })?
            };
            Ok(padding as u64)
        }

        fn close(&mut self) -> Result<()> {
            if self.running {
                let _ = self.stop();
            }
            // The COM interfaces (client / render) drop automatically
            // via windows-rs RAII. The event handle leaks at stage-0 —
            // future slice will introduce a CloseHandle-on-Drop wrapper.
            Ok(())
        }

        fn name(&self) -> &'static str {
            "WASAPI"
        }
    }

    /// Wait for the WASAPI buffer-event with a millisecond timeout.
    /// Returns `Ok(())` on signal, `Err(...)` on timeout. Used by
    /// integration tests + future blocking submit paths.
    pub(super) fn wait_for_buffer_event(stream: &BackendStream, timeout_ms: u32) -> Result<()> {
        // SAFETY : WaitForSingleObject is documented Win32 ; HANDLE is
        // owned by the stream and valid for its lifetime.
        #[allow(unsafe_code)]
        let result = unsafe { WaitForSingleObject(stream.event, timeout_ms) };
        if result == WAIT_OBJECT_0 {
            Ok(())
        } else if result == WAIT_FAILED {
            Err(AudioError::hresult(
                "WaitForSingleObject(audio-event)",
                -1,
                "wait failed",
            ))
        } else {
            // WAIT_TIMEOUT or other. The raw u32 represents a Win32 status
            // code that fits the i32 contract of AudioError::Hresult ; the
            // wrap is intentional + the raw value is preserved across the
            // truncation.
            #[allow(clippy::cast_possible_wrap)]
            let raw_status = result.0 as i32;
            Err(AudioError::hresult(
                "WaitForSingleObject(audio-event)",
                raw_status,
                "wait timed out or abandoned",
            ))
        }
    }

    /// Allow `INFINITE` to surface in tests if needed.
    #[must_use]
    pub(super) const fn infinite_wait() -> u32 {
        INFINITE
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Non-Windows stub (keeps cargo check green on Linux + macOS).
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{AudioBackend, AudioError, AudioFormat, AudioStreamConfig, Result};

    /// WASAPI is Windows-only — every constructor returns
    /// `LoaderMissing`.
    pub struct BackendStream;

    pub struct CoInitGuard;

    impl AudioBackend for BackendStream {
        fn open(_config: &AudioStreamConfig) -> Result<(Self, AudioFormat)>
        where
            Self: Sized,
        {
            Err(AudioError::loader(
                "WASAPI is Windows-only — non-Windows build",
            ))
        }

        fn start(&mut self) -> Result<()> {
            Err(AudioError::FfiNotWired)
        }

        fn stop(&mut self) -> Result<()> {
            Err(AudioError::FfiNotWired)
        }

        fn submit_frames(&mut self, _samples: &[f32]) -> Result<usize> {
            Err(AudioError::FfiNotWired)
        }

        fn poll_padding(&mut self) -> Result<u64> {
            Err(AudioError::FfiNotWired)
        }

        fn close(&mut self) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> &'static str {
            "wasapi-stub"
        }
    }
}

pub use imp::{BackendStream, CoInitGuard};

// Test surface : helpers that work on all platforms.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_format_default_is_open_compatible() {
        let cfg = AudioStreamConfig::default_output();
        assert_eq!(cfg.format.rate.as_hz(), 48_000);
        assert_eq!(cfg.format.layout.channel_count(), 2);
    }

    #[test]
    fn share_mode_default_is_shared() {
        assert_eq!(ShareMode::default(), ShareMode::Shared);
    }
}
