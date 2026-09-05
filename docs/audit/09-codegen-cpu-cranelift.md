# Audit: `cssl-cgen-cpu-cranelift`

**Auditor:** Claude Sonnet 4.6  
**Date:** 2026-05-14  
**Spec references:** `specs/07_CODEGEN.csl` § CPU BACKEND, `specs/14_BACKEND.csl`  
**Crate path:** `compiler-rs/crates/cssl-cgen-cpu-cranelift/`

---

## 1. Crate Overview

`cssl-cgen-cpu-cranelift` is the stage-0 CPU code-generation backend for the CSSLv3 / Sigil compiler. Its position in the pipeline is:

```
MirModule (cssl-mir)
    → emit_module()         [emit.rs]  text-CLIF artifact (stage-0 inspectable output)
    → JitModule::compile()  [jit.rs]   real Cranelift IR construction via FunctionBuilder
    → JitModule::finalize() [jit.rs]   cranelift_jit::JITModule::finalize_definitions()
    → JitFn::call_*()       [jit.rs]   fn-pointer cast → in-process machine-code execution
```

The crate has two distinct output modes that coexist:

1. **Text-CLIF emitter** (`emit.rs` + `lower.rs`): Produces a human-readable, CLIF-flavoured text artifact (one `function %name(…) -> … { … }` block per MIR function). This is stage-0 "phase-1" — it exists to be diffable and inspectable without requiring the full Cranelift build chain at scaffold time. The text uses proper CLIF opcode names but is rendered via string formatting, not via Cranelift's IR types.

2. **Real JIT engine** (`jit.rs`): Stage-0.5. Uses `cranelift-frontend::FunctionBuilder` to construct real Cranelift IR, then `cranelift-jit::JITModule` to JIT-compile and link the resulting machine code into the host process. After `finalize()`, compiled functions are called via `unsafe` fn-pointer casts. This is described in the crate as "the stage-0.5 bridge to stage-1 self-host" — it is the mechanism by which CSSLv3 programs first execute.

The supporting modules (`types.rs`, `abi.rs`, `feature.rs`, `target.rs`) define the type system mapping, calling-convention enumerations, CPU feature flags, and target-profile bundles respectively. They are shared infrastructure used by both output modes.

**Maturity:** The text-CLIF emitter is complete for scalars. The JIT engine is substantially functional: it handles integer and float arithmetic, comparisons, conditional select, constants, returns, inter-fn calls, libm transcendentals (sin/cos/exp/log via extern linkage), and multi-result functions via an out-parameter ABI. Control flow (`scf.if`, `scf.for`), memref load/store, and SIMD/vector lowering are explicitly deferred to T11-D22+.

**No LLVM.** Cranelift is the sole code-generation backend. This is intentional and documented in both the crate `lib.rs` and `specs/14_BACKEND.csl`.

---

## 2. Crate Metadata

| Property | Value |
|---|---|
| Crate name | `cssl-cgen-cpu-cranelift` |
| Path | `compiler-rs/crates/cssl-cgen-cpu-cranelift/` |
| Description | "CSSLv3 stage0 — Cranelift-based CPU codegen (stage0 throwaway)" |
| Version | workspace-inherited |
| Edition | workspace-inherited |

### Cargo.toml Dependencies

| Dependency | Source | Role |
|---|---|---|
| `cssl-mir` | `path = "../cssl-mir"` | Provides `MirModule`, `MirFunc`, `MirOp`, `MirType`, `ValueId` — the input IR |
| `thiserror` | workspace | Derives `Error` on the two error enums |
| `cranelift-codegen` | workspace | Core Cranelift IR types, `FunctionBuilder` output types, `Context`, `settings::builder` |
| `cranelift-frontend` | workspace | `FunctionBuilder`, `FunctionBuilderContext` — the high-level IR construction API |
| `cranelift-module` | workspace | `Module`, `FuncId`, `Linkage`, `default_libcall_names` |
| `cranelift-jit` | workspace | `JITBuilder`, `JITModule` — in-process executable memory management |
| `cranelift-native` | workspace | `cranelift_native::builder()` — auto-detects host ISA for the JIT |

All five cranelift crates are listed as version `0.115` in the workspace (implied by "Cranelift 0.115 directly" in the task description). No dependency on `cranelift-object` (AoT object-file emission is T11-phase-2 deferred).

### Source Files and Line Counts

| File | Lines (approx.) | Role |
|---|---|---|
| `src/lib.rs` | 64 | Crate root: module declarations, pub re-exports, `STAGE0_SCAFFOLD` const, version test |
| `src/types.rs` | 175 | MIR type → CLIF type mapping (`ClifType` enum + `clif_type_for`) |
| `src/abi.rs` | 119 | Calling-convention (`Abi`) + object-format (`ObjectFormat`) enumerations |
| `src/feature.rs` | 272 | SIMD tier (`SimdTier`) + CPU feature flags (`CpuFeature`, `CpuFeatureSet`) |
| `src/target.rs` | 271 | µarch enum (`CpuTarget`), debug-format (`DebugFormat`), profile bundle (`CpuTargetProfile`) |
| `src/lower.rs` | 351 | Text-CLIF MIR-op lowering (`ClifInsn`, `lower_op`, helpers, formatting utilities) |
| `src/emit.rs` | 401 | Text-CLIF module emitter (`emit_module`, `EmittedArtifact`, `CpuCodegenError`) |
| `src/jit.rs` | 2199 | Real JIT engine (`JitModule`, `JitFn`, all op lowering helpers, 40+ tests) |

**Total source:** approximately 3,852 lines across 8 files.

---

## 3. Per-File Audit

Files are presented in dependency order (foundational types first, consumer modules last).

---

### 3.1 `src/lib.rs` — 64 lines

**Purpose:** Crate root. Declares all seven public modules, re-exports the primary public API surface, defines the `STAGE0_SCAFFOLD` version constant, and contains one trivial smoke test. Contains the stage-0 scope declaration comment (T10-phase-1 delivered, T10-phase-2 deferred items listed).

**Lint configuration:**
- `#![deny(unsafe_code)]` — Note: the file-level attribute reads `#![deny(unsafe_code)]` but `jit.rs` opens with `#![allow(unsafe_code)]`. Since `allow` in a child module overrides `deny` in the crate root for that module, this works correctly — unsafe is narrowly scoped to `jit.rs`. The comment at line 28–31 documents this decision explicitly (T11-D20).
- `#![deny(rustdoc::broken_intra_doc_links)]` and `#![deny(rustdoc::private_intra_doc_links)]` — doc link integrity enforced.
- `#![allow(clippy::match_same_arms)]` and `#![allow(clippy::module_name_repetitions)]` — pragmatic suppressions for the type-mapping match arms.

**Items:**

