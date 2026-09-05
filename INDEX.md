# CSSLv3 / Sigil — Stage-0 Bootstrap Compiler: Master Index

**Synthesized from:** `docs/audit/01` through `17` (16 docs; `15-specs.md` absent — `[OPEN: A15 pending]`)
**Audit date:** 2026-05-14 | **Branch:** `claude/mystifying-bardeen-dcb4d6`
**Source of truth for everything below:** actual source files, not README claims.

---

## § 1 — Identity

CSSLv3 (also called **Sigil** throughout the spec corpus) is a hardware-first systems language designed around a foundational consent axiom: the PRIME_DIRECTIVE (`t∞: consent = OS · sovereignty = substrate-invariant`). It is not a research toy and not a web language. Its design goals are: zero-LLVM codegen (Cranelift JIT + SPIR-V/DXIL/MSL/WGSL for GPU), consent encoded *structurally* in the type system (not as policy bolted on top), six non-negotiable features (autodiff, refinement types, effect system, staged computation, information-flow control, and observability), and a Pony-6 capability system that eliminates data races by construction.

The language draws from a specific set of prior art: Pony for capabilities (iso/trn/ref/val/box/tag), Koka for row-polymorphic effects, Jif/DLM for information-flow control, Vale for generational references (`GenRef: u40 index + u24 generation` packed in `u64`), Racket for hygienic macros (set-of-scopes), and the Futamura projections for staged computation. None of these are imported as libraries; all are re-implemented from scratch in the stage-0 Rust host. This is intentional: the goal is a language whose semantics are precisely specified in its own spec corpus, not inherited from upstream library semantics.

This repository contains **stage-0**: a throwaway Rust-hosted bootstrap compiler whose job is to demonstrate that all six features are at minimum-viable depth. Stage-0 is not production-grade, not self-hosting, and many subsystems are deliberate scaffolds. The stage-0 compiler is not intended to survive contact with a real program; it is intended to prove the design space is implementable before stage-1 (the self-hosted compiler) is written in Sigil itself. The 32-crate Cargo workspace is the stage-0 artifact; it will be retired when stage-1 bootstraps.

The IR design is MLIR-dialect-shaped structured SSA (regions and blocks), not a flat three-address code. This is a deliberate choice to make lowering passes composable and to allow the future MLIR bridge to emit real MLIR without structural conversion. The current MLIR bridge (`cssl-mlir-bridge`) wraps only the MIR printer; the `melior`/`mlir-sys` FFI is commented out pending Windows compatibility verification.

**What is real:** the front-end (lexer, parser, type system, HM inference, effect-row unification, autodiff transform), the MIR lowering with monomorphization, the Cranelift CPU JIT, the 12-mode test harness, and the telemetry/audit chain skeleton.
**What is scaffold:** the `csslc` binary, all five host-GPU FFI crates, LIR, the MLIR bridge, cssl-ifc (standalone), cssl-rt, and the CSLv3-native lexer surface.
**What is hollow:** the SMT discharge() loop (always Sat), the MLIR bridge FFI (melior commented out), cssl-lir (22 LOC, no types), and cssl-rt (19 LOC, no logic).
**What is documented but not yet audited:** the `specs/` directory (`[OPEN: A15 pending]`).

---

## § 2 — Critical Findings (Front-Loaded)

Ten findings a new contributor must read before touching any code. All citations are verified against the audit documents listed in § 7.

### F-01: `csslc` binary does nothing
**File:** `compiler-rs/csslc/src/main.rs` · **LOC:** ~23
The `csslc` binary prints two lines (`"csslc — Sigil stage-0 bootstrap compiler"` and a version string) and exits 0. There is no argument parsing, no compiler invocation, no pipeline call. Every demo, benchmark, and integration test that claims to "run csslc" is calling the library crates directly, not this binary. A new contributor who runs `cargo run -p csslc -- myfile.cssl` will get a no-op exit.
**Action:** wire `main()` to the actual pipeline before claiming any end-to-end demo works.

### F-02: All five host-GPU FFI crates are pure scaffolds with zero real FFI
**Files:** `compiler-rs/cssl-host-vulkan/`, `cssl-host-level-zero/`, `cssl-host-d3d12/`, `cssl-host-metal/`, `cssl-host-webgpu/`
None of these crates list `ash`, `windows`, `wgpu`, or any GPU runtime in their own `[dependencies]`. The workspace `Cargo.toml` lists them as optional workspace deps but the individual crates do not depend on them. `level-zero-sys` is commented out entirely (`# T10 : verify-registry-availability`). `melior`/`mlir-sys` are also commented out (`# T6 : verify-melior-windows-compat`). These five crates are approximately 918, ~100, ~100, ~100, ~100 LOC respectively and contain only `pub fn init() -> Result<()> { todo!() }` style stubs. The Arc A770 hardware profile is hardcoded in Vulkan despite being Apocky's personal machine.
**Action:** before any GPU execution claim is valid, wire at least one backend end-to-end.

### F-03: SMT discharge() always returns Sat — F2 Refinement Types is semantically hollow
**File:** `compiler-rs/cssl-smt/src/solver.rs:216`
`build_stub_query()` ignores the obligation passed in and unconditionally emits `(assert true)` as the SMT-LIB query body. `discharge()` then calls Z3/CVC5 via CLI subprocess on that query, which always returns `Sat`. Every refinement type constraint in the system is therefore trivially "verified." This is not a partial implementation — it is a no-op wrapped in real-looking infrastructure. F2 refinement types have real *syntax* (query serialization, obligation structs, Z3/CVC5 dispatch) but no *semantics*.
**Action:** replace `build_stub_query` with an obligation-faithful query builder before F2 can be called implemented.

