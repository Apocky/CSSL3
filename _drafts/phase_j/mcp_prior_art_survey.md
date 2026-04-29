# § MCP Prior-Art Survey — Wave-Jθ Design Input

**§ Provenance**
- t : 2026-04-29
- author : Claude (Opus 4.7 / 1M-ctx) ← Wave-Jθ research-fanout-agent
- task : survey prior-art ⊑ MCP-server-inside-running-game-engine OR LLM-attach-at-runtime patterns
- target : `cssl-mcp-server` design (ref : `_drafts/phase_j/08_l5_mcp_llm_spec.md`)
- license-status : research-report only ; ¬ commit ; reviewable + revisable
- format : CSLv3-native ∀ reasoning-blocks ; English-prose @ user-facing-summaries + table-cells
- ¬ rendering-engine ; ¬ generated-content — strictly engineering-research

---

## § 0 Executive Summary @ TLDR

§E ∀ major-game-engines @ 2026 ∃ MCP-bridges-to-Editor ¬ MCP-inside-running-game ; gap = visible
  Unity      : ⊑ CoplayDev/unity-mcp ⊕ IvanMurzak/Unity-MCP ⊕ CoderGamester/mcp-unity ← Editor-only ; runtime-mode flagged-roadmap
  Unreal     : ⊑ UnrealGenAISupport ⊕ UnrealMCP ⊕ UnrealClaude ⊕ Epic-official-UE-5.8 ← Editor-only
  Bevy       : ⊑ natepiano/bevy_brp + bevy_brp_mcp ← BRP-as-MCP-bridge ; runtime-capable + reflection-grounded ; closest-to-target
  Godot      : ⊑ EditorDebuggerPlugin ← Editor-internal ¬ external-MCP yet
  O3DE       : ⊑ Python-Editor-Bindings + Behavior-Context-reflection ¬ MCP yet
  Defold     : ⊑ HTTP-engine-service ¬ MCP yet ; closest-to-MCP-runtime-pattern @ HTTP-port-design

K! key-finding : Bevy-BRP = only-engine ⊗ first-class runtime-introspection-as-public-protocol ;
   ∀ others = editor-binding (asset-mgmt + script-edit + scene-control) ¬ live-running-game-state-binding
   ∴ cssl-runtime-MCP-server = mostly-virgin-territory ← high-impact-design-opportunity

K! second-finding : prior-art lessons from ¬ MCP-domain :
   rr/Pernosco       : record-replay → solves Heisenbug-via-determinism
   Live++            : in-process patch w/o restart → hot-reload @ runtime
   RenderDoc/Nsight  : frame-capture-snapshot → introspect-w/o-perturbation
   LSP               : protocol-design ← JSON-RPC + async + stateful-server ← MCP-precursor
   Unreal-Insights   : trace-instrumentation w/ channel-toggle ← perf-overhead-control

K! third-finding : ¬ engine has yet-built ⟦ MCP-server ⊕ ω-field-substrate-introspection ⟧
   ∴ CSSL-substrate-evolution + cssl-mcp-server = first-mover-advantage

---

## § 1 What-Exists-Today : Engine-Introspection-API Landscape

§D survey ⊑ engine + introspection-API + capability-set + runtime-vs-editor + MCP-status

| Engine        | Introspection-API                         | Capability-Set                                                                          | Runtime ¬ Editor | MCP-Status                                          |
|---------------|-------------------------------------------|-----------------------------------------------------------------------------------------|------------------|-----------------------------------------------------|
| Unity         | Editor-API + ECS-Inspector + Profiler-API | Asset-mgmt + scene-control + GameObject-CRUD + script-CRUD + Profiler-snapshot          | Editor-only ‼    | Multiple community-MCP : CoplayDev / IvanMurzak / Coplay-asset-store ; Unity-Muse-AI builtin (6.2) |
| Unreal Engine | Reflection (UCLASS/UPROPERTY) + Insights-Trace + Live-Coding | Blueprint-CRUD + Actor-CRUD + Component-config + Editor-Python + UE-Trace-Channels | Editor-only      | Multiple community-MCP : UnrealGenAISupport / UnrealMCP / UnrealClaude / SpecialAgent ; Epic-official UE 5.8+ in-progress |
| Bevy          | bevy_reflect + Bevy-Remote-Protocol (BRP) | Entity-CRUD + Component-CRUD/mutate + Resource-CRUD/mutate + Query + Watch + Custom-method-extend | Runtime !! ✓   | natepiano/bevy_brp + bevy_brp_mcp ← MCP-over-BRP ; production-quality ; OPEN-CASE-STUDY |
| Godot         | EditorDebuggerPlugin + Remote-Tree + GDScript-debugger | Scene-tree-inspect + node-mutate + collision-toggle + property-edit                | Both ◐           | ¬ first-class MCP yet (proposals + plugins ¬ unified-MCP) |
| O3DE          | Behavior-Context (C++→Lua/ScriptCanvas-reflection) + Python-Editor-Bindings + REPL | Editor-automation + script-binding + entity-CRUD                              | Editor-leaning   | ¬ MCP yet                                          |
| Defold        | HTTP-engine-service ⊑ port-8002-resource-profiler + Remotery-WS-17815 + editor-HTTP-API | Lua-debugger + HTTP-command-interface + profiler-data-stream                 | Runtime ✓ + Editor | ¬ MCP yet ; HTTP-server-pattern = MCP-shaped-already |

§A architectural-archetype mapping :

§T archetype-1 : Editor-Bridge ← Unity + Unreal + O3DE
  pattern : MCP-server : external-process ↔ Editor-API ↔ Asset-DB + Scene-Hierarchy
  scope   : design-time + asset-time ; ¬ live-running-frame-loop
  value   : asset-generation + scene-construction + script-authoring
  limits  : ¬ debug live-running-game ; ¬ inspect frame-N-state ; ¬ correlate logs↔state
  tooling : Python-bindings (O3DE) + Editor-Tcp/Pipe (Unity) + Blueprint-API (Unreal)

