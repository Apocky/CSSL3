# Audit Report 08: cssl-lir + cssl-mlir-bridge
<!-- Audited: 2026-05-14 -->
<!-- Auditor: Claude Sonnet 4.6 (automated) -->
<!-- Slice: Low-level IR and MLIR FFI bridge -->

---

## 1. SLICE OVERVIEW

This audit covers two crates in the CSSLv3 stage0 Rust-hosted bootstrap compiler:

- **`cssl-lir`** — described in README as "low-level IR + target orchestration"
- **`cssl-mlir-bridge`** — described in README as "melior FFI bridge to MLIR"

### What they are supposed to be

According to the spec files and module doc-comments, these two crates represent the bridge between the mid-level IR (MIR, held in `cssl-mir`) and the actual native code generation backends. The pipeline as designed in `specs/07_CODEGEN.csl`, `specs/14_BACKEND.csl`, and `specs/15_MLIR.csl` is roughly:

```
HIR (cssl-hir)
  → MIR as MLIR-dialect (cssl-mir)
    → LIR / target orchestration (cssl-lir)   ← [this slice, crate 1]
      → CPU: Cranelift (cssl-cgen-cpu-cranelift)
      → GPU: rspirv / SPIR-V (cssl-cgen-gpu-*)
    → MLIR textual/FFI bridge (cssl-mlir-bridge)   ← [this slice, crate 2]
      → melior/mlir-sys C++ FFI (deferred, T6-phase-2)
      → textual --emit-mlir dump (active, T6-phase-1)
```

**`cssl-lir`** is supposed to act as the dispatcher that takes a `MirModule` and routes it through the correct code-generation backend — Cranelift for CPU targets and the rspirv/DXIL/MSL/WGSL emitters for GPU targets — ultimately assembling the fat-binary format described in `specs/07_CODEGEN.csl § FAT-BINARY`. It is labeled as the "low-level IR target-emission orchestration" layer.

**`cssl-mlir-bridge`** is supposed to wrap the `melior` and `mlir-sys` Rust crates (FFI bindings to the MLIR C++ library) so that CSSLv3 can register its custom `cssl.*` MLIR dialect, construct `mlir::Operation` nodes from `MirOp` values, invoke the MLIR pass pipeline (canonicalization, CSE, LICM, DCE, dialect conversion), and eventually lower all the way to `spirv-dialect` or `llvm-dialect` for code generation. The 26+ custom `cssl.*` ops enumerated in `specs/15_MLIR.csl § CSSL-DIALECT OPS` are the target.

### What they ACTUALLY are

**Both crates are scaffolding placeholders with essentially no real implementation.**

`cssl-lir` contains only a single file of 22 lines. Its entire implementation body is one string constant (`STAGE0_SCAFFOLD`) and one test that asserts the string is non-empty. There is no LIR type, no dispatch logic, no fat-binary assembly, no target orchestration of any kind. The module doc-comment itself says "§ STATUS : T10 scaffold — target-emit orchestration pending."

`cssl-mlir-bridge` is more developed — it has two source files totaling 107 lines — but it is also far from its intended purpose. The FFI dependency (melior/mlir-sys) is explicitly **commented out** in the workspace `Cargo.toml` with the annotation `# T6 : verify-melior-windows-compat`, meaning the crate has no actual linkage to the MLIR C++ library. What it does instead is wrap the MIR pretty-printer already present in `cssl-mir::print_module`, exposing it through two thin adapter functions. This provides the `--emit-mlir` textual dump path only; real MLIR Operation construction, dialect registration, and pass-pipeline invocation are all deferred to T6-phase-2.

In summary: **`cssl-lir` is a pure empty scaffold. `cssl-mlir-bridge` is a thin wrapper around an already-existing printer, providing exactly one functional feature (textual MLIR dump), with the real FFI work explicitly deferred.**

---

## 2. CRATE SUMMARIES

### 2.1 cssl-lir

