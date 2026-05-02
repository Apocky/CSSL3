# 13 : CSSL-LINEAGE-DISTILLATION (W14-D)
# CSSL-v1 → CSSL2 → CSSLv3 · spelunked-lineage for-Infinity-Engine
# ══════════════════════════════════════════════════════════════════

§ SOURCE-CORPUS  ⟦READ-ONLY · ¬ modified⟧
  CSSL-v1  : C:\Users\Apocky\source\repos\CSSL\
            ⊢ CLAUDE.md (52L) + PRIME_DIRECTIVE.md + specs\CSL_SPEC.csl (211L)
            ≡ NOTATION-only · ¬ programming-language · single-file-spec
  CSSL2    : C:\Users\Apocky\source\repos\CSSL2\
            ⊢ README.md (300L) + CLAUDE.md (215L) + 16 specs (00..15) ≈ 256KB
            ⊢ compiler\ Odin (~5600 LOC : 13 .odin files)
            ⊢ runtime\ C (cssl2_runtime.c + .h)
            ⊢ tests\ basic + programs\ (20 .cssl + .expected)
            ≡ REAL programming-language · phase-2a vec3 ✓ · 34/34 tests
  CSSLv3   : C:\Users\Apocky\source\repos\CSSLv3\
            ⊢ specs\ 00..30 + grand-vision + infinity-engine
            ⊢ compiler-rs\ Rust workspace · 31+ per-concern crates
            ≡ CURRENT · stands on-the-shoulders of-both-predecessors

# ══════════════════════════════════════════════════════════════════
§ 1 · CSSL-V1-ARCHITECTURE
# ══════════════════════════════════════════════════════════════════

§ IDENTITY
  CSSL-v1 ≠ programming-language
  CSSL-v1 ≡ Caveman-Substrate-Specification-Language ≡ NOTATION
  ⊢ creator : Shawn-Wolfgang-Michael-Baker (Apocky)
  ⊢ origin  : SSL (SIGIL-Spec-Language) → stripped-English → caveman-directive → CSSL
  ⊢ proven  : 3.5× compression vs-markdown · zero info-loss
  ⊢ runtime : Claude (or any-LLM that-learns-notation)
  ⊢ "compiler" ≡ Claude.tokenizer · "CPU" ≡ Claude.attention
  ⊢ optimize: tokens-per-insight ¬ cycles-per-instruction

§ §0..§11 SPEC-STRUCTURE (CSL_SPEC.csl ≡ self-hosting bootstrap)
  §0  WHAT      ← purpose : encode specs/designs/knowledge for-LLM-consumption
  §1  PRINCIPLES P1..P7
       P1 every-token load-bearing
       P2 structure>prose
       P3 glyphs>words
       P4 self-describing (§0 defines-notation)
       P5 human-readable-fallback ¬ encrypted
       P6 domain-specific shorthand encouraged
       P7 Claude IS-runtime
  §2  GRAMMAR : DOC := HEADER SECTION+
              SECTION := SEPARATOR SECTION_HEAD CONTENT*
              CONTENT := ENTRY | BLOCK | RULE | INSIGHT | WARNING | TABLE | CODE
       glyphs : § I> W! R ✓ ◐ ○ ✗ → ← ↔ ⇒ ├─ └─ │ ∂ ∇ ∆ Σ ∫
  §3  COMPRESSION-RULES
       STRIP : articles · prepositions · hedging · filler · connectors · meta
       KEEP  : nouns · verbs · relationships · numbers · gates · rules
       ratios: arch-docs 3.5× · API-specs 4× · narrative 1.5× · code 1×
  §4  REASONING-FORMAT (anti-hedge templates)
  §5  FILE-FORMAT : .cssl ext · UTF-8 · LF · 2sp indent
  §6  TOOLCHAIN  : CSSL→MD · MD→CSSL · CSSL→JSON · validator · formatter
  §7  IMPL : Python (proto) | Odin (LoA-toolchain-match) | Rust (binary)
  §8  TOKEN-ANALYSIS : glyphs ~10% · §3 compression ~60% · combined ~65-70%
  §9  BOOTSTRAP : self-hosting ✓ · proven on-Apocky-master-specs
  §10 vs-MARKDOWN/YAML/JSON/COMMENTS
  §11 RULES R0..R8