### F-04: F6 audit chain has a forgeable bypass
**File:** `compiler-rs/cssl-telemetry/src/audit.rs:329–344`
`verify_chain()` checks whether the stored signature matches the output of `stub_sign()` (which is a fixed 64-byte constant). If it matches, the function returns `Ok(())` without performing real Ed25519 verification via `ed25519-dalek`. A log that was signed with the stub key will pass chain verification unconditionally. This means the audit chain — which the PRIME_DIRECTIVE depends on for non-repudiation — is forgeable at stage-0.
**Action:** remove the stub-sign bypass path; require real Ed25519 verification unconditionally. This is a security issue regardless of stage.

### F-05: F5 IFC `combine_labels` unions both C and I sets (over-approximates DLM join)
**File:** `compiler-rs/cssl-hir/src/ifc.rs:583`
The DLM join of two labels should be `join(L1, L2) = (C1 ∩ C2, I1 ∪ I2)`. The actual implementation unions *both* the confidentiality and integrity sets: `(C1 ∪ C2, I1 ∪ I2)`. This over-approximates: it is *sound* (no information leaks) but *imprecise* (legitimate label combinations that should type-check are rejected). The audit document explicitly labels this as intentional stage-0 over-approximation, not an undetected bug. The `banned_composition` light-variant check is a near-no-op for PRIME_DIRECTIVE prohibited principals.
**Action:** document the over-approximation with a `// STAGE-0-APPROX` comment and a spec reference. Stage-1 must implement the correct DLM join.

### F-06: GPU SPIR-V backend maps two distinct capabilities to the same enum variant
**File:** `compiler-rs/cssl-cgen-gpu-spirv/src/binary_emit.rs:265–267`
`FloatControls2` and `ShaderNonSemanticInfo` both emit `spirv::Capability::Shader`. The SPIR-V spec requires distinct capability declarations for these. Any shader that relies on non-IEEE float controls or non-semantic debug extensions will silently mis-declare its capabilities and likely fail SPIR-V validation.
**Action:** add distinct `spirv::Capability::FloatControls2` and `spirv::Capability::ShaderNonSemanticInfo` variants (or use the correct existing ones from the spirv crate).

### F-07: README claims "1600+ tests passing" — actual record is 1049–1074
**File:** `README.md` (claim) vs `DECISIONS.md` (record at T11-D38 through T11-D50)
The decision log records test counts at multiple milestones in the 1049–1074 range. The README rounds this up to "1600+." This is not a rounding error; it is a ~50% inflation. `CONTRIBUTING.md` is more honest. Any CI dashboard or external claim based on the README test count is wrong.
**Action:** update README to reflect actual test count from the decision log.

### F-08: Three critical crates are empty scaffolds: cssl-lir (22 LOC), cssl-rt (19 LOC), cssl-ifc standalone (24 LOC)
**Files:** `compiler-rs/cssl-lir/src/lib.rs`, `compiler-rs/cssl-rt/src/lib.rs`, `compiler-rs/cssl-ifc/src/lib.rs`
All three contain only a module declaration and a `// TODO` comment. They have no tests, no types, no logic. The real IFC implementation lives in `cssl-hir/src/ifc.rs` (1,168 lines). cssl-lir is the planned lowering layer between MIR and codegen; its absence means the Cranelift backend reads MIR directly. cssl-rt is the planned runtime ABI; its absence means there is no standard runtime interface.
**Action:** do not count these toward any "crates implemented" metric.

### F-09: MIR pipeline has 1 real pass and 5 named stubs
**File:** `compiler-rs/cssl-mir/src/pipeline.rs`
`StructuredCfgValidator` is the only pass with real logic. Five additional passes are named (`ConstantFoldingPass`, `DeadCodeEliminationPass`, `InliningPass`, `LoopOptimizationPass`, `MemoryOptimizationPass`) but contain `todo!()` bodies. The pipeline therefore provides no optimization, no inlining, and no dead-code elimination at stage-0.
**Action:** document as intentional stage-0 scope; do not claim optimization passes in any external description.

### F-10: GPU backends are independent codegen paths, not funneled through SPIR-V
**Files:** `cssl-cgen-gpu-spirv/`, `cssl-cgen-gpu-dxil/`, `cssl-cgen-gpu-msl/`, `cssl-cgen-gpu-wgsl/`
This is a *design choice*, not a bug, but it contradicts the mental model many contributors will bring (where SPIR-V is the universal IR). Each backend reads MIR independently. All four backends currently return an error on any non-empty MIR function body — they emit valid empty module headers but cannot process real code. The independence means bugs must be fixed four times; it also means backends can be optimized independently.
**Action:** document the independent-path design prominently; gate any GPU demo behind a check that the target backend actually handles the input function.

---

## § 3 — Repository Layout

