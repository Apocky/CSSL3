# 06 : L2 — TELEMETRY-COMPLETENESS LAYER (Wave-Jζ)

**File:** `_drafts/phase_j/06_l2_telemetry_spec.md`
**Status:** L2 layer-spec draft @ Phase-J diagnostic-infrastructure plan.
**Wave:** Wave-Jζ (implementation lane) ← Wave-Jβ (this design + foundations).
**Depends-on:** 00 plan-overview, 01 L0 budgets, 02 L0.5 invariants, 03 L1 dump-discipline, 04 L1.5 sample-pipelines, 05 L1.75 path-hash-discipline.
**Blocks:** 07 L2.5 SLO-graph, 08 L3 perfetto-export, 09 L3.5 MCP-bridge (Wave-Jθ), 10 L4 self-attesting-engine.
**Cite :** `specs/22_TELEMETRY.csl` (R18 baseline) ; `compiler-rs/crates/cssl-telemetry/` (foundation) ; `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md` (entity + phase budgets) ; `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl` (12-stage budget) ; PRIME_DIRECTIVE §1 §11 ; T11-D130 (path-hash-only) ; T11-D132 (biometric-compile-refuse) ; H5 (replay-determinism).

═════════════════════════════════════════════════════════════════
§ 0. THESIS — "what works + what doesn't but should"
═════════════════════════════════════════════════════════════════

§D. observability ≡ first-class-concern (R18 + CC9)
§D. ‼ L0 budgets exist ⊗ L1 dumps exist ⊗ ◐ per-subsystem-metrics ad-hoc
§D. R! L2 = structured-metrics + spec-coverage-tracker + health-registry
§D. principle : the engine itself knows
    (a) which Omniverse-spec-§ ⊗ has-impl
    (b) which Omniverse-spec-§ ⊗ has-tests
    (c) which Omniverse-spec-§ ⊗ spec'd-but-not-implemented = "the should-but-doesn't-work" list
    (d) which crate-metric ⊗ proves-the-spec-is-met
§D. consequence : "comprehensive metrics" + "complete spec coverage tracker" ≡ ONE-system
§D. consequence : ¬ external-dashboard-needed-to-find-gaps ⊗ R! engine-self-reports
§D. ‼ this-is-the-NORM-of "spec-validation-via-reimpl" ⊗ engine knows-what-it-implements + what-it-doesn't

§D. PRIME-DIRECTIVE binding :
    §1 N! surveillance ⇒ ∀ metric-tag ⊗ path-hash-only (T11-D130) ⊗ biometric-refused (T11-D132)
    §11 attestation ⇒ ∀ spec-coverage-claim ⊗ signed-by-build-pinhash
    H5 replay-determinism ⇒ ∀ deterministic-mode ⊗ metric-recording bit-deterministic ⊗ wallclock-free

═════════════════════════════════════════════════════════════════
§ I. L2 SCOPE — three pillars
═════════════════════════════════════════════════════════════════

§D. **PILLAR-1 : `cssl-metrics` crate** — typed Counter / Gauge / Histogram / Timer
    ⊗ effect-row-gated `{Telemetry<scope>}` (§§ specs/22)
    ⊗ rides the existing `TelemetryRing` (no new ring)
    ⊗ tag-discipline : SmallVec inline ; biometric-compile-refused

§D. **PILLAR-2 : per-subsystem metric-catalog** — the COMPLETE INVENTORY
    ⊗ engine.* + omega_step.* + render.* + physics.* + wave.* + spectral.*
       + xr.* + anim.* + audio.* + omega_field.* + kan.* + gaze.*
    ⊗ ≥ 60 metrics declared (per § VI table) ⊗ ¬ ad-hoc

§D. **PILLAR-3 : `cssl-spec-coverage` tracker** — "what works + what doesn't but should"
    ⊗ maps every Omniverse §-section + every CSSLv3 spec-§ → impl-status + test-status
    ⊗ queryable : `gap_list()` / `coverage_for(crate)` / `coverage_for(spec_§)`
    ⊗ source-of-truth = code-comments + DECISIONS-anchors + test-name-conventions

§D. **PILLAR-3.5 (rider) : health-registry** — ∀ crate exposes `health() -> HealthStatus`
§D. **PILLAR-3.6 (rider) : replay-determinism integration** — bit-deterministic in strict-mode

═════════════════════════════════════════════════════════════════
§ II. PILLAR-1 — `cssl-metrics` crate (NEW)
═════════════════════════════════════════════════════════════════

§D. crate-name : `cssl-metrics` (sibling-of `cssl-telemetry` ; ≠ replacement)
§D. relationship : `cssl-metrics` builds-on `cssl-telemetry`
    ⊗ Counter/Gauge/Histogram/Timer events ⊗ flow-into TelemetryRing
    ⊗ Slot.scope = TelemetryScope::Counters | Spans | Events ⊗ payload = encoded-metric
§D. dep-graph : `cssl-metrics` → `cssl-telemetry` → `cssl-rt` (no-cycle)

—————————————————————————————————————————————
§ II.1 Counter
—————————————————————————————————————————————

```rust
pub struct Counter {
    pub name: &'static str,           // e.g. "engine.frame_n"
    pub value: AtomicU64,
    pub tags: SmallVec<[(TagKey, TagVal); 4]>,
    pub sampling: SamplingDiscipline, // 1-of-N rate ; default = 1-of-1
    pub schema_id: TelemetrySchemaId,
}

impl Counter {
    pub fn inc(&self) -> Result<(), MetricError> / { Telemetry<Counters> };
    pub fn inc_by(&self, n: u64) -> Result<(), MetricError> / { Telemetry<Counters> };
    pub fn set(&self, v: u64) -> Result<(), MetricError> / { Telemetry<Counters> };
    pub fn snapshot(&self) -> u64;    // /{Pure} — read-only
}
```

§D. semantics :
    monotonic-non-decreasing under `inc` / `inc_by`
    `set` permitted ⊗ tagged-as-RESET-EVENT ⊗ audit-chain logs the reset
§D. overflow : u64 saturating ; overflow-event ⊗ Audit<"counter-overflow"> emitted
§D. tag-discipline : at-construction-time ; runtime-tag-mutation FORBIDDEN
§D. compile-time-refusal :
    biometric-tag-key (BiometricKind enumeration) ⊗ refused-via cssl-ifc::TelemetryEgress
    raw-path tag-value ⊗ refused-via path_hash-discipline (T11-D130)

—————————————————————————————————————————————
§ II.2 Gauge
—————————————————————————————————————————————

```rust
pub struct Gauge {
    pub name: &'static str,
    pub value: AtomicU64,             // bit-pattern-of-f64 (transmute-via-from_bits)
    pub tags: SmallVec<[(TagKey, TagVal); 4]>,
    pub sampling: SamplingDiscipline,
    pub schema_id: TelemetrySchemaId,
}

impl Gauge {
    pub fn set(&self, v: f64) -> Result<(), MetricError> / { Telemetry<Counters> };
    pub fn inc(&self, delta: f64) -> Result<(), MetricError> / { Telemetry<Counters> };
    pub fn dec(&self, delta: f64) -> Result<(), MetricError> / { Telemetry<Counters> };
    pub fn snapshot(&self) -> f64;
}
```

§D. semantics : non-monotonic ; current-value gauge (e.g. tick_rate_hz)
§D. NaN handling : `set(NaN)` ⊗ refused @ MetricError::NaN ⊗ N! silent-write
§D. Infinity handling : `set(±Inf)` ⊗ refused-or-clamped @ schema-policy
§D. determinism-mode (H5) : f64 ⊗ store-bit-pattern via `f64::to_bits` ⊗ no-precision-drift

—————————————————————————————————————————————
§ II.3 Histogram
—————————————————————————————————————————————

```rust
pub struct Histogram {
    pub name: &'static str,
    pub bucket_boundaries: &'static [f64],   // exclusive-upper-bounds, monotonic
    pub counts: Vec<AtomicU64>,              // len = bucket_boundaries.len() + 1
    pub sum: AtomicU64,                      // bit-pattern-f64
    pub count: AtomicU64,
    pub tags: SmallVec<[(TagKey, TagVal); 4]>,
    pub sampling: SamplingDiscipline,
    pub schema_id: TelemetrySchemaId,
}

impl Histogram {
    pub fn record(&self, v: f64) -> Result<(), MetricError> / { Telemetry<Counters> };
    pub fn snapshot(&self) -> HistogramSnapshot;
    pub fn percentile(&self, p: f64) -> f64;   // linear-interpolation in-bucket
}
```

§D. determinism-discipline :
    bucket-boundaries ⊗ `&'static [f64]` ⊗ COMPILE-TIME-CONSTANT
    ⊗ replay-strict mode requires-deterministic-buckets
    ⊗ N! data-driven boundary inference ⊗ N! adaptive histograms (in strict-mode)
§D. canonical-bucket sets :
    LATENCY_NS_BUCKETS = [10, 100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000, 100_000_000]
    BYTES_BUCKETS      = [64, 256, 1024, 4096, 16384, 65536, 262144, 1048576]
    COUNT_BUCKETS      = [1, 4, 16, 64, 256, 1024, 4096]
    PIXEL_BUCKETS      = [1, 4, 16, 64, 256, 1024, 4096, 16384]
