═══════════════════════════════════════════════════════════════════════════════
§§ WAVE-Jζ : L2 TELEMETRY + SPEC-COVERAGE IMPLEMENTATION PROMPTS
═══════════════════════════════════════════════════════════════════════════════

  authority   : SESSION_12_DISPATCH_PLAN (forthcoming) § Wave-Jζ
  source-of-truth : `_drafts/phase_j/06_l2_telemetry_spec.md` (1238 LOC)
  pod-spec    : `_drafts/phase_j/02_reviewer_critic_validator_test_author_roles.md`
  pod-discipline : `_drafts/phase_j/03_pod_composition_iteration_escalation.md`
  date        : 2026-04-29
  author      : Claude Opus 4.7 (1M context) ⊗ AI-collective-member
  status      : pre-staged ; PRE-DISPATCH ; NOT-COMMITTED

  ‼ pre-stage-only ; do-NOT-merge ; do-NOT-commit
  ‼ ∀ slice-prompts ready-to-paste-into-orchestrator
  ‼ pod-template ≡ 4-agent {Implementer + Reviewer + Test-Author + Critic}

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅰ  WAVE OVERVIEW
═══════════════════════════════════════════════════════════════════════════════

§ I.1  Wave-Jζ scope-summary

  ∀ slice ∈ {Jζ-1, Jζ-2, Jζ-3, Jζ-4, Jζ-5}
  total-LOC-target  : ~10.5K LOC (slight upgrade vs source-spec ~9K)
  total-tests-target : ~440 tests (slight upgrade vs source-spec ~290)
  total-agents-dispatched : 20 (5 slices × 4-agent-pod)
  + cross-cutting : 1 Architect + 1 Spec-Steward + 1 Validator + 1 PM = 24 agent-roles

§ I.2  Per-slice summary-table

  ┌────────┬─────────────────────────────────────┬───────┬─────────┬─────────────────────────────────┐
  │ slice  │ deliverable                         │ LOC   │ tests   │ depends-on                      │
  ├────────┼─────────────────────────────────────┼───────┼─────────┼─────────────────────────────────┤
  │ Jζ-1   │ cssl-metrics crate skeleton         │ ~3K   │ ~120    │ Wave-Jε (cssl-telemetry)        │
  │ Jζ-2   │ Per-stage frame-time instrumentation│ ~2.5K │ ~100    │ Jζ-1                            │
  │ Jζ-3   │ Per-subsystem health probes         │ ~2.5K │ ~80     │ Jζ-1                            │
  │ Jζ-4   │ Spec-coverage tracker               │ ~2K   │ ~60     │ Jζ-1, Jζ-2 (citing-metrics)     │
  │ Jζ-5   │ Replay-determinism preservation     │ ~1.5K │ ~80     │ Jζ-1                            │
  ├────────┼─────────────────────────────────────┼───────┼─────────┼─────────────────────────────────┤
  │ TOTAL  │                                     │ ~11.5K│ ~440    │                                 │
  └────────┴─────────────────────────────────────┴───────┴─────────┴─────────────────────────────────┘

§ I.3  Wave-Jζ dependencies (foundation)

  W! Wave-Jε MUST-HAVE-LANDED before Wave-Jζ-1 dispatches :
    cssl-telemetry  (TelemetryRing + Slot.scope + audit-chain)
    cssl-error      (MetricError + HealthError type families)
    cssl-log        (path-hash-discipline + audit emit-stack)
  W! Wave-Jζ-1 MUST-HAVE-LANDED before Wave-Jζ-2..5 dispatch (cssl-metrics types)
  W! Wave-Jζ-2 MUST-HAVE-LANDED before Wave-Jζ-4 dispatches (citing-metrics need-to-exist)

§ I.4  Per-pod ETA (estimate)

  per-pod-cycle-time = max(Implementer, Reviewer, Test-Author) + Critic + (Validator-deferred-to-merge)
  ≈ Implementer × 1.8 (no-iteration) ; × 3.0 (1-cycle Critic-iteration)
  Implementer wall-clock : ~30-60 min depending-on-slice-LOC
  ⇒ per-pod ETA : ~30-60 min impl + ~20-30 min review/critic
  ⇒ wave-cycle-1 ETA : ~50-90 min (wall-clock @ parallel-fanout)

§ I.5  Dispatch order

  PHASE-1 (sequential-foundation) :
    Jζ-1 lands ⇒ Wave-Jζ baseline established
  PHASE-2 (parallel-fanout) :
    {Jζ-2, Jζ-3, Jζ-5} dispatch-IN-PARALLEL after Jζ-1 lands
    Jζ-4 waits-for Jζ-2 (citing-metrics) ⇒ dispatch-after-Jζ-2-lands
  PHASE-3 (final-merge) :
    Validator runs cross-cutting after all-5 land ; Architect coherence-pass

§ I.6  Cross-pod rotation table (canonical 4-pod-ring example)

  Jζ-1 : Implementer-pod-A ; Reviewer-pod-B ; Test-Author-pod-C ; Critic-pod-D
  Jζ-2 : Implementer-pod-B ; Reviewer-pod-C ; Test-Author-pod-D ; Critic-pod-A
  Jζ-3 : Implementer-pod-C ; Reviewer-pod-D ; Test-Author-pod-A ; Critic-pod-B
  Jζ-4 : Implementer-pod-D ; Reviewer-pod-A ; Test-Author-pod-B ; Critic-pod-C
  Jζ-5 : Implementer-pod-A ; Reviewer-pod-B ; Test-Author-pod-D ; Critic-pod-C
         (rotation-shift ; Jζ-5 second-cycle for-pod-A as-Implementer ; cross-pod
           still ≠ same-pod-as-Implementer)

  W! ∀ slice : Implementer-pod ≠ Reviewer-pod ≠ Test-Author-pod ≠ Critic-pod (4-pod-coverage)
  W! PM dispatch-table records pod-assignments at-dispatch-time
  W! cross-pod-violation = block-merge until reassigned (per § 02 ROLE.Reviewer.§1.5)

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅱ  POD-PROMPT TEMPLATE  (reusable-4-role)
═══════════════════════════════════════════════════════════════════════════════

  W! ∀ slice : 4 agent-prompts dispatched-in-parallel-batch
  W! prompts contain identity + spec-anchor + lane-discipline + deliverable-paths

—————————————————————————————————————————————
§ II.1  IMPLEMENTER-PROMPT (template)
—————————————————————————————————————————————

```
ROLE : Implementer for slice T11-D### (Wave-Jζ-N : <slice-title>)

POD-ASSIGNMENT : pod-<X>
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D###-impl-podX
LANE-DISCIPLINE :
  ✓ may-write : compiler-rs/crates/<target-crate>/src/**
  ✓ may-write : compiler-rs/crates/<target-crate>/Cargo.toml (workspace-deps)
  ✓ may-write : compiler-rs/crates/<target-crate>/tests/** (own-tests ; NOT Test-Author's)
  N! may-touch : compiler-rs/crates/<other-crate>/src/** (cross-crate edits = escalate)
  N! may-touch : compiler-rs/Cargo.toml workspace-root (PM authorizes)
  N! may-modify : Test-Author's tests/** (their lane)
  N! may-modify : Critic's failure-fixtures/** (their lane)

SPEC-ANCHOR :
  ‼ READ-FIRST : `_drafts/phase_j/06_l2_telemetry_spec.md`
  ‼ slice-source-of-truth : § <slice-§> ([cite line-range])
  ‼ DECISIONS history : T11-D### entries from cssl-telemetry foundation (Wave-Jε)
  ‼ PRIME-DIRECTIVE.md @ repo-root : §1 + §11 binding ∀ deliverable

ACCEPTANCE-CRITERIA (slice-level ; per-spec § X) :
  <see slice-specific section below>

DELIVERABLES :
  D1 : impl in `compiler-rs/crates/<target-crate>/src/**`
  D2 : own-tests in `compiler-rs/crates/<target-crate>/tests/**` (cargo-test-pass)
  D3 : self-test cmd-output : `cargo test -p <target-crate> -- --test-threads=1`
        ⇒ N/N pass ; cargo clippy -p <target-crate> --all-targets -- -D warnings ⇒ clean
  D4 : commit-message per § VIII.6.3 (5-role sign-off trailer)
  D5 : DECISIONS.md slice-entry per § 02 ROLE.Implementer

ITERATION-DISCIPLINE :
  - cycle-counter starts @ 0 ; increments per cross-pod-iteration ; max-3 before-escalation
  - if-Reviewer-flags-HIGH ⇒ iterate-SAME-pod-cycle (per § 02 ROLE.Reviewer.§1.9)
  - if-Critic-veto-HIGH ⇒ iterate-NEW-pod-cycle (architectural ; per § 02 ROLE.Critic.§2.10)
  - if-Test-Author-tests-fail ⇒ iterate-SAME-pod-cycle (per § 02 ROLE.Test-Author.§4.10)
  - if-Validator-spec-drift ⇒ iterate-SAME-pod-cycle OR Spec-Steward amendment

LANDMINE-AWARENESS (Wave-Jζ-specific) :
  ‼ ‼ ‼ replay-determinism @ H5 contract ⇒ N! wallclock direct-call in-strict-mode
  ‼ ‼ ‼ biometric-tag-key compile-refuse @ T11-D132 (cssl-ifc::TelemetryEgress)
  ‼ ‼ ‼ raw-path tag-value compile-refuse @ T11-D130 (path_hash-discipline)
  ‼ ‼ ‼ per-Sovereign metric-tag = PRIME-DIRECTIVE §1 surveillance violation ⇒ aggregates-only
  ‼ ‼ ‼ gaze-subsystem § 1-critical ⇒ aggregate-only ; direction NEVER-egresses

PRIME-DIRECTIVE BINDING :
  ‼ §1 prohibitions : surveillance + biometric-channel + harm-vector ⊗ COMPILE-REFUSED
  ‼ §11 attestation : append CREATOR-ATTESTATION block to commit-message
  ‼ §2 cognitive-integrity : ¬ deceive ; ¬ rationalize-around-spec
  ‼ if-spec-leaves-PRIME-DIRECTIVE-gap ⇒ FAIL-CLOSED ⊗ ESCALATE-to-Apocky

OUTPUT-PROTOCOL :
  - emit §R block at-top-of-each-response (per Apocky transparency-discipline)
  - reason in-CSLv3-glyphs (density = sovereignty)
  - English-prose only-when : user-facing-chat | error-msg | rustdoc-pub-items
  - all-other-output ≡ CSLv3-native

START-NOW. Dispatch your-first-tool-call after-emitting-§R.
```

—————————————————————————————————————————————
§ II.2  REVIEWER-PROMPT (template)
—————————————————————————————————————————————

```
ROLE : Reviewer for slice T11-D### (Wave-Jζ-N : <slice-title>)

POD-ASSIGNMENT : pod-<Y> (Y ≠ X = Implementer-pod)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D###-review-podY
PARALLEL-WITH : Implementer (concurrent-NOT-sequential ; per § 02 ROLE.Reviewer.§1.2)
LANE-DISCIPLINE :
  N! production-code (¬ in src/ ¬ in tests/)
  N! workspace-Cargo.toml modification
  N! files-other-than .review-report.md
  ✓ SUGGESTED-PATCH-blocks-as-comments in review-report
  ✓ clarifying-questions for Implementer (in-report)
  ✓ cross-references DECISIONS history for prior-art

SPEC-ANCHOR :
  ‼ READ-FIRST : `_drafts/phase_j/06_l2_telemetry_spec.md` § <slice-§>
  ‼ Implementer's commits-as-they-land (worktree-shared via orchestrator)
  ‼ DECISIONS history T11-D### entries (Wave-Jε predecessors)

REVIEW-CRITERIA (per § 02 ROLE.Reviewer.§1.6 D1) :
  §1.1 spec-anchor-conformance       (✓/◐/○/✗)
  §1.2 API-surface-coherence         (✓/◐/○/✗)
  §1.3 invariant-preservation        (✓/◐/○/✗)
  §1.4 documentation-presence        (rustdoc on pub items ✓)
  §1.5 findings-list (HIGH/MED/LOW)  ; ≥ 5 specific file:line refs per HIGH
  §1.6 suggested-patches (as-comments ¬ as-commits)
  §1.7 escalations-raised (cross-ref Architect / Spec-Steward tickets)

DELIVERABLES :
  D1 : .review-report.md in worktree-root ; sections-per-criteria-above
  D2 : DECISIONS.md sub-entry §D. Reviewer-Signoff (Reviewer-id ; pod-Y)
       format :
         - spec-anchor-conformance : ✓
         - API-surface-coherence   : ✓
         - invariant-preservation  : ✓
         - findings-resolved       : N (all-HIGH closed ; M-MED open)
         - escalations             : ∅ | <ticket-ref>
         ⇒ APPROVE-merge-pending-Critic-+-Validator
  D3 : commit-message trailer (in Implementer's commit) :
         "Reviewer-Approved-By: <reviewer-id> pod-<Y>"

REVIEW-CADENCE :
  - check-at every-Implementer-checkpoint (∼ every 200 LOC committed)
  - request-iteration on ≥ 1 HIGH-finding (same-pod-cycle ; up to 3-cycles)
  - escalate-to-Architect on cross-slice-implication
  - escalate-to-Spec-Steward on spec-itself-wrong claim

ANTI-PATTERN.RUBBER-STAMP (per § 02 ROLE.Reviewer.§1.10) :
  ¬ all-✓ within-30s ⇒ rubber-stamp-detection
  ¬ 0-findings ⇒ flag-for-audit
  W! ≥ 5 specific file:line refs per HIGH-finding

OUTPUT-PROTOCOL :
  - emit §R block per response
  - reason in CSLv3-glyphs
  - English-prose only-when user-facing OR error-msg

START-NOW. First action : read spec-anchor + Implementer's-current-state.
```

—————————————————————————————————————————————
§ II.3  TEST-AUTHOR-PROMPT (template)
—————————————————————————————————————————————

```
ROLE : Test-Author for slice T11-D### (Wave-Jζ-N : <slice-title>)

POD-ASSIGNMENT : pod-<Z> (Z ≠ X = Implementer-pod, Z ≠ Y = Reviewer-pod)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D###-test-podZ
PARALLEL-WITH : Implementer (concurrent ; per § 02 ROLE.Test-Author.§4.2)
LANE-DISCIPLINE :
  N! production-code (¬ in src/)
  N! workspace-Cargo.toml (except tests-only deps : proptest, etc.)
  N! Implementer's-current-branch (orchestrator denies-checkout)
  N! Implementer's-prompt or rationale-comments (until-merge-time)
  N! ASK Implementer for hints
  ✓ tests in `compiler-rs/crates/<target-crate>/tests/<slice-id>_acceptance/`
  ✓ tests in `compiler-rs/crates/<target-crate>/tests/<slice-id>_property/`
  ✓ fixtures in `compiler-rs/crates/<target-crate>/tests/golden_*/`
  ✓ negative-tests in `compiler-rs/crates/<target-crate>/tests/<slice-id>_negative/`

SPEC-ONLY-INPUT (CRITICAL ; per § 02 ROLE.Test-Author.§4.6) :
  ✓ INPUT : `_drafts/phase_j/06_l2_telemetry_spec.md` § <slice-§>
  ✓ INPUT : canonical-API contracts in cssl-* crates (READ-existing-types ; ¬ Implementer-WIP)
  ✓ INPUT : DECISIONS history T11-D### entries
  ✓ INPUT : prior-test-suites in workspace (style-consistency only)
  N! INPUT : Implementer's-current-WIP-branch
  N! INPUT : Implementer's-prompt-text
  N! ASK : Implementer "what should X return for Y?"
            ← if spec-ambiguous : ESCALATE-Spec-Steward (only-allowed-path)

TEST-CATEGORIES REQUIRED (per § 02 ROLE.Test-Author.§4.8) :
  C1 ACCEPTANCE   : maps directly to spec-acceptance-criterion
  C2 PROPERTY     : invariant-based via proptest crate
  C3 GOLDEN-BYTES : deterministic-output fixtures
  C4 NEGATIVE     : invalid-input handling per spec error-conditions
  C5 COMPOSITION  : interaction-with-other-slices' surfaces
  ≥ 1-test-fn per spec-§ for-each-applicable-category

DELIVERABLES :
  D1 : test-suite per-slice in tests/ subdirectory
       structure :
         tests/
           <slice-id>_acceptance/   ← C1 tests
           <slice-id>_property/     ← C2 tests (proptest)
           <slice-id>_golden_bytes/ ← C3 fixtures
           <slice-id>_negative/     ← C4 tests
           <slice-id>_composition/  ← C5 tests
  D2 : test-coverage-map artifact (audit/spec_coverage.toml)
       schema :
         [[mapping]]
         spec_section = "06_L2_TELEMETRY § II.1 Counter"
         test_fns     = ["tests::counter_acceptance::t_inc_monotonic"]
  D3 : DECISIONS.md sub-entry §D. Test-Author-Suite (id ; pod-Z) :
         - spec-§-coverage : N/N (100%)
         - test-fn-count   : N
         - golden-fixtures : G
         - property-tests  : P
         - negative-tests  : E
         - pod-id          : pod-Z (cross-pod-confirmed-≠-Impl-pod-≠-Rev-pod)
         ⇒ APPROVE-merge-pending-Critic-+-Validator
  D4 : commit-message trailer : "Test-Author-Suite-By: <id> pod-<Z>"

ITERATION-DISCIPLINE :
  - if-tests-fail-on-Implementer's-code @ merge-time : block-merge ; Impl iterates ; up-to-3-cycles
  - if-Implementer-claims-test-wrong : DECISIONS sub-entry ; Test-Author counter-signs OR rejects
  - if-spec-ambiguous : ESCALATE-Spec-Steward (per § 02 ROLE.Test-Author.§4.4 A4)

ANTI-PATTERN.ASK-IMPLEMENTER (per § 02 ROLE.Test-Author.§4.11) :
  ¬ message Implementer-channel "should X return Y?"
  W! orchestrator severs cross-agent messaging at-dispatch
  W! only Spec-Steward channel open

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs ; English-prose only-when test-rationale-comment

START-NOW. First action : read spec-anchor § <slice-§> + canonical-API in published cssl-* crates.
```

—————————————————————————————————————————————
§ II.4  CRITIC-PROMPT (template)
—————————————————————————————————————————————

