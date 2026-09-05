# Audit: F1 Automatic Differentiation Slice
## Crates: `cssl-autodiff` + `cssl-jets`

**Auditor:** Claude Sonnet 4.6  
**Date:** 2026-05-14  
**Spec authority:** `specs/05_AUTODIFF.csl`, `specs/17_JETS.csl`  
**Files audited:** 8 (6 `.rs` + 2 `Cargo.toml`)  
**Total LOC:** ~4,354 (cssl-autodiff: ~4,061; cssl-jets: ~293)

---

## 1. SLICE OVERVIEW

Language feature F1 provides source-to-source automatic differentiation (AD) operating on the Mid-level Intermediate Representation (MIR), rather than on source text or HIR expressions. The implementation spans two crates:

**`cssl-autodiff`** is the primary AD engine. Given a function annotated `@differentiable` in CSSL source, it produces two new `MirFunc` values appended to the owning `MirModule`: a forward-mode JVP variant (named `<fn>_fwd`) and a reverse-mode VJP variant (named `<fn>_bwd`). The transform is "source-to-source on MIR" in the sense that it clones the primal function's MIR ops and interleaves new ops derived from the chain rule, without invoking any external AD library or compiler pass outside the workspace.

**Forward mode (JVP):** The forward variant takes extra tangent input parameters (one `d_x` per float primal param, interleaved immediately after each primal param) and returns extra tangent results (one `d_y` per float primal result, appended). As it walks each primal op in order, it looks up the op's primitive class, applies the corresponding forward rule, and emits a sequence of concrete `arith.*` ops carrying the `diff_role = "tangent"` attribute. The primal op is preserved in-place; tangent ops immediately follow it. At `func.return`, the tangent of the return value is appended as an extra operand.

**Reverse mode (VJP):** The reverse variant takes extra adjoint-input parameters (one `d_y` per float primal result, seeded by the caller) and returns extra adjoint-out results (one `d_x` per float primal param). The primal ops are first zero-initialized for all float-param adjoints, then walked in reverse order. For each recognized primitive, adjoint-accumulation ops are emitted that route the incoming adjoint backwards through the chain rule. The function body ends with a custom `cssl.diff.bwd_return` terminator (not `func.return`), which carries the accumulated adjoint values for the original float params. The primal `func.return` is stripped to avoid mid-block terminator issues.

**Piecewise-linear primitives (T11-D13 through D16):** The crate covers not only the classical smooth transcendentals but also `min`, `max`, `abs`, and `sign`. For `min`/`max`, the tangent/adjoint is routed by a runtime comparison (`arith.cmpf` with predicate `ole`/`oge`) followed by `arith.select` — a real, branchful, MIR-level subgradient. For `abs`, the sign of the primal input determines which branch of the tangent is selected. For `sign`, the derivative is treated as uniformly zero (zero-gradient convention at stage-0).

**What `cssl-jets` adds:** `Jet<T,N>` is a higher-order AD type — a truncated Taylor series of order N, carrying the primal value plus N derivative coefficients. The `cssl-jets` crate defines the abstract type machinery: `JetOrder`, `JetOp` (5 operations), `JetSignature`, `JetError`, and three standalone validation functions. No actual runtime representation (struct fields, array layout) or code-generation logic is implemented here; that is explicitly deferred to the `cssl-staging` pass (T8) and later phases. The crate is purely a schema and validation layer for stage-0.

**Relation to MIR and HIR:** The autodiff crate consumes `cssl-hir` to discover `@differentiable` annotations via the `ad_legality` pass (exposed by `collect_differentiable_fns`) and produces new `MirFunc` values in `cssl-mir`. It does not produce HIR output — the transform lives entirely at the MIR level. The `cssl-jets` crate is standalone (depends only on `thiserror`) and has no MIR or HIR dependencies at stage-0; it feeds into `cssl-staging` which is not part of this audit slice.

---

## 2. CRATE INVENTORY

### 2.1 `cssl-autodiff`

**Path:** `compiler-rs/crates/cssl-autodiff/`  
**Purpose:** Source-to-source automatic differentiation on MIR. Discovers `@differentiable`-annotated functions from HIR, builds a canonical differentiation rule table for 19 primitives, and emits forward-mode JVP and reverse-mode VJP `MirFunc` variants carrying real `arith.*` tangent and adjoint ops. Acts as the stage-0 implementation of language feature F1.  
**Pipeline role:** Post-HIR, pre-codegen. Plugs into the `cssl-mir` pass pipeline via the `AdWalkerPass` adapter.

**Cargo.toml dependencies:**
- `cssl-ast` (path) — `SourceFile`, `SourceId`, `Surface` (used in tests only)
- `cssl-hir` (path) — `HirAttr`, `HirFn`, `HirItem`, `HirModule`, `Interner`, `Symbol`, `DefId`
- `cssl-mir` (path) — `MirFunc`, `MirOp`, `MirRegion`, `MirType`, `MirValue`, `ValueId`, `FloatWidth`, `MirPass`, `PassPipeline`, `PassResult`, `PassDiagnostic`
- `thiserror` (workspace)

**Dev-dependencies:** `cssl-lex`, `cssl-parse` (both path; used to set up parse-then-lower pipelines in tests)

**Total LOC:** ~4,061 across 6 source files  
**Files:**
- `src/lib.rs` — 57 lines
- `src/decl.rs` — 152 lines
- `src/rules.rs` — 437 lines
- `src/transform.rs` — 177 lines
- `src/walker.rs` — 811 lines
- `src/substitute.rs` — 2,427 lines

---

### 2.2 `cssl-jets`

**Path:** `compiler-rs/crates/cssl-jets/`  
**Purpose:** Abstract type machinery for `Jet<T,N>` higher-order AD via truncated Taylor series. Defines the `JetOrder`, `JetOp`, `JetSignature`, and `JetError` types plus three validation functions. No runtime struct layout or code-generation is performed; actual staging specialization deferred to T8 (`cssl-staging`).  
**Pipeline role:** Type/validation layer. Consumed by `cssl-staging` (not yet wired). No direct MIR or HIR dependency.

**Cargo.toml dependencies:**
- `thiserror` (workspace only)

**Total LOC:** 293  
**Files:**
- `src/lib.rs` — 293 lines

---

## 3. FILE-BY-FILE AUDIT

### 3.1 `cssl-autodiff/src/lib.rs` (57 lines)

