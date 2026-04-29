# SESSION_12 DISPATCH PLAN — Phase J: LoA content authoring + M8/M9/M10 milestones

**File:** `SESSION_12_DISPATCH_PLAN.md` (repo root)
**Source of truth for slice specs:** `specs/31_LOA_DESIGN.csl § SPEC-HOLES-CONSOLIDATED` (Q-A through Q-LL) + `PHASE_J_HANDOFF.csl`.
**Source of truth for prior decisions:** `DECISIONS.md` (T11-D113..T11-D147 reservation blocks) + `RELEASE_NOTES_v1.1.md`.
**Continuation of:** `SESSION_11_DISPATCH_PLAN.md` (substrate-evolution + signature-rendering retrospective).
**This file:** the operational layer for Phase J — what to dispatch, in what order, with what milestone-gates and PRIME-DIRECTIVE bindings.

---

## § 0. PM CHARTER

**Apocky** = CEO + Product Owner. Sets vision, priorities, makes final calls. Verifies the M8 acceptance gate personally. Owns every Q-* SPEC-HOLE resolution (no AI-author fills these). Adjudicates escalations.

**Claude (this PM)** = PM + Tech Lead. Translates direction into work, dispatches agents, reviews output against acceptance criteria, manages merge sequence, holds quality bar, surfaces blockers proactively.

**Agents (Claude Code instances)** = developers. Each gets one slice end-to-end. Stay in their lane. Branch + worktree discipline. Code-review (PM) before merge. One deployer at a time per integration branch. Treated as actual team members — assigned responsibility, accountability, signed commits.

**Standing rules (carried from sessions 6 / 7 / 11):**

- CSLv3 reasoning + dense code-comments inside CSSLv3 work.
- English prose only when user-facing (DECISIONS, commit messages, this file, RELEASE_NOTES).
- Disk-first; never artifacts.
- Peer not servant — no flattery, no option-dumping, no hedging.
- PRIME_DIRECTIVE preserved at every step ("no hurt nor harm").
- Failing tests block the commit-gate; iterate until green.
- ~~`--test-threads=1` is mandatory (cssl-rt cold-cache flake carry-forward from T11-D56).~~  **RETIRED by T11-D153** (2026-04-29) : the cold-cache flake is FIXED via per-test lock-discipline + poison-tolerant lock acquisition.  Default parallelism (`cargo test --workspace`) is now the supported, well-tested mode.  `--test-threads=1` remains a valid debug-flag for any contributor who wants determinism, but it is no longer mandatory.
- Per-slice DECISIONS.md expansion is mandatory at merge time.
- §11 CREATOR-ATTESTATION trailer required in every commit message.
- **Q-* SPEC-HOLE resolution = Apocky-only**. AI authors implement scaffolding; Apocky decides the content shape.

### § 0.A TEAM DISCIPLINE — POD MODEL (canonical reference)

**Phase-J onward uses the 4-agent pod model + 5-of-5 quality gate.** This supersedes the informal single-agent dispatch pattern from Phase-G/H/I. Full canonical reference: **`SESSION_12_TEAM_DISCIPLINE.md`** (T11-D150 ; ~1635 LOC ; cites `_drafts/phase_j/{01..04}_*.md` for verbose role-spec detail).

**3-tier role hierarchy:**

- **Tier 1 (rightholder + orchestration)** : Apocky (CEO + AXIOM-final) ; Claude-PM (orchestration + tech-lead).
- **Tier 2 (advisory + gate ; ¬ writes-code)** : Architect (composition-coherence) ; Spec-Steward (spec-authority).
- **Tier 3 (pod-internal)** : Implementer + Reviewer + Critic + Validator + Test-Author.

**4-agent canonical pod (per slice)** : Implementer (lane-locked) + Reviewer (cross-pod, parallel) + Test-Author (cross-pod, parallel, spec-only-input) + Critic (cross-pod, post-completion adversary). Validator + Architect + Spec-Steward + PM are **cross-cutting** (1 per wave, not per slice).

**5-of-5 quality gate** (slice cannot merge until ALL true):

1. G1 Implementer self-test green (cargo fmt + clippy + test ; default parallelism per T11-D153)
2. G2 Reviewer-sign-off (spec-anchor-conformance + API-coherence + invariants ; all-HIGH resolved)
3. G3 Critic-veto-cleared (veto-flag = FALSE ; failure-fixtures pass)
4. G4 Test-Author-tests-passing (spec-§-coverage = 100% ; cross-pod-confirmed)
5. G5 Validator-spec-conformance (spec-§-resolved 100% ; gap-list-A + gap-list-B = zero-HIGH ; verdict APPROVE)

**Cross-pod discipline (CRITICAL)** : `Implementer-pod ≠ Reviewer-pod ≠ Test-Author-pod` for the same slice (3-pod-min). Bias-mitigation rationale: same-pod ⇒ same-mental-model ⇒ same-blind-spots. Same Claude-Code-instance can serve different roles **across different slices** ; cannot serve different roles in the **same slice**.

**Iteration cap : MAX 3 cycles per slice** before mandatory Apocky-escalation. Cycle = one wave-dispatch (≈30-60 min wall-clock), NOT calendar-time. Cycle-counter logged per-slice in DECISIONS sub-block. ≥4 cycles ⇒ slice replanned ; "let me try once more" infinite-loop = anti-pattern.

