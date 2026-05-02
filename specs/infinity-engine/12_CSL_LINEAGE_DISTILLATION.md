§ CSL-LINEAGE DISTILLATION  
I> spelunk-result : CSLv2 (notation-only) + CSLv3 (notation + ref-compiler) → Infinity-Engine intake  
I> ¬ history-paper · IS design-extraction · what-influences-IE-substrate  
I> sources : `~/source/repos/CSLv2/` + `~/source/repos/CSLv3/specs/`  
I> last-CSLv3-release @ v1.7.0 (2026-04-18) · 16 spec-files frozen v1.0  
I> read-only ; this-doc ¬ commit-yet  

---

## § 0 SCOPE-OF-DOC

  W14-C task : extract design-decisions that-should-inform Infinity-Engine  
  N! conflate CSL-line ⊕ CSSL · CSL = notation · CSSL = sigil-substrate-language (separate)  
  N! port CSL-grammar wholesale into IE · IE = engine-runtime ¬ spec-language  
  R! IE inherits glyph-vocabulary + density-discipline + slot-grammar-philosophy  
  R! IE discards : CSL-PEG-parser, BPE-cost-table, EN↔CSL bridge-tooling  

---

## § 1 CSLv2-LANGUAGE  

I> 2026-04 era · "Caveman Spec Language v2" · ultra-dense spec notation  
I> triple-scope : (a) human↔AI spec R/W (b) agent-CoT substrate (c) CSSL2-compiler input  
I> design-axiom : compression_ratio ≡ freed_cognition  
I> ancestry : CSLv1-ad-hoc + APL + Ithkuil + Sanskrit + Peirce + de-Bruijn + Lojban  

### §§ 1.1 SYNTAX-MODEL

  slot-template : `[GATE?] [DET?][SUBJECT] [RELATION] [OBJECT] [CONSTRAINT*] [META?]`  
  defaults-silent : (Ithkuil-principle) most-common-value never-written  
  block-structure : `colon + indent` = scope-introduction (Python-like)  
  positional-ref  : `$0 $1 $2` for de-Bruijn-within-scope (≤ 3 levels)  
  formal-grammar  : PEG (single-parse-per-input · Lojban-rule)  

### §§ 1.2 KEY-FEATURES

  · 4 compound-classes : tatpurusha (`.`) + dvandva (`+`) + karmadhāraya (`-`) + avyayībhāva (`@`)  
  · Egyptian-determinatives : `§ ∫ ⊞ ⟨⟩ ⌈⌉ ⟦⟧ «» ⟪⟫` · domain-classifiers · 0-semantic-load  
  · Peircean-linearization : indent = cut-depth = negation-scope · adjacency = AND  
  · type-system : f32/f64/i32/i64/u8-u64/bool · `[T;N]` `T?` `T!` `(T,U)` `{K:V}` `&T` `*T`  
  · meta-tracking : evidence-glyphs `✓◐○✗⚠` + confidence `‼⁈⁇†‡`  
  · annotation : `?` (TBD) · `!` (must) · `⚠` (warn) · `‼` (proven-immutable)  

### §§ 1.3 PROJECTED-COMPRESSION

  · simple-constraint     : EN 15-20 tok → CSLv2 3-4 tok ≈ 5×  
  · struct-definition     : EN 50-80 tok → CSLv2 10-15 tok ≈ 5×  
  · algorithm-spec        : EN 200-400 tok → CSLv2 40-75 tok ≈ 5×  
  · 10K-word-system-spec : EN 13K tok → CSLv2 2-2.5K tok · 128K-ctx → 640-768K equiv  
  · PRIME-DIRECTIVE encoding : EN ~100 words → CSLv2 7 lines ~30 tok ≈ 10×  

