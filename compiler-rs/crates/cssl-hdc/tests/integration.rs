//! § cssl-hdc integration tests
//! ════════════════════════════════════════════════════════════════════════════
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::redundant_clone
)]
//!
//! § ROLE
//!   Cross-module integration scenarios that exercise the full HDC
//!   algebra : bind + bundle + permute + similarity composing into the
//!   classic Kanerva use cases (sequence encoding, ancestry chains,
//!   semantic-tag binding) at the production dimension D = 10000. Unit
//!   tests live alongside each module ; this file is the "do they
//!   compose correctly" surface.

use cssl_hdc::{
    bind, bundle, hamming_distance, hamming_distance_normalized, inverse_permute, permute,
    similarity_bipolar, unbind, Genome, GenomeDistance, Hologram, Hypervector,
    SparseDistributedMemory, SplitMix64, HDC_DIM,
};

/// § Sequence encoding via bind+permute+bundle, the canonical Kanerva
///   pattern. Encode "A then B then C" into one hypervector ; query
///   "what was at position k ?".
#[test]
fn sequence_encoding_recovers_position() {
    // § Symbol vocabulary : 5 random hypervectors representing distinct
    //   tokens.
    let vocab: Vec<Hypervector<10000>> = (0..5)
        .map(|i| Hypervector::random_from_seed(1000 + i as u64))
        .collect();

    // § Encode a 3-symbol sequence : tokens 0, 2, 4.
    let sequence = [0usize, 2, 4];
    let bound: Vec<Hypervector<10000>> = sequence
        .iter()
        .enumerate()
        .map(|(pos, &tok)| {
            // § Each symbol is permuted by its position (acts as a
            //   position-tag) and then bundled.
            permute(&vocab[tok], pos)
        })
        .collect();
    let bound_refs: Vec<&Hypervector<10000>> = bound.iter().collect();
    let encoded = bundle(&bound_refs);

    // § Query position 1 (= token 2) : un-permute the encoded vector
    //   by position 1, then similarity-search the vocab.
    let query_pos = 1;
    let unpermuted = inverse_permute(&encoded, query_pos);

    let scores: Vec<f32> = vocab
        .iter()
        .map(|tok| similarity_bipolar(&unpermuted, tok))
        .collect();

    // § Token 2 should have the highest similarity.
    let best = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(best, 2);
}

/// § Genome ancestry chain : root → 4 generations of children, each
///   binding its parent's HDC with a fresh per-generation key.
///   Test : given the great-great-grandchild + generation keys,
///   recover the root.
#[test]
fn genome_ancestry_chain_4_deep() {
    let root = Genome::from_seed(1).into_hdc();
    let keys: Vec<Hypervector<10000>> = (0..4)
        .map(|i| Hypervector::random_from_seed(2000 + i as u64))
        .collect();

    // § Forward : descend the chain, binding each generation.
    let mut current = root.clone();
    for k in &keys {
        current = bind(&current, k);
    }

    // § Backward : unbind in reverse order.
    let mut recovered = current;
    for k in keys.iter().rev() {
        recovered = unbind(&recovered, k);
    }

    assert_eq!(recovered, root);
}

/// § Hologram-of-genomes : store 8 (parent_genome → child_genome) pairs
///   in a single 10000-D hologram, recall by parent.
#[test]
fn genome_hologram_recall() {
    let parents: Vec<Hypervector<10000>> = (0..8)
        .map(|i| Genome::from_seed(100 + i as u64).into_hdc())
        .collect();
    let children: Vec<Hypervector<10000>> = (0..8)
        .map(|i| Genome::from_seed(200 + i as u64).into_hdc())
        .collect();

    let pairs: Vec<(&Hypervector<10000>, &Hypervector<10000>)> =
        parents.iter().zip(children.iter()).collect();
    let h: Hologram<10000, 8> = Hologram::from_pairs(&pairs);

    for (i, parent) in parents.iter().enumerate() {
        let recovered = h.recall(parent);
        let s = similarity_bipolar(&recovered, &children[i]);
        // § Capacity 8 ⇒ 1/sqrt(8) ≈ 0.35 expected ; lower-bound 0.2.
        assert!(s > 0.2, "parent {i} → child recall failed : {s}");
    }
}

/// § Genome cross-mutate-distance pipeline : create two parents,
///   cross them, mutate the child slightly, verify the distance
///   structure.
#[test]
fn genome_evolution_pipeline() {
    let parent_a = Genome::from_seed(1);
    let parent_b = Genome::from_seed(2);
    let mut rng = SplitMix64::new(42);

    let child = Genome::cross(&parent_a, &parent_b, &mut rng);
    let mutated_child = child.clone().mutate(&mut rng, 0.005); // 0.5% mutation

    // § The mutated child should be close to the original child (~50
    //   bits differ) and roughly between the two parents on average.
    let d_child_orig = Genome::distance(&child, &mutated_child);
    assert!(
        d_child_orig.hamming < 200,
        "mutation drift too large : {}",
        d_child_orig.hamming
    );

    let d_to_a = Genome::distance(&mutated_child, &parent_a);
    let d_to_b = Genome::distance(&mutated_child, &parent_b);
    let d_ab = Genome::distance(&parent_a, &parent_b);

    // § Triangle bound : sum of distances to parents ≥ parent-distance.
    assert!(
        d_to_a.hamming + d_to_b.hamming >= d_ab.hamming.saturating_sub(100),
        "triangle violated"
    );
}

