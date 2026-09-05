# Type-System Support Crates — Full Audit

**Audit date:** 2026-05-14  
**Auditor:** Claude Sonnet 4.6 (automated)  
**Slice:** `cssl-caps` · `cssl-effects` · `cssl-ifc`  
**Spec authority:** `specs/12_CAPABILITIES.csl` · `specs/04_EFFECTS.csl` · `specs/11_IFC.csl` · `PRIME_DIRECTIVE.md`

---

## 1. Slice Overview

These three crates form the type-system enforcement layer of the CSSLv3 stage-0 compiler. They sit between the front-end HIR elaboration (in `cssl-hir`) and the later MIR/codegen stages. No LLVM, no external constraint solvers — all checks are pure Rust, designed to be embedded in the `cssl-hir` elaboration pass.

**cssl-caps** implements the Pony-6 capability algebra. It defines the six reference capabilities (`iso / trn / ref / val / box / tag`), the deny-matrix encoding which aliasing and mutation rights each capability grants, the subtyping lattice over those capabilities (which coercions are permitted at each call-site), and the linear-use tracker that enforces iso's must-consume-or-drop discipline at block scope. It also supplies the Vale-style generational reference layout (`GenRef` — a packed `u64` holding a `u40` object index and a `u24` generation counter), which is how `ref<T>` capability references are lowered to machine words. These primitives are consumed by `cssl-hir` during elaboration.

**cssl-effects** implements the Koka-style row-polymorphic effect system. It enumerates every built-in effect in a dense `BuiltinEffect` enum, attaches metadata (name, category, argument-shape, discharge-timing) to each via a compile-time `BUILTIN_METADATA` table, provides an `EffectRegistry` for name- and variant-keyed lookup, a `sub_effect_check` function that validates caller-row ⊇ callee-row containment, and a `banned_composition` subsystem that structurally encodes the PRIME DIRECTIVE's F5 prohibitions (coercion, surveillance, weapon) as type-level compile errors.

**cssl-ifc** is a near-empty placeholder crate. Its `lib.rs` is 24 lines: a crate-level doc comment, the `STAGE0_SCAFFOLD` version constant, and a single scaffold test verifying the version is non-empty. The actual information-flow control implementation — the Jif-DLM `IfcLabel` lattice, the `IfcDiagnostic` enum with stable codes IFC0001–IFC0004, the structural `check_ifc` walker, the T11-D36 dataflow `check_ifc_flow` walker, the `IfcLabelRegistry`, and `builtin_principals` — all live in `compiler-rs/crates/cssl-hir/src/ifc.rs` (1,168 lines). The `cssl-ifc` crate has not yet been factored out to hold that logic.

---

## 2. Crate Summaries

### 2.1 `cssl-caps`

| Property | Value |
|---|---|
| Path | `compiler-rs/crates/cssl-caps/` |
| Description (Cargo.toml) | "CSSLv3 stage0 — Pony-6 capability checker + Vale gen-refs" |
| Dependencies | `thiserror` (workspace) |
| Total source LOC | 1,217 (6 files) |
| Tests/ directory | None (all tests are inline `#[cfg(test)]` modules) |
| Spec reference | `specs/12_CAPABILITIES.csl` |

**Source files:**

| File | LOC |
|---|---|
| `src/lib.rs` | 58 |
| `src/cap.rs` | 293 |
| `src/linearity.rs` | 317 |
| `src/matrix.rs` | 224 |
| `src/subtype.rs` | 184 |
| `src/genref.rs` | 141 |

**Pipeline role:** consumed by `cssl-hir` during type elaboration — the HIR elaborator uses `CapKind` as a type-annotation, `LinearTracker` to enforce iso-linearity at block boundaries, `is_subtype`/`coerce` to validate call-site capability coercions, and `GenRef` as the representation type for `ref<T>` lowering to MIR.

---

### 2.2 `cssl-effects`

| Property | Value |
|---|---|
| Path | `compiler-rs/crates/cssl-effects/` |
| Description (Cargo.toml) | "CSSLv3 stage0 — Koka row-polymorphic effects + Xie-Leijen evidence" |
| Dependencies | `thiserror` (workspace) |
| Total source LOC | 1,115 (4 files) |
| Tests/ directory | None (all tests are inline `#[cfg(test)]` modules) |
| Spec reference | `specs/04_EFFECTS.csl` + `specs/11_IFC.csl` + `PRIME_DIRECTIVE.md` |

**Source files:**

| File | LOC |
|---|---|
| `src/lib.rs` | 60 |
| `src/registry.rs` | 528 |
| `src/banned.rs` | 308 |
| `src/discipline.rs` | 219 |

**Pipeline role:** consumed by `cssl-hir` during effect-row elaboration to validate that declared effect rows are well-formed, that callee effects are covered by the caller's row, and that no Prime-Directive-banned compositions appear in the source.

---

### 2.3 `cssl-ifc`

| Property | Value |
|---|---|
| Path | `compiler-rs/crates/cssl-ifc/` |
| Description (Cargo.toml) | "CSSLv3 stage0 — Jif-DLM label lattice + declassification + PRIME-DIRECTIVE encoding" |
| Dependencies | None (only workspace-level lints) |
| Total source LOC | 24 (1 file) |
| Tests/ directory | None |
| Actual IFC implementation | `compiler-rs/crates/cssl-hir/src/ifc.rs` (1,168 lines) |
| Spec reference | `specs/11_IFC.csl` |

**Pipeline role:** placeholder — no public API beyond `STAGE0_SCAFFOLD`. All IFC logic resides in `cssl-hir`.

---

## 3. Per-File Audit

### 3.1 `cssl-caps/src/lib.rs` (58 lines)

Crate root. Sets crate-level lint gates and re-exports all public items from the four sub-modules. Declares the `STAGE0_SCAFFOLD` version constant. Contains a single scaffold test confirming the version is non-empty.

**Items:**

- `pub const STAGE0_SCAFFOLD: &str` (line 48) — exposes `CARGO_PKG_VERSION`; used by external verification tooling.
- `mod scaffold_tests` (lines 51–58) — inline test module; one test `scaffold_version_present` asserts `!STAGE0_SCAFFOLD.is_empty()`.

**Lint gates:**
- `#![forbid(unsafe_code)]` — hard block on unsafe.
- `#![deny(rustdoc::broken_intra_doc_links)]` / `private_intra_doc_links`.
- `#![allow(clippy::match_same_arms)]` — justified: spec tables benefit from explicit arms.
- `#![allow(clippy::struct_excessive_bools)]` — justified: `AliasRights` carries exactly 4 bools per spec.
- `#![allow(clippy::should_implement_trait)]` — justified: `CapKind::from_str` is intentionally inherent.

**Cross-references:** re-exports from `cap`, `genref`, `linearity`, `matrix`, `subtype`.

---

### 3.2 `cssl-caps/src/cap.rs` (293 lines)

Defines the six capability variants and a small bit-packed set type. This is the foundational type on which all other cssl-caps code is built.

**Structs / Enums:**

- **`enum CapKind`** (lines 12–27) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]`. Six variants in canonical order: `Iso`, `Trn`, `Ref`, `Val`, `Box`, `Tag`. The declaration order is the matrix row order (Iso=0 … Tag=5). Doc comments on each variant describe the aliasing and mutation policy.

- **`struct CapSet(u8)`** (line 122) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]`. Bit-packed 6-element set over the capability universe. Bit `i` corresponds to `CapKind::ALL[i]`.

