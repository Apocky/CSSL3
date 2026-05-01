// § integration-tests · cssl-host-akashic-records
#![allow(clippy::manual_let_else)] // explicit `match` on PurchaseOutcome reads clearer in tests
// § ≥ 32 tests covering :
//   - 5-tier construction (5)
//   - basic-imprint-free (2)
//   - shard-deduct-correct (3)
//   - insufficient-shards rejection (2)
//   - eternal-one-time enforcement (3)
//   - cosmetic-only-axiom-guard (3)
//   - BLAKE3-hash-stable (2)
//   - browse-query-by-scene (2)
//   - browse-by-author (2)
//   - browse-by-fidelity (2)
//   - revoked-filtered (1)
//   - serde round-trip (2)
//   - author-attribution permanence (1)
//   - shards-checked-arithmetic (1)
//   - 16-band-flag toggle (1)
//   total = 32 ; bonus tests for thoroughness

use cssl_host_akashic_records::{
    assert_cosmetic_only, AethericShards, AkashicLedger, AuthorPubkey, BrowseQuery, FidelityTier,
    Imprint, ImprintId, ImprintState, PurchaseOutcome, PurchaseRequest, RevokedReason, SceneMeta,
    ShardCostConfig,
};

fn pk(b: u8) -> AuthorPubkey {
    AuthorPubkey::new([b; 32])
}

fn meta(scene: &str) -> SceneMeta {
    SceneMeta {
        scene_name: scene.to_owned(),
        location: "Verdant-Spire".to_owned(),
        runeset: "rune-bundle-spec18".to_owned(),
        spectral_16band_rendered: false,
        audio_loop: false,
    }
}

fn ledger() -> AkashicLedger {
    AkashicLedger::new(ShardCostConfig::default())
}

// ═══════ 5-tier construction (5) ═══════

#[test]
fn t01_construct_basic() {
    let mut l = ledger();
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 100,
        scene_metadata: meta("first-deed"),
        commissioned_narration: None,
    });
    assert!(matches!(out, PurchaseOutcome::Granted { .. }));
}

#[test]
fn t02_construct_high_fidelity() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(50));
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 100,
        scene_metadata: meta("scene-A"),
        commissioned_narration: None,
    });
    assert!(matches!(out, PurchaseOutcome::Granted { .. }));
}

#[test]
fn t03_construct_commissioned() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(200));
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Commissioned,
        ts: 100,
        scene_metadata: meta("scene-B"),
        commissioned_narration: Some("the hollow crowned itself".into()),
    });
    if let PurchaseOutcome::Granted { imprint, .. } = out {
        assert_eq!(imprint.fidelity, FidelityTier::Commissioned);
        assert!(imprint.commissioned_narration.is_some());
    } else {
        panic!("expected Granted, got {out:?}");
    }
}

#[test]
fn t04_construct_eternal_attribution() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(1000));
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 100,
        scene_metadata: meta("scene-eternal"),
        commissioned_narration: None,
    });
    if let PurchaseOutcome::Granted { imprint, .. } = out {
        assert!(imprint.eternal);
        assert_eq!(imprint.shard_cost, 1000);
    } else {
        panic!("expected Granted");
    }
}

#[test]
fn t05_construct_historical_tour() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(50));
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HistoricalReconstructionTour,
        ts: 1000,
        scene_metadata: meta("tour-A"),
        commissioned_narration: None,
    });
    if let PurchaseOutcome::Granted { imprint, .. } = out {
        let token = imprint.ttl_token.expect("tour must issue token");
        assert_eq!(token.issued_at, 1000);
        assert_eq!(token.expires_at, 1000 + 30 * 60);
    } else {
        panic!("expected Granted");
    }
}

// ═══════ basic-imprint-free (2) ═══════

#[test]
fn t06_basic_imprint_is_free_no_balance_required() {
    let mut l = ledger();
    // Note: balance defaults to 0 ; Basic must still succeed
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 0,
        scene_metadata: meta("free-deed"),
        commissioned_narration: None,
    });
    if let PurchaseOutcome::Granted {
        imprint,
        new_balance,
    } = out
    {
        assert_eq!(imprint.shard_cost, 0);
        assert_eq!(new_balance, AethericShards::ZERO);
    } else {
        panic!("Basic must always succeed");
    }
}

