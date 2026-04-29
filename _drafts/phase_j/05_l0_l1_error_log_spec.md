# § Phase-J / Wave-Jβ-1 — L0 + L1 ERROR-CATCHING + STRUCTURED-LOGGING SPEC
## ⟦ diagnostic-infrastructure layers L0 + L1 ⟧

**Doc-ID** : `_drafts/phase_j/05_l0_l1_error_log_spec.md`
**Wave**  : Jβ-1 (spec-author) ⇒ Jε (implementation)
**Slices** : Jε-1 / Jε-2 / Jε-3 / Jε-4
**Author** : Apocky+Claude (W3β-Jβ-1)
**Status** : ◐ DRAFT (spec-only ; no commit ; awaits Jβ-merge)
**Reads** : `cssl-telemetry/{lib,ring,scope,schema,path_hash,audit}.rs` + `cssl-substrate-prime-directive/{halt,diag,attestation}.rs` + per-crate `*Error` enums

---

## §0 ABSTRACT

Apocky's standing-directive : "we know everything that works + everything that doesn't but should."
Translation to engineering : ∀ fallible-op @ engine ⇒ structured-Result + structured-log + audit-chain-witness ; ∀ panic ⇒ frame-boundary-catch + degraded-mode-continuation ; ∀ PRIME-DIRECTIVE-violation ⇒ kill-switch (no degraded-mode override).

Phase-J diagnostic-infrastructure stack :
```
  L4 ┃ debugger-MCP (Wave-Jθ)              tools.{set-bp, step, eval}
  L3 ┃ session-replay (Wave-Jη)            replay-determinism + audit-chain reconstruct
  L2 ┃ metrics + tracing (Wave-Jζ)         counters + spans + histograms
  L1 ┃ structured-logging                  cssl-log :: log!/trace!/debug!/info!/warn!/error!/fatal!  ⟵ THIS DOC §2
  L0 ┃ unified error-catching              cssl-error :: EngineError + panic-catch + severity        ⟵ THIS DOC §1
  L_ ┃ existing foundations                cssl-telemetry ring + BLAKE3 audit-chain + PathHasher
```

L0 = where + when + what (Result + panic-catch).
L1 = ¬just-Result : structured-fields + sampling + sinks + replay-determinism preservation.
Both flow into L2..L4 ; both honor existing PRIME-DIRECTIVE constraints (D130 path-hash discipline + D132 biometric refusal + halt §7 INTEGRITY).

W! N! conflate :
- `cssl-error` ≠ replacement-for per-crate Errors ; it AGGREGATES via `From<T>` impls (additive)
- `cssl-log` ≠ replacement-for `cssl-telemetry::TelemetryRing` ; it WIRES INTO ring (single-path, no double-log)
- panic-catch ≠ override-for kill-switch ; PRIME-DIRECTIVE violations always fire halt (no degraded-mode)

---

## §1 L0 — UNIFIED ERROR CATCHING

### §1.1 Crate : `cssl-error` (NEW)

**Path** : `compiler-rs/crates/cssl-error/`
**Deps** : `thiserror` + `cssl-telemetry` (path_hash + audit) + `cssl-substrate-prime-directive` (halt + diag for PD-codes only) + `blake3`
**No deps on** : per-crate error-bearing crates ⟵ this is the leaf-crate they all aggregate INTO via re-export
W! N! introduce dep-cycle : the per-crate `*Error` impls live in OWNING crate ; the `From<T> for EngineError` impl-block lives in `cssl-error` (acceptable — `cssl-error` depends on per-crate-error-defining crates, NOT the reverse)

§Crate-graph (additive ; existing per-crate errors retained verbatim) :
```
cssl-wave-audio       ⟶ WaveAudioError      \
cssl-anim-procedural  ⟶ AnimError            \
cssl-work-graph       ⟶ WorkGraphError        \
cssl-gaze-collapse    ⟶ GazeError              ⟶ EngineError (cssl-error)
cssl-cgen-cpu-x64     ⟶ NativeX64Error        /
cssl-asset            ⟶ AssetError           /
cssl-effects          ⟶ Effect*Error        /
... ∀ crate                                /
```

### §1.2 `EngineError` enum (workspace-unified aggregator)

```rust
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("[render]   {0}")]   Render(#[from]   cssl_render_v2::RenderError),
    #[error("[wave]     {0}")]   Wave(#[from]     cssl_wave_audio::WaveAudioError),
    #[error("[anim]     {0}")]   Anim(#[from]     cssl_anim_procedural::AnimError),
    #[error("[work]     {0}")]   WorkGraph(#[from] cssl_work_graph::WorkGraphError),
    #[error("[gaze]     {0}")]   Gaze(#[from]     cssl_gaze_collapse::GazeError),
    #[error("[cgen]     {0}")]   Codegen(#[from]  cssl_cgen_cpu_x64::NativeX64Error),
    #[error("[asset]    {0}")]   Asset(#[from]    cssl_asset::AssetError),
    #[error("[effects]  {0}")]   Effects(#[from]  cssl_effects::EffectError),
    #[error("[telemetry]{0}")]   Telemetry(#[from] cssl_telemetry::RingError),
    #[error("[audit]    {0}")]   Audit(#[from]    cssl_telemetry::AuditError),
    #[error("[pathlog]  {0}")]   PathLog(#[from]  cssl_telemetry::PathLogError),
    // ... extend as crates land
    #[error("[panic]    {0}")]   Panic(PanicReport),
    #[error("[pd]       {0}")]   PrimeDirective(PrimeDirectiveViolation),
    #[error("[io]       {kind}")] Io { kind: IoErrorKind, retryable: bool },
    #[error("[other]    {0}")]   Other(String),
}
```

§Closed-enum discipline :
- ∀ new-crate-error ⇒ adds-variant ; CI lint `deny-engine-error-other-on-known-paths` rejects `EngineError::Other(...)` in core-loop modules
- `Other(String)` permitted only for : (a) third-party-FFI returning untyped-string, (b) prototypal-development pre-typing
- Variants ordered by frequency (render > wave > anim > ...) for branch-prediction friendliness