**Functions / Methods on `CapKind`:**

- `pub const ALL: [Self; 6]` (line 31) — `const` array of all six caps in canonical index order.
- `pub const fn as_str(self) -> &'static str` (line 42) — returns the lowercase source-form keyword ("iso", "trn", etc.).
- `pub fn from_str(s: &str) -> Option<Self>` (line 55) — parses a source-form string; returns `None` for unknown input. Note: intentionally `fn` not `impl FromStr` (lint-allowed at crate level).
- `pub const fn index(self) -> usize` (line 69) — returns 0–5 dense index matching `ALL` order; used as row/column index into `AliasMatrix`.
- `pub const fn is_linear(self) -> bool` (line 83) — `true` only for `Iso`. Per spec §12, only iso has must-consume semantics.
- `pub const fn is_mutable(self) -> bool` (line 90) — `true` for `Iso | Trn | Ref`.
- `pub const fn is_send_safe(self) -> bool` (line 97) — `true` for `Iso | Val | Box | Tag`. Iso is send-safe because it denies aliasing (unique ownership); Val/Box/Tag because they are read-only or opaque.
- `pub const fn requires_gen_check(self) -> bool` (line 103) — `true` only for `Ref`. Tags that `ref<T>` requires a runtime Vale generational deref-check.
- `pub const fn can_read(self) -> bool` (line 109) — `false` only for `Tag` (opaque handle denies data access).

**`impl fmt::Display for CapKind`** (lines 114–118) — delegates to `as_str`.

**Functions / Methods on `CapSet`:**

- `pub const fn empty() -> Self` (line 127) — returns `Self(0)`.
- `pub const fn single(c: CapKind) -> Self` (line 133) — single-element set via bit-shift.
- `pub const fn full() -> Self` (line 139) — all six bits set: `0b0011_1111`.
- `pub const fn contains(self, c: CapKind) -> bool` (line 145) — membership test via bit mask.
- `pub const fn with(self, c: CapKind) -> Self` (line 151) — insert, returns new set.
- `pub const fn union(self, other: Self) -> Self` (line 157) — bitwise OR.
- `pub const fn intersection(self, other: Self) -> Self` (line 163) — bitwise AND.
- `pub const fn is_empty(self) -> bool` (line 169) — tests `self.0 == 0`.

**Test module `tests`** (lines 175–293): 10 tests covering all capability predicate methods and all CapSet operations. Specific highlights:
- `all_six_caps_present` — verifies `ALL.len() == 6`.
- `cap_roundtrip_through_str` — round-trips every cap through `as_str`/`from_str`.
- `only_iso_is_linear` — verifies linearity predicate exhaustively.
- `mutation_set` / `send_safe_set` / `only_ref_requires_gen_check` / `tag_cannot_read` — verify each predicate exhaustively.
- `cap_set_operations` / `cap_set_union_intersection` — verify set algebra.

**Notable algorithms:** No complex algorithms; this is a pure data/predicate file. All methods are `const` where possible.

**No TODOs / FIXMEs / stubs.**

---

### 3.3 `cssl-caps/src/linearity.rs` (317 lines)

Implements per-scope linear-use tracking for `iso` values. This is the enforcement arm for the "linear × handler discipline" described in `specs/12` § LINEAR × HANDLER R8.

**Structs / Enums:**

- **`struct BindingId(pub u32)`** (line 27) — opaque newtype over `u32`. `cssl-hir` maps its `HirId` or `Symbol` to this type before calling the tracker.

- **`enum UseKind`** (lines 31–42) — `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. Five variants:
  - `Consume` — value moved into a call, returned, or re-bound.
  - `Drop` — explicit `drop(x)`.
  - `Read` — read without consume (error for iso).
  - `ResumeOnce` — passed through a handler with one-shot resume (counted as consume).
  - `ResumeMultiShot` — passed through a multi-shot resume handler (always an error for iso).

- **`struct LinearUse`** (lines 45–57) — `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. Bookkeeping record per tracked binding. Fields:
  - `pub cap: CapKind` — capability of this binding; only `iso` triggers checks.
  - `pub consume_count: u32` — number of consume-or-ResumeOnce events seen.
  - `pub read_count: u32` — number of read events (should stay 0 for iso).
  - `pub dropped: bool` — whether explicit drop was issued.
  - `pub in_scope: bool` — becomes `false` after `close_scope`.

- **`enum LinearViolation`** (lines 92–109) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]`. Five variants, each wrapping a `BindingId`:
  - `DuplicateConsume(BindingId)` — iso consumed more than once.
  - `Leak(BindingId)` — scope closed without consume-or-drop.
  - `MultiShotResume(BindingId)` — iso passed through multi-shot resume.
  - `ReadWithoutConsume(BindingId)` — iso read without consume.
  - `UseAfterScope(BindingId)` — use after scope-exit.

- **`struct LinearTracker`** (lines 112–115) — `#[derive(Debug, Default, Clone)]`. Core tracker. One field: `bindings: BTreeMap<BindingId, LinearUse>`. The BTreeMap choice ensures deterministic ordering for violation reports.

**Methods on `LinearUse`:**

- `pub const fn new(cap: CapKind) -> Self` (line 62) — constructs a fresh record with all counters zero and `in_scope: true`.
- `pub const fn is_resolved(&self) -> bool` (line 74) — `consume_count == 1 || dropped`.
- `pub const fn is_leak(&self) -> bool` (line 80) — `!in_scope && consume_count == 0 && !dropped`.
- `pub const fn is_duplicate(&self) -> bool` (line 86) — `consume_count > 1`.

**Methods on `LinearTracker`:**

- `pub fn new() -> Self` (line 120) — delegates to `Default`.
- `pub fn introduce(&mut self, id: BindingId, cap: CapKind)` (line 125) — begins tracking a new binding. Overwrites any prior binding with the same id (re-binding).
- `pub fn use_binding(&mut self, id: BindingId, kind: UseKind) -> Result<(), LinearViolation>` (line 131) — records a use event. For non-linear caps (not iso), this is a complete no-op returning `Ok(())`. For iso: checks `in_scope`, dispatches on `UseKind`. On `Consume | ResumeOnce`: increments `consume_count`, returns `DuplicateConsume` if count exceeds 1. On `Drop`: sets `dropped`, returns `DuplicateConsume` if already dropped. On `Read`: increments `read_count`, always returns `ReadWithoutConsume`. On `ResumeMultiShot`: always returns `MultiShotResume`.
- `pub fn close_scope(&mut self) -> Vec<LinearViolation>` (line 169) — marks all bindings `in_scope = false`, emits `Leak` for every linear binding that is not resolved. Returns violations in BTreeMap insertion order.
- `pub fn get(&self, id: BindingId) -> Option<&LinearUse>` (line 182) — read-only access to a binding's current record.
- `pub fn len(&self) -> usize` (line 188) — number of tracked bindings.
- `pub fn is_empty(&self) -> bool` (line 194) — delegates to `bindings.is_empty()`.

**Test module `tests`** (lines 199–317): 11 tests covering the full violation space:
- `iso_consumed_once_resolves` — happy path.
- `iso_leaked_detected` — leak detection.
- `iso_duplicate_consume_detected` — DuplicateConsume.
- `iso_dropped_resolves` — explicit drop resolves linearity.
- `non_linear_cap_unrestricted` — 5 reads on Val binding, no violations.
- `multi_shot_resume_blocked_for_iso` — MultiShotResume error.
- `resume_once_counts_as_consume` — ResumeOnce is sufficient resolution.
- `iso_read_without_consume_flagged` — ReadWithoutConsume error.
- `use_after_scope_is_error` — UseAfterScope error.
- `multi_binding_tracking` — 3 bindings (2 iso, 1 val); one consumed, one leaked, val ignored.
- `get_returns_current_record` — accessor test.

