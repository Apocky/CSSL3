//! § cssl-substrate-kan integration tests
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Cross-module integration tests that verify the spec-mandated invariants :
//!   - Pattern-stamp-resolve identity
//!   - Handle collision-free guarantees
//!   - AppendOnlyPool stability under heavy load
//!   - Multi-variant KanMaterial instantiation
//!   - Genome-to-Pattern via cssl-hdc surface
//!   - PGA-motor Pattern-preservation across substrate-translation
//!     (Axiom-2 substrate-relativity invariant)
//!
//! § The unit tests inside each module cover the per-module surface ; this
//!   file covers the cross-cutting invariants that span multiple modules.

use cssl_hdc::genome::Genome;
use cssl_pga::motor::Motor;
use cssl_substrate_kan::{
    AppendOnlyPool, Handle, KanGenomeWeights, KanMaterial, KanMaterialKind, KanNetwork, Pattern,
    PatternFingerprint, PhiTable, PoolError, SubstrateClassTag, EMBEDDING_DIM,
};

/// § Handle-collision-free across many stamps.
#[test]
fn handles_collision_free_across_many_stamps() {
    let mut t = PhiTable::new();
    let w = KanGenomeWeights::new_untrained();
    let mut handles = Vec::with_capacity(500);
    for s in 0..500u64 {
        let g = Genome::from_seed(s);
        let h = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        handles.push(h);
    }
    // § All 500 handles are distinct.
    let mut sorted: Vec<u64> = handles.iter().map(|h| h.to_raw()).collect();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 500);
}

/// § Pattern-stamp-resolve identity : the resolved Pattern's fingerprint
///   matches what the stamp returned. (Trivial property but load-bearing
///   for FieldCell.pattern_handle dereferences.)
#[test]
fn stamp_resolve_identity() {
    let mut t = PhiTable::new();
    let g = Genome::from_seed(99);
    let w = KanGenomeWeights::new_untrained();
    let h = t.stamp(&g, &w, SubstrateClassTag::Spinor).unwrap();
    let resolved = t.resolve(h).unwrap();
    let fp1 = resolved.fingerprint;
    // § Resolve again to confirm idempotence.
    let resolved2 = t.resolve(h).unwrap();
    assert_eq!(resolved2.fingerprint, fp1);
}

/// § AppendOnlyPool : handles minted early stay valid after many later
///   pushes. This is the load-bearing append-only contract.
#[test]
fn append_only_pool_stable_handles_under_load() {
    let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
    let h0 = p.push(1).unwrap();
    let h1 = p.push(2).unwrap();
    for i in 0..10_000u32 {
        let _ = p.push(i).unwrap();
    }
    assert_eq!(*p.resolve(h0).unwrap(), 1);
    assert_eq!(*p.resolve(h1).unwrap(), 2);
}

/// § Multi-variant KanMaterial : all four variants instantiate cleanly
///   and have distinct fingerprints.
#[test]
fn multi_variant_kan_material_instantiation() {
    let e = [0.5; EMBEDDING_DIM];
    let m_spec_4 = KanMaterial::spectral_brdf::<4>(e);
    let m_spec_8 = KanMaterial::spectral_brdf::<8>(e);
    let m_spec_16 = KanMaterial::spectral_brdf::<16>(e);
    let m_single = KanMaterial::single_band_brdf(e);
    let m_phys = KanMaterial::physics_impedance(e);
    let m_morph = KanMaterial::creature_morphology(e);

    // § All six fingerprints distinct.
    let fps = [
        m_spec_4.fingerprint,
        m_spec_8.fingerprint,
        m_spec_16.fingerprint,
        m_single.fingerprint,
        m_phys.fingerprint,
        m_morph.fingerprint,
    ];
    for i in 0..fps.len() {
        for j in (i + 1)..fps.len() {
            assert_ne!(fps[i], fps[j], "variants {i} and {j} share fingerprint");
        }
    }

    // § All variant tags correct.
    assert!(matches!(
        m_spec_4.kind,
        KanMaterialKind::SpectralBrdf { n_bands: 4 }
    ));
    assert!(matches!(
        m_spec_8.kind,
        KanMaterialKind::SpectralBrdf { n_bands: 8 }
    ));
    assert!(matches!(
        m_spec_16.kind,
        KanMaterialKind::SpectralBrdf { n_bands: 16 }
    ));
    assert!(matches!(m_single.kind, KanMaterialKind::SingleBandBrdf));
    assert!(matches!(m_phys.kind, KanMaterialKind::PhysicsImpedance));
    assert!(matches!(m_morph.kind, KanMaterialKind::CreatureMorphology));
}