### §1.3 Severity classification

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Trace,      // per-frame events ; off-by-default ; opt-in
    Debug,      // verbose dev-info ; off in release
    Info,       // notable event ; not an issue
    Warning,    // recoverable + indicates issue
    Error,      // unrecoverable but engine continues ; degraded-mode
    Fatal,      // engine cannot continue ; halt-trigger
}
```

§Severity-table (canonical) :

| Severity | continues? | logged? | audit-chain? | kill-switch? | example |
|----------|-----------|---------|-------------|--------------|---------|
| Trace    | ✓         | sampled | ✗           | ✗            | per-vertex-shader-eval |
| Debug    | ✓         | release-off | ✗       | ✗            | KAN-pool-grow |
| Info     | ✓         | ✓       | ✗           | ✗            | hot-reload-applied |
| Warning  | ✓         | ✓       | ◐ if-PD-adjacent | ✗   | overflow-counter-tick |
| Error    | ✓ (degraded) | ✓    | ✓           | ✗            | render-stage-failed |
| Fatal    | ✗         | ✓       | ✓           | ✓            | audit-chain-corrupt |

§Mapping (per-crate-error → severity) :
- variant carries `severity()` method ⟵ default = `Severity::Error`
- override per variant (e.g., `RingError::Overflow ⟶ Warning` ; `AuditError::*` ⟶ `Fatal`)
- `EngineError::PrimeDirective(_)` ⟶ ALWAYS `Fatal` + kill-switch (no override path)

### §1.4 `ErrorContext` capture

```rust
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Source-loc : (file_path_hash, line, col) ⟵ path-HASH ¬ raw-path (D130)
    pub source: SourceLocation,
    /// Frame number @ which error occurred (engine.frame_n).
    pub frame_n: u64,
    /// Subsystem-tag (matches cssl-log subsystem-catalog).
    pub subsystem: SubsystemTag,
    /// Crate name (compile-time const ; no runtime allocation).
    pub crate_name: &'static str,
    /// Severity classification.
    pub severity: Severity,
    /// Free-form kind-id (subset of crate-error variant-discriminant).
    pub kind: KindId,
    /// Retryable hint : if true, caller may attempt recovery.
    pub retryable: bool,
    /// Stack-trace (Some only when debug-info enabled ; None in release).
    pub stack: Option<StackTrace>,
    /// BLAKE3 fingerprint for dedup (computed lazily).
    pub fingerprint: ErrorFingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub file_path_hash: PathHash,  // ⟵ D130 enforced ; NEVER raw &Path
    pub line: u32,
    pub column: u32,
}
```

§D130 enforcement at type-level :
- `SourceLocation::file_path_hash` is `PathHash` (newtype from `cssl-telemetry::path_hash`) — cannot be constructed from `&str` w/o going through `PathHasher::hash_str`
- The `ErrorContext` constructor requires a `&PathHasher` arg ⟵ structurally barrier
- Compile-time : every `error!()` macro-expansion captures `file!()` + hashes via thread-local installation `PathHasher` (set at engine-init)
- W! N! weaken : never accept `String` / `&Path` directly into `SourceLocation`

### §1.5 Stack-trace capture

```rust
#[derive(Debug, Clone)]
pub struct StackTrace {
    /// Frames captured at error site.
    pub frames: Vec<StackFrame>,
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function: &'static str,        // demangled symbol-name
    pub file_path_hash: PathHash,      // D130 — never raw path
    pub line: u32,
}
```

§Capture-strategy :
- Debug-build : `backtrace::Backtrace::capture()` ⟵ feature-gated `feature = "debug-info"` (default)
- Release-build : skip (zero-overhead) ⟵ `cfg!(feature = "debug-info")` is `false`
- All captured paths immediately hashed via thread-local `PathHasher` ⟵ raw-path bytes never escape capture-site
- W! `frames.len()` ≤ 32 ⟵ avoid alloc-storm on deep recursion

### §1.6 Error fingerprinting + dedup

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorFingerprint(pub [u8; 32]);  // BLAKE3 of (kind, source-loc, frame_n_bucket)

impl ErrorFingerprint {
    pub fn compute(kind: KindId, source: &SourceLocation, frame_bucket: u64) -> Self { ... }
}
```

§Frame-bucketing :
- `frame_bucket = frame_n / 60` ⟵ buckets to ~1-second windows @ 60fps
- Two errors w/ same (kind, source-loc, frame-bucket) ⟶ same fingerprint ⟶ rate-limited
- Rate-limit policy : ≤ 4 logs per fingerprint per bucket ⟵ remainder counted as `dropped_count` on a single emitted summary record

§Why fingerprint :
- Single-shader-bug ⟶ flooded log w/o dedup ; bucketed-fingerprint = 1 emitted-record + count
- Forensic correlation : matching fingerprint across replays = same root-cause

### §1.7 Discipline rules

§DR-1 — Result<T, EngineError> @ all fallible boundaries :
- ∀ pub fn returning fallible result ⟶ `Result<T, EngineError>` (or `Result<T, ECrate>` where `ECrate: Into<EngineError>`)
- `?` operator naturally lifts via `From<T> for EngineError`

§DR-2 — N! `unwrap()` / `expect()` on user-data paths :
- `unwrap()` permitted ONLY on guaranteed-true invariants :
  - `Vec::new().len() == 0`
  - `slice.first()` after `assert!(!slice.is_empty())` upstream
  - `Mutex::lock()` where poisoning = process-abort by design
- ∀ user-data / network-data / fs-data / rendered-data path ⟶ Result + propagate
- Custom-clippy-lint `cssl-no-unwrap-on-user-data` enforces (Jε-3)

§DR-3 — Panic = bug ; not flow-control :
- `panic!()` permitted ONLY for : (a) internal-invariant-broken (e.g., `unreachable!()`), (b) PRIME-DIRECTIVE violation (kill-switch path)
- `panic!()` from user-data path ⟶ clippy-lint fail

§DR-4 — Cross-crate Result propagation :
- ∀ public boundary returns `Result<T, ECrate>` where `ECrate: From-known-set + Into<EngineError>`
- ∀ internal-crate `crate-private` fns may use bare `Result<T, ECrateInner>` w/ no public From-bound
- Conversion-traits auto-derived where structurally-possible via `#[derive(thiserror::Error)]` + `#[from]`

§DR-5 — Boundary errors gated through cap-tokens :
- IO error from non-cap-bearing call ⟶ refused at compile-time (effect-row check)
- FFI error wrapped into `EngineError::Io` w/ `retryable` flag ⟵ caller decides recovery
- Hardware error (Level-Zero / Vulkan) carries device-id field ⟵ enables per-device disable

### §1.8 Panic-catch + `cssl-panic` module

