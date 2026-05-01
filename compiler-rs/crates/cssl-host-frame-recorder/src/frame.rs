//! § frame.rs
//! ══════════════════════════════════════════════════════════════════
//! § Frame = RGBA8 framebuffer + (w,h) + ts_micros + kind tag.
//! § validate() = total enforce: len == w*h*4 · w/h ≤ 8192 · w/h > 0.
//! § FrameKind::DeltaFromPrevious is reserved for stage-1 ; stage-0
//!   recorders only emit `KeyFrame`.

use serde::{Deserialize, Serialize};

/// § Maximum permitted dimension for any single frame axis.
///
/// 8192 × 8192 × 4 bytes = 256 MiB per frame upper-bound — well above
/// any reasonable engine output (8K UHD = 7680×4320) but bounded so a
/// malicious or corrupt LFRC header cannot trigger an unbounded
/// allocation in the decoder.
pub const MAX_DIMENSION: u32 = 8192;

/// § kind discriminant for stream-level frame typing.
///
/// Stage-0 only emits `KeyFrame` ; `DeltaFromPrevious` is reserved for
/// a future inter-frame delta encoder (deferred wave-6+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameKind {
    /// Self-contained RGBA8 framebuffer ; decoder needs no prior frame.
    KeyFrame,
    /// Reserved for future delta-coded frames ; unused in stage-0.
    DeltaFromPrevious,
}

/// § validation errors surfaced by [`Frame::validate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameErr {
    /// rgba.len() != width * height * 4
    LengthMismatch,
    /// width or height is zero
    ZeroDimension,
    /// width or height exceeds [`MAX_DIMENSION`]
    OversizedDimension,
}

impl std::fmt::Display for FrameErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameErr::LengthMismatch => f.write_str("frame rgba length != width * height * 4"),
            FrameErr::ZeroDimension => f.write_str("frame width or height is zero"),
            FrameErr::OversizedDimension => write!(
                f,
                "frame dimension exceeds maximum permitted axis ({MAX_DIMENSION})",
            ),
        }
    }
}

impl std::error::Error for FrameErr {}

/// § single recorded framebuffer.
///
/// Layout : `width * height` pixels of 4-byte RGBA8 (row-major, no
/// padding) packed in `rgba`. `ts_micros` is monotonically-increasing
/// microseconds since the recorder's `started_at_ts` reference clock
/// (caller-supplied — recorder does not call the system clock itself
/// to keep the crate side-effect-free for replay-determinism tests).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// pixel width in [1, [`MAX_DIMENSION`]]
    pub width: u32,
    /// pixel height in [1, [`MAX_DIMENSION`]]
    pub height: u32,
    /// caller-supplied timestamp in microseconds
    pub ts_micros: u64,
    /// frame-kind discriminant ; stage-0 always [`FrameKind::KeyFrame`]
    pub kind: FrameKind,
    /// raw RGBA8 pixel bytes ; length must equal `width * height * 4`
    pub rgba: Vec<u8>,
}

impl Frame {
    /// § create a new keyframe ; **does not** validate dimensions —
    /// callers should call [`Self::validate`] (or use the recorder's
    /// `push`, which is permissive but documented).
    #[must_use]
    pub fn new_keyframe(width: u32, height: u32, ts_micros: u64, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            ts_micros,
            kind: FrameKind::KeyFrame,
            rgba,
        }
    }

    /// § verify (w,h,len) consistency + bounded dimensions.
    pub fn validate(&self) -> Result<(), FrameErr> {
        if self.width == 0 || self.height == 0 {
            return Err(FrameErr::ZeroDimension);
        }
        if self.width > MAX_DIMENSION || self.height > MAX_DIMENSION {
            return Err(FrameErr::OversizedDimension);
        }
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(FrameErr::OversizedDimension)?;
        if self.rgba.len() != expected {
            return Err(FrameErr::LengthMismatch);
        }
        Ok(())
    }

    /// § total bytes consumed by the rgba payload.
    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        self.rgba.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(width: u32, height: u32, ts: u64) -> Frame {
        let len = (width as usize) * (height as usize) * 4;
        Frame::new_keyframe(width, height, ts, vec![0xAB; len])
    }

    #[test]
    fn valid_frame_passes() {
        let f = make(4, 3, 1_000);
        assert!(f.validate().is_ok());
        assert_eq!(f.payload_bytes(), 4 * 3 * 4);
        assert_eq!(f.kind, FrameKind::KeyFrame);
    }

    #[test]
    fn length_mismatch_rejected() {
        let mut f = make(4, 3, 0);
        f.rgba.pop();
        assert_eq!(f.validate(), Err(FrameErr::LengthMismatch));
    }

    #[test]
    fn zero_dimension_rejected() {
        let f = Frame::new_keyframe(0, 4, 0, vec![]);
        assert_eq!(f.validate(), Err(FrameErr::ZeroDimension));
        let g = Frame::new_keyframe(4, 0, 0, vec![]);
        assert_eq!(g.validate(), Err(FrameErr::ZeroDimension));
    }

    #[test]
    fn oversize_dimension_rejected() {
        let f = Frame::new_keyframe(MAX_DIMENSION + 1, 1, 0, vec![]);
        assert_eq!(f.validate(), Err(FrameErr::OversizedDimension));
        let g = Frame::new_keyframe(1, MAX_DIMENSION + 1, 0, vec![]);
        assert_eq!(g.validate(), Err(FrameErr::OversizedDimension));
    }

    #[test]
    fn serde_roundtrip_via_json() {
        // serde-roundtrip guarantees Frame survives storage + IPC paths.
        let f = make(2, 2, 42);
        let json = serde_json::to_string(&f).expect("ser");
        let back: Frame = serde_json::from_str(&json).expect("de");
        assert_eq!(f, back);
        assert!(back.validate().is_ok());
    }

    #[test]
    fn frame_err_display_human_readable() {
        assert!(format!("{}", FrameErr::LengthMismatch).contains("length"));
        assert!(format!("{}", FrameErr::ZeroDimension).contains("zero"));
        assert!(format!("{}", FrameErr::OversizedDimension).contains("maximum"));
    }
}
