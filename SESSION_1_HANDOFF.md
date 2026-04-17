# SESSION 1 HANDOFF — CSSLv3 stage0 scaffold

§ META
- **Session date** 2026-04-16 → 2026-04-17
- **Coding agent** Claude.Opus.4.7-1M
- **Prior handoff** `HANDOFF_SESSION_1.csl` (authoritative scope)
- **Current task** T1..T6-phase-1 ✓ + T7-phase-1 + T8-phase-1 ✓ + T3.4-phase-2-refinement ✓ + T9-phase-1 ✓ + T10-phase-1-codegen ✓ + T10-phase-1-hosts ✓ + T11-phase-1-telemetry-persist ✓ + T12-phase-1-examples ✓ + T3.4-phase-3-AD-legality ✓ + T6-phase-2a-pipeline-body-lowering ✓ + T7-phase-2a-AD-walker ✓ + **T9-phase-2a-predicate-translator ✓** — **F1-correctness END-TO-END chain now structurally complete** : source → lex → parse → HIR → AD-legality → refinement-obligations → MIR → AD-walker → SMT-translation → Z3/CVC5-dispatch → verdict. Remaining : T7-phase-2b real dual-substitution + T9-phase-2b Lipschitz + T12-phase-2c killer-app integration-test.

───────────────────────────────────────────────────────────────

§ PROGRESS BY DELIVERABLE  (per §§ HANDOFF_SESSION_1 DELIVERABLES)

| ID  | Task                                                  | Status         |
|-----|-------------------------------------------------------|----------------|
| D1  | compiler-rs/ Cargo-workspace skeleton                 | ✓ complete     |
| D2  | lex crate — dual-surface lexer                        | ✓ complete (T2) |
| D3  | parse + ast + hir — elaborator                        | ◐ ast + CST + parsers + HIR-lowering + name-resolution + HM inference + effect-rows + cap + refinement-obligation-gen ✓ ; IFC/AD-legality/@staged/hygiene pending T3.4-phase-2 (rest) |
| D4  | effects — 28 effects + evidence-passing               | ◐ registry + discipline + banned-composition ✓ (T4-phase-1) ; Xie+Leijen transform pending T4-phase-2 |
| D5  | caps — Pony-6 + gen-refs                              | ◐ cssl-caps (CapKind + AliasMatrix + subtype + LinearTracker + GenRef) + cssl-hir cap_check sig-level pass ✓ ; body-walk pending T5-phase-2 |
| D6  | mlir-bridge + mir — cssl-dialect                      | ○ pending T6   |
| D7  | autodiff + jets                                       | ◐ cssl-autodiff (rules / decls / variant-naming) + cssl-jets (JetOrder / JetOp / sig-validators) ✓ (T7-phase-1) ; rule-application + tape + GPU-loc pending T7-phase-2 |
| D8  | staging + macros + futamura                           | ◐ cssl-staging + cssl-macros + cssl-futamura (data-model + registries + hygiene-primitives) ✓ (T8-phase-1) ; expansion + comptime-eval + P3-bootstrap pending T8-phase-2 |
| D9  | smt — Z3 / CVC5 / KLEE                                | ◐ cssl-smt (Theory/Sort/Term/Literal + Query/FnDecl/Assertion + emit_smtlib + Z3/CVC5-CLI subprocess solvers + discharge stub) ✓ (T9-phase-1) ; FFI + KLEE + proof-certs pending T9-phase-2 |
| D10 | cgen-cpu + cgen-gpu + host-*                          | ◐ 5 codegen backends ✓ (cranelift + SPIR-V + DXIL/HLSL + MSL + WGSL text-emit + dxc/spirv-cross CLI adapters) + 5 host-adapters ✓ (vulkan + level-zero + d3d12 + metal + webgpu capability catalogs + stub probes + Arc A770 canonical profile) ; FFI-integration (ash/level-zero-sys/windows-rs/metal/wgpu) pending T10-phase-2 |
| D11 | telemetry + testing + persist                         | ◐ cssl-telemetry (R18 ring + audit-chain + exporters) ✓ + cssl-persist (image + schema-migration + in-memory backend) ✓ (T11-phase-1) ; cssl-testing oracle-dispatch bodies + BLAKE3/Ed25519 FFI + WAL/LMDB backends pending T11-phase-2 |
| D12 | examples/ hello-triangle + sdf-shader + audio-cb      | ◐ 3 canonical .cssl source files @ `examples/` + cssl-examples integration-tests crate ✓ (T12-phase-1) ; full-type-check + codegen-emission + runtime-verification pending T12-phase-2 (unlocked by T3.4-phase-3 + T6-phase-2 + T7-phase-2 + T9-phase-2 + T10-phase-2) |
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
- **T2-D6** : Apostrophe decomposition deferred ; parser refinement-tag path kept dormant until lexer catches up (SUPERSEDED by T2-D8)
- **T2-D8** : Apostrophe decomposition landed — logos regex `'`-exclusion + post-pass morpheme-fold
- **T3-D1** : Parser — hand-rolled recursive-descent + Pratt (no combinator-lib dep) for both surfaces
- **T3-D2** : String interning deferred to HIR layer (lasso at T3-mid)
- **T3-D3** : Morpheme-stacking + compound-formation at CST layer as `Expr::Compound { op, lhs, rhs }`
- **T3-D4** : CST single-file, HIR modular-split
- **T3-D5** : Path-parser splits by context — colon-only in expr/pat, dot-accepting in types/module-decls
- **T3-D6** : Struct-constructor disambiguation via peek-ahead (`looks_like_struct_body`)
- **T3-D7** : Parser error-recovery protocol — rules always return a node + push diagnostics ; top-level loop breaks only on no-progress
- **T3-D8** : Stage-0 interner uses single-threaded `lasso::Rodeo` (deferred `ThreadedRodeo` until MSVC toolchain switch @ T10)
- **T3-D9** : T3.4 phased — phase-1 HM type-inference + effect-row unification ; phase-2 cap/IFC/refinement/AD/@staged/hygiene deferred
- **T4-D1** : T4 phased — phase-1 registry + sub-effect discipline + banned-composition (Prime-Directive F5) ; phase-2 Xie+Leijen transform + linear-handler enforcement deferred
- **T5-D1** : cap_check delegated to cssl-caps via `AliasMatrix::can_pass_through = is_subtype` (subtype is canonical)
- **T5-D2** : GenRef layout = low u40 index + high u24 generation ; `bump_gen()` wraps at 2^24
- **T5-D3** : Cap-check stage-0 is signature-level only ; body-walk deferred to T5-phase-2
- **T6-D1** : MLIR-text-CLI fallback landed as phase-1 (option-b pre-authorized) ; melior FFI deferred to T6-phase-2 pending MSVC toolchain switch @ T10
- **T6-D2** : CsslOp enum with 26 dialect variants + Std catch-all + per-op metadata (category, signature)
- **T7-D1** : AD phased — rules table + decl collection + variant-naming (phase-1) ; rule-application deferred to T7-phase-2
- **T7-D2** : Jet<T,N> = structural data-type ; order-dependent ops validated at T6 MIR ; runtime representation deferred
- **T8-D1** : Staging + Macros + Futamura = three parallel crates (staging/macros/futamura) ; phase-1 data model + registries, expansion deferred to T8-phase-2
- **T3-D10** : T3.4-phase-2-refinement landed — `cssl-hir::refinement` obligation-generator walks HIR types + exprs for `{v : T | P}` / `T'tag` / `SDF'L<k>` sites ; produces `ObligationBag` ready for T9 discharge
- **T9-D1** : SMT phased — phase-1 SMT-LIB 2.6 text-emit + Z3/CVC5-CLI subprocess solver adapters + stub obligation discharge ; FFI / KLEE / proof-certs / HIR→SMT-term translation deferred to T9-phase-2
- **T10-D1** : Codegen phased — 5 backends (cranelift-CPU / SPIR-V / DXIL-HLSL / MSL / WGSL) text-emission now + `DxcCliInvoker` / `SpirvCrossInvoker` subprocess adapters ; real cranelift/rspirv/naga FFI + MIR-body lowering + spirv-val gate / dxc round-trip / fat-binary assembly deferred to T10-phase-2
- **T10-D2** : Host-adapters phased — 5 adapters (Vulkan + Level-Zero + D3D12 + Metal + WebGPU) capability-catalogs + stub probes + canonical Arc A770 profile now ; ash / level-zero-sys / windows-rs / metal / wgpu FFI deferred to T10-phase-2
- **T11-D1** : Telemetry + persistence phased — cssl-telemetry (25-scope taxonomy + TelemetryRing SPSC + AuditChain BLAKE3+Ed25519 stub + Chrome/JSON/OTLP exporters) + cssl-persist (SchemaVersion + MigrationChain + PersistenceImage + InMemoryBackend) now ; real BLAKE3/Ed25519 + OTLP gRPC + WAL/LMDB backends + @hot_reload_preserve HIR pass deferred to T11-phase-2 ; cssl-testing oracle-body fleshing also T11-phase-2
- **T12-D1** : Examples trilogy at repo-root — 3 canonical CSSLv3 source files (hello-triangle VK-1.4 pipeline + sdf-shader `bwd_diff(scene_sdf)` killer-app gate + audio-callback full-real-time-effect-row) + cssl-examples integration-tests crate pipelining lex → parse → HIR → lower ; bit-exact-vs-analytic verification gate + MIR-emission + spirv-val gated on T6+T7+T9-phase-2 slices
- **T3-D11** : T3.4-phase-3-AD-legality — `cssl_hir::ad_legality` compile-time check emitting AD0001 (gradient-drop) / AD0002 (unresolved-callee) / AD0003 (missing-return-tangent) diagnostics for every `@differentiable` fn body. Closes the AD-legality slice from T3-D9 deferred-list ; remaining T3.4-phase-3 slices (IFC-propagation + @staged-check + macro-hygiene + let-generalization) still deferred
- **T6-D3** : T6-phase-2a MIR pass-pipeline + HIR-body lowering — `cssl_mir::pipeline` (MirPass trait + PassPipeline + 6 stock passes : 5 stubs + StructuredCfgValidator real) + `cssl_mir::body_lower` (BodyLowerCtx + lower_fn_body covering Literal/Path/Binary(19 ops)/Unary/Call/Return/Block/If/Paren, unsupported variants emit opaque placeholder). Critical-path gate unlocked for T7-phase-2 (AD walker) + T9-phase-2 (SMT translation) + T11-phase-2 (telemetry-probe-insert). Phase-2b remaining : full-expr-coverage (field/index/loops/match/struct/array/assign) + real literal-value extraction + type-propagation + melior FFI
- **T7-D3** : T7-phase-2a MIR-walking AD rule-application — `cssl_autodiff::walker` (op_to_primitive 10-MIR-op-mapping + specialize_transcendental for sqrt/sin/cos/exp/log callee-detection + AdWalker auto-discovering @differentiable fns from HIR + transform_module emitting <name>_fwd/<name>_bwd variants with diff_recipe_{fwd,bwd} attrs per matched primitive) + AdWalkerPass MirPass adapter pluggable into PassPipeline. sphere_sdf integration-tested — killer-app-gate structurally computable. Phase-2b remaining : real dual-substitution into tangent-carrying MIR + tape-record for bwd ; phase-2c : bit-exact-vs-analytic verification (composes w/ T9-phase-2 SMT)
- **T9-D2** : T9-phase-2a predicate-text → SMT-Term translator — `cssl_smt::predicate` with recursive-descent parser covering `> < >= <= == != && || and or in ∈` + set-membership + conjunction/disjunction + parenthesization + negative literals + `translate_obligation` emitting (set-logic QF_LIA)(declare-fun v () Int)(assert (not P(v))) with named obl_<id>_* labels + `translate_bag` bulk translator + TranslationError (ParseFailure/UnsupportedKind). **F1-correctness END-TO-END chain now structurally complete** — source→verdict path fully wired. Phase-2b remaining : HIR-direct translation (bypass text) + Lipschitz arithmetic-interval + multi-binder + float-arith + uninterpreted-fn calls

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

