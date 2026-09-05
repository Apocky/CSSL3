# Audit 14: cssl-examples, csslc, Workspace Metadata

**Date:** 2026-05-14
**Auditor:** Claude Sonnet 4.6 (automated)
**Scope:** `compiler-rs/crates/cssl-examples/`, `compiler-rs/crates/csslc/`, workspace Cargo.toml and toolchain files.

---

## 1. SLICE OVERVIEW

### cssl-examples

`cssl-examples` is the vertical-slice integration library for the CSSLv3 stage-0 compiler. It is not a standalone binary; it is a library crate that other tools and CI pipelines consume via `cargo test`. Its purpose is threefold:

1. **Pipeline smoke tests.** Three canonical `.cssl` example files (`hello_triangle.cssl`, `sdf_shader.cssl`, `audio_callback.cssl`) are embedded at compile time via `include_str!`. Each is driven through the full front-end (lex → parse → HIR lower) and must produce zero fatal parse errors to be "accepted." This exercises `cssl-lex`, `cssl-parse`, and `cssl-hir` collectively.

2. **F1 (AutoDiff) correctness chain.** The `F1ChainOutcome` / `run_f1_chain` pair extends the pipeline through HIR AD-legality checking, refinement-obligation collection, MIR lowering, the AD walker, and SMT-query translation. This is the "killer-app gate" at the structural level — it proves every intermediate stage composes without error on real CSSL source.

3. **JIT execution and runtime gradient verification.** `jit_chain.rs` drives CSSLv3 source all the way through to Cranelift JIT-compiled machine code and verifies forward- and reverse-mode AD gradient correctness against central differences at sample points. This is the T11-D23..D44 arc: the most complete end-to-end proof in the codebase.

4. **Stage-1 scaffold grammar guard.** `stage1_scaffold.rs` embeds `stage1/hello.cssl` and `stage1/compiler.cssl` and checks they remain parse-valid through the stage-0 front-end as the grammar evolves.

5. **Symbolic analytic verification.** `ad_gate.rs` provides the `AnalyticExpr` symbolic algebra and `MirAdjointInterpreter`, which interpret the MIR reverse-mode variant symbolically and compare against hand-written analytic gradients using sampling-based equivalence. This covers FAdd/FSub/FMul/FDiv/FNeg + five transcendentals + chain-rule composition + scalar-expanded sphere-SDF. The R18 attestation layer (BLAKE3 + Ed25519) signs the gate report and per-case SMT proof-certs into an auditable `AuditChain`.

6. **Vec3 symbolic algebra.** `analytic_vec3.rs` extends `AnalyticExpr` with `AnalyticVec3Expr`, a companion symbolic algebra for vec3 expressions. It provides scene-SDF primitives (union/intersection/subtraction), smooth-min/max, piecewise gradient utilities, and n-ary fold variants — all routing back to scalar `AnalyticExpr` machinery.

**Compiler crates exercised:**

| crate | exercised by |
|---|---|
| `cssl-lex` | `lib.rs`, `jit_chain.rs`, `stage1_scaffold.rs` |
| `cssl-parse` | same |
| `cssl-hir` | same + F1 chain |
| `cssl-mir` | F1 chain, `jit_chain.rs` |
| `cssl-autodiff` | `ad_gate.rs`, `jit_chain.rs` |
| `cssl-smt` | `ad_gate.rs`, F1 chain |
| `cssl-telemetry` | `ad_gate.rs` (R18 attestation) |
| `cssl-cgen-cpu-cranelift` | `jit_chain.rs` |

### csslc

`csslc` is the compiler binary crate. In its current form (`src/main.rs`, 23 lines) it is a pure scaffold. `main()` prints two status lines to stderr and exits with code 0. There is no argument parsing, no subcommand dispatch, no invocation of any compiler crate. All actual compile-pipeline logic lives in the library crates; `csslc` as a binary is a named placeholder for the future CLI. The docstring lists the intended subcommands: `build`, `check`, `fmt`, `test`, `bench`, `lint`, `doc`, `emit-mlir`, `emit-spirv`, `emit-c99`, `replay`, `bench --update-baseline`, `test --update-golden`, `verify`, `attest`.

### Workspace structure

The workspace uses a flat `crates/*` glob member discovery under `compiler-rs/`. Resolver 2 is used. Workspace-level `[package]` sets version/edition/MSRV/license/author shared across every member. Workspace `[lints.clippy]` enforces `all = deny` + `pedantic = warn` + `nursery = warn` with a documented set of allowances for scaffold-phase patterns. External dependencies are declared once in `[workspace.dependencies]` and inherited via `foo.workspace = true` in each crate.

---

## 2. CRATE DESCRIPTIONS

### cssl-examples

- **Path:** `compiler-rs/crates/cssl-examples/`
- **Purpose:** Vertical-slice integration tests, killer-app gate, R18 attestation, stage-1 scaffold guard.
- **Total LOC:** ~6,671 (ad_gate.rs ~3,308 + jit_chain.rs ~1,589 + analytic_vec3.rs ~1,090 + lib.rs ~518 + stage1_scaffold.rs ~166)
- **Cargo.toml dependencies:**
  - `cssl-ast` (CST types, `SourceFile`, `SourceId`, `Surface`, `Module`)
  - `cssl-lex` (tokenizer)
  - `cssl-parse` (recursive-descent parser)
  - `cssl-hir` (CST → HIR lowering, AD-legality, refinement-obligation collection)
  - `cssl-mir` (MIR module/function representation, lowering, monomorphization)
  - `cssl-autodiff` (`AdWalker`, `apply_bwd`, `DiffRuleTable`)
  - `cssl-smt` (SMT query / solver infrastructure)
  - `cssl-telemetry` (BLAKE3 hashing, Ed25519 signing, `AuditChain`)
  - `cssl-cgen-cpu-cranelift` (`JitModule`, `JitFn`)
- **Crate-level lint allowances:** `clippy::similar_names`, `clippy::module_name_repetitions`, `clippy::too_many_lines`

### csslc

- **Path:** `compiler-rs/crates/csslc/`
- **Purpose:** Compiler CLI binary entry-point (scaffold placeholder).
- **Total LOC:** 23
- **Cargo.toml dependencies:** None — csslc has zero `[dependencies]`.
- **Binary name:** `csslc` (from `[[bin]]` section)

---

## 3. SOURCE FILE AUDIT

---

### 3.1 `compiler-rs/crates/csslc/src/main.rs` (23 lines)

**Purpose:** The sole entry-point for the `csslc` compiler binary. Serves as a named scaffold placeholder; does nothing beyond printing status information and exiting.

#### Items

**`fn main()`** — `main.rs:15`
The program entry point. Has no parameters. Body: calls `eprintln!` twice to print the crate version (via `env!("CARGO_PKG_VERSION")`) and a status message, then calls `std::process::exit(0)`. There is no argument parsing (no `std::env::args`, no clap, no similar), no file I/O, and no invocation of any compiler crate. The function is the only item in the file.

**Attributes:** `#![forbid(unsafe_code)]` — prevents any unsafe block in this crate.

**TODOs / Scaffolds:** The entire binary is a scaffold. The docstring (`//!`) explicitly states: "Status : T1 workspace scaffold — subcommands pending (T2+)." The list of future subcommands includes: `build`, `check`, `fmt`, `test`, `bench`, `lint`, `doc`, `emit-mlir`, `emit-spirv`, `emit-c99`, `replay`, `bench --update-baseline`, `test --update-golden`, `verify`, `attest`.

**Cross-references:** Spec authority is `specs/01_BOOTSTRAP.csl` + `specs/14_BACKEND.csl`. No compiler crates are imported or used.

---

### 3.2 `compiler-rs/crates/cssl-examples/src/lib.rs` (518 lines)

**Purpose:** Root of `cssl-examples`. Declares the four submodules, embeds the three canonical `.cssl` example files at compile time, provides the `pipeline_example` and `run_f1_chain` pipelines, and hosts the `all_examples` / `run_f1_chain_all` convenience functions plus their test suites.

#### Structs

**`PipelineOutcome`** — `lib.rs:67`
```rust
pub struct PipelineOutcome {
    pub name: String,
    pub token_count: usize,
    pub cst_item_count: usize,
    pub parse_error_count: usize,
    pub hir_item_count: usize,
    pub lower_diag_count: usize,
}
```
Records the lex+parse+HIR pipeline outcome for a single example. Derives `Debug, Clone, PartialEq, Eq`.

**`F1ChainOutcome`** — `lib.rs:163`
```rust
pub struct F1ChainOutcome {
    pub name: String,
    pub frontend: PipelineOutcome,
    pub obligation_count: usize,
    pub diff_fn_count: usize,
    pub ad_legality_diag_count: usize,
    pub mir_fn_count: usize,
    pub ad_variants_emitted: u32,
    pub ad_ops_matched: u32,
    pub smt_queries_translated: usize,
    pub smt_translation_failures: usize,
}
```
Extends `PipelineOutcome` with the full F1 correctness chain outcome. Derives `Debug, Clone, PartialEq, Eq`.

#### Constants

**`HELLO_TRIANGLE_SRC: &str`** — `lib.rs:50` — Compile-time embedded content of `examples/hello_triangle.cssl`.

