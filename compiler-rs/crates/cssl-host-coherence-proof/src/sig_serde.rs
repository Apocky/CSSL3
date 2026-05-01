// § sig_serde.rs · serde<-> Ed25519 64-byte signatures (hex-encoded)
// ══════════════════════════════════════════════════════════════════════════════
// § I> serde does NOT auto-impl Deserialize for [u8; 64] (only up to 32)
//   ¬ adding serde-big-array (forbidden : "no new external deps outside workspace")
//   → custom-serde via #[serde(with = "crate::sig_serde")] on SigBytes fields
// § I> Wire-format : 128-char lower-case hex string (canonical)
// ══════════════════════════════════════════════════════════════════════════════
use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};

use crate::tiebreak::hex_lower;

/// Serialize a 64-byte signature as a 128-char lower-case hex string.
pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
    let hex = hex_lower(sig);
    hex.serialize(s)
}

/// Deserialize a 64-byte signature from a 128-char hex string.
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
    let s = String::deserialize(d)?;
    if s.len() != 128 {
        return Err(D::Error::custom(format!(
            "ed25519 sig hex must be 128 chars, got {}",
            s.len()
        )));
    }
    let mut out = [0u8; 64];
    for (i, byte_hex) in s.as_bytes().chunks_exact(2).enumerate() {
        let hi = char_to_nybble(byte_hex[0]).map_err(D::Error::custom)?;
        let lo = char_to_nybble(byte_hex[1]).map_err(D::Error::custom)?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn char_to_nybble(c: u8) -> Result<u8, &'static str> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err("invalid hex char"),
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
    struct Wrap(#[serde(with = "super")] [u8; 64]);

    #[test]
    fn round_trip_zero() {
        let w = Wrap([0u8; 64]);
        let json = serde_json::to_string(&w).unwrap();
        let back: Wrap = serde_json::from_str(&json).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn round_trip_pattern() {
        let mut sig = [0u8; 64];
        for (i, b) in sig.iter_mut().enumerate() {
            *b = i as u8;
        }
        let w = Wrap(sig);
        let json = serde_json::to_string(&w).unwrap();
        let back: Wrap = serde_json::from_str(&json).unwrap();
        assert_eq!(w, back);
    }

    #[test]
    fn rejects_wrong_length() {
        let bad = "\"abcd\"";
        let r: Result<Wrap, _> = serde_json::from_str(bad);
        assert!(r.is_err());
    }
}
