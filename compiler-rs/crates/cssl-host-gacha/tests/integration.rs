// § integration.rs — End-to-end gacha flows : pull → anchor → refund → anchor
// ════════════════════════════════════════════════════════════════════
// § COVERS :
//   1. cosmetic-only-attestation : pull → cosmetic-handle starts with "cosmetic:"
//   2. pity-within-90-pulls : 90-pull-trail produces ≥1 Mythic
//   3. 7-day-refund-roundtrip : pull → refund within window → cosmetic removed
//   4. Σ-Chain-anchor every-pull : pull-anchor + refund-anchor distinct anchor-id
//   5. distribution-100k-trials : matches disclosed-rates within tolerance (in unit-tests)
//   6. predatory-pattern attestations : all-10 upheld in canonical build
//   7. cross-banner replay determinism
//   8. ten-pull bundle uniformity
//   9. refund-rollback updates pity counter
//  10. transparency-mandate : banner not pullable until disclosed_at set
// ════════════════════════════════════════════════════════════════════

use cssl_host_gacha::{
    canonical_drop_rates, run_pull, run_refund, AttestationFlags, Banner, PityCounter, PullMode,
    PullRequest, Rarity, RefundRequest, SigmaAnchor, ATTESTATIONS_COUNT, PITY_THRESHOLD,
    REFUND_WINDOW_SECS,
};

fn pubkey() -> Vec<u8> {
    b"integration-32-byte-pubkey-fix!!".to_vec()
}

fn fresh_banner(id: &str) -> Banner {
    Banner::canonical(id.to_string(), 1, "2026-05-01T00:00:00Z".into()).unwrap()
}

#[test]
fn integration_pull_returns_cosmetic_only_handle() {
    let banner = fresh_banner("integ-A");
    let req = PullRequest {
        player_pubkey: pubkey(),
        banner_id: "integ-A".into(),
        starting_pull_index: 0,
        mode: PullMode::Single,
        pity_in: PityCounter::new(),
    };
    let res = run_pull(&req, &banner).unwrap();
    assert_eq!(res.outcomes.len(), 1);
    let h = &res.outcomes[0].cosmetic_handle;
    assert!(
        h.starts_with("cosmetic:"),
        "cosmetic-only axiom : handle MUST start with 'cosmetic:' — got {h}"
    );
}

#[test]
fn integration_pity_guarantees_mythic_within_90() {
    let banner = fresh_banner("integ-B");
    let mut pity = PityCounter::new();
    let mut found = false;
    for i in 0..PITY_THRESHOLD {
        let req = PullRequest {
            player_pubkey: pubkey(),
            banner_id: "integ-B".into(),
            starting_pull_index: u64::from(i),
            mode: PullMode::Single,
            pity_in: pity.clone(),
        };
        let res = run_pull(&req, &banner).unwrap();
        if res.outcomes[0].rarity == Rarity::Mythic {
            found = true;
            break;
        }
        pity = res.pity_out;
    }
    assert!(
        found,
        "pity-system MUST guarantee a Mythic within {PITY_THRESHOLD} pulls"
    );
}

#[test]
fn integration_full_refund_roundtrip() {
    let banner = fresh_banner("integ-C");
    // 1. Pull a Single.
    let pull_req = PullRequest {
        player_pubkey: pubkey(),
        banner_id: "integ-C".into(),
        starting_pull_index: 0,
        mode: PullMode::Single,
        pity_in: PityCounter::new(),
    };
    let pull_res = run_pull(&pull_req, &banner).unwrap();
    let outcome = &pull_res.outcomes[0];

    // 2. Anchor the pull on Σ-Chain.
    let pull_anchor = SigmaAnchor::pull_anchor(
        &pull_req.player_pubkey,
        &pull_req.banner_id,
        "pull-id-001",
        1_700_000_000,
        serde_json::to_string(outcome).unwrap(),
    )
    .unwrap();

    // 3. Within window (1 day later) refund.
    let refund_req = RefundRequest {
        player_pubkey: pull_req.player_pubkey.clone(),
        pull_id: "pull-id-001".into(),
        banner_id: pull_req.banner_id.clone(),
        cosmetic_handle: outcome.cosmetic_handle.clone(),
        rarity_at_pull: outcome.rarity,
        pull_ts_epoch_secs: 1_700_000_000,
        now_epoch_secs: 1_700_000_000 + 24 * 60 * 60,
        pity_after_pull: pull_res.pity_out.clone(),
    };
    let refund_res = run_refund(&refund_req).unwrap();
    assert!(refund_res.refunded);
    assert_eq!(refund_res.removed_cosmetic_handle, outcome.cosmetic_handle);

    // 4. Anchor the refund.
    let refund_anchor = SigmaAnchor::refund_anchor(
        &refund_req.player_pubkey,
        &refund_req.banner_id,
        "pull-id-001",
        refund_req.now_epoch_secs,
        serde_json::to_string(&refund_res).unwrap(),
    )
    .unwrap();

    // 5. Pull-anchor and refund-anchor MUST have different anchor-id (kind tag).
    assert_ne!(pull_anchor.anchor_id_hex, refund_anchor.anchor_id_hex);
}