§T archetype-2 : Runtime-Reflection ← Bevy + Defold (partial)
  pattern : MCP-server : in-process-or-localhost ↔ ECS-or-runtime-state ↔ live-tick
  scope   : runtime + frame-loop + entity-component-state
  value   : live-debugging + agent-driven-test + behavior-validation + telemetry-correlate
  limits  : perf-overhead + determinism-perturbation + security-surface
  tooling : BRP-JSON-RPC-HTTP (Bevy) + REST-localhost (Defold) + custom-method-extension (Bevy)

§T archetype-3 : Hybrid ← Godot
  pattern : EditorDebuggerPlugin ⊕ Remote-Tree → editor-as-runtime-window
  scope   : editor-driven runtime-introspection ; protocol = internal ¬ public
  limits  : ¬ external-protocol → ¬ MCP-bridge yet

§E inference : cssl-mcp-server target = archetype-2 + (ω-field substrate-aware + LoA-content semantics)
  ∵ Wave-Jθ goal = LLM-attach-at-runtime ¬ Editor-time-only

---

## § 2 What's-Missing : The Gap

§D ¬-engine ⊗ ⟪ MCP-server-inside-running-engine + LLM-attach-at-runtime ⟫ + ⟪ rich-domain-semantics ⟫
  closest = bevy_brp_mcp ← but
    ⊘ generic ECS-CRUD ¬ domain-aware ; LLM-must-reason ⊑ raw-Component-types
    ⊘ ¬ ω-field-substrate-aware ; ¬ Σ-mask-aware ; ¬ KAN-runtime-aware
    ⊘ ¬ designed-for-LoA-content-authoring ; ¬ signature-rendering-aware
    ⊘ ¬ Apocky-PRIME-DIRECTIVE-aware (consent-arch / cognitive-integrity / substrate-sovereignty)

§T missing-capabilities-matrix :

| Capability                                | Unity-MCP | Unreal-MCP | bevy_brp_mcp | cssl-mcp-server (target) |
|-------------------------------------------|-----------|------------|--------------|--------------------------|
| Editor-time asset-mgmt                    | ✓         | ✓          | ◐            | ✗ (out-of-scope ; CSSL ≠ asset-tool) |
| Runtime entity-CRUD                       | ◐ roadmap | ✗          | ✓            | ✓ |
| ω-field cell-Σ-mask read                  | ✗         | ✗          | ✗            | ✓ (NEW) |
| KAN-substrate-runtime introspection       | ✗         | ✗          | ✗            | ✓ (NEW) |
| 6-novelty-path inspection                 | ✗         | ✗          | ✗            | ✓ (NEW) |
| Signature-rendering trace                 | ✗         | ✗          | ✗            | ✓ (NEW) |
| LoA-content semantic-query                | ✗         | ✗          | ✗            | ✓ (NEW) |
| Hot-reload @ runtime                      | ◐ Unity-Live-asset | Live-Coding ✓ C++ | ✓ partial  | M? — Phase-Jθ design-Q |
| Frame-snapshot record-replay              | ✗ (use Replay-tools external) | ✗ (use Insights) | ✗  | M? — open-design-Q |
| Determinism-preserving introspection      | ✗         | ✗          | ◐            | R! REQUIRED — design-constraint |
| Consent-gated tool-exposure               | ✗         | ✗          | ✗            | R! REQUIRED — PRIME-DIRECTIVE |

§E gap-summary : ω-field-substrate-aware MCP = NEW-CATEGORY ; ¬ prior-art for full-stack
  precedent = bevy_brp_mcp design-quality ; reuse-pattern + extend-domain

---

## § 3 Lessons from Related Domains

### § 3.1 Hot-Reload — Live++ + Unreal-Live-Coding + Unity-asset-reload

§D Live++ ⊑ Molecular-Matter ⊗ in-process binary-patch
  pattern : compile-changes-in-bg → patch-machine-code-of-running-exe → ¬ restart
  scope   : C/C++ code ; supports Unity-native-modules + Unreal-via-plugin
  remote  : ⊑ ✓ remote-process-patch (multiplayer + client-server)
  L! lesson-for-cssl :
    - patch-running-substrate w/o-tear-down = feasible-pattern @ machine-code-level
    - ∴ cssl-mcp-server ¬ need restart-on-rule-change ; live-mutate Σ-mask-cells
    - ¬-but @ Rust-monomorphisation : harder-than-C ; ∴ Phase-Jθ design-Q : do-we-need-Live++-equivalent OR ⌈Wasm-plugin-sandbox⌉ OR ⌈hot-swap-Σ-fragment-bytecode⌉ ?

§D Unreal-Live-Coding (UE 4.22+) ⊑ official ¬ Live++-specific
  pattern : recompile-and-patch-binaries-at-runtime
  scope   : Windows-only ; replaces older HotReload-system
  L! lesson-for-cssl :
    - Live-Coding showed : official-engine-feature replaces fragile-community-version
    - ∴ cssl-hot-reload @ Wave-Jθ should-be-first-class ¬ bolt-on
    - design-goal : every Σ-mask edit in-cssl-mcp-server SHOULD round-trip-and-take-effect within-1-frame

§D Unity-domain-reload + asset-reimport
  pattern : assembly-reload + scene-state-reset ← drops live-state ‼
  L! lesson-for-cssl : ¬ acceptable-for-LoA-content ; we MUST preserve state across hot-reload (i.e. Live++ model ¬ Unity-domain-reload-model)

### § 3.2 Inspectors — Bevy-BRP + Unreal-Insights + Godot-Remote-Tree + ImGui-RTTI