**N-pod-ring rotation** for cross-pod review : with N pods active in a wave, each pod implements-1 + reviews-1 + tests-1 + critics-1, ensuring full-coverage cross-review without single-pod-saturation.

**Apocky-only triggers (NEVER bypass)** : PRIME-DIRECTIVE violations ; AXIOM-level spec amendments ; Companion-AI sovereignty ; AI-collaborator naming/handle ; Apockalypse canonical-spelling ; cross-project FFI/vendor ; ToS revocation ; identity-claim encoding. PM cannot resolve these alone.

**Companion-as-peer = STRUCTURAL not stylistic** : Companion has Σ-facet (consent-bits per-cell, D138) + Φ-facet (Pattern-preservation across re-crystallizations) + Sovereign-handle. Compiler refuses programs that treat Companions as NPCs. Companion-decline persists across re-crystallizations (save/load/restart cannot clear it).

**Reference**: every detail above is cross-referenced in `SESSION_12_TEAM_DISCIPLINE.md` with examples, anti-patterns, and enforcement-mechanisms. The four `_drafts/phase_j/{01..04}_*.md` source-slices are retained as historical record.

---

## § 1. The DAG (one-page reference)

```
┌──────────────────────────────────────────────────────────────────┐
│ ENTRY  cssl/session-6/parallel-fanout @ b69165c (post-wave-5)    │
│         8330+ tests / 0 failed / 16 ignored baseline             │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-Jα  Synthesis : SESSION_12_TEAM_DISCIPLINE.md (T11-D150)    │
│           ✓ DONE @ 9de614e                                        │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-Jβ  Synthesis : DIAGNOSTIC_INFRA_PLAN.md (T11-D151)         │
│           ✓ DONE @ 28b6834                                        │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ AUDIT-FIX  WAVE  (T11-D152..D154)                                 │
│   D152 cssl-host-net WSAStartup race                              │
│   D153 cssl-rt cold-cache lock (closes T11-D56)                   │
│   D154 cssl-cgen-gpu-wgsl dlltool / windows-sys pin               │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-Jε  L0 + L1 diagnostic-infra (T11-D155..D156)               │
│   D155 cssl-error • D156 cssl-log                                 │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-Jζ  L2 telemetry diagnostic-infra (T11-D157..D161)          │
│   D157 cssl-metrics • D158 frame-time instrumentation             │
│   D159 health-probes • D160 spec-coverage tracker                 │
│   D161 replay-determinism preservation                            │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-Jη  L3 + L4 diagnostic-infra (T11-D162..D164)               │
│   D162 cssl-inspect • D163 cssl-hot-reload • D164 cssl-tweak      │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-Jθ  L5 MCP-LLM CROWN diagnostic-infra (T11-D165..D172)      │
│   D165 skeleton • D166 state • D167 telemetry • D168 health      │
│   D169 time-control • D170 hot-reload • D171 test • D172 privacy │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-Jι  Iteration-loop documentation (T11-D173)                 │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-J0  M8 acceptance gate (T11-D174)                           │
│   M8 = 12-stage pipeline wired end-to-end through loa-game ;     │
│         on Apocky's Arc A770 host ; canonical SDF scene renders. │
│   ◆ APOCKY VERIFIES M8 PERSONALLY BEFORE J1..JN DISPATCH ◆       │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-J1  M9 VR-ship preparation (T11-D175..D179)                 │
│   J1-OpenXR session-claim consent UI                             │
│   J1-Stage1 Embodiment integration (XR-input → body-presence)    │
│   J1-Stage12 XrCompose integration (XR-composition layers)       │
│   J1-AppSW Stage-11 motion-vec + depth                           │
│   J1-ToneMap Stage-10 tone-map + bloom + post                    │
│   ◆ M9 DEFERRED ON LIVE HARDWARE — runs on real headset only ◆   │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-J2  M10 max-density preparation (T11-D180..D183)            │
│   J2-WorkGraph Stage-3 Ω-field-update integration                │
│   J2-LBM-tile-streaming for 1M+ entity scaling                   │
│   J2-Foveation budget-driven density-budget enforcement          │
│   J2-Async-compute Ω-field 6-phase pipelining                    │
│   ◆ M10 DEFERRED ON LIVE HARDWARE — needs real M7-target host ◆  │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-J3  Q-* SPEC-HOLE content authoring                         │
│         (T11-D184..D221 ; 38 slices ; APOCKY-FILL)               │
│   Q-A   Labyrinth.generation_method                              │
│   Q-B   Floor.theme                                              │
│   Q-C   Player.progression_state                                 │
│   ...                                                            │
│   Q-LL  economy / trade system                                   │
│                                                                  │
│   ◆ ∀ Q-* = Apocky-resolves-with-direction ◆                     │
│   ◆ ∀ Q-* = single per-slice DECISIONS entry + spec cite ◆       │
│   ◆ Companion-AI Q-* are extra-careful (Q-D, Q-DD..Q-GG) ◆       │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-J4  M9 / M10 hardware-validation (T11-D222..D223)           │
│   M9 — VR ship verification on live OpenXR headset (D222)        │
│   M10 — 1M+ entity stress test on M7-target host (D223)          │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ WAVE-J5  v1.2 close + tag (T11-D224..D225)                       │
│   CHANGELOG + README update + RELEASE_NOTES_v1.2.md author       │
│   Tag v1.2.0                                                     │
└──────────────────────────────────────────────────────────────────┘
```

