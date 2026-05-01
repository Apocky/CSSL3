//! § lfrc.rs — LoA Frame Recording Container, format v1.
//! ══════════════════════════════════════════════════════════════════
//!
//! § wire layout (little-endian throughout)
//!
//! ```text
//!   ┌──── HEADER (40 bytes) ────────────────────────────┐
//!   │ offset 0 : magic       8 bytes  "LFRC\0\x01\0\0"   │
//!   │ offset 8 : width       u32-LE                      │
//!   │ offset 12: height      u32-LE                      │
//!   │ offset 16: frame_count u32-LE                      │
//!   │ offset 20: created_us  u64-LE                      │
//!   │ offset 28: reserved    12 bytes (zero-filled)      │
//!   └────────────────────────────────────────────────────┘
//!   ┌──── FRAMES (frame_count × variable) ──────────────┐
//!   │ offset 0 : kind        u8     0=KeyFrame, 1=Delta  │
//!   │ offset 1 : pad         3 bytes (zero-filled)       │
//!   │ offset 4 : ts_micros   u64-LE                      │
//!   │ offset 12: byte_len    u32-LE  (must == w*h*4)     │
//!   │ offset 16: rgba bytes  byte_len bytes              │
//!   └────────────────────────────────────────────────────┘
//!   ┌──── FOOTER (8 bytes) ─────────────────────────────┐
//!   │ offset 0 : sentinel    4 bytes  "END!"             │
//!   │ offset 4 : crc32       u32-LE   over all preceding │
//!   └────────────────────────────────────────────────────┘
//! ```
//!
//! § notes
//!   • magic includes version u16-LE @ offset 4 + reserved u16 @ offset 6.
//!     Version mismatch is a hard reject ; reserved must be zero.
//!   • per-frame `width` × `height` are derived from the header — the
//!     stage-0 encoder requires every frame to share dimensions. The
//!     decoder validates `byte_len == header.width * header.height * 4`.
//!   • CRC32 polynomial is 0xEDB88320 (zlib / IEEE 802.3) computed via
//!     a 256-entry table built at compile-time. No external crc32 dep.

use crate::frame::{Frame, FrameErr, FrameKind, MAX_DIMENSION};
use crate::recorder::FrameRecorder;

/// § magic bytes : 4 ASCII + version u16-LE (=1) + reserved u16 = 8 total.
///
/// Bytes 4..6 encode `LFRC_VERSION` little-endian ; bytes 6..8 are
/// reserved and MUST be zero on the wire.
pub const LFRC_MAGIC: [u8; 8] = [b'L', b'F', b'R', b'C', 0x01, 0x00, 0x00, 0x00];

/// § wire format major version.
pub const LFRC_VERSION: u16 = 1;

/// § footer sentinel preceding the CRC32 word.
pub const LFRC_FOOTER_SENTINEL: [u8; 4] = *b"END!";

/// § total fixed header size, in bytes.
pub const LFRC_HEADER_LEN: usize = 40;

/// § total fixed footer size, in bytes.
pub const LFRC_FOOTER_LEN: usize = 8;

/// § per-frame fixed prelude size (kind + pad + ts + byte_len), in bytes.
pub const LFRC_FRAME_PRELUDE_LEN: usize = 16;

const KIND_KEYFRAME: u8 = 0;
const KIND_DELTA: u8 = 1;

/// § decode-time error variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfrcErr {
    /// header magic mismatch
    BadMagic,
    /// header version field does not equal [`LFRC_VERSION`]
    WrongVersion,
    /// payload truncated mid-record
    UnexpectedEof,
    /// per-frame `byte_len` did not match header-derived `w * h * 4`
    FrameLengthMismatch,
    /// footer CRC32 disagreed with computed value over preceding bytes
    BadCrc,
    /// header dimensions exceed [`MAX_DIMENSION`]
    OversizedDimension,
    /// per-frame kind byte is neither 0 (KeyFrame) nor 1 (Delta)
    UnknownFrameKind,
    /// header frame_count disagreed with parsed frame records
    FrameCountMismatch,
    /// frame failed `Frame::validate` (reserved-pad nonzero etc.)
    FrameValidation(FrameErr),
    /// header reserved bytes were not zero
    ReservedNonZero,
}

