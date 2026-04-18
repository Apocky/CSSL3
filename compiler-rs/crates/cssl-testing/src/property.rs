//! Property-based oracle (`@property`) — QuickCheck / Hypothesis lineage.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • property-based.
//! § ROLE   : generators derived from refinement-types (§§ 20) produce well-typed
//!            inputs; shrinking auto-derived from refinement constraints; seeds
//!            deterministic for replay-safety.
//! § STATUS : T11-phase-2b live implementation — `PropertyOracle` runs
//!            `cases`-many generated inputs against a user-supplied check-fn,
//!            returns the first counterexample shrunk to minimal form.

/// Config for the `@property` oracle.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Number of generated cases per-run. Default 1000 (§§ 23 "scale"); 10000 in `nightly-extended`.
    pub cases: u32,
    /// Deterministic seed for generator (replay-safe).
    pub seed: u64,
    /// Maximum shrink-iterations after a counterexample is found. 0 = no shrink.
    pub shrink_rounds: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cases: 1000,
            seed: 0xc551_a770_c551_a770_u64,
            shrink_rounds: 64,
        }
    }
}

/// Outcome of running the `@property` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11.
    Stage0Unimplemented,
    /// All cases passed.
    Ok { cases_run: u32 },
    /// A counterexample was found and shrunk to the given form.
    Counterexample {
        shrunk_input: String,
        message: String,
    },
}

/// Dispatcher trait for `@property` oracle.
pub trait Dispatcher {
    /// Execute the oracle against the given config.
    fn run(&self, config: &Config) -> Outcome;
}

/// Stage0 stub dispatcher — always returns `Stage0Unimplemented`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0Stub;

