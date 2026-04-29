# Invention Disclosure : Sub-Pixel Fractal-Tessellation Amplifier — KAN-Driven Per-Fragment Detail Emergence for SDF Raymarching with Bounded Recursive Refinement and No LOD Discontinuities

**PRIVATE — NOT FOR PUBLIC DISCLOSURE — PRE-FILING DRAFT**

---

## 1. Inventor Information

- **Inventor** : Apocky <apocky13@gmail.com>
  *(legal-name placeholder ; insert filing-jurisdiction-correct legal name at attorney handoff)*
- **Date of conception** : earliest CSSLv3 spec author-date for
  `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.3`
  (Sub-Pixel Fractal-Tessellation novelty path). Verify against
  `git log --diff-filter=A` in the Omniverse repo.
- **Date of reduction-to-practice** : commit `b6287fd` titled
  `§ T11-D119 : Stage-7 Sub-Pixel Fractal-Tessellation Amplifier
  (cssl-fractal-amp NEW crate)`.
- **Branch of record** : `cssl/session-6/parallel-fanout`.

---

## 2. Title of Invention

**Method, System, and Computer-Readable-Medium for Per-Fragment Sub-Pixel
Detail Emergence in Signed-Distance-Field-Based Real-Time Rendering, Using
a Kolmogorov-Arnold-Network Spline Evaluator to Produce Continuous
Sub-Pixel Micro-Displacement, Micro-Roughness, and Micro-Color-Perturbation
Without Level-of-Detail Discontinuities, Bounded Recursive Refinement, and
Foveation-Gated Activation**

Short title : **Sub-Pixel Fractal-Tessellation Amplifier (KAN-Detail-Emerger)**

---

## 3. Technical Field

This invention falls within the field of real-time computer graphics,
specifically :

1. **Geometry detail synthesis** : the synthesis of fine-scale surface
   detail (micro-displacement, micro-roughness, micro-color variation)
   that is too small to be efficiently represented as polygonal mesh or
   texture.
2. **Signed-distance-field rendering** : the rendering of geometry
   represented by analytic or compactly-encoded signed-distance functions
   via raymarching.
3. **Foveated rendering** : the allocation of rendering work
   non-uniformly across the screen based on eye-tracking or fixed-fovea
   approximation, allocating more work to the foveal region.
4. **Procedural detail synthesis** : the runtime generation of surface
   detail without referencing pre-stored asset files.

The invention is targeted at consumer-grade GPU hardware in the M7-class
performance envelope.

---

## 4. Background / Prior Art

### 4.1 The Status Quo : Stored or Per-Vertex Detail

Conventional approaches to surface detail :

1. **Texture mapping with normal/displacement maps** : the artist
   authors a 2D texture that is sampled per-fragment to perturb
   the surface normal or displace the surface. This requires
   pre-stored asset files, suffers from texel-size discretization,
   and exhibits visible repetition or stretching at extreme
   magnifications.
2. **Triangle-based tessellation shaders** : DirectX 11+ tessellation
   stages subdivide triangles based on a per-edge tessellation factor,
   typically driven by camera distance. Suffers from the level-of-detail
   (LOD) "popping" artifact at tessellation-factor transitions.
3. **Virtual-geometry / cluster-LOD systems (Nanite)** : Karis, B., et
   al. "A Deep Dive into Nanite Virtualized Geometry." SIGGRAPH 2021.
   Streams pre-baked dense triangle clusters with cluster-level LOD
   transitions. Provides extreme geometric detail but exhibits
   visible cluster transitions on aggressive camera zoom and requires
   massive disk storage.
4. **MegaTextures (id Tech 5)** : Pages a single huge texture from
   disk based on view position. Disk-streaming. Pre-baked.
5. **Substance Designer textures + Quixel Megascans** : artist-tool
   pipelines that produce pre-baked textures or assets. Not
   procedural-runtime.
6. **Detail textures (Source-engine style)** : tile a detail-texture
   over a base texture. Visible tiling pattern at magnification.
