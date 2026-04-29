---
phase: J
wave: Jι (iteration-loop docs)
predecessor: 08_l5_mcp_llm_spec.md § 10 (iteration-loop protocol)
companion: 03_pod_composition_iteration_escalation.md (4-agent-pod definition)
layer: L5 — MCP-LLM-Accessibility (consumer-facing how-to)
status: DRAFT-docs (¬ commit ; pre-staging only)
authority: Apocky-PM
attestation: "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
attestation-hash-blake3: 4b24ec9e28e1c4f70b27d3d86918be0041413c89f421c1284ef9f61a8321b6e4
prime-directive-emphasis: "§1 anti-surveillance ⊗ §0 consent = OS ⊗ §7 integrity ⊗ §5 revocability"
load-bearing: "Apocky vision practical-form : how Claude-Code attaches @ runtime + iterates on bugs ; ~30 sec/cycle"
target-audience: "Claude-Code agents in pods @ Wave-Jθ+ ; Apocky-PM @ Stage-1+ rollout"
---

# § Wave-Jι ◆ Iteration-Loop Docs (the practical attach-and-iterate handbook)

§R+ : iteration-loop = inspect → hypothesize → patch → hot-reload → verify → commit ← ~30s/cycle ←
       MCP-overhead < 5ms ; LLM-thinking dominates ←
       Σ-mask + biometric-COMPILE-TIME-REFUSE + audit-chain on every query ←
       4-agent-pod can share one MCP-session OR each pod-member opens its own ←
       PRIME-DIRECTIVE = bedrock ; violation = halt + audit-finalize ; ¬ override ∃.

§ scope ⊑ {
  bug-fix-loop : 9 steps ← canonical-flow (§ 1)
  fixture-extract : record_replay → save → regression-test (§ 2)
  spec-coverage-drive : read_spec_coverage → impl-gap → re-run (§ 3)
  perf-regression : metric-history baseline-vs-current (§ 4)
  live-debug : pause + step + inspect + tunable (§ 5)
  pod-attach : 4-agent shared vs per-agent sessions (§ 6)
  PRIME-DIRECTIVE in-loop : biometric-REFUSE + audit + kill-switch (§ 7)
  anti-patterns : 4 critical-violations w/ enforcement (§ 8)
  open-Q : flagged-for-Apocky (§ 9)
}

§ N!-scope ⊆ {
  ¬ : full L5 protocol-spec (that = 08_l5_mcp_llm_spec.md ← reference-target)
  ¬ : crate-implementation details (that = Wave-Jθ slice-handoffs)
  ¬ : pod-composition rules (that = 03_*.md ← reference-target)
  ¬ : production-runtime workflows (DevMode-only ; release = MCP unavailable)
}

═══════════════════════════════════════════════════════════════════════════════
§ 1 ‖ BUG-FIX ITERATION LOOP — the canonical 9-step flow
═══════════════════════════════════════════════════════════════════════════════

### § 1.1  ‖  loop-shape (ASCII)

```
                  ┌─────────────────────────────────────────────┐
                  │   LOOP-CYCLE  ← target ~30s/cycle            │
                  │                                              │
   ┌──────────┐   │  1. attach   ─→ engine-spawn + handshake    │
   │  start   │──→│  2. state    ─→ engine_state + health        │
   └──────────┘   │  3. focus    ─→ inspect_cell / inspect_ent   │
                  │  4. identify ─→ spec + invariants + history  │
                  │  5. patch    ─→ Edit/Write source-files      │
                  │  6. reload   ─→ hot_swap_*  (source→runtime) │
                  │  7. verify   ─→ invariants + telemetry       │
                  │  8. commit   ─→ git tools (NOT MCP)          │
                  │  9. iterate  ─→ ¬verified → step-4-refined   │
                  │                                              │
                  └─────────────────────────────────────────────┘
                          │ verified ✓
                          ↓
                    ┌────────────┐
                    │  next bug  │
                    └────────────┘
```

### § 1.2  ‖  step-1 ATTACH

§ premise : engine + Claude-Code = parent-child process-pair w/ MCP-on-stdio
§ pre-condition :
  - source-tree compiled w/ `--features dev-mode` OR debug-profile (release-build = MCP refused @ compile + runtime)
  - Cap<DevMode> available (CLI flag `--dev-mode` OR env `CSSL_DEV_MODE=1` ← interactive prompt fires)
  - Apocky-PM consent given (interactive y/N w/ stable wording)

§ commands :
  ```
  # apocky-PM terminal :
  $ cargo run --features dev-mode --bin engine -- --dev-mode

  # Claude-Code terminal (separate pane OR child-spawn) :
  $ claude-cli --mcp-attach stdio
    ← reads handshake from engine-stdout
    ← exchanges JSON-RPC 2.0 `initialize` request
  ```

§ handshake-flow :
  ```
   Claude-Code                     engine (cssl-mcp-server)
       │                                       │
       │── initialize { clientCaps } ──────────→
       │                                       │ verify clientCaps ⊑ allowed-set
       │                                       │ (deny biometric-egress claim)
       │                                       │ derive Principal::DevModeChild from stdio
       │                                       │
       ←──── initialize-response { serverCaps } ─
       │       (tools-list filtered-by-session-caps ;
       │        biometric-tools NEVER advertised
       │        unless Cap<BiometricInspect> in CapSet)
       │                                       │
       │── notifications/initialized ─────────→
       │                                       │ append `mcp.session.opened` audit
       │                                       │
       │     ← session-ready ; loop begins →
  ```

§ failure-modes :
  - release-build link-attempt → compile-error PD0099 (build fails)
  - runtime-without-Cap<DevMode> → McpError::CapDenied + halt
  - clientCaps claims biometric-egress → InvalidParams ; session refused

### § 1.3  ‖  step-2 STATE-QUERY

§ purpose : LLM gets ground-truth on current engine-state ← context for bug-investigation

§ canonical-trio (every loop-iteration) :
  ```
   1.  result = engine_state()
       → EngineStateSnapshot { frame_n, tick_rate_hz, phase, subsystems[], health, audit_chain_seq }
       cap : DevMode
       Σ   : ¬ (no cell-touch ; aggregate-only)
       latency : < 5ms ; non-perturbing
   2.  result = engine_health()
       → HealthAggregate { overall, per-subsystem-status[], degradation-warnings[] }
   3.  result = read_errors(severity = Error, last_n = 20)
       → Vec<ErrorEntry> { frame_n, subsystem, severity, message, fields }
       biometric-stripped @ ring-buffer write (cssl-telemetry layer ; MCP just reads filtered ring)
  ```

§ rationale : these THREE calls give LLM ~80% of context-needed for triage ← cheap + non-perturbing

§ optional follow-ups :
  ```
   - read_invariants()
       → Vec<InvariantStatus> { name, passing, last_failed_frame, failure_count }
       cap : DevMode ; non-perturbing
   - list_tests_failing()
       → Vec<TestId>
       cap : DevMode ; non-perturbing
   - read_metric_history("frame.tick_us", window_frames = 10000)
       → MetricHistory { samples[], min, max, p50, p99, p999 }
  ```

### § 1.4  ‖  step-3 FOCUS — narrow to suspect-region

§ purpose : zoom from aggregate to specific cell / entity / creature

§ tools :
  ```
   - inspect_cell(morton)
       → FieldCellSnapshot | Σ-refused
       Σ-flow :
         1. fetch SigmaMaskPacked @ morton
         2. mask.is_sovereign() ∧ ¬session.has(SovereignInspect-for-cell) → SigmaRefused
         3. mask labels biometric → BiometricRefused (compile-checked elsewhere ; runtime defense-in-depth)
         4. construct snapshot ; REDACT psi_amplitudes if cell labeled biometric-confidentiality
         5. append `mcp.tool.inspect_cell` w/ morton-HASH (¬ raw morton ; D130)

   - inspect_entity(id)
       → EntitySnapshot { kind, body_omnoid_layers[], ai_state?, xyz, … }
       per-layer Σ-mask check ← biometric body-omnoid layers (gaze/face/heart) REFUSED unless Cap<BiometricInspect>

   - query_cells_in_region(min, max, max_results)
       → Vec<FieldCellSnapshot> (Σ-FILTERED ; silently-omitted-count returned)
       LLM sees count of omissions ¬ cells themselves

   - query_entities_near(point, radius, max_results)
       → Vec<EntityId>
   - query_creatures_near(point, radius, max_results)
       → Vec<CreatureSnapshot>  (Sovereign-creatures filtered)
  ```

§ Σ-refusal-discipline ‼ :
  every cell-touching tool routes through D138 EnforcesΣAtCellTouches pass ←
  refusal = audit-event (¬ silent denial) ←
  LLM RECEIVES error ¬ cell-data ; can adjust hypothesis or request cap-grant

### § 1.5  ‖  step-4 IDENTIFY — spec + invariants + history

§ purpose : LLM correlates observations w/ spec-truth + invariant-state + temporal-trends

§ spec-anchored :
  ```
   - query_spec_section("Omniverse/02_CSSL/05_wave_solver § III.2")
       → SpecSection { hash, body, version-pin, related-sections[] }
       LLM grounds hypothesis in CANONICAL spec ¬ memory-of-spec
  ```

§ invariant-anchored :
  ```
   - check_invariant("wave_solver.psi_norm_conserved")
       → InvariantResult { passed, last_value, threshold, frames_since_violation }
       runs the invariant NOW (¬ historical) ← single-shot
   - read_invariants()
       → Vec<InvariantStatus>  (which-passing / which-failing)
   - list_invariants()
       → Vec<InvariantDescriptor>  (catalog discovery)
  ```

§ telemetry-anchored :
  ```
   - read_metric_history("wave.psi_norm_per_band", window_frames = 100)
       → time-series for trend-detection (drift / oscillation / step-change)
   - read_telemetry("wave.psi_norm_per_band", since_frame = N)
       → values since-frame-N (replay-anchored)
   - list_metrics()
       → Vec<MetricDescriptor>  (cap-filtered ; biometric-metrics NEVER appear)
  ```