The crate root. Re-exports all public items from the five submodules. Declares a single `pub const STAGE0_SCAFFOLD: &str` exposing the crate version. Contains a single trivial test (`scaffold_version_present`) that asserts the version string is non-empty.

**Items:**

| Item | Kind | Description |
|---|---|---|
| `pub mod decl` | module | `@differentiable` declaration extraction |
| `pub mod rules` | module | Per-primitive AD rule table |
| `pub mod substitute` | module | Real tangent/adjoint op emission |
| `pub mod transform` | module | HIR-level variant name table |
| `pub mod walker` | module | MIR module driver + pass adapter |
| `pub use decl::{collect_differentiable_fns, DiffDecl}` | re-export | |
| `pub use rules::{DiffMode, DiffRule, DiffRuleTable, Primitive}` | re-export | |
| `pub use substitute::{apply_bwd, apply_fwd, SubstitutionReport, TangentMap}` | re-export | |
| `pub use transform::{DiffTransform, DiffVariants}` | re-export | |
| `pub use walker::{op_to_primitive, specialize_transcendental, AdWalker, AdWalkerPass, AdWalkerReport}` | re-export | |
| `STAGE0_SCAFFOLD: &str` | const | Crate version from `CARGO_PKG_VERSION` |
| `scaffold_tests::scaffold_version_present` | test | Version string non-empty |

**Notable deferred work** (quoted from module-level comment at `lib.rs:17–22`):
```
//! § T7-phase-2c DEFERRED
//!   - Tape-buffer allocation (iso-capability scoped) for control-flow.
//!   - `@checkpoint` attribute recognition.
//!   - GPU-AD tape-location resolution.
//!   - Multi-result tangent-tuple emission.
//!   - Killer-app gate : `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic.
```

---

### 3.2 `cssl-autodiff/src/decl.rs` (152 lines)

Extracts `@differentiable` function declarations from a HIR module. Walks the HIR tree and collects `DiffDecl` records for every function annotated with `@differentiable`, subject to override by `@NoDiff`.

**Structs:**

**`DiffDecl`** (line 7)  
Fields: `name: Symbol`, `def: DefId`, `param_count: usize`, `no_diff: bool`, `lipschitz_bound: Option<String>`, `checkpoint: bool`. Carries the full AD-annotation metadata for one function. Invariant: `name` and `def` always refer to the same function; `no_diff` overrides `@differentiable` if both are present.

**Functions (pub):**

- `DiffDecl::from_fn(f: &HirFn, interner: &Interner) -> Option<Self>` (line 25)  
  Constructs a `DiffDecl` from a `HirFn`. Returns `None` if `@differentiable` is absent. Reads `@NoDiff`, `@checkpoint`, and `@lipschitz`. Note: Lipschitz bound extraction is a stage-0 placeholder — it always records `"k"` as the bound string regardless of the `@lipschitz(k = N)` argument (`decl.rs:42`: `// Stage-0 placeholder ; full arg-extraction @ T7-phase-2.`).

- `collect_differentiable_fns(module: &HirModule, interner: &Interner) -> Vec<DiffDecl>` (line 65)  
  Top-level entry point. Walks all `HirItem`s in the module and returns all collected decls in encounter order.

**Functions (private):**

- `collect_item(item: &HirItem, interner: &Interner, out: &mut Vec<DiffDecl>)` (line 73)  
  Recursively handles `HirItem::Fn`, `HirItem::Impl` (iterates methods), and `HirItem::Module` (recurses into nested modules). All other item kinds are silently skipped.

- `attr_matches(attr: &HirAttr, interner: &Interner, expected: &str) -> bool` (line 54)  
  Compares a single-segment attribute path against an expected name string. Multi-segment paths (e.g., `@cssl.diff.primal`) return `false`.

**Test module `tests`** (line 98): 4 tests covering empty module (no decls), single differentiable fn (param count correct), non-differentiable fn (skipped), and multiple fns (all collected in order). Tests drive the full lex → parse → lower → HIR pipeline.

**Incomplete behavior noted:** The `@lipschitz` arg value is not parsed — `lipschitz_bound` is always `Some("k".to_string())` if the attribute is present, never the numeric value. This is documented inline but is a semantic gap.

---

### 3.3 `cssl-autodiff/src/rules.rs` (437 lines)

Defines the differentiation modes, the 19 recognized primitive operations, and the canonical 38-rule table (Fwd + Bwd for each primitive). Rules are stored as free-form source-form recipe strings at stage-0.

**Enums:**

**`DiffMode`** (line 9)  
Variants: `Primal`, `Fwd`, `Bwd`. The `Primal` variant represents the undifferentiated function; no rules are generated for it.

Methods:
- `fn suffix(self) -> &'static str` (line 22) — Returns `""` / `"_fwd"` / `"_bwd"` for naming generated variants.
- `const ALL: [Self; 3]` (line 31) — Canonical iteration order.

