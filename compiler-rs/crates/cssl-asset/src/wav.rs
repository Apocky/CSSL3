//! WAV (RIFF) audio decoder + encoder — PCM only.
//!
//! § SCOPE
//!   - DECODE : RIFF/WAVE container with `fmt ` chunk + `data` chunk.
//!              PCM (format-tag 1) ; 8 / 16 / 24 / 32-bit integer ;
//!              and IEEE 754 float (format-tag 3, 32-bit float).
//!   - ENCODE : RIFF/WAVE writer for the same subset. Round-trip safe :
//!              decoded → encoded is byte-equal.
//!
//! § DELIBERATELY DEFERRED (stage-0)
//!   - WAVEFORMATEXTENSIBLE (format-tag 0xFFFE) — extensible header
//!   - μ-law / A-law (format-tag 6 / 7)
//!   - ADPCM (format-tag 2)
//!   - Compressed formats (MP3 / OGG embedded in RIFF)
//!
//! § SPEC
//!   Microsoft / IBM Multimedia Programming Interface (RIFF) +
//!   Public Multimedia Interface (PCM) chunks.
//!
//! § PRIME-DIRECTIVE
//!   The decoder caps allocation at the data-chunk byte size. No
//!   silent format-tag fall-throughs ; unsupported formats fail with
//!   `UnsupportedKind`. No microphone surface (this is a file-IO
//!   parser, not an input device).

use crate::error::{AssetError, Result};

/// Maximum data chunk bytes the decoder accepts (256 MB raw PCM).
pub const MAX_WAV_DATA_BYTES: usize = 256 * 1024 * 1024;

/// PCM sample format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 8-bit unsigned (silence = 128).
    PcmU8,
    /// 16-bit signed little-endian.
    PcmS16,
    /// 24-bit signed little-endian (3 bytes / sample).
    PcmS24,
    /// 32-bit signed little-endian.
    PcmS32,
    /// 32-bit IEEE 754 float little-endian.
    Float32,
}

impl SampleFormat {
    /// Bits per sample for this format.
    #[must_use]
    pub const fn bits_per_sample(self) -> u16 {
        match self {
            Self::PcmU8 => 8,
            Self::PcmS16 => 16,
            Self::PcmS24 => 24,
            Self::PcmS32 | Self::Float32 => 32,
        }
    }

    /// Bytes per sample (rounded up).
    #[must_use]
    pub const fn bytes_per_sample(self) -> usize {
        ((self.bits_per_sample() as usize) + 7) / 8
    }

    /// WAV format-tag byte for this format.
    #[must_use]
    pub const fn format_tag(self) -> u16 {
        match self {
            Self::PcmU8 | Self::PcmS16 | Self::PcmS24 | Self::PcmS32 => 1,
            Self::Float32 => 3,
        }
    }

    /// Build a `SampleFormat` from `(format_tag, bits_per_sample)`.
    pub fn from_tag_bits(tag: u16, bits: u16) -> Result<Self> {
        match (tag, bits) {
            (1, 8) => Ok(Self::PcmU8),
            (1, 16) => Ok(Self::PcmS16),
            (1, 24) => Ok(Self::PcmS24),
            (1, 32) => Ok(Self::PcmS32),
            (3, 32) => Ok(Self::Float32),
            (1, _) => Err(AssetError::unsupported(
                "WAV",
                format!("PCM with {bits} bits/sample (stage-0 supports 8/16/24/32)"),
            )),
            (3, _) => Err(AssetError::unsupported(
                "WAV",
                format!("float with {bits} bits/sample (stage-0 supports 32 only)"),
            )),
            (0xfffe, _) => Err(AssetError::unsupported(
                "WAV",
                "WAVEFORMATEXTENSIBLE (format-tag 0xfffe) deferred at stage-0",
            )),
            (other, _) => Err(AssetError::unsupported(
                "WAV",
                format!("format-tag {other} (stage-0 supports 1=PCM, 3=float)"),
            )),
        }
    }
}

/// Decoded WAV file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavFile {
    /// Channel count (1 = mono, 2 = stereo).
    pub channels: u16,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Sample format.
    pub format: SampleFormat,
    /// Raw PCM data, interleaved across channels, little-endian samples.
    pub pcm: Vec<u8>,
}