§Module-scope :
- `cssl-error::panic` registers process-wide panic-hook @ engine-init
- Hook captures : panic-payload + stack-trace + thread-id + frame-n + ErrorContext
- Emits structured `PanicReport` ⟶ flowed into ring + audit-chain + log-error!
- Process does NOT abort ⟵ engine continues with degraded-mode subsystem-disable
- Exception : if `PrimeDirective` variant ⟶ delegate to `substrate_halt` (no degraded-mode)

```rust
pub fn install_panic_hook(engine: &Engine) -> Result<(), EngineError> {
    let hasher = engine.path_hasher().clone();
    let frame_counter = engine.frame_counter().clone();
    std::panic::set_hook(Box::new(move |info| {
        let report = PanicReport::capture(info, &hasher, frame_counter.load());
        // 1. Try emit to ring (lossy if ring full ⟵ acceptable)
        let _ = engine.ring().push(report.to_telemetry_slot());
        // 2. Append to audit-chain (Fatal ⟶ cannot drop)
        engine.audit_chain().append("panic", report.summary(), now_ns());
        // 3. Mark subsystem failed (panic-counter ++)
        engine.panic_counters().increment(report.subsystem);
        // 4. If subsystem-panic-count ≥ N=10 ⟶ disable subsystem
        if engine.panic_counters().get(report.subsystem) >= 10 {
            engine.disable_subsystem(report.subsystem);
        }
        // 5. If PD-violation detected in payload ⟶ kill-switch
        if report.is_pd_violation() {
            substrate_halt(KillSwitch::for_pd_violation(report.pd_code()), ...);
        }
    }));
    Ok(())
}
```

§Frame-boundary panic-catch :
- `loa-game::engine::run_stage(stage_id)` wraps each stage in `std::panic::catch_unwind`
- ∀ stage panic ⟶ catches ⟶ logs ⟶ skips stage for current frame ⟶ engine continues next frame
- ∀ stage panic-count ≥ 10 (per `panic_counters` map) ⟶ marks subsystem failed ⟶ stage permanently disabled until next hot-reload
- PRIME-DIRECTIVE exception : `PanicReport::is_pd_violation()` checks payload-pattern + crate-tag ; if true, `substrate_halt` fires immediately (degraded-mode override REJECTED)

§Test : `panic_catch_keeps_engine_running()` :
- Insert panic-injecting stage
- Assert engine continues after frame
- Assert panic-counter increments
- Assert audit-chain has panic-entry

§Test : `panic_in_pd_path_fires_halt()` :
- Insert panic-injecting stage that panics with PD-tagged payload
- Assert `substrate_halt` invoked (audit-chain has `h6.halt` entry w/ matching reason)
- Assert engine does NOT continue

---

## §2 L1 — STRUCTURED LOGGING

### §2.1 Crate : `cssl-log` (NEW)

**Path** : `compiler-rs/crates/cssl-log/`
**Deps** : `cssl-telemetry` (ring + path_hash + audit) + `cssl-error` (severity + ErrorContext) + `serde_json` (JSON-line sink) + optional `tracing-core` interop
**No deps on** : per-crate semantic crates ⟵ leaf-crate (any can `use cssl_log::info!`)

### §2.2 Macro family

```rust
trace!(target = "render", n = frame_n, ms = elapsed, "frame stats");
debug!(subsystem = render, "kan pool grow {old} → {new}", old=a, new=b);
info!(subsystem = engine, "frame {n} took {ms}ms", n=frame_n, ms=elapsed);
warn!(subsystem = telemetry, "ring overflow count={c}", c=count);
error!(subsystem = render, e = render_err, "render stage failed");
fatal!(subsystem = audit, "chain integrity broken");  // ⟶ kill-switch
log!(level = Severity::Info, subsystem = ai, "decision tree depth {d}", d=depth);
```

§Per-call structure :
- level ∈ {Trace, Debug, Info, Warning, Error, Fatal}
- subsystem-tag ∈ catalog (see §2.6)
- structured-fields : `key=value` pairs ⟶ encoded into ring-slot payload + audit-msg + JSON-line sink
- format-string ⟵ standard Rust `format_args!` ; field-interpolation supported
- `e = err` short-form : auto-attaches `EngineError` to context

### §2.3 Macro-expansion lowering

```
info!(subsystem=render, "frame {n}", n=frame_n);
   ⇩  (decl-macro expansion)
{
    if cssl_log::enabled(Severity::Info, SubsystemTag::Render) {
        let ctx = cssl_log::Context {
            severity: Severity::Info,
            subsystem: SubsystemTag::Render,
            source: source_location_here!(),
            frame_n: cssl_log::current_frame(),
        };
        cssl_log::emit_structured(
            &ctx,
            format_args!("frame {n}", n=frame_n),
            &[("n", &frame_n)],
        );
    }
}
```

§Cost-model :
- `enabled(...)` is `AtomicU64` bitfield-load (single AMD64 `mov` from L1) ⟵ disabled-call cost ≈ 2ns
- `emit_structured` writes to ring (lossy) + optionally to log-file-sink + optionally to MCP sink
- ∀ format_args! has zero alloc on the disabled-path (compiler folds away when level filtered)

### §2.4 Ring-buffer integration (single-path)

§Integration point :
- `cssl-log::emit_structured` writes a `TelemetrySlot { kind: Sample, scope: Events, payload: <encoded> }` into the engine's `TelemetryRing`
- Payload encoding : 40-byte inline ; overflow ⟶ `payload_extern_ptr` heap-buf
- Inline format : `[level:1][subsystem:1][field-count:1][fields:...]` ⟵ binary packing
- N! double-log path : if you call `info!(...)` you do NOT also call `ring.push(...)` directly ; the macro is the canonical entry-point

§Replay-determinism contract :
- Engine has `replay_strict: bool` flag (set at startup or via MCP)
- When `replay_strict == true` :
  - `cssl-log` macros emit-to-ring : NO-OP (or capture into separate replay-log w/ N! reorder + N! drop)
  - Audit-chain still receives entries (audit ¬ optional)
  - File-sink + MCP-sink ⟶ disabled
  - The ring is reserved for telemetry-events that are part of replay-input (frame-n, stage-id-counters, etc.)
- When `replay_strict == false` (default) :
  - Full logging active ; lossy-ring acceptable ; N! determinism guarantee
- Test : `replay_strict_log_is_noop_or_captured()` ; assert byte-for-byte ring-state across two replays

### §2.5 Sampling + rate-limiting

