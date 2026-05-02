// § T11-W11-SIGMA-CHAIN-INTEG : integration tests · 11 mandatory tests + extras
// §§ exercises full crate-API : entry · merkle · chain · attest · coherence · federation
// §§ runs as separate test-target (not lib-tests) → catches pub-API ergonomic issues

#![forbid(unsafe_code)]

use cssl_substrate_sigma_chain::{
    checkpoint_is_self_consistent, count_checkpoint_marks, hash_payload, record_attestation,
    record_cap_grant, record_cell_emission, record_hotfix_bundle, record_knowledge_ingest,
    record_mycelium_pattern, verify_coherence_from_genesis, verify_inclusion, AppendResult,
    Attestation, ChainError, CoherenceOutcome, EntryKind, IncoherenceReason, IncrementalMerkle,
    LedgerEntry, SigmaChain, ENTRY_WIRE_SIZE, ZERO_HASH,
};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use std::sync::Arc;
use std::thread;

// § helper : sign a single entry with given signer
fn sign_entry(signer: &SigningKey, entry: &mut LedgerEntry) {
    let msg = entry.canonical_bytes_for_sign();
    let sig = signer.sign(&msg);
    entry.signature = sig.to_bytes();
}

// § helper : build a signed entry · seq+prev_root ALREADY filled in by caller
fn build_signed_entry(
    signer: &SigningKey,
    seq_no: u64,
    prev_root: [u8; 32],
    kind: EntryKind,
    payload: &[u8],
    ts_unix: u64,
) -> LedgerEntry {
    let mut entry = LedgerEntry {
        seq_no,
        ts_unix,
        kind,
        actor_pubkey: signer.verifying_key().to_bytes(),
        payload_hash: hash_payload(payload),
        prev_root,
        signature: [0u8; 64],
    };
    sign_entry(signer, &mut entry);
    entry
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 1 : Genesis empty-chain → root = ZERO_HASH
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t01_genesis_empty_root_zero() {
    let chain = SigmaChain::new();
    assert_eq!(chain.tail().last_root, ZERO_HASH);
    assert_eq!(chain.len(), 0);
    assert_eq!(chain.tail().next_seq_no, 1);
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 2 : Single-entry append · root != ZERO · entry.prev_root == ZERO
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t02_single_entry_append() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    let res =
        record_cap_grant(&chain, &signer, b"cap=read:fs:/etc", 1_700_000_000).expect("record");
    assert_eq!(res.seq_no, 1);
    assert_ne!(res.root, ZERO_HASH);
    let e = chain.get(1).expect("get");
    assert_eq!(e.prev_root, ZERO_HASH);
    assert_eq!(chain.tail().last_root, res.root);
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 3 : 1000 entries · final root = replay-derived root
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t03_thousand_entries_replay_match() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    for i in 0..1000u64 {
        let payload = format!("entry-{i}");
        record_attestation(
            &chain,
            &signer,
            &Attestation::new(EntryKind::CapGrant, payload.as_bytes(), 1_700_000_000 + i),
        )
        .expect("record");
    }
    assert_eq!(chain.len(), 1000);
    let claimed_root = chain.tail().last_root;
    let outcome = verify_coherence_from_genesis(&chain, &claimed_root);
    match outcome {
        CoherenceOutcome::Coherent {
            entries_verified,
            final_root,
        } => {
            assert_eq!(entries_verified, 1000);
            assert_eq!(final_root, claimed_root);
        }
        CoherenceOutcome::Incoherent { failed_at_seq, .. } => {
            panic!("Coherence failed at seq {failed_at_seq}");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 4 : Tamper-detect · change one entry's payload-hash → replay fails
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t04_tamper_payload_hash_caught() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    for i in 0..10u64 {
        record_cap_grant(&chain, &signer, format!("e{i}").as_bytes(), 1_700_000_000 + i)
            .expect("record");
    }
    // simulate tampering : grab all entries · mutate one · attempt to verify-coherent
    let mut entries = chain.all_entries();
    entries[5].payload_hash[0] ^= 0xFF;
    // construct a fresh chain · feed mutated entries (bypassing API · simulating an attacker)
    // Since SigmaChain only allows valid append, we verify_replay manually :
    // The signature over the original payload-hash will FAIL when re-verified.
    // Use coherence module on a fresh chain populated from mutated stream.
    // We'll simulate by constructing a mock SigmaChain via raw insertion — not possible
    // through pub API ; so instead test : verify the signature DIRECTLY on tampered.
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let bad_entry = &entries[5];
    let pk = VerifyingKey::from_bytes(&bad_entry.actor_pubkey).expect("pk");
    let sig = Signature::from_bytes(&bad_entry.signature);
    let msg = bad_entry.canonical_bytes_for_sign();
    // verification should FAIL because payload_hash has been mutated, but the signature
    // was over the ORIGINAL payload_hash. Hence verify(pk, sig, mutated_msg) → Err.
    assert!(
        pk.verify(&msg, &sig).is_err(),
        "Ed25519 should reject tampered payload-hash"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 5 : Tamper-detect · change one entry's signature → verify fails
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t05_tamper_signature_caught() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    for i in 0..5u64 {
        record_cap_grant(&chain, &signer, format!("e{i}").as_bytes(), 1_700_000_000 + i)
            .expect("record");
    }
    let mut entries = chain.all_entries();
    // tamper signature of entry 2
    entries[2].signature[0] ^= 0xFF;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let bad_entry = &entries[2];
    let pk = VerifyingKey::from_bytes(&bad_entry.actor_pubkey).expect("pk");
    let sig = Signature::from_bytes(&bad_entry.signature);
    let msg = bad_entry.canonical_bytes_for_sign();
    assert!(
        pk.verify(&msg, &sig).is_err(),
        "Ed25519 should reject tampered signature"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 6 : Inclusion-proof · O(log N) sibling-hashes
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t06_inclusion_proof_size_log_n() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    for i in 0..256u64 {
        record_cap_grant(&chain, &signer, format!("e{i}").as_bytes(), 1_700_000_000 + i)
            .expect("record");
    }
    let entries = chain.all_entries();
    for seq in [1u64, 50, 128, 200, 256] {
        let proof = chain.prove(seq).expect("prove");
        // log2(256) = 8 ; depth-of-tree ≤ 8 sibling-hashes
        assert!(
            proof.path.len() <= 9,
            "inclusion-proof for seq {} should be O(log N) ≤ 9 (got {})",
            seq,
            proof.path.len()
        );
        let leaf_data = entries[(seq - 1) as usize].leaf_hash();
        assert!(verify_inclusion(&leaf_data, &proof, &chain.tail().last_root));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 7 : Inclusion-proof tamper · bad sibling-hash → proof rejected
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t07_inclusion_proof_tamper_rejected() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    for i in 0..32u64 {
        record_cap_grant(&chain, &signer, format!("e{i}").as_bytes(), 1_700_000_000 + i)
            .expect("record");
    }
    let entries = chain.all_entries();
    let mut proof = chain.prove(7).expect("prove");
    // tamper a sibling-hash mid-path
    proof.path[1].1[0] ^= 0xFF;
    let leaf_data = entries[6].leaf_hash();
    assert!(
        !verify_inclusion(&leaf_data, &proof, &chain.tail().last_root),
        "tampered proof must be rejected"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 8 : Checkpoint · 2048 entries → 2 checkpoints @ 1024 + 2048
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t08_checkpoints_at_1024_intervals() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    let mut checkpoint_count = 0;
    for i in 0..2048u64 {
        let res = record_cap_grant(
            &chain,
            &signer,
            format!("e{i}").as_bytes(),
            1_700_000_000 + i,
        )
        .expect("record");
        if res.checkpoint_emitted {
            checkpoint_count += 1;
        }
    }
    assert_eq!(checkpoint_count, 2, "expected 2 checkpoints @ seq 1024 + 2048");
    let cps = chain.checkpoints();
    assert_eq!(cps.len(), 2);
    assert_eq!(cps[0].seq_no, 1024);
    assert_eq!(cps[1].seq_no, 2048);
    assert_eq!(cps[0].epoch, 1);
    assert_eq!(cps[1].epoch, 2);
    // Each checkpoint must be self-consistent (root reproducible from snapshot leaves)
    for cp in &cps {
        assert!(checkpoint_is_self_consistent(cp));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 9 : Snapshot-replay · replay from checkpoint produces same final-root
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t09_snapshot_replay_matches_root() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    for i in 0..1100u64 {
        record_cap_grant(
            &chain,
            &signer,
            format!("e{i}").as_bytes(),
            1_700_000_000 + i,
        )
        .expect("record");
    }
    let cps = chain.checkpoints();
    assert_eq!(cps.len(), 1);
    let cp = &cps[0];
    // snapshot leaves at 1024 + 76 post-cp leaves should reproduce the chain root
    // by extending the snapshot with the leaf-hashes of entries 1025..=1100
    let mut merkle = IncrementalMerkle::restore_from(cp.leaves.clone());
    let entries = chain.all_entries();
    for e in &entries[(cp.seq_no as usize)..] {
        let leaf = e.leaf_hash();
        merkle.append(&leaf);
    }
    assert_eq!(merkle.root(), chain.tail().last_root);
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 10 : Concurrent-append · 10 threads × 100 appends · final seq_no = 1000
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t10_concurrent_append_no_seq_dupes() {
    let chain = SigmaChain::new();
    let signer = Arc::new(SigningKey::generate(&mut OsRng));

    let mut handles = Vec::new();
    for thread_id in 0..10u64 {
        let chain_c = chain.clone();
        let signer_c = signer.clone();
        let h = thread::spawn(move || {
            for i in 0..100u64 {
                let payload = format!("t{thread_id}-i{i}");
                record_attestation(
                    &chain_c,
                    &signer_c,
                    &Attestation::new(
                        EntryKind::CapGrant,
                        payload.as_bytes(),
                        1_700_000_000 + thread_id * 1000 + i,
                    ),
                )
                .expect("record");
            }
        });
        handles.push(h);
    }
    for h in handles {
        h.join().expect("thread join");
    }

    assert_eq!(chain.len(), 1000);
    assert_eq!(chain.tail().next_seq_no, 1001);

    // gapless monotonic seq_no check
    let entries = chain.all_entries();
    for (i, e) in entries.iter().enumerate() {
        assert_eq!(
            e.seq_no,
            (i + 1) as u64,
            "seq_no must be gapless monotonic"
        );
    }

    // verify Coherence-Proof : full-chain replay must succeed
    let outcome = verify_coherence_from_genesis(&chain, &chain.tail().last_root);
    match outcome {
        CoherenceOutcome::Coherent {
            entries_verified, ..
        } => assert_eq!(entries_verified, 1000),
        CoherenceOutcome::Incoherent { failed_at_seq, .. } => {
            panic!("concurrent-append broke coherence at seq {failed_at_seq}")
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § TEST 11 : EntryKind · all kinds round-trip serialize+deserialize
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn t11_all_entry_kinds_round_trip() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);

    let r1 = record_cap_grant(&chain, &signer, b"cap=read", 1_700_000_001).expect("cap-grant");
    let r2 = record_attestation(
        &chain,
        &signer,
        &Attestation::new(EntryKind::CapRevoke, b"cap=read", 1_700_000_002),
    )
    .expect("cap-revoke");
    let r3 = record_cell_emission(&chain, &signer, b"cell-bytes", 1_700_000_003).expect("cell");
    let r4 = record_attestation(
        &chain,
        &signer,
        &Attestation::new(EntryKind::AttestationAnchor, b"att-doc", 1_700_000_004),
    )
    .expect("att");
    let r5 = record_hotfix_bundle(&chain, &signer, b"bundle-sig", 1_700_000_005).expect("hf");
    let r6 = record_mycelium_pattern(&chain, &signer, b"pattern", 1_700_000_006).expect("myc");
    let r7 = record_knowledge_ingest(&chain, &signer, b"knowledge", 1_700_000_007).expect("kn");
    let r8 = record_attestation(
        &chain,
        &signer,
        &Attestation::new(EntryKind::FederationAnchor, b"peer-cp", 1_700_000_008),
    )
    .expect("fed");

    assert_eq!(chain.len(), 8);

    // Verify each entry has the right discriminant
    let kinds = [
        (r1.seq_no, EntryKind::CapGrant),
        (r2.seq_no, EntryKind::CapRevoke),
        (r3.seq_no, EntryKind::CellEmission),
        (r4.seq_no, EntryKind::AttestationAnchor),
        (r5.seq_no, EntryKind::HotfixBundle),
        (r6.seq_no, EntryKind::MyceliumPattern),
        (r7.seq_no, EntryKind::KnowledgeIngest),
        (r8.seq_no, EntryKind::FederationAnchor),
    ];
    for (seq, expected_kind) in kinds {
        let e = chain.get(seq).expect("get");
        assert_eq!(e.kind, expected_kind, "kind mismatch at seq {seq}");
        // round-trip via u16
        let n = e.kind.as_u16();
        assert_eq!(EntryKind::from_u16(n), expected_kind);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § BONUS : Coherence-Proof catches PrevRoot mismatch
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn bonus_seq_mismatch_rejected() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    let bad = build_signed_entry(&signer, 99, ZERO_HASH, EntryKind::CapGrant, b"x", 1);
    let res = chain.append(bad);
    assert!(matches!(res, Err(ChainError::SeqMismatch { .. })));
}

// ═══════════════════════════════════════════════════════════════════════
// § BONUS : sovereign-rollback rewinds chain
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn bonus_sovereign_rollback() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    for i in 0..10u64 {
        record_cap_grant(&chain, &signer, format!("e{i}").as_bytes(), 1_700_000_000 + i)
            .expect("record");
    }
    let original_root = chain.tail().last_root;
    chain.sovereign_rollback(5).expect("rollback");
    assert_eq!(chain.len(), 5);
    assert_eq!(chain.tail().next_seq_no, 6);
    assert_ne!(chain.tail().last_root, original_root);
    // Re-verify coherence after rollback
    let outcome = verify_coherence_from_genesis(&chain, &chain.tail().last_root);
    assert!(matches!(outcome, CoherenceOutcome::Coherent { .. }));
}

// ═══════════════════════════════════════════════════════════════════════
// § BONUS : entry wire-size attestation
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn bonus_entry_wire_size_constant() {
    // 8 + 8 + 2 + 32 + 32 + 32 + 64 = 178 bytes wire-encoded
    assert_eq!(ENTRY_WIRE_SIZE, 178);
    let signer = SigningKey::generate(&mut OsRng);
    let e = build_signed_entry(&signer, 1, ZERO_HASH, EntryKind::CapGrant, b"x", 1);
    let bytes = e.canonical_bytes_for_sign();
    // canonical_bytes excludes signature itself = 178 - 64 = 114 bytes (+ domain prefix)
    let domain_len = b"cssl-substrate-sigma-chain/v0/entry".len();
    assert_eq!(bytes.len(), domain_len + 114);
}

// ═══════════════════════════════════════════════════════════════════════
// § BONUS : count_checkpoint_marks reflects auto-emitted checkpoints
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn bonus_checkpoint_count_helper() {
    // The current implementation auto-records checkpoint METADATA but does not
    // emit a CheckpointMark ENTRY (that's a follow-up design choice). This test
    // confirms current behavior : count_checkpoint_marks == 0 for now.
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    for i in 0..1024u64 {
        record_cap_grant(&chain, &signer, format!("e{i}").as_bytes(), 1_700_000_000 + i)
            .expect("record");
    }
    assert_eq!(chain.checkpoints().len(), 1);
    assert_eq!(count_checkpoint_marks(&chain), 0); // checkpoints stored as metadata, not entries
}

// ═══════════════════════════════════════════════════════════════════════
// § BONUS : verify the AppendResult signals checkpoint correctly
// ═══════════════════════════════════════════════════════════════════════
#[test]
fn bonus_append_result_checkpoint_signal() {
    let chain = SigmaChain::new();
    let signer = SigningKey::generate(&mut OsRng);
    let mut hits = 0;
    for i in 0..1025u64 {
        let r = record_cap_grant(
            &chain,
            &signer,
            format!("e{i}").as_bytes(),
            1_700_000_000 + i,
        )
        .expect("record");
        if r.checkpoint_emitted {
            hits += 1;
            assert_eq!(r.seq_no, 1024);
        }
    }
    assert_eq!(hits, 1);
}

// helper test — silence unused-imports if any
#[test]
fn smoke_imports_resolve() {
    let _ = AppendResult {
        seq_no: 0,
        root: ZERO_HASH,
        checkpoint_emitted: false,
    };
    let _ = IncoherenceReason::BadSignature;
}
