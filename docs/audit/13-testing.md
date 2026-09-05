# Audit: cssl-testing crate

**Auditor:** Claude (Sonnet 4.6)
**Date:** 2026-05-14
**Repo root:** `compiler-rs/crates/cssl-testing/`
**Design authority:** `specs/23_TESTING.csl`

---

## 1. Crate Overview

`cssl-testing` is the oracle-test-modes dispatcher and testing/verification infrastructure for the CSSLv3 stage-0 (Rust-hosted) bootstrap compiler. It implements the full suite of oracle modes described in `specs/23_TESTING.csl` — a specification that treats testing as "multi-oracle against the same source," meaning each function under test can be exercised by several independent verification strategies simultaneously.

### The Oracle-Mode Concept

Rather than writing ad hoc test code, CSSLv3's design intends that test attributes (`@property`, `@differential`, `@bench`, etc.) written in CSSLv3 surface syntax map 1:1 to oracle modes in this crate. The `OracleMode` enum in `oracle.rs` is the authoritative registry — every attribute variant has exactly one corresponding module with a `Config` struct, an `Outcome` enum, a `Dispatcher` trait, and a `Stage0Stub` unit-struct. During the stage-0 compiler phase, stubs return `Outcome::Stage0Unimplemented`; the T11 development phase replaces these with real runners.

### Relationship Between Strategies

The twelve oracle modes group into three tiers by their current implementation state:

**Tier 1 — Fully live (T11-phase-2b):**
- `property` — QuickCheck-lineage random testing with a custom LCG PRNG, typed generators (int, bool, float, triple, vec, refined), shrinking, and a generic `run_property` function.
- `metamorphic` — algebraic law checking (commutativity, associativity, distributivity, idempotence, Leibniz product rule, chain rule, Lipschitz) via generic, higher-order verifiers.
- `fuzz` — dumb-mode (no coverage guidance) byte-fuzzing with LCG-driven corpus generation, `catch_unwind` panic capture, and greedy byte-level shrinking.
- `bench` — wall-clock timing over N runs, median computation, baseline-file read/write, ±threshold regression classification.
- `golden` — byte-exact / byte-diff-pct comparison to golden fixtures; SSIM and FLIP fields reserved but zeroed pending image decode.
- `differential` — abstract two-implementation comparator with ULP-difference helpers for float outputs; real GPU backend dispatch deferred.
- `r16_attestation` — BLAKE3 canonical serialization + Ed25519 sign/verify + `decide_attestation` helper; live crypto, stage3 rebuild pipeline pending.
- `replay` — seed-based N-repeat determinism testing using the shared `Lcg` PRNG.
- `audit` — AuditChain structural verification (hash-linkage + signature-chain + required-event filtering) via `cssl-telemetry`.

**Tier 2 — Stub only:**
- `power` — `Stage0Stub` only; needs `cssl-host-level-zero` sysman.
- `thermal` — `Stage0Stub` only; needs `zesTemperatureGetState` via sysman.
- `hot_reload` — `Stage0Stub` only; needs `cssl-persist`.

**Adjunct (not oracle-enum variants):**
- `metrics` — data structures (`FrequencySample`, `LatencyPercentiles`, `MetricsSnapshot`) consumed by bench/stress oracles.
- `r16_attestation` — the R16/OG10 reproducibility attestation hook (listed above in Tier 1).

### Role in CI

As designed in `specs/23_TESTING.csl`, the crate is meant to back a `csslc test` command with a per-commit fast suite (unit + differential-primary, ≤10 min), per-PR full suite (all oracles, ≤45 min), and nightly extended suite (fuzz + stress). At stage-0 the real CI driver is `cargo test`, which runs all the in-module `#[test]` blocks directly. No integration-test files exist outside `src/`.

### Maturity Assessment

The crate is at a well-structured scaffold-to-live transition point. Eight of the twelve modes have working implementations with meaningful test coverage; four (power, thermal, hot_reload, plus the cross-backend and cross-machine paths of differential/replay) remain stubs. The stub/live split is explicitly documented in every module's header comment. The pattern is disciplined and mechanically consistent throughout.

---

## 2. Crate Metadata

| Field | Value |
|---|---|
| Path | `compiler-rs/crates/cssl-testing/` |
| Crate name | `cssl-testing` |
| Description | `CSSLv3 stage0 — §§ 23 oracle-modes + golden-fixture + differential-backend harness` |
| Version | workspace-inherited |
| Edition | workspace-inherited |
| License | workspace-inherited |
| `#[forbid(unsafe_code)]` | yes |

### Cargo.toml Dependencies

```toml
[dependencies]
cssl-telemetry = { path = "../cssl-telemetry" }
```

No external crates (no `proptest`, no `insta`, no `criterion`). The crate implements its own PRNG (`Lcg`), shrinking, and timing. The only dependency is the sibling `cssl-telemetry` crate, used by `audit.rs` (`AuditChain`, `AuditError`) and `r16_attestation.rs` (`ContentHash`, `Signature`, `SigningKey`).

### Total LOC and File List

| File | Approx. LOC |
|---|---|
| `src/property.rs` | 851 |
| `src/metamorphic.rs` | 516 |
| `src/fuzz.rs` | 315 |
| `src/bench.rs` | 294 |
| `src/golden.rs` | 294 |
| `src/differential.rs` | 272 |
| `src/r16_attestation.rs` | 258 |
| `src/replay.rs` | 228 |
| `src/audit.rs` | 217 |
| `src/oracle.rs` | 118 |
| `src/power.rs` | 88 |
| `src/thermal.rs` | 80 |
| `src/lib.rs` | 80 |
| `src/metrics.rs` | 61 |
| `src/hot_reload.rs` | 58 |
| **Total** | **~3,730** |

No `tests/` directory exists.

---

## 3. Per-File Analysis

Files are presented in dependency/conceptual order: `lib.rs`, `oracle.rs`, `metrics.rs`, then each test-mode module, then the adjunct modules.

---

### 3.1 `src/lib.rs` (80 lines)

**Purpose:** Crate root. Declares all submodules as `pub mod`, re-exports `OracleMode`, defines two crate-level constants, and contains a small `scaffold_tests` module that sanity-checks the oracle registry.

#### Items

**`pub const STAGE0_SCAFFOLD: &str`** (`lib.rs:50`)
Expands to `CARGO_PKG_VERSION` via `env!`. Serves as a sentinel that the crate built; also used by the scaffold test below.

**`pub const ORACLE_MODE_COUNT: usize`** (`lib.rs:54`)
Hard-coded to `12`. Must equal `OracleMode::ALL.len()`. Asserted in `oracle_mode_count_matches_registry`.

**`pub use oracle::OracleMode`** (`lib.rs:47`)
Flat re-export so callers can write `cssl_testing::OracleMode` without the module path.

**Compiler lint flags** (`lib.rs:28-30`)
- `#![forbid(unsafe_code)]` — no unsafe anywhere in the crate.
- `#![deny(rustdoc::broken_intra_doc_links)]`
- `#![deny(rustdoc::private_intra_doc_links)]`

#### `#[cfg(test)] mod scaffold_tests` (`lib.rs:57-79`)

Three scaffold tests:

| Test | Assertion |
|---|---|
| `scaffold_version_present` | `STAGE0_SCAFFOLD` is non-empty |
| `oracle_mode_count_matches_registry` | `OracleMode::ALL.len() == ORACLE_MODE_COUNT` |
| `every_oracle_mode_has_display_name` | no mode has an empty `display_name()` |

#### Cross-References
All thirteen submodules are declared here. The `oracle_mode_count_matches_registry` test links `lib.rs` constants to `oracle.rs`'s `ALL` slice — a structural self-integrity check.

#### TODOs / Stubs
None in `lib.rs` itself; every stub is in the submodules.

---

### 3.2 `src/oracle.rs` (118 lines)

**Purpose:** The oracle-mode registry. Defines `OracleMode` — the single enum that 1:1 maps every `@<attr>` CSSLv3 test attribute to a named variant. All other modules are subordinate to this enum.

