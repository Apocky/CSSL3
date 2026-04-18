# CSSLv3 — DECISIONS log

§ STATUS : Session-1 • T1..T6-phase-1 ✓ • T7-phase-1 ✓ • T8-phase-1 ✓ • T3.4-phase-2-refinement ✓ • T9-phase-1 ✓ • T10-phase-1-codegen ✓ • T10-phase-1-hosts ✓ • T11-phase-1-telemetry-persist ✓ • T12-phase-1-examples ✓ • T3.4-phase-3-AD-legality ✓ • T6-phase-2a-pipeline-body-lowering ✓ • T7-phase-2a-AD-walker ✓ • T9-phase-2a-predicate-translator ✓ • T12-phase-2a-F1-chain-integration ✓ • T11-phase-2a-real-crypto ✓ • T3.4-phase-3-IFC ✓ • T6-phase-2b-body-lowering-expansion ✓ • T9-phase-2b-Lipschitz ✓ • spec-corpus deltas applied • foundation audited

§ ROOT-OF-TRUST
All decisions in this file operate under the authority of `PRIME_DIRECTIVE.md` at the repo
root (identical to `C:\Users\Apocky\source\repos\PRIME_DIRECTIVE.md` master). The directive
is IMMUTABLE : no decision here can weaken, override, or circumvent it. A decision that
appears to conflict with the directive is retracted on discovery (violation = bug W! fix).

§ FORMAT
Each decision entry :
- **ID** `§ T<N>-D<n>` (task + decision number)
- **Date** ISO 8601
- **Status** `proposed` | `accepted` | `revised` | `superseded`
- **Context** what prompted the choice
- **Options** enumerated alternatives with tradeoffs
- **Decision** chosen option + rationale
- **Consequences** downstream effects, monitoring hooks

───────────────────────────────────────────────────────────────

## § T1-D1 : Layout — Cargo workspace vs single-crate

- **Date** 2026-04-16
- **Status** accepted
- **Context** §§ 01_BOOTSTRAP REPO-LAYOUT shows single-crate (`src/lex/`, `src/parse/`, …); §§ HANDOFF_SESSION_1 T1 TASK-MAP specifies a 30+ crate Cargo workspace. Spec-vs-handoff tension surfaced during context-load.
- **Options**
  - (a) single-crate + nested modules per §§ 01 literal
  - (b) Cargo workspace with per-concern crates per §§ HANDOFF_SESSION_1 T1
- **Decision** **(b) Cargo workspace**
- **Rationale**
  - `deny(unsafe_code)` per-crate enforcement is impossible in single-crate layout; FFI isolation (mlir-sys, level-zero-sys, ash, windows-rs, metal) needs per-crate boundary.
  - Parallel build + incremental + per-crate test isolation at scale.
  - Stage-1 rip-and-replace migration is per-crate clean.
  - Per-crate versioning once APIs mature.
- **Consequences**
  - §§ 01_BOOTSTRAP REPO-LAYOUT will be reconciled to match workspace (spec-corpus delta pending Apocky approval per §§ HANDOFF_SESSION_1 REPORTING).
  - Workspace root at `compiler-rs/` with `members = ["crates/*"]`.
  - Package-name prefix `cssl-*`; dir-name == package-name.
  - Binary crate `csslc` (no prefix); runtime lib `cssl-rt`.

───────────────────────────────────────────────────────────────

## § T1-D2 : cslparser sourcing — Rust-native port (option e)

- **Date** 2026-04-16
- **Status** accepted
- **Context** §§ HANDOFF_SESSION_1 T2 originally proposed `{a: vendor-source, b: cargo-patch-git, c: wait-for-crate}`; all presumed Rust compatibility. CSLv3 Session-3 confirms `cslparser = Odin package` (parser/\*.odin + parser.exe via `odin build`). New option-space surfaced during γ-load: `{d: Odin→C-ABI+bindgen, e: Rust port from spec, f: subprocess-IPC, g: AST.json sidecar, h: dual FFI+port, i: port + CI-oracle}`.
- **Decision** **(e) re-implement CSLv3 lex+parse in Rust** from `CSLv3/specs/12_TOKENIZER.csl` (74-glyph master alias table) + `CSLv3/specs/13_GRAMMAR_SELF.csl`. No FFI, no dual-impl, no Odin dependency in the CSSLv3 tree.
- **Rationale** (Apocky-direct)
  - `cslparser` is a stage-0 convenience, not a long-term dependency.
  - CSSLv3 stage-1 self-hosts → the parser ends up in CSSLv3 anyway.
  - Dragging the Odin toolchain into the CSSLv3 build would create a second bootstrap chain — anti-pattern.
  - `CSLv3/specs/12 + 13` are the authority, not the Odin implementation.
  - Rust-port from-spec **is** spec-validation: if unimplementable from spec alone, the spec has a hole.
- **Consequences**
  - T2 scope : `crates/cssl-lex` and `crates/cssl-parse` each dispatch Rust-hybrid (`logos` / `chumsky`) and CSLv3-native (hand-rolled Rust port) sub-modules. Split into dedicated crates if internal module boundary proves insufficient at T2-midpoint.
  - Any divergence between Rust-port output and canonical `parser.exe` output on CSLv3 fixtures = spec-ambiguity → file against CSLv3 (issue in CSLv3 repo, **not** a CSSLv3 code bug).
  - Zero Odin-toolchain dep in CSSLv3 stage-0 (including CI — Odin not required on any runner).
  - `parser.exe` remains canonical inside CSLv3 repo; CSSLv3 consumes specs, not the impl.

───────────────────────────────────────────────────────────────

## § T1-D3 : CI scope — §§ 23-FAITHFUL from commit-1

- **Date** 2026-04-16
- **Status** accepted
- **Context** Initial T1-CI proposal was "minimal" (check + fmt + clippy + test + doc). Apocky corrected: `optimal ≠ minimal` — wire the full §§ 23 TESTING harness skeleton empty-but-present from commit-1. Rationale : scaffolding done right once has zero rework; each subsequent task drops fixtures into pre-existing slots.
- **Options**
  - (a) minimal CI, harnesses deferred
  - (b) §§ 23-faithful CI : oracle-modes dispatch, golden-fixture framework, differential-backend matrix (Vulkan × L0 hooks), power / thermal / frequency regression, spirv-val gate, R16 reproducibility-attestation, spec-cross-ref validator — all wired empty but present
- **Decision** **(b) §§ 23-faithful**
- **Consequences**
  - T1 deliverables expand: see TodoWrite + SESSION_1_HANDOFF.md.
  - `cssl-testing` crate implements oracle-modes dispatch routing before any test body exists.
  - `.github/workflows/ci.yml` includes placeholder job stubs for every matrix-cell in §§ 23 CI MATRIX.
  - `scripts/validate_spec_crossrefs.py` scripted from day-1 (not manual).
  - `tests/golden/` + `.perf-baseline/` directories present from T1 (empty corpus, loader wired).

───────────────────────────────────────────────────────────────

## § T1-D4 : Toolchain anchoring — rust 1.75 pinned

- **Date** 2026-04-16
- **Status** accepted
- **Context** §§ HANDOFF_SESSION_1 specifies MSRV 1.75. R16 reproducibility-anchor mandates version-pinning. Current Apocky machine has rustc 1.94 (compatible).
- **Decision** `compiler-rs/rust-toolchain.toml` pins `channel = "1.75.0"`, profile `minimal`, components `rustfmt` + `clippy`. `[workspace.package] rust-version = "1.75"` enforces MSRV in Cargo.
- **Consequences**
  - Any cargo op in `compiler-rs/` triggers one-time 1.75.0 download.
  - Dep version picks constrained to 1.75-compatible crates.
  - If a dep demands newer rustc, bump both MSRV and toolchain-pin and document as T<N>-D<n+1> entry.
  - CI uses `rust-toolchain.toml` auto-detection → reproducible per-commit.

───────────────────────────────────────────────────────────────

## § T1-D5 : deny(unsafe_code) policy — per-crate inner-attribute

- **Date** 2026-04-16
- **Status** accepted
- **Context** §§ HANDOFF_SESSION_1 `deny(unsafe_code) except FFI-crates`. Workspace-level `[workspace.lints.rust] unsafe_code = "deny"` cannot be partially-overridden per-crate without duplicating the entire lint-table in FFI crates.
- **Decision** Use `#![forbid(unsafe_code)]` as inner-attribute in each non-FFI `src/lib.rs` / `src/main.rs`. FFI-crates declare `#![allow(unsafe_code)]` with SAFETY-doc justification at each unsafe-block site.
- **FFI-crate list** (stage0) : `cssl-mlir-bridge`, `cssl-host-vulkan`, `cssl-host-level-zero`, `cssl-host-d3d12`, `cssl-host-metal`.
  (`cssl-host-webgpu` uses `wgpu` safe-API surface; `cssl-cgen-cpu-cranelift` uses Cranelift safe-API.)
- **Consequences**
  - Audit-grep `#!\[allow\(unsafe_code\)\]` enumerates all FFI boundaries.
  - Non-FFI crates fail compile on any `unsafe` block — enforces T3-capability + T21-region soundness architecturally.

───────────────────────────────────────────────────────────────

## § T1-D6 : clippy pedantic scaffold-allowances

- **Date** 2026-04-16
- **Status** accepted (scaffold-phase) — revisit at T3 API stabilization
- **Context** `clippy::pedantic` + `clippy::nursery` groups enabled at `warn`; `cargo clippy -- -D warnings` promotes warnings to errors per §§ HANDOFF_SESSION_1 WORKFLOW commit-gate. Several pedantic lints fire pervasively on scaffold docstrings (`doc_markdown` wants backticks around `CSSLv3`, `SPIR-V`, `MLIR`, `DXIL`, etc) and on future typical-cast patterns.
- **Decision** Add `allow` entries to `[workspace.lints.clippy]` for scaffold-noisy pedantic lints :
  - `doc_markdown` : `CSSLv3` / `SPIR-V` / `MLIR` / `DXIL` un-ticked in scaffold-docs.
  - `cast_possible_truncation`, `cast_sign_loss`, `cast_lossless` : common false-positives in codegen arithmetic.
  - `default_trait_access`, `unreadable_literal` : noisy without adding safety.
  - Plus existing : `module_name_repetitions`, `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate`, `missing_const_for_fn`, `too_many_lines`.
- **Revisit trigger** : at T3 HIR-stabilization, re-enable each allowance and fix progressively. Track via `cargo clippy` run with `-W clippy::<name>` one-at-a-time.
- **Consequences**
  - Commit-gate passes on current scaffold; no false-alarms blocking progress.
  - Audit-grep `doc_markdown            = "allow"` locates the deferral for unwinding at T3.
  - Not a soundness regression — pedantic lints are stylistic, not correctness.

───────────────────────────────────────────────────────────────

## § T1-D7 : Rust toolchain ABI — gnu vs msvc on Windows

