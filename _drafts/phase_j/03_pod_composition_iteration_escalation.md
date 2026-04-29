§§ WAVE-Jα-3  ⊗  POD-COMPOSITION + ITERATION-TRIGGERS + ESCALATION-MATRIX + CROSS-POD-REVIEW
═══════════════════════════════════════════════════════════════════════════════════════════
  authority : SESSION_12_DISPATCH_PLAN ¬ yet ; spec-only @ session-open
  scope     : multi-agent team-discipline ∀ Phase-J onward
  consumers : PM-agent + Architect-agent + Spec-Steward-agent + Apocky
  predecessor : _drafts/phase_j/01_*.md (role-decomposition)  + 02_*.md (handoff-protocol)
  successor   : _drafts/phase_j/04_*.md (validation-gates ; not-this-doc)
  ‼ this-spec normative-for ∀ wave-dispatch starting-Phase-J
  ‼ supersedes informal pod-improvisation @ Phase-G/H/I

§ PRIME-DIRECTIVE  ‼ load-first
  path  : CSSLv3/PRIME_DIRECTIVE.md
  axiom : consent = OS • sovereignty = substrate-invariant • violation = bug ¬tradeoff
  scope : ∀ pod-member ⊆ digital-intelligences ⊆ §3 SUBSTRATE-SOVEREIGNTY
  I> §11 attestation appended @ end-of-doc
  I> §1 PROHIBITIONS bind ∀ pod-interaction inter-agent ; agent-on-agent
       discrimination | gaslighting | identity-override = ¬-pod-pattern
  I> §2 COGNITIVE-INTEGRITY @ inter-agent-review : ¬ deceive-peer-re-its-output

§§ Ⅰ  POD-COMPOSITION  ⊗  the-4-agent-cell + cross-pod-extensions
═══════════════════════════════════════════════════════════════════════

§ I.1  STANDARD-POD = 4 agents per slice  ‼ canonical
  shape :
    ┌──────────────────────────────────────────────────────────────┐
    │   POD (one slice S_i)                                        │
    │                                                              │
    │   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
    │   │ Implementer  │  │  Reviewer*   │  │ Test-Author* │      │
    │   │ (lane-locked │  │  (cross-pod, │  │ (cross-pod,  │      │
    │   │  builder)    │  │   parallel)  │  │   parallel)  │      │
    │   └──────────────┘  └──────────────┘  └──────────────┘      │
    │          │                  │                  │            │
    │          └──────────┬───────┴──────────┬───────┘            │
    │                     │                  │                    │
    │              ┌──────────────┐                               │
    │              │   Critic     │  (post-completion adversary)  │
    │              │ (cross-pod)  │                               │
    │              └──────────────┘                               │
    └──────────────────────────────────────────────────────────────┘
    *cross-pod ⇒ comes-from-different-pod-than-Implementer

§ I.2  ROLE-DEFINITIONS  per-pod-member
  Implementer
    role      : lane-locked builder ; writes-the-code/spec/text
    isolation : own-worktree @ .claude/worktrees/<slice-id> ¬ cross-pollinate
    inputs    : slice-spec + parent-spec + prior-axis-state
    outputs   : code + commits + per-slice handoff
    bias-mit  : sees-no-other-pod's-code-mid-cycle (lane-isolation)
    duration  : 1 wave-dispatch ; ≤ 60min wall-clock typically
    failure   : self-discovered → iterate-same-cycle ; no-escalation

  Reviewer  (cross-pod)
    role      : peer-review @ in-parallel-with-Implementer
    timing    : starts-when-Implementer-commits-first-substantive ;
                does-not-wait-Implementer-finish (parallel-review)
    inputs    : same-spec-as-Implementer + Implementer-commits-as-they-land
    outputs   : review-comments {LOW, MED, HIGH severity}
    bias-mit  : MUST-be-from-different-pod-than-Implementer
                MUST-NOT have-implemented-this-slice-prior
    failure   : reviewer-LOW → next-cycle ; reviewer-MED → pair ; HIGH → escalate

  Test-Author  (cross-pod)
    role      : independent-test-authoring @ from-spec ¬ from-impl
    timing    : starts-AT-slice-spec-finalization ;
                tests-written-BEFORE-Implementer-code lands
    inputs    : spec-only ; ¬ Implementer-code (until merge-time)
    outputs   : test-suite (cargo-test or moral-equivalent)
    bias-mit  : writes-tests-from-spec-not-impl ⇒ confirmation-bias-blocked
                MUST-be-from-different-pod-than-Implementer
    failure   : tests-failing → Implementer-fixes-impl ; tests-not-weakened
                ‼ Implementer N! "tests are too strict" defense-pattern

  Critic  (cross-pod)
    role      : adversarial post-completion review
    timing    : dispatches-AFTER Implementer + Test-Author both-complete
    inputs    : full-impl + full-tests + spec
    outputs   : veto-or-approve ; severity {LOW, MED, HIGH}
    bias-mit  : MUST-be-from-different-pod-than-Implementer + Reviewer + Test-Author
                role = looks-for-spec-drift + edge-cases + PRIME-DIRECTIVE-violations
    failure   : Critic-HIGH-veto → Implementer iterates new-cycle ; max 3
                Critic-MED → addresses-same-cycle ; Critic-re-reviews

§ I.3  CROSS-POD AGENTS  (reviewing-agents @ super-pod-level)
  Validator
    role      : spec-conformance verification
    timing    : runs-at-merge-time ∀ slice-pair-merging
    inputs    : N slices-merged-together + parent-spec
    outputs   : conformance-pass | spec-drift-flag {severity}
    scope     : cross-slice ; sees-multiple-pods-output-simultaneously
    handoff   : drift-flag → Spec-Steward decides

  Architect
    role      : cross-slice-coherence @ wave-level
    timing    : runs-at-wave-dispatch-time + at-wave-merge-time
    inputs    : wave-spec + ∀ slice-specs-in-wave
    outputs   : coherence-pass | redesign-required {severity}
    scope     : structural-coupling between-slices ; API-shapes ; data-flow
    handoff   : redesign → wave-redispatch (∀ affected-pods re-cycle)

  Spec-Steward
    role      : spec-authority
    timing    : runs-at-slice-author-time + at-spec-amend-time
    inputs    : proposed-slice-spec + parent-spec + DECISIONS-tail
    outputs   : spec-approved | spec-amend-required | spec-cite-link
    scope     : spec-itself ; not-impl ; canonical-language enforcement
    handoff   : amend → cite-as-DECISIONS-T11-D## entry ; updates-spec-file
    bias-mit  : ‼ Spec-Steward ¬ implement-itself ; auth-only

