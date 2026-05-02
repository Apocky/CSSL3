//! § anchor — Σ-Chain anchor of the playtest-completion + report-hash.
//!
//! § ROLE
//!   The author cannot fake-passing : the report is canonical-bytes-
//!   serialized, BLAKE3-hashed, then Ed25519-signed by the playtest-
//!   coordinator's keypair. The resulting [`PlayTestAnchor`] is what
//!   feeds Σ-Chain. Verifying the anchor reproduces the hash + verifies
//!   the signature ; the host can then trust the report's integrity.
//!
//! § PRIME-DIRECTIVE
//!   ¬ surveillance — the anchor carries ONLY hashes + scores + the
//!   coordinator's pubkey. No scene-bytes ; no creator-pubkey ; no
//!   content body. Aggregate-mode by-default per spec.
//!
//! § COORDINATOR-KEYPAIR
//!   The keypair lives in the host's keystore (same one Σ-Chain uses).
//!   Tests construct an ephemeral keypair via `rand::rngs::OsRng`.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::report::PlayTestReport;
use crate::PROTOCOL_VERSION;

/// § The anchor that gets appended to Σ-Chain. `serde`-stable so the
/// host can persist + re-emit it. The whole struct ≤ 128 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayTestAnchor {
    /// Wire-format protocol-version (mirrors the source report).
    pub protocol_version: u32,
    /// Echoed for fast-lookup (matches `report.content_id`).
    pub content_id: u32,
    /// Echoed for fast-lookup (matches `report.agent_persona_seed`).
    pub agent_persona_seed: u64,
    /// BLAKE3 hash of the canonical-JSON report ; 32 bytes.
    pub report_hash: [u8; 32],
    /// Coordinator's ed25519 public key ; 32 bytes.
    pub coordinator_pubkey: [u8; 32],
    /// Detached ed25519 signature over the preimage — first 32 bytes.
    /// Split in two because serde's array-`Deserialize` derives are
    /// limited to length ≤ 32 ; combined with [`Self::signature_lo`]
    /// this gives the full 64-byte signature without pulling
    /// `serde-arrays` into the workspace.
    pub signature_hi: [u8; 32],
    /// Detached ed25519 signature over the preimage — last 32 bytes.
    pub signature_lo: [u8; 32],
    /// Aggregate score echoed for fast-rank (Σ-Chain TIER-2 row).
    pub total_score: u8,
    /// Safety score echoed for fast no-tolerance-rank.
    pub safety_score: u8,
}

/// § Errors raised during anchor construction or verification.
#[derive(Debug, Error)]
pub enum AnchorError {
    /// Canonical-JSON serialization failed.
    #[error("serialize failed: {0}")]
    Serialize(String),
    /// Signature verification failed.
    #[error("signature verify failed")]
    BadSignature,
    /// Hash recomputation did not match the anchor's recorded hash.
    #[error("report-hash mismatch")]
    HashMismatch,
}

/// § Compute the canonical preimage bytes for the anchor's signature.
///
/// § FORMAT (≤ 64 bytes)
///   `b"cssl-playtest-anchor-v1\0" || protocol_version_le || content_id_le ||
///    seed_le || report_hash`
///
///   The version-prefix prevents cross-protocol replays ; the
///   `report_hash` is the only piece that varies per-test. We sign the
///   compact preimage rather than the JSON body so signature cost is
///   constant-bounded.
fn anchor_preimage(
    protocol_version: u32,
    content_id: u32,
    seed: u64,
    report_hash: &[u8; 32],
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(24 + 32);
    buf.extend_from_slice(b"cssl-playtest-anchor-v1\0");
    buf.extend_from_slice(&protocol_version.to_le_bytes());
    buf.extend_from_slice(&content_id.to_le_bytes());
    buf.extend_from_slice(&seed.to_le_bytes());
    buf.extend_from_slice(report_hash);
    buf
}

