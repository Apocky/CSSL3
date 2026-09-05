# Audit: cssl-mir — Mid-Level IR

**Auditor:** Claude (claude-sonnet-4-6)  
**Date:** 2026-05-14  
**Files audited:** 12 (11 Rust sources + Cargo.toml)  
**Total functions/types documented:** ~180 items

---

## 1. Crate Overview

`cssl-mir` is the **Mid-Level Intermediate Representation** (MIR) crate for the CSSLv3 stage-0 bootstrap compiler. It sits between the HIR (High-level IR produced by `cssl-hir`) and downstream code generation (Cranelift JIT in `cssl-cgen-cpu-cranelift`). Its design explicitly follows an **MLIR-dialect shape**: the IR is structured rather than flat, uses SSA values with monotonic IDs, and groups operations into regions of blocks — exactly the structural invariants MLIR enforces.

### Place in the pipeline

```
Source text
    ↓  cssl-lex + cssl-parse
CST (Concrete Syntax Tree)
    ↓  cssl-hir
HirModule  ← cssl-hir defines this
    ↓  cssl-mir: lower.rs (signatures), body_lower.rs (bodies)
MirModule  ← cssl-mir defines this
    ↓  cssl-mir: monomorph.rs + auto_monomorph.rs
MirModule (monomorphized, all generic fns specialized)
    ↓  cssl-mir: pipeline.rs (pass driver)
MirModule (validated, passes applied)
    ↓  print.rs → textual MLIR for --emit-mlir
    ↓  cssl-cgen-cpu-cranelift → native code
```

### MIR data model

The core data model has four layers:

- **`MirModule`**: top-level container holding a list of `MirFunc`s and optional module-level attributes.
- **`MirFunc`**: one function — name, flat param type list, result types, effect-row string, cap annotation, IFC label, body (`MirRegion`), and a monotonic value-ID counter.
- **`MirRegion`** / **`MirBlock`**: a region is a sequence of blocks; a block is a label, a list of block-argument `MirValue`s, and a list of `MirOp`s. At stage-0 every function compiles to exactly one region with one `^entry` block.
- **`MirOp`**: a single dialect operation — dialect kind (`CsslOp` enum), a string name (either the canonical cssl.* name or a free-form standard-dialect name), operand value-IDs, result `MirValue`s (typed), attribute key-value pairs, and zero or more nested regions.
- **`MirValue`** / **`ValueId`**: SSA value identified by a monotonic `u32`. Every `MirValue` carries both the ID and its `MirType`.
- **`MirType`**: tagged union covering Int (I8/I16/I32/I64/Index), Float (F16/Bf16/F32/F64), Bool, None, Handle, Tuple, Function, Memref, Vec (lane-count + FloatWidth), and Opaque (pass-through string).

### Lowering strategy

HIR-to-MIR lowering happens in two stages. `lower.rs` (signature-level) produces an empty-body `MirFunc` per HIR fn item in one module walk, resolving types and effect-rows from HIR into flat MIR types. `body_lower.rs` then populates the entry-block with actual MIR ops by recursively traversing `HirExprKind` variants, allocating fresh SSA value-IDs, and emitting `MirOp` nodes. Vec parameters (`vec2`/`vec3`/`vec4`) are scalarized into N consecutive scalar entries (T11-D35).

### Monomorphization

The crate implements a **monomorphization quartet** across two files:

- **`monomorph.rs`** (T11-D38/D45/D47/D49): the **core specialization API**. `TypeSubst` maps generic-param symbols to concrete `HirType`s. `substitute_hir_type` recursively rewrites a type tree. `specialize_generic_fn`, `specialize_generic_struct`, `specialize_generic_enum`, and `specialize_generic_impl` each take a generic HIR item + a substitution and produce a concrete MIR or HIR specialization with a deterministic mangled name.
- **`auto_monomorph.rs`** (T11-D40/D43/D41/D46/D48/D50): the **auto-discovery walkers**. Four walkers — `auto_monomorphize` (fns), `auto_monomorphize_structs` (structs), `auto_monomorphize_enums` (enums), `auto_monomorphize_impls` (impl blocks) — scan the HIR module for generic references, build substitutions, deduplicate, and invoke the core API. `drop_unspecialized_generic_fns` removes template fns post-specialization. `rewrite_generic_call_sites` rewrites `func.call` callee attributes from generic names to mangled specialization names using the `hir_id` attribute stamped by body-lowering.

### Maturity

Stage-0 — fully functional for the stated scope. All 31 `HirExprKind` variants are covered in `body_lower.rs` (the last six completed in T6-phase-2c). The pass pipeline has one real pass (`StructuredCfgValidator`) and five informational stubs. No LLVM/melior FFI. TableGen authoring, type-inference-driven lowering, and dialect conversion are deferred to T6-phase-2d and later.

---

## 2. Crate Metadata

| Field | Value |
|---|---|
| Path | `compiler-rs/crates/cssl-mir/` |
| Purpose | Structured MIR — MLIR-dialect-shaped IR and all related lowering, monomorphization, pass infrastructure, and pretty-printing |
| Version | workspace-inherited |
| Dependencies | `cssl-ast`, `cssl-hir`, `thiserror` |
| Dev-dependencies | `cssl-lex`, `cssl-parse` (for round-trip integration tests) |
| Total source files | 11 `.rs` files |
| Total LOC | ~9,600 |

**File list (with rough LOC):**

| File | Approx. LOC |
|---|---|
| `src/lib.rs` | 88 |
| `src/value.rs` | 296 |
| `src/op.rs` | 352 |
| `src/block.rs` | 223 |
| `src/func.rs` | 171 |
| `src/lower.rs` | 292 |
| `src/body_lower.rs` | 2,066 |
| `src/monomorph.rs` | 1,398 |
| `src/auto_monomorph.rs` | 1,857 |
| `src/pipeline.rs` | 539 |
| `src/print.rs` | 341 |

---

## 3. Per-File Audit

### 3.1 `src/lib.rs` (88 lines)

**Purpose:** Crate root. Sets lint configuration and re-exports every public item from the sub-modules into the crate's flat public surface.

**Items:**

- `#![forbid(unsafe_code)]` — no unsafe anywhere in the crate.
- `#![allow(...)]` — 14 specific Clippy lints suppressed with comments explaining the rationale (large match arms, redundant clones, similar names, etc.). These are appropriate for a stage-0 dialect-op crate.
- `pub mod` declarations for all 10 sub-modules.
- `pub use` re-exports covering every significant public type and function across all modules.
- `const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION")` — exposes the crate version as a constant for scaffold verification.
- **`#[cfg(test)] mod scaffold_tests`**: one test (`scaffold_version_present`) that asserts `STAGE0_SCAFFOLD` is non-empty.

**Spec references:** Module doc comment cites `specs/02_IR.csl` § MIR and `specs/15_MLIR.csl` (full dialect design).

**Notes:** The T6-phase-2 deferred items listed in the module doc (melior/mlir-sys FFI, TableGen, full body lowering, pass pipeline, dialect conversion) are now partially addressed in subsequent sub-modules added after the initial commit.

---