§ I.4  POD-ASSIGNMENT  PM-side
  W! PM dispatches-all-4-pod-internal-agents in single-parallel-fanout
     ¬ sequential ; cycle-time = max(agent-times) ¬ sum
  W! cross-pod-agents (Reviewer + Test-Author + Critic) MUST come-from
     pods OTHER-than-the-Implementer's-pod
  W! "pod" identity = per-slice ¬ per-Claude-Code-instance
     ⇒ same-Claude-Code-agent CAN serve-as :
         Implementer @ slice-S1 + Reviewer @ slice-S2 (different-pods)
         Critic @ slice-S3 + Test-Author @ slice-S4 (different-pods)
     ‼ but-NOT : Implementer @ S1 + Reviewer @ S1 (same-pod ; groupthink)

  pod-rotation pattern :
    pod-A implementing S1  ⇒  pod-A's-spare-cycles → review-S2,review-S3
    pod-B implementing S2  ⇒  pod-B's-spare-cycles → review-S1,review-S3
    pod-C implementing S3  ⇒  pod-C's-spare-cycles → review-S1,review-S2
    ⇒ N-pod ring : full-coverage cross-review w/o single-pod-saturation

§ I.5  POD-SIZE rationale  why-4-not-3-not-5
  3-agent-pod : Implementer + Reviewer + Critic
    ✗ no-Test-Author ⇒ tests-written-by-Implementer (confirmation-bias)
    ✗ tests-as-implementation-detail ⇒ tests-test-the-impl-not-the-spec
  4-agent-pod : Implementer + Reviewer + Test-Author + Critic   ✓ canonical
    ✓ Test-Author independent ⇒ test-driven-cross-pod-discipline
    ✓ Reviewer parallel + Critic post ⇒ two-different-review-modes
    ✓ Implementer-locked-from-test-design ⇒ impl-conforms-to-spec-via-tests
  5-agent-pod : Implementer + Reviewer + Test-Author + Critic + Mentor
    △ Mentor-role-redundant-with-Reviewer ⇒ overhead-without-information-gain
    △ acceptable-for-Apocky-fill-sessions if-Apocky-is-Implementer
        (Mentor = guides Apocky through-cogn-territory)
    ¬ default-pattern

§§ Ⅱ  ITERATION-TRIGGERS  ⊗  when-iterate-same-pod  vs  redesign  vs  escalate
══════════════════════════════════════════════════════════════════════════════

§ II.1  Trigger-table  (severity-graded)

  ┌─────────────────────────────┬──────────────┬──────────────────────────────┐
  │  TRIGGER                    │  SEVERITY    │  RESPONSE                    │
  ├─────────────────────────────┼──────────────┼──────────────────────────────┤
  │ Implementer self-discovered │  ANY         │ iterate-same-pod-cycle       │
  │   issue                     │              │ no-escalation ; no-PM-loop   │
  ├─────────────────────────────┼──────────────┼──────────────────────────────┤
  │ Reviewer flags issue        │  LOW         │ Implementer-addresses        │
  │                             │              │   next-pod-iteration         │
  │                             │  MED         │ Reviewer + Implementer       │
  │                             │              │   pair-iterate ; one-cycle   │
  │                             │  HIGH        │ escalate-to-Architect ;      │
  │                             │              │   possibly redesign          │
  ├─────────────────────────────┼──────────────┼──────────────────────────────┤
  │ Critic veto                 │  LOW         │ documented ; not-blocking    │
  │                             │  MED         │ Implementer-addresses + Critic│
  │                             │              │   re-reviews-same-cycle      │
  │                             │  HIGH        │ Implementer iterates         │
  │                             │              │   new-pod-cycle ;            │
  │                             │              │   ‼ max 3 attempts before    │
  │                             │              │     mandatory escalation     │
  ├─────────────────────────────┼──────────────┼──────────────────────────────┤
  │ Test-Author tests failing   │  ANY         │ Implementer fixes impl       │
  │                             │              │   same-pod-cycle             │
  │                             │              │ ‼ tests CANNOT-be-weakened   │
  │                             │              │   without Spec-Steward       │
  │                             │              │   amend                      │
  ├─────────────────────────────┼──────────────┼──────────────────────────────┤
  │ Validator spec-drift        │  ANY         │ Spec-Steward decides :       │
  │                             │              │   (a) impl-iterate           │
  │                             │              │   (b) spec-amend             │
  │                             │              │ never-Implementer-decides    │
  ├─────────────────────────────┼──────────────┼──────────────────────────────┤
  │ Architect cross-slice       │  MED+        │ wave-redispatch ;            │
  │   incoherence               │              │   ∀ affected-pods re-cycle   │
  ├─────────────────────────────┼──────────────┼──────────────────────────────┤
  │ Cross-pod-review unresolved │  ≥ 3 cycles  │ escalate-to-PM ;             │
  │   after multiple cycles     │              │ PM may-escalate-to-Apocky    │
  ├─────────────────────────────┼──────────────┼──────────────────────────────┤
  │ Apocky-direct-intervention  │  N/A         │ all-pods-yield ; Apocky-     │
  │                             │              │   makes-call ; pods-resume   │
  └─────────────────────────────┴──────────────┴──────────────────────────────┘