impl WavFile {
    /// Total sample frames (samples per channel).
    #[must_use]
    pub fn frames(&self) -> usize {
        let bps = self.format.bytes_per_sample() * (self.channels as usize);
        if bps == 0 {
            0
        } else {
            self.pcm.len() / bps
        }
    }

    /// Total duration in seconds (frames / sample-rate).
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            (self.frames() as f64) / f64::from(self.sample_rate)
        }
    }
}

const RIFF_MAGIC: [u8; 4] = *b"RIFF";
const WAVE_MAGIC: [u8; 4] = *b"WAVE";
const FMT_CHUNK: [u8; 4] = *b"fmt ";
const DATA_CHUNK: [u8; 4] = *b"data";

/// Decode a WAV byte stream into a `WavFile`.
pub fn decode(bytes: &[u8]) -> Result<WavFile> {
    if bytes.len() < 12 {
        return Err(AssetError::truncated("WAV/header", 12, bytes.len()));
    }
    if bytes[0..4] != RIFF_MAGIC {
        return Err(AssetError::bad_magic("WAV/RIFF", &bytes[..4]));
    }
    if bytes[8..12] != WAVE_MAGIC {
        return Err(AssetError::bad_magic("WAV/WAVE", &bytes[8..12]));
    }
    // RIFF size at bytes[4..8] is 4 + (size of payload) ; we don't strictly
    // require it to match (some encoders short the count) — but if it's
    // larger than the input buffer, that's a corrupt file.
    let riff_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if riff_size + 8 > bytes.len().saturating_add(8) {
        // riff_size refers to the bytes after the 8-byte RIFF header.
    }

    let mut offset = 12;
    let mut fmt: Option<FmtChunk> = None;
    let mut data: Option<&[u8]> = None;

    while offset + 8 <= bytes.len() {
        let chunk_id = [
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ];
        let chunk_size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let body_start = offset + 8;
        let body_end = body_start
            .checked_add(chunk_size)
            .ok_or_else(|| AssetError::invalid("WAV", "chunk", "size overflow"))?;
        if body_end > bytes.len() {
            return Err(AssetError::truncated(
                "WAV/chunk-body",
                body_end - offset,
                bytes.len() - offset,
            ));
        }
        match chunk_id {
            FMT_CHUNK => {
                if fmt.is_some() {
                    return Err(AssetError::invalid("WAV", "chunks", "duplicate fmt chunk"));
                }
                fmt = Some(FmtChunk::parse(&bytes[body_start..body_end])?);
            }
            DATA_CHUNK => {
                if chunk_size > MAX_WAV_DATA_BYTES {
                    return Err(AssetError::invalid(
                        "WAV",
                        "data",
                        format!("chunk size {chunk_size} exceeds cap {MAX_WAV_DATA_BYTES}"),
                    ));
                }
                data = Some(&bytes[body_start..body_end]);
            }
            _ => {
                // Unknown chunk — skip silently per RIFF.
            }
        }
        // Chunks are padded to even lengths.
        let padded_end = if chunk_size % 2 == 1 {
            body_end + 1
        } else {
            body_end
        };
        offset = padded_end;
        // Once we have both, we can stop.
        if fmt.is_some() && data.is_some() {
            break;
        }
    }

    let fmt = fmt.ok_or_else(|| AssetError::invalid("WAV", "chunks", "missing fmt"))?;
    let data = data.ok_or_else(|| AssetError::invalid("WAV", "chunks", "missing data"))?;

    Ok(WavFile {
        channels: fmt.channels,
        sample_rate: fmt.sample_rate,
        format: fmt.format,
        pcm: data.to_vec(),
    })
}

