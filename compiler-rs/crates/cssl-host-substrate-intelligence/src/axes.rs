//! § axes — 8 substrate-resonance axes derived deterministically from inputs.
//!
//! § ALGORITHM
//!   BLAKE3(role · kind · seed · params) → 32-byte digest. The first 8 bytes
//!   directly map to the 8 axes (each a `u8` in `[0, 255]`). The remaining
//!   24 bytes feed the morpheme entropy stream consumed by `composer`.
//!
//! § NOVELTY
//!   The axes are NOT a phrase-table-index. They are continuous parameters
//!   that drive morphological synthesis. Two distinct seeds produce two
//!   distinct 8-axis vectors → two distinct generated texts.

use crate::{ComposeKind, Role};

/// 8-axis substrate-resonance vector. Each axis is a `u8` in `[0, 255]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubstrateAxes {
    /// somber (0) → playful (255)
    pub solemnity: u8,
    /// terse (0) → ornate (255)
    pub verbosity: u8,
    /// modern (0) → ancient (255)
    pub antiquity: u8,
    /// clear (0) → cryptic (255)
    pub mystery: u8,
    /// calm (0) → urgent (255)
    pub dynamism: u8,
    /// formal (0) → personal (255)
    pub intimacy: u8,
    /// abstract (0) → concrete (255)
    pub concreteness: u8,
    /// low (0) → high pitch (255)
    pub resonance: u8,
    /// 24-byte entropy reserve for composer state machines.
    pub entropy: [u8; 24],
}

impl SubstrateAxes {
    /// Derive deterministic axes from `(role, kind, seed, params)`.
    pub fn derive(role: Role, kind: ComposeKind, seed: u64, params: &[u8]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(&(role as u32).to_le_bytes());
        h.update(&(kind as u32).to_le_bytes());
        h.update(&seed.to_le_bytes());
        h.update(params);
        // Mix in role-specific salt so adjacent kinds don't share axis-bits.
        let salt: [u8; 16] = match role {
            Role::Gm => *b"loa-gm-narrator0",
            Role::Dm => *b"loa-dm-director0",
            Role::Collaborator => *b"loa-collab-coaut",
            Role::Coder => *b"loa-coder-mutate",
        };
        h.update(&salt);
        let digest: [u8; 32] = h.finalize().into();
        let mut entropy = [0u8; 24];
        entropy.copy_from_slice(&digest[8..32]);
        Self {
            solemnity: digest[0],
            verbosity: digest[1],
            antiquity: digest[2],
            mystery: digest[3],
            dynamism: digest[4],
            intimacy: digest[5],
            concreteness: digest[6],
            resonance: digest[7],
            entropy,
        }
    }

    /// Sample a fresh u32 from the entropy reserve at `byte_offset`. The
    /// caller MUST not call with `byte_offset > 20` (reserve is 24 bytes,
    /// reading 4 from offset 20 is the last legal sample).
    pub fn entropy_u32_at(&self, byte_offset: usize) -> u32 {
        debug_assert!(byte_offset + 4 <= self.entropy.len());
        let off = byte_offset.min(self.entropy.len().saturating_sub(4));
        u32::from_le_bytes([
            self.entropy[off],
            self.entropy[off + 1],
            self.entropy[off + 2],
            self.entropy[off + 3],
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_deterministic() {
        let a = SubstrateAxes::derive(Role::Gm, ComposeKind::DialogueLine, 1, b"x");
        let b = SubstrateAxes::derive(Role::Gm, ComposeKind::DialogueLine, 1, b"x");
        assert_eq!(a, b);
    }

    #[test]
    fn axes_change_with_role() {
        let a = SubstrateAxes::derive(Role::Gm, ComposeKind::DialogueLine, 1, b"");
        let b = SubstrateAxes::derive(Role::Dm, ComposeKind::DialogueLine, 1, b"");
        assert_ne!(a, b);
    }

    #[test]
    fn axes_change_with_seed() {
        let a = SubstrateAxes::derive(Role::Gm, ComposeKind::DialogueLine, 1, b"");
        let b = SubstrateAxes::derive(Role::Gm, ComposeKind::DialogueLine, 2, b"");
        assert_ne!(a, b);
    }

    #[test]
    fn entropy_offsets_within_reserve() {
        let a = SubstrateAxes::derive(Role::Gm, ComposeKind::DialogueLine, 1, b"");
        let _ = a.entropy_u32_at(0);
        let _ = a.entropy_u32_at(20);
    }
}