#[test]
fn integration_refund_outside_7d_rejects() {
    let req = RefundRequest {
        player_pubkey: pubkey(),
        pull_id: "pull-id-002".into(),
        banner_id: "integ-D".into(),
        cosmetic_handle: "cosmetic:rare:integ-D:001".into(),
        rarity_at_pull: Rarity::Rare,
        pull_ts_epoch_secs: 1_700_000_000,
        now_epoch_secs: 1_700_000_000 + REFUND_WINDOW_SECS + 1,
        pity_after_pull: PityCounter::new(),
    };
    assert!(run_refund(&req).is_err());
}

#[test]
fn integration_attestations_all_upheld_in_canonical_build() {
    let f = AttestationFlags::all_upheld();
    assert_eq!(f.upheld_count(), ATTESTATIONS_COUNT);
    assert!(f.is_prime_directive_compliant());
    assert!(ATTESTATIONS_COUNT >= 10);
}

#[test]
fn integration_canonical_drop_rates_publicly_match_spec() {
    use canonical_drop_rates::*;
    assert_eq!(COMMON_BPS, 60_000);
    assert_eq!(UNCOMMON_BPS, 25_000);
    assert_eq!(RARE_BPS, 10_000);
    assert_eq!(EPIC_BPS, 4_000);
    assert_eq!(LEGENDARY_BPS, 900);
    assert_eq!(MYTHIC_BPS, 100);
    assert_eq!(SUM_INVARIANT, TOTAL_BPS);
}

#[test]
fn integration_replay_determinism_across_banners() {
    let banner_a = fresh_banner("integ-E1");
    let banner_b = fresh_banner("integ-E2");
    let pk = pubkey();
    let req_a = PullRequest {
        player_pubkey: pk.clone(),
        banner_id: "integ-E1".into(),
        starting_pull_index: 7,
        mode: PullMode::Single,
        pity_in: PityCounter::new(),
    };
    let req_b = PullRequest {
        player_pubkey: pk,
        banner_id: "integ-E2".into(),
        starting_pull_index: 7,
        mode: PullMode::Single,
        pity_in: PityCounter::new(),
    };
    let r_a = run_pull(&req_a, &banner_a).unwrap();
    let r_b = run_pull(&req_b, &banner_b).unwrap();
    // Distinct banners ⇒ distinct seed-derivation ⇒ may produce distinct rolls.
    // We assert the Σ-Chain ANCHOR-id differs (banner_id is part of the hash).
    let anchor_a = SigmaAnchor::pull_anchor(
        &req_a.player_pubkey,
        &req_a.banner_id,
        "pull-001",
        1,
        serde_json::to_string(&r_a.outcomes[0]).unwrap(),
    )
    .unwrap();
    let anchor_b = SigmaAnchor::pull_anchor(
        &req_b.player_pubkey,
        &req_b.banner_id,
        "pull-001",
        1,
        serde_json::to_string(&r_b.outcomes[0]).unwrap(),
    )
    .unwrap();
    assert_ne!(anchor_a.anchor_id_hex, anchor_b.anchor_id_hex);
}

#[test]
fn integration_ten_pull_bundle_consistency() {
    let banner = fresh_banner("integ-F");
    let req = PullRequest {
        player_pubkey: pubkey(),
        banner_id: "integ-F".into(),
        starting_pull_index: 0,
        mode: PullMode::TenPull,
        pity_in: PityCounter::new(),
    };
    let res = run_pull(&req, &banner).unwrap();
    assert_eq!(res.outcomes.len(), 11); // 10 + bonus
    // All outcomes must have unique pull_index in this bundle.
    let indices: std::collections::BTreeSet<_> =
        res.outcomes.iter().map(|o| o.pull_index).collect();
    assert_eq!(indices.len(), 11);
    // Histogram sum must equal total rolls.
    let sum: u32 = res.rarity_histogram.values().sum();
    assert_eq!(sum, 11);
}

#[test]
fn integration_refund_rolls_back_pity() {
    let mut pity = PityCounter::new();
    pity.pulls_since_mythic = 10;

    let req = RefundRequest {
        player_pubkey: pubkey(),
        pull_id: "pull-id-003".into(),
        banner_id: "integ-G".into(),
        cosmetic_handle: "cosmetic:common:integ-G:050".into(),
        rarity_at_pull: Rarity::Common,
        pull_ts_epoch_secs: 1_700_000_000,
        now_epoch_secs: 1_700_000_000 + 60,
        pity_after_pull: pity,
    };
    let res = run_refund(&req).unwrap();
    // Non-mythic refund decrements pity-counter by 1.
    assert_eq!(res.pity_after_refund.pulls_since_mythic, 9);
}

#[test]
fn integration_banner_not_disclosed_blocks_pull() {
    let mut banner = fresh_banner("integ-H");
    banner.disclosed_at = None;
    let req = PullRequest {
        player_pubkey: pubkey(),
        banner_id: "integ-H".into(),
        starting_pull_index: 0,
        mode: PullMode::Single,
        pity_in: PityCounter::new(),
    };
    assert!(run_pull(&req, &banner).is_err());
}