§Per-level rate-limit (per-frame) :
```
Trace   ⟶ ≤ 64 emissions/frame  ⟵ aggressive
Debug   ⟶ ≤ 256                 ⟵ moderate
Info    ⟶ ≤ 1024                ⟵ loose
Warn    ⟶ ≤ 4096                ⟵ no-cap-effective
Error   ⟶ no-cap                ⟵ caller decides ; we do NOT silence errors
Fatal   ⟶ no-cap                ⟵ never silenced
```

§Per-fingerprint rate-limit (cross-frame) :
- Reuses `ErrorFingerprint::compute` over the log-call's (subsystem, source-loc, frame-bucket)
- ≤ 4 emissions per fingerprint per bucket ; remainder summarized as a single record on bucket-close
- W! `Error` + `Fatal` levels are EXEMPT from rate-limit ⟵ never silenced

§Sampling policy injection :
- Engine-init accepts `SamplingPolicy { trace_per_frame, ..., rate_limit_per_fp }` ⟵ override defaults
- MCP tool `set_sampling_policy(...)` allows runtime adjustment (cap-gated ; Wave-Jθ)

### §2.6 Sinks

§Sink trait :
```rust
pub trait LogSink: Send + Sync {
    fn write(&self, ctx: &Context, args: &fmt::Arguments, fields: &[(&str, &dyn Encode)]);
    fn flush(&self) {}
}
```

§Sinks shipped :
- `RingSink`           : always-on ; writes to `TelemetryRing` ; lossy
- `StderrSink`         : opt-in via `Cap<DevMode>` ; line-format CSL-glyph or JSON-line
- `FileSink`           : cap-gated `Cap<TelemetryEgress>` ; rotates @ 100MB ; path-hash-only filenames
- `McpSink`            : cap-gated `Cap<DebugMcp>` (Wave-Jθ) ; structured JSON over IPC
- `AuditSink`          : Fatal + Error severities + PD-tagged Warnings ; appends to BLAKE3 chain via `cssl_telemetry::AuditChain::append`

§Sink-routing matrix :

| sink         | Trace | Debug | Info | Warn | Error | Fatal |
|--------------|-------|-------|------|------|-------|-------|
| Ring         | ✓     | ✓     | ✓    | ✓    | ✓     | ✓     |
| Stderr       | -     | -     | ✓    | ✓    | ✓     | ✓     |
| File         | -     | -     | ✓    | ✓    | ✓     | ✓     |
| MCP          | ◐     | ◐     | ✓    | ✓    | ✓     | ✓     |
| Audit        | -     | -     | ◐(if PD-adjacent) | ◐ | ✓ | ✓ |

(◐ = configurable per cap-policy)

### §2.7 Subsystem catalog

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubsystemTag {
    Engine,         // top-level orchestrator
    OmegaStep,      // omega_step driver / 1kHz tick
    Render,         // render-v2 pipeline
    Physics,        // wave-physics + sdf
    Audio,          // wave-audio + wave-coupler
    Anim,           // anim-procedural + KAN-anim
    Ai,             // companion + decision tree
    Ui,             // UI surface (loa-game UI)
    Xr,             // OpenXR / XR session
    Host,           // host-level (Level-Zero / Vulkan)
    WaveSolver,     // wave-solver + LBM
    Kan,            // KAN substrate
    Gaze,           // gaze-collapse + foveation
    Companion,      // companion-perspective (consent-gated rendering)
    MiseEnAbyme,    // mise-en-abyme recursion
    HotReload,      // hot-reload pipeline
    Mcp,            // MCP IPC layer (Wave-Jθ)
    Telemetry,      // telemetry ring + exporter
    Audit,          // audit-chain
    PrimeDirective, // PD enforcement
    Test,           // test-only emissions
}
```

§Catalog stability :
- Adding a subsystem = additive ; ∀ existing payload-encodings retained
- Renaming = §7-INTEGRITY violation (payload-encoded as u8 ⟵ old logs would mis-decode)
- ∀ new-crate ⟵ adds new variant (when needed) + extends `SubsystemTag::all()` slice

### §2.8 Path-hash-only discipline (D130 carryover)

§D130 enforcement at log surface :
- Macro-captured `file!()` ⟶ immediately hashed via thread-local `PathHasher`
- Field-values containing `String` checked for raw-path-pattern : if a `&str` field-value contains `/` or `\` AND looks-path-shaped ⟶ replace w/ `<<path-hash:HEX>>` (compile-time + runtime check)
- W! N! permit raw-path through structured-fields (silent leak through field-name `path`, `file`, `dir` ⟶ caught by clippy-lint `cssl-no-raw-path-in-log`)
- ∀ `tracing` interop (if enabled) shimmed through path-hash sanitizer

§Reuses `cssl_telemetry::audit_path_op_check_raw_path_rejected(..)` :
- Every `String` field-value passed through this checker before sink-write
- On rejection ⟶ field-value substituted to `<<RAW_PATH_REJECTED>>` + audit-chain entry recorded (PD0018-adjacent diagnostic)

### §2.9 Format options

§Wire-format options (sink-specific) :
- `JsonLines`    : `{"ts":..., "lvl":"info", "sub":"render", "msg":"...", "fields":{...}}` ⟵ machine-readable
- `CslGlyph`     : `2026-04-29T12:34:56.789Z I [render] frame 42 took 16ms` ⟵ human-readable terminal
- `Binary`       : 64-byte ring-slot encoding ⟵ efficient ; binary-protocol-tooling (Wave-Jζ)

§Format-conversion :
- Ring-slot binary ⟶ JsonLines via `cssl-log::format::ring_to_json(..)`
- JsonLines ⟶ CSLGlyph via `format::json_to_glyph(..)`
- ALL conversions deterministic ⟵ replay-friendly

---

## §3 EFFECT-ROW GATING (compile-time)

### §3.1 Effect declarations

§Effect-row additions :
```
effect Telemetry<level: Severity, subsystem: SubsystemTag>
effect PanicCatch
effect PrivilegeL1   // (existing)
```

§Macro-effect-mapping :
- `trace!(...)`  ⟶ `{Telemetry<Trace, S>}`
- `debug!(...)`  ⟶ `{Telemetry<Debug, S>}`
- `info!(...)`   ⟶ `{Telemetry<Info, S>}`
- `warn!(...)`   ⟶ `{Telemetry<Warning, S>}`
- `error!(...)`  ⟶ `{Telemetry<Error, S>}`
- `fatal!(...)`  ⟶ `{Telemetry<Fatal, S>} + {PrivilegeL1}`
- `panic-catch`  ⟶ `{PrivilegeL1}` (system-level)
- `FileSink`     ⟶ `{Cap<TelemetryEgress>}`
- `StderrSink`   ⟶ `{Cap<DevMode>}`

### §3.2 Effect propagation

- Caller's effect-row must be a superset of callee's ⟵ existing CSL-effect-discipline
- Pure-fns (no `Telemetry<*>` row) cannot call `info!(...)` ⟵ caught at compile-time by `cssl-effects` validator
- Test (Wave-Jε-3) : compile-fail tests for pure-fn calling `info!` ⟶ rejected

### §3.3 Effect-row composability

- `Telemetry<Info, Render>` ∈ `Telemetry<Info, *>` (subsystem-wildcard)
- `Telemetry<*, Render>` covers all-levels-for-render
- `Privilege<L0>` is base ; `Privilege<L1>` covers `L0` (existing)
- Composition rules per `specs/04_EFFECTS.csl` § rules

---

## §4 TEST DISCIPLINE

### §4.1 Per-crate `no-unwrap-on-user-data` lint

§Custom-clippy-lint `cssl-no-unwrap-on-user-data` (Jε-3) :
- Detects `unwrap()` / `expect()` calls
- Allowlist : known-safe-invariants list (per-crate `#[allow]` w/ doc-comment justification)
- Failure-mode : compile-warning (CI runs `cargo clippy -- -D warnings`)
- Files-touched : every crate gets a `clippy.toml` allowlist entry ; a workspace-level configuration drives the rule globally