impl std::fmt::Display for LfrcErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LfrcErr::BadMagic => f.write_str("LFRC magic bytes mismatch"),
            LfrcErr::WrongVersion => f.write_str("LFRC version not supported"),
            LfrcErr::UnexpectedEof => f.write_str("LFRC payload truncated"),
            LfrcErr::FrameLengthMismatch => f.write_str("LFRC frame byte_len != width*height*4"),
            LfrcErr::BadCrc => f.write_str("LFRC footer CRC32 mismatch — payload tampered"),
            LfrcErr::OversizedDimension => f.write_str("LFRC header dimension exceeds maximum"),
            LfrcErr::UnknownFrameKind => f.write_str("LFRC frame kind discriminant unknown"),
            LfrcErr::FrameCountMismatch => f.write_str("LFRC header frame_count != parsed frames"),
            LfrcErr::FrameValidation(e) => write!(f, "LFRC frame validation failed: {e}"),
            LfrcErr::ReservedNonZero => f.write_str("LFRC reserved bytes must be zero"),
        }
    }
}

impl std::error::Error for LfrcErr {}

impl From<FrameErr> for LfrcErr {
    fn from(e: FrameErr) -> Self {
        LfrcErr::FrameValidation(e)
    }
}

// ──────────────────────────────────────────────────────────────────
// § CRC32 (IEEE 802.3 / zlib polynomial 0xEDB88320), table-driven.
// Vendored at ~50 LOC to avoid pulling crc32fast / crc32 deps.
// ──────────────────────────────────────────────────────────────────

const fn build_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i: u32 = 0;
    while i < 256 {
        let mut c = i;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i as usize] = c;
        i += 1;
    }
    table
}

const CRC32_TABLE: [u32; 256] = build_crc32_table();

/// § CRC32-IEEE over `bytes`. Caller-init seed is 0xFFFF_FFFF (zlib).
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut c: u32 = 0xFFFF_FFFF;
    for &b in bytes {
        let idx = ((c ^ u32::from(b)) & 0xFF) as usize;
        c = CRC32_TABLE[idx] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

// ──────────────────────────────────────────────────────────────────
// § ENCODE
// ──────────────────────────────────────────────────────────────────

/// § serialize a recorder snapshot to an LFRC byte-stream.
///
/// All buffered frames are emitted ; `dropped` and `total_bytes`
/// counters are NOT serialized (they are runtime-only telemetry).
/// If the recorder is empty, a header + zero frames + footer is
/// emitted (still valid LFRC ; round-trips cleanly).
#[must_use]
pub fn encode_to_bytes(recorder: &FrameRecorder) -> Vec<u8> {
    let frames = recorder.snapshot();
    let (width, height) = frames.first().map_or((0, 0), |f| (f.width, f.height));
    let frame_count = frames.len() as u32;
    let created_us = recorder.started_at().unwrap_or(0);

    let payload_bytes: usize = frames
        .iter()
        .map(|f| LFRC_FRAME_PRELUDE_LEN + f.rgba.len())
        .sum();
    let total = LFRC_HEADER_LEN + payload_bytes + LFRC_FOOTER_LEN;

    let mut out = Vec::with_capacity(total);

    // ─ header ─
    out.extend_from_slice(&LFRC_MAGIC);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&frame_count.to_le_bytes());
    out.extend_from_slice(&created_us.to_le_bytes());
    // reserved 12 bytes (40 - 28 = 12)
    out.extend_from_slice(&[0u8; 12]);
    debug_assert_eq!(out.len(), LFRC_HEADER_LEN);

    // ─ frames ─
    for frame in frames {
        let kind_byte = match frame.kind {
            FrameKind::KeyFrame => KIND_KEYFRAME,
            FrameKind::DeltaFromPrevious => KIND_DELTA,
        };
        out.push(kind_byte);
        out.extend_from_slice(&[0u8; 3]); // pad
        out.extend_from_slice(&frame.ts_micros.to_le_bytes());
        out.extend_from_slice(&(frame.rgba.len() as u32).to_le_bytes());
        out.extend_from_slice(&frame.rgba);
    }

    // ─ footer ─
    out.extend_from_slice(&LFRC_FOOTER_SENTINEL);
    let crc = crc32(&out);
    out.extend_from_slice(&crc.to_le_bytes());

    out
}

