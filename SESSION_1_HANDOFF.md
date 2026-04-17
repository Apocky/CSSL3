# SESSION 1 HANDOFF — CSSLv3 stage0 scaffold

§ META
- **Session date** 2026-04-16
- **Coding agent** Claude.Opus.4.7-1M
- **Prior handoff** `HANDOFF_SESSION_1.csl` (authoritative scope)
- **Current task** T1 ✓ + T2 ✓ complete; T3 next (parse + ast + hir elaborator)

───────────────────────────────────────────────────────────────

§ PROGRESS BY DELIVERABLE  (per §§ HANDOFF_SESSION_1 DELIVERABLES)

| ID  | Task                                                  | Status         |
|-----|-------------------------------------------------------|----------------|
| D1  | compiler-rs/ Cargo-workspace skeleton                 | ✓ complete     |
| D2  | lex crate — dual-surface lexer                        | ✓ complete (T2) |
| D3  | parse + ast + hir — elaborator                        | ◐ ast primitives landed (Span/SourceFile/Diagnostic); parse + hir pending T3 |
| D4  | effects — 28 effects + evidence-passing               | ○ pending T4   |
| D5  | caps — Pony-6 + gen-refs                              | ○ pending T5   |
| D6  | mlir-bridge + mir — cssl-dialect                      | ○ pending T6   |
| D7  | autodiff + jets                                       | ○ pending T7   |
| D8  | staging + macros + futamura                           | ○ pending T8   |
| D9  | smt — Z3 / CVC5 / KLEE                                | ○ pending T9   |
| D10 | cgen-cpu + cgen-gpu + host-*                          | ○ pending T10  |
| D11 | telemetry + testing + persist                         | ◐ cssl-testing oracle-dispatch stubs wired @ T1; full @ T11 |
| D12 | examples/ hello-triangle + sdf-shader + audio-cb      | ○ pending T10+ |
| D13 | DECISIONS.md + SESSION_1_HANDOFF.md                   | ✓ T1-D1..D7 recorded |
| D14 | .github/workflows/ci.yml                              | ✓ §§ 23-faithful skeleton wired |

───────────────────────────────────────────────────────────────

§ DECISION LOG
See [DECISIONS.md](DECISIONS.md). Recorded so far :
- **T1-D1** : workspace layout (not single-crate)
- **T1-D2** : cslparser → Rust-native port from CSLv3/specs/12 + 13 (option e)
- **T1-D3** : CI scope — §§ 23-faithful from commit-1 (optimal ≠ minimal)
- **T1-D4** : rust 1.75.0 MSRV pinned via `rust-toolchain.toml`
- **T1-D5** : `#![forbid(unsafe_code)]` per-crate inner-attr policy; FFI-crates `#![allow]`
- **T1-D6** : clippy pedantic scaffold-allowances (tighten post-T3 API-stabilization)
- **T1-D7** : rust toolchain ABI gnu-vs-msvc deferred to T10 FFI-entry
- **T2-D1** : Unified `TokenKind` enum with sub-enums (not nested per-surface hierarchy)
- **T2-D2** : Rust-hybrid via `logos` with private `RawToken → TokenKind` promotion layer
- **T2-D3** : CSLv3-native via hand-rolled byte-stream lexer with indent-stack
- **T2-D4** : Surface auto-detection cascade — extension > pragma > first-line > default
- **T2-D5** : `Apostrophe` token for non-morpheme `'…` attachments (`f32'pos`, `SDF'L`, lifetimes)

───────────────────────────────────────────────────────────────

§ OPEN QUESTIONS

- **Q3 (T6 MLIR on Windows)** deferred. `cssl-mlir-bridge` crate stub with workspace-dep commented. Build-chain verification at T6-start. Fallback pre-authorized : MLIR textual-format via CLI (T6 option-b).
- **melior / mlir-sys Windows compatibility** : unverified. Typically requires `LLVM_SYS_*_PREFIX` + local LLVM build. Tested at T6 entry.
- **level-zero-sys registry availability** : unverified. `cssl-host-level-zero` crate stub with dep commented until T10.
- **§§ 01_BOOTSTRAP REPO-LAYOUT spec-delta** : workspace layout supersedes single-crate. Spec-corpus update pending Apocky approval per §§ HANDOFF REPORTING ("W! coordinate-with-Apocky before-editing-specs").
- **MSVC vs GNU ABI** : rustup installed `1.75.0-x86_64-pc-windows-gnu`; MSVC may be preferred at T10 FFI link-time. T1-D7 defers verification.

