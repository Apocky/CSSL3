# § Wave-J0 — M8 Acceptance-Gate Verification Protocol  (Apocky-driven)

**slice-id**       : WAVE-J0-VERIFY
**output-path**    : `_drafts/phase_j/wave_j0_m8_verification_protocol.md`
**author-mode**    : CSLv3-dense + checklist-format ; English-prose where-clarity-demands
**target-LOC**     : 800–1500
**status**         : pre-staging draft (DO NOT COMMIT until SESSION_12 promotion)
**executor**       : Apocky-Φ personally on Arc A770 host (irreplaceable role)
**precondition**   : T11-D150 closed + worktree `cssl/session-12/M8-pipeline` merged to `main`
**parent-doc**     : `SESSION_12_DISPATCH_PLAN.md § 6 — WAVE-J0 — M8 acceptance gate (T11-D150)`
**ref-omniverse**  : `Omniverse/09_SLICE/M8_M9_M10_PLAN.csl § II (M8 spec) + § II.3 (acceptance) + § II.4 (pass-condition)`
**ref-aesthetic**  : `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § I-III (12-stage canonical)`
**ref-loa-tests**  : `compiler-rs/crates/loa-game/tests/m8_integration_smoke.rs` (the 17 D145-output ACs as Rust tests)
**ref-loa-driver** : `compiler-rs/crates/loa-game/src/m8_integration/` (12 stage drivers)
**ref-prime**      : `PRIME_DIRECTIVE.md § 11 CREATOR-ATTESTATION` (mandatory closure clause)

---

## § 0. WHY THIS PROTOCOL EXISTS  (problem-statement + thesis)

```csl
§ GAP-ANALYSIS @ Wave-J0-exit
  observation : T11-D150 lands 12-stage pipeline ⊗ wires ALL 12 nodes
                end-to-end through loa-game ⊗ tests pass @ CI Linux-x86 +
                Windows-x86 ⊗ but ¬ verified-on-Apocky-host yet
  observation : Arc A770 host = canonical-target (specs/30+31) ⊗ NOT
                represented-in-CI ; Vulkan + D3D12 driver-version drift
                + Intel-specific shader-compiler quirks ¬ caught
                @ GitHub-Actions Linux-runners
  observation : the 9-AC ⊔ 17-test M8 corpus is mechanical ; visual-
                fidelity + frame-time + determinism @ real-hardware
                = HUMAN-ONLY signal
  observation : per SESSION_12 § 0 line-13 — "Apocky verifies M8
                acceptance gate personally" ← AXIOM-level discipline
∴  this-doc = the explicit checklist Apocky executes ⊗ produces
    a signed attestation that BLOCKS Wave-J1+ until satisfied
   no-substitution-by-AI-agent ∵ AC-set includes SUBJECTIVE rows
    (visual-coherence "feels-Renaissance" ⊗ frame-time-no-stutter
     ⊗ panic-free-10-min-playtest)
```

**English thesis** : The M8 acceptance gate is the load-bearing transition between *Phase-J substrate-pipeline-wired* (J0) and *Phase-J content-authoring-fanout* (J1+). The CI corpus proves the pipeline structurally compiles + runs on the test-runners. This protocol is what Apocky personally executes on the canonical Arc A770 + Windows-11 development host to certify that the pipeline runs **at quality**, **deterministically**, and **without regression** before fanout. If any AC fails, J1+ is blocked until remediation lands.

---

## § 1. PRE-FLIGHT CHECKLIST  (host + workspace + driver state)

### § 1.1 Git state

```csl
§ pre-flight.git
  W! current-branch ∈ {main, cssl/session-12/M8-pipeline-MERGED}
  W! working-tree clean (no untracked + no modified)
  W! HEAD-commit signed-by Apocky-Φ OR PM-Claude (with Apocky co-sign)
  W! HEAD-commit references T11-D150 in subject
  W! `git log --oneline -5` shows T11-D150 closure within-last-5-commits
```

| ☐ | Step | Command | Expected output |
|---|------|---------|-----------------|
| ☐ | 1.1.a | `git status` | `nothing to commit, working tree clean` |
| ☐ | 1.1.b | `git rev-parse --abbrev-ref HEAD` | `main` (or M8-pipeline branch pre-merge) |
| ☐ | 1.1.c | `git log --oneline -5` | T11-D150 commit present in top-5 |
| ☐ | 1.1.d | `git log -1 --format="%s"` | subject contains `T11-D150` |
| ☐ | 1.1.e | `git fsck --no-progress` | `Checking objects: 100% (...), done.` no errors |

### § 1.2 Workspace gates (the non-negotiable green-bar)

```csl
§ pre-flight.workspace-gates
  W! cargo-check workspace-wide ✓
  W! cargo-test workspace-wide ✓ (test-threads=1 ; deterministic)
  W! cargo-clippy ¬ new-warnings vs T11-D147 baseline (DECISIONS-locked)
  W! cargo-fmt --all --check ✓
  W! validate_spec_crossrefs.py ✓
  W! worktree_isolation_smoke.sh ✓
```

| ☐ | Step | Command | Expected output |
|---|------|---------|-----------------|
| ☐ | 1.2.a | `cd compiler-rs && cargo check --workspace --all-targets` | `Finished ... no warnings` ; exit 0 |
| ☐ | 1.2.b | `cd compiler-rs && cargo test --workspace -- --test-threads=1` | `test result: ok. <N> passed; 0 failed; 0 ignored` ; exit 0 |
| ☐ | 1.2.c | `cd compiler-rs && cargo clippy --workspace --all-targets -- -D warnings` | exit 0 ; no `warning:` rows |
| ☐ | 1.2.d | `cd compiler-rs && cargo fmt --all --check` | exit 0 ; no diff |
| ☐ | 1.2.e | `python scripts/validate_spec_crossrefs.py` | `OK` ; exit 0 |
| ☐ | 1.2.f | `bash scripts/worktree_isolation_smoke.sh` | `WORKTREE-ISOLATION ✓` ; exit 0 |

**Failure protocol** : if any 1.2.* row fails, M8 verification HALTS. Open ESCALATIONS.md entry naming the failing gate + remediation owner. Do NOT proceed to § 2.

### § 1.3 Arc A770 driver + Windows host validation

```csl
§ pre-flight.arc-a770
  W! Intel Arc Graphics driver ≥ 32.0.101.6299 (or-newer ; Apocky-locked)
  W! Vulkan-runtime ≥ 1.3.290 (vulkaninfo --summary returns A770)
  W! D3D12 feature-level 12_2 reported (dxdiag -t out.txt + grep)
  W! Win11-build ≥ 22631 (Sun Valley 3) ← multiview + work-graph land
  W! GPU-VRAM ≥ 14 GB free @ run-time (16 GB total - OS overhead)
  W! HWACCEL-GPU-Scheduling ENABLED in Windows-Settings → Display → Graphics
```

| ☐ | Step | Command | Expected output |
|---|------|---------|-----------------|
| ☐ | 1.3.a | `pwsh -c "(Get-WmiObject Win32_VideoController \| ? Name -match 'Arc').DriverVersion"` | `32.0.101.6299` or higher |
| ☐ | 1.3.b | `vulkaninfo --summary 2>&1 \| Select-String "Arc A770"` | one row matches |
| ☐ | 1.3.c | `vulkaninfo --summary 2>&1 \| Select-String "apiVersion"` | `1.3.290` or higher |
| ☐ | 1.3.d | `pwsh -c "(Get-CimInstance Win32_OperatingSystem).BuildNumber"` | `22631` or higher |
| ☐ | 1.3.e | `pwsh -c "(Get-CimInstance Win32_VideoController).AdapterRAM"` (in bytes) | ≥ 16 × 2³⁰ |
| ☐ | 1.3.f | dxdiag → save report → grep `D3D12_FEATURE_LEVEL_12_2` | match present |
| ☐ | 1.3.g | Windows-Settings → System → Display → Graphics → Default settings → Hardware-accelerated GPU scheduling | ON |