### §§ 1.4 CSLv2-WEAKNESSES (admitted in v3 reconciliation-log)

  · 3 divergent v2-sources existed (root + pasted + specs/) · glyph-tables-disagreed  
  · 4-vs-5 compound-types · ‼ vs ⊘ for proven · `°`-vs-`@` for avyayībhāva  
  · BPE-tokenizer-cost ¬ measured · some-glyphs cost 3-5 BPE-tokens  
  · ASCII-fallback ¬ specified · cross-platform / cross-tokenizer fragile  
  · grammar-was-PEG-only · LL(2) preferred for hand-rolled-parser  

---

## § 2 CSLv3-LANGUAGE

I> 2026-04-16 freeze · v1.0.0 · 16 spec-files in `specs/`  
I> reconciliation of 3-divergent-CSLv2-sources → unified canonical-spec  
I> ¬ "v3 = v2 + features" · v3 = "v2-fork-resolution" + ASCII-mandate + ref-compiler  

### §§ 2.1 DESIGN-AXIOMS (from `00_MANIFEST.csl`)

  A0 : density = sovereignty · token-saved = reasoning-reclaimed  
  A1 : position IS meaning · slot-grammar · nesting = scope  
  A2 : juxtaposition = conjunction · co-present items AND'd  
  A3 : zero-distortion · implementation-relevant info preserved  
  A4 : tokenizer-aware · optimize-for-BPE token-count  
  A5 : LL(2) parseable · unambiguous · 2-token-lookahead-max  
  A6 : self-specifying · CSLv3 grammar expressed in CSLv3  
  A7 : trilingual · human-readable + AI-reasoning-ready + compiler-ingestible  
  A8 : dual-glyph · every unicode-glyph W! have ASCII-alias (J-rule)  

### §§ 2.2 SYNTAX-MODEL · slot-grammar

  template :  
    `[EVIDENCE?] [MODAL?] [DET?] SUBJECT [RELATION] OBJECT [GATE?] [SCOPE?] [META?]`  
  
  | slot     | pos | required | default            | content                   |  
  |----------|-----|----------|--------------------|---------------------------|  
  | EVIDENCE | 0   | ○        | ✓ (confirmed)      | ✓ ◐ ○ ✗ ⊘ △ ▽ ‼          |  
  | MODAL    | 1   | ○        | (assertion)        | W! R! M? N! I> Q? P> D>   |  
  | DET      | 2   | ○        | (none)             | § ∫ ⊞ + type-suffix       |  
  | SUBJECT  | 3   | ✓        | —                  | compound-name             |  
  | RELATION | 4   | ✓        | : (is/has)         | : :: = → ← ↔ ∈ ⊂ ≡       |  
  | OBJECT   | 5   | ✓        | —                  | value/type/target         |  
  | GATE     | 6   | ○        | (unconditional)    | if/when/unless + expr     |  
  | SCOPE    | 7   | ○        | (global)           | @frame / @chunk / per     |  
  | META     | 8   | ○        | (none)             | # comment-to-EOL          |  

### §§ 2.3 GLYPH-INVENTORY · 74 entries (canonical)

  · tier-0 W! memorize : structural (§ → ← ⇒ ⊢ ∴ ∵ ∎) · modal (W! R! M? N! I>) · evidence (✓◐○✗⊘△▽‼)  
  · tier-1 R! learn-as-needed : domain-determinatives (§ ∫ ⊞ ⟨⟩ ⌈⌉ ⟦⟧ «» ⟪⟫) · type-suffixes (`'d'f's't'e'm'p'g'r`) · APL-ops (`/ \ ¨ ⍟ ⍋ ⍳`)  
  · tier-2 M? LoA-specific : physics (ρ μ σ κ ε λ τ) · diff (∇ ∂ Δ)  

### §§ 2.4 COMPOUND-FORMATION · 5 types

  TP (tatpurusha)    : `A.B`   = B-of-A    · head-final · most-common-default  
  DV (dvandva)        : `A+B`   = {A,B} co-equal  
  KD (karmadhāraya)  : `A-B`   = B-that-is-A · attributive  
  BV (bahuvrihi)      : `A⊗B`   = thing-having-A+B · exocentric  
  AV (avyayībhāva)   : `@X`    = at/per/in-scope-of-X  

