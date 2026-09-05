# docs/audit — Navigation Index

**Generated:** 2026-05-14  
**Auditor:** Claude Sonnet 4.6 (agent session mystifying-bardeen-dcb4d6)

---

## Purpose

This folder is the output of a full repository audit conducted on 2026-05-14 covering every tracked source file in the CSSLv3 stage-0 ("Sigil") 32-crate Cargo workspace. The audit was a single-session, parallel-agent effort that read every `.rs` file, every `Cargo.toml`, every spec and script and governance document, and reported honestly on what each unit does, what it defers, and where the gaps are.

Each document in this folder is a thorough English-prose audit of one slice of the codebase: a named group of related crates, directories, or files. "Slice" means the audit was scoped, not a drive-by. Each doc identifies every public item, notes every deferred phase, measures maturity, flags discrepancies between code and spec, and calls out bugs found by static reading. The docs are long by design: the value is the specificity, not the summary.

**How to use this folder.** Find the doc or docs that cover your area of interest using the reading-order guide and the doc-by-doc index below. Read the full doc, not just the summary section — the headline findings in this index are entry points, not substitutes. Where a concern spans multiple docs, both docs are noted explicitly in the cross-references section. For known issues and broken things, the `INDEX.md` "Known Issues" section at the repo root remains the authoritative bug tracker; this audit supplements it but does not replace it.

**What is not here.** `15-specs.md` (the spec-corpus audit) is absent from this folder. See the OPEN marker at that entry in the index below.

---

## Reading Order by Goal

Pick the path that matches what you want to understand. Docs are listed by number; you can open them directly from this folder.

**"I want to understand the compiler frontend (lexer and concrete syntax tree)"**  
Start with `01-frontend-lex-ast.md`. It covers `cssl-ast` (the foundational CST type library, 1,030 lines) and `cssl-lex` (the dual-surface lexer, 1,350 lines). These are the two crates that exist below the parser. The dual-surface design — one lexer dispatching to Rust-hybrid and CSLv3-native modes, both producing the same CST — is explained in full here.

**"I want to understand the parser"**  
Read `02-frontend-parse.md`. It covers `cssl-parse` (4,573 lines, 17 files): the Pratt-climber Rust-hybrid surface, the structural CSLv3-native surface stub, the shared `TokenCursor` and error-recovery machinery. This is where the unconditional-CST-return error philosophy is documented.

**"I want to understand the type system and HIR"**  
Read `03-typesys-hir.md`. The HIR crate (`cssl-hir`, 21 files) is the most complex single crate in the workspace. It contains not just the type inference engine (HM + Remy-style effect-row unification) but also the F1 autodiff-legality walker, the F2 refinement-obligation collector, the F4 staged-consistency walker, the F5 IFC walker, and the F6 macro-hygiene walker. Then read `04-typesys-caps-effects-ifc.md` for the three support crates that `cssl-hir` depends on.

**"I want to understand the capability system (F3 — iso/trn/ref/val/box/tag)"**  
Read `04-typesys-caps-effects-ifc.md`, specifically the `cssl-caps` section. The Pony-6 algebra, deny-matrix, subtype lattice, linear tracker, and `GenRef` packed-pointer layout are all here (1,217 lines). The effect system (`cssl-effects`, 1,115 lines) is in the same doc.

**"I want to understand the effect system (F3 — Koka row-polymorphic)"**  
Read `04-typesys-caps-effects-ifc.md`, the `cssl-effects` section. The 32 built-in effects, the `BuiltinEffect` metadata table, `sub_effect_check`, and the `banned_composition` subsystem encoding PRIME_DIRECTIVE prohibitions as type-level compile errors are all there.

**"I want to understand IFC (F5 — Jif-DLM information-flow control)"**  
The critical fact: `cssl-ifc` (the dedicated crate) is empty — it is a 24-line scaffold. The real IFC implementation (1,168 lines: `IfcLabel` lattice, `check_ifc` walker, `check_ifc_flow` dataflow walker, nine built-in principals, diagnostic codes IFC0001–IFC0004) lives in `cssl-hir/src/ifc.rs`. Read `03-typesys-hir.md` for the implementation, and `04-typesys-caps-effects-ifc.md` only for the confirmation that `cssl-ifc` is empty and why.

**"I want to understand autodiff (F1)"**  
This spans three docs and three crates. Read them in order:  
1. `03-typesys-hir.md` — for `ad_legality.rs` in `cssl-hir`, which discovers `@differentiable` functions and validates them structurally.  
2. `05-transform-autodiff-jets.md` — for `cssl-autodiff` (4,061 lines), the MIR-level forward-mode JVP and reverse-mode VJP transform, and `cssl-jets` (293 lines), the abstract `Jet<T,N>` type schema.  
3. `07-ir-mir.md` — for the `AdWalkerPass` adapter in `cssl-mir` that plugs `cssl-autodiff` into the pass pipeline.  
Understanding all three is required to understand F1 end-to-end.

