// § T11-W8-C1 sigma_chain_tests — ≥30 tests covering :
//   privacy-tier-construction · sign+verify happy-path · sig-tamper · payload-tamper
//   merkle-tree various-leaf-counts · merkle-path good/bad · privacy-strip-on-Anonymized
//   LocalOnly never-egress invariant · serde round-trip · canonical-bytes-stable
//   deterministic-recompute

use cssl_host_sigma_chain::{
    canonical_bytes, egress_check, merkle_path_verify, merkle_root_of, payload_blake3,
    sanitize_for_egress, sign_event, verify_event, CoherenceProof, EventKind, LedgerSnapshot,
    PrivacyTier, SigmaEvent, SigmaLedger, SigmaPayload, VerifyError, VerifyOutcome,
    PUBKEY_LEN, SIG_LEN,
};
use cssl_host_sigma_chain::merkle::{empty_root, leaf_hash, merkle_path_of, node_hash, Digest};
use cssl_host_sigma_chain::privacy::{anonymized_pubkey_replacement, is_sensitive_field};
use cssl_host_sigma_chain::sign::recompute_event_id;
use cssl_host_sigma_chain::verify::{
    verify_coherence_proof, verify_coherence_proof_with_lineage,
};

use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rand::SeedableRng;
use rand::rngs::StdRng;

fn fixed_signer(seed: u8) -> SigningKey {
    let mut rng = StdRng::seed_from_u64(0xA1B2_C3D4_E5F6_0700 | u64::from(seed));
    SigningKey::generate(&mut rng)
}

fn make_payload(label: &[u8]) -> SigmaPayload {
    SigmaPayload::new(label.to_vec())
}

fn signed(
    sk: &SigningKey,
    kind: EventKind,
    ts: u64,
    parent: Option<[u8; 32]>,
    payload: &SigmaPayload,
    tier: PrivacyTier,
) -> SigmaEvent {
    sign_event(sk, kind, ts, parent, payload, tier)
}

// ───────────────────────────────────────────────────────────────────────────
// 1. Privacy-tier construction (4 tests : one per variant)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn privacy_tier_local_only_default() {
    assert_eq!(PrivacyTier::default(), PrivacyTier::LocalOnly);
    assert!(!PrivacyTier::LocalOnly.permits_egress());
    assert_eq!(PrivacyTier::LocalOnly.tag(), "local_only");
}

#[test]
fn privacy_tier_anonymized_egress_ok() {
    assert!(PrivacyTier::Anonymized.permits_egress());
    assert_eq!(PrivacyTier::Anonymized.tag(), "anonymized");
}

#[test]
fn privacy_tier_pseudonymous_egress_ok() {
    assert!(PrivacyTier::Pseudonymous.permits_egress());
    assert_eq!(PrivacyTier::Pseudonymous.tag(), "pseudonymous");
}

#[test]
fn privacy_tier_public_egress_ok() {
    assert!(PrivacyTier::Public.permits_egress());
    assert_eq!(PrivacyTier::Public.tag(), "public");
}

// ───────────────────────────────────────────────────────────────────────────
// 2. Sign-then-verify happy path (3 tests)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn sign_verify_happy_loot_drop() {
    let sk = fixed_signer(1);
    let payload = make_payload(b"loot=epic-sword");
    let ev = signed(&sk, EventKind::LootDrop, 100, None, &payload, PrivacyTier::Pseudonymous);
    assert_eq!(verify_event(&ev), VerifyOutcome::Verified);
}

#[test]
fn sign_verify_happy_combat_outcome_with_parent() {
    let sk = fixed_signer(2);
    let p1 = signed(
        &sk,
        EventKind::CombatOutcome,
        50,
        None,
        &make_payload(b"win"),
        PrivacyTier::Public,
    );
    let p2 = signed(
        &sk,
        EventKind::CombatOutcome,
        51,
        Some(p1.id),
        &make_payload(b"loss"),
        PrivacyTier::Public,
    );
    assert_eq!(verify_event(&p1), VerifyOutcome::Verified);
    assert_eq!(verify_event(&p2), VerifyOutcome::Verified);
    assert_eq!(p2.parent_event_id, Some(p1.id));
}

#[test]
fn sign_verify_happy_kan_canary_pseudonymous() {
    let sk = fixed_signer(3);
    let ev = signed(
        &sk,
        EventKind::KanCanary,
        9999,
        None,
        &make_payload(b"obs=0.42"),
        PrivacyTier::Pseudonymous,
    );
    assert_eq!(verify_event(&ev), VerifyOutcome::Verified);
}