| Item | Signature | Description |
|---|---|---|
| `pub const STAGE0_SCAFFOLD: &str` | `= env!("CARGO_PKG_VERSION")` | Version string, exposed for scaffold verification by integration tests |
| `mod scaffold_tests` | `#[cfg(test)]` | Contains one test |
| `fn scaffold_version_present` | `#[test]` | Asserts `STAGE0_SCAFFOLD` is non-empty — trivial smoke test confirming the const is populated at compile time |

**Re-exports (via `pub use`):**
- `abi::{Abi, ObjectFormat}`
- `emit::{emit_module, CpuCodegenError, EmittedArtifact}`
- `feature::{CpuFeature, CpuFeatureSet, SimdTier}`
- `jit::{JitError, JitFn, JitModule}`
- `lower::{format_operands, format_value, lower_op, ClifInsn}`
- `target::{CpuTarget, CpuTargetProfile, DebugFormat}`
- `types::{clif_type_for, ClifType}`

---

### 3.2 `src/types.rs` — 175 lines

**Purpose:** Maps MIR scalar types to CLIF type names. Stage-0 stores CLIF types as a Rust enum with string representations; phase-2 will swap `ClifType` for the actual `cranelift_codegen::ir::Type`.

**Items:**

| Item | Signature | Description |
|---|---|---|
| `enum ClifType` | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` | Enumeration of CLIF scalar types: `I8`, `I16`, `I32`, `I64`, `B1`, `F16`, `F32`, `F64`, `R64` |
| `impl ClifType` | — | Methods on `ClifType` |
| `fn as_str` | `pub const fn as_str(self) -> &'static str` | Returns the CLIF textual name (`"i8"`, `"f32"`, etc.) |
| `fn byte_size` | `pub const fn byte_size(self) -> u8` | Returns the width in bytes; B1 counts as 1 for lane-layout purposes |
| `fn clif_type_for` | `pub fn clif_type_for(mir: &MirType) -> Option<ClifType>` | Maps a `MirType` to a `ClifType`; returns `None` for aggregates, tuples, function types, memrefs, opaques, and vectors |
| `mod tests` | `#[cfg(test)]` | 8 unit tests |

**Key data structure — `ClifType` enum fields and invariants:**
- `I8/I16/I32/I64`: signed integer widths.
- `B1`: boolean; CLIF's `b1`; used for comparison results.
- `F16`: 16-bit float — "not uniformly supported; accepted as attribute at stage-0" (comment at `types.rs:21`). Also used as approximation for `Bf16` (see bug note below).
- `F32/F64`: IEEE floats.
- `R64`: reference/pointer on 64-bit targets.

**Notable algorithm — `clif_type_for`:**
The match is exhaustive over all `MirType` variants. Significant mappings:
- `IntWidth::I1` → `ClifType::B1` (not `I1`; Cranelift has no `b1` as an ABI param type in the JIT path, but `B1` is used in the text emitter).
- `IntWidth::Index` → `ClifType::I64` (pointer-width integer lowered as 64-bit).
- `FloatWidth::Bf16` → `ClifType::F16` — **stage-0 approximation** noted at `types.rs:77`. BFloat16 and Float16 are mapped to the same CLIF type. This is a known inaccuracy with a `// stage-0 approximation` comment.
- `MirType::Vec(_, _)` → `None` with comment at `types.rs:86–90`: "vec3/vec4 not yet mappable to a single CLIF scalar type. Cranelift has vector types (e.g., f32x4) but stage-0.5 JIT lowers scalar-only; vec3 ops are scalarized at a later stage."

**Stubs / deferred:** The `ClifType::B1` mapping works for the text emitter but is incompatible with the real JIT path (`jit.rs:797` maps `MirType::Bool` to `cl_types::I8`, not `cl_types::I8` being `b1`). The two paths diverge here — this is intentional for stage-0 but a correctness gap to close in phase-2.

**Test coverage:** All 8 tests pass essential type mappings: name strings, byte sizes, int/float/bool/handle/aggregate conversions. Good coverage for a utility module.

---

### 3.3 `src/abi.rs` — 119 lines

**Purpose:** Defines calling-convention and object-format enumerations. These are pure data enumerations; no actual ABI encoding logic exists here — that is deferred to phase-2 when cranelift's real ABI handling takes over.

**Items:**

| Item | Signature | Description |
|---|---|---|
| `enum Abi` | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` | Three variants: `SysVAmd64`, `WindowsX64`, `DarwinAmd64` |
| `impl Abi` | — | Methods |
| `fn as_str` | `pub const fn as_str(self) -> &'static str` | Short name: `"sysv"`, `"win64"`, `"darwin"` |
| `fn typical_object_format` | `pub const fn typical_object_format(self) -> ObjectFormat` | Canonical ABI→object-format pairing (SysV→ELF, Windows→COFF, Darwin→MachO) |
| `impl fmt::Display for Abi` | — | Delegates to `as_str` |
| `enum ObjectFormat` | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` | Three variants: `Elf`, `Coff`, `MachO` |
| `impl ObjectFormat` | — | Methods |
| `fn as_str` | `pub const fn as_str(self) -> &'static str` | Short name: `"elf"`, `"coff"`, `"macho"` |
| `fn extension` | `pub const fn extension(self) -> &'static str` | File extension: `".o"`, `".obj"`, `".o"` |
| `impl fmt::Display for ObjectFormat` | — | Delegates to `as_str` |
| `mod tests` | `#[cfg(test)]` | 4 unit tests |

**Note:** `Abi::DarwinAmd64` description says "uses SysV with Apple-extensions" in the doc comment, but the actual calling-convention enforcement is entirely deferred to cranelift's ISA layer. The `Abi` enum here is metadata only.

**Test coverage:** Four tests covering names, extensions, and ABI→format pairings. Complete for a pure-data module.

---

### 3.4 `src/feature.rs` — 272 lines

**Purpose:** Defines the SIMD ISA tier hierarchy and individual CPU feature flags. Used by `CpuTargetProfile` to record what instruction-set extensions are available for a given target. The feature set drives target-feature strings passed to Cranelift's ISA configuration.

**Items:**