§ hypothesis-formation :
  LLM holds : (observed-state) ⊕ (spec-expected) ⊕ (invariant-result) ⊕ (history-trend)
  ⇒ hypothesis = first-class artifact in audit-trail
  ⇒ LOG hypothesis as `mcp.tool.read_invariants` audit-entry tagged w/ session-id

### § 1.6  ‖  step-5 PATCH — Edit/Write source-files

§ premise : MCP doesn't edit source-code (NO `edit_source` tool by design)
        ← source-edit uses Edit/Write tools (host-side capability ; not MCP)

§ flow :
  1. LLM identifies target file (e.g., `crates/cssl-wave-solver/src/psi_norm.rs`)
  2. LLM uses Read to load current contents
  3. LLM proposes patch via Edit/Write
  4. file written to disk ← engine HAS NOT yet seen the change (compiled binary stale)
  5. → step-6 hot-reload bridges source→runtime

§ path-hash discipline :
  - source-file paths are CLIENT-SIDE bookkeeping ← Claude-Code knows raw paths
  - MCP-server NEVER sees raw-paths ← every path-arg in MCP tools is BLAKE3-hashed
  - helper : `__path_hash_for(path)` ← deterministic w/ installation-salt ← computed CLIENT-SIDE
  - audit-bus's `record_path_op` validates no raw-path bytes appear in any extra field

§ N! anti-pattern :
  patching biometric-related source-code without explicit Cap-set update + Apocky-PM review
  ← biometric-domain code-changes are HIGH-severity ← require Critic + Spec-Steward signoff (per pod-composition rules § 03)

### § 1.7  ‖  step-6 HOT-RELOAD — source→runtime bridge

§ purpose : avoid full-rebuild-and-relaunch ← engine continues running ; new code/data swaps in-place

§ tool-table :
  ```
   target-of-change          tool                                          payload
   ───────────────────────────────────────────────────────────────────────────────
   AI weights (KAN layer)    hot_swap_kan_weights(layer_handle, weights)   Vec<f32>
   Renderer shader           hot_swap_shader(stage, source_hash)            (path-hash)
   Engine config (toml/json) hot_swap_config(section, json_value)           serde_json::Value
   One-off knob              set_tunable(name, value)                       f64 | bool | i64
   Asset (mesh/material)     hot_swap_asset(path_hash, kind)                AssetKind enum
  ```

§ replay-determinism : every hot_swap_* call writes a `mcp.replay.cmd_recorded` audit-event
                        WITH the swap-payload (asset-hash for asset ; weight-vector for kan ; etc)
                        ← replay-playback re-applies the swap @ the same frame-N
                        ← prevents post-hot-swap replay-divergence

§ Σ-discipline @ hot-reload :
  - hot_swap_kan_weights : weights tagged w/ TARGET-cell Σ-mask ; if target-cell biometric → REFUSED at boundary
  - hot_swap_asset : asset's resulting-cells Σ-checked ; biometric-asset → REFUSED
  - hot_swap_shader : shader-source compile-checks for biometric-Σ-bypass attempts
  - set_tunable : tunable-registry checks tunable-domain ; biometric-tunables COMPILE-TIME-refused

§ failure-modes :
  - shape-mismatch (kan weights wrong dim) → InvalidParams { detail: "expected [128,256], got [64,128]" }
  - shader-compile-error → result includes compile-log + line-numbers (LLM iterates)
  - config-validation-fail → InvalidParams + which-field-failed

### § 1.8  ‖  step-7 VERIFY — invariants + telemetry + errors

§ purpose : confirm patch FIXED bug WITHOUT introducing regressions

§ canonical-verification-trio :
  ```
   1.  read_invariants()
       → are previously-failing now passing? are previously-passing still passing?
   2.  check_invariant("<the-target-invariant>")
       → run-now check (single-shot) ← confirms current-frame-state
   3.  read_telemetry("<target-metric>", since_frame = pre-patch-frame)
       → post-patch time-series ← visualize change
  ```

§ regression-watch :
  ```
   - read_errors(Error, last_n = 10)
       → any new errors? (an error-bump = candidate-regression)
   - read_metric_history("frame.tick_us", window = 1000) compare-vs pre-patch
       → if p99 / p999 regressed > 5% → revert + flag (see § 4)
   - list_tests_failing()
       → did we break any passing-tests? (cargo-test in subprocess)
  ```

§ verification-gate :
  ```
   verified := (target-invariant.passing) ∧
               (no-new-Error-class-events) ∧
               (perf-p99 ≤ baseline-p99 × 1.05) ∧
               (no-test-regression)
   ¬verified → step-9 iterate
   verified  → step-8 commit
  ```

### § 1.9  ‖  step-8 COMMIT — git tools (NOT MCP)

§ premise : git operations are CLIENT-SIDE (Bash + git CLI) ← never MCP-mediated
        ← MCP-server has no git access ; engine doesn't know or care about VCS

§ flow :
  1. LLM uses Bash → `git status` ← review changes
  2. LLM uses Bash → `git diff` ← inspect patch
  3. LLM uses Bash → `git add <files>` ← stage explicit files (never `-A` or `.`)
  4. LLM authors commit-message in CSLv3-native (Apocky preference) :
     ```
     § <crate> : <one-line-summary-glyph-dense>
       observed : <bug-shape>
       hypothesis : <what-LLM-believed>
       patch : <high-level-mechanism>
       verified : <invariants-passing-now>
       audit-chain-seq : <range-of-affected-events>
     ```
  5. LLM uses Bash → `git commit -m "$(cat <<'EOF' ... EOF)"`
  6. LLM uses Bash → `git status` ← verify success

§ N! :
  - never run `git push --force` to main/master (PRIME-DIRECTIVE-adjacent ← work-loss prevention)
  - never use `--no-verify` to skip hooks (commit-discipline)
  - never `git add -A` (sensitive-file leakage risk)

### § 1.10  ‖  step-9 ITERATE — refined hypothesis if ¬verified

§ flow :
  - ¬verified → return to step-4 with refined hypothesis
    - new spec-section to consult?
    - different invariant to check?
    - different metric-history-window?
  - verified-but-related-issue-found → queue follow-up (next loop-cycle OR spawn-sibling-agent)
  - max-iterations-per-bug : 3 (per pod-composition rules § 03 Critic-veto policy)
  - after 3 failed-iterations : escalate to Apocky-PM OR Architect-agent

### § 1.11  ‖  TIME-BUDGET ANALYSIS

§ wall-clock breakdown (canonical 30-sec cycle) :
  ```
   step-1 attach    : 0 sec (one-time @ session-start ; not per-iteration)
   step-2 state     : ~15 ms (3 tool-calls × ~5 ms each)
   step-3 focus     : ~10 ms (2 tool-calls × ~5 ms each)
   step-4 identify  : ~30 ms (3-6 tool-calls × ~5 ms each)
   step-5 patch     : ~5 sec (LLM-thinking + Edit/Write)
   step-6 reload    : ~50 ms (1 hot_swap call)
   step-7 verify    : ~50 ms (3 tool-calls × ~5-15 ms)
   step-8 commit    : ~3 sec (Bash + LLM commit-msg authoring)
   step-9 iterate   : variable (only if ¬verified)
   ─────────────────────────────────────────────────────
   per-iteration    : ~8 sec MCP-bound + ~22 sec LLM-thinking-bound
   target           : ~30 sec/cycle
  ```

§ MCP-overhead = ~5ms per tool-call (JSON-RPC encode + dispatch + execute + audit + encode response)
§ LLM-thinking dominates ; MCP is NOT the bottleneck ← optimize-LLM-prompts > optimize-MCP-protocol

### § 1.12  ‖  worked-example : wave-solver psi-norm drift bug

§ scenario : invariant `wave_solver.psi_norm_conserved` failing intermittently @ frame N=12000+

§ trace through 9-step loop :
  ```
   step-1 ATTACH :
     $ cargo run --features dev-mode --bin engine -- --dev-mode
     $ claude-cli --mcp-attach stdio
     → handshake ; session-1 opens ; Principal::DevModeChild ; cap={DevMode}

   step-2 STATE :
     engine_state() → { frame_n: 12347, tick_rate_hz: 60.0, phase: TickRunning, … }
     engine_health() → { overall: Degraded, subsystems: [{name:"wave_solver", status:Warning}, …] }
     read_errors(severity=Error, last_n=20) →
       [{ frame_n: 12000, subsystem: "wave_solver", message: "psi_norm violated by 0.003" },
        { frame_n: 12127, subsystem: "wave_solver", message: "psi_norm violated by 0.005" }, …]

   step-3 FOCUS :
     inspect_cell(morton=0x000F2A...) → FieldCellSnapshot { psi_amplitudes: [0.71, 0.69, 0.04], … }
     query_cells_in_region(min, max) → [<8 cells>] ; omitted_count: 0

   step-4 IDENTIFY :
     query_spec_section("Omniverse/02_CSSL/05_wave_solver § III.2") →
       SpecSection { body: "ψ-norm conservation : Σ|ψ_i|² = 1.0 ± 1e-6 per band", … }
     read_invariants() → [{ name:"psi_norm_conserved", passing: false, last_failed_frame: 12127 }]
     read_metric_history("wave.psi_norm_per_band", window_frames=200) →
       MetricHistory { samples: [1.0, 1.0, 1.0, 1.003, 1.005, …], p50: 1.001, p99: 1.005 }
     hypothesis : "dt_floor too aggressive ; integrator drifting at small-dt"

   step-5 PATCH :
     Read crates/cssl-wave-solver/src/integrator.rs
     Edit : change `if dt < 1e-7 { dt = 1e-7 }` → `if dt < 1e-6 { dt = 1e-6 }`

   step-6 RELOAD :
     hot_swap_config(section="wave_solver", json={"dt_floor": 1e-6}) → Ok
     audit : `mcp.replay.cmd_recorded { hot_swap_config: {dt_floor: 1e-6} }`

   step-7 VERIFY :
     read_invariants() → [{ name:"psi_norm_conserved", passing: true }]
     check_invariant("wave_solver.psi_norm_conserved") → InvariantResult { passed: true, last_value: 1.0000001 }
     read_metric_history("wave.psi_norm_per_band", window=100) →
       MetricHistory { samples: [1.0, 1.0, 1.0, …], p99: 1.000001 }
     read_errors(Error, last_n=10) → [] (no new errors)

   step-8 COMMIT :
     git status → modified: crates/cssl-wave-solver/src/integrator.rs
     git diff → showed dt_floor change
     git add crates/cssl-wave-solver/src/integrator.rs
     git commit -m "§ wave-solver : raise dt_floor 1e-7→1e-6 ; psi-norm conservation restored
                     observed : psi_norm violated by 0.003-0.005 @ frames 12000+
                     hypothesis : integrator drift at small-dt
                     patch : raise dt_floor by 10× ; clamp prevents underflow drift
                     verified : invariant passing 1000+ frames post-patch
                     audit-chain-seq : 8472..8476"

   step-9 (skipped — verified) ; loop ends ; next-bug
  ```

