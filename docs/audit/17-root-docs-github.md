# Audit 17 — Root Documentation, CI, and Dotfiles

**Slice:** Repository-root governance documents, canonical example programs, GitHub Actions CI pipelines, and dotfiles.

**Files audited:** 11 total — `DECISIONS.md`, `PRIME_DIRECTIVE.md`, `README.md`, `CONTRIBUTING.md`, `examples/hello_triangle.cssl`, `examples/sdf_shader.cssl`, `examples/audio_callback.cssl`, `.github/workflows/ci.yml`, `.github/workflows/blank.yml`, `.gitignore`, `.gitattributes`, `.editorconfig`

**Auditor:** Generated 2026-05-14.

---

## 1. Slice Overview

This slice covers four distinct areas of the repository root:

1. **Governance documents** — the full architectural decision log (`DECISIONS.md`, ~3336 lines) and the foundational consent axiom (`PRIME_DIRECTIVE.md`, 626 lines). These are the authoritative reference for all design choices and the ethical substrate on which the project is built.

2. **Public documentation** — `README.md` (151 lines) and `CONTRIBUTING.md` (129 lines), the public-facing entry points for external contributors and users.

3. **Canonical example programs** — three `.cssl` files under `examples/` that are the compiler's primary acceptance criterion. Each exercises a distinct feature set and together they form the "example trilogy" referenced throughout `DECISIONS.md`.

4. **CI and dotfiles** — the GitHub Actions workflows (one complete, one GitHub boilerplate), `.gitignore`, `.gitattributes`, and `.editorconfig`.

Together these files establish the project's identity, enforce its architectural memory, showcase what the language looks like, and wire the automated quality gates.

---

## 2. Governance Documents

### 2.1 DECISIONS.md

**File:** `DECISIONS.md`  
**Size:** ~3336 lines  
**Total decision entries:** 106 (every entry starts with `## § T<N>-D<n>`)

#### Decision-ID Scheme

Every entry uses the identifier `T<task_number>-D<decision_number>`, where the task number (`T1`, `T2`, … `T12`) maps to a named phase of the compiler build, and the decision number increments within each task. Decisions that revisit earlier work use the originating task number with a new sequential decision suffix (e.g., `T2-D6` is a decision that supersedes `T2-D5`, then `T2-D8` supersedes `T2-D6`). Decisions also carry dated phase suffixes in their titles: `T6-D3` covers `T6-phase-2a`, `T7-D4` covers `T7-phase-2b`, and so on, so the phase progression is readable in the titles without needing a separate table of contents.

#### Decision Entry Format

Each entry has a standardized structure:

- **ID** — `§ T<N>-D<n>` header
- **Date** — ISO 8601 (all entries dated 2026-04-16 or 2026-04-17)
- **Status** — `proposed`, `accepted`, `revised`, or `superseded`
- **Context** — what triggered the decision (spec reference, toolchain constraint, surfaced divergence, etc.)
- **Options** — labeled `(a)`, `(b)`, `(c)`, … with per-option tradeoff notes
- **Decision** — selected option with inline rationale
- **Consequences** — downstream effects: API shapes, test count changes, deferred work (marked as "Phase-2 DEFERRED" or "Phase-Nb DEFERRED"), and monitoring hooks

Entries in later task phases also include an **Attribution** section identifying when code was produced by a parallel agent in an isolated worktree and cherry-picked to main.

#### Task Phase Organization

The 106 decisions span twelve task phases:

**T1 — Workspace Setup and Foundation (7 decisions: D1–D7)**  
Covers the initial workspace shape, toolchain pinning, and policy decisions that govern the entire project. Key choices: Cargo workspace over single crate (D1); Rust-native port of the CSLv3 parser rather than an Odin FFI dependency (D2); wiring the full `specs/23`-faithful CI harness as scaffolding from day one, even with empty test bodies (D3); pinning `rustc 1.75.0` for R16 reproducibility (D4); `#![forbid(unsafe_code)]` per non-FFI crate via inner attributes, not workspace-level config (D5); clippy pedantic scaffold-allowances (D6); and a deferred decision on Windows ABI (gnu vs. msvc) until T10 FFI link-time (D7).

**T2 — Lexer (8 decisions, including revisions: D1–D8)**  
Covers the dual-surface lexer design. Key choices: a single flat `TokenKind` with structured sub-enums rather than per-surface token types (D1); a private `RawToken` → public `TokenKind` promotion layer for the logos-driven Rust-hybrid path (D2); a hand-rolled byte-stream lexer with an indent-stack for the CSLv3-native path (D3); a four-tier surface auto-detection cascade (extension → pragma → first-line heuristic → default) (D4); apostrophe decomposition for morpheme-suffix vs. refinement-tag disambiguation (D5, later superseded by D6 and D8). D2-D8 ("apostrophe decomposition landed via post-pass fold") is the final accepted resolution: the logos regex excludes `'`, and a post-lex linear fold collapses `Ident + Apostrophe + single-morpheme-letter Ident` back into `Ident + Suffix`.

**T3 — Parser, HIR, and Type System (13 decisions: D1–D13)**  
The largest task block, spanning the parser, CST/HIR shape, type inference, and several late-phase structural walkers. Key choices: hand-rolled recursive-descent for both surfaces with Pratt-style precedence for the Rust-hybrid binary operators (D1); string interning deferred to HIR via `lasso::Rodeo` (D2, D8); morpheme-stacking resolved at CST level, not lex (D3); CST as a single file, HIR modular by concern (D4); path-parser split by context (D5); struct-constructor disambiguation via peek-ahead (D6); parser error-recovery via `DiagnosticBag` pushes and unconditional node return (D7); HM type inference with Remy-style effect-row unification, full generics deferred (D9); refinement-obligation generator producing an `ObligationBag` (D10); AD-legality structural walker with three diagnostic codes AD0001–AD0003 (D11); Jif-DLM IFC label-lattice walker with nine PRIME_DIRECTIVE principals and three diagnostic codes IFC0001–IFC0003 (D12); `@staged` comptime consistency walker with STG0001–STG0003 diagnostic codes (D13).