#### Items

**`pub enum OracleMode`** (`oracle.rs:10-36`)
Derives `Debug, Clone, Copy, PartialEq, Eq, Hash`. Twelve variants:

| Variant | Attribute | Module |
|---|---|---|
| `Unit` | `@test` | (std `#[test]`) |
| `Property` | `@property` | `property.rs` |
| `Differential` | `@differential` | `differential.rs` |
| `Metamorphic` | `@metamorphic` | `metamorphic.rs` |
| `Bench` | `@bench` | `bench.rs` |
| `PowerBench` | `@power_bench` | `power.rs` |
| `ThermalStress` | `@thermal_stress` | `thermal.rs` |
| `Replay` | `@replay` | `replay.rs` |
| `HotReload` | `@hot_reload_test` | `hot_reload.rs` |
| `Fuzz` | `@fuzz` | `fuzz.rs` |
| `Golden` | `@golden` | `golden.rs` |
| `Audit` | `@audit_test` | `audit.rs` |

**`impl OracleMode`**

- **`pub const ALL: &'static [Self]`** (`oracle.rs:40-53`) — All twelve variants in registration order. Length enforced at compile-time by `lib.rs`'s scaffold test.
- **`pub const fn display_name(self) -> &'static str`** (`oracle.rs:56-71`) — CLI-flag style name (no `@` prefix). `const fn` — usable in static contexts.
- **`pub const fn attribute_form(self) -> &'static str`** (`oracle.rs:74-89`) — The `@<attr>` surface-syntax form. Always starts with `@`.

**`impl fmt::Display for OracleMode`** (`oracle.rs:92-96`) — delegates to `display_name()`.

#### `#[cfg(test)] mod tests` (`oracle.rs:98-117`)

| Test | Assertion |
|---|---|
| `display_matches_attribute_form` | every mode: `attribute_form` starts with `@`; `display_name` does not |
| `all_modes_are_unique` | no duplicate variants in `ALL` |

#### Notable Algorithm
The oracle-dispatch design: there is no runtime dispatch table here. `OracleMode` is a data type; the actual dispatch is performed by calling the relevant module's `Dispatcher::run`. The `attribute_form()` method bridges the surface-syntax to the enum for future csslc-parser integration.

#### TODOs / Stubs
None. This file is complete for its scope.

---

### 3.3 `src/metrics.rs` (61 lines)

**Purpose:** Shared observability data structures consumed by bench, power, and thermal oracle modes. Not an oracle mode itself — purely data.

#### Items

**`pub struct FrequencySample`** (`metrics.rs:11-21`)
Four `f32` fields: `mean_mhz`, `stdev_mhz`, `min_mhz`, `max_mhz`. Derives `Debug, Clone, Copy, Default, PartialEq`. Intended to hold output from `zesFrequencyGetState` sampled at 100Hz. Spec targets: `stdev/mean < 0.05` (5% stability); `min_mhz > 90%` of nominal base-clock.

**`pub struct LatencyPercentiles`** (`metrics.rs:23-33`)
Four `u64` fields: `p50_ns`, `p90_ns`, `p99_ns`, `p99_9_ns`. Spec target: `p99_ns < 1µs` for Level-Zero immediate command lists.

**`pub struct MetricsSnapshot`** (`metrics.rs:36-43`)
Composite of `FrequencySample` + `LatencyPercentiles`. All-fields re-derived from the two inner structs. Consumed by bench/stress oracle modes.

#### `#[cfg(test)] mod tests` (`metrics.rs:44-60`)

Single test `defaults_are_zero`: verifies that `Default` gives zero-valued fields. Uses bit-pattern comparison for `f32` to avoid `clippy::float_cmp`.

#### TODOs / Stubs
Module header says "T11 stub — structs wired; sampling pipeline pending." The structs exist and are correct data shapes; no sampling pipeline (no connection to sysman or ring-buffer) is present.

---

### 3.4 `src/property.rs` (851 lines)

**Purpose:** The `@property` oracle — QuickCheck-lineage property-based testing. This is the largest and most fully implemented module. Contains a custom deterministic PRNG (`Lcg`), typed generators, refinement-guided generation, a generic property runner, and a greedy shrinker.

#### Items

**`pub struct Config`** (`property.rs:13-20`)
- `cases: u32` — number of generated inputs per run. Default 1000; spec says 10000 in `nightly-extended`.
- `seed: u64` — deterministic seed (`0xc551_a770_c551_a770_u64` default). Same seed + same generator = identical input sequence.
- `shrink_rounds: u32` — max greedy shrink iterations after a counterexample is found. Default 64; 0 disables shrinking.

**`pub enum Outcome`** (`property.rs:33-45`)
- `Stage0Unimplemented` — legacy stub variant (still present for `Stage0Stub`).
- `Ok { cases_run: u32 }` — all cases passed.
- `Counterexample { shrunk_input: String, message: String }` — failure found and shrunk.

**`pub trait Dispatcher`** (`property.rs:47-50`)
`fn run(&self, config: &Config) -> Outcome;` — the oracle interface.

**`pub struct Stage0Stub`** (`property.rs:53-54`)
Unit-struct. `impl Dispatcher` returns `Outcome::Stage0Unimplemented`.

#### PRNG: `pub struct Lcg` (`property.rs:69-117`)

A 64-bit linear congruential generator. Fields: `state: u64` (private).

| Method | Signature | Description |
|---|---|---|
| `new` | `pub const fn new(seed: u64) -> Self` | Seed the LCG. |
| `next_u64` | `pub fn next_u64(&mut self) -> u64` | Advance state with constants from Knuth (Numerical Recipes): `a = 6364136223846793005`, `c = 1442695040888963407`. |
| `gen_i64` | `pub fn gen_i64(&mut self, min: i64, max: i64) -> i64` | Uniform `[min, max]`. Uses `i128` widening to avoid range overflow. |
| `gen_bool` | `pub fn gen_bool(&mut self) -> bool` | Low bit of `next_u64()`. |
| `gen_unit_f64` | `pub fn gen_unit_f64(&mut self) -> f64` | 53-bit mantissa shifted into `[0.0, 1.0)`. |
| `gen_f64` | `pub fn gen_f64(&mut self, min: f64, max: f64) -> f64` | Scales `gen_unit_f64()` into `[min, max)` using `mul_add`. |

The `Lcg` type is `pub` and re-used by `fuzz.rs` and `replay.rs` (imported as `crate::property::Lcg`).

#### Generator Trait: `pub trait Generator<T>` (`property.rs:127-137`)

```rust
pub trait Generator<T: core::fmt::Debug + Clone> {
    fn generate(&self, rng: &mut Lcg) -> T;
    fn shrink(&self, _v: &T) -> Vec<T> { Vec::new() }
}
```

Default `shrink` returns empty (no shrinking); overrides provide type-specific paths.

#### Concrete Generators

**`pub struct IntGen`** (`property.rs:140-165`)
Fields: `pub min: i64`, `pub max: i64`. Implements `Generator<i64>`. Shrinks toward 0 (if in range), then halving, then ±1.

**`pub struct BoolGen`** (`property.rs:168-183`)
Unit-struct with `Default`. Shrinks `true → [false]`, `false → []`.

**`pub struct FloatGen`** (`property.rs:188-212`)
Fields: `pub min: f64`, `pub max: f64`. Implements `Generator<f64>`. Shrinks toward 0 (if in range), then halved-magnitude. `#[allow(clippy::float_cmp)]` on `shrink` justified: exact equality tests are intentional (checking `v == 0.0` and `halved == *v`).

**`pub struct TripleGen<G>`** (`property.rs:215-246`)
Generic wrapper producing `(T, T, T)` from an inner `Generator<T>`. Shrinks one component at a time (three shrink-chains interleaved).

**`pub struct VecGen<G>`** (`property.rs:249-295`)
Fields: `pub inner: G`, `pub max_len: u32`. Generates `Vec<T>` of length `[0, max_len]`. Shrinks by: half-truncation, drop-last, and last-element inner-shrink.