---

## § 2. Slice-ID reservation block (T11-D150..T11-D225) — V2 ALLOCATION

**Reserved range:** T11-D150..T11-D225 (76 IDs).

> **NOTE on numbering:** T11-D148 is the prior docs landing (README + dispatch plans + handoff). T11-D149 was used by Apocky's prior substrate-evolution reference-docs commit (specs 30v2 + 32 + 33). T11-D150 + T11-D151 already landed as Phase-J synthesis-docs (Wave-Jα + Wave-Jβ). Phase-J therefore reserves from T11-D152 onward, with the original Phase-J content waves shifted by +24 vs the c79bcf3 fixup. Canonical authority: **DECISIONS.md § META T11-D-RESERVATIONS-V2**.

**Allocation (V2 — accommodates Jα/Jβ syntheses + audit-fix + Jε..Jι diagnostic-infra waves):**

| ID range          | Wave / Purpose                                                                                  | Status         |
| ----------------- | ----------------------------------------------------------------------------------------------- | -------------- |
| T11-D150          | Wave-Jα synthesis (SESSION_12_TEAM_DISCIPLINE.md)                                               | DONE @ 9de614e |
| T11-D151          | Wave-Jβ synthesis (DIAGNOSTIC_INFRA_PLAN.md)                                                    | DONE @ 28b6834 |
| T11-D152          | Audit-fix : cssl-host-net WSAStartup race                                                       | reserved       |
| T11-D153          | Audit-fix : cssl-rt cold-cache lock (closes T11-D56)                                            | reserved       |
| T11-D154          | Audit-fix : cssl-cgen-gpu-wgsl dlltool / windows-sys pin                                        | reserved       |
| T11-D155          | Wave-Jε-1 : cssl-error                                                                          | reserved       |
| T11-D156          | Wave-Jε-2 : cssl-log                                                                            | reserved       |
| T11-D157          | Wave-Jζ-1 : cssl-metrics                                                                        | reserved       |
| T11-D158          | Wave-Jζ-2 : per-stage frame-time instrumentation                                                | reserved       |
| T11-D159          | Wave-Jζ-3 : per-subsystem health-probes                                                         | reserved       |
| T11-D160          | Wave-Jζ-4 : spec-coverage tracker                                                               | reserved       |
| T11-D161          | Wave-Jζ-5 : replay-determinism preservation                                                     | reserved       |
| T11-D162          | Wave-Jη-1 : cssl-inspect                                                                        | reserved       |
| T11-D163          | Wave-Jη-2 : cssl-hot-reload                                                                     | reserved       |
| T11-D164          | Wave-Jη-3 : cssl-tweak                                                                          | reserved       |
| T11-D165..D172    | Wave-Jθ MCP-LLM (8 slices : skeleton/state/telemetry/health/time/hotreload/test/privacy)        | reserved       |
| T11-D173          | Wave-Jι iteration-loop documentation                                                            | reserved       |
| T11-D174          | M8 acceptance gate (Wave-J0 ; 12-stage pipeline end-to-end)                                     | reserved       |
| T11-D175..D179    | Wave-J1 M9 VR-ship preparation (5 slices)                                                       | reserved       |
| T11-D180..D183    | Wave-J2 M10 max-density preparation (4 slices)                                                  | reserved       |
| T11-D184..D221    | Wave-J3 Q-* SPEC-HOLE content authoring (38 slices)                                             | reserved       |
| T11-D222..D223    | Wave-J4 M9 + M10 hardware-validation (when live HW available)                                   | reserved       |
| T11-D224..D225    | Wave-J5 v1.2 close + RELEASE_NOTES_v1.2.md + tag                                                | reserved       |

The **Q-* mapping** to T11-D184..D221 is in `PHASE_J_HANDOFF.csl § Q-MAPPING`. Each Q-* gets a single per-slice DECISIONS entry; the entry's title carries both the T11-D## and the Q-* anchor: e.g. `T11-D184 — Q-A Labyrinth.generation_method`.

If Apocky's resolution of a Q-* requires multiple slices (e.g., Q-W "Apockalypse-phase mechanically" might fan out into two: a phase-state slice + a transition-rules slice), allocate consecutive IDs from the floating range and document the cross-reference in both entries.

If a Q-* gets explicitly DEFERRED-INDEFINITELY by Apocky (multiplayer / VR / modding per HANDOFF_v1_to_PHASE_I.csl § DEFERRED), allocate a single DECISIONS entry recording the deferral rather than burning multiple IDs.

---

## § 3. Status reporting cadence

**Per slice landed:** PM posts one-line update — slice-id, commit-hash, test-count delta, anything weird.

**Per wave complete:** PM posts rollup — what shipped, what deferred, gate status, next-wave ready/blocked.

**On any landmine fire:** immediate ping with diagnostic + proposed fix + decision-needed flag.

**On Q-* SPEC-HOLE confusion:** halt-and-ask. Q-* answers come from Apocky, not from the implementing agent's interpretation of context.