**`SDF_SHADER_SRC: &str`** — `lib.rs:55` — Compile-time embedded content of `examples/sdf_shader.cssl` (the killer-app: contains `@differentiable` and `bwd_diff(scene_sdf)`).

**`AUDIO_CALLBACK_SRC: &str`** — `lib.rs:59` — Compile-time embedded content of `examples/audio_callback.cssl` (contains `Realtime<Crit>` and `Audit<"audio-callback">`).

**`STAGE0_SCAFFOLD: &str`** — `lib.rs:292` — Crate version string from `env!("CARGO_PKG_VERSION")`, used as a guard in `scaffold_version_present` test.

#### Functions

**`fn pipeline_example(name: &str, source: &str) -> PipelineOutcome`** — `lib.rs:106`
Runs the stage-0 front-end (lex → parse → HIR-lower) on a `(name, source)` pair. Creates a `SourceFile`, calls `cssl_lex::lex`, `cssl_parse::parse`, then `run_hir`. Returns a `PipelineOutcome`. Marked `#[must_use]`.

**`fn run_hir(file: &SourceFile, module: &Module) -> (usize, usize)`** — `lib.rs:126`
Private helper. Calls `cssl_hir::lower_module`, counts HIR items and lower diagnostics. Returns `(hir_item_count, lower_diag_count)`.

**`fn hir_item_count(m: &HirModule) -> usize`** — `lib.rs:131`
Private helper. Returns `m.items.len()`.

**`fn all_examples() -> Vec<PipelineOutcome>`** — `lib.rs:137`
Runs `pipeline_example` on the three canonical `.cssl` sources. Marked `#[must_use]`.

**`fn run_f1_chain(name: &str, source: &str) -> F1ChainOutcome`** — `lib.rs:221`
The complete F1-correctness chain: lex + parse → HIR → AD-legality check (`cssl_hir::check_ad_legality`) → refinement-obligation collection (`cssl_hir::collect_refinement_obligations`) → MIR lowering per HIR fn → AD walker transform (`cssl_autodiff::AdWalker::transform_module`) → SMT predicate translation (`cssl_smt::translate_bag`). Returns `F1ChainOutcome`. Marked `#[must_use]`.

**`fn run_f1_chain_all() -> Vec<F1ChainOutcome>`** — `lib.rs:283`
Applies `run_f1_chain` to all three canonical sources. Marked `#[must_use]`.

#### impl blocks

**`impl PipelineOutcome`** — `lib.rs:82`
- `pub fn summary(&self) -> String` — `lib.rs:85`: Formats a one-line summary with all counters. Marked `#[must_use]`.
- `pub fn is_accepted(&self) -> bool` — `lib.rs:99`: Returns `parse_error_count == 0`. Marked `#[must_use]`.

**`impl F1ChainOutcome`** — `lib.rs:187`
- `pub fn summary(&self) -> String` — `lib.rs:190`: Formats a multi-field summary line. Marked `#[must_use]`.
- `pub fn is_composed(&self) -> bool` — `lib.rs:212`: Returns `true` iff `parse_error_count == 0 && ad_legality_diag_count == 0`. Stage-0 acceptance criterion. Marked `#[must_use]`.

#### Test module (`lib.rs:294`)

13 tests covering:
- `scaffold_version_present` — STAGE0_SCAFFOLD non-empty.
- `hello_triangle_source_non_empty` — checks module marker.
- `sdf_shader_source_non_empty` — checks `@differentiable` and `bwd_diff(scene_sdf)` markers.
- `audio_callback_source_non_empty` — checks `Realtime<Crit>` and `Audit<"audio-callback">`.
- `hello_triangle_tokenizes`, `sdf_shader_tokenizes`, `audio_callback_tokenizes` — sanity token-count check.
- `all_examples_returns_three_outcomes` — verifies names.
- `summary_shape` — checks summary string shape.
- `outcome_is_accepted_returns_bool` — type-check on `is_accepted()`.
- `each_example_emits_nontrivial_tokens_and_items` — asserts `token_count >= 10` and `cst_item_count >= 1` for each.
- `f1_chain_runs_on_sdf_shader` — asserts `diff_fn_count >= 3` and `ad_variants_emitted >= 6`.
- `f1_chain_audio_callback_has_refinement_obligations` — asserts `obligation_count >= 1`.
- `f1_chain_all_examples_compose_without_structural_failure` — runs all three; calls `summary()` without hard-failing on `ad_legality_diag_count > 0` (parser may emit unresolved-path warnings for stdlib references).
- `f1_chain_outcome_summary_shape` — string shape.
- `f1_chain_is_composed_predicate` — constructs a zero-error `F1ChainOutcome` and asserts `is_composed()`.
- `f1_chain_sdf_mir_fn_count_nonzero` — asserts `mir_fn_count >= 1`.
- `f1_chain_ad_walker_matches_primitives_in_sdf_shader` — reads `ad_ops_matched` without asserting (observational).
- `f1_chain_smt_queries_audio_refinement` — if `obligation_count > 0`, asserts `smt_queries_translated + smt_translation_failures == obligation_count`.

**Deferred (T12-phase-2):** Full type-check, MIR lowering, codegen, spirv-val, Vulkan render, bit-exact AD verification — all documented as deferred in the module docstring.

---

### 3.3 `compiler-rs/crates/cssl-examples/src/stage1_scaffold.rs` (166 lines)

**Purpose:** Grammar regression guard for the stage-1 self-host placeholder files. Embeds `stage1/hello.cssl` and `stage1/compiler.cssl` at compile time and drives them through the stage-0 front-end to confirm they remain lex/parse-accepting as the grammar evolves.

#### Constants

**`STAGE1_HELLO_SRC: &str`** — `stage1_scaffold.rs:41` — Compile-time embedded `stage1/hello.cssl`. Expected to contain `"fn hello"` and `"42"`.

**`STAGE1_COMPILER_SRC: &str`** — `stage1_scaffold.rs:47` — Compile-time embedded `stage1/compiler.cssl`. Expected to contain `"fn main"` and `"P1"` (capability gate reference from `stage1/README.csl`).

#### Functions

**`pub fn all_stage1_scaffold_outcomes() -> Vec<crate::PipelineOutcome>`** — `stage1_scaffold.rs:60`
Runs `pipeline_example` on both scaffold files and returns the outcomes. Available to downstream drivers without re-duplicating `include_str!` boilerplate. Marked `#[must_use]`.

#### Test module (`stage1_scaffold.rs:67`)

8 tests:
- `stage1_hello_src_non_empty` — checks markers `"42"` and `"fn hello"`.
- `stage1_compiler_src_non_empty` — checks markers `"fn main"` and `"P1"`.
- `stage1_hello_tokenizes` — asserts `token_count > 0`.
- `stage1_compiler_tokenizes` — asserts `token_count > 0`.
- `stage1_hello_parses_without_errors` — asserts `parse_error_count == 0` and `cst_item_count >= 1`.
- `stage1_compiler_parses_without_errors` — same for `compiler.cssl`.
- `all_stage1_scaffold_outcomes_returns_two` — asserts len=2 and names.
- `all_stage1_scaffold_files_accepted` — the **canary test**: asserts `is_accepted()` on every scaffold file; failure here means a grammar change broke the stage-1 placeholder.

---

### 3.4 `compiler-rs/crates/cssl-examples/src/analytic_vec3.rs` (1,090 lines)

**Purpose:** Vec3-valued symbolic expression algebra extending the scalar `AnalyticExpr` from `ad_gate.rs`. Provides scene-SDF primitives (union/intersect/subtract), smooth-min/max, piecewise gradient utilities, and n-ary fold variants. All vec3 operations reduce to scalar per-component via `to_scalar_components`, bridging into existing AD machinery without requiring a new AD primitive.

Module-level clippy allowances: `float_cmp`, `should_implement_trait`, `implicit_hasher`.

Spec reference: `specs/05_AUTODIFF.csl § SDF-NORMAL` and `§ APPENDIX-SMOOTH`.

#### Enums

**`pub enum VecComp`** — `analytic_vec3.rs:49` — `X`, `Y`, `Z`. Used for scalar projection.

**`pub enum AnalyticVec3Expr`** — `analytic_vec3.rs:72`
Variants: `Const(f64, f64, f64)`, `Var(String)`, `Neg(Box<Self>)`, `Add(Box<Self>, Box<Self>)`, `Sub(Box<Self>, Box<Self>)`, `ScalarMul(Box<AnalyticExpr>, Box<Self>)`, `ScalarDiv(Box<Self>, Box<AnalyticExpr>)`, `Normalize(Box<Self>)`.
Derives `Debug, Clone, PartialEq`.

#### impl VecComp

**`pub const fn suffix(self) -> &'static str`** — `analytic_vec3.rs:59` — Returns `"x"`, `"y"`, or `"z"`.

#### impl AnalyticVec3Expr

**`pub const fn c(x: f64, y: f64, z: f64) -> Self`** — `analytic_vec3.rs:94` — Literal vec3 constructor.

**`pub fn v(name: impl Into<String>) -> Self`** — `analytic_vec3.rs:100` — Named variable constructor.