| Item | Signature | Description |
|---|---|---|
| `enum SimdTier` | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` | Four variants: `ScalarOnly`, `Sse2`, `Avx2`, `Avx512`. Monotonic lattice: `ScalarOnly ⊑ Sse2 ⊑ Avx2 ⊑ Avx512` |
| `impl SimdTier` | — | Methods |
| `fn as_str` | `pub const fn as_str(self) -> &'static str` | Short name: `"scalar"`, `"sse2"`, `"avx2"`, `"avx512"` |
| `fn at_least` | `pub const fn at_least(self, other: Self) -> bool` | True iff `self.rank() >= other.rank()` — lattice comparison |
| `fn rank` | `const fn rank(self) -> u8` | Private; maps to `0/1/2/3` |
| `impl fmt::Display for SimdTier` | — | Delegates to `as_str` |
| `enum CpuFeature` | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]` | 17 variants covering FMA, BMI1/2, POPCNT, LZCNT, MOVBE, AVX-512 F/DQ/BW/VL/VNNI/BF16, VAES, PCLMULQDQ, SHA, RDRAND, RDSEED |
| `impl CpuFeature` | — | Methods |
| `fn as_str` | `pub const fn as_str(self) -> &'static str` | LLVM/Cranelift target-feature string (e.g., `"fma"`, `"avx512vnni"`) |
| `impl fmt::Display for CpuFeature` | — | Delegates to `as_str` |
| `struct CpuFeatureSet` | `#[derive(Debug, Clone, Default, PartialEq, Eq)]` | Wraps a `BTreeSet<CpuFeature>` to provide a sorted, deduplicated feature set |
| `impl CpuFeatureSet` | — | Methods |
| `fn new` | `pub fn new() -> Self` | Creates empty set (delegates to `Default`) |
| `fn add` | `pub fn add(&mut self, f: CpuFeature)` | Inserts a feature into the set |
| `fn contains` | `pub fn contains(&self, f: CpuFeature) -> bool` | Membership test |
| `fn iter` | `pub fn iter(&self) -> impl Iterator<Item = CpuFeature> + '_` | Iterates in stable sorted order (enum declaration order via `Ord` derive) |
| `fn len` | `pub fn len(&self) -> usize` | Count of features |
| `fn is_empty` | `pub fn is_empty(&self) -> bool` | True iff set has zero features |
| `fn summary_suffix` | `pub fn summary_suffix(&self) -> String` | Produces `"+fma+bmi2"` style suffix for diagnostics |
| `fn render_target_features` | `pub fn render_target_features(&self) -> String` | Produces `"+fma,+bmi2,+popcnt"` — the format Cranelift's ISA builder expects |
| `impl FromIterator<CpuFeature> for CpuFeatureSet` | — | Enables `.collect()` and `from_iter([…])` construction |
| `mod tests` | `#[cfg(test)]` | 8 unit tests |

**Key invariant:** `CpuFeatureSet` uses `BTreeSet` internally, which means iteration order is determined by the derived `Ord` on `CpuFeature`. Because `Ord` is derived, the order is enum declaration order (discriminant order). The test `feature_set_iter_is_sorted` at `feature.rs:241` verifies this explicitly: `Fma < Bmi1 < Bmi2 < Popcnt` in the enum declaration.

**Observation:** The `summary_suffix` and `render_target_features` methods produce slightly different formats (`+fma+bmi2` vs `+fma,+bmi2`). The comma-separated form is what Cranelift accepts; the plus-concatenated form is for human diagnostics in `CpuTargetProfile::summary()`. This is correct but worth noting for callers.

**Test coverage:** 8 tests covering tier names, monotonic ordering, feature names, empty/non-empty sets, sorted iteration, summary suffix, and target-feature string rendering. Thorough.

---

### 3.5 `src/target.rs` — 271 lines

**Purpose:** Defines the canonical µarch enumeration and bundles all codegen knobs into `CpuTargetProfile`. This is the primary configuration object passed through the codegen pipeline.

**Items:**

| Item | Signature | Description |
|---|---|---|
| `enum CpuTarget` | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` | 7 variants: `IntelAlderLake`, `IntelRaptorLake`, `IntelMeteorLake`, `IntelArrowLake`, `AmdZen4`, `AmdZen5`, `GenericX86_64V3` |
| `impl CpuTarget` | — | Methods |
| `fn triple` | `pub const fn triple(self) -> &'static str` | Canonical name for diagnostics (e.g., `"intel-alder-lake"`, `"amd-zen5"`, `"x86-64-v3"`) |
| `fn default_simd_tier` | `pub const fn default_simd_tier(self) -> SimdTier` | SIMD tier this µarch supports by default |
| `const ALL_TARGETS: [Self; 7]` | `pub const ALL_TARGETS: [Self; 7]` | Static array of all 7 supported targets — useful for iteration in tests and profile generation |
| `impl fmt::Display for CpuTarget` | — | Delegates to `triple()` |
| `enum DebugFormat` | `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` | Three variants: `Dwarf5`, `CodeView`, `None` |
| `impl DebugFormat` | — | Methods |
| `fn as_str` | `pub const fn as_str(self) -> &'static str` | Short name: `"dwarf5"`, `"codeview"`, `"none"` |
| `struct CpuTargetProfile` | `#[derive(Debug, Clone, PartialEq, Eq)]` | Bundle: `target`, `simd_tier`, `features`, `abi`, `object_format`, `debug_format` |
| `impl CpuTargetProfile` | — | Constructor and diagnostic methods |
| `fn windows_default` | `pub fn windows_default() -> Self` | Intel Alder Lake + AVX2 + FMA/BMI1/BMI2/POPCNT/LZCNT/MOVBE + Windows x64 + COFF + CodeView |
| `fn linux_default` | `pub fn linux_default() -> Self` | Intel Alder Lake + AVX2 + same features + SysV + ELF + DWARF5 |
| `fn darwin_default` | `pub fn darwin_default() -> Self` | GenericX86_64V3 + AVX2 + FMA/BMI1/BMI2/POPCNT/LZCNT + Darwin + MachO + DWARF5 |
| `fn summary` | `pub fn summary(&self) -> String` | One-line diagnostic: `"intel-alder-lake / avx2+fma+bmi2 / sysv / elf"` |
| `mod tests` | `#[cfg(test)]` | 9 unit tests |

**Notable design notes:**

- `IntelArrowLake` has `default_simd_tier` returning `Avx2` despite the comment noting "Arrow Lake reintroduces AVX-512 on some SKUs via AVX10.1". This is conservative and correct for the majority of Arrow Lake SKUs, but should be revisited when AVX10.1 support lands in the Cranelift ISA backend.

- `IntelAlderLake`'s AVX-512 situation is correctly handled: the P-core silicon has AVX-512 circuitry but it was disabled in production microcode to avoid heterogeneous-core dispatch issues. The tier is `Avx2`. A future mechanism to override this (for hypothetical unlocked systems) would go through `CpuTargetProfile.simd_tier` override.

- `darwin_default` uses `GenericX86_64V3` (not `IntelAlderLake`) because macOS Intel systems span a wider generation range. The `darwin_default` omits `Movbe` from its feature set (present in windows/linux defaults) — this is correct since `movbe` is primarily a byte-swap convenience unavailable on older Intel Mac-era chips.

**Test coverage:** 9 tests covering target uniqueness, SIMD tier correctness for Intel and AMD, the three platform defaults, summary string content, debug-format names, profile equality, and feature-set mutability. Thorough.

---

### 3.6 `src/lower.rs` — 351 lines

**Purpose:** Text-CLIF MIR-op lowering. Each `MirOp` is translated to zero-or-more `ClifInsn` structs containing formatted CLIF text strings. This module feeds the `emit.rs` text emitter. It is parallel to but separate from the real Cranelift IR construction in `jit.rs`.

