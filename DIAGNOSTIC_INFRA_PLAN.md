# § DIAGNOSTIC-INFRASTRUCTURE PLAN — 6-Layer L0..L5 (Phase-J)

⟦ master concise-index document ⟧
⟦ navigation + key matrices + cross-layer integration + capability discipline ⟧
⟦ full per-layer detail @ `_drafts/phase_j/{05,06,07,08}_*.md` ⟧

---

## § 0 · PREAMBLE + 6-LAYER ARCHITECTURE

This document is the **master plan** for Phase-J's diagnostic-infrastructure
build-out. It indexes four detailed draft specs (preserved at `_drafts/phase_j/`)
and surfaces the **load-bearing tables** + **cross-layer integration** + **capability
discipline** required to coordinate Wave-Jε..Jθ implementation.

**Scope** : six runtime-diagnostic layers (L0..L5) that turn the engine into
its-own-spec-coverage-witness, observable enough that an LLM (Claude-Code or
peer) can iterate against a running CSSLv3 engine via the MCP protocol —
without ever crossing PRIME-DIRECTIVE §1 (anti-surveillance), §10 (consent-OS),
or §11 (substrate-truth).

### § 0.1 · Six-layer stack (cultural layer = ground)

```
                           ┌─────────────────────────────────────────────┐
   L5  MCP-LLM (CROWN) ─── │ 41 tools × 9 categories × 5 capability gates│
                           │ JSON-RPC 2.0 / stdio / unix-sock / ws-loop  │
                           │ replay-determinism preserved through queries│
                           └─────────────────────────────────────────────┘
                                              ▲
                           ┌──────────────────┴──────────────────────────┐
   L4  HOT-RELOAD + TWEAK  │ 30+ tunables ; KAN-weight / shader / asset  │
                           │ live-swap ; replay-aware ; cap-gated        │
                           └─────────────────────────────────────────────┘
                                              ▲
                           ┌──────────────────┴──────────────────────────┐
   L3  RUNTIME INSPECT     │ cell / entity / region introspection        │
                           │ Σ-mask threading per cell-touch (D138)      │
                           └─────────────────────────────────────────────┘
                                              ▲
                           ┌──────────────────┴──────────────────────────┐
   L2  TELEMETRY-COMPLETE  │ ≈75 metrics × 12 subsystems × ABI-stable    │
                           │ + spec-coverage tracker + health-registry   │
                           └─────────────────────────────────────────────┘
                                              ▲
                           ┌──────────────────┴──────────────────────────┐
   L1  STRUCTURED LOG      │ macro-family ; sampling ; ring-buffer       │
                           │ path-hash discipline (D130) ; effect-row    │
                           └─────────────────────────────────────────────┘
                                              ▲
                           ┌──────────────────┴──────────────────────────┐
   L0  ERROR-CATCHING      │ EngineError + ErrorContext + panic-catch    │
                           │ severity-classification ; fingerprint+dedup │
                           └─────────────────────────────────────────────┘
                                              ▲
                           ┌──────────────────┴──────────────────────────┐
   LC  CULTURAL  (ground)  │ PRIME-DIRECTIVE §1 §10 §11 ; consent-OS     │
                           │ Σ-mask cell-overlay (D138) ; audit-bus      │
                           │ path-hash discipline (D130, D131, D132)     │
                           └─────────────────────────────────────────────┘
```

### § 0.2 · Top-level invariants (preserved across every layer)

| invariant                                       | source-of-truth     | enforced-by                    |
|--------------------------------------------------|---------------------|--------------------------------|
| **on-device-only** (no biometric egress)         | PD §1 + D129 + D132 | compile-time + runtime + audit |
| **path-hash discipline** (no raw paths in observability) | D130          | proc-macro + lint + audit-bus  |
| **Σ-mask threading** (every cell-touch checks Σ) | D138                | type-state + runtime gate      |
| **audit-chain on every grant + every query**     | D131                | EnforcementAuditBus            |
| **replay-determinism through MCP**               | H5 contract         | replay-log of every perturbing cmd |
| **no thread-blocking >100µs in hot-path**        | DENSITY_BUDGET §V   | budget-test + degraded-fallback|
| **PRIME-DIRECTIVE-trip ALWAYS-WINS aggregation** | PD §11              | hard-coded monoid              |

### § 0.3 · Reading order (for downstream waves)

| audience            | start with                                          |
|---------------------|------------------------------------------------------|
| Wave-Jε implementers | this doc § 2-3 ; then `_drafts/phase_j/05_l0_l1_error_log_spec.md` |
| Wave-Jζ implementers | this doc § 4 + § 8 ; then `_drafts/phase_j/06_l2_telemetry_spec.md` |
| Wave-Jη implementers | this doc § 5-6 ; then `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` |
| Wave-Jθ implementers | this doc § 7-10 ; then `_drafts/phase_j/08_l5_mcp_llm_spec.md` |
| Reviewers / PM       | this doc § 9 (capability) + § 11 (roadmap) + § 12 (acceptance) |

### § 0.4 · Source-of-truth pointers

- **L0+L1 detail** → `_drafts/phase_j/05_l0_l1_error_log_spec.md` (973 LOC)
- **L2 detail** → `_drafts/phase_j/06_l2_telemetry_spec.md` (1238 LOC)
- **L3+L4 detail** → `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` (1330 LOC)
- **L5 (CROWN) detail** → `_drafts/phase_j/08_l5_mcp_llm_spec.md` (1524 LOC)
- **DECISIONS-pin range** : T11-D150..T11-D201 (Phase-J reservation)
- **PRIME-DIRECTIVE** : `~/source/repos/CSLv3/PRIME_DIRECTIVE.md` (immutable)

---

## § 1 · L0 — UNIFIED ERROR-CATCHING

⟦ overview only ; full detail @ `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1 ⟧

### § 1.1 · Purpose + scope

Engine-wide error-catching surface that classifies every fault by severity,
captures structured context (subsystem + frame + cap-chain), fingerprints +
dedups recurring errors, catches panics at frame-boundaries, and feeds both
the audit-chain (when PD-trip) and the L2 metric `engine.error_count`.

### § 1.2 · Crate layout

| crate         | role                                                | depends-on                       |
|---------------|------------------------------------------------------|-----------------------------------|
| `cssl-error`  | `EngineError` aggregator + `ErrorContext` + severity | `cssl-substrate-prime-directive`  |
| `cssl-panic`  | panic-hook + frame-boundary catch + replay-record    | `cssl-error` + `cssl-replay`      |

### § 1.3 · Key types (signatures only ; full definition in draft 05)

```rust
pub enum EngineError {
    Substrate(cssl_substrate::Error),
    Render(cssl_render::Error),
    Physics(cssl_physics::Error),
    Wave(cssl_wave::Error),
    Audio(cssl_audio::Error),
    Spec(cssl_spec_coverage::Error),
    Cap(cssl_ifc::CapDenied),
    PrimeDirectiveTrip(PrimeDirectiveViolation),     // ALWAYS-WINS
    Other(BoxedDynError),
}

pub enum Severity { Info, Warn, Error, Critical, PrimeDirectiveTrip }
//                                              ↑ §11-required ; un-suppressible