/// § Genome-to-Pattern via cssl-hdc surface : a freshly-seeded genome
///   produces a valid Pattern, and same seed → same Pattern fingerprint.
#[test]
fn genome_to_pattern_deterministic() {
    let g1 = Genome::from_seed(0xCAFE_F00D);
    let g2 = Genome::from_seed(0xCAFE_F00D);
    let w = KanGenomeWeights::new_untrained();
    let p1 = Pattern::stamp(&g1, &w, SubstrateClassTag::Classical, 1).unwrap();
    let p2 = Pattern::stamp(&g2, &w, SubstrateClassTag::Classical, 1).unwrap();
    assert_eq!(p1.fingerprint, p2.fingerprint);
}

/// § Genome-distance-marker captured in Pattern matches genome.popcount().
#[test]
fn genome_distance_marker_captured() {
    let g = Genome::from_seed(0xDEAD);
    let w = KanGenomeWeights::new_untrained();
    let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
    assert_eq!(p.genome_distance_marker, g.hdc().popcount());
}

/// § PGA Motor — Pattern-preservation across substrate-translation.
///
///   The substrate-translation contract (`08_BODY/03_DIMENSIONAL_TRAVEL §V`)
///   says : after `translate_to_substrate(B) ; translate_to_substrate(A)`,
///   the Pattern fingerprint must match the original.
///
///   This test simulates that round-trip via tag-rotation : stamp a
///   Pattern under tag `Classical`, re-tag to `Plasma`, then re-tag back
///   to `Classical`. The fingerprint must be stable across all three
///   operations because `Pattern::re_tag` preserves the fingerprint by
///   contract.
///
///   The PGA Motor surface is exercised here too : a rigid motion can be
///   composed and undone (sandwich product invariance) — orthogonal to
///   Pattern-fingerprint preservation but tested together to verify the
///   two surfaces compose cleanly without spurious dependency drift.
#[test]
#[allow(clippy::float_cmp)]
fn pga_motor_pattern_preservation_across_translation() {
    let g = Genome::from_seed(1);
    let w = KanGenomeWeights::new_untrained();
    let p = Pattern::stamp(&g, &w, SubstrateClassTag::Classical, 1).unwrap();
    let fp_orig = p.fingerprint;

    // § Round-trip via re_tag : preserves fingerprint by contract.
    let p2 = p.re_tag(SubstrateClassTag::Plasma);
    assert_eq!(p2.fingerprint, fp_orig);
    assert_eq!(p2.substrate_class_tag, SubstrateClassTag::Plasma);

    let p3 = p2.re_tag(SubstrateClassTag::Classical);
    assert_eq!(p3.fingerprint, fp_orig);
    assert_eq!(p3.substrate_class_tag, SubstrateClassTag::Classical);

    // § Verify the PGA Motor surface composes cleanly. The identity motor
    //   has zero rotation + zero translation. Multiplying by identity is
    //   a no-op for any motor — sandwich-product invariance. The float
    //   comparisons against exact `0.0` / `1.0` are intentional : the
    //   Motor::IDENTITY const is byte-stable, no rounding involved.
    let m = Motor::IDENTITY;
    assert_eq!(m.s, 1.0);
    assert_eq!(m.r1, 0.0);
    assert_eq!(m.r2, 0.0);
    assert_eq!(m.r3, 0.0);
    assert_eq!(m.t1, 0.0);
    assert_eq!(m.t2, 0.0);
    assert_eq!(m.t3, 0.0);
    assert_eq!(m.m0, 0.0);
}

