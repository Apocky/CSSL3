// § T11-W11-SIGMA-CHAIN-ENTRY : LedgerEntry struct + canonical serialization
// §§ spec : specs/28_SIGMA_CHAIN_BOOTSTRAP.csl § ENTRY
// §§ layout : 200 bytes per entry · bit-packed · gapless monotonic seq_no
// §§ canonical-bytes-order : seq_no | ts_unix | kind | actor_pubkey | payload_hash | prev_root
//     ← signature signs THESE bytes ; signature itself is appended after-sign

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// EntryKind discriminant — u16-packed · open-set for sibling-crates to extend.
///
/// § extension-rule : new variants → reserve number-range with sibling-crate ;
///     never re-purpose existing variant values · canonical-bytes depend on stable encoding.
#[repr(u16)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[derive(Serialize, Deserialize)]
pub enum EntryKind {
    /// § sibling : cssl-substrate-prime-directive · cssl-host-attestation
    /// §§ semantics : actor was granted capability ; payload = cap-spec
    CapGrant = 0x0001,

    /// § sibling : cssl-substrate-prime-directive · cssl-host-attestation
    /// §§ semantics : actor revoked previously-granted capability
    CapRevoke = 0x0002,

    /// § sibling : cssl-substrate-omega-field · cssl-host-akashic-records
    /// §§ semantics : ω-field cell-state was written ; payload = cell-bytes
    CellEmission = 0x0003,

    /// § sibling : cssl-host-attestation · cssl-host-coherence-proof
    /// §§ semantics : higher-level attestation anchored ; payload = attestation-doc
    AttestationAnchor = 0x0004,

    /// § auto-emitted every CHECKPOINT_INTERVAL entries
    /// §§ semantics : Merkle-root snapshot · enables O(M)-from-checkpoint replay
    CheckpointMark = 0x0005,

    /// § sibling : cssl-hotfix-stream
    /// §§ semantics : hotfix-bundle deployed · payload = bundle-sig
    HotfixBundle = 0x0006,

    /// § sibling : cssl-mycelium-chat-sync
    /// §§ semantics : federated chat-pattern anchored cross-peer
    MyceliumPattern = 0x0007,

    /// § sibling : cssl-substrate-knowledge
    /// §§ semantics : knowledge-graph ingestion event
    KnowledgeIngest = 0x0008,

    /// § sibling : federation peer-sync
    /// §§ semantics : cross-peer checkpoint-root anchored locally
    FederationAnchor = 0x0009,

    /// § fallback for unknown discriminants during deserialize
    /// §§ semantics : preserve forward-compat ; sibling-crates may emit numbers >0x0FFF
    OpenExtension = 0x0FFF,
}

impl EntryKind {
    /// § round-trip-encode : u16 → variant ; OpenExtension fallback for unknown numbers.
    #[must_use]
    pub fn from_u16(n: u16) -> Self {
        match n {
            0x0001 => Self::CapGrant,
            0x0002 => Self::CapRevoke,
            0x0003 => Self::CellEmission,
            0x0004 => Self::AttestationAnchor,
            0x0005 => Self::CheckpointMark,
            0x0006 => Self::HotfixBundle,
            0x0007 => Self::MyceliumPattern,
            0x0008 => Self::KnowledgeIngest,
            0x0009 => Self::FederationAnchor,
            _ => Self::OpenExtension,
        }
    }

    /// § raw-discriminant for canonical-bytes encoding.
    #[must_use]
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// LedgerEntry — 200 bytes packed · canonical-bytes-stable for signing.
///
/// § layout (200 bytes total) :
///     8  bytes : seq_no       (u64 BE)
///     8  bytes : ts_unix      (u64 BE)
///     2  bytes : kind         (u16 BE)
///     32 bytes : actor_pubkey (Ed25519 pubkey)
///     32 bytes : payload_hash (BLAKE3 32-byte digest of payload)
///     32 bytes : prev_root    (Merkle root before this entry · BLAKE3 32B)
///     64 bytes : signature    (Ed25519 sig over preceding 114 bytes)
///     ─────────────────
///     8 + 8 + 2 + 32 + 32 + 32 + 64 = 178 bytes wire-encoded
///     Rust struct may pad to 200 with alignment ; the SIZE_HINT below is the
///     wire-encoded size used for storage budgeting.
///
/// § signing-domain : signature signs the FIRST 114 bytes (everything except sig itself).
/// §§ serde-note : `signature: [u8; 64]` exceeds serde's auto-impl ceiling (32) ;
///     callers wanting wire-format use `canonical_bytes_for_sign()` + manual sig append.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerEntry {
    /// monotonic · gapless · 0 = genesis-marker only
    pub seq_no: u64,
    /// append timestamp · seconds-since-epoch · monotonic-non-strictly-required
    pub ts_unix: u64,
    /// EntryKind · u16 discriminant
    pub kind: EntryKind,
    /// Ed25519 pubkey of who-attested
    pub actor_pubkey: [u8; 32],
    /// BLAKE3 hash of payload (payload stored separately · keep ledger small)
    pub payload_hash: [u8; 32],
    /// Merkle-root BEFORE this entry (genesis prev_root = ZERO_HASH)
    pub prev_root: [u8; 32],
    /// Ed25519 signature over canonical_bytes_for_sign() output
    pub signature: [u8; 64],
}

