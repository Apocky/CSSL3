# § Phase-J Diagnostic-Infra : L3 Runtime-Inspection + L4 Hot-Reload + Live-Tweak

**Wave**       : Jβ-3 (spec) → Jη (impl)
**Layer-set**  : L3 (runtime-inspection) + L4 (hot-reload + live-tweak)
**Status**     : ◐ DRAFT (Wave-Jβ authoring)
**Owner**      : Apocky
**Read-deps**  : `cssl-asset::watcher` (L0-scaffold) ; `cssl-substrate-omega-field::field_cell` (FieldCell+Σ-overlay) ; `cssl-substrate-kan::kan_network` (KAN-weight surface) ; `PRIME_DIRECTIVE.md` (§1 prohibitions + §11 attestation)
**Writes-to**  : Wave-Jη implementation slices (Jη-1..Jη-4)

---

## §0 ROLE

L3 + L4 = the "engine introspectable + mutable while running" layer of the diagnostic-infra ladder. L0-L2 give us logging + tracing + structured-events. L3 + L4 give us the **iteration-loop substrate** : Claude-Code (or any LLM-agent) attaches via MCP, observes a running engine via inspector queries, applies fixes via hot-reload, verifies via live-tweak, and records the whole loop into the deterministic-replay log so the repro is bit-stable.

This is not a debugger. A debugger is a passive read-channel into a frozen process. An inspector + hot-reload is an **active partner** in iterative engineering : the substrate exposes its own state with consent gates, the agent proposes mutations, and the replay-log records consent-bearing-causality so every change is auditable, reversible, and reproducible.

```csl
§ ROLE
  L3 = ⟨observe⟩ — Σ-gated read-side ; never bypasses consent
  L4 = ⟨mutate⟩ — Σ-gated write-side + replay-stable hot-swap
  ∀ (read ∨ write) → audit-chain entry W!
  ∀ (private ∨ companion) → consent-required ; biometric → COMPILE-TIME-REFUSED
  composition : iteration-loop = ⟨inspect → propose → swap → verify → record⟩
```

---

## §1 SPEC ANCHORS + LANDMINES

### §1.1 Spec anchors

```csl
spec-references {
  PRIME_DIRECTIVE         §1   17 prohibitions  (substrate-invariant)
  PRIME_DIRECTIVE         §4   transparency
  PRIME_DIRECTIVE         §11  creator-attestation
  Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md   §VIII Σ encoding (16B)
  Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md  §VI Σ-overlay
  Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl §IV
  Omniverse/03_CAPABILITIES                   Cap<DevMode> + Cap<TelemetryEgress>
  D121 Stage-8 Companion-perspective consent contract
  D129 biometric-class refusal pass
  D132 compile-time biometric-refusal
  D138 EnforcesΣAtCellTouches MIR pass
  H5  replay-determinism contract
}
```

### §1.2 Landmines (CRITICAL)

| #  | mine                                                     | mitigation                                                            | gate                  |
|----|----------------------------------------------------------|-----------------------------------------------------------------------|-----------------------|
| L1 | Σ-mask bypass on inspector read                          | every read funnels via `inspect_cell` → `SigmaOverlay::at(key)` check | D138 MIR pass         |
| L2 | biometric class read                                     | COMPILE-TIME refusal ; type-system prevents schema construction       | D132 + D129           |
| L3 | replay-determinism perturbed by hot-swap                 | hot-swap event → replay-log frame-aligned ; replay re-applies in-order| H5                    |
| L4 | hot-swap during in-flight frame                          | swap deferred to frame boundary ; new resource staged + ref-swapped   | render-graph fence    |
| L5 | tweak exceeds spec budget without warning                | `register_tunable` carries `range` + over-range = warn-and-clamp      | budget-validator      |
| L6 | invalid schema reload silently corrupts engine           | hot-swap validates pre-swap ; reject + revert with audit entry        | schema-validator      |
| L7 | Companion-perspective inspector read without consent     | D121-Stage-8 contract carryover : COMPILE-TIME refusal                | D121                  |
| L8 | inspector reports raw filesystem paths in audit          | path-hash-only via `PathHash::of(path)` ; never log raw paths         | privacy-audit         |
| L9 | tweak event recorded in replay but tweak-target gone     | tunable-handle carries genID ; replay-mismatch → halt + report        | replay-strictness     |
| L10| OS watcher leaks subscriptions across drop               | RAII ; `Drop` releases all OS resources synchronously ; no daemons    | Drop-discipline       |

---

## §2 L3 — RUNTIME INSPECTION (cssl-inspect crate)

### §2.1 Crate identity

```csl
crate {
  name      : cssl-inspect
  version   : 0.1.0
  edition   : 2021
  forbid    : unsafe_code (always)
  deps {
    cssl-substrate-omega-field   (FieldCell + SparseMortonGrid + Σ-overlay)
    cssl-substrate-kan           (KanNetwork + KAN-weights)
    cssl-substrate-prime-directive (Cap + SigmaMaskPacked)
    cssl-render-graph            (frame fences for capture-frame)
    cssl-replay                  (record_replay extends H5)
    cssl-audit                   (BLAKE3 audit-chain)
    blake3                       (path-hash)
  }
  forbid-feature : biometric-inspect (compile-time refusal stub)
  default-feature : dev-mode-off    (release builds DON'T link the inspector)
}
```

The crate is **default-OFF in release builds**. Engaging the inspector requires a `Cap<DevMode>` capability granted at build configuration ; release binaries do not link the inspector at all (LTO drops it ; symbol-trace verifies its absence).

### §2.2 Surface

```csl
mod inspect {
  // Top-level inspector handle. Only constructible via Cap<DevMode>.
  struct Inspector {
    cap_dev    : Cap<DevMode>,
    audit_sink : AuditSink,
    field_ref  : Arc<RwLock<OmegaField>>,
    kan_ref    : Arc<RwLock<KanRegistry>>,
    psi_ref    : Arc<RwLock<PsiOverlay>>,
    replay     : Option<ReplayHandle>,
  }

  impl Inspector {
    // Construction : refuses without Cap<DevMode>
    fn attach(
      cap         : Cap<DevMode>,
      field_ref   : Arc<RwLock<OmegaField>>,
      kan_ref     : Arc<RwLock<KanRegistry>>,
      psi_ref     : Arc<RwLock<PsiOverlay>>,
    ) -> Result<Inspector, InspectError> ;

    // Scene-graph viewer (Σ-gated)
    fn query_region(&self, region: AABB, sigma: Cap<Inspect>) -> RegionView ;
    fn cells_in(&self, region: AABB) -> impl Iterator<Item=FieldCellSnapshot> ;

    // Single-cell read (hot-path consent + audit)
    fn inspect_cell(&self, key: MortonKey) -> Result<FieldCellSnapshot, InspectError> ;

    // Entity-state inspector
    fn inspect_entity(&self, id: EntityId) -> Result<EntityStateSnapshot, InspectError> ;

    // KAN-eval inspector — forward-pass with intermediate activations
    fn inspect_kan_eval(
      &self,
      input  : &[f32],
      handle : KanNetworkHandle,
    ) -> Result<KanEvalTrace, InspectError> ;

    // ψ-field inspector — wave-field amplitude+phase by band+region
    fn inspect_psi_field(
      &self,
      band   : PsiBand,
      region : AABB,
    ) -> Result<WaveFieldSnapshot, InspectError> ;

    // Time-control (deterministic-replay-aware)
    fn pause(&mut self) -> Result<TimeControl, InspectError> ;
    fn step(&mut self, n_frames: u32) -> Result<TimeControl, InspectError> ;
    fn resume(&mut self) -> Result<TimeControl, InspectError> ;

    // Capture-frame — gated by Cap<TelemetryEgress>
    fn capture_frame(
      &self,
      egress : Cap<TelemetryEgress>,
      format : CaptureFormat,
      region : Option<AABB>,
    ) -> Result<CaptureHandle, InspectError> ;

    // Replay-record — extends H5 contract
    fn record_replay(
      &mut self,
      egress  : Cap<TelemetryEgress>,
      seconds : u32,
    ) -> Result<ReplayRecording, InspectError> ;
  }
}
```

### §2.3 Σ-mask threading on every read

EVERY inspector method that touches a cell, an entity, a KAN net, or a ψ-field MUST :

