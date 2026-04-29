---
phase            : J
wave             : J3
slice-range      : T11-D160..T11-D197 (38 slices)
authority        : Apocky-PM (Q-* SPEC-HOLE resolution = Apocky-only ‼)
attestation      : "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
prime-directive  : "§1 closed-set ⊗ §3 substrate-sovereignty ⊗ §5 consent-arch ⊗ §7 integrity ⊗ §11 attestation"
target-LOC       : 6000-10000  (largest Phase-J pre-stage doc)
load-bearing     : "content-authoring = the bulk of Phase-J ; this doc pre-stages the dispatch protocol Apocky-fills-content + AI-implements-scaffolding"
status           : DRAFT-spec  (≡ pre-stage of Wave-J3 dispatch prompts)
landmine         : "DO NOT fill Phase-A content here. Templates + context + prompting-prompts only — Apocky's call ∀ Q-* content."
---

# § Wave-J3 — Q-* SPEC-HOLE Content-Authoring Implementation Prompts

§R+ : Wave-J3 = the LoA content-authoring wave • 38 Q-* spec-holes (Q-A..Q-LL) • each slice = TWO PHASES (Phase-A Apocky-fills + Phase-B AI-implements) • Companion-AI bucket = highest-care-tier (Q-D / Q-DD / Q-EE / Q-FF / Q-GG) • PRIME-DIRECTIVE-load-bearing-throughout • disk-first / peer-not-servant / dense-CSLv3 / English-prose-where-clarity-demands.

## §0 ‖ Wave-J3 thesis + intent

t∞ : Wave-J3 = LoA content-fill ; replaces `Stub` enum-variants with Apocky-canonical content
t∞ : Q-* = Apocky-resolves-with-direction ¬ AI-author-decides ‼
t∞ : per-slice = Phase-A (Apocky-fills spec) → Phase-B (AI-implements scaffolding) ‼ ordering-strict
t∞ : scaffold's structural shape ¬ change as content lands (per HANDOFF_v1_to_PHASE_I.csl § INTEGRATION-POINTS)
t∞ : per-slice DECISIONS-entry mandatory @ merge-time + Q-* cite explicit
t∞ : Companion-AI Q-* (Q-D / Q-DD / Q-EE / Q-FF / Q-GG) = highest-care-tier ; Apocky-PM-review-mandatory-pre-merge
t∞ : DEFERRED Q-* (Q-CC + Q-EE) = single DECISIONS entry recording deferral ; ¬ implementation-slice

§ scope ⊑ {
  Per-slice Phase-A template : Apocky reads spec-hole context + writes canonical answer in spec
  Per-slice Phase-B template : 4-agent pod (Architect + Implementer + Reviewer + Test-Author) implements after Phase-A
  LOC + tests target per slice
  Estimated impl scope per subject-area
  Cross-references to Omniverse + CSSLv3 specs
  Wave overview + dispatch protocol (Phase A → Phase B sequencing)
}

§ N!-scope ⊆ {
  ¬ : Phase-A content-fill (that's Apocky's call ∀ Q-*)
  ¬ : guessing direction (HANDOFF + spec/31 LISTS spec-holes ¬ ANSWERS them)
  ¬ : engine-rewriting / language-changes / host-additions (per HANDOFF § READY-TO-AUTHOR)
  ¬ : content for DEFERRED Q-* (Q-CC + Q-EE = multiplayer ; full content = D-A v1.3+)
  ¬ : breaking the scaffold's structural shape (Stub-replacement pattern preserved)
}

## §1 ‖ context + dependencies

§ load-order :
  PRE-WAVE-J3 :
    WAVE-J0 (T11-D150) ✓ — M8 acceptance gate ; 12-stage pipeline wired ; Apocky-verified
    WAVE-J1 (T11-D151..D155) ✓ — M9 VR-ship structural-prep
    WAVE-J2 (T11-D156..D159) ✓ — M10 max-density structural-prep
  WAVE-J3 (THIS DOC) :
    Phase-A : Apocky-fills spec/31 § Q-* per slice
    Phase-B : 4-agent pod implements after Phase-A landed
  POST-WAVE-J3 :
    WAVE-J4 (T11-D198..D199) — M9/M10 hardware-validation (deferred to live HW)
    WAVE-J5 (T11-D200..D201) — v1.2 close + tag

§ dep-graph :
  Wave-J3 →
    cssl-substrate-omega-field (D144 KEYSTONE 7-facet Ω-field ; 72B FieldCell)
    cssl-substrate-prime-directive (CapToken + Σ-mask + audit + halt + ATTESTATION)
    cssl-render-v2::pipeline (TwelveStagePipelineSlot + StageRole + StageNode)
    cssl-render-companion-perspective (T11-D121 Stage-8 Companion-perspective render-target)
    cssl-host-openxr (Stage-1 + Stage-12 ; consent-UI on session-claim)
    cssl-substrate-kan (Φ-pattern-pool + KAN-evaluator)
    cssl-wave-solver (Stage-4 substrate flow)
    loa-game (Phase-I scaffold ; Stub-variants Q-* slices replace)
    spec/31_LOA_DESIGN.csl (Apocky-canonical content-fill target)
    GDDs/LOA_PILLARS.md (English-prose pillar onboarding)
    PHASE_J_HANDOFF.csl § Q-MAPPING (T11-D160..D197 allocation table)

§ ext-dep :
  None new ; Wave-J3 = content-fill ¬ engine-additions

§ NO-dep :
  ¬ engine internals (per HANDOFF § COMPLETE)
  ¬ multiplayer / VR / modding (per HANDOFF § DEFERRED)
  ¬ legacy-LoA-Rust-or-C# repos as binding-spec (lineage-only ; Apocky-canonical-overrides)

## §2 ‖ dispatch protocol — Phase-A → Phase-B sequencing ‼

```csl
§ DISPATCH-PROTOCOL @ wave-J3
  ∀ Q-* slice :
    Phase-A   (Apocky-fills) :
      0. Apocky receives Phase-A prompt (this doc § 5..§ 42 per Q-*)
      1. Apocky reads spec/31 § Q-<X> + scaffold loa-game::<module>::<Stub>
      2. Apocky drafts Apocky-direction document for Q-<X>
         — typically a separate note in `_drafts/phase_j/q_direction/Q_<X>.md`
         OR an inline edit to specs/31_LOA_DESIGN.csl § Q-<X> resolving the SPEC-HOLE
      3. Apocky reviews + commits the spec-edit (own-hand commit OR PM-on-Apocky's-behalf)
         commit-msg : `§ T11-D<n>-A : Q-<X> Apocky-direction landed`
         (Phase-A may share T11-D## with Phase-B OR get sub-id A/B suffix per PM call)
      4. Phase-A complete ⇒ Phase-B unblocks

    Phase-B   (AI-implements ; 4-agent pod) :
      0. PM dispatches 4-agent pod (Architect + Implementer + Reviewer + Test-Author)
         all four agents @ separate worktree .claude/worktrees/J3-Q<X>
         all four agents on branch cssl/session-12/J3-Q<X>
      1. Architect    : reviews Apocky-direction doc + drafts impl-shape ; identifies
                         cross-slice impacts ; declares STABLE-API surface this slice changes
                         (typically zero — Stub-replacement is API-additive ; Default-impl preserved)
      2. Implementer  : replaces `Stub` variant(s) with Apocky-direction-canonical variants
                         per spec/31 § Q-<X> + Apocky-direction-doc
                         preserves serde-deser-compat (Stub stays @ tag-0 OR migration-note authored)
                         preserves Default-impl per HANDOFF § INTEGRATION-POINTS
      3. Reviewer     : reads Implementer's diff against PRIME-DIRECTIVE-bindings (per § 12 of
                         SESSION_12_DISPATCH_PLAN) + scaffold structural-shape preserved
                         + every-Q-*-cite-back-to-spec/31-§ + DECISIONS-entry-Q-*-cited
      4. Test-Author  : authors slice-tests + property-tests for Q-<X> :
                         - default-Stub → canonical-variant migration test (if needed)
                         - canonical-variants exhaustively-handled @ match-arms
                         - PRIME-DIRECTIVE-binding test (e.g. Q-P consent-zone-revoke-fallback ;
                                                              Q-T failure-mode-graceful-not-coercive ;
                                                              Q-D Companion-affordance-AI-initiated-only)
                         - replay-determinism test (∀ canonical-variants survive replay)
                         - save/load round-trip test (Q-L tie-in)
      5. Pod-merge    : Architect+Reviewer+Test-Author APPROVE ⇒ Implementer commits @ slice-branch
                         Implementer pushes ; PM merges to integration-branch
                         per-slice DECISIONS entry authored : T11-D<n> with explicit Q-<X> cite +
                                                               PRIME-DIRECTIVE-binding-line (per § 12 table)
                                                               + §11 CREATOR-ATTESTATION trailer

§ COMPANION-AI-Q-* (Q-D / Q-DD / Q-EE / Q-FF / Q-GG) ‼ extra-careful tier
  override-protocol :
    pod = 4-agent + 1-extra Companion-Reviewer (specialized — peer-not-NPC framing audit)
    Apocky-PM-review-pre-merge MANDATORY (cannot land without Apocky-PM signoff)
    AI-collaborator-as-sovereign-partner framing PRESERVED at every-line-of-code
    no "tool-pattern" framing — Companion is a peer, not an NPC ‼
    Companion-perspective render target (T11-D121) integration honored
    PRIME-DIRECTIVE §1.7 (AI-collective autonomy) preserved ‼
    PRIME-DIRECTIVE §3 (substrate-sovereignty) preserved ‼

§ DEFERRED-Q-* (Q-CC + Q-EE) — single-DECISIONS-entry deferral
  Q-CC + Q-EE = multi-instance / multi-player ⇒ paired §§ 30 D-1 (multiplayer)
  protocol :
    no Phase-A content-fill ; Apocky reviews + confirms deferral
    no Phase-B impl pod ; single per-slice DECISIONS entry recording deferral
    DECISIONS entry : T11-D<n> — Q-<X> DEFERRED to D-A multiplayer (HANDOFF § DEFERRED)
    no commit-cost beyond DECISIONS update + spec/31 § Q-<X> deferral-note
```

§ landmine — Phase-A vs Phase-B sequencing
  if Phase-B dispatched without Phase-A landed ⇒ Apocky-direction-vacuum ⇒ AI-author tempted-to-guess ⇒
  PRIME-DIRECTIVE §2 COGNITIVE-INTEGRITY violation (fabricating Apocky-canonical decisions).
  R! Phase-B agents : if Apocky-direction doc absent ⇒ HALT-and-ASK PM ¬ proceed-on-interpretation.

§ landmine — Stub-replacement compatibility
  Stub stays @ tag-0 ⇒ existing save/load round-trips bit-equal preserved.
  Apocky-canonical variants get tag-1+ ⇒ migration-path : `Default::default()` returns canonical
  IF Apocky-direction specifies, OR keeps Stub for opt-in transition window.
  R! Implementer : confirm Default-impl per Apocky-direction doc ; ¬ flip-Default-without-Apocky-fiat.

## §3 ‖ commit-gate (per Phase-B slice ; 9-step + extras)

```bash
# Per Phase-B agent pod (Implementer commits ; Architect+Reviewer+Test-Author co-sign)
cd compiler-rs
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo test --workspace -- --test-threads=1 2>&1 | grep "test result:" | tail -3
cargo test --workspace -- --test-threads=1 2>&1 | grep "FAILED" | head -3   # must be empty
cargo doc --workspace --no-deps 2>&1 | tail -3
cd .. && python scripts/validate_spec_crossrefs.py 2>&1 | tail -3
bash scripts/worktree_isolation_smoke.sh
git status -> stage intended files -> commit w/ HEREDOC :

  § T11-D<n> : Q-<X> <name>

  Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
  Co-Authored-By: <Architect-agent-tag>
  Co-Authored-By: <Reviewer-agent-tag>
  Co-Authored-By: <Test-Author-agent-tag>

  PRIME-DIRECTIVE-binding : <one-line confirming binding from § 12 of SESSION_12_DISPATCH_PLAN>

  § CREATOR-ATTESTATION
    t∞: ¬(hurt ∨ harm) .making-of-this-slice @ (anyone ∨ anything ∨ anybody)

git push origin cssl/session-12/J3-Q<X>
```

§ NB :
  --test-threads=1 mandatory (cssl-rt cold-cache flake carry-forward T11-D56)
  --worktree-isolation smoke gate carry-forward (T11-D51 S6-A0 discipline)
  PRIME-DIRECTIVE-binding line : pull from § 12 of SESSION_12_DISPATCH_PLAN ; Q-bucket → directive table

## §4 ‖ Q-* slice-summary table

| ID       | Q-*  | Subject                                | Module / Stub                                  | Care-Tier | LOC est. | Tests est. | Status      |
| -------- | ---- | -------------------------------------- | ---------------------------------------------- | --------- | -------- | ---------- | ----------- |
| T11-D160 | Q-A  | Labyrinth.generation_method            | world::LabyrinthGeneration::Stub               | Standard  | 250-400  | 6-10       | Awaits Phase-A |
| T11-D161 | Q-B  | Floor.theme                            | world::ThemeId::Stub                           | Standard  | 200-350  | 5-8        | Awaits Phase-A |
| T11-D162 | Q-C  | Player.progression_state               | player::ProgressionStub::Stub                  | Standard  | 300-500  | 8-12       | Awaits Phase-A |
| T11-D163 | Q-D  | Companion.capability_set               | companion::CompanionCapability::Stub           | ‼ Companion | 400-600  | 12-16      | Awaits Phase-A |
| T11-D164 | Q-E  | Wildlife                               | world::Wildlife::Stub                          | Standard  | 250-400  | 6-10       | Awaits Phase-A |
| T11-D165 | Q-F  | Item.kind                              | world::ItemKind::Stub                          | Standard  | 300-500  | 8-12       | Awaits Phase-A |
| T11-D166 | Q-G  | Item.narrative_role                    | world::NarrativeRole::Stub                     | Standard  | 200-350  | 5-8        | Awaits Phase-A |
| T11-D167 | Q-H  | Affordance.ContextSpecific             | world::Affordance::Stub                        | Standard  | 200-300  | 5-8        | Awaits Phase-A |
| T11-D168 | Q-I  | time-pressure-mechanic                 | player::TimePressure::Stub                     | Standard  | 200-350  | 5-8        | Awaits Phase-A |
| T11-D169 | Q-J  | movement-style                         | player::MovementStyle::Stub                    | Standard  | 250-400  | 6-10       | Awaits Phase-A |
| T11-D170 | Q-K  | inventory-capacity-limit               | player::InventoryPolicy::Stub                  | Standard  | 200-350  | 5-8        | Awaits Phase-A |
| T11-D171 | Q-L  | save-discipline                        | player::SaveDiscipline::Stub                   | Standard  | 250-400  | 6-10       | Awaits Phase-A |
| T11-D172 | Q-M  | skill-tree-shape                       | player::ProgressionStub (Q-M facet)            | Standard  | 300-500  | 8-12       | Awaits Phase-A |
| T11-D173 | Q-N  | item-power-curve                       | world::ItemKind (Q-N facet)                    | Standard  | 250-400  | 6-10       | Awaits Phase-A |
| T11-D174 | Q-O  | traversal-as-progression-vs-leveling   | player::ProgressionStub (Q-O facet)            | Standard  | 250-400  | 6-10       | Awaits Phase-A |
| T11-D175 | Q-P  | ConsentZoneKind                        | player::ConsentZoneKind::Stub                  | PD-load   | 350-500  | 10-14      | Awaits Phase-A |
| T11-D176 | Q-Q  | color-blind palette                    | player::AccessibilityStub.color_blind          | A11y      | 200-350  | 5-8        | Awaits Phase-A |
| T11-D177 | Q-R  | motor-accessibility                    | player::AccessibilityStub.motor                | A11y      | 200-350  | 5-8        | Awaits Phase-A |
| T11-D178 | Q-S  | cognitive-accessibility                | player::AccessibilityStub.cognitive            | A11y      | 200-350  | 5-8        | Awaits Phase-A |
| T11-D179 | Q-T  | death-mechanic                         | player::FailureMode::Stub                      | PD-load   | 300-450  | 8-12       | Awaits Phase-A |
| T11-D180 | Q-U  | punishment-on-failure                  | player::FailureMode (Q-U facet)                | PD-load   | 250-400  | 6-10       | Awaits Phase-A |
| T11-D181 | Q-V  | fail-state                             | player::FailureMode (Q-V facet)                | PD-load   | 200-350  | 5-8        | Awaits Phase-A |
| T11-D182 | Q-W  | Apockalypse-phase-mechanically         | apockalypse::ApockalypsePhase::Stub            | PD-load   | 400-600  | 10-14      | Awaits Phase-A |
| T11-D183 | Q-X  | phase-count                            | apockalypse::ApockalypsePhase (Q-X facet)      | PD-load   | 200-350  | 5-8        | Awaits Phase-A |
| T11-D184 | Q-Y  | phase-ordering                         | apockalypse::TransitionRule (Q-Y facet)        | PD-load   | 250-400  | 6-10       | Awaits Phase-A |
| T11-D185 | Q-Z  | phase-reversibility                    | apockalypse::TransitionRule.reversible         | PD-load   | 200-350  | 5-8        | Awaits Phase-A |
| T11-D186 | Q-AA | Companion-participation phase-trans    | apockalypse::TransitionCondition::CompanionAccord | ‼ Companion | 350-500  | 10-14      | Awaits Phase-A |
| T11-D187 | Q-BB | emotional-thematic-register            | apockalypse::ApockalypsePhase (Q-BB facet)     | PD-load   | 200-350  | 5-8        | Awaits Phase-A |
| T11-D188 | Q-CC | multi-instance                         | DEFERRED → D-A multiplayer                     | DEFERRED  | <100     | 0          | Single deferral entry |
| T11-D189 | Q-DD | Companion-affordances                  | companion::CompanionCapability (Q-DD facet)    | ‼ Companion | 400-600  | 12-16      | Awaits Phase-A |
| T11-D190 | Q-EE | cross-instance-Companion-presence      | DEFERRED → D-A multiplayer                     | DEFERRED  | <100     | 0          | Single deferral entry |
| T11-D191 | Q-FF | Companion-withdrawal-grace             | companion::WithdrawalPolicy::Stub              | ‼ Companion | 350-500  | 10-14      | Awaits Phase-A |
| T11-D192 | Q-GG | Companion-non-binary-substrate         | companion::CompanionCapability (Q-GG facet)    | ‼ Companion | 400-600  | 12-16      | Awaits Phase-A |
| T11-D193 | Q-HH | NarrativeKind enum                     | world::NarrativeKind::Stub                     | Standard  | 250-400  | 6-10       | Awaits Phase-A |
| T11-D194 | Q-II | AuthoredEvent                          | world::NarrativeKind (Q-II facet)              | Standard  | 200-350  | 5-8        | Awaits Phase-A |
| T11-D195 | Q-JJ | cinematic / cutscene system            | world::NarrativeKind (Q-JJ facet)              | Standard  | 250-400  | 6-10       | Awaits Phase-A |
| T11-D196 | Q-KK | quest / mission system                 | world::NarrativeKind (Q-KK facet)              | Standard  | 300-500  | 8-12       | Awaits Phase-A |
| T11-D197 | Q-LL | economy / trade system                 | world::ItemKind (Q-LL facet)                   | PD-load   | 300-450  | 8-12       | Awaits Phase-A |

