# § Wave-Jε Dispatch Prompts — L0 errors + L1 logging
## ⟦ pre-staging ; ¬-commit ; awaits Jβ-merge ⟧

**Doc-ID** : `_drafts/phase_j/wave_je_implementation_prompts.md`
**Wave** : Jε (implementation) ⇐ spec-author Jβ-1
**Author** : Apocky+Claude (W-Jε prompts)
**Status** : ◐ DRAFT (pre-stage ; N! commit ; ready-for-dispatch on Jβ-merge)
**Spec-anchor** : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1-2 + § 6.1-6.2
**Role-anchor** : `_drafts/phase_j/02_reviewer_critic_validator_test_author_roles.md` § 1-4

---

## §0 WAVE OVERVIEW

```csl
§ WAVE-Jε
  slices            : 2 (Jε-1 cssl-error + Jε-2 cssl-log)
  agents            : 8 (2 pods × 4 roles ⟨Implementer + Reviewer + Critic + Validator⟩)
                       + 0 Test-Author ∵ collapsed-into-Validator-tests-from-spec @ Jε scope
  parallelism       : Jε-1 ∥ Jε-2 (independent ; loose-interface @ Severity-only)
  ETA per pod       : 30-60 min ⟨multi-agent + iteration-loop included⟩
  total LOC         : ~6K ⟨Jε-1 ~3K + Jε-2 ~3K⟩
  total tests       : ~250 ⟨Jε-1 ~120 + Jε-2 ~130⟩
  foundation-deps   : cssl-telemetry + cssl-substrate-prime-directive ⟵ already-merged
  out-of-scope      : Jε-3 lints + Jε-4 panic-boundary ⟵ separate downstream wave
  pre-merge-gate    : 5-of-5 per pod ⟨Implementer-self-test + Reviewer-OK + Critic-veto-cleared
                       + Validator-spec-conformance + tests-passing⟩
```

§Wave-graph (Jε-1 + Jε-2 only) :
```
  ⟨Jβ-merge⟩
       │
       ▼
  Jε-1 cssl-error  ∥  Jε-2 cssl-log     ⟵ THIS WAVE (parallel ; independent)
       │                       │
       └───────┬───────────────┘
               ▼
            Jε-3 + Jε-4 ⟵ downstream (separate dispatch ; depends on Jε-1/Jε-2 surface stable)
               │
               ▼
            Wave-Jζ telemetry-build (consumes L0 + L1)
            Wave-Jθ MCP-server      (exposes L0/L1 query)
```

§Loose-interface contract (Jε-1 ↔ Jε-2 boundary) :
- Jε-2 imports `Severity` + `EngineError` + `ErrorContext` from Jε-1
- Jε-2 does NOT import panic-types ; Jε-2 does NOT import PD-violation-types
- Severity-enum is the only true coupling-surface ⟵ stabilize FIRST in Jε-1 ; Jε-2 may stub-then-replace

---

## §1 POD-TEMPLATE (reusable across Jε-1 + Jε-2)

⟨W! 4 prompts below = filled-into per-slice § 2 + § 3 ; do-not-paraphrase ; substitute ⟪VARS⟫⟩

### §1.1 IMPLEMENTER prompt-template

