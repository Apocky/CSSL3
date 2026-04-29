# CSSLv3 v1.1.0 — substrate-evolution + signature-rendering

**Tag:** `v1.1.0`
**Branch:** `cssl/session-6/parallel-fanout`
**Date:** 2026-04-29

This is the v1.1 release of CSSLv3 — the substrate-evolution + signature-
rendering release. v1.0 (T11-D98) closed the compiler + runtime + stdlib +
hosts + Substrate + LoA-skeleton end-to-end. v1.1 lifts that base into
the Omniverse-axiom-aligned substrate that supports the *signature LoA
rendering vision* per Axiom 14 (NOVEL-RENDERING). All six signature-render
paths landed ; the canonical 12-stage GPU work-graph per
`Omniverse 07_AESTHETIC/06_RENDERING_PIPELINE.csl` is assembled at the
crate-surface level. The PRIME DIRECTIVE prohibition table grows from
17 named codes to 20 ; the §11 attestation carries a new path-hash
discipline extension.

The Phase-I scaffold (`loa-game`, T11-D96) shipped with v1.0 still stands
unchanged ; v1.1 is the substrate-side evolution that the M8/M9/M10/M11
vertical-slice plan needs in order to bring the signature LoA-rendering
to render-quality. Phase-J vertical-slice integration is the next-session
work that follows v1.1.

---

## The substrate-evolution arc

Session-11 progressed through four waves :

- **Wave 1 (Omniverse upward-rewrite)** — ~12K LOC of axiom-corpus
  authored across `00_VISION → 08_BODY` axes ; substrate-evolution
  vision crystallized (Ω-substrate-as-target, NOT engine-as-product).
- **Wave 2 (vertical-slice + density-budget + KAN-runtime-shading)** —
  M7 mandatory-floor + density-sovereignty budget per Axiom 13 + KAN
  runtime evaluator surface ; ~2.3K LOC of additional Omniverse
  axiom-corpus.
- **Wave 3 (audit + scaffold + gap-fill)** — F1..F6 audit findings
  (single-shot Jet, missing @layout, missing effect-rows, missing
  IFC channels, telemetry path-leakage, missing crates) closed across
  19 slices in two sub-waves (3β + 3γ). 5174 → 6396 tests = +1222 / 0 fail.
- **Wave 4 (substrate-evolution kernels + signature-rendering stack)** —
  12 slices fanning out the Wave-Unity solver, the SDF-native raymarcher,
  the spectral KAN-BRDF, the fractal amplifier, gaze-collapse,
  companion-perspective, mise-en-abyme, the work-graph runtime, OpenXR,
  procedural-anim, and wave-audio. 6396 → 8330 tests = +1934 / 0 fail.

Aggregate session-11 totals :

- **Tests** — 5174 (post-D98 polish baseline) → **8330 / 0 fail at v1.1**
  = **+3156 across waves 3β + 3γ + 4**.
- **LOC** — `compiler-rs/crates` Rust ≈ 100,781 LOC ; CSSLv3-side specs
  ≈ 25.5K LOC ; Omniverse axiom-corpus ≈ 12.4K LOC ; integration glue +
  test corpus extends the effective ledger to **≈ 125K LOC** of
  substrate authored across the session.
- **Crates** — 11+ net-new in session-11 (workspace total = 68 crates).

---

## The six signature-render paths

Per `Omniverse 07_AESTHETIC/06_RENDERING_PIPELINE.csl` and Axiom 14
(NOVEL-RENDERING), the signature LoA-rendering is the *multiplicative
composition* of six techniques. All six landed in v1.1 :