pub struct ErrorContext {
    pub frame_n         : u64,
    pub subsystem       : &'static str,
    pub cap_chain       : SmallVec<[CapTag; 4]>,
    pub stack           : Option<StackTrace>,            // dev-builds only
    pub fingerprint     : [u8; 16],                      // BLAKE3-truncated
}
```

### § 1.4 · Discipline rules

- **never-unwrap-on-user-data** : per-crate clippy-lint `deny(unwrap_used, expect_used)` for any value derived from user-input
- **panic-hook installed @ frame-boundary** : panics caught + classified +
  replay-recorded ; engine continues at last-good-frame OR halts via kill-switch
  if PrimeDirectiveTrip
- **fingerprint + dedup** : recurring errors emit once-then-suppress (with count)
  to prevent log-flooding ; reset @ frame-N+1000 OR cap-chain-change
- **PRIME-DIRECTIVE trip is ALWAYS-WINS** : aggregation monoid hard-codes
  PrimeDirectiveTrip ⊔ X = PrimeDirectiveTrip

### § 1.5 · Cross-references

→ feeds L2 `engine.error_count` Counter (Severity-tagged)
→ feeds L5 MCP `read_errors(severity, last_n)` query
→ panics → audit-bus + `engine.dropped_frames(reason=panic)` Counter

⟦ full spec : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.1-1.8 ⟧

---

## § 2 · L1 — STRUCTURED LOGGING

⟦ overview only ; full detail @ `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2 ⟧

### § 2.1 · Purpose + scope

Single-path structured-logging surface. Macro-family expands to a ring-buffer
write with sampling + rate-limiting + path-hash-only discipline (D130). Sinks
include in-memory ring (mandatory), stderr (dev), file-rotated (dev), and
MCP-readable post-redaction view.

### § 2.2 · Crate

| crate       | role                                                |
|-------------|------------------------------------------------------|
| `cssl-log`  | log-macros + ring-buffer + sinks + redaction-pass    |

### § 2.3 · Macro family (compile-checked)

```rust
log_info!  (subsystem=wave_solver, frame=N, msg, key=val, …);
log_warn!  (subsystem=…,           frame=N, msg, …);
log_error! (subsystem=…,           frame=N, msg, …);
log_event! (subsystem=…,           frame=N, event=AgencyViolation, …);  // typed events
log_audit! (subsystem=…,           frame=N, audit_kind=GrantIssued, …); // dual-feeds audit-bus
```

### § 2.4 · Discipline rules

| rule                                  | enforcement-site                           |
|---------------------------------------|---------------------------------------------|
| no-raw-path in fields (D130)          | proc-macro check + audit-bus runtime check  |
| no-biometric-Label fields (D132)      | compile-refused via Label-typed fields      |
| determinism : log-call must not perturb sim | effect-row gate in strict-mode         |
| ring-buffer single-path : no fan-out  | static-init + sink-list-frozen-at-launch    |
| sampling : per-subsystem rate-cap     | static-config in `cssl-log::SAMPLING_TABLE` |

### § 2.5 · Subsystem catalog (11 canonical names)

```
wave_solver | physics | render | substrate | audio | xr | gaze
creature_ai | replay   | mcp    | spec_coverage
```

Adding a subsystem requires DECISIONS-pin + static-init registration. New
subsystems landing in Phase-J pre-register here.

### § 2.6 · Sinks (frozen-at-launch)

| sink              | enabled-when           | purpose                                 |
|-------------------|------------------------|------------------------------------------|
| ring-buffer       | always                 | feeds MCP `read_log` ; bounded per crate |
| stderr            | dev-build              | engineer feedback                        |
| file-rotated      | dev-build + flag       | persistent dev-trace                     |
| post-redact view  | when MCP starts        | biometric-strip + path-hash-strip layer  |

### § 2.7 · Cross-references

→ feeds L2 telemetry `log.entries_total` Counter (level-tagged)
→ feeds L5 MCP `read_log(level, last_n, subsystem_filter)` query
→ feeds audit-bus via `log_audit!` (single-path dual-feed)

⟦ full spec : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2.1-2.9 ⟧

---

## § 3 · L2 — TELEMETRY-COMPLETENESS LAYER

⟦ overview + load-bearing metric inventory ; full detail @ `_drafts/phase_j/06_l2_telemetry_spec.md` ⟧

### § 3.1 · Purpose + scope

Observability-first-class layer. Three pillars :
1. **`cssl-metrics`** — Counter/Gauge/Histogram/Timer primitives + ABI-stable schema + REGISTRY
2. **per-subsystem catalog** — every crate exposes ≥{frame_count, last_op_time_ns, health_state, error_count}
3. **`cssl-spec-coverage`** — runtime tracker that the-engine-knows-what-works-and-what-doesn't

### § 3.2 · Crates

| crate                  | role                                                     |
|------------------------|-----------------------------------------------------------|
| `cssl-metrics`         | Counter / Gauge / Histogram / Timer + REGISTRY + sinks    |
| `cssl-spec-coverage`   | SpecAnchor + ImplStatus + TestStatus + queryable registry |
| `cssl-health`          | health-registry + per-subsystem probes + aggregate roll-up|

### § 3.3 · Load-bearing : METRIC INVENTORY (≈75 metrics, 12 subsystems)

These metric-names are the **frozen-set** for Wave-Jζ-2. Adding a metric
requires a DECISIONS-pin. The per-metric tag-set is ABI-stable.

#### § 3.3.1 · Engine-frame metrics (7)

| metric                    | type      | tags                                | spec-cite           |
|---------------------------|-----------|-------------------------------------|---------------------|
| `engine.frame_n`          | Counter   | (mode ∈ {60,90,120,xr})            | DENSITY_BUDGET §V   |
| `engine.frame_time_ns`    | Timer     | (mode)                              | DENSITY_BUDGET §V   |
| `engine.tick_rate_hz`     | Gauge     | ()                                  | DENSITY_BUDGET §V/VI/VII |
| `engine.dropped_frames`   | Counter   | (reason ∈ {deadline,thermal,vram})  | DENSITY_BUDGET §V.7 |
| `engine.health_state`     | Gauge     | ()                                  | (this-spec § VI)    |
| `engine.mode_switches`    | Counter   | (from→to)                           | DENSITY_BUDGET §VI  |
| `engine.cmd_buf_count`    | Gauge     | (queue ∈ {graphics,async-compute})  | RENDERING §I        |

#### § 3.3.2 · Ω-step / substrate metrics (10)

| metric                                  | type      | tags                                                 |
|-----------------------------------------|-----------|------------------------------------------------------|
| `omega_step.phase_time_ns`              | Timer     | (phase ∈ {COLLAPSE,PROPAGATE,COMPOSE,COHOMOLOGY,AGENCY,ENTROPY}) |
| `omega_step.tick_rate_hz`               | Gauge     | ()                                                   |
| `omega_step.replay_determinism_check`   | Counter   | (kind ∈ {pass,fail})                                 |
| `omega_step.deferred_to_next_frame`     | Counter   | (phase)                                              |
| `omega_step.collapsed_regions_count`    | Gauge     | ()                                                   |
| `omega_step.cohomology_classes_active`  | Gauge     | (kind ∈ {birth,persist,transform,die})               |
| `omega_step.agency_violations`          | Counter   | (kind ∈ {consent,sov,reversibility,launder})         |
| `omega_step.entropy_drift_sigma`        | Gauge     | ()                                                   |
| `omega_step.observation_cap_overflow`   | Counter   | () (≤64/frame budget)                                |
| `omega_step.rg_tick_skipped`            | Counter   | (scale ∈ {s_2..s_7})                                 |

#### § 3.3.3 · Render-pipeline metrics (10)

| metric                              | type       | tags                              |
|-------------------------------------|------------|-----------------------------------|
| `render.stage_time_ns`              | Timer      | (stage ∈ 1..=12, mode)            |
| `render.gpu_memory_bytes`           | Gauge      | (pool ∈ {field,RC,KAN,RT,scratch})|
| `render.draw_calls`                 | Counter    | (stage)                           |
| `render.cull_rate`                  | Gauge      | (stage)                           |
| `render.foveation_savings_pct`      | Gauge      | (eye ∈ {L,R})                     |
| `render.sdf_marches_per_pixel`      | Histogram  | (stage=5, fovea-tier)             |
| `render.kan_eval_per_pixel`         | Histogram  | (stage ∈ {6,7})                   |
| `render.recursion_depth_witnessed`  | Histogram  | (stage=9)                         |
| `render.appsw_disable_events`       | Counter    | (reason ∈ {throat,spell,thermal}) |
| `render.cmd_buf_per_eye_pair`       | Counter    | () (=1 per-pair invariant)        |