### 3.2 `src/value.rs` (296 lines)

**Purpose:** Defines the SSA value type system: `ValueId`, `MirValue`, `MirType`, `IntWidth`, `FloatWidth`.

**Structs:**

- **`ValueId(pub u32)`**: newtype wrapping a monotonic u32. Derives `Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd`. Displays as `%N` (MLIR convention).
  - `impl fmt::Display` — formats as `%{self.0}`.

- **`MirValue`**: pairs a `ValueId` with a `MirType`.
  - Fields: `id: ValueId`, `ty: MirType`.
  - `pub const fn new(id: ValueId, ty: MirType) -> Self` — constructor.

**Enums:**

- **`MirType`**: the MIR type universe. Variants:
  - `Int(IntWidth)` — signless integer per MLIR convention.
  - `Float(FloatWidth)`.
  - `Bool` — `i1`.
  - `None` — absence of value.
  - `Handle` — `!cssl.handle`, packed generational reference.
  - `Tuple(Vec<MirType>)` — `tuple<T0, T1, ...>`.
  - `Function { params: Vec<MirType>, results: Vec<MirType> }`.
  - `Memref { shape: Vec<Option<u64>>, elem: Box<MirType> }` — `None` = dynamic dimension.
  - `Vec(u32, FloatWidth)` — fixed-size float vector added at T11-D31 (`vector<NxfM>`).
  - `Opaque(String)` — pass-through name for types not structurally categorized.
  - `impl fmt::Display` — produces canonical MLIR textual form for all variants.

- **`IntWidth`**: `I1, I8, I16, I32, I64, Index`.
  - `pub const fn as_str(self) -> &'static str` — returns canonical MLIR string.

- **`FloatWidth`**: `F16, Bf16, F32, F64`.
  - `pub const fn as_str(self) -> &'static str` — returns canonical MLIR string.

**Key design notes:**
- Signedness is unified to signless (`i32`) at stage-0; signed/unsigned is encoded as an attribute when needed, matching MLIR convention.
- `MirType::Vec` was added specifically for T11-D31 sphere-SDF operations (`length(p) - r`). The `FloatWidth` element field enables type-correct AD walks.
- `Opaque` is the escape hatch for user-defined nominal types and unresolved references. It is heavily used in body_lower for struct types, closure types, effect result types, and unresolved paths.

**Tests (`#[cfg(test)]` mod tests, 16 tests):** Covers `ValueId` display, `IntWidth`/`FloatWidth` name strings, `MirType::Display` for all structural variants including function multi-result, dynamic memref, and all four `Vec` variants. Tests `MirValue` construction and `Vec` equality semantics.

---

### 3.3 `src/op.rs` (352 lines)

**Purpose:** Defines the 26 `cssl.*` custom dialect operations enumerated in `specs/15_MLIR.csl` § CSSL-DIALECT OPS, plus the `Std` pass-through, their category grouping, and expected arity signatures.

**Enums:**

- **`CsslOp`**: `Copy, PartialEq, Eq, Hash` dialect-op variant.
  - Variants (27 total, 26 cssl.* + Std):
    - AD/Jet: `DiffPrimal`, `DiffFwd`, `DiffBwd`, `JetConstruct`, `JetProject`.
    - Effects: `EffectPerform`, `EffectHandle`.
    - Regions/Handles: `RegionEnter`, `RegionExit`, `HandlePack`, `HandleUnpack`, `HandleCheck`.
    - Staging/Macros (F4): `StagedSplice`, `StagedQuote`, `StagedRun`, `MacroExpand`.
    - IFC/Verify (F5): `IfcLabel`, `IfcDeclassify`, `VerifyAssert`.
    - Engine: `SdfMarch`, `SdfNormal`.
    - GPU: `GpuBarrier`.
    - XMX: `XmxCoopMatmul`.
    - RT: `RtTraceRay`, `RtIntersect`.
    - Telemetry: `TelemetryProbe`.
    - `Std` — standard-dialect passthrough.
  - `pub const fn name(self) -> &'static str` — canonical source-form name (e.g. `"cssl.diff.primal"`). Returns `"cssl.std"` for `Std`.
  - `pub const fn category(self) -> OpCategory` — groups into `OpCategory`.
  - `pub const fn signature(self) -> OpSignature` — encodes expected operand/result count (`None` = variadic).
  - `pub const ALL_CSSL: [Self; 26]` — exhaustive compile-time array of all non-Std ops.
  - `impl fmt::Display` — delegates to `name()`.

- **`OpCategory`**: `AutoDiff, Jet, Effect, Region, Handle, Staged, Macro, Ifc, Verify, Sdf, Gpu, Xmx, Rt, Telemetry, Std`.

- **`OpSignature`**: struct with `operands: Option<usize>`, `results: Option<usize>`. `None` = variadic.

**Key design notes:**
- `Std` is a critical escape hatch: all standard-dialect ops (`arith.*`, `scf.*`, `func.*`, `memref.*`, `vector.*`, etc.) are represented as `CsslOp::Std` with a free-form `name` string. This avoids building a schema for every MLIR standard op at stage-0.
- `ALL_CSSL` enables O(N) tests that prove name uniqueness, prefix compliance, category coverage, and count.

**Tests (9 tests):** Unique names, `cssl.` prefix on all, count is exactly 26, all `ALL_CSSL` ops map to non-Std categories, specific signature spot-checks (`HandlePack` 2→1, `RegionEnter` 0→1, `TelemetryProbe` 0→0, `Std` free-form), `Display` matches name.

---

### 3.4 `src/block.rs` (223 lines)

**Purpose:** Defines `MirBlock`, `MirRegion`, and `MirOp` — the structured MLIR building blocks.

**Structs:**

- **`MirBlock`**: basic block.
  - Fields: `label: String`, `args: Vec<MirValue>` (block parameters), `ops: Vec<MirOp>`.
  - `pub fn new(label: impl Into<String>) -> Self` — empty block.
  - `pub fn entry(args: Vec<MirValue>) -> Self` — the canonical `"entry"` block.
  - `pub fn push(&mut self, op: MirOp)` — append an op.

- **`MirRegion`**: sequence of blocks (MLIR region).
  - Fields: `blocks: Vec<MirBlock>`.
  - `pub fn new() -> Self` — empty.
  - `pub fn with_entry(args: Vec<MirValue>) -> Self` — single entry-block with given args.
  - `pub fn push(&mut self, block: MirBlock)`.
  - `pub fn entry(&self) -> Option<&MirBlock>` — first block.
  - `pub fn entry_mut(&mut self) -> Option<&mut MirBlock>`.

- **`MirOp`**: one dialect operation.
  - Fields: `op: CsslOp`, `name: String`, `operands: Vec<ValueId>`, `results: Vec<MirValue>`, `attributes: Vec<(String, String)>`, `regions: Vec<MirRegion>`.
  - Attributes are `Vec<(String, String)>` at stage-0 (structured attribute types deferred to T6-phase-2).
  - `pub fn new(op: CsslOp) -> Self` — from dialect variant; name auto-set from `op.name()`.
  - `pub fn std(name: impl Into<String>) -> Self` — constructs a `Std` op with caller-supplied name.
  - Builder methods (all `#[must_use]`): `with_operand`, `with_result`, `with_attribute`, `with_region` — all consume self and return `Self`, enabling chaining.

