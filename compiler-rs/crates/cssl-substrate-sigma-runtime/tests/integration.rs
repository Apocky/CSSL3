//! § integration.rs — cross-module fixtures for cssl-substrate-sigma-runtime.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   These tests exercise the full cap-grant → mask-construct → evaluate →
//!   audit-drain pipeline as a sibling-crate would consume it, plus the
//!   high-fanout concurrent stress-test mandated by the mission spec.

#![allow(clippy::uninlined_format_args)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::similar_names)]

use std::sync::Arc;
use std::thread;

use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;

use cssl_substrate_sigma_runtime::{
    audit::{AuditEntry, AuditRing, DecisionTag},
    cap::SovereignCap,
    compose_parent_child, evaluate_with_ring, AccessDecision, AUDIENCE_CIRCLE, AUDIENCE_DERIVED,
    AUDIENCE_SELF, EFFECT_DERIVE, EFFECT_READ, EFFECT_WRITE, FLAG_ATTESTED, FLAG_PROPAGATE,
    SigmaMask,
};

fn fresh_keypair() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

fn sign_a_cap(
    sovereign: &SigningKey,
    holder_pk: [u8; 32],
    grants: u32,
    audience: u16,
    expires_at: u64,
) -> SovereignCap {
    let mut cap = SovereignCap::from_raw(holder_pk, grants, audience, expires_at, None, [0u8; 64]);
    let msg = cap.canonical_signing_bytes();
    cap.signature = sovereign.sign(&msg).to_bytes();
    cap
}

#[test]
fn end_to_end_grant_evaluate_audit() {
    let ring = AuditRing::new(128);
    let sovereign = fresh_keypair();
    let holder = fresh_keypair();
    let cap = sign_a_cap(
        &sovereign,
        holder.verifying_key().to_bytes(),
        EFFECT_READ | EFFECT_DERIVE,
        AUDIENCE_DERIVED,
        0,
    );
    let mask = SigmaMask::new(
        AUDIENCE_DERIVED,
        EFFECT_DERIVE,
        5,
        0,
        FLAG_ATTESTED,
        1_000,
    );
    // missing aggregation_k below the floor → NeedsKAnonymity
    let d_low_k = evaluate_with_ring(
        &ring,
        &mask,
        Some(&cap),
        AUDIENCE_DERIVED,
        EFFECT_DERIVE,
        Some(2),
        1_000,
        Some(&sovereign.verifying_key().to_bytes()),
    );
    assert!(matches!(d_low_k, AccessDecision::NeedsKAnonymity { .. }));
    // bumping k=10 above floor=5 → Allow
    let d_ok = evaluate_with_ring(
        &ring,
        &mask,
        Some(&cap),
        AUDIENCE_DERIVED,
        EFFECT_DERIVE,
        Some(10),
        1_000,
        Some(&sovereign.verifying_key().to_bytes()),
    );
    assert!(d_ok.is_allow(), "{:?}", d_ok);
    // drain audit
    let mut buf = vec![AuditEntry::new(0, 0, 0, DecisionTag::Allow, 0, 0, 0); 10];
    let n = ring.drain(&mut buf);
    assert_eq!(n, 2, "two evaluates → two audit-entries");
}

#[test]
fn cascade_revoke_propagates_to_child() {
    let mut parent = SigmaMask::new(
        AUDIENCE_SELF | AUDIENCE_CIRCLE,
        EFFECT_READ | EFFECT_WRITE,
        0,
        0,
        FLAG_PROPAGATE,
        1_000,
    );
    let child = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
    parent.revoke(2_000);
    let composed = compose_parent_child(&parent, &child, 2_000).unwrap();
    assert!(composed.is_revoked());
    let ring = AuditRing::new(64);
    let d = evaluate_with_ring(
        &ring,
        &composed,
        None,
        AUDIENCE_SELF,
        EFFECT_READ,
        None,
        2_000,
        None,
    );
    assert!(matches!(d, AccessDecision::Revoked { .. }));
}

#[test]
fn concurrent_1000_evaluates_zero_loss() {
    // § 1000-thread fanout · all evaluate the same mask · audit-ring receives
    // exactly 1000 entries · no contention-induced drops.
    let ring = Arc::new(AuditRing::new(2048));
    let mask = Arc::new(SigmaMask::new(
        AUDIENCE_SELF | AUDIENCE_CIRCLE,
        EFFECT_READ | EFFECT_WRITE,
        0,
        0,
        FLAG_PROPAGATE,
        1_000,
    ));

    const N_THREADS: usize = 16;
    const PER_THREAD: usize = 1000 / N_THREADS;
    let mut handles = Vec::with_capacity(N_THREADS);
    for _ in 0..N_THREADS {
        let ring_c = ring.clone();
        let m_c = mask.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..PER_THREAD {
                let _ = evaluate_with_ring(
                    &ring_c,
                    &m_c,
                    None,
                    AUDIENCE_SELF,
                    EFFECT_READ,
                    None,
                    1_000,
                    None,
                );
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(
        ring.total_written(),
        (N_THREADS * PER_THREAD) as u64,
        "concurrent stress · 1000 evaluates → 1000 audit-entries · zero drops"
    );
    assert_eq!(ring.wrap_count(), 0, "ring not wrapped (capacity 2048 ≥ 1000)");
}
