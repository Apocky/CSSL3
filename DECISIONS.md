# CSSLv3 — DECISIONS log

§ STATUS : Session-1 • T1..T6-phase-1 ✓ • T7-phase-1 ✓ • T8-phase-1 ✓ • T3.4-phase-2-refinement ✓ • T9-phase-1 ✓ • T10-phase-1-codegen ✓ • T10-phase-1-hosts ✓ • T11-phase-1-telemetry-persist ✓ • T12-phase-1-examples ✓ • T3.4-phase-3-AD-legality ✓ • spec-corpus deltas applied • foundation audited

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
  - (b) Cargo workspace with per-concern crates per §§ HANDOFF T1
- **Decision** **(b) Cargo workspace**
- **Rationale**
  - `deny(unsafe_code)` per-crate enforcement is impossible in single-crate layout; FFI isolation (mlir-sys, level-zero-sys, ash, windows-rs, metal) needs per-crate boundary.
  - Parallel build + incremental + per-crate test isolation at scale.
  - Stage-1 rip-and-replace migration is per-crate clean.
  - Per-crate versioning once APIs mature.
- **Consequences**
  - §§ 01_BOOTSTRAP REPO-LAYOUT will be reconciled to match workspace (spec-corpus delta pending Apocky approval per §§ HANDOFF REPORTING).
  - Workspace root at `compiler-rs/` with `members = ["crates/*"]`.
  - Package-name prefix `cssl-*`; dir-name == package-name.
  - Binary crate `csslc` (no prefix); runtime lib `cssl-rt`.

───────────────────────────────────────────────────────────────

## § T1-D2 : cslparser sourcing — Rust-native port (option e)

- **Date** 2026-04-16
- **Status** accepted
- **Context** §§ HANDOFF T2 originally proposed `{a: vendor-source, b: cargo-patch-git, c: wait-for-crate}`; all presumed Rust compatibility. CSLv3 Session-3 confirms `cslparser = Odin package` (parser/\*.odin + parser.exe via `odin build`). New option-space surfaced during γ-load: `{d: Odin→C-ABI+bindgen, e: Rust port from spec, f: subprocess-IPC, g: AST.json sidecar, h: dual FFI+port, i: port + CI-oracle}`.
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
- **Context** §§ HANDOFF specifies MSRV 1.75. R16 reproducibility-anchor mandates version-pinning. Current Apocky machine has rustc 1.94 (compatible).
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
- **Context** §§ HANDOFF `deny(unsafe_code) except FFI-crates`. Workspace-level `[workspace.lints.rust] unsafe_code = "deny"` cannot be partially-overridden per-crate without duplicating the entire lint-table in FFI crates.
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
- **Context** `clippy::pedantic` + `clippy::nursery` groups enabled at `warn`; `cargo clippy -- -D warnings` promotes warnings to errors per §§ HANDOFF WORKFLOW commit-gate. Several pedantic lints fire pervasively on scaffold docstrings (`doc_markdown` wants backticks around `CSSLv3`, `SPIR-V`, `MLIR`, `DXIL`, etc) and on future typical-cast patterns.
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
- **Context** T4 scope (per §§ HANDOFF + §§ 04_EFFECTS) enumerates : 28 built-in effect registration, row-unification engine, sub-effect discipline checker, Xie+Leijen evidence-passing transform, linear×handler one-shot enforcement. Landing the full Xie+Leijen transform (HIR → HIR+evidence) in one commit is a multi-week project — phasing lets T5 (caps), T6 (MLIR), T7 (AD), T8 (staging) build on the registry + discipline without blocking on the transform.
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
- **Context** T3.4 scope (per §§ HANDOFF) enumerates : bidirectional type inference + effect-row unification + cap inference + IFC-label propagation + refinement-obligation generation + AD-legality + `@staged` check + macro hygiene. Landing all of these in one commit is ~10K LOC ; phasing makes the inference surface reviewable without blocking T4 effects integration.
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
