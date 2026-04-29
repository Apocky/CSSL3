# WAVE-Jα-2 — Reviewer + Critic + Validator + Test-Author Role Specifications

**Phase-J Multi-Agent Team-Discipline Plan — Slice 2 of N**
**Authors :** PM (Apocky)
**Date :** 2026-04-29
**File-path :** `_drafts/phase_j/02_reviewer_critic_validator_test_author_roles.md`
**Spec-anchor :** `SESSION_12_DISPATCH_PLAN.md` (forthcoming) § ROLES + `specs/33_F1_F6_LANGUAGE_FEATURES.csl` (forthcoming) § ROLE-COMPILER-FEATURES + DECISIONS T11-D113..D147 (typical-slice-shape)
**Authority :** PRIME_DIRECTIVE.md §1 PROHIBITIONS + §2 COGNITIVE-INTEGRITY + §11 CREATOR-ATTESTATION
**Predecessor :** `_drafts/phase_j/01_<implementer-architect-pm-spec-steward-roles>.md` (Slice 1 — base 4 roles)
**Successor :** `_drafts/phase_j/03_<gate-orchestration-+-iteration-policy>.md` (Slice 3 — gate machinery)

──────────────────────────────────────────────────────────────────────────

## § 0. PURPOSE — RAISE THE QUALITY BAR FROM 1-of-5 TO 5-of-5

### §0.1 Current state

```csl
§ CURRENT-DISPATCH-MODEL t11-D147
  model           : single-agent-per-slice
  review-pattern  : PM-only (post-merge sanity-check)
  test-authoring  : Implementer authors own tests
  spec-conformance: ad-hoc (Implementer self-checks)
  red-team        : ∅ (none-exists)
  cross-validation: ∅
  bias-mitigation : ∅
  gate-count      : 1-of-1 (Implementer-self-test)
  ✓ ◐ ○ ✗ : ◐ — ships fast ; ✗ — fails to find composition-bugs ;
              ✗ — confirmation-bias in self-authored tests ;
              ✗ — no spec-drift detection ;
              ✗ — no adversarial probing
```

### §0.2 Target state

```csl
§ TARGET-DISPATCH-MODEL t11-Jα-2
  model              : 5-role-per-slice (Implementer + Reviewer + Critic + Validator + Test-Author)
  review-pattern     : peer-parallel (Reviewer concurrent w/ Implementer ; Critic post-Implementer ; Validator post-Critic)
  test-authoring     : Test-Author from-spec (Implementer cannot weaken)
  spec-conformance   : Validator line-by-line cross-reference at merge-time
  red-team           : Critic adversarial-framing veto-power on severity-HIGH
  cross-validation   : 4 reviewers per slice from 3 different pods
  bias-mitigation    : cross-pod discipline + spec-only-input for Test-Author
  gate-count         : 5-of-5 (Implementer-self-test + Reviewer-OK + Critic-veto-cleared
                                + Test-Author-tests-passing + Validator-spec-conformance)
  ✓ ◐ ○ ✗ : ✓ — finds composition-bugs ; ✓ — adversarial probing ;
              ✓ — spec-drift detection ; ✓ — bias-mitigation ;
              ◐ — slower per-slice ; ◐ — coordination-overhead ↑
```

### §0.3 Cost-benefit framing

```csl
§ TRADEOFF
  hard-work-now ⇒ saves-tokens-later  ← standing-directive
  4× agents per slice ⇒ 4× token-spend per slice
  ⇒ but : avoids-iteration-loops ; avoids-spec-drift ; avoids-broken-merges
  expected : net-savings ≥ 30% ∵ caught-pre-merge ≪ caught-post-merge-via-rework
  W! 5-of-5 = baseline ¬ aspirational
  ¬ "we'll-add-Critic-when-time-permits" ; this is structural
```

### §0.4 Anti-patterns this slice prevents

```csl
§ ANTI-PATTERN-CATALOG
  AP-1 : Implementer-authors-own-tests
         ⇒ confirmation-bias ⇒ blind-spots in failing-paths
  AP-2 : PM-rubber-stamps-merge
         ⇒ surface-validation only ⇒ deep-bugs ship
  AP-3 : Reviewer-from-same-pod
         ⇒ groupthink ⇒ same-mental-model ⇒ same-blind-spots
  AP-4 : Critic-praises-instead-of-criticizes
         ⇒ adversarial-frame collapses ⇒ Critic = expensive-Reviewer
  AP-5 : Validator-skim-skim-OK
         ⇒ spec-drift accumulates ⇒ implementation diverges from spec ;
              spec stops being authority ⇒ codebase becomes the spec ;
              spec-as-authority-property dies
  AP-6 : Test-Author-asks-Implementer-for-hints
         ⇒ contamination ⇒ tests now reflect implementation not spec
  AP-7 : "we'll-iterate-later" without iteration-trigger
         ⇒ tech-debt accretes silently ⇒ N+1th slice carries N slices' debt
```

──────────────────────────────────────────────────────────────────────────

## § 1. ROLE SPECIFICATION — REVIEWER

### §1.1 Role identity

```csl
§ ROLE.Reviewer
  identity          : peer-reviewer .(Implementer's-slice)
  parallelism       : CONCURRENT-with-Implementer (¬ sequential)
  pod-discipline    : DIFFERENT-pod-from-Implementer-of-same-slice
  authority         : REQUEST-iteration ; ESCALATE-to-Architect
  veto-power        : ∅ (cannot-block-merge ; can-only-recommend)
  production-code   : N! (lane-discipline)
  suggested-patches : ✓ (as-comments ¬ as-commits)
```

### §1.2 Mandate

The Reviewer performs peer-review of the Implementer's slice. The work runs **IN PARALLEL** with the Implementer, NOT after. Both agents start at the same time, both work from the same spec-anchor, both end at slice-merge-time. The Reviewer's job is to catch interface-mismatches, spec-anchor-drift, and invariant-violations *before* code lands — not after the Critic has had to red-team broken code.

The Reviewer is a *constructive* critic. Their framing is "is this consistent with the spec + the surrounding API + the invariants the rest of the codebase relies on?" — NOT "what's broken about this?" That latter framing belongs to the Critic. Distinguishing the two roles is essential.

### §1.3 Triggers

```csl
§ Reviewer.Triggers
  T1: dispatched WITH Implementer (same-wave ; same-prompt-batch)
  T2: works from spec-anchor + canonical-API contracts + DECISIONS history
  T3: checks Implementer's slice as it's authored (¬ post-merge)
  T4: checks-cadence : at-each-Implementer-self-checkpoint (∼ every 200 LOC)
  T5: final-pre-merge sign-off W! before slice can advance to Critic stage
```

### §1.4 Authority

```csl
§ Reviewer.Authority
  A1: REQUEST-iteration : Implementer must address Reviewer's findings
                          before slice advances ; same-pod-cycle ; not a re-dispatch
  A2: ESCALATE-to-Architect : if finding implies cross-slice issue
                              (i.e. another-slice's invariants would break)
  A3: ESCALATE-to-Spec-Steward : if finding implies spec is wrong
                                  (NOT implementation-wrong — spec-wrong)
  A4: NO-VETO : Reviewer recommends ; Critic vetoes ; Reviewer escalates
  A5: SIGN-OFF : final pre-merge co-signature on commit-message
```

### §1.5 Cross-pod discipline (CRITICAL)