#[test]
fn t07_basic_imprint_balance_unchanged() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(123));
    l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 0,
        scene_metadata: meta("free-deed"),
        commissioned_narration: None,
    });
    assert_eq!(l.balance(&pk(1)), AethericShards::new(123));
}

// ═══════ shard-deduct-correct (3) ═══════

#[test]
fn t08_shard_deduct_high_fidelity() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(100));
    l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 0,
        scene_metadata: meta("hi-fi"),
        commissioned_narration: None,
    });
    assert_eq!(l.balance(&pk(1)), AethericShards::new(50));
}

#[test]
fn t09_shard_deduct_eternal() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(2000));
    l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 0,
        scene_metadata: meta("eternal-1"),
        commissioned_narration: None,
    });
    assert_eq!(l.balance(&pk(1)), AethericShards::new(1000));
}

#[test]
fn t10_shard_deduct_multiple_imprints() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(300));
    for s in ["a", "b", "c"] {
        l.imprint(PurchaseRequest {
            author: pk(1),
            fidelity: FidelityTier::HighFidelity,
            ts: 0,
            scene_metadata: meta(s),
            commissioned_narration: None,
        });
    }
    // 300 - 3*50 = 150
    assert_eq!(l.balance(&pk(1)), AethericShards::new(150));
}

// ═══════ insufficient-shards rejection (2) ═══════

#[test]
fn t11_insufficient_shards_rejection_high_fidelity() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(10));
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 0,
        scene_metadata: meta("nope"),
        commissioned_narration: None,
    });
    assert!(matches!(
        out,
        PurchaseOutcome::InsufficientShards {
            have: 10,
            need: 50
        }
    ));
    // Balance unchanged
    assert_eq!(l.balance(&pk(1)), AethericShards::new(10));
    // No imprint stored
    assert_eq!(l.imprint_count(), 0);
}

#[test]
fn t12_insufficient_shards_zero_balance_eternal() {
    let mut l = ledger();
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 0,
        scene_metadata: meta("dream-eternal"),
        commissioned_narration: None,
    });
    assert!(matches!(
        out,
        PurchaseOutcome::InsufficientShards {
            have: 0,
            need: 1000
        }
    ));
}

// ═══════ eternal-one-time enforcement (3) ═══════

#[test]
fn t13_eternal_one_time_same_author_same_scene() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(5000));
    // First claim succeeds
    let _ = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 0,
        scene_metadata: meta("the-coronation"),
        commissioned_narration: None,
    });
    // Second claim must reject
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 1,
        scene_metadata: meta("the-coronation"),
        commissioned_narration: None,
    });
    assert!(matches!(out, PurchaseOutcome::AlreadyOwnedEternal { .. }));
    // Balance only deducted once
    assert_eq!(l.balance(&pk(1)), AethericShards::new(4000));
}

#[test]
fn t14_eternal_different_scenes_both_succeed() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(2500));
    let a = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 0,
        scene_metadata: meta("scene-eternal-A"),
        commissioned_narration: None,
    });
    let b = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 1,
        scene_metadata: meta("scene-eternal-B"),
        commissioned_narration: None,
    });
    assert!(matches!(a, PurchaseOutcome::Granted { .. }));
    assert!(matches!(b, PurchaseOutcome::Granted { .. }));
    assert_eq!(l.balance(&pk(1)), AethericShards::new(500));
}

#[test]
fn t15_eternal_different_authors_same_scene_both_succeed() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(1000));
    l.set_balance(pk(2), AethericShards::new(1000));
    let a = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 0,
        scene_metadata: meta("the-tournament"),
        commissioned_narration: None,
    });
    let b = l.imprint(PurchaseRequest {
        author: pk(2),
        fidelity: FidelityTier::EternalAttribution,
        ts: 1,
        scene_metadata: meta("the-tournament"),
        commissioned_narration: None,
    });
    assert!(matches!(a, PurchaseOutcome::Granted { .. }));
    assert!(matches!(b, PurchaseOutcome::Granted { .. }));
}