**`pub fn neg(a: Self) -> Self`** — `analytic_vec3.rs:106`

**`pub fn add(a: Self, b: Self) -> Self`** — `analytic_vec3.rs:112`

**`pub fn sub(a: Self, b: Self) -> Self`** — `analytic_vec3.rs:118`

**`pub fn scalar_mul(s: AnalyticExpr, v: Self) -> Self`** — `analytic_vec3.rs:124`

**`pub fn scalar_div(v: Self, s: AnalyticExpr) -> Self`** — `analytic_vec3.rs:130`

**`pub fn normalize(v: Self) -> Self`** — `analytic_vec3.rs:136`

**`pub fn simplify(&self) -> Self`** — `analytic_vec3.rs:143` — Structurally lifts `AnalyticExpr::simplify` componentwise. Not a full constant-folder for vec3; preserves evaluation semantics.

**`pub fn evaluate(&self, env: &HashMap<String, f64>) -> [f64; 3]`** — `analytic_vec3.rs:164` — Numerically evaluates the vec3 expression. `Var` lookups use `"<name>.x"` / `"<name>.y"` / `"<name>.z"` keys. Missing keys produce NaN; zero-vector normalize produces `[NaN, NaN, NaN]`.

**`pub fn to_scalar_components(&self) -> (AnalyticExpr, AnalyticExpr, AnalyticExpr)`** — `analytic_vec3.rs:212` — The bridge to scalar AD. Returns the (x, y, z) scalar expansion, allowing all vec3 operations to verify through existing `AnalyticExpr` machinery.

#### Module-level free functions

**`pub fn length(v: &AnalyticVec3Expr) -> AnalyticExpr`** — `analytic_vec3.rs:290` — Expands `length(v) = sqrt(v.x² + v.y² + v.z²)` into scalar ops. Uses `AnalyticExpr::Sqrt`. Routes through `to_smt` / `to_term` without adding new primitives.

**`pub fn dot(a: &AnalyticVec3Expr, b: &AnalyticVec3Expr) -> AnalyticExpr`** — `analytic_vec3.rs:300` — `a.x·b.x + a.y·b.y + a.z·b.z` as a scalar expression.

**`pub fn vec3_proj(v: &AnalyticVec3Expr, comp: VecComp) -> AnalyticExpr`** — `analytic_vec3.rs:312` — Scalar projection of one component.

**`pub fn sphere_sdf_vec3(p: &AnalyticVec3Expr, r: &AnalyticExpr) -> AnalyticExpr`** — `analytic_vec3.rs:324` — `length(p) - r` as an `AnalyticExpr`. Directly exercises the killer-app primal.

**`pub fn sphere_sdf_grad_p(p: &AnalyticVec3Expr, d_y: &AnalyticExpr) -> AnalyticVec3Expr`** — `analytic_vec3.rs:332` — Analytic gradient of `sphere_sdf` wrt `p`: `normalize(p) · d_y`.

**`pub fn sphere_sdf_grad_r(d_y: &AnalyticExpr) -> AnalyticExpr`** — `analytic_vec3.rs:338` — Analytic gradient wrt `r`: `-d_y`.

**`pub fn scene_sdf_union(a: AnalyticExpr, b: AnalyticExpr) -> AnalyticExpr`** — `analytic_vec3.rs:353` — `min(a, b)`.

**`pub fn scene_sdf_intersect(a: AnalyticExpr, b: AnalyticExpr) -> AnalyticExpr`** — `analytic_vec3.rs:362` — `max(a, b)`.

**`pub fn scene_sdf_subtract(a: AnalyticExpr, b: AnalyticExpr) -> AnalyticExpr`** — `analytic_vec3.rs:369` — `max(a, -b)`.

**`pub fn scene_sdf_union_grad(a: &AnalyticExpr, b: &AnalyticExpr, da: &AnalyticExpr, db: &AnalyticExpr, env: &HashMap<String, f64>) -> AnalyticExpr`** — `analytic_vec3.rs:377` — Piecewise gradient of `min(a, b)`. Evaluates `a` and `b` numerically at `env` to pick the winning branch. At the cusp (`a == b`), picks `da` by convention.

**`pub fn scene_sdf_intersect_grad(a: &AnalyticExpr, b: &AnalyticExpr, da: &AnalyticExpr, db: &AnalyticExpr, env: &HashMap<String, f64>) -> AnalyticExpr`** — `analytic_vec3.rs:394` — Same for `max(a, b)`.

**`pub fn smooth_min(a: AnalyticExpr, b: AnalyticExpr, k: f64) -> AnalyticExpr`** — `analytic_vec3.rs:428` — Log-sum-exp soft minimum: `-log(exp(-k·a) + exp(-k·b)) / k`. Differentiable everywhere. As `k → ∞` approaches `min(a, b)`. Spec: `specs/05_AUTODIFF.csl § APPENDIX-SMOOTH`.

**`pub fn is_near_cusp(a: &AnalyticExpr, b: &AnalyticExpr, env: &HashMap<String, f64>, epsilon: f64) -> bool`** — `analytic_vec3.rs:448` — Returns `true` iff `|a(env) - b(env)| < epsilon`. Non-finite values are treated as "near cusp." Used by samplers to avoid the subgradient-valued cusp surface.

**`pub fn smooth_max(a: AnalyticExpr, b: AnalyticExpr, k: f64) -> AnalyticExpr`** — `analytic_vec3.rs:463` — `log(exp(k·a) + exp(k·b)) / k`. Implemented as `neg(smooth_min(neg(a), neg(b), k))`.

**`pub fn min_n(items: &[AnalyticExpr]) -> Option<AnalyticExpr>`** — `analytic_vec3.rs:476` — Left-associative fold of `min`. Returns `None` for empty slice.

**`pub fn max_n(items: &[AnalyticExpr]) -> Option<AnalyticExpr>`** — `analytic_vec3.rs:481`

**`pub fn smooth_min_n(items: &[AnalyticExpr], k: f64) -> Option<AnalyticExpr>`** — `analytic_vec3.rs:489`

**`pub fn smooth_max_n(items: &[AnalyticExpr], k: f64) -> Option<AnalyticExpr>`** — `analytic_vec3.rs:494`

#### Test module (`analytic_vec3.rs:502`)

56 tests organized in four groups:
1. Basic vec3 operations: `VecComp::suffix`, `Const` / `Var` evaluation, neg/add/sub/scalar_mul/scalar_div, normalize (unit-length check), normalize-zero-vector (NaN check), simplify roundtrip.
2. Scalar reductions: `length` at (3,4,0)=5, `dot` product, `vec3_proj`, sphere SDF primal + gradient wrt p and r, central-difference gradient match (numerical vs analytic for d(length)/d(p.x) at (3,4,0)).
3. Scene-SDF union/intersect/subtract: `scene_union_picks_nearer_distance`, `scene_intersect_picks_farther_distance`, `scene_subtract_carves_via_max_neg_b`, union/intersect gradient pick-the-winner, two-sphere union numerical gradient against winner branch, `min/max` constant folds, SMT emission (`min_uf`, `max_uf`).
4. Abs/Sign/smooth_min/smooth_max/n-ary folds: evaluation, constant folding, SMT emission, smooth_min convergence (k=100 ≈ sharp min), symmetry, central-difference continuity at cusp, `is_near_cusp`, `smooth_max`, all four n-ary fold variants.

**Notable algorithm:** The central-difference test at `analytic_vec3.rs:673` confirms d(length(p))/d(p.x) = 0.6 at p=(3,4,0), anchoring the killer-app gradient claim numerically.

---

### 3.5 `compiler-rs/crates/cssl-examples/src/ad_gate.rs` (3,308 lines)

**Purpose:** The T7-phase-2c killer-app gate. Provides the `AnalyticExpr` symbolic algebra, `MirAdjointInterpreter` (MIR bwd-variant interpreter), the `verify_gradient_case` entry-point, and the full R18 attestation stack (BLAKE3 + Ed25519 signed gate report + per-case SMT proof-certs + `AuditChain`). Also defines the structured SMT query path (`to_smt_query`) alongside the text-form (`to_smt`).

Module-level clippy allowances: `float_cmp`, `should_implement_trait`, `cast_precision_loss`, `redundant_closure`, `useless_format`, `needless_pass_by_value`, `redundant_clone`, `single_char_pattern`, `map_unwrap_or`, `redundant_closure_for_method_calls`.

Spec references: `specs/05_AUTODIFF.csl § SDF-NORMAL` + `§ INTEGRATIONS`; `HANDOFF_SESSION_2.csl § GATES § F1-correctness-gate`.

#### Enums

**`pub enum AnalyticExpr`** — `ad_gate.rs:80`
The symbolic expression tree for gradients. Variants:
- `Const(f64)` — numeric literal
- `Var(String)` — named variable (primal param or adjoint seed `d_y`)
- `Neg(Box<AnalyticExpr>)` — unary negation
- `Add(Box<AnalyticExpr>, Box<AnalyticExpr>)` — binary sum
- `Sub(...)` — binary difference
- `Mul(...)` — binary product
- `Div(...)` — binary division
- `Sqrt(Box<AnalyticExpr>)` — square root
- `Sin(...)`, `Cos(...)`, `Exp(...)`, `Log(...)` — transcendentals
- `Min(...)`, `Max(...)` — piecewise min/max for scene-SDF
- `Abs(...)` — absolute value (piecewise linear)
- `Sign(...)` — sign function (discontinuous at 0)
- `Uninterpreted(String, Vec<AnalyticExpr>)` — fallback for unrecognized ops; evaluates to NaN