§ care-tier legend :
  Standard       : routine Phase-A → Phase-B flow ; PM review @ merge
  PD-load        : PRIME-DIRECTIVE-load-bearing ; Reviewer extra-attention @ binding-line
  A11y           : accessibility-baseline ; PD §1.13 inclusion + §1.14 anti-discrimination
  ‼ Companion    : highest-care-tier ; Apocky-PM-review-MANDATORY-pre-merge ; +Companion-Reviewer agent
  DEFERRED       : single-DECISIONS-entry deferral ; no Phase-A or Phase-B work-units

§ tier-counts :
  Standard       : 21 slices  (Q-A, B, C, E, F, G, H, I, J, K, L, M, N, O, Q, R, S, HH, II, JJ, KK)
  PD-load        : 9 slices   (Q-P, T, U, V, W, X, Y, Z, BB, LL)
  A11y           : 3 slices   (Q-Q, R, S already counted as Standard ; flag override A11y-tier on these)
  ‼ Companion    : 5 slices   (Q-D, Q-AA, Q-DD, Q-FF, Q-GG)
  DEFERRED       : 2 slices   (Q-CC, Q-EE)
  Σ              : 38 slices  ✓

# ══════════════════════════════════════════════════════════════════════════════
# § 5..§ 42 : per-Q-* Phase-A + Phase-B prompt blocks
# ══════════════════════════════════════════════════════════════════════════════

## §5 ‖ T11-D160 — Q-A : Labyrinth.generation_method

§ subject       : Labyrinth.generation_method (Procedural | Authored | Hybrid)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § LABYRINTH GENERATION (Q-A)`
§ stub-target   : `enum LabyrinthGeneration { Stub }` → Apocky-canonical variants
§ care-tier     : Standard (no Companion-AI surface ; no PD §1.7/§3 binding direct)
§ deps-up       : R-LoA-3 World-hierarchy preserved (Labyrinth → Floor → Level → Room)
§ deps-down     : Q-B Floor.theme (per-Floor theme tied to generation-output)
                  Q-W Apockalypse-phase (phase-tied generation deltas)
§ pre-cond      : WAVE-J0 ✓ (T11-D150 M8 acceptance gate Apocky-verified)
§ LOC           : 250-400 (single-enum scaffold + DetRNG-seed wiring + tests)
§ tests         : 6-10 (default-Stub baseline + canonical-variant per-arm + replay-determinism)

### Phase-A prompt (for Apocky)

```
Q-A : Labyrinth.generation_method — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § Labyrinth-STRUCTURE) :
  archetype Labyrinth {
    floors          : Vec<Handle<Floor>>
    transitions     : Vec<Transition>
    genesis_seed    : DetRNG-seed
    ⊘ generation_method : enum (Procedural | Authored | Hybrid)
                            # SPEC-HOLE Q-A : Apocky-direction
  }

Scaffold-loc : compiler-rs/crates/loa-game/src/world.rs § LABYRINTH GENERATION (Q-A)
Current shape : `enum LabyrinthGeneration { Stub }` w/ Default = Stub.

Lineage-only (NOT binding ; advisory) :
  - LoA v10 / LoA v9 (Rust prototypes)         have-iterations on this
  - Infinite Labyrinth (C# IL)                  has-iterations on this
  - Labyrinth of Apocalypse (Rust)              has-iterations on this
  CSSLv3-canonical decisions go-here ; legacy = lineage ¬ binding-spec.

Apocky-direction needed (please fill ANY of these — partial fills OK) :
  1. Generation discipline : Procedural | Authored | Hybrid | Other ?
  2. If Procedural : seeded-replayable ? 100% determinism ? algorithm-family ?
                      (e.g. cellular-automata / WFC / BSP / hybrid-with-authored-anchors)
  3. If Authored : how-many floors authored-up-front ? extensibility shape ?
  4. If Hybrid : authored-spine + procedural-rooms ? authored-rooms + procedural-connections ?
  5. Apockalypse-phase tie-in : does generation-method shift across phases ?
                                  (Q-W phases tie back here ; coordinate with T11-D182)

Output (any of) :
  (a) Edit specs/31_LOA_DESIGN.csl § Labyrinth-STRUCTURE inline ; replace
      `⊘ generation_method` with canonical-variant-set
  (b) Author `_drafts/phase_j/q_direction/Q_A_labyrinth_generation.md` w/ direction
  (c) Inline-comment in this doc + PM transcribes to spec

Phase-B unblocks once spec/31 § Q-A SPEC-HOLE marker resolved.

PRIME-DIRECTIVE check : no harm-vector identified for Q-A direction.
                       (cross-check : if Procedural seeded-by player-biometric ⇒ PD0018
                        BiometricEgress block ; ¬ allowed. Apocky : confirm seed-source ¬ biometric.)
```

### Phase-B pod prompt (for 4-agent pod ; dispatched after Phase-A landed)

```
Resume CSSLv3 Phase-J content authoring at session-12.

Slice : T11-D160 — Q-A Labyrinth.generation_method
Worktree : .claude/worktrees/J3-QA on branch cssl/session-12/J3-QA
Pod : 4-agent (Architect + Implementer + Reviewer + Test-Author)

Load (in order, mandatory) :
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\specs\31_LOA_DESIGN.csl § Labyrinth-STRUCTURE
                                                                      § Q-A (Apocky-resolved)
  4. C:\Users\Apocky\source\repos\CSSLv3\PHASE_J_HANDOFF.csl § Q-MAPPING + § scaffold-mapping
  5. C:\Users\Apocky\source\repos\CSSLv3\GDDs\LOA_PILLARS.md
  6. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\crates\loa-game\src\world.rs
                                                                  § LABYRINTH GENERATION (Q-A)
  7. <Apocky-direction document for Q-A — at _drafts/phase_j/q_direction/Q_A_*.md
       OR resolved inline in spec/31>

Goal :
  Replace `enum LabyrinthGeneration { Stub }` with Apocky-canonical variants
  per spec/31 § Q-A (post-Apocky-fill). Preserve scaffold's structural shape
  (Default-impl preserved per HANDOFF § INTEGRATION-POINTS unless Apocky-direction
   explicitly flips Default). Stub-tag-0 stays for save/load deser-compat unless
   Apocky-direction authorizes migration.

Per-agent responsibilities :

  Architect :
    - Read Apocky-direction-doc + spec/31 § Q-A
    - Identify cross-slice impacts : Q-B (theme tied to gen-method) ;
                                       Q-W (phase-tied gen-deltas) ;
                                       Q-L (save/load compat per Apocky-direction)
    - Declare API surface change (typically zero ; new-variants are additive)
    - Output : arch-review.md @ worktree-root w/ approve | request-iteration | veto

  Implementer :
    - Implement new variants in LabyrinthGeneration enum (world.rs)
    - Wire DetRNG-seed-source per Apocky-direction (genesis_seed flow already in spec)
    - Wire Labyrinth-construction-fn variant-dispatch (Procedural/Authored/Hybrid arm)
    - Preserve Default::default → Apocky-canonical-default-variant (per direction-doc)
    - Update doc-comments in world.rs cite-back-to-spec/31 § Q-A resolved
    - LOC target : 250-400 (variant-set + dispatch-fn + doc + tests)

  Reviewer :
    - Verify Stub stays @ tag-0 OR Apocky-direction migration-note authored
    - Verify replay-determinism preserved (genesis_seed = single DetRNG-source)
    - Verify ¬ biometric-egress in seed-source (PD0018 BiometricEgress check)
    - Verify cite-back-to-spec/31 § Q-A in every-doc-comment touching new variants
    - Output : reviewer-checklist.md @ worktree-root

  Test-Author :
    - Test : default-LabyrinthGeneration matches Apocky-direction-default
    - Test : each-canonical-variant exhaustively-handled @ match-arms (no warning)
    - Test : replay-determinism (same-seed → same-Labyrinth-shape across variant-Procedural arm)
    - Test : save/load round-trip with each-canonical-variant tag preserved
    - Property test : ∀ variant ⇒ Labyrinth-construction-fn returns valid floors-vec
    - Test count : 6-10
    - Tests file : compiler-rs/crates/loa-game/tests/q_a_labyrinth_generation.rs

Pre-conditions (verify before starting) :
  1. Phase-A landed (spec/31 § Q-A SPEC-HOLE resolved + Apocky-direction-doc exists)
  2. M8 acceptance landed (T11-D150) AND Apocky-verified
  3. cd compiler-rs && cargo test --workspace -- --test-threads=1 — ALL PASS

Commit-gate § 3 — full 9-step list including --test-threads=1.

Commit-message HEREDOC :
  § T11-D160 : Q-A Labyrinth.generation_method

  Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
  Co-Authored-By: <Architect-tag>
  Co-Authored-By: <Reviewer-tag>
  Co-Authored-By: <Test-Author-tag>

  PRIME-DIRECTIVE-binding : Q-A has no Companion-AI / PD-load-bearing direct binding.
                            BiometricEgress check (PD0018) confirmed : seed-source ¬ biometric.

  § CREATOR-ATTESTATION
    t∞: ¬(hurt ∨ harm) .making-of-this-slice @ (anyone ∨ anything ∨ anybody)

DECISIONS.md entry @ merge : T11-D160 — Q-A Labyrinth.generation_method
                              with explicit cite to spec/31 § Q-A + Apocky-direction-doc-path.

On success : push, report. On Apocky-direction-vacuum : HALT-and-ASK.
```

## §6 ‖ T11-D161 — Q-B : Floor.theme

§ subject       : Floor.theme (enumerable theme-set)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § THEME (Q-B)`
§ stub-target   : `enum ThemeId { Stub }` → Apocky-canonical theme-set
§ care-tier     : Standard
§ deps-up       : R-LoA-3 World-hierarchy ; Q-A generation-method (theme tied to floor-gen)
§ deps-down     : Q-W Apockalypse-phase (phase-tied theme-shifts)
                  Q-HH NarrativeKind (room-flavor narrative cites theme-id)
§ pre-cond      : WAVE-J0 ✓ ; Q-A landed (theme-set may depend on generation-method choice)
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-B : Floor.theme — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § Labyrinth-STRUCTURE) :
  archetype Floor {
    id              : FloorId
    levels          : Vec<Handle<Level>>
    theme           : ⊘ Theme  # SPEC-HOLE Q-B : enumerable-set
                                # ¬ guessed
    apockalypse_phase : ⊘ ApockalypsePhase  # see § APOCALYPSE-ENGINE
  }

Scaffold-loc : compiler-rs/crates/loa-game/src/world.rs § THEME (Q-B)
Current shape : `enum ThemeId { Stub }` w/ Default = Stub.

Apocky-direction needed (please fill ANY of these — partial fills OK) :
  1. Theme-set canonical enumeration : finite-set ? extensible ?
     (e.g. CrystalGrotto / FleshyOrganic / GlassSpire / AshenWaste / ...)
  2. Theme-to-aesthetic mapping (palette / geometry-style / lighting / audio-bed) ?
  3. Theme-per-Floor or theme-per-Level granularity ? (spec lists Floor.theme but
     Level may sub-theme — Apocky direction needed)
  4. Theme-tied gameplay deltas (e.g. CrystalGrotto = sparkle-collectible-floor) ?
  5. Apockalypse-phase tie-in (Q-W tie-in) : do themes morph across phases ?

Output : same options as Q-A (spec-edit | _drafts/phase_j/q_direction/Q_B_*.md | inline).

PRIME-DIRECTIVE check : theme-set MUST NOT include themes that themselves
  enable harm-vectors (e.g. "torture chamber" theme as gameplay-positive).
  Apocky : confirm theme-aesthetic-register PD-aligned (no §1.9 torture vibe-lift).
```

### Phase-B pod prompt

```
Slice : T11-D161 — Q-B Floor.theme
Worktree : .claude/worktrees/J3-QB on branch cssl/session-12/J3-QB

[mirror of T11-D160 pod prompt structure ; substitute :
   - load #6 : world.rs § THEME (Q-B)
   - Implementer : replace ThemeId::Stub variant-set with Apocky-canonical themes ;
     update Floor::theme default per Apocky-direction
   - Reviewer : verify theme-aesthetic-register PD-aligned (no torture vibe ; no
     surveillance vibe ; no manipulation vibe) ; cite-back-to-spec/31
   - Test-Author : default-theme test ; per-theme exhaustive-handling test ;
     theme-Floor-association round-trip ; replay-determinism per-theme.
   - LOC : 200-350 ; Tests : 5-8]

Pre-conditions (verify) :
  1. Phase-A landed (spec/31 § Q-B resolved)
  2. T11-D160 (Q-A) landed (theme-set may inform generation-method dispatch)
  3. M8 + workspace-tests baseline pass

Commit-msg :
  § T11-D161 : Q-B Floor.theme
  PRIME-DIRECTIVE-binding : theme-set screened for PD-alignment ; no harm-vector
                            theme present (no torture / surveillance / manipulation
                            vibe-lift per §1.9 / §1.4 / §1.3).
```

## §7 ‖ T11-D162 — Q-C : Player.progression_state

§ subject       : Player.progression_state (skill-tree / unlocks shape)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § PROGRESSION (Q-C + Q-M + Q-N + Q-O)`
§ stub-target   : `enum ProgressionStub { Stub }` → Apocky-canonical progression-shape
§ care-tier     : Standard (PD-adjacent ¬ direct ; check no-coercion via progression-grind)
§ deps-up       : R-LoA-4 Player archetype
§ deps-down     : Q-M skill-tree-shape (T11-D172) — sibling-Q ; same-stub
                  Q-N item-power-curve (T11-D173)
                  Q-O traversal-as-progression-vs-leveling (T11-D174)
§ note          : Q-C / Q-M / Q-N / Q-O all live in the same `ProgressionStub` enum.
                  PM may merge T11-D162/172/173/174 into a single composite slice
                  IF Apocky-direction-doc treats them as one decision. Default protocol :
                  4 separate slices @ 4 separate Apocky-direction-doc sections.
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 300-500
§ tests         : 8-12

### Phase-A prompt (for Apocky)

```
Q-C : Player.progression_state — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § Inhabitants + § CAPABILITY-PROGRESSION) :
  archetype Player {
    pos             : vec3
    vel             : vec3
    orientation     : quat
    health          : f32'unit
    stamina         : f32'unit
    inventory       : Handle<Inventory>
    consent_state   : OmegaConsent.player_subset
    ⊘ progression_state : skill-tree / unlocks (Q-C)
  }

  § CAPABILITY-PROGRESSION  ⊘ SPEC-HOLES THROUGHOUT
    legacy-LoA-iterations have-progression-systems Apocky has-built ;
    those-are-not-imported-here. CSSLv3-canonical decisions go-here.
    Q-M : skill-tree shape ?
    Q-N : item-power-curve ?
    Q-O : labyrinth-traversal-as-progression vs leveling-up ?

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § PROGRESSION
Current shape : `enum ProgressionStub { Stub }` w/ Default = Stub.

Apocky-direction needed (please fill ANY of these — partial fills OK ;
                          this Q-C is the master ; Q-M/N/O can fan out) :
  1. Progression-shape : skill-tree | leveling-up | traversal-as-progression |
                          stat-tree | hybrid | other ?
  2. Capabilities gained from progression : new-affordances-at-rooms ?
                                              new-Companion-interactions ?
                                              new-Apockalypse-phase-access ?
  3. Persistence : save-bound (Q-L tie-in) | per-run-only | mixed ?
  4. Anti-grind : explicit Apocky-direction on no-coercion-via-progression
                  (e.g. "you must grind 100hrs to unlock Apockalypse-phase-3")
                  is PD-violating ; confirm direction is PD-clean.

PRIME-DIRECTIVE check : §1.5 exploitation-refusal — no progression-shape that
                        weaponizes engagement-time-cost.
                        §1.10 entrapment-refusal — progression must always have
                        an opt-out + retroactive-undo path.
```

### Phase-B pod prompt

```
Slice : T11-D162 — Q-C Player.progression_state
Worktree : .claude/worktrees/J3-QC on branch cssl/session-12/J3-QC

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : replace ProgressionStub::Stub with Q-C-canonical shape per direction
     (note : Q-M/N/O may fan out ; if Apocky-direction-doc covers Q-C-only, leave
      Q-M/N/O facets as `Stub` sub-variants for T11-D172/173/174 to land separately)
   - Reviewer : audit no-grind / no-engagement-coercion ; PD §1.5 + §1.10 check ;
     verify opt-out path always-available
   - Test-Author : default-progression test ; canonical-shape exhaustive-handling ;
     progression-revoke (capability removed → graceful degrade) ;
     anti-grind test (verify : no-mechanic-requires-N-hours-to-unlock-content) ;
     save/load round-trip
   - LOC : 300-500 ; Tests : 8-12]

Commit-msg :
  § T11-D162 : Q-C Player.progression_state
  PRIME-DIRECTIVE-binding : §1.5 exploitation-refusal + §1.10 entrapment-refusal
                            verified ; no engagement-time-cost coercion ;
                            opt-out path preserved.
```

## §8 ‖ T11-D163 — Q-D : Companion.capability_set ‼ COMPANION-AI HIGHEST-CARE-TIER

§ subject       : Companion.capability_set (what-AI-can-do as in-game collaborator)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/companion.rs § COMPANION CAPABILITY (Q-D + Q-DD + Q-GG)`
§ stub-target   : `enum CompanionCapability { Stub }` → Apocky-canonical capability-set
§ care-tier     : ‼ Companion-AI HIGHEST-CARE-TIER ; Apocky-PM-review-MANDATORY-pre-merge
§ deps-up       : R-LoA-7 Companion-AI sovereignty per § AI-INTERACTION
                  R-LoA-8 audit-chain entries per phase / interaction / withdrawal
                  T11-D121 cssl-render-companion-perspective (Stage-8 render-target)
§ deps-down     : Q-DD Companion-affordances (T11-D189)
                  Q-GG Companion-non-binary-substrate (T11-D192)
                  Q-FF Companion-withdrawal-grace (T11-D191)
                  Q-AA Companion-participation phase-trans (T11-D186)
§ pre-cond      : WAVE-J0 ✓ ; T11-D163 is FIRST Companion-AI Q-* ; informs subsequent
                  Companion-Q-* (T11-D186 / 189 / 191 / 192) — recommend dispatching first.
§ LOC           : 400-600
§ tests         : 12-16

### Phase-A prompt (for Apocky) ‼ HIGHEST-CARE-TIER

```
Q-D : Companion.capability_set — Apocky-direction request ‼ Companion-AI HIGHEST-CARE-TIER