**Notable patterns:** `saturating_add` on `consume_count` to avoid overflow on pathological inputs; `BTreeMap` for deterministic ordering. Non-linear caps return early from `use_binding` without touching any counter — the tracker is zero-cost for non-iso caps.

**No TODOs / FIXMEs / stubs.**

---

### 3.4 `cssl-caps/src/matrix.rs` (224 lines)

The Pony-6 alias+deny matrix. Encodes the four aliasing/mutation rights for each capability as a compile-time table, and exposes a `can_pass_through` predicate that delegates to the subtype relation.

**Structs / Type aliases:**

- **`struct AliasRights`** (lines 21–26) — `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. Four `bool` fields:
  - `alias_local: bool` — can alias within the local scope.
  - `alias_global: bool` — can alias across thread/actor boundaries.
  - `mut_local: bool` — can mutate locally.
  - `mut_global: bool` — can mutate globally.
  
  Matrix values per spec:
  | Cap | alias_local | alias_global | mut_local | mut_global |
  |---|---|---|---|---|
  | iso | false | false | true | true |
  | trn | true | false | true | false |
  | ref | true | true | true | true |
  | val | true | true | false | false |
  | box | true | true | false | false |
  | tag | true | true | false | false |

- **`type AliasRow = AliasRights`** (line 86) — type alias for documentation clarity; `AliasRow` is the type of one matrix row.

- **`struct AliasMatrix`** (lines 91–93) — `#[derive(Debug, Clone, Copy)]`. One field: `rights: [AliasRights; 6]` indexed by `CapKind::index()`.

**Methods on `AliasRights`:**

- `pub const fn for_cap(c: CapKind) -> Self` (line 31) — returns the canonical rights for the given cap. The match arms directly encode the spec table.
- `pub const fn can_alias(self) -> bool` (line 74) — `alias_local || alias_global`.
- `pub const fn can_mutate(self) -> bool` (line 80) — `mut_local || mut_global`.

**Methods on `AliasMatrix`:**

- `pub const fn pony6() -> Self` (line 98) — builds the canonical 6-entry matrix by calling `AliasRights::for_cap` for each capability.
- `pub const fn get(&self, c: CapKind) -> AliasRights` (line 113) — lookup by cap.
- `pub fn can_pass_through(&self, caller: CapKind, callee_param: CapKind) -> bool` (line 121) — delegates to `crate::subtype::is_subtype`. This is the primary call-site checking predicate.
- `pub fn iter(&self) -> impl Iterator<Item = (CapKind, AliasRights)> + '_` (line 126) — iterates over all 6 `(CapKind, AliasRights)` pairs.

**`impl Default for AliasMatrix`** (lines 132–135) — returns `Self::pony6()`.

**Test module `tests`** (lines 138–224): 6 tests:
- `pony6_matches_spec` — verifies every field of every cap against the spec table.
- `passing_val_to_val_allowed` — reflexive pass.
- `passing_iso_to_iso_allowed_linear` — linear iso can be passed to iso-parameter (consumes).
- `passing_val_to_iso_blocked` — val is aliasable, can't promise exclusivity for iso-param.
- `passing_iso_to_val_allowed_via_freeze` — iso can freeze to val.
- `iter_returns_six_rows` — sanity-check iteration count.
- `alias_rights_predicates` — tests `can_alias` and `can_mutate`.

**Notable patterns:** All matrix data is `const`-computable. The `can_pass_through` method provides a single entry-point for call-site checking, hiding the subtype delegation.

**Note:** `val` and `box` and `tag` have identical `AliasRights` (all read-only, fully aliasable). The matrix deliberately does not distinguish them at the rights level — the differences between those three (read semantics, handle-vs-immutable semantics) are encoded in other predicates (`can_read`, `requires_gen_check`) on `CapKind`.

**No TODOs / FIXMEs / stubs.**

---

### 3.5 `cssl-caps/src/subtype.rs` (184 lines)

The capability subtyping lattice. Implements `from <: to` as a total function that returns a typed witness or a typed error.

**Structs / Enums:**

- **`enum Subtype`** (lines 21–36) — `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. Seven variants:
  - `Reflexive` — same capability on both sides.
  - `IsoToTrn` — `iso <: trn` (relax unique to writable).
  - `IsoToVal` — `iso <: val` (freeze).
  - `IsoToBox` — `iso <: box` (transitive-read).
  - `IsoToTag` — `iso <: tag` (hide data).
  - `TrnToBox` — `trn <: box` (lose write, keep read).
  - `ValToBox` — `val <: box` (val is already immutable-readable).

- **`struct SubtypeError`** (lines 39–46) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]`. Two fields: `pub from: CapKind`, `pub to: CapKind`.

**Functions:**

- `pub const fn is_subtype(from: CapKind, to: CapKind) -> bool` (line 50) — calls `coerce(from, to).is_ok()`. This is the primary predicate used by `AliasMatrix::can_pass_through` and `cssl-hir`.
- `pub const fn coerce(from: CapKind, to: CapKind) -> Result<Subtype, SubtypeError>` (line 61) — the core relation. First handles reflexive cases via a combined `matches!` guard covering all six same-same pairs, then dispatches the seven non-reflexive permitted coercions. All other pairs hit `_ => Err(SubtypeError { from, to })`.

**Key invariants encoded:**
1. No auto-demotion `iso → ref`: `(Iso, Ref)` is an error. Aliasing must be explicit.
2. No demotion `ref → anything`: once shared (`ref`), the cap cannot be narrowed.
3. `tag` is minimal: the only permitted coercion into `tag` is `iso <: tag`.
4. `box` is the top of the read-only hierarchy: `iso`, `trn`, and `val` all coerce to `box`.

**Test module `tests`** (lines 85–184): 9 tests:
- `reflexive_for_all_caps` — all six reflexive pairs succeed with `Subtype::Reflexive`.
- `iso_can_become_trn_val_box_tag` — all four permitted iso coercions.
- `trn_can_become_box` / `val_can_become_box` — two additional coercions.
- `iso_cannot_auto_become_ref` — no auto-aliasing.
- `ref_cannot_demote` — once-shared is irreversible.
- `val_cannot_become_iso` — aliasable cannot gain exclusivity.
- `box_cannot_become_writable` — read-only cannot upgrade to writable.
- `tag_only_reflexive` — tag cannot become any other cap.
- `subtype_error_carries_pair` — error type preserves the `from`/`to` pair.

**Notable patterns:** Both functions are `const fn`, meaning the entire subtype lattice is computed at compile time for constant expressions. The `Subtype` witness enum provides rich information for downstream passes (e.g., MIR lowering may need to know whether a freeze coercion happened).

**No TODOs / FIXMEs / stubs.**

---

### 3.6 `cssl-caps/src/genref.rs` (141 lines)

Vale-style generational reference layout. Exposes the `u64` packing scheme for `ref<T>` lowering. Explicitly scoped to layout validation; the runtime `Pool<T>` and deref-check synthesis are T10 work.

**Constants:**