Derives `Debug, Clone, PartialEq`.

#### impl AnalyticExpr

Constructor shorthand methods (all `#[must_use]`):

**`pub fn c(v: f64) -> Self`** — `ad_gate.rs:127`
**`pub fn v(name: impl Into<String>) -> Self`** — `ad_gate.rs:133`
**`pub fn neg(a: Self) -> Self`** — `ad_gate.rs:139`
**`pub fn add(a: Self, b: Self) -> Self`** — `ad_gate.rs:145`
**`pub fn sub(a: Self, b: Self) -> Self`** — `ad_gate.rs:151`
**`pub fn mul(a: Self, b: Self) -> Self`** — `ad_gate.rs:157`
**`pub fn div(a: Self, b: Self) -> Self`** — `ad_gate.rs:163`
**`pub fn min(a: Self, b: Self) -> Self`** — `ad_gate.rs:169`
**`pub fn max(a: Self, b: Self) -> Self`** — `ad_gate.rs:175`

**`pub fn simplify(&self) -> Self`** — `ad_gate.rs:190`
Recursive algebraic constant-folding and neutral-element elimination. Rules: `0 + x → x`, `x + 0 → x`, `x - 0 → x`, `0 - x → -x`, `1 * x → x`, `x * 1 → x`, `0 * x → 0`, `x * 0 → 0`, `x / 1 → x`, `-(-x) → x`, `-(const) → -const`, `const op const → const` for all binary ops. `Min(const, const)` folds to `min`. `Max`, `Abs`, `Sign` fold similarly. Transcendentals are simplified only recursively (no constant-fold for `sin(const)` etc.).

**`pub fn evaluate(&self, env: &HashMap<String, f64>) -> f64`** — `ad_gate.rs:332`
Numeric evaluation. `Var` uses `env.get(name).copied().unwrap_or(f64::NAN)`. `Uninterpreted` → NaN. Standard math ops for all other variants.

**`pub fn equivalent_by_sampling(&self, other: &Self, samples: &[HashMap<String, f64>], tolerance: f64) -> bool`** — `ad_gate.rs:388`
Sampling-based equivalence check. Both-NaN samples are inconclusive (skipped). One-NaN → mismatch. Both-Inf with same sign → match. Finite: match iff `|a - b| ≤ tolerance`. Requires at least one conclusive match; an all-NaN result returns `false`.

**`pub fn to_term(&self) -> Term`** — `ad_gate.rs:436`
Structural translation to `cssl_smt::Term`. Transcendentals become uninterpreted function applications (`sqrt_uf(x)` etc.). Constants: integer-valued `f64` → `Rational { num, den: 1 }`; fractional → approximated as `(/ round(v×10^6) 10^6)`. NaN → `Term::Var("nan_sentinel")`.

**`pub fn to_smt(&self) -> String`** — `ad_gate.rs:469`
SMT-LIB text emission. Z3/CVC5 compatible. Transcendentals as UF calls (`(sqrt_uf x)`). Min/Max as `(min_uf a b)` / `(max_uf a b)`. Abs → `(abs_uf a)`. Sign → `(sign_uf a)`.

**`pub fn free_vars(&self) -> Vec<String>`** — `ad_gate.rs:505`
Collects, sorts, and deduplicates all `Var` names in the expression tree.

**`fn collect_vars(&self, out: &mut Vec<String>)`** — `ad_gate.rs:513`
Private recursive helper for `free_vars`.

#### Module-level private helpers

**`fn format_smt_real(v: f64) -> String`** — `ad_gate.rs:544`
Formats `f64` for SMT-LIB: NaN → `"nan"`, integer-valued → `"{v:.1}"`, fractional → `"{v}"`.

**`fn f64_to_term(v: f64) -> Term`** — `ad_gate.rs:561`
Converts `f64` to a `Term::Lit`. NaN → `"nan_sentinel"` var; Inf → `"plus/minus_inf_sentinel"` var; integer-valued `f64` → `Rational { num: v as i64, den: 1 }`; fractional → `Rational { num: round(v × 10^6), den: 10^6 }`.

#### Structs

**`pub struct MirAdjointInterpreter<'a>`** — `ad_gate.rs:601`
Fields:
- `pub bwd_fn: &'a MirFunc` — the bwd-mode variant being interpreted
- `pub primal_param_names: Vec<String>` — param names in positional order
- `pub tolerance: f64` — default 1e-9 for downstream sampling checks
- `primal_exprs: HashMap<ValueId, AnalyticExpr>` — private; seeded from primal entry-block args
- `adjoint_exprs: HashMap<ValueId, AnalyticExpr>` — private; seeded from adjoint-in param(s)

Walks a reverse-mode MIR variant (from `apply_bwd`) and reconstructs `AnalyticExpr` for each `cssl.diff.bwd_return` operand. Maintains two parallel symbol tables: primal and adjoint. Distinguishes ops by the `diff_role == "adjoint"` attribute.

**`pub struct ParamCheck`** — `ad_gate.rs:797`
Fields: `name: String`, `analytic: AnalyticExpr`, `mir_derived: AnalyticExpr`, `matches: bool`.

**`pub struct GradientCase`** — `ad_gate.rs:809`
Fields: `name: String`, `param_names: Vec<String>`, `params: Vec<ParamCheck>`, `all_match: bool`.

**`pub struct KillerAppGateReport`** — `ad_gate.rs:1177`
Fields: `cases: Vec<GradientCase>`, `total: usize`, `passing: usize`.

**`pub struct SignedKillerAppGateReport`** — `ad_gate.rs:1412`
Fields: `report: KillerAppGateReport`, `canonical_payload: Vec<u8>`, `content_hash: ContentHash`, `signature: Signature`, `verifying_key: [u8; 32]`, `format: String`.

**`pub struct AttestationVerdict`** — `ad_gate.rs:1561`
Fields (all `bool`): `format_matches`, `payload_hash_matches`, `signature_verifies`, `gate_is_green`. Allow `clippy::struct_excessive_bools`.

**`pub struct SmtVerification`** — `ad_gate.rs:1632`
Fields: `case_name: String`, `verdict: Verdict`, `solver_kind: SolverKind`.

**`pub struct SmtVerificationReport`** — `ad_gate.rs:1661` (derives `Default`)
Fields: `verifications: Vec<SmtVerification>`, `unavailable: u32`, `unsat_count: u32`, `sat_count: u32`, `unknown_count: u32`.

**`pub struct SignedProofCert`** — `ad_gate.rs:1764`
Fields: `case_name`, `query_text`, `verdict`, `solver_kind`, `canonical_payload: Vec<u8>`, `content_hash`, `signature`, `verifying_key: [u8; 32]`, `format: String`.

**`pub struct ProofCertVerdict`** — `ad_gate.rs:1873`
Fields (all `bool`): `format_matches`, `payload_hash_matches`, `signature_verifies`, `is_unsat_proof`.

**`pub struct AttestationBundle`** — `ad_gate.rs:1913`
Fields: `signed_gate: SignedKillerAppGateReport`, `proof_certs: Vec<SignedProofCert>`, `audit_chain: AuditChain`.

#### impl MirAdjointInterpreter

**`pub fn new(bwd_fn: &'a MirFunc, primal_param_names: Vec<String>) -> Self`** — `ad_gate.rs:617`
Constructs and seeds the interpreter. Calls `seed_params` which maps the first `n` entry-block args to primal vars and remaining args to adjoint vars (`d_y`, `d_y_1`, etc.).

**`fn seed_params(&mut self)`** — `ad_gate.rs:629` — Private. Seeds `primal_exprs` and `adjoint_exprs` from the bwd variant's entry-block args.

**`pub fn compute_adjoint_outs(&mut self) -> Vec<AnalyticExpr>`** — `ad_gate.rs:654`
Walks the entry block ops, dispatching each through `interpret_op`. When a `cssl.diff.bwd_return` op is encountered, its operands are saved as the return list. After the walk, resolves each return operand through `adjoint_exprs` and simplifies.

**`fn interpret_op(&mut self, op: &MirOp)`** — `ad_gate.rs:673` — Private. Checks `diff_role == "adjoint"` attribute, calls `compute_op_expr`, inserts result into the appropriate table.

**`fn compute_op_expr(&self, op: &MirOp, is_adjoint: bool) -> AnalyticExpr`** — `ad_gate.rs:690` — Private. Dispatches by `op.name`:
- `"arith.constant"` → parse `value` attribute via `parse_const_value`
- `"arith.addf"` / `"arith.subf"` / `"arith.mulf"` / `"arith.divf"` → binary ops with two operands
- `"arith.negf"` → unary negation
- `"func.call"` → dispatch on `callee` attribute: `"sqrt"`, `"sin"`, `"cos"`, `"exp"`, `"log"`/`"ln"` recognized; all others become `Uninterpreted`
- `"func.return"` → `Uninterpreted("return", [])`
- other → `Uninterpreted(name, [])`