**Items:**

| Item | Signature | Description |
|---|---|---|
| `struct ClifInsn` | `#[derive(Debug, Clone, PartialEq, Eq)]` | Single CLIF text instruction; `text: String` includes 4-space indentation |
| `impl ClifInsn` | — | `fn new(text: impl Into<String>) -> Self` — private constructor |
| `fn lower_op` | `pub fn lower_op(op: &MirOp) -> Option<Vec<ClifInsn>>` | Top-level dispatch: maps `op.name` to a lowering helper; returns `None` for unrecognized ops |
| `fn lower_binary` | `fn lower_binary(op: &MirOp, clif_name: &str) -> Option<Vec<ClifInsn>>` | Private helper for two-operand scalar ops: `%r = <clif_name> %a, %b` |
| `fn lower_unary` | `fn lower_unary(op: &MirOp, clif_name: &str) -> Option<Vec<ClifInsn>>` | Private helper for one-operand scalar ops: `%r = <clif_name> %a` |
| `fn lower_constant` | `fn lower_constant(op: &MirOp) -> Option<Vec<ClifInsn>>` | Handles `arith.constant`: reads `value` attribute + result type; emits `iconst.i32 42` for ints or `f32const 3.14` for floats |
| `fn lower_cmp` | `fn lower_cmp(op: &MirOp, kind: &str) -> Option<Vec<ClifInsn>>` | Comparison ops: reads `predicate` attribute; emits `%r = icmp/fcmp <pred> %a, %b` |
| `fn lower_select` | `fn lower_select(op: &MirOp) -> Option<Vec<ClifInsn>>` | `arith.select`: emits `%r = select %cond, %t, %f` |
| `fn lower_call` | `fn lower_call(op: &MirOp) -> Option<Vec<ClifInsn>>` | `func.call`: reads `callee` attribute; emits either `call %callee(args)` (void) or `%r = call %callee(args)` (valued) |
| `fn format_value` | `pub fn format_value(v: ValueId) -> String` | Maps `ValueId(n)` → `"v{n}"` — CLIF's textual value name |
| `fn format_operands` | `pub fn format_operands(ops: &[ValueId]) -> String` | Maps a slice of `ValueId` to comma-separated `"v0, v1, v2"` |
| `mod tests` | `#[cfg(test)]` | 17 unit tests |

**Notable algorithm — `lower_op` dispatch table:**

The dispatch is a `match op.name.as_str()` over recognized op names. Currently recognized:
- `arith.constant` → `lower_constant`
- `arith.addi/subi/muli/divsi/remsi` → binary with `iadd/isub/imul/sdiv/srem`
- `arith.negi` → unary with `ineg`
- `arith.addf/subf/mulf/divf` → binary with `fadd/fsub/fmul/fdiv`
- `arith.negf` → unary with `fneg`
- `arith.cmpi` → `lower_cmp("icmp")`
- `arith.cmpf` → `lower_cmp("fcmp")`
- `arith.select` → `lower_select`
- `func.return` → inline: `"    return {operands}"`
- `func.call` → `lower_call`
- `math.sqrtf` or `math.sqrt` → unary with `sqrt`
- `_` → `None` (unrecognized; callers emit a comment placeholder)

**Deferred ops (documented in module header, `lower.rs:22–24`):**
- Control flow: `scf.if`, `scf.for`, `scf.while` → CLIF blocks + jumps.
- Memref ops: `memref.load`, `memref.store`.
- Vector ops: `arith.minimumf`, `arith.maximumf`, SIMD ops.

**Notable quirk — `func.return` formatting (`lower.rs:69–72`):**
```rust
"func.return" => Some(vec![ClifInsn::new(format!(
    "    return {}",
    format_operands(&op.operands)
))]),
```
When `op.operands` is empty, `format_operands` returns `""` and the emitted string is `"    return "` — with a trailing space. The test at `lower.rs:323` explicitly checks for `"    return "` (with trailing space) and passes. This is a minor cosmetic issue: valid CLIF but not idiomatic. The test locks in this behavior.

**Test coverage:** 17 tests covering all major lowering paths including: `addi/addf/muli/divf/negf`, constants (int/float), comparisons with predicates, select, return (with and without value), call (with and without result), sqrt intrinsic, and the `None` return for unknown ops. Excellent coverage for the text emitter.

---

### 3.7 `src/emit.rs` — 401 lines

**Purpose:** The text-CLIF module emitter. Translates a complete `MirModule` into a `EmittedArtifact` containing a header banner and one CLIF-style function block per MIR function. Uses `lower.rs` to lower individual ops.

**Items:**

| Item | Signature | Description |
|---|---|---|
| `enum CpuCodegenError` | `#[derive(Debug, Error, PartialEq, Eq)]` | Two variants |
| `CpuCodegenError::NonScalarParam` | fields: `fn_name: String`, `param_idx: usize`, `ty: String` | A MIR fn parameter has a non-scalar type (tuple, memref, etc.) |
| `CpuCodegenError::TooManyResults` | fields: `fn_name: String`, `count: usize` | A MIR fn has more than one result |
| `struct EmittedArtifact` | `#[derive(Debug, Clone, PartialEq, Eq)]` | Output bundle: `profile: CpuTargetProfile`, `clif_text: String`, `fn_count: usize` |
| `impl EmittedArtifact` | — | Methods |
| `fn summary` | `pub fn summary(&self) -> String` | One-line diagnostic with profile summary, fn count, and first-line preview of the CLIF text |
| `fn emit_module` | `pub fn emit_module(module: &MirModule, profile: &CpuTargetProfile) -> Result<EmittedArtifact, CpuCodegenError>` | Main entry point; writes header banner + iterates `module.funcs` calling `emit_function` |
| `fn emit_function` | `fn emit_function(f: &MirFunc, out: &mut String) -> Result<(), CpuCodegenError>` | Private; emits one function block. Validates param/result types, emits signature, entry block header, and body ops |
| `fn ret_text` | `fn ret_text(r: Option<ClifType>) -> &'static str` | Private helper; maps `Option<ClifType>` to `"()"` (void) or the CLIF type name |
| `mod tests` | `#[cfg(test)]` | 12 unit tests |

**Notable algorithm — `emit_function`:**

1. Rejects `f.results.len() > 1` with `TooManyResults` — this is the text emitter's limitation (not the JIT engine's, which handles multi-result via out-params).
2. Converts each param type via `clif_type_for`; rejects non-scalars.
3. Emits function signature: `\nfunction %{name}({params}) -> {ret} {`.
4. Emits `block0({params}):`.
5. If `op_count == 0`: emits a skeleton comment + `return`.
6. Otherwise: iterates only the `entry` (first) block's ops. Additional blocks are explicitly "phase-2 work" (comment at `emit.rs:151`).
7. Calls `lower_op` on each op. Unrecognized ops emit `; unlowered : {op.name} (stage-0 recognizes arith/func/math only)` comments.
8. Detects whether the loop saw a `func.return` op; if not, auto-appends `    return`.