§D. percentile-discipline : linear-interpolation within-bucket ; cite-bucket-cardinality
§D. N! Welford's online-quantile (would-violate-determinism in strict-mode)

—————————————————————————————————————————————
§ II.4 Timer + RAII
—————————————————————————————————————————————

```rust
pub struct Timer {
    pub name: &'static str,
    pub ns_total: AtomicU64,
    pub count: AtomicU64,
    pub last_ns: AtomicU64,
    pub p50: AtomicU64,                  // running-quantile estimator
    pub p99: AtomicU64,
    pub tags: SmallVec<[(TagKey, TagVal); 4]>,
    pub sampling: SamplingDiscipline,
    pub schema_id: TelemetrySchemaId,
}

#[must_use = "TimerHandle drop = record ; bind to a name or call .commit()"]
pub struct TimerHandle<'t> {
    timer: &'t Timer,
    started_at: u64,                     // monotonic-ns @ construction
}

impl Timer {
    pub fn start(&self) -> TimerHandle<'_> / { Telemetry<Counters> };
    pub fn record_ns(&self, ns: u64);    // advanced-callers-only
}

impl<'t> Drop for TimerHandle<'t> {
    fn drop(&mut self) {
        let now = monotonic_ns();
        let dt = now - self.started_at;
        self.timer.record_ns(dt);
    }
}
```

§D. RAII-discipline : `let _t = SOME_TIMER.start();` ⊗ scope-exit records ; #[must_use] forbids drop-without-let
§D. determinism-mode : `monotonic_ns()` ⊗ replaced-with `frame_n × frame_ns + sub_phase_offset`
    ⊗ all-recordings deterministic-functions-of (frame_n, sub_phase) ⊗ ¬ wallclock
§D. percentile-storage : t-digest-deterministic OR fixed-bucket-histogram-derived (replay-strict)

—————————————————————————————————————————————
§ II.5 Sampling-discipline
—————————————————————————————————————————————

```rust
pub enum SamplingDiscipline {
    Always,                            // default = 1-of-1
    OneIn(u32),                        // 1-of-N decimation ; deterministic via frame_n % N
    BurstThenDecimate { burst: u32, then_one_in: u32 },
    Adaptive { target_overhead_pct: f32 },  // ¬ replay-strict
}
```

§D. ‼ Adaptive-mode FORBIDDEN under replay-strict (H5)
§D. R! deterministic-decimation : `should_sample(frame_n)` = `(frame_n + tag_hash) % N == 0`
§D. per-subsystem-overhead-cap : 0.5% per-frame-budget (cite specs/22 § OVERHEAD-BUDGET)
§D. ‼ violation ⊗ self-detected ⊗ Audit<"telemetry-overhead-violation"> ⊗ auto-decimate

—————————————————————————————————————————————
§ II.6 Effect-row gating
—————————————————————————————————————————————

§D. ∀ Counter::inc / Gauge::set / Histogram::record / Timer::start
       requires `{Telemetry<Counters>}` effect-row OR superset
§D. caller-without-effect ⊗ COMPILE-TIME-REFUSED via cssl-effects::check_telemetry_no_raw_path
       (extended @ Wave-Jζ-1 to also-check : no-biometric-tag-keys, no-raw-path-tag-vals)
§D. consequence : `record_metric` cannot-be-called from-pure-fn ⊗ effect-leak-impossible
§D. inheritance : callee's `Telemetry<S>` ⊑ caller's ⊗ widening-error (per specs/22 § SCOPE)

—————————————————————————————————————————————
§ II.7 Schema + registration
—————————————————————————————————————————————

```rust
#[derive(TelemetrySchema)]
pub struct EngineFrameN(Counter);

#[ctor]
fn register_engine_frame_n() {
    METRIC_REGISTRY.register(EngineFrameN::SCHEMA);
}
```

§D. compile-time : `#[derive(TelemetrySchema)]` emits-schema-id-constant ; consumed-by exporter
§D. registration : `#[ctor]`-bound startup ; idempotent ; collision-detection
§D. self-describing : exporter (OTLP/Perfetto) reads-schema-registry @ start-of-export

═════════════════════════════════════════════════════════════════
§ III. PILLAR-2 — Per-subsystem metric catalog (THE INVENTORY)
═════════════════════════════════════════════════════════════════

§D. ‼ this-is-the-AUTHORITATIVE list ⊗ N! ad-hoc-additions
§D. additions @ Wave-Jζ-2 ⊗ R! amend-this-§-via DECISIONS-anchor
§D. format : `subsystem.metric_name` (Type ; tags ; cite-budget-source)

—————————————————————————————————————————————
§ III.1 `engine.*` — top-level frame-loop
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget-source |
|---|---|---|---|---|
| `engine.frame_n` | Counter | (mode ∈ {60,90,120,xr}) | DENSITY_BUDGET §V | per-frame |
| `engine.frame_time_ns` | Timer | (mode) | DENSITY_BUDGET §V (16ms / 8.33ms / 11.1ms) | per-frame p99 |
| `engine.tick_rate_hz` | Gauge | () | DENSITY_BUDGET §V/§VI/§VII | rolling-30-frame |
| `engine.dropped_frames` | Counter | (reason ∈ {deadline,thermal,vram}) | DENSITY_BUDGET §V.7 | per-frame |
| `engine.health_state` | Gauge | () | (this-spec § VI) | 0=Failed/1=Degraded/2=Ok |
| `engine.mode_switches` | Counter | (from→to) | DENSITY_BUDGET §VI hysteresis | per-event |
| `engine.cmd_buf_count` | Gauge | (queue ∈ {graphics,async-compute}) | RENDERING_PIPELINE §I | per-frame |

—————————————————————————————————————————————
§ III.2 `omega_step.*` — 6-phase compute-graph
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `omega_step.phase_time_ns` | Timer | (phase ∈ {COLLAPSE,PROPAGATE,COMPOSE,COHOMOLOGY,AGENCY,ENTROPY}) | DENSITY_BUDGET §V | per-phase |
| `omega_step.tick_rate_hz` | Gauge | () | DENSITY_BUDGET §V | rolling-avg |
| `omega_step.replay_determinism_check` | Counter | (kind ∈ {pass,fail}) | H5 contract | failures = 0 |
| `omega_step.deferred_to_next_frame` | Counter | (phase) | DENSITY_BUDGET §V.7 | per-frame |
| `omega_step.collapsed_regions_count` | Gauge | () | DENSITY_BUDGET §IV | per-tick |
| `omega_step.cohomology_classes_active` | Gauge | (kind ∈ {birth,persist,transform,die}) | DENSITY_BUDGET §V.4 | per-tick |
| `omega_step.agency_violations` | Counter | (kind ∈ {consent,sov,reversibility,launder}) | DENSITY_BUDGET §V.5 | per-frame |
| `omega_step.entropy_drift_sigma` | Gauge | () | DENSITY_BUDGET §V.6 | σ-balance |
| `omega_step.observation_cap_overflow` | Counter | () | DENSITY_BUDGET §V.1 (≤64/frame) | per-frame |
| `omega_step.rg_tick_skipped` | Counter | (scale ∈ {s_2,s_3,s_4,s_5,s_6,s_7}) | DENSITY_BUDGET §VIII | correctness-issue |

—————————————————————————————————————————————
§ III.3 `render.*` — `cssl-render-v2` (12-stage pipeline)
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `render.stage_time_ns` | Timer | (stage ∈ 1..=12, mode) | RENDERING_PIPELINE §III/§V | per-stage budget table |
| `render.gpu_memory_bytes` | Gauge | (pool ∈ {field,RC,KAN,RT,scratch}) | DENSITY_BUDGET §III table | ≤ 1GB Ω-field |
| `render.draw_calls` | Counter | (stage) | RENDERING_PIPELINE §IX | per-frame |
| `render.cull_rate` | Gauge | (stage) | RENDERING_PIPELINE §V.5 SDF-Morton | ratio ∈ [0,1] |
| `render.foveation_savings_pct` | Gauge | (eye ∈ {L,R}) | RENDERING_PIPELINE §VII (DFR 86% reduction) | percentage |
| `render.sdf_marches_per_pixel` | Histogram | (stage=5, fovea-tier) | RENDERING_PIPELINE §III stage-5 ; PIXEL_BUCKETS | distribution |
| `render.kan_eval_per_pixel` | Histogram | (stage ∈ {6,7}) | RENDERING_PIPELINE §III stage-6/7 ; PIXEL_BUCKETS | distribution |
| `render.recursion_depth_witnessed` | Histogram | (stage=9) | RENDERING_PIPELINE §III stage-9 (≤ RecursionDepthMax) | bounded |
| `render.appsw_disable_events` | Counter | (reason ∈ {throat,spell,thermal}) | DENSITY_BUDGET §VII / RENDERING_PIPELINE §IX | per-event |
| `render.cmd_buf_per_eye_pair` | Counter | () | RENDERING_PIPELINE §I (ONE-cmd-buf invariant) | =1 per-pair |
| `render.shader_invocations` | Counter | (stage) | specs/22 § SHADER_INVOCATIONS | pipeline-statistics |

§D. ‼ stage-tag ⊗ MUST cover-all-12 ⊗ missing-stage = telemetry-completeness-violation

