// § verify.rs — full verify-pipeline w/ caused-rejection reporting
// §§ steps :
//     1 recompute event-id from canonical-bytes (tamper-detect)
//     2 ed25519-verify(emitter_pubkey, sig, canonical-bytes)
//     3 if proof-given : merkle-path-verify
//     4 if lineage-given : deterministic-recompute-root must equal claimed-root

use crate::event::SigmaEvent;
use crate::ledger::{CoherenceProof, SigmaLedger};
use crate::merkle::{merkle_path_verify, Digest};
use crate::sign::{recompute_event_id, verify_signature, SignatureError};

/// Outcome of a verify-call : Verified, or Rejected with cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// All checks passed.
    Verified,
    /// Verification failed — see `cause`.
    Rejected(VerifyError),
}

/// Cause of a verify-rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// `event.id` does not equal recompute(canonical-bytes).
    IdTampered,
    /// `emitter_pubkey` is not a valid Ed25519 public-key.
    InvalidPubkey,
    /// Ed25519 signature did not validate.
    SignatureInvalid,
    /// Merkle-path supplied did not reconstruct the claimed root.
    MerklePathInvalid,
    /// Deterministic-recompute over (seed,lineage) did not equal the claimed root.
    RecomputeMismatch,
}

impl core::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VerifyError::IdTampered => f.write_str("event id mismatch (tamper)"),
            VerifyError::InvalidPubkey => f.write_str("emitter_pubkey malformed"),
            VerifyError::SignatureInvalid => f.write_str("ed25519 signature invalid"),
            VerifyError::MerklePathInvalid => f.write_str("merkle path did not reproduce root"),
            VerifyError::RecomputeMismatch => {
                f.write_str("deterministic recompute did not match claimed root")
            }
        }
    }
}

impl std::error::Error for VerifyError {}

/// Verify a single event's id-integrity + signature.
///
/// Returns `Verified` on success ; otherwise `Rejected(cause)`.
#[must_use]
pub fn verify_event(event: &SigmaEvent) -> VerifyOutcome {
    if recompute_event_id(event) != event.id {
        return VerifyOutcome::Rejected(VerifyError::IdTampered);
    }
    match verify_signature(event) {
        Ok(()) => VerifyOutcome::Verified,
        Err(SignatureError::InvalidPubkey) => {
            VerifyOutcome::Rejected(VerifyError::InvalidPubkey)
        }
        Err(SignatureError::Ed25519) => VerifyOutcome::Rejected(VerifyError::SignatureInvalid),
    }
}

/// Verify a full Coherence-Proof : event-sig + merkle-path against claimed-root.
#[must_use]
pub fn verify_coherence_proof(proof: &CoherenceProof) -> VerifyOutcome {
    match verify_event(&proof.event) {
        VerifyOutcome::Verified => {}
        rej => return rej,
    }
    if !merkle_path_verify(&proof.event.id, &proof.merkle_path, &proof.merkle_root) {
        return VerifyOutcome::Rejected(VerifyError::MerklePathInvalid);
    }
    VerifyOutcome::Verified
}

/// Verify a Coherence-Proof + cross-check claimed-root against a deterministic-recompute
/// over `(seed_ids , lineage_events)` — the spec/14 § COHERENCE-PROOF step 4 contract.
#[must_use]
pub fn verify_coherence_proof_with_lineage(
    proof: &CoherenceProof,
    seed_ids: &[crate::event::EventId],
    lineage_events: &[SigmaEvent],
) -> VerifyOutcome {
    match verify_coherence_proof(proof) {
        VerifyOutcome::Verified => {}
        rej => return rej,
    }
    let recomputed: Digest =
        SigmaLedger::deterministic_recompute_root(seed_ids, lineage_events);
    if recomputed != proof.merkle_root {
        return VerifyOutcome::Rejected(VerifyError::RecomputeMismatch);
    }
    VerifyOutcome::Verified
}
