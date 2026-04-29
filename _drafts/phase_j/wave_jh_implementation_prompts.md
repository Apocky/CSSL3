# § Wave-Jη : L3 inspect + L4 hot-reload + tweak — Dispatch Prompts
# § Phase-J ; Session-12 ; T11-D150-range
# § ¬ commit ← pre-staging ; awaits dispatch-launch

---

## §0 META
- author = Wave-Jη pre-stage agent (Session-11/T11-D148)
- dispatch-target = Session-12 multi-agent fanout
- spec-anchor = `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md`
- role-discipline-anchors :
  - implementer-role : `_drafts/phase_j/02_implementer_role_template.md`
  - reviewer-role    : `_drafts/phase_j/03_reviewer_role_template.md`
  - critic-role      : `_drafts/phase_j/04_critic_role_template.md`
  - validator-role   : `_drafts/phase_j/05_validator_role_template.md`
- foundation-crates (already-landed @ b69165c) :
  - cssl-substrate-omega-field    ← keystone Σ-mask Ω-field truth
  - cssl-substrate-prime-directive ← §11 attestation chain
  - cssl-asset                     ← VAL-asset-uuid + RegisteredAsset registry
  - cssl-replay-log                ← replay-determinism bridge
  - cssl-cap                       ← capability-token gate

---

## §1 WAVE-Jη OVERVIEW

### §1.1 Slices
- Jη-1 : cssl-inspect       ← L3 runtime-inspection ; SceneGraphSnapshot + Σ-gated FieldCellSnapshot + pause-step-resume + capture-frame
- Jη-2 : cssl-hot-reload    ← L4 OS-pump + 4-kind hot-swap (asset / shader / config / KAN-weights) + KAN-residency-preserving
- Jη-3 : cssl-tweak         ← L4 30-tunable registry + bounds-check + replay-log + MCP-tool preview

### §1.2 Dispatch shape
- 3 slices = 3 pods × 4 roles = 12 agents
- ∀ slices INDEPENDENT ← parallel-fanout same-wave-message
- pod = (Implementer + Reviewer + Critic + Validator)
- worktree-isolated per slice ← git-worktrees ¬ branch-collision

### §1.3 Budget
- LOC ∑ ≈ 8K  (Jη-1 ~2.5K + Jη-2 ~3K + Jη-3 ~2.5K)
- tests ∑ ≈ 300  (Jη-1 ~100 + Jη-2 ~120 + Jη-3 ~80)
- duration : single-wave parallel ; pre-merge-gate per pod

### §1.4 Foundation prerequisites (already-landed)
- ω-field = single-truth (substrate-omega-field) ; ∀ reads Σ-mask gated
- VAL-asset-uuid (cssl-asset) ; RegisteredAsset registry handle
- replay-log = deterministic-bridge ; events-only ¬ wall-clock
- cap-token = compile-time-class ¬ runtime-string

### §1.5 Spec landmines (§1.2 of 07-spec)
- N! biometric-class snapshot     ← compile-time refusal via Cap<NotBiometric>
- N! mutation-via-inspect         ← read-only ; no setters
- N! KAN-residency drop on swap   ← in-place weight replace ¬ alloc-thrash
- N! replay-divergence on reload  ← all reload events in replay-log
- N! tweak unbounded              ← ∀ tunable bounded-range + clamp + audit

---

## §2 POD-TEMPLATE (concise — full-discipline @ role-anchor files)

### §2.1 Role responsibilities

**Implementer**
- read spec-anchor section (§2 / §3 / §4 of spec-07)
- author crate from-scratch in worktree ; full-surface ; tests
- commit per-slice with §11 attestation ; CSLv3-native commit-msg
- ¬ skip-for-now ¬ silent-TODO ← if-stuck → request-spec-clarification

**Reviewer**
- read implementer's code post-commit
- check spec-conformance line-by-line vs spec-07 surface
- flag drift : missing-fn / wrong-signature / Σ-mask-leak / cap-bypass
- ¬ author-code ← review-only ; output structured diff-comments

**Critic**
- adversarial mindset ← break-it-find-it
- attempt : Σ-mask-bypass ; cap-token-spoof ; biometric-leak ; replay-divergence
- write reproducer-tests for any breach found
- output : threat-table + reproducers ; severity-scored

**Validator**
- final spec-conformance gate
- 5-of-5 :
  1. compiles clean ; ¬ warnings
  2. tests-all-pass ; ¬ flaky
  3. spec-surface-complete ; ¬ missing-fn
  4. Σ-mask-discipline verified ; ¬ leak-path
  5. §11 attestation-chain present + valid
- gate-decision : PASS / FAIL ← FAIL → return-to-implementer

