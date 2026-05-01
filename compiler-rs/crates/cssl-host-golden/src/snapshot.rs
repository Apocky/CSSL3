// § snapshot.rs : Snapshot type + FNV-1a-128 fingerprint
// ══════════════════════════════════════════════════════════════════
// § I> Snapshot wraps raw RGBA buffer + dimensions + perceptual fingerprint
// § I> fingerprint = FNV-1a-128 hex-encoded (stdlib-only ; ¬ crypto)
// § I> docstring per task-prompt : "perceptual fingerprint hash, not crypto"

use serde::{Deserialize, Serialize};
use std::fmt;

/// Snapshot of a rendered frame : RGBA bytes + dimensions + metadata.
///
/// # Fingerprint
/// `sha256_hex` is named historically but actually contains a hex-encoded
/// FNV-1a-128 fingerprint of the RGBA buffer. This is a *perceptual
/// fingerprint hash, not crypto* : it is deterministic and detects bit-exact
/// regressions cheaply but provides no collision resistance against an
/// adversary. For golden-image regression that is sufficient — the diff
/// pipeline (`crate::diff`) is the source-of-truth for "did pixels change".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub sha256_hex: String,
    pub ts_iso: String,
    pub label: String,
}

/// Snapshot construction errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapErr {
    /// `rgba.len() != width * height * 4`.
    LengthMismatch { expected: usize, actual: usize },
    /// width or height was zero.
    ZeroDim,
}

impl fmt::Display for SnapErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { expected, actual } => {
                write!(f, "rgba length mismatch : expected {expected} bytes got {actual}")
            }
            Self::ZeroDim => write!(f, "snapshot dimensions must be non-zero"),
        }
    }
}

impl std::error::Error for SnapErr {}

/// Construct a [`Snapshot`] from a raw RGBA byte buffer.
///
/// `ts_iso` is currently always `"1970-01-01T00:00:00Z"` (host-clock-free
/// stage-0 default). Callers that need a real timestamp can mutate the
/// returned struct directly.
pub fn from_rgba(
    label: String,
    rgba: Vec<u8>,
    w: u32,
    h: u32,
) -> Result<Snapshot, SnapErr> {
    if w == 0 || h == 0 {
        return Err(SnapErr::ZeroDim);
    }
    let expected = (w as usize) * (h as usize) * 4;
    if rgba.len() != expected {
        return Err(SnapErr::LengthMismatch { expected, actual: rgba.len() });
    }
    let sha256_hex = fingerprint_hex(&rgba);
    Ok(Snapshot {
        width: w,
        height: h,
        rgba,
        sha256_hex,
        ts_iso: "1970-01-01T00:00:00Z".to_string(),
        label,
    })
}

// ─────────────────────────────────────────────────────────────────
// FNV-1a-128 fingerprint (stdlib-only)
// ─────────────────────────────────────────────────────────────────

const FNV_OFFSET_128_HI: u64 = 0x6c62_272e_07bb_0142;
const FNV_OFFSET_128_LO: u64 = 0x62b8_2175_6295_c58d;
const FNV_PRIME_128_HI: u64 = 0x0000_0000_0100_0000;
const FNV_PRIME_128_LO: u64 = 0x0000_0000_0000_013b;

/// Compute FNV-1a-128 of `bytes` and return as 32-char lower-hex.
///
/// Implementation : 128-bit accumulator stored as `(hi, lo)` u64 pair.
/// Each byte XORs into the low byte of `lo`, then the accumulator is
/// multiplied by the 128-bit FNV prime via 64x64 → 128 partial products.
fn fingerprint_hex(bytes: &[u8]) -> String {
    let mut hi = FNV_OFFSET_128_HI;
    let mut lo = FNV_OFFSET_128_LO;
    for &b in bytes {
        lo ^= u64::from(b);
        let (new_hi, new_lo) = mul_128(hi, lo, FNV_PRIME_128_HI, FNV_PRIME_128_LO);
        hi = new_hi;
        lo = new_lo;
    }
    let mut out = String::with_capacity(32);
    for byte in hi.to_be_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    for byte in lo.to_be_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 128 × 128 → 128 mul (truncated, wrapping).
fn mul_128(a_hi: u64, a_lo: u64, b_hi: u64, b_lo: u64) -> (u64, u64) {
    // (a_hi*2^64 + a_lo) * (b_hi*2^64 + b_lo)
    //   = a_hi*b_hi * 2^128                       ← discarded (overflow)
    //   + (a_hi*b_lo + a_lo*b_hi) * 2^64          ← contributes to hi
    //   + a_lo * b_lo                             ← contributes to lo+carry-to-hi
    let lo_lo = u128::from(a_lo) * u128::from(b_lo);
    let lo_lo_hi = (lo_lo >> 64) as u64;
    let lo_lo_lo = lo_lo as u64;
    let cross = a_hi.wrapping_mul(b_lo).wrapping_add(a_lo.wrapping_mul(b_hi));
    let hi = cross.wrapping_add(lo_lo_hi);
    (hi, lo_lo_lo)
}

// ─────────────────────────────────────────────────────────────────
// tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_snapshot() {
        let rgba = vec![0u8; 2 * 2 * 4];
        let snap = from_rgba("frame-A".to_string(), rgba, 2, 2).expect("valid");
        assert_eq!(snap.width, 2);
        assert_eq!(snap.height, 2);
        assert_eq!(snap.rgba.len(), 16);
        assert_eq!(snap.sha256_hex.len(), 32);
        assert_eq!(snap.label, "frame-A");
    }

    #[test]
    fn length_mismatch_rejected() {
        let rgba = vec![0u8; 15]; // expected 16
        let err = from_rgba("bad".to_string(), rgba, 2, 2).unwrap_err();
        match err {
            SnapErr::LengthMismatch { expected, actual } => {
                assert_eq!(expected, 16);
                assert_eq!(actual, 15);
            }
            SnapErr::ZeroDim => panic!("wrong err : ZeroDim"),
        }
    }

    #[test]
    fn zero_dim_rejected() {
        let rgba = vec![];
        assert_eq!(
            from_rgba("zero-w".to_string(), rgba.clone(), 0, 4).unwrap_err(),
            SnapErr::ZeroDim
        );
        assert_eq!(
            from_rgba("zero-h".to_string(), rgba, 4, 0).unwrap_err(),
            SnapErr::ZeroDim
        );
    }

    #[test]
    fn sha_deterministic() {
        let rgba_a = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let rgba_b = rgba_a.clone();
        let s1 = from_rgba("x".to_string(), rgba_a, 2, 2).unwrap();
        let s2 = from_rgba("x".to_string(), rgba_b, 2, 2).unwrap();
        assert_eq!(s1.sha256_hex, s2.sha256_hex);
        // and changes when bytes change :
        let mut rgba_c = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        rgba_c[0] = 99;
        let s3 = from_rgba("x".to_string(), rgba_c, 2, 2).unwrap();
        assert_ne!(s1.sha256_hex, s3.sha256_hex);
    }
}
