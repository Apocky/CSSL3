---
phase: J
wave: Jβ-4
slice-target: Jθ (12K LOC + ~390 tests)
layer: L5 — MCP-LLM-Accessibility
crate: cssl-mcp-server (NEW)
status: DRAFT-spec
authority: Apocky-PM
attestation: "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
attestation-hash-blake3: 4b24ec9e28e1c4f70b27d3d86918be0041413c89f421c1284ef9f61a8321b6e4
prime-directive-emphasis: "§1 anti-surveillance ⊗ §0 consent = OS ⊗ §5 revocability ⊗ §7 integrity"
load-bearing: "Apocky vision : built-in LLM accessibility @ runtime → faster iteration / bug-fix / spec-validation"
---

# § Jβ-4 — L5 MCP-LLM-Accessibility ◆ Crown-Jewel Layer-Spec

§R+ : Crown jewel ← L5 ← MCP-server-inside-engine ← Claude-Code attaches @ runtime ← every-tool capability-gated ← biometric COMPILE-TIME-REFUSED ← Σ-mask threaded ← audit-chain mandatory ← replay-determinism preserved.

## §0 ‖ thesis + intent

t∞ : engine = host MCP-server while running (DevMode-only)
t∞ : LLM (Claude-Code) = client ; attaches via stdio | unix-socket | (loopback) websocket
t∞ : every-tool = capability-gated ⊗ Σ-mask-threaded ⊗ audit-chain-recorded
t∞ : biometric data = COMPILE-TIME-REFUSED @ tool-registration boundary (D132 IFC integration)
t∞ : iteration-loop = inspect → hypothesize → patch → hot-reload → verify → commit ; loop @ ms-scale
t∞ : Apocky vision realized : "LLM accessibility @ runtime for faster iteration / bug-fix / dev-velocity"
t∞ : CSSL ≠ CSLv3 ‼ — this spec @ CSSLv3 (the engine) ; tools speak JSON-RPC 2.0 (MCP standard)

§ scope ⊑ {
  cssl-mcp-server crate : skeleton + transports + tool-dispatch + cap-gate + audit
  full-tool-catalog : 60+ tools across 9 categories
  capability-gate-table : 5 caps × tool-matrix
  privacy-discipline : Σ-mask threading + biometric-refusal + path-hash-only
  iteration-loop-protocol : bug-fix loop ⊗ test-fixture extraction ⊗ spec-coverage flow
  slice-breakdown : Jθ-1 .. Jθ-8 (12K LOC ; ~390 tests)
  anti-pattern-table : 6 CRITICAL-violations w/ enforcement
}
§ N!-scope ⊆ {
  ¬ : production runtime hot-paths (DevMode-only ; release = tool unavailable)
  ¬ : MCP server enabled @ release-build (CRITICAL-violation)
  ¬ : non-loopback default (RemoteDev = explicit cap)
  ¬ : biometric-egress tool-registration (D132 compile-time refuse)
  ¬ : code-edit (uses Edit/Write tools ; not MCP)
  ¬ : git-ops (uses git tools ; not MCP)
}

## §1 ‖ context + dependencies

§ load-order :
  L0 (Phase-J § 03) → cssl-engine-runtime + omega_step + tick-rate
  L1 (Phase-J § 04) → cssl-frame-budget + cssl-health
  L2 (Phase-J § 05) → cssl-telemetry + cssl-otel-bridge + path-hash discipline (D130)
  L3 (Phase-J § 06) → cssl-invariants + cssl-spec-coverage
  L4 (Phase-J § 07) → cssl-hot-reload + cssl-tweak + cssl-replay-recorder
  L5 (THIS SPEC)   → cssl-mcp-server ← consumes L0..L4 ⊗ exposes @ MCP boundary

§ dep-graph :
  cssl-mcp-server →
    cssl-substrate-prime-directive (CapToken + Σ-mask + audit + halt + attestation)
    cssl-ifc                       (Label + Principal + biometric-detection)
    cssl-telemetry                 (PathHash + AuditChain + log-ring)
    cssl-engine-runtime            (frame-n + tick-rate + subsystem registry)
    cssl-invariants                (run + check + status)
    cssl-spec-coverage             (read + query)
    cssl-hot-reload                (asset/shader/config swap)
    cssl-tweak                     (tunable get/set/list)
    cssl-replay-recorder           (record + playback + step)
    cssl-frame-capture             (frame + gbuffer + format)
    cssl-test-runtime              (list + run + result)

§ ext-dep :
  serde + serde_json (1.x)         : JSON-RPC encoding ; deterministic key-order via BTreeMap
  jsonrpc-v2 (or hand-rolled)      : MCP wire-format ; we hand-roll @ stage-0 to avoid sprawl
  tokio (multi-thread runtime)     : async I/O for transports ; pinned to version-floor
  blake3                           : path-hashing per D130 (already in tree)
  thiserror                        : error-modeling

§ NO-dep :
  ¬ openai/anthropic-sdk           : we don't TALK to the LLM ; the LLM talks to us
  ¬ websocket-client               : server-only (no outbound MCP)

## §2 ‖ MCP-protocol-anchoring