// ═══════ cosmetic-only-axiom-guard (3) ═══════

#[test]
fn t16_cosmetic_axiom_rejects_oversized_scene_name() {
    let mut l = ledger();
    let mut m = meta("ok");
    m.scene_name = "x".repeat(SceneMeta::MAX_STRING_BYTES + 5);
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 0,
        scene_metadata: m,
        commissioned_narration: None,
    });
    assert!(matches!(
        out,
        PurchaseOutcome::CosmeticAxiomViolation { .. }
    ));
}

#[test]
fn t17_cosmetic_axiom_rejects_control_bytes() {
    let mut l = ledger();
    let mut m = meta("ok");
    m.location = "bad\x01control".to_string();
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 0,
        scene_metadata: m,
        commissioned_narration: None,
    });
    assert!(matches!(
        out,
        PurchaseOutcome::CosmeticAxiomViolation { .. }
    ));
}

#[test]
fn t18_cosmetic_axiom_eternal_never_revoked() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(1000));
    let granted = match l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 0,
        scene_metadata: meta("eternal-revoke-test"),
        commissioned_narration: None,
    }) {
        PurchaseOutcome::Granted { imprint, .. } => imprint,
        _ => panic!("must grant"),
    };
    let err = l
        .revoke(granted.id, RevokedReason::AuthorRequested)
        .unwrap_err();
    // Must NOT mutate state
    let still = l.get(granted.id).unwrap();
    assert!(matches!(still.state, ImprintState::Permanent));
    assert!(format!("{err}").contains("NEVER"));
}

// ═══════ BLAKE3-hash-stable (2) ═══════

#[test]
fn t19_blake3_deterministic_same_inputs() {
    let m = meta("scene-x");
    let h1 = Imprint::compute_content_hash(&m, &pk(1), 12345);
    let h2 = Imprint::compute_content_hash(&m, &pk(1), 12345);
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 32);
}

#[test]
fn t20_blake3_changes_with_any_field() {
    let m = meta("scene-x");
    let base = Imprint::compute_content_hash(&m, &pk(1), 100);
    assert_ne!(base, Imprint::compute_content_hash(&m, &pk(2), 100));
    assert_ne!(base, Imprint::compute_content_hash(&m, &pk(1), 101));
    assert_ne!(
        base,
        Imprint::compute_content_hash(&meta("scene-y"), &pk(1), 100)
    );
}

// ═══════ browse-query-by-scene (2) ═══════

fn populated_ledger() -> AkashicLedger {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(2000));
    l.set_balance(pk(2), AethericShards::new(2000));
    // Author 1: scene-A Basic, scene-B HighFidelity, scene-C Eternal
    let _ = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 1,
        scene_metadata: meta("scene-A"),
        commissioned_narration: None,
    });
    let _ = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 2,
        scene_metadata: meta("scene-B"),
        commissioned_narration: None,
    });
    let _ = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 3,
        scene_metadata: meta("scene-C"),
        commissioned_narration: None,
    });
    // Author 2: scene-A Basic
    let _ = l.imprint(PurchaseRequest {
        author: pk(2),
        fidelity: FidelityTier::Basic,
        ts: 4,
        scene_metadata: meta("scene-A"),
        commissioned_narration: None,
    });
    l
}

#[test]
fn t21_browse_by_scene_name_matches_both_authors() {
    let l = populated_ledger();
    let q = BrowseQuery::new().with_scene_name("scene-A");
    let r = cssl_host_akashic_records::browse::browse(&l, &q);
    assert_eq!(r.count(), 2);
}

#[test]
fn t22_browse_by_scene_name_no_matches() {
    let l = populated_ledger();
    let q = BrowseQuery::new().with_scene_name("nonexistent");
    let r = cssl_host_akashic_records::browse::browse(&l, &q);
    assert_eq!(r.count(), 0);
}

// ═══════ browse-by-author (2) ═══════

#[test]
fn t23_browse_by_author_filters() {
    let l = populated_ledger();
    let q = BrowseQuery::new().with_author(pk(1));
    let r = cssl_host_akashic_records::browse::browse(&l, &q);
    assert_eq!(r.count(), 3);
}