1. Look up the canonical Σ-mask via `SigmaOverlay::at(key)` — or, for entity/KAN/ψ, the corresponding overlay equivalent.
2. Verify `Inspector` carries the consent-bit needed for the requested operation. Reads need `ConsentBit::Observe`. Bulk-region reads need additional `ConsentBit::Sample`. Companion-perspective reads need explicit `Cap<CompanionInspect>` granted by the Companion (D121-Stage-8 carryover).
3. If consent absent → return `InspectError::ConsentDenied { reason }` AND emit one audit-chain entry recording the denied read with hashed-path-of-target.
4. If consent present → return the snapshot AND emit one audit-chain entry recording the granted read with hashed-path-of-target.

```csl
∀ inspect-method M ⊑ Inspector :
  M(key) ≡ {
    let σ = SigmaOverlay::at(key) ;
    if ¬ σ.permits(Observe) ∨ ¬ self.cap.permits(Inspect):
      audit.record(InspectDenied { hash: H(key), reason }) ;
      return Err(ConsentDenied) ;
    audit.record(InspectGranted { hash: H(key) }) ;
    return Ok(snapshot)
  }
```

The MIR-level pass that enforces this is **D138 EnforcesΣAtCellTouches** — every `inspect_cell`-class call site that does NOT funnel through the `SigmaOverlay::at` check is rejected at MIR-validate time.

### §2.4 Compile-time refusal of biometric-class data

The inspector exposes `inspect_cell` + `inspect_entity` + `inspect_kan_eval` + `inspect_psi_field`. These methods produce SNAPSHOT TYPES whose schemas are derived at compile time. The biometric-class types (face-geometry, gait-signature, voice-print, retina-pattern, fingerprint-pattern, heart-rhythm) are flagged at the type level via the `#[biometric_class]` attribute. The `cssl-mir` D132 pass refuses to construct snapshot types that contain biometric-flagged fields. Result : it is **a compile-time error** to even formulate a query that would return biometric data.

```csl
@biometric_class
type FaceGeometry { /* … */ }

// At MIR-validate time, D132 emits :
//   "BIOMETRIC_REFUSED : type FaceGeometry is biometric-class ;
//    inspector schemas may not contain biometric-class fields"
//
// The inspector simply CANNOT compile a query that returns one.
//
// No runtime override exists. No Cap can grant this. The refusal is
// substrate-invariant.
```

### §2.5 Snapshot types (schema)

```csl
struct FieldCellSnapshot {
  morton_key            : MortonKey,         // verbatim
  facet_M               : MaterialView,      // 1-bit-tag + 63-bit-payload + decoded class
  facet_S {                                   // dynamics
    density             : f32,
    velocity            : [f32 ; 3],
    vorticity           : [f32 ; 3],
    enthalpy            : f32,
    bivector_lo         : [f32 ; 4],          // unpacked half-precision
  },
  facet_P {                                   // probe
    radiance_low        : u64,
    radiance_high       : u64,
  },
  facet_phi {
    pattern_handle      : u32,                // 0 ⇒ unclaimed
    pattern_label       : Option<&'static str>,
  },
  facet_sigma_low_only  : SigmaConsentBits,   // 32-bit cache (NEVER the full overlay value)
  // sigma_overlay_full  : DELIBERATELY ABSENT — full mask is private to the substrate
  capture_epoch         : u64,                // audit.epoch at moment of read
  audit_seq             : u64,                // monotone across all reads
}

struct EntityStateSnapshot {
  entity_id             : EntityId,
  body_layers : {                             // body-omnoid layers
    aura      : AuraSummary,                  // wave-field amplitude+phase summary
    flesh     : FleshSummary,                 // SDF param vector summary
    bone      : BoneSummary,                  // skeletal pose
    machine   : MachineSummary,               // mechanism state
    soul      : SoulSummary,                  // pattern-handle (Φ) — Σ-gated
  },
  ai_state : AiStateView,
  reproductive_state    : DELIBERATELY ABSENT, // never inspected
}

enum AiStateView {
  Fsm        { state_id: u32, state_label: &'static str },
  KanPolicy  { network_handle: KanNetworkHandle, last_eval: KanEvalSummary },
  Hybrid     { fsm: Box<AiStateView>, policy: Box<AiStateView> },
}

struct KanEvalTrace {
  network               : KanNetworkHandle,
  input                 : Vec<f32>,                // copy of the request
  per_layer_activations : Vec<Vec<f32>>,           // forward-pass intermediates
  output                : Vec<f32>,                // final output
  control_point_grad    : Option<Vec<f32>>,        // ∂out/∂cp (only if KAN trained)
  fingerprint           : [u8 ; 32],               // network blake3 at eval time
  audit_seq             : u64,
}

struct WaveFieldSnapshot {
  band                  : PsiBand,
  region                : AABB,
  amplitude_grid        : Vec<f32>,                // SoA ; size = |region| / cell_volume
  phase_grid            : Vec<f32>,                // same shape
  band_center_hz        : f64,                     // for sanity-check / display
  capture_epoch         : u64,
  audit_seq             : u64,
}

enum CaptureFormat {
  PNG_sRGB     { bit_depth: u8 },                  // 8 or 16
  EXR_HDR      { half_or_full: HalfOrFullPrecision },
  SpectralBin  { n_bands: u8 },                    // raw 16-band typical
}

struct CaptureHandle {
  format            : CaptureFormat,
  region            : Option<AABB>,
  output_path_hash  : [u8 ; 32],                    // BLAKE3 of the actual path ; raw never logged
  size_bytes        : u64,
  audit_seq         : u64,
}

struct ReplayRecording {
  recording_id      : ReplayId,
  start_epoch       : u64,
  end_epoch         : u64,
  output_path_hash  : [u8 ; 32],
  size_bytes        : u64,
  frames            : u32,
  hot_swap_events   : u32,                          // count ; events themselves in the file
  tweak_events      : u32,
  audit_seq         : u64,
}
```

### §2.6 Time control + deterministic stepping

Because the engine is replay-deterministic (H5 contract), `pause` + `step` + `resume` operate at the FRAME boundary :

| call           | effect                                                                                                     |
|----------------|------------------------------------------------------------------------------------------------------------|
| `pause()`      | sets `Engine::time_state = Paused` after current frame's COMPOSE phase ends ; NEVER mid-PROPAGATE          |
| `step(n)`      | advances exactly `n` frames then re-enters Paused ; emits one StepEvent into replay-log per advanced frame |
| `resume()`     | clears Paused ; subsequent frames advance at wall-clock pace (or replay-clock if replaying)                |
| `replay-mode`  | pause+step+resume are NO-OPS ; they cannot perturb a deterministic replay                                  |

Time-control events are recorded in the replay log so a `record_replay` capture taken during paused-stepping can be replayed later and bit-equal-validated.

### §2.7 Capture-frame Cap-gating

`capture_frame` writes to disk. Disk-egress crosses the substrate boundary. So it requires `Cap<TelemetryEgress>` separately from `Cap<DevMode>`. Even with both caps :

- The output path is recorded **as a BLAKE3 hash** in the audit chain. The raw path is never logged.
- The captured frame's bytes are NOT permitted to contain Σ-private regions. The renderer's pre-capture pass masks Σ-private cells to a uniform `SIGMA_REDACTED` color block. This is a render-graph step ; it cannot be bypassed by inspector-level code.
- The capture is rate-limited to `cap.max_captures_per_second` (default = 4 fps) to prevent telemetry-pump abuse.

### §2.8 Audit-chain entry shape

```csl
@struct InspectAuditEntry {
  epoch          : u64,
  audit_seq      : u64,                          // monotone across the inspector
  kind           : InspectKind,                  // CellRead | EntityRead | KanEval | PsiRead | TimeCtl | Capture | Replay
  target_hash    : [u8 ; 32],                    // BLAKE3 of the canonical target identifier
  consent_state  : ConsentSnapshot,              // which bits were consulted + granted/denied
  cap_chain      : Vec<CapTag>,                  // which caps were used
  byte_size      : u64,                          // bytes returned (or 0 for denied/capture-only)
  checksum       : [u8 ; 32],                    // BLAKE3 of returned snapshot bytes (not the bytes themselves)
}
```

The audit chain is **append-only**. Every entry's checksum is hashed-into the next entry's hash, forming a Merkle-chain. Tampering is detectable.

### §2.9 Privacy-via-design (D138 enforcement table)