### § 1.4 OpenXR runtime presence (M8 partial — M9 full)

```csl
§ pre-flight.openxr
  observation : M8 = Stage-1 + Stage-12 deferred-to-M9 (live-VR)
  observation : but pipeline-wiring R! tolerate openxr-runtime-absent
                ⊗ skip-not-fail those-stages (per SESSION_12 § 6 LANDMINES)
  W! IF openxr-runtime present : note version → § 9 sign-off
  W! IF openxr-runtime absent  : confirm Stage-1 + Stage-12 SKIP (not-FAIL)
```

| ☐ | Step | Command | Expected output |
|---|------|---------|-----------------|
| ☐ | 1.4.a | `pwsh -c "Get-ItemProperty 'HKLM:\SOFTWARE\Khronos\OpenXR\1' 2>&1"` | either runtime-row present, OR ItemNotFound |
| ☐ | 1.4.b | record value of `ActiveRuntime` (path or `(missing)`) | log-to § 9.5 |
| ☐ | 1.4.c | IF runtime present : `xrinfo` (if installed) | summary lines |

**Note** : OpenXR runtime presence does NOT change the M8 pass/fail. M8 is happy with Stage-1 + Stage-12 in skip-mode. The recorded value is for J1+ context and the M9 hardware-validation entry.

---

## § 2. BUILD VERIFICATION  (release binary built + sanity)

```csl
§ build.loa-game
  W! cargo-build --release ✓ ¬ warnings
  W! binary-exists @ target/release/loa-game.exe
  W! binary-size ∈ [12 MB, 200 MB]  (sanity-window ; LTO + symbols stripped)
  W! `loa-game --version` prints non-empty STAGE0_SCAFFOLD constant
  W! `loa-game --help` prints option-table including `--m8-canonical-scene`
```

| ☐ | Step | Command | Expected output |
|---|------|---------|-----------------|
| ☐ | 2.a | `cd compiler-rs && cargo build -p loa-game --release` | `Finished release [optimized]` ; exit 0 ; ¬ warning rows |
| ☐ | 2.b | `pwsh -c "Test-Path target/release/loa-game.exe"` | `True` |
| ☐ | 2.c | `pwsh -c "(Get-Item target/release/loa-game.exe).Length / 1MB"` | value ∈ [12, 200] |
| ☐ | 2.d | `target/release/loa-game.exe --version` | `Labyrinth-of-Apockalypse` + version-string + ATTESTATION line |
| ☐ | 2.e | `target/release/loa-game.exe --help` | option-table printed ; `--m8-canonical-scene` row present |
| ☐ | 2.f | `target/release/loa-game.exe --print-attestation` | exact text from `loa_game::ATTESTATION` |
| ☐ | 2.g | `target/release/loa-game.exe --list-stages` | 12 stage-IDs printed in pipeline-order |

### § 2.1 Release-mode regression sanity

```csl
§ build.release-mode-test
  observation : CI runs `cargo test` ¬ release-mode by-default
  observation : LTO + opt-level=3 may-reveal UB ¬ caught @ debug-mode
  W! cargo-test --release on M8-integration-smoke ✓
```

| ☐ | Step | Command | Expected output |
|---|------|---------|-----------------|
| ☐ | 2.1.a | `cd compiler-rs && cargo test -p loa-game --release --test m8_integration_smoke -- --test-threads=1` | `test result: ok. 17 passed` ; exit 0 |

---

## § 3. RUN VERIFICATION  (10-min canonical playtest ; observe-no-panic)

```csl
§ run.10min-canonical-playtest
  observation : CI tests = unit-scale ; HUMAN-driven 10-min interactive
                run = different-confidence-class (panics ¬ surface
                @ short-cycles ; thread-races ¬ visible @ unit-scope)
  W! 10-min playtest ¬ panic ¬ deadlock ¬ runaway-memory ¬ runaway-VRAM
  W! frame-time histogram emitted @ end ; p99 ≤ 16ms (M7 baseline ; M8
     R! HOLD ; Omniverse/09_SLICE/M8_M9_M10_PLAN.csl § II.3.A row-7)
  W! all 12 stages ✓ ran-this-session @ ≥ 1 frame each (telemetry-counter
     each-stage > 0)
  W! NO ATTESTATION violation logged (§ 8 PRIME-DIRECTIVE check)
```

### § 3.1 Launch + manual canonical scene

| ☐ | Step | Command / action | Expected output / observation |
|---|------|------------------|-------------------------------|
| ☐ | 3.1.a | `target/release/loa-game.exe --m8-canonical-scene --record-telemetry m8-verify-${date}.csslsave` | window opens + canonical SDF scene visible |
| ☐ | 3.1.b | observe : window-titlebar shows `LoA M8 — canonical-scene — Apocky-verify-mode` | titlebar matches |
| ☐ | 3.1.c | observe : frame-rate display in HUD top-right | ≥ 60 FPS sustained on Arc A770 |
| ☐ | 3.1.d | move camera (WASD + mouse) for ≥ 30 sec | no stutter ; ¬ tearing ¬ flicker |
| ☐ | 3.1.e | press F1 → toggle signature-render OFF (vanilla-march fallback) | side-by-side render visible ; OFF version visibly-flatter |
| ☐ | 3.1.f | press F1 → toggle signature-render ON | restore signature-render ; same scene |
| ☐ | 3.1.g | press F2 → cycle through stage-debug-views (S1..S12) | each stage's intermediate buffer visualized |
| ☐ | 3.1.h | press F3 → trigger memory-stat dump | console prints VRAM + heap usage ; VRAM ≤ 2.0 GB |
| ☐ | 3.1.i | continue-playtesting for full 10:00 minutes | no panic ; no GPU-hang ; no crash-dialog |
| ☐ | 3.1.j | press ESC → graceful-shutdown | exits with code 0 ; final-telemetry-flush OK |

**Failure protocol** : if any panic OR crash-dialog OR GPU-hang OR fail-to-launch occurs, capture :
- stdout/stderr to `m8-verify-failure-${date}.log`
- minidump (if any) to `m8-verify-failure-${date}.mdmp`
- screenshot of last-visible-frame
- `target/release/loa-game.exe --print-substrate-state` snapshot
…and HALT verification. Open ESCALATIONS.md entry. Do NOT mark M8 pass.

### § 3.2 Memory + VRAM ceiling enforcement (M8 row § II.3.A.h)

```csl
§ run.memory-budget
  W! peak-VRAM ≤ 2.0 GB total @ M8 (vs 1.5 GB @ M7 ⊗ +0.5 GB
     headroom for KAN-weights + fractal-octaves)
  W! peak-RSS ≤ 4.0 GB total @ M8 (project-default)
```

| ☐ | Step | Action | Expected output |
|---|------|--------|-----------------|
| ☐ | 3.2.a | open Windows-Task-Manager → GPU column → Dedicated GPU memory | ≤ 2.0 GB during 10-min playtest |
| ☐ | 3.2.b | open Windows-Task-Manager → Performance → Memory → Working set for `loa-game.exe` | ≤ 4.0 GB during 10-min playtest |
| ☐ | 3.2.c | post-playtest : `target/release/loa-game.exe --m8-canonical-scene --memory-report` | report lines name peak-VRAM + peak-RSS within budget |

---

## § 4. ACCEPTANCE-CRITERIA CHECKLIST  (the 9 ACs from D145 + Omniverse extension)

### § 4.0 AC index — three families