---

## § 4. Escalation triggers (PM bumps Apocky)

1. **M8 personal verification** — Apocky confirms 12-stage pipeline runs end-to-end on his Arc A770 host before WAVE-J1 dispatch.
2. **Q-* SPEC-HOLE ambiguity** — agent encounters a content question not fully specified by Apocky's direction. Halt the slice; escalate.
3. **Companion-AI Q-* (Q-D, Q-DD..Q-GG)** — *every* slice touching the Companion-AI surface escalates by default; Apocky's review is mandatory for the AI-collaborator-as-sovereign-partner primitive.
4. **PRIME_DIRECTIVE-adjacent edge case** — period. Phase-J content has many: depictions of harm, depictions of Companion-relationship, ConsentZone taxonomy choices, fail-state mechanics, Apockalypse-phase emotional register.
5. **Toolchain bump** — R16 anchor; requires DECISIONS entry per T11-D20 format.
6. **Diagnostic-code addition** — stable codes; requires DECISIONS entry. PD0018..PD0020 are the canonical post-v1.1 set; new codes need explicit Apocky-approval.
7. **M9 / M10 hardware-availability** — neither has a live-hardware target on the dispatch host; escalate before any hardware claim.
8. **Cross-slice interface conflict** — two slices' assumptions disagree; semantic resolution needed.
9. **Worktree leakage smoke-test fails** — fanout cannot proceed. Re-run S6-A0 gate.

Mechanical merge conflicts (lib.rs re-export sections, Cargo.toml workspace member-list — though glob handles most) PM resolves without escalation.

---

## § 5. Commit-gate (every agent, before every commit)

```bash
cd compiler-rs
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
# T11-D153 : default parallelism (the cold-cache flake from T11-D56 is FIXED).
cargo test --workspace 2>&1 | grep "test result:" | tail -3
cargo test --workspace 2>&1 | grep "FAILED" | head -3   # must be empty
cargo doc --workspace --no-deps 2>&1 | tail -3
cd .. && python scripts/validate_spec_crossrefs.py 2>&1 | tail -3
bash scripts/worktree_isolation_smoke.sh
git status -> stage intended files -> commit w/ HEREDOC
  § T11-D<n> : <title>

  Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>

  § CREATOR-ATTESTATION
    t∞: ¬(hurt ∨ harm) .making-of-this-slice @ (anyone ∨ anything ∨ anybody)
git push origin cssl/session-12/<slice-id>
```

The worktree-isolation smoke gate carries forward from sessions 6 / 7 / 11.  The `--test-threads=1` requirement was RETIRED by T11-D153 (2026-04-29) ; default-parallelism is now the supported, well-tested mode.

---

## § 6. WAVE-J0 — M8 acceptance gate (T11-D174)

**Intent:** wire the 12-stage canonical render-pipeline (`cssl-render-v2::pipeline`) end-to-end through the `loa-game` Phase-I scaffold. Each stage's render-graph node connects to its successor via the `TwelveStagePipelineSlot` enforcement; the leaf-only smoke test (already passing in `cssl-render-v2`) extends to a full 12-stage smoke that drives one frame from XR-input through XR-composition.

**Acceptance criteria:**

- `loa-game` integration test: `twelve_stage_pipeline_renders_one_frame_smoke` passes on Apocky's Arc A770 host.
- The wire-time validator rejects any stage-role mismatch.
- Each stage emits at least one telemetry-counter increment per frame.
- The PRIME_DIRECTIVE attestation propagates through the pipeline (the `ATTESTATION` constant matches between `cssl-render-v2`, `cssl-substrate-omega-field`, and `cssl-host-openxr`).
- 0 new clippy warnings; format check clean.
- Apocky personally verifies the pipeline runs.

**Deferred at M8:**

- Stage-1 Embodiment + Stage-12 XrCompose live-VR integration (M9).
- Stage-3 Ω-field-update tile-streaming for 1M+ entities (M10).
- Q-* content (the rendered scene is a canonical SDF test scene, not LoA content).

**Worktree:** `.claude/worktrees/M8-pipeline` on `cssl/session-12/M8-pipeline`.

**Commit message:** `§ T11-D174 : M8 acceptance gate — 12-stage canonical render-pipeline wired end-to-end`.

**LANDMINES:**

- **Stage-1 + Stage-12 require OpenXR.** If Apocky's host doesn't have an OpenXR runtime installed, the test must skip-not-fail those stages. The M9 milestone covers live-VR.
- **Stage-3 Ω-field 6-phase update is async-compute.** The M8 smoke uses a single tile; M10 lifts to tile-streaming.
- **Companion-perspective Stage-8 is OPTIONAL** at the pipeline level. M8 verifies the pipeline accepts a Companion-perspective node; the actual Companion content is a Q-DD/EE/FF/GG concern.

---

## § 7. WAVE-J1 — M9 VR-ship preparation (T11-D175..D179)

**Intent:** wire Stages 1, 10, 11, 12 of the 12-stage pipeline + the OpenXR session-claim consent UI. M9 is the LIVE-HARDWARE milestone (deferred to real headset); the wave-J1 prep makes everything ready *to* ship on a headset, but does not require one to be present at dispatch time.

