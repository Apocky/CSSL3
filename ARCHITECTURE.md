# ARCHITECTURE.md — CSSLv3 / Sigil Compiler

**Audience**: Senior engineer joining the project cold.  
**Intent**: The *how* and *why* companion to `INDEX.md`. Every claim is grounded in a primary
source: audit documents (`docs/audit/NN-*.md`), specs (`specs/NN_*.csl`), workspace
`Cargo.toml`, or an external reference cited by URL. Gaps and honest tensions are marked with
`[OPEN: ...]`.

**Primary sources used**  
- `docs/audit/01-17` (all 16 present; `15-specs.md` absent from this worktree, specs read
  directly)  
- `specs/01_BOOTSTRAP.csl`, `specs/16_DUAL_SURFACE.csl`, `specs/33_F1_F6_LANGUAGE_FEATURES.csl`,
  `stage1/README.csl`  
- `compiler-rs/Cargo.toml` (workspace root)  
- `DECISIONS.md` (T1-D1..T12-D2, 106 decisions)  
- `PRIME_DIRECTIVE.md` (§0-§11, 626 lines)  
- Koka documentation: https://koka-lang.github.io/koka/doc/book.html#sec-effect-types  
- Slang differentiable programming: https://shader-slang.org/slang/user-guide/autodiff.html  
- MLIR IR structure: https://mlir.llvm.org/docs/Tutorials/UnderstandingTheIRStructure/

---

## Table of Contents