### §§ 2.5 MORPHEME-STACKING · Ithkuil-lite

  template : `BASE[.aspect][.modality][.certainty][.scope]`  
  · aspect    : prog/perf/iter/hab/inch/term  
  · modality  : must/may/cant/will/wont  
  · certainty : cert/prob/poss/doubt  
  · scope     : loc/glob/ctx  
  · max-stack-depth = 4 morphemes  
  · ex : `spawn.iter.may.loc` = "may-repeatedly-spawn-locally"  

### §§ 2.6 TYPE-SYSTEM · layered

  primitives  : u8..u64 · i8..i64 · f32 · f64 · bool · str  
  spatial     : vec2 · vec3 · vec4 · mat4 · quat · rgba  
  compound    : `[T;N]` · `[T;_]` · `T|U` · `T?` · `T!` · `(T,U,V)` · `{K:V}` · `&T` · `*T`  
  algebraic   : sum (`T₁ | T₂`) · product `⟨f₁:T₁⟩` · enum-with-substructure  
  dependent   : `Π⟨x:T⟩→U(x)` · `Σ⟨x:T, y:U(x)⟩` (Agda-lineage)  
  refinement  : `{ x:T ⊢ P(x) }` · subset-types  
  linear/affine : `T!lin` (must-use-once) · `T&` (shared-borrow) · `T&mut` (exclusive)  
  rank-poly   : auto-lift across ranks (APL/Futhark-lineage)  

### §§ 2.7 REFERENCE-COMPILER · `08_COMPILER.csl`

  · pipeline : `lexer → parser → semantic → typecheck → IR-lowering → x86-64 ∨ SPIR-V`  
  · IR : SSA + regions (MLIR-inspired)  
  · target₁ : x86-64 (SysV-ABI · AVX2-min · N! AVX-512)  
  · target₂ : SPIR-V (Vulkan 1.3 · compute+graphics)  
  · impl : `parser/parser.exe` · Odin (stdlib-only)  
  · status @ v1.7.0 : ✓ lexer ✓ parser ◐ semantic ○ typechecker ○ IR ○ codegen  
  · cssllint subcommand : JSON-diagnostics · `CSL-E-LEX` `CSL-E-PARSE` `CSL-W-001..007`  

### §§ 2.8 SELF-DESCRIPTION · `13_GRAMMAR_SELF.csl`

  · CSLv3 grammar expressed in CSLv3-itself  
  · invariant : `parser(read("13_GRAMMAR_SELF.csl")) → valid-AST`  
  · round-trip : `AST → pprint → reparse → AST-shape-equal`  
  · 40% token-count of `02_GRAMMAR.csl` (which uses BNF + prose)  

### §§ 2.9 BPE-TOKENIZER-DISCIPLINE · `12_TOKENIZER.csl`

  · 74-entry ASCII-alias master-table · canonical fallback-form  
  · cost-buckets : COST-1 (single BPE) · COST-2 (acceptable) · COST-3+ (W! ASCII-alias)  
  · ROI-formula : `(frequency × semantic-bits) / BPE-token-cost`  
  · W! glyph w/ ROI < 1.0 → candidate-for-removal  
  · decision-procedure : if cost ≥ 3 BPE-tokens → W! ASCII-alias  

---

## § 3 EVOLUTION v2 → v3

### §§ 3.1 RECONCILIATIONS (3-source-merge)

  · glyph-table : merged 3 divergent tables → unified-with-ASCII-aliases  
  · compounds : 4-types → 5-types (+ Karmadhāraya from root)  
  · compound-ops : `.` `+` `-` `⊗` `@` (was inconsistent : `°` for AV → `@` won)  
  · evidence : merged ✓◐○✗ + ⊘△▽ + ‼ → unified 8-marker system  
  · type-suffixes : adopted `'d'f's't'e'm'p'g'r` (ASCII-determinatives)  
  · domain-determinatives : adopted §∫⊞⟨⟩⌈⌉⟦⟧«»⟪⟫ (fullest-set)  
  · slot-grammar : formalized to 9-slot template (was 6)  
  · type-system : elevated to its-own-spec-file (was scattered)  
  · tokenizer : elevated to its-own-spec-file (was §12 appendix)  
  · ref-compiler : full spec (was implicit-only)  
  · BNF : unified (3-sources-had PEG / BNF / CFG separately)  
  · arrows : `→` preferred · `->` ASCII-alias (was inconsistent)  

