# Invention Disclosure : Mise-en-Abyme Recursive Witness — Bounded-Recursion Frame-in-Frame Rendering with KAN-Confidence-Driven Early Termination, Companion-AI Perceptual Render-Target Embedding, and Per-Region Anti-Surveillance Gating for Real-Time Mirror, Reflective-Eye, and Still-Water Surfaces

**PRIVATE — NOT FOR PUBLIC DISCLOSURE — PRE-FILING DRAFT**

---

## 1. Inventor Information

- **Inventor** : Apocky <apocky13@gmail.com>
  *(legal-name placeholder ; insert filing-jurisdiction-correct legal name at attorney handoff)*
- **Date of conception** : earliest CSSLv3 spec author-date for
  `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.6`
  (Mise-en-Abyme novelty path), and
  `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-9`.
  Verify against `git log --diff-filter=A` on those spec files.
- **Date of reduction-to-practice** : Stage-9 implementation under
  `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/`. The
  `cssl-render-v2` crate is part of the substrate-evolution wave
  landed at commit `b69165c`.
- **Branch of record** : `cssl/session-6/parallel-fanout`.

---

## 2. Title of Invention

**Method, System, and Computer-Readable-Medium for Bounded
Recursive Frame-in-Frame Rendering of Reflective Surfaces in
Real-Time Computer Graphics, Comprising a Hard-Capped Recursion
Depth, a Kolmogorov-Arnold-Network-Based Confidence Attenuation
Function for Soft Early Termination, an Optional Companion
Artificial-Intelligence Perceptual-Render-Target Channel for
Reflective Eyes of Non-Player Characters, and Per-Region
Anti-Surveillance Boundary Enforcement Preventing the Recursive
Witness from Crossing Sovereignty Lines**

Short title : **Mise-en-Abyme Recursive Witness (Stage-9)**

---

## 3. Technical Field

This invention falls within the field of real-time computer
graphics, specifically :

1. **Recursive reflection rendering** : the rendering of mirrored
   surfaces, glossy reflective surfaces, reflective creature eyes,
   and still-water surfaces, where the reflection itself contains
   reflections recursively to a finite depth, often called
   "mise-en-abyme" or "image-within-image" rendering.
2. **Bounded-recursion compute pipelines** : the construction of
   per-frame compute pipelines whose recursion depth is hard-capped
   at compile-time so that the pipeline is provably terminating and
   bounded in both space and time.
3. **Confidence-attenuation** in neural-augmented rendering : the
   use of a small neural network to decide, at each recursion level,
   whether further recursion will produce a visually-perceptible
   contribution and therefore whether to early-terminate.
4. **Mixed-reality and virtual-reality rendering** : per-eye
   stereoscopic rendering in head-mounted displays, where the
   recursive witness must produce two slightly-different recursive
   views from two slightly-different camera positions per frame.
5. **Bystander-safety and consent-architecture in mixed reality** :
   the prevention of recursive-reflection rendering from crossing
   per-region sovereignty boundaries (e.g., a private region whose
   contents are masked off must not appear inside a mirror in a
   public region).

The invention is targeted at consumer-grade GPU hardware in the
M7-class performance envelope within head-mounted-display
per-frame budgets and within the Stage-9 sub-budget of 0.8 ms at
Quest-3 reference hardware and 0.6 ms at Vision-Pro reference
hardware.

---

## 4. Background / Prior Art

### 4.1 The Status Quo : Cube-Map and Planar-Mirror Approximations

In conventional real-time graphics engines, reflections of mirrors
and glossy surfaces are approximated by a small fixed set of
techniques :

**Cube-map reflections** sample a low-resolution six-face cube-map
that is either pre-baked or rendered once per frame from a
representative scene point. The cube-map is then sampled by
reflective fragments. The cube-map cannot capture the recursive
property — a mirror inside the cube-map renders as a static
texture, not as a further reflective surface.

**Planar mirror via additional render pass** is the technique used
for flat-water and flat-mirror surfaces : the scene is rendered
twice (once normally, once with the camera flipped across the
mirror plane), and the second render is composited onto the mirror
surface with a Fresnel weight. This is a single-bounce
approximation ; mirrors visible inside the second render are again
flat textures, not further reflective.

**Screen-space reflections** ray-march in screen-space along the
reflection vector, sampling the screen-space depth and color
buffers. This is faster than a second render pass but is limited
to surfaces visible in the original screen-space view ; mirrors
inside mirrors are again not properly recursive.