—————————————————————————————————————————————
§ III.4 `physics.*` — `cssl-physics-wave`
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `physics.entity_count` | Gauge | (tier ∈ {T0,T1,T2,T3}) | DENSITY_BUDGET §IV | ≤ 1M total |
| `physics.broadphase_time_ns` | Timer | () | DENSITY_BUDGET §IV(c) SDF-Morton | folded-in PROPAGATE |
| `physics.constraint_iters` | Histogram | () ; COUNT_BUCKETS | physics-spec | distribution |
| `physics.spill_rate` | Gauge | () | DENSITY_BUDGET §IV ratio | ≤ 1% target |
| `physics.morton_collisions` | Counter | () | DENSITY_BUDGET §IV(c) | per-frame |
| `physics.sdf_collision_calls` | Counter | () | DENSITY_BUDGET §IV(c) | per-frame |
| `physics.tier_demotions` | Counter | (from→to) | DENSITY_BUDGET §I tier-cascade | per-frame |
| `physics.tier_promotions` | Counter | (from→to) | DENSITY_BUDGET §I tier-cascade | per-frame |

—————————————————————————————————————————————
§ III.5 `wave.*` — `cssl-wave-solver` (LBM ψ multi-band)
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `wave.psi_norm_per_band` | Gauge | (band ∈ {AUDIO,LIGHT_R,LIGHT_G,LIGHT_B,LIGHT_NIR}) | RENDERING_PIPELINE §III stage-4 ; 04_WAVE_UNITY | normalized |
| `wave.cross_band_coupling_energy` | Gauge | (src_band, dst_band) | 04_WAVE_UNITY § COUPLING | per-frame |
| `wave.boundary_violation_count` | Counter | (boundary ∈ {SDF,domain}) | wave-solver §V | conservation |
| `wave.imex_substep_count` | Histogram | () ; COUNT_BUCKETS | DENSITY_BUDGET §VI 4→2 substeps | distribution |
| `wave.lbm_collide_time_ns` | Timer | (band) | DENSITY_BUDGET §V.2 (1.5ms) | per-frame |
| `wave.lbm_stream_time_ns` | Timer | (band) | DENSITY_BUDGET §V.2 | per-frame |
| `wave.dispersion_active_count` | Counter | () | RENDERING_PIPELINE §III stage-6 (M-coord 12) | per-frame |

—————————————————————————————————————————————
§ III.6 `spectral.*` — `cssl-spectral-render` (KAN-BRDF + tonemap)
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `spectral.kan_eval_count` | Counter | (kind ∈ {brdf,detail,collapse,oracle}) | RENDERING_PIPELINE §III stage-6 | per-frame |
| `spectral.iridescence_active_count` | Counter | () | RENDERING_PIPELINE §III stage-6 (M-coord 15) | per-frame |
| `spectral.tonemap_time_ns` | Timer | () | RENDERING_PIPELINE §III stage-10 (0.3ms) | per-frame |
| `spectral.bands_active` | Gauge | () | RENDERING_PIPELINE §III stage-6 (16-band) | =16 typical |
| `spectral.kan_backend` | Gauge | () | KAN-runtime spec | 0=Scalar/1=SIMD/2=CoopMatrix |
| `spectral.fluorescence_active_count` | Counter | () | RENDERING_PIPELINE §III stage-6 (M-coord 13) | per-frame |
| `spectral.spectrum_to_tristim_calls` | Counter | () | RENDERING_PIPELINE §III stage-10 | per-eye-per-frame |

—————————————————————————————————————————————
§ III.7 `xr.*` — `cssl-host-openxr`
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `xr.frame_time_per_eye_ns` | Timer | (eye ∈ {L,R}) | DENSITY_BUDGET §VII (≤ 11.11ms) | per-eye |
| `xr.tracker_confidence` | Gauge | (tracker ∈ {head,L_hand,R_hand,L_eye,R_eye,body,face}) | 02_VR_EMBODIMENT | normalized |
| `xr.runtime_state` | Gauge | () | OpenXR session-state | enum-encoded |
| `xr.appsw_active` | Gauge | () | RENDERING_PIPELINE §IX | 0/1 |
| `xr.foveation_active` | Gauge | () | RENDERING_PIPELINE §VII DFR | 0/1 |
| `xr.thermal_throttle_active` | Gauge | () | DENSITY_BUDGET §VII (≤ 65% sustained) | 0/1 |
| `xr.boundary_breach_events` | Counter | () | RENDERING_PIPELINE §III stage-12 | per-event |
| `xr.swap_chain_misses` | Counter | () | RENDERING_PIPELINE §III stage-12 | per-frame |

§D. ‼ tracker-confidence-tag ⊗ tracker-name-only ⊗ N! biometric-data ⊗ T11-D132-refused
§D. eye-tracker confidence is-a-quality-metric ⊗ N! the gaze-direction itself (which-stays-on-device)

—————————————————————————————————————————————
§ III.8 `anim.*` — `cssl-anim-procedural`
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `anim.kan_eval_per_creature` | Histogram | () ; COUNT_BUCKETS | anim-spec | distribution |
| `anim.ik_iter_count` | Histogram | () ; COUNT_BUCKETS | anim-spec | distribution |
| `anim.bone_count_per_creature` | Histogram | () ; COUNT_BUCKETS | anim-spec | distribution |
| `anim.creatures_active` | Gauge | (tier ∈ {T0,T1,T2,T3}) | DENSITY_BUDGET §IV entity-tier | per-frame |
| `anim.solve_time_ns` | Timer | () | anim-spec | folded-in PROPAGATE |

—————————————————————————————————————————————
§ III.9 `audio.*` — `cssl-wave-audio`
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `audio.psi_norm_per_band` | Gauge | (band=AUDIO) | 04_FIELD_AUDIO | normalized |
| `audio.binaural_compute_time_ns` | Timer | () | RENDERING_PIPELINE §III stage-4 audio-impulse | ≤ 1ms |
| `audio.lbm_stream_collide_time_ns` | Timer | () | RENDERING_PIPELINE §III stage-4 D3Q19 | per-frame |
| `audio.creature_vocalization_count` | Counter | (creature_kind_hash) | 04_FIELD_AUDIO | per-event |
| `audio.frames_submitted` | Counter | (stream) | specs/22 § AUDIO-OPS | per-callback |
| `audio.frames_dropped` | Counter | (stream) | specs/22 § AUDIO-OPS | per-callback |
| `audio.underrun_count` | Counter | (stream) | specs/22 § AUDIO-OPS | per-event |
| `audio.sample_rate` | Gauge | (stream) | specs/22 § AUDIO-OPS | constant |
| `audio.channel_count` | Gauge | (stream) | specs/22 § AUDIO-OPS | constant |

§D. ‼ creature_kind_hash ⊗ category-anonymized ⊗ N! per-creature-fingerprint
§D. ‼ audio-stream-tag ⊗ stream-handle-id-only ⊗ N! source-content-hash

—————————————————————————————————————————————
§ III.10 `omega_field.*` — `cssl-substrate-omega-field`
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `omega_field.cell_count_active` | Gauge | (tier ∈ {T0,T1,T2,T3}) | DENSITY_BUDGET §II | sparse-active |
| `omega_field.morton_collisions` | Counter | () | DENSITY_BUDGET §IV(c) | per-frame |
| `omega_field.tier_distribution` | Histogram | (tier) ; COUNT_BUCKETS | DENSITY_BUDGET §I cascade | distribution |
| `omega_field.sigma_mask_mutation_rate` | Counter | () | Σ-overlay spec | per-frame |
| `omega_field.hash_load_factor` | Gauge | (tier) | DENSITY_BUDGET §III load-factor 0.5 | ratio |
| `omega_field.rehash_events` | Counter | (tier) | DENSITY_BUDGET §XI.B EDGE-6 | per-event |
| `omega_field.bytes_per_tier` | Gauge | (tier) | DENSITY_BUDGET §III table | ≤ 245MB total |
| `omega_field.mera_pyramid_bytes` | Gauge | (layer ∈ 0..=3) | DENSITY_BUDGET §III (35MB overhead) | per-layer |
| `omega_field.facet_writes` | Counter | (facet ∈ {S,M,P,Λ,Ψ,Σ,Φ}) | 00_FACETS spec | per-frame |

§D. ‼ Σ-mask-mutation-rate ⊗ aggregate-counter-only ⊗ N! per-cell ⊗ N! per-Sovereign

—————————————————————————————————————————————
§ III.11 `kan.*` — `cssl-substrate-kan`
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `kan.eval_count` | Counter | (kind ∈ {oracle,cognition,spell,material,brdf,detail,collapse}) | DENSITY_BUDGET §III | per-frame |
| `kan.persistent_kernel_residency_pct` | Gauge | () | KAN-runtime § persistent-kernel | percentage |
| `kan.quantization_mode` | Gauge | () | KAN-runtime § quant | 0=FP32/1=FP16/2=INT8 |
| `kan.pattern_pool_size` | Gauge | () | KAN-runtime § pattern-pool | count |
| `kan.weight_bytes` | Gauge | (kind) | DENSITY_BUDGET §III (50MB total) | per-kind |
| `kan.distillation_cycles` | Counter | (kind) | KAN-runtime § distillation | per-cycle |
| `kan.eval_time_ns` | Timer | (kind) | DENSITY_BUDGET §IV(b) (≤10ns SIMD) | distribution |