§ KEY-INSIGHT
  v1 = NOTATION-tier-only
  v1 W! ≠ programming-language
  v1 ≡ CSL precursor (later renamed CSL ; CSSL reserved for-language)
  v1 introduces : self-hosting + density-as-sovereignty + glyph-vocabulary
  ⊢ v1 LIVES-ON inside-CSSLv3 as-CSLv3-default-notation (per Apocky-CLAUDE.md)
  ⊢ ∀ CSSL-v2 + CSSLv3 specs WRITTEN-IN CSL-notation ← bootstrap-chain

# ══════════════════════════════════════════════════════════════════
§ 2 · CSSL2-IMPROVEMENTS-OVER-V1 (the-leap : notation → language)
# ══════════════════════════════════════════════════════════════════

§ IDENTITY-SHIFT
  CSSL2 ≡ Caveman-Sigil-Substrate-Language v2
  CSSL2 ≠ notation · CSSL2 ≡ REAL programming-language
  CSSL2.thesis : "every-bug-across LoA-V5..V12 + DGI-V1..V2
                  came-from-same-root : LANGUAGE-ALLOWED-IT"
  CSSL2.ask    : make-each-bug-class STRUCTURALLY-IMPOSSIBLE
                 ¬ by-policy · by-types + immune-systems

§ SUPERSEDED PRIOR-RESEARCH (Sigilv2 ≡ "compiler-textbook-with-our-name")
  diagnosis : recursive-descent + HM-inference + C-emission + SPIR-V = generic
  swap "CSSL"→"any-language" ; research-identical
  signal : ¬ deep-enough
  rejected:
    ✗ "emit-C-cowardice"      ← C¬SIMD ¬coord-spaces ¬refinement ¬effects ¬GA ¬tensors
    ✗ "HM-inference"          ← designed-for-λ-calc · CSSL=imperative-mutation-heavy-GPU
    ✗ "dimensional-types-only" ← Kennedy-1996 · stopped-at-units · ¬ pushed-further
    ✗ "missed-AD"             ← V11-worst-perf-bug ¬ even-mentioned
    ✗ "missed-effects"        ← ¬ GPU/audio/frame-context-awareness
    ✗ "missed-staging"        ← ¬ scene-as-data + code-as-data story

§ NEW ARCHITECTURE (CSSL2)
  ¬ compiler-textbook
  ⊕ DOMAIN-SPECIFIC : SDF-game-engines (LoA + ApockyDGI)
  ⊕ BUG-DRIVEN : every-feature ties-to-specific-V11/V12/DGI-bug
  ⊕ TYPE-SYSTEM-AS-IMMUNE-SYSTEM (R14)

§ SIX-NON-NEGOTIABLE-FEATURES (the-six-pillars)
  F1 AUTODIFF
     @differentiable fn → analytic-gradient generated
     ref : Slang ([Differentiable]) · Taichi · Swift-for-TF · Enzyme
     fixes : V11 SDF-normal 6-central-diff-evals → 1 analytic-eval (4×+ recovery)
  F2 REFINEMENT-TYPES + LIPSCHITZ
     sdf<lip=K> propagates-through-composition
     ref : Liquid-Types · F* · LiquidHaskell
     fixes : V11 text-glyph crash @ feature-size + lip-mismatch (6hr→0hr debug)
  F3 EFFECT-SYSTEM (graded-capabilities)
     GPU-context : {NoAlloc · NoRecurse · NoLock · NoBlock}
     Audio-ctx   : {NoAlloc · NoLock · NoBlock · Deadline<2ms>}
     ref : Koka · algebraic-effects · row-polymorphism
     fixes : 5+ V11-bug-classes (alloc-in-audio · GPU-recursion · frame-leak · MCP-block · thermal-deadlock)
  F4 STAGED-COMPUTATION
     scene-DSL→evaluator · grammar→content · material→shader
     ref : MetaOCaml · Zig-comptime · Terra · Halide
     fixes : V12 hand-authored-scene + every-ray-evals-all-primitives (no-BVH)
  F5 DIGITAL-ANTIBODIES (RUNTIME · spec-14)
     L1 innate → L2 adaptive → L3 behavioral → L4 semantic → L5 lymph (gossip)
     fixes : prompt-injection · jailbreak · adversarial-input neutralization
     ref : artificial-immune-systems · Lakera-Guard · MISP/STIX
  F6 CODE-ANTIBODIES (COMPILE-TIME · spec-15)
     compiler-learns-from-past-bugfixes
     each-shipped-fix becomes future-compile-error for-same-anti-pattern
     ref : Semgrep · Clippy · case-based-reasoning