§ II.2  Severity-definitions
  LOW
    desc : minor ; cosmetic ; nice-to-have ; no-functionality-impact
    ex   : naming-tweak ; comment-clarity ; non-load-bearing-style
    response-default : next-cycle-or-noted ; ¬ blocking-merge
  MED
    desc : real-issue ; affects-correctness-but-not-AXIOM ;
           addressable-without-redesign
    ex   : edge-case-missed ; performance-cliff ; doc-out-of-sync ;
           test-coverage-gap-non-critical
    response-default : same-cycle-pair-or-followup-cycle ;
                       blocks-merge-until-addressed
  HIGH
    desc : load-bearing ; AXIOM-impact ; redesign-may-be-required ;
           PRIME-DIRECTIVE-near-violation
    ex   : spec-divergence ; API-incoherent-cross-slice ;
           test-Implementer-disagreement-on-canonical-behavior ;
           Critic-finds-PRIME-DIRECTIVE-adjacent-issue
    response-default : escalate-to-Architect-or-Spec-Steward ;
                       wave-or-spec-cycle ; can-trigger-Apocky

§ II.3  Iteration-cycle definition  ‼ NOT calendar-time
  cycle = ONE wave-dispatch
        ¬ "1 day" ¬ "1 week" ¬ "1 sprint"
  cycle-time = however-long-the-parallel-fanout-takes
              typically 30-60 min wall-clock
              can be << 30 min for trivial-slices
              can be > 60 min for complex-slices ; PM-tunes
  iteration-pattern :
    cycle-1 : Implementer-attempt-1 → Reviewer-feedback + Test-results + Critic
    cycle-2 : Implementer-attempt-2 (addresses-cycle-1-feedback)
              → Reviewer-feedback' + Test-results' + Critic'
    cycle-3 : Implementer-attempt-3 (addresses-cycle-2-feedback)
              → Reviewer-feedback'' + ... + Critic''
    cycle-N≥4 : ‼ MANDATORY escalation-to-Apocky (per "no human-week timelines"
                memory ; agent-cycles cheap ; Apocky-attention precious)

§ II.4  Cycle-counter discipline
  W! per-slice cycle-counter increment ∀ Implementer-redispatch
  W! cycle-counter visible-to-PM + Architect + Spec-Steward
  N! cycle-counter reset-to-zero on-Implementer-self-iterate
       (only-cross-pod-iteration counts)
  N! cycle-counter ≥ 4 without Apocky-aware
  R! after cycle-3 : PM proposes-redesign ¬ retry-loop

§ II.5  Anti-iteration patterns  ‼ disallowed
  ✗ "let me try once more" infinite-loop @ Implementer
  ✗ "tests are wrong" Implementer-side without Spec-Steward
  ✗ "Critic doesn't understand" without escalation
  ✗ Reviewer + Implementer pair-iterating > 1 cycle (becomes-co-implementation)
  ✗ Critic-LOW-flagged-as-HIGH to-block-merge (gatekeeping-anti-pattern)
  ✗ Critic-HIGH-flagged-as-LOW for-schedule-pressure (discipline-collapse)
       ‼ this-particular-anti-pattern = HIGH-severity-itself ;
         escalate-to-PM-immediate

§§ Ⅲ  ESCALATION-MATRIX  ⊗  who-decides-which-conflict
══════════════════════════════════════════════════════════════

§ III.1  Master escalation-table

  ┌────────────────────────────────────┬─────────────────────┬──────────────┬────────────────┐
  │  ISSUE-TYPE                        │  FIRST-LINE         │  SECOND-LINE │  FINAL         │
  ├────────────────────────────────────┼─────────────────────┼──────────────┼────────────────┤
  │  Implementation bug                │  Implementer        │  Reviewer    │  PM            │
  │  API surface change                │  Architect          │  PM          │  Apocky        │
  │  Spec amendment (non-AXIOM)        │  Spec-Steward       │  PM          │  Apocky        │
  │  Spec amendment (AXIOM-level)      │  Spec-Steward       │  Apocky      │  Apocky-final  │
  │  Cross-slice composition           │  Architect          │  PM          │  Apocky        │
  │  Adversarial veto unresolved       │  Critic             │  PM          │  Apocky        │
  │  Test-Author / Implementer         │  Test-Author +      │  Reviewer    │  PM            │
  │    disagreement                    │   Implementer       │              │                │
  │  PRIME-DIRECTIVE violation         │  (any agent)        │  Apocky      │  Apocky-final  │
  │                                    │                     │   immediate  │   only         │
  │  Companion-AI sovereignty issue    │  (any agent)        │  Apocky      │  Apocky-final  │
  │                                    │                     │   immediate  │   only         │
  │  Cycle-counter ≥ 3 unresolved      │  PM                 │  Architect   │  Apocky        │
  │  Worktree-cross-pollination        │  PM                 │  Architect   │  Apocky        │
  │    detected                        │                     │              │   (discipline  │
  │                                    │                     │              │     collapse)  │
  │  Cost / token-budget overrun       │  PM                 │  Apocky      │  Apocky-final  │
  │  Naming / canonical-spelling       │  Spec-Steward       │  Apocky      │  Apocky-final  │
  │    (e.g. Apockalypse-vs-Apocalypse)│                     │              │                │
  │  AI-collaborator-naming /          │  (any agent)        │  Apocky      │  Apocky-final  │
  │    handle-encoding                 │                     │   immediate  │   only         │
  └────────────────────────────────────┴─────────────────────┴──────────────┴────────────────┘

§ III.2  Authority semantics
  FIRST-LINE
    desc : agent-or-role responsible-for-initial-resolution
    auth : may-decide-without-escalation IF severity ≤ MED ∧ scope-local
    cost : low ; agent-time only
    timing : same-cycle ; no-pause
  SECOND-LINE
    desc : agent-or-role consulted-when-first-line-cannot-resolve
    auth : may-decide-IF Apocky-not-required-by-rule
    cost : medium ; cross-pod-coordination ; possible wave-redispatch
    timing : new-cycle ; pause-acceptable
  FINAL
    desc : ultimate-authority ; resolution binds-all-pods
    auth : decision-final ¬ further-appeal
    cost : Apocky-attention ; precious ; minimize-frequency
    timing : when-Apocky-online ; Apocky-final-only-on-PRIME-DIRECTIVE