**"I want to understand refinement types and SMT solving (F2)"**  
Read `03-typesys-hir.md` for `refinement.rs` in `cssl-hir` (the obligation-collection pass), then `06-transform-smt-staging-futamura-macros.md` for `cssl-smt` (1,756 lines: the SMT-LIB 2.6 emitter, Z3/CVC5 CLI subprocess dispatch, predicate-text parser).

**"I want to understand staged computation and Futamura projections (F4)"**  
Read `06-transform-smt-staging-futamura-macros.md`. It covers `cssl-staging` (455 lines — data model for `@staged` collection, actual specialization deferred to T8-phase-2), `cssl-futamura` (284 lines — P1/P2/P3 data types and the fixed-point convergence checker), and `cssl-macros` (343 lines — Racket set-of-scopes hygiene mark system).

**"I want to understand observability and the audit chain (F6)"**  
Read `12-observability-persist-rt.md`. It covers `cssl-telemetry` (1,518 lines — real BLAKE3 + Ed25519 crypto, SPSC ring buffer, 26-scope taxonomy, Chrome-trace/JSONL/OTLP exporters), `cssl-persist` (610 lines — orthogonal persistence scaffold, WAL deferred), and `cssl-rt` (19 lines — empty runtime placeholder). Note the security bug flagged in this doc (audit-chain stub-signature bypass, `audit.rs:329–344`).

**"I want to understand the MIR (mid-level intermediate representation)"**  
Read `07-ir-mir.md`. The `cssl-mir` crate (9,600 lines, 12 files) is the largest single-crate doc in the audit. It covers the MLIR-dialect-shaped IR data model, the two-phase HIR-to-MIR lowering (`lower.rs` for signatures, `body_lower.rs` for the 31 `HirExprKind` variants), the monomorphization quartet (`monomorph.rs` + `auto_monomorph.rs`), and the pass pipeline.

**"I want to understand the LIR and MLIR bridge"**  
Read `08-ir-lir-mlir-bridge.md` — but be prepared for disappointment. `cssl-lir` is a 22-line pure scaffold with no LIR types. `cssl-mlir-bridge` (107 lines) is a thin wrapper around the MIR pretty-printer; real MLIR C++ FFI (`melior`/`mlir-sys`) is commented out and explicitly deferred.

**"I want to understand CPU codegen"**  
Read `09-codegen-cpu-cranelift.md`. The `cssl-cgen-cpu-cranelift` crate has two modes: a text-CLIF emitter (stage-0, inspectable) and a real Cranelift JIT engine (`cranelift-frontend` + `cranelift-jit`) that actually executes compiled code. The JIT handles scalar arithmetic, comparisons, libm transcendentals, and multi-result functions; control flow and SIMD are deferred.

**"I want to understand GPU codegen"**  
Read `10-codegen-gpu.md`. Four backends: SPIR-V (most mature, with real `rspirv` binary emission), DXIL (textual HLSL + optional `dxc.exe` subprocess), MSL (textual Metal + optional `spirv-cross` subprocess), WGSL (textual + `naga` in-process validation in tests). All four reject non-empty MIR function bodies at stage-0 — they emit skeleton functions only.

**"I want to understand where compiled code actually runs (host runtimes)"**  
Read `11-host-runtimes.md`. The five `cssl-host-*` crates (Vulkan, Level-Zero, D3D12, Metal, WebGPU) are pure-Rust capability catalogs with Arc A770 stub constructors. Every GPU API crate (`ash`, `windows`, `wgpu`, `metal`, `level-zero-sys`) is declared in the workspace dependency table but wired into zero individual crates. There is no FFI anywhere in this slice.

**"I want to understand the testing infrastructure"**  
Read `13-testing.md`. The `cssl-testing` crate implements 12 oracle modes (9 live, 3 stubs), its own LCG PRNG and shrinker, metamorphic law checkers, fuzzer, bench runner, golden-file comparator, and the R16 attestation BLAKE3+Ed25519 signing hooks. No external test crates (`proptest`, `criterion`, `insta`) are used.

**"I want to understand what the csslc binary actually does"**  
Read `14-examples-csslc-meta.md`. The answer: `csslc/src/main.rs` is 23 lines that print two status lines to stderr and exit 0. No argument parsing, no compiler invocations. The real end-to-end compilation capability lives in `cssl-examples` (6,671 lines), which drives the full pipeline through Cranelift JIT and verifies AD gradients against analytic references.