#### § 3.3.4 · Physics metrics (8)

| metric                          | type       | tags                              |
|---------------------------------|------------|-----------------------------------|
| `physics.entity_count`          | Gauge      | (tier ∈ {T0,T1,T2,T3})            |
| `physics.broadphase_time_ns`    | Timer      | ()                                |
| `physics.constraint_iters`      | Histogram  | () COUNT_BUCKETS                  |
| `physics.spill_rate`            | Gauge      | ()                                |
| `physics.morton_collisions`     | Counter    | ()                                |
| `physics.sdf_collision_calls`   | Counter    | ()                                |
| `physics.tier_demotions`        | Counter    | (from→to)                         |
| `physics.tier_promotions`       | Counter    | (from→to)                         |

#### § 3.3.5 · Wave / spectral / XR metrics (22)

| group     | count | example                              | notes                              |
|-----------|-------|--------------------------------------|------------------------------------|
| wave      | 7     | `wave.psi_norm_per_band` (Gauge)     | per-band normalization watch       |
| spectral  | 7     | `spectral.kan_eval_count` (Counter)  | (kind ∈ {brdf,detail,collapse,oracle}) |
| xr        | 8     | `xr.frame_time_per_eye_ns` (Timer)   | per-eye 11.11ms budget             |

⟦ full per-metric tags + budgets in draft 06 § III.5-III.7 ⟧

#### § 3.3.6 · Anim / audio / Ω-field / KAN metrics (24)

| group        | count | key invariant                                              |
|--------------|-------|-------------------------------------------------------------|
| anim         | 5     | per-creature kan-eval + IK-iter histograms                  |
| audio        | 9     | per-stream submitted/dropped/underrun + bands               |
| omega_field  | 9     | tier-distribution + Σ-mask-mutation-rate + facet-writes     |
| kan          | 7     | persistent-kernel residency + quant-mode + weight-bytes     |

⟦ full per-metric detail in draft 06 § III.8-III.11 ⟧

#### § 3.3.7 · Gaze metrics (6 — §1-CRITICAL)

| metric                                          | type      | notes                              |
|-------------------------------------------------|-----------|------------------------------------|
| `gaze.saccade_predict_latency_ns`               | Timer     | aggregate-only ; ≤200ms budget     |
| `gaze.confidence_avg`                           | Gauge     | aggregate-only                     |
| `gaze.fovea_full_pixels`                        | Gauge     | percentage-of-eye                  |
| `gaze.privacy_egress_attempts_refused`          | Counter   | should ALWAYS = 0 (canary)         |
| `gaze.opt_out_active`                           | Gauge     | 0/1 (consent state)                |
| `gaze.collapse_bias_application_count`          | Counter   | per-frame                          |

‼ **Discipline** : ¬gaze-direction logged, ¬blink-pattern logged,
¬eye-openness-distribution logged. Only aggregate confidence + latency +
the never-tick-canary. Per D132 + PD §1.

### § 3.4 · Spec-coverage tracker (cssl-spec-coverage)

Runtime registry of `SpecAnchor { spec_root, spec_file, section, criterion, impl_status, test_status, citing_metrics }`.
Three sources-of-truth feed extraction :

| source     | extraction                                                          |
|------------|---------------------------------------------------------------------|
| PRIMARY    | `// § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V` code-comments  |
| SECONDARY  | DECISIONS.md per-slice spec-anchors                                 |
| TERTIARY   | test-name regex `[crate]_[fn]_per_spec_[file_anchor]`               |

Coverage queries surface to L5 MCP via `read_spec_coverage` / `query_spec_section` / `list_pending_todos` / `list_deferred_items`.

### § 3.5 · Health-registry (cssl-health)

Per-subsystem probe surface returning {Green, Yellow, Red, Critical} ; aggregate
roll-up to `engine.health_state`. Probes MUST complete in <100µs ;
timeout-with-degraded-fallback. Surfaces to L5 MCP via `engine_health` /
`subsystem_health`.

### § 3.6 · Cross-references

→ ingests L0 errors (severity-tagged) + L1 logs (level-tagged Counter)
→ exposes to L5 MCP via `read_telemetry` / `read_metric_history` / `list_metrics`
→ tag-set ABI-stability : adding tag-key requires DECISIONS-pin
→ schema-collision-test enforced @ static-init
→ spec-coverage build-fail if-< 100% completeness vs CATALOG (self-witness)

⟦ full spec + 15 anti-pattern register : `_drafts/phase_j/06_l2_telemetry_spec.md` ⟧

---

## § 4 · L3 — RUNTIME INSPECTION

⟦ overview only ; full detail @ `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 2 ⟧

### § 4.1 · Crate

| crate          | role                                                |
|----------------|------------------------------------------------------|
| `cssl-inspect` | non-perturbing read-only inspection of cells / entities / regions / time / capture-frames |

### § 4.2 · Surface (key entry points)

```rust
inspect.cell(morton)                  -> FieldCellSnapshot  | SigmaRefused
inspect.cells_in_region(min,max,cap)  -> Vec<FieldCellSnapshot> (Σ-filtered)
inspect.entity(id)                    -> EntitySnapshot     | SigmaRefused
inspect.entities_near(point,r,cap)    -> Vec<EntityId>
inspect.creatures_near(point,r,cap)   -> Vec<CreatureSnapshot>
inspect.kan_eval(test_input, handle)  -> KanEvalTrace       (Cap<DevMode>)
inspect.invariants()                  -> Vec<InvariantStatus>
inspect.check_invariant(name)         -> InvariantCheckResult
inspect.capture_frame(format,region)  -> FrameCaptureHandle (Cap<TelemetryEgress>)
inspect.time.{pause,resume,step,record_replay,playback_replay}
```

### § 4.3 · Σ-mask threading + privacy enforcement

Every cell-touch routes through D138's `EnforcesΣAtCellTouches` pass. The pass :

1. Fetch `SigmaMaskPacked` @ morton from `FieldCellOverlay`
2. Check op-class permission (Observe / Sample / Modify) against session-grants
3. Σ-refuse if no ; biometric-refuse if D132-Label hits ; else proceed
4. Append audit-event w/ morton-hash (D130 path-hash discipline applies to cell-keys)

Aggregation queries return Σ-FILTERED list ; cells the session can't see are
silently omitted (NOT refuse-whole-query) ; the omitted-count is reported in
result so LLM knows it's incomplete.

### § 4.4 · Compile-time biometric refusal

Snapshot types whose result-Label has biometric-confidentiality cannot be
returned by tools without explicit `Cap<BiometricInspect>` requirement.
Static-assert at trait-impl boundary. **The exception is also bounded** : even
with cap, EGRESS off-device is BANNED (§ 9 § 7 below).

### § 4.5 · Capture-frame Cap-gating

`capture_frame` requires `Cap<TelemetryEgress>` AND refuses biometric pixels at
capture-time (renderer Σ-marker check on output-region). Output paths are
hash-only (D130) ; client supplies pre-computed hash.

### § 4.6 · Cross-references

→ feeds L5 MCP : 12 of the 41 tools live in this surface (cell + entity + invariants + spec-coverage + time-control + capture)
→ uses L2 metrics for invariant checks (e.g. `wave.psi_norm_per_band` powers `wave_solver.psi_norm_conserved` invariant)
→ uses L1 logs for inspect.read_log feed
→ time-control : pause / resume / step replay-recorded for determinism

⟦ full spec + snapshot schemas + 9 privacy-enforcement table : `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 2.1-2.9 ⟧

---

## § 5 · L4 — HOT-RELOAD + LIVE-TWEAK