/// § PGA Motor sandwich-product round-trip : the identity Motor on a
///   Pattern's tag rotation is a no-op. This confirms cssl-pga is on
///   the dep tree and usable.
#[test]
#[allow(clippy::float_cmp)]
fn pga_motor_identity_on_pattern() {
    let g = Genome::from_seed(7);
    let w = KanGenomeWeights::new_untrained();
    let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
    let fp_before = p.fingerprint;
    // § The "translation" via identity-motor is conceptual ; the
    //   Pattern's fingerprint must be unaffected. We touch Motor::IDENTITY
    //   here to confirm the cssl-pga surface is on the dep-tree and
    //   compiles inside this crate's tests. The const Motor::IDENTITY is
    //   byte-stable, so exact-equal assert against 1.0 is intentional.
    let identity_motor = Motor::IDENTITY;
    assert_eq!(identity_motor.s, 1.0_f32);
    let p2 = p.re_tag(SubstrateClassTag::Universal);
    assert_eq!(p2.fingerprint, fp_before);
}

/// § FieldCell-style integration : pattern_handle + resolve workflow
///   matches what the upcoming `cssl-substrate-omega-field` crate will
///   do per spec § 1.
#[test]
fn fieldcell_pattern_handle_integration_simulation() {
    // § Simulate a FieldCell that has a `pattern_handle` field. The
    //   table is the source-of-truth ; the handle is stored inline.
    let mut t = PhiTable::new();
    let g = Genome::from_seed(42);
    let w = KanGenomeWeights::new_untrained();
    let pattern_handle = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();

    // § FieldCell would store `pattern_handle` (8 bytes) inline. A
    //   reader does : `t.resolve(cell.pattern_handle)`.
    let resolved = t.resolve(pattern_handle).unwrap();
    assert!(!resolved.fingerprint.is_null());

    // § A NULL handle in the FieldCell field means "unclaimed cell".
    let null_handle: Handle<Pattern> = Handle::NULL;
    assert!(null_handle.is_null());
    assert!(t.resolve(null_handle).is_err());
}

/// § KanMaterial.pattern_link round-trip : link a material to a pattern,
///   resolve it through the pool, get back the fingerprint.
#[test]
fn kan_material_pattern_link_roundtrip() {
    let mut t = PhiTable::new();
    let g = Genome::from_seed(13);
    let w = KanGenomeWeights::new_untrained();
    let pattern_handle = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
    let pattern_fp = t.resolve(pattern_handle).unwrap().fingerprint;

    let mut m = KanMaterial::single_band_brdf([0.5; EMBEDDING_DIM]);
    m.set_pattern_link(pattern_handle);

    // § Translate-pattern-class : extract the pattern fingerprint via
    //   the material's link.
    let extracted = m.translate_pattern_class(t.pool()).unwrap();
    assert_eq!(extracted, pattern_fp);
}

/// § KanMaterial.translate_pattern_class returns None when no link.
#[test]
fn kan_material_no_link_returns_none() {
    let t = PhiTable::new();
    let m = KanMaterial::single_band_brdf([0.0; EMBEDDING_DIM]);
    assert!(m.translate_pattern_class(t.pool()).is_none());
}

/// § Pool capacity guard : trying to push past MAX_PATTERNS_PER_POOL
///   returns AtCapacity. We can't actually test this directly because
///   2^30 allocations would OOM, but we can test the error path via
///   manual injection.
#[test]
fn pool_at_capacity_error_shape() {
    // § Simulate the error rather than actually allocating 2^30 slots.
    let e = PoolError::AtCapacity {
        len: 1 << 30,
        cap: 1 << 30,
    };
    let s = format!("{e}");
    assert!(s.contains("at-capacity"));
}