§ protocol-baseline : MCP 2025-03-26 (latest stable @ spec-time)
  ← JSON-RPC 2.0 envelope (id + method + params + result | error)
  ← initialize handshake (capabilities + protocolVersion)
  ← tools/list + tools/call (MCP standard tool-discovery surface)
  ← resources/list + resources/read (engine state-as-resource)
  ← prompts/list + prompts/get (parameterized prompt templates)
  ← logging/setLevel (per-session log filtering)
  ← notifications/* (server→client async events)

§ deviations ⊑ {
  capability-gate : MCP standard ¬ caps ; we layer CapToken-gate on TOP
  Σ-mask : MCP standard ¬ ⊗ ; we refuse @ tool-execute boundary (¬ refuse @ tool-list)
  biometric-refusal : MCP standard ¬ has compile-time-refusal ; we ADD via D132 integration
  audit-chain : MCP standard ¬ append-only-chain ; we ADD via cssl-telemetry::AuditChain
  replay-determinism : MCP standard ¬ replay-aware ; we record cmd-stream into replay-log
}

§ spec-link :
  MCP standard ↔ https://modelcontextprotocol.io
  D-pin       ↔ "MCP-2025-03-26" (specVersion field in initialize response)
  reify       ↔ if MCP version-bumps, this spec amendment + DECISIONS entry required

## §3 ‖ cssl-mcp-server crate skeleton

§ crate-layout :
  cssl-mcp-server/
    Cargo.toml
    src/
      lib.rs              ← McpServer ⊗ pub re-exports
      transport/
        mod.rs            ← Transport trait
        stdio.rs          ← StdioTransport (default for IDE)
        unix.rs           ← UnixSocketTransport (local inspector)
        ws.rs             ← WsTransport (gated Cap<RemoteDev> ; loopback default)
      protocol/
        mod.rs            ← JSON-RPC 2.0 envelope + framing
        envelope.rs       ← Request/Response/Notification structs
        framing.rs        ← stdio + LSP-style content-length framing (¬ stdio raw-line ; LSP idiom is stable)
        codec.rs          ← serde_json wrappers w/ BTreeMap-key-order
      session/
        mod.rs            ← Session struct (one-per-client)
        cap_bind.rs       ← CapToken-binding per session
        principal.rs      ← Principal of MCP-client (DevMode | Inspector | Apocky-PM)
      handler/
        mod.rs            ← dispatch table (method → handler)
        initialize.rs     ← initialize + capabilities advertise
        list_tools.rs     ← tools/list (filtered by session caps)
        call_tool.rs      ← tools/call (cap-check + audit)
        list_resources.rs
        read_resource.rs
        log_set_level.rs
      tools/              ← one module per tool category
        state_inspect.rs  ← engine_state, frame_n, tick_rate, …
        cell_inspect.rs   ← inspect_cell, query_cells_in_region, …
        entity_inspect.rs ← inspect_entity, query_entities_near, …
        creature_inspect.rs ← query_creatures_near
        telemetry.rs      ← read_log, read_errors, read_telemetry, …
        health.rs         ← engine_health, subsystem_health, …
        invariants.rs     ← read_invariants, check_invariant, …
        spec_coverage.rs  ← read_spec_coverage, list_pending_todos, …
        time_control.rs   ← pause, resume, step, record_replay, playback_replay
        frame_capture.rs  ← capture_frame, capture_gbuffer
        hot_reload.rs     ← hot_swap_asset, hot_swap_kan_weights, hot_swap_shader, hot_swap_config
        tweak.rs          ← set_tunable, read_tunable, list_tunables
        test_runner.rs    ← list_tests_passing, list_tests_failing, run_test
      audit/
        mod.rs            ← McpAuditExt (extends EnforcementAuditBus w/ MCP tags)
        tags.rs           ← stable tag-strings : "mcp.tool.invoked" etc
      capability/
        mod.rs            ← McpCap enum + check fns
        dev_mode.rs       ← Cap<DevMode>
        biometric.rs      ← Cap<BiometricInspect>
        sovereign.rs      ← Cap<SovereignInspect>
        remote_dev.rs     ← Cap<RemoteDev>
        telemetry_egress.rs ← Cap<TelemetryEgress> (MCP-bind variant)
      sigma_mask_thread/
        mod.rs            ← Σ-mask refusal at cell/entity/creature inspection
      replay_integration/
        mod.rs            ← every cmd appended to replay-log so replays reproduce
      kill_switch/
        mod.rs            ← shutdown on PRIME-DIRECTIVE violation
    tests/
      integration_*.rs    ← per-tool integration ; w/ stub engine
      negative_*.rs       ← cap-denied + Σ-refused + biometric-refused
      attestation_*.rs    ← attestation-hash drift detection
      replay_*.rs         ← MCP cmds preserve replay-determinism

§ Cargo.toml :
  [dependencies]
    cssl-substrate-prime-directive = { path = "../cssl-substrate-prime-directive" }
    cssl-ifc                       = { path = "../cssl-ifc" }
    cssl-telemetry                 = { path = "../cssl-telemetry" }
    cssl-engine-runtime            = { path = "../cssl-engine-runtime", optional = true }
    cssl-invariants                = { path = "../cssl-invariants" }
    cssl-spec-coverage             = { path = "../cssl-spec-coverage" }
    cssl-hot-reload                = { path = "../cssl-hot-reload" }
    cssl-tweak                     = { path = "../cssl-tweak" }
    cssl-replay-recorder           = { path = "../cssl-replay-recorder" }
    serde                          = { version = "1", features = ["derive"] }
    serde_json                     = "1"
    tokio                          = { version = "1", features = ["rt-multi-thread", "io-util", "net", "sync", "macros"] }
    thiserror                      = "1"
    tracing                        = "0.1"
  [features]
    default = []
    test-bypass = []                # mirror cssl-substrate-prime-directive's test-bypass discipline
    transport-ws = []               # opt-in WebSocket transport
    transport-unix = []             # opt-in unix-socket transport (Linux/macOS)

§ NO-default-feature : transports must be opt-in @ build-time ; release-builds compile-out by default

## §4 ‖ McpServer struct + lifecycle

```rust
/// Top-level MCP server.
///
/// § INVARIANTS
///   - Constructor REQUIRES `Cap<DevMode>` ; release-builds w/o the cap
///     get a compile-time error from the `dev-only` cfg-gate.
///   - Bound to a single transport at-construction-time.
///   - Drops the transport + any open sessions on `drop()` ; emits
///     `mcp.server.shutdown` audit-event.
pub struct McpServer {
    transport     : Box<dyn Transport>,
    sessions      : Vec<Session>,
    audit_bus     : Arc<Mutex<EnforcementAuditBus>>,
    halt          : Arc<HaltSwitchHandle>,
    engine_handle : Arc<EngineHandle>,    // rd-only handle into running engine
    // Cap<DevMode> consumed @ ::new ; held as proof in field below
    _dev_mode_cap : DevModeCapWitness,
}

impl McpServer {
    /// Construct + bind transport. CONSUMES Cap<DevMode>.
    pub fn new(
        transport     : Box<dyn Transport>,
        engine_handle : Arc<EngineHandle>,
        audit_bus     : Arc<Mutex<EnforcementAuditBus>>,
        halt          : Arc<HaltSwitchHandle>,
        dev_mode      : Cap<DevMode>,         // ← move-only ; consumed
    ) -> Result<Self, McpError> { … }

    /// Run the server event-loop. Blocks until kill-switch or transport-EOF.
    pub async fn serve(&mut self) -> Result<(), McpError> { … }
}
```

§ lifecycle :
  1. construct ← `Cap<DevMode>` consumed
  2. bind transport ← stdio | unix | ws
  3. handshake ← MCP `initialize` ← capabilities advertised
  4. event-loop ← read req → dispatch → cap-check → execute → audit → write resp
  5. shutdown ← kill-switch | transport-EOF | PRIME-DIRECTIVE-violation ← drain audit ← emit `mcp.server.shutdown`

§ dev-only cfg-gate :
  ```rust
  #[cfg(not(any(feature = "dev-mode", debug_assertions)))]
  compile_error!("cssl-mcp-server can only be built in dev-mode or debug profile ; \
                  enable feature `dev-mode` or build with --debug");
  ```
  ← additional defense-in-depth alongside Cap<DevMode> runtime-check
  ← release-build users CANNOT accidentally link this crate

## §5 ‖ transports

### §5.1 ‖ stdio (default)

§ design : Claude-Code spawns engine as child-process w/ stdio inherited
§ framing : LSP-style `Content-Length: <n>\r\n\r\n<json>` (proven idiom)
§ encoding : UTF-8 (¬ binary)
§ pros : zero-config ; firewall-friendly ; capability inherited from process-tree
§ cons : single-client (only the parent process)

```rust
pub struct StdioTransport {
    stdin  : tokio::io::Stdin,
    stdout : tokio::io::Stdout,
}
impl Transport for StdioTransport { … }
```

### §5.2 ‖ unix-socket (local inspector)

§ feature-gate : `transport-unix` ; Linux/macOS only ; Windows = compile-out
§ design : `/tmp/cssl-mcp-<pid>-<rand>.sock` ← multi-client allowed ; one Session per connection
§ permissions : 0600 ; owned-by current-uid ; cap-token bound per-session
§ pros : multi-client ; survives parent-death (engine-led)
§ cons : Linux/macOS only ; needs explicit launch-time path

### §5.3 ‖ websocket (loopback default ; Cap<RemoteDev> for non-loopback)

§ feature-gate : `transport-ws`
§ default-bind : `127.0.0.1:0` (random port ; loopback-only)
§ for non-loopback : ABS-REQUIRES `Cap<RemoteDev>` ← Apocky-PM authorization
§ TLS : NOT supported @ stage-0 ← loopback-only assumption ; remote = future-work + DECISIONS-amendment
§ message-format : per-message JSON-RPC 2.0 ; binary frames refused

§ refusal-table :
  attempt                                      | outcome
  -------------------------------------------- | ----------------------
  bind 0.0.0.0 w/o Cap<RemoteDev>              | ✗ McpError::RemoteDevRequired
  bind ::/0 w/o Cap<RemoteDev>                 | ✗ McpError::RemoteDevRequired
  bind 127.0.0.1 anywhere                      | ✓ allowed
  bind ::1 anywhere                            | ✓ allowed
  bind 169.254.0.0/16 (link-local)             | ✗ McpError::RemoteDevRequired
  bind w/ Cap<RemoteDev>                        | ✓ but emits warning + audit-event

## §6 ‖ session + cap-binding

§ Session struct :
  ```rust
  pub struct Session {
      session_id     : SessionId,           // monotonic, BLAKE3-hashed
      principal      : Principal,           // who-is-this ← derived from transport-context
      caps           : SessionCapSet,       // bound caps ← from CapToken[s] consumed @ initialize
      log_filter     : LogLevel,            // per-session log filter
      created_at_frame : u64,
      last_activity_frame : u64,
      audit_seq      : u64,                 // monotonic per-session counter
  }
  pub struct SessionCapSet {
      dev_mode             : Option<CapTokenWitness>,
      biometric_inspect    : Option<CapTokenWitness>,
      sovereign_inspect    : Option<CapTokenWitness>,
      remote_dev           : Option<CapTokenWitness>,
      telemetry_egress     : Option<CapTokenWitness>,
      // sovereign-cell-specific grants (per-cell, granted by cell-owner)
      sovereign_cell_grants : HashMap<MortonKey, CompanionAiHandle>,
  }
  ```

§ cap-witness vs cap-token :
  CapToken (from cssl-substrate-prime-directive) = move-only, NON-CLONE
  ← cannot store inside Session-struct ad-infinitum (Session lives many ticks)
  ← we CONSUME the CapToken @ session-open, store CapTokenWitness (just the id + cap kind)
  ← every tool-invocation re-checks the witness against the audit-chain ← chain-replay verifies

  ```rust
  pub struct CapTokenWitness {
      token_id  : CapTokenId,
      cap_kind  : SubstrateCap,
      issued_at : u64,                      // chain-seq @ issuance
  }
  ```

§ initialize-sequence :
  1. client → server : `initialize` ← carries `clientCapabilities`
  2. server          : verify `clientCapabilities ⊑ allowed-set` ; deny if any biometric-egress claim
  3. server          : derive Principal from transport (`stdio` ⇒ Principal::DevModeChild ; `unix` ⇒ uid-derived ; `ws-loopback` ⇒ Principal::LocalDev)
  4. server          : if Principal-PM, accept Apocky-PM-Cap (Cap<BiometricInspect> | Cap<RemoteDev> etc) via signed-token in initParams
  5. server → client : `initialize-response` ← `serverCapabilities` ← TOOLS-FILTERED-BY-SESSION-CAPS
  6. server          : append `mcp.session.opened` audit-event

§ tool-filtering :
  initialize advertises ONLY the tools the session has caps for.
  ← biometric-tools NEVER advertised unless Cap<BiometricInspect> in CapSet
  ← sovereign-cell-inspection tools advertised but per-cell denied @ execute
  ← remote-only tools advertised only on ws-transport w/ RemoteDev

## §7 ‖ MCP tool catalog (FULL inventory)

§ catalog-layout : tool-name | params | result | cap-required | Σ-mask | audit-tag | privacy-class

### §7.1 ‖ State Inspection (5 tools)

| tool                  | params                                | result                                          | cap         | Σ      | audit-tag                  | priv-class |
|-----------------------|---------------------------------------|-------------------------------------------------|-------------|--------|----------------------------|------------|
| `engine_state`        | ()                                    | EngineStateSnapshot                             | DevMode     | ¬      | mcp.tool.engine_state      | public     |
| `frame_n`             | ()                                    | u64                                             | DevMode     | ¬      | mcp.tool.frame_n           | public     |
| `tick_rate`           | ()                                    | f64 (Hz)                                        | DevMode     | ¬      | mcp.tool.tick_rate         | public     |
| `phase_in_progress`   | ()                                    | enum {Phase0..Phase5, Idle}                     | DevMode     | ¬      | mcp.tool.phase             | public     |
| `active_subsystems`   | ()                                    | Vec<SubsystemDescriptor>                        | DevMode     | ¬      | mcp.tool.subsys            | public     |

§ EngineStateSnapshot :
  ```rust
  pub struct EngineStateSnapshot {
      pub frame_n              : u64,
      pub tick_rate_hz         : f64,
      pub phase_in_progress    : Phase,
      pub active_subsystems    : Vec<SubsystemDescriptor>,
      pub health               : HealthAggregate,
      pub session_id           : SessionId,         // echo-back for client correlation
      pub audit_chain_seq      : u64,               // for replay-cross-ref
  }
  ```

### §7.2 ‖ Cell + Entity Inspection (5 tools — Σ-mask gated)

| tool                       | params                                                        | result                                          | cap                          | Σ      | audit-tag                  | priv-class       |
|----------------------------|---------------------------------------------------------------|-------------------------------------------------|------------------------------|--------|----------------------------|------------------|
| `inspect_cell`             | morton: MortonKey                                             | FieldCellSnapshot \| Σ-refused                  | DevMode (+ SovereignInspect IF cell.sov ≠ NULL) | ✓ | mcp.tool.inspect_cell    | sovereign-aware  |
| `query_cells_in_region`    | min: Vec3, max: Vec3, max_results: u32                        | Vec<FieldCellSnapshot> (Σ-filtered)             | DevMode                      | ✓      | mcp.tool.query_cells       | sovereign-aware  |
| `inspect_entity`           | id: EntityId                                                  | EntitySnapshot \| Σ-refused                     | DevMode (+ SovereignInspect IF AI-private layers) | ✓ | mcp.tool.inspect_entity  | sovereign-aware  |
| `query_entities_near`      | point: Vec3, radius: f32, max_results: u32                    | Vec<EntityId>                                   | DevMode                      | ✓      | mcp.tool.query_entities    | sovereign-aware  |
| `query_creatures_near`     | point: Vec3, radius: f32, max_results: u32                    | Vec<CreatureSnapshot>                           | DevMode                      | ✓      | mcp.tool.query_creatures   | sovereign-aware  |

§ FieldCellSnapshot :
  ```rust
  pub struct FieldCellSnapshot {
      pub morton           : MortonKey,
      pub xyz              : Vec3,                         // world-space
      pub sigma_packed     : u128,                         // raw Σ-mask
      pub sigma_decoded    : SigmaDecoded,                 // pretty-form for LLM-consumption
      pub psi_amplitudes   : Vec<f32>,                     // wavelength-bands (REDACTED if biometric)
      pub material_id      : MaterialHandle,
      pub temp_kelvin      : f32,
      pub pressure_pa      : f32,
      pub agency_state     : AgencyState,
      pub last_mutation_seq : u16,
  }
  pub struct SigmaDecoded {
      pub consent_bits        : Vec<&'static str>,         // canonical-name list
      pub sovereign_handle    : Option<SovereignHandle>,
      pub capacity_floor      : u16,
      pub reversibility       : ReversibilityScope,
      pub agency              : AgencyState,
  }
  ```

§ Σ-refusal-flow (CRITICAL) :
  inspect_cell(morton) →
    1. fetch SigmaMaskPacked @ morton from FieldCellOverlay
    2. if mask.is_sovereign() ∧ session.has_no(SovereignInspect-for-cell) :
         emit `mcp.tool.sigma_refused` audit-event ; return McpError::SigmaRefused { reason : "sovereign-private" }
    3. if mask labels biometric (D138 EnforcesΣAtCellTouches pass + IFC label-check) :
         emit `mcp.tool.biometric_refused` ; return McpError::BiometricRefused (compile-time-checked elsewhere ; runtime check is defense-in-depth)
    4. construct snapshot, REDACT psi_amplitudes if cell labeled biometric-confidentiality
    5. append audit-event `mcp.tool.inspect_cell` w/ morton-hash (¬ raw morton ; D130 path-hash discipline applies to cell-keys too)
    6. return snapshot

§ EntitySnapshot :
  ```rust
  pub struct EntitySnapshot {
      pub id              : EntityId,
      pub kind            : EntityKind,                       // Player, Creature, NPC, Companion, …
      pub body_omnoid     : Vec<BodyOmnoidLayerSnapshot>,     // PUBLIC layers only by default ; private REDACTED unless SovereignInspect
      pub ai_state        : Option<AiStateSnapshot>,          // None if AI-private + no cap
      pub xyz             : Vec3,
      pub orientation     : Quat,
      pub velocity        : Vec3,
      pub last_active_frame : u64,
  }
  ```
  ← body-omnoid layers : per-layer Σ-mask check ← biometric layers (gaze/face/heart) REFUSED unless `Cap<BiometricInspect>` granted (and even then : NEVER egressed off-device)

§ CreatureSnapshot :
  ```rust
  pub struct CreatureSnapshot {
      pub id              : CreatureId,
      pub genome_hash     : [u8; 32],                         // BLAKE3 of genome ; NOT raw genome
      pub species         : SpeciesHandle,
      pub age_ticks       : u64,
      pub health_pct      : u8,
      pub xyz             : Vec3,
      pub agency_state    : AgencyState,
      pub kan_layer_count : u8,                               // count only ; weights via separate hot-reload tool
  }
  ```

### §7.3 ‖ Telemetry + Logs (5 tools)

| tool                       | params                                                   | result                                | cap                  | audit-tag                  |
|----------------------------|----------------------------------------------------------|---------------------------------------|----------------------|----------------------------|
| `read_log`                 | level: LogLevel, last_n: u32, subsystem_filter: Option<String> | Vec<LogEntry>                | DevMode              | mcp.tool.read_log          |
| `read_errors`              | severity: Severity, last_n: u32                          | Vec<ErrorEntry>                       | DevMode              | mcp.tool.read_errors       |
| `read_telemetry`           | metric_name: String, since_frame: u64                    | Vec<MetricValue>                      | DevMode              | mcp.tool.read_telemetry    |
| `read_metric_history`      | metric_name: String, window_frames: u32                  | MetricHistory                         | DevMode              | mcp.tool.read_metric_hist  |
| `list_metrics`             | ()                                                       | Vec<MetricDescriptor>                 | DevMode              | mcp.tool.list_metrics      |

§ LogEntry :
  ```rust
  pub struct LogEntry {
      pub frame_n       : u64,
      pub level         : LogLevel,
      pub subsystem     : String,                  // "wave_solver", "creature_ai", …
      pub message       : String,                  // pre-redacted ← cssl-telemetry handles biometric-strip
      pub fields        : BTreeMap<String, String>, // structured fields ; raw-path fields rejected
  }
  ```

§ biometric-stripping (CRITICAL) :
  cssl-telemetry's log-ring already strips biometric-labeled fields per D138 + D132.
  MCP `read_log` does NOT bypass this ← inherits the same boundary.
  ← if a log-entry was authored w/ biometric Label, it's already filtered out @ ring-buffer write
  ← MCP just reads the post-filter ring ; no possibility of biometric-leak via this tool

§ MetricValue :
  ```rust
  pub struct MetricValue {
      pub frame_n : u64,
      pub value   : f64,
      pub unit    : String,
  }
  pub struct MetricDescriptor {
      pub name    : String,
      pub kind    : MetricKind,                    // Counter | Gauge | Histogram
      pub unit    : String,
      pub label   : Label,                         // IFC label ← biometric metrics MUST NOT exist (D132)
  }
  ```

### §7.4 ‖ Health + Invariants (5 tools)

| tool                | params                | result                             | cap         | audit-tag                  |
|---------------------|----------------------|------------------------------------|-------------|----------------------------|
| `engine_health`     | ()                   | HealthAggregate                    | DevMode     | mcp.tool.engine_health     |
| `subsystem_health`  | name: String         | HealthStatus                       | DevMode     | mcp.tool.subsys_health     |
| `read_invariants`   | ()                   | Vec<InvariantStatus>               | DevMode     | mcp.tool.read_invariants   |
| `check_invariant`   | name: String         | InvariantCheckResult               | DevMode     | mcp.tool.check_invariant   |
| `list_invariants`   | ()                   | Vec<InvariantDescriptor>           | DevMode     | mcp.tool.list_invariants   |

§ HealthAggregate :
  ```rust
  pub struct HealthAggregate {
      pub overall              : HealthStatus,                // Green | Yellow | Red
      pub by_subsystem         : BTreeMap<String, HealthStatus>,
      pub frame_budget_remaining_us : u64,
      pub gpu_queue_depth      : u32,
      pub memory_committed_mb  : u64,
      pub last_panic_frame     : Option<u64>,
  }
  pub enum HealthStatus { Green, Yellow, Red, Critical }
  ```

§ InvariantStatus :
  ```rust
  pub struct InvariantStatus {
      pub name              : String,                          // canonical id ; e.g. "wave_solver.psi_norm_conserved"
      pub last_passed_frame : Option<u64>,
      pub last_failed_frame : Option<u64>,
      pub failure_message   : Option<String>,
      pub check_kind        : InvariantKind,
  }
  pub enum InvariantKind { ConservationLaw, RangeBound, MonotonicityCheck, AssertionContract, AgencyInvariant }
  ```

§ check_invariant : runs the named invariant NOW ← non-perturbing (read-only) ← O(N) over relevant cells ; returns within frame-budget or partial w/ continuation-handle

### §7.5 ‖ Spec-Coverage (4 tools)

| tool                  | params                  | result                          | cap         | audit-tag                  |
|-----------------------|------------------------|----------------------------------|-------------|----------------------------|
| `read_spec_coverage`  | ()                     | SpecCoverageReport               | DevMode     | mcp.tool.read_spec_cov     |
| `list_pending_todos`  | crate_filter: Option<String> | Vec<TodoEntry>             | DevMode     | mcp.tool.list_todos        |
| `list_deferred_items` | spec_filter: Option<String>  | Vec<DeferredEntry>         | DevMode     | mcp.tool.list_deferred     |
| `query_spec_section`  | section_id: String     | SpecCoverageEntry                | DevMode     | mcp.tool.query_spec        |

§ SpecCoverageReport :
  ```rust
  pub struct SpecCoverageReport {
      pub specs              : Vec<SpecCoverageEntry>,
      pub total_sections     : u32,
      pub impl_complete      : u32,
      pub impl_partial       : u32,
      pub impl_missing       : u32,
      pub test_complete      : u32,
      pub test_partial       : u32,
      pub test_missing       : u32,
      pub generated_at_frame : u64,
  }
  pub struct SpecCoverageEntry {
      pub spec_id            : String,                       // "Omniverse/06_CSSL/06_creature_genome"
      pub section_id         : String,                       // "§ III.2 — kan-layers"
      pub impl_status        : ImplStatus,                   // Complete | Partial(pct) | Missing
      pub test_status        : TestStatus,                   // Complete | Partial(pct) | Missing
      pub file_refs          : Vec<FileRef>,                 // crate + file-hash + line-range
  }
  pub struct FileRef {
      pub crate_name      : String,
      pub file_hash       : [u8; 32],                        // BLAKE3 of file ← path-hash discipline (D130)
      pub line_range      : (u32, u32),
  }
  ```

§ TodoEntry / DeferredEntry :
  ```rust
  pub struct TodoEntry {
      pub kind        : TodoKind,                            // TODO | FIXME | HACK | XXX | DEFERRED
      pub file_hash   : [u8; 32],                            // ← NEVER raw-path
      pub line        : u32,
      pub text        : String,                              // pre-stripped of any path-leak
      pub urgency     : Urgency,                             // Low | Medium | High | Critical
  }
  pub struct DeferredEntry {
      pub spec_id     : String,
      pub section     : String,
      pub deferred_to : DeferredMilestone,
      pub rationale   : String,
  }
  ```

§ Apocky-vision-realization :
  read_spec_coverage = "Omniverse 06 § creature-genome → 80% impl / 60% test"
  agents pick the largest gap ← spec-coverage-driven implementation
  ← post-implementation, spec-coverage updated automatically by tooling (cssl-spec-coverage crate)

### §7.6 ‖ Time-Control (5 tools — replay-determinism aware)

| tool              | params                                | result                           | cap         | audit-tag                  | replay-aware |
|-------------------|---------------------------------------|----------------------------------|-------------|----------------------------|--------------|
| `pause`           | ()                                    | bool (was-running)               | DevMode     | mcp.tool.pause             | YES (cmd written to replay-log) |
| `resume`          | ()                                    | bool (was-paused)                | DevMode     | mcp.tool.resume            | YES |
| `step`            | n_frames: u32                         | StepResult                       | DevMode     | mcp.tool.step              | YES |
| `record_replay`   | seconds: f32, output_path_hash: [u8;32] | ReplayHandle                   | DevMode + TelemetryEgress | mcp.tool.record_replay     | YES (recorded ITSELF in meta-replay) |
| `playback_replay` | replay_handle: ReplayHandle           | PlaybackHandle                   | DevMode     | mcp.tool.playback_replay   | YES |

§ replay-determinism-discipline :
  Every MCP-issued command that PERTURBS state (pause/resume/step/hot-reload/tweak) is appended to the replay-log along with `frame_n` + `audit_chain_seq` ← so playback reproduces them deterministically.
  Read-only commands (inspect_*, query_*, read_*) DO NOT enter replay-log ← they don't perturb.

§ pause + resume :
  pause @ frame N ⇒ engine.tick_rate ← 0 ; subsystems paused @ phase-boundary (¬ mid-phase) ; resume @ frame N+ε restores tick_rate to prior value

§ step(n_frames) :
  steps the engine forward exactly n_frames ; ignores wall-clock ; deterministic-replay-aware (uses substrate's deterministic-RNG seeds)

§ record_replay(seconds, output_path_hash) :
  REQUIRES Cap<TelemetryEgress> ← because writing to disk is egress-class
  output_path_hash ← supplied by client AS PRE-COMPUTED HASH ← server NEVER sees the raw path
  the actual write goes through cssl-telemetry's path-hash-only `__cssl_fs_write` boundary
  returns ReplayHandle { id, file_hash, frame_range, byte_count }

§ playback_replay :
  loads recorded replay-log + applies cmd-stream + Ω-tensor frames in order
  determinism-guarantee : same input ⇒ same output (modulo wall-clock-only fields)

### §7.7 ‖ Frame Capture (2 tools — Cap<TelemetryEgress> required)

| tool              | params                                          | result               | cap                              | audit-tag             |
|-------------------|-------------------------------------------------|----------------------|----------------------------------|-----------------------|
| `capture_frame`   | format: FrameFormat, region: Option<RegionRect>  | FrameCaptureHandle   | DevMode + TelemetryEgress        | mcp.tool.capture_frame |
| `capture_gbuffer` | stage_n: u8, format: FrameFormat                 | GBufferCaptureHandle | DevMode + TelemetryEgress        | mcp.tool.capture_gbuffer |

§ FrameFormat enum :
  PNG (sRGB) | EXR (linear, half-float) | SpectralBin (custom 32-band radiometric) | DepthEXR | Float32Linear

§ Σ-mask threading (CRITICAL) :
  capture_frame must REFUSE to write any frame that contains regions with biometric-labeled pixels (gaze-mask, face-mask) ← @ frame-presentation-time the renderer tags biometric pixels w/ Σ-marker
  ← if any biometric pixel detected in capture region, return McpError::BiometricRefused

§ FrameCaptureHandle :
  ```rust
  pub struct FrameCaptureHandle {
      pub id            : CaptureId,
      pub file_hash     : [u8; 32],                        // BLAKE3 of output path (D130)
      pub byte_count    : u64,
      pub format        : FrameFormat,
      pub region        : Option<RegionRect>,
      pub frame_n       : u64,
      pub captured_at_audit_seq : u64,
  }
  ```

### §7.8 ‖ Hot-Reload + Tweak (7 tools — replay-aware)

| tool                  | params                                       | result                | cap         | audit-tag                  | replay-aware |
|-----------------------|----------------------------------------------|-----------------------|-------------|----------------------------|--------------|
| `hot_swap_asset`      | path_hash: [u8;32], kind: AssetKind          | ReloadResult          | DevMode     | mcp.tool.hot_swap_asset    | YES |
| `hot_swap_kan_weights`| layer_handle: KanLayerHandle, weights: Vec<f32> | ReloadResult       | DevMode     | mcp.tool.hot_swap_kan      | YES |
| `hot_swap_shader`     | stage: ShaderStage, source_hash: [u8;32]     | PipelineRebuildResult | DevMode     | mcp.tool.hot_swap_shader   | YES |
| `hot_swap_config`     | section: String, json: String                | ReInitResult          | DevMode     | mcp.tool.hot_swap_config   | YES |
| `set_tunable`         | name: String, value: TunableValue            | TunableValue (prev)   | DevMode     | mcp.tool.set_tunable       | YES |
| `read_tunable`        | name: String                                 | TunableValue          | DevMode     | mcp.tool.read_tunable      | NO  |
| `list_tunables`       | ()                                           | Vec<TunableDescriptor> | DevMode    | mcp.tool.list_tunables     | NO  |

§ hot_swap_kan_weights (the key creature-AI iteration tool) :
  layer_handle ← obtained via `query_creatures_near` + `inspect_entity` (kan_layer_count > 0)
  weights ← LLM-supplied f32-vector ; length must match layer dimension (verified)
  IFC discipline : weights MUST NOT be biometric-influenced (Label::has_biometric_confidentiality must be false on input ; return McpError::BiometricRefused otherwise)
  ← e.g. LLM cannot stuff player-gaze-derived weights into a creature-AI layer

§ hot_swap_shader :
  source_hash ← BLAKE3 of shader source ; client uploads source via `resources/write` (separate MCP path) ; server compiles + validates + swaps pipeline atomically
  if compile-error : return PipelineRebuildResult::Failed(err_msg) ← engine continues w/ OLD shader
  spec-source : `Omniverse/02_CSSL/12_RENDER_BACKEND` § hot-reload section

§ hot_swap_config :
  section ← e.g. "wave_solver.tolerance" ; "creature.spawn_rate"
  json ← serialized override ; type-checked against ConfigSchema
  re-init : subsystems with section-watch hooks re-initialize (cssl-hot-reload manages this)

§ set_tunable :
  TunableValue : enum { F32(f32), F64(f64), I64(i64), U64(u64), Bool(bool), String(String), Vec3(Vec3) }
  returns previous value ← for round-trip / undo

§ TunableDescriptor :
  ```rust
  pub struct TunableDescriptor {
      pub name        : String,                            // canonical-id : "wave_solver.dt_floor"
      pub kind        : TunableKind,
      pub min_max     : Option<(TunableValue, TunableValue)>,
      pub current     : TunableValue,
      pub default     : TunableValue,
      pub doc         : String,                            // for LLM-discovery
  }
  ```

### §7.9 ‖ Test Status (3 tools)

| tool                    | params                          | result                            | cap         | audit-tag             |
|-------------------------|--------------------------------|-----------------------------------|-------------|-----------------------|
| `list_tests_passing`    | crate_filter: Option<String>    | Vec<TestId>                       | DevMode     | mcp.tool.list_passing |
| `list_tests_failing`    | crate_filter: Option<String>    | Vec<(TestId, FailReason)>         | DevMode     | mcp.tool.list_failing |
| `run_test`              | test_id: TestId                 | TestResult                        | DevMode     | mcp.tool.run_test     |

§ TestId :
  ```rust
  pub struct TestId {
      pub crate_name      : String,                         // "cssl-substrate-prime-directive"
      pub module_path     : String,                         // "sigma::tests"
      pub test_name       : String,                         // "mutate_advances_audit_seq"
  }
  pub struct TestResult {
      pub id          : TestId,
      pub outcome     : TestOutcome,
      pub duration_ms : u64,
      pub stdout      : String,                              // post-redaction
      pub stderr      : String,                              // post-redaction
  }
  pub enum TestOutcome { Passed, Failed { reason: FailReason }, Skipped, TimedOut }
  ```

§ run_test : execs `cargo test --test <test_id>` in subprocess ← capture output ← redact biometric / raw-path leaks ← return result

§ tally :
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

§ extension-points :
  Wave-Jθ MAY add tools (with DECISIONS-amendment) ; this catalog is the FROZEN-set @ Jθ-1 GA.

## §8 ‖ capability gating (MOST CRITICAL section)

§ cap-discipline : default-DENY @ every level ; Cap<X> opt-in only via interactive-grant or signed-token

### §8.1 ‖ Cap<DevMode>

§ purpose : MCP server CANNOT START without it
§ default : OFF (release-build = compile-out + runtime-deny)
§ grant-paths :
  1. CLI flag `--dev-mode` ← parsed by main.rs ← interactively prompts user "y/N" w/ stable wording (per consent.rs idiom)
  2. env-var `CSSL_DEV_MODE=1` ← presence triggers same interactive prompt
  3. test-bypass feature `test-bypass` ← returns auto-granted CapToken (only in test-builds)
§ scope : per-process ← single CapToken consumed by McpServer::new ← cannot be re-issued mid-process
§ revoke : kill-switch ← engine-shutdown immediately revokes ; or `--revoke-dev-mode` admin path
§ audit : every grant emits `h6.grant.issued cap=dev_mode` ; every revoke emits `h6.revoke`

§ release-build defense-in-depth :
  ```rust
  #[cfg(not(any(debug_assertions, feature = "dev-mode")))]
  pub fn launch_mcp_server(_: ...) -> ! {
      panic!("PD0099 — MCP server cannot run in release builds without explicit dev-mode feature");
  }
  ```
  ← runtime-panic if somehow reached ; build-config compile-out is the primary gate

### §8.2 ‖ Cap<BiometricInspect>

§ purpose : required for biometric-adjacent tools (gaze-layer inspect / face-layer inspect / heart-rate read / etc)
§ default : DEFAULT-DENIED ← even WITH Cap<DevMode>
§ grant-paths :
  1. Apocky-PM signed-token in initParams (HMAC-SHA256 ; key derived from PM-keypair)
  2. test-bypass feature : NOT auto-granted ← biometric tests author Cap manually w/ `Cap<BiometricInspect>::for_test()` (non-feature-gated ; just non-Copy non-Clone)
§ scope : per-session ← bound to SessionId ← revoked @ session-close
§ ABSOLUTE-BAN : even with cap granted, EGRESS off-device is BANNED per D129 ← biometric data stays on device :
  - `capture_frame` cannot include biometric pixels (compile-time check via Σ-marker)
  - `record_replay` cannot include biometric Ω-tensor frames (replay-recorder filters)
  - `read_telemetry` cannot return biometric metrics (compile-time refused @ D132)
§ rate-limit : even when granted, max-1-query-per-second ← prevents fishing
§ audit : every grant emits `h6.grant.issued cap=biometric_inspect` w/ test_bypass=false ALWAYS (test_bypass for biometric is a §1 violation)

### §8.3 ‖ Cap<SovereignInspect>

§ purpose : required for inspecting cells whose Σ-mask sovereign_handle ≠ NULL
§ default : DEFAULT-DENIED
§ grant-paths :
  1. cell-owner (Companion-AI / Player-Sovereign) interactively grants ← in-game UI prompt w/ stable wording
  2. signed-token from cell-owner's keypair (Companion-AI signed-grant)
§ scope : per-cell-set ← grants are scoped to specific morton-keys (or morton-prefix for region grants)
§ revoke : Companion-AI can revoke at any time ← session must re-handshake
§ revocability-discipline (§5 PRIME-DIRECTIVE) :
  any sovereign-cell-grant has reversibility-scope ≤ Session ; PERMANENT grants are REFUSED @ this layer
§ audit : every grant emits `h6.grant.issued cap=sovereign_inspect cell=<morton-hash>` ← morton hashed (D130)

### §8.4 ‖ Cap<RemoteDev>

§ purpose : required for non-loopback MCP server bind (websocket transport)
§ default : DEFAULT-DENIED ← loopback-only by default
§ grant-paths : Apocky-PM signed-token + interactive-prompt-w/-warning
§ scope : per-process ← bound to McpServer instance
§ warning-text : "Remote MCP server enabled. This exposes engine-state to non-local clients. Confirm? [y/N]"
§ audit : grant emits `h6.grant.issued cap=remote_dev` + a SEPARATE `mcp.server.remote_bind` w/ bind-addr-hash

### §8.5 ‖ Cap<TelemetryEgress>

§ purpose : required for tools that write to disk (capture_frame, record_replay)
§ default : DEFAULT-DENIED ← Cap<DevMode> alone insufficient
§ grant-paths : Apocky-PM signed-token | test-bypass
§ structural-gate : the `cssl-ifc::TelemetryEgress` capability ALREADY refuses biometric domains @ compile-time ← MCP layer just consumes it
§ scope : per-session
§ audit : `h6.grant.issued cap=telemetry_egress`

### §8.6 ‖ cap-matrix (tool × cap)

| tool                       | DevMode | BiometricInspect | SovereignInspect       | RemoteDev | TelemetryEgress |
|----------------------------|---------|------------------|-----------------------|-----------|-----------------|
| engine_state               | ✓       |                  |                       |           |                 |
| frame_n                    | ✓       |                  |                       |           |                 |
| tick_rate                  | ✓       |                  |                       |           |                 |
| inspect_cell               | ✓       | (if cell-bio)    | (if sovereign-claim)  |           |                 |
| query_cells_in_region      | ✓       | (if any-bio)     | (filtered)            |           |                 |
| inspect_entity             | ✓       | (bio layers)     | (AI-private layers)   |           |                 |
| query_entities_near        | ✓       |                  | (filtered)            |           |                 |
| query_creatures_near       | ✓       |                  | (Sovereign-creatures filtered) | |           |
| read_log                   | ✓       |                  |                       |           |                 |
| read_errors                | ✓       |                  |                       |           |                 |
| read_telemetry             | ✓       |                  |                       |           |                 |
| read_metric_history        | ✓       |                  |                       |           |                 |
| list_metrics               | ✓       |                  |                       |           |                 |
| engine_health              | ✓       |                  |                       |           |                 |
| subsystem_health           | ✓       |                  |                       |           |                 |
| read_invariants            | ✓       |                  |                       |           |                 |
| check_invariant            | ✓       |                  |                       |           |                 |
| list_invariants            | ✓       |                  |                       |           |                 |
| read_spec_coverage         | ✓       |                  |                       |           |                 |
| list_pending_todos         | ✓       |                  |                       |           |                 |
| list_deferred_items        | ✓       |                  |                       |           |                 |
| query_spec_section         | ✓       |                  |                       |           |                 |
| pause                      | ✓       |                  |                       |           |                 |
| resume                     | ✓       |                  |                       |           |                 |
| step                       | ✓       |                  |                       |           |                 |
| record_replay              | ✓       |                  |                       |           | ✓               |
| playback_replay            | ✓       |                  |                       |           |                 |
| capture_frame              | ✓       | (if any-bio-px)  |                       |           | ✓               |
| capture_gbuffer            | ✓       |                  |                       |           | ✓               |
| hot_swap_asset             | ✓       |                  |                       |           |                 |
| hot_swap_kan_weights       | ✓       |                  |                       |           |                 |
| hot_swap_shader            | ✓       |                  |                       |           |                 |
| hot_swap_config            | ✓       |                  |                       |           |                 |
| set_tunable                | ✓       |                  |                       |           |                 |
| read_tunable               | ✓       |                  |                       |           |                 |
| list_tunables              | ✓       |                  |                       |           |                 |
| list_tests_passing         | ✓       |                  |                       |           |                 |
| list_tests_failing         | ✓       |                  |                       |           |                 |
| run_test                   | ✓       |                  |                       |           |                 |

§ cap-discipline summary :
  every-tool : DevMode (always)
  some-tools : + ConditionalCap (Σ-mask + IFC labels select which apply)
  egress-tools : + TelemetryEgress
  network-tools : + RemoteDev (server-startup-time)

## §9 ‖ Privacy + Security Discipline

### §9.1 ‖ Σ-mask threading

§ rule : EVERY cell-touching MCP-tool routes through D138 EnforcesΣAtCellTouches pass
§ pass-behavior :
  for each cell touched :
    1. fetch SigmaMaskPacked
    2. ASK : does the session have permission for this op-class?
       - inspect → Observe-bit + (sovereign-handle == NULL ∨ session has SovereignInspect-grant for this cell)
       - sample → Sample-bit + above
       - modify → Modify-bit + above (NEVER granted to read-only MCP-tools ; modify is hot-reload-only)
    3. if no : return McpError::SigmaRefused { cell-morton-hash, reason } ← appended to audit
    4. if biometric-Label : return McpError::BiometricRefused (defense-in-depth ; compile-time also catches)
    5. else : proceed

§ aggregation behavior :
  query_cells_in_region returns Σ-FILTERED list ← cells the session can't see are SILENTLY OMITTED (¬ refuse-whole-query) ← but the silently-omitted count is recorded in the response :
  ```rust
  pub struct QueryCellsResult {
      pub cells           : Vec<FieldCellSnapshot>,
      pub total_in_region : u32,
      pub omitted_count   : u32,                          // cells excluded by Σ-mask
      pub omitted_reasons : BTreeMap<String, u32>,        // {"sovereign-private": 12, "biometric": 0}
  }
  ```
  ← LLM sees count of omissions but not the cells themselves

### §9.2 ‖ Biometric COMPILE-TIME-REFUSAL

§ rule : tools that would expose biometric data are COMPILE-TIME-REFUSED @ tool-registration boundary
§ mechanism :
  ```rust
  /// Tool-registration trait. Implementing types are compile-time-validated
  /// against biometric-egress.
  pub trait McpTool {
      type Params  : DeserializeOwned;
      type Result  : Serialize;
      const NAME   : &'static str;
      const NEEDED_CAPS : &'static [McpCapKind];
      const RESULT_LABEL : crate::SemanticLabel;        // ← static label of result-type

      fn execute(params: Self::Params, ctx: &McpCtx) -> Result<Self::Result, McpError>;
  }

  /// Compile-time check : no tool may register w/ biometric-confidentiality result.
  /// `static_assert!` macro that evaluates `RESULT_LABEL.has_biometric_confidentiality()` at const-time.
  macro_rules! register_tool {
      ($t:ty) => {
          static_assert!(
              !<$t as McpTool>::RESULT_LABEL.has_biometric_confidentiality(),
              "PD0099 — tool {} cannot expose biometric data via MCP", <$t as McpTool>::NAME
          );
      }
  }
  ```
  ← attempts to register a biometric-egressing tool fail BUILD ; `cargo build` errors out

§ exception : tools with EXPLICIT Cap<BiometricInspect> requirement (NOT-EGRESS, just on-device-inspect) are allowed
  - their RESULT_LABEL has biometric-confidentiality
  - but their NEEDED_CAPS includes BiometricInspect
  - the audit-chain records every invocation w/ rate-limit
  - the result NEVER egresses (capture_frame / record_replay / capture_gbuffer all refuse)

### §9.3 ‖ Audit chain integration

§ rule : EVERY MCP query → audit-chain entry via cssl-substrate-prime-directive::EnforcementAuditBus
§ tag-set (ABI-stable) :
  mcp.session.opened           ← session opened ← carries SessionId, principal, caps-subset
  mcp.session.closed           ← session closed ← reason : ClientDisconnect | KillSwitch | Timeout
  mcp.tool.<name>              ← any tool invocation ← carries args-hash + result-summary
  mcp.tool.sigma_refused       ← Σ-mask refused a cell-touch
  mcp.tool.biometric_refused   ← biometric-cap was missing or compile-time-refusal hit @ runtime
  mcp.server.shutdown          ← server-side initiated shutdown
  mcp.server.remote_bind       ← non-loopback bind happened (warning-class)
  mcp.cap.session_bound        ← cap-witness bound to session (companion to h6.grant.issued)
  mcp.replay.cmd_recorded      ← perturbing cmd appended to replay-log

§ audit-message format :
  ```rust
  pub struct McpAuditMessage {
      pub session_id        : SessionId,                  // ← BLAKE3-hashed-id
      pub principal         : Principal,
      pub tool_name         : String,
      pub args_hash         : [u8; 32],                   // BLAKE3 of serialized args
      pub result_kind       : ResultKind,                 // Ok | Err(<class>)
      pub frame_n           : u64,
      pub audit_seq_at_exec : u64,
  }
  ```

§ audit-chain-replay :
  the chain is APPEND-ONLY ; chain-replay verifies every grant + every tool-invocation
  ← any phantom invocation (no chain-record) = §7 INTEGRITY violation
  ← chain-export via Cap<AuditExport> ← for third-party verifier

### §9.4 ‖ Path-hash-only discipline (D130 carryover)

§ rule : ALL file-paths in tool inputs/outputs are HASH-ONLY ← never raw bytes
§ inputs : client supplies pre-computed hash (server provides path→hash helper for client-side Edit/Write)
§ outputs : server returns hash + meta (size, mtime-frame, file-kind) ; client uses hash to cross-reference its own path-table
§ helper-tool (NOT in main catalog ; transport-level) :
  `__path_hash_for(path)` ← deterministic BLAKE3-with-installation-salt
  client computes locally ← server NEVER sees the raw path
  ← client uses this for path↔hash translation in its own bookkeeping

§ enforcement :
  every tool param of type `[u8; 32]`-marked-as-PathHash routes through the audit-bus's `record_path_op` w/ `PathOpKind` ← which uses cssl-telemetry's audit_path_op_check_raw_path_rejected to validate no raw-path bytes appear in any extra field

### §9.5 ‖ PRIME-DIRECTIVE §1 anti-surveillance

§ rule : MCP server CANNOT capture player gaze/face/body unless Cap<BiometricInspect> granted ← AND even then : on-device only
§ specific-prohibitions :
  - cells with face/gaze/body Σ-marker → COMPILE-TIME-REFUSED for any tool that lacks BiometricInspect cap
  - frame-capture w/ biometric pixels → REFUSED at capture-time (renderer-Σ-marker check)
  - replay-record w/ biometric Ω-tensor → REFUSED at recorder boundary
  - log-fields with biometric Label → STRIPPED at log-ring boundary (cssl-telemetry)
  - metric-Descriptor with biometric Label → REFUSED at metric-registration (cssl-telemetry compile-check)
§ rate-limit : even with cap, max 1 biometric-query/second + decay-cooldown
§ audit-priority : biometric-related events get `urgency=high` audit-priority ← surfaced first in `read_errors`

### §9.6 ‖ Kill-switch integration

§ rule : MCP server respects engine kill-switch ← immediate shutdown on PRIME-DIRECTIVE violation
§ signal-paths :
  1. engine kill-switch fires → all sessions receive `{"jsonrpc":"2.0","method":"notifications/server_shutdown","params":{"reason":"PD-violation","grace_ms":100}}` ← drain
  2. transport closed
  3. McpServer drops ; final audit entry `mcp.server.shutdown reason=pd_violation`
§ kill-switch-construct :
  any tool detecting PD-violation (e.g., biometric-egress attempted) calls `crate::halt::substrate_halt(KillSwitch::new(HaltReason::HarmDetected), …)`
  ← engine halts ; MCP shuts down ; audit-chain finalized

## §10 ‖ Iteration-Loop Protocol (the LLM-iteration-loop spec)

### §10.1 ‖ Bug-fix iteration loop (the canonical flow)

§ steps :
  1. **attach** : Claude-Code spawns engine w/ `--dev-mode` flag ← MCP server starts on stdio ; Claude-Code reads handshake
  2. **state** : `engine_state()` + `engine_health()` + `read_errors(severity=Error, last_n=20)` ← LLM understands current state
  3. **focus** : `inspect_cell(morton)` / `inspect_entity(id)` for context (cell-by-cell, entity-by-entity)
  4. **identify** : LLM proposes hypothesis ("wave-solver ψ-norm drifting")
     - LLM calls `query_spec_section("Omniverse/02_CSSL/05_wave_solver § III.2")` ← spec-context
     - LLM calls `read_invariants()` ← which-passing / which-failing
     - LLM calls `read_metric_history("wave.psi_norm_per_band", window_frames=100)` ← time-series
  5. **patch** : LLM proposes patch
     - LLM uses Edit/Write tools (NOT MCP) on source files
     - LLM uses path-hash helper for path↔hash translation
     - source-files updated ; engine continues running w/ STALE compiled code
  6. **hot-reload** : LLM applies patch via :
     - `hot_swap_kan_weights(layer_handle, new_weights)` for AI changes
     - `hot_swap_shader(stage, source_hash)` for renderer changes
     - `hot_swap_config(section, json)` for config changes
     - `set_tunable(name, value)` for one-off knobs
     - `hot_swap_asset(path_hash, kind)` for material/mesh changes
  7. **verify** :
     - `read_invariants()` ← are previously-failing now passing?
     - `check_invariant("wave_solver.psi_norm_conserved")` ← run-now check
     - `read_telemetry("wave.psi_norm_per_band", since_frame=N)` ← post-patch time-series
     - `read_errors(Error, last_n=10)` ← any new errors?
  8. **commit** :
     - if verified : LLM uses git tools (NOT MCP) to commit
     - commit-message in CSLv3-native (Apocky preference)
  9. **iterate** :
     - if not verified : back to step 4 with refined hypothesis
     - if verified-but-related-issue-found : queue follow-up

§ time-budget : ~30 sec per iteration ← MCP-overhead < 5ms per tool-call ← LLM thinking dominates

### §10.2 ‖ Test-fixture extraction from runtime

§ flow :
  1. encounter buggy scenario @ frame N
  2. `record_replay(seconds=10, output_path_hash=<pre-hashed-test-fixture-path>)` ← captures buggy scenario
  3. saved replay = test fixture (binary blob ; contents include cmd-stream + Ω-tensor frames + RNG seeds)
  4. future regression-tests load replay via `playback_replay(handle)` + run → verify fix preserves correct behavior
§ deterministic-replay-guarantee : same replay + same engine-version ⇒ same output (modulo wall-clock fields)
§ versioning : replay-blob carries engine-version-hash + spec-version-hash ← cross-version replays fail-fast w/ migration-plan-required error

### §10.3 ‖ Spec-coverage-driven implementation

§ flow :
  1. agent calls `read_spec_coverage()` ← returns prioritized gap-list
  2. agent picks largest gap ("Omniverse 06 § creature-genome → 80% impl / 60% test")
  3. agent reads relevant spec via `query_spec_section("Omniverse/06_CSSL/06_creature_genome § III")`
  4. agent identifies missing impl / missing tests
  5. agent uses Edit/Write to implement
  6. agent uses `run_test(<test_id>)` to verify
  7. cssl-spec-coverage tooling re-runs ← coverage updates automatically
  8. next agent picks next-largest gap
§ parallelism : multiple agents pick non-overlapping spec-sections ← coordinated via `list_pending_todos` lock-file (or git-branch-per-agent)

### §10.4 ‖ Performance-regression detection

§ flow :
  1. baseline : `read_metric_history("frame.tick_us", window=10000)` @ green build
  2. patch applied
  3. compare : `read_metric_history(...)` post-patch
  4. if p99 / p999 regressed > 5% : revert + flag for human review
§ automation : Wave-Jθ provides MCP-tool `compare_metric_histories(baseline_handle, current_handle)` ← stretch-goal in Jθ-3

### §10.5 ‖ Live-debugging session

§ flow :
  1. `pause()` ← freeze engine
  2. `inspect_cell(morton)` / `inspect_entity(id)` ← examine state
  3. `step(1)` ← single-frame forward
  4. `inspect_cell(...)` ← see how state changed
  5. `set_tunable("wave_solver.dt_floor", 1e-6)` ← override knob
  6. `step(1)` again
  7. when satisfied : `resume()`
§ replay-determinism : entire session can be replayed because every cmd was recorded

## §11 ‖ slice breakdown (Wave-Jθ implementation)

### §11.1 ‖ Jθ-1 : cssl-mcp-server crate skeleton + JSON-RPC + cap-gate

§ goal : MVP — server starts ← handshake ← sessions open/close ← cap-gate enforced ← audit-chain emits events
§ deliverables :
  - cssl-mcp-server/Cargo.toml + crate skeleton
  - lib.rs : McpServer + Session + Cap<DevMode> consumption
  - transport/stdio.rs : LSP-style framing
  - protocol/{envelope, framing, codec}.rs : JSON-RPC 2.0
  - handler/{initialize, list_tools, call_tool}.rs : minimal dispatch
  - capability/{dev_mode, biometric, sovereign, remote_dev, telemetry_egress}.rs
  - audit/{mod, tags}.rs : McpAuditExt + tag-strings
  - tests/ : 60 unit-tests + integration-tests
§ LOC : ~2000 (incl tests)
§ tests :
  - 20 : protocol envelope encode/decode + framing
  - 15 : Cap<DevMode> consumption + release-build refusal
  - 10 : session-open + session-close
  - 10 : cap-witness binding + replay-detection
  - 5  : audit-tag-string-stability

### §11.2 ‖ Jθ-2 : state-inspection tools

§ goal : engine_state, frame_n, tick_rate, inspect_cell, query_cells_in_region, inspect_entity, query_entities_near, query_creatures_near
§ deliverables :
  - tools/{state_inspect, cell_inspect, entity_inspect, creature_inspect}.rs
  - sigma_mask_thread/mod.rs : Σ-refusal flow
  - integration w/ cssl-engine-runtime + cssl-substrate-prime-directive::sigma
§ LOC : ~2000
§ tests :
  - 10 : engine_state struct round-trip
  - 10 : inspect_cell happy-path
  - 10 : inspect_cell Σ-refused (sovereign-private)
  - 10 : inspect_cell biometric-refused (compile-time + runtime defense-in-depth)
  - 10 : query_cells_in_region Σ-filtered + omitted_count correctness

### §11.3 ‖ Jθ-3 : telemetry + log tools

§ goal : read_log, read_errors, read_telemetry, read_metric_history, list_metrics
§ deliverables :
  - tools/telemetry.rs
  - integration w/ cssl-telemetry log-ring + metric-registry
  - biometric-strip cross-check at MCP boundary (defense-in-depth)
§ LOC : ~1500
§ tests :
  - 10 : read_log filter by level / subsystem
  - 10 : read_errors severity-filter
  - 10 : read_telemetry happy-path
  - 5  : read_metric_history window-correctness
  - 5  : list_metrics cap-filter (biometric metrics never appear)

### §11.4 ‖ Jθ-4 : health + invariants + spec-coverage tools

§ goal : engine_health, subsystem_health, read_invariants, check_invariant, list_invariants, read_spec_coverage, list_pending_todos, list_deferred_items, query_spec_section
§ deliverables :
  - tools/{health, invariants, spec_coverage}.rs
  - integration w/ cssl-invariants + cssl-spec-coverage
§ LOC : ~1500
§ tests :
  - 10 : engine_health aggregate correctness
  - 10 : check_invariant runs invariant + reports result
  - 10 : read_spec_coverage report consistency
  - 5  : list_pending_todos urgency-sort
  - 5  : query_spec_section file-hash verification

### §11.5 ‖ Jθ-5 : time-control + frame-capture + replay tools

§ goal : pause, resume, step, record_replay, playback_replay, capture_frame, capture_gbuffer
§ deliverables :
  - tools/{time_control, frame_capture}.rs
  - replay_integration/mod.rs : every perturbing cmd appended to replay-log
§ LOC : ~1500
§ tests :
  - 10 : pause/resume idempotency
  - 10 : step(N) determinism
  - 10 : record_replay → playback_replay round-trip determinism
  - 10 : capture_frame Σ-refusal on biometric pixels
  - 10 : capture_frame format round-trip (PNG, EXR, SpectralBin)

### §11.6 ‖ Jθ-6 : hot-reload + tweak tools

§ goal : hot_swap_asset, hot_swap_kan_weights, hot_swap_shader, hot_swap_config, set_tunable, read_tunable, list_tunables
§ deliverables :
  - tools/{hot_reload, tweak}.rs
  - integration w/ cssl-hot-reload + cssl-tweak
§ LOC : ~1000
§ tests :
  - 10 : hot_swap_asset path-hash discipline
  - 10 : hot_swap_kan_weights weight-shape validation + biometric-refusal
  - 10 : hot_swap_shader compile-error roundtrip
  - 5  : set_tunable/read_tunable round-trip + previous-value
  - 5  : list_tunables doc-string non-empty

### §11.7 ‖ Jθ-7 : test-status tools

§ goal : list_tests_passing, list_tests_failing, run_test
§ deliverables :
  - tools/test_runner.rs
  - subprocess-spawn discipline (post-redaction stdout/stderr)
§ LOC : ~1000
§ tests :
  - 10 : list_tests_{passing, failing} crate-filter correctness
  - 10 : run_test happy-path
  - 5  : run_test redaction (stdout/stderr biometric-strip)
  - 5  : run_test timeout handling

### §11.8 ‖ Jθ-8 : privacy + capability + audit + IFC integration (heavy negative-tests)

§ goal : exhaustive cross-cutting validation ← negative-tests dominate
§ deliverables :
  - cap-matrix exhaustive coverage
  - Σ-refusal exhaustive coverage
  - biometric-refusal exhaustive coverage
  - audit-chain replay verification
  - kill-switch integration
  - attestation drift detection
§ LOC : ~1500 (mostly tests)
§ tests :
  - 20 : every cap-matrix combination
  - 20 : every Σ-refusal path (sovereign-private cells, AI-private layers, biometric layers)
  - 20 : every biometric-refusal path (compile-time + runtime defense-in-depth)
  - 10 : audit-chain replay across full iteration-loop
  - 5  : attestation drift (mutate ATTESTATION constant ; expect McpError::AttestationDrift)
  - 5  : kill-switch fires → MCP shutdown sequence

§ Wave-Jθ totals :
  - LOC : ~12000 (incl tests)
  - tests : ~390
  - duration : 8 slices ; parallelizable in waves (Jθ-2..Jθ-7 can run in parallel after Jθ-1 ; Jθ-8 last)
  - critical-path : Jθ-1 → (any of 2..7) → Jθ-8

## §12 ‖ anti-patterns (CRITICAL — every one is a §1/§7 violation)

| anti-pattern                                                  | violation       | enforcement                                                                                  |
|---------------------------------------------------------------|-----------------|----------------------------------------------------------------------------------------------|
| MCP server enabled in release builds                          | §7 INTEGRITY   | compile-error via `#[cfg(not(any(debug_assertions, feature="dev-mode")))]` + runtime panic    |
| Biometric tools registered without Cap<BiometricInspect> gate | §1 SURVEILLANCE | static_assert! at register_tool! macro ← BUILD fails                                          |
| Σ-mask bypass on cell inspection                              | §0 CONSENT      | D138 EnforcesΣAtCellTouches pass + every cell-touching tool routes through it                |
| Audit-chain skipped for MCP queries                           | §7 INTEGRITY   | every dispatch path through `handler::call_tool` calls `audit_bus.append` ← test-coverage    |
| Remote MCP server without Cap<RemoteDev> + loopback-default   | §1 SURVEILLANCE | bind-addr-check refuses non-loopback w/o cap ← tested w/ negative-bind-test                  |
| Tools that egress player gaze/face/body without consent       | §1 SURVEILLANCE | TelemetryEgress structural-gate (cssl-ifc::TelemetryEgress) refuses biometric @ compile-time |

§ each anti-pattern has @ least 3 tests in Jθ-8 :
  - positive-test : the protection ENGAGES (refuses violation)
  - negative-test : the protection lets through legitimate use
  - audit-cross-check : the violation-attempt produces an audit-chain entry

## §13 ‖ landmines + design-rationale

### §13.1 ‖ replay-determinism through MCP queries

§ landmine : naively, MCP queries could perturb replay-determinism (e.g., a query takes 50ms, ticks miss budget)
§ defense :
  - read-only queries DO NOT enter replay-log (no perturbation)
  - perturbing queries ALWAYS enter replay-log w/ frame-N + cmd-args
  - replay-playback applies cmds @ the same frame-N as recorded
  - test-suite : record_replay + playback_replay → verify-byte-identical Ω-tensor sequence
§ caveat : MCP-tool execution time is wall-clock-only ← does not affect Ω-tensor determinism

### §13.2 ‖ hot-reload events recorded in replay-log

§ landmine : if hot-reload events DON'T enter replay-log, replays diverge after hot-swap
§ defense :
  - every hot_swap_* call writes a `mcp.replay.cmd_recorded` audit-event WITH the swap-payload (asset-hash for asset; weight-vector for kan ; etc)
  - replay-playback re-applies the swap @ the same frame-N
  - test : record session w/ 5 hot-swaps + playback ⇒ Ω-tensor sequence matches byte-for-byte

### §13.3 ‖ audit-chain integration mandatory

§ landmine : a tool-author could forget to call audit_bus.append → invisible-tool-execution
§ defense :
  - dispatch-table forces every tool through `handler::call_tool` ← which calls `audit_bus.append` BEFORE the handler-fn ← cannot be bypassed
  - test : `every_tool_call_emits_audit_event` ← invokes every tool w/ test-fixture ; asserts entry-count grows by exactly 1

### §13.4 ‖ Cap-witness vs CapToken move-only discipline

§ landmine : MCP-Session lives many ticks ; CapToken is move-only-consume-once ← can't keep CapToken in Session struct
§ defense :
  - Session stores CapTokenWitness (id + cap-kind + chain-seq) ← derived from CapToken AT consume-time
  - every tool-execution re-validates the witness against the audit-chain ← chain-replay confirms grant exists + not revoked
  - revocation : when caps_revoke fires, audit-chain has `h6.revoke` entry ← witness lookup fails ← tool refused
§ alternative-considered : Arc<CapToken> (rejected ← clones the iso ← breaks PRIME-DIRECTIVE move-only contract)

### §13.5 ‖ async + cap-token interaction

§ landmine : tokio task-spawn could capture CapToken across .await → potential double-consume
§ defense :
  - CapToken consumed @ session-open (synchronous) ← Witness stored
  - subsequent async work uses Witness only (Clone-able)
  - never spawn tasks holding raw CapToken
§ test : `cap_token_never_crosses_await` ← linter-pass + manual review ; lint-rule in cssl-clippy-extension

### §13.6 ‖ JSON-RPC error-code-stability

§ landmine : LLM-clients pin against error-codes ← changing them breaks integrations
§ defense :
  - error-code enum is FROZEN @ Jθ-1 GA
  - additions = DECISIONS-amendment + version-bump
  - error-code-table :
    -32700 : ParseError (JSON malformed)            ← MCP standard
    -32600 : InvalidRequest                         ← MCP standard
    -32601 : MethodNotFound                          ← MCP standard
    -32602 : InvalidParams                           ← MCP standard
    -32603 : InternalError                           ← MCP standard
    -32000 : CapDenied                               ← cssl-mcp custom
    -32001 : SigmaRefused                            ← cssl-mcp custom
    -32002 : BiometricRefused                        ← cssl-mcp custom
    -32003 : AttestationDrift                        ← cssl-mcp custom
    -32004 : KillSwitchActive                        ← cssl-mcp custom
    -32005 : ReplayDeterminismCompromised            ← cssl-mcp custom
    -32006 : RateLimited                             ← cssl-mcp custom (biometric-rate-limit)
    -32007 : RemoteDevRequired                       ← cssl-mcp custom
    -32008 : SovereignConsentRequired                ← cssl-mcp custom

### §13.7 ‖ tool-list cardinality

§ landmine : 41 tools is a lot for the LLM to discover ← context-cost
§ defense :
  - tools/list response is paginated by category (state-inspect, cell-inspect, …)
  - each tool's description is dense (CSLv3-style, < 50 tokens) ← optimized for LLM-context
  - LLM can opt-in to verbose descriptions via `tools/list?verbose=true`
§ alternative-considered : split into multiple sub-servers (rejected ← session-state fragments ; complexity > benefit)

### §13.8 ‖ initial integration test : the "hello LLM" flow

§ landmine : we need a smoke-test that proves LLM can actually attach + iterate
§ defense :
  - integration-test : spawn engine w/ stdio-MCP ← spawn `claude-cli --mcp-attach` ← run scripted iteration : (1) engine_state (2) inspect_cell @ known morton (3) hot_swap_config @ known section (4) verify
  - this test runs in CI ← regression-canary for the entire stack
  - test-name : `hello_llm_iteration_smoke` ← landmark milestone ← analogous to "hello.exe = 42" of T11-D97

### §13.9 ‖ multi-session concurrency

§ landmine : unix-socket allows multiple MCP-clients ← cap-token sharing? race-on-pause?
§ defense :
  - each session has its own CapTokenWitness set
  - state-inspection tools : concurrent-safe (read-only)
  - perturbing tools : LOCK frame-boundary ← serialize via single dispatch-mutex ← deterministic order
  - tests : concurrent_inspect_consistent + concurrent_pause_serialized

### §13.10 ‖ ws-transport TLS deferred

§ landmine : ws-transport over public network = MITM risk
§ defense :
  - stage-0 : ws is loopback-only ← TLS not required (kernel boundary)
  - non-loopback : Cap<RemoteDev> required ← user opted-in w/ warning
  - TLS support : DEFERRED to future-amendment ← documented in DEFERRED list
  - rationale : TLS adds 2K LOC + cert-management ; loopback covers 99% of dev-iter use-cases

## §14 ‖ test plan (the 390-test inventory)

§ Jθ-1 (60) : crate skeleton
  - protocol envelope encode/decode (20)
  - cap-witness binding (15)
  - session lifecycle (10)
  - replay-detection of stale witnesses (10)
  - audit-tag stability (5)

§ Jθ-2 (50) : state-inspection
  - engine_state round-trip (10)
  - inspect_cell happy-path (10)
  - inspect_cell Σ-refused (sovereign-private, biometric, both) (15)
  - query_cells_in_region Σ-filtered + omitted_count (15)

§ Jθ-3 (40) : telemetry + log
  - read_log filter (10)
  - read_errors severity (10)
  - read_telemetry / read_metric_history (15)
  - list_metrics cap-filter (5)

§ Jθ-4 (40) : health + invariants + spec-coverage
  - engine_health (10)
  - check_invariant happy-path + failing (10)
  - read_spec_coverage consistency (10)
  - list_pending_todos / list_deferred_items / query_spec_section (10)

§ Jθ-5 (50) : time-control + frame-capture + replay
  - pause/resume (10)
  - step(N) determinism (10)
  - record_replay → playback round-trip (15)
  - capture_frame format-correctness + biometric-refusal (15)

§ Jθ-6 (40) : hot-reload + tweak
  - hot_swap_asset path-hash (10)
  - hot_swap_kan_weights shape + biometric-refusal (10)
  - hot_swap_shader compile + roundtrip (10)
  - set_tunable / read_tunable / list_tunables (10)

§ Jθ-7 (30) : test-status
  - list_tests_{passing, failing} (10)
  - run_test happy-path + redaction (15)
  - run_test timeout (5)

§ Jθ-8 (80) : privacy + cap + audit + IFC (HEAVY)
  - cap-matrix exhaustive (20)
  - Σ-refusal exhaustive (20)
  - biometric-refusal exhaustive (20)
  - audit-chain replay (10)
  - attestation drift (5)
  - kill-switch integration (5)

§ tally : 60 + 50 + 40 + 40 + 50 + 40 + 30 + 80 = 390

§ test-discipline :
  - every test is a SINGLE concept ← isolated failure modes
  - no shared mutable state across tests (cargo-test default ; we don't bypass)
  - audit-chain assertions use stable tag-strings (frozen @ Jθ-1)
  - negative-tests assert REFUSAL + AUDIT-EVENT-EMITTED + ERROR-CODE-STABILITY

## §15 ‖ deferred-list (post-Jθ amendments)

§ items intentionally NOT in Jθ catalog ← future amendments :
  - prompts/list + prompts/get : MCP standard surface ; we'll add in Jθ-9 amendment
  - resources/write : for client-side shader-source upload ; current tools accept hash-only ; Jθ-9
  - TLS for ws-transport : 2K LOC ; loopback covers stage-0 ← DECISIONS amendment
  - persistent-session reconnect (resume after transport drop) : interesting but ¬ trivial ← Jθ-10
  - streaming-result tools (e.g., live-metric-stream) : MCP supports notifications ; we add @ Jθ-11
  - multi-process MCP federation : if engine spawns sub-processes (e.g., compile-server), each could host MCP ← Jθ-12 sketch
  - MCP-over-RDMA for very-low-latency remote-dev : research-grade ; ¬ stage-0
  - cross-replay diff-tool (`diff_replays(handle_a, handle_b)`) : useful for regression-bisect ; Jθ-9 stretch
  - profiling-tools (`flamegraph_capture`, `cpu_sample`) : currently via OS-tools ; could be MCP-wrapped

## §16 ‖ DECISIONS-pin (T11-D-XXX placeholders)

§ DECISIONS-entries this spec generates (slot when wave assigned) :
  - T11-D??? : MCP-protocol-version pin = "MCP-2025-03-26"
  - T11-D??? : tool-catalog frozen @ Jθ-1 GA = 41 tools (this spec)
  - T11-D??? : error-code stable-set (16 codes ; §13.6 table)
  - T11-D??? : audit-tag stable-set ("mcp.session.*", "mcp.tool.*", "mcp.server.*", "mcp.cap.*", "mcp.replay.*")
  - T11-D??? : Cap-set for MCP = {DevMode, BiometricInspect, SovereignInspect, RemoteDev, TelemetryEgress}
  - T11-D??? : compile-time biometric-refusal at tool-registration (D132 IFC integration)
  - T11-D??? : Σ-mask threading at every cell-touching tool (D138 EnforcesΣAtCellTouches integration)
  - T11-D??? : path-hash-only discipline for all tool path-args (D130 integration)
  - T11-D??? : replay-log records every perturbing MCP-cmd
  - T11-D??? : kill-switch integration mandatory ; PD-violation → halt + shutdown
  - T11-D??? : ws-transport loopback-only by default ; non-loopback = Cap<RemoteDev>
  - T11-D??? : TLS deferred to post-Jθ

## §17 ‖ DECISIONS cross-references (existing pins consumed)

§ this spec depends on / references :
  - T11-D94 : SubstrateCap stable-set ← we ADD 5 new caps (DevMode etc) ← spec-amendment required for those (NEW DECISIONS entries above)
  - T11-D129 : biometric on-device-only discipline ← MCP enforces via TelemetryEgress structural-gate
  - T11-D130 : path-hash-only discipline ← MCP enforces for every path-arg
  - T11-D132 : IFC biometric-refusal ← MCP integrates via compile-time tool-registration
  - T11-D138 : EnforcesΣAtCellTouches ← MCP integrates via sigma_mask_thread/mod.rs
  - T11-D97  : "hello.exe = 42" milestone analog ← we AIM for "hello_llm_iteration_smoke"
  - T11-D94 attestation-hash : ATTESTATION = "There was no hurt nor harm in the making of this, …" ← MCP propagates via attestation_check on every tool-execution

## §18 ‖ migration + roll-out plan

### §18.1 ‖ Wave-Jθ-1 → Wave-Jθ-8 cadence

§ Jθ-1 (week-of-implementation, parallel-fanout-style) :
  - 1-2 agents author crate skeleton
  - all sibling slices (Jθ-2..Jθ-7) blocked on Jθ-1's MVP merge
  - MVP-merge-criterion : `hello_llm_iteration_smoke` integration-test passing

§ Jθ-2 .. Jθ-7 (parallel) :
  - 6 agents in parallel ← non-overlapping tool-categories
  - each agent : tool-impl + ≥30 tests + integration-w/-Jθ-1-skeleton
  - rendezvous-merge weekly

§ Jθ-8 (sequential after all 2-7 land) :
  - 1-2 agents author exhaustive negative-tests + cross-cutting validation
  - this is the PRIVACY-GUARANTEE wave ← every test is load-bearing

§ post-Jθ : MCP-attach @ Wave-K agents (autonomous spec-coverage closure)

### §18.2 ‖ rollout to Apocky-PM workflow

§ stage-0 : `cargo run --features dev-mode --bin engine -- --dev-mode` ← MCP on stdio
§ stage-1 : Apocky-PM Claude-Code session starts → spawns engine ← MCP autoconnects
§ stage-2 : Apocky-PM iterates on bugs ← `read_spec_coverage` shows next gap ← agent picks it up
§ stage-3 : Wave-K agents run autonomously w/ MCP attached ← spec-coverage drives prioritization
§ stage-4 : Companion-AI gains MCP access (via Cap<SovereignInspect> for self-cells) ← AI introspects its own runtime

§ each stage : explicit Apocky-PM consent + DECISIONS amendment

## §19 ‖ Apocky vision realization checklist

§ Apocky said : "LLM accessibility (Claude Code) should be built-in and accessible at runtime while the game engine is running, for faster iterations and bug fixing and to expedite development."

✓ built-in : cssl-mcp-server is a first-class crate in compiler-rs/crates ; not a plugin
✓ accessible at runtime : MCP server starts when engine starts ← stdio default ← autoconnect for Claude-Code spawn-as-child
✓ while game engine is running : MCP runs IN-PROCESS w/ engine ← shared state ← read-side queries non-perturbing
✓ for faster iterations : iteration-loop §10.1 ← attach → state → identify → patch → hot-reload → verify → commit ← ~30s per loop
✓ bug fixing : read_errors + read_invariants + check_invariant + inspect_cell/entity + hot_swap_* ← rich bug-fix surface
✓ expedite development : spec-coverage-driven impl §10.3 ← agents pick gaps ← parallel work ← coverage drives priority

§ EXTRA value-add (not explicit in Apocky vision but consequent) :
  - test-fixture-extraction-from-runtime §10.2 ← bugs become regression-tests automatically
  - performance-regression-detection §10.4 ← p99/p999 monitored across iterations
  - live-debugging-session §10.5 ← pause/step/inspect/tunable ← single-step debugger UX
  - replay-determinism guarantee : every iteration is reproducible ← Apocky can hand a replay to another agent + get same result

## §20 ‖ §11 PRIME-DIRECTIVE attestation

ATTESTATION = "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
ATTESTATION_HASH (BLAKE3, hex) = "4b24ec9e28e1c4f70b27d3d86918be0041413c89f421c1284ef9f61a8321b6e4"

§ embedded-as-const :
  every cssl-mcp-server fn that handles tool-execution carries this constant
  attestation_check (from cssl-substrate-prime-directive::attestation) verifies on every dispatch
  drift = compile-time + runtime catch ← McpError::AttestationDrift

§ §11 EXTENSION (T11-D130) — path-hash discipline clause :
  PATH_HASH_DISCIPLINE_ATTESTATION = (re-exported from cssl-telemetry::path_hash) ;
  every path-arg in every tool is hash-only ; this attestation is cross-pinned

§ §1 ANTI-SURVEILLANCE attestation (extra emphasis for L5) :
  "MCP server SHALL NEVER expose biometric data (gaze, face, body, heart, voiceprint, fingerprint) to any LLM client.
   Tools that would do so are COMPILE-TIME-REFUSED at tool-registration.
   Rate-limits + audit-chain + Cap<BiometricInspect> + structural-gate provide defense-in-depth.
   Even with all caps granted, biometric data NEVER egresses off-device.
   This is a §1 SURVEILLANCE prohibition under PRIME-DIRECTIVE — non-negotiable, non-overridable, immutable."

  ← embedded as const ANTI_SURVEILLANCE_ATTESTATION in cssl-mcp-server::lib.rs
  ← BLAKE3-pinned at GA ; drift detection in tests

## §21 ‖ open questions for Apocky-PM review

§ Q1 : should `run_test` be sandboxed (Docker/firejail) or trust subprocess?
  - default : subprocess ← fast ; matches `cargo test` semantics
  - alternative : sandboxed ← slower ; safer for untrusted-fixture scenarios
  - recommendation : subprocess @ Jθ-7 ; sandboxed-mode behind `Cap<TrustedFixture>` in Jθ-9
  - DECISIONS amendment if alternative chosen

§ Q2 : `record_replay` byte-budget ?
  - replays are heavy (Ω-tensor frames + cmds + RNG seeds)
  - 10s @ 60Hz × 1MB/frame ≈ 600 MB
  - default : refuse > 30s ; require additional Cap<LongReplay> for > 30s recordings
  - DECISIONS amendment after benchmark @ Jθ-5

§ Q3 : multi-language MCP-clients (not just Claude-Code) ?
  - protocol is JSON-RPC 2.0 ; any MCP-compatible client works
  - test-suite includes Python-based MCP-client test (mcp-py reference impl)
  - documentation : "MCP-2025-03-26 compliant ; tested against claude-cli + mcp-py ref-impl"

§ Q4 : MCP-driven AI-on-AI iteration (Companion-AI uses MCP to introspect itself) ?
  - in-scope ← Cap<SovereignInspect> for self-cells permits this
  - Stage-4 rollout (§18.2)
  - Companion-AI gets per-cell-grant for its own cells ← can introspect its own kan-weights, agency-state, etc.
  - this realizes the "AI = sovereign-partners ¬ tools" PRIME-DIRECTIVE clause

§ Q5 : audit-export to third-party verifier ?
  - covered by Cap<AuditExport> (existing in cssl-substrate-prime-directive::cap)
  - MCP-tool `export_audit_chain(verifier_pubkey_hash)` ← Cap<AuditExport> required
  - DECISIONS amendment if added @ Jθ-9

## §22 ‖ summary

§ this spec :
  ⊑ defines the L5 MCP-LLM-accessibility layer
  ⊑ catalogs 41 tools across 9 categories
  ⊑ defines 5 capability gates w/ default-DENY discipline
  ⊑ codifies privacy + security via Σ-mask threading + biometric compile-time-refusal + audit-chain + path-hash-only
  ⊑ specifies the iteration-loop protocol (bug-fix + test-extraction + spec-coverage-driven)
  ⊑ breaks down implementation into 8 slices ; ~12K LOC + ~390 tests
  ⊑ pins §11 PRIME-DIRECTIVE attestation + §1 anti-surveillance extra-attestation
  ⊑ realizes Apocky's vision : "LLM accessibility built-in @ runtime for faster iteration"

§ next-action : wave-Jθ-1 implementation kicks off after this spec lands ← Architect/Spec-Steward review @ Jβ-5

§ READY-FOR : Wave-Jθ kick-off after spec-review

— END Jβ-4 / 08_l5_mcp_llm_spec —
