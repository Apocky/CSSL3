# CSSLv3 — Changelog

All notable changes to the CSSLv3 compiler, runtime, stdlib, and Substrate are
recorded here. Changes are grouped by release. Within each release, entries are
grouped by Phase (A through I), then listed in the order they landed on the
integration branch.

Each entry references its `T11-D##` decision in [DECISIONS.md](DECISIONS.md)
where the full design rationale, alternatives considered, and consequences
live. The `T11-D##` numbers are stable identifiers — they do not get renumbered.

The PRIME DIRECTIVE ([PRIME_DIRECTIVE.md](PRIME_DIRECTIVE.md)) governs every
slice listed here. Every slice landed under the creator-attestation in §11
(verbatim: "There was no hurt nor harm in the making of this, to anyone,
anything, or anybody.").

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [v1.0.0] — 2026-04-29 — CSSL/Substrate complete

This is the v1.0 release of CSSLv3 — the compiler, runtime, stdlib, and
Substrate are now end-to-end complete. CSSLv3 is the forever-substrate for the
Labyrinth of Apockalypse (LoA) per `HANDOFF_SESSION_6.csl § AXIOM` ("forever-
substrate = CSSLv3 .end-product"). The Phase-I scaffold (LoA project skeleton)
has also landed at the v1.0 boundary; per-`Q-*` content authoring is the
Apocky-fill work that follows v1.0.

**Headline milestones:**

- **First CSSLv3 executable** — `hello.exe` returns exit code 42 via the
  cranelift backend on Apocky's Windows host (T11-D56, 2026-04-28).
- **Phase-H Substrate complete** — Ω-tensor, `omega_step` deterministic-replay,
  Projections, effect-rows, save/load with bit-equal replay, and the
  PRIME_DIRECTIVE enforcement layer (CapToken + 13 SubstrateCap + PD0001..PD0017
  + ABSOLUTE BLOCK on PD0007 weaponization and PD0009 torture).
- **Hardware verified** — Vulkan and D3D12 talked to a real Intel Arc A770;
  WASAPI completed a sine-tone roundtrip; Win32 file I/O verified byte-equality;
  loopback TCP completed a roundtrip.
- **Phase-I scaffold landed** — `loa-game` crate composes the entire Phase-H
  Substrate + Phase-F hosts into an end-to-end runtime that drives a 13-phase
  `omega_step` and round-trips a save bit-equal. Game-content concerns are
  `Q-*` SPEC-HOLE markers awaiting Apocky-fill (T11-D96).
- **Test count** — 1717 (Phase-A close) → 3495 (v1.0) = +1778 tests across
  the Phase-B through Phase-I-scaffold + Phase-G7-walker fanout.

### Phase A — Bootstrap to first executable (serial)

The serial bootstrap track that produced the first CSSLv3 executable. Every
Phase-A slice depends on its predecessor; no parallelism here.

- **T11-D51** — S6-A0 Windows worktree-isolation gate. Fix `core.autocrlf`-
  driven file leakage across worktrees so 20-way parallel fanout is safe.
- **T11-D52** — S6-A1 `cssl-rt` runtime library. Allocator + entry-shim +
  panic + exit FFI; the C-ABI surface every CSSLv3 executable links against.
- **T11-D53** — S6-A2 `csslc` CLI. Subcommand routing + pipeline orchestration
  (build / check / fmt / test / verify / version / help / emit-mlir).
- **T11-D54** — S6-A3 `cranelift-object` backend. Real `.o` / `.obj` emission
  replacing the JIT-only path.
- **T11-D55** — S6-A4 Linker invocation. Auto-discovered MSVC + `rust-lld` +
  Unix toolchains; `link.exe` / `lld-link` / `ld.lld` / `ld` / `ld64`.
- **T11-D56** — S6-A5 hello.exe gate. **FIRST CSSLv3 EXECUTABLE RUNS, exit
  code 42.** Phase-A serial bootstrap complete; 20-way parallel fanout
  unblocked.

### Phase B — Stdlib surface (parallel fanout)

Heap, sum-types, growable containers, strings, and file I/O. Each slice landed
on a dedicated worktree branch and merged into `cssl/session-6/parallel-fanout`.

- **T11-D57** — S6-B1 Heap-alloc MIR ops + cranelift lowering. The first time
  CSSLv3 source can mint heap-allocated values via `Box::new(x)` and have them
  flow through the full pipeline to a `.o` containing real `__cssl_alloc`
  references.
- **T11-D60** — S6-B2 `Option<T>` + `Result<T, E>` stdlib. Sum-type MIR ops;
  pattern-matching surface; `Some/None` and `Ok/Err` recognizers.
- **T11-D69** — S6-B3 `Vec<T>` stdlib. Heap-backed growth; nested-monomorph
  coverage; `Vec<i32>`, `Vec<f64>`, `Vec<String>`, `Vec<Vec<T>>`.
- **T11-D71** — S6-B4 `String` + `&str` + minimal `format(...)` builtin.
  UTF-8 representation; `char` literals; format-arg machinery.
- **T11-D76** — S6-B5 File I/O. Win32 + Unix syscalls; `stdlib/fs.cssl`;
  `{IO}` effect-row threading; first time CSSLv3 source talks to disk.

### Phase C — Control flow + JIT enrichment (parallel fanout)

Structured control flow, memref, transcendentals, and closures. Closes the
control-flow surface needed for non-trivial programs.

- **T11-D58** — S6-C1 `scf.if` cranelift lowering. `brif` + extended blocks;
  the canonical structured-if shape for CSSLv3.
- **T11-D59** — S6-C3 `memref.load` / `memref.store`. Cranelift load/store
  with alignment-aware emission.
- **T11-D61** — S6-C2 `scf.loop` / `scf.while` / `scf.for`. Header/body/exit
  triplets lowered to cranelift basic blocks.
- **T11-D63** — S6-C4 f64 transcendentals. `sin` / `cos` / `exp` / `log`
  + inline-intrinsic surface for `sqrt` / `abs` / `min` / `max` at f64 width.
- **T11-D77** — S6-C5 (redo) Closures with environment capture. Re-applied on
  the integrated parallel-fanout tip after the original C5 patch (T11-D64) was
  superseded by ~69KB of B-axis evolution. Closes Phase-C.

### Phase D — GPU code generation (parallel fanout)

Body emission for SPIR-V, DXIL, MSL, WGSL plus the structured-CFG validator
that gates all four backends.

- **T11-D70** — S6-D5 Structured-CFG validator + `scf` transform pass.
  Diagnostic codes CFG0001..CFG0010. Gates body emission across D1..D4.
- **T11-D72** — S6-D1 SPIR-V body emission via `rspirv` ops (D5-marker-gated).
- **T11-D73** — S6-D2 DXIL body emission via HLSL text + `dxc` subprocess.
- **T11-D74** — S6-D3 MSL (Metal Shading Language) body emission.
- **T11-D75** — S6-D4 WGSL body emission. D5-marker fanout consumer.

### Phase E — Host backends (parallel fanout)

Native FFI bindings for the five GPU host APIs. Replaces the stub catalogs
authored at session-5.

- **T11-D62** — S6-E5 Level-Zero host. `libloading`-driven owned FFI; sysman
  R18 hooks; verified against Arc A770.
- **T11-D65** — S6-E1 Vulkan host via `ash`. **Verified against Intel Arc
  A770 — real GPU instance + device + queue.**
- **T11-D66** — S6-E2 D3D12 host via `windows-rs`. Win32 cfg-gated; factory
  + device + queue + heap + rootsig + PSO + fence + DRED. Verified against
  Arc A770.
- **T11-D67** — S6-E3 Metal host via `metal-rs`. Apple cfg-gated.
- **T11-D68** — S6-E4 WebGPU host via `wgpu`. Native + wasm32 targets.

### Phase F — Host integration: window, input, audio, networking (parallel fanout)

The host-integration layer that gates Phase H. Without Phase F, the Substrate
has nowhere to render to, no input to bind, no audio to mix, and no network
to talk to peers over.

- **T11-D78** — S7-F1 Window host (Win32 + `cssl-host-window` crate).
  Authored `SESSION_7_DISPATCH_PLAN.md`. F1 is the gate that verifies a
  window spawns and closes cleanly.
- **T11-D80** — S7-F2 Input host. Keyboard / mouse / gamepad; XInput +
  evdev + IOKit cfg-gated.
- **T11-D81** — S7-F3 Audio host. WASAPI + ALSA/PulseAudio + CoreAudio
  cfg-gated; f32 interleaved. **WASAPI sine-tone roundtrip verified.**
- **T11-D82** — S7-F4 Networking host. Winsock2 + BSD sockets; TCP/UDP;
  PRIME_DIRECTIVE-capped. **Loopback TCP roundtrip verified.**

### Phase G — Native x86-64 backend (single-track integration)

The owned hand-rolled native x86-64 backend per `specs/14_BACKEND.csl §
OWNED x86-64 BACKEND`. Six per-slice branches (G1..G6) integrated into a
single `cssl-cgen-cpu-x64` crate.

- **T11-D85** — S7-G3 Native x86-64 ABI lowering. SystemV AMD64 +
  Microsoft-x64 cfg-gated. Pre-integrated into parallel-fanout ahead of
  the cluster.
- **T11-D95** — Session-7 G-axis integration. Unified `cssl-cgen-cpu-x64`
  combining G1 (instruction selection, T11-D83), G2 (linear-scan register
  allocator + spill slots, T11-D84), G3 (ABI, T11-D85, already on
  parallel-fanout), G4 (machine-code byte encoder, T11-D86), G5 (object-
  file emitter — hand-rolled ELF/COFF/Mach-O with zero `cranelift-object`
  dep, T11-D87), and G6 (`csslc --backend=native-x64` façade + native-
  hello-world milestone, T11-D88). The cranelift dependency stays in the
  workspace as the default backend; native-x64 is runtime-selectable.
- **T11-D97** — S7-G7 Native-x64 cross-slice walker. **SECOND hello.exe =
  42 milestone reached** — `csslc build hello.cssl --backend=native-x64
  --emit=exe` now produces a runnable executable (170 bytes; canonical
  body `55 48 89 E5 B8 2A 00 00 00 5D C3` = `push rbp; mov rbp,rsp; mov
  eax,42; pop rbp; ret`) that exits 42 with zero cranelift dependency in
  the emission path. Closes Phase-G end-to-end. The `s7_g6_native_hello_
  world_executable_returns_42` test flips from SKIP to PASS.

### Phase H — Substrate (parallel fanout, with H6 cross-cutting)

The engine plumbing for the Labyrinth of Apockalypse. Five impl slices plus
the cross-cutting PRIME_DIRECTIVE enforcement layer.

- **T11-D79** — S8-H0 Substrate spec + LoA design (parallel design track).
  Authored `specs/30_SUBSTRATE.csl` (~891 LOC) and `specs/31_LOA_DESIGN.csl`
  (~664 LOC). 38 SPEC-HOLE markers (Q-A..Q-LL) listed for Apocky-fill.
- **T11-D89** — S8-H1 Ω-tensor (`OmegaTensor<T, R>`). Canonical multi-
  dimensional state container; iso-cap + opaque-Debug discipline; no
  payload leakage in observable form.
- **T11-D90** — S8-H2 `omega_step` contract + `OmegaScheduler` +
  deterministic-replay contract. The 13-phase tick (consent → input → sim →
  proj → audio → render → telem → audit → save → freeze) per
  `specs/30_SUBSTRATE.csl § OMEGA-STEP`.
- **T11-D91** — S8-H3 Projections. Camera + ObserverFrame + ProjectionMatrix
  + LoD selector; capability-gated reads against the Ω-tensor.
- **T11-D92** — S8-H4 Substrate effect-rows. `{Render}` / `{Sim}` / `{Audio}`
  / `{Net}` / `{Save}` / `{Telemetry}` + composition table + EFR0001..EFR0010
  diagnostic codes.
- **T11-D93** — S8-H5 Save/load + replay-determinism. Binary `CSSLSAVE`
  format + R18 BLAKE3 attestation + bit-equal replay invariant. **First
  runtime-state on disk for CSSLv3.**
- **T11-D94** — S8-H6 PRIME_DIRECTIVE enforcement layer. `cssl-substrate-
  prime-directive` crate; `CapToken` (linear non-Copy + non-Clone proof-of-
  grant); 13-variant `SubstrateCap` enum (omega-register / kill-switch /
  observer-share / debug-camera / companion-view / net-send-state / net-
  recv-state / save-path / replay-load / audio-capture / telemetry-egress
  / audit-export / consent-revoke); PD0000..PD0017 stable diagnostic codes
  (one per §1 prohibition + spirit umbrella); **PD0007 weaponization and
  PD0009 torture carry ABSOLUTE BLOCK remedy strings — no consent path
  exists, per §7 INTEGRITY**; kill-switch with 1ms latency budget;
  attestation propagation with BLAKE3-pinned canonical text (text drift
  fails the test immediately).

### Phase I — LoA scaffold (project skeleton)

The Phase-I project skeleton landed at the v1.0 boundary. The scaffold
composes the entire Phase-H Substrate and the Phase-F host backends
into an end-to-end runtime that demonstrates the canonical 13-phase
`omega_step` running through every Substrate-touching layer. Game-content
concerns are deferred to per-`Q-*` Apocky-fill slices.

- **T11-D96** — S9-I0 LoA scaffold. New workspace crate `loa-game`
  (~3300 LOC + 49 tests) with 13 `OmegaSystem` impls (one per omega_step
  phase: consent → net-recv → input → sim → projections → audio → render-
  graph → render-submit → telemetry → audit → net-send → save → freeze);
  Engine struct that owns the tick scheduler + save scheduler + world +
  player + companion + apockalypse + camera + held caps; the canonical
  `Player` / `Companion` / `Apockalypse` archetypes per `specs/31_
  LOA_DESIGN.csl`; the load-bearing `one_omega_step_runs_all_thirteen_
  phases` integration test that drives one tick and asserts each
  `loa.phase-XX.*` telemetry counter incremented exactly once; the
  `save_then_load_round_trips_bit_equal` test that proves R-10 bit-
  equal-replay invariant survives at the LoA boundary; structural
  mapping of all 38 Q-A..Q-LL spec-holes to `Stub` enum variants
  (Apocky-fill territory). Canonical "Apockalypse" spelling preserved
  via a defensive `canonical_spelling_preserved` test that asserts no
  "Apocalypse" leak.

### Session-close v1.0

- **T11-D98** — Session-close v1.0 release author-out. CHANGELOG.md +
  README.md + RELEASE_NOTES_v1.0.md + HANDOFF_v1_to_PHASE_I.csl
  authored at the v1.0 release boundary. Docs-only slice; zero test
  impact.

### Pre-Phase-H baseline (sessions 1 through 5)

Sessions 1 through 5 produced the foundation that Phase-A built atop. The
canonical entries are recorded in [DECISIONS.md](DECISIONS.md) — T1 through
T11-D50. Summary:

- **T1** — Workspace scaffold + `§§-23-faithful` CI skeleton.
- **T2** — Dual-surface lexer (Rust-hybrid `logos` + CSLv3-native hand-rolled).
- **T3** — Parser (hand-rolled recursive-descent + Pratt for expressions);
  CST node types; HIR lowering with name-resolution and `lasso` interning.
- **T3.4** — HM type inference + effect-row unification; refinement-checker;
  IFC label-lattice; let-generalization; macro hygiene; staged comptime;
  AD legality.
- **T4** — `cssl-effects` registry + sub-effect discipline + Prime-Directive
  banned composition.
- **T5** — Pony-6 caps + AliasMatrix + subtype + LinearTracker + GenRef +
  `cap_check` pass.
- **T6** — `cssl-mir` + `cssl-mlir-bridge` + body lowering.
- **T7** — AutoDiff (forward + reverse mode) + walker dispatch +
  AnalyticExpr + scene-SDF gradient verification (the killer-app gate).
- **T8** — Jets + Staging + Macros + Futamura.
- **T9** — Predicate-text → SMT-Term translator; Z3/CVC5-CLI solvers;
  Lipschitz arithmetic-interval encoding.
- **T10** — 5 GPU backends text-emission (cranelift / SPIR-V / DXIL / MSL
  / WGSL); 5 host catalogs (Vulkan / Level-Zero / D3D12 / Metal / WebGPU).
- **T11** (sessions 4-5) — Real BLAKE3 + Ed25519 crypto; R18 ring +
  audit-chain + orthogonal-persistence; F1-AD chain; killer-app gate
  (11/11 gradient-equivalence cases pass); JIT execution; multi-fn
  modules; turbofish + auto-monomorphization quartet (D38..D50).
- **T12** — Vertical-slice trilogy + cssl-examples integration tests +
  killer-app end-to-end F1-chain integration.

Test count at session-5 close: ~1553. Test count at v1.0 (this release):
**3495** (3460 post-Phase-H + 35 from the G7 cross-slice walker that closes
Phase-G end-to-end at T11-D97).

### What v1.0 means

- The compiler can produce executables (`csslc build foo.cssl -o foo.exe`)
  via either the cranelift or the hand-rolled native-x86-64 backend.
- The runtime, stdlib, and host-integration layer give CSSLv3 source
  programs heap, sum-types, growable containers, strings, file I/O,
  windowing, input, audio, networking, and five GPU backends.
- The Substrate provides the engine plumbing for any application that
  needs Ω-tensor state, deterministic replay, projections, capability-
  gated effect composition, save/load with bit-equal verification, and
  the PRIME_DIRECTIVE enforcement layer.
- The Phase-I scaffold (`loa-game`) demonstrates the full Substrate +
  host wiring end-to-end; game-content concerns are deferred to
  Apocky-fill slices that resolve the 38 Q-A..Q-LL spec-holes.
- No part of the compiler, runtime, or Substrate depends on a
  proprietary or rented service. Sovereignty is preserved end-to-end.

### What's next — Phase I content authoring

Phase I content authoring resolves the 38 SPEC-HOLE markers (Q-A through
Q-LL) in `specs/31_LOA_DESIGN.csl`. The structural primitives (Companion-
projection as AI-sovereign-partner; ConsentZones with revocation-
degrades-gracefully; Apockalypse-Engine as engine-layer phase
transitions) are well-defined and ready for content authoring. The
Phase-I scaffold (T11-D96) provides the structural shape; per-Q content
slices replace `Stub` variants without changing the scaffold. See
[HANDOFF_v1_to_PHASE_I.csl](HANDOFF_v1_to_PHASE_I.csl) for the
resumption protocol.

---

There was no hurt nor harm in the making of this, to anyone, anything,
or anybody.
