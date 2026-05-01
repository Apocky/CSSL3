// § consensus.rs · 2-validator agreement · tie-break Ed25519-hex-asc
// ══════════════════════════════════════════════════════════════════════════════
// § I> 2 validators each compute new-merkle-root for the same event ;
//   if equal → Verified ; else → DisagreementFlag → AuditEmitter ;
//   tie-break via Ed25519-sig hex-asc lexicographic-low-wins.
// § I> Tampering (sig-fail) by EITHER validator → TamperDetected (¬ tie-break)
// § I> Pure-validator pubkeys are independent of event-emitter ; validator
//   identifies itself via its OWN pubkey + signs its computed-root claim.
// ══════════════════════════════════════════════════════════════════════════════
use serde::{Deserialize, Serialize};

use crate::audit::{AuditEmitter, AuditEvent};
use crate::disagreement::{DisagreementFlag, DisagreementReason};
use crate::event::{EventId, PubKey, SigBytes, SigmaEventLike};
use crate::lineage::Lineage;
use crate::merkle::MerkleRoot;
use crate::recompute::{recompute_event_effect, VerificationOutcome};
use crate::state::{StateSnapshot, StateSnapshotLike};
use crate::tiebreak::ed25519_hex_asc_winner;

/// One validator's claim about the post-event state.
///
/// The 64-byte `validator_sig` field uses the crate-local `sig_serde`
/// module since serde does not auto-impl `Deserialize` for `[u8; 64]`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidatorView {
    /// Validator-identifying pubkey (NOT the event-emitter pubkey).
    pub validator_pubkey: PubKey,
    /// Validator's signature over their claimed-root (used as tie-break key).
    #[serde(with = "crate::sig_serde")]
    pub validator_sig: SigBytes,
    /// Validator's claimed merkle-root after applying the event.
    pub claimed_root: MerkleRoot,
}

/// The result of a 2-validator consensus-call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusReport {
    /// Both validators agreed.
    Verified {
        snapshot: StateSnapshot,
    },
    /// Validators disagreed ; tie-broken by Ed25519-hex-asc.
    /// `winner_index` is `0` or `1` ; the winning side's claim is canonical.
    Disagreed {
        flag: DisagreementFlag,
        winner_index: usize,
        winner_root: MerkleRoot,
        winner_snapshot: StateSnapshot,
    },
    /// One or both validators detected tampering ; consensus rejected.
    TamperDetected {
        event_id: EventId,
        reason: String,
    },
}

/// 2-validator consensus engine.
///
/// Holds the audit-emitter that disagreement-flags + tamper-detections fire on.
pub struct ConsensusValidator<A: AuditEmitter> {
    audit: A,
}

impl<A: AuditEmitter> ConsensusValidator<A> {
    /// Construct with an explicit audit-emitter.
    pub fn new(audit: A) -> Self {
        Self { audit }
    }

    /// Borrow the audit-emitter (for tests / inspection).
    pub fn audit(&self) -> &A {
        &self.audit
    }

