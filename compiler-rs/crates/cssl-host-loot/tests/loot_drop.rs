// § T11-W13-LOOT-TESTS — integration tests per W13-8 spec (Q-06 8-tier)
// ════════════════════════════════════════════════════════════════════
// Q-06 Apocky-canonical 2026-05-01 : 6-tier → 8-tier extension
// Required (≥10) :
//   1. drop-rate-distribution-100k-trials (8-tier sum-check)
//   2. cosmetic-only-attestation-attest-all
//   3. KAN-bias-influences-distribution (8-dim vector)
//   4. cap-deny-without-grant
//   5. Σ-Chain-anchor-roundtrip
//   6. pay-for-power rejected-at-load (affix-count-overflow)
//   7. determinism (same seed → bit-equal item)
//   8. public-rates-sum-to-one (Q-06 8-tier)
//   9. bias-clamp-prevents-runaway
//  10. consent-token-zero-rejected
//  11. rarity-affix-count-band (Chaotic ships richer than Common)
//  12. canonical-bytes round-trip stable
//  13. Q-06 8-tier rarity-set complete (Prismatic + Chaotic present)

use cssl_host_gear_archetype::Rarity;
use cssl_host_loot::{
    anchor_drop_to_sigma_chain, attest_no_pay_for_power,
    attest::{attest_no_pay_for_power_strict, AFFIX_COUNT_HARD_CAP},
    bias::{KanBiasConsent, KanBiasVector, MAX_BIAS_DELTA},
    distribution::{DropRateDistribution, PUBLIC_DROP_RATES},
    item::{LootItem, LootSeason},
    roll::{roll_loot, roll_loot_with_bias, sample_rarity_with_bias, LootContext},
    LootAffix, LootDropEvent, PayForPowerError,
};
use cssl_host_sigma_chain::{verify_event, EventKind, SigmaLedger, VerifyOutcome};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;

// ───────────────────────────────────────────────────────────────────────
// § Test 1 — drop-rate distribution over 100k trials
// ───────────────────────────────────────────────────────────────────────