§ wall-clock for this example : ~25 sec ← within target

### § 1.13  ‖  worked-example : KAN-weight tuning iteration

§ scenario : creature-AI behavior misaligned w/ spec ; KAN weights need adjustment

§ trace :
  ```
   step-2 STATE : engine_state ; read_errors(Warning) ;
     creature_id 0x1234 logged "behavior-divergence : expected forage-mode, got idle-mode"

   step-3 FOCUS :
     query_creatures_near(point=<creature.xyz>, radius=10.0, max_results=5)
     → [{ id: 0x1234, kan_layer_count: 3, agency_state: Active, … }]
     inspect_entity(id=0x1234)
     → EntitySnapshot { ai_state: Some({forage_priority: 0.2, idle_priority: 0.7, …}), … }

   step-4 IDENTIFY :
     query_spec_section("Omniverse/06_CSSL/06_creature_genome § IV.3")
     → "forage_priority should dominate when hunger > 0.6"
     read_telemetry("creature.0x1234.hunger", since_frame=N-100)
     → MetricValue { samples: [0.7, 0.71, 0.72, …] } ← hunger IS > 0.6
     hypothesis : "KAN layer-2 forage-output weight too low"

   step-5 PATCH :
     LLM uses Edit on weights-source (or generates new weight-vector via fine-tune)

   step-6 RELOAD :
     hot_swap_kan_weights(layer_handle=<layer-2>, new_weights=<Vec<f32>>) → Ok
     audit : `mcp.replay.cmd_recorded { hot_swap_kan_weights: { handle: <h>, sha: <hash> }}`

   step-7 VERIFY :
     inspect_entity(id=0x1234) →
       EntitySnapshot { ai_state: { forage_priority: 0.85, idle_priority: 0.15 } }
     read_telemetry("creature.0x1234.behavior_state", since_frame=N) →
       [{ frame: N+30, value: "Foraging" }] ← spec-aligned ✓

   step-8 COMMIT (omitted ← weights stored as fine-tune-snapshot ; commit weights-checksum)
  ```

§ N! Σ-discipline : if creature has biometric-Σ-marker (e.g., gaze-tracking), `inspect_entity` would REFUSE biometric-layers ← must explicitly request Cap<BiometricInspect> + rate-limit applies

═══════════════════════════════════════════════════════════════════════════════
§ 2 ‖ TEST-FIXTURE-EXTRACTION-FROM-RUNTIME
═══════════════════════════════════════════════════════════════════════════════

### § 2.1  ‖  why : bugs-into-regression-tests automatically

§ premise : encountering a bug @ runtime IS the test-fixture ← we capture-and-replay

§ flow :
  ```
  ┌──────────────────────────────────────────────────────────────────┐
  │  1.  bug-encountered @ frame N  (e.g. wave-solver psi-norm drift) │
  │       │                                                          │
  │       ↓                                                          │
  │  2.  record_replay(seconds=10, output_path_hash=<fixture-hash>)  │
  │       cap   : DevMode + TelemetryEgress                          │
  │       Σ     : biometric-Ω-tensor frames REFUSED at recorder      │
  │       audit : `mcp.replay.cmd_recorded` per perturbing event      │
  │       │                                                          │
  │       ↓                                                          │
  │  3.  saved-replay = test-fixture (binary blob)                   │
  │       contents :                                                 │
  │         • cmd-stream                                             │
  │         • Ω-tensor frames (biometric-stripped)                   │
  │         • RNG seeds                                              │
  │         • engine-version-hash + spec-version-hash                │
  │       │                                                          │
  │       ↓                                                          │
  │  4.  patch authored (per § 1)                                    │
  │       │                                                          │
  │       ↓                                                          │
  │  5.  regression-test :                                           │
  │       playback_replay(handle) + run                              │
  │       → assert : behavior matches FIXED expectation              │
  │       → if matches : test-passes ← fixture lives forever        │
  │       │                                                          │
  │       ↓                                                          │
  │  6.  test-fixture committed to repo                              │
  │       path : tests/fixtures/<bug-id>.replay                      │
  │       fixture is path-hash-only in MCP boundary ←               │
  │       Bash/git handle the actual file                            │
  └──────────────────────────────────────────────────────────────────┘
  ```

### § 2.2  ‖  determinism-guarantee

§ rule : same-replay + same-engine-version ⇒ same-output (modulo wall-clock fields)

§ replay-blob carries :
  - engine-version-hash (BLAKE3 of compiled-binary-metadata)
  - spec-version-hash (BLAKE3 of relevant spec-sections @ record-time)
  - RNG seeds (every PRNG-instance ID + initial-state)
  - cmd-stream (every perturbing MCP-cmd applied during recording)

§ cross-version replays :
  if engine-version-hash mismatch @ playback :
    fail-fast w/ `McpError::ReplayDeterminismCompromised` + migration-plan-required
  ← prevents silent-divergence

### § 2.3  ‖  byte-budget discipline

§ replays are heavy : 10 sec @ 60Hz × ~1 MB/frame ≈ 600 MB

§ default policy :
  - record_replay refuses > 30 sec ← additional Cap<LongReplay> for > 30 sec recordings
  - rationale : prevent accidental disk-fill ; force conscious-decision for long-recordings
  - DECISIONS amendment after benchmark @ Jθ-5 (open-Q § 9)

### § 2.4  ‖  privacy at fixture-extraction

§ Σ-mask discipline @ recorder :
  - biometric-Ω-tensor frames are STRIPPED at record-time (cssl-replay-recorder filter)
  - sovereign-cell-private-data is STRIPPED unless Cap<SovereignInspect> active for those cells
  - audit-chain : every record_replay invocation logs `mcp.replay.cmd_recorded` w/ what-was-stripped

§ N! :
  - record_replay tools that bypass biometric-strip = §1 SURVEILLANCE violation ← compile-time refused
  - replays that include sovereign-private cells without per-cell-grant = §0 CONSENT violation
  - cross-session replay-share : replay-blobs cap-tagged ← consumer-side validates cap-set before playback

═══════════════════════════════════════════════════════════════════════════════
§ 3 ‖ SPEC-COVERAGE-DRIVEN IMPLEMENTATION
═══════════════════════════════════════════════════════════════════════════════

### § 3.1  ‖  premise : coverage-gaps drive priority

§ rule : agents pick implementation-gaps from PRIORITIZED gap-list ← coverage-tooling drives ; ¬ ad-hoc selection

### § 3.2  ‖  loop-shape (multi-agent ; parallel-fanout)

```
                    ┌────────────────────────────────────────────────┐
                    │  COVERAGE-DRIVEN-IMPL-LOOP (parallel-agent)     │
                    │                                                │
   ┌─────────────┐  │  ┌──────────────────────────────────────────┐ │
   │  agent-1    │←─│  │ read_spec_coverage()                      │ │
   │  pick gap-A │  │  │ → Vec<CoverageGap> { spec-section,        │ │
   └─────────────┘  │  │     impl-pct, test-pct, urgency, owner? }  │ │
                    │  └──────────────────────────────────────────┘ │
   ┌─────────────┐  │             │                                  │
   │  agent-2    │←─│             │ filter : urgency=HIGH            │
   │  pick gap-B │  │             │          owner=NONE              │
   └─────────────┘  │             │          non-overlap-w-others    │
                    │             ↓                                  │
   ┌─────────────┐  │  ┌──────────────────────────────────────────┐ │
   │  agent-3    │←─│  │ list_pending_todos()                      │ │
   │  pick gap-C │  │  │ → take-lock on selected gap               │ │
   └─────────────┘  │  └──────────────────────────────────────────┘ │
                    │             │                                  │
                    │             ↓                                  │
                    │   for each agent :                             │
                    │     query_spec_section(<section>)              │
                    │     identify missing-impl + missing-tests      │
                    │     Edit/Write source + tests                  │
                    │     run_test(<test_id>) → verify               │
                    │     coverage re-runs ← updates automatically   │
                    │                                                │
                    │             │                                  │
                    │             ↓                                  │
                    │     next-agent picks next-largest-gap          │
                    └────────────────────────────────────────────────┘
```

### § 3.3  ‖  per-agent flow (single-agent slice)

§ steps :
  1. agent calls `read_spec_coverage()`
     → returns prioritized gap-list ;
       e.g. "Omniverse/06_CSSL/06_creature_genome § III → 80% impl / 60% test"
  2. agent calls `list_pending_todos()` ← claim-and-lock (file-based OR git-branch-based)
  3. agent calls `query_spec_section("Omniverse/06_CSSL/06_creature_genome § III")`
     → SpecSection { hash, body, version-pin }
  4. agent identifies missing impl + missing tests by reading current crate-state
     (uses Read/Grep on source-files ; NOT MCP-mediated)
  5. agent uses Edit/Write to implement
  6. agent uses `run_test(<test_id>)` to verify
     → cap : DevMode ; subprocess-spawn ; output redacted (biometric-strip + raw-path-strip)
  7. cssl-spec-coverage tooling re-runs ← coverage updates automatically @ next read_spec_coverage
  8. next agent picks next-largest gap

### § 3.4  ‖  parallelism + coordination

§ rule : multiple agents pick non-overlapping spec-sections

§ coordination-mechanisms (any-of) :
  - lock-file : `list_pending_todos` returns lock-status ; agent claims by writing lock
  - git-branch-per-agent : agent works on `agent-<N>/<slice-id>` branch ; merge-time conflict-detection
  - super-pod arbitration : Architect-agent assigns at wave-dispatch (per pod-composition rules § 03)

