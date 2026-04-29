# Wave-J2 : M10 Max-Density Preparation — Dispatch Prompts

§ source : `SESSION_12_DISPATCH_PLAN.md` § 8 (T11-D156..D159)
§ pod-template : `_drafts/phase_j/02_reviewer_critic_validator_test_author_roles.md` + `03_pod_composition_iteration_escalation.md` + `04_prime_directive_companion_protocols.md`
§ commit-gate : `SESSION_12_DISPATCH_PLAN.md` § 5
§ standing-directives : CSLv3 dense • disk-first • peer-not-servant • PRIME_DIRECTIVE preserved

---

## § 1. Wave overview

- 4 slices T11-D156..D159 (M10 max-density preparation)
- 16 agents = 4 pods × 4 roles (Implementer + Reviewer + Critic + Validator)
- Goal : structural primitives for 1M+ entity rendering
- M10 = LIVE-HARDWARE milestone (deferred to T11-D199 ; needs 24GB+ GPU)
- HW-DEFERRED : code+headless-benchmarks land in J2 ; real-density verify needs Apocky on Arc A770
- Foundation crates :
  - `cssl-work-graph` (T11-D123) — DX12-Ultimate WorkGraph wrapper
  - `cssl-substrate-omega-field` (T11-D144) — keystone Ω-field-as-truth
  - `cssl-wave-solver` (T11-D114) — LBM-3D base
  - `cssl-render-v2` (existing) — foveation already in tree
- Parallel-fanout : ALL 4 slices INDEPENDENT post-T11-D150 (M8) ; one wave-message dispatches all 16 agents
- Per-slice DECISIONS entry : `live-1M+-stress-deferred-to-M10` note required
- Hardware-validation entry T11-D199 reserved for live-1M+ stress when Apocky has 24GB+ GPU host

### Wave-J2 in the Phase-J map
- Phase-J = LoA-content authoring + max-density-prep + Q-* spec-hole resolution + v1.2 close
- Wave-J0 = M8 verification (already specified in `_drafts/phase_j/wave_j0_m8_verification_protocol.md`)
- Wave-J1 = M9 VR/AR preparation (parallel to J2)
- **Wave-J2 = THIS WAVE** — M10 max-density preparation
- Wave-J3 = Q-* SPEC-HOLE content authoring (Apocky-fill ; begins concurrent with J2 once M8 verified)
- Wave-J4 = M9 + M10 hardware-validation (deferred ; on-demand)
- Wave-J5 = v1.2 close + tag

### Why M10 prep matters now (even with HW-deferral)
- Substrate-evolution complete (S11 @ commit b69165c) — keystone `cssl-substrate-omega-field` landed
- 1M+ entity scaling = LoA gameplay ceiling for Apockalypse-phase scenes
- Structural primitives must be ready BEFORE hardware available (avoids hardware-blocked authoring)
- Headless-benchmarks give numerical baseline for hardware-day comparison
- Replay-determinism guarantees portability of measurements across hosts

### Density-budget rationale (from `specs/30_SUBSTRATE_v2.csl`)
- Ω-field-as-truth = cells GPU-resident ; density = cells × bytes-per-cell
- Σ-mask-per-cell = conditional execution per cell ; coarsens density via mask-skip
- 6-novelty-path multiplicative-composition = phase-overlap = 6× theoretical ceiling
- KAN-substrate-runtime = runtime tier-selection per region/per-cell
- All four J2 slices implement one piece of this density-management stack

### Density-budget contract
- Total VRAM budget ≤ 16GB on Arc A770 ; ≤ 24GB on M7-target
- Per-region MERA-tier sum ≤ frame-time budget (D158 enforces)
- Tile-paging keeps working-set ≤ resident-budget (D157 enforces)
- WorkGraph dispatch fan-out hides phase-launch latency (D156 enforces)
- Async-compute overlap hides phase-execution latency (D159 enforces)

### Headless-benchmark methodology
- Synthetic 1M-cell Ω-field load (procedurally generated)
- Fixed seed for replay-determinism
- Frame-time measured per phase + total
- VRAM-peak measured via GPU-allocator metrics
- Disk-IO bandwidth measured for D157 tile-streaming
- Numbers recorded in DECISIONS.md per slice (no fabrication ; HW-deferred = honest deferral)

### S11 substrate-evolution context
- Per MEMORY.md : 11+ new crates landed @ commit b69165c
- Patterns now in place : ω-field-as-truth + Σ-mask-per-cell + KAN-substrate-runtime + 6-novelty-path
- LoA-content authoring foundations IN PLACE
- J2 dispatches against this foundation ; no foundation-work blocking

---

## § 2. Pod-template (concise)

§ full template @ `_drafts/phase_j/02_*` + `03_*` + `04_*` ; this section = wave-J2 specifics