**`fn resolve_operand(&self, id: Option<ValueId>, is_adjoint: bool) -> AnalyticExpr`** — `ad_gate.rs:752` — Private. For adjoint ops, looks up adjoint table first, then primal table (cross-reference for primal values needed in gradient expressions like `contrib = d_y * b`). For primal ops, looks up primal table only.

**`fn resolve_adjoint(&self, id: ValueId) -> AnalyticExpr`** — `ad_gate.rs:772` — Private. Returns `adjoint_exprs[id]` or `Uninterpreted("?a{id}", [])`.

**`fn parse_const_value(s: &str) -> Option<f64>`** — `ad_gate.rs:783` — Module-level private. Rejects `"stage0_int"` and `"stage0_float"` placeholders (returns `None`); otherwise calls `s.parse::<f64>().ok()`.

#### impl GradientCase

**`pub fn summary(&self) -> String`** — `ad_gate.rs:824`

**`pub fn smt_query_text(&self) -> String`** — `ad_gate.rs:844`
Emits SMT-LIB text for the negated-equivalence query. Declares every free var as a `Real` constant, declares 5 uninterpreted transcendental UFs, asserts `(not (and (= mir_i analytic_i) ...))`. `(check-sat)` at end. Theory: `QF_UFNRA`.

**`pub fn to_smt_query(&self) -> Query`** — `ad_gate.rs:894`
Structured path for the same query. Builds a `cssl_smt::Query` with theory `ALL`, declares vars + UFs via `FnDecl`, builds the conjunction + negation as `Term` trees, labels the assertion `"gradient_equivalence_{sanitized_name}"` for unsat-core extraction.

**`pub fn run_smt_verification(&self, solver: &dyn Solver) -> Option<SmtVerification>`** — `ad_gate.rs:1706`
Dispatches through `solver.check_text(&self.smt_query_text())`. Returns `None` on `BinaryMissing` or any subprocess failure.

**`pub fn run_smt_verification_via_query(&self, solver: &dyn Solver) -> Option<SmtVerification>`** — `ad_gate.rs:1729`
Structured path: uses `solver.check(&self.to_smt_query())`. Preferred for unsat-core or incremental-solving hooks.

**`pub fn sign_proof_cert(&self, solver: &dyn Solver, key: &SigningKey) -> Option<SignedProofCert>`** — `ad_gate.rs:2027`
Re-emits SMT query text, dispatches solver, wraps `(query, verdict, solver_kind)` in `canonical_proof_cert_bytes`, BLAKE3-hashes, Ed25519-signs.

#### impl KillerAppGateReport

**`pub fn summary(&self) -> String`** — `ad_gate.rs:1190`

**`pub fn is_green(&self) -> bool`** — `ad_gate.rs:1205` — `passing == total`.

**`pub fn run_smt_verification(&self, solver: &dyn Solver) -> SmtVerificationReport`** — `ad_gate.rs:2058`
Iterates over cases; tallies unsat/sat/unknown/unavailable counts.

#### impl SignedKillerAppGateReport

**`pub fn summary(&self) -> String`** — `ad_gate.rs:1433`
**`pub const fn audit_tag() -> &'static str`** — `ad_gate.rs:1447` — Returns `"killer-app-gate"`.
**`pub fn audit_message(&self) -> String`** — `ad_gate.rs:1455`
**`pub fn append_to_audit_chain(&self, chain: &mut AuditChain, timestamp_s: u64)`** — `ad_gate.rs:1480`

#### impl AttestationVerdict

**`pub fn is_fully_valid(&self) -> bool`** — `ad_gate.rs:1579` — All four checks.
**`pub fn cryptographically_valid(&self) -> bool`** — `ad_gate.rs:1589` — Format + hash + sig (ignores gate-green).

#### impl SmtVerification

**`pub fn is_proof(&self) -> bool`** — `ad_gate.rs:1647` — `matches!(self.verdict, Verdict::Unsat)`.
**`pub fn summary(&self) -> String`** — `ad_gate.rs:1652`

#### impl SmtVerificationReport

**`pub fn summary(&self) -> String`** — `ad_gate.rs:1680`
**`pub fn all_decided_cases_proved(&self) -> bool`** — `ad_gate.rs:1692` — `sat_count == 0 && unknown_count == 0` (vacuously true when solver unavailable for all).

#### impl SignedProofCert

**`pub fn is_proof(&self) -> bool`** — `ad_gate.rs:1789`
**`pub fn summary(&self) -> String`** — `ad_gate.rs:1794`
**`pub const fn audit_tag() -> &'static str`** — `ad_gate.rs:1804` — Returns `"smt-proof-cert"`.
**`pub fn audit_message(&self) -> String`** — `ad_gate.rs:1810`
**`pub fn append_to_audit_chain(&self, chain: &mut AuditChain, timestamp_s: u64)`** — `ad_gate.rs:1824`

#### impl ProofCertVerdict

**`pub fn is_fully_valid(&self) -> bool`** — `ad_gate.rs:1888` — All four checks.
**`pub fn cryptographically_valid(&self) -> bool`** — `ad_gate.rs:1895` — Format + hash + sig.

#### impl AttestationBundle

**`pub fn summary(&self) -> String`** — `ad_gate.rs:1929`
**`pub fn is_fully_proven(&self) -> bool`** — `ad_gate.rs:1942` — Gate is-green + all proof-certs are Unsat proofs + chain verifies. Note: vacuously true for empty `proof_certs` (see slice note below).

#### Module-level public functions

**`fn sanitize_label(name: &str) -> String`** — `ad_gate.rs:943` — Private. Replaces non-alphanumeric-non-underscore chars with `_` for SMT-LIB assertion labels.

**`pub fn verify_gradient_case(name: &str, primal: &MirFunc, param_names: Vec<String>, analytic_gradients: Vec<AnalyticExpr>) -> GradientCase`** — `ad_gate.rs:972`
The main verification entry-point. Calls `apply_bwd(primal, &DiffRuleTable::canonical())` to get the bwd variant, runs `MirAdjointInterpreter::compute_adjoint_outs`, builds sample environments via `default_samples`, compares each adjoint with the corresponding analytic gradient via `equivalent_by_sampling` at `tolerance = 1e-9`.

**`fn default_samples(param_names: &[String]) -> Vec<HashMap<String, f64>>`** — `ad_gate.rs:1016` — Private. Generates 11-sample environments with values from `[-3.0, 3.0]` avoiding 0, alternating `d_y = ±1.0`. Each param gets a different offset per sample.

**Primal builder functions (private):**

**`fn build_binary_primal(name: &str, op_name: &str) -> MirFunc`** — `ad_gate.rs:1040` — Builds `op(param_0, param_1) → ret` with two F32 params.
**`fn build_unary_primal(name: &str, op_name: &str) -> MirFunc`** — `ad_gate.rs:1054` — Builds `op(param_0) → ret`.
**`fn build_transcendental_primal(name: &str, callee: &str) -> MirFunc`** — `ad_gate.rs:1067` — Builds `func.call @callee(param_0) → ret`.
**`fn build_chain_primal() -> MirFunc`** — `ad_gate.rs:1083` — Builds `f(x, r) = (x - r) * (x - r)`: two ops `arith.subf` and `arith.mulf`.
**`fn build_sphere_sdf_vec3_primal() -> MirFunc`** — `ad_gate.rs:1114` — Builds the scalar-expanded sphere-SDF: 4 params (`px`, `py`, `pz`, `r`), 7 ops (`mulf×3`, `addf×2`, `func.call @sqrt`, `subf`), returns `sqrt(px²+py²+pz²) - r`. Represents `length(p) - r` without vec3 primitives.

**`pub fn run_killer_app_gate() -> KillerAppGateReport`** — `ad_gate.rs:1216`
Runs all 12 canonical gradient cases (5 arithmetic, 5 transcendentals, 1 chain-rule, 1 vector-SDF scalar expansion) via `verify_gradient_case`. Returns a `KillerAppGateReport`.

**`pub const ATTESTATION_FORMAT: &str`** — `ad_gate.rs:1401` — `"CSSLv3-R18-KILLER-APP-GATE-v1"`.

**`pub fn canonical_report_bytes(report: &KillerAppGateReport) -> Vec<u8>`** — `ad_gate.rs:1510`
Deterministic UTF-8 serialization of a gate report. Format: format-tag line, `total=N`, `passing=N`, per-case lines `"case[i]: name | match=bool | params=csv"` and per-param lines. Terminated with `"end\n"`.

**`pub fn sign_gate_report(report: KillerAppGateReport, key: &SigningKey) -> SignedKillerAppGateReport`** — `ad_gate.rs:1540`
Hashes canonical payload via BLAKE3, signs via Ed25519, packages into `SignedKillerAppGateReport`.

**`pub fn verify_signed_gate_report(signed: &SignedKillerAppGateReport, expected_verifying_key: &[u8; 32]) -> AttestationVerdict`** — `ad_gate.rs:1600`
Recomputes canonical payload, hashes, verifies signature. Returns `AttestationVerdict` with per-step status.