```
ROLE : Implementer ⟪SLICE⟫
SCOPE : ⟪CRATE-NAME⟫ @ ⟪WORKTREE-PATH⟫
SPEC-ANCHOR : _drafts/phase_j/05_l0_l1_error_log_spec.md § ⟪SPEC-SECTION⟫
BRANCH : ⟪BRANCH-NAME⟫
LOC TARGET : ⟪LOC⟫ ; TEST TARGET : ⟪TESTS⟫

DELIVERABLES :
  1. ⟪CRATE-NAME⟫/Cargo.toml ⟨deps per spec § ⟪DEPS-SECTION⟫⟩
  2. ⟪CRATE-NAME⟫/src/lib.rs ⟨public surface ; module re-exports⟩
  3. ⟪MODULE-LIST⟫ ⟨per spec § ⟪MODULE-SECTION⟫⟩
  4. ⟪CRATE-NAME⟫/tests/*.rs ⟨categories per spec § ⟪TEST-SECTION⟫⟩
  5. workspace Cargo.toml addition ⟨add to members ; alphabetical⟩

DISCIPLINE :
  - N! unwrap / N! expect on user-data paths ⟨severe-lint-pre-merge⟩
  - N! raw-path strings in any field ⟨D130 path-hash-only ; ALL paths via PathHasher⟩
  - N! double-log ⟨single-canonical-entry per ring-write⟩
  - W! preserve replay-determinism ⟨replay_strict flag honored⟩
  - W! From-impl every per-crate-error-variant ⟨exhaustive ; closed-enum⟩
  - W! severity-classification per § 1.3 ⟨default Error ; override per-variant⟩

ITERATION-LOOP :
  - on-Reviewer-comment : address-then-respond ; ¬ silent-skip
  - on-Critic-veto : address-or-escalate-to-Architect
  - on-Validator-divergence : reconcile-spec-or-implementation ; spec-wins-by-default

OUTPUT :
  - branch-pushed @ ⟪BRANCH-NAME⟫ on worktree ⟪WORKTREE-PATH⟫
  - commit-msg in CSLv3 ⟨§ apocky-global ; W! attestation⟩
  - PR-style summary @ end-of-turn : LOC + tests + spec-deltas-if-any

ANTI-PATTERN-AVOIDANCE :
  - AP-1 ⟨Implementer-authors-own-tests⟩ : tests-are-Validator's ; you may add SMOKE-tests only
  - AP-7 ⟨iterate-later-with-no-trigger⟩ : if-blocked → escalate ¬ TODO-and-skip
```

### §1.2 REVIEWER prompt-template

```
ROLE : Reviewer ⟪SLICE⟫
PARALLELISM : CONCURRENT-with-Implementer ⟨¬ post-hoc⟩
POD-DISCIPLINE : DIFFERENT-pod-from-Implementer ⟨bias-mitigation⟩
SPEC-ANCHOR : _drafts/phase_j/05_l0_l1_error_log_spec.md § ⟪SPEC-SECTION⟫
INPUT : Implementer-WIP-branch + spec ⟨no other context⟩

DUTIES :
  - read every file Implementer touches ; line-by-line
  - flag : naming-drift ; missing-From-impl ; severity-mis-classification
  - flag : doc-comment-missing on pub-items ; clippy-lint-warning ignored
  - flag : test-thinness ⟨e.g., one-happy-path no-edge-case⟩
  - suggest : clearer-naming ; structural-refactor ; doc-comment-text
  - suggested-patches : as-COMMENTS ¬ as-commits ⟨lane-discipline⟩

AUTHORITY :
  - REQUEST-iteration ⟨polite ; specific ; cite-spec-line⟩
  - ESCALATE-to-Architect ⟨if Implementer-disagrees + spec-ambiguous⟩
  - veto-power : ∅ ⟨cannot-block-merge ; can-only-recommend⟩

OUTPUT :
  - inline-comments + summary-comment on Implementer's branch
  - severity-rating : ⟨BLOCKER | NIT | KUDOS⟩ per comment
  - end-of-turn : "Reviewer-OK" ⟨if-clean⟩ OR "Reviewer-iterate" ⟨if-pending⟩

ANTI-PATTERN-AVOIDANCE :
  - AP-3 ⟨Reviewer-from-same-pod⟩ : confirm pod-identity-differs ¬ accept-otherwise
  - AP-4 ⟨praise-instead-of-criticize⟩ : you are NOT a Critic ; flag issues ¬ rubber-stamp
```

### §1.3 CRITIC prompt-template

```
ROLE : Critic (red-team) ⟪SLICE⟫
TIMING : POST-Implementer-self-test ⟨after Implementer says "done" ; before Validator⟩
POD-DISCIPLINE : DIFFERENT-pod-from-Implementer + DIFFERENT-pod-from-Reviewer
SPEC-ANCHOR : _drafts/phase_j/05_l0_l1_error_log_spec.md § ⟪SPEC-SECTION⟫ + § 7 LANDMINES
INPUT : Implementer-branch + spec ⟨adversarial-frame ; assume-bugs-exist⟩

DUTIES :
  - construct adversarial-scenarios :
    * what-if : caller-passes-malicious-input ⟨e.g., 16MB error-msg ; embedded-path⟩
    * what-if : panic-during-panic ⟨reentrancy⟩
    * what-if : ring-full + audit-chain-write-fails ⟨partial-state⟩
    * what-if : From-impl-cycle-via-future-crate ⟨API-stability⟩
    * what-if : Severity-classification-wrong-for-PD-adjacent-variant
  - probe : tests-cover-failure-paths ¬ just-happy-paths
  - probe : spec-§7 LANDMINES addressed ⟨D130 + replay-determinism + PD-halt-uninterceptable⟩
  - probe : semver-stability of pub-types ⟨enum-variants closed-set?⟩

AUTHORITY :
  - veto-power : YES on severity-HIGH findings ⟨blocks-merge⟩
  - veto-power : NO on stylistic + nit ⟨those go to Reviewer⟩
  - escalation : Architect on veto-disagree

OUTPUT :
  - adversarial-scenario-list ⟨~5-10 scenarios⟩ + per-scenario verdict ⟨addressed | gap | needs-test⟩
  - veto-list ⟨if-any⟩ with cite-spec-line + cite-code-line
  - end-of-turn : "Critic-cleared" OR "Critic-veto ⟨reasons⟩"

ANTI-PATTERN-AVOIDANCE :
  - AP-4 ⟨Critic-praises⟩ : silent-on-clean-code is OK ; HIGH-severity-veto when-warranted is REQUIRED
  - N! lower-severity-to-keep-pace ⟨bias toward STRICT⟩
```