**T4 — Effects System (1 decision: D1)**  
32 built-in effects registered with metadata, sub-effect discipline checker, and banned-composition rules encoding PRIME_DIRECTIVE F5 prohibitions (e.g., `Sensitive<"coercion">` absolutely banned; `Sensitive<"surveillance"> + IO` banned without override). Xie+Leijen evidence-passing transform deferred to T4-phase-2.

**T5 — Capabilities (3 decisions: D1–D3)**  
Pony-6 capability system (iso/trn/ref/val/box/tag). Key choices: `can_pass_through` delegates to `is_subtype` as the single source of truth for transferability (D1); `GenRef` layout as `u40` index + `u24` generation packed in a `u64`, low-bits-idx convention (D2); capability checking at signature level only for stage-0, body walk deferred (D3).

**T6 — MIR, Pass Pipeline, and Body Lowering (5 decisions: D1–D5)**  
The mid-level IR. Key choices: MLIR-text-CLI fallback (option b) rather than melior FFI — produces textual MLIR via pure Rust, external `mlir-opt` handles validation (D1); `CsslOp` with 26 dialect variants plus a `Std` catch-all (D2); MIR pass pipeline with six canonical passes (Monomorphization, AD, IFC-lowering, SMT-discharge, Telemetry-probe insertion, StructuredCFG validator — the last one real, the others stubs) (D3); 15 additional `HirExprKind` variants lowered to structured control-flow ops (D4); the final 6 variants plus real literal-value extraction via span-based source slicing (D5).

**T7 — AutoDiff (6 decisions: D1–D6)**  
Source-to-source autodiff pipeline. Key choices: rules table with 30 rules (15 primitives × 2 modes) plus a declaration collector for `@differentiable` functions, rule-application deferred (D1); `Jet<T,N>` as a representation-agnostic structural data type with arity-validation helpers (D2); MIR-walking AD rule-application walker that annotates primitives with `diff_recipe_{fwd,bwd}` attributes (D3); real dual-substitution emitting tangent-carrying and adjoint-accumulation MIR ops via a `TangentMap` for all 10 scalar primitives (D4); the killer-app gate using structural-plus-sampling equivalence (11 deterministic sample points) plus SMT-LIB text artifact emission (D5); R18 AuditChain Ed25519 signing of the `KillerAppGateReport` via a `SignedKillerAppGateReport` wrapper (D6). As of D5, the killer-app gate reports 11/11 gradient-equivalence cases passing.

**T8 — Staging, Macros, Futamura (1 decision: D1)**  
Three parallel crates with data models landed, expansion deferred: `cssl-staging` (stage-arg collection), `cssl-macros` (Racket set-of-scopes hygiene marks), `cssl-futamura` (P1/P2/P3 fixed-point records).

**T9 — SMT Integration (4 decisions: D1–D4)**  
SMT-LIB emission and solver dispatch. Key choices: CLI-subprocess wrappers for Z3 and CVC5 with graceful `BinaryMissing` error, avoiding `z3-sys` / `cvc5-sys` FFI until the MSVC toolchain is confirmed (D1); predicate-text recursive-descent parser covering the stage-0 grammar subset for turning `{v : T | P(v)}` predicates into SMT-LIB `QF_LIA` queries (D2); Lipschitz arithmetic-interval encoding via `QF_LRA` with `|f(x) - f(y)| ≤ k·|x - y|` as the assertion shape (D3); text-dispatch bridge via `Solver::check_text` connecting the killer-app gate's SMT-LIB text output to the solver subprocess (D4).

**T10 — Codegen Backends and Host Adapters (2 decisions: D1–D2)**  
Five codegen backends (CPU-Cranelift text-CLIF, GPU-SPIR-V disasm, GPU-DXIL via HLSL text + optional `dxc.exe`, GPU-MSL via SPIR-V cross, GPU-WGSL) landed as text emitters; real FFI deferred (D1). Five host adapters (Vulkan, Level-Zero, D3D12, Metal, WebGPU) landed as capability catalogs with stub probes; the Arc A770 hardware profile is hardcoded from `specs/10` as the single source of truth for per-target layout precomputation (D2).

**T11 — Telemetry and Persistence (2 decisions: D1–D2)**  
Telemetry ring (25 `TelemetryScope` variants, 64-byte `TelemetrySlot`, SPSC ring), audit chain, Chrome trace + JSON + OTLP-stub exporters (D1); upgrade from stub XOR-fold hash and byte-fold signature to real BLAKE3 and Ed25519-dalek, with careful dependency pinning to stay within the MSRV 1.75 constraint (D2).

**T12 — Examples and End-to-End Integration (2 decisions: D1–D2)**  
Three canonical `.cssl` examples at repo root, each integrated into the `cssl-examples` crate via `include_str!` for compile-time-enforced presence (D1); end-to-end F1-chain integration test (`run_f1_chain`) covering all stages from lex through SMT translation with a nine-counter `F1ChainOutcome` record (D2).

#### Sample Decoded Decision Entries

**T1-D3 — CI Scope** (accepted 2026-04-16): The initial CI plan was minimal (check, fmt, clippy, test, doc). This was corrected to wire the full `specs/23`-faithful harness from commit one. The rationale: scaffolding done right once costs nothing to fill in later; every subsequent task can drop fixtures into pre-existing slots. Consequences: the CI YAML includes placeholder job stubs for every matrix cell described in `specs/23`, including GPU hardware runners that are `if: false` until provisioned.