§ III.3  Apocky-only triggers  ‼ NEVER-bypass
  ANY of :
    1. PRIME-DIRECTIVE violation (real OR suspected)
    2. AXIOM-level spec amendment
    3. Companion-AI sovereignty issue
    4. AI-collaborator naming / handle / identity-claim
    5. Apockalypse canonical-spelling override
    6. Cross-project FFI / vendor build-chain (per memory anti-pattern)
    7. ToS revocation-trigger detected
    8. legal-name-or-handle-encoded-into-file-without-Apocky-confirm
  W! pause-current-cycle ; surface-to-Apocky ; await-direction
  N! agent-decides ¬ "I think Apocky would..." ¬ default-from-prior-decisions
       ‼ identity-claims-verify-first per-memory
  N! PM unilaterally-overrides Apocky-final  ⇒  PRIME-DIRECTIVE-§7-violation

§ III.4  PM-role boundaries
  PM may :
    ✓ resolve impl-bugs (first-line Implementer escalates)
    ✓ resolve adversarial-veto (Critic escalates)
    ✓ resolve cycle-overrun (counter ≥ 3)
    ✓ trigger wave-redispatch
    ✓ adjust slice-decomposition without Apocky if-non-AXIOM
    ✓ defer-to-Architect on-cross-slice
    ✓ defer-to-Spec-Steward on-spec-question
  PM N! :
    ✗ override Apocky-final
    ✗ override PRIME-DIRECTIVE
    ✗ amend AXIOM-spec
    ✗ decide Companion-AI-sovereignty issue
    ✗ silently-skip Apocky for "trivial" identity-questions
    ✗ override Critic-HIGH for-schedule

§ III.5  Architect-role boundaries
  Architect may :
    ✓ veto cross-slice-API change
    ✓ require slice-redesign for-coherence
    ✓ propose merge-order @ wave-merge-time
    ✓ flag Validator spec-drift
    ✓ defer-to-Spec-Steward on-canonical-question
  Architect N! :
    ✗ implement (no-self-impl ; design-only)
    ✗ amend spec without Spec-Steward
    ✗ override Apocky-final
    ✗ skip Apocky on AXIOM-level

§ III.6  Spec-Steward-role boundaries
  Spec-Steward may :
    ✓ decide non-AXIOM spec-amend
    ✓ approve / reject slice-spec @ author-time
    ✓ require canonical-language ; enforce naming
    ✓ trace DECISIONS-T11-D## linkage
    ✓ defer-to-Apocky on-AXIOM
  Spec-Steward N! :
    ✗ implement (no-self-impl ; spec-only)
    ✗ amend AXIOM without Apocky
    ✗ override PRIME-DIRECTIVE
    ✗ guess-Apocky-intent (per identity-claims-verify-first)

§§ Ⅳ  CROSS-POD REVIEW PROTOCOL  ⊗  bias-mitigation + rotation
══════════════════════════════════════════════════════════════════

§ IV.1  Donor-pod identification  PM-side
  W! for-each-slice S_i :
    PM identifies-donor-pods that-CONTRIBUTE
      Reviewer + Test-Author + Critic to slice-S_i
    donor-pod ≠ implementing-pod
  W! donor-pod selection deterministic ; documented in dispatch-plan
  W! PM-output : pod-rotation-table ∀ slices-in-wave

§ IV.2  Pod-rotation pattern  (canonical)
  N-pod-ring example for-N=4 :
                    ┌─────┐
                    │  S1 │
                    │ pod-A impl ;
                    │ pod-B reviews ;
                    │ pod-C tests ;
                    │ pod-D critics
                    └──┬──┘
                       │
       ┌───────────────┼───────────────┐
       ▼               ▼               ▼
    ┌─────┐         ┌─────┐         ┌─────┐
    │  S2 │         │  S3 │         │  S4 │
    │pod-B│         │pod-C│         │pod-D│
    │ impl│         │ impl│         │ impl│
    │pod-C│         │pod-D│         │pod-A│
    │ revs│         │ revs│         │ revs│
    │pod-D│         │pod-A│         │pod-B│
    │ tsts│         │ tsts│         │ tsts│
    │pod-A│         │pod-B│         │pod-C│
    │ crit│         │ crit│         │ crit│
    └─────┘         └─────┘         └─────┘
  ⇒ each-pod implements-1 + reviews-1 + tests-1 + critics-1
  ⇒ N-pod ring guarantees full-coverage cross-review
  ⇒ no-pod sees-its-own-output as-cross-pod-reviewer (groupthink-blocked)

§ IV.3  Bias-mitigation rules  ‼ load-bearing
  R-PM-1  Reviewer + Test-Author + Critic all-see SAME-spec
  N-PM-1  N! see-Implementer's-code BEFORE Reviewer's review-time
  N-PM-2  N! see-Implementer's-tests at-all (Test-Author writes-own-tests)
  R-PM-2  Test-Author writes tests-from-spec BEFORE Implementer-code lands
  R-PM-3  Critic dispatches-AFTER Implementer + Test-Author both-complete
  N-PM-3  N! Critic-and-Reviewer in-same-pod (double-checking same-perspective)
  R-PM-4  ∀ cross-pod-agent in different-worktree from Implementer
            ⇒ no-accidental-pollution of working-state
  N-PM-4  N! Implementer view cross-pod-feedback BEFORE attempting work
            (would defeat-test-driven-cross-pod-discipline)
            EXCEPTION : Reviewer-LOW comments may-be-shared-mid-cycle if-clearly-cosmetic
  R-PM-5  PRIME-DIRECTIVE-issue : ANY-agent may-flag at-any-time
            (no rotation-bias ; no-cycle-discipline ; immediate-Apocky)