#[test]
fn t24_browse_by_author_unknown_returns_empty() {
    let l = populated_ledger();
    let q = BrowseQuery::new().with_author(pk(99));
    let r = cssl_host_akashic_records::browse::browse(&l, &q);
    assert_eq!(r.count(), 0);
}

// ═══════ browse-by-fidelity (2) ═══════

#[test]
fn t25_browse_by_fidelity_min_high_includes_eternal() {
    let l = populated_ledger();
    let q = BrowseQuery::new().with_fidelity_min(FidelityTier::HighFidelity);
    let r = cssl_host_akashic_records::browse::browse(&l, &q);
    // HighFidelity (1) + Commissioned (2) + Eternal (3) + Tour (4)
    // populated has scene-B HighFid + scene-C Eternal = 2
    assert_eq!(r.count(), 2);
}

#[test]
fn t26_browse_by_fidelity_min_eternal_only() {
    let l = populated_ledger();
    let q = BrowseQuery::new().with_fidelity_min(FidelityTier::EternalAttribution);
    let r = cssl_host_akashic_records::browse::browse(&l, &q);
    assert_eq!(r.count(), 1);
}

// ═══════ revoked-filtered (1) ═══════

#[test]
fn t27_revoked_filtered_from_browse() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(50));
    let granted = match l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 0,
        scene_metadata: meta("doomed"),
        commissioned_narration: None,
    }) {
        PurchaseOutcome::Granted { imprint, .. } => imprint,
        _ => panic!("must grant"),
    };
    // Visible before revoke
    let q = BrowseQuery::new().with_scene_name("doomed");
    assert_eq!(cssl_host_akashic_records::browse::browse(&l, &q).count(), 1);
    // Revoke
    l.revoke(granted.id, RevokedReason::PolicyViolation).unwrap();
    // Filtered after revoke
    assert_eq!(cssl_host_akashic_records::browse::browse(&l, &q).count(), 0);
}

// ═══════ serde round-trip (2) ═══════

#[test]
fn t28_serde_roundtrip_imprint() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(50));
    let granted = match l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 42,
        scene_metadata: meta("for-serde"),
        commissioned_narration: None,
    }) {
        PurchaseOutcome::Granted { imprint, .. } => imprint,
        _ => panic!(),
    };
    let json = serde_json::to_string(&granted).unwrap();
    let back: Imprint = serde_json::from_str(&json).unwrap();
    assert_eq!(back, granted);
}

#[test]
fn t29_serde_roundtrip_ledger() {
    let l = populated_ledger();
    let json = serde_json::to_string(&l).unwrap();
    let back: AkashicLedger = serde_json::from_str(&json).unwrap();
    assert_eq!(l.imprint_count(), back.imprint_count());
    assert_eq!(l.balance(&pk(1)), back.balance(&pk(1)));
}

// ═══════ author-attribution permanence (1) ═══════

#[test]
fn t30_author_attribution_permanence() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(1000));
    let granted = match l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 0,
        scene_metadata: meta("immortal"),
        commissioned_narration: None,
    }) {
        PurchaseOutcome::Granted { imprint, .. } => imprint,
        _ => panic!(),
    };
    // The author stays attached t∞
    let stored = l.get(granted.id).unwrap();
    assert_eq!(stored.author_pubkey, pk(1));
    assert!(stored.eternal);
    // Even if we attempt revoke, attribution survives
    let _ = l.revoke(granted.id, RevokedReason::AuthorRequested);
    let still = l.get(granted.id).unwrap();
    assert_eq!(still.author_pubkey, pk(1));
}

// ═══════ shards-checked-arithmetic (1) ═══════

#[test]
fn t31_shards_checked_arithmetic_no_panic_on_overflow() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(u64::MAX));
    let err = l.credit(pk(1), AethericShards::new(1)).unwrap_err();
    assert_eq!(err.to_string(), "shard balance overflow");
}

// ═══════ 16-band-flag toggle (1) ═══════