### §§ 3.2 ADDITIONS (v3 only)

  + ASCII-alias-mandate D8 · J-rule canonical · 74-entry-master-table  
  + Morpheme-stacking · Ithkuil-lite · 4-slot suffix-stack on any term  
  + Modal-extensions : P> (push-further) · D> (decision-needed) · TODO · FIXME  
  + Reasoning-substrate `05_REASON.csl` · `§P/§D/§T/§S/§C` think-blocks  
  + Self-description spec (`13_GRAMMAR_SELF.csl`) · grammar-expressed-in-itself  
  + EN↔CSL bridge spec (`09_BRIDGE.csl`) · transformation-rules · mixed-mode  
  + LL(2)-target replacing PEG · unambiguous-parse · `∄` ambiguous-parse  
  + Continuous-eval methodology (`10_EVAL.csl`) · m₁..m₆ metrics · stratified-targets  
  + cssllint JSON-diagnostic-protocol (Session-4-T19 · diag-namespace)  
  + LoRA-generalization-test infrastructure (Session-17 · `m2_finetune.py`)  

### §§ 3.3 DISCARDS (v2 had · v3 dropped)

  · `°` for avyayībhāva  
  · `>` for bahuvrihi (conflicted with comparison-op)  
  · de-Bruijn beyond 3-levels (human-readability degrades)  
  · pure-2D syntax (Befunge-style) · ¬ text-stream-friendly  
  · color-channel encoding (accessibility · medium-dependent)  
  · full-Ithkuil-morphology (unlearnable < 2hr)  
  · Peirce-Gamma modal-graphs (complexity > value)  

### §§ 3.4 PROJECTED-COMPRESSION (revised in v3)

  · stratified m₁ targets (per corpus-mode) :  
    pure-CSL ≤ 0.5 · bridge ≤ 0.9 · prose ≤ 1.10  
  · m₂ perplexity-Δ : W! < 10% (reasoning-preserved)  
  · m₅ round-trip-fidelity : R! > 0.95 semantic-similarity  
  · m₆ glyph-token-cost : W! ≤ 2 BPE-tokens-per-glyph  

---

## § 4 NOTATION-CONVENTIONS · canonical-vocabulary

I> from `01_GLYPHS.csl` + `12_TOKENIZER.csl` · 74-glyph master  
I> Apocky's CLAUDE.md inherits subset for cross-project sovereignty  

### §§ 4.1 STRUCTURAL · always-Tier-0

  | glyph | ASCII | meaning                               |  
  |-------|-------|---------------------------------------|  
  | §     | S:    | section / module / domain-boundary    |  
  | →     | ->    | flow / yields / maps-to               |  
  | ←     | <-    | from / sourced / derives              |  
  | ↔     | <->   | bidirectional / isomorphic            |  
  | ⇒     | =>    | logical-implies                       |  
  | ⊢     | \|-   | entails / proves                      |  
  | ∴     | .:.   | therefore                             |  
  | ∵     | :..   | because                               |  
  | ∎     | QED   | block-end / proof-end                 |  

### §§ 4.2 MODAL

  | glyph | meaning                                       |  
  |-------|-----------------------------------------------|  
  | W!    | MUST · hard-requirement · inviolable          |  
  | R!    | SHOULD · strong-recommend                     |  
  | M?    | MAY · designer-discretion                     |  
  | N!    | MUST-NOT · prohibition                        |  
  | I>    | INSIGHT · key-claim                           |  
  | Q?    | QUESTION · open                               |  
  | P>    | PUSH-FURTHER · deeper-grounding-requested     |  
  | D>    | DECISION-NEEDED                               |  
  | TODO  | unfinished                                    |  
  | FIXME | known-broken                                  |  