### §1.4 VALIDATOR prompt-template

```
ROLE : Validator + Test-Author-collapsed ⟪SLICE⟫
TIMING : POST-Critic-cleared ⟨gate before merge⟩
POD-DISCIPLINE : DIFFERENT-pod-from-Implementer ⟨may-share-pod-with-Reviewer-if-staffing-tight⟩
SPEC-ANCHOR : _drafts/phase_j/05_l0_l1_error_log_spec.md § ⟪SPEC-SECTION⟫ + § ⟪TEST-SECTION⟫
INPUT : spec ONLY ⟨N! Implementer-code ; N! Implementer-hints⟩ for test-authoring
INPUT : spec + Implementer-branch for spec-conformance-cross-reference

DUTIES :
  PHASE-A ⟨from-spec-test-authoring⟩ :
    - read spec § ⟪TEST-SECTION⟫
    - author tests purely from spec ⟨~⟪TESTS⟫ tests⟩
    - test-categories per spec § ⟪TEST-SECTION⟫ ⟨e.g., From-impl exhaustive ; severity ; etc.⟩
    - tests must FAIL on spec-violation ⟨not just exercise-happy-path⟩
    - tests must PASS on spec-conforming-implementation ⟨Implementer's must pass⟩

  PHASE-B ⟨spec-conformance-cross-reference⟩ :
    - read every spec-line in § ⟪SPEC-SECTION⟫
    - read every Implementer-line ; map spec→code
    - flag spec-drift ⟨§ apocky AP-5⟩
    - flag implementation-extra ⟨code-without-spec-anchor ⟶ spec-update OR remove⟩

AUTHORITY :
  - merge-gate : YES ⟨"Validator-OK" required for 5-of-5⟩
  - escalation : Architect on spec-ambiguity ; Spec-Steward on spec-update-needed

OUTPUT :
  - tests-from-spec PR'd into Implementer-branch
  - spec-conformance-table : spec-§ × code-file × ⟨✓ ◐ ✗⟩
  - end-of-turn : "Validator-OK" OR "Validator-iterate ⟨reasons⟩"

ANTI-PATTERN-AVOIDANCE :
  - AP-5 ⟨skim-skim-OK⟩ : line-by-line cross-reference REQUIRED
  - AP-6 ⟨test-author-asks-Implementer⟩ : N! Implementer-input @ Phase-A ⟨spec-only⟩
```

---

## §2 SLICE Jε-1 — `cssl-error`

### §2.1 Slice metadata

```csl
§ Jε-1
  slice-id           : Jε-1
  decision-id        : T11-D??? ⟨assigned-at-dispatch ; per Phase-J slice-ID range⟩
  spec-anchor        : 05_l0_l1_error_log_spec.md § 1 + § 6.1
  crate              : cssl-error ⟨NEW⟩
  path               : compiler-rs/crates/cssl-error/
  worktree           : .claude/worktrees/Je-1
  branch             : cssl/session-12/T11-D???-cssl-error
  LOC target         : ~3,000 ⟨spec-§6.1 baseline ~2,000 + 50% buffer for tests-from-spec⟩
  test target        : ~120
  parallel-with      : Jε-2 ⟨independent ; no shared mutable state⟩
  blocks             : Jε-3 + Jε-4 + Wave-Jζ
```

### §2.2 API surface ⟨~10 bullets ; spec § 1⟩

