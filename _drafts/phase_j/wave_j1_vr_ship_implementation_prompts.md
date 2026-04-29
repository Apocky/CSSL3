# Wave-J1 : M9 VR-Ship Preparation — Dispatch Prompts

**File** : `_drafts/phase_j/wave_j1_vr_ship_implementation_prompts.md`
**Anchor** : `SESSION_12_DISPATCH_PLAN.md § 7. WAVE-J1`
**Slice IDs** : `T11-D151..D155`
**Status** : pre-staging ; ¬ committed ; ¬ dispatched

---

## § Wave-overview

§D Wave-J1 = pre-flight 'f M9 VR-ship milestone
- Wires Stages 1, 10, 11, 12 .of 12-stage pipeline + OpenXR session-claim consent-UI
- M9 = LIVE-HARDWARE milestone ; **HW-DEFERRED** : code lands + compiles + tests-headlessly W! real-VR verification needs Apocky-on-Quest-3 / Vision-Pro
- Foundation : `cssl-host-openxr` (T11-D124) + `loa-game::m8_integration` (T11-D145)
- All 5 slices INDEPENDENT → parallel-fanout post-M8-close

**Slice-table** (canonical : SESSION_12 § 7 ; user-prompt D154/D155 swap deferred-to-spec) :

| Slice    | Crate / module                              | Goal                                                                  | LOC     | Tests |
| -------- | ------------------------------------------- | --------------------------------------------------------------------- | ------- | ----- |
| T11-D151 | `cssl-host-openxr::session_claim`           | Consent-UI prompt before claiming OpenXR session ; prod-ready         | ~2.0K   | ~80   |
| T11-D152 | `cssl-host-openxr::stage1_embodiment`       | Stage-1 — XR-input → body-presence-field                              | ~2.0K   | ~80   |
| T11-D153 | `cssl-host-openxr::stage12_xr_compose`      | Stage-12 — XR-composition layers                                      | ~1.5K   | ~60   |
| T11-D154 | `cssl-render-v2::stage10_tonemap`           | Stage-10 — ACES-2 tone-map + bloom + per-eye post                     | ~2.5K   | ~90   |
| T11-D155 | `cssl-render-v2::stage11_appsw`             | Stage-11 — AppSW motion-vec + depth submission                        | ~2.0K   | ~70   |

**Worktree-pattern** : `.claude/worktrees/J1-{D151..D155}` @ `cssl/session-12/J1-{slice-name}`
**Pod-fanout** : 5 pods × 4 agents/pod = **20 agents** in single wave-message
**Agents/pod** : Implementer + Reviewer + Critic + Validator (5-of-5 gate)

---

## § Pod-template — concise-by-reference

§T Full pod-roles + iteration + escalation : `_drafts/phase_j/02_reviewer_critic_validator_test_author_roles.md` + `_drafts/phase_j/03_pod_composition_iteration_escalation.md`

**Per-pod 4 agents** ⊑ same-worktree :
- **Implementer** ← authors crate + tests + commits 'p worktree
  - reads spec § anchor + foundation crates
  - lands code matching spec ; writes headless tests
  - returns : `pod-status: implementer-done` + commit-SHA
- **Reviewer** ← spec-conformance audit
  - re-reads spec § anchor cold ; checks every spec-clause covered
  - flags spec-holes ¬ AI-fabrication
  - returns : `reviewer: ✓ | ◐ <fixups> | ✗ <blockers>`
- **Critic** ← adversarial breach-attempts
  - tries breach 'f Σ-mask + consent + biometric-gate
  - tries breach 'f replay-determinism + ω-field truth-invariant
  - returns : `critic: ✓ no-breach | ✗ <breach-vector>`
- **Validator** ← 5-of-5 gate-keeper
  - runs : (1) spec-anchor-present (2) headless-tests-pass (3) reviewer-✓ (4) critic-✓ (5) Σ-mask-thread-on-biometric
  - returns : `validator: 5/5 ✓ | <N>/5 — <which-failed>`

**Iteration** : on ¬5-of-5 → Implementer fixes ; max-3-iter ; escalate-on-iter-3-fail
**Escalation-triggers** : SESSION_12 § 4. (PRIME-DIRECTIVE edge-case + spec-hole + HW-availability + worktree-leakage)