```
CSSLv3/                         (repo root)
├── INDEX.md                    ← this file
├── README.md                   ← public-facing; test count inflated (see F-07)
├── CONTRIBUTING.md             ← more accurate than README; good first-read
├── PRIME_DIRECTIVE.md          ← 626 lines; 17 prohibitions; §10 ToS; supersedes license
├── DECISIONS.md                ← 3336 lines; T1-D1 through T12-D2; authoritative history
├── ARCHITECTURE.md             ← [OPEN: A17 notes this may exist; verify before citing]
├── GLOSSARY.md                 ← [OPEN: A17 notes this may exist; verify before citing]
├── compiler-rs/                ← 32-crate Cargo workspace (MSRV 1.75)
│   ├── Cargo.toml              ← workspace manifest; melior+level-zero-sys commented out
│   ├── csslc/                  ← the binary (23 LOC; does nothing — see F-01)
│   ├── cssl-ast/               ← CST node types (~1,030 LOC; REAL)
│   ├── cssl-lex/               ← dual-surface lexer (~1,350 LOC; REAL)
│   ├── cssl-parse/             ← recursive-descent + Pratt (~4,573 LOC; REAL)
│   ├── cssl-hir/               ← HM inference + feature passes (21 files; REAL)
│   ├── cssl-caps/              ← Pony-6 capability checker (~1,217 LOC; REAL)
│   ├── cssl-effects/           ← effect-row system (~1,115 LOC; REAL)
│   ├── cssl-ifc/               ← standalone IFC crate (24 LOC; EMPTY — real impl in hir)
│   ├── cssl-autodiff/          ← source-to-source AD on MIR (~4,061 LOC; REAL)
│   ├── cssl-jets/              ← primitive jet schemas (293 LOC; PARTIAL — schema only)
│   ├── cssl-smt/               ← SMT-LIB emission + Z3/CVC5 dispatch (1,756 LOC; PARTIAL)
│   ├── cssl-staging/           ← @staged + Futamura P1 (455 LOC; PARTIAL)
│   ├── cssl-futamura/          ← Futamura P2/P3 specializer (284 LOC; PARTIAL)
│   ├── cssl-macros/            ← hygienic macro expander (343 LOC; PARTIAL)
│   ├── cssl-mir/               ← MIR IR + body lowering (~9,600 LOC; REAL)
│   ├── cssl-lir/               ← LIR scaffold (22 LOC; EMPTY)
│   ├── cssl-mlir-bridge/       ← MIR printer wrapper (107 LOC; no real FFI)
│   ├── cssl-cgen-cpu-cranelift/← Cranelift JIT + CLIF emitter (~3,852 LOC; REAL)
│   ├── cssl-cgen-gpu-spirv/    ← SPIR-V backend (2,123 LOC; PARTIAL — empty-fn only)
│   ├── cssl-cgen-gpu-dxil/     ← DXIL backend (1,096 LOC; PARTIAL — empty-fn only)
│   ├── cssl-cgen-gpu-msl/      ← MSL backend (929 LOC; PARTIAL — empty-fn only)
│   ├── cssl-cgen-gpu-wgsl/     ← WGSL backend (887 LOC; PARTIAL — empty-fn only)
│   ├── cssl-host-vulkan/       ← Vulkan host runtime (~918 LOC; SCAFFOLD — no FFI)
│   ├── cssl-host-level-zero/   ← Level Zero host runtime (SCAFFOLD — no FFI)
│   ├── cssl-host-d3d12/        ← D3D12 host runtime (SCAFFOLD — no FFI)
│   ├── cssl-host-metal/        ← Metal host runtime (SCAFFOLD — no FFI)
│   ├── cssl-host-webgpu/       ← WebGPU host runtime (SCAFFOLD — no FFI)
│   ├── cssl-telemetry/         ← R18 telemetry + audit chain (1,518 LOC; REAL with bugs)
│   ├── cssl-persist/           ← persistence layer (610 LOC; PARTIAL — in-memory only)
│   ├── cssl-rt/                ← runtime ABI (19 LOC; EMPTY)
│   ├── cssl-testing/           ← 12-mode test harness (~3,730 LOC; REAL)
│   └── cssl-examples/          ← integration examples (~6,671 LOC; REAL — JIT + AD demo)
├── docs/
│   ├── audit/                  ← 16 detailed audit reports (01–17, excluding 15)
│   │   └── [OPEN: 15-specs.md absent — pending A15 spec audit]
│   └── [other docs]
├── research/                   ← 14 design-survey files (read-only reference)
├── stage1/                     ← 2 placeholder .cssl files (scaffold only)
└── scripts/                    ← 3 Python utilities (codegen helpers)
```

---

## § 4 — 32-Crate Workspace Table

Maturity assessed from audit source, not README claims. LOC counts are approximate from audit reports.

| # | Crate | LOC | Maturity | Audit | Key fact |
|---|-------|-----|----------|-------|----------|
| 1 | `csslc` | 23 | EMPTY | A14 | Binary prints 2 lines, exits 0 |
| 2 | `cssl-ast` | ~1,030 | REAL | A01 | Surface-agnostic CST; SurfaceKind enum |
| 3 | `cssl-lex` | ~1,350 | REAL | A01 | Dual-surface: logos + hand-rolled CSLv3 |
| 4 | `cssl-parse` | ~4,573 | REAL | A02 | Recursive-descent + Pratt; CSLv3-surface structurally correct but stub at S0 |
| 5 | `cssl-hir` | 21 files, 210+ items | REAL | A03 | HM + effect-row; real IFC at `ifc.rs` (1,168 LOC); feature passes embedded |
| 6 | `cssl-caps` | ~1,217 | REAL | A04 | Pony-6 iso/trn/ref/val/box/tag; capability lattice |
| 7 | `cssl-effects` | ~1,115 | REAL | A04 | 32 builtin effects (spec says 28+); row-polymorphic; closes over `cssl-hir` |
| 8 | `cssl-ifc` | 24 | EMPTY | A04 | Standalone crate is a stub; real impl in `cssl-hir/src/ifc.rs` |
| 9 | `cssl-autodiff` | ~4,061 | REAL | A05 | 38-rule JVP/VJP table; 19 primitives; piecewise-linear handled; @lipschitz stub |
| 10 | `cssl-jets` | 293 | PARTIAL | A05 | Schema + type stubs only; no runtime execution; staging deferred to T8 |
| 11 | `cssl-smt` | 1,756 | PARTIAL | A06 | Z3/CVC5 dispatch real; `build_stub_query` always asserts true (F-03) |
| 12 | `cssl-staging` | 455 | PARTIAL | A06 | @staged annotation + P1 specialization; P2/P3 deferred |
| 13 | `cssl-futamura` | 284 | PARTIAL | A06 | P2 interpreter-specializer and P3 compiler-generator stubs |
| 14 | `cssl-macros` | 343 | PARTIAL | A06 | Racket set-of-scopes hygiene; expansion driver real; eval deferred |
| 15 | `cssl-mir` | ~9,600 | REAL | A07 | All 31 HirExprKind variants lowered; monomorphization quartet complete; 1 real pipeline pass |
| 16 | `cssl-lir` | 22 | EMPTY | A08 | Pure scaffold; no types, no logic |
| 17 | `cssl-mlir-bridge` | 107 | SCAFFOLD | A08 | Wraps MIR printer only; no melior FFI; melior commented out in workspace |
| 18 | `cssl-cgen-cpu-cranelift` | ~3,852 | REAL | A09 | Two modes: CLIF text + real JIT; scalars/cmp/calls/libm live; control-flow/SIMD deferred |
| 19 | `cssl-cgen-gpu-spirv` | 2,123 | PARTIAL | A10 | Independent path from MIR; empty-fn modules only; FloatControls2 bug (F-06) |
| 20 | `cssl-cgen-gpu-dxil` | 1,096 | PARTIAL | A10 | Independent path; empty-fn only; no real DXIL instruction emission |
| 21 | `cssl-cgen-gpu-msl` | 929 | PARTIAL | A10 | Independent path; empty-fn only |
| 22 | `cssl-cgen-gpu-wgsl` | 887 | PARTIAL | A10 | Independent path; empty-fn only |
| 23 | `cssl-host-vulkan` | ~918 | SCAFFOLD | A11 | No ash dep in crate; Arc A770 hardcoded |
| 24 | `cssl-host-level-zero` | ~100 | SCAFFOLD | A11 | level-zero-sys commented out entirely |
| 25 | `cssl-host-d3d12` | ~100 | SCAFFOLD | A11 | No windows dep in crate |
| 26 | `cssl-host-metal` | ~100 | SCAFFOLD | A11 | No metal dep in crate |
| 27 | `cssl-host-webgpu` | ~100 | SCAFFOLD | A11 | No wgpu dep in crate |
| 28 | `cssl-telemetry` | 1,518 | REAL+BUGS | A12 | OtlpExporter NotWired; ChromeTrace unclosed JSON; audit bypass (F-04); TelemetrySlot 68 not 64 bytes |
| 29 | `cssl-persist` | 610 | PARTIAL | A12 | In-memory only; no disk/DB backend |
| 30 | `cssl-rt` | 19 | EMPTY | A12 | Module decl + `// TODO` only |
| 31 | `cssl-testing` | ~3,730 | REAL | A13 | 12 oracle modes; 9 live; 3 stub (power/thermal/hot_reload) |
| 32 | `cssl-examples` | ~6,671 | REAL | A14 | Killer-app gate, JIT execution, R18 attestation, symbolic AD verification |