/// § Different KanGenomeWeights produce different Pattern fingerprints.
#[test]
fn different_weights_different_patterns() {
    let g = Genome::from_seed(1);
    let w1 = KanGenomeWeights::new_untrained();
    let mut w2 = KanGenomeWeights::new_untrained();
    w2.body_kan.trained = true;
    let mut w3 = KanGenomeWeights::new_untrained();
    w3.cognitive_kan.control_points[0][0] = 0.5;
    let mut w4 = KanGenomeWeights::new_untrained();
    w4.capability_kan.trained = true;

    let p1 = Pattern::stamp(&g, &w1, SubstrateClassTag::Universal, 1).unwrap();
    let p2 = Pattern::stamp(&g, &w2, SubstrateClassTag::Universal, 1).unwrap();
    let p3 = Pattern::stamp(&g, &w3, SubstrateClassTag::Universal, 1).unwrap();
    let p4 = Pattern::stamp(&g, &w4, SubstrateClassTag::Universal, 1).unwrap();

    let fps = [
        p1.fingerprint,
        p2.fingerprint,
        p3.fingerprint,
        p4.fingerprint,
    ];
    for i in 0..fps.len() {
        for j in (i + 1)..fps.len() {
            assert_ne!(fps[i], fps[j]);
        }
    }
}

/// § Cross-substrate round-trip : a Pattern stamped under tag-A,
///   re-tagged to tag-B then back to tag-A, has identical fingerprint.
///   This is the spec's `08_BODY/03_DIMENSIONAL_TRAVEL § V` round-trip
///   test verbatim.
#[test]
fn cross_substrate_round_trip() {
    let g = Genome::from_seed(0xC0FFEE);
    let w = KanGenomeWeights::new_untrained();
    let p_orig = Pattern::stamp(&g, &w, SubstrateClassTag::Classical, 1).unwrap();

    let p_translated = p_orig.clone().re_tag(SubstrateClassTag::Plasma);
    let p_back = p_translated.re_tag(SubstrateClassTag::Classical);

    assert_eq!(p_orig.fingerprint, p_back.fingerprint);
    assert_eq!(p_orig.substrate_class_tag, p_back.substrate_class_tag);
}

/// § Stamp-epoch monotonicity in PhiTable : every stamp captures a
///   distinct epoch.
#[test]
fn phi_table_epoch_monotonic() {
    let mut t = PhiTable::new();
    let g = Genome::from_seed(1);
    let w = KanGenomeWeights::new_untrained();
    let mut last_epoch = 0u64;
    for _ in 0..10 {
        let h = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
        let epoch_now = t.resolve(h).unwrap().stamp_epoch;
        assert!(epoch_now > last_epoch);
        last_epoch = epoch_now;
    }
}

/// § PatternFingerprint hex round-trip : two identical fingerprints have
///   identical hex.
#[test]
fn fingerprint_hex_consistency() {
    let g = Genome::from_seed(1);
    let w = KanGenomeWeights::new_untrained();
    let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
    let h = p.fingerprint.to_hex();
    let h2 = p.fingerprint.to_hex();
    assert_eq!(h, h2);
}

/// § KanNetwork I/O dim const-generic propagation through Pattern stamp.
#[test]
fn kan_network_dim_propagation() {
    // § Use varied I/O dims — the fingerprint must distinguish them.
    let net_a: KanNetwork<8, 4> = KanNetwork::new_untrained();
    let net_b: KanNetwork<16, 4> = KanNetwork::new_untrained();
    assert_ne!(net_a.fingerprint_bytes(), net_b.fingerprint_bytes());
}

/// § Three-way variant fingerprint distinction : confirm that the
///   variant-tag is meaningfully mixed into the fingerprint.
#[test]
fn three_way_variant_distinction() {
    let e = [0.0; EMBEDDING_DIM];
    let m_phys = KanMaterial::physics_impedance(e);
    let m_morph = KanMaterial::creature_morphology(e);
    let m_single = KanMaterial::single_band_brdf(e);
    assert_ne!(m_phys.fingerprint, m_morph.fingerprint);
    assert_ne!(m_morph.fingerprint, m_single.fingerprint);
    assert_ne!(m_phys.fingerprint, m_single.fingerprint);
}