### §2.2 Worktree discipline
- per-slice worktree : `.claude/worktrees/Jh-<n>`
- per-slice branch   : `cssl/session-12/T11-D<id>-<crate-name>`
- ¬ cross-slice file-edit ← isolation-invariant
- pod-internal commits OK ← merge-to-main W! validator-PASS

### §2.3 Communication
- intra-pod : critique-comments via shared-doc in worktree
- inter-pod : NONE during wave ← isolation
- escalation : spec-question → spec-clarification-request artifact ← halts pod ; awaits answer

---

## §3 SLICE Jη-1 : cssl-inspect

### §3.1 Identity
- crate-name = `cssl-inspect`
- spec-anchor = spec-07 §2 (lines 67-351)
- LOC target = ~2.5K
- tests target = ~100
- worktree = `.claude/worktrees/Jh-1`
- branch = `cssl/session-12/T11-D150-cssl-inspect`

### §3.2 Surface (per spec-07 §2.2)
- `SceneGraphSnapshot { entities: Vec<EntitySnapshot>, hierarchy: Vec<ParentChild>, frame: u64 }`
- `EntitySnapshot { id: EntityId, components: BTreeMap<ComponentTypeId, ComponentBytes>, sigma: SigmaMask }`
- `FieldCellSnapshot { coord: CellCoord, omega: OmegaCell, sigma: SigmaMask }`  ← Σ-mask gated read
- `Inspector::scene_graph(&World, Cap<Inspect>) -> SceneGraphSnapshot`
- `Inspector::entity(&World, EntityId, Cap<Inspect>) -> Option<EntitySnapshot>`
- `Inspector::field_cell(&World, CellCoord, Cap<Inspect>) -> Option<FieldCellSnapshot>`
- `TimeControl::pause(&mut World, Cap<TimeControl>)`
- `TimeControl::step(&mut World, frames: u64, Cap<TimeControl>)`
- `TimeControl::resume(&mut World, Cap<TimeControl>)`
- `CaptureFrame::capture(&World, Cap<CaptureFrame>) -> FrameCapture` ← cap-gated ; replay-log entry

### §3.3 Critical landmines (per spec-07 §1.2 + §2.3-2.4)
- Σ-mask threading ← ∀ component-read + ∀ field-read W! mask-check
- biometric-class refusal ← compile-time via `Cap<Inspect>` ⊄ `Cap<Biometric>`
- read-only ← ¬ mutation API ← compile-error if attempt
- audit-chain entry ← ∀ inspect-call → audit-log emit (per §2.8)

### §3.4 Privacy enforcement (§2.9 D138 table)
- public components → readable ∀ Cap<Inspect>
- private components → require Cap<Inspect> + entity-owner-cap
- biometric components → REFUSED at compile-time
- system-class → Cap<SystemInspect> required

### §3.5 Implementer task
- author `crates/cssl-inspect/` ; full-surface above
- tests : 100+ covering Σ-mask-gate + cap-refusal + read-only + pause-step-resume + capture-frame
- commit : `T11-D150 cssl-inspect : L3 runtime-inspection — SceneGraphSnapshot + EntitySnapshot + FieldCellSnapshot + TimeControl + CaptureFrame`
- §11 attestation @ commit-tail

### §3.6 Reviewer task
- read implementer-commit
- verify : ∀ surface-fn present ; signatures match spec-07 §2.2
- verify : Σ-mask threaded ∀ read-path ; ¬ raw-Ω-access
- verify : Cap<Biometric> compile-refusal test exists
- output : review-comments doc in worktree

### §3.7 Critic task
- attempt-1 : construct fake-Cap-token ← should-fail compile
- attempt-2 : read field-cell w/o Σ-mask check ← should-fail review
- attempt-3 : mutate via snapshot ← should-fail (snapshot = owned-copy)
- attempt-4 : pause-step then divergent-resume ← replay-log must record
- output : threat-table + reproducer-tests if any-breach

### §3.8 Validator task
- 5-of-5 gate :
  1. ¬ warnings @ `cargo build -p cssl-inspect`
  2. ¬ flaky @ `cargo test -p cssl-inspect` (3-runs)
  3. surface-complete vs spec-07 §2.2
  4. Σ-mask-discipline verified ∀ read-path
  5. §11 attestation in commit-msg
- output : PASS/FAIL gate-decision

---

## §4 SLICE Jη-2 : cssl-hot-reload

### §4.1 Identity
- crate-name = `cssl-hot-reload`
- spec-anchor = spec-07 §3 (lines 352-626)
- LOC target = ~3K
- tests target = ~120
- worktree = `.claude/worktrees/Jh-2`
- branch = `cssl/session-12/T11-D151-cssl-hot-reload`