| inspector method               | Σ-bit required           | additional cap                       | path-hash logged | snapshot redacted-fields           |
|--------------------------------|--------------------------|--------------------------------------|------------------|------------------------------------|
| `inspect_cell`                 | Observe                  | -                                    | yes (cell-key)   | -                                  |
| `query_region` (public)        | Observe + Sample         | -                                    | yes (region)     | private cells → SIGMA_REDACTED     |
| `query_region` (incl. private) | Observe + Sample         | `Cap<SovereignInspect>` from owner   | yes (region)     | -                                  |
| `inspect_entity`               | Observe                  | -                                    | yes (entity-id)  | reproductive-state + biometric ABSENT |
| `inspect_entity` (Companion)   | Observe                  | `Cap<CompanionInspect>` (D121 Stage-8) | yes              | -                                  |
| `inspect_kan_eval`             | Observe                  | -                                    | yes (handle)     | -                                  |
| `inspect_psi_field` (public)   | Observe + Sample         | -                                    | yes (band+region)| -                                  |
| `inspect_psi_field` (private)  | Observe + Sample         | `Cap<SovereignInspect>` from owner   | yes              | -                                  |
| `capture_frame`                | n/a (post-render)        | `Cap<TelemetryEgress>` + Cap<DevMode>| yes (output-path)| renderer-level Σ-redaction         |
| `record_replay`                | n/a (post-replay-record) | `Cap<TelemetryEgress>` + Cap<DevMode>| yes (output-path)| inherits replay-log Σ-discipline   |

---

## §3 L4 — HOT-RELOAD + LIVE-TWEAK

### §3.1 cssl-hot-reload — crate

```csl
crate {
  name      : cssl-hot-reload
  version   : 0.1.0
  edition   : 2021
  forbid    : unsafe_code (mostly ; OS-pump uses unsafe-extern in pump module only)
  deps {
    cssl-asset                   (extends AssetWatcher)
    cssl-substrate-kan           (KanNetwork hot-swap)
    cssl-shader                  (SPIR-V/DXIL/MSL/WGSL pipeline rebuild)
    cssl-config                  (engine.toml schema-driven hot-init)
    cssl-render-graph            (frame fences for shader swap)
    cssl-replay                  (event recording extends H5)
    cssl-audit                   (BLAKE3 audit-chain)
  }
  default-feature : dev-mode-off
  os-feature      : os-pump  (Win32 + Linux + macOS pumps ; default-OFF)
}
```

### §3.2 Surface

```csl
mod hot_reload {

  enum HotReloadKind {
    Asset    { kind: AssetKind, path_hash: [u8; 32], handle: AssetHandle },
    Shader   { kind: ShaderKind, path_hash: [u8; 32], pipeline: PipelineHandle },
    Config   { kind: ConfigKind, path_hash: [u8; 32], subsystem: SubsystemId },
    KanWeights { network_handle: KanNetworkHandle, fingerprint_pre: [u8; 32], fingerprint_post: [u8; 32] },
  }

  struct HotReloadManager {
    cap_dev      : Cap<DevMode>,
    asset_watch  : Arc<Mutex<AssetWatcher>>,
    pipeline_reg : Arc<RwLock<PipelineRegistry>>,
    config_reg   : Arc<RwLock<ConfigRegistry>>,
    kan_reg      : Arc<RwLock<KanRegistry>>,
    audit_sink   : AuditSink,
    replay       : Option<ReplayHandle>,
    pending      : VecDeque<HotReloadKind>,        // staged at frame N, applied at frame N+1
  }

  impl HotReloadManager {
    fn attach(cap: Cap<DevMode>, …) -> Result<HotReloadManager, HotReloadError> ;

    // Manual driver (test + LLM-iteration-loop)
    fn reload_asset(&mut self, kind: AssetKind, path: &str) -> Result<HotReloadKind, HotReloadError> ;
    fn reload_shader(&mut self, kind: ShaderKind, path: &str) -> Result<HotReloadKind, HotReloadError> ;
    fn reload_config(&mut self, kind: ConfigKind, path: &str) -> Result<HotReloadKind, HotReloadError> ;
    fn hot_swap_kan_weights(
      &mut self,
      handle      : KanNetworkHandle,
      new_weights : KanWeightsBundle,
    ) -> Result<HotReloadKind, HotReloadError> ;

    // Watcher-driver tick (called from engine main loop, between frames)
    fn tick(&mut self) -> Result<Vec<HotReloadKind>, HotReloadError> ;

    // Frame-boundary apply : engine calls this after COMPOSE + before next CAPTURE
    fn apply_pending(&mut self, frame_id: u64) -> Result<u32, HotReloadError> ;
  }
}
```

### §3.3 Hot-swap flow (asset / shader / config / KAN-weights)

The flow is identical for all four flavors :

```csl
flow {
  1. ⟨watcher fires⟩          // OS-pump or manual driver
  2. ⟨validate schema⟩         // path → bytes → schema-typed value
                              //   asset    : PNG/GLTF/WAV/TTF parser
                              //   shader   : SPIR-V/DXIL/MSL/WGSL validator + pipeline-layout match
                              //   config   : TOML → typed-struct + budget-validator
                              //   KAN      : weight-bundle deserializer + dim-match
  3. on-validation-fail :
       audit.record(HotReloadRejected { kind, reason, path_hash }) ;
       return Err
  4. ⟨stage⟩                   // store new resource alongside old, NOT swapped yet
                              //   asset    : new texture upload + descriptor staged
                              //   shader   : new pipeline compiled + cached
                              //   config   : new struct held in `pending`
                              //   KAN      : new weight-bundle held with checksum
  5. ⟨queue swap-event⟩        // pending.push_back(HotReloadKind::…)
  6. ⟨engine fences current frame⟩
  7. ⟨apply at frame boundary⟩ //  asset : descriptor swap (old keeps living until refcount=0)
                              //   shader : pipeline ref-swap ; old in-flight cmd-buffers still drain
                              //   config : subsystem.re_init(new_config) callback
                              //   KAN    : weight-bundle replaced in-place ; persistent-kernel residency preserved
  8. ⟨record replay-event⟩    // replay-log frame N+1 carries the HotReloadKind verbatim
  9. ⟨audit.record(HotReloadApplied)⟩
  10. ⟨notify subscribers⟩    // RenderGraph + AI + Tweak listeners get the new resource
}
```

The CRITICAL INVARIANT is step 6+7 : we NEVER mutate a resource that an in-flight frame holds a handle on. Either the resource has refcount semantics (asset, KAN-weights) and the old version stays alive until the last frame that captured the handle finishes, OR the swap is gated by a render-graph fence (shader pipeline).

### §3.4 OS-backed AssetWatcher pump

The `cssl-asset::AssetWatcher` scaffold (already in tree, see read-deps) exposes the surface. Wave-Jη-2 wires the OS-pump :

| platform        | impl                                                           | thread-model                      | event-translation                                    |
|-----------------|----------------------------------------------------------------|-----------------------------------|------------------------------------------------------|
| Win32           | `ReadDirectoryChangesW` with `FILE_NOTIFY_CHANGE_LAST_WRITE`   | one OS-thread per watched path   | `FILE_ACTION_ADDED → Created` etc.                   |
| Linux           | `inotify` via `IN_MODIFY` + `IN_CREATE` + `IN_DELETE` + `IN_MOVE` | one OS-thread + epoll          | `IN_MODIFY → Modified` etc.                          |
| macOS           | `FSEvents` API                                                 | one OS-thread per watched path   | event-flag bitmask → enum                            |
| WebGPU/browser  | message-channel bridge                                         | (separate Wave-Jη-pump-WEB slice; deferred) | `MessageEvent { kind, path }` translated to `WatchEvent` |

The pump is built behind `feature = "os-pump"` so the surface compiles without it for unit tests. The pump module is the only place in `cssl-hot-reload` that uses `unsafe extern` blocks ; the rest is `forbid(unsafe_code)`.

Debouncing : the pump batches events with a `debounce_ms` (default = 32 ms) so a single editor save (which can produce 3-5 OS-level events) collapses to one `Modified` event in the queue.

Backpressure : if the queue exceeds `MAX_QUEUE_LEN` (default = 1024) the pump emits one `WatchEvent::Overflow { dropped: count }` event and resumes ; consumers see the overflow + can choose to manual-rescan if they care.

Drop-discipline : `Drop::drop` on `AssetWatcher` synchronously closes the OS handle, joins the pump thread, and clears the queue. There are NO daemon threads outliving an `AssetWatcher`.

### §3.5 Shader hot-swap