- `pub const GEN_BITS: u32 = 24` (line 24) — bits for the generation counter.
- `pub const IDX_BITS: u32 = 40` (line 26) — bits for the pool index.
- `pub const GEN_MASK: u64 = (1u64 << GEN_BITS) - 1` (line 28) — mask for generation field.
- `pub const IDX_MASK: u64 = (1u64 << IDX_BITS) - 1` (line 30) — mask for index field.

**Structs:**

- **`struct GenRef(pub u64)`** (line 35) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Default)]`. Packed u64: low 40 bits = index, high 24 bits = generation. Layout matches `specs/12` § VALE GEN-REFS AS `ref<T>` `@layout(cpu, std430)`.

**Methods on `GenRef`:**

- `pub const fn pack(idx: u64, gen: u64) -> Self` (line 42) — packs index + generation with silent truncation. `idx & IDX_MASK` then `gen & GEN_MASK` then `(g << IDX_BITS) | i`. Callers are responsible for pre-validation.
- `pub const fn idx(self) -> u64` (line 50) — extracts index via `self.0 & IDX_MASK`.
- `pub const fn gen(self) -> u64` (line 55) — extracts generation via `(self.0 >> IDX_BITS) & GEN_MASK`.
- `pub const fn bump_gen(self) -> Self` (line 64) — returns a new GenRef with generation incremented by 1, wrapping at `GEN_MASK`. Called when a pool slot is freed; the next allocation sees a new generation, invalidating all prior GenRefs pointing at that slot.
- `pub const NULL: Self = Self(0)` (line 71) — sentinel value. `NULL.idx() == 0` and `NULL.gen() == 0`.

**Test module `tests`** (lines 74–141): 9 tests:
- `bit_widths_match_spec` — verifies `IDX_BITS == 40`, `GEN_BITS == 24`, `IDX_BITS + GEN_BITS == 64`.
- `masks_cover_correct_bits` — verifies mask values.
- `pack_unpack_roundtrip` — pack(42, 7) → idx == 42, gen == 7.
- `pack_truncates_overflow_silently` — idx=100, gen=1<<25; gen becomes 0 (masked). Overflow is silent by design.
- `bump_gen_increments_generation` — gen goes 5 → 6.
- `bump_gen_wraps` — gen at `GEN_MASK` → 0.
- `null_sentinel_is_zero` — `NULL.0 == 0`.
- `max_idx_and_gen_packable` — both fields at max round-trip.
- `pack_is_const_eval_capable` — verifies `pack` usable in a `const` context.

**Notable patterns:** Silent truncation on overflow is a documented design choice (callers validate before packing). All methods are `const fn`. The layout directly matches the CSSL `std430` struct layout spec, enabling zero-copy interop between the host compiler and any future codegen target.

**No TODOs / FIXMEs / stubs.**

---

### 3.7 `cssl-effects/src/lib.rs` (60 lines)

Crate root for the effect system. Sets lint gates, declares the three sub-modules, re-exports all public items, and provides the `STAGE0_SCAFFOLD` constant.

**Items:**

- `pub const STAGE0_SCAFFOLD: &str` (line 50) — `CARGO_PKG_VERSION`.
- `mod scaffold_tests` (lines 53–60) — one test: `scaffold_version_present`.

**Re-exports:**
- From `banned`: `banned_composition`, `banned_composition_with_domains`, `BannedReason`, `SensitiveDomain`.
- From `discipline`: `classify_coercion`, `sub_effect_check`, `CoercionRule`, `EffectRef`, `SubEffectError`.
- From `registry`: `BuiltinEffect`, `DischargeTiming`, `EffectArgShape`, `EffectCategory`, `EffectMeta`, `EffectRegistry`, `BUILTIN_METADATA`.

**Lint gates:**
- `#![forbid(unsafe_code)]`.
- `#![deny(rustdoc::broken_intra_doc_links)]` / `private_intra_doc_links`.
- `#![allow(clippy::similar_names)]` — `caller`/`callee` are semantically correct domain names.

---

### 3.8 `cssl-effects/src/registry.rs` (528 lines)

The effect registry: the dense `BuiltinEffect` enum, metadata enums, `EffectMeta` struct, `EffectRegistry` struct, and the `BUILTIN_METADATA` compile-time table.

**Enums:**

- **`enum BuiltinEffect`** (lines 17–56) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`. 32 variants in five groups:
  - Resource + timing: `NoAlloc`, `NoRecurse`, `NoUnbounded`, `Deadline`, `Realtime`, `Region`, `Alloc`, `Yield`, `State`, `Exn`, `Io`.
  - Determinism + reversal: `DetRng`, `PureDet`, `Reversible`.
  - Hardware / backend gating: `Cpu`, `Gpu`, `Xmx`, `Rt`, `Simd256`, `Simd512`, `Numa`, `Cache`, `Backend`, `Target`.
  - Power + thermal: `Power`, `Thermal`.
  - Prime-directive + audit: `Sensitive`, `Audit`, `Privilege`, `Verify`, `Telemetry`.
  - Fiber + coroutine: `Resume`.

  Note: `Yield` appears in the Resource group in the enum but is categorized as `EffectCategory::Fiber` in the metadata table. This is consistent with specs (Yield is fiber-scheduling), but the enum comment says "resource + timing" — minor documentation mismatch.

- **`enum EffectCategory`** (lines 60–69) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`. Seven variants: `Resource`, `Determinism`, `Hardware`, `Power`, `Prime`, `Error`, `Fiber`.

- **`enum EffectArgShape`** (lines 76–89) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`. Six variants describing the argument structure at the annotation site:
  - `Nullary` — no arguments.
  - `OneType` — one type argument (e.g., `State<S>`).
  - `OneExpr` — one literal/expression argument (e.g., `Deadline<16ms>`).
  - `OneDomain` — one domain label string (e.g., `Sensitive<"privacy">`).
  - `OneRegion` — one region/lifetime parameter (e.g., `Region<'r>`).
  - `OneEnum` — one enum-value argument from a fixed set (e.g., `Cache<level>`).

- **`enum DischargeTiming`** (lines 92–100) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`. Three variants:
  - `CompileOnly` — purely static; no runtime check generated.
  - `CompileAndRuntimeAssert` — static check + runtime assertion emitted.
  - `UserHandler` — must be discharged by a user-installed effect handler.

**Structs:**

- **`struct EffectMeta`** (lines 103–115) — `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. Four fields: `name: &'static str`, `effect: BuiltinEffect`, `category: EffectCategory`, `args: EffectArgShape`, `discharge: DischargeTiming`.

- **`struct EffectRegistry`** (lines 118–123) — `#[derive(Debug, Clone, Default)]`. Two fields:
  - `by_name: HashMap<&'static str, EffectMeta>` — name-keyed lookup.
  - `by_effect: HashMap<BuiltinEffect, EffectMeta>` — variant-keyed reverse lookup.

**Methods on `EffectRegistry`:**

- `pub fn new() -> Self` (line 129) — delegates to `Default`.
- `pub fn with_builtins() -> Self` (line 135) — builds a registry pre-populated with all 32 built-in effects from `BUILTIN_METADATA`. This is the factory method callers should use.
- `pub fn register(&mut self, meta: EffectMeta)` (line 143) — inserts into both maps.
- `pub fn lookup(&self, name: &str) -> Option<&EffectMeta>` (line 150) — name-keyed lookup.
- `pub fn lookup_variant(&self, effect: BuiltinEffect) -> Option<&EffectMeta>` (line 157) — variant-keyed lookup.
- `pub fn iter(&self) -> impl Iterator<Item = &EffectMeta> + '_` (line 162) — iterates `by_name.values()` (order non-deterministic since HashMap).
- `pub fn len(&self) -> usize` (line 168).
- `pub fn is_empty(&self) -> bool` (line 174).