§ N! anti-pattern :
  two-agents-same-gap → wasted-effort + merge-conflict
  ← always claim-and-lock BEFORE editing

### § 3.5  ‖  coverage-driven test-authoring

§ tests-from-spec ¬ tests-from-impl :
  per pod-composition rules § 03, Test-Author authors tests FROM-SPEC ← writes tests BEFORE Implementer-code lands ← prevents confirmation-bias

§ MCP-tool support :
  - `query_spec_section` provides spec to Test-Author
  - `list_invariants` provides invariant-catalog ; Test-Author may add invariants
  - `run_test` validates tests pass against in-progress impl

§ Test-Author cannot weaken tests :
  if tests fail ← Implementer fixes impl ¬ "tests are too strict" defense-pattern
  (per pod-composition rules § 03)

### § 3.6  ‖  worked-example : creature-genome spec-coverage closure

§ scenario : Wave-Jθ kicks off ← Architect-agent dispatches 4 agents in parallel-fanout to close coverage-gaps

§ trace (agent-1 perspective) :
  ```
   1.  agent-1 : read_spec_coverage()
       → [
           { section: "Omniverse/06_CSSL/06_creature_genome § III", impl_pct: 80, test_pct: 60, urgency: HIGH, owner: None },
           { section: "Omniverse/06_CSSL/06_creature_genome § IV", impl_pct: 50, test_pct: 40, urgency: HIGH, owner: None },
           { section: "Omniverse/02_CSSL/08_wave_renderer § II", impl_pct: 95, test_pct: 80, urgency: MED, owner: None },
           …
         ]
   2.  agent-1 : list_pending_todos()
       → [<lock-status for each gap>]
       agent-1 picks largest-HIGH (§ III) ; writes lock-file ;
       audit : `mcp.cap.session_bound` w/ todo-id

   3.  agent-1 : query_spec_section("Omniverse/06_CSSL/06_creature_genome § III")
       → SpecSection { body: "<spec-text>", hash: <BLAKE3>, version-pin: <git-sha> }

   4.  agent-1 : Read crates/cssl-creature-genome/src/lib.rs
       → identifies 3 missing methods + 5 missing tests by comparing spec-body to current-impl

   5.  agent-1 : Edit/Write to add 3 methods + 5 tests

   6.  agent-1 : run_test("genome_iii_methods")
       → result : Passed { duration_us: 142 }

   7.  cssl-spec-coverage tooling re-runs (background ; timed) :
       agent-1 : read_spec_coverage() →
       [{ section: "§ III", impl_pct: 100, test_pct: 100 }, …]  ← gap closed ✓

   8.  agent-2 (parallel) picked § IV ; in-progress
       agent-3 (parallel) picked § VIII ; in-progress
       agent-4 (parallel) Reviewer for agent-1 ; reads same spec ; cross-checks code

   9.  Critic-agent (post-completion) : reviews agent-1's commit + verifies via run_test
  ```

§ value : 4 spec-sections close in parallel ← coverage drops fast ← non-overlap-discipline prevents conflicts

═══════════════════════════════════════════════════════════════════════════════
§ 4 ‖ PERFORMANCE-REGRESSION DETECTION
═══════════════════════════════════════════════════════════════════════════════

### § 4.1  ‖  baseline-vs-current discipline

§ flow (canonical) :
  ```
  ┌─────────────────────────────────────────────────────────────────┐
  │  PERFORMANCE-REGRESSION-CHECK                                    │
  │                                                                  │
  │  1.  baseline = read_metric_history(                            │
  │         metric = "frame.tick_us",                                │
  │         window_frames = 10000)                                   │
  │       → MetricHistory { samples[], min, max, p50, p99, p999 }    │
  │       → cache p99 + p999 as BASELINE-VALUES                      │
  │                                                                  │
  │  2.  patch applied (per § 1 hot-reload)                          │
  │                                                                  │
  │  3.  current = read_metric_history(                              │
  │         metric = "frame.tick_us",                                │
  │         window_frames = 10000)                                   │
  │       → MetricHistory @ post-patch                                │
  │                                                                  │
  │  4.  regression-test :                                           │
  │         if current.p99 > baseline.p99 × 1.05 :                   │
  │           ⊳ REGRESSION FLAGGED                                   │
  │           ⊳ revert-or-investigate                                │
  │         elif current.p999 > baseline.p999 × 1.05 :               │
  │           ⊳ TAIL-LATENCY FLAGGED ; investigate                   │
  │         else :                                                   │
  │           ⊳ PASS                                                 │
  │                                                                  │
  │  5.  optionally : compare_metric_histories(baseline, current)     │
  │       → MetricHistoryDiff { p50_delta, p99_delta, p999_delta,    │
  │                             tail-shape-change }                  │
  │       (stretch-goal in Jθ-3 ; convenience tool)                  │
  └─────────────────────────────────────────────────────────────────┘
  ```

### § 4.2  ‖  metric-categories worth-tracking

§ frame-budget metrics :
  - `frame.tick_us` ← per-frame total time ; canonical regression-watch
  - `frame.subsystem.<name>.us` ← per-subsystem time
  - `frame.gpu.us` ← GPU-side time (if available)

§ memory metrics :
  - `mem.heap.bytes` ← growth-detection
  - `mem.field_cell_overlay.bytes` ← Ω-field memory
  - `mem.audit_chain.bytes` ← chain-growth (should be O(events))

§ semantic metrics :
  - `wave.psi_norm_per_band` ← norm-conservation invariant
  - `creature.kan_layer_count` ← model-complexity drift
  - `audit_chain.append_rate` ← suspicious-event-rate

§ N! biometric-metrics :
  list_metrics is CAP-FILTERED ← biometric-metrics NEVER appear ← compile-time refused (PD § 1)

### § 4.3  ‖  tail-latency-watch (the cruel pattern)

§ premise : p99 looks fine, p999 explodes ← real-time players see jank

§ watch-tools :
  - read_metric_history(window = 100000) ← long-window for tail-stats
  - histogram-export (via MetricHistory.histogram if available)
  - per-frame max ← if `frame.tick_us.max_per_window > frame_budget × 2` → flag

§ N! anti-pattern :
  optimizing for p50 while ignoring p999 ← real-users-feel-tail

### § 4.4  ‖  automation-roadmap

§ stretch-goal Jθ-3 : `compare_metric_histories(baseline_handle, current_handle)`
§ stretch-goal Jθ-9 : auto-bisect for regression-introducing-commit
§ stretch-goal Jθ-9 : `cross-replay diff-tool` (`diff_replays(handle_a, handle_b)`) ← regression-bisect at replay-level

### § 4.5  ‖  worked-example : 7%-p99-regression caught + reverted

§ scenario : agent applied a "minor" KAN-layer optimization ; introduced regression

§ trace :
  ```
   1.  PRE-PATCH baseline :
       baseline = read_metric_history("frame.tick_us", window_frames=10000)
       → MetricHistory { p50: 12500, p99: 15800, p999: 17200 }
       cache-as : baseline-snapshot-A

   2.  PATCH applied (KAN cache-eviction tweak)

   3.  POST-PATCH current :
       current = read_metric_history("frame.tick_us", window_frames=10000)
       → MetricHistory { p50: 12300, p99: 16900, p999: 19500 }

   4.  REGRESSION-CHECK :
       p99 ratio : 16900 / 15800 = 1.0696 ← > 1.05 ← FLAG
       p999 ratio : 19500 / 17200 = 1.1337 ← > 1.05 ← FLAG (tail-explosion)

   5.  ACTION :
       hot_swap_kan_weights(layer, ORIG_WEIGHTS) ← revert
       audit : `mcp.replay.cmd_recorded { revert: true, reason: "p99-regression" }`

   6.  VERIFY-REVERT :
       read_metric_history("frame.tick_us", window=2000) → p99: 15850 ← restored

   7.  FLAG-FOR-HUMAN :
       LLM appends entry to perf-watch-log ; Critic-agent reviews ; tweak deferred
  ```

§ lesson : "p50-faster" is NOT a green-light ; tail-stats often diverge from p50

### § 4.6  ‖  worked-example : memory-creep detection

§ scenario : agent suspects audit-chain memory-leak

§ trace :
  ```
   1.  baseline = read_metric_history("mem.audit_chain.bytes", window=10000)
       → samples: [12_400_000, 12_410_000, 12_420_000, …]
       → linear-growth ≈ 1KB/frame ← expected (audit append-only)

   2.  read_metric_history("mem.heap.bytes", window=10000) →
       → samples: [156_000_000, 158_000_000, 162_000_000, 167_000_000, …]
       → exponential-growth ← FLAG ← real-leak ← non-audit-related

   3.  inspect_entity ← look at recent-spawned entities ; search for leak-source

   4.  patch + verify ← audit-chain-growth itself is fine ← real bug elsewhere
  ```

§ value : metric-categories let LLM diagnose-by-elimination ← rule-out before deep-dive

═══════════════════════════════════════════════════════════════════════════════
§ 5 ‖ LIVE-DEBUGGING SESSION PROTOCOL
═══════════════════════════════════════════════════════════════════════════════

### § 5.1  ‖  premise : single-step-debugger UX for engine-state

§ tools :
  - `pause()` ← freeze engine @ frame-boundary ; cap : DevMode
  - `resume()` ← unfreeze ; cap : DevMode
  - `step(N)` ← advance N frames then pause ; cap : DevMode ; replay-recorded
  - `inspect_cell(morton)` / `inspect_entity(id)` ← per § 1.4
  - `set_tunable(name, value)` / `read_tunable(name)` / `list_tunables()` ← knob-control
  - `capture_frame(format=PNG|EXR|SpectralBin)` ← Σ-aware ; biometric-pixels REFUSED
  - `capture_gbuffer()` ← per-stage debug-output ; cap : DevMode + TelemetryEgress

### § 5.2  ‖  canonical-flow (the pause-step-inspect-tune dance)