| Field | Value |
|---|---|
| **Crate name** | `cssl-lir` |
| **Path** | `compiler-rs/crates/cssl-lir/` |
| **Intended purpose** | Dispatch MIR → CPU/GPU code-generation backends; assemble fat-binary per `specs/07_CODEGEN.csl` |
| **Actual current state** | Pure scaffold; single const + one trivial test; zero implementation |
| **Pipeline role** | Should be the penultimate stage before native code emission, sitting between `cssl-mir` and `cssl-cgen-*` crates |
| **Cargo.toml dependencies** | None (inherits workspace package fields only; no `[dependencies]` section present) |
| **Total LOC** | 22 (lib.rs only) |
| **File list** | `Cargo.toml`, `src/lib.rs` |

### 2.2 cssl-mlir-bridge

| Field | Value |
|---|---|
| **Crate name** | `cssl-mlir-bridge` |
| **Path** | `compiler-rs/crates/cssl-mlir-bridge/` |
| **Intended purpose** | melior/mlir-sys FFI wrapper; MLIR Operation construction from MirOp; cssl-dialect registration; pass-pipeline invocation |
| **Actual current state** | Thin wrapper around `cssl_mir::print_module`; provides textual MLIR dump only; no FFI |
| **Pipeline role** | Should bridge MIR to the MLIR pass pipeline before dialect lowering; currently only serves `--emit-mlir` CLI option |
| **Cargo.toml dependencies** | `cssl-mir = { path = "../cssl-mir" }` (only) |
| **Total LOC** | 107 (lib.rs: 46, emit.rs: 61) |
| **File list** | `Cargo.toml`, `src/lib.rs`, `src/emit.rs` |

**Critical note on melior/mlir-sys:** In the workspace `Cargo.toml` at `compiler-rs/Cargo.toml:86-87`, both melior and mlir-sys are commented out:

```toml
# melior     = "0.20"  # T6 : verify-melior-windows-compat + LLVM_SYS_*_PREFIX
# mlir-sys   = "0.3"
```

These are the only entries in `[workspace.dependencies]` that `cssl-mlir-bridge` would need for its stated purpose. Since they are not compiled, the crate has no MLIR C++ linkage whatsoever and **cannot perform any real MLIR operations at this time.**

---

## 3. SOURCE FILE ANALYSIS

### 3.1 cssl-lir/Cargo.toml

**Path:** `compiler-rs/crates/cssl-lir/Cargo.toml`  
**Line count:** 12

This manifest inherits all package metadata from the workspace (version, edition, rust-version, license, authors) and delegates lint configuration to the workspace. There are no `[dependencies]`, `[dev-dependencies]`, `[features]`, or `[build-dependencies]` sections. The absence of any declared dependencies is consistent with the crate's scaffold status: it has nothing to depend on because it implements nothing.

**Items declared:** none beyond workspace inheritance.

---

### 3.2 cssl-lir/src/lib.rs

**Path:** `compiler-rs/crates/cssl-lir/src/lib.rs`  
**Line count:** 22

This is the entire implementation of `cssl-lir`. The file opens with a module-level doc-comment that accurately describes the intended role (MIR → emitter dispatch, fat-binary assembly) and immediately flags the actual status with `§ STATUS : T10 scaffold — target-emit orchestration pending.` Three compiler attributes follow: `#![forbid(unsafe_code)]` (line 9) is appropriate for a crate at this stage, and two `#![deny(rustdoc::*)]` attributes enforce documentation link correctness.

#### Items

**`pub const STAGE0_SCAFFOLD: &str`** (line 14)  
Signature: `pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");`  
A compile-time string constant that captures the crate's version from the `CARGO_PKG_VERSION` environment variable. The name is accurate: this is pure scaffold scaffolding — it provides evidence that the crate compiled (and what version it compiled as) but no functional content. This same `STAGE0_SCAFFOLD` pattern appears identically in `cssl-mlir-bridge/src/lib.rs`.

**`mod scaffold_tests`** (lines 17–22) — `#[cfg(test)]`  
A test-gated module containing a single test.