**`pub const PROOF_CERT_FORMAT: &str`** — `ad_gate.rs:1749` — `"CSSLv3-R18-SMT-PROOF-CERT-v1"`.

**`pub fn canonical_proof_cert_bytes(case_name: &str, query_text: &str, verdict: Verdict, solver_kind: SolverKind) -> Vec<u8>`** — `ad_gate.rs:1848`
Deterministic serialization: format-tag, case, verdict, solver, `query-len=N`, `query:\n<N bytes>`, `end\n`.

**`pub fn verify_signed_proof_cert(cert: &SignedProofCert, expected_verifying_key: &[u8; 32]) -> ProofCertVerdict`** — `ad_gate.rs:1991`

**`pub fn run_full_attestation_stack(solver: &dyn Solver, key: &SigningKey, timestamp_s_base: u64) -> AttestationBundle`** — `ad_gate.rs:1959`
Runs the gate, signs the report, iterates cases to sign proof-certs (if solver available), appends all to a fresh `AuditChain` in deterministic order (gate-seal first, certs in case-order with timestamps `base + i + 1`).

**`fn hex_short(bytes: &[u8], n: usize) -> String`** — `ad_gate.rs:1485` — Private. Hex-encodes first `n` bytes.

#### Test module (`ad_gate.rs:2082`)

~100 tests organized in 8 groups:
1. **AnalyticExpr algebra:** simplify (add-zero, mul-zero, mul-one, double-neg), evaluate, `equivalent_by_sampling` (commutativity), `to_smt`, `free_vars`.
2. **MirAdjointInterpreter:** seeding check (primal/adjoint table sizes).
3. **Per-primitive gradient:** FAdd, FSub, FMul, FDiv, FNeg, Sqrt, Sin, Cos, Exp, Log, Chain rule — each calls `verify_gradient_case` and asserts `all_match`.
4. **Top-level gate:** `killer_app_gate_all_cases_pass` asserts `total = 12`, `passing = 12`, `is_green()`. `gate_summary_shape`. `gate_smt_query_text_contains_declarations_and_assertion`. `gate_chain_gradient_numerically_matches_at_point` (evaluates at `(x=3, r=1, d_y=1)` and checks `dx=4`, `dr=-4`).
5. **R18 attestation:** `attestation_format_tag_is_stable`, `canonical_bytes_is_deterministic_across_calls`, `canonical_bytes_contains_every_case`, `sign_then_verify_roundtrip_fully_valid`, `verify_fails_under_wrong_key`, `tampered_report_fails_payload_hash_check`, `tampered_format_tag_fails_format_check`, `tampered_signature_fails_signature_check`, `signed_report_summary_contains_gate_verdict`, `signing_is_deterministic_under_fixed_seed`, `cryptographically_valid_accepts_failed_gate_when_hash_and_sig_ok`.
6. **SMT verification (stub solvers):** `MissingBinarySolver` (always BinaryMissing), `FixedVerdictSolver(Verdict)` (always returns fixed verdict). Tests: unavailable path, unsat/sat wrapping, counts under fixed-unsat/missing-solver, `all_decided_cases_proved`, real Z3 subprocess (graceful whether binary present or not).
7. **AnalyticExpr → cssl_smt::Term:** integer constant, 0.5 rational encoding, var, add, sub/neg/div, transcendentals as UF apps.
8. **GradientCase::to_smt_query:** shape, sorts, label sanitization, both paths return None when solver missing, wrap verdict when present, render-matches-text-shape.
9. **AuditChain integration:** audit_tag stable, audit_message shape and content, `append_to_audit_chain` (len=1, tag correct), multi-run sequential chain, tamper detection (verify_chain).
10. **Proof-cert emission:** format tag stable, canonical bytes deterministic, sign returns None when missing, sign_under_fixed_unsat produces valid cert, tampered query_text fails hash, wrong key fails sig, Sat cert is cryptographically-valid but not proof, cert appends to chain.
11. **AttestationBundle:** fixed-unsat solver → fully-proven, missing solver → zero certs + 1-entry chain + vacuously proven (documented edge case), chain ordering deterministic, summary shape, roundtrip determinism under fixed seed, forged-Sat solver → not fully-proven.
12. **ProofCertVerdict:** all-false not valid, all-true fully valid.

**Private test helpers:**
- `fn env2(a_name, a, b_name, b, d_y) -> HashMap<...>` — 2-var + d_y environment builder.
- `fn fixed_seed_key() -> SigningKey` — deterministic 32-byte seed `[i*7+13]` for test reproducibility.
- `struct MissingBinarySolver` and `struct FixedVerdictSolver(Verdict)` — test stub solvers implementing `cssl_smt::Solver`.

**Notable potential issue:** `ad_gate.rs:753`
```rust
AnalyticExpr::Uninterpreted(format!("?missing_operand"), Vec::new())
```
The `format!("?missing_operand")` with no arguments is a useless-format — `"?missing_operand".to_string()` would be cleaner. Clippy `useless_format` is explicitly allowed at module top (`#![allow(clippy::useless_format)]`). This is a known and intentional allowance, not a bug.

**Documented edge case:** `ad_gate.rs:3216` comment confirms: `is_fully_proven()` returns `true` vacuously when `proof_certs` is empty (all-elements-satisfy-predicate on empty vec). The code comment explicitly documents this as intentional ("an unavailable solver doesn't invalidate structural correctness"). Callers requiring a non-empty proof-set must check `proof_certs.len() > 0` separately.

---

### 3.6 `compiler-rs/crates/cssl-examples/src/jit_chain.rs` (1,589 lines)

**Purpose:** End-to-end JIT integration — CSSLv3 source through the full pipeline down to Cranelift-JIT-compiled machine code with runtime gradient verification. Implements T11-D23 through T11-D44 (vec3 scalarization, scene-SDF intrinsics, bwd-mode per-param adjoint extraction, inter-fn calls, generic monomorphization).

Imports: `cssl_ast`, `cssl_autodiff::AdWalker`, `cssl_cgen_cpu_cranelift::{JitError, JitModule}`, `cssl_mir::{MirFunc, MirModule}`.

#### Structs

**`pub struct JitChainHandle`** — `jit_chain.rs:151`
Fields: `pub module: JitModule`, `pub primal_fn: cssl_cgen_cpu_cranelift::JitFn`, `pub tangent_fn: cssl_cgen_cpu_cranelift::JitFn`.
Bundles the live JIT module with the two compiled fn handles so code stays mapped while callers invoke. Implements `core::fmt::Debug` manually (elides internal Cranelift state per the `missing_fields_in_debug` allowance at workspace level).

#### Functions

**`pub fn pipeline_source_to_ad_mir(name: &str, source: &str) -> MirModule`** — `jit_chain.rs:40`
Drives CSSLv3 source through lex → parse → HIR lower → per-fn MIR lower (signature + body) → `AdWalker::transform_module`. Returns the populated `MirModule` containing primal fns + `_fwd` / `_bwd` variants. Panics on fatal lex/parse/HIR errors (intended for test fixtures with known-valid source). Marked `#[must_use]`.

**`pub fn extract_bwd_single_adjoint(bwd_variant: &MirFunc, adjoint_index: usize) -> MirFunc`** — `jit_chain.rs:79`
Post-processes a multi-result bwd variant to produce a single-adjoint fn. Clones the variant, replaces `results` with just the `adjoint_index`-th type, renames to `"{name}_d{adjoint_index}"`, and rewrites the `cssl.diff.bwd_return` op's operand list to keep only operand-at-index. Panics if `adjoint_index >= bwd_variant.results.len()`. Marked `#[must_use]`.

**`pub fn extract_tangent_only_variant(fwd_variant: &MirFunc) -> MirFunc`** — `jit_chain.rs:107`
Post-processes a multi-result fwd variant to produce a single-tangent fn by keeping only the last result type and rewriting `func.return` to keep only the last operand. Renames to `"{name}_tangent_only"`. Marked `#[must_use]`.

**`pub fn jit_primal_and_tangent(primal: &MirFunc, tangent_only: &MirFunc) -> Result<JitChainHandle, JitError>`** — `jit_chain.rs:134`
Compiles both fns in a shared `JitModule`, finalizes, and returns a `JitChainHandle`. Propagates any `JitError`.

#### Test module (`jit_chain.rs:166`)

28 tests (all in `mod tests`):