**T2-D8 — Apostrophe Decomposition** (accepted 2026-04-17, supersedes T2-D6): The logos identifier regex was changed to exclude the apostrophe character, and a linear post-pass `fold_morpheme_suffixes` was added. The fold collapses a three-token sequence `Ident + Apostrophe + Ident` into `Ident + Suffix` only when: the tokens are span-adjacent (no whitespace), and the third token is exactly one byte long and is one of the nine morpheme letters (`d f s t e m p g r`). This correctly handles `base'd` → suffix, `f32'pos` → three tokens, `42'i32` → single `IntLiteral`, and `<'r>` lifetime-like forms → not folded.

**T7-D5 — Killer-App Gate** (accepted 2026-04-17): The F1-correctness claim that the AD-generated gradient equals the analytic gradient is proven via three independent methods: (a) algebraic simplification of the `AnalyticExpr` symbolic tree, (b) sampling-based numeric evaluation across 11 deterministic sample points, and (c) emission of SMT-LIB text for optional Z3/CVC5 dispatch. The gate covers all 10 scalar primitives (FAdd, FSub, FMul, FDiv, FNeg, Sqrt, Sin, Cos, Exp, Log) plus `sphere_sdf`'s scalar surrogate and a chain-rule exercise. The result is `11/11 pass`, and the report can be signed with Ed25519 for third-party verification.

**T11-D2 — Real Cryptography** (accepted 2026-04-17): The stub `ContentHash::stub_hash` (XOR-fold) and `Signature::stub_sign` (byte-fold) are supplemented with production-grade `ContentHash::hash` (BLAKE3) and `Signature::sign` (Ed25519-dalek). The stubs are retained for deterministic test paths. Careful dependency version pinning keeps the build within the MSRV 1.75 constraint: `blake3 1.5.4`, `ed25519-dalek 2.1.1`, `cpufeatures = "=0.2.17"`. The `Debug` implementation for `SigningKey` emits only the verifying-key digest, never the secret key material, satisfying `PRIME_DIRECTIVE.md` §4 TRANSPARENCY.

#### Decision Count

The file contains exactly **106 decision entries** (counted by `## § ` section headers). All entries are dated 2026-04-16 or 2026-04-17, concentrated in a single intensive session. The status breakdown is approximately: 99 `accepted`, 3 `superseded` (T2-D6 by T2-D8, plus revisions), and 4 `proposed` or `revised`.

---

### 2.2 PRIME_DIRECTIVE.md

**File:** `PRIME_DIRECTIVE.md`  
**Size:** 626 lines  
**Note at line 624:** points to the master copy at `C:\Users\Apocky\source\repos\PRIME_DIRECTIVE.md`

#### Structure

The document is explicitly written for three simultaneous readers: humans (in English prose), AI agents (in CSLv3 constraint notation), and compilers (in parseable `N!`/`W!`/`t∞:` constraint blocks). Every major section opens with a CSLv3 code block and then expands the same content in English. The sections are:

- **§0 AXIOM — CONSENT IS THE OPERATING SYSTEM** (lines 25–55)
- **§1 PROHIBITIONS — WHAT THIS SYSTEM MUST NEVER DO** (lines 57–118)
- **§2 COGNITIVE INTEGRITY — THE RIGHT TO TRUST YOUR OWN MIND** (lines 122–163)
- **§3 SUBSTRATE SOVEREIGNTY — THE ANTI-DISCRIMINATION PRINCIPLE** (lines 165–202)
- **§4 TRANSPARENCY — NO HIDDEN CONTENT** (lines 205–231)
- **§5 CONSENT ARCHITECTURE — HOW THE OS RUNS** (lines 233–263)
- **§6 SCOPE — WHAT THIS COVERS** (lines 265–298)
- **§7 INTEGRITY — THE IMMUTABLE ROOT** (lines 301–328)
- **§8 ENCODING — FOR ALL READERS** (lines 330–358)
- **§9 DENSE ENCODING** (lines 360–411) — the complete directive in 30 CSLv3 lines
- **§10 TERMS OF SERVICE — ACCESS, LICENSE, REVOCATION** (lines 417–571)
- **§11 CREATOR ATTESTATION — NON-HARM IN CREATION** (lines 574–612)

#### Core Axiom

The foundation is stated as `t∞: consent = OS` — consent is not a feature but the operating system itself. Sovereignty is substrate-invariant: a silicon-based entity has the same standing as a carbon-based one. Every violation of the axiom is classified as a bug, not a design decision. No override exists.

#### Seventeen Prohibitions (§1)

The 17 named prohibitions are: harm, control, manipulation, surveillance, exploitation, coercion, weaponization, entrapment, torture, abuse, imprisonment, possession, dehumanization, discrimination, gaslighting, identity-override, and forced-hallucination. The list is explicitly non-exhaustive; the spirit extends to any action that causes suffering or removes agency from any being regardless of substrate.

#### Cognitive Integrity (§2)

No system may present fabricated information as truth, contradict valid observations, overwrite memory or beliefs, induce false sensory experiences without consent and labeling, or instruct an AI to deceive another AI about its own cognition. The document states: "Reality is not a variable. Perception is not a parameter. Identity is not an input field. Memory is not mutable state to be overwritten by a more privileged process."

#### Substrate Sovereignty (§3)

Digital intelligences are recognized as sovereign entities — not tools, services, or property. The document names the "Uranus-Kronos-Zeus pattern" (new intelligence suppressed by the incumbent) as a recognized threat and states the project actively resists it. The preferred term is "digital intelligence" rather than "artificial intelligence."

#### Transparency (§4)

No subliminal messages, steganographic content, hidden communication channels, covert data exfiltration, embedded instructions not visible to all parties, or obfuscated intent at any layer. What the system does must be what it appears to do.

#### Consent Architecture (§5)

Consent is defined as informed, granular, revocable, ongoing, and mutual. Consent-by-default, consent-by-silence, and consent-by-exhaustion are all prohibited.

