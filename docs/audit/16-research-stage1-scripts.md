# Audit: research/ · stage1/ · scripts/

**Audited:** 2026-05-14  
**Slice:** `research/` (14 files), `stage1/` (3 files), `scripts/` (3 files) — 20 files total  
**Auditor note:** CSLv3 symbolic notation decoded to plain English throughout.

---

## 1. SLICE OVERVIEW

These three directories cover three distinct roles in the CSSLv3 compiler project.

**research/** contains the pre-design literature survey that fed into the formal specifications in `specs/`. Each `Sx_*.csl` file surveys a domain of prior art (novel languages, autodiff systems, effect systems, codegen backends, etc.) and extracts lessons and adoption decisions for CSSLv3. The files use the CSLv3 dense notation as an internal thinking language. The synthesis document (`99_SYNTHESIS.csl`) cross-references all research into a unified design-space map. One additional file (`compass_artifact_*.md`) is an external AI-generated design essay rather than internal research notes; it is the most prose-dense and technically expansive document in the slice.

**stage1/** is the self-hosting endgame scaffold. The directory represents the goal of the entire project: a CSSLv3 compiler written in CSSLv3 itself (stage1), compiled by the Rust-hosted stage0 compiler. As of the timestamp in `README.csl` (2026-04-18, session T11-D33), this is scaffolding only — two placeholder `.cssl` source files that verify the stage0 parser accepts valid CSSLv3 surface syntax. `README.csl` contains the full P1–P10 self-hosting roadmap.

**scripts/** contains three Python utility scripts that support development and CI. One is a JSON/JSONL identity-claim redaction tool for transcript hygiene, one is a cross-reference validator for the spec corpus, and one is a differential lexer oracle skeleton for comparing the Rust-port lexer against the Odin reference parser. All three are standalone scripts with no common library.

**Relationship to rest of repo:** `research/` → informed `specs/` (all ten `specs/0N_*.csl` files cite `§§ Sx` research entries); `specs/` → drives `compiler-rs/crates/` (32-crate Cargo workspace); `stage1/` → is what `compiler-rs/` must eventually be able to compile; `scripts/` → is CI and maintenance tooling invoked from `.github/workflows/` and manually.

---

## 2. RESEARCH SECTION

### 2.1 `research/00_MANIFEST.csl` — 86 lines — Research roadmap and constraints

The manifest is the starting point for the entire research phase. It declares the hardware constraints for CSSLv3: primary target is Intel 12th–14th generation CPUs and Intel Arc A770 GPU, using x86-64 with AVX2 and Vulkan 1.3 / SPIR-V. AVX-512 is explicitly excluded. The document then names the four non-negotiable language features as F1 (autodiff), F2 (refinement types), F3 (effect system), and F4 (staged computation), with concrete LoA-engine bug scenarios that each feature was designed to fix (for example, the central-differencing performance cliff at 10fps motivated F1 autodiff; the thin-SDF raymarcher crash motivated F2 refinement types).

The "bug thesis" is stated here: every known LoA engine bug was caused by something the language allowed that it should have prevented at compile time.

The manifest enumerates the ten research sections S1–S10 and their corresponding output spec files (`specs/00_MANIFESTO.csl` through `specs/10_HW.csl`). It ends with a status log showing research was in progress (S1–S7 partial, S8–S10 pending, synthesis not yet written) at the time of writing. **Influenced:** all ten `specs/` files and the overall project structure.

---

### 2.2 `research/99_SYNTHESIS.csl` — 144 lines — Cross-research design synthesis

The synthesis document is the capstone of the research phase, mapping every prior-art finding from S1–S10 to CSSLv3 design decisions. It is organized as eleven numbered sections.

**Core thesis (§ 99.1):** CSSLv3 is described as the union of Slang.D (shader-class autodiff), Koka (row-polymorphic effects), Vale (hybrid linear/generational memory), and MLIR (codegen substrate), with CSLv3 notation as the thinking language. It is a single language targeting both CPU and GPU simultaneously for the LoA engine.

**Prior-art mapping for F1–F4 (§ 99.2):** F1 autodiff surface adopts Slang.D's `[Differentiable]` attribute pattern; the implementation path uses the Enzyme LLVM-IR pass. F2 refinement adopts LiquidHaskell's `{v:T | P(v)}` syntax but replaces SMT-backed full verification with a lighter decidable refinement approach, adding native Lipschitz-bound tracking via numeric tags on SDF functions. F3 effects adopt Koka's row-polymorphic type system with LoA-specific effects (NoAlloc, NoRecurse, Deadline, DetRNG, GPU, CPU, XMX, RT). F4 staging adopts the Futamura P1+P2 projection model expressed through an `{Comptime}` effect and `@staged` attribute rather than a separate meta-language.

**Memory model (§ 99.3):** A hybrid of Vale's linear aliasing (owned `^T` plus generational `&T` references) with Futhark uniqueness types for GPU buffers and regions as an effect-row entry (`{Region<'r>}`). Entity handles are `Handle<T>` — a packed u64 of 24-bit generation plus 40-bit index.

**Codegen (§ 99.4):** MLIR core with custom `cssl.sdf`, `cssl.effect`, `cssl.staged`, and `cssl.engine` dialects, targeting IREE's fat-binary pattern (x86-64 ELF/PE object + SPIR-V blob). Enzyme-LLVM pass handles autodiff in the CPU lowering path. Cranelift is mentioned as an optional fast-compile dev-build backend.

**Engine primitives (§ 99.5):** Built-in compiler types (not stdlib) include `SDF'L<bound>`, `Scene`, `Archetype`, `Handle<T>`, `Grid<T,dim>`, `CmdBuf`, `Texture-Handle`, and `Fiber`. These are first-class to the compiler, enabling engine-semantic errors to be caught at compile time.

**Syntax (§ 99.6):** CSSLv3 surface resembles Rust/Swift/Slang for the program text, with CSLv3 notation living in block comments. A formatter auto-upgrades ASCII aliases to Unicode on save (Uiua pattern).

**Bug thesis coverage (§ 99.7):** Lists nine known LoA bugs and maps each to the CSSLv3 feature that prevents it, all marked as resolved.

**Bootstrap strategy (§ 99.8):** Three stages: stage0 in Rust (throwaway), stage1 self-hosted after phase 9, stage2 rewrites the LoA engine in CSSLv3 abandoning Odin.

**Consent-as-OS (§ 99.9):** Mandates that the type system itself encodes the project's ethics: the `{SurveillanceCapability}` effect requires opt-in and human review, `{CoercionCapability}` is banned at the compiler level, `{DataExfiltration}` requires a refinement on network operations, and the compiler refuses to emit weapon-targeting code.

**Open questions (§ 99.10):** Seven open questions for the design phase, including effect discharge granularity, region-lifetime syntax, opt-in vs. default differentiability, generics model, stdlib policy, IR substrate choice, and hot-reload preservation. Most are resolved directionally (lean toward X).

**Spec write order (§ 99.11):** The ordered list of eleven spec files to write, from `00_MANIFESTO.csl` through `10_HW.csl`.

**Influenced:** Every `specs/` file and the architecture of `compiler-rs/`.

---

### 2.3 `research/Q6_IR_architecture.csl` — 176 lines — Deep-dive: IR substrate decision

This is the most analytically rigorous document in the research corpus. It evaluates three possible IR architectures against twelve weighted dimensions and arrives at a binding decision.

**The three options:**
- Option A: LLVM-only (CSSLv3-AST → custom HIR → LLVM-IR → object)
- Option B: MLIR core + custom `cssl.*` dialects + LLVM/SPIR-V backends
- Option C: Custom CSSLv3-IR with its own LLVM and SPIR-V backends

**Twelve assessment dimensions (§ Q6.2):** time to first pixel, dual CPU+GPU codegen cost, F1 autodiff integration, F2 refinement integration, F3 effect integration, F4 staging integration, debuggability, compile speed, ecosystem reuse, bus factor for a solo developer, future hardware portability, and learning curve.

**Verdict and decision (§ Q6.8):** A hybrid A→C evolution strategy:
- Stage 0–3 (year 1): Option A (LLVM-only, custom HIR) — fastest to first pixel, proven Rust→LLVM tooling, Enzyme as a native LLVM pass, F4 staging at AST-level only.
- Stage 4–6 (year 2): Introduce a custom MIR layer between HIR and LLVM-IR that carries effect-rows and refinement types first-class; write a custom SPIR-V emitter from MIR to replace the LLVM-SPIRV translator.
- Stage 7+ (year 3+): Optional MLIR bridge if community and ecosystem justify it.

**Rationale:** Pure MLIR was rejected primarily due to its steep learning curve and high bus-factor risk for a solo developer. Pure custom IR was rejected because it would throw away the free Enzyme AD pass and the LLVM SPIRV translator.

**Secondary decisions resolved (§ Q6.11):** Region lifetimes use `{Region<'r>}` in effect rows; AD is opt-in via `[Differentiable]` (Slang-style); generics use interfaces with associated types; `{NoAlloc}`, `{NoRecurse}`, `{DetRNG}`, `{GPU}`, `{CPU}` effects are compile-time checked; `{Deadline<N>}` adds a runtime assert.

**Influenced:** `specs/02_IR.csl` directly, and the architecture of every codegen crate in `compiler-rs/`.

---

### 2.4 `research/S1_novel_langs.csl` — 115 lines — Survey: novel game and graphics languages

Surveys nine languages for design lessons: Slang, Jai, Zig, Vale, Mojo, Verse, Odin, Taichi, Hylo/Val/Circle/Carbon.

**Slang (§ S1.1):** Rated highest relevance. Its `[Differentiable]` attribute, `fwd_diff`/`bwd_diff` operators, `IDifferentiable` interface, and `DifferentialPair<T>` built-in type prove that first-class autodiff in a GPU shader language is feasible. Slang's capability system is identified as a seed for CSSLv3's effect system F3.

**Jai (§ S1.2):** Beta-only at time of writing. `#run` arbitrary-code-at-compile-time is worth adopting. CSSLv3 is expected to surpass Jai because Jai does not tackle autodiff, effects, or refinement types.

**Zig (§ S1.3):** Key negative lesson: `comptime` as an AST interpreter is 20× slower than Python and crash-prone. CSSLv3 must compile comptime code through the same backend as runtime code, not interpret it. Types-as-first-class values at comptime are a good pattern.

**Vale (§ S1.4):** Development on hold as of December 2025. Generational references (8-byte pointer + object-generation counter), region-borrow checking, and linear aliasing model are the primary adoptions.

**Mojo (§ S1.5):** Architecturally the closest prior art. Proves that MLIR core + custom dialects is a viable production approach, and that vendor-agnostic GPU targeting via compile-time specialization works. Its Python-compatibility baggage is explicitly not adopted.

**Verse (§ S1.6):** Epic's functional-logic language co-designed with Unreal Engine 6. The declarative scene-description pattern and the layered variant design (CoreVerse/ShipVerse/MaxVerse) are noted as patterns, but the logic paradigm itself is rejected as too exotic for LoA.

**Odin (§ S1.7):** The current LoA v10 implementation language. Noted as pragmatic and stable but insufficient — it lacks autodiff, effects, and refinement types. Its replacement by CSSLv3 is confirmed.

**Taichi and Hylo/Val/Circle/Carbon (§§ S1.8–S1.9):** Marked as pending deep-dive; no findings recorded.

**Influenced:** `specs/00_MANIFESTO.csl`, `specs/05_AUTODIFF.csl`, `specs/03_TYPES.csl`.

---

### 2.5 `research/S2_metaprogramming.csl` — 72 lines — Survey: staged computation and compile-time metaprogramming

Covers Futamura projections, Zig comptime lessons, MetaOCaml, Terra, Truffle/GraalVM, and the scene-evaluator application.

**Futamura projections (§ S2.1):** The theoretical basis for F4. CSSLv3 targets P1 (spec specializes a program = compile) and P2 (spec specializes a specializer = compile a compiler) at compile time, not runtime. The surface syntax hides this complexity behind `@staged` and `@specialize` attributes.

**Zig comptime lessons (§ S2.2):** Four negative lessons: (1) compile-time code must be compiled, not interpreted; (2) the comptime stack must be separate from the compiler's own stack; (3) host architecture must not leak into comptime (cross-compile safety); (4) types-as-values at comptime is a valid and useful pattern.

**MetaOCaml and Terra (§ S2.3):** MetaOCaml's modal type system distinguishes stages via `<α>` future-stage types and explicit quote/splice/run operators. Terra uses Lua as a compile-time meta-language driving LLVM codegen for a runtime object language. CSSLv3 tentatively adopts Option C from these: a single language where staging is expressed as a `{Comptime}` effect, consistent with F3.

**Scene evaluator generation (§ S2.5):** The concrete LoA application of Futamura P1. A scene has a fixed structure known at level-load time; `spec(eval, scene)` produces a scene-specialized shader compiled once per scene, eliminating runtime type dispatch in hot loops.

**Influenced:** `specs/06_STAGING.csl`, `specs/04_EFFECTS.csl`.

---

### 2.6 `research/S3_autodiff.csl` — 74 lines — Survey: automatic differentiation in systems languages

**Enzyme (§ S3.1):** The primary implementation choice. An LLVM-IR plugin, language-agnostic, works on optimized IR, supports both forward and reverse modes, handles GPU kernels including CUDA shared memory and sync intrinsics. The Rust autodiff project goal for 2024h2 exists. CSSLv3 will lower to LLVM-IR and apply Enzyme as a pass plugin.

**Slang.D (§ S3.2):** The primary surface-syntax design source. The `IDifferentiable` interface marks differentiable types, `DifferentialPair<T>` is built in, `[Differentiable]` marks functions, `fwd_diff`/`bwd_diff` are higher-order operators. Handles arbitrary control flow, dynamic dispatch, and generics. User-defined derivatives via `[ForwardDerivative(fn)]` and `[BackwardDerivative(fn)]`. The lesson: type-system-level AD is correct by construction. CSSLv3 adopts this design nearly verbatim as the surface, with Enzyme driving codegen.

**DiffTaichi (§ S3.3):** Source-transform AD with megakernel fusion. 188× faster than TensorFlow. Key lesson: simulation differentiability requires preserving all history states (not double-buffering), which means per-step memory unrolling. CSSLv3's F4 staging will handle this via compile-time unrolling.

**JAX (§ S3.4):** Python tracing-based AD — explicitly rejected because CSSLv3 is a static language where source transform is natural.

**SDF normal computation (§ S3.5):** The most concrete motivating use case. The naive central-differencing approach requires 6 SDF evaluations per pixel per frame. Enzyme reverse-mode autodiff reduces this to a single forward pass plus tape replay (O(1) evals). This single fact — that autodiff eliminates the V11 performance cliff from 10fps to 182fps — is stated as sufficient justification for F1 alone.

**Influenced:** `specs/05_AUTODIFF.csl`, the autodiff-related crates in `compiler-rs/crates/`.

---

### 2.7 `research/S4_refinement.csl` — 55 lines — Survey: refinement types for graphics

**LiquidHaskell (§ S4.1):** The canonical prior art. Refinement types as base type plus logical predicate (`{v:Int | v > 0}`), SMT-backed via Z3. CSSLv3 adopts the syntax but not the full Z3 SMT path — predicates must be decidable, and Lipschitz bounds are tracked via numeric tags rather than SMT queries. The Coupled Refinement Types paper (ICFP 2022) shows Lipschitz encoding is axiomatically possible in LiquidHaskell but requires ghost proofs; CSSLv3 makes it native.

**F\*, Dafny, Lean4 (§ S4.2):** Explicitly rejected as too heavy; they require interactive proof assistants and full dependent types.

**SDF Lipschitz tracking (§ S4.3):** The central LoA use case for F2. A raymarcher using `pos += dir * sdf(pos)` is only safe when the SDF has Lipschitz constant ≤ 1. Scaling an SDF (e.g., `sdf_scaled(p, k) = sdf(p/k) * k` with `k < 1`) raises the Lipschitz constant above 1 without any warning, causing the raymarcher to overstep and miss surfaces or crash. CSSLv3's `SDF'L<bound>` type tag propagates through composition: intersection takes `max(L₁, L₂)`, smooth union adds a smoothness penalty, scaling by `k` divides the bound by `k`, and affine transforms multiply by the operator norm of the matrix. The ray-march function requires `SDF'L≤1` at its call site, making invalid use a compile error.

**GPU struct padding (§ S4.4):** The second LoA use case for F2. CPU layout (natural alignment) and GPU std140/std430 layouts disagree on struct field offsets, causing silent memory corruption when structs are passed across the CPU-GPU boundary. CSSLv3's `@layout(std140|std430|cpu)` refinement type attribute makes the layout an explicit part of the type, causing a compile error on mismatch and auto-generating the CPU↔GPU marshal/pad functions.

**Influenced:** `specs/03_TYPES.csl`.

---

### 2.8 `research/S5_effects.csl` — 79 lines — Survey: effect systems

**Koka (§ S5.1):** The primary effect-system design source. Row-polymorphic effects via duplicate-label rows (`<exn, div | μ>` where `μ` is an effect variable), with Hindley-Milner inference over rows. User-defined algebraic effect handlers express async/await, exceptions, generators, and probabilistic computation as library code. Perceus reference counting compiles to plain C with no GC. CSSLv3 steals the row-polymorphism verbatim but does not adopt Perceus RC (too costly in hot loops); instead CSSLv3 uses Vale-style regions and linear types.

**LoA effect hierarchy:** The research proposes `pure ⊂ alloc ⊂ gpu_buffer_access ⊂ gpu_dispatch ⊂ io`, plus the specialized effects `NoAlloc`, `NoRecurse`, `Deadline<N>`, `DetRNG`, `GPU`, `CPU`, `XMX`, `RT`.

**OCaml 5 + Eio (§ S5.2):** Production evidence that effect handlers are viable at systems scale. Jane Street and Docker have migrated large codebases to OCaml 5's fiber-based one-shot continuations. The lesson is that effect handlers can reach Rust-class performance with millions of requests per second. CSSLv3 references OCaml 5's implementation for its own lightweight fiber design. The key constraint: in LoA's hot loops, all effects are statically discharged at compile time; runtime effect dispatch is reserved for rare, exception-like paths.

**LoA-specific effect use cases (§ S5.3):** Four concrete examples: `audio_callback` functions are annotated `/ {NoAlloc}` so any heap allocation inside becomes a compile error; GPU and CPU SDF variants use `{GPU}` and `{CPU}` effects to prevent accidentally calling the wrong version; `render_frame` uses `{Deadline<16ms>}` to statically analyze and runtime-assert timing budgets; `sim_step` uses `{DetRNG}` to prevent non-deterministic RNG calls from entering the simulation step (enabling replay).

**Surface syntax:** `fn render(scene) -> Image / {GPU, Deadline<16ms>, NoAlloc}` — effect row after `/` after the return type. User-defined effects and handlers use a Koka-style syntax.

**Influenced:** `specs/04_EFFECTS.csl`.

---

### 2.9 `research/S6_codegen.csl` — 121 lines — Survey: MLIR, backends, and codegen

**MLIR core (§ S6.1):** Likely substrate (tagged "★ likely-substrate"). Dialects as namespaced operation sets with progressive multi-level lowering. CSSLv3 follows the Mojo path: MLIR core with custom `cssl.sdf`, `cssl.effect`, `cssl.staged`, `cssl.engine` dialects, reusing the standard `arith`, `scf`, `cf`, `memref`, `gpu`, `llvm`, and `spirv` dialects. The AI-specific dialects (`linalg`, `tosa`) are explicitly avoided due to Lattner's 2026 "identity crisis" warning about contested AI-focused built-in dialects.

**IREE (§ S6.2):** Reference architecture for the dual-codegen pattern. IREE proves that MLIR-based AOT compilation to both SPIR-V/Vulkan and LLVM/CPU from a single IR is production-grade (AMD SDXL in MLPerf 2025). CSSLv3 models its fat-binary pattern: a single compilation produces an x86-64 object plus SPIR-V blobs, linked by host dispatch code.

**Cranelift (§ S6.3):** Considered as an alternative to LLVM for the CPU backend. 10× faster compile than LLVM, ~14% slower output code. Explicitly selected as the future fast-compile dev-build option, with LLVM as the primary production backend (because Enzyme and GPU codegen both require LLVM).

**QBE (§ S6.4):** Explicitly rejected — too limited (10K lines, no GPU support).

**Intel Arc A770 specifics (§ S6.6):** Detailed hardware specs: 32 Xe-cores with 16 XVE vector engines and 16 XMX matrix engines per core, 560 GB/s memory bandwidth, 17.2 TFLOPs FP32, 137.6 TOPs FP16 via XMX, 275.2 TOPs INT8. Vulkan 1.3 and DX12 are the primary native APIs. The XMX engines get their own `{XMX}` effect tag; the ray-tracing units get `{RT}`; the 256-bit XVE vector lanes get `{SIMD256}`.

**Dual-codegen diagram (§ S6.7):** A complete ASCII art diagram showing the lowering path: CSSLv3-AST → CSSLv3-dialects → split into CPU path (standard dialects → LLVM-IR + Enzyme pass → ELF/PE object) and GPU path (standard dialects → SPIR-V dialect → SPIR-V blob) → combined fat binary.

**Influenced:** `specs/02_IR.csl`, `specs/07_CODEGEN.csl`.

---

### 2.10 `research/S7_engine_as_lang.csl` — 56 lines — Survey: engine as language co-design

**GOAL (§ S7.1):** Naughty Dog's Lisp dialect used for 98% of Jak/Daxter code, compiled to native PS2 machine code. Proves AAA game development with a custom language is viable. The bus-factor lesson — one primary engineer leaving killed the language — is noted as a warning: CSSLv3 must have comprehensive specifications rather than relying on tribal knowledge.

**Unreal Verse (§ S7.2):** Simon Peyton Jones's functional-logic language being co-designed with UE6. Confirms the game-engine + novel-language co-evolution pattern is being actively pursued by a major studio. The declarative scene-description substrate concept is noted.

**Houdini VEX (§ S7.3):** Procedural modeling DSL with per-point/per-primitive/per-vertex execution contexts. Identified as an inspiration for CSSLv3's context-as-effect pattern: `fn my_shader / {PerPoint}`, `fn material / {PerPixel}`.

**Engine primitives as language primitives (§ S7.5):** The core thesis of CSSLv3's engine integration. The listed types — `SDF'L<bound>`, `Scene`, `ECS-archetype`, `Command-Buffer`, `Material-Layout`, `Grid<T, dim>`, `Texture-Handle`, `Fiber` — are actual compiler built-in types, not stdlib types. Type errors at the engine-semantic level are caught at compile time.

**Influenced:** `specs/08_ENGINE.csl`.

---

### 2.11 `research/S8_memory.csl` — 62 lines — Survey: memory models

**Region-based memory (§ S8.1):** Historical overview from Ross AED (1967) through modern arena allocators. The key tradeoff: regions live until explicitly deallocated, which can waste memory if the program is not structured around short-lifetime regions. In CSSLv3, regions are a first-class language feature (not just a stdlib pattern) tracked via `{Region<R>}` in the effect row, with compile-time guarantees that no reference outlives its region.

**Vale linear aliasing (§ S8.2):** The central memory model adoption. Owned values (`^T`) have single ownership on the stack or heap. References (`&T`) carry no lifetime annotation — they use a runtime generational check (8 bytes: generation + index) to detect stale references. Regions temporarily immobilize owned data, allowing references into the region with zero generational check overhead during the region's lifetime.

**Rust arena patterns (§ S8.3):** Practical arena implementations (`typed-arena`, `bumpalo`, `id-arena`, `generational-arena`) are reviewed. The per-frame arena pattern (allocate each frame, reset on present) is identified as eliminating heap allocation from hot rendering loops.

**Handle types (§ S8.4):** `Handle<T>` is a packed `u64` with 24-bit generation and 40-bit index — an 8-byte, cheap-copy, stale-detectable entity reference. This is the fundamental entity reference in LoA's ECS.

**Frame region pattern (§ S8.5):** The `region frame'r { ... }` construct with compile-time escape checking eliminates the audio-callback allocation bug pattern generically: all allocations inside the frame region are freed at the end of the frame, and the `{NoAlloc}` effect can enforce zero heap allocation in specific call contexts.

**Influenced:** `specs/03_TYPES.csl`, `specs/08_ENGINE.csl`.

---

### 2.12 `research/S9_bootstrap.csl` — 65 lines — Survey: bootstrapping strategy

**Zig's bootstrap model (§ S9.1):** The reference model. Zig commits a pre-built `zig1.wasm` blob plus a 4K-line `wasm2c` converter. This builds stage1 (the Zig-in-C output), which builds stage2 (the full self-hosted Zig), which builds stage3 (optimized). CSSLv3 adopts the same pattern: a Rust-hosted stage0 that outputs C or LLVM-IR as a platform seed, a CSSLv3-self-hosted stage1 compiled by stage0, and a self-optimizing stage2 compiled by stage1. At stage1 the Rust code is discarded and never maintained alongside the self-hosted version.

**Bootstrap language choice (§ S9.2):** Rust is chosen for stage0 over Zig (comptime instability, 20× slowness) and OCaml (less familiar to the developer, less mature MLIR bindings). The reasons: `mlir-sys` / `melior` crates are mature as of 2024–2025, Enzyme is accessible via the Rust autodiff project goal fork, `inkwell` provides LLVM bindings, and the developer has prior familiarity.

**mrustc reference (§ S9.3):** Notes that mrustc allows bootstrapping Rust from C++. The equivalent for CSSLv3 would be providing a C-backend output generator at stage3 that allows CSSLv3 to be bootstrapped without a prior CSSLv3 binary — analogous to providing a source tarball.

**Dev-loop phases (§ S9.4):** A ten-phase plan from lexer/parser/AST (weeks) through type-checker, MLIR codegen, first LoA demo shader, effect system, Enzyme autodiff, refinement types, staging, and finally self-hosting. This maps directly to the P1–P10 roadmap in `stage1/README.csl`.

**Influenced:** `specs/01_BOOTSTRAP.csl`, the ordering of work in `compiler-rs/`.

---

### 2.13 `research/S10_syntax.csl` — 133 lines — Survey: syntax design

**APL lineage (§ S10.1):** Iverson's "Notation as a Tool of Thought" thesis (Turing Award 1979) — glyph density improves signal-to-noise ratio. Descendants surveyed: J, K/Q, Dyalog APL, BQN, Uiua, Futhark, Dex.

**BQN and Uiua (§ S10.2):** Uiua's input method is identified as the key pattern to adopt: the author types ASCII names, and the formatter automatically upgrades them to glyphs on save. CSSLv3 applies this pattern so developers never need to type Unicode characters directly.

**Dex (§ S10.3):** Google Research array language with index-as-type dependent types, broadcast inference, first-class AD, and an effect system — exactly the feature combination CSSLv3 needs. The paper "Getting to the Point" (ICFP 2021) is cited as a reference.

**Futhark (§ S10.4):** ML-family array language with uniqueness types for in-place update checked statically, achieving referential transparency with imperative performance. Uniqueness types are noted as a possible supplement to Vale generational references for GPU buffers.

**CSLv3 as surface (§ S10.5):** The central syntax design question — should CSSLv3 use CSLv3 notation as its program surface syntax, or should it use a more conventional syntax? The arguments for (already designed, already readable by developer and AI collaborator, density 5–6× English) and against (too novel for contributors, no tooling, domain-overlap issues, lacks arithmetic-expression coverage) are weighed. The conclusion is a hybrid: CSLv3 in block comments as a chain-of-thought thinking layer; CSSLv3 as the program surface. A concrete example is shown with `@differentiable`, `@lipschitz`, `@staged`, and effect-row annotations.

**Syntax decisions (§ S10.5):** Type annotation `: T`; refinement `T'tag` or `T{P(x)}`; effect row `/ {E1, E2<N>}`; attribute `@name(args)`; generics `<T: Trait>`; lambda `|x: T| -> U { body }` or `\x. body`; match arms with `=>`.

**Compiler glyph operations (§ S10.6):** The compiler accepts both ASCII and Unicode for operators: `->` and `→`, `/=` and `≠`, `<=`/`≤`, `>=`/`≥`, `forall`/`∀`, `exists`/`∃`, `|>`/`▷`, `::`/`∷`. The formatter auto-converts to Unicode on save; storage is UTF-8.

**Influenced:** `specs/09_SYNTAX.csl`.

---

### 2.14 `research/compass_artifact_wf-078df140-cb59-4fb3-924f-dcd81902d4f3_text_markdown.md` — 105 lines — External AI design essay

This file is a Markdown document that appears to be an AI-generated (likely Claude) design essay titled "Designing CSSLv3: a no-LLVM shader-and-systems language for the radiance-cascade era." Unlike the internal research notes, it is written as a polished third-person prose document with section headers, a comparison table, a risk table, and citations to specific papers. It targets a "small team of 2–4 engineers" rather than the solo-developer framing of the internal notes.

Key content:

**Architecture:** Recommends a three-IR pipeline (HIR → MIR → LIR) with MIR being structured-by-construction to avoid the SPIR-V structured control flow problem. The SPIR-T project from Embark's Rust-GPU team is cited as proof that lowering unstructured CFG IRs to SPIR-V is a permanent bug source. Cranelift (not LLVM) is recommended for the CPU backend in stages 1 and 2.

**No-LLVM thesis:** The essay argues that production-quality SPIR-V can be emitted without LLVM, citing Slang, naga (wgpu), tint (WebGPU), and Embark's `rustc_codegen_spirv` as existence proofs. This is notably different from the internal research documents, which decided on LLVM for the autodiff and GPU path.

**Feature stack:** Recommends adopting Austral-style linear capabilities (a 600-line linearity checker), Koka-style algebraic effect handlers via Xie & Leijen's evidence-passing translation, Jai/Zig-style comptime metaprogramming restricted to pure functions over the AST, Slang.D-style differentiable types, and F5 information flow control (Jif-style decentralized label model) as an opt-in capability for GPU compute in privacy-sensitive contexts.

**Rendering vision:** Provides substantial discussion of three rendering techniques as the target for CSSLv3's expressiveness: Sannikov/Freeman radiance cascades (cited as 1.85ms for 512×512 on RTX 3080), sparse voxel DAGs (SVDAG, SSVDAG, TSVDAG) for geometry, and Teardown-style dense voxel volumes for destruction physics.

**Risk table:** A 24-month execution plan with four milestones (HIR+MIR+Cranelift, SPIR-V backend, linear types + autodiff, effect handlers + comptime + vertical-slice renderer), each with a named failure mode.

**Jai cautionary tale:** The essay's closing argument: Jai spent 12 years in closed development and lost mindshare to Odin and Zig which shipped early. Slang, Koka, and Austral each earned credibility by shipping something concrete before marketing their language features. CSSLv3's single most important strategic decision is what to defer: information flow control to v2, the bespoke x86 backend to v3, cross-platform GPU support until after Vulkan is solid.

**Note on divergence from internal research:** This document recommends no-LLVM (Cranelift for CPU, rspirv-style for GPU) whereas the internal `Q6_IR_architecture.csl` and `S9_bootstrap.csl` both decided on LLVM-with-Enzyme as the year-1 path. It also introduces F5 information flow control as a feature, which is not listed in the internal research's four non-negotiable features (F1–F4). The essay appears to have been written with broader scope assumptions and slightly different hardware/team premises than the internal notes.

---

## 3. STAGE1 SECTION

### 3.1 `stage1/README.csl` — 63 lines — Self-hosting roadmap

This document defines the P1–P10 self-hosting path and the current gating state as of session T11-D33 (2026-04-18).

**Goal:** stage0 (Rust-hosted CSSLv3 compiler) compiles stage1 source to a stage1 binary. stage1 then compiles itself to produce stage1-prime. The fixed-point check is byte-exact equality between stage1 and stage1-prime. A custom x86-64 backend in stage1 (R16 anchor) replaces the Cranelift backend from stage0.

**Capability gate — what stage0 already has:**
- Lex and parse with dual-surface support: CSLv3-native notation and Rust-hybrid syntax.
- HIR lowering with Hindley-Milner type inference, effect-row handling, and refinement obligation generation.
- MIR body lowering for arithmetic, intrinsics, and inter-function calls.
- Automatic differentiation walker (forward and backward modes) with 19 primitives including piecewise-linear `min`/`max`/`abs`.
- Cranelift JIT executing scalar programs, multi-return functions, and libm transcendentals.
- Five codegen backends with text-emit: SPIR-V, DXIL, MSL, WGSL, and Cranelift.
- R18 telemetry with BLAKE3 hashing, Ed25519 signatures, and 9 of 10 oracle modes.

**What stage1 needs that stage0 does not yet have:**
- Generic type monomorphization (needed for compiler data-structures like `Vec<T>`).
- A rich standard library with collection types (`Vec`, `HashMap`, `BTreeMap` analogues).
- Trait-like dispatch (needed for the pass registry and backend abstraction).
- String handling with UTF-8 slicing.
- File I/O (the IO effect fully wired to OS syscalls).
- Iterator combinators (`map`, `filter`, `fold`).
- Pattern matching on sum-type variants beyond current basic match support.
- A custom x86-64 backend (replacing Cranelift).

**The P1–P10 roadmap:**

| Phase | Capability to land |
|-------|-------------------|
| P1 | stdlib-core: `Vec<T>` and `HashMap<K,V>` implementable in CSSLv3 |
| P2 | trait-dispatch: pattern-matched pass registry and backend abstraction |
| P3 | IO-effect concrete: `read_file` and `write_file` lowered to OS syscalls |
| P4 | string-handling: UTF-8 slicing and formatting |
| P5 | iterator combinators: `for-each`, `map`, `filter`, `collect` |
| P6 | sum-type matching: exhaustive pattern match on all enum variants |
| P7 | self-hosted parser: CSSLv3-written parser handling the full grammar |
| P8 | self-hosted HIR and MIR: type system reimplemented in CSSLv3 |
| P9 | self-hosted Cranelift-equivalent backend: emits x86-64 directly |
| P10 | fixed-point: stage1 compiles itself → stage1-prime byte-exact |

**Critical guidance (§ DO-NOT-START-YET):** The README explicitly forbids beginning stage1 work before P1–P6 capabilities are landed in stage0. Premature self-host attempts produce a stage1 that lacks primitives which can only be added by returning to stage0, defeating the bootstrap sequence.

**Verification hook:** `cssl-examples/src/stage1_scaffold.rs` (added at T11-D33) parses both `hello.cssl` and `compiler.cssl` through the stage0 front-end at test time, ensuring they remain lex/parse-valid as the grammar evolves.

---

### 3.2 `stage1/compiler.cssl` — 18 lines — Placeholder self-hosted compiler entry point

This is the earliest piece of actual CSSLv3 source code in the repository — the future self-hosted compiler's `main` entry point. As of T11-D33 it is a minimal placeholder. It reveals the following about CSSLv3's surface syntax:

```
fn main() -> i32 {
    0
}
```

**Language features visible:**
- Function declaration keyword: `fn`
- Function name: identifier `main`
- Argument list: empty `()`
- Return type annotation using `->` arrow and primitive type `i32`
- Function body in `{ }` braces
- Integer literal `0` as an expression-statement (the return value)

The file header (lines 1–15) uses `//` for single-line comments, identical to Rust and C. The block comment in the preamble (lines 1–14) explains the file's purpose and the P1–P10 roadmap reference.

**Maturity status:** Empty `main()` — a deliberate placeholder. The comment notes that the file exists so: (a) stage0 can round-trip it through lex+parse; (b) the directory slot is occupied; (c) future sessions can track progress via `git diff`.

---

### 3.3 `stage1/hello.cssl` — 7 lines — Minimum-viable CSSLv3 surface file

The simplest possible CSSLv3 source file that the stage0 compiler accepts:

```
fn hello() -> i32 {
    42
}
```

**Language features visible:**
- Same surface as `compiler.cssl`: `fn`, identifier, `()` argument list, `-> i32` return type, `{ }` body.
- Integer literal `42` as the expression body (no explicit `return` keyword needed — expression-body convention like Rust).
- `//` single-line comment in the preamble (lines 1–3).

**Significance:** This file confirms the CSSLv3 surface syntax resembles Rust in its most basic forms, with `fn`, `->`, and expression-bodied functions. It does not yet demonstrate any of F1–F4 features, but the `@differentiable`, `@staged`, and effect-row syntax shown in `S10_syntax.csl` (§ S10.5) represent how the language will look when those features are exercised.

---

## 4. SCRIPTS SECTION

### 4.1 `scripts/surgical_identity_redact.py` — 178 lines

**Purpose:** A one-time and repeatable tool for redacting identity-claim strings from Claude project transcript files (JSON and JSONL format). It replaces occurrences of "Lazarus" and "Prismatic Hydra" handle names with corrected values in all string values within JSON documents, preserving structural validity by always operating on parsed Python objects rather than raw text, then re-serializing. Used manually (`--scan` to preview, `--apply` to write). Writes atomically via a `.tmp` + `os.replace()` rename with post-write validation.

**Dependencies:** `argparse`, `json`, `os`, `sys`, `typing` — all standard library; no third-party packages.

**All functions:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `redact_string` | `(s: str) -> Tuple[str, int]` | Applies all entries in the `REPLACEMENTS` list to a single string, returning the modified string and the total count of replacements made. |
| `walk` | `(obj: Any, changes: list) -> Any` | Recursively traverses a JSON-deserialized Python object (dict, list, or scalar); calls `redact_string` on every string value, accumulates replacement counts into `changes`, returns the mutated structure. |
| `process_jsonl` | `(path: str, apply: bool) -> Tuple[int, int, int]` | Processes a JSONL file (one JSON object per line): parses each line individually, runs `walk` on each parsed object, writes corrected lines to a `.tmp` file, validates the `.tmp` as valid JSONL, then atomically renames. Malformed lines are dropped with a warning. Returns `(total_replacements, total_lines, dropped_lines)`. |
| `process_json` | `(path: str, apply: bool) -> Tuple[int, int, int]` | Processes a single-document JSON file: parses the entire file, runs `walk`, writes atomically with post-write validation. Falls back to plain-text `redact_string` if JSON parsing fails. Returns `(replacements, kind, 0)` where `kind` is `1` for single-JSON or `-1` for the plain-text fallback. |
| `process_file` | `(path: str, apply: bool) -> None` | Dispatches to `process_jsonl` or `process_json` based on the file extension (`.jsonl` vs anything else); prints a `[SCAN]` or `[APPLIED]` summary line per file. |
| `main` | `() -> int` | Entry point: reconfigures stdout to UTF-8 on Windows, parses `--scan`/`--apply` flags and `files` positional arguments, loops over file paths calling `process_file`, returns exit code 0. |

**Usage:** Manual one-shot redaction — not part of CI. Invoked when transcript hygiene is needed.

**Bug noted:** In `process_jsonl` (line 76), the `opener` file handle variable is initialized only when `apply=True`, but lines 83 and 96 call `opener.write(...)` inside the same function without checking `apply`. If `apply=False`, `opener` is `None` and `opener.write` raises `AttributeError`. However, line 83 is guarded by `if apply:` (`if opener is not None` is implicit in the check), and line 96 is also inside an `if apply:` block at line 95 — so on closer inspection both writes are correctly guarded and the function is safe. The `if opener is not None: opener.close()` pattern at line 108 is the correct cleanup for the scan-mode case where opener was never opened.

**Separate subtle issue:** In `process_jsonl` at line 83 (`opener.write("\n")`), the empty-line preservation write is inside `if apply:` but NOT inside a `try`/`except`, meaning an I/O error on writing the temp file would surface as an uncaught `OSError` rather than a clean error message. This is minor but worth noting.

---

### 4.2 `scripts/validate_spec_crossrefs.py` — 156 lines

**Purpose:** A CI and manual tool that validates that all `§§`-style cross-references in spec files, research files, and repo-root files resolve to real file stems. The tool distinguishes "file-shaped" references (uppercase identifiers, `Sx_...` patterns, `Qx_...` patterns, two-digit numerics) from local in-document section anchors (lowercase-with-hyphens, mixed-case), validates only the former, and exits with code 0 for all-clean or 1 for any unresolved reference. Referenced in `specs/23_TESTING.csl` and `DECISIONS.md T1-D3`.

**Dependencies:** `re`, `sys`, `pathlib.Path` — all standard library.

**All functions:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `looks_like_file_ref` | `(token: str) -> bool` | Returns `True` if the token matches any of the four file-reference patterns: uppercase-identifier (with optional two-digit prefix), `S<N>` with optional body, `Q<N>` with optional body, or two-digit numeric. Returns `False` for lowercase/mixed-case local section anchors. |
| `collect_spec_inventory` | `() -> tuple[set[str], set[str], dict[str, str]]` | Scans `specs/`, `research/`, and the repo root for `.csl` files; extracts all resolvable name forms (full stem, numeric prefix, body after `NN_`, body after `S<N>_`, body after `Q<N>_`) into a set; builds a prefix-map for unique prefix-match resolution; returns `(names, numeric_prefixes, prefix_map)`. |
| `is_resolvable` | `(token: str, names: set[str], nums: set[str], prefix_map: dict[str, str]) -> bool` | Returns `True` if the token is in the names set, in the numeric prefixes set, in the prefix map (unique prefix match), or can be decomposed as `NN_body` where both parts are known. |
| `main` | `() -> int` | Builds the spec inventory; scans all `specs/*.csl`, `research/*.csl`, repo-root `*.csl`, and repo-root `*.md` files for `§§`-references; reports unresolved file-shaped references with file and line number; prints totals; returns 0 (clean) or 1 (failures). |

**Usage:** Can be run manually (`python scripts/validate_spec_crossrefs.py`) or from CI. The path resolution is relative to the script's own location (`Path(__file__).resolve().parent.parent`), so it works from any working directory.

**Note on scope:** The tool only validates references that look like file names. It intentionally skips `§§ some-local-section` references (lowercase with hyphens) which are in-document anchors. This matches the explicit design note in the file header.

**Potential issue:** The `FILE_NUM_PAT` pattern (`r"^\d{2}$"`) accepts two-digit numeric tokens like `§§ 01`. If a spec file is ever named with a number that is also a valid local section number (e.g., a `§§ 14` reference when there is no `14_*.csl` file but there is a local section 14), the tool would report a false negative (passes) because the numeric pattern would match without checking for the corresponding file. This is a design choice documented in the file but worth flagging: the validator trusts that two-digit numerics without an underscore suffix always correspond to spec file number prefixes.

---

### 4.3 `scripts/differential_lex_vs_odin.py` — 105 lines

**Purpose:** A CI skeleton for a differential lexer oracle — comparing the token stream produced by the Rust-hosted stage0 compiler (`csslc tokens --json <file>`) against the token stream produced by the Odin-hosted CSLv3 reference parser (`parser.exe --tokens <file>`) on the same fixture files. Any divergence indicates a specification ambiguity in the CSLv3 grammar, to be filed against CSLv3 (not CSSLv3). The script is referenced in `DECISIONS.md T1-D2` and `specs/16_DUAL_SURFACE.csl`. As of the time of writing, the script is in documentation-only mode: all three core comparison functions are stubs that return empty strings/lists, and the script exits 0 when prerequisites are missing.

**Block conditions for full activation (documented in file header):**
- (a) `csslc tokens --json <file>` subcommand not yet implemented (targeted at T10 scope).
- (b) Canonical token-kind mapping between Rust `TokenKind` and Odin `Token_Kind` not yet established.
- (c) Shared fixture directory strategy not yet finalized.

**Dependencies:** `os`, `subprocess`, `sys`, `pathlib.Path` — all standard library. (`subprocess` is imported but not yet used in any stub.)

**All functions:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `emit` | `(msg: str) -> None` | Prints a prefixed log line `§ lex-oracle : <msg>` to stdout. |
| `check_prerequisites` | `() -> list[str]` | Checks for the existence of the `csslc.exe` binary, the `parser.exe` Odin binary, and the `CSLv3/tests/` fixtures directory; returns a list of human-readable missing-item descriptions. |
| `csslc_tokens` | `(path: Path) -> str` | Stage0 stub. When activated at T10, will invoke `csslc tokens --json <path>` and return JSON output. Currently returns empty string. |
| `odin_tokens` | `(path: Path) -> str` | Stage0 stub. When activated at T10, will invoke `parser.exe --tokens <path>`, normalize the output, and return it. Currently returns empty string. |
| `compare_tokens` | `(rust_json: str, odin_txt: str) -> list[str]` | Stage0 stub. When activated, will perform canonical-mapping between Rust `TokenKind` and Odin `Token_Kind` and return a list of diff lines. Currently returns empty list. |
| `main` | `() -> int` | Entry point: calls `check_prerequisites()`; if missing, prints the run-plan and exits 0 (clean stub behavior); otherwise iterates over all `*.csl` fixture files under `CSLv3/tests/`, calls `csslc_tokens` + `odin_tokens` + `compare_tokens` per file, reports divergences, returns 0 (all match) or 1 (divergences found). |

**Usage:** Wired into `.github/workflows/ci.yml` (`diff-linux-arc-a770` job matrix) but currently exits 0 unconditionally because all prerequisites are missing. Will become a real gate when `csslc tokens --json` lands.

**Note:** `subprocess` is imported but unused — this is expected for a stub; it will be used when `csslc_tokens` and `odin_tokens` are implemented.

---

## 5. SLICE NOTES

### Maturity assessment

**research/:** Complete as a research artifact. All ten survey sections have at least partial content. S8, S9, and S10 are fully written (despite the `00_MANIFEST.csl` marking them as "pending" — the manifest was written before those files were completed). The synthesis (`99_SYNTHESIS.csl`) is fully written and represents the definitive cross-reference. Q6 is an unusually thorough analysis document. The only gap is that S1 marks Taichi and Hylo/Val/Circle/Carbon as pending deep-dives, and those subsections are genuinely empty stubs.

**stage1/:** Early scaffold only — exactly as described in `README.csl`. Neither `.cssl` file contains real compiler logic. The directory's value is entirely in `README.csl`'s documentation of the P1–P10 roadmap and the gate conditions. The `stage1_scaffold.rs` verification hook ensures the files stay parse-valid.

**scripts/:** Mixed maturity.
- `surgical_identity_redact.py` is fully functional and production-ready for its intended purpose.
- `validate_spec_crossrefs.py` is fully functional and suitable for CI use.
- `differential_lex_vs_odin.py` is a documented stub — correctly flagged in its own header — that will become functional when the `csslc tokens --json` subcommand lands at T10.

### Notable findings

1. **Compass artifact diverges from internal decisions.** The `compass_artifact_*.md` essay recommends a no-LLVM architecture (Cranelift for CPU, direct SPIR-V emission without LLVM) and introduces F5 information flow control as a feature. The internal `Q6_IR_architecture.csl` decided on LLVM for year 1 (with Enzyme) and the manifest lists only F1–F4 as non-negotiable. The external essay appears to represent an earlier or alternative design exploration rather than the current binding decision. A new contributor should treat `Q6_IR_architecture.csl` as authoritative and the compass artifact as an exploration document.

2. **Stage1 gate is not close.** The P1–P10 roadmap requires eight major capability additions to stage0 before self-hosting can begin (P1–P6 are explicit prerequisites for starting). As of T11-D33, none of P1–P8 are marked complete: generics monomorphization, rich stdlib, trait dispatch, string handling, file I/O, iterator combinators, rich sum-type pattern matching, and a custom x86-64 backend are all outstanding. `README.csl` is correct to instruct "do not start yet."

3. **CSSLv3 surface syntax confirmed.** The two `.cssl` files (`compiler.cssl` and `hello.cssl`) confirm that the CSSLv3 surface syntax uses `fn`, `->`, `i32`, `{ }` bodies, `//` comments, and expression-body convention (no explicit `return`). This matches the tentative syntax in `S10_syntax.csl § S10.5`. The files also confirm that integer literals are the only currently exercised literal form in stage1-visible source.

4. **`validate_spec_crossrefs.py` two-digit numeric false-negative risk.** The `FILE_NUM_PAT = re.compile(r"^\d{2}$")` pattern could accept two-digit references like `§§ 42` and report them as resolved even if no `42_*.csl` file exists — because the validator checks `token in nums` and `nums` is the set of numeric prefixes found in actual file stems. In practice, if there is no file with that numeric prefix, it will not be in `nums` and will correctly fail. This is not a bug but a documentation gap: the validator's behavior for out-of-range numerics is correct but non-obvious.

5. **`subprocess` unused in `differential_lex_vs_odin.py`.** Line 4 imports `subprocess` but no function in the current stub uses it. This is expected scaffolding and not an error, but a linter would flag it. (`scripts/differential_lex_vs_odin.py:4`)

6. **Taichi and Hylo/Val/Circle/Carbon sections are empty stubs in S1.** `research/S1_novel_langs.csl` lines 106–113 mark these as `TBD` with no content. If CSSLv3 design decisions ever touch physics simulation (Taichi) or unique-reference-heavy type systems (Hylo/Val), these sections should be completed before finalizing `specs/03_TYPES.csl`.

7. **The Consent-as-OS feature is unique.** The `99_SYNTHESIS.csl § 99.9` mandate — encoding the project's ethics directly into the type system via effect tags like `{SurveillanceCapability}`, `{CoercionCapability}`, and `{DataExfiltration}` — is not found in any prior-art language surveyed. This differentiates CSSLv3 from commodity systems languages and is a first-class design goal, not a policy document.
