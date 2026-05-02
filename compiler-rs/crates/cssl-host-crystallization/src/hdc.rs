//! § hdc — 256-bit Hyperdimensional-Computing vector for crystal semantic identity.
//!
//! HDC primitives :
//!   - `bind`     : XOR (commutative · involutive · preserves Hamming-distance)
//!   - `bundle`   : majority-vote across many vectors (returns "centroid")
//!   - `permute`  : bit-rotation (encodes position / sequence)
//!   - `similarity` : Hamming-distance / 256 → fraction
//!
//! The 256-bit width is a stage-0 compromise between memory + speed. Real
//! HDC papers use 10000-bit or larger, but 256-bit gives sufficient
//! ε-distinguishability for our crystal-count (≤ 10000 crystals per scene
//! gives ~4% expected collision).

/// 256-bit binary HDC vector (4× u64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HdcVec256 {
    pub words: [u64; 4],
}

impl HdcVec256 {
    pub const ZERO: Self = Self { words: [0; 4] };

    pub const fn new(words: [u64; 4]) -> Self {
        Self { words }
    }

    /// Derive a deterministic HDC vector from a 32-byte digest.
    pub fn derive(digest: &[u8; 32]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"hdc-derive-v1");
        h.update(digest);
        let bytes: [u8; 32] = h.finalize().into();
        let words = [
            u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
                bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            u64::from_le_bytes([
                bytes[8], bytes[9], bytes[10], bytes[11],
                bytes[12], bytes[13], bytes[14], bytes[15],
            ]),
            u64::from_le_bytes([
                bytes[16], bytes[17], bytes[18], bytes[19],
                bytes[20], bytes[21], bytes[22], bytes[23],
            ]),
            u64::from_le_bytes([
                bytes[24], bytes[25], bytes[26], bytes[27],
                bytes[28], bytes[29], bytes[30], bytes[31],
            ]),
        ];
        Self { words }
    }

    /// XOR-bind two vectors. Commutative + involutive : `a.bind(a) == ZERO`.
    pub fn bind(&self, other: &Self) -> Self {
        Self {
            words: [
                self.words[0] ^ other.words[0],
                self.words[1] ^ other.words[1],
                self.words[2] ^ other.words[2],
                self.words[3] ^ other.words[3],
            ],
        }
    }

    /// Cyclic bit-permutation by `n` (encodes position).
    pub fn permute(&self, n: u32) -> Self {
        let n = n & 255;
        let word_shift = (n / 64) as usize;
        let bit_shift = n % 64;
        let mut out = [0u64; 4];
        for i in 0..4 {
            let src_a = (i + word_shift) % 4;
            let src_b = (i + word_shift + 1) % 4;
            if bit_shift == 0 {
                out[i] = self.words[src_a];
            } else {
                out[i] =
                    (self.words[src_a] << bit_shift) | (self.words[src_b] >> (64 - bit_shift));
            }
        }
        Self { words: out }
    }

    /// Hamming distance (number of differing bits, 0..=256).
    pub fn hamming(&self, other: &Self) -> u32 {
        let mut d = 0;
        for i in 0..4 {
            d += (self.words[i] ^ other.words[i]).count_ones();
        }
        d
    }

    /// Similarity in 0..=255 (0 = identical · 255 = nothing in common).
    /// Stage-0 returns Hamming / 1.0 mapped onto u8 with rounding.
    pub fn similarity(&self, other: &Self) -> u8 {
        let h = self.hamming(other);
        // 256 bits possible; map to 0..=255.
        h.min(255) as u8
    }

    /// Resonance: 255 - similarity (255 = identical · 0 = orthogonal).
    pub fn resonance(&self, other: &Self) -> u8 {
        255u8.saturating_sub(self.similarity(other))
    }
}

/// Bundle (majority-vote) up to 8 vectors. Returns the majority-vote
/// centroid.
pub fn bundle(vecs: &[HdcVec256]) -> HdcVec256 {
    if vecs.is_empty() {
        return HdcVec256::ZERO;
    }
    let mut count = [[0u32; 64]; 4]; // count[word][bit]
    for v in vecs {
        for w in 0..4 {
            for b in 0..64 {
                if (v.words[w] >> b) & 1 != 0 {
                    count[w][b] += 1;
                }
            }
        }
    }
    let threshold = (vecs.len() as u32) / 2;
    let mut out = [0u64; 4];
    for w in 0..4 {
        for b in 0..64 {
            if count[w][b] > threshold {
                out[w] |= 1u64 << b;
            }
        }
    }
    HdcVec256 { words: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_is_deterministic() {
        let d = [42u8; 32];
        let a = HdcVec256::derive(&d);
        let b = HdcVec256::derive(&d);
        assert_eq!(a, b);
    }

    #[test]
    fn bind_is_involutive() {
        let v = HdcVec256::derive(&[1u8; 32]);
        assert_eq!(v.bind(&v), HdcVec256::ZERO);
    }

    #[test]
    fn bind_is_commutative() {
        let a = HdcVec256::derive(&[1u8; 32]);
        let b = HdcVec256::derive(&[2u8; 32]);
        assert_eq!(a.bind(&b), b.bind(&a));
    }

    #[test]
    fn hamming_is_bounded() {
        let a = HdcVec256::derive(&[1u8; 32]);
        let b = HdcVec256::derive(&[2u8; 32]);
        let h = a.hamming(&b);
        assert!(h <= 256);
    }

    #[test]
    fn self_resonance_is_max() {
        let a = HdcVec256::derive(&[1u8; 32]);
        assert_eq!(a.resonance(&a), 255);
    }

    #[test]
    fn permute_changes_vector() {
        let a = HdcVec256::derive(&[1u8; 32]);
        let p = a.permute(7);
        assert_ne!(a, p);
    }

    #[test]
    fn bundle_of_one_returns_input() {
        let a = HdcVec256::derive(&[1u8; 32]);
        let b = bundle(&[a]);
        // With 1 vector, threshold = 0, so all bits ≥ 1 vote remain set.
        assert_eq!(a, b);
    }
}