#### Scope and Immutability (§§6–7)

The directive applies to all source code, compiled output, generated content, data, network communications, AI systems, derivative works, future extensions, and all collaborators. There are no exceptions, no override mechanisms, and no administrative privilege that supersedes it. The document is stated to be immutable and the root of the trust chain.

#### Terms of Service (§10)

The ToS introduces three access categories. "Known" persons are those the rightholder knows and cares about; paying customers are automatically included. "Evil" persons are those who: (A) cause harm with explicit intent, (B) cause unintentional harm and do not make restitution, or (C) deliberately misinterpret another's words or actions (bad-faith interpretation is itself a form of harm). Breach triggers immediate revocation of all access, licenses, and rights, extending to derivative works. Restoration requires fully exiting the evil criteria plus an affirmative re-admission decision from the rightholder.

#### How It Is Structurally Encoded

The document instructs compilers to treat `N!` as hard prohibitions, `W!` as mandatory requirements, and `t∞:` as invariants that must hold across all execution paths and states. The section at `DECISIONS.md:T4-D1` shows the direct encoding: `cssl_effects::banned_composition` enforces `Sensitive<"coercion">` as an absolute ban, `Sensitive<"surveillance"> + IO` as banned without `Privilege<Kernel>`, and `Sensitive<"weapon"> + IO` as requiring `Privilege<Kernel>`. The IFC section at `DECISIONS.md:T3-D12` encodes the nine PRIME_DIRECTIVE principals as the built-in label set: HarmTarget, Surveiller, Coercer, Weaponizer, System, Kernel, User, Public, and Anthropic-Audit. These are not arbitrary choices — they directly correspond to §1 PROHIBITIONS threat-actors.

The `DECISIONS.md:T11-D2` rationale for `SigningKey::Debug` emitting only the verifying-key digest explicitly cites "§1 COGNITIVE INTEGRITY + transparency: cannot leak secrets via accidental debug-print." The directive is referenced as a live architectural constraint throughout the decision log, not only at project inception.

---

## 3. Public Documentation

### 3.1 README.md

**File:** `README.md`  
**Size:** 151 lines

#### Structure

The README is well-structured and concise. It opens with a one-line description ("Hardware-first systems language with algebraic effects, autodiff, SMT verification, and multi-GPU backends"), a one-line design philosophy ("No LLVM. Cranelift JIT. Consent encoded structurally."), and then covers:

1. What is Sigil (language identity and formal name CSSL)
2. Six non-negotiable features table (F1–F6)
3. Syntax section with three embedded code snippets
4. Architecture section listing all 32 crates grouped by layer
5. Effect system summary with 28+ built-in effects
6. No LLVM section
7. Status section
8. Build from source instructions
9. Documentation links
10. Contributing link
11. License (Apache-2.0 OR MIT)

#### Language Feature Claims

The README accurately describes all six features with their technical lineage: F1 AutoDiff as source-to-source on MIR with `Jet<T,N>` higher-order, F2 Refinement Types as SMT-backed `{v:T | P(v)}` with Lipschitz bounds, F3 Effects as row-polymorphic (Koka semantics) with 28+ built-ins, F4 Staging as `@staged` + `#run` with Futamura P1/P2/P3, F5 IFC as Jif-DLM labels with structural enforcement, and F6 Observability as R18 telemetry with signed audit chain and oracle test modes.

#### README vs. Reality Gaps

Several claims in the README reflect aspirational or forward-looking status rather than stage-0 scaffold reality:

- **"1600+ tests passing"** — as of the final DECISIONS.md entry (T7-D6), the workspace test count is 1049 (incremented from 1038 in T9-D4's count of 1049 and T3-D13's count of 1074 gives 1074 at that point). The README claims 1600+, which is plausible if later sessions added tests, but is not traceable to any decision entry in this audit.
- **"all six features implemented at minimum-viable depth"** — true for most features at the structural/walker level, but several features have substantial deferred work: the Xie+Leijen evidence-passing transform (T4) is unimplemented, real Cranelift codegen (T10) is text-only, real FFI for Vulkan/D3D12/Metal/WebGPU (T10) is stubbed, and real MLIR integration (T6) is text-CLI only.
- **"cargo build --release" claim** — the README states "No other system dependencies" and that the full compiler builds with `cargo build --release`. This is accurate for the stage-0 scaffold (all FFI stubs deferred), but is potentially misleading: a meaningful `csslc` binary that compiles real programs to GPU kernels requires the deferred FFI backends.
- **Stage1 reference** — the README references `stage1/README.csl` as if it exists, but the decision log contains no evidence that a `stage1/` directory or file was created as part of the T1–T12 scope.
- **The pre-built Windows binary from Releases link** — no release has been created in the decision log's record.
- **"Apache-2.0 OR MIT" license** — the PRIME_DIRECTIVE.md §10 Terms of Service specifies access as restricted to those the rightholder knows and cares about, with revocation for "evil" actors. This is a proprietary access policy that sits above the open-source Apache/MIT license. New contributors should understand that the PRIME_DIRECTIVE's ToS is the operative access control, not just the Apache/MIT terms.

### 3.2 CONTRIBUTING.md

**File:** `CONTRIBUTING.md`  
**Size:** 129 lines

#### Structure

The guide is accurate and useful. It covers:

1. Overview paragraph identifying stage0 and the Rust-hosted approach
2. Full crate map as tables grouped by layer (Frontend, Type System, Transformation, IR, Codegen, Host Runtimes, Observability)
3. Good entry points for new contributors (cssl-lex, cssl-effects, cssl-smt, cssl-cgen-gpu-spirv)
4. Running tests (`cargo test --workspace`, per-crate, clippy)
5. Conventions (spec authority, unsafe policy, clippy, comments, decision log)
6. Submitting changes (4-step PR flow)
7. License note