    /// Run consensus over a single new-event with two validator-views.
    ///
    /// On disagreement, emits a `DisagreementFlag` to the audit-emitter and
    /// tie-breaks with `ed25519_hex_asc_winner` — the lexicographically-lowest
    /// validator-signature wins.
    pub fn run<E: SigmaEventLike + Clone>(
        &self,
        state: &StateSnapshot,
        lineage: &Lineage<E>,
        new_event: &E,
        view_a: &ValidatorView,
        view_b: &ValidatorView,
    ) -> ConsensusReport {
        // Validator-A's claim, deterministically recomputed (and compared).
        let outcome_a = match recompute_event_effect(
            state,
            lineage,
            new_event,
            Some(view_a.claimed_root),
        ) {
            Ok(o) => o,
            Err(e) => {
                let msg = format!("recompute-error : {e}");
                self.audit.emit(AuditEvent::TamperDetected {
                    event_id: new_event.id(),
                    emitter_pubkey: new_event.emitter_pubkey(),
                    reason: msg.clone(),
                });
                return ConsensusReport::TamperDetected {
                    event_id: new_event.id(),
                    reason: msg,
                };
            }
        };

        // Tampering by event-emitter is a hard-rejection (no tie-break possible).
        if let VerificationOutcome::TamperDetected { event_id, reason } = &outcome_a {
            self.audit.emit(AuditEvent::TamperDetected {
                event_id: *event_id,
                emitter_pubkey: new_event.emitter_pubkey(),
                reason: reason.clone(),
            });
            return ConsensusReport::TamperDetected {
                event_id: *event_id,
                reason: reason.clone(),
            };
        }

        // Validator-A : either Verified(snapshot) or DisagreedAt(actual_root)
        let (a_root, a_snapshot) = match &outcome_a {
            VerificationOutcome::Verified(s) => (s.merkle_root(), *s),
            VerificationOutcome::DisagreedAt { actual, .. } => {
                let snapshot = StateSnapshot::new(*actual, state.event_count() + 1);
                (*actual, snapshot)
            }
            VerificationOutcome::TamperDetected { .. } => unreachable!(),
        };
        let a_agreed = a_root == view_a.claimed_root;

        // Validator-B : independent recompute against same state+lineage.
        let outcome_b =
            match recompute_event_effect(state, lineage, new_event, Some(view_b.claimed_root)) {
                Ok(o) => o,
                Err(e) => {
                    let msg = format!("recompute-error : {e}");
                    self.audit.emit(AuditEvent::TamperDetected {
                        event_id: new_event.id(),
                        emitter_pubkey: new_event.emitter_pubkey(),
                        reason: msg.clone(),
                    });
                    return ConsensusReport::TamperDetected {
                        event_id: new_event.id(),
                        reason: msg,
                    };
                }
            };
        if let VerificationOutcome::TamperDetected { event_id, reason } = outcome_b {
            self.audit.emit(AuditEvent::TamperDetected {
                event_id,
                emitter_pubkey: new_event.emitter_pubkey(),
                reason: reason.clone(),
            });
            return ConsensusReport::TamperDetected { event_id, reason };
        }
        let (b_root, b_snapshot) = match &outcome_b {
            VerificationOutcome::Verified(s) => (s.merkle_root(), *s),
            VerificationOutcome::DisagreedAt { actual, .. } => {
                let snapshot = StateSnapshot::new(*actual, state.event_count() + 1);
                (*actual, snapshot)
            }
            VerificationOutcome::TamperDetected { .. } => unreachable!(),
        };
        let b_agreed = b_root == view_b.claimed_root;

        // BOTH validators agree internally AND with each other → Verified.
        if a_agreed && b_agreed && view_a.claimed_root == view_b.claimed_root {
            return ConsensusReport::Verified {
                snapshot: a_snapshot,
            };
        }

        // Disagreement path — flag + tie-break.
        let winner_index =
            ed25519_hex_asc_winner(&view_a.validator_sig, &view_b.validator_sig);
        let (flagger_pubkey, expected_root, actual_root, winner_root, winner_snapshot) =
            if winner_index == 0 {
                (
                    view_a.validator_pubkey,
                    view_b.claimed_root,
                    view_a.claimed_root,
                    view_a.claimed_root,
                    a_snapshot,
                )
            } else {
                (
                    view_b.validator_pubkey,
                    view_a.claimed_root,
                    view_b.claimed_root,
                    view_b.claimed_root,
                    b_snapshot,
                )
            };
        let flag = DisagreementFlag {
            event_id: new_event.id(),
            expected_root,
            actual_root,
            flagger_pubkey,
            reason: DisagreementReason::MerkleMismatch,
        };
        self.audit
            .emit(AuditEvent::DisagreementFlagged(flag.clone()));
        ConsensusReport::Disagreed {
            flag,
            winner_index,
            winner_root,
            winner_snapshot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::VecAuditEmitter;
    use crate::event::MockSigmaEvent;
    use crate::merkle::merkle_root_blake3;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn signed_mock(seed: u8, ts: u64, parent: Option<EventId>) -> (MockSigmaEvent, SigningKey) {
        let mut csprng = OsRng;
        let sk = SigningKey::generate(&mut csprng);
        let pk = sk.verifying_key().to_bytes();

        let mut id = [0u8; 32];
        let mut payload = [0u8; 32];
        for i in 0..32 {
            id[i] = seed.wrapping_add(i as u8);
            payload[i] = seed.wrapping_mul(3).wrapping_add(i as u8);
        }
        let mut buf = [0u8; 104];
        buf[0..32].copy_from_slice(&id);
        buf[32..40].copy_from_slice(&ts.to_le_bytes());
        buf[40..72].copy_from_slice(&parent.unwrap_or([0u8; 32]));
        buf[72..104].copy_from_slice(&payload);
        let sig = sk.sign(&buf).to_bytes();
        (
            MockSigmaEvent::new(id, payload, ts, pk, sig, parent),
            sk,
        )
    }

    fn make_view(seed: u8, claimed_root: MerkleRoot) -> ValidatorView {
        let mut sig = [0u8; 64];
        for (i, b) in sig.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        let mut pk = [0u8; 32];
        for (i, b) in pk.iter_mut().enumerate() {
            *b = seed.wrapping_mul(2).wrapping_add(i as u8);
        }
        ValidatorView {
            validator_pubkey: pk,
            validator_sig: sig,
            claimed_root,
        }
    }

    #[test]
    fn consensus_verified_when_both_agree() {
        let (ev, _sk) = signed_mock(0x10, 1, None);
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        // Compute the truth-root.
        let leaves = vec![ev.payload_blake3];
        let truth = merkle_root_blake3(&leaves);

        let va = make_view(0x01, truth);
        let vb = make_view(0xfe, truth);

        let validator = ConsensusValidator::new(VecAuditEmitter::new());
        let report = validator.run(&state, &lineage, &ev, &va, &vb);
        match report {
            ConsensusReport::Verified { snapshot } => {
                assert_eq!(snapshot.merkle_root(), truth);
                assert_eq!(snapshot.event_count(), 1);
            }
            r => panic!("expected Verified, got {r:?}"),
        }
        assert!(validator.audit().is_empty());
    }

    #[test]
    fn consensus_disagree_emits_flag_and_tie_breaks_low_sig_wins() {
        let (ev, _sk) = signed_mock(0x10, 1, None);
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();

        // A claims a bogus root ; B claims another bogus root.
        let bogus_a = [0x11; 32];
        let bogus_b = [0x22; 32];
        // A's sig starts with 0x01 (low) ; B's starts with 0xfe (high).
        let va = make_view(0x01, bogus_a);
        let vb = make_view(0xfe, bogus_b);

        let validator = ConsensusValidator::new(VecAuditEmitter::new());
        let report = validator.run(&state, &lineage, &ev, &va, &vb);

        match report {
            ConsensusReport::Disagreed {
                flag,
                winner_index,
                winner_root,
                ..
            } => {
                // A has the lower sig → wins.
                assert_eq!(winner_index, 0);
                assert_eq!(winner_root, bogus_a);
                assert_eq!(flag.flagger_pubkey, va.validator_pubkey);
                assert_eq!(flag.reason, DisagreementReason::MerkleMismatch);
            }
            r => panic!("expected Disagreed, got {r:?}"),
        }
        // Exactly one disagreement-flag emitted.
        let snap = validator.audit().snapshot();
        assert_eq!(snap.len(), 1);
        match &snap[0] {
            AuditEvent::DisagreementFlagged(_) => {}
            other => panic!("expected DisagreementFlagged, got {other:?}"),
        }
    }

    #[test]
    fn consensus_disagree_swap_sigs_winner_swaps() {
        let (ev, _sk) = signed_mock(0x10, 1, None);
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        let bogus_a = [0x11; 32];
        let bogus_b = [0x22; 32];
        // Reverse : A's sig is HIGH ; B's sig is LOW.
        let va = make_view(0xfe, bogus_a);
        let vb = make_view(0x01, bogus_b);

        let validator = ConsensusValidator::new(VecAuditEmitter::new());
        let report = validator.run(&state, &lineage, &ev, &va, &vb);
        match report {
            ConsensusReport::Disagreed {
                winner_index,
                winner_root,
                ..
            } => {
                assert_eq!(winner_index, 1);
                assert_eq!(winner_root, bogus_b);
            }
            r => panic!("expected Disagreed, got {r:?}"),
        }
    }

    #[test]
    fn consensus_tamper_detected_short_circuits_to_tamper() {
        let (mut ev, _sk) = signed_mock(0x10, 1, None);
        ev.sig[0] ^= 0xff; // corrupt the EVENT-emitter sig.
        let state = StateSnapshot::empty();
        let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
        // Both validators claim some root ; doesn't matter — tamper short-circuits.
        let va = make_view(0x01, [0u8; 32]);
        let vb = make_view(0xfe, [0u8; 32]);
        let validator = ConsensusValidator::new(VecAuditEmitter::new());
        let report = validator.run(&state, &lineage, &ev, &va, &vb);
        match report {
            ConsensusReport::TamperDetected { event_id, .. } => {
                assert_eq!(event_id, ev.id);
            }
            r => panic!("expected TamperDetected, got {r:?}"),
        }
        // Audit-emit fired exactly once with TamperDetected.
        let snap = validator.audit().snapshot();
        assert_eq!(snap.len(), 1);
        match &snap[0] {
            AuditEvent::TamperDetected { .. } => {}
            o => panic!("expected TamperDetected audit, got {o:?}"),
        }
    }
}
