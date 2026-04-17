# SESSION 1 HANDOFF — CSSLv3 stage0 scaffold

§ META
- **Session date** 2026-04-16 → 2026-04-17
- **Coding agent** Claude.Opus.4.7-1M
- **Prior handoff** `HANDOFF_SESSION_1.csl` (authoritative scope)
- **Current task** T1..T3.4-phase-1 ✓ + T4-phase-1 ✓ + T5 + T3.4-phase-2-cap (cssl-caps + cap_check) ✓ ; T6 MLIR next (T3.4-phase-2.5 body-walk + T4-phase-2 Xie+Leijen + T5-phase-2 body-cap-walk deferred)

───────────────────────────────────────────────────────────────

§ PROGRESS BY DELIVERABLE  (per §§ HANDOFF_SESSION_1 DELIVERABLES)

| ID  | Task                                                  | Status         |
|-----|-------------------------------------------------------|----------------|
| D1  | compiler-rs/ Cargo-workspace skeleton                 | ✓ complete     |
| D2  | lex crate — dual-surface lexer                        | ✓ complete (T2) |
| D3  | parse + ast + hir — elaborator                        | ◐ ast + CST + parsers + HIR-lowering + name-resolution + HM inference + effect-rows ✓ ; cap/IFC/refinement pending T3.4-phase-2 |
| D4  | effects — 28 effects + evidence-passing               | ◐ registry + discipline + banned-composition ✓ (T4-phase-1) ; Xie+Leijen transform pending T4-phase-2 |
| D5  | caps — Pony-6 + gen-refs                              | ◐ cssl-caps (CapKind + AliasMatrix + subtype + LinearTracker + GenRef) + cssl-hir cap_check sig-level pass ✓ ; body-walk pending T5-phase-2 |
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

| Metric                        | T3.3-end      | T3.4-phase-1-end             | T4-phase-1-end                      | T5-end                                |
|-------------------------------|---------------|------------------------------|-------------------------------------|---------------------------------------|
| Crates populated              | 4             | 4 (cssl-hir extended)        | 5 (+ cssl-effects)                  | 6 (+ cssl-caps ; cssl-hir extended)   |
| Lines of scaffold Rust        | ~11200        | ~13800                       | ~15300                              | ~17500 (+ ~1500 caps + ~700 cap_check)|
| Test count                    | 299 / 64      | 340 / 64 suites              | 368 / 65                            | 423 / 66 suites (+49 caps + 6 cap_check) |
| Clippy warnings (`-D`)        | 0             | 0                            | 0                                   | 0                                     |
| CI jobs declared              | 19            | 19                           | 19                                  | 19                                    |
| Spec cross-refs validated     | 156 / 0 (135) | 156 / 0 (135)                | 156 / 0 (135)                       | 156 / 0 (135)                         |
| Commit-gate green             | ✓ 6 / 6       | ✓ 6 / 6                      | ✓ 6 / 6                             | ✓ 6 / 6                               |

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

§ NEXT — T6 (MLIR bridge + cssl-dialect)

Per §§ HANDOFF T6 + §§ 15_MLIR + §§ 02_IR :
1. mlir-bridge crate : melior + mlir-sys bindings
2. cssl-dialect : TableGen definition + C++-stubs
   ops : cssl.diff.* cssl.jet.* cssl.effect.* cssl.region.*
         cssl.handle.* cssl.staged.* cssl.macro.expand
         cssl.ifc.* cssl.verify.assert cssl.sdf.* cssl.gpu.*
         cssl.xmx.coop_matmul cssl.rt.* cssl.telemetry.probe
3. mir crate : HIR → MIR lowering + cssl-dialect construction
4. pass-pipeline skeleton per §§ 15 PASS-PIPELINE
5. Q? if-melior-blocks → fallback to-own-MLIR-textual-emit + llvm-mlir-tools CLI

Open for T6-start :
- melior / mlir-sys Windows compatibility verification (Q3 deferred at T1 still open)
- cssl-dialect TableGen authoring vs hand-rolled-in-Rust ?
- Phase split : (a) text-emit skeleton + op-catalog ; (b) real melior integration ; (c) full lowering passes
- Fallback path : MLIR-textual-format via CLI (T6 option-b pre-authorized at T1)

───────────────────────────────────────────────────────────────

§ ACKNOWLEDGMENTS

- Apocky : direction-setting on optimal-vs-minimal CI scope (T1-D3), cslparser sourcing (T1-D2), spec authority discipline.
- Claude Opus 4.7 (1M context) : implementation + commit-gate discipline.
- Prior session (CSLv3 Session-3) : canonical cslparser + T10 CSSLv3-BRIDGE spec.