// ───────────────────────────────────────────────────────────────────────────
// 3. Sig-tamper rejected (2 tests)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn sig_tamper_one_bit_flip_rejected() {
    let sk = fixed_signer(4);
    let mut ev = signed(
        &sk,
        EventKind::CraftSuccess,
        10,
        None,
        &make_payload(b"recipe=42"),
        PrivacyTier::Public,
    );
    ev.ed25519_sig[0] ^= 0x01;
    assert_eq!(
        verify_event(&ev),
        VerifyOutcome::Rejected(VerifyError::SignatureInvalid)
    );
}

#[test]
fn sig_tamper_full_overwrite_rejected() {
    let sk = fixed_signer(5);
    let mut ev = signed(
        &sk,
        EventKind::AchievementUnlock,
        77,
        None,
        &make_payload(b"first-clear"),
        PrivacyTier::Public,
    );
    ev.ed25519_sig = [0xCD; SIG_LEN];
    let outcome = verify_event(&ev);
    assert!(matches!(
        outcome,
        VerifyOutcome::Rejected(VerifyError::SignatureInvalid | VerifyError::InvalidPubkey)
    ));
}

// ───────────────────────────────────────────────────────────────────────────
// 4. Payload-tamper rejected (2 tests : flip blake3 vs flip kind)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn payload_tamper_blake3_flip_detected_via_id_or_sig() {
    let sk = fixed_signer(6);
    let mut ev = signed(
        &sk,
        EventKind::NpcDeath,
        500,
        None,
        &make_payload(b"npc=goblin-king"),
        PrivacyTier::Public,
    );
    ev.payload_blake3[0] ^= 0xFF;
    let outcome = verify_event(&ev);
    assert!(matches!(
        outcome,
        VerifyOutcome::Rejected(VerifyError::IdTampered | VerifyError::SignatureInvalid)
    ));
}

#[test]
fn payload_tamper_kind_swap_detected() {
    let sk = fixed_signer(7);
    let mut ev = signed(
        &sk,
        EventKind::GearTransfer,
        333,
        None,
        &make_payload(b"item=cloak"),
        PrivacyTier::Public,
    );
    ev.kind = EventKind::NemesisDefeat;
    let outcome = verify_event(&ev);
    assert!(matches!(
        outcome,
        VerifyOutcome::Rejected(VerifyError::IdTampered | VerifyError::SignatureInvalid)
    ));
}

// ───────────────────────────────────────────────────────────────────────────
// 5. Merkle tree leaf-counts (5 tests : 1/2/4/8/16)
// ───────────────────────────────────────────────────────────────────────────

fn fake_ids(n: usize, salt: u8) -> Vec<Digest> {
    (0..n)
        .map(|i| {
            let mut h = blake3::Hasher::new();
            h.update(&[salt]);
            h.update(&u64::try_from(i).unwrap().to_le_bytes());
            let mut out = [0u8; 32];
            out.copy_from_slice(h.finalize().as_bytes());
            out
        })
        .collect()
}

#[test]
fn merkle_tree_1_leaf() {
    let ids = fake_ids(1, 0xAA);
    let root = merkle_root_of(&ids);
    assert_eq!(root, leaf_hash(&ids[0]));
}

#[test]
fn merkle_tree_2_leaves() {
    let ids = fake_ids(2, 0xBB);
    let root = merkle_root_of(&ids);
    assert_eq!(root, node_hash(&leaf_hash(&ids[0]), &leaf_hash(&ids[1])));
}

#[test]
fn merkle_tree_4_leaves() {
    let ids = fake_ids(4, 0xCC);
    let root = merkle_root_of(&ids);
    let l0 = leaf_hash(&ids[0]);
    let l1 = leaf_hash(&ids[1]);
    let l2 = leaf_hash(&ids[2]);
    let l3 = leaf_hash(&ids[3]);
    let n01 = node_hash(&l0, &l1);
    let n23 = node_hash(&l2, &l3);
    assert_eq!(root, node_hash(&n01, &n23));
}

#[test]
fn merkle_tree_8_leaves_root_stable() {
    let ids = fake_ids(8, 0xDD);
    let r1 = merkle_root_of(&ids);
    let r2 = merkle_root_of(&ids);
    assert_eq!(r1, r2);
    assert_ne!(r1, [0u8; 32]);
}