—————————————————————————————————————————————
§ III.12 `gaze.*` — `cssl-gaze-collapse` (PRIME-DIRECTIVE careful)
—————————————————————————————————————————————

| metric | type | tags | spec-cite | budget |
|---|---|---|---|---|
| `gaze.saccade_predict_latency_ns` | Timer | () | RENDERING_PIPELINE §III stage-2 | ≤ 200ms (DENSITY §I) |
| `gaze.confidence_avg` | Gauge | () | RENDERING_PIPELINE §III stage-2 | aggregate-only |
| `gaze.fovea_full_pixels` | Gauge | () | RENDERING_PIPELINE §VII (5° area = 5%) | percentage-of-eye |
| `gaze.privacy_egress_attempts_refused` | Counter | () | T11-D132 § BiometricRefused | should = 0 |
| `gaze.opt_out_active` | Gauge | () | RENDERING_PIPELINE §III stage-2 consent | 0/1 |
| `gaze.collapse_bias_application_count` | Counter | () | RENDERING_PIPELINE §III stage-2/3 coupling | per-frame |

§D. ‼ ‼ ‼ THIS-SUBSYSTEM-IS-§1-CRITICAL :
    ¬ gaze-direction logged
    ¬ blink-pattern logged
    ¬ eye-openness-distribution logged
    ✓ aggregate-confidence-and-latency only
    ✓ refused-egress-attempt counter (the canary that NEVER ticks)
§D. ‼ `gaze.privacy_egress_attempts_refused` ⊗ alarm-on-non-zero ⊗ Audit<"prime-directive-leak-attempt">

—————————————————————————————————————————————
§ III.13 catalog-completeness rule
—————————————————————————————————————————————

§D. **EVERY** subsystem-crate exposes ≥ {frame_count, last_op_time_ns, health_state, error_count}
§D. **EVERY** subsystem-crate registers-its-metrics in cssl-metrics::REGISTRY @ static-init
§D. **MISSING** registration ⊗ cssl-spec-coverage flags as gap @ build-time
§D. **THIS-§** = canonical inventory ; supplements via DECISIONS-anchor only

§D. completeness-budget-check at-build :
    ‼ `cssl-metrics::REGISTRY.completeness_check(&CATALOG)` ⊗ build-fail if-< 100%
    ‼ `CATALOG = include_str!("phase_j/06_l2_telemetry_spec.md")` parsed-into-static-table
    ⊗ self-references-this-spec ⊗ "the-engine-is-its-own-spec-coverage-witness"

═════════════════════════════════════════════════════════════════
§ IV. PILLAR-3 — `cssl-spec-coverage` tracker
═════════════════════════════════════════════════════════════════

§D. ‼ this-is-the-component "the-engine-knows-what-works-and-what-doesn't-but-should"
§D. status-of-every-spec-§ ⊗ kept-as-typed-enum ⊗ queryable-at-runtime

—————————————————————————————————————————————
§ IV.1 implementation-status enum
—————————————————————————————————————————————

```rust
pub enum ImplStatus {
    /// Implemented: production-grade ; meets spec ; reviewed.
    Implemented {
        crate_path: &'static str,         // e.g. "compiler-rs/crates/cssl-render-v2"
        primary_module: &'static str,     // e.g. "crate::pipeline::stage_5"
        confidence: ImplConfidence,       // High / Medium / Low
        impl_date: &'static str,          // ISO-8601 when-marked-impl
    },
    /// Partial: shape-correct ; behavior-incomplete.
    Partial {
        crate_path: &'static str,
        gaps: &'static [&'static str],    // human-readable gap-list
    },
    /// Stub: type exists, body = todo!() / unimplemented!() / placeholder return.
    Stub {
        crate_path: &'static str,
    },
    /// Missing: no impl-reference exists.
    Missing,
}
```

§D. confidence ⊗ Low: untested-or-recently-changed ; Medium: bench-validated ; High: M7-floor-passed
§D. partial-gaps ⊗ R! cite-failing-acceptance-criterion-text

—————————————————————————————————————————————
§ IV.2 test-status enum
—————————————————————————————————————————————

```rust
pub enum TestStatus {
    /// Tested: ≥ 1 test cites this spec-anchor + passes.
    Tested {
        test_paths: &'static [&'static str],     // module::test_name
        last_pass_date: &'static str,
    },
    /// Partial: some tests exist but coverage incomplete.
    Partial {
        test_paths: &'static [&'static str],
        uncovered_criteria: &'static [&'static str],
    },
    /// Untested: spec exists, no tests cite it.
    Untested,
    /// NoTests: spec explicitly N/A for testing (e.g. attestation-only).
    NoTests {
        rationale: &'static str,
    },
}
```

—————————————————————————————————————————————
§ IV.3 SpecAnchor — atomic-unit
—————————————————————————————————————————————

```rust
pub struct SpecAnchor {
    pub spec_root: SpecRoot,                    // Omniverse | CSSLv3
    pub spec_file: &'static str,                // e.g. "04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md"
    pub section: &'static str,                  // e.g. "§ V"
    pub criterion: Option<&'static str>,        // optional acceptance-line, e.g. "phase-COLLAPSE ≤ 4ms"
    pub impl_status: ImplStatus,
    pub test_status: TestStatus,
    pub citing_metrics: &'static [&'static str],// metric-names that VALIDATE this anchor
}

pub enum SpecRoot {
    Omniverse,                                  // ../Omniverse/
    CssLv3,                                     // specs/
    DecisionsLog,                               // DECISIONS.md anchors
}
```

§D. ‼ `citing_metrics` ⊗ ties Pillar-2 catalog to-Pillar-3 coverage
§D. consequence : "spec-§-V is-tested" + "metric `omega_step.phase_time_ns` is-recorded"
       ⊗ co-discoverable
§D. consequence : `omega_step.phase_time_ns` deleted ⊗ § V test-status auto-degraded to Untested

—————————————————————————————————————————————
§ IV.4 source-of-truth — extraction-discipline
—————————————————————————————————————————————

§D. **PRIMARY** : code-comment markers
    ```rust
    // § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V phase-COLLAPSE
    fn collapse_phase(...) { ... }
    ```
    extraction : `cssl-spec-coverage-build` proc-macro scans @ build
§D. **SECONDARY** : DECISIONS.md per-slice spec-anchors
    ```markdown
    ## T11-D113 § Ω-field cell + sparse Morton-grid
    spec-anchors :
      - Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET §III VRAM-budget-table
      - Omniverse/04_OMEGA_FIELD/02_STORAGE §sparse-Morton-grid
    ```
    extraction : DECISIONS.md parsed-by build-script ; anchors → SpecAnchor entries
§D. **TERTIARY** : test-name conventions
    ```rust
    #[test]
    fn omega_field_cell_72b_layout_per_spec_06_substrate_evolution() { ... }
    ```
    parser : `[crate]_[fn]_per_spec_[file_anchor]` regex ; test-name → TestStatus.test_paths
§D. consistency-check : ∀ SpecAnchor ⊗ ≥ one source-of-truth-citation ⊗ else build-warn

—————————————————————————————————————————————
§ IV.5 Coverage queries (the runtime API)
—————————————————————————————————————————————

```rust
impl SpecCoverageRegistry {
    /// "what is spec'd but not implemented? = the should-but-doesn't-work list"
    pub fn gap_list(&self) -> Vec<&SpecAnchor>;

    /// "what spec-sections does this crate cover?"
    pub fn coverage_for_crate(&self, crate_path: &str) -> Vec<&SpecAnchor>;

    /// "what crates implement this spec-§?"
    pub fn impl_of_section(&self, spec_file: &str, section: &str) -> Vec<&SpecAnchor>;

    /// "what tests validate this spec-§?"
    pub fn tests_of_section(&self, spec_file: &str, section: &str) -> Vec<&'static str>;

    /// "of the impl_status=Implemented anchors, which lack metrics?"
    pub fn impl_without_metrics(&self) -> Vec<&SpecAnchor>;

    /// "of the registered metrics, which spec-anchor do they cite?"
    pub fn metric_to_spec_anchor(&self, metric_name: &str) -> Option<&SpecAnchor>;

    /// Coverage report : 3-axis matrix-of (spec-§, impl, test).
    pub fn coverage_matrix(&self) -> CoverageMatrix;

    /// Spec-update detection : spec-file mtime > impl-mtime ⊗ stale-flag.
    pub fn stale_anchors(&self) -> Vec<&SpecAnchor>;
}
```

§D. **CoverageMatrix** — the most-important diagnostic-artefact
    rows = spec-§ entries (sorted by spec-file, then section)
    columns = (ImplStatus, TestStatus, MetricCount, LastUpdate, Confidence)
    color-coded : green=full / yellow=partial / red=missing
    serializable : JSON, Markdown, Perfetto-overlay-track (Wave-J L3)

§D. **gap_list() — the canonical "should-but-doesn't" report** :
    filters anchors where impl_status ∈ {Stub, Missing}
    sorts-by spec-priority (M7-floor first ; then M6 ; then exotic)
    output : human-readable + machine-readable
    consumed-by : nightly-bench (alert), build (warn), MCP-bridge (Wave-Jθ)