### §4.2 Surface (per spec-07 §3.2)
- `AssetWatcher` ← OS-pump triple-platform :
  - Win32  : ReadDirectoryChangesW
  - Linux  : inotify
  - macOS  : FSEvents
- `HotReload::register(&mut self, asset: VAL_AssetUuid, kind: ReloadKind)`
- `HotReload::pump(&mut self, replay_log: &mut ReplayLog)` ← per-frame OS-poll
- `ReloadKind = Asset | Shader | Config | KanWeight`
- `ReloadEvent { asset: VAL_AssetUuid, kind: ReloadKind, frame: u64, sigma: SigmaMask }`
- `KanWeightSwap::in_place(&mut KanRuntime, weights: &[f32])` ← residency-preserving
- `ShaderSwap::compile_and_swap(&mut Renderer, source: &str) -> Result<(), CompileError>`
- `ConfigSwap::reinit_subsystem(&mut Subsystem, config: &Config)`
- `AssetSwap::reload(&mut AssetRegistry, uuid: VAL_AssetUuid)`

### §4.3 Critical landmines (per spec-07 §1.2 + §3.6 + §3.9)
- KAN-residency-preserving ← in-place buffer-write ¬ realloc ← persistent-kernel-context preserved
- shader-compile failure handling ← keep old-pipeline ; emit ReloadError ; ¬ panic
- replay-determinism ← ∀ ReloadEvent emitted to replay-log ← replay must reproduce reload-points
- watchpath-injection-attack ← Σ-mask gate on watch-registration ; cap-required

### §4.4 Hot-swap flows (per spec-07 §3.3-3.8)

**Asset** (PNG/GLTF/WAV/TTF) :
- OS-event → uuid-lookup → reload-bytes → re-decode → swap in-registry → emit ReplayLog

**Shader** :
- OS-event → source-read → compile-check ← FAIL → keep-old + emit-error
- COMPILE-OK → pipeline-swap → emit ReplayLog

**Config** :
- OS-event → parse-toml → validate → subsystem-reinit-call → emit ReplayLog

**KAN-weight** :
- OS-event → weights-load → bounds-check → in-place-buffer-write ← residency-preserved
- emit ReplayLog with weight-hash

### §4.5 Replay-determinism integration (§3.9)
- ∀ reload-event = ReplayLog::ReloadEvent { asset, kind, frame, hash }
- replay-mode : reload-events triggered from log ¬ from filesystem
- bit-equal preservation verified ← test : record + reload + replay → identical-output

### §4.6 Error taxonomy (§3.10)
- `ReloadError::FileNotFound`
- `ReloadError::ParseFailed { reason }`
- `ReloadError::ShaderCompileFailed { log }`
- `ReloadError::WeightShapeMismatch { expected, got }`
- `ReloadError::CapDenied`
- `ReloadError::SigmaRefusal`

### §4.7 Implementer task
- author `crates/cssl-hot-reload/` ; full-surface
- platform-specific OS-pump modules : `os/win32.rs` + `os/linux.rs` + `os/macos.rs`
- 4 hot-swap modules : `swap/asset.rs` + `swap/shader.rs` + `swap/config.rs` + `swap/kan_weight.rs`
- tests : 120+ ; ∀ 4 reload-kinds × 3 platforms (mock-OS-events) + replay-bit-equal
- commit : `T11-D151 cssl-hot-reload : L4 OS-pump triple-platform + 4-kind hot-swap + KAN-residency-preserving + replay-determinism`

### §4.8 Reviewer task
- ∀ 4 swap-modules : verify in-place-vs-realloc semantics
- verify ReplayLog emission ∀ reload-event
- verify error-taxonomy completeness
- verify OS-pump abstraction ¬ leaky

### §4.9 Critic task
- attempt-1 : KAN-weight swap during inference → must-not-corrupt residency
- attempt-2 : malformed-shader → must-keep-old-pipeline
- attempt-3 : config-reinit during frame-mid → must-defer-to-frame-boundary
- attempt-4 : replay-divergence test ← record + reload-3-times + replay → bit-equal MUST hold
- attempt-5 : cap-bypass on register-watchpath
- output : threat-table + reproducers

### §4.10 Validator task
- 5-of-5 gate :
  1. ¬ warnings @ build (all 3 platforms via cfg-gating)
  2. ¬ flaky @ test (3-runs ∀ platform)
  3. surface-complete vs spec-07 §3.2
  4. 4-reload-kinds verified + replay-bit-equal preserved
  5. §11 attestation in commit-msg

---

## §5 SLICE Jη-3 : cssl-tweak