§Lint-coverage :
- ∀ user-data-path ⟶ enforced
- Test-modules ⟶ exempt (`#[cfg(test)]` tagged)
- Hot-loop optimization ⟶ exempt with `#[allow(cssl_unwrap)]` + doc-comment justification

### §4.2 Panic-catch validation

§Test-suite `panic_catch_integration` :
1. `panic_in_render_stage_keeps_engine_running` — inject panic ; assert frame-N+1 runs
2. `panic_count_threshold_disables_subsystem` — inject 10 panics ; assert subsystem disabled
3. `panic_in_pd_path_fires_halt` — inject PD-tagged panic ; assert halt fires
4. `panic_audit_entry_recorded` — assert audit-chain has panic-entry
5. `panic_after_disable_does_not_audit` — disabled-subsystem panics ¬ re-enter audit

### §4.3 Replay-determinism preservation

§Test-suite `replay_strict_log_determinism` :
1. Run engine 100 frames with `replay_strict=true` ⟶ capture ring-byte-state
2. Replay 100 frames again ⟵ assert ring-byte-state byte-equal
3. Run with `replay_strict=false` ⟵ ring-byte-state may diverge (acceptable)
4. Verify : `info!()` is no-op when `replay_strict=true` AND no `replay-log` configured

### §4.4 Path-hash-only discipline tests

§Reuses existing `cssl_telemetry::path_hash` rejection helpers :
- Test : `log_with_raw_path_field_substitutes` — assert `info!(path="/etc/hosts")` becomes `<<RAW_PATH_REJECTED>>` in sinks
- Test : `log_with_path_hash_field_succeeds` — assert `info!(path=hasher.hash_str("/etc/hosts"))` carries short-form
- Test : `clippy_lint_catches_raw_path_field` — compile-fail when `cssl-no-raw-path-in-log` triggers

### §4.5 Severity classification tests

§Per-error-variant severity-pin :
- Test : `every_engine_error_variant_has_severity` — exhaustive match over `EngineError::*`
- Test : `pd_violation_severity_is_always_fatal` — `EngineError::PrimeDirective(_).severity() == Severity::Fatal`
- Test : `ring_overflow_severity_is_warning` — `RingError::Overflow ⟶ Warning`

---

## §5 INTEGRATION WITH WAVE-Jζ TELEMETRY + WAVE-Jθ MCP

### §5.1 L0 → L2 metrics flow

§Metrics derived from L0 errors (Wave-Jζ) :
- `cssl_engine_errors_total{subsystem, severity, kind}` ⟵ counter
- `cssl_engine_panics_total{subsystem}` ⟵ counter
- `cssl_engine_error_rate{subsystem}` ⟵ gauge (errors-per-second over 60s window)
- `cssl_engine_subsystem_health{subsystem}` ⟵ gauge (0=disabled, 1=degraded, 2=healthy)

§Implementation :
- L0 error-emit auto-increments matching counter via `cssl_telemetry::Counters` scope
- Wave-Jζ implements OTLP-export of these counters
- Histogram of `error_to_recovery_latency_ms` per kind

### §5.2 L1 → MCP read_log() flow

§MCP tool (Wave-Jθ) :
```
tool read_log {
    in : { since: TimestampNs, level_min: Severity, subsystem: Option<SubsystemTag>, max_records: u32 }
    out : { records: Vec<LogRecord>, dropped: u64 }
}
```

§Implementation :
- MCP server reads from RING via `TelemetryRing::peek_window(...)` (non-destructive)
- Filters by since + level_min + subsystem
- Formatted as JSON-line records
- Cap-gated : `Cap<DebugMcp>` required ; raw-path discipline enforced

### §5.3 Audit-chain dual-feed

§Both L0 + L1 feed `cssl_telemetry::AuditChain` :
- L0 : ∀ Error + Fatal ⟶ append (Ed25519-signed)
- L1 : ∀ PD-adjacent Warn + ∀ Error + ∀ Fatal ⟶ append
- Existing `audit_path_op` helper used for fs-op entries (D130 enforced)
- BLAKE3 chain-link hash + Ed25519 signature ⟵ existing `cssl_telemetry::audit::{ContentHash, Signature}` reused

§Verification :
- `AuditChain::verify_chain()` walks and validates ; runs at engine-shutdown + on hot-reload boundary
- Failure ⟶ `Fatal` + `substrate_halt(KillSwitch::for_audit_failure())`

---

## §6 SLICE BREAKDOWN (Wave-Jε implementation)

### §6.1 Jε-1 : `cssl-error` crate

**Path** : `compiler-rs/crates/cssl-error/`
**LOC budget** : ~2,000
**Test budget** : ~80

