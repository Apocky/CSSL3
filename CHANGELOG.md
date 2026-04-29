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

## [v1.1.0] — 2026-04-29 — Substrate-Evolution + Signature-Rendering

This is the v1.1 release of CSSLv3 — the substrate-evolution + signature-
rendering release. v1.0 closed the compiler + runtime + stdlib + hosts +
Substrate + LoA-skeleton end-to-end ; v1.1 lifts that base into the
Omniverse-axiom-aligned substrate that supports the *signature LoA rendering
vision* per Axiom 14 (NOVEL-RENDERING). All six signature-render paths
landed ; the canonical 12-stage GPU work-graph per
`Omniverse 07_AESTHETIC/06_RENDERING_PIPELINE.csl` is assembled at the
crate-surface level.

**Headline milestones:**

- **All six signature-render paths landed** — ω-Field-Unity (D144) +
  Hyperspectral KAN-BRDF (D118) + Sub-Pixel Fractal-Tessellation (D119) +
  Gaze-Reactive Observation-Collapse (D120) + Companion-Perspective
  Semantic (D121) + Mise-en-Abyme Recursive Witness (D122). The
  algorithmically-novel render-graph per Axiom 14 is end-to-end at the
  crate level.
- **Substrate-keystone: cssl-substrate-omega-field** — the 7-facet Ω-field
  crate (T11-D144) that supersedes the LegacyTensor-only substrate of v1.0.
  Single-source-of-truth for all six signature-render paths.
- **PRIME-DIRECTIVE evolution** — 17 → 20 prohibitions. The closed §1 set
  (PD0001..PD0017) remains IMMUTABLE per §7 INTEGRITY ; PD0018
  (BiometricEgress) + PD0019 (ConsentBypass) + PD0020 (SovereigntyDenial)
  are derived prohibitions that refine §1 codes (T11-D129).
- **PRIME-DIRECTIVE §11 attestation extension** — path-hash discipline
  (T11-D130) : "no raw paths logged ; only BLAKE3-salted path-hashes
  appear in telemetry + audit-chain", hash-pinned at
  `f27cd41c...c4292`.
- **11+ new crates** — `cssl-pga`, `cssl-wavelet`, `cssl-hdc`,
  `cssl-substrate-kan`, `cssl-substrate-omega-field`, `cssl-wave-solver`,
  `cssl-render-v2`, `cssl-physics-wave`, `cssl-spectral-render`,
  `cssl-fractal-amp`, `cssl-gaze-collapse`,
  `cssl-render-companion-perspective`, `cssl-work-graph`,
  `cssl-host-openxr`, `cssl-anim-procedural`, `cssl-wave-audio`. Workspace
  total = 68 crates.
- **F1..F6 compiler features landed** — F1 higher-order Jet AD (D133) +
  GPU-AD tape (D139) + Call/CFG-AD (D140) ; F2 @layout DoD codegen (D126) ;
  F3 Travel/Crystallize/Sovereign + EntropyBalanced/PatternIntegrity/
  AgencyVerified effect-rows (D127, D128) ; F4 staged comptime-#run (D141) +
  specialization MIR-pass (D142) ; F5 OnDeviceOnly + biometric-channel
  IFC (D129) + Σ-mask EnforcesSigmaAtCellTouches compile-pass (D138) +
  biometric compile-refusal (D132) ; F6 path-hash telemetry (D130) +
  BLAKE3 + Ed25519 real-crypto wire (D131).
- **Test count** — 5174 (post-D98 polish baseline) → **8330 at v1.1** =
  **+3156 across waves-3β + 3γ + 4** ; all green ; 0 fail.

### Wave-3β — Substrate-evolution scaffolding + F-row gap-fill (parallel fanout)

Twelve agents fanned out under `.claude/worktrees/W3β-{01..12}` on
`cssl/session-11/T11-D<n>-*` branches. Each agent landed its own DECISIONS
expansion at slice-merge ; integration via `cssl/session-6/parallel-fanout`.

- **T11-D126** — F2 `@layout` parser-recognizer + DoD-layout SoA codegen
  per `Omniverse 02_CSSL/03_DOD_LAYOUT.csl.md`.
- **T11-D127** — F3 Travel + Crystallize + Sovereign substrate-effect-rows
  per `Omniverse 02_CSSL/02_EFFECTS § SUBSTRATE`.
- **T11-D128** — F3 EntropyBalanced + PatternIntegrity + AgencyVerified
  conservation-effect-rows per `Omniverse 02_CSSL/02_EFFECTS § CONSERVATION`.