**"I want to understand the language spec corpus"**  
`[OPEN: 15-specs.md not present in this folder. The spec-corpus audit either was not completed or was not committed. The specs themselves live in compiler-rs/specs/ and are referenced throughout all other audit docs by citation.]`

**"I want to understand the research, stage1 scaffold, and utility scripts"**  
Read `16-research-stage1-scripts.md`. It covers 20 files across three directories: `research/` (14 CSLv3-notation pre-design literature-survey files + 1 synthesis document), `stage1/` (2 placeholder `.cssl` source files representing the self-hosting endgame, plus a `README.csl` P1–P10 roadmap), and `scripts/` (3 Python utility scripts: transcript redaction, spec cross-reference validator, differential lexer oracle skeleton).

**"I want to understand the root governance, CI, and public docs"**  
Read `17-root-docs-github.md`. It covers `DECISIONS.md` (3,336 lines, 106 architectural decision entries across 12 task phases), `PRIME_DIRECTIVE.md` (626 lines, the foundational consent axiom), `README.md` (151 lines, with several aspirational claims flagged), `CONTRIBUTING.md` (129 lines, accurate), the three canonical `.cssl` example programs, and the GitHub Actions CI pipelines.

**"I want to know what is broken or wrong"**  
Start with the Cross-References section of this document for structural mismatches that span docs. Then check the bugs-flagged summaries in `12-observability-persist-rt.md` (audit-chain bypass, ring-slot size mismatch) and `17-root-docs-github.md` (README test-count discrepancy, aspirational feature claims). The `INDEX.md` "Known Issues" section at the repo root is the authoritative tracker.

---

## Doc-by-Doc Index

The maturity verdicts use four labels: **REAL** (functional implementation, substantive code), **PARTIAL** (real implementation with significant deferred sections), **SCAFFOLD** (correct data model but no real processing), **EMPTY** (version constant + one trivial test, nothing else).

---

### 01 — `01-frontend-lex-ast.md`

**Slice:** `cssl-ast` + `cssl-lex`  
**Files audited:** 11 source files (9 `.rs` + 2 fixture text files) + 3 Cargo.toml  
**Audit doc size:** large (>700 lines)  
**Maturity:** REAL (both crates)

Headline findings:
- `cssl-ast` is a zero-dependency type library providing `SourceFile`, `Span`, `Diagnostic`, and the full CST node hierarchy; it is the leaf all other crates depend on safely.
- `cssl-lex` implements a dual-surface tokenizer with auto-detection (four-tier cascade: file extension → pragma → first-line heuristic → default Rust-hybrid), apostrophe-morpheme post-pass folding (T2-D8), and EvidenceMark + ModalOp token types matching the CSLv3 glyph surface.
- The `Diagnostic` type is explicitly marked as a T2 scaffold awaiting richer `miette` integration; it accumulates errors but does not yet produce rich terminal output.

---

### 02 — `02-frontend-parse.md`

**Slice:** `cssl-parse`  
**Files audited:** 17 (16 `.rs` + 1 `Cargo.toml`)  
**Total source LOC:** ~4,573  
**Maturity:** REAL (Rust-hybrid surface), SCAFFOLD (CSLv3-native surface)

Headline findings:
- The Rust-hybrid surface is a fully hand-rolled recursive-descent parser with a Pratt binding-power climber covering 15 precedence levels and the full item grammar (`fn`, `struct`, `enum`, `interface`, `impl`, `effect`, `handler`, capability types, refinement types, effect rows, `perform`/`with`/`region`, pipeline operator, `#run`).
- The CSLv3-native surface parses `§ name [body]` hierarchies and recognizes evidence/modal slot prefixes, but full morpheme-stacking elaboration is deferred to `cssl-hir`.
- Error recovery is unconditional: parse functions always return a CST node; partial trees are walkable after errors. There is no `Result` return from the public `parse()` API.

---

### 03 — `03-typesys-hir.md`

**Slice:** `cssl-hir`  
**Files audited:** 21 (1 `Cargo.toml` + 20 `.rs`)  
**Maturity:** REAL (inference, lowering, all walkers)

Headline findings:
- `cssl-hir` embeds five language-feature passes: F1 autodiff legality (`ad_legality.rs`), F2 refinement-obligation collection (`refinement.rs`), F4 staged-consistency checking (`staged_check.rs`), F5 IFC flow (`ifc.rs`, 1,168 lines), and F6 macro-hygiene (`macro_hygiene.rs`). The dedicated `cssl-ifc` crate is empty; all IFC logic lives here.
- HM type inference with Remy-style effect-row unification is complete (T3.4). Full generics monomorphization is handled at MIR level in `cssl-mir`.
- The `ifc.rs` file (1,168 lines) implements the complete Jif-DLM label lattice with nine built-in PRIME_DIRECTIVE principals (`HarmTarget`, `Surveiller`, `Coercer`, `Weaponizer`, `System`, `Kernel`, `User`, `Public`, `Anthropic-Audit`) and diagnostic codes IFC0001–IFC0004 including a T11-D36 dataflow walker.