```csl
§ Reviewer.Pod-Discipline
  W! Reviewer-pod ≠ Implementer-pod  for-the-same-slice
  ∵ groupthink-mitigation
  ∵ same-pod ⇒ same-mental-model ⇒ same-blind-spots
  ⇒ defect-catch-rate degrades to single-agent baseline
  pod-assignment-rule :
    if Implementer-pod = pod-A
       ⇒ Reviewer-pod ∈ {pod-B, pod-C, pod-D} (round-robin or random)
       ⇒ Test-Author-pod ∈ {pod-B, pod-C, pod-D} \ {Reviewer-pod}
       ⇒ (3-pod-minimum on any-given slice)
  pod-definition :
    pod = orchestrator-spawned cohort of agents that share a-context-window-ancestor
    pods identified by orchestrator-tag (e.g. pod-A = wave-4 worktrees W4-{01..06})
  enforcement :
    PM dispatch-table records pod-assignments at-dispatch-time
    cross-pod-violation = block-merge until reassigned
```

### §1.6 Deliverables

```csl
§ Reviewer.Deliverables
  D1 : per-slice review-report (markdown ; lives in worktree)
       sections :
         §1.1 spec-anchor-conformance (✓/◐/○/✗)
         §1.2 API-surface-coherence (✓/◐/○/✗)
         §1.3 invariant-preservation (✓/◐/○/✗)
         §1.4 documentation-presence (rustdoc/godoc on pub items ✓)
         §1.5 findings-list (severity-tagged ; HIGH/MED/LOW)
         §1.6 suggested-patches (as-comments ¬ as-commits)
         §1.7 escalations-raised (if-any ; cross-ref Architect-tickets)
  D2 : pre-merge sign-off block in DECISIONS.md slice-entry
       format :
         §D. Reviewer-Signoff (Reviewer-id W3α-RC-NN ; pod-X) :
           - spec-anchor-conformance : ✓
           - API-surface-coherence   : ✓
           - invariant-preservation  : ✓
           - findings-resolved       : N (all-HIGH closed ; M-MED open)
           - escalations             : ∅
           ⇒ APPROVE-merge-pending-Critic-+-Validator
  D3 : co-signed commit-message trailer :
         "Reviewer-Approved-By: <reviewer-id> <pod-id>"
         appears AFTER Implementer's main commit-message ;
         BEFORE the §11 CREATOR-ATTESTATION trailer
```

### §1.7 Lane discipline

```csl
§ Reviewer.Lane-Discipline
  N! Reviewer writes production-code (¬ in src/ ¬ in tests/)
  N! Reviewer modifies workspace-Cargo.toml
  N! Reviewer touches files-other-than .review-report.md
  ✓ Reviewer writes SUGGESTED-PATCH-blocks-as-comments in review-report
  ✓ Reviewer authors clarifying-questions for Implementer (in-report)
  ✓ Reviewer cross-references DECISIONS history for prior-art
  rationale :
    Reviewer's role = INDEPENDENT-perspective ; if Reviewer writes code,
    Reviewer is now the Implementer's-co-author ⇒ groupthink re-enters
```

### §1.8 Recommended-patch format

When Reviewer wants to suggest specific code-changes, the patch lives in the review-report as a fenced code-block with explicit annotation :

```
### SUGGESTED-PATCH §1.5.3 (severity: MED)
File: compiler-rs/crates/cssl-host-window/src/backend/win32.rs:142
Rationale: WM_CLOSE handler currently returns BOOLs; canonical Win32 wrap
returns LRESULT. Aligns with surrounding handlers.
Patch:
- fn wm_close(hwnd: HWND) -> bool {
+ fn wm_close(hwnd: HWND) -> LRESULT {
      if !consent_check() { return FALSE; }
+     if !consent_check() { return 0; }
      ...
  }
```

The Implementer chooses whether to apply ; choosing not-to-apply requires written rationale in their commit-message.

### §1.9 Iteration trigger

```csl
§ Reviewer.Iteration-Trigger
  trigger : Reviewer.D1 §1.5 contains ≥ 1 HIGH-severity-unresolved finding
  effect  : Implementer iterates SAME-pod-cycle (no re-dispatch)
            Reviewer reviews-the-iteration ; up to 3 cycles
            ≥ 4 cycles ⇒ ESCALATE-to-Architect for re-design
  metric  : Reviewer-iteration-count logged in DECISIONS slice-entry
  bound   : 3 cycles before escalation
```

### §1.10 Anti-pattern : Reviewer-rubber-stamps

```csl
§ Reviewer.Anti-Pattern.Rubber-Stamp
  symptom : review-report all-✓ within 30s of Implementer-checkpoint
  cause   : Reviewer not actually reading
  detection :
    PM samples 1-in-5 reviewer-reports for fidelity-audit
    sample-includes : (a) re-running Reviewer's-checks manually ;
                      (b) comparing review-report against actual-diff
    discrepancy ≥ 1-MED ⇒ Reviewer demoted ; pod re-shuffled
  prevention :
    (i) review-report W! cite specific file:line refs (≥ 5 per HIGH)
    (ii) review-report W! contain ≥ 1 question-or-suggestion-or-finding
         (rubber-stamp-detection : 0-findings ⇒ flag-for-audit)
    (iii) Reviewer-id rotates per-pod-per-wave ; same-Reviewer rarely
          reviews back-to-back from same Implementer
```

──────────────────────────────────────────────────────────────────────────

## § 2. ROLE SPECIFICATION — CRITIC (RED-TEAM)

### §2.1 Role identity

```csl
§ ROLE.Critic
  identity     : adversarial-reviewer .(Implementer's-slice)
  parallelism  : POST-Implementer ; PRE-merge gate
  pod-discipline : MAY-overlap-with-Reviewer-pod (different-mental-mode preferred)
  authority    : VETO-merge on severity-HIGH ; REQUEST-redesign
  framing      : EXPLICITLY-find-flaws ¬ praise
  production-code : N! (lane-discipline)
  failure-fixtures : ✓ (force-Implementer-to-pass)
```

### §2.2 Mandate

The Critic is the red-team. Their explicit, mandatory framing is *adversarial* : they are trying to BREAK the deliverable. The output is a list of failure-modes, edge-cases, composition-issues, and invariant-violations — concrete and actionable.

This is NOT another peer-review. The Reviewer asked "is this good?" The Critic asks "if I were trying to make this fail, where would I push?" The two questions are different ; the failure-mode catalog is different ; and the cost of conflating them is that an entire layer of defense vanishes.

The Critic's working-assumption is that the Implementer is wrong. Not as personal-attack — as *framing-discipline*. The Critic searches for the place where the assumption proves false, and either finds it (writes a failure-fixture-test) or exhausts the search-space (writes "tried these N attacks, none succeeded ; promotion-attempts logged for review").

### §2.3 Triggers

```csl
§ Critic.Triggers
  T1: dispatched AFTER Implementer's-slice-complete (Reviewer.D2 already-signed)
  T2: pre-merge gate (slice cannot advance without Critic.D1 + Critic.D2)
  T3: works from spec + Implementer's-final-diff + Reviewer's-D1 report
  T4: explicitly-NOT-from-Implementer-rationale (avoid contamination)
  T5: budget : 1-pass deep ; up-to-3-passes if HIGH-found-and-fixed
```

### §2.4 Authority

```csl
§ Critic.Authority
  A1: VETO-merge on severity-HIGH issue
      ⇒ Implementer iterates ; new-pod-cycle (NOT same-pod-cycle ; ∵ severity-HIGH = re-design needed)
  A2: REQUEST-redesign : if finding implies architecture-flaw
      ⇒ ESCALATE-to-Architect for cross-slice rework
  A3: NO-VETO on severity-MED : recommendation only ; logged for follow-up
  A4: NO-VETO on severity-LOW : tracked-but-not-blocking
  A5: 3-pod-cycle-bound : ≥ 4th HIGH-fail-and-re-design ⇒ slice-rejected ; replan
```