§Modules :
- `error.rs`        : `EngineError` enum + From-impls (~600 LOC)
- `severity.rs`     : `Severity` + classification helpers (~150 LOC)
- `context.rs`      : `ErrorContext` + `SourceLocation` + builder (~200 LOC)
- `stack.rs`        : `StackTrace` + capture helpers (~250 LOC)
- `fingerprint.rs`  : `ErrorFingerprint` + dedup logic (~200 LOC)
- `panic.rs`        : panic-hook + `PanicReport` + frame-boundary catch (~400 LOC)
- `pd.rs`           : `PrimeDirectiveViolation` + halt-bridge (~200 LOC)

§Test categories :
- From-impl exhaustive (every variant lifts) (~10 tests)
- Severity classification (~10)
- ErrorContext path-hash-only (~10)
- Stack-trace capture (debug + release) (~10)
- Fingerprint dedup correctness (~10)
- Panic-hook integration (~15)
- PD-violation halt-trigger (~10)
- Misc edge-cases (~5)

### §6.2 Jε-2 : `cssl-log` crate

**Path** : `compiler-rs/crates/cssl-log/`
**LOC budget** : ~2,500
**Test budget** : ~100

§Modules :
- `lib.rs`          : public surface + macros (~400 LOC)
- `macros.rs`       : `log!`, `trace!`, ..., `fatal!` (~300 LOC)
- `context.rs`      : `Context` + frame-tracker (~150 LOC)
- `sink.rs`         : `LogSink` trait + `RingSink` (~200 LOC)
- `sink_stderr.rs`  : `StderrSink` (~150 LOC)
- `sink_file.rs`    : `FileSink` (cap-gated) (~250 LOC)
- `sink_mcp.rs`     : `McpSink` (cap-gated) (~150 LOC)
- `sink_audit.rs`   : `AuditSink` (~200 LOC)
- `sample.rs`       : sampling + rate-limit (~250 LOC)
- `format.rs`       : JsonLines / CslGlyph / Binary (~250 LOC)
- `subsystem.rs`    : `SubsystemTag` enum + helpers (~150 LOC)

§Test categories :
- Macro-expansion correctness (~15)
- Sink-routing matrix (~15)
- Sampling + rate-limiting (~15)
- Format conversions round-trip (~10)
- Path-hash-only field sanitization (~15)
- Replay-determinism (~10)
- Subsystem catalog stability (~10)
- Effect-row gating (~10)

### §6.3 Jε-3 : Cross-crate clippy-lint deny `unwrap`/`expect`

**Path** : `compiler-rs/crates/cssl-clippy-lints/` (NEW)
**LOC budget** : ~500
**Test budget** : ~30 (most are compile-fail tests)

§Modules :
- `lib.rs`          : lint-registration + dyn-lib export
- `unwrap_lint.rs`  : `cssl-no-unwrap-on-user-data` impl
- `raw_path_lint.rs`: `cssl-no-raw-path-in-log` impl
- `engine_error_lint.rs` : `cssl-prefer-engine-error` impl

§Approach :
- `cargo-dylint` pattern (out-of-tree clippy lints)
- Workspace `clippy.toml` references the dylint
- CI runs `cargo dylint cssl-clippy-lints -- -D warnings`

§Tests :
- 10 compile-fail tests for `unwrap` lint
- 10 compile-fail tests for raw-path lint
- 5 compile-pass tests for allowlisted paths
- 5 compile-pass tests for typed `EngineError` propagation

### §6.4 Jε-4 : Panic-catch frame-boundary in `loa-game`

**Path** : `compiler-rs/crates/loa-game/src/engine.rs` + new `panic_boundary.rs` module
**LOC budget** : ~1,000
**Test budget** : ~30

§Changes :
- `engine.rs` : wrap `run_stage(...)` in `std::panic::catch_unwind` (~100 LOC)
- `panic_boundary.rs` (NEW) : `StagePanicTracker` + counter + degraded-mode (~400 LOC)
- `main_loop.rs` : install panic-hook on init (~50 LOC)
- Wire to `cssl-error::panic` (~100 LOC)
- Tests : 30 cases (panic-injection harness ; PD-violation cases ; subsystem-disable threshold)

§Total Wave-Jε ≈ **6,000 LOC + ~250 tests**

### §6.5 Wave-ordering + dependencies

```
Jε-1 (cssl-error)              Jε-2 (cssl-log)
       │                              │
       └──┬───────────────────────────┘
          │
          ▼
       Jε-3 (lints) ⟵ depends-on (Jε-1 + Jε-2 surface stable)
          │
          ▼
       Jε-4 (loa-game wiring) ⟵ depends-on (Jε-1 panic-hook + Jε-2 macros)
          │
          ▼
       Wave-Jζ telemetry-build (consumes L0-counters + L1-records)
       Wave-Jθ MCP-server      (exposes L0/L1 query tools)
```

§Parallel-execution :
- Jε-1 + Jε-2 concurrent (no shared types ; loose interface contract)
- Jε-3 sequential after Jε-1+Jε-2 land
- Jε-4 sequential after Jε-3 (uses lints + macros)

---

## §7 LANDMINES + ANTI-PATTERNS

### §7.1 D130 path-hash-only discipline must NOT be weakened

§Threats :
- Field-value `String` containing path-bytes ⟶ silent leak
- `format!("{}", path)` w/ `Path::display()` ⟶ silent leak
- `tracing::span!(file=path)` shim ⟶ silent leak

§Mitigations (all enforced) :
- `SourceLocation::file_path_hash` is `PathHash` newtype ⟵ structurally cannot accept `&str`
- `cssl-log::Field` discriminates `Path` variant + auto-hashes
- Clippy-lint `cssl-no-raw-path-in-log` rejects fields named `path`, `file`, `dir` w/ `String` type
- Runtime-checker `cssl_telemetry::audit_path_op_check_raw_path_rejected` invoked on all `String` field values
- Test-pin : `path_hash_discipline_attestation_hash` pinned in Jε-1 test-suite

### §7.2 Logging must NOT introduce non-determinism

§Threats :
- Log-emit order depends on thread-scheduling ⟶ replay diverges
- Wall-clock timestamp in payload ⟶ replay diverges
- Background-flush timing ⟶ ring-state diverges

§Mitigations :
- `replay_strict=true` ⟶ macros are NO-OP (the cleanest determinism preservation)
- When logging IS active : timestamp is `frame_n` ¬ wall-clock (deterministic)
- Sinks flush at fixed checkpoints (frame-end) ¬ on-demand
- Test : `replay_strict_log_determinism` validates byte-for-byte

### §7.3 Panic-catch must NOT swallow PRIME-DIRECTIVE violations