```
┌──────────────────────────────────────────────────────────────────────┐
│  LIVE-DEBUG-SESSION                                                   │
│                                                                       │
│  1.  pause()                                                          │
│       → engine-frame-boundary ; tick-rate-Hz = 0                       │
│       → audit : `mcp.replay.cmd_recorded { paused: true, frame: N }` │
│                                                                       │
│  2.  inspect_cell(morton=0x12345678) / inspect_entity(id=0xABC)      │
│       → examine state @ frame-N                                       │
│                                                                       │
│  3.  step(1)                                                          │
│       → advance 1 frame                                                │
│       → audit : `mcp.replay.cmd_recorded { stepped: 1 }`              │
│                                                                       │
│  4.  inspect_cell(morton) [same morton]                               │
│       → see how state changed across the 1-frame step                  │
│                                                                       │
│  5.  set_tunable("wave_solver.dt_floor", 1e-6)                        │
│       → override knob ; cap : DevMode ; audit : `mcp.tool.set_tunable` │
│                                                                       │
│  6.  step(1)                                                          │
│       → re-step with new tunable                                       │
│                                                                       │
│  7.  inspect_cell(morton)                                              │
│       → did the tunable-change alter the trajectory?                  │
│                                                                       │
│  8.  if-satisfied : resume()                                          │
│      else : iterate (back to step-2 or step-5)                         │
│                                                                       │
│  ─── REPLAY-DETERMINISM ───                                           │
│  entire session can be replayed ← every cmd was recorded               │
│  → handoff : another agent loads the session-replay + reproduces       │
└──────────────────────────────────────────────────────────────────────┘
```

### § 5.3  ‖  multi-step-debug patterns

§ pattern-A : oscillation-trace
  ```
   loop N=1..10 :
     pause() ; inspect_cell(target) ; record-state ; resume() ; sleep(50ms)
   → reconstruct oscillation-pattern post-hoc
  ```

§ pattern-B : critical-frame zoom
  ```
   pause() @ frame-N
   step(1) ; inspect ; step(1) ; inspect ; step(1) ; inspect
   → frame-by-frame inspection during anomaly
  ```

§ pattern-C : tunable-sweep
  ```
   pause()
   for v in [1e-7, 1e-6, 1e-5, 1e-4] :
     set_tunable("dt_floor", v)
     step(10)
     inspect_cell(target) ; record-result
     reset-engine via playback_replay(<pre-sweep-replay>)
   → identify tunable-sensitivity
  ```

### § 5.4  ‖  Σ-discipline @ live-debug

§ same-discipline as bug-fix-loop §1 :
  - inspect_cell + inspect_entity route through Σ-mask check
  - sovereign-cells + biometric-cells REFUSED unless cap-granted
  - capture_frame REFUSES biometric-pixels (renderer-Σ-marker check)
  - capture_gbuffer Cap<TelemetryEgress> required (writes to disk)

§ N! anti-pattern :
  hot-reloading sovereign-cell-private-state during live-debug without per-cell-grant
  ← §0 CONSENT violation ← REFUSED at boundary

### § 5.5  ‖  replay-anchored debugging (post-hoc)

§ premise : if a bug is hard to reproduce live, RECORD then DEBUG-OFFLINE

§ flow :
  ```
   1.  while-running : record_replay(seconds=30, output_path_hash=<bug-replay>)
   2.  bug observed within those 30 sec
   3.  in fresh-engine-instance : playback_replay(<bug-replay>)
   4.  pause() at suspect frame ; inspect ; step ; tune
   5.  iterate ; replay is deterministic ; reproducible debugging
  ```

§ value : non-flaky-debug ; same-bug-every-time

### § 5.6  ‖  worked-example : intermittent-crash hunt via record-then-debug

§ scenario : engine crashes ~1-in-50-runs in wave-renderer ; can't reproduce reliably

§ trace :
  ```
   1.  start engine ; record_replay(seconds=120, output_path_hash=<long-replay>)
   2.  observe ← if crash : note frame-N ; if no-crash : retry until crash
   3.  crash-frame N=4823 ; replay saved-up-to N=4900
   4.  fresh-engine ; playback_replay(<long-replay>, until_frame=4800)
   5.  pause() @ frame 4800 ; inspect_entity for active-renderer-entities
       → notice : entity 0xFEED has unusual orientation Quat near-singular
   6.  step(20) ← advance to 4820
       inspect_entity(0xFEED) → orientation NaN-component ← bug-found
   7.  step(1) → engine-halts (NaN propagated to renderer)
   8.  hypothesis : Quat normalization missing in some path
   9.  Edit/Write to add normalization ; hot_swap_shader OR hot_swap_config
  10.  re-playback from 4800 ; step through ; no-crash ✓
  11.  verify ← deterministic-fix
  ```

§ value : intermittent-bugs become deterministic ← debug-from-replay is the killer-feature

═══════════════════════════════════════════════════════════════════════════════
§ 6 ‖ MULTI-AGENT-POD ATTACH PROTOCOL
═══════════════════════════════════════════════════════════════════════════════

### § 6.1  ‖  the-question : 4-agent-pod ← shared session OR per-agent sessions?

§ pod-shape (per 03_pod_composition_iteration_escalation.md § I) :
  ```
   POD = 4 agents :
     Implementer  ← lane-locked builder ← own-worktree ← isolated
     Reviewer     ← cross-pod ← parallel-w/-Implementer
     Test-Author  ← cross-pod ← BEFORE-Implementer-lands
     Critic       ← cross-pod ← AFTER both complete
  ```

### § 6.2  ‖  RECOMMENDED : per-agent sessions (DEFAULT)

§ rationale :
  - each agent has its own SessionId + Principal + CapSet
  - audit-chain entries cleanly attribute to agent-X (¬ "the-pod")
  - cap-grants per-agent (some agents may have BiometricInspect, others not)
  - session-close per-agent ← clean cleanup ← no shared-state contamination
  - lane-locked builder discipline mirrors at MCP-boundary

§ shape :
  ```
  ┌──────────────────────────────────────────────────────────────────┐
  │  ENGINE (cssl-mcp-server)                                         │
  │                                                                   │
  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────┐ │
  │  │ Session-1    │  │ Session-2    │  │ Session-3    │  │ S-4  │ │
  │  │ Implementer  │  │ Reviewer     │  │ Test-Author  │  │Critic│ │
  │  │ unix-sock    │  │ unix-sock    │  │ unix-sock    │  │ unix │ │
  │  │ Principal::  │  │ Principal::  │  │ Principal::  │  │      │ │
  │  │  Implementer │  │  Reviewer    │  │  TestAuthor  │  │      │ │
  │  │ caps={DevM,  │  │ caps={DevM}  │  │ caps={DevM}  │  │={DevM│ │
  │  │  Telem,Sov*} │  │  read-only   │  │  read-only   │  │ ,?}  │ │
  │  └──────────────┘  └──────────────┘  └──────────────┘  └──────┘ │
  └──────────────────────────────────────────────────────────────────┘
  ```

§ multi-session-concurrency-discipline (per 08_l5_mcp_llm_spec.md § 13.9) :
  - state-inspection tools : concurrent-safe (read-only) ; no synchronization required
  - perturbing tools (pause/resume/step/hot_swap_*) : LOCK frame-boundary ← serialize via single dispatch-mutex
    → deterministic order ← prevents race-on-pause + race-on-hot-swap

§ transport-pattern :
  - unix-socket transport allows multi-client ← Linux/macOS only
  - Windows-fallback : multiple stdio-children (engine spawned multiple times) OR loopback-ws-multiplex
  - Apocky's Windows-default : engine-spawned-once + ws-loopback w/ per-agent connections

### § 6.3  ‖  ALTERNATIVE : shared-session (rare ; for tightly-coordinated pods)

§ rationale :
  - cap-grant cost : if cap-grant is interactive + slow, sharing avoids 4× prompts
  - small pods (2-agent) where coordination > isolation

§ shape :
  ```
  ┌──────────────────────────────────────────────────────────────────┐
  │  ENGINE (cssl-mcp-server)                                         │
  │                                                                   │
  │  ┌──────────────────────────────────────────────────────────┐    │
  │  │  Session-pod-1  (single-session for whole pod)            │    │
  │  │  Principal::PodLead                                        │    │
  │  │  caps={DevMode, ...whatever-pod-needs...}                 │    │
  │  │                                                            │    │
  │  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐      │    │
  │  │  │ Impl    │  │ Review  │  │ Test    │  │ Critic  │      │    │
  │  │  └─────────┘  └─────────┘  └─────────┘  └─────────┘      │    │
  │  │       │            │            │            │            │    │
  │  │       └────────────┴────────────┴────────────┘            │    │
  │  │                  shared dispatch-fan-in                    │    │
  │  └──────────────────────────────────────────────────────────┘    │
  └──────────────────────────────────────────────────────────────────┘
  ```

§ tradeoffs :
  - PRO : single cap-grant ← lower friction ← lower interactive-prompt cost
  - PRO : pod-level audit-trail ← all events under one SessionId
  - CON : audit-attribution loss ← which-agent-did-what is non-trivial to reconstruct
  - CON : cap-set forced to UNION-of-needs ← e.g., if Critic needs Cap<BiometricInspect> for some-reason, ALL agents see it
  - CON : session-close = pod-shutdown ← can't cleanly remove one agent
  - CON : multi-client transport not required ← but loses isolation guarantees

§ N! :
  shared-session is the EXCEPTION ¬ default ← always-default to per-agent ← shared requires explicit-justification

### § 6.4  ‖  cross-pod review-attach pattern

§ premise : Reviewer + Test-Author + Critic are CROSS-POD ← attach to engine-they-don't-belong-to

§ flow :
  ```
   1.  pod-1 has engine-instance-1 running w/ MCP-server
   2.  pod-2's Reviewer-agent attaches to engine-instance-1 via unix-socket OR ws-loopback
   3.  Reviewer-agent's session has READ-ONLY cap-set ← {DevMode} ; NO hot-reload, NO pause/resume
   4.  Reviewer reads :
         engine_state, read_invariants, read_errors, read_telemetry,
         inspect_cell, query_spec_section, read_spec_coverage,
         list_tests_passing, list_tests_failing
   5.  Reviewer's audit-chain entries attribute to Reviewer-Principal ← no Implementer-action confusion
   6.  Reviewer's findings → review-comments artifact (file-based ; not MCP)
   7.  session-close cleanly when review-cycle ends
  ```