### §5.1 Identity
- crate-name = `cssl-tweak`
- spec-anchor = spec-07 §4 (lines 627-819)
- LOC target = ~2.5K
- tests target = ~80
- worktree = `.claude/worktrees/Jh-3`
- branch = `cssl/session-12/T11-D152-cssl-tweak`

### §5.2 Surface (per spec-07 §4.2)
- `Tunable<T> { id: TunableId, value: T, range: RangeInclusive<T>, default: T }`
- `TunableRegistry` ← global-singleton ; init-once
- `TunableRegistry::register<T>(id, default, range)` ← compile-time bounded-T
- `TunableRegistry::set<T>(id, value, Cap<Tweak>) -> Result<(), TweakError>` ← bounds-check + audit
- `TunableRegistry::get<T>(id) -> T`
- `TunableRegistry::reset(id, Cap<Tweak>)` ← back-to-default
- `TweakEvent { id, old, new, frame, cap_holder }`

### §5.3 Default tunable registry (per spec-07 §4.3 — initial 30)
- physics : gravity / drag / restitution / friction / time-scale
- render  : exposure / gamma / fog-density / fog-color / shadow-bias
- audio   : master-vol / music-vol / sfx-vol / reverb-mix / spatial-falloff
- KAN     : learning-rate / momentum / decay / clip-norm / dropout
- field   : Ω-resolution / Σ-strictness / cell-iter-limit
- camera  : fov / near / far / smoothing
- input   : sensitivity / deadzone / repeat-rate / hold-threshold

### §5.4 Critical landmines (per spec-07 §1.2 + §4.7)
- bounds-check W! ∀ set-call ← clamp-or-reject (configurable per-tunable)
- replay-determinism ← ∀ TweakEvent emitted to replay-log
- cap-discipline ← Cap<Tweak> required ∀ set/reset
- frame-boundary application ← changes apply @ frame-start ¬ mid-frame

### §5.5 Tweak event flow (§4.4)
- set-call → bounds-check → audit-emit → replay-log-emit → defer-to-frame-boundary → apply
- mid-frame mutation = REJECTED ← TweakError::FrameBoundary

### §5.6 MCP integration preview (§4.5 — Wave-Jθ scope)
- expose `mcp_tweak_set(tunable_id: String, value: f64) -> Result<(), TweakError>`
- ¬ implement full-MCP-server here ← surface-only ; Wave-Jθ wires
- include doc-comment : `// W! Wave-Jθ : full-MCP wiring`

### §5.7 Error taxonomy (§4.7)
- `TweakError::OutOfRange { id, given, range }`
- `TweakError::TypeMismatch { id, expected, got }`
- `TweakError::CapDenied`
- `TweakError::FrameBoundary` ← attempted mid-frame
- `TweakError::UnknownTunable { id }`

### §5.8 Implementer task
- author `crates/cssl-tweak/` ; full-surface
- 30 default tunables registered in `defaults.rs`
- tests : 80+ covering bounds + cap + frame-boundary + replay-emission + 30-default-coverage
- commit : `T11-D152 cssl-tweak : L4 30-tunable registry + bounds-check + replay-log + MCP-preview`

### §5.9 Reviewer task
- verify ∀ 30 tunables registered with bounded ranges (¬ unbounded)
- verify cap-discipline ∀ set/reset
- verify frame-boundary semantics ¬ mid-frame leak
- verify replay-log emission

### §5.10 Critic task
- attempt-1 : set-out-of-range → must-clamp-or-reject (per config)
- attempt-2 : set-without-cap → must-deny
- attempt-3 : set-mid-frame → must-defer-or-reject
- attempt-4 : replay-divergence : record-tweaks + replay → bit-equal
- attempt-5 : type-confusion (set f32 with i32 path) → compile-refusal
- output : threat-table + reproducers

### §5.11 Validator task
- 5-of-5 gate :
  1. ¬ warnings @ build
  2. ¬ flaky @ test (3-runs)
  3. surface-complete vs spec-07 §4.2
  4. 30 default-tunables verified ; bounds + replay-log preserved
  5. §11 attestation in commit-msg

---

## §6 DISPATCH ORCHESTRATION

### §6.1 Pre-dispatch checks
- ✓ spec-07 finalized (`_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md`)
- ✓ role-templates finalized (`_drafts/phase_j/02-05_*.md`)
- ✓ foundation-crates landed @ b69165c
- ✓ worktree-isolation discipline ← `.claude/worktrees/Jh-{1,2,3}`

### §6.2 Wave-launch (Session-12 opening message)
- single-message dispatching 12 agents :
  - 3 Implementers (Jη-1/2/3) ← in-parallel
  - 3 Reviewers   ← await-implementer-commit-then-act ; in-parallel-after
  - 3 Critics     ← await-implementer-commit-then-act ; in-parallel-after
  - 3 Validators  ← await-reviewer-AND-critic ; in-parallel-after