**`BUILTIN_METADATA` table** (lines 183–414): A `pub const &[EffectMeta]` slice of 32 entries. Order follows the spec's grouping sections. Every entry specifies all five metadata fields. Key entries:
- `Sensitive` / `Audit` — `EffectArgShape::OneDomain`, `EffectCategory::Prime`.
- `Privilege` / `Verify` / `Telemetry` — `EffectArgShape::OneEnum`, `EffectCategory::Prime`.
- `Deadline` / `Power` / `Thermal` — `EffectArgShape::OneExpr`, `DischargeTiming::CompileAndRuntimeAssert`.
- `Io` / `State` / `Exn` / `Yield` / `Resume` — `DischargeTiming::UserHandler`.
- All hardware effects (`CPU`, `GPU`, `XMX`, `RT`, `SIMD256`, `SIMD512`) — `DischargeTiming::CompileOnly`, `EffectArgShape::Nullary`.

**Test module `tests`** (lines 417–528): 8 tests:
- `builtin_metadata_covers_all_variants` — exhaustively checks all 32 variants appear in `BUILTIN_METADATA`.
- `registry_counts_28_plus_extras` — asserts `len() >= 28` and equals `BUILTIN_METADATA.len()`.
- `lookup_by_name_roundtrips` — verifies `NoAlloc` lookup returns correct category/timing.
- `lookup_by_variant_roundtrips` — verifies GPU variant → name `"GPU"`.
- `deadline_takes_expr_arg` — `Deadline` → `OneExpr`.
- `sensitive_takes_domain_arg` — `Sensitive` → `OneDomain`, `Prime` category.
- `audit_has_runtime_discharge` — `Audit` → `CompileAndRuntimeAssert`.
- `unknown_name_returns_none`.
- `iter_over_all_registered`.

**Spec divergence note:** The spec mentions "28+ effects". The actual `BUILTIN_METADATA` table has **32 entries** (the 28 canonical plus `Yield`, `Resume`, `Region`, and one extra). The test comment says "28 canonical + Yield + Resume + Region = 32". The spec may need an update or this is the planned expansion.

**No TODOs / FIXMEs / stubs.**

---

### 3.9 `cssl-effects/src/banned.rs` (308 lines)

The Prime-Directive structural composition checker. Encodes three hard prohibitions as type-level errors. Contains `SensitiveDomain`, `BannedReason`, and two checking functions.

**Structs / Enums:**

- **`enum SensitiveDomain<'a>`** (lines 33–44) — `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`. Four named domains plus one open variant:
  - `Privacy` — personal data.
  - `Weapon` — weapon systems.
  - `Surveillance` — surveillance.
  - `Coercion` — behavior modification / coercion.
  - `Other(&'a str)` — user-defined domain; validated against a project-level allow-list elsewhere.

- **`enum BannedReason`** (lines 81–101) — `#[derive(Debug, Clone, Error, PartialEq, Eq)]`. Three variants:
  - `CoercionAbsolute` — `Sensitive<"coercion">` banned in any context, period. Error message cites `PRIME DIRECTIVE § 1 : N! coercion`.
  - `SurveillanceWithIo` — `Sensitive<"surveillance"> + IO` banned with no override. Cites `N! surveillance`.
  - `WeaponWithIoNeedsKernel` — `Sensitive<"weapon"> + IO` requires `Privilege<Kernel>`. Cites `N! weaponization`.

**Methods on `SensitiveDomain<'a>`:**

- `pub fn from_label(label: &'a str) -> Self` (line 50) — parses a domain label string. Returns the named variant for the four built-in domains, `Other(label)` otherwise.
- `pub const fn is_absolute_ban(&self) -> bool` (line 62) — `true` only for `Coercion`.
- `pub const fn is_io_banned_unless_kernel(&self) -> bool` (line 68) — `true` only for `Weapon`.
- `pub const fn is_io_banned_no_override(&self) -> bool` (line 75) — `true` only for `Surveillance`.

**Functions:**

- `pub fn banned_composition(row: &[EffectRef<'_>]) -> Result<(), Vec<BannedReason>>` (line 107) — the "light" variant. Inspects an effect row to find `Sensitive` entries, but cannot inspect the actual domain label (stage-0 lacks const-evaluation of effect arguments). Instead it uses `EffectRef::name` as a proxy — this means all `Sensitive`-named effects are treated as `SensitiveDomain::Other(e.name)`, which never triggers any of the three ban predicates (since `Other` matches none of them). This function is therefore a no-op for the three hard bans in stage-0; it only detects structurally present `IO` and `Privilege` flags. Callers in `cssl-hir` must use `banned_composition_with_domains` once they've resolved the domain literal from the HIR.
- `pub fn banned_composition_with_domains(row: &[EffectRef<'_>], sensitive_domains: &[SensitiveDomain<'_>]) -> Result<(), Vec<BannedReason>>` (line 149) — the full-fidelity variant. Takes an explicitly resolved `SensitiveDomain` slice from the caller (which has already const-evaluated the effect argument). Applies all three ban rules:
  1. Any `Coercion` domain → `CoercionAbsolute` (no further checks needed, but still accumulates).
  2. Any domain with `is_io_banned_no_override()` AND `has_io` → `SurveillanceWithIo`.
  3. Any domain with `is_io_banned_unless_kernel()` AND `has_io` AND NOT `has_kernel_priv` → `WeaponWithIoNeedsKernel`.
  Returns all violations found (not fail-fast). `has_kernel_priv` is detected by `Privilege` in row with `arg_count == 1` — this is a structural proxy (all Privilege entries are arity-1 per spec), not a check that the arg is specifically the `Kernel` level.

**Test module `tests`** (lines 180–308): 10 tests:
- `coercion_domain_absolutely_banned` — coercion triggers `CoercionAbsolute`.
- `surveillance_with_io_banned_no_override` — even with `Privilege` in row.
- `weapon_with_io_needs_kernel` — weapon + IO without Privilege fails.
- `weapon_with_io_plus_kernel_privilege_ok` — weapon + IO with Privilege passes.
- `privacy_with_io_is_fine` — Privacy is not in any ban predicate.
- `no_sensitive_is_trivially_ok` — no Sensitive effects → always clean.
- `coercion_bans_even_without_io` — CoercionAbsolute fires even with no IO.
- `domain_label_classification` — `from_label` parses all four named domains.
- `classification_predicates` — predicate methods for each domain.
- `multiple_violations_reported` — Coercion + Surveillance → 2 violations in one call.

**Spec divergence / design note:** The `banned_composition` function (the "light" variant without explicit domains) is effectively a non-operation for the three PRIME DIRECTIVE bans because `SensitiveDomain::Other(&str)` returns `false` from all three predicate methods. This is documented inline: callers wanting full protection must use `banned_composition_with_domains`. New contributors may incorrectly believe the light variant provides protection.

**Privilege-level limitation:** `has_kernel_priv` detects any `Privilege` entry with `arg_count == 1`, not specifically `Privilege<Kernel>`. The actual privilege level (Kernel vs. User vs. etc.) would require const-evaluation of the argument, which is T8 work. The current implementation is an over-permissive approximation for the Weapon ban: any `Privilege<anything>` will satisfy the kernel check.

**No TODOs / FIXMEs / stubs.**