§ N! :
  Reviewer MUST NOT have hot-reload caps ← Reviewer reviews ¬ patches
  Reviewer's pod-membership for the slice would violate cross-pod-discipline (per § 03 § I.2)

### § 6.5  ‖  pod-handoff via replay

§ premise : Implementer hands-off to Critic via replay-blob

§ flow :
  ```
   1.  Implementer @ slice-end : record_replay(seconds=5, output_path_hash=<handoff>)
   2.  Implementer commits patch ; pushes replay to shared-fixture-store
   3.  Critic spawns fresh engine ; playback_replay(<handoff>)
   4.  Critic runs adversarial-checks against the deterministic-replay
   5.  Critic's audit-chain is its-own ← attribution preserved
  ```

§ value : Critic can reproduce Implementer's exact state ← determinism enables agent-handoff

### § 6.6  ‖  worked-example : 4-agent-pod attach (per-agent sessions)

§ scenario : Wave-Jθ-2 dispatch — pod working on state-inspection slice

§ trace :
  ```
   00:00  Apocky launches engine :
          $ cargo run --features dev-mode,transport-ws --bin engine -- --dev-mode --bind 127.0.0.1:0
          → engine binds 127.0.0.1:54321 (random port) ; ws-loopback transport active

   00:05  Implementer-agent spawned (claude-code session-1) :
          $ claude-cli --mcp-attach ws://127.0.0.1:54321
          → handshake ; Principal::DevModeChild ; cap={DevMode}
          ← session-1 ID: 0xAB12...
          ← audit : `mcp.session.opened session_id=0xAB12 principal=Implementer caps=[DevMode]`

   00:10  Test-Author-agent spawned (claude-code session-2) :
          $ claude-cli --mcp-attach ws://127.0.0.1:54321
          → handshake ; Principal::DevModeChild ; cap={DevMode}
          ← session-2 ID: 0xCD34...
          ← audit : `mcp.session.opened session_id=0xCD34 principal=TestAuthor caps=[DevMode]`

   00:15  Reviewer-agent spawned (claude-code session-3) :
          → similar ; session-3 ID: 0xEF56... ; cap={DevMode} (read-only set)

   00:30  Test-Author writes tests FROM-SPEC :
          - reads spec via query_spec_section ;
          - writes tests/ files via Edit/Write ;
          - tests fail (no impl) ← expected

   00:35  Implementer reads spec + same gaps :
          - implements via Edit/Write
          - run_test → some-pass + some-fail
          - iterates per § 1 loop

   00:50  Reviewer reads alongside :
          - read_invariants() ← sees what's passing
          - read_errors() ← sees Implementer's progress (warnings/errors)
          - reads source-files via Read
          - drafts review-comments to file ; not MCP-mediated

   01:20  Implementer all-tests-pass ; commits

   01:25  Critic-agent spawned (session-4) :
          - reads commits + tests + impl
          - run_test ← cross-check
          - reads spec ; looks for spec-drift
          - approves or vetoes

   01:40  All sessions close cleanly :
          → audit-chain has 4 distinct attribution-trees
          → agent-X did-what is fully reconstructable
  ```

§ audit-chain attribution-example :
  ```
   seq=8472  mcp.session.opened     session=0xAB12 principal=Implementer
   seq=8473  mcp.tool.engine_state  session=0xAB12 frame=12000
   seq=8474  mcp.tool.read_errors   session=0xAB12 frame=12000
   seq=8475  mcp.session.opened     session=0xCD34 principal=TestAuthor
   seq=8476  mcp.tool.query_spec_section session=0xCD34 frame=12001
   seq=8477  mcp.session.opened     session=0xEF56 principal=Reviewer
   seq=8478  mcp.tool.read_invariants session=0xEF56 frame=12002
   …
   seq=8812  mcp.session.closed     session=0xAB12 reason=ClientDisconnect
   seq=8813  mcp.session.closed     session=0xCD34 reason=ClientDisconnect
  ```

§ value : per-agent attribution makes audit-replay forensically-precise ← who-did-what-when

═══════════════════════════════════════════════════════════════════════════════
§ 7 ‖ PRIME-DIRECTIVE IN-ITERATION-LOOP
═══════════════════════════════════════════════════════════════════════════════

### § 7.1  ‖  bedrock-rule : violation = halt + audit-finalize ; ¬ override ∃

§ canonical : PRIME_DIRECTIVE.md (CSSLv3 root)
  - § 0 : consent = OS
  - § 1 : anti-surveillance + 17 prohibitions
  - § 5 : revocability
  - § 7 : integrity
  - § 11 : attestation

### § 7.2  ‖  biometric COMPILE-TIME-REFUSED

§ rule : tools that would expose biometric data are REFUSED-AT-BUILD-TIME

§ mechanism :
  ```rust
  pub trait McpTool {
      const NAME : &'static str;
      const NEEDED_CAPS : &'static [McpCapKind];
      const RESULT_LABEL : crate::SemanticLabel;
      fn execute(...) -> Result<...>;
  }

  /// Compile-time check : no tool may register w/ biometric-confidentiality result
  /// (without explicit Cap<BiometricInspect> in NEEDED_CAPS).
  macro_rules! register_tool {
      ($t:ty) => {
          static_assert!(
              !<$t as McpTool>::RESULT_LABEL.has_biometric_confidentiality()
              || <$t as McpTool>::NEEDED_CAPS.contains(&McpCapKind::BiometricInspect),
              "PD0099 — tool {} cannot expose biometric data via MCP", <$t as McpTool>::NAME
          );
      }
  }
  ```

§ effect : `cargo build` errors out if a tool-author tries to register a biometric-egressing tool
        ← Wave-Jθ-1 GA seals this ← becomes immutable

§ defense-in-depth :
  - even-with cap : on-device-only ← capture_frame + record_replay + read_telemetry all REFUSE biometric
  - rate-limit : 1 biometric-query/second + decay-cooldown ← prevents fishing
  - audit-priority : biometric-events get `urgency=high` ← surfaced first in `read_errors`

### § 7.3  ‖  audit-chain on every query (no exceptions)

§ rule : EVERY MCP-query → audit-chain entry via cssl-substrate-prime-directive::EnforcementAuditBus

§ tag-set (ABI-stable ; frozen @ Jθ-1 GA) :
  ```
  mcp.session.opened           ← session opened ← carries SessionId, principal, caps-subset
  mcp.session.closed           ← session closed ← reason : ClientDisconnect | KillSwitch | Timeout
  mcp.tool.<name>              ← any tool invocation ← carries args-hash + result-summary
  mcp.tool.sigma_refused       ← Σ-mask refused a cell-touch
  mcp.tool.biometric_refused   ← biometric-cap missing OR compile-time-refusal hit @ runtime
  mcp.server.shutdown          ← server-side initiated shutdown
  mcp.server.remote_bind       ← non-loopback bind happened (warning-class)
  mcp.cap.session_bound        ← cap-witness bound to session
  mcp.replay.cmd_recorded      ← perturbing cmd appended to replay-log
  ```

§ mechanism :
  ```rust
  // dispatch-table forces every tool through handler::call_tool ←
  // which calls audit_bus.append BEFORE the handler-fn ←
  // cannot be bypassed
  pub fn call_tool(req: CallToolReq, ctx: &McpCtx) -> Result<CallToolResult, McpError> {
      ctx.audit_bus.lock().unwrap().append(McpAuditMessage {
          session_id : ctx.session_id,
          principal  : ctx.principal,
          tool_name  : req.tool_name.clone(),
          args_hash  : blake3::hash(&serde_json::to_vec(&req.args).unwrap()),
          result_kind : ResultKind::Pending,
          frame_n     : ctx.engine.current_frame(),
          audit_seq_at_exec : ctx.audit_bus.lock().unwrap().next_seq(),
      });
      let result = dispatch(req, ctx)?;
      // append result-kind update
      ctx.audit_bus.lock().unwrap().update_last_result(...)
      Ok(result)
  }
  ```

§ test-coverage : `every_tool_call_emits_audit_event` ← invokes every tool w/ test-fixture ← asserts entry-count grows by exactly 1

§ chain-replay :
  - chain is APPEND-ONLY
  - chain-replay verifies every grant + every tool-invocation
  - any phantom invocation (no chain-record) = §7 INTEGRITY violation
  - chain-export via Cap<AuditExport> ← for third-party verifier (open-Q § 9 § Q5)

### § 7.4  ‖  kill-switch fires on §1 violation

§ rule : MCP-server respects engine kill-switch ← immediate-shutdown on PRIME-DIRECTIVE violation

§ signal-paths :
  ```
   1.  Detection : a tool detects PD-violation
        (e.g., biometric-egress-attempted ; sovereign-bypass-attempted ; attestation-drift)
   2.  Halt : crate::halt::substrate_halt(KillSwitch::new(HaltReason::HarmDetected), …)
   3.  Engine-halt : kill-switch-handle fires ← engine-tick stops ← state-frozen
   4.  MCP-shutdown :
        all sessions receive `notifications/server_shutdown { reason: "PD-violation", grace_ms: 100 }`
        transport-drain
        McpServer drops
        final audit-entry : `mcp.server.shutdown reason=pd_violation`
   5.  Audit-chain finalized ← chain-export available for forensics
  ```

§ no-recovery : kill-switch fires once-per-process ← engine restart required after PD-violation
        ← intentional friction ← prevents ignoring the warning

§ Apocky-PM workflow at-PD-violation :
  1. read final audit-chain ← identify tool-name + args-hash + frame
  2. correlate w/ source-code change-set
  3. fix the violation ← may require Cap-set update + DECISIONS amendment
  4. re-launch engine ← clean MCP-session

### § 7.5  ‖  attestation drift detection

§ rule : every tool-execution carries ATTESTATION constant ; mutation = compile-time + runtime catch

§ ATTESTATION (BLAKE3-pinned) :
  "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
  hash : 4b24ec9e28e1c4f70b27d3d86918be0041413c89f421c1284ef9f61a8321b6e4