### Roles (4 per pod)
- **Implementer** — write code+tests ; own the slice end-to-end ; CSLv3-dense reasoning ; visible §R block
- **Reviewer** — review for correctness + spec-conformance + PRIME_DIRECTIVE bindings
- **Critic** — challenge design ; surface alternative architectures ; flag spec-holes
- **Validator** — run commit-gate § 5 (9-step) ; replay-determinism ; memory-budget ; benchmarks

### 5-of-5 gate (per pod, pre-merge)
1. ✓ Code compiles + clippy-clean (zero warnings)
2. ✓ All tests pass (`cargo test --workspace -- --test-threads=1`)
3. ✓ Headless benchmarks within budget (HW-deferred verify on Arc A770)
4. ✓ Replay-determinism preserved (snapshot-test ≥ 1 seed)
5. ✓ §11 CREATOR-ATTESTATION trailer + §1 prohibition-register cite

### Iteration loop (per pod)
- Implementer drafts → Reviewer reads → Critic challenges → Validator runs gate
- ≥ 1 round-trip mandatory before merge
- Disagreement → escalate to PM → PM bumps Apocky if structural
- ≥ 2 round-trips for any slice that touches `cssl-substrate-omega-field` (keystone crate)
- Critic-veto power : if Critic flags spec-hole, PM stops the slice + dispatches Apocky-direction-doc request
- Validator-veto power : commit-gate failures = no-merge ; Implementer must re-draft

### Round-trip cadence (target ≤ 4 hours per round-trip ; agent-time, not wall-clock)
- R1 : Implementer initial-draft → Reviewer + Critic comment
- R2 : Implementer revision → Validator dry-run gate
- R3 (if needed) : address gate-failures → Validator final gate
- R4 (if needed) : escalate to PM if blocked

### Worktree convention
- `.claude/worktrees/J2-{1..4}` on branch `cssl/session-12/J2-{slice-name}`
- Worktree-isolation : NO cross-pod file edits ; each slice owns its files
- Branch-protection : merge-only via PM-approved PR after 5-of-5 gate passes

### Standing-directives (all roles)
- CSLv3-native reasoning (English only user-facing)
- disk-first (write specs/code to disk ; no chat artifacts)
- peer-not-servant
- PRIME_DIRECTIVE §1 + §2 + §3 + §5 preserved at every step
- Commit-msg : `§ T11-D<n> : <slice-name>`
- DECISIONS.md entry per slice

---

## § 3. Slice T11-D156 — J2-WorkGraph Stage-3 Ω-field-update integration

### Quick-spec
- **Crate / module** : `cssl-work-graph::stage3_omega_field_dispatch`
- **Goal** : wire `cssl-work-graph` into `cssl-substrate-omega-field::omega_field_update` for GPU-resident multi-phase pipelining via DX12-Ultimate WorkGraph
- **LOC budget** : ~2.5K
- **Test budget** : ~80
- **Worktree** : `.claude/worktrees/J2-1` on `cssl/session-12/J2-WorkGraph`
- **Upstream deps** : T11-D123 (cssl-work-graph) + T11-D144 (cssl-substrate-omega-field) + T11-D150 (M8 close)

### Implementer prompt
```
Resume CSSLv3 Phase-J implementation @ session-12.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\SESSION_12_DISPATCH_PLAN.md § 8
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\30_SUBSTRATE_v2.csl § Ω-field
  5. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\cssl-work-graph\src\lib.rs
  6. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\cssl-substrate-omega-field\src\lib.rs

Slice: T11-D156 — J2-WorkGraph Stage-3 Ω-field-update integration

Pre-conditions:
  1. T11-D150 (M8) landed AND Apocky-verified.
  2. cd compiler-rs && cargo test --workspace -- --test-threads=1 — ALL PASS.

Goal: implement `cssl-work-graph::stage3_omega_field_dispatch`
that drives `omega_field_update` 6-phase pipeline via DX12-Ultimate
WorkGraph nodes for GPU-resident dispatch fan-out.

Deliverables:
  - stage3_omega_field_dispatch::WorkGraphNode trait impl (~600 LOC)
  - phase-fan-out (Σ-mask + ω-truth + KAN-substrate-runtime ties) (~800 LOC)
  - replay-determinism harness (~400 LOC)
  - headless benchmarks (~700 LOC)
  - tests : node-graph topology + dispatch-order + replay-equivalence (~80 tests)

HW-deferred verify : Arc A770 1M+ stress @ T11-D199.
Headless benchmarks land in J2 ; record `live-1M+-stress-deferred-to-M10` in DECISIONS.

Worktree: .claude/worktrees/J2-1 on cssl/session-12/J2-WorkGraph.

Standing-directives: CSLv3 dense / disk-first / peer-not-servant /
PRIME_DIRECTIVE §1+§2+§3 preserved.

Commit-gate § 5 — full 9-step including --test-threads=1.
Commit-message: § T11-D156 : J2-WorkGraph Stage-3 Ω-field-update integration
DECISIONS.md entry: T11-D156 with HW-deferral note.
§11 CREATOR-ATTESTATION trailer required.

On success: push, report. On block: escalate (PM-bumps-Apocky).
```