**Maturity key:** REAL = core logic present and tested · PARTIAL = real infrastructure, hollow semantics in at least one critical path · SCAFFOLD = types/signatures only, no logic · EMPTY = `// TODO` or minimal module decl only · REAL+BUGS = substantive logic with documented correctness issues

---

## § 5 — Compiler Pipeline (ASCII, with Maturity Annotations)

```
Source text (.cssl or CSLv3-native)
         │
         ▼
┌─────────────────────────────────────────────────────┐
│  cssl-lex  [REAL]                                   │
│  Dual-surface lexer                                 │
│  · Rust surface: logos-based, production-ready      │
│  · CSLv3-native surface: hand-rolled; structurally  │
│    correct but tokenization is stage-0 stub         │
└─────────────────────────┬───────────────────────────┘
                          │ Token stream
                          ▼
┌─────────────────────────────────────────────────────┐
│  cssl-ast + cssl-parse  [REAL]                      │
│  Recursive-descent + Pratt parser                   │
│  · Produces surface-agnostic CST                    │
│  · All 31 expression kinds parsed                   │
│  · Error recovery via synchronization points        │
└─────────────────────────┬───────────────────────────┘
                          │ CST
                          ▼
┌─────────────────────────────────────────────────────┐
│  cssl-hir  [REAL]                                   │
│  HIR + type inference hub                           │
│  · HM with effect-row unification                   │
│  · Feature passes embedded:                         │
│    - ad_legality (autodiff preconditions)           │
│    - refinement check (calls cssl-smt → hollow)     │
│    - ifc (IFC labeling — over-approx, see F-05)     │
│    - staged_check (phasing validation)              │
│    - macro_hygiene (set-of-scopes)                  │
│  · Closes over cssl-caps + cssl-effects             │
└──────┬──────────────────┬──────────────────┬────────┘
       │                  │                  │
       ▼                  ▼                  ▼
 cssl-caps [REAL]  cssl-effects [REAL]  cssl-smt [PARTIAL]
 Pony-6 caps       32 builtin effects   discharge() → always Sat
 lattice check     row-poly unify       (see F-03)

                          │ HIR (type-checked)
                          ▼
┌─────────────────────────────────────────────────────┐
│  cssl-autodiff  [REAL]                              │
│  Source-to-source AD on MIR                         │
│  · 38-rule JVP/VJP table                            │
│  · 19 primitives incl. piecewise-linear             │
│  · @lipschitz constant always recorded as "k"       │
│  · Consumes/emits MIR                               │
└─────────────────────────┬───────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────┐
│  cssl-staging + cssl-futamura + cssl-macros          │
│  [PARTIAL — infrastructure real, semantics partial] │
│  · @staged annotation processing                    │
│  · Futamura P1 specialization                       │
│  · P2/P3 stubs                                      │
│  · Hygienic macro expansion (eval deferred)         │
└─────────────────────────┬───────────────────────────┘
                          │ HIR
                          ▼
┌─────────────────────────────────────────────────────┐
│  cssl-mir  [REAL]                                   │
│  MLIR-dialect-shaped MIR (SSA + regions + blocks)  │
│  · body_lower.rs covers all 31 HirExprKind variants │
│  · Monomorphization quartet complete (T11-D38–D50)  │
│  · Pipeline passes: 1 real + 5 stubs (see F-09)    │
│  · Known gap: let-bindings not tracked in           │
│    param_vars; Implies/Entails → verify.assert      │
│    stub; MirType::None for if/for/while/loop        │
└───────────────────────┬─────────────────────────────┘
                        │ MIR
          ┌─────────────┼──────────────────────┐
          ▼             ▼                      ▼
┌──────────────┐ ┌──────────────┐    ┌────────────────────┐
│ cssl-lir     │ │ cssl-mlir-   │    │ GPU backends       │
│ [EMPTY]      │ │ bridge       │    │ [PARTIAL]          │
│              │ │ [SCAFFOLD]   │    │ · spirv (2,123 LOC)│
│ Planned      │ │ MIR printer  │    │ · dxil (1,096 LOC) │
│ lowering IR  │ │ only; no     │    │ · msl  (929 LOC)   │
│ between MIR  │ │ melior FFI   │    │ · wgsl (887 LOC)   │
│ and codegen  │ │              │    │ All: empty-fn only │
└──────────────┘ └──────────────┘    │ Independent paths  │
                                     │ (see F-10)         │
                                     └────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────┐
│  cssl-cgen-cpu-cranelift  [REAL]                    │
│  Two modes:                                         │
│  · CLIF text emitter (for debugging)                │
│  · Real JIT via cranelift-jit                       │
│  Handles: scalars, comparisons, function calls,     │
│           libm transcendentals, basic control flow  │
│  Deferred: memref, SIMD, advanced control flow      │
│  Note: Bf16→F16 approximation (not native Bf16)     │
└─────────────────────────┬───────────────────────────┘
                          │ Native code
                          ▼
                    Host execution
                    (cssl-rt: EMPTY — no standard ABI)
```