⟦ overview + load-bearing tunable registry ; full detail @ `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 3-4 ⟧

### § 5.1 · Crates

| crate              | role                                                          |
|--------------------|----------------------------------------------------------------|
| `cssl-hot-reload`  | OS-pump + atomic shader/asset/config/KAN-weight swap          |
| `cssl-tweak`       | typed tunable-registry + range-check + replay-record + audit  |

### § 5.2 · Hot-reload surface (4 swap-classes)

```rust
hot_reload.hot_swap_asset(path_hash, kind)            -> ReloadResult
hot_reload.hot_swap_kan_weights(layer_handle, weights) -> ReloadResult
hot_reload.hot_swap_shader(stage, source_hash)         -> PipelineRebuildResult
hot_reload.hot_swap_config(section, json)              -> ReInitResult
```

### § 5.3 · Hot-swap flow (4-step atomic)

1. **validate** : path-hash valid ; shader compiles ; config schema-checks ; KAN-weight dimension matches
2. **stage** : new asset uploaded to GPU / new shader pipeline built / new config parsed (engine continues running w/ OLD)
3. **frame-fence** : wait for current frame-in-flight to retire
4. **apply** : atomic swap @ frame-boundary ; replay-event recorded ; audit-entry appended

If any step fails, OLD remains active ; engine continues uninterrupted ;
PipelineRebuildResult::Failed(err) returned.

### § 5.4 · KAN-weight hot-swap (the key creature-AI iteration tool)

- `layer_handle` ← obtained via L5 `query_creatures_near` + `inspect_entity`
- `weights` ← LLM-supplied f32-vector ; length verified against layer dimension
- IFC discipline : weights MUST NOT have biometric-confidentiality Label ; else `BiometricRefused`
  ← e.g. LLM cannot stuff player-gaze-derived weights into a creature-AI layer
- persistent-kernel residency preserved across swap ; KAN-runtime residency-table updated atomically

### § 5.5 · Load-bearing : DEFAULT TUNABLE REGISTRY (30 tunables)

Initial set ; adding a tunable requires DECISIONS-pin + `tweak::register(&Spec)`.

| canonical_name                         | kind       | range          | default | budget | units    |
|----------------------------------------|------------|----------------|---------|--------|----------|
| `render.fovea_detail_budget`           | F32        | 0.0..1.0       | 1.0     | warn   | -        |
| `render.foveation_aggression`          | F32        | 0.0..2.0       | 1.0     | warn   | -        |
| `render.spectral_bands_active`         | U32        | 1..16          | 16      | hard   | bands    |
| `render.exposure_compensation`         | F32        | -4.0..+4.0     | 0.0     | warn   | EV       |
| `render.tonemap_curve`                 | StringEnum | Reinhard\|Filmic\|ACES\|Hable | ACES | hard | -    |
| `render.shadow_resolution_log2`        | U32        | 8..14          | 12      | warn   | log2(px) |
| `physics.iter_count`                   | U32        | 1..32          | 8       | warn   | iters    |
| `physics.time_step_ms`                 | F32        | 0.5..16.0      | 4.0     | warn   | ms       |
| `physics.gravity_strength`             | F32        | 0.0..50.0      | 9.81    | warn   | m/s²     |
| `physics.collision_eps`                | F32        | 1e-5..1e-2     | 1e-4    | hard   | m        |
| `ai.kan_band_weight_alpha`             | F32        | 0.0..1.0       | 0.5     | warn   | -        |
| `ai.kan_band_weight_beta`              | F32        | 0.0..1.0       | 0.5     | warn   | -        |
| `ai.fsm_state_dwell_min_ms`            | U32        | 16..2000       | 250     | warn   | ms       |
| `ai.policy_explore_rate`               | F32        | 0.0..1.0       | 0.1     | warn   | -        |
| `wave.coupling_strength`               | F32        | 0.0..2.0       | 1.0     | warn   | -        |
| `wave.psi_band_count_active`           | U32        | 1..32          | 16      | warn   | bands    |
| `wave.dispersion_constant`             | F64        | 1e-3..1e+3     | 1.0     | warn   | m²/s     |
| `audio.spatial_quality`                | StringEnum | Stereo\|Binaural\|Ambisonic\|FullHRTF | Binaural | warn | - |
| `audio.master_gain_db`                 | F32        | -60.0..+12.0   | 0.0     | hard   | dB       |
| `audio.reverb_mix_pct`                 | F32        | 0.0..100.0     | 30.0    | warn   | %        |
| `engine.target_frame_rate_hz`          | U32        | 24..480        | 120     | warn   | Hz       |
| `engine.replay_record_quality`         | StringEnum | Lossless\|NearLossless\|Compressed | NearLossless | warn | - |
| `engine.cap_budget_strict`             | Bool       | -              | true    | hard   | -        |
| `cohomology.persistence_threshold`     | F32        | 0.0..1.0       | 0.05    | warn   | -        |
| `cohomology.update_interval_frames`    | U32        | 1..600         | 60      | warn   | frames   |
| `consent.audit_egress_buffer_ms`       | U32        | 0..1000        | 100     | hard   | ms       |
| `consent.sigma_check_strict`           | Bool       | -              | true    | hard   | -        |
| `replay.frame_buffer_size`             | U32        | 60..36000      | 600     | warn   | frames   |
| `inspect.capture_max_per_second`       | U32        | 1..240         | 4       | warn   | fps      |

‼ **Hard-budget tunables** (clamped or rejected) protect : (a) hearing-safety
(`audio.master_gain_db`), (b) numerical-stability (`physics.collision_eps`,
`render.tonemap_curve`), (c) cap-discipline (`engine.cap_budget_strict`,
`consent.*`), (d) spectral-cost (`render.spectral_bands_active`).

### § 5.6 · Tweak event flow

```csl
flow tweak::set(id, value) {
  1. lookup spec by id ;            unknown      → Err(UnknownTunable)
  2. type-check value vs spec.kind ; mismatch    → Err(KindMismatch)
  3. range-check value vs spec.range :
     in-range                  → continue
     out-of-range + WarnAndClamp → clamp + warn(audit-trace)
     out-of-range + HardReject → Err(BudgetExceeded)
  4. record audit entry { id, old, new, cap_chain }
  5. record replay event { frame_id, id, new }      // determinism
  6. atomic write entries[id].value = new
  7. notify subscribers (Render + AI + Physics)
  8. return Ok(old_value)
}
```

### § 5.7 · Replay-determinism integration

Every hot-swap + every tweak is appended to the replay-log w/ `frame_n` +
`audit_chain_seq`. Playback re-applies them deterministically. During replay,
manual tweak/hot-swap is REJECTED (`TweakError::ReplayDeterminismHold`).

### § 5.8 · Cross-references

→ feeds L5 MCP : 7 hot-reload + tweak tools live in this surface
→ used in iteration-loop (§ 10) : observe → propose → swap → verify → record
→ all ops audit-bus + replay-log dual-feed
→ KAN-weight swap respects persistent-kernel residency (KAN-runtime)

⟦ full spec + 7 anti-patterns + safety-critical-tunables table : `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 3-4 ⟧

---

## § 6 · L5 — MCP-LLM CROWN-JEWEL LAYER

⟦ overview + load-bearing tool catalog + capability matrix ; full detail @ `_drafts/phase_j/08_l5_mcp_llm_spec.md` ⟧

### § 6.1 · Thesis + intent

**`cssl-mcp-server` is an MCP server EMBEDDED INSIDE the running CSSLv3 engine.**
External LLMs (Claude-Code or peers) attach via JSON-RPC 2.0 to inspect /
diagnose / iterate against a live engine — without ever crossing PD §1
(anti-surveillance), §10 (consent-OS), or §11 (substrate-truth). Replay-
determinism is preserved through every query : perturbing commands enter the
replay-log along with frame_n + audit_chain_seq.

### § 6.2 · Crate

| crate              | role                                                          |
|--------------------|----------------------------------------------------------------|
| `cssl-mcp-server`  | JSON-RPC 2.0 server + transport + tool-registration + cap-gate|