**`fn scaffold_version_present()`** (line 19) — inside `scaffold_tests`, `#[test]`  
Signature: `fn scaffold_version_present()`  
Asserts that `super::STAGE0_SCAFFOLD` is not an empty string. This is a presence-and-compile guard: if the crate fails to link or the version string somehow becomes empty, this test catches it. The workspace `Cargo.toml` notes that the clippy lint `const_is_empty` is allowed precisely because of this pattern (`assert!(!STAGE0_SCAFFOLD.is_empty()) — trivially-true guard-rail`).

#### Stubs, TODOs, Placeholders

The module doc-comment (lines 5–7) is itself the primary stub declaration:

```
//! § STATUS : T10 scaffold — target-emit orchestration pending.
//! § ROLE   : dispatches MIR → `cssl-cgen-cpu-cranelift` / `cssl-cgen-gpu-*` emitters;
//!            assembles fat-binary per `specs/07_CODEGEN.csl`.
```

There are no `todo!()`, `unimplemented!()`, or `panic!("stub")` calls because there are no function bodies. The entire LIR layer is absent.

#### Cross-references

- References `specs/07_CODEGEN.csl` and `specs/14_BACKEND.csl` in the module doc.
- Would ultimately need `cssl-mir` for its input type (`MirModule`).
- Would dispatch to `cssl-cgen-cpu-cranelift` and the GPU emitter crates — none of which are declared as dependencies.

---

### 3.3 cssl-mlir-bridge/Cargo.toml

**Path:** `compiler-rs/crates/cssl-mlir-bridge/Cargo.toml`  
**Line count:** 15

The manifest is sparse. Beyond workspace-inherited metadata it declares only one dependency:

```toml
[dependencies]
cssl-mir = { path = "../cssl-mir" }
```

No `melior`, no `mlir-sys`, no `llvm-sys`. The description field (`"CSSLv3 stage0 — mlir-sys + melior wrapper + cssl-dialect op emission (FFI)"`) accurately describes the intended future state, not the current one. The `#[lints]` section delegates to the workspace.

---

### 3.4 cssl-mlir-bridge/src/lib.rs

**Path:** `compiler-rs/crates/cssl-mlir-bridge/src/lib.rs`  
**Line count:** 46

The crate root. A detailed module doc-comment (lines 1–26) explains the three-tier architecture:

1. **T6-phase-1 (current):** Pure-Rust textual emission wrapping `cssl_mir::print_module`. Active and functional.
2. **T6-phase-2 (deferred):** melior context/module construction, cssl-dialect registration, `mlir::Operation` construction from `MirOp`, pass-pipeline invocation. Blocked on MSVC toolchain per T1-D7.
3. **FALLBACK (pre-authorized):** If melior's Windows build chain blocks, the textual path continues to work, and the compiler driver pipes output through an external `mlir-opt` CLI.

The doc-comment also notes that `unsafe_code` is *permitted* at the melior FFI boundary — yet the file currently carries `#![forbid(unsafe_code)]` at line 27 because no FFI exists yet.

#### Items

**`pub mod emit`** (line 31)  
Declares the `emit` submodule, loaded from `src/emit.rs`.

**`pub use emit::{emit_module_to_string, emit_module_to_writer}`** (line 33)  
Re-exports both public functions from `emit.rs` at the crate root so callers can write `cssl_mlir_bridge::emit_module_to_string(...)` without knowing the internal module structure.

**`pub const STAGE0_SCAFFOLD: &str`** (line 36)  
Signature: `pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");`  
Identical in purpose to the same constant in `cssl-lir`: compile-time version capture for scaffold verification.

**`mod scaffold_tests`** (lines 39–46) — `#[cfg(test)]`  
Test-gated module with one test.

**`fn scaffold_version_present()`** (line 43) — inside `scaffold_tests`, `#[test]`  
Signature: `fn scaffold_version_present()`  
Asserts that `STAGE0_SCAFFOLD` is non-empty. Uses `use super::STAGE0_SCAFFOLD;` (explicit import, unlike `cssl-lir` which uses `super::STAGE0_SCAFFOLD` inline).