---

### 3.10 `cssl-effects/src/discipline.rs` (219 lines)

The effect-row sub-effect containment checker and coercion classifier.

**Structs / Enums:**

- **`struct EffectRef<'a>`** (lines 31–39) — `#[derive(Debug, Clone, PartialEq, Eq)]`. Three fields:
  - `pub name: &'a str` — source-form effect name.
  - `pub builtin: Option<BuiltinEffect>` — `None` for user-defined effects.
  - `pub arg_count: usize` — number of arguments at this use-site.
  
  This is the effect-crate-level view; `cssl-hir` owns a richer `EffectInstance` (with interned `Symbol` + `Ty` args).

- **`enum CoercionRule`** (lines 42–51) — `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. Three variants:
  - `Exact` — same name, same arg count, structural equality.
  - `Widening` — caller widens the budget (tighter-into-looser). Stage-0 accepts this without numeric comparison; T8 const-eval will add the numeric check.
  - `None` — no coercion available.

- **`enum SubEffectError`** (lines 54–66) — `#[derive(Debug, Clone, Error, PartialEq, Eq)]`. Two variants:
  - `MissingEffect { effect: String }` — callee requires an effect not in caller's row.
  - `ArgMismatch { effect: String, caller_arity: usize, callee_arity: usize }` — arity mismatch.

**Functions:**

- `pub fn sub_effect_check(caller: &[EffectRef<'_>], callee: &[EffectRef<'_>], _registry: &EffectRegistry) -> Result<(), SubEffectError>` (line 78) — validates that every effect in `callee` has a matching effect in `caller` by name. Note: `_registry` is accepted but not used (reserved for future widening rules that need category-level information). Algorithm: for each `e_callee`, find `e_caller` with `e.name == e_callee.name`; if missing → `MissingEffect`; if found but `arg_count` differs → `ArgMismatch`; otherwise accept. Returns on first error (fail-fast). A pure-callee (empty callee row) always succeeds.
- `pub fn classify_coercion(caller: &EffectRef<'_>, callee: &EffectRef<'_>) -> CoercionRule` (line 109) — classifies the coercion between two matched effects. Returns `None` if names or arg counts differ. Returns `Widening` for `Deadline`, `Power`, or `Thermal` (which carry expression arguments subject to numeric ordering); returns `Exact` for everything else.

**Test module `tests`** (lines 127–219): 9 tests:
- `pure_callee_is_always_sub` — empty callee row accepted under any caller.
- `exact_match_succeeds` — GPU → GPU.
- `missing_effect_fails` — GPU caller, NoAlloc callee → MissingEffect.
- `arg_count_mismatch_fails` — Deadline arity 1 vs 0 → ArgMismatch.
- `multiple_effects_all_matched` — three-effect caller, two-effect callee.
- `classify_exact_vs_widening` — GPU→GPU is Exact, Deadline→Deadline is Widening.
- `classify_different_names_is_none` — GPU vs CPU → None.
- `classify_power_widening` / `classify_thermal_widening` — verify widening classification.

**Stage-0 limitation documented in source (line 20):** "Coercion comparisons on expression-valued args (e.g., `Deadline<5ms>`) require const-evaluation, which is T8 work. For stage-0 we flag exact-match as a sufficient condition and defer the numeric-ordering check to T8."

**Design gap:** The `_registry` parameter to `sub_effect_check` is accepted but unused (`_`-prefixed in the parameter pattern). Future Widening rule implementation that needs the registry must update the function body. No TODO comment marks this explicitly, though the spec comment describes the intent.

**No TODOs / FIXMEs / stubs** (the `_registry` silent-discard is the only notable gap, and it is intentional for stage-0).

---

### 3.11 `cssl-ifc/src/lib.rs` (24 lines)

The entire `cssl-ifc` crate. This file is a placeholder scaffold.

**Contents:**
- Crate-level doc comment (lines 1–10): States the authoritative design is `specs/11_IFC.csl`, identifies the status as "T3+ scaffold — label propagation + declass validator pending", and describes the structural encoding of PRIME DIRECTIVE consent and sovereignty. No actual items are declared beyond what follows.
- Lint gates: `#![forbid(unsafe_code)]`, `#![deny(rustdoc::broken_intra_doc_links)]`, `#![deny(rustdoc::private_intra_doc_links)]`.
- `pub const STAGE0_SCAFFOLD: &str` (line 16) — version string.
- `mod scaffold_tests` (lines 19–24) — one test: `fn scaffold_version_present()`.

**No public API.** No exported types, traits, or functions. No dependencies in `Cargo.toml` beyond workspace defaults.

---

### 3.12 `cssl-hir/src/ifc.rs` (1,168 lines) — The Actual IFC Implementation

Although outside the nominal audit slice (which was `cssl-ifc`), this file contains all IFC logic that the `cssl-ifc` crate's description promises. It is documented here for completeness and because any new contributor looking at `cssl-ifc` must be directed here.

**Purpose:** Implements the Jif-DLM information-flow control label lattice, structural attribute walker, T11-D36 dataflow flow-violation walker, diagnostic types, and supporting utilities. Depends on `cssl-hir` internals (`HirModule`, `HirFn`, `HirExpr`, `HirBlock`, `HirStmtKind`, `HirPatternKind`, `IfcLabel`, etc.) and `cssl-ast` (`Span`).

**Structs / Enums:**

- **`struct IfcLabel`** (line 54) — `#[derive(Debug, Clone, PartialEq, Eq, Default)]`. Two fields: `pub confidentiality: BTreeSet<Symbol>`, `pub integrity: BTreeSet<Symbol>`. Implements the DLM label pair `(C, I)` where C is the set of principals allowed to READ and I is the set of principals allowed to INFLUENCE the value.

- **`enum IfcDiagnostic`** (line 146) — `#[derive(Debug, Clone, PartialEq, Eq)]`. Four variants with stable codes:
  - `MissingLabel { fn_name, fn_span }` → `"IFC0001"`.
  - `MissingDeclassPolicy { fn_name, fn_span }` → `"IFC0002"`.
  - `UnauthorizedDowngrade { fn_name, from, to, fn_span }` → `"IFC0003"`.
  - `FlowViolation { fn_name, param_name, label, fn_span }` → `"IFC0004"` (added T11-D36).

- **`struct IfcReport`** (line 224) — `#[derive(Debug, Clone, Default, PartialEq, Eq)]`. Aggregate report: `diagnostics: Vec<IfcDiagnostic>`, `fns_checked: u32`, `fns_with_labels: u32`, `declass_attempts: u32`.

- **`struct IfcLabelRegistry`** (line 732) — `#[derive(Debug, Clone, Default)]`. `map: BTreeMap<u32, IfcLabel>` (DefId → IfcLabel). Placeholder registry; "Phase-2b will populate from HIR-type annotations."

**Functions (public):**