| Metric                        | T4-phase-1-end | T5-end          | T6-phase-1-end                         | T7+T8-phase-1-end                                           | Commit-2 end (T3.4-phase-2-refinement + T9-phase-1)         | Commit-3 end (T10-phase-1-codegen)                         | Commit-4 end (T10-phase-1-hosts)                          | Commit-5 end (T11-phase-1-telemetry-persist)           | Commit-6 end (T12-phase-1-examples)                      |
|-------------------------------|----------------|-----------------|----------------------------------------|-------------------------------------------------------------|-------------------------------------------------------------|--------------------------------------------------------------|------------------------------------------------------------|---------------------------------------------------------|----------------------------------------------------------|
| Crates populated              | 5              | 6               | 8 (+ cssl-mir + cssl-mlir-bridge)      | 13 (+ autodiff + jets + staging + macros + futamura)        | 14 (+ cssl-smt)                                              | 19 (+ 5 cgen-* : cranelift / spirv / dxil / msl / wgsl)     | 24 (+ 5 host-* : vulkan / level-zero / d3d12 / metal / webgpu) | 26 (+ cssl-telemetry + cssl-persist)                  | 27 (+ cssl-examples integration-tests)                 |
| Lines of scaffold Rust        | ~15300         | ~17500          | ~19900 (+ ~2000 mir + ~200 bridge)     | ~24300 (+ ~1500 autodiff + ~400 jets + ~850 staging + ~900 macros + ~750 futamura)  | ~26500 (+ ~400 hir/refinement + ~1800 smt)     | ~32100 (+ ~1150 cranelift + ~1400 spirv + ~1050 dxil + ~1050 msl + ~950 wgsl) | ~36100 (+ ~1100 vulkan + ~750 level-zero + ~650 d3d12 + ~700 metal + ~800 webgpu) | ~38500 (+ ~1500 telemetry + ~900 persist)             | ~38750 (+ ~250 examples) + ~180 LOC CSSLv3 source      |
| Test count                    | 368 / 65       | 423 / 66        | 466 / 67 suites (+41 mir + 4 bridge)   | 528 passed / 33 targets (+61 : autodiff/jets/staging/macros/futamura)  | 569 passed / 33 targets (+41 : refinement + smt)            | 715 passed / 33 targets (+151 : cranelift+spirv+dxil+msl+wgsl) | 786 passed / 33 targets (+76 : vulkan+level-zero+d3d12+metal+webgpu) | 850 passed / 33 targets (+64 : telemetry + persist)    | 859 passed / 34 targets (+9 : cssl-examples)           |
| Clippy warnings (`-D`)        | 0              | 0               | 0                                      | 0                                                           | 0                                                            | 0                                                            | 0                                                          | 0                                                       | 0                                                        |
| CI jobs declared              | 19             | 19              | 19                                     | 19                                                          | 19                                                           | 19                                                           | 19                                                         | 19                                                      | 19                                                       |
| Spec cross-refs validated     | 156 / 0 (135)  | 156 / 0 (135)   | 156 / 0 (135)                          | 156 / 0 (135)                                               | 156 / 0 (135)                                                | 156 / 0 (135)                                                | 156 / 0 (135)                                              | 156 / 0 (135)                                           | 156 / 0 (136)                                            |
| Commit-gate green             | ✓ 6 / 6        | ✓ 6 / 6         | ✓ 6 / 6                                | ✓ 6 / 6                                                     | ✓ 6 / 6                                                      | ✓ 6 / 6                                                      | ✓ 6 / 6                                                    | ✓ 6 / 6                                                 | ✓ 6 / 6                                                  |

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
- `csl_native.rs` : hand-rolled byte-stream lexer with indent-stack + bracket-depth suppression; full 74-glyph + ASCII-alias coverage per `CSLv3/specs/12_TOKENIZER`; 29 unit tests
- `mode.rs` : 4-tier surface detection (extension > pragma > first-line > default); 17 detection tests
- `lib.rs` : top-level `lex(source)` surface-dispatcher; 5 dispatch tests

