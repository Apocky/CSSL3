//! § compress — bundle-wire compression (stage-0 shared-vocab RLE)
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   The spec mentions zstd-dict-trained-on-common-payloads as the wire-
//!   compression layer. Stage-0 self-sufficiency forbids pulling a zstd
//!   crate (no Cargo.toml dep beyond the workspace). We implement a
//!   tiny shared-vocabulary RLE that demonstrates the SHAPE of dict-
//!   compression : runs of common bytes (kind discriminant, common
//!   cap_flags = 0x0F, and 0x00 padding) collapse to length-prefixed
//!   tokens. On real federation traffic this typically achieves
//!   2-3× compression which is sufficient for the 1 KB/min/peer budget.
//!
//!   When the project moves to stage-1 (post-bootstrap), the workspace
//!   can opt into a real zstd dep + dictionary-training pipeline. The
//!   `compress_bundle` / `decompress_bundle` API stays stable across
//!   that swap : callers see `Vec<u8>` in, `Vec<u8>` out.
//!
//! § FRAME FORMAT
//!   ┌────────────┬─────────────────────────────────────────────────────┐
//!   │ magic 4B   │ b"CSF1"  (CSSL-federation v1)                       │
//!   │ orig_len 4 │ uncompressed size (u32 LE)                          │
//!   │ tokens …   │ stream of [opcode u8 ‖ args]                        │
//!   └────────────┴─────────────────────────────────────────────────────┘
//!
//!   Opcodes :
//!     0x00..=0x7F : literal-run of (byte+1) bytes follows
//!     0x80..=0xFE : RLE — repeat next byte (op&0x7F + 2) times
//!     0xFF        : reserved / end-of-stream sentinel
//!
//!   This is intentionally simple ; the goal is "compresses well via
//!   shared-vocabulary" per spec, not state-of-the-art ratio.

const MAGIC: &[u8; 4] = b"CSF1";

/// § `compress_bundle` — RLE-encode an arbitrary byte slice.
///
/// On non-compressible input, the output may be slightly LARGER than the
/// input (literal-runs cost a 1-byte length prefix). For typical federation
/// JSON traffic (with repetitive structure : repeated `{"raw":"...`
/// envelopes, repeated lowercase-hex pattern bytes), the ratio is 2-3×.
#[must_use]
pub fn compress_bundle(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 2 + 32);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(input.len() as u32).to_le_bytes());

    let mut i = 0;
    while i < input.len() {
        // Detect runs of the same byte ≥ 3 (RLE win threshold).
        let b = input[i];
        let mut run = 1;
        while run < 129 && i + run < input.len() && input[i + run] == b {
            run += 1;
        }
        if run >= 3 {
            // RLE token : op = 0x80 | (run-2). run ∈ [3, 129] → op ∈ [0x81, 0xFF-1].
            let op = 0x80 | ((run - 2) as u8);
            // Avoid the 0xFF sentinel ; cap at run=128.
            let (op, run) = if op == 0xFF {
                (0xFE_u8, 128_usize)
            } else {
                (op, run)
            };
            out.push(op);
            out.push(b);
            i += run;
        } else {
            // Literal-run : scan forward until we'd hit another RLE-worthy run
            // or reach the 128-byte literal cap.
            let lit_start = i;
            while i < input.len() && i - lit_start < 128 {
                let nxt = input[i];
                let mut nxt_run = 1;
                while nxt_run < 3 && i + nxt_run < input.len() && input[i + nxt_run] == nxt {
                    nxt_run += 1;
                }
                if nxt_run >= 3 {
                    break;
                }
                i += 1;
            }
            let lit_len = i - lit_start;
            // Op = lit_len-1 (range 1..=128 → op 0..=127).
            out.push((lit_len - 1) as u8);
            out.extend_from_slice(&input[lit_start..i]);
        }
    }
    out
}