**Path-traced reflections** in offline production renderers
(Mental Ray, Arnold, V-Ray, Manuka, RenderMan) are recursive by
construction : a reflective bounce spawns a child path that bounces
recursively until either a maximum-depth-cap or a Russian-roulette
termination is reached. These are not real-time and not deployed
on consumer-grade GPU hardware in production today.

**Real-time path tracing on the highest-end GPUs** (NVIDIA RTX
Blackwell/Lovelace classes with hardware ray tracing) supports
recursive bounces, but the typical recursion-depth cap in shipping
games is 1 or 2 bounces, and recursion depth is dynamic
(determined by Russian roulette) rather than hard-capped at
compile-time.

### 4.2 The Specific Deficiencies Addressed by This Invention

**Deficiency 1 — no hard-cap, no provability.** Production
real-time path tracers use Russian-roulette termination, which is
unbounded in the worst case. For real-time deployment in safety-
critical mixed-reality contexts (where a frame deadline missed
causes user discomfort or motion-sickness), a hard-capped, provably-
terminating recursion is required.

**Deficiency 2 — no perceptual gating of recursion.** A common
artistic complaint about recursive mirrors is that the deeper
levels of recursion produce visually-noisy or visually-redundant
contributions. The deeper levels are expensive to render but, in
practice, contain very little new information once attenuation,
defocus, and glossiness have accumulated. There is no production
technique that decides, per-pixel-per-recursion-level, whether
the next level will produce a perceptible contribution.

**Deficiency 3 — no companion-AI render-target channel.** The
reflective eyes of non-player-characters (creature eyes, character
eyes, robot eyes) in production games either reflect the static
environment cube-map, the planar-mirror render, or are textured
with a static iris texture. There is no production technique that
allows a Companion AI's perceptual render-target — the AI's
"world view from the eyes of the character" — to be embedded in
the iris reflection. (This is a novel artistic device made
available by the present invention's per-eye 16-band radiance
buffer wired through the recursive-witness pass.)

**Deficiency 4 — no anti-surveillance per-region boundary.**
Existing recursive-reflection techniques have no concept of
"private regions whose contents must not appear in mirrors in
public regions." This is a sovereignty / consent-architecture
concern not previously addressed by the prior-art real-time-
rendering community. Bystander-present mixed-reality contexts
require this in any system that respects consent architecture per
the CSSLv3 PRIME DIRECTIVE.

### 4.3 The Closest Prior Art

**RenderMan, Arnold, V-Ray, Manuka offline path tracers** :
recursive but not real-time, no perceptual gating, no anti-
surveillance per-region.

**Lumen / Nanite (Unreal Engine 5)** : screen-space + voxel /
distance-field cone tracing for indirect light, single-bounce
mirror approximation, no recursive witness.

**MetaHuman creature-eye shaders** : iris textures with parallax-
mapping, no companion-AI render-target embedding.

**OpenXR / Meta horizon worlds** : per-region content boundaries
for social-VR, but no recursive-reflection enforcement of those
boundaries.

The novel combination of bounded-hard-capped recursion, KAN-
confidence-driven early termination, Companion-AI render-target
embedding, and per-region anti-surveillance boundary enforcement,
all within a real-time per-frame budget on consumer GPU hardware,
has not appeared in any prior art known to the inventor as of the
date of this disclosure.

---

## 5. Summary of the Invention

The invention is a real-time recursive-reflection render pass
positioned as Stage-9 of a 12-stage canonical render pipeline.
It consumes the output of Stage-8 (post-fractal-amplified
spectral-illuminated radiance) and produces a recursive-witness-
augmented per-eye spectral radiance buffer that downstream
stages composite onto the final image.

The novel features are :

1. **Hard-capped recursion depth.** The recursion depth is bounded
   by a compile-time constant `RECURSION_DEPTH_HARD_CAP = 5`. The
   constant is exposed to the runtime as a `const` so that no
   configuration, environment variable, runtime-condition, or
   command-line argument can exceed it. Per-frame, per-platform
   wall-clock budgets are enforced separately (0.8 ms at Quest-3,
   0.6 ms at Vision-Pro) by a cost-model gate that may early-
   terminate before reaching the hard cap.

2. **KAN-confidence early-termination.** A
   Kolmogorov-Arnold-Network confidence evaluator runs at every
   recursion level. The evaluator inputs are : the current
   recursion depth, the accumulated attenuation product, the
   surface roughness at the current bounce, the per-band radiance
   variance estimate, and the foveation index of the affected
   pixel tile. The output is a scalar confidence value in [0, 1]
   that decides whether the next recursion level should proceed
   ("continue=true") or whether the partial accumulated radiance
   is the final answer ("continue=false"). Soft termination
   (continue=false) is allowed at any depth ≤ hard cap.