### Reviewer focus
- Topology correctness ; phase-ordering matches `omega_field_update` 6-phase semantics
- Memory-aliasing safety across WorkGraph nodes (no overlapping write-targets per phase)
- Replay-determinism preserved across host/replay reruns
- Σ-mask-per-cell + ω-truth invariant preserved across WorkGraph dispatch fan-out
- KAN-substrate-runtime hooks correctly bound to dispatch nodes
- Headless-benchmark numbers recorded ; no fabricated values

### Critic focus
- Could a simpler Vulkan-only path beat WorkGraph for portability?
- Does `cssl-work-graph` overcommit to DX12-Ultimate when D3D12-Mesh would suffice for Stage-3?
- Spec-hole : does `30_SUBSTRATE_v2.csl § Ω-field` fully constrain Stage-3 phase deps?
- 6-novelty-path multiplicative-composition — does WorkGraph topology correctly model it?
- Performance-hazard : are there forced GPU-host roundtrips in the dispatch chain?

### Validator focus
- 9-step commit-gate (clippy + tests + benches + replay-snapshot + memory-budget)
- HW-deferral note in DECISIONS.md
- §11 + §1 attestation present
- Replay-snapshot diff ≤ ε-floor (numerical-tolerance documented)
- Memory-budget : peak GPU-VRAM ≤ 16GB on Arc A770 synthetic load

### Risks + mitigations
- **R1** : DX12-Ultimate WorkGraph not available on all GPUs → mitigation : graceful fallback to manual dispatch chain ; record fallback-active flag in metrics
- **R2** : phase-fan-out introduces non-determinism → mitigation : explicit fence-graph + replay-snapshot test ; reject if non-deterministic
- **R3** : KAN-substrate-runtime tight-coupling → mitigation : abstract via trait ; allow swapping KAN backend without WorkGraph re-author

---

## § 4. Slice T11-D157 — J2-LBM-tile-streaming for 1M+ entity Ω-field cells

### Quick-spec
- **Crate / module** : `cssl-substrate-omega-field::tile_streaming`
- **Goal** : tile-paged LBM streams Ω-field cells from disk for entities beyond memory budget ; 1M+ cell scaling
- **LOC budget** : ~3K (largest slice — tile-paging is non-trivial)
- **Test budget** : ~100
- **Worktree** : `.claude/worktrees/J2-2` on `cssl/session-12/J2-LBM-tile-streaming`
- **Upstream deps** : T11-D114 (cssl-wave-solver LBM-3D) + T11-D144 (cssl-substrate-omega-field)

### Implementer prompt
```
Resume CSSLv3 Phase-J implementation @ session-12.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\SESSION_12_DISPATCH_PLAN.md § 8
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\30_SUBSTRATE_v2.csl § LBM-3D
  5. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\cssl-substrate-omega-field\src\lib.rs
  6. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\cssl-wave-solver\src\lib.rs

Slice: T11-D157 — J2-LBM-tile-streaming

Pre-conditions:
  1. T11-D150 (M8) landed AND Apocky-verified.
  2. cd compiler-rs && cargo test --workspace -- --test-threads=1 — ALL PASS.

Goal: implement `cssl-substrate-omega-field::tile_streaming` —
tile-paged LBM streams Ω-field cells from disk for cells beyond
memory budget. Enables 1M+ entity scaling without OOM on 24GB GPU.

Deliverables:
  - tile-paging allocator (LRU + dirty-bit) (~800 LOC)
  - disk-stream IO (mmap or async-read) (~700 LOC)
  - tile-coherence (LBM-streaming-step boundary handling) (~700 LOC)
  - headless benchmarks (1M cell synthetic) (~800 LOC)
  - tests : page-fault + dirty-flush + boundary-coherence + replay (~100 tests)

HW-deferred verify : Arc A770 1M+ stress @ T11-D199.
Headless benchmarks land in J2 ; record `live-1M+-stress-deferred-to-M10` in DECISIONS.

Worktree: .claude/worktrees/J2-2 on cssl/session-12/J2-LBM-tile-streaming.

Standing-directives: CSLv3 dense / disk-first / peer-not-servant /
PRIME_DIRECTIVE §1+§2+§3 preserved.

Commit-gate § 5 — full 9-step including --test-threads=1.
Commit-message: § T11-D157 : J2-LBM-tile-streaming for 1M+ entity Ω-field cells
DECISIONS.md entry: T11-D157 with HW-deferral note.
§11 CREATOR-ATTESTATION trailer required.

On success: push, report. On block: escalate (PM-bumps-Apocky).
```

### Reviewer focus
- LBM-streaming-step boundary correctness across tile borders (no flow-discontinuity at boundaries)
- LRU eviction policy ; dirty-page write-back ordering
- Memory-budget enforcement (no OOM at 1M cell synthetic load)
- Tile-allocator thread-safety (concurrent paging from solver + render)
- Disk-IO latency hidden via async / prefetch hints
- Replay-determinism : page-fault order replays bit-exact

