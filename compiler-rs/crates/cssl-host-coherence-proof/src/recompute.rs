// § recompute.rs · deterministic recompute-event-effect
// ══════════════════════════════════════════════════════════════════════════════
// § I> Given (state · lineage · event-with-sig) :
//   1. canonicalize event-bytes  : id ‖ ts.le_bytes ‖ parent_id-or-zero ‖ payload_blake3
//   2. Ed25519-verify(pubkey, sig, canonical-bytes)
//   3. simulate-effect-on-state  : extend lineage with this event
//   4. recompute merkle-root      : merkle_root_blake3(payload-hashes)
//   5. return new StateSnapshot OR VerificationOutcome variant
// § I> CANONICAL-BYTES rule MUST match cssl-host-sigma-chain (W8-C1) — assumption
//   documented HERE for integration-time audit.
// ══════════════════════════════════════════════════════════════════════════════
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

use crate::event::{EventId, SigmaEventLike};
use crate::lineage::Lineage;
use crate::merkle::merkle_root_blake3;
use crate::state::{StateSnapshot, StateSnapshotLike};

/// Errors during recompute.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RecomputeError {
    /// Ed25519 pubkey rejected by the dalek decoder (off-curve, etc.).
    #[error("invalid ed25519 verifying-key bytes")]
    InvalidPubKey,
    /// Signature verification failed — possible tampering.
    #[error("ed25519 signature verification failed")]
    SignatureVerify,
}

/// Outcome of validating a single event against a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationOutcome {
    /// Event verified ; new snapshot returned.
    Verified(StateSnapshot),
    /// Disagreement detected at this event — see id + roots.
    DisagreedAt {
        event_id: EventId,
        expected: [u8; 32],
        actual: [u8; 32],
    },
    /// Signature failure → tampering detected.
    TamperDetected {
        event_id: EventId,
        reason: String,
    },
}

/// Build canonical-bytes for an event (the bytes we sign over).
///
/// Layout: `id (32) ‖ ts (8 LE) ‖ parent_id_or_zero (32) ‖ payload_blake3 (32)`.
/// Total: 104 bytes.
fn canonical_bytes<E: SigmaEventLike>(ev: &E) -> [u8; 104] {
    let mut buf = [0u8; 104];
    buf[0..32].copy_from_slice(&ev.id());
    buf[32..40].copy_from_slice(&ev.ts().to_le_bytes());
    let parent = ev.parent_id().unwrap_or([0u8; 32]);
    buf[40..72].copy_from_slice(&parent);
    buf[72..104].copy_from_slice(&ev.payload_blake3());
    buf
}