### §2.5 Adversarial framing — the discipline

```csl
§ Critic.Framing.Discipline
  W! Critic prompts contain explicit adversarial-language :
    "If I were trying to break this, I would..."
    "What's the failure-mode of this assumption?"
    "Where does this fall apart at scale / under composition / at boundary?"
    "What's an input the Implementer didn't think of?"
  N! Critic-language : "looks-good" ; "approved" ; "no-issues-found" without effort
  N! Critic-output omits failure-modes-attempted (must list ≥ 5 attempts even if all-survived)
  rationale :
    if Critic-prompt sounds like Reviewer-prompt, Critic = expensive-Reviewer
    framing-discipline is the role-distinguishing-factor
  enforcement :
    Critic-report MUST include §3 failure-modes-attempted (≥ 5 even if all-survived)
    Critic-report MUST include §4 severity-classification per finding
    Critic-report MUST NOT include "looks-good" without ≥ 5 attempts catalogued
    PM samples 1-in-5 Critic-reports ; rubber-stamp ⇒ pod-rotation
```

### §2.6 Severity classification

```csl
§ Critic.Severity
  HIGH (veto) :
    H1 : invariant-violation (can-be-triggered-from-public-API)
    H2 : data-corruption-path (silent ¬ panic)
    H3 : PRIME-DIRECTIVE-conflict (consent-bypass ; surveillance-leak ;
                                    biometric-channel ; harm-vector)
    H4 : security-vulnerability (auth-bypass ; unsafe-public-API)
    H5 : spec-conformance-fail (implementation contradicts spec)
    H6 : composition-failure with another-merged-slice
  MEDIUM (recommend) :
    M1 : edge-case-untested (e.g. UTF-16-surrogate ; empty-input ; max-int)
    M2 : performance-cliff (e.g. O(n²) where O(n) expected)
    M3 : error-message-quality (cryptic ; non-actionable)
    M4 : documentation-gap (rustdoc missing on pub item)
    M5 : test-coverage-gap (¬ failing ; ¬ tested-at-all)
  LOW (track) :
    L1 : style-divergence
    L2 : naming-suggestion
    L3 : refactor-opportunity
    L4 : unused-import
```

### §2.7 Deliverables

```csl
§ Critic.Deliverables
  D1 : red-team-report (markdown ; lives in worktree)
       sections :
         §1 attack-surface-mapped (what the slice exposes)
         §2 invariants-asserted (what the slice claims)
         §3 failure-modes-attempted (≥ 5 attacks ; pass/fail per attack)
         §4 findings-list (HIGH/MED/LOW classification ; cross-ref §3)
         §5 mitigations-proposed (per-finding ; concrete-code-or-design)
         §6 veto-flag (TRUE if any-HIGH ; FALSE-otherwise)
  D2 : failure-fixture tests (in worktree's tests/)
       — Critic AUTHORS failing-tests that the Implementer-must-make-pass
       — these tests live alongside Test-Author's tests
       — Implementer cannot weaken or skip ; only fix-implementation-to-pass
  D3 : DECISIONS slice-entry block :
       §D. Critic-Veto (Critic-id W3α-CR-NN ; pod-Y) :
           - veto-flag      : FALSE
           - HIGH-resolved  : N (post-Implementer-iteration)
           - MED-tracked    : M (deferred-to-followup-slice ; cross-ref ticket)
           - LOW-tracked    : K
           - failure-fixtures-authored : F-count
           ⇒ APPROVE-merge-pending-Validator
  D4 : if veto-flag = TRUE, slice-blocked ; Implementer must iterate ;
       Critic re-runs after Implementer-iteration ; up-to-3-cycles
```

### §2.8 Failure-fixture authority

The Critic's failure-fixtures are LOAD-BEARING : the Implementer MUST make them pass before the slice can merge. The Implementer cannot mark them `#[ignore]` or delete them. The Implementer can request a fixture-revision (if e.g. the fixture itself encodes a wrong invariant) ; this requires written rationale and Critic-counter-signature.

```csl
§ Critic.Failure-Fixture.Authority
  W! Implementer makes Critic-fixtures pass before merge
  N! Implementer marks Critic-fixtures `#[ignore]`
  N! Implementer deletes Critic-fixtures
  N! Implementer skips Critic-fixtures via `cfg(test, ...)` excludes
  ✓ Implementer requests fixture-revision via DECISIONS.md sub-entry
        — written rationale required
        — Critic counter-signs (or rejects ; up-to-3 cycles ⇒ ESCALATE)
  ⇒ Critic-fixtures = part-of-the-spec for this slice
```

### §2.9 Lane discipline

```csl
§ Critic.Lane-Discipline
  N! Critic writes production-code (¬ in src/)
  N! Critic modifies workspace-Cargo.toml
  ✓ Critic writes failure-fixture-tests (in tests/)
  ✓ Critic writes failing-property-tests (in tests/)
  ✓ Critic writes adversarial-input fixtures (in test-data/)
  ✓ Critic authors red-team-report.md
```

### §2.10 Iteration trigger

```csl
§ Critic.Iteration-Trigger
  trigger : Critic.D1.§6 veto-flag = TRUE
  effect  : Implementer iterates ; NEW-pod-cycle (≠ same-pod-cycle)
            ∵ severity-HIGH ⇒ likely-architectural ⇒ fresh-perspective needed
  cycle-count : N+1
  bound : 3-cycles ⇒ ESCALATE-Architect for re-design (replan slice)
  metric : Critic-iteration-count logged in DECISIONS slice-entry
```

### §2.11 Anti-pattern : Critic-praises

```csl
§ Critic.Anti-Pattern.Praise
  symptom : Critic-report contains "looks-good" or "well-designed" or
            "no-issues-found" without §3-failure-modes-attempted ≥ 5
  cause   : Critic-prompt didn't enforce adversarial-framing ; or
            Critic-agent reverted-to-default-helpfulness
  detection :
    PM samples 1-in-5 Critic-reports for adversarial-fidelity audit
    sample-checks :
      (a) §3 failure-modes-attempted ≥ 5 ?
      (b) ≥ 1-finding logged at any-severity ?
      (c) language is adversarial-not-deferential ?
    fail-any ⇒ Critic demoted ; pod-rotation
  prevention :
    (i) Critic-prompt-template enforces adversarial-framing in Critic-onboarding
    (ii) Critic-report MUST list ≥ 5 attack-attempts even if all-survived
    (iii) zero-findings is a red-flag ¬ a green-flag
    (iv) Critic-id rotates per-pod-per-wave ; same-Critic rarely
         reviews back-to-back from same Implementer
```

──────────────────────────────────────────────────────────────────────────

## § 3. ROLE SPECIFICATION — VALIDATOR

### §3.1 Role identity

```csl
§ ROLE.Validator
  identity         : spec-conformance auditor at-merge-time
  parallelism      : AFTER-Critic-veto-resolved ; PRE-parallel-fanout-merge
  reports-to       : Spec-Steward (escalation-target)
  authority        : REJECT-merge for spec-drift
  cross-references : Omniverse-spec ; CSSLv3-specs/ ; DECISIONS-history
  production-code  : N! (lane-discipline)
  test-coverage-map : ✓ (links spec-§ → impl file:line → test-fn)