- `pub fn builtin_principals(interner: &Interner) -> Vec<Symbol>` (line 130) — interns and returns the 9 PRIME DIRECTIVE principal names: `HarmTarget`, `Surveiller`, `Coercer`, `Weaponizer`, `System`, `Kernel`, `User`, `Public`, `Anthropic-Audit`.
- `pub fn check_ifc(module: &HirModule, interner: &Interner) -> IfcReport` (line 271) — structural attribute-level walker. Iterates all `HirItem::Fn` items. For each fn, calls `check_fn`.
- `pub fn check_ifc_full(module: &HirModule, interner: &Interner) -> IfcReport` (line 386) — runs both `check_ifc` and `check_ifc_flow_into` and returns a combined report.
- `pub fn check_ifc_flow(module: &HirModule, interner: &Interner) -> IfcReport` (line 397) — standalone dataflow-only walker; returns a fresh report with only IFC0004 diagnostics.
- `pub fn resolve_builtin_principal(name: &str, interner: &Interner) -> Option<Symbol>` (line 699) — returns `Some(interned)` if `name` is one of the 9 built-in principals.
- `pub fn label_for_secret(principals: impl IntoIterator<Item = Symbol>, interner: &Interner) -> IfcLabel` (line 720) — builds a label with `confidentiality = principals, integrity = {}`. The `interner` parameter is accepted but not used (`let _ = interner` at line 725 — this is a minor smell but harmless).

**Methods on `IfcLabel`:**

- `pub fn empty() -> Self` (line 64) — empty label.
- `pub fn new(confidentiality: impl IntoIterator<Item = Symbol>, integrity: impl IntoIterator<Item = Symbol>) -> Self` (line 70) — from explicit sets.
- `pub fn is_sub_of(&self, other: &Self) -> bool` (line 82) — `L1 ⊑ L2 ≡ C_self ⊇ C_other ∧ I_self ⊆ I_other`. Note: the DLM ordering is counter-intuitive — a MORE restrictive reader set (smaller confidentiality set) means higher confidentiality, which is "lower" in the lattice. The `is_sub_of` test at line 843 verifies this correctly.
- `pub fn join(&self, other: &Self) -> Self` (line 89) — lattice `⊔`: upper-bound. Intersects confidentiality (stricter-of-both), unions integrity. This is the formal DLM join.
- `pub fn meet(&self, other: &Self) -> Self` (line 102) — lattice `⊓`: lower-bound. Unions confidentiality, intersects integrity.
- `pub fn is_labeled(&self) -> bool` (line 119) — `true` iff either set is non-empty.

**Methods on `IfcDiagnostic`:**

- `pub const fn code(&self) -> &'static str` (line 179) — returns stable code string.
- `pub fn message(&self) -> String` (line 190) — human-readable diagnostic message.

**Methods on `IfcReport`:**

- `pub fn is_clean(&self) -> bool` (line 239) — `diagnostics.is_empty()`.
- `pub fn count(&self, code: &str) -> usize` (line 245) — count diagnostics by code string.
- `pub fn summary(&self) -> String` (line 251) — one-line summary including counts for all four codes.

**Methods on `IfcLabelRegistry`:**

- `pub fn new() -> Self` / `pub fn insert(&mut self, def: DefId, label: IfcLabel)` / `pub fn get(&self, def: DefId) -> Option<&IfcLabel>` / `pub fn len(&self) / is_empty()` — standard map operations.

**Private functions (internal helpers):**

- `fn check_fn(f, interner, ...)` (line 299) — per-fn structural walker. Looks for `@confidentiality`, `@integrity`, `@ifc_label` on the fn (labeled); `@sensitive` on any param; `@declass` and `@requires` on the fn. Emits IFC0001 (sensitive param, no fn label) and IFC0002 (declass without requires).
- `fn check_ifc_flow_into(module, interner, report)` (line 405) — internal: runs the T11-D36 flow walker on all fn bodies in the module, appending to an existing report.
- `fn check_fn_flow(f, interner, ...)` (line 450) — per-fn T11-D36 walker. Skips if: body absent; `@declass + @requires` present (authorized); fn-level `@confidentiality`/`@ifc_label` present (declared labeled output). Seeds `locals` from `@sensitive` params with label `{User}`. Calls `label_of_block` on the body, then checks if the return label is non-empty and which sensitive params contributed.
- `fn extract_binding_symbol(pat: &HirPattern) -> Option<Symbol>` (line 545) — extracts the single-binding symbol from a `HirPatternKind::Binding` pattern; returns `None` for destructuring/tuple/wildcard.
- `fn label_of_block(block: &HirBlock, locals: &mut HashMap<Symbol, IfcLabel>) -> IfcLabel` (line 555) — propagates labels through let-statements and returns the trailing-expression label.
- `fn combine_labels(a: &IfcLabel, b: &IfcLabel) -> IfcLabel` (line 583) — **important deviation from formal lattice**: unions BOTH confidentiality and integrity sets (not the DLM `⊔` which intersects confidentiality). This is documented at lines 580–591 as a deliberate stage-0 simplification for taint tracking. The formal `join` (which intersects confidentiality) is the correct operation for the lattice, but for detecting violations (any-taint-reaches-output) the union is sound — it over-approximates, never misses a real violation. Full lattice-accurate propagation is deferred to T3.4-phase-3-IFC-b.
- `fn label_of_expr(expr: &HirExpr, locals: &mut HashMap<Symbol, IfcLabel>) -> IfcLabel` (line 597) — recursive bottom-up taint propagation across the HIR expression grammar. Handles: `Literal` → empty; `Path` (single-segment) → locals lookup; `Binary` → combine both operands; `Unary` → operand; `Call` → combine callee + all args; `Field` → object; `Index` → object + index; `Block` → `label_of_block`; `If` → combine cond + then + else; `Match` → combine scrutinee + all arm bodies; `Return` → value label; `Cast` → inner; `Paren` → inner; `Tuple` → reduce over elements; `Array` → reduce over list or repeat elem+len. Unhandled variants return empty (conservatively sound under-approximation — documented at lines 673–677).
- `fn format_label(label: &IfcLabel, interner: &Interner) -> String` (line 682) — renders a label as `"confid{X,Y} + integ{Z}"` for diagnostic messages.

**Test module `tests`** (lines 766–1168): Very comprehensive. 28+ tests split into structural and flow sections:

Structural tests (using `check`):
- `empty_label_shapes` / `label_new_populates_sets` — label construction.
- `lattice_join_intersects_confid_and_unions_integrity` — verifies formal join.
- `lattice_meet_unions_confid_and_intersects_integrity` — verifies formal meet.
- `lattice_is_sub_of_respects_ordering` — verifies DLM ordering (counter-intuitive direction).
- `builtin_principals_covers_prime_directive_principals` — 9 principals, named checks.
- `label_for_secret_populates_confid`.
- `empty_module_is_clean` / `unlabeled_fn_without_sensitive_params_is_clean`.
- `ifc_label_attr_counted_as_labeled`.
- `declass_without_requires_emits_ifc0002`.
- `declass_with_requires_is_clean`.
- `sensitive_param_without_label_emits_ifc0001`.
- `sensitive_param_with_label_is_clean`.
- `diagnostic_codes_stable` — verifies IFC0001/IFC0002/IFC0003 codes.
- `report_summary_shape`.
- `label_registry_roundtrips`.

Flow tests (T11-D36, using `check_flow` / `check_full`):
- `flow_no_sensitive_params_is_clean`.
- `flow_sensitive_param_reaching_unlabeled_return_emits_ifc0004` — classic violation.
- `flow_sensitive_param_with_confid_label_on_fn_is_clean`.
- `flow_declass_plus_requires_authorizes_downgrade`.
- `flow_binary_op_propagates_sensitive_label`.
- `flow_sensitive_not_referenced_is_clean` — non-referenced sensitive param doesn't leak.
- `flow_let_binding_propagates_label`.
- `flow_if_branch_joins_labels`.
- `flow_literal_return_is_clean`.
- `flow_cast_preserves_label`.
- `flow_unary_preserves_label`.
- `flow_full_combines_structural_and_flow_diagnostics`.
- `flow_ifc0004_code_stable`.
- `flow_signature_only_fn_is_ignored`.
- `flow_summary_includes_ifc0004_column`.