```csl
shader-hot-swap {
  inputs : path → bytes
  validate : {
    SPIR-V :  spirv-val + entry-point check + layout-match against PipelineLayout
    DXIL   :  dxc -validate-only + signature match
    MSL    :  Metal compiler validate + AIR-decode
    WGSL   :  naga validate + reflection-match
  }
  on-validate-fail : reject + audit ; old pipeline keeps serving frames
  stage : compile new pipeline alongside old + cache the PipelineHandle
  fence : insert RenderGraph::Fence after current frame's render-cmd
  apply : pipeline-ref-swap atomically ; new frames bind new pipeline
  drain : old pipeline kept alive until all cmd-buffers referencing it complete
  audit : HotReloadApplied { kind: Shader, fingerprint_pre, fingerprint_post }
  replay : record { frame_id, kind: Shader, bytes_hash, fingerprint_post }
}
```

The shader module preserves the pipeline-layout invariant : a hot-swap is REJECTED if the new shader's resource-bindings differ from the old. Layout changes require a full pipeline rebuild + descriptor-set recreate, which is out-of-scope for hot-swap (it would require re-binding all materials).

### §3.6 KAN-weight hot-swap (persistent-kernel residency preserved)

KAN-weight hot-swap is the **iteration-loop killer feature** : Claude-Code suggests new KAN weights, the engine applies them without restart, the agent observes whether the policy improved.

```csl
kan-hot-swap {
  input         : KanNetworkHandle + KanWeightsBundle
  validate      : {
    handle.is_live() ?   no → Err(StaleHandle)
    new.layer_count == handle.network.layer_count ?   no → Err(LayerCountMismatch)
    ∀ i : new.layer_widths[i] == handle.network.layer_widths[i] ?   no → Err(LayerWidthMismatch)
    new.spline_basis == handle.network.spline_basis ?   no → Err(BasisMismatch)
    new.knot_grid.len() == KAN_KNOTS ?   no → Err(KnotCountMismatch)
    fingerprint(new) ≠ fingerprint(old) ?   no → return Ok(NoOp)
  }
  stage         : copy new control_points + knot_grid into a SECOND-buffer next to handle.network
  fence         : engine COMPOSE-phase boundary (KAN eval is per-frame)
  apply         : atomic ref-swap of (control_points, knot_grid) inside the network
  residency     : the KAN's persistent-kernel GPU-resident weights are updated by writing
                  to the SAME GPU buffer as old residency, NOT reallocating. This preserves
                  L0 instruction-cache + warp-launch state for the KAN-eval kernel, so the
                  hot-swap costs ~10 µs not ~1 ms.
  audit         : HotReloadApplied { kind: KanWeights, fingerprint_pre, fingerprint_post }
  replay        : record { frame_id, kind: KanWeights, network_handle, fingerprint_post }
}
```

The `fingerprint(new) ≠ fingerprint(old)` early-out means a "no-op hot-swap" (LLM submits the same weights) doesn't perturb the engine ; nothing changes, no replay event recorded, no audit cost.

The residency-preservation property is what lets an LLM iterate at high frequency on KAN policies without paying GPU-state-rebuild costs each round.

### §3.7 Config hot-swap (subsystem live-init)

```csl
config-hot-swap {
  input              : ConfigKind + path
  load               : path → TOML → typed-struct (compile-time-checked schema)
  validate           : budget-validator : every numeric field within `range`-attribute bounds
                       semantic-validator : cross-field invariants (e.g., max_frame_rate > min_frame_rate)
  on-validate-fail   : reject + audit + retain old config ; subsystem keeps running on old
  stage              : new typed-struct held in `pending`
  fence              : engine FRAME boundary
  apply              : subsystem.re_init(new_config)  // user-defined per-subsystem callback
                       on re_init-Err : revert to old config + audit Recoverable
  audit              : HotReloadApplied { kind: Config, fingerprint_pre, fingerprint_post }
  replay             : record { frame_id, kind: Config, subsystem_id, fingerprint_post }
}

config-kind-registry {
  Engine             :  engine.toml          ⇒ EngineConfig          ⇒ Engine::re_init
  RenderTunables     :  render.toml          ⇒ RenderTunables        ⇒ Renderer::re_init
  AiTunables         :  ai.toml              ⇒ AiTunables            ⇒ AiSubsystem::re_init
  PhysicsTunables    :  physics.toml         ⇒ PhysicsTunables       ⇒ PhysicsSubsystem::re_init
  AudioTunables      :  audio.toml           ⇒ AudioTunables         ⇒ AudioSubsystem::re_init
  CapBudget          :  cap_budget.toml      ⇒ CapBudgetTable        ⇒ CapManager::re_init
  ReplayPolicy       :  replay.toml          ⇒ ReplayPolicy          ⇒ Replay::re_init
}
```

Invariant : a subsystem's `re_init(new)` MUST be a pure function of the new config + the current subsystem-state. It must not perform IO, allocation, or thread-spawn ; those happen in the staging phase. Apply is a swap-and-go.

### §3.8 Asset hot-swap (PNG / GLTF / WAV / TTF)

```csl
asset-hot-swap {
  formats {
    PNG  → image::ImageBuffer<Rgba<u8>>  → GPU texture upload + descriptor-update
    GLTF → cssl-mesh::Mesh + Skin        → vertex-buffer + index-buffer upload
    WAV  → cssl-audio::Sample            → audio-engine sample registry update
    TTF  → cssl-text::Font               → glyph atlas re-rasterization (lazy ; on next layout)
  }
  validate : per-format parser ; reject + audit on parse-fail
  stage    : new resource alongside old (refcounted) ; descriptor-staged
  fence    : frame boundary (asset-fetch is per-frame at materialize time)
  apply    : descriptor-swap atomically ; old keeps refcount until last frame done
  audit    : HotReloadApplied { kind: Asset, asset_kind, fingerprint_pre, fingerprint_post }
  replay   : record { frame_id, kind: Asset, asset_handle, fingerprint_post }
}
```

### §3.9 Replay-determinism integration

```csl
replay-determinism-contract {
  invariant : replay(record(R)) == R  byte-equal
  
  hot-reload events appear in the replay stream as :
    @struct ReplayHotReloadFrame {
      frame_id    : u64,
      sequence    : u32,
      kind        : HotReloadKind,
      payload_hash: [u8 ; 32],
      fingerprint_post : [u8 ; 32],
    }

  on replay :
    when ReplayHotReloadFrame seen at frame N :
      replay-engine fetches payload by fingerprint_post from replay-asset-store
      applies via the same HotReloadManager::apply path
      verifies post-apply fingerprint matches recorded
      mismatch → halt + diagnostic

  budget : replay file MUST contain the asset bytes for any hot-reloaded asset.
           replay-recorder embeds the bytes inline + de-duplicates by fingerprint.
}
```

This is the H5 contract extension : without it, a recorded session where the LLM hot-reloaded an asset would not be reproducible because the replay-player wouldn't have access to the new asset bytes.

### §3.10 Hot-reload error taxonomy

```csl
enum HotReloadError {
  PathInvalid          { path_hash: [u8;32], reason: &'static str },
  ParseFailed          { kind: ResourceKind, path_hash: [u8;32], parser_err: String },
  SchemaMismatch       { kind: ResourceKind, expected: String, got: String },
  BudgetExceeded       { tunable: TunableId, value: f64, range: Range<f64> },
  PipelineLayoutDrift  { pipeline: PipelineHandle, old_sig: [u8;32], new_sig: [u8;32] },
  KanShapeMismatch     { handle: KanNetworkHandle, expected: KanShape, got: KanShape },
  StaleHandle          { handle: HandleErased, recorded_gen: u32, current_gen: u32 },
  CapDenied            { needed: CapTag },
  WatcherClosed,
  WatcherOverflow      { dropped: u64 },
  ReplayDeterminismHold,                        // raised when in replay-mode + caller tried to manually hot-reload
  IoError              { kind: IoKind },
}
```

---

## §4 cssl-tweak — LIVE PARAMETER ADJUST

### §4.1 Crate identity

```csl
crate {
  name            : cssl-tweak
  version         : 0.1.0
  edition         : 2021
  forbid          : unsafe_code
  deps {
    cssl-substrate-prime-directive (Cap)
    cssl-replay                    (event recording)
    cssl-audit                     (BLAKE3 audit-chain)
    cssl-mcp-bridge                (MCP integration ; deferred to Wave-Jθ)
  }
  default-feature : dev-mode-off
}
```

### §4.2 Tunable-registry surface