/// Recompute the effect of one new event on top of a state-snapshot,
/// given the existing lineage. Returns the new snapshot OR an outcome variant
/// indicating disagreement / tampering.
///
/// `expected_root_after`, when supplied, lets the caller compare the
/// validator's claimed post-event-root against the deterministically-computed
/// one. `None` skips the comparison and just returns `Verified(new_snapshot)`.
pub fn recompute_event_effect<E: SigmaEventLike + Clone>(
    state: &StateSnapshot,
    lineage: &Lineage<E>,
    new_event: &E,
    expected_root_after: Option<[u8; 32]>,
) -> Result<VerificationOutcome, RecomputeError> {
    // Step 1. Verify signature.
    let pk = VerifyingKey::from_bytes(&new_event.emitter_pubkey())
        .map_err(|_| RecomputeError::InvalidPubKey)?;
    let sig = Signature::from_bytes(&new_event.sig());
    let bytes = canonical_bytes(new_event);
    if pk.verify(&bytes, &sig).is_err() {
        return Ok(VerificationOutcome::TamperDetected {
            event_id: new_event.id(),
            reason: "ed25519 signature verification failed".into(),
        });
    }

    // Step 2. Build new merkle-leaf-set : existing payload-hashes + new event's.
    let mut leaves: Vec<[u8; 32]> = lineage
        .events()
        .iter()
        .map(SigmaEventLike::payload_blake3)
        .collect();
    leaves.push(new_event.payload_blake3());
    let new_root = merkle_root_blake3(&leaves);
    let new_count = state.event_count().saturating_add(1);

    // Step 3. Optional disagreement check.
    if let Some(expected) = expected_root_after {
        if expected != new_root {
            return Ok(VerificationOutcome::DisagreedAt {
                event_id: new_event.id(),
                expected,
                actual: new_root,
            });
        }
    }

    Ok(VerificationOutcome::Verified(StateSnapshot::new(
        new_root, new_count,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MockSigmaEvent;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn signed_event(seed: u8, ts: u64, parent: Option<EventId>) -> (MockSigmaEvent, SigningKey) {
        let mut csprng = OsRng;
        let sk = SigningKey::generate(&mut csprng);
        let pk = sk.verifying_key().to_bytes();

        let mut id = [0u8; 32];
        let mut payload = [0u8; 32];
        for i in 0..32 {
            id[i] = seed.wrapping_add(i as u8);
            payload[i] = seed.wrapping_mul(3).wrapping_add(i as u8);
        }

        // Build canonical-bytes manually to match canonical_bytes layout.
        let mut buf = [0u8; 104];
        buf[0..32].copy_from_slice(&id);
        buf[32..40].copy_from_slice(&ts.to_le_bytes());
        let parent_bytes = parent.unwrap_or([0u8; 32]);
        buf[40..72].copy_from_slice(&parent_bytes);
        buf[72..104].copy_from_slice(&payload);

        let sig = sk.sign(&buf).to_bytes();

        (
            MockSigmaEvent::new(id, payload, ts, pk, sig, parent),
            sk,
        )
    }

    #[test]
    fn recompute_happy_path_genesis() {
        let (ev, _sk) = signed_event(0x10, 1, None);
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        let outcome = recompute_event_effect(&state, &lineage, &ev, None).unwrap();
        match outcome {
            VerificationOutcome::Verified(s) => {
                assert_eq!(s.event_count(), 1);
                assert_ne!(s.merkle_root(), [0u8; 32]);
            }
            _ => panic!("expected Verified"),
        }
    }

    #[test]
    fn recompute_happy_path_chained() {
        let (ev1, _sk1) = signed_event(0x10, 1, None);
        let ev1_id = ev1.id;
        let (ev2, _sk2) = signed_event(0x20, 2, Some(ev1_id));
        let state = StateSnapshot::empty();
        let lineage = Lineage::from_unsorted(vec![ev1]).unwrap();
        let outcome = recompute_event_effect(&state, &lineage, &ev2, None).unwrap();
        match outcome {
            VerificationOutcome::Verified(s) => assert_eq!(s.event_count(), 1),
            _ => panic!("expected Verified"),
        }
    }

    #[test]
    fn recompute_returns_root_independent_of_state_count() {
        // The merkle-root is a function of the leaves not the prior state-count
        // ; this is a behavioural guarantee for tests in the consensus layer.
        let (ev, _sk) = signed_event(0x10, 1, None);
        let s_a = StateSnapshot::empty();
        let s_b = StateSnapshot::new([0u8; 32], 50);
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        let oa = recompute_event_effect(&s_a, &lineage, &ev, None).unwrap();
        let ob = recompute_event_effect(&s_b, &lineage, &ev, None).unwrap();
        match (oa, ob) {
            (VerificationOutcome::Verified(a), VerificationOutcome::Verified(b)) => {
                assert_eq!(a.merkle_root(), b.merkle_root());
                assert_eq!(a.event_count(), 1); // saturating from 0
                assert_eq!(b.event_count(), 51); // saturating from 50
            }
            _ => panic!("expected both Verified"),
        }
    }

    #[test]
    fn recompute_tamper_detected_on_bad_sig() {
        let (mut ev, _sk) = signed_event(0x10, 1, None);
        // Corrupt the signature.
        ev.sig[0] ^= 0xff;
        ev.sig[1] ^= 0x11;
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        let outcome = recompute_event_effect(&state, &lineage, &ev, None).unwrap();
        match outcome {
            VerificationOutcome::TamperDetected { event_id, .. } => {
                assert_eq!(event_id, ev.id);
            }
            _ => panic!("expected TamperDetected"),
        }
    }

    #[test]
    fn recompute_tamper_detected_on_payload_substitute() {
        let (mut ev, _sk) = signed_event(0x10, 1, None);
        // Substitute payload — sig was over the old payload, so verify fails.
        ev.payload_blake3 = [0xab; 32];
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        let outcome = recompute_event_effect(&state, &lineage, &ev, None).unwrap();
        assert!(matches!(outcome, VerificationOutcome::TamperDetected { .. }));
    }

    #[test]
    fn recompute_disagreement_when_expected_root_wrong() {
        let (ev, _sk) = signed_event(0x10, 1, None);
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        // Pass a deliberately-wrong expected-root.
        let bogus = [0xee; 32];
        let outcome = recompute_event_effect(&state, &lineage, &ev, Some(bogus)).unwrap();
        match outcome {
            VerificationOutcome::DisagreedAt {
                event_id, expected, actual,
            } => {
                assert_eq!(event_id, ev.id);
                assert_eq!(expected, bogus);
                assert_ne!(actual, bogus);
            }
            _ => panic!("expected DisagreedAt"),
        }
    }

    #[test]
    fn recompute_invalid_pubkey_errors() {
        let (mut ev, _sk) = signed_event(0x10, 1, None);
        // off-curve / invalid pubkey bytes ; ed25519-dalek's from_bytes rejects
        // any pubkey whose high-order byte makes it invalid. 0xff-pattern is
        // commonly rejected ; if it ever lands on-curve, swap for a known-bad.
        ev.emitter_pubkey = [0xff; 32];
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        // Either InvalidPubKey error OR a TamperDetected ; both are rejection.
        match recompute_event_effect(&state, &lineage, &ev, None) {
            Err(RecomputeError::InvalidPubKey)
            | Ok(VerificationOutcome::TamperDetected { .. }) => {}
            other => panic!("expected rejection, got {other:?}"),
        }
    }
}