**Correctness observation:** The multi-block limitation is prominently documented but does mean that any MIR function with more than one basic block will have only its entry block emitted, with no error or warning. This could silently produce incorrect/incomplete text for control-flow-bearing functions. The behavior is documented as intentional for stage-0 but is a correctness gap for non-trivial functions.

**Test coverage:** 12 tests spanning: empty module header, single empty function, i32→i32 signature, rejection of non-scalar params, rejection of multi-result, summary format, end-to-end add (`iadd` + `return`), constant + arith, float mul, unrecognized op comment, and more. The end-to-end body-lowering tests (T11-D18) are particularly valuable.

---

### 3.8 `src/jit.rs` — 2,199 lines

**Purpose:** The real Cranelift JIT engine. This is the largest and most critical file. It constructs actual Cranelift IR via `FunctionBuilder`, compiles to machine code via `cranelift_jit::JITModule`, and exposes callable function handles via fn-pointer casts. This is the mechanism by which CSSLv3-derived programs first execute.

**Unsafe usage:** The module-level `#![allow(unsafe_code)]` (line 2) overrides the crate-root `#![deny(unsafe_code)]`. All unsafe blocks are narrowly scoped to `std::mem::transmute(addr)` in the `JitFn::call_*` methods. Each unsafe block is accompanied by a `// SAFETY:` comment explaining the invariants: the address comes from `cranelift_jit::JITModule::get_finalized_function`, the module is kept alive through the `&JitModule` borrow, and the MIR signature was verified to match the fn-ptr type before the transmute.

**Items — `JitError` enum:**

| Variant | Fields | Description |
|---|---|---|
| `UnsupportedFeature` | `fn_name: String`, `reason: String` | Feature the stage-0 JIT doesn't support (non-scalar type, empty body, etc.) |
| `UnsupportedMirOp` | `fn_name: String`, `op_name: String` | Op name not in the JIT's dispatch table |
| `LoweringFailed` | `fn_name: String`, `detail: String` | Cranelift reported a codegen error, or a structural constraint was violated |
| `UnknownFunction` | `name: String` | Fn name not in the JIT module's fn_table |
| `AlreadyFinalized` | — | `compile()` called after `finalize()` |
| `NotFinalized` | — | `call_*` invoked before `finalize()` |
| `SignatureMismatch` | `name: String`, `expected: String`, `actual: String` | Wrong `call_*` method for this fn's MIR signature |

**Items — `JitFn` struct:**

| Field | Type | Description |
|---|---|---|
| `name` | `String` | Primal fn name from MIR |
| `param_count` | `usize` | Number of original MIR params (not counting out-param pointers) |
| `has_result` | `bool` | True iff the fn has at least one result |
| `param_types` | `Vec<MirType>` | MIR param types — used to validate `call_*` method selection |
| `result_type` | `Option<MirType>` | First MIR result type (for single-result API) |
| `all_result_types` | `Vec<MirType>` | All MIR result types (for multi-result via out-param ABI) |
| `uses_out_params` | `bool` | True if the cranelift signature uses out-param pointers for multi-result ABI |

**Items — `JitFn` methods (all `pub`):**

| Method | Signature | Description |
|---|---|---|
| `call_i64_i64_to_i64` | `pub fn call_i64_i64_to_i64(&self, a: i64, b: i64, module: &JitModule) -> Result<i64, JitError>` | Calls fn as `extern "C" fn(i64, i64) -> i64` after signature validation |
| `call_i32_i32_to_i32` | `pub fn call_i32_i32_to_i32(&self, a: i32, b: i32, module: &JitModule) -> Result<i32, JitError>` | Calls fn as `extern "C" fn(i32, i32) -> i32` |
| `call_f32_f32_to_f32` | `pub fn call_f32_f32_to_f32(&self, a: f32, b: f32, module: &JitModule) -> Result<f32, JitError>` | Calls fn as `extern "C" fn(f32, f32) -> f32` |
| `call_unit_to_i32` | `pub fn call_unit_to_i32(&self, module: &JitModule) -> Result<i32, JitError>` | Calls fn as `extern "C" fn() -> i32` |
| `call_i32_to_i32` | `pub fn call_i32_to_i32(&self, a: i32, module: &JitModule) -> Result<i32, JitError>` | Calls fn as `extern "C" fn(i32) -> i32` |
| `call_f32_to_f32` | `pub fn call_f32_to_f32(&self, a: f32, module: &JitModule) -> Result<f32, JitError>` | Calls fn as `extern "C" fn(f32) -> f32` |
| `call_f32_f32_f32_to_f32` | `pub fn call_f32_f32_f32_to_f32(&self, a: f32, b: f32, c: f32, module: &JitModule) -> Result<f32, JitError>` | AD reverse-mode shape: `(a, b, d_y) -> d_x` |
| `call_f32_f32_f32_f32_to_f32` | `pub fn call_f32_f32_f32_f32_to_f32(&self, a: f32, b: f32, d_a: f32, d_b: f32, module: &JitModule) -> Result<f32, JitError>` | AD forward-mode tangent body for 2-param primal |
| `call_f32x8_to_f32` | `pub fn call_f32x8_to_f32(&self, arg0..arg7: f32, module: &JitModule) -> Result<f32, JitError>` | 8 f32 args → f32; canonical shape for scalarized `sphere_sdf(p: vec3, r: f32)` fwd-tangent |
| `call_f32x5_to_f32` | `pub fn call_f32x5_to_f32(&self, arg0..arg4: f32, module: &JitModule) -> Result<f32, JitError>` | 5 f32 args → f32; bwd single-adjoint extraction for 4-param primal |
| `call_bwd_2_f32_f32_f32_to_f32f32` | `pub fn call_bwd_2_f32_f32_f32_to_f32f32(&self, a: f32, b: f32, d_y: f32, module: &JitModule) -> Result<(f32, f32), JitError>` | Multi-result out-param ABI call: native signature `(a, b, d_y, *mut f32, *mut f32) -> ()`; returns `(d_a, d_b)` |
| `fn check_sig` | `fn check_sig(&self, expected_params: &[MirType], expected_result: MirType) -> Result<(), JitError>` | Private; validates param types + result type match |

**Items — `JitModule` struct:**

| Field | Type | Description |
|---|---|---|
| `inner` | `Option<ClJitModule>` | The Cranelift `JITModule`; `Option` to allow `take()` during finalize |
| `builder_ctx` | `FunctionBuilderContext` | Reused across `compile()` calls |
| `codegen_ctx` | `Context` | Cranelift codegen context; cleared and reused per function |
| `fn_table` | `HashMap<String, (FuncId, Option<*const u8>)>` | fn name → (FuncId, code-addr-after-finalize) |
| `handles` | `Vec<JitFn>` | Metadata handles for all compiled fns |
| `finalized` | `bool` | Whether finalization has occurred |