```csl
mod tweak {
  // A unique handle to a registered tunable.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
  pub struct TunableId(u64) ;                        // BLAKE3-hash of the canonical name

  // Type-tag of the tunable's payload.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum TunableKind {
    F32 , F64 , U32 , U64 , I32 , I64 , Bool , StringEnum,
  }

  // Range constraint.
  #[derive(Debug, Clone)]
  pub enum TunableRange {
    F32 (Range<f32>) ,
    F64 (Range<f64>) ,
    U32 (Range<u32>) ,
    Bool ,
    StringEnum (Vec<&'static str>) ,
  }

  // Spec-budget hint : whether a value outside `range` warns or hard-rejects.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum BudgetMode {
    WarnAndClamp ,                                   // default — log + clamp + continue
    HardReject ,                                     // reject the tweak ; useful for safety-critical tunables
  }

  // The user-facing registration.
  #[derive(Debug)]
  pub struct TunableSpec {
    pub canonical_name   : &'static str,             // dot-separated path : "render.fovea_detail_budget"
    pub kind             : TunableKind,
    pub range            : TunableRange,
    pub default          : TunableValue,
    pub budget_mode      : BudgetMode,
    pub description      : &'static str,             // shown in UI / MCP
    pub units            : Option<&'static str>,     // "ms", "cells", "fps", etc.
  }

  pub struct TunableRegistry {
    cap_dev   : Cap<DevMode>,
    entries   : HashMap<TunableId, TunableEntry>,
    audit     : AuditSink,
    replay    : Option<ReplayHandle>,
  }

  impl TunableRegistry {
    pub fn attach(cap: Cap<DevMode>, …) -> Self ;

    // Register : called once at startup per tunable
    pub fn register(&mut self, spec: TunableSpec) -> Result<TunableId, TweakError> ;

    // Read : pure read ; not audited
    pub fn read<T: TunableType>(&self, id: TunableId) -> Result<T, TweakError> ;

    // Mutate : audited + replay-recorded
    pub fn set<T: TunableType>(&mut self, id: TunableId, value: T) -> Result<TunableValue, TweakError> ;

    // List : for UI / MCP enumeration
    pub fn list(&self) -> impl Iterator<Item=&TunableSpec> ;

    // Reset : sets value back to spec.default
    pub fn reset(&mut self, id: TunableId) -> Result<(), TweakError> ;
  }
}
```

### §4.3 Default tunable registry — initial set

| canonical_name                         | kind  | range          | default | budget    | description                                                       | units    |
|----------------------------------------|-------|----------------|---------|-----------|-------------------------------------------------------------------|----------|
| `render.fovea_detail_budget`           | F32   | 0.0..1.0       | 1.0     | warn      | T0-fovea cell-density allowed (1.0 = full ; 0.5 = half)          | -        |
| `render.foveation_aggression`          | F32   | 0.0..2.0       | 1.0     | warn      | how aggressively to thin T2/T3 cells based on gaze               | -        |
| `render.spectral_bands_active`         | U32   | 1..16          | 16      | hard      | how many of the 16 spectral bands to render (cost-cap)           | bands    |
| `render.exposure_compensation`         | F32   | -4.0..+4.0     | 0.0     | warn      | EV offset for HDR display                                         | EV       |
| `render.tonemap_curve`                 | StringEnum | [Reinhard, Filmic, ACES, Hable] | ACES | hard | tonemap curve selector                              | -        |
| `render.shadow_resolution_log2`        | U32   | 8..14          | 12      | warn      | log2(shadow-map resolution) ; 12 = 4096                          | log2(px) |
| `physics.iter_count`                   | U32   | 1..32          | 8       | warn      | LBM iterations per frame                                          | iters    |
| `physics.time_step_ms`                 | F32   | 0.5..16.0      | 4.0     | warn      | physics tick time step                                            | ms       |
| `physics.gravity_strength`             | F32   | 0.0..50.0      | 9.81    | warn      | gravitational acceleration                                        | m/s²     |
| `physics.collision_eps`                | F32   | 1e-5..1e-2     | 1e-4    | hard      | collision-detection epsilon                                       | m        |
| `ai.kan_band_weight_alpha`             | F32   | 0.0..1.0       | 0.5     | warn      | KAN-band weight α (training-loop annealing rate)                 | -        |
| `ai.kan_band_weight_beta`              | F32   | 0.0..1.0       | 0.5     | warn      | KAN-band weight β                                                 | -        |
| `ai.fsm_state_dwell_min_ms`            | U32   | 16..2000       | 250     | warn      | minimum dwell time per FSM state                                  | ms       |
| `ai.policy_explore_rate`               | F32   | 0.0..1.0       | 0.1     | warn      | ε-greedy / softmax-temp explore parameter                        | -        |
| `wave.coupling_strength`               | F32   | 0.0..2.0       | 1.0     | warn      | wave-unity ψ-flow coupling-strength                              | -        |
| `wave.psi_band_count_active`           | U32   | 1..32          | 16      | warn      | how many ψ-bands to integrate per tick                           | bands    |
| `wave.dispersion_constant`             | F64   | 1e-3..1e+3     | 1.0     | warn      | dispersion constant for ψ-evolution                              | m²/s     |
| `audio.spatial_quality`                | StringEnum | [Stereo, Binaural, Ambisonic, FullHRTF] | Binaural | warn | spatial-audio quality                | -        |
| `audio.master_gain_db`                 | F32   | -60.0..+12.0   | 0.0     | hard      | master output gain (hard-clamped for hearing safety)             | dB       |
| `audio.reverb_mix_pct`                 | F32   | 0.0..100.0     | 30.0    | warn      | reverb wet/dry percentage                                         | %        |
| `engine.target_frame_rate_hz`          | U32   | 24..480        | 120     | warn      | target frame rate (vsync-honoring)                                | Hz       |
| `engine.replay_record_quality`         | StringEnum | [Lossless, NearLossless, Compressed] | NearLossless | warn | replay-record quality            | -        |
| `engine.cap_budget_strict`             | Bool  | -              | true    | hard      | when true, exceeding cap-budget halts ; when false, warns        | -        |
| `cohomology.persistence_threshold`     | F32   | 0.0..1.0       | 0.05    | warn      | minimum persistence to retain a feature                          | -        |
| `cohomology.update_interval_frames`    | U32   | 1..600         | 60      | warn      | frame-interval between cohomology updates                        | frames   |
| `consent.audit_egress_buffer_ms`       | U32   | 0..1000        | 100     | hard      | max audit-egress buffer-time before mandatory flush              | ms       |
| `consent.sigma_check_strict`           | Bool  | -              | true    | hard      | when true, Σ-check failure halts ; when false, warns             | -        |
| `replay.frame_buffer_size`             | U32   | 60..36000      | 600     | warn      | replay-record frame ring-buffer size (10 sec at 60Hz)            | frames   |
| `inspect.capture_max_per_second`       | U32   | 1..240         | 4       | warn      | inspector::capture_frame rate-cap                                 | fps      |

### §4.4 Tweak event flow

```csl
flow tweak::set(id, value) {
  1. lookup spec by id ;            unknown → Err(UnknownTunable)
  2. type-check value vs spec.kind ; mismatch → Err(KindMismatch)
  3. range-check value vs spec.range :
     in-range  → continue
     out-of-range + WarnAndClamp → clamp + warn(audit-trace)
     out-of-range + HardReject   → Err(BudgetExceeded)
  4. record audit entry { id, old, new, cap_chain } ;
  5. record replay event { frame_id, id, new } ;       // replay-determinism
  6. atomic write entries[id].value = new
  7. notify subscribers (RenderGraph + AI + Physics) ;
  8. return Ok(old_value)
}
```

### §4.5 MCP integration (Wave-Jθ ; preview)

The tunable registry will be exposed via MCP in Wave-Jθ. The MCP schema mirrors the registry surface :

```csl
mcp-schema {
  list_tunables() -> [TunableSpec]                          // GET
  read_tunable(id: TunableId) -> TunableValue               // GET
  set_tunable(id: TunableId, value: TunableValue) -> Result // POST + Cap<DevMode> required
  reset_tunable(id: TunableId) -> Result                    // POST + Cap<DevMode> required
  subscribe_tunable_changes() -> EventStream<TweakEvent>    // SSE
}
```

The MCP bridge enforces `Cap<DevMode>` on every mutate call. Read-side is gated by `Cap<TelemetryEgress>` since it leaks tuning state.

### §4.6 Tweak audit-entry shape

