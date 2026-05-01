// § integration.rs · cssl-host-coherence-proof — end-to-end test surface
// ══════════════════════════════════════════════════════════════════════════════
// § I> covers PROMPT-required test-buckets that need cross-module fixtures :
//   ⊑ multi-event merkle-mismatch DisagreedAt (3)
//   ⊑ ServerTick monotonic enforced (extras)
//   ⊑ DisagreementFlag emitted (extras)
//   ⊑ lineage-sorting stable (extra)
//   ⊑ single-event lineage
//   ⊑ 100-event lineage
//   ⊑ empty-lineage edge
// ══════════════════════════════════════════════════════════════════════════════

use cssl_host_coherence_proof::{
    audit::{AuditEvent, VecAuditEmitter},
    consensus::{ConsensusReport, ConsensusValidator, ValidatorView},
    event::{EventId, MockSigmaEvent, SigmaEventLike},
    lineage::Lineage,
    merkle::merkle_root_blake3,
    recompute::{recompute_event_effect, VerificationOutcome},
    state::{StateSnapshot, StateSnapshotLike},
    tick::{ServerTick, TickStream},
};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;

fn signed_event(seed: u8, ts: u64, parent: Option<EventId>) -> MockSigmaEvent {
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
    MockSigmaEvent::new(id, payload, ts, pk, sig, parent)
}

fn make_view(seed: u8, claimed_root: [u8; 32]) -> ValidatorView {
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

// ══════════════════════════════════════════════════════════════════════════════
// § merkle-mismatch DisagreedAt — ≥ 3 cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn merkle_mismatch_disagreed_at_genesis() {
    let ev = signed_event(0x10, 1, None);
    let state = StateSnapshot::empty();
    let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
    let bogus = [0xee; 32];
    match recompute_event_effect(&state, &lineage, &ev, Some(bogus)).unwrap() {
        VerificationOutcome::DisagreedAt {
            event_id, expected, ..
        } => {
            assert_eq!(event_id, ev.id());
            assert_eq!(expected, bogus);
        }
        _ => panic!("expected DisagreedAt"),
    }
}

#[test]
fn merkle_mismatch_disagreed_at_chained() {
    let e1 = signed_event(0x11, 1, None);
    let e2 = signed_event(0x21, 2, Some(e1.id()));
    let lineage = Lineage::from_unsorted(vec![e1.clone()]).unwrap();
    let state = StateSnapshot::new(merkle_root_blake3(&[e1.payload_blake3()]), 1);
    let bogus = [0xab; 32];
    match recompute_event_effect(&state, &lineage, &e2, Some(bogus)).unwrap() {
        VerificationOutcome::DisagreedAt {
            event_id, actual, ..
        } => {
            assert_eq!(event_id, e2.id());
            assert_ne!(actual, bogus);
        }
        _ => panic!("expected DisagreedAt"),
    }
}