---

### 04 — `04-typesys-caps-effects-ifc.md`

**Slice:** `cssl-caps` + `cssl-effects` + `cssl-ifc`  
**Files audited:** 13 (9 `.rs` + 3 `Cargo.toml` + 1 near-empty `lib.rs`)  
**Source LOC:** `cssl-caps` 1,217 / `cssl-effects` 1,115 / `cssl-ifc` 24  
**Maturity:** REAL (`cssl-caps`, `cssl-effects`), EMPTY (`cssl-ifc`)

Headline findings:
- `cssl-caps` implements the full Pony-6 capability algebra: six capability kinds, deny-matrix subtyping, `LinearTracker` for `iso` must-consume discipline, and `GenRef` (`u40` object-index + `u24` generation counter packed in `u64`).
- `cssl-effects` provides 32 built-in effects with a compile-time metadata table, row-containment checking via `sub_effect_check`, and `banned_composition` rules that encode PRIME_DIRECTIVE §1 prohibitions as type-level compile-time errors (coercion banned absolutely, surveillance+IO banned without `Privilege<Kernel>`).
- `cssl-ifc` is 24 lines: version constant, one scaffold test. The IFC implementation is entirely in `cssl-hir/src/ifc.rs` and has not been factored into this crate.

---

### 05 — `05-transform-autodiff-jets.md`

**Slice:** `cssl-autodiff` + `cssl-jets`  
**Files audited:** 8 (6 `.rs` + 2 `Cargo.toml`)  
**Total LOC:** ~4,354 (autodiff ~4,061; jets 293)  
**Maturity:** REAL (`cssl-autodiff`), SCAFFOLD (`cssl-jets`)

Headline findings:
- `cssl-autodiff` implements source-to-source autodiff on MIR: a 19-primitive rule table (covering all smooth transcendentals plus piecewise-linear `min`/`max`/`abs`/`sign` with runtime-comparison subgradients), forward-mode JVP emission, and reverse-mode VJP emission via adjoint accumulation.
- The transform plugs into the `cssl-mir` pass pipeline via the `AdWalkerPass` adapter; it consumes `cssl-hir` for `@differentiable` annotation discovery and produces new `MirFunc` values.
- `cssl-jets` is an abstract schema for `Jet<T,N>` higher-order AD via truncated Taylor series; it defines types and validation functions but no runtime layout or codegen — those are deferred to `cssl-staging` (T8).

---

### 06 — `06-transform-smt-staging-futamura-macros.md`

**Slice:** `cssl-smt` + `cssl-staging` + `cssl-futamura` + `cssl-macros`  
**Files audited:** 11 (8 `.rs` + 4 `Cargo.toml`)  
**Total LOC:** ~2,838 (smt 1,756 / staging 455 / futamura 284 / macros 343)  
**Maturity:** PARTIAL (`cssl-smt`), SCAFFOLD (`cssl-staging`, `cssl-futamura`, `cssl-macros`)

Headline findings:
- `cssl-smt` translates `ObligationBag` entries from `cssl-hir` into SMT-LIB 2.6 text and dispatches to Z3 or CVC5 via subprocess (no FFI). The predicate-text recursive-descent parser covers the stage-0 grammar subset. KLEE symbolic execution is named in module documentation but entirely absent from implementation.
- `cssl-staging` and `cssl-futamura` have correct data models (`Specializer`, `SpecializationSite`, `Projection`, `FixedPointRecord`) but no actual specialization transform or partial-evaluation algorithm — both deferred to T8-phase-2.
- `cssl-macros` implements Racket set-of-scopes hygiene marks (`HygieneMark` as `BTreeSet<ScopeId>`, `ScopeAllocator`, three-tier `MacroRegistry`) but expansion and proc-macro evaluation are deferred.

---

### 07 — `07-ir-mir.md`

**Slice:** `cssl-mir`  
**Files audited:** 12 (11 `.rs` + `Cargo.toml`)  
**Total LOC:** ~9,600  
**Maturity:** REAL (data model, lowering, monomorphization), SCAFFOLD (most pipeline passes)