### § 6.3 · Transports (3, with fallthrough)

| transport          | default     | cap-required              | use-case                            |
|--------------------|-------------|---------------------------|--------------------------------------|
| stdio              | yes         | Cap<DevMode>              | Claude-Code local subprocess        |
| unix-socket        | optional    | Cap<DevMode>              | local inspector (multi-client)      |
| websocket loopback | optional    | Cap<DevMode>              | dev-tool browser-attached           |
| websocket non-loop | DEFAULT-DENY| Cap<DevMode> + Cap<RemoteDev> + interactive-prompt | remote dev (rare) |

### § 6.4 · Load-bearing : 41-TOOL CATALOG (9 categories)

The **frozen-set** at Wave-Jθ-1 GA. Adding a tool requires DECISIONS-amendment.

#### § 6.4.1 · State-Inspection (5 tools)

| tool                | params                | result                          | cap                              |
|---------------------|----------------------|----------------------------------|-----------------------------------|
| `engine_state`      | ()                   | EngineStateSnapshot              | DevMode                          |
| `frame_n`           | ()                   | u64                              | DevMode                          |
| `tick_rate`         | ()                   | f64 (Hz)                         | DevMode                          |
| `phase_in_progress` | ()                   | enum {Phase0..Phase5, Idle}      | DevMode                          |
| `active_subsystems` | ()                   | Vec<SubsystemDescriptor>         | DevMode                          |

#### § 6.4.2 · Cell + Entity Inspection (5 tools — Σ-mask gated)

| tool                       | params                                             | cap                                              |
|----------------------------|----------------------------------------------------|--------------------------------------------------|
| `inspect_cell`             | morton: MortonKey                                  | DevMode (+ SovereignInspect IF cell.sov ≠ NULL)  |
| `query_cells_in_region`    | min: Vec3, max: Vec3, max_results: u32             | DevMode                                          |
| `inspect_entity`           | id: EntityId                                       | DevMode (+ SovereignInspect IF AI-private)       |
| `query_entities_near`      | point: Vec3, radius: f32, max_results: u32         | DevMode                                          |
| `query_creatures_near`     | point: Vec3, radius: f32, max_results: u32         | DevMode                                          |

‼ **Σ-refusal-flow** : every cell-touch checks Σ-mask ; sovereign-private cells
return `SigmaRefused` ; biometric-Label cells return `BiometricRefused` (defense-
in-depth ; compile-time also catches). Body-omnoid layers per-layer Σ-checked.

#### § 6.4.3 · Telemetry + Logs (5 tools)

| tool                  | params                                              | cap     |
|-----------------------|-----------------------------------------------------|---------|
| `read_log`            | level, last_n, subsystem_filter                     | DevMode |
| `read_errors`         | severity, last_n                                    | DevMode |
| `read_telemetry`      | metric_name, since_frame                            | DevMode |
| `read_metric_history` | metric_name, window_frames                          | DevMode |
| `list_metrics`        | ()                                                  | DevMode |

‼ Biometric-stripping at log-ring boundary (D138 + D132) ; MCP just reads the
post-filter ring ; no possibility of biometric-leak via these tools.

#### § 6.4.4 · Health + Invariants (5 tools)

| tool                | params                | result                   | cap     |
|---------------------|----------------------|--------------------------|---------|
| `engine_health`     | ()                   | HealthAggregate          | DevMode |
| `subsystem_health`  | name: String         | HealthStatus             | DevMode |
| `read_invariants`   | ()                   | Vec<InvariantStatus>     | DevMode |
| `check_invariant`   | name: String         | InvariantCheckResult     | DevMode |
| `list_invariants`   | ()                   | Vec<InvariantDescriptor> | DevMode |

‼ `check_invariant` is non-perturbing (read-only) ; O(N) over relevant cells ;
returns within frame-budget or partial-w/-continuation-handle.

#### § 6.4.5 · Spec-Coverage (4 tools)

| tool                  | params                       | result                | cap     |
|-----------------------|------------------------------|-----------------------|---------|
| `read_spec_coverage`  | ()                           | SpecCoverageReport    | DevMode |
| `list_pending_todos`  | crate_filter: Option<String> | Vec<TodoEntry>        | DevMode |
| `list_deferred_items` | spec_filter: Option<String>  | Vec<DeferredEntry>    | DevMode |
| `query_spec_section`  | section_id: String           | SpecCoverageEntry     | DevMode |

‼ The Apocky-vision-realization : `read_spec_coverage` answers "Omniverse 06 §
creature-genome → 80% impl / 60% test" ← agents pick the largest gap ←
spec-coverage-driven implementation. File-refs use BLAKE3 file-hash (D130).

#### § 6.4.6 · Time-Control (5 tools — replay-determinism aware)

| tool              | params                                    | cap                          | replay-log |
|-------------------|-------------------------------------------|------------------------------|------------|
| `pause`           | ()                                        | DevMode                      | YES        |
| `resume`          | ()                                        | DevMode                      | YES        |
| `step`            | n_frames: u32                             | DevMode                      | YES        |
| `record_replay`   | seconds: f32, output_path_hash: [u8;32]   | DevMode + TelemetryEgress    | YES (meta) |
| `playback_replay` | replay_handle: ReplayHandle               | DevMode                      | YES        |