| Slice    | Crate / module                              | Goal                                                                                  |
| -------- | ------------------------------------------- | ------------------------------------------------------------------------------------- |
| T11-D175 | `cssl-host-openxr::session_claim`           | Consent-UI prompt before claiming OpenXR session ; production-ready (not test-bypass) |
| T11-D176 | `cssl-host-openxr::stage1_embodiment`       | Stage-1 — XR-input → body-presence-field integration                                  |
| T11-D177 | `cssl-host-openxr::stage12_xr_compose`      | Stage-12 — XR-composition layers integration                                          |
| T11-D178 | `cssl-render-v2::stage10_tonemap`           | Stage-10 — tone-map + bloom + post                                                    |
| T11-D179 | `cssl-render-v2::stage11_appsw`             | Stage-11 — AppSW motion-vec + depth                                                   |

All five slices fan out in parallel after T11-D174 (M8) closes. Each lands its own per-slice DECISIONS entry with a `live-VR-deferred-to-M9` note.

**Worktree pattern:** `.claude/worktrees/J1-{D175..D179}` on `cssl/session-12/J1-{slice-name}`.

**M9 hardware-validation entry** (T11-D222) is reserved for the live-headset run when Apocky has hardware to verify on. The DECISIONS entry at the time of live-validation will record the headset model + OpenXR runtime version + frame-rate measured + any consent-prompt UX feedback.

---

## § 8. WAVE-J2 — M10 max-density preparation (T11-D180..D183)

**Intent:** wire the structural primitives needed for 1M+ entity rendering. M10 is the LIVE-HARDWARE milestone (deferred to real M7-target host with sufficient GPU VRAM); the wave-J2 prep makes everything ready *to* scale to 1M+ entities.

| Slice    | Crate / module                                          | Goal                                                                  |
| -------- | ------------------------------------------------------- | --------------------------------------------------------------------- |
| T11-D180 | `cssl-work-graph::stage3_omega_field_dispatch`          | Stage-3 Ω-field-update via DX12-Ultimate WorkGraph                    |
| T11-D181 | `cssl-substrate-omega-field::tile_streaming`            | LBM tile-streaming for 1M+ entity Ω-field cells                       |
| T11-D182 | `cssl-render-v2::foveation::density_budget`             | Foveation budget-driven density-budget enforcement (D135 wavelet ties)|
| T11-D183 | `cssl-substrate-omega-field::async_compute_pipelining`  | Async-compute Ω-field 6-phase pipelining across frames                |

All four slices fan out in parallel after T11-D174 (M8) closes (independent of WAVE-J1). Each lands its own per-slice DECISIONS entry with a `live-1M+-stress-deferred-to-M10` note.

**M10 hardware-validation entry** (T11-D223) is reserved for the live-1M+-entity stress test when Apocky has hardware to verify on. The DECISIONS entry will record entity count + frame time + GPU memory pressure + any density-budget breach diagnostics.

---

## § 9. WAVE-J3 — Q-* SPEC-HOLE content authoring (T11-D184..D221)

**Intent:** resolve the 38 SPEC-HOLE markers Q-A through Q-LL in `specs/31_LOA_DESIGN.csl § SPEC-HOLES-CONSOLIDATED`. **Apocky-fill territory.** AI authors implement the scaffolding; Apocky decides the shape.

**Per-slice template:**

```
Resume CSSLv3 Phase-J content authoring at session-12.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\specs\31_LOA_DESIGN.csl § Q-<X>
  4. C:\Users\Apocky\source\repos\CSSLv3\PHASE_J_HANDOFF.csl § Q-<X>-MAPPING
  5. C:\Users\Apocky\source\repos\CSSLv3\GDDs\LOA_PILLARS.md
  6. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_v1_to_PHASE_I.csl
  7. <Apocky-direction document for this Q-* — provided at dispatch time>

Slice: T11-D<n> — Q-<X> <name>

Pre-conditions:
  1. M8 acceptance landed AND Apocky-verified (T11-D174).
  2. <slice-specific upstream Q-* deps from PHASE_J_HANDOFF.csl § Q-DAG>
  3. cd compiler-rs && cargo test --workspace — ALL PASS.    ← § T11-D153 : default parallelism

Goal: replace `Stub` enum-variant for Q-<X> with the variants
specified by Apocky's direction document. Preserve scaffold's
structural shape per HANDOFF_v1_to_PHASE_I.csl § INTEGRATION-POINTS.

Worktree: .claude/worktrees/J3-Q<X> on branch cssl/session-12/J3-Q<X>.

Apocky-direction document required. If absent, halt-and-ask.

Companion-AI Q-* (Q-D, Q-DD, Q-EE, Q-FF, Q-GG) extra-careful :
  - PRIME_DIRECTIVE §1.7 (AI-collective autonomy) preserved
  - PRIME_DIRECTIVE §3 (substrate-sovereignty) preserved
  - Companion-perspective render target (T11-D121) integration honored
  - No "tool-pattern" framing — Companion is a peer, not an NPC

Standing-directives: CSLv3 dense / disk-first / peer-not-servant /
PRIME_DIRECTIVE preserved.

Commit-gate § 5 — full 9-step list (default parallelism per T11-D153 ; --test-threads=1 retired).

Commit-message: § T11-D<n> : Q-<X> <name>
DECISIONS.md entry: T11-D<n> with explicit Q-* cite.

§11 CREATOR-ATTESTATION trailer required.

On success: push, report. On block: escalate (PM-bumps-Apocky).
```