Headline findings:
- The MIR data model is MLIR-dialect-shaped: `MirModule` → `MirFunc` → `MirRegion` / `MirBlock` → `MirOp` with SSA `ValueId`s and typed `MirValue`s. All 31 `HirExprKind` variants are covered in `body_lower.rs`.
- The monomorphization quartet (`monomorph.rs` + `auto_monomorph.rs`) is real: generic HIR functions are specialized with deterministic mangled names, call-site attributes are rewritten, and unspecialized templates are dropped post-specialization.
- The pass pipeline has one real pass (`StructuredCfgValidator`) and five informational stubs; the `AdWalkerPass` adapter integrates `cssl-autodiff` into the pipeline. TableGen authoring, type-inference-driven lowering, and dialect conversion are deferred.

---

### 08 — `08-ir-lir-mlir-bridge.md`

**Slice:** `cssl-lir` + `cssl-mlir-bridge`  
**Files audited:** 4 (2 `.rs` + 2 `Cargo.toml`)  
**Total LOC:** `cssl-lir` 22 / `cssl-mlir-bridge` 107  
**Maturity:** EMPTY (`cssl-lir`), SCAFFOLD (`cssl-mlir-bridge`)

Headline findings:
- `cssl-lir` is a 22-line file containing one version constant and one trivial test. There is no LIR type, no dispatch logic, no fat-binary assembly, no target orchestration of any kind.
- `cssl-mlir-bridge` wraps the MIR pretty-printer already present in `cssl-mir::print_module`, providing only the `--emit-mlir` textual dump path. The `melior`/`mlir-sys` FFI dependency is commented out in the workspace `Cargo.toml`.
- The gap between these crates' Cargo descriptions and their actual content is the largest spec-vs-reality divergence in the entire workspace.

---

### 09 — `09-codegen-cpu-cranelift.md`

**Slice:** `cssl-cgen-cpu-cranelift`  
**Spec references:** `specs/07_CODEGEN.csl § CPU BACKEND`, `specs/14_BACKEND.csl`  
**Maturity:** PARTIAL (scalar JIT real, control flow and SIMD deferred)

Headline findings:
- Two coexisting output modes: a text-CLIF emitter (`emit.rs` + `lower.rs`) producing human-readable diffable output, and a real Cranelift JIT (`jit.rs`) using `cranelift-frontend::FunctionBuilder` and `cranelift-jit::JITModule` to compile and execute code in-process via `unsafe` fn-pointer casts.
- The JIT handles integer/float arithmetic, comparisons, conditional select, constants, returns, inter-fn calls, and libm transcendentals via extern linkage. Control flow (`scf.if`, `scf.for`), memref load/store, and SIMD are deferred to T11-D22+.
- No LLVM anywhere in the codebase. Cranelift 0.115 is the sole CPU codegen backend; this is intentional and documented.

---

### 10 — `10-codegen-gpu.md`

**Slice:** `cssl-cgen-gpu-spirv` + `cssl-cgen-gpu-dxil` + `cssl-cgen-gpu-msl` + `cssl-cgen-gpu-wgsl`  
**Files audited:** 24 (20 `.rs` + 4 `Cargo.toml`)  
**Total source LOC:** 5,035 (SPIR-V 2,123 / DXIL 1,096 / MSL 929 / WGSL 887)  
**Maturity:** PARTIAL (SPIR-V, DXIL, MSL, WGSL — all emit skeleton functions only)

Headline findings:
- All four backends are independent MIR-to-target-text emitters; they do not funnel through SPIR-V. The README architecture diagram implies a SPIR-V funnel that does not exist in the code. The `spirv_cross` module in `cssl-cgen-gpu-msl` and the `dxc` subprocess in `cssl-cgen-gpu-dxil` are optional validation adapters, not mandatory pipeline steps.
- All four backends reject non-empty MIR function bodies — they emit skeleton function signatures only. Body lowering is deferred to T10-phase-2 for all four.
- `cssl-cgen-gpu-spirv` is the only backend with a live external runtime dependency (`rspirv`) producing real binary output (`Vec<u32>`) validated via `rspirv::dr::load_words`. The WGSL backend is the only one with in-process structural validation via `naga` in dev-dependencies.

---

### 11 — `11-host-runtimes.md`

**Slice:** `cssl-host-vulkan` + `cssl-host-level-zero` + `cssl-host-d3d12` + `cssl-host-metal` + `cssl-host-webgpu`  
**Files audited:** 27 (22 `.rs` + 5 `Cargo.toml`)  
**Maturity:** SCAFFOLD (all five crates)