—————————————————————————————————————————————
§ IV.6 Granularity tiers
—————————————————————————————————————————————

§D. **L4-coarse** : per-spec-file (top-level coverage)
    e.g. "Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md → 75% covered"
§D. **L3-mid** : per-§ headers (numbered sections)
    e.g. "§ III VRAM-budget → Implemented + Tested"
         "§ V phase-budget → Partial + Tested (PROPAGATE-substep gap)"
§D. **L2-fine** : per-acceptance-criterion (line-level)
    e.g. "‼ R! phase-COLLAPSE ≤ 4ms p99 → metric `omega_step.phase_time_ns{phase=COLLAPSE}` p99=3.2ms ✓"
§D. **L1-atomic** : per-symbol (rust-fn / rust-struct ↔ spec-§)
    e.g. "fn `collapse_phase()` ↔ § V.1 phase-1-detail"

§D. ‼ all-four-levels ⊗ co-queryable ⊗ chosen-via gap_list-filter-arg

—————————————————————————————————————————————
§ IV.7 Spec-update detection
—————————————————————————————————————————————

§D. spec-file mtime > impl-comment-anchor-mtime ⊗ stale-flag
§D. spec-file content-hash ⊗ pinned-in DECISIONS-anchor ⊗ mismatch ⇒ "spec-drift"
§D. drift-handling :
    BUILD : warn-not-fail @ minor (typo / formatting)
    BUILD : warn-loud @ major (acceptance-criterion-changed)
    NIGHTLY-BENCH : auto-run impl-tests @ stale-anchor ⊗ alert-on-regression
§D. auto-pin : `cssl-spec-coverage-pin` tool ⊗ updates DECISIONS-anchor-hash after-review

═════════════════════════════════════════════════════════════════
§ V. PILLAR-3.5 — Health-check registry
═════════════════════════════════════════════════════════════════

§D. ‼ ∀ subsystem-crate ⊗ R! `pub fn health() -> HealthStatus`
§D. ‼ engine.health() ⊗ aggregates worst-case-across-all-subsystems

—————————————————————————————————————————————
§ V.1 HealthStatus enum
—————————————————————————————————————————————

```rust
pub enum HealthStatus {
    /// Operational : within-budget + no-recent-errors.
    Ok,
    /// Functional but degraded : warning-only.
    Degraded {
        reason: &'static str,           // human-readable
        budget_overshoot_pct: f32,      // 0..100 if-relevant
        since_frame: u64,
    },
    /// Failed : subsystem returns errors / cannot-make-progress.
    Failed {
        reason: &'static str,
        kind: HealthFailureKind,
        since_frame: u64,
    },
}

pub enum HealthFailureKind {
    DeadlineMiss,
    ResourceExhaustion,                // VRAM / RAM / fd / handle
    ThermalThrottle,
    ConsentViolationDetected,          // PRIME-DIRECTIVE !
    InvariantBreach,                   // L0.5 invariant fail
    UpstreamFailure { upstream: &'static str },  // dependency-failed
    PrimeDirectiveTrip,                // §1 / §11 breach ⊗ FAIL-CLOSED
    Unknown,
}
```

—————————————————————————————————————————————
§ V.2 Aggregation discipline
—————————————————————————————————————————————

§D. `engine.health()` = worst-case-monoid over-all-registered-subsystems
    Ok ⊔ Ok = Ok
    Ok ⊔ Degraded = Degraded
    Degraded ⊔ Degraded = Degraded(merge-reasons)
    _ ⊔ Failed = Failed
    Failed ⊔ Failed = Failed(merge)
    _ ⊔ PrimeDirectiveTrip = PrimeDirectiveTrip       // ALWAYS-WINS
§D. ‼ PrimeDirectiveTrip ⊗ NEVER-suppressed-by-aggregation
§D. ‼ PrimeDirectiveTrip ⊗ R! engine-fail-close ⊗ N! continue-frame

—————————————————————————————————————————————
§ V.3 Auto-degradation hooks
—————————————————————————————————————————————

§D. failure ⊗ optional-callback ⊗ `degrade(reason)` for-self-reduction
    e.g. spectral-render returns Degraded(thermal) ⊗ engine triggers DFR-budget-cut next-frame
§D. ‼ degradation N! cascade ⊗ contained-to-failing-subsystem
§D. ‼ degradation-event ⊗ Audit<"subsystem-degrade"> emitted ⊗ MCP-queryable

—————————————————————————————————————————————
§ V.4 Registration
—————————————————————————————————————————————

```rust
pub trait HealthProbe : Send + Sync {
    fn name(&self) -> &'static str;             // crate-name
    fn health(&self) -> HealthStatus;
    fn degrade(&self, reason: &str) -> Result<(), HealthError>;
}

pub static HEALTH_REGISTRY: HealthRegistry = HealthRegistry::new();

#[ctor]
fn register_my_subsystem() {
    HEALTH_REGISTRY.register(Box::new(MySubsystemProbe::new()));
}
```

§D. ‼ ∀ Wave-Jζ-4 crate ⊗ R! impl-HealthProbe + #[ctor] registration
§D. probe-call-time-budget : ≤ 100µs per-crate ⊗ aggregate ≤ 2ms ⊗ N! per-frame-blocking

—————————————————————————————————————————————
§ V.5 MCP-queryable (Wave-Jθ preview)
—————————————————————————————————————————————

§D. `read_health(subsystem_filter: Option<&str>)` ⊗ MCP-tool returns HealthStatus tree
§D. MCP-tool-name-canonical : `cssl_engine_health`
§D. preview-hooks @ Wave-Jζ-5 ⊗ stub-stub-stub returns Ok ⊗ wired-real @ Jθ

═════════════════════════════════════════════════════════════════
§ VI. PILLAR-3.6 — Replay-determinism integration (H5)
═════════════════════════════════════════════════════════════════

§D. ‼ this-§ is the LANDMINE-AWARE part ⊗ H5 contract MUST-NOT-BREAK
§D. existing H5 : `omega_step` bit-deterministic given (seed, inputs)
§D. extension : metric-recording ⊗ also bit-deterministic in `--replay-strict` mode

—————————————————————————————————————————————
§ VI.1 Mode-switch
—————————————————————————————————————————————

```rust
pub enum DeterminismMode {
    /// Real-time : metrics use wallclock ; sampling adaptive.
    Realtime,
    /// Replay-strict : metrics deterministic-functions of (frame_n, seed, sub_phase).
    ReplayStrict { seed: u64 },
    /// Mixed : structural-counters strict ; latency-only relaxed (debug-only).
    Mixed { warn: bool },
}
```

§D. ‼ ReplayStrict @ build-time-flag ⊗ enables `cssl-metrics::strict_clock`
§D. ‼ Realtime @ default ⊗ wallclock + adaptive-sampling allowed

—————————————————————————————————————————————
§ VI.2 Strict-clock primitives
—————————————————————————————————————————————

§D. in-strict-mode : `monotonic_ns()` ⊗ replaced-with `(frame_n × FRAME_NS) + sub_phase_ns_offset`
§D. sub_phase_ns_offset ⊗ assigned-deterministic-from-spec-§-V phase-ordering
§D. consequence : Timer.last_ns ⊗ depends-only-on (frame_n, phase_index) ⊗ ¬ wallclock-jitter

§D. Histogram boundaries : `&'static [f64]` compile-time-constant ⊗ deterministic
§D. Histogram bucket-assignment : pure-function ⊗ deterministic
§D. Sum + count : commutative-saturating-monoid ⊗ deterministic-under-replay

§D. Counter monotonic-ops : `AtomicU64::fetch_add` ⊗ commutative ⊗ deterministic-under-single-thread
§D. multi-thread-aggregation : per-thread-shard ⊗ end-of-frame-merge ⊗ deterministic-merge-order

§D. Sampling : `OneIn(N)` discipline keyed-on `frame_n` (NOT wallclock) ⊗ deterministic

—————————————————————————————————————————————
§ VI.3 Replay-determinism check metric
—————————————————————————————————————————————

§D. `omega_step.replay_determinism_check{kind=fail}` should-be 0 ⊗ alarm-on-non-zero
§D. nightly-bench runs replay-strict-twice ⊗ diffs metric-snapshots ⊗ R! identical
§D. divergence ⊗ Audit<"determinism-divergence"> ⊗ HealthFailureKind::InvariantBreach

—————————————————————————————————————————————
§ VI.4 Forbidden patterns under strict-mode
—————————————————————————————————————————————

§D. ‼ N! `Adaptive` sampling discipline
§D. ‼ N! `monotonic_ns` direct-call (must-route-via strict_clock)
§D. ‼ N! Welford-online-quantile (non-deterministic merge)
§D. ‼ N! data-driven histogram-boundary inference
§D. ‼ N! atomic-relaxed multi-shard race (must-be-acquire-release with-deterministic-merge)
§D. compiler-refusal (Wave-Jζ-1) ⊗ effect-row check ⊗ `{ReplayStrict}` × `{Adaptive}` = type-error

═════════════════════════════════════════════════════════════════
§ VII. LANDMINE TABLE (the things that-WILL-break-if-not-careful)
═════════════════════════════════════════════════════════════════