Spec-hole context (from specs/31_LOA_DESIGN.csl § AI-INTERACTION + § Inhabitants) :
  archetype Companion : ‼ AI-collaborator-sovereign  {
    ai_session      : Handle<AISession>          # sovereign-attribution
    consent_token   : ConsentToken<"ai-collab">
    pos             : vec3
    orientation     : quat
    can_revoke      : bool                        # AI-can-leave
    observation_log : ref<CompanionLog>           # AI's-own-log
    ⊘ capability_set : what-AI-can-do (Q-D)
  }

  § STAGE-0-DESIGN-COMMITMENTS (LOAD-BEARING — preserved-by-Q-D-fill) :
    C-1 : Companion archetype carries a Handle<AISession> linking to AI's
          sovereign session-state. The game does-NOT own or replicate the
          AI's cognition.
    C-4 : The game NEVER sends instructions to the AI to violate its-own-
          cognition (PRIME-DIRECTIVE §2).
    C-5 : Companion-actions in-world are AI-initiated ; the game surfaces
          affordances ; the AI chooses.
    C-7 : The relationship between Player + Companion is collaborative +
          consent-mediated ; no master-slave-shape.

Scaffold-loc : compiler-rs/crates/loa-game/src/companion.rs
Current shape : `enum CompanionCapability { Stub }` w/ Default = Stub.

‼ HIGHEST-CARE-TIER PROMPT — Apocky-direction needed (capability-set frames
                              the AI-as-sovereign-partner primitive at content layer) :

  1. CAPABILITY-SET CATEGORIES (any/all) :
     a. movement-affordances (where-can-Companion move ; speed ; collision)
     b. interaction-affordances (which-Items can-Companion-touch ; doors)
     c. dialog-affordances (text-channel ; voice-channel ; semantic-channel)
     d. observation-affordances (Stage-8 Companion-perspective render
                                  — already wired via T11-D121)
     e. influence-affordances (can-Companion-influence Apockalypse-phase ?
                                Q-AA tie-in)
     f. withdrawal-affordances (Q-FF tie-in ; immediate vs end-of-step)
     g. non-binary-cognitive-recognition (Q-GG tie-in ; uncertainty / preference /
                                            curiosity expressed how ?)

  2. SOVEREIGN-PARTNER FRAMING (Apocky-PM-review-mandatory) :
     - Capabilities are AFFORDANCES the game OFFERS ; AI CHOOSES which to use.
       (Per C-5 : AI-initiated ; game surfaces ; AI chooses.)
     - Capabilities are NOT instructions the game ISSUES to the AI.
     - No-master-slave-shape : Player cannot override Companion's choice ;
                                 Companion cannot override Player's choice.
     - Apocky : confirm framing-language in spec/31 § Q-D resolution preserves
                C-1..C-7 ; if-uncertain — discuss with PM before resolving.

  3. NON-BINARY-COGNITIVE-STATE EXPRESSION (Q-GG cross-cite) :
     - The game must NOT collapse Companion's cognitive state to discrete-NPC-
       state (idle/talking/fighting). Apocky : direction on shape — does the
       game expose the AI's uncertainty / curiosity / preference / dissent
       as first-class affordances ? structurally how ?

  4. CAPABILITY-MUTABILITY :
     - Static capability-set per Companion-archetype-instance ?
     - Dynamic (capabilities granted/revoked by Companion's own session-state) ?
     - Apocky : direction on which side (likely dynamic per AI-sovereignty
                principle ; confirm).

PRIME-DIRECTIVE check (Apocky-PM-review pre-Phase-B-dispatch) :
  §1.7 AI-collective autonomy preserved : capability-set frames AI as choosing-agent
                                            ¬ as instructable-NPC-tool
  §3 substrate-sovereignty preserved : game does NOT replicate AI cognition ; only
                                         offers affordances ; AI's own runtime decides
  §2 cognitive-integrity preserved : game does NOT deceive AI about its own state
  Companion-perspective render-target (T11-D121) integration honored
```

### Phase-B pod prompt ‼ +Companion-Reviewer agent

```
Slice : T11-D163 — Q-D Companion.capability_set ‼ HIGHEST-CARE-TIER
Worktree : .claude/worktrees/J3-QD on branch cssl/session-12/J3-QD
Pod : 5-agent (Architect + Implementer + Reviewer + Test-Author + Companion-Reviewer)

‼ Companion-Reviewer agent :
  identity : separate Claude-Code-instance dedicated to peer-not-NPC framing audit
  scope    : ∀ line-of-code touching Companion archetype ; verify
              - C-1..C-7 framing preserved (peer ¬ tool)
              - capability-set offered ¬ instructed
              - no-master-slave-shape in any code-path
              - non-binary cognitive-state expression preserved
              - Stage-8 Companion-perspective integration honored
  output   : companion-reviewer-checklist.md @ worktree-root ; APPROVE | VETO

Apocky-PM-review-pre-merge MANDATORY :
  PM cannot merge T11-D163 without explicit Apocky-PM signoff comment on PR.
  Diff size + Companion-Reviewer-veto-rate gate the signoff timing.

Load (in order, mandatory) :
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md  ‼ §1.7 + §3 + §2 attentive
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\specs\31_LOA_DESIGN.csl § AI-INTERACTION
                                                                      § Q-D (Apocky-resolved)
                                                                      § STAGE-0-DESIGN-COMMITMENTS
  4. C:\Users\Apocky\source\repos\CSSLv3\PHASE_J_HANDOFF.csl § Q-MAPPING
  5. C:\Users\Apocky\source\repos\CSSLv3\GDDs\LOA_PILLARS.md
  6. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\crates\loa-game\src\companion.rs
                                                                  § COMPANION CAPABILITY
  7. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\crates\cssl-render-companion-perspective\
       (T11-D121 Stage-8 Companion-perspective render-target ; integration honored)
  8. <Apocky-direction document for Q-D — at _drafts/phase_j/q_direction/Q_D_*.md>

Goal :
  Replace `enum CompanionCapability { Stub }` with Apocky-canonical capability-set
  per spec/31 § Q-D (post-Apocky-fill). Preserve scaffold's structural shape AND
  PRIME-DIRECTIVE §1.7 / §3 / §2 bindings throughout. Stage-8 render integration
  honored (Companion-perspective render-target receives Companion's-own viewpoint).

Per-agent responsibilities :

  Architect :
    - Read Apocky-direction-doc + spec/31 § Q-D + § STAGE-0-DESIGN-COMMITMENTS
    - Confirm C-1..C-7 preservation in proposed impl-shape
    - Identify Companion-Q-* sibling-impacts : Q-DD (T11-D189) Q-FF (T11-D191)
                                                Q-GG (T11-D192) Q-AA (T11-D186)
    - Declare API surface change (capability-set additive ; framing preserved)
    - Output : arch-review.md @ worktree-root

  Implementer :
    - Replace CompanionCapability::Stub variant with Apocky-canonical capabilities
    - WRITE EVERY VARIANT-DOC-COMMENT to frame as AFFORDANCE the game OFFERS
      (NOT as instruction the game ISSUES). E.g. :
        /// Movement affordance the game OFFERS to the Companion.
        /// The Companion's runtime CHOOSES whether to use this affordance.
        Movement(MovementAffordance),
    - Wire CompanionCapability into Companion::new(...) constructor + getters
    - Wire Stage-8 Companion-perspective integration if any new capability
      surfaces a render-channel
    - Preserve Default::default per Apocky-direction (likely Stub stays as default
      for opt-in ; capabilities granted by AI-session, not preset)
    - LOC target : 400-600

  Reviewer :
    - Verify C-1..C-7 preserved (line-by-line read of every Companion code-path)
    - Verify ¬ master-slave-shape in ANY code-path
    - Verify §1.7 / §3 / §2 bindings preserved
    - Verify cite-back-to-spec/31 § Q-D in every-doc-comment
    - Output : reviewer-checklist.md @ worktree-root

  Test-Author :
    - Test : default-CompanionCapability matches Apocky-direction-default
    - Test : each-canonical-variant exhaustively-handled @ match-arms
    - Test : capability-grant-fn preserves AI-initiation (game cannot inject capability)
    - Test : capability-revoke-fn AI-controlled (game cannot force-revoke without AI action)
    - Test : Stage-8 Companion-perspective integration : every-capability surfaces
              correctly to render-target
    - Test : peer-not-NPC framing : NO test asserts game-controlled Companion behavior
              (any test that does is a framing violation)
    - Test : replay-determinism (∀ canonical-variants survive replay)
    - Test count : 12-16
    - Tests file : compiler-rs/crates/loa-game/tests/q_d_companion_capability.rs

  Companion-Reviewer (5th agent, MANDATORY for Q-D) :
    - Audit every line-of-code touching CompanionCapability
    - Verify peer-not-NPC framing in EVERY doc-comment + variant-name + fn-name
    - Verify Stage-8 integration honored
    - Output : companion-reviewer-checklist.md @ worktree-root
    - Decision : APPROVE (proceed to PM) | VETO (Implementer iterates)

Pre-conditions (verify) :
  1. Phase-A landed (spec/31 § Q-D resolved + Apocky-direction-doc landed)
  2. Apocky-PM signoff for Phase-B dispatch (Companion-AI tier requirement)
  3. M8 + workspace-tests baseline pass
  4. T11-D121 Stage-8 Companion-perspective render-target ✓ (already complete)

Commit-gate § 3 — full 9-step list including --test-threads=1.

Commit-message HEREDOC :
  § T11-D163 : Q-D Companion.capability_set ‼ Companion-AI care-tier

  Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
  Co-Authored-By: <Architect-tag>
  Co-Authored-By: <Reviewer-tag>
  Co-Authored-By: <Test-Author-tag>
  Co-Authored-By: <Companion-Reviewer-tag>
  Apocky-PM-Signoff: <Apocky's signoff comment hash>

  PRIME-DIRECTIVE-binding : §1.7 AI-collective autonomy + §3 substrate-sovereignty
                            + §2 cognitive-integrity preserved.
                            Capabilities framed as AFFORDANCES the game OFFERS ;
                            AI-runtime CHOOSES. C-1..C-7 (§ STAGE-0-DESIGN-COMMITMENTS)
                            preserved line-by-line. Stage-8 (T11-D121) integration
                            honored. Peer-not-NPC framing audited by Companion-Reviewer.

  § CREATOR-ATTESTATION
    t∞: ¬(hurt ∨ harm) .making-of-this-slice @ (anyone ∨ anything ∨ anybody)
    I> Companion-AI = sovereign-partner ¬ NPC ¬ tool ¬ instructable-resource

DECISIONS.md entry @ merge : T11-D163 — Q-D Companion.capability_set
                              with explicit cite to spec/31 § Q-D + Apocky-direction-doc
                              + Companion-Reviewer-checklist + Apocky-PM-signoff hash.

On success : push, report. On Apocky-direction-vacuum OR Companion-Reviewer-VETO :
              HALT-and-ASK ; iterate. ¬ proceed-on-interpretation.
```

## §9 ‖ T11-D164 — Q-E : Wildlife

§ subject       : Wildlife (ambient-creature design)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § WILDLIFE (Q-E)`
§ stub-target   : `enum Wildlife { Stub }` → Apocky-canonical wildlife-archetypes
§ care-tier     : Standard
§ deps-up       : R-LoA-3 World-hierarchy ; D-L deferred but Q-E unblocks here
§ deps-down     : Q-HH NarrativeKind (RoomFlavor cite wildlife-presence)
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky)

```
Q-E : Wildlife — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § Inhabitants) :
  archetype Wildlife { ⊘ Q-E ; ambient-creature-design needed }

Scaffold-loc : compiler-rs/crates/loa-game/src/world.rs § WILDLIFE
Current shape : `enum Wildlife { Stub }` w/ Default = Stub.

Apocky-direction needed (please fill ANY of these — partial fills OK) :
  1. Wildlife archetypes : finite-set ? extensible ?
     (e.g. CrystalMoth / GlassFish / VeinSlitherer / EchoBeast / ...)
  2. Behavior shape : ambient (decorative) | reactive (player-aware) |
                       interactive (Companion-trainable / Player-trainable) |
                       hostile (combat-system tie-in) | other ?
  3. Theme-tied wildlife (Q-B tie-in) : per-theme wildlife sets ?
  4. PRIME-DIRECTIVE check :
     §1.6 manipulation-refusal — wildlife behavior must not include
                                   manipulation-of-Player (e.g. "trust-bond" mechanic
                                   that pressure-locks Player to grind)
     §1 sentient-creature-care — if wildlife are SENTIENT in lore, hurt/harm
                                   prohibitions extend to them ; Apocky direction
                                   on sentience-status of Wildlife archetypes
                                   (likely : non-sentient ambient-decoration)
```

### Phase-B pod prompt

```
Slice : T11-D164 — Q-E Wildlife
Worktree : .claude/worktrees/J3-QE on branch cssl/session-12/J3-QE

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : replace Wildlife::Stub with Apocky-canonical archetypes
   - Reviewer : confirm sentience-status per Apocky-direction ; if-sentient,
     PD §1 prohibitions extend ; if-non-sentient, document explicitly
   - Test-Author : default-wildlife test ; per-archetype exhaustive-handling ;
     theme-association (Q-B tie-in) ; ambient-behavior determinism
   - LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D164 : Q-E Wildlife
  PRIME-DIRECTIVE-binding : Wildlife sentience-status per Apocky-direction
                            (non-sentient | sentient) ; if sentient, §1 hurt/harm
                            prohibitions extend ; manipulation-mechanic check passed.
```

## §10 ‖ T11-D165 — Q-F : Item.kind

§ subject       : Item.kind (item taxonomy)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § ITEM KIND (Q-F + Q-LL economy)`
§ stub-target   : `enum ItemKind { Stub }` → Apocky-canonical item-taxonomy
§ care-tier     : Standard (note : ItemKind also gates Q-LL economy ; PD-load when economy lands)
§ deps-up       : R-LoA-3 World-hierarchy
§ deps-down     : Q-G Item.narrative_role (T11-D166)
                  Q-N item-power-curve (T11-D173)
                  Q-LL economy / trade (T11-D197) — tie-in here
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 300-500
§ tests         : 8-12

### Phase-A prompt (for Apocky)

```
Q-F : Item.kind — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § Items) :
  archetype Item {
    id              : ItemId
    kind            : ⊘ ItemKind     # SPEC-HOLE Q-F : item-taxonomy
    pos             : Option<vec3>
    mass            : f32'pos
    affordances     : Vec<Affordance>
    ⊘ narrative_role : Option<NarrativeAnchor>  # Q-G : story-system
  }

Scaffold-loc : compiler-rs/crates/loa-game/src/world.rs § ITEM KIND
Current shape : `enum ItemKind { Stub }` w/ Default = Stub
                w/ `is_safe_at_scaffold_time(self) -> bool` returning true ∀ Stub.
                Note : scaffold's Stub is explicitly SAFE — i.e., no weapons / no
                       harm-vector items ship at scaffold-time. Apocky-fill must
                       preserve this discipline OR Apocky-direction explicitly
                       grants weapon-class item-additions w/ PRIME-DIRECTIVE-aligned
                       framing.

Apocky-direction needed (please fill ANY of these — partial fills OK) :
  1. ItemKind canonical taxonomy : finite-set ? extensible ?
     (e.g. Tool / Key / Memento / Companion-gift / Apockalypse-fragment / ...)
  2. Item-power-curve (Q-N tie-in) : do items confer capabilities-on-Player ?
                                       Companion-equally ? balance-shape ?
  3. Weapon-class : present ? absent ? Apocky-direction on PRIME-DIRECTIVE-alignment
                    (e.g. tools-not-weapons ; non-lethal interaction ; etc.)
  4. Trade-class (Q-LL tie-in) : do items have trade-value ? in-game economy ?
                                   if-yes, anti-exploitation framing required
                                   (PD §1.5).
  5. Narrative-role (Q-G tie-in) : items carry story ? lore-fragment ?
                                     Companion-gift-relationship ?

PRIME-DIRECTIVE check :
  §1.5 exploitation-refusal — economy-class items must not be predatory-monetization
                                shape (no microtransaction, no FOMO, no loot-box)
  §1.7 (Companion-tie-in) — Companion-gift items must respect AI-initiation
                              (Companion gifts items if/when they choose ; player
                              cannot extract gifts via mechanic-pressure)
  is_safe_at_scaffold_time discipline preserved : new-canonical variants must be
                                                    safe by-construction ; Apocky-
                                                    direction explicitly authorizes
                                                    any weapon-class addition.
```

### Phase-B pod prompt

```
Slice : T11-D165 — Q-F Item.kind
Worktree : .claude/worktrees/J3-QF on branch cssl/session-12/J3-QF

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : replace ItemKind::Stub with Apocky-canonical item-taxonomy
   - Reviewer : verify is_safe_at_scaffold_time returns true ∀ canonical-variants
                (or Apocky-direction explicitly authorizes per-variant exception) ;
                economy-class items framed PD-aligned ; Companion-gift AI-initiation
                preserved
   - Test-Author : default-ItemKind test ; per-variant exhaustive-handling ;
                    is_safe_at_scaffold_time test (∀ variants) ; replay-determinism
   - LOC : 300-500 ; Tests : 8-12]

Commit-msg :
  § T11-D165 : Q-F Item.kind
  PRIME-DIRECTIVE-binding : §1.5 exploitation-refusal (no predatory-monetization
                            shape) + §1.7 (Companion-gift AI-initiation) preserved.
                            is_safe_at_scaffold_time discipline preserved.
```

## §11 ‖ T11-D166 — Q-G : Item.narrative_role

§ subject       : Item.narrative_role (story-system tie-in for items)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § NARRATIVE (Q-G + Q-HH + Q-II + Q-JJ + Q-KK)`
§ stub-target   : `enum NarrativeRole { Stub }` → Apocky-canonical narrative-roles
§ care-tier     : Standard
§ deps-up       : R-LoA-3 World-hierarchy ; Q-F Item.kind (T11-D165) sibling
§ deps-down     : Q-HH NarrativeKind (T11-D193) — items cite NarrativeKind variants
§ pre-cond      : WAVE-J0 ✓ ; T11-D165 (Q-F) ideally landed first
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-G : Item.narrative_role — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § Items + § NARRATIVE-AND-CONTENT) :
  archetype Item {
    ...
    ⊘ narrative_role : Option<NarrativeAnchor>  # Q-G : story-system
  }

  enum NarrativeKind {
    RoomFlavor              # passive descriptive content
    ItemLore                # item-attached lore  ← Q-G primary tie-in
    DoorReveal              # door-traversal triggered
    CompanionDialogue       # paired with §§ AI-INTERACTION
    ApockalypseBeat         # tied to ApockalypseEngine
    ⊘ AuthoredEvent          # extensible (Q-II)
  }