Headline findings:
- All five crates are pure-Rust capability catalogs with no GPU API FFI whatsoever. Every GPU API crate (`ash`, `windows`, `wgpu`, `metal`, `level-zero-sys`) is declared in the workspace `[workspace.dependencies]` table but not listed in any individual crate's `[dependencies]`. Phase-2 FFI integration is blocked pending MSVC toolchain confirmation for all five.
- Every crate provides at least one Arc A770 stub constructor hardcoded to PCI device ID `0x56A0` / vendor `0x8086` — the primary development target from `specs/10_HW.csl` — so the rest of the toolchain can reference device capabilities without a live GPU.
- `cssl-host-level-zero` is the weakest: even its workspace dependency line is commented out (`# level-zero-sys = "0.3"`), meaning there is no FFI path defined at any level.

---

### 12 — `12-observability-persist-rt.md`

**Slice:** `cssl-telemetry` + `cssl-persist` + `cssl-rt`  
**Files audited:** 15 (12 `.rs` + 3 `Cargo.toml`)  
**Total LOC:** telemetry 1,518 / persist 610 / rt 19  
**Maturity:** PARTIAL (`cssl-telemetry`), SCAFFOLD (`cssl-persist`), EMPTY (`cssl-rt`)

Headline findings:
- `cssl-telemetry` uses real cryptographic primitives: BLAKE3 1.5.4 and ed25519-dalek 2.1.1 are live dependencies. The audit chain (`audit.rs`, 519 lines) genuinely signs entries with Ed25519 and chains them with BLAKE3 content hashes. The lib.rs deferral comment calling blake3/ed25519 "currently stubbed" is stale and was not updated after the phase-2a upgrade.
- Security bug flagged (High severity): `verify_chain` at `audit.rs:329–344` skips Ed25519 signature verification for entries whose stored signature matches the stub-sign output, even when a real signing key is attached. An attacker knowing the deterministic stub-sign algorithm can forge entries that pass keyed-chain verification.
- `cssl-rt` is 19 lines (version constant + one trivial test). No allocator hooks, no telemetry plumbing, no orthopersist image API are present.

Notable follow-up flagged: the audit-chain stub-signature bypass is a security issue in the F6 integrity guarantee and should be fixed before the audit chain is used in any production context.

---

### 13 — `13-testing.md`

**Slice:** `cssl-testing`  
**Spec authority:** `specs/23_TESTING.csl`  
**Maturity:** PARTIAL (9 of 12 oracle modes live, 3 stubs)

Headline findings:
- Nine oracle modes have real implementations: `property` (custom LCG PRNG + typed generators + shrinking), `metamorphic` (algebraic law checkers for commutativity, associativity, Leibniz product rule, chain rule, Lipschitz), `fuzz` (dumb-mode byte fuzzing + `catch_unwind` panic capture), `bench` (wall-clock timing + baseline-file regression), `golden` (byte-exact comparison), `differential` (ULP-difference helpers), `r16_attestation` (BLAKE3 + Ed25519 signing of audit reports), `replay` (seed-based determinism), and `audit` (AuditChain structural verification via `cssl-telemetry`).
- Three modes remain stubs pending hardware integration: `power` (needs Level-Zero sysman), `thermal` (needs `zesTemperatureGetState`), and `hot_reload` (needs `cssl-persist`).
- No external test crates are used anywhere. The crate implements its own PRNG, shrinker, and timing machinery — the only dependency is `cssl-telemetry`.

---

### 14 — `14-examples-csslc-meta.md`

**Slice:** `cssl-examples` + `csslc` binary + workspace metadata  
**Files audited:** covers the two crate roots + workspace `Cargo.toml`  
**Total LOC:** `cssl-examples` ~6,671 / `csslc` 23  
**Maturity:** REAL (`cssl-examples`), EMPTY (`csslc`)

Headline findings:
- `csslc/src/main.rs` is 23 lines: prints two status lines to stderr, exits 0. No argument parsing, no subcommand dispatch, no compiler crate invocations. The docstring names the intended subcommands (`build`, `check`, `fmt`, `test`, `bench`, `lint`, `doc`, `emit-mlir`, `emit-spirv`, `verify`, `attest`, and others) but none are implemented.
- `cssl-examples` is the most complete end-to-end integration point: it drives pipeline smoke tests on three embedded `.cssl` examples, an F1-chain gate covering lex through SMT translation, a Cranelift JIT chain verifying forward- and reverse-mode AD gradient correctness against central differences at sample points, and a symbolic `AnalyticExpr` verifier with R18 BLAKE3+Ed25519 attestation signing.
- The workspace resolver is 2, all package metadata is inherited, and the workspace `[lints.clippy]` enforces `all = deny` + `pedantic = warn` + `nursery = warn`. External dependencies are declared once in `[workspace.dependencies]` and inherited per-crate.

---

### 15 — `15-specs.md`