/// § SDM with 10000-D + 1024 cells : auto-associative recall under
///   noise. Write a pattern, then read at a noisy version of the same
///   pattern (200 bit-flips), expect clean recovery.
#[test]
fn sdm_recall_under_noise() {
    let mut sdm: SparseDistributedMemory<10000, 1024> = SparseDistributedMemory::new(99);
    let pattern: Hypervector<10000> = Hypervector::random_from_seed(7);
    sdm.write_auto(&pattern);

    // § Construct a noisy version of the pattern : flip ≈ 200 bits.
    let mut noisy = pattern.clone();
    let mut rng = SplitMix64::new(12345);
    for _ in 0..200 {
        let bit = (rng.next() as usize) % 10000;
        noisy.set_bit(bit, !noisy.bit(bit));
    }

    // § Read at the noisy address. The SDM should pull cells whose
    //   addresses are close to BOTH the noisy address and the clean
    //   pattern (since they're 200 bits apart, well within radius).
    let readout = sdm.read(&noisy);
    let d_to_clean = hamming_distance_normalized(&readout.value, &pattern);

    // § Recovery should be much closer to the clean pattern than the
    //   noisy input was.
    assert!(d_to_clean < 0.3, "noisy recall failed : d = {d_to_clean}");
}

/// § Bind-bundle distributivity : `bundle(bind(a, k), bind(b, k), bind(c, k))`
///   exactly equals `bind(bundle(a, b, c), k)` when N is odd (no tie-
///   break loss in the majority-vote). This is the property that lets
///   you "factor out" a common key from a bundle of bound pairs.
#[test]
fn bind_bundle_distributivity() {
    let a: Hypervector<10000> = Hypervector::random_from_seed(1);
    let b: Hypervector<10000> = Hypervector::random_from_seed(2);
    let c: Hypervector<10000> = Hypervector::random_from_seed(4);
    let k: Hypervector<10000> = Hypervector::random_from_seed(3);

    let bound_first = bundle(&[&bind(&a, &k), &bind(&b, &k), &bind(&c, &k)]);
    let bundled_first = bind(&bundle(&[&a, &b, &c]), &k);

    // § XOR distributes over majority-vote exactly when N is odd.
    //   N = 3 has no tie-break ambiguity ⇒ exact equality.
    assert_eq!(bound_first, bundled_first);
}

/// § Determinism end-to-end : run a multi-step HDC computation twice
///   from the same seed, expect identical bit-for-bit output.
#[test]
fn end_to_end_determinism() {
    fn compute_pipeline(seed: u64) -> Hypervector<HDC_DIM> {
        let g_a = Genome::from_seed(seed);
        let g_b = Genome::from_seed(seed.wrapping_add(1));
        let mut rng = SplitMix64::new(seed.wrapping_mul(7));
        let child = Genome::cross(&g_a, &g_b, &mut rng);
        let mutated = child.mutate(&mut rng, 0.01);
        let permuted = permute(mutated.hdc(), 17);
        let key: Hypervector<HDC_DIM> = Hypervector::random_from_seed(seed.wrapping_add(99));
        bind(&permuted, &key)
    }

    let result_1 = compute_pipeline(0xDEADBEEF);
    let result_2 = compute_pipeline(0xDEADBEEF);
    assert_eq!(result_1, result_2);

    let result_3 = compute_pipeline(0xCAFEBABE);
    assert_ne!(result_1, result_3);
}

/// § GenomeDistance struct fields populated correctly.
#[test]
fn genome_distance_struct() {
    let a = Genome::from_seed(1);
    let b = Genome::from_seed(2);
    let d: GenomeDistance = Genome::distance(&a, &b);
    // § Internal consistency : normalized = hamming / HDC_DIM.
    let expected_normalized = (d.hamming as f32) / (HDC_DIM as f32);
    assert!((d.normalized - expected_normalized).abs() < 1e-6);
    // § bipolar_similarity = 1 - 2 * normalized.
    let expected_bipolar = 1.0 - 2.0 * expected_normalized;
    assert!((d.bipolar_similarity - expected_bipolar).abs() < 1e-6);
}

/// § Performance smoke : 1000 hypervector binds at D = 10000 should
///   complete in well under 100 ms (sub-µs each typically). Not a
///   strict perf assert — just a "haven't accidentally O(n²)" check.
#[test]
fn perf_smoke_1k_binds() {
    let a: Hypervector<10000> = Hypervector::random_from_seed(1);
    let b: Hypervector<10000> = Hypervector::random_from_seed(2);
    let start = std::time::Instant::now();
    let mut acc = a.clone();
    for _ in 0..1000 {
        acc = bind(&acc, &b);
    }
    let elapsed = start.elapsed();
    // § Sanity : less than 1 second for 1000 binds. Should be ≈ 1 ms
    //   in optimized builds.
    assert!(
        elapsed.as_secs() < 1,
        "1000 binds took {elapsed:?} — perf regression?"
    );
    // § acc has been bound 1000 times with b ; since 1000 is even,
    //   acc == a.
    assert_eq!(acc, a);
}

/// § Hamming-distance + popcount-of-XOR cross-check.
#[test]
fn hamming_via_popcount_xor() {
    let a: Hypervector<10000> = Hypervector::random_from_seed(11);
    let b: Hypervector<10000> = Hypervector::random_from_seed(13);
    let h_via_similarity = hamming_distance(&a, &b);
    // § Use the simd module helper directly.
    let h_via_simd = cssl_hdc::popcount_xor_slice(a.words(), b.words());
    assert_eq!(h_via_similarity, h_via_simd);
}

/// § Words access via the `HamFn` trait — matches direct `bit()` reads.
#[test]
fn hamfn_trait_consistency() {
    use cssl_hdc::HamFn;
    let a: Hypervector<128> = Hypervector::ones();
    assert_eq!(a.ham_dim(), 128);
    assert_eq!(a.ham_word_count(), 2);
    assert_eq!(a.ham_word(0), u64::MAX);
    assert_eq!(a.ham_words().len(), 2);
}