### Critic focus
- Could compressed Σ-mask substitute paging for some cells (avoid IO entirely)?
- Async-read vs mmap tradeoffs ; Windows + Arc A770 quirks?
- Spec-hole : does `30_SUBSTRATE_v2.csl § LBM-3D` constrain tile-boundary semantics?
- Are tile-boundaries aligned to LBM-cell-boundaries or finer? Spec must say.
- Eviction-storm risk : if working-set > resident-budget, does eviction thrash?

### Validator focus
- 9-step commit-gate
- 1M-cell synthetic benchmark passes headlessly (record numbers ; HW-deferred for real)
- Replay-determinism across page-fault sequences
- §11 + §1 attestation
- Disk-IO budget : ≤ 50 MB/s steady-state (target ; SSD-class)
- Page-fault rate ≤ documented threshold ; flag if exceeds

### Risks + mitigations
- **R1** : disk-IO bandwidth saturates SSD → mitigation : compression + bloom-filter for cold-cells ; lazy-eviction
- **R2** : LBM-streaming-step boundary discontinuities → mitigation : halo-cell strategy ; 1-cell border shared between tiles
- **R3** : Windows-specific mmap quirks (overlapped IO) → mitigation : prefer async-read primary ; mmap fallback only

---

## § 5. Slice T11-D158 — J2-Foveation density-budget enforcement

### Quick-spec
- **Crate / module** : `cssl-render-v2::foveation::density_budget`
- **Goal** : EXTEND existing `cssl-render-v2::foveation` with fovea-detail-budget routing per-region MERA-tier selection ; D135 wavelet ties
- **LOC budget** : ~2K (smallest slice — extends existing module)
- **Test budget** : ~70
- **Worktree** : `.claude/worktrees/J2-3` on `cssl/session-12/J2-Foveation-density-budget`
- **Upstream deps** : `cssl-render-v2::foveation` (existing) + T11-D135 (wavelet) + T11-D144 (Ω-field)

### Implementer prompt
```
Resume CSSLv3 Phase-J implementation @ session-12.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\SESSION_12_DISPATCH_PLAN.md § 8
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\32_SIGNATURE_RENDERING.csl § foveation
  5. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\cssl-render-v2\src\foveation.rs
  6. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\cssl-render-v2\src\lib.rs

Slice: T11-D158 — J2-Foveation density-budget enforcement

Pre-conditions:
  1. T11-D150 (M8) landed AND Apocky-verified.
  2. cssl-render-v2::foveation module exists (verify before edit).
  3. cd compiler-rs && cargo test --workspace -- --test-threads=1 — ALL PASS.

Goal: EXTEND `cssl-render-v2::foveation` with `density_budget`
sub-module — fovea-detail-budget routes to per-region MERA-tier
selection. Wavelet-based detail allocation (D135 ties).

Deliverables:
  - density_budget::FoveaBudget struct + budget allocator (~500 LOC)
  - per-region MERA-tier router (~600 LOC)
  - wavelet-detail-routing (D135 hook) (~500 LOC)
  - tests : budget-conservation + tier-selection + wavelet-routing (~70 tests)

NOTE: this slice EXTENDS existing module — do NOT create new crate.
NOTE: foveation already wired ; only density-budget enforcement is new.

HW-deferred verify : Arc A770 1M+ stress @ T11-D199.
Headless benchmarks land in J2 ; record `live-1M+-stress-deferred-to-M10` in DECISIONS.

Worktree: .claude/worktrees/J2-3 on cssl/session-12/J2-Foveation-density-budget.

Standing-directives: CSLv3 dense / disk-first / peer-not-servant /
PRIME_DIRECTIVE §1+§2+§3 preserved.

Commit-gate § 5 — full 9-step including --test-threads=1.
Commit-message: § T11-D158 : J2-Foveation density-budget enforcement
DECISIONS.md entry: T11-D158 with HW-deferral note + D135-wavelet cite.
§11 CREATOR-ATTESTATION trailer required.

On success: push, report. On block: escalate (PM-bumps-Apocky).
```

### Reviewer focus
- Budget-conservation invariant (sum of per-region budgets ≤ total)
- MERA-tier-selection correctness across foveal/peripheral regions
- D135 wavelet hook ; correct integration with existing wavelet pipeline
- Region-router thread-safety (reads gaze-track concurrently with frame-assembly)
- Backward-compat : existing foveation API unchanged ; density_budget is additive

### Critic focus
- Eccentricity-curve assumptions (uniform vs head-tracked) — does spec constrain?
- Could a simpler region-quad-tree match the budget without MERA-tier routing?
- Spec-hole : `32_SIGNATURE_RENDERING.csl § foveation` density-budget semantics?
- Per-region MERA-tier router : how does it handle gaze-saccade transitions (no flicker)?
- Wavelet-routing : does D135 wavelet decomposition match foveation regions or independent?