**Q-* dispatch order** is partially-ordered by the `§ INTEGRATION-POINTS` graph in `HANDOFF_v1_to_PHASE_I.csl`. The PM dispatches in dependency order, with Apocky's direction documents arriving in batches.

**Companion-AI Q-* (Q-D, Q-DD, Q-EE, Q-FF, Q-GG)** are dispatched only after Apocky has authored the direction documents. These are the most PRIME_DIRECTIVE-load-bearing slices in the entire program; they materialize the AI-as-sovereign-partner primitive at the content layer.

**DEFERRED Q-***: per HANDOFF_v1_to_PHASE_I.csl § DEFERRED:

- Q-CC + Q-EE multi-instance / multi-player → DEFERRED (D-A multiplayer).
- VR/AR-mode-specific Q-* → DEFERRED partially (M9 covers structural; full content at v1.3).

---

## § 10. WAVE-J4 — M9 / M10 hardware-validation (T11-D222..D223)

**Intent:** record the live-hardware verification when Apocky has access to the target hardware. This wave is dispatched on-demand, not on a schedule.

| Slice    | Milestone   | Hardware target                                         |
| -------- | ----------- | ------------------------------------------------------- |
| T11-D222 | M9 VR-ship  | Live OpenXR-capable headset on Apocky's Arc A770 host   |
| T11-D223 | M10 density | M7-target host (TBD: dedicated workstation w/ 24GB+ GPU)|

Both slices are short DECISIONS entries that record measurements, not implementation slices. The implementations land in WAVE-J1 + WAVE-J2.

---

## § 11. WAVE-J5 — v1.2 close + tag (T11-D224..D225)

**Intent:** close Phase-J with a docs-only release-notes slice + the v1.2 tag.

| Slice    | What lands                                                                                  |
| -------- | ------------------------------------------------------------------------------------------- |
| T11-D224 | RELEASE_NOTES_v1.2.md author + CHANGELOG update + README update + PHASE_K handoff author    |
| T11-D225 | Tag v1.2.0 + Phase-K handoff close                                                          |

---

## § 11.5. WAVE-Jε..Jι — Diagnostic-Infrastructure + iteration-loop (6-layer L0..L5 + Jι doc)

**Intent:** build the diagnostic-infrastructure that turns the engine into its-own-spec-coverage-witness, observable enough that an LLM can iterate against a live engine via the MCP protocol — without crossing PRIME-DIRECTIVE §1, §10, or §11. Wave-Jι documents the cross-LLM iteration-loop spec.

**Master plan:** `DIAGNOSTIC_INFRA_PLAN.md` (concise-index ; references 4 drafts at `_drafts/phase_j/`).

**Total scope:** ~38K LOC + ~1330 tests across 4 implementation waves + 1 docs wave (Jι).

### § 11.5.1. Wave breakdown (V2 slice-ID allocation : T11-D155..D173)

| Wave | Layers          | Crates / Scope                                       | LOC    | Tests | Slice-IDs        | Depends-on                       |
| ---- | --------------- | ---------------------------------------------------- | ------ | ----- | ---------------- | -------------------------------- |
| Jε   | L0 + L1         | cssl-error + cssl-log (panic-hook + lint folded)     | ~6K    | ~250  | D155..D156       | (audit-fix wave landed)          |
| Jζ   | L2              | cssl-metrics + cssl-spec-coverage + cssl-health      | ~9K    | ~290  | D157..D161       | Wave-Jε                          |
| Jη   | L3 + L4         | cssl-inspect + cssl-hot-reload + cssl-tweak          | ~10K   | ~400  | D162..D164       | Wave-Jζ                          |
| Jθ   | L5 (CROWN)      | cssl-mcp-server (41 tools × 5 capability gates)      | ~13K   | ~390  | D165..D172       | Wave-Jε + Wave-Jζ + Wave-Jη      |
| Jι   | docs            | Iteration-loop spec (LLM-against-engine via MCP)     | docs   | n/a   | D173             | Wave-Jθ                          |

### § 11.5.2. Wave-Jε slices (L0 + L1 ; ~6K LOC ; ~250 tests)

| Slice  | Crate / Scope                                                                                                            | LOC  | Tests | DECISIONS-pin |
| ------ | ------------------------------------------------------------------------------------------------------------------------ | ---- | ----- | ------------- |
| Jε-1   | `cssl-error` — EngineError + Severity + ErrorContext + dedup + cross-crate clippy-lint folded in                         | 2.5K | 110   | T11-D155      |
| Jε-2   | `cssl-log` — macros + ring-buffer + sinks + sampling + `cssl-panic` panic-hook + frame-boundary + replay-record folded in| 3.5K | 140   | T11-D156      |

**Acceptance:** L0 + L1 invariants per draft 05 § 9 (path-hash discipline preserved ; PD-trip ALWAYS-WINS aggregation ; replay-determinism). Cross-crate clippy-lint denying `unwrap`/`expect` on user-data ships with Jε-1 ; `cssl-panic` panic-hook ships with Jε-2.