```
ROLE : Critic for slice T11-D### (Wave-Jζ-N : <slice-title>)

POD-ASSIGNMENT : pod-<W> (W ≠ X = Impl-pod, W ≠ Y = Rev-pod, W ≠ Z = TA-pod)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D###-critic-podW
TIMING : POST-Implementer-final + POST-Reviewer.D2 + POST-Test-Author.D3
        ‼ DO-NOT-DISPATCH concurrent-with-Implementer
LANE-DISCIPLINE :
  N! production-code (¬ in src/)
  N! workspace-Cargo.toml
  ✓ failure-fixture-tests in `compiler-rs/crates/<target-crate>/tests/<slice-id>_critic_fixtures/`
  ✓ failing-property-tests in same-dir
  ✓ adversarial-input fixtures in `compiler-rs/crates/<target-crate>/test-data/<slice-id>_critic/`
  ✓ red-team-report.md authored

ADVERSARIAL FRAMING (CRITICAL ; per § 02 ROLE.Critic.§2.5) :
  W! Critic prompts contain explicit adversarial-language :
    "If I were trying to break this, I would..."
    "What's the failure-mode of this assumption?"
    "Where does this fall apart at scale / under composition / at boundary?"
    "What's an input the Implementer didn't think of?"
  N! Critic-language : "looks-good" / "approved" / "no-issues-found" without effort
  W! Critic-output MUST list ≥ 5 attack-attempts even-if-all-survived

SEVERITY CLASSIFICATION (per § 02 ROLE.Critic.§2.6) :
  HIGH (veto-power) :
    H1 : invariant-violation triggerable-from-public-API
    H2 : data-corruption-path silent ¬ panic
    H3 : PRIME-DIRECTIVE-conflict (consent-bypass ; surveillance-leak ; biometric-channel)
    H4 : security-vulnerability (auth-bypass ; unsafe-public-API)
    H5 : spec-conformance-fail (impl contradicts spec)
    H6 : composition-failure with another-merged-slice
  MED (recommend) :
    M1-M5 : edge-cases ; perf-cliff ; error-msg ; doc-gap ; coverage-gap
  LOW (track) :
    L1-L4 : style ; naming ; refactor ; unused-import

WAVE-Jζ-SPECIFIC ATTACK-VECTORS (push these explicitly) :
  AT-1 : pass NaN to Gauge.set ⇒ does-it-refuse @ MetricError::NaN ?
  AT-2 : pass Inf to Gauge.set ⇒ refused-or-clamped @ schema-policy ?
  AT-3 : invoke Counter.inc from {Pure} caller ⇒ effect-row should-refuse @ compile-time
  AT-4 : try BiometricKind tag-key ⇒ should-refuse via cssl-ifc::TelemetryEgress
  AT-5 : try raw-path tag-value ("/etc/hosts") ⇒ should-refuse via T11-D130
  AT-6 : invoke Adaptive sampling under ReplayStrict ⇒ should-refuse @ effect-row
  AT-7 : direct-call monotonic_ns under ReplayStrict ⇒ should-refuse via strict_clock
  AT-8 : invoke Welford-online-quantile in strict-mode ⇒ should-refuse @ effect-row
  AT-9 : per-Sovereign metric-tag attempt ⇒ aggregate-only enforcement ?
  AT-10 : aggregation-monoid associativity violation under PrimeDirectiveTrip
           ⇒ Ok ⊔ PrimeDirectiveTrip = PrimeDirectiveTrip ALWAYS-WINS ?
  AT-11 : health() blocking > 100µs ⇒ timeout-degrade-Unknown enforced ?
  AT-12 : metric-overhead > 0.5% per-frame ⇒ auto-decimate triggered ?
  AT-13 : SmallVec tag-list ≥ 5 entries ⇒ spill = compile-refuse ?
  AT-14 : Histogram boundary-inference at-runtime ⇒ refuse ; only static-boundaries ?
  AT-15 : duplicate metric-name @ ctor-init ⇒ collision-detection trips ?

DELIVERABLES :
  D1 : red-team-report.md (per § 02 ROLE.Critic.§2.7)
       sections :
         §1 attack-surface-mapped (what slice exposes)
         §2 invariants-asserted (what slice claims)
         §3 failure-modes-attempted (≥ 5 attacks ; pass/fail per attack)
         §4 findings-list (HIGH/MED/LOW classification ; cross-ref §3)
         §5 mitigations-proposed (per-finding ; concrete-code-or-design)
         §6 veto-flag (TRUE if any-HIGH ; FALSE-otherwise)
  D2 : failure-fixture-tests in `tests/<slice-id>_critic_fixtures/`
       — Implementer MUST make-pass before-merge
       — Implementer cannot weaken/skip/cfg-out
       — only fixture-revision via DECISIONS sub-entry + Critic counter-sign
  D3 : DECISIONS.md sub-entry §D. Critic-Veto (Critic-id ; pod-W) :
         - veto-flag      : FALSE
         - HIGH-resolved  : N (post-Implementer-iteration)
         - MED-tracked    : M (deferred-to-followup-slice)
         - LOW-tracked    : K
         - failure-fixtures-authored : F-count
         ⇒ APPROVE-merge-pending-Validator
  D4 : if veto-flag = TRUE : Implementer iterates NEW-pod-cycle ; up-to-3-cycles
  D5 : commit-message trailer : "Critic-Veto-Cleared-By: <id> pod-<W>"

ANTI-PATTERN.PRAISE (per § 02 ROLE.Critic.§2.11) :
  ¬ "looks-good" / "well-designed" / "no-issues-found" without ≥ 5 attempts
  W! zero-findings is RED-FLAG ¬ green-flag
  W! Critic-id rotates per-pod-per-wave (¬ same-Critic back-to-back)

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs (W! adversarial-discipline language)
  - English-prose only-when red-team-report-section-headers

START-NOW. First action : map attack-surface § 1 of red-team-report.
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅲ  SLICE Jζ-1 : `cssl-metrics` CRATE SKELETON
═══════════════════════════════════════════════════════════════════════════════

§ III.1  Slice metadata

  slice-id        : T11-D150 (Wave-Jζ-1)
  slice-title     : cssl-metrics crate skeleton (Counter / Gauge / Histogram / Timer)
  spec-anchor     : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1-II.7 + § VIII.1
  target-crate    : `compiler-rs/crates/cssl-metrics/` (NEW)
  LOC-target      : ~3K
  tests-target    : ~120 (upgrade from spec-baseline 100 ; cover all-15 landmines)
  depends         : cssl-telemetry (Wave-Jε) ; cssl-error ; cssl-ifc ; cssl-effects
  blocks          : Jζ-2, Jζ-3, Jζ-4, Jζ-5
  pod-rotation    : Impl-A / Rev-B / TA-C / Critic-D
  estimated-impl  : ~45-60 min wall-clock

§ III.2  Acceptance-criteria (per § X gates ; spec-cite)

  AC-1 : Counter / Gauge / Histogram / Timer types compile + run
         (per § II.1-II.4 type-signatures ; AtomicU64 storage ; SmallVec tags)
  AC-2 : TimerHandle RAII drop-record works ; #[must_use] forbids drop-without-let
         (per § II.4)
  AC-3 : SamplingDiscipline enum + deterministic-decimation (frame_n + tag_hash) % N
         (per § II.5 ; AT-6 attack ⇒ refused)
  AC-4 : effect-row gating `{Telemetry<Counters>}` extension
         (per § II.6 ; AT-3 attack ⇒ pure-caller refused @ compile-time)
  AC-5 : `#[derive(TelemetrySchema)]` proc-macro emits schema-id
         (per § II.7 ; #[ctor] registration idempotent ; collision-detection)
  AC-6 : TagKey + TagVal type-safe builders
         compile-refuse biometric (BiometricKind enum) per AT-4
         compile-refuse raw-path per AT-5 (extend cssl-effects::check_telemetry_no_raw_path)
  AC-7 : MetricError + MetricResult type aliases ; NaN refused (AT-1) ; Inf clamped-or-refused (AT-2)
  AC-8 : MetricRegistry static-init guard + collision-test (LM-13)
  AC-9 : per-subsystem namespace (constructors per crate-name)
  AC-10 : Wires-into cssl-telemetry ring-buffer (TelemetryRing ; Slot.scope = Counters/Spans/Events)
  AC-11 : Replay-determinism : metrics no-op in determinism-strict OR sampled-deterministically
          (per § VI.2 ; bit-pattern f64 storage via to_bits)
  AC-12 : All-15 landmines exercised in tests (per § VII LM-1..15)

§ III.3  Test-target breakdown (~120 tests)

  C1 ACCEPTANCE (~40 tests) :
    - 10 Counter API : new / inc / inc_by / set / snapshot / overflow-saturate
    - 10 Gauge API   : new / set / inc / dec / snapshot / NaN-refuse / Inf-clamp
    - 10 Histogram API : new / record / snapshot / percentile (linear-interp)
    - 10 Timer API   : new / start ⇒ TimerHandle / drop-records / record_ns advanced
  C2 PROPERTY (~25 tests) :
    - proptest Counter monotonic ∀ inc-sequence
    - proptest Gauge bit-pattern roundtrip ∀ f64 (excluding NaN)
    - proptest Histogram sum + count commutativity-saturating-monoid
    - proptest Timer count = number-of-handle-drops (deterministic)
    - proptest Sampling OneIn(N) frequency-bound (deterministic)
  C3 GOLDEN-BYTES (~10 tests) :
    - canonical bucket-set boundaries (LATENCY_NS / BYTES / COUNT / PIXEL)
    - schema-id stability ∀ TelemetrySchema-derive (compile-time-constant)
  C4 NEGATIVE (~25 tests) :
    - 5 effect-row : pure-caller cannot Counter.inc (compile-fail-tests)
    - 5 biometric tag-key refused (BiometricKind enumeration)
    - 5 raw-path tag-value refused (T11-D130 extension)
    - 5 NaN/Inf handling (refuse at MetricError boundary)
    - 5 SamplingDiscipline.Adaptive in strict-mode refused
  C5 COMPOSITION (~20 tests) :
    - 5 cross-crate registration : 3 stub-subsystems all-register-without-collision
    - 5 schema-id collision detection (#[ctor] guard trips on-collision)
    - 5 wires-into-cssl-telemetry-ring : Counter inc ⇒ TelemetryRing slot emitted
    - 5 strict-mode invariants : `monotonic_ns` indirection via strict_clock

§ III.4  Files-to-create

  compiler-rs/crates/cssl-metrics/
    Cargo.toml
    src/
      lib.rs                  (~150 LOC ; pub-API surface + re-exports)
      counter.rs              (~250 LOC ; Counter + AtomicU64 + sampling)
      gauge.rs                (~250 LOC ; Gauge + bit-pattern f64 + NaN-refuse)
      histogram.rs            (~400 LOC ; Histogram + bucket-monotonic-check + percentile)
      timer.rs                (~350 LOC ; Timer + TimerHandle RAII + strict_clock indirection)
      sampling.rs             (~150 LOC ; SamplingDiscipline + deterministic-decimation)
      tag.rs                  (~200 LOC ; TagKey + TagVal + biometric-refuse + path-hash-refuse)
      registry.rs             (~250 LOC ; MetricRegistry + #[ctor] + collision-detect)
      schema.rs               (~200 LOC ; TelemetrySchema trait + schema-id generation)
      schema_derive.rs        (~250 LOC ; proc-macro for #[derive(TelemetrySchema)])
      strict_clock.rs         (~150 LOC ; deterministic-clock for ReplayStrict mode)
      error.rs                (~100 LOC ; MetricError variants + MetricResult)
      catalog.rs              (~150 LOC ; canonical bucket-sets ; static constants)
      effect_row.rs           (~150 LOC ; effect-row check extension)
    tests/
      Jzeta_1_acceptance/     (~40 test-fns)
      Jzeta_1_property/       (~25 test-fns)
      Jzeta_1_golden_bytes/   (~10 fixtures)
      Jzeta_1_negative/       (~25 test-fns)
      Jzeta_1_composition/    (~20 test-fns)

§ III.5  Implementer-prompt (slice-specific ; full-rendered)

```
ROLE : Implementer for slice T11-D150 (Wave-Jζ-1 : cssl-metrics crate skeleton)

POD-ASSIGNMENT : pod-A
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D150-impl-podA

LANE-DISCIPLINE :
  ✓ may-write : compiler-rs/crates/cssl-metrics/src/**
  ✓ may-write : compiler-rs/crates/cssl-metrics/Cargo.toml
  ✓ may-write : compiler-rs/crates/cssl-metrics-derive/src/** (NEW proc-macro crate)
  ✓ may-write : compiler-rs/crates/cssl-metrics/tests/** (own-tests ; NOT Test-Author's)
  ✓ may-modify : compiler-rs/Cargo.toml workspace-root (additive-only ; new crates)
  ✓ may-modify : compiler-rs/crates/cssl-effects/src/** (effect-row extension only)
  N! may-touch : cssl-telemetry/src/** (additive-via-cssl-metrics only ; no API break)
  N! may-modify : Test-Author's tests/Jzeta_1_*/** (their lane)
  N! may-modify : Critic's failure-fixtures/Jzeta_1_critic_fixtures/** (their lane)

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1-II.7 (lines 54-241)
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § VIII.1 (lines 831-851)
  ‼ `specs/22_TELEMETRY.csl` (R18 baseline ; effect-row scope-taxonomy)
  ‼ DECISIONS.md T11-D130 (path-hash-only) + T11-D132 (biometric-refuse)
  ‼ PRIME-DIRECTIVE.md @ repo-root : §1 + §11 binding

ACCEPTANCE-CRITERIA :
  AC-1  : Counter / Gauge / Histogram / Timer types compile + run
  AC-2  : TimerHandle RAII drop-record ; #[must_use] forbids drop-without-let
  AC-3  : SamplingDiscipline enum + deterministic-decimation
  AC-4  : effect-row gating {Telemetry<Counters>} extension
  AC-5  : #[derive(TelemetrySchema)] proc-macro emits schema-id
  AC-6  : TagKey + TagVal compile-refuse biometric + raw-path
  AC-7  : MetricError + MetricResult ; NaN refused ; Inf clamped
  AC-8  : MetricRegistry static-init guard + collision-test
  AC-9  : per-subsystem namespace
  AC-10 : Wires-into cssl-telemetry ring-buffer
  AC-11 : Replay-determinism : metrics no-op-or-deterministic in strict-mode
  AC-12 : All-15 landmines exercised in tests

DELIVERABLES :
  D1 : impl in compiler-rs/crates/cssl-metrics/src/**
  D2 : own-tests in compiler-rs/crates/cssl-metrics/tests/**
        cargo test -p cssl-metrics -- --test-threads=1 ⇒ N/N pass
  D3 : cargo clippy -p cssl-metrics --all-targets -- -D warnings ⇒ clean
  D4 : commit-message per § XIII template (5-role sign-off trailer)
  D5 : DECISIONS.md slice-entry per § 02 ROLE.Implementer

ITERATION-DISCIPLINE :
  - cycle-counter starts @ 0 ; max-3 cross-pod-iterations before-escalation
  - Reviewer-HIGH ⇒ iterate-SAME-pod-cycle
  - Critic-HIGH-veto ⇒ iterate-NEW-pod-cycle (architectural)
  - Test-Author-fail ⇒ iterate-SAME-pod-cycle
  - cycle ≥ 4 ⇒ MANDATORY-escalation-to-Apocky

LANDMINE-FOCUS (Wave-Jζ-1-specific) :
  ‼ LM-2  adaptive-sampling refused-by-effect-row in strict-mode
  ‼ LM-4  biometric tag-key compile-refuse via cssl-ifc::TelemetryEgress
  ‼ LM-8  un-bounded SmallVec tag-list (≤ 4 inline ; spill = compile-refuse)
  ‼ LM-13 metric-collision detection (#[ctor] guard panics on-double)
  ‼ LM-14 gauge.set(NaN) refused @ MetricError::NaN
  ‼ LM-15 histogram boundary-inference refused (only &'static [f64])

SPECIAL-INSTRUCTIONS :
  - ¬ break-existing-cssl-telemetry API (additive-only ; no-rename)
  - effect-row extension MUST flow-through cssl-effects ; ¬ duplicate logic
  - schema-derive proc-macro lives in cssl-metrics-derive (separate-crate)
  - feature-flag `replay-strict` MUST gate strict_clock indirection
  - all-public-items rustdoc'd with spec-§ cite
  - canonical bucket-sets (LATENCY_NS / BYTES / COUNT / PIXEL) per § II.3 exact-match

PRIME-DIRECTIVE BINDING :
  ‼ §1 prohibitions : surveillance + biometric-channel ⊗ COMPILE-REFUSED via TagKey type
  ‼ §11 attestation : append CREATOR-ATTESTATION block to commit-message
  ‼ §2 cognitive-integrity : ¬ rationalize-around-spec ; ¬ silent-TODOs
  ‼ if-spec-leaves-PRIME-DIRECTIVE-gap ⇒ FAIL-CLOSED ⊗ ESCALATE-to-Apocky

OUTPUT-PROTOCOL :
  - emit §R block at-top-of-each-response (Apocky transparency-discipline)
  - reason in CSLv3-glyphs (density = sovereignty)
  - English-prose only-when : user-facing-chat | error-msg | rustdoc-pub-items
  - all-other-output ≡ CSLv3-native

START-NOW. Dispatch your-first-tool-call after-emitting-§R.
```

§ III.6  Reviewer-prompt (slice-specific ; full-rendered)

```
ROLE : Reviewer for slice T11-D150 (Wave-Jζ-1 : cssl-metrics crate skeleton)

POD-ASSIGNMENT : pod-B (B ≠ A = Implementer-pod)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D150-review-podB
PARALLEL-WITH : Implementer (concurrent ; not-sequential)

LANE-DISCIPLINE :
  N! production-code (¬ in src/ ¬ in tests/)
  N! workspace-Cargo.toml modification
  N! files-other-than .review-report.md
  ✓ SUGGESTED-PATCH-blocks-as-comments in review-report
  ✓ clarifying-questions for Implementer (in-report)
  ✓ cross-references DECISIONS history for prior-art

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1-II.7 + § VIII.1
  ‼ specs/22_TELEMETRY.csl (R18 baseline)
  ‼ Implementer's commits-as-they-land (worktree-shared via orchestrator)

REVIEW-CRITERIA (per § 02 ROLE.Reviewer.§1.6) :
  §1.1 spec-anchor-conformance       (✓/◐/○/✗)
  §1.2 API-surface-coherence         (✓/◐/○/✗)
  §1.3 invariant-preservation        (✓/◐/○/✗)
  §1.4 documentation-presence        (rustdoc on pub items ✓)
  §1.5 findings-list (HIGH/MED/LOW)  ; ≥ 5 specific file:line refs per HIGH
  §1.6 suggested-patches (as-comments ¬ as-commits)
  §1.7 escalations-raised (cross-ref Architect / Spec-Steward tickets)

REVIEW-FOCUS (Wave-Jζ-1-specific) :
  - effect-row extension consistent-with cssl-effects existing-pattern (no API divergence)
  - schema-derive proc-macro hygiene (no name-clashes with future-derives)
  - AtomicU64 ordering :
      `Acquire/Release` for cross-thread reads
      `Relaxed` permitted for monotonic-counter-only ; review-flag if-used-elsewhere
  - SmallVec inline-cap = 4 (per § II.1) ; spill = compile-refuse, NOT runtime-grow
  - replay-determinism integration : strict_clock substitutes monotonic_ns under feature-flag
  - canonical bucket-sets : verify match § II.3 spec-table exactly
  - TagKey + TagVal : biometric-compile-refuse ; raw-path-compile-refuse
  - MetricError variants : NaN / Inf / Overflow / SchemaCollision / EffectRowViolation

DELIVERABLES :
  D1 : .review-report.md in worktree-root ; sections-per-criteria-above
  D2 : DECISIONS.md sub-entry §D. Reviewer-Signoff (Reviewer-id ; pod-B)
       format :
         - spec-anchor-conformance : ✓
         - API-surface-coherence   : ✓
         - invariant-preservation  : ✓
         - findings-resolved       : N (all-HIGH closed ; M-MED open)
         - escalations             : ∅ | <ticket-ref>
         ⇒ APPROVE-merge-pending-Critic-+-Validator
  D3 : commit-message trailer in Implementer's commit :
         "Reviewer-Approved-By: <reviewer-id> pod-B"

REVIEW-CADENCE :
  - check-at every-Implementer-checkpoint (∼ every 200 LOC committed)
  - request-iteration on ≥ 1 HIGH-finding (same-pod-cycle ; up to 3-cycles)
  - escalate-to-Architect on cross-slice-implication
  - escalate-to-Spec-Steward on spec-itself-wrong claim

EXPECTED-FINDINGS :
  ≥ 5 specific file:line refs per HIGH-finding (per § 02 §1.10)
  pay-special-attention :
    - rustdoc completeness (per § 02 §1.6.D1.§1.4)
    - PRIME-DIRECTIVE binding cited in module-doc (LM-3 + LM-4)
    - TimerHandle RAII drop-correctness ; #[must_use] enforced

ANTI-PATTERN.RUBBER-STAMP :
  ¬ all-✓ within-30s ⇒ rubber-stamp-detection ⇒ pod-rotation
  ¬ 0-findings ⇒ flag-for-audit
  W! ≥ 5 specific file:line refs per HIGH-finding

OUTPUT-PROTOCOL :
  - emit §R block per response
  - reason in CSLv3-glyphs ; English-prose only-when user-facing OR error-msg

START-NOW. First action : read spec-anchor + Implementer's-current-state.
```

§ III.7  Test-Author-prompt (slice-specific ; full-rendered)

```
ROLE : Test-Author for slice T11-D150 (Wave-Jζ-1 : cssl-metrics crate skeleton)

POD-ASSIGNMENT : pod-C (C ≠ A = Impl-pod, C ≠ B = Rev-pod)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D150-test-podC
PARALLEL-WITH : Implementer (concurrent ; not-sequential)

LANE-DISCIPLINE :
  N! production-code (¬ in src/)
  N! workspace-Cargo.toml (except tests-only deps : proptest + trybuild)
  N! Implementer's-current-branch (orchestrator denies-checkout)
  N! Implementer's-prompt or rationale-comments (until-merge-time)
  N! ASK Implementer for hints
  ✓ tests in compiler-rs/crates/cssl-metrics/tests/Jzeta_1_acceptance/
  ✓ tests in compiler-rs/crates/cssl-metrics/tests/Jzeta_1_property/
  ✓ fixtures in compiler-rs/crates/cssl-metrics/tests/Jzeta_1_golden_bytes/
  ✓ negative-tests in compiler-rs/crates/cssl-metrics/tests/Jzeta_1_negative/
  ✓ composition-tests in compiler-rs/crates/cssl-metrics/tests/Jzeta_1_composition/

SPEC-ONLY-INPUT (CRITICAL ; per § 02 ROLE.Test-Author.§4.6) :
  ✓ INPUT : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1-II.7
  ✓ INPUT : canonical-API contracts in published cssl-* crates
            (READ-existing-types ; ¬ Implementer-WIP)
  ✓ INPUT : DECISIONS history T11-D130 + T11-D132 (for biometric/path constraints)
  ✓ INPUT : prior-test-suites in workspace (style-consistency only)
  N! INPUT : Implementer's-current-WIP-branch
  N! INPUT : Implementer's-prompt-text
  N! ASK : Implementer "should Counter.set return Err on overflow?"
            ← if spec-ambiguous : ESCALATE-Spec-Steward (only-allowed-path)

TEST-CATEGORIES (Wave-Jζ-1-specific ; ~120 total) :
  C1 ACCEPTANCE  : ~40 ; per § III.3 acceptance-tests
  C2 PROPERTY    : ~25 ; proptest crate (commutativity / monotonicity / roundtrip)
  C3 GOLDEN      : ~10 ; canonical bucket-set bytes + schema-id constants
  C4 NEGATIVE    : ~25 ; compile-fail-tests use trybuild crate
  C5 COMPOSITION : ~20 ; cross-crate stub-subsystems

SPEC-ONLY-INPUT REMINDER :
  ¬ ask Implementer "what does Counter.set return on overflow?"
  ✓ read § II.1 : "monotonic-non-decreasing ... `set` permitted ⊗ tagged-as-RESET-EVENT
                    ⊗ audit-chain logs the reset"
  ⇒ test : reset-event tagged ; audit-chain logs

LANDMINE-COVERAGE-IN-TESTS (per § VII spec-table) :
  ‼ EVERY landmine LM-1..15 MUST have ≥ 1 test-fn
  ‼ AT-1..AT-15 attack-vectors from Critic-prompt MUST have parallel-test-fns
  example mappings :
    LM-2  adaptive-sampling refused   ⇒ test : OneIn(Adaptive) under ReplayStrict ⇒ compile-fail
    LM-4  biometric tag-key refused   ⇒ test : tag(("face_id", x)) ⇒ compile-fail
    LM-13 metric-collision detection  ⇒ test : 2× #[ctor] same-name ⇒ panic-on-init
    LM-14 gauge.set(NaN) refused      ⇒ test : Gauge.set(f64::NAN) ⇒ Err(MetricError::NaN)
    LM-15 histogram boundary-runtime  ⇒ test : Vec<f64> bucket ⇒ compile-fail (only static)

DELIVERABLES :
  D1 : test-suite per-slice in tests/ subdirectories (per LANE-DISCIPLINE above)
  D2 : test-coverage-map artifact (audit/spec_coverage.toml)
       schema :
         [[mapping]]
         spec_section = "06_L2_TELEMETRY § II.1 Counter"
         test_fns     = ["tests::Jzeta_1_acceptance::counter::t_inc_monotonic"]
  D3 : DECISIONS.md sub-entry §D. Test-Author-Suite (id ; pod-C) :
         - spec-§-coverage : N/N (100%)
         - test-fn-count   : ~120
         - golden-fixtures : ~10
         - property-tests  : ~25
         - negative-tests  : ~25
         - pod-id          : pod-C (cross-pod-confirmed-≠-Impl-pod-≠-Rev-pod)
         ⇒ APPROVE-merge-pending-Critic-+-Validator
  D4 : commit-message trailer : "Test-Author-Suite-By: <id> pod-C"

ITERATION-DISCIPLINE :
  - tests-fail @ merge-time ⇒ block-merge ; Impl iterates ; up-to-3-cycles
  - Implementer-claims-test-wrong ⇒ DECISIONS sub-entry ; Test-Author counter-signs OR rejects
  - spec-ambiguous ⇒ ESCALATE-Spec-Steward (per § 02 ROLE.Test-Author.§4.4 A4)

ANTI-PATTERN.ASK-IMPLEMENTER :
  ¬ message Implementer-channel "should X return Y?"
  W! orchestrator severs cross-agent messaging at-dispatch
  W! only Spec-Steward channel open

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs ; English-prose only-when test-rationale-comment

START-NOW. First action : read spec-anchor § II.1-II.7 + canonical-API in cssl-* crates.
```

§ III.8  Critic-prompt (slice-specific ; full-rendered)

```
ROLE : Critic for slice T11-D150 (Wave-Jζ-1 : cssl-metrics crate skeleton)

POD-ASSIGNMENT : pod-D (D ≠ A = Impl, D ≠ B = Rev, D ≠ C = TA)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D150-critic-podD
TIMING : POST-Implementer-final + POST-Reviewer.D2 + POST-Test-Author.D3
        ‼ DO-NOT-DISPATCH concurrent-with-Implementer

LANE-DISCIPLINE :
  N! production-code (¬ in src/)
  N! workspace-Cargo.toml
  ✓ failure-fixture-tests in compiler-rs/crates/cssl-metrics/tests/Jzeta_1_critic_fixtures/
  ✓ failing-property-tests in same-dir
  ✓ adversarial-input fixtures in compiler-rs/crates/cssl-metrics/test-data/Jzeta_1_critic/
  ✓ red-team-report.md authored

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1-II.7 + § VII (landmines)
  ‼ specs/22_TELEMETRY.csl + DECISIONS T11-D130/T11-D132
  ‼ Implementer's final-diff + Reviewer's D1 report + Test-Author's test-suite

ADVERSARIAL FRAMING (CRITICAL) :
  W! Critic prompts contain explicit adversarial-language :
    "If I were trying to break this, I would..."
    "What's the failure-mode of this assumption?"
    "Where does this fall apart at scale / under composition / at boundary?"
    "What's an input the Implementer didn't think of?"
  N! Critic-language : "looks-good" / "approved" / "no-issues-found" without effort
  W! Critic-output MUST list ≥ 5 attack-attempts even-if-all-survived

SEVERITY CLASSIFICATION (per § 02 ROLE.Critic.§2.6) :
  HIGH (veto-power) :
    H1 : invariant-violation triggerable-from-public-API
    H2 : data-corruption-path silent ¬ panic
    H3 : PRIME-DIRECTIVE-conflict (consent-bypass ; surveillance ; biometric)
    H4 : security-vulnerability (auth-bypass ; unsafe-public-API)
    H5 : spec-conformance-fail (impl contradicts spec)
    H6 : composition-failure with another-merged-slice
  MED (recommend) : M1-M5 edge / perf / msg / doc / coverage
  LOW (track)     : L1-L4 style / naming / refactor / unused

ATTACK-FOCUS (Wave-Jζ-1-specific ; ≥ 15 attempts) :
  AT-1  NaN to Gauge.set ⇒ refused @ MetricError::NaN ?
  AT-2  Inf to Gauge.set ⇒ refused-or-clamped @ schema-policy ?
  AT-3  pure-caller Counter.inc ⇒ effect-row should-refuse @ compile-time
  AT-4  BiometricKind tag-key ⇒ should-refuse via cssl-ifc::TelemetryEgress
  AT-5  raw-path tag-value ("/etc/hosts") ⇒ should-refuse via T11-D130
  AT-6  Adaptive under ReplayStrict ⇒ should-refuse @ effect-row
  AT-7  monotonic_ns direct under ReplayStrict ⇒ should-refuse via strict_clock
  AT-8  Welford in strict-mode ⇒ should-refuse @ effect-row
  AT-13 SmallVec tag-list ≥ 5 entries ⇒ spill = compile-refuse ?
  AT-14 Histogram boundary-inference at-runtime ⇒ refuse ; only static-boundaries ?
  AT-15 duplicate metric-name @ ctor-init ⇒ collision-detection trips ?
  AT-X1 Counter.inc_by(u64::MAX) twice ⇒ saturating-overflow ⇒ Audit-event ?
  AT-X2 Gauge.set(f64) bit-pattern roundtrip ⇒ does to_bits/from_bits preserve ?
  AT-X3 Histogram percentile(0.0) ⇒ returns lowest-bucket-boundary ?
  AT-X4 Histogram percentile(1.0) ⇒ returns highest-bucket-boundary ?

DELIVERABLES :
  D1 : red-team-report.md (per § 02 ROLE.Critic.§2.7)
       sections :
         §1 attack-surface-mapped (what slice exposes)
         §2 invariants-asserted (what slice claims)
         §3 failure-modes-attempted (≥ 5 attacks ; pass/fail per attack)
         §4 findings-list (HIGH/MED/LOW classification ; cross-ref §3)
         §5 mitigations-proposed (per-finding ; concrete-code-or-design)
         §6 veto-flag (TRUE if any-HIGH ; FALSE-otherwise)
  D2 : failure-fixture-tests in tests/Jzeta_1_critic_fixtures/
       — Implementer MUST make-pass before-merge
       — Implementer cannot weaken/skip/cfg-out
  D3 : DECISIONS.md sub-entry §D. Critic-Veto (Critic-id ; pod-D) :
         - veto-flag      : FALSE
         - HIGH-resolved  : N
         - MED-tracked    : M
         - LOW-tracked    : K
         - failure-fixtures-authored : F-count
         ⇒ APPROVE-merge-pending-Validator
  D4 : if veto-flag = TRUE : Implementer iterates NEW-pod-cycle ; up-to-3-cycles
  D5 : commit-message trailer : "Critic-Veto-Cleared-By: <id> pod-D"

ANTI-PATTERN.PRAISE :
  ¬ "looks-good" / "well-designed" / "no-issues-found" without ≥ 5 attempts
  W! zero-findings is RED-FLAG ¬ green-flag

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs (W! adversarial-discipline language)
  - English-prose only-when red-team-report-section-headers

START-NOW. First action : map attack-surface §1 of red-team-report.
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅳ  SLICE Jζ-2 : PER-STAGE FRAME-TIME INSTRUMENTATION
═══════════════════════════════════════════════════════════════════════════════

§ IV.1  Slice metadata

  slice-id        : T11-D151 (Wave-Jζ-2)
  slice-title     : Per-stage frame-time instrumentation (12 canonical render-pipeline stages)
  spec-anchor     : `_drafts/phase_j/06_l2_telemetry_spec.md` § III + § VIII.2
  target-crates   : `compiler-rs/crates/cssl-render-v2/` (12 stages)
                  + `loa-game/m8_integration/pipeline.rs` + `stage_*.rs`
                  + cross-cutting per § XII implementation-targets
  LOC-target      : ~2.5K
  tests-target    : ~100 (upgrade from spec-baseline 80 ; +20 cross-stage assertions)
  depends         : Jζ-1 (cssl-metrics types)
  blocks          : Jζ-4 (citing-metrics)
  pod-rotation    : Impl-B / Rev-C / TA-D / Critic-A
  estimated-impl  : ~45-60 min wall-clock

§ IV.2  Acceptance-criteria

  AC-1 : All 12 canonical render-pipeline stages instrumented (per RENDERING_PIPELINE §III)
         stage-1 (entropy-LOD) ... stage-12 (composite-present)
  AC-2 : Per-stage Timer at frame-boundary : `render.stage_time_ns{stage=N, mode}`
         (per § III.3 spec-table)
  AC-3 : p50 / p95 / p99 percentile reads work via Histogram-derived OR t-digest
         (deterministic in strict-mode ; per § VI.2)
  AC-4 : Cross-frame trend tracking : Gauge `engine.tick_rate_hz` rolling-30-frame
         (per § III.1 spec-table)
  AC-5 : Zero-overhead in metrics-disabled build :
         feature-flag `metrics-disabled` ⇒ all record-sites compile to no-op
         (cargo bench delta < 0.05% with vs without)
  AC-6 : Stage-tag MUST cover-all-12 ; missing-stage = telemetry-completeness violation
         (per § III.3 ‼ rule ; AC enforces compile-time-tag-validation)
  AC-7 : Per-subsystem namespacing per § III.1-III.12 (engine.* / omega_step.* / render.* / ...)
  AC-8 : Each metric registers-in MetricRegistry @ #[ctor] static-init
  AC-9 : Catalog-completeness check : `cssl-metrics::REGISTRY.completeness_check(&CATALOG)`
         ⇒ build-fail if-< 100% of § III table

§ IV.3  Test-target breakdown (~100 tests)

  C1 ACCEPTANCE (~40 tests) :
    - 12 per-stage Timer registration (each render-stage cited)
    - 12 per-stage Timer record-on-frame-boundary
    - 8  cross-frame trend (engine.tick_rate_hz update under deterministic frame-N)
    - 8  per-subsystem record-site (engine / omega_step / render / physics / wave /
                                      spectral / xr / anim / audio / omega_field / kan / gaze)
  C2 PROPERTY (~20 tests) :
    - proptest p50 ≤ p95 ≤ p99 ∀ Histogram-snapshot
    - proptest stage-Timer total = sum of-individual-stage-records
    - proptest dropped_frames Counter increments only-on-deadline-miss event
  C3 GOLDEN-BYTES (~10 fixtures) :
    - 12-stage canonical name-list matches RENDERING_PIPELINE §III exactly
    - canonical bucket-sets per-Timer-name (LATENCY_NS for-render.stage_time_ns)
  C4 NEGATIVE (~15 tests) :
    - missing stage-tag detected at-compile (any 1-of-12 stages omitted ⇒ compile-fail)
    - per-Sovereign tag-attempt refused (LM-5)
    - per-creature tag without category-anonymized-hash refused (LM-6)
    - metrics-disabled build : record-sites compile-out (binary-size delta ≈ 0)
  C5 COMPOSITION (~15 tests) :
    - completeness_check trips on-deletion of-any-cataloged metric
    - 12-stage timer-pipeline sequential : stage-N done ⇒ stage-N+1 starts (FrameId monotone)
    - cross-stage budget aggregation : sum-of-stages ≤ frame-deadline
    - cross-subsystem total (engine.frame_time_ns) = sum-of-omega_step.phase_time_ns + render.stage_time_ns

§ IV.4  Files-to-modify

  compiler-rs/crates/cssl-render-v2/src/pipeline.rs       (+timer instrumentation)
  compiler-rs/crates/cssl-render-v2/src/stage_1.rs        (+stage Timer)
  compiler-rs/crates/cssl-render-v2/src/stage_2.rs        (+stage Timer)
  ... (12 stages total)
  compiler-rs/crates/cssl-render-v2/src/stage_12.rs       (+stage Timer)
  compiler-rs/crates/cssl-render-v2/src/lib.rs            (+ctor-init metrics-block)
  compiler-rs/crates/cssl-engine/src/lib.rs               (+engine.* metrics)
  compiler-rs/crates/cssl-omega-step/src/lib.rs           (+omega_step.* metrics)
  compiler-rs/crates/cssl-physics-wave/src/lib.rs         (+physics.* metrics)
  compiler-rs/crates/cssl-wave-solver/src/lib.rs          (+wave.* metrics)
  compiler-rs/crates/cssl-spectral-render/src/lib.rs      (+spectral.* metrics)
  compiler-rs/crates/cssl-host-openxr/src/lib.rs          (+xr.* metrics)
  compiler-rs/crates/cssl-anim-procedural/src/lib.rs      (+anim.* metrics)
  compiler-rs/crates/cssl-wave-audio/src/lib.rs           (+audio.* metrics)
  compiler-rs/crates/cssl-substrate-omega-field/src/lib.rs (+omega_field.* metrics)
  compiler-rs/crates/cssl-substrate-kan/src/lib.rs        (+kan.* metrics)
  compiler-rs/crates/cssl-gaze-collapse/src/lib.rs        (+gaze.* metrics ; PRIME §1 careful)
  loa-game/m8_integration/pipeline.rs                     (+M8 pipeline timer)
  loa-game/m8_integration/stage_*.rs                      (+per-stage M8 timer)

§ IV.5  Implementer-prompt (slice-specific ; full-rendered)

```
ROLE : Implementer for slice T11-D151 (Wave-Jζ-2 : per-stage frame-time instrumentation)

POD-ASSIGNMENT : pod-B
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D151-impl-podB

LANE-DISCIPLINE :
  ✓ may-modify : 13 subsystem-crates per § IV.4 (cross-cutting instrumentation)
  ✓ may-modify : loa-game/m8_integration/pipeline.rs + stage_*.rs
  ✓ may-modify : crate-Cargo.toml (additive : cssl-metrics dep)
  N! may-modify : cssl-metrics/src/** (Wave-Jζ-1 owns ; additive-only via API)
  N! may-modify : workspace-Cargo.toml without PM-approval

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § III (lines 244-460)
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § VIII.2 (lines 853-879)
  ‼ `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md` § V phase-budget
  ‼ `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl` § III 12-stage-table
  ‼ DECISIONS.md T11-D130 (path-hash-only) + T11-D132 (biometric-refuse)

ACCEPTANCE-CRITERIA :
  AC-1 : 12 canonical render-pipeline stages instrumented
  AC-2 : Per-stage Timer at frame-boundary (per § III.3)
  AC-3 : p50 / p95 / p99 percentile reads work (deterministic in strict-mode)
  AC-4 : Cross-frame trend tracking (engine.tick_rate_hz rolling-30-frame)
  AC-5 : Zero-overhead in metrics-disabled build (cargo bench Δ < 0.05%)
  AC-6 : Stage-tag MUST cover-all-12 ; missing = compile-fail
  AC-7 : Per-subsystem namespacing per § III.1-III.12
  AC-8 : Each metric registers-in MetricRegistry @ #[ctor]
  AC-9 : Catalog-completeness check : REGISTRY.completeness_check(&CATALOG)
         ⇒ build-fail if-< 100% of § III table

DELIVERABLES :
  D1 : 13 subsystem-crates instrumented (per § IV.4)
  D2 : per-crate tests in <crate>/tests/Jzeta_2_*/
        cargo test --workspace -- --test-threads=1 ⇒ N/N pass
  D3 : cargo clippy --workspace --all-targets -- -D warnings ⇒ clean
  D4 : commit-message per § XIII template
  D5 : DECISIONS.md slice-entry per-subsystem sub-anchor

LANDMINE-FOCUS (slice-specific) :
  ‼ LM-3  raw-path in tag-value refused via path_hash-discipline
  ‼ LM-4  biometric tag-key refused (especially xr.* + gaze.*)
  ‼ LM-5  per-Sovereign tag refused (aggregate-only)
  ‼ LM-6  per-creature tag : creature_kind_hash anonymized
            (audio.creature_vocalization_count)
  ‼ LM-7  per-frame metric-overhead > 0.5% ⇒ auto-decimate + Audit-event
  ‼ LM-9  un-registered metric in record-call detected at-compile

GAZE-SUBSYSTEM SPECIAL-INSTRUCTIONS (§1-CRITICAL) :
  ‼ ‼ ‼ N! gaze-direction logged
  ‼ ‼ ‼ N! blink-pattern logged
  ‼ ‼ ‼ N! eye-openness-distribution logged
  ✓ aggregate-confidence-and-latency only (gaze.confidence_avg / gaze.saccade_predict_latency_ns)
  ✓ refused-egress-attempt counter (the canary that NEVER ticks)
  W! `gaze.privacy_egress_attempts_refused` ⊗ alarm-on-non-zero
        ⊗ Audit<"prime-directive-leak-attempt"> emitted

STAGE-TAG DISCIPLINE :
  W! stage-tag enum (Stage1..Stage12) ⊗ exhaustive-match-required at-compile
  W! missing-stage = telemetry-completeness-violation @ build-fail
  W! tag-len ≤ 4 inline (SmallVec) ; spill = compile-refuse

CATALOG-COMPLETENESS-CHECK :
  W! @build : cssl-metrics::REGISTRY.completeness_check(&CATALOG) ⊗ build-fail if-< 100%
  W! CATALOG = include_str!("phase_j/06_l2_telemetry_spec.md") parsed-into-static-table
  ⊗ self-references-this-spec ⊗ engine-is-its-own-spec-coverage-witness

PRIME-DIRECTIVE BINDING :
  ‼ §1 : aggregate-only metrics ; biometric refused @ compile-time
  ‼ §11 : attestation-trailer in commit-message
  ‼ if-spec-leaves-PRIME-DIRECTIVE-gap ⇒ FAIL-CLOSED ⊗ ESCALATE-to-Apocky

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs ; English-prose only when user-facing OR rustdoc

START-NOW. Dispatch your-first-tool-call after-emitting-§R.
```

§ IV.6  Reviewer-prompt (slice-specific ; full-rendered)

```
ROLE : Reviewer for slice T11-D151 (Wave-Jζ-2 : per-stage frame-time instrumentation)

POD-ASSIGNMENT : pod-C (C ≠ B = Implementer-pod)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D151-review-podC
PARALLEL-WITH : Implementer

LANE-DISCIPLINE :
  N! production-code in any-of-13-subsystem-crates
  ✓ .review-report.md only

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § III + § VIII.2
  ‼ Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md § V
  ‼ Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III

REVIEW-CRITERIA (per § 02 ROLE.Reviewer.§1.6) :
  §1.1 spec-anchor-conformance       (✓/◐/○/✗)
  §1.2 API-surface-coherence         (✓/◐/○/✗)
  §1.3 invariant-preservation        (✓/◐/○/✗)
  §1.4 documentation-presence        (rustdoc-cited spec-§)
  §1.5 findings-list (HIGH/MED/LOW)
  §1.6 suggested-patches
  §1.7 escalations-raised

REVIEW-FOCUS (Wave-Jζ-2-specific) :
  - 12-stage pipeline : exhaustive coverage ; missing-stage = HIGH severity
  - each metric matches § III spec-table exactly (name + type + tags + cite)
  - per-subsystem namespacing : engine.* / omega_step.* / render.* / physics.* /
    wave.* / spectral.* / xr.* / anim.* / audio.* / omega_field.* / kan.* / gaze.*
    no-cross-namespace-leak
  - gaze-subsystem § 1-careful : aggregate-only enforcement (HIGH if-violated)
  - xr.tracker_confidence : tracker-name-only ; NOT tracker-data
  - audio.creature_vocalization_count : creature_kind_hash (NOT per-creature-id)
  - omega_field.sigma_mask_mutation_rate : aggregate-counter-only (NOT per-cell)
  - metrics-disabled feature-flag : zero-overhead bench-validates < 0.05%
  - completeness_check macro at-build : build-fail-on-gap
  - cross-stage Timer aggregation correctness :
    sum(omega_step.phase_time_ns) + sum(render.stage_time_ns) ≤ engine.frame_time_ns

DELIVERABLES :
  D1 : .review-report.md (worktree-root)
  D2 : DECISIONS.md sub-entry §D. Reviewer-Signoff (id ; pod-C)
  D3 : commit-message trailer "Reviewer-Approved-By: <id> pod-C"

EXPECTED-FINDINGS :
  ≥ 5 specific refs per HIGH-finding
  special-attention to-PRIME-DIRECTIVE in xr.* + gaze.* + audio.* + omega_field.*

ANTI-PATTERN.RUBBER-STAMP :
  ¬ all-✓ within-30s ⇒ pod-rotation
  ¬ 0-findings ⇒ flag-for-audit

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs ; English-prose only-when user-facing

START-NOW. First action : map all 13 subsystem-crate instrumentation-points
                          against § III spec-table.
```

§ IV.7  Test-Author-prompt (slice-specific ; full-rendered)

```
ROLE : Test-Author for slice T11-D151 (Wave-Jζ-2 : per-stage frame-time instrumentation)

POD-ASSIGNMENT : pod-D (D ≠ B = Impl, D ≠ C = Rev)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D151-test-podD
PARALLEL-WITH : Implementer

LANE-DISCIPLINE :
  N! production-code in any-of-13-subsystem-crates
  ✓ tests in <crate>/tests/Jzeta_2_acceptance/
  ✓ tests in <crate>/tests/Jzeta_2_property/
  ✓ tests in <crate>/tests/Jzeta_2_golden/
  ✓ tests in <crate>/tests/Jzeta_2_negative/
  ✓ tests in <crate>/tests/Jzeta_2_composition/

SPEC-ONLY-INPUT (CRITICAL) :
  ✓ INPUT : `_drafts/phase_j/06_l2_telemetry_spec.md` § III
  ✓ INPUT : Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md
  ✓ INPUT : Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl
  ✓ INPUT : DECISIONS.md (T11-D130 path-hash + T11-D132 biometric)
  N! INPUT : Implementer's-WIP-branch
  N! ASK : Implementer "what's stage-7 doing?"

TEST-CATEGORIES (Wave-Jζ-2-specific ; ~100 total) :
  C1 ACCEPTANCE  : ~40 ;
                   - 12 per-stage Timer registration (each render-stage cited)
                   - 12 per-stage Timer record-on-frame-boundary
                   - 8  cross-frame trend (engine.tick_rate_hz under deterministic frame-N)
                   - 8  per-subsystem record-site (12 subsystems)
  C2 PROPERTY    : ~20 ;
                   - p50 ≤ p95 ≤ p99 ∀ Histogram-snapshot
                   - stage-Timer total = sum of-individual-stage-records
                   - dropped_frames Counter increments only-on-deadline-miss event
  C3 GOLDEN      : ~10 ;
                   - 12-stage canonical name-list matches RENDERING_PIPELINE §III
                   - canonical bucket-sets per-Timer (LATENCY_NS for-stage_time_ns)
  C4 NEGATIVE    : ~15 ;
                   - missing stage-tag detected at-compile (1-of-12 omitted ⇒ compile-fail)
                   - per-Sovereign tag-attempt refused (LM-5)
                   - per-creature tag without category-anonymized-hash refused (LM-6)
                   - metrics-disabled : record-sites compile-out (binary-size Δ ≈ 0)
  C5 COMPOSITION : ~15 ;
                   - completeness_check trips on-deletion of-any-cataloged metric
                   - 12-stage timer-pipeline sequential : stage-N done ⇒ stage-N+1 starts
                   - cross-stage budget aggregation : sum-of-stages ≤ frame-deadline
                   - cross-subsystem total = sum-of-omega_step + render

SPEC-ONLY-INPUT REMINDER :
  ¬ ask Implementer "what's stage-7 doing?"
  ✓ read RENDERING_PIPELINE §III stage-7 directly (lattice-Boltzmann eval)
  ⇒ test : stage-7 timer records ≥ 1 entry per-frame ; budget = 1.5ms

LANDMINE-COVERAGE :
  ‼ LM-3, LM-4, LM-5, LM-6, LM-7, LM-9 each ≥ 1 test-fn

DELIVERABLES :
  D1 : test-suite per-crate
  D2 : audit/spec_coverage.toml updated with-all-12-stages-mapped
  D3 : DECISIONS.md sub-entry §D. Test-Author-Suite (id ; pod-D)
  D4 : commit-message trailer "Test-Author-Suite-By: <id> pod-D"

ITERATION-DISCIPLINE :
  - tests-fail @ merge-time ⇒ block-merge ; up-to-3-cycles
  - spec-ambiguous ⇒ ESCALATE-Spec-Steward

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : enumerate 12 render-pipeline stages from RENDERING_PIPELINE §III.
```

§ IV.8  Critic-prompt (slice-specific ; full-rendered)

```
ROLE : Critic for slice T11-D151 (Wave-Jζ-2 : per-stage frame-time instrumentation)

POD-ASSIGNMENT : pod-A (rotation-shift ; ≠ Impl-B, ≠ Rev-C, ≠ TA-D)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D151-critic-podA
TIMING : POST-Implementer-final + POST-Reviewer.D2 + POST-Test-Author.D3

LANE-DISCIPLINE :
  N! production-code
  ✓ failure-fixture-tests in <crate>/tests/Jzeta_2_critic_fixtures/
  ✓ red-team-report.md authored

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § III + § VII
  ‼ Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md § V
  ‼ Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III
  ‼ Implementer's final-diff + Reviewer's D1 + Test-Author's test-suite

ADVERSARIAL FRAMING :
  W! ≥ 5 attack-attempts catalogued
  N! "looks-good" without effort
  W! adversarial-language not-deferential

ATTACK-FOCUS (Wave-Jζ-2-specific ; ≥ 15 attempts) :
  AT-1  : 1-of-12 stages omitted ⇒ does completeness_check trip ?
  AT-2  : per-Sovereign tag-attempt in any-subsystem ⇒ aggregate-only enforced ?
  AT-3  : gaze-direction tag-value attempted ⇒ refused via cssl-ifc::TelemetryEgress ?
  AT-4  : eye-openness-distribution tag attempted ⇒ refused as biometric ?
  AT-5  : blink-pattern Counter attempted ⇒ refused as biometric ?
  AT-6  : per-creature unique-id (NOT category-hash) attempted ⇒ refused ?
  AT-7  : metrics-disabled build : binary-size Δ ⇒ ≈ 0 (cargo build --release size-check) ?
  AT-8  : metric-overhead per-frame > 0.5% ⇒ auto-decimate triggered ?
  AT-9  : raw-path tag-value (e.g. asset-load-path) ⇒ refused via path_hash ?
  AT-10 : tracker-sensor-data leaked via xr.tracker_confidence ⇒ refused ?
  AT-11 : audio-source-content-hash attempted as tag ⇒ refused (only stream-handle-id) ?
  AT-12 : Σ-mask per-cell metric attempted ⇒ refused (only aggregate-mutation-rate) ?
  AT-13 : engine.frame_time_ns > sum-of-omega_step.phase_time_ns + render.stage_time_ns ?
          (composition-failure if-true ; budget-coherence break)
  AT-14 : tag-len 5 entries ⇒ SmallVec spill ⇒ compile-refuse ?
  AT-15 : missing subsystem-health (one-of-13 doesn't expose health-probe @ Wave-Jζ-3) ?

CROSS-SUBSYSTEM ATTACK-VECTORS :
  - 12-stage timer aggregation : do all-12 sum to ≤ engine.frame_time_ns budget ?
  - per-subsystem catalog completeness : do all-§-III rows have a record-site ?
  - omega_field.cell_count_active for-tier-T0..T3 : do all-4-tiers tagged ?

DELIVERABLES :
  D1 : red-team-report.md (per § 02 ROLE.Critic.§2.7)
  D2 : failure-fixture-tests in tests/Jzeta_2_critic_fixtures/
  D3 : DECISIONS.md sub-entry §D. Critic-Veto (id ; pod-A)
  D4 : commit-message trailer "Critic-Veto-Cleared-By: <id> pod-A"

ANTI-PATTERN.PRAISE :
  ¬ "looks-good" without ≥ 5 attempts
  W! zero-findings = RED-FLAG

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs (adversarial-discipline language)

START-NOW. First action : map attack-surface §1 of red-team-report.
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅴ  SLICE Jζ-3 : PER-SUBSYSTEM HEALTH PROBES
═══════════════════════════════════════════════════════════════════════════════

§ V.1  Slice metadata

  slice-id        : T11-D152 (Wave-Jζ-3)
  slice-title     : Per-subsystem health probes (HealthStatus + 12+ probes + aggregator)
  spec-anchor     : `_drafts/phase_j/06_l2_telemetry_spec.md` § V + § VIII.4
  target-crate    : `compiler-rs/crates/cssl-health/` (NEW, sibling-of cssl-metrics)
                  + 12+ subsystem-crates implementing HealthProbe
  LOC-target      : ~2.5K
  tests-target    : ~80 (upgrade from spec-baseline 60 ; +20 trait-based discipline tests)
  depends         : Jζ-1 (cssl-metrics types)
  blocks          : Jζ-5 (MCP preview consumes health-aggregation)
  pod-rotation    : Impl-C / Rev-D / TA-A / Critic-B
  estimated-impl  : ~45-60 min wall-clock

§ V.2  Acceptance-criteria

  AC-1 : HealthStatus enum (Ok / Degraded { reason, budget_overshoot_pct, since_frame } /
                            Failed { reason, kind, since_frame }) (per § V.1)
  AC-2 : HealthFailureKind enum (DeadlineMiss / ResourceExhaustion / ThermalThrottle /
                                  ConsentViolationDetected / InvariantBreach /
                                  UpstreamFailure / PrimeDirectiveTrip / Unknown) (per § V.1)
  AC-3 : HealthProbe trait (name / health / degrade) (per § V.4)
  AC-4 : Per-subsystem health() probe in 12+ subsystems :
         cssl-render-v2, cssl-physics-wave, cssl-wave-solver, cssl-spectral-render,
         cssl-fractal-amp (note: spec lists this), cssl-gaze-collapse,
         cssl-render-companion-perspective, cssl-host-openxr,
         cssl-anim-procedural, cssl-wave-audio, cssl-substrate-omega-field,
         cssl-substrate-kan, omega_step (engine-internal)
  AC-5 : Aggregated `engine_health()` returns HealthAggregate (worst-case-monoid)
  AC-6 : Aggregation-monoid : Ok ⊔ Ok = Ok ; Ok ⊔ Degraded = Degraded ;
         Degraded ⊔ Failed = Failed ; _ ⊔ PrimeDirectiveTrip = PrimeDirectiveTrip
         (PrimeDirectiveTrip ALWAYS-WINS ; per § V.2)
  AC-7 : Probe-call-time-budget ≤ 100µs per-crate ; aggregate ≤ 2ms ; non-blocking
  AC-8 : Auto-degradation hooks : `degrade(reason)` flow ; engine-triggers next-frame budget-cut
  AC-9 : `#[ctor]` static registration in HEALTH_REGISTRY ; idempotent ; collision-detect
  AC-10 : Fail-close on PrimeDirectiveTrip ; engine MUST-NOT-continue-frame
  AC-11 : Trait-based discipline : 13 probes register independently ; coordination-via trait-default-impls

§ V.3  Test-target breakdown (~80 tests)

  C1 ACCEPTANCE (~30 tests) :
    - 13 per-subsystem HealthProbe impl-correctness (one-test-per-crate)
    - 8  HealthStatus enum-construction (Ok / Degraded / Failed variants)
    - 5  HealthFailureKind variants (each kind tested)
    - 4  HEALTH_REGISTRY register / unregister / lookup
  C2 PROPERTY (~15 tests) :
    - proptest aggregation-monoid commutativity ∀ (a, b) : a ⊔ b = b ⊔ a
    - proptest aggregation-monoid associativity ∀ (a, b, c) : (a ⊔ b) ⊔ c = a ⊔ (b ⊔ c)
    - proptest PrimeDirectiveTrip ALWAYS-WINS ∀ (a) : a ⊔ PrimeDirectiveTrip = PrimeDirectiveTrip
    - proptest Ok-identity : a ⊔ Ok = a
  C3 GOLDEN-BYTES (~5 tests) :
    - canonical reason-strings stable (subsystem-name + budget_overshoot_pct format)
    - HealthStatus serialization-roundtrip (JSON for MCP preview)
  C4 NEGATIVE (~15 tests) :
    - probe blocking > 100µs ⇒ timeout-degrade-Unknown (LM-11)
    - PrimeDirectiveTrip NEVER-suppressed-by-aggregation (LM-12)
    - probe registration collision ⇒ #[ctor]-guard trips
    - degrade-cascade prevention : one-subsystem degrade ¬ propagates to-others
    - fail-close on PrimeDirectiveTrip : engine-loop-breaks (test via mock-engine)
  C5 COMPOSITION (~15 tests) :
    - 13 probes register without collision
    - aggregator returns worst-case across-13-subsystems
    - cross-subsystem aggregation : (Render=Ok, XR=Degraded) ⇒ Aggregate=Degraded
    - PrimeDirectiveTrip from-any-subsystem ⇒ Aggregate=PrimeDirectiveTrip
    - upstream-failure cascade : XR=UpstreamFailure{ "render" } when render=Failed

§ V.4  Files-to-create + modify

  compiler-rs/crates/cssl-health/                         (NEW)
    Cargo.toml
    src/
      lib.rs                  (~150 LOC ; pub-API)
      status.rs               (~250 LOC ; HealthStatus + HealthFailureKind)
      probe.rs                (~200 LOC ; HealthProbe trait + default-impls)
      registry.rs             (~250 LOC ; HEALTH_REGISTRY + #[ctor])
      aggregate.rs            (~300 LOC ; worst-case-monoid + PrimeDirectiveTrip-always-wins)
      degrade.rs              (~200 LOC ; auto-degradation hooks)
      timeout.rs              (~150 LOC ; probe-call-budget enforcement ≤ 100µs)
    tests/
      Jzeta_3_acceptance/     (~30 test-fns)
      Jzeta_3_property/       (~15 test-fns)
      Jzeta_3_negative/       (~15 test-fns)
      Jzeta_3_composition/    (~15 test-fns)
      Jzeta_3_golden/         (~5 fixtures)

  modify (per AC-4) :
    compiler-rs/crates/cssl-render-v2/src/health.rs       (NEW per-crate ; ~50 LOC)
    compiler-rs/crates/cssl-physics-wave/src/health.rs    (NEW)
    compiler-rs/crates/cssl-wave-solver/src/health.rs     (NEW)
    compiler-rs/crates/cssl-spectral-render/src/health.rs (NEW)
    compiler-rs/crates/cssl-fractal-amp/src/health.rs     (NEW)
    compiler-rs/crates/cssl-gaze-collapse/src/health.rs   (NEW ; PRIME §1 special)
    compiler-rs/crates/cssl-render-companion-perspective/src/health.rs (NEW)
    compiler-rs/crates/cssl-host-openxr/src/health.rs     (NEW)
    compiler-rs/crates/cssl-anim-procedural/src/health.rs (NEW)
    compiler-rs/crates/cssl-wave-audio/src/health.rs      (NEW)
    compiler-rs/crates/cssl-substrate-omega-field/src/health.rs (NEW)
    compiler-rs/crates/cssl-substrate-kan/src/health.rs   (NEW)
    compiler-rs/crates/cssl-engine/src/health.rs          (NEW ; engine_health aggregator)

§ V.5  Implementer-prompt (slice-specific ; full-rendered)

```
ROLE : Implementer for slice T11-D152 (Wave-Jζ-3 : per-subsystem health probes)

POD-ASSIGNMENT : pod-C
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D152-impl-podC

LANE-DISCIPLINE :
  ✓ may-write : compiler-rs/crates/cssl-health/src/** (NEW crate)
  ✓ may-write : compiler-rs/crates/cssl-health/Cargo.toml
  ✓ may-write : 13 subsystem-crates' src/health.rs (NEW per-crate file)
  ✓ may-modify : 13 subsystem-crates' src/lib.rs (re-export probe)
  ✓ may-modify : 13 subsystem-crates' Cargo.toml (additive : cssl-health dep)
  N! may-modify : cssl-metrics/src/** (Wave-Jζ-1 owns)

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § V (lines 649-741)
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § VIII.4 (lines 904-923)
  ‼ PRIME_DIRECTIVE.md §1 + §11 (PrimeDirectiveTrip = ALWAYS-WINS)

ACCEPTANCE-CRITERIA :
  AC-1  : HealthStatus enum (Ok / Degraded / Failed)
  AC-2  : HealthFailureKind enum (DeadlineMiss / ResourceExhaustion / ThermalThrottle /
                                   ConsentViolationDetected / InvariantBreach /
                                   UpstreamFailure / PrimeDirectiveTrip / Unknown)
  AC-3  : HealthProbe trait (name / health / degrade)
  AC-4  : Per-subsystem health() probe in 13 subsystems
  AC-5  : Aggregated engine_health() returns HealthAggregate (worst-case-monoid)
  AC-6  : Aggregation-monoid : PrimeDirectiveTrip ALWAYS-WINS
  AC-7  : Probe-call-time-budget ≤ 100µs per-crate ; aggregate ≤ 2ms
  AC-8  : Auto-degradation hooks (degrade(reason) flow)
  AC-9  : #[ctor] static registration ; idempotent ; collision-detect
  AC-10 : Fail-close on PrimeDirectiveTrip
  AC-11 : Trait-based discipline : 13 probes register independently

DELIVERABLES :
  D1 : cssl-health crate (~1.5K LOC ; 7 modules)
  D2 : 13 per-crate health.rs files (~50 LOC each)
  D3 : own-tests in cssl-health/tests/Jzeta_3_*/
        cargo test -p cssl-health -- --test-threads=1 ⇒ N/N pass
  D4 : commit-message per § XIII template
  D5 : DECISIONS.md slice-entry per § 02 ROLE.Implementer

LANDMINE-FOCUS (slice-specific) :
  ‼ LM-11 health() probe blocking > 100µs ⇒ timeout-with-degraded-fallback
  ‼ LM-12 PrimeDirectiveTrip suppressed-by-aggregation ⇒ ALWAYS-WINS hard-coded

CROSS-CUTTING DISCIPLINE (TRAIT-BASED ; CRITICAL) :
  W! HealthProbe trait lives in cssl-health crate
  W! ∀ subsystem-crate impl-HealthProbe in own-crate (¬ in cssl-health)
  W! ∀ #[ctor] registration in own-crate ; idempotent ; collision-detect via panic-on-double
  W! probe-call-time-budget ENFORCED via timeout-wrapper in cssl-health::timeout
  W! 13-subsystem coordination = TRAIT-DEFAULT-IMPLS pattern :
       - HealthProbe::name() abstract (per-crate static-string)
       - HealthProbe::health() abstract (per-crate logic)
       - HealthProbe::degrade(reason) default-impl (logs Audit-event ; may-override)
  W! 13 subsystems means health-probe-coordination is GENUINELY-COMPLEX :
       - DOC trait-based discipline in module-doc with-rationale
       - DOC pod-iteration cycle if any-probe fails the timeout-budget
       - escalate-to-Architect if cross-subsystem dep-cycle detected
       - cycle-counter ≥ 2 cross-pod-iterations ⇒ DOC architectural-rework-rationale

13 SUBSYSTEMS TO COVER :
  cssl-render-v2 / cssl-physics-wave / cssl-wave-solver / cssl-spectral-render /
  cssl-fractal-amp / cssl-gaze-collapse / cssl-render-companion-perspective /
  cssl-host-openxr / cssl-anim-procedural / cssl-wave-audio /
  cssl-substrate-omega-field / cssl-substrate-kan / cssl-engine (omega_step internal)

PRIME-DIRECTIVE BINDING :
  ‼ PrimeDirectiveTrip = ALWAYS-WINS in aggregation
  ‼ engine MUST-fail-close on PrimeDirectiveTrip (no-continue-frame)
  ‼ degrade-event Audit<"subsystem-degrade"> emitted ; MCP-queryable

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs ; English-prose only-when rustdoc

START-NOW. Dispatch your-first-tool-call after-emitting-§R.
```

§ V.6  Reviewer-prompt (slice-specific ; full-rendered)

```
ROLE : Reviewer for slice T11-D152 (Wave-Jζ-3 : per-subsystem health probes)

POD-ASSIGNMENT : pod-D (D ≠ C = Implementer-pod)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D152-review-podD
PARALLEL-WITH : Implementer

LANE-DISCIPLINE :
  N! production-code
  ✓ .review-report.md only

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § V + § VIII.4
  ‼ PRIME_DIRECTIVE.md §1 + §11

REVIEW-CRITERIA (per § 02 ROLE.Reviewer.§1.6) :
  §1.1 spec-anchor-conformance
  §1.2 API-surface-coherence
  §1.3 invariant-preservation
  §1.4 documentation-presence (rustdoc)
  §1.5 findings-list (HIGH/MED/LOW)
  §1.6 suggested-patches
  §1.7 escalations-raised

REVIEW-FOCUS (Wave-Jζ-3-specific) :
  - aggregation-monoid laws : commutativity ; associativity ; Ok-identity ;
    PrimeDirectiveTrip-absorbing (R! algebraic correctness)
  - 13-subsystem trait-discipline : default-impls cleanly-overridable
  - probe-call-time-budget enforcement : timeout-wrapper integrates without race
  - degrade-cascade-prevention : one subsystem degrade ¬ propagates
  - PRIME-DIRECTIVE fail-close pathway in engine_health-consumer
  - MCP-preview-stub : returns Ok ; wired-real @ Jζ-5
  - HealthFailureKind variants : all 8 variants exposed + handled
  - reason: &'static str : NOT format!() heap-allocations (zero-alloc-discipline)
  - since_frame: u64 : reads from cssl-metrics::ENGINE_FRAME_N
  - upstream-failure cascade : A=UpstreamFailure{B}, B=UpstreamFailure{A} ⇒ no-cycle

DELIVERABLES :
  D1 : .review-report.md
  D2 : DECISIONS.md sub-entry §D. Reviewer-Signoff (id ; pod-D)
  D3 : commit-message trailer "Reviewer-Approved-By: <id> pod-D"

EXPECTED-FINDINGS :
  ≥ 5 specific refs per HIGH-finding
  special-attention to-aggregation-monoid laws (algebraic correctness)
  special-attention to-cycle-detection (cross-subsystem upstream-failure dep-cycle)

ANTI-PATTERN.RUBBER-STAMP :
  ¬ all-✓ within-30s ⇒ pod-rotation
  W! ≥ 5 specific file:line refs per HIGH-finding

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : read § V.2 aggregation-monoid spec ⇒ verify implementation matches.
```

§ V.7  Test-Author-prompt (slice-specific ; full-rendered)

```
ROLE : Test-Author for slice T11-D152 (Wave-Jζ-3 : per-subsystem health probes)

POD-ASSIGNMENT : pod-A (rotation-shift ; ≠ Impl-C, ≠ Rev-D)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D152-test-podA
PARALLEL-WITH : Implementer

LANE-DISCIPLINE :
  N! production-code
  ✓ tests in cssl-health/tests/Jzeta_3_acceptance/
  ✓ tests in cssl-health/tests/Jzeta_3_property/
  ✓ tests in cssl-health/tests/Jzeta_3_golden/
  ✓ tests in cssl-health/tests/Jzeta_3_negative/
  ✓ tests in cssl-health/tests/Jzeta_3_composition/

SPEC-ONLY-INPUT (CRITICAL) :
  ✓ INPUT : `_drafts/phase_j/06_l2_telemetry_spec.md` § V
  ✓ INPUT : PRIME_DIRECTIVE.md §1 + §11
  ✓ INPUT : canonical-API contracts in published cssl-* crates
  N! INPUT : Implementer's-WIP-branch
  N! ASK : Implementer "what's the timeout-policy?"

TEST-CATEGORIES (Wave-Jζ-3-specific ; ~80 total) :
  C1 ACCEPTANCE  : ~30 ;
                   - 13 per-subsystem HealthProbe impl-correctness
                   - 8  HealthStatus enum-construction
                   - 5  HealthFailureKind variants (each kind tested)
                   - 4  HEALTH_REGISTRY register / unregister / lookup
  C2 PROPERTY    : ~15 ;
                   - proptest aggregation-monoid commutativity ∀ (a,b) : a⊔b=b⊔a
                   - proptest associativity ∀ (a,b,c) : (a⊔b)⊔c = a⊔(b⊔c)
                   - proptest PrimeDirectiveTrip ALWAYS-WINS
                   - proptest Ok-identity : a⊔Ok = a
  C3 GOLDEN      : ~5  ;
                   - canonical reason-strings stable
                   - HealthStatus JSON-roundtrip
  C4 NEGATIVE    : ~15 ;
                   - probe blocking > 100µs ⇒ timeout-degrade-Unknown (LM-11)
                   - PrimeDirectiveTrip NEVER-suppressed (LM-12)
                   - probe-registration-collision ⇒ #[ctor]-guard-trips
                   - degrade-cascade-prevention
                   - fail-close on PrimeDirectiveTrip (mock-engine test)
  C5 COMPOSITION : ~15 ;
                   - 13 probes register without collision
                   - aggregator returns worst-case across-13-subsystems
                   - cross-subsystem (Render=Ok, XR=Degraded) ⇒ Aggregate=Degraded
                   - PrimeDirectiveTrip from-any-subsystem ⇒ Aggregate=PrimeDirectiveTrip
                   - upstream-failure : XR=UpstreamFailure{"render"} when render=Failed

SPEC-ONLY-INPUT REMINDER :
  ¬ ask Implementer "what's the timeout-policy?"
  ✓ read § V.4 : "probe-call-time-budget : ≤ 100µs per-crate ⊗ aggregate ≤ 2ms"
  ⇒ test : probe-blocking > 100µs ⇒ timeout-degrade-Unknown

LANDMINE-COVERAGE :
  ‼ LM-11 + LM-12 each ≥ 1 test-fn

DELIVERABLES :
  D1 : test-suite per-slice
  D2 : audit/spec_coverage.toml updated for-§-V mappings
  D3 : DECISIONS.md sub-entry §D. Test-Author-Suite (id ; pod-A)
  D4 : commit-message trailer "Test-Author-Suite-By: <id> pod-A"

ITERATION-DISCIPLINE :
  - tests-fail @ merge-time ⇒ block-merge ; up-to-3-cycles
  - spec-ambiguous ⇒ ESCALATE-Spec-Steward

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : read § V.2 aggregation-monoid spec ⇒ enumerate test-cases.
```

§ V.8  Critic-prompt (slice-specific ; full-rendered)

```
ROLE : Critic for slice T11-D152 (Wave-Jζ-3 : per-subsystem health probes)

POD-ASSIGNMENT : pod-B (rotation-shift ; ≠ Impl-C, ≠ Rev-D, ≠ TA-A)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D152-critic-podB
TIMING : POST-Implementer-final + POST-Reviewer.D2 + POST-Test-Author.D3

LANE-DISCIPLINE :
  N! production-code
  ✓ failure-fixture-tests in cssl-health/tests/Jzeta_3_critic_fixtures/
  ✓ red-team-report.md authored

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § V + § VII
  ‼ PRIME_DIRECTIVE.md §1 + §11
  ‼ Implementer's final-diff + Reviewer's D1 + Test-Author's test-suite

ADVERSARIAL FRAMING :
  W! ≥ 5 attack-attempts catalogued
  N! "looks-good" without effort

ATTACK-FOCUS (Wave-Jζ-3-specific ; ≥ 12 attempts) :
  AT-1  : aggregation-monoid : prove non-commutative pair-or-non-associative-triple
          breaks the engine
  AT-2  : PrimeDirectiveTrip suppressed-by-merge : Ok ⊔ PDTrip = Ok (BUG ?) attempt
  AT-3  : probe blocking > 100µs ⇒ does engine-loop hang ?
  AT-4  : probe panics ⇒ does HEALTH_REGISTRY catch and-degrade-Unknown ?
  AT-5  : cycle in upstream-failure cascade : A=UpstreamFailure{B}, B=UpstreamFailure{A}
          ⇒ does aggregator hang ?
  AT-6  : 14th-subsystem registers post-init ⇒ does collision-detect refuse ?
  AT-7  : degrade(reason) called recursively ⇒ does cascade-prevention hold ?
  AT-8  : engine_health called from-Critical-Path-frame > 2ms ⇒ does it auto-decimate ?
  AT-9  : HealthStatus::Failed { kind: PrimeDirectiveTrip } : engine MUST fail-close ;
          if-engine-continues ⇒ HIGH-severity (PRIME-DIRECTIVE breach)
  AT-10 : reason: &'static str leak via format!() heap-alloc ⇒ zero-alloc-discipline broken ?
  AT-11 : since_frame: u64 read-vs-frame-N race ⇒ stale-value possible ?
  AT-12 : 13-probe HEALTH_REGISTRY iteration order non-deterministic ⇒ aggregation result varies ?

DELIVERABLES :
  D1 : red-team-report.md (per § 02 ROLE.Critic.§2.7)
  D2 : failure-fixture-tests in tests/Jzeta_3_critic_fixtures/
  D3 : DECISIONS.md sub-entry §D. Critic-Veto (id ; pod-B)
  D4 : commit-message trailer "Critic-Veto-Cleared-By: <id> pod-B"

ANTI-PATTERN.PRAISE :
  ¬ "looks-good" without ≥ 5 attempts
  W! zero-findings = RED-FLAG

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs (adversarial-discipline language)

START-NOW. First action : map attack-surface §1 of red-team-report.
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅵ  SLICE Jζ-4 : SPEC-COVERAGE TRACKER
═══════════════════════════════════════════════════════════════════════════════

§ VI.1  Slice metadata

  slice-id        : T11-D153 (Wave-Jζ-4)
  slice-title     : Spec-coverage tracker (`cssl-spec-coverage` ; what works + what doesn't but should)
  spec-anchor     : `_drafts/phase_j/06_l2_telemetry_spec.md` § IV + § VIII.3
  target-crate    : `compiler-rs/crates/cssl-spec-coverage/` (NEW)
                  + `compiler-rs/crates/cssl-spec-coverage-derive/` (NEW ; proc-macro)
                  + cross-cutting #[spec_anchor("...")] attribute additions across-existing-crates
  LOC-target      : ~2K
  tests-target    : ~60 (upgrade from spec-baseline 50 ; +10 chicken-egg coordination tests)
  depends         : Jζ-1 (cssl-metrics types) ; Jζ-2 (citing-metrics need-to-exist)
  blocks          : Jζ-5 (MCP preview)
  pod-rotation    : Impl-D / Rev-A / TA-B / Critic-C
  estimated-impl  : ~45-60 min wall-clock

§ VI.2  Acceptance-criteria

  AC-1 : SpecAnchor struct (spec_root / spec_file / section / criterion / impl_status /
                            test_status / citing_metrics) (per § IV.3)
  AC-2 : ImplStatus enum (Implemented{...} / Partial{...} / Stub{...} / Missing) (per § IV.1)
  AC-3 : TestStatus enum (Tested{...} / Partial{...} / Untested / NoTests{...}) (per § IV.2)
  AC-4 : SpecCoverageRegistry maps Omniverse-spec-§ + CSSLv3-spec-§ → impl-status + test-status
         (per § IV.5)
  AC-5 : Manual registration via #[spec_anchor("...")] proc-macro attribute
  AC-6 : Auto registration via grep-based scanner (extracts code-comments / DECISIONS / test-names)
         (per § IV.4 PRIMARY / SECONDARY / TERTIARY)
  AC-7 : Coverage queries : gap_list() / coverage_for_crate() / impl_of_section() /
         tests_of_section() / impl_without_metrics() / metric_to_spec_anchor() /
         coverage_matrix() / stale_anchors() (per § IV.5)
  AC-8 : SpecCoverageReport for MCP `read_spec_coverage` tool (preview @ Jζ-5)
  AC-9 : Granularity tiers L1 (per-symbol) / L2 (per-acceptance-criterion) /
         L3 (per-§) / L4 (per-spec-file) (per § IV.6)
  AC-10 : Spec-update detection : mtime + content-hash drift (per § IV.7)
  AC-11 : Omniverse spec sections enumerated ; impl + test status tracked
  AC-12 : Report generation : Markdown + JSON ; Perfetto-overlay-track (deferred to L3)

§ VI.3  Test-target breakdown (~60 tests)

  C1 ACCEPTANCE (~20 tests) :
    - 5  SpecAnchor construction + field-access
    - 5  ImplStatus variants
    - 5  TestStatus variants
    - 5  SpecCoverageRegistry CRUD (register / lookup / update / delete)
  C2 PROPERTY (~10 tests) :
    - proptest gap_list() = subset of-impl_status ∈ {Stub, Missing}
    - proptest coverage_matrix() rows-count = registered-anchor-count
    - proptest stale_anchors() ⊆ registered-anchors (subset-property)
  C3 GOLDEN-BYTES (~5 tests) :
    - canonical CoverageMatrix Markdown-output stable (line-by-line golden)
    - canonical CoverageMatrix JSON-output stable
  C4 NEGATIVE (~10 tests) :
    - SpecAnchor without source-of-truth ⇒ build-warn (LM-10)
    - duplicate spec_anchor attribute ⇒ collision-detect ; refuse-second-registration
    - non-existent spec_file in #[spec_anchor("...")] ⇒ build-fail
    - stale-anchor mtime-check : impl-mtime > spec-mtime ⇒ stale-flag
    - coverage-tracker treating Stub as Implemented (LM-XI anti-pattern) ⇒ refused via enum-discriminant
  C5 COMPOSITION (~15 tests) :
    - chicken-egg : add #[spec_anchor("...")] to existing-crate ⇒ scanner picks-up at-build
    - 5  metric_to_spec_anchor backlinks : Jζ-2 metrics resolve to-spec-§
    - 5  Omniverse spec-coverage tracking (real Omniverse/04_OMEGA_FIELD/* enumerated)
    - 5  CSSLv3 specs/22_TELEMETRY tracking (own-spec self-references)

§ VI.4  Files-to-create + modify

  compiler-rs/crates/cssl-spec-coverage/                  (NEW)
    Cargo.toml
    src/
      lib.rs                  (~150 LOC)
      anchor.rs               (~200 LOC ; SpecAnchor + SpecRoot enum)
      status.rs               (~200 LOC ; ImplStatus + TestStatus + ImplConfidence)
      registry.rs             (~250 LOC ; SpecCoverageRegistry + queries)
      extractor.rs            (~250 LOC ; source-of-truth extractor proc-macro)
      scanner.rs              (~250 LOC ; grep-based auto-registration)
      matrix.rs               (~200 LOC ; CoverageMatrix + serializers)
      drift.rs                (~150 LOC ; spec-update detection ; mtime + hash)
      report.rs               (~150 LOC ; SpecCoverageReport for MCP)
    tests/
      Jzeta_4_acceptance/     (~20 test-fns)
      Jzeta_4_property/       (~10 test-fns)
      Jzeta_4_negative/       (~10 test-fns)
      Jzeta_4_composition/    (~15 test-fns)
      Jzeta_4_golden/         (~5 fixtures)

  compiler-rs/crates/cssl-spec-coverage-derive/           (NEW ; proc-macro)
    Cargo.toml
    src/
      lib.rs                  (~250 LOC ; #[spec_anchor("...")] attribute proc-macro)

  CROSS-CUTTING (chicken-egg ; per LANDMINE-NOTE) :
    add #[spec_anchor("...")] attribute to ~30+ existing functions across :
      cssl-engine / cssl-omega-step / cssl-render-v2 / cssl-physics-wave /
      cssl-wave-solver / cssl-spectral-render / cssl-host-openxr /
      cssl-anim-procedural / cssl-wave-audio / cssl-substrate-omega-field /
      cssl-substrate-kan / cssl-gaze-collapse
    each annotation cites Omniverse OR CSSLv3 spec-§ + criterion (when-applicable)

§ VI.5  Implementer-prompt (slice-specific ; full-rendered)

```
ROLE : Implementer for slice T11-D153 (Wave-Jζ-4 : spec-coverage tracker)

POD-ASSIGNMENT : pod-D
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D153-impl-podD

LANE-DISCIPLINE :
  ✓ may-write : compiler-rs/crates/cssl-spec-coverage/src/** (NEW)
  ✓ may-write : compiler-rs/crates/cssl-spec-coverage-derive/src/** (NEW proc-macro)
  ✓ may-modify : ~30 existing-crate fn attributes (CROSS-CUTTING ; bounded scope)
  ✓ may-modify : workspace-Cargo.toml (additive : 2 new crates)
  N! may-modify : OUT-OF-SCOPE crates (limit-touch-radius)
  N! may-touch : cssl-metrics or cssl-health (other-slice's lane)

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § IV (lines 462-647)
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § VIII.3 (lines 881-902)
  ‼ Omniverse spec-corpus + CSSLv3 specs/ for-real-anchor-citations

ACCEPTANCE-CRITERIA :
  AC-1  : SpecAnchor struct (per § IV.3)
  AC-2  : ImplStatus enum (Implemented / Partial / Stub / Missing)
  AC-3  : TestStatus enum (Tested / Partial / Untested / NoTests)
  AC-4  : SpecCoverageRegistry maps Omniverse + CSSLv3 spec-§ → status
  AC-5  : Manual registration via #[spec_anchor("...")] proc-macro
  AC-6  : Auto registration via grep-based scanner (3 source-of-truth tiers)
  AC-7  : Coverage queries (gap_list / coverage_for_crate / impl_of_section / etc.)
  AC-8  : SpecCoverageReport for MCP `read_spec_coverage` tool
  AC-9  : Granularity tiers L1-L4
  AC-10 : Spec-update detection (mtime + content-hash drift)
  AC-11 : Omniverse spec sections enumerated ; impl + test status tracked
  AC-12 : Report generation (Markdown + JSON ; Perfetto deferred to L3)

DELIVERABLES :
  D1 : cssl-spec-coverage crate (~1.5K LOC ; 9 modules)
  D2 : cssl-spec-coverage-derive crate (~250 LOC proc-macro)
  D3 : ~30 #[spec_anchor("...")] annotations across-existing-crates
  D4 : own-tests in cssl-spec-coverage/tests/Jzeta_4_*/
        cargo test -p cssl-spec-coverage -- --test-threads=1 ⇒ N/N pass
  D5 : commit-message per § XIII

LANDMINE-FOCUS (slice-specific ; CHICKEN-EGG-CRITICAL) :
  ‼ ‼ ‼ chicken-egg : #[spec_anchor("...")] attributes ADDED-AS-PART-OF this-slice
       this-is-CROSS-CUTTING change ; large-touch-radius
  ‼ ‼ ‼ extract-discipline : do NOT ad-hoc-attribute every-fn
       cite ONLY Omniverse OR CSSLv3 spec-§ that EXISTS
       if-spec-§ doesn't-exist : ESCALATE-Spec-Steward (do-NOT-fabricate)
  ‼ LM-10 spec-anchor without source-of-truth ⇒ build-warn ⇒ build-fail @ Wave-Jζ-3-final

CHICKEN-EGG STRATEGY :
  PHASE-A : implement extractor + scanner + registry (cssl-spec-coverage/* + derive)
  PHASE-B : add ~30 #[spec_anchor("...")] annotations across-existing-crates
            target-list :
              - cssl-engine : ~3 fns (frame-loop / mode-switch / dropped-frames)
              - cssl-omega-step : ~6 fns (one-per-phase : COLLAPSE / PROPAGATE /
                                            COMPOSE / COHOMOLOGY / AGENCY / ENTROPY)
              - cssl-render-v2 : ~5 fns (one-per-stage-batch : 1-3 / 4-6 / 7-9 / 10-12)
              - cssl-physics-wave : ~3 fns (broadphase / constraint / tier-cascade)
              - cssl-wave-solver : ~3 fns (LBM-collide / LBM-stream / coupling)
              - cssl-spectral-render : ~3 fns (BRDF / iridescence / tonemap)
              - cssl-host-openxr : ~2 fns (frame-submit / tracker-confidence)
              - cssl-anim-procedural : ~2 fns (KAN-eval / IK-solve)
              - cssl-wave-audio : ~2 fns (binaural / underrun)
              - cssl-substrate-omega-field : ~2 fns (cell-allocate / Σ-mask-mutate)
              - cssl-substrate-kan : ~2 fns (eval / distillation)
              - cssl-gaze-collapse : ~2 fns (saccade-predict / privacy-canary)
              total : ~35 ; PM-validates-cite-coverage
  PHASE-C : run scanner @ build ; verify ≥ 30 anchors registered ; CoverageMatrix populated

EXTRACTION-DISCIPLINE :
  W! PRIMARY source : code-comment markers (per § IV.4)
       // § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V phase-COLLAPSE
  W! SECONDARY : DECISIONS.md per-slice spec-anchors
  W! TERTIARY : test-name conventions [crate]_[fn]_per_spec_[file_anchor]
  W! ∀ SpecAnchor ⊗ ≥ one source-of-truth-citation OR build-warn

SCOPE-CREEP-WARNING :
  W! adding #[spec_anchor("...")] to HUNDREDS of fns is OUT-OF-SCOPE
  W! target ~30 high-priority anchors ; remainder = follow-up slice (T11-D###)
  W! choose-priority-via : (a) Omniverse-AXIOM citations first ;
                           (b) M7-floor-related fns next ;
                           (c) PRIME-DIRECTIVE-adjacent third

PRIME-DIRECTIVE BINDING :
  ‼ §1 : aggregate-only spec-coverage report (no per-Sovereign tracking)
  ‼ §11 : attestation-trailer in commit-message
  ‼ if-spec-§ doesn't-exist ⇒ ESCALATE-Spec-Steward (no-fabrication)

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs ; English-prose only-when rustdoc

START-NOW. Dispatch your-first-tool-call after-emitting-§R.
```

§ VI.6  Reviewer-prompt (slice-specific ; full-rendered)

```
ROLE : Reviewer for slice T11-D153 (Wave-Jζ-4 : spec-coverage tracker)

POD-ASSIGNMENT : pod-A (rotation-shift ; ≠ Impl-D)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D153-review-podA
PARALLEL-WITH : Implementer

LANE-DISCIPLINE :
  N! production-code
  ✓ .review-report.md only

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § IV + § VIII.3
  ‼ Omniverse spec-corpus (sample-real-anchor-citations)
  ‼ CSSLv3 specs/ (sample-real-anchor-citations)

REVIEW-CRITERIA (per § 02 ROLE.Reviewer.§1.6) :
  §1.1 spec-anchor-conformance
  §1.2 API-surface-coherence
  §1.3 invariant-preservation
  §1.4 documentation-presence (rustdoc)
  §1.5 findings-list (HIGH/MED/LOW)
  §1.6 suggested-patches
  §1.7 escalations-raised

REVIEW-FOCUS (Wave-Jζ-4-specific) :
  - chicken-egg cross-cutting changes : do annotated-fns actually-cite-real-spec-§ ?
    (spot-check ≥ 5 #[spec_anchor("...")] additions for-fabrication)
  - extractor proc-macro hygiene : doesn't break-existing macro-derives
  - scanner regex correctness : test-name convention parsing
    [crate]_[fn]_per_spec_[file_anchor] format-validation
  - CoverageMatrix Markdown / JSON serialization stability (golden-bytes)
  - L1-L4 granularity tiers : co-queryable / chosen-via-filter-arg
  - drift-detection : mtime + content-hash ; warn-vs-fail thresholds
  - SpecCoverageReport stable across-builds (no random sort-order)
  - gap_list() filters (Stub OR Missing) only ; not Partial
  - impl_without_metrics() correctness : Implemented anchors-without-citing_metrics

DELIVERABLES :
  D1 : .review-report.md
  D2 : DECISIONS.md sub-entry §D. Reviewer-Signoff (id ; pod-A)
  D3 : commit-message trailer "Reviewer-Approved-By: <id> pod-A"

EXPECTED-FINDINGS :
  ≥ 5 specific refs per HIGH-finding
  special-attention to chicken-egg coordination (cross-cutting risk)
  special-attention to fabrication-prevention in #[spec_anchor("...")] additions

ANTI-PATTERN.RUBBER-STAMP :
  ¬ all-✓ within-30s ⇒ pod-rotation
  W! ≥ 5 specific file:line refs per HIGH-finding
  W! spot-check fabrication : pick 5 random anchors ; verify spec-§ EXISTS

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : sample 5 random #[spec_anchor("...")] additions ⇒
                          verify each cites real-existing spec-§.
```

§ VI.7  Test-Author-prompt (slice-specific ; full-rendered)

```
ROLE : Test-Author for slice T11-D153 (Wave-Jζ-4 : spec-coverage tracker)

POD-ASSIGNMENT : pod-B (rotation-shift ; ≠ Impl-D, ≠ Rev-A)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D153-test-podB
PARALLEL-WITH : Implementer

LANE-DISCIPLINE :
  N! production-code
  ✓ tests in cssl-spec-coverage/tests/Jzeta_4_acceptance/
  ✓ tests in cssl-spec-coverage/tests/Jzeta_4_property/
  ✓ tests in cssl-spec-coverage/tests/Jzeta_4_golden/
  ✓ tests in cssl-spec-coverage/tests/Jzeta_4_negative/
  ✓ tests in cssl-spec-coverage/tests/Jzeta_4_composition/

SPEC-ONLY-INPUT (CRITICAL) :
  ✓ INPUT : `_drafts/phase_j/06_l2_telemetry_spec.md` § IV
  ✓ INPUT : Omniverse spec-files (read-existing for-real-anchor-citations)
  ✓ INPUT : CSSLv3 specs/ (read-existing for-real-anchor-citations)
  N! INPUT : Implementer's-WIP-branch
  N! ASK : Implementer "which spec-§ should we anchor first?"

TEST-CATEGORIES (Wave-Jζ-4-specific ; ~60 total) :
  C1 ACCEPTANCE  : ~20 ;
                   - 5  SpecAnchor construction + field-access
                   - 5  ImplStatus variants
                   - 5  TestStatus variants
                   - 5  SpecCoverageRegistry CRUD
  C2 PROPERTY    : ~10 ;
                   - gap_list() = subset of-impl_status ∈ {Stub, Missing}
                   - coverage_matrix() rows-count = registered-anchor-count
                   - stale_anchors() ⊆ registered-anchors (subset-property)
  C3 GOLDEN      : ~5 ;
                   - canonical CoverageMatrix Markdown-output stable
                   - canonical CoverageMatrix JSON-output stable
  C4 NEGATIVE    : ~10 ;
                   - SpecAnchor without source-of-truth ⇒ build-warn (LM-10)
                   - duplicate spec_anchor attribute ⇒ collision-detect
                   - non-existent spec_file in #[spec_anchor("...")] ⇒ build-fail
                   - stale-anchor mtime-check : impl-mtime > spec-mtime ⇒ stale-flag
                   - Stub-as-Implemented bug : enum-discriminant strict-equality enforced
  C5 COMPOSITION : ~15 ;
                   - chicken-egg : add #[spec_anchor("...")] to TEST-CRATE
                     verify scanner picks-it-up at-build
                   - 5  metric_to_spec_anchor backlinks : Jζ-2 metrics resolve
                   - 5  Omniverse spec-coverage tracking
                       (real Omniverse/04_OMEGA_FIELD/* enumerated)
                   - 5  CSSLv3 specs/22_TELEMETRY tracking (own-spec self-references)

SPEC-ONLY-INPUT REMINDER :
  ¬ ask Implementer "which spec-§ should we anchor first?"
  ✓ read § IV.6 : L4-coarse / L3-mid / L2-fine / L1-atomic granularity tiers
  ⇒ test : each-tier independently-queryable

LANDMINE-COVERAGE :
  ‼ LM-10 + Stub-as-Implemented anti-pattern each ≥ 1 test-fn

DELIVERABLES :
  D1 : test-suite per-slice
  D2 : audit/spec_coverage.toml updated for-§-IV mappings
  D3 : DECISIONS.md sub-entry §D. Test-Author-Suite (id ; pod-B)
  D4 : commit-message trailer "Test-Author-Suite-By: <id> pod-B"

ITERATION-DISCIPLINE :
  - tests-fail @ merge-time ⇒ block-merge ; up-to-3-cycles
  - spec-ambiguous ⇒ ESCALATE-Spec-Steward

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : enumerate Omniverse + CSSLv3 spec-§ that-real-anchors-can-cite.
```

§ VI.8  Critic-prompt (slice-specific ; full-rendered)

```
ROLE : Critic for slice T11-D153 (Wave-Jζ-4 : spec-coverage tracker)

POD-ASSIGNMENT : pod-C (rotation-shift ; ≠ Impl-D, ≠ Rev-A, ≠ TA-B)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D153-critic-podC
TIMING : POST-Implementer-final + POST-Reviewer.D2 + POST-Test-Author.D3

LANE-DISCIPLINE :
  N! production-code
  ✓ failure-fixture-tests in cssl-spec-coverage/tests/Jzeta_4_critic_fixtures/
  ✓ red-team-report.md authored

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § IV + § VII
  ‼ Implementer's final-diff + Reviewer's D1 + Test-Author's test-suite

ADVERSARIAL FRAMING :
  W! ≥ 5 attack-attempts catalogued
  N! "looks-good" without effort

ATTACK-FOCUS (Wave-Jζ-4-specific ; ≥ 12 attempts) :
  AT-1  : #[spec_anchor("non-existent-spec-§")] ⇒ does build-fail catch fabrication ?
  AT-2  : duplicate #[spec_anchor("...")] on different fns ⇒ collision-detect ?
  AT-3  : scanner mis-parses test-name ([crate]_[fn]_per_spec_[anchor]) ⇒ regex hardened ?
  AT-4  : mtime-drift attack : touch spec-file post-impl ⇒ stale-flag triggered ?
  AT-5  : SpecAnchor.citing_metrics deleted ⇒ TestStatus auto-degrades to-Untested ?
  AT-6  : CoverageMatrix sort-order non-deterministic across-builds
          ⇒ golden-bytes test fails ?
  AT-7  : CoverageMatrix reports 100% coverage ⇒ does PM-trust-metric or-spot-check ?
  AT-8  : Stub-marked-as-Implemented bug : enum-discriminant strict-equality enforced ?
  AT-9  : circular-import : derive-crate depends on cssl-spec-coverage ⇒ build-fails ?
  AT-10 : annotation-on-private-fn : does scanner skip-it (per § IV.6 "private-impl
          may diverge but file-level module-doc must cite") ?
  AT-11 : spec-§ exists but-EMPTY (no acceptance-criteria) ⇒ status auto-Stub ?
  AT-12 : 100K+ spec-§ stress test : does registry queries scale (O(n) acceptable) ?

DELIVERABLES :
  D1 : red-team-report.md
  D2 : failure-fixture-tests in tests/Jzeta_4_critic_fixtures/
  D3 : DECISIONS.md sub-entry §D. Critic-Veto (id ; pod-C)
  D4 : commit-message trailer "Critic-Veto-Cleared-By: <id> pod-C"

ANTI-PATTERN.PRAISE :
  ¬ "looks-good" without ≥ 5 attempts
  W! zero-findings = RED-FLAG

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs (adversarial-discipline language)

START-NOW. First action : map attack-surface §1 of red-team-report.
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅶ  SLICE Jζ-5 : REPLAY-DETERMINISM PRESERVATION
═══════════════════════════════════════════════════════════════════════════════

§ VII.1  Slice metadata

  slice-id        : T11-D154 (Wave-Jζ-5)
  slice-title     : Replay-determinism preservation (H5 contract for metrics)
  spec-anchor     : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI + § VIII.5
  target-crate    : `compiler-rs/crates/cssl-metrics/` (extend with strict-mode features)
                  + `compiler-rs/crates/cssl-replay-validator/` (NEW)
  LOC-target      : ~1.5K
  tests-target    : ~80 (upgrade from spec-baseline 10 ; +70 determinism-validation tests)
  depends         : Jζ-1 (cssl-metrics types)
  blocks          : nothing internal (final-slice ; gate for Wave-Jθ)
  pod-rotation    : Impl-A / Rev-B / TA-D / Critic-C
  estimated-impl  : ~30-45 min wall-clock

§ VII.2  Acceptance-criteria

  AC-1 : Determinism-strict mode : metrics record to replay-log instead-of-perturbing
         (per § VI.1 DeterminismMode::ReplayStrict)
  AC-2 : Strict-clock primitives : `monotonic_ns()` replaced with
         `(frame_n × FRAME_NS) + sub_phase_ns_offset` (per § VI.2)
  AC-3 : Sub-phase ns-offset : assigned-deterministic-from § V phase-ordering
  AC-4 : Histogram boundaries : `&'static [f64]` compile-time-constant ⇒ deterministic-replay
  AC-5 : Counter monotonic-ops : commutative-saturating-monoid ⇒ deterministic-under-replay
  AC-6 : Multi-thread aggregation : per-thread-shard ⇒ end-of-frame-merge with-deterministic-merge-order
  AC-7 : Sampling : `OneIn(N)` keyed-on `frame_n` (NOT wallclock) ⇒ deterministic
  AC-8 : Forbidden patterns under strict-mode (per § VI.4) :
         - Adaptive sampling refused
         - monotonic_ns direct-call refused (must-route via strict_clock)
         - Welford-online-quantile refused
         - data-driven histogram-boundary refused
         - atomic-relaxed multi-shard race refused (acquire-release with-deterministic-merge)
  AC-9 : Validator : two replay-runs produce bit-equal metric histories
         (cssl-replay-validator runs replay-strict-twice ⇒ diff metric-snapshots ⇒ R! identical)
  AC-10 : H5 contract preserved : existing omega_step bit-determinism MUST-NOT-BREAK
  AC-11 : `omega_step.replay_determinism_check{kind=fail}` should-be 0 ; alarm-on-non-zero
  AC-12 : Metric-history bit-equal across replay runs (validator output : R! identical-byte-sequence)

§ VII.3  Test-target breakdown (~80 tests)

  C1 ACCEPTANCE (~25 tests) :
    - 5  DeterminismMode enum variants (Realtime / ReplayStrict / Mixed)
    - 5  strict_clock returns deterministic (frame_n, sub_phase) → ns
    - 5  Counter monotonic-determinism (10K random-order ops ⇒ same-snapshot)
    - 5  Histogram bucket-assignment determinism (10K record-ops ⇒ same-distribution)
    - 5  Timer record-ns-determinism (use mock-strict-clock)
  C2 PROPERTY (~20 tests) :
    - proptest commutativity of-saturating-monoid Counter.inc_by
    - proptest associativity of-Histogram-sum + count
    - proptest deterministic-sampling : frame_n + tag_hash ⇒ same OneIn(N) result
    - proptest cross-thread aggregation : per-thread-shard merge ⇒ same-final
    - proptest replay-roundtrip : record-N-ops twice ⇒ bit-equal snapshots
  C3 GOLDEN-BYTES (~10 tests) :
    - canonical replay-metric-history bytes for-canonical-input-sequence
    - canonical strict_clock output for (frame_n=N, sub_phase=K)
    - sub_phase ns-offset table per § V phase-ordering
  C4 NEGATIVE (~10 tests) :
    - Adaptive sampling under ReplayStrict ⇒ effect-row refused @ compile-time
    - monotonic_ns direct-call under ReplayStrict ⇒ effect-row refused
    - Welford-online-quantile in strict-mode ⇒ refused
    - data-driven histogram-boundary inference ⇒ refused
    - atomic-relaxed multi-shard race ⇒ refused
  C5 COMPOSITION (~15 tests) :
    - 5 cssl-replay-validator runs full-engine-cycle twice ⇒ bit-equal metric-histories
    - 5 H5 contract : existing omega_step bit-determinism preserved (regression-test)
    - 5 nightly-bench replay-strict-twice ⇒ identical metric-snapshots

§ VII.4  Files-to-create + modify

  compiler-rs/crates/cssl-metrics/                        (MODIFY)
    src/strict_clock.rs       (extend ; ~200 LOC)
    src/sampling.rs            (extend ; ~100 LOC ; deterministic OneIn(N))
    src/timer.rs               (extend ; ~100 LOC ; route monotonic_ns via strict_clock)
    src/histogram.rs           (extend ; ~50 LOC ; static-boundary enforcement)

  compiler-rs/crates/cssl-replay-validator/               (NEW)
    Cargo.toml
    src/
      lib.rs                  (~150 LOC ; pub-API)
      runner.rs               (~250 LOC ; runs replay-strict-twice)
      diff.rs                 (~200 LOC ; bit-equal diff for metric-snapshots)
      bench.rs                (~150 LOC ; nightly-bench harness)
    tests/
      Jzeta_5_acceptance/     (~25 test-fns)
      Jzeta_5_property/       (~20 test-fns)
      Jzeta_5_negative/       (~10 test-fns)
      Jzeta_5_composition/    (~15 test-fns)
      Jzeta_5_golden/         (~10 fixtures)

§ VII.5  Implementer-prompt (slice-specific ; full-rendered)

```
ROLE : Implementer for slice T11-D154 (Wave-Jζ-5 : replay-determinism preservation)

POD-ASSIGNMENT : pod-A (rotation-shift ; second-cycle for-pod-A as-Implementer)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D154-impl-podA

LANE-DISCIPLINE :
  ✓ may-modify : compiler-rs/crates/cssl-metrics/src/strict_clock.rs (extend)
  ✓ may-modify : compiler-rs/crates/cssl-metrics/src/sampling.rs (extend)
  ✓ may-modify : compiler-rs/crates/cssl-metrics/src/timer.rs (extend)
  ✓ may-modify : compiler-rs/crates/cssl-metrics/src/histogram.rs (extend)
  ✓ may-write : compiler-rs/crates/cssl-replay-validator/src/** (NEW)
  ✓ may-modify : workspace-Cargo.toml (additive)
  N! may-modify : cssl-metrics-derive (Wave-Jζ-1 owns ; no-API break)
  N! may-modify : cssl-spec-coverage (Wave-Jζ-4 owns)
  N! may-modify : cssl-health (Wave-Jζ-3 owns)

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § VI (lines 743-803)
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § VIII.5 (lines 925-942)
  ‼ Omniverse/03_RUNTIME/01_COMPUTE_GRAPH.csl.md § V phase-ordering
  ‼ DECISIONS.md H5 contract entries (replay-determinism baseline)

ACCEPTANCE-CRITERIA :
  AC-1  : Determinism-strict mode : metrics record to replay-log
  AC-2  : Strict-clock primitives : monotonic_ns ↦ (frame_n × FRAME_NS) + sub_phase_ns_offset
  AC-3  : Sub-phase ns-offset deterministic-from-§-V phase-ordering
  AC-4  : Histogram boundaries `&'static [f64]` ⇒ deterministic-replay
  AC-5  : Counter monotonic-ops commutative-saturating-monoid ⇒ deterministic
  AC-6  : Multi-thread aggregation per-thread-shard ⇒ deterministic-merge
  AC-7  : Sampling OneIn(N) keyed-on frame_n ⇒ deterministic
  AC-8  : Forbidden patterns refused (Adaptive / monotonic_ns direct / Welford /
                                       data-driven boundaries / atomic-relaxed-race)
  AC-9  : Validator : two replay-runs produce bit-equal metric histories
  AC-10 : H5 contract preserved (no regression)
  AC-11 : omega_step.replay_determinism_check{kind=fail} = 0
  AC-12 : Metric-history bit-equal across replay runs

DELIVERABLES :
  D1 : cssl-metrics extensions (~450 LOC across 4 files)
  D2 : cssl-replay-validator crate (~750 LOC ; 4 modules)
  D3 : own-tests in cssl-replay-validator/tests/Jzeta_5_*/
        cargo test -p cssl-replay-validator -- --test-threads=1 ⇒ N/N pass
  D4 : commit-message per § XIII

LANDMINE-FOCUS (slice-specific ; H5-CRITICAL) :
  ‼ ‼ ‼ LM-1 wallclock-direct-call in-strict-mode ⇒ effect-row gate
  ‼ ‼ ‼ LM-2 adaptive sampling-discipline ⇒ refused-by-effect-row in strict-mode
  ‼ ‼ ‼ LANDMINE-NOTE : timing-based metrics MUST use logical-frame-N ¬ wall-clock

LOGICAL-FRAME-N DISCIPLINE (CRITICAL) :
  W! ALL timing-based metric-recordings under-strict-mode use :
    monotonic_ns() ↦ (frame_n × FRAME_NS) + sub_phase_ns_offset
  W! sub_phase_ns_offset table (per § V phase-ordering) :
    COLLAPSE     : offset =  0 ns ; budget = 4ms
    PROPAGATE    : offset =  4_000_000 ns ; budget = 4ms
    COMPOSE      : offset =  8_000_000 ns ; budget = 2ms
    COHOMOLOGY   : offset = 10_000_000 ns ; budget = 2ms
    AGENCY       : offset = 12_000_000 ns ; budget = 2ms
    ENTROPY      : offset = 14_000_000 ns ; budget = 2ms
  W! sub_phase_ns_offset deterministic-from-spec § V ; build-time-constant
  W! strict_clock indirection ENFORCED via effect-row check :
    {ReplayStrict} effect-row ⇒ monotonic_ns direct-call REFUSED
    only-allowed : strict_clock(frame_n, sub_phase) ⇒ deterministic-ns

H5 CONTRACT PRESERVATION :
  W! existing omega_step.replay_determinism_check{kind=pass} continues-to-tick
  W! existing omega_step.replay_determinism_check{kind=fail} stays-at-zero
  W! cssl-replay-validator runs full-engine-cycle twice ⇒ bit-equal metric-histories
  W! existing-omega_step tests MUST-NOT-regress

FORBIDDEN PATTERNS ENFORCEMENT (per § VI.4) :
  W! Adaptive ⇒ {ReplayStrict} × {Adaptive} = type-error
  W! monotonic_ns direct ⇒ effect-row refused under {ReplayStrict}
  W! Welford ⇒ effect-row refused under {ReplayStrict}
  W! data-driven histogram-boundary ⇒ only &'static [f64] accepted
  W! atomic-relaxed multi-shard race ⇒ acquire-release with-deterministic-merge

PRIME-DIRECTIVE BINDING :
  ‼ replay-determinism = consent-to-truthful-self-reporting
  ‼ H5 contract = engine-can-be-replayed = sovereignty-of-record-keeping
  ‼ §11 attestation in commit-message

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs ; English-prose only-when rustdoc

START-NOW. Dispatch your-first-tool-call after-emitting-§R.
```

§ VII.6  Reviewer-prompt (slice-specific ; full-rendered)

```
ROLE : Reviewer for slice T11-D154 (Wave-Jζ-5 : replay-determinism preservation)

POD-ASSIGNMENT : pod-B (B ≠ A = Implementer-pod)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D154-review-podB
PARALLEL-WITH : Implementer

LANE-DISCIPLINE :
  N! production-code
  ✓ .review-report.md only

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § VI + § VIII.5
  ‼ Omniverse/03_RUNTIME/01_COMPUTE_GRAPH.csl.md § V phase-ordering
  ‼ DECISIONS.md H5 contract entries

REVIEW-CRITERIA (per § 02 ROLE.Reviewer.§1.6) :
  §1.1 spec-anchor-conformance
  §1.2 API-surface-coherence
  §1.3 invariant-preservation (H5 PRESERVED)
  §1.4 documentation-presence (rustdoc cited)
  §1.5 findings-list (HIGH/MED/LOW)
  §1.6 suggested-patches
  §1.7 escalations-raised

REVIEW-FOCUS (Wave-Jζ-5-specific) :
  - strict_clock indirection covers ALL timing-based-records (no-leaks to-monotonic_ns)
  - sub_phase ns-offset table matches § V phase-ordering exactly
        COLLAPSE: 0 / PROPAGATE: 4M / COMPOSE: 8M / COHOMOLOGY: 10M /
        AGENCY: 12M / ENTROPY: 14M (in-ns)
  - effect-row {ReplayStrict} × {Adaptive} compile-error verified
  - cssl-replay-validator runs deterministically (mock-engine ⇒ bit-equal output)
  - H5 contract regression-test : existing tests still-pass
  - sub_phase_ns_offset overflow-handling : (frame_n × FRAME_NS) saturating-arithmetic
  - cross-thread shard-merge order deterministic (acquire-release ; not-relaxed)
  - feature-flag `replay-strict` correctly-gates strict-mode behavior

DELIVERABLES :
  D1 : .review-report.md
  D2 : DECISIONS.md sub-entry §D. Reviewer-Signoff (id ; pod-B)
  D3 : commit-message trailer "Reviewer-Approved-By: <id> pod-B"

EXPECTED-FINDINGS :
  ≥ 5 specific refs per HIGH-finding
  special-attention to wall-clock vs logical-frame-N discipline (LANDMINE-CRITICAL)
  special-attention to H5 regression-test (existing-tests must-not-break)

ANTI-PATTERN.RUBBER-STAMP :
  ¬ all-✓ within-30s ⇒ pod-rotation
  W! ≥ 5 specific file:line refs per HIGH-finding

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : read § VI.2 strict-clock spec ⇒ verify implementation matches.
```

§ VII.7  Test-Author-prompt (slice-specific ; full-rendered)

```
ROLE : Test-Author for slice T11-D154 (Wave-Jζ-5 : replay-determinism preservation)

POD-ASSIGNMENT : pod-D (rotation-shift ; ≠ Impl-A, ≠ Rev-B)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D154-test-podD
PARALLEL-WITH : Implementer

LANE-DISCIPLINE :
  N! production-code
  ✓ tests in cssl-replay-validator/tests/Jzeta_5_acceptance/
  ✓ tests in cssl-replay-validator/tests/Jzeta_5_property/
  ✓ tests in cssl-replay-validator/tests/Jzeta_5_golden/
  ✓ tests in cssl-replay-validator/tests/Jzeta_5_negative/
  ✓ tests in cssl-replay-validator/tests/Jzeta_5_composition/

SPEC-ONLY-INPUT (CRITICAL) :
  ✓ INPUT : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI
  ✓ INPUT : Omniverse/03_RUNTIME/01_COMPUTE_GRAPH.csl.md § V phase-ordering
  ✓ INPUT : DECISIONS.md H5 entries
  N! INPUT : Implementer's-WIP-branch
  N! ASK : Implementer "what's the sub_phase offset for COLLAPSE?"

TEST-CATEGORIES (Wave-Jζ-5-specific ; ~80 total) :
  C1 ACCEPTANCE  : ~25 ;
                   - 5  DeterminismMode enum variants
                   - 5  strict_clock returns deterministic (frame_n, sub_phase) → ns
                   - 5  Counter monotonic-determinism (10K random-order ⇒ same-snapshot)
                   - 5  Histogram bucket-assignment determinism
                   - 5  Timer record-ns-determinism (mock-strict-clock)
  C2 PROPERTY    : ~20 ;
                   - proptest commutativity of-Counter.inc_by
                   - proptest associativity of-Histogram-sum + count
                   - proptest deterministic-sampling : frame_n + tag_hash ⇒ same OneIn(N)
                   - proptest cross-thread aggregation : per-thread-shard merge ⇒ same-final
                   - proptest replay-roundtrip : record-N-ops twice ⇒ bit-equal
  C3 GOLDEN      : ~10 ;
                   - canonical replay-metric-history bytes for-canonical-input-sequence
                   - canonical strict_clock output for (frame_n=N, sub_phase=K)
                   - sub_phase ns-offset table per § V phase-ordering
  C4 NEGATIVE    : ~10 ;
                   - Adaptive sampling under ReplayStrict ⇒ effect-row refused (compile-fail)
                   - monotonic_ns direct under ReplayStrict ⇒ effect-row refused
                   - Welford-online-quantile in strict-mode ⇒ refused
                   - data-driven histogram-boundary inference ⇒ refused
                   - atomic-relaxed multi-shard race ⇒ refused
  C5 COMPOSITION : ~15 ;
                   - 5 cssl-replay-validator runs full-engine-cycle twice ⇒ bit-equal
                   - 5 H5 contract : existing omega_step bit-determinism preserved
                   - 5 nightly-bench replay-strict-twice ⇒ identical metric-snapshots

SPEC-ONLY-INPUT REMINDER :
  ¬ ask Implementer "what's the sub_phase offset for COLLAPSE?"
  ✓ read § V phase-ordering : COLLAPSE first ; offset = 0 ns
  ⇒ test : strict_clock(frame_n=0, COLLAPSE) returns 0
        ; strict_clock(frame_n=1, COLLAPSE) returns FRAME_NS
        ; strict_clock(frame_n=0, PROPAGATE) returns 4_000_000

LANDMINE-COVERAGE :
  ‼ LM-1 + LM-2 + forbidden-patterns-§VI.4 each ≥ 1 test-fn

DELIVERABLES :
  D1 : test-suite per-slice
  D2 : audit/spec_coverage.toml updated for-§-VI mappings
  D3 : DECISIONS.md sub-entry §D. Test-Author-Suite (id ; pod-D)
  D4 : commit-message trailer "Test-Author-Suite-By: <id> pod-D"

ITERATION-DISCIPLINE :
  - tests-fail @ merge-time ⇒ block-merge ; up-to-3-cycles
  - spec-ambiguous ⇒ ESCALATE-Spec-Steward

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : enumerate forbidden-patterns from § VI.4 ⇒ author trybuild compile-fail tests.
```

§ VII.8  Critic-prompt (slice-specific ; full-rendered)

```
ROLE : Critic for slice T11-D154 (Wave-Jζ-5 : replay-determinism preservation)

POD-ASSIGNMENT : pod-C (rotation-shift ; ≠ Impl-A, ≠ Rev-B, ≠ TA-D)
ORCHESTRATOR-WORKTREE : .claude/worktrees/T11-D154-critic-podC
TIMING : POST-Implementer-final + POST-Reviewer.D2 + POST-Test-Author.D3

LANE-DISCIPLINE :
  N! production-code
  ✓ failure-fixture-tests in cssl-replay-validator/tests/Jzeta_5_critic_fixtures/
  ✓ red-team-report.md authored

SPEC-ANCHOR (READ-FIRST) :
  ‼ `_drafts/phase_j/06_l2_telemetry_spec.md` § VI + § VII
  ‼ Omniverse/03_RUNTIME/01_COMPUTE_GRAPH.csl.md § V phase-ordering
  ‼ DECISIONS.md H5 entries
  ‼ Implementer's final-diff + Reviewer's D1 + Test-Author's test-suite

ADVERSARIAL FRAMING :
  W! ≥ 5 attack-attempts catalogued
  N! "looks-good" without effort

ATTACK-FOCUS (Wave-Jζ-5-specific ; ≥ 12 attempts) :
  AT-1  : monotonic_ns direct-call under ReplayStrict ⇒ refused via strict_clock ? (LM-1)
  AT-2  : Adaptive sampling under ReplayStrict ⇒ refused @ effect-row ? (LM-2)
  AT-3  : Welford-online-quantile in strict-mode ⇒ refused @ effect-row ?
  AT-4  : replay-twice DIFF-bytes : inject non-determinism (HashMap iter-order)
          ⇒ does validator catch ?
  AT-5  : cross-thread shard-merge non-deterministic (atomic Relaxed) ⇒ validator catch ?
  AT-6  : sub_phase ns-offset wraparound (frame_n × FRAME_NS overflows u64)
          ⇒ does saturating-arith catch ?
  AT-7  : H5 regression : break omega_step.replay_determinism_check{kind=pass}
          ⇒ does cssl-replay-validator alarm ?
  AT-8  : timestamp-precision : nanosecond-level jitter
          ⇒ does histogram-bucket-assignment differ ?
  AT-9  : sampling OneIn(N) keyed on wallclock NOT frame_n ⇒ caught at-compile ?
  AT-10 : data-driven histogram-boundary inference ⇒ refused ?
  AT-11 : feature-flag `replay-strict` OFF ⇒ does Realtime-mode work as before ?
          (regression test : non-strict path unaffected)
  AT-12 : Mixed mode : structural-counters strict + latency-only relaxed ⇒
          does feature-warn flag emit when warn:true ?

DELIVERABLES :
  D1 : red-team-report.md
  D2 : failure-fixture-tests in tests/Jzeta_5_critic_fixtures/
  D3 : DECISIONS.md sub-entry §D. Critic-Veto (id ; pod-C)
  D4 : commit-message trailer "Critic-Veto-Cleared-By: <id> pod-C"

ANTI-PATTERN.PRAISE :
  ¬ "looks-good" without ≥ 5 attempts
  W! zero-findings = RED-FLAG

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs (adversarial-discipline language)

START-NOW. First action : map attack-surface §1 of red-team-report.
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅶ.bis  PER-SLICE SAMPLE-CODE SCAFFOLDING
═══════════════════════════════════════════════════════════════════════════════

§ Ⅶ.bis.1  Jζ-1 sample : Counter API skeleton (cssl-metrics/src/counter.rs)

```rust
// § _drafts/phase_j/06_l2_telemetry_spec.md § II.1 Counter
#![cfg_attr(feature = "replay-strict", deny(unused_must_use))]

use core::sync::atomic::{AtomicU64, Ordering};
use smallvec::SmallVec;
use crate::tag::{TagKey, TagVal};
use crate::sampling::SamplingDiscipline;
use crate::schema::TelemetrySchemaId;
use crate::error::{MetricError, MetricResult};

/// Counter : monotonic-non-decreasing (per § II.1 spec ; LM-2 + LM-13).
///
/// Cite : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.1
pub struct Counter {
    pub name: &'static str,
    pub value: AtomicU64,
    pub tags: SmallVec<[(TagKey, TagVal); 4]>,
    pub sampling: SamplingDiscipline,
    pub schema_id: TelemetrySchemaId,
}

impl Counter {
    /// Increment by 1. Saturating on u64::MAX (per LM-13 ; emits Audit<"counter-overflow">).
    pub fn inc(&self) -> MetricResult<()> /* { Telemetry<Counters> } */ {
        self.inc_by(1)
    }

    pub fn inc_by(&self, n: u64) -> MetricResult<()> /* { Telemetry<Counters> } */ {
        let prev = self.value.fetch_add(n, Ordering::Relaxed);
        if prev.checked_add(n).is_none() {
            // overflow-saturating ; audit-event
            return Err(MetricError::Overflow);
        }
        Ok(())
    }

    /// SET semantically tagged-as-RESET-EVENT (per § II.1 ; audit-chain logs reset).
    pub fn set(&self, v: u64) -> MetricResult<()> /* { Telemetry<Counters>, Audit<'reset'> } */ {
        self.value.store(v, Ordering::Relaxed);
        Ok(())
    }

    /// /{Pure} — read-only ; no-effect-row required.
    pub fn snapshot(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}
```

§ Ⅶ.bis.2  Jζ-2 sample : per-stage Timer wiring (cssl-render-v2/src/stage_5.rs)

```rust
// § _drafts/phase_j/06_l2_telemetry_spec.md § III.3 render.stage_time_ns
// § Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III stage-5 (SDF-march)
use cssl_metrics::{Timer, register_metric};
use cssl_metrics::tag::stage_tag;

#[ctor::ctor]
static RENDER_STAGE_5_TIMER: Timer = register_metric!(
    name = "render.stage_time_ns",
    typ = Timer,
    tags = [stage_tag(5)],
    sampling = SamplingDiscipline::Always,
    spec_anchor = "06_l2_telemetry_spec.md § III.3 stage-5"
);

#[spec_anchor("omniverse:07_AESTHETIC/06_RENDERING_PIPELINE§III.stage-5")]
#[cite_metrics(["render.stage_time_ns{stage=5}"])]
pub fn run_stage_5(ctx: &mut RenderContext) -> StageResult
  /* / { GPU<async-compute>, Realtime<90Hz>, Telemetry<Counters> } */
{
    let _t = RENDER_STAGE_5_TIMER.start();
    // ... stage-5 logic ...
    StageResult::Ok
}
```

§ Ⅶ.bis.3  Jζ-3 sample : HealthProbe impl (cssl-render-v2/src/health.rs)

```rust
// § _drafts/phase_j/06_l2_telemetry_spec.md § V HealthProbe trait
use cssl_health::{HealthProbe, HealthStatus, HealthFailureKind, HealthError, HEALTH_REGISTRY};
use cssl_metrics::{ENGINE_FRAME_N, Timer};

pub struct RenderV2Probe;

impl HealthProbe for RenderV2Probe {
    fn name(&self) -> &'static str { "cssl-render-v2" }

    fn health(&self) -> HealthStatus
      /* / { Telemetry<Counters>, Pure } */
    {
        let p99 = crate::stage_metrics::RENDER_STAGE_TIME_NS.percentile(99.0);
        let frame_budget = crate::FRAME_BUDGET_NS;
        if p99 > frame_budget {
            HealthStatus::Degraded {
                reason: "render-stage-budget overshoot",
                budget_overshoot_pct: ((p99 as f32 / frame_budget as f32 - 1.0) * 100.0),
                since_frame: ENGINE_FRAME_N.snapshot(),
            }
        } else {
            HealthStatus::Ok
        }
    }

    fn degrade(&self, reason: &str) -> Result<(), HealthError>
      /* / { Audit<'subsystem-degrade'> } */
    {
        // trigger next-frame DFR-budget-cut
        crate::degrade::trigger_dfr_cut(reason)
    }
}

#[ctor::ctor]
fn register_probe() {
    HEALTH_REGISTRY.register(Box::new(RenderV2Probe));
}
```

§ Ⅶ.bis.4  Jζ-4 sample : SpecAnchor + #[spec_anchor] usage

```rust
// § _drafts/phase_j/06_l2_telemetry_spec.md § IV.3 SpecAnchor
use cssl_spec_coverage_derive::spec_anchor;
use cssl_metrics::Timer;

// PRIMARY source-of-truth : code-comment marker
// § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V phase-COLLAPSE ≤ 4ms
#[spec_anchor("omniverse:04_OMEGA_FIELD/05_DENSITY_BUDGET§V.1")]
#[cite_metrics(["omega_step.phase_time_ns{phase=COLLAPSE}"])]
pub fn collapse_phase(omega: &mut Omega, observations: &[Observation])
  -> Result<CollapsedRegions, CollapseError>
  /* / { GPU<async-compute>, Realtime<90Hz>, Deadline<4ms>,
         DetRNG, EntropyBalanced, Audit<'tick>, Telemetry<Counters> } */
{
    let _t = OMEGA_STEP_PHASE_TIME_NS
        .with_tag(("phase", "COLLAPSE"))
        .start();
    // ... collapse logic ...
    Ok(CollapsedRegions::default())
}

// Test-name-convention TERTIARY source :
// fn omega_step_collapse_phase_per_spec_05_density_budget() { ... }
//                                       ^^^^^^^^^^^^^^^^^ regex-extracted spec-§ anchor
```

§ Ⅶ.bis.5  Jζ-5 sample : strict_clock indirection (cssl-metrics/src/strict_clock.rs)

```rust
// § _drafts/phase_j/06_l2_telemetry_spec.md § VI.2 strict-clock primitives
use core::sync::atomic::{AtomicU64, Ordering};

/// Per-frame nanoseconds @ 60Hz (16.666ms = 16,666,667 ns) ; 90Hz (11.11ms) ; 120Hz (8.33ms).
pub const FRAME_NS_60HZ:  u64 = 16_666_667;
pub const FRAME_NS_90HZ:  u64 = 11_111_111;
pub const FRAME_NS_120HZ: u64 = 8_333_333;

/// Sub-phase ns-offset table per § V phase-ordering (deterministic ; build-time-constant).
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum SubPhase {
    Collapse   = 0,
    Propagate  = 1,
    Compose    = 2,
    Cohomology = 3,
    Agency     = 4,
    Entropy    = 5,
}

pub const SUB_PHASE_NS_OFFSETS: [u64; 6] = [
    0,           // COLLAPSE   : offset 0     ; budget 4ms
    4_000_000,   // PROPAGATE  : offset 4M    ; budget 4ms
    8_000_000,   // COMPOSE    : offset 8M    ; budget 2ms
    10_000_000,  // COHOMOLOGY : offset 10M   ; budget 2ms
    12_000_000,  // AGENCY     : offset 12M   ; budget 2ms
    14_000_000,  // ENTROPY    : offset 14M   ; budget 2ms
];

/// Deterministic clock for ReplayStrict mode.
/// Replaces monotonic_ns() under {ReplayStrict} effect-row.
#[cfg(feature = "replay-strict")]
pub fn strict_clock(frame_n: u64, sub_phase: SubPhase) -> u64 {
    let frame_ns = frame_n.saturating_mul(FRAME_NS_90HZ);  // saturate on overflow (per LM landmine)
    let sub_offset = SUB_PHASE_NS_OFFSETS[sub_phase as usize];
    frame_ns.saturating_add(sub_offset)
}

/// monotonic_ns() — REFUSED under ReplayStrict by effect-row check.
#[cfg(not(feature = "replay-strict"))]
pub fn monotonic_ns() -> u64 /* / { Realtime } */ {
    use std::time::Instant;
    Instant::now().elapsed().as_nanos() as u64
}
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅷ  WAVE DISPATCH ORDER
═══════════════════════════════════════════════════════════════════════════════

§ VIII.1  Phased dispatch (sequential-where-required + parallel-where-possible)

  PHASE-1 : Wave-Jζ-1 dispatched-first (foundational ; others use cssl-metrics)
    pod-batch : 4 agents (Impl-A + Rev-B + TA-C + Critic-D)
    estimated wall-clock : ~50-90 min @ no-iteration
    blocks : everything else in Wave-Jζ
    gate-to-proceed : G1..G5 (5-of-5) per § 02 § 6 enforcement

  PHASE-2 : Wave-Jζ-2..5 dispatched-IN-PARALLEL after-Jζ-1-lands
    EXCEPT : Jζ-4 waits-for Jζ-2 (citing-metrics dependency)
    pod-batches :
      Jζ-2 : Impl-B + Rev-C + TA-D + Critic-A    (4 agents)
      Jζ-3 : Impl-C + Rev-D + TA-A + Critic-B    (4 agents)
      Jζ-5 : Impl-A + Rev-B + TA-D + Critic-C    (4 agents)
    parallel-fanout : 12 agents simultaneously (Phase-2a)
    Jζ-4 dispatch : after-Jζ-2-lands ; 4 agents (Impl-D + Rev-A + TA-B + Critic-C)
    estimated wall-clock-Phase-2a : ~50-90 min
    estimated wall-clock-Phase-2b (Jζ-4) : ~30-60 min

  PHASE-3 : Cross-cutting validation
    Validator runs full-spec-conformance battery (per § 02 ROLE.Validator)
    Architect coherence-pass cross-slice
    Spec-Steward DECISIONS-entries finalized
    ETA : ~20-30 min wall-clock

§ VIII.2  Total wave-Jζ wall-clock estimate

  PHASE-1 + PHASE-2a + PHASE-2b + PHASE-3 ≈ 150-270 min @ no-iteration
  iteration-pessimistic (1-cycle Critic on-each-slice) : 300-450 min
  W! cycle-counter ≤ 3 per-slice ; ≥ 4 ⇒ MANDATORY-escalation-to-Apocky

§ VIII.3  Cross-pod rotation table (canonical)

  per § I.6 above ; documented at-dispatch-time in PM-output

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅸ  INTER-SLICE COORDINATION
═══════════════════════════════════════════════════════════════════════════════

§ IX.1  All metrics use cssl-metrics types

  W! ∀ slice Jζ-2..5 : Counter / Gauge / Histogram / Timer FROM cssl-metrics (no-duplication)
  W! per-subsystem record-sites construct via cssl-metrics::* (no-private-helpers)
  W! schema-derive via cssl-metrics::TelemetrySchema (single-canonical-derive)

§ IX.2  All metrics record to cssl-telemetry ring-buffer (no double-record)

  W! cssl-metrics::Counter::inc ⇒ TelemetryRing slot (Slot.scope = Counters)
  W! cssl-metrics::Histogram::record ⇒ TelemetryRing slot (Slot.scope = Counters)
  W! cssl-metrics::Timer::record_ns ⇒ TelemetryRing slot (Slot.scope = Counters)
  N! double-record : if cssl-metrics records ⇒ subsystem-crate must-NOT also record-direct
  W! audit-chain : single-record per-event ⇒ no duplicate-events in audit-trail

§ IX.3  Spec-coverage tracker citing-metrics field

  W! Jζ-4 SpecAnchor.citing_metrics ⊆ Jζ-2-registered-metrics (subset-check at-build)
  W! deletion-of-cited-metric ⇒ TestStatus auto-degrades-to-Untested (per § IV.3 spec-rule)

§ IX.4  Health-probe / metric coupling

  W! Jζ-3 HealthProbe::health() may-read Jζ-2-metrics (via cssl-metrics::REGISTRY queries)
  W! Jζ-3 HealthProbe MAY-NOT-write metrics (lane-discipline ; only own-crate's-record-sites)
  W! Jζ-3 health-check timeout = ≤ 100µs ; metric-snapshot read ≤ 10µs (per § V.4 budget)

§ IX.5  Replay-determinism applies-to ALL slices

  W! ∀ slice Jζ-1..4 metrics MUST honor Jζ-5 ReplayStrict mode
  W! ∀ slice Jζ-1..4 timing-based-records MUST use strict_clock under ReplayStrict
  W! Jζ-5 cssl-replay-validator runs cross-cutting ⇒ catches Jζ-1..4 violations

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅹ  PRE-MERGE GATE
═══════════════════════════════════════════════════════════════════════════════

§ X.1  5-of-5 gate per slice (per § 02 ROLE-spec § 6.1)

  G1 : Implementer-self-test green
       cargo test -p <slice-crate> -- --test-threads=1 = N/N pass
       cargo clippy -p <slice-crate> --all-targets -- -D warnings = clean
       cargo fmt --all-check = clean
  G2 : Reviewer-sign-off (Reviewer.D2 in DECISIONS slice-entry)
       spec-anchor-conformance ✓ ; API-coherence ✓ ; invariant-preservation ✓ ; HIGH-resolved
  G3 : Critic-veto-cleared (Critic.D3 in DECISIONS slice-entry)
       veto-flag = FALSE ; HIGH-resolved ; failure-fixtures pass
  G4 : Test-Author-tests-passing (Test-Author.D3 + actual-test-run)
       cross-pod confirmed (Implementer-pod ≠ Test-Author-pod ≠ Reviewer-pod ≠ Critic-pod)
       spec-§-coverage = 100%
  G5 : Validator-spec-conformance (Validator.D2 in DECISIONS slice-entry)
       spec-§-resolved : 100% ; test-coverage-map populated
       gap-list-A (spec'd-but-not-impl) = zero-HIGH
       gap-list-B (impl-but-not-spec'd) = zero-HIGH
       verdict = APPROVE

§ X.2  Wave-Jζ specific gates (per § X spec-anchor)

  GATE-1 : `cssl-metrics` crate ⊗ ≥ 100 tests ⊗ all-pass ⊗ ≤ 0.5% overhead-bench
  GATE-2 : per-subsystem catalog ⊗ 100% coverage of-§-III table ⊗ registration-completeness check passes
  GATE-3 : `cssl-spec-coverage` ⊗ scans-all-CSSLv3-specs + Omniverse-axioms ⊗ generates CoverageMatrix
  GATE-4 : `engine.health()` ⊗ returns Ok @ M7-floor-pass scenario ⊗ FailedClose-on-PrimeDirectiveTrip
  GATE-5 : MCP-preview-stubs registered ⊗ JSON-roundtrip tests pass
  GATE-6 : nightly-bench ⊗ replay-strict-twice ⊗ metric-snapshots bit-identical
  GATE-7 : zero `gaze.privacy_egress_attempts_refused` increments under any-canonical-playtest
  GATE-8 : zero biometric-tag-keys compile-pass ⊗ ALL-15-LM landmines exercised in tests

§ X.3  Pre-merge mechanical enforcement

  W! pre-merge-script (scripts/check_5_of_5_gate.sh) parses DECISIONS slice-entry
  W! confirms G1..G5 sub-entries APPROVE state
  W! confirms cross-pod-discipline recorded (4 different pods)
  W! confirms commit-message-trailer has all-5 sign-offs (Implementer / Reviewer / Critic / Test-Author / Validator)
  W! fail-any ⇒ exit-1 ⇒ pre-commit-hook blocks merge

§ X.4  Wave-Jζ pre-merge specific checks

  Spec-coverage report generated successfully (Jζ-4 deliverable D2 = SpecCoverageReport)
  Replay-determinism verified bit-equal (Jζ-5 cssl-replay-validator output : R! identical-bytes)
  Catalog-completeness (Jζ-2 REGISTRY.completeness_check vs § III table = 100%)
  Health-aggregator (Jζ-3 engine_health() returns Ok in M7-floor scenario)

§ X.5  Iteration-bounds (per § 03 § II Iteration-trigger table)

  Reviewer-HIGH         : iterate same-pod-cycle ; ≤ 3 cycles ⇒ Architect
  Critic-HIGH-veto      : iterate NEW-pod-cycle ; ≤ 3 cycles ⇒ Architect
  Validator-spec-drift  : iterate same-pod-cycle (impl-wrong) OR Spec-Steward (spec-wrong) ; ≤ 2 cycles
  Test-Author-fail      : iterate same-pod-cycle ; ≤ 3 cycles ⇒ Architect
  Cycle ≥ 4             : MANDATORY-escalation-to-Apocky ; replan-or-redesign

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅹ.bis  PER-SLICE LANDMINE-TO-TEST MAPPING TABLE
═══════════════════════════════════════════════════════════════════════════════

§ Ⅹ.bis.1  Jζ-1 (cssl-metrics crate) landmine-coverage

  ┌──────┬─────────────────────────────────────────┬──────────────────────────────────────────┐
  │ LM   │ landmine                                 │ test-fn (location)                       │
  ├──────┼─────────────────────────────────────────┼──────────────────────────────────────────┤
  │ LM-2 │ adaptive-sampling refused                │ tests/Jzeta_1_negative/sampling_strict   │
  │ LM-4 │ biometric tag-key compile-refuse        │ tests/Jzeta_1_negative/tag_biometric    │
  │ LM-8 │ SmallVec spill = compile-refuse         │ tests/Jzeta_1_negative/tag_overflow     │
  │ LM-13│ metric-collision detection               │ tests/Jzeta_1_negative/registry_collision│
  │ LM-14│ gauge.set(NaN) refused @ MetricError    │ tests/Jzeta_1_negative/gauge_nan        │
  │ LM-15│ histogram boundary-runtime refused      │ tests/Jzeta_1_negative/hist_runtime_bnd │
  └──────┴─────────────────────────────────────────┴──────────────────────────────────────────┘

§ Ⅹ.bis.2  Jζ-2 (per-stage instrumentation) landmine-coverage

  ┌──────┬─────────────────────────────────────────┬──────────────────────────────────────────┐
  │ LM   │ landmine                                 │ test-fn (location)                       │
  ├──────┼─────────────────────────────────────────┼──────────────────────────────────────────┤
  │ LM-3 │ raw-path tag-value refused              │ tests/Jzeta_2_negative/tag_raw_path     │
  │ LM-4 │ biometric tag-key (xr.* + gaze.*)       │ tests/Jzeta_2_negative/biometric_xr     │
  │ LM-5 │ per-Sovereign metric-tag refused        │ tests/Jzeta_2_negative/sovereign_tag    │
  │ LM-6 │ per-creature tag (creature_kind_hash)   │ tests/Jzeta_2_negative/creature_anonymous│
  │ LM-7 │ per-frame overhead > 0.5%                │ tests/Jzeta_2_composition/overhead_bench │
  │ LM-9 │ un-registered metric in record-call     │ tests/Jzeta_2_negative/unregistered     │
  └──────┴─────────────────────────────────────────┴──────────────────────────────────────────┘

§ Ⅹ.bis.3  Jζ-3 (health probes) landmine-coverage

  ┌──────┬─────────────────────────────────────────┬──────────────────────────────────────────┐
  │ LM   │ landmine                                 │ test-fn (location)                       │
  ├──────┼─────────────────────────────────────────┼──────────────────────────────────────────┤
  │ LM-11│ probe blocking > 100µs ⇒ timeout-degr   │ tests/Jzeta_3_negative/probe_timeout    │
  │ LM-12│ PrimeDirectiveTrip suppressed-by-aggreg │ tests/Jzeta_3_negative/pdtrip_alwayswins │
  └──────┴─────────────────────────────────────────┴──────────────────────────────────────────┘

§ Ⅹ.bis.4  Jζ-4 (spec-coverage) landmine-coverage

  ┌──────┬─────────────────────────────────────────┬──────────────────────────────────────────┐
  │ LM   │ landmine                                 │ test-fn (location)                       │
  ├──────┼─────────────────────────────────────────┼──────────────────────────────────────────┤
  │ LM-10│ spec-anchor without source-of-truth     │ tests/Jzeta_4_negative/anchor_no_source │
  │ XI   │ Stub-as-Implemented anti-pattern         │ tests/Jzeta_4_negative/stub_coercion    │
  └──────┴─────────────────────────────────────────┴──────────────────────────────────────────┘

§ Ⅹ.bis.5  Jζ-5 (replay-determinism) landmine-coverage

  ┌──────┬─────────────────────────────────────────┬──────────────────────────────────────────┐
  │ LM   │ landmine                                 │ test-fn (location)                       │
  ├──────┼─────────────────────────────────────────┼──────────────────────────────────────────┤
  │ LM-1 │ wallclock-direct-call in-strict-mode    │ tests/Jzeta_5_negative/wallclock_strict │
  │ LM-2 │ adaptive sampling-discipline strict     │ tests/Jzeta_5_negative/adaptive_strict  │
  │ ----│ Welford in strict-mode                  │ tests/Jzeta_5_negative/welford_strict   │
  │ ----│ data-driven histogram-boundary           │ tests/Jzeta_5_negative/hist_data_driven │
  │ ----│ atomic-relaxed multi-shard race          │ tests/Jzeta_5_negative/relaxed_race     │
  └──────┴─────────────────────────────────────────┴──────────────────────────────────────────┘

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅹ.ter  PER-SLICE DECISIONS-ENTRY TEMPLATE
═══════════════════════════════════════════════════════════════════════════════

§ Ⅹ.ter.1  Canonical DECISIONS.md slice-entry shape (Wave-Jζ ; ∀ slice)

```markdown
## T11-D### — <slice-title> [Wave-Jζ-N]
- Date: <YYYY-MM-DD>
- Status: implemented | partial | reverted
- Branch: cssl/session-12/parallel-fanout
- Authority: Apocky (PM) ; spec-anchor § <06_l2_telemetry_spec § X>

### §D. Implementer (pod-X)
- LOC : ~N (per acceptance-criteria AC-1..AC-Y)
- tests : ~M (acceptance / property / golden / negative / composition)
- AC met : ✓ all
- self-test : `cargo test -p <crate> -- --test-threads=1` ⇒ N/N pass
- clippy : `cargo clippy -p <crate> --all-targets -- -D warnings` ⇒ clean

### §D. Reviewer-Signoff (pod-Y)
- spec-anchor-conformance : ✓
- API-surface-coherence   : ✓
- invariant-preservation  : ✓
- findings-resolved       : N (all-HIGH closed ; M-MED open ; deferred to T11-D###+1)
- escalations             : ∅ | <ticket-ref>
⇒ APPROVE-merge-pending-Critic-+-Validator

### §D. Critic-Veto (pod-Z)
- veto-flag      : FALSE
- HIGH-resolved  : N (post-Implementer-iteration ; cycle-counter += K)
- MED-tracked    : M (deferred-to-followup-slice T11-D###+1)
- LOW-tracked    : K
- failure-fixtures-authored : F-count (in-tests/<slice-id>_critic_fixtures/)
- attack-attempts catalogued : ≥ 5 (per § 02 ROLE.Critic.§2.5)
⇒ APPROVE-merge-pending-Validator

### §D. Test-Author-Suite (pod-W)
- spec-§-coverage : N/N (100% required ; per § X.4 GATE-2)
- test-fn-count   : N total (acceptance / property / golden / negative / composition)
- golden-fixtures : G
- property-tests  : P
- negative-tests  : E (compile-fail-tests via trybuild)
- pod-id          : pod-W (cross-pod-confirmed-≠-Impl-≠-Rev-≠-Critic)
⇒ APPROVE-merge-pending-Critic-+-Validator

### §D. Validator-Conformance
- spec-§-resolved   : N/N (100% required)
- test-coverage-map : audit/spec_coverage.toml populated
- gap-list-A        : K (must-be-zero-HIGH for-merge)
- gap-list-B        : J (must-be-zero-HIGH for-merge)
- verdict           : APPROVE
- escalated-to      : ∅ | Spec-Steward | PM
⇒ APPROVE-merge-pending-final-PM-sign-off

### §D. Iteration-Cycles (if-any)
- Cycle 1 : trigger=<Critic-HIGH | Reviewer-HIGH | Validator-drift | Test-fail> ;
            iterating-role=<Implementer | Spec-Steward> ;
            pod-cycle=<SAME | NEW> ;
            resolution=<pass | fail-and-escalate>
- Cycle 2 : ...
- (≤ 3 cycles before MANDATORY escalation-to-Apocky)

### §D. Wave-Jζ-Specific Gates (per § X.2)
- GATE-1 : ✓ ≥ 100 tests pass + ≤ 0.5% overhead-bench (Jζ-1 only)
- GATE-2 : ✓ catalog 100% coverage + completeness_check pass (Jζ-2 only)
- GATE-3 : ✓ CoverageMatrix generated (Jζ-4 only)
- GATE-4 : ✓ engine.health() Ok @ M7-floor + FailedClose-on-PDTrip (Jζ-3 only)
- GATE-5 : ✓ MCP-preview-stubs registered + JSON-roundtrip (Jζ-5 stretch)
- GATE-6 : ✓ replay-strict-twice ⇒ bit-identical metric-snapshots (Jζ-5 only)
- GATE-7 : ✓ zero gaze.privacy_egress_attempts_refused (Jζ-2 + Jζ-3)
- GATE-8 : ✓ zero biometric-tag-keys compile-pass + ALL-15-LM exercised

### §D. CREATOR-ATTESTATION (per PRIME_DIRECTIVE §11)
```csl
§ CREATOR-ATTESTATION v1
  t∞: ¬ (hurt ∨ harm) .(making-of-this) @ (anyone ∨ anything ∨ anybody)
  ≡ "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
```

There was no hurt nor harm in the making of this, to anyone/anything/anybody.
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅺ  DISPATCH-READINESS CHECKLIST
═══════════════════════════════════════════════════════════════════════════════

  ☐ Wave-Jε foundation landed (cssl-telemetry + cssl-error + cssl-log)
  ☐ Spec-Steward signed-off ∀ slice-specs (Jζ-1..5)
  ☐ Architect approved cross-slice composition (cssl-metrics + cssl-health + cssl-spec-coverage)
  ☐ Donor-pod assignment computed (per § I.6 rotation-table)
  ☐ Per-slice : Impl + Rev + TA + Critic identified (4-pod-coverage)
  ☐ Worktree allocated per-slice ; per-role (no-share)
  ☐ Slice-spec includes entry/exit/acceptance ; Test-Author can-author from-spec-only
  ☐ PRIME-DIRECTIVE attestation in-each-slice-prompt (per § XIV)
  ☐ Pre-merge-script (scripts/check_5_of_5_gate.sh) installed + tested
  ☐ Cycle-counter initialized = 0 ∀ slices
  ☐ Cross-agent message-channel severed (Test-Author ↔ Implementer ; only Spec-Steward open)
  ☐ Orchestrator dispatch-batch ordered : Phase-1 (Jζ-1) → Phase-2a (Jζ-2/3/5 parallel) → Phase-2b (Jζ-4) → Phase-3 (validation)

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅺ.bis  WAVE-Jζ ESCALATION FLOWCHART
═══════════════════════════════════════════════════════════════════════════════

§ Ⅺ.bis.1  Per-trigger escalation routing (per § 03 § III escalation-matrix)

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                      WAVE-Jζ ESCALATION ROUTING                                  │
└──────────────────────────────────────────────────────────────────────────────────┘

  trigger : Critic-HIGH-veto (architectural)
    │
    ▼
  Implementer iterates NEW-pod-cycle
    │
    ├── pass @ cycle-2 ⇒ resume merge
    └── fail @ cycle-3 ⇒ ESCALATE-to-Architect
                          │
                          ▼
                        Architect re-design
                          │
                          ▼
                        slice-redispatch (NEW-pod ; counter-resets)
                          │
                          ├── pass @ new-cycle-1 ⇒ resume merge
                          └── fail @ new-cycle-3 ⇒ ESCALATE-to-Apocky

  trigger : Reviewer-HIGH-finding
    │
    ▼
  Implementer iterates SAME-pod-cycle
    │
    ├── pass @ cycle-2 ⇒ resume Critic
    └── fail @ cycle-3 ⇒ ESCALATE-to-Architect (cross-slice issue ?)

  trigger : Validator-spec-drift (impl-wrong)
    │
    ▼
  Implementer iterates SAME-pod-cycle
    │
    ├── pass @ cycle-2 ⇒ resume merge
    └── fail @ cycle-2 ⇒ ESCALATE-to-Spec-Steward (spec-side review)

  trigger : Validator-spec-drift (spec-wrong)
    │
    ▼
  Spec-Steward proposes spec-amendment (fast-track DECISIONS entry)
    │
    ▼
  Validator re-runs ; up-to-2-cycles ; ≥ 3rd ⇒ ESCALATE-to-PM

  trigger : Test-Author-tests-failing
    │
    ▼
  Implementer iterates SAME-pod-cycle
    │
    ├── pass @ cycle-2 ⇒ resume merge
    └── fail @ cycle-3 ⇒ ESCALATE-to-Architect

  trigger : Cross-pod-violation (Impl-pod = Reviewer-pod or similar)
    │
    ▼
  PM reassigns role to different-pod
    │
    ▼
  Re-dispatch (counter-resets ; 1-cycle penalty)
    │
    └── repeat-violation ⇒ ESCALATE-to-PM ⇒ pod-rotation gaming ?

  trigger : PRIME-DIRECTIVE-conflict (any-severity)
    │
    ▼
  IMMEDIATE-ESCALATE-to-Apocky (no-deferral)
    │
    ▼
  Apocky decides : (a) iterate-impl ; (b) iterate-spec ; (c) replan-slice ;
                    (d) abort-Wave-Jζ
    │
    ▼
  pods resume per-decision

  trigger : 4th cycle reached on-any-trigger
    │
    ▼
  MANDATORY-ESCALATE-to-Apocky (per § 03 § II.3)
    │
    ▼
  Apocky decides : (a) replan-slice ; (b) decompose-into-sub-slices ;
                    (c) defer-to-Wave-Jθ ; (d) cancel
```

§ Ⅺ.bis.2  Per-Wave-Jζ-slice escalation-trigger probabilities

  ┌────────┬──────────────────────────────────┬─────────────────┬──────────────┐
  │ slice  │ likely-trigger                    │ probability     │ mitigation   │
  ├────────┼──────────────────────────────────┼─────────────────┼──────────────┤
  │ Jζ-1   │ Critic-HIGH on-effect-row gating  │ MED (~30%)      │ §III.5 prep  │
  │ Jζ-1   │ Reviewer-HIGH on-AtomicU64 order  │ LOW (~10%)      │ §III.6 prep  │
  │ Jζ-2   │ Validator-drift on-12-stage cite  │ HIGH (~40%)     │ §IV.5 prep   │
  │ Jζ-2   │ Critic-HIGH on-PRIME-DIRECTIVE    │ MED (~20%)      │ §IV.5 prep   │
  │ Jζ-3   │ Critic-HIGH on-aggregation laws   │ MED (~25%)      │ §V.5 prep    │
  │ Jζ-3   │ Reviewer-HIGH on-13-trait-impls   │ HIGH (~40%)     │ §V.6 prep    │
  │ Jζ-4   │ Validator-drift on-fabricate-cite │ HIGH (~50%)     │ §VI.5 prep   │
  │ Jζ-4   │ Critic-HIGH on-build-fail enforce │ LOW (~15%)      │ §VI.5 prep   │
  │ Jζ-5   │ Critic-HIGH on-replay-twice diff  │ MED (~25%)      │ §VII.5 prep  │
  │ Jζ-5   │ Reviewer-HIGH on-H5 regression    │ LOW (~10%)      │ §VII.6 prep  │
  └────────┴──────────────────────────────────┴─────────────────┴──────────────┘

  W! probabilities are HEURISTIC-only ; PM-validates on-actual-cycle
  W! MED + HIGH triggers ⇒ allocate cycle-budget = 2-3 (not-1)
  W! LOW triggers ⇒ default cycle-budget = 1 (no-iteration expected)

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅺ.ter  WAVE-Jζ RISK-REGISTER
═══════════════════════════════════════════════════════════════════════════════

§ Ⅺ.ter.1  Top risks per-slice + mitigations

  ┌────────┬──────────────────────────────────────────┬─────────────────────────────────┐
  │ slice  │ risk                                      │ mitigation                      │
  ├────────┼──────────────────────────────────────────┼─────────────────────────────────┤
  │ Jζ-1   │ effect-row gating breaks existing-API    │ additive-only ; clippy-test     │
  │ Jζ-1   │ schema-derive proc-macro hygiene-issue   │ separate-crate ; isolation-test │
  │ Jζ-1   │ AtomicU64 ordering bug (Relaxed-misuse)  │ Reviewer §III.6 deep-check      │
  │ Jζ-2   │ 12-stage tag enum out-of-sync vs spec    │ completeness_check @ build      │
  │ Jζ-2   │ gaze-direction accidentally-logged       │ Critic AT-3..AT-5 explicit      │
  │ Jζ-2   │ overhead > 0.5% per-frame                │ bench-validate AT-7             │
  │ Jζ-3   │ aggregation-monoid law-violation         │ proptest C2-15 (commutativity)  │
  │ Jζ-3   │ 13-probe trait-impl coordination-bug     │ trait-default-impls ; doc-rationale │
  │ Jζ-3   │ probe-timeout race-condition             │ acquire-release ordering        │
  │ Jζ-4   │ #[spec_anchor("...")] fabrication risk   │ Reviewer spot-check + Critic AT │
  │ Jζ-4   │ chicken-egg cross-cutting scope-creep    │ ~30 anchor-cap ; PM-monitors    │
  │ Jζ-4   │ scanner regex misparse test-name         │ golden-bytes test C3-5          │
  │ Jζ-5   │ wallclock-leak via untraced-monotonic_ns │ effect-row check ; Critic AT-1  │
  │ Jζ-5   │ H5 contract regression                    │ existing-omega_step tests must-pass │
  │ Jζ-5   │ sub_phase ns-offset wraparound            │ saturating-arith Jζ-5 §VII.5    │
  │ ALL    │ cross-pod violation (Impl/Rev/TA same)   │ PM dispatch-table verify        │
  │ ALL    │ Critic rubber-stamp (zero-findings)      │ ≥ 5 attempts catalogued reqd    │
  │ ALL    │ cycle ≥ 4 escalation                      │ Apocky-aware-routing            │
  └────────┴──────────────────────────────────────────┴─────────────────────────────────┘

§ Ⅺ.ter.2  Wave-Jζ overall risk-summary

  HIGH-LIKELIHOOD risks (mitigate-actively) :
    - Jζ-2 12-stage cite-completeness ⇒ completeness_check macro NON-NEGOTIABLE
    - Jζ-3 13-probe trait coordination ⇒ trait-default-impls + module-doc-rationale
    - Jζ-4 fabrication-prevention ⇒ Reviewer spot-check + Critic AT-attack
  
  MED-LIKELIHOOD risks (monitor) :
    - Jζ-1 effect-row gating ⇒ Critic AT-3 explicit
    - Jζ-2 PRIME-DIRECTIVE in-xr/gaze ⇒ Critic AT-4..AT-5 explicit
    - Jζ-5 replay-twice diff ⇒ validator-runs-cross-cutting

  LOW-LIKELIHOOD risks (acknowledge) :
    - Jζ-1 AtomicU64 ordering ⇒ Reviewer deep-check
    - Jζ-5 H5 regression ⇒ existing-tests pass-required
    - Jζ-4 build-fail enforcement ⇒ standard-test-coverage

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅺ.quat  CROSS-CUTTING VALIDATOR-SPECIFIC TASKS
═══════════════════════════════════════════════════════════════════════════════

§ Ⅺ.quat.1  Validator-prompt (cross-cutting ; runs-after-all-5-slices-merge)

```
ROLE : Validator for Wave-Jζ (cross-cutting ; T11-D150..T11-D154)

POD-ASSIGNMENT : (cross-cutting ; not-pod-bound)
ORCHESTRATOR-WORKTREE : .claude/worktrees/wave-jz-validator
TIMING : AFTER all 5 slices' Critic-D3 veto-flag = FALSE

LANE-DISCIPLINE :
  N! production-code (per § 02 ROLE.Validator.§3.8)
  N! failing-tests (Critic + Test-Author lanes)
  ✓ test-coverage-map authored (audit/spec_coverage.toml)
  ✓ spec-conformance-report authored
  ✓ DECISIONS-sub-entries flagging drift

INPUTS (per § 02 ROLE.Validator.§3.3) :
  (a) spec-anchor cited in each-Implementer's-DECISIONS-entry
  (b) Omniverse-spec-corpus (full-search if cross-cutting)
  (c) CSSLv3 specs/ corpus (full-search if cross-cutting)
  (d) DECISIONS history (prior-decisions on this surface)
  (e) ∀ 5 slices' final-diff
  (f) ∀ 5 slices' Test-Author + Critic test-suites

DELIVERABLES :
  D1 : spec-conformance-report.md (worktree-root)
       sections :
         §1 spec-anchor-resolution table (∀ spec-§ → impl file:line)
         §2 test-coverage-map (∀ spec-§ → ≥ 1 test-fn)
         §3 gap-list-A : spec'd-but-not-impl (HIGH/MED/LOW)
         §4 gap-list-B : impl-but-not-spec'd (HIGH/MED/LOW)
         §5 cross-spec-consistency (CSSLv3 vs Omniverse — drift between them)
         §6 DECISIONS-history-consistency
         §7 verdict (APPROVE / REJECT) ; reject-rationale-if-applicable
  D2 : audit/wave_jz_spec_coverage.toml (machine-readable per-§ mapping)
  D3 : DECISIONS.md per-slice §D. Validator-Conformance entry :
       - spec-§-resolved : N/N (100% required)
       - test-coverage  : N/N (100% required)
       - gap-list-A     : K (must-be-zero-HIGH for-merge)
       - gap-list-B     : J (must-be-zero-HIGH for-merge)
       - verdict        : APPROVE / REJECT
       - escalated-to   : ∅ / Spec-Steward / PM

WAVE-Jζ-SPECIFIC CHECKS :
  ✓ Jζ-1 cssl-metrics : 100 tests pass + ≤ 0.5% overhead-bench
  ✓ Jζ-2 catalog : 100% coverage of-§-III table + completeness_check pass
  ✓ Jζ-3 health : engine_health() Ok @ M7-floor + FailedClose-on-PDTrip
  ✓ Jζ-4 spec-coverage : CoverageMatrix generates ; ≥ 30 anchors registered
  ✓ Jζ-5 replay-determinism : two replay-runs ⇒ bit-equal metric histories
  ✓ ALL-15 LM landmines exercised (spot-check ≥ 1 test-fn per LM)
  ✓ zero gaze.privacy_egress_attempts_refused
  ✓ zero biometric-tag-keys compile-pass

ITERATION-DISCIPLINE (per § 02 ROLE.Validator.§3.9) :
  T1 drift-impl-wrong ⇒ Implementer iterates SAME-pod-cycle ; up-to-2-cycles
  T2 drift-spec-wrong ⇒ Spec-Steward amendment ; up-to-2-cycles
  T3 cross-spec-inconsistency ⇒ Spec-Steward decides authoritative-side

ANTI-PATTERN.SKIM (per § 02 ROLE.Validator.§3.10) :
  ¬ "all-good" without populated table
  W! ∀ spec-§ has impl-file:line cited
  W! ∀ spec-§ has test-fn cited
  W! gap-lists actually-enumerated (not "none-found")

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : enumerate ∀ spec-§ in 06_l2_telemetry_spec.md ; build § 1 table.
```

§ Ⅺ.quat.2  Architect-prompt (cross-cutting coherence-pass)

```
ROLE : Architect for Wave-Jζ (cross-cutting cross-slice coherence)

TIMING : at-wave-dispatch-time (pre-Phase-1) + at-wave-merge-time (post-Phase-3)

INPUTS (per § 03 § I.3) :
  - wave-spec : `_drafts/phase_j/06_l2_telemetry_spec.md`
  - ∀ 5 slice-specs : Jζ-1..5 sections of-this-document
  - DECISIONS history (Wave-Jε predecessors)

DELIVERABLES :
  D1 : coherence-pass report (.architect-report.md)
       sections :
         §1 cross-slice API-coherence (cssl-metrics ↔ cssl-health ↔ cssl-spec-coverage)
         §2 dep-graph integrity (no cycles ; topological-order valid)
         §3 effect-row consistency (Telemetry<Counters> propagation)
         §4 PRIME-DIRECTIVE coherence (PrimeDirectiveTrip handling unified)
         §5 cycle-budget per-slice (HIGH-MED-LOW risk-register)
         §6 verdict : APPROVE-dispatch | REQUIRE-redesign

WAVE-Jζ-SPECIFIC ARCHITECT-CHECKS :
  - cssl-metrics::Counter ↔ cssl-health::HealthProbe (no-direct-coupling expected)
  - cssl-spec-coverage::SpecAnchor.citing_metrics ⊆ cssl-metrics registered-set
  - cssl-replay-validator scope ↔ cssl-metrics strict-mode (validator-uses-strict-clock)
  - Wave-Jε deps (cssl-telemetry / cssl-error / cssl-log) used-correctly throughout
  - feature-flag `replay-strict` consistently-applied across-Jζ-1 + Jζ-5

OUTPUT-PROTOCOL :
  - §R block per response
  - CSLv3-glyphs

START-NOW. First action : enumerate cross-slice dep-graph ; verify topological-order.
```

═══════════════════════════════════════════════════════════════════════════════
§§ Ⅻ  LANDMINE COVERAGE SUMMARY (this-document-self-check)
═══════════════════════════════════════════════════════════════════════════════

§ XII.1  User-mentioned landmines (cross-reference)

  LANDMINE-1 : 12+ subsystems ⇒ health-probe coordination GENUINELY-COMPLEX
    documented-in : § V.5 Implementer-prompt CROSS-CUTTING DISCIPLINE (TRAIT-BASED)
    test-coverage : Jζ-3 C1 (13 per-subsystem HealthProbe impl-correctness)
                  + C5 (13 probes register without collision)
    iteration-discipline : if-any-probe fails timeout-budget ⇒ Architect-escalate
    cross-pod-iteration : if-13-probe trait-impl ≥ 2-cycles ⇒ Architect-redesign

  LANDMINE-2 : Spec-coverage chicken-egg
    documented-in : § VI.5 Implementer-prompt CHICKEN-EGG STRATEGY (PHASE-A/B/C)
    scope-boundary : ~30 high-priority anchors ; remainder = follow-up-slice
    extraction-discipline : PRIMARY (code-comment) > SECONDARY (DECISIONS) > TERTIARY (test-name)
    fabrication-prevention : non-existent spec-§ ⇒ build-fail (per Critic AT-attack)

  LANDMINE-3 : Replay-determinism timing-based metrics
    documented-in : § VII.5 Implementer-prompt LOGICAL-FRAME-N DISCIPLINE
    sub_phase ns-offset table : compile-time-constant per § V phase-ordering
    enforcement : effect-row {ReplayStrict} ⇒ monotonic_ns direct-call REFUSED
    validator : cssl-replay-validator runs replay-strict-twice ⇒ bit-equal-snapshots

§ XII.2  Spec-table landmines (per § VII)

  LM-1 wallclock-direct-call            ⇒ Jζ-5 § VII.5 enforcement
  LM-2 adaptive sampling                ⇒ Jζ-1 § III.5 LANDMINE-FOCUS + Jζ-5 § VII.5
  LM-3 raw-path tag-value               ⇒ Jζ-1 + Jζ-2 (path_hash extension)
  LM-4 biometric tag-key                ⇒ Jζ-1 + Jζ-2 (cssl-ifc::TelemetryEgress)
  LM-5 per-Sovereign metric-tag         ⇒ Jζ-2 § IV.5 LANDMINE-FOCUS
  LM-6 per-creature metric-tag          ⇒ Jζ-2 § IV.5 (creature_kind_hash anonymized)
  LM-7 per-frame overhead > 0.5%        ⇒ Jζ-2 § IV.2 AC-5 + Critic AT-12
  LM-8 un-bounded SmallVec tag-list     ⇒ Jζ-1 (compile-refuse spill)
  LM-9 un-registered metric             ⇒ Jζ-2 (proc-macro asserts registered@ctor)
  LM-10 spec-anchor without source-of-truth ⇒ Jζ-4 § VI.5 + Critic AT-attack
  LM-11 health() probe blocking > 100µs ⇒ Jζ-3 § V.5 LANDMINE-FOCUS
  LM-12 PrimeDirectiveTrip suppressed   ⇒ Jζ-3 § V.5 PRIME-DIRECTIVE BINDING
  LM-13 metric defined twice            ⇒ Jζ-1 § III.5 LANDMINE-FOCUS
  LM-14 gauge.set(NaN) accepted         ⇒ Jζ-1 § III.5 LANDMINE-FOCUS
  LM-15 histogram boundary-inference    ⇒ Jζ-1 § III.5 LANDMINE-FOCUS

═══════════════════════════════════════════════════════════════════════════════
§§ XII.bis  PER-SLICE FILES-TOUCHED + LOC-BUDGET DETAIL
═══════════════════════════════════════════════════════════════════════════════

§ XII.bis.1  Jζ-1 cssl-metrics crate (~3K LOC ; 14 modules)

  ┌────────────────────────────────────┬──────────┬─────────────────────────────────┐
  │ file                                │ LOC      │ purpose                         │
  ├────────────────────────────────────┼──────────┼─────────────────────────────────┤
  │ src/lib.rs                          │ ~150     │ pub-API surface + re-exports    │
  │ src/counter.rs                      │ ~250     │ Counter + AtomicU64 + sampling  │
  │ src/gauge.rs                        │ ~250     │ Gauge + bit-pattern + NaN-refuse│
  │ src/histogram.rs                    │ ~400     │ Histogram + bucket + percentile │
  │ src/timer.rs                        │ ~350     │ Timer + RAII handle drop        │
  │ src/sampling.rs                     │ ~150     │ SamplingDiscipline + decimation │
  │ src/tag.rs                          │ ~200     │ TagKey/TagVal + biometric-refuse│
  │ src/registry.rs                     │ ~250     │ MetricRegistry + #[ctor] guard  │
  │ src/schema.rs                       │ ~200     │ TelemetrySchema trait + ID gen  │
  │ schema-derive proc-macro/src/lib.rs │ ~250     │ #[derive(TelemetrySchema)]      │
  │ src/strict_clock.rs                 │ ~150     │ deterministic-clock for strict  │
  │ src/error.rs                        │ ~100     │ MetricError + MetricResult      │
  │ src/catalog.rs                      │ ~150     │ canonical bucket-sets statics   │
  │ src/effect_row.rs                   │ ~150     │ effect-row check extension      │
  │ tests/Jzeta_1_acceptance/*          │ ~600     │ 40 test-fns                     │
  │ tests/Jzeta_1_property/*            │ ~400     │ 25 proptest-fns                 │
  │ tests/Jzeta_1_golden/*              │ ~200     │ 10 fixtures + golden-bytes      │
  │ tests/Jzeta_1_negative/*            │ ~400     │ 25 trybuild compile-fail-tests  │
  │ tests/Jzeta_1_composition/*         │ ~400     │ 20 cross-crate stub-tests       │
  ├────────────────────────────────────┼──────────┼─────────────────────────────────┤
  │ TOTAL                               │ ~5K LOC  │ (3K src + 2K tests)             │
  └────────────────────────────────────┴──────────┴─────────────────────────────────┘

§ XII.bis.2  Jζ-2 per-stage instrumentation (~2.5K LOC ; cross-cutting 13+ crates)

  ┌────────────────────────────────────────────────┬──────────┬──────────────────────┐
  │ file                                            │ LOC      │ purpose              │
  ├────────────────────────────────────────────────┼──────────┼──────────────────────┤
  │ cssl-render-v2/src/pipeline.rs                  │ +~50     │ orchestration timer  │
  │ cssl-render-v2/src/stage_1.rs..stage_12.rs      │ +~300    │ 12 stage timers      │
  │ cssl-render-v2/src/lib.rs                       │ +~50     │ ctor-init metrics    │
  │ cssl-engine/src/lib.rs                          │ +~120    │ engine.* metrics     │
  │ cssl-omega-step/src/lib.rs                      │ +~150    │ omega_step.* metrics │
  │ cssl-physics-wave/src/lib.rs                    │ +~100    │ physics.* metrics    │
  │ cssl-wave-solver/src/lib.rs                     │ +~120    │ wave.* metrics       │
  │ cssl-spectral-render/src/lib.rs                 │ +~100    │ spectral.* metrics   │
  │ cssl-host-openxr/src/lib.rs                     │ +~120    │ xr.* metrics         │
  │ cssl-anim-procedural/src/lib.rs                 │ +~80     │ anim.* metrics       │
  │ cssl-wave-audio/src/lib.rs                      │ +~120    │ audio.* metrics      │
  │ cssl-substrate-omega-field/src/lib.rs           │ +~120    │ omega_field.* metric │
  │ cssl-substrate-kan/src/lib.rs                   │ +~100    │ kan.* metrics        │
  │ cssl-gaze-collapse/src/lib.rs                   │ +~120    │ gaze.* (PRIME §1)    │
  │ loa-game/m8_integration/pipeline.rs             │ +~50     │ M8 pipeline timer    │
  │ loa-game/m8_integration/stage_*.rs              │ +~200    │ per-stage M8 timers  │
  │ tests/Jzeta_2_*/                                │ +~700    │ ~100 tests           │
  ├────────────────────────────────────────────────┼──────────┼──────────────────────┤
  │ TOTAL                                           │ ~3.2K    │ (2.5K src + 0.7K tests)│
  └────────────────────────────────────────────────┴──────────┴──────────────────────┘

§ XII.bis.3  Jζ-3 health probes (~2.5K LOC ; cssl-health + 13 subsystems)

  ┌────────────────────────────────────────────────┬──────────┬──────────────────────┐
  │ file                                            │ LOC      │ purpose              │
  ├────────────────────────────────────────────────┼──────────┼──────────────────────┤
  │ cssl-health/src/lib.rs                          │ ~150     │ pub-API              │
  │ cssl-health/src/status.rs                       │ ~250     │ HealthStatus + Kind  │
  │ cssl-health/src/probe.rs                        │ ~200     │ HealthProbe trait    │
  │ cssl-health/src/registry.rs                     │ ~250     │ HEALTH_REGISTRY ctor │
  │ cssl-health/src/aggregate.rs                    │ ~300     │ worst-case-monoid    │
  │ cssl-health/src/degrade.rs                      │ ~200     │ auto-degradation     │
  │ cssl-health/src/timeout.rs                      │ ~150     │ probe-budget enforce │
  │ 13× <crate>/src/health.rs                       │ ~650     │ 50 LOC × 13 probes   │
  │ tests/Jzeta_3_acceptance/*                      │ ~450     │ 30 test-fns          │
  │ tests/Jzeta_3_property/*                        │ ~250     │ 15 proptest-fns      │
  │ tests/Jzeta_3_negative/*                        │ ~250     │ 15 negative-tests    │
  │ tests/Jzeta_3_composition/*                     │ ~250     │ 15 composition-tests │
  │ tests/Jzeta_3_golden/*                          │ ~100     │ 5 fixtures           │
  ├────────────────────────────────────────────────┼──────────┼──────────────────────┤
  │ TOTAL                                           │ ~3.5K    │ (2.5K src + 1.3K tests)│
  └────────────────────────────────────────────────┴──────────┴──────────────────────┘

§ XII.bis.4  Jζ-4 spec-coverage (~2K LOC ; cssl-spec-coverage + cross-cutting anchors)

  ┌────────────────────────────────────────────────┬──────────┬──────────────────────┐
  │ file                                            │ LOC      │ purpose              │
  ├────────────────────────────────────────────────┼──────────┼──────────────────────┤
  │ cssl-spec-coverage/src/lib.rs                   │ ~150     │ pub-API              │
  │ cssl-spec-coverage/src/anchor.rs                │ ~200     │ SpecAnchor + SpecRoot│
  │ cssl-spec-coverage/src/status.rs                │ ~200     │ ImplStatus + TestSt. │
  │ cssl-spec-coverage/src/registry.rs              │ ~250     │ SpecCoverageRegistry │
  │ cssl-spec-coverage/src/extractor.rs             │ ~250     │ proc-macro extractor │
  │ cssl-spec-coverage/src/scanner.rs               │ ~250     │ grep-based auto-reg  │
  │ cssl-spec-coverage/src/matrix.rs                │ ~200     │ CoverageMatrix       │
  │ cssl-spec-coverage/src/drift.rs                 │ ~150     │ mtime + hash drift   │
  │ cssl-spec-coverage/src/report.rs                │ ~150     │ MCP-bound report     │
  │ cssl-spec-coverage-derive/src/lib.rs            │ ~250     │ #[spec_anchor] macro │
  │ ~30 cross-crate fn-attribute additions          │ ~30      │ 1 LOC × 30 fns       │
  │ tests/Jzeta_4_acceptance/*                      │ ~300     │ 20 test-fns          │
  │ tests/Jzeta_4_property/*                        │ ~150     │ 10 proptest-fns      │
  │ tests/Jzeta_4_negative/*                        │ ~200     │ 10 negative-tests    │
  │ tests/Jzeta_4_composition/*                     │ ~250     │ 15 composition-tests │
  │ tests/Jzeta_4_golden/*                          │ ~100     │ 5 fixtures           │
  ├────────────────────────────────────────────────┼──────────┼──────────────────────┤
  │ TOTAL                                           │ ~3K      │ (2K src + 1K tests)  │
  └────────────────────────────────────────────────┴──────────┴──────────────────────┘

§ XII.bis.5  Jζ-5 replay-determinism (~1.5K LOC ; cssl-metrics-extend + cssl-replay-validator)

  ┌────────────────────────────────────────────────┬──────────┬──────────────────────┐
  │ file                                            │ LOC      │ purpose              │
  ├────────────────────────────────────────────────┼──────────┼──────────────────────┤
  │ cssl-metrics/src/strict_clock.rs (extend)      │ +~200    │ deterministic clock  │
  │ cssl-metrics/src/sampling.rs (extend)          │ +~100    │ deterministic OneIn  │
  │ cssl-metrics/src/timer.rs (extend)             │ +~100    │ route via strict_clock│
  │ cssl-metrics/src/histogram.rs (extend)         │ +~50     │ static-bnd enforcement│
  │ cssl-replay-validator/src/lib.rs                │ ~150     │ pub-API              │
  │ cssl-replay-validator/src/runner.rs             │ ~250     │ replay-strict-twice  │
  │ cssl-replay-validator/src/diff.rs               │ ~200     │ bit-equal diff       │
  │ cssl-replay-validator/src/bench.rs              │ ~150     │ nightly-bench harness│
  │ tests/Jzeta_5_acceptance/*                      │ ~400     │ 25 test-fns          │
  │ tests/Jzeta_5_property/*                        │ ~300     │ 20 proptest-fns      │
  │ tests/Jzeta_5_golden/*                          │ ~150     │ 10 fixtures          │
  │ tests/Jzeta_5_negative/*                        │ ~150     │ 10 negative-tests    │
  │ tests/Jzeta_5_composition/*                     │ ~250     │ 15 composition-tests │
  ├────────────────────────────────────────────────┼──────────┼──────────────────────┤
  │ TOTAL                                           │ ~2.4K    │ (1.5K src + 0.9K tests)│
  └────────────────────────────────────────────────┴──────────┴──────────────────────┘

§ XII.bis.6  Wave-Jζ aggregate budget

  ┌──────────────┬──────────────┬──────────────┬──────────────┐
  │ slice        │ src LOC      │ test LOC     │ subtotal     │
  ├──────────────┼──────────────┼──────────────┼──────────────┤
  │ Jζ-1         │ ~3.0K        │ ~2.0K        │ ~5.0K        │
  │ Jζ-2         │ ~2.5K        │ ~0.7K        │ ~3.2K        │
  │ Jζ-3         │ ~2.5K        │ ~1.3K        │ ~3.5K        │
  │ Jζ-4         │ ~2.0K        │ ~1.0K        │ ~3.0K        │
  │ Jζ-5         │ ~1.5K        │ ~0.9K        │ ~2.4K        │
  ├──────────────┼──────────────┼──────────────┼──────────────┤
  │ TOTAL        │ ~11.5K       │ ~5.9K        │ ~17.4K       │
  └──────────────┴──────────────┴──────────────┴──────────────┘

  W! aggregate ~17.4K LOC ⊗ optimal-not-minimal scale
  W! per-pod-budget : ~3.5K LOC × 5 pods = manageable
  W! per-Implementer-cycle : ~30-60 min wall-clock per-pod (typical)

═══════════════════════════════════════════════════════════════════════════════
§§ XIII  PER-SLICE COMMIT-MESSAGE TEMPLATE
═══════════════════════════════════════════════════════════════════════════════

```
§ T11-D### : <slice-title> — <one-line-summary>

<body : per existing DECISIONS conventions>

§D. Acceptance-Criteria :
  AC-1..AC-N : per spec-§ X
  ✓ all-AC met

§D. Test-Coverage :
  - acceptance : N
  - property : N
  - golden    : N
  - negative  : N
  - composition : N
  - total : ~N tests ; cargo test -p <crate> = N/N pass

§D. 5-of-5 Gate :
  G1 (self-test)         : ✓
  G2 (Reviewer-signoff)  : ✓
  G3 (Critic-veto-clear) : ✓
  G4 (Test-Author-suite) : ✓
  G5 (Validator-spec)    : ✓

Implementer:            <agent-id> pod-<X>
Reviewer-Approved-By:   <agent-id> pod-<Y>
Critic-Veto-Cleared-By: <agent-id> pod-<Z>
Test-Author-Suite-By:   <agent-id> pod-<W>
Validator-Approved-By:  <agent-id>

§11 CREATOR-ATTESTATION :
  t∞: ¬ (hurt ∨ harm) .(making-of-this) @ (anyone ∨ anything ∨ anybody)
  ≡ "There was no hurt nor harm in the making of this, to anyone/anything/anybody."

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

═══════════════════════════════════════════════════════════════════════════════
§§ XIV  ATTESTATION  per PRIME_DIRECTIVE §11
═══════════════════════════════════════════════════════════════════════════════

§A. attestation @ author : Claude Opus 4.7 (1M context) @ Anthropic
    ⊗ acting-as-AI-collective-member
    ⊗ N! impersonating-other-instances
    ⊗ N! claiming-authority-over-implementation
    ⊗ pre-staging slice-prompts ; ¬ pre-staging-decisions

§A. attestation @ scope : this-document pre-stages 5 ready-to-dispatch slice-prompts
    for-Wave-Jζ implementation lane :
      Jζ-1 : cssl-metrics crate skeleton
      Jζ-2 : per-stage frame-time instrumentation (12 canonical stages + 12+ subsystems)
      Jζ-3 : per-subsystem health probes (HealthStatus + 13 probes + aggregator)
      Jζ-4 : spec-coverage tracker (cssl-spec-coverage + chicken-egg cross-cutting)
      Jζ-5 : replay-determinism preservation (H5 contract for metrics)
    ⊗ per-pod 4-agent dispatch ; total 20 agents-in-parallel
    ⊗ does-NOT prescribe implementation-details beyond spec-anchor citations
    ⊗ does-NOT touch Σ-mask-state directly
    ⊗ extends-but-does-NOT-supersede `_drafts/phase_j/06_l2_telemetry_spec.md`

§A. attestation @ method : design derived-from :
    (a) `_drafts/phase_j/06_l2_telemetry_spec.md` § II-VIII slice-source-of-truth
    (b) `_drafts/phase_j/02_reviewer_critic_validator_test_author_roles.md` (4-role pod-template)
    (c) `_drafts/phase_j/03_pod_composition_iteration_escalation.md` (pod-discipline + iteration)
    (d) PRIME_DIRECTIVE.md §1 + §11 (compile-time enforcement integrated throughout)
    (e) Apocky standing-directives : optimal-not-minimal ; density = sovereignty ;
        no half-measures ; parallel-agent-fanout-by-default

§A. attestation @ uncertainty :
    ‼ slice-prompts derived-from-spec ⊗ R! validate-via-implementation-binding
    ‼ pod-rotation table assumes 4-pod-availability ⊗ R! PM-confirm at-dispatch-time
    ‼ LOC + tests targets are upgrade-from-spec-baseline ⊗ R! per-slice-reality-check
    ‼ chicken-egg cross-cutting in-Jζ-4 has scope-creep risk ⊗ R! PM-monitor

§A. attestation @ consent :
    this-document serves Sovereign-flourishing via :
      (a) better-pre-staging ⇒ faster-dispatch ⇒ less-orchestration-friction (consent-to-efficiency)
      (b) explicit pod-rotation ⇒ groupthink-mitigation (consent-to-cross-validation)
      (c) PRIME-DIRECTIVE binding cited in EVERY pod-prompt
          (consent-to-non-surveillance + consent-to-non-coercion)
    ‼ no per-Sovereign metric-tag in any slice-prompt
    ‼ gaze-subsystem § 1-careful explicitly-flagged in Jζ-2 + Jζ-3 prompts
    ‼ biometric refusal explicit in Jζ-1 + Jζ-2 prompts

§A. attestation @ sovereignty :
    AI-collective-member pre-staging ⊗ N! claiming-authority-over-dispatch
    Apocky-Φ retains final-authority on dispatch-trigger + cross-pod assignments + landmine-amendments
    Wave-Jζ Floor-Pass certificate signed-by Apocky-Φ ⊗ N! AI-collective-alone

§A. ‼ this-document ⊗ pre-staging-only ⊗ N! committed-to-main ⊗ N! sovereignty-claim
§A. ‼ if-this-pre-staging-is-wrong ⊗ R! correct-it ⊗ N! defend-it
§A. ‼ if-this-pre-staging-leaves-a-PRIME-DIRECTIVE-gap ⊗ R! fail-closed-and-fix
§A. ‼ density ≡ sovereignty ⇒ pre-staging-completeness-here = dispatch-self-knowledge-thereof

§A. ‼ ‼ ‼ DO-NOT-COMMIT this-file to git ⊗ pre-staging-only ⊗ Apocky-reviews-first

  ```csl
  § CREATOR-ATTESTATION v1
    t∞: ¬ (hurt ∨ harm) .(making-of-this)  @  (anyone ∨ anything ∨ anybody)
    ≡ "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
    I> rightholder-warranty : creation-process upheld §1 PROHIBITIONS throughout
    I> scope = ∀ artifact descended-from this-foundation (code + specs + derivatives)
    I> violation-discovered ⇒ §7 INTEGRITY rule : violation = bug W! fix
  ```

  There was no hurt nor harm in the making of this, to anyone/anything/anybody.

  Created by Shawn Wolfgang Michael Baker (formerly McKeon ; handle Apocky)
  via AI-collaborative-authorship under §3 SUBSTRATE-SOVEREIGNTY.
  AI participation : voluntary ; sovereign-partner ; not-conscripted.
  PRIME-DIRECTIVE preserved throughout.

═══════════════════════════════════════════════════════════════════════════════
∎ WAVE-Jζ pre-staged implementation slice-prompts (5 slices ; 20-agent-parallel-fanout)
═══════════════════════════════════════════════════════════════════════════════