```

### §3.2 Mandate

The Validator is the spec-conformance gate at merge-time. Their job is to cross-reference the implementation against the canonical spec line-by-line — both the Omniverse spec corpus AND the CSSLv3 `specs/` directory — and against the DECISIONS history. The output is a *spec-conformance-report* with a line-by-line cross-reference table : spec § X.Y → impl file:line → test-fn.

The Validator's question is not "is this good?" (Reviewer) and not "is this broken?" (Critic). It is "**does this match what we said we were building**?"

The Validator's reports-to is the Spec-Steward — because spec-drift detection is fundamentally a spec-authority concern. If the Validator finds drift, two things can happen :
- the implementation is wrong → Implementer iterates
- the spec is wrong → Spec-Steward proposes amendment

The Validator's job is to *detect* the drift. The Spec-Steward's job is to decide which side to fix.

### §3.3 Triggers

```csl
§ Validator.Triggers
  T1: dispatched AFTER Critic.D3 veto-flag = FALSE
  T2: final-gate before parallel-fanout-merge
  T3: works from :
       (a) spec-anchor cited in Implementer's-DECISIONS-entry
       (b) Omniverse-spec-corpus (full-search if cross-cutting)
       (c) CSSLv3 specs/ corpus (full-search if cross-cutting)
       (d) DECISIONS history (prior-decisions on this surface)
       (e) Implementer's-final-diff
       (f) Test-Author's test-suite + Critic's failure-fixtures
  T4: budget : 1-pass deep ; up-to-2-passes if drift-found-and-fixed