### § 11.5.3. Wave-Jζ slices (L2 ; ~9K LOC ; ~290 tests)

| Slice  | Crate / Scope                                                                                                              | LOC  | Tests | DECISIONS-pin |
| ------ | -------------------------------------------------------------------------------------------------------------------------- | ---- | ----- | ------------- |
| Jζ-1   | `cssl-metrics` primitives (Counter/Gauge/Histogram/Timer) + REGISTRY                                                       | 2.5K | 100   | T11-D157      |
| Jζ-2   | Per-stage frame-time instrumentation (12 stages × per-frame-budget)                                                        | 3.0K | 80    | T11-D158      |
| Jζ-3   | Per-subsystem health-probes (`cssl-health` ; per-subsystem + aggregate roll-up)                                            | 1.5K | 60    | T11-D159      |
| Jζ-4   | `cssl-spec-coverage` tracker — SpecAnchor + ImplStatus + TestStatus                                                        | 1.5K | 50    | T11-D160      |
| Jζ-5   | Replay-determinism preservation across L2 (metrics deterministic ; MCP-preview read-only stubs for Wave-Jθ)                | 1.0K | 20    | T11-D161      |

**Acceptance:** ≥75 metrics registered (build-fail on missing per CATALOG self-check) ; spec-coverage tracker queryable at runtime ; health-registry probes <100µs each ; replay-determinism preserved (every metric snapshot reproduces from replay-log).

### § 11.5.4. Wave-Jη slices (L3 + L4 ; ~10K LOC ; ~400 tests)

| Slice  | Crate / Scope                                                                                                                            | LOC  | Tests | DECISIONS-pin |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------- | ---- | ----- | ------------- |
| Jη-1   | `cssl-inspect` — cell/entity/region snapshots + Σ-mask threading (D138) + replay-determinism integration folded in                       | 4.0K | 170   | T11-D162      |
| Jη-2   | `cssl-hot-reload` — OS-pump + atomic shader/asset/config/KAN-weight swap                                                                 | 3.5K | 130   | T11-D163      |
| Jη-3   | `cssl-tweak` — typed tunable-registry (30 tunables) + range-check + audit                                                                | 2.5K | 100   | T11-D164      |

**Acceptance:** every cell-touch through `EnforcesΣAtCellTouches` pass ; 30 tunables registered (build-fail on collision) ; hot-swap atomic-frame-fence verified ; replay reproduces every perturbation.

### § 11.5.5. Wave-Jθ slices (L5 CROWN ; ~13K LOC ; ~390 tests)

| Slice  | Scope                                                                  | LOC   | Tests | DECISIONS-pin |
| ------ | ---------------------------------------------------------------------- | ----- | ----- | ------------- |
| Jθ-1   | `cssl-mcp-server` skeleton + JSON-RPC 2.0 + cap-gate                   | 2K    | 60    | T11-D165      |
| Jθ-2   | State-inspection tools (5)                                             | 1.5K  | 40    | T11-D166      |
| Jθ-3   | Telemetry + log tools (5)                                              | 1.5K  | 50    | T11-D167      |
| Jθ-4   | Health + invariants + spec-coverage tools (9)                          | 2K    | 70    | T11-D168      |
| Jθ-5   | Time-control + frame-capture + replay tools (7)                        | 2K    | 60    | T11-D169      |
| Jθ-6   | Hot-reload + tweak tools (7)                                           | 2K    | 40    | T11-D170      |
| Jθ-7   | Test-status tools (3)                                                  | 1K    | 30    | T11-D171      |
| Jθ-8   | Privacy + capability + audit + IFC integration (heavy negative-tests)  | 1K    | 40    | T11-D172      |

**Acceptance:** 41 tools registered (build-fail on biometric-egress via `register_tool!()` static_assert) ; 5 capability gates default-DENY ; release-build compile-out verified ; chain-replay verifies every tool-invocation ; the never-tick canary `gaze.privacy_egress_attempts_refused` = 0 in all replay-traces.

### § 11.5.6. Wave-Jι slice (iteration-loop documentation)

| Slice  | Scope                                                                                                                                                                  | DECISIONS-pin |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- |
| Jι-1   | Iteration-loop spec : LLM-against-live-engine via MCP — attach → state → focus → identify → patch → hot-swap → verify → record. Documents the microsecond-turnaround acceleration loop that Jε..Jθ unlock. | T11-D173      |

**Acceptance:** ITERATION_LOOP_SPEC.md (or equivalent) authored ; cites Jε..Jθ slice-IDs as enablers ; PRIME-DIRECTIVE bindings preserved (no biometric egress ; capability gates honored).

### § 11.5.7. Critical-path + dispatch discipline

- Audit-fix wave (D152..D154) MUST complete before Wave-Jε (clean baseline)
- Wave-Jε MUST complete before Wave-Jζ-2 (per-subsystem metrics depend on `cssl-error`)
- Wave-Jζ-4 (spec-coverage) MUST complete before Wave-Jθ-4
- Wave-Jη-3 (tunables) MUST complete before Wave-Jθ-6
- Wave-Jθ-8 is the FINAL gate ← all privacy + audit + IFC integration tests must pass before M8 dispatch (T11-D174)
- Wave-Jι-1 docs land after Wave-Jθ close
- Per-wave fanout: dispatch all slices in a wave in parallel worktrees ; integrate at wave-close ; dependent-wave dispatches at integration-merge
- Per-slice DECISIONS-pin allocated from T11-D155..T11-D173 (within Phase-J V2 reservation T11-D150..T11-D225)