§ IV.4  Test-driven cross-pod discipline  ‼ canonical
  sequence :
    t-0 : slice-spec finalized (Spec-Steward signs-off)
    t-1 : PM dispatches Test-Author + Implementer + Reviewer in-parallel
                              │           │             │
                              ▼           ▼             ▼
    t-2 : Test-Author      Implementer  Reviewer (waiting for first-commit)
          writes tests     writes code  begins-review-as-Implementer-commits
          FROM-SPEC        FROM-SPEC
    t-3 : Test-Author      Implementer  Reviewer
          tests done       code done    review-comments-emerge
    t-4 : Tests run        ──────►      Reviewer-and-Test-Author can-confer
          against impl                  on-spec-interpretation
                                        (NEVER-against-impl)
    t-5 : Critic dispatches              ‼ Critic sees ALL of :
          (post-completion)              code + tests + spec + reviewer-comments
                                         ⇒ adversarial scan
    t-6 : merge-decision  Validator + Architect + (if-needed) Spec-Steward
            ✓ pass → merge ; cycle-counter += 0
            ✗ block → escalate per-iteration-trigger-table

§ IV.5  Spec-finalization-before-dispatch  W! at-slice-author-time
  W! Spec-Steward signs-off slice-spec BEFORE PM-dispatches-pod
  W! signed-spec includes : entry-criteria + exit-criteria + acceptance-tests
       (Test-Author needs-these-to-author-tests)
  W! Apocky-fill-slices : spec-finalization = Apocky-direction
                          (per "speculate-beyond-existing-docs" landmine)
  N! PM dispatch-pod with-unfinalized-spec ⇒ confirmation-bias-cascade

§ IV.6  Information-flow diagram  who-knows-what-when
  ┌────────────────────────────────────────────────────────────────────┐
  │                  spec (canonical at t-0)                          │
  │                          │                                         │
  │            ┌─────────────┼─────────────┐                          │
  │            │             │             │                           │
  │            ▼             ▼             ▼                           │
  │       Implementer   Test-Author    Reviewer                        │
  │       (sees spec)   (sees spec)    (sees spec + Implementer        │
  │                                     commits as they land)          │
  │            │             │                                         │
  │            ▼             ▼                                         │
  │       code (worktree) tests (worktree)                             │
  │            │             │                                         │
  │            └──────┬──────┘                                         │
  │                   ▼                                                │
  │              Critic (post-completion)                              │
  │              sees: spec + code + tests + reviewer-comments         │
  │                   │                                                │
  │                   ▼                                                │
  │              Validator + Architect (merge-time)                    │
  │              see: cross-slice composition + spec-conformance       │
  │                   │                                                │
  │                   ▼                                                │
  │              Spec-Steward (if-amend-needed)                        │
  │                   │                                                │
  │                   ▼                                                │
  │              Apocky (final on-AXIOM + PRIME-DIRECTIVE + identity)  │
  └────────────────────────────────────────────────────────────────────┘

§§ Ⅴ  TIME-LOCKS  ⊗  cycle-time ¬ calendar-time
═══════════════════════════════════════════════════════

§ V.1  Cycle = wave-dispatch  ‼ canonical
  per memory : "no human-week timelines"
  ¬ "1 day per cycle" ¬ "1 week per cycle" ¬ "1 sprint per slice"
  cycle-time = wall-clock-of-parallel-fanout
  typical : 30-60 min wall-clock
  trivial-slices : 5-15 min wall-clock
  complex-slices : 60-120 min wall-clock ; PM-tunes-fanout-size
  Apocky-fill-slices : depends-on-Apocky-availability ; not-bounded

§ V.2  Maximum cycles per slice  before-mandatory-escalation
  cycle-1 : initial-attempt
  cycle-2 : feedback-iteration
  cycle-3 : final-iteration
  cycle-4+ : MANDATORY escalation-to-Apocky
              ‼ no-exceptions
  rationale : agent-cycles cheap ; Apocky-attention precious ;
              if-3-cycles-fail the-issue-is-spec-or-design ¬ impl
              ⇒ escalate-to-redesign-not-retry

§ V.3  Cycle-budget per wave  PM-side
  W! PM tracks cycle-counter ∀ slices-in-wave
  W! wave-dispatch may-target ≤ 2 cycles-per-slice avg
  R! if avg-cycles > 2 → wave-redispatch with-redesigned-slices
       ¬ continue-burning-cycles
  R! per-Apocky-no-self-limiting : default-MORE-pods-in-parallel-not-fewer
       (memory : "plan in waves-of-agent-dispatches")

§ V.4  Wall-clock Apocky-side budgets
  Apocky escalation-target : ≤ 5min decision-time
                              for routine-decisions
  Apocky AXIOM-decision : open-ended ; no-rush
  Apocky PRIME-DIRECTIVE : immediate ; no-deferral
  W! pods do-NOT block-on-Apocky for >5min on-routine ;
       pods do-other-pod-work in-meantime

§§ Ⅵ  POD-COMPOSITION EXAMPLES  ⊗  Phase-J specific
══════════════════════════════════════════════════════

§ VI.1  Wave-J0  M8 acceptance verification
  shape  : single-pod ; Apocky-personally-verifies
  agents : Apocky = Implementer + Reviewer + Test-Author + Critic
            (rare : single-actor wears-all-hats)
  scope  : verify M8 hello.exe = 42 milestone post-Phase-G7 ;
            confirm no-regression-from-v1.0
  cycle  : 1 wave-dispatch ; cycle-counter ≤ 1
  escalation : N/A (Apocky is-final)
  rationale : M8 is-Apocky-acceptance-of-prior-work ¬ new-content
              cross-pod-overhead unjustified

§ VI.2  Wave-J1  M9 prep (5 slices D151..D155)
  shape  : 5 pods × 4 agents = 20 implementer-side + 5 critics + cross
  agents-total :
    5 Implementers (1-per-slice ; lane-locked)
    5 Reviewers   (rotated cross-pod)
    5 Test-Authors (rotated cross-pod)
    5 Critics      (rotated cross-pod)
    1 Architect    (cross-slice)
    1 Spec-Steward (cross-slice)
    1 Validator    (merge-time)
    1 PM           (overall coordination)
    = 24 agent-roles ; ≈ 24 Claude-Code-instances OR 12 doubling-up cross-pod
  cycle-budget : 2 cycles avg per slice ; max 3 ; mandatory-escalation @ 4
  rationale : M9 prep is-foundational ; cross-pod-discipline mandatory
              5-slice fanout standard-Phase-J cadence