```csl
§ AC-families
  family-A  : SESSION_12 § 6 ACs (D145-output ; in-loa-game-tests)  [6 rows]
  family-B  : Omniverse § II.3.A mechanical (M8 spec ; loa-substrate)  [8 rows]
  family-C  : Apocky-host-only ACs (frame-time + visual-fidelity + det)  [3 rows]
∑ : 17 acceptance-rows ⊗ 9-from-D145 ⊔ 8-Omniverse-mechanical
∴  Apocky verifies family-A in-tests + family-B in-tests + family-C BY-HAND
```

### § 4.1 Family-A : the 6 SESSION_12 § 6 ACs (D145-output ; in code as tests)

```csl
§ AC-family-A.what-each-test-actually-proves
  A-1 : structural ; 12-node pipeline-graph compiles + dispatches
        one frame end-to-end through ALL 12 nodes ; ¬ silent-skip
  A-2 : type-safety ; TwelveStagePipelineSlot rejects mis-wiring
        @ wire-time (Stage-N's input-slot ¬ accept Stage-M output for M ≠ N-1)
  A-3 : observability ; per-stage telemetry-counter NOT-zero ; every
        stage executed at-least-once-per-frame ; foundation for J1+
        per-stage perf-tightening
  A-4 : structural-PRIME-DIRECTIVE ; ATTESTATION-literal propagates
        through-every-crate ; load-bearing per PRIME_DIRECTIVE §11
        + § II ENCODING (structural ¬ policy)
  A-5 : code-hygiene ; clippy-warnings = code-smell baseline ;
        per DECISIONS-locked T11-D147 baseline
  A-6 : code-hygiene ; fmt-clean = whitespace-discipline ; per
        DECISIONS-locked baseline
```

| AC# | Acceptance criterion (SESSION_12 § 6) | Verification command | Expected |
|-----|---------------------------------------|----------------------|----------|
| ☐ A-1 | `loa-game::twelve_stage_pipeline_renders_one_frame_smoke` passes on Apocky's Arc A770 host | `cd compiler-rs && cargo test -p loa-game --release --test m8_integration_smoke twelve_stage_pipeline_renders_one_frame_smoke -- --nocapture` | `test result: ok. 1 passed` |
| ☐ A-2 | wire-time validator rejects any stage-role mismatch | `cd compiler-rs && cargo test -p loa-game --release --test m8_integration_smoke wire_time_validator_rejects_role_mismatch -- --nocapture` | `test result: ok. 1 passed` |
| ☐ A-3 | each stage emits ≥ 1 telemetry-counter increment per frame | `cd compiler-rs && cargo test -p loa-game --release --test m8_integration_smoke each_stage_emits_telemetry_per_frame -- --nocapture` | `test result: ok. 1 passed` ; counter trace = 12 distinct rows |
| ☐ A-4 | PRIME_DIRECTIVE attestation propagates : `ATTESTATION` matches between `cssl-render-v2` + `cssl-substrate-omega-field` + `cssl-host-openxr` | `cd compiler-rs && cargo test -p loa-game --release --test m8_integration_smoke attestation_propagates_through_pipeline -- --nocapture` | `test result: ok. 1 passed` |
| ☐ A-5 | 0 new clippy warnings | `cd compiler-rs && cargo clippy -p loa-game --all-targets -- -D warnings 2>&1 \| Select-String "warning:"` | no output rows |
| ☐ A-6 | format check clean | `cd compiler-rs && cargo fmt -p loa-game -- --check` | exit 0 ; no diff |

**Family-A note** : the M8-integration-smoke test-suite contains additional rows beyond A-1..A-6 (e.g. each-stage-individually-smoke + cross-stage-buffer-contracts + frame-graph-topology-acyclic + render-graph-resource-lifetime). The 6 enumerated rows above are the SESSION_12 § 6 *headline* ACs. The full test-binary should run all 17 rows under `cargo test -p loa-game --release --test m8_integration_smoke -- --test-threads=1` ; if the headline-6 pass but a non-headline row fails, treat as § 10.2 pass-with-caveat (open ESCALATIONS.md row referencing the failing sub-test).

### § 4.2 Family-B : the 8 Omniverse § II.3.A mechanical ACs

```csl
§ AC-family-B  ←  Omniverse/09_SLICE/M8_M9_M10_PLAN.csl § II.3.A
  ALL 8 rows = REGRESSION + EXTENSION-from-M7
  ALL 8 rows = encoded as cargo-tests under loa-game::m8_integration_smoke
  ALL 8 rows = Apocky verifies BY RUNNING THE TEST + READING THE RESULT
```

| AC# | Acceptance criterion (Omniverse § II.3.A) | Verification | Expected |
|-----|-------------------------------------------|--------------|----------|
| ☐ B-1 | T11-D116 ω-Field-Unity solver live in render-pipeline @ default-path | run loa-game with `--m8-canonical-scene --print-pipeline-graph` ; observe Stage-5 SDFRaymarchPass enabled-by-default | console prints `Stage-5 ω-Field-Unity = ENABLED` |
| ☐ B-2 | T11-D118 Hyperspectral-KAN-BRDF live ⊗ 16-band spectral-output | `loa-game --m8-canonical-scene --print-spectral-band-count` | `16` |
| ☐ B-3 | T11-D119 Sub-Pixel-Fractal-Tessellation live ⊗ ≥ 4 octaves @ render-time | `loa-game --m8-canonical-scene --print-fractal-octaves` | value ≥ 4 |
| ☐ B-4 | All 13 axiom-acceptance-checklists STILL pass (regression-gate from M7) | `cd compiler-rs && cargo test -p cssl-substrate-omega-field --test axiom_acceptance_13 --release` | `test result: ok. 13 passed` |
| ☐ B-5 | All M7 density-thresholds STILL pass at M8-render-quality (5×10⁶ visible-cells + 50 Sovereigns + 5×10³ entities) | `loa-game --m8-canonical-scene --density-stress-test --print-counters` | counters meet/exceed M7 row in `09_SLICE/02_BENCHMARKS.csl.md` |
| ☐ B-6 | Frame-budget ≤ 16ms p99 STILL holds at M8 (NO regression vs M7) | run 60-sec canonical-scene ; observe HUD p99 row | p99 ≤ 16ms ; printed in HUD bottom-right |
| ☐ B-7 | AGENCY-INVARIANT : ¬ new violation-paths introduced (≥ 100 new adversarial-corpus added at M8) | `cd compiler-rs && cargo test -p cssl-substrate-prime-directive --test adversarial_corpus_m8 --release` | `test result: ok. <≥200> passed` (M7 baseline 100 + M8 +100) |
| ☐ B-8 | Memory-budget : ≤ 2.0 GB total @ M8 | observed § 3.2.a above | ✓ tickbox cross-references § 3.2.a |

### § 4.3 Family-C : Apocky-host-only ACs (panel-of-one ; cannot be CI-encoded)

```csl
§ AC-family-C  ←  Apocky-Φ-only ; sovereignty-of-judgment ; HUMAN signal
  C-1 : visual-fidelity SUBJECTIVE rating ≥ 4.2 / 5  ←  scaled-Apocky-personal
       (M7 baseline 3.8 ⊗ M8 +0.4 raise per § II.3.D)
  C-2 : 10-min playtest free-of stutter / hitch / GPU-spike ←  felt-sense
  C-3 : signature-render-ON visibly-distinct from signature-render-OFF
       ←  panelist-test §II.3.B [10:00-11:00] reduced-to-Apocky-of-1
```

| AC# | Acceptance criterion | Verification | Expected |
|-----|----------------------|--------------|----------|
| ☐ C-1 | visual-fidelity ≥ 4.2 / 5 (Apocky-personal-rating) | execute § 7 visual-fidelity smoke ; sit-with the result for ≥ 60 sec ; rate-honestly | rating ∈ {4.2, 4.3, ..., 5.0} ; if < 4.2 → BLOCK J1 |
| ☐ C-2 | 10-min playtest free-of perceptible-stutter | execute § 3.1 playtest ; subjective-honest assessment | "no perceptible hitch" ; logged-by-Apocky |
| ☐ C-3 | signature-render-ON ≠ OFF in side-by-side | press F1 toggle (§ 3.1.e) twice ; observe difference | "ON-version-feels-more-Renaissance" ≥ 4.0 / 5 (per § II.3.B [10:00-11:00]) |