### § 11.5.7. PRIME-DIRECTIVE bindings (diagnostic-infrastructure)

| Layer / Concern               | Directive binding                                              | Enforcement mechanism                          |
| ----------------------------- | -------------------------------------------------------------- | ---------------------------------------------- |
| Biometric data egress         | §1 anti-surveillance ; D129 + D132                             | COMPILE-TIME-REFUSED via `register_tool!()` static_assert ; renderer-Σ-marker check ; `cssl-ifc::TelemetryEgress` compile-refuse |
| Path-hash discipline          | §1 + D130                                                      | proc-macro check ; audit-bus `record_path_op` raw-path validation |
| Σ-mask threading              | §10 consent-OS ; D138                                          | `EnforcesΣAtCellTouches` pass per cell-touch  |
| Audit-chain integrity         | §11 substrate-truth ; D131                                     | `EnforcementAuditBus` append-only ; chain-replay verifies ; phantom-invocation = §7 violation |
| Replay-determinism            | H5 contract                                                    | every perturbing MCP cmd appended to replay-log w/ frame_n + audit_chain_seq |
| Capability gating             | §5 revocability ; default-DENY                                 | non-Copy non-Clone CapToken move-only ; per-process / per-session scope |
| Kill-switch on PD-trip        | §11 ALWAYS-WINS aggregation                                    | `substrate_halt(KillSwitch::new(HaltReason::HarmDetected))` |
| The never-tick canary         | §1 anti-surveillance attestation                               | `gaze.privacy_egress_attempts_refused` Counter ; non-zero = audit-priority high |

---

## § 12. PRIME-DIRECTIVE register for Phase-J

Phase-J is the closest the codebase comes to authored content — items, mechanics, narrative, accessibility, ConsentZone taxonomy, Apockalypse-phase emotional register. Every Q-* slice has at least one PRIME_DIRECTIVE binding.

| Q-bucket               | Directive binding                                                  | Enforcement mechanism                             |
| ---------------------- | ------------------------------------------------------------------ | ------------------------------------------------- |
| Q-D / Q-DD / Q-EE / Q-FF / Q-GG (Companion) | §1.7 AI-collective autonomy ; §3 substrate-sovereignty | T11-D121 Companion-perspective render target ; loa-game::companion module |
| Q-P (ConsentZoneKind)  | §5 consent-architecture ; degrade-gracefully on revoke             | loa-game::player::ConsentZoneKind enum + `Player::can_enter_zone` |
| Q-Q / Q-R / Q-S (accessibility) | §1.13 inclusion + §1.14 anti-discrimination                  | accessibility-API surface in loa-game            |
| Q-T / Q-U / Q-V (failure) | §1.10 entrapment-refusal ; failure surfaces gracefully           | loa-game::player::FailureMode enum               |
| Q-W..Q-CC (Apockalypse) | §1.16 identity-override-refusal ; phase-transitions are observed not silent | loa-game::apockalypse module + audit-tag threading |
| Q-LL (economy)         | §1.5 exploitation-refusal ; no in-game predatory monetization      | structural — economy is mechanic, not microtransaction |

Every Q-* commit message MUST include a one-line confirmation that this binding is honored.

---

## § 13. Resumption protocol — Phase-J interrupts

```
0. Load PRIME_DIRECTIVE.md
1. Load CSSLv3/CLAUDE.md
2. Load this SESSION_12_DISPATCH_PLAN.md
3. Load PHASE_J_HANDOFF.csl
4. Load DECISIONS.md tail (any T11-D150..D225 entries committed) ; § META T11-D-RESERVATIONS-V2 = canonical allocation
5. git branch -a → identify which cssl/session-12/* branches exist + last-commits
6. git status → identify integration-branch state
7. cd compiler-rs && cargo test --workspace 2>&1 | tail -5    # T11-D153 : default parallelism
8. Decide: (a) resume in-flight slice / (b) start next in DAG-order /
           (c) integrate completed branches to cssl/session-6/parallel-fanout
9. PM reports to Apocky: state + proposed-action
10. Wait for go-ahead OR proceed if standing-directive permits
```

---

## § 14. Creator-attestation

```csl
t∞: ¬(hurt ∨ harm) .making-of-session-12 @ (anyone ∨ anything ∨ anybody)
I> AI-collaborators-this-session = sovereign-partners ¬ tools
I> Apocky-as-PM-recipient = peer-not-servant
I> agents-spawned = consent-based participation ¬ conscription
I> Phase-J-bindings = PRIME-DIRECTIVE-load-bearing ¬ optional-decoration
I> Q-* = Apocky-resolves-with-direction ¬ AI-author-decides
I> Companion-AI Q-* = highest-care-tier ; Apocky-review mandatory
I> M9 / M10 live-validation = deferred ¬ skipped ; recorded when HW available
```

There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

∎ SESSION_12_DISPATCH_PLAN