**Items — `JitModule` methods:**

| Method | Signature | Description |
|---|---|---|
| `impl fmt::Debug for JitModule` | — | Emits `JitModule { fn_count: N, finalized: B }` — non-destructive |
| `fn new` | `pub fn new() -> Self` | Creates a JIT module: configures Cranelift ISA via `cranelift_native::builder()`, sets `is_pic = false`, builds `JITBuilder` + `JITModule` |
| `fn compile` | `pub fn compile(&mut self, primal: &MirFunc) -> Result<JitFn, JitError>` | Main compilation entry; see algorithm detail below |
| `fn finalize` | `pub fn finalize(&mut self) -> Result<(), JitError>` | Calls `finalize_definitions()` + populates code-addrs in `fn_table`; idempotent |
| `fn get` | `pub fn get(&self, name: &str) -> Option<&JitFn>` | Looks up a handle by fn name |
| `fn len` | `pub fn len(&self) -> usize` | Count of compiled fns |
| `fn is_empty` | `pub fn is_empty(&self) -> bool` | True iff no fns compiled |
| `fn is_finalized` | `pub const fn is_finalized(&self) -> bool` | Finalization predicate |
| `fn is_activated` | `pub const fn is_activated() -> bool` | Always `true` at T11-D20 — used for scaffold-mode detection in callers |
| `fn code_addr_for` | `fn code_addr_for(&self, name: &str) -> Result<*const u8, JitError>` | Private; resolves fn name → code address; returns `NotFinalized` or `UnknownFunction` on failure |
| `impl Default for JitModule` | — | Delegates to `new()` |

**Items — private helper functions:**

| Function | Signature | Description |
|---|---|---|
| `fn mir_to_cl_type` | `fn mir_to_cl_type(mir: &MirType) -> Option<cranelift_codegen::ir::Type>` | Maps MIR scalars to real Cranelift `ir::Type`; `I1` → `I8` (no b1 in ABI); `F16/Bf16` → `None` (not in stable CLIF) |
| `fn lower_op_to_cl` | `fn lower_op_to_cl(op: &MirOp, builder: &mut FunctionBuilder<'_>, value_map: &mut HashMap<ValueId, cl::Value>, fn_name: &str, callee_refs: &HashMap<String, cl::FuncRef>) -> Result<bool, JitError>` | Main per-op dispatch for real IR construction; returns `Ok(true)` if op was a terminator |
| `fn emit_binary` | `fn emit_binary<F>(op, builder, value_map, fn_name, emit: F) -> Result<bool, JitError>` | Generic binary op helper; resolves two operand ValueIds from `value_map`, calls `emit` closure, inserts result |
| `fn emit_unary` | `fn emit_unary<F>(op, builder, value_map, fn_name, emit: F) -> Result<bool, JitError>` | Generic unary op helper; resolves one operand ValueId, calls `emit` closure, inserts result |
| `fn lower_cmpf` | `fn lower_cmpf(op, builder, value_map, fn_name) -> Result<bool, JitError>` | `arith.cmpf` → `parse_float_cc` + `emit_binary` with `fcmp` |
| `fn lower_cmpi` | `fn lower_cmpi(op, builder, value_map, fn_name) -> Result<bool, JitError>` | `arith.cmpi` → `parse_int_cc` + `emit_binary` with `icmp` |
| `fn lower_intrinsic_call` | `fn lower_intrinsic_call(op, builder, value_map, fn_name, callee_refs) -> Result<bool, JitError>` | `func.call` dispatch: inline intrinsics (min/max/abs/sqrt/neg), libm transcendentals (sin/cos/exp/log via FuncRef), user-defined callees via pre-declared FuncRef |
| `pub fn is_intrinsic_callee` | `pub fn is_intrinsic_callee(name: &str) -> bool` | Public; returns true for inline or transcendental intrinsics; exposed for test introspection |
| `fn is_inline_intrinsic_callee` | `fn is_inline_intrinsic_callee(name: &str) -> bool` | Private; true for intrinsics emitted as direct CLIF insts (min/max/abs/sqrt/neg variants) |
| `fn transcendental_extern_name` | `fn transcendental_extern_name(name: &str) -> Option<&'static str>` | Private; maps `"sin"/"math.sin"` → `"sinf"`, etc.; returns libm symbol for extern linkage |
| `fn lower_select` | `fn lower_select(op, builder, value_map, fn_name) -> Result<bool, JitError>` | `arith.select`: resolves 3 operands (cond, t, f), emits `builder.ins().select(cond, t, f)` |
| `fn predicate_attr` | `fn predicate_attr(op: &MirOp) -> Result<&str, JitError>` | Extracts `predicate` attribute from op or returns `LoweringFailed` |
| `fn parse_float_cc` | `fn parse_float_cc(s: &str) -> Option<FloatCC>` | Maps MLIR-style predicate strings to Cranelift `FloatCC` variants |
| `fn parse_int_cc` | `fn parse_int_cc(s: &str) -> Option<IntCC>` | Maps MLIR-style predicate strings to Cranelift `IntCC` variants |

**Notable algorithm — `JitModule::compile`:**

The compile path is the heart of this file. It proceeds in these stages:

1. **Multi-result detection** (`jit.rs:466`): If `primal.results.len() > 1`, sets `use_out_params = true`. The out-param ABI appends one pointer parameter per result, and the return type becomes void.

2. **Signature construction** (`jit.rs:477–509`): Calls `module.isa().default_call_conv()` to get the host ABI. Maps each MIR param type via `mir_to_cl_type`. For out-param mode, appends one `pointer_type` ABI param per result. For normal mode, maps result types to `sig.returns`.

3. **Function declaration** (`jit.rs:512–522`): Calls `cranelift_module::Module::declare_function` with `Linkage::Export`.

4. **Callee pre-scan** (`jit.rs:523–573`): Scans the entry block's `func.call` ops to pre-declare any callees as FuncRefs before building the function body. This handles three callee classes:
   - **Transcendentals** (sin/cos/exp/log): declared as `Linkage::Import` externals with signature `(f32) -> f32`.
   - **Inline intrinsics** (min/max/abs/sqrt/neg): skipped (will be emitted as CLIF insts directly).
   - **User-defined**: looked up in `self.fn_table` (must have been compiled first in the same module).

5. **Body construction** (`jit.rs:575–683`): Creates a single entry block via `builder.create_block()`. Wires block params to MIR `ValueId`s, handling three cases: (a) simple alignment (param count == block param count), (b) out-param alignment (primal params + out-ptr params), (c) fallback by index for primals with empty `entry.args`.

