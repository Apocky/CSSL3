//! § T11-W12-RATING-INTEGRATION — end-to-end paths.
//!
//! Covers the full submit → aggregate → KAN-emit flow ; revoke-recompute ;
//! cap-deny ; k-floor enforcement.

use cssl_content_rating::{
    tag_index, AggregateView, AggregateVisibility, QualitySignal, Rating, RatingStore, TagBitset,
    CAP_AGGREGATE_PUBLIC, CAP_RATE, K_FLOOR_SINGLE, K_FLOOR_TRENDING,
};

fn flag(name: &str) -> TagBitset {
    let mut t = TagBitset::EMPTY;
    t.set(tag_index(name).expect(name));
    t
}

fn populate_with_n(store: &mut RatingStore, content: u32, n: u32, stars: u8, tag: &str) {
    let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
    for i in 0..n {
        let r = Rating::new(
            u64::from(i) + 1,
            content,
            stars,
            flag(tag),
            mask,
            1_000 + i,
            200,
        )
        .expect("valid");
        store.submit(r, None).expect("submit ok");
    }
}

#[test]
fn full_submit_aggregate_signal_path() {
    let mut store = RatingStore::new();
    populate_with_n(&mut store, 7, K_FLOOR_SINGLE, 5, "remix-worthy");

    // Aggregate visible
    let agg = store.aggregate_for(7);
    assert_eq!(agg.visibility, AggregateVisibility::Visible);
    assert_eq!(agg.distinct_rater_count, K_FLOOR_SINGLE);

    // KAN-bridge signal emits at this k-level
    let sig = store
        .quality_signal_for(7)
        .expect("k met → signal emits");
    assert!(sig.is_strong_positive(), "5★+remix-worthy = strong positive");
}

#[test]
fn k_floor_trending_gates_rank_influence() {
    let mut store = RatingStore::new();
    populate_with_n(&mut store, 7, K_FLOOR_SINGLE, 5, "fun");
    let agg = store.aggregate_for(7);
    assert_eq!(agg.visibility, AggregateVisibility::Visible);
    assert!(!agg.visibility.eligible_for_trending());

    // Add 5 more → reach K_FLOOR_TRENDING
    populate_with_n(&mut store, 7, K_FLOOR_TRENDING, 5, "fun");
    let agg2 = store.aggregate_for(7);
    assert_eq!(agg2.visibility, AggregateVisibility::Trending);
    assert!(agg2.visibility.eligible_for_trending());
}

#[test]
fn sovereign_revoke_recomputes_aggregate_to_hidden() {
    let mut store = RatingStore::new();
    populate_with_n(&mut store, 9, K_FLOOR_SINGLE, 4, "balanced");
    assert_eq!(store.aggregate_for(9).visibility, AggregateVisibility::Visible);

    // Rater 1 revokes — distinct count drops to 4 → Hidden
    store
        .revoke(1, 9, CAP_RATE | CAP_AGGREGATE_PUBLIC, 9_999)
        .expect("revoke ok");
    assert_eq!(
        store.aggregate_for(9).visibility,
        AggregateVisibility::Hidden
    );

    // Rater 1 can still see their own (withdrawn) row
    let row = store.get_for_rater(1, 9).expect("rater sees own row");
    assert!(row.0.is_withdrawn());
}

#[test]
fn cap_aggregate_public_omitted_keeps_aggregate_hidden() {
    let mut store = RatingStore::new();
    let mask_priv = CAP_RATE; // missing CAP_AGGREGATE_PUBLIC
    for i in 0..K_FLOOR_SINGLE {
        let r = Rating::new(
            u64::from(i) + 1,
            11,
            5,
            TagBitset::EMPTY,
            mask_priv,
            1_000,
            200,
        )
        .expect("valid (private)");
        store.submit(r, None).expect("submit ok");
    }
    assert_eq!(
        store.aggregate_for(11).visibility,
        AggregateVisibility::Hidden
    );
    assert!(
        store.quality_signal_for(11).is_none(),
        "KAN must not see suppressed rows"
    );
}