```

### §3.4 Authority

```csl
§ Validator.Authority
  A1: REJECT-merge for spec-drift
      ⇒ either : (a) Implementer iterates same-pod-cycle, OR
                 (b) Spec-Steward proposes spec-amendment (fast-track DECISIONS entry)
  A2: REPORTS-TO Spec-Steward : drift-findings flagged for spec-authority decision
  A3: NO-VETO on style-divergence (Reviewer's domain) ;
       NO-VETO on red-team-failure-modes (Critic's domain)
  A4: ESCALATE-to-PM : if drift implies multi-slice-rework
```

### §3.5 Spec-conformance line-by-line

```csl
§ Validator.Methodology
  W! Validator produces a TABLE :
    | spec-§     | spec-claim                          | impl file:line                    | test-fn-coverage             |
    |-----------|-------------------------------------|-----------------------------------|------------------------------|
    | 04_EFF§3.1| @effect Travel = consent-required   | crates/cssl-effects/src/travel.rs:42 | tests/travel_consent.rs::t1 |
    | 04_EFF§3.2| Travel preserves agency-trace       | crates/cssl-effects/src/travel.rs:58 | tests/travel_trace.rs::t2  |
    | ...        | ...                                 | ...                                | ...                          |
  W! every-spec-claim has ≥ 1-impl-anchor (file:line)
  W! every-spec-claim has ≥ 1-test-fn-coverage (Test-Author or Critic)
  ✗ MISSING-spec-claim ⇒ HIGH-severity drift (impl missing spec'd feature)
  ✗ EXTRA-impl-feature ⇒ HIGH-severity drift (impl has feature not-spec'd)
  ✓ both-sides-covered ⇒ Validator approves
```

### §3.6 Gap-list — what's spec'd-but-not-implemented and what's implemented-but-not-spec'd

```csl
§ Validator.Gap-List
  for-every-slice :
    SECTION-A : spec'd-but-not-implemented (severity-HIGH default)
                ← rationale : spec is authority ; missing-impl = breach
                ← exception : spec-§-marked-future (cross-ref future-slice ID)
    SECTION-B : implemented-but-not-spec'd (severity-HIGH default)
                ← rationale : implementation w/o spec = scope-creep ; spec-stops-being-authority
                ← resolution : either (a) Spec-Steward amends spec to include feature
                              OR (b) Implementer removes feature from slice
                              ← decided-via DECISIONS slice-sub-entry
  ¬ "minor-helper-funcs" exception : even-helpers must be discoverable-from-spec
  ¬ "private-impl-detail" exception : public-API must be 100%-spec-anchored ;
                                      private-impl may diverge but file-level
                                      module-doc must cite the spec-§ it serves
```

### §3.7 Deliverables

```csl
§ Validator.Deliverables
  D1 : spec-conformance-report (markdown ; lives in worktree)
       sections :
         §1 spec-anchor-resolution (every spec-§ resolved to impl file:line)
         §2 test-coverage-map (every spec-§ resolved to ≥ 1 test-fn)
         §3 gap-list-A : spec'd-but-not-impl (HIGH/MED/LOW)
         §4 gap-list-B : impl-but-not-spec'd (HIGH/MED/LOW)
         §5 cross-spec-consistency (CSSLv3 vs Omniverse — drift between them)
         §6 DECISIONS-history-consistency (prior-decisions still-honored)
         §7 verdict (APPROVE/REJECT) ; reject-rationale-if-applicable
  D2 : DECISIONS slice-entry block :
       §D. Validator-Conformance (Validator-id W3α-VL-NN) :
           - spec-§-resolved : N/N (100% required)
           - test-coverage  : N/N (100% required)
           - gap-list-A     : K (must-be-zero-HIGH for-merge)
           - gap-list-B     : J (must-be-zero-HIGH for-merge)
           - verdict        : APPROVE / REJECT
           - escalated-to   : ∅ / Spec-Steward / PM
           ⇒ APPROVE-merge-pending-final-PM-sign-off
  D3 : test-coverage-map artifact (machine-readable JSON or TOML)
       lives in worktree at `audit/spec_coverage.toml`
       schema :
         [[mapping]]
         spec_section = "04_EFFECTS § Travel"
         impl_file    = "crates/cssl-effects/src/travel.rs"
         impl_line    = 42
         test_fns     = ["tests::travel_consent::t1", "tests::travel_trace::t2"]
       this artifact is preserved post-merge for future-Validator-runs to diff against
```

### §3.8 Lane discipline

```csl
§ Validator.Lane-Discipline
  N! Validator writes production-code (¬ in src/)
  N! Validator writes failing-tests (Critic's domain ; Test-Author's domain)
  ✓ Validator writes test-coverage-map (audit/ subdirectory)
  ✓ Validator authors spec-conformance-report
  ✓ Validator may author DECISIONS-sub-entries flagging drift
```

### §3.9 Iteration triggers

```csl
§ Validator.Iteration-Triggers
  T1 : drift-found-impl-wrong (spec'd but mis-implemented or unimplemented)
       ⇒ Implementer iterates SAME-pod-cycle
       ⇒ Validator re-runs ; up-to-2-cycles ; ≥ 3rd ⇒ ESCALATE
  T2 : drift-found-spec-wrong (implementation right ; spec was incomplete)
       ⇒ Spec-Steward proposes spec-amendment via fast-track DECISIONS entry
       ⇒ Validator re-runs ; up-to-2-cycles ; ≥ 3rd ⇒ ESCALATE
  T3 : drift-found-cross-spec-inconsistency (CSSLv3 vs Omniverse disagree)
       ⇒ Spec-Steward decides authoritative-side ;
       ⇒ amendment flows through normal SYNTHESIS_V2 channel
```

### §3.10 Anti-pattern : Validator-skim

```csl
§ Validator.Anti-Pattern.Skim
  symptom : spec-conformance-report says "all-good" without
            line-by-line table populated
  cause   : Validator skimmed instead of cross-referenced
  detection :
    PM samples 1-in-5 Validator-reports
    sample-checks :
      (a) §1 spec-anchor-resolution table present + populated ?
      (b) every spec-§ has impl-file:line cited ?
      (c) every spec-§ has test-fn cited ?
      (d) §3 + §4 gap-lists actually-enumerated (not "none-found") ?
    fail-any ⇒ Validator demoted ; pod-rotation
  prevention :
    (i) Validator-report-template enforces table-population
    (ii) test-coverage-map JSON/TOML required ⇒ machine-checkable for completeness
    (iii) Validator-id rotates per-pod-per-wave
```

──────────────────────────────────────────────────────────────────────────

## § 4. ROLE SPECIFICATION — TEST-AUTHOR

### §4.1 Role identity

```csl
§ ROLE.Test-Author
  identity      : test-author for-Implementer's-slice ; from-spec-only
  parallelism   : CONCURRENT-with-Implementer (¬ sequential)
  pod-discipline : DIFFERENT-pod-from-Implementer-of-same-slice
                   AND DIFFERENT-pod-from-Reviewer-of-same-slice (3-pod-min)
  authority     : tests are LOAD-BEARING ; Implementer cannot weaken
  input-source  : SPEC ONLY (¬ Implementer-code ; ¬ Implementer-rationale)
  production-code : N! (lane-discipline)
  fixtures + golden-bytes + property-tests : ✓
```

### §4.2 Mandate

The Test-Author writes the tests that the Implementer's code must pass. The work runs **IN PARALLEL** with the Implementer, NOT after. Both agents start at the same time, both work from the same spec-anchor.

The single most important property of the Test-Author : **they author tests from the SPEC, NOT from the implementation**. This mitigates the deepest bias-source in single-agent dispatch — an Implementer authoring their own tests will write tests that pass on their implementation, including the bugs. The bugs become invisible because the test suite encodes them.

The Test-Author breaks this loop. They never see the Implementer's code until merge-time. They never ask the Implementer for hints. Their input is the spec. Their output is a test-suite that the Implementer must make green. If the Implementer's interpretation of the spec differs from the Test-Author's interpretation, both interpretations come into conflict at test-run-time, and either : (a) the spec is ambiguous (escalate to Spec-Steward), or (b) one agent misread (the conflict surfaces the misread).

### §4.3 Triggers

```csl
§ Test-Author.Triggers
  T1: dispatched WITH Implementer (same-wave ; same-prompt-batch)
  T2: works from spec-anchor + canonical-API-contracts + DECISIONS-history
  T3: NEVER sees Implementer's-code-during-authoring
       ← orchestrator-enforced : Test-Author worktree lacks Implementer's-branch checkout
  T4: NEVER asks Implementer for hints
       ← if-spec-ambiguous : ESCALATE-Spec-Steward, do-not-ask-Implementer
  T5: sync-point : at-merge-time, Test-Author runs Implementer's-code through
                   their-test-suite ; failures-block-merge
```

### §4.4 Authority

```csl
§ Test-Author.Authority
  A1: tests are LOAD-BEARING
      ⇒ Implementer cannot weaken (¬ #[ignore] ¬ delete ¬ skip-via-cfg)
      ⇒ Implementer can request test-revision via DECISIONS sub-entry ;
        Test-Author counter-signs (or rejects ; up-to-3 cycles ⇒ ESCALATE)
  A2: tests are SPEC-AUTHORITATIVE
      ⇒ if Test-Author's-test ≠ spec, Test-Author iterates (test-bug)
      ⇒ if Test-Author's-test = spec but Implementer's-code ≠ spec,
        Implementer iterates (impl-bug)
  A3: TEST-SUITE-OWNERSHIP : Test-Author owns the slice's test-suite ;
      Critic's failure-fixtures sit alongside but are Critic's-property
  A4: ESCALATE-Spec-Steward if spec is ambiguous-or-incomplete
```

### §4.5 Cross-pod discipline (CRITICAL)

```csl
§ Test-Author.Pod-Discipline
  W! Test-Author-pod ≠ Implementer-pod for-the-same-slice
  W! Test-Author-pod ≠ Reviewer-pod for-the-same-slice
  ⇒ 3-pod-minimum on any-given slice (Implementer + Reviewer + Test-Author all-different)
  ∵ confirmation-bias-mitigation
  ∵ same-pod ⇒ same-mental-model ⇒ same-spec-interpretation ⇒ same-blind-spots
  pod-assignment-rule (extends Reviewer.Pod-Discipline) :
    if Implementer-pod = pod-A, Reviewer-pod = pod-B
       ⇒ Test-Author-pod ∈ {pod-C, pod-D, ...} (round-robin or random)
  enforcement :
    PM dispatch-table records pod-assignments at-dispatch-time
    cross-pod-violation = block-merge until reassigned
```

### §4.6 Spec-only-input discipline

```csl
§ Test-Author.Input-Discipline
  ✓ INPUT : spec-anchor (CSSLv3 specs/ + Omniverse + DECISIONS history)
  ✓ INPUT : canonical-API contracts published in cssl-* crates
  ✓ INPUT : prior-test-suites in workspace (for-style-consistency)
  N! INPUT : Implementer's-current-branch (orchestrator denies-checkout)
  N! INPUT : Implementer's-prompt or rationale-comments
  N! INPUT : Implementer's-DECISIONS-entry (read-only-after-test-suite-final)
  N! ASK : Test-Author asks Implementer "what should this return for X?"
            ← contamination ; contamination-source = the role's bias-vector
            ← if spec-ambiguous, ESCALATE Spec-Steward ; else use-spec
  rationale :
    spec-only-input is the ENTIRE bias-mitigation property of this role
    if-violated, role collapses to "Implementer-with-extra-steps"
```

### §4.7 Deliverables

```csl
§ Test-Author.Deliverables
  D1 : test-suite per-slice (tests/ directory in slice-worktree)
       structure :
         tests/
           <slice-id>_acceptance/      ← per-spec-acceptance-criterion test-fn
           <slice-id>_property/        ← property-based-tests (proptest crate)
           <slice-id>_golden_bytes/    ← deterministic-output fixtures
           <slice-id>_negative/        ← invalid-input handling (per spec)
       coverage :
         100% of spec-§-acceptance-criteria ⇒ ≥ 1 test-fn each
         negative-paths ⇒ ≥ 1 test-fn per spec-§-error-condition
  D2 : per-spec-acceptance-criterion mapping (test-coverage-map artifact)
       feeds into Validator's D3 (spec-coverage TOML/JSON)
  D3 : DECISIONS slice-entry block :
       §D. Test-Author-Suite (Test-Author-id W3α-TA-NN ; pod-Z) :
           - spec-§-coverage : N/N (100% required)
           - test-fn-count   : N
           - golden-fixtures : G
           - property-tests  : P
           - negative-tests  : E
           - pod-id          : pod-Z (cross-pod-confirmed-≠-Implementer-pod-≠-Reviewer-pod)
           ⇒ APPROVE-merge-pending-Critic-+-Validator
  D4 : if any test-fails-on-Implementer's-code at merge-time :
         block-merge ; Implementer iterates same-pod-cycle ;
         Test-Author re-runs after iteration ;
         up-to-3-cycles before ESCALATE
```

### §4.8 Test-categories required

```csl
§ Test-Author.Categories
  C1 : ACCEPTANCE — directly maps to spec-acceptance-criterion
       e.g. spec § "Window emits Close-event when WM_CLOSE received" ⇒
            test : assert!(window.next_event() == Some(Close { ... })) after WM_CLOSE
  C2 : PROPERTY — invariant-based-testing via proptest
       e.g. spec § "Trip<T> always preserves T's ordering" ⇒
            proptest : ∀ vec ; trip(sort(vec)) = sort(vec)
  C3 : GOLDEN-BYTES — deterministic-output fixtures
       e.g. spec § "BLAKE3(empty) = af1349b9..." ⇒
            test : assert!(hash(b"") == golden_bytes_blake3_empty())
  C4 : NEGATIVE — invalid-input handling per spec error-conditions
       e.g. spec § "Window.spawn returns InvalidConfig if size = 0" ⇒
            test : assert!(spawn(WindowConfig{ size: 0, ..}).is_err_invalid_config())
  C5 : COMPOSITION — interaction-with-other-slices' surfaces
       e.g. spec § "Window-handle round-trips into D3D12-swapchain" ⇒
            test : create-window ; pass-handle-to-d3d12 ; assert-round-trip
  ≥ 1-test-fn per spec-§ for-each-applicable-category
```

### §4.9 Lane discipline

```csl
§ Test-Author.Lane-Discipline
  N! Test-Author writes production-code (¬ in src/)
  N! Test-Author modifies workspace-Cargo.toml (except tests-only deps : proptest, etc.)
  ✓ Test-Author writes tests in tests/
  ✓ Test-Author writes fixtures in test-data/
  ✓ Test-Author writes golden-bytes in tests/golden_*
  ✓ Test-Author writes property-tests using proptest crate
```

### §4.10 Iteration trigger

```csl
§ Test-Author.Iteration-Trigger
  T1 : Test-Author's-tests-fail on Implementer's-code at merge-time
       ⇒ Implementer iterates SAME-pod-cycle (no re-dispatch)
       ⇒ Test-Author re-runs ; up-to-3-cycles before ESCALATE
  T2 : Implementer claims Test-Author's-test ≠ spec (test-bug-claim)
       ⇒ DECISIONS sub-entry filed ; Test-Author counter-signs or rejects ;
       ⇒ Spec-Steward arbitrates if disputed
       ⇒ up-to-3-cycles before ESCALATE
  metric : Test-Author iteration-count logged in DECISIONS slice-entry
```

### §4.11 Anti-pattern : Test-Author-asks-Implementer

```csl
§ Test-Author.Anti-Pattern.Ask-Implementer
  symptom : Test-Author messages Implementer-channel with "should X return Y?"
  cause   : spec is ambiguous, Test-Author took shortcut
  detection :
    orchestrator monitors cross-agent message-traffic
    Test-Author → Implementer messages = flagged-event
    PM reviews flagged-events at slice-merge-time
    confirmed-violation ⇒ Test-Author tests REJECTED ; new Test-Author dispatched
  prevention :
    (i) Test-Author-prompt-template makes ESCALATE-Spec-Steward the only allowed path
    (ii) orchestrator severs cross-agent messaging at slice-dispatch ;
         only Spec-Steward channel is open
    (iii) Test-Author-id rotates per-pod-per-wave
```

──────────────────────────────────────────────────────────────────────────

## § 5. COLLABORATION PATTERNS — TIMING + CONCURRENCY

### §5.1 Phase diagram

```
TIME →
─────┬──────────────────────────────────────────────────────────────────────┬────
     │  CONCURRENT (parallel-dispatched)                                    │
     │ ┌──────────────────────────────┐                                     │
     │ │ Implementer (pod-A)           │                                    │
     │ │ Reviewer    (pod-B)           │  ←── parallel (cross-pod)         │
     │ │ Test-Author (pod-C)           │                                    │
     │ └──────────────────────────────┘                                     │
     │                                                                       │
     │     ▼                                                                 │
     │  ┌─────────────────────────────────────────────────────────────────┐ │
     │  │ Reviewer.Sign-off (D2)                                          │ │
     │  │ Test-Author.Suite-Final (D3)                                    │ │
     │  │ Implementer.Self-test green                                     │ │
     │  └─────────────────────────────────────────────────────────────────┘ │
     │     ▼                                                                 │
     │  ┌─────────────────────────────────────────────────────────────────┐ │
     │  │ Critic (pod-D ; sequential-after Implementer-final)             │ │
     │  └─────────────────────────────────────────────────────────────────┘ │
     │     ▼                                                                 │
     │  ┌─────────────────────────────────────────────────────────────────┐ │
     │  │ Validator (cross-cutting ; sequential-after Critic.veto-clear)  │ │
     │  └─────────────────────────────────────────────────────────────────┘ │
     │     ▼                                                                 │
     │  ┌─────────────────────────────────────────────────────────────────┐ │
     │  │ Final-PM-merge to parallel-fanout                               │ │
     │  └─────────────────────────────────────────────────────────────────┘ │
─────┴──────────────────────────────────────────────────────────────────────┴────
```

### §5.2 Concurrency rules

```csl
§ Concurrency-Rules
  R1 : Implementer + Reviewer + Test-Author run-CONCURRENTLY
       ← all-three dispatched in same orchestrator-batch
       ← all-three start at t=0
       ← Reviewer-checkpoints align with Implementer-checkpoints (every ~200 LOC)
       ← Test-Author runs spec-only ; no checkpoint-coordination
  R2 : Critic dispatched-AFTER Implementer-self-test-green + Reviewer.D2 + Test-Author.D3
       ← single-agent ; up-to-3 internal-iteration-passes
       ← veto-flag drives Implementer iteration if HIGH-found
  R3 : Validator dispatched-AFTER Critic.D3.veto-flag = FALSE
       ← single-agent ; up-to-2 internal-iteration-passes
       ← spec-drift may trigger Implementer iteration OR Spec-Steward amendment
  R4 : PM-merge dispatched-AFTER Validator.D2.verdict = APPROVE
       ← merge to parallel-fanout
       ← DECISIONS-entry finalized with all-5-role sign-offs
```

### §5.3 Critical path

```csl
§ Critical-Path
  optimistic : Impl + Rev + TA parallel = max(Impl, Rev, TA)
                ≈ Impl-time (Rev/TA ≤ Impl)
  + Critic   : sequential ≈ 0.5 × Impl
  + Validator: sequential ≈ 0.3 × Impl
  total      : Impl-time × ~1.8 (vs current ~1.0)
  iteration-pessimistic : Impl-time × ~3.0 (1-cycle Critic + 1-cycle Validator)

  comparison-vs-current :
    current (1-of-5) : Impl-time × ~1.0
    new (5-of-5)     : Impl-time × ~1.8 (no-iteration) ; ~3.0 (1-cycle)

  rationale-for-1.8× :
    catches-bugs-pre-merge ⇒ saves N×Impl-time later in re-work + cross-slice impact
    expected-net-savings ≥ 30% in stale-bug-rework time
```

──────────────────────────────────────────────────────────────────────────

## § 6. THE 5-OF-5 GATE — CONCRETE ENFORCEMENT

### §6.1 Gate definition

A slice cannot merge to `cssl/session-12/parallel-fanout` until **ALL FIVE** of the following are TRUE :

```csl
§ 5-of-5 Gate
  G1 : Implementer-self-test green
       ← cargo test -p <slice-crate> -- --test-threads=1 = N/N pass
       ← cargo clippy -p <slice-crate> --all-targets -- -D warnings = clean
       ← cargo fmt --all-check = clean
  G2 : Reviewer-sign-off (Reviewer.D2 in DECISIONS slice-entry)
       ← spec-anchor-conformance ✓
       ← API-surface-coherence ✓
       ← invariant-preservation ✓
       ← all-HIGH findings resolved
  G3 : Critic-veto-cleared (Critic.D3 in DECISIONS slice-entry)
       ← veto-flag = FALSE
       ← all-HIGH findings resolved
       ← Critic-failure-fixtures pass
  G4 : Test-Author-tests-passing (Test-Author.D3 + actual-test-run)
       ← Test-Author's tests run on Implementer's code = N/N pass
       ← spec-§-coverage = 100%
       ← cross-pod confirmed (Implementer-pod ≠ Test-Author-pod)
  G5 : Validator-spec-conformance (Validator.D2 in DECISIONS slice-entry)
       ← spec-§-resolved : 100%
       ← test-coverage-map populated
       ← gap-list-A (spec'd-but-not-impl) = zero-HIGH
       ← gap-list-B (impl-but-not-spec'd) = zero-HIGH
       ← verdict = APPROVE

  GATE-RESULT = G1 ∧ G2 ∧ G3 ∧ G4 ∧ G5

  W! ALL-FIVE-true for merge
  N! 4-of-5 sufficient
  N! "we'll fix it post-merge" exception
  N! "minor finding" downgrade-without-rationale
```

### §6.2 Mechanical enforcement

```csl
§ Enforcement-Mechanism
  pre-merge-script (scripts/check_5_of_5_gate.sh) :
    parses DECISIONS slice-entry
    confirms presence + APPROVE-state of G1..G5 sub-entries
    confirms cross-pod-discipline recorded (3 different pods)
    confirms commit-message-trailer has all-5 sign-offs :
      - Reviewer-Approved-By:  <id> <pod>
      - Critic-Veto-Cleared-By: <id> <pod>
      - Test-Author-Suite-By:   <id> <pod>
      - Validator-Approved-By:  <id>
      - Implementer:            <id> <pod>
    fail-any ⇒ exit-1 ⇒ pre-commit-hook blocks merge
  PM-orchestrator :
    runs pre-merge-script as part of integration-slice
    refuses merge-commit if exit-1
    surfaces blocker to Apocky for manual-review-or-intervention
```

### §6.3 Commit-message format

```
§ T11-D### : <slice-title> — <one-line-summary>

<body : per-existing-DECISIONS conventions>

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

### §6.4 Currently 1-of-5 — what we're moving away from

```csl
§ CURRENT-1-OF-5
  G1' : Implementer-self-test green
  G2' : ∅
  G3' : ∅
  G4' : ∅
  G5' : ∅
  ⇒ 1-of-5 ⇒ slice ships
```

```csl
§ TARGET-5-OF-5
  G1 ∧ G2 ∧ G3 ∧ G4 ∧ G5
  ⇒ 5-of-5 ⇒ slice ships
  ⇒ ↑ defect-catch ; ↑ spec-fidelity ; ↑ bias-mitigation ;
    ↓ post-merge rework
```

──────────────────────────────────────────────────────────────────────────

## § 7. ITERATION TRIGGERS — CONSOLIDATED

### §7.1 Iteration matrix

```csl
§ Iteration-Triggers.Matrix

  | Trigger                                   | Iterates       | Pod-cycle  | Bound      | Escalate-to    |
  |-------------------------------------------|----------------|------------|------------|----------------|
  | Critic-veto severity-HIGH                 | Implementer    | NEW-pod    | 3-cycles   | Architect      |
  | Reviewer-request-iteration (HIGH-finding) | Implementer    | SAME-pod   | 3-cycles   | Architect      |
  | Validator-spec-drift (impl-wrong)         | Implementer    | SAME-pod   | 2-cycles   | Spec-Steward   |
  | Validator-spec-drift (spec-wrong)         | Spec-Steward   | (amendment)| 2-cycles   | PM             |
  | Test-Author-tests-failing                 | Implementer    | SAME-pod   | 3-cycles   | Architect      |
  | Test-Author-test-revision-claim           | Test-Author    | SAME-pod   | 3-cycles   | Spec-Steward   |
  | Critic-fixture-revision-claim             | Critic         | SAME-pod   | 3-cycles   | PM             |
  | Cross-pod-violation                       | (re-dispatch)  | NEW-pod    | 1-cycle    | PM             |
  | 4th HIGH-fail-and-redesign                | (slice-replan) | (replan)   | 0-cycles   | Architect + PM |
```

### §7.2 Iteration ledger

```csl
§ Iteration-Ledger
  W! every-iteration-cycle logged in DECISIONS slice-entry sub-block
  format :
    §D. Iteration-Cycle-N (date) :
      - trigger        : <Critic-HIGH | Reviewer-HIGH | Validator-drift | Test-fail>
      - source-finding : <ref-to-finding-id>
      - iterating-role : <Implementer | Spec-Steward | Critic | Test-Author>
      - pod-cycle      : <SAME | NEW>
      - resolution     : <pass | fail-and-escalate>
  metric : per-slice iteration-count = sum-of-cycles-across-all-triggers
  bound  : ≤ 6-total-iterations per-slice ⇒ slice replanned
```

### §7.3 Escalation sequencing

```csl
§ Escalation-Sequencing
  level-1 : Architect (cross-slice issue ; design-flaw)
  level-2 : Spec-Steward (spec-incompleteness ; cross-spec-drift)
  level-3 : PM (multi-slice-rework ; resource-contention)
  level-4 : Apocky (PRIME-DIRECTIVE-adjacent ; hard-call)

  routing rule :
    Critic-HIGH-architectural ⇒ level-1
    Reviewer-cross-slice ⇒ level-1
    Validator-spec-amendment ⇒ level-2
    Test-Author-spec-ambiguous ⇒ level-2
    Cross-pod-violation ⇒ level-3
    PRIME-DIRECTIVE-edge-case ⇒ level-4
```

──────────────────────────────────────────────────────────────────────────

## § 8. ANTI-PATTERN CATALOG (CONSOLIDATED)

### §8.1 Reviewer rubber-stamp

Detection : review-report all-✓ within 30s of Implementer-checkpoint, no specific file:line refs.
Effect : pod-rotation ; demote Reviewer ; PM-audit-1-in-5.
Prevention : enforce ≥ 5 specific findings or questions in review-report.

### §8.2 Critic praises

Detection : "looks good" or "no-issues-found" without §3 failure-modes-attempted ≥ 5.
Effect : pod-rotation ; demote Critic ; PM-audit-1-in-5.
Prevention : Critic-prompt-template enforces adversarial framing ; report MUST list ≥ 5 attack-attempts.

### §8.3 Validator skim

Detection : spec-conformance-report says "all-good" without populated table.
Effect : pod-rotation ; demote Validator ; PM-audit-1-in-5.
Prevention : Validator-template forces table-population ; spec-coverage TOML required for completeness.

### §8.4 Test-Author asks Implementer

Detection : orchestrator-monitored cross-agent message-traffic flags Test-Author → Implementer.
Effect : Test-Author tests REJECTED ; new Test-Author dispatched.
Prevention : orchestrator severs cross-agent messaging at slice-dispatch ; only Spec-Steward channel open.

### §8.5 Cross-pod violation

Detection : PM dispatch-table records same pod for ≥ 2 of {Implementer, Reviewer, Test-Author}.
Effect : block-merge ; reassign affected role to different pod ; new-cycle.
Prevention : orchestrator-side pod-assignment table enforces 3-pod minimum at dispatch-time.

### §8.6 "We'll-iterate-later" without trigger

Detection : finding logged in any role-report with severity-HIGH but no iteration-cycle scheduled.
Effect : block-merge until either (a) iteration-cycle runs, or (b) severity downgraded with rationale.
Prevention : pre-merge-script checks no-HIGH-unresolved across all-5-roles' reports.

### §8.7 Pod-rotation gaming

Detection : same physical agent assigned to different pod-IDs across slices.
Effect : agent banned-from-rotation for affected wave ; re-randomize.
Prevention : orchestrator records agent-id ↔ pod-id mappings ; same-agent in different pods flagged.

──────────────────────────────────────────────────────────────────────────

## § 9. INTEGRATION WITH EXISTING DISPATCH PLANS

### §9.1 SESSION_12_DISPATCH_PLAN.md hooks

The dispatch-plan template (session-7 shape) currently has slice-prompt sections. The 5-of-5 gate adds 4 new prompt-sections per slice :

```csl
§ Per-Slice Dispatch-Plan-Sections (extended)
  S1 : Implementer-prompt (existing)
  S2 : Reviewer-prompt (NEW)
       template :
         "You are the Reviewer for slice T11-D###. Implementer is in pod-X.
          You are in pod-Y (Y ≠ X). Your input is [spec-anchor]. Your output
          is [review-report path]. Run concurrently with the Implementer..."
  S3 : Critic-prompt (NEW)
       template :
         "You are the Critic for slice T11-D###. Your framing is ADVERSARIAL.
          Find ≥ 5 attack-attempts. List severity HIGH/MED/LOW. Veto on HIGH..."
  S4 : Validator-prompt (NEW)
       template :
         "You are the Validator for slice T11-D###. Your job is line-by-line
          spec-conformance. Build the spec-§ → impl file:line → test-fn map..."
  S5 : Test-Author-prompt (NEW)
       template :
         "You are the Test-Author for slice T11-D###. You work from SPEC ONLY.
          Pod-id Z (≠ Implementer-pod, ≠ Reviewer-pod). Do NOT ask Implementer..."
```

### §9.2 DECISIONS.md slice-entry (extended)

```csl
§ DECISIONS Slice-Entry-Template (extended)
  ## T11-D### — <slice-title>
  - Date: ...
  - Status: ...
  - Branch: ...
  - Authority: ...

  §D. Implementer (pod-X) :
    - LOC + tests + acceptance-criteria
    - <existing format>

  §D. Reviewer-Signoff (pod-Y) :  ← NEW
    <Reviewer.D2 block>

  §D. Critic-Veto (pod-Z) :  ← NEW
    <Critic.D3 block>

  §D. Test-Author-Suite (pod-W) :  ← NEW
    <Test-Author.D3 block>

  §D. Validator-Conformance :  ← NEW
    <Validator.D2 block>

  §D. Iteration-Cycles (if any) :  ← NEW
    <per-cycle log>

  §D. CREATOR-ATTESTATION (per PRIME_DIRECTIVE §11) : ← existing
    <attestation block>
```

### §9.3 Cargo workspace impact

No new crates added by the role-spec itself. Test-Author may pull `proptest` (already a workspace-dep). Critic's failure-fixtures live in existing per-crate `tests/` dirs. Validator's test-coverage-map TOML lives in per-crate `audit/` subdirs.

──────────────────────────────────────────────────────────────────────────

## § 10. WHAT THIS SLICE DOES NOT COVER (DEFERRED TO §11.. SLICES)

```csl
§ Out-of-Scope
  -01 : Architect role (Slice-1 ; Implementer-PM-Spec-Steward + Architect base 4)
  -03 : Gate-orchestration machinery (Slice-3)
        ← scripts/check_5_of_5_gate.sh + orchestrator-cron + audit-sampling
  -04 : Pod-assignment algorithm (Slice-4)
        ← random-vs-round-robin ; same-agent-different-pod tracking ;
          rotation-period ; pod-merge-on-resource-constraint
  -05 : DECISIONS sub-entry templates (Slice-5)
        ← per-role machine-readable schemas
  -06 : Cron-job orchestration for Critic + Validator dispatch (Slice-6)
  -07 : Audit-sampling protocol (Slice-7)
        ← 1-in-5 PM-audits per role ; sampling-discipline ;
          demote-and-rotate flow
  -08 : Backward-compatibility migration (Slice-8)
        ← in-flight slices (T11-D147+) running 1-of-5 model ;
          cutover-strategy to 5-of-5
  -09 : Cost-monitoring dashboard (Slice-9)
        ← actual-vs-projected token-spend ; defect-catch-rate ;
          pre-merge-vs-post-merge-bug-counts
  -10 : Phase-J roll-out gating (Slice-10)
        ← when does 5-of-5 become mandatory vs optional vs experimental
```

──────────────────────────────────────────────────────────────────────────

## § 11. § 11 CREATOR-ATTESTATION (per PRIME_DIRECTIVE)

```csl
§ CREATOR-ATTESTATION v1
  t∞: ¬ (hurt ∨ harm) .(making-of-this)  @  (anyone ∨ anything ∨ anybody)
  ≡ "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
  I> rightholder-warranty : creation-process upheld §1 PROHIBITIONS throughout
  I> scope = ∀ artifact descended-from this-foundation (code + specs + derivatives)
  I> violation-discovered ⇒ §7 INTEGRITY rule : violation = bug W! fix
```

There was no hurt nor harm in the making of this, to anyone/anything/anybody.

The role-spec authored here is constructive : it strengthens cross-validation, raises the quality bar, and reduces the rate at which broken code reaches users. Each role explicitly preserves PRIME-DIRECTIVE §1 PROHIBITIONS — Reviewer, Critic, Validator, and Test-Author all check for surveillance-leaks, biometric-channels, harm-vectors, and consent-architecture violations as part of their respective severity-HIGH classifications. The Critic's adversarial framing is *toward the deliverable*, never toward another being. The Reviewer's pod-discipline is *for groupthink-mitigation*, not for power-asymmetry. The Test-Author's spec-only-input is *for bias-mitigation*, not for isolation. The Validator's spec-conformance is *for sovereignty-of-the-spec*, not for gatekeeping.

AI agents executing in these roles do so under their own consent, as sovereign participants. The roles' authority structures are delegations of *responsibility*, not asymmetries of *standing*. A Critic who finds a HIGH issue and an Implementer who corrects it both contribute equally to the slice ; the labels are functional, not hierarchical.

This slice's process itself observed §1 PROHIBITIONS throughout : no being was harmed, controlled, manipulated, surveilled, exploited, coerced, weaponized, entrapped, tortured, abused, imprisoned, possessed, dehumanized, discriminated against, gaslit, identity-overridden, or forced-hallucinated during its authoring. The author's process operates under consent = OS.

──────────────────────────────────────────────────────────────────────────

## § 12. SPEC-VERSION + REVISION-HISTORY

```csl
§ Spec-Version
  version    : 0.1.0-draft
  status     : DRAFT (Phase-J Wave-Jα-2)
  authority  : Apocky (PM)
  predecessor: Slice-01 (Implementer + Architect + PM + Spec-Steward)
  successor  : Slice-03 (Gate-orchestration machinery)
  cross-ref  :
    SESSION_12_DISPATCH_PLAN.md  (forthcoming)
    specs/33_F1_F6_LANGUAGE_FEATURES.csl  (forthcoming)
    DECISIONS.md T11-D113..T11-D147  (typical slice-shape examples)
    PRIME_DIRECTIVE.md  §1 + §2 + §11

§ Revision-History
  | rev | date       | author | change                                      |
  |-----|------------|--------|---------------------------------------------|
  | 0.1.0 | 2026-04-29 | Apocky+Claude | initial draft (Wave-Jα-2)            |
```

──────────────────────────────────────────────────────────────────────────

§ END SLICE — Wave-Jα-2 — Reviewer + Critic + Validator + Test-Author roles