#### Refinement Generator

**`pub struct RefinedGen<G, P>`** (`property.rs:316-359`)
Fields: `pub inner: G`, `pub predicate: P`, `pub max_attempts: u32` (default 100 via `new`).

- `generate()` calls `inner.generate()` until `predicate(v)` or attempts exhausted; returns last value on exhaustion (caller must handle invalid refinement boundary).
- `shrink()` delegates to `inner.shrink()` then filters through `predicate` — guaranteeing all shrink candidates satisfy the refinement.
- Constructor `pub const fn new(inner: G, predicate: P) -> Self` sets `max_attempts = 100`.

#### Property Runner

**`pub fn run_property<T, G, F>(config: &Config, generator: &G, check: F, label: &str) -> Outcome`** (`property.rs:377-397`)

Top-level function. Seeds an `Lcg`, iterates `config.cases` times: generates an input, calls `check(&input)`. On failure at case `i`, calls `shrink_counterexample` and returns `Outcome::Counterexample { shrunk_input, message }`. On all-pass, returns `Outcome::Ok { cases_run: config.cases }`.

**`fn shrink_counterexample<T, G, F>(...) -> T`** (`property.rs:401-423`, private)

Greedy shrinker. For up to `max_rounds` iterations, calls `generator.shrink(&current)`, tries each candidate under `check`; keeps the first still-failing candidate. Halts when a round yields no improvement.

#### `#[cfg(test)] mod tests` (`property.rs:425-851`)

32 tests covering:
- `Stage0Stub` behavior
- LCG reproducibility and range bounds
- All generator types (generate + shrink)
- `run_property` pass and counterexample paths
- Shrunk counterexample quality (small odd number for "even" property)
- Replay reproducibility (same seed = same outcome)
- `RefinedGen` predicate satisfaction, failed-exhaustion fallback, shrink-filter

Notable test: `property_shrinks_int_counterexample_toward_small_odd` (`property.rs:516`) asserts `|shrunk_int| <= 5` — this is a behavioral contract on the shrinker quality.

#### TODOs / Stubs
None explicit in `property.rs`. The module header says "T11-phase-2b live implementation."

#### Potential Issue
`RefinedGen::generate` documents that on `max_attempts` exhaustion it returns the last generated value, which may not satisfy `predicate`. There is no warning or error emitted. A caller using `RefinedGen` with a highly-selective predicate over a poorly-ranged inner generator will silently get invalid inputs into the property check — this could produce false-pass results (the predicate being checked on a value the refinement type would reject).

---

### 3.5 `src/metamorphic.rs` (516 lines)

**Purpose:** The `@metamorphic` oracle — algebraic law preservation. Provides a `Law` enum classifying known algebraic properties, generic higher-order verifiers for each law class, and numeric calculus-rule validators.

#### Items

**`pub enum Law`** (`metamorphic.rs:9-27`)
Eight variants: `Commutative`, `Associative`, `Distributive`, `Leibniz`, `FaaDiBruno`, `Lipschitz`, `Conservation`, `Custom`. Derives `Debug, Clone, Copy, PartialEq, Eq, Hash`.

**`pub struct Config`** (`metamorphic.rs:29-38`)
- `law: Law` — which law to verify. Default `Law::Commutative`.
- `custom_name: Option<String>` — discriminator when `law == Law::Custom`.
- `samples: u32` — default 256.

**`pub enum Outcome`** (`metamorphic.rs:51-59`)
- `Stage0Unimplemented`
- `Ok { samples_tested: u32 }`
- `Violation { sample: String, message: String }`

**`pub trait Dispatcher`** (`metamorphic.rs:61-64`)**
`fn run(&self, config: &Config) -> Outcome;`

**`pub struct Stage0Stub`** (`metamorphic.rs:67-74`)
Returns `Outcome::Stage0Unimplemented`.

#### Algebraic Verifiers (all `pub fn`)

**`pub fn check_commutative<T, Op, Eq>(samples: &[(T, T)], op: Op, eq: Eq) -> Outcome`** (`metamorphic.rs:86-107`)
Checks `op(a, b) == op(b, a)` for all sample pairs. Returns `Violation` with first failing pair's debug-formatted `(a, b)`.

**`pub fn check_associative<T, Op, Eq>(samples: &[(T, T, T)], op: Op, eq: Eq) -> Outcome`** (`metamorphic.rs:111-134`)
Checks `(a·b)·c == a·(b·c)` for all triples.

**`pub fn check_distributive<T, Mul, Add, Eq>(samples: &[(T, T, T)], mul: Mul, add: Add, eq: Eq) -> Outcome`** (`metamorphic.rs:139-169`)
Checks `mul(a, add(b, c)) == add(mul(a, b), mul(a, c))`.

**`pub fn check_idempotent<T, Op, Eq>(samples: &[T], op: Op, eq: Eq) -> Outcome`** (`metamorphic.rs:172-193`)
Checks `op(op(x)) == op(x)` for all samples.

#### Calculus-Rule Validators (all `pub fn`)

**`pub fn check_leibniz<F, DF, G, DG>(samples: &[f64], f, df, g, dg, tolerance: f64) -> Outcome`** (`metamorphic.rs:210-247`)
Numerically verifies the Leibniz product rule `(f·g)'(x) = f'(x)·g(x) + f(x)·g'(x)`. LHS computed via central-differences with adaptive step `h = max(1e-5, |x|*1e-6)`. `#[allow(clippy::similar_names)]` justified by calculus notation.

**`pub fn check_chain_rule<F, DF, G, DG>(samples: &[f64], f, df, g, dg, tolerance: f64) -> Outcome`** (`metamorphic.rs:253-288`)
Numerically verifies `(f∘g)'(x) = f'(g(x)) · g'(x)`. Same central-difference approach.

**`pub fn check_lipschitz<F>(samples: &[(f64, f64)], f: F, k: f64) -> Outcome`** (`metamorphic.rs:294-315`)
Checks `|f(x) - f(y)| <= k * |x - y|` for all sample pairs. Directly compares floating-point magnitudes; no tolerance (the spec requires exact Lipschitz).

#### `#[cfg(test)] mod tests` (`metamorphic.rs:317-515`)

25 tests:
- `stub_returns_unimplemented`
- Commutativity: addition passes, subtraction violates (non-self-symmetric input pair)
- Associativity: addition passes
- Distributivity: integer mul/add passes
- Idempotence: identity function trivially passes
- Boolean-AND commutativity
- `Violation` message-shape checks (verifies `sample.contains("a=")` etc.)
- Empty-sample edge case: `Ok { samples_tested: 0 }`
- Leibniz rule: polynomial product x²(x+1) passes; wrong derivative fails
- Chain rule: sin(x²) passes; wrong inner derivative fails
- Lipschitz: linear function passes with exact K; sine passes with K=1; 100x fails with K=1

#### Notable Issues / Observations