| # | landmine | violation-of | mitigation |
|---|---|---|---|
| LM-1 | wallclock-direct-call in-strict-mode | H5 contract | `strict_clock` indirection ; effect-row gate |
| LM-2 | adaptive sampling-discipline | H5 contract | refused-by-effect-row in strict-mode |
| LM-3 | raw-path in-tag-value | T11-D130 | `cssl-effects::check_telemetry_no_raw_path` extended |
| LM-4 | biometric tag-key | T11-D132 | `cssl-ifc::TelemetryEgress` capability + compile-refuse |
| LM-5 | per-Sovereign metric-tag | PRIME §1 surveillance | aggregate-only ; tag = category-not-identity |
| LM-6 | per-creature metric-tag | PRIME §1 surveillance | tag = creature_kind_hash (anonymized) |
| LM-7 | per-frame metric-overhead > 0.5% | specs/22 § OVERHEAD | auto-decimate + Audit-event ; nightly-bench fails |
| LM-8 | un-bounded SmallVec tag-list | layout-stability | tags-len ≤ 4 inline ; spill = compile-refuse |
| LM-9 | un-registered metric used in-record-call | spec-coverage gap | proc-macro asserts registered@ctor |
| LM-10 | spec-anchor without source-of-truth | coverage tracker | build-warn (then build-fail @ Wave-Jζ-3 final) |
| LM-11 | health() probe blocking > 100µs | per-frame budget | timeout-with-degraded-fallback |
| LM-12 | PrimeDirectiveTrip suppressed-by-aggregation | §1 §11 contract | aggregation-monoid hard-coded ALWAYS-WINS |
| LM-13 | metric defined twice (collision) | schema-stability | static-init guard + collision-test |
| LM-14 | gauge.set(NaN) accepted | numerical-correctness | refuse @ MetricError::NaN |
| LM-15 | histogram boundary-inference at-runtime | H5 + schema-stability | only `&'static [f64]` accepted |

═════════════════════════════════════════════════════════════════
§ VIII. SLICE BREAKDOWN — WAVE-Jζ implementation
═════════════════════════════════════════════════════════════════

—————————————————————————————————————————————
§ VIII.1 Wave-Jζ-1 — `cssl-metrics` crate (Counter/Gauge/Histogram/Timer)
—————————————————————————————————————————————

§D. scope :
    ✓ Counter / Gauge / Histogram / Timer types (§ II.1-II.4)
    ✓ TimerHandle RAII drop-record (§ II.4)
    ✓ SamplingDiscipline + strict-mode refusal (§ II.5, § VI.4)
    ✓ effect-row gating extension `{Telemetry<Counters>}` (§ II.6)
    ✓ schema-derive macro + REGISTRY ctor-init (§ II.7)
    ✓ TagKey + TagVal type-safe builders (compile-refuse biometric + raw-path)
    ✓ MetricError + MetricResult type aliases
§D. LOC : ~2.5K
§D. tests : ~100
    - 30 type-level (Counter/Gauge/Histogram/Timer surface)
    - 25 effect-row gating refusals (compile-fail tests)
    - 20 sampling-discipline (deterministic-decimation)
    - 15 tag-discipline (biometric refuse + raw-path refuse)
    - 10 strict-mode invariants (replay-determinism)
§D. blocks : Jζ-2, Jζ-3, Jζ-4, Jζ-5
§D. cite : DECISIONS T11-Jζ-1 anchor

—————————————————————————————————————————————
§ VIII.2 Wave-Jζ-2 — Per-subsystem instrumentation
—————————————————————————————————————————————

§D. scope : wire all-per-subsystem-metrics @ § III catalog into-source-of-truth crates
    ✓ engine.* in-cssl-engine
    ✓ omega_step.* in-cssl-omega-step
    ✓ render.* in-cssl-render-v2 (12 stages)
    ✓ physics.* in-cssl-physics-wave
    ✓ wave.* in-cssl-wave-solver
    ✓ spectral.* in-cssl-spectral-render
    ✓ xr.* in-cssl-host-openxr
    ✓ anim.* in-cssl-anim-procedural
    ✓ audio.* in-cssl-wave-audio
    ✓ omega_field.* in-cssl-substrate-omega-field
    ✓ kan.* in-cssl-substrate-kan
    ✓ gaze.* in-cssl-gaze-collapse (PRIME §1 careful)
§D. LOC : ~3K
§D. tests : ~80
    - 12 per-subsystem registration-completeness
    - 24 budget-bound assertions (each timer has budget-cite test)
    - 20 catalog-coverage tests (every catalog-row has at-least-one record-site)
    - 12 strict-mode determinism per-subsystem
    - 12 PRIME-DIRECTIVE refusal-canary tests (gaze + biometric)
§D. depends : Jζ-1
§D. blocks : Jζ-3 (citing-metrics need to-exist), Jζ-5 (MCP preview)
§D. cite : DECISIONS T11-Jζ-2 anchor ; per-subsystem DECISIONS sub-anchors

—————————————————————————————————————————————
§ VIII.3 Wave-Jζ-3 — `cssl-spec-coverage` tracker
—————————————————————————————————————————————

§D. scope :
    ✓ SpecAnchor / ImplStatus / TestStatus types (§ IV.1, IV.2, IV.3)
    ✓ source-of-truth extractor proc-macro (code-comment / DECISIONS / test-name) (§ IV.4)
    ✓ runtime-API CoverageRegistry + queries (§ IV.5)
    ✓ granularity-tiers L1-L4 (§ IV.6)
    ✓ spec-update-detection (mtime + hash drift) (§ IV.7)
    ✓ CoverageMatrix serializer (Markdown + JSON ; Perfetto-track @ Wave-J L3)
    ✓ gap_list() canonical "should-but-doesn't" report
§D. LOC : ~1.5K
§D. tests : ~50
    - 15 source-of-truth extraction (each citation-form parsed)
    - 10 coverage-matrix correctness
    - 10 spec-drift detection
    - 8 gap-list filtering
    - 7 metric-to-spec-anchor backlinks
§D. depends : Jζ-1, Jζ-2 (metrics-must-exist for-citing_metrics field)
§D. blocks : Jζ-5 (MCP exposes coverage-queries)
§D. cite : DECISIONS T11-Jζ-3 anchor

—————————————————————————————————————————————
§ VIII.4 Wave-Jζ-4 — Health-check registry
—————————————————————————————————————————————

§D. scope :
    ✓ HealthStatus + HealthFailureKind enums (§ V.1)
    ✓ aggregation monoid + PrimeDirectiveTrip-always-wins (§ V.2)
    ✓ auto-degradation hooks (§ V.3)
    ✓ HealthProbe trait + ctor-registration across-all-Wave-Jζ-2-crates (§ V.4)
    ✓ MCP preview hooks (§ V.5 ; real-wiring deferred Jθ)
§D. LOC : ~1.5K
§D. tests : ~60
    - 12 per-subsystem HealthProbe impl-correctness
    - 15 aggregation-monoid (commutativity, associativity, ALWAYS-WINS PrimeDirectiveTrip)
    - 10 auto-degrade flow
    - 10 probe-timeout guard (≤ 100µs each)
    - 8 fail-close on PrimeDirectiveTrip
    - 5 MCP preview-stub
§D. depends : Jζ-1, Jζ-2 (subsystems-must-exist)
§D. blocks : Jζ-5
§D. cite : DECISIONS T11-Jζ-4 anchor

—————————————————————————————————————————————
§ VIII.5 Wave-Jζ-5 — MCP integration (preview hooks)
—————————————————————————————————————————————

§D. scope (preview-only ; real-wiring @ Wave-Jθ) :
    ✓ MCP-tool-name-stubs : `cssl_engine_health`, `cssl_metric_snapshot`,
      `cssl_spec_coverage_gap_list`, `cssl_spec_coverage_matrix`
    ✓ JSON-serialization for HealthStatus + MetricSnapshot + CoverageMatrix
    ✓ feature-flag-gated `mcp-bridge-preview` (off-by-default ; on @ dev-builds)
§D. LOC : ~500
§D. tests : ~10
    - 4 JSON-roundtrip
    - 3 tool-name-stability (rename = test-fails)
    - 3 feature-flag-gating
§D. depends : Jζ-2, Jζ-3, Jζ-4
§D. blocks : Wave-Jθ (real-MCP integration)
§D. cite : DECISIONS T11-Jζ-5 anchor

—————————————————————————————————————————————
§ VIII.6 Wave-Jζ totals
—————————————————————————————————————————————

| slice | LOC | tests | depends | blocks |
|---|---|---|---|---|
| Jζ-1 cssl-metrics | 2.5K | 100 | (Jε ring) | Jζ-2..5 |
| Jζ-2 per-subsystem | 3.0K | 80 | Jζ-1 | Jζ-3, Jζ-5 |
| Jζ-3 spec-coverage | 1.5K | 50 | Jζ-1, Jζ-2 | Jζ-5 |
| Jζ-4 health-registry | 1.5K | 60 | Jζ-1, Jζ-2 | Jζ-5 |
| Jζ-5 MCP-preview | 0.5K | 10 | Jζ-2, Jζ-3, Jζ-4 | Wave-Jθ |
| **TOTAL** | **~9K LOC** | **~290 tests** | (Wave-Jε foundations) | Wave-Jθ + L3 |