**Observation:** The pipeline has no `csslc` driver. End-to-end compilation requires calling library crates directly (see `cssl-examples`).

---

## § 6 — F1–F6 Feature Status Table

| Feature | Name | Crate(s) | Status | Evidence | Gap |
|---------|------|----------|--------|----------|-----|
| F1 | Autodiff | `cssl-autodiff` | REAL | 38-rule table, 19 primitives, JVP/VJP both implemented, piecewise-linear handled | `@lipschitz` constant always "k" (stub); `cssl-jets` schema only |
| F2 | Refinement Types | `cssl-smt`, `cssl-hir` | HOLLOW | Z3/CVC5 subprocess dispatch works; obligation structs exist; HIR pass calls `discharge()` | `build_stub_query` asserts `(assert true)` — every constraint trivially Sat (F-03) |
| F3 | Effect System | `cssl-effects`, `cssl-hir` | REAL | 32 builtin effects; row-polymorphic; unification integrated in HIR | Spec documents 28+ builtins; count discrepancy may indicate undocumented additions |
| F4 | Staged Computation | `cssl-staging`, `cssl-futamura` | PARTIAL | @staged annotation processing; P1 specialization | P2 interpreter-specializer and P3 compiler-generator are named stubs |
| F5 | Information Flow Control | `cssl-hir/src/ifc.rs` | PARTIAL | 1,168 LOC; real label lattice; policy enforcement; 9 PRIME_DIRECTIVE principals | `combine_labels` over-approximates DLM join (C also unioned, see F-05); `banned_composition` light-variant near-no-op; standalone `cssl-ifc` crate is empty |
| F6 | Observability | `cssl-telemetry`, `cssl-persist` | PARTIAL+BUG | BLAKE3 hashing real; Ed25519-dalek linked; telemetry slot infrastructure real | Audit-chain bypass via stub-sign path (F-04); OtlpExporter NotWired; ChromeTrace unclosed JSON; `cssl-persist` in-memory only; `cssl-rt` empty |

**Summary:** F1 and F3 are genuine stage-0 implementations. F4 and F5 are functional at depth-1 but have named gaps. F2 is infrastructure with hollow semantics. F6 has a security-class defect in the verification path.

---

## § 7 — Navigation Guide

**"I want to understand the overall design intent"**
→ Read `PRIME_DIRECTIVE.md` (consent axiom, 17 prohibitions, §10 ToS)
→ Read `CONTRIBUTING.md` (accurate stage-0 framing, good scaffolding context)
→ Read `DECISIONS.md` T1-D1 through T3 (foundational design decisions)
→ Avoid: `README.md` (test count wrong; stage-0 caveats understated)

**"I want to understand the type system"**
→ `docs/audit/03-typesys-hir.md` — HM inference, effect-row unification, feature passes
→ `compiler-rs/cssl-hir/src/` — 21 files; start with `infer.rs` and `ifc.rs`
→ `docs/audit/04-typesys-caps-effects-ifc.md` — Pony-6 caps + effect builtins + IFC lattice

**"I want to contribute to the front-end (lex/parse)"**
→ `docs/audit/01-frontend-lex-ast.md` — dual-surface design, CST node types
→ `docs/audit/02-frontend-parse.md` — Pratt parser, precedence table, CSLv3-native gaps
→ Start: `compiler-rs/cssl-lex/src/` and `compiler-rs/cssl-parse/src/`

**"I want to understand MIR and IR lowering"**
→ `docs/audit/07-ir-mir.md` — all 31 HirExprKind variants, monomorphization, pipeline stubs
→ `compiler-rs/cssl-mir/src/body_lower.rs` — primary lowering file
→ `docs/audit/08-ir-lir-mlir-bridge.md` — LIR/MLIR-bridge limitations

**"I want to work on autodiff (F1)"**
→ `docs/audit/05-transform-autodiff-jets.md` — 38-rule table, primitive list, jet schema
→ `compiler-rs/cssl-autodiff/src/` — real implementation
→ Note: `cssl-jets` is schema only; runtime execution not wired

**"I want to fix F2 (refinement types / SMT)"**
→ `docs/audit/06-transform-smt-staging-futamura-macros.md` — `build_stub_query` analysis
→ `compiler-rs/cssl-smt/src/solver.rs:216` — the stub query builder to replace
→ SMT-LIB 2.6 spec for correct query serialization

**"I want to work on GPU codegen"**
→ `docs/audit/10-codegen-gpu.md` — four backends, capability mapping bug, empty-fn limitation
→ Start with SPIR-V: `compiler-rs/cssl-cgen-gpu-spirv/src/`
→ Fix F-06 first: `binary_emit.rs:265–267`
→ All four backends need: body emission for non-trivial MIR functions