§ SHOULD-HAVE FEATURES (D6..D8)
  GA      : Cl<p,q,r> as-type-constructor · rotor/motor replace-quat/mat4
  TENSORS : tensor<T,Shape> + Einstein-summation + symmetric/antisymmetric mods
  SPATIAL : region-tags + lifetimes + session-types + flow-narrowing

§ COULD-HAVE (D9..D10)
  info-geometric optimization (deferred · ¬ phase-1)
  syntax-entropy (CSL-extends-to-collaboration · per-spec-09)

§ COMPILER-ARCHITECTURE (CSSL2)
  ⊢ host-lang : Odin (zero-learn-curve · LoA/V12 already-Odin)
  ⊢ default-backend : LLVM textual .ll → llc (per-L2-spec-13)
  ⊢ alt-backend : C11 (phase-0 fallback · kept-as-escape-hatch)
  ⊢ deferred : SPIR-V textual .spvasm (phase-2c · gated-by spirv-tools)
  ⊢ runtime : minimal-C (cssl2_print · cssl2_print_int · cssl2_print_float · vec3-math)

§ PHASE-PROGRESSION (status @-2026-04-16 commit-time)
  P0   ✓ bootstrap (~4123 LOC · 20/20 tests)
  P1a  ✓ polish + LLVM + effects + refinements (+~2000 LOC · 26/26 tests)
  P1b  ◐⊘ comptime + scalar-AD ✓ · scalar-scene-DSL + SPIR-V deferred (per-L8)
  P2a  ✓ vec3-unlock (Cl<3,0,0>.Vector minimum + dot/cross/length/normalize · 34/34 tests)
  P2b..2g ○ vec3-AD · scene-DSL+BVH · GA · spatial · tensors · antibodies (¬ shipped)

# ══════════════════════════════════════════════════════════════════
§ 3 · COMPILER-STAGES per-version
# ══════════════════════════════════════════════════════════════════

§ CSSL-v1 (NOTATION · ¬ compiler-stages)
  ⊢ Claude.tokenizer ≡ "lexer"
  ⊢ Claude.attention ≡ "parser+sema"
  ⊢ ¬ codegen · ¬ runtime · ¬ binary-output
  ⊢ optional toolchain : validate · format · CSSL→MD · CSSL→JSON · MD→CSSL (Claude-API)

§ CSSL2 PIPELINE (linear · per-driver.odin run_frontend)
  source.cssl
    → tokenize        (lexer.odin · 47 token-kinds · newline-aware · error-recovering)
    → parse           (parser.odin · recursive-descent + precedence-climbing)
    → run_autodiff    (synthesize <name>_grad fns · pre-sema)
    → analyze         (sema.odin : name-resolution + type-inference + checking)
                      (effect-system + refinement + Lipschitz passes integrated)
    → run_comptime    (@unroll lower · `comptime let` evaluation)
    → CSSL-IR         (semantic-rich-SSA · types+effects+lifetime preserved)
    → optimize        (CSE · DCE · constant-fold · BVH-insert (deferred) · AD-source-to-source)
    → lower
       ├─ LLVM-IR-textual  (codegen_llvm.odin · default · ~1043 LOC)
       │   → llc → linker → .exe
       └─ C11             (codegen_c.odin · fallback · phase-0 path)
           → clang → .exe

  CLI : compile · run · test · tokens · ast · types · emit-c · version · help
  exit-codes : 0 success · 1 errors · 2 usage · forwards-program-exit on-run

