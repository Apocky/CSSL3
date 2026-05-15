#![forbid(unsafe_code)]
#![doc = "cssl-ocap — unforgeable capability tokens.\n\n\
Spec: `specs/Upgrade/impl/IMPL_02_FOUNDATION.csl` § cssl-ocap. \
A `Grantor` holds a 32-byte BLAKE3 keyed-hash key and mints `CapToken`s for \
specific capability types. Tokens carry `(cap_type, nonce, mac)`. Verification \
recomputes the keyed-hash and compares in constant time."]

use cssl_cas::Cid;
use rand::RngCore;

/// Identifier for a capability type (hash of the capability spec).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CapType(pub Cid);

/// A capability token : `(cap_type, nonce, mac)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapToken {
    pub cap_type: CapType,
    pub nonce: [u8; 32],
    pub mac: [u8; 32],
}

/// Capability grantor : holds the secret key used to mint and verify tokens.
#[derive(Clone)]
pub struct Grantor {
    key: [u8; 32],
}

impl std::fmt::Debug for Grantor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grantor").field("key", &"<redacted>").finish()
    }
}

impl Grantor {
    /// Create a grantor with the given 32-byte key.
    #[must_use]
    pub const fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Mint a fresh token for the given capability type.
    pub fn mint<R: RngCore>(&self, cap_type: CapType, rng: &mut R) -> CapToken {
        let mut nonce = [0u8; 32];
        rng.fill_bytes(&mut nonce);
        let mac = self.compute_mac(&cap_type, &nonce);
        CapToken { cap_type, nonce, mac }
    }

    /// Verify a token : recompute MAC and constant-time compare.
    #[must_use]
    pub fn verify(&self, token: &CapToken) -> bool {
        let expected = self.compute_mac(&token.cap_type, &token.nonce);
        constant_time_eq(&expected, &token.mac)
    }

    fn compute_mac(&self, cap_type: &CapType, nonce: &[u8; 32]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&self.key);
        hasher.update(cap_type.0.as_bytes());
        hasher.update(nonce);
        *hasher.finalize().as_bytes()
    }
}

fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_cas::cid_of_bytes;

    fn ct(seed: u8) -> CapType { CapType(cid_of_bytes(&[seed])) }

    fn grantor(seed: u8) -> Grantor {
        let mut key = [0u8; 32];
        key[0] = seed;
        Grantor::new(key)
    }

    #[test]
    fn mint_then_verify_succeeds() {
        let g = grantor(1);
        let mut rng = rand::thread_rng();
        let t = g.mint(ct(7), &mut rng);
        assert!(g.verify(&t));
    }

    #[test]
    fn tampered_mac_verify_fails() {
        let g = grantor(1);
        let mut rng = rand::thread_rng();
        let mut t = g.mint(ct(7), &mut rng);
        t.mac[0] ^= 1;
        assert!(!g.verify(&t));
    }

    #[test]
    fn tampered_nonce_verify_fails() {
        let g = grantor(1);
        let mut rng = rand::thread_rng();
        let mut t = g.mint(ct(7), &mut rng);
        t.nonce[0] ^= 1;
        assert!(!g.verify(&t));
    }

    #[test]
    fn tampered_cap_type_verify_fails() {
        let g = grantor(1);
        let mut rng = rand::thread_rng();
        let mut t = g.mint(ct(7), &mut rng);
        t.cap_type = ct(8);
        assert!(!g.verify(&t));
    }

    #[test]
    fn distinct_grantors_dont_cross_verify() {
        let g1 = grantor(1);
        let g2 = grantor(2);
        let mut rng = rand::thread_rng();
        let t = g1.mint(ct(7), &mut rng);
        assert!(g1.verify(&t));
        assert!(!g2.verify(&t), "g2 must not accept g1's token");
    }
}