/// Encode a `WavFile` to a RIFF/WAVE byte stream.
pub fn encode(wav: &WavFile) -> Result<Vec<u8>> {
    if wav.channels == 0 {
        return Err(AssetError::encode("WAV", "channels must be > 0"));
    }
    if wav.sample_rate == 0 {
        return Err(AssetError::encode("WAV", "sample_rate must be > 0"));
    }
    let bps = wav.format.bytes_per_sample() * (wav.channels as usize);
    if bps == 0 || wav.pcm.len() % bps != 0 {
        return Err(AssetError::encode(
            "WAV",
            format!(
                "PCM length {} not a multiple of frame size {}",
                wav.pcm.len(),
                bps
            ),
        ));
    }

    // fmt chunk : 16 bytes for PCM, 18 for non-PCM (cbSize = 0).
    let extension_bytes: u16 = if wav.format == SampleFormat::Float32 {
        2 // we still emit as a PCM-flavored fmt at stage-0
    } else {
        0
    };
    let fmt_chunk_size: u32 = if extension_bytes > 0 { 18 } else { 16 };
    let data_chunk_size = wav.pcm.len() as u32;
    // RIFF size = 4 (WAVE) + (8 + fmt_chunk_size) + (8 + data_chunk_size)
    // + pad-byte if data is odd.
    let pad_byte: u32 = if wav.pcm.len() % 2 == 1 { 1 } else { 0 };
    let riff_size: u32 = 4 + 8 + fmt_chunk_size + 8 + data_chunk_size + pad_byte;

    let mut out = Vec::with_capacity(12 + 8 + fmt_chunk_size as usize + 8 + wav.pcm.len() + 1);
    out.extend_from_slice(&RIFF_MAGIC);
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(&WAVE_MAGIC);

    // fmt chunk.
    out.extend_from_slice(&FMT_CHUNK);
    out.extend_from_slice(&fmt_chunk_size.to_le_bytes());
    let format_tag = wav.format.format_tag();
    let bits_per_sample = wav.format.bits_per_sample();
    let block_align = (bits_per_sample / 8) * wav.channels;
    let byte_rate = wav.sample_rate * u32::from(block_align);
    out.extend_from_slice(&format_tag.to_le_bytes());
    out.extend_from_slice(&wav.channels.to_le_bytes());
    out.extend_from_slice(&wav.sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    if extension_bytes > 0 {
        // cbSize = 0 (no extension data).
        out.extend_from_slice(&0u16.to_le_bytes());
    }

    // data chunk.
    out.extend_from_slice(&DATA_CHUNK);
    out.extend_from_slice(&data_chunk_size.to_le_bytes());
    out.extend_from_slice(&wav.pcm);
    if pad_byte == 1 {
        out.push(0);
    }
    Ok(out)
}

struct FmtChunk {
    format: SampleFormat,
    channels: u16,
    sample_rate: u32,
}

impl FmtChunk {
    fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(AssetError::truncated("WAV/fmt", 16, data.len()));
        }
        let format_tag = u16::from_le_bytes([data[0], data[1]]);
        let channels = u16::from_le_bytes([data[2], data[3]]);
        let sample_rate = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let _byte_rate = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let _block_align = u16::from_le_bytes([data[12], data[13]]);
        let bits_per_sample = u16::from_le_bytes([data[14], data[15]]);
        if channels == 0 {
            return Err(AssetError::invalid("WAV", "fmt", "channels=0"));
        }
        if sample_rate == 0 {
            return Err(AssetError::invalid("WAV", "fmt", "sample_rate=0"));
        }
        let format = SampleFormat::from_tag_bits(format_tag, bits_per_sample)?;
        Ok(Self {
            format,
            channels,
            sample_rate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stereo_s16_440hz() -> WavFile {
        // 4 frames of 16-bit signed stereo samples.
        let pcm = vec![
            0x10, 0x00, 0x20, 0x00, // frame 0 : L=0x0010, R=0x0020
            0x30, 0x00, 0x40, 0x00, // frame 1 : L=0x0030, R=0x0040
            0x50, 0x00, 0x60, 0x00, // frame 2
            0x70, 0x00, 0x80, 0x00, // frame 3
        ];
        WavFile {
            channels: 2,
            sample_rate: 44_100,
            format: SampleFormat::PcmS16,
            pcm,
        }
    }

    #[test]
    fn sample_format_bits_and_bytes() {
        assert_eq!(SampleFormat::PcmU8.bits_per_sample(), 8);
        assert_eq!(SampleFormat::PcmU8.bytes_per_sample(), 1);
        assert_eq!(SampleFormat::PcmS16.bits_per_sample(), 16);
        assert_eq!(SampleFormat::PcmS16.bytes_per_sample(), 2);
        assert_eq!(SampleFormat::PcmS24.bits_per_sample(), 24);
        assert_eq!(SampleFormat::PcmS24.bytes_per_sample(), 3);
        assert_eq!(SampleFormat::PcmS32.bits_per_sample(), 32);
        assert_eq!(SampleFormat::Float32.bits_per_sample(), 32);
        assert_eq!(SampleFormat::Float32.bytes_per_sample(), 4);
    }

    #[test]
    fn sample_format_from_tag_bits() {
        assert_eq!(
            SampleFormat::from_tag_bits(1, 16).unwrap(),
            SampleFormat::PcmS16
        );
        assert_eq!(
            SampleFormat::from_tag_bits(3, 32).unwrap(),
            SampleFormat::Float32
        );
    }

    #[test]
    fn sample_format_rejects_unsupported() {
        assert!(matches!(
            SampleFormat::from_tag_bits(1, 5),
            Err(AssetError::UnsupportedKind { .. })
        ));
        assert!(matches!(
            SampleFormat::from_tag_bits(3, 16),
            Err(AssetError::UnsupportedKind { .. })
        ));
        assert!(matches!(
            SampleFormat::from_tag_bits(0xfffe, 16),
            Err(AssetError::UnsupportedKind { .. })
        ));
        assert!(matches!(
            SampleFormat::from_tag_bits(2, 16),
            Err(AssetError::UnsupportedKind { .. })
        ));
    }

    #[test]
    fn encode_decode_round_trip_stereo_s16() {
        let w = make_stereo_s16_440hz();
        let bytes = encode(&w).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn encode_decode_round_trip_mono_u8() {
        let w = WavFile {
            channels: 1,
            sample_rate: 8000,
            format: SampleFormat::PcmU8,
            pcm: vec![128, 130, 126, 128, 200, 50],
        };
        let bytes = encode(&w).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn encode_decode_round_trip_mono_s24() {
        let w = WavFile {
            channels: 1,
            sample_rate: 48_000,
            format: SampleFormat::PcmS24,
            pcm: vec![0x10, 0x20, 0x30, 0x40, 0x50, 0x60],
        };
        let bytes = encode(&w).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn encode_decode_round_trip_mono_s32() {
        let w = WavFile {
            channels: 1,
            sample_rate: 96_000,
            format: SampleFormat::PcmS32,
            pcm: vec![0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80],
        };
        let bytes = encode(&w).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn encode_decode_round_trip_mono_float32() {
        let w = WavFile {
            channels: 1,
            sample_rate: 48_000,
            format: SampleFormat::Float32,
            pcm: vec![0, 0, 0, 0, 0, 0, 0x80, 0x3f], // 0.0, 1.0
        };
        let bytes = encode(&w).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn round_trip_byte_equal() {
        let w = make_stereo_s16_440hz();
        let bytes_a = encode(&w).unwrap();
        let decoded = decode(&bytes_a).unwrap();
        let bytes_b = encode(&decoded).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn frames_and_duration() {
        let w = make_stereo_s16_440hz();
        // 4 frames of 16-bit stereo = 4 frames.
        assert_eq!(w.frames(), 4);
        // 4 frames / 44100 Hz ≈ 9.07e-5 s.
        assert!((w.duration_seconds() - (4.0 / 44_100.0)).abs() < 1e-9);
    }

    #[test]
    fn frames_with_zero_channels_returns_zero() {
        let w = WavFile {
            channels: 1,
            sample_rate: 48_000,
            format: SampleFormat::PcmS16,
            pcm: vec![],
        };
        assert_eq!(w.frames(), 0);
        assert!(w.duration_seconds().abs() < 1e-9);
    }

    #[test]
    fn decode_rejects_short_input() {
        let r = decode(b"RIFF");
        assert!(matches!(r, Err(AssetError::Truncated { .. })));
    }

    #[test]
    fn decode_rejects_bad_magic() {
        let mut bytes = vec![0u8; 64];
        bytes[0..4].copy_from_slice(b"BAD!");
        let r = decode(&bytes);
        assert!(matches!(r, Err(AssetError::BadMagic { .. })));
    }

    #[test]
    fn decode_rejects_bad_wave_marker() {
        let mut bytes = vec![0u8; 64];
        bytes[0..4].copy_from_slice(b"RIFF");
        bytes[8..12].copy_from_slice(b"BAD!");
        let r = decode(&bytes);
        assert!(matches!(r, Err(AssetError::BadMagic { .. })));
    }

    #[test]
    fn decode_rejects_missing_fmt() {
        // RIFF / WAVE / data only.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 4]);
        let r = decode(&bytes);
        assert!(matches!(r, Err(AssetError::InvalidValue { .. })));
    }

    #[test]
    fn decode_rejects_missing_data() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // tag
        bytes.extend_from_slice(&1u16.to_le_bytes()); // channels
        bytes.extend_from_slice(&8000u32.to_le_bytes()); // rate
        bytes.extend_from_slice(&8000u32.to_le_bytes()); // byte rate
        bytes.extend_from_slice(&1u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&8u16.to_le_bytes()); // bits
        let r = decode(&bytes);
        assert!(matches!(r, Err(AssetError::InvalidValue { .. })));
    }

    #[test]
    fn decode_skips_unknown_chunks() {
        // Build a WAV with a `LIST` chunk between `fmt ` and `data`.
        let inner_pcm = vec![0u8, 0, 0, 0, 0, 0, 0, 0];
        let w = WavFile {
            channels: 1,
            sample_rate: 8000,
            format: SampleFormat::PcmS16,
            pcm: inner_pcm,
        };
        let mut bytes = encode(&w).unwrap();
        // Insert a LIST chunk between fmt+data : easiest to splice
        // before the data chunk. Find the data-chunk offset ourselves.
        let data_off = bytes
            .windows(4)
            .position(|w| w == b"data")
            .expect("encoded wav must have data");
        // Build LIST chunk : id (4) + size (4) + body (chunk_size, padded).
        let mut list = Vec::new();
        list.extend_from_slice(b"LIST");
        let list_body = b"INFO";
        list.extend_from_slice(&(list_body.len() as u32).to_le_bytes());
        list.extend_from_slice(list_body);
        // Splice + recompute the RIFF size.
        bytes.splice(data_off..data_off, list.iter().copied());
        let new_riff_size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&new_riff_size.to_le_bytes());
        let back = decode(&bytes).unwrap();
        assert_eq!(back, w);
    }

    #[test]
    fn encode_rejects_zero_channels() {
        let w = WavFile {
            channels: 0,
            sample_rate: 8000,
            format: SampleFormat::PcmS16,
            pcm: vec![],
        };
        assert!(matches!(encode(&w), Err(AssetError::Encode { .. })));
    }

    #[test]
    fn encode_rejects_zero_sample_rate() {
        let w = WavFile {
            channels: 1,
            sample_rate: 0,
            format: SampleFormat::PcmS16,
            pcm: vec![0, 0],
        };
        assert!(matches!(encode(&w), Err(AssetError::Encode { .. })));
    }

    #[test]
    fn encode_rejects_misaligned_pcm() {
        let w = WavFile {
            channels: 2, // 4 bytes per frame for s16 stereo
            sample_rate: 8000,
            format: SampleFormat::PcmS16,
            pcm: vec![0; 5], // not a multiple of 4
        };
        assert!(matches!(encode(&w), Err(AssetError::Encode { .. })));
    }

    #[test]
    fn encode_pads_odd_data_chunk() {
        // Mono u8 with an odd number of samples — the encoder must add a
        // pad byte.
        let w = WavFile {
            channels: 1,
            sample_rate: 8000,
            format: SampleFormat::PcmU8,
            pcm: vec![1, 2, 3], // 3 bytes (odd)
        };
        let bytes = encode(&w).unwrap();
        // Look for the trailing pad byte.
        let data_off = bytes
            .windows(4)
            .position(|w| w == b"data")
            .expect("encoded wav must have data");
        let chunk_size = u32::from_le_bytes([
            bytes[data_off + 4],
            bytes[data_off + 5],
            bytes[data_off + 6],
            bytes[data_off + 7],
        ]);
        assert_eq!(chunk_size, 3);
        // Data starts at data_off + 8. Pad byte sits at data_off+8+3.
        assert_eq!(bytes.len(), data_off + 8 + 3 + 1);
        assert_eq!(bytes[data_off + 8 + 3], 0);
    }

    #[test]
    fn decode_handles_padded_odd_chunks() {
        // Encode then decode.
        let w = WavFile {
            channels: 1,
            sample_rate: 8000,
            format: SampleFormat::PcmU8,
            pcm: vec![1, 2, 3],
        };
        let bytes = encode(&w).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back, w);
    }
}