§ CSSLv3 PIPELINE (per-spec-01 + spec-02 · multi-stage bootstrap)
  STAGE-0 (Rust-host · throwaway) : Cargo-workspace · 31+ per-concern-crates
    cssl-lex → cssl-parse → cssl-ast → cssl-hir (typed-elaborated)
              → cssl-mir (MLIR-dialect bridge) → cssl-lir (low-level)
              → cssl-cgen-cpu-cranelift  (CPU · throwaway)
              → cssl-cgen-gpu-{spirv|dxil|msl|wgsl}  (GPU multi-target)
    parallel-passes : cssl-caps (Pony-6) + cssl-effects (Koka-rows)
                    + cssl-ifc (Jif-DLM) + cssl-autodiff + cssl-jets
                    + cssl-staging + cssl-macros + cssl-smt (Z3+CVC5)
                    + cssl-futamura (P1+P2+P3) + cssl-mlir-bridge
  STAGE-1 (self-hosted) : own-x86-64-backend replaces Cranelift
  STAGE-2 (LoA migration) : Odin-LoA-v10 → CSSLv3-LoA-v11
  STAGE-3 (C99-reproducibility R16) : ∀ version emits self-compiling C99-tarball
    ≡ third-party-can-verify consent-encoded · ¬ binary-blob-cope

# ══════════════════════════════════════════════════════════════════
§ 4 · SUBSTRATE-PRIMITIVES per-version
# ══════════════════════════════════════════════════════════════════

§ CSSL-v1 PRIMITIVES (notation-tier)
  ⊢ § (section) · I> (insight) · W! (warning) · R (rule) · ✓◐○✗ (status)
  ⊢ → ← ↔ ⇒ (flow-glyphs) · ├─ └─ │ (tree) · ∂∇∆Σ∫ (math) · ─── (separator)
  ⊢ ENTRY (key:value) · RULE (R<n> "text") · INSIGHT (I> text) · WARNING (W! text)
  ⊢ DEAD/CHOICE/ALT (alternative-tracking)
  ⊢ DOMAIN-EXTENSIONS (define-in-§0) : engine-glyphs · ROM-glyphs · biz-glyphs
  ¬ types · ¬ memory · ¬ runtime

§ CSSL2 PRIMITIVES (language-tier)
  TYPES (phase-0 + 1a)
    bool · i8/i32/i64 · u8/u32/u64 · f32/f64 · string
    user-struct · struct-literal `Point { x: 1.0, y: 2.0 }`
    refined-scalar-aliases : `type Probability = f32 | 0.0..=1.0`
    `type CellIndex = u32 | 0..100`
    vec3 (Cl<3,0,0>.Vector subset · phase-2a)
  STATEMENTS
    let · let-mut (1a) · const · return · while · for-i-in-a..b/..= · break · continue
    expr-statements · NO-semicolons (newline-separates)
  EXPRESSIONS
    arithmetic +-*/% · comparison ==!=<>≤≥ · logical &&||!
    if-else (expression) · match · short-form `fn f(x) = expr`
    field-access · struct-literals · implicit-return (final-expr-of-block)
  FUNCTION-ANNOTATIONS (1a)
    @pure · @gpu_kernel · @audio_callback · @inline
    @sdf_lip(K) · @require_sdf_lip_le(K)
    `fn f() with { Alloc, IO } { ... }`
  EFFECT-LATTICE (1a)
    Pure · Alloc · ArenaAlloc · NoAlloc
    GPU · Audio · Render · Worker · CPU
    StorageRead · StorageWrite · UniformRead · TextureRead · TextureWrite
    NoLock · NoBlock · NoRecurse · Deadline<Tms>
    Frame · Scene · Permanent
    SoA · AoS (layout)
    representation : 64-bit bitmask · O(1) subset-check
  LIPSCHITZ (1a · attribute-form)
    Lipschitz_Bound :: distinct-f64
    composition-rules : min/max/smin/scale/translate/rotate/twist/bend/displacement
    primitive-table : sd_sphere/box/torus/capsule/plane/cylinder ≡ sdf<lip=1.0>
  COMPTIME (1b)
    `comptime let X = ...` (compile-time-resolved)
    @unroll for-loops · @if conditional-compilation · @inline
    recursion-cap-256 · time-budget-10s · pure-only
  AUTODIFF (1b · scalar)
    forward-mode dual-numbers · single-f32-param fns
    primitive-gradient-table : +-*/ · sin/cos/sqrt/exp/log/pow · max/min/abs
    chain-rule at-call-sites · per-arm-gradient on-branches
    @grad_override for-discontinuous-ops (step/floor/ceil)
  VEC3 (2a)
    `vec3 { x: f32, y: f32, z: f32 }` (struct-of-3-floats · SIMD-friendly)
    operators : + - * (scalar) · dot · cross · length · normalize (zero-guarded)
    layout : 12B SoA-friendly ; rotor3 → 16B ; motor → 32B (planned)
    SIMD-codegen : verified-via clang -O2 (struct-of-3 OK · <3 x float>-vector deferred)
  SCENE-DSL (2a-smoke · 2c-full)
    `scene "name" { composition = <expr> }` → fn name_sdf(p: vec3) -> f32