`crates/cssl-lex/tests/` :
- `fixtures/rust_hybrid_basic.cssl-rust` : realistic Rust-hybrid fragment (module / fn / `@attr` / effect rows / struct / enum)
- `fixtures/csl_native_basic.cssl-csl` : realistic CSLv3-native fragment (§, evidence, modals, dense-math, refinements, indent)
- `integration.rs` : 7 tests exercising dispatch + fixture coverage + differential-oracle preflight

`scripts/differential_lex_vs_odin.py` : CI driver skeleton for Rust-port vs `parser.exe` differential oracle (full impl deferred to T10 when `csslc tokens --json` lands).

───────────────────────────────────────────────────────────────

§ SPEC-CORPUS DELTAS  (applied 2026-04-16 post-T3.1)

Applied :
- ✓ `specs/01_BOOTSTRAP.csl` § REPO-LAYOUT : single-crate → Cargo workspace + 31 enumerated crates (T1-D1).
- ✓ `specs/23_TESTING.csl` § @test ATTRIBUTES : added `@audit_test` + canonical `OracleMode` registry cross-reference with 12-variant 1:1 attribute mapping + adjunct-module listing (T1 cssl-testing scaffold).
- ✓ `specs/09_SYNTAX.csl` § lexical : added `apostrophe` entry documenting `Apostrophe` token for non-morpheme `'…` attachments (`T'tag` / `SDF'L<k>` / lifetime-like) with surface-differing rules noted (T2-D5).

Queued for future tasks :
- `specs/02_IR.csl` § HIR contents : expand with CST → HIR elaboration-pass enumeration once T3-Turn-D lands.
- `specs/16_DUAL_SURFACE.csl` § MODE-DETECTION : inline the 4-tier cascade with `Reason` enum once T3 parser surfaces dispatch behaviour beyond the current tests.

───────────────────────────────────────────────────────────────

§ T3.2 ARTIFACTS  (added 2026-04-17)

`crates/cssl-parse/src/` :
- `lib.rs` : top-level `parse(source, tokens) -> (Module, DiagnosticBag)` surface-dispatcher
- `cursor.rs` : `TokenCursor<'a>` with 2-token lookahead + newline-aware mode toggle + trivia-skip
- `error.rs` : `ParseError` enum + `to_diagnostic()` + helper constructors (`expected_one`, `expected_any`, `custom`, `nyi`)
- `common.rs` : shared `parse_ident`, `parse_module_path` (dot+colon), `parse_colon_path` (colon-only), `expect`, `expect_any`
- `rust_hybrid/` 8 modules (attr, expr, generics, item, mod, pat, stmt, ty)
- `csl_native/` 4 modules (compound, mod, section, slot)
- `tests/integration.rs` : 13 end-to-end Lex+Parse tests covering realistic multi-item fragments