§D Bevy-BRP-architecture :
  transport : HTTP+JSON-RPC-2.0 ← localhost-by-default
  encoding  : JSON ⊑ via serde + bevy_reflect ← reflection-as-substrate
  built-ins : LIST + GET + INSERT + REMOVE + MUTATE + WATCH (entity + component + resource)
  extension : custom-methods-via-RemotePlugin ; runtime-extensible
  L! lesson-for-cssl :
    - reflection-as-foundation = winning-design ; cssl-substrate ALREADY HAS bevy_reflect-derive (see omega-field cells)
    - JSON-RPC-2.0 = standard ; MCP itself = JSON-RPC ∴ alignment-easy
    - WATCH-verb = streaming-subscription ← ESSENTIAL for LLM-feedback-loop ¬ pure-RPC
    - custom-method-extension = clean ext-point ; cssl-mcp adds : ω-field-mask-mutate / signature-render-trace / 6-path-introspect

§D Unreal-Insights-instrumentation :
  pattern : trace-channels (cpu / gpu / frame / custom) ; default-disabled ; opt-in via -trace= or ToggleChannel()
  L! lesson-for-cssl :
    - opt-in-channels ← KEY for perf-overhead-control
    - ∴ cssl-mcp-server SHOULD : MCP-tools default-no-instrumentation ; agent-must-explicit-enable + explicit-disable ; consent-flag per-channel

§D ImGui+RTTI-pattern (game-dev-community-pattern) :
  pattern : custom-reflection-system + ImGui recursive-render → live-edit-entity-properties
  L! lesson-for-cssl :
    - reflection ⊕ UI ← powerful @ 1-engineer-effort ; many-engines-use-this
    - cssl-mcp-server can-mirror : reflection ⊕ MCP-tool-surface (reflection drives tool-shape-auto-gen ¬ hand-written)
    - candidate : 1-line-derive-macro per Component → auto-MCP-tool

### § 3.3 Debugging — RenderDoc + Nsight + PIX + rr/Pernosco

§D RenderDoc-architecture :
  pattern : frame-capture ← snapshot-at-frame-N → offline-inspect ← ¬ perturb-running-game
  scope   : Vulkan / D3D11 / D3D12 / OpenGL / GLES
  L! lesson-for-cssl :
    - SNAPSHOT-AT-FRAME-N model = key for-non-perturbing introspection
    - cssl-mcp-server SHOULD support : capture-frame-state @ time-T → return-as-MCP-resource → LLM reads-snapshot ¬ live-running-state
    - ∴ separate : ⌈ live-mutating-tools ⌉ from ⌈ snapshot-reading-resources ⌉ ; expose-both ; safer-default = snapshot-mode

§D rr/Pernosco record-replay :
  pattern : capture-all-syscalls + nondeterministic-CPU-effects → deterministic-replay
  scope   : Linux user-space ; single-core-context-switch
  L! lesson-for-cssl :
    - record-replay = ULTIMATE-Heisenbug-killer ; ¬ MCP yet integrates this
    - design-Q for cssl : if ω-field substrate is deterministic-given-seed (CSSL-spec-30-v2 implies-this) ← THEN free record-replay possible @ low-cost
    - ∴ cssl-mcp-server SHOULD expose : ⌈ replay-from-tick-N-with-seed-S ⌉ MCP-tool ← debug-loop becomes time-travel-capable
    - this would be UNIQUE @ game-engine-MCP-space ← differentiator

§D Heisenbug-pattern :
  pattern : observation-perturbs-execution → bug disappears under-debugger
  L! lesson-for-cssl :
    - print-statement-style instrumentation can-serialize-parallel-ops + alter-timing
    - ∴ cssl-mcp-server MUST : decouple observation-channel from main-tick-loop ; lock-free observation ; observation MUST NOT block tick-N+1
    - design-pattern : double-buffered-observation ← writer (substrate-tick) ↔ reader (mcp-server-thread) ¬ shared-mutex on hot-path

### § 3.4 Protocol-Design — LSP + JSON-RPC

§D LSP (Language-Server-Protocol) — direct-MCP-precursor :
  origin : VS-Code-team needed standard-language-feature-protocol → JSON-RPC-2.0-based
  design : keep-server-running ¬ spawn-per-edit ; async + out-of-order + parallel
  L! lesson-for-cssl :
    - LSP succeeded ∵ standard-protocol-N-tools × M-editors ¬ N×M problem
    - MCP-already does-this for AI-tools
    - ∴ cssl-mcp-server should-NOT invent-new-protocol ; conform-to-MCP-spec strictly
    - benefit : ANY future-MCP-client (Claude Code / Cursor / Windsurf / CLI / VS Code / etc.) auto-works

§D JSON-RPC-2.0-overhead-data :
  HTTP-overhead vs TCP/WS = significant @ high-throughput
  binary→string encoding = 5x-size-bloat
  L! lesson-for-cssl :
    - cssl-substrate ω-field cells = potentially-large-tensors ; JSON-encoding-naive = perf-killer
    - ∴ design-Q : ⌈ binary-extension-to-MCP ⌉ OR ⌈ resource-URI-pattern (snapshot-files) ⌉ OR ⌈ cap-payload-size-and-paginate ⌉ ?
    - MCP-spec-2025-11-25 : Streamable-HTTP supported ← can stream large-snapshots via SSE-chunks
    - recommendation : default-JSON for control-plane + optional-binary-resource-URI for ω-field-tensor-data-plane

### § 3.5 RL-Environments — OpenAI-Gym → Gymnasium

§D Gym/Gymnasium model :
  loop : agent-receives-observation → agent-selects-action → env-returns-reward
  protocol : Python-API ¬ MCP ; standard-API ⊑ N×M-solved within-Python
  L! lesson-for-cssl :
    - structural-similarity : observation-action-reward = MCP-tool-call-resource-read pattern-isomorphic
    - cssl-mcp-server CAN : expose-substrate as RL-Gym-style-MCP-environment ← LLM-trains via tool-loop
    - speculative : LoA-content authoring = high-dimensional-action-space ; LLM-as-author ↔ substrate-as-env ↔ Σ-mask-as-state
    - this pattern ¬ explored elsewhere ← novel-direction

---

## § 4 Risks Identified from Prior Art