1. `pipeline_source_emits_fwd_variant_for_differentiable_fn` — asserts `sphere_sdf` and `sphere_sdf_fwd` appear in the module.
2. `extract_tangent_only_drops_primal_result` — verifies `results.len() == 1`, name ends with `"_tangent_only"`, `func.return` has 1 operand.
3. **`full_chain_source_to_jit_sphere_sdf_gradient`** (T11-D23 killer test) — JIT-executes `sphere_sdf(p, r) = p - r`, verifies primal (5.0 - 2.0 = 3.0), then tangent with `d_p=1,d_r=0` → 1.0 and `d_p=0,d_r=1` → -1.0. Cross-checks against central differences at 4 sample points.
4. `full_chain_source_to_jit_fmul_gradient` — `a * b`, verifies `∂/∂a = b` and `∂/∂b = a` at (3,5).
5. **`full_chain_source_scene_sdf_min_runtime_gradient`** (T11-D24) — `min(a, b)` intrinsic. Verifies pick-the-winner gradient. Central-diff cross-check at 5 cusp-avoided samples.
6. `full_chain_source_scene_sdf_max_runtime_gradient` — `max(a, b)`.
7. `full_chain_source_inter_fn_call_runtime` (T11-D26) — Multi-fn: `helper(x) = x*x`, `scene(x) = helper(x)`. Verifies JIT resolves callee.
8. `full_chain_source_bwd_mul_per_param_adjoints` (T11-D27) — `mul(a, b) = a*b`, extracts `d_a` and `d_b` via `extract_bwd_single_adjoint`. Verifies at (3,5,d_y=1): `d_a=5`, `d_b=3`. Central-diff cross-check.
9. `full_chain_source_scene_sdf_union_composition` (T11-D28) — Two-sphere union via `min(sphere_sdf(p,r0), sphere_sdf(p,r1))`. Verifies primal and central-diff ∂/∂p and ∂/∂r0 at 4 samples.
10. `full_chain_source_bwd_two_params_affine` — `lin2(a, b) = a + a + b`, `∂/∂a = 2`, `∂/∂b = 1`.
11. `full_chain_source_bwd_sq_adjoint` — `sq(x) = x*x`, `∂(x²)/∂x = 2x`. Analytic + central-diff.
12. `full_chain_source_bwd_cube_adjoint` — `cube(x) = x*x*x`, `∂/∂x = 3x²`.
13. `full_chain_source_bwd_affine_adjoint` — `affine(x) = x+x+x`, `∂/∂x = 3`.
14. `full_chain_source_scene_sdf_abs_runtime_gradient` — `abs(a)`. Verifies sign-based tangent (+1 for a>0, -1 for a<0).
15. **`full_chain_source_to_jit_sphere_sdf_vec3_gradient_matches_normalize`** (T11-D35) — Real `vec3<f32>` sphere-SDF. Verifies: primal has 4 scalar params after scalarization, fwd has 8. Primal at (3,0,4,1) = 4. Forward tangent: `∂/∂p_0 = 0.6`, `∂/∂p_1 = 0.0`, `∂/∂p_2 = 0.8`, `∂/∂r = -1.0`. Plus central-diff cross-check at each lane.
16. `sphere_sdf_vec3_param_scalarization_produces_4_scalar_params` — regression guard: signature lowering scalarizes `vec3<f32>` before body lowering.
17. `sphere_sdf_vec3_length_expansion_emits_scalar_ops` — regression guard: `length(p)` inlines to ≥3 `arith.mulf` + ≥2 `arith.addf` + `func.call @sqrt`.
18. `full_chain_sphere_sdf_vec3_bwd_mode_gradient` (T11-D37) — Bwd-mode counterpart. Extracts each of 4 adjoints separately. Verifies at (3,0,4,r=1,d_y=1): `[0.6, 0.0, 0.8, -1.0]`.
19. `full_chain_vec2_length_runtime` — `len2(vec2<f32>) = length(p)`. Verifies `length(3,4) = 5.0`.
20. `full_chain_vec4_length_runtime` — `len4(vec4<f32>) = length(p)`. Verifies `length(2,3,6,0) = 7.0`.
21. `monomorph_specialize_id_i32_jit_executes` (T11-D38) — Manual `specialize_generic_fn` call. `fn id<T>`, specialize T↦i32 and T↦f32, JIT both. Calls: `id_i32(5)=5`, `id_i32(-42)=-42`, `id_f32(2.5)≈2.5`.
22. `auto_monomorphize_discovers_specializations_from_turbofish_calls` (T11-D40) — `auto_monomorphize` discovers `id::<i32>` and `id::<f32>` call sites. Asserts `specialization_count=2`, deduplication. JIT-compiles and calls both.
23. `end_to_end_main_calls_generic_id_via_full_flow` (T11-D42) — Full pipeline: parse → HIR → lower → `auto_monomorphize` → `rewrite_generic_call_sites` (1 rewrite) → `drop_unspecialized_generic_fns` (1 dropped) → JIT `id_i32` + `main` → `call_unit_to_i32()` returns 5.
24. `end_to_end_generic_add_specializes_and_computes` (T11-D44) — `add<T>(a, b) = a + b`, specialize T↦i32. `main() = add_i32(3, 4) = 7`.
25. `end_to_end_generic_twice_specializes_and_computes_f32` — `twice<T>(x) = x + x`, specialize T↦f32. `twice_f32(2.5) = 5.0`. Also checks `main_f32` has 0 params and correct result type.
26. `auto_monomorphize_deduplicates_same_type_args` — 3 call sites to `id::<i32>` from functions `a`, `b`, `c` → 1 specialization. All 3 map to same name.
27. `vec_scalarization_preserves_scalar_params_untouched` (T11-D37 regression) — `fn mix(p: vec3<f32>, r: f32, s: f32)` → 5 scalar params (3+1+1).

**Note on T11-D25 (Bwd-mode single-float-param, commented section):** `jit_chain.rs:455` marks "§ T11-D25" but the test for single-float-param bwd is not present as a named test in the file; the capability is instead covered by `full_chain_source_bwd_sq_adjoint` (test 11). The section comment at line 455-457 is a placeholder with no body.

---

## 4. WORKSPACE METADATA

### `compiler-rs/Cargo.toml` (134 lines)

#### `[workspace]`
- `resolver = "2"` — Cargo resolver 2 (feature unification semantics)
- `members = ["crates/*"]` — glob discovery; all crates under `compiler-rs/crates/` are workspace members

#### `[workspace.package]`
Shared for all members via `.workspace = true` inheritance:
- `version = "0.1.0"`
- `edition = "2021"`
- `rust-version = "1.75"` — MSRV
- `license = "Apache-2.0 OR MIT"`
- `authors = ["Apocky <apocky13@gmail.com>"]`
- `repository = "https://github.com/Apocky/CSSL3"`
- `homepage = "https://cssl.dev"`

#### `[workspace.lints.clippy]` — fully documented allowances

| lint | level | rationale |
|---|---|---|
| `all` | deny (-1) | baseline; maximum strictness |
| `pedantic` | warn (-1) | additional style checks |
| `nursery` | warn (-1) | unstable but useful lints |
| `module_name_repetitions` | allow | scaffold naming conventions |
| `missing_errors_doc` | allow | scaffold phase |
| `missing_panics_doc` | allow | scaffold phase |
| `must_use_candidate` | allow | scaffold phase |
| `missing_const_for_fn` | allow | scaffold phase |
| `too_many_lines` | allow | scaffold phase |
| `doc_markdown` | allow | CSSLv3/SPIR-V/MLIR/DXIL names in docs |
| `cast_possible_truncation` | allow | scaffold |
| `cast_sign_loss` | allow | scaffold |
| `cast_lossless` | allow | scaffold |
| `default_trait_access` | allow | scaffold |
| `unreadable_literal` | allow | scaffold |
| `derive_partial_eq_without_eq` | allow | f32/f64 structs can't derive Eq (NaN) |
| `doc_lazy_continuation` | allow | T11-D20 toolchain bump (1.75→1.85) surfaced this; indented bullets not recognized by newer clippy |
| `too_long_first_doc_paragraph` | allow | scaffold module docs have rich first-para summaries |
| `const_is_empty` | allow | `assert!(!STAGE0_SCAFFOLD.is_empty())` — trivially-true guard-rail |
| `needless_lifetimes` | allow | explicit lifetimes preserved for readability in parser/HIR walkers |
| `single_match_else` | allow | match-else reads clearer for some pattern discriminations |
| `needless_pass_by_ref_mut` | allow | walker fns take `&mut self` for future extension |
| `or_fun_call` | allow | `unwrap_or(String::new())` preserved for readability |
| `use_self` | allow | scaffolding prefers fully-qualified type names for discoverability |
| `literal_string_with_formatting_args` | allow | CSLv3-notation strings contain `{}` that are not format-args |
| `assigning_clones` | allow | `x = y.clone()` preserved over `y.clone_from(&x)` for readability |
| `missing_fields_in_debug` | allow | `JitModule Debug` intentionally elides internal Cranelift state |
| `needless_pass_by_value` | allow | closures passed by value in builder-pattern APIs |

#### `[workspace.lints.rust]`
- `rust_2018_idioms = "warn"`
- `unreachable_pub = "warn"`

#### `[workspace.dependencies]` (grouped as in the file)

**Frontend (T2/T3):**
- `logos = "0.14"` — lexer generator
- `chumsky = "0.10"` — parser combinator
- `lasso = "0.7"` — string interner

**Shared data:**
- `smallvec = { version = "1", features = ["union", "const_generics"] }`
- `indexmap = "2"`
- `bitflags = { version = "2", features = ["serde"] }`
- `bytemuck = { version = "1", features = ["derive"] }`

**Diagnostics:**
- `miette = { version = "7", features = ["fancy"] }`
- `thiserror = "1"`
- `tracing = "0.1"`
- `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`

**SMT (T9):**
- `z3 = { version = "0.12", default-features = false }`
- `# cvc5-sys` — commented out, T9: verify-registry
- `# klee-sys` — commented out, T9: verify-registry