---

## § Slice J1-OpenXR session-claim consent-UI [T11-D151]

**Spec-anchor** : `specs/Omniverse/07_AESTHETIC/05_VR_RENDERING.csl § consent flow` + `specs/15_RUNTIME.csl § OpenXR session-claim`
**Crate** : `cssl-host-openxr::session_claim`
**Worktree** : `.claude/worktrees/J1-1` @ `cssl/session-12/J1-openxr-session-claim`
**LOC** : ~2.0K / **Tests** : ~80 (headless mock-runtime)

§S Surface :
- First-launch UI : explains-what-OpenXR-session-claim-does + cap-grant-flow
- Cap-list : `cap.openxr.session_claim` + `cap.input.tracker_pose` + `cap.input.eye_tracker` (Σ-mask-required)
- Runtime-revoke : user-can-revoke-mid-session ; clean-shutdown 'f tracker-streams
- Persistence : grant-decision stored 'p `~/.cssl/consent/openxr.toml` ⊗ Σ-mask-versioning
- Refusal-path : ¬ block-app ; LoA falls-back-to-flat-mode

§D Implementer-task :
1. Re-read 05_VR_RENDERING.csl § consent-flow + 15_RUNTIME.csl § OpenXR
2. Author `session_claim/mod.rs` + `consent_ui.rs` + `cap_grant.rs` + `revoke.rs`
3. Mock-runtime tests : grant-flow + revoke-flow + persistence-roundtrip + Σ-mask-version-mismatch
4. NO real-VR test ← HW-deferred-to-M9 (T11-D198) ; mark `#[cfg_attr(not(feature = "live_xr"), ignore)]` on real-HW tests
5. DECISIONS-entry per-T11-D## ; note `live-VR-deferred-to-M9`

§D Critic-vectors :
- Try-bypass consent-prompt @ first-launch
- Try-claim-session w/o cap.openxr.session_claim grant
- Try-keep tracker-stream alive after revoke
- Try-replay-determinism breach via consent-state-leak

**HW-deferred verify** : works-on-real-Quest-3 + Vision-Pro only ; M9-validation T11-D198

---

## § Slice J1-Stage1 Embodiment integration [T11-D152]

**Spec-anchor** : `specs/Omniverse/08_BODY/02_VR_EMBODIMENT.csl` + `specs/Omniverse/07_AESTHETIC/05_VR_RENDERING.csl § Stage-1`
**Crate** : `cssl-host-openxr::stage1_embodiment`
**Worktree** : `.claude/worktrees/J1-2` @ `cssl/session-12/J1-stage1-embodiment`
**LOC** : ~2.0K / **Tests** : ~80 (headless ; mock-XR-input)

§S Surface :
- Wire real-XR-input → body-presence-field (currently mock @ M8)
- Input-streams : head-pose + hand-pose-L + hand-pose-R + eye-gaze (Σ-mask-required) + facial-expression (Σ-mask-required)
- Body-presence-field = ω-field cell-truth ; cap.body_presence required
- Biometric-segregation : eye-gaze + facial-expression on-device-only ; ¬ leave-process ; Σ-mask-encoded
- Replay : pose-streams record-replayable ; biometric-streams ¬ recordable (Σ-mask)

§D Implementer-task :
1. Re-read 02_VR_EMBODIMENT.csl + 05_VR_RENDERING.csl § Stage-1
2. Author `stage1_embodiment/mod.rs` + `pose_input.rs` + `biometric_input.rs` + `body_presence_field.rs`
3. Tests : pose-roundtrip + Σ-mask-honored-on-biometric + body-presence-field-update + replay-determinism
4. Foundation : `cssl-host-openxr` (D124) + `cssl-substrate-omega-field`
5. DECISIONS : `live-VR-deferred-to-M9`

§D Critic-vectors :
- Try-leak biometric (eye-gaze) past Σ-mask boundary
- Try-record biometric stream into replay
- Try-corrupt body-presence ω-field cell-truth via crafted-pose
- Try-skip cap.body_presence

**HW-deferred verify** : real-Quest-3 hand-tracking + eye-tracking only