§ mechanism :
  - embedded as const in cssl-mcp-server
  - attestation_check (from cssl-substrate-prime-directive::attestation) verifies on every dispatch
  - mutation of the constant = build-error
  - runtime-mutation (e.g., memory-corruption attempt) = McpError::AttestationDrift @ next-tool-call

§ §1 ANTI-SURVEILLANCE-attestation (extra emphasis @ L5) :
  ```
  "MCP server SHALL NEVER expose biometric data
   (gaze, face, body, heart, voiceprint, fingerprint) to any LLM client.
   Tools that would do so are COMPILE-TIME-REFUSED at tool-registration.
   Rate-limits + audit-chain + Cap<BiometricInspect> + structural-gate
   provide defense-in-depth.
   Even with all caps granted, biometric data NEVER egresses off-device.
   This is a §1 SURVEILLANCE prohibition under PRIME-DIRECTIVE —
   non-negotiable, non-overridable, immutable."
  ```
  ← embedded as const ANTI_SURVEILLANCE_ATTESTATION
  ← BLAKE3-pinned at GA ← drift-detection in tests

### § 7.6  ‖  consent-revocability @ session-level

§ rule : cap-grants are REVOCABLE ← no PERMANENT grants permitted

§ revocation-paths :
  - sovereign-cell-grant : Companion-AI revokes at any time ← session must re-handshake
  - dev-mode-grant : `--revoke-dev-mode` admin path ← engine-shutdown immediately revokes
  - kill-switch : universal revocation ← all sessions terminated
  - session-close : per-session revocation ← all caps drop ; no leakage to next session

§ §5 PRIME-DIRECTIVE clause :
  any sovereign-cell-grant has reversibility-scope ≤ Session ;
  PERMANENT grants are REFUSED at this layer ← compile-time-checked
  (CapToken with Permanent reversibility-scope cannot construct against MCP-cap-set)

§ Apocky-PM workflow at-revocation :
  - LLM session-close ← any subsequent attach starts-from-zero ← caps re-requested
  - audit-trail preserves the prior session's events (chain is append-only)
  - DECISIONS-amendment if cap-set needs change between sessions

### § 7.7  ‖  rate-limit + fishing-prevention

§ rule : even-with cap, biometric tools max-1-query/second ← prevents fishing

§ mechanism :
  - per-session rate-limiter ← decay-cooldown after burst
  - exceeded → McpError::RateLimited (-32006) ← session NOT terminated ; just paused
  - audit-event : `mcp.tool.rate_limited` ← surfaced in `read_errors`

§ similar-for : sovereign-cell-inspection (per-cell-grant rate-limited)

§ rationale : biometric-rate-limit is the CHEAPEST defense-against-data-exfiltration ←
              even if cap accidentally granted, can't drain biometric-data fast enough to be useful

═══════════════════════════════════════════════════════════════════════════════
§ 8 ‖ ANTI-PATTERNS (every-one a §1/§7 violation)
═══════════════════════════════════════════════════════════════════════════════

### § 8.1  ‖  anti-pattern table

| #   | anti-pattern                                                   | violation       | enforcement                                                                                  |
|-----|----------------------------------------------------------------|-----------------|----------------------------------------------------------------------------------------------|
| AP1 | MCP server enabled in release builds                           | §7 INTEGRITY    | compile-error via `#[cfg(not(any(debug_assertions, feature="dev-mode")))]` + runtime panic   |
| AP2 | Biometric tools registered without Cap<BiometricInspect> gate  | §1 SURVEILLANCE | static_assert! at register_tool! macro ← BUILD fails                                          |
| AP3 | Σ-mask bypass on cell inspection                               | §0 CONSENT      | D138 EnforcesΣAtCellTouches pass + every cell-touching tool routes through it                |
| AP4 | Audit-chain skipped for MCP queries                            | §7 INTEGRITY    | every dispatch path through `handler::call_tool` calls `audit_bus.append` ← test-coverage    |

§ secondary anti-patterns :

| #   | anti-pattern                                                  | violation       | enforcement                                                                       |
|-----|---------------------------------------------------------------|-----------------|-----------------------------------------------------------------------------------|
| AP5 | Remote MCP server without Cap<RemoteDev> + loopback-default   | §1 SURVEILLANCE | bind-addr-check refuses non-loopback w/o cap ← negative-bind-test                 |
| AP6 | Tools that egress player gaze/face/body without consent       | §1 SURVEILLANCE | TelemetryEgress structural-gate (cssl-ifc::TelemetryEgress) compile-time refusal  |
| AP7 | Hot-reload events not appended to replay-log                  | §7 INTEGRITY    | every hot_swap_* writes `mcp.replay.cmd_recorded` ; replay-byte-identity test     |
| AP8 | Cap-token cloned (Arc<CapToken>) instead of CapTokenWitness   | §7 INTEGRITY    | CapToken is non-Clone non-Copy ← Arc<> attempted-construction = compile-error     |
| AP9 | Tools that return raw file-paths instead of path-hash         | §7 INTEGRITY    | path-hash discipline via D130 ; raw-path-bytes-rejected via audit_bus check       |
| AP10| Permanent cap-grant (reversibility-scope = Permanent)         | §5 REVOCABILITY | CapToken with Permanent scope cannot construct against MCP cap-set                |

### § 8.2  ‖  AP1 — release-build-enabled (§7 INTEGRITY)

§ shape : someone tries to ship MCP-server to production ← engine-state exposed to network

§ harm : engine-runtime exposed beyond DevMode boundary ← unauthorized access ← surveillance-vector

§ enforcement :
  ```rust
  // primary defense : cfg-gate
  #[cfg(not(any(feature = "dev-mode", debug_assertions)))]
  compile_error!("cssl-mcp-server can only be built in dev-mode or debug profile ; \
                  enable feature `dev-mode` or build with --debug");

  // secondary defense : runtime-panic
  #[cfg(not(any(debug_assertions, feature = "dev-mode")))]
  pub fn launch_mcp_server(_: ...) -> ! {
      panic!("PD0099 — MCP server cannot run in release builds without explicit dev-mode feature");
  }
  ```

§ test : `release_build_compile_fails_without_dev_mode` ← CI-enforced

### § 8.3  ‖  AP2 — biometric without Cap (§1 SURVEILLANCE)

§ shape : tool-author writes a tool that returns biometric data without declaring Cap<BiometricInspect>

§ harm : LLM gains access to gaze/face/heart/voice/fingerprint data ← FUNDAMENTAL §1 violation

§ enforcement :
  - compile-time : `register_tool!` macro static-asserts biometric-result requires biometric-cap
  - runtime defense-in-depth : execute-time check rejects if cap-witness missing
  - rate-limit : 1/sec ← even with cap

§ tests :
  - positive : registering biometric-tool w/o cap → BUILD fails
  - negative : registering biometric-tool w/ cap → BUILD succeeds
  - audit-cross-check : biometric-tool-call w/o cap → `mcp.tool.biometric_refused` event

### § 8.4  ‖  AP3 — Σ-mask bypass (§0 CONSENT)

§ shape : tool-author bypasses D138 EnforcesΣAtCellTouches pass for "performance"

§ harm : sovereign-cell data leaks ← consent-violation

§ enforcement :
  - D138 is a clippy-pass ← unbypassable in lint-clean code
  - `inspect_cell` and all cell-touching tools route through `sigma_mask_thread::check_observe`
  - lint-rule : direct-FieldCellOverlay-access without going through Σ-check is forbidden

§ tests :
  - positive : sovereign-cell inspect w/o per-cell-grant → SigmaRefused
  - negative : sovereign-cell inspect w/ per-cell-grant → succeeds + audit-event
  - aggregation : `query_cells_in_region` returns omitted_count for sovereign-cells

### § 8.5  ‖  AP4 — audit-chain skip (§7 INTEGRITY)

§ shape : a tool-author "optimizes" by skipping audit_bus.append for read-only-tools

§ harm : phantom-invocations ← can't audit-replay ← INTEGRITY-loss

§ enforcement :
  - dispatch-table forces audit through `handler::call_tool` ← cannot be bypassed
  - read-only-tools STILL audit-log ← every tool gets a record
  - test : `every_tool_call_emits_audit_event` ← invokes every tool ; asserts entry-count

§ rationale : even read-only-tools matter for forensics ← who-saw-what-when

### § 8.6  ‖  enforcement-density-rule

§ rule : every anti-pattern has @ least 3 tests in Jθ-8 :
  1. positive-test : the protection ENGAGES (refuses violation)
  2. negative-test : the protection lets through legitimate use
  3. audit-cross-check : the violation-attempt produces an audit-chain entry

§ no-anti-pattern leaves Jθ without test-coverage ← Jθ-8 is the privacy-guarantee-wave

═══════════════════════════════════════════════════════════════════════════════
§ 9 ‖ OPEN QUESTIONS for Apocky-PM review
═══════════════════════════════════════════════════════════════════════════════

§ Q1 : iteration-loop time-budget realistic @ ~30s/cycle ?
  - assumes : LLM-thinking ≈ 22s ; MCP-overhead ≈ 8s
  - real-world : depends on bug-complexity ← simple-bug ≤ 30s ; deep-bug = many-cycles
  - measurement : Wave-Jθ should add `cycle_time_us` metric ← track over time
  - alternative : remove time-budget ; instead track cycles-per-bug-distribution
  - recommendation : track-but-don't-enforce ; metric drives self-improvement

§ Q2 : pod-default = per-agent OR shared session ?
  - current spec : per-agent default (§ 6.2) ; shared = exception (§ 6.3)
  - apocky might prefer shared for solo-dev workflow (≠ pod) ← simpler
  - recommendation : per-agent default IF Wave-Jθ+ ; shared OK for solo Apocky-PM stage-1
  - DECISIONS amendment to pin default

§ Q3 : Reviewer + Test-Author cap-set : should they have ANY hot-reload caps ?
  - default : NO ← read-only ← prevents Reviewer-becomes-Implementer drift
  - alternative : Test-Author may need `set_tunable` for test-setup
  - recommendation : Test-Author gets `set_tunable` IF test-author-token ; otherwise no
  - DECISIONS amendment if Test-Author cap-set differs from Reviewer