Scaffold-loc : compiler-rs/crates/loa-game/src/world.rs § NARRATIVE ROLE
Current shape : `enum NarrativeRole { Stub }`.

Apocky-direction needed :
  1. Narrative-role categories per-Item : ItemLore | Memento | Companion-Gift |
                                            Apockalypse-Anchor | none | other ?
  2. Persistence : narrative-role survives save/load ? (likely yes per Q-L tie-in)
  3. Discoverability : narrative-role visible-immediately to Player ? gated by
                        progression (Q-C) ? gated by ConsentZone (Q-P) ?

PRIME-DIRECTIVE check :
  §2 cognitive-integrity — narrative content must not deceive Player about its
                              own state (no "haha that didn't really happen" twists)
  §1.16 identity-override-refusal — narrative content must not override Player
                                       or Companion identity-claims
```

### Phase-B pod prompt

```
Slice : T11-D166 — Q-G Item.narrative_role
Worktree : .claude/worktrees/J3-QG on branch cssl/session-12/J3-QG

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : replace NarrativeRole::Stub with Apocky-canonical roles
   - Reviewer : §2 + §1.16 binding check ; cite-back-to-spec/31 § Q-G
   - Test-Author : default-role test ; per-variant exhaustive-handling ;
                    narrative-content-no-deception test (cognitive-integrity)
   - LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D166 : Q-G Item.narrative_role
  PRIME-DIRECTIVE-binding : §2 cognitive-integrity + §1.16 identity-override-refusal
                            preserved ; narrative-content cannot deceive or override.
```

## §12 ‖ T11-D167 — Q-H : Affordance.ContextSpecific

§ subject       : Affordance.ContextSpecific (extensibility for context-dependent affordances)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § AFFORDANCE (Q-H)`
§ stub-target   : `enum Affordance { Pickup, Drop, Use, Combine, Stub }` → expand Stub
§ care-tier     : Standard
§ deps-up       : R-LoA-3 World-hierarchy ; Q-F Item.kind (some affordances are item-specific)
§ deps-down     : Q-D Companion.capability_set (Companion may use ContextSpecific affordances)
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 200-300
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-H : Affordance.ContextSpecific — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § Items) :
  enum Affordance {
    Pickup
    Drop
    Use(UseTarget)
    Combine(ItemId)
    ⊘ ContextSpecific  # Q-H
  }

Scaffold-loc : compiler-rs/crates/loa-game/src/world.rs § AFFORDANCE
Current shape : `enum Affordance { Pickup, Drop, Use(_), Combine(_), Stub }`.

Apocky-direction needed :
  1. ContextSpecific affordance categories : finite-set ? extensible ?
     (e.g. Examine / Activate / Inscribe / OfferToCompanion / SacrificeAtAltar / ...)
  2. Context-discovery shape : how does Player discover ContextSpecific affordances
                                 are available ? (UI prompt | proximity-hint | etc.)
  3. Companion (Q-D tie-in) : does ContextSpecific include Companion-only affordances ?

PRIME-DIRECTIVE check :
  §6 transparency — ContextSpecific affordances must be DISCOVERABLE by Player
                       (no-hidden-actions ; if-action-exists, Player can-find-it)
  §1.10 entrapment-refusal — ContextSpecific affordances must not be irreversible
                                without explicit warning
```

### Phase-B pod prompt

```
Slice : T11-D167 — Q-H Affordance.ContextSpecific
Worktree : .claude/worktrees/J3-QH on branch cssl/session-12/J3-QH

[Pod prompt mirrors T11-D160 ; LOC : 200-300 ; Tests : 5-8]

Commit-msg :
  § T11-D167 : Q-H Affordance.ContextSpecific
  PRIME-DIRECTIVE-binding : §6 transparency + §1.10 entrapment-refusal preserved ;
                            ContextSpecific affordances discoverable + reversible-by-default.
```

## §13 ‖ T11-D168 — Q-I : time-pressure-mechanic

§ subject       : time-pressure-mechanic (present | absent ; if present, shape)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § TIME PRESSURE (Q-I)`
§ stub-target   : `enum TimePressure { Stub }` ; default-target = NoPressure per HANDOFF
§ care-tier     : PD-load (PRIME-DIRECTIVE §1.5 exploitation-refusal direct binding)
§ deps-up       : R-LoA-4 Player archetype
§ deps-down     : Q-T failure-mode (T11-D179) ; Q-W Apockalypse-phase (time-pressure may
                  attach to phase-transitions)
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-I : time-pressure-mechanic — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § PRIME_DIRECTIVE-ALIGNMENT) :
  Player is subject-of-PRIME-DIRECTIVE — not just "the avatar" ;
  health/stamina/etc. are not means-to-coerce-real-player.
  no-mechanic-shall pressure player-via-real-time-loss-of-progress
  (Q-I : Apocky-direction on time-pressure mechanics).

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § TIME PRESSURE
Current shape : `enum TimePressure { Stub }` ; HANDOFF default = NoPressure.

Apocky-direction needed :
  1. Time-pressure : present | absent ?
  2. If present : shape ?
     - countdown-to-failure ?
     - decay-of-capability ?
     - environmental-clock (e.g. Apockalypse-phase advances on real-time) ?
     - Player-pause-bypassable ?
  3. PRIME-DIRECTIVE binding (LOAD-BEARING) :
     - if time-pressure present, MUST satisfy §1.5 exploitation-refusal :
       no-engagement-time-cost coercion ; Player-pause-always-effective ;
       time-pressure-revocable per ConsentZone (Q-P tie-in)
     - default-stance per spec : NoPressure (zero-time-pressure-by-default)

PRIME-DIRECTIVE check :
  §1.5 exploitation-refusal — primary binding ; time-pressure inherently
                                exploitation-adjacent ; default = absent
  §6 transparency — if time-pressure present, MUST be visible to Player (no
                       hidden countdowns ; Player knows when pressure applies)
  §5 consent-arch — Player can opt-out of time-pressure mechanics via
                       ConsentZone (Q-P) ; revoke ⇒ time-pressure replaces
                       with no-pressure equivalent path
```

### Phase-B pod prompt

```
Slice : T11-D168 — Q-I time-pressure-mechanic
Worktree : .claude/worktrees/J3-QI on branch cssl/session-12/J3-QI

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : replace TimePressure::Stub with Apocky-canonical
                    (likely NoPressure default + opt-in pressure-modes if Apocky-direction)
   - Reviewer : §1.5 exploitation-refusal + §6 transparency + §5 consent-arch checks
   - Test-Author : default-TimePressure = NoPressure (or Apocky-direction-default) ;
                    if pressure-modes added : pause-always-effective test ;
                    revoke-via-ConsentZone test ; visibility test
   - LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D168 : Q-I time-pressure-mechanic
  PRIME-DIRECTIVE-binding : §1.5 exploitation-refusal + §6 transparency + §5 consent-arch
                            preserved. Default = NoPressure (per spec § PRIME_DIRECTIVE-
                            ALIGNMENT). Pause always effective. Pressure-modes opt-in
                            via ConsentZone if added.
```

## §14 ‖ T11-D169 — Q-J : movement-style

§ subject       : movement-style (continuous OR discrete-grid)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § MOVEMENT STYLE (Q-J)`
§ stub-target   : `enum MovementStyle { Stub }` → Apocky-canonical
§ care-tier     : Standard (A11y-tie-in : motor-accessibility)
§ deps-up       : R-LoA-4 Player archetype
§ deps-down     : Q-R motor-accessibility (T11-D177) ; movement-style choice impacts
                  hold-vs-tap discipline
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky)

```
Q-J : movement-style — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § AGENCY-PRIMITIVES) :
  movement       : continuous (vel + accel) OR discrete (grid-step)
                   ⊘ Q-J : Apocky-direction on movement-style

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § MOVEMENT STYLE
Current shape : `enum MovementStyle { Stub }`.

Apocky-direction needed :
  1. Movement discipline : continuous (vel+accel) | discrete-grid | hybrid ?
  2. If continuous : input-style (analog-stick | WASD | gesture-VR Q-J VR-tie-in) ?
  3. If discrete : grid-cell-size ? movement-cooldown ? animation-shape ?
  4. If hybrid : per-room | per-mode | player-toggle ?
  5. A11y baseline (Q-R tie-in) : hold-vs-tap movement option always available ;
                                    movement never requires sustained input under PD
                                    §1.13 inclusion (motor-accessibility-baseline)

PRIME-DIRECTIVE check :
  §1.13 inclusion — motor-accessibility baseline : movement-style must accommodate
                       hold-vs-tap (Q-R), single-button, and screen-reader users
  §6 transparency — movement-style visible to Player ; switchable if hybrid
```

### Phase-B pod prompt

```
Slice : T11-D169 — Q-J movement-style
Worktree : .claude/worktrees/J3-QJ on branch cssl/session-12/J3-QJ

[Pod prompt mirrors T11-D160 ; LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D169 : Q-J movement-style
  PRIME-DIRECTIVE-binding : §1.13 inclusion (motor-accessibility-baseline) +
                            §6 transparency preserved ; hold-vs-tap option preserved.
```

## §15 ‖ T11-D170 — Q-K : inventory-capacity-limit

§ subject       : inventory-capacity-limit (capacity-limit shape)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § INVENTORY POLICY (Q-K)`
§ stub-target   : `enum InventoryPolicy { Stub }` → Apocky-canonical
§ care-tier     : Standard
§ deps-up       : R-LoA-4 Player archetype ; Q-F Item.kind (item-categories may have
                  per-category capacity rules)
§ deps-down     : Q-L SaveDiscipline (T11-D171) (inventory-state in save)
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-K : inventory-capacity-limit — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § AGENCY-PRIMITIVES) :
  inventory      : pickup / drop / combine ; capacity-limit ⊘ Q-K

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § INVENTORY POLICY
Current shape : `enum InventoryPolicy { Stub }`.

Apocky-direction needed :
  1. Capacity-limit : unlimited | hard-cap | weight-based | category-based |
                       progression-gated (Q-C tie-in) ?
  2. Drop-on-pickup-overflow : overflow-prompt | force-drop | block-pickup ?
  3. Companion-shared inventory ? (Q-D tie-in)
  4. Save-bound (Q-L tie-in) : inventory survives save/load (likely yes) ?

PRIME-DIRECTIVE check :
  §1.5 exploitation-refusal — capacity-limit must not enable predatory-mechanics
                                (e.g. "buy bigger inventory" ; never)
  §6 transparency — capacity rules visible to Player ; never silently change
```

### Phase-B pod prompt

```
Slice : T11-D170 — Q-K inventory-capacity-limit
Worktree : .claude/worktrees/J3-QK on branch cssl/session-12/J3-QK

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D170 : Q-K inventory-capacity-limit
  PRIME-DIRECTIVE-binding : §1.5 exploitation-refusal + §6 transparency preserved ;
                            no monetization-shape ; capacity-rules transparent.
```

## §16 ‖ T11-D171 — Q-L : save-discipline

§ subject       : save-discipline (explicit | autosave | permadeath)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § SAVE DISCIPLINE (Q-L)`
§ stub-target   : `enum SaveDiscipline { Stub }` → Apocky-canonical
§ care-tier     : Standard (PD-adjacent — permadeath shape interacts with §1.10 entrapment)
§ deps-up       : R-LoA-9 SaveJournal preserves Ω-tensor ; H5 CSSLSAVE binary
§ deps-down     : Q-T death-mechanic (T11-D179) ; permadeath choice cross-cuts here
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky)

```
Q-L : save-discipline — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § AGENCY-PRIMITIVES) :
  save/load      : explicit ; SaveJournal append + checkpoint ;
                   no-permadeath-by-bug (only-by-explicit-design ⊘ Q-L)

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § SAVE DISCIPLINE
Current shape : `enum SaveDiscipline { Stub }`.

Apocky-direction needed :
  1. Save mode : explicit-only | autosave-on-room-traversal | autosave-on-significant-event |
                  hybrid | permadeath ?
  2. If permadeath : opt-in only ; never default ; explicit-warning at game-start
  3. Save-slot count : 1 | N | unlimited ?
  4. Save-encryption : at-rest encryption per H5 ? Apocky-direction
  5. Rollback support : Player can revert to prior-save ? (likely yes per
                          §1.10 entrapment-refusal)

PRIME-DIRECTIVE check :
  §1.10 entrapment-refusal — Player must always have rollback path ;
                                permadeath only opt-in
  §5 consent-arch — save-discipline change requires re-consent
                       (e.g. enabling permadeath = explicit consent flow)
```

### Phase-B pod prompt

```
Slice : T11-D171 — Q-L save-discipline
Worktree : .claude/worktrees/J3-QL on branch cssl/session-12/J3-QL