**Key invariants:**
- Structured-by-construction: control flow must nest regions (scf.if branches, loop bodies, effect handler bodies, lambda bodies) rather than using unstructured branches. Stage-0 enforces this by building nested `MirRegion`s in the lowering pass.
- Stage-0 operates with exactly one entry block per function. Multi-block regions are reserved for future control-flow passes.

**Tests (5 tests):** Block build/push, region-with-entry shape, builder chain on `MirOp`, std op name, nested block inside a region.

---

### 3.5 `src/func.rs` (171 lines)

**Purpose:** Defines `MirFunc` and `MirModule` — the top-level MIR containers.

**Structs:**

- **`MirFunc`**: one function in the module.
  - Fields:
    - `name: String` — source-form name (no `@` prefix; printer adds it).
    - `params: Vec<MirType>` — flat scalarized param types (vec params are expanded here).
    - `results: Vec<MirType>` — return types.
    - `effect_row: Option<String>` — stringified effect row (e.g. `"{GPU, NoAlloc}"`). Structured form is T6-phase-2.
    - `cap: Option<String>` — cap annotation from return-type.
    - `ifc_label: Option<String>` — IFC label attribute.
    - `attributes: Vec<(String, String)>` — additional flags.
    - `is_generic: bool` — T11-D43: set true when the HIR fn had non-empty `generics.params`. Generic fns are removed after monomorphization by `drop_unspecialized_generic_fns`.
    - `body: MirRegion` — the fn body; starts with an entry block whose args match params.
    - `next_value_id: u32` — monotonic counter for fresh SSA IDs within this fn.
  - `pub fn new(name, params, results) -> Self` — allocates entry-block args matching params, sets `next_value_id = params.len()`.
  - `pub fn fresh_value_id(&mut self) -> ValueId` — allocates next SSA ID (saturating add).
  - `pub fn is_signature_only(&self) -> bool` — true if all blocks have empty op lists.
  - `pub fn push_op(&mut self, op: MirOp)` — appends to entry block.

- **`MirModule`**: top-level container.
  - Fields: `name: Option<String>`, `funcs: Vec<MirFunc>`, `attributes: Vec<(String, String)>`.
  - `pub fn new() -> Self` / `pub fn with_name(name) -> Self`.
  - `pub fn push_func(&mut self, f: MirFunc)`.
  - `pub fn find_func(&self, name: &str) -> Option<&MirFunc>` — linear scan.

