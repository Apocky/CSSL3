//! § wired_frame_recorder — wrapper around `cssl-host-frame-recorder`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the bounded-ring frame accumulator + LFRC v1 encode/decode
//!   surface so MCP tools can probe the format magic without reaching into
//!   the path-dep.
//!
//! § wrapped surface
//!   - [`Frame`] / [`FrameKind`] / [`FrameErr`] — RGBA8 + metadata.
//!   - [`FrameRecorder`] — bounded-ring accumulator with drop counters.
//!   - [`encode_to_bytes`] / [`decode_from_bytes`] — round-trip codec.
//!   - [`LFRC_MAGIC`] / [`LFRC_VERSION`] / [`LfrcStore`] — magic + storage.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; pure bytes only.

pub use cssl_host_frame_recorder::{
    decode_from_bytes, encode_to_bytes, lfrc, Frame, FrameErr, FrameKind, FrameRecorder, LfrcErr,
    LfrcStore, LFRC_MAGIC, LFRC_VERSION,
};

/// Convenience : LFRC magic bytes as a hex string ("4c46524301000000" — 8
/// bytes : ASCII "LFRC" followed by version + reserved). Used by the
/// `frame_recorder.lfrc_magic` MCP tool to surface the format identifier.
#[must_use]
pub fn lfrc_magic_hex() -> String {
    let mut s = String::with_capacity(LFRC_MAGIC.len() * 2);
    for b in LFRC_MAGIC {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lfrc_magic_hex_starts_with_ascii_lfrc() {
        let s = lfrc_magic_hex();
        // 'L' = 0x4c, 'F' = 0x46, 'R' = 0x52, 'C' = 0x43
        assert!(s.starts_with("4c465243"));
        assert_eq!(s.len(), LFRC_MAGIC.len() * 2);
    }

    #[test]
    fn version_is_one() {
        assert_eq!(LFRC_VERSION, 1);
    }

    #[test]
    fn empty_recorder_round_trips() {
        let rec = FrameRecorder::new(4);
        let bytes = encode_to_bytes(&rec);
        let decoded = decode_from_bytes(&bytes).expect("decode ok");
        assert_eq!(decoded.snapshot().len(), 0);
    }
}