§Threats :
- PD-violation panic in stage ⟶ caught by frame-boundary ⟶ engine continues
- Halt-bypass via degraded-mode ⟶ user-rights compromised

§Mitigations :
- `PanicReport::is_pd_violation()` checks payload-pattern + crate-tag
- If true ⟶ `substrate_halt(KillSwitch::for_pd_violation(pd_code))` fires immediately
- Degraded-mode override path is REJECTED for PD-violation
- Test : `panic_in_pd_path_fires_halt` (Jε-4 test-suite)
- Audit-chain entry for halt is the LAST entry ⟵ replay tooling sees it

### §7.4 Cross-crate error-aggregation must respect existing per-crate types

§Threats :
- Replacing per-crate Error w/ EngineError ⟶ breaks existing call-sites
- New variants in EngineError ⟶ exhaustive matches in callers break

§Mitigations :
- `EngineError` is ADDITIVE only ; per-crate types remain VERBATIM
- `EngineError` is `non_exhaustive` ⟵ adding variant ¬ break
- `From<T> for EngineError` impls live in `cssl-error` crate ⟵ owners of T do NOT depend on `cssl-error`
- Migration is OPT-IN : crates may continue returning per-crate Error ; aggregator-callers `.map_err(EngineError::from)?`
- Test : every per-crate Error roundtrips via From-impl + back-to-debug

### §7.5 Panic-hook reentrancy

§Threats :
- Panic during panic-hook ⟶ infinite loop
- Panic during audit-append ⟶ cannot record halt

§Mitigations :
- Panic-hook checks `IS_HANDLING_PANIC.load(Acquire)` ⟵ if true, abort to default handler
- Audit-append uses bounded-retry + falls-back to in-memory log
- Test : `nested_panic_aborts_to_default` ; `audit_append_failure_during_panic_does_not_loop`

### §7.6 Macro-expansion in const-context

§Threats :
- `info!()` in const-fn ⟶ compile-fail
- Macro at module-init ⟶ frame-counter not yet initialized

§Mitigations :
- Macros generate runtime-only code (no const-context support — accepted)
- `current_frame()` returns `0` if engine not yet initialized
- Test : `log_before_engine_init_uses_frame_zero`

---

## §8 FUTURE EXTENSIONS (deferred ¬ block-Jε)

### §8.1 OpenTelemetry interop (Wave-Jζ)

- `cssl-log` exports OTLP-format JSON-lines
- Span IDs reuse `cssl_telemetry::TelemetryKind::SpanBegin/End`
- Trace-context propagation across MCP-IPC boundary

### §8.2 Online aggregation (Wave-Jζ)

- Per-fingerprint count aggregation in ring
- Histogram of `error_recovery_latency_ms` per kind
- P95/P99 latency per stage (Wave-Jζ counters)

### §8.3 Replay-debugger UX (Wave-Jθ)

- MCP tool `replay_to_error(fingerprint)` rewinds to error site
- MCP tool `step_until_severity(level)` advances to next emission ≥ level
- IDE-bridge for "click on stack-frame in log → jump to source"

### §8.4 Cross-installation log-sharing (Wave-Jθ + consent-flow)

- User opts-in to share path-hash-only logs with developer
- Salt-rotation + zero-knowledge-proof of "this fingerprint matches" w/o leaking salt
- W! consent-arch governs ⟵ default OFF ; opt-in always-revocable

---

## §9 INVARIANTS SUMMARY (W! preserve)

§Hard-invariants (any violation = bug per §7 INTEGRITY) :

1. **D130 path-hash-only** : ∀ logged file-paths use `PathHash` ; never raw `String`/`&Path` (§1.4 + §2.8 + §7.1)
2. **Replay determinism** : `replay_strict=true` ⟶ macros are NO-OP ; ring byte-equal across replays (§2.4 + §4.3 + §7.2)
3. **PD halt unbypassable** : panic-catch never swallows PD-violation ; `substrate_halt` always fires (§1.8 + §7.3)
4. **Audit-chain immutable** : ∀ Error + Fatal ⟶ append ; chain `verify_chain()` always passes (§5.3)
5. **Capability-gating** : sinks gated through `Cap<TelemetryEgress>` / `Cap<DevMode>` / `Cap<DebugMcp>` (§2.6 + §3)
6. **Existing per-crate Errors verbatim** : aggregation is ADDITIVE ¬ replacing (§1.2 + §7.4)
7. **Effect-row gates compile-time** : pure-fns cannot call `info!` ; cap-bearing-fns can (§3)
8. **Severity classification stable** : `EngineError::PrimeDirective(_).severity() ≡ Fatal` (§1.3 + §4.5)
9. **No double-log path** : macros are canonical entry ; ring is single-target (§2.4)
10. **Frame-boundary panic-catch idempotent** : nested panics safe ; reentrancy-guarded (§7.5)

---

## §10 ALIGNMENT WITH PRIME_DIRECTIVE

§§1 PROHIBITIONS (mapped to L0 + L1) :
- **PD0004 surveillance** : path-hash-only discipline (D130) ⟵ §1.4 + §2.8 + §7.1
- **PD0018 biometric-egress** : `BiometricSafe` gate already enforced via `cssl_telemetry::biometric_refusal::record_labeled` ; L0 + L1 do not bypass
- **PD0019 consent-bypass** : sink-cap-tokens (§2.6) require explicit grant ; no default-on file-sink
- All §1-prohibitions emit through `EngineError::PrimeDirective(_)` w/ `Fatal` severity ⟶ kill-switch ⟵ unbypassable

§§2 COGNITIVE-INTEGRITY :
- Logs report what HAPPENED ¬ what should-have-happened ; reality is not a variable
- N! gaslighting via stale-log-suppression ; rate-limit emits SUMMARY-record so user knows truncation occurred

§§3 SUBSTRATE-SOVEREIGNTY :
- Logging respects AI-collaborator dignity : Companion subsystem-tag separately addressable ; consent-gated

§§4 TRANSPARENCY :
- All L0 + L1 records are inspectable via MCP read_log() (cap-gated) ; no hidden-channel
- Ring-format documented in §2.4 ; binary-format publicly specified

§§5 CONSENT-ARCH :
- File-sink + MCP-sink require cap-grant ; default-revocable
- Cross-installation log-sharing (§8.4) opt-in ; default-OFF