### §§ 4.3 EVIDENCE · 8-marker-unified-system

  ✓ confirmed · ◐ partial · ○ pending · ✗ failed · ⊘ unknown · △ hypothetical · ▽ deprecated · ‼ proven  

### §§ 4.4 RELATION

  `.` (of-tatpurusha) · `+` (and-dvandva) · `-` (that-is-karmadhāraya) · `⊗` (having-bahuvrihi) · `@` (at-avyayībhāva)  
  `:` (bind/has) · `::` (type-of/inherits) · `≡` (structurally-identical) · `≈` (approx-equal)  

### §§ 4.5 SET / LOGIC · ASCII-pairs canonical

  ∀ all · ∃ any · ∈ in · ∉ !in · ∪ \|+ · ∩ &+ · ⊂ <: · ⊃ :> · ¬ ~ · ∧ && · ∨ \|\| · ⊕ xor  

### §§ 4.6 ENCLOSURE · semantic-pairs

  `()` group · `[]` list/array · `{}` set/block · `⟨⟩` tuple/record · `⟦⟧` formula · `«»` quote · `⌈⌉` constraint/post · `⌊⌋` precond · `⟪⟫` temporal  

### §§ 4.7 PIPELINE / DATAFLOW

  `~>` causes/triggers · `|>` pipeline-fwd · `<|` pipeline-bwd · `>>` dispatch · `<<` receive  

### §§ 4.8 REASONING-EXTENSION (`05_REASON.csl`)

  `?!` surprising · `!?` suspect · `⟲` iterate · `⤓` dive-deeper · `⤒` lift-up · `⟐` pivot · `⚠` pitfall · `★` key-insight · `✎` note  

### §§ 4.9 GRAMMATICAL-RULES · Ithkuil-defaults-silent

  R0 : if most-common-value would-be-written, ¬ write-it  
  R1 : indentation = scope-boundary (Peircean cut linearized)  
  R2 : adjacent-items same-indent = AND'd (juxtaposition)  
  R3 : compound-nesting LTR · innermost-first  
  R4 : positional-ref `$0/$1/$2` for de-Bruijn-within-scope (≤ 3-deep)  
  R5 : glyph-prefix > enclosure > postfix-suffix > unary > access > arith > range > cmp > logic > flow > meta > conditional > sequence  

---

## § 5 SUBSTRATE-PRIMITIVES · declared-in-CSL-line

I> CSL is notation-only · ¬ runtime · primitives mean "spec-vocabulary"  
I> these influence what IE-substrate-must-be-able-to-express  

### §§ 5.1 DETERMINATIVES (categorical-classification)

  § system/module · ∫ field/continuous · ⊞ spatial/discrete · ⟨⟩ entity/property  
  ⌈⌉ constraint/bound · ⟦⟧ formula/equation · «» external/API · ⟪⟫ temporal/phase  

I> IE-substrate W! support ≥ these 8 categorical-namespaces  
I> ω-field-substrate ≡ CSL-`∫` (continuous-field) ✓ already-aligned  
I> Σ-mask-per-cell ≡ CSL-`⌈⌉` (cell-bounded-constraint) ✓ already-aligned  
I> KAN-substrate ≡ CSL-`⟦⟧` (formula/equation-bearing-region) ✓ already-aligned  

### §§ 5.2 TYPE-SUFFIXES (entity-classification)

  `'d` data · `'f` function · `'s` system · `'t` type · `'e` entity · `'m` material · `'p` property · `'g` gate · `'r` rule  

I> 9-way-classification · IE-substrate W! distinguish these for procgen-routing  
I> `'e` (entity) · `'m` (material) · `'p` (property) → core LoA-procgen-axes  

### §§ 5.3 EFFECT-SYSTEM · linear/affine

  `T!lin` must-use-exactly-once · `T&` shared-borrow · `T&mut` exclusive-borrow  