**MLIR (T6):** both commented out pending Windows compat + LLVM prefix verification:
- `# melior = "0.20"`, `# mlir-sys = "0.3"`

**Codegen CPU (T10) — Cranelift 0.115:**
- `cranelift-codegen`, `cranelift-frontend`, `cranelift-module`, `cranelift-object`, `cranelift-jit`, `cranelift-native`

**Codegen GPU (T10):**
- `rspirv = "0.12"`, `spirv-tools = "0.12"`

**HW/Host (T10):**
- `ash = "0.38"` — Vulkan
- `# level-zero-sys = "0.3"` — commented out, T10: verify-registry
- `windows = { version = "0.58", features = ["Win32_Foundation"] }`
- `wgpu = "23"`
- `naga = { version = "23", features = ["wgsl-in"] }` — pinned to match wgpu 23's internal naga (T11-D32)

**Crypto (R18, T11):**
- `blake3 = "1"`
- `ed25519-dalek = { version = "2", features = ["rand_core"] }`
- `rand = { version = "0.8", features = ["std"] }`
- `cpufeatures = "=0.2.17"` — pinned; 0.3.0 requires edition2024, not MSRV-compatible

**Serde/Config:**
- `serde = { version = "1", features = ["derive"] }`
- `serde_json = "1"`
- `toml = "0.8"`

**Concurrency:**
- `parking_lot = "0.12"`, `crossbeam = "0.8"`

**Testing/Property (T11):**
- `proptest = "1"`, `insta = "1"`

**Bench (T11):**
- `criterion = { version = "0.5", features = ["html_reports"] }`

#### `[workspace.metadata.cssl]`
Custom metadata (read by tooling, not Cargo itself):
```toml
prime_directive  = "consent=OS • violation=bug • no-override-exists"
spec_root        = "../specs"
research_root    = "../research"
stage            = "0"
stage_note       = "Rust-hosted scaffold per §§ 01_BOOTSTRAP • stage1-self-host pending"
msrv_rationale   = "1.75 = workspace-lints-stable + resolver-2 mature"
ci_faithful_to   = "§§ 23_TESTING oracle-modes + differential-backends"
decisions_log    = "../DECISIONS.md"
```

---

### `compiler-rs/rust-toolchain.toml` (12 lines)

```toml
[toolchain]
channel    = "1.85.0"
components = ["rustfmt", "clippy"]
profile    = "minimal"
```

- **Channel:** `1.85.0` — pinned per R16 reproducibility. Bumped from 1.75.0 at T11-D20 to unblock `cranelift-jit` activation (edition2024 dependency chain). History recorded in the file header.
- **Components:** `rustfmt` and `clippy` only — `profile = "minimal"` avoids pulling docs/std-src/etc.
- **Rationale:** Toolchain-pins every commit so CI and contributors always produce identical codegen and lint results.

---

### `compiler-rs/rustfmt.toml` (9 lines)

```toml
edition                  = "2021"
max_width                = 100
tab_spaces               = 4
newline_style            = "Unix"
use_field_init_shorthand = true
use_try_shorthand        = true
```

Uses stable `rustfmt` options only (comment states unstable options deferred until nightly-pin or 1.80+ MSRV bump). 100-char line limit; Unix line endings; shorthand for field init and `?` operator.

---

### Placeholder Directories

**`compiler-rs/tests/golden/.gitkeep`** — Empty directory committed via `.gitkeep`. Intended for future snapshot/golden-file tests (per spec `§§ 23_TESTING oracle-modes + test --update-golden` subcommand listed in `csslc/main.rs`). No golden files exist yet.

**`compiler-rs/.perf-baseline/.gitkeep`** — Empty directory committed via `.gitkeep`. Intended for future performance baseline files (per `bench --update-baseline` subcommand reference). No baseline files exist yet.

---

### `compiler-rs/Cargo.lock` (1,272 lines, 149 packages)

Committed for R16 reproducibility — identical dependency graph across all CI runs and contributor machines. Contains 149 external packages including: cranelift family (codegen, frontend, module, object, jit, native), wgpu, naga, ash, rspirv, spirv-tools, blake3, ed25519-dalek, rand, z3, logos, chumsky, lasso, miette, thiserror, tracing, serde, proptest, insta, criterion, indexmap, smallvec, bitflags, bytemuck, parking_lot, crossbeam, windows, toml, serde_json. Notable: the Cranelift suite alone pulls in several supporting crates (regalloc2, cranelift-bforest, etc.); wgpu pulls in its full ecosystem including naga, raw-window-handle, etc.

---

## 5. SLICE NOTES

### csslc maturity

`csslc` is not a functioning compiler driver. The binary is entirely a scaffold — `main()` does nothing but print and exit. There is no argument parsing, no pipeline invocation, no file reading, no subcommand dispatch. The entire compile pipeline is exercised through library crates and the `cssl-examples` test suite; `csslc` as an executable is currently a named placeholder for future CLI integration. No crates are imported by `csslc` (no `[dependencies]` in its Cargo.toml). All maturity is in the library crates.

### Test coverage

`cssl-examples` has excellent internal test coverage given its scope:
- `lib.rs`: 18 tests covering pipeline, F1-chain, and edge-case predicates
- `stage1_scaffold.rs`: 8 tests including the grammar-regression canary
- `analytic_vec3.rs`: 56 tests covering the full scalar/vec3 symbolic algebra
- `ad_gate.rs`: ~100 tests covering the killer-app gate, R18 attestation, SMT verification, AuditChain
- `jit_chain.rs`: 28 tests including end-to-end JIT execution with central-difference gradient verification

No external integration test directory exists under `compiler-rs/tests/` (the `golden/` subdirectory holds only a `.gitkeep`).

### Deferred / Incomplete

The following capabilities are explicitly deferred (documented in module docstrings):
- **T12-phase-2:** Full type-check, refinement-obligation verification via solver, MIR codegen backends, spirv-val/dxc/naga round-trip, Vulkan pixel rendering, bit-exact AD verification.
- **T9-phase-2:** Real solver dispatch for gradient equivalence proofs (Z3/CVC5 binary required; tests handle unavailability gracefully via `BinaryMissing`).
- **T7-phase-2d:** Vector-SDF `length(p) - r` in `ad_gate.rs` symbolic form (the scalar-expanded `build_sphere_sdf_vec3_primal` is a surrogate; the real vec3 path is T11-D35 in `jit_chain.rs`).

The `T11-D25` section marker in `jit_chain.rs:455` ("§ T11-D25 : Bwd-mode (reverse) full-chain JIT integration") is present as a comment with no associated test body. The capability is demonstrably working (covered by downstream tests like `full_chain_source_bwd_sq_adjoint`), but the named section placeholder was not filled in with an explicitly-named D25 test.

### Surprises / Notable Findings

1. **`csslc` has no dependencies.** The compiler binary crate has an empty `[dependencies]` section. It does not link to any of the 32 compiler crates. This means `cargo build --bin csslc` builds in seconds and is entirely symbolic — it proves nothing about pipeline integration. The actual integration proof is in `cssl-examples`.

2. **`cpufeatures` version pin is exact.** `cpufeatures = "=0.2.17"` — a hard pin to avoid 0.3.0's edition2024 requirement. This is a documented compatibility constraint and will need attention on the next toolchain bump that moves MSRV past edition2024.

3. **MLIR (melior, mlir-sys) and Level Zero are fully commented out.** These backends remain at `# ...` status. T6 (MLIR) is documented as blocked on Windows compatibility and `LLVM_SYS_*_PREFIX` resolution. Level Zero is "T10: verify-registry-availability."

4. **`is_fully_proven()` vacuous truth on empty proof-certs.** As documented at `ad_gate.rs:3216`, `AttestationBundle::is_fully_proven()` returns `true` when `proof_certs` is empty because `Vec::iter().all(...)` is vacuously true. The code comment explicitly calls this out as intentional, but external auditors using this predicate must additionally check `proof_certs.len() == report.total` to confirm all cases have SMT-backed proofs.

5. **`parse_const_value` rejects placeholders cleanly.** `ad_gate.rs:783` special-cases `"stage0_int"` and `"stage0_float"` attribute values by returning `None`, which maps to `0.0` in the caller. This is a well-documented forward-compatibility gap: the MIR body-lowering emits placeholder values for literals it cannot yet extract from HIR, and the interpreter handles them gracefully without panicking.

6. **Stage-0 acceptance criterion is zero parse errors, not zero all diagnostics.** `PipelineOutcome::is_accepted()` checks only `parse_error_count == 0`. Lower-level HIR diagnostics (name-resolution warnings for unresolved stdlib paths) are counted but do not fail acceptance in stage-0.

7. **The `all_stage1_scaffold_files_accepted` canary is the only grammar-regression guard.** As the grammar evolves, this test in `stage1_scaffold.rs` is the only automated check that the stage-1 placeholder files remain accepted. It asserts both `stage1/hello.cssl` and `stage1/compiler.cssl` parse cleanly. If a grammar change breaks either, this test is the first signal.

8. **No README in cssl-examples or csslc.** Neither crate has a `README.md`. Documentation is purely in-code (`//!` module docstrings) and in spec files under `../specs/`.