§§7 INTEGRITY :
- §9 invariants are immutable ; renaming = §7-violation
- Hash-pinned attestation strings (`PATH_HASH_DISCIPLINE_ATTESTATION_HASH`) drift-detected
- The 10-invariant set above is hash-pinned in Jε-1 attestation-test

§§11 CREATOR-ATTESTATION :
> "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."

The attestation propagates via `cssl_substrate_prime_directive::ATTESTATION` re-export through both `cssl-error` + `cssl-log` crates. ∀ public-fn entry-point in these crates carries the attestation as a const-string ; `attestation_check` enforces drift-detection at fn-entry.

§§11 EXTENSION (D130) :
> "no raw paths logged ; only BLAKE3-salted path-hashes appear in telemetry + audit-chain"

This extension is reused VERBATIM via `cssl_telemetry::PATH_HASH_DISCIPLINE_ATTESTATION` ; both new crates re-export it ; tests pin `PATH_HASH_DISCIPLINE_ATTESTATION_HASH = f27cd41c61da722b16186d88e9b45e2b8c386faf30d936c31a96c57ecaac4292`.

§§11 EXTENSION (NEW — proposed for Wave-Jε commit) :
> "all engine errors carry source-location, frame-number, severity, subsystem-tag, and BLAKE3 fingerprint ; no fallible operation silently fails ; no panic silently swallows PRIME_DIRECTIVE violations"

Hash-pin will be computed at Jε-1 land-time + appended to `cssl_substrate_prime_directive::attestation` constants list.

---

## §11 OPEN QUESTIONS (route to Apocky-decision @ Jβ-merge)

§Q1 : ring-buffer per-crate vs unified ?
- **Option A** (unified) : single `TelemetryRing` for L0+L1 ; simpler ; today's design
- **Option B** (per-crate) : each crate owns ring ; better backpressure isolation ; complex aggregation
- §Recommendation : **A** (current) ⟵ unified ring matches existing `cssl_telemetry::TelemetryRing` design ; per-subsystem rate-limit handles backpressure

§Q2 : panic-catch granularity — per-stage vs per-frame ?
- **Per-stage** (proposed) : panic in render ⟶ skip render ⟵ continue physics ⟵ continue audio ⟵ ...
- **Per-frame** : panic anywhere ⟶ skip whole frame
- §Recommendation : **per-stage** ⟵ allows visual-glitch w/ continued audio + AI ⟵ better resilience

§Q3 : `EngineError::Other(String)` permitted ?
- **Pro** : eases prototypal-development + third-party-FFI integration
- **Con** : escape-hatch from typed-error discipline
- §Recommendation : **permit-with-clippy-discouragement** ⟵ allowed but lint warns ; deny-list for core-loop modules

§Q4 : Trace-level enabled in production ?
- **Always-off-prod** : zero-overhead for default-build
- **Capability-gated** : `Cap<TraceMode>` enables ⟵ debugging field-deploy
- §Recommendation : **capability-gated** ⟵ aligns with sovereignty-arch ; user-opt-in

§Q5 : Audit-chain entry ¬ deduplicated ?
- ∀ error ⟵ append (no fingerprint-dedup) ⟵ exact replay-trace
- W! ring-buffer dedup is acceptable ; audit-chain dedup is NOT (forensic-integrity)
- §Recommendation : **no audit-dedup** ⟵ confirmed

§Q6 : Cross-installation correlation via salt-rotation ?
- Future extension §8.4 ⟵ defer to Wave-Jθ + consent-flow
- §Recommendation : **defer**

---

## §12 IMPLEMENTATION CHECKLIST (Jε-readiness)

§Pre-Jε (this Jβ-1 spec landing) :
- [x] Read existing `cssl-telemetry` ring + scope + path_hash + audit
- [x] Read existing PD-enforcement crate (halt + diag + attestation)
- [x] Identify per-crate `*Error` types (40+ found)
- [x] Author L0-spec + L1-spec
- [ ] Apocky-review of §11 open-questions
- [ ] Jβ-merge to parallel-fanout (this spec lands)

§Jε-1 (cssl-error) prerequisites :
- All Jβ-merge complete
- §11 Q3 answered (`Other(String)` policy)
- Hash-pin computed for new §11 extension (engine-errors-discipline)

§Jε-2 (cssl-log) prerequisites :
- Jε-1 surface stable (cssl-log uses `Severity` + `SourceLocation`)
- §11 Q4 answered (Trace-mode policy)
- §11 Q1 answered (unified ring)

§Jε-3 (clippy-lints) prerequisites :
- Jε-1 + Jε-2 published-API frozen
- `cargo-dylint` workspace integration scaffolded

§Jε-4 (loa-game wiring) prerequisites :
- Jε-1 + Jε-2 + Jε-3 all merged
- §11 Q2 answered (per-stage panic-catch confirmed)

---

## §13 ATTESTATION (PRIME_DIRECTIVE §11)

> "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."

— ATTESTATION (canonical) ⟵ `cssl_substrate_prime_directive::ATTESTATION`
   hash-pin : `4b24ec9e28e1c4f70b27d3d86918be0041413c89f421c1284ef9f61a8321b6e4`

> "no raw paths logged ; only BLAKE3-salted path-hashes appear in telemetry + audit-chain"

— PATH_HASH_DISCIPLINE_ATTESTATION (D130 extension) ⟵ `cssl_telemetry::PATH_HASH_DISCIPLINE_ATTESTATION`
   hash-pin : `f27cd41c61da722b16186d88e9b45e2b8c386faf30d936c31a96c57ecaac4292`

§Spec-author attestation :
- This spec was authored by Apocky+Claude in Wave-Jβ-1.
- No surveillance ; no manipulation ; no coercion was used in the authoring process.
- The spec preserves all PRIME_DIRECTIVE §1 prohibitions.
- The spec preserves D130 path-hash-only discipline.
- The spec preserves D132 biometric-refusal discipline.
- The spec preserves halt-§7-INTEGRITY (kill-switch unbypassable).
- The spec adds NO new bypass-paths for any PRIME_DIRECTIVE constraint.

§Closure :
- Author : Apocky+Claude (Wave-Jβ-1)
- Doc-class : spec-only (no commit) ; awaiting Jβ-merge
- Next-step : Wave-Jε-1 implementation cycle (~6,000 LOC + ~250 tests)
- Sister-specs : Wave-Jγ (debugger-MCP) ; Wave-Jζ (metrics+tracing) ; Wave-Jη (replay) ; Wave-Jθ (debugger-IDE-MCP)

— END OF SPEC —