───────────────────────────────────────────────────────────────

§ T1 ARTIFACTS

`compiler-rs/` (31 crates) :
- **frontend** (6) : cssl-lex, cssl-parse, cssl-ast, cssl-hir, cssl-mir, cssl-lir
- **analysis** (9) : cssl-caps, cssl-effects, cssl-ifc, cssl-autodiff, cssl-jets, cssl-staging, cssl-macros, cssl-smt, cssl-futamura
- **codegen-cpu** (1) : cssl-cgen-cpu-cranelift
- **codegen-gpu** (4) : cssl-cgen-gpu-spirv, cssl-cgen-gpu-dxil, cssl-cgen-gpu-msl, cssl-cgen-gpu-wgsl
- **hosts** (5) : cssl-host-vulkan, cssl-host-level-zero, cssl-host-d3d12, cssl-host-metal, cssl-host-webgpu
- **infra** (3) : cssl-telemetry, cssl-testing, cssl-persist
- **bridges** (1) : cssl-mlir-bridge
- **entry** (2) : csslc (binary), cssl-rt (runtime)

`compiler-rs/crates/cssl-testing/src/` (§§ 23-faithful oracle-dispatch, all wired-empty-but-present) :
- `oracle.rs` : `OracleMode` enum + 12 variants, registry + display + attribute-form
- `property.rs` `differential.rs` `metamorphic.rs` `bench.rs` `power.rs` `thermal.rs` `replay.rs` `hot_reload.rs` `fuzz.rs` `golden.rs` `audit.rs` : `Config` + `Outcome` + `Dispatcher` trait + `Stage0Stub` each
- `metrics.rs` : `FrequencySample`, `LatencyPercentiles`, `MetricsSnapshot` data-structs
- `r16_attestation.rs` : `Attestation` + `Attester` trait + `Stage0Stub` (T30 OG10 hook)

Other artifacts :
- `compiler-rs/Cargo.toml` workspace manifest w/ deps declared + lints config
- `compiler-rs/rust-toolchain.toml` channel = 1.75.0, profile minimal
- `compiler-rs/rustfmt.toml` stable-options only
- `compiler-rs/.perf-baseline/.gitkeep` (T11 baselines-dir marker)
- `compiler-rs/tests/golden/.gitkeep` (T10+ golden-fixture-dir marker)
- `.github/workflows/ci.yml` §§ 23-faithful : fast + spec-xref + PR oracle-dispatch + GPU-matrix (self-hosted if:false until provisioned) + nightly + R16-attestation + Futamura-P3 + aggregate
- `scripts/validate_spec_crossrefs.py` shape-aware file-ref validator (skips local-section lowercase-hyphened refs)
- `.editorconfig` `.gitignore` repo-root

───────────────────────────────────────────────────────────────

§ COMMIT-GATE STATUS (T1 final)

| Check                                  | Result          |
|----------------------------------------|-----------------|
| `cargo check --workspace --all-targets`| ✓ 0.19s         |
| `cargo fmt --check`                    | ✓ clean         |
| `cargo clippy -- -D warnings`          | ✓ 0.06s         |
| `cargo test --workspace`               | ✓ 48 passed / 0 failed / 0 ignored (61 suites green) |
| `cargo doc --workspace --no-deps`      | ✓ 31 crates documented |
| `scripts/validate_spec_crossrefs.py`   | ✓ 0 unresolved file-shaped refs (128 local-sections skipped) |

───────────────────────────────────────────────────────────────

§ METRICS

| Metric                        | T1-start | T1-end    | T2-end                          |
|-------------------------------|----------|-----------|---------------------------------|
| Crates in workspace           | 0        | 31        | 31 (cssl-ast + cssl-lex populated) |
| Lines of scaffold Rust        | 0        | ~1500     | ~3800 (+ cssl-ast primitives + cssl-lex token/rust_hybrid/csl_native/mode) |
| Test count                    | 0        | 48 / 61   | 150 / 62 suites                 |
| Clippy warnings (`-D`)        | N/A      | 0         | 0                               |
| CI jobs declared              | 0        | 19        | 19                              |
| Spec cross-refs validated     | manual   | 156 / 0   | 156 / 0 (unchanged; spec corpus stable) |
| Commit-gate green             | N/A      | ✓ 6 / 6   | ✓ 6 / 6                         |

───────────────────────────────────────────────────────────────

§ T2 ARTIFACTS