§ CSSLv3 PRIMITIVES (substrate-tier · expanded)
  ⊕ INHERITS-ALL CSSL2 F1..F4 (autodiff · refinement · effects · staged)
  ⊕ ELEVATES F5 + F6 to-non-negotiable :
    F5 IFC (information-flow-control · Jif-DLM-style)
    F6 OBSERVABILITY ({Telemetry<scope>} effect · Level-Zero sysman · audit-chain)
  ⊕ NEW-PRIMITIVES (¬ in-CSSL2)
    Pony-6-capabilities : iso · trn · ref · val · box · tag (¬ Austral-3)
    Vale-gen-refs (8-byte-packed-u64 · ref-capability encoding)
    Handle<T> primitive (generational · stale-ref-detection)
    Jet<T,N> higher-order-AD-native
    @staged + #run comptime (Futamura P1+P2+P3 self-applicable)
    Racket-hygienic macros UNIFIED-WITH-staging
    SMT-backed verification ({Verify<method>} · Z3+CVC5)
    @layout(std140|std430|cpu|packed) refinement
    {Privilege<level>} · {Audit<dom>} · @sensitive(domain) · IFC-labels
    {DetRNG} · {PureDet} (replay-determinism)
    {Power<W>} · {Thermal<C>} · {Realtime<p>} (hardware-bounds)
    orthogonal-persistence (Pharo-class hot-reload · per-spec-18)
    Σ-mask runtime (per-spec-27) · Σ-Chain bootstrap (per-spec-28) ← post-CSSL2 substrate-novelty
  ⊕ DUAL-SURFACE
    CSLv3-surface (primary · density=sovereignty)
    Rust-hybrid bridge (interop)
  ⊕ MULTI-BACKEND (CSSL2 had-2 · CSSLv3 has-≥4)
    SPIR-V · DXIL · MSL · WGSL (GPU)
    x86-64 own-backend (CPU · stage-1)
    C99-tarball (R16 reproducibility-anchor)

# ══════════════════════════════════════════════════════════════════
§ 5 · FEATURES-RETAINED + DISCARDED + TRANSFORMED (v2 → v3)
# ══════════════════════════════════════════════════════════════════