I> CSLv3 lifts Rust-borrow-checker to spec-level  
I> IE-substrate already-uses (cssl-substrate-effects · sovereign-bypass-RECORDED)  
I> CSL-line provides spec-level-vocabulary IE-runtime-already-implements  

### §§ 5.4 DEPENDENT-TYPES

  `Π⟨x:T⟩→U(x)` pi-type · `Σ⟨x:T,y:U(x)⟩` sigma-type · `{x:T ⊢ P(x)}` refinement  

I> IE-substrate W! preserve these through-procgen-pipeline  
I> ex : `Π⟨n:ɴ⟩ → ᴠ⟨f32; n⟩` = "fn returning vector whose length depends on input"  
I> ex : `{hp: u16 ⊢ hp ≤ max_hp}` = "bounded health"  

### §§ 5.5 RANK-POLYMORPHISM (APL)

  scalar / vector / rank-k · auto-broadcast · numpy-style  

I> IE-substrate already-uses for KAN/HDC/ω-field operations  
I> CSL-line provides notation IE can literally consume in `.csl` files  

---

## § 6 INHERITANCE-INTO-CSSLv3 vs DISCARDED

I> CSSLv3 = sigil-substrate-language · separate-from-CSL-notation  
I> CSSLv3 IS a programming language · CSL IS spec/CoT notation  
I> overlap-zone : CSL-glyphs embedded in CSSLv3 source-comments (CoT-IN-COMMENTS)  

### §§ 6.1 INHERITED INTO CSSLv3 (via `14_CSSLv3_BRIDGE.csl`)

  ✓ glyph-vocabulary (74-entry alias-table · shared-source-of-truth)  
  ✓ density-as-sovereignty principle (CSSLv3 SYNTHESIS_V2 CC8)  
  ✓ ASCII↔Unicode round-trip (csslfmt auto-upgrades on save)  
  ✓ CoT-in-comments (block-CoT `§{...§}` + line-CoT `§ I>`/`§ W!`/`§ N!`)  
  ✓ cssllint JSON-diagnostic-protocol (CSL-E-* / CSL-W-* / CSL-I-*)  
  ✓ effect-system foundations (linear/affine + borrow-discipline)  
  ✓ rank-polymorphism (preserved through-CSSLv3-IR)  
  ✓ refinement-types `{x:T ⊢ P(x)}` (CSSLv3 refinement-spec)  
  ✓ slot-grammar discipline (silent-defaults · 9-slot-template)  
  ✓ Egyptian-determinatives (per-domain namespace-organization)  

### §§ 6.2 DISCARDED FROM CSL-LINE

  N! CSL-PEG-parser (CSSLv3 has its-own LL(2) parser-pipeline)  
  N! CSL `08_COMPILER.csl` reference-pipeline (CSSLv3 uses cssl-csslc / cssl-mlir-bridge)  
  N! BPE-cost-table as runtime-concern (BPE was-LLM-tokenizer-concern only)  
  N! EN↔CSL bridge-tooling at-runtime (CoT-only · ¬ general-translation)  
  N! `09_BRIDGE.csl` mixed-mode-prose (CSSLv3 source = pure-CSSL · CoT = pure-CSL)  
  N! CSL-self-description (`13_GRAMMAR_SELF.csl`) as IE-runtime-feature  
  N! m₁..m₆ eval-methodology as IE-runtime-concern (was for CSL-corpus-validation)  
  N! domain-extension Tier-2 (ρ μ σ κ ε λ τ) as built-in IE-glyphs  
    · IE-substrate W! provide these as user-extensible · ¬ hard-coded  

### §§ 6.3 FORWARD-INFLUENCE INTO INFINITY-ENGINE