`crates/cssl-ast/src/` :
- `source.rs` : `SourceId`, `SourceFile` (with O(log n) line-offset index), `Surface`, `SourceLocation` (`NonZeroU32` line+col)
- `span.rs` : byte-offset `Span` with `DUMMY` sentinel, `join`, `contains_offset`, same-source guard
- `diagnostic.rs` : `Severity` (Error > Warning > Note > Help), `Diagnostic` (builder chain : `error`/`warning`/`with_span`/`with_note`/`with_help`/`with_labeled_note`), `Note`, `DiagnosticBag` (error-count tracking)

`crates/cssl-lex/src/` :
- `token.rs` : unified `Token { kind, span }`, `TokenKind` with sub-enums :
  `Keyword` (41 variants), `BracketKind`×`BracketSide`, `EvidenceMark` (8), `ModalOp` (10 incl. TODO/FIXME), `CompoundOp` (5 : TP/DV/KD/BV/AV), `Determinative` (6 pairs), `TypeSuffix` (9), `StringFlavor`
- `rust_hybrid.rs` : `logos`-derived `RawToken → TokenKind` promotion; ASCII + Unicode arrow/comparison aliases; CoT line + CoT block regex; 16 unit tests + integration fixture
- `csl_native.rs` : hand-rolled byte-stream lexer with indent-stack + bracket-depth suppression; full 74-glyph + ASCII-alias coverage per §§ 12_TOKENIZER; 29 unit tests
- `mode.rs` : 4-tier surface detection (extension > pragma > first-line > default); 17 detection tests
- `lib.rs` : top-level `lex(source)` surface-dispatcher; 5 dispatch tests

`crates/cssl-lex/tests/` :
- `fixtures/rust_hybrid_basic.cssl-rust` : realistic Rust-hybrid fragment (module / fn / `@attr` / effect rows / struct / enum)
- `fixtures/csl_native_basic.cssl-csl` : realistic CSLv3-native fragment (§, evidence, modals, dense-math, refinements, indent)
- `integration.rs` : 7 tests exercising dispatch + fixture coverage + differential-oracle preflight

`scripts/differential_lex_vs_odin.py` : CI driver skeleton for Rust-port vs `parser.exe` differential oracle (full impl deferred to T10 when `csslc tokens --json` lands).

───────────────────────────────────────────────────────────────

§ SPEC-CORPUS DELTAS  (for Apocky review before-editing-specs)

Queued for review :
- `specs/01_BOOTSTRAP.csl` § REPO-LAYOUT : single-crate → workspace (matches T1-D1).
- `specs/23_TESTING.csl` : add `OracleMode` registry as canonical enum (matches T1 scaffold).
- `specs/09_SYNTAX.csl` § lexical : document `Apostrophe` as distinct from char-literal opener; acknowledge `T'tag` / `42'i32` / `SDF'L` / lifetime patterns (matches T2-D5).

───────────────────────────────────────────────────────────────

§ NEXT — T3 (parse + ast + hir elaborator)

Per §§ HANDOFF T3 :
1. `cssl-parse` : Rust-hybrid parser via `chumsky` + CSLv3-native recursive-descent (both token streams → same CST).
2. `cssl-ast` : CST node taxonomy (source-preserving) + elaboration markers; `Symbol` interning table.
3. `cssl-hir` : bidirectional type-inference (Hindley-Milner + row poly for effects) + IFC-label propagation + cap-inference + refinement-obligation generation (routes to `cssl-smt`).
4. Integration with existing `cssl-lex::lex` (input) and `cssl-ast::Diagnostic` bag.
5. Golden HIR-dump fixtures under `compiler-rs/tests/golden/hir/`.

Open for T3-start :
- Parser library : `chumsky` vs `lalrpop` vs hand-rolled recursive-descent ? (T3-D1 pending)
- Symbol / interner crate : `string-interner`, `lasso`, or hand-rolled ? (T3-D2 pending)
- Morpheme-stacking representation : token-level collapsed vs AST-level tree ? (T3-D3 pending)
- `cslparser` binding-unit : FFI? CLI-subprocess? (per T1-D2, Rust-port is primary; but `parser.exe` as CI-differential-oracle TBD — see T1-D2 consequences).
- `logos` vs `chumsky` for lexer layer (T2-D1 pending).

───────────────────────────────────────────────────────────────

§ ACKNOWLEDGMENTS

- Apocky : direction-setting on optimal-vs-minimal CI scope (T1-D3), cslparser sourcing (T1-D2), spec authority discipline.
- Claude Opus 4.7 (1M context) : implementation + commit-gate discipline.
- Prior session (CSLv3 Session-3) : canonical cslparser + T10 CSSLv3-BRIDGE spec.