#### Cross-references

- `pub mod emit` → `src/emit.rs`
- Transitively depends on `cssl-mir` via the `emit` module's `use cssl_mir::MirModule`.

---

### 3.5 cssl-mlir-bridge/src/emit.rs

**Path:** `compiler-rs/crates/cssl-mlir-bridge/src/emit.rs`  
**Line count:** 61

This is the only file in the slice that contains real, working, non-trivial code (excluding tests). It provides two public functions that form the `--emit-mlir` dump path.

The module doc (lines 1–4) is concise: "Wraps `cssl_mir::print_module` so that the compiler driver has a stable emission API regardless of whether the melior FFI path is active."

#### Items

**`use std::io::{self, Write}`** (line 6)  
Standard library import for I/O traits.

**`use cssl_mir::MirModule`** (line 8)  
The sole external dependency. Brings in the `MirModule` container type from `cssl-mir`, which holds a list of `MirFunc` values each containing `MirRegion → MirBlock → MirOp` chains.

**`pub fn emit_module_to_string`** (lines 11–14)  
Signature: `pub fn emit_module_to_string(module: &MirModule) -> String`  
Attributes: `#[must_use]`  
Takes a shared reference to a `MirModule` and returns its MLIR textual representation as a `String`. The implementation is a one-liner: `cssl_mir::print_module(module)`. All formatting logic lives in `cssl-mir/src/print.rs`; this function is a stable re-export boundary so the compiler driver does not need to import `cssl-mir` directly for the `--emit-mlir` path.

**`pub fn emit_module_to_writer`** (lines 17–20)  
Signature: `pub fn emit_module_to_writer<W: Write>(module: &MirModule, w: &mut W) -> io::Result<()>`  
A generic version of `emit_module_to_string` that writes to any `io::Write` sink (e.g., a `File`, `stdout`, a `Vec<u8>`). Internally calls `emit_module_to_string`, then `w.write_all(s.as_bytes())`. This means the full string is always allocated before being written, which is acceptable for stage0 purposes (no streaming). Returns `io::Result<()>` to propagate write errors.

**`mod tests`** (lines 22–61) — `#[cfg(test)]`  
Test module with three tests. Uses `use cssl_mir::{IntWidth, MirFunc, MirModule, MirType}` (line 25).

**`fn sample() -> MirModule`** (lines 27–35) — private helper inside `tests`  
Constructs a minimal `MirModule` named `"sample"` with one function `"add"` that takes two `i32` arguments and returns one `i32`. Used as test fixture for the two emission tests.

**`fn emit_to_string_produces_valid_text()`** (lines 38–43) — `#[test]`  
Calls `emit_module_to_string` on the sample module and asserts that the result contains `"module @sample"` and `"func.func @add"`. Verifies that the printer produces recognizable MLIR textual format for a named module with a named function.

**`fn emit_to_writer_matches_string()`** (lines 45–52) — `#[test]`  
Calls both `emit_module_to_writer` (into a `Vec<u8>`) and `emit_module_to_string` on the same sample module, then asserts the two outputs are identical. Verifies that the writer path is consistent with the string path — which is guaranteed by the implementation (the writer calls the string function), but still a useful regression guard.

**`fn emit_empty_module()`** (lines 54–59) — `#[test]`  
Calls `emit_module_to_string` on a `MirModule::new()` (no name, no functions) and asserts the output starts with `"module"`. Verifies that the edge case of an empty/anonymous module does not panic or produce empty output.

#### Key Data Structures (from cssl-mir, used here)

**`MirModule`** — a top-level container holding an optional `name: Option<String>` and a `funcs: Vec<MirFunc>`. The bridge receives `&MirModule` and delegates all printing to `cssl_mir::print_module`.

#### Stubs, TODOs, Placeholders