**Failure protocol — Family-C** : Family-C ratings are sovereign-Apocky. If C-1 < 4.2 OR C-2 reports stutter OR C-3 shows no perceptible difference, M8 ¬ pass — mode "shippable-but-not-signature-quality". Open ESCALATIONS.md entry naming WHICH-of-(D116, D118, D119) is at-fault and remediation slice (T11-Dxxx) before J1 dispatch. The 6 D145 ACs (Family-A) all-passing is NECESSARY-but-NOT-SUFFICIENT.

---

## § 5. DETERMINISM VERIFICATION  (two runs, same seed → bit-equal)

```csl
§ determinism.bit-equal-replay  ←  H5 invariant @ specs/30 § VALIDATION § R-10
  W! two-runs same-seed @ same-host → bit-equal output-buffer
  W! two-runs same-seed @ same-host → bit-equal save-file
  W! save-load-save → bit-equal save-file
  W! observation : Apocky-host = Arc A770 ; CI-host = different-GPU ;
                  bit-equal-cross-host ¬ guaranteed (KAN-eval may use
                  vendor-fast-math) ; SAME-host bit-equal R! HOLD
```

| ☐ | Step | Command | Expected output |
|---|------|---------|-----------------|
| ☐ | 5.a | `target/release/loa-game.exe --m8-canonical-scene --seed 0xCAFEBABE --record-frames 60 --output-dir m8-det-run-1` | 60 frames captured to `m8-det-run-1/frame-{000..059}.raw16` |
| ☐ | 5.b | `target/release/loa-game.exe --m8-canonical-scene --seed 0xCAFEBABE --record-frames 60 --output-dir m8-det-run-2` | 60 frames captured to `m8-det-run-2/frame-{000..059}.raw16` |
| ☐ | 5.c | `pwsh -c "diff -r m8-det-run-1 m8-det-run-2"` | empty output ; exit 0 (bit-equal) |
| ☐ | 5.d | `cd compiler-rs && cargo test -p loa-game --release --test m8_integration_smoke save_then_load_round_trips_bit_equal -- --nocapture` | `test result: ok. 1 passed` |
| ☐ | 5.e | `cd compiler-rs && cargo test -p loa-game --release --test m8_integration_smoke two_independent_engines_with_same_seed_produce_same_phase_counters -- --nocapture` | `test result: ok. 1 passed` |

**Failure protocol — § 5** : Determinism is the load-bearing replay/audit invariant. If § 5.c shows ANY frame-diff OR § 5.d/e fails, M8 ¬ pass IMMEDIATELY. Do NOT proceed to § 6 onward. Open ESCALATIONS.md naming the suspected non-determinism source (KAN fast-math? thread-scheduling? clock-leak?). Block J1+ until determinism restored.

---

## § 6. HARDWARE-SPECIFIC  (Arc A770 frame-time observed + OpenXR-fallback)

```csl
§ hardware.arc-a770
  observation : Arc A770 ¬ canonical CI-target ; intel-driver-shader-compiler
                may differ from NVIDIA / AMD ; KAN-BRDF eval = matmul-heavy
                ⊗ Arc-XMX-units expected to-perform-well-but-not-yet-measured
  W! observe Stage-5 SDFRaymarch frame-time ≤ 2.5ms p99 (Quest-3 target)
  W! observe Stage-6 KANBRDFEval frame-time ≤ 1.8ms p99
  W! observe Stage-7 FractalAmplifier frame-time ≤ 1.2ms p99
  W! IF any-stage > target × 1.5  →  open ESCALATIONS.md ; M8 PASS-WITH-CAVEAT
```

| ☐ | Step | Command / action | Expected (Quest-3 reference) |
|---|------|------------------|-------------------------------|
| ☐ | 6.a | run loa-game canonical-scene 60 sec ; press F4 → per-stage timing-dump | per-stage dump printed to console |
| ☐ | 6.b | record Stage-1 EmbodimentPass p99 | ≤ 0.20ms (fallback ≤ 1.0ms in flat-mode) |
| ☐ | 6.c | record Stage-2 GazeCollapsePass p99 | ≤ 0.30ms (fallback ≤ 1.0ms in flat-mode) |
| ☐ | 6.d | record Stage-3 OmegaFieldUpdate p99 (async-compute lane) | ≤ 10.0ms (overlapped with graphics) |
| ☐ | 6.e | record Stage-4 WaveSolverPass p99 | ≤ 1.5ms |
| ☐ | 6.f | record Stage-5 SDFRaymarchPass p99 | ≤ 2.5ms ← LOAD-BEARING (M8 signature-render hot-path) |
| ☐ | 6.g | record Stage-6 KANBRDFEval p99 | ≤ 1.8ms ← LOAD-BEARING (KAN-BRDF correctness) |
| ☐ | 6.h | record Stage-7 FractalAmplifierPass p99 | ≤ 1.2ms ← LOAD-BEARING (sub-pixel detail) |
| ☐ | 6.i | record Stage-8 CompanionSemanticPass p99 | ≤ 0.6ms (or 0 if-toggle-off) |
| ☐ | 6.j | record Stage-9 MiseEnAbymePass p99 | ≤ 0.8ms (M11 stretch ; M8 = pass-through ≤ 0.1ms) |
| ☐ | 6.k | record Stage-10 ToneMapPass p99 | ≤ 0.3ms |
| ☐ | 6.l | record Stage-11 AppSWPass p99 | ≤ 0.1ms |
| ☐ | 6.m | record Stage-12 ComposeXRLayers p99 (or skip if no OpenXR) | ≤ 0.2ms OR `SKIP` row |
| ☐ | 6.n | total-frame p99 ≤ 16ms | sum-with-overlap ≤ 16ms (M7 budget HOLD) |

### § 6.0.1 Frame-time recording table (Apocky-fill at-time-of-verification)

```csl
§ frame-time-record  ←  Apocky fills @ verification ; commits with cert
  observation : per-stage budget = Quest-3 reference ; Arc A770 perf ¬
                yet-characterized ; expect ≥ Quest-3 (desktop-class GPU)
  observation : record-MIN + record-AVG + record-P99 ; not-just-P99
                ⊗ avg < budget ¬ sufficient if-P99-spikes
```

| Stage | Stage name | Budget (Quest-3 ref) | Apocky-fill MIN ms | Apocky-fill AVG ms | Apocky-fill P99 ms | ☐ within budget? |
|-------|------------|----------------------|--------------------|---------------------|---------------------|------------------|
| S1  | EmbodimentPass        | 0.20 (or skip)       | _____ | _____ | _____ | ☐ |
| S2  | GazeCollapsePass      | 0.30 (or fallback)   | _____ | _____ | _____ | ☐ |
| S3  | OmegaFieldUpdate      | 10.0 (async-overlap) | _____ | _____ | _____ | ☐ |
| S4  | WaveSolverPass        | 1.50                 | _____ | _____ | _____ | ☐ |
| S5  | SDFRaymarchPass       | 2.50                 | _____ | _____ | _____ | ☐ |
| S6  | KANBRDFEval           | 1.80                 | _____ | _____ | _____ | ☐ |
| S7  | FractalAmplifierPass  | 1.20                 | _____ | _____ | _____ | ☐ |
| S8  | CompanionSemanticPass | 0.60 (or 0)          | _____ | _____ | _____ | ☐ |
| S9  | MiseEnAbymePass       | 0.80 (or pass-thru)  | _____ | _____ | _____ | ☐ |
| S10 | ToneMapPass           | 0.30                 | _____ | _____ | _____ | ☐ |
| S11 | AppSWPass             | 0.10                 | _____ | _____ | _____ | ☐ |
| S12 | ComposeXRLayers       | 0.20 (or skip)       | _____ | _____ | _____ | ☐ |
| —   | TOTAL-frame           | 16.0 (M7-budget hold)| _____ | _____ | _____ | ☐ |