`[OPEN: This file is not present in docs/audit/ as of 2026-05-14. The spec-corpus audit (covering compiler-rs/specs/ — approximately 23+ .csl files spanning specs/00_MANIFESTO.csl through specs/23_TESTING.csl and beyond) was not completed or not committed. All other audit docs reference spec files by citation but none audit the spec text itself. Until this doc is written, the specs/ directory has no dedicated audit coverage.]`

---

### 16 — `16-research-stage1-scripts.md`

**Slice:** `research/` (14 files) + `stage1/` (3 files) + `scripts/` (3 files) — 20 files total  
**Maturity:** SCAFFOLD (stage1), REAL (research as design documents and literature survey)

Headline findings:
- The `research/` pre-design survey files (`S1_*.csl` through `S10_*.csl` + `99_SYNTHESIS.csl`) collectively establish the prior-art basis for every CSSLv3 design decision: F1 from Slang.D, F2 from LiquidHaskell, F3 from Koka, F4 from Futamura/staging theory, the memory model from Vale and Futhark, codegen from MLIR/IREE. The synthesis doc (`99_SYNTHESIS.csl`) maps every design decision back to a research finding.
- `stage1/` contains two placeholder `.cssl` source files (`hello.cssl`, `compiler.cssl`) plus `README.csl` with a P1–P10 self-hosting roadmap. These files verify that the stage-0 parser accepts valid CSSLv3 syntax as the grammar evolves (exercised by `cssl-examples/src/stage1_scaffold.rs`). They are not compilable programs.
- `scripts/` contains three Python utility scripts: a transcript identity-claim redaction tool, a spec cross-reference validator, and a differential lexer oracle skeleton comparing the Rust-port lexer against an Odin reference parser.

---

### 17 — `17-root-docs-github.md`

**Slice:** `DECISIONS.md` + `PRIME_DIRECTIVE.md` + `README.md` + `CONTRIBUTING.md` + `examples/*.cssl` + `.github/workflows/*.yml` + dotfiles  
**Files audited:** 12  
**Maturity:** REAL (governance), PARTIAL (README — several aspirational claims)

Headline findings:
- `DECISIONS.md` (3,336 lines, 106 entries) is the authoritative architectural memory for all design decisions from T1-D1 through T12-D2. The 106 entries span 12 task phases and are all dated 2026-04-16 or 2026-04-17. The decision log confirms 1,049–1,074 tests in the workspace at the point of final logged entries.
- `README.md` claims "1600+ tests passing" — this figure is not traceable to any decision log entry and is higher than the 1,049–1,074 tracked in DECISIONS.md. The README also claims "all six features implemented at minimum-viable depth" and states a pre-built Windows binary is available; neither reflects stage-0 scaffold reality for all features or for any binary release.
- `PRIME_DIRECTIVE.md` §10 Terms of Service defines an access-revocation policy (revocation for "evil" actors: intentional harm-causers, unrepentant harm-causers, or bad-faith interpreters) that sits above the Apache-2.0 OR MIT open-source license declared in every crate's metadata. New contributors should read §10 alongside the standard license header.

---

## Cross-References: Facts That Span Multiple Docs

These are non-obvious connections that could mislead a reader looking at only one doc.

**cssl-ifc is empty; real IFC is in cssl-hir.**
The `cssl-ifc` crate (doc 04) is a 24-line placeholder. The entire IFC implementation — `IfcLabel` lattice, `check_ifc` structural walker, `check_ifc_flow` dataflow walker, nine PRIME_DIRECTIVE principals, diagnostic codes IFC0001–IFC0004 — is in `cssl-hir/src/ifc.rs` (1,168 lines, covered in doc 03). A reader auditing doc 04 looking for F5 will find nothing; they need doc 03.

**csslc does nothing; real maturity is in the library crates.**
The `csslc` binary (doc 14) is 23 lines of stderr output. The actual demonstrated capability — lex → parse → HIR → MIR → Cranelift JIT → executed gradient-correct machine code — exists only in `cssl-examples` (also doc 14) and the library crates documented in docs 01–13. Do not judge the compiler's maturity by the binary.

**GPU backends are not a SPIR-V funnel.**
The architecture diagram in `README.md` (doc 17) implies all GPU targets flow through SPIR-V. The implementation (doc 10) shows four independent paths: SPIR-V, DXIL, MSL, and WGSL each accept `MirModule` directly. The `spirv_cross` and `dxc` subprocess adapters are optional CI validation tools, not mandatory pipeline stages.

**Host runtime crates have zero FFI despite their Cargo descriptions.**
The five `cssl-host-*` crates (doc 11) are described in their `Cargo.toml` as targeting Vulkan, Level-Zero, D3D12, Metal, and WebGPU respectively. None of them link against any GPU API library. The API crates (`ash`, `windows`, `wgpu`, `metal`, `level-zero-sys`) are workspace-declared but wired into zero individual crates at stage-0.