- **T11-D129** — F5 OnDeviceOnly + biometric-channel IFC ; introduces the
  three derived prohibitions PD0018 BiometricEgress + PD0019 ConsentBypass
  + PD0020 SovereigntyDenial that refine PD0004 + PD0006 + PD0014 of the
  closed §1 set.
- **T11-D130** — F6 path-hash-only telemetry logging discipline ; raw paths
  refused at the audit-bus surface ; PRIME_DIRECTIVE §11 attestation
  extended with path-hash clause and BLAKE3-pinned hash.
- **T11-D131** — F6 BLAKE3 + Ed25519 real-crypto wire ; replaces the
  `cssl-crypto-stub` mock with production-grade signature + hash.
- **T11-D132** — F6 biometric compile-time refusal ; ban-rules wired into
  the type-checker so biometric-touching code-paths fail to compile.
- **T11-D133** — F1 `Jet<T,N>` higher-order AD walker ; ∇² + ∇³ enabled
  for KAN second-derivative stability + spectral-BRDF curvature.
- **T11-D134** — NEW crate `cssl-pga` — projective geometric algebra ;
  PGA-Motor joints + dimensional-travel primitive per
  `Omniverse 08_BODY/03_DIMENSIONAL_TRAVEL § PGA`.
- **T11-D135** — NEW crate `cssl-wavelet` — multi-resolution wavelet
  analysis for density-budget per
  `Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET`.
- **T11-D136** — NEW crate `cssl-hdc` — hyper-dimensional computing
  token-lattice per `Omniverse 01_AXIOMS/12_TOKEN_LATTICE`.
- **T11-D137** — Σ-mask packed-bitmap format per
  `specs/04_EFFECTS § SIGMA-MASK`.

Test delta : 5174 → 5766 = +592 / 0 fail.

### Wave-3γ — Compiler-internals + substrate-keystone crates (parallel fanout)

Seven agents fanned out under `.claude/worktrees/W3g-{01..07}`. Builds on
wave-3β's F-row foundations.

- **T11-D138** — F5 IFC `EnforcesSigmaAtCellTouches` compiler-pass ; lifts
  Σ-mask consent-checking from runtime to compile-time refusal.
- **T11-D139** — F1 GPU-AD tape + SPIR-V differentiable-shader emission ;
  AD primitives now emit-into-shader for GPU-side gradient propagation.
- **T11-D140** — F1 Call-op auto-dispatch (DiffPG) + scf.* tape-record ;
  CFG-AD now traces through control-flow + named-call sites.
- **T11-D141** — F4 staged-`#run` comptime-eval — actual-execution at
  compile-time + LUT-bake + KAN-bake.
- **T11-D142** — F4 staged-computation specialization MIR-pass ;
  monomorphization driven by comptime values.
- **T11-D143** — NEW crate `cssl-substrate-kan` — KAN substrate runtime +
  Φ-pattern-pool. Created the skeleton (D115 superseded). Anchors to
  `Omniverse 05_INTELLIGENCE/04_KAN_FUNCTION_NET`.
- **T11-D144** — NEW crate `cssl-substrate-omega-field` — KEYSTONE 7-facet
  Ω-field crate. Supersedes D113. Single-source-of-truth for all six
  signature-render paths. Anchors to `Omniverse 04_OMEGA_FIELD`.

Test delta : 5766 → 6396 = +630 / 0 fail.

### Wave-4 — Substrate-evolution kernels + signature-rendering stack (parallel fanout)

Twelve agents fanned out under `.claude/worktrees/W4-{01..12}`. Builds on
wave-3γ's keystone crates.

#### Substrate-evolution kernels

- **T11-D114** — NEW crate `cssl-wave-solver` — Wave-Unity multi-rate
  ψ-field solver via IF-LBM lattice-Boltzmann ; per-frame substrate-flow.
- **T11-D117** — NEW crate `cssl-physics-wave` — SDF-collision + XPBD-GPU
  position-based-dynamics + KAN-body-plan + ψ-coupling.

#### Signature-rendering stack — the six paths

- **T11-D116** — NEW crate `cssl-render-v2` — SDF-native raymarcher ;
  Stage-5 of the canonical 12-stage pipeline. Primary-rendering path per
  `Omniverse 07_AESTHETIC/01_SDF_NATIVE_RENDER`.