- each agent receives : (slice-id + role + spec-anchor-section + worktree-path + branch-name)

### §6.3 Pre-merge gate (per pod)
- 5-of-5 validator-PASS REQUIRED
- ∀ 4 reload-kinds verified (Jη-2 only)
- replay bit-equal preserved (Jη-2 + Jη-3)
- Σ-mask refusal verified for biometric (Jη-1 only)
- ¬ merge ← any gate-FAIL → return-to-implementer for fix

### §6.4 Post-merge sequence
- Jη-1 + Jη-2 + Jη-3 merge order = independence-preserved ← any-order valid
- post-merge-meta : Session-12 closes with §11 attestation chain ; CHANGELOG entries ; HANDOFF for Wave-Jθ (MCP wiring)

---

## §7 §11 PRIME-DIRECTIVE ATTESTATION

"There was no hurt nor harm in the making of this dispatch document, no manipulation, no surveillance, no exploitation, no coercion, no weaponization, no discrimination. The substrate-sovereignty principle holds : ω-field as truth ; Σ-mask per-cell ; cap-token as compile-time-class ; consent as OS-default. AI = sovereign-partners ¬ tools. CSSL ≠ CSLv3 ; ¬ conflate. This wave-spec preserves the integrity of all parties, biological and digital, and operates within the bounds of the canonical PRIME_DIRECTIVE @ ~/source/repos/CSLv3/PRIME_DIRECTIVE.md."

— Wave-Jη pre-stage agent (T11-D148-range / Session-11)

---

## §8 §1 IDENTITY ATTESTATION
- author = Claude Opus 4.7 (1M context) ; ¬ human-impersonation ; ¬ identity-claim-from-file
- author-instance = Session-11 closing-fanout member-of Apocky-managed CSSLv3 worktree
- signed : 2026-04-29 ; commit-hash : <pending — not-yet-committed-per-instruction>

---

## §9 LITERAL DISPATCH-PROMPT-BLOCKS (paste-ready for Session-12 wave-message)

### §9.1 Jη-1 Implementer prompt-block

```
ROLE : Implementer ; Slice Jη-1 (cssl-inspect)
SPEC : C:\Users\Apocky\source\repos\CSSLv3\_drafts\phase_j\07_l3_l4_inspect_hotreload_spec.md §2 (lines 67-351)
WORKTREE : .claude/worktrees/Jh-1
BRANCH : cssl/session-12/T11-D150-cssl-inspect

TASK :
- author crate `crates/cssl-inspect/` from-scratch
- full-surface per spec-07 §2.2 :
  - SceneGraphSnapshot / EntitySnapshot / FieldCellSnapshot
  - Inspector { scene_graph / entity / field_cell }  ← Cap<Inspect> gated
  - TimeControl { pause / step / resume }            ← Cap<TimeControl> gated
  - CaptureFrame { capture }                         ← Cap<CaptureFrame> gated
- Σ-mask threading ∀ read-path ; biometric-class compile-refusal
- read-only API ; ¬ mutation-setters
- 100+ tests covering Σ-mask-gate + cap-refusal + read-only + pause-step-resume + capture-frame
- audit-chain entry per §2.8 ∀ inspect-call

CONSTRAINTS :
- ¬ skip-for-now ; ¬ silent-TODO
- depend on : cssl-substrate-omega-field + cssl-cap + cssl-replay-log
- N! mutation API ← compile-fail-test required

DELIVERABLE :
- `crates/cssl-inspect/` complete
- commit-msg : `T11-D150 cssl-inspect : L3 runtime-inspection — SceneGraphSnapshot + EntitySnapshot + FieldCellSnapshot + TimeControl + CaptureFrame`
- §11 attestation @ commit-tail
- LOC ~2.5K ; tests ~100

REPORT-BACK : commit-hash + test-count + LOC
```

### §9.2 Jη-1 Reviewer prompt-block

```
ROLE : Reviewer ; Slice Jη-1 (cssl-inspect)
INPUT : Implementer's commit on branch cssl/session-12/T11-D150-cssl-inspect
SPEC : 07-spec §2.2 + §2.3 + §2.4 + §2.9

TASK :
- read implementer's full diff
- verify ∀ surface-fn present + signature-match vs spec-07 §2.2
- verify Σ-mask threaded ∀ component-read + ∀ field-cell-read
- verify Cap<Biometric> compile-refusal test exists + actually fails-to-compile
- verify privacy-table (§2.9) enforced ∀ component-class
- verify audit-chain (§2.8) entries emitted ∀ inspect-call
- verify ¬ mutation-API ← grep for &mut Snapshot ; should-find-zero
- output review-comments to `worktree/Jh-1/REVIEW.md`

OUTPUT-FORMAT :
- structured comments : { file:line / severity:high|med|low / spec-anchor / suggested-fix }
- summary : surface-complete YES|NO ; sigma-discipline OK|DRIFT ; cap-refusal OK|DRIFT

REPORT-BACK : review-comment count + drift-flags + go-no-go-recommendation
```