#[test]
fn drop_rate_distribution_100k_trials() {
    // Q-06 8-tier : tolerances widened for ultra-rare tiers (Prismatic + Chaotic
    // expected ~9 / ~1 hits in 100k samples — variance dominates).
    const N: u32 = 100_000;

    let dist = DropRateDistribution::PUBLIC;
    let consent = KanBiasConsent::denied();
    let bias = KanBiasVector::zero();

    let mut counts = [0_u32; 8];
    for i in 0..N {
        // Vary seed widely so we sample the distribution rather than one path.
        let seed = (u128::from(i)).wrapping_mul(0x9E37_79B9_7F4A_7C15_u128)
            ^ 0xDEAD_BEEF_BADF_00D0_u128;
        let r = sample_rarity_with_bias(&dist, &bias, &consent, seed);
        let idx = DropRateDistribution::index_of(r);
        counts[idx] += 1;
    }

    let observed = [
        f64::from(counts[0]) / f64::from(N),
        f64::from(counts[1]) / f64::from(N),
        f64::from(counts[2]) / f64::from(N),
        f64::from(counts[3]) / f64::from(N),
        f64::from(counts[4]) / f64::from(N),
        f64::from(counts[5]) / f64::from(N),
        f64::from(counts[6]) / f64::from(N),
        f64::from(counts[7]) / f64::from(N),
    ];

    // Tolerance windows (wide for ultra-rare tiers ; Prismatic+Chaotic loose ∵ tiny-N).
    let expected = PUBLIC_DROP_RATES;
    let tol = [
        0.02_f64,  // Common 60%
        0.02,      // Uncommon 25%
        0.015,     // Rare 10%
        0.01,      // Epic 4%
        0.005,     // Legendary 0.9%
        0.005,     // Mythic 0.09% (Q-06)
        0.0005,    // Prismatic 0.009% (Q-06 NEW · ~9 hits expected · ±5)
        0.0001,    // Chaotic 0.001% (Q-06 NEW · ~1 hit expected · loose)
    ];

    for i in 0..8 {
        let exp = f64::from(expected[i]);
        let delta = (observed[i] - exp).abs();
        let obs_i = observed[i];
        let tol_i = tol[i];
        assert!(
            delta <= tol[i],
            "tier {i} expected {exp:.6} got {obs_i:.6} (delta {delta:.6} > tol {tol_i:.6})"
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Test 2 — cosmetic-only attestation passes for ALL shipped items
// ───────────────────────────────────────────────────────────────────────

#[test]
fn cosmetic_only_attestation_attest_all() {
    let dist = DropRateDistribution::PUBLIC;
    let consent = KanBiasConsent::denied();
    let ctx = LootContext::default_for_combat_end();

    for i in 0..10_000_u128 {
        let item = roll_loot(&dist, &consent, &ctx, i.wrapping_mul(0xA51F_3C7B));
        assert!(
            attest_no_pay_for_power(&item),
            "shipped item failed attestation : {item:?}"
        );
        // Strict variant returns Ok.
        attest_no_pay_for_power_strict(&item).expect("strict attest must pass");
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Test 3 — KAN-bias influences distribution (under consent)
// ───────────────────────────────────────────────────────────────────────

#[test]
fn kan_bias_influences_distribution() {
    // Q-06 8-tier : KanBiasVector now has 8 weight entries.
    let base = DropRateDistribution::PUBLIC;
    let consent = KanBiasConsent::granted(0xDEAD_BEEF);
    // Bias toward Rare/Epic (positive deltas for indices 2, 3).
    let bias = KanBiasVector::new([-0.05, -0.02, 0.05, 0.05, 0.0, 0.0, 0.0, 0.0]);

    let modulated = bias.apply_to(&base, &consent);
    let baseline = bias.apply_to(&base, &KanBiasConsent::denied());

    // Under consent : Rare + Epic must be HIGHER than under default-deny.
    let rare_idx = DropRateDistribution::index_of(Rarity::Rare);
    let epic_idx = DropRateDistribution::index_of(Rarity::Epic);
    let mr = modulated.rates[rare_idx];
    let br = baseline.rates[rare_idx];
    let me = modulated.rates[epic_idx];
    let be = baseline.rates[epic_idx];
    assert!(mr > br, "Rare should be biased up : modulated {mr} vs baseline {br}");
    assert!(me > be, "Epic should be biased up : modulated {me} vs baseline {be}");
    // Distribution still normalized.
    assert!(modulated.is_normalized(), "modulated dist must remain normalized");
}

// ───────────────────────────────────────────────────────────────────────
// § Test 4 — cap-deny without grant (Σ-mask default-deny)
// ───────────────────────────────────────────────────────────────────────

#[test]
fn cap_deny_without_grant() {
    let base = DropRateDistribution::PUBLIC;
    // Default consent = denied.
    let consent = KanBiasConsent::default();
    assert!(!consent.permits(), "default consent must be denied");

    // Even with extreme bias, denied consent = identity-application.
    // Q-06 8-tier : bias-vec dim = 8.
    let bias = KanBiasVector::new([1.0; 8]); // would be capped + amplified hugely
    let result = bias.apply_to(&base, &consent);
    assert_eq!(
        result, base,
        "denied consent must return base distribution unchanged"
    );

    // Granted but with zero session-hash : also denied (degenerate).
    let zero_grant = KanBiasConsent::granted(0);
    assert!(!zero_grant.permits(), "zero-hash grant must collapse to denied");
}

// ───────────────────────────────────────────────────────────────────────
// § Test 5 — Σ-Chain anchor roundtrip
// ───────────────────────────────────────────────────────────────────────

#[test]
fn sigma_chain_anchor_roundtrip() {
    let dist = DropRateDistribution::PUBLIC;
    let consent = KanBiasConsent::denied();
    let ctx = LootContext::default_for_combat_end();
    let item = roll_loot(&dist, &consent, &ctx, 0xCAFE_BABE_DEAD_BEEF_DEAD_BEEF_CAFE_BABE_u128);

    let signer = SigningKey::generate(&mut OsRng);
    let drop_event = LootDropEvent::new(item, 1234);

    let sigma_event = anchor_drop_to_sigma_chain(&signer, &drop_event);
    assert_eq!(sigma_event.kind, EventKind::LootDrop, "kind must be LootDrop");

    // Insert into ledger and verify-roundtrip.
    let mut ledger = SigmaLedger::new();
    ledger.insert(sigma_event.clone()).expect("insert must succeed");
    let outcome = verify_event(&sigma_event);
    assert!(matches!(outcome, VerifyOutcome::Verified), "verify must succeed : {outcome:?}");
    assert_eq!(ledger.len(), 1, "ledger must contain the inserted drop");
}

// ───────────────────────────────────────────────────────────────────────
// § Test 6 — pay-for-power rejected at load (affix-count overflow)
// ───────────────────────────────────────────────────────────────────────

#[test]
fn pay_for_power_rejected_at_load() {
    // Construct a malicious LootItem with affix-count > hard-cap (simulates
    // ingest from rogue server / modder smuggling extra payload).
    let mut bad_affixes = Vec::new();
    for i in 0..(AFFIX_COUNT_HARD_CAP + 5) {
        bad_affixes.push(LootAffix::Visual(
            cssl_host_loot::affix::VisualAffix::TracerColor(i as u32),
        ));
    }
    let bad_item = LootItem::new(Rarity::Mythic, 0, bad_affixes, LootSeason::BOOTSTRAP, 0);
    let result = attest_no_pay_for_power_strict(&bad_item);
    assert!(
        matches!(result, Err(PayForPowerError::AffixCountOverflow { .. })),
        "overflow must be rejected : got {result:?}"
    );
    assert!(!attest_no_pay_for_power(&bad_item), "lenient attest must also reject");
}

// ───────────────────────────────────────────────────────────────────────
// § Test 7 — determinism : same seed → bit-equal item
// ───────────────────────────────────────────────────────────────────────

#[test]
fn determinism_same_seed_bit_equal() {
    let dist = DropRateDistribution::PUBLIC;
    let consent = KanBiasConsent::denied();
    let ctx = LootContext::default_for_combat_end();
    let seed = 0x1234_5678_9ABC_DEF0_1234_5678_9ABC_DEF0_u128;

    let a = roll_loot(&dist, &consent, &ctx, seed);
    let b = roll_loot(&dist, &consent, &ctx, seed);
    assert_eq!(a, b, "same seed must produce bit-equal item");
}

// ───────────────────────────────────────────────────────────────────────
// § Test 8 — public rates sum to ~1.0
// ───────────────────────────────────────────────────────────────────────

#[test]
fn public_rates_sum_to_one() {
    let dist = DropRateDistribution::PUBLIC;
    let total = dist.total();
    assert!((0.999..=1.001).contains(&total), "total {total} must be in [0.999, 1.001]");
    assert!(dist.is_normalized());
}

// ───────────────────────────────────────────────────────────────────────
// § Test 9 — bias clamp prevents runaway amplification
// ───────────────────────────────────────────────────────────────────────

#[test]
fn bias_clamp_prevents_runaway() {
    // Q-06 8-tier : try to push Chaotic to 100% via huge weight (top tier).
    let base = DropRateDistribution::PUBLIC;
    let consent = KanBiasConsent::granted(0xDEAD);
    let bias = KanBiasVector::new([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 999_999.0]);
    let modulated = bias.apply_to(&base, &consent);

    // After clamp + renormalize, Chaotic shifts UP by at most the cap budget.
    let chaotic_idx = DropRateDistribution::index_of(Rarity::Chaotic);
    let chaotic_rate = modulated.rates[chaotic_idx];
    // The cap is MAX_BIAS_DELTA = 0.05 ; base is 0.00001 ; floor sum is ~1.05 after add.
    // Upper bound : (0.00001 + 0.05) / 0.999 ≈ 0.0501. Pin a generous ceiling.
    let upper_bound = 2.0 * (PUBLIC_DROP_RATES[7] + MAX_BIAS_DELTA);
    assert!(
        chaotic_rate < upper_bound,
        "Chaotic rate {chaotic_rate} must be bounded below {upper_bound}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// § Test 10 — zero-token consent rejected
// ───────────────────────────────────────────────────────────────────────

#[test]
fn consent_token_zero_rejected() {
    let zero = KanBiasConsent::granted(0);
    assert!(!zero.permits(), "zero session-hash must fail permits()");
    assert!(!zero.granted, "zero session-hash must collapse granted=false");
}

// ───────────────────────────────────────────────────────────────────────
// § Test 11 — rarity affix-count band (Mythic ships richer than Common)
// ───────────────────────────────────────────────────────────────────────

#[test]
fn rarity_affix_count_band_mythic_richer() {
    // Q-06 8-tier : Mythic (6,8) ships richer than Common (0,1).
    let dist = DropRateDistribution::PUBLIC;
    let consent = KanBiasConsent::granted(0xBEEF);
    let ctx = LootContext::default_for_combat_end();

    // Force Common via heavy negative bias on top tiers (Q-06 : 8-dim vec).
    let common_bias = KanBiasVector::new([0.05, -0.05, -0.05, -0.05, -0.05, -0.05, -0.05, -0.05]);
    // Force Mythic via heavy positive bias on Mythic (idx 5).
    let mythic_bias = KanBiasVector::new([-0.05, -0.05, -0.05, -0.05, -0.05, 0.05, 0.0, 0.0]);

    let mut common_total_affix = 0_u64;
    let mut mythic_total_affix = 0_u64;
    let mut common_n = 0_u64;
    let mut mythic_n = 0_u64;

    for i in 0..2_000_u128 {
        let seed = i.wrapping_mul(0xA51F_3C7B);
        let c_item = roll_loot_with_bias(&dist, &common_bias, &consent, &ctx, seed);
        let m_item = roll_loot_with_bias(&dist, &mythic_bias, &consent, &ctx, seed);
        if c_item.rarity == Rarity::Common {
            common_total_affix += c_item.affix_count() as u64;
            common_n += 1;
        }
        if m_item.rarity == Rarity::Mythic {
            mythic_total_affix += m_item.affix_count() as u64;
            mythic_n += 1;
        }
    }

    if common_n > 0 && mythic_n > 0 {
        // u64 → f64 cast bounded ; counts well below 2^52 mantissa.
        #[allow(clippy::cast_precision_loss)]
        let c_avg = common_total_affix as f64 / common_n as f64;
        #[allow(clippy::cast_precision_loss)]
        let m_avg = mythic_total_affix as f64 / mythic_n as f64;
        assert!(
            m_avg > c_avg,
            "Mythic affix-avg {m_avg} must be richer than Common {c_avg}"
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Test 13 — Q-06 8-tier rarity-set complete (Prismatic + Chaotic present)
// ───────────────────────────────────────────────────────────────────────

#[test]
fn q06_eight_tier_rarity_set_canonical() {
    // Q-06 Apocky-canonical 2026-05-01 : 8 tiers in canonical order.
    let all = Rarity::all();
    assert_eq!(all.len(), 8, "Q-06 canonical : 8-tier rarity ladder");
    assert_eq!(all[0], Rarity::Common);
    assert_eq!(all[1], Rarity::Uncommon);
    assert_eq!(all[2], Rarity::Rare);
    assert_eq!(all[3], Rarity::Epic);
    assert_eq!(all[4], Rarity::Legendary);
    assert_eq!(all[5], Rarity::Mythic);
    assert_eq!(all[6], Rarity::Prismatic);   // Q-06 NEW
    assert_eq!(all[7], Rarity::Chaotic);     // Q-06 NEW
    // Drop-rate array length matches rarity-array length.
    assert_eq!(PUBLIC_DROP_RATES.len(), 8);
}

// ───────────────────────────────────────────────────────────────────────
// § Test 12 — canonical bytes round-trip stable
// ───────────────────────────────────────────────────────────────────────

#[test]
fn canonical_bytes_stable_per_seed() {
    let dist = DropRateDistribution::PUBLIC;
    let consent = KanBiasConsent::denied();
    let ctx = LootContext::default_for_combat_end();
    let seed = 0x1357_9BDF_2468_ACE0_1357_9BDF_2468_ACE0_u128;

    let a = roll_loot(&dist, &consent, &ctx, seed).canonical_bytes();
    let b = roll_loot(&dist, &consent, &ctx, seed).canonical_bytes();
    assert_eq!(a, b, "canonical bytes must be stable per seed");
}