6. **Op lowering loop** (`jit.rs:633–668`): Iterates ops in the entry block only. For out-param fns, intercepts `func.return` / `cssl.diff.bwd_return` to emit stores through out-ptr params then `return &[]`. For normal fns, calls `lower_op_to_cl` per op.

7. **Trailing return** (`jit.rs:670–679`): If no terminator was seen, auto-emits `return &[]` for zero-result fns or errors for result-bearing fns without an explicit return.

8. **Definition** (`jit.rs:685–691`): Calls `module.define_function(func_id, &mut self.codegen_ctx)`. 

9. **Handle construction** (`jit.rs:693–704`): Builds and returns a `JitFn` handle; inserts `(func_id, None)` into `fn_table` (code-addr populated at finalize).

**Notable algorithm — `mir_to_cl_type` vs `clif_type_for` divergence:**

`mir_to_cl_type` in `jit.rs` (line 794) differs from `clif_type_for` in `types.rs` in two significant ways:
- `MirType::Int(IntWidth::I1)` → `cl_types::I8` (not `B1`). Comment: "cranelift has no b1 param". This is correct for ABI purposes.
- `MirType::Float(FloatWidth::F16 | FloatWidth::Bf16)` → `return None`. In `types.rs`, both mapped to `ClifType::F16`. The JIT path correctly refuses these since neither is in stable Cranelift.
- `MirType::Bool` → `cl_types::I8`. In `types.rs`, this mapped to `ClifType::B1`. Again, the JIT path is more correct for ABI purposes.

This divergence between the text-emitter type map and the JIT type map is intentional (different fidelity requirements) but could cause confusion for a new contributor reading both files. It is not a bug.

**parse_float_cc predicate table:**

Maps MLIR-style predicates to Cranelift `FloatCC`:
`eq/oeq`, `ne/one`, `olt/lt`, `ole/le`, `ogt/gt`, `oge/ge`, `ult`, `ule`, `ugt`, `uge`, `ord`, `uno`.

**parse_int_cc predicate table:**

Maps to Cranelift `IntCC`:
`eq`, `ne`, `slt`, `sle`, `sgt`, `sge`, `ult`, `ule`, `ugt`, `uge`.

**Tests in `jit.rs`:** 40 tests covering:

| Test | What it verifies |
|---|---|
| `jit_module_is_activated_in_stage_0_5` | `is_activated()` == true |
| `empty_module_is_empty_not_finalized` | Initial state |
| `compile_records_handle_before_finalize` | Handle fields after compile |
| `call_before_finalize_returns_not_finalized` | `NotFinalized` error gate |
| `add_i32_roundtrip_3_plus_4_equals_7` | **THE STAGE-0.5 KILLER TEST** — first CSSLv3 program executes |
| `add_i32_handles_negative_inputs` | Negative/large i32 |
| `add_i64_roundtrip` | i64 arithmetic |
| `mul_f32_roundtrip` | f32 multiply |
| `const_fn_returning_42` | `arith.constant` + `func.return` |
| `compile_multi_result_empty_body_errors` | Empty multi-result body → error |
| `compile_rejects_unsupported_mir_op` | Unknown op → `UnsupportedMirOp` |
| `compile_after_finalize_errors` | `AlreadyFinalized` gate |
| `sig_mismatch_on_wrong_call_arm` | Wrong `call_*` method → `SignatureMismatch` |
| `unknown_function_lookup_errors` | Fake handle on unfinalized module |
| `module_debug_is_nondestructive` | `Debug` trait |
| `finalize_is_idempotent` | Second finalize is a no-op |
| `scene_sdf_min_a_b_jit_roundtrip` | **SCENE-SDF MILESTONE** — `cmpf + select` min function |
| `scene_sdf_max_a_b_jit_roundtrip` | max function |
| `cmpi_slt_plus_select_jit_roundtrip` | Integer min |
| `compose_arith_and_select_jit_roundtrip` | Multi-op: `abs(a - b)` |
| `killer_app_scene_sdf_min_gradient_matches_central_difference` | **KILLER-APP RUNTIME VERIFICATION** — forward-tangent vs central differences |
| `killer_app_scene_sdf_min_exact_gradient_values` | Exact gradient values at specific points |
| `multi_result_native_via_out_params` | **T11-D30** — out-param ABI for bwd adjoints |
| `multi_result_sig_mismatch_rejects_wrong_call_shape` | Wrong call shape → `SignatureMismatch` |
| `inter_fn_call_jit_roundtrip` | **T11-D26** — inter-fn calls within same JIT module |
| `inter_fn_call_unknown_callee_errors` | Caller refs uncompiled callee → error |
| `multi_fn_jit_module_shares_finalize` | Two fns share one `finalize()` |
| `cmpf_unknown_predicate_errors` | Unknown float predicate → `LoweringFailed` |
| `libm_sin_jit_roundtrip` | **T11-D29** — libm sin via extern |
| `libm_cos_jit_roundtrip` | libm cos |
| `libm_exp_log_roundtrip` | libm exp + log |
| `hand_built_scene_sdf_min_fwd` (helper) | Builder for forward-tangent of min(a,b) |
| `hand_built_double_f32` (helper) | Builder for `fn double(x) { x + x }` |
| `hand_built_caller_f32` (helper) | Builder for `fn caller(x) { double(x) + 1 }` |
| `hand_built_sin_wrap` (helper) | Builder for `fn sinf_wrap(x) { sin(x) }` |

---

## 4. Crate Notes

### 4.1 Test Coverage

Test coverage is very strong for a stage-0 crate. Every major path through `lower_op`, `emit_module`, and `JitModule::compile/finalize/call_*` is exercised. The "killer app" tests — central-difference gradient verification and out-param multi-result bwd adjoints — are particularly valuable as they verify correctness of the AD integration end-to-end at runtime, not just structurally.

Total test count: approximately 4 (`lib.rs`) + 8 (`types.rs`) + 4 (`abi.rs`) + 8 (`feature.rs`) + 9 (`target.rs`) + 17 (`lower.rs`) + 12 (`emit.rs`) + 40 (`jit.rs`) = **102 tests**.

### 4.2 What Is Incomplete / Stubbed

The crate is explicit about its deferred items. The following are deferred to T11-D22+ or phase-2:

1. **Control flow** (`jit.rs:46`, `lower.rs:22–24`): `scf.if`, `scf.for`, `scf.while`. No basic blocks beyond the entry block are lowered in either the text emitter or the JIT engine. Functions with multiple blocks will silently drop all blocks after the first in the text emitter (`emit.rs:151`).

2. **Memref load/store** (`jit.rs:47`, `lower.rs:23`): No memory access ops are lowered.

3. **Multi-block MIR in text emitter** (`emit.rs:151`): Only the first block (entry) is emitted. No `brif` / jump targets. No error raised for multi-block input.

4. **DWARF-5 + CodeView debug-info** (`lib.rs:22`): `DebugFormat` enum exists but no debug information is actually emitted by either path.