**`Primitive`** (line 35)  
19 variants covering the full AD primitive set:
- Arithmetic: `FAdd`, `FSub`, `FMul`, `FDiv`, `FNeg`
- Transcendental: `Sqrt`, `Sin`, `Cos`, `Exp`, `Log`
- Higher-level: `Call` (delegates to callee's variant), `Load`/`Store` (memref tape ops), `If`/`Loop` (control flow, piecewise/bounded)
- Piecewise-linear: `Min`, `Max`, `Abs`, `Sign` (added T11-D13)

Methods:
- `fn name(self) -> &'static str` (line 79) — Canonical source-form name (e.g., `"fadd"`, `"sqrt"`, `"min"`).
- `const ALL: [Self; 19]` (line 105) — All 19 primitives in declaration order.

**`DiffRule`** (line 133)  
Fields: `primitive: Primitive`, `mode: DiffMode`, `recipe: &'static str`. The `recipe` is a human-readable source-form string (e.g., `"dy = dx_0 + dx_1"`) documenting the differentiation formula. At stage-0 this is informational; actual op emission is handled by `substitute.rs`.

**`DiffRuleTable`** (line 143)  
Wrapper around `HashMap<(Primitive, DiffMode), DiffRule>`.

Methods:
- `fn new() -> Self` (line 150) — Empty table.
- `fn canonical() -> Self` (line 154) — Builds the 38-rule table matching `specs/05_AUTODIFF.csl`. Rules cover: FAdd/FSub/FMul/FDiv/FNeg (arithmetic), Sqrt/Sin/Cos/Exp/Log (transcendentals), Call (callee delegation), Load/Store (tape memory), If/Loop (control flow), Min/Max/Abs/Sign (piecewise-linear). No Primal-mode rules are added; the table lookup for `(any, Primal)` always returns `None`.
- `fn insert(&mut self, primitive: Primitive, mode: DiffMode, recipe: &'static str)` (line 258) — Private helper for building the table.
- `fn lookup(&self, primitive: Primitive, mode: DiffMode) -> Option<&DiffRule>` (line 271) — Primary lookup used by `substitute.rs` and `walker.rs`.
- `fn len(&self) -> usize` (line 277) — Number of rules.
- `fn is_empty(&self) -> bool` (line 283) — Empty predicate.
- `fn iter(&self) -> impl Iterator<Item = &DiffRule>` (line 288) — Iterates all rules (order undefined due to `HashMap` backing).

**Test module `tests`** (line 293): 14 tests. Covers: `DiffMode::ALL` length = 3, mode suffixes, 19 primitives count, canonical table has 38 rules, FMul fwd/bwd shapes, Sqrt fwd shape, Primal has no rules, empty table behavior, iter count == len, primitive name uniqueness; and a T11-D13 group covering Min/Max/Abs/Sign fwd and bwd rules for both modes.

---

### 3.4 `cssl-autodiff/src/transform.rs` (177 lines)

HIR-level bookkeeping: for each `@differentiable` function, records the three variant names (primal, `_fwd`, `_bwd`) in a `BTreeMap` keyed by `DefId`. This is a data-model pass; actual body expansion happens in `substitute.rs`.

**Structs:**

**`DiffVariants`** (line 19)  
Fields: `primal: Symbol`, `fwd_name: String`, `bwd_name: String`, `primal_def: DefId`, `decl: DiffDecl`. Carries all name-resolution outputs for one `@differentiable` function.

Methods:
- `fn from_decl(decl: DiffDecl, interner: &Interner) -> Self` (line 35) — Resolves the primal name string and derives `fwd_name`/`bwd_name` by appending `DiffMode::Fwd.suffix()` / `DiffMode::Bwd.suffix()`.

**`DiffTransform<'a>`** (line 51)  
Fields: `interner: &'a Interner`, `rules: DiffRuleTable`, `variants: BTreeMap<u32, DiffVariants>`. The `u32` key is `DefId.0`. Uses `BTreeMap` for deterministic iteration order.

Methods:
- `fn new(interner: &'a Interner) -> Self` (line 59) — Initializes with the canonical rules table.
- `fn register_module(&mut self, module: &HirModule)` (line 69) — Collects all `@differentiable` fns; skips those with `@NoDiff`.
- `fn get(&self, def: DefId) -> Option<&DiffVariants>` (line 83) — Lookup by `DefId`.
- `fn len(&self) -> usize` (line 89) — Registered variant count.
- `fn is_empty(&self) -> bool` (line 95) — Empty predicate.
- `fn iter(&self) -> impl Iterator<Item = &DiffVariants>` (line 100) — Deterministic `BTreeMap` iteration.

**Test module `tests`** (line 105): 5 tests. Covers: empty module registers nothing, single differentiable fn gets `_fwd`/`_bwd` names, multiple fns all registered, canonical table has 38 rules, and name-roundtrip with a hand-built `DiffDecl`.

---

### 3.5 `cssl-autodiff/src/walker.rs` (811 lines)

The MIR-module driver. Discovers which functions are `@differentiable` (either from HIR or an explicit name set), iterates primal functions in the `MirModule`, calls `apply_fwd` / `apply_bwd` from `substitute.rs`, appends the two new variants, and returns a telemetry report. Also provides the `AdWalkerPass` adapter that plugs the walker into the `cssl-mir` `PassPipeline`.

**Free functions (pub):**

- `fn op_to_primitive(op_name: &str) -> Option<Primitive>` (line 54)  
  Maps MLIR-dialect op names to `Primitive` variants. Covered mappings: `arith.addf` → FAdd, `arith.subf` → FSub, `arith.mulf` → FMul, `arith.divf` → FDiv, `arith.negf` → FNeg, `arith.minimumf`/`arith.minf` → Min, `arith.maximumf`/`arith.maxf` → Max, `math.absf`/`math.abs` → Abs, `math.copysign` → Sign, `func.call`/`cssl.call_indirect` → Call, `scf.if` → If, `scf.for`/`scf.while`/`scf.loop`/`scf.while_loop` → Loop, `memref.load` → Load, `memref.store` → Store. Returns `None` for integer arithmetic and all other ops.

- `fn specialize_transcendental(prim: Primitive, callee: Option<&str>) -> Primitive` (line 77)  
  When `prim == Primitive::Call`, inspects the `callee` attribute string to specialize into the concrete transcendental: `"sqrt"` → Sqrt, `"sin"` → Sin, `"cos"` → Cos, `"exp"` → Exp, `"log"/"ln"` → Log, `"min"/"math.min"/"fmin"` → Min, `"max"/"math.max"/"fmax"` → Max, `"abs"/"math.abs"/"fabs"` → Abs, `"sign"/"math.sign"/"signum"` → Sign, anything else stays Call. For non-Call primitives, returns the input unchanged.

**Structs:**

**`AdWalkerReport`** (line 97)  
Telemetry aggregation. Fields: `fns_transformed: u32`, `variants_emitted: u32`, `ops_matched: u32`, `rules_applied: u32`, `unsupported_ops: u32`, `tangent_ops_emitted: u32`, `tangent_params_added: u32`. All arithmetic uses `saturating_add`.

Methods:
- `fn summary(&self) -> String` (line 117) — One-line diagnostic string.
- `fn accumulate(&mut self, sub: &SubstitutionReport)` (line 132) — Private. Folds a per-variant `SubstitutionReport` into this module-level summary.

**`AdWalker`** (line 154)  
Fields: `rules: DiffRuleTable`, `diff_fn_names: HashSet<String>`.

Methods:
- `fn from_hir(hir_module: &HirModule, interner: &Interner) -> Self` (line 162) — Discovers all `@differentiable` (non-`@NoDiff`) function names from the HIR module.
- `fn with_names(names: impl IntoIterator<Item = String>) -> Self` (line 178) — Explicit name-set constructor for tests and manual wiring.
- `fn transform_module(&self, module: &mut MirModule) -> AdWalkerReport` (line 191) — Main driver. Collects indices of matching primal functions first (to avoid iterating over freshly appended variants), calls `apply_fwd`/`apply_bwd` for each, appends results, accumulates telemetry.

**`AdWalkerPass`** (line 231)  
Thin `MirPass` adapter.

Fields: `walker: AdWalker`.

Methods (on `AdWalkerPass`):
- `impl Debug` (line 235) — Custom `Debug` showing `diff_fn_count` and `rule_count`.
- `impl MirPass` (line 244):
  - `fn name(&self) -> &'static str` — Returns `"ad-walker"`.
  - `fn run(&self, module: &mut MirModule) -> PassResult` — Invokes `transform_module`, wraps the summary in a `PassDiagnostic::info("AD0100", ...)`, reports `changed = variants_emitted > 0`.

**Test module `tests`** (line 260): 15 tests, including:
- `op_to_primitive_float_arith` — all 5 float arith ops map correctly
- `op_to_primitive_ignores_integer_arith` — addi/subi/muli/divsi all → None
- `op_to_primitive_call_control_memory` — call/if/for/load/store
- `specialize_transcendental_variants` — sqrt/sin/ln/unknown/non-Call passthrough
- `specialize_transcendental_piecewise_primitives` — min/fmin/max/abs/fabs/sign/signum (T11-D13/D14)
- `op_to_primitive_maps_arith_min_max_abs` — arith.minimumf/minf/maximumf/maxf/absf/copysign
- `walker_empty_module_transforms_nothing`
- `walker_emits_fwd_and_bwd_variants` — name presence + count
- `walker_emits_real_tangent_ops_for_float_arith` — `arith.addf` with `diff_role=tangent` in fwd variant
- `walker_marks_variant_fns_with_diff_variant_attr` — fwd → `"fwd"`, bwd → `"bwd"`
- `walker_preserves_primal_function` — primal has no `diff_variant`
- `walker_skips_non_differentiable_fns`
- `report_summary_shape`
- `from_hir_discovers_differentiable_fns` — `@NoDiff` excluded, plain skipped
- `ad_walker_pass_plugs_into_pipeline` — full `PassPipeline` integration, code `AD0100`
- `ad_walker_pass_debug_shape`
- `sphere_sdf_integration_emits_real_tangent_and_adjoint_ops` — end-to-end for `p - r`
- `transcendental_callee_resolution_matches_rules`
- `scene_union_min_integration_emits_branchful_tangent_and_adjoint` — T11-D16 scene-SDF min chain (checks no `fwd_placeholder`)
- `nested_min_emits_two_branchful_tangents` — nested `min(min(a,b),c)`
- `abs_integration_emits_branchful_tangent` — `abs(a-b)` chain
- `max_integration_emits_branchful_tangent` — `max(a,b)` with `predicate=oge`
- `union_intersect_subtract_chain_emits_three_primitives` — `max(max(a,b),c)`

---

### 3.6 `cssl-autodiff/src/substitute.rs` (2,427 lines)

The core substitution engine — the largest file in the slice. Implements the actual MIR-op emission for both forward and reverse differentiation modes across all 14 fully-supported primitives (FAdd, FSub, FMul, FDiv, FNeg, Sqrt, Sin, Cos, Exp, Log, Min, Max, Abs, Sign). Control-flow and memory primitives (Call, Load, Store, If, Loop) emit structural placeholders.

**Structs:**

**`TangentMap`** (line 49)  
Wraps `HashMap<ValueId, ValueId>`. Maps a primal SSA value to its tangent (in forward mode) or adjoint (in reverse mode). In both modes the data structure is identical; semantics differ.

Methods:
- `fn new() -> Self` (line 55) — Empty.
- `fn insert(&mut self, primal: ValueId, derivative: ValueId)` (line 61) — Upserts.
- `fn get(&self, primal: ValueId) -> Option<ValueId>` (line 67) — Lookup, returns a copy.
- `fn len(&self) -> usize` (line 73) — Entry count.
- `fn is_empty(&self) -> bool` (line 79) — Empty predicate.

**`SubstitutionReport`** (line 86)  
Per-invocation telemetry. Fields: `primitives_substituted: u32`, `tangent_ops_emitted: u32`, `unsupported_primitives: u32`, `tangent_params_added: u32`, `tangent_results_added: u32`.

Methods:
- `fn summary(&self) -> String` (line 102) — One-line diagnostic.

**Public entry points:**

- `fn apply_fwd(primal: &MirFunc, rules: &DiffRuleTable) -> (MirFunc, TangentMap, SubstitutionReport)` (line 129)  
  Builds the forward-mode variant. Delegates to `apply_mode` with `DiffMode::Fwd`. The returned `TangentMap` reflects the final state after the walk (useful for downstream inspection and tests).

- `fn apply_bwd(primal: &MirFunc, rules: &DiffRuleTable) -> (MirFunc, TangentMap, SubstitutionReport)` (line 147)  
  Builds the reverse-mode variant. Delegates to `apply_mode` with `DiffMode::Bwd`.

**Private internal driver:**

- `fn apply_mode(primal: &MirFunc, rules: &DiffRuleTable, mode: DiffMode) -> (MirFunc, TangentMap, SubstitutionReport)` (line 158)  
  Shared scaffolding. Clones the primal, renames the variant, attaches `diff_variant` and `diff_primal_name` attributes, calls `reconcile_next_value_id`, then dispatches to `synthesize_tangent_params` → `substitute_fwd`/`substitute_bwd` → `synthesize_tangent_results`.

**SSA bookkeeping:**

- `fn reconcile_next_value_id(variant: &mut MirFunc)` (line 207)  
  Scans all operand and result IDs across the entire function body (recursing into nested regions) and sets `variant.next_value_id` to `max(seen) + 1`. Necessary because test helpers construct `MirFunc` values without advancing the counter.

- `fn scan_region(region: &MirRegion, max_id: &mut u32)` (line 219)  
  Recursive helper for `reconcile_next_value_id`. Visits all blocks, their args, op operands, op results, and nested regions.

**Signature synthesis:**

- `fn synthesize_tangent_params(variant: &mut MirFunc, mode: DiffMode, tangent_map: &mut TangentMap, report: &mut SubstitutionReport)` (line 247)  
  Fwd mode: interleaves tangent params immediately after each float primal param, recording the mapping in `tangent_map`. Non-float params pass through unchanged. Bwd mode: retains primal params, then appends one adjoint-in param per primal float result, seeded via the sentinel key `ValueId(u32::MAX)` in the tangent map.

- `fn synthesize_tangent_results(variant: &mut MirFunc, mode: DiffMode, original_param_count: usize, report: &mut SubstitutionReport)` (line 323)  
  Fwd mode: appends one tangent result type per primal float result. Bwd mode: drops primal results entirely, emits one adjoint-out result per original float param (indexed via `original_param_count` to skip the adjoint-in params appended to the signature).

**Forward-mode substitution:**

- `fn substitute_fwd(variant: &mut MirFunc, rules: &DiffRuleTable, tangent_map: &mut TangentMap, report: &mut SubstitutionReport)` (line 360)  
  Walks entry-block ops. For each op, calls `recognize_primitive`, looks up the Fwd rule, calls `emit_fwd_tangent_ops`. Handles `func.return` specially: appends the tangent of each return value as extra operands. Recurses into nested regions by calling `substitute_fwd_region`.

- `fn substitute_fwd_region(region: &mut MirRegion, rules: &DiffRuleTable, tangent_map: &mut TangentMap, report: &mut SubstitutionReport, next_id: &mut u32)` (line 416)  
  Recursively applies fwd substitution to all blocks in a nested region (used for `scf.if`/`scf.for` bodies).

- `fn emit_fwd_tangent_ops(op: &MirOp, prim: Primitive, recipe: &str, tangent_map: &mut TangentMap, next_id: &mut u32) -> Vec<MirOp>` (line 457)  
  Dispatch hub. Matches on `prim` and delegates to one of 14 specialized per-primitive emitters. For the 5 structural primitives (Call, Load, Store, If, Loop), emits a `cssl.diff.fwd_placeholder` op carrying the recipe attribute.

**Forward-mode per-primitive emitters (all private, all return `Vec<MirOp>`):**

- `fn emit_fadd_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 503)  
  `d_y = d_a + d_b`. Emits 1 `arith.addf` tagged `diff_role=tangent, diff_primitive=fadd`.

- `fn emit_fsub_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 525)  
  `d_y = d_a - d_b`. Emits 1 `arith.subf`.