**F1 autodiff spans three docs and three crates.**
To understand automatic differentiation end-to-end, a reader must consult: doc 03 (the `ad_legality.rs` walker in `cssl-hir` that discovers and validates `@differentiable` functions), doc 05 (the `cssl-autodiff` transform that emits JVP/VJP `MirFunc` pairs + the `cssl-jets` abstract type schema), and doc 07 (the `AdWalkerPass` in `cssl-mir` that plugs the transform into the pass pipeline). No single doc has the complete picture.

**Test-count discrepancy between README and DECISIONS.md.**
`README.md` (doc 17) claims "1600+ tests passing." `DECISIONS.md` (also doc 17) records test counts of 1,049 (at T9-D4) and 1,074 (at T3-D13) — both significantly lower. The 1,600+ figure may reflect later session additions not captured in DECISIONS.md, but it is not traceable to any entry in the decision log as audited.

**Telemetry audit chain has a stub-signature bypass (security, doc 12).**
`cssl-telemetry/src/audit.rs:329–344` skips Ed25519 signature verification for any chain entry whose stored signature matches the deterministic stub-sign output, even when a real signing key is attached to the chain. This means the F6 integrity guarantee can be bypassed by anyone who knows the stub-sign algorithm. A reader relying on doc 12's F6 maturity assessment should weight this finding; the crypto is real but the verification path has a conditional bypass.

**MIR pretty-printer is what `cssl-mlir-bridge` actually wraps.**
`cssl-mlir-bridge` (doc 08) is described in its Cargo.toml as a "melior FFI bridge to MLIR." In reality it wraps `cssl-mir::print_module` (documented in doc 07) via two thin adapter functions. The `--emit-mlir` output from the compiler is MIR pretty-printed to MLIR text, not MLIR C++ IR objects. The two docs must be read together to understand what the bridge actually provides.

---

## Maintenance

**Adding a new audit doc.** Create the file as `NN-descriptive-name.md` where NN is the next sequential number. Add an entry to this README's index section following the established format: slice, files audited, total LOC, maturity verdict, headline findings. Check whether the new crates or files have cross-reference implications for existing entries.

**Re-auditing an existing slice.** Rename the existing doc to `NN-descriptive-name.YYYY-MM-DD.md` (preserving it as a dated snapshot), write a fresh doc at the original name, and update this README's entry for that doc. Note any maturity regressions or improvements explicitly in the new doc's introduction.

**When 15-specs.md is written.** Remove the OPEN marker at its index entry. Read the specs audit and check whether any of the cross-references above need updating — particularly the IFC and autodiff entries, which cite specific spec files as authorities.

---

## What This Audit Did Not Cover

The audit read source files. It did not:

**Test runtime behavior on real hardware.** No GPU was used. Stub constructors were not probed against a live Arc A770 or any other device. The Cranelift JIT tests were run against the compiled audit agent's host CPU, not independently verified on the target hardware profile in `specs/10_HW.csl`.

**Measure performance.** No benchmarks were run. The `cssl-testing` bench oracle mode and the `FrequencySample`/`LatencyPercentiles` data structures were read for correctness but not exercised against real workloads. Claims about overhead budgets (0.5% for `Counters` scope per `specs/22_TELEMETRY.csl`) were not verified empirically.

**Conduct adversarial security testing.** The audit-chain stub-signature bypass (doc 12) was found by static reading of the `verify_chain` method's conditional branches, not by pen-testing. No fuzzing of the SMT-LIB emission path, the predicate-text parser, or the Cranelift JIT was performed. The PRIME_DIRECTIVE's `banned_composition` rules were read for correctness against the spec but not subjected to type-system escape attempts.

**Diff against other branches.** The audit covers the `main` branch at HEAD `7600523` as of 2026-05-14. The `cssl/session-11/T11-W18-L8-DXIL-DIRECT` branch referenced in project memory files may contain csslc fixes and additional features not present on `main`. No cross-branch comparison was performed.

**Audit the spec corpus.** The `specs/` directory (23+ `.csl` files) was not audited. Every audit doc cites spec files by filename and section as authorities, but the spec text itself — its internal consistency, completeness, and alignment with implementation — was not systematically examined. See the OPEN marker for `15-specs.md`.

**Verify build reproducibility.** R16 reproducibility (`r16_attestation` in `cssl-testing`) is a documented design requirement backed by Ed25519-signed attestation. The audit confirmed the attestation infrastructure code is real, but did not perform a stage3 rebuild bit-comparison to verify the claim in practice.