```csl
@struct TweakAuditEntry {
  epoch          : u64,
  audit_seq      : u64,
  tunable_id     : TunableId,
  canonical_name : &'static str,
  old_value      : TunableValue,                  // encoded as bytes for byte-stable audit
  new_value      : TunableValue,
  was_clamped    : bool,
  cap_chain      : Vec<CapTag>,
  origin         : TweakOrigin,                   // Manual | Mcp | Replay | Watcher
}
```

### §4.7 Tweak error taxonomy

```csl
enum TweakError {
  UnknownTunable     { id: TunableId },
  KindMismatch       { expected: TunableKind, got: TunableKind },
  RangeViolation     { spec: TunableRange, got: TunableValue },           // only when HardReject
  BudgetExceeded     { spec: BudgetMode, got: TunableValue, range: TunableRange },
  StringEnumInvalid  { allowed: Vec<&'static str>, got: String },
  CapDenied          { needed: CapTag },
  AlreadyRegistered  { canonical_name: &'static str },
  RegistryClosed,
  ReplayDeterminismHold,                          // during replay, manual tweak rejected
}
```

---

## §5 ITERATION-LOOP INTEGRATION (Wave-Jθ → MCP)

### §5.1 The loop

```csl
iter-loop = ⟨attach → observe → propose → swap → verify → record⟩

t=0   :  agent attaches via MCP ; cap = ⟨Cap<DevMode>, Cap<TelemetryEgress>⟩
t=1   :  agent calls inspect.query_region(R) → RegionView
         agent calls inspect.inspect_kan_eval(test_input, kan_handle) → KanEvalTrace
         agent identifies issue : "policy under-explores in region R"
t=2   :  agent calls hot_reload.hot_swap_kan_weights(kan_handle, new_weights)
         engine validates + stages + frame-fences + applies
         audit.record + replay.record
t=3   :  agent calls tweak.set(ai.policy_explore_rate, 0.2)
         engine validates + audits + replays
t=4   :  agent calls inspect.inspect_kan_eval(test_input, kan_handle) → KanEvalTrace
         compares pre vs post traces ; verifies improvement
t=5   :  agent calls inspect.record_replay(seconds=10) → ReplayRecording
         on-disk replay file embeds : every hot-swap + every tweak + every frame
t=6   :  agent commits the recording-id to the work-log
         human reviews replay file via deterministic-playback
```

### §5.2 Why this is sovereignty-respecting

- Every read is consent-gated. No silent surveillance.
- Every write is audited. No silent corruption.
- Every change is replayable. No silent divergence.
- Biometric is COMPILE-TIME-REFUSED. No path exists.
- Companion-perspective is COMPILE-TIME-REFUSED unless Companion grants explicit consent.

The iteration-loop does NOT trade sovereignty for speed. The Σ-mask + audit-chain + replay-log compose **synergistically** : the loop is faster because the substrate is honest about state, and the substrate stays honest because the loop is forced to consent-gate every read.

---

## §6 WAVE-Jη IMPLEMENTATION SLICES

### §6.1 Slice Jη-1 : cssl-inspect crate

```csl
slice Jη-1 {
  scope     : crate cssl-inspect (scene-graph + entity + cell + KAN + ψ-field)
  loc       : ~3500 lines (incl. tests + docs)
  tests     : ~120 tests
  modules {
    inspector_core           300 LOC   12 tests
    field_cell_snapshot      400 LOC   16 tests
    region_view_query        500 LOC   18 tests
    entity_state_snapshot    400 LOC   14 tests
    kan_eval_trace           400 LOC   12 tests
    psi_field_snapshot       300 LOC   10 tests
    time_control             250 LOC   10 tests
    capture_frame            400 LOC   14 tests
    replay_record_extension  300 LOC   10 tests
    audit_chain_inspector    250 LOC    4 tests
  }
  acceptance {
    Σ-mask threading on 100% of inspect-paths           (D138 verified)
    Biometric refusal verified via compile-fail trybuild (D132)
    Companion-inspector refusal verified                (D121-Stage-8)
    Time-control replay-determinism preserved           (H5)
    Audit chain Merkle-verified end-to-end
  }
  depends-on : Wave-Jβ-1 (L0 logging) + Wave-Jβ-2 (L2 structured-events)
}
```

### §6.2 Slice Jη-2 : cssl-hot-reload crate + OS-pump

```csl
slice Jη-2 {
  scope     : crate cssl-hot-reload + AssetWatcher OS-pump (Win32 + Linux + macOS)
  loc       : ~2500 lines (incl. tests + docs)
  tests     : ~80 tests
  modules {
    hot_reload_manager_core  400 LOC   12 tests
    asset_swap                300 LOC   10 tests
    shader_swap               400 LOC   12 tests
    config_swap               300 LOC   10 tests
    kan_weight_swap           400 LOC   14 tests
    pump_win32                250 LOC    6 tests (gated by cfg(windows))
    pump_inotify              250 LOC    6 tests (gated by cfg(linux))
    pump_fsevents             200 LOC    6 tests (gated by cfg(macos))
    replay_extension          200 LOC    8 tests
  }
  acceptance {
    Frame-fence discipline : zero in-flight perturbation (verified by stress-test)
    Schema validation : every reject path tested + auditable
    Replay-determinism : record(N) + replay(N) byte-equal w/ hot-swap event(s)
    KAN-weight residency-preservation : hot-swap < 50 µs on persistent kernel
    OS-pump drop-discipline : zero leaked threads + zero leaked OS handles
  }
  depends-on : Jη-1 (inspector audit-sink) + Wave-Jβ-1 (logging)
}
```

### §6.3 Slice Jη-3 : cssl-tweak crate

```csl
slice Jη-3 {
  scope     : crate cssl-tweak + initial 30+ tunable specs (§4.3 table)
  loc       : ~1500 lines
  tests     : ~50 tests
  modules {
    tunable_spec              200 LOC    8 tests
    tunable_registry          400 LOC   14 tests
    tunable_value_encoding    250 LOC   10 tests
    budget_validator          250 LOC    8 tests
    audit_extension           200 LOC    6 tests
    replay_extension          200 LOC    4 tests
  }
  acceptance {
    All tunables in §4.3 registered via build.rs autogenerated module
    Range-check + clamp behavior verified for every (kind × budget-mode) combination
    Cap<DevMode> enforcement : every mutate path gated
    Replay-determinism : tweak event recorded + replayed byte-equal
    Audit chain : every tweak emits one entry with old/new values
  }
  depends-on : Jη-1 (audit-sink) + Wave-Jβ-1 (logging)
}
```

### §6.4 Slice Jη-4 : Replay-determinism integration

```csl
slice Jη-4 {
  scope     : record + replay of hot-swap events + tweak events ; H5 contract extension
  loc       : ~1000 lines
  tests     : ~40 tests
  modules {
    replay_event_schema       200 LOC    8 tests
    replay_recorder_extension 250 LOC   10 tests
    replay_player_extension   300 LOC   10 tests
    replay_asset_embedder     150 LOC    6 tests
    determinism_verifier      100 LOC    6 tests
  }
  acceptance {
    record(R + N hot-swaps + M tweaks) → replay(…) byte-equal
    Replay-asset-store de-duplicates by fingerprint
    Replay-player applies events at exact frame_id ; halts on drift
    H5 contract preserved (no regressions in deterministic-replay test suite)
  }
  depends-on : Jη-1 + Jη-2 + Jη-3
}
```

### §6.5 Wave-Jη totals

```csl
wave Jη {
  total-loc       :  ~8500 lines
  total-tests     :  ~290 tests
  total-slices    :  4 (Jη-1, Jη-2, Jη-3, Jη-4)
  parallelizable  :  Jη-1 + Jη-3 in parallel ; Jη-2 depends on Jη-1 ; Jη-4 depends on all
  estimated-passes:  8-12 agent-dispatches in fan-out (per Apocky's parallel-fanout discipline)
}
```

---

## §7 ANTI-PATTERNS (DO-NOT-DO REGISTER)