- `fn emit_fmul_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 548)  
  Product rule `d_y = d_a*b + a*d_b`. Emits 3 ops: 2 `arith.mulf` + 1 `arith.addf`, all tagged `fmul`.

- `fn emit_fdiv_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 588)  
  Quotient rule `d_y = (d_a*b - a*d_b) / (b*b)`. Emits 5 ops: 2 `arith.mulf` + 1 `arith.subf` + 1 `arith.mulf` + 1 `arith.divf`, all tagged `fdiv`.

- `fn emit_fneg_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 641)  
  `d_y = -d_a`. Emits 1 `arith.negf`.

- `fn emit_sqrt_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 663)  
  `d_y = d_a / (2 * y)` where `y` is the primal result. Emits 3 ops: `arith.constant 2.0` + `arith.mulf` + `arith.divf`, all tagged `sqrt`.

- `fn emit_sin_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 700)  
  `d_y = d_a * cos(a)`. Emits 2 ops: `func.call {callee=cos}` + `arith.mulf`, tagged `sin`.

- `fn emit_cos_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 731)  
  `d_y = -d_a * sin(a)`. Emits 3 ops: `func.call {callee=sin}` + `arith.negf` + `arith.mulf`, tagged `cos`.

- `fn emit_exp_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 768)  
  `d_y = d_a * y` (reuses primal result). Emits 1 `arith.mulf`, tagged `exp`.

- `fn emit_log_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 790)  
  `d_y = d_a / a`. Emits 1 `arith.divf`, tagged `log`.