**"I want to understand CPU JIT (Cranelift)"**
→ `docs/audit/09-codegen-cpu-cranelift.md` — two modes, what's live, what's deferred
→ `compiler-rs/cssl-cgen-cpu-cranelift/src/` — main codegen implementation
→ `cssl-examples` — has real JIT execution demo

**"I want to understand the audit/telemetry chain (F6)"**
→ `docs/audit/12-observability-persist-rt.md` — all three crates, the bypass bug
→ `compiler-rs/cssl-telemetry/src/audit.rs:329–344` — the forgeable bypass (F-04)
→ Fix: remove `stub_sign` path from `verify_chain`

**"I want to run tests"**
→ `docs/audit/13-testing.md` — 12 oracle modes, which 9 are live
→ `compiler-rs/cssl-testing/src/` — test harness
→ `cargo test -p cssl-testing` — recommended entry point
→ Note: actual count ~1049–1074, not 1600+

**"I want to understand the PRIME_DIRECTIVE and IFC integration"**
→ `PRIME_DIRECTIVE.md` — canonical; 17 prohibitions + §10 ToS
→ `docs/audit/04-typesys-caps-effects-ifc.md` — IFC lattice, 9 principals, `banned_composition`
→ `compiler-rs/cssl-hir/src/ifc.rs` — real implementation (1,168 LOC)
→ Note: `combine_labels` over-approximates (F-05); document as intentional before fixing

**"I want to know what's genuinely done vs. aspirational"**
→ This document, §§ 2, 4, 6 — the critical findings and maturity table
→ `DECISIONS.md` — decisions record actual implementation milestones with test counts
→ `CONTRIBUTING.md` — honest about scaffold and deferred phases

**"I want to understand the spec corpus"**
→ `docs/audit/16-research-stage1-scripts.md` — research/ and stage1/ survey
→ `research/` directory — 14 design-survey files (read-only; defines design space)
→ `[OPEN: A15 spec audit pending — specs/ directory not yet audited]`
→ `DECISIONS.md` — decisions often cite spec sections; cross-reference there

---

## § 8 — Known Issues Categorized

### SECURITY

**S-01: Forgeable audit chain** (CRITICAL)
`cssl-telemetry/src/audit.rs:329–344` — `verify_chain()` bypasses Ed25519 verification if stored signature matches `stub_sign()` output. The PRIME_DIRECTIVE's non-repudiation guarantee depends on this chain. Any log signed with the stub key (which is a fixed constant) passes verification unconditionally.
**Priority:** fix before any production or shared use. The bypass must be removed, not just documented.

**S-02: IFC over-approximation for PRIME_DIRECTIVE prohibitions** (MODERATE)
`cssl-hir/src/ifc.rs:583` — `combine_labels` unions both C and I sets. The `banned_composition` check for PRIME_DIRECTIVE prohibited principals (harm, control, manipulation, surveillance, exploitation, coercion, weaponization, discrimination) uses the light-variant path, which is near-no-op. Information flows involving prohibited principals are not structurally blocked at stage-0.
**Priority:** document as known stage-0 limitation in PRIME_DIRECTIVE.md. Stage-1 must implement correct DLM join with strict principal enforcement.

### CORRECTNESS

**C-01: SMT discharge always returns Sat** (HIGH)
`cssl-smt/src/solver.rs:216` — `build_stub_query` ignores obligation content. All refinement type constraints are trivially satisfied. Programs with real type-safety violations in refinement constraints will pass type-checking.
**Priority:** cannot claim F2 is implemented until this is replaced.

**C-02: GPU FloatControls2/ShaderNonSemanticInfo capability collision** (MODERATE)
`cssl-cgen-gpu-spirv/src/binary_emit.rs:265–267` — both capabilities emit `spirv::Capability::Shader`. SPIR-V validation will reject shaders that depend on either capability.
**Priority:** fix before any GPU shader validation test.

**C-03: MIR let-bindings not tracked in param_vars** (LOW)
`cssl-mir/src/body_lower.rs` — let-binding variables are not added to param_vars during lowering. Downstream passes that iterate param_vars may silently miss let-bound names. Symptom: incorrect variable availability in optimization passes (when implemented).
**Priority:** fix when optimization passes are implemented.

**C-04: MirType::None assigned for if/for/while/loop expressions** (LOW)
`cssl-mir/src/body_lower.rs` — control-flow expressions that yield values get `MirType::None` instead of their actual type. Type propagation downstream is incorrect for value-producing control flow.
**Priority:** fix before any type-directed optimization.

**C-05: Implies/Entails HirExpr variants map to `cssl.verify.assert` stub in MIR** (LOW)
`cssl-mir/src/body_lower.rs` — logical implication and entailment are lowered to a verification assertion stub rather than a real semantics. Any program that depends on logical reasoning over these will silently get wrong behavior.
**Priority:** fix when F2 (SMT) is unfaked.

**C-06: TelemetrySlot documented as 64 bytes, actually 68 bytes** (LOW)
`cssl-telemetry/` — struct layout mismatch. Any code that assumes 64-byte aligned slots (e.g., SIMD loads, cache-line alignment) will have incorrect assumptions.
**Priority:** fix or update documentation to match reality.

**C-07: ChromeTraceExporter produces unclosed JSON array** (LOW)
`cssl-telemetry/` — output file is not valid JSON until explicitly closed. Any consumer that reads the file mid-run will fail to parse it.
**Priority:** fix before any production trace collection.

**C-08: @lipschitz constant always recorded as "k" regardless of argument** (LOW)
`cssl-autodiff/` — the Lipschitz constant annotation always stores the string literal "k" rather than the actual value passed. Downstream analyses that depend on the constant are getting wrong data.
**Priority:** fix when Lipschitz-constrained optimization is implemented.

### SPEC-DIVERGENCE