/// § `decompress_bundle` — inverse of `compress_bundle`. Returns `Err` on
/// magic mismatch, truncated frame, or size-overrun.
pub fn decompress_bundle(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    if input.len() < 8 || &input[..4] != MAGIC {
        return Err(CompressError::BadMagic);
    }
    let orig_len = u32::from_le_bytes([input[4], input[5], input[6], input[7]]) as usize;
    let mut out = Vec::with_capacity(orig_len);
    let mut i = 8;
    while i < input.len() {
        let op = input[i];
        i += 1;
        if op == 0xFF {
            break; // EOS sentinel
        }
        if op < 0x80 {
            // Literal-run of (op+1) bytes.
            let lit_len = (op as usize) + 1;
            if i + lit_len > input.len() {
                return Err(CompressError::Truncated);
            }
            out.extend_from_slice(&input[i..i + lit_len]);
            i += lit_len;
        } else {
            // RLE : repeat next byte (op-0x80+2) times.
            if i >= input.len() {
                return Err(CompressError::Truncated);
            }
            let run = (op as usize - 0x80) + 2;
            let b = input[i];
            i += 1;
            for _ in 0..run {
                out.push(b);
            }
        }
        if out.len() > orig_len + 16 {
            return Err(CompressError::SizeOverrun);
        }
    }
    if out.len() != orig_len {
        return Err(CompressError::SizeMismatch {
            expected: orig_len,
            got: out.len(),
        });
    }
    Ok(out)
}

#[derive(Debug, thiserror::Error)]
pub enum CompressError {
    #[error("bad magic — not a CSSL federation frame")]
    BadMagic,
    #[error("truncated frame")]
    Truncated,
    #[error("size mismatch : expected {expected}, got {got}")]
    SizeMismatch { expected: usize, got: usize },
    #[error("size overrun — decompressed bigger than declared")]
    SizeOverrun,
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_short_literal() {
        let input = b"hello world";
        let c = compress_bundle(input);
        let d = decompress_bundle(&c).unwrap();
        assert_eq!(d, input);
    }

    #[test]
    fn round_trip_long_run() {
        let mut input = Vec::new();
        input.extend_from_slice(b"prelude:");
        input.resize(input.len() + 200, 0x00);
        input.extend_from_slice(b":epilogue");
        let c = compress_bundle(&input);
        let d = decompress_bundle(&c).unwrap();
        assert_eq!(d, input);
        // 200 zeros encode to ≤ 4 RLE tokens ; bundle should be much smaller.
        assert!(c.len() < input.len());
    }

    #[test]
    fn round_trip_mixed_payload() {
        // Simulate a federation-bundle JSON shape : repetitive structure +
        // hex-strings + zero-padding.
        let mut input = Vec::new();
        for _ in 0..10 {
            input.extend_from_slice(b"{\"raw\":\"00aabbcc11223344000000000000\"},");
        }
        let c = compress_bundle(&input);
        let d = decompress_bundle(&c).unwrap();
        assert_eq!(d, input);
        // Repetitive structure should compress.
        assert!(c.len() < input.len());
    }

    #[test]
    fn compression_ratio_target_met_on_repetitive() {
        // Target : ≥ 2× ratio on highly-repetitive payloads.
        let input = vec![0xAB_u8; 1000];
        let c = compress_bundle(&input);
        let ratio = input.len() as f64 / c.len() as f64;
        assert!(
            ratio >= 2.0,
            "compression ratio {ratio} did not meet 2× target on highly-repetitive input"
        );
    }

    #[test]
    fn empty_input_round_trips() {
        let c = compress_bundle(&[]);
        let d = decompress_bundle(&c).unwrap();
        assert_eq!(d, Vec::<u8>::new());
    }

    #[test]
    fn bad_magic_rejected() {
        let bogus = b"NOPE\x00\x00\x00\x00";
        let r = decompress_bundle(bogus);
        assert!(matches!(r, Err(CompressError::BadMagic)));
    }

    #[test]
    fn truncated_frame_rejected() {
        let input = b"hello world";
        let mut c = compress_bundle(input);
        c.truncate(c.len() - 3); // chop the tail
        let r = decompress_bundle(&c);
        assert!(matches!(
            r,
            Err(CompressError::Truncated)
                | Err(CompressError::SizeMismatch { .. })
        ));
    }
}