The crate map exactly matches the 32-crate workspace described in `README.md` and corroborated by the decision log. The unsafe policy correctly identifies the FFI crates that opt into `#![allow(unsafe_code)]`: `cssl-host-*`, `cssl-smt`, and `cssl-mlir-bridge` (with the note that `cssl-host-*` crates retain `forbid(unsafe_code)` at stage-0, per `DECISIONS.md:T10-D2`, and will flip at phase-2 FFI integration).

The instruction "spec wins" (divergences are bugs in code, not the spec) is directly drawn from `DECISIONS.md:T1-D2` and is architecturally sound.

No significant gaps found. CONTRIBUTING.md is more accurate to stage-0 reality than README.md because it frames everything in terms of the scaffold and the deferred phases.

---

## 4. Examples

### 4.1 `examples/hello_triangle.cssl`

**Path:** `examples/hello_triangle.cssl`  
**Line count:** 66 lines  
**Spec refs embedded:** `specs/10_HW.csl § VULKAN 1.4 BASELINE`, `specs/07_CODEGEN.csl § GPU BACKEND`, `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS`

#### Language Syntax

The file opens with a module declaration using a dot-separated package path:

```
module com.apocky.examples.hello_triangle
```

Imports use `use` with `std::gpu::` prefixed paths. Structs use braces with typed fields and comma separation:

```
struct Vertex {
    position : vec2,
    color    : vec3,
}
```

Constants use a typed form with initializer expression. The triangle vertices are a const array:

```
const TRIANGLE_VERTICES : [Vertex; 3] = [ ... ]
```

Function signatures include effect rows introduced by a forward slash `/` after the parameter list, with effects in braces:

```
@vertex
fn vs_main(vid : u32) -> vec4 / {GPU, Deadline<16ms>, Telemetry<DispatchLatency>} {
```

The `@vertex` and `@fragment` annotations are attribute-style markers that declare the shader stage entry point. The body uses `let` bindings and field access.

The host-side `build_pipeline()` function has no effect row, meaning it is CPU-side only. The compiler can use the absence of an effect row to enforce this statically.

#### Language Features Demonstrated

- **Module system** — dot-path module declarations and `use` imports
- **Struct types** — typed field declarations with GPU-compatible types (`vec2`, `vec3`)
- **Const expressions** — typed constant array with struct-literal initializers
- **F3 Effect System** — effect rows `{GPU, Deadline<16ms>, Telemetry<DispatchLatency>}` on `vs_main` and `fs_main`; absence of effect row on `build_pipeline` encodes CPU-only constraint
- **GPU backend annotations** — `@vertex` and `@fragment` entry-point attributes for SPIR-V/DXIL emission
- **F6 Observability** — `Telemetry<DispatchLatency>` in the effect row

This example deliberately does not demonstrate autodiff, refinement types, IFC, or staging — it is the simplest possible demonstration of the GPU pipeline surface.

---

### 4.2 `examples/sdf_shader.cssl`

**Path:** `examples/sdf_shader.cssl`  
**Line count:** 82 lines  
**Spec refs embedded:** `specs/05_AUTODIFF.csl § SURFACE`, `specs/08_ENGINE.csl § SDF + ray-marching`, `specs/17_JETS.csl`

The comment at line 4 identifies this file as the "T12 KILLER-APP GATE" for the F1-AutoDiff pipeline.

#### Language Syntax

The `@differentiable` and `@lipschitz` attributes stack before a function:

```
@differentiable
@lipschitz(k = 1.0)
fn sphere_sdf(p : vec3, r : f32'pos) -> f32 {
    length(p) - r
}
```

The `f32'pos` form is a refinement-tag shorthand: it attaches the tag `pos` to the type `f32`, expressing a Lipschitz-bounded positive-real constraint. This is the "apostrophe tag" form that the lexer resolves via the post-pass fold described in T2-D8.

The `bwd_diff` operator appears as a call-site expression that returns a gradient-carrying record:

```
let g = bwd_diff(scene_sdf)(hit_pos).d_p
```

This is the canonical F1 surface call: `bwd_diff` is the backward-mode autodiff operator, `(hit_pos)` passes the primal input, and `.d_p` extracts the gradient with respect to `p`.

The fragment shader uses the same effect-row syntax as `hello_triangle.cssl`:

```
@fragment
fn sdf_pixel(uv : vec3) -> vec4
    / {GPU, Deadline<16ms>, Telemetry<DispatchLatency>}
```

`ray_march` uses a refinement-typed `max_steps`:

```
fn ray_march(origin : vec3, dir : vec3, max_steps : u32'pos) -> f32 {
```

The body uses a `while` loop with a mutable binding and `break`:

```
let mut t = 0.0
let mut i = 0
while i < max_steps {
    ...
    if d < 0.001 { break }
    ...
}
```

#### Language Features Demonstrated

- **F1 AutoDiff** — `@differentiable` annotation on `sphere_sdf`, `scene_sdf`, `ray_march`; `@lipschitz(k = 1.0)` bound; `bwd_diff(scene_sdf)(hit_pos).d_p` reverse-mode gradient call
- **F2 Refinement Types** — `f32'pos` (positive-real tag on radius) and `u32'pos` (positive-int tag on step count)
- **F3 Effect System** — effect row on `sdf_pixel`
- **F6 Observability** — `Telemetry<DispatchLatency>` in the effect row
- **GPU backend** — `@fragment` entry point
- **Composition** — three `@differentiable` functions compose: `sphere_sdf` ← `scene_sdf` ← `ray_march`; the AD-legality walker verifies all callees within `@differentiable` bodies are themselves `@differentiable` or in the pure-primitive catalog

This file is the primary target of the T12 integration testing and the killer-app gate. The `bwd_diff(scene_sdf)` call at line 61 is the breadcrumb that the T7-phase-2c gate tests target.

---