- `fn emit_min_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 812)  
  Delegates to `emit_piecewise_binary_fwd` with predicate `"ole"` and name `"min"`.

- `fn emit_max_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 830)  
  Delegates to `emit_piecewise_binary_fwd` with predicate `"oge"` and name `"max"`.

- `fn emit_piecewise_binary_fwd(op, primal_result, result_ty, tangent_map, next_id, predicate: &'static str, prim_name: &'static str) -> Vec<MirOp>` (line 851)  
  Shared min/max forward emitter. `d_y = select(cmp(a, b, predicate), d_a, d_b)`. Emits 2 ops: `arith.cmpf {predicate}` (result type `MirType::Bool`) + `arith.select`.

- `fn emit_abs_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 887)  
  `d_y = select(x >= 0, d_x, -d_x)`. Emits 4 ops: `arith.constant 0.0` + `arith.cmpf {oge}` + `arith.negf d_x` + `arith.select`, all tagged `abs`.

- `fn emit_sign_fwd(op, primal_result, result_ty, tangent_map, next_id)` (line 932)  
  Zero-gradient convention: `d_y = 0`. Emits 1 `arith.constant 0.0`, tagged `sign`.

**Reverse-mode substitution:**

- `fn substitute_bwd(variant: &mut MirFunc, rules: &DiffRuleTable, original_param_count: usize, tangent_map: &mut TangentMap, report: &mut SubstitutionReport)` (line 952)  
  Main reverse-mode driver. Steps: (1) zero-initialize the adjoint of every float primal param; (2) locate `func.return` in the primal ops and seed the adjoint map via the sentinel `ValueId(u32::MAX)` key; (3) walk primal ops in reverse, calling `emit_bwd_adjoint_ops` for each recognized primitive; (4) append a `cssl.diff.bwd_return` terminator carrying adjoint-out values for all original float params; (5) combine primal ops (with `func.return` stripped) + bwd ops into the variant body. Note at `substitute.rs:1035–1038`: the primal `func.return` is filtered out explicitly to avoid a mid-block terminator.

- `fn emit_bwd_adjoint_ops(op: &MirOp, prim: Primitive, tangent_map: &mut TangentMap, next_id: &mut u32) -> Vec<MirOp>` (line 1052)  
  Per-op adjoint dispatch. For any operand not yet in the tangent map, emits an `arith.constant 0.0` zero-init op first (intermediate value whose adjoint starts at zero). Then dispatches to the per-primitive bwd emitter. For structural primitives (Call, Load, Store, If, Loop), emits a `cssl.diff.bwd_placeholder` carrying the incoming adjoint `d_y`.

**Reverse-mode per-primitive emitters (all private, all return `Vec<MirOp>`):**

- `fn emit_bwd_additive(op, result_ty, d_y, tangent_map, next_id, sub: bool) -> Vec<MirOp>` (line 1132)  
  Shared FAdd/FSub bwd: `d_a += d_y; d_b (±=) d_y`. If `sub = true`, uses `arith.subf` for the b-update (FSub bwd); otherwise `arith.addf` (FAdd bwd). Contains a self-reference safety note: the a-update is committed to the map before the b-update reads it, so `x + x`-style aliasing accumulates correctly (2 × d_y on the same adjoint slot).

- `fn emit_bwd_multiplicative(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1170)  
  FMul bwd: `d_a += d_y * b; d_b += d_y * a`. Emits 4 ops: 2 contrib `arith.mulf` + 2 accumulate `arith.addf`. Self-reference safety note at line 1181 for the `a * a` case.