**SD-01: cssl-effects has 32 builtin effects; spec documents 28+** (INFORMATIONAL)
`cssl-effects/` — four effects exist in the codebase that are not in the spec or are under-specified. This may be intentional extension; it may be divergence. No audit decision available.
**Action:** compare effect list against spec corpus; update one to match the other.

**SD-02: F5 IFC combine_labels uses ∪ for both C and I; DLM specifies ∩ for C** (MODERATE)
`cssl-hir/src/ifc.rs:583` — documented as intentional stage-0 over-approximation in audit-04. The spec (PRIME_DIRECTIVE + Jif-DLM reference) requires the correct join. Stage-1 must diverge from stage-0 here.
**Action:** add `// STAGE-0-APPROX: DLM join requires C1∩C2; stage-1 must fix` comment.

**SD-03: README claims "1600+ tests passing"; DECISIONS.md records 1049–1074** (HIGH)
See F-07. External communications based on README are misleading.
**Action:** update README immediately.

**SD-04: Futamura P2/P3 stubs present but spec describes full Futamura projection** (LOW)
`cssl-futamura/`, `cssl-staging/` — P1 is partially real; P2 and P3 are named stubs. The spec (F4) describes all three projections. Stage-0 scope note: P1 at minimum-viable is acceptable per design.
**Action:** document as stage-0 intentional scope in F4 spec references.

**SD-05: `15-specs.md` absent from audit corpus** [OPEN: A15 pending]
The specs/ directory has not been audited. Any claims about spec completeness or spec-to-code alignment are based on DECISIONS.md and audit cross-references only.
**Action:** complete A15 before any spec-completeness claims.

### DEAD-CODE

**DC-01: Five host-GPU FFI crates are unreachable from any real codepath**
`cssl-host-vulkan/`, `cssl-host-level-zero/`, `cssl-host-d3d12/`, `cssl-host-metal/`, `cssl-host-webgpu/` — no crate in the workspace depends on these. They are not called by any example, test, or binary. They are not dead in the sense of "compiled but uncalled" — they are not even depended upon.
**Action:** either wire at least one backend to a real codepath or clearly gate them as T8+ work.

**DC-02: cssl-lir has no dependents**
`cssl-lir/` — 22 LOC; no crate uses it; Cranelift backend reads MIR directly. The LIR layer is architecturally intended to sit between MIR and codegen but is currently bypassed.
**Action:** document bypass explicitly in `cssl-lir/src/lib.rs`.

**DC-03: cssl-rt has no dependents**
`cssl-rt/` — 19 LOC; no crate uses it. The Cranelift JIT calls into C ABI directly; no standard runtime interface is used.

**DC-04: cssl-mlir-bridge has no real FFI and melior is commented out**
`cssl-mlir-bridge/` — wraps the MIR printer; the melior dependency that would give it actual MLIR lowering capability is commented out in workspace Cargo.toml.

### COSMETIC

**CO-01: Arc A770 hardware profile hardcoded in cssl-host-vulkan**
Should be runtime-detected; currently a compile-time constant. Aesthetically fine for stage-0 if documented.

**CO-02: OtlpExporter always returns NotWired**
`cssl-telemetry/` — function signature and wiring point exist but the actual OpenTelemetry HTTP export is not implemented. Cosmetic at stage-0 since local trace files still work.

**CO-03: `cssl-jets` has 293 LOC of schema/type stubs but no runtime**
The schema is useful design documentation but the crate cannot execute. Mark as documentation crate until T8.

**CO-04: `stage1/` directory has 2 placeholder `.cssl` files**
These are not parsed, not compiled, not tested. They are design placeholders. Do not confuse with real stage-1 progress.

**CO-05: `research/` has 14 design-survey files that are not cross-referenced from any code**
Useful background reading; not canonical design docs. DECISIONS.md is authoritative for design choices.

---

## § 9 — Build and Run Quickstart

### Prerequisites

- Rust toolchain: MSRV **1.75**, stable channel
- Windows recommended (D3D12/DXIL backends assume Windows; Metal assumes macOS)
- Z3 or CVC5 CLI in PATH for SMT tests (F2 — currently always Sat regardless)
- No external C deps required for core build (LLVM-free by design)

### Build

```sh
# Full workspace build
cd compiler-rs
cargo build --workspace

# Note: melior (MLIR) and level-zero-sys are commented out in Cargo.toml.
# Build will succeed without them. Any feature requiring them is not wired.
```

### Test

```sh
# Run the test suite (actual count ~1049-1074, not 1600+)
cargo test --workspace

# Run a specific crate
cargo test -p cssl-testing
cargo test -p cssl-mir
cargo test -p cssl-autodiff

# Run examples (these exercise the real JIT path)
cargo run -p cssl-examples
```

The `cssl-testing` crate provides a 12-mode test oracle:

| Mode | Status | Description |
|------|--------|-------------|
| property | LIVE | QuickCheck-style property tests |
| metamorphic | LIVE | Relation-based; catches output inconsistency |
| fuzz | LIVE | Structure-aware fuzzing |
| bench | LIVE | Criterion-backed throughput benchmarks |
| golden | LIVE | Expected-output snapshot tests |
| differential | LIVE | Cross-backend output comparison |
| r16_attestation | LIVE | PRIME_DIRECTIVE compliance attestation (R16) |
| replay | LIVE | Recorded-session replay |
| audit | LIVE | Telemetry chain integrity check |
| power | STUB | Power consumption oracle (not implemented) |
| thermal | STUB | Thermal budget oracle (not implemented) |
| hot_reload | STUB | Hot-reload test mode (not implemented) |

The `r16_attestation` and `audit` modes are particularly important: they exercise the PRIME_DIRECTIVE enforcement path and the telemetry audit chain. Both will pass currently even with the F-04 bypass present, because the test harness calls `verify_chain` which uses the stub path. After fixing F-04, these modes will become meaningful gatekeepers.

### Run the "compiler"

