// § rng.rs — Deterministic RNG (xoshiro128++ stage-0) + seed-derivation
// ════════════════════════════════════════════════════════════════════
// § STAGE-0 : we deliberately do NOT pull `rand` here · this crate must
//   produce bit-equal pull-rolls across hosts for replay-validation and
//   for Σ-Chain re-attribution. xoshiro128++ is small · well-tested · has
//   a 2^128 - 1 period · suitable for non-crypto deterministic gameplay.
// § ATTESTATION : seed derived via blake3-keyed-MAC of (pubkey || banner_id
//   || pull_index) — this means the player's pubkey is the ONLY entropy
//   source under their control · no server-side secret can bias the roll.
// ════════════════════════════════════════════════════════════════════

/// § DetRng — xoshiro128++ deterministic generator. Stable across hosts ;
/// re-seedable via `from_seed(u128)`. Output `next_u32` is uniform on u32.
#[derive(Debug, Clone, Copy)]
pub struct DetRng {
    state: [u32; 4],
}

impl DetRng {
    /// Construct from a 128-bit seed. The seed is split into four u32 lanes ;
    /// any all-zero seed is replaced with a canonical non-zero default
    /// (xoshiro requires non-zero seed).
    #[must_use]
    pub const fn from_seed(seed: u128) -> Self {
        let lanes = [
            (seed & 0xFFFF_FFFF) as u32,
            ((seed >> 32) & 0xFFFF_FFFF) as u32,
            ((seed >> 64) & 0xFFFF_FFFF) as u32,
            ((seed >> 96) & 0xFFFF_FFFF) as u32,
        ];
        let any_nonzero = lanes[0] | lanes[1] | lanes[2] | lanes[3];
        let final_lanes = if any_nonzero == 0 {
            // SplitMix-style non-zero replacement constants.
            [0x9E37_79B9, 0x517C_C1B7, 0x6510_5C53, 0xCBA9_4F2C]
        } else {
            lanes
        };
        Self { state: final_lanes }
    }

    /// Next 32-bit uniform random integer.
    pub fn next_u32(&mut self) -> u32 {
        let result = self.state[0]
            .wrapping_add(self.state[3])
            .rotate_left(7)
            .wrapping_add(self.state[0]);

        let t = self.state[1] << 9;

        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];

        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(11);

        result
    }

    /// Next u32 in `[0, ceil)` via biased-modular method · acceptable for
    /// gameplay (bias is ~1/2^32 worst-case for our basis-point sizes).
    /// Returns 0 if `ceil == 0` (caller must guard).
    pub fn next_u32_below(&mut self, ceil: u32) -> u32 {
        if ceil == 0 {
            return 0;
        }
        self.next_u32() % ceil
    }
}

/// § derive_seed_from_pubkey — blake3-keyed-MAC of (pubkey || banner_id
/// || pull_index). Public-input only — no server-side secret. The first
/// 16 bytes of the hash form the u128 seed.
///
/// `pubkey_bytes` is whatever the player's identity-key serializes as
/// (Ed25519 32-byte by convention). `banner_id` and `pull_index` ensure
/// disjoint roll-streams across banners and across pulls.
#[must_use]
pub fn derive_seed_from_pubkey(
    pubkey_bytes: &[u8],
    banner_id: &str,
    pull_index: u64,
) -> u128 {
    // Domain-separation tag : keep the keyed-mode anchored to this crate
    // so unrelated keyed-modes can't collide. blake3::keyed_hash requires
    // a 32-byte key — we derive it from a fixed-domain string.
    const KEY: [u8; 32] = *b"cssl-host-gacha:seed-deriv-v1!!!";
    let mut hasher = blake3::Hasher::new_keyed(&KEY);
    hasher.update(pubkey_bytes);
    hasher.update(b"|banner=");
    hasher.update(banner_id.as_bytes());
    hasher.update(b"|pull_idx=");
    hasher.update(&pull_index.to_le_bytes());
    let hash = hasher.finalize();
    let bytes = hash.as_bytes();
    let mut s = [0u8; 16];
    s.copy_from_slice(&bytes[..16]);
    u128::from_le_bytes(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_for_same_seed() {
        let mut a = DetRng::from_seed(0x1234_5678_9ABC_DEF0_1122_3344_5566_7788);
        let mut b = DetRng::from_seed(0x1234_5678_9ABC_DEF0_1122_3344_5566_7788);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn rng_diverges_for_different_seeds() {
        let mut a = DetRng::from_seed(1);
        let mut b = DetRng::from_seed(2);
        let mut diff = 0;
        for _ in 0..100 {
            if a.next_u32() != b.next_u32() {
                diff += 1;
            }
        }
        assert!(diff > 80, "expected substantial divergence, got {diff}");
    }

    #[test]
    fn zero_seed_replaced_with_canonical_default() {
        let mut a = DetRng::from_seed(0);
        // Should NOT lock-up at zero.
        let n = a.next_u32();
        assert_ne!(n, 0);
    }

    #[test]
    fn derive_seed_stable_for_same_input() {
        let s1 = derive_seed_from_pubkey(b"pubkey-32-bytes-fixed-content!!!", "banner-A", 7);
        let s2 = derive_seed_from_pubkey(b"pubkey-32-bytes-fixed-content!!!", "banner-A", 7);
        assert_eq!(s1, s2);
    }

    #[test]
    fn derive_seed_diverges_per_pull_index() {
        let s1 = derive_seed_from_pubkey(b"pk", "banner-A", 1);
        let s2 = derive_seed_from_pubkey(b"pk", "banner-A", 2);
        assert_ne!(s1, s2);
    }

    #[test]
    fn derive_seed_diverges_per_banner() {
        let s1 = derive_seed_from_pubkey(b"pk", "banner-A", 1);
        let s2 = derive_seed_from_pubkey(b"pk", "banner-B", 1);
        assert_ne!(s1, s2);
    }

    #[test]
    fn next_u32_below_ranges_correctly() {
        let mut r = DetRng::from_seed(42);
        for _ in 0..1000 {
            let v = r.next_u32_below(100_000);
            assert!(v < 100_000);
        }
    }

    #[test]
    fn next_u32_below_zero_returns_zero() {
        let mut r = DetRng::from_seed(42);
        assert_eq!(r.next_u32_below(0), 0);
    }
}