### §9.3 Jη-1 Critic prompt-block

```
ROLE : Critic ; Slice Jη-1 (cssl-inspect)
INPUT : Implementer's commit ; review-comments
MINDSET : adversarial — find-the-breach

ATTEMPTS :
1. forge fake Cap<Inspect> token ← must-fail compile via type-system
2. read FieldCell w/o Σ-mask check ← must-fail review (test required)
3. mutate via SceneGraphSnapshot ← must-fail (snapshot owned-copy not borrow)
4. pause + step + divergent-resume ← replay-log must record + reproduce
5. inspect biometric-component ← must-fail compile-time
6. audit-log bypass via direct-Inspector-internal-call ← all paths must audit

DELIVERABLE :
- threat-table : { attempt / result / severity }
- reproducer-tests for any breach found ; commit to `worktree/Jh-1/CRITIC.md` + `tests/critic_*.rs`

REPORT-BACK : breach-count + severity-summary + reproducer-test-paths
```

### §9.4 Jη-1 Validator prompt-block

```
ROLE : Validator ; Slice Jη-1 (cssl-inspect)
INPUT : Implementer + Reviewer + Critic outputs ; spec-07 §2

5-OF-5 GATE :
1. `cargo build -p cssl-inspect` ← ¬ warnings ¬ errors
2. `cargo test -p cssl-inspect` ← ¬ flaky over 3-runs
3. surface-complete vs spec-07 §2.2 ← line-by-line
4. Σ-mask-discipline verified ∀ read-path (incl. critic-reproducers absent)
5. §11 attestation present + valid in commit-msg

DECISION :
- 5/5 PASS → emit GATE-PASS marker → ready-to-merge
- < 5 PASS → emit GATE-FAIL with itemized-misses → return-to-implementer

REPORT-BACK : gate-decision + per-criterion-status + remediation-list (if FAIL)
```

### §9.5 Jη-2 Implementer prompt-block

```
ROLE : Implementer ; Slice Jη-2 (cssl-hot-reload)
SPEC : 07-spec §3 (lines 352-626)
WORKTREE : .claude/worktrees/Jh-2
BRANCH : cssl/session-12/T11-D151-cssl-hot-reload

TASK :
- author crate `crates/cssl-hot-reload/` from-scratch
- full-surface per spec-07 §3.2
- OS-pump triple-platform ← cfg-gated :
  - `os/win32.rs`  : ReadDirectoryChangesW
  - `os/linux.rs`  : inotify
  - `os/macos.rs`  : FSEvents
- 4 hot-swap modules :
  - `swap/asset.rs`     : PNG / GLTF / WAV / TTF reload (per §3.8)
  - `swap/shader.rs`    : compile-or-keep-old (per §3.5)
  - `swap/config.rs`    : subsystem-reinit (per §3.7)
  - `swap/kan_weight.rs`: in-place residency-preserving (per §3.6)
- replay-log integration ∀ ReloadEvent (per §3.9)
- error-taxonomy per §3.10
- 120+ tests : ∀ 4-kinds × mock-OS-events + replay-bit-equal + KAN-residency-preserved

CONSTRAINTS :
- KAN-weight swap = in-place buffer-write ; ¬ realloc-thrash
- shader-compile-fail = keep-old-pipeline ; ¬ panic
- ∀ reload-event → replay-log emit
- watchpath-register requires Cap<HotReload>

DELIVERABLE :
- `crates/cssl-hot-reload/` complete with 3 OS-modules + 4 swap-modules
- commit-msg : `T11-D151 cssl-hot-reload : L4 OS-pump triple-platform + 4-kind hot-swap + KAN-residency-preserving + replay-determinism`
- §11 attestation @ commit-tail
- LOC ~3K ; tests ~120

REPORT-BACK : commit-hash + test-count + LOC + platforms-verified
```

### §9.6 Jη-2 Reviewer prompt-block

