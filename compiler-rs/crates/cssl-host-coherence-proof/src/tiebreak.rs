// § tiebreak.rs · Ed25519-sig hex-asc lexicographic-low-wins
// ══════════════════════════════════════════════════════════════════════════════
// § I> When 2 validators DISAGREE on the new merkle-root, the canonical-winner
//   is the validator whose Ed25519 64-byte detached-signature, encoded as
//   lower-case hex (128 chars), sorts FIRST under standard lexicographic order.
// § I> THE LEXICOGRAPHICALLY-LOWEST hex WINS.
// § I> rationale : reproducible · uniform · auditable · Σ-mask-friendly
// ══════════════════════════════════════════════════════════════════════════════
use crate::event::SigBytes;

/// Encode 64-byte signature as 128-char lower-hex.
pub fn hex_lower(sig: &SigBytes) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(128);
    for b in sig {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Returns the index (`0` or `1`) of the WINNING signature : lower-hex-asc.
///
/// Used by `consensus` to deterministically resolve a 2-validator disagreement.
/// Ties (equal hex) resolve to `0` (first-side-wins, deterministic).
pub fn ed25519_hex_asc_winner(sig_a: &SigBytes, sig_b: &SigBytes) -> usize {
    let a = hex_lower(sig_a);
    let b = hex_lower(sig_b);
    usize::from(a > b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_lower_zero_sig_is_all_zeros() {
        let s = hex_lower(&[0u8; 64]);
        assert_eq!(s.len(), 128);
        assert!(s.chars().all(|c| c == '0'));
    }

    #[test]
    fn hex_lower_distinguishes_bytes() {
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        a[0] = 0x0a; // → "0a..."
        b[0] = 0xff; // → "ff..."
        let ha = hex_lower(&a);
        let hb = hex_lower(&b);
        assert!(ha < hb);
    }

    #[test]
    fn winner_is_lex_lowest() {
        let mut lo = [0u8; 64];
        let mut hi = [0u8; 64];
        lo[0] = 0x01;
        hi[0] = 0xfe;
        assert_eq!(ed25519_hex_asc_winner(&lo, &hi), 0);
        assert_eq!(ed25519_hex_asc_winner(&hi, &lo), 1);
    }

    #[test]
    fn equal_sigs_returns_zero() {
        let a = [42u8; 64];
        let b = [42u8; 64];
        // ties resolved as `0` (first wins, deterministic).
        assert_eq!(ed25519_hex_asc_winner(&a, &b), 0);
    }
}