#[test]
fn t32_16_band_spectral_flag_round_trip() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(50));
    let mut m = meta("spectral");
    m.spectral_16band_rendered = true;
    m.audio_loop = true;
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 0,
        scene_metadata: m,
        commissioned_narration: None,
    });
    if let PurchaseOutcome::Granted { imprint, .. } = out {
        assert!(imprint.scene_metadata.spectral_16band_rendered);
        assert!(imprint.scene_metadata.audio_loop);
        // Hash must differ vs flag-off
        let m_off = meta("spectral");
        let h_off = Imprint::compute_content_hash(&m_off, &pk(1), 0);
        assert_ne!(h_off, imprint.content_blake3);
    } else {
        panic!();
    }
}

// ═══════ bonus thoroughness tests ═══════

#[test]
fn t33_audit_emits_basic_imprint_free() {
    let mut l = ledger();
    l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 0,
        scene_metadata: meta("free-audit"),
        commissioned_narration: None,
    });
    let trail = l.audit_trail();
    assert!(matches!(
        trail.first().unwrap(),
        cssl_host_akashic_records::AuditEvent::BasicImprintFree { .. }
    ));
}

#[test]
fn t34_audit_emits_shards_deducted() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(50));
    l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 0,
        scene_metadata: meta("paid-audit"),
        commissioned_narration: None,
    });
    let has_deduct = l.audit_trail().iter().any(|e| matches!(
        e,
        cssl_host_akashic_records::AuditEvent::ShardsDeducted { .. }
    ));
    assert!(has_deduct);
}

#[test]
fn t35_audit_emits_eternal_attribution_claimed() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(1000));
    l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::EternalAttribution,
        ts: 0,
        scene_metadata: meta("audited-eternal"),
        commissioned_narration: None,
    });
    let has_eternal = l.audit_trail().iter().any(|e| matches!(
        e,
        cssl_host_akashic_records::AuditEvent::EternalAttributionClaimed { .. }
    ));
    assert!(has_eternal);
}

#[test]
fn t36_commissioned_missing_narration_rejected() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(500));
    let out = l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Commissioned,
        ts: 0,
        scene_metadata: meta("no-narration"),
        commissioned_narration: None,
    });
    assert!(matches!(out, PurchaseOutcome::MissingNarration));
    // No deduction
    assert_eq!(l.balance(&pk(1)), AethericShards::new(500));
}

#[test]
fn t37_assert_cosmetic_only_passes_for_valid_imprint() {
    let mut l = ledger();
    l.set_balance(pk(1), AethericShards::new(50));
    let granted = match l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::HighFidelity,
        ts: 0,
        scene_metadata: meta("axiom-pass"),
        commissioned_narration: None,
    }) {
        PurchaseOutcome::Granted { imprint, .. } => imprint,
        _ => panic!(),
    };
    assert_cosmetic_only(&granted).expect("valid imprint passes guard");
}

#[test]
fn t38_imprint_id_monotonic() {
    let mut l = ledger();
    let a = match l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 0,
        scene_metadata: meta("a"),
        commissioned_narration: None,
    }) {
        PurchaseOutcome::Granted { imprint, .. } => imprint.id,
        _ => panic!(),
    };
    let b = match l.imprint(PurchaseRequest {
        author: pk(1),
        fidelity: FidelityTier::Basic,
        ts: 0,
        scene_metadata: meta("b"),
        commissioned_narration: None,
    }) {
        PurchaseOutcome::Granted { imprint, .. } => imprint.id,
        _ => panic!(),
    };
    assert!(b.raw() > a.raw());
}

#[test]
fn t39_revoke_unknown_imprint_errors() {
    let mut l = ledger();
    let err = l
        .revoke(ImprintId::new(99999), RevokedReason::AuthorRequested)
        .unwrap_err();
    assert!(format!("{err}").contains("unknown"));
}

#[test]
fn t40_browse_combined_filters() {
    let l = populated_ledger();
    let q = BrowseQuery::new()
        .with_author(pk(1))
        .with_fidelity_min(FidelityTier::HighFidelity);
    let r = cssl_host_akashic_records::browse::browse(&l, &q);
    // Author 1 has HighFid + Eternal = 2
    assert_eq!(r.count(), 2);
}