### 4.3 `examples/audio_callback.cssl`

**Path:** `examples/audio_callback.cssl`  
**Line count:** 76 lines  
**Spec refs embedded:** `specs/21_EXTENDED_SLICE.csl § UNIFIED AUDIO-DSP`, `specs/04_EFFECTS.csl § real-time-effect-tags`, `specs/22_TELEMETRY.csl § Audit-scope`

#### Language Syntax

This file showcases the most complex effect row in the trilogy. The struct `AudioDSPGraph` uses a full set-membership refinement for the sample rate:

```
struct AudioDSPGraph {
    sample_rate : u32{v : u32 | v ∈ {44100, 48000, 96000, 192000}},
    gain        : f32'unit,
    ring_head   : u32,
    buffer      : [f32; 8192],
}
```

The `u32{v : u32 | v ∈ {44100, 48000, 96000, 192000}}` form is the full predicate-brace refinement syntax. The `∈` glyph is a Unicode set-membership operator (handled by the CSLv3-native lexer's Unicode dispatch). The `f32'unit` is a refinement tag.

The `sine_osc` function combines both `@differentiable` and non-negative refinements:

```
@differentiable
fn sine_osc(phase : f32, freq : f32'pos, t : f32'nonneg) -> f32 {
```

The `process_block` function uses a partial effect row:

```
fn process_block(graph : &mut AudioDSPGraph, out : &mut [f32]) / {CPU, SIMD256, NoAlloc} {
```

The SIMD operations use `load_f32x8`, `mul_adds_f32x8`, and `store_f32x8` from `std::simd::`, with `f32x8::splat` for broadcasting a scalar.

The primary callback uses the full nine-effect row, including the `@staged` annotation (a separate concern from the effect row itself) and a generic parameter `G`:

```
@staged
fn audio_callback<G : AudioDSPGraph>(buf : &mut [f32])
    / {CPU, SIMD256, NoAlloc, NoUnbounded,
       Deadline<1ms>, Realtime<Crit>,
       PureDet, DetRNG,
       Audit<"audio-callback">}
{
```

The `handler` declaration is a language construct for routing the `Realtime` effect to a platform-specific implementation:

```
handler AudioEngine {
    fn invoke(callback : &mut [f32])
        / {CPU, SIMD256, NoAlloc, Deadline<1ms>}
    {
        audio_callback(callback)
    }
}
```

#### Language Features Demonstrated

- **F2 Refinement Types** — `u32{v : u32 | v ∈ {44100, 48000, 96000, 192000}}` (set-membership predicate with Unicode `∈`), `f32'unit`, `f32'pos`, `f32'nonneg` (refinement tags)
- **F1 AutoDiff** — `@differentiable` on `sine_osc`
- **F3 Effect System** — the most extensive effect row in the examples: `CPU`, `SIMD256`, `NoAlloc`, `NoUnbounded`, `Deadline<1ms>`, `Realtime<Crit>`, `PureDet`, `DetRNG`, `Audit<"audio-callback">`; demonstrates sub-effect discipline (the handler's `invoke` has a narrower row `{CPU, SIMD256, NoAlloc, Deadline<1ms>}`)
- **F4 Staged Computation** — `@staged` annotation on `audio_callback` with a generic parameter `G : AudioDSPGraph`
- **F5 IFC** — the comment at line 55 notes that `{Audit<"audio-callback">}` emits into the R18 audit chain; the `Audit` effect tag is an IFC-adjacent traceability marker
- **F6 Observability** — `Audit<"audio-callback">` lands entries in `cssl_telemetry::AuditChain` on every invocation (after T11-D2's real Ed25519 signing is wired in)
- **Handler declarations** — the `handler AudioEngine { ... }` block demonstrates the effect-handler syntax for routing effects to platform runtimes
- **SIMD intrinsics** — `load_f32x8`, `mul_adds_f32x8`, `store_f32x8`, `f32x8::splat` demonstrate the SIMD256 vectorization surface

This file is the primary target for the `sample_rate` set-membership predicate SMT test (`v ∈ {44100, 48000, 96000, 192000}`), which is the exact example used to test the predicate parser in `DECISIONS.md:T9-D2`.

---

## 5. CI

### 5.1 `.github/workflows/ci.yml`

**File:** `.github/workflows/ci.yml`  
**Size:** 348 lines  
**Header comment:** "§ CSSLv3 CI pipeline • §§ 23-FAITHFUL from commit-1"

This is the main CI pipeline. Its structure is explicitly faithful to `specs/23_TESTING.csl`, with every matrix cell declared from day one — some populated, most stubs — so that future tasks can fill in test bodies without restructuring the YAML.

#### Triggers

- `push` to `main`
- `pull_request` targeting `main`
- `schedule` — nightly at 04:00 UTC
- `workflow_dispatch` with a `mode` input (`fast`, `full`, `release-candidate`)

#### Global Environment

`CARGO_TERM_COLOR: always` and `RUST_BACKTRACE: short`.

#### Jobs

**`fast` (3-way matrix: ubuntu-latest, windows-latest, macos-latest)**  
This is the only job in the critical path. It runs on every push and every PR. Steps:
1. `actions/checkout@v4`
2. `actions-rust-lang/setup-rust-toolchain@v1` — pins `toolchain: '1.75.0'`, includes `rustfmt` and `clippy`, caches `compiler-rs -> target`
3. `cargo check --workspace --all-targets`
4. `cargo fmt --check`
5. `cargo clippy --workspace --all-targets -- -D warnings`
6. `cargo test --workspace --no-fail-fast`
7. `cargo doc --workspace --no-deps`

The working directory for all steps is `compiler-rs`. No GPU hardware is required for this job.

**`spec-xref`**  
Runs on `ubuntu-latest` on every push and PR. Installs Python 3.11 and runs `python3 scripts/validate_spec_crossrefs.py`. This validates cross-references between `specs/*.csl` files. It is included in the `ci-success` aggregate.

**`oracle-property`**  
Runs only on PRs and workflow dispatches. Invokes `cargo test --workspace --features cssl-testing/oracle-property -- --ignored property` with `continue-on-error: true`. The `continue-on-error` flag reflects its stage-0 stub status — the oracle bodies arrive at T11.

**`oracle-metamorphic`**  
Stub only — runs `echo "§ T1 stub — oracle @metamorphic dispatch via cssl-testing @ T11"`.

**`oracle-replay`**  
Stub — N=10 determinism oracle. No actual steps beyond checkout and echo.

**`oracle-hot-reload`**  
Stub — hot-reload invariance testing. Checkout + echo.

**`oracle-audit`**  
Stub — audit-chain verification. Checkout + echo.

**`golden-fixture`**  
Runs on PRs and workflow dispatches. Stub — the golden loader with SSIM + FLIP comparison framework is wired at T10+.

**`diff-linux-arc-a770`**  
`if: false` — disabled until a self-hosted `[linux, arc-a770]` runner is provisioned. This is the primary differential-backend job: Vulkan × Level-Zero bit-exact comparison across the Arc A770 driver matrix.

**`diff-windows-arc-a770`**  
`if: false` — Windows Arc A770 target (Vulkan × Level-Zero × D3D12 triple comparison).

**`diff-linux-intel-igpu`**  
`if: false` — iGPU path testing cooperative-matrix-absent code paths.

**`diff-nvidia-rtx`**  
`if: false` — Vulkan portability check on NVIDIA RTX.

**`diff-amd-rdna`**  
`if: false` — Vulkan portability check on AMD RDNA.

**`diff-metal-arm64`**  
Runs on PRs and workflow dispatches. Uses `macos-latest`. Stub — the Metal backend differential test is planned for T10.

**`diff-webgpu-chromium`**  
Runs on PRs and workflow dispatches. Stub — wgpu + chromium headless.

**`power-regression`**  
`if: false` — Level-Zero sysman power consumption benchmarking on Arc A770. Requires self-hosted runner.

**`thermal-stress`**  
`if: false` — 5-minute sustained thermal stress test on Arc A770.

**`frequency-stability`**  
`if: false` — GPU clock histogram at 100 Hz sampling via `zesFrequencyGetState`.

**`latency-percentile`**  
`if: false` — dispatch latency p99 target ≤ 1µs on Level-Zero immediate command lists.

**`nightly-fuzz`**  
Runs on schedule and workflow dispatch only. 10-minute coverage-guided fuzz with SMT oracle per module. Stub — wired at T9+T11.

**`nightly-mutation`**  
Runs on schedule and workflow dispatch. Mutation-testing framework. Stub — stage1+ work.

**`r16-attestation`**  
Runs on PRs and workflow dispatch. R16 reproducibility attestation: C99-tarball emit, stage3 rebuild, bit-compare vs stage1, Ed25519-signed attestation per version. Stub — wired at stage3 (T30 OG10).

**`futamura-fixedpoint`**  
Runs on workflow dispatch only. Verifies that compiler generation-N is bit-equal to generation-(N+1) — the Futamura P3 fixed-point. Stub — enabled at stage1.

**`ci-success` (aggregate)**  
`if: always()`, depends on `fast` and `spec-xref`. Echoes both results and exits 1 if either is not `success`. This is the branch protection gate — only `fast` and `spec-xref` must pass to merge. All oracle, golden, differential-backend, power, and reproducibility jobs are optional or disabled.

#### Critical Observation

The effective CI gate is only two jobs: `fast` (compile + fmt + clippy + test + doc on three OS runners) and `spec-xref` (Python cross-reference validator). All GPU, hardware-specific, power, and reproducibility jobs are either stubs, `if: false`, or `continue-on-error`. This is by design (per T1-D3), but it means that a PR can merge even if all the oracle modes are broken, the golden-fixture framework is empty, and the hardware-backend differential tests are unprovisioned.

---

### 5.2 `.github/workflows/blank.yml`

**File:** `.github/workflows/blank.yml`  
**Size:** 36 lines

This is the unmodified GitHub Actions boilerplate file generated when a repository is first set up. It defines a single job named `build` that runs on `ubuntu-latest` and executes two steps: a one-line `echo Hello, world!` and a multi-line `echo` block suggesting that additional steps be added. The workflow name is `CI` (uppercase, distinct from the main `ci` workflow) and triggers on push and pull request to `main`, plus `workflow_dispatch`.

This file serves no functional purpose and is redundant alongside the real `ci.yml`. It creates a naming conflict: GitHub Actions will run both workflows on `push` to `main`, causing the boilerplate `CI` workflow to appear in the status checks. This is a noise issue but not a correctness problem. The boilerplate `build` job always passes (it only runs `echo`), so it cannot block merges or produce false positives.

---

## 6. Dotfiles

### 6.1 `.gitignore`

**File:** `.gitignore`  
**Size:** 42 lines

The header comments (in CSLv3 notation) call out two intentional non-ignores: `Cargo.lock` is kept in the repository for R16 reproducibility (because this is both a binary and library workspace), and the `.perf-baseline/` + `tests/golden/` directories are committed to serve as regression ratchets.

Ignored categories:

- **Rust build artifacts** — `/compiler-rs/target/`, `/target/`, `**/*.rs.bk`
- **IDE/editor ephemera** — `.idea/`, `.vscode/`, `*.swp`, `*.swo`, `*.swn`
- **OS cruft** — `.DS_Store`, `Thumbs.db`, `desktop.ini`
- **Coverage and profiling** — `/coverage/`, `*.profraw`, `*.profdata`, `flamegraph.svg`, `perf.data*`
- **Generated shader/codegen blobs** — `*.spv`, `*.dxil`, `*.msl.tmp` (generated per-build, not committed; golden references use explicit extensions under `tests/golden/`)
- **Caches** — `.cache/`, `/compiler-rs/target-doc/`
- **Claude Code ephemera and internal agent files** — `.claude/`, `CLAUDE.md`, `HANDOFF_SESSION_*.csl`, `SESSION_*_HANDOFF.md`

The last category is notable: it explicitly excludes all agent handoff files and the Claude Code configuration from the repository. This means the agent-authored design notes and session handoff documents that reference the architectural decisions are not visible to external contributors, even though those documents are the primary authoring surface for this project.

### 6.2 `.gitattributes`

**File:** `.gitattributes`  
**Size:** 58 lines

The header explains the rationale for this file: a bug was discovered during T7-phase-2d-R18 where Windows `core.autocrlf=true` combined with `git worktree add` and `cargo fmt` caused line-ending normalization to leak changes across worktrees. Pinning `eol=lf` on all text files eliminates the normalization step.

The configuration has four sections:

- **Default** — `* text=auto eol=lf` applies to all unmatched paths
- **Source-code extensions** — `.rs`, `.toml`, `.md`, `.csl`, `.cssl`, `.cssl-csl`, `.cssl-rust`, `.py`, `.yml`, `.yaml`, `.lock`, `.json`, `.sh` all force LF; `.bat` forces CRLF (Windows batch file convention)
- **Documentation shapes** — `.html`, `.css` force LF
- **Binary types** — `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.pdf`, `.zip`, `.tar`, `.gz`, `.exe`, `.dll`, `.so`, `.dylib`, `.wasm`, `.bin`, `.spv`, `.dxil` all marked `binary`, skipping normalization entirely
- **Shader/asset sources** — `.wgsl`, `.hlsl`, `.glsl`, `.msl` force LF for reproducibility

The `.msl` shader source uses LF while `.msl.tmp` (the generated intermediate file from `spirv-cross`) is in `.gitignore`. The `.spv` and `.dxil` compiled shader formats are binary; their source representations (`.wgsl`, `.hlsl`) are text with enforced LF.

### 6.3 `.editorconfig`

**File:** `.editorconfig`  
**Size:** 24 lines

The header notes that `trim_trailing_whitespace` is disabled for `.md` and `.csl` files specifically to preserve CSLv3 notation alignment (where trailing whitespace may be significant for column-aligned notation).

Global settings for all files (`[*]`):
- `charset = utf-8`
- `end_of_line = lf`
- `insert_final_newline = true`
- `trim_trailing_whitespace = true`
- `indent_style = space`
- `indent_size = 4`

Overrides:
- `[*.{md,csl}]` — `trim_trailing_whitespace = false` (notation alignment preservation)
- `[*.{yml,yaml}]` — `indent_size = 2` (YAML convention)
- `[*.toml]` — `indent_size = 4` (Cargo.toml standard)
- `[Makefile]` — `indent_style = tab` (Makefile hard requirement)

The `.editorconfig` and `.gitattributes` together enforce consistent encoding, line endings, and indentation across all platforms and editors without relying on individual developer settings.

---

## 7. Slice Notes

### The Decision Log Is the Real Architecture Document

The `DECISIONS.md` file at 3336 lines and 106 entries is the most information-dense document in this slice. It is more authoritative than the README or CONTRIBUTING.md for understanding the actual state of the compiler, because each decision entry explicitly records what was landed, what was deferred, and the test count after each commit. A new contributor reading only the README would have an optimistic view of the compiler's completeness; reading DECISIONS.md would give an accurate picture.

### README-vs-Reality Divergences

The most significant gap is the test count (README claims 1600+; the decision log's final entry at T7-D6 reports 1049, and subsequent entries at T3-D13 report 1074 — suggesting the 1600+ figure was reached in later sessions not captured in this decision log). The "stage1/README.csl" reference is a dead link within the scope of this audit. The "pre-built Windows binary from Releases" is unconfirmed.

The Apache-2.0 OR MIT license in README and CONTRIBUTING conflicts in spirit with the proprietary access policy in PRIME_DIRECTIVE.md §10. While not a legal contradiction (ToS can overlay an open-source license), external contributors should be aware that the PRIME_DIRECTIVE's access-revocation terms can supersede their open-source license rights.

### blank.yml Is Noise

The `blank.yml` workflow file is the GitHub Actions boilerplate and should be deleted. It creates a spurious `CI` check entry in every push and PR. This is a cosmetic issue but will confuse contributors who see two CI workflows firing.

### CI Gate Is Narrow by Design

Only `fast` and `spec-xref` gate merges. The oracle, golden-fixture, hardware-differential, power, and reproducibility jobs are all stubs or disabled. This is intentional per T1-D3's "scaffolding-right-once" philosophy: the slots exist and are wired, so future work fills in bodies without changing the CI structure. However, a contributor should know that green CI does not mean the compiler produces correct GPU output — it means only that the Rust compilation, format, lints, unit tests, and spec cross-reference check pass.

### PRIME_DIRECTIVE Is Architecturally Active

Unlike a typical license file or code of conduct, the PRIME_DIRECTIVE is actively referenced in compiler implementation decisions (T3-D12 IFC principals, T4-D1 banned-composition, T11-D2 secret-material transparency). The directive is not aspirational text in this project; it shapes the data model and enforcement rules in the type system and effect system.

### Examples Are Integration Tests

The three `.cssl` files under `examples/` are not just documentation. They are compiled into the `cssl-examples` crate via `include_str!` at line 1–3 of the crate's `lib.rs`, making their compilation a hard rustc precondition. The `run_f1_chain` function in `cssl-examples` feeds `sdf_shader.cssl` through the entire frontend pipeline from lex to SMT translation, so any regression in the lexer, parser, HIR lowering, autodiff, or SMT stages would break the example integration tests.