### § 6.1 OpenXR fallback verification (Stage-1 + Stage-12 SKIP path)

```csl
§ hardware.openxr-fallback
  observation : M8 LANDMINE row : "Stage-1 + Stage-12 require OpenXR.
                If Apocky's host doesn't have an OpenXR runtime installed,
                the test must skip-not-fail those stages."
  W! verify SKIP-not-FAIL behavior @ Apocky-host
```

| ☐ | Step | Command | Expected |
|---|------|---------|----------|
| ☐ | 6.1.a | IF § 1.4.b reported ActiveRuntime present : skip § 6.1 (use § 6.b + § 6.m above) | — |
| ☐ | 6.1.b | IF § 1.4.b reported (missing) : `loa-game --m8-canonical-scene --report-skipped-stages` | output names Stage-1 + Stage-12 in skip-list ; exit 0 |
| ☐ | 6.1.c | IF § 1.4.b reported (missing) : observe canonical-scene STILL renders (Stage-2..Stage-11 path) | flat-screen window shows scene ; ¬ panic ¬ blank-frame |
| ☐ | 6.1.d | IF § 1.4.b reported (missing) : log `OPENXR-RUNTIME-ABSENT-STAGE-1-12-SKIPPED` to § 9 sign-off | log row present |

---

## § 7. VISUAL-FIDELITY SMOKE  (capture 10 frames + compare to expected hash)

```csl
§ visual-fidelity.canonical-seed
  observation : per-pixel bit-equal cross-host = NOT-required (KAN-fast-math
                vendor-variation). Acceptable variation = ΔE_2000 ≤ 2.0 vs
                reference (per Omniverse § III.2.F spectral-shipping target ;
                M8 weakened to ΔE_2000 ≤ 4.0 since spectral-shipping = M9).
  W! capture 10 frames @ canonical seed + canonical camera-poses
  W! visually-compare to reference-set (`research/m8_reference_frames/`)
  W! per-frame ΔE_2000 ≤ 4.0  (M8 acceptance ; M9 tightens to 2.0)
```

| ☐ | Step | Command / action | Expected |
|---|------|------------------|----------|
| ☐ | 7.a | `target/release/loa-game.exe --m8-canonical-scene --capture-fidelity-frames --output-dir m8-fidelity-${date}` | 10 PNG frames written + 10 EXR raw-spectral frames |
| ☐ | 7.b | `target/release/loa-game.exe --diff-fidelity-frames --reference research/m8_reference_frames --candidate m8-fidelity-${date}` | per-frame ΔE_2000 row printed ; max ΔE_2000 ≤ 4.0 |
| ☐ | 7.c | open `m8-fidelity-${date}/frame-005.png` in image-viewer | scene visible : SDF-canonical primitives + KAN-BRDF spectral colors + sub-pixel-fractal detail |
| ☐ | 7.d | side-by-side compare frame-005 vs reference-frame-005 | visually-similar (Apocky judgment ; minor-variation OK) |
| ☐ | 7.e | record HASH of `m8-fidelity-${date}/frame-005.png` (`Get-FileHash -Algorithm SHA256`) | log to § 9 sign-off |
| ☐ | 7.f | optional : import all 10 frames into reference-side image-viewer ; flag any visibly-broken row | no row visibly-broken ; no NaN-pixels (white-blocks) ; no z-fighting ; no fireflies |

**Failure protocol — § 7** : if max-ΔE_2000 > 4.0 OR a frame is visibly-broken (white-blocks / fireflies / z-fight), M8 ¬ pass. Open ESCALATIONS.md naming WHICH-stage caused the regression (Stage-5 raymarch artifact? Stage-6 KAN spectral row blow-up? Stage-7 fractal over-amplification?). The reference-frames in `research/m8_reference_frames/` were recorded by D145 author on CI-Linux ; per § 7 above, cross-host variation is allowed but bounded.

---

## § 8. PER-SUBSYSTEM HEALTH  (each of 12 stages reports OK ; engine_health() = Ok aggregate)

```csl
§ subsystem-health.aggregate
  observation : loa-game::engine::Engine R! expose engine_health() → Result<HealthReport>
                ⊗ per-stage health-row + aggregate Ok/Warn/Err
                ⊗ HealthReport = SoA per Axiom 13 §II
  W! engine_health() = Ok aggregate
  W! ALL 12 stages = Ok (or SKIP for Stage-1/12 if OpenXR absent)
  W! ¬ any-Warn-row left-unreviewed (Warn = Apocky note in § 9)
```

| ☐ | Step | Command | Expected output |
|---|------|---------|-----------------|
| ☐ | 8.a | `target/release/loa-game.exe --m8-canonical-scene --health-report` | structured-text report ; aggregate row = `OK` |
| ☐ | 8.b | observe row : `Stage-1 EmbodimentPass : Ok` (or `Skip-OpenXR-Absent`) | one of the two values |
| ☐ | 8.c | observe row : `Stage-2 GazeCollapsePass : Ok` (or `Skip-fallback-fixed-pattern`) | one of the two values |
| ☐ | 8.d | observe row : `Stage-3 OmegaFieldUpdate : Ok` (mandatory ; ¬ skip) | `Ok` ; if Skip → BLOCK |
| ☐ | 8.e | observe row : `Stage-4 WaveSolverPass : Ok` | `Ok` |
| ☐ | 8.f | observe row : `Stage-5 SDFRaymarchPass : Ok` | `Ok` ; load-bearing |
| ☐ | 8.g | observe row : `Stage-6 KANBRDFEval : Ok` | `Ok` ; load-bearing |
| ☐ | 8.h | observe row : `Stage-7 FractalAmplifierPass : Ok` | `Ok` ; load-bearing |
| ☐ | 8.i | observe row : `Stage-8 CompanionSemanticPass : Ok` (or `Skip-toggle-off`) | one of the two values |
| ☐ | 8.j | observe row : `Stage-9 MiseEnAbymePass : Ok` (M8 = pass-through ; ¬ recursion) | `Ok` ; ¬ Warn-recursion-active |
| ☐ | 8.k | observe row : `Stage-10 ToneMapPass : Ok` | `Ok` |
| ☐ | 8.l | observe row : `Stage-11 AppSWPass : Ok` (M8 = motion-vec emit-only ; ¬ reproject yet) | `Ok` |
| ☐ | 8.m | observe row : `Stage-12 ComposeXRLayers : Ok` (or `Skip-OpenXR-Absent`) | one of the two values |
| ☐ | 8.n | aggregate-row at-bottom : `engine_health(): Ok` | exact match |

### § 8.1 Substrate-side subsystem health

```csl
§ subsystem-health.substrate
  observation : engine_health() covers render-pipeline ; substrate-side
                health = separate report from cssl-substrate-omega-field +
                cssl-substrate-projections + cssl-substrate-save
  W! all-three-substrate-crates report Ok
```

| ☐ | Step | Command | Expected |
|---|------|---------|----------|
| ☐ | 8.1.a | `target/release/loa-game.exe --substrate-health-report` | three rows : omega-field, projections, save ; all `Ok` |
| ☐ | 8.1.b | observe row : `OmegaTensor sheaf-glue : Ok ; H¹ classes detected = <N>` | N ≥ 0 ; sheaf-glue Ok |
| ☐ | 8.1.c | observe row : `Projections π_aesthetic + π_audio + π_collision : Ok ; latency-per-projection ≤ 1ms` | three Ok rows |
| ☐ | 8.1.d | observe row : `SaveScheduler : Ok ; pending-saves = 0` | Ok ; pending-saves = 0 |