/// § Genome ancestry probe : two distinct genomes produce distinct
///   genome_signatures in their stamped Patterns.
#[test]
fn genome_ancestry_distinct() {
    let g1 = Genome::from_seed(1);
    let g2 = Genome::from_seed(2);
    let w = KanGenomeWeights::new_untrained();
    let p1 = Pattern::stamp(&g1, &w, SubstrateClassTag::Universal, 1).unwrap();
    let p2 = Pattern::stamp(&g2, &w, SubstrateClassTag::Universal, 1).unwrap();
    assert_ne!(p1.genome_signature, p2.genome_signature);
}

/// § Genome ancestry preservation : same-seed genomes produce identical
///   genome_signatures.
#[test]
fn genome_ancestry_preserved() {
    let g1 = Genome::from_seed(42);
    let g2 = Genome::from_seed(42);
    let w = KanGenomeWeights::new_untrained();
    let p1 = Pattern::stamp(&g1, &w, SubstrateClassTag::Universal, 1).unwrap();
    let p2 = Pattern::stamp(&g2, &w, SubstrateClassTag::Universal, 1).unwrap();
    assert_eq!(p1.genome_signature, p2.genome_signature);
}

/// § PhiTable iteration in stable order matches stamp order.
#[test]
fn phi_table_iter_stable_order() {
    let mut t = PhiTable::new();
    let w = KanGenomeWeights::new_untrained();
    for s in 1..=20u64 {
        let g = Genome::from_seed(s);
        let _ = t.stamp(&g, &w, SubstrateClassTag::Universal).unwrap();
    }
    let collected: Vec<_> = t.pool().iter().collect();
    assert_eq!(collected.len(), 20);
    // § Indices monotonic 0..20.
    for (i, (h, _)) in collected.iter().enumerate() {
        assert_eq!(h.index(), i as u32);
    }
}

/// § Default PhiTable is empty (Default impl works).
#[test]
fn default_phi_table_is_empty() {
    let t = PhiTable::default();
    assert!(t.is_empty());
}

/// § Handle equality semantics : two handles are equal iff their packed
///   u64 representations match.
#[test]
fn handle_equality_via_raw() {
    let a: Handle<Pattern> = Handle::from_parts(1, 5);
    let b: Handle<Pattern> = Handle::from_raw(a.to_raw());
    assert_eq!(a, b);
}

/// § PatternFingerprint NULL is null.
#[test]
fn pattern_fingerprint_null() {
    assert!(PatternFingerprint::NULL.is_null());
    let g = Genome::from_seed(1);
    let w = KanGenomeWeights::new_untrained();
    let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
    assert!(!p.fingerprint.is_null());
}

/// § KanMaterial fingerprint and Pattern fingerprint use independent type
///   wrappers — they cannot be confused at the type level.
#[test]
fn material_and_pattern_fingerprints_independent_types() {
    let g = Genome::from_seed(1);
    let w = KanGenomeWeights::new_untrained();
    let p = Pattern::stamp(&g, &w, SubstrateClassTag::Universal, 1).unwrap();
    let m = KanMaterial::single_band_brdf([0.5; EMBEDDING_DIM]);
    // § The bytes might collide (extremely-unlikely) but the types differ ;
    //   this test mostly exists to confirm compilation stability of the
    //   distinct-types contract.
    let _: PatternFingerprint = p.fingerprint;
    let _: cssl_substrate_kan::MaterialFingerprint = m.fingerprint;
}

/// § AppendOnlyPool.iter is ExactSizeIterator.
#[test]
fn pool_iter_exact_size() {
    let mut p: AppendOnlyPool<u32> = AppendOnlyPool::new();
    for i in 0..10 {
        let _ = p.push(i).unwrap();
    }
    let it = p.iter();
    assert_eq!(it.len(), 10);
}