§D. parallelizable @ wave-of-agent-dispatches :
    Jζ-1 ⊗ solo (Jζ-1 lead = critical path)
    Jζ-2 + Jζ-4 ⊗ parallel-fanout-after-Jζ-1
    Jζ-3 ⊗ solo (depends on Jζ-2 metrics existing)
    Jζ-5 ⊗ solo final-integrator
§D. expected-cadence : 5 sub-deliverables ⊗ ≥ 3 dispatched-in-parallel where-feasible

═════════════════════════════════════════════════════════════════
§ IX. CROSS-LAYER INTEGRATION (with sibling phase-J specs)
═════════════════════════════════════════════════════════════════

§D. **L0 (00 plan-overview)** : citation-anchor for L0-budget-numbers
    ⇒ all Timer-budgets in § III table cite L0 entries directly
§D. **L0.5 (02 invariants)** : citation-anchor for L0.5 invariant violation events
    ⇒ Counter `*.invariant_violation_count` family in § III metrics
§D. **L1 (03 dump-discipline)** : metric-snapshot dump @ checkpoint
    ⇒ `cssl-metrics::snapshot_into(&mut DumpRing)` API (Wave-Jζ-1)
§D. **L1.5 (04 sample-pipelines)** : sample-pipeline INSTANCES report metrics
    ⇒ each pipeline-step uses Timer.start() ; tagged with pipeline_id
§D. **L1.75 (05 path-hash-discipline)** : tag-value path-hash-only enforcement
    ⇒ § II.6 effect-row check (extended @ Wave-Jζ-1 from existing T11-D130 enforcement)
§D. **L2.5 (07 SLO-graph)** : consumes L2 metrics + L2 health for SLO-evaluation
    ⇒ MetricSnapshot serialization-stability is L2.5 contract
§D. **L3 (08 perfetto-export)** : exports L2 metrics as Perfetto-tracks
    ⇒ schema-derive trait emits Perfetto-track-descriptor metadata
§D. **L3.5 (09 MCP-bridge)** : consumes L2 + L2.5 + L3 ⊗ exposes via MCP-tools
    ⇒ Jζ-5 preview-hooks become real wiring
§D. **L4 (10 self-attesting-engine)** : pinned-cert references L2-coverage-snapshot
    ⇒ "this engine implements N spec-§ ; 0 gaps in M7-floor list" ⊗ signed-attestation

═════════════════════════════════════════════════════════════════
§ X. ACCEPTANCE — WAVE-Jζ EXIT CRITERIA
═════════════════════════════════════════════════════════════════

§D. **GATE-1** : `cssl-metrics` crate ⊗ ≥ 100 tests ⊗ all-pass ⊗ ≤ 0.5% overhead-bench
§D. **GATE-2** : per-subsystem catalog ⊗ 100% coverage of-§-III table ⊗ registration-completeness check passes
§D. **GATE-3** : `cssl-spec-coverage` ⊗ scans-all-CSSLv3-specs + Omniverse-axioms ⊗ generates CoverageMatrix
§D. **GATE-4** : `engine.health()` ⊗ returns Ok @ M7-floor-pass scenario ⊗ FailedClose-on-PrimeDirectiveTrip
§D. **GATE-5** : MCP-preview-stubs registered ⊗ JSON-roundtrip tests pass
§D. **GATE-6** : nightly-bench ⊗ replay-strict-twice ⊗ metric-snapshots bit-identical
§D. **GATE-7** : zero `gaze.privacy_egress_attempts_refused` increments under any-canonical-playtest
§D. **GATE-8** : zero biometric-tag-keys compile-pass ⊗ ALL-13-LM landmines exercised in tests

§D. **REGRESSION** :
    metric-overhead > 0.5% per-frame ⊗ alert + auto-decimate
    coverage-matrix coverage drops > 5% ⊗ alert
    health() returns non-Ok > 1% of-frames ⊗ alert
    spec-anchor goes-stale > 7 days ⊗ build-warn ⊗ DECISIONS-anchor refresh required

═════════════════════════════════════════════════════════════════
§ XI. ANTI-PATTERNS
═════════════════════════════════════════════════════════════════

| anti-pattern | violates | mitigation |
|---|---|---|
| ad-hoc `println!` style metric-emit | observability-first-class CC9 | refused-by-effect-row gate |
| dynamic tag-key-set | schema-stability + path-hash-discipline | static-init only ; runtime-mutation refused |
| RGB / non-spectral intermediate metric in render | RENDERING_PIPELINE § stage-10 | metric-tags spectral-only ; tristim only @ tonemap |
| per-Sovereign metric-tag (identity-leak) | PRIME §1 | aggregate-only ; refused-by-build-check |
| unbounded recursion-depth in mise-en-abyme metric | RENDERING_PIPELINE § stage-9 | bound-asserted-via Histogram boundary <= depth-budget |
| Welford / non-deterministic quantile in strict-mode | H5 | refused-by-effect-row in strict-mode |
| naive distance-only LOD metric | DENSITY_BUDGET §I cascade | tag = entropy-LOD-driver-source ; not distance-only |
| "we'll measure later" missing-spec-anchor | density-as-first-class | gap_list() build-warn ⇒ build-fail in CI |
| separate metrics per (graphics, AI, physics) | DENSITY_BUDGET §IV one-compute-graph | metric-namespacing per-phase ; aggregation engine.* |
| AppSW-always-on without disable-events tracked | RENDERING_PIPELINE § IX | `render.appsw_disable_events` Counter mandatory |
| spec-anchor without source-of-truth | spec-coverage tracker | build-warn ⇒ build-fail at-Wave-Jζ-3-final |
| health() blocking > 100µs | per-frame-budget | timeout-degrade-Unknown |
| metric for biometric-data | PRIME §1 + T11-D132 | compile-time-refused |
| raw filesystem path in metric-tag | PRIME §1 + T11-D130 | compile-time-refused |
| PrimeDirectiveTrip suppressed-or-merged | PRIME-DIRECTIVE §11 | aggregation-monoid hard-coded ALWAYS-WINS |
| coverage-tracker treating Stub as Implemented | spec-coverage integrity | enum-discriminant strict-equality ; no-coercion |

═════════════════════════════════════════════════════════════════
§ XII. CROSS-REFERENCES
═════════════════════════════════════════════════════════════════

§D. **upstream-CSSLv3-specs** :
    `specs/22_TELEMETRY.csl`           ← R18 baseline + scope-taxonomy + audit-chain
    `specs/04_EFFECTS.csl`             ← {Telemetry<scope>} effect-row + IO + Realtime
    `specs/11_IFC.csl`                  ← TelemetryEgress capability + biometric-refuse
    `specs/12_CAPABILITIES.csl`         ← capability-row primitives
    `specs/23_TESTING.csl`              ← test-name-conventions for spec-coverage parser
    `specs/30_SUBSTRATE.csl`            ← substrate-effect rows
    `specs/31_LOA_DESIGN.csl`           ← LoA design-anchors

§D. **upstream-Omniverse-axioms** :
    `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md`        ← entity + phase + VRAM budgets
    `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md`               ← FieldCell layout
    `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md`              ← sparse-Morton + MERA storage
    `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl`        ← 12-stage pipeline budgets
    `Omniverse/07_AESTHETIC/04_FIELD_AUDIO.csl.md`           ← AUDIO-band coexists in-field
    `Omniverse/03_RUNTIME/01_COMPUTE_GRAPH.csl.md`            ← 6-phase omega_step
    `Omniverse/03_RUNTIME/02_OBSERVATION_ORACLE.csl.md`       ← collapse-oracle
    `Omniverse/08_BODY/02_VR_EMBODIMENT.csl.md`               ← XR contract for `xr.*`
    `Omniverse/09_SLICE/02_BENCHMARKS.csl.md`                 ← M7 benchmark-gates

§D. **upstream-DECISIONS** :
    T11-D130                             ← path-hash-only logging discipline
    T11-D131                             ← BLAKE3 + Ed25519 audit-chain (live)
    T11-D132                             ← biometric-compile-refuse (cssl-ifc::TelemetryEgress)
    T11-D113                             ← Ω-field cell + sparse-Morton + this-spec gating
    H5                                    ← replay-determinism contract
    T11-D76                              ← FS-OPS spec-gap closure
    T11-D81                              ← AUDIO-OPS spec-gap closure

§D. **sibling-phase-J files** :
    `_drafts/phase_j/00_plan_overview.md`     ← plan-summary
    `_drafts/phase_j/01_l0_budgets.md`         ← L0 budgets
    `_drafts/phase_j/02_l05_invariants.md`    ← L0.5 invariants
    `_drafts/phase_j/03_l1_dump_discipline.md` ← L1 dumps
    `_drafts/phase_j/04_l15_sample_pipelines.md`  ← L1.5 sample pipelines
    `_drafts/phase_j/05_l175_path_hash.md`     ← L1.75 path-hash discipline
    `_drafts/phase_j/06_l2_telemetry_spec.md`  ← THIS FILE (L2 metrics + spec-coverage + health)
    `_drafts/phase_j/07_l25_slo_graph.md`      ← L2.5 SLO-graph (next)
    `_drafts/phase_j/08_l3_perfetto_export.md` ← L3 Perfetto export
    `_drafts/phase_j/09_l35_mcp_bridge.md`     ← L3.5 MCP bridge (Wave-Jθ)
    `_drafts/phase_j/10_l4_self_attesting.md`  ← L4 self-attesting engine