- `fn emit_bwd_div(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1221)  
  FDiv bwd: `d_a += d_y / b; d_b -= d_y * a / (b*b)`. Emits 6 ops.

- `fn emit_bwd_neg(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1283)  
  FNeg bwd: `d_a -= d_y` (as `d_a_new = prev_d_a - d_y` via `arith.subf`). Emits 1 op.

- `fn emit_bwd_sqrt(op, primal_result, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1305)  
  Sqrt bwd: `d_a += d_y / (2 * y)`. Emits 4 ops: `arith.constant 2.0` + `arith.mulf` (2*y) + `arith.divf` (contrib) + `arith.addf` (accumulate).

- `fn emit_bwd_sin(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1350)  
  Sin bwd: `d_a += d_y * cos(a)`. Emits 3 ops: `func.call {callee=cos}` + `arith.mulf` + `arith.addf`.

- `fn emit_bwd_cos(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1388)  
  Cos bwd: `d_a -= d_y * sin(a)`. Emits 3 ops: `func.call {callee=sin}` + `arith.mulf` + `arith.subf` (subtract from prev adjoint).

- `fn emit_bwd_exp(op, primal_result, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1426)  
  Exp bwd: `d_a += d_y * y`. Emits 2 ops: `arith.mulf` (reusing primal result) + `arith.addf`.

- `fn emit_bwd_min(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1458)  
  Delegates to `emit_bwd_piecewise_binary` with predicate `"ole"`, name `"min"`.

- `fn emit_bwd_max(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1468)  
  Delegates to `emit_bwd_piecewise_binary` with predicate `"oge"`, name `"max"`.

- `fn emit_bwd_piecewise_binary(op, result_ty, d_y, tangent_map, next_id, predicate, prim_name) -> Vec<MirOp>` (line 1481)  
  Shared min/max bwd: emits 6 ops — `arith.constant 0.0` + `arith.cmpf {predicate}` + `arith.select` (contrib_a: d_y if winner, 0 otherwise) + `arith.addf` (accumulate d_a) + `arith.select` (contrib_b: 0 if winner, d_y otherwise) + `arith.addf` (accumulate d_b).

- `fn emit_bwd_abs(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1547)  
  Abs bwd: `d_x += select(x >= 0, d_y, -d_y)`. Emits 5 ops: `arith.constant 0.0` + `arith.cmpf {oge}` + `arith.negf d_y` + `arith.select` + `arith.addf`.

- `fn emit_bwd_sign(op: &MirOp, tangent_map: &TangentMap, _next_id: &mut u32) -> Vec<MirOp>` (line 1600)  
  Sign bwd: no-op. Returns an empty `Vec`. The existing adjoint is preserved without modification. The operand lookup (`tangent_map.get(x)`) is evaluated but its result discarded — this reads the map without side effects.

- `fn emit_bwd_log(op, result_ty, d_y, tangent_map, next_id) -> Vec<MirOp>` (line 1611)  
  Log bwd: `d_a += d_y / a`. Emits 2 ops: `arith.divf` + `arith.addf`.

**Helper functions (all private):**

- `fn fresh_id(next_id: &mut u32) -> ValueId` (line 1646) — Allocates a fresh SSA value-id and advances the counter using `saturating_add`.

- `fn recognize_primitive(op: &MirOp) -> Option<Primitive>` (line 1653) — Classifies a `MirOp` as a recognized AD primitive. For `Call`, inspects the `callee` attribute and calls `specialize_transcendental`.

- `fn tangent_or_zero(map: &TangentMap, v: ValueId) -> ValueId` (line 1672) — Returns the tangent/adjoint from the map, or the primal `ValueId` itself as a fallback when no entry exists. The implicit-zero convention: unknown tangents are treated as zero without emitting a constant op. This relies on downstream consumers using `diff_role` attributes to distinguish primal from tangent values.

- `fn is_float(t: &MirType) -> bool` (line 1677) — Returns `true` iff `t` is `MirType::Float(_)`.

- `fn tangent_type_of(t: &MirType) -> MirType` (line 1683) — Returns the tangent type for a primal type. Float stays float; Int/Bool map to `Float(F32)`; other types clone through unchanged. This is a stage-0 simplification — jets/higher-order tangents are not represented here.

- `fn default_tangent_ty() -> MirType` (line 1692) — Returns `MirType::Float(FloatWidth::F32)` as the fallback when an op has no annotated result type.

- `fn mode_str(mode: DiffMode) -> &'static str` (line 1697) — Returns `"primal"` / `"fwd"` / `"bwd"` for use as attribute values.

**Test module `tests`** (line 1705): 25 tests. Includes:
- `tangent_map_insert_and_get`
- `report_summary_mentions_counts`
- Per-primitive fwd tests: `fwd_fadd_emits_tangent_addf`, `fwd_fsub_emits_tangent_subf`, `fwd_fmul_emits_two_muls_plus_add`, `fwd_fdiv_emits_full_chain`, `fwd_fneg_emits_tangent_negf`, `fwd_sqrt_emits_constant_mul_div_chain`, `fwd_sin_emits_cos_call_and_mul`, `fwd_exp_reuses_primal_result`
- Bwd tests: `bwd_fadd_emits_adjoint_accumulation`, `bwd_fmul_emits_contribution_and_accumulate`, `bwd_ends_with_bwd_return`
- Structural: `fwd_preserves_primal_ops`, `fwd_on_non_primitive_ops_is_identity`, `sphere_sdf_shape_fwd_and_bwd`, `tangent_params_appear_in_signature`, `apply_fwd_on_empty_body_does_not_crash`, `apply_bwd_on_empty_body_does_not_crash`, `types_roundtrip`
- T11-D15 piecewise group: `fwd_min_emits_cmpf_ole_plus_select`, `fwd_max_emits_cmpf_oge_plus_select`, `fwd_abs_emits_constant_cmpf_negf_select`, `fwd_sign_emits_constant_zero`, `bwd_min_emits_select_plus_accumulate`, `bwd_abs_emits_select_plus_accumulate`, `bwd_sign_is_noop`, `min_and_max_no_longer_emit_fwd_placeholder`