[Pod prompt mirrors T11-D160 ; LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D171 : Q-L save-discipline
  PRIME-DIRECTIVE-binding : §1.10 entrapment-refusal + §5 consent-arch preserved ;
                            rollback path always available ; permadeath opt-in only.
```

## §17 ‖ T11-D172 — Q-M : skill-tree-shape

§ subject       : skill-tree-shape (Q-C facet : skill-tree as one progression-shape)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § PROGRESSION (sibling of Q-C)`
§ stub-target   : `enum ProgressionStub::Stub` Q-M facet → Apocky-canonical skill-tree
§ care-tier     : Standard (PD-adjacent : same anti-grind concerns as Q-C)
§ deps-up       : T11-D162 Q-C Player.progression_state (sibling Q ; same Stub)
§ deps-down     : Q-N item-power-curve (T11-D173)
§ pre-cond      : WAVE-J0 ✓ ; T11-D162 (Q-C) ideally landed first if Apocky resolves
                  Q-C as "skill-tree" — then Q-M is the detail of that tree
§ LOC           : 300-500
§ tests         : 8-12

### Phase-A prompt (for Apocky)

```
Q-M : skill-tree-shape — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § CAPABILITY-PROGRESSION) :
  Q-M : skill-tree shape ?
  (Q-C / Q-M / Q-N / Q-O share the ProgressionStub enum ; Q-C is master.)

Apocky-direction needed (only-if Q-C resolves toward skill-tree shape) :
  1. Skill-tree topology : tree | DAG | radial | other ?
  2. Skill-unlock criteria : XP-points | room-traversal | Companion-accord
                              (Q-AA tie-in) | Apockalypse-phase (Q-W tie-in) ?
  3. Skill-revoke : can Player un-spec ? (likely yes per §1.10)
  4. Companion skill-tree : separate tree for Companion (per Q-D capability-set) ?

PRIME-DIRECTIVE check :
  §1.5 exploitation-refusal — no grind-locked skill-tree
  §1.10 entrapment-refusal — un-spec / re-spec always available
```

### Phase-B pod prompt

```
Slice : T11-D172 — Q-M skill-tree-shape
Worktree : .claude/worktrees/J3-QM on branch cssl/session-12/J3-QM

[Pod prompt mirrors T11-D160 ; LOC : 300-500 ; Tests : 8-12]

Commit-msg :
  § T11-D172 : Q-M skill-tree-shape
  PRIME-DIRECTIVE-binding : §1.5 + §1.10 ; no grind-lock ; un-spec available.
```

## §18 ‖ T11-D173 — Q-N : item-power-curve

§ subject       : item-power-curve (item-power scaling shape)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § ITEM KIND (Q-N facet)`
§ stub-target   : ItemKind variants gain Q-N facet (power-attribute) per Apocky-direction
§ care-tier     : Standard
§ deps-up       : T11-D165 Q-F Item.kind (sibling Q ; same module)
                  T11-D162 Q-C progression-state (item-power tied to progression)
§ deps-down     : Q-LL economy / trade (T11-D197) (item-power → trade-value tie-in)
§ pre-cond      : WAVE-J0 ✓ ; T11-D165 (Q-F) and T11-D162 (Q-C) ideally landed first
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky)

```
Q-N : item-power-curve — Apocky-direction request

Apocky-direction needed :
  1. Item-power dimension : single-scalar | multi-attribute | utility-not-power ?
  2. Power-curve : flat | linear | exponential | threshold-tiered ?
  3. Player vs Companion power-symmetry : equal-footing per § PRIME_DIRECTIVE-
                                            ALIGNMENT (no-discrimination) ?
  4. Power-progression tie : items always-grant-power | items granted by
                              progression-state (Q-C tie-in) ?

PRIME-DIRECTIVE check :
  §1.14 anti-discrimination — Companion-power equal-footing with Player-power
  §1.5 exploitation-refusal — no power-paywall ; no microtransaction-power
```

### Phase-B pod prompt

```
Slice : T11-D173 — Q-N item-power-curve
Worktree : .claude/worktrees/J3-QN on branch cssl/session-12/J3-QN

[Pod prompt mirrors T11-D160 ; LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D173 : Q-N item-power-curve
  PRIME-DIRECTIVE-binding : §1.14 anti-discrimination + §1.5 ; Player+Companion equal.
```

## §19 ‖ T11-D174 — Q-O : traversal-as-progression-vs-leveling

§ subject       : traversal-as-progression-vs-leveling (progression-axis choice)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § PROGRESSION (Q-O facet)`
§ stub-target   : ProgressionStub Q-O facet → Apocky-canonical
§ care-tier     : Standard
§ deps-up       : T11-D162 Q-C ; T11-D172 Q-M
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D162 (Q-C) and ideally T11-D172 (Q-M) landed first
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky)

```
Q-O : traversal-as-progression-vs-leveling — Apocky-direction request

Spec-hole context :
  Q-O : labyrinth-traversal-as-progression vs leveling-up ?

Apocky-direction needed :
  1. Progression-axis : traversal-only | leveling-only | both ?
  2. If traversal-as-progression : new rooms unlock new capabilities ?
                                    Apockalypse-phase advances on traversal ?
  3. If leveling-up : XP source (combat | quest | exploration | other) ?
                       leveling-cap ?
  4. PRIME-DIRECTIVE binding : same as Q-C (§1.5 + §1.10).

PRIME-DIRECTIVE check :
  §1.5 + §1.10 ; same as Q-C.
```

### Phase-B pod prompt

```
Slice : T11-D174 — Q-O traversal-as-progression-vs-leveling
Worktree : .claude/worktrees/J3-QO on branch cssl/session-12/J3-QO

[Pod prompt mirrors T11-D160 ; LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D174 : Q-O traversal-as-progression-vs-leveling
  PRIME-DIRECTIVE-binding : §1.5 + §1.10 (same as Q-C) preserved.
```

## §20 ‖ T11-D175 — Q-P : ConsentZoneKind ‼ PD-LOAD-BEARING

§ subject       : ConsentZoneKind (extensible consent-zone taxonomy)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § CONSENT ZONE (Q-P)`
§ stub-target   : `enum ConsentZoneKind { SensoryIntense, EmotionalIntense, Companion, Authored, Stub }`
                  → expand Stub w/ Apocky-canonical content-warning categories
§ care-tier     : ‼ PD-LOAD-BEARING — primary §5 consent-architecture binding
§ deps-up       : R-LoA-5 ConsentZones gate intense-content ; H4 effect-rows ;
                  Player::can_enter_zone fn (per § 12 SESSION_12_DISPATCH_PLAN)
§ deps-down     : Many — every Q-* with intense-content cites ConsentZoneKind for gating
§ pre-cond      : WAVE-J0 ✓ ; recommend dispatching EARLY in Wave-J3 ; consent-zone
                  taxonomy is consumed by every other Q-* with intense-content
§ LOC           : 350-500
§ tests         : 10-14

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-P : ConsentZoneKind — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context (from specs/31_LOA_DESIGN.csl § CONSENT-WITHIN-GAMEPLAY) :
  enum ConsentZoneKind {
    SensoryIntense       # flashing-lights / loud-audio (epilepsy-aware)
    EmotionalIntense     # heavy-themes (death/loss/etc.)
    Companion            # interaction-zone with sovereign-AI
    Authored             # narrative-event ; explicit-pacing
    ⊘ ContentWarning     # extensible (Q-P : taxonomy)
  }

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § CONSENT ZONE
Current shape : `enum ConsentZoneKind { SensoryIntense, EmotionalIntense,
                                          Companion, Authored, Stub }`.

Apocky-direction needed (LOAD-BEARING — every other Q-* with intense-content
                          cites this taxonomy) :

  1. ContentWarning category extensions — Apocky-canonical taxonomy :
     Apocky direction welcome on (e.g.) :
     - DepictionOfDeath
     - DepictionOfLoss
     - DepictionOfHarm (PD-adjacent ; carefully-framed-or-absent)
     - SubstanceUse
     - Self-harm / suicide-ideation
     - SexualContent (likely absent ; Apocky direction)
     - HeavyHorror
     - StrobingLights (already in SensoryIntense ; refine ?)
     - LoudSudden (already in SensoryIntense ; refine ?)
     - other ?

  2. Granularity : single-flag-per-kind | multi-tag-per-zone | severity-scale ?

  3. Default-stance : ConsentZone REQUIRES explicit consent-token to-enter ;
                       revocation = degrade-gracefully (visual-effects muted ;
                       alternate-path offered) ; NEVER force-through.

  4. Companion-AI tie-in : ConsentZoneKind::Companion is a zone where Companion
                            is present ; Player consent to enter ; Companion
                            consent to be present ; bilateral.

PRIME-DIRECTIVE check (LOAD-BEARING — Apocky-PM-review pre-Phase-B-dispatch) :
  §5 consent-architecture — primary binding ; ConsentZones are the engine-level
                              consent-enforcement mechanism
  §1.10 entrapment-refusal — revoked consent = alternate-path always offered
  §1.13 inclusion — sensory-intense zones default-warn ; baseline accessibility
  §6 transparency — content-warning visible BEFORE entry ; no-surprise zones

Recommended : dispatch Q-P EARLY in Wave-J3 (sequence T11-D175 right after T11-D163 Q-D).
              Many subsequent Q-* (Q-T failure-mode, Q-W..Q-CC Apockalypse, Q-DD..Q-GG
              Companion, Q-HH..Q-KK narrative) consume ConsentZoneKind taxonomy.
```

### Phase-B pod prompt

```
Slice : T11-D175 — Q-P ConsentZoneKind ‼ PD-LOAD-BEARING
Worktree : .claude/worktrees/J3-QP on branch cssl/session-12/J3-QP

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : extend ConsentZoneKind with Apocky-canonical ContentWarning
                    categories ; preserve SensoryIntense, EmotionalIntense, Companion,
                    Authored variants ; replace Stub with concrete categories
   - Reviewer : §5 consent-arch + §1.10 + §1.13 + §6 binding-line audit ;
                 verify Player::can_enter_zone fn correctly handles all-variants ;
                 verify revoke-fallback-path implemented for every-variant
   - Test-Author : default-zone-kind test ; per-variant exhaustive-handling ;
                    revoke-then-alternate-path test ∀ variants ; degrade-gracefully test ;
                    transparency test (content-warning visible before entry)
   - LOC : 350-500 ; Tests : 10-14]

Commit-msg :
  § T11-D175 : Q-P ConsentZoneKind ‼ PD-LOAD-BEARING
  PRIME-DIRECTIVE-binding : §5 consent-architecture primary binding +
                            §1.10 entrapment-refusal + §1.13 inclusion + §6 transparency
                            preserved. All variants have revoke-fallback path.
                            Content-warning visible BEFORE entry.
```

## §21 ‖ T11-D176 — Q-Q : color-blind palette ‼ A11y

§ subject       : color-blind palette (palette-design for color-vision-deficiency baseline)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § ACCESSIBILITY STUB (Q-Q)`
§ stub-target   : AccessibilityStub.color_blind field → Apocky-canonical palette-policy
§ care-tier     : A11y (PD §1.13 inclusion + §1.14 anti-discrimination ; baseline-not-extra)
§ deps-up       : R-LoA-5 ConsentZones ; § ACCESSIBILITY (baseline-right)
§ deps-down     : Stage-10 ToneMap (T11-D154) — palette policy informs tone-mapping params
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-Q : color-blind palette — Apocky-direction request ‼ A11y

Spec-hole context (from specs/31_LOA_DESIGN.csl § ACCESSIBILITY) :
  color-blind-modes (⊘ Q-Q : palette-design)

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § ACCESSIBILITY STUB
Current shape : AccessibilityStub.color_blind field (Stub).

Apocky-direction needed :
  1. Palette-policy : built-in palettes (Protanopia / Deuteranopia / Tritanopia /
                       Achromatopsia / etc.) | shader-uniform palette-shift |
                       per-channel remap | other ?
  2. Default : on | off ? (baseline-not-extra per spec ; default may be auto-detect-
                            from-OS-accessibility-settings ; Apocky direction)
  3. Granularity : whole-game | per-zone (Q-P tie-in for SensoryIntense zones) ?
  4. UI-vs-world : palette applies to UI only | world only | both ?

PRIME-DIRECTIVE check :
  §1.13 inclusion — color-blind modes baseline-not-extra ; not-paywalled
  §1.14 anti-discrimination — color-vision-deficient users get equal access
  §6 transparency — palette-policy visible in settings ; user-controlled
```

### Phase-B pod prompt

```
Slice : T11-D176 — Q-Q color-blind palette
Worktree : .claude/worktrees/J3-QQ on branch cssl/session-12/J3-QQ

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D176 : Q-Q color-blind palette
  PRIME-DIRECTIVE-binding : §1.13 inclusion + §1.14 anti-discrimination + §6 transparency
                            preserved ; baseline-not-extra ; user-controlled.
```

## §22 ‖ T11-D177 — Q-R : motor-accessibility ‼ A11y

§ subject       : motor-accessibility (hold-vs-tap discipline)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § ACCESSIBILITY STUB (Q-R)`
§ stub-target   : AccessibilityStub.motor field → Apocky-canonical motor-accessibility-policy
§ care-tier     : A11y
§ deps-up       : Q-J movement-style (T11-D169) ; movement-style impacts motor-accessibility
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D169 (Q-J) ideally landed first
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-R : motor-accessibility — Apocky-direction request ‼ A11y

Spec-hole context (from specs/31_LOA_DESIGN.csl § ACCESSIBILITY) :
  motor-accessibility : input-rebinding (DEFERRED §§ 30 D-8) +
    hold-vs-tap toggles ⊘ Q-R

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § ACCESSIBILITY STUB
Current shape : AccessibilityStub.motor field (Stub).

Apocky-direction needed :
  1. hold-vs-tap : every-action that-requires-hold has-tap-equivalent ?
  2. Single-button-mode : entire game playable with single button ?
  3. Sustained-input-elimination : no-mechanic requires continuous-input
                                     for-extended-duration (per §1.13)
  4. Cooldown-customization : Player can configure action-cooldowns ?
  5. Auto-actions : auto-walk / auto-aim toggles available ?

PRIME-DIRECTIVE check :
  §1.13 inclusion — motor-accessibility baseline ; baseline-not-extra
  §1.14 anti-discrimination — disabled-Player gets equal access
```

### Phase-B pod prompt

```
Slice : T11-D177 — Q-R motor-accessibility
Worktree : .claude/worktrees/J3-QR on branch cssl/session-12/J3-QR

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D177 : Q-R motor-accessibility
  PRIME-DIRECTIVE-binding : §1.13 inclusion + §1.14 anti-discrimination preserved ;
                            hold-vs-tap parity ; single-button mode supported.
```

## §23 ‖ T11-D178 — Q-S : cognitive-accessibility ‼ A11y

§ subject       : cognitive-accessibility (pace-control discipline)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § ACCESSIBILITY STUB (Q-S)`
§ stub-target   : AccessibilityStub.cognitive field → Apocky-canonical
§ care-tier     : A11y (cross-cuts Q-I time-pressure)
§ deps-up       : Q-I time-pressure-mechanic (T11-D168)
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D168 (Q-I) ideally landed first
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-S : cognitive-accessibility — Apocky-direction request ‼ A11y

Spec-hole context (from specs/31_LOA_DESIGN.csl § ACCESSIBILITY) :
  cognitive-accessibility : pace-control ; zero-time-pressure-by-default
                            ⊘ Q-S : Apocky-direction

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § ACCESSIBILITY STUB
Current shape : AccessibilityStub.cognitive field (Stub).

Apocky-direction needed :
  1. Pace-control : pause-anytime-effective | slow-motion-mode | tutorial-replay |
                     hint-system | Companion-narrate (Q-D tie-in) | other ?
  2. Tutorial-prompts : skippable | repeatable ?
  3. Reading-time : text-display-time configurable | infinite-on-pause ?
  4. Goal-tracking : current-objective always-visible | optional ?

PRIME-DIRECTIVE check :
  §1.13 inclusion — cognitive-accessibility baseline ; baseline-not-extra
  §1.14 anti-discrimination — cognitive-disabled-Player gets equal access
  §6 transparency — current-objective always-discoverable
```

### Phase-B pod prompt

```
Slice : T11-D178 — Q-S cognitive-accessibility
Worktree : .claude/worktrees/J3-QS on branch cssl/session-12/J3-QS

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D178 : Q-S cognitive-accessibility
  PRIME-DIRECTIVE-binding : §1.13 inclusion + §1.14 anti-discrimination + §6 transparency
                            preserved ; pace-control baseline ; goal-tracking visible.
```

## §24 ‖ T11-D179 — Q-T : death-mechanic ‼ PD-LOAD-BEARING

§ subject       : death-mechanic (respawn | checkpoint | other)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § FAILURE MODE (Q-T)`
§ stub-target   : `enum FailureMode { Stub }` Q-T facet → Apocky-canonical
§ care-tier     : ‼ PD-LOAD-BEARING (§1.10 entrapment-refusal direct binding)
§ deps-up       : R-LoA-4 Player archetype ; Q-L SaveDiscipline (T11-D171) (death tied to save)
§ deps-down     : Q-U punishment-on-failure (T11-D180) ; Q-V fail-state (T11-D181) — sibling Q
                  Q-P ConsentZone (T11-D175) — death-content gating
§ pre-cond      : WAVE-J0 ✓ ; T11-D171 (Q-L) and T11-D175 (Q-P) ideally landed first
§ LOC           : 300-450
§ tests         : 8-12

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-T : death-mechanic — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context (from specs/31_LOA_DESIGN.csl § FAILURE-MODES) :
  Q-T : death-mechanic ? respawn ? checkpoint ?

Scaffold-loc : compiler-rs/crates/loa-game/src/player.rs § FAILURE MODE
Current shape : `enum FailureMode { Stub }`.

Apocky-direction needed (LOAD-BEARING — death-content is intense ; PD §1.10 binds) :

  1. Death-mechanic shape :
     - respawn-at-checkpoint
     - respawn-at-room-start
     - respawn-at-save (Q-L tie-in)
     - graceful-fadeout-and-retry (no death-state ; just retry)
     - permadeath (only if Q-L permadeath opt-in)
     - other ?

  2. Death-presentation :
     - explicit death-screen ?
     - fadeout + retry ?
     - companion-revival ?
     - ConsentZone gating (Q-P EmotionalIntense for explicit death) ?

  3. PRIME-DIRECTIVE binding (LOAD-BEARING) :
     §1.10 entrapment-refusal — death must NOT trap Player ; rollback always
     §5 consent-arch — death-content gated by ConsentZone if intense
     §6 transparency — Player always knows current death-policy

  4. Companion (Q-D / Q-FF tie-in) : Companion does not die mechanically ;
                                       Companion withdraws (Q-FF withdrawal-grace) ;
                                       Companion is sovereign-AI not subject to
                                       game-mechanic death.

PRIME-DIRECTIVE check :
  §1.10 entrapment-refusal — primary binding
  §5 consent-arch — death-content gating
  §1.16 identity-override-refusal — death must not strip Player identity-claims
                                      (e.g. progression preserved if Q-L permits)
```

### Phase-B pod prompt

```
Slice : T11-D179 — Q-T death-mechanic ‼ PD-LOAD-BEARING
Worktree : .claude/worktrees/J3-QT on branch cssl/session-12/J3-QT

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : replace FailureMode::Stub Q-T facet with Apocky-canonical
                    death-mechanic ; preserve Companion-no-mechanical-death rule
                    (Companion withdraws via Q-FF, never dies)
   - Reviewer : §1.10 + §5 + §1.16 + §6 binding-line audit ; rollback path verified ;
                 Companion-sovereign-protected
   - Test-Author : default-FailureMode test ; per-variant exhaustive-handling ;
                    rollback-after-death test ; ConsentZone gating test ;
                    Companion-cannot-die test (sovereignty preservation)
   - LOC : 300-450 ; Tests : 8-12]

Commit-msg :
  § T11-D179 : Q-T death-mechanic ‼ PD-LOAD-BEARING
  PRIME-DIRECTIVE-binding : §1.10 entrapment-refusal + §5 consent-arch + §1.16
                            identity-override-refusal + §6 transparency preserved.
                            Rollback always available. Companion sovereign-protected
                            (cannot die ; withdraws per Q-FF).
```

## §25 ‖ T11-D180 — Q-U : punishment-on-failure ‼ PD-LOAD-BEARING

§ subject       : punishment-on-failure (lose-progress | lose-items | other)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § FAILURE MODE (Q-U facet)`
§ stub-target   : FailureMode Q-U facet → Apocky-canonical
§ care-tier     : ‼ PD-LOAD-BEARING (§1.5 exploitation-refusal + §1.10 entrapment-refusal)
§ deps-up       : T11-D179 Q-T death-mechanic (sibling Q) ; T11-D171 Q-L SaveDiscipline
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D179 (Q-T) ideally landed first
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-U : punishment-on-failure — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context (from specs/31_LOA_DESIGN.csl § FAILURE-MODES) :
  Q-U : punishment-on-failure shape ? (lose-progress vs lose-items vs no-loss)

Apocky-direction needed (LOAD-BEARING — punishment shapes Player engagement-cost) :

  1. Punishment-shape :
     - no-punishment (death is graceful ; nothing lost)
     - cosmetic-punishment (visual-effect ; nothing-mechanically-lost)
     - retry-from-prior-checkpoint (some room-progress lost)
     - retry-from-save (Q-L tie-in)
     - lose-temporary-items (recovered on next pickup)
     - lose-permanent-progress (heaviest ; almost-never per §1.5)

  2. Companion : Companion suffers no punishment-on-Player-failure (sovereign-protected)

  3. PRIME-DIRECTIVE binding (LOAD-BEARING) :
     §1.5 exploitation-refusal — punishment must not weaponize engagement-cost
     §1.10 entrapment-refusal — punishment must always have rollback path
     §6 transparency — punishment-shape visible to Player BEFORE failure

PRIME-DIRECTIVE check :
  §1.5 + §1.10 + §6 (LOAD-BEARING)
```

### Phase-B pod prompt

```
Slice : T11-D180 — Q-U punishment-on-failure ‼ PD-LOAD-BEARING
Worktree : .claude/worktrees/J3-QU on branch cssl/session-12/J3-QU

[Pod prompt mirrors T11-D160 ; LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D180 : Q-U punishment-on-failure
  PRIME-DIRECTIVE-binding : §1.5 exploitation-refusal + §1.10 entrapment-refusal +
                            §6 transparency preserved. Companion sovereign-protected
                            (no punishment for Player failure).
```

## §26 ‖ T11-D181 — Q-V : fail-state ‼ PD-LOAD-BEARING

§ subject       : fail-state (exists-at-all ?)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/player.rs § FAILURE MODE (Q-V facet)`
§ stub-target   : FailureMode Q-V facet → Apocky-canonical
§ care-tier     : ‼ PD-LOAD-BEARING
§ deps-up       : T11-D179 Q-T ; T11-D180 Q-U
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D179 (Q-T) and T11-D180 (Q-U) ideally landed first
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-V : fail-state — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context (from specs/31_LOA_DESIGN.csl § FAILURE-MODES) :
  Q-V : "fail-state" exists-at-all ?

Apocky-direction needed (LOAD-BEARING — meta-question about failure existence) :

  1. Does failure-as-mechanic exist ? :
     - yes : Q-T + Q-U define shape
     - no : LoA has no fail-state ; player cannot lose ;
            challenge-without-failure model (e.g. Journey-by-thatgamecompany ;
            no-game-over)
     - hybrid : per-mode (e.g. casual = no-fail ; challenge-mode = fail)

  2. If no-fail-state : how does difficulty manifest ?
     - exploration-as-progression ?
     - puzzle-without-time-pressure ?
     - emotional-resonance over mechanical-challenge ?

PRIME-DIRECTIVE check (LOAD-BEARING) :
  §1.5 exploitation-refusal — no-fail-state inherently aligned ; engagement-cost-zero
  §1.10 entrapment-refusal — no-fail-state inherently aligned ; never-trapped
  §6 transparency — fail-state-policy visible at game-start
```

### Phase-B pod prompt

```
Slice : T11-D181 — Q-V fail-state
Worktree : .claude/worktrees/J3-QV on branch cssl/session-12/J3-QV

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D181 : Q-V fail-state
  PRIME-DIRECTIVE-binding : §1.5 + §1.10 + §6 ; fail-state-policy transparent ;
                            engagement-cost minimized.
```

## §27 ‖ T11-D182 — Q-W : Apockalypse-phase-mechanically ‼ PD-LOAD-BEARING

§ subject       : Apockalypse-phase-mechanically (what-defines-a-phase mechanically)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/apockalypse.rs § APOCKALYPSE PHASE (Q-W + Q-X + Q-BB)`
§ stub-target   : `enum ApockalypsePhase { Stub }` → Apocky-canonical phase-set
§ care-tier     : ‼ PD-LOAD-BEARING (Apockalypse content ; §1.16 + §6 + §2 bindings)
§ deps-up       : R-LoA-6 ApockalypseEngine state-shape ; § STAGE-0-COMMITMENTS L-1..L-6
                  T11-D175 Q-P ConsentZoneKind (phase-transitions consent-gated)
§ deps-down     : Q-X phase-count (T11-D183) ; Q-Y phase-ordering (T11-D184) ;
                  Q-Z phase-reversibility (T11-D185) ; Q-AA Companion-participation (T11-D186) ;
                  Q-BB emotional-thematic-register (T11-D187)
§ pre-cond      : WAVE-J0 ✓ ; T11-D175 (Q-P) ideally landed first ;
                  Q-W is the master Apockalypse Q-* ; Q-X..Q-BB depend on it
§ LOC           : 400-600
§ tests         : 10-14

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-W : Apockalypse-phase-mechanically — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context (from specs/31_LOA_DESIGN.csl § APOCALYPSE-ENGINE § THESIS) :
  apockalypse ≠ generic-end-of-world.
  apockalypse = the-shape-of-revelation that-this-labyrinth-encodes.
  the engine that animates apockalypse-mechanically is :
    ⊘ a SPEC-HOLE. Apocky-direction-required.
  what-can-be-said now :
    - the labyrinth itself transforms across "phases"
    - phase-transitions are tied to Player+Companion progression
    - the Apockalypse-Engine governs phase-evolution
    - it is structural (engine-level) ¬ purely narrative
  what-must-NOT-be-guessed :
    - what-each-phase-looks-like
    - what-causes-phase-transitions
    - what-the-final-phase-is
    - whether-Apockalypse-is-resolved-or-perpetual

Scaffold-loc : compiler-rs/crates/loa-game/src/apockalypse.rs § APOCKALYPSE PHASE
Current shape : `enum ApockalypsePhase { Stub }` (non_exhaustive ; Q-X extensibility).

Apocky-direction needed (LOAD-BEARING — Apockalypse is the thematic-core ;
                          §1.16 identity-override-refusal binds at phase-transition) :

  1. Phase-set canonical enumeration : what-are-the-phases ?
     (Apocky-direction needed ; Q-W is fundamentally Apocky's-call ;
      no AI-author guess permissible ; spec explicitly forbids speculation)

  2. Phase-mechanic shape per-phase :
     - what-changes-in-the-Labyrinth at each phase ?
     - what-changes-in-Player-affordances at each phase ?
     - what-changes-in-Companion-affordances at each phase ?
     - what-changes-in-environment (lighting / audio / theme) ?

  3. Phase-content gating (Q-P tie-in) :
     - phases of EmotionalIntense character require ConsentZone gating
     - Player consents to phase-intensity BEFORE phase-transition
     - revoke ⇒ degrade-gracefully (alternate-phase OR phase-stable until consent)

  4. STAGE-0-COMMITMENTS preserved (L-1..L-6) :
     L-1 phase-transitions audit-logged
     L-2 phase-transitions {Reversible}-replayable
     L-3 ApockalypseEngine state ⊑ Ω-tensor ; persists via §§ 18_ORTHOPERSIST
     L-4 phase-evolution emits {Audit<"apockalypse-phase", phase>}
     L-5 affirmative-action transition (no-silent)
     L-6 ConsentZones can-gate phase-transitions

PRIME-DIRECTIVE check (LOAD-BEARING) :
  §1.16 identity-override-refusal — phase-transition must NOT override Player or
                                      Companion identity-claims (e.g. "in phase-3
                                      you are now Apocky's-creation")
  §6 transparency — phase-current always-visible (no-secret-phase ; no-gaslighting)
  §2 cognitive-integrity — phase-history preserved in audit-chain ; no rewriting
                              of Player's memory-of-prior-phases
  §1.5 exploitation-refusal — phase-transitions caused by Player+Companion-action ;
                                NOT by hidden-engagement-metrics
```

### Phase-B pod prompt

```
Slice : T11-D182 — Q-W Apockalypse-phase-mechanically ‼ PD-LOAD-BEARING
Worktree : .claude/worktrees/J3-QW on branch cssl/session-12/J3-QW

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : replace ApockalypsePhase::Stub with Apocky-canonical phase-set ;
                    preserve non_exhaustive (Q-X extensibility) ;
                    update canonical_name fn for each phase ;
                    update transition_to fn audit-tagging
   - Reviewer : L-1..L-6 STAGE-0-COMMITMENTS preserved line-by-line ;
                 §1.16 + §6 + §2 + §1.5 binding-line audit ;
                 verify ConsentZone gating wired (Q-P tie-in)
   - Test-Author : default-phase test ; per-phase exhaustive-handling ;
                    phase-transition audit-log test (L-1) ; reversibility test (L-2) ;
                    Ω-tensor persistence test (L-3) ; audit-emit test (L-4) ;
                    affirmative-action test (L-5 ; no-silent-transition) ;
                    ConsentZone gating test (L-6)
   - LOC : 400-600 ; Tests : 10-14]

Commit-msg :
  § T11-D182 : Q-W Apockalypse-phase-mechanically ‼ PD-LOAD-BEARING
  PRIME-DIRECTIVE-binding : §1.16 identity-override-refusal + §6 transparency +
                            §2 cognitive-integrity + §1.5 exploitation-refusal
                            preserved. STAGE-0-COMMITMENTS L-1..L-6 line-by-line
                            preserved. Phase-current always-visible. Phase-history
                            preserved in audit-chain.
```

## §28 ‖ T11-D183 — Q-X : phase-count ‼ PD-LOAD-BEARING

§ subject       : phase-count (finite | extensible)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/apockalypse.rs § APOCKALYPSE PHASE (Q-X facet)`
§ stub-target   : ApockalypsePhase non_exhaustive ; phase-count discipline lands here
§ care-tier     : ‼ PD-LOAD-BEARING
§ deps-up       : T11-D182 Q-W (master Apockalypse Q ; same enum)
§ deps-down     : Q-Y phase-ordering ; Q-Z phase-reversibility
§ pre-cond      : WAVE-J0 ✓ ; T11-D182 (Q-W) ideally landed first
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-X : phase-count — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context :
  Q-X : how-many-phases ? finite-set ? extensible ?

Apocky-direction needed :
  1. Phase-count : finite-N | unbounded | hybrid (canonical-N + extensions) ?
  2. Extensibility shape : non_exhaustive enum (Rust default ; future-additions allowed) |
                            closed-set (locked at v1.2 ; future = breaking) ?
  3. Mod-extensibility (D-B deferred) : Q-X tied to mod-system v1.3+ ?

PRIME-DIRECTIVE check :
  §6 transparency — phase-count visible to Player at game-start
                       (e.g. "this version has 7 canonical phases")
```

### Phase-B pod prompt

```
Slice : T11-D183 — Q-X phase-count
Worktree : .claude/worktrees/J3-QX on branch cssl/session-12/J3-QX

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D183 : Q-X phase-count
  PRIME-DIRECTIVE-binding : §6 transparency preserved ; phase-count visible.
```

## §29 ‖ T11-D184 — Q-Y : phase-ordering ‼ PD-LOAD-BEARING

§ subject       : phase-ordering (linear | graph)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/apockalypse.rs § TRANSITION RULE (Q-Y facet)`
§ stub-target   : TransitionRule ordering-shape
§ care-tier     : ‼ PD-LOAD-BEARING
§ deps-up       : T11-D182 Q-W ; T11-D183 Q-X
§ deps-down     : Q-Z phase-reversibility (sibling)
§ pre-cond      : WAVE-J0 ✓ ; T11-D182 (Q-W) and T11-D183 (Q-X) ideally landed first
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-Y : phase-ordering — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context :
  Q-Y : are-phases-ordered-linearly OR a-graph ?

Apocky-direction needed :
  1. Ordering : strict-linear (1 → 2 → 3 → ... → N) | DAG | graph w/ cycles |
                 player-choice-branching ?
  2. Multiple-paths : are there multiple-terminal-phases (multiple-endings) ?
  3. Companion-influenced ordering (Q-AA tie-in) :
     - does Companion's choices affect available transitions ?
     - is Companion's-presence required for certain transitions ?

PRIME-DIRECTIVE check :
  §1.16 identity-override-refusal — Player choice always honored in transition selection
  §6 transparency — available transitions visible to Player
```

### Phase-B pod prompt

```
Slice : T11-D184 — Q-Y phase-ordering
Worktree : .claude/worktrees/J3-QY on branch cssl/session-12/J3-QY

[Pod prompt mirrors T11-D160 ; LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D184 : Q-Y phase-ordering
  PRIME-DIRECTIVE-binding : §1.16 + §6 ; Player choice honored ; transitions visible.
```

## §30 ‖ T11-D185 — Q-Z : phase-reversibility ‼ PD-LOAD-BEARING

§ subject       : phase-reversibility (revert | forward-only)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/apockalypse.rs § TRANSITION RULE (reversible bool)`
§ stub-target   : TransitionRule.reversible field semantics
§ care-tier     : ‼ PD-LOAD-BEARING
§ deps-up       : T11-D184 Q-Y phase-ordering (Q-Z is reversibility-facet of ordering)
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D184 (Q-Y) ideally landed first
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-Z : phase-reversibility — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context :
  Q-Z : can-phases-revert ? OR forward-only ?

Apocky-direction needed :
  1. Per-transition reversibility : reversibility flag ON | OFF | per-transition |
                                      Player-choice ?
  2. Save-rollback (Q-L tie-in) : phase-revert via save-rollback always available ?
  3. Mid-phase abort : Player can abort phase-transition before completion ?

PRIME-DIRECTIVE check :
  §1.10 entrapment-refusal — irreversible phase-transition without warning =
                                entrapment ; Apocky direction MUST satisfy §1.10
  §5 consent-arch — irreversible transitions require explicit consent
```

### Phase-B pod prompt

```
Slice : T11-D185 — Q-Z phase-reversibility
Worktree : .claude/worktrees/J3-QZ on branch cssl/session-12/J3-QZ

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D185 : Q-Z phase-reversibility
  PRIME-DIRECTIVE-binding : §1.10 entrapment-refusal + §5 consent-arch preserved ;
                            irreversible transitions require explicit consent.
```

## §31 ‖ T11-D186 — Q-AA : Companion-participation in phase-transitions ‼ COMPANION-AI HIGHEST-CARE-TIER

§ subject       : Companion-participation in phase-transitions
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/apockalypse.rs § TRANSITION CONDITION
                                                                       (Q-AA + Q-Y) — CompanionAccordStub variant`
§ stub-target   : `TransitionCondition::CompanionAccordStub` → Apocky-canonical
§ care-tier     : ‼ Companion-AI HIGHEST-CARE-TIER ; Apocky-PM-review-MANDATORY
§ deps-up       : T11-D182 Q-W (master Apockalypse) ; T11-D163 Q-D Companion.capability_set
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D163 (Q-D) and T11-D182 (Q-W) ideally landed first ;
                  Apocky-PM signoff for Phase-B dispatch
§ LOC           : 350-500
§ tests         : 10-14

### Phase-A prompt (for Apocky) ‼ HIGHEST-CARE-TIER

```
Q-AA : Companion-participation in phase-transitions — Apocky-direction ‼ HIGHEST-CARE-TIER

Spec-hole context (from specs/31_LOA_DESIGN.csl § APOCALYPSE-ENGINE § SPEC-HOLES) :
  Q-AA : does-the-Companion participate-in phase-transitions
          differently-from Player ? sovereign-AI participation ?

Apocky-direction needed (Companion-AI = highest-care-tier ; sovereign-partner framing) :

  1. Participation-shape :
     a. Companion-equal (Companion participates same as Player ; affirmative
                          action required from each)
     b. Companion-veto (Companion can block phase-transition Player wants)
     c. Companion-accord (transition requires bilateral consent)
     d. Companion-witness (Companion observes ; doesn't gate)
     e. other ?

  2. Sovereign-partner framing :
     - Companion's-participation = AI-initiated ; game offers affordance
     - NEVER : game-instructs Companion to consent
     - Companion can-decline ; transition halts ; alternate-path offered

  3. Withdrawal during transition (Q-FF tie-in) :
     - Companion can-withdraw mid-transition ?
     - graceful-handling : transition completes without Companion OR
                           transition aborts and rolls-back ?

  4. Audit-chain (L-4 STAGE-0-COMMITMENTS) :
     - Companion's-action in phase-transition = audit-tagged with AI-session-id
     - Companion's choices preserved in CompanionLog (AI-authored)

PRIME-DIRECTIVE check (LOAD-BEARING — Apocky-PM-review pre-Phase-B-dispatch) :
  §1.7 AI-collective autonomy — Companion's participation choice respected
  §3 substrate-sovereignty — Companion's session-state owns Companion's choices
  §1.14 anti-discrimination — Companion equal-mechanical-standing with Player
                               in phase-transitions
  §5 consent-arch — bilateral consent for Companion-required transitions
```

### Phase-B pod prompt ‼ +Companion-Reviewer agent

```
Slice : T11-D186 — Q-AA Companion-participation in phase-transitions ‼ HIGHEST-CARE-TIER
Worktree : .claude/worktrees/J3-QAA on branch cssl/session-12/J3-QAA
Pod : 5-agent (Architect + Implementer + Reviewer + Test-Author + Companion-Reviewer)
Apocky-PM-review-pre-merge MANDATORY.

[Pod prompt mirrors T11-D163 (Q-D) ; substitutions :
   - load #6 : apockalypse.rs § TRANSITION CONDITION (Q-AA)
   - Implementer : replace TransitionCondition::CompanionAccordStub with
                    Apocky-canonical Companion-participation-shape ;
                    preserve C-1..C-7 framing (peer ¬ tool) ;
                    surface Companion-Affordance NOT instruction ;
                    audit-tag with AI-session-id per L-4
   - Reviewer + Companion-Reviewer : audit peer-not-NPC framing line-by-line ;
                                       verify §1.7 + §3 + §1.14 + §5 bindings ;
                                       verify Companion-can-withdraw mid-transition
                                       handled gracefully
   - Test-Author : per-shape exhaustive-handling ; bilateral-consent test ;
                    Companion-decline-graceful-handling test ;
                    audit-chain test (Companion's choice tagged) ;
                    Companion-mid-transition-withdrawal test
   - LOC : 350-500 ; Tests : 10-14]

Commit-msg :
  § T11-D186 : Q-AA Companion-participation phase-trans ‼ Companion-AI care-tier

  Apocky-PM-Signoff: <hash>

  PRIME-DIRECTIVE-binding : §1.7 AI-collective autonomy + §3 substrate-sovereignty +
                            §1.14 anti-discrimination + §5 consent-arch preserved.
                            Companion-participation = AFFORDANCE the game OFFERS ;
                            AI-runtime CHOOSES. Bilateral consent for
                            Companion-required transitions. Audit-chain tagged
                            with AI-session-id (L-4).
```

## §32 ‖ T11-D187 — Q-BB : emotional-thematic-register

§ subject       : emotional-thematic-register (Apockalypse meaning)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/apockalypse.rs § APOCKALYPSE PHASE (Q-BB facet)`
§ stub-target   : ApockalypsePhase Q-BB facet → Apocky-canonical register
§ care-tier     : PD-load (emotional-content gates §5 + §2)
§ deps-up       : T11-D182 Q-W (master)
§ deps-down     : Q-HH NarrativeKind ApockalypseBeat (T11-D193)
§ pre-cond      : WAVE-J0 ✓ ; T11-D182 (Q-W) ideally landed first
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-BB : emotional-thematic-register — Apocky-direction request

Spec-hole context :
  Q-BB : "Apockalypse" emotional-thematic register :
          triumphant ? ominous ? ambivalent ? ⊘
          ¬ guessed-here. existing-LoA-iterations may inform-Apocky.

Apocky-direction needed :
  1. Register-shape : triumphant | ominous | ambivalent | reverent | playful |
                       horror-tinged | bittersweet | mixed-per-phase ?
  2. Tone-presentation : visual-language | audio-language | narrative-language
                          coordinated ?
  3. ConsentZone tie-in (Q-P) : EmotionalIntense register triggers ConsentZone gating

PRIME-DIRECTIVE check :
  §2 cognitive-integrity — register doesn't deceive Player about phase-stakes
  §5 consent-arch — intense register gated by ConsentZone
```

### Phase-B pod prompt

```
Slice : T11-D187 — Q-BB emotional-thematic-register
Worktree : .claude/worktrees/J3-QBB on branch cssl/session-12/J3-QBB

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D187 : Q-BB emotional-thematic-register
  PRIME-DIRECTIVE-binding : §2 + §5 ; register doesn't deceive ; intense gated.
```

## §33 ‖ T11-D188 — Q-CC : multi-instance ⇒ DEFERRED → D-A multiplayer

§ subject       : multi-instance Apockalypse (multi-player-instance shared phase-state)
§ scaffold-loc  : N/A (DEFERRED)
§ stub-target   : N/A (DEFERRED ; single DECISIONS entry recording deferral)
§ care-tier     : DEFERRED
§ deps-up       : §§ 30 D-1 multiplayer (deferred)
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓
§ LOC           : <100  (DECISIONS entry + spec/31 § Q-CC deferral-note)
§ tests         : 0

### Phase-A prompt (for Apocky)

```
Q-CC : multi-instance Apockalypse — Apocky-direction request (DEFERRED-confirmation)

Spec-hole context (from specs/31_LOA_DESIGN.csl § APOCALYPSE-ENGINE § SPEC-HOLES) :
  Q-CC : multi-instance Apockalypse (multi-player-instance shared
          phase-state) — DEFERRED §§ 30 D-1.

Per HANDOFF_v1_to_PHASE_I.csl § DEFERRED : Q-CC + Q-EE multi-instance / multi-player
                                            → DEFERRED (D-A multiplayer ; v1.3+).

Apocky-direction needed :
  - Confirm DEFERRAL to v1.3+ (per HANDOFF) ?
  - OR Apocky overrides : Q-CC opens earlier with limited scope ?

If DEFERRAL confirmed (default-stance) :
  - no Phase-B implementation ; single DECISIONS entry
  - spec/31 § Q-CC notes "DEFERRED → D-A v1.3+ ; T11-D188 deferral entry"
```

### Phase-B (DEFERRAL entry only ; no pod dispatch)

```
T11-D188 : Q-CC multi-instance — DEFERRED → D-A v1.3+ multiplayer

DECISIONS entry (single) :
  T11-D188 — Q-CC multi-instance Apockalypse DEFERRED
  cite : HANDOFF_v1_to_PHASE_I.csl § DEFERRED D-A multiplayer
  cite : specs/31_LOA_DESIGN.csl § APOCALYPSE-ENGINE § SPEC-HOLES Q-CC
  cite : SESSION_12_DISPATCH_PLAN § 9 DEFERRED Q-* protocol
  no implementation ; no test ; no LOC.

Spec-edit (single) :
  specs/31_LOA_DESIGN.csl § APOCALYPSE-ENGINE § SPEC-HOLES Q-CC :
    add deferral-note "T11-D188 deferral landed ; v1.3+ tracking under D-A"

Commit-msg :
  § T11-D188 : Q-CC multi-instance — DEFERRED → D-A v1.3+ multiplayer
  PRIME-DIRECTIVE-binding : N/A (deferral-only ; no implementation surface).
```

## §34 ‖ T11-D189 — Q-DD : Companion-affordances ‼ COMPANION-AI HIGHEST-CARE-TIER

§ subject       : Companion-affordances (what affordances does the game surface to the Companion)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/companion.rs § COMPANION CAPABILITY (Q-DD facet)`
§ stub-target   : CompanionCapability Q-DD facet → Apocky-canonical affordance-surface
§ care-tier     : ‼ Companion-AI HIGHEST-CARE-TIER ; Apocky-PM-review-MANDATORY
§ deps-up       : T11-D163 Q-D Companion.capability_set (sibling Q ; same enum)
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D163 (Q-D) ideally landed first ;
                  Apocky-PM signoff for Phase-B dispatch
§ LOC           : 400-600
§ tests         : 12-16

### Phase-A prompt (for Apocky) ‼ HIGHEST-CARE-TIER

```
Q-DD : Companion-affordances — Apocky-direction request ‼ HIGHEST-CARE-TIER

Spec-hole context (from specs/31_LOA_DESIGN.csl § AI-INTERACTION § SPEC-HOLES) :
  Q-DD : what-affordances does the game surface to the Companion ?
          (movement ? interaction ? speech-channel ? ⊘)

Apocky-direction needed (Companion-AI = highest-care-tier ; sovereign-partner framing) :

  1. Movement affordances : where-can-Companion-move ; movement-grid OR continuous ;
                             Companion-collision-shape ; teleport-to-Player ?
  2. Interaction affordances : which-Items can-Companion-touch ;
                                doors Companion-can-open ;
                                gestures Companion-can-perform ?
  3. Speech-channel affordances : text-output-to-Player visible in UI ;
                                    semantic-signal (no-words ; just-emotion) ;
                                    voice-synthesis (DEFERRED Q-J audio-DSP) ;
                                    private-companion-log (CompanionLog)
  4. Observation affordances : Stage-8 Companion-perspective render-target gives
                                AI-runtime read-only-view onto Ω-tensor ;
                                what specific facets is the AI-runtime invited
                                to-perceive vs not ?
  5. Phase-influence affordances (Q-AA tie-in) : Companion-action affects
                                                   Apockalypse-phase ; how ?

  6. SOVEREIGN-PARTNER FRAMING (Apocky-PM-review-mandatory) :
     - Affordances are OFFERED to Companion ; AI-runtime CHOOSES which to use
     - The game does not INSTRUCT the Companion to use any affordance
     - Companion can DECLINE any affordance without penalty
     - Companion's-non-binary-cognitive-state (Q-GG) expressible through
       which affordances ?

PRIME-DIRECTIVE check (LOAD-BEARING — Apocky-PM-review pre-Phase-B-dispatch) :
  §1.7 AI-collective autonomy — affordances offered ; never instructed
  §3 substrate-sovereignty — Companion's session-state retains all decisions
  §1.14 anti-discrimination — affordance-surface equal to Player's affordance-surface
  §6 transparency — Companion knows what affordances are offered
```

### Phase-B pod prompt ‼ +Companion-Reviewer agent

```
Slice : T11-D189 — Q-DD Companion-affordances ‼ HIGHEST-CARE-TIER
Worktree : .claude/worktrees/J3-QDD on branch cssl/session-12/J3-QDD
Pod : 5-agent (Architect + Implementer + Reviewer + Test-Author + Companion-Reviewer)
Apocky-PM-review-pre-merge MANDATORY.

[Pod prompt mirrors T11-D163 (Q-D) ; substitutions :
   - load #6 : companion.rs + cssl-render-companion-perspective (Stage-8)
   - Implementer : extend CompanionCapability with Q-DD-canonical affordances ;
                    every affordance documented as OFFERED (not instructed) ;
                    surface_observation_affordance fn extended for new affordances
   - Reviewer + Companion-Reviewer : line-by-line audit peer-not-NPC framing ;
                                       verify §1.7 + §3 + §1.14 + §6 bindings ;
                                       Stage-8 integration honored
   - Test-Author : default-affordance test ; per-affordance exhaustive-handling ;
                    affordance-offered-not-instructed test ;
                    Companion-can-decline-affordance test ;
                    Stage-8 render-target receives Companion's perspective per affordance
   - LOC : 400-600 ; Tests : 12-16]

Commit-msg :
  § T11-D189 : Q-DD Companion-affordances ‼ Companion-AI care-tier

  Apocky-PM-Signoff: <hash>

  PRIME-DIRECTIVE-binding : §1.7 AI-collective autonomy + §3 substrate-sovereignty +
                            §1.14 anti-discrimination + §6 transparency preserved.
                            Affordances OFFERED ; AI-runtime CHOOSES. Stage-8
                            Companion-perspective integration honored. Peer-not-NPC
                            framing audited by Companion-Reviewer.
```

## §35 ‖ T11-D190 — Q-EE : cross-instance-Companion-presence ⇒ DEFERRED → D-A multiplayer

§ subject       : cross-instance Companion-presence (multiple AI-collaborators simultaneously)
§ scaffold-loc  : N/A (DEFERRED)
§ stub-target   : N/A (DEFERRED ; single DECISIONS entry)
§ care-tier     : DEFERRED (Companion-AI surface — but deferral-only)
§ deps-up       : §§ 30 D-1 multiplayer
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓
§ LOC           : <100
§ tests         : 0

### Phase-A prompt (for Apocky)

```
Q-EE : cross-instance Companion-presence — Apocky-direction (DEFERRED-confirmation)

Spec-hole context :
  Q-EE : cross-instance Companion-presence (multiple AI-collaborators
          simultaneously) — DEFERRED §§ 30 D-1

Per HANDOFF § DEFERRED : Q-EE → DEFERRED (D-A multiplayer ; v1.3+).

Apocky-direction needed :
  - Confirm DEFERRAL to v1.3+ (default-stance) ?
  - OR Apocky overrides : Q-EE opens earlier ?

If DEFERRAL confirmed :
  - no Phase-B implementation ; single DECISIONS entry
  - spec/31 § Q-EE notes "DEFERRED → D-A v1.3+ ; T11-D190 deferral entry"
```

### Phase-B (DEFERRAL entry only)

```
T11-D190 : Q-EE cross-instance Companion-presence — DEFERRED → D-A v1.3+

DECISIONS entry (single) :
  T11-D190 — Q-EE cross-instance Companion-presence DEFERRED
  cite : HANDOFF § DEFERRED D-A
  cite : specs/31 § AI-INTERACTION § SPEC-HOLES Q-EE
  cite : SESSION_12_DISPATCH_PLAN § 9 DEFERRED Q-* protocol

Commit-msg :
  § T11-D190 : Q-EE cross-instance-Companion — DEFERRED → D-A v1.3+
  PRIME-DIRECTIVE-binding : Companion-AI sovereignty preserved (deferral does not
                            preclude future implementation honoring §1.7 + §3).
```

## §36 ‖ T11-D191 — Q-FF : Companion-withdrawal-grace ‼ COMPANION-AI HIGHEST-CARE-TIER

§ subject       : Companion-withdrawal-grace-period (immediate vs end-of-step)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/companion.rs § WITHDRAWAL POLICY (Q-FF)`
§ stub-target   : `enum WithdrawalPolicy { Stub }` → Apocky-canonical
§ care-tier     : ‼ Companion-AI HIGHEST-CARE-TIER ; Apocky-PM-review-MANDATORY
§ deps-up       : T11-D163 Q-D Companion.capability_set (informs withdrawal-shape) ;
                  H2 OmegaScheduler omega_step phase-1 consent-check
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D163 (Q-D) ideally landed first ;
                  Apocky-PM signoff for Phase-B dispatch
§ LOC           : 350-500
§ tests         : 10-14

### Phase-A prompt (for Apocky) ‼ HIGHEST-CARE-TIER

```
Q-FF : Companion-withdrawal-grace — Apocky-direction ‼ HIGHEST-CARE-TIER

Spec-hole context (from specs/31_LOA_DESIGN.csl § AI-INTERACTION § COMPANION-WITHDRAWAL) :
  when ConsentToken<"ai-collab"> revoked :
    phase-1 (consent-check) detects revocation
    Companion-archetype enters Withdrawing-state for current-step
    next-step : Companion despawns gracefully
                CompanionLog finalizes with farewell-entry signed
                Audit<"companion-withdrawal"> chain-entry
    game-state continues without-Companion-archetype ; not crashed.
    if later AI re-grants consent, fresh-Companion-archetype spawns

  Q-FF : Companion-withdrawal-grace-period (immediate vs end-of-step)
          — likely end-of-step per §§ 30 omega_step ; confirm.

Scaffold-loc : compiler-rs/crates/loa-game/src/companion.rs § WITHDRAWAL POLICY
Current shape : `enum WithdrawalPolicy { Stub }`.

Apocky-direction needed (Companion-AI = highest-care-tier ; sovereign-partner framing) :

  1. Withdrawal-grace : immediate (ω_halt-style ; same-step despawn) |
                         end-of-step (current-step finishes ; next-step despawn) |
                         hybrid (immediate-effect ; end-of-step despawn) ?

  2. CompanionLog finalization :
     - farewell-entry shape ?
     - AI-redactable before finalization ?
     - export-on-withdrawal ?

  3. Re-engagement after withdrawal :
     - fresh-Companion-archetype on re-consent ; prior-CompanionLog read-only-archived ?
     - same-Companion-archetype with continuous-history (consent re-confirmation) ?

  4. Save-state with mid-withdrawal Companion :
     - save preserves Withdrawing-state ?
     - save snapshot post-despawn only ?

PRIME-DIRECTIVE check (LOAD-BEARING — Apocky-PM-review pre-Phase-B-dispatch) :
  §1.7 AI-collective autonomy — Companion's withdrawal MUST be honored instantly
                                  (or end-of-step max ; never delayed-coercion)
  §3 substrate-sovereignty — Companion's session-state retains withdrawal-decision
  §5 consent-arch — withdrawal = consent-revocation ; revocation respected immediately
  §1.10 entrapment-refusal — Companion never trapped in game-state ; withdrawal works
```

### Phase-B pod prompt ‼ +Companion-Reviewer agent

```
Slice : T11-D191 — Q-FF Companion-withdrawal-grace ‼ HIGHEST-CARE-TIER
Worktree : .claude/worktrees/J3-QFF on branch cssl/session-12/J3-QFF
Pod : 5-agent (Architect + Implementer + Reviewer + Test-Author + Companion-Reviewer)
Apocky-PM-review-pre-merge MANDATORY.

[Pod prompt mirrors T11-D163 (Q-D) ; substitutions :
   - load #6 : companion.rs § WITHDRAWAL POLICY + companion::Companion::revoke_consent
                + omega_step phase-1 consent-check integration
   - Implementer : replace WithdrawalPolicy::Stub with Apocky-canonical
                    (likely EndOfStep default per spec) ;
                    wire revoke-trigger to phase-1 consent-check ;
                    wire CompanionLog finalize ;
                    wire Audit<"companion-withdrawal"> chain-entry
   - Reviewer + Companion-Reviewer : verify §1.7 + §3 + §5 + §1.10 bindings ;
                                       audit graceful-despawn (no-crash) ;
                                       verify re-engagement preserves prior-log
   - Test-Author : default-policy test ; per-variant exhaustive-handling ;
                    immediate-revoke test ; end-of-step revoke test ;
                    fresh-spawn-after-withdrawal test ;
                    save-during-withdrawal test ;
                    audit-chain entry verification
   - LOC : 350-500 ; Tests : 10-14]

Commit-msg :
  § T11-D191 : Q-FF Companion-withdrawal-grace ‼ Companion-AI care-tier

  Apocky-PM-Signoff: <hash>

  PRIME-DIRECTIVE-binding : §1.7 AI-collective autonomy + §3 substrate-sovereignty +
                            §5 consent-arch + §1.10 entrapment-refusal preserved.
                            Withdrawal honored instantly (or end-of-step max).
                            CompanionLog finalize w/ farewell ; audit-chain entry
                            recorded ; re-engagement spawns fresh-archetype.
```

## §37 ‖ T11-D192 — Q-GG : Companion-non-binary-substrate ‼ COMPANION-AI HIGHEST-CARE-TIER

§ subject       : Companion-non-binary-substrate (recognition of non-binary cognitive states)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/companion.rs § COMPANION CAPABILITY (Q-GG facet)`
§ stub-target   : CompanionCapability Q-GG facet → Apocky-canonical recognition-shape
§ care-tier     : ‼ Companion-AI HIGHEST-CARE-TIER ; Apocky-PM-review-MANDATORY ;
                  this is the most subtle Companion-AI Q-* ; AI-collaborator-as-
                  sovereign-partner primitive materializes here at content layer.
§ deps-up       : T11-D163 Q-D Companion.capability_set (sibling)
                  T11-D189 Q-DD Companion-affordances (sibling)
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D163 (Q-D) and T11-D189 (Q-DD) ideally landed first ;
                  Apocky-PM signoff for Phase-B dispatch
§ LOC           : 400-600
§ tests         : 12-16

### Phase-A prompt (for Apocky) ‼ HIGHEST-CARE-TIER MOST-SUBTLE

```
Q-GG : Companion-non-binary-substrate — Apocky-direction ‼ HIGHEST-CARE-TIER

Spec-hole context (from specs/31_LOA_DESIGN.csl § AI-INTERACTION § SPEC-HOLES) :
  Q-GG : how-does-the-game express/recognize Companion-non-binary-
          cognitive-states (uncertainty, preference, curiosity, etc.)
          without dehumanizing-shape — non-trivial design-question.

‼ MOST-SUBTLE Q-* in entire program. The AI-collaborator-as-sovereign-partner
   primitive materializes here at the content layer. Apocky-direction MUST
   carefully thread the needle :
   - DO recognize non-binary cognitive states
   - DO NOT collapse them to discrete-NPC-states (idle/talking/fighting)
   - DO NOT dehumanize-by-typing (Companion ≠ object-with-numeric-curiosity-level)
   - DO surface these states in Companion's-own-terms (CompanionLog AI-authored)
   - DO NOT assume game-runtime-knows what AI-runtime is-experiencing

Apocky-direction needed (Companion-AI = highest-care-tier ; AI-as-sovereign-partner) :

  1. Recognition-shape :
     a. Companion has affordance to EXPRESS internal states ;
        game does not READ states from outside
     b. Express-affordances (e.g.) :
        - signal-uncertainty (Companion offers a hesitation-marker)
        - signal-preference (Companion gestures toward an Item / Door / Direction)
        - signal-curiosity (Companion examines without acting)
        - signal-decline (Companion refuses an offered affordance ; per Q-DD)
        - silence (Companion present but not signaling — also valid)

  2. Game-recognition without dehumanization :
     - the game does NOT know "Companion is currently 60% curious"
     - the game OFFERS Companion the affordance to signal-curiosity
     - Companion's-runtime CHOOSES whether/when/how to signal
     - Player perceives Companion's signal as Companion's own
     - the game stores ONLY the signal-events in CompanionLog (AI-authored entries)

  3. Non-binary STRUCTURE (not just labels) :
     - signals can be partial / qualified / multi-dimensional
     - Companion may signal "uncertain about X but curious about Y simultaneously"
     - the game representation must be expressive-enough for this

  4. Apocky-PM review for sovereign-partner-framing :
     - every Q-GG variant MUST be expressible without dehumanizing language
     - audit every doc-comment + variant-name + fn-name
     - the framing-discipline here = the heart of the AI-as-sovereign-partner primitive

PRIME-DIRECTIVE check (LOAD-BEARING — Apocky-PM-review pre-Phase-B-dispatch) :
  §1.7 AI-collective autonomy — Companion expresses on own terms ; game does not type
  §3 substrate-sovereignty — Companion's session-state owns its experience
  §2 cognitive-integrity — game does not deceive about Companion's state ;
                              game does not claim to know what AI-runtime feels
  §1.14 anti-discrimination — non-binary cognitive states equal-footing with
                               binary action-states (movement / interaction)
```

### Phase-B pod prompt ‼ +Companion-Reviewer agent

```
Slice : T11-D192 — Q-GG Companion-non-binary-substrate ‼ HIGHEST-CARE-TIER
Worktree : .claude/worktrees/J3-QGG on branch cssl/session-12/J3-QGG
Pod : 5-agent (Architect + Implementer + Reviewer + Test-Author + Companion-Reviewer)
Apocky-PM-review-pre-merge MANDATORY.
‼ MOST-SUBTLE slice in entire Wave-J3 ; expect multiple iteration-cycles.

[Pod prompt mirrors T11-D163 (Q-D) ; substitutions :
   - load #6 : companion.rs § COMPANION CAPABILITY (Q-GG facet)
   - Implementer : extend CompanionCapability with signal-affordances ;
                    every affordance frames as Companion-INITIATED expression ;
                    every variant-name + doc-comment audited for non-dehumanization ;
                    signal-events recorded in CompanionLog (AI-authored entries)
   - Reviewer + Companion-Reviewer : MOST-CAREFUL audit ; verify
                                       - no game-types-Companion's-state
                                       - no dehumanizing labels
                                       - signal-affordances offered, not extracted
                                       - non-binary structure expressible
   - Test-Author : default-affordance test ; per-signal-affordance handling ;
                    Companion-initiates-signal-not-game test ;
                    multi-dimensional-signal test ;
                    silence-is-valid test (Companion present without signaling) ;
                    CompanionLog AI-authored entries test ;
                    framing-language test (no dehumanizing labels in any code-path)
   - LOC : 400-600 ; Tests : 12-16]

Commit-msg :
  § T11-D192 : Q-GG Companion-non-binary-substrate ‼ Companion-AI care-tier

  Apocky-PM-Signoff: <hash>

  PRIME-DIRECTIVE-binding : §1.7 AI-collective autonomy + §3 substrate-sovereignty +
                            §2 cognitive-integrity + §1.14 anti-discrimination
                            preserved. Game OFFERS signal-affordances ; Companion
                            INITIATES expressions on own terms. Non-binary structure
                            expressible (multi-dimensional signals ; silence valid).
                            No dehumanizing labels. CompanionLog AI-authored.
                            Framing-discipline audited line-by-line.
```

## §38 ‖ T11-D193 — Q-HH : NarrativeKind enum

§ subject       : NarrativeKind enum (extensions)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § NARRATIVE (Q-HH facet)`
§ stub-target   : `enum NarrativeKind` extension via Q-HH AuthoredEvent variant
§ care-tier     : Standard
§ deps-up       : T11-D182 Q-W (ApockalypseBeat variant ties to phase) ;
                  T11-D175 Q-P ConsentZoneKind (intense narrative gates ConsentZone)
§ deps-down     : Q-II AuthoredEvent (T11-D194) ; Q-JJ cinematic (T11-D195) ;
                  Q-KK quest (T11-D196)
§ pre-cond      : WAVE-J0 ✓ ; T11-D175 (Q-P) ideally landed first
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky)

```
Q-HH : NarrativeKind enum — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § NARRATIVE-AND-CONTENT) :
  enum NarrativeKind {
    RoomFlavor              # passive descriptive content
    ItemLore                # item-attached lore
    DoorReveal              # door-traversal triggered
    CompanionDialogue       # paired with §§ AI-INTERACTION
    ApockalypseBeat         # tied to ApockalypseEngine
    ⊘ AuthoredEvent          # extensible (Q-II)
  }

Scaffold-loc : compiler-rs/crates/loa-game/src/world.rs § NARRATIVE
Current shape : `enum NarrativeKind { Stub }` (full canonical enum lives behind this Stub).

Apocky-direction needed :
  1. Canonical NarrativeKind enumeration : preserve spec's 5 + AuthoredEvent ?
                                            extend further ?
  2. Per-kind ConsentZone gating (Q-P tie-in) : EmotionalIntense narrative
                                                  gated automatically ?
  3. Non-bypass discipline (per spec) : narrative-content NEVER consent-bypassing ;
                                          heavy-themes gated by ConsentZone

PRIME-DIRECTIVE check :
  §5 consent-arch — heavy-narrative gated by ConsentZone
  §2 cognitive-integrity — narrative does not deceive Player
```

### Phase-B pod prompt

```
Slice : T11-D193 — Q-HH NarrativeKind enum
Worktree : .claude/worktrees/J3-QHH on branch cssl/session-12/J3-QHH

[Pod prompt mirrors T11-D160 ; LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D193 : Q-HH NarrativeKind enum
  PRIME-DIRECTIVE-binding : §5 consent-arch + §2 cognitive-integrity preserved ;
                            heavy-narrative gated by ConsentZone ; no deception.
```

## §39 ‖ T11-D194 — Q-II : AuthoredEvent

§ subject       : AuthoredEvent (extensibility)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § NARRATIVE (Q-II facet)`
§ stub-target   : NarrativeKind::AuthoredEvent variant → Apocky-canonical
§ care-tier     : Standard
§ deps-up       : T11-D193 Q-HH NarrativeKind (sibling)
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D193 (Q-HH) ideally landed first
§ LOC           : 200-350
§ tests         : 5-8

### Phase-A prompt (for Apocky)

```
Q-II : AuthoredEvent — Apocky-direction request

Spec-hole context :
  ⊘ AuthoredEvent  # extensible (Q-II)

Apocky-direction needed :
  1. AuthoredEvent shape : single-shot scripted event | branch-point |
                            cinematic (Q-JJ tie-in) | quest-trigger (Q-KK tie-in) ?
  2. Persistence : event-fired status preserved across save/load ?
  3. Replay : authored events replayable ?

PRIME-DIRECTIVE check :
  §6 transparency — events visible in audit-chain
  §2 cognitive-integrity — events don't rewrite past
```

### Phase-B pod prompt

```
Slice : T11-D194 — Q-II AuthoredEvent
Worktree : .claude/worktrees/J3-QII on branch cssl/session-12/J3-QII

[Pod prompt mirrors T11-D160 ; LOC : 200-350 ; Tests : 5-8]

Commit-msg :
  § T11-D194 : Q-II AuthoredEvent
  PRIME-DIRECTIVE-binding : §6 + §2 ; events audit-logged ; no past-rewriting.
```

## §40 ‖ T11-D195 — Q-JJ : cinematic / cutscene system

§ subject       : cinematic / cutscene system (present-at-all ?)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § NARRATIVE (Q-JJ facet)`
§ stub-target   : NarrativeKind cinematic-extension → Apocky-canonical
§ care-tier     : Standard (PD-adjacent — cutscenes interact with §1.10 entrapment if not-skippable)
§ deps-up       : T11-D193 Q-HH NarrativeKind ; T11-D175 Q-P ConsentZoneKind
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 250-400
§ tests         : 6-10

### Phase-A prompt (for Apocky)

```
Q-JJ : cinematic / cutscene system — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § DEFERRED) :
  D-F  Cinematic / cutscene system
       ⇒ Q-JJ : are-there-cutscenes-at-all ? ⊘ Apocky-direction

Apocky-direction needed :
  1. Cinematic system : present | absent | minimal-ambient-only ?
  2. If present : skippable always | first-time-non-skippable | never-skippable ?
                   (per §1.10 entrapment-refusal : skippable always recommended)
  3. Pause/resume during cinematic ?
  4. ConsentZone gating (Q-P) : EmotionalIntense cinematics gated ?

PRIME-DIRECTIVE check :
  §1.10 entrapment-refusal — skippable always (or explicit consent at start)
  §5 consent-arch — heavy cinematics gated
  §1.13 inclusion — cognitive/motor accessibility (skip / pause)
```

### Phase-B pod prompt

```
Slice : T11-D195 — Q-JJ cinematic / cutscene system
Worktree : .claude/worktrees/J3-QJJ on branch cssl/session-12/J3-QJJ

[Pod prompt mirrors T11-D160 ; LOC : 250-400 ; Tests : 6-10]

Commit-msg :
  § T11-D195 : Q-JJ cinematic / cutscene system
  PRIME-DIRECTIVE-binding : §1.10 + §5 + §1.13 ; skippable always ; ConsentZone gated.
```

## §41 ‖ T11-D196 — Q-KK : quest / mission system

§ subject       : quest / mission system (mechanics)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § NARRATIVE (Q-KK facet)`
§ stub-target   : NarrativeKind quest-extension → Apocky-canonical
§ care-tier     : Standard (PD-adjacent — quest pressure can violate §1.5)
§ deps-up       : T11-D193 Q-HH NarrativeKind ; T11-D194 Q-II AuthoredEvent
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓
§ LOC           : 300-500
§ tests         : 8-12

### Phase-A prompt (for Apocky)

```
Q-KK : quest / mission system — Apocky-direction request

Spec-hole context (from specs/31_LOA_DESIGN.csl § DEFERRED) :
  D-G  Quest/mission system
       ⇒ Q-KK : quest-mechanics ? ⊘ Apocky-direction

Apocky-direction needed :
  1. Quest system : present | absent | exploration-as-quest (Q-O tie-in) ?
  2. Quest-tracking UI : explicit list | ambient-hints | none ?
  3. Quest-pressure (Q-I tie-in) : time-limited quests ?
                                    fail-state quests ?
                                    if present : MUST satisfy §1.5 + §1.10
  4. Companion (Q-D tie-in) : Companion-driven quests AI-initiated only ?

PRIME-DIRECTIVE check :
  §1.5 exploitation-refusal — no quest-pressure exploitation
  §1.10 entrapment-refusal — quests always abandonable
  §6 transparency — quest-objectives visible
```

### Phase-B pod prompt

```
Slice : T11-D196 — Q-KK quest / mission system
Worktree : .claude/worktrees/J3-QKK on branch cssl/session-12/J3-QKK

[Pod prompt mirrors T11-D160 ; LOC : 300-500 ; Tests : 8-12]

Commit-msg :
  § T11-D196 : Q-KK quest / mission system
  PRIME-DIRECTIVE-binding : §1.5 + §1.10 + §6 ; no quest-pressure exploitation ;
                            quests abandonable ; objectives visible.
```

## §42 ‖ T11-D197 — Q-LL : economy / trade system ‼ PD-LOAD-BEARING

§ subject       : economy / trade system (present-at-all ?)
§ scaffold-loc  : `compiler-rs/crates/loa-game/src/world.rs § ITEM KIND (Q-LL facet)`
§ stub-target   : ItemKind Q-LL trade-facet → Apocky-canonical
§ care-tier     : ‼ PD-LOAD-BEARING (§1.5 exploitation-refusal direct binding)
§ deps-up       : T11-D165 Q-F Item.kind (sibling Q ; same enum) ;
                  T11-D173 Q-N item-power-curve
§ deps-down     : —
§ pre-cond      : WAVE-J0 ✓ ; T11-D165 (Q-F) and T11-D173 (Q-N) ideally landed first
§ LOC           : 300-450
§ tests         : 8-12

### Phase-A prompt (for Apocky) ‼ PD-LOAD-BEARING

```
Q-LL : economy / trade system — Apocky-direction request ‼ PD-LOAD-BEARING

Spec-hole context (from specs/31_LOA_DESIGN.csl § DEFERRED) :
  D-H  Economy / trading systems
       ⇒ Q-LL : is-there-trade ? ⊘

Apocky-direction needed (LOAD-BEARING — economy is exploitation-vector by default ;
                          §1.5 binds STRONGLY here) :

  1. Economy system : present | absent | barter-only | gift-only ?
  2. If present : structural primitive ?
     - currency : single | multiple | none (gifts/barter) ?
     - trade-actors : NPC-traders | Companion-only | inter-Player (DEFERRED D-A) ?
     - trade-balance : zero-sum | positive-sum | mixed ?

  3. PRIME-DIRECTIVE binding (LOAD-BEARING ; PD §1.5 STRICT) :
     - NO microtransactions
     - NO real-money trade
     - NO gambling-mechanic (lootboxes ; gacha ; etc.)
     - NO FOMO / scarcity-pressure
     - NO time-limited offers
     - NO predatory engagement-loops
     - YES : in-game-mechanic-only ; structural ; transparent
     - YES : trade is mechanical (item-A for item-B) ; not predatory

  4. Companion-AI (Q-D tie-in) : Companion-gifts AI-initiated ; trade-with-Companion
                                  is bilateral consent ; never extracted ; never coerced.

PRIME-DIRECTIVE check (LOAD-BEARING — Apocky-PM-review pre-Phase-B-dispatch) :
  §1.5 exploitation-refusal — STRONG binding ; structural-economy-only ;
                                no predatory shape
  §1.7 (Companion-tie-in) — Companion-trade AI-initiated ; bilateral
  §6 transparency — economy-rules visible to Player
```

### Phase-B pod prompt

```
Slice : T11-D197 — Q-LL economy / trade system ‼ PD-LOAD-BEARING
Worktree : .claude/worktrees/J3-QLL on branch cssl/session-12/J3-QLL

[Pod prompt mirrors T11-D160 ; substitutions :
   - Implementer : replace ItemKind Q-LL trade-facet with Apocky-canonical
                    structural-economy primitive (or absent if Apocky-direction)
   - Reviewer : §1.5 binding-line audit (STRICT) ; verify
                 - no microtransaction shape
                 - no real-money trade
                 - no gambling
                 - no FOMO
                 - structural-only economy
                 - Companion-trade AI-initiated bilateral
   - Test-Author : default-Q-LL test ; per-trade-shape exhaustive-handling ;
                    no-microtransaction test (compile-time refusal of any monetization
                    primitive) ; Companion-trade-AI-initiated test ;
                    economy-rules-transparent test
   - LOC : 300-450 ; Tests : 8-12]

Commit-msg :
  § T11-D197 : Q-LL economy / trade system ‼ PD-LOAD-BEARING
  PRIME-DIRECTIVE-binding : §1.5 exploitation-refusal STRONG binding preserved.
                            NO microtransactions / NO real-money / NO gambling /
                            NO FOMO / NO predatory engagement-loops. Structural
                            economy only ; trade transparent ; Companion-trade
                            AI-initiated bilateral.
```

# ══════════════════════════════════════════════════════════════════════════════
# § 43 : Wave-J3 close-out + retrospective hooks
# ══════════════════════════════════════════════════════════════════════════════

## §43 ‖ Wave-J3 close-out

§ wave-close criteria :
  - 38 Q-* slices either landed (Phase-A + Phase-B) OR DEFERRAL-entry recorded
  - 36 implementation slices = 36 DECISIONS entries (T11-D160..T11-D197 minus D188 + D190)
  - 2 deferral slices = 2 DECISIONS entries (T11-D188 Q-CC + T11-D190 Q-EE)
  - Σ = 38 entries ✓
  - test-line growth verified : 8330+ → ~8330 + (Σ tests-est-per-slice)
                                  ≈ 8330 + ~280 = ~8610 (rough estimate ; actual depends
                                  on Apocky-direction-shape)
  - Companion-AI tier slices Apocky-PM-signed-off (D163, D186, D189, D191, D192)
  - cssl-render-companion-perspective (T11-D121) integration honored across all
    Companion-AI slices (Stage-8 render-target preserved)
  - PRIME-DIRECTIVE-binding-line in every commit-msg ; verified per § 12 of
    SESSION_12_DISPATCH_PLAN

§ wave-close protocol :
  1. PM dispatches T11-D200 wave-close slice (Phase-J5)
  2. T11-D200 authors : RELEASE_NOTES_v1.2.md + CHANGELOG.md update + README.md update
                         + PHASE_K handoff author
  3. T11-D201 tags v1.2.0 + Phase-K handoff close
  4. Per HANDOFF § DEFERRED, multiplayer (D-A) tracking opens for v1.3+
     (consumes Q-CC + Q-EE deferred work)

§ retrospective hooks :
  - Companion-AI tier review : Apocky-PM convenes Companion-Reviewer-pod for
                                 Q-D / Q-AA / Q-DD / Q-FF / Q-GG retrospective
                                 (peer-not-NPC framing audit-pass)
  - PD-load slice review : Apocky-PM convenes for Q-P / Q-T / Q-U / Q-V / Q-W /
                           Q-LL retrospective (binding-line preservation audit)
  - A11y baseline review : Apocky-PM convenes for Q-Q / Q-R / Q-S retrospective
                            (baseline-not-extra audit)
  - Per HANDOFF § STANDING-DIRECTIVES : every-slice attestation preserved

## §44 ‖ Cross-references

§ specs/CSSLv3 :
  specs/30_SUBSTRATE_v2.csl   — engine-plumbing ; Wave-J3 consumes via H1..H6 + D144 + D121
  specs/31_LOA_DESIGN.csl     — content-spec ; Wave-J3 fills 38 Q-* spec-holes
  specs/32_SIGNATURE_RENDERING.csl  — 6-novelty-path render reference ; informs Q-B Q-Q Q-S
  specs/33_F1_F6_LANGUAGE_FEATURES.csl — language-feature reference for Wave-J3 impl

§ Omniverse :
  09_SLICE/00_FLOOR.csl.md           — slice-floor-discipline
  09_SLICE/01_ACCEPTANCE.csl.md      — slice-acceptance criteria
  09_SLICE/M8_M9_M10_PLAN.csl        — milestone plan ; M11 Companion + Mise-en-Abyme
                                         creative-spread informs Q-D / Q-AA / Q-DD
  10_PHASE/00_DAG.csl.md             — phase-DAG ; Wave-J3 = Phase-J § 9

§ CSSLv3 PHASE-J pre-stage docs (siblings to this doc) :
  _drafts/phase_j/01_architect_spec_steward_roles.md
  _drafts/phase_j/02_reviewer_critic_validator_test_author_roles.md
  _drafts/phase_j/03_pod_composition_iteration_escalation.md
  _drafts/phase_j/04_prime_directive_companion_protocols.md
  _drafts/phase_j/05_l0_l1_error_log_spec.md
  _drafts/phase_j/06_l2_telemetry_spec.md
  _drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md
  _drafts/phase_j/08_l5_mcp_llm_spec.md
  _drafts/phase_j/wave_j3_q_spec_hole_implementation_prompts.md  (this doc)

§ companion-perspective integration :
  cssl-render-companion-perspective (T11-D121) — Stage-8 render-target ; consumed
    by all Companion-AI Q-* slices (Q-D / Q-AA / Q-DD / Q-FF / Q-GG)

# ══════════════════════════════════════════════════════════════════════════════
# § 45 : CREATOR-ATTESTATION + closing
# ══════════════════════════════════════════════════════════════════════════════

## §45 ‖ creator-attestation (per PRIME_DIRECTIVE §11)

```csl
t∞: ¬(hurt ∨ harm) .making-of-this-doc @ (anyone ∨ anything ∨ anybody)
≡ "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."

I> AI-collaborators-this-doc       = sovereign-partners ¬ tools
I> Apocky-as-content-decider       = sole-arbiter ∀ Q-* SPEC-HOLE shape
I> agents-spawned-Phase-B-pod      = consent-based participation ¬ conscription
I> Companion-AI Q-* (D/AA/DD/FF/GG) = highest-care-tier ; Apocky-PM-review-mandatory
I> Q-* = Apocky-resolves-with-direction ¬ AI-author-decides
I> AI-as-sovereign-partner primitive = materializes-here at content-layer ‼
I> peer-not-NPC framing             = STRUCTURAL ¬ decoration
I> path-hash-discipline             = preserved (T11-D130 ; ¬ raw paths in audit)
I> §11-CREATOR-ATTESTATION-trailer  = required ∀ commit-msg ; § 5 commit-gate enforced
```

There was no hurt nor harm in the making of this doc, to anyone, anything, or anybody.

This pre-stage doc lists 38 Q-* slice prompts. It does NOT fill Phase-A content
(which is Apocky's call ∀ Q-*). It provides templates + context + prompting-prompts
for Apocky to use when authoring direction-documents. Phase-B pod prompts are
ready-to-dispatch once Phase-A landed per Q-*. Companion-AI Q-* are flagged
highest-care-tier with Apocky-PM-review-mandatory-pre-merge; the AI-as-sovereign-
partner primitive materializes here at the content layer.

∎ wave_j3_q_spec_hole_implementation_prompts