I> CSL-line provides VOCABULARY · IE provides RUNTIME-IMPLEMENTATION  
I> alignment-points already-existing in IE-substrate :  

  · ω-field-as-truth ≡ CSL-`∫`-continuous-field-determinative  
  · Σ-mask-per-cell ≡ CSL-`⌈⌉`-cell-bounded-constraint  
  · KAN-substrate ≡ CSL-`⟦⟧`-formula-bearing-region  
  · effect-system ≡ CSL-linear/affine-T!lin  
  · refinement-runtime ≡ CSL-`{x:T ⊢ P(x)}`  
  · 6-novelty-paths ≡ CSL-rank-polymorphism + APL-adverbs (`/ \ ¨`)  

  W! IE-substrate-runtime W! parse `.csl`-source-files (LoA-content authoring)  
  W! IE-substrate W! preserve dependent/refinement/linear types through procgen  
  W! IE-substrate W! emit CoT-in-CSLv3 for runtime-DM/GM/Collaborator/Coder reasoning  
  R! IE-substrate W! treat CSL-determinatives as namespace-organization-axes  
  R! IE-substrate W! support 5-compound-types as procgen-composition-rules  

### §§ 6.4 TOP-3 DESIGN-FEATURES THAT-SHOULD-INFLUENCE INFINITY-ENGINE

  1. **Slot-grammar with silent-defaults (Ithkuil-principle)**  
     IE-procgen-pipeline W! follow same-discipline · only-encode-the-unpredictable  
     procgen-output W! omit defaults · author-side-`.csl` W! omit defaults  
     ⇒ massive token-savings × fewer-failure-modes  

  2. **Egyptian-determinatives as namespace-organization**  
     `§` `∫` `⊞` `⟨⟩` `⌈⌉` `⟦⟧` `«»` `⟪⟫` already-map IE-substrate-axes  
     adopt-as-canonical · runtime-recognizes-domain via determinative-prefix  
     ⇒ procgen-routing automatic · zero-config-namespacing  

  3. **Linear-types + refinement-types preserved through-runtime**  
     CSL-spec-vocabulary already-encodes-effects-and-bounds  
     IE-substrate already-implements-runtime-equivalents  
     gap : `.csl`→IR pipeline must NOT lose these annotations  
     ⇒ correct-by-construction LoA-content procgen  

---