Helper function `mk_primal` (line 1714) builds a test `MirFunc` from typed signature and op list. `f32_ty()` (line 1727) is a convenience alias.

---

### 3.7 `cssl-jets/src/lib.rs` (293 lines)

The entire `cssl-jets` crate lives in this one file. Defines the abstract type system for `Jet<T,N>` higher-order AD. No concrete Rust struct for `Jet<T,N>` exists here; the crate is entirely a schema layer used by downstream staging.

**Types:**

**`JetOrder(pub u32)`** (line 39)  
Newtype wrapping the Taylor-series order N. Implements `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Ord`, `PartialOrd`.

Methods:
- `const FIRST: Self` (line 43) — `JetOrder(1)` — primal + first derivative.
- `const SECOND: Self` (line 45) — `JetOrder(2)` — primal + first + second derivative.
- `fn coefficient_count(self) -> u32` (line 48) — Returns `N + 1` via `saturating_add`. A `JetOrder(N)` carries N+1 coefficients (the primal is coefficient 0, derivatives are 1..N).

**`JetOp`** (line 54)  
Enum of 5 recognized Jet operations:
- `Construct` — Build a Jet from N+1 scalar coefficients.
- `Project` — Extract the k-th coefficient (k=0 is the primal value).
- `Add` — Elementwise addition of two Jets of the same order.
- `Mul` — Leibniz product rule up to order N.
- `Apply` — Apply a scalar function via Taylor-series expansion.

Methods:
- `fn name(self) -> &'static str` (line 71) — Returns `"cssl.jet.*"` dialect-prefixed names: `"cssl.jet.construct"`, `"cssl.jet.project"`, `"cssl.jet.add"`, `"cssl.jet.mul"`, `"cssl.jet.apply"`.
- `const ALL: [Self; 5]` (line 82) — All 5 ops in declaration order.
- `fn signature(self) -> JetSignature` (line 107) — Returns the `JetSignature` for each op. Note: `Construct` marks `scalar_operands = 0` with an inline comment that the caller is responsible for enforcing the `1+N` arity by order — the variadic nature is not representable in a fixed signature struct. This is a known design gap.

**`JetSignature`** (line 92)  
Fields: `jet_operands: u32`, `scalar_operands: u32`, `jet_results: u32`, `order_dependent: bool`. Describes the static arity of a `JetOp`. `order_dependent = true` means the exact count varies with the concrete `JetOrder` value.

**`JetError`** (line 149)  
Three error variants via `thiserror::Error`:
- `ProjectOutOfBounds { index: u32, count: u32, order: u32 }` — `project` index ≥ coefficient count.
- `OrderMismatch { lhs: u32, rhs: u32 }` — Binary op receives Jets of different orders.
- `ArityMismatch { expected: u32, actual: u32 }` — `construct` called with wrong number of coefficients.

**Free validation functions (pub):**

- `fn validate_construct(order: JetOrder, coefficient_count: u32) -> Result<(), JetError>` (line 163) — Checks that `coefficient_count == order.coefficient_count()`.
- `fn validate_project(order: JetOrder, index: u32) -> Result<(), JetError>` (line 175) — Checks that `index < order.coefficient_count()`.
- `fn validate_binary_order(lhs: JetOrder, rhs: JetOrder) -> Result<(), JetError>` (line 188) — Checks that both operand orders match.

**`STAGE0_SCAFFOLD: &str`** (line 199) — Crate version constant.

**Test module `tests`** (line 201): 14 tests. Covers: scaffold non-empty, first-order has 2 coefficients, second-order has 3, all 5 ops, op names start with `"cssl.jet."`, Project signature shape, Add signature shape, construct arity passes/fails, project in-range/out-of-range, binary order match/mismatch, total ordering.

---

## 4. SLICE NOTES

### 4.1 Test Coverage

Test coverage is strong for the implemented scope. The crate has no separate `tests/` directory — all tests are inline `#[cfg(test)]` modules. Key highlights:

- `decl.rs`: 4 tests — basic collection, empty, non-annotated, multiple.
- `rules.rs`: 14 tests — full canonical table shape, per-rule spot checks, piecewise group.
- `transform.rs`: 5 tests — name derivation, registration, BTreeMap ordering (implicit via len).
- `walker.rs`: 22 tests — op mapping, transcendental specialization, pipeline integration, piecewise scene-SDF end-to-end chains.
- `substitute.rs`: 25 tests — per-primitive fwd + bwd, structural preservation, empty body, piecewise T11-D15 group.
- `cssl-jets/lib.rs`: 14 tests — type schema, validation paths.

No integration test suite (`tests/` directory) exists for either crate. All tests are unit-style, driving either the full lex→parse→lower→HIR→MIR pipeline or hand-constructed `MirFunc` fixtures.

### 4.2 Incomplete / Stubbed Items

The following items are explicitly deferred by inline comments or module-level documentation:

**`decl.rs:42`** — Lipschitz bound extraction placeholder:
```rust
.map(|_| "k".to_string()); // Stage-0 placeholder ; full arg-extraction @ T7-phase-2.
```
The `lipschitz_bound` field in `DiffDecl` is always `Some("k")` if the `@lipschitz` attribute is present, regardless of the actual bound value. The numeric `k` is never parsed.