- `pub enum EngineError` — closed-set From<T> aggregator over per-crate `*Error` types ⟨§ 1.2⟩
- `pub enum Severity { Trace, Debug, Info, Warning, Error, Fatal }` ⟨§ 1.3⟩
- `pub trait Severable { fn severity(&self) -> Severity; }` — impl'd on EngineError + per-crate-errors
- `pub struct ErrorContext { source: SourceLocation, frame_n, stage_id, path_hash, ... }` ⟨§ 1.4⟩
- `pub struct SourceLocation { file_hash: u64, line: u32, col: u32 }` — file-hash ¬ raw-path ⟨D130⟩
- `pub struct StackTrace` — capture via `backtrace` crate ; debug-symbols-only-in-debug-builds ⟨§ 1.5⟩
- `pub struct ErrorFingerprint(blake3::Hash)` + `pub fn fingerprint(&Error) -> ErrorFingerprint` ⟨§ 1.6⟩
- `pub struct PanicReport { fingerprint, stack, severity, location }` ⟨§ 1.8⟩
- `pub fn install_panic_hook()` — registers hook ; idempotent ; honors PD-halt ⟨§ 1.8⟩
- `pub struct PrimeDirectiveViolation` + `From<PrimeDirectiveViolation> for EngineError` — ALWAYS Fatal + halt ⟨§ 1.3 + § 7.3⟩

### §2.3 Module decomposition ⟨per spec § 6.1⟩

```
src/
  lib.rs          ⟨public surface ; re-exports ; ~150 LOC⟩
  error.rs        ⟨EngineError + From-impls ; ~600 LOC⟩
  severity.rs     ⟨Severity + Severable ; ~150 LOC⟩
  context.rs      ⟨ErrorContext + SourceLocation + builder ; ~200 LOC⟩
  stack.rs        ⟨StackTrace + capture ; ~250 LOC⟩
  fingerprint.rs  ⟨ErrorFingerprint + dedup ; ~200 LOC⟩
  panic.rs        ⟨panic-hook + PanicReport ; ~400 LOC⟩
  pd.rs           ⟨PrimeDirectiveViolation + halt-bridge ; ~200 LOC⟩
tests/
  from_impl_exhaustive.rs   ⟨~10⟩
  severity_classification.rs ⟨~10⟩
  context_path_hash_only.rs ⟨~10⟩
  stack_capture.rs          ⟨~10⟩
  fingerprint_dedup.rs      ⟨~10⟩
  panic_hook_integration.rs ⟨~15⟩
  pd_violation_halt.rs      ⟨~10⟩
  closed_enum_lint.rs       ⟨~5⟩
  ⟨buffer for spec-driven extras ; ~40⟩
```

### §2.4 Acceptance ⟨5-of-5⟩

```csl
§ ACCEPT.Jε-1
  G1  Implementer-self-test : cargo test -p cssl-error PASS @ debug + release
  G2  Reviewer-OK            : different-pod ; line-by-line ; "Reviewer-OK" or-iterate-resolved
  G3  Critic-cleared         : adversarial-list addressed ; HIGH-severity vetoes resolved
  G4  Validator-OK           : tests-from-spec authored ; spec-conformance-table green
  G5  workspace-build         : `cargo build --workspace` PASS ; ¬ regress other-crate
  pre-merge-gate : G1 ∧ G2 ∧ G3 ∧ G4 ∧ G5
  out-of-band-blockers : (a) PD-violation in panic-hook ⟶ halt-bridge missing ⟶ HARD-FAIL ;
                          (b) `unsafe` introduced ⟶ Architect-review REQUIRED ;
                          (c) D130 raw-path leak ⟶ HARD-FAIL
```

### §2.5 Pod-roster ⟨fill-in @ dispatch⟩

- Implementer @ Pod-α ⟨Je-1-impl⟩
- Reviewer @ Pod-β ⟨Je-1-rev⟩
- Critic @ Pod-γ ⟨Je-1-crit⟩
- Validator @ Pod-δ ⟨Je-1-val⟩

⟨W! pod-α ≠ pod-β ≠ pod-γ ≠ pod-δ for Je-1 ; bias-mitigation discipline⟩

---

## §3 SLICE Jε-2 — `cssl-log`

### §3.1 Slice metadata