**Key design notes:**
- `is_generic` is critical to the monomorphization cleanup. It is set by `lower_function_signature` (from `HirFn.generics.params.is_empty()`) and is `false` on all specialized fns produced by `specialize_generic_fn` (which empties the HIR fn's generics before lowering).
- `next_value_id` starts at `params.len()` so entry-block param args occupy IDs `0..params.len()`.

**Tests (4 tests):** Entry args populated from params, `fresh_value_id` increments, `is_signature_only` for empty body, `find_func` by name.

---

### 3.6 `src/lower.rs` (292 lines)

**Purpose:** Skeleton HIR→MIR signature-level lowering. For each `HirFn`/`HirImpl`/`HirInterface`/`HirEffect`/`HirHandler` item in a `HirModule`, emits a `MirFunc` with correct name, flat scalarized params, results, and string-form effect-row/cap attributes. Bodies are empty (entry block with no ops) — full body lowering is in `body_lower.rs`.

**Structs:**

- **`LowerCtx<'a>`**: per-module lowering context.
  - Fields: `interner: &'a Interner`, `cap_map: Option<&'a CapMap>`.
  - `pub fn new(interner: &'a Interner) -> Self`.
  - `pub const fn with_cap_map(mut self, m: &'a CapMap) -> Self`.
  - `pub fn lower_type(&self, t: &HirType) -> MirType` — maps HIR types to MIR types. Recognizes primitive path names (`i8`, `i16`, `i32`, `u32`, `isize`, `usize`, `i64`, `u64`, `f16`, `bf16`, `f32`, `f64`, `bool`, `Handle`) and yields corresponding `MirType` variants. Paths of length > 1 become `Opaque(joined)`. Structural types (`Tuple`, `Function`, `Reference`, `Capability`, `Array`, `Slice`, `Refined`, `Infer`, `Error`) are lowered structurally. `Reference` peels through to inner type; `Refined` peels to base; `Infer` → `None`; `Error` → `Opaque("!cssl.error")`.
  - `fn format_effect_row(&self, row: &HirEffectRow) -> String` — stringifies effect annotations with optional row-tail variable. Output format: `{Effect1, Effect2 | tail}`.

**Public functions:**

- `pub fn lower_function_signature(ctx: &LowerCtx<'_>, f: &HirFn) -> MirFunc` — produces a signature-only `MirFunc`. Uses `body_lower::expand_fn_param_types` for vec scalarization (T11-D35 single source of truth between signature- and body-lowering). Sets `is_generic` from `f.generics.params.is_empty()`. Records effect-row and cap attributes.
- `pub fn lower_module_signatures(ctx: &LowerCtx<'_>, module: &HirModule) -> MirModule` — walks all items calling `lower_item_into`.

**Private functions:**

- `fn lower_item_into(ctx, item, mir)` — matches on `HirItem` variants: `Fn` → single fn, `Impl`/`Interface`/`Effect`/`Handler` → one fn per method/op, `Module` → recurses into nested items. Structs/enums/type-aliases/use/const items produce no MIR fns at stage-0.

**Tests (5 integration tests, using `cssl-lex` + `cssl-parse` + `cssl-hir`):**
- Empty module → empty MIR.
- `fn add(a: i32, b: i32) -> i32 { a + b }` → single MirFunc with 2 i32 params.
- Effect row formatted correctly.
- Module path preserved.
- Multiple fns in source order.
- `lower_function_signature` called directly.

**Notable code/spec divergence:** The `u32`/`usize`/`isize` types are silently mapped to `i32` (MLIR signless). This is a deliberate stage-0 simplification but may matter when 64-bit pointer sizes are needed. No comment explains the lossy mapping.

---

### 3.7 `src/body_lower.rs` (2,066 lines)

**Purpose:** Full HIR function body → MIR op sequence lowering. This is the largest file and the core of the lowering pass. It covers all 31 `HirExprKind` variants. Bodies are lowered into the entry-block of an existing `MirFunc`.

**Structs:**

- **`BodyLowerCtx<'a>`**: per-fn lowering context (carries state through the recursive expression walk).
  - Fields:
    - `interner: &'a Interner`.
    - `source: Option<&'a SourceFile>` — for literal text extraction.
    - `param_vars: HashMap<Symbol, (ValueId, MirType)>` — scalar param → SSA ID mapping.
    - `vec_param_vars: HashMap<Symbol, (Vec<ValueId>, u32, FloatWidth)>` — T11-D35: vec param → N lane IDs.
    - `next_value_id: u32`.
    - `ops: Vec<MirOp>` — accumulated output ops.
  - `pub fn new(interner) -> Self` — no source.
  - `pub fn with_source(interner, source) -> Self` — with source for literal extraction.
  - `pub fn fresh_value_id(&mut self) -> ValueId` — saturating increment.
  - `fn sub(&self) -> BodyLowerCtx<'a>` — creates a sub-context inheriting source + current next_value_id but with empty param_vars/vec_param_vars/ops. Used for nested regions (branches, loop bodies, lambda bodies, match arms, effect handler bodies).

**Public functions:**

- `pub fn lower_fn_body(interner, source, hir_fn, mir_fn)` — entry point. Populates `mir_fn.body.entry().ops`. Returns early if `hir_fn.body` is None. Builds param mappings (scalar vs vec), calls `lower_block`, emits `func.return`, installs ops into entry block.
- `pub fn hir_type_as_vec_lanes(interner, t) -> Option<(u32, FloatWidth)>` — T11-D35: recognizes `vec2`/`vec3`/`vec4` (with optional `<f32>` type arg) HIR types. Peels through `Refined` and `Reference` wrappers.
- `pub fn expand_fn_param_types(interner, t) -> Vec<MirType>` — T11-D35: single source of truth for vec scalarization used by both `lower.rs` and `body_lower.rs`. Vec params expand to N `Float(width)` entries; everything else yields one type.

**Private lowering functions (one per HirExprKind and helpers):**

- `fn lower_block(ctx, block) -> Option<(ValueId, MirType)>` — walks stmts, then trailing expr.
- `fn lower_stmt(ctx, stmt)` — handles `Let`, `Expr`, `Item` (item stmts skipped at stage-0).
- `fn lower_expr(ctx, expr) -> Option<(ValueId, MirType)>` — central dispatch over all 31 `HirExprKind` variants. Returns the SSA value + type of the result, or `None` for statements/void.
- `fn lower_for(ctx, iter, body, span) -> (ValueId, MirType)` — emits `scf.for` with body region, iter operand. Result type `MirType::None`.
- `fn lower_while(ctx, cond, body, span) -> (ValueId, MirType)` — emits `scf.while` with condition operand and body region.
- `fn lower_loop(ctx, body, span) -> (ValueId, MirType)` — emits `scf.loop` with body region.
- `fn lower_match(ctx, scrutinee, arms, span) -> (ValueId, MirType)` — emits `scf.match` with scrutinee operand and one nested region per arm. Arm regions each contain an `"arm"` block.
- `fn lower_field(ctx, obj, name, span) -> (ValueId, MirType)` — emits `cssl.field` with `field_name` attribute. Result type is `Opaque("!cssl.field.{name}")`.
- `fn lower_index(ctx, obj, index, span) -> (ValueId, MirType)` — emits `memref.load`.
- `fn lower_assign(ctx, op, lhs, rhs, span) -> (ValueId, MirType)` — emits compound-assign ops (`cssl.assign`, `cssl.assign_add`, `cssl.assign_sub`, `cssl.assign_mul`, `cssl.assign_div`, `cssl.assign_compound`).
- `fn lower_cast(ctx, inner, span) -> (ValueId, MirType)` — emits `arith.bitcast`. Result type `MirType::None` (no type-propagation at stage-0).
- `fn lower_tuple(ctx, elements, span) -> (ValueId, MirType)` — emits `cssl.tuple` with N operands and `arity` attribute.
- `fn lower_array(ctx, arr, span) -> (ValueId, MirType)` — handles `HirArrayExpr::List` (emits `cssl.array_list`) and `HirArrayExpr::Repeat` (emits `cssl.array_repeat`).
- `fn lower_struct_expr(ctx, path, fields, span) -> (ValueId, MirType)` — emits `cssl.struct` with `struct_name`, `field_count` attributes. Type is `Opaque("!cssl.struct.{struct_name}")`.
- `fn lower_pipeline(ctx, lhs, rhs, span) -> (ValueId, MirType)` — emits `cssl.pipeline` for the `|>` operator.
- `fn lower_try_default(ctx, inner, default, span) -> (ValueId, MirType)` — emits `cssl.try_default`.
- `fn lower_try(ctx, inner, span) -> (ValueId, MirType)` — emits `cssl.try`.
- `fn lower_range(ctx, lo, hi, inclusive, span) -> (ValueId, MirType)` — emits `cssl.range` or `cssl.range_inclusive`.
- `fn lower_lambda(ctx, params, return_ty, body, span) -> (ValueId, MirType)` — T6-phase-2c: emits `cssl.closure` with a nested body region. Seeds lambda param bindings in a sub-context. No closure-capture analysis (deferred to T6-phase-2d).
- `fn lower_perform(ctx, path, args, span) -> (ValueId, MirType)` — T6-phase-2c: emits `cssl.effect.perform` with `effect_path` and `arg_count` attributes.
- `fn lower_with(ctx, handler, body, span) -> (ValueId, MirType)` — T6-phase-2c: emits `cssl.effect.handle` with handler operand and body region.
- `fn lower_region(ctx, label, body, span) -> (ValueId, MirType)` — T6-phase-2c: emits `cssl.region.enter` with body region and `label` attribute. The pairing `cssl.region.exit` + arena-lifetime synthesis is a later MIR-to-MIR pass.
- `fn lower_compound(ctx, op, lhs, rhs, span) -> (ValueId, MirType)` — T6-phase-2c: emits `cssl.compound` with `compound_op` attribute encoding the CSLv3 morpheme code.
- `fn lower_section_ref(ctx, path, span) -> (ValueId, MirType)` — T6-phase-2c: emits `cssl.section_ref` with `section_path` attribute.
- `fn lower_literal(ctx, lit, span) -> (ValueId, MirType)` — emits `arith.constant`. Extracts real literal values from source text when `source` is threaded; falls back to `"stage0_int"`, `"stage0_float"`, `"stage0_str"`, `"stage0_char"` placeholders otherwise.
- `fn parse_int_literal(raw) -> Option<i64>` — handles `_` separators, `0x`/`0b`/`0o` prefixes, and trailing type suffixes (`i32`, `u64`, etc.).
- `fn parse_float_literal(raw) -> Option<f64>` — strips `_` separators and `f32`/`f64`/`f16`/`bf16` suffixes.
- `fn strip_int_type_suffix(raw) -> &str` — strips trailing int type suffixes.
- `fn strip_float_type_suffix(raw) -> &str` — strips trailing float type suffixes.
- `fn strip_string_quotes(raw) -> Option<&str>` — strips surrounding `"..."`. Escape sequences left as-is at stage-0.
- `fn strip_char_quotes(raw) -> Option<&str>` — strips surrounding `'...'`.
- `fn lower_path(ctx, segments, span) -> (ValueId, MirType)` — single-segment paths check `param_vars` and return the param's SSA ID/type directly (no op emitted). Multi-segment or unresolved paths emit `cssl.path_ref` with an `Opaque("!cssl.unresolved.{name}")` type.
- `fn lower_binary(ctx, op, lhs, rhs, span) -> Option<(ValueId, MirType)>` — maps `HirBinOp` to `arith.*` ops. Integer vs float dispatched from `lhs_ty`. Comparison ops return `MirType::Bool`. `Implies`/`Entails` map to `cssl.verify.assert` (unusual mapping; see notes).
- `fn lower_unary(ctx, op, operand, span) -> Option<(ValueId, MirType)>` — maps `HirUnOp` to `arith.*`/`cssl.*` ops. `Not` → `arith.xori`; `Neg` → `arith.negf` or `arith.subi_neg`; `Ref`/`RefMut`/`Deref` → `cssl.borrow`/`cssl.borrow_mut`/`cssl.deref`.
- `fn lower_call(ctx, callee, args, span, hir_id) -> Option<(ValueId, MirType)>` — path callees become `func.call @target`; non-path callees emit `cssl.call_indirect`. Special-cases `length`/`math.length` (delegates to `try_lower_vec_length_from_path`). Infers result types for known math intrinsics via `infer_intrinsic_result_type`. Stamps `hir_id` attribute for call-site rewriting (T11-D41).
- `fn try_lower_vec_length_from_path(ctx, arg, span) -> Option<(ValueId, MirType)>` — T11-D35: if the sole arg is a vec-param name, emits the `sqrt(Σ xᵢ²)` expansion: N `arith.mulf` ops + (N-1) `arith.addf` ops + one `func.call @sqrt`. Returns `None` if arg is not a vec-param (caller falls through to generic lowering).
- `fn infer_intrinsic_result_type(callee, operand_tys) -> Option<MirType>` — for a catalog of math intrinsic names (`min`, `max`, `abs`, `sqrt`, `sin`, `cos`, `exp`, `log`, `ln`, and variants), returns the first operand's type as result type.
- `fn lower_if(ctx, cond, then_branch, else_branch, span) -> Option<(ValueId, MirType)>` — emits `scf.if` with condition operand and two nested regions (then + optional else). Result type is `MirType::None` at stage-0.
- `fn lower_sub_region_from(ctx, block) -> MirRegion` — creates a sub-context, lowers the block into it, writes back `next_value_id`, and packages the ops into a single-block region. Preserves monotonic SSA IDs across nested regions.
- `fn emit_return(ctx, trailing, span)` — emits `func.return` with optional trailing-expr operand.
- `fn emit_unsupported(ctx, span, kind_name) -> (ValueId, MirType)` — escape-hatch emitting a `cssl.std` placeholder with `unsupported_kind` attribute. Used only for `Break`, `Continue`, and `Error` — all other 28 variants have dedicated lowerers.
- `fn extract_pattern_symbol(pat) -> Option<Symbol>` — extracts the binding name from `HirPatternKind::Binding`.
- `fn lower_hir_type_light(interner, t) -> MirType` — shallow HIR→MIR type mapping for use inside the body lowerer (mirrors `lower.rs`'s `LowerCtx::lower_type` but without access to the full context).
- `fn discriminant_name(kind) -> &'static str` — `#[allow(dead_code)]` debug helper mapping every `HirExprKind` variant to a name string.
- `fn _unused(_: MirValue)` — suppresses dead-code warning on `MirValue` in the module scope.

**Notable observations and issues:**

1. **`Implies`/`Entails` binary ops map to `"cssl.verify.assert"`** (`lower_binary`, line ~1136). This is semantically unexpected — logical implication `⇒` lowering to an assertion op rather than a boolean connective is a design choice that needs spec validation. The result type for these ops is `Bool` (from the comparison arm match), which is also questionable for an assertion that returns `()`.

2. **Type propagation gaps**: Many lowerers return `MirType::None` where precise types could be inferred. Specifically: `lower_if`, `lower_for`, `lower_while`, `lower_loop`, `lower_index`, `lower_cast`, `lower_pipeline`, `lower_range`, `lower_match`. This is documented as deferred to T6-phase-2d but means downstream AD passes and codegen must handle `None`-typed operands.

3. **`lower_stmt` for `Let` bindings** does not bind the let-pattern symbol to the resulting SSA ID. The value is lowered and discarded (`let _ = lower_expr(ctx, e)`). This means subsequent references to the let-bound name via `lower_path` will not resolve to `param_vars` and will instead emit `cssl.path_ref` with an unresolved-opaque type. Correct binding through let-statements requires either a separate symbol table (beyond `param_vars`) or propagating the let-bound symbol through the lowering context.

4. **`lower_path` only resolves function params** — it checks only `param_vars`. Local variable bindings from `let` statements are not tracked. This is a known stage-0 limitation; it is partially mitigated by the fact that the body lowerer does emit ops for the RHS of let-bindings (producing the value), and downstream tools rely on the SSA form rather than named resolution.

5. **Escape sequences in string literals** are left as-is at stage-0 (documented in `strip_string_quotes`). This is acceptable for the compiler pipeline but means string content in attribute values may contain raw `\n`, `\t`, etc.

**Tests (37 tests):** `hir_from` / `lower_one` / `lower_one_nosrc` integration helpers. Tests cover: empty body, int/float/bool literals, param reference, binary add (int and float), comparison returning bool, unary negation, call emitting `func.call`, if with two regions, explicit return, unsupported-variant placeholder, monotonic value IDs, signature unchanged by body lowering, while/for loops, field access, indexing, tuple, cast, assign, compound-assign, range, array literal, struct constructor, pipeline operator, match, all-discriminants smoke test, lambda with closure and body region, perform with effect path, with handler, region enter, section ref, literal value extraction (int, float, bool), and source-less fallback to `stage0_*` placeholder.

---

### 3.8 `src/monomorph.rs` (1,398 lines)

**Purpose:** Core generic specialization API — the "monomorphization quartet" low-level machinery. Provides `TypeSubst` plus `specialize_generic_fn`, `specialize_generic_struct`, `specialize_generic_enum`, and `specialize_generic_impl`. These are the building blocks; the auto-discovery walkers in `auto_monomorph.rs` call them.

**Structs:**

- **`TypeSubst`**: `HashMap<Symbol, HirType>` wrapper.
  - `pub fn new() -> Self`.
  - `pub fn bind(&mut self, name: Symbol, ty: HirType)`.
  - `pub fn get(&self, name: &Symbol) -> Option<&HirType>`.
  - `pub fn iter_sorted<'a>(&'a self, interner) -> impl Iterator<Item = (Symbol, &'a HirType)>` — sorted by resolved name for deterministic mangling.
  - `pub fn len(&self) -> usize` / `pub fn is_empty(&self) -> bool`.

**Public functions:**

- `pub fn substitute_hir_type(t, interner, subst) -> HirType` — recursively walks a `HirType` tree substituting any single-segment `Path` node whose name matches a key in `subst`. All other variants are traversed structurally. Preserves `span` and `id` of outer nodes (source-linked diagnostics).
- `pub fn mangle_specialization_name(base_name, interner, subst) -> String` — deterministic name mangling: `{fn_name}_{arg0}_{arg1}_...` sorted by param name. Fragments: primitives → lowercase name; nominal paths → last segment lowercase; function types → `"fn"`; tuple → `"tup{elems...}"`; array → `"arr"`; slice → `"slice"`; reference → `"ref{inner}"`; refined → pass through base; capability → pass through inner; infer → `"infer"`; error → `"err"`.
- `pub fn specialize_generic_fn(interner, source, hir_fn, subst) -> MirFunc` — pipeline: (1) clone + substitute signature via `substitute_fn_signature`; (2) lower signature via `lower_function_signature`; (3) apply mangled name; (4) lower body via `lower_fn_body`. The specialized fn's HIR clone has empty generics, so it is concrete.
- `pub fn specialize_generic_struct(interner, hir_struct, subst) -> HirStruct` — T11-D45: clones struct, empties generics, substitutes field types via `substitute_struct_body`.
- `pub fn mangle_struct_specialization_name(hir_struct, interner, subst) -> String` — thin wrapper resolving struct name and calling `mangle_specialization_name`.
- `pub fn specialize_generic_enum(interner, hir_enum, subst) -> HirEnum` — T11-D47: clones enum, empties generics, substitutes each variant's body via `substitute_struct_body`.
- `pub fn mangle_enum_specialization_name(hir_enum, interner, subst) -> String` — parallel to struct version.
- `pub fn specialize_generic_impl(interner, source, hir_impl, subst) -> Vec<MirFunc>` — T11-D49: for each method in the impl, substitutes param types + return type, calls `lower_function_signature` + `lower_fn_body`, and names the result `{self_mangle}__{fn_name}`. Returns one `MirFunc` per method.
- `pub fn hir_primitive_type(name, interner) -> HirType` — convenience for building `HirType::Path` primitives in tests.
- `pub fn primitive_hir_to_mir(t, interner) -> Option<MirType>` — maps common primitive `HirType` path names to `MirType`. Returns `None` for non-primitive paths (including generic params like `T`).

**Private functions:**

- `fn substitute_kind(k, interner, subst) -> HirTypeKind` — the structural recursion engine. All `HirTypeKind` variants handled.
- `fn type_mangle_fragment(t, interner) -> String` — per-type mangle fragment.
- `fn substitute_fn_signature(hir_fn, interner, subst) -> HirFn` — clones the fn, empties generics, substitutes param + return types.
- `fn substitute_struct_body(body, interner, subst) -> HirStructBody` — handles Unit/Tuple/Named.
- `fn substitute_field_decl(f, interner, subst) -> HirFieldDecl` — substitutes field's ty.
- `fn substitute_enum_variant(v, interner, subst) -> HirEnumVariant` — substitutes variant body.
- `fn mangle_self_ty(self_ty, interner, subst) -> String` — extracts and substitutes the self-type name for impl name mangling (Path form).
- `fn self_ty_fragment(t, interner) -> String` — renders a post-substitution type to a fragment string.

**Tests (37 tests — integration, using `cssl-lex` + `cssl-parse` + `cssl-hir`):** TypeSubst basics (new, bind, get, iter_sorted determinism), substitute_hir_type (single-segment substitution, pass-through for non-generic paths), name mangling (no subst, one subst, two substs sorted), specialize_generic_fn end-to-end (id→i32, id→f32, two-param, generics stripped, non-generic fn is identity, body lowers cleanly), primitive_hir_to_mir (canonical names, returns None for T), T11-D45 struct tests (named struct field substitution, tuple struct, unit struct, empty generics after specialization, nested type arg `Box<T>` → `Box<i32>`), T11-D47 enum tests (Option-like, named variant, non-generic pass-through, empty generics, mangle name, nested type args `Tree<T> { Node(Box<T>) }`), T11-D49 impl tests (one fn per method, self-mangle prepended, two type params, param types substituted, empty impl, non-generic impl → unsuffixed names).

---

### 3.9 `src/auto_monomorph.rs` (1,857 lines)

**Purpose:** Auto-discovery walkers that scan the HIR module and automatically invoke the core monomorphization API from `monomorph.rs`. Implements the full monomorphization quartet: fn call-site discovery, struct type-annotation discovery, enum type-annotation discovery, and impl self-type discovery.

**Report structs:**

- **`AutoMonomorphReport`**: result of `auto_monomorphize`.
  - Fields: `specializations: Vec<MirFunc>`, `call_site_names: HashMap<HirId, String>`, `generic_fn_count: u32`, `call_site_count: u32`, `specialization_count: u32`.
  - `pub fn summary(&self) -> String` — diagnostic summary string.
  - `pub fn is_empty(&self) -> bool`.

- **`AutoStructReport`**: result of `auto_monomorphize_structs`.
  - Fields: `specializations: Vec<HirStruct>`, `ref_to_mangled: HashMap<String, String>`, `generic_struct_count: u32`, `ref_count: u32`, `specialization_count: u32`.
  - `pub fn summary(&self) -> String` / `pub fn is_empty(&self) -> bool`.

- **`AutoEnumReport`**: result of `auto_monomorphize_enums`.
  - Fields: `specializations: Vec<HirEnum>`, `ref_to_mangled: HashMap<String, String>`, `generic_enum_count: u32`, `ref_count: u32`, `specialization_count: u32`.
  - `pub fn summary(&self) -> String` / `pub fn is_empty(&self) -> bool`.

- **`AutoImplReport`**: result of `auto_monomorphize_impls`.
  - Fields: `specializations: Vec<MirFunc>`, `generic_impl_count: u32`, `ref_count: u32`, `unique_spec_count: u32`.
  - `pub fn summary(&self) -> String` / `pub fn is_empty(&self) -> bool`.

**Public functions:**

- `pub fn auto_monomorphize(module, interner, source) -> AutoMonomorphReport` — T11-D40: three-phase algorithm: (1) index generic top-level fn decls by name; (2) walk every fn body collecting turbofish `Call` nodes with non-empty `type_args` on single-segment path callees that match a known generic; (3) deduplicate by mangled name, invoke `specialize_generic_fn` per unique tuple, populate `call_site_names` for the rewriter.
- `pub fn drop_unspecialized_generic_fns(module: &mut MirModule) -> u32` — T11-D43: `module.funcs.retain(|f| !f.is_generic)`. Returns count dropped.
- `pub fn rewrite_generic_call_sites(module: &mut MirModule, call_site_names) -> u32` — T11-D41: walks every `func.call` op in every block of every fn, finds ops with an `hir_id` attribute, looks up in `call_site_names`, rewrites `callee` attribute to mangled name. Returns rewrite count.
- `pub fn auto_monomorphize_structs(module, interner) -> AutoStructReport` — T11-D46: indexes generic struct decls; walks fn param/return types + struct field types collecting `HirTypeKind::Path` refs with non-empty `type_args` matching a known generic struct; deduplicates; invokes `specialize_generic_struct`. Handles nested refs (`Outer<Inner<i32>>`) via recursive type-walker.
- `pub fn auto_monomorphize_enums(module, interner) -> AutoEnumReport` — T11-D48: parallel to struct walker but for generic enums. Also scans enum-variant field types.
- `pub fn auto_monomorphize_impls(module, interner, source) -> AutoImplReport` — T11-D50: indexes generic impls by self-type name; scans fn signatures + struct/enum body fields for matching `Path` refs; deduplicates by `{self_name}_{mangle_key}`; invokes `specialize_generic_impl`.

**Private functions:**

- `fn collect_turbofish_calls(block, interner, fn_index, out)` — walks `HirBlock` stmts + trailing expr, calling `collect_in_expr` for each.
- `fn collect_in_expr(expr, interner, fn_index, out)` — `#[allow(clippy::too_many_lines)]` — exhaustive match over `HirExprKind` (31 variants). Turbofish sites detected at `Call` nodes with non-empty `type_args` + single-segment callee + callee in `fn_index`. Recursion into all structural variants. Leaf variants and some complex forms (Lambda, Perform, With, Region, Compound, SectionRef, Struct) treated as terminal with `_ => {}`.
- `fn mangle_key(subst, interner) -> String` — internal stable-order key for deduplication (mirrors mangle fragment logic).
- `fn collect_generic_struct_refs(t, struct_index, out)` — recursive type walker for struct ref collection.
- `fn walk_struct_fields(body, struct_index, out)` — walks `HirStructBody` fields for struct refs.
- `fn collect_generic_enum_refs(t, enum_index, out)` — parallel to struct walker for enums.
- `fn walk_struct_fields_for_enum_refs(body, enum_index, out)` — walks body fields for enum refs.
- `fn collect_impl_self_ty_refs(t, impl_index, out)` — recursive type walker for impl self-type refs.
- `fn walk_body_for_impl_refs(body, impl_index, out)` — walks body fields for impl refs.

**Notable issues:**

1. **`collect_in_expr` uses `_ => {}` for Lambda, Perform, With, Region, Compound, SectionRef, Struct** — these are marked as "leaf + opaque" but some of them (especially `Lambda` which has a body block, and `With` which has a handler body) could contain turbofish call sites that are currently invisible to the auto-discovery walker. This means generic calls inside closures or effect-handler bodies will be missed. The comment says "stage-0 doesn't need them for generic call discovery" which is a deliberate scope limitation but is worth flagging.

2. **Multi-segment path callees not supported** — only `id::<i32>(5)` is detected, not `mod::id::<i32>(5)`. Documented in the module header as out of scope.

3. **No inference for non-turbofish calls** — `id(5)` (without `::<i32>`) is not detected. Requires type inference, deferred. The test `call_without_turbofish_not_captured_even_if_callee_generic` documents this explicitly.

4. **Struct/enum walker only scans fn signatures and type-annotations in struct/enum fields** — expression-level struct constructor expressions (`Pair { first: 1, second: 2.0 }`) in fn bodies are not scanned. Documented in headers as deferred.

**Tests (37 tests):** Organized into four groups corresponding to the four walkers. Fn walker: empty module, non-generic fn, indexed-but-not-called, single turbofish, two distinct types, same type twice deduplicated, two generic fns, multi-type-arg, nested call in binary, non-turbofish not captured, summary shape, call_site_names populated. Call-site rewriter (T11-D41): `rewrite_updates_callee_attr_to_mangled_name`, non-generic calls untouched, multiple call sites in one fn, empty map. `drop_unspecialized_generic_fns` (T11-D43): drops generic keeps concrete, zero drops on empty, is_generic set correctly, specialized fn has is_generic=false, specialized fn has correct signature. Struct walker (T11-D46): empty, ignores non-generic, indexed but no refs, fn param type triggers, return type triggers, two distinct refs, same dedup, nested refs both specializations, struct field type scanned, arity mismatch skipped, summary shape. Enum walker (T11-D48): similar set, plus enum-variant-field trigger. Impl walker (T11-D50): empty, ignores non-generic, single ref specializes all methods, two distinct type args, dedup same refs, arity mismatch, summary shape.

---

### 3.10 `src/pipeline.rs` (539 lines)

**Purpose:** MIR pass pipeline driver. Defines the `MirPass` trait, `PassPipeline`, `PassResult`/`PassDiagnostic`/`PassSeverity`, and six stock passes (one real, five stubs).

**Enums:**

- **`PassSeverity`**: `Info`, `Warning`, `Error` (Copy).
  - `pub const fn as_str(self) -> &'static str`.

**Structs:**

- **`PassDiagnostic`**: `severity: PassSeverity`, `code: String`, `message: String`.
  - `pub fn info(code, message) -> Self` / `pub fn warning(...)` / `pub fn error(...)` — constructors.

- **`PassResult`**: `name: String`, `changed: bool`, `diagnostics: Vec<PassDiagnostic>`.
  - `pub fn has_errors(&self) -> bool` — any diagnostic with severity `Error`.
  - `pub fn count_by(&self, sev: PassSeverity) -> usize`.

- **`PassPipeline`**: `passes: Vec<Box<dyn MirPass>>` (private field).
  - `impl Debug` — shows pass names.
  - `pub fn new() -> Self`.
  - `pub fn canonical() -> Self` — builds the standard stage-0 pipeline in spec-order: Monomorphization, AD-transform, IFC-lowering, SMT-discharge-queue, telemetry-probe-insert, structured-CFG-validator.
  - `pub fn push(&mut self, pass: Box<dyn MirPass>)`.
  - `pub fn len(&self) -> usize` / `pub fn is_empty(&self) -> bool`.
  - `pub fn names(&self) -> impl Iterator<Item = &'static str> + '_`.
  - `pub fn run_all(&self, module: &mut MirModule) -> Vec<PassResult>` — runs passes in order; halts on first pass that returns `has_errors() == true`. Returns vec of results for completed passes only.

**Trait:**

- **`MirPass`**: `fn name(&self) -> &'static str` + `fn run(&self, module: &mut MirModule) -> PassResult`.

**Stock passes:**

- **`StructuredCfgValidator`** (Copy, Default, real pass): validates every fn's region has at least one block (emits `CFG0001` error if not). Recursively validates nested sub-regions within op regions.
  - `fn validate_region(region, fn_name, out)` — private recursive helper.
- **`MonomorphizationPass`** (Copy, Default, stub): emits info diagnostic `MONO0000`. Does not modify module.
- **`AdTransformPass`** (Copy, Default, stub): emits info diagnostic `AD0000`. Placeholder for `cssl_autodiff::DiffRuleTable` integration.
- **`IfcLoweringPass`** (Copy, Default, stub): emits info diagnostic `IFC0000`. Placeholder for `cssl.ifc.label`/`cssl.ifc.declassify` emission.
- **`SmtDischargeQueuePass`** (Copy, Default, stub): emits info diagnostic `SMT0000`. Placeholder for `cssl.verify.assert` + SMT obligation queueing.
- **`TelemetryProbeInsertPass`** (Copy, Default, stub): emits info diagnostic `TEL0000`. Placeholder for scope-gated `cssl.telemetry.probe` emission.

**Notable issues:**
- **Pass ordering** specified in `canonical()` has Monomorphization before AD-transform, which is correct per spec. However, `MonomorphizationPass` is a stub — it does not call `auto_monomorphize`. The real monomorphization must be run externally (by the caller) before invoking the pipeline. This split is undocumented at the call site.
- **`validate_region` only checks emptiness**, not proper termination. A block with no `func.return`/`scf.yield` at the end is not flagged. Full terminator validation is T6-phase-2b work.

**Tests (14 tests):** Severity names, diagnostic builder shapes, `has_errors`, `count_by`, empty pipeline, canonical pipeline shape, canonical runs all 6 on empty module, stub passes emit exactly one info diagnostic each with stable codes, `StructuredCfgValidator` passes on well-formed fn, flags empty region with `CFG0001`, pipeline halts on error (only 1 result when validator errors), debug format.

---

### 3.11 `src/print.rs` (341 lines)

**Purpose:** MLIR textual-format pretty-printer. Produces valid MLIR source text for the subset of ops emitted by stage-0 body lowering. Suitable for `--emit-mlir` dumps and validation with downstream tools.

**Structs:**

- **`MlirPrinter`**: `out: String` (pub), `indent: usize` (private).
  - `pub fn new() -> Self`.
  - `pub fn into_string(self) -> String`.
  - `fn push_indent(&mut self)` — writes `self.indent` spaces.
  - `fn nl(&mut self)` — writes newline.
  - `fn write_module(&mut self, module: &MirModule)` — emits `module @name {` / `module {`, recurses into fns, closes with `}`.
  - `fn write_func(&mut self, f: &MirFunc)` — emits `func.func @name(arg0: T0, ...) -> R attributes { effect_row = "...", cap = "...", ... } { ... }`. Entry-block args already in the fn header; nested blocks get their own headers.
  - `fn write_region(&mut self, region, skip_entry_args)` — iterates blocks. First block skips its block-header when `skip_entry_args = true` (fn-level entry block).
  - `fn write_block_header(&mut self, block)` — emits `^label(arg0: T0, ...):`.
  - `fn write_op(&mut self, op)` — emits `%r0, %r1 = op_name %v0, %v1 { attr0 = "val0" } ({ region_blocks }) : () -> result_type`.

**Public function:**

- `pub fn print_module(module: &MirModule) -> String` — convenience entry that creates a printer, calls `write_module`, returns the accumulated string.

**Notable issue:** The `write_op` method emits the operand-type list as an empty `()` regardless of actual operand types (`// Operand types are not tracked in operand-only form ; printer records result-types and an indeterminate operand-type list`). This produces technically invalid MLIR type annotations that `mlir-opt` would reject. Full type-elaboration requires propagating operand types through the lowering walk, which is T6-phase-2 work.

**Tests (6 tests):** Empty module, named module, fn signature with two params, fn with telemetry probe op, fn with effect_row and cap attributes, op with results and operands (HandlePack).

---

## 4. Crate Notes

### Test coverage

Test coverage is thorough for the crate's stated scope. Every module has inline `#[cfg(test)]` blocks. The integration tests in `lower.rs`, `monomorph.rs`, `auto_monomorph.rs`, and `body_lower.rs` use the full `cssl-lex + cssl-parse + cssl-hir` pipeline to test end-to-end lowering from source strings, which gives strong confidence in the full HIR→MIR path.

Estimated test count across all modules: ~120 unit/integration tests.

### Incomplete / stubbed items

1. **Five of six pipeline passes are stubs** — `MonomorphizationPass`, `AdTransformPass`, `IfcLoweringPass`, `SmtDischargeQueuePass`, `TelemetryProbeInsertPass` all return info-only diagnostics and leave the module unchanged. These map directly to the F1–F5 language features (AD, effects, IFC, refinement types, observability).

2. **Type propagation is absent in most body-lowering paths** — `MirType::None` is returned for if, for, while, loop, index, cast, pipeline, and range lowerers. Downstream JIT and AD passes must tolerate opaque/None-typed intermediate values.

3. **Local variable bindings are not tracked** — let-statement RHS values are lowered but the bound symbol is not entered into a symbol table. References to let-bound names in subsequent code emit unresolved `cssl.path_ref` ops rather than the let-bound SSA value.

4. **Closure-capture analysis is absent** — lambda bodies lower with only parameter bindings visible. Any reference to outer-scope variables from inside a lambda will produce unresolved path refs.

5. **`cssl.region.exit` pairing** is not emitted — `lower_region` only emits `cssl.region.enter`. The arena-lifetime synthesis (matching enter with exit) is documented as a later MIR-to-MIR pass.

6. **Structured CFG validator only checks emptiness**, not proper termination or block argument consistency.

7. **Multi-segment path callees and non-turbofish generic calls** are not discovered by `auto_monomorphize`.

8. **Lambda/with/region bodies are not scanned** for turbofish call sites in `collect_in_expr` (`_ => {}` arm).

9. **MLIR output type annotations are incomplete** — operand types in the `write_op` printer are always empty `()`. Full round-trip parity with `mlir-opt` requires type elaboration.

### Spec divergences

- **`Implies`/`Entails` map to `"cssl.verify.assert"`** in `lower_binary`. This is not documented in `specs/02_IR.csl` or `specs/15_MLIR.csl`. Logical implication should arguably produce a `bool` result from a logic operation, not an assertion side-effect. This should be reviewed against the spec.

- **`u32`/`isize`/`usize` silently map to `MirType::Int(I32)`** in both `lower.rs` and `body_lower.rs`. On 64-bit targets `usize` is 64 bits. The truncation is unacknowledged in comments or tests.

- **`MirFunc` carries a string-form effect row** rather than a structured `HirEffectRow` or MLIR attribute type. This means downstream passes cannot introspect the effect row without re-parsing the string. Structured attribute types are documented as T6-phase-2 work in `func.rs`.

- **Monomorphization is split between `pipeline.rs`'s stub `MonomorphizationPass` and the external `auto_monomorphize` function** — callers must know to run the latter before the former. This is not documented at call sites and could lead to the pass pipeline being run on an unmonomorphized module.

### Surprises / notable positives

- The **monomorphization quartet** (D38/D45/D47/D49 specs + D40/D46/D48/D50 walkers) is the most substantial and mature piece of the crate. The deduplication logic (mangled-name sets + `iter_sorted` determinism) is well-designed. The `hir_id` stamping on `func.call` ops for call-site rewriting (T11-D41) is a clean integration point.

- The **vec scalarization** (T11-D35, `expand_fn_param_types` + `try_lower_vec_length_from_path`) is a complete, working implementation of vector-to-scalar lowering with a correct `sqrt(Σ xᵢ²)` expansion that the AD walker can differentiate.

- The **sub-context pattern** (`BodyLowerCtx::sub()`) is clean: sub-contexts inherit source and monotonic value IDs but have fresh param/op state. The `next_value_id` write-back after each sub-context ensures global SSA monotonicity across nested regions.

- The crate uses **`#![forbid(unsafe_code)]`** throughout and has no unsafe blocks.

- All 31 `HirExprKind` variants are handled — no `unreachable!()` or `todo!()` in `lower_expr`. The two remaining `emit_unsupported` calls (`Break`, `Continue`) are semantically correct stubs for constructs that need break-target synthesis.

- The `StructuredCfgValidator` is the **only real pass in the pipeline** and it does meaningful work: it recursively validates all nested regions, not just top-level fn bodies. This catches malformed nested ops emitted by buggy lowerers.