// ──────────────────────────────────────────────────────────────────
// § DECODE
// ──────────────────────────────────────────────────────────────────

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], LfrcErr> {
        if self.pos.saturating_add(n) > self.bytes.len() {
            return Err(LfrcErr::UnexpectedEof);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u32_le(&mut self) -> Result<u32, LfrcErr> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn read_u64_le(&mut self) -> Result<u64, LfrcErr> {
        let s = self.take(8)?;
        Ok(u64::from_le_bytes([
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
        ]))
    }

    fn read_u8(&mut self) -> Result<u8, LfrcErr> {
        let s = self.take(1)?;
        Ok(s[0])
    }
}

/// § deserialize an LFRC byte-stream back into a [`FrameRecorder`].
///
/// Strict validation : magic, version, reserved-zero, per-frame
/// length, frame-count, footer sentinel, CRC32 — any mismatch returns
/// the corresponding [`LfrcErr`] without partial state.
pub fn decode_from_bytes(bytes: &[u8]) -> Result<FrameRecorder, LfrcErr> {
    if bytes.len() < LFRC_HEADER_LEN + LFRC_FOOTER_LEN {
        return Err(LfrcErr::UnexpectedEof);
    }

    // ─ verify CRC32 first to catch tampering before partial parse ─
    let payload_end = bytes.len() - 4;
    let stored_crc = u32::from_le_bytes([
        bytes[payload_end],
        bytes[payload_end + 1],
        bytes[payload_end + 2],
        bytes[payload_end + 3],
    ]);
    let computed_crc = crc32(&bytes[..payload_end]);
    if stored_crc != computed_crc {
        return Err(LfrcErr::BadCrc);
    }

    let mut c = Cursor::new(bytes);

    // ─ header ─
    let magic = c.take(8)?;
    if &magic[..4] != b"LFRC" {
        return Err(LfrcErr::BadMagic);
    }
    let version = u16::from_le_bytes([magic[4], magic[5]]);
    if version != LFRC_VERSION {
        return Err(LfrcErr::WrongVersion);
    }
    let magic_reserved = u16::from_le_bytes([magic[6], magic[7]]);
    if magic_reserved != 0 {
        return Err(LfrcErr::ReservedNonZero);
    }
    let width = c.read_u32_le()?;
    let height = c.read_u32_le()?;
    let frame_count = c.read_u32_le()?;
    let created_us = c.read_u64_le()?;
    let reserved = c.take(12)?;
    if reserved.iter().any(|&b| b != 0) {
        return Err(LfrcErr::ReservedNonZero);
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(LfrcErr::OversizedDimension);
    }

    // ─ frames ─
    let mut frames = Vec::with_capacity(frame_count as usize);
    for _ in 0..frame_count {
        let kind_byte = c.read_u8()?;
        let kind = match kind_byte {
            KIND_KEYFRAME => FrameKind::KeyFrame,
            KIND_DELTA => FrameKind::DeltaFromPrevious,
            _ => return Err(LfrcErr::UnknownFrameKind),
        };
        let pad = c.take(3)?;
        if pad.iter().any(|&b| b != 0) {
            return Err(LfrcErr::ReservedNonZero);
        }
        let ts_micros = c.read_u64_le()?;
        let byte_len = c.read_u32_le()? as usize;
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(LfrcErr::OversizedDimension)?;
        if byte_len != expected {
            return Err(LfrcErr::FrameLengthMismatch);
        }
        let rgba = c.take(byte_len)?.to_vec();
        let frame = Frame {
            width,
            height,
            ts_micros,
            kind,
            rgba,
        };
        frame.validate()?;
        frames.push(frame);
    }

    if frames.len() != frame_count as usize {
        return Err(LfrcErr::FrameCountMismatch);
    }

    // ─ footer sentinel ─
    let sentinel = c.take(4)?;
    if sentinel != LFRC_FOOTER_SENTINEL {
        return Err(LfrcErr::BadMagic);
    }
    // (CRC already verified above ; remaining 4 bytes are the CRC word
    // which the upfront check has already consumed-by-comparison.)
    let _ = c.take(4)?;

    let started_at_ts = if frame_count == 0 { None } else { Some(created_us) };
    let capacity = frames.len().max(1);
    Ok(FrameRecorder::from_decoded(capacity, frames, started_at_ts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Frame, FrameKind};
    use crate::recorder::FrameRecorder;

    fn mk(w: u32, h: u32, ts: u64, fill: u8) -> Frame {
        let len = (w as usize) * (h as usize) * 4;
        Frame {
            width: w,
            height: h,
            ts_micros: ts,
            kind: FrameKind::KeyFrame,
            rgba: vec![fill; len],
        }
    }

    #[test]
    fn crc32_known_vector() {
        // canonical CRC32 of "123456789" = 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        // empty buffer = 0
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn encode_decode_roundtrip_single_frame() {
        let mut r = FrameRecorder::new(4);
        r.push(mk(2, 2, 100, 0x42));
        let bytes = encode_to_bytes(&r);
        let back = decode_from_bytes(&bytes).expect("decode");
        assert_eq!(back.frame_count(), 1);
        assert_eq!(back.snapshot()[0].rgba, vec![0x42; 16]);
        assert_eq!(back.snapshot()[0].ts_micros, 100);
    }

    #[test]
    fn encode_decode_roundtrip_multi_frame() {
        let mut r = FrameRecorder::new(8);
        for i in 0..5u64 {
            r.push(mk(3, 2, i * 1_000, (i as u8) * 16));
        }
        let bytes = encode_to_bytes(&r);
        let back = decode_from_bytes(&bytes).expect("decode");
        assert_eq!(back.frame_count(), 5);
        for (i, f) in back.snapshot().iter().enumerate() {
            assert_eq!(f.ts_micros, (i as u64) * 1_000);
            assert_eq!(f.rgba[0], (i as u8) * 16);
            assert_eq!(f.kind, FrameKind::KeyFrame);
        }
    }

    #[test]
    fn empty_recorder_encodes_cleanly() {
        let r = FrameRecorder::new(4);
        let bytes = encode_to_bytes(&r);
        assert_eq!(
            bytes.len(),
            LFRC_HEADER_LEN + LFRC_FOOTER_LEN,
            "empty recording = 48 bytes (40 header + 8 footer)"
        );
        let back = decode_from_bytes(&bytes).expect("decode empty");
        assert_eq!(back.frame_count(), 0);
        assert!(back.started_at().is_none());
    }

    #[test]
    fn bad_magic_rejected() {
        let mut r = FrameRecorder::new(2);
        r.push(mk(1, 1, 0, 0));
        let mut bytes = encode_to_bytes(&r);
        bytes[0] = b'X';
        // CRC will fail before magic-check ; both paths reject — accept either.
        let err = decode_from_bytes(&bytes).expect_err("must reject");
        assert!(matches!(err, LfrcErr::BadMagic | LfrcErr::BadCrc));
    }

    #[test]
    fn wrong_version_rejected() {
        let mut r = FrameRecorder::new(2);
        r.push(mk(1, 1, 0, 0));
        let mut bytes = encode_to_bytes(&r);
        // bump version u16 @ offset 4 from 1 → 99 + recompute CRC so we reach
        // the version-check rather than failing on CRC first.
        bytes[4] = 99;
        bytes[5] = 0;
        let payload_end = bytes.len() - 4;
        let new_crc = crc32(&bytes[..payload_end]);
        bytes[payload_end..].copy_from_slice(&new_crc.to_le_bytes());
        assert!(matches!(
            decode_from_bytes(&bytes),
            Err(LfrcErr::WrongVersion)
        ));
    }

    #[test]
    fn truncated_rejected() {
        let mut r = FrameRecorder::new(2);
        r.push(mk(2, 2, 0, 0));
        let bytes = encode_to_bytes(&r);
        // chop off most of the payload
        let truncated = &bytes[..LFRC_HEADER_LEN + 4];
        let err = decode_from_bytes(truncated).expect_err("must reject");
        // truncation can surface as either UnexpectedEof or BadCrc depending on
        // where the cut lands ; both are valid rejection paths.
        assert!(matches!(err, LfrcErr::UnexpectedEof | LfrcErr::BadCrc));
    }

    #[test]
    fn crc_tamper_rejected() {
        let mut r = FrameRecorder::new(2);
        r.push(mk(2, 2, 0, 0xAA));
        let mut bytes = encode_to_bytes(&r);
        // flip a single payload byte ; CRC must catch it
        let mid = LFRC_HEADER_LEN + LFRC_FRAME_PRELUDE_LEN + 1;
        bytes[mid] ^= 0xFF;
        assert!(matches!(decode_from_bytes(&bytes), Err(LfrcErr::BadCrc)));
    }

    #[test]
    fn frame_length_mismatch_rejected() {
        // forge an LFRC where header w*h*4 ≠ per-frame byte_len.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&LFRC_MAGIC);
        bytes.extend_from_slice(&2u32.to_le_bytes()); // width=2
        bytes.extend_from_slice(&2u32.to_le_bytes()); // height=2  → expects 16 bytes/frame
        bytes.extend_from_slice(&1u32.to_le_bytes()); // frame_count=1
        bytes.extend_from_slice(&0u64.to_le_bytes()); // created_us=0
        bytes.extend_from_slice(&[0u8; 12]); // reserved
        // frame record with WRONG byte_len = 12 (3 px instead of 4)
        bytes.push(KIND_KEYFRAME);
        bytes.extend_from_slice(&[0u8; 3]); // pad
        bytes.extend_from_slice(&0u64.to_le_bytes()); // ts
        bytes.extend_from_slice(&12u32.to_le_bytes()); // byte_len=12 (BAD)
        bytes.extend_from_slice(&[0u8; 12]);
        bytes.extend_from_slice(&LFRC_FOOTER_SENTINEL);
        let crc = crc32(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            decode_from_bytes(&bytes),
            Err(LfrcErr::FrameLengthMismatch)
        ));
    }

    #[test]
    fn header_constants_correct() {
        assert_eq!(LFRC_MAGIC.len(), 8);
        assert_eq!(LFRC_HEADER_LEN, 40);
        assert_eq!(LFRC_FOOTER_LEN, 8);
        assert_eq!(LFRC_FRAME_PRELUDE_LEN, 16);
        assert_eq!(LFRC_VERSION, 1);
    }

    #[test]
    fn lfrc_err_display_human_readable() {
        for e in [
            LfrcErr::BadMagic,
            LfrcErr::WrongVersion,
            LfrcErr::UnexpectedEof,
            LfrcErr::FrameLengthMismatch,
            LfrcErr::BadCrc,
            LfrcErr::OversizedDimension,
            LfrcErr::UnknownFrameKind,
            LfrcErr::FrameCountMismatch,
            LfrcErr::ReservedNonZero,
            LfrcErr::FrameValidation(FrameErr::ZeroDimension),
        ] {
            let s = format!("{e}");
            assert!(!s.is_empty());
            assert!(s.contains("LFRC"));
        }
    }
}