§ RETAINED (CSSL2 → CSSLv3 · direct-evolution)
  ✓ AUTODIFF (F1)
    CSSL2 : @differentiable fn · forward-mode-scalar (1b) + vec3-deferred-2b
    CSSLv3: same-spirit + Slang.D-surface ([Differentiable]+fwd_diff+bwd_diff+IDifferentiable)
            + Jet<T,N> higher-order native + inverse-rendering/fluids stdlib
  ✓ REFINEMENT (F2)
    CSSL2 : range-types (1a) · @sdf_lip+@require_sdf_lip_le (1a) · typed-sdf<lip=K> (2)
    CSSLv3: LiquidHaskell {v:T | P(v)} + tagged-suffix T'tag
            + SMT-backed (Z3+CVC5) via {Verify<method>}
            + Lipschitz-proofs compile-time
            + layout-refinement @layout(std140|std430|cpu|packed)
  ✓ EFFECTS (F3)
    CSSL2 : 20-tag-bitmask · annotation-based · pragmatic ¬ algebraic-effects-full
    CSSLv3: KOKA row-polymorphic ⟨e₁ … | μ⟩ + Xie-Leijen evidence-passing→plain-C
            + 28+ built-in effects (resource/time/power/thermal/backend/telemetry/audit/privilege)
            + linear×handler full-integration (Eio-style one-shot)
  ✓ STAGED-COMPUTATION (F4)
    CSSL2 : Zig-like comptime + Halide-inspired schedules (deferred 1b/2c)
    CSSLv3: @staged + #run · Futamura P1+P2+P3 self-applicable · macros-unified-w/-staging
            + shader-permutations + DSL-compilers emerge-for-free
  ✓ ANTIBODIES (F5+F6 in-CSSL2 · re-coded in-CSSLv3)
    CSSL2 : runtime-antibodies (spec-14) + code-antibodies (spec-15) ¬ shipped
    CSSLv3: code-antibody-spirit lifted INTO type-system itself
            (¬ "rules-bolted-on" · "patterns-impossible-by-construction")
            runtime-antibodies → IFC-labels + privilege-effects (spec-11+12)
  ✓ CSL-NOTATION-DISCIPLINE
    v1→v2 : CSL applied-to specs/handoffs/commits (spec-09 §3)
    v2→v3 : CSLv3 = primary-surface · NOT-just-specs · code-itself reads-CSL-dense
  ✓ BUG-DRIVEN-DESIGN
    v2 : every-feature ties-to-V11/V12/DGI bug
    v3 : CSSLv3 manifesto §BUG-PATTERN→COMPILER-FEATURE table
         (12+ patterns · 1-to-1 feature-mapping)
  ✓ SELF-HOSTING-AMBITION
    v2 : phase-3 visionary (deferred-rewrite-compiler-in-CSSL2)
    v3 : staged-bootstrap MANDATORY (stage-0 Rust→stage-1 self-host @ ≤2-weeks)
  ✓ TEXTUAL-IR-EMISSION-DEFAULT (per L2)
    v2 : LLVM .ll · SPIR-V .spvasm
    v3 : SPIR-V via-rspirv · Cranelift CPU · MLIR-dialect for-stage-1+
  ✓ DEPENDENCY-AUDIT-DISCIPLINE (per L1+L8)
    v2 : every spec §3 PHASE-N "DEPENDS ON" subsection
    v3 : same-pattern · DECISIONS.md cross-crate log · pinned-toolchain
  ✓ PRIME-DIRECTIVE
    v1+v2+v3 : R0 "PRIME_DIRECTIVE applies" inviolable across-all-three
    v3-elevation : encoded-STRUCTURALLY in-IFC-labels ¬ just-policy
                   "violation = type-error ¬ policy-breach · no-override-exists"