**Idempotence test comment** (`metamorphic.rs:368-378`): The test comment contains a correct self-correction — it notes that `|x| !x` is NOT idempotent (it's its own inverse), and uses identity instead. This shows the test author caught a conceptual error during writing, but the final test is trivially true (identity is always idempotent). The idempotence verifier is not tested on a genuinely non-trivial idempotent operation (e.g., `|x| x.max(0)` would be more meaningful). This is a test quality gap, not a bug.

**Lipschitz check has no tolerance** (`metamorphic.rs:301-302`): `check_lipschitz` uses `lhs > rhs` with no floating-point tolerance. For exactly-at-boundary functions (e.g., `f(x) = k*x`), floating-point arithmetic can produce `lhs` infinitesimally greater than `rhs` even when mathematically equal, producing false violations. The test coverage for Lipschitz only uses clean integer-scaled functions, so this latent issue is not caught by the current tests.

**`FaaDiBruno` and `Conservation` laws** (`metamorphic.rs:19-24`): These two `Law` enum variants have no corresponding verifier functions. Only `Commutative`, `Associative`, `Distributive`, `Leibniz`, `Lipschitz` (and trivially `Idempotent`, `ChainRule`) have implementations. `FaaDiBruno`, `Conservation`, and `Custom` are registered but unimplemented. No `todo!()` markers are present — the gap is silent.

---

### 3.6 `src/fuzz.rs` (315 lines)

**Purpose:** The `@fuzz` oracle — dumb-mode byte fuzzer backed by the shared `Lcg`. Implements budget-bounded fuzzing with panic catching and greedy byte-level shrinking. Coverage guidance and SMT-oracle hookup are explicitly deferred.

#### Items

**`pub struct Config`** (`fuzz.rs:19-32`)
- `budget: Duration` — default 600s (10 min per `specs/23`).
- `smt_oracle: bool` — flag for SMT-oracle hookup; field exists, no code acts on it yet.
- `min_exec_per_sec: u32` — throughput target for CI regression on fuzz infra; field exists, not enforced.
- `seed: u64` — same canonical seed as property (`0xc551_a770_c551_a770_u64`).
- `max_input_len: usize` — default 1024 bytes.
- `shrink_rounds: u32` — default 32.

**`pub enum Outcome`** (`fuzz.rs:49-59`)
- `Stage0Unimplemented`
- `Ok { total_execs: u64 }` — fuzzing ran to budget without finding failures.
- `Counterexample { shrunk_input: String, message: String }`

**`pub trait Dispatcher`** (`fuzz.rs:62-64`)**
`fn run(&self, config: &Config) -> Outcome;`

**`pub struct Stage0Stub`** (`fuzz.rs:67-74`)**
Returns `Outcome::Stage0Unimplemented`.

#### Live Fuzzer

**`pub fn run_fuzz_dumb<F>(config: &Config, mut check: F) -> Outcome`** (`fuzz.rs:92-120`)
Main fuzzer loop. Checks deadline every 256 iterations (to amortize `Instant::now()` cost). Uses `std::panic::catch_unwind(AssertUnwindSafe(...))` to catch `check` panics — both `false` return and `Err(_)` (panic) count as failures. On failure, calls `shrink_input` and returns `Counterexample`.

Bound: `F: FnMut(&[u8]) -> bool + core::panic::RefUnwindSafe` — the `RefUnwindSafe` bound is required for `catch_unwind`.

**`fn generate_bytes(rng: &mut Lcg, max_len: usize) -> Vec<u8>`** (`fuzz.rs:125-138`, private)
Draws a single raw `u64` for length (uniform over `[0, max_len]`), then draws one byte per position from the low 8 bits of `rng.next_u64()`.

**`fn shrink_input<F>(start: &[u8], check: &mut F, max_rounds: u32) -> Vec<u8>`** (`fuzz.rs:143-172`, private)
Greedy shrinker over byte slices. On each round, tries `shrink_candidates` in order; keeps the first smaller candidate that still fails. Halts on no improvement.

**`fn shrink_candidates(bytes: &[u8]) -> Vec<Vec<u8>>`** (`fuzz.rs:176-189`, private)
Generates three candidates: right-half truncation, drop-first-byte, drop-last-byte. Returns empty for length-0 input.

#### `#[cfg(test)] mod tests` (`fuzz.rs:191-314`)

8 tests:
- `stub_returns_unimplemented`
- `always_ok_check_never_finds_counterexample` — 50ms budget, all-pass check
- `return_false_counts_as_failure` — rejects non-empty inputs
- `panic_is_caught_as_counterexample` — `assert!(bytes.is_empty())` caught
- `zero_max_input_len_only_produces_empty_inputs` — forced empty corpus
- `shrink_reduces_counterexample_size` — failure-on-len>1, verifies shrunk output is short
- `zero_budget_still_runs_at_least_once` — deadline check is at iteration 256

#### Potential Issues

**`smt_oracle` field is checked nowhere** (`fuzz.rs:22`): The `Config` field exists but `run_fuzz_dumb` does not read it. SMT-oracle integration is documented as deferred to T11-phase-2c, but the field currently creates a misleading impression of capability.

**`min_exec_per_sec` unenforced** (`fuzz.rs:24`): Similarly, this throughput target is recorded but never measured or enforced. There is no post-run check that `total_execs / budget_secs >= min_exec_per_sec`.

**Byte-level shrinking is weak** (`fuzz.rs:176-189`): The three-candidate set (halve, drop-first, drop-last) can get stuck if the minimal failing input is not reachable by repeated application of these three moves. For example, the minimal failing input `[0x41]` might not be reachable from `[0x42, 0x43, ...]` via truncation alone. The shrinker does not try single-byte mutations (zeroing bytes, reducing byte values). This is by design for stage-0 dumb-mode but worth flagging for T11-phase-2c.

---

### 3.7 `src/bench.rs` (294 lines)

**Purpose:** The `@bench` oracle — wall-clock performance measurement with baseline tracking and regression detection.

#### Items

**`pub struct Config`** (`bench.rs:16-23`)
- `bench_id: String` — identifier for `<baseline_root>/<bench_id>/latest.txt`. Default empty.
- `runs: u32` — number of measurement repetitions. Default 10.
- `regression_threshold: f64` — fractional tolerance. Default `0.10` (±10%).

**`pub enum Outcome`** (`bench.rs:37-50`)
- `Stage0Unimplemented`
- `Ok { median_ns: u64, baseline_ns: u64 }`
- `Regressed { median_ns: u64, baseline_ns: u64, delta_pct: f64 }`
- `NoBaseline { median_ns: u64 }` — baseline file absent (first-run path)

**`pub trait Dispatcher`** (`bench.rs:53-55`)**
`fn run(&self, config: &Config) -> Outcome;`

**`pub struct Stage0Stub`** (`bench.rs:58-65`)**
Returns `Outcome::Stage0Unimplemented`.

#### Live Bench Harness

**`pub fn run_bench_vs_baseline<F>(config: &Config, baseline_root: &Path, mut f: F) -> Outcome`** (`bench.rs:85-98`)
Orchestrator. Calls `run_and_collect_samples`, then `median_ns`, then `read_baseline`. If no file: `NoBaseline`. Else: calls `classify`.

**`pub fn classify(median_ns: u64, baseline_ns: u64, threshold: f64) -> Outcome`** (`bench.rs:105-124`)
Pure classifier. Special case: `baseline_ns == 0` treated as first-run (returns `NoBaseline` to avoid divide-by-zero). Only positive delta triggers `Regressed` — improvement (faster than baseline) returns `Ok`. `#[allow(clippy::cast_precision_loss)]` justified: nanosecond counts fit in f64 for realistic bench durations.

**`fn run_and_collect_samples<F>(runs: u32, f: &mut F) -> Vec<u64>`** (`bench.rs:127-139`, private)
Iterates `runs` times; records `Instant::now()` before and `t0.elapsed()` after each call. Returns the per-run nanosecond latencies.

**`fn duration_to_ns(d: Duration) -> u64`** (`bench.rs:141-144`, private)
Converts `Duration::as_nanos() -> u128` to `u64` via `try_from`; saturates at `u64::MAX` for durations > ~584 years.

**`pub fn median_ns(samples: &[u64]) -> u64`** (`bench.rs:150-157`)
Sorts a clone of the slice, returns `sorted[len/2]` (lower midpoint for even-length slices). Returns 0 for empty slice.

**`fn baseline_path(root: &Path, bench_id: &str) -> PathBuf`** (`bench.rs:159-161`, private)
Constructs `root/bench_id/latest.txt`.

**`fn read_baseline(path: &Path) -> Option<u64>`** (`bench.rs:163-167`, private)
Reads the file, UTF-8 decodes, trims whitespace, parses as `u64`. Returns `None` on any error (file missing, parse error, etc.).

**`pub fn update_baseline(root: &Path, bench_id: &str, median_ns: u64) -> std::io::Result<()>`** (`bench.rs:173-179`)
Writes `median_ns.to_string()` to `baseline_path`. Creates parent directories via `create_dir_all`.

#### `#[cfg(test)] mod tests` (`bench.rs:181-294`)

9 tests:
- `stub_returns_unimplemented`
- `median_of_odd_length_returns_middle`
- `median_of_even_length_returns_upper_midpoint` — documents the lower-midpoint choice
- `median_of_empty_is_zero`
- `classify_within_tolerance_returns_ok`
- `classify_above_tolerance_returns_regressed`
- `classify_below_baseline_is_ok_not_regressed` — improvement is OK, not a regression
- `classify_zero_baseline_returns_no_baseline`
- `run_bench_with_no_baseline_file_returns_no_baseline` — uses temp dir
- `update_baseline_then_run_reads_it_back` — round-trip test; uses 50% threshold to absorb clock jitter

#### Observation

**Median implementation uses upper midpoint for even-length** (`bench.rs:151-156`): The comment says "returns the lower midpoint," but `sorted[sorted.len() / 2]` for `[1, 2, 3, 4]` returns `sorted[2] = 3` (the upper of the two middle elements). The test `median_of_even_length_returns_upper_midpoint` asserts `median_ns(&[1, 2, 3, 4]) == 3` and `median_ns(&[10, 20]) == 20`, both of which are the upper midpoints. The comment on line 155 ("returns the lower midpoint") is incorrect — it should say "upper midpoint." This is a documentation bug. The behavior is tested correctly and consistently; only the comment is wrong.

---

### 3.8 `src/golden.rs` (294 lines)

**Purpose:** The `@golden` oracle — golden-file fixture comparison. Implements byte-exact and byte-diff-pct comparison; SSIM and FLIP are defined but zeroed pending image-decode dependencies.

#### Items

**`pub struct Config`** (`golden.rs:17-27`)
- `path: String` — relative path to golden fixture.
- `ssim_threshold: f32` — structural similarity threshold. Default 0.99.
- `flip_threshold: f32` — FLIP metric threshold (lower = more similar). Default 0.05.
- `pixel_tolerance_pct: f32` — fraction of bytes/pixels allowed to differ. Default 0.001 (0.1%).

**`pub struct Metrics`** (`golden.rs:40-48`)
- `ssim: f32` — structural similarity. Set to `0.0` in current implementation (SSIM deferred).
- `flip: f32` — FLIP score. Set to `0.0` (deferred).
- `pixel_diff_pct: f32` — byte-diff fraction. Actually computed.

Derives `Debug, Clone, Copy, Default, PartialEq`.

**`pub enum Outcome`** (`golden.rs:51-64`)
- `Stage0Unimplemented`
- `Ok { metrics: Metrics }`
- `ThresholdExceeded { metrics: Metrics, breached: &'static str }` — `breached` is always `"byte-diff"` currently.
- `NoReference { path: String }` — golden file cannot be opened.

**`pub trait Dispatcher`** (`golden.rs:67-69`)**
`fn run(&self, config: &Config) -> Outcome;`

**`pub struct Stage0Stub`** (`golden.rs:72-79`)**
Returns `Outcome::Stage0Unimplemented`.

#### Live Functions

**`pub fn compare_bytes_to_golden(config: &Config, actual: &[u8]) -> Outcome`** (`golden.rs:97-104`)
Reads `config.path` from the filesystem. On failure: `NoReference`. On success: delegates to `compare_bytes_against`.

**`pub fn compare_bytes_against(config: &Config, actual: &[u8], expected: &[u8]) -> Outcome`** (`golden.rs:109-119`)
Pure (no filesystem). Calls `compute_byte_metrics`, then compares `pixel_diff_pct <= pixel_tolerance_pct`. `#[must_use]`.

**`pub fn compute_byte_metrics(actual: &[u8], expected: &[u8]) -> Metrics`** (`golden.rs:129-152`)
Computes byte-diff percentage. Both-empty special case returns `Metrics { ssim: 1.0, flip: 0.0, pixel_diff_pct: 0.0 }` (ssim=1 for identical). For non-empty: iterates `min_len` bytes comparing; adds `max_len - min_len` as extra diff for length mismatches. Normalizes by `max_len`. `ssim` and `flip` are always `0.0` for non-empty (deferred). `#[must_use]`.

**`pub fn update_golden(path: &str, bytes: &[u8]) -> std::io::Result<()>`** (`golden.rs:157-164`)
Creates parent directories if needed, writes bytes to path.

#### `#[cfg(test)] mod tests` (`golden.rs:166-293`)

9 tests:
- `stub_returns_unimplemented`
- `empty_buffers_are_identical`
- `identical_buffers_report_zero_diff`
- `one_byte_differs_out_of_ten_reports_ten_percent`
- `length_mismatch_counts_toward_diff`
- `within_tolerance_reports_ok`
- `above_tolerance_reports_breach`
- `missing_reference_reports_no_reference`
- `update_golden_roundtrip`
- `metrics_default_is_all_zero`

#### Notable Issue

**`ssim` field is misleading for empty buffers** (`golden.rs:130-135`): For both-empty case, `ssim` is set to `1.0` (conceptually correct: two empty buffers are "identical"). For any non-empty pair — including two identical non-empty buffers — `ssim` is `0.0`. This means `Metrics::ssim == 0.0` for `compare_bytes_against` on identical non-empty data, which is semantically wrong (SSIM of identical images is 1.0). The field is explicitly documented as "deferred," so this is by design, but callers who read `metrics.ssim` will get misleading values. The `ssim_threshold` in `Config` is also never checked (threshold logic only tests `pixel_diff_pct`).

**SSIM and FLIP thresholds are dead config** (`golden.rs:111-118`): `config.ssim_threshold` and `config.flip_threshold` are stored but never used in `compare_bytes_against`. The only threshold applied is `pixel_tolerance_pct`. This is documented as a T11-phase-2c gap, but could cause confusion for callers who set these thresholds expecting them to have effect.

---

### 3.9 `src/differential.rs` (272 lines)

**Purpose:** The `@differential` oracle — cross-backend bit-exact comparison. Provides a backend enum, a generic two-implementation comparator, and ULP-distance helpers for floating-point comparison.

#### Items

**`pub enum Backend`** (`differential.rs:12-26`)
Six variants: `Vulkan`, `LevelZero`, `D3d12`, `Metal`, `WebGpu`, `CpuRef`. `CpuRef` is the oracle implementation used as the ground truth. Derives `Debug, Clone, Copy, PartialEq, Eq, Hash`.

**`pub struct Config`** (`differential.rs:29-38`)
- `backends: Vec<Backend>` — list of backends to compare. Default `Vec::new()` (empty — stub state).
- `pure_det: bool` — if true, require byte-exact equality. Default true.
- `ulp_tolerance: u32` — ULP-allowed divergence for non-`pure_det`. Default 0.

**`pub enum Outcome`** (`differential.rs:52-63`)
- `Stage0Unimplemented`
- `Ok`
- `Divergence { backend: Backend, delta: String, message: String }` — reports which backend diverged and at which input.

**`pub trait Dispatcher`** (`differential.rs:66-69`)**
`fn run(&self, config: &Config) -> Outcome;`

**`pub struct Stage0Stub`** (`differential.rs:72-79`)**
Returns `Outcome::Stage0Unimplemented`.

#### Live Functions

**`pub fn check_two_impls<T, U, A, B, Eq>(inputs: &[T], backend_a: Backend, mut a: A, backend_b: Backend, mut b: B, eq: Eq) -> Outcome`** (`differential.rs:95-124`)
Generic two-implementation comparator. For each input, calls `a(&inp)` and `b(&inp)`, applies `eq`. On mismatch, returns `Divergence` tagged with `backend_b`. The delta message includes `input[i]`, `backend_a=out_a`, `backend_b=out_b` debug-formatted.

**`pub fn ulp_diff_f32(a: f32, b: f32) -> u32`** (`differential.rs:130-135`)
Returns `u32::MAX` for NaN inputs. Otherwise calls `sortable_u32` on each and returns their `abs_diff`. Documents that `ulp_diff_f32(+0.0, -0.0) == 1` (they are adjacent in total ordering).

**`fn sortable_u32(x: f32) -> u32`** (`differential.rs:142-149`, private)
Standard IEEE-754 total-ordering trick: if sign bit is set (negative), bit-invert all bits; else toggle sign bit. Maps floats to `u32` preserving numeric order.

**`pub fn ulp_tolerant_eq_f32(tolerance: u32) -> impl Fn(&f32, &f32) -> bool`** (`differential.rs:153-155`)
Returns a closure suitable as the `eq` argument to `check_two_impls`. Captures `tolerance`.

#### `#[cfg(test)] mod tests` (`differential.rs:157-271`)

9 tests:
- `stub_returns_unimplemented`
- `two_matching_impls_are_ok` — `x*2` vs `x+x` for i64
- `divergence_pinpoints_failing_backend` — LevelZero off-by-one, verifies delta message
- `ulp_diff_zero_for_identical_floats` — including `+0.0/-0.0` == 1 ULP
- `ulp_diff_one_for_adjacent_floats` — `bits + 1`
- `ulp_diff_nan_is_max`
- `ulp_tolerant_eq_accepts_close_floats` — tolerance=4, 3 ULPs pass, 5 ULPs fail
- `check_two_impls_with_ulp_tolerance` — `x*2` vs `x+x` for f32 at 1 ULP
- `empty_inputs_is_ok`

#### Observation

**`Config.backends` is empty by default and `Stage0Stub` returns `Stage0Unimplemented`** — but `check_two_impls` operates independently of `Config`. There is no function that dispatches `Config.backends` to real GPU runners; that path is entirely deferred (the function requires caller-provided closures for each impl). This means the `pure_det` and `ulp_tolerance` fields in `Config` are not wired to `check_two_impls` either — callers must construct their own `ulp_tolerant_eq_f32(config.ulp_tolerance)` manually.

---

### 3.10 `src/replay.rs` (228 lines)

**Purpose:** The `@replay` oracle — deterministic replay of N runs using the same seed, asserting bit-exact outputs. Implements the T29/OG9 ship-gate guarantee.

#### Items

**`pub struct Config`** (`replay.rs:13-21`)
- `n: u32` — number of replays. Default 10 (OG9 CI-gate).
- `cross_backend: bool` — whether to include Vulkan × Level-Zero cross-backend replay when `{PureDet}+{Portable}`. Default true. **Currently unused in the live runner** — `run_replay_deterministic` ignores it.
- `seed: u64` — canonical seed.

**`pub enum Outcome`** (`replay.rs:34-42`)
- `Stage0Unimplemented`
- `Ok { replays: u32 }`
- `Divergence { replay_index: u32, diff_bytes: u64 }` — index of first divergent replay; `diff_bytes = core::mem::size_of::<T>()` as a proxy.

**`pub trait Dispatcher`** (`replay.rs:45-47`)**
`fn run(&self, config: &Config) -> Outcome;`

**`pub struct Stage0Stub`** (`replay.rs:50-57`)**
Returns `Outcome::Stage0Unimplemented`.

#### Live Runner

**`pub fn run_replay_deterministic<T, F>(config: &Config, mut f: F) -> Outcome`** (`replay.rs:77-98`)
Runs `f` with `Lcg::new(config.seed)` to get `first`. Then for `k` in `1..config.n`, re-seeds with the same `config.seed`, runs `f` again, compares with `first`. On mismatch: `Divergence { replay_index: k, diff_bytes: size_of::<T>() }`. On all-match: `Ok { replays: config.n }`.

Edge case: `config.n == 0` returns `Ok { replays: 0 }` immediately.

#### `#[cfg(test)] mod tests` (`replay.rs:100-227`)

8 tests:
- `stub_returns_unimplemented`
- `default_n_equals_10`
- `deterministic_prng_reader_replays_bit_exact` — sum of 100 LCG draws, 10 replays
- `hidden_state_breaks_determinism` — mutable cell survives across replays, diverges at index 1
- `zero_replays_is_ok_with_zero`
- `single_replay_always_ok` — trivially passes even with non-deterministic f
- `divergence_reports_byte_width_of_type` — `u32 = 4 bytes`
- `different_seeds_still_replay_deterministically`

#### Observations

**`cross_backend` field is not consumed** (`replay.rs:16`): `run_replay_deterministic` does not inspect `Config::cross_backend`. The cross-machine / cross-backend replay variant is explicitly deferred in the module header comment ("deferred — still `Stage0Unimplemented` through the legacy dispatcher").

**`diff_bytes` is not a real divergence magnitude** (`replay.rs:91`): The field name `diff_bytes` suggests a count of differing bytes, but it is `core::mem::size_of::<T>()` — the size of the output type, not the number of differing bytes. This can mislead a reader into thinking 4 differing bytes were found when actually it just means the type is 4 bytes wide. This is documented in the test `divergence_reports_byte_width_of_type` but the field name is still misleading.

---

### 3.11 `src/audit.rs` (217 lines)

**Purpose:** The `@audit_test` oracle — verifies that the `cssl-telemetry` audit chain is structurally intact, signatures are valid, required events are present, and domain-filtered views work. Bridges IFC (§11) and telemetry (§22) testing.

#### Items

**`pub struct Config`** (`audit.rs:15-21`)
- `domain_filter: Option<String>` — tag prefix filter. `None` = all domains.
- `check_negative_cases: bool` — flag for PRIME-DIRECTIVE compile-error negative tests. Default true. **Unused in the live runner** — negative test harness deferred to T11-phase-2c.

**`pub enum Outcome`** (`audit.rs:34-48`)
- `Stage0Unimplemented`
- `Ok { events_verified: u64 }`
- `ChainTampered { first_broken_index: u64 }` — hash-linkage or signature broken
- `EventMissing { expected_domain: String, expected_kind: String }`
- `NegativeCaseCompiled { case: String }` — PRIME-DIRECTIVE violation slipped through

**`pub trait Dispatcher`** (`audit.rs:51-53`)**
`fn run(&self, config: &Config) -> Outcome;`

**`pub struct Stage0Stub`** (`audit.rs:56-63`)**
Returns `Outcome::Stage0Unimplemented`.

#### Live Verifier

**`pub fn run_audit_verify(config: &Config, chain: &AuditChain, required_events: &[(&str, &str)]) -> Outcome`** (`audit.rs:82-118`)

1. Calls `chain.verify_chain()`. On `Err(e)`, maps error variants to `ChainTampered { first_broken_index }`:
   - `GenesisPrevNonZero | SignatureInvalid` → index 0.
   - `ChainBreak { seq } | InvalidSequence { actual: seq, .. }` → `seq`.
2. Extracts domain filter (empty string if `None`).
3. Filters chain entries by tag prefix.
4. Checks all `required_events` pairs: `(domain_prefix, kind_substring)` — entry must have `tag.starts_with(domain)` AND `message.contains(kind)`.
5. On all required events found: `Ok { events_verified: filtered.len() }`.

#### `#[cfg(test)] mod tests` (`audit.rs:120-216`)

7 tests:
- `stub_returns_unimplemented`
- `valid_chain_verifies_with_no_required_events`
- `required_events_found_reports_ok`
- `missing_required_event_reports_event_missing`
- `domain_filter_restricts_verification` — 3-entry chain, filter="declass" → 2 events counted
- `empty_chain_verifies_to_zero_events`
- `chain_with_signing_key_verifies_real_signatures` — exercises the Ed25519 sign path via `AuditChain::with_signing_key`

#### Observations

**`check_negative_cases` is never read** (`audit.rs:18`): The field is in `Config.default()` as `true`, but `run_audit_verify` does not inspect it. The negative-test harness (verifying PRIME-DIRECTIVE compile-errors) is deferred to T11-phase-2c and requires `cssl-ifc` + `cssl-macros`.

**No test for a broken chain** (`audit.rs:120-216`): The seven tests all use valid chains or valid chains with missing required events. There is no test that tampers with a chain and verifies `ChainTampered` is returned. This is a gap in audit test coverage — the tamper-detection path is untested.

---

### 3.12 `src/r16_attestation.rs` (258 lines)

**Purpose:** R16 C99-anchor reproducibility attestation. Implements the signing and verification of build reproducibility records. Tied to T30/OG10 ship-gate: C99-compiled stage3 must be bit-exact with CSSLv3-compiled stage1.

#### Items

**`pub struct Attestation`** (`r16_attestation.rs:13-24`)
- `compiler_version: String`
- `source_commit: String`
- `c99_tarball_blake3: String` — BLAKE3 hash of the emitted C99 tarball.
- `stage1_blake3: String` — BLAKE3 hash of the stage1 compiler binary.
- `signature: Vec<u8>` — 64-byte Ed25519 signature over the canonical serialization.

All fields are `String` (or `Vec<u8>` for signature). Derives `Debug, Clone, PartialEq, Eq`.

#### `impl Attestation` Methods

**`pub fn canonical_bytes(&self) -> Vec<u8>`** (`r16_attestation.rs:31-41`)
Serializes as `compiler_version|source_commit|c99_tarball_blake3|stage1_blake3` with literal `|` separators, UTF-8 encoded. This is the sign-input.

**`pub fn build_signed(...) -> Self`** (`r16_attestation.rs:46-63`)
Constructs the record, sets signature to `Vec::new()`, calls `Signature::sign(key, &canonical_bytes())`, then stores `sig.0.to_vec()`. `#[must_use]`.

**`pub fn verify(&self, key: &SigningKey) -> bool`** (`r16_attestation.rs:68-77`)
Returns `false` if `signature.len() != 64`. Otherwise copies to `[u8; 64]`, constructs a `Signature`, calls `key.verify(&canonical_bytes(), &sig)`.

**`pub fn content_hash(&self) -> ContentHash`** (`r16_attestation.rs:82-84`)
BLAKE3 hash of `canonical_bytes()`. Used as a compact ID.

**`pub enum Outcome`** (`r16_attestation.rs:87-100`)
- `Stage0Unimplemented`
- `Attested { record: Attestation }`
- `Diverged { expected_blake3: String, actual_blake3: String }`
- `NoSigningKey`

**`pub trait Attester`** (`r16_attestation.rs:103-106`)**
`fn attest(&self) -> Outcome;`

**`pub struct Stage0Stub`** (`r16_attestation.rs:109-116`)**
Returns `Outcome::Stage0Unimplemented`.

#### Live Decision Helper

**`pub fn decide_attestation(expected_blake3: &str, actual_blake3: &str, compiler_version: &str, source_commit: &str, signing_key: Option<&SigningKey>) -> Outcome`** (`r16_attestation.rs:127-153`)

Logic:
1. If `expected != actual`: `Diverged { ... }`.
2. If `signing_key.is_none()`: `NoSigningKey`.
3. On match + key present: calls `Attestation::build_signed` with `expected_blake3` used for **both** `c99_tarball_blake3` and `stage1_blake3`. Returns `Attested { record }`.

The comment at line 143-145 documents this explicitly: "c99-tarball-hash and stage1-hash are both equal to expected_blake3 from the R16-anchor perspective."

#### `#[cfg(test)] mod tests` (`r16_attestation.rs:155-257`)

9 tests:
- `stub_returns_unimplemented`
- `canonical_bytes_has_expected_shape` — asserts `"1.0.0|deadbeef|hash-a|hash-b"`
- `sign_then_verify_roundtrip`
- `signature_tampered_fails_verify` — flips bit 0 of signature
- `content_hash_is_deterministic` — also asserts non-zero
- `decide_attestation_matching_hashes_produces_attested`
- `decide_attestation_divergent_hashes_produces_diverged`
- `decide_attestation_no_key_produces_no_signing_key`
- `cross_key_signature_fails_verify`

#### Observations

**Single-hash collapse in `decide_attestation`** (`r16_attestation.rs:143-150`): When hashes match, `build_signed` is called with `expected_blake3` for both the C99 tarball and the stage1 binary hashes. This is semantically correct for the R16 scenario (equality of the two hashes is the precondition, so they're the same), but the resulting `Attestation` record loses the conceptual distinction between the tarball and the binary. A reader examining a persisted attestation cannot tell whether the two were independently measured or collapsed from a single comparison.

---

### 3.13 `src/power.rs` (88 lines)

**Purpose:** The `@power_bench` oracle — power regression via Level-Zero sysman `zesPowerGetEnergyCounter`. Currently a stub.

#### Items

**`pub struct Config`** (`power.rs:10-18`)
- `bench_id: String` — baseline lookup key.
- `runs: u32` — default 5 (rolling-median 3-of-5 per spec).
- `regression_threshold: f64` — default 0.05 (5%).

**`pub struct PowerSample`** (`power.rs:31-39`)
- `total_joules: f64`
- `peak_watts: f64`
- `avg_watts: f64`

Derives `Debug, Clone, Copy, Default, PartialEq`.

**`pub enum Outcome`** (`power.rs:43-59`)
- `Stage0Unimplemented`
- `Ok { measured: PowerSample, baseline: PowerSample }`
- `Regressed { measured: PowerSample, baseline: PowerSample, delta_pct: f64 }`
- `SysmanUnavailable` — non-Intel platform

**`pub trait Dispatcher`** (`power.rs:62-64`)**

**`pub struct Stage0Stub`** (`power.rs:67-73`)**
Returns `Outcome::Stage0Unimplemented`.

#### `#[cfg(test)] mod tests` (`power.rs:76-87`)

Single test: `stub_returns_unimplemented`.

#### Status

Fully stubbed. No live implementation. Requires `cssl-host-level-zero` sysman binding.

---

### 3.14 `src/thermal.rs` (80 lines)

**Purpose:** The `@thermal_stress` oracle — sustained workload thermal sampling via `zesTemperatureGetState`. Currently a stub.

#### Items

**`pub struct Config`** (`thermal.rs:13-25`)
- `duration: Duration` — default 5 min.
- `sample_interval: Duration` — default 100ms.
- `limit_c: f32` — thermal hard limit in Celsius. Default 100.0°C.
- `safety_margin_c: f32` — default 5.0°C.
- `steady_state_max: Duration` — convergence deadline. Default 120s.

**`pub enum Outcome`** (`thermal.rs:38-51`)
- `Stage0Unimplemented`
- `Ok { steady_c: f32, peak_c: f32 }`
- `LimitBreached { peak_c: f32, limit_c: f32 }`
- `NoSteadyState { final_c: f32 }`
- `SysmanUnavailable`

**`pub trait Dispatcher`** (`thermal.rs:54-56`)**

**`pub struct Stage0Stub`** (`thermal.rs:59-65`)**
Returns `Outcome::Stage0Unimplemented`.

#### `#[cfg(test)] mod tests` (`thermal.rs:68-79`)

Single test: `stub_returns_unimplemented`.

#### Status

Fully stubbed. No live implementation. Requires `cssl-host-level-zero` sysman binding.

---

### 3.15 `src/hot_reload.rs` (58 lines)

**Purpose:** The `@hot_reload_test` oracle — schema-migration invariance testing. Currently a stub.

#### Items

**`pub struct Config`** (`hot_reload.rs:11-14`)
- `reload_source: String` — path to the reload-target file (new schema version).

Derives `Debug, Default, Clone`.

**`pub enum Outcome`** (`hot_reload.rs:17-28`)
- `Stage0Unimplemented`
- `Ok`
- `StateDiverged { field_path: String, before: String, after: String }` — migration or `@transient` misapplied.

**`pub trait Dispatcher`** (`hot_reload.rs:31-33`)**

**`pub struct Stage0Stub`** (`hot_reload.rs:36-43`)**
Returns `Outcome::Stage0Unimplemented`.

#### `#[cfg(test)] mod tests` (`hot_reload.rs:46-57`)

Single test: `stub_returns_unimplemented`.

#### Status

Fully stubbed. No live implementation. Requires `cssl-persist` integration.

---

## 4. Crate Notes

### Test Coverage (Meta)

The test infrastructure is itself well-tested by in-module `#[cfg(test)]` blocks. There are no external integration tests (`tests/` directory does not exist). Coverage by module:

| Module | Tests | Quality |
|---|---|---|
| `lib.rs` | 3 scaffold tests | Good: registry integrity |
| `oracle.rs` | 2 tests | Good: completeness + uniqueness |
| `metrics.rs` | 1 test | Minimal: only `Default` values |
| `property.rs` | 32 tests | Excellent: PRNG, all generators, shrink quality |
| `metamorphic.rs` | 25 tests | Good; missing: `FaaDiBruno`, `Conservation`, `Custom` |
| `fuzz.rs` | 8 tests | Good; missing: `smt_oracle`, throughput |
| `bench.rs` | 9 tests | Good; covers regression detection and I/O |
| `golden.rs` | 9 tests | Good; SSIM/FLIP deferred |
| `differential.rs` | 9 tests | Good; missing: real backend dispatch |
| `r16_attestation.rs` | 9 tests | Good: full crypto round-trips |
| `replay.rs` | 8 tests | Good; missing: cross-backend |
| `audit.rs` | 7 tests | Moderate: missing chain-tamper test |
| `power.rs` | 1 test | Stub-only |
| `thermal.rs` | 1 test | Stub-only |
| `hot_reload.rs` | 1 test | Stub-only |

**Total test functions: ~135** across all modules.

### What Is Incomplete or Stubbed

1. **`power.rs` / `thermal.rs` / `hot_reload.rs`**: Entire oracle implementation missing. No live code beyond `Config` and `Outcome` type definitions.
2. **`metrics.rs` sampling pipeline**: Structs defined, no connection to real ring-buffers or sysman.
3. **SSIM + FLIP in `golden.rs`**: Fields always `0.0`; `ssim_threshold` and `flip_threshold` config values never applied.
4. **Coverage guidance in `fuzz.rs`**: `smt_oracle` and `min_exec_per_sec` config fields exist but have no effect.
5. **Cross-backend replay in `replay.rs`**: `cross_backend` config field is unused.
6. **Negative-test harness in `audit.rs`**: `check_negative_cases` config field is unused.
7. **`FaaDiBruno` and `Conservation` laws in `metamorphic.rs`**: Enum variants registered, no verifier functions.
8. **Real GPU dispatch in `differential.rs`**: `check_two_impls` is a generic function; actual Vulkan/LevelZero dispatch requires FFI not yet present.
9. **CI runner and `csslc test` command**: No integration with a test-runner binary; all execution is through `cargo test`.
10. **Stage3 rebuild pipeline in `r16_attestation.rs`**: Crypto is live; rebuild orchestration is not.

### Spec Divergences

1. **`specs/23_TESTING.csl` lists `frequency-stability` and `latency-percentile` as oracle modes** but these are not `OracleMode` variants — they are data shapes in `metrics.rs`. The spec's `§ ORACLE MODES` section includes them as sub-bullets under hardware tests, and the lib.rs oracle registry comment correctly categorizes them as "adjunct," but a new contributor reading the spec would expect them to appear in `OracleMode::ALL`. The discrepancy is that the spec text treats them descriptively while the enum treats them as infrastructure. No code issue, but worth documenting.

2. **`specs/23_TESTING.csl § BASELINE MANAGEMENT`** says baselines are stored in JSON: "stored in git (small-enough — JSON-per-bench)." The `bench.rs` implementation uses a plain `latest.txt` with a single integer. The module header notes "full JSON schema + multi-dimensional statistics (p50 + p95 + p99) deferred to T11-phase-2c." This is an explicit documented divergence, not an accidental one.

3. **`specs/23_TESTING.csl § @bench`** says "per-bench median of 10 runs... stored in `.perf-baseline/<bench-id>/*.json`." Code stores `<bench_id>/latest.txt`. Same divergence as above.

4. **`specs/23_TESTING.csl § power-regression`** says "rolling-median 3-of-5 runs." `power.rs` Config has `runs: u32 = 5`, but no rolling-median implementation exists at all (fully stubbed).

### Bugs Found

1. **`bench.rs:155` — Documentation bug:** Comment says "returns the lower midpoint" but the code computes `sorted[len/2]` which for even-length slices returns the upper-of-two midpoints (e.g., index 2 of 4 = upper midpoint). The test at `bench.rs:204` correctly labels the test `median_of_even_length_returns_upper_midpoint`. Comment and code diverge; code and test agree; the comment is wrong.

2. **`golden.rs:147-149` — ssim misleadingly 0.0 for non-empty identical buffers:** When `actual == expected` (non-empty), `compute_byte_metrics` returns `ssim: 0.0`. A caller checking `metrics.ssim` to confirm quality will get `0.0` even for a perfect match. The empty-buffer special case at line 130 correctly sets `ssim: 1.0`, creating an inconsistency between the empty and non-empty identical cases.

3. **`metamorphic.rs:301` — potential false Lipschitz violation for boundary functions:** `check_lipschitz` uses `lhs > rhs` with no epsilon. For exactly-Lipschitz functions like `f(x) = k*x`, floating-point rounding can produce `lhs` fractionally exceeding `rhs`, yielding a false `Violation`. Low probability on the existing test cases (integer-scaled), but real for functions near the exact Lipschitz boundary.

4. **`replay.rs:91` — `diff_bytes` field name is misleading:** The `Divergence` variant's `diff_bytes` field stores `core::mem::size_of::<T>() as u64`, which is the bit-width of the output type, not a count of differing bytes. A caller reading `diff_bytes = 4` cannot distinguish "4 bytes of a u32 differed" from "1 out of 8 bytes of a u64 differed." The field should be named `output_type_size_bytes` or the semantics changed to a real byte-diff count.

### Dead Code / Unused Fields

The following `Config` fields are declared but never read by the corresponding live function:
- `fuzz::Config::smt_oracle` (`fuzz.rs:22`)
- `fuzz::Config::min_exec_per_sec` (`fuzz.rs:24`)
- `replay::Config::cross_backend` (`replay.rs:16`)
- `audit::Config::check_negative_cases` (`audit.rs:18`)
- `golden::Config::ssim_threshold` (`golden.rs:21`) — not read in `compare_bytes_against`
- `golden::Config::flip_threshold` (`golden.rs:23`) — not read in `compare_bytes_against`

All are documented gaps (deferred implementations), not accidental dead code, but they should be flagged so the T11-phase-2c implementer knows which fields need activation.

### Surprises

1. **No external testing crates**: The crate's description mentions "proptest, insta, criterion" in the audit task brief, but none are present. The crate implements its own PRNG, generators, shrinking, timing, and baseline storage entirely from scratch. This is architecturally consistent with the "no external deps" design mandate for the proprietary language stack.

2. **`Lcg` is shared across modules**: `fuzz.rs` and `replay.rs` both `use crate::property::Lcg` directly. The PRNG is a first-class public type in `property.rs`, not in a dedicated `prng.rs` module. This creates a minor semantic coupling: the PRNG is discoverable under the "property" namespace rather than a neutral location.

3. **`r16_attestation.rs` is production-grade crypto**: Unlike the stub-heavy modules, `r16_attestation.rs` has working BLAKE3 + Ed25519 sign/verify with 9 tests including cross-key failure checks and tamper detection. The crypto infrastructure is ready; only the rebuild pipeline integration is missing.

4. **`audit.rs` tamper path untested**: The `ChainTampered` outcome variant exists and is mapped in `run_audit_verify`, but no test in the module actually tampers with a chain. The `cssl-telemetry::AuditChain` may or may not expose a way to corrupt an existing chain for testing — if it does, this test should be added.
