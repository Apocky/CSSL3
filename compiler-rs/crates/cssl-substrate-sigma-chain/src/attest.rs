// § T11-W11-SIGMA-CHAIN-ATTEST : high-level "record_attestation()" API for sibling-crates
// §§ purpose : siblings (cssl-hotfix-stream, cssl-substrate-prime-directive,
//     cssl-mycelium-chat-sync, cssl-substrate-knowledge) can call record_attestation()
//     without knowing about Merkle / seq_no / Ed25519 internals.
// §§ key-management : the SigningKey is owned by the caller ; this fn signs on demand.

#![forbid(unsafe_code)]

use ed25519_dalek::{Signer, SigningKey};

use crate::chain::{AppendResult, ChainError, SigmaChain};
use crate::entry::{EntryKind, LedgerEntry};

/// § convenience-shape : payload-blob + kind · sibling-crates pass these.
#[derive(Clone, Debug)]
pub struct Attestation<'a> {
    pub kind: EntryKind,
    pub payload: &'a [u8],
    /// unix-epoch-seconds · caller supplies ; tests can pass deterministic value
    pub ts_unix: u64,
}

impl<'a> Attestation<'a> {
    /// Convenience constructor.
    #[must_use]
    pub fn new(kind: EntryKind, payload: &'a [u8], ts_unix: u64) -> Self {
        Self {
            kind,
            payload,
            ts_unix,
        }
    }
}

/// § hash payload via BLAKE3 · domain-separated.
#[must_use]
pub fn hash_payload(payload: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"cssl-substrate-sigma-chain/v0/payload");
    hasher.update(payload);
    *hasher.finalize().as_bytes()
}

/// § record an attestation onto the chain · concurrent-safe · auto-fills seq + prev_root.
///
/// § returns AppendResult on success ; ChainError on validation failure (rare).
/// §§ siblings will typically call this from arbitrary threads ; safe to do so.
pub fn record_attestation(
    chain: &SigmaChain,
    signer: &SigningKey,
    att: &Attestation<'_>,
) -> Result<AppendResult, ChainError> {
    let payload_hash = hash_payload(att.payload);
    let actor_pubkey = signer.verifying_key().to_bytes();
    let kind = att.kind;
    let ts_unix = att.ts_unix;

    chain.try_append(move |seq_no, prev_root| {
        let mut entry = LedgerEntry {
            seq_no,
            ts_unix,
            kind,
            actor_pubkey,
            payload_hash,
            prev_root,
            signature: [0u8; 64],
        };
        let msg = entry.canonical_bytes_for_sign();
        let sig = signer.sign(&msg);
        entry.signature = sig.to_bytes();
        entry
    })
}

/// § convenience for hotfix-bundle siblings · pre-fills kind=HotfixBundle.
pub fn record_hotfix_bundle(
    chain: &SigmaChain,
    signer: &SigningKey,
    bundle_sig: &[u8],
    ts_unix: u64,
) -> Result<AppendResult, ChainError> {
    record_attestation(
        chain,
        signer,
        &Attestation::new(EntryKind::HotfixBundle, bundle_sig, ts_unix),
    )
}

/// § convenience for cap-grant siblings.
pub fn record_cap_grant(
    chain: &SigmaChain,
    signer: &SigningKey,
    cap_spec: &[u8],
    ts_unix: u64,
) -> Result<AppendResult, ChainError> {
    record_attestation(
        chain,
        signer,
        &Attestation::new(EntryKind::CapGrant, cap_spec, ts_unix),
    )
}

/// § convenience for cap-revoke siblings.
pub fn record_cap_revoke(
    chain: &SigmaChain,
    signer: &SigningKey,
    cap_spec: &[u8],
    ts_unix: u64,
) -> Result<AppendResult, ChainError> {
    record_attestation(
        chain,
        signer,
        &Attestation::new(EntryKind::CapRevoke, cap_spec, ts_unix),
    )
}

/// § convenience for cssl-substrate-omega-field cell-emission siblings.
pub fn record_cell_emission(
    chain: &SigmaChain,
    signer: &SigningKey,
    cell_bytes: &[u8],
    ts_unix: u64,
) -> Result<AppendResult, ChainError> {
    record_attestation(
        chain,
        signer,
        &Attestation::new(EntryKind::CellEmission, cell_bytes, ts_unix),
    )
}

/// § convenience for cssl-mycelium-chat-sync federated chat-pattern anchoring.
pub fn record_mycelium_pattern(
    chain: &SigmaChain,
    signer: &SigningKey,
    pattern_bytes: &[u8],
    ts_unix: u64,
) -> Result<AppendResult, ChainError> {
    record_attestation(
        chain,
        signer,
        &Attestation::new(EntryKind::MyceliumPattern, pattern_bytes, ts_unix),
    )
}

/// § convenience for cssl-substrate-knowledge ingestion.
pub fn record_knowledge_ingest(
    chain: &SigmaChain,
    signer: &SigningKey,
    knowledge_bytes: &[u8],
    ts_unix: u64,
) -> Result<AppendResult, ChainError> {
    record_attestation(
        chain,
        signer,
        &Attestation::new(EntryKind::KnowledgeIngest, knowledge_bytes, ts_unix),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn record_smoke() {
        let chain = SigmaChain::new();
        let signer = SigningKey::generate(&mut OsRng);
        let res = record_cap_grant(&chain, &signer, b"cap=read:fs:/home/user", 1_700_000_000)
            .expect("record");
        assert_eq!(res.seq_no, 1);
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn record_concurrent_smoke() {
        let chain = SigmaChain::new();
        let signer = SigningKey::generate(&mut OsRng);
        for i in 0..10 {
            record_attestation(
                &chain,
                &signer,
                &Attestation::new(EntryKind::CapGrant, &[i as u8], 1_700_000_000 + i),
            )
            .expect("record");
        }
        assert_eq!(chain.len(), 10);
        // seq_no monotonic gapless
        for (i, e) in chain.all_entries().iter().enumerate() {
            assert_eq!(e.seq_no, (i + 1) as u64);
        }
    }
}