#[test]
fn merkle_tree_16_leaves_root_stable() {
    let ids = fake_ids(16, 0xEE);
    let r1 = merkle_root_of(&ids);
    let r2 = merkle_root_of(&ids);
    assert_eq!(r1, r2);
}

// odd-leaf-count duplicate-last-leaf
#[test]
fn merkle_tree_3_leaves_duplicates_last() {
    let ids = fake_ids(3, 0xF0);
    let root = merkle_root_of(&ids);
    // hand-recompute : pad ids to 4 by duplicating-last-LEAF-HASH (per merkle.rs row-pad).
    let l0 = leaf_hash(&ids[0]);
    let l1 = leaf_hash(&ids[1]);
    let l2 = leaf_hash(&ids[2]);
    let n01 = node_hash(&l0, &l1);
    let n22 = node_hash(&l2, &l2);
    assert_eq!(root, node_hash(&n01, &n22));
}

#[test]
fn merkle_tree_empty_root_distinct() {
    assert_ne!(empty_root(), [0u8; 32]);
    let r = merkle_root_of(&[]);
    assert_eq!(r, empty_root());
}

// ───────────────────────────────────────────────────────────────────────────
// 6. Merkle path verify good/bad (4 tests)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn merkle_path_good_4leaves() {
    let ids = fake_ids(4, 0xA1);
    let root = merkle_root_of(&ids);
    for target in &ids {
        let path = merkle_path_of(&ids, target).expect("path");
        assert!(merkle_path_verify(target, &path, &root));
    }
}

#[test]
fn merkle_path_good_8leaves() {
    let ids = fake_ids(8, 0xA2);
    let root = merkle_root_of(&ids);
    let path = merkle_path_of(&ids, &ids[5]).expect("path");
    assert!(merkle_path_verify(&ids[5], &path, &root));
}

#[test]
fn merkle_path_bad_wrong_target() {
    let ids = fake_ids(4, 0xA3);
    let root = merkle_root_of(&ids);
    let path = merkle_path_of(&ids, &ids[0]).expect("path");
    let wrong = fake_ids(1, 0xFE)[0];
    assert!(!merkle_path_verify(&wrong, &path, &root));
}

#[test]
fn merkle_path_bad_tampered_step() {
    let ids = fake_ids(4, 0xA4);
    let root = merkle_root_of(&ids);
    let mut path = merkle_path_of(&ids, &ids[2]).expect("path");
    path[0].sibling[0] ^= 0xFF;
    assert!(!merkle_path_verify(&ids[2], &path, &root));
}

// ───────────────────────────────────────────────────────────────────────────
// 7. Privacy-tier strip emitter on Anonymized (2 tests)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn anonymized_strips_emitter_pubkey() {
    let sk = fixed_signer(8);
    let ev = signed(
        &sk,
        EventKind::CombatOutcome,
        1,
        Some([7u8; 32]),
        &make_payload(b"x"),
        PrivacyTier::Anonymized,
    );
    let san = sanitize_for_egress(&ev).unwrap();
    assert_ne!(san.emitter_pubkey, ev.emitter_pubkey);
    assert_eq!(
        san.emitter_pubkey,
        anonymized_pubkey_replacement(&ev.emitter_pubkey)
    );
    // parent-id stripped to prevent longitudinal-tracking
    assert_eq!(san.parent_event_id, None);
}

#[test]
fn pseudonymous_keeps_emitter_pubkey_and_lineage() {
    let sk = fixed_signer(9);
    let ev = signed(
        &sk,
        EventKind::CraftSuccess,
        2,
        Some([8u8; 32]),
        &make_payload(b"y"),
        PrivacyTier::Pseudonymous,
    );
    let san = sanitize_for_egress(&ev).unwrap();
    assert_eq!(san.emitter_pubkey, ev.emitter_pubkey);
    assert_eq!(san.parent_event_id, Some([8u8; 32]));
}

// ───────────────────────────────────────────────────────────────────────────
// 8. LocalOnly never-egress invariant (2 tests)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn local_only_egress_check_refuses() {
    let sk = fixed_signer(10);
    let ev = signed(
        &sk,
        EventKind::LootDrop,
        1,
        None,
        &make_payload(b"private"),
        PrivacyTier::LocalOnly,
    );
    assert!(egress_check(&ev).is_err());
    assert!(!ev.may_egress());
}