```csl
§ Jε-2
  slice-id           : Jε-2
  decision-id        : T11-D??? ⟨assigned-at-dispatch⟩
  spec-anchor        : 05_l0_l1_error_log_spec.md § 2 + § 6.2
  crate              : cssl-log ⟨NEW⟩
  path               : compiler-rs/crates/cssl-log/
  worktree           : .claude/worktrees/Je-2
  branch             : cssl/session-12/T11-D???-cssl-log
  LOC target         : ~3,000 ⟨spec-§6.2 baseline ~2,500 + 20% buffer⟩
  test target        : ~130
  parallel-with      : Jε-1 ⟨independent ; loose-coupling @ Severity-import only⟩
  blocks             : Jε-3 + Jε-4 + Wave-Jζ + Wave-Jθ
```

### §3.2 API surface ⟨~10 bullets ; spec § 2⟩

- `cssl_log::trace! / debug! / info! / warn! / error! / fatal!` — decl-macros ⟨§ 2.2⟩
- `cssl_log::log!(level=Severity::X, subsystem=Tag, ...)` — generic-form ⟨§ 2.2⟩
- `pub fn enabled(Severity, SubsystemTag) -> bool` — AtomicU64 bitfield ; ≈2ns disabled-cost ⟨§ 2.3⟩
- `pub fn emit_structured(&Context, format_args, fields)` — canonical-entry ; ¬ double-log ⟨§ 2.4⟩
- `pub struct Context { severity, subsystem, source, frame_n }` ⟨§ 2.3⟩
- `pub trait LogSink { fn write(&self, &LogRecord); }` + impls ⟨§ 2.6⟩
- `pub struct RingSink + StderrSink + FileSink + McpSink + AuditSink` ⟨§ 2.6⟩
- `pub enum Format { JsonLines, CslGlyph, Binary }` ⟨§ 2.9⟩
- `pub enum SubsystemTag { Render, Wave, Anim, Work, Gaze, Codegen, Asset, Effects, Telemetry, Audit, Engine, AI, Mcp, ... }` ⟨§ 2.7⟩
- `pub fn set_replay_strict(bool)` — flips ring-emit-noop discipline ⟨§ 2.4 replay-determinism⟩

### §3.3 Module decomposition ⟨per spec § 6.2⟩

```
src/
  lib.rs          ⟨public surface + macros ; ~400 LOC⟩
  macros.rs       ⟨log/trace/debug/info/warn/error/fatal ; ~300 LOC⟩
  context.rs      ⟨Context + frame-tracker ; ~150 LOC⟩
  sink.rs         ⟨LogSink trait + RingSink ; ~200 LOC⟩
  sink_stderr.rs  ⟨StderrSink ; ~150 LOC⟩
  sink_file.rs    ⟨FileSink ; cap-gated ; ~250 LOC⟩
  sink_mcp.rs     ⟨McpSink ; cap-gated ; ~150 LOC⟩
  sink_audit.rs   ⟨AuditSink ; ~200 LOC⟩
  sample.rs       ⟨sampling + rate-limit ; ~250 LOC⟩
  format.rs       ⟨JsonLines / CslGlyph / Binary ; ~250 LOC⟩
  subsystem.rs    ⟨SubsystemTag enum + helpers ; ~150 LOC⟩
tests/
  macro_expansion.rs           ⟨~15⟩
  sink_routing_matrix.rs       ⟨~15⟩
  sampling_rate_limit.rs       ⟨~15⟩
  format_round_trip.rs         ⟨~10⟩
  path_hash_field_sanitize.rs  ⟨~15⟩
  replay_determinism.rs        ⟨~10⟩
  subsystem_catalog_stable.rs  ⟨~10⟩
  effect_row_gating.rs         ⟨~10⟩
  ⟨buffer for spec-driven extras ; ~30⟩
```

### §3.4 Acceptance ⟨5-of-5⟩

```csl
§ ACCEPT.Jε-2
  G1  Implementer-self-test : cargo test -p cssl-log PASS @ debug + release
  G2  Reviewer-OK            : different-pod ; line-by-line ; "Reviewer-OK"
  G3  Critic-cleared         : adversarial-list addressed ; HIGH-severity vetoes resolved
  G4  Validator-OK           : tests-from-spec authored ; spec-conformance-table green
  G5  workspace-build         : `cargo build --workspace` PASS ; ¬ regress other-crate
  pre-merge-gate : G1 ∧ G2 ∧ G3 ∧ G4 ∧ G5
  out-of-band-blockers : (a) double-log path detected ⟶ HARD-FAIL ⟨§ 2.4 N! double-log⟩ ;
                          (b) replay-determinism break ⟶ HARD-FAIL ⟨§ 7.2⟩ ;
                          (c) D130 raw-path in any field ⟶ HARD-FAIL ⟨§ 2.8⟩ ;
                          (d) Severity-import drifts from Jε-1 ⟶ Architect-reconcile
```