#[test]
fn aggregate_filters_other_content_so_no_cross_pollination() {
    let mut store = RatingStore::new();
    populate_with_n(&mut store, 7, K_FLOOR_SINGLE, 5, "fun");
    populate_with_n(&mut store, 8, K_FLOOR_SINGLE, 1, "tense");
    let a = store.aggregate_for(7);
    let b = store.aggregate_for(8);
    assert_eq!(a.distinct_rater_count, K_FLOOR_SINGLE);
    assert_eq!(b.distinct_rater_count, K_FLOOR_SINGLE);
    assert_eq!(a.mean_stars_q8, 255);
    assert!(b.mean_stars_q8 < 100);
    let top_a = a.top_tags(1);
    let top_b = b.top_tags(1);
    assert_eq!(top_a[0].0, "fun");
    assert_eq!(top_b[0].0, "tense");
}

#[test]
fn rating_pack_roundtrip_through_aggregate() {
    let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
    let r1 = Rating::new(42, 13, 4, flag("creative"), mask, 1_234, 200).expect("valid");
    let buf = r1.pack();
    let r2 = Rating::unpack(&buf).expect("unpack ok");
    assert_eq!(r1, r2);

    // Use the unpacked rating in an aggregate
    let agg = AggregateView::from_ratings(13, &[r2]);
    assert_eq!(agg.distinct_rater_count, 1);
    // 1 < K_FLOOR_SINGLE → Hidden, no leakage
    assert_eq!(agg.visibility, AggregateVisibility::Hidden);
    assert_eq!(agg.mean_stars_q8, 0);
}

#[test]
fn submit_rejects_zero_sigma_via_construction_layer() {
    use cssl_content_rating::RatingError;
    let err = Rating::new(1, 7, 5, TagBitset::EMPTY, 0, 1, 0)
        .expect_err("sigma_mask=0 must reject construction");
    assert!(matches!(err, RatingError::CapRateMissing(0)));
}

#[test]
fn submit_overwrite_does_not_double_count_aggregate() {
    let mut store = RatingStore::new();
    populate_with_n(&mut store, 7, K_FLOOR_SINGLE, 4, "fun");
    let agg_before = store.aggregate_for(7);
    assert_eq!(agg_before.distinct_rater_count, K_FLOOR_SINGLE);
    // Re-submit same rater 1 → must overwrite, not add
    let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
    let r = Rating::new(1, 7, 5, flag("novel"), mask, 2_000, 200).expect("valid");
    store.submit(r, None).expect("submit ok");
    let agg_after = store.aggregate_for(7);
    assert_eq!(agg_after.distinct_rater_count, K_FLOOR_SINGLE);
}

#[test]
fn quality_signal_for_aggregate_path_matches_from_ratings_path() {
    let mask = CAP_RATE | CAP_AGGREGATE_PUBLIC;
    let mut rs: Vec<Rating> = Vec::new();
    for i in 0..K_FLOOR_SINGLE {
        rs.push(
            Rating::new(
                u64::from(i) + 1,
                7,
                5,
                flag("remix-worthy"),
                mask,
                1_000,
                200,
            )
            .expect("valid"),
        );
    }
    let agg = AggregateView::from_ratings(7, &rs);
    let sig_a = QualitySignal::from_ratings(7, &rs).expect("path A");
    let sig_b = QualitySignal::from_aggregate(7, &rs, &agg).expect("path B");
    assert_eq!(sig_a, sig_b);
}

#[test]
fn revoke_idempotent_does_not_corrupt_aggregate() {
    let mut store = RatingStore::new();
    populate_with_n(&mut store, 7, K_FLOOR_SINGLE, 5, "fun");
    store.revoke(1, 7, CAP_RATE, 1).expect("revoke 1");
    store.revoke(1, 7, CAP_RATE, 2).expect("revoke 2 idempotent");
    store.revoke(1, 7, CAP_RATE, 3).expect("revoke 3 idempotent");
    // Distinct count at 4 stable, Hidden
    let agg = store.aggregate_for(7);
    assert_eq!(agg.visibility, AggregateVisibility::Hidden);
    assert_eq!(agg.distinct_rater_count, K_FLOOR_SINGLE - 1);
}
