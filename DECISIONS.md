# CSSLv3 — DECISIONS log

§ STATUS : Session-1 T1 in-progress

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