There are no explicit `todo!()`, `unimplemented!()`, or `TODO` comments in `emit.rs`. The incompleteness is structural (the file's entire purpose is a thin wrapper), not marked in-code. The deferral of real MLIR work is documented in `lib.rs`, not here.

#### Cross-references

- `cssl_mir::MirModule` — input type
- `cssl_mir::print_module` — the actual implementation being wrapped
- The tests additionally import `cssl_mir::{IntWidth, MirFunc, MirType}`

---

## 4. SLICE NOTES

### 4.1 Test Coverage

**`cssl-lir`:** One test (`scaffold_version_present`) that does nothing but confirm the crate compiled and has a non-empty version string. Coverage: 0% of intended functionality, since no functionality is implemented.

**`cssl-mlir-bridge`:** Four tests total — one scaffold test in `lib.rs` and three meaningful tests in `emit.rs`. The three `emit.rs` tests provide reasonable coverage of the two public API functions across normal cases (named module with functions), the writer-vs-string consistency check, and the empty-module edge case. These tests actually exercise real code and would catch regressions in the textual emission path. Coverage of the *intended* FFI functionality: 0%, since no FFI is implemented.

### 4.2 What Is Incomplete or Stubbed

**`cssl-lir` — everything:**

The entire purpose of this crate — LIR types, target dispatch, fat-binary assembly — is absent. The crate is a pure name-holder. To make it real, the minimum work required would be:

1. Define a `LirModule` or equivalent type that holds lowered, target-specific representations.
2. Implement `fn lower_mir_to_lir(module: &MirModule, target: Target) -> LirModule` dispatch logic.
3. Implement `fn emit_fat_binary(lir: &LirModule, targets: &[TargetProfile]) -> Result<Vec<u8>>` assembling the `.cssl-bin` format from `specs/07_CODEGEN.csl § FAT-BINARY`.
4. Wire to `cssl-cgen-cpu-cranelift` for CPU object emission.
5. Wire to the GPU emitter crates (rspirv / DXIL shim / spirv-cross) for GPU blobs.
6. Add the symbol table, telemetry schema, and signed audit manifest to the fat-binary.

**`cssl-mlir-bridge` — the FFI layer (T6-phase-2), which is the entire stated purpose:**

What is missing:

1. Un-commenting `melior = "0.20"` and `mlir-sys = "0.3"` in `compiler-rs/Cargo.toml` and verifying Windows MSVC toolchain compatibility (`LLVM_SYS_*_PREFIX` environment setup).
2. Removing `#![forbid(unsafe_code)]` from `lib.rs` and replacing it with a targeted `#![allow(unsafe_code)]` scoped only to the FFI boundary, as the doc-comment pre-authorizes.
3. Implementing MLIR context initialization via `melior::Context`.
4. Registering the `cssl.*` dialect (the 26+ ops enumerated in `specs/15_MLIR.csl § CSSL-DIALECT OPS`) using melior's dialect registration API.
5. Writing the `MirOp → mlir::Operation` construction bridge for each op.
6. Implementing the pass pipeline (canonicalization → CSE → LICM → DCE → inliner → dialect conversion) using melior's pass manager.
7. Implementing dialect lowering: `spirv-dialect → rspirv` (stage0) and `llvm-dialect → Cranelift bridge` (stage0).
8. Writing the TableGen `.td` file (`include/cssl/Dialect/CSSL/IR/CSSLOps.td`) for the cssl dialect definition.

### 4.3 README / Spec Divergences

**cssl-lir:**  
- README description ("low-level IR + target orchestration") implies an implemented crate. The reality is a 22-line scaffold. This is not a spec error — the module doc-comment's `§ STATUS` note accurately flags it — but any new contributor reading the README will be misled.
- `specs/07_CODEGEN.csl` describes a detailed fat-binary format, CPU/GPU dispatch logic, and validation pipeline. None of this is represented in code.
- `specs/14_BACKEND.csl` describes a four-phase owned x86-64 backend (isel → regalloc → schedule → emit). This is explicitly stage1+ work, not stage0, so the absence here is by design. However, even the stage0 Cranelift path that IS in-scope for the current crate is not wired.

**cssl-mlir-bridge:**  
- `specs/15_MLIR.csl` specifies 26 `cssl.*` custom ops, four-stage pass pipelines, structured CFG preservation guarantees, and Transform-dialect integration. The bridge implements none of this.
- The `lib.rs` doc-comment notes the divergence explicitly and honestly at lines 11–22 (T6-phase-2 deferred list and fallback pre-authorization). This is transparent scaffolding, not silent incompleteness.
- The spec's `§ DIAGNOSTIC + DEBUGGING` section mentions `--emit-mlir` and `--dump-pass=<name>` flags. The textual emission path (T6-phase-1) supports `--emit-mlir` only. `--dump-pass` would require the actual pass pipeline to exist.

### 4.4 Structural Observations

**The thin-wrapper design of `emit.rs` is sound as a stabilization boundary.** The compiler driver (`csslc`) can already emit MLIR text without importing `cssl-mir` directly, and the API surface (`emit_module_to_string`, `emit_module_to_writer`) is exactly what a CLI driver needs. When T6-phase-2 arrives, these two functions will remain the public contract — their implementations will simply become more complex (running the real MLIR pass pipeline before printing).

**`cssl-lir` has no equivalent stabilization boundary.** Because it contains nothing, the compiler driver today must wire directly to `cssl-cgen-cpu-cranelift` (if at all). When the LIR layer is implemented, there will be an insertion point at the crate boundary, but there is no skeletal API yet to anchor that insertion.

**The melior Windows compatibility concern is legitimate.** `melior` and `mlir-sys` require LLVM system libraries to be present and pointed to via environment variables. On Windows with MSVC toolchain, this requires a full LLVM build from source or a pre-built LLVM distribution. The decision to defer this (T1-D7) and keep the comment-out is pragmatic for stage0 velocity.

**`STAGE0_SCAFFOLD` constant pattern** appears in at least three crates (`cssl-lir`, `cssl-mlir-bridge`, and `cssl-mir`). It is a workspace-wide convention for scaffold tracking — probably the scaffolding-presence test that CI can grep for to identify crates with no real implementation.

### 4.5 Surprises

- **`cssl-lir` has zero dependencies** — not even `cssl-mir`. This means it could not accept a `MirModule` even if it wanted to. The LIR layer is completely disconnected from the pipeline.
- **The bridge's textual emission tests in `emit.rs` are more thorough than most scaffolding** in this codebase tier. They verify string contents, round-trip consistency, and edge cases. This suggests the textual-emit path was taken seriously as a real deliverable, not just filler.
- **The workspace `Cargo.toml` comment tag `# T6 : verify-melior-windows-compat`** serves as a to-do tracker embedded in build configuration — consistent with the codebase's use of `§ STATUS` and `§ DEFERRED` in doc-comments.
- **`#![forbid(unsafe_code)]` in `cssl-mlir-bridge/src/lib.rs`** will need to be changed to allow-scoped-unsafe before T6-phase-2 can proceed. The doc-comment pre-documents this (`§ FFI SAFETY: unsafe_code is permitted here only at the melior FFI boundary`), but it is easy to forget when the time comes.

---

## Appendix: File Inventory

| File | LOC | Status |
|---|---|---|
| `compiler-rs/crates/cssl-lir/Cargo.toml` | 12 | Workspace-inherit only; no deps |
| `compiler-rs/crates/cssl-lir/src/lib.rs` | 22 | Pure scaffold: 1 const + 1 test |
| `compiler-rs/crates/cssl-mlir-bridge/Cargo.toml` | 15 | One dep: cssl-mir |
| `compiler-rs/crates/cssl-mlir-bridge/src/lib.rs` | 46 | Re-exports; scaffold const + test |
| `compiler-rs/crates/cssl-mlir-bridge/src/emit.rs` | 61 | Real code: 2 pub fns + 3 tests |

**Total audited source files:** 5 (3 Rust, 2 TOML)  
**Total items documented:** 14 (2 constants, 8 functions/tests, 1 module declaration, 1 re-export, 2 use-statements of structural significance)

---

*End of audit — compiler-rs/crates/cssl-lir + compiler-rs/crates/cssl-mlir-bridge*