#[test]
fn merkle_mismatch_disagreed_at_three_event_chain() {
    let e1 = signed_event(0x12, 1, None);
    let e2 = signed_event(0x22, 2, Some(e1.id()));
    let e3 = signed_event(0x32, 3, Some(e2.id()));
    let lineage = Lineage::from_unsorted(vec![e1.clone(), e2.clone()]).unwrap();
    let leaves = vec![e1.payload_blake3(), e2.payload_blake3()];
    let state = StateSnapshot::new(merkle_root_blake3(&leaves), 2);
    let bogus = [0xcc; 32];
    match recompute_event_effect(&state, &lineage, &e3, Some(bogus)).unwrap() {
        VerificationOutcome::DisagreedAt {
            event_id, expected, actual,
        } => {
            assert_eq!(event_id, e3.id());
            assert_eq!(expected, bogus);
            assert_ne!(actual, bogus);
        }
        _ => panic!("expected DisagreedAt"),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// § DisagreementFlag emitted — extra coverage
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn disagreement_flag_emitted_carries_event_id_and_roots() {
    let ev = signed_event(0x10, 1, None);
    let state = StateSnapshot::empty();
    let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
    let va = make_view(0x01, [0x11; 32]);
    let vb = make_view(0xfe, [0x22; 32]);
    let validator = ConsensusValidator::new(VecAuditEmitter::new());
    let report = validator.run(&state, &lineage, &ev, &va, &vb);
    if let ConsensusReport::Disagreed { flag, .. } = report {
        assert_eq!(flag.event_id, ev.id());
        // Either side's root is one of the claimed-roots.
        assert!(flag.expected_root == [0x11; 32] || flag.expected_root == [0x22; 32]);
        assert!(flag.actual_root == [0x11; 32] || flag.actual_root == [0x22; 32]);
    } else {
        panic!("expected Disagreed");
    }
    assert_eq!(validator.audit().len(), 1);
}

// ══════════════════════════════════════════════════════════════════════════════
// § ServerTick monotonic-counter — extra
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn tick_stream_validates_event_within_latest_tick() {
    let mut s = TickStream::new();
    s.append(ServerTick::new(1, 100, 1000)).unwrap();
    s.append(ServerTick::new(2, 200, 2000)).unwrap();
    let tick = s.validate_ts(1500).unwrap();
    assert_eq!(tick.tick_id, 2);
}

// ══════════════════════════════════════════════════════════════════════════════
// § lineage-sorting stable — extra
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn lineage_sorting_is_idempotent() {
    let a = MockSigmaEvent::seeded(0x01, 5, None);
    let b = MockSigmaEvent::seeded(0x02, 5, None);
    let c = MockSigmaEvent::seeded(0x03, 5, None);
    let l1 = Lineage::from_unsorted(vec![c, a, b]).unwrap();
    let l2 = Lineage::from_unsorted(l1.events().to_vec()).unwrap();
    let ids1: Vec<_> = l1.events().iter().map(SigmaEventLike::id).collect();
    let ids2: Vec<_> = l2.events().iter().map(SigmaEventLike::id).collect();
    assert_eq!(ids1, ids2);
}

// ══════════════════════════════════════════════════════════════════════════════
// § single-event lineage
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn single_event_lineage_recompute() {
    let e1 = signed_event(0x10, 1, None);
    let state = StateSnapshot::empty();
    let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
    let truth = merkle_root_blake3(&[e1.payload_blake3()]);
    let outcome = recompute_event_effect(&state, &lineage, &e1, Some(truth)).unwrap();
    assert!(matches!(outcome, VerificationOutcome::Verified(s) if s.event_count() == 1));
}

// ══════════════════════════════════════════════════════════════════════════════
// § 100-event lineage — stress + determinism
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn hundred_event_lineage_recompute_deterministic() {
    // Build a 100-event chain.
    let mut events: Vec<MockSigmaEvent> = Vec::with_capacity(100);
    let mut parent: Option<EventId> = None;
    for i in 0..100u8 {
        // distinct seeds keep ids unique
        let ev = signed_event(i.wrapping_add(1), u64::from(i) + 1, parent);
        parent = Some(ev.id());
        events.push(ev);
    }

    // Lineage holds events 0..99 ; new event = events[99] applied on top of 0..98.
    let leaves_98: Vec<[u8; 32]> = events[..99]
        .iter()
        .map(SigmaEventLike::payload_blake3)
        .collect();
    let state_98 =
        StateSnapshot::new(merkle_root_blake3(&leaves_98), 99);
    let lineage_98 = Lineage::from_unsorted(events[..99].to_vec()).unwrap();
    let new_event = events[99].clone();

    let leaves_99: Vec<[u8; 32]> = events
        .iter()
        .map(SigmaEventLike::payload_blake3)
        .collect();
    let truth_99 = merkle_root_blake3(&leaves_99);

    let outcome =
        recompute_event_effect(&state_98, &lineage_98, &new_event, Some(truth_99)).unwrap();
    match outcome {
        VerificationOutcome::Verified(s) => {
            assert_eq!(s.event_count(), 100);
            assert_eq!(s.merkle_root(), truth_99);
        }
        _ => panic!("expected Verified for 100-event lineage"),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// § empty-lineage recompute → first-event Verified
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_lineage_first_event_verified() {
    let e1 = signed_event(0x10, 1, None);
    let state = StateSnapshot::empty();
    let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
    assert_eq!(lineage.len(), 0);
    let outcome = recompute_event_effect(&state, &lineage, &e1, None).unwrap();
    match outcome {
        VerificationOutcome::Verified(s) => {
            assert_eq!(s.event_count(), 1);
        }
        _ => panic!("expected Verified"),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// § audit-emit fires exactly-once on disagreement (¬ silent-skip)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn audit_emit_fires_on_consensus_disagreement_not_silent() {
    let ev = signed_event(0x10, 1, None);
    let state = StateSnapshot::empty();
    let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
    let va = make_view(0x01, [0x11; 32]);
    let vb = make_view(0xfe, [0x22; 32]);
    let validator = ConsensusValidator::new(VecAuditEmitter::new());
    let _ = validator.run(&state, &lineage, &ev, &va, &vb);
    let snap = validator.audit().snapshot();
    assert_eq!(snap.len(), 1);
    assert!(matches!(snap[0], AuditEvent::DisagreementFlagged(_)));
}

// ══════════════════════════════════════════════════════════════════════════════
// § tie-break STABLE under sig-permutation
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn tie_break_stable_when_sigs_permuted() {
    let ev = signed_event(0x10, 1, None);
    let state = StateSnapshot::empty();
    let lineage: Lineage<MockSigmaEvent> = Lineage::empty();
    let bogus_a = [0x11; 32];
    let bogus_b = [0x22; 32];

    let validator = ConsensusValidator::new(VecAuditEmitter::new());
    let r1 = validator.run(
        &state,
        &lineage,
        &ev,
        &make_view(0x01, bogus_a),
        &make_view(0xfe, bogus_b),
    );
    let r2 = validator.run(
        &state,
        &lineage,
        &ev,
        &make_view(0xfe, bogus_b),
        &make_view(0x01, bogus_a),
    );

    // r1 winner is index 0 with bogus_a ; r2 winner is index 1 with bogus_a.
    let ConsensusReport::Disagreed { winner_root: w1, .. } = r1 else {
        panic!("expected Disagreed");
    };
    let ConsensusReport::Disagreed { winner_root: w2, .. } = r2 else {
        panic!("expected Disagreed");
    };
    assert_eq!(w1, w2);
    assert_eq!(w1, bogus_a);
}