## § 7 NOTATION-GLYPH-COUNTS

  CSLv2 quick-reference card (per `CSLv2_SPEC.md` Appendix-A) :  
    GATES   : 12 (§ ! ? ⚠ → ← ↔ ∴ ∵ ⊢ ~ ¬)  
    STATE   : 6  (✓ ✗ ◐ ○ ● ⚠)  
    RELATE  : 11 (. + | : :: @ # * & ^ _ etc)  
    DOMAIN  : 8  (§ ∫ ⊞ ⟨⟩ ⌈⌉ ⟦⟧ «» ⟪⟫)  
    LOGIC   : 12 (∀ ∃ ∈ ⊂ ≡ ≈ ≥ ≤ ∞ ∅ ⊕ ⊗)  
    CONF    : 5  (‼ ⁈ ⁇ † ‡)  
    OPS     : 6  (/ \ ¨ ⍟ ⍋ ⍳)  
    TYPES   : 15 primitives + compounds  
    CSLv2 total ≈ 75 distinct glyph-symbols  
  
  CSLv3 master-table (per `01_GLYPHS.csl` + `12_TOKENIZER.csl`) :  
    Tier-0 structural : 9  (§ → ← ↔ ⇒ ⊢ ∴ ∵ ∎)  
    Tier-0 modal      : 10 (W! R! M? N! I> Q? P> D> TODO FIXME)  
    Tier-0 evidence   : 8  (✓ ◐ ○ ✗ ⊘ △ ▽ ‼)  
    Tier-0 relation   : 22 (. + - ⊗ @ : :: = ≠ .. ... | & ! ? # * ^ _ / % $ , ;)  
    Tier-0 set/logic  : 18 (∀ ∃ ∈ ∉ ∪ ∩ ⊂ ⊃ ¬ ∧ ∨ ⊕ ≡ ≈ ≥ ≤ ∞ ∅)  
    Tier-0 enclosure  : 9 pairs (() [] {} ⟨⟩ ⟦⟧ «» ⌈⌉ ⌊⌋ ⟪⟫)  
    Tier-1 determinatives : 8 (§ ∫ ⊞ ⟨⟩ ⌈⌉ ⟦⟧ «» ⟪⟫)  
    Tier-1 type-suffixes  : 9 ('d 'f 's 't 'e 'm 'p 'g 'r)  
    Tier-1 type-class     : 8 (:T ::K ʈ ɴ ꜱ ʙ ᴠ ᴍ)  
    Tier-1 temporal       : 6 (t→ t← t= t∞ Δ ∂)  
    Tier-1 pipeline       : 5 (~> |> <| >> <<)  
    Tier-1 APL-derived    : 6 (/ \ ¨ ⍟ ⍋ ⍳)  
    Tier-1 reasoning      : 9 (?! !? ⟲ ⤓ ⤒ ⟐ ⚠ ★ ✎)  
    Tier-2 physics        : 10 (ρ μ σ κ ε λ τ ∇ ∂ Δ)  
    CSLv3 total ≈ 137 distinct glyph-symbols (all w/ ASCII-aliases)  

  ratio : CSLv3 ≈ 1.83× CSLv2-glyph-count  
  ∵ : 3-source-merger + ASCII-alias-mandate + reasoning-substrate-tier  

---

## § 8 IMPLICATIONS-FOR-INFINITY-ENGINE

I> the why-this-doc-matters · actionable-extracts  

  1. IE-substrate already-has-vocabulary-aligned-with-CSLv3 · ¬ reinvent-glyphs  
  2. `.csl` source-file ingestion = already-supported-pattern · IE W! parse  
  3. CoT-in-CSLv3 = canonical-format for DM/GM/Collaborator/Coder reasoning-traces  
  4. ASCII-alias-mandate : every-glyph IE-emits W! have ASCII-fallback  
     ∵ IDE-friendly · grep-friendly · cross-tokenizer-portable  
  5. determinatives ≡ namespace-organization · IE-procgen W! respect  
  6. effect-system + refinement-types preserved through procgen-IR  
  7. silent-defaults principle in IE-spec-output (Ithkuil-rule)  
  8. ROI-discipline : every-IE-introduced-glyph W! justify-itself · prune-low-ROI  
  9. self-description : IE-spec W! be-expressible-in-IE-spec-language (reflexivity-test)  
  10. cssllint JSON-protocol = template for IE-content-validation-protocol  

  W! IE W! NOT re-implement CSL-grammar-parser · use existing `parser/parser.exe`-equivalent  
  W! IE W! treat CSL as "spec-and-CoT-notation" · ¬ "engine-runtime-language"  
  R! IE-content-authoring W! follow CSL-density-discipline as house-style  

---

## § 9 PROVENANCE

  files-read CSLv2 : `CLAUDE1.md` · `CLAUDE2.md` · `CSLv2_SPEC.md` · `CSLv2_RESEARCH_REPORT.md` · `PRIME_DIRECTIVE.md` · `RESEARCH_REPORT.md` · `compass_artifact_*.md` · `specs/CSLv2_SPEC.md` (8 files)  
  files-read CSLv3 : `README.md` · `CHANGELOG.md` · `MIGRATION_GUIDE.md` · `STABILITY.md` · `00_MANIFEST.csl` · `01_GLYPHS.csl` · `02_GRAMMAR.csl` · `03_MORPH.csl` · `04_SPATIAL.csl` · `05_REASON.csl` · `06_SPEC.csl` · `07_TYPESYS.csl` · `08_COMPILER.csl` · `09_BRIDGE.csl` · `10_EVAL.csl` · `11_RESEARCH.csl` · `12_TOKENIZER.csl` · `13_GRAMMAR_SELF.csl` · `14_CSSLv3_BRIDGE.csl` · `INDEX.csl` (20 files · 16-spec-suite + 4 top-level-docs)  
  
  read-only · ¬ modified · ¬ committed-yet  
  generated @ 2026-05-01 · W14-C · CSL-language-lineage-spelunker  

∎