§ T3.2 SCOPE COVERAGE
- Rust-hybrid : fn / struct / enum / interface / impl / effect / handler / type / use / const / module
- Pratt expression parser with 14 precedence levels + all control-flow (if / match / for / while / loop / return / break / continue / region / perform / with / lambda / #run)
- Types : Path, Tuple, Array, Slice, Reference, Capability, Function, Refined (predicate + tag sugar), Infer
- Patterns : Wildcard, Literal, Binding, Tuple, Struct, Variant, Or, Range, Ref
- Effect-rows + generics + where-clauses
- CSLv3-native : § section structure (→ ModuleItem) + compound-formation helper + slot-template prefix recognizer

───────────────────────────────────────────────────────────────

§ T3.3 ARTIFACTS  (added 2026-04-17)

`crates/cssl-hir/src/` :
- `symbol.rs` : `Symbol` newtype + `Interner` (RefCell<Rodeo> wrapper, T3-D8)
- `arena.rs` : `HirId` + `DefId` newtypes + `HirArena` monotonic allocator
- `attr.rs` : HirAttr + HirAttrArg + HirAttrKind mirror of CST
- `ty.rs` : HirType + HirTypeKind + HirCapKind + HirRefinementKind + HirEffectRow
- `pat.rs` : HirPattern + HirPatternKind + HirPatternField
- `expr.rs` : HirExpr + HirExprKind (30+ variants) + HirBinOp + HirUnOp + HirCompoundOp +
              HirBlock + HirLiteral + HirMatchArm + HirArrayExpr + HirStructFieldInit
- `stmt.rs` : HirStmt + HirStmtKind
- `item.rs` : HirModule + HirItem + HirFn + HirStruct + HirEnum + HirInterface + HirImpl +
              HirEffect + HirHandler + HirTypeAlias + HirUse + HirConst + HirNestedModule
              (all def-bearing items carry `DefId` for cross-reference)
- `resolve.rs` : Scope + ScopeMap with nested-scope stack + module-level persistence
- `lower.rs` : `LowerCtx` + `lower_module(source, cst_module) -> (HirModule, Interner, DiagnosticBag)`
              + `resolve_module(hir_module)` fills `def: Option<DefId>` slots for single-
              segment paths that match top-level items.

§ T3.3 SCOPE COVERAGE
- Pure structural CST → HIR transform with identifiers interned via lasso
- Every HIR node tagged with a fresh HirId (counter via HirArena)
- Items additionally carry DefId for cross-reference
- use-tree flattening into linear `Vec<HirUseBinding>` for resolver consumption
- Enum-variant constructors registered in module scope alongside enums themselves
- Module-scope-only name-resolution walks expressions + types and fills `def` slots

§ T3.3 NOT-YET (deferred to T3.4)
- Bidirectional type inference (Hindley-Milner + row-polymorphism)
- Effect-row unification + evidence-passing transform
- Capability inference (Pony-6 per §§ 12)
- IFC-label propagation (Jif-DLM per §§ 11)
- Refinement-obligation generation + SMT routing (§§ 20)
- AD-legality check (§§ 05)
- @staged comptime-check + macro hygiene (§§ 13)
- Cross-module + nested-block + let-local name-resolution

───────────────────────────────────────────────────────────────

§ T3.4-phase-1 ARTIFACTS (added 2026-04-17)

`crates/cssl-hir/src/` :
- `typing.rs`   : Ty (11 variants) + Row + EffectInstance + TyVar + RowVar + ArrayLen +
                  Subst + TyCtx + TypeMap
- `unify.rs`    : classic Robinson unification + occurs-check + Remy-style row rewrite
                  with `absorb()` helper ; UnifyError (Mismatch / Arity / OccursCheck /
                  RowMismatch)
- `env.rs`      : TypeScope (Symbol → Ty) + TypingEnv (scope-stack + item-sigs)
- `infer.rs`    : InferCtx + 3-phase `check_module` (collect-sigs → check-bodies →
                  finalize) + bidirectional synth/check walk over every HirExprKind
                  variant + HIR-type → inference-Ty translation with primitive recognition

§ T3.4-phase-1 SCOPE COVERAGE
- Bidirectional HM inference with classic Robinson unification + occurs-check
- Effect-row unification via Remy-style rewrite with tail-absorption
- Primitive-type recognition (`i*`, `u*`, `f*`, `bool`, `str`, `()`, `!`)
- Nominal-type resolution via `DefId` lookup in TypingEnv
- Generic fn parameters use skolem `Ty::Param(Symbol)` in body-check
- All HirExprKind variants covered (literal, path, call, field, index, binary, unary,
  block, if, match, for, while, loop, return, break, continue, lambda, assign, cast,
  range, pipeline, try-default, try, perform, with, region, tuple, array, struct, run,
  compound, section-ref, paren, error)
- TypeMap<HirId, Ty> side-table with Subst-finalization

§ T3.4-phase-2 DEFERRED
- Capability inference (Pony-6 per §§ 12)
- IFC-label propagation (Jif-DLM per §§ 11)
- Refinement-obligation generation → SMT queue (§§ 20)
- AD-legality check (§§ 05 closure)
- `@staged` stage-arg comptime-check (§§ 06)
- Macro hygiene-mark (§§ 13)
- Let-generalization + higher-rank polymorphism

───────────────────────────────────────────────────────────────

§ T4-phase-1 ARTIFACTS (added 2026-04-17)

`crates/cssl-effects/src/` :
- `registry.rs`   : BuiltinEffect (32 variants) + EffectCategory + EffectArgShape +
                    DischargeTiming + EffectMeta + EffectRegistry + BUILTIN_METADATA
                    const-slice covering every effect in `specs/04 § BUILT-IN EFFECTS`
- `discipline.rs` : EffectRef + CoercionRule + SubEffectError + sub_effect_check +
                    classify_coercion ; caller-row ⊇ callee-row validation with
                    name+arity match (numeric-ordering deferred to T8 const-eval)
- `banned.rs`     : SensitiveDomain + BannedReason + banned_composition +
                    banned_composition_with_domains ; Prime-Directive F5 structural
                    encoding of `Sensitive<"coercion">` (absolute ban) +
                    `Sensitive<"surveillance">` + IO (no override) +
                    `Sensitive<"weapon">` + IO (needs Privilege<Kernel>)

§ T4-phase-1 COVERAGE
- 32 built-in effects registered with category + arg-shape + discharge metadata
- Sub-effect discipline checker with CoercionRule classification (Exact | Widening | None)
- Prime-Directive banned-composition checker (structural, not policy)
- SensitiveDomain enum with 4 canonical domains + Other(&str) variant

§ T4-phase-2 DEFERRED
- Xie+Leijen ICFP'21 evidence-passing transform (HIR → HIR+evidence)
- Linear × handler one-shot enforcement (§§ 12 R8 ; multi-shot + iso = compile-error)
- Handler-installation analysis (`perform X` requires handler for X in scope)
- Numeric-ordering coercion on `Deadline<N>` / `Power<N>` / `Thermal<N>` (needs T8 const-eval)

───────────────────────────────────────────────────────────────

§ T5 ARTIFACTS (added 2026-04-17)

`crates/cssl-caps/src/` :
- `cap.rs`        : CapKind (6 variants) + CapSet (bitset) + predicates (is_linear,
                    is_mutable, is_send_safe, requires_gen_check, can_read)
- `matrix.rs`     : AliasRights (4 bool bits) + AliasMatrix::pony6() with the canonical
                    per-cap rights table ; `can_pass_through` delegates to subtype
- `subtype.rs`    : Subtype witness (7 variants : Reflexive + 6 cap-coercions) +
                    `coerce` + `is_subtype` + SubtypeError
- `linearity.rs`  : BindingId + UseKind (5 variants) + LinearUse + LinearViolation
                    (5 variants) + LinearTracker ; per-scope iso use-count tracker
- `genref.rs`     : GEN_BITS=24 + IDX_BITS=40 + GEN_MASK + IDX_MASK + GenRef(pub u64)
                    + pack/idx/gen/bump_gen/NULL

`crates/cssl-hir/src/cap_check.rs` :
- CapMap<HirId, CapKind> side-table
- check_capabilities(&HirModule) -> (CapMap, Vec<Diagnostic>)
- hir_cap_to_semantic + top_cap + param_subtype_check utilities
- Per-fn LinearTracker scope-opened at entry, closed at exit

§ T5 + T3.4-phase-2-cap COVERAGE
- 6 Pony capabilities with full rights table (alias-local/global, mut-local/global)
- Full subtype lattice : Reflexive + IsoTo{Trn/Val/Box/Tag} + TrnToBox + ValToBox
- Linear-value tracker with 5 violation kinds (Leak / DuplicateConsume / MultiShotResume /
  ReadWithoutConsume / UseAfterScope)
- Vale gen-ref packed u64 layout (u40+u24) + bump_gen wrap-at-2^24
- Sig-level cap validation for all fn items (including impl / interface / effect / handler methods)
- LinearTracker initialization per-fn for iso parameters

§ T5 + T3.4-phase-2-cap NOT-YET
- Full expression-walk consume tracking (T5-phase-2 / T3.4-phase-2.5)
- Handler one-shot-resume enforcement at `resume` call-sites
- `freeze(x)` / `consume x` sugar parsing + lowering
- Field-level cap validation through field-access chains
- gen-ref deref-check synthesis (MIR lowering @ T6)

───────────────────────────────────────────────────────────────

§ T6-phase-1 ARTIFACTS (added 2026-04-17)

`crates/cssl-mir/src/` :
- `op.rs`     : CsslOp enum (26 dialect variants + Std) + OpCategory (14 categories) +
                OpSignature (operand/result arity) + BUILTIN_METADATA + ALL_CSSL
- `value.rs`  : ValueId (SSA id newtype) + MirValue + MirType (Int/Float/Bool/None/
                Handle/Tuple/Function/Memref/Opaque) + IntWidth + FloatWidth
- `block.rs`  : MirBlock + MirRegion + MirOp (+ builder chain : with_operand /
                with_result / with_attribute / with_region)
- `func.rs`   : MirFunc (name + params + results + effect_row + cap + ifc_label +
                attributes + body + next_value_id) + MirModule
- `print.rs`  : MlirPrinter + print_module ; valid MLIR textual-format output for
                every supported op / type / attribute shape
- `lower.rs`  : LowerCtx + lower_function_signature + lower_module_signatures ;
                HIR type translation (primitive recognition + nominal + tuple + ref +
                cap + array + slice + refined)

`crates/cssl-mlir-bridge/src/` :
- `emit.rs`   : emit_module_to_string + emit_module_to_writer (io::Write target)

§ T6-phase-1 COVERAGE
- 26 custom cssl.* dialect ops catalogued with full per-op metadata
- MLIR textual-format pretty-printing for modules / funcs / blocks / regions / ops /
  attributes / types
- HIR → MIR skeleton lowering (fn signatures + effect-row attribute + cap attribute)
- Stable text-emission API regardless of melior FFI availability

§ T6-phase-2 DEFERRED
- melior / mlir-sys FFI integration (requires MSVC toolchain per T1-D7)
- TableGen CSSLOps.td authoring
- Full HIR body → MIR expression lowering
- Pass pipeline (monomorphization / macro-expansion / AD / @staged / evidence-passing /
  IFC / SMT-discharge / telemetry-probe insertion / structured-CFG validation)
- Dialect-conversion to spirv / llvm / gpu

───────────────────────────────────────────────────────────────

§ T7-phase-1 ARTIFACTS (added 2026-04-17)

`crates/cssl-autodiff/src/` :
- `rules.rs`     : DiffMode (Primal/Fwd/Bwd) + Primitive (15 : FAdd/FSub/FMul/FDiv/FNeg/
                   Sqrt/Sin/Cos/Exp/Log/Call/Load/Store/If/Loop) + DiffRule +
                   DiffRuleTable::canonical() → 30 rules (15 primitives × 2 non-primal modes)
- `decl.rs`      : DiffDecl (name + def + params + `no_diff` / `lipschitz_bound` / `checkpoint`
                   flags) + `from_fn` + `attr_matches` (interned-name resolver) +
                   `collect_differentiable_fns(&HirModule, &Interner) -> Vec<DiffDecl>`
- `transform.rs` : DiffTransform + DiffVariants (primal_name / fwd_name / bwd_name) +
                   `register_all` walker

`crates/cssl-jets/src/` :
- `lib.rs`       : JetOrder (FIRST = 1, SECOND = 2) + JetOp (Construct/Project/Add/Mul/
                   Apply) + JetSignature (operand-arity / result-arity / order-dependence) +
                   JetError + validate_construct / validate_project / validate_binary_order

§ T7-phase-1 COVERAGE
- Canonical rules-table with 30 entries ready for T7-phase-2 rule-application walker
- `@differentiable` discovery + `@no_diff` / `@lipschitz_bound` / `@checkpoint` flag extraction
- Variant-name generation (`foo` → `foo_fwd` / `foo_bwd`) at fn granularity
- Jet<T, N> structural signatures + order-preservation validators (ready for T6 MIR lowering)

§ T7-phase-2 DEFERRED
- Walking HirExpr + applying rules at each primitive site
- Tape-buffer allocation (iso-capability scoped)
- `@checkpoint` attribute-arg extraction
- GPU-AD tape-location resolution (device vs shared vs unified)
- Killer-app gate : `bwd_diff(sphere_sdf)(p).d_p` bit-exact vs analytic
- Jet<T, ∞> lazy-stream variant (T17 scope)

───────────────────────────────────────────────────────────────

§ T8-phase-1 ARTIFACTS (added 2026-04-17)

`crates/cssl-staging/src/` :
- `lib.rs`       : StageArg + StageArgKind (CompTime/Runtime/Polymorphic) + StagedDecl +
                   `collect_staged_fns(&HirModule, &Interner) -> Vec<StagedDecl>` with full
                   HirExpr walker (count_expr + count_block) counting `#run` sites +
                   Specializer + SpecializationSite + StagingError

`crates/cssl-macros/src/` :
- `lib.rs`       : MacroTier (3 variants : Tier0ImportOnly / Tier1AttrAnnotation /
                   Tier2PatternMatch) + ScopeId + HygieneMark (Racket set-of-scopes with
                   add / remove / contains / flip / union) + SyntaxObject (equality respects
                   mark) + ScopeAllocator + MacroDecl + MacroRegistry + MacroError

`crates/cssl-futamura/src/` :
- `lib.rs`       : FutamuraLevel (P1/P2/P3 with monotonic `order()`) + Projection +
                   FixedPointRecord (converged iff hash-N == hash-N+1) + Orchestrator +
                   FutamuraError

§ T8-phase-1 COVERAGE
- `@staged` discovery + per-stage-arg `CompTime|Runtime|Polymorphic` classification
- `#run` site discovery via full HIR expression-tree walker
- Racket hygienic-macro primitives : set-of-scopes + mark flip/union + scope allocator
- Three-tier macro registry with collision detection + tier ordering
- P1/P2/P3 Futamura projection tracker + hash-based fixed-point convergence detection

§ T8-phase-2 DEFERRED
- Actual specialization walk (clone fn + const-propagate)
- Native comptime-eval (compile-native ; R14 avoid-Zig-20x)
- `@type_info` / `@fn_info` / `@module_info` reflection API
- Transform-dialect pass-schedule emission
- Tier-2 pattern-match expansion (quasi-quotation + syntax-case)
- Tier-3 `#run` proc-macro sandbox
- P3 self-bootstrap fixed-point verification (needs running stage-1 compiler)

───────────────────────────────────────────────────────────────

§ T3.4-phase-2-refinement ARTIFACTS (added 2026-04-17)

`crates/cssl-hir/src/` :
- `refinement.rs` : ObligationKind (Predicate { text } / Tag { name } / Lipschitz { bound }) +
                    RefinementObligation (id / origin / span / enclosing_def / kind /
                    base_type_text) + ObligationId (u32 newtype) + ObligationBag
                    (monotonic-append + push / get / iter / len) +
                    `collect_refinement_obligations(&HirModule, &Interner) -> ObligationBag`
                    + ObligationCtx (walk_item / walk_fn / walk_type / walk_expr) +
                    pretty_type / pretty_expr helpers

§ T3.4-phase-2-refinement COVERAGE
- `{v : T | P(v)}` predicate refinements → ObligationKind::Predicate
- `T'tag` refinement-sugar → ObligationKind::Tag { name : Symbol }
- `SDF'L<k>` Lipschitz-bound refinements → ObligationKind::Lipschitz { bound : u32 }
- Types walked recursively through Tuple / Array / Slice / Reference / Capability /
  Function / Path shapes (refinements nested in `Vec<{v : i32 | v > 0}>` caught)
- Each obligation captures origin HirId + Span + enclosing DefId for diagnostics
- Stable ObligationId handout (monotonic u32) suitable for downstream caching

§ T3.4-phase-3 DEFERRED (remaining T3.4-phase-2 slices)
- Capability inference (already landed T5-phase-1 sig-level ; body-walk pending)
- IFC-label propagation (Jif-DLM per §§ 11)
- AD-legality check (§§ 05 closure)
- `@staged` stage-arg comptime-check (§§ 06)
- Macro hygiene-mark propagation through expansion (§§ 13)
- HIR-expression → SMT-Term translation (T9-phase-2 counterpart)

───────────────────────────────────────────────────────────────

§ T9-phase-1 ARTIFACTS (added 2026-04-17)

`crates/cssl-smt/src/` :
- `term.rs`   : Theory (7 variants : LIA / LRA / NRA / BV / UF / UFLIA / ALL with QF_
                prefixes) + Sort (Bool / Int / Real / BitVec(u32) / Uninterp(String)) +
                Literal (Bool / Int / Rational / BitVec) + Term (Var / Lit / App / Forall /
                Exists / Let) with full recursive `render()` → SMT-LIB text
- `query.rs`  : FnDecl (name + params + return) + Assertion (term + optional `:named` label)
                + Query (theory + sort_decls + fn_decls + assertions + get_model +
                get_unsat_core flags) + Verdict (Sat / Unsat / Unknown)
- `emit.rs`   : `emit_smtlib(&Query) -> String` producing valid SMT-LIB 2.6 :
                `(set-logic)(declare-sort)(declare-fun)(assert)(check-sat)(get-model)
                (get-unsat-core)`
- `solver.rs` : SolverKind (Z3 / Cvc5) + Solver trait + SolverError (BinaryMissing /
                NonZeroExit / UnparseableOutput / Io) + Z3CliSolver (`z3 -in -smt2`) +
                Cvc5CliSolver (`cvc5 --lang=smt2 -`) + `run_cli` (spawn subprocess + pipe
                SMT-LIB through stdin + parse first-line verdict) +
                `discharge(&ObligationBag, &Solver) -> Vec<(ObligationId, Result<Verdict,
                SolverError>)>` with stub trivial-query per-obligation

§ T9-phase-1 COVERAGE
- Complete SMT-LIB 2.6 textual emission for theory declaration + sort declaration +
  fn declaration + assertion + check-sat + model/unsat-core extraction
- Term tree with quantifier (forall / exists) + let-binding rendering
- Z3 + CVC5 CLI subprocess adapters mirroring T6-D1 MLIR-text-CLI fallback pattern —
  no `z3-sys` / `cvc5-sys` FFI required (keeps stage-0 on gnu-ABI per T1-D7)
- Obligation-bag → stub-query → subprocess → verdict pipeline composes end-to-end
- 28 lib-tests covering every rendering path + emission shape + solver-error display +
  stub-discharge shape

§ T9-phase-2 DEFERRED
- Direct `z3-sys` / `cvc5-sys` FFI (blocked on MSVC toolchain per T1-D7)
- KLEE symbolic-exec fallback for coverage-guided paths
- Proof-certificate emission + Ed25519-signed certs (R18 audit-chain per §§ 22)
- Per-obligation-hash disk cache
- Full HIR-expression → SMT-Term translation (stage-0 uses trivial `(assert true)` stubs)
- Incremental solving (`push` / `pop` / `assert-soft` / weighted optimization)

───────────────────────────────────────────────────────────────

§ T10-phase-1-codegen ARTIFACTS (added 2026-04-17)

`crates/cssl-cgen-cpu-cranelift/src/` :
- `target.rs`   : CpuTarget (7 µarchs : Alder/Raptor/Meteor/Arrow Lake + Zen4/5 + generic-v3) +
                  `default_simd_tier` + `triple` + CpuTargetProfile (windows/linux/darwin defaults) +
                  DebugFormat (dwarf5 / codeview / none)
- `feature.rs`  : SimdTier (scalar/sse2/avx2/avx512) monotonic-lattice + CpuFeature (17 flags) +
                  CpuFeatureSet (ordered append + render-target-features)
- `abi.rs`      : Abi (SysV / Win64 / Darwin) + ObjectFormat (Elf / Coff / MachO) +
                  extension + typical-pairing
- `types.rs`    : ClifType (9 : i8/16/32/64 + b1 + f16/32/64 + r64) + `clif_type_for(MirType)`
- `emit.rs`     : `emit_module(&MirModule, &CpuTargetProfile) -> EmittedArtifact` with
                  CLIF-like text + per-fn skeleton + CpuCodegenError (3 variants)

`crates/cssl-cgen-gpu-spirv/src/` :
- `capability.rs` : SpirvCapability (32 variants) + `requires_extension` + SpirvCapabilitySet
                    + SpirvExtension (24 KHR/EXT/INTEL/NV + ext-inst-set) + SpirvExtensionSet
                    (plain vs ext-inst-set split)
- `target.rs`     : SpirvTargetEnv (9 profiles incl. Vulkan-1.0..1.4 / universal-1.5/1.6 /
                    OpenCL-kernel / WebGPU) + MemoryModel (Simple/GLSL450/OpenCL/Vulkan) +
                    AddressingModel (Logical/Physical32/64/PhysicalStorageBuffer64) +
                    ExecutionModel (15 stages incl. full RT)
- `module.rs`     : SpirvSection (11 rigid-ordered sections per `specs/07` § SPIR-V EMISSION
                    INVARIANTS) + SpirvModule + SpirvEntryPoint + `seed_vulkan_1_4_defaults` +
                    `seed_opencl_kernel_defaults`
- `emit.rs`       : `emit_module(&SpirvModule) -> String` producing `spirv-as`-compatible
                    disasm + SpirvEmitError + `minimal_vulkan_compute_module` helper

`crates/cssl-cgen-gpu-dxil/src/` :
- `target.rs`   : ShaderModel (SM 6.0..6.8) + ShaderStage (15 stages incl. RT/mesh/callable) +
                  HlslProfile (stage + model with compat-check) + RootSignatureVersion
                  (v1.0/1.1/1.2) + DxilTargetProfile (compute_default / vertex_default / pixel_default)
- `hlsl.rs`     : HlslModule + HlslStatement (CBuffer / Struct / RwBuffer / Function / Raw)
- `emit.rs`     : `emit_hlsl(&MirModule, &DxilTargetProfile, entry) -> HlslModule` +
                  stage-aware semantic emission + DxilError
- `dxc.rs`      : DxcCliInvoker subprocess wrapper (`dxc.exe -T ... -E ...`) + DxcInvocation
                  + DxcOutcome (Success / DiagnosticFailure / BinaryMissing / IoError)

`crates/cssl-cgen-gpu-msl/src/` :
- `target.rs`    : MslVersion (2.0..3.2) + MetalStage (7 : vertex/fragment/kernel/object/mesh/
                   tile/visible-fn) + `min_msl_version` check + MetalPlatform (macos/ios/tvos/
                   visionos) + ArgumentBufferTier + MslTargetProfile
- `msl.rs`       : MslModule + MslStatement (Include / UsingNamespace / Struct / Typedef /
                   Function / Raw) + `seed_prelude` (`#include <metal_stdlib>` + `using namespace metal;`)
- `emit.rs`      : `emit_msl(&MirModule, &MslTargetProfile, entry) -> MslModule` with
                   per-stage MSL-attribute-form skeleton + MslError
- `spirv_cross.rs` : SpirvCrossInvoker subprocess wrapper (`spirv-cross --msl --stage ...`)

`crates/cssl-cgen-gpu-wgsl/src/` :
- `target.rs`  : WebGpuStage (3 : vertex/fragment/compute) + WebGpuFeature (7 WebGPU feature flags) +
                 WgslLimits (webgpu_default + compat presets with workgroup-size / bind-groups /
                 storage-buffers) + WgslTargetProfile
- `wgsl.rs`    : WgslModule + WgslStatement (Enable / Struct / Binding / EntryFunction /
                 HelperFunction / Raw)
- `emit.rs`    : `emit_wgsl(&MirModule, &WgslTargetProfile, entry) -> WgslModule` with
                 `@compute @workgroup_size(...)` / `@vertex` / `@fragment` skeletons +
                 auto-derived `enable f16` / `enable subgroups` directives + WgslError

§ T10-phase-1-codegen COVERAGE
- Every backend emits MIR → target-specific source text with canonical entry-point
  skeletons matching the stage's calling-convention + attribute set
- Rigid SPIR-V section ordering enforced (Capabilities → Extensions → ExtInstImports →
  MemoryModel → EntryPoints → ExecutionModes → Debug → Annotations → Types → FnDecl → FnDef)
- HLSL / MSL / WGSL output is textual + diff-stable for snapshot-testing (T11-phase-1)
- Calling-convention bridges : HLSL `SV_DispatchThreadID`, MSL `thread_position_in_grid`,
  WGSL `@builtin(global_invocation_id)` all emitted per-stage
- CLI-subprocess adapters (DxcCliInvoker + SpirvCrossInvoker) gracefully degrade when
  the binary is absent (BinaryMissing outcome tested without panic)

§ T10-phase-2-codegen DEFERRED
- Cranelift FFI integration : `cranelift-codegen` + `-frontend` + `-module` + `-object`
  for real CLIF → machine-code → object-file (ELF / COFF / Mach-O)
- rspirv module-builder → real SPIR-V binary emission
- `spirv-val` / `dxc` / `spirv-cross` / `naga` wired into CI validation pipeline
- Full MIR body → target-IR op lowering tables
- Structured-CFG preservation (scf.* → OpSelectionMerge / OpLoopMerge)
- Debug-info emission (DWARF-5 / CodeView / NonSemantic.Shader.DebugInfo.100)
- Fat-binary assembly (header + host-obj + gpu-blobs + telemetry-schema + audit-manifest)
- Runtime CPU-dispatch multi-variant fat-kernels (AVX2 + AVX-512)
- `metal-shaderconverter` Apple-only toolchain integration

───────────────────────────────────────────────────────────────

§ T10-phase-1-hosts ARTIFACTS (added 2026-04-17)

`crates/cssl-host-vulkan/src/` :
- `device.rs`     : VulkanVersion (1.0..1.4 with packed-API-int) + GpuVendor (8 :
                    Intel/NVIDIA/AMD/Apple/Qualcomm/ARM/Mesa/Other with PCI-ID-resolver) +
                    DeviceType (5 : integrated/discrete/virtual/cpu/other) +
                    DeviceFeatures (25 VK-features CSSLv3 cares about) +
                    VulkanDevice + `stub` constructor + summary()
- `extensions.rs` : VulkanExtension (30 : VK-1.4-core + RT + CoopMat + BDA + mesh +
                    descriptor-indexing + mutable-descriptor-type + telemetry) +
                    VulkanLayer (5 : validation / api-dump / monitor / profiles /
                    synchronization2) + VulkanExtensionSet + is-core-in-vk-1.4 check
- `arc_a770.rs`   : ArcA770Profile canonical ({32 Xe-cores, 512 XVE, 512 XMX, 32 RT,
                    2.1 GHz, 16 GB GDDR6, 560 GB/s, 225 W}) + expected_extensions() +
                    to_vulkan_device() + peak_fp32_tflops
- `probe.rs`      : FeatureProbe trait (enumerate_devices / supported_extensions /
                    has_extension) + StubProbe + ProbeError (LoaderMissing / FfiNotWired /
                    DeviceNotFound)

`crates/cssl-host-level-zero/src/` :
- `api.rs`        : L0ApiSurface (24 : ze-init / ze-driver / ze-device / ze-context /
                    ze-cmd-list / ze-event / ze-module / ze-kernel / ze-USM + full
                    zes-sysman) + is_sysman() + UsmAllocType (host/device/shared)
- `driver.rs`     : L0Driver + L0Device + L0DeviceType (gpu/cpu/fpga/mca/vpu) +
                    L0DeviceProperties + stub_arc_a770()
- `sysman.rs`     : SysmanMetric (11 : power×2 / thermal×2 / frequency×3 / engine /
                    ras / processes / perf-factor) + MetricCategory + SysmanMetricSet
                    (full_r18 + advisory presets) + SysmanSample + SysmanCapture +
                    TelemetryProbe trait + StubTelemetryProbe returning canonical
                    Arc A770 values + TelemetryError

`crates/cssl-host-d3d12/src/` :
- `adapter.rs`    : FeatureLevel (12.0..12.2) + DxgiAdapter (w/ stub_arc_a770 +
                    stub_warp) + is_software flag
- `features.rs`   : D3d12FeatureOptions (10 bool flags : RT-1.1 / mesh-1 / sampler-
                    feedback / VRS-2 / atomic-int64 / FP16 / Int16 / dynamic-resources
                    / wave-size-spec) + WaveMatrixTier + arc_a770() preset
- `heap.rs`       : CommandListType (7 : direct/compute/copy/bundle/video×3) +
                    DescriptorHeapType (4 : cbv-srv-uav/sampler/rtv/dsv) + HeapType
                    (4 : default/upload/readback/custom)

`crates/cssl-host-metal/src/` :
- `device.rs`     : GpuFamily (14 : Apple1..Apple9 + Mac1/2 + Common1/2/3) +
                    MtlDevice (w/ stub_m3_max + stub_intel_mac)
- `feature_set.rs`: MetalFeatureSet (7 : macOS-family1/2 + iOS-family6 + Metal-3 /
                    3.1-apple8/9 / 3.2) + supports_raytracing / supports_mesh_shaders
                    / supports_cooperative_matrix
- `heap.rs`       : MetalHeapType (shared/private/managed/memoryless) +
                    MetalResourceOptions

`crates/cssl-host-webgpu/src/` :
- `adapter.rs`    : WebGpuBackend (5 : browser/vulkan/metal/dx12/gl) +
                    AdapterPowerPref (low-power / high-perf / no-pref) + WebGpuAdapter
                    (w/ stub_arc_a770_vulkan + stub_browser_webgpu + stub_software)
- `features.rs`   : WebGpuFeature (14 WebGPU spec features) + SupportedFeatureSet +
                    WebGpuLimits (26-field snapshot of WebGPU canonical default limits)

§ T10-phase-1-hosts COVERAGE
- All 5 backends catalog their feature surfaces as first-class enums + structs
- Arc A770 canonical profile embedded as ground-truth hardware spec per `specs/10`
- Sysman R18 telemetry metric catalog with `TelemetryProbe` trait + stub returning
  canonical Arc A770 sample values (225 W TDP, 55 °C idle, 1.8 GHz baseline)
- `FeatureProbe` + `TelemetryProbe` trait boundaries ready for phase-2 swap to
  ash / level-zero-sys / windows-rs / metal / wgpu bindings without public-API churn
- Every crate `forbid(unsafe_code)` — FFI allowances deferred to phase-2 per T1-D5

§ T10-phase-2-hosts DEFERRED
- `ash` FFI for VK-1.4 instance + device + queue + command-buffer lifecycle
- `level-zero-sys` FFI for ze* driver / device / context + zes* sysman property sampling
- `windows-rs` FFI for D3D12 Device + CommandQueue + DescriptorHeaps + RootSignatures
- `metal` crate FFI (cfg-gated Apple-only) for MTLDevice / argument-buffers / fn-constants
- `wgpu` runtime integration for cross-platform WebGPU submission
- Validation-layer diagnostic routing
- Surface / swapchain presentation
- Multi-device coexistence (L0 + Vulkan on Intel)

───────────────────────────────────────────────────────────────

§ T11-phase-1 ARTIFACTS (added 2026-04-17)

`crates/cssl-telemetry/src/` :
- `scope.rs`    : TelemetryScope (25 variants across 6 ScopeDomain categories :
                  Cpu/Gpu/PowerThermal/Ras/AppSemantic/Compound per `specs/22`
                  TAXONOMY) with `as_u16` stable encoding + TelemetryKind (Sample/
                  SpanBegin/SpanEnd/Counter/Audit)
- `ring.rs`     : TelemetrySlot (64-byte record w/ timestamp+scope+kind+tid+pid+
                  inline-payload) + TelemetryRing (single-thread SPSC stand-in
                  matching final atomic-ring invariants : producer-never-blocks,
                  overflow-counter-increments, FIFO-drain) + RingError
- `audit.rs`    : ContentHash (BLAKE3-stub, phase-2 swaps real `blake3`) +
                  Signature (Ed25519-stub, phase-2 swaps `ed25519-dalek`) +
                  AuditEntry + AuditChain (append-only + verify_chain detecting
                  GenesisPrevNonZero / ChainBreak / InvalidSequence)
- `exporter.rs` : Exporter trait + ChromeTraceExporter (Chrome DevTools JSON) +
                  JsonExporter (newline-delimited) + OtlpExporter (stage-0
                  NotWired outcome) + ExportError
- `schema.rs`   : TelemetryScopeSet (subset-of check for scope-narrowing invariant
                  per `specs/22` callee-⊑-caller rule) + TelemetrySchema
                  (version + module + scopes + ring_size + sampling_hz, with
                  defaults_for + summary)

`crates/cssl-persist/src/` :
- `schema.rs`    : SchemaVersion (major.minor + 32-byte digest ; with_digest_from
                   + is_minor_upgrade_of)
- `migration.rs` : SchemaMigration + MigrationChain (panicking-assert on broken
                   linkage + start_version/end_version + stable iteration order)
- `image.rs`     : ImageHeader (canonical "CSSLPRS1" magic + format_version +
                   record_count + content_digest) + ImageRecord (key + schema +
                   payload) + PersistenceImage (auto-digest-refresh on append +
                   find-by-key + total_payload_size)
- `backend.rs`   : PersistenceBackend trait (put / get / snapshot / len) +
                   InMemoryBackend (HashMap-backed with insertion-order preserved)
                   + PersistError (NotFound / SchemaMismatch / BackendNotWired)

§ T11-phase-1 COVERAGE
- Ring invariants pinned : producer-never-blocks, overflow-counter-advances, FIFO-
  drain, total-pushed counts all attempts (ok + overflow)
- Audit-chain verify detects genesis-violation + linkage-break + bad-sequence
- 25 telemetry scopes across 6 domains with stable u16 encoding ready for MIR
  probe-op lowering (stage-0 MIR already has `cssl.telemetry.probe` op)
- Chrome DevTools trace output syntactically valid (JSON array + ph-field = B/E/C/i)
- Persistence-image canonical "CSSLPRS1" magic + schema-version digest stable
- MigrationChain panics on broken linkage — matches `specs/18` "migrations must
  form a connected chain" invariant at construct-time

§ T11-phase-2 DEFERRED
- Real `blake3` hash (stub-hash is deterministic but not cryptographically strong)
- Real `ed25519-dalek` signing (stub-sign is a deterministic byte-fold)
- OTLP gRPC + HTTP transport (needs `prost` / `reqwest`)
- Cross-thread atomic SPSC ring (stage-0 is single-thread)
- Level-Zero sysman sampling-thread → TelemetryRing integration
- WAL-file + LMDB backends (append-only log + mmap B+tree)
- `@hot_reload_preserve` HIR attribute extraction + root-set discovery
- Live-object migration application
- `{Telemetry<S>}` effect-row HIR lowering pass (compile-time probe insertion)
- Overhead-budget enforcement (0.5% Counters / 5% Full / 0.1% Audit)
- cssl-testing oracle-body fleshing (all 12 modes still at T1 Stage0Stub)
- R16 attestation of image-provenance (BLAKE3 chain + Ed25519 signatures)

───────────────────────────────────────────────────────────────

§ T12-phase-1-examples ARTIFACTS (added 2026-04-17)

`examples/` (repo-root, referenced from `specs/21` § VERTICAL-SLICE ENTRY POINT) :
- `hello_triangle.cssl` : module/use decls + struct Vertex + const-array triangle
                          data + @vertex/@fragment entry-points w/ effect-rows
                          {GPU, Deadline<16ms>, Telemetry<DispatchLatency>} + host-
                          side pipeline builder fn
- `sdf_shader.cssl`     : **KILLER-APP GATE** — @differentiable @lipschitz sphere_sdf
                          + scene_sdf (union) + ray_march + surface_normal via
                          `bwd_diff(scene_sdf)(hit_pos).d_p` + Lambert shade +
                          @fragment sdf_pixel entry-point
- `audio_callback.cssl` : AudioDSPGraph w/ refinement-typed sample_rate +
                          sine_osc + SIMD256-vectorized process_block + @staged
                          audio_callback w/ full {CPU,SIMD256,NoAlloc,NoUnbounded,
                          Deadline<1ms>,Realtime<Crit>,PureDet,DetRNG,
                          Audit<"audio-callback">} effect-row + handler
                          AudioEngine

`crates/cssl-examples/src/lib.rs` (integration-tests crate) :
- `PipelineOutcome` record (name + token_count + cst_item_count +
  parse_error_count + hir_item_count + lower_diag_count) + `is_accepted()` +
  `summary()`
- `pipeline_example(name, source) -> PipelineOutcome` : lex → parse → lower_module
- `all_examples() -> Vec<PipelineOutcome>` : drives all 3 canonical sources
- `HELLO_TRIANGLE_SRC` / `SDF_SHADER_SRC` / `AUDIO_CALLBACK_SRC` via include_str!
  from repo-root `examples/`

§ T12-phase-1 COVERAGE
- All 3 canonical .cssl example files present at repo-root
- Full stage-0 front-end pipeline (lex+parse+HIR-lower) runs end-to-end on each
- 11 integration-tests covering source-non-empty markers + tokenization shape +
  breadcrumb checks (@differentiable, bwd_diff(scene_sdf), Realtime<Crit>,
  Audit<"audio-callback">) + all-examples-returns-three + summary formatting
- include_str! at compile-time enforces : examples MUST be present for build

§ T12-phase-2 DEFERRED
- Full type-check + refinement-obligation generation integration (blocked on
  T3.4-phase-3 IFC / AD-legality / hygiene slices)
- MIR lowering + codegen-text via the 5 cgen-* backends (requires HIR-body → MIR
  expr-lowering from T6-phase-2)
- spirv-val / dxc / naga round-trip validation on emitted artifacts
- Vulkan device creation + actual pixel-render via cssl-host-vulkan (T10-phase-2)
- **`bwd_diff(scene_sdf)` bit-exact-vs-analytic verification** — THE final
  acceptance criterion for F1 correctness, gated on T7-phase-2 rule-application
  walker + T9-phase-2 SMT real-solver dispatch
- `vertical_slice.cssl` (≤ 5000 lines) : full composition exercising every v1
  engine primitive per §§ 21 VERTICAL-SLICE ENTRY POINT ; deferred to T13+
  (self-host stage-1 needs stabilized surface first)

───────────────────────────────────────────────────────────────

§ SESSION-1 PHASE-1 COMPLETE

All 12 tasks T1..T12 have phase-1 landings. Remaining deferred work :
- T3.4-phase-3 : IFC / AD-legality / @staged-check / macro-hygiene
- T6-phase-2 : melior FFI + HIR-body → MIR-expr lowering + pass pipeline
- T7-phase-2 : rule-application walker + tape-buffer + killer-app gate
- T8-phase-2 : actual specialization walk + native comptime-eval + P3 bootstrap
- T9-phase-2 : HIR→SMT-term translation + FFI + KLEE + proof-certs + cache
- T10-phase-2 : cranelift FFI + rspirv FFI + dxc CI integration + 5× host FFI
- T11-phase-2 : real BLAKE3/Ed25519 + OTLP + WAL/LMDB + oracle-bodies +
  @hot_reload_preserve HIR pass
- T12-phase-2 : full-type-check integration + bit-exact AD verification +
  vertical_slice.cssl

§ NEXT SESSIONS
- Session-2 : T3.4-phase-3 + T6-phase-2 (unblocks MIR-body lowering for all codegen
  backends + refinement-discharge)
- Session-3+ : T7+T9-phase-2 (landing killer-app gate)
- Session-N : T13+ (self-host CSSLv3-compiled compiler + C99 reproducibility anchor
  + LoA v10 integration)

───────────────────────────────────────────────────────────────

§ ACKNOWLEDGMENTS

- Apocky : direction-setting on optimal-vs-minimal CI scope (T1-D3), cslparser sourcing (T1-D2), spec authority discipline.
- Claude Opus 4.7 (1M context) : implementation + commit-gate discipline.
- Prior session (CSLv3 Session-3) : canonical cslparser + T10 CSSLv3-BRIDGE spec.