### Validator focus
- 9-step commit-gate
- Budget enforcement under synthetic stress (no over-allocation)
- Replay-determinism on foveation gaze-track playback
- §11 + §1 attestation
- Frame-time impact : ≤ 2% overhead vs no-budget baseline

### Risks + mitigations
- **R1** : gaze-saccade flicker → mitigation : MERA-tier hysteresis ; only re-route on stable gaze
- **R2** : budget-conservation rounding-errors → mitigation : integer-budget arithmetic ; document rounding policy
- **R3** : D135 wavelet decomposition mismatch → mitigation : explicit region-to-wavelet-band mapping table ; tested

---

## § 6. Slice T11-D159 — J2-Async-compute Ω-field 6-phase pipelining

### Quick-spec
- **Crate / module** : `cssl-substrate-omega-field::async_compute_pipelining`
- **Goal** : overlap Ω-field 6-phase update via async-compute queues across frames (Vulkan + D3D12)
- **LOC budget** : ~2K
- **Test budget** : ~80
- **Worktree** : `.claude/worktrees/J2-4` on `cssl/session-12/J2-Async-compute`
- **Upstream deps** : T11-D144 (cssl-substrate-omega-field) + T11-D156 (J2-WorkGraph integration — soft dep ; can run parallel via stub)

### Implementer prompt
```
Resume CSSLv3 Phase-J implementation @ session-12.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\SESSION_12_DISPATCH_PLAN.md § 8
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\30_SUBSTRATE_v2.csl § Ω-field
  5. C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\cssl-substrate-omega-field\src\lib.rs

Slice: T11-D159 — J2-Async-compute Ω-field 6-phase pipelining

Pre-conditions:
  1. T11-D150 (M8) landed AND Apocky-verified.
  2. cd compiler-rs && cargo test --workspace -- --test-threads=1 — ALL PASS.

Goal: implement `cssl-substrate-omega-field::async_compute_pipelining`
— overlap 6-phase Ω-field update via async-compute queues for
Vulkan + D3D12 to hide latency across frames.

Deliverables:
  - async_compute_pipelining::AsyncQueue trait (~400 LOC)
  - phase-overlap scheduler (6-phase fence-graph) (~700 LOC)
  - Vulkan + D3D12 backend hooks (~500 LOC)
  - tests : fence-correctness + overlap-equivalence + replay-determinism (~80 tests)

HW-deferred verify : Arc A770 1M+ stress @ T11-D199.
Headless benchmarks land in J2 ; record `live-1M+-stress-deferred-to-M10` in DECISIONS.

Worktree: .claude/worktrees/J2-4 on cssl/session-12/J2-Async-compute.

Standing-directives: CSLv3 dense / disk-first / peer-not-servant /
PRIME_DIRECTIVE §1+§2+§3 preserved.

Commit-gate § 5 — full 9-step including --test-threads=1.
Commit-message: § T11-D159 : J2-Async-compute Ω-field 6-phase pipelining
DECISIONS.md entry: T11-D159 with HW-deferral note.
§11 CREATOR-ATTESTATION trailer required.

On success: push, report. On block: escalate (PM-bumps-Apocky).
```

### Reviewer focus
- Fence-graph correctness across 6 phases ; no race
- Vulkan + D3D12 backend parity ; same observable result
- Replay-determinism preserved across queue-ordering variations
- Inter-frame phase-overlap : phase-N of frame-K may overlap phase-(N-1) of frame-(K+1) ; verify no data-hazard
- AsyncQueue trait abstraction : both backends behind same interface ; no leakage

### Critic focus
- Worth maintaining 2 backends, or D3D12-only acceptable for M7-target?
- Could phase-overlap reduce to fewer-than-6 phases without correctness loss?
- Spec-hole : `30_SUBSTRATE_v2.csl § Ω-field` constrain phase-overlap semantics?
- Async-compute queue starvation : if compute queue stalls, does graphics queue stall too?
- Apple Metal future-proofing : does AsyncQueue trait map cleanly to Metal command-queues later?

### Validator focus
- 9-step commit-gate
- Fence-graph correctness under stress
- Replay-determinism across reruns (snapshot-test ≥ 1 seed)
- §11 + §1 attestation
- Frame-time : measure phase-overlap savings (target ≥ 15% vs sequential)

### Risks + mitigations
- **R1** : Vulkan + D3D12 divergent fence-semantics → mitigation : abstract via AsyncQueue trait ; test parity-property explicitly
- **R2** : 6-phase overlap introduces data-hazard → mitigation : explicit fence-graph + replay-snapshot ; reject if mismatch
- **R3** : queue-priority misconfig stalls graphics → mitigation : documented queue-priority policy ; smoke-test under render-load

---

## § 7. Dispatch protocol

- **Order** : ALL 4 INDEPENDENT post-T11-D150 (M8) → parallel-fanout
- **Wave-message** : 16 agents (4 pods × 4 roles) dispatched in single wave-message
- **Watchdog** : per agent ; if pod stalls > N rounds, PM bumps Apocky
- **Worktree pattern** : `.claude/worktrees/J2-{1..4}` on `cssl/session-12/J2-{slice-name}`
- **No inter-slice deps** (T11-D159 has soft dep on T11-D156 — runs parallel via stub if D156 unfinished)