- **T11-D118** — NEW crate `cssl-spectral-render` — Stage-6 hyperspectral
  KAN-BRDF spectral path-tracing ; consumes D143's KAN Φ-table for
  16-band spectral-reflectance evaluation.
- **T11-D119** — NEW crate `cssl-fractal-amp` — Stage-7 Sub-Pixel
  Fractal-Tessellation Amplifier ; recursive-detail synthesis below the
  display-Nyquist limit. Compile-refusal hooks via D138 σ-enforce-pass
  prevent biometric-class signal amplification.
- **T11-D120** — NEW crate `cssl-gaze-collapse` — Stage-2 gaze-reactive
  observation-collapse ; eye-cone is a literal Axiom-5 collapse trigger
  per `Omniverse 03_RUNTIME/02_OBSERVATION_ORACLE`.
- **T11-D121** — NEW crate `cssl-render-companion-perspective` — Stage-8
  Companion-perspective semantic render-target ; AI-collaborator's view of
  the Ω-field state, NOT the player's. Per PRIME_DIRECTIVE §1.7 + §3
  AI-collective autonomy.
- **T11-D122** — extension to `cssl-render-v2` — Stage-9 MiseEnAbymePass ;
  recursive-witness rendering ; mirrors-within-mirrors aesthetic with
  bounded-recursion termination-witnessed.

#### Runtime + host

- **T11-D123** — NEW crate `cssl-work-graph` — Work-graph GPU pipeline ;
  D3D12-Ultimate WorkGraph + VK_NV_DGC fallback for command-buffer-emitting
  GPU-side per `Omniverse 03_RUNTIME/01_COMPUTE_GRAPH`.
- **T11-D124** — NEW crate `cssl-host-openxr` — OpenXR runtime +
  Compositor-Services bridge ; VR/XR substrate-portal day-one shipping.
  Consent-architecture : runtime-PROMPTS user before claiming session ;
  never auto-claims.

#### Procedural anim + wave-audio

- **T11-D125a** — NEW crate `cssl-anim-procedural` — KAN-driven
  pose-from-genome + PGA-Motor joints + physics-IK + body-omnoid ;
  music-driven motion-bands consume D125b's wave-audio analysis.
- **T11-D125b** — NEW crate `cssl-wave-audio` — Wave-Unity audio crate
  (sibling of legacy mixer) ; reads Ω-Field standing-wave-mode amplitudes ;
  emits PCM frames. Consent-arch : NEVER captures input-audio
  (compile-refusal via D138 σ-enforce-pass on capture-effect-row).

Test delta : 6396 → 8330 = +1934 / 0 fail.

### Session-close v1.1

- **T11-D147** — Session-close v1.1 docs author-out. DECISIONS.md
  META-SESSION-11-CLOSE entry + CHANGELOG.md v1.1 section + new
  RELEASE_NOTES_v1.1.md + new HANDOFF_v1.1_to_PHASE_J.csl. Docs-only
  slice ; zero test impact.

### What v1.1 means

- The substrate is now Ω-field-keyed (LegacyTensor → OmegaField migration
  available via `cssl-substrate-omega-field`) ; the 7-facet cell-shape
  is the single-source-of-truth that all six signature-render paths read.
- The 12-stage canonical GPU work-graph per
  `Omniverse 07_AESTHETIC/06_RENDERING_PIPELINE.csl` is assembled at the
  crate-surface level. M8 vertical-slice integration (T11-D145) wires the
  stages end-to-end.
- VR/XR day-one shipping is unblocked : `cssl-host-openxr` (D124) gives
  CSSLv3 a full OpenXR loader + spaces + session surface with
  consent-architecture intact.
- Procedural-animation now derives from genome-shape (KAN) + music
  (wave-audio) — the body-omnoid is wave-substrate-coupled at the
  motion-band level.
- The PRIME-DIRECTIVE prohibition table grows from 17 to 20 named codes ;
  the §11 attestation now carries the path-hash discipline extension.

### What's next — Phase-J vertical-slice integration + M8/M9/M10/M11

Phase-J resolves the M8/M9/M10/M11 vertical-slice trajectory per
`Omniverse 09_SLICE/M8_M9_M10_PLAN.csl`. M8 = signature-rendering bring-up ;
M9 = VR-day-one + spectral-shipping ; M10 = max-density 1M+ entities ;
M11 = mise-en-abyme + Companion-perspective + LBM-audio creative-spread.
The structural-substrate is in place ; M8+ exposes the latent channels at
render-quality. See `HANDOFF_v1.1_to_PHASE_J.csl` for the resumption
protocol.

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