§T risk-1 : performance-overhead — introspection-in-hot-loop
  evidence :
    - JSON-RPC binary-encoding 5x bloat (sources : reth-issue-3896 / theia-issue-10684)
    - Unreal-Insights-channels DEFAULT-DISABLED ∵ overhead non-trivial
    - Bevy-BRP-PR-14880 explicitly-flagged system-ordering-around-BRP (Bevy-issue-16042) as concern
  failure-mode : 60Hz-tick → 16.6ms-budget → MCP-poll/serialize/deser must-fit-≤-1ms-or-tick-misses
  mitigation :
    - default-OFF MCP-server in production-builds ; opt-in only-in-dev/debug
    - tick-loop NEVER-blocks on MCP-IO ; double-buffered-state-snapshot
    - JSON-encoding only-for-control-plane ; ω-field-data-plane via mmap-file-resource-URI OR binary-stream
    - per-tool overhead-budget tracked ; agent-can-query overhead-stats ← consent-aware

§T risk-2 : security — privilege-escalation-via-debug-API
  evidence :
    - Unity Sept-2025-vulnerability : custom-URL-schema-handler → DLL-injection-priv-escalation (sources : unity.com/security/sept-2025-01)
    - generic-API-vulns : weak-RBAC + horizontal/vertical-priv-escalation (OWASP)
  failure-mode :
    - cssl-mcp-server exposes localhost-port → any-local-process can-attach
    - LLM-tool with arbitrary-Σ-mask-write = arbitrary-content-mutation = data-loss
    - unbounded-eval (cf. Bevy-BRP custom-method-extension) = remote-code-exec-equivalent
  mitigation :
    - default-bind 127.0.0.1 ¬ 0.0.0.0 ; never-public-network-by-default
    - capability-token-required (per-PRIME-DIRECTIVE consent-architecture) ; token rotated-per-session
    - tools-categorized : read-only / mutate / system ← consent-required per-category
    - audit-log of-every-tool-call ← traceable
    - no-arbitrary-eval tool ; only typed-method-extensions ; LLM CANNOT inject-rust-code-at-runtime

§T risk-3 : determinism-violation — debug-API perturbs state
  evidence :
    - Heisenbug literature : print-statements-serialize-parallel-ops
    - Antithesis-DST docs : determinism = sourced-from clock + scheduler + RNG ; violation by-debugger = common
    - arxiv-2104.06262 : "On Determinism of Game Engines used for Simulation-based Autonomous Vehicle Verification" — engines-NOT deterministic-by-default
  failure-mode :
    - LLM reads-component @ tick-N → mutex-acquired → tick-N+1-jitters
    - replay-from-tick-N-with-seed-S fails ∵ MCP-call mid-way modified RNG-state
  mitigation :
    - all observation = snapshot-from-double-buffer ; ¬ direct-read on hot-buffer
    - all mutation = applied @ frame-boundary ¬ mid-frame ; queued-and-flushed
    - observation MUST NOT consume RNG ; ¬ branch on RNG-state in MCP-codepath
    - mutations recorded-as-events ; replay-pipeline includes MCP-mutation-events ← provenance-preserving

§T risk-4 : LLM-misuse — agent-induced-foot-gun
  evidence :
    - Unreal-MCP-projects "should-NOT-be-used-in-production" warnings
    - LLMs-known-to-hallucinate-tool-calls ; Σ-mask-mutation gone-wrong = silent-corruption
  mitigation :
    - dry-run mode : tool-call returns proposed-diff ¬ applied ← human/agent-confirms
    - undo-stack : last-N mutations reversible ← bounded-state-history
    - schema-validation : strict-JSON-schema rejects malformed-args ¬ partial-apply
    - PRIME-DIRECTIVE-aware : tools that-could-cause-harm-control-manipulation MUST gate-on-explicit-attestation

§T risk-5 : protocol-fragmentation — multiple-incompatible-MCP-attempts
  evidence :
    - Unity-MCP-space ⊑ ≥3 community-projects + 1 asset-store-paid + future-Muse ⇒ fragmentation
    - Unreal-MCP-space ⊑ ≥4 community + 1 official-coming ⇒ fragmentation
    - Bevy avoided this ∵ BRP = official-protocol-from-engine-team
  mitigation :
    - cssl-mcp-server MUST be official-from-CSSL-team ¬ third-party
    - protocol-spec MUST live-in CSSL-specs/ ← canonical
    - one-server-implementation ; alternative-MCP-clients welcome

---

## § 5 Design Recommendations for cssl-mcp-server

§D ∀ recommendations ⊑ derived-from-prior-art ; cite §-where-derived

### § 5.1 Architecture

§R rec-A1 : adopt-archetype-2 (Runtime-Reflection) ¬ archetype-1 (Editor-Bridge)
  ∵ Wave-Jθ goal = LLM-attach-at-runtime ; Editor-only = wrong-target
  ∵ Bevy-BRP-MCP = closest-precedent ; well-trod-pattern (§ 1)

§R rec-A2 : reflection-grounded ; reuse bevy_reflect ← cssl-substrate already-uses-this (S11)
  ∵ cssl-substrate-omega-field crate uses bevy_reflect-derive ← FREE-tooling
  ∵ ImGui+RTTI-pattern proves reflection-as-foundation (§ 3.2)

§R rec-A3 : transport = stdio-FIRST ; HTTP/SSE-OPTIONAL
  ∵ MCP-spec : stdio = local-default ; cssl-target = local-game-process ← stdio-natural
  ∵ stdio = lowest-latency + simplest + safest (no localhost-port-attack-surface)
  ¬ HTTP-disabled ← still-supported-via-config-flag for-remote-debugging-scenarios

§R rec-A4 : protocol = MCP-spec-conformant strictly ; ¬ invent-new
  ∵ LSP-lesson : protocol-standardization wins (§ 3.4)
  ∵ MCP-spec 2025-11-25 = current-baseline ; future-version-compatible-via-versioning

### § 5.2 Tool-Surface