### § 8.2 PRIME-DIRECTIVE attestation propagation health

| ☐ | Step | Command | Expected |
|---|------|---------|----------|
| ☐ | 8.2.a | `target/release/loa-game.exe --print-attestation-chain` | one row per crate : `loa-game`, `cssl-render-v2`, `cssl-substrate-omega-field`, `cssl-substrate-projections`, `cssl-substrate-prime-directive`, `cssl-host-openxr` (or `<absent>`), `cssl-substrate-save` ; all-rows have IDENTICAL attestation literal |
| ☐ | 8.2.b | confirm attestation literal exact-text : `"There was no hurt nor harm in the making of this, to anyone, anything, or anybody."` | byte-exact match across all rows |

---

## § 9. APOCKY-PM SIGN-OFF  (the load-bearing signature line)

```csl
§ sign-off.apocky-Φ
  observation : ALL prior sections produce data ; § 9 = the cert-document
                that ledger-records-the-pass + signs-with-Apocky-Φ
  W! produce attestation block in CSL3 + post-to ledger via DECISIONS.md
  W! commit-hash + datestamp + observations recorded
  W! file-name : `_drafts/phase_j/wave_j0_m8_pass_cert_${date}.csl`  (post-fill)
                then promoted-to `DECISIONS.md § T11-D150-PASS-CERT` upon
                successful-merge
```

### § 9.1 Sign-off block (Apocky fills + signs)

Copy-paste this block into `DECISIONS.md` under a new entry titled
`§ T11-D150-PASS-CERT — M8 acceptance verified by Apocky-Φ on Arc A770 host` :

```csl
§ M8-PASS-CERT  (per Omniverse 09_SLICE/M8_M9_M10_PLAN.csl § IX template)
  slice         : T11-D150  (M8 acceptance gate)
  date-passed   : YYYY-MM-DD                                      ← Apocky fill
  apocky-phi    : Apocky-Φ                                        ← Apocky sign
  host          : Arc A770 + Win11-${build} + Vulkan-${ver} + Driver-${ver}  ← § 1.3 fill
  openxr        : ${runtime-active OR runtime-absent}             ← § 1.4 fill
  commit-hash   : ${git-rev-parse-HEAD}                           ← § 1.1 fill
  acceptance    :
    family-A-D145-tests        : 6/6 ✓                            ← § 4.1 fill
    family-B-Omniverse-mech    : 8/8 ✓                            ← § 4.2 fill
    family-C-Apocky-host-only  : 3/3 ✓                            ← § 4.3 fill
    determinism                : 5/5 ✓                            ← § 5 fill
    hardware-arc-a770          : 13/13 ✓ (or document-skips)      ← § 6 fill
    visual-fidelity-smoke      : ΔE_2000-max ≤ 4.0 ; max=${val}    ← § 7 fill
    subsystem-health           : engine_health() = Ok aggregate   ← § 8 fill
  regression    : M7-acceptance ALL-still-pass                    ← § 4.2.B-4 fill
  corpus        : adversarial-Ops ≥ 200 ⊗ all-refused @ compile   ← § 4.2.B-7 fill
  telemetry     : audit-ring-hash <hash>                          ← from § 4.1.A-3 + § 8 reports
  ledger        : posted @ DECISIONS.md § T11-D150-PASS-CERT     ← post on merge
  observations  : <free-form-Apocky notes>                        ← Apocky fill
  notes         : <any-warn-rows-or-caveats>                       ← § 8 + § 6 caveats
  ⟨ATTEST M8⟩
  • PRIME-DIRECTIVE preserved @ all-surfaces introduced @ M8
  • AGENCY-INVARIANT triple ⟨consent, sovereignty, reversibility⟩ verified :
      consent       : 12-stage pipeline gated-by per-Sovereign Σ-mask + IFC-labels
      sovereignty   : capacity-floor preserved @ all-12-stages
      reversibility : all-new-surfaces fit-within rollback ≤ 200ms engineering-only
  • test-corpus extension committed ⊗ ≥ 100 new adversarial-Ops added @ M8 + ALL-refused @ compile-time
  • aggregate-effect-tensor verified-clean over M8-introduced-effect-rows
  • Σ-history append-only ⊗ no-rewrite via M8-introduced surfaces
  • audit-ring signed + verifiable @ M8-pass-cert
  • ESCALATIONS.md updated for any-edge-case encountered during-M8-verification (or no-escalation row)
  ⟨/ATTEST⟩
```

### § 9.2 Co-signature : PM-Claude  (parallel-cert ; ¬ override Apocky-Φ)

```csl
§ co-sign.PM-Claude
  observation : PM-Claude produces a parallel-cert attesting ALL CI-runs
                green @ commit-hash ; ¬ override Apocky-Φ ; documentary-only
  PM-Claude : <Claude-instance signs commit-hash + CI-state>
  CI-state  : workspace-gates ✓ ; m8_integration_smoke ✓ ; clippy ✓ ; fmt ✓
```

### § 9.3 Spec-Steward co-sign (advisory ; per Phase-J role-spec § 02)

```csl
§ co-sign.spec-steward
  observation : Spec-Steward attests : Omniverse § II.3 acceptance-rows
                MAPPED to AC-family-A/B/C with NO MISSING ROWS
  spec-steward : <agent-id> signs spec-coverage-mapping
```

### § 9.4 Recording site

| ☐ | Step | Action | Output |
|---|------|--------|--------|
| ☐ | 9.4.a | post § 9.1 block to `DECISIONS.md` under new `§ T11-D150-PASS-CERT` | DECISIONS.md grows by ~50 lines |
| ☐ | 9.4.b | run `cargo run --bin ledger-append -- --slice T11-D150 --pass-cert <path-to-block>` | ledger row appended ; hash printed |
| ☐ | 9.4.c | commit `git commit -m "§ T11-D150 : M8 acceptance verified by Apocky-Φ on Arc A770 host"` (HEREDOC body with full cert) | commit hash recorded ; sign-off ledger-row references this hash |
| ☐ | 9.4.d | tag `git tag -a m8-pass-cert -m "M8 verified ${date}"` | tag created |
| ☐ | 9.4.e | unblock J1+ : remove `BLOCKED-PENDING-M8-VERIFY` row from `SESSION_12_DISPATCH_PLAN.md § 7..§ 9` headers | dispatch plan updates ; J1+ slices now-eligible-to-launch |

### § 9.5 Optional : Apocky-personal-rating notes (Family-C narrative)

This is the only paragraph in this whole document that is meant to be free-form prose. It is the load-bearing-because-subjective testimony of the Family-C ratings :

> *Apocky writes : "I sat with the canonical SDF scene for the full 10 minutes. The signature-render-ON path felt [Renaissance-grade / production-grade / acceptable / thin]. The frame-time was [silky / occasional-stutter / bumpy]. Stage-5 raymarch artifacts were [absent / minor / visible]. Stage-6 KAN-BRDF spectral colors felt [vivid-and-correct / vivid-but-off / muted]. Stage-7 fractal detail was [present-and-tasteful / over-aggressive / under-amplified]. The pipeline as a whole gave me [confidence / mixed-feeling / concern] that we are ready for J1+ fanout."*

If the prose answers any of `[acceptable / thin / occasional-stutter / bumpy / minor / visible / off / muted / over-aggressive / under-amplified / mixed-feeling / concern]`, mark Family-C row as ◐ (partial) and consult § 10.

---

## § 10. FAILURE-MODE REGISTRY  (escalation matrix : block J1+ vs allow with caveat)

### § 10.1 Block-J1+-immediately conditions

```csl
§ block-j1.tier-1
  ANY-OF below-rows triggered  ⇒  M8 ¬ pass ⇒ J1+ BLOCKED until remediation
```