3. **Mirror-surface detection.** A `MirrorSurface` SDF detector
   identifies which fragments are reflective (mirror, eye, still-
   water) by inspecting the per-fragment material embedding's
   "mirrorness channel" (an axis of the embedding). The detector
   produces a per-fragment mirrorness scalar in [0, 1] ; fragments
   with mirrorness above a threshold enter the recursive witness ;
   others bypass it and use the Stage-8 base radiance.

4. **Companion-AI iris render-target embedding.** For non-player-
   character eyes, the recursive witness optionally embeds the
   Companion AI's perceptual render-target — a 16-band per-eye
   radiance buffer rendered from the character's perspective — as
   the deepest recursion level's content. This is the artistic
   device that gives the character's eyes "what they see," not
   merely what an environment cube-map gives.

5. **Per-region anti-surveillance boundary.** The recursive witness
   honors per-region content boundaries. A `RegionBoundary`
   structure carries a per-region policy that decides, for each
   recursion level, whether the witness is allowed to cross the
   boundary. Private regions (e.g., the volume occupied by another
   user's body in a shared mixed-reality space) are masked off from
   the recursive witness so that their contents do not appear in
   mirrors visible from public regions.

6. **Per-eye stereoscopic recursive witness.** The Stage-9 pass
   produces two slightly-different recursive-witness buffers per
   frame, one for each eye, with the recursion seeded from the
   per-eye camera position. The 16-band spectral representation
   is preserved through the recursive witness so that downstream
   tonemap (Stage-10) sees per-eye 16-band data.

7. **Bounded budget cost model.** A `MiseEnAbymeCostModel` gate
   runs in parallel with the recursion and reports, per-recursion-
   level, the wall-clock cost consumed thus far. When the
   per-platform budget is exceeded, the gate signals a soft-
   termination to the next recursion-level invocation.

The novel combination of these elements, operating in a single
render pass within a 0.8 ms / 0.6 ms Stage-9 budget, is the
inventive step.

---

## 6. Detailed Description

### 6.1 System Overview

The Mise-en-Abyme Recursive Witness pass is Stage-9 of the
12-stage canonical render pipeline. It receives :

- the per-eye 16-band spectral-illuminated radiance buffer from
  Stage-8 (after global-illumination + post-fractal-amplification
  + spectral-illumination + post-FX),
- the per-fragment `RayHit` records from Stage-1 (carrying surface
  position, normal, material id, and per-cell sovereignty mask),
- the foveation mask from Stage-2,
- the per-region policy table from the substrate runtime,
- the optional Companion-AI iris-render-target buffer from the
  per-character render path (if any).

It produces :

- a per-eye 16-band recursive-witness-augmented spectral radiance
  buffer that replaces the Stage-8 output for fragments whose
  mirrorness exceeds the threshold.

### 6.2 Hard-Capped Recursion Depth

The recursion depth hard cap is a compile-time constant :

```rust
pub const RECURSION_DEPTH_HARD_CAP: u8 = 5;
```

The cap is enforced by a `RecursionDepthBudget` structure that
tracks the current depth and refuses to issue a further bounce
when the cap is reached. Refusal returns
`Stage9Error::RecursionDepthExhausted { depth, hard_cap }` and
causes the partial accumulated radiance to be the final answer
for the affected ray.

The choice of 5 is empirically grounded :

- depth 0 : the primary view (no recursion needed),
- depth 1 : the first reflection (the most-perceptually-important),
- depth 2 : reflection-of-reflection (still perceptible in
  high-quality mirrors),
- depth 3 : two-deep recursion (perceptible only in tightly-
  controlled scenes such as parallel-mirror corridors),
- depth 4 : three-deep recursion (rarely-perceptible, included
  for high-quality demonstrations and for anti-aliasing of the
  deeper levels),
- depth 5 : four-deep recursion (the absolute cap ; below this
  level the contribution is ≤ 1% of the primary radiance for
  any plausible material configuration and is bypassed by the
  KAN-confidence gate in nearly all cases).

The hard-cap of 5 was chosen as the smallest cap such that the
typical artistic intent of recursive-mirror imagery is preserved
without exposing the system to unbounded compute.

### 6.3 KAN-Confidence Early Termination

At each recursion level `d ∈ {1..5}`, a small KAN evaluator
produces a confidence value :

```
c_d = KAN_confidence(
    d,                   // current depth
    α_accumulated,        // product of per-bounce attenuations
    ρ_d,                  // surface roughness at this bounce
    σ²_radiance,          // per-band radiance variance estimate
    foveation_index)      // per-tile foveation index
```

The KAN topology is small (4 input edges, 4-node hidden, 4 hidden
edges, 1-output) and the spline coefficients are quantized to
8 bits per coefficient and stored in a per-pass uniform buffer.
The KAN is evaluated once per recursion level per ray.

The decision is : if `c_d < MIN_CONFIDENCE` (a constant, e.g.,
0.05), then early-terminate ; else, recurse.

Importantly, the foveation-index input means peripheral rays
preferentially early-terminate (because the human visual system
cannot perceive the recursive detail in peripheral regions) while
foveal rays preferentially recurse. This is a coupling of the
recursive-witness budget to the foveation mask that production
engines do not exhibit.

### 6.4 Mirror Surface Detection

The `MirrorSurface` SDF detector inspects the per-fragment
`MaterialEmbedding`'s `mirrorness` channel — an axis of the
material embedding (e.g., `m_31`'s mirror-bit and `m_30`'s
roughness baseline are jointly read). A surface is considered
"reflective enough to enter the recursive witness" when :

```
mirrorness = (1 - ρ) · m_metal + m_mirror_bit
```

exceeds a threshold (e.g., 0.65). Below the threshold, the
fragment uses the Stage-8 radiance directly and bypasses the
recursive witness.

Mirror surfaces have three characteristic types :

1. **Hard mirror** (wall mirror, polished metal) : `m_mirror_bit = 1`
   and `ρ ≈ 0`, so `mirrorness ≈ 1`. The recursive witness produces
   a near-sharp reflection.
2. **Reflective creature eye** (NPC iris) : `m_mirror_bit = 0`,
   `m_metal ≈ 0.6`, `ρ ≈ 0.2`. The recursive witness produces a
   slightly-glossy reflection ; this is the channel into which the
   Companion-AI iris render-target is optionally composited.
3. **Still-water surface** (lake, puddle, pool) : `m_mirror_bit = 0`,
   `m_metal = 0` (water is dielectric), `ρ ≈ 0.05`. The recursive
   witness produces a Fresnel-weighted reflection of the
   above-surface scene.

### 6.5 Companion-AI Iris Render-Target Channel

For non-player characters whose eyes are reflective and whose
character ID has a Companion-AI association, the recursive witness
deepest level (depth 5 by default, or earlier if confidence-gated)
embeds the Companion-AI's perceptual render-target. The render-
target is a 16-band per-eye spectral radiance buffer rendered from
the character's eye position, not from the player's eye position.

The Companion-AI render-target represents the AI's perceptual
state in a way that is visible to a careful observer. Artistically,
this is the device of "the eyes that see." Implementationally, it
is wired via the `CompanionEyeWitness` module which exposes an
`IrisDepthHint` that tells the recursive witness "stop at depth N
and use the Companion-AI buffer there."

The Companion-AI render-target is itself produced under the same
sovereignty and consent-architecture rules as the primary render :
it does not consume biometric data, does not mutate substrate
state, is replayable and deterministic given the same Companion-
AI internal state, and respects per-region anti-surveillance
boundaries.

### 6.6 Per-Region Anti-Surveillance Boundary

The `RegionBoundary` structure carries a `RegionPolicy` per region
identifier. Policies include :

- `RegionPolicy::Public` — recursive witness may freely enter
  this region.
- `RegionPolicy::Private` — recursive witness may NOT enter ; the
  region is masked off from the witness (its contents replaced
  with a uniform black or a region-specific opaque color).
- `RegionPolicy::ConsentRequired` — recursive witness may enter
  only if the affected ray's source has a per-frame consent flag
  set ; otherwise the region is masked off.

The region boundary is checked at every recursive level. When a
ray crosses from a public region into a private region during a
mirror bounce, the bounce contribution is masked off so that the
private region's contents do not appear in the mirror.

This is the load-bearing anti-surveillance feature : without it,
a public-region wall mirror could capture the private contents of
an adjacent volume occupied by another user's body, defeating the
sovereignty-preserving intent of the system.

### 6.7 Per-Eye Stereoscopic Recursive Witness

The Stage-9 pass produces two recursive-witness buffers per
frame, one for each eye. The recursion is seeded from the per-eye
camera position. The 16-band spectral representation is preserved
through the recursive witness so that downstream tonemap (Stage-10)
sees per-eye 16-band data.

Per-eye recursive witnesses must be consistent : the same physical
mirror in the world must produce a consistent reflection across
the two eyes (with the appropriate stereoscopic disparity). This
is enforced by feeding identical KAN-confidence gates and identical
region-boundary policies to both eye passes.

### 6.8 Cost Model and Budget Gate

The `MiseEnAbymeCostModel` exposes a `RuntimePlatform` enum
(`Quest3`, `VisionPro`, `Desktop`) and the corresponding budgets :

- `STAGE9_BUDGET_QUEST3_US: u32 = 800` (0.8 ms),
- `STAGE9_BUDGET_VISION_PRO_US: u32 = 600` (0.6 ms).

A wall-clock counter ticks at the start of each recursion level.
When the cumulative cost exceeds the platform budget, the cost
model raises `Stage9Error::BudgetExceeded { used_us, budget_us }`
and the compositor falls back to the most-recent partial radiance
for any rays that have not yet finished. This is a soft failure
that does not crash the frame ; it merely truncates the recursive
witness for the affected rays.

### 6.9 Reference Implementation in CSSLv3

The reference implementation lives in
`compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/`. The
modules are :

- `mod.rs` — Stage-9 module-level documentation, public constants
  (`RECURSION_DEPTH_HARD_CAP = 5`, budget constants), and error
  surface (`Stage9Error`).
- `pass.rs` — `MiseEnAbymePass` top-level + `RecursionDepthBudget`
  enforcement.
- `compositor.rs` — `WitnessCompositor` per-frame attenuated
  composition.
- `mirror.rs` — `MirrorSurface` SDF detector + `KanMaterial`-based
  mirrorness-channel inspection.
- `confidence.rs` — `KanConfidence` confidence evaluator + the
  `MIN_CONFIDENCE` constant.
- `radiance.rs` — `MiseEnAbymeRadiance` per-eye 16-band buffer.
- `companion.rs` — `CompanionEyeWitness` Companion-AI iris path-5
  link + `IrisDepthHint`.
- `region.rs` — `RegionBoundary` anti-surveillance gate.
- `probe.rs` — `MirrorRaymarchProbe` Stage-5-replay shim that
  invokes the Stage-1 raymarcher on a recursive bounce.
- `cost.rs` — `MiseEnAbymeCostModel` budget-gate.

### 6.10 Algorithmic Pseudocode

```
function MiseEnAbymePass::execute(eye, fragments) :
    let radiance_buffer = SpectralRadianceBuffer::from_stage8(eye)
    for fragment in fragments :
        let mirrorness = MirrorSurface::detect(fragment.material)
        if mirrorness < τ_mirror :
            continue                      // bypass ; use Stage-8 base
        let mut accumulated = SpectralRadiance::zero()
        let mut attenuation = 1.0
        let mut depth = 0
        let mut current = fragment.position
        let mut current_dir = fragment.reflection_dir
        loop :
            depth += 1
            if depth > RECURSION_DEPTH_HARD_CAP :
                break                     // hard cap ; partial result
            if cost_model.exceeded_budget() :
                break                     // soft cap ; partial result
            let next_hit = MirrorRaymarchProbe::trace(current, current_dir)
            let region = RegionBoundary::lookup(next_hit.position)
            if region.policy == Private :
                break                     // anti-surveillance boundary
            let level_radiance = SpectralRender::evaluate(next_hit, ...)
            accumulated += level_radiance * attenuation
            let conf = KanConfidence::evaluate(
                depth, attenuation, next_hit.roughness,
                level_radiance.variance(), foveation_index_of(fragment))
            if conf < MIN_CONFIDENCE :
                break                     // soft ; KAN early-term
            attenuation *= per_bounce_attenuation(next_hit)
            current = next_hit.position
            current_dir = next_hit.reflection_dir
        if companion_eye_witness_applies(fragment) :
            accumulated += CompanionEyeWitness::contribute(fragment)
        radiance_buffer.write(fragment.pixel, accumulated)
    return radiance_buffer
```

---

## 7. Drawings Needed

The patent attorney should commission the following figures :

- **Figure 1** : System architecture diagram showing Stage-9
  positioned in the 12-stage canonical render pipeline, with the
  recursive-witness data flow from Stage-8 base radiance and
  Stage-1 raymarcher inputs to the per-eye 16-band recursive-
  witness buffer output to Stage-10 tonemap.

- **Figure 2** : Recursion-depth diagram showing five recursion
  levels labeled 1..5, with the hard cap at level 5 and the soft-
  termination conditions (KAN-confidence < threshold, budget
  exceeded, region boundary crossed) labeled at each level.

- **Figure 3** : KAN-confidence evaluator topology diagram showing
  4 input edges (depth, attenuation, roughness, variance), 4-node
  hidden layer, 4 hidden edges, 1-output (confidence value), with
  each edge depicted as a B-spline.

- **Figure 4** : Mirror-surface detection diagram showing three
  mirror types (hard mirror, reflective creature eye, still-water
  surface) and their respective `mirrorness` values from the
  material embedding, with the threshold gate visible.

- **Figure 5** : Companion-AI iris render-target diagram showing
  the Companion-AI's per-eye spectral render-target buffer,
  rendered from the character's perspective, embedded as the
  deepest level of the recursive witness for that character's
  iris fragments.

- **Figure 6** : Region boundary diagram showing a public region
  containing a wall mirror, a private region adjacent, and the
  recursive witness's mask-off behavior at the boundary
  (private-region contents not appearing in the mirror).

- **Figure 7** : Per-eye stereoscopic diagram showing two slightly-
  different recursive-witness buffers per frame, with disparity
  between the two views as the per-eye camera position differs.

- **Figure 8** : Cost-model gate diagram showing the wall-clock
  counter, the per-platform budget threshold (0.8 ms at Quest-3,
  0.6 ms at Vision-Pro), and the soft-termination signal raised
  to the compositor.

- **Figure 9** : Comparative output renderings showing the same
  scene rendered (a) with no recursive witness (Stage-8 base
  radiance only), (b) with one-bounce mirror (production
  baseline), and (c) with the full present invention (5-level
  recursive witness with KAN-confidence gating).

- **Figure 10** : Companion-AI iris comparative rendering showing
  the same NPC character rendered (a) with a static iris texture
  (production baseline) and (b) with the Companion-AI render-
  target embedded in the iris (the present invention).

---

## 8. Claims (Initial Draft for Attorney)

### Claim 1 (Independent — Method)

A computer-implemented method for real-time recursive reflection
rendering, comprising :
- receiving, at a graphics processing unit, a base spectral-
  radiance buffer for a frame and a set of per-fragment ray hit
  records ;
- detecting, for each fragment, whether the fragment's surface
  material is reflective above a mirrorness threshold and, if so,
  initiating a recursive-witness procedure ;
- iterating up to a compile-time-fixed hard recursion depth cap
  of N levels, where N ≥ 3, performing at each level :
  (i) raymarching from the current bounce position along the
      reflection direction to find the next surface hit ;
  (ii) computing a Kolmogorov-Arnold-Network confidence score
       from the current depth, the accumulated attenuation, the
       surface roughness at the next hit, a per-band radiance-
       variance estimate, and a per-tile foveation index, and
       early-terminating the recursion if the confidence is below
       a threshold ;
  (iii) checking, against a per-region policy table, whether the
        next hit lies within a region whose policy permits
        recursive-witness entry, and masking off contributions
        from any private region ;
  (iv) accumulating per-band attenuated radiance from the next
       hit into a per-fragment recursive-witness accumulator ;
- producing, as output, a per-fragment spectral radiance value
  that replaces the base spectral-radiance for fragments whose
  mirrorness exceeded the threshold.

### Claim 2 (Dependent — Hard Cap)

The method of Claim 1 wherein the compile-time-fixed hard
recursion depth cap is exposed in the implementation as a
constant value, accessible to the runtime as a read-only constant,
not modifiable by configuration, environment variable, runtime
condition, or command-line argument.

### Claim 3 (Dependent — KAN Confidence Topology)

The method of Claim 1 wherein the Kolmogorov-Arnold-Network
confidence evaluator comprises a topology of four input edges, a
four-node hidden layer, four hidden edges, and a one-output layer,
each edge being a B-spline with quantized coefficients.

### Claim 4 (Dependent — Foveation Coupling)

The method of Claim 1 wherein the per-tile foveation index is
produced by an upstream eye-tracking-driven foveated-rendering
stage, and wherein peripheral rays preferentially early-terminate
under the KAN-confidence gate while foveal rays preferentially
recurse.

### Claim 5 (Dependent — Companion-AI Iris)

The method of Claim 1 further comprising :
- identifying, for at least one fragment, that the fragment is on
  a non-player-character's reflective iris surface and that the
  non-player-character has an associated artificial-intelligence
  perceptual-render-target buffer ;
- substituting the artificial-intelligence perceptual-render-
  target buffer at a designated recursion depth in the recursive-
  witness for that fragment, producing a "what the character
  sees" reflective channel.

### Claim 6 (Dependent — Per-Region Anti-Surveillance)

The method of Claim 1 wherein the per-region policy table is
populated with a policy per region identifier selected from at
least the set { Public, Private, ConsentRequired }, and wherein
the recursive-witness procedure masks off contributions from
private regions, optionally including contributions from
consent-required regions absent an explicit per-frame consent
flag.

### Claim 7 (Dependent — Per-Eye Stereo)

The method of Claim 1 wherein the recursive-witness procedure is
performed once per eye for a stereoscopic head-mounted-display
output, with each eye's recursion seeded from the corresponding
eye's camera position, and wherein the two eyes' recursive-
witnesses preserve consistency through identical KAN-confidence
gates and identical region-boundary policies.

### Claim 8 (Dependent — Spectral 16-Band)

The method of Claim 1 wherein the per-fragment spectral radiance
is represented as a sixteen-band hyperspectral vector, and
wherein the recursive-witness preserves the sixteen-band
representation throughout, without conversion to red-green-blue
tristimulus prior to a final tonemap stage.

### Claim 9 (Dependent — Cost-Model Soft Cap)

The method of Claim 1 further comprising a wall-clock cost model
that monitors the cumulative compute time consumed by the
recursive-witness procedure for the current frame and signals a
soft-termination to subsequent recursion-level invocations when
the cumulative time exceeds a per-platform budget threshold.

### Claim 10 (Independent — System)

A real-time graphics rendering system, comprising :
- a graphics processing unit ;
- a base spectral-radiance-buffer producer ;
- a recursive-witness fragment-shader compute kernel resident on
  the graphics processing unit, configured to perform the method
  of Claim 1 with N = 5 within a per-frame budget of less than
  one millisecond at a head-mounted-display target frame rate ;
- an output spectral-radiance-buffer writer.

### Claim 11 (Dependent — Quest-3 Class)

The system of Claim 10 wherein the head-mounted-display target
frame rate is at least 90 Hertz and wherein the recursive-witness
fragment-shader compute kernel completes in less than 0.8
milliseconds per frame at a 1080p-class rendering resolution per
eye.

### Claim 12 (Dependent — Vision-Pro Class)

The system of Claim 10 wherein the head-mounted-display target
frame rate is at least 90 Hertz and wherein the recursive-witness
fragment-shader compute kernel completes in less than 0.6
milliseconds per frame at a 2160p-class rendering resolution per
eye.

### Claim 13 (Independent — Computer-Readable Medium)

A non-transitory computer-readable medium storing instructions
that, when executed by a graphics processing unit, cause the
graphics processing unit to perform the method of Claim 1.

### Claim 14 (Dependent — Sovereignty Compliance)

The medium of Claim 13 wherein the instructions are configured
such that the recursive-witness procedure does not mutate
per-cell sovereignty-mask state and does not consume biometric
data, and wherein the procedure is replayable and deterministic
given identical inputs.

### Claim 15 (Dependent — Mirror Type Detection)

The method of Claim 1 wherein the detecting step distinguishes
among at least three mirror surface types : a hard mirror, a
reflective creature-eye iris, and a still-water surface, by
inspecting respective bit-fields and roughness coefficients of the
fragment's material embedding.

---

## 9. Embodiments

### Embodiment A : Quest-3 Class Mobile XR

The system runs on a Snapdragon-class mobile GPU. Hard cap N = 5.
KAN-confidence gating enabled. Companion-AI iris render-target
embedded for designated NPCs. Per-region anti-surveillance gate
enforced. Per-frame budget 11.1 ms (90 Hz) ; Stage-9 budget
0.8 ms.

### Embodiment B : Vision-Pro Class Mixed Reality

The system runs on Apple M-series GPU with metal3. Hard cap
N = 5. KAN-confidence gating enabled with tighter threshold to
take advantage of higher fidelity. Per-frame budget 8.3 ms
(120 Hz) ; Stage-9 budget 0.6 ms.

### Embodiment C : Desktop Discrete GPU

The system runs on a high-end discrete GPU. Hard cap N may be
configurable up to 7 (a strict superset of the production cap)
under a developer-mode flag, with the production deployment
defaulting to N = 5. KAN-confidence gating uses larger spline
coefficients (e.g., 16 bits per coefficient) for higher fidelity.

### Embodiment D : Companion-AI Disabled Variant

The system is deployed in a context where Companion-AI iris
embedding is not used (e.g., a non-character-driven application
such as architectural visualization). The recursive-witness
proceeds normally without the Companion-AI render-target
substitution.

### Embodiment E : Anti-Surveillance Strict Variant

The system is deployed in a strict-bystander-safety context
(e.g., a public-space mixed-reality experience with bystanders
present). The per-region anti-surveillance gate defaults to
Private for all unannotated regions, requiring explicit
annotation for any region whose contents are permitted to appear
in mirrors.

### Embodiment F : Audio-Recursive Variant

The recursive-witness mechanism is generalized from spectral
radiance to spectral pressure (audio), using the same hard cap
and KAN-confidence gate, to render acoustic-mirror reverberation
in environments with hard reflective walls. The cross-modal
generalization is enabled by the unified ψ-field substrate (see
Invention 1).

### Embodiment G : Material-Edit Mode

The system runs in a content-authoring tool where the artist can
adjust the mirrorness threshold, the hard-cap (within the
production-permitted range), and the KAN-confidence threshold
interactively, observing the per-frame budget impact in real
time. The production deployment locks these parameters at the
production-shipping values.

---

## 10. Industrial Applicability

This invention is industrially applicable in :

- Real-time computer-generated imagery for video games where
  recursive-mirror imagery is artistically used (parallel-
  mirror corridors, infinity-mirror puzzle mechanics, mirror-
  based stealth gameplay).
- Mixed-reality and virtual-reality applications, especially
  those that take place in environments containing mirrors,
  reflective-glass walls, or still-water surfaces.
- Content-driven character art where the perceptual state of a
  non-player character's eyes is artistically load-bearing
  (Companion-AI based games, narrative VR, MetaHuman-derivative
  characters).
- Public-space mixed-reality experiences where bystander privacy
  is a regulatory and ethical requirement.
- Architectural and interior-design visualization where the
  mirror placement is a load-bearing design element.

The licensing market includes head-mounted-display original
equipment manufacturers, game studios, content-authoring-tool
vendors, and any mixed-reality application developer subject to
bystander-privacy regulations.

---

## 11. Reference Implementation in CSSLv3

The reference implementation is in :

- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/mod.rs` —
  Stage-9 module-level documentation and public constants
  (`RECURSION_DEPTH_HARD_CAP = 5`, `STAGE9_BUDGET_QUEST3_US = 800`,
  `STAGE9_BUDGET_VISION_PRO_US = 600`).
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/pass.rs` —
  `MiseEnAbymePass` top-level + `RecursionDepthBudget` hard-cap
  enforcement.
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/compositor.rs` —
  `WitnessCompositor` per-frame attenuated composition.
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/mirror.rs` —
  `MirrorSurface` SDF detector, `MirrorDetectionThreshold`,
  `MirrornessChannel`.
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/confidence.rs` —
  `KanConfidence` evaluator + `MIN_CONFIDENCE` constant +
  `KanConfidenceInputs` / `KanConfidenceOutputs` types.
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/radiance.rs` —
  `MiseEnAbymeRadiance` per-eye 16-band spectral buffer,
  `BANDS_PER_EYE`, `EYES_PER_FRAME` constants.
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/companion.rs` —
  `CompanionEyeWitness`, `CompanionEyeWitnessError`, `IrisDepthHint`.
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/region.rs` —
  `RegionBoundary`, `RegionId`, `RegionPolicy` (anti-surveillance
  gate).
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/probe.rs` —
  `MirrorRaymarchProbe`, `ConstantProbe`, `ProbeResult`
  (Stage-5-replay shim invoking the Stage-1 raymarcher on a
  recursive bounce).
- `compiler-rs/crates/cssl-render-v2/src/mise_en_abyme/cost.rs` —
  `MiseEnAbymeCostModel`, `RuntimePlatform` budget gate.

The verbatim spec anchors are :

- `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.6` —
  the path-V.6 specification of the six-novelty-paths roster
  (the immutable Mise-en-Abyme entry).
- `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-9` —
  pipeline-position, budget, bounded-recursion, effect-row.
- `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md` —
  bounded-recursion as a direct AGENCY-INVARIANT corollary.

---

## 12. Confidentiality

THIS DOCUMENT IS PRIVATE. NOT FOR PUBLIC DISCLOSURE.

Patent novelty law allows the inventor a one-year grace period
after public disclosure in the United States ; outside the United
States, premature public disclosure is generally fatal to
patentability. Accordingly, this document MUST NOT be shared,
posted, committed to a public repository, distributed, or
otherwise made publicly accessible until either (a) a provisional
patent application has been filed claiming the present invention
as a priority date, or (b) the inventor and patent counsel have
agreed in writing that public disclosure is permissible.

Distribution of this document is limited to :
- the inventor (Apocky / legal-name-of-record),
- patent counsel of record,
- co-inventors and assignees with a written non-disclosure agreement,
- such persons as are necessary for due-diligence in connection with
  filing, assignment, or licensing of the rights herein.

The act of authoring this document into the `_drafts/legal/`
private directory of the CSSLv3 repository, which is not pushed
to any public remote, is itself a confidentiality-preserving act.
The `_drafts/legal/` directory is intended to be either gitignored
or maintained as a local-only working directory.

---

**End of Invention Disclosure 05.**