#[test]
fn local_only_sanitize_refuses() {
    let sk = fixed_signer(11);
    let ev = signed(
        &sk,
        EventKind::AchievementUnlock,
        2,
        None,
        &make_payload(b"private2"),
        PrivacyTier::LocalOnly,
    );
    let r = sanitize_for_egress(&ev);
    assert!(r.is_err());
    let msg = format!("{}", r.unwrap_err());
    assert!(msg.contains("LocalOnly"));
}

// ───────────────────────────────────────────────────────────────────────────
// 9. Serde round-trip (2 tests : SigmaEvent · LedgerSnapshot)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn serde_roundtrip_sigma_event() {
    let sk = fixed_signer(12);
    let ev = signed(
        &sk,
        EventKind::NemesisDefeat,
        42,
        None,
        &make_payload(b"nemesis=lich-lord"),
        PrivacyTier::Public,
    );
    let json = serde_json::to_string(&ev).unwrap();
    let back: SigmaEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(ev, back);
    assert_eq!(verify_event(&back), VerifyOutcome::Verified);
}

#[test]
fn serde_roundtrip_ledger_snapshot() {
    let sk = fixed_signer(13);
    let mut ledger = SigmaLedger::new();
    for i in 0..4 {
        let ev = signed(
            &sk,
            EventKind::LootDrop,
            i as u64,
            None,
            &make_payload(format!("drop-{i}").as_bytes()),
            PrivacyTier::Public,
        );
        ledger.insert(ev).unwrap();
    }
    let snap = ledger.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let back: LedgerSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back.events.len(), 4);
    assert_eq!(back.merkle_root, snap.merkle_root);
}

// ───────────────────────────────────────────────────────────────────────────
// 10. Canonical-bytes stable (2 tests)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn canonical_bytes_stable_across_invocations() {
    let pk = [0xAB; PUBKEY_LEN];
    let payload_h = [0xCD; 32];
    let a = canonical_bytes(
        EventKind::LootDrop,
        100,
        None,
        &pk,
        &payload_h,
        PrivacyTier::Public,
    );
    let b = canonical_bytes(
        EventKind::LootDrop,
        100,
        None,
        &pk,
        &payload_h,
        PrivacyTier::Public,
    );
    assert_eq!(a, b);
    assert!(a.len() > 64);
}

#[test]
fn canonical_bytes_changes_with_kind_or_tier() {
    let pk = [0u8; PUBKEY_LEN];
    let payload_h = [0u8; 32];
    let base = canonical_bytes(
        EventKind::LootDrop,
        1,
        None,
        &pk,
        &payload_h,
        PrivacyTier::Public,
    );
    let other_kind = canonical_bytes(
        EventKind::NpcDeath,
        1,
        None,
        &pk,
        &payload_h,
        PrivacyTier::Public,
    );
    let other_tier = canonical_bytes(
        EventKind::LootDrop,
        1,
        None,
        &pk,
        &payload_h,
        PrivacyTier::Pseudonymous,
    );
    assert_ne!(base, other_kind);
    assert_ne!(base, other_tier);
}

// ───────────────────────────────────────────────────────────────────────────
// 11. Deterministic-recompute (2 tests + bonus invariants)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn deterministic_recompute_matches_ledger_root() {
    let sk = fixed_signer(14);
    let mut ledger = SigmaLedger::new();
    let mut events = Vec::new();
    for i in 0..6 {
        let ev = signed(
            &sk,
            EventKind::CombatOutcome,
            i as u64,
            None,
            &make_payload(format!("c{i}").as_bytes()),
            PrivacyTier::Public,
        );
        ledger.insert(ev.clone()).unwrap();
        events.push(ev);
    }
    let direct_root = ledger.merkle_root();
    let recomputed = SigmaLedger::deterministic_recompute_root(&[], &events);
    assert_eq!(direct_root, recomputed);
}

#[test]
fn deterministic_recompute_idempotent_with_dupes() {
    let sk = fixed_signer(15);
    let ev = signed(
        &sk,
        EventKind::CraftSuccess,
        7,
        None,
        &make_payload(b"only"),
        PrivacyTier::Public,
    );
    let r1 = SigmaLedger::deterministic_recompute_root(&[], &[ev.clone()]);
    // Duplicate the event in input ; result must be unchanged (BTreeMap dedup).
    let r2 = SigmaLedger::deterministic_recompute_root(&[], &[ev.clone(), ev]);
    assert_eq!(r1, r2);
}