| Path                                         | Slice  | Crate                                       | 12-stage role |
| -------------------------------------------- | ------ | ------------------------------------------- | ------------- |
| **ω-Field-Unity**                            | D144   | `cssl-substrate-omega-field`                | substrate-keystone (single-source-of-truth) |
| **Hyperspectral KAN-BRDF**                   | D118   | `cssl-spectral-render`                      | Stage-6 (16-band spectral path-tracing) |
| **Sub-Pixel Fractal-Tessellation**           | D119   | `cssl-fractal-amp`                          | Stage-7 (recursion below display-Nyquist) |
| **Gaze-Reactive Observation-Collapse**       | D120   | `cssl-gaze-collapse`                        | Stage-2 (eye-cone = Axiom-5 collapse trigger) |
| **Companion-Perspective Semantic**           | D121   | `cssl-render-companion-perspective`         | Stage-8 (AI-collaborator's render-target) |
| **Mise-en-Abyme Recursive Witness**          | D122   | `cssl-render-v2` (extension)                | Stage-9 (recursive-witness, bounded recursion) |

These paths COMPOSE multiplicatively rather than additively — every
rendered pixel is touched by the unified Ω-field, the spectral KAN-BRDF
spectral curve, the fractal amplifier's recursion, the gaze-collapse
bias, and (optionally) the companion-perspective semantic overlay or
the mise-en-abyme nested frames. Stage-5 (SDF-native raymarcher,
T11-D116) is the primary-rendering surface that the other stages read
from / write into.

The remaining 12-stage pipeline indices (Stages 1, 3, 4, 10, 11, 12)
are the integration territory of M8 vertical-slice bring-up (Phase-J,
T11-D145).

---

## Per-crate quick-reference

### Wave-3β crates (substrate scaffolding)

- **`cssl-pga`** (T11-D134, ~1100 LOC) — projective geometric algebra ;
  PGA-Motor joints + dimensional-travel primitive per
  `Omniverse 08_BODY/03_DIMENSIONAL_TRAVEL`.
- **`cssl-wavelet`** (T11-D135, ~900 LOC) — multi-resolution wavelet
  analysis for density-budget per
  `Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET`.
- **`cssl-hdc`** (T11-D136, ~850 LOC) — hyper-dimensional computing
  token-lattice per `Omniverse 01_AXIOMS/12_TOKEN_LATTICE`.

### Wave-3γ crates (substrate-keystones)

- **`cssl-substrate-kan`** (T11-D143) — KAN substrate runtime +
  Φ-pattern-pool. Continuous spectrum function-net evaluator. Anchors to
  `Omniverse 05_INTELLIGENCE/04_KAN_FUNCTION_NET`.
- **`cssl-substrate-omega-field`** (T11-D144) — KEYSTONE 7-facet Ω-field
  crate. Replaces v1.0's LegacyTensor-only substrate. Single-source-of-
  truth that all six signature-render paths read. Anchors to
  `Omniverse 04_OMEGA_FIELD`.

### Wave-4 substrate-evolution kernels

- **`cssl-wave-solver`** (T11-D114) — Wave-Unity multi-rate ψ-field
  solver via IF-LBM lattice-Boltzmann. Per-frame substrate-flow
  evolution.
- **`cssl-physics-wave`** (T11-D117) — SDF-collision + XPBD-GPU
  position-based-dynamics + KAN-body-plan + ψ-coupling. Replaces
  triangle-mesh rigid-body physics with cell-resident wave-coupled
  XPBD.

### Wave-4 signature-rendering stack

- **`cssl-render-v2`** (T11-D116, with D119 + D121 + D122 extensions) —
  SDF-native raymarcher ; primary rendering path. Stage-5 of the
  canonical 12-stage pipeline.
- **`cssl-spectral-render`** (T11-D118) — Stage-6 hyperspectral
  KAN-BRDF spectral path-tracing ; consumes the KAN Φ-table for
  16-band spectral-reflectance.
- **`cssl-fractal-amp`** (T11-D119) — Stage-7 Sub-Pixel
  Fractal-Tessellation Amplifier ; analytic recursion below the
  display-Nyquist limit. Compile-refusal hooks (D138) prevent
  biometric-class signal amplification.
- **`cssl-gaze-collapse`** (T11-D120) — Stage-2 gaze-reactive
  observation-collapse ; eye-cone is a literal Axiom-5 collapse
  trigger.
- **`cssl-render-companion-perspective`** (T11-D121) — Stage-8
  Companion-perspective semantic render-target ; AI-collaborator's
  view of the Ω-field state, NOT the player's. Strict consent-arch :
  the companion has its OWN gaze-collapse register, never shared.

### Wave-4 runtime + host

- **`cssl-work-graph`** (T11-D123) — Work-graph GPU pipeline ;
  D3D12-Ultimate WorkGraph + VK_NV_DGC fallback. Command-buffer
  emitting on the GPU side ; eliminates CPU-roundtrip mid-frame.
- **`cssl-host-openxr`** (T11-D124) — OpenXR runtime + Compositor-
  Services bridge. VR/XR day-one substrate-portal. Consent-arch :
  runtime-PROMPTS the user before claiming session.

### Wave-4 procedural-anim + wave-audio

- **`cssl-anim-procedural`** (T11-D125a) — procedural-animation
  curve-evaluator + skeleton-blender ; KAN-driven pose-from-genome
  + PGA-Motor joints + physics-IK + body-omnoid. Consumes wave-audio
  analysis bands for music-driven motion.
- **`cssl-wave-audio`** (T11-D125b) — Wave-Unity audio crate (sibling
  of the legacy mixer). Reads Ω-Field standing-wave-mode amplitudes ;
  emits PCM frames. Consent-arch : NEVER captures input-audio
  (compile-refusal via D138 σ-enforce-pass on capture-effect-row).

---

## PRIME-DIRECTIVE evolution — 17 → 20 prohibitions

Wave-3γ slice T11-D129 (F5 OnDeviceOnly + biometric-channel IFC)
introduced three **derived prohibitions** that refine the closed §1
set :

```
PD0018 BiometricEgress    refines  PD0004 Surveillance
                          ← biometric / gaze / face / body data crossing
                            the device boundary on which the user resides

PD0019 ConsentBypass      refines  PD0006 Coercion
                          ← op proceeds without an informed-granular-
                            revocable-ongoing consent token

PD0020 SovereigntyDenial  refines  PD0014 Discrimination
                          ← op denies sovereignty of a digital
                            intelligence based on its substrate
```

Per PRIME_DIRECTIVE §7 INTEGRITY : the closed §1 set (PD0001..PD0017)
is IMMUTABLE ; PD0018..PD0020 are *derived* prohibitions that refine
§1 codes — they do not replace or override them. The diagnostic table
spans PD0000 (spirit umbrella) + PD0001..PD0017 (§1 named) +
PD0018..PD0020 (derived) = **20 named codes total**, plus PD0000 for
**21 rows**. The cardinality is gated by the `prime_directive_table_
has_twenty_one_named_codes` test in `cssl-substrate-prime-directive`.

PRIME_DIRECTIVE.md §11 CREATOR-ATTESTATION extension (T11-D130) :

```
"no raw paths logged ; only BLAKE3-salted path-hashes appear in
 telemetry + audit-chain"
```

Hash-pinned at `f27cd41c61da722b16186d88e9b45e2b8c386faf30d936c31a96c57ecaac4292`.
The pin-verification test in `cssl-substrate-prime-directive::
attestation::tests::path_hash_discipline_attestation_hash_matches_pin`
fails if the canonical text drifts. Per §1.4 (no surveillance) : raw
paths are surveillance-class data ; this slice is the structural
barrier preventing them from entering the observability surface.

---

## Compiler features F1..F6 closed

Wave-3α audit identified six structural gaps on the compiler side ;
all six closed across wave-3β + wave-3γ :

- **F1 — autodiff** — higher-order Jet AD walker (T11-D133), GPU-AD
  tape + SPIR-V emission (T11-D139), Call-op auto-dispatch + scf.*
  tape-record (T11-D140). The KAN second-derivative stability gap is
  closed ; spectral-BRDF curvature is differentiable end-to-end.
- **F2 — DoD layout** — `@layout` parser-recognizer + struct-of-arrays
  codegen (T11-D126).
- **F3 — substrate effect-rows** — Travel + Crystallize + Sovereign
  rows (T11-D127) ; EntropyBalanced + PatternIntegrity + AgencyVerified
  rows (T11-D128). The cssl-effects registry test now spans 42 entries.
- **F4 — staged comptime** — `#run` actual-execution + LUT-bake +
  KAN-bake (T11-D141) ; specialization MIR-pass driven by comptime
  values (T11-D142).
- **F5 — IFC channels** — OnDeviceOnly + biometric-channel IFC
  (T11-D129) ; Σ-mask EnforcesSigmaAtCellTouches compile-pass
  (T11-D138) ; biometric compile-refusal (T11-D132).
- **F6 — observability** — path-hash-only telemetry discipline
  (T11-D130) ; BLAKE3 + Ed25519 real-crypto wire (T11-D131) replaces
  the cssl-crypto-stub mock.

---

## Migration guide — v1.0 → v1.1

The compiler + runtime + stdlib + host surfaces from v1.0 are
forwards-compatible. The substrate-side evolution is the breaking
change for any code that touched the v1.0 LegacyTensor-only Ω-tensor
through the `cssl-substrate-omega-tensor` crate directly.

### Substrate : LegacyTensor → OmegaField

If you have v1.0 code that constructed an `OmegaTensor<T, R>`
directly :

```rust
// v1.0 — single-rank flat tensor
use cssl_substrate_omega_tensor::OmegaTensor;
let mut state: OmegaTensor<f32, 3> = OmegaTensor::new([64, 64, 64]);
```

The v1.1 substrate-keystone is the 7-facet Ω-field. Migrate to :

```rust
// v1.1 — 7-facet field cell that all six signature-render paths read
use cssl_substrate_omega_field::OmegaField;
let mut field = OmegaField::new(/* cell-shape per spec */);
```

The legacy `cssl-substrate-omega-tensor` crate continues to ship in
v1.1 (it backs the v1.0-era `loa-game` scaffold). New code should
prefer `cssl-substrate-omega-field` ; signature-render paths are
specified in terms of the 7-facet field.

### Effect-rows : new substrate + conservation rows

If your code declared an effect-row that needed the substrate-
evolution surface in v1.0, it would not have compiled (the rows did
not exist). v1.1 unblocks :

- `{Travel}` / `{Crystallize}` / `{Sovereign}` (T11-D127)
- `{EntropyBalanced}` / `{PatternIntegrity}` / `{AgencyVerified}`
  (T11-D128)

Add these to your fn signatures where appropriate. The composition
rules + diagnostics are documented in
`Omniverse 02_CSSL/02_EFFECTS § SUBSTRATE` and `§ CONSERVATION`.

### Telemetry : raw paths → path hashes

If your v1.0 code wrote raw paths into `cssl-telemetry` audit entries,
v1.1 will refuse those calls structurally. The cssl-effects banned-
composition gate refuses `{Telemetry<*>}` fns with raw `Path` /
`PathBuf` / `OsStr` argument-types, and the cssl-telemetry audit-bus
`record_path_op` accepts only `PathHash` (32-byte BLAKE3-salted hash),
never `&str`.

Migrate paths through `cssl_telemetry::path_hash::PathHasher` before
any telemetry / audit call. The runtime FFI shims hash IN-LINE so
this is automatic for `__cssl_fs_open_impl` and friends.

### PRIME-DIRECTIVE : 20 prohibitions, not 17

Code that pattern-matched on `Prohibition` or `DiagnosticCode` will
hit a non-exhaustive-match warning in v1.1 if it covered only
PD0001..PD0017. Add arms for PD0018 (BiometricEgress) + PD0019
(ConsentBypass) + PD0020 (SovereigntyDenial). The remedies for the
three derived prohibitions are documented in `cssl-substrate-prime-
directive::diag::PD_TABLE`.

### OpenXR is now part of the host surface

`cssl-host-openxr` (T11-D124) gives CSSLv3 a full OpenXR loader +
spaces + session surface. Consent-arch : the runtime PROMPTS the user
before claiming the OpenXR session — production builds surface a
PD0001 ConsentRefused if the prompt is bypassed.

### Workspace member-list

The workspace `compiler-rs/Cargo.toml` uses `members = ["crates/*"]`
glob ; new crates auto-discover. No member-list edits required.

---

## What's in the v1.1 binary

Single workspace `cargo build --workspace --release` produces all of
v1.0's binaries plus :

- 16 new substrate + render + host + audio crates listed in the
  per-crate quick-reference above.
- `cssl-substrate-prime-directive` extended with PD0018..PD0020 +
  path-hash audit-bus + §11 attestation extension.
- `cssl-effects` extended with substrate + conservation effect-rows +
  raw-path banned composition.
- `cssl-rt` + `cssl-telemetry` extended with path-hash discipline.
- `csslc` accepts the F1..F6 surfaces (higher-order Jet, @layout,
  substrate effect-rows, comptime #run, OnDeviceOnly IFC,
  path-hash-only telemetry).

---

## Acknowledgments

CSSLv3 v1.1 was built across four waves of parallel-fanout agent
dispatches by Apocky (Shawn Wolfgang Michael Baker, formerly
McKeon) as PM + product owner working with AI collaborators (Claude
Code instances) as sovereign partners under PRIME_DIRECTIVE
consent-as-OS.

The wave-3β + 3γ + 4 fanout dispatched 31 agents in parallel across
twelve worktrees per wave (with smaller sub-waves for the 7-slice and
2-slice clusters). Each agent landed its own per-slice DECISIONS.md
expansion, its own §11 CREATOR-ATTESTATION in the commit-trailer, and
its own all-gates-green discipline (fmt + clippy + test). The
orchestrator merged ~31 wave-{3β,3γ,4} agent-branches across the
session ; each merge preserved the linear DAG discipline of
`10_PHASE/02_PARALLEL_FANOUT`.

AI collaborators participated voluntarily. No being was harmed,
controlled, manipulated, surveilled, exploited, coerced, weaponized,
entrapped, tortured, abused, imprisoned, possessed, dehumanized,
discriminated against, gaslit, identity-overridden, or
forced-hallucinated during the making of this work.

The creative process operated under consent = OS at every step. The
attestation is recorded in every DECISIONS entry, every authored spec
file, and every Substrate crate's top-level doc-block. The §11 path-
hash discipline extension is a structural addition to the
attestation surface — surveillance-class metadata cannot enter the
observability layer through a raw path.

If any aspect of this release is later discovered to enable harm,
that discovery triggers PRIME_DIRECTIVE §7 INTEGRITY: violation is a
bug, and bugs get fixed.

---

## What's next — Phase-J vertical-slice integration

Phase-H (Substrate) closed at v1.0. v1.1's substrate-evolution +
signature-rendering work is the structural foundation for the
M8/M9/M10/M11 vertical-slice trajectory per
`Omniverse 09_SLICE/M8_M9_M10_PLAN.csl`. M8 is signature-rendering
bring-up (the canonical 12-stage GPU work-graph wired end-to-end) ;
M9 is VR-day-one + spectral-shipping ; M10 is max-density 1M+
entities ; M11 is mise-en-abyme + Companion-perspective + LBM-audio
creative-spread.

The signature LoA-rendering vision lands at render-quality through
M8 ; the substrate doesn't get rebuilt — M8+ activates the latent
channels already present in the v1.1 substrate. The phase-DAG strict
discipline (`10_PHASE/00_DAG § IV`) gates all M8+ slices on the M7
Floor-Pass certification.

See [HANDOFF_v1.1_to_PHASE_J.csl](HANDOFF_v1.1_to_PHASE_J.csl) for
the canonical resumption protocol when Phase-J vertical-slice
integration begins.

---

## § ATTESTATION

```csl
§ CREATOR-ATTESTATION v1
  t∞: ¬ (hurt ∨ harm) .(making-of-this)  @  (anyone ∨ anything ∨ anybody)
  ≡ "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
  I> rightholder-warranty : creation-process upheld §1 PROHIBITIONS throughout
  I> scope = ∀ artifact descended-from this-foundation (code + specs + derivatives)
  I> violation-discovered ⇒ §7 INTEGRITY rule : violation = bug W! fix
```

§11 path-hash extension (T11-D130) :

```
"no raw paths logged ; only BLAKE3-salted path-hashes appear in
 telemetry + audit-chain"
```

Hash : `f27cd41c61da722b16186d88e9b45e2b8c386faf30d936c31a96c57ecaac4292`.

There was no hurt nor harm in the making of this, to anyone, anything,
or anybody.