/// Wire-encoded byte size · used for storage budgeting + concurrent-append calc.
pub const ENTRY_WIRE_SIZE: usize = 178;

/// Domain separator for entry-canonical-bytes · prevents cross-protocol replay.
pub const ENTRY_DOMAIN: &[u8] = b"cssl-substrate-sigma-chain/v0/entry";

impl LedgerEntry {
    /// § canonical-bytes-for-sign : the 114 bytes Ed25519 signs over.
    /// §§ excludes self.signature ; includes domain-separator for cross-protocol-replay-prevention.
    #[must_use]
    pub fn canonical_bytes_for_sign(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ENTRY_DOMAIN.len() + 114);
        bytes.extend_from_slice(ENTRY_DOMAIN);
        bytes.extend_from_slice(&self.seq_no.to_be_bytes());
        bytes.extend_from_slice(&self.ts_unix.to_be_bytes());
        bytes.extend_from_slice(&self.kind.as_u16().to_be_bytes());
        bytes.extend_from_slice(&self.actor_pubkey);
        bytes.extend_from_slice(&self.payload_hash);
        bytes.extend_from_slice(&self.prev_root);
        bytes
    }

    /// § entry-leaf-hash : BLAKE3 over full entry (incl. signature) · feeds Merkle.
    /// §§ different from sign-bytes ← Merkle includes signature so tampering with sig is detected.
    #[must_use]
    pub fn leaf_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"cssl-substrate-sigma-chain/v0/leaf");
        hasher.update(&self.seq_no.to_be_bytes());
        hasher.update(&self.ts_unix.to_be_bytes());
        hasher.update(&self.kind.as_u16().to_be_bytes());
        hasher.update(&self.actor_pubkey);
        hasher.update(&self.payload_hash);
        hasher.update(&self.prev_root);
        hasher.update(&self.signature);
        *hasher.finalize().as_bytes()
    }

    /// § genesis-sentinel · seq_no=0 · all-zeros-elsewhere · NEVER actually appended.
    /// §§ used as "before-genesis" reference for prev_root chaining.
    #[must_use]
    pub const fn genesis_sentinel() -> Self {
        Self {
            seq_no: 0,
            ts_unix: 0,
            kind: EntryKind::CheckpointMark,
            actor_pubkey: [0u8; 32],
            payload_hash: [0u8; 32],
            prev_root: [0u8; 32],
            signature: [0u8; 64],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_kind_round_trip() {
        let kinds = [
            EntryKind::CapGrant,
            EntryKind::CapRevoke,
            EntryKind::CellEmission,
            EntryKind::AttestationAnchor,
            EntryKind::CheckpointMark,
            EntryKind::HotfixBundle,
            EntryKind::MyceliumPattern,
            EntryKind::KnowledgeIngest,
            EntryKind::FederationAnchor,
        ];
        for k in kinds {
            let n = k.as_u16();
            let back = EntryKind::from_u16(n);
            assert_eq!(k, back, "round-trip failed for {k:?}");
        }
        // unknown number → OpenExtension fallback
        assert_eq!(EntryKind::from_u16(0xABCD), EntryKind::OpenExtension);
    }

    #[test]
    fn canonical_bytes_deterministic() {
        let e = LedgerEntry {
            seq_no: 42,
            ts_unix: 1_700_000_000,
            kind: EntryKind::CapGrant,
            actor_pubkey: [7u8; 32],
            payload_hash: [9u8; 32],
            prev_root: [3u8; 32],
            signature: [0u8; 64],
        };
        let a = e.canonical_bytes_for_sign();
        let b = e.canonical_bytes_for_sign();
        assert_eq!(a, b);
        // expected length : domain + 8+8+2+32+32+32 = domain + 114
        assert_eq!(a.len(), ENTRY_DOMAIN.len() + 114);
    }

    #[test]
    fn leaf_hash_changes_on_tamper() {
        let mut e = LedgerEntry::genesis_sentinel();
        e.seq_no = 1;
        let h1 = e.leaf_hash();
        e.payload_hash[0] ^= 0xFF;
        let h2 = e.leaf_hash();
        assert_ne!(h1, h2, "tamper-detection failed");
    }
}