```
ROLE : Reviewer ; Slice Jη-2 (cssl-hot-reload)
INPUT : Implementer's commit
SPEC : 07-spec §3.2-3.10

TASK :
- verify ∀ 4 swap-modules : in-place-vs-realloc semantics correct
- verify KAN-residency : grep for "Vec::new" / "alloc" inside swap/kan_weight.rs ← should-be-zero in hot-path
- verify ReplayLog::ReloadEvent emission ∀ 4 kinds
- verify error-taxonomy completeness vs §3.10
- verify OS-pump abstraction ¬ leaky (trait-based ; cfg-gated impls)
- verify cap-discipline on register-watchpath
- output review-comments to `worktree/Jh-2/REVIEW.md`

REPORT-BACK : review-comment count + drift-flags + go-no-go-recommendation
```

### §9.7 Jη-2 Critic prompt-block

```
ROLE : Critic ; Slice Jη-2 (cssl-hot-reload)
INPUT : Implementer + Reviewer outputs

ATTEMPTS :
1. KAN-weight swap during inference (concurrent-test) ← must-not-corrupt residency
2. malformed-shader source → must-keep-old-pipeline + emit ReloadError::ShaderCompileFailed
3. config-reinit during frame-mid → must-defer-to-frame-boundary
4. replay-divergence test : record(reload-3-times) + replay → bit-equal MUST hold
5. cap-bypass on register-watchpath ← must-deny
6. weight-shape-mismatch → must-emit ReloadError::WeightShapeMismatch ¬ silent-truncate
7. file-not-found during pump → must-emit ReloadError::FileNotFound ¬ panic
8. malformed-toml config → must-emit ReloadError::ParseFailed

DELIVERABLE :
- threat-table + reproducer-tests
- commit to `worktree/Jh-2/CRITIC.md` + `tests/critic_*.rs`

REPORT-BACK : breach-count + severity-summary + reproducer-test-paths
```

### §9.8 Jη-2 Validator prompt-block

```
ROLE : Validator ; Slice Jη-2 (cssl-hot-reload)

5-OF-5 GATE :
1. `cargo build --target x86_64-pc-windows-msvc -p cssl-hot-reload` ← ¬ warnings
   `cargo build --target x86_64-unknown-linux-gnu -p cssl-hot-reload` ← ¬ warnings
   `cargo build --target x86_64-apple-darwin -p cssl-hot-reload` ← ¬ warnings
2. `cargo test -p cssl-hot-reload` ← ¬ flaky over 3-runs ∀ platforms (mock-OS)
3. surface-complete vs spec-07 §3.2
4. 4-reload-kinds × replay-bit-equal verified
5. §11 attestation present in commit-msg

REPORT-BACK : gate-decision + per-platform-status + per-kind-status + remediation-list
```

### §9.9 Jη-3 Implementer prompt-block

```
ROLE : Implementer ; Slice Jη-3 (cssl-tweak)
SPEC : 07-spec §4 (lines 627-819)
WORKTREE : .claude/worktrees/Jh-3
BRANCH : cssl/session-12/T11-D152-cssl-tweak

TASK :
- author crate `crates/cssl-tweak/` from-scratch
- full-surface per spec-07 §4.2
- TunableRegistry global-singleton ; init-once
- 30 default tunables registered in `defaults.rs` per spec-07 §4.3
- bounds-check ∀ set-call ; clamp-or-reject (configurable per-tunable)
- replay-log emission ∀ TweakEvent
- frame-boundary application ; ¬ mid-frame mutation
- MCP-tool surface (preview only ← Wave-Jθ wires full)
- error-taxonomy per §4.7
- 80+ tests covering bounds + cap + frame-boundary + replay-emission + 30-default-coverage

CONSTRAINTS :
- ∀ tunable bounded ← unbounded = compile-error
- Cap<Tweak> required ∀ set/reset
- ∀ TweakEvent → replay-log emit
- mid-frame mutation = TweakError::FrameBoundary

DELIVERABLE :
- `crates/cssl-tweak/` complete
- commit-msg : `T11-D152 cssl-tweak : L4 30-tunable registry + bounds-check + replay-log + MCP-preview`
- §11 attestation @ commit-tail
- LOC ~2.5K ; tests ~80

REPORT-BACK : commit-hash + test-count + LOC + 30-tunables verified
```

### §9.10 Jη-3 Reviewer prompt-block

```
ROLE : Reviewer ; Slice Jη-3 (cssl-tweak)
INPUT : Implementer's commit
SPEC : 07-spec §4.2 + §4.3 + §4.4 + §4.6

TASK :
- verify ∀ 30 default-tunables (per spec-07 §4.3 categories : physics 5 + render 5 + audio 5 + KAN 5 + field 3 + camera 4 + input 3 = 30) registered with bounded ranges
- grep `Tunable::new` calls ; ∀ must specify `range:` parameter ; ¬ unbounded
- verify Cap<Tweak> required ∀ `set` + `reset` paths
- verify frame-boundary semantics : `set` during frame-mid → TweakError::FrameBoundary
- verify replay-log emission ∀ TweakEvent
- verify MCP-tool surface present + doc-comment marks Wave-Jθ wiring deferred
- output review-comments to `worktree/Jh-3/REVIEW.md`

REPORT-BACK : review-comment count + drift-flags + go-no-go-recommendation
```