// ───────────────────────────────────────────────────────────────────────────
// 12. Bonus : Coherence-Proof end-to-end + ledger guards + sensitive-fields
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn coherence_proof_verifies_end_to_end() {
    let sk = fixed_signer(16);
    let mut ledger = SigmaLedger::new();
    let mut all = Vec::new();
    for i in 0..5 {
        let ev = signed(
            &sk,
            EventKind::AchievementUnlock,
            (100 + i) as u64,
            None,
            &make_payload(format!("ach-{i}").as_bytes()),
            PrivacyTier::Public,
        );
        ledger.insert(ev.clone()).unwrap();
        all.push(ev);
    }
    let target = &all[2];
    let proof: CoherenceProof = ledger.coherence_proof_for(&target.id).unwrap();
    assert_eq!(verify_coherence_proof(&proof), VerifyOutcome::Verified);
    assert_eq!(
        verify_coherence_proof_with_lineage(&proof, &[], &all),
        VerifyOutcome::Verified
    );
}

#[test]
fn coherence_proof_lineage_mismatch_rejected() {
    let sk = fixed_signer(17);
    let mut ledger = SigmaLedger::new();
    let mut all = Vec::new();
    for i in 0..3 {
        let ev = signed(
            &sk,
            EventKind::LootDrop,
            i as u64,
            None,
            &make_payload(format!("d{i}").as_bytes()),
            PrivacyTier::Public,
        );
        ledger.insert(ev.clone()).unwrap();
        all.push(ev);
    }
    let target = all[0].clone();
    let proof = ledger.coherence_proof_for(&target.id).unwrap();
    // Provide WRONG lineage (drop one event) → recompute must mismatch.
    let bad_lineage: Vec<_> = all.iter().take(2).cloned().collect();
    assert_eq!(
        verify_coherence_proof_with_lineage(&proof, &[], &bad_lineage),
        VerifyOutcome::Rejected(VerifyError::RecomputeMismatch)
    );
}

#[test]
fn ledger_rejects_id_mismatch() {
    let sk = fixed_signer(18);
    let mut ev = signed(
        &sk,
        EventKind::GearTransfer,
        9,
        None,
        &make_payload(b"transfer"),
        PrivacyTier::Public,
    );
    ev.id[0] ^= 0xFF;
    let mut ledger = SigmaLedger::new();
    assert!(ledger.insert(ev).is_err());
}

#[test]
fn ledger_rejects_duplicate_id() {
    let sk = fixed_signer(19);
    let ev = signed(
        &sk,
        EventKind::KanCanary,
        11,
        None,
        &make_payload(b"obs"),
        PrivacyTier::Public,
    );
    let mut ledger = SigmaLedger::new();
    ledger.insert(ev.clone()).unwrap();
    assert!(ledger.insert(ev).is_err());
}

#[test]
fn sensitive_fields_detected_by_substring() {
    assert!(is_sensitive_field("biometric_hr"));
    assert!(is_sensitive_field("user_face_landmarks"));
    assert!(is_sensitive_field("GAZE_DIRECTION"));
    assert!(is_sensitive_field("body_pose_3d"));
    assert!(is_sensitive_field("raw_field_cell_dump"));
    assert!(!is_sensitive_field("loot_id"));
    assert!(!is_sensitive_field("combat_score"));
}

#[test]
fn payload_blake3_stable() {
    let p = make_payload(b"abc");
    assert_eq!(payload_blake3(&p), payload_blake3(&p));
    assert_ne!(payload_blake3(&p), payload_blake3(&make_payload(b"abd")));
}

#[test]
fn recompute_event_id_matches_signed_id() {
    let sk = fixed_signer(20);
    let ev = signed(
        &sk,
        EventKind::CombatOutcome,
        555,
        None,
        &make_payload(b"win"),
        PrivacyTier::Public,
    );
    assert_eq!(recompute_event_id(&ev), ev.id);
}

#[test]
fn fresh_signer_unique_pubkeys() {
    let s1 = SigningKey::generate(&mut OsRng);
    let s2 = SigningKey::generate(&mut OsRng);
    assert_ne!(s1.verifying_key().to_bytes(), s2.verifying_key().to_bytes());
}

#[test]
fn ledger_iter_is_sorted_by_id() {
    let sk = fixed_signer(21);
    let mut ledger = SigmaLedger::new();
    for i in 0..10u64 {
        let ev = signed(
            &sk,
            EventKind::LootDrop,
            i,
            None,
            &make_payload(format!("p{i}").as_bytes()),
            PrivacyTier::Public,
        );
        ledger.insert(ev).unwrap();
    }
    let ids = ledger.sorted_event_ids();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
}