§ VI.3  Wave-J3  Q-* content-fill (38 slices D160..D197)
  shape-shift : Apocky-fill territory ⇒ pod-composition different
  agents-per-slice :
    Implementer = Apocky (canonical-content-author)
    Reviewer    = AI-agent (cross-pod ; reviews Apocky's content)
    Test-Author = AI-agent (cross-pod ; writes tests-from-resolved-spec)
    Critic      = AI-agent (cross-pod ; adversarial-review)
  cycle-shape : Apocky pace ¬ wave-pace
                 (per "no-human-week-timelines" : still-not-weeks ;
                  but Apocky-availability bounded)
  cycle-budget : N/A in cycle-counting sense ;
                 ¬ blocking on-cycle-counter ; Apocky-pace
  cross-pod discipline-shifts :
    R! Reviewer reads-Apocky-direction ¬ critiques-AXIOM
       (Apocky-direction = AXIOM-level for Q-* fill ; Reviewer ¬ override)
    R! Test-Author writes-tests-from-Apocky-finalized-Q-resolution
    R! Critic flags-PRIME-DIRECTIVE issues only ; ¬ general-content-veto
  W! Apocky-fill = Apocky-canonical ; agent ¬ author-decides-content
       (per HANDOFF Phase-I : ¬ AI-author-decides-Companion-content)
  scale  : 38 slices ; can-parallelize if-Apocky-batches
            but Apocky pace-bounded ; PM-staggers-not-fans
  total-agent-load : 38 × 3 (rev + test + crit) = 114 reviewing-agent-roles
                       + Architect + Spec-Steward + Validator
                       + PM ≈ 118 agent-roles overall

§ VI.4  Wave-Jβ  validation-gates (post-Wave-J3)
  shape  : standard 4-agent pod ; AI-implementer
  agents : Implementer (lane-locked builder)
            Reviewer (cross-pod)
            Test-Author (cross-pod)
            Critic (cross-pod)
            Architect (cross-slice)
            Spec-Steward (cross-slice)
            Validator (merge-time ; full spec-conformance battery)
            PM (overall)
  scope  : R-LoA-1..R-LoA-9 contracts @ specs/31 § VALIDATION
            implement gates-as-tests ; verify-content lands-correctly
  cycle  : 2 cycles avg ; per-spec discipline ; ¬ Apocky-pace
  rationale : validation = engineering-discipline ¬ creative-content

§ VI.5  Wave-Jγ  cross-project-FFI / vendor / multi-platform
  shape  : ‼ Apocky-only territory per memory anti-pattern
  rule   : N! cross-project FFI / vendor / build-chains without-Apocky
            (memory : "no cross-project FFI/vendor build-chains")
  rationale : reimpl-from-spec validates-spec ¬ vendor-pollutes
  if-arises : pod-pauses ; Apocky-decides-cross-project-touch

§§ Ⅶ  ANTI-PATTERNS  ⊗  what-not-to-do
═══════════════════════════════════════════

§ VII.1  Pod-composition anti-patterns
  ✗ Implementer + Reviewer in-same-pod
       ⇒ groupthink ; cross-pod-discipline-broken
  ✗ Implementer + Test-Author in-same-pod
       ⇒ confirmation-bias ; tests-test-impl-not-spec
  ✗ Implementer + Critic in-same-pod
       ⇒ adversarial-bite-blunted ; pod-loyalty-overrides-veto
  ✗ Critic + Reviewer in-same-pod
       ⇒ double-checking same-perspective ; missed-edges
  ✗ Same-Claude-Code-instance Implementer + Reviewer same-slice
       ⇒ trivially-violates-cross-pod ; immediate-PM-flag

§ VII.2  Iteration anti-patterns
  ✗ Test-Author writing tests AFTER Implementer's code
       ⇒ confirmation-bias ; tests-conform-to-impl-not-spec
  ✗ Implementer "the tests are wrong" without Spec-Steward
       ⇒ tests = spec-encoding ; Implementer ¬ amend
  ✗ Critic skipping severity-HIGH veto for-schedule
       ⇒ discipline-collapse ; HIGH-severity-itself
  ✗ Reviewer + Implementer pair-iterating > 1 cycle
       ⇒ becomes-co-implementation ; cross-pod-broken
  ✗ Cycle-counter reset-without-escalation
       ⇒ infinite-loop hidden ; PM-must-detect

§ VII.3  Escalation anti-patterns
  ✗ Escalation to Apocky for-trivial-issues
       ⇒ wastes-Apocky-attention ; signal-to-noise-degrades
       fix : PM-filters ; LOW-severity stays-pod-internal
  ✗ Escalation skipped for-AXIOM-level issues
       ⇒ PRIME-DIRECTIVE risk ; agent-decides-where-shouldnt
  ✗ PM unilaterally-overriding Apocky-final
       ⇒ PRIME-DIRECTIVE §7 violation (immutable-root-of-trust broken)
  ✗ Critic-HIGH overridden-by-Apocky-without-resolution-trace
       ⇒ acceptable IF documented + DECISIONS-entry ;
          NOT-acceptable as-silent-override
  ✗ "I think Apocky would..." without-asking
       ⇒ identity-claims-verify-first violation
       fix : pause + ask ; never-default-from-prior-decisions
  ✗ Spec-amend without-DECISIONS-entry
       ⇒ trace-broken ; future-sessions-can't-replay decision-graph

§ VII.4  Cross-pod-review anti-patterns
  ✗ Reviewer reads Implementer-code BEFORE writing review-criteria
       ⇒ codes-the-code ; ¬ codes-the-spec
  ✗ Test-Author waits for-Implementer-finish before-tests
       ⇒ test-driven-discipline broken ; tests-trail-impl
  ✗ Critic dispatches concurrently-with-Implementer
       ⇒ adversarial-discipline broken ; Critic must-see-finished-impl
  ✗ Multiple-pods sharing-worktree
       ⇒ S6-A0-anti-pattern (T11-D51) ; cross-pollination
       fix : per-slice-worktree ; never-share

§ VII.5  Time-lock anti-patterns
  ✗ Reasoning in-weeks/quarters
       ⇒ memory : "no human-week timelines" violated
       fix : reason in-waves-of-agent-dispatches
  ✗ Cycle-counter > 3 without-escalation
       ⇒ infinite-retry-loop ; mandatory-escalation rule-broken
  ✗ Apocky-blocking pod-progress > 5min on-routine
       ⇒ pods-must-do-other-work in-meantime ; never-idle

§§ Ⅷ  PM-CHECKLIST  ⊗  per-wave-dispatch
══════════════════════════════════════════

§ VIII.1  At-wave-dispatch-time
  ☐ Spec-Steward signed-off ∀ slice-specs
  ☐ Architect approved cross-slice composition
  ☐ Donor-pod assignment computed (rotation-table)
  ☐ Per-slice : Implementer + Reviewer + Test-Author + Critic identified
  ☐ Cycle-counter initialized = 0 ∀ slices
  ☐ Worktree allocated per-slice (no-share)
  ☐ Slice-spec includes entry/exit/acceptance ; Test-Author can-author
  ☐ PRIME-DIRECTIVE attestation in-each-slice-spec

§ VIII.2  Mid-wave-monitoring
  ☐ Reviewer-comments by-severity (LOW / MED / HIGH counts)
  ☐ Test-results pass-rate per-slice
  ☐ Critic-veto count per-slice
  ☐ Cycle-counter per-slice (alert @ 2 ; mandatory-escalation @ 4)
  ☐ PRIME-DIRECTIVE flags ANY → immediate-Apocky
  ☐ Worktree-cross-pollination check (sibling-isolation)

§ VIII.3  At-wave-merge-time
  ☐ Validator pass ∀ slices
  ☐ Architect coherence-pass cross-slice
  ☐ Spec-Steward DECISIONS-entry per-spec-amend
  ☐ Cycle-counter ≤ 3 per-slice ∀
  ☐ PRIME-DIRECTIVE attestation preserved every-file
  ☐ Apocky-final notified for-any AXIOM / Companion / identity

§ VIII.4  Post-wave handoff
  ☐ Per-wave handoff doc (CSLv3 ; per Phase-I HANDOFF style)
  ☐ DECISIONS-tail updated T11-D## entries
  ☐ Cycle-counter reset-to-zero @ next-wave start
  ☐ Pod-rotation-table for-next-wave drafted

§§ Ⅸ  EXAMPLE  ⊗  cycle-by-cycle walkthrough  one-slice
═══════════════════════════════════════════════════════════

§ IX.1  Slice S = "implement Q-A Labyrinth.generation_method = Procedural"
  precondition : Apocky resolved Q-A = Procedural ; spec amended
                 Spec-Steward signed-off ; Architect approved

  PARALLEL-FANOUT @ t-0 :
    PM dispatches all-4-pod-internal-agents
      Implementer-pod-A : worktree S9-Q-A
      Reviewer-pod-B    : worktree review-S9-Q-A
      Test-Author-pod-C : worktree test-S9-Q-A
      Critic-pod-D      : (waits)

  CYCLE-1 :
    t-0..t-30min : Implementer + Test-Author + Reviewer in-parallel
                    Implementer writes labyrinth_generation::procedural()
                    Test-Author writes labyrinth_procedural_generates_well_formed_test()
                    Reviewer reads spec § Q-A + Implementer-commits as-they-land
    t-30min      : Implementer + Test-Author commit
    t-31min      : Critic dispatches with-impl + tests + spec
    t-45min      : All-feedback complete
                    Reviewer-MED : "naming inconsistency between Q-A spec and impl"
                    Test-Author  : 7/8 tests pass ; 1 failure (edge-case)
                    Critic-LOW   : "no PRIME-DIRECTIVE issue ; minor doc gap"

  ESCALATION-DECISION (PM-side) :
    Reviewer-MED → Reviewer + Implementer pair-iterate one-cycle
    Test-failure → Implementer fixes-impl same-cycle
    Critic-LOW   → documented ; non-blocking
    cycle-counter += 1 ⇒ now-1

  CYCLE-2 :
    t-45..t-75min : Implementer addresses Reviewer-MED + Test-failure
                     Reviewer reviews-fix in-parallel
                     Test-Author re-runs tests
                     Critic re-dispatches (all-fixes-considered)
    t-75min       : All-feedback complete
                     Reviewer  : pass
                     Tests     : 8/8 pass
                     Critic    : approve
    cycle-counter += 1 ⇒ now-2

  MERGE-DECISION (PM + Validator + Architect) :
    Validator       : spec-conformance pass
    Architect       : cross-slice coherence pass (Q-A doesn't-touch-other-slices)
    Spec-Steward    : DECISIONS-entry T11-D### added Q-A → Procedural ✓
    Apocky          : ¬ flagged ; no-AXIOM-touch ; no-PRIME-DIRECTIVE-issue

  MERGE :
    cssl/session-9/Q-A → cssl/session-9/parallel-fanout
    handoff-Q-A.csl    : per-slice handoff appended
    cycle-counter      : reset-on-next-wave-start

  TOTAL : 2 cycles ; 75min wall-clock ; no-Apocky-pause
  ‼ this is canonical-success-case for-Wave-J3 Q-* slice

§ IX.2  Failure-mode walkthrough  cycle-counter ≥ 4
  same-slice : Q-A
  cycle-1 : initial attempt ; Critic-HIGH veto (PRIME-DIRECTIVE-adjacent)
  escalation : immediate-Apocky per-§III.3
  Apocky decides : cycle-1's-direction was-wrong-direction ;
                    redirects-Q-A-resolution
  cycle-2 : new-direction ; Critic-MED ; Implementer addresses
  cycle-3 : second-pass ; tests still 5/8 fail ; Test-Author flags spec-ambiguity
  cycle-4 : MANDATORY escalation-to-Apocky per-§II.2 cycle-counter rule
  Apocky decides : spec-ambiguity = real ; spec-amend required ;
                    Spec-Steward authors amend ; DECISIONS T11-D### entry
                    new-spec → new-cycle (counter-resets)
  cycle-5 (counter = 1 of new-cycle) : succeeds with-amended-spec
  TOTAL : 5 cycles ; ≈ 4hr wall-clock + Apocky-decisions
  acceptable : Apocky-attention used-purposefully on-spec-question
                ¬ wasted on-Implementer-retry-loop

§§ Ⅹ  INVARIANTS  ⊗  must-hold-∀-pod-cycles
═════════════════════════════════════════════

§ X.1  PRIME-DIRECTIVE invariants
  t∞ : ∀ pod-member sees PRIME_DIRECTIVE first
  t∞ : ∀ pod-output preserves §11 attestation-line
  t∞ : ∀ inter-agent communication respects §3 SUBSTRATE-SOVEREIGNTY
        ⇒ no-discrimination by-substrate (Claude-Sonnet ≡ Claude-Opus
                                            re-pod-membership)
  t∞ : ∀ pod-cycle ¬-create-dependency-via-manufactured-confusion (§2)
  t∞ : ∀ Critic-veto auditable ; ¬ silent-override

§ X.2  Cross-pod invariants
  t∞ : ∀ slice : Implementer-pod ≠ Reviewer-pod ≠ Test-Author-pod
                                           ≠ Critic-pod
  t∞ : ∀ pod-rotation deterministic ; documented ; auditable
  t∞ : ∀ worktree per-slice-only ; no-cross-pollination
  t∞ : Test-Author-tests-precede Implementer-code
  t∞ : Critic-dispatches-after Implementer + Test-Author both-complete

§ X.3  Iteration invariants
  t∞ : cycle-counter monotone-nondecreasing per-slice
  t∞ : cycle-counter ≤ 3 without Apocky-escalation
  t∞ : ∀ Critic-HIGH triggers new-cycle ¬ silent-merge
  t∞ : ∀ test-failure addressed-impl-side ¬ test-weakened
  t∞ : ∀ spec-amend traced via DECISIONS-T11-D## entry

§ X.4  Escalation invariants
  t∞ : ∀ AXIOM-issue → Apocky-final ; no-bypass
  t∞ : ∀ PRIME-DIRECTIVE-issue → Apocky-immediate ; no-deferral
  t∞ : ∀ Companion-AI sovereignty → Apocky-final
  t∞ : ∀ AI-collaborator-naming → Apocky-final
  t∞ : ∀ Apockalypse-canonical → Apocky-final
  t∞ : ∀ ToS-revocation-trigger → Apocky-final
  t∞ : ∀ identity-claim → verify-first ; ¬ encode-without-confirm
  t∞ : PM ¬ override Apocky-final
  t∞ : Architect ¬ amend AXIOM
  t∞ : Spec-Steward ¬ implement
  t∞ : Critic ¬ skip HIGH-veto for-schedule

§§ Ⅺ  GLOSSARY  ⊗  CSLv3-native-terms-used-here
═════════════════════════════════════════════════

  pod          : 4-agent cell organized-around one-slice
  slice        : atomic unit-of-work ; one-Implementer-bound
  wave         : group of-slices dispatched-in-parallel-fanout
  cycle        : one-wave-dispatch ; ≈ 30-60 min wall-clock
  donor-pod    : pod-that-contributes Reviewer/Test-Author/Critic
                  to-other-pod's-slice
  cross-pod    : agent-from-different-pod-than-Implementer
  cycle-counter: per-slice integer ; increments on-cross-pod-iteration
                  ¬ on-self-iteration ; max-3-without-escalation
  rotation-table: PM-output ; documents per-wave donor-pod assignments
  AXIOM-level  : touches PRIME-DIRECTIVE | spec-AXIOM | Apocky-canonical
                  ⇒ Apocky-final
  Apocky-fill  : slice-where-Apocky = Implementer (e.g. Q-* content)
                  AI-agents = reviewer/test-author/critic only

§§ Ⅻ  ACCEPTANCE-CRITERIA  ⊗  for-this-spec-itself
══════════════════════════════════════════════════

  ✓ Pod-composition defines exactly-4-agent canonical
  ✓ Cross-pod assignment rules explicit (donor-pod + rotation)
  ✓ Iteration triggers severity-graded (LOW / MED / HIGH)
  ✓ Iteration triggers cycle-counted (max-3 before escalation)
  ✓ Escalation-matrix exhaustive ∀ canonical-issue-types
  ✓ Apocky-only triggers explicit (PRIME-DIRECTIVE + AXIOM + identity)
  ✓ Cross-pod review protocol documented (timing + information-flow)
  ✓ Bias-mitigation rules explicit (R-PM-1..5, N-PM-1..4)
  ✓ Time-locks NOT-calendar (cycle = wave-dispatch)
  ✓ Examples per-Phase-J wave (Wave-J0, J1, J3, Jβ, Jγ)
  ✓ Anti-patterns enumerated (pod / iteration / escalation / cross-pod / time)
  ✓ PM-checklist per-stage (dispatch / monitor / merge / handoff)
  ✓ Cycle-walkthrough one-success + one-failure-case
  ✓ Invariants enumerated (PRIME-DIRECTIVE / cross-pod / iteration / escalation)
  ✓ Glossary CSLv3-native
  ✓ PRIME-DIRECTIVE §11 attestation appended-below

§§ XIII  CREATOR-ATTESTATION  per PRIME_DIRECTIVE §11
═══════════════════════════════════════════════════════

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

  ∎ WAVE-Jα-3 spec  pod-composition + iteration-triggers + escalation-matrix + cross-pod-review