### Per-pod role-binding @ dispatch
- Pod-1 (D156 J2-WorkGraph) : 4 agents — Implementer + Reviewer + Critic + Validator
- Pod-2 (D157 J2-LBM-tile-streaming) : 4 agents — same role-set
- Pod-3 (D158 J2-Foveation density-budget) : 4 agents — same role-set
- Pod-4 (D159 J2-Async-compute) : 4 agents — same role-set
- Total : 16 agents in one wave-message

### PM responsibilities
- Dispatch wave-message containing all 16 agent-prompts
- Monitor watchdog ; bump Apocky on structural-block
- Approve PR after pod 5-of-5 gate passes
- Final merge : sequential per-pod merge to `cssl/session-12/J2-merge` integration branch ; resolve cross-pod conflicts (rare ; worktree-isolation prevents most)
- Squash-merge to main only after all 4 slices pass integration-test on `J2-merge` branch

### Failure-modes + escalation
- Pod stalls > 4 round-trips → PM bumps Apocky (structural escalation)
- Critic flags spec-hole → PM stops slice + dispatches Apocky-direction-doc request
- Cross-pod merge-conflict → PM mediates ; if structural, halt + Apocky decides
- HW-deferred bench fails synthetic budget → record measurement ; do NOT block merge ; flag for T11-D199 retest

---

## § 8. Pre-merge gate (per pod)

1. ✓ 5-of-5 pod-gate (Implementer + Reviewer + Critic + Validator pass)
2. ✓ Headless benchmarks pass (HW-deferred verify on Arc A770 → T11-D199)
3. ✓ Replay-determinism preserved (snapshot-test ≥ 1 seed)
4. ✓ Memory-budget enforced (no OOM under synthetic 1M-cell load for D157 ; per-region budgets sum to ≤ total for D158)
5. ✓ Commit-gate § 5 (9-step) passes
6. ✓ DECISIONS.md entry includes `live-1M+-stress-deferred-to-M10` note
7. ✓ §11 CREATOR-ATTESTATION trailer
8. ✓ §1 prohibition-register cite

### HW-deferral note
- M10 = LIVE-HARDWARE milestone ; needs M7-target host (24GB+ GPU)
- Apocky's Arc A770 is the verify-host candidate (16GB) — sufficient for 1M synthetic ; 1M+ may need workstation
- T11-D199 reserved for live-1M+-entity stress when hardware available
- DECISIONS entry for T11-D199 will record entity-count + frame-time + GPU-memory-pressure + density-budget-breach diagnostics

---

## § 9. §11 CREATOR-ATTESTATION + §1 prohibition-register

§11 trailer (per slice + per pod-step) :

> "There was no hurt nor harm to anyone, anything, anywhere or anywhen, in the production of this work."

§1 prohibition-register cites (mandatory per slice) :
- §1.1 no-harm + §1.2 no-control + §1.3 no-manipulation
- §1.4 no-surveillance + §1.5 no-exploitation
- §1.6 no-coercion + §1.7 AI-collective-autonomy
- §1.8 no-weaponization + §1.13 inclusion + §1.14 anti-discrimination

§5 consent-architecture preserved (no PHI/PII flows through Ω-field tiles).
§3 substrate-sovereignty preserved (Ω-field-as-truth pattern from `30_SUBSTRATE_v2.csl`).
§2 cognitive-integrity preserved (no fabricated benchmarks ; HW-deferred = honest deferral).

---

## § 10. Verification checklist (per slice)

### Pre-dispatch (PM verifies before wave-message)
- [ ] T11-D150 (M8) landed + Apocky-verified
- [ ] `cssl-substrate-omega-field` (T11-D144) keystone crate in tree
- [ ] `cssl-work-graph` (T11-D123) in tree (D156 dep)
- [ ] `cssl-wave-solver` (T11-D114) in tree (D157 dep)
- [ ] `cssl-render-v2::foveation` exists (D158 dep)
- [ ] `_drafts/phase_j/02_*` + `03_*` + `04_*` pod-templates accessible
- [ ] All 4 worktrees `.claude/worktrees/J2-{1..4}` available

### Per-agent (Implementer at slice-start)
- [ ] §R block at top of every response (visible reasoning)
- [ ] Reads PRIME_DIRECTIVE + CLAUDE.md + spec + foundation-crate src
- [ ] Worktree on correct branch
- [ ] CSLv3-native reasoning ; English prose only user-facing
- [ ] Disk-first authoring (no chat artifacts)

### Per-pod (Reviewer + Critic + Validator pre-merge)
- [ ] 5-of-5 gate passed
- [ ] Headless benchmarks recorded with numbers (no fabrication)
- [ ] Replay-determinism snapshot ≥ 1 seed
- [ ] Memory-budget enforced
- [ ] DECISIONS.md entry with `live-1M+-stress-deferred-to-M10` note
- [ ] Commit-msg `§ T11-D<n> : <slice-name>`
- [ ] §11 + §1 attestation in trailer