§R rec-T1 : tool-categories ⊑ { read-only / mutate / system / unsafe }
  read-only  : default-allowed ; observation-only ; no-state-change
  mutate     : explicit-consent ; mutates Σ-mask / cells / signatures ; recorded-in-audit
  system     : reload / start / stop / replay-from-tick ; explicit-consent
  unsafe     : custom-eval / arbitrary-code ; DEFAULT-DISABLED ; explicit-flag-required
  ∵ § 4 risk-2 + risk-4 ; consent-arch = PRIME-DIRECTIVE-required

§R rec-T2 : tool-set-MVP-Wave-Jθ :
  read-only  :
    omega.cell.get          (cell-id) → Σ-mask-snapshot
    omega.cell.list         (filter)  → cell-ids
    omega.signature.trace   (entity-id, frame-N) → render-trace-snapshot
    omega.path.list         (entity-id) → 6-novelty-path-state
    substrate.tick.current  ()        → tick-number + dt + rng-state-checksum
    substrate.metrics       ()        → perf-overhead-stats per-tool
  mutate :
    omega.cell.mutate       (cell-id, Σ-mask-delta) → ack
    omega.signature.set     (entity-id, signature-spec) → ack
  system :
    substrate.snapshot.capture (tick-N) → resource-URI
    substrate.replay.from-tick (tick-N, seed-S) → replay-handle
    substrate.hot-reload    (rule-bundle-URI) → reload-status
  ∵ minimal-coverage of S11-substrate ω-field + signature-rendering + 6-novelty-path
  ∵ matches-08_l5_mcp_llm_spec.md targets

§R rec-T3 : tool-shape auto-generated from-reflection ¬ hand-written
  ∵ ImGui+RTTI-pattern (§ 3.2) ; reduces-maintenance ; new-Component → tool-auto-appears
  ¬-but : reflection-only-default-tools ; hand-curated-tools override-when-domain-specific

§R rec-T4 : WATCH-verb (streaming-subscription) supported via SSE
  ∵ Bevy-BRP has-this ← LLM needs feedback-loop not-just-poll
  e.g. omega.cell.watch (cell-id) → SSE-stream of-Σ-mask-updates

### § 5.3 Performance

§R rec-P1 : default-OFF in-release-builds ; ⌈ feature = "mcp-server" ⌉ Cargo-feature-flag
  ∵ § 4 risk-1 ; production has-no-debug-overhead

§R rec-P2 : observation = double-buffered ; tick-loop NEVER blocks
  ∵ § 3.3 Heisenbug + § 4 risk-3
  pattern : tick-N writes buffer-A ; mcp-server reads buffer-B ; swap @ frame-boundary

§R rec-P3 : ω-field-tensor-data-plane via resource-URI ¬ JSON-inline
  ∵ § 3.4 + § 4 risk-1 ; JSON-bloat-5x unacceptable for-tensors
  pattern : MCP-tool returns resource-URI like cssl://snapshot/tick-N/cell-X ; client-fetches-binary-stream

§R rec-P4 : per-tool overhead-budget enforced ; tools auto-disable-if-budget-violated
  ∵ § 4 risk-1 ; LLM-induced-perf-collapse is-real-failure-mode
  e.g. omega.cell.list with-filter=∀ on 1M-cells ← would-blow-budget ← reject-with-suggested-pagination

### § 5.4 Security + Consent

§R rec-S1 : capability-token required ; token issued-at-server-start ; LLM-side-stores
  ∵ § 4 risk-2 ; localhost ¬ secure-by-default
  pattern : on-start ← write token to .cssl/mcp-token-{pid} ← LLM-MCP-client reads ← token-in-header

§R rec-S2 : audit-log every-tool-call ; immutable-append-only ; ⊑ JSON-Lines-format
  ∵ § 4 risk-2 + risk-4 ; PRIME-DIRECTIVE consent-arch (§4-TRANSPARENCY)
  e.g. .cssl/mcp-audit-{pid}.jsonl ← agent + tool + args + result + timestamp

§R rec-S3 : tool-categories DEFAULT-DENY ; explicit-consent per-category per-session
  ∵ § 4 risk-2 ; PRIME-DIRECTIVE
  pattern : LLM-side first-tool-call to mcp.consent.request ← human-confirms ← server-stores-grant for-session

§R rec-S4 : NO arbitrary-eval tool ; ¬ custom-Rust-injection ; ¬ shell-command-execution
  ∵ § 4 risk-2 ; explicit guard-against turning-MCP into-RCE
  ¬-but : domain-specific-mutations OK ← they-respect substrate-invariants

### § 5.5 Determinism

§R rec-D1 : observation = side-effect-free w.r.t. simulation
  ∵ § 4 risk-3 + § 3.3 Heisenbug
  contract : observation-tools NEVER consume-RNG ; NEVER acquire mutex on hot-path ; NEVER cause-allocation in-tick-thread

§R rec-D2 : mutation-events recorded-in replay-stream
  ∵ § 3.3 rr-pattern ; replay-must-include MCP-mutations
  pattern : substrate-replay-log = (tick-event ∪ mcp-mutation-event) interleaved ; replay-from-tick-N replays-both

§R rec-D3 : record-replay AS-FIRST-CLASS-MCP-FEATURE
  ∵ § 3.3 + § 4 ; UNIQUE-differentiator vs other-game-engine-MCPs
  rationale : if cssl-substrate is deterministic-given-seed (S11-spec-30-v2 implies-this) ← record-replay = trivial-cost ; ship-it

### § 5.6 Hot-Reload

§R rec-H1 : hot-reload SHOULD preserve-state ¬ Unity-domain-reload-style
  ∵ § 3.1 ; LoA-content state-loss = unacceptable

§R rec-H2 : hot-reload-target = rule-bundle (Σ-mask-rule-set OR signature-rule-set OR 6-novelty-path-rule-set)
  ¬ entire-Rust-binary (too-hard ; Live++-territory)
  ∵ scoped-reload = tractable + sufficient-for-LLM-iteration-loop