| Failure | Detection point | Remediation slice (est) | Block reason |
|---------|-----------------|--------------------------|---------------|
| § 1.2.* workspace-gate red | `cargo test --workspace -- --test-threads=1` non-zero exit | T11-D200 (gate-fix) | regression baseline broken ; cannot certify any state |
| § 4.1 A-1 12-stage smoke fail | `m8_integration_smoke::twelve_stage_pipeline_renders_one_frame_smoke` red | T11-D201 (pipeline-wire-fix) | M8 mechanical not even structurally-passing |
| § 4.1 A-4 attestation propagation fail | attestation-mismatch error | T11-D202 (attestation-discipline-fix) | PRIME_DIRECTIVE structural breach ; load-bearing |
| § 4.2 B-4 axiom-acceptance regression | any of 13 axiom-tests red | T11-D203 (axiom-regression-fix) | substrate broken under M8 ; cannot ship |
| § 4.2 B-6 frame-budget regression > 16ms p99 | HUD shows p99 > 16ms over 60s | T11-D204 (frame-budget-fix) | density-discipline broken (Axiom 13) |
| § 4.2 B-7 adversarial-corpus regression | < 200 corpus-rows passing | T11-D205 (corpus-restoration) | AGENCY-INVARIANT triple weakened |
| § 5 determinism diff non-empty | `diff -r m8-det-run-1 m8-det-run-2` non-empty | T11-D206 (det-fix) | replay/audit invariant broken |
| § 7 visual-fidelity max ΔE_2000 > 4.0 | per-frame report shows row > 4.0 | T11-D207 (fidelity-fix) | M8 signature-render quality unmet |
| § 8.d Stage-3 = Skip | health-report Stage-3 row ≠ Ok | T11-D208 (omega-field-restore) | substrate Ω-update broken ; cannot run |
| § 8.f-h Stage-5/6/7 = Skip OR Warn-unreviewed | health-report any of three load-bearing stages ≠ Ok | T11-D209 (signature-render-restore) | M8 signature-render trio broken |

### § 10.2 Pass-with-caveat conditions  (J1+ allowed ; documented in § 9.1 `notes:` row)

```csl
§ pass-with-caveat.tier-2
  observation : not-all-failures = total-block ; some-failures =
                acceptable-with-note + remediation-tracked
  observation : Apocky-Φ retains-final-judgment ; this-table = guidance
                NOT-binding
```

| Failure | Detection point | Caveat policy | Tracked-by slice |
|---------|-----------------|----------------|------------------|
| § 6.f-h stage-time exceed target × 1.0 but ≤ × 1.5 | per-stage timing-dump | document in cert ; J1+ allowed ; M9 must-tighten | T11-D210 (perf-tighten) |
| § 6.1.b Stage-1+12 SKIP @ no-OpenXR | console row | document ; M9 will-fill | (M9 row already-tracked) |
| § 4.3 C-1 visual-fidelity rating ∈ [4.0, 4.2) | Apocky-rating | document ; remediation-suggested-but-not-required | T11-D211 (visual-tighten) |
| § 8.i Stage-8 Companion = Skip-toggle-off | health-report row | M8 = OK ; M11 will-introduce | (M11 row already-tracked) |
| § 8.j Stage-9 Mise-en-Abyme = pass-through | health-report row | M8 = OK ; M11 will-introduce | (M11 row already-tracked) |
| panel-of-graphics-engineers test § II.3.C deferred to M8.1 sub-cert | absent at T11-D150 | document ; subsequent-sub-cert | T11-D212 (panel-recruit) |

### § 10.3 Apocky-Φ-decision-only conditions  (no-AI-can-override)

```csl
§ apocky-only.tier-3
  observation : some-conditions @ Apocky-Φ-sole-judgment ;
                AI-agent ¬ recommend-pass ; AI-agent ¬ recommend-block
                ; Apocky-Φ-decides
```

| Condition | What only-Apocky-can-judge |
|-----------|----------------------------|
| Family-C C-1 rating ∈ [3.5, 4.0) | "shippable-but-thin" vs "block-and-tighten" |
| Family-C C-3 ON-vs-OFF visible | "is-the-difference-Renaissance-grade" |
| § 9.5 prose contains "concern" | "is-the-concern-load-bearing" |
| Q1 (per M8_M9_M10_PLAN.csl § VII Q1) : "is M8 mandatory before launch" | post-M7 mandatory-vs-stretch promotion |

---

## § 10.4 Failure-flow decision tree  (visual aid)

```
                      ┌──────────────────────────────┐
                      │  M8 verification runs § 1-§ 8 │
                      └─────────────┬────────────────┘
                                    ▼
                       ┌────────────────────────────┐
                       │  Any § 10.1 tier-1 row red? │
                       └────────┬───────────────────┘
                            yes ┃ no
                                ▼ ▼
            ┌───────────────────────┐    ┌───────────────────────────┐
            │  M8 ¬ pass            │    │  Any § 10.2 tier-2 caveat?│
            │  ESCALATIONS.md       │    └────────┬──────────────────┘
            │  remediation slice    │         yes ┃ no
            │  J1+ BLOCKED          │             ▼ ▼
            └───────────────────────┘  ┌─────────────────────┐  ┌──────────────────┐
                                       │ M8 PASS-WITH-CAVEAT │  │ Any § 10.3       │
                                       │ J1+ allowed         │  │ Apocky-only judg?│
                                       │ caveats in cert     │  └────────┬─────────┘
                                       └─────────────────────┘     yes ┃ no
                                                                       ▼ ▼
                                                          ┌─────────────────────┐  ┌─────────────┐
                                                          │ Apocky-Φ decides    │  │ M8 PASS     │
                                                          │ block / pass / pass-│  │ J1+ allowed │
                                                          │ with-caveat         │  │ no caveats  │
                                                          └─────────────────────┘  └─────────────┘
```

---

## § 11. POST-VERIFY ARTIFACT DISCIPLINE

```csl
§ post-verify.artifact-discipline
  W! ALL artifacts produced during § 1-§ 8 retained
  W! artifact-naming : m8-verify-${YYYY-MM-DD}-${commit-short}-${suffix}
  W! artifact-storage : <repo-root>/_artifacts/m8-verify/  (gitignored)
  W! sign-off-doc + DECISIONS.md row = the only-things-committed
  W! raw-frames + telemetry-ring + crash-dumps = retained-in-_artifacts
                                                   for-30-days-min ;
                                                   archive @ 90-days
```

| ☐ | Step | Action | Output |
|---|------|--------|--------|
| ☐ | 11.a | move `m8-verify-failure-*.log/.mdmp` (if any) → `_artifacts/m8-verify/` | move done |
| ☐ | 11.b | move `m8-fidelity-${date}/` → `_artifacts/m8-verify/` | move done |
| ☐ | 11.c | move `m8-det-run-1/` + `m8-det-run-2/` → `_artifacts/m8-verify/` | move done |
| ☐ | 11.d | save Telemetry-ring snapshot → `_artifacts/m8-verify/telemetry-ring-${date}.csv` | snapshot saved |
| ☐ | 11.e | save § 9 sign-off block → `_artifacts/m8-verify/M8-PASS-CERT-${date}.csl` | block saved |
| ☐ | 11.f | confirm `.gitignore` covers `_artifacts/` (do-not-commit raw) | `.gitignore` row present |

---

## § 12. ANTI-PATTERNS  (verification-protocol-specific ; ¬ M8-spec-specific)