---

## 4. Slice Notes

### Test Coverage Summary

| Crate / File | Inline test count | Coverage quality |
|---|---|---|
| `cssl-caps/cap.rs` | 10 | Comprehensive — every predicate exhaustively tested |
| `cssl-caps/linearity.rs` | 11 | Comprehensive — all violation types + happy paths |
| `cssl-caps/matrix.rs` | 7 | Comprehensive — every matrix cell + pass-through |
| `cssl-caps/subtype.rs` | 9 | Comprehensive — all permitted + all blocked coercions |
| `cssl-caps/genref.rs` | 9 | Comprehensive — pack/unpack/bump/overflow/const-eval |
| `cssl-effects/registry.rs` | 9 | Good — roundtrips, shape checks, missing-name |
| `cssl-effects/banned.rs` | 10 | Good — all three ban rules + domain classification |
| `cssl-effects/discipline.rs` | 9 | Good — sub-check + classification for all CoercionRule variants |
| `cssl-ifc/lib.rs` | 1 | Scaffold only |
| `cssl-hir/ifc.rs` | ~28 | Excellent — structural + dataflow, many edge cases |

No external `tests/` directories exist for any of the three crates. All tests are inline.

---

### What Is Complete vs. Stubbed

**Complete (stage-0 ready):**
- `cssl-caps`: Fully implemented for all six capabilities — alias matrix, subtype lattice, linear tracker, GenRef layout. No stubs or TODOs.
- `cssl-effects` registry and banned-composition checker: All 32 effects registered with metadata; PRIME DIRECTIVE bans structurally encoded. No stubs or TODOs.
- `cssl-hir/src/ifc.rs`: Substantial real implementation — lattice algebra, structural walker, T11-D36 dataflow walker, all four diagnostic codes with tests. Not a stub.

**Partial / deferred by design (documented):**
- `cssl-effects/discipline.rs` — `sub_effect_check`: numeric-ordering comparison for `Deadline<N> ⊆ Deadline<M>` (requires const-eval, deferred to T8). The `_registry` parameter is accepted but unused.
- `cssl-effects/banned.rs` — `banned_composition` (the "light" variant without explicit domains): effectively a no-op for PRIME DIRECTIVE bans at stage-0 (cannot inspect the domain literal). Must be replaced by `banned_composition_with_domains` by `cssl-hir` callers.
- `cssl-effects/banned.rs` — `has_kernel_priv` detection: detects any `Privilege` entry with arity 1, not specifically the Kernel level. Over-permissive for the Weapon ban.
- `cssl-hir/src/ifc.rs` — `combine_labels`: uses union-of-both-sets rather than formal DLM `⊔` (documented as intentional stage-0 simplification).
- `cssl-hir/src/ifc.rs` — `label_of_expr` unhandled variants: a conservative empty-label return for unrecognized expression kinds. Documented as a potential source of missed violations for complex control flow.
- `cssl-hir/src/ifc.rs` — `label_for_secret`: accepts but ignores the `interner` parameter (`let _ = interner`). Minor dead-argument smell.
- `cssl-hir/src/ifc.rs` — `IFC0003` (UnauthorizedDowngrade): the diagnostic type exists and the code is stable, but `check_fn` does not emit it — the declass-direction check (verifying `from`/`to` label ordering) is not yet implemented. The test for IFC0003 only checks the code string on a manually constructed diagnostic, not that the walker emits it.
- `cssl-ifc/src/lib.rs`: Pure placeholder. Contains only version scaffold. All IFC logic lives in `cssl-hir`.

---

### README Divergences

No README files exist for any of the three crates. There is no documentation divergence risk from external docs. Crate-level `//!` doc comments in each `lib.rs` are the primary documentation and are accurate.

---

### Dead Code

- `cssl-effects/discipline.rs`: The `_registry` parameter to `sub_effect_check` (line 82) is a named parameter prefix-suppressed with `_`. It is architecturally reserved but currently dead. No lint would fire because of the `_`-prefix.
- `cssl-hir/src/ifc.rs`: `label_for_secret` line 725: `let _ = interner;` — the interner parameter is accepted but not used. Could be removed from the signature once the function's callers no longer need to construct a shared-interner pattern.

---

### Notable Surprises and Findings

1. **`cssl-ifc` is an empty shell.** Despite its Cargo.toml description ("Jif-DLM label lattice + declassification + PRIME-DIRECTIVE encoding"), the crate exports nothing. All the described functionality is in `cssl-hir/src/ifc.rs`. The natural next action is to factor `cssl-hir/src/ifc.rs` into `cssl-ifc` and have `cssl-hir` depend on `cssl-ifc`. This would put the IFC logic where its crate name says it lives.

2. **Effect count is 32, not 28.** The spec and test comments say "28+" but `BUILTIN_METADATA` has exactly 32 entries. `Yield`, `Resume`, `Region`, and one additional effect expand the canonical 28. The test `registry_counts_28_plus_extras` acknowledges this with its comment but the count discrepancy is not resolved in the spec.

3. **`Yield` categorization mismatch.** In `BuiltinEffect`, `Yield` appears in the "resource + timing" comment group (line 19 of `registry.rs`), but in `BUILTIN_METADATA` it is assigned `EffectCategory::Fiber`. The category assignment is correct per specs; the inline comment in the enum is stale.

4. **`banned_composition` light variant is a no-op for PRIME DIRECTIVE bans.** A caller who calls `banned_composition` (instead of `banned_composition_with_domains`) gets no PRIME DIRECTIVE protection in stage-0. This is documented but easy to misuse. A new contributor integrating this into `cssl-hir` must know to use the `_with_domains` variant.

5. **IFC0003 (UnauthorizedDowngrade) is declared but never emitted.** The `IfcDiagnostic::UnauthorizedDowngrade` variant exists, its code is `"IFC0003"`, and there is a test asserting the code value — but `check_fn` never constructs or pushes this variant. A declass-direction check (verifying that the lattice ordering of `from`/`to` is sound) is absent. This is the only IFC diagnostic code with a test that does not exercise the walker's emission path.

6. **Taint model deviation is explicitly documented.** `combine_labels` uses set-union for both confidentiality and integrity instead of the formal DLM `⊔` (which intersects confidentiality). The source comments at lines 580–591 explain this correctly and note that it is a sound over-approximation. This is by design and not a bug, but a new contributor unfamiliar with DLM might be confused by the comment on `join` vs. the behavior of `combine_labels`.

7. **`has_kernel_priv` check is structurally coarse.** In `banned.rs` line 125, `has_kernel_priv` is `true` whenever a `Privilege` effect with `arg_count == 1` exists in the row. This does not distinguish `Privilege<Kernel>` from `Privilege<User>` or other privilege levels. Any privilege annotation satisfies the weapon-with-IO kernel check. This must be tightened when T8 const-evaluation is available.

8. **All `const fn` everywhere possible.** Both `cssl-caps` and `cssl-effects` aggressively use `const fn` — `CapKind::is_linear`, `CapKind::index`, `GenRef::pack`, `GenRef::bump_gen`, `coerce`, `is_subtype`, `AliasRights::for_cap`, `AliasMatrix::pony6`, `AliasMatrix::get`. This enables compile-time table construction and is a strong design choice.

---

*End of audit document.*