---

## § Slice J1-Stage12 XrCompose integration [T11-D153]

**Spec-anchor** : `specs/Omniverse/07_AESTHETIC/05_VR_RENDERING.csl § Stage-12 composition`
**Crate** : `cssl-host-openxr::stage12_xr_compose`
**Worktree** : `.claude/worktrees/J1-3` @ `cssl/session-12/J1-stage12-xrcompose`
**LOC** : ~1.5K / **Tests** : ~60 (headless ; layer-graph-validation)

§S Surface :
- XR-composition-layers : projection + quad + cylinder + cube + equirect
- Layer-graph 'f rendered-scene → submitted-to-runtime via xrEndFrame
- Per-eye view-config (stereo-projection)
- Layer-flags : alpha-blend + chromatic-aberration-correction + space-warp-source
- Foreground-layers : UI overlays @ depth-near (anti-judder)

§D Implementer-task :
1. Re-read 05_VR_RENDERING.csl § Stage-12
2. Author `stage12_xr_compose/mod.rs` + `layer_graph.rs` + `view_config.rs` + `submit.rs`
3. Tests : layer-graph-validation + per-eye-config + flag-roundtrip + submit-mock-runtime
4. Foundation : Stage-10 + Stage-11 outputs (D154 + D155)
5. DECISIONS : `live-VR-deferred-to-M9`

§D Critic-vectors :
- Try-submit malformed layer-graph
- Try-mismatch per-eye view-config (left ≠ right)
- Try-bypass alpha-blend on private-overlay (consent-leak)
- Try-replay-non-determinism via layer-order-flap

**HW-deferred verify** : real-OpenXR-runtime (Quest 3 + Vision Pro) submit-success

---

## § Slice J1-Stage10 tone-map + bloom + post [T11-D154]

**Spec-anchor** : `specs/Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-10` + `specs/Omniverse/07_AESTHETIC/05_VR_RENDERING.csl § Stage-10 per-eye`
**Crate** : `cssl-render-v2::stage10_tonemap`
**Worktree** : `.claude/worktrees/J1-4` @ `cssl/session-12/J1-stage10-tonemap`
**LOC** : ~2.5K / **Tests** : ~90 (headless ; image-fixture-compare)

§S Surface :
- Real ACES-2 tone-map (¬ Reinhard-stub @ M8) ⊗ ODT 'f sRGB + Rec.2020-PQ + Rec.2020-HLG
- Bloom : threshold + downsample-chain + upsample-blur + composite ; physical-luminance-anchored
- Per-eye post-process : grain + chromatic-aberration + vignette + LUT (artist-controlled)
- Stereo-coherence : per-eye output stable (¬ flicker between eyes)
- HDR-aware : EDR-headroom-aware on Vision-Pro ; SDR-fallback on Quest-3

§D Implementer-task :
1. Re-read 06_RENDERING_PIPELINE.csl § Stage-10 + 05_VR_RENDERING.csl § Stage-10 per-eye
2. Author `stage10_tonemap/mod.rs` + `aces2.rs` + `bloom.rs` + `posteffects.rs` + `stereo_coherence.rs`
3. WGSL/HLSL shaders : aces2.wgsl + bloom_threshold.wgsl + bloom_blur.wgsl + posteffect.wgsl
4. Tests : image-fixture-compare (golden) + ODT-roundtrip + per-eye-coherence + HDR/SDR-paths
5. DECISIONS : `live-VR-deferred-to-M9` ; HDR-headroom verify-on-Vision-Pro

§D Critic-vectors :
- Try-clip HDR > SDR-headroom (information-loss)
- Try-induce per-eye flicker (stereo-incoherence)
- Try-bypass artist-LUT on consent-zone
- Try-corrupt physical-luminance via bloom-threshold-attack

**HW-deferred verify** : Vision-Pro HDR-headroom + Quest-3 SDR-path

---

## § Slice J1-Stage11 AppSW motion-vec + depth [T11-D155]