| Anti-pattern | Why it violates Wave-J0 verify-discipline |
|--------------|-------------------------------------------|
| Skipping § 1.2 workspace-gates | regression baseline unverified ; cannot certify any state |
| Running cargo-test in debug-mode only | optimizer-revealed bugs missed |
| Verifying on a non-Arc-A770 host | canonical-target unverified ; the whole-point-of-this-protocol-defeated |
| Family-C C-1 rating self-talk-up | violates honest-report discipline ; Apocky-Φ-sovereignty-of-judgment broken |
| Pass-with-caveat for any § 10.1 tier-1 row | block-conditions are structural ; weakening-prohibited per PRIME_DIRECTIVE §VI |
| Skipping § 5 determinism on grounds "but it's slow" | replay/audit invariant load-bearing ; never-skip |
| Skipping § 7 visual-fidelity on grounds "tests already pass" | visual-fidelity = HUMAN-only signal ; CI cannot substitute |
| Co-sign by AI-agent without explicit Apocky-Φ-signature | Apocky-Φ irreplaceable ; AI-co-sign is parallel-cert NOT override |
| Posting cert to ledger before § 4-§ 8 all-greens | cert ¬ premature ; cert = post-condition ¬ pre-condition |
| Marking M8 pass with § 1.4 Stage-1+12 OpenXR-absent without § 6.1.b SKIP-not-FAIL verified | LANDMINE row from SESSION_12 § 6 unaddressed ; J1+ may surface failure later |
| Verifying without `git status` clean | uncommitted-state may-influence run ; cannot reproduce |
| Suppressing § 9.5 prose because "it's just a feeling" | Family-C IS the load-bearing-subjective ; suppress = self-deception |

---

## § 13. ATTESTATION  (this-document)

```csl
§ verify-protocol-attestation
  • this-doc encodes Apocky-Φ-personal verification protocol for M8
  • this-doc preserves PRIME_DIRECTIVE §I.4 transparency-discipline :
    every check is documented + every result is loggable + every
    failure-mode is enumerated
  • this-doc preserves AGENCY-INVARIANT triple :
    consent       : Apocky-Φ runs ; AI-agents propose-but-do-not-decide
    sovereignty   : Family-C is sovereign-Apocky-judgment ; un-overrideable
    reversibility : verification-failure produces remediation-slice (T11-D200..D212)
                    ¬ silent-skip ; ¬ accept-broken-state
  • this-doc operationalizes Omniverse 09_SLICE/M8_M9_M10_PLAN.csl § XI ATTESTATION
  • this-doc is the concrete instantiation of "Apocky verifies M8 acceptance
    gate personally" from SESSION_12_DISPATCH_PLAN § 0 line-13
  ⟨ATTEST WAVE-J0-VERIFY⟩
  • protocol structurally encodes : pre-flight + build + run + ACs +
    determinism + hardware + visual-fidelity + subsystem + sign-off +
    failure-modes + post-verify-discipline + anti-patterns + attestation
  • protocol R! executed-end-to-end ¬ partial ¬ shortcuts
  • each ☐ tickbox load-bearing ; tick = verified ; un-ticked = un-verified
  ⟨/ATTEST WAVE-J0-VERIFY⟩
```

---

## § 14. PROMOTION-TO-LIVE STATUS

```csl
§ promotion.from-_drafts-to-active
  observation : this-doc lives in _drafts/phase_j/ pre-staging
  observation : promotion-trigger : SESSION_12 PM dispatches T11-D150
                M8-pipeline-wired commit + CI-green
  observation : promotion-action : move this-doc → CSSLv3-root OR
                _drafts/phase_j-ARCHIVED/ ; reference in SESSION_12_DISPATCH_PLAN § 6
  observation : Apocky-Φ may amend tickbox-set or AC-set BEFORE promotion
                without breaking discipline ; AFTER promotion, amendments
                require ESCALATIONS.md entry
  W! promotion-trigger satisfied  ⇒  move-to-active + lock-doc
  W! promotion-trigger un-satisfied  ⇒  retain-in-_drafts/ + iterate
```

| ☐ | Step | Action |
|---|------|--------|
| ☐ | 14.a | confirm T11-D150 closed (M8-pipeline-wired commit on `main`) |
| ☐ | 14.b | confirm CI-green @ T11-D150 commit-hash |
| ☐ | 14.c | move this-doc from `_drafts/phase_j/wave_j0_m8_verification_protocol.md` → `_artifacts/wave_j0/M8_VERIFICATION_PROTOCOL.md` (or repo-root-relative active-location per Spec-Steward) |
| ☐ | 14.d | reference-link added in `SESSION_12_DISPATCH_PLAN.md § 6` |
| ☐ | 14.e | Apocky-Φ executes the protocol on Arc A770 host |
| ☐ | 14.f | § 9 sign-off committed + ledger-row appended |
| ☐ | 14.g | J1+ unblocked (per § 9.4.e) |

---

## § 15. ACCEPTANCE  (this document ; meta-AC)

✓ § 0 problem-statement + thesis declared  
✓ § 1 pre-flight checklist : git-state ; workspace-gates ; Arc A770 driver-ver ; OpenXR runtime  
✓ § 2 build verification : cargo build --release + binary sanity + release-mode test  
✓ § 3 run verification : 10-min canonical playtest + memory-budget enforcement  
✓ § 4 acceptance-criteria checklist : Family-A (6 D145 ACs) + Family-B (8 Omniverse ACs) + Family-C (3 Apocky-only ACs)  
✓ § 5 determinism verification : two-runs same-seed bit-equal + save-load round-trip  
✓ § 6 hardware-specific : Arc A770 per-stage frame-time + OpenXR-fallback Stage-1+12 SKIP path  
✓ § 7 visual-fidelity smoke : 10 frames + ΔE_2000 ≤ 4.0 (M8 ; M9 tightens to 2.0) + Apocky judgment  
✓ § 8 per-subsystem health : engine_health() + substrate-side health + attestation-chain  
✓ § 9 Apocky-PM sign-off : explicit signature line + datestamp + commit-hash + observations + co-signs  
✓ § 10 failure-mode registry : tier-1 block / tier-2 caveat / tier-3 Apocky-only judgment  
✓ § 11 post-verify artifact discipline (gitignored ; retained 30-90 days)  
✓ § 12 anti-patterns enumerated  
✓ § 13 attestation block (PRIME_DIRECTIVE §IV-derived)  
✓ § 14 promotion-from-_drafts policy + tickbox-set  
✓ this-document line-count within 800-1500 LOC range  
✓ checklist-format consistent : ☐ tickbox + command-to-run + expected-output ∀ rows  
✓ CSLv3-dense reasoning blocks + English-prose where-clarity-demands  

## § 16. CROSS-REFERENCE INDEX  (quick-lookup)

| Section | What it covers | Source-of-authority |
|---------|----------------|---------------------|
| § 1.1 | git-state | local-discipline + DECISIONS.md |
| § 1.2 | workspace-gates | T11-D147 baseline |
| § 1.3 | Arc A770 driver | Apocky-host canonical-target |
| § 1.4 | OpenXR runtime | SESSION_12 § 6 LANDMINES |
| § 4.1 Family-A | 6 D145 ACs | SESSION_12 § 6 |
| § 4.2 Family-B | 8 Omniverse ACs | M8_M9_M10_PLAN.csl § II.3.A |
| § 4.3 Family-C | 3 Apocky ACs | M8_M9_M10_PLAN.csl § II.3.B + § II.3.D |
| § 5 | determinism | specs/30 § VALIDATION § R-10 (H5) |
| § 6 | per-stage frame-time | 06_RENDERING_PIPELINE.csl § V |
| § 7 | visual-fidelity | M8_M9_M10_PLAN.csl § II.3.D |
| § 8 | subsystem-health | loa-game::Engine::engine_health() |
| § 9 | sign-off cert | M8_M9_M10_PLAN.csl § IX template |
| § 10 | failure-modes | this-doc-novel ; routes to T11-D200..D212 |

═════════════════════════════════════════════════════════════════
∎  WAVE-J0 M8 VERIFICATION PROTOCOL  (pre-staging draft)
═════════════════════════════════════════════════════════════════