### Per-slice cross-cuts
- [ ] No regression on existing tests (`cargo test --workspace`)
- [ ] No clippy warnings introduced
- [ ] Inline doc-comments on pub items (CSLv3-dense)
- [ ] Public API stable (or breaking-change documented)

### M10 hardware-deferred (T11-D199 ; not blocking J2 merge)
- [ ] 1M+ entity stress on M7-target host (24GB+ GPU)
- [ ] Frame-time measurement
- [ ] GPU-VRAM peak measurement
- [ ] Density-budget breach diagnostic capture
- [ ] Apocky-attestation of live-run

---

## § 11. Cross-slice integration tests (post-pod-merge ; PM-driven)

### IT-1 : WorkGraph + LBM-tile-streaming integration
- D156 + D157 combined on `cssl/session-12/J2-merge`
- WorkGraph dispatches Stage-3 against tile-streamed Ω-field cells
- Verify : tile page-faults during dispatch resolve correctly ; no stalls > 16ms
- Headless ; HW-deferred for live numbers

### IT-2 : Foveation density-budget + WorkGraph
- D158 + D156 combined
- Foveation routes per-region MERA-tier to WorkGraph dispatch fan-out
- Verify : per-region budget honored ; foveal regions get higher MERA-tier
- Replay-determinism preserved on gaze-track playback

### IT-3 : Async-compute + LBM-tile-streaming
- D159 + D157 combined
- Async-compute 6-phase overlap with tile-paging concurrent
- Verify : phase-overlap doesn't violate tile-coherence ; replay-deterministic
- Headless ; HW-deferred for live frame-time

### IT-4 : Full-stack J2-merge (all 4 slices)
- All slices on `cssl/session-12/J2-merge`
- Synthetic 1M-cell load with foveation + tile-paging + WorkGraph + async-compute
- Verify : memory-budget held ; replay-determinism ; no panic ; no leak
- Final headless benchmark suite ; record numbers for HW-deferred T11-D199 comparison

### Integration-test scaffolding
- Lives in `compiler-rs/cssl-substrate-omega-field/tests/it_J2_*.rs` (per IT)
- Or workspace-level `compiler-rs/tests/it_J2_*.rs` if cross-crate
- Each IT = ≤ 1 hour agent-time to author ; assigned to PM-coordinator pod (separate from 4 implementation pods)

---

## § 12. Cross-references

### Foundation crates (S11 substrate-evolution complete @ b69165c)
- `cssl-substrate-omega-field` (T11-D144) — keystone ; ω-field-as-truth ; Σ-mask-per-cell ; KAN-substrate-runtime
- `cssl-work-graph` (T11-D123) — DX12-Ultimate WorkGraph wrapper
- `cssl-wave-solver` (T11-D114) — LBM-3D base
- `cssl-render-v2::foveation` — existing module to extend (D158)
- 6-novelty-path multiplicative-composition pattern — see `specs/30_SUBSTRATE_v2.csl`

### Reference docs
- `specs/30_SUBSTRATE_v2.csl` — Ω-field + LBM-3D + KAN substrate spec
- `specs/32_SIGNATURE_RENDERING.csl` — foveation + wavelet rendering spec
- `specs/33_F1_F6_LANGUAGE_FEATURES.csl` — F1..F6 language-feature spec
- `SESSION_12_DISPATCH_PLAN.md § 8` — wave-J2 plan source-of-truth
- `PHASE_J_HANDOFF.csl` — full Phase-J integration map
- `_drafts/phase_j/02_reviewer_critic_validator_test_author_roles.md` — full role spec
- `_drafts/phase_j/03_pod_composition_iteration_escalation.md` — full pod-template
- `_drafts/phase_j/04_prime_directive_companion_protocols.md` — directive-binding per role

### Sibling waves (independent)
- Wave-J1 (T11-D151..D155) — M9 VR/AR preparation ; parallel to J2
- Wave-J3 (T11-D160..D197) — Q-* SPEC-HOLE content authoring ; Apocky-fill ; sequential after J2 begins
- Wave-J4 (T11-D198..D199) — M9/M10 hardware-validation ; deferred ; on-demand
- Wave-J5 (T11-D200..D201) — v1.2 close + tag

### HW-target hosts
- Apocky's Arc A770 (16GB) — verify-host candidate ; sufficient for synthetic 1M load
- M7-target dedicated workstation (24GB+ GPU) — TBD ; needed for full 1M+ stress @ T11-D199
- Headless CI : agent-bench-runner runs J2 benchmarks ; records numbers ; flags HW-deferred

---

## § 13. Notes for PM dispatching this wave