5. **cranelift-object AoT emission** (`lib.rs:21`): ELF/COFF/MachO object file writing is not implemented. The `ObjectFormat` enum is metadata only.

6. **Runtime CPU dispatch** (`lib.rs:23`): AVX2 + AVX-512 multi-variant fat-kernels are not yet implemented.

7. **F16 / BF16 JIT support** (`jit.rs:804`): `mir_to_cl_type` returns `None` for these; the JIT path correctly rejects them.

8. **`arith.divsi` / `arith.remsi` / `arith.negi` in JIT** (`jit.rs:826–906`): The text emitter (`lower.rs`) handles `arith.divsi`, `arith.remsi`, and `arith.negi`, but `lower_op_to_cl` in `jit.rs` does **not** have arms for these three ops. Calling code that produces a `sdiv`, `srem`, or `ineg` MIR op will hit the `other => Err(JitError::UnsupportedMirOp { … })` fallthrough. This is a gap between the text emitter's coverage and the JIT's coverage. It is not marked with a TODO comment — a new contributor might not notice it.

9. **`math.sin/cos/exp/log` in the text emitter** (`lower.rs:74–75`): `lower.rs` recognizes `math.sqrtf` and `math.sqrt` but not `math.sin`, `math.cos`, `math.exp`, or `math.log`. The JIT path handles these via libm extern linkage (`jit.rs:1059–1089`). The text emitter would emit `; unlowered : math.sin` for these. Minor asymmetry.

### 4.3 Bugs and Correctness Issues

**Issue 1 — `func.return` trailing space** (`lower.rs:72`)  
The text emitter generates `"    return "` (trailing space) for a void return. This is cosmetically incorrect CLIF. The test at `lower.rs:323` locks this in. Not a functional bug (the text artifact is never parsed by Cranelift in the current path) but would become a problem if a CLIF parser were introduced.

**Issue 2 — `predicate_attr` loses fn_name context** (`jit.rs:1212–1221`)  
```rust
fn predicate_attr(op: &MirOp) -> Result<&str, JitError> {
    op.attributes
        .iter()
        .find(|(k, _)| k == "predicate")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: String::new(),    // ← empty string
            detail: format!("{} missing `predicate` attribute", op.name),
        })
}
```
The `fn_name` field in the returned error is `String::new()` — an empty string. The caller (`lower_cmpf` / `lower_cmpi`) passes `fn_name` as a parameter but `predicate_attr` does not accept it. Errors from this path will have `fn_name = ""` in the error message, which is unhelpful for diagnostics. File: `jit.rs:1217`.

**Issue 3 — `mir_to_cl_type` / `clif_type_for` divergence for Bool**  
`types.rs:80` maps `MirType::Bool → ClifType::B1`.  
`jit.rs:808` maps `MirType::Bool → cl_types::I8`.  
These are semantically different. In the JIT path, comparison results (which have MirType::Bool) are mapped to `I8`, and the `select` instruction receives an `I8` condition rather than a `B1`. Cranelift's `select` instruction expects its condition to be a type that Cranelift considers "boolean" (not necessarily `B1`; any integer with value 0/nonzero works in practice). This is likely correct but could cause subtle issues if Cranelift's type checker is ever tightened. No `// SAFETY` or comment documents why `I8` was chosen over `B1` here.

**Issue 4 — JIT does not cover `arith.divsi`, `arith.remsi`, `arith.negi`**  
As noted in section 4.2, the text emitter handles all six integer-arith ops from the spec, but the JIT engine only handles `addi`, `subi`, `muli`. Integer division, remainder, and negation hit `UnsupportedMirOp`. This is an undocumented limitation relative to the text emitter.

### 4.4 Dead Code / Surprises

- `JitFn::call_f32x8_to_f32` (`jit.rs:273`) and `JitFn::call_f32x5_to_f32` (`jit.rs:315`) are highly specialized call wrappers for the `sphere_sdf` AD end-to-end test. They are documented with detailed derivation in their doc comments but are not covered by tests in this file (the tests for sphere_sdf appear to be in a different crate — `cssl-autodiff`). These methods are not dead but are hard to exercise without the autodiff test infrastructure.

- `JitModule::inner` is `Option<ClJitModule>` but `take()` is never called on it. The option wrapper seems to anticipate a future "move out and drop" pattern (possibly for a reset/reload flow) but currently `inner` is always `Some` after construction. The `Option` adds a small overhead to every access (`.as_mut()` calls). Not a bug, but potentially unnecessary complexity.

- The `CpuFeature::as_str` comment (`feature.rs:95`) says "Canonical short-name (matches the LLVM target-feature string)." LLVM is not used by this crate; the correct attribution should be "Cranelift target-feature string" or "LLVM-compatible target-feature string." Minor documentation inaccuracy.

### 4.5 Spec Divergences

- `specs/07_CODEGEN.csl` § CPU BACKEND mentions `regalloc2 dispatch + linear-scan fallback` as a deliverable. Neither is implemented; Cranelift's built-in register allocator is used transparently. This is explicitly deferred in `lib.rs:20` ("regalloc2 dispatch + linear-scan fallback").

- `specs/14_BACKEND.csl` § ABI mentions ABI encoding is handled for all three platforms. The `Abi` enum exists but no ABI-specific parameter marshalling is done beyond what Cranelift's `default_call_conv()` provides. This is acceptable for stage-0 but is a spec/implementation gap.

- `specs/07_CODEGEN.csl` and the lib.rs scope block both list DWARF-5 + CodeView debug-info as in-scope for a later phase. `DebugFormat` enum exists but is only metadata in `CpuTargetProfile`; no actual debug section is emitted.

- The text emitter (`emit.rs:130–141`) generates `block0({params}):` with the full parameter list repeated in the block header. Real CLIF format specifies params on the block header, not the function signature. This is cosmetically correct for CLIF's textual format (function parameters _are_ the entry block's parameters), so this is not a divergence but worth noting.

### 4.6 Summary of Findings

The crate is well-structured, well-tested, and delivers its stated stage-0.5 scope: scalar integer and float arithmetic execute via JIT, the autodiff killer-app (forward-mode gradient of `min(a, b)`) is verified at runtime, inter-fn calls and libm transcendentals work, and multi-result functions are handled via an out-param ABI. The main correctness gaps are:

1. `predicate_attr` discards `fn_name` in errors (`jit.rs:1217`) — diagnostics quality issue.
2. `arith.divsi`, `arith.remsi`, `arith.negi` are absent from the JIT (present in text emitter) — undocumented JIT limitation.
3. `MirType::Bool` maps to `B1` in the text path and `I8` in the JIT path — divergence that should be reconciled in phase-2.
4. Multi-block MIR functions silently lose all blocks after the first in the text emitter — no error or warning.
5. `inner: Option<ClJitModule>` adds unnecessary option-wrapping since `take()` is never used.