§ DISCARDED (v2 → v3)
  ✗ ODIN host-language
    rationale : CSSLv3 chose-Rust + Cranelift-stage0 (better-ecosystem · safer-FFI · Cargo-workspace-multi-crate)
    ¬ CSSL2-mistake · Odin-was-right-for-V11/V12-context · Rust-better for-31-crate-workspace
  ✗ HM-INFERENCE (rejected-already-in-CSSL2 from Sigilv2 critique)
    v3 confirms : structured-MIR + bidirectional-typing > λ-calc-HM
  ✗ "EMIT-C-AS-PRIMARY" (v3-keeps-it as-R16-reproducibility-only)
    v2-decision : LLVM-textual default · C-fallback
    v3-decision : same-default + C99-tarball as-stage-3 audit-anchor (¬ primary-path)
  ✗ AUSTRAL-3-CAPABILITIES
    v3 explicit : "Austral-3 insufficient for-multi-fiber-scene with-shared-state"
    v3 chose Pony-6 + Vale-gen-refs ← ¬ in-CSSL2-spec-set
  ✗ "PHASE-3-VISIONARY-MAYBE" hedging
    v2-spec-11 §6 listed phase-3 as-emergent + tentative
    v3 collapses : self-host = stage-1 MANDATORY · vertical-slice ≤4-weeks
  ✗ "NO-SMT-IN-PHASE-1" (v2-spec-03 R4)
    rationale-v2 : SMT slow + 50MB-dep + non-decidable
    rationale-v3 : SMT NOW non-negotiable F2 ({Verify<method>} · Z3+CVC5 mandatory)
                   ← v3 has-the-team-budget-to-handle-SMT-cost · v2 didn't
  ✗ MLIR-AS-PHASE-3+ (v2-spec-08 deferred-MLIR per-cost-100MB-dep)
    v3 promotes MLIR to-spec-15 + integrates-stage-0 via-mlir-sys+melior
    same-rationale as-SMT : v3 dependency-budget allows-it
  ✗ "SOLO-DEV-CONSTRAINT" framing
    v2 frequently invoked solo-dev as-justification-for-deferral
    v3 ¬ uses-this-frame · "Phase-A : spec-corpus complete (this chat) · Phase-B : Claude-Code handoff"
       ≡ "vision is-the-constraint · time isn't" lifted-into-architecture

§ TRANSFORMED (v2-form → v3-form)
  Δ DIMENSIONAL-TYPES
    v2 : f32<m> · vec3<m,world,InGrid> (multi-axis-annotation-cascade)
    v3 : same-cascade + tagged-suffix T'tag · richer-IFC-label-composition
  Δ EFFECT-INFERENCE
    v2 : annotation+inference ; bitmask 64-bit ; pragmatic-tier-only
    v3 : Koka-row-polymorphic ; effect-VARIABLES (ε) at-fn-bounds ; full-handlers
  Δ SCENE-DSL (F4)
    v2 : `scene "name" { entity { sdf=... } composition=... }` → fn
    v3 : @staged + #run · DSL-compilers emerge-for-free via-Futamura-P3
  Δ ANTIBODIES (F5+F6)
    v2 : SEPARATE specs-14+15 + 5-layer immune-architecture + Bloom-filter+gossip
    v3 : ABSORBED-into IFC + capability + verify-effect (no-separate-immune-system needed
         ∵ patterns-impossible-by-construction supersedes-runtime-detection)
  Δ TARGET-MULTIPLICITY
    v2 : LLVM (CPU) + SPIR-V (GPU) + C-fallback ≡ 3-targets
    v3 : Cranelift+x86-own + SPIR-V+DXIL+MSL+WGSL + C99-tarball ≡ 7-targets
  Δ PHASE-LETTER-DISCIPLINE (per-L5)
    v2 : phases-subdivided-into-letters-N/a/b/c · ◐⊘ partial-status-allowed
    v3 : per-T-task gates (T1..T11+) · same-spirit · finer-grain
  Δ POSTMORTEM-TEMPLATE (per-L10)
    v2 : spec-13 §3 agent-postmortem template (LOC · friction · proposed-L<n> · proposed-BS<id>)
    v3 : same-pattern · DECISIONS.md cross-crate-arch-log
  Δ BENCHMARK-TARGETS
    v2 : compile <50ms-file · <3s-50K-project · <500ms-incremental
    v3 : same-targets + per-perf-baseline-corpus + golden-fixture-corpus + R18-telemetry
  Δ OPS-VOCABULARY
    v2 : @differentiable · @sdf_lip(K) · @require_sdf_lip_le(K) · @inline · @unroll · @if · @guard
    v3 : full-syntax-table @ spec-09 (CSSLv3) · adds @sensitive · @layout · #run · others