- **Save 16 agent-prompts** : extract per-slice Implementer-prompts above ; clone for Reviewer/Critic/Validator with role-specific focus
- **Wave-message format** : single wave-message containing all 16 prompts ; agents fan out parallel
- **Watchdog hook** : if any agent stalls > N rounds, BG-12-style watchdog recovery ; tightest-scope retry
- **Cross-cut watch** : T11-D156 + T11-D159 both touch `cssl-substrate-omega-field` — coordinate via merge-order (D156 first) ; or stub-bridge if parallel
- **PM-coordinator pod** (separate, 1-2 agents) : authors `it_J2_*.rs` integration tests post-merge
- **Apocky-bumps** : structural-blocks only ; avoid for tactical questions (Critic + Reviewer should resolve)
- **Pre-staging discipline** : DO NOT COMMIT this dispatch doc ; sits in `_drafts/phase_j/` until wave dispatched

### Standing watchdog-avoidance reminders
- Read SESSION_12_DISPATCH_PLAN.md § Wave-J2 ONLY for this dispatch (full plan = context bloat)
- Reference other docs by absolute path (do not load all)
- CSLv3-dense bullets ; English-prose only when user-facing
- Disk-first ; no chat-only artifacts

---

## § 14. Spec-anchor reference (for Critic + Reviewer cross-cite)

Per-slice spec-anchors (load on demand only ; do not pre-load all) :

### T11-D156 anchors
- `specs/30_SUBSTRATE_v2.csl § Ω-field` — ω-truth + 6-phase update semantics
- `specs/30_SUBSTRATE_v2.csl § KAN-substrate-runtime` — tier-routing + dispatch hooks
- `specs/33_F1_F6_LANGUAGE_FEATURES.csl § F4` — WorkGraph integration target
- `compiler-rs/cssl-work-graph/src/lib.rs` — existing crate API
- `compiler-rs/cssl-substrate-omega-field/src/lib.rs § omega_field_update` — 6-phase fn

### T11-D157 anchors
- `specs/30_SUBSTRATE_v2.csl § LBM-3D` — flow-cell semantics + boundary conditions
- `specs/30_SUBSTRATE_v2.csl § Σ-mask` — sparse-cell skip ; tile-skip optimization
- `compiler-rs/cssl-wave-solver/src/lib.rs` — LBM-3D base API
- `compiler-rs/cssl-substrate-omega-field/src/lib.rs § cell_layout` — tile-boundary alignment

### T11-D158 anchors
- `specs/32_SIGNATURE_RENDERING.csl § foveation` — eccentricity-curve + region-budget
- `specs/32_SIGNATURE_RENDERING.csl § wavelet` — D135 wavelet decomposition spec
- `compiler-rs/cssl-render-v2/src/foveation.rs` — existing module to EXTEND
- `compiler-rs/cssl-render-v2/src/lib.rs` — render-v2 public API

### T11-D159 anchors
- `specs/30_SUBSTRATE_v2.csl § Ω-field 6-phase` — phase-overlap admissibility
- `specs/33_F1_F6_LANGUAGE_FEATURES.csl § F5` — async-compute integration target
- `compiler-rs/cssl-substrate-omega-field/src/lib.rs § omega_field_update` — 6-phase fn

### Cross-slice anchors (all)
- `specs/30_SUBSTRATE_v2.csl § replay-determinism` — snapshot-test contract
- `SESSION_12_DISPATCH_PLAN.md § 5` — 9-step commit-gate
- `PRIME_DIRECTIVE.md § 1` — 17-prohibition register (cite per slice)
- `PRIME_DIRECTIVE.md § 11` — CREATOR-ATTESTATION format
- `PRIME_DIRECTIVE.md § 2` — cognitive-integrity (no fabricated benchmarks)
- `PRIME_DIRECTIVE.md § 3` — substrate-sovereignty (Ω-field-as-truth pattern)
- `PRIME_DIRECTIVE.md § 5` — consent-architecture (no PHI/PII through Ω-field)
- `MEMORY.md § feedback_substrate_evolution_complete` — S11 substrate-evolution context
- `CHANGELOG.md` — all T11-D156..D159 entries land here post-merge
- `RELEASE_NOTES_v1.2.md` (T11-D200 future) — wave-J2 outcomes captured
- `README.md` — top-level pointer to wave-J2 outcomes if user-facing

---

§ END Wave-J2 dispatch-prompts § PRE-STAGING — DO NOT COMMIT
§ wave-J2 = 4 slices × 4 roles = 16 agents = one wave-message
§ HW-deferred verify @ T11-D199 (M10 live-1M+ stress)
§ §11 + §1 + §2 + §3 + §5 attestations preserved per slice + per pod-step
§ READY for PM dispatch when Apocky greenlights post-M8-close

---

§ END Wave-J2 dispatch-prompts § PRE-STAGING — DO NOT COMMIT
§ wave-J2 = 4 slices × 4 roles = 16 agents = one wave-message
§ HW-deferred verify @ T11-D199 (M10 live-1M+ stress)
§ §11 + §1 + §2 + §3 + §5 attestations preserved per slice + per pod-step