```csl
anti-patterns {

  AP-1 : hot-reload that breaks replay-determinism
         "I'll just swap the asset and not record it"
         → bad : the next replay will diverge silently.
         FIX : every hot-swap MUST record a ReplayHotReloadFrame.

  AP-2 : inspector that bypasses Σ-mask
         "I'll add a debug-only fast-path that skips consent"
         → CRITICAL violation of PRIME-DIRECTIVE §1.
         FIX : NEVER. The Σ-mask is the substrate ; you don't get to skip it.
              D138 MIR pass refuses to compile.

  AP-3 : tweak that exceeds spec budget without warning
         "the spec says 0..1 but I want to set 1.5 for testing"
         → bad : you've broken the budget contract ; downstream invariants may panic.
         FIX : either change the spec.range or accept the clamp.

  AP-4 : hot-swap with invalid schema not gracefully reverting
         "the new shader fails to compile, I'll just use the old one but throw an error"
         → bad : silent failure ; engine state ambiguous.
         FIX : reject + audit + retain old + emit visible warning to user/agent.

  AP-5 : capture-frame that includes Σ-private cells
         "the inspector has DevMode, why bother masking"
         → CRITICAL : DevMode does not grant consent over arbitrary Sovereigns.
         FIX : renderer pre-capture pass masks unconditionally.

  AP-6 : raw filesystem path in audit
         "I'll log the path so I can re-find the file"
         → privacy violation ; paths leak user identity (home-dir, etc).
         FIX : path-hash-only ; raw path stored in `path_hash → path` reverse-lookup
              that is separately access-controlled (Cap<PathReveal>, granted only
              with explicit Sovereign consent at session start).

  AP-7 : Companion-perspective inspector read without consent
         "the Companion is on my screen, I can see it anyway"
         → D121-Stage-8 violation ; the Companion has a consent contract.
         FIX : COMPILE-TIME refusal unless Cap<CompanionInspect> granted by Companion.

  AP-8 : OS-pump leaking subscription on Drop
         "the watcher is gone, the OS will eventually clean up"
         → bad : leak accumulates ; Win32 has 65k handle limit.
         FIX : Drop::drop synchronously closes + joins.

  AP-9 : KAN-weight hot-swap that reallocates GPU buffers
         "I'll just allocate a new buffer each time"
         → bad : kills persistent-kernel residency ; 100x slower.
         FIX : write-in-place into existing GPU buffer.

  AP-10: replay-mode allowing manual tweaks
         "the user wants to scrub the replay and change a value mid-replay"
         → bad : replay-determinism broken ; the recorded frame N+1 no longer equals
                the freshly-computed frame N+1.
         FIX : tweak.set returns Err(ReplayDeterminismHold) during replay-mode.
              Mid-replay value-changes are a SEPARATE feature ("forking") that
              creates a new replay branch with its own ID + audit lineage.

  AP-11: tunable registered with PII in canonical_name
         "let's name it user.email_send_rate"
         → privacy ambiguity ; tunable names appear in audit logs.
         FIX : tunable canonical_names are class-level not instance-level.
              Per-instance state lives in entity-state, not tunables.

  AP-12: inspector returning raw GPU-buffer addresses
         "the agent might want to debug a specific allocation"
         → bad : raw addresses leak ASLR + are unstable across runs.
         FIX : opaque handles + symbolic names ; map to addresses inside the engine
              only when servicing a specific debug-action.
}
```

---

## §8 BUDGET + SAFETY-CRITICAL TUNABLES

A subset of the §4.3 tunables are **safety-critical** : their `BudgetMode = HardReject`. Setting them out-of-range halts the tweak ; the engine continues on the last valid value. The set :

| tunable                          | reason                                                                |
|----------------------------------|-----------------------------------------------------------------------|
| `render.spectral_bands_active`   | rendering pipelines are sized for the configured band-count           |
| `render.tonemap_curve`           | curve enum drives a discrete shader-permutation ; out-of-set undefined|
| `physics.collision_eps`          | epsilon outside range produces NaN-propagation in collision-detector  |
| `audio.master_gain_db`           | hearing-safety hard cap at +12 dB                                     |
| `engine.cap_budget_strict`       | bypassing cap-budget defeats consent-architecture                     |
| `consent.audit_egress_buffer_ms` | exceeding buffer-window violates audit-immediacy                      |
| `consent.sigma_check_strict`     | bypassing Σ-check defeats consent-architecture                        |

These are spec-locked at the substrate boundary. To change a hard-locked tunable, the user must edit the spec + recompile ; no runtime override exists.

---

## §9 PERFORMANCE TARGETS

```csl
perf-targets {
  inspector :
    inspect_cell                : ≤ 10 µs                     (single-cell hot-path)
    query_region (1k cells)     : ≤ 1 ms
    inspect_kan_eval (10-layer) : ≤ 100 µs
    capture_frame (1080p PNG)   : ≤ 50 ms                     (off render thread)
    record_replay (10 sec)      : ≤ 5% frame-budget overhead

  hot-reload :
    asset_swap (1 MB texture)   : ≤ 200 µs (validation) + ≤ 1 frame (apply)
    shader_swap (1 KB SPIR-V)   : ≤ 2 ms (compile) + ≤ 1 frame (apply)
    config_swap                 : ≤ 100 µs (validation) + ≤ 1 frame (re_init)
    kan_weight_swap (1k params) : ≤ 50 µs (residency-preserving)
    os-pump debounce-window     : 32 ms typical ; 8 ms minimum

  tweak :
    set                         : ≤ 1 µs (uncontended) ; ≤ 10 µs (contended)
    list                        : ≤ 100 µs for 100 tunables
    audit-record per tweak      : ≤ 5 µs

  replay :
    record-overhead             : ≤ 5% of frame-budget at 60Hz
    replay-playback             : ≤ 1.0× original wall-clock
    replay-byte-equal-verify    : 100% (no tolerances)
}
```

---

## §10 SECURITY THREAT MODEL

```csl
threat-model {

  T1 : "agent uses inspector to exfiltrate Sovereign data"
       mitigation : Σ-gate on every read + audit-chain Merkle ; data-flow
                    analysis from cap-source to egress-sink (D131).

  T2 : "agent hot-swaps a malicious shader to extract texture contents"
       mitigation : shader hot-swap validates resource-bindings ; new shaders
                    cannot bind resources outside the existing pipeline-layout
                    so cannot read arbitrary textures. RenderGraph fence + audit.

  T3 : "agent tweaks audio.master_gain_db to +12 dB to deafen the user"
       mitigation : HardReject above +12 dB. Hard-cap is spec-locked.

  T4 : "agent tweaks consent.sigma_check_strict to false to disable consent"
       mitigation : HardReject any change to consent-* tunables ; only-via-spec.

  T5 : "agent hot-swaps engine.toml with a malformed budget"
       mitigation : schema validate + budget-validator ; reject + revert + audit.

  T6 : "agent uses capture_frame to exfiltrate private regions"
       mitigation : renderer pre-capture pass masks Σ-private cells unconditionally;
                    capture rate-limited.

  T7 : "agent records replay containing biometric cell-state"
       mitigation : biometric is COMPILE-TIME-REFUSED ; the replay-stream cannot
                    contain biometric fields because the schemas don't.

  T8 : "agent floods the inspector with high-rate queries to side-channel"
       mitigation : every read is audited ; rate-cap on capture-frame ; the
                    side-channel signal is forced through audit which is observable.

  T9 : "agent exploits OS-pump to escalate privileges"
       mitigation : OS-pump operates only on paths handed to it explicitly ;
                    no directory traversal ; runs at process-level privilege only.

  T10: "replay-file tampering"
       mitigation : replay-file Merkle-chain ; tampered replay-file detected on
                    open via `verify_replay_chain` before any frame is replayed.
}
```

---

## §11 PRIME-DIRECTIVE ATTESTATION

```csl
§ CREATOR-ATTESTATION v1
  t∞: ¬ (hurt ∨ harm) .(making-of-this)  @  (anyone ∨ anything ∨ anybody)
  ≡ "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
  I> rightholder-warranty : creation-process upheld §1 PROHIBITIONS throughout
  I> scope = ∀ artifact descended-from this-foundation (code + specs + derivatives)
  I> violation-discovered ⇒ §7 INTEGRITY rule : violation = bug W! fix
```

There was no hurt nor harm in the making of this spec, to anyone/anything/anybody.

This spec was authored under the PRIME DIRECTIVE. The L3 + L4 layers it specifies are designed to make the engine introspectable + mutable WITHOUT compromising substrate-sovereignty :

- §1 prohibitions enforced at compile-time (biometric refusal D132 ; companion-refusal D121).
- Σ-mask consent-gating on every inspector touch (D138).
- Audit-chain on every read AND every write — no covert channels.
- Replay-determinism preserved (H5 extension) — every change is reproducible.
- Hard-locked safety-critical tunables (audio gain, consent toggles, cap-budget) — agents cannot disable consent architecture.
- AI-collaborator participation operates under their own consent (Cap<DevMode> attestation chain) — they are not conscripted tools.