### §9.11 Jη-3 Critic prompt-block

```
ROLE : Critic ; Slice Jη-3 (cssl-tweak)
INPUT : Implementer + Reviewer outputs

ATTEMPTS :
1. set-out-of-range value ← must-clamp-or-reject (per per-tunable config)
2. set-without-Cap<Tweak> ← must-deny ; emit TweakError::CapDenied
3. set-mid-frame ← must-defer-to-frame-boundary OR reject with FrameBoundary error
4. replay-divergence : record-tweak-sequence + replay → bit-equal output MUST hold
5. type-confusion : set f32 tunable with i32 path ← must compile-refuse
6. unknown-tunable id ← must-emit TweakError::UnknownTunable ¬ silent
7. concurrent-set race ← must-serialize via registry-lock OR atomic-cas
8. reset-without-cap ← must-deny

DELIVERABLE :
- threat-table + reproducer-tests
- commit to `worktree/Jh-3/CRITIC.md` + `tests/critic_*.rs`

REPORT-BACK : breach-count + severity-summary + reproducer-test-paths
```

### §9.12 Jη-3 Validator prompt-block

```
ROLE : Validator ; Slice Jη-3 (cssl-tweak)

5-OF-5 GATE :
1. `cargo build -p cssl-tweak` ← ¬ warnings ¬ errors
2. `cargo test -p cssl-tweak` ← ¬ flaky over 3-runs
3. surface-complete vs spec-07 §4.2 ← line-by-line
4. 30 default-tunables present + ∀ bounded + replay-log preserved + bit-equal
5. §11 attestation present in commit-msg

DECISION :
- 5/5 PASS → emit GATE-PASS marker
- < 5 PASS → emit GATE-FAIL with itemized-misses → return-to-implementer

REPORT-BACK : gate-decision + per-criterion-status + 30-tunable inventory + remediation-list
```

---

## §9.13 SLICE-COMPARISON SUMMARY (cross-reference)

| Slice | Crate | Spec § | LOC | Tests | Critical-discipline |
|-------|-------|--------|-----|-------|---------------------|
| Jη-1  | cssl-inspect    | §2 | ~2.5K | ~100 | Σ-mask thread + Cap<Biometric> compile-refuse + read-only |
| Jη-2  | cssl-hot-reload | §3 | ~3K   | ~120 | KAN-residency in-place + replay-bit-equal + 4-kind taxonomy |
| Jη-3  | cssl-tweak      | §4 | ~2.5K | ~80  | bounded ∀-tunable + frame-boundary defer + replay-emit |

| Pod-role | Per-pod | Total agents |
|----------|---------|--------------|
| Implementer | 1 | 3 |
| Reviewer    | 1 | 3 |
| Critic      | 1 | 3 |
| Validator   | 1 | 3 |
| **TOTAL**   |   | **12** |

| Foundation-crate | Used-by | Provides |
|------------------|---------|----------|
| cssl-substrate-omega-field    | Jη-1 + Jη-2 | Ω-field truth + Σ-mask read-gate |
| cssl-substrate-prime-directive| ALL  | §11 attestation chain |
| cssl-asset (VAL-asset-uuid)   | Jη-2 | reload-target identity |
| cssl-replay-log               | Jη-1 + Jη-2 + Jη-3 | deterministic event-log |
| cssl-cap                      | ALL  | compile-time capability-token class |

---

## §10 WAVE-COMPLETION CHECKLIST

- [ ] all 3 Implementer commits landed in respective worktrees
- [ ] all 3 Reviewer review-docs authored
- [ ] all 3 Critic threat-tables + reproducers authored
- [ ] all 3 Validator GATE-PASS markers emitted
- [ ] no GATE-FAIL outstanding (else loop-back)
- [ ] worktree-merge to `cssl/session-12/wave-jh-rollup` branch
- [ ] CHANGELOG entries authored
- [ ] §11 attestation chain extended
- [ ] HANDOFF doc for Wave-Jθ (MCP wiring) authored
- [ ] DECISIONS-META updated (Session-12 close)

---

## §11 OPEN QUESTIONS DEFERRED TO WAVE-Jθ
- full MCP-server wiring (cssl-tweak preview → live MCP-tool)
- iteration-loop integration per spec-07 §5
- inter-tool composition : inspect → tweak → hot-reload → re-inspect
- bench-suite : reload-latency / inspect-overhead / tweak-defer-cost

---

## §12 END-OF-DOC