impl Dispatcher for Stage0Stub {
    fn run(&self, _config: &Config) -> Outcome {
        Outcome::Stage0Unimplemented
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Deterministic PRNG : tiny LCG suitable for replay-safe property-gen.
// ─────────────────────────────────────────────────────────────────────────

/// Deterministic linear-congruential generator. Small, fast, and crucially
/// reproducible — same seed → same stream. Not cryptographically strong, but
/// property-based testing doesn't need that ; it needs REPLAY.
#[derive(Debug, Clone, Copy)]
pub struct Lcg {
    state: u64,
}

impl Lcg {
    /// Seed a new LCG.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next raw `u64`. Uses the canonical LCG constants from
    /// Numerical Recipes (Knuth) : `a = 6364136223846793005`, `c = 1442695040888963407`.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    /// Generate an `i64` in `[min, max]` inclusive. `min ≤ max` required.
    #[allow(clippy::cast_possible_wrap)] // `r as i64` : r < range ≤ i64-span, offset by min keeps it in-range
    #[allow(clippy::cast_sign_loss)]
    pub fn gen_i64(&mut self, min: i64, max: i64) -> i64 {
        debug_assert!(min <= max, "min > max");
        let range = (max as i128 - min as i128 + 1) as u64;
        let r = self.next_u64() % range;
        min + r as i64
    }

    /// Generate a `bool` (50/50).
    pub fn gen_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Generate an `f64` uniformly in `[0.0, 1.0)`.
    #[allow(clippy::cast_precision_loss)] // shift-11 keeps high 53 bits — exactly f64-mantissa-width
    pub fn gen_unit_f64(&mut self) -> f64 {
        let bits = (self.next_u64() >> 11) as f64; // 53-bit mantissa
        bits / (1u64 << 53) as f64
    }

    /// Generate an `f64` in `[min, max)`.
    pub fn gen_f64(&mut self, min: f64, max: f64) -> f64 {
        (max - min).mul_add(self.gen_unit_f64(), min)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Generator trait + shrinker : inputs produced + reduced from LCG.
// ─────────────────────────────────────────────────────────────────────────

/// Generator trait : produces an input value and shrinks it toward the
/// origin. The `generate(&mut Lcg) -> T` method is random-but-deterministic ;
/// `shrink(&T) -> Vec<T>` returns "smaller" candidate values to try after a
/// counterexample is found (empty vec = cannot shrink further).
pub trait Generator<T: core::fmt::Debug + Clone> {
    /// Produce a fresh value using the LCG.
    fn generate(&self, rng: &mut Lcg) -> T;

    /// Return smaller candidate values for shrinking. Empty vec = at-minimum.
    /// Default : no shrinking (type-independent minimum-form cannot be
    /// guessed). Overrides provide type-specific shrink paths.
    fn shrink(&self, _v: &T) -> Vec<T> {
        Vec::new()
    }
}

/// Integer generator over a closed `[min, max]` range.
#[derive(Debug, Clone, Copy)]
pub struct IntGen {
    pub min: i64,
    pub max: i64,
}

impl Generator<i64> for IntGen {
    fn generate(&self, rng: &mut Lcg) -> i64 {
        rng.gen_i64(self.min, self.max)
    }
    fn shrink(&self, v: &i64) -> Vec<i64> {
        // Shrink toward 0 (if in range), then toward smaller-magnitude.
        let mut out = Vec::new();
        if *v != 0 && self.min <= 0 && 0 <= self.max {
            out.push(0);
        }
        if *v > self.min {
            out.push(v / 2);
            out.push(v - 1);
        }
        if *v < self.max {
            out.push(v + 1);
        }
        out.into_iter().filter(|x| x != v).collect()
    }
}

/// Bool generator (50/50).
#[derive(Debug, Clone, Copy, Default)]
pub struct BoolGen;

impl Generator<bool> for BoolGen {
    fn generate(&self, rng: &mut Lcg) -> bool {
        rng.gen_bool()
    }
    fn shrink(&self, v: &bool) -> Vec<bool> {
        // Shrink true → false (false is the "minimum").
        if *v {
            vec![false]
        } else {
            Vec::new()
        }
    }
}

/// Float generator over a half-open `[min, max)` range. Produces real `f64`
/// values via `Lcg::gen_f64` (mantissa-preserving). Shrinks toward zero (if in
/// range) then toward halved-magnitude — the same shape as `IntGen`.
#[derive(Debug, Clone, Copy)]
pub struct FloatGen {
    pub min: f64,
    pub max: f64,
}

impl Generator<f64> for FloatGen {
    fn generate(&self, rng: &mut Lcg) -> f64 {
        rng.gen_f64(self.min, self.max)
    }
    #[allow(clippy::float_cmp)] // exact-equality ok : v=0 and halved=v are bit-exact cases
    fn shrink(&self, v: &f64) -> Vec<f64> {
        let mut out = Vec::new();
        // Shrink toward 0 if 0 ∈ [min, max) and v ≠ 0.
        if *v != 0.0 && self.min <= 0.0 && 0.0 < self.max {
            out.push(0.0);
        }
        // Halve-magnitude (convergent toward 0 in finitely-many rounds).
        let halved = v * 0.5;
        if halved >= self.min && halved < self.max && halved != *v {
            out.push(halved);
        }
        out
    }
}

/// 3-tuple generator (e.g., for SDF-point components `(px, py, pz)`).
/// Generic over an inner `Generator<T>` that drives each component.
#[derive(Debug, Clone, Copy)]
pub struct TripleGen<G> {
    pub inner: G,
}

impl<T, G> Generator<(T, T, T)> for TripleGen<G>
where
    T: core::fmt::Debug + Clone,
    G: Generator<T>,
{
    fn generate(&self, rng: &mut Lcg) -> (T, T, T) {
        let a = self.inner.generate(rng);
        let b = self.inner.generate(rng);
        let c = self.inner.generate(rng);
        (a, b, c)
    }
    fn shrink(&self, v: &(T, T, T)) -> Vec<(T, T, T)> {
        // Shrink one component at a time, keeping the other two fixed.
        let mut out = Vec::new();
        for candidate_a in self.inner.shrink(&v.0) {
            out.push((candidate_a, v.1.clone(), v.2.clone()));
        }
        for candidate_b in self.inner.shrink(&v.1) {
            out.push((v.0.clone(), candidate_b, v.2.clone()));
        }
        for candidate_c in self.inner.shrink(&v.2) {
            out.push((v.0.clone(), v.1.clone(), candidate_c));
        }
        out
    }
}

/// Variable-length `Vec<T>` generator. Length is drawn uniformly from
/// `[0, max_len]`. Each element is drawn via the inner `Generator<T>`.
/// Shrinks first by truncation (halve-length), then by shrinking the
/// last element.
#[derive(Debug, Clone, Copy)]
pub struct VecGen<G> {
    pub inner: G,
    pub max_len: u32,
}

impl<T, G> Generator<Vec<T>> for VecGen<G>
where
    T: core::fmt::Debug + Clone,
    G: Generator<T>,
{
    fn generate(&self, rng: &mut Lcg) -> Vec<T> {
        let raw = rng.next_u64();
        let len = if self.max_len == 0 {
            0
        } else {
            (raw % (u64::from(self.max_len) + 1)) as usize
        };
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(self.inner.generate(rng));
        }
        out
    }
    fn shrink(&self, v: &Vec<T>) -> Vec<Vec<T>> {
        let mut out = Vec::new();
        if v.is_empty() {
            return out;
        }
        // Truncate to half-length.
        out.push(v[..v.len() / 2].to_vec());
        // Drop last element.
        out.push(v[..v.len() - 1].to_vec());
        // Shrink last element, keep others.
        if let Some(last) = v.last() {
            for candidate in self.inner.shrink(last) {
                let mut shrunk = v[..v.len() - 1].to_vec();
                shrunk.push(candidate);
                out.push(shrunk);
            }
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Property runner : orchestrates generator + check-fn + shrinker.
// ─────────────────────────────────────────────────────────────────────────

/// Run a property check over `config.cases` generated inputs. Returns
/// `Ok { cases_run }` if every case passes, or `Counterexample` with the
/// shrunken minimal input if one fails.
///
/// The `check` closure returns `true` iff the property holds for `input`.
/// `label` is a human-readable property name used in the counterexample
/// message (e.g., `"addition is commutative"`).
///
/// § REPLAY-SAFETY
///   Same `config.seed` + same generator → identical input sequence across
///   runs. Captured counterexamples can be replayed by re-running with the
///   logged seed.
pub fn run_property<T, G, F>(config: &Config, generator: &G, check: F, label: &str) -> Outcome
where
    T: core::fmt::Debug + Clone,
    G: Generator<T>,
    F: Fn(&T) -> bool,
{
    let mut rng = Lcg::new(config.seed);
    for i in 0..config.cases {
        let input = generator.generate(&mut rng);
        if !check(&input) {
            let shrunk = shrink_counterexample(generator, &check, &input, config.shrink_rounds);
            return Outcome::Counterexample {
                shrunk_input: format!("{shrunk:?}"),
                message: format!("property `{label}` failed at case {i} ; shrunk to above form"),
            };
        }
    }
    Outcome::Ok {
        cases_run: config.cases,
    }
}

/// Greedy shrinker : repeatedly try smaller candidates until a round passes
/// without finding a further-shrunk failing input.
fn shrink_counterexample<T, G, F>(generator: &G, check: &F, start: &T, max_rounds: u32) -> T
where
    T: core::fmt::Debug + Clone,
    G: Generator<T>,
    F: Fn(&T) -> bool,
{
    let mut current = start.clone();
    for _ in 0..max_rounds {
        let candidates = generator.shrink(&current);
        let mut shrunk_this_round = false;
        for c in candidates {
            if !check(&c) {
                current = c;
                shrunk_this_round = true;
                break;
            }
        }
        if !shrunk_this_round {
            break;
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::{
        run_property, BoolGen, Config, Dispatcher, FloatGen, Generator, IntGen, Lcg, Outcome,
        Stage0Stub, TripleGen, VecGen,
    };

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }

    #[test]
    fn lcg_same_seed_produces_same_stream() {
        let mut a = Lcg::new(42);
        let mut b = Lcg::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn lcg_different_seeds_diverge() {
        let mut a = Lcg::new(1);
        let mut b = Lcg::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn lcg_gen_i64_stays_in_range() {
        let mut rng = Lcg::new(123);
        for _ in 0..1000 {
            let v = rng.gen_i64(-10, 10);
            assert!((-10..=10).contains(&v));
        }
    }

    #[test]
    fn lcg_gen_unit_f64_stays_in_unit_interval() {
        let mut rng = Lcg::new(456);
        for _ in 0..1000 {
            let v = rng.gen_unit_f64();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn int_gen_shrinks_toward_zero() {
        let g = IntGen {
            min: -100,
            max: 100,
        };
        let shrunk = g.shrink(&50);
        assert!(shrunk.contains(&0), "shrink(50) should suggest 0");
    }

    #[test]
    fn bool_gen_shrinks_true_to_false() {
        let g = BoolGen;
        assert_eq!(g.shrink(&true), vec![false]);
        assert!(g.shrink(&false).is_empty());
    }

    #[test]
    fn property_passes_for_universal_truth() {
        let config = Config::default();
        let g = IntGen { min: 0, max: 1000 };
        let outcome = run_property(&config, &g, |x: &i64| *x >= 0, "non-negative");
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn property_finds_counterexample_when_false() {
        let config = Config {
            cases: 100,
            seed: 7,
            shrink_rounds: 32,
        };
        // Bogus property : "every int is even".
        let g = IntGen { min: 0, max: 1000 };
        let outcome = run_property(&config, &g, |x: &i64| x % 2 == 0, "every int is even");
        match outcome {
            Outcome::Counterexample { .. } => {}
            other => panic!("expected Counterexample, got {other:?}"),
        }
    }

    #[test]
    fn property_shrinks_int_counterexample_toward_small_odd() {
        let config = Config {
            cases: 1000,
            seed: 42,
            shrink_rounds: 64,
        };
        let g = IntGen {
            min: -100,
            max: 100,
        };
        // "Every int is even" — any odd int fails.
        let outcome = run_property(&config, &g, |x: &i64| x % 2 == 0, "every int is even");
        match outcome {
            Outcome::Counterexample { shrunk_input, .. } => {
                // Parse the debug-form back to an int and check it's odd.
                let parsed: i64 = shrunk_input.parse().expect("parse");
                assert!(parsed % 2 != 0, "shrunk input must be odd");
                // Should be near 0 after shrinking (|v| ≤ 5 is generous).
                assert!(
                    parsed.abs() <= 5,
                    "expected small shrink-result, got {parsed}"
                );
            }
            other => panic!("expected Counterexample, got {other:?}"),
        }
    }

    #[test]
    fn property_bool_all_true_finds_false() {
        let config = Config {
            cases: 1000,
            seed: 11,
            shrink_rounds: 8,
        };
        let outcome = run_property(&config, &BoolGen, |b: &bool| *b, "always-true");
        match outcome {
            Outcome::Counterexample { shrunk_input, .. } => {
                assert_eq!(shrunk_input, "false");
            }
            other => panic!("expected Counterexample, got {other:?}"),
        }
    }

    #[test]
    fn property_same_seed_reproduces_same_counterexample() {
        let config = Config {
            cases: 100,
            seed: 99,
            shrink_rounds: 0, // disable shrinking to make the raw-generate stable
        };
        let g = IntGen { min: -50, max: 50 };
        let o1 = run_property(&config, &g, |x: &i64| x % 3 == 0, "divisible-by-3");
        let o2 = run_property(&config, &g, |x: &i64| x % 3 == 0, "divisible-by-3");
        assert_eq!(o1, o2);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § FloatGen + TripleGen + VecGen : refinement-guided generator suite.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn float_gen_stays_in_range() {
        let g = FloatGen {
            min: -10.0,
            max: 10.0,
        };
        let mut rng = Lcg::new(42);
        for _ in 0..1000 {
            let v = g.generate(&mut rng);
            assert!((-10.0..10.0).contains(&v));
        }
    }

    #[test]
    fn float_gen_shrinks_toward_zero() {
        let g = FloatGen {
            min: -100.0,
            max: 100.0,
        };
        let shrunk = g.shrink(&50.0);
        assert!(shrunk.contains(&0.0), "shrink(50.0) should suggest 0.0");
        assert!(
            shrunk.contains(&25.0),
            "shrink(50.0) should suggest 25.0 (halved)"
        );
    }

    #[test]
    fn float_gen_shrink_at_zero_is_empty() {
        let g = FloatGen {
            min: -1.0,
            max: 1.0,
        };
        let shrunk = g.shrink(&0.0);
        // At 0 : no zero-candidate (already there) + halved 0.0 is also 0.0 (deduped).
        assert!(shrunk.is_empty());
    }

    #[test]
    fn float_gen_positive_range_shrinks_to_min_if_zero_out_of_range() {
        let g = FloatGen {
            min: 1.0,
            max: 10.0,
        };
        // 0.0 not in range ; halved 5.0 = 2.5 which IS in [1.0, 10.0).
        let shrunk = g.shrink(&5.0);
        assert!(
            !shrunk.contains(&0.0),
            "0.0 not in range — should not be suggested"
        );
        assert!(shrunk.contains(&2.5));
    }

    #[test]
    fn triple_gen_produces_three_independent_samples() {
        let g = TripleGen {
            inner: IntGen { min: 0, max: 100 },
        };
        let mut rng = Lcg::new(123);
        let (a, b, c) = g.generate(&mut rng);
        assert!((0..=100).contains(&a));
        assert!((0..=100).contains(&b));
        assert!((0..=100).contains(&c));
    }

    #[test]
    fn triple_gen_shrinks_component_at_a_time() {
        let g = TripleGen {
            inner: IntGen { min: -50, max: 50 },
        };
        let shrunk = g.shrink(&(10i64, 20i64, 30i64));
        assert!(!shrunk.is_empty(), "should produce shrink candidates");
        // Each candidate changes exactly one of the three fields.
        for (a, b, c) in &shrunk {
            let diff_count = u32::from(*a != 10) + u32::from(*b != 20) + u32::from(*c != 30);
            assert!(
                diff_count == 1,
                "candidate {a},{b},{c} should change exactly one component, changed {diff_count}"
            );
        }
    }

    #[test]
    fn vec_gen_respects_max_length() {
        let g = VecGen {
            inner: IntGen { min: 0, max: 10 },
            max_len: 5,
        };
        let mut rng = Lcg::new(7);
        for _ in 0..100 {
            let v = g.generate(&mut rng);
            assert!(v.len() <= 5);
        }
    }

    #[test]
    fn vec_gen_zero_max_len_produces_empty() {
        let g: VecGen<IntGen> = VecGen {
            inner: IntGen { min: 0, max: 10 },
            max_len: 0,
        };
        let mut rng = Lcg::new(9);
        let v = g.generate(&mut rng);
        assert!(v.is_empty());
    }

    #[test]
    fn vec_gen_shrinks_by_truncation_first() {
        let g = VecGen {
            inner: IntGen { min: 0, max: 10 },
            max_len: 20,
        };
        let shrunk = g.shrink(&vec![1i64, 2, 3, 4, 5, 6, 7, 8]);
        // First candidate = half-truncation (length 4).
        assert_eq!(shrunk[0].len(), 4);
        // Second candidate = drop-last (length 7).
        assert_eq!(shrunk[1].len(), 7);
    }

    #[test]
    fn vec_gen_empty_shrink_is_empty() {
        let g = VecGen {
            inner: IntGen { min: 0, max: 10 },
            max_len: 5,
        };
        let v: Vec<i64> = Vec::new();
        let shrunk = g.shrink(&v);
        assert!(shrunk.is_empty());
    }

    #[test]
    fn float_gen_run_property_passes_for_universal_truth() {
        let g = FloatGen {
            min: 0.0,
            max: 100.0,
        };
        let config = Config {
            cases: 500,
            seed: 13,
            shrink_rounds: 8,
        };
        let outcome = run_property(&config, &g, |x: &f64| *x >= 0.0, "non-negative");
        assert!(matches!(outcome, Outcome::Ok { .. }));
    }

    #[test]
    fn triple_gen_run_property_shrinks_component() {
        let g = TripleGen {
            inner: IntGen { min: 0, max: 100 },
        };
        let config = Config {
            cases: 200,
            seed: 17,
            shrink_rounds: 16,
        };
        // Property : at least one component is zero (false in general).
        let outcome = run_property(
            &config,
            &g,
            |t: &(i64, i64, i64)| t.0 == 0 || t.1 == 0 || t.2 == 0,
            "at-least-one-component-is-zero",
        );
        // Almost certainly fails somewhere in 200 draws from [0,100]^3.
        match outcome {
            Outcome::Counterexample { shrunk_input, .. } => {
                // Shrunk form should print all three components.
                assert!(shrunk_input.starts_with('('));
                assert!(shrunk_input.ends_with(')'));
            }
            Outcome::Ok { .. } => {
                // Rare but possible ; don't fail the test just because the
                // property happened to hold on this seed.
            }
            Outcome::Stage0Unimplemented => panic!("runner should not return stub"),
        }
    }
}