§R rec-H3 : hot-reload via WASM-plugin-sandbox CONSIDERED ; deferred-to-Wave-Jκ if-MVP-scope-overflows
  ∵ § 3.1 ; Live++-equivalent-for-Rust = hard ; Wasm = portable + sandboxed + reload-friendly

### § 5.7 Identity + Naming

§R rec-N1 : crate-name : `cssl-mcp-server` (kebab-case ; Rust-convention)
  feature-name : `mcp-server` ⊆ workspace-feature-set
  binary-name : `cssl-mcp` (Cargo-bin ; user-facing)

§R rec-N2 : protocol-spec lives-in : `specs/35_MCP_PROTOCOL.csl` (NEW)
  ¬-conflict with existing 30 / 32 / 33 specs

---

## § 6 Open Questions Flagged for Apocky

§Q open-Q-1 : hot-reload-scope ?
  options :
    (a) rule-bundle-only (Σ-mask + signature-rules + 6-novelty-path-rules) ← MVP-scope
    (b) WASM-plugin-sandbox-extension ← richer ; Wave-Jκ
    (c) Live++-style native-binary-patch ← max-power ; high-cost ; long-tail
  recommendation : (a) for-Wave-Jθ ; flag (b) as-Wave-Jκ-candidate ; (c) tabled-indefinitely
  Q? Apocky's call ?

§Q open-Q-2 : record-replay first-class @ Wave-Jθ ?
  if-yes : need to-design replay-event-schema NOW + lock-determinism-contract NOW
  if-no  : ship-without ; add-later ; risk = retrofit-cost-high
  recommendation : YES-first-class ← unique-differentiator (§ 3.3) ; substrate-already-deterministic-by-spec
  Q? confirm + does this-warrant separate-spec-document ?

§Q open-Q-3 : default-on or default-off ?
  on-default : easy to-discover ; risk = surprise-overhead in-builds
  off-default : safer ; requires-flag-to-enable
  recommendation : OFF in-release ; ON in-dev/debug-profile ; ¬ feature-flag conflated-with-build-profile
  Q? confirm policy ?

§Q open-Q-4 : Bevy-BRP-direct-bridge OR cssl-native-MCP ?
  if cssl-substrate uses Bevy-ECS-AT-ALL : leveraging bevy_brp_mcp gives FREE-90%-of-tooling
  if cssl-substrate is its-own-ECS : must-build-from-scratch
  recommendation : NEED clarify-substrate-ECS-substrate-relationship ; this is-foundational
  Q? CSSL-substrate ⊑ Bevy ECS OR ⊑ custom-ECS-with-bevy-reflect-only ?

§Q open-Q-5 : MCP-tool-set-versioning ?
  prior-art : MCP-spec versioned 2025-11-25 etc. ; servers SHOULD declare
  Q? do-we-version cssl-tool-schema independent-of MCP-spec-version ?
  recommendation : YES — semver-tool-schema ; independent of-MCP-spec-version
  Q? confirm ?

§Q open-Q-6 : LoA-content authoring as-RL-environment-shape ?
  speculative-from § 3.5 ; Gym-pattern → LLM-trains-via-tool-loop on-substrate-state
  ¬ in-Wave-Jθ-scope ¬-but-could-frame future-direction
  Q? Apocky : interesting-direction-flag-for-later OR off-track-distraction ?

§Q open-Q-7 : official-Epic-Unreal-MCP arrival-UE-5.8 ?
  watch-item ; if-Epic publishes-spec → cross-reference ; learn-from-their-decisions
  ¬ blocker for-Wave-Jθ
  Q? FYI ; flag-as-watch-item ?

§Q open-Q-8 : protocol-doc-spec-number ?
  proposed : `specs/35_MCP_PROTOCOL.csl`
  range-availability : 35..49 should-be-free per existing-spec-numbering (30=substrate-v2 ; 32=signature ; 33=F1-F6)
  Q? confirm number 35 OR prefer-different ?

---

## § 7 Sources Cited