/// § Build the Σ-Chain anchor for a finished report.
///
/// § STEPS
///   1. Canonical-JSON-serialize the report.
///   2. BLAKE3 the JSON bytes → `report_hash`.
///   3. Build the preimage (see [`anchor_preimage`]).
///   4. Ed25519-sign the preimage with the coordinator's keypair.
///   5. Assemble the [`PlayTestAnchor`].
///
/// § DETERMINISM
///   Steps 1–3 are deterministic given the same inputs. Step 4 is
///   deterministic for ed25519-dalek (no nonce-randomness in this curve
///   variant). The whole pipeline therefore yields equal anchors for
///   equal inputs — needed for the Σ-Chain replay-equality property.
pub fn anchor_report(
    report: &PlayTestReport,
    coordinator_signing_key: &SigningKey,
) -> Result<PlayTestAnchor, AnchorError> {
    let json = serde_json::to_vec(report).map_err(|e| AnchorError::Serialize(e.to_string()))?;
    let mut report_hash = [0_u8; 32];
    report_hash.copy_from_slice(blake3::hash(&json).as_bytes());

    let preimage = anchor_preimage(
        report.protocol_version,
        report.content_id,
        report.agent_persona_seed,
        &report_hash,
    );
    let sig: Signature = coordinator_signing_key.sign(&preimage);

    let pubkey = coordinator_signing_key.verifying_key().to_bytes();
    let sig_bytes = sig.to_bytes();
    let mut hi = [0_u8; 32];
    let mut lo = [0_u8; 32];
    hi.copy_from_slice(&sig_bytes[..32]);
    lo.copy_from_slice(&sig_bytes[32..]);

    Ok(PlayTestAnchor {
        protocol_version: PROTOCOL_VERSION,
        content_id: report.content_id,
        agent_persona_seed: report.agent_persona_seed,
        report_hash,
        coordinator_pubkey: pubkey,
        signature_hi: hi,
        signature_lo: lo,
        total_score: report.total,
        safety_score: report.safety.0,
    })
}

/// § Verify an anchor against the supplied report. Used by Σ-Chain peers
/// to confirm the report-bytes match the anchor's hash + the signature
/// is valid under the recorded coordinator-pubkey.
pub fn verify_anchor(report: &PlayTestReport, anchor: &PlayTestAnchor) -> Result<(), AnchorError> {
    let json = serde_json::to_vec(report).map_err(|e| AnchorError::Serialize(e.to_string()))?;
    let recomputed = blake3::hash(&json);
    if recomputed.as_bytes() != &anchor.report_hash {
        return Err(AnchorError::HashMismatch);
    }
    let pubkey = VerifyingKey::from_bytes(&anchor.coordinator_pubkey)
        .map_err(|_| AnchorError::BadSignature)?;
    let mut sig_bytes = [0_u8; 64];
    sig_bytes[..32].copy_from_slice(&anchor.signature_hi);
    sig_bytes[32..].copy_from_slice(&anchor.signature_lo);
    let sig = Signature::from_bytes(&sig_bytes);
    let preimage = anchor_preimage(
        anchor.protocol_version,
        anchor.content_id,
        anchor.agent_persona_seed,
        &anchor.report_hash,
    );
    pubkey
        .verify(&preimage, &sig)
        .map_err(|_| AnchorError::BadSignature)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Suggestion;
    use crate::scoring::{Score, Thresholds};
    use rand::rngs::OsRng;

    fn sample_report() -> PlayTestReport {
        PlayTestReport::assemble(
            7,
            42,
            0,
            0,
            true,
            true,
            Score(80),
            Score(70),
            Score(100),
            Score(90),
            Thresholds::default(),
            vec![Suggestion::new("§ ALL ✓")],
        )
    }

    #[test]
    fn anchor_round_trip_verifies() {
        let key = SigningKey::generate(&mut OsRng);
        let r = sample_report();
        let a = anchor_report(&r, &key).unwrap();
        assert!(verify_anchor(&r, &a).is_ok());
        assert_eq!(a.total_score, r.total);
        assert_eq!(a.safety_score, r.safety.0);
    }

    #[test]
    fn tampered_report_fails_verify() {
        let key = SigningKey::generate(&mut OsRng);
        let r = sample_report();
        let a = anchor_report(&r, &key).unwrap();

        let mut tampered = r;
        tampered.total = 99; // mutate
        let res = verify_anchor(&tampered, &a);
        assert!(matches!(res, Err(AnchorError::HashMismatch)));
    }

    #[test]
    fn anchor_is_deterministic_for_equal_inputs() {
        let key = SigningKey::generate(&mut OsRng);
        let r = sample_report();
        let a1 = anchor_report(&r, &key).unwrap();
        let a2 = anchor_report(&r, &key).unwrap();
        // ed25519-dalek is deterministic ; same key + preimage → equal sig.
        assert_eq!(a1, a2);
    }
}