### §3.5 Pod-roster ⟨fill-in @ dispatch⟩

- Implementer @ Pod-ε ⟨Je-2-impl⟩
- Reviewer @ Pod-ζ ⟨Je-2-rev⟩
- Critic @ Pod-η ⟨Je-2-crit⟩
- Validator @ Pod-θ ⟨Je-2-val⟩

⟨W! pod-ε ≠ pod-ζ ≠ pod-η ≠ pod-θ for Je-2 ; bias-mitigation⟩
⟨W! Jε-1 pods ⊥ Jε-2 pods overlap-OK across-slices ⟨e.g., Pod-α @ Jε-1 may ≠ Pod-ε @ Jε-2 ; not-required⟩⟩

---

## §4 DISPATCH ORDER + PRE-MERGE GATE

```csl
§ DISPATCH-ORDER
  T0  : both Jε-1 + Jε-2 dispatched in PARALLEL ⟨independent⟩
  T1  : Jε-1 Implementer + Reviewer concurrent ⟨~30 min⟩
  T1  : Jε-2 Implementer + Reviewer concurrent ⟨~30 min⟩
  T2  : Jε-1 Critic post-Implementer-self-test ⟨~10 min⟩
  T2  : Jε-2 Critic post-Implementer-self-test ⟨~10 min⟩
  T3  : Jε-1 Validator post-Critic-cleared ⟨~20 min⟩
  T3  : Jε-2 Validator post-Critic-cleared ⟨~20 min⟩
  T4  : pre-merge-gate per slice ⟨5-of-5⟩
  T5  : merge-to-trunk ⟨Jε-1 + Jε-2 in either-order ; both-must-be-clean⟩

§ PRE-MERGE-GATE per slice :
  ✓ Implementer-self-test PASS
  ✓ Reviewer "Reviewer-OK"
  ✓ Critic "Critic-cleared"
  ✓ Validator "Validator-OK"
  ✓ workspace-build PASS @ that-slice-branch

§ POST-MERGE :
  → Architect updates SESSION_12_DISPATCH_PLAN.md with Jε-1 + Jε-2 done-marker
  → Spec-Steward closes any spec-deltas as DECISIONS entries
  → Wave-Jε-3 + Jε-4 unblocked for next dispatch wave
```

---

## §5 §11 ATTESTATION (PRIME_DIRECTIVE.md §11 CREATOR-ATTESTATION)

```csl
§ ATTEST.W-Jε-prompts
  authored-by      : Apocky+Claude ⟨W-Jε-prompts agent⟩
  date             : 2026-04-29
  status           : ◐ DRAFT pre-stage ⟨no-commit ; awaits-Jβ-merge-then-dispatch⟩
  PD-alignment     : ✓ § 1 PROHIBITIONS ⟨no-harm ; no-coercion⟩
                     ✓ § 2 COGNITIVE-INTEGRITY ⟨bias-mitigation pod-discipline⟩
                     ✓ § 4 TRANSPARENCY ⟨§R block ; visible-reasoning⟩
                     ✓ § 11 CREATOR-ATTESTATION ⟨this block⟩
  spec-fidelity    : prompts derive ONLY from 05_l0_l1_error_log_spec.md § 1-2 + § 6.1-6.2
                     + 02_reviewer_critic_validator_test_author_roles.md § 1-4
  scope-discipline : Jε-1 + Jε-2 only ⟨Jε-3 + Jε-4 explicitly OUT-OF-SCOPE⟩
  watchdog-note    : skeleton-first-write methodology used ⟨recovery from prior BG-3 stall⟩
  ¬ commit         : pre-staging only per dispatch-spec
```

§ Dispatch-checklist ⟨pre-flight before Jβ-merge-trigger⟩ :
- [ ] Jβ merged ⟨spec-stable⟩
- [ ] T11-D??? slice-IDs assigned for Jε-1 + Jε-2
- [ ] worktrees `.claude/worktrees/Je-1` + `.claude/worktrees/Je-2` created
- [ ] branches `cssl/session-12/T11-D???-cssl-error` + `cssl/session-12/T11-D???-cssl-log` opened
- [ ] pod-rosters populated ⟨§ 2.5 + § 3.5⟩ ; pod-discipline confirmed ⟨all-distinct⟩
- [ ] T0 parallel-dispatch fired

§ END-OF-DOC.