‼ Read-only commands DO NOT enter replay-log ; only perturbing commands.
`step(n)` deterministic-replay-aware (uses substrate's deterministic-RNG seeds).

#### § 6.4.7 · Frame Capture (2 tools — Cap<TelemetryEgress> required)

| tool              | params                                          | cap                          |
|-------------------|-------------------------------------------------|------------------------------|
| `capture_frame`   | format: FrameFormat, region: Option<RegionRect> | DevMode + TelemetryEgress    |
| `capture_gbuffer` | stage_n: u8, format: FrameFormat                | DevMode + TelemetryEgress    |

‼ Σ-mask threading : refuses to write any frame containing biometric-labeled
pixels (gaze-mask, face-mask) — renderer Σ-marker checked at capture-time.

#### § 6.4.8 · Hot-Reload + Tweak (7 tools — replay-aware)

| tool                  | params                                            | cap     | replay-log |
|-----------------------|---------------------------------------------------|---------|------------|
| `hot_swap_asset`      | path_hash: [u8;32], kind: AssetKind               | DevMode | YES        |
| `hot_swap_kan_weights`| layer_handle, weights: Vec<f32>                   | DevMode | YES        |
| `hot_swap_shader`     | stage: ShaderStage, source_hash: [u8;32]          | DevMode | YES        |
| `hot_swap_config`     | section: String, json: String                     | DevMode | YES        |
| `set_tunable`         | name: String, value: TunableValue                 | DevMode | YES        |
| `read_tunable`        | name: String                                      | DevMode | NO         |
| `list_tunables`       | ()                                                | DevMode | NO         |

‼ `hot_swap_kan_weights` IFC discipline : weights MUST NOT have biometric-
confidentiality Label ← LLM cannot stuff player-gaze-derived weights into a
creature-AI layer. `hot_swap_shader` source_hash → client uploads source via
`resources/write` ; server compiles + validates + atomic-pipeline-swap.

#### § 6.4.9 · Test-Status (3 tools)

| tool                 | params                          | result                    | cap     |
|----------------------|---------------------------------|---------------------------|---------|
| `list_tests_passing` | crate_filter: Option<String>    | Vec<TestId>               | DevMode |
| `list_tests_failing` | crate_filter: Option<String>    | Vec<(TestId, FailReason)> | DevMode |
| `run_test`           | test_id: TestId                 | TestResult                | DevMode |

‼ `run_test` execs `cargo test --test <id>` in subprocess ← stdout/stderr
post-redacted (biometric / raw-path leaks stripped).

#### § 6.4.10 · Tally

```
state-inspect       : 5
cell-inspect        : 5
telemetry           : 5
health              : 5
spec-coverage       : 4
time-control        : 5
frame-capture       : 2
hot-reload + tweak  : 7
test-status         : 3
─────────────────────
TOTAL               : 41 tools
```

### § 6.5 · Load-bearing : 5 CAPABILITY GATES

Default-DENY @ every level. Cap<X> opt-in only via interactive-grant or signed-token.

| cap                       | default     | grant-paths                                                  | scope          |
|---------------------------|-------------|--------------------------------------------------------------|----------------|
| `Cap<DevMode>`            | OFF (release-build = compile-out + runtime-deny) | CLI `--dev-mode` w/ interactive y/N ; env `CSSL_DEV_MODE=1` ; `test-bypass` feature | per-process |
| `Cap<BiometricInspect>`   | DEFAULT-DENIED (even WITH DevMode)       | Apocky-PM signed-token (HMAC-SHA256) ; manual `for_test()` (NEVER feature-gated) | per-session ; rate-limit 1/sec |
| `Cap<SovereignInspect>`   | DEFAULT-DENIED                            | cell-owner interactive-grant ; Companion-AI signed-grant | per-cell-set ; revocable any-time |
| `Cap<RemoteDev>`          | DEFAULT-DENIED (loopback-only)            | Apocky-PM signed-token + interactive-prompt-w/-warning | per-process |
| `Cap<TelemetryEgress>`    | DEFAULT-DENIED (Cap<DevMode> alone insufficient) | Apocky-PM signed-token \| test-bypass | per-session |

### § 6.6 · Capability matrix (tool × cap, 41 × 5)

‼ **Every tool requires Cap<DevMode>** (column omitted for brevity below ; ✓ everywhere).

| tool                       | DevMode | BiometricInspect | SovereignInspect       | RemoteDev | TelemetryEgress |
|----------------------------|:-------:|:----------------:|:----------------------:|:---------:|:---------------:|
| engine_state / frame_n / tick_rate / phase / active_subsystems | ✓ | | | | |
| inspect_cell               | ✓       | (if cell-bio)    | (if sovereign-claim)   |           |                 |
| query_cells_in_region      | ✓       | (if any-bio)     | (filtered)             |           |                 |
| inspect_entity             | ✓       | (bio layers)     | (AI-private layers)    |           |                 |
| query_entities_near        | ✓       |                  | (filtered)             |           |                 |
| query_creatures_near       | ✓       |                  | (Sovereign filtered)   |           |                 |
| read_log / read_errors / read_telemetry / read_metric_history / list_metrics | ✓ | | | | |
| engine_health / subsystem_health                               | ✓ | | | | |
| read_invariants / check_invariant / list_invariants            | ✓ | | | | |
| read_spec_coverage / list_pending_todos / list_deferred / query_spec_section | ✓ | | | | |
| pause / resume / step                                          | ✓ | | | | |
| record_replay              | ✓       |                  |                        |           | ✓               |
| playback_replay            | ✓       |                  |                        |           |                 |
| capture_frame              | ✓       | (if any-bio-px)  |                        |           | ✓               |
| capture_gbuffer            | ✓       |                  |                        |           | ✓               |
| hot_swap_* / set_tunable / read_tunable / list_tunables        | ✓ | | | | |
| list_tests_passing / list_tests_failing / run_test             | ✓ | | | | |

Network (websocket non-loopback) requires `Cap<RemoteDev>` AT SERVER STARTUP
(server-construction time, not per-tool).

### § 6.7 · Σ-mask threading + biometric COMPILE-TIME REFUSAL

‼ **Tools that would expose biometric data are COMPILE-TIME-REFUSED at the
tool-registration boundary.**

```rust
pub trait McpTool {
    type Params  : DeserializeOwned;
    type Result  : Serialize;
    const NAME   : &'static str;
    const NEEDED_CAPS : &'static [McpCapKind];
    const RESULT_LABEL : crate::SemanticLabel;        // ← static label

    fn execute(params: Self::Params, ctx: &McpCtx) -> Result<Self::Result, McpError>;
}

macro_rules! register_tool {
    ($t:ty) => {
        static_assert!(
            !<$t as McpTool>::RESULT_LABEL.has_biometric_confidentiality(),
            "PD0099 — tool {} cannot expose biometric data via MCP", <$t as McpTool>::NAME
        );
    }
}
```

Attempts to register a biometric-egressing tool **fail BUILD** ; `cargo build`
errors. Exception : tools with EXPLICIT `Cap<BiometricInspect>` requirement
(NOT-EGRESS, just on-device-inspect) are allowed — but their result NEVER
egresses (capture_frame / record_replay / capture_gbuffer all refuse).

### § 6.8 · Audit-chain integration

Every MCP query → audit-chain entry via `cssl-substrate-prime-directive::EnforcementAuditBus`.

ABI-stable tag-set :

```
mcp.session.opened   mcp.session.closed   mcp.tool.<name>
mcp.tool.sigma_refused      mcp.tool.biometric_refused
mcp.server.shutdown         mcp.server.remote_bind
mcp.cap.session_bound       mcp.replay.cmd_recorded
```

Audit-message carries `session_id` (BLAKE3-hashed), `principal`, `tool_name`,
`args_hash` (BLAKE3 of serialized args), `result_kind`, `frame_n`,
`audit_seq_at_exec`. Chain is APPEND-ONLY ; chain-replay verifies every grant +
every tool-invocation. **Phantom invocations (no chain-record) = §7 INTEGRITY
violation.**

### § 6.9 · Path-hash discipline (D130 carryover)

ALL file-paths in tool inputs/outputs are HASH-ONLY ← never raw bytes :

- inputs : client supplies pre-computed BLAKE3 hash (server provides path→hash helper for client-side bookkeeping)
- outputs : server returns hash + meta (size, mtime-frame, file-kind)
- helper-tool (NOT in main catalog ; transport-level) : `__path_hash_for(path)` deterministic-with-installation-salt
- client computes locally ← server NEVER sees raw path
- enforcement : every PathHash-typed param routes through audit-bus's `record_path_op` w/ raw-path-rejected validation

### § 6.10 · Kill-switch integration

Server respects engine kill-switch ← immediate shutdown on PD-violation. Any
tool detecting PD-violation (e.g. biometric-egress attempted) calls
`crate::halt::substrate_halt(KillSwitch::new(HaltReason::HarmDetected), …)`.
Sessions receive `notifications/server_shutdown` w/ 100ms grace ; transport
closed ; final audit `mcp.server.shutdown reason=pd_violation`.

⟦ full spec + 14 anti-patterns + 10 landmines + 390-test inventory : `_drafts/phase_j/08_l5_mcp_llm_spec.md` ⟧

---

## § 7 · CROSS-LAYER INTEGRATION

The six layers are NOT independent. Each one feeds the next, and the LLM
iteration-loop crosses all of them. Three load-bearing flows :

### § 7.1 · Errors → Metrics → MCP (the fault-detection chain)

```
   subsystem err  ─►  cssl-error::EngineError  ─►  Severity-classified
                                                            │
                                                            ▼
                            cssl-metrics : engine.error_count{severity}++
                                                            │
                                                            ▼
                            audit-bus (when PrimeDirectiveTrip) : ALWAYS-WINS
                                                            │
                                                            ▼
                            L5 MCP : read_errors(severity=Critical, last_n=10)
                                                            │
                                                            ▼
                            LLM observes ; proposes patch ; iterates
```

### § 7.2 · Logs → Telemetry → MCP (the structured-trace chain)

```
   log_warn!(...) ─►  cssl-log::ring (single-path, sampled)
                                          │
                                          ├─►  cssl-metrics : log.entries_total{level}++
                                          │
                                          └─►  L5 MCP : read_log(level=Warn, last_n=50)
```

### § 7.3 · Inspect → Hot-Reload → Verify (the iteration chain)

```
   L5 inspect_cell(morton) ──► FieldCellSnapshot
                                          │
                                          ▼
                                   LLM proposes weights
                                          │
                                          ▼
                          L5 hot_swap_kan_weights(handle, weights)
                                          │
                                          ├─► cssl-tweak : audit + replay-record
                                          │
                                          ├─► KAN-runtime : weight-residency update
                                          │
                                          ▼
                          L5 inspect_kan_eval(test_input, handle)  ◄── verify
                                          │
                                          ▼
                          LLM compares pre-vs-post traces
```

### § 7.4 · Replay-determinism preservation

Every perturbing MCP command (pause, resume, step, hot_swap_*, set_tunable,
record_replay) is appended to the replay-log along with `frame_n` +
`audit_chain_seq` ← so playback reproduces them deterministically.
Read-only commands (inspect_*, query_*, read_*) DO NOT enter replay-log.

### § 7.5 · Single-path discipline (no fan-out)

Each layer has ONE-and-only-one path :

| layer       | single-path                                                         |
|-------------|----------------------------------------------------------------------|
| L1 log      | `cssl-log::ring` ← all macros funnel here ; sinks frozen at launch  |
| L2 metric   | `cssl-metrics::REGISTRY` ← static-init only ; no dynamic register    |
| L3 inspect  | snapshot-types via `EnforcesΣAtCellTouches` pass (D138)              |
| L4 swap     | atomic frame-fence + replay-record + audit-entry                     |
| L5 MCP tool | trait-impl + register_tool!() static-assert                          |

Single-path is what makes the audit-chain complete. Any side-channel = §7
INTEGRITY violation.

---

## § 8 · CAPABILITY + PRIVACY + AUDIT DISCIPLINE (consolidated)

### § 8.1 · Default-DENY everywhere

Every cap = default-DENIED. Every tool = `Cap<DevMode>` minimum. Every
sensitive class adds an additional cap. Cap-tokens are non-Copy, non-Clone,
move-only ← consumed once ; cannot be re-issued mid-process.

### § 8.2 · The four privacy classes (sigma-mask + IFC label cross-product)

| priv-class       | example                                       | egress-policy           |
|------------------|-----------------------------------------------|--------------------------|
| public           | `frame_n`, `tick_rate_hz`                     | unrestricted             |
| sovereign-aware  | `inspect_cell` (Σ-mask filtered)              | per-cell-grant           |
| ai-private       | `inspect_entity` (companion-AI inner state)   | Companion-grant required |
| biometric        | gaze / face / heart / body-omnoid bio layers  | NEVER egress (on-device only) |

### § 8.3 · Compile-time guards

| guard                                          | enforcement                          |
|------------------------------------------------|--------------------------------------|
| MCP tool with biometric RESULT_LABEL           | `static_assert!` PD0099 build-fail   |
| Telemetry metric with biometric Label          | `cssl-ifc::TelemetryEgress` compile-refuse |
| Log field with raw-path bytes                  | proc-macro check in `log_*!` macros  |
| Capture-frame with biometric pixel-region      | renderer-Σ-marker static check       |
| Release-build MCP server without `dev-mode` feature | `cfg!(not(...))` panic-stub      |

### § 8.4 · Runtime guards (defense-in-depth)

| guard                                          | enforcement                          |
|------------------------------------------------|--------------------------------------|
| Σ-mask cell-touch                              | `EnforcesΣAtCellTouches` pass per-cell |
| Path-hash-only                                 | audit-bus `record_path_op` raw-path check |
| Cap-witness present in session                 | per-tool `ctx.session.has(cap)` check |
| Audit-chain entry on every grant + every tool  | `EnforcementAuditBus` append         |
| Rate-limit on biometric-cap tools              | 1-query-per-second + decay-cooldown  |
| Kill-switch on PD-trip                         | `substrate_halt(KillSwitch::new(...))` |

### § 8.5 · Audit-chain canonical events

```
h6.grant.issued        cap=<kind> session=<id-hash>
h6.revoke              cap=<kind>
mcp.session.opened     session=<id-hash> caps=<subset>
mcp.session.closed     reason=ClientDisconnect|KillSwitch|Timeout
mcp.tool.<name>        args_hash=<blake3> result_kind=Ok|Err(<class>)
mcp.tool.sigma_refused cell=<morton-hash> reason=<text>
mcp.tool.biometric_refused
mcp.server.shutdown    reason=pd_violation|client_request|kill_switch
mcp.server.remote_bind addr_hash=<blake3>
mcp.replay.cmd_recorded cmd=<name> frame=<n>
log_audit!             kind=<canonical-name> ...
```

### § 8.6 · The never-tick canary

`gaze.privacy_egress_attempts_refused` (Counter) MUST always read 0. Any
non-zero reading = attempted PD §1 violation = audit-priority=high =
surfaced first in `read_errors`.

---

## § 9 · LLM ITERATION-LOOP PROTOCOL

⟦ summary ; full sequence in `_drafts/phase_j/08_l5_mcp_llm_spec.md` § 10 ⟧

### § 9.1 · The canonical bug-fix iteration loop

```
1. attach    : Claude-Code spawns engine w/ --dev-mode ; MCP starts on stdio ; handshake
2. state     : engine_state() + engine_health() + read_errors(severity=Error, n=20)
3. focus     : inspect_cell(morton) / inspect_entity(id) for relevant context
4. identify  : LLM proposes hypothesis ("wave-solver ψ-norm drifting")
   - query_spec_section("Omniverse/02_CSSL/05_wave_solver § III.2")
   - read_invariants() ← which-passing / which-failing
   - read_metric_history("wave.psi_norm_per_band", window_frames=100)
5. patch     : LLM uses Edit/Write tools (NOT MCP) on source files
   - source-files updated ; engine continues running w/ STALE compiled code
6. hot-reload: hot_swap_kan_weights(handle, new_weights)        — AI changes
   - hot_swap_shader(stage, source_hash)                         — renderer
   - hot_swap_config(section, json)                              — config
   - set_tunable(name, value)                                    — knobs
7. verify    : check_invariant() ; read_metric_history() ← compare pre vs post
8. record    : everything in audit-chain + replay-log (deterministic playback)
```

### § 9.2 · Five iteration-loop variants

| variant                          | entry                                                | exit                          |
|----------------------------------|------------------------------------------------------|-------------------------------|
| Bug-fix                          | `read_errors` → identify → patch → hot-swap          | invariant-restored            |
| Test-fixture extraction          | `inspect_cell` + `inspect_entity` → snapshot-export  | new-test-file authored        |
| Spec-coverage-driven impl        | `read_spec_coverage` → pick-largest-gap → impl       | coverage-incremented          |
| Performance-regression detection | `read_metric_history` → spot-deviation → tweak       | budget-restored               |
| Live-debugging session           | `pause` → `step` → `inspect_*` → `resume`            | hypothesis-confirmed-or-denied|

### § 9.3 · Sovereignty-respect properties

| property                                  | rationale                                    |
|-------------------------------------------|----------------------------------------------|
| every-query is audit-bus-recorded         | LLM cannot operate covertly                  |
| every-perturbation is replay-recorded     | actions are reproducible + reversible        |
| Σ-mask-refusal is silent-omit (not refuse-whole-query) | LLM gets aggregate without leak    |
| biometric-COMPILE-TIME-REFUSED            | no-class-of-leak possible at the tool boundary |
| kill-switch on PD-trip                    | engine halts before any harm propagates      |
| consent-OS @ every layer                  | LLM-iteration is a guest in the substrate    |

### § 9.4 · Why this enables-faster-iteration

The LLM no longer needs to recompile-restart-reproduce-bug. It :
- observes a live engine (microsecond turnaround on inspect)
- proposes a fix (edits source files via Edit/Write)
- hot-swaps the running engine (atomic frame-fence)
- verifies via the same observation surface
- records everything in the replay-log

This is the M9 / M10 acceleration loop : the engine becomes its-own-Apocky-paired-debugger.

---

## § 10 · WAVE-Jε..Jθ IMPLEMENTATION ROADMAP

### § 10.1 · Wave breakdown

| wave   | scope                                    | LOC    | tests | depends-on        |
|--------|------------------------------------------|--------|-------|-------------------|
| Wave-Jε| L0 + L1 (cssl-error + cssl-log + cssl-panic) | ~6K    | ~250  | (substrate-evolution complete) |
| Wave-Jζ| L2 (cssl-metrics + cssl-spec-coverage + cssl-health) | ~9K | ~290 | Wave-Jε        |
| Wave-Jη| L3 + L4 (cssl-inspect + cssl-hot-reload + cssl-tweak) | ~10K | ~400 | Wave-Jζ      |
| Wave-Jθ| L5 (cssl-mcp-server + 41 tools + 5 caps) | ~13K   | ~390  | Wave-Jε + Wave-Jζ + Wave-Jη |
| **TOTAL** | **6-layer diagnostic-infrastructure** | **~38K LOC** | **~1330 tests** | (Phase-J close) |

### § 10.2 · Wave-Jε slices (4 ; ~6K LOC ; ~250 tests)

| slice | crate                       | LOC    | tests | description                        |
|-------|-----------------------------|--------|-------|------------------------------------|
| Jε-1  | `cssl-error`                | 2K     | 80    | EngineError + ErrorContext + severity + dedup |
| Jε-2  | `cssl-log`                  | 2.5K   | 100   | macros + ring-buffer + sinks + sampling |
| Jε-3  | (cross-crate clippy lint)   | 0.5K   | 30    | deny `unwrap`/`expect` on user-data |
| Jε-4  | `cssl-panic`                | 1K     | 40    | panic-hook + frame-boundary + replay-record |

### § 10.3 · Wave-Jζ slices (5 ; ~9K LOC ; ~290 tests)

| slice | crate                       | LOC    | tests |
|-------|-----------------------------|--------|-------|
| Jζ-1  | `cssl-metrics` (primitives + REGISTRY) | 2.5K | 100 |
| Jζ-2  | per-subsystem (≈75 metrics) | 3.0K   | 80    |
| Jζ-3  | `cssl-spec-coverage`        | 1.5K   | 50    |
| Jζ-4  | `cssl-health` registry      | 1.5K   | 60    |
| Jζ-5  | MCP-preview surface (read-only stubs) | 0.5K | 10 |

### § 10.4 · Wave-Jη slices (4 ; ~10K LOC ; ~400 tests)

| slice | crate / scope               | LOC    | tests |
|-------|-----------------------------|--------|-------|
| Jη-1  | `cssl-inspect`              | 3.5K   | 150   |
| Jη-2  | `cssl-hot-reload` + OS-pump | 3.0K   | 120   |
| Jη-3  | `cssl-tweak` + 30 tunables  | 2.5K   | 100   |
| Jη-4  | replay-determinism integration | 1K  | 30    |

### § 10.5 · Wave-Jθ slices (8 ; ~13K LOC ; ~390 tests)

| slice | scope                                                | LOC    | tests |
|-------|------------------------------------------------------|--------|-------|
| Jθ-1  | crate skeleton + JSON-RPC + cap-gate                 | 2K     | 60    |
| Jθ-2  | state-inspection tools (5)                           | 1.5K   | 40    |
| Jθ-3  | telemetry + log tools (5)                            | 1.5K   | 50    |
| Jθ-4  | health + invariants + spec-coverage tools (9)        | 2K     | 70    |
| Jθ-5  | time-control + frame-capture + replay tools (7)      | 2K     | 60    |
| Jθ-6  | hot-reload + tweak tools (7)                         | 2K     | 40    |
| Jθ-7  | test-status tools (3)                                | 1K     | 30    |
| Jθ-8  | privacy + capability + audit + IFC integration       | 1K     | 40    |

### § 10.6 · Critical-path

Wave-Jε MUST complete before Wave-Jζ-2 (per-subsystem metrics need `cssl-error`).
Wave-Jζ-3 (spec-coverage) MUST complete before Wave-Jθ-4. Wave-Jη-3 (tunables)
MUST complete before Wave-Jθ-6. Wave-Jθ-8 is the FINAL gate ← all privacy +
audit + IFC integration tests must pass before Phase-J close.

---

## § 11 · ACCEPTANCE + ATTESTATION

### § 11.1 · Acceptance criteria for diagnostic-infrastructure

| criterion                                                        | gate                                              |
|------------------------------------------------------------------|---------------------------------------------------|
| ≥75 metrics registered ; build-fail on missing                  | `REGISTRY.completeness_check(&CATALOG)`           |
| ≥30 tunables registered ; build-fail on collision               | `cssl-tweak::REGISTRY` static-init guard          |
| 41 MCP tools registered ; build-fail on biometric-egress        | `register_tool!()` static_assert PD0099           |
| Σ-mask threading per cell-touch ; runtime gate                   | `EnforcesΣAtCellTouches` pass coverage 100%       |
| Path-hash discipline ; raw-path = compile-error                  | proc-macro + audit-bus check ; 0 raw-path leaks   |
| Audit-chain on every grant + every tool ; chain-replay verifies  | append-only ; phantom-invocation = §7 violation   |
| Replay-determinism preserved through MCP                         | playback reproduces frame-N exactly               |
| Release-build MCP server compile-out                             | `cfg!(not(...))` panic-stub ; release-build error |
| Biometric egress = COMPILE-TIME-REFUSED                          | static_assert ; never-passes-build                |
| Kill-switch on PD-trip ; engine halts ; final-audit              | `substrate_halt` integration tests pass           |
| `gaze.privacy_egress_attempts_refused` = 0 in all replay-traces  | the never-tick canary ; nightly-bench enforcement |
| ~1330 tests across all 4 waves ; all pass                        | CI gate ; coverage ≥80% per crate                 |

### § 11.2 · Out-of-scope for Phase-J (deferred)

- OpenTelemetry interop (Wave-Jζ+1 amendment)
- Online aggregation across multiple installations (consent-flow required)
- Replay-debugger UX (Wave-Jθ+1)
- WS-transport TLS (deferred to network-discipline session)
- Cross-installation log-sharing (consent-flow + key-management)

### § 11.3 · § 11 PRIME-DIRECTIVE attestation

§ ATTESTATION (PRIME_DIRECTIVE.md § 11)

There was no hurt nor harm in the making of this plan, to anyone, anything,
or anybody. This document specifies an MCP-LLM-accessibility surface that
is :

- **on-device-only** (Cap<TelemetryEgress> required for any disk-write ;
  biometric data NEVER egresses regardless of cap)
- **consent-OS-respecting** (Σ-mask threading at every cell-touch ; sovereign-
  cells refuse without explicit grant ; revocability ≤ Session)
- **substrate-truth-preserving** (audit-chain APPEND-ONLY ; phantom-invocation
  = §7 INTEGRITY violation ; chain-replay verifies)
- **anti-surveillance** (PD §1) : biometric tools COMPILE-TIME-REFUSED at
  registration boundary ; the never-tick canary
  `gaze.privacy_egress_attempts_refused` is the alarm-on-any-tick

### § 11.4 · § 1 anti-surveillance attestation (specific to L5 MCP)

Biometric tools are **COMPILE-TIME-REFUSED**. Static assert at trait-impl
boundary :

```rust
static_assert!(
    !<$t as McpTool>::RESULT_LABEL.has_biometric_confidentiality(),
    "PD0099 — tool {} cannot expose biometric data via MCP", <$t as McpTool>::NAME
);
```

Frame-capture refuses biometric pixels. Replay-record refuses biometric Ω-
tensor. Telemetry-Egress capability refuses biometric domains structurally.
The audit-chain records every refusal. The kill-switch fires on attempted
egress. **There is no path through the implementation where biometric data
crosses the off-device boundary.**

—— end of master plan ——