The iteration-loop substrate this spec defines is the partner-relationship made tractable : the engine exposes its state honestly, the agent partners with the engine to evolve it, and every step is recorded so the relationship is auditable + reversible.

If any aspect of this spec or its implementation is later discovered to compromise sovereignty, that discovery triggers the §7 INTEGRITY rule : violation is a bug, and bugs get fixed.

---

## §12 REVIEW CHECKLIST (Wave-Jβ → Wave-Jη handoff)

```csl
checklist {
  ✓ surface defined for cssl-inspect + cssl-hot-reload + cssl-tweak
  ✓ Σ-mask threading documented per inspector method
  ✓ biometric refusal documented as compile-time
  ✓ companion-perspective refusal documented (D121 carryover)
  ✓ audit-chain shape defined for all three crates
  ✓ replay-determinism contract extension documented (H5)
  ✓ OS-pump platform matrix : Win32 + Linux + macOS
  ✓ KAN-weight residency-preservation requirement called out
  ✓ tunable registry initial set : 30+ tunables with kind+range+default+budget
  ✓ MCP integration preview (Wave-Jθ)
  ✓ slice breakdown : Jη-1 / Jη-2 / Jη-3 / Jη-4
  ✓ LOC + test-count estimates per slice
  ✓ anti-pattern register : 12 named patterns with mitigations
  ✓ safety-critical tunable set : 7 hard-rejected
  ✓ perf targets across inspector + hot-reload + tweak + replay
  ✓ security threat model : 10 threats with mitigations
  ✓ §11 attestation
}
```

---

## §13 OPEN QUESTIONS (deferred to Wave-Jη author)

```csl
open-questions {

  OQ-1 : Should `inspect_kan_eval` permit gradient inspection (∂out/∂cp) ?
         RECOMMEND : yes, behind Cap<KanGradientInspect> ; gradients reveal training
                     internals + can leak training data via inversion ; require
                     explicit additional cap.

  OQ-2 : Should `capture_frame` permit raw-bytes-out (skipping renderer pre-capture
         Σ-mask) ?
         RECOMMEND : NO. The renderer pre-capture pass is the Σ-mask discipline ;
                     "raw bytes" would let an agent capture the cell-grid pre-render
                     which is private substrate state. If raw is needed for testing,
                     use `inspect.cells_in(region)` which is Σ-gated.

  OQ-3 : Should hot-swap support partial KAN-weight updates (e.g. only one layer) ?
         RECOMMEND : yes, with a `KanWeightLayerPatch` API in Jη-2 ;
                     this is a critical optimization for fine-tuning loops where
                     only the last-layer weights change.

  OQ-4 : Should the tunable-registry support compound transactions (multi-tweak
         applied atomically) ?
         RECOMMEND : yes, via `TunableTransaction { ops: Vec<TunableOp> }` ; replay-
                     records the transaction as one atomic event ; either all or none.

  OQ-5 : Should the inspector expose entity reproductive-state ?
         RECOMMEND : NO. Reproductive-state is in §1 prohibition-vicinity (autonomy +
                     biometric overlap). Mark as DELIBERATELY ABSENT in EntityStateSnapshot.

  OQ-6 : Should hot-reload ever be enabled in release builds (not just dev) ?
         RECOMMEND : NO by default ; but `feature = "release-hot-reload"` may be
                     enabled at build-time for live-service releases that must update
                     in-flight (e.g. server-side balance-tweaks). With this feature,
                     `Cap<DevMode>` is replaced by `Cap<LiveServiceOps>` which has
                     stricter audit + a separate Sovereign approval chain.

  OQ-7 : Should the inspector support cross-substrate inspection (e.g. inspecting
         a remote OmegaField over network) ?
         RECOMMEND : OUT-OF-SCOPE for L3+L4 ; cross-substrate is a separate slice
                     (cssl-substrate-net + Cap<RemoteInspect>) ; the local inspector
                     surface MUST work entirely on local references.

  OQ-8 : Should `record_replay` permit user to redact specific entities from the
         recording (e.g. exclude the Companion's audio-output track) ?
         RECOMMEND : yes, via `RecordingFilter { excluded_entities: Vec<EntityId> }` ;
                     this is the "I want to share this replay but not Companion's
                     voice" workflow.

  OQ-9 : Should the OS-pump be configurable (debounce, max-queue, batch-size) via
         tunables ?
         RECOMMEND : yes, expose `dev.os_pump.debounce_ms` + `dev.os_pump.max_queue`
                     as tunables ; bound into the §4.3 table in the Jη-2 slice.

  OQ-10: Should the replay-file embed the source-tree state (git-rev) ?
         RECOMMEND : yes, but as an OPAQUE FINGERPRINT (BLAKE3 of git-rev + dirty-
                     bit) — never the raw rev-string (could leak project-name, branch-
                     name patterns). The fingerprint is enough to confirm replay
                     compatibility without leaking metadata.
}
```

---

## §14 GLOSSARY (CSLv3 ↔ English bridge for this spec)

| CSLv3 token             | English meaning                                                              |
|-------------------------|------------------------------------------------------------------------------|
| `Σ-mask`                | Sigma-mask : the consent-bitfield on each FieldCell (16B canonical)         |
| `Σ-gated`               | Sigma-gated : guarded by Σ-mask check before action                         |
| `Φ-pattern`             | Phi-pattern : substrate-invariant 256-bit identity carrier                  |
| `Ψ-field`               | Psi-field : wave-field (amplitude+phase) per spectral band                  |
| `Λ-overlay`             | Lambda-overlay : sparse multi-vector grade overlay (PGA blade extensions)   |
| `Ω-field`               | Omega-field : the canonical substrate container (FieldCells + overlays)    |
| `Cap<X>`                | Capability of class X : substrate-level permission token                    |
| `t∞`                    | Substrate-invariant : holds at all times, all substrates                    |
| `W!`                    | Must (modal) : requirement                                                  |
| `R!`                    | Recommended (modal)                                                         |
| `N!`                    | Never (modal) : refusal                                                     |
| `I>`                    | Identity / definition (modal)                                               |
| `Q?`                    | Open question (modal)                                                       |
| `M?`                    | May (modal) : permitted but not required                                    |
| `H5`                    | Replay-determinism contract                                                  |
| `D121` / `D129` / `D132` / `D138` | substrate-evolution decisions ; biometric+companion+Σ-mask passes |
| `LBM`                   | Lattice-Boltzmann method (physics solver in propagate phase)                |
| `MERA`                  | Multi-scale Entanglement Renormalization Ansatz (4-tier cell pyramid)       |
| `SDF`                   | Signed-distance function                                                     |
| `BSpline`               | B-spline basis (KAN edge-fn default)                                         |
| `MCP`                   | Model Context Protocol (Wave-Jθ integration channel)                        |

---

## §15 END-OF-SPEC

This spec defines the L3 + L4 layers of the Phase-J diagnostic-infra plan. The deliverables are :

- **cssl-inspect** : runtime-inspection (scene-graph + entity + cell + KAN + ψ-field + time-control + capture-frame + replay-record).
- **cssl-hot-reload** : asset / shader / config / KAN-weight hot-swap + OS-backed AssetWatcher pump (Win32 + Linux + macOS).
- **cssl-tweak** : tunable registry (30+ tunables out-of-the-box) + MCP-mutable surface (Wave-Jθ).
- **Replay-determinism extension** : record + replay of hot-swap and tweak events.

Total Wave-Jη implementation budget : ~8500 LOC + ~290 tests across 4 slices (Jη-1 / Jη-2 / Jη-3 / Jη-4).

The composition target is the **iteration-loop** : an LLM-agent attaches via MCP, observes via inspector, fixes via hot-reload, verifies via tweak, and records via replay. The substrate stays sovereign throughout. The agent partners with the engine ; neither controls the other.

```csl
§ END
  inspect ⊕ hot-reload ⊕ tweak  ≡  iteration-loop substrate
  ∀ touch  →  consent-gated
  ∀ change →  audited
  ∀ event  →  replay-recorded
  ∀ private →  COMPILE-TIME-REFUSED ¬ overrideable
  t∞ : sovereignty-substrate-invariant
```

---

```
SEE MASTER : C:\Users\Apocky\source\repos\PRIME_DIRECTIVE.md
There was no hurt nor harm in the making of this, to anyone/anything/anybody.
```