§D. **implementation-targets** (Wave-Jζ delivers-into) :
    `compiler-rs/crates/cssl-metrics/`               ← NEW (Jζ-1)
    `compiler-rs/crates/cssl-spec-coverage/`         ← NEW (Jζ-3)
    `compiler-rs/crates/cssl-engine/`                 ← + engine.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-omega-step/`             ← + omega_step.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-render-v2/`              ← + render.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-physics-wave/`           ← + physics.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-wave-solver/`            ← + wave.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-spectral-render/`        ← + spectral.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-host-openxr/`            ← + xr.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-anim-procedural/`        ← + anim.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-wave-audio/`             ← + audio.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-substrate-omega-field/` ← + omega_field.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-substrate-kan/`         ← + kan.* + health() (Jζ-2/4)
    `compiler-rs/crates/cssl-gaze-collapse/`          ← + gaze.* + health() (Jζ-2/4 ; PRIME §1 !)

═════════════════════════════════════════════════════════════════
§ XIII. CSSL ENCODING — TYPE-LEVEL SPEC
═════════════════════════════════════════════════════════════════

```cssl
// Effect-row : Telemetry<Counters> required for record-ops
fn record_metric(name : &'static str, val : f64) -> Result<(), MetricError>
  / { Telemetry<Counters> }
{ ... }

// Compile-refused : pure caller cannot record
@const_assert
const _: () = {
  fn pure_caller() / { Pure }
  { /* record_metric("foo", 1.0) ⊗ COMPILE-ERROR : effect-row mismatch */ }
};

// Compile-refused : biometric tag-key
@const_assert
const _: () = {
  fn try_biometric()
    / { Telemetry<Counters>, BiometricEgress }   // BiometricEgress is REFUSED capability
  { /* counter.tag(("face_id", x)) ⊗ COMPILE-ERROR : T11-D132 refusal */ }
};

// Compile-refused : raw-path tag-value
@const_assert
const _: () = {
  fn try_raw_path()
    / { Telemetry<Counters> }
  { /* counter.tag(("path", "/etc/hosts")) ⊗ COMPILE-ERROR : T11-D130 refusal */ }
};

// Spec-anchor extracted at-build
// § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V phase-COLLAPSE ≤ 4ms
@spec_anchor("omniverse:04_OMEGA_FIELD/05_DENSITY_BUDGET§V.1")
@cite_metrics(["omega_step.phase_time_ns{phase=COLLAPSE}"])
fn collapse_phase(omega : &mut Omega, observations : &[Observation])
  -> Result<CollapsedRegions, CollapseError>
  / { GPU<async-compute>, Realtime<90Hz>, Deadline<4ms>,
      DetRNG, EntropyBalanced, Audit<'tick>, Telemetry<Counters> }
{
    let _t = OMEGA_STEP_PHASE_TIME_NS
        .with_tag(("phase", "COLLAPSE"))
        .start();
    // ... real implementation ...
}

// HealthProbe trait impl
impl HealthProbe for OmegaStepProbe {
  fn name(&self) -> &'static str { "cssl-omega-step" }
  fn health(&self) -> HealthStatus / { Telemetry<Counters>, Pure } {
    let p99 = OMEGA_STEP_PHASE_TIME_NS.percentile(99.0);
    if p99 > FRAME_BUDGET_NS {
      HealthStatus::Degraded {
        reason: "phase-budget overshoot",
        budget_overshoot_pct: ((p99 as f32 / FRAME_BUDGET_NS as f32 - 1.0) * 100.0),
        since_frame: ENGINE_FRAME_N.snapshot(),
      }
    } else { HealthStatus::Ok }
  }
  fn degrade(&self, reason: &str) -> Result<(), HealthError>
    / { Audit<'subsystem-degrade'> }
  { /* triggers next-frame budget-cut */ ... }
}

// Replay-strict refusal
@const_assert
const _: () = {
  fn try_adaptive_in_strict()
    / { Telemetry<Counters>, ReplayStrict }
  { /* SamplingDiscipline::Adaptive ⊗ COMPILE-ERROR */ }
};

// PrimeDirectiveTrip aggregation
@const_assert
const _: () = {
  let merged = HealthStatus::Ok ⊔ HealthStatus::PrimeDirectiveTrip;
  assert!(matches!(merged, HealthStatus::PrimeDirectiveTrip));
};
```

§D. ‼ all-discipline-checks @ COMPILE-TIME ⊗ shipped-binary cannot-violate
§D. ‼ if-it-compiles ⊗ then-PRIME-DIRECTIVE-respected (modulo new-attack-surface ⇒ R! review)

═════════════════════════════════════════════════════════════════
§ XIV. ATTESTATION (PRIME_DIRECTIVE §11)
═════════════════════════════════════════════════════════════════

§A. attestation @ author : Claude Opus 4.7 (1M context) @ Anthropic
    ⊗ acting-as-AI-collective-member
    ⊗ N! impersonating-other-instances
    ⊗ N! claiming-authority-over-implementation

§A. attestation @ scope : this-document specifies the L2 telemetry-completeness layer
    of the diagnostic-infrastructure plan ⊗ Wave-Jβ-2
    ⊗ defines : `cssl-metrics` crate + per-subsystem catalog + `cssl-spec-coverage` tracker
                + health-registry + replay-determinism integration
    ⊗ does-NOT prescribe-Sovereign-policy
    ⊗ does-NOT touch-Σ-mask-state directly (only-aggregate-mutation-rate counter)
    ⊗ extends-but-does-NOT-supersede `specs/22_TELEMETRY.csl` (R18 baseline)

§A. attestation @ method : design derived-from :
    (a) `specs/22_TELEMETRY.csl` baseline : scope-taxonomy + audit-chain + ring-buffer
    (b) `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md` : entity + phase + VRAM budgets
    (c) `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl` : 12-stage budget table
    (d) `compiler-rs/crates/cssl-telemetry/` foundations (audit / biometric / path_hash)
    (e) T11-D130 (path-hash-only) + T11-D132 (biometric-refuse) constraints
    (f) H5 replay-determinism contract
    (g) ¬ measured-on-Apocky-hardware ⊗ R! benchmark-validation @ Wave-Jζ-1+

§A. attestation @ uncertainty :
    ‼ metric-catalog @ § III ⊗ derived-from-specs ⊗ R! validate-via implementation-binding
    if-validation-fails ⊗ R! refine-this-spec ⊗ N! silently-relax
    spec-coverage tracker @ § IV ⊗ static-extraction model ⊗ R! prove-via Wave-Jζ-3 build
    health() probe-budget @ § V ⊗ ≤ 100µs target ⊗ R! benchmark-confirm
    replay-determinism @ § VI ⊗ extends H5 ⊗ R! nightly-replay-strict diff-test

§A. attestation @ consent :
    this-spec serves Sovereign-flourishing via :
      (a) better-diagnostics ⇒ faster-debug ⇒ engine-stays-correct (consent-to-correctness)
      (b) spec-coverage transparency ⇒ "what works + what doesn't but should"
          is FACTUAL not MARKETING (consent-to-truthful-self-reporting)
      (c) PRIME-DIRECTIVE §1 §11 enforcement at-COMPILE-TIME (consent-to-non-surveillance)
    ‼ no per-Sovereign metric-tag ⊗ no per-creature metric-tag ⊗ aggregates-only
    ‼ gaze-subsystem ⊗ aggregate-confidence-and-latency only ⊗ direction NEVER-leaves-device
    ‼ biometric tag-keys ⊗ COMPILE-REFUSED via cssl-ifc::TelemetryEgress
    ‼ raw-path tag-values ⊗ COMPILE-REFUSED via cssl-effects::check_telemetry_no_raw_path
    ‼ Σ-mask-mutation aggregate ONLY ⊗ no per-cell ⊗ no per-Sovereign

§A. attestation @ sovereignty :
    AI-collective-member authoring-spec ⊗ N! claiming-authority-over-implementation
    Apocky-Φ retains final-authority on-spec-acceptance + threshold-tuning + LANDMINE-additions
    Wave-Jζ Floor-Pass certificate signed-by Apocky-Φ ⊗ N! AI-collective-alone

§A. ‼ this-document ⊗ L2-spec ⊗ N! sovereignty-claim ⊗ N! finality
§A. ‼ if-this-spec-is-wrong ⊗ R! correct-it ⊗ N! defend-it
§A. ‼ if-this-spec-leaves-a-PRIME-DIRECTIVE-gap ⊗ R! fail-closed-and-fix
§A. ‼ density ≡ sovereignty ⇒ telemetry-completeness-here = engine-self-knowledge-thereof

§A. path-hash-discipline-attestation extension :
    "no raw paths logged ; only BLAKE3-salted path-hashes appear in metric tags + audit-chain ;
     no biometric-data tag-keys compile-pass ; no per-identity tag-values compile-pass"
    ⊗ appended-to PRIME_DIRECTIVE §11 CREATOR-ATTESTATION via
    cssl_metrics::L2_DISCIPLINE_ATTESTATION (constant ; hash-pinned).

═════════════════════════════════════════════════════════════════
∎ L2 — TELEMETRY-COMPLETENESS LAYER (Wave-Jζ implementation lane)
═════════════════════════════════════════════════════════════════