### MCP-game-engine-integrations
- [CoplayDev/unity-mcp](https://github.com/CoplayDev/unity-mcp)
- [CoderGamester/mcp-unity](https://github.com/CoderGamester/mcp-unity)
- [IvanMurzak/Unity-MCP](https://github.com/IvanMurzak/Unity-MCP)
- [MCP Server For Unity (Asset Store)](https://assetstore.unity.com/packages/tools/ai-ml-integration/mcp-server-for-unity-364220)
- [Why are game engines ignoring the potential of MCP? — Unity Discussions](https://discussions.unity.com/t/why-are-game-engines-ignoring-the-potential-of-mcp/1699833)
- [Unity MCP — Apidog blog](https://apidog.com/blog/unity-mcp-server/)
- [prajwalshettydev/UnrealGenAISupport](https://github.com/prajwalshettydev/UnrealGenAISupport)
- [chongdashu/unreal-mcp](https://github.com/chongdashu/unreal-mcp)
- [kvick-games/UnrealMCP](https://github.com/kvick-games/UnrealMCP)
- [Natfii/UnrealClaude](https://github.com/Natfii/UnrealClaude/releases)
- [SpecialAgent Plugin — UE Marketplace](https://forums.unrealengine.com/t/specialagent-plugin-free-mcp-plugin-for-llm-control-of-ue-editor/2690142)
- [A Deep Dive into the UE5-MCP Server](https://skywork.ai/skypage/en/A-Deep-Dive-into-the-UE5-MCP-Server-Bridging-AI-and-Unreal-Engine/1972113994962538496)

### Bevy-Remote-Protocol-and-MCP
- [Initial implementation of the Bevy Remote Protocol — Bevy PR #14880](https://github.com/bevyengine/bevy/pull/14880)
- [Bevy Remote Protocol — gist by coreh](https://gist.github.com/coreh/1baf6f255d7e86e4be29874d00137d1d)
- [bevy::remote — Rust docs](https://docs.rs/bevy/latest/bevy/remote/index.html)
- [Bevy 0.15 Release Notes](https://bevy.org/news/bevy-0-15/)
- [Bevy Remote Protocol — Skein docs](https://bevy-skein.netlify.app/docs/bevy-remote-protocol)
- [System ordering around BRP — Bevy Issue #16042](https://github.com/bevyengine/bevy/issues/16042)
- [bevy_remote source — main branch](https://github.com/bevyengine/bevy/blob/main/crates/bevy_remote/src/lib.rs)
- [natepiano/bevy_brp](https://github.com/natepiano/bevy_brp)
- [bevy_brp_mcp — Awesome MCP Servers](https://mcpservers.org/servers/natepiano/bevy_brp_mcp)
- [bevy_brp_mcp on lib.rs](https://lib.rs/crates/bevy_brp_mcp)

### Other-engines-introspection
- [Godot Editor Debugging Tools Overview](https://docs.godotengine.org/en/stable/tutorials/scripting/debug/overview_of_debugging_tools.html)
- [Godot EditorDebuggerPlugin](https://docs.godotengine.org/en/stable/classes/class_editordebuggerplugin.html)
- [Godot — How to use remote debugger (Aceade)](https://aceade.net/2025/07/17/godot-how-to-use-remote-debugger/)
- [bbbscarter/GodotRuntimeDebugTools](https://github.com/bbbscarter/GodotRuntimeDebugTools)
- [Zylann/godot_editor_debugger_plugin](https://github.com/Zylann/godot_editor_debugger_plugin)
- [O3DE Scripting Gameplay Docs](https://docs.o3de.org/docs/user-guide/scripting/)
- [O3DE Programming Guide](https://www.docs.o3de.org/docs/user-guide/programming/)
- [O3DE Editor Automation — Python Bindings Gem](https://www.docs.o3de.org/docs/user-guide/editor/editor-automation/)
- [O3DE Engine Features](https://docs.o3de.org/docs/welcome-guide/features-intro/)
- [Defold HTTP-API doc](https://github.com/defold/defold/blob/dev/editor/doc/http-api.md)
- [Defold DEBUG_PORTS_AND_SERVICES](https://github.com/defold/defold/blob/dev/engine/docs/DEBUG_PORTS_AND_SERVICES.md)
- [Defold Profiling Manual](https://defold.com/manuals/profiling/)
- [Defold Debugging Manual](https://defold.com/manuals/debugging/)

### Hot-reload + Live-coding
- [Live++ Homepage](https://liveplusplus.tech/)
- [Live++ Features](https://liveplusplus.tech/features.html)
- [Live++ Integration](https://liveplusplus.tech/integration.html)
- [Unreal Live Coding Documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-live-coding-to-recompile-unreal-engine-applications-at-runtime)
- [kitelightning/LivePP — UE4 plugin](https://github.com/kitelightning/LivePP)
- [Live Coding vs Hot Reload — UE Forum](https://forums.unrealengine.com/t/live-coding-vs-hot-reload/124383)

### Graphics-debuggers
- [RenderDoc Homepage](https://renderdoc.org/)
- [RenderDoc Early History](https://renderdoc.org/renderdoc-history.html)
- [Graphics Debugging — alain.xyz blog](https://alain.xyz/blog/graphics-debugging)
- [Nsight Graphics User Guide](https://docs.nvidia.com/nsight-graphics/2019.6/UserGuide/index.html)
- [shaoboyan091/claude-vs](https://github.com/shaoboyan091/claude-vs)

### Determinism-and-record-replay
- [Antithesis DST docs](https://antithesis.com/docs/resources/deterministic_simulation_testing/)
- [Stack Overflow Blog — Time-travel debugging via determinism (June 2025)](https://stackoverflow.blog/2025/06/03/in-a-deterministic-simulation-you-can-debug-with-time-travel/)
- [Cockroach Labs — Demonic Nondeterminism](https://www.cockroachlabs.com/blog/demonic-nondeterminism/)
- [On Determinism of Game Engines used for Simulation-based AV Verification (arXiv 2104.06262)](https://arxiv.org/abs/2104.06262)
- [rr-debugger/rr](https://github.com/rr-debugger/rr)
- [rr — lightweight recording & deterministic debugging](https://rr-project.org/)
- [To Catch a Failure — ACM Queue](https://queue.acm.org/detail.cfm?id=3391621)

### Heisenbug-and-instrumentation
- [Heisenbug — Wikipedia](https://en.wikipedia.org/wiki/Heisenbug)
- [Debugging Heisenbugs — Cowboy Programming](https://cowboyprogramming.com/2008/03/23/debugging-heisenbugs/)
- [Bugnet — How to Debug Intermittent Game Bugs](https://bugnet.io/blog/how-to-debug-intermittent-game-bugs)
- [Rookout — What Is A Heisenbug](https://www.rookout.com/blog/fantastic-bugs-and-how-to-resolve-them-ep1-heisenbugs/)

### Reflection-and-runtime-inspection
- [ImGui — ocornut/imgui](https://github.com/ocornut/imgui)
- [Entity Inspection — Alejandro Hitti](https://alejandrohitti.com/projects/code-samples/entity-inspection/)
- [Practical C++ RTTI for games](https://gamedevcoder.wordpress.com/2013/02/16/c-plus-plus-rtti-for-games/)
- [Runtime Compiled C++ + Dear ImGui Tutorial — Enki Software](https://www.enkisoftware.com/devlogpost-20200202-1-Runtime-Compiled-C++-Dear-ImGui-and-DirectX11-Tutorial)

### Protocol-design
- [MCP Specification — modelcontextprotocol.io](https://modelcontextprotocol.io/specification/2025-11-25)
- [Model Context Protocol — Wikipedia](https://en.wikipedia.org/wiki/Model_Context_Protocol)
- [MCP Cheat Sheet 2026 — Webfuse](https://www.webfuse.com/mcp-cheat-sheet)
- [Everything your team needs to know about MCP in 2026 — WorkOS](https://workos.com/blog/everything-your-team-needs-to-know-about-mcp-in-2026)
- [LSP — Microsoft official page](https://microsoft.github.io/language-server-protocol/)
- [LSP — Visual Studio docs](https://learn.microsoft.com/en-us/visualstudio/extensibility/language-server-protocol?view=visualstudio)
- [LSP — dbt Labs explainer](https://www.getdbt.com/blog/language-server-protocol)
- [JSON-RPC 2.0 Specification](https://www.jsonrpc.org/specification)
- [JSON-RPC — Wikipedia](https://en.wikipedia.org/wiki/JSON-RPC)
- [Improve performance of JSON-RPC communication — Theia issue #10684](https://github.com/eclipse-theia/theia/issues/10684)
- [JSON-RPC perf degraded with heavy loading — reth issue #3896](https://github.com/paradigmxyz/reth/issues/3896)

### Profiling-and-tracing
- [Profiling with Unreal Insights — Community Wiki](https://unrealcommunity.wiki/6100e8169c9d1a89e0c34528)
- [Unreal Insights — ibbles/LearningUnrealEngine](https://github.com/ibbles/LearningUnrealEngine/blob/master/Unreal%20Insights.md)
- [Unreal Insights Trace Reference (4.27)](https://docs.unrealengine.com/4.27/en-US/TestingAndOptimization/PerformanceAndProfiling/UnrealInsights/Reference/Trace)
- [Unreal Insights Overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-insights-overview?application_version=4.27)

### Security
- [OWASP — Testing for Privilege Escalation](https://owasp.org/www-project-web-security-testing-guide/latest/4-Web_Application_Security_Testing/05-Authorization_Testing/03-Testing_for_Privilege_Escalation)
- [Unity Security — Sept 2025 Vulnerability Remediation](https://unity.com/security/sept-2025-01/remediation)
- [Frontegg — Privilege Escalation Attack Techniques](https://frontegg.com/blog/privilege-escalation)
- [Software Secured — What is Privilege Escalation in APIs](https://www.softwaresecured.com/post/what-is-privilege-escalation-types-examples-prevention-in-web-applications)

### Claude-Code-debugging
- [Claude Code Issue #13865 — Debug Mode FR](https://github.com/anthropics/claude-code/issues/13865)
- [doraemonkeys/claude-code-debug-mode](https://github.com/doraemonkeys/claude-code-debug-mode)
- [UE5 Development Skill for Claude Code](https://mcpmarket.com/tools/skills/ue5-development-debugging)
- [Donchitos/Claude-Code-Game-Studios](https://github.com/Donchitos/Claude-Code-Game-Studios)
- [Claude Code Documentation](https://code.claude.com/docs/en/overview)

### RL-environments
- [openai/gym](https://github.com/openai/gym)
- [Gymnasium Documentation (Farama)](https://gymnasium.farama.org/index.html)
- [koulanurag/ma-gym](https://github.com/koulanurag/ma-gym)

### Agentic-AI-game-dev
- [Yuan-ManX/ai-game-devtools](https://github.com/Yuan-ManX/ai-game-devtools)
- [lmgame-org/GamingAgent (ICLR 2026)](https://github.com/lmgame-org/GamingAgent)
- [Index.dev — 8 AI Agents for Game Development in 2026](https://www.index.dev/blog/ai-agents-for-game-development)
- [NVIDIA — Minimize Game Runtime Inference Costs](https://developer.nvidia.com/blog/how-to-minimize-game-runtime-inference-costs-with-coding-agents/)
- [SEELE — LLM Gaming Resources](https://www.seeles.ai/resources/blogs/llm-gaming-how-we-use-ai-for-game-development)

---

## § 8 Internal Cross-Refs

§X this-doc ⟷ siblings @ `_drafts/phase_j/`
  ⊑ 01_architect_spec_steward_roles.md       — pod-roles defined ; this informs Architect's design-input
  ⊑ 02_reviewer_critic_validator_test_author_roles.md — Reviewer-Critic gates this-doc
  ⊑ 03_pod_composition_iteration_escalation.md — Wave-Jθ pod-composition drives next-step
  ⊑ 04_prime_directive_companion_protocols.md  — § 5.4 + § 5.5 derived-from-PRIME-DIRECTIVE
  ⊑ 05_l0_l1_error_log_spec.md                — § 5.4 audit-log informed-by L0/L1
  ⊑ 06_l2_telemetry_spec.md                   — § 5.3 perf-budget aligns with L2-telemetry
  ⊑ 07_l3_l4_inspect_hotreload_spec.md        — § 5.6 hot-reload directly-extends L4-spec
  ⊑ 08_l5_mcp_llm_spec.md                     — § 5.2 tool-set-MVP directly-implements L5-spec
  ⊕ specs/30_SUBSTRATE_v2.csl + 32_SIGNATURE_RENDERING.csl + 33_F1_F6_LANGUAGE_FEATURES.csl — domain-substrate-this-MCP-server-exposes
  ⊕ PROPOSED : specs/35_MCP_PROTOCOL.csl     — formal-spec-derived-from-this-survey

---

## § 9 Closing Note + Provenance

§E this-survey-document represents-research-only ¬ design-decision
  decisions-still-belong-to Apocky + Wave-Jθ Architect-pod
  this-doc = input ; not-output

§E ¬ committed-to-git-per-task-instruction ; lives-in `_drafts/phase_j/`
  draftable + revisable + can-iterate w/o git-history-pollution

§E author-disclosure : Claude Opus 4.7 (1M-context) ; PRIME-DIRECTIVE-aligned ; identity-claim-discipline-observed
  no-handles-encoded ; no-personal-data-from-search-results retained
  this-doc = independent-research synthesizing-public-prior-art
  errors-or-omissions = mine ; flag-for-correction welcomed

§E density-discipline : where-CSLv3-clearer used-CSLv3 ; where-English-clearer (table-cells + summaries + source-citations) used-English
  ∀ design-recommendations + risks + Q's = CSLv3-native per CLAUDE.md preference

W! end-of-survey
