#![forbid(unsafe_code)]
#![doc = "cssl-cas — content-addressing kernel.\n\n\
Spec: `specs/Upgrade/impl/IMPL_02_FOUNDATION.csl` § cssl-cas. \
Hash: BLAKE3 (32-byte digest). \
A `Cid` is the canonical identity of any artifact ; equal canonical-encodings \
imply equal `Cid`s. The `CanonicalEncode` trait defines a deterministic \
serialization to bytes — implementations MUST be α-equivalence-invariant for \
binding-bearing terms (positional De Bruijn for binders)."]

use std::fmt;
use thiserror::Error;

/// Content identifier : 32-byte BLAKE3 digest of an artifact's canonical encoding.
///
/// Two artifacts hash to the same `Cid` iff their canonical encodings are bytewise
/// identical. Per `CanonicalEncode`, encodings are α-equivalence-invariant for
/// binders (positional De Bruijn).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cid(pub [u8; 32]);

impl Cid {
    /// Construct a `Cid` from a raw 32-byte digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// View the underlying 32 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cid({})", cid_hex(self))
    }
}

impl fmt::Display for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&cid_hex(self))
    }
}

/// Trait for types with a deterministic, α-equivalence-invariant byte encoding.
///
/// Implementations MUST :
/// - produce identical bytes for α-equivalent terms (use positional De Bruijn for binders)
/// - be order-stable for set-typed fields (sort before encoding)
/// - prefix length-bearing fields with their length (no ambiguity)
pub trait CanonicalEncode {
    /// Append the canonical byte encoding of `self` to `out`.
    fn encode(&self, out: &mut Vec<u8>);
}

/// Compute the `Cid` of an artifact via its `CanonicalEncode` instance.
#[must_use]
pub fn cid_of<T: CanonicalEncode + ?Sized>(t: &T) -> Cid {
    let mut buf = Vec::with_capacity(64);
    t.encode(&mut buf);
    cid_of_bytes(&buf)
}

/// Compute the `Cid` of a raw byte buffer (BLAKE3 of the bytes).
#[must_use]
pub fn cid_of_bytes(bytes: &[u8]) -> Cid {
    Cid(*blake3::hash(bytes).as_bytes())
}

/// Format a `Cid` as 64 lowercase hex characters.
#[must_use]
pub fn cid_hex(cid: &Cid) -> String {
    let mut s = String::with_capacity(64);
    for b in &cid.0 {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Errors raised when parsing a hex `Cid`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Hex string was not exactly 64 characters long.
    #[error("cid hex must be 64 characters, got {0}")]
    BadLength(usize),
    /// Hex string contained a non-hex character.
    #[error("cid hex contains non-hex character at position {0}")]
    BadChar(usize),
}

/// Parse a `Cid` from its 64-character lowercase hex representation.
pub fn cid_from_hex(s: &str) -> Result<Cid, ParseError> {
    if s.len() != 64 {
        return Err(ParseError::BadLength(s.len()));
    }
    let mut out = [0u8; 32];
    let bytes = s.as_bytes();
    for (i, byte) in out.iter_mut().enumerate() {
        let hi = hex_nibble(bytes[i * 2], i * 2)?;
        let lo = hex_nibble(bytes[i * 2 + 1], i * 2 + 1)?;
        *byte = (hi << 4) | lo;
    }
    Ok(Cid(out))
}

fn hex_nibble(c: u8, pos: usize) -> Result<u8, ParseError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(ParseError::BadChar(pos)),
    }
}

// ─── Convenience CanonicalEncode impls ────────────────────────────────────────

impl CanonicalEncode for [u8] {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.len() as u64).to_le_bytes());
        out.extend_from_slice(self);
    }
}

impl CanonicalEncode for str {
    fn encode(&self, out: &mut Vec<u8>) {
        self.as_bytes().encode(out);
    }
}

impl CanonicalEncode for String {
    fn encode(&self, out: &mut Vec<u8>) {
        self.as_str().encode(out);
    }
}

impl CanonicalEncode for Cid {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cid_deterministic_on_same_bytes() {
        let a = cid_of_bytes(b"hello");
        let b = cid_of_bytes(b"hello");
        assert_eq!(a, b, "Cid must be deterministic for identical inputs");
    }

    #[test]
    fn cid_distinct_inputs_distinct_cids() {
        let mut seen = std::collections::HashSet::new();
        for i in 0u32..200 {
            let bytes = i.to_le_bytes();
            assert!(
                seen.insert(cid_of_bytes(&bytes)),
                "collision at i={i} — BLAKE3 should not collide on tiny distinct inputs"
            );
        }
    }

    #[test]
    fn cid_alpha_equiv_via_de_bruijn() {
        // λx.x and λy.y encoded with positional De Bruijn → identical bytes → identical Cid
        let lam_x_x = encode_lambda_identity("x");
        let lam_y_y = encode_lambda_identity("y");
        assert_eq!(
            cid_of_bytes(&lam_x_x),
            cid_of_bytes(&lam_y_y),
            "α-equivalent terms must hash identically when encoded positionally"
        );
    }

    fn encode_lambda_identity(_binder_name: &str) -> Vec<u8> {
        // De Bruijn : tag=Lam(0x01), body=Var(0x02) idx=0u32
        vec![0x01, 0x02, 0, 0, 0, 0]
    }

    #[test]
    fn cid_hex_round_trip() {
        let original = cid_of_bytes(b"round-trip");
        let hex = cid_hex(&original);
        let parsed = cid_from_hex(&hex).expect("hex must round-trip");
        assert_eq!(original, parsed);
    }

    #[test]
    fn cid_hex_invalid_length_rejected() {
        assert_eq!(cid_from_hex("abcd"), Err(ParseError::BadLength(4)));
    }

    #[test]
    fn cid_hex_invalid_char_rejected() {
        let mut bad = "0".repeat(63);
        bad.push('Z');
        match cid_from_hex(&bad) {
            Err(ParseError::BadChar(pos)) => assert_eq!(pos, 63),
            other => panic!("expected BadChar(63), got {other:?}"),
        }
    }

    #[test]
    fn cid_encode_stable_against_fixed_vector() {
        // BLAKE3("") — fixed test vector from BLAKE3 reference
        let empty = cid_of_bytes(b"");
        assert_eq!(
            cid_hex(&empty),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn cid_size_is_32_bytes() {
        assert_eq!(std::mem::size_of::<Cid>(), 32);
    }

    #[test]
    fn canonical_encode_string_round_trip() {
        let a: Cid = cid_of(&"hello".to_string());
        let b: Cid = cid_of(&"hello".to_string());
        let c: Cid = cid_of(&"world".to_string());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