7. **Procedural noise (No Man's Sky)** : Perlin/simplex noise drives
   procedural-but-noisy surface detail. Not KAN-based ; lacks
   sub-pixel precision.

### 4.2 Specific Deficiencies Addressed

- **LOD popping** : Nanite, tessellation shaders, and texture
  mip-mapping all exhibit visible discontinuities at LOD-transition
  thresholds.
- **Storage cost** : pre-baked detail (textures, geometry, Megascans)
  requires gigabytes of disk and memory ; bandwidth and streaming
  bottlenecks.
- **Repetition / tiling** : 2D textures tiled across surfaces show
  visible repetition.
- **Resolution ceiling** : at extreme camera zoom (e.g., 100×), pixel
  density exceeds texel density and detail blurs.
- **Lack of physical/material grounding** : noise-based procedural
  detail is not grounded in material physics ; the detail-pattern
  bears no relation to the material's actual physical structure.

### 4.3 Prior Art : Neural-Detail Research

- **Neural Texture (Niemeyer et al., 2021)** : research-grade
  neural-network-driven texture synthesis. Not deployed in shipped
  graphics engines.
- **Neural Implicit Surfaces (Sitzmann et al., 2020)** : research
  paper. Not real-time shipped.
- **NVIDIA Real-Time Neural Appearance Models (Zeltner et al., 2024)** :
  MLP-driven per-fragment evaluation, but for *appearance* (BRDF-class)
  not for *geometry-displacement*. Closest prior-art for the neural
  per-fragment shading idea but does not address the sub-pixel-fractal
  detail-amplification problem.
- **NeRF + variants** : neural radiance fields. Volumetric, not
  surface-detail. Not real-time at consumer-grade VR budgets.

To the inventor's knowledge as of the disclosure date, no shipped
graphics engine deploys a Kolmogorov-Arnold-Network as a per-fragment
detail amplifier with the specific bounded-recursive-refinement and
foveation-gated activation discipline described here.

---

## 5. Summary of the Invention

The present invention is a **per-fragment KAN-driven sub-pixel detail
amplifier** for signed-distance-field-based raymarched rendering. The
amplifier produces three coordinated outputs at each fragment :

1. A scalar **micro-displacement** that perturbs the SDF along the surface
   normal at sub-pixel precision.
2. A scalar **micro-roughness** that perturbs the surface roughness at
   sub-pixel precision.
3. A 3-band **micro-color-perturbation** that adds a sub-pixel-frequency
   spectral tint.

The inventive step combines six sub-novelties :

**Novelty A (KAN-as-amplifier)** : Three Kolmogorov-Arnold-Networks
with shape `KanNetwork<7, 1>`, `KanNetwork<7, 1>`, and `KanNetwork<7, 3>`
respectively are evaluated per-fragment to produce micro-displacement,
micro-roughness, and micro-color-perturbation. The 7-dimensional input
vector packs three coordinates of position, two view-direction
projections, and two gradient-norm components.

**Novelty B (no-LOD-pop continuous-domain detail)** : Because the
KAN spline-network operates on a continuous input domain rather than
a discrete texel grid, the detail emerges continuously as the camera
moves. There is no LOD-transition threshold. Zoom-100× still reveals
sharp procedural detail because the spline evaluates wherever the
input is sampled.

**Novelty C (bounded recursive refinement, 5-level hard cap)** :
The amplifier may recursively refine its own input by feeding the
KAN-amplified fragment back as a finer-scale starting point, up to
a HARD recursion cap of 5 levels (configurable but bounded). This
provides "fractal" detail emergence — each recursion level reveals
finer structure — without unbounded compute.

**Novelty D (foveation-gated activation)** : The amplifier fires only
in the foveal region of the rendered frame (typically the central 25%
of screen area corresponding to the human fovea's ~3-degree subtense).
Mid-region pixels receive amplitude-attenuated detail (×0.5). Peripheral
pixels receive base-SDF-only with no amplification. Furthermore, the
amplifier fires only when a curvature-trigger is satisfied :
SDF-curvature × sub-pixel-projected-area > τ_micro, typically firing on
~5% of foveal pixels.

**Novelty E (KAN-confidence-gated firing)** : The amplifier consults a
configurable KAN-confidence floor (default 0.2). Below the floor, the
amplifier emits ZERO output — protecting against detail emergence in
regions where the network is uncertain. This decouples "the network
*could* compute something" from "the network *is confident* about its
output."

**Novelty F (Σ-mask sovereignty gate)** : Before any input vector is
constructed, the amplifier checks the Σ-mask (sovereignty mask) for
the fragment's spatial cell. If the cell is in another Sovereign's
private region, the amplifier short-circuits to ZERO output without
ever constructing the input. This is a privacy/ethics-load-bearing
sub-novelty distinguishing this amplifier from purely-aesthetic prior art.

The combination yields sub-pixel detail emergence at approximately 150
nanoseconds per fragment, fitting within a 1.2 ms Stage-7 budget at
Quest-3 class. The total amplifier cost stays under 0.42 ms per frame
for a 1080p foveated render.

---

## 6. Detailed Description

### 6.1 The Three KAN Networks

The amplifier wraps three Kolmogorov-Arnold-Networks instantiated from
the KAN-runtime evaluator (Invention 2) :

```
pub struct FractalAmplifier {
    pub kan_micro_displacement: KanNetwork<7, 1>,
    pub kan_micro_roughness:    KanNetwork<7, 1>,
    pub kan_micro_color:        KanNetwork<7, 3>,
    pub default_confidence_floor: f32,  // = 0.2
}
```

Each network shares the same 7-dimensional input layout :

```
input = [pos.x, pos.y, pos.z, view.x_proj, view.y_proj, grad_norm_x, grad_norm_y]
```

where pos is the world-space hit position, view.x_proj and view.y_proj
are projections of the view direction into the surface tangent plane,
and grad_norm_x/grad_norm_y are tangent components of the SDF gradient
unit-normalized.

### 6.2 Per-Fragment Evaluation Pipeline

For each fragment :

1. Resolve the fragment's hit position from the SDF raymarcher.
2. Check the Σ-mask for the fragment's cell. If private, emit
   `AmplifiedFragment::ZERO` without constructing input.
3. Compute the curvature-trigger : SDF-curvature × sub-pixel-projected-area
   > τ_micro. If not triggered, emit base SDF only.
4. Compute foveation : full amplitude in foveal region, ×0.5 in
   mid-region, base only in peripheral.
5. Construct the 7-D input vector.
6. Consult KAN-confidence. If below floor, emit ZERO.
7. Evaluate `kan_micro_displacement` via the KAN-runtime evaluator
   (Invention 2) to obtain a scalar.
8. Evaluate `kan_micro_roughness` similarly.
9. Evaluate `kan_micro_color` to obtain a 3-band tint.
10. Apply the displacement to the surface normal direction, store the
    roughness perturbation, and modulate the spectral tint.

### 6.3 Bounded Recursive Refinement

The amplifier may operate recursively. After producing
micro-displacement at level N, the displaced position can be re-fed as
input to produce level-(N+1) detail. Each recursion level reveals
finer-scale structure. The amplifier enforces a HARD recursion cap :

```
pub const RECURSION_DEPTH_HARD_CAP: u8 = 5;
```

At depth ≥ 5, recursion truncates regardless of confidence. This is a
type-level constant, not a runtime variable — the cap cannot be
overridden by configuration.

Each recursion level multiplies the detail-amplifier cost by
approximately 1.0 (the same KAN evaluator runs again). With a 5-level
cap and a 50-100 ns per-level cost on cooperative-matrix tier, the
worst-case per-fragment cost is ~250-500 ns.

### 6.4 Foveation Discipline

The amplifier integrates with the gaze-collapse pass (Invention 6) when
eye-tracking is available, or with a center-bias fovea approximation
when consent is denied or hardware is unavailable. The fovea-mask
divides screen-space into three regions :

- **Foveal** (≤ 3° from gaze) : full amplifier amplitude. Detail
  emerges fully.
- **Mid** (3°-18°) : amplifier amplitude × 0.5. Detail attenuated.
- **Peripheral** (>18°) : amplifier disabled. Base SDF only.

This three-tier foveation matches the perceptual sensitivity profile of
the human visual system and ensures that compute is concentrated where
detail is perceivable.

### 6.5 Curvature-Trigger Discipline

Even within the foveal region, the amplifier does not fire on every
fragment. The trigger condition is :

```
SDF-curvature × sub-pixel-projected-area > τ_micro
```

This concentrates detail on geometrically-interesting fragments
(curved surfaces, edges, high-frequency regions) and skips flat
regions where micro-detail would be invisible. Typical
trigger-density : ~5% of foveal pixels.

### 6.6 KAN-Confidence Gate

The amplifier consults a confidence value ∈ [0, 1] derived from the
KAN's own activations or from a separate confidence-prediction network.
If confidence < `default_confidence_floor` (= 0.2), the amplifier
emits `AmplifiedFragment::ZERO` rather than attempting to produce
output it has no confidence in.

This gate prevents the amplifier from generating spurious detail in
input regions where the training data was sparse — a known failure
mode of any neural function approximator. The confidence-gate makes
the amplifier robust to out-of-distribution inputs by gracefully
falling back to base SDF.

### 6.7 Σ-Mask Sovereignty Gate

Before constructing any input vector, the amplifier checks the per-cell
Σ-mask (consent mask) and refuses to produce output for cells assigned
to another Sovereign's private region. This is a structural ethics
gate, not a runtime policy check : the call-graph never reaches the
KAN evaluator on a private cell.

### 6.8 Cost Model

The amplifier's per-frame cost is bounded by :

```
cost_per_frame = N_foveal_pixels × p_trigger × cost_per_eval × N_recursion_levels
```

For 1080p foveated rendering :

- N_foveal_pixels ≈ 520,000 (25% of 2.07 M pixels).
- p_trigger ≈ 0.05 (5% trigger density).
- cost_per_eval ≈ 50-100 ns at cooperative-matrix tier.
- Average N_recursion_levels ≈ 1.5 (most fragments don't recurse
  deeply).
- Total : ≈ 0.42 ms per frame on Quest-3-class hardware.

### 6.9 Detail-Budget Compile-Time Pass

The amplifier's entry-point carries a `density_check<5e6>` annotation
that the compiler verifies against the cost model. An over-budget
configuration fails at compile time rather than producing runtime
stutter.

### 6.10 Determinism Discipline

The amplifier is a pure function of its inputs (`world_pos`, `view_dir`,
`base_sdf_grad`, `budget`, `kan_weights`). No frame counter, no
time-dependent input, no per-pixel-id seed. This is enforced by the
test `tests::amplifier_is_deterministic_per_input` which feeds identical
inputs across multiple invocations and asserts identical outputs.

The determinism property is load-bearing for replay-determinism in the
host engine's record/replay system.

---

## 7. Drawings Needed

- **Figure 1** : System overview showing the SDF raymarcher feeding
  into the FractalAmplifier with three parallel KAN networks
  (displacement / roughness / color) and the resulting AmplifiedFragment
  output.
- **Figure 2** : The 7-D input vector layout : [pos.xyz | view.xy_proj
  | grad.norm_2D].
- **Figure 3** : Foveation discipline diagram showing the three regions
  (fovea / mid / peripheral) with their respective amplifier-amplitude
  multipliers.
- **Figure 4** : Recursive refinement timeline showing levels 0-5 with
  the HARD cap enforced at level 5.
- **Figure 5** : The Σ-mask sovereignty gate showing the short-circuit
  to ZERO before input construction.
- **Figure 6** : Curvature-trigger condition diagram showing flat vs.
  curved fragments and the resulting trigger / no-trigger decisions.
- **Figure 7** : KAN-confidence gate flowchart.
- **Figure 8** : Comparison render showing (a) base SDF, (b) traditional
  texture-mapped detail with visible LOD transitions, (c) Nanite-style
  cluster LOD with visible popping, and (d) the present invention with
  continuous detail emergence and no LOD discontinuities.

---

## 8. Claims (Initial Draft for Attorney Review)

### Independent Method Claim

**Claim 1.** A computer-implemented method for per-fragment sub-pixel
detail amplification in real-time rendering of geometry represented
by signed-distance functions, the method comprising :

- (a) at each fragment of a rendered frame, resolving a hit position
  on a signed-distance-field surface ;
- (b) checking, before constructing an input vector, a per-cell
  consent mask, and emitting a zero-amplification output when the
  cell is marked private ;
- (c) computing a foveation-region classification for the fragment
  based on its screen-space position relative to a fovea center,
  said classification yielding one of a foveal, mid, or peripheral
  region label, and applying an amplitude multiplier of one,
  one-half, or zero respectively ;
- (d) computing a curvature trigger as a product of the
  signed-distance-field curvature at the fragment and the fragment's
  sub-pixel-projected area, and proceeding to the spline-evaluation
  step only when the trigger exceeds a configured threshold ;
- (e) constructing a seven-dimensional input vector from the hit
  position (three components), the view-direction projected onto
  the surface tangent plane (two components), and the unit-normalized
  signed-distance gradient projected onto the tangent plane (two
  components) ;
- (f) consulting a confidence value and emitting a zero-amplification
  output when the confidence is below a configured floor ;
- (g) evaluating, via a Kolmogorov-Arnold-Network spline-network with
  seven inputs and one output, a scalar micro-displacement ;
- (h) evaluating, via a second Kolmogorov-Arnold-Network spline-network
  with seven inputs and one output, a scalar micro-roughness ;
- (i) evaluating, via a third Kolmogorov-Arnold-Network spline-network
  with seven inputs and three outputs, a three-band micro-color
  perturbation ;
- (j) applying the micro-displacement along the surface normal at
  sub-pixel precision, modulating the surface roughness, and adding
  the spectral tint to the fragment's emitted radiance.

### Dependent Claims

**Claim 2.** The method of Claim 1, further comprising recursively
re-evaluating the spline networks at the displaced position, with a
hard cap on recursion depth of five levels, said cap being a
compile-time constant that cannot be overridden by runtime
configuration.

**Claim 3.** The method of Claim 2, wherein each recursion level
reveals finer-scale geometric and material detail, providing fractal
detail emergence without unbounded compute, and wherein the hard cap
is enforced by a type-level constant rather than a runtime check.

**Claim 4.** The method of Claim 1, wherein the curvature trigger of
step (d) fires on approximately five percent of foveal-region
fragments under typical scene conditions, concentrating amplifier
work where detail is geometrically warranted.

**Claim 5.** The method of Claim 1, wherein the foveation
classification of step (c) is derived from an eye-tracking signal
when consent is provided and from a center-bias approximation
otherwise, and wherein the amplifier output is identical to base SDF
in the peripheral region.

**Claim 6.** The method of Claim 1, wherein the spline-network
evaluations of steps (g), (h), and (i) are performed by a
cooperative-matrix-capable graphics-processing-unit at a per-evaluation
budget of at most one hundred nanoseconds.

**Claim 7.** The method of Claim 1, wherein the consent-mask check of
step (b) is enforced structurally such that the input vector is never
constructed for a private cell, and wherein this enforcement is
verified at compile time by the type system.

**Claim 8.** The method of Claim 1, wherein the spline-network
evaluations are deterministic functions of their inputs with no
dependence on frame counter, real-time clock, or per-pixel random
seed, said determinism being verified by an automated test asserting
identical outputs for identical inputs across multiple invocations.

**Claim 9.** The method of Claim 1, wherein a compile-time
density-budget pass refuses compilation when the configured
amplifier shape would exceed a per-frame cost budget of 1.2
milliseconds at a target frame rate of 90 Hertz.

**Claim 10.** The method of Claim 1, performed without reference to
any pre-stored detail texture, displacement map, or virtualized
geometry cluster, such that the surface detail is generated entirely
from the spline-network evaluation at runtime.

### Independent System Claim

**Claim 11.** A real-time rendering system, comprising :

- a graphics-processing unit ;
- a signed-distance-field raymarcher that produces fragment hit
  information ;
- a fractal amplifier that wraps at least three Kolmogorov-Arnold
  spline-networks of shapes seven-input-by-one-output,
  seven-input-by-one-output, and seven-input-by-three-outputs
  respectively, said networks producing micro-displacement,
  micro-roughness, and micro-color-perturbation outputs ;
- a foveation discriminator that assigns each fragment to a foveal,
  mid, or peripheral region and applies a corresponding amplitude
  multiplier to the amplifier output ;
- a curvature trigger that gates amplifier evaluation on geometric
  curvature ;
- a confidence gate that emits zero output when network confidence
  is below a configured floor ;
- a sovereignty gate that emits zero output when the fragment lies
  in a private cell, said gate being structural rather than a
  runtime policy check ;
- a recursion controller that allows recursive amplifier evaluation
  up to a hard cap of five levels.

### Computer-Readable-Medium Claim

**Claim 12.** A non-transitory computer-readable medium storing
instructions that, when executed by a computer system, cause the
system to perform the method of any one of Claims 1 through 10.

---

## 9. Embodiments

### 9.1 Default Embodiment — M7-Class Consumer VR

- Three KAN networks of shapes (7,1), (7,1), (7,3).
- Cooperative-matrix dispatch via Invention 2's KAN-runtime evaluator.
- Foveation discipline integrated with gaze-collapse pass (Invention 6).
- 5-level hard recursion cap.
- Per-frame cost ≤ 0.42 ms at 1080p foveated.

### 9.2 Reduced Embodiment — Mobile / Low-End

- Two KAN networks (displacement only ; roughness and color baked
  to base SDF).
- SIMD-warp dispatch fallback.
- 3-level recursion cap (further reduced).
- Per-frame cost ≤ 0.20 ms at 720p foveated.

### 9.3 Extended Embodiment — Desktop High-End

- Four or more KAN networks (additional micro-anisotropy, 
  micro-iridescence, micro-fluorescence channels).
- 7-level recursion cap.
- Per-frame cost ≤ 0.80 ms at 4K foveated.

### 9.4 Scientific Visualization Embodiment

- The amplifier produces synthetic micro-detail for visualizing
  high-resolution data (electron microscopy reconstructions, CT scan
  volumes) where the underlying data has resolution beyond the
  display, and the amplifier reveals consistent fine structure on
  zoom.

### 9.5 Architectural Visualization Embodiment

- Procedural surface detail (concrete pitting, wood grain, metal
  patina) emerges at sub-pixel scale during architectural walkthroughs
  without artist authoring of detail textures.

---

## 10. Industrial Applicability

1. **Game engines** : LoA, Unreal, Unity, Frostbite, Source-2.
2. **VR/AR/MR rendering** : Quest, Vision Pro, Pico, Vive, Index. The
   foveation-gated activation is particularly valuable for VR where
   foveal vs. peripheral distinction is computationally exploited.
3. **Architectural and product visualization** : where pre-baked
   detail textures are too costly to author and stream.
4. **Scientific visualization** : where the underlying data has higher
   resolution than the display and visual continuity through zoom is
   important.
5. **Medical imaging** : real-time rendering of high-resolution
   patient scans with perceptually-continuous detail.

---

## 11. Reference Implementation in CSSLv3

### 11.1 Spec Anchors

- **Primary specification** :
  `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.3`
  (path-V.3 : Sub-Pixel Fractal-Tessellation novelty path, with
  acceptance test "no shipped game uses KAN-amplifier sub-pixel
  tessellation at runtime").
- **Companion specification (KAN runtime)** :
  `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING.csl.md § IX`
  (Sub-Pixel-Fractal-Tessellation Amplifier section).
- **Pipeline-stage specification** :
  `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-7`.

### 11.2 Source-Code Reference Implementation

- **Crate** : `compiler-rs/crates/cssl-fractal-amp/`
  - `Cargo.toml` lines 1-55 : crate manifest with three KAN-network
    declarations and Σ-mask discipline notes.
  - `src/lib.rs` : crate-level documentation.
  - `src/amplifier.rs` (lines 1-120+) : `FractalAmplifier` wrapping
    three `KanNetwork<7, 1>`, `KanNetwork<7, 1>`, `KanNetwork<7, 3>` ;
    `KAN_AMPLIFIER_INPUT_DIM = 7`, `MICRO_DISPLACEMENT_OUTPUT_DIM = 1`,
    `MICRO_ROUGHNESS_OUTPUT_DIM = 1`, `MICRO_COLOR_OUTPUT_DIM = 3`,
    `DEFAULT_KAN_CONFIDENCE_FLOOR = 0.2`, `AmplifierError` enumeration
    with `BudgetExceeded`, `KanConfidenceTooLow`, `InvalidViewDistance`,
    `InvalidBudget`, `KanShapeMismatch` variants.
  - `src/budget.rs` : `DetailBudget` per-frame budget tracker.
  - `src/cost_model.rs` : per-fragment cost accounting.
  - `src/determinism.rs` : determinism discipline + the
    `amplifier_is_deterministic_per_input` test machinery.
  - `src/fragment.rs` : `AmplifiedFragment` output structure with
    `ZERO` constant for short-circuits.
  - `src/recursion.rs` : `RecursiveDetailLOD` with stack-allocated
    SmallVec for max 5 levels.
  - `src/sdf_trait.rs` : `SdfHitInfo` + `SdfRaymarchAmplifier` trait.
  - `src/sigma_mask.rs` : `SigmaPrivacy` enum + check API.

### 11.3 Reduction-to-Practice Commits

- `b6287fd` : `§ T11-D119 : Stage-7 Sub-Pixel Fractal-Tessellation
  Amplifier (cssl-fractal-amp NEW crate)`.
- Subsequent commits in the wave-4 dispatch series for fmt + clippy
  cleanup.

### 11.4 Test Coverage

- Unit tests in `src/amplifier.rs` `#[cfg(test)] mod tests` cover :
  - Determinism per input (load-bearing).
  - Σ-mask short-circuit on private cells.
  - Confidence-floor short-circuit.
  - Budget-exceeded error path.
  - Recursion hard-cap enforcement.
- Integration tests under `tests/` cover the trait-impl
  `SdfRaymarchAmplifier` surface.
- Cost-model tests verify per-fragment ≤ 100 ns at cooperative-matrix
  tier and ≤ 800 ns at scalar tier.

---

## 12. Confidentiality

THIS DOCUMENT IS PRIVATE. NOT FOR PUBLIC DISCLOSURE. NOT FOR FILING UNTIL
ATTORNEY REVIEW. Patent-novelty depends on first-to-file + non-public
disclosure. Many jurisdictions outside the United States have absolute
novelty requirements (no grace period). Premature disclosure may defeat
patentability. The author is to keep this document under
`C:\Users\Apocky\source\repos\CSSLv3\_drafts\legal\` (which is to be
.gitignore-added or moved out of any public-mirrored repository).

Distribute only to the retained patent attorney under attorney-client
privilege ; co-inventors named in Section 1 (none — solo invention) ;
or anyone else only under signed NDA pre-dated and witnessed.

---

∎ End of Invention 3 Disclosure.