**`lib.rs:17–22`** and **`walker.rs:25–33`** and **`substitute.rs:29–35`** — T7-phase-2c deferred work (all three files document the same gaps):
- **Tape-buffer allocation** for control-flow ops (`scf.if`, `scf.for`, `scf.while`): The reverse-mode bwd walk encounters these as `Loop`/`If` primitives but only emits a `cssl.diff.bwd_placeholder` op. No tape record/replay mechanism exists.
- **`@checkpoint` selective recomputation**: The `checkpoint` field is collected in `DiffDecl` but has no downstream effect — it is never read by `substitute.rs` or `walker.rs`.
- **GPU-AD tape-location resolution**: No device/shared/unified memory annotation mechanism.
- **Multi-result tangent-tuple emission**: The fwd/bwd signature synthesis assumes single-result primal functions in several places. `synthesize_tangent_results` loops over `original_results` correctly for fwd, but the bwd mode assumes at most one float result when seeding the adjoint map (it inserts into `tangent_map` using the sentinel `ValueId(u32::MAX)` for all float results — if there were multiple float results, only the last one's seed would be active).
- **Killer-app gate verification**: `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic — deferred to T9-phase-2 SMT integration.
- **Higher-order AD via `Jet<T,N>`**: Listed as deferred in `walker.rs:33`.

**Structural primitives** (Call, Load, Store, If, Loop) in both fwd and bwd mode emit placeholder ops (`cssl.diff.fwd_placeholder` / `cssl.diff.bwd_placeholder`) rather than real tangent/adjoint ops. This is correct for stage-0 but means any `@differentiable` function containing a `func.call` to a non-transcendental callee, a memory access, or control flow will produce a variant body with placeholders rather than correct derivatives.

**`cssl-jets`** — The `Jet<T,N>` type has no concrete Rust representation (no struct layout, no actual field definitions). The crate is entirely abstract schema at stage-0. The `JetOp::Apply` operation has no implementation specification beyond the signature. The `Construct` op marks `scalar_operands = 0` with a comment acknowledging the variadic gap.

### 4.3 Design Observations and Surprises

**`tangent_or_zero` implicit-zero convention (substitute.rs:1672):** When a primal value has no tangent entry in the map, the primal value-id itself is returned as a fallback. This is documented as "zero ish — stage-0 relies on diff_role attributes to disambiguate." It is a pragmatic shortcut that works for straight-line code with complete coverage, but it could produce silently incorrect derivatives if a primitive's operand's tangent was not registered (e.g., because it came from a non-float operation that was incorrectly classified). The attributes on emitted ops do provide a disambiguation path for consumers, but an opaque consumer that doesn't inspect attributes would see the primal value used where a zero should appear.

**`emit_bwd_sign` reads the map without effect (substitute.rs:1606):** The no-op reverse emitter calls `tangent_map.get(x)` and discards the result (`let _ = tangent_map.get(x)`). This is presumably for documentation purposes (to show that the adjoint would be read here if a contribution existed), but it adds no runtime value and could be removed.

**Sentinel `ValueId(u32::MAX)` for bwd adjoint seeding:** The mechanism of using `ValueId(u32::MAX)` as a map key to transfer the seeded `d_y` from parameter synthesis to the `func.return` location in the body is clever but fragile. If `next_value_id` ever reaches `u32::MAX` and a fresh SSA id is allocated, there will be a collision. In practice this cannot happen with plausible MIR functions, but it is an unguarded invariant.

**`math.copysign` → `Sign` mapping (walker.rs:64):** MLIR's `math.copysign` computes `copysign(a, b)` — it copies the sign of `b` into `a`, returning a value with `a`'s magnitude and `b`'s sign. This is semantically distinct from `sign(x)` (which returns -1/0/+1). Mapping `math.copysign` to `Primitive::Sign` is therefore incorrect if the intent is to AD-differentiate the copysign operation; the derivative of `copysign(a, b)` w.r.t. `a` is `sign(b)` and w.r.t. `b` is zero. The current mapping assigns zero gradient to both, which is the subgradient of `sign`, not the derivative of `copysign`. This is a spec divergence.

**No `@checkpoint` downstream effect:** The `checkpoint` field is populated in `DiffDecl` and threaded through `DiffVariants`, but neither `AdWalker`, `substitute.rs`, nor `walker.rs` reads it. When T7-phase-2c implements tape-buffer allocation, this field would need to be consulted to selectively recompute rather than store.

**BTreeMap in `DiffTransform`:** `DiffTransform.variants` uses `BTreeMap<u32, DiffVariants>` for deterministic iteration. This is a good practice for test stability, but the key is `DefId.0` (a raw `u32`). There is no guarantee that `DefId` values are assigned in declaration order, so the iteration order reflects `DefId` numeric order, not declaration order. This is not a bug but may surprise consumers expecting declaration-order iteration.

**`mode_str` is `const fn` (substitute.rs:1697):** The function uses `match` on `DiffMode` and returns a `&'static str`. It is correctly marked `const fn`. This is consistent with `DiffMode::suffix`.

### 4.4 Spec Divergences

1. **`math.copysign` → `Sign`** (noted above): The spec (`specs/05_AUTODIFF.csl`) defines `sign(a) ∈ {-1, 0, +1}` as the primitive with zero gradient everywhere except 0. MLIR's `math.copysign(a, b)` is a different operation. The mapping in `op_to_primitive` at `walker.rs:64` applies the wrong semantic.

2. **Multi-result tangent-tuple emission deferred:** `specs/05_AUTODIFF.csl` likely anticipates multi-result functions (functions that return tuples). Stage-0 only handles single-result primals correctly in bwd mode (single-adjoint-in seeding). This is documented as deferred but is a functional gap relative to the spec.

3. **`@checkpoint` recognized but not actionable:** The spec references `@checkpoint` as a memory/recompute trade-off attribute. Stage-0 collects it but ignores it entirely in the AD transform passes.

4. **Loop/If bwd uses placeholder, not tape replay:** The spec describes reverse-mode through control flow as requiring tape-record/replay. Stage-0 emits a `cssl.diff.bwd_placeholder` instead, which is not a correct VJP for loops or branches.

5. **Lipschitz bound not extracted:** The spec's `@lipschitz(k = N)` attribute is intended to carry a concrete numeric Lipschitz constant. Stage-0 always stores `"k"` rather than parsing the actual value.

### 4.5 Dead Code

No `#[allow(dead_code)]` attributes appear in the slice. The `emit_bwd_sign` map read (`let _ = tangent_map.get(x)`) at `substitute.rs:1606` is a functional no-op and could be removed without behavioral change. The `_op` parameter in `emit_sign_fwd` (line 932) is prefixed with underscore correctly, indicating the intentional unused-parameter pattern.

### 4.6 Cross-Crate Dependencies Summary

- `cssl-autodiff` depends on `cssl-ast`, `cssl-hir`, `cssl-mir` (production), and `cssl-lex`, `cssl-parse` (dev/tests only).
- `cssl-jets` depends only on `thiserror`. It is self-contained and has no dependency on either MIR or HIR.
- Neither crate depends on LLVM, any external AD library, or any crate outside the workspace.
- The `cssl-staging` crate (T8) is documented as the intended consumer of `cssl-jets` but is not in this audit slice.

---

## Summary Statistics

| Crate | Files | Total LOC | Pub fns | Priv fns | Structs/Enums | Tests |
|---|---|---|---|---|---|---|
| `cssl-autodiff` | 6 | 4,061 | 24 | 32 | 8 | 80 |
| `cssl-jets` | 1 | 293 | 3 | 0 | 4 | 14 |
| **Total** | **7** | **4,354** | **27** | **32** | **12** | **94** |

*(Cargo.toml files counted in audit but not in LOC totals)*