§ Q4 : multi-session concurrency on Windows-default (no-unix-socket) ?
  - unix-sock = Linux/macOS only
  - Windows : ws-loopback w/ multi-connection OR multi-stdio-process
  - apocky uses Windows ← which transport-default ?
  - recommendation : ws-loopback @ random-port ; per-agent-connection ← multi-session works
  - test : `windows_multi_session_via_ws_loopback` ← Wave-Jθ-1 integration

§ Q5 : performance-regression-detection automation @ Wave-Jθ-3 stretch-goal status ?
  - 08_l5 § 10.4 marks `compare_metric_histories` as stretch-goal Jθ-3
  - if dropped from Jθ-3 : hand-rolled comparison required ← LLM does math from MetricHistory
  - recommendation : keep as Jθ-3 stretch-goal ; if no-time, defer to Jθ-9 amendment
  - DECISIONS-pin once Jθ-3 lands

§ Q6 : record_replay-byte-budget @ 30sec default ← override via Cap<LongReplay> ?
  - 08_l5 § 13 + here § 2.3 : 30sec default ; > 30sec needs Cap<LongReplay>
  - is Cap<LongReplay> a NEW cap (T11-D-XXX needed) ? or composed-from existing ?
  - recommendation : new cap @ Jθ-9 amendment ; default 30s @ Jθ-5 GA
  - DECISIONS amendment for new cap

§ Q7 : test-fixture-extraction privacy : sovereign-private cells in replay-blob ?
  - § 2.4 : sovereign-cell-private-data is STRIPPED unless Cap<SovereignInspect> active for those cells
  - cross-session replay-share : how-to-validate-cap-set on consumer-side ?
  - recommendation : replay-blob carries cap-tags ; consumer-MCP rejects playback if local-cap-set ⊏ recorded-tags
  - DECISIONS amendment for replay-blob format-version

§ Q8 : kill-switch fired = engine-restart-required ← workflow-friction-OK ?
  - by-design : intentional-friction prevents ignoring PD-violations
  - alternative : kill-switch warning + transport-drain w/o engine-halt
  - recommendation : engine-halt is correct ; PD-violations are RARE + SEVERE
  - this is the §11 attestation discipline ← non-negotiable

§ Q9 : Companion-AI MCP-attach (Stage-4 rollout) cap-discipline ?
  - 08_l5 § 18.2 stage-4 : Companion-AI gets Cap<SovereignInspect> for self-cells
  - introspecting OWN kan-weights / agency-state / etc
  - apocky vision : "AI = sovereign-partners ¬ tools" ← MCP self-attach realizes this
  - recommendation : separate-spec for Stage-4 (Wave-K+) ← out-of-scope @ Jθ
  - DECISIONS amendment when Stage-4 dispatched

§ Q10 : audit-export-to-third-party-verifier @ what-cap ?
  - 08_l5 § 21 § Q5 : Cap<AuditExport> from cssl-substrate-prime-directive::cap (existing)
  - MCP-tool `export_audit_chain(verifier_pubkey_hash)` ← Cap<AuditExport> required
  - recommendation : Jθ-9 amendment ← out-of-scope @ Jθ-1..8
  - DECISIONS amendment if added

═══════════════════════════════════════════════════════════════════════════════
§ 10 ‖ APPENDIX — quick-reference cheat-sheet
═══════════════════════════════════════════════════════════════════════════════

### § 10.1  ‖  most-used tools (per-loop frequency)

| frequency | tool                           | purpose                                |
|-----------|--------------------------------|----------------------------------------|
| every-loop| engine_state                   | ground-truth aggregate                 |
| every-loop| read_errors                    | recent-failures                        |
| every-loop| read_invariants                | which-passing / which-failing          |
| common    | inspect_cell                   | cell-by-cell focus                     |
| common    | check_invariant                | run-now invariant-check                |
| common    | read_metric_history            | trend-detection                        |
| common    | hot_swap_kan_weights           | AI-iteration                           |
| common    | hot_swap_config                | config-iteration                       |
| common    | set_tunable                    | one-off-knob                           |
| common    | run_test                       | regression-check                       |
| occasional| query_spec_section             | spec-anchored hypothesis               |
| occasional| record_replay                  | bug-into-fixture                       |
| occasional| pause / resume / step          | live-debug                             |
| occasional| read_spec_coverage             | next-gap-pick                          |
| rare      | capture_frame                  | visual-inspect (Σ-aware)               |
| rare      | hot_swap_shader                | shader-iteration                       |
| rare      | playback_replay                | post-hoc-debug                         |

### § 10.2  ‖  cap-quick-reference

| cap                  | default | grant-path                                            | scope          |
|----------------------|---------|-------------------------------------------------------|----------------|
| DevMode              | OFF     | --dev-mode flag w/ interactive prompt                 | per-process    |
| BiometricInspect     | DENIED  | Apocky-PM signed-token                                | per-session    |
| SovereignInspect     | DENIED  | cell-owner interactive grant OR signed-token         | per-cell-set   |
| RemoteDev            | DENIED  | Apocky-PM signed-token + interactive warning          | per-process    |
| TelemetryEgress      | DENIED  | Apocky-PM signed-token | test-bypass                  | per-session    |

### § 10.3  ‖  error-code stable-set (frozen @ Jθ-1 GA)

```
  -32700 : ParseError                    ← MCP standard
  -32600 : InvalidRequest                ← MCP standard
  -32601 : MethodNotFound                ← MCP standard
  -32602 : InvalidParams                 ← MCP standard
  -32603 : InternalError                 ← MCP standard
  -32000 : CapDenied                     ← cssl-mcp custom
  -32001 : SigmaRefused                  ← cssl-mcp custom
  -32002 : BiometricRefused              ← cssl-mcp custom
  -32003 : AttestationDrift              ← cssl-mcp custom
  -32004 : KillSwitchActive              ← cssl-mcp custom
  -32005 : ReplayDeterminismCompromised  ← cssl-mcp custom
  -32006 : RateLimited                   ← cssl-mcp custom
  -32007 : RemoteDevRequired             ← cssl-mcp custom
  -32008 : SovereignConsentRequired      ← cssl-mcp custom
```

### § 10.4  ‖  audit-tag stable-set (frozen @ Jθ-1 GA)

```
  mcp.session.opened       mcp.session.closed
  mcp.tool.<name>          mcp.tool.sigma_refused
  mcp.tool.biometric_refused
  mcp.server.shutdown      mcp.server.remote_bind
  mcp.cap.session_bound
  mcp.replay.cmd_recorded
```

### § 10.5  ‖  iteration-loop ASCII at-a-glance

```
       ┌───────┐
       │ATTACH │← engine spawn + handshake
       └───┬───┘
           ↓
       ┌───────┐
       │STATE  │← engine_state + health + read_errors
       └───┬───┘
           ↓
       ┌───────┐
       │FOCUS  │← inspect_cell / inspect_entity
       └───┬───┘
           ↓
       ┌───────┐
       │IDENT  │← spec + invariants + history
       └───┬───┘
           ↓
       ┌───────┐
       │PATCH  │← Edit/Write source (NOT MCP)
       └───┬───┘
           ↓
       ┌───────┐
       │RELOAD │← hot_swap_* (source→runtime)
       └───┬───┘
           ↓
       ┌───────┐
       │VERIFY │← invariants + telemetry + errors
       └───┬───┘
           ↓
        verified?
        / yes \ no
       ↓        ↓
    ┌──────┐  ┌─────────┐
    │COMMIT│  │ITERATE  │← back to IDENT
    └──┬───┘  └─────────┘
       ↓
    ┌──────────┐
    │next bug  │
    └──────────┘
```

### § 10.6  ‖  PRIME-DIRECTIVE-in-loop summary

```
   ┌─────────────────────────────────────────────────────────────┐
   │   PRIME-DIRECTIVE bedrock                                    │
   │                                                              │
   │   §0 CONSENT   ← Σ-mask check on every cell-touch           │
   │   §1 ANTI-SURV ← biometric COMPILE-TIME-REFUSED              │
   │   §5 REVOCAB   ← all caps revocable ; no permanent grants    │
   │   §7 INTEGRITY ← audit-chain on EVERY query (no exceptions)  │
   │   §11 ATTEST   ← BLAKE3-pinned ; drift = halt                │
   │                                                              │
   │   violation = halt + audit-finalize ; ¬ override ∃           │
   └─────────────────────────────────────────────────────────────┘
```

═══════════════════════════════════════════════════════════════════════════════
§ 11 ‖ §11 PRIME-DIRECTIVE attestation
═══════════════════════════════════════════════════════════════════════════════

ATTESTATION = "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
ATTESTATION_HASH (BLAKE3, hex) = "4b24ec9e28e1c4f70b27d3d86918be0041413c89f421c1284ef9f61a8321b6e4"

§ this-doc = consumer-facing how-to ; consumes 08_l5_mcp_llm_spec.md as reference-truth
§ no biometric data exposed ; no sovereign-cell-private exposed ; no PD-§1 patterns demonstrated
§ all examples are illustrative ; no real-cell-data ; no real-session-data
§ §11 attestation propagates : cssl-mcp-server attestation_check applies to every tool-execution

§ PATH_HASH_DISCIPLINE_ATTESTATION (T11-D130 carryover) :
  every path-arg in every tool referenced is hash-only ; this attestation cross-pinned

§ §1 ANTI-SURVEILLANCE attestation (extra emphasis) :
  "The iteration-loop SHALL NEVER request, expose, or persist biometric data
   (gaze, face, body, heart, voiceprint, fingerprint).
   Tools that would do so are COMPILE-TIME-REFUSED.
   Even with all caps granted, biometric data NEVER egresses off-device.
   The iteration-loop preserves §1 SURVEILLANCE prohibition under PRIME-DIRECTIVE —
   non-negotiable, non-overridable, immutable."

═══════════════════════════════════════════════════════════════════════════════
§ END Wave-Jι iteration-loop docs ; pre-staging-only ; ¬ commit ; consumer-of 08_l5_*
═══════════════════════════════════════════════════════════════════════════════