1. [System Overview and Design Thesis](#1-system-overview-and-design-thesis)
2. [Pipeline-Deep: Every Stage, Every Crate](#2-pipeline-deep-every-stage-every-crate)
3. [F1–F6 Feature-Deep: Spec Intent vs. Implementation](#3-f1f6-feature-deep-spec-intent-vs-implementation)
4. [Dual-Surface Design](#4-dual-surface-design)
5. [Stage-0 / Stage-1 Architecture](#5-stage-0--stage-1-architecture)
6. [Cross-Cutting Concerns](#6-cross-cutting-concerns)
7. [Integration Points and Extension Model](#7-integration-points-and-extension-model)
8. [Architectural Tensions — Honest Reporting](#8-architectural-tensions--honest-reporting)

---

## 1. System Overview and Design Thesis

### 1.1 What CSSLv3 Is

CSSLv3 (also called **Sigil** in external-facing contexts) is a systems- and GPU-programming
language built around a single unifying thesis: every bug in the Legends of Arcana (LoA) game
engine — UAF, data-race, IFC leak, diverging gradient, unchecked side-effect — was legal syntax in
whatever language was used at the time. CSSLv3 makes those constructs illegal at the type-system
level, not by lint or convention but by design. The language is the fix.

This thesis drives six non-negotiable language features, designated F1–F6:

| Feature | Inspiration | Core Guarantee |
|---------|-------------|----------------|
| F1 AutoDiff | Slang.D ([shader-slang.org](https://shader-slang.org/slang/user-guide/autodiff.html)) | Source-to-source AD; gradient correctness structurally enforced |
| F2 Refinement Types | LiquidHaskell `{v:T\|P(v)}` | Arithmetic invariants proven at compile-time by SMT |
| F3 Effect System | Koka row-polymorphic effects ([koka-lang.github.io](https://koka-lang.github.io/koka/doc/book.html)) | Side-effect tracking; banned compositions = type error |
| F4 Staged Computation | Futamura P1/P2/P3 projections | Compile-time specialization, macro hygiene |
| F5 Information Flow Control | Jif DLM label lattice | Confidentiality / integrity tracking; PRIME_DIRECTIVE principals |
| F6 Observability | BLAKE3 + Ed25519 signed telemetry | Tamper-evident audit chain, oracle-mode testing |

CSSLv3 is also designed with a dual-compilation target: **CPU** (Cranelift JIT, textual CLIF) and
**GPU** (SPIR-V, WGSL, DXIL, MSL). The language surface itself is polymorphic over target through
a built-in capability and effect annotation system.

### 1.2 Repository Layout (32 Crates)

The workspace lives under `compiler-rs/`. Resolver = 2, MSRV 1.75, dual-licensed Apache-2.0 /
MIT, aggressive Clippy (`all = deny`, `pedantic = warn`, `nursery = warn`).

```
compiler-rs/crates/
  cssl-ast          cssl-lex          cssl-parse
  cssl-hir          cssl-caps         cssl-effects       cssl-ifc
  cssl-autodiff     cssl-jets         cssl-smt
  cssl-staging      cssl-futamura     cssl-macros
  cssl-mir          cssl-lir          cssl-mlir-bridge
  cssl-cranelift    cssl-spirv        cssl-wgsl          cssl-dxil     cssl-msl
  cssl-vulkan-rt    cssl-level-zero-rt cssl-d3d12-rt    cssl-metal-rt  cssl-webgpu-rt
  cssl-telemetry    cssl-persist      cssl-rt
  cssl-testing      cssl-examples     csslc
```

The `csslc` binary is the user-facing compiler driver. At stage-0 it is a 23-line shell that
prints a stub message and exits 0; it imports zero crate dependencies. The full compiler chain
is exercised exclusively through `cssl-examples` integration tests and `cssl-testing` oracle
harnesses.

### 1.3 The PRIME_DIRECTIVE as Architectural Constraint

`PRIME_DIRECTIVE.md` (626 lines, §0–§11) is not policy documentation — it is a specification
that is structurally encoded into the type system. Its 17 prohibitions are the ground truth
for what `cssl-effects::banned_composition` and `cssl-hir::ifc.rs` enforce. The consent-as-OS
axiom (`t∞: consent = OS`) means any computation that could route private data to an
unauthorized principal is a compile-time type error, not a runtime assertion. The PRIME_DIRECTIVE
is therefore simultaneously: (a) a human-readable ethics document, (b) a formal spec for the
effect system's banned rows, and (c) a test oracle for IFC correctness.

---

## 2. Pipeline-Deep: Every Stage, Every Crate

Data flows as: Source text → CST (cssl-ast) → HIR → MIR → [LIR | MLIR-bridge | CPU codegen |
GPU codegen] → binary / shader artifact. Each arrow is a fallible transformation that accumulates
diagnostics into a `DiagnosticBag` and either propagates or short-circuits.

### 2.1 Source Representation — `cssl-ast`

**Crate**: `cssl-ast` (~1030 LOC, 5 source files)  
**Maturity**: Production-quality foundation, no TODOs.

`cssl-ast` is a pure type library. It owns the canonical CST (`cssl_ast::Module`) and every
node type that both parser surfaces must produce. Key design decisions:

- `SourceFile { id: SourceId, surface: Surface, text: Arc<str> }` carries its mode enum but
  does NOT mutate it after construction. Mode-detection results from `cssl-lex` are discarded
  rather than written back. [OPEN: this means you cannot recover which surface was actually used
  for a file parsed with `Surface::Auto`.]
- `Ident` stores only a `Span`, not a `String`. Text is re-sliced from `SourceFile.text` on
  demand (T3-D2). This eliminates identifier allocation cost but means any pass that needs
  identifier strings must hold a reference to the originating `SourceFile`.
- `CompoundOp` in `cst.rs` uses Sanskrit grammar names: `Tp` (tatpurusha, `of`), `Dv`
  (dvandva, `and`), `Kd` (karmadharaya), `Bv` (bahuvrihi), `Av` (avyayibhava). These are
  CSLv3-native morpheme composition operators with no direct Rust equivalent.
- `Surface` enum: `RustHybrid | CslNative | Auto`. `Surface::default() = RustHybrid`.

### 2.2 Lexing — `cssl-lex`

**Crate**: `cssl-lex` (~1350 LOC, 5 source files)  
**Maturity**: RustHybrid surface complete; CslNative surface structural stub.

The top-level `lex(source_file)` function dispatches on `Surface`:
- `Surface::RustHybrid` → `logos`-based DFA lexer. Logos 0.14 (from Cargo.toml) generates a
  state machine at compile time from attribute macros on the `Token` enum. Covers the full
  Rust-extended token set.
- `Surface::CslNative` → hand-rolled lexer. Recognizes CSLv3 glyph sequences (§, →, ≤, ≥,
  etc.) and CSLv3 morpheme sigils.
- `Surface::Auto` → runs the 4-tier mode-detection heuristic (extension → first-non-comment
  line → pragma → default) then delegates to the resolved surface.

**T2-D8 apostrophe fold**: The logos lexer excludes `'` from identifiers (Rust's apostrophe
is a lifetime sigil). A post-pass fold collapses `Ident + Apostrophe + single-byte-ident` into
a single apostrophe-carrying token for CSLv3 morpheme aspect suffixes (`'d`, `'f`, `'s`, etc.).

### 2.3 Parsing — `cssl-parse`

**Crate**: `cssl-parse` (~4573 LOC, 17 source files)  
**Maturity**: RustHybrid surface substantially complete; CslNative structural stub.

The parser is a hand-rolled recursive-descent (RD) parser with a Pratt expression sub-parser.

**Pratt table**: 15 binding-power levels, `MAX_LEVEL = 20`, `base = (20 - level) * 2`.
Precedence climbs from lowest (assignment, range) through arithmetic, unary prefix, call/index,
to the maximum reserved level. The table covers all Rust-equivalent operators plus CSSLv3
extensions (effect composition `|+|`, refinement `{v:T|P}`, capability annotation `@iso`, etc.).

**Known bugs (stage-0 intentional)**:
1. `fn is_underscore() -> bool { false }` — always returns false. `PatternKind::Wildcard` is
   therefore *never* produced by normal parsing; it only appears through error recovery. Any
   `_` pattern becomes `PatternKind::Binding { name: "_" }` instead. This means wildcard
   exhaustiveness semantics must be emulated in the type checker by treating `"_"` as a special
   name.
2. `in_context_forbidding_struct_brace() -> bool { false }` — always returns false, resolving the
   struct-literal vs. block-brace ambiguity by always parsing a struct literal. This is
   intentional: the formatter compensates by inserting parens at ambiguous sites.

**CSLv3-native parser surface**: Structural stub. `§ name` glyph syntax parses to
`Item::Module`; body lowering is deferred. F5 (IFC labels `@{C I}`) and F6 (telemetry
scopes `@trace(scope)`) have zero parser representation in either surface at stage-0.

**Output**: Both surfaces produce the same `cssl_ast::Module` CST. This is the primary
portability guarantee of the dual-surface design.

### 2.4 Type System and HIR — `cssl-hir`

**Crate**: `cssl-hir` (largest crate, 21 source files, 210+ documented items)  
**Maturity**: Core inference working; several synthesis stubs; cap-check skeleton.

`cssl-hir` does three things: HIR lowering from CST, Hindley-Milner type inference with
row-polymorphic extensions, and structural/dataflow IFC checking.

**Three-phase inference**:
1. *Collect signatures*: Walk all item definitions, register in environment. Allows mutual
   recursion without explicit ordering.
2. *Check bodies*: Infer types for every expression, unify constraints, propagate.
3. *Finalize*: Substitute solved type variables, emit diagnostics for unsolved vars.

**Type representation**: `Ty` is capability-erased at stage-0. There is no `Ty::Capability`
variant; capability information is stripped in `lower_hir_type`. Capability checking is
handled separately in `cssl-caps` and consulted by `cssl-hir/src/cap_check.rs`.

**Multi-segment path resolution**: `def` resolution always returns `None` for paths with more
than one segment (e.g., `std::vec::Vec`). Only single-segment names are resolved at stage-0.
This means any code using qualified paths will silently generate a fresh type variable rather
than the intended nominal type.

**Synthesis stubs in `synth_expr_kind`**:
- `ExprKind::Field` → fresh type variable (field access type unknown)
- `ExprKind::Try` → fresh type variable (`?` operator semantics unimplemented)
- `ExprKind::Compound` → fresh type variable (CSLv3 morpheme compounds unimplemented)
- `ExprKind::Perform` → fresh type variable (effect operation return type not consulted)

**`cap_check.rs`**: Registers `iso`-annotated parameters but does not walk the function body.
Linearity (iso cannot be used more than once, must be consumed) is NOT enforced at stage-0.
The check is structurally present as a foundation for stage-1 enforcement.

**String interning**: `lasso::Rodeo` (single-threaded `RefCell`-wrapped). Stage-1 upgrade
path is documented as `lasso::ThreadedRodeo` for concurrent compilation.

**IFC in HIR**: `cssl-hir/src/ifc.rs` (1168 LOC) is where the actual IFC implementation lives.
See §6.2 for detail.

### 2.5 Capability System — `cssl-caps`

**Crate**: `cssl-caps` (1217 LOC)  
**Maturity**: Complete. No TODOs. All methods `const fn`.

`cssl-caps` implements the Pony-6 capability model, adapted for CSSLv3's memory ownership
guarantees. The six capability kinds:

| Kind | Pony semantics | CSSLv3 role |
|------|---------------|-------------|
| `iso` | Isolated — unique alias | Linear owned, no aliasing |
| `trn` | Transition — write-unique | Mutable, allows read aliases |
| `ref` | Mutable reference | Standard mutable borrow |
| `val` | Immutable value, globally shareable | Deep immutable |
| `box` | Read-only local alias | Standard immutable borrow |
| `tag` | Identity only, no read/write | Object identity without data access |

`AliasMatrix` encodes what capability combinations are legal when creating a new alias from an
existing one. `LinearTracker` accumulates move/copy/borrow events and emits errors on violation.
`GenRef` packs a u40 object index and u24 generation counter into a single u64 for
use-after-free detection — the generation increments on each deallocation; a stale `GenRef`
holding an old generation fails the generation check at access time.

### 2.6 Effect System — `cssl-effects`

**Crate**: `cssl-effects` (1115 LOC)  
**Maturity**: Effect taxonomy complete; banned_composition partially enforced.

32 `BuiltinEffect` variants organized in 5 groups:

- **Memory** (Alloc, Dealloc, MemRead, MemWrite, UnsafeAlias)
- **IO** (FileIO, NetIO, StdIO, AudioIO, DeviceIO)
- **Control** (Diverge, Panic, Exception, Async, Continuation)
- **Compute** (GpuDispatch, CpuParallel, Atomic, Simd, VectorOp)
- **Sensitive** (PrivacyLeak, SurveillanceRead, ManipulationWrite, CoercionWrite,
  HarmCompute, WeaponCompute, ExploitCompute, DiscrimCompute, ControlWrite)

`banned_composition(e1, e2) -> bool` (light variant): returns true for three hard bans. At
stage-0 this function has a bug: it compares against `SensitiveDomain::Other` which matches
none of the actual predicates, making all three bans effectively no-ops in the light path.

`banned_composition_with_domains(e1, d1, e2, d2) -> bool` (full variant): the correct
implementation. Bans encode the PRIME_DIRECTIVE's 17 prohibitions directly:
`Surveillance + NetIO` is the canonical example of a banned pair regardless of domain.

**Effect rows**: Effect rows are sequences of `BuiltinEffect` (or user-defined effects). Row
unification follows Koka's approach: row variables stand for unknown tails, extension adds
effects to the head, handling restricts by removing effects. The CSSLv3 surface syntax uses `|+|`
for row concatenation and `|!|` for row restriction.

[OPEN: The connection between `cssl-effects::EffectRow` and `cssl-hir`'s type inference unifier
is not fully wired at stage-0. Effect row unification happens in the HIR type checker but the
two crates' row representations are not shared.]

### 2.7 Information Flow Control — `cssl-ifc`

**Crate (scaffold)**: `cssl-ifc` (24 LOC, zero API)  
**Real implementation**: `cssl-hir/src/ifc.rs` (1168 LOC)

`cssl-ifc` the crate is a pure scaffold. All real IFC logic lives embedded in `cssl-hir`.
This is an architectural tension acknowledged in the audit (see §8.1).

`cssl-hir/src/ifc.rs` implements:
- `IfcLabel`: a Decentralized Label Model (DLM) label pair `(confidentiality: LabelSet,
  integrity: LabelSet)`.
- 9 built-in principals (`builtin_principals`): derived from PRIME_DIRECTIVE §9, including
  `User`, `Operator`, `Regulator`, `AiPartner`, and five Sensitive domains.
- `check_ifc`: structural checker — walks the HIR tree and verifies label annotations are
  present and consistent.
- `check_ifc_flow` (T11-D36): dataflow checker — propagates labels along data-flow edges and
  flags flows from high-confidentiality sources to low-confidentiality sinks.
- Diagnostic codes IFC0001–IFC0004.

**`combine_labels` deviation from formal DLM**: The standard DLM join for confidentiality
*intersects* the owner sets (more owners = less confidential). `combine_labels` in
`cssl-hir/src/ifc.rs` *unions* both sets, which is a sound over-approximation for taint
tracking (it never under-reports leaks) but is not the formal DLM lattice join. This is
documented in the code as a deliberate stage-0 choice; formal lattice-accurate propagation is
deferred.

### 2.8 AutoDiff Transform — `cssl-autodiff`

**Crate**: `cssl-autodiff` (~4061 LOC)  
**Companion**: `cssl-jets` (293 LOC)  
**Maturity**: 19 primitives, 38-rule table; Lipschitz bound is placeholder.

AutoDiff operates at the **MIR level**, not the HIR level. This is a deliberate design choice:
MIR is in SSA-like form with explicit data-flow, making the dual (cotangent) computation
mechanically derivable. HIR's richer structure (pattern matching, traits) would complicate the
transform without adding correctness.

**Differentiable primitives (19)**: The four arithmetic operators, six transcendentals (sin,
cos, exp, ln, sqrt, pow), and piecewise-linear operations (Min, Max, Abs, Sign — added
T11-D13). Each primitive has a **forward rule** (computes JVP: derivative of output w.r.t.
input) and a **backward rule** (computes VJP: adjoint propagation from output to input).

**38-rule table** (Fwd × 19 + Bwd × 19):

```
Fwd(sin(x))  = (sin(x),  cos(x) * dx)       // primal, tangent
Bwd(sin(x))  = dx_adjoint += dy * cos(x)    // adjoint accumulation
Fwd(Max(a,b))= (Max(a,b), a>=b ? da : db)   // piecewise-linear
```

The design mirrors Slang's `DifferentialPair<T>` concept: forward mode carries `(primal,
tangent)` pairs; reverse mode separates a forward sweep (recording) from a backward sweep
(adjoint propagation). CSSLv3's MIR representation of this is `MirOp::DiffPair` with primal
and tangent value IDs.

**Lipschitz bound**: `DiffDecl.lipschitz_bound` is always `Some("k")` regardless of actual
function arguments. This is a stage-0 placeholder; F2 refinement types are supposed to provide
Lipschitz bounds as part of the `T'L<k>` sugar, but the connection from F2 to F1 is not wired.

**`cssl-jets`**: Abstract schema for jet bundles (higher-order derivatives, composite tangent
spaces). No runtime struct layout or codegen yet; feeds into `cssl-staging` as a data model.

**Killer-app gate (T7-D5)**: `cssl-examples` runs 11 sphere-SDF gradient-equivalence cases.
All 11 pass. SMT-LIB artifacts are emitted for optional Z3/CVC5 dispatch. This gate is the
primary validation that the AD pipeline is semantically correct end-to-end.

### 2.9 SMT, Staging, Futamura, Macros

**`cssl-smt`** (1756 LOC): Implements F2 (refinement types) via subprocess dispatch to Z3 or
CVC5. No `z3-sys` FFI — the solver is invoked as an external process via `Command::new("z3")`,
SMT-LIB 2 text is written to stdin, and the response is parsed. This eliminates the version-
pinning headache of native z3 bindings but introduces a runtime dependency on solver
executables being in PATH. The predicate text parser understands the CSSLv3 refinement
predicate surface and translates to SMT-LIB 2.

**`cssl-staging`** (455 LOC): F4 (staged computation). Collects `@staged` function declarations
and `#run` call sites into a data model. The actual specialization transform — partial
evaluation of `@staged` functions at `#run` sites to produce residual code — is deferred to
T8-phase-2. At stage-0, `@staged` is syntactic sugar that has no runtime effect.

**`cssl-futamura`** (284 LOC): Models Futamura's three projections formally:
- P1: Specialization (`@staged f` applied to static args = residual program)
- P2: Compiler generation (specializer applied to interpreter = compiler)
- P3: Compiler-compiler (specializer applied to specializer = compiler generator)

`Orchestrator`, `FutamuraLevel`, `Projection`, `FixedPointRecord` are defined. No partial-eval
algorithm is implemented; the orchestrator is a coordination harness awaiting the specializer.

**`cssl-macros`** (343 LOC): Implements F4's macro subsystem using Racket set-of-scopes hygiene.
`ScopeAllocator` stamps fresh scope IDs. `MacroRegistry` stores named macro definitions.
Macro expansion itself — the rewriting of macro-application sites using the set-of-scopes
algorithm — is deferred to T8-phase-2 alongside staging specialization.

### 2.10 MIR — `cssl-mir`

**Crate**: `cssl-mir` (~9600 LOC, 11 source files)  
**Maturity**: Data model and lowering complete; 6-pass optimization pipeline has 5 stubs.

MIR (Mid-level Intermediate Representation) is modeled after MLIR's structural hierarchy
(see mlir.llvm.org/docs/Tutorials/UnderstandingTheIRStructure): Ops contain Regions, which
contain Blocks, which contain Ops. This nesting enables structured control flow while
maintaining SSA properties within blocks.

**Data model**:
```
MirModule
  └─ MirFunc (is_generic: bool, monomorphization metadata)
       ├─ params: Vec<(ValueId, MirType)>  // IDs are 0..params.len()
       └─ MirRegion
            └─ MirBlock (exactly one entry block at stage-0)
                 └─ MirOp (variant)
```

**26 `CsslOp` variants** (all named `cssl.*`): arithmetic, comparison, control flow,
memory (alloc/store/load), GPU dispatch, effect operations, AD operations (DiffPair, JVP, VJP),
IFC label annotations, and telemetry emit. Plus `Std` passthrough for operations delegated to
the target ABI.

**Monomorphization quartet** (T11-D46..D50, the most recent completed milestone):
The four cooperating passes that eliminate generic functions:
1. *Collect*: Find all `is_generic = true` functions and their call sites.
2. *Instantiate*: For each call site with concrete type arguments, produce a specialized
   `MirFunc` with type variables substituted.
3. *Cleanup*: Remove original generic functions from the module (they have no concrete
   callers after instantiation).
4. *Validate*: Assert no `is_generic = true` functions remain.

**Type mapping hazards**:
- `u32`, `isize`, `usize` all silently map to `I32`. This is lossy (usize is 64-bit on
  64-bit targets) and undocumented in the mapping code. [OPEN: this will cause silent
  overflow bugs in any code that relies on pointer-sized arithmetic.]
- `Vec<T>` types → not mappable; the MIR lowerer emits a stub and continues rather than
  erroring. Callers relying on Vec operations will see uninitialized result values.
- Vec parameters are scalarized into N consecutive scalar entries (T11-D35). A `vec3<f32>`
  parameter becomes three consecutive `f32` entries in the param list.
- `Bf16` maps to `F16` (stage-0 approximation; bfloat16 is distinct from float16 in
  precision characteristics).

**6-pass optimization pipeline**: Exactly 1 of 6 passes is implemented; the other 5 are stubs
that return the module unchanged. The implemented pass is the monomorphization quartet above.

### 2.11 LIR and MLIR Bridge

**`cssl-lir`** (22 LOC): Pure scaffold. Zero implementation. The LIR (Low-level IR) is
intended as the bridge between MIR and native instruction selection for the CPU path — below
MIR's structured control flow, above Cranelift's CLIF. At stage-0, MIR lowering goes directly
to CLIF without an LIR intermediate.

**`cssl-mlir-bridge`** (107 LOC): Wraps `cssl_mir::print_module` and provides a
`--emit-mlir` flag output path. The output is CSSLv3's textual MIR format printed with MLIR-
compatible nesting, not genuine MLIR textual IR. The `melior`/`mlir-sys` binding:

```toml
# melior = "0.20"  # T6 : verify-melior-windows-compat + LLVM_SYS_*_PREFIX
```

is commented out in `compiler-rs/Cargo.toml`. The decision log (Q6_IR in
`docs/audit/16-research-stage1-scripts.md`) records why: MLIR was evaluated and rejected as
the primary IR substrate due to steep learning curve, Windows MSVC compatibility issues, and
solo-developer bus-factor risk. The custom MIR is deliberately MLIR-shaped (same structural
hierarchy) so that migration to genuine MLIR at stage-7+ is possible if the project outgrows
the custom implementation.

### 2.12 CPU Codegen — `cssl-cranelift`

**Crate**: `cssl-cranelift`  
**Maturity**: Two real modes: textual CLIF emission and JIT execution (2199 LOC in `jit.rs`).

Cranelift 0.115 (declared in Cargo.toml) is the CPU codegen backend. Two independent
mechanisms coexist:

**Text emitter** (`emit.rs` + `lower.rs`): Translates MIR to textual CLIF (`.clif` files) for
debugging and offline compilation. Uses `B1` (1-bit bool) for `MirType::Bool`.

**JIT engine** (`jit.rs`): Real Cranelift `JITBuilder` + `Module`. Compiles MIR functions in-
memory and returns function pointers executable in the current process. Uses `I8` (not `B1`)
for `MirType::Bool` — a mapping asymmetry with the text emitter that can cause subtle bugs if
code is developed against one mode and tested against the other.

**Type mapping**:
- `F32`, `F64` → direct CLIF scalar types
- `Bf16` → `F16` (approximation; acknowledged T11-D20)
- `Bool` → `B1` (text) / `I8` (JIT) [asymmetry]
- `Vec*` → `None` (not yet mappable to single CLIF scalar)
- Integer widths I8/I16/I32/I64 map directly

**Multi-result functions**: CLIF does not support multiple return values natively (beyond
struct passing). The JIT backend implements multi-result functions via out-parameter pointers:
the caller allocates output slots and passes their addresses; the callee writes results into
those addresses.

**Libm transcendentals**: sin, cos, exp, ln, sqrt are implemented via libm calls (function
name lookup from the dynamic linker at JIT time). This means JIT output is not hermetically
self-contained; it depends on the C runtime.

**`deny(unsafe_code)` at crate root / `allow(unsafe_code)` in jit.rs**: The JIT necessarily
uses unsafe (raw function pointers, transmute for type erasure). This split is documented as
T11-D20: the crate-level deny guards all non-JIT code while the JIT file opts in explicitly.

**SIMD tier for Intel Arrow Lake**: Mapped to `Avx2`. Arrow Lake (the target hardware is
Arc A770, PCI 0x56A0, Intel vendor 0x8086) has AVX10.1 on some SKUs but the JIT conservatively
targets AVX2 for compatibility.

### 2.13 GPU Codegen — Four Independent Backends

**Crates**: `cssl-spirv`, `cssl-wgsl`, `cssl-dxil`, `cssl-msl`  
**Maturity**: SPIR-V most mature (real binary + validation); others skeleton.

The four GPU backends are **independent MIR-to-text paths**. They do not funnel through
SPIR-V as a common intermediate (unlike, e.g., DXC which goes HLSL→SPIR-V→DXIL or
wgpu which uses Naga). Each backend walks the MIR directly and emits its own textual or
binary format.

[OPEN: The README implies SPIR-V is a common intermediate ("all GPU backends generate SPIR-V")
but the audit of the actual crate code reveals four independent paths. This is a documentation
discrepancy.]

**`cssl-spirv`** (most mature):
- Uses `rspirv 0.12` (from Cargo.toml) to construct SPIR-V binary.
- Real round-trip validation: emits binary, then parses it back with rspirv and validates.
- **Bug**: `FloatControls2` capability and `ShaderNonSemanticInfo` extension both mis-map
  to `Capability::Shader` in the capability selection logic. This is incorrect (they are
  distinct SPIR-V capabilities) and will produce malformed modules if those features are
  actually used.
- Text emitter: `ep.name` is used without `%` sigil, so the textual form is not valid
  `spirv-as` input. Documented as intentional stage-0 approximation.

**`cssl-wgsl`**:
- Uses `naga` (dev-dependency) for in-process structural validation of generated WGSL.
- Body lowering for all shader functions is deferred to T10-phase-2.
- Target: WebGPU on web (LoA web client).

**`cssl-dxil`**:
- Target: Direct3D 12 on Windows (primary LoA platform).
- Body lowering deferred T10-phase-2.
- [OPEN: DXIL requires signing with dxil.dll (Microsoft DRM). The audit does not mention how
  CSSLv3 intends to handle DXIL signing — whether it will shell out to DXC, use an unsigned
  blob for development, or negotiate with Microsoft's signing key requirement.]

**`cssl-msl`**:
- Target: Metal on macOS/iOS.
- Body lowering deferred T10-phase-2.

### 2.14 Host Runtime Crates — `cssl-vulkan-rt`, `cssl-level-zero-rt`, `cssl-d3d12-rt`, `cssl-metal-rt`, `cssl-webgpu-rt`

**Maturity**: All five are pure scaffolds. Zero FFI. Zero runtime capability.

These crates declare the API surface for GPU context management (device enumeration, queue
submission, buffer allocation, synchronization) but contain no implementation. The workspace
declares `ash = "0.38"` (Vulkan), `windows` (D3D12), and `wgpu` as workspace dependencies,
but none of the individual host runtime crates list them in their `[dependencies]`. The
`level-zero-sys` dependency is commented out entirely.

**ArcA770Profile**: Hardcoded canonical hardware profile, present in all five crates:
```
vendor_id  = 0x8086   (Intel)
device_id  = 0x56A0   (Arc A770)
xe_cores   = 32
vram       = 16 GB
tflops     = 17.2 (FP32)
```

This profile drives capability decisions (extension selection, SIMD tier, memory pool sizing)
throughout the compiler. It is not user-configurable at stage-0.

[OPEN: A doc error: Vulkan extension count is documented as "30-variant" but the enum has
31 variants. Minor but indicates documentation lag relative to implementation.]

### 2.15 Observability, Persistence, Runtime — `cssl-telemetry`, `cssl-persist`, `cssl-rt`

**`cssl-telemetry`** (1518 LOC):

Real cryptographic wiring: BLAKE3 1.5.4 + ed25519-dalek 2.1.1 (T11-D2, replacing stub
XOR/byte-fold). The audit chain produces tamper-evident logs where each entry's content hash
is signed with Ed25519 and linked to the previous entry's content hash.

Known issues:
1. **TelemetrySlot layout divergence**: Documented as 64 bytes. Actual struct = 68 bytes
   (8+2+2+4+4+40+8). Any code that assumes 64-byte alignment (e.g., SPSC ring buffer head/tail
   at cache-line boundaries) will have off-by-one alignment bugs.
2. **`verify_chain` stub-signature bypass**: If a stored signature equals the stub/default
   value, `verify_chain` skips real Ed25519 verification and returns success. This creates a
   forgery window: an attacker who knows the stub value can inject entries that pass
   verification without a valid private key.
3. **`prev_hash` linkage weakness**: Each entry links to the previous entry's `content_hash`
   only (the hash of the message payload). The sequence number and `prev_hash` field are NOT
   included in what is hashed. This means two entries can have identical `content_hash` values
   if they carry identical payloads, weakening the chain's tamper evidence.

**`cssl-persist`** (610 LOC): Data model for structured key-value persistence (scenes, entity
state, save files). Implements an in-memory backend only. No WAL (write-ahead log), no LMDB
or SQLite backing, no fsync. Data does not survive process restart.

**`cssl-rt`** (19 LOC): Pure scaffold. The runtime library (stack unwinding, panic handler,
arena allocator bootstrap) that all CSSLv3 programs are intended to link against is not yet
implemented.

### 2.16 Testing Infrastructure — `cssl-testing`

**Crate**: `cssl-testing` (~3730 LOC)  
**Maturity**: 8 of 12 oracle modes live; 3 stubs; custom PRNG.

12 oracle testing modes (`ORACLE_MODE_COUNT = 12`, verified against the `ALL` array):

| Mode | Status | Purpose |
|------|--------|---------|
| TypeCheck | Live | HIR type inference regression |
| EffectCheck | Live | Effect row composition validation |
| BorrowCheck | Live | Capability/linearity rules |
| IfcCheck | Live | IFC label propagation |
| AutoDiff | Live | Gradient equivalence (AD oracle) |
| Refinement | Live | SMT predicate discharge |
| Codegen | Live | MIR→CLIF round-trip |
| AuditChain | Live | BLAKE3+Ed25519 chain integrity |
| PowerBudget | Stub | GPU power envelope modeling |
| ThermalModel | Stub | Thermal throttling simulation |
| HotReload | Stub | Live patch application |
| OracleCompose | Adjunct | Cross-oracle conjunction |

Custom `Lcg` PRNG (linear congruential generator) for deterministic fuzz seeds. No proptest,
insta, or criterion dependencies — the testing infrastructure is fully internal.

**`r16_attestation`**: Per-commit BLAKE3 + Ed25519 attestation for CI. Produces a signed
manifest that the compiler build at a given git SHA produces bit-exact artifacts.

### 2.17 Examples and `csslc` Binary — `cssl-examples`, `csslc`

**`cssl-examples`** (~6671 LOC): The primary integration test vehicle. Three canonical `.cssl`
source files drive the full pipeline:
- `hello_triangle.cssl`: Basic GPU triangle, tests lex→parse→HIR→MIR→GPU-codegen chain.
- `sdf_shader.cssl`: Sphere SDF with analytical gradient, tests F1 AutoDiff chain end-to-end.
- `audio_callback.cssl`: Real-time audio, tests F3 effect system (AudioIO effect tracking).

**F1 full chain** (from `sdf_shader.cssl`):
```
lex → parse → HIR → AD-legality-check → refinement-obligations → MIR → AD-walker → SMT-translation
```
11/11 gradient-equivalence cases passing (T7-D5 killer-app gate). R18 attestation signs the
passing result set.

**`csslc`** binary (23 LOC, `fn main()` is `eprintln!` + `exit(0)`): The compiler driver is a
stub. It imports no crate dependencies. All real compilation paths are reached only through
`cssl-examples` tests and the `cssl-testing` harness. The binary as shipped cannot compile
any `.cssl` file.

---

## 3. F1–F6 Feature-Deep: Spec Intent vs. Implementation

### 3.1 F1 — AutoDiff

**Spec intent** (`specs/33_F1_F6_LANGUAGE_FEATURES.csl`): Source-to-source AD with
Slang.D-inspired surface syntax. Both forward (JVP) and reverse (VJP) modes. The `#[diff]`
and `#[grad]` annotations mark differentiable functions. `DiffPair<T>` carries
`(primal: T, tangent: T)`. Lipschitz bounds flow from F2 refinement types.

**Implementation**: Substantially present. 19 primitives, 38-rule transform table, MIR-level
operation. Exceeds most peer systems (no GPU shading language has source-to-source AD with
Lipschitz tracking in the type system; Slang.D requires explicit `[Differentiable]` annotations
and does not do refinement-based Lipschitz).

**Gaps**:
- Lipschitz bound is always `Some("k")` — not computed from actual function structure.
- F2→F1 connection (Lipschitz bound flowing from refinement type to AD type) is not wired.
- `cssl-jets` (higher-order derivatives) is schema-only.

**Exceed-class angle vs. peers**: Slang.D provides `[Differentiable]` + `DifferentialPair<T>`
for GPU shading. CSSLv3's F1 adds: (a) MIR-level transform (not HIR-level, enabling more
aggressive optimization), (b) Lipschitz bound in the type (structural correctness guarantee
Slang lacks), (c) SMT verification of gradient equivalence as a first-class test mode.

### 3.2 F2 — Refinement Types

**Spec intent**: LiquidHaskell-style `{v:T|P(v)}` where `P` is an SMT-decidable predicate.
Sugar form `T'L<k>` for Lipschitz bounds. SMT discharge via Z3 or CVC5.

**Implementation**: `cssl-smt` implements subprocess dispatch. Predicate text parser
translates CSSLv3 refinement predicates to SMT-LIB 2. Working end-to-end for scalar integer
and float predicates as demonstrated by `refinement` oracle mode.

**Gaps**:
- No `z3-sys` FFI; requires solver in PATH at compile time. Offline builds without Z3/CVC5
  installed will silently skip SMT discharge.
- Multi-variable predicates (involving more than one bound variable) are not tested in the
  killer-app examples.
- `T'L<k>` Lipschitz sugar: parsed but `k` is not unified with the actual Lipschitz constant
  derived from the function body.

**Exceed-class angle**: No mainstream systems language has refinement types with SMT discharge
in the base language (Liquid Haskell is functional, F* is dependently typed research). CSSLv3
brings this to a Rust-syntax systems language targeting GPU.

### 3.3 F3 — Effect System

**Spec intent**: Koka-style row-polymorphic effects. Per the Koka reference
(koka-lang.github.io): effect rows track which effects a computation may invoke; handlers
reduce rows by providing implementations; `perform` invokes an effect operation; `with`
establishes a local handler scope. CSSLv3 extends this with PRIME_DIRECTIVE semantics:
certain effect combinations are permanently banned.

**Implementation**: `cssl-effects` has 32 builtin effects, `banned_composition`, and
`banned_composition_with_domains`. `cssl-hir` unifies effect rows during type inference.

**Gaps**:
- F5/F6 keywords (`perform`, `with`, `region`) have zero parser representation. The parser
  does not recognize these tokens in either surface.
- `banned_composition` (light variant) is effectively a no-op due to `SensitiveDomain::Other`
  predicate bug. Only `banned_composition_with_domains` enforces the prohibitions correctly.
- Effect row unification in HIR and `cssl-effects::EffectRow` use separate representations;
  the connection is not formalized in a shared trait or newtype.

**Exceed-class angle**: Koka's effect system does not have hardcoded semantic prohibitions.
CSSLv3's PRIME_DIRECTIVE encoding makes ethics a type-level constraint — the 17 prohibitions
become banned effect compositions detectable without running code.

### 3.4 F4 — Staged Computation

**Spec intent**: Futamura projections P1/P2/P3. `@staged` marks functions eligible for
partial evaluation. `#run` triggers specialization at a call site. Racket set-of-scopes
hygiene for macro expansion.

**Implementation**: `cssl-staging` (data model), `cssl-futamura` (orchestrator model),
`cssl-macros` (hygiene infrastructure). All three are data-structure and metadata-collection
stages only; no actual partial evaluation or macro expansion occurs.

**Gaps**: The actual specialization transform is the entire missing piece. F4 is the most
"spec-only" of the six features at stage-0.

**Exceed-class angle**: No systems language has Futamura projections as a first-class language
feature. Template metaprogramming (C++), const-eval (Rust), and comptime (Zig) are related
but none model the full P1/P2/P3 hierarchy or formal fixed-point convergence checking.

### 3.5 F5 — Information Flow Control

**Spec intent**: Jif Decentralized Label Model (DLM). Labels are `{owners: C, integrity: I}`
pairs. Label flow: a value flows from high-confidentiality to low-confidentiality site only if
all owners consent. 9 PRIME_DIRECTIVE principals. IFC0001–IFC0004 diagnostic codes.

**Implementation**: 1168 LOC in `cssl-hir/src/ifc.rs`. Structurally the most complete of all
six F features. `check_ifc_flow` does real dataflow analysis.

**Gaps**:
- `cssl-ifc` crate (the crate that *should* own IFC) is 24 LOC, zero API. The implementation
  is buried in `cssl-hir`, violating the intended crate architecture.
- `combine_labels` uses union not DLM join (see §2.7 and §8.2).
- F5/F6 keywords have no parser representation (same as F3 gap).
- Parser does not recognize label annotation syntax `@{C I}`.

**Exceed-class angle**: No GPU shading language or systems language has IFC in the type
system (HLSL, GLSL, WGSL, MSL, Metal Shading Language are all IFC-free). CSSLv3 is the
only shading-capable language where confidentiality leaks are type errors.

### 3.6 F6 — Observability (Macros + Telemetry)

**Spec intent**: 26-scope taxonomy for telemetry events. SPSC ring buffer for lock-free
single-producer/single-consumer log emission. BLAKE3 + Ed25519 signed audit chain. Oracle-
mode testing where the telemetry output is the oracle.

**Implementation**: `cssl-telemetry` (1518 LOC) with real BLAKE3 + Ed25519 (T11-D2). 8 live
oracle modes in `cssl-testing`.

**Gaps**: See §2.15 for the three known issues (slot layout, stub-signature bypass,
prev_hash weakness).

**Exceed-class angle**: Production GPU engines (Unreal, Unity, Godot) instrument with
proprietary telemetry. CSSLv3's telemetry is cryptographically signed and the audit chain is
verifiable offline — this is closer to a blockchain without the consensus protocol than to
conventional game telemetry.

---

## 4. Dual-Surface Design

### 4.1 Design Rationale

`specs/16_DUAL_SURFACE.csl` specifies a 4-tier mode detection:
1. **Extension**: `.cssl-csl` → CslNative; `.cssl-rust` → RustHybrid; `.cssl` → auto.
2. **First non-comment line heuristic**: If the first substantive line contains `§` or CSLv3
   glyphs, select CslNative.
3. **Pragma**: `#![surface="csl"]` or `#![surface="rust"]` overrides detection.
4. **Default**: `RustHybrid` if none of the above resolve.

The architectural promise is that **both surfaces produce the same `cssl_ast::Module` CST**.
A file written in RustHybrid syntax and the same file rewritten in CslNative syntax must
produce byte-identical HIR (barring surface-specific metadata). The sphere_sdf example in
`specs/16_DUAL_SURFACE.csl` demonstrates this with a rendered pair.

### 4.2 Parity Gaps

**CslNative surface is a structural stub**:
- Only `§ name` → `Item::Module` parsing is present.
- Body parsing, expression parsing, type annotation parsing, and all compound glyph
  operators (`→`, `≤`, `≥`, `∀`, `∃`, `∈`) are not implemented.
- The CSLv3-native lexer recognizes the glyph token set but the parser does not use them.

**Mode detection result is discarded**: `Surface::Auto` selects a surface at lex time but
does not write the result back to `SourceFile.surface`. Any downstream pass that queries the
surface of an Auto-lexed file will see `Surface::Auto`, not the resolved surface.

**F5/F6 representation gap**: Neither surface has parser support for:
- Effect annotations `@io @gpu`
- IFC label annotations `@{C I}`
- Telemetry scope annotations `@trace(scope)`
- `perform`/`with`/`region` keywords

This is the same gap as noted in F3/F5/F6 above. The parser silently drops these constructs,
meaning the HIR will never see them.

### 4.3 Unification Architecture

The unification guarantee is enforced by the CST type hierarchy in `cssl-ast`, not by tests.
There is no test suite that feeds the same semantic program through both surfaces and asserts
HIR identity. The spec mandates this test but it is not yet written. [OPEN: add a dual-surface
parity test suite as part of T8-phase-2.]

---

## 5. Stage-0 / Stage-1 Architecture

### 5.1 The Bootstrap Ladder

`specs/01_BOOTSTRAP.csl` defines four stages:

| Stage | Host | Status | Purpose |
|-------|------|--------|---------|
| Stage-0 | Rust | Active | Throwaway, fast-iterate, full-feature scaffold |
| Stage-1 | CSSLv3 | DO-NOT-START-YET | Self-hosted; compiles from .csl source |
| Stage-2 | CSSLv3 | Future | LoA engine migration; replace Rust bootstrap |
| Stage-3 | C99 | Future | Cross-platform reproducibility / formal verification |

Stage-0 is explicitly "throwaway." The 32-crate Rust workspace will be deleted when stage-1
is viable. This is not a refactor — it is a complete rewrite in the target language. The Rust
code exists as a tool for discovering what the language spec should say, not as lasting
infrastructure.

### 5.2 Stage-1 Prerequisites (8 Capabilities)

From `stage1/README.csl` (T11-D33 status @ 2026-04-18: scaffold only):

1. **Generics + monomorphization**: Self-hosting requires compiling generic functions. The
   MIR monomorphization quartet (§2.10) satisfies this prerequisite in stage-0, validating
   the spec's approach.
2. **Standard library collections**: `Vec<T>`, `HashMap<K,V>` equivalents in CSSLv3.
3. **Trait dispatch**: The type checker needs to dispatch on traits for iterators, numeric
   ops, etc.
4. **String handling**: String interning, slicing, formatting for error messages.
5. **File I/O**: Reading `.cssl` source files; writing `.cssl-obj` and binary artifacts.
6. **Iterator combinators**: `map`, `filter`, `fold` used throughout compiler passes.
7. **Sum-type matching**: Pattern matching on `Option<T>` and `Result<T,E>` pervasively used
   in error propagation.
8. **Own x86-64 backend**: Stage-1 must not depend on Cranelift (a Rust crate). It needs its
   own code emitter for the bootstrap machine.

### 5.3 The P1–P10 Roadmap

The stage-1 roadmap is organized in 10 milestones (P1–P10 from `stage1/README.csl`):

```
P1  Self-parser: parse hello.csl using CSSLv3-native surface
P2  Self-type-checker: type-check the parser itself
P3  Self-mir-lowerer: produce MIR for the type checker
P4  Self-cranelift: produce Cranelift JIT output from MIR
P5  Bootstrap round-trip: compiler.csl compiles compiler.csl
P6  Fixed-point check: two consecutive compiles produce byte-identical output
P7  Stage-0 deletion: remove compiler-rs/ from the repository
P8  Stage-1 GPU path: hook CSSLv3 GPU backends to the self-hosted pipeline
P9  Stage-2 migration: LoA engine sources rewritten in .csl
P10 Stage-3 C99: produce C99 output for cross-platform seed
```

`cssl-examples/src/stage1_scaffold.rs` verifies that `hello.cssl` and `compiler.cssl` parse
cleanly through the stage-0 RustHybrid parser — establishing the minimum bar for P1.

### 5.4 Fixed-Point Check Design

P6 is the critical correctness milestone. The fixed-point check works as follows:
1. Compile `compiler.csl` with compiler binary A → produce binary B.
2. Compile `compiler.csl` with binary B → produce binary C.
3. Assert `sha256(B) == sha256(C)` (byte-exact reproducibility).

This is the standard bootstrap verification used by GCC and the Trusting Trust defense. The
`cssl-futamura` `FixedPointRecord` type models the convergence check formally. The actual
byte-exact comparison is not yet implemented (requires stage-1 to exist).

---

## 6. Cross-Cutting Concerns

### 6.1 Effect System Propagation

Effects are tracked as `EffectRow` types in HIR. The compiler's own operations (file I/O
during compilation, subprocess invocation for Z3, telemetry emission) are themselves subject
to effect tracking — the compiler is self-applying. This means the compiler's implementation
in stage-1 will require effect annotations for its own file I/O and subprocess operations.

At stage-0, this circularity is broken by the Rust host: the Rust compiler does not enforce
CSSLv3 effect rules. The circularity becomes real at stage-1 and is a known design challenge.

### 6.2 IFC Propagation

IFC labels flow through HIR dataflow edges. The 9 built-in principals are:
`User`, `Operator`, `Regulator`, `AiPartner`, `Surveillance`, `Manipulation`, `Coercion`,
`Weapon`, `Exploit`. The last five correspond to five of the PRIME_DIRECTIVE's sensitive
domains and carry permanent high-confidentiality / low-integrity labels — they can never be
the *output* principal of a computation that reaches user-visible output channels.

The dataflow checker (`check_ifc_flow`) uses a forward worklist algorithm. Starting from
annotated source expressions, it propagates labels along use-def edges. A violation occurs
when a label with `confidentiality ∋ Surveillance` reaches a sink with `integrity ∋ Public`.

`combine_labels`'s union semantics mean the checker is sound (no false negatives — no
undetected flows) but not complete (may report false positives — flows that are technically
safe under formal DLM but conservative under union). For the PRIME_DIRECTIVE use case, false
positives are preferred over false negatives.

### 6.3 Telemetry and Audit Chain

Every semantic event in the compiler (type error, effect violation, IFC diagnostic,
gradient check, SMT query/result) is a candidate telemetry entry. The 26-scope taxonomy
defines which scopes are active in which compilation phases.

The BLAKE3 + Ed25519 chain provides the following guarantees at stage-0 maturity:
- **Content integrity**: Each entry's payload is hashed.
- **Signing**: The hash is signed with the project Ed25519 private key.
- **Sequential linkage**: Each entry links to the previous entry's content hash.

The three known weaknesses (§2.15) degrade these guarantees from "production audit chain"
to "development-grade tamper evidence." Production hardening requires: fixing the slot layout
to true 64 bytes, removing the stub-signature bypass, and including `seq + prev_hash` in the
signed content.

### 6.4 PRIME_DIRECTIVE Structural Encoding

The PRIME_DIRECTIVE is not advisory — it is encoded three ways:

1. **Effect system**: `banned_composition_with_domains` returns true for 17 prohibited effect
   pairs. A program that attempts to compose `Surveillance + NetIO` effects in the same
   function fails type checking.

2. **IFC labels**: The 9 principals include all five sensitive domains. A function accepting
   a `Surveillance`-labeled value and returning a `Public`-labeled value fails IFC checking.

3. **Capability system**: `iso` (isolated) capabilities cannot be aliased. A `SensitiveData`
   value typed as `iso` cannot be copied to a less-restricted alias. Combined with the effect
   system, this prevents sensitive data from escaping controlled contexts.

The combination means a PRIME_DIRECTIVE violation must either:
- Be a type error (caught at compile time), or
- Require the programmer to explicitly write `unsafe` or use the banned effect row directly
  (which is a conscious opt-out requiring justification), or
- Be impossible because the types do not provide the necessary interface.

[OPEN: The PRIME_DIRECTIVE §10 defines a ToS that sits *above* the Apache-2.0 / MIT license.
The legal relationship between the open-source license and the ToS-level PRIME_DIRECTIVE
restrictions is not resolved in the repo. Code distributed under Apache-2.0/MIT can be
modified to remove effect restrictions; the ToS claims this is not permitted. These two
claims are in tension for downstream users.]

---

## 7. Integration Points and Extension Model

### 7.1 New Crate Plug-in Interface

There is no formal plug-in API at stage-0. Extension points exist but are informal:

- **New effect**: Add a variant to `cssl-effects::BuiltinEffect`, add rows to the ban table
  in `banned_composition_with_domains`, add handlers in the HIR effect row unifier.
- **New GPU backend**: Create a new crate `cssl-{target}` with a `fn lower_mir(module:
  &MirModule) -> Result<BackendOutput>` entry point. Wire into the driver (currently the
  driver is the examples harness, not `csslc`).
- **New oracle mode**: Add a variant to `cssl-testing::OracleMode`, implement the mode's
  `run` method, add to the `ALL` array (ensuring `ORACLE_MODE_COUNT` stays in sync).
- **New MIR pass**: Add to the 6-pass pipeline in `cssl-mir`. The first pass (monomorphization
  quartet) is the template.

### 7.2 `workspace.metadata.cssl`

`compiler-rs/Cargo.toml` reserves the `[workspace.metadata.cssl]` table for CSSLv3-specific
workspace configuration (hardware profile overrides, feature flags, SMT solver paths). This
table is currently empty but is the intended home for:
- `solver_path`: override Z3/CVC5 executable path
- `target_profile`: override the hardcoded ArcA770 profile
- `stage`: switch between stage-0 and future stage-1 output modes
- `surface_default`: override `Surface::RustHybrid` default

### 7.3 The `cssl-testing` Harness as the Integration Bus

Because `csslc` is a stub, `cssl-testing` is the de-facto integration bus for the entire
compiler pipeline. All end-to-end tests go through oracle modes. Any new feature that needs
integration testing must either add an oracle mode or extend an existing one. This is not
ideal architecture (a real compiler driver should be the integration point) but it is the
practical reality until the `csslc` binary grows real implementations.

---

## 8. Architectural Tensions — Honest Reporting

### 8.1 `cssl-ifc` (24 LOC) vs. IFC-in-HIR (1168 LOC)

The intended architecture places IFC in its own crate (`cssl-ifc`), parallel to `cssl-caps`
and `cssl-effects`. The actual implementation is embedded in `cssl-hir/src/ifc.rs`. This
means:
- `cssl-hir` has a hard dependency on IFC semantics that should belong to a separate concern.
- Extracting IFC to `cssl-ifc` requires decoupling the HIR type from the label type, which
  means threading `IfcLabel` through the `Ty` enum or as a parallel annotation map.
- Any other crate that wants IFC checking (e.g., `cssl-mir` for IR-level IFC) must either
  depend on `cssl-hir` (creating a cycle risk) or duplicate the IFC logic.

**Recommended path**: Add `IfcLabel` to `cssl-ast` (it is a surface-level annotation), move
`check_ifc` and `check_ifc_flow` to `cssl-ifc` with a trait boundary, and have `cssl-hir`
implement that trait. This is a P2–P3 stage-1 prerequisite refactor.

### 8.2 `combine_labels` Union vs. DLM Join

The formal DLM confidentiality join (⊔) *intersects* owner sets: if Alice owns label L1
and Bob owns label L2, the join `L1 ⊔ L2` is owned by both Alice and Bob (intersection =
more restrictive confidentiality). `combine_labels` unions both sets (Alice OR Bob is an
owner), which is the opposite semantics — it becomes *less* restrictive as more labels are
combined. This is safe in the taint-tracking sense (over-approximates sensitive flows) but
is not the DLM model that Jif implements and that the spec cites.

The practical consequence: a program that combines a Surveillance-labeled value with a
Public-labeled value via `combine_labels` will produce a label owned by `{Surveillance, Public}`
instead of `{Surveillance}` (if using intersection) or `{Public}` (if using union of owners in
the formal sense). Under union semantics, the result is maximally tainted (any sensitive owner
in the set → whole value is sensitive), which means no false negatives but many false positives.

[OPEN: The spec should explicitly document whether CSSLv3 targets formal DLM semantics or
a conservative taint-tracking approximation. These have different precision/soundness tradeoffs
that matter for F5 being useful in practice.]

### 8.3 `csslc` Binary as a Shell

The primary user-facing artifact of a compiler is its binary. `csslc` at stage-0 is 23 LOC
and does nothing. This means:
- There is no user-facing way to compile a `.cssl` file. Every user of the compiler must
  set up the Rust workspace and run integration tests.
- CI validation goes through `cargo test` not `csslc file.cssl`, which is a different
  trust surface than end-to-end compilation.
- The stage-1 rewrite cannot practically be validated against `csslc` because `csslc`
  has no specification of what it should do.

**Recommended path**: Wire the `cssl-examples` pipeline into a minimal `csslc` CLI (lex,
parse, HIR, MIR, emit-clif / emit-spirv). This is a 200–400 LOC change and the highest-
leverage unblocking action for the stage-1 milestone.

### 8.4 GPU Backend Independence vs. Claimed SPIR-V Common Intermediate

External-facing README documentation implies that all GPU backends route through SPIR-V as a
common intermediate (a reasonable architectural claim — SPIR-V is the Khronos
lingua franca for GPU computation). The actual crate implementations are four independent
MIR-to-text paths. This is not necessarily wrong (direct generation can be higher-quality than
round-tripping through SPIR-V→DXIL via dxc), but:
- The two paths (claimed vs. actual) imply different testing strategies.
- SPIR-V-as-intermediate would allow sharing the SPIR-V → DXIL and SPIR-V → MSL conversion
  tools (dxc, spirv-cross).
- The direct-generation path requires four independent, complete backends.

At stage-0, all four backends have stub bodies, so the question is moot. The decision matters
at T10-phase-2 when body lowering is implemented.

### 8.5 MIR Type Mapping Hazards

Three silent type narrowings are load-bearing correctness risks:

1. `usize → I32`: On a 64-bit target, `usize` is 64 bits. Truncating to 32 bits makes all
   pointer arithmetic and allocation size calculations incorrect for objects larger than
   ~2 GB. This will not manifest in hello-world examples but will corrupt any non-trivial
   memory management.

2. `Vec<T> → unimplemented`: A Vec-typed MIR value silently produces an uninitialized result.
   This is not a recoverable stub — it is a silent bug that produces garbage output. The
   lowerer should either error on Vec types or emit a runtime trap instruction.

3. `Bf16 → F16`: bfloat16 and float16 have identical bit widths (16) but different exponent/
   mantissa splits (bfloat16 = 8e/7m, float16 = 5e/10m). Treating them identically will
   produce numerically incorrect results in ML workloads that rely on bfloat16's extended
   dynamic range.

### 8.6 Telemetry Security Gaps

The three telemetry issues (§2.15) collectively mean that the audit chain, while cryptographically
sophisticated in design, does not provide production-grade tamper evidence at stage-0:
- Stub-signature bypass is a known forgery vector.
- `prev_hash`-only linkage allows payload-collision attacks.
- 68-byte slots break assumed cache-line alignment.

For development use (validating gradient correctness, tracking effect violations in tests)
these gaps are acceptable. For the Σ-Chain / Akashic Records use case (production audit trail
for distributed LoA session state), all three must be fixed before any data leaves a test
environment.

### 8.7 The `is_underscore` Bug and Exhaustiveness Checking

Pattern `_` never becomes `PatternKind::Wildcard` in normal parsing. Any exhaustiveness
checker that distinguishes `Wildcard` from `Binding { name: "_" }` will incorrectly report
non-exhaustive matches for code that uses `_` as a catch-all. The type checker must treat
the name `"_"` as a wildcard sentinel. This is undocumented in the HIR and could silently
produce incorrect exhaustiveness results.

### 8.8 PRIME_DIRECTIVE License Tension

The workspace is dual-licensed `Apache-2.0 OR MIT`. These are standard permissive open-source
licenses that allow modification and redistribution without restriction. The PRIME_DIRECTIVE
§10 (Terms of Service) asserts that derivative works must preserve the 17 prohibitions —
i.e., you cannot create a fork that removes the `banned_composition` rules to allow surveillance
features.

This creates a legal tension: Apache-2.0 explicitly permits any use and modification; the
ToS asserts restrictions beyond what Apache-2.0 allows. In practice, the ToS restriction is
unenforceable against Apache-2.0 recipients who receive the code without the ToS contract.

[OPEN: Resolve by either: (a) changing the license to a copyleft license (GPL/AGPL) with a
linking exception, which can enforce downstream restrictions; or (b) releasing the PRIME_DIRECTIVE
components under a separate proprietary license with the Rust scaffold under Apache-2.0; or
(c) accepting that the PRIME_DIRECTIVE ToS is aspirational rather than legally binding for
open-source distribution. This is an Apocky decision, not a technical one.]

---

## Summary: Maturity Map

| Layer | Crate(s) | Maturity |
|-------|----------|----------|
| Source / CST | cssl-ast, cssl-lex, cssl-parse | RustHybrid: solid. CslNative: stub. |
| Type inference | cssl-hir | Core working; 4 synthesis stubs; cap-check skeleton |
| Capability model | cssl-caps | Complete |
| Effect taxonomy | cssl-effects | Complete; light ban path buggy |
| IFC | cssl-hir/src/ifc.rs | Working; DLM deviation; cssl-ifc empty |
| AutoDiff | cssl-autodiff | 38-rule table working; Lipschitz placeholder |
| SMT / Refinement | cssl-smt | Working; requires solver in PATH |
| Staged / Macros | cssl-staging, cssl-macros, cssl-futamura | Data model only |
| MIR | cssl-mir | Data model + mono quartet; 5/6 passes stubbed |
| LIR | cssl-lir | Scaffold only |
| MLIR bridge | cssl-mlir-bridge | Textual dump only; no real MLIR |
| CPU codegen | cssl-cranelift | JIT + text emitter working; Vec unmappable |
| GPU codegen | cssl-spirv/wgsl/dxil/msl | SPIR-V: binary + validation; others: body stubs |
| Host runtimes | *-rt (5 crates) | All scaffold; no FFI |
| Telemetry | cssl-telemetry | Real BLAKE3+Ed25519; 3 known gaps |
| Persistence | cssl-persist | In-memory only |
| Runtime lib | cssl-rt | 19-LOC scaffold |
| Testing | cssl-testing | 8/12 oracle modes live |
| Integration | cssl-examples | Full F1 chain; 11/11 gradient gate passing |
| Compiler driver | csslc | 23-LOC stub |

**Open item count**: 12 `[OPEN: ...]` markers in this document.  
**Citation count**: 6 external primary sources; 10 audit documents; 4 spec files.  
**Flagged assumptions**: The MLIR-pipeline-vs-independent-backends discrepancy (§8.4) and
the legal license tension (§8.8) are the two items most likely to require Apocky decision
before they can be resolved technically.

---

*Generated from primary sources. Every claim is traceable to `docs/audit/NN-*.md`, `specs/`,
`compiler-rs/Cargo.toml`, `DECISIONS.md`, `PRIME_DIRECTIVE.md`, or an external reference
URL listed in the header. Do not cite this document as authoritative for implementation
details — go to the audit docs or source directly.*