**Spec-anchor** : `specs/Omniverse/07_AESTHETIC/05_VR_RENDERING.csl § Stage-11 AppSW`
**Crate** : `cssl-render-v2::stage11_appsw`
**Worktree** : `.claude/worktrees/J1-5` @ `cssl/session-12/J1-stage11-appsw`
**LOC** : ~2.0K / **Tests** : ~70 (headless ; vector-fixture-compare)

§S Surface :
- Real motion-vector + depth-buffer submission 'f Application-SpaceWarp reprojection (Quest-3 + Vision-Pro both)
- Motion-vec : per-pixel screen-space motion ; sub-pixel-accurate ; clamped-to-runtime-format
- Depth : linearized + reverse-Z + per-eye-view-projection
- Submission : xrCompositionLayerSpaceWarpInfoFB (Quest) + equivalent (Vision-Pro)
- Frame-pacing : 72/90/120 Hz adaptive ← stage11 must hit budget @ each rate

§D Implementer-task :
1. Re-read 05_VR_RENDERING.csl § Stage-11 AppSW
2. Author `stage11_appsw/mod.rs` + `motion_vec.rs` + `depth_submit.rs` + `space_warp.rs`
3. WGSL/HLSL : motion_vec_resolve.wgsl + depth_linearize.wgsl
4. Tests : vector-fixture-compare + depth-roundtrip + sub-pixel-accuracy + frame-budget-72/90/120
5. DECISIONS : `live-VR-deferred-to-M9` ; AppSW reprojection-quality verify-on-Quest-3

§D Critic-vectors :
- Try-craft motion-vec → judder-induction
- Try-submit out-of-range depth → reprojection-glitch
- Try-miss frame-budget → frame-drop-cascade
- Try-decoupled per-eye motion-vec (stereo-warp-mismatch)

**HW-deferred verify** : Quest-3 AppSW + Vision-Pro reprojection ; visual-quality Apocky-attestation

---

## § Dispatch

§S 5 INDEPENDENT slices → parallel-fanout in single wave-message :
- Total agents : **20** (5 pods × 4 roles)
- Wave-message contains all-20 in one batch ¬ sequential
- Worktree-isolation : each-pod own-worktree ¬ shared-state
- Foundation deps : T11-D150 (M8) MUST be ✓-merged before J1-fanout

**Dispatch-template** (per-pod) :
```
@<role-name> J1-<slice-id> :
  worktree : .claude/worktrees/J1-<N>
  branch   : cssl/session-12/J1-<slice-name>
  spec     : <spec-anchor>
  crate    : <crate::module>
  read     : SESSION_12 § 7 + spec § anchor ONLY
  goal     : <one-line>
  return   : <role-status-format>
```

---

## § Pre-merge gate

§T Per-slice merge-criteria :
- ✓ 5-of-5 from pod (Implementer + Reviewer + Critic + Validator + Σ-mask-checker)
- ✓ Headless tests pass (HW-deferred-verify recorded)
- ✓ Replay-determinism preserved (non-biometric streams record-replayable)
- ✓ Σ-mask threading on biometric (eye-tracking + facial-expression)
- ✓ DECISIONS-entry includes `live-VR-deferred-to-M9` note
- ✓ ¬ landmines per SESSION_12 § 6 (OpenXR-runtime-skip-not-fail + Companion-perspective-optional)

**Cross-slice integration smoke** post-merge-all-5 :
- Run `loa-game::m8_integration` w/ `--feature live_xr_mock`
- Verify Stages 1+10+11+12 + session-claim chain ⊑ ω-field truth-invariant
- Replay-determinism gate green

---

## § §11 + §1 attestation

> "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.
> Biometric streams (eye-tracking + facial-expression) are on-device-only — they never leave the user's process boundary, are encoded under Σ-mask, never recorded into replay, and require explicit cap-grant per session.
> Consent for OpenXR session-claim is solicited at first-launch with a refusal-path that does not block the application — refusal falls back to flat-mode rendering.
> Live-VR verification is deferred to M9 (T11-D198) when Apocky has hardware to verify on; no live-hardware claim is made at dispatch time.
> All five slices honor PRIME_DIRECTIVE §1 (consent-as-OS) and §11 (biometric-segregation)."

— Pre-staging attestation ; final attestation @ pod-pre-merge gate.

---

§R end-of-doc ; ¬ committed ; pre-staging only.