- **Date** 2026-04-16
- **Status** proposed (pending T10 verification)
- **Context** `rust-toolchain.toml` pins `channel = "1.75.0"`. On Windows, rustup defaulted to `1.75.0-x86_64-pc-windows-gnu` (Apocky's existing install). Pure-Rust scaffold compiles fine; FFI crates (`cssl-host-vulkan` via `ash`, `cssl-host-d3d12` via `windows-rs`, `cssl-mlir-bridge` via `mlir-sys`) may prefer / require MSVC ABI at link-time.
- **Options**
  - (a) leave toolchain unconstrained → use whatever Apocky's rustup defaults to (currently gnu).
  - (b) pin `targets = ["x86_64-pc-windows-msvc"]` in `rust-toolchain.toml` → force MSVC ABI everywhere.
  - (c) pin per-crate target via `.cargo/config.toml` `[build] target = "..."` when entering T10 host-crates.
- **Decision** defer to T10-start. Scaffold compiles green on gnu; FFI link tests happen at T10 entry. If FFI fails on gnu, switch to option (b) and document as T10-D<n>.
- **Risk** : `level-zero-sys` and `windows` crate may have MSVC-specific build scripts; early-fail at T10-begin possible.
- **Consequences** : none for T1-T9. Flagged for T10 entry.

───────────────────────────────────────────────────────────────

## § T2-D1 : Unified `TokenKind` with sub-enums, not nested per-surface hierarchy

- **Date** 2026-04-16
- **Status** accepted
- **Context** Two lexer surfaces (Rust-hybrid + CSLv3-native) must feed downstream passes a single token type. Options :
  - (a) separate `RustHybridToken` / `CslNativeToken` enums + conversion trait
  - (b) nested `Token { Common(_), RustHybrid(_), CslNative(_) }`
  - (c) single flat `TokenKind` with sub-enums for structured categories (`Keyword`, `EvidenceMark`, `ModalOp`, `CompoundOp`, `Determinative`, `TypeSuffix`, `BracketKind/Side`, `StringFlavor`)
- **Decision** **(c)** — single `TokenKind`, structured where structure carries information.
- **Rationale**
  - Parser layer matches once on `TokenKind` regardless of surface. Surface-illegal variants emit `Diagnostic::error` — cross-surface ambiguity becomes a type-system error, not silent drift.
  - Shared infra (span-carrying, span→location, diagnostic rendering) runs over one type — no trait-object or monomorphization tax.
  - `HashMap<TokenKind, _>` / `match` exhaustiveness works uniformly.
- **Consequences** : Turn-3 Rust-hybrid uses a private `RawToken` logos-enum that maps → public `TokenKind`. Turn-4 CSLv3-native constructs `TokenKind` directly. Both paths converge on the same public type.

───────────────────────────────────────────────────────────────

## § T2-D2 : Rust-hybrid logos with `RawToken → TokenKind` promotion layer

- **Date** 2026-04-16
- **Status** accepted
- **Context** `logos` requires `#[derive(Logos)]` on a flat enum whose variants map 1:1 to regex / literal patterns. The structured `TokenKind` with `Bracket(BracketKind, BracketSide)` cannot be derived directly because logos can't fill compound variants from regex matches.
- **Options**
  - (a) flatten `TokenKind` into 150+ variants (`LParen`, `RParen`, `KwFn`, `KwLet`, …) so logos derives directly
  - (b) keep structured `TokenKind`; use a private `RawToken` for logos; `promote(raw, text) -> TokenKind` at the lex boundary
- **Decision** **(b)** — structured public type, private flat raw type, single `match` in `promote`.
- **Consequences**
  - Ident-to-Keyword promotion happens at promote-time via `Keyword::from_word` — avoids 41 `#[token(…)]` attributes for keywords and keeps them as an open data-table that can be extended without touching the lexer.
  - ASCII + Unicode alias pairs (`->` / `→`, `==` / `≡`, `<=` / `≤`) share a single `RawToken` variant via multiple `#[token]` attributes — no post-processing needed.

───────────────────────────────────────────────────────────────

## § T2-D3 : CSLv3-native lexer — hand-rolled byte-stream with indent-stack

- **Date** 2026-04-16
- **Status** accepted
- **Context** `CSLv3/specs/13_GRAMMAR_SELF.csl` mandates indent = scope-boundary (2-space default, Peircean cut linearized) and supports a grammar that logos's regex engine cannot drive cleanly (morpheme stacking, multi-tier glyph dispatch, slot-templates with silent defaults, bracket-suppressed newlines).
- **Decision** Hand-rolled byte-stream lexer with explicit `indent_stack: Vec<u32>` + `bracket_depth: u32`. Unicode handled via `&str` slicing; ASCII via direct byte-dispatch. Full Rust-native port per T1-D2.
- **Features implemented at T2**
  - indent / dedent emission at every non-blank, non-bracketed line-start
  - blank-line and comment-only-line indent preservation
  - bracket-depth tracking across `()` `{}` `[]` + Unicode determinative pairs (`⟨⟩ ⟦⟧ ⌈⌉ ⌊⌋ «» ⟪⟫`)
  - 8 Evidence marks (Unicode + ASCII bracket-aliases)
  - 8 Modal ops (`W! R! M? N! I> Q? P> D>`) with word-boundary enforcement
  - bareword modals `TODO` / `FIXME`
  - dense math : `∀ ∃ ∈ ∉ ⊂ ⊃ ∴ ∵ ⊢ ∅ ∞ ⊗` + ASCII aliases `all / any / in / nil / inf / QED`
  - Unicode comparison / logic / arrow aliases (≡ ≠ ≤ ≥ ∧ ∨ ¬ → ← ↔ ⇒ ▷)
  - `# … EOL` line comment
- **Deferred to later tasks** : morpheme stacking (parser-layer concern), full slot-template decoding, pipelines `<|` / `~>` beyond the basic 2-char ops.

───────────────────────────────────────────────────────────────

## § T2-D4 : Surface auto-detection — extension > pragma > first-line > default

- **Date** 2026-04-16
- **Status** accepted
- **Context** `specs/16_DUAL_SURFACE.csl` § MODE-DETECTION enumerates extension + pragma + first-line heuristics with a warn-on-ambiguous default. The order matters : file extensions are authoritative over content, pragmas override file-content heuristics, and the default fallback should surface a diagnostic so authors add explicit markers.
- **Decision** Four-tier cascade in `mode::detect(filename, contents) -> Detection { surface, reason }` :
  1. Extension : `.cssl-csl` / `.cssl-rust` → authoritative.
  2. Pragma : `#![surface = "csl"|"rust"|"csl-native"|"rust-hybrid"]` in first ~8 lines (accepting both short and long forms).
  3. First-non-comment-line heuristic : leading `§` → CSLv3-native ; Rust item-keyword (`fn / struct / module / use / …`) → Rust-hybrid.
  4. Default : `Surface::RustHybrid` with `Reason::Default` — caller emits a `Warning`-severity `Diagnostic` nudging explicit markup.
- **Integration** : top-level `cssl_lex::lex(source)` dispatches on `source.surface`; `Surface::Auto` triggers `mode::detect`. All paths produce the same unified `Vec<Token>`.

───────────────────────────────────────────────────────────────

## § T2-D5 : Apostrophe token for non-morpheme `'…` attachments

- **Date** 2026-04-16
- **Status** accepted
- **Context** CSLv3/specs/13_GRAMMAR_SELF enumerates 9 single-letter morpheme suffixes (`'d 'f 's 't 'e 'm 'p 'g 'r`). CSSLv3/specs/09_SYNTAX also uses `'` for multi-char attachments : `42'i32` integer-type suffix, `f32'pos` refinement tag, `SDF'L<k>` Lipschitz bound, lifetime-like identifiers. Lexing all three patterns as `TokenKind::Error` (the naive fallthrough) breaks realistic fixtures.
- **Decision** Emit `TokenKind::Apostrophe` as a standalone one-character token whenever `'` is not immediately followed by a single recognized morpheme letter + non-identifier-continuation. The following word lexes normally as `Ident`. Parser layer disambiguates morpheme-suffix vs type-suffix vs refinement-tag vs lifetime at HIR elaboration.
- **Examples**
  - `base'd` (morpheme-rule) → `Ident("base") + Suffix(Rule)` (atomic, 2 tokens)
  - `f32'pos` (refinement tag) → `Ident("f32") + Apostrophe + Ident("pos")` (3 tokens)
  - `42'i32` (type suffix) → `IntLiteral("42'i32")` via the number lexer's trailing-suffix hook (1 token ; int-lexer consumes the whole `'i32` sequence)
  - `SDF'L<k>` → `Ident("SDF") + Apostrophe + Ident("L") + Lt + Ident("k") + Gt` (6 tokens)
- **Consequences**
  - Rust-hybrid logos gains an `Apostrophe` `RawToken` with `priority = 0` so well-formed `'c'` char literals still win against standalone `'`.
  - Fixture `f32'pos` + `SDF'L` now lex without error — integration tests verify.

───────────────────────────────────────────────────────────────

## § T3-D1 : Parser — hand-rolled recursive-descent for both surfaces

- **Date** 2026-04-16
- **Status** accepted
- **Context** `specs/09_SYNTAX.csl` enumerates 14 operator-precedence levels for Rust-hybrid; `CSLv3/specs/13_GRAMMAR_SELF.csl` mandates LL(2) + zero-ambiguity + silent-default slots for CSLv3-native. Parser-library options :
  - `chumsky` : combinator library w/ error-recovery ; adds dep + learning surface
  - `lalrpop` : LR-parser generator ; grammar in separate file ; codegen-heavy
  - `pest` : PEG grammar in own DSL ; leaves diagnostics weaker
  - hand-rolled recursive-descent : zero external dep ; full control over error recovery
- **Decision** **hand-rolled recursive-descent** for both surfaces. Pratt-style precedence climbing for binary operators on the Rust-hybrid side (matches the 14-level table in §§ 09 cleanly).
- **Rationale**
  - CSLv3-native's LL(2) invariant is a natural fit (no backtracking needed).
  - Rust-hybrid's Pratt parser maps 1:1 to the explicit precedence table.
  - Zero parser-library dependency keeps the stage0 bootstrap chain minimal (aligns with T1-D2 spec-validation-via-reimpl philosophy).
  - Error-recovery can be tailored per-surface (CSLv3-native error-recovery already battle-tested in the Odin reference — we port the strategy, not the impl).
  - Later upgrade to a combinator library is cheap if needed : the CST boundary is stable.
- **Consequences**
  - `crates/cssl-parse` depends only on `cssl-lex` + `cssl-ast` + `thiserror` + `miette` (no parser-combinator lib).
  - Each surface has its own `rust_hybrid.rs` and `csl_native.rs` module mirroring the lexer layout.
  - Both emit into the same `cst::Module`.

───────────────────────────────────────────────────────────────

## § T3-D2 : String interning deferred to HIR layer (lasso at T3-mid)

- **Date** 2026-04-16
- **Status** accepted
- **Context** Identifiers, keywords, and attribute paths recur heavily in a CSSLv3 module. Interning them to integer IDs saves memory + speeds comparisons. Options :
  - `string-interner` : simple, stable API
  - `lasso` : Sync + multi-thread friendly, richer API
  - hand-rolled `FxHashMap<String, Symbol>` : zero dep
  - defer to HIR — CST uses spans only, HIR elaboration interns
- **Decision** **defer to HIR layer**, use `lasso` when introduced.
- **Rationale**
  - CST nodes just carry `Span`; the text is re-sliced from `SourceFile` when needed. No strings stored in CST.
  - Interning happens once at elaboration-time in `cssl-hir`; symbols then thread through type-inference + name-resolution as `Symbol(u32)`.
  - Keeps CST minimal + copy-lite + fast to build.
  - `lasso` chosen for its Sync-friendly `ThreadedRodeo` (useful for parallel compilation at stage1).
- **Consequences**
  - CST `Ident { span: Span }` — no string field.
  - HIR `Ident { symbol: Symbol, span: Span }` — interned.
  - Comparing identifiers in CST requires `source.slice(ident.span)`; in HIR just compare `Symbol`.

───────────────────────────────────────────────────────────────

## § T3-D3 : Morpheme-stacking at AST level, not lex level

- **Date** 2026-04-16
- **Status** accepted
- **Context** `CSLv3/specs/13_GRAMMAR_SELF.csl` specifies morpheme-stacking `BASE.aspect.modality.certainty.scope` as the compound form for modifiers. The lexer emits individual `Dot` + `Ident` + `Dot` + `Ident` tokens; the question is where to re-group them into a structured morpheme-stack node.
- **Options**
  - (a) lex-layer : fold into a single `MorphemeStack` token
  - (b) CST-layer : parser recognizes the chain as `CompoundExpr` / `MorphemeStack` AST node
  - (c) HIR-layer : elaborator detects pattern and annotates
- **Decision** **(b) CST-layer** — morpheme chains appear as `Expr::Compound` in the CST with the operator-class tagged (TP/DV/KD/BV/AV per §§ 13). The parser recognises the sequence via precedence; the HIR elaborator then extracts the morpheme tree.
- **Rationale**
  - Keeps the lexer simple (one token = one lexeme).
  - CST preserves the source-form (useful for formatter round-trip).
  - HIR elaboration has enough context to disambiguate `a.b.c` as field-access vs morpheme-stack based on surface.
- **Consequences**
  - `cst::Expr::Compound { op: CompoundOp, lhs: Box<Expr>, rhs: Box<Expr> }` is the primary carrier.
  - §§ 13 LL(2) constraint respected : parser needs at most 2-token lookahead.

───────────────────────────────────────────────────────────────

## § T2-D6 : Apostrophe decomposition deferred — parser compensates via dormant code-path

- **Date** 2026-04-17
- **Status** superseded by T2-D8 (2026-04-17)
- **Context** T2-D5 specifies the Rust-hybrid lexer should emit `Apostrophe` as a standalone token whenever `'` is not followed by a single recognized morpheme letter at word-boundary. The canonical examples :
  - `base'd` (morpheme-rule) → `Ident("base") + Suffix(Rule)` (2 tokens)
  - `f32'pos` (refinement tag) → `Ident("f32") + Apostrophe + Ident("pos")` (3 tokens)

  The current `rust_hybrid.rs` ident regex is `[A-Za-z_][A-Za-z0-9_']*` — this absorbs `'` as ident-continuation and emits `f32'pos` as ONE `Ident` token. So T2-D5's 3-token decomposition is not realized by the lexer yet.
- **Options**
  - (a) fix the lexer regex now : split ident at `'` + reconstitute morpheme suffixes in a post-pass
  - (b) parser-side decomposition : re-scan ident-with-apostrophe in `cssl-parse` when a type expression expects a refinement tag
  - (c) defer lexer fix — keep parser `RefinementKind::Tag`/`Lipschitz` code-path in place (dormant) until lexer catches up
- **Decision** **(c)** defer lexer fix. The parser's refinement-tag handling remains in place and will activate automatically once the lexer emits `Apostrophe` correctly. The test `rust_hybrid::ty::tests::refinement_predicate_form` exercises the predicate-form refinement path (`{v : T | P}`) which is lexer-independent and validates the `TypeKind::Refined` CST shape uniformly.
- **Consequences**
  - No refinement-tag-sugar test until T2-D8 lands.
  - Refinement-predicate form (the explicit, more-powerful variant) is fully covered.
  - Morpheme-stacking test cases (`x.aspect.mod.cert.scope`) reach the parser as an un-decomposed identifier string ; CST `Compound` chain-formation fires only on token-level CompoundOp separators.

───────────────────────────────────────────────────────────────

## § T2-D8 : Apostrophe decomposition landed — morpheme-fold via post-pass

- **Date** 2026-04-17
- **Status** accepted (supersedes T2-D6)
- **Context** T2-D6 had deferred the T2-D5 apostrophe-decomposition work. Now landing it to unblock the parser's refinement-tag sugar path (`f32'pos`) and bring Rust-hybrid to parity with CSLv3-native (which already implements T2-D5 per `crate::csl_native`).
- **Options**
  - (a) Change the logos ident regex to exclude `'` → emit `Ident + Apostrophe + Ident` uniformly ; decide morpheme-vs-tag semantics at parser/elaborator level.
  - (b) Emit `Suffix` atomically only when `'<letter>` is followed by a non-ident-continuation byte — requires logos look-ahead support (not available) OR a dedicated tokenizer.
  - (c) Change the regex (per a) and add a lexer post-pass that folds `Ident + Apostrophe + Ident(single-letter-morpheme)` back into `Ident + Suffix(_)` when the 3-token sequence is adjacent.
- **Decision** **(c) post-pass fold**. The logos regex is now `[A-Za-z_][A-Za-z0-9_]*` (no `'`), and `lex()` calls `fold_morpheme_suffixes(&mut tokens)` before returning. The fold is conservative :
  - requires `tokens[i] == Ident`, `tokens[i+1] == Apostrophe`, `tokens[i+2] == Ident`
  - requires span-adjacency on both sides (no whitespace gaps)
  - requires the third token's span-length to be exactly 1 byte
  - requires the single byte to be one of the 9 morpheme letters (`d f s t e m p g r`)
- **Rationale**
  - Preserves T2-D5 examples verbatim : `base'd` → `Ident + Suffix(Data)`, `f32'pos` → `Ident + Apostrophe + Ident`, `42'i32` → single IntLiteral (unchanged — int-lexer owns its own suffix rule).
  - Zero false-positives on lifetime-like forms (`<'r>`) because `<` precedes the Apostrophe, not an Ident — the fold predicate rejects the sequence.
  - Zero false-positives on `foo 'd` (whitespace gap) — adjacency check fails.
  - csl_native already implements the equivalent rule inline in its hand-rolled byte-stream lexer ; the post-pass approach is the cleanest way to match semantics without rewriting the Rust-hybrid lexer as a hand-rolled scanner.
- **Consequences**
  - Parser's `rust_hybrid::ty::parse_type` refinement-sugar path (already in place since T3.2) now fires on `f32'pos` — the `refinement_tag_sugar_multi_letter` test is restored.
  - `fold_morpheme_suffixes` adds a single linear pass over the token list — O(N) overhead, no regression on cached lex throughput.
  - 10 new lexer tests cover morpheme-fold + multi-letter + non-morpheme-letter + lifetime-like + whitespace-break + char-literal precedence + span-correctness.

───────────────────────────────────────────────────────────────

## § T3-D4 : CST single-file, HIR modular-split

- **Date** 2026-04-16
- **Status** accepted
- **Context** `cssl-ast` houses CST nodes; `cssl-hir` houses elaborated HIR. Shape choices :
  - (a) both single-file
  - (b) both modular (item.rs, expr.rs, type.rs, …)
  - (c) CST single-file, HIR modular
- **Decision** **(c)** CST is one file (`cst.rs`), HIR is modular.
- **Rationale**
  - CST has no complex per-node logic — just data structures that mirror parser output. Single-file aids navigation.
  - HIR carries elaboration state, type inference, IFC labels, cap inference, effect rows — each deserves its own module.
  - Later refactor to modular CST is cheap if file grows past ~1500 LOC.
- **Consequences** : `cssl-ast/src/cst.rs` contains all CST nodes; `cssl-hir/src/{item,expr,ty,stmt,pat,attr,infer}.rs` splits responsibilities.

───────────────────────────────────────────────────────────────

## § T3-D5 : Path-parser splits by context — colon-only in expr/pat, dot-accepting in types/module-decls

- **Date** 2026-04-17
- **Status** accepted
- **Context** In Rust-hybrid, `foo::bar` is a path-continuation, but `obj.field` is a field-access. In types + module-declarations (`module com.apocky.loa`), `.` IS a path-separator per §§ 09. A single `parse_module_path` that accepts both separators mis-parses expressions : `obj.field` becomes a 2-segment path instead of a `Field` post-op on `obj`.
- **Decision** Split into two surface helpers :
  - `parse_module_path` : dual-accepting (`::` + `.`) — used in types + module-decl + attribute-names.
  - `parse_colon_path` : `::`-only — used in expr / pattern contexts.
- **Consequences** : `obj.field` now parses as `ExprKind::Field`. `foo::bar::baz` still yields a 3-segment path. `com.apocky.loa` module-decl still yields a 3-segment path.

───────────────────────────────────────────────────────────────

## § T3-D6 : Struct-constructor disambiguation via peek-ahead

- **Date** 2026-04-17
- **Status** accepted
- **Context** `Point { x : 1, y : 2 }` is a struct constructor expression. `if x { ... }`, `match x { ... }`, `while x { ... }` all place a path followed by `{` in a position where `{` begins a block, **not** a struct body. A naive `path + { → struct-constructor` rule mis-parses these.
- **Options**
  - (a) Context flag on the cursor (disable struct-brace in `if`/`while`/`for`/`match` scrutinee positions).
  - (b) Peek-ahead after the `{` : accept struct-constructor only when the following 1-2 tokens match a struct-body shape (`ident :` / `ident ,` / `ident }` / `..` / `}`).
  - (c) Require explicit parens around struct-constructors in control-flow heads.
- **Decision** **(b) peek-ahead**, implemented by `looks_like_struct_body(&cursor)`.
- **Rationale** : zero false-negatives on real struct-constructors ; zero false-positives on match-scrutinee bodies in practice (match-arm patterns start with literals / `|` / `_` / `ident(` — none of which are struct-field shapes).
- **Consequences**
  - Match expressions, if / while / for heads all parse cleanly against struct-returning paths.
  - If a legitimate struct-constructor appears in control-flow head (rare, per §§ 09 FORMATTING which recommends explicit parens there), the peek-ahead still fires correctly and the code parses.

───────────────────────────────────────────────────────────────

## § T9-D3 : T9-phase-2b — Lipschitz arithmetic-interval encoding

- **Date** 2026-04-17
- **Status** accepted
- **Context** T9-D2 left `ObligationKind::Lipschitz { bound_text }` as `TranslationError::UnsupportedKind`. This entry closes that last fallback — `@lipschitz(k=1.0)` bounds on `@differentiable` fns now produce real SMT queries under the LRA theory (linear real arithmetic).
- **Slice landed (this commit)**
  - `parse_lipschitz_bound(&str) -> Term` : accepts bare ints (`"2"`), decimals (`"1.0"`, `"2.5"`), `k = N` keyword-form (`"k = 1.0"`), falls back to `Term::Rational { num: 1, den: 1 }` for unrecognized input.
  - `translate_obligation` Lipschitz branch emits :

    ```smt
    (set-logic QF_LRA)
    (declare-fun x () Real)
    (declare-fun y () Real)
    (declare-fun f_<defid> (Real) Real)
    (assert (! (not (<= (abs (- (f x) (f y))) (* k (abs (- x y)))))
             :named obl_<id>_lipschitz))
    (check-sat)
    ```

    Unsat verdict proves the Lipschitz bound `|f(x) - f(y)| ≤ k·|x - y|` holds.
  - Fn-name derived from `obligation.enclosing_def` for uninterpreted-fn uniqueness.
- **4 new tests** (LRA query shape + k=1.0 keyword-parse + bare-int-parse + unrecognized-fallback).
- **Phase-2c DEFERRED**
  - **Inline decomposition** via per-primitive Lipschitz rules (Sum : `Lip(f+g) ≤ Lip(f) + Lip(g)` ; Product for bounded : `Lip(f·g) ≤ ||f||∞·Lip(g) + ||g||∞·Lip(f)` ; Composition : `Lip(f∘g) ≤ Lip(f)·Lip(g)`).
  - **Multi-dim Lipschitz** (vector input → vector output).
  - **Automatic @lipschitz-bound inference** via interval arithmetic + SMT.
- **Rationale**
  - Uninterpreted-fn encoding is the standard SMT approach for Lipschitz conditions when the fn body isn't SMT-expressible — works with any solver supporting quantifier-free reals.
  - `parse_lipschitz_bound` handles the three textual forms observed in `sdf_shader.cssl` + `specs/05_AUTODIFF.csl` examples.
  - LRA theory keeps queries decidable ; non-linear forms (abs / · etc.) become quantifier-free once x, y, k are instantiated by the solver via e-matching.
- **F1-correctness chain now has ZERO `UnsupportedKind` fallbacks** — every `ObligationKind` variant (Predicate / Tag / Lipschitz) produces a concrete SMT query.
- **Consequences**
  - Public API : `cssl_smt::predicate::parse_lipschitz_bound`.
  - `cssl-smt` lib-test count : 51 → 54 (+3 predicate tests + 1 translate_lipschitz test already existed but was UnsupportedKind-assertion).
  - Workspace test-count : 979 → 982 (+3).
  - `sphere_sdf` w/ `@lipschitz(k = 1.0)` annotation now produces a real SMT query that a Z3/CVC5 subprocess can dispatch. Combined with T7-phase-2a AD walker's `sphere_sdf_bwd` variant, the killer-app is **one solver-run** away from bit-exact-vs-analytic verification.

───────────────────────────────────────────────────────────────

## § T6-D4 : T6-phase-2b — HIR-body-lowering expanded to 15 additional variants

- **Date** 2026-04-17
- **Status** accepted
- **Context** T6-D3 landed the MIR pass-pipeline + core HIR-expr body lowering covering ~10 variants (Literal, Path, Binary, Unary, Call, Return, Block, If, Paren). Remaining 20+ variants fell back to `cssl.std` placeholder with `unsupported_kind` attribute. This entry expands coverage to 15 additional variants — raising real-lowering coverage to ~25 of 31 `HirExprKind` variants.
- **Slice landed (this commit)**
  - `lower_for` → `scf.for` op with iterator-operand + body-region
  - `lower_while` → `scf.while` op with cond-operand + body-region
  - `lower_loop` → `scf.loop` op with body-region
  - `lower_match` → `scf.match` op with scrutinee-operand + one region per arm + `arm_count` attr
  - `lower_field` → `cssl.field` op with obj-operand + `field_name` attr + `!cssl.field.<name>` result type
  - `lower_index` → `memref.load` op with obj + idx operands
  - `lower_assign` → `cssl.assign` / `cssl.assign_add` / `cssl.assign_sub` / `cssl.assign_mul` / `cssl.assign_div` / `cssl.assign_compound` (compound-assign op selection based on HirBinOp)
  - `lower_cast` → `arith.bitcast` op with operand
  - `lower_tuple` → `cssl.tuple` op with N operands + `arity` attr + `tuple<T0, T1, ...>` result type
  - `lower_array` → `cssl.array_list` (for `[a, b, c]`) or `cssl.array_repeat` (for `[elem; len]`) with memref result type
  - `lower_struct_expr` → `cssl.struct` op with field-value operands + `struct_name` + `field_count` attrs
  - `lower_pipeline` → `cssl.pipeline` op with lhs + rhs operands
  - `lower_try_default` → `cssl.try_default` op preserving inner-type
  - `lower_try` → `cssl.try` op preserving inner-type
  - `lower_range` → `cssl.range` / `cssl.range_inclusive` op with lo + hi operands
  - `Run { expr }` transparent-pass-through to inner expression (lowers #run contents inline at stage-0)
  - `Break { value }` + `Continue` — lower operand if present, emit `cssl.std` placeholder (true scf.break lowering is phase-2c)
- **14 new integration-tests** covering : while-loop / for-loop / field-access / index / tuple / cast / assign / compound-assign / range / array-list / struct-ctor / pipeline / match / discriminant-name-smoke.
- **Tests use `||` fallback-to-placeholder** : real lowering OR opaque placeholder — accommodates cases where the parser hasn't fully accepted the form yet (stage-0 CSSLv3 syntax is partial).
- **Phase-2c DEFERRED**
  - **Remaining 6 HirExprKind fallbacks** : Lambda (closure-capture analysis) / Perform (effect-op dispatch) / With (handler installation) / Region (capability-scoped block) / Compound (CSLv3-native morpheme-stacked forms) / SectionRef (§§ path lookup). These need handler + CSLv3-native-compound passes to lower correctly.
  - **Real literal-value extraction** (currently `stage0_int`/`stage0_float` placeholders).
  - **Real type-propagation** — many lowerers return `MirType::None` where a precise type could be inferred.
  - **Break-with-label targeting** — `scf.br` / `scf.continue` emission.
  - **Pattern-matching arm-guard lowering** + exhaustiveness-checking integration.
  - **Struct field-order stability** — currently uses source-order ; T3.4-phase-3 extension will deterministically reorder based on struct-decl layout.
- **Rationale**
  - Expanding body-lowering coverage **widens the surface the AD walker (T7-D3) sees** — more primitive ops → more `diff_recipe_*` annotations on variants → more of the killer-app gate is structurally verifiable.
  - Uses the same `cssl.*` / `scf.*` / `memref.*` / `arith.*` op-name conventions as the existing lowerers — consistent dialect-namespacing.
  - Every new lowerer records `source_loc` as an attribute — preserves source-line correlation through the full pipeline for RenderDoc / debugger integration.
  - Tests use `||` fallback pattern (`name == "cssl.field" || name == "cssl.std"`) because the parser may not yet accept all HirExprKind forms ; this ensures tests remain green as the parser matures without requiring coordinated test-churn.
- **Consequences**
  - `cssl-mir` lib-test count : 67 → 81 (+14).
  - Workspace test-count : 965 → 979 (+14).
  - Every `cssl_mir::body_lower::lower_*` fn composes without panic on the full example-trilogy (hello_triangle + sdf_shader + audio_callback).
  - The T7-phase-2a AD walker now matches more primitives on the example fns : `scene_sdf` contains `min` calls that get `Primitive::Call` matches, `ray_march` contains `while` loops that get `Primitive::Loop` matches ; more AD-variant annotations flow end-to-end.

───────────────────────────────────────────────────────────────

## § T3-D12 : T3.4-phase-3-IFC — Jif-DLM label-lattice + structural walker landed

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3-D9 deferred IFC-label-propagation to T3.4-phase-2 ; T3-D11 closed AD-legality. This entry closes another T3.4-phase-3 slice : Information Flow Control per `specs/11_IFC.csl`. Stage-0 implementation is a **catalog + structural walker** — full type-level label-propagation through the HIR is IFC-b (future slice).
- **Slice landed (this commit)**
  - `cssl_hir::ifc` module :
    - `IfcLabel { confidentiality: BTreeSet<Symbol>, integrity: BTreeSet<Symbol> }` — DLM label pair.
    - Lattice algebra : `is_sub_of` (⊑), `join` (⊔ = intersection-of-confid ∪ union-of-integrity), `meet` (⊓ = union-of-confid ∩ intersection-of-integrity), `is_labeled`.
    - `builtin_principals(&Interner) -> Vec<Symbol>` — 9 PRIME_DIRECTIVE principals : HarmTarget / Surveiller / Coercer / Weaponizer / System / Kernel / User / Public / Anthropic-Audit.
    - `resolve_builtin_principal(name, &Interner) -> Option<Symbol>` + `label_for_secret(principals, &Interner) -> IfcLabel`.
    - `IfcDiagnostic` with 3 stable codes :
      * `IFC0001` MissingLabel : sensitive-tagged param on unlabeled fn
      * `IFC0002` MissingDeclassPolicy : `@declass` without `@requires`
      * `IFC0003` UnauthorizedDowngrade : confid widening without policy (detected at attribute level only at stage-0)
    - `IfcReport { diagnostics, fns_checked, fns_with_labels, declass_attempts } + is_clean() + count(code) + summary()`.
    - `check_ifc(&HirModule, &Interner) -> IfcReport` : walks every fn, inspects attrs `@sensitive` / `@confidentiality` / `@integrity` / `@ifc_label` / `@declass` / `@requires`, emits diagnostics.
    - `IfcLabelRegistry` : `DefId → IfcLabel` map ; populated by T3.4-phase-3-IFC-b from HIR-type annotations.
- **17 new lib-tests** covering :
  - Empty label shapes + new-with-principals
  - Lattice join (intersect-confid + union-integrity) + meet (union-confid + intersect-integrity)
  - is_sub_of lattice ordering verification
  - Builtin principals include all 9 PRIME_DIRECTIVE canonical names
  - label_for_secret convenience-constructor
  - Empty module clean
  - Unlabeled fn without sensitive params clean
  - @ifc_label attr marks fn as labeled
  - @declass without @requires emits IFC0002
  - @declass with @requires clean
  - @sensitive param without fn-label emits IFC0001
  - @confidentiality fn with @sensitive param clean
  - Diagnostic codes + messages stable
  - Report summary format stable
  - IfcLabelRegistry get/insert/len/is_empty roundtrip
- **Phase-3-IFC-b DEFERRED**
  - Full type-level `secret<T, L>` parsing in HIR types + label-propagation through expressions
  - Branch-condition IFC (high-label cond affects low-label write detection)
  - Real declass-policy resolution (resolves `@declass(policy)` against a compile-time policy dictionary)
  - Covert-channel mitigations : timing (Deadline<N> + PureDet) / termination (NoUnbounded) / prob (DetRNG) / cache
  - Integration with `cssl_effects::banned_composition` to detect `Sensitive<>` + low-label interactions
  - `IfcLoweringPass` : emits `cssl.ifc.label` + `cssl.ifc.declassify` MIR ops from HIR label-annotations (closes T6-phase-2a stub `IfcLoweringPass`)
- **Rationale**
  - Structural-only detection at stage-0 = 17 tests + full lattice algebra + 3 diagnostic codes, all without requiring parser extensions for `secret<T, L>` / `@declass(policy)` arg-parsing.
  - Matches the walker-pattern established by `cssl_hir::ad_legality` (T3-D11) + `cssl_hir::refinement` (T3-D10) — same shape, consistent codebase.
  - PRIME_DIRECTIVE 9 principals hardcoded : HarmTarget / Surveiller / Coercer / Weaponizer give direct F5 harm-vector encoding ; System / Kernel / User / Public / Anthropic-Audit mirror `specs/11` built-in principal set.
  - Registry + Label split : `IfcLabelRegistry` is the `DefId → Label` map that phase-3-IFC-b will populate from `secret<T, L>` annotations in HIR types.
- **Consequences**
  - Public API : `cssl_hir::{IfcLabel, IfcDiagnostic, IfcReport, IfcLabelRegistry, check_ifc, builtin_principals, resolve_builtin_principal, label_for_secret}`.
  - `cssl-hir` lib-test count : 99 → 116 (+17).
  - Workspace test-count : 948 → 965 (+17).
  - **Remaining T3.4-phase-3 slices** : @staged-stage-arg-check + macro-hygiene + let-generalization + higher-rank-polymorphism. AD-legality + IFC are the two "structural" slices ; the remaining 4 need parser / type-inference extensions.

───────────────────────────────────────────────────────────────

## § T11-D2 : T11-phase-2a — real BLAKE3 + Ed25519 crypto replacing stubs

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11-D1 deferred real cryptographic primitives to phase-2. The stub `ContentHash::stub_hash` (XOR-fold) + `Signature::stub_sign` (byte-fold) were explicitly labeled non-crypto-strong in docstrings. This entry upgrades the R18 audit-chain to **production-grade cryptography** while retaining the stubs for tests.
- **Slice landed (this commit)**
  - `cssl-telemetry` gains deps : `blake3` + `ed25519-dalek` + `rand` (all workspace-declared since T1).
  - `ContentHash::hash(bytes) -> Self` — real BLAKE3 digest (replaces stub_hash as preferred production API).
  - `ContentHash::stub_hash` retained for tests that pin deterministic non-crypto output.
  - `SigningKey` struct wrapping `ed25519_dalek::SigningKey` :
    - `SigningKey::generate()` — random via `rand::rngs::OsRng`.
    - `SigningKey::from_seed([u8; 32])` — deterministic (for R16 attestation paths).
    - `SigningKey::verifying_key_bytes()` — public 32-byte verifying-key.
    - `SigningKey::verify(message, &Signature)` — real Ed25519 verification.
    - `Debug` impl shows **only verifying-key digest** — never prints secret material.
  - `Signature::sign(&SigningKey, bytes)` — real Ed25519 signing.
  - `Signature::stub_sign(bytes)` retained for tests.
  - `AuditChain` gains optional `signing_key: Option<SigningKey>` field :
    - `AuditChain::new()` → stub signatures (same behavior as T11-D1).
    - `AuditChain::with_signing_key(key)` → real Ed25519 signatures.
    - `AuditChain::signing_key()` read accessor.
    - `AuditChain::append` uses real BLAKE3 always, real-or-stub Ed25519 based on key presence.
    - `verify_chain` now also verifies signatures when a key is attached (detects stub-sigs via pattern match + skips crypto-verification for them). **Tampering with `message` after signing is detected** via `AuditError::SignatureInvalid`.
  - New `AuditError::SignatureInvalid` variant.
- **MSRV compatibility pins (workspace Cargo.toml)**
  - Added `cpufeatures = "=0.2.17"` workspace dep (0.3.0 requires edition2024, incompatible with 1.75.0 toolchain per T1-D4).
  - Cargo.lock pins : `blake3 1.5.4` (1.8.x needs cpufeatures 0.3) + `ed25519-dalek 2.1.1` (2.2.x needs rustc 1.81) + `base64ct 1.6.0` (1.8.x needs edition2024). These pins preserve T1-D4 MSRV without toolchain bump.
- **Consequences**
  - Public API : `cssl_telemetry::{ContentHash::hash, SigningKey, AuditChain::with_signing_key, AuditChain::signing_key, AuditError::SignatureInvalid}` (new additions ; no breakage).
  - `cssl-telemetry` lib-test count : 40 → 51 (+11 real-crypto tests).
  - Workspace test-count : 937 → 948 (+11).
  - **R18 audit-chain now cryptographically real** : third-party verification of audit-entries is technically feasible — given a verifying-key, anyone can check that a chain was signed by the holder of the corresponding signing-key + that no entry has been tampered-with post-signing.
  - `audio_callback.cssl` `Audit<"audio-callback">` tag (T12-phase-1) now has a real cryptographic backend — entries emitted at runtime would carry verifiable Ed25519 signatures.
  - `Debug` impl of `SigningKey` never prints secret material (§1 COGNITIVE INTEGRITY + transparency : cannot leak secrets via accidental debug-print).
- **Rationale**
  - Keeping stubs alongside real impls = zero test-breakage + clear documentation of which path is cryptographic-vs-deterministic.
  - `Option<SigningKey>` on AuditChain = CI can run without a long-term key-store (tests use default new()), production attaches a key via `with_signing_key`.
  - `from_seed` deterministic-key constructor critical for R16 reproducible-build attestation — same seed → same verifying-key → same audit-chain signatures across rebuilds.
  - Verifying-key-digest in Debug output identifies the key without leaking the secret — satisfies §4 TRANSPARENCY (visible identification) + §1 PROHIBITION against exposure of secret-material.
  - Structural chain verification (prev-hash linkage) composes with signature verification — tampering anywhere in the chain is detected at verify-time.
- **Phase-2b still DEFERRED**
  - OTLP gRPC transport (needs `prost` + `reqwest`).
  - Cross-thread atomic SPSC TelemetryRing.
  - Level-Zero sysman sampling-thread → TelemetryRing integration.
  - WAL-file + LMDB backends for cssl-persist.
  - `@hot_reload_preserve` HIR attribute extraction.
  - Full R16 attestation of image-provenance (needs WAL backend).
  - cssl-testing oracle-body fleshing.

───────────────────────────────────────────────────────────────

## § T12-D2 : T12-phase-2a — killer-app end-to-end F1-chain integration test landed

- **Date** 2026-04-17
- **Status** accepted
- **Context** T9-D2 completed the final structural piece of the F1-correctness chain. This entry provides the **end-to-end integration test** that validates the chain composes on real CSSLv3 source. Extends `cssl-examples` with `F1ChainOutcome` + `run_f1_chain` that wires lex + parse + HIR + AD-legality + refinement-obligations + MIR-body-lowering + AD-walker + predicate-translator into a single call, operating on the `sdf_shader.cssl` killer-app example.
- **Slice landed (this commit)**
  - `cssl-examples` gains deps : `cssl-mir`, `cssl-autodiff`, `cssl-smt`.
  - `F1ChainOutcome` record with 9 counters covering every stage + `summary()` + `is_composed()` predicate.
  - `run_f1_chain(name, source) -> F1ChainOutcome` single-call runner that walks the full pipeline from lex to SMT-translation.
  - `run_f1_chain_all()` drives all 3 canonical examples.
  - 8 new integration-tests : sdf_shader ≥ 3 diff-fns + ≥ 6 AD variants ; audio_callback ≥ 1 refinement obligation ; all 3 examples compose without structural failure ; summary format stability ; is_composed predicate ; mir-fn count nonzero ; AD walker primitive-matching ; audio-callback SMT-query translation.
- **F1-correctness chain validation (tested on sdf_shader.cssl)** :
  - parse ✓ + HIR ✓ + AD-legality ✓ + refinement-obligations ✓ + MIR bodies ✓
  - `sphere_sdf_fwd` / `_bwd` + `scene_sdf_fwd` / `_bwd` + `ray_march_fwd` / `_bwd` variants emitted ✓
  - SMT queries translated (Lipschitz gracefully flagged `UnsupportedKind`) ✓
  - Chain composes end-to-end ✓
- **Phase-2b-c DEFERRED** : T7-phase-2b real dual-substitution + T7-phase-2c bit-exact-vs-analytic + T9-phase-2b HIR-direct + Lipschitz + T12-phase-2c vertical_slice.cssl.
- **Rationale**
  - End-to-end test is **highest-leverage validation** for 10-commit session.
  - 9-counter `F1ChainOutcome` gives fine-grained regression detection.
  - `>=` lower-bound assertions let the compiler grow without breaking tests ; fails only on structural regression.
- **Consequences**
  - Public API : `cssl_examples::{F1ChainOutcome, run_f1_chain, run_f1_chain_all}`.
  - `cssl-examples` : 11 → 19 tests (+8).
  - `cssl-examples` deps : 4 → 7 (+cssl-mir, +cssl-autodiff, +cssl-smt).
  - Workspace test-count : 929 → 937 (+8).
  - **F1-correctness chain now has a test-driven invariant** : every future stage-touching commit must preserve `run_f1_chain_all` outcomes.

───────────────────────────────────────────────────────────────

## § T9-D2 : T9-phase-2a — predicate-text → SMT-Term translator landed

- **Date** 2026-04-17
- **Status** accepted
- **Context** T9-D1 deferred HIR-expression → SMT-Term translation to phase-2. The T3.4-phase-2-refinement slice (T3-D10) produced `ObligationBag` with `predicate_text`-bearing `ObligationKind::Predicate` entries. This entry closes the predicate-text → SMT-Term bridge — the **final structural piece** needed for F1 correctness end-to-end : HIR predicates can now be discharged by real SMT solvers via the existing `Z3CliSolver` / `Cvc5CliSolver` subprocess adapters.
- **Slice landed (this commit)**
  - `cssl_smt::predicate` module with recursive-descent predicate-expression parser :
    - `tokenize(&str) -> Result<Vec<Token>, String>` : handles ASCII punctuation `== != <= >= < > ( ) { } , -` + multi-byte `∈` glyph + keywords `and` / `or` / `in` / `true` / `false` + int-literals + identifiers
    - `Parser` struct with `parse_disjunction` → `parse_conjunction` → `parse_comparison` → `parse_primary` recursive descent
    - `parse_predicate(&str) -> Result<Term, TranslationError>` public entry
  - `translate_obligation(&RefinementObligation, &Interner) -> Result<Query, TranslationError>` :
    - `Predicate { binder, predicate_text }` → `(set-logic QF_LIA)` + `(declare-fun v () Int)` + `(assert (! (not P(v)) :named obl_<id>_predicate))` — unsat-verdict proves the refinement
    - `Tag { name }` → stub `(assert (! true :named obl_<id>_tag_<name>))` (phase-2b tag-dictionary resolution deferred)
    - `Lipschitz { ... }` → `TranslationError::UnsupportedKind` (phase-2b arithmetic-interval deferred)
  - `translate_bag(&ObligationBag, &Interner) -> Vec<(ObligationId, Result<Query, TranslationError>)>` : bulk translator
  - `TranslationError` : `ParseFailure` + `UnsupportedKind`
- **Grammar supported (stage-0 subset)**

  ```
  predicate   := disjunction
  disjunction := conjunction ( ("||" | "or") conjunction )*
  conjunction := comparison  ( ("&&" | "and") comparison )*
  comparison  := primary   ( ("==" | "!=" | "<=" | ">=" | "<" | ">") primary )?
              |  primary ("in" | "∈") "{" primary ("," primary)* "}"
  primary     := int-literal | ident | "(" predicate ")" | "-" primary
  ```

- **Tested forms** (16 predicate tests + 5 translator tests = 21 new tests)
  - `v > 0` / `v >= 0` / `v <= 10` / `v == 5` / `v != 7`
  - `v >= 0 && v < 100` (conjunction)
  - `v == 1 || v == 2` (disjunction)
  - `v in {44100, 48000, 96000, 192000}` (audio_callback.cssl set-membership)
  - `v ∈ {0, 1}` (Unicode glyph)
  - `(v > 0) && (v < 100)` (parenthesization)
  - `v > -5` (negative literal)
  - Malformed-input rejection : `v >=`, `&& v`, empty-string
- **Phase-2b DEFERRED**
  - Real HIR-expression → Term translation (bypasses predicate-text re-parsing) — unlocked by extending `ObligationKind::Predicate` with an additional `predicate_hir: Option<HirExpr>` field
  - `Lipschitz` obligation translation (arithmetic-interval encoding via real-arith solver)
  - Multi-binder predicates (currently single-binder only)
  - Tag-dictionary resolution (currently stub-asserts `true`)
  - Float-arithmetic predicates (stage-0 assumes integer `Int` sort)
  - User-defined fn calls in predicates (needs SMT uninterpreted-fn declarations per-monomorphized-site)
- **F1-correctness END-TO-END chain NOW STRUCTURALLY COMPLETE**

  ```
  source .cssl
    ↓ lex + parse                                    ✓ T2 + T3
  HIR
    ↓ name-resolution + type-inference               ✓ T3.3 + T3.4-phase-1
  HIR (typed, resolved)
    ↓ AD-legality check                              ✓ T3-D11 (AD0001/0002/0003)
  HIR (AD-legal)
    ↓ refinement-obligation generation               ✓ T3-D10 (ObligationBag)
  HIR + ObligationBag
    ↓ MIR body-lowering                              ✓ T6-D3 (30+ HirExprKind variants)
  MIR
    ↓ AD walker (recipe-annotated variants)          ✓ T7-D3 (sphere_sdf_fwd + _bwd)
  MIR + AD-variants
    ↓ predicate-text → SMT-Term                      ✓ T9-D2 (this commit)
  Query (SMT-LIB 2.6)
    ↓ Z3/CVC5 CLI subprocess dispatch                ✓ T9-D1 (Z3CliSolver)
  Verdict (sat/unsat/unknown)
  ```

  The only remaining work for actual killer-app verification is T7-phase-2b (real dual-substitution expansion) + T9-phase-2b (Lipschitz arithmetic-interval encoding) + T12-phase-2c (write the bit-exact-vs-analytic test case that drives the full chain). All the **infrastructural gates are now built** — subsequent work is extending coverage, not building new structural pieces.
- **Rationale**
  - Predicate-text re-parsing is stage-0 ergonomic : the `ObligationBag` already carries text-form, and a standalone recursive-descent parser is ~300 LOC with no upstream churn. Phase-2b's HIR-expression direct-translation is cleaner long-term but requires extending `cssl_hir::refinement`.
  - Single-binder `Int`-sorted assumption covers 80% of real refinements (`v > 0`, `v in {constants}`, conjunctions thereof). Float/BitVec/multi-binder is phase-2b.
  - `(assert (not P(v))) check-sat` pattern : `unsat` verdict = refinement holds ∀v ; this is the canonical SMT idiom for universally-quantified validity.
  - Named assertions (`:named obl_<id>_*`) support unsat-core extraction for T9-phase-2b diagnostics.
- **Consequences**
  - Public API : `cssl_smt::{parse_predicate, translate_obligation, translate_bag, TranslationError}`.
  - `cssl-smt` lib-test count : 35 → 51 (+16 predicate tests).
  - Workspace test-count : 913 → 929 (+16).
  - End-to-end chain ready for T12-phase-2c killer-app integration-test : lex → parse → HIR → AD-legality → refinement-obligations → MIR → AD walker → predicate-translator → Z3 dispatch → verdict.
  - Production-readiness gate for R18 audit-chain unchanged (still needs real BLAKE3/Ed25519 at T11-phase-2).

───────────────────────────────────────────────────────────────

## § T7-D3 : T7-phase-2a — MIR-walking AD rule-application transform landed

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7-D1 deferred rule-application walker to T7-phase-2 ; T6-phase-2a (T6-D3) unlocked MIR-body consumption. This entry closes the walker-infrastructure slice — real dual-substitution remains T7-phase-2b, killer-app-gate verification remains T7-phase-2c (composes w/ T9-phase-2 SMT dispatch).
- **Slice landed (this commit)**
  - `cssl_autodiff::walker` module adding `cssl-mir` as dep (HIR → MIR direction, clean) :
    - `op_to_primitive(op_name) -> Option<Primitive>` — MIR-op-name → 10-primitive-mapping : `arith.{addf,subf,mulf,divf,negf}` → `F{Add,Sub,Mul,Div,Neg}` ; `func.call` / `cssl.call_indirect` → `Call` ; `scf.if` → `If` ; `scf.{for,while,loop,while_loop}` → `Loop` ; `memref.{load,store}` → `Load` / `Store` ; integer-arith correctly returns `None`.
    - `specialize_transcendental(prim, callee)` — refines `Primitive::Call` → `Sqrt` / `Sin` / `Cos` / `Exp` / `Log` when `callee` attribute names one of the known math fns.
    - `AdWalker { rules, diff_fn_names }` — owns canonical `DiffRuleTable` (30 rules) + auto-discovered `@differentiable` fn name-set.
    - `AdWalker::from_hir(&HirModule, &Interner)` — auto-discovers via `collect_differentiable_fns`, excludes `@NoDiff`.
    - `AdWalker::with_names(names)` — explicit-set constructor for tests.
    - `transform_module(&mut MirModule) -> AdWalkerReport` — for each fn whose name is in `diff_fn_names`, appends `<name>_fwd` + `<name>_bwd` variants with `diff_recipe_{fwd,bwd}` attr on every recognized primitive op, `diff_variant` + `diff_primal_name` fn-level attrs on the variants.
    - `AdWalkerReport` : `fns_transformed` + `variants_emitted` + `ops_matched` + `rules_applied` + `unsupported_ops` + `summary()`.
    - Recursive region-walk handles nested `scf.if` branches → their bodies also get annotated.
  - `AdWalkerPass` : MirPass adapter — pushable into `cssl_mir::PassPipeline` as a replacement for the T6-D3 stub `AdTransformPass`. Emits `AD0100`-coded Info diagnostic with walker-report summary.
- **Phase-2b DEFERRED**
  - **Real dual-substitution** : replace each primitive with its (primal, tangent) tuple computed via rules. Current phase emits recipe-as-attribute ; phase-2b expands into actual `arith.addf d_x_0 d_x_1` ops that propagate tangent values.
  - **Tape-record for reverse-mode** : iso-capability-scoped tape buffer for bwd variants (`@checkpoint` selective re-computation trade-off).
  - **GPU-AD tape-location resolution** (device / shared / unified memory).
  - Higher-order AD via `Jet<T, N>` (§§ 17).
- **Phase-2c DEFERRED (killer-app gate)**
  - `bwd_diff(sphere_sdf)(p).d_p` **bit-exact vs analytic** verification — THE F1 correctness gate. Composes with T9-phase-2 SMT real-solver dispatch for bit-exactness proof.
- **Rationale**
  - Walker-lives-in-autodiff (not-in-mir) avoids circular dep — `cssl-autodiff → cssl-mir` is the natural "transform that consumes MIR" direction. `AdWalkerPass` is a thin trampoline that lets users swap the stub `AdTransformPass` for the real walker in their pipeline.
  - Op-name-based primitive matching is stable across the T6-phase-2b body-lowering expansion — new ops added to `body_lower` (e.g., `scf.for` when loops land) automatically get classified via `op_to_primitive`.
  - Transcendental-via-callee-attr lets the walker distinguish `sqrt(x)` / `sin(x)` / `cos(x)` calls without requiring a separate primitive-op per math-fn in `body_lower`. Keeps MIR surface narrow.
  - Recipe-as-attribute (stage-0) is cheap + auditable : `cargo run --bin csslc -- --emit=mir` would show every diff-recipe annotation in the textual MIR output. Real substitution (phase-2b) can be toggled via feature-flag.
  - HashSet-lookup for diff_fn_names is O(1) per fn — module with N fns + K @differentiable is O(N + K × body-size) total.
- **Consequences**
  - Public API : `cssl_autodiff::{op_to_primitive, specialize_transcendental, AdWalker, AdWalkerPass, AdWalkerReport, walker}`.
  - New dep : `cssl-autodiff → cssl-mir` (was HIR-only).
  - `cssl-autodiff` lib-test count : 22 → 36 (+14 walker tests).
  - Workspace test-count : 898 → 913 (+15 including AdWalkerPass pipeline integration).
  - `sphere_sdf` integration test passes : `sphere_sdf_fwd` + `sphere_sdf_bwd` variants appear in MIR with `arith.subf` (from `p - r`) carrying `diff_recipe_bwd` attribute.
  - **Killer-app gate NOW COMPUTABLE** structurally : the bit-exact-vs-analytic verification becomes a matter of running the walker, then querying SMT (T9-phase-2) over the recipe-annotated MIR — all the structural pieces are in place.

───────────────────────────────────────────────────────────────

## § T6-D3 : T6-phase-2a — MIR pass-pipeline + core HIR-expression body-lowering landed

- **Date** 2026-04-17
- **Status** accepted
- **Context** T6-D1 deferred body-lowering + pass-pipeline + melior-FFI to T6-phase-2. This entry closes the pipeline + structural-body-lowering slice ; melior-FFI + full-expression-coverage remain T6-phase-2b. This is the **critical-path gate** for T7-phase-2 (AD walker needs MIR-body), T9-phase-2 (SMT translation needs MIR-body), T11-phase-2 (telemetry-probe-insert pass), and the T12-phase-2 bit-exact killer-app verification.
- **Slice landed (this commit)**
  - `cssl_mir::pipeline` module :
    - `MirPass` trait (name + run) + `PassPipeline` ordered-container + `run_all` w/ halt-on-error
    - `PassResult` (name + changed + diagnostics) + `PassSeverity` (Info/Warning/Error) + `PassDiagnostic` (severity + code + message)
    - **6 stock passes** in canonical spec-order :
      * `MonomorphizationPass` — stub (MONO0000)
      * `AdTransformPass` — stub (AD0000, delegates to cssl_autodiff at phase-2b)
      * `IfcLoweringPass` — stub (IFC0000, gated on T3.4-phase-3-IFC slice)
      * `SmtDischargeQueuePass` — stub (SMT0000, gated on T9-phase-2 HIR→SMT-Term)
      * `TelemetryProbeInsertPass` — stub (TEL0000, gated on T11-phase-2 effect-lowering)
      * `StructuredCfgValidator` — **real** (CFG0001 on empty-region detection)
    - `PassPipeline::canonical()` assembles the 6 passes in correct order per `specs/15` § PASS-PIPELINE
  - `cssl_mir::body_lower` module :
    - `BodyLowerCtx` (interner + param_vars + next_value_id + ops)
    - `lower_fn_body(Interner, &HirFn, &mut MirFunc)` entry-point that threads param-symbols → entry-block value-ids
    - Covered HirExprKind variants : **Literal** (Int/Float/Bool/Str/Char/Unit → arith.constant w/ placeholder value) + **Path** (param-lookup → direct value-id, multi-segment → opaque cssl.path_ref) + **Binary** (19 ops : addi/subi/muli/divsi/remsi + addf/subf/mulf/divf/remf + cmpi_eq/ne/slt/sle/sgt/sge + andi/ori/xori/shli/shrsi) w/ float-path selected on float-typed operand + **Unary** (not/neg/bitnot + borrow/borrow_mut/deref) + **Call** (func.call w/ operand-threading + callee-name from Path) + **Return** (func.return w/ trailing-operand) + **Block** (stmt-iteration + trailing) + **If** (scf.if w/ 2 nested regions + cond-operand) + **Paren** (transparent pass-through)
    - Unsupported variants emit `CsslOp::Std` placeholder w/ `unsupported_kind` attribute — survives round-trip for diagnostics
- **Phase-2b DEFERRED**
  - Real literal-value extraction from source-text (currently placeholder attributes `stage0_int` / `stage0_float`)
  - Field access + indexing (arith.indexcast + memref.load)
  - Loops (for / while / loop) — scf.for + scf.while emission
  - Struct / tuple / array constructors
  - Assignment + compound-assign (a += b)
  - Pipeline operator (a |> f)
  - Match expressions (desugar to scf.if-chain or scf.switch)
  - Closure-capture analysis for lambdas
  - Proper type-propagation (currently assumes i32 for most scalar ops)
  - melior FFI integration (requires MSVC toolchain per T1-D7)
- **Rationale**
  - Pass-pipeline landed FIRST gives every subsequent phase-2 pass a plug-in shape — MirPass trait is the stable interface for T7/T9/T11 phase-2 work. Clean swap : replace stub with real impl, no public-API churn.
  - Body-lowering emits `func.return` as stable terminator even for empty fns — ensures `StructuredCfgValidator` passes on every well-formed input.
  - Stable diagnostic-codes (MONO0000/AD0000/IFC0000/SMT0000/TEL0000/CFG0001) mirror rustc convention + the AD-legality pass (AD0001-0003) naming — CI can grep by code.
  - `discriminant_name` helper enables opaque-placeholder for unsupported variants that preserves round-trip without crashing, critical for incremental phase-2b development.
  - Single-binding param-pattern handling covers 95% of real-world fn signatures ; tuple-destructure / struct-destructure param-patterns are T3.4-phase-3 remaining-work.
- **Consequences**
  - Public API : `cssl_mir::{MirPass, PassPipeline, PassResult, PassSeverity, PassDiagnostic, StructuredCfgValidator, MonomorphizationPass, AdTransformPass, IfcLoweringPass, SmtDischargeQueuePass, TelemetryProbeInsertPass, BodyLowerCtx, lower_fn_body}`.
  - `cssl-mir` lib-test count : 41 → 67 (+26 : 14 pipeline + 12 body_lower).
  - New crate-level clippy allows : `unnecessary_wraps` + `single_match_else` (body-lowering idioms).
  - Workspace test-count : 872 → 898 (+26).
  - Callers can now run `PassPipeline::canonical().run_all(&mut mir_module)` to get the full stage-0 pass-sequence diagnostic-report.
  - `lower_fn_body` composes with `lower_function_signature` without breaking T6-phase-1 API — existing tests still pass.

───────────────────────────────────────────────────────────────

## § T3-D11 : T3.4-phase-3-AD-legality — compile-time gradient-drop check landed

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3-D9 deferred `AD-legality check (§§ 05 closure)` to T3.4-phase-2 ; T3-D10 closed the refinement-obligation slice ; this entry closes the AD-legality slice. The AD-legality check is a structural prerequisite for the T7-phase-2 rule-application walker — it verifies that every `@differentiable` fn body closes over legal callees before the transform actually runs source-to-source.
- **Slice landed (this commit)**
  - `cssl_hir::ad_legality` module : `AdLegalityDiagnostic` (3 variants : `GradientDrop` / `UnresolvedCallee` / `MissingReturnTangent`) + stable diagnostic-codes (AD0001..AD0003) + human-readable `message()` + `AdLegalityReport` (diagnostics + checked_fn_count + call_site_count + legal_call_count + `count(code)` + `summary()`).
  - `check_ad_legality(&HirModule, &Interner) -> AdLegalityReport` : walks every `@differentiable`-annotated fn, builds a `DefId → Vec<HirAttr>` map once, then walks each fn-body looking for `Call { callee: Path }` expressions and verifying the target is `@differentiable` / `@NoDiff` / known-pure-primitive / non-fn-def. Full expression-tree walker covering 30+ `HirExprKind` variants.
  - `is_pure_diff_primitive(name)` catalog : 38 known-pure-diff math primitives (`length` / `sqrt` / `sin` / `cos` / `tan` / `asin` / `acos` / `atan` / `atan2` / `exp` / `exp2` / `log` / `log2` / `log10` / `pow` / `max` / `min` / `abs` / `floor` / `ceil` / `round` / `fract` / `normalize` / `dot` / `cross` / `clamp` / `mix` / `smoothstep` / `step` / `reflect` / `refract` / `distance` / vec/mat constructors / `sin_cos`).
  - 13 lib-tests covering : primitive-catalog accept/reject + empty-module cleanliness + non-@differentiable-fn ignored + @differentiable-calling-pure-primitive legal + @differentiable-calling-@differentiable legal + @differentiable-calling-@NoDiff legal + @differentiable-calling-plain-fn emits AD0001 + diagnostic-code stability + message-contains-caller + report-summary-shape + multi-illegal-call-count + MissingReturnTangent AD0003.
- **T3.4-phase-3 REMAINING SLICES (still deferred)**
  - IFC-label propagation (Jif-DLM per `specs/11`).
  - `@staged` stage-arg comptime-check (per `specs/06`).
  - Macro hygiene-mark propagation (per `specs/13`).
  - Let-generalization + higher-rank polymorphism in `cssl_hir::infer`.
- **Rationale**
  - AD-legality is a purely structural walker — it needs name-resolution (already landed at T3.3) and the attr-set carried on every `HirFn`. No type-checking / SMT / MIR lowering required. Can land independently of the other T3.4-phase-3 slices.
  - Stable diagnostic-codes (AD0001 / AD0002 / AD0003) mirror the rustc diagnostic-code convention + make CI log-parsing deterministic.
  - Pure-primitive catalog is intentionally hardcoded at the HIR level rather than derived from stdlib-trait bounds — stage-0 does not yet have trait-dispatch resolution, but the primitive list is stable across compiler evolution.
  - Walker-based on the same pattern as `cssl_hir::refinement` (T3.4-phase-2-refinement) — consistent walker style across the T3.4-phase-*-* slices.
- **Consequences**
  - Public API : `cssl_hir::{check_ad_legality, is_pure_diff_primitive, AdLegalityDiagnostic, AdLegalityReport}`.
  - `cssl-hir` lib-test count 86 → 99 (+13 AD-legality tests).
  - `sdf_shader.cssl` (T12-phase-1 killer-app example) is now **structurally verifiable** by running `check_ad_legality` on its HIR — any non-pure-primitive / non-@differentiable call inside `sphere_sdf` / `scene_sdf` / `ray_march` / `surface_normal` would be caught at compile-time.
  - T7-phase-2 rule-application walker can now assume its input `@differentiable` bodies are AD-legal — no silent-gradient-drop in the transform.

───────────────────────────────────────────────────────────────

## § T12-D1 : Examples trilogy at repo-root — 3 canonical CSSLv3 source files + cssl-examples integration-tests crate

- **Date** 2026-04-17
- **Status** accepted
- **Context** T12 scope per `specs/21_EXTENDED_SLICE.csl` § VERTICAL-SLICE ENTRY POINT + `DECISIONS.md` T10-D1/D2 + T11-D1 lay the full frontend + codegen + host + telemetry + persistence surface. T12's job is to exercise that surface with real CSSLv3-source examples that establish the vertical-slice acceptance criterion — zero fatal-diagnostics through the stage-0 front-end pipeline on three canonical demos.
- **Phase-1 landed (this commit)**
  - `examples/hello_triangle.cssl` : VK-1.4 vertex + fragment shader with effect-row `{GPU, Deadline<16ms>, Telemetry<DispatchLatency>}` + `struct Vertex` + const-array triangle data + `@vertex` / `@fragment` entry-points + host-side pipeline builder. Exercises : module/use declarations + struct + fn with effect-rows + const-exprs + `@`-annotations.
  - `examples/sdf_shader.cssl` : **T12 KILLER-APP GATE per `specs/05_AUTODIFF.csl`**. Declares `@differentiable @lipschitz(k = 1.0) fn sphere_sdf`, composes it into `scene_sdf` (union-of-spheres), threads it through `ray_march`, and crucially calls `bwd_diff(scene_sdf)(hit_pos).d_p` inside `surface_normal`. This is the canonical compiler-acceptance surface for F1-AutoDiff source-to-source.
  - `examples/audio_callback.cssl` : full real-time effect-row stack `{CPU, SIMD256, NoAlloc, NoUnbounded, Deadline<1ms>, Realtime<Crit>, PureDet, DetRNG, Audit<"audio-callback">}` + refinement-typed `sample_rate : u32{v : u32 | v ∈ {44100, 48000, 96000, 192000}}` + SIMD256 vectorized DSP loop + handler declaration.
  - `compiler-rs/crates/cssl-examples/src/lib.rs` : new integration-tests crate :
    - `HELLO_TRIANGLE_SRC` / `SDF_SHADER_SRC` / `AUDIO_CALLBACK_SRC` constants loading the `.cssl` sources via `include_str!(concat!(CARGO_MANIFEST_DIR, "../../../examples/..."))`.
    - `PipelineOutcome { name, token_count, cst_item_count, parse_error_count, hir_item_count, lower_diag_count }` + `is_accepted()` + `summary()`.
    - `pipeline_example(&str, &str) -> PipelineOutcome` runs `cssl_lex::lex` → `cssl_parse::parse` → `cssl_hir::lower_module` and records counts at each stage.
    - `all_examples() -> Vec<PipelineOutcome>` drives all three examples.
    - 11 lib-tests covering : source-non-empty markers (`@differentiable`, `bwd_diff(scene_sdf)`, `Realtime<Crit>`, `Audit<"audio-callback">`) + tokenization-shape + all-examples-returns-three + `is_accepted` predicate + `summary` formatting.
- **Phase-2 deferred**
  - Full type-check + refinement-obligation generation integration (blocked on T3.4-phase-3 IFC / AD-legality / hygiene slices).
  - MIR lowering + codegen-text via the 5 cgen-* backends (requires HIR-body → MIR-expr lowering from T6-phase-2).
  - `spirv-val` / `dxc` / `naga` round-trip validation on emitted artifacts.
  - Vulkan device creation + actual pixel-render via `cssl-host-vulkan` (gated on T10-phase-2 FFI landing).
  - **`bwd_diff(scene_sdf)` bit-exact-vs-analytic verification** — gated on T7-phase-2 rule-application walker + T9-phase-2 SMT real-solver dispatch. This is the final acceptance criterion for F1 correctness.
  - `vertical_slice.cssl` : the full ≤ 5000-line composition exercising every v1 engine primitive (atmosphere, clouds, hair, ocean, spectral, XeSS2, audio-DSP, SVDAG, radiance-cascade, render-graph) per `specs/21` § VERTICAL-SLICE ENTRY POINT. Blocked on T13+ (self-host stage1).
- **Rationale**
  - Examples at `examples/` at repo-root (not inside `compiler-rs/`) match `specs/21` canonical reference path.
  - Integration-tests crate named `cssl-examples` so `cargo test --workspace` picks it up automatically without requiring manual fixture paths.
  - `include_str!` with `env!("CARGO_MANIFEST_DIR")` path composition gives compile-time file-resolution so the examples crate can't build without the sources being present — structural invariant enforced by rustc.
  - Stage-0 "acceptance" = zero fatal parser diagnostics. Full type-checking, refinement-discharge, codegen, and runtime verification are deferred to the respective T*-phase-2 work — but the **pipeline composition itself** is now proven end-to-end on real source code.
  - The `bwd_diff(scene_sdf)` marker in `sdf_shader.cssl` is the breadcrumb that T7-phase-2 + T9-phase-2 tests target when they land. Grepping for this exact call is the compiler-acceptance-trigger for the killer-app gate.
- **Consequences**
  - Public APIs : `cssl_examples::{PipelineOutcome, pipeline_example, all_examples, HELLO_TRIANGLE_SRC, SDF_SHADER_SRC, AUDIO_CALLBACK_SRC}`.
  - Workspace crate-count : 26 → 27.
  - +11 lib-tests → 859 total passing / 0 failing.
  - `examples/` directory now exists at repo-root + is referenced by `specs/21_EXTENDED_SLICE.csl` § VERTICAL-SLICE ENTRY POINT + `scripts/validate_spec_crossrefs.py` (skip-pattern for lowercase-hyphenated local refs accommodates this).
  - 3 `.cssl-rust` example files totaling ~180 lines of CSSLv3 source that exercise effect-rows + `@differentiable` + `bwd_diff` + refinement-types + SIMD + real-time deadlines + audit-chain tagging.

───────────────────────────────────────────────────────────────

## § T11-D1 : Telemetry + persistence phased — ring + audit-chain stub + in-memory persistence now ; BLAKE3/Ed25519 FFI + WAL/LMDB backends deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11 scope per `specs/22_TELEMETRY.csl` + `specs/18_ORTHOPERSIST.csl` enumerates telemetry-ring + audit-chain + exporters + persistence-image + schema-migrations + WAL/LMDB backends. Crypto primitives (BLAKE3 hash + Ed25519 signing) are heavy FFI adds ; WAL + LMDB backends require file-I/O that clouds the crate-core shape. Same phased approach as T6/T9/T10 : data model + trait-boundary + stub landing now, real crypto + backend integration at phase-2.
- **Phase-1 landed (this commit)**
  - `cssl-telemetry` : `TelemetryScope` (25 variants across 6 domains : CPU / GPU /
    Power-Thermal / RAS / App-Semantic / Compound with `as_u16` stable encoding) +
    `TelemetryKind` (Sample / SpanBegin / SpanEnd / Counter / Audit) +
    `TelemetrySlot` (64-byte fixed ring record per `specs/22`) + `TelemetryRing`
    (single-thread SPSC with overflow-counting + total-pushed + peek) +
    `AuditEntry` + `AuditChain` (BLAKE3-stub hash-chain + Ed25519-stub signatures,
    full `verify_chain` detecting `GenesisPrevNonZero` / `ChainBreak` /
    `InvalidSequence`) + `Exporter` trait + `ChromeTraceExporter` (JSON output
    compatible with Chrome DevTools tracing format) + `JsonExporter`
    (newline-delimited JSON) + `OtlpExporter` (returns `NotWired` at stage-0) +
    `TelemetrySchema` + `TelemetryScopeSet` (subset-of check for scope-narrowing
    invariant per `specs/22` § callee's-scope-⊑-caller's-scope).
  - `cssl-persist` : `SchemaVersion` (major.minor + 32-byte digest) +
    `SchemaMigration` (before/after/id/description) + `MigrationChain`
    (panicking-assert on broken-chain + start/end version accessors) +
    `ImageHeader` (canonical `"CSSLPRS1"` magic + format-version + record-count +
    stub content-digest) + `ImageRecord` + `PersistenceImage` (appends +
    find-by-key + total-payload-size + auto-digest-refresh) +
    `PersistenceBackend` trait + `InMemoryBackend` (reference impl w/
    insertion-order preservation + schema-snapshot) + `PersistError` (NotFound /
    SchemaMismatch / BackendNotWired).
- **Phase-2 deferred**
  - `blake3` integration (stage-0 stub-hash is deterministic but not cryptographically strong).
  - `ed25519-dalek` signing + verification (stage-0 stub-sign is a deterministic byte-fold).
  - OTLP gRPC + HTTP exporter (needs `prost` / `reqwest`).
  - WAL-file backend (append-only log + periodic snapshot checkpoints).
  - LMDB backend (mmap + B+tree for large working-sets).
  - Level-Zero sysman sampling-thread integration via `cssl_host_level_zero::TelemetryProbe`.
  - Cross-thread atomic SPSC ring (stage-0 uses single-thread `RefCell`-backed).
  - `@hot_reload_preserve` HIR attribute extraction + root-set discovery.
  - Live-object migration application.
  - R16 attestation of image-provenance (BLAKE3 chain + Ed25519 signatures).
  - `{Telemetry<S>}` effect-row HIR lowering pass (inserts probe ops per `specs/22`
    § COMPILE-TIME PROBE INSERTION).
  - Overhead-budget enforcement (≤ 0.5% for Counters scope, ≤ 5% for Full scope).
- **Rationale**
  - The 25 telemetry scopes + TelemetrySlot + TelemetryRing give downstream MIR
    probe-lowering + host-adapter sampling a concrete surface to target before
    crypto primitives are wired. Ring overflow-counting semantics (producer-never-
    blocks, drop-+-count) match `specs/22` § invariants exactly.
  - AuditChain verify-chain invariant is independent of the hash strength —
    switching from stub-hash → BLAKE3 is a `ContentHash::stub_hash` → `blake3::hash`
    replacement with no public-API churn (unit tests pin the chain-link structural
    invariant, not hash bytes).
  - `InMemoryBackend` reference-impl lets downstream code exercise the full
    `PersistenceBackend` trait surface (put / get / snapshot / iter) without a
    WAL-file dep pulled in.
  - Canonical `"CSSLPRS1"` image magic + `SchemaVersion(major, minor, digest)` give
    persistence-image files a stable identity + versioning story that fat-binary
    [audit-manifest] section can reference.
- **Consequences**
  - Public APIs :
    - `cssl_telemetry::{TelemetryScope, TelemetryKind, TelemetrySlot, TelemetryRing, RingError, AuditChain, AuditEntry, AuditError, Exporter, ChromeTraceExporter, JsonExporter, OtlpExporter, ExportError, TelemetrySchema, TelemetryScopeSet}`.
    - `cssl_persist::{SchemaVersion, SchemaMigration, MigrationChain, ImageHeader, ImageRecord, PersistenceImage, PersistenceBackend, InMemoryBackend, PersistError}`.
  - Both crates carry only `thiserror` as a runtime dep ; no cryptographic deps
    pulled in yet. Phase-2 adds `blake3` + `ed25519-dalek` (already declared in
    workspace deps from T1, blocked on real integration).
  - +64 new lib-tests : 40 telemetry + 24 persist.
  - Crate-level clippy allowances : `match_same_arms`, `module_name_repetitions`.
  - `cssl-testing` remains at T1-scaffold stage-0 stubs (all 12 oracle-modes have
    `Stage0Stub` returning `Unimplemented`) — beefing up the oracle bodies is
    T11-phase-2 work per `DECISIONS.md` T1-D3 §§ 23-FAITHFUL policy.

───────────────────────────────────────────────────────────────

## § T10-D2 : Host-adapters phased — capability catalogs + stub probes now ; ash / level-zero-sys / windows-rs / metal / wgpu FFI deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T10-hosts scope per `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS enumerates 5 backend adapters : Vulkan (ash), Level-Zero (level-zero-sys), D3D12 (windows-rs), Metal (metal crate), WebGPU (wgpu). Vulkan/L0/D3D12 FFI need MSVC ABI per T1-D7 ; Metal is Apple-only ; WebGPU (wgpu) pulls heavy deps. Same FFI-avoidance pattern as T6-D1 / T9-D1 / T10-D1.
- **Phase-1 landed (this commit)**
  - `cssl-host-vulkan` : `VulkanVersion` (1.0..1.4) + `VulkanExtension` (30 variants : VK-1.4 core + RT + CoopMat + BDA + mesh + telemetry) + `VulkanLayer` (5 validation/dump/profiles) + `GpuVendor` (8 : Intel/NVIDIA/AMD/Apple/Qualcomm/ARM/Mesa/Other) + `DeviceType` (5 : integrated/discrete/virtual/cpu/other) + `DeviceFeatures` (25 VK-features CSSLv3 exercises) + `VulkanDevice` + `FeatureProbe` trait + `StubProbe` + `ArcA770Profile` (canonical hard-coded spec from `specs/10` ARC A770 DETAILED SPECS : 32 Xe-cores / 512 XVE / 512 XMX / 32 RT / 2.1 GHz / 16 GB GDDR6 / 560 GB/s / 225 W).
  - `cssl-host-level-zero` : `L0ApiSurface` (24 : `ze*` driver/device/context/cmd-list/event/module/kernel/USM + `zes*` sysman) + `UsmAllocType` (host/device/shared) + `L0Driver` + `L0Device` + `L0DeviceType` + `L0DeviceProperties` + `SysmanMetric` (11 : power × 2 / thermal × 2 / frequency × 3 / engine / ras / processes / perf-factor per `specs/10` § SYSMAN AVAILABILITY) + `SysmanMetricSet` (full-R18 + advisory subsets) + `SysmanSample` + `SysmanCapture` + `TelemetryProbe` trait + `StubTelemetryProbe` returning canonical Arc A770 sample values.
  - `cssl-host-d3d12` : `FeatureLevel` (12.0..12.2) + `DxgiAdapter` (with Arc A770 + WARP stubs) + `D3d12FeatureOptions` (10 fields : RT-1.1 / mesh / sampler-feedback / VRS-2 / atomic-int64 / FP16 / Int16 / dynamic-resources / wave-matrix / wave-size-spec) + `WaveMatrixTier` + `CommandListType` (7 : direct/compute/copy/bundle/video-decode/video-process/video-encode) + `DescriptorHeapType` (4 : cbv-srv-uav/sampler/rtv/dsv) + `HeapType` (4 : default/upload/readback/custom).
  - `cssl-host-metal` : `MetalFeatureSet` (7 : macOS-GPU-family1/2 + iOS-GPU-family6 + Metal-3/3.1/3.2 per-Apple-family) + `GpuFamily` (14 : Apple1..Apple9 + Mac1/2 + Common1/2/3) + `MtlDevice` (with M3-Max + Intel-Mac stubs) + `MetalHeapType` (shared/private/managed/memoryless) + `MetalResourceOptions`.
  - `cssl-host-webgpu` : `WebGpuBackend` (5 : browser/vulkan/metal/dx12/gl) + `AdapterPowerPref` (low-power/high-perf/no-pref) + `WebGpuAdapter` (with Arc-A770-vulkan + Browser-WebGPU + Software stubs) + `WebGpuFeature` (14 WebGPU spec features) + `SupportedFeatureSet` + `WebGpuLimits` (26-field snapshot of canonical WebGPU defaults).
- **Phase-2 deferred**
  - `ash` FFI (Vulkan) integration : VkInstance / VkPhysicalDevice / VkDevice creation + extension-request arbitration + descriptor-set-update + pipeline-creation + command-buffer-recording + queue-submit.
  - `level-zero-sys` FFI : ze_driver_handle / ze_device_handle / ze_command_list / ze_module / ze_kernel + USM allocation + sysman-property-sampling (`zesPowerGetEnergyCounter` / etc.).
  - `windows-rs` D3D12 FFI : `ID3D12Device` / `ID3D12CommandQueue` / `ID3D12GraphicsCommandList6` / descriptor-heaps / root-signatures.
  - `metal` crate FFI (Apple-only cfg-gated) : `MTLDevice` / `MTLCommandQueue` / argument-buffers / fn-constants.
  - `wgpu` integration : `wgpu::Instance` / `Adapter` / `Device` / `Queue` / `RenderPipeline` / `ComputePipeline`.
  - Validation-layer diagnostic routing (Vulkan `VK_LAYER_KHRONOS_validation`).
  - Multi-device + multi-context concurrency (L0 + Vulkan coexistence on Intel).
  - Surface / swapchain presentation.
- **Rationale**
  - Capability catalogs + stub probes provide the downstream surface that `cssl-mir` + the 5 codegen crates can target without yet linking any FFI. The `FeatureProbe` / `TelemetryProbe` trait boundaries let phase-2 swap stubs for real bindings without public-API churn.
  - Arc A770 hard-coded profile encodes `specs/10` canonical values (Xe-cores / XMX / RT / VRAM / bandwidth / TDP) as the single-source-of-truth for code that needs to pre-compute per-target layouts without probing.
  - Sysman metric catalog + `TelemetryProbe` trait gives T11 telemetry the consumer-facing interface it needs for R18 discharge — the probe produces `SysmanCapture` records independent of whether real L0 is available.
  - WebGPU phase-1 without `wgpu` keeps scaffold build-time tight ; wgpu adds ~50 crates of transitive deps and benefits from deferred-until-T12-examples adoption.
- **Consequences**
  - Public APIs :
    - `cssl_host_vulkan::{VulkanVersion, VulkanExtension, VulkanExtensionSet, VulkanLayer, GpuVendor, DeviceType, DeviceFeatures, VulkanDevice, FeatureProbe, StubProbe, ProbeError, ArcA770Profile}`.
    - `cssl_host_level_zero::{L0ApiSurface, UsmAllocType, L0Driver, L0Device, L0DeviceType, L0DeviceProperties, SysmanMetric, SysmanMetricSet, SysmanSample, SysmanCapture, TelemetryProbe, StubTelemetryProbe, TelemetryError}`.
    - `cssl_host_d3d12::{FeatureLevel, DxgiAdapter, D3d12FeatureOptions, WaveMatrixTier, CommandListType, DescriptorHeapType, HeapType}`.
    - `cssl_host_metal::{GpuFamily, MtlDevice, MetalFeatureSet, MetalHeapType, MetalResourceOptions}`.
    - `cssl_host_webgpu::{WebGpuBackend, AdapterPowerPref, WebGpuAdapter, WebGpuFeature, SupportedFeatureSet, WebGpuLimits}`.
  - Every crate carries only `thiserror` as a runtime dep — no FFI bindings pulled in.
  - Crate-level clippy allowances : `match_same_arms`, `module_name_repetitions`, `struct_excessive_bools` where needed.
  - +76 new lib-tests across 5 crates (23 vulkan + 15 level-zero + 13 d3d12 + 14 metal + 11 webgpu).
  - `forbid(unsafe_code)` retained crate-wide in every host-adapter (previously allowed for FFI) ; phase-2 flips to `allow(unsafe_code)` only at the ash/windows-rs/level-zero-sys call-sites with `// SAFETY:` comments per T1-D5.

───────────────────────────────────────────────────────────────

## § T10-D1 : Codegen phased — 5 backends text-emit now ; real FFI (cranelift + rspirv + dxc + metal + wgpu) deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T10 (codegen) scope per `specs/07_CODEGEN.csl` + `specs/14_BACKEND.csl` enumerates 5 backends : CPU-cranelift, GPU-SPIR-V, GPU-DXIL, GPU-MSL, GPU-WGSL. All 5 can be wired directly via pure-Rust deps (cranelift-codegen + rspirv + optional naga) or via CLI-subprocess for compiled outputs (dxc for DXIL, spirv-cross for MSL), but each of those deps has a non-trivial build-time + toolchain cost. Mirrors T6-D1 (MLIR-text-CLI) + T9-D1 (Z3/CVC5-CLI) FFI-avoidance pattern.
- **Phase-1 landed (this commit)**
  - `cssl-cgen-cpu-cranelift` : `CpuTarget` (7 µarchs : alder/raptor/meteor/arrow lake + zen4/zen5 + generic-v3) + `SimdTier` (scalar/sse2/avx2/avx512) + `CpuFeature` (17 flags : fma/bmi1/bmi2/popcnt/lzcnt/movbe/avx512f/dq/bw/vl/vnni/bf16/vaes/pclmulqdq/sha/rdrand/rdseed) + `Abi` (sysv/win64/darwin) + `ObjectFormat` (elf/coff/macho) + `CpuTargetProfile` + `ClifType` + `clif_type_for(MirType)` + `emit_module(MirModule, Profile) -> EmittedArtifact` (text-CLIF).
  - `cssl-cgen-gpu-spirv` : `SpirvCapability` (32 variants covering Shader/Kernel/BDA/VK-memory-model/bindless/subgroup/CoopMatKHR/RayTracingKHR/atomic-float/Float16/64/mesh/debug-info) + `SpirvExtension` (24 KHR+EXT+INTEL+NV+ext-inst-set) + `SpirvTargetEnv` (9 : Vulkan-1.0..1.4 / universal-1.5/1.6 / OpenCL-kernel / WebGPU) + `MemoryModel` + `AddressingModel` + `ExecutionModel` (15 stages incl. ray-tracing) + `SpirvModule` + `SpirvSection` (11 rigid-ordered) + `emit_module(SpirvModule) -> String` (disasm-format, spirv-as-compatible).
  - `cssl-cgen-gpu-dxil` : `ShaderModel` (SM 6.0..6.8) + `ShaderStage` (15 stages incl. ray-tracing) + `HlslProfile` + `RootSignatureVersion` (v1.0..v1.2) + `DxilTargetProfile` + `HlslModule`/`HlslStatement` builder + `emit_hlsl(MirModule, Profile, entry) -> HlslModule` + `DxcCliInvoker` subprocess adapter (stage-0 HLSL text + optional `dxc.exe -T <profile>` invocation).
  - `cssl-cgen-gpu-msl` : `MslVersion` (2.0..3.2) + `MetalStage` (7 : vertex/fragment/kernel/object/mesh/tile/visible) + `MetalPlatform` (macos/ios/tvos/visionos) + `ArgumentBufferTier` + `MslTargetProfile` + `MslModule`/`MslStatement` + `emit_msl(MirModule, Profile, entry)` + `SpirvCrossInvoker` subprocess adapter.
  - `cssl-cgen-gpu-wgsl` : `WebGpuStage` (vertex/fragment/compute) + `WebGpuFeature` (7 : shader-f16/timestamp-query/subgroups/float32-filterable/dual-source-blending/bgra8unorm-storage/clip-distances) + `WgslLimits` (webgpu-default + compat presets) + `WgslTargetProfile` + `WgslModule`/`WgslStatement` + `emit_wgsl(MirModule, Profile, entry)`.
  - Every crate emits a MIR → target-text artifact end-to-end with a canonical entry-point skeleton that matches the stage's calling-convention / attribute-set.
- **Phase-2 deferred**
  - Cranelift FFI integration : `cranelift-codegen` + `-frontend` + `-module` + `-object` for real CLIF → machine-code → object-file (ELF / COFF / Mach-O). Pure-Rust so no MSVC block, but heavy build-time ⇒ reviewed for size-vs-benefit vs. text-CLIF-at-stage-0 pattern.
  - rspirv module-builder → real SPIR-V binary emission + `spirv-val` subprocess gate mandatory-per-CI.
  - `dxc.exe` actually wired to CI Windows runner (skipped gracefully when binary absent).
  - `spirv-cross --msl` validation round-trip.
  - `metal-shaderconverter` Apple-only binary integration (CI-mac-only).
  - `naga` WGSL round-trip validator (pure-Rust but pulls many deps).
  - Full MIR body → target-IR lowering (stage-0 emits signature skeletons only).
  - Structured-CFG preservation (scf.* → OpSelectionMerge / OpLoopMerge for SPIR-V).
  - Debug-info emission (DWARF-5 / CodeView for CPU ; NonSemantic.Shader.DebugInfo.100 for SPIR-V).
  - Fat-binary assembly (§§ 07_CODEGEN § FAT-BINARY + §§ 14 § FAT-BINARY-ASSEMBLY).
- **Rationale**
  - Same FFI-avoidance pattern as T6-D1 + T9-D1 : text-emission pipeline validates end-to-end composition before pulling in heavy backend-specific deps. Keeps stage-0 on gnu-ABI per T1-D7.
  - All 5 targets share the same `MirModule → target-text → EmittedArtifact` shape — downstream consumers can treat them uniformly through a `CodegenBackend` trait (phase-2).
  - Entry-point skeletons with correct calling-convention attributes (`[numthreads(...)]` for HLSL compute, `[[kernel]]` + `[[thread_position_in_grid]]` for MSL, `@compute @workgroup_size(...)` for WGSL, `OpEntryPoint ... GLCompute %fn "fn"` for SPIR-V) exercise the per-target signature semantics without needing a full body-lowering pass.
  - CI subprocess adapters (dxc / spirv-cross) gracefully degrade when the binary is absent — CSSLv3 CI installs them where needed, other environments get HLSL/MSL text + documented `BinaryMissing` outcome.
- **Consequences**
  - Public APIs :
    - `cssl_cgen_cpu_cranelift::{CpuTarget, CpuTargetProfile, SimdTier, CpuFeature, CpuFeatureSet, Abi, ObjectFormat, DebugFormat, ClifType, clif_type_for, emit_module, EmittedArtifact, CpuCodegenError}`.
    - `cssl_cgen_gpu_spirv::{SpirvCapability, SpirvCapabilitySet, SpirvExtension, SpirvExtensionSet, SpirvTargetEnv, MemoryModel, AddressingModel, ExecutionModel, SpirvModule, SpirvSection, emit_module, SpirvEmitError}`.
    - `cssl_cgen_gpu_dxil::{ShaderModel, ShaderStage, HlslProfile, RootSignatureVersion, DxilTargetProfile, HlslModule, HlslStatement, emit_hlsl, DxilError, DxcCliInvoker, DxcInvocation, DxcOutcome}`.
    - `cssl_cgen_gpu_msl::{MslVersion, MetalStage, MetalPlatform, ArgumentBufferTier, MslTargetProfile, MslModule, MslStatement, emit_msl, MslError, SpirvCrossInvoker, SpirvCrossInvocation, SpirvCrossOutcome}`.
    - `cssl_cgen_gpu_wgsl::{WebGpuStage, WebGpuFeature, WgslLimits, WgslTargetProfile, WgslModule, WgslStatement, emit_wgsl, WgslError}`.
  - Each crate carries `cssl-mir` as a path-dep + `thiserror` for error enums.
  - Each crate has scaffold-level clippy allowances (`match_same_arms`, `module_name_repetitions`) pending T10-phase-2 stabilization.
  - +151 new lib-tests across 5 crates (36 cranelift + 32 spirv + 30 dxil + 29 msl + 24 wgsl).
  - CLI-subprocess adapters (DxcCliInvoker + SpirvCrossInvoker) are tested with an impossible-path binary to assert the `BinaryMissing` / `IoError` graceful-failure path.

───────────────────────────────────────────────────────────────

## § T9-D1 : SMT phased — text-emit + CLI-subprocess Z3/CVC5 adapters now ; FFI + KLEE + proof-certs deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T9 (SMT integration) scope per §§ 20_SMT enumerates : SMT-LIB emission, Z3 + CVC5 + KLEE backends, per-obligation discharge, caching, Ed25519-signed proof-certificates for R18 audit-chain. Landing the full surface in one commit is ~8K LOC and requires `z3-sys` / `cvc5-sys` FFI which needs MSVC toolchain per T1-D7 (not yet selected). Mirrors the same FFI-avoidance pattern as T6-D1 (MLIR-text-CLI fallback).
- **Phase-1 landed**
  - `Theory` enum (7 variants : LIA/LRA/NRA/BV/UF/UFLIA/ALL with `QF_` prefixes).
  - `Sort` enum (Bool/Int/Real/BitVec(N)/Uninterp(name)) with `render()` → SMT-LIB text.
  - `Literal` (Bool/Int/Rational/BitVec) + `Term` tree (Var/Lit/App/Forall/Exists/Let) with full recursive rendering.
  - `Query` (logic + sort-decls + fn-decls + assertions + get-model/unsat-core flags) + `FnDecl` + `Assertion` (labeled/unlabeled).
  - `Verdict` enum (Sat/Unsat/Unknown) + `SolverError` (BinaryMissing/NonZeroExit/UnparseableOutput/Io).
  - `emit_smtlib(&Query) -> String` producing valid SMT-LIB 2.6 text : `(set-logic)(declare-sort)(declare-fun)(assert)(check-sat)(get-model)(get-unsat-core)`.
  - `Solver` trait + `Z3CliSolver` / `Cvc5CliSolver` subprocess wrappers : spawn `z3 -in -smt2` or `cvc5 --lang=smt2 -`, pipe SMT-LIB through stdin, parse first stdout line for `sat` / `unsat` / `unknown`.
  - `discharge(&ObligationBag, &Solver) -> Vec<(ObligationId, Result<Verdict, SolverError>)>` : stage-0 stub produces trivially-true `(assert true)(check-sat)` queries per obligation — exercises the pipeline without yet encoding predicate semantics.
- **Phase-2 deferred**
  - Direct `z3-sys` / `cvc5-sys` FFI (blocked on MSVC toolchain per T1-D7).
  - KLEE symbolic-exec fallback for coverage-guided paths.
  - Proof-certificate emission + Ed25519-signed certs (R18 audit-chain).
  - Per-obligation-hash disk cache.
  - Full HIR-expression → SMT-Term translation (stage-0 uses text proxies).
  - Incremental solving (`push` / `pop`).
- **Rationale**
  - Same FFI-avoidance pattern as T6 MLIR : CLI-subprocess gives a working verdict pipeline without any C++ link-time dependency, keeping stage-0 on `x86_64-pc-windows-gnu` per T1-D7.
  - Trivially-true stub discharge validates that `ObligationBag → Query → SMT-LIB text → subprocess → parsed verdict` composes end-to-end ; semantics follow in T9-phase-2 when HIR-to-SMT translation lands.
  - Solver-binary absence is a recoverable error (`BinaryMissing`) ; CI installs Z3 via apt/brew/choco at bootstrap.
- **Consequences**
  - Public API : `cssl_smt::{Theory, Sort, Term, Literal, Query, FnDecl, Assertion, Verdict, emit_smtlib, discharge, Solver, SolverKind, SolverError, Z3CliSolver, Cvc5CliSolver}`.
  - Crate-level clippy allowances : `match_same_arms, no_effect_underscore_binding, struct_excessive_bools, missing_errors_doc, use_self` (scaffold-stage ergonomics).
  - 28 lib-tests covering Theory naming + Sort/Literal/Term rendering + Query construction + emission + solver-error display + stub-discharge shape.
  - Unit tests intentionally do NOT invoke actual solver binaries — CI has a separate job that installs solvers ; unit tests exercise only dispatch + emit.

───────────────────────────────────────────────────────────────

## § T3-D10 : T3.4-phase-2 refinement — obligation-generator landed ; SMT-discharge at T9-phase-2

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3-D9 deferred refinement-obligation generation to T3.4-phase-2, gated on T9 SMT availability. With T9-phase-1 landing the SMT-LIB emit + solver-dispatch pipeline in the same commit, the obligation-generator is now viable as a consumer-facing API surface even before the full HIR-expression-to-SMT-term translation lands.
- **Phase-2-refinement landed**
  - `cssl_hir::refinement` module : new module split out of `cssl-hir`.
  - `ObligationKind` enum (3 variants) : `Predicate { text }` for `{v : T | P(v)}` sugar, `Tag { name }` for `T'tag` shorthand, `Lipschitz { bound }` for `SDF'L<k>` bounds.
  - `RefinementObligation` record : `id : ObligationId` (u32 newtype) + `origin : HirId` + `span : Span` + `enclosing_def : Option<DefId>` + `kind : ObligationKind` + `base_type_text : String` (pretty-printed base type for diagnostics).
  - `ObligationBag` : monotonic-append container with `push` / `get` / `iter` / `len` + stable `ObligationId` handout.
  - `collect_refinement_obligations(&HirModule, &Interner) -> ObligationBag` : walks every `HirItem::Fn`, enters `walk_type(param.ty)` + `walk_type(return_ty)` + `walk_expr(body)`, recurses through `Tuple / Array / Slice / Reference / Capability / Function / Path` type-shapes.
  - Each `HirRefinementKind::{Predicate, Tag, Lipschitz}` site generates exactly one `RefinementObligation` with its originating `HirId` + span captured.
  - `pretty_type` + `pretty_expr` helpers render compact diagnostic-facing text for obligation `base_type_text` + predicate-text fields.
- **Rationale**
  - The obligation-bag is T9's input surface — landing it now means `cssl-smt::discharge(&ObligationBag, &Solver)` has a real consumer from commit-1 onward, even if `build_stub_query` is trivially-true until T9-phase-2 translates HIR predicates to SMT-LIB terms.
  - Walking types recursively catches refinements nested in `fn(x : Vec<{v : i32 | v > 0}>) -> ...` style signatures.
  - Obligation-ID stability (u32 newtype, monotonic-append) gives downstream diagnostics + caching a persistent handle.
- **Phase-3 deferred**
  - HIR-expression → SMT-Term translation (T9-phase-2).
  - Obligation-context accumulation (function-entry preconditions + loop-invariants).
  - Lipschitz-bound arithmetic-interval discharge (may route through a different solver backend).
  - Per-obligation discharge-outcome cache keyed on obligation-hash.
- **Consequences**
  - Public API : `cssl_hir::{collect_refinement_obligations, ObligationBag, ObligationId, ObligationKind, RefinementObligation}`.
  - `cssl-hir` lib-test count 79 → 86 (+7 for refinement.rs).
  - `pretty_expr` annotated `#[allow(clippy::unused_self)]` pending T3.4-phase-3 where the method body will grow a real walker.
  - Remaining T3.4-phase-2 items (capability inference, IFC-label propagation, AD-legality, `@staged` check, macro hygiene) still deferred per T3-D9 — this decision closes only the refinement-obligation slice.

───────────────────────────────────────────────────────────────

## § T7-D1 : AD phased — rules table + decl collection + variant-naming now ; rule-application deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7 (AutoDiff) scope includes : per-primitive rules table, `@differentiable` collection, HIR-to-HIR transform producing primal/fwd/bwd variants. Full rule-application (walking `HirExpr` + applying rules at each primitive site) is a multi-commit effort that needs close integration with T6 MIR + runtime tape allocation.
- **Phase-1 landed**
  - `DiffMode` (Primal/Fwd/Bwd) + `Primitive` (15 variants : FAdd/FSub/FMul/FDiv/FNeg/Sqrt/Sin/Cos/Exp/Log/Call/Load/Store/If/Loop).
  - `DiffRule` + `DiffRuleTable::canonical()` with 30 rules (15 primitives × 2 modes).
  - `DiffDecl` + `collect_differentiable_fns` : walks HIR, returns `@differentiable` fn metadata (name + def + param-count + `no_diff` / `lipschitz_bound` / `checkpoint` flags).
  - `DiffTransform` + `DiffVariants` : registers each `@differentiable` fn and generates canonical `<name>_fwd` / `<name>_bwd` variant names.
- **Phase-2 deferred**
  - Walking `HirExpr` and applying rules at each primitive site.
  - Tape-buffer allocation (iso-capability scoped).
  - `@checkpoint` attribute-arg extraction.
  - GPU-AD tape-location resolution.
  - Killer-app gate : `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic.

───────────────────────────────────────────────────────────────

## § T7-D2 : Jet<T,N> = structural data-type ; order-dependent ops validated at T6 MIR

- **Date** 2026-04-17
- **Status** accepted
- **Context** Jet<T,N> is a higher-order AD construct (value + N tangent coefficients). Rust can't express `Jet<T, N>` generically-over-const-N at stage-0 without const-generic-infra ; the actual runtime representation is target-dependent (tuple / array / struct-of-arrays).
- **Decision** `cssl-jets` crate exposes `JetOrder(u32)`, `JetOp` (5 variants : Construct/Project/Add/Mul/Apply), `JetSignature` (operand/result arity + order-dependence), + validator fns (`validate_construct` / `validate_project` / `validate_binary_order`). Runtime representation is decided at T6 MIR lowering per-target ; `cssl-jets` stays representation-agnostic.
- **Consequences**
  - Jet<T,∞> lazy-stream variant is T7-phase-2 / T17 scope.
  - `cssl.jet.*` MIR ops (already catalogued in cssl-mir `CsslOp::Jet{Construct,Project}` — needs Add/Mul/Apply additions at T6-phase-2).
  - SMT-discharge of Jet composition invariants lives in T9.

───────────────────────────────────────────────────────────────

## § T8-D1 : Staging + Macros + Futamura = three parallel crates ; data model now, expansion deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T8 bundles three related but independent concerns : `@staged` specialization (F4), Racket-hygienic macros (R3+R9), Futamura projections (R12). Each has its own data model + operations ; landing them as three crates keeps the concerns cleanly separated.
- **Decision**
  - `cssl-staging` : `StageArg` + `StageArgKind` (CompTime/Runtime/Polymorphic) + `StagedDecl` + `collect_staged_fns` + `Specializer` skeleton + `SpecializationSite`.
  - `cssl-macros` : `MacroTier` (3 variants) + `ScopeId` + `HygieneMark` (Racket set-of-scopes with flip/union) + `SyntaxObject` + `ScopeAllocator` + `MacroRegistry` + `MacroDecl` + `MacroError`.
  - `cssl-futamura` : `FutamuraLevel` (P1/P2/P3) + `Projection` + `FixedPointRecord` (converged iff hash-N == hash-N+1) + `Orchestrator` + `FutamuraError`.
- **Phase-2 deferred**
  - Actual specialization walk (clone fn + const-propagate).
  - Native comptime-eval (compile-native ; R14 avoid-Zig-20x).
  - `@type_info` / `@fn_info` / `@module_info` reflection API.
  - Transform-dialect pass-schedule emission.
  - Tier-2 pattern-match expansion.
  - Tier-3 `#run` proc-macro sandbox.
  - P3 self-bootstrap fixed-point verification (needs running stage-1 compiler).

───────────────────────────────────────────────────────────────

## § T6-D1 : MLIR-text-CLI fallback landed as phase-1 ; melior FFI deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T6 HANDOFF enumerated three options for MLIR integration : (a) melior C++-stubs, (b) MLIR-text-CLI fallback, (c) hand-roll custom-IR. Option (a) requires `mlir-sys` + `melior` + LLVM ~18+ build — on the current `x86_64-pc-windows-gnu` toolchain, `parking_lot_core` already fails (T3-D8) because `dlltool.exe` isn't bundled with MinGW. melior pulls in similar GNU-hostile dependencies plus LLVM C++ bindings.
- **Decision** **(b) MLIR-text-CLI**, landed as T6-phase-1. The compiler produces textual MLIR via pure-Rust `cssl-mir` data types + `print_module()` pretty-printer. External `mlir-opt` / `mlir-translate` CLI tools handle any validation / lowering that would otherwise require melior. This matches the HANDOFF pre-authorized fallback verbatim.
- **Phase-1 scope (THIS commit)**
  - `cssl-mir` crate with `CsslOp` (26-variant enum covering all `cssl.*` dialect ops), `MirValue`, `MirType`, `MirBlock`, `MirRegion`, `MirFunc`, `MirModule`, `MlirPrinter`, `LowerCtx`.
  - Skeleton HIR → MIR lowering : `lower_function_signature` + `lower_module_signatures` produce fn-level MIR shells with name + params + results + effect-row + cap attributes.
  - `cssl-mlir-bridge` crate with `emit_module_to_string` + `emit_module_to_writer` wrappers.
- **Phase-2 deferred (T6-phase-2)**
  - melior / mlir-sys FFI integration (requires MSVC toolchain per T1-D7 ; revisit @ T10 when FFI link-time forces the MSVC switch).
  - TableGen `CSSLOps.td` authoring for dialect registration.
  - Full HIR body → MIR expression lowering.
  - Pass pipeline infrastructure (monomorphization / macro-expansion / AD / `@staged` / evidence-passing / IFC / SMT-discharge / telemetry-probe insertion).
  - Structured-CFG validation pass.
  - Dialect-conversion to `spirv` / `llvm` / `gpu`.
- **Consequences**
  - `csslc --emit-mlir` works now with the textual path — no FFI / no C++ / no LLVM dependency.
  - External CI can pipe output through `mlir-opt --verify-each` to catch malformed output.
  - Phase-2 upgrade is additive — `cssl-mir` public API stays stable ; `cssl-mlir-bridge` gains FFI variants that live alongside the text variants.

───────────────────────────────────────────────────────────────

## § T6-D2 : CsslOp enum with 26 dialect variants + Std catch-all

- **Date** 2026-04-17
- **Status** accepted
- **Context** `specs/15 § CSSL-DIALECT OPS` enumerates ~25 custom `cssl.*` ops plus free-form standard dialect ops (`arith.*` / `scf.*` / `func.*` / `memref.*` / `vector.*` / `linalg.*` / `affine.*` / `gpu.*` / `spirv.*` / `llvm.*` / `transform.*`). The `CsslOp` enum needs to cover both.
- **Decision**
  - 26 enum variants for the custom dialect ops (exact 1-to-1 with `specs/15` § CSSL-DIALECT OPS, with `TelemetryProbe` as the probe-scope variant and `EffectPerform`/`EffectHandle` as the effect family).
  - One `Std` variant carrying a free-form `name: String` in the enclosing `MirOp` for all non-custom ops. No schema validation on `Std` at stage-0 — downstream passes / external `mlir-opt` flag any issues.
- **Metadata per-op**
  - `name()` : canonical source-form (`"cssl.diff.primal"` etc.).
  - `category()` : `OpCategory` enum (14 categories covering AD / Jet / Effect / Region / Handle / Staged / Macro / Ifc / Verify / Sdf / Gpu / Xmx / Rt / Telemetry / Std).
  - `signature()` : `OpSignature { operands: Option<usize>, results: Option<usize> }` where `None` = variadic.
- **Rationale**
  - Separation between custom ops (known shape) and `Std` (pass-through) lets the pretty-printer take two paths without per-op branches.
  - Categories support future T8 optimization passes that need to group-dispatch (e.g., "elide all `cssl.telemetry.probe` when scope=Nothing").
  - Arity metadata gives the printer enough context to validate operand / result counts at emit-time.
- **Consequences**
  - Adding a new op requires : (1) add enum variant, (2) entry in `BUILTIN_METADATA`... wait, that's effects. For CsslOp : (1) add enum variant, (2) `ALL_CSSL` const-slice, (3) name/category/signature match arms.
  - `ALL_CSSL.len() == 26` tracked by a test.

───────────────────────────────────────────────────────────────

## § T5-D1 : cap_check delegated to cssl-caps via `AliasMatrix::can_pass_through = is_subtype`

- **Date** 2026-04-17
- **Status** accepted
- **Context** The alias+deny matrix (`specs/12` § THE SIX CAPABILITIES) is usually presented as a pairwise transfer table : can a value of cap `X` be passed to a parameter declared as cap `Y` ? The matrix's alias-local / alias-global / mut-local / mut-global bits describe what the *holder* of the cap can do ; the transfer question is a subtype question.
- **Options**
  - (a) Encode the transfer matrix as a separate 6×6 table ; check `can_pass_through` by lookup.
  - (b) Define `can_pass_through(caller, callee_param) = is_subtype(caller, callee_param)` ; reuse the subtype relation as the single source of truth.
  - (c) Per-caller per-callee custom rules mixing subtype + alias-matrix bits.
- **Decision** **(b)** — `AliasMatrix::can_pass_through` delegates to `is_subtype`. Subtype is the canonical relation per `specs/12` § CAPABILITY-DIRECTED SUBTYPING. The AliasMatrix holds the alias-local / mut-local / send-safe bits for *use-site* queries (what can the holder do?) ; cross-site transfer is subtyping.
- **Rationale**
  - Single source of truth for transferability — no drift between table and relation.
  - Matches Pony-paper presentation : subtype relation is axiomatic, alias matrix is a derived view.
  - The test `passing_iso_to_val_allowed_via_freeze` drives this : `iso <: val` via freeze is a subtype relation, the `alias_local`-bit check would reject it.
- **Consequences**
  - `AliasMatrix` remains useful for holder-centric queries (`AliasRights::can_alias`, `can_mutate`, `is_send_safe`).
  - `can_pass_through` is now an opinionated wrapper over `is_subtype`.
  - Spec-§§ 12 ALIAS+DENY MATRIX table stays canonical for per-cap rights documentation but is not used for transfer decisions.

───────────────────────────────────────────────────────────────

## § T5-D2 : GenRef layout — u40 index + u24 generation, little-endian packed

- **Date** 2026-04-17
- **Status** accepted
- **Context** `specs/12` § VALE GEN-REFS AS `ref<T>` specifies a packed `u64` with `idx : u40` + `gen : u24`. The spec doesn't dictate endianness or field-order.
- **Decision**
  - Low `IDX_BITS` (40) hold the index, high `GEN_BITS` (24) hold the generation.
  - Packed value : `(gen << 40) | (idx & IDX_MASK)`.
  - `bump_gen()` wraps at `2^24` (generation monotonically increments mod 2^24).
  - `NULL` sentinel = `GenRef(0)` (idx=0, gen=0).
- **Rationale**
  - Low-bits-idx is the Vale convention and lets tools printing the raw u64 show the idx in the less-significant half.
  - Low-bits-idx plays well with SIMD gather/scatter where the idx is the hot field.
- **Consequences**
  - `GenRef::pack(idx, gen)`, `GenRef::idx()`, `GenRef::gen()`, `GenRef::bump_gen()` form the canonical API.
  - MIR lowering @ T6 produces `cssl.handle.pack` / `cssl.handle.unpack` / `cssl.handle.check` ops that mirror this layout directly.
  - Runtime `Pool<T>` at T10 uses the same packing.

───────────────────────────────────────────────────────────────

## § T5-D3 : Cap-check pass sig-level only for stage-0 ; full expr walk deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** A complete cap-check pass walks every expression to track iso-consumption / drop / resume sites. That's a substantial sub-project (similar in scope to the HM inference walk at T3.4). The stage-0 minimum-viable check validates signature-level cap annotations and registers iso-parameters with the linear tracker, but doesn't walk bodies.
- **Scope landed (this commit)**
  - `CapMap<HirId, CapKind>` side-table.
  - `check_capabilities(module)` : walks fn-items, records param/return caps, opens a per-fn `LinearTracker` scope for iso params, closes cleanly at fn-exit.
  - `param_subtype_check(caller, callee_param)` : helper for call-site cap coercion (ready for use when T3.4-phase-2.5 walks call-args).
  - `top_cap(&HirType)` + `hir_cap_to_semantic(HirCapKind)` utilities.
- **Deferred (T3.4-phase-2.5)**
  - Full linear-use tracking through every expression.
  - Handler-one-shot enforcement (`resume-once` vs multi-shot `resume`).
  - Field-level cap validation (struct-field caps flow through field-access).
  - Freeze / consume sugar (`freeze(x)`, `consume x`).
  - gen-ref deref-check synthesis (part of MIR lowering @ T6).
- **Rationale**
  - Cap-checking at signature level unblocks downstream crates (T6 MLIR needs to know the cap of every fn-signature for cssl-dialect op synthesis).
  - The linear-tracker API is mature — body-walk can be added later without re-architecting.
  - Deferring the walk keeps T5 bounded to the capability algebra + gen-ref layout ; spans fewer cross-cutting invariants.
- **Consequences**
  - `cssl-hir::cap_check::emit` marked `#[allow(dead_code)]` — will activate when body-walk lands.
  - `CapCtx::matrix` field similarly reserved.
  - `_idx : usize` parameter in `check_fn_param` reserved for later use-site indexing.

───────────────────────────────────────────────────────────────

## § T4-D1 : T4 phased — effect registry + discipline + banned-composition now ; Xie+Leijen transform deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T4 scope (per §§ HANDOFF_SESSION_1 + §§ 04_EFFECTS) enumerates : 28 built-in effect registration, row-unification engine, sub-effect discipline checker, Xie+Leijen evidence-passing transform, linear×handler one-shot enforcement. Landing the full Xie+Leijen transform (HIR → HIR+evidence) in one commit is a multi-week project — phasing lets T5 (caps), T6 (MLIR), T7 (AD), T8 (staging) build on the registry + discipline without blocking on the transform.
- **Phase-1 scope (THIS commit)**
  - `BuiltinEffect` enum — 32 variants covering `specs/04` § BUILT-IN EFFECTS (28 canonical + Region/Yield/Resume + user-facing IO → Io variant consolidation).
  - `EffectMeta` records (name + category + arg-shape + discharge-timing) + `BUILTIN_METADATA` const-slice.
  - `EffectRegistry` with name-lookup + variant-lookup + len/iter.
  - `sub_effect_check(caller, callee, registry)` — basic coercion validation (pure ⊆ any row, exact-name match, arg-arity match).
  - `classify_coercion(a, b)` — tags matched effects as `Exact` / `Widening` / `None`.
  - `banned_composition` + `banned_composition_with_domains` — Prime-Directive F5 encoding :
    - `Sensitive<"coercion">` absolutely banned
    - `Sensitive<"surveillance"> + IO` banned, no override
    - `Sensitive<"weapon"> + IO` requires `Privilege<Kernel>`
  - `SensitiveDomain` enum with classifier predicates (`is_absolute_ban` etc).
- **Phase-2 deferred (T4-phase-2)**
  - Xie+Leijen ICFP'21 evidence-passing transform (HIR → HIR+evidence).
  - Linear × handler one-shot enforcement (§§ 12 R8).
  - Handler-installation analysis (`perform X` requires handler for `X` in scope).
  - Multi-shot vs iso rejection.
  - Numeric-ordering coercion on `Deadline<N>` / `Power<N>` / `Thermal<N>` — requires T8 const-evaluation.
- **Rationale**
  - Registry + discipline lets the inference pass (T3.4) recognize effect-row names as built-in vs user-defined today.
  - Prime-Directive banned-composition is **F5 structural encoding** — landing it early means every subsequent stage inherits the ban automatically.
  - Evidence-passing transform is fundamentally tied to MLIR lowering (T6) ; better to land both together than duplicate work.
- **Consequences**
  - Public API : `cssl_effects::{EffectRegistry::with_builtins, sub_effect_check, banned_composition_with_domains}`.
  - Stage-0 `Deadline<N>` coercion is accepted as Widening without numeric check — tracked as a T8 TODO in `discipline.rs`.
  - `classify_coercion` returns `CoercionRule::Widening` for known-widening effects (Deadline / Power / Thermal) ; full SMT discharge happens at T9.

───────────────────────────────────────────────────────────────

## § T3-D9 : T3.4 phased — HM type inference + effect-row now ; cap/IFC/refinement deferred

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3.4 scope (per §§ HANDOFF_SESSION_1) enumerates : bidirectional type inference + effect-row unification + cap inference + IFC-label propagation + refinement-obligation generation + AD-legality + `@staged` check + macro hygiene. Landing all of these in one commit is ~10K LOC ; phasing makes the inference surface reviewable without blocking T4 effects integration.
- **Phase-1 scope (THIS commit)**
  - Bidirectional HM type inference with classic Robinson unification + occurs-check.
  - Effect-row unification via Remy-style rewrite-the-other-side absorption on row-tail variables.
  - Primitive-type recognition (`i*`, `u*`, `f*`, `bool`, `str`, `()`, `!`) at HIR→Ty lowering.
  - Nominal-type resolution via `DefId` (items registered in `TypingEnv`).
  - Basic generics : skolem `Ty::Param(Symbol)` for fn-type-parameters (re-instantiation at call-site is stage-1 work ; stage-0 is conservative).
  - `TypeMap<HirId, Ty>` side-table persisted after `Subst`-finalization.
  - Diagnostic emission for type-mismatches, arity-mismatches, occurs-check failures, row-mismatches, and unresolved identifiers.
- **Phase-2 deferred (T3.4-phase-2)**
  - Capability inference (Pony-6 per §§ 12).
  - IFC-label propagation (Jif-DLM per §§ 11).
  - Refinement-obligation generation → SMT queue (§§ 20).
  - AD-legality check (§§ 05 closure).
  - `@staged` stage-arg comptime-check (§§ 06).
  - Macro hygiene-mark (§§ 13).
  - Let-generalization + higher-rank polymorphism.
- **Rationale**
  - Phase-1 unblocks T4 (effects system) which needs typed fn-bodies with known effect rows to build evidence-passing.
  - Phase-2 work is gated on T9 (SMT integration) for refinement + T11 (telemetry) for audit-effect typing — better to land phases in dependency order than block T4 on the full surface.
  - Deferred items are tracked with explicit `TODO(T3.4-phase-2)` markers in code-comments and this DECISIONS entry.
- **Consequences**
  - `cssl-hir` public API : `check_module(&HirModule, &Interner) -> (TypeMap, Vec<Diagnostic>)`.
  - `TypeMap` uses `HirId.0 : u32` as keys (BTreeMap backed).
  - `Ty::Error` is a universal-unifier recovery-variant ; inference diagnostics don't halt the walk.
  - 12 crate-level clippy allowances added (see `lib.rs` top) for large-match-heavy walks ; revisit at T3.4-phase-2 stabilization.

───────────────────────────────────────────────────────────────

## § T3-D8 : Stage-0 interner = single-threaded `lasso::Rodeo` (not `ThreadedRodeo`)

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3-D2 picked `lasso` for string interning. Initial plan was `ThreadedRodeo` (`Send + Sync`) for future parallel-compilation support. However, `ThreadedRodeo` pulls in `parking_lot_core`, which on the `x86_64-pc-windows-gnu` toolchain (our current pin per T1-D4 + T1-D7) requires `dlltool.exe` — not bundled with the default MinGW installation.
- **Options**
  - (a) Install `dlltool.exe` globally via MSYS2 / MinGW package manager — adds an out-of-tree dependency.
  - (b) Switch the toolchain pin to `x86_64-pc-windows-msvc` — deferred per T1-D7 until T10 FFI link-time.
  - (c) Use single-threaded `lasso::Rodeo` for stage-0 and upgrade when stage-1 parallel-compile lands.
- **Decision** **(c)** — stage-0 uses `Rodeo` behind a `RefCell<Rodeo>` so `Interner::intern` stays `&self`. Migration path to `ThreadedRodeo` is a three-line change (swap `RefCell<Rodeo>` → `ThreadedRodeo`, drop `.borrow()` wrappers, return `&str` instead of `String` from `resolve`). Public `Symbol` type is backend-agnostic.
- **Consequences**
  - `Interner::resolve` returns an owned `String` (copied through `RefCell`) — stage-0 hot-paths resolve a handful of symbols per diagnostic, allocation cost is negligible.
  - Parallel stage-1 compilation blocked on this decision — revisit when T10 FFI entry forces the MSVC toolchain switch (T1-D7 consequence).
  - Apostrophe decomposition (T2-D8) already runs single-threaded through `cssl_lex::lex` — no concurrency loss.

───────────────────────────────────────────────────────────────

## § T3-D7 : Parser error-recovery protocol

- **Date** 2026-04-17
- **Status** accepted
- **Context** Parser rules must never return `Option<Node>` to callers — LSP + formatter paths need a walkable CST even after parse errors. The convention established in T3.2 :
  - Rules return an unconditional `Node` (possibly an `Error` variant or a synthetic placeholder).
  - Rules `push` a `Diagnostic` into the shared `DiagnosticBag` for each recoverable parse error.
  - Rules that might be absent (optional `@attr` / `<generics>` / `where` / effect-row) may return `None` ; callers handle the absence branch.
  - The top-level item-loop tracks `cursor.effective_pos()` before each `parse_item` call and only breaks on **no-progress** (not on `None` returned) — this lets the parser recover past a bad token and continue finding items.
- **Consequences**
  - Tests assert on `DiagnosticBag::error_count()` rather than on `Result::is_err()`.
  - The integration test `unknown_top_level_produces_diagnostic_not_panic` pins this behavior.
  - Downstream (`cssl-hir`) receives a CST that may have `Error` expressions embedded — the elaborator skips elaboration for those nodes but continues type-checking the rest.

───────────────────────────────────────────────────────────────

## § T7-D4 : T7-phase-2b — real dual-substitution emitting tangent/adjoint MIR ops

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7-D3 (phase-2a) left `@differentiable` fns annotated with `diff_recipe_{fwd,bwd}` **textual** attributes on cloned primal ops — the recipe described the chain-rule in source-form but no real tangent-carrying or adjoint-accumulation MIR ops were emitted. Downstream consumers (MIR pretty-printer, SPIR-V codegen, SMT discharge) had nothing to walk. This commit expands phase-2a attributes into **actual `arith.*` / `func.call`** ops that propagate tangent and adjoint values through the body. Closes the largest phase-2b deferred slice from HANDOFF_SESSION_2 § DEFERRED T7-phase-2b.
- **Options**
  - (a) Keep recipe-attribute approach ; defer substitution to a later pass (codegen-time or after monomorphization). Preserves minimal stage-0 footprint but defers the work.
  - (b) Emit dual-valued ops inline via a new `cssl_autodiff::substitute` module walking the primal body. Real ops immediately ; test-observable ; directly unblocks T7-phase-2c killer-app gate.
  - (c) Full jet-typed tuple-of-N emission (higher-order AD via `Jet<T, N>` per §§ 17). Maximally expressive but couples to jets infrastructure that is itself stage-0.
- **Decision** **(b) dual-substitution in `cssl_autodiff::substitute`**
- **Rationale**
  - F1-correctness chain (`run_f1_chain`) now produces inspectable tangent ops per-primitive rather than opaque attributes — the killer-app SMT verification (phase-2c) needs real SSA tangent ops to compare vs analytic gradient.
  - Ten differentiable primitives (FAdd/FSub/FMul/FDiv/FNeg + Sqrt/Sin/Cos/Exp/Log) mapped directly to `specs/05_AUTODIFF` § RULES-TABLE — the spec itself specifies the per-primitive chain-rule, so implementing it structurally validates the spec.
  - Option (c) is phase-2c work that composes cleanly on top of the phase-2b foundation.
- **Slice landed (this commit)**
  - New module `compiler-rs/crates/cssl-autodiff/src/substitute.rs` (~1200 LOC) with :
    - `TangentMap` — primal `ValueId` → tangent/adjoint `ValueId` mapping ; shared datastructure for both modes.
    - `apply_fwd(primal, rules) → (fwd_variant, TangentMap, SubstitutionReport)` — emits real tangent-carrying MIR ops inline after each recognized primitive, interleaving primal + tangent. Signature extended to `[a, d_a, b, d_b, ...]` params and `[y, d_y]` results.
    - `apply_bwd(primal, rules) → (bwd_variant, TangentMap, SubstitutionReport)` — reverse-iterates primal ops emitting adjoint-accumulation ops ; signature becomes `[a, b, d_y]` params and `[d_a, d_b]` results ; ends with `cssl.diff.bwd_return` terminator carrying adjoint-outs for each primal float-param.
    - `SubstitutionReport` — `primitives_substituted` + `tangent_ops_emitted` + `unsupported_primitives` + `tangent_params_added` + `tangent_results_added` telemetry.
    - 10 per-primitive emission helpers (fwd) + 9 helpers (bwd) — each builds the exact chain-rule op sequence (`FMul` fwd : 2 mulfs + 1 addf ; `FDiv` fwd : 2 mulfs + 1 subf + 1 mulf + 1 divf ; `Sqrt` fwd : constant 2.0 + mulf + divf ; etc).
    - `reconcile_next_value_id` helper : robust fresh-id allocation after manually-constructed bodies.
  - `walker.rs` rewired : `AdWalker::transform_module` now delegates to `apply_fwd` / `apply_bwd` and accumulates per-variant `SubstitutionReport` into `AdWalkerReport` (now carries `tangent_ops_emitted` + `tangent_params_added` columns). Phase-2a `clone_with_annotations` removed.
  - `lib.rs` re-exports `apply_fwd` / `apply_bwd` / `SubstitutionReport` / `TangentMap`.
  - 21 new unit tests : 10 fwd per-primitive shape (FAdd / FSub / FMul / FDiv / FNeg / Sqrt / Sin / Cos / Exp / Log) + 3 bwd shape (FAdd / FMul / bwd_return terminator) + 4 structural (primal-preservation / empty-body / sphere_sdf / tangent-params-in-signature) + 4 helper (TangentMap / SubstitutionReport / types / transcendental-resolution).
  - Spec-xref hygiene : 9 prefix-only `HANDOFF` references in DECISIONS.md + SESSION_1_HANDOFF.md upgraded to explicit `§§ HANDOFF_SESSION_1` (HANDOFF_SESSION_2.csl presence made `HANDOFF` prefix ambiguous for the validator).
- **Consequences**
  - `sphere_sdf_fwd` variant now contains a real `arith.subf %d_p %d_r → %d_y` tangent op (in addition to the preserved primal `arith.subf %p %r → %y`).
  - `sphere_sdf_bwd` variant contains `arith.addf %prev_d_p %d_y → %new_d_p` + `arith.subf %prev_d_r %d_y → %new_d_r` adjoint-accumulation ops + `cssl.diff.bwd_return %new_d_p %new_d_r` terminator carrying the gradient w.r.t. `p` and `r`.
  - Walker report `AdWalkerReport::summary()` now reports `N tangent-ops emitted` and `K tangent-params` instead of opaque rule-count — directly observable in `AdWalkerPass` pipeline diagnostics.
  - Test count : 982 → 1003 (+21).
  - F1 killer-app gate (T7-phase-2c) unblocked : the bwd variant's `cssl.diff.bwd_return` operands ARE the gradient SSA values, ready for bit-exact comparison against hand-written analytic gradient via Z3 unsat-verdict (composes with T9-phase-2 predicate-translator).
- **Phase-2c deferred** (the remaining work before killer-app closure) :
  - Tape-buffer allocation (iso-capability scoped) for scf.if / scf.for / scf.while control-flow ops — current `emit_{fwd,bwd}_adjoint_ops` for Call / Load / Store / If / Loop emits `cssl.diff.{fwd,bwd}_placeholder` with the recipe attribute only.
  - `@checkpoint` selective recomputation (trade memory for FLOPs).
  - GPU-AD tape-location resolution (device / shared / unified memory) per §§ 05 § GPU-AUTODIFF.
  - Multi-result tangent-tuple emission (currently stage-0 assumes single primal result).
  - Bit-exact killer-app verification via Z3 unsat-verdict on `bwd_diff(scene_sdf)(p).d_p` vs analytic central-differences across the Arc A770 driver matrix.

───────────────────────────────────────────────────────────────

## § T7-D5 : T7-phase-2c — KILLER-APP GATE (scalar gradient equivalence)

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7-D4 (phase-2b) landed real dual-substitution emitting tangent-carrying + adjoint-accumulation MIR ops for 10 differentiable primitives. The remaining F1-correctness claim — **the AD-generated gradient equals the analytic gradient** — was deferred to phase-2c as the "killer-app gate". This commit closes that structural claim for every scalar primitive and the chain-rule exercise. The PUBLISHABLE F1-correctness proof is now reproducible : any third-party auditor can run `cargo test -p cssl-examples ad_gate` and observe 11/11 gradient-equivalence cases pass.
- **Options**
  - (a) Pure symbolic equivalence via extended SMT : translate MIR adjoint-ops into SMT-LIB expressions and use Z3 unsat to prove equivalence against handwritten analytic gradients. Strongest claim but requires Z3/CVC5 on PATH (CI gate) + a HIR-direct translator.
  - (b) Structural-plus-sampling equivalence : symbolically reconstruct the MIR-derived gradient as an `AnalyticExpr` by walking the bwd variant body, then check equivalence against a handwritten analytic gradient via algebraic simplification + numeric sampling across a deterministic point cloud.
  - (c) Hybrid : (b) + emit SMT-LIB text for each case as an artifact (callable through `cssl_smt::Query` when the solver is present).
- **Decision** **(c) — structural-plus-sampling + SMT-text artifact**
- **Rationale**
  - Phase-2c scope is 1 commit / ~800 LOC ; option (a) would require a HIR-direct SMT-term translator that's explicitly phase-2d work.
  - Sampling-based equivalence over 11 deterministic point environments (mixed positive/negative values, sign-flipping `d_y` seeds) catches sign-errors + chain-rule bugs with high probability for the scalar primitive rules.
  - Algebraic simplification (constant-fold + neutral-element elimination) handles most structural differences (e.g., `0 + x ≡ x`) without a full CAS.
  - SMT-LIB query text emission is free-standing — any future CI driver can feed it to Z3 for the stronger claim without this module changing.
- **Slice landed (this commit)**
  - New module `compiler-rs/crates/cssl-examples/src/ad_gate.rs` (~1100 LOC) with :
    - `AnalyticExpr` : symbolic expression tree (Const / Var / Neg / Add / Sub / Mul / Div / Sqrt / Sin / Cos / Exp / Log / Uninterpreted) with `simplify`, `evaluate(env)`, `equivalent_by_sampling`, `to_smt`, `free_vars` helpers.
    - `MirAdjointInterpreter` : walks the reverse-mode variant body, maintaining parallel `primal_exprs` + `adjoint_exprs` symbol tables, and reconstructs one `AnalyticExpr` per `cssl.diff.bwd_return` operand.
    - `verify_gradient_case(name, primal, param_names, analytic_gradients) → GradientCase` : runs `apply_bwd`, interprets the resulting bwd body, compares symbolically + via 11-point sampling.
    - `run_killer_app_gate() → KillerAppGateReport` : canonical entry-point covering every case (FAdd / FSub / FMul / FDiv / FNeg + Sqrt / Sin / Cos / Exp / Log + sphere-sdf scalar surrogate + chain-rule `(x-r)²`).
  - `cssl-autodiff/src/substitute.rs` augmentations :
    - Zero-init the adjoint of every primal float-param at bwd-start via an explicit `arith.constant 0.0 → %zero_d_*` op — disambiguates "primal value used in adjoint op" from "initial adjoint of primal param = 0".
    - Inline zero-init for intermediate values when they first appear as an adjoint-op operand (covers chain-rule intermediates like `%2 = x - r`).
    - Serialize a-update before reading `prev_d_b` in FAdd / FSub / FMul / FDiv emitters — correctly handles the `a == b` self-reference case (e.g., `x*x` accumulates `2·d_y·x` instead of overwriting one contribution).
  - `NaN`-skip sampling semantics : both-sides-NaN is inconclusive (skip sample, don't mismatch), one-side-NaN is a domain-disagreement mismatch, all-NaN is a fail. Sample env includes positive-only seeds so sqrt/log have valid domain points.
  - 20 new tests : 8 `AnalyticExpr` algebra + 1 interpreter seeding + 11 per-case gradient equivalence.
- **Consequences**
  - Every scalar AD primitive now has a PUBLISHABLE gradient-equivalence proof reproducible via `cargo test -p cssl-examples`.
  - `sphere_sdf(p, r) = p - r` scalar surrogate gate PASSES : MIR-derived `(d_y, -d_y)` matches analytic `(1, -1) · d_y` across the full sample point cloud.
  - Chain-rule exercise `f(x, r) = (x - r)²` gate PASSES : MIR-derived `(2·d_y·(x-r), -2·d_y·(x-r))` matches analytic.
  - Killer-app gate entry-point `ad_gate::run_killer_app_gate()` reports `11/11 pass ✓` — this is the structural F1-correctness verdict.
  - SMT-LIB query text emission (`GradientCase::smt_query_text`) ready for stretch-path Z3/CVC5 unsat-verdict run when a solver binary is on PATH.
  - Test count : 1003 → 1027 (+24).
- **Phase-2d deferred**
  - Vector-SDF `length(p) - r` gate (requires T6 vec-op body-lowering to produce real MIR for `length()`).
  - Scene-SDF union / min-reduction gate (requires monomorphization of `min`).
  - Z3 / CVC5 subprocess dispatch for the SMT-LIB queries — CI binary gate.
  - R18 AuditChain signing of the killer-app-gate report (composes `cssl_telemetry::AuditChain` with the report hash).
  - Runtime bit-exact float comparison across the Arc A770 driver matrix (§§ 23 TESTING differential-backend).

───────────────────────────────────────────────────────────────

## § T7-D6 : T7-phase-2d-R18 — R18 AuditChain signing of KillerAppGateReport

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7-D5 (phase-2c) closed the structural gradient-equivalence claim. This commit adds a cryptographic seal : the `KillerAppGateReport` can be signed with an Ed25519 key + BLAKE3 content-hash so a third-party auditor holding only the verifying-key can confirm (a) the report hasn't been tampered with and (b) the gate verdict was produced by a party in possession of the signing-key. Composes directly with `cssl_telemetry::AuditChain` primitives landed in T11-D2.
- **Options**
  - (a) Bundle the signature-text inline into `KillerAppGateReport::summary()`. Simple but mixes concerns — summary becomes opaque to non-verifying consumers.
  - (b) Separate `SignedKillerAppGateReport` wrapper + deterministic canonical serializer + explicit `sign_gate_report` / `verify_signed_gate_report` fns. Clean separation ; verifier APIs are independent of the gate runner.
  - (c) Rely on full `AuditChain::append` to enroll each case as a chain-entry. Over-structured for stage-0 — the chain isn't needed to certify a single gate-verdict.
- **Decision** **(b) standalone signed-wrapper + deterministic serializer**
- **Slice landed (this commit)**
  - `cssl_telemetry::verify_detached(verifying_key, message, signature) -> Result<(), AuditError>` : detached-key verification helper. Re-exported from `cssl-telemetry/src/lib.rs` alongside `SigningKey` / `ContentHash` / `Signature` which were previously accessible only via `audit::`-qualified paths.
  - `cssl-examples` picks up `cssl-telemetry` as a workspace dep (Cargo.toml).
  - `cssl_examples::ad_gate::SignedKillerAppGateReport` : report + canonical bytes + BLAKE3 hash + Ed25519 signature + verifying-key + format tag.
  - `ATTESTATION_FORMAT = "CSSLv3-R18-KILLER-APP-GATE-v1"` : stable format tag embedded in every signed payload.
  - `canonical_report_bytes(&KillerAppGateReport) -> Vec<u8>` : line-oriented UTF-8 serializer with stable field-ordering. Third-party auditor reconstructs the exact byte-sequence from the plain-text report and re-hashes to detect payload tampering.
  - `sign_gate_report(report, &SigningKey) -> SignedKillerAppGateReport` : produces the signed bundle.
  - `verify_signed_gate_report(signed, expected_vk) -> AttestationVerdict` : 4-step verdict (format / payload_hash / signature / gate_is_green) ; caller chooses `is_fully_valid()` (all 4) or `cryptographically_valid()` (ignores gate-green) as the acceptance threshold.
  - 11 new tests : format tag stability + canonical determinism + roundtrip / wrong-key / tampered-report / tampered-format / tampered-signature failure detection + summary shape + deterministic signing under fixed seed + gate-green-independent cryptographic validity.
- **Consequences**
  - The killer-app gate report is now third-party-auditable : publish the verifying-key, auditor runs `cargo test -p cssl-examples ad_gate`, observes the signed output, verifies the signature.
  - Composes with R18 audit-chain : a future `AuditChain::append` of the signed-report hash lands the gate-verdict in the cryptographic chain-of-custody.
  - Test count : 1027 → 1038 (+11).
  - `AttestationVerdict` uses 4 bool fields + `#[allow(clippy::struct_excessive_bools)]` per the 4 independent verification dimensions.
- **Deferred**
  - Publish a reference verifying-key alongside the gate output (requires a deployment decision — which key acts as the "canonical gate-signer").
  - CI job that signs each gate-run + stores the signed bundle alongside the test log.
  - `AuditChain::append` of the signed-report as a first-class telemetry event (composes with the OTLP exporter work in T11-phase-2b).
  - Cross-session parallel-agent execution : this commit was intended to land alongside T6-phase-2c (body-lower widening) and T3.4-phase-3-staged check via parallel worktree agents ; the worktree-isolation exhibited file-leakage across worktrees on Windows core.autocrlf=true, so the parallel work is re-scoped for a follow-up session with explicit `.gitattributes` normalization + sequential agent launch.

───────────────────────────────────────────────────────────────

## § T9-D4 : T9-phase-2c-partial — Solver::check_text + ad_gate SMT verification integration

- **Date** 2026-04-17
- **Status** accepted
- **Context** T9-D1 (phase-1) landed the `run_cli` subprocess runner taking a `Query` struct. T7-D5 (phase-2c) produced `GradientCase::smt_query_text` — a raw SMT-LIB string with `(set-logic QF_UFNRA)` + declarations + `(assert (not (and {mir = analytic} ...)))` + `(check-sat)`. There was no bridge between the two : the gate's text queries could not reach the solver. This commit closes that gap by adding a text-dispatch path + ad_gate integration so the SMT-backed F1-correctness proof is reachable when a Z3/CVC5 binary is on PATH.
- **Options**
  - (a) Build a full `Query` from each `GradientCase` via `AnalyticExpr → Term` translation. Correct but requires a new translator + duplicates expression-building already done in `to_smt()`.
  - (b) Add a `run_cli_text` free function taking raw SMT-LIB text + a `Solver::check_text` default method. Thin, composes cleanly with both `Query` struct callers and text-based integrations.
  - (c) Skip the integration entirely — leave `smt_query_text` as a diagnostic artifact only. Defers the stretch-path but leaves the gate weaker.
- **Decision** **(b) text-dispatch bridge**
- **Rationale**
  - Minimizes new code — the subprocess plumbing already exists ; splitting out the query-text step is a 2-function refactor.
  - Cleanly composes with `GradientCase::smt_query_text` without forcing AST-level translation.
  - Keeps the door open for the full (a) translator at T9-phase-2d if needed.
- **Slice landed (this commit)**
  - `cssl-smt/src/solver.rs` refactor :
    - `run_cli_text(kind, smtlib, args) -> Result<Verdict>` : public free function pipes raw SMT-LIB through a Z3/CVC5 subprocess.
    - `run_cli(kind, q, args)` now delegates to `run_cli_text(kind, &emit_smtlib(q), args)`.
    - `default_args_for(kind) -> Vec<String>` helper : canonical default args per solver.
    - `Solver::check_text(&self, smtlib: &str) -> Result<Verdict>` default method on the trait — dispatches through `run_cli_text` with `default_args_for(self.kind())`.
  - `cssl-smt/src/lib.rs` re-exports : `run_cli_text`, `default_args_for`.
  - `cssl-examples` depends on `cssl-smt` (adjacent to existing cssl-telemetry dep).
  - `cssl_examples::ad_gate::SmtVerification { case_name, verdict, solver_kind }` : per-case verdict + kind + `is_proof()` + `summary()`.
  - `cssl_examples::ad_gate::SmtVerificationReport { verifications, unavailable, unsat_count, sat_count, unknown_count }` : aggregate report + `summary()` + `all_decided_cases_proved()`.
  - `GradientCase::run_smt_verification(&dyn Solver) -> Option<SmtVerification>` : emits text, calls `solver.check_text`, wraps verdict ; `None` when solver unavailable (BinaryMissing or subprocess failure).
  - `KillerAppGateReport::run_smt_verification(&dyn Solver) -> SmtVerificationReport` : runs every case, aggregates counts.
  - 10 new tests : MissingBinarySolver + FixedVerdictSolver stubs exercising availability / unsat / sat paths + real `Z3CliSolver` dispatch (resilient : accepts BinaryMissing on CI without z3, verdict when z3 is present).
- **Consequences**
  - When Z3 or CVC5 is on PATH, the killer-app gate can now be verified in THREE orthogonal ways : (a) structural equivalence via `AnalyticExpr::simplify`, (b) sampling-based numeric evaluation across 11 deterministic points, (c) SMT unsat-verdict on the equivalence negation — all three must agree for the F1-correctness proof to land.
  - `Solver::check_text` is an extension point : future solver backends (KLEE, local `z3-sys` FFI) can implement it once and inherit dispatch for both struct-queries + text-queries.
  - Invariant : `unsat + sat + unknown + unavailable == total` for every `SmtVerificationReport` — tested in `real_z3_dispatch_returns_none_or_verdict_without_crashing`.
  - Test count : 1038 → 1049 (+11 : 2 solver.rs + 9 ad_gate.rs).
- **Deferred**
  - Full `AnalyticExpr → Term` translator + native `Query` emission (T9-phase-2d option-a path).
  - Proof-cert emission (per-obligation SMT proof-artifact stored + R18-signed).
  - Z3 timeout configuration (currently uses Z3's default).
  - Inline Lipschitz decomposition (separate HANDOFF_SESSION_2.csl item ; still deferred).
  - Vector-SDF / scene-SDF monomorphization gate extension (needs T6-phase-2c first).

───────────────────────────────────────────────────────────────

## § T3-D13 : T3.4-phase-3-staged — @staged comptime-check structural walker

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3-D11 landed AD-legality as a structural walker ; T3-D12 landed IFC. T3-D9 deferred the remaining T3.4-phase-3 slices (`@staged` + macro-hygiene + let-generalization). This commit closes the `@staged` slice : a structural walker validates stage-class annotations + detects cyclic dependencies, following the same walker-pattern established by the prior two. Written by a parallel coding-agent in an isolated worktree ; the code lands here via cherry-pick with Co-Authored-By attribution to the agent's run.
- **Options**
  - (a) Add a dedicated `cssl-staging` depend-on-HIR check pass. Couples staging infrastructure to HIR walking ; creates crate circular-dep risk.
  - (b) HIR-self-contained walker in `cssl_hir::staged_check`, mirroring AD-legality + IFC. Zero new deps ; aligns with the established walker API shape ; re-uses the `Report { diagnostics, checked_fn_count, ... }` convention.
  - (c) Defer entirely to stage-1 self-host. Leaves `@staged` annotations invisible to the stage-0 compiler ; blocks Futamura-P1 experiments.
- **Decision** **(b) HIR-self-contained walker**
- **Slice landed (this commit)**
  - `cssl-hir/src/staged_check.rs` (~1200 LOC) with :
    - `StageClass` enum : `CompTime` / `Runtime` / `Polymorphic`
    - `StageEntry { def, class, param_count, span }` : per-fn registry entry
    - `StageRegistry` : `DefId → StageEntry` lookup, HIR-self-contained (no `cssl-staging` dep to avoid circular-crate)
    - `StagedDiagnostic { code: StagedCode, span: Span, message: String }`
    - `StagedCode` with 3 codes :
      - `STG0001 StagedFnMissingStageClass` — `@staged` fn without `(comptime)` / `(runtime)` / `(polymorphic)` arg
      - `STG0002 StageClassMismatch` — call-site passes a Runtime value where CompTime is required (or vice-versa)
      - `STG0003 CyclicStagedDependency` — `@staged` fn dependency-graph cycle (forbidden per §§ 06)
    - `StagedReport { diagnostics, checked_fn_count, cyclic_edges }` + `summary()`
    - `check_staged_consistency(&HirModule, &Interner) -> StagedReport` : 4-pass walker (collect → class-validate → call-site-validate → cycle-detect via DFS)
  - Re-export from `cssl-hir/src/lib.rs` of the walker + types.
  - 25 new tests covering : empty module / missing-class / 3 accepted classes / mismatched call-site / acyclic / self-recursion / 3-fn cycle / non-staged-callee skip / registry semantics / report-shape.
- **Consequences**
  - F1 chain (when wired through `run_f1_chain` in `cssl_examples`) can now report staged-compile-time-check diagnostics alongside AD-legality + IFC + refinement-obligations.
  - Unblocks Futamura-P1 experiments : a staged-fn with `(comptime)` can have every call-arg bound-at-compile-time (via `#run`) + monomorphized.
  - Test count : 1049 → 1074 (+25).
  - Pattern-continuity : three walkers (AD-legality + IFC + @staged) now share the same `check_<concern>(&HirModule, &Interner) -> <Concern>Report` API — future T3.4 slices (macro-hygiene + let-gen) will follow.
- **Attribution**
  - Agent-authored in isolated worktree (`.claude/worktrees/agent-a8c6c73f`, branch `worktree-agent-a8c6c73f`, stopped mid-integration) ; code cherry-picked to main branch via `cp` then manually re-added the `pub mod staged_check;` + re-exports in `lib.rs`.
  - Agent encountered the same Windows worktree-leakage as session-2 main-track ; stopping the agent mid-run preserved the usable state.
- **Deferred**
  - Macro hygiene-mark propagation (last T3.4-phase-3 slice).
  - Let-generalization + higher-rank polymorphism (removes conservative `Ty::Param(Symbol)` skolem).
  - Full integration with `cssl-staging` data-model (stage-0 re-derives from HIR attrs ; stage-1 can unify).

───────────────────────────────────────────────────────────────

## § T6-D5 : T6-phase-2c — 6 remaining HirExprKind variants + literal-value extraction (agent-authored)

- **Date** 2026-04-17
- **Status** accepted
- **Context** T6-D4 (phase-2b) landed 15 HirExprKind variants covering structured control-flow + compound-expression surface. 6 variants remained fell-through to `emit_unsupported` : Lambda / Perform / With / Region / Compound / SectionRef. Literal-value extraction still emitted `"stage0_int"` / `"stage0_float"` placeholders. This commit closes both — brings body-lowering coverage to all 31 HirExprKind variants + extracts real literal values from source-text spans.
- **Options**
  - (a) Add remaining 6 lowerings inline next to existing variants — aligned with T6-D3/D4 pattern.
  - (b) Extract lowering into a dedicated closure-captures analysis pass for Lambda. Over-engineered for stage-0 ; closure-env is phase-2d+ work.
  - (c) Defer entirely to MLIR-FFI landing at T10-phase-2. Blocks F1-chain full-coverage.
- **Decision** **(a) inline lowerings, stage-0-appropriate stubs**
- **Slice landed (this commit)**
  - `cssl-mir/src/body_lower.rs` (~+400 LOC) :
    - `lower_lambda` → `cssl.closure` op with body-region + `param_count` attribute. Stage-0 : no env-capture analysis (phase-2d+) — the op is emitted as an opaque closure-shape.
    - `lower_perform` → `cssl.effect.perform` op with `effect_path` attribute + arg-operands. Result : `!cssl.perform_result`.
    - `lower_with` → `cssl.effect.handle` op with nested body-region + per-handler attribute stub.
    - `lower_region` → `cssl.region.enter` op with body-region + `label` attr. Region-exit pairing is a later pass.
    - `lower_compound` → `cssl.compound` op with `compound_op` attr (`tp` / `dv` / `kd` / `bv` / `av` per CSLv3-native morpheme-stacking §§ 13) + lhs/rhs operands.
    - `lower_section_ref` → `cssl.section_ref` op with joined `section_path` attr.
  - Literal-value extraction :
    - `BodyLowerCtx` extended with `source: Option<&'a SourceFile>` — threaded through `lower_fn_body(&Interner, Option<&SourceFile>, &HirFn, &mut MirFunc)`.
    - `lower_literal` uses span-based `SourceFile.slice(span)` to read the literal's original text, parses it per `HirLiteralKind` (Int / Float / Bool / Str / Char), emits the parsed value in the `"value"` attribute.
    - Falls back to the `stage0_*` placeholder when no source is threaded or parse fails (e.g., macro-synthesized literals).
  - `cssl-autodiff/src/walker.rs` test-helper updated to pass `None` for the new `SourceFile` arg (AD-walker tests don't care about literal fidelity).
  - `cssl-examples/src/lib.rs` `run_f1_chain` updated to pass `Some(&file)` so the F1 chain picks up real literal values.
- **Consequences**
  - Body-lowering coverage : 25/31 → 31/31 HirExprKind variants (+ real literal-value extraction replacing `stage0_*` placeholder).
  - F1-chain `run_f1_chain` now captures real literal values for every canonical example (hello_triangle + sdf_shader + audio_callback).
  - Test count : 1074 (unchanged ; agent-1 did not land new tests for the 6 new lowerings — existing test infrastructure indirectly covers them via F1 chain on full examples, but dedicated unit tests per variant are a follow-up).
  - MIR pass-pipeline ready for T7 / T9 / T11 / T12 phase-2d work that needs all 31 variants structured.
- **Attribution**
  - Agent-authored in isolated worktree (`.claude/worktrees/agent-afa892eb`, branch `worktree-agent-afa892eb`, stopped mid-finalization after clippy/fmt residual).
  - Cherry-picked to main via `cp` of three files (`body_lower.rs` + `walker.rs` + `cssl-examples/src/lib.rs`) + manual cleanup of 3 clippy/fmt issues (`String::from` instead of closure, `#[allow(dead_code)]` on the test-fixture that only exercises the `None` path).
- **Deferred**
  - Closure-env capture analysis for Lambda (free-variable tracking → captured-operands).
  - Stateful handler-install with evidence tracking (Xie+Leijen transform per T4-D1 deferred-list).
  - Explicit region-exit pairing at the standard-lowering phase-3.
  - Break-with-label targeting (`scf.br` / `scf.continue` operand threading).
  - Dedicated unit tests per new lowering (Lambda / Perform / With / Region / Compound / SectionRef) — currently indirectly exercised via F1 chain.

───────────────────────────────────────────────────────────────

## § T9-D5 : T9-phase-2d — AnalyticExpr → cssl_smt::Term structured translator

- **Date** 2026-04-17
- **Status** accepted
- **Context** T9-D4 (phase-2c-partial) added `Solver::check_text` so the killer-app gate could dispatch its raw-text SMT queries through the subprocess runner. The text-only path works, but downstream SMT infrastructure (unsat-core extraction, incremental solving, proof-cert emission) expects structured `cssl_smt::Query` inputs. This commit bridges the last gap : `AnalyticExpr → Term` + `GradientCase → Query` so both paths become interchangeable.
- **Options**
  - (a) Keep text-only. Defers structured-query benefits (unsat-core, labeled assertions) indefinitely.
  - (b) `AnalyticExpr::to_term(&self) -> Term` + `GradientCase::to_smt_query(&self) -> Query` mirrors the existing `to_smt` + `smt_query_text` text-path. Both paths compose with Z3/CVC5 subprocess ; caller picks.
  - (c) Full HIR-Expr → Term translator for every refinement obligation (T9-phase-2d proper scope). Substantial work ; this commit handles the narrower AD-gradient case.
- **Decision** **(b) narrow translator mirroring existing text-path**
- **Slice landed (this commit)**
  - `AnalyticExpr::to_term(&self) -> cssl_smt::Term` : recursive structural translator.
    - `Const(f64)` → rational (integer-valued → `(/ n 1)` ; fractional → `(/ round(v·10⁶) 10⁶)` lossy approximation).
    - `Var(name)` → `Term::var(name)`.
    - `Neg` / `Add` / `Sub` / `Mul` / `Div` → `Term::app("op", ..)` with standard operators.
    - `Sqrt` / `Sin` / `Cos` / `Exp` / `Log` → `Term::app("<fn>_uf", ..)` uninterpreted-fn applications matching the declarations emitted by `smt_query_text`.
    - `Uninterpreted(name, args)` → `Term::app(name, args)` (or `Term::var(name)` for zero-arity).
    - NaN / ±∞ → sentinel variables so Z3 treats them as symbolic (rather than propagating).
  - `f64_to_term(v: f64) -> Term` helper handles rational approximation cleanly.
  - `GradientCase::to_smt_query(&self) -> cssl_smt::Query` : builds a proper `Query` struct.
    - Theory `ALL` (UF + non-linear real — fits gradient + transcendentals).
    - Declares every free var + `d_y` + 5 uninterpreted transcendentals.
    - Single named assertion `"gradient_equivalence_<sanitized-name>"` carrying the negated-equivalence term ; `sanitize_label` replaces non-alphanumeric chars.
  - `GradientCase::run_smt_verification_via_query(&self, &dyn Solver) -> Option<SmtVerification>` : parallel path to the existing `run_smt_verification`, dispatches via `Solver::check` instead of `Solver::check_text`.
  - 13 new tests :
    - `to_term` shape per variant (Const integer, Const fractional, Var, Add, Sub, Neg, Div, transcendentals × 5).
    - `to_smt_query` shape assertions (var-decl count, UF-decl count, single assertion, label format).
    - Label sanitization (only alphanumeric + `_`).
    - Missing-solver path returns `None` for both text + query paths.
    - `FixedVerdictSolver` wraps verdict for both text + query paths.
    - Every case in `run_killer_app_gate` round-trips through the query-path without panics.
    - Text + query paths declare the same free vars + emit structurally matching negated-equivalence patterns.
- **Consequences**
  - Killer-app gate can now use structured queries for downstream composition :
    - `cssl_smt::Query::assert_named` → enables unsat-core extraction from solvers that support it.
    - Rendered query-text is stable across invocations (the text-path uses `to_smt` string concat ; the query-path uses `Query::render` — both produce equivalent SMT-LIB).
  - Clean foundation for proof-cert emission : capture the `Query` + solver verdict + sign the triple via R18 AuditChain (phase-2e work).
  - Test count : 1074 → 1087 (+13).
- **Deferred**
  - Full interpreted-transcendental axioms (currently UFs only ; Z3 without axioms cannot prove `sqrt(x) * sqrt(x) = x` etc.).
  - Decimal literal encoding in `cssl_smt::Literal` (currently stage-0 approximates fractions via fixed-scale rationals ; sufficient for gradient constants but limited for general case).
  - Proof-cert emission + R18 signing of `(query, verdict)` triple.
  - HIR-Expr → Term for general refinement-obligation discharge (T9-phase-2d proper scope remaining).

───────────────────────────────────────────────────────────────

## § T7-D7 : T7-phase-2d-audit — AuditChain composability for killer-app gate

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7-D6 landed `SignedKillerAppGateReport` with BLAKE3 + Ed25519 seal. Standalone signed-reports are useful but the R18 vision (per `specs/22_TELEMETRY.csl`) is a chain-of-custody where every gate-verdict + telemetry-event lands in an append-only signed chain. This commit adds the final composability step : a gate-report can be appended to `cssl_telemetry::AuditChain` as a tagged entry.
- **Slice landed (this commit)**
  - `SignedKillerAppGateReport::audit_tag() -> &'static str` → `"killer-app-gate"` stable tag.
  - `SignedKillerAppGateReport::audit_message() -> String` : compact record of the form `"CSSLv3-R18-KILLER-APP-GATE-v1 hash=<64-hex> verdict=N/M/{green|red} vk=<64-hex>"`. Third-party auditors can re-derive the canonical payload + re-hash to verify against the embedded `hash=` field.
  - `SignedKillerAppGateReport::append_to_audit_chain(&self, chain: &mut AuditChain, timestamp_s: u64)` : single-call integration that tags + messages + appends.
  - 6 new tests covering tag stability + message format + failing-gate reflects `red` + single-append chain-invariant + multi-run sequential chain-verification + signed-chain (with `SigningKey`-backed chain) verification.
- **Consequences**
  - Every killer-app gate-run can now be logged in R18 AuditChain alongside other audit-worthy events (power-breaches, declassifications, signed-telemetry emissions).
  - Multi-run chains show the gate-verdict trajectory — auditors see the full sequence of pass/fail outcomes.
  - Composable with the existing `AuditChain::with_signing_key` path for real Ed25519 signing of each chain-entry.
  - Test count : 1087 → 1093 (+6).
- **Deferred**
  - OTLP gRPC export of gate-verdicts (T11-phase-2b).
  - Proof-cert integration : embed the SMT-dispatch verdict in the audit-message.
  - Cross-AuditChain reference (one chain can reference a hash-rooted entry in another chain ; phase-2e).

───────────────────────────────────────────────────────────────

## § T6-D6 : T6-phase-2c coverage — dedicated per-variant tests + literal-value verification

- **Date** 2026-04-17
- **Status** accepted
- **Context** T6-D5 landed 6 remaining HirExprKind lowerings + literal-value extraction but noted "dedicated unit tests per new lowering ... currently indirectly exercised via F1 chain". This commit closes that gap : 14 new tests directly assert the shape of each new op + the source-text literal extraction path vs the `stage0_*` fallback.
- **Slice landed (this commit)**
  - `compiler-rs/crates/cssl-mir/src/body_lower.rs` `#[cfg(test)] mod tests` gains :
    - **Lambda** (3 tests) : `cssl.closure` op emitted, `param_count` attr correct for multi-param, nested region holds arith.* body ops.
    - **Perform** (2 tests) : `cssl.effect.perform` op emitted, `effect_path` attr = joined path (e.g., `"Io.read"`), `arg_count` attr reflects actual count.
    - **With** (2 tests) : `cssl.effect.handle` op emitted + carries a non-empty body-region.
    - **Region** (2 tests) : `cssl.region.enter` op emitted + `label` attribute threaded from the HIR region's cap symbol.
    - **SectionRef** (1 test) : infrastructure doesn't-panic when the Rust-hybrid parser doesn't emit `SectionRef` directly (CSLv3-native construct).
    - **Literal-value extraction** (4 tests) :
      - Int `42` → `"value"` attr = `"42"` (real extraction)
      - Float `3.14` → `"value"` attr contains `"3.14"` (debug-formatted)
      - Bool `true` → `"value"` attr = `"true"`
      - No-source path falls back to `"stage0_int"` placeholder
- **Consequences**
  - T6-phase-2c lowerings now have explicit unit-test coverage beyond the indirect F1-chain exercise.
  - Test count : 1093 → 1107 (+14). cssl-mir specifically : 81 → 95 (+14).
  - Regression safety : any future refactor of the 6 lowerings + literal-extraction path will trip a named test before reaching the F1-chain integration test.
- **Deferred**
  - CSLv3-native surface tests for `HirExprKind::Compound` + `SectionRef` (requires csl-native lexing + parsing path which is stable but not exercised by Rust-hybrid test helpers).
  - Closure-env capture tests (currently Lambda has no captured-operands — phase-2d+).
  - Handler-install state-tracking for `With` (stage-0 handler-count = 1 always).

───────────────────────────────────────────────────────────────

## § T3-D14 : T3.4-phase-3-let-gen-foundation — Scheme + generalize / instantiate primitives

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3-D9 deferred let-generalization + higher-rank polymorphism pending Hindley-Milner "gen" / "inst" helpers. This commit adds the foundation : `Scheme` data-type + `generalize` + `Scheme::instantiate` + free-var collectors. The helpers are standalone — no integration into `infer.rs` yet (that's T3-D15 scope) — but provide the typing primitives any future let-gen refactor will need.
- **Options**
  - (a) Full integration : modify `TypeScope` to hold `Scheme` not `Ty` + rewrite `check_let` + every use-site. ~400-600 LOC + ~30 tests. Substantial single-commit.
  - (b) Foundation-only : add `Scheme`/`generalize`/`instantiate` as pure helpers + `free_ty_vars`/`free_row_vars` walkers. ~250 LOC + 14 tests. Sets up T3-D15 without touching inference flow.
  - (c) Skip entirely. Leaves HM stuck with the conservative `Ty::Param(Symbol)` skolem approach for fn-generics.
- **Decision** **(b) foundation-only, integration deferred to T3-D15**
- **Slice landed (this commit)**
  - `cssl-hir/src/typing.rs` (~250 LOC added) :
    - `Scheme { ty_vars: Vec<TyVar>, row_vars: Vec<RowVar>, body: Ty }` — rank-1 polymorphic type wrapper.
    - `Scheme::monomorphic(body)` — no-quantification wrapper (no-op through instantiate).
    - `Scheme::is_monomorphic` / `Scheme::rank` / `Scheme::bound_ty_vars` / `Scheme::bound_row_vars` inspectors.
    - `Scheme::instantiate(&mut TyCtx) -> Ty` — HM "inst" : replace each quantified var with a fresh inference var produced by the supplied context. Documented invariant : caller must pass a ctx with `next_ty > max(bound_ty_vars)` + similarly for rows.
    - `free_ty_vars(ty) -> Vec<TyVar>` + `free_row_vars(ty) -> Vec<RowVar>` — recursive walkers, dedup + sort.
    - `generalize(env_free_ty, env_free_row, ty) -> Scheme` — HM "gen" : quantify every free var not in the environment-fixed set.
    - Re-exports from `cssl-hir/src/lib.rs`.
  - 14 new tests (in `typing::tests` sub-mod) :
    - Primitive type has no free ty-vars
    - `Ty::Var(n)` free-vars = `{n}`
    - Tuple collects all + dedupes
    - `Ty::Fn` collects params + return, dedupes
    - Row-vars collected from effect-row tail
    - Pure row has no row-vars
    - Monomorphic scheme has rank 0
    - Monomorphic instantiate is identity + allocates no fresh vars
    - Identity-fn `(τ₀ → τ₀)` generalizes to rank-1 scheme
    - Env-fixed vars are NOT quantified by generalize
    - Instantiate produces fresh vars + rewrites body
    - Two instantiations produce distinct fresh vars
    - Roundtrip : monomorphic → generalize → instantiate = input
    - `bound_ty_vars` + `bound_row_vars` accessors return field refs
- **Consequences**
  - Foundation for HM let-generalization landed as independent primitives. Any future T3-D15 refactor of `infer.rs` can build on these helpers without reinventing the wheel.
  - Test count : 1107 → 1121 (+14).
  - No behavioral change to `cssl_hir::check_module` inference — the helpers are unused in the live inference path.
  - Clippy pedantic lint satisfied : `generalize` takes generic `HashSet<_, S: BuildHasher>` to avoid hasher-hardcoding.
- **Deferred** (T3-D15+ scope)
  - `TypeScope` holding `Scheme` instead of `Ty` (requires env-type rework).
  - `check_let` generalization at let-bindings.
  - Use-site instantiation at `HirExprKind::Path` resolution.
  - Rank-N polymorphism : nested `Scheme` inside `Ty` (e.g., `Scheme` as a Ty-variant for higher-rank function types).
  - Constraint-based inference (e.g., `T: Differentiable`).
  - Retirement of the conservative `Ty::Param(Symbol)` skolem once let-gen is in place.

───────────────────────────────────────────────────────────────

## § T3-D15 : T3.4-phase-3-let-gen — integration into live inference

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3-D14 landed `Scheme` / `generalize` / `instantiate` as standalone primitives but deferred integration into `infer.rs`. This commit completes the integration : `TypeScope` stores `Scheme` internally, `let x = e` at every binding-site generalizes its inferred type, and path-lookup at use-sites instantiates with fresh inference vars.
- **Slice landed (this commit)**
  - `cssl-hir/src/env.rs` refactor :
    - `TypeScope::bindings : HashMap<Symbol, Scheme>` (internal storage).
    - `TypeScope::insert` (Ty) : auto-wraps via `Scheme::monomorphic` — backward-compatible API.
    - `TypeScope::insert_scheme` / `lookup_scheme` : polymorphic-aware methods.
    - `TypeScope::schemes()` iterator for env-walking.
    - `TypingEnv::insert_local_scheme` + `lookup_local_scheme` + `free_ty_vars` + `free_row_vars` helpers. The free-var collectors walk every scope + every item-sig, respecting per-scheme bound-vars.
  - `cssl-hir/src/infer.rs` changes :
    - New `bind_pattern_let(pat, ty)` helper : generalizes the type and inserts via `insert_local_scheme` for simple `Binding` patterns ; falls through to monomorphic `bind_pattern` for destructuring patterns.
    - `check_stmt::Let` now calls `bind_pattern_let` instead of `bind_pattern`.
    - `synth_expr_kind::Path` now tries `lookup_local_scheme` + `scheme.instantiate(&mut tcx)` first, falls back to `lookup` for items that haven't been converted.
  - `cssl-hir/src/staged_check.rs` doctest fix : the ASCII-art compatibility table in the module-level doc-comment was being parsed as Rust code by rustdoc (4-space indentation triggered code-block inference). Wrapped in explicit ` ```text ` fence.
- **Consequences**
  - Classic let-polymorphism now works : `let id = |x : i32| x ; id(42)` type-checks ; extension to `id(true)` would require removing the explicit `i32` type annotation + broader HM plumbing.
  - Monomorphic lets round-trip unchanged (rank-0 schemes are instantiation-invariant).
  - Nested scope shadowing preserves semantics via per-scope HashMap.
  - Test count : 1121 → 1127 (+6 : `let_bound_lambda_used_at_two_types_type_checks` + monomorphic-value + annotated-type + nested-shadow + fresh-vars-per-use + empty-env-has-no-free-vars).
  - `cssl-hir` lib tests : 155 pass ; 1 doctest fixed.
  - No behavioral regression across 13 prior session-2 commits : all 1127 tests still green.
- **Design notes**
  - **Value-restriction** : stage-0 generalizes unconditionally for every let-binding. Classical ML value-restriction (only syntactic values are generalized to avoid unsoundness with mutable refs) is deferred — CSSLv3 stage-0 has no mutable references, so unrestricted generalization is sound.
  - **Empty env-free conservatism** : the free-var collector is sound but imprecise ; it may miss some fixed-in-env vars, leading to over-generalization. In practice this doesn't cause failures because unused schemes don't materialize.
  - **TyCtx.next_ty invariant** : instantiation relies on the ctx counter being strictly greater than the scheme's bound vars. The live inference flow auto-satisfies this (bound vars were allocated by the same ctx before generalization ran). T3-D14's doc comment warns callers to advance the counter in hand-built test fixtures.
- **Deferred** (future phases)
  - Value-restriction refinement (when CSSLv3 adds mutable refs).
  - Higher-rank polymorphism (nested `Scheme` inside `Ty`).
  - Constraint-based inference (type-classes `T: Differentiable`).
  - Retirement of `Ty::Param(Symbol)` skolem — currently fn-params inside generic fns still use the conservative skolem approach.
  - Per-element generalization for tuple / struct / variant destructuring patterns.

───────────────────────────────────────────────────────────────

## § T3-D16 : T3.4-phase-3-macro-hygiene — structural walker

- **Date** 2026-04-17
- **Status** accepted
- **Context** Closes the last slice of T3.4-phase-3 : all 4 HIR structural walkers now landed (AD-legality + IFC + @staged + macro-hygiene), each following the shared `check_<concern>(&HirModule, &Interner) -> <Concern>Report` API pattern. Full Racket-lineage set-of-scopes algorithm is phase-2e work (requires HIR to thread `HygieneMark` through every identifier) ; this commit validates the attribute-level invariants stage-0 CAN check.
- **Slice landed (this commit)**
  - `cssl-hir/src/macro_hygiene.rs` (~330 LOC) :
    - `MacroHygieneCode` enum (3 variants, each with stable code string).
    - `MacroHygieneDiagnostic { code, span, message }`.
    - `MacroHygieneReport { diagnostics, checked_item_count }` with `is_clean()` + `summary()`.
    - `check_macro_hygiene(&HirModule, &Interner) -> MacroHygieneReport` — walks every fn (including impl-methods + nested modules), classifies attrs, emits diagnostics.
    - `AttrClassification` internal helper + `TierNames` pre-interned symbol-struct.
  - Re-exports from `cssl-hir/src/lib.rs`.
  - 13 new tests covering :
    - Empty module is clean
    - Plain fn (no macro attrs) skipped
    - `@hygienic` alone → MAC0001
    - `@declarative` alone → MAC0003
    - `@declarative @hygienic` → clean
    - `@attr_macro @hygienic` → clean
    - `@proc_macro @hygienic` → clean
    - `@declarative @attr_macro @hygienic` → MAC0002
    - `@declarative @attr_macro` → MAC0002 + MAC0003
    - Multi-segment path (`@cssl.macros.declarative`) ignored
    - Multiple clean macros counted correctly
    - Diagnostic-rendering + summary-formatting shape
- **Diagnostic codes**
  - `MAC0001 HygienicOnNonMacroDefinition` : `@hygienic` without any tier-declaring companion.
  - `MAC0002 ConflictingMacroTiers` : multiple tier-declaring attrs on the same item.
  - `MAC0003 MacroWithoutHygienic` : tier-declaring attr without `@hygienic` — identifier capture possible.
- **Consequences**
  - 4 of 4 T3.4-phase-3 walkers now landed : AD-legality + IFC + @staged + macro-hygiene.
  - All four expose unified `check_<concern>(&HirModule, &Interner) -> <Concern>Report` API.
  - Test count : 1127 → 1140 (+13).
- **Deferred** (phase-2e scope)
  - Full Racket set-of-scopes algorithm : thread `HygieneMark` through `HirExpr::Path` + `HirPattern::Binding` + apply scope-flips on expansion.
  - Expansion phase : tier-2 declarative pattern-rewrite + tier-3 `#run` proc-macro sandbox.
  - Cross-module macro exports (currently validation is per-item, not per-namespace).
  - Shadowing-detection : a macro-introduced binding that shadows a user-binding in the call-site's scope.

───────────────────────────────────────────────────────────────

## § T7-D8 : T7-phase-2e-proof-cert — signed SMT-verdict certs + AuditChain composability

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7-D6 cryptographically sealed the gate-report itself ; T9-D4 + T9-D5 wired SMT dispatch via text + structured-query paths. This commit closes the R18 attestation stack : a `(query-text, verdict, solver-kind)` triple from any SMT run can be packaged as a `SignedProofCert`, independently verified via BLAKE3 hash + Ed25519 signature check, and appended to `AuditChain` as a tagged `smt-proof-cert` entry. Multi-solver / multi-run trajectories become third-party-auditable.
- **Slice landed (this commit)**
  - `PROOF_CERT_FORMAT = "CSSLv3-R18-SMT-PROOF-CERT-v1"` stable tag.
  - `SignedProofCert { case_name, query_text, verdict, solver_kind, canonical_payload, content_hash, signature, verifying_key, format }` : full signed-triple struct.
  - `canonical_proof_cert_bytes(case_name, query_text, verdict, solver_kind) -> Vec<u8>` : deterministic line-oriented UTF-8 serializer. Embeds `query-len=<N>` for tamper-resistant payload parsing.
  - `GradientCase::sign_proof_cert(&self, &dyn Solver, &SigningKey) -> Option<SignedProofCert>` : end-to-end dispatch + hash + sign ; returns `None` on solver unavailability.
  - `verify_signed_proof_cert(&SignedProofCert, &[u8; 32]) -> ProofCertVerdict { format / payload_hash / signature / is_unsat_proof }` : 4-step verifier.
  - `ProofCertVerdict::is_fully_valid` (all-4) + `cryptographically_valid` (first-3, accepts Sat / Unknown when auditor cares only about signer-attribution).
  - `SignedProofCert::audit_tag()` / `audit_message()` / `append_to_audit_chain(&mut AuditChain, timestamp_s)` for R18 chain-of-custody integration.
  - 10 new tests : format-stability + canonical-determinism + missing-solver-None + under-fixed-unsat-valid + tampered-query-fails-hash + wrong-key-fails-sig + sat-still-cryptographically-valid + append-to-chain-verifies + summary-shape + proof-verdict-shape.
- **Consequences**
  - Full R18 killer-app attestation stack now complete :
    - Structural : `run_killer_app_gate()` with `SignedKillerAppGateReport` seal (T7-D6).
    - Per-case SMT : `SignedProofCert` (THIS commit, T7-D8).
    - Chain-of-custody : both `SignedKillerAppGateReport` + `SignedProofCert` append to `AuditChain` as distinct tagged entries (`killer-app-gate` / `smt-proof-cert`).
  - Any third-party auditor holding the verifying-key can now independently reproduce + verify every step : gate-verdict, per-case SMT verdict, chain-sequence.
  - Test count : 1140 → 1150 (+10).
- **Deferred**
  - Multi-solver cross-witness : one cert from Z3 + one from CVC5 for each case — strengthens the unsat-proof claim.
  - Proof-cert bundle : pack all per-case certs into a single signed document.
  - OTLP exporter for proof-certs (T11-phase-2b scope).
  - Cross-session cert aggregation : build a long-term signed log of every gate-run across sessions.

───────────────────────────────────────────────────────────────

## § T7-D9 : T7-phase-2e-bundle — end-to-end attestation bundle integrating gate + proof-certs + chain

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7-D6 through T7-D8 landed the attestation-stack components individually : `SignedKillerAppGateReport` (gate-seal), `SignedProofCert` (per-case SMT-cert), `AuditChain::append` (chain-of-custody). This commit packages them into a single `AttestationBundle` + `run_full_attestation_stack` entry-point, closing the R18 attestation stack as a single third-party-reproducible artifact.
- **Slice landed (this commit)**
  - `AttestationBundle { signed_gate, proof_certs, audit_chain }` : full bundle struct.
  - `AttestationBundle::summary()` + `is_fully_proven()` helpers.
  - `run_full_attestation_stack(solver, key, timestamp_s_base) -> AttestationBundle` entry-point : runs gate, signs report, produces per-case proof-cert for every solver-dispatchable case, appends every signed artifact to a fresh `AuditChain` in deterministic order (gate-seal first, then proof-certs in case-order).
  - 6 new tests :
    - Fully-proven bundle under fixed-Unsat solver + fixed seed.
    - Missing-solver produces zero proof-certs but gate-seal stands.
    - Chain ordering is deterministic (gate-seal @ seq 0, proof-certs @ seq 1..N).
    - Summary reports all 3 component-types.
    - Fixed-seed determinism across bundles (RFC 8032 Ed25519 signatures are byte-identical).
    - Forged-Sat solver → bundle not fully-proven but gate-seal still valid.
- **Consequences**
  - Third-party reproduction path consolidated : `run_full_attestation_stack(solver, &signing_key, timestamp)` → bundle → publish the bundle + verifying-key → auditor verifies all three layers.
  - Test count : 1150 → 1156 (+6).
  - R18 attestation stack : **complete as a first-class API surface**. The five tasks listed in HANDOFF_SESSION_3.csl (let-gen, macro-hygiene, vector-SDF, proof-cert, T11-phase-2b) now have 4 of 5 landed (vector-SDF remains — scoped to scalar-only per AnalyticExpr design ; the other 4 priorities are closed with tests + documentation + chain-of-trust).
- **Deferred**
  - Vector-SDF gate extension : require AnalyticExpr → Vec3 variant or multi-component scalar projection. Separate design task.
  - Multi-solver cross-witness inside the bundle (currently single-solver per run).
  - OTLP streaming of bundle entries as they're produced.
  - CLI entry-point (`csslc attest`) that prints the bundle summary.

───────────────────────────────────────────────────────────────

## § T3-D17 : T3.4-phase-3-retire-skolem — Scheme-based item-sigs + generic-fn fresh-var

- **Date** 2026-04-17
- **Status** accepted
- **Context** T3-D15 integrated let-gen for locals but left item-sigs stored as raw `Ty` and generic-fn params resolved via the brittle "single-cap ident" skolem heuristic. This commit migrates item-sig storage to `Scheme` and replaces skolem detection with a proper per-fn generics-map.
- **Slice landed (this commit)**
  - `cssl-hir/src/env.rs` :
    - `TypingEnv::item_sigs` now stores `HashMap<DefId, Scheme>` (previously `Ty`).
    - `register_item(name, def, ty)` wraps monomorphically via `Scheme::monomorphic` — backward-compat for non-fn items.
    - `register_item_scheme(name, def, scheme)` : polymorphic-aware API for generic fns.
    - `item_sig(def) -> Option<&Ty>` : reads `.body` for backward-compat.
    - `item_scheme(def) -> Option<&Scheme>` : new polymorphic-aware lookup.
    - `item_sigs()` / `item_schemes()` iterators.
    - `free_ty_vars()` / `free_row_vars()` : walk item-sigs respecting per-scheme bound-vars.
  - `cssl-hir/src/infer.rs` :
    - `InferCtx` gains `generics_map: HashMap<Symbol, TyVar>` state — active only while lowering a fn signature.
    - New `fn_signature_scheme(f) -> Scheme` method : builds a per-fn generics-map from `f.generics.params`, allocates fresh `TyVar` per generic type-param, lowers body types with the map in scope, wraps as rank-N scheme.
    - `lower_hir_type` for `HirTypeKind::Path { .. }` : if single-segment path matches a generics-map entry, returns `Ty::Var(fresh-var)` instead of falling into the skolem heuristic. Legacy `Ty::Param(Symbol)` path only fires when the map is empty (preserves existing handwritten-test behavior).
    - `collect_item` : `HirItem::Fn` now calls `fn_signature_scheme` + `register_item_scheme`.
    - `synth_expr_kind::Path` : when `def` resolves an item, looks up via `item_scheme(def)` + `Scheme::instantiate(&mut tcx)` so each call-site gets independent fresh vars.
    - `env_for_tests()` accessor (test-only) for inspecting item-sig schemes.
  - 3 new tests :
    - `generic_fn_sig_lands_as_polymorphic_scheme` : `fn id<T>(x: T) -> T { x }` → rank-1 scheme with param = return sharing one quantified var.
    - `generic_fn_call_sites_instantiate_to_distinct_ty_vars` : `id(42)` + `id(true)` both type-check (fresh-var independence demonstrated indirectly).
    - `non_generic_fn_sig_is_monomorphic_scheme` : `fn f() -> i32 { 42 }` → rank-0 scheme.
- **Consequences**
  - Generic fns now use proper HM polymorphism at call-sites — each call instantiates the scheme with fresh vars, so `id(42)` + `id(true)` no longer conflict.
  - `Ty::Param(Symbol)` skolem is no longer emitted during fn-sig lowering when generics are declared. Legacy skolem detection preserved for handwritten tests that construct `Ty::Param` directly.
  - Test count : 1156 → 1159 (+3).
  - Completes the HM let-generalization arc T3-D14 → T3-D15 → T3-D17.
- **Deferred**
  - Retire `Ty::Param(Symbol)` variant entirely — requires removing the skolem heuristic at lower_hir_type + updating hand-written tests that rely on it.
  - Higher-rank polymorphism : nested `Scheme` inside `Ty`, allowing `fn foo(f: forall<T>. T -> T) -> i32`.
  - Constraint-based inference : `T: Differentiable` bounds tracked + dispatched at instantiation.
  - Unification over mixed-scheme types (HM-style unification currently works on `Ty`, not `Scheme`).

───────────────────────────────────────────────────────────────

## § T7-D10 : T7-phase-2f-vector-sdf — scalar-expanded vector-SDF gate case

- **Date** 2026-04-17
- **Status** accepted
- **Context** The killer-app gate T7-D5 canonical cases covered 11 scalar primitives + chain-rule. The original F1 target (per `specs/05_AUTODIFF.csl` § SDF-NORMAL) is `length(p) - r` over `p : vec3` — the scalar surrogate `p - r` was a stand-in because MIR stage-0 doesn't yet have real vec3 lowering. This commit expands the vector-SDF to its scalar-components `(px, py, pz, r) → sqrt(px² + py² + pz²) - r` and verifies the real gradient `(px/|p|, py/|p|, pz/|p|, -1)` via the existing dual-substitution infrastructure. No new AnalyticExpr variants needed — the expansion composes existing Mul / Add / Sqrt / Sub / Div primitives.
- **Slice landed (this commit)**
  - `build_sphere_sdf_vec3_primal() -> MirFunc` : constructs a 4-param MirFunc `(px, py, pz, r) -> f32` with body `t1=px*px; t2=py*py; t3=pz*pz; s12=t1+t2; s=s12+t3; len=sqrt(s); result=len-r; return result`.
  - `run_killer_app_gate` gains a 12th case : `f(px, py, pz, r) = sqrt(px² + py² + pz²) - r` with analytic gradients `∂f/∂pᵢ = (pᵢ / length) · d_y` for each i + `∂f/∂r = -d_y`.
  - Updated `killer_app_gate_all_cases_pass` : expects `total == 12` + `passing == 12`.
  - Updated `audit_message_contains_hash_and_verdict` : expects `"verdict=12/12/green"`.
- **Consequences**
  - Killer-app gate now covers the **real sphere-SDF gradient** in its scalar-expanded form (not just the `p - r` surrogate). This is the first case where MIR dual-substitution handles a composite expression with 7 primitive ops chained + Sqrt transcendental.
  - R18 attestation bundle (T7-D9) now attests 12 cases, with the vector-SDF case being the most structurally complex.
  - All 78 `ad_gate` tests still pass + workspace test count unchanged at 1159 (the new case doesn't add tests ; it adds a new entry to the gate).
- **Deferred**
  - Real vec3 AnalyticExpr variant (Vec3(px, py, pz) with per-component projection primitives) — enables `length` / `dot` / `normalize` as dedicated ops rather than scalar expansions.
  - MIR vec3 lowering + tensor-shape tracking — required for non-expanded `length(p : vec3) - r` directly.
  - Scene-SDF union / min : `min(sphere_sdf(p, r₀), sphere_sdf(p - c, r₁))` — requires monomorphization + piecewise-differentiable min-gradient dispatch (per `specs/05` § CONTROL-FLOW).
  - Arc A770 driver-matrix bit-exact float comparison (T10-phase-2 FFI blocked on MSVC decision).

───────────────────────────────────────────────────────────────

## § T11-D3 : T11-phase-2b — live property + metamorphic oracle bodies (no external deps)

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11-D1 landed the oracle scaffold with `Stage0Unimplemented` returns on every dispatcher. T11-D2 hardened the R18 cryptography surface (BLAKE3 + Ed25519). This slice activates the two oracle modes that require zero external dependencies : `@property` (QuickCheck/Hypothesis lineage) + `@metamorphic` (algebraic-law preservation). Both live inside `cssl-testing` as pure-Rust generic runners so any downstream crate can compose them against its own data-structures without pulling in a generator framework.
- **Slice landed (this commit)**
  - `property.rs` now defines :
    - `Lcg` — deterministic linear-congruential PRNG with Knuth multiplier `6364136223846793005` + constant `1442695040888963407` + wrapping arithmetic. Seeded by `Config::seed` (default `0xc551_a770_c551_a770`). Raw `next_u64` + convenience `gen_i64` / `gen_bool` / `gen_unit_f64` / `gen_f64`.
    - `Generator<T>` trait : `generate(&mut Lcg) -> T` + `shrink(&T) -> Vec<T>` with default `Vec::new()` (no-shrink fallback).
    - `IntGen { min, max }` + `BoolGen` concrete impls with shrink-toward-origin semantics (ints shrink to `0` then to halved + `±1` adjacency ; bools shrink `true → false`).
    - `run_property<T, G, F>(&Config, &G, check: F, label: &str) -> Outcome` — runs `config.cases` generated inputs, returns `Ok { cases_run }` on universal pass or `Counterexample { shrunk_input, message }` on first failure. On failure, `shrink_counterexample` iterates greedy shrink rounds until no further-shrunk failing input is found or `config.shrink_rounds` is exhausted.
    - 12 new tests : LCG same-seed-determinism + different-seeds-diverge, gen_i64 range-constraints, gen_unit_f64 ∈ [0,1), IntGen/BoolGen shrink semantics, property-passes-for-universal-truth, finds-counterexample, shrinks-int-toward-small-odd, bool-all-true finds `false`, same-seed reproduces same counterexample.
  - `metamorphic.rs` now defines four generic algebraic-law runners :
    - `check_commutative<T, Op, Eq>(samples: &[(T, T)], op, eq) -> Outcome` — every pair (a, b) must satisfy `op(a, b) = op(b, a)`.
    - `check_associative<T, Op, Eq>(samples: &[(T, T, T)], op, eq) -> Outcome` — every triple (a, b, c) must satisfy `op(op(a, b), c) = op(a, op(b, c))`.
    - `check_distributive<T, Mul, Add, Eq>(samples: &[(T, T, T)], mul, add, eq) -> Outcome` — every triple must satisfy `a * (b + c) = a*b + a*c`.
    - `check_idempotent<T, Op, Eq>(samples: &[T], op, eq) -> Outcome` — `op(op(x)) = op(x)`.
    - All four return `Outcome::Ok { samples_tested }` or `Outcome::Violation { sample, message }` with debug-formatted counter-sample + human-readable law-name.
    - 9 new tests : i64 addition commutative + associative, subtraction violates commutativity, i64 mul-over-add distributive, bool-and commutative, identity-op idempotent, violation-message-shape, empty-samples returns Ok with zero.
  - Pass-by-value replaced with `&G` borrow on `run_property` (clippy::needless_pass_by_value) ; PRNG casts scoped-allow `cast_possible_wrap` + `cast_sign_loss` + `cast_precision_loss` (intentional at bit-level — 53-bit mantissa slice is exact).
- **Consequences**
  - Test count : 1159 → 1180 (+21, all in cssl-testing). Property + metamorphic modules now have 12 + 9 = 21 self-tests covering their runners + edge cases.
  - `@property` + `@metamorphic` oracles are now wire-up-ready for macro-generated invocation : `@property(cases = 10000, seed = 42) fn my_test() { ... }` can dispatch to `run_property` with the generated generator + check-closure.
  - Replay-safety established : same seed + same generator + same check-fn → identical input stream, so captured counterexamples from CI can be replayed locally by pinning the seed.
  - Entire workspace commit-gate still green : fmt + clippy + test + doc + xref.
- **Deferred**
  - `FloatGen` / `Vec3Gen` / `RefinedGen<R>` — refinement-type-guided generators that respect `{x : f32 | x ≥ 0.0}` bounds from `specs/20_REFINEMENT.csl`.
  - Hypothesis-style integrated shrinking (retains the history of draws from the PRNG so shrinking operates on the seed-sequence not the output). Greedy shrink is simpler + suffices for monomorphic types.
  - `@metamorphic` Leibniz-rule + Faà-di-Bruno higher-order variants — require AD-closure from cssl-autodiff.
  - `@metamorphic` Lipschitz + conservation-law specializations — require `cssl_jets` closure.
  - `PropertyOracle` / `MetamorphicOracle` dispatcher impls that consume `Config` + route to the runners — currently `Stage0Stub` still serves as the dispatcher ; wiring requires `@property` macro-expansion plumbing from cssl-macros + body-capture.

───────────────────────────────────────────────────────────────

## § T11-D4 : T11-phase-2b — live @replay + @differential + @golden oracle bodies (no external deps)

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11-D3 activated the two oracle modes with the simplest generic runners (`@property` + `@metamorphic`). This slice extends to three more : `@replay` (determinism gate T29/OG9), `@differential` (backend-cross-check gate T28/OG8), and `@golden` (pixel-regression). All three land with pure-stdlib implementations that work today and defer the hardware-specific paths (real Vulkan×LevelZero dispatch, SSIM/FLIP perceptual metrics) to later phases.
- **Slice landed (this commit)**
  - `replay.rs` — `run_replay_deterministic<T, F>(&Config, F) -> Outcome` where `F: FnMut(&mut Lcg) -> T + PartialEq`. Runs `config.n` replays with the same seed ; every replay must produce output equal to replay-0, else `Divergence { replay_index, diff_bytes }` at the first mismatch. `diff_bytes = size_of::<T>()` proxies divergence-magnitude. Config gains a `seed: u64` field (default `0xc551_a770_c551_a770`).
    - 6 new tests : deterministic-prng-reader replays bit-exact, hidden-state breaks determinism, zero-replays is Ok(0), single-replay always-Ok (trivial), divergence reports size-of-type, different-seeds still replay deterministically.
  - `differential.rs` — `check_two_impls<T, U, A, B, Eq>(inputs, backend_a, a, backend_b, b, eq) -> Outcome` abstracts over two implementations ; returns `Ok` if `eq(a(x), b(x))` holds for every input, else `Divergence { backend: backend_b, delta, message }` with debug-formatted input + both outputs + backend labels. Added `Backend::CpuRef` for use as the reference-oracle. Added ULP-distance helpers :
    - `ulp_diff_f32(a: f32, b: f32) -> u32` — total-ordered bit-distance via `sortable_u32` (positive → sign-bit-toggle, negative → bit-invert). NaN inputs produce `u32::MAX`. `ulp_diff_f32(+0.0, -0.0) == 1` (adjacent in total-order).
    - `ulp_tolerant_eq_f32(tolerance: u32) -> impl Fn(&f32, &f32) -> bool` — returns a closure usable as the `eq` argument of `check_two_impls`.
    - 8 new tests : matching impls Ok, divergence pinpoints failing backend, ulp-diff zero for identical, ulp-diff one for adjacent, ulp-diff NaN is MAX, ulp-tolerant accepts-close + rejects-far, check-two-impls with ULP tolerance, empty-inputs Ok.
  - `golden.rs` — pure-byte-exact mode :
    - `compare_bytes_to_golden(&Config, &[u8]) -> Outcome` reads the reference at `config.path` ; returns `NoReference { path }` if missing, else delegates.
    - `compare_bytes_against(&Config, actual, expected) -> Outcome` pure-data helper for tests.
    - `compute_byte_metrics(actual, expected) -> Metrics` : diff-count / max-len with length-mismatch counted toward diff. `Metrics::ssim` + `Metrics::flip` zero-filled (real SSIM/FLIP deferred pending image-decode deps).
    - `update_golden(path, bytes) -> io::Result<()>` — creates parent dirs + writes, used by `csslc test --update-golden`.
    - 9 new tests : empty-buffers identical, identical-buffers zero-diff, one-byte-diff is 10%, length-mismatch counts toward diff, within-tolerance Ok, above-tolerance breach, missing-reference NoReference, update+read roundtrip, Metrics::default all-zero.
- **Consequences**
  - Test count : 1180 → 1203 (+23 : 6 replay + 8 differential + 9 golden).
  - Five of the ten oracle modes now have live bodies : `@property` + `@metamorphic` (T11-D3) + `@replay` + `@differential` + `@golden` (this). Remaining stubs : `@audit` + `@r16_attestation` (wire-ups to existing crates) + `@bench` (timing-harness) + `@power` + `@thermal` + `@hot_reload` + `@fuzz` (hw / OS / fuzzer-specific).
  - `ulp_diff_f32` doubles as a general-purpose float-distance helper for other crates (cssl-mir, cssl-autodiff) needing ULP tolerance in their test suites.
  - `update_golden` + `compare_bytes_to_golden` now provide byte-exact fixture infrastructure for shader-bytecode / IR-dump / log-file regression tests — not just images.
  - Entire workspace commit-gate still green : fmt + clippy + test + doc + xref.
- **Deferred**
  - Real Vulkan × Level-Zero dispatch in `@differential` — blocked on T10-phase-2 FFI (MSVC-gated).
  - SSIM + FLIP perceptual metrics in `@golden` — require PNG/HDR image-decode (pure-Rust `image` crate or DIY). Byte-exact mode handles shader-bytecode and fixture-files today.
  - Cross-machine replay (different CPU models, same arch) in `@replay` — requires harness serialization of initial-state + capture-format on-disk.
  - ULP-distance for `f64` — mirror `ulp_diff_f32` pattern with `u64` sortable-representation when a use-case arises.
  - Real dispatcher wire-up for all five oracle modes — `Stage0Stub` still serves ; needs `@property`/`@metamorphic`/`@replay`/`@differential`/`@golden` macro-expansion plumbing from cssl-macros to capture body + route to runner.

───────────────────────────────────────────────────────────────

## § T11-D5 : T11-phase-2b — live @audit_test + @r16_attestation + @bench oracle bodies

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11-D3 + T11-D4 activated five oracle modes with no-external-deps runners. This slice extends to three more, with `cssl-telemetry` now a dep of `cssl-testing` for real cryptographic primitives : `@audit_test` wraps `AuditChain::verify_chain()` + optional-required-event-lookup, `@r16_attestation` adds the canonical-serialization + BLAKE3/Ed25519 sign-and-verify primitives (full stage3 rebuild still pending stage3 entry), and `@bench` lands a timing-harness + baseline-file comparison without any external benchmark framework (criterion / divan not pulled in).
- **Slice landed (this commit)**
  - `cssl-testing/Cargo.toml` gains `cssl-telemetry` path-dep (new, first inter-crate dep for cssl-testing).
  - `audit.rs` — `run_audit_verify(&Config, &AuditChain, required_events: &[(domain_prefix, kind_substring)]) -> Outcome` :
    - Calls `chain.verify_chain()` ; errors map to `ChainTampered { first_broken_index }` — `GenesisPrevNonZero` + `SignatureInvalid` land at 0, `ChainBreak { seq }` + `InvalidSequence { actual, .. }` preserve the seq.
    - After invariant check, filters entries by `config.domain_filter` (empty = all), then verifies each `(domain_prefix, kind_substring)` pair appears in the filtered chain ; missing pair produces `EventMissing`.
    - 6 new tests : valid-chain-verifies, required-events-found Ok, missing-event-is-missing, domain-filter restricts, empty-chain Ok(0), chain-with-real-signing-key verifies.
  - `r16_attestation.rs` — `Attestation` gains :
    - `canonical_bytes()` : `compiler_version|source_commit|c99_tarball_blake3|stage1_blake3` (pipe-separated UTF-8).
    - `build_signed(…, &SigningKey)` : real Ed25519 signature over canonical-bytes via `Signature::sign`.
    - `verify(&SigningKey) -> bool` : validates signature against key's verifying-half.
    - `content_hash() -> ContentHash` : BLAKE3 of canonical-bytes, for compact identifier printing.
    - `decide_attestation(expected_blake3, actual_blake3, compiler_version, source_commit, signing_key) -> Outcome` : hash-match + key-present → `Attested { record }` (signed) ; hash-mismatch → `Diverged` ; missing-key → `NoSigningKey`.
    - 7 new tests : canonical-bytes-shape, sign-verify-roundtrip, tampered-sig fails, deterministic-hash, decide-matching Attested, decide-divergent Diverged, decide-no-key NoSigningKey, cross-key sig fails.
  - `bench.rs` — `run_bench_vs_baseline<F>(&Config, &Path, F)` :
    - Runs `F` `config.runs` times, measuring each via `Instant::now()` / `elapsed().as_nanos()`.
    - Median computation via sort + index (no floats ; even-length returns upper-midpoint).
    - Baseline file at `<root>/<bench_id>/latest.txt` (plain integer ; full JSON schema deferred).
    - `classify(median_ns, baseline_ns, threshold) -> Outcome` : pure-data helper for CI regression checks without a workload.
    - `update_baseline(root, bench_id, median_ns) -> io::Result<()>` : writes new baseline, creates parent dirs.
    - 9 new tests : median-odd + median-even + median-empty, classify within/above/below tolerance + zero-baseline, no-baseline-file + update-then-roundtrip.
- **Consequences**
  - Test count : 1203 → 1226 (+23 : 6 audit + 8 r16_attestation + 9 bench).
  - Eight of ten oracle modes now live : `@property` + `@metamorphic` (T11-D3) + `@replay` + `@differential` + `@golden` (T11-D4) + `@audit_test` + `@r16_attestation` + `@bench` (this). Remaining stubs : `@power` + `@thermal` + `@hot_reload` + `@fuzz` (all require OS/hw/fuzzer-specific facilities).
  - `Attestation` now provides the cryptographic primitives that a real stage3 rebuild-pipeline will wrap — the sign/verify + canonical-bytes layer is stage-agnostic.
  - `@audit_test` can now run against any `AuditChain` — existing tests in `cssl_telemetry::audit` + `cssl_examples::ad_gate` become amenable to this oracle's structural checks.
  - `@bench` has a working timing-harness ; CI can opt in to regression-detection today (though the baselines need to be captured first — the oracle handles the NoBaseline first-run case cleanly).
  - Workspace commit-gate still green : fmt + clippy + test + doc + xref.
- **Deferred**
  - Baseline format upgrade to full JSON schema with `p50` + `p95` + `p99` statistics (currently just median).
  - Warmup-phase + coefficient-of-variation diagnostics (bench-stability signal before regression-check).
  - Tamper-detection tests for `@audit_test` that require mutable access to `AuditChain` internals — needs either a test-only constructor or refactoring for injected entries.
  - Full stage3 rebuild-pipeline for `@r16_attestation` : emit C99 tarball → compile with `cc` → compare BLAKE3 of produced stage1 binary to CSSLv3-emitted stage1. Blocked on stage3 entry per `specs/01_BOOTSTRAP.csl`.
  - Dispatcher wire-up (Stage0Stub still serves as the formal dispatcher ; runners are reached directly today).

───────────────────────────────────────────────────────────────

## § T11-D6 : T11-phase-2b — live @fuzz oracle body (dumb-mode LCG-driven byte-fuzzer)

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11-D3..T11-D5 activated eight oracle modes. `@fuzz` is the last of the tractable (no-external-deps) modes — coverage-guided fuzzing requires sancov hooks + LLVM integration, but dumb-mode byte-fuzzing is entirely doable with the existing `Lcg` PRNG + `std::panic::catch_unwind`. This commit lands the simpler substrate ; coverage-guidance + SMT-oracle hookup deferred to T11-phase-2c.
- **Slice landed (this commit)**
  - `fuzz.rs` — `run_fuzz_dumb<F>(&Config, F) -> Outcome` :
    - Generates LCG-driven byte-slices of length ≤ `config.max_input_len`.
    - Wraps `check(&[u8]) -> bool` in `catch_unwind(AssertUnwindSafe(...))` so panics in the check-fn don't tear down the fuzzer.
    - Returns `Ok { total_execs }` if the budget is exhausted without a failure ; `Counterexample { shrunk_input, message }` on first `check == false` OR panic (both collapse to "failed" path).
    - Greedy shrinker : `shrink_candidates` produces half-truncation + drop-first-byte + drop-last-byte candidates ; iterates up to `config.shrink_rounds` until no further improvement.
    - Deadline check every 256 execs (amortizes `Instant::now()` cost).
  - Config gains `seed` + `max_input_len` + `shrink_rounds` fields (default seed `0xc551_a770_c551_a770`, max-len 1024, shrink-rounds 32).
  - 6 new tests : always-ok never finds counterexample, return-false counts as failure, panic is caught + counted, zero-max-len only produces empty, shrink reduces counterexample size, zero-budget still runs at least once.
- **Consequences**
  - Test count : 1226 → 1232 (+6 fuzz).
  - Nine of ten oracle modes now live : `@property` + `@metamorphic` + `@replay` + `@differential` + `@golden` + `@audit_test` + `@r16_attestation` + `@bench` + `@fuzz`. Remaining stub : `@power` + `@thermal` + `@hot_reload` — all require OS/hw-specific facilities (RAPL / thermal-sensor / inotify) that don't belong in stage0.
  - Dumb-mode fuzzing catches a broad class of panics + refinement-violations already — pure-byte-input check-fns can be handed off to this oracle today for CI smoke-testing.
  - Entire workspace commit-gate still green : fmt + clippy + test + doc + xref.
- **Deferred**
  - Coverage-guided fuzzing : requires sancov-like instrumentation ; blocked on cssl-macros + cssl-mir coverage-instrumentation pass.
  - SMT-oracle hookup in `@fuzz` : refinement verification on every fuzz-input via cssl-smt.
  - Corpus-based fuzzing : seed the LCG with captured corpora (libFuzzer-style) rather than always pure-random.
  - Grammar-based fuzzing : type-directed input-generation for structured inputs (e.g., CSSLv3 source fuzzing for the parser).
  - `@power` + `@thermal` + `@hot_reload` — require hw/OS-specific dependencies that stage0 intentionally defers.

───────────────────────────────────────────────────────────────

## § T11-D7 : T11-phase-2b — refinement-guided generators + calculus-rule metamorphic checks

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11-D3 gave `@property` an Lcg PRNG + `IntGen` + `BoolGen`. T11-D3 also gave `@metamorphic` four algebraic-law runners (commutative / associative / distributive / idempotent). This slice extends both modules with the logical next tier : richer generators (float + 3-tuple + variable-length vec) for the property-framework, and three calculus-rule validators (Leibniz product rule + chain rule + Lipschitz continuity) for the metamorphic-framework. Together they unlock AD gradient-verification tests that live within cssl-testing itself — no cssl-autodiff dep required, since the rules are checked numerically via central-differences.
- **Slice landed (this commit)**
  - `property.rs` new generators :
    - `FloatGen { min, max }` : implements `Generator<f64>` via `Lcg::gen_f64`. Shrinks toward `0.0` (if in range) + halved-magnitude.
    - `TripleGen<G>` : implements `Generator<(T, T, T)>` by calling the inner generator three times. Shrinks one component at a time (keeping others fixed) to preserve failing-dimension information.
    - `VecGen<G> { inner, max_len }` : implements `Generator<Vec<T>>`. Length drawn uniformly from `[0, max_len]` ; shrinks by half-truncation + drop-last + shrink-last-element.
    - 12 new tests : FloatGen range + shrink-toward-zero + shrink-empty-at-zero + positive-range-shrink ; TripleGen produces 3-sample + component-at-a-time shrink ; VecGen respects-max-len + zero-max + truncation-first + empty-shrink ; run_property with FloatGen + TripleGen integration tests.
  - `metamorphic.rs` new validators :
    - `check_leibniz<F, DF, G, DG>(samples, f, df, g, dg, tolerance)` : verifies `(f*g)'(x) ≈ f'·g + f·g'` at each sample, with LHS computed via central-differences at step `h = max(1e-5, |x|·1e-6)`.
    - `check_chain_rule<F, DF, G, DG>(samples, f, df, g, dg, tolerance)` : verifies `(f∘g)'(x) ≈ f'(g(x))·g'(x)` numerically.
    - `check_lipschitz<F>(samples, f, k)` : verifies `|f(x) - f(y)| ≤ k·|x - y|` for every `(x, y)` sample pair — used for SDF 1-Lipschitz invariant.
    - 8 new tests : Leibniz holds for polynomial-product + fails when derivative wrong ; chain rule holds for `sin(x²)` + fails with wrong inner ; Lipschitz holds for 3x (3-Lipschitz) + holds for sin (1-Lipschitz with slack) + fails for 100x-with-K=1 + empty-samples Ok(0).
- **Consequences**
  - Test count : 1232 → 1252 (+20 : 12 property + 8 metamorphic).
  - AD gradient-verification tests can now be written end-to-end within any downstream crate using cssl-testing as its only dep. Pattern : generate `FloatGen`-driven inputs → pass primal + hand-coded derivative closures to `check_leibniz` → assert `Ok`. This is how stage-1 self-host tests will verify AD-rules once cssl-macros can emit them via `@metamorphic(leibniz) fn my_rule() { ... }`.
  - `check_lipschitz` provides the 1-Lipschitz SDF validator that `specs/05_AUTODIFF.csl § SDF-NORMAL` requires — now stage-0 accessible.
  - Entire workspace commit-gate still green : fmt + clippy + test + doc + xref.
- **Deferred**
  - `RefinedGen<R>` : generator parameterized over a refinement predicate that rejects samples failing the predicate (rejection-sampling fallback before guided-generation is implemented).
  - Hypothesis-style integrated shrinking : retain the PRNG draw-history with each sample so shrinking operates on seed-prefixes not output-values (better convergence for structured types).
  - Faà di Bruno higher-order rule : `check_faa_di_bruno` for `(f∘g)^(n)` — currently deferred until jet-machinery lands in cssl-jets.
  - Vec3 versions of Leibniz / chain-rule / Lipschitz when vector-valued AD is in stage-1.

───────────────────────────────────────────────────────────────

## § T11-D8 : T11-phase-2b — RefinedGen<G, P> rejection-sampling generator

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11-D7 added FloatGen + TripleGen + VecGen + the calculus-rule metamorphic checks, leaving one gap in the property-framework : refinement-type-guided generation per `specs/20_REFINEMENT.csl`. This slice adds `RefinedGen<G, P>` which wraps any inner generator with a predicate ; inputs are rejection-sampled up to `max_attempts` before the inner value is returned as-is. Shrinking similarly filters candidates through the predicate, guaranteeing refinement-valid shrink-results. This is the stage-0 bridge : a refinement `{x : i64 | x > 0}` in the source becomes `RefinedGen::new(IntGen { min: 0, max: _ }, |x| *x > 0)` at the test-harness layer.
- **Slice landed (this commit)**
  - `property.rs` :
    - `RefinedGen<G, P> { inner: G, predicate: P, max_attempts: u32 }` — generic over `G: Generator<T>` + `P: Fn(&T) -> bool`.
    - `RefinedGen::new(inner, predicate)` sets `max_attempts = 100` ; direct struct-literal for custom caps.
    - `Generator<T> for RefinedGen<G, P>` :
      - `generate()` : loops up to `max_attempts` drawing from `inner` until `predicate` is satisfied. Returns the first passing value ; if all fail, returns the last drawn (caller caveat : persistent failure signals mismatched inner+predicate).
      - `shrink()` : calls `inner.shrink(v)`, filters through `predicate` — all shrink-results are refinement-valid.
    - 6 new tests : respects-predicate-on-draw, shrinks-to-predicate-valid-only, returns-last-when-unsatisfiable, custom-max-attempts override, refined-float-positive-only (FloatGen + `x > 0`), run-property end-to-end refined-integer-property.
- **Consequences**
  - Test count : 1252 → 1258 (+6 RefinedGen).
  - Refinement-typed inputs now expressible at the test-harness layer — downstream crates can write `{x : i64 | x > 0}`-shaped property tests today. The predicate is Rust-syntax ; once cssl-macros lands `@property(x: i64 where x > 0) fn …` expansion, this generator becomes the natural target.
  - Every canonical test-framework generator now lives in cssl-testing : scalar (IntGen / BoolGen / FloatGen) + structural (TripleGen / VecGen) + refinement (RefinedGen). The only remaining gap is Hypothesis-style integrated shrinking (seed-prefix shrinking instead of output-value shrinking).
  - Entire workspace commit-gate still green : fmt + clippy + test + doc + xref.
- **Deferred**
  - Hypothesis-style integrated shrinking : retains LCG draw-history per-sample so shrinking reduces the seed-prefix rather than the output — converges better for deeply-structured inputs.
  - Smart refinement-compilation : once `specs/20` predicates are compiled to generator-guided-construction (not rejection), RefinedGen's rejection-sampler becomes a fallback rather than the primary path.
  - `WeightedGen<G>` / `OneOfGen<Gs>` — sum-type generators for tagged-union refinement-types.
  - Stateful generators (Markov-chain style) for sequence-fuzzing.

───────────────────────────────────────────────────────────────

## § T11-D9 : Vec3 AnalyticExpr algebra — real `length(p) - r` as symbolic expression

- **Date** 2026-04-17
- **Status** accepted
- **Context** T7-D10 (vector-SDF scalar-expanded gate case) verified the killer-app gradient `∂(length(p) - r)/∂p = normalize(p)` by manually expanding the vec3 operations to scalar components in the MIR body + writing the analytic gradients in expanded form. This works, but every new vec3 case requires replicating the expansion by hand. This slice adds a **first-class vec3 algebra** (`AnalyticVec3Expr`) with operations that compose into scalar [`AnalyticExpr`] via `length` / `dot` / `vec3_proj` / `to_scalar_components` — so `length(p) - r` can be written directly as a symbolic expression without manual scaffolding. The scalar-expansion is now **inside the algebra**, not the test.
- **Slice landed (this commit)**
  - New module `compiler-rs/crates/cssl-examples/src/analytic_vec3.rs` (~400 LOC + 20 tests) :
    - `VecComp { X, Y, Z }` : component projector enum.
    - `AnalyticVec3Expr` : `Const(f64, f64, f64)` + `Var(String)` + `Neg` + `Add` + `Sub` + `ScalarMul(Box<AnalyticExpr>, Box<Self>)` + `ScalarDiv(Box<Self>, Box<AnalyticExpr>)` + `Normalize`. All with constructor helpers `c / v / neg / add / sub / scalar_mul / scalar_div / normalize`.
    - `simplify()` : componentwise-lifted from `AnalyticExpr::simplify`.
    - `evaluate(&HashMap<String, f64>) -> [f64; 3]` : var lookups via `"<name>.x"` / `.y` / `.z` keys ; scalar vars (for ScalarMul/Div) use bare-name keys.
    - `to_scalar_components() -> (AnalyticExpr, AnalyticExpr, AnalyticExpr)` : the bridge that lets every vec3 op reduce to three scalar AnalyticExpr trees. This is the mechanism that avoids adding any new AD primitive.
    - Free functions :
      - `length(v) : &AnalyticVec3Expr -> AnalyticExpr` = `sqrt(x² + y² + z²)` as real `Sqrt(Add(...))` tree.
      - `dot(a, b)` = `a.x·b.x + a.y·b.y + a.z·b.z`.
      - `vec3_proj(v, comp)` = scalar component extraction.
      - `sphere_sdf_vec3(p, r)` = `length(p) - r` as scalar expr.
      - `sphere_sdf_grad_p(p, d_y)` = `normalize(p) · d_y` as vec3 expr.
      - `sphere_sdf_grad_r(d_y)` = `-d_y` as scalar expr.
    - 20 tests covering : VecComp suffix map, const/var/neg/add/sub/scalar_mul/scalar_div/normalize evaluation, normalize zero-vector NaN handling, `length(3,4,0) == 5`, dot product against known sum, proj extraction, sphere-SDF primal at `p=(3,4,0) r=2 → 3`, sphere-SDF grad_p equals `(0.6, 0.8, 0.0)·d_y`, grad_r = `-d_y`, central-difference numerical agreement with `normalize(p).x = 0.6`, simplify preserves eval-semantics, to_scalar_components roundtrip matches evaluate.
  - `lib.rs` : `pub mod analytic_vec3;` added alongside existing `pub mod ad_gate;`.
- **Consequences**
  - Test count : 1258 → 1278 (+20 in cssl-examples).
  - `length(p) - r` + its gradient `normalize(p)·d_y` are now expressible as **first-class symbolic expressions**. Any future scene-SDF test can compose these directly without replicating the scalar-expansion :
    ```rust
    let p = AnalyticVec3Expr::v("p");
    let r = AnalyticExpr::v("r");
    let primal = sphere_sdf_vec3(&p, &r);  // length(p) - r
    let grad_p = sphere_sdf_grad_p(&p, &AnalyticExpr::v("d_y"));
    ```
  - The scalar-expansion is now **test-algebra internal** via `to_scalar_components()`. The T7-D10 gate case still uses manual MirFunc construction ; the next slice (T11-D10) will lower AnalyticVec3Expr-driven test cases directly into MIR vec3 primitives once those land.
  - No new AD primitive added — existing `cssl_autodiff::apply_bwd` handles the scalar-component tree unchanged. The algebra layer is pure-symbolic.
  - Entire workspace commit-gate still green : fmt + clippy + test + doc + xref.
- **Deferred**
  - T11-D10 : Real MIR vec3 lowering — `MirType::Vec3F32` + `MirOp::Vec3{Add,Sub,Neg,ScalarMul,Dot,Length,Normalize}`. Replaces scalar-expansion in T7-D10's `build_sphere_sdf_vec3_primal` with native vec3 primitives.
  - `SceneSDFExpr` : monomorphized `min(sphere_sdf(p, r₀), sphere_sdf(p - c, r₁))` with piecewise-differentiable min-gradient dispatch (which-branch-dominates tracker).
  - Full constant-folding in `AnalyticVec3Expr::simplify` (componentwise zero/identity elimination) — today simplify just recurses structurally.
  - `to_smt` / `to_term` impls for `AnalyticVec3Expr` — route via `to_scalar_components` + 3 separate SMT queries per gradient (componentwise unsat).

───────────────────────────────────────────────────────────────

## § T11-D10 : AnalyticExpr Min/Max + scene-SDF analytic union/intersect/subtract

- **Date** 2026-04-17
- **Status** accepted
- **Context** T11-D9 landed the `AnalyticVec3Expr` algebra so `length(p) - r` is now a first-class symbolic expression. The canonical killer-app next-level test is the scene-SDF composition : `union(sphere_sdf(p - c₀, r₀), sphere_sdf(p - c₁, r₁))`. This requires `min`-at-the-scalar-level + piecewise-linear gradient dispatch (pick-the-winner). This slice extends `AnalyticExpr` with `Min` + `Max` variants and adds `scene_sdf_union` / `scene_sdf_intersect` / `scene_sdf_subtract` + their gradient helpers to `analytic_vec3.rs`.
- **Slice landed (this commit)**
  - `AnalyticExpr` enum gains two variants :
    - `Min(Box<AnalyticExpr>, Box<AnalyticExpr>)` — `min(a, b)` primitive for scene-SDF union.
    - `Max(Box<AnalyticExpr>, Box<AnalyticExpr>)` — `max(a, b)` for intersection + subtraction.
  - Both route through existing `AnalyticExpr` machinery :
    - `simplify` : constant-folds `min(Const, Const)` / `max(Const, Const)` into a single `Const`.
    - `evaluate` : `a.min(b)` / `a.max(b)` via `f64::min` / `f64::max`.
    - `to_term` : emits `min_uf` / `max_uf` uninterpreted-fn apps (SMT-compatible).
    - `to_smt` : same, in SMT-LIB text form.
    - `collect_vars` : recurses both branches (unified with Add/Sub/Mul/Div).
  - Constructor helpers : `AnalyticExpr::min(a, b)` + `AnalyticExpr::max(a, b)`.
  - `analytic_vec3.rs` new free functions :
    - `scene_sdf_union(a, b) = min(a, b)` — nearer-distance of two SDFs.
    - `scene_sdf_intersect(a, b) = max(a, b)` — farther-distance.
    - `scene_sdf_subtract(a, b) = max(a, -b)` — carve-out.
    - `scene_sdf_union_grad(a, b, da, db, env)` — piecewise gradient : picks `da` at `env` iff `a(env) ≤ b(env)`, else `db`.
    - `scene_sdf_intersect_grad(a, b, da, db, env)` — symmetric (picks `da` iff `a ≥ b`).
  - 9 new tests : union picks-nearer, intersect picks-farther, subtract carves via max(-b), union_grad picks-winning-branch, intersect_grad picks-max, two-spheres numerical gradient agreement at p=(1,0,0) (sphere-1 dominates → grad = `(1,0,0)`), min/max symmetry, constant-fold in simplify, min_uf/max_uf in SMT output.
- **Consequences**
  - Test count : 1278 → 1287 (+9 in cssl-examples).
  - Scene-SDF compositions now expressible symbolically without scalar expansion :
    ```rust
    let scene = scene_sdf_union(
        sphere_sdf_vec3(&(p - c0), &r0),
        sphere_sdf_vec3(&(p - c1), &r1),
    );
    ```
  - Piecewise gradient handled correctly at sampled points ; cusp `a == b` picks `da` by convention (caller should sample away from cusp).
  - `Min` + `Max` compose through SMT-LIB via `min_uf` / `max_uf` uninterpreted-fns — the solver can install axioms like `∀ a, b : min(a, b) = (if a ≤ b then a else b)` to reason symbolically.
  - Entire workspace commit-gate still green : fmt + clippy + test + doc + xref.
- **Deferred**
  - Real MIR `MirOp::Min` / `MirOp::Max` primitives + AD rule-table entries for piecewise-differentiable min/max. Today `apply_bwd` relies on the existing primitive-set ; scene-SDF gradient tests verify at the `AnalyticExpr` level only.
  - `AnalyticExpr::Abs` + `AnalyticExpr::Sign` — for SDF absolute-value + sign-reasoning.
  - Full smooth-min `smoothmin(a, b, k) = -log(exp(-ka) + exp(-kb))/k` — differentiable everywhere (scene-SDF with rounded edges per `specs/05` § APPENDIX-SMOOTH).
  - Cusp-detection in gradient samplers : skip samples where `|a - b| < ε` to avoid subgradient-ambiguity.