# ══════════════════════════════════════════════════════════════════
§ 6 · CROSS-VERSION GROUNDING-DISCIPLINE
# ══════════════════════════════════════════════════════════════════

§ CSSL2-grounding (per-spec-12 §5)
  every-spec ties-to-real-bug
    spec-01 ← V11 SDF-normal perf-collapse (182→10 fps · 6 central-diff)
    spec-02 ← V11 audio-alloc · GPU-recursion · frame-leak · MCP-block · thermal-deadlock
    spec-03 ← V11 text-glyph march-crash (lip=1.8 · 6hr debug)
    spec-04 ← V12 hand-authored-scene + no-BVH (every-ray-evals-all)
    spec-05 ← V11 cosmetic-MvCell-naming (no-actual-GA-ops)
    spec-06 ← V12 86M-bounds-checks/frame + V10 read/write-phase-confusion
    spec-07 ← V11 LBM-flat-array-indexing + 3×3-stress-manual-indexing
    spec-08 ← "emit-C-cowardice" + WGSL-hop info-loss
    spec-14 ← prompt-injection · jailbreak · adversarial-input
    spec-15 ← bug-recurrence-across-files/contributors

§ CSSLv3-grounding-table (manifesto §BUG-PATTERN → COMPILER-FEATURE)
  central-diff-perf-cliff   → F1 autodiff
  CPU/GPU-sdf-divergence    → F3 effect-rows {GPU}|{CPU}|{}-pure
  thin-SDF-raymarch-crash   → F2 refinement SDF'L<k≤1>
  audio-callback-alloc      → F3 {NoAlloc} + {Deadline<N>}
  GPU-struct-padding        → F2 @layout(std140|std430|cpu)
  hot-loop-deadline-miss    → F3 {Deadline<N>} + {Realtime<p>}
  stale-entity-refs         → Handle<T> generational-packed-u64
  replay-nondeterminism     → F3 {DetRNG} + {PureDet}
  power-regression          → F3 {Power<W>} + R18-telemetry
  thermal-throttle          → F3 {Thermal<C>} + sysman-enforcement
  harm-enabling-effect      → IFC-label + {Privilege<l>} + compiler-refusal
  unreproducible-build      → C99-anchor from-stage0 + signed-audit
  untracked-performance     → {Telemetry<scope>} + oracle-tests

§ COMMON-RULES across-v1+v2+v3
  R0 PRIME-DIRECTIVE applies
  Rn density=sovereignty (CSL/CSLv3 notation)
  Rn self-hosting (v1 spec-in-CSL · v2 specs-in-CSL · v3 ditto + bootstrap-self-host)
  Rn ¬ academic-indulgence · grounding-test-passes
  Rn append-only learnings-log (spec-13 → spec-13-v3-equivalent)
  Rn dependency-audit before-implementation (per-L1+L8)
  Rn negative-tests ≈20% per-phase (per-L3)
  Rn ◐⊘ honest-partial-shipping ¬ fake-completeness (per-L8)

§ INFINITY-ENGINE TAKE-AWAY
  Infinity-Engine builds-on : CSSLv3 ≡ post-CSSL2-distillation
                            : CSSLv3 absorbs CSSL2's-six-pillars + elevates-IFC + adds-Pony-6/Vale + Futamura-P3
  Infinity-Engine ¬ should-revisit : Sigilv2-textbook-rejection · CSSL2-bug-driven-discipline · CSSL-notation-bootstrap
  Infinity-Engine W! preserve : grounding-discipline · honest-partial-shipping · density=sovereignty · PRIME-DIRECTIVE-structurally-encoded
  Infinity-Engine W! avoid    : "emit-C-cowardice" · HM-on-imperative · academic-indulgence · hedging-without-evidence

# ══════════════════════════════════════════════════════════════════
§ END · 13_CSSL_LINEAGE_DISTILLATION
# ══════════════════════════════════════════════════════════════════