```sh
# WARNING: This does nothing useful at stage-0
cargo run -p csslc

# Actual compilation requires calling library crates directly.
# See cssl-examples/src/ for a working end-to-end path.
```

### What actually executes end-to-end

The `cssl-examples` crate is the closest thing to a working end-to-end demo:
- Parses a minimal CSSL program
- Runs it through HIR + type inference
- Lowers to MIR (monomorphization included)
- JIT-compiles via Cranelift
- Executes and checks result
- Records a telemetry audit entry (with BLAKE3; Ed25519 stub at stage-0)

GPU execution is not available at stage-0. All host-GPU crates are scaffold.

### Workspace Cargo.toml notes

- `melior`/`mlir-sys` commented out — do not uncomment without testing on Windows
- `level-zero-sys` commented out — do not uncomment without verifying crate registry availability
- Aggressive clippy profile: `#![allow(dead_code)]` and `#![allow(unused_imports)]` are permitted as scaffold allowances (see workspace-level clippy config)

---

## § 10 — Where to Go Next

### For new contributors

1. Read `PRIME_DIRECTIVE.md` completely. The consent axiom is not optional and is structurally encoded. Understand the 17 prohibitions before touching IFC or audit code.
2. Read `CONTRIBUTING.md`. It is accurate and frames the scaffold vs. real split correctly.
3. Run `cargo test --workspace` and get it green on your machine.
4. Pick a CORRECTNESS or SECURITY issue from § 8 above — these have the highest leverage.

### Highest-leverage unblocking work (in priority order)

1. **Fix F-04 (audit bypass)** — `cssl-telemetry/src/audit.rs:329–344`. Remove the `stub_sign` bypass path. This is a security issue and relatively small in scope.
2. **Fix F-07 (README test count)** — Update README.md to reflect actual test count from DECISIONS.md (1049–1074). External-facing accuracy matters.
3. **Fix F-03 (SMT hollow discharge)** — Replace `build_stub_query` in `cssl-smt/src/solver.rs:216` with an obligation-faithful query builder. This unblocks F2 from being a real feature.
4. **Wire csslc binary (F-01)** — Implement argument parsing and pipeline invocation in `csslc/src/main.rs`. This enables real end-to-end testing from the command line.
5. **Fix GPU capability collision (F-06)** — `cssl-cgen-gpu-spirv/src/binary_emit.rs:265–267`. Small fix, high correctness impact.

### Design advancement (stage-0 → stage-1 prerequisites)

Stage-1 will be written in Sigil itself. The stage-0 compiler must be stable enough to parse and type-check stage-1 source before stage-1 can bootstrap. That means: every feature pass in HIR must produce correct output (not just syntactically valid output), the MIR pipeline must be reliable enough to lower arbitrarily complex programs, and the Cranelift JIT must handle all control-flow patterns stage-1 source will use. Current gaps:

- **F5 IFC:** implement correct DLM join — `combine_labels` must use ∩ for C, not ∪. The over-approximation is currently documented as intentional; it must be lifted before any security-critical program can be type-checked correctly.
- **F4 staged:** implement Futamura P2 (interpreter specializer) and P3 (compiler generator). P1 is partially real. Without P2/P3, the "staged computation" claim covers only partial evaluation, not the full Futamura projection model.
- **MIR pipeline:** implement at least constant folding and dead-code elimination. Currently 1 real pass (StructuredCfgValidator) and 5 named stubs. Any program with dead let bindings or constant-foldable arithmetic passes through unoptimized.
- **cssl-lir:** design and implement the LIR lowering layer. Currently MIR is read directly by the Cranelift backend. LIR is the planned intermediate that makes the backend pluggable; without it, adding a second CPU backend (e.g., QBE) requires forking the MIR representation.
- **Host-GPU:** wire at least one GPU backend end-to-end. Recommend Vulkan/ash on Windows (cssl-host-vulkan is the most developed scaffold). All five host crates are currently unreachable from any codepath.
- **cssl-rt:** design and implement the runtime ABI. Currently empty. Required for any multi-function program that uses the standard calling convention, stack unwinding, or runtime-dispatch traits.
- **cssl-ifc standalone:** the `cssl-ifc` crate (24 LOC) must become the canonical home for IFC logic. Currently all real IFC logic lives in `cssl-hir/src/ifc.rs`, which creates a circular dependency risk as IFC is needed by codegen passes that must not depend on the full HIR.

### Spec gaps

- `[OPEN: A15 pending]` — the `specs/` directory has not been audited. The `research/` directory contains 14 design-survey files that informed the spec, but the canonical spec corpus has not been cross-referenced against implementing crates. Before stage-1 design begins, every spec file should be verified against its counterpart crate. The audit workflow used for docs/audit/01–17 should be applied identically to specs/.
- `[OPEN: ARCHITECTURE.md and GLOSSARY.md]` — audit-17 notes these files may exist at repo root but they were not confirmed as present during the audit pass. Verify on disk and link from this document if they exist; if they do not exist, create stubs with the same primary-source-validated discipline used here.
- `[OPEN: stage1/ .cssl files]` — two placeholder `.cssl` source files exist in `stage1/`. They are not parsed, compiled, or tested at stage-0. Their content should be reviewed against the stage-1 design intent before any stage-1 work begins, to avoid encoding stale assumptions into the first real Sigil programs.

### Decision log cross-reference

Key decision clusters in `DECISIONS.md`:
- T1-D1 through T3: foundational design (consent axiom, F1-F6 commitments)
- T11-D38 through T11-D50: monomorphization quartet (the most recent major completed feature)
- T12-D1, T12-D2: most recent decisions (consult for current work-in-progress)

The decision log is the authoritative record of why things are the way they are. When a codebase choice looks wrong, check DECISIONS.md before filing a bug.

---

*Synthesized by audit pass over 16 audit documents (01–17 excluding 15-specs.md). Primary-source validated: all critical findings (F-01 through F-10) verified against cited files and line numbers before inclusion. [OPEN: A15] marker indicates spec audit not yet available.*
