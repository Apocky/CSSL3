# Invention Disclosure : Gaze-Reactive Observation-Collapse — Detail-Emergence Conditioned on Eye-Tracking with World-State Superposition Collapse, Compile-Time-Enforced Biometric Information-Flow Control, and On-Device-Only Gaze Data Treatment

**PRIVATE — NOT FOR PUBLIC DISCLOSURE — PRE-FILING DRAFT**

---

## 1. Inventor Information

- **Inventor** : Apocky <apocky13@gmail.com>
  *(legal-name placeholder ; insert filing-jurisdiction-correct legal name at attorney handoff)*
- **Date of conception** : earliest CSSLv3 spec author-date for
  `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.4`
  (Gaze-Reactive Observation-Collapse novelty path) and
  `Omniverse/01_AXIOMS/05_OBSERVATION_COLLAPSE.csl.md`
  (Axiom 5). Verify against `git log --diff-filter=A` on those
  spec files.
- **Date of reduction-to-practice** : Stage-2 implementation
  under `compiler-rs/crates/cssl-gaze-collapse/`, slice T11-D120,
  with the biometric information-flow-control surface in
  `compiler-rs/crates/cssl-ifc/` (slices D129 / D132).
- **Branch of record** : `cssl/session-6/parallel-fanout`.

---

## 2. Title of Invention

**Method, System, and Computer-Readable-Medium for Gaze-Reactive
World-State Detail Emergence in Real-Time Computer Graphics,
Comprising an Eye-Tracking-Driven Observation-Collapse Evolver
that Conditionally Synthesizes Detail in a Region of the Rendered
World When and Only When the Region Transitions from Peripheral
to Foveal in the Viewer's Gaze, Coupled with a Compile-Time-
Enforced Biometric Information-Flow-Control Subsystem that
Prevents Gaze Data from Egressing the Local Device by any Code
Path, Including a Saccade-Predictor that Pre-Collapses the
Saccade-Target Region with Latency-Hiding under Saccadic
Suppression**

Short title : **Gaze-Reactive Observation-Collapse (Stage-2)**

---

## 3. Technical Field

This invention falls within the field of real-time computer
graphics, specifically :

1. **Foveated rendering** : the allocation of rendering work
   non-uniformly across the screen as a function of the user's
   gaze, allocating more work to the foveal region (the small
   high-acuity region of the human visual system, approximately
   2 degrees of visual angle) and less to the periphery.
2. **Eye-tracking-driven content adaptation** : the use of
   eye-tracking data to adapt the rendered content (not just
   the rendering quality) based on where the user is looking.
3. **Variable-rate shading and dynamic-render-quality** :
   GPU-hardware techniques for varying the shading rate per tile
   based on per-tile importance.
4. **Privacy and information-flow-control for biometric data** :
   the regulation of how biometric data flows through software
   systems, especially under data-protection regulations such as
   the General Data Protection Regulation, biometric-privacy
   acts, and equivalent jurisdictional regimes.
5. **Mixed-reality and virtual-reality applications** : per-eye
   stereoscopic rendering at consumer head-mounted-display
   refresh rates with eye-tracking sensors integrated.

The invention is targeted at consumer-grade GPU hardware in the
M7-class performance envelope, on head-mounted-display devices
equipped with eye-tracking sensors, within the Stage-2 sub-budget
of 0.3 ms at Quest-3 reference hardware and 0.25 ms at Vision-Pro
reference hardware, contained within the per-frame budget of
11.1 ms at 90 Hz or 8.3 ms at 120 Hz.

---

## 4. Background / Prior Art

### 4.1 The Status Quo : Foveated Rendering as Performance Trick

Foveated rendering, as deployed in production today (Quest 3,
Vision Pro, PlayStation VR2, Pico 4), is a performance optimization.
The basic premise is that the human visual system has high acuity
only in a small foveal region (~2 degrees of visual angle), so
the rendering quality outside that region can be reduced without
the user perceiving the reduction. Implementations include :

- **Fixed-foveation (no eye-tracking)** : the screen is divided
  into concentric zones with progressively-lower shading rate.
  This is the approach in low-end devices without eye-tracking.
- **Eye-tracked foveation (with sensors)** : the eye-tracking
  sensor reports the user's gaze direction per frame, and the
  high-shading-rate region tracks the gaze. This is the approach
  in Vision Pro, Quest Pro, PSVR2, and similar.
- **Variable-rate shading** : a GPU-hardware feature that allows
  each tile of the screen to be shaded at 1x, 2x, or 4x the
  resolution of its base sample. This is the substrate that
  foveated rendering targets.
- **Foveated dynamic-render-quality** : a per-tile shading-rate
  selection driven by foveation parameters.

In all of these approaches, the rendered content is identical
regardless of where the user is looking ; only the rendering
quality varies. The same world is rendered ; only the pixel
budget per region changes.

### 4.2 The Specific Deficiencies Addressed by This Invention

**Deficiency 1 — gaze does not affect content.** Production
foveated rendering does not use gaze information to modify what
is rendered, only how. There is no production technique that
conditions detail-emergence (the synthesis of new world content)
on whether or where the user is looking.

**Deficiency 2 — observation-collapse is unmodeled.** A novel
artistic and physical premise — that unobserved regions of the
world exist in a kind of superposition of possibilities, and
that the act of looking literally collapses the world to a
specific cosmology-consistent state — is not modeled in any
production renderer. This is conceptually inspired by the
quantum-mechanical observer-effect and is operationally relevant
to procedural-content systems where the procedural seed itself
can be fixed lazily (only when looked at).

**Deficiency 3 — biometric data egress is unconstrained.**
Production eye-tracking systems do not enforce, at compile-time,
that gaze data cannot leave the local device. Eye-tracking-data
treatment is governed by privacy policies, runtime checks, and
the developer's discretion ; there is no compile-time guarantee
that no code path can transmit gaze data over the network or
write it to non-volatile storage.

**Deficiency 4 — no saccade-prediction with latency-hiding.**
Production foveated rendering reactively responds to current
gaze position. By the time a saccade (a rapid eye movement) is
complete, the rendering pipeline has lagged behind by 5-15 ms.
There is no production technique that predicts the saccade
target ahead of time and pre-collapses the target region under
the cover of saccadic suppression (the brief 50-100 ms window
during which the human visual system is functionally blind during
a saccade).

**Deficiency 5 — no per-cell sovereignty respect.** Production
foveated rendering does not respect per-cell sovereignty masks
in a substrate-level world model. The detail-emergence procedure,
when conditioned on gaze, must respect each cell's per-cell
consent mask : a private cell that the gaze has crossed must not
have its detail emerged into the rendered output.

### 4.3 The Closest Prior Art

**Foveated Rendering (Patney et al., 2016 ; Guenter et al., 2012)**
: the foundational published technique. Quality-only ; not content-
varying.

**Variable-Rate Shading (DirectX 12, Vulkan VK_NV_shading_rate_image)**
: GPU-hardware feature ; quality-only.

**Apple Visual Inertial Odometry + Eye-Tracking** : Vision Pro's
gaze-driven UI focus. The UI focus changes based on where the user
looks, but the rendered world content does not.

**Procedural Content Generation (Smith et al.)** : pre-rendered
or runtime-generated content keyed by procedural seeds, but not
keyed by per-frame gaze.

**Data-Flow-Confidentiality work (Liskov et al. ; Chong et al.)** :
information-flow-control as a programming-language concept, not
applied to biometric data in a real-time graphics pipeline.

The novel combination of gaze-driven content adaptation,
observation-collapse evolver, compile-time biometric IFC,
saccade-prediction with latency-hiding, and per-cell sovereignty
respect, all within a real-time per-frame budget on consumer GPU
hardware, has not appeared in any prior art known to the inventor
as of the date of this disclosure.

---

## 5. Summary of the Invention

The invention is a real-time gaze-reactive content-adaptation
render pass positioned as Stage-2 of a 12-stage canonical render
pipeline. It consumes per-eye gaze direction, per-eye openness,
the body-presence field (other users' bodies in the shared
environment), and a summary of the world-state from the previous
frame ; it produces per-eye fovea masks, a KAN-detail-budget
allocation table, and a collapse-bias vector that downstream
stages use to drive detail-emergence.

The novel features are :

1. **Observation-collapse evolver.** When a region transitions
   from peripheral to foveal under the gaze (as detected by the
   per-frame gaze direction crossing into a region's screen-
   space projection), the observation-collapse evolver evaluates
   a Kolmogorov-Arnold-Network conditioned on the recent-glance-
   history of the user. The KAN output is a "collapsed cosmology"
   for the region — a fixed seed and a set of detail parameters
   that lock the region's content for as long as the gaze remains
   on it (or for a configurable timeout after the gaze leaves).
   The act of looking literally CHANGES what is rendered, in a
   way that distinguishes the present invention from foveated-
   rendering-as-quality-trick.

2. **Compile-time biometric information-flow control.** All
   gaze-bearing values are wrapped in a typed `Sensitive<gaze>`
   IFC label. The compile-time gate `validate_egress` refuses
   to permit any such value to flow to a network-egress sink, a
   persistent-storage sink, or a cross-session-storage sink, by
   any code path, including by way of any privilege-elevation
   mechanism (no `Privilege<*>` capability, including the
   highest-privilege `ApockyRoot`, can override the biometric
   refusal). Cross-session gaze-storage cannot be enabled by any
   flag, configuration, environment variable, command-line
   argument, runtime condition, or remote API call.

3. **Saccade-predictor with latency-hiding.** A hybrid
   extended-Kalman-filter plus convolutional-LSTM predicts the
   gaze target 3-5 ms ahead of the current gaze position. The
   pre-collapse of the predicted target region happens under the
   cover of saccadic suppression (the 50-100 ms window during
   which the human visual system is functionally blind during a
   saccade). The user perceives the target region as already-
   collapsed by the time the saccade completes, with no
   visible latency.

4. **Per-cell sovereignty mask respect.** The observation-
   collapse evolver checks each cell's per-cell consent mask
   (the `SigmaMaskPacked` interface from
   `cssl-substrate-prime-directive`) before writing into the
   ω-field. Cells whose consent mask permits writing are
   collapsed ; cells whose consent mask does not permit writing
   are skipped, and the rendered output for those cells reflects
   their pre-collapse state (a uniform default or a
   region-specific opaque color).

5. **Opt-in default with center-bias-foveation fallback.** The
   gaze-reactive content-adaptation feature is opt-in. When opt-
   out, a center-bias-foveation fallback is used (no eye-tracking
   data flows ; the foveation index is computed from a
   center-bias model). This is the default behavior.

6. **Saccadic-suppression hides flicker.** Because the user is
   functionally blind during the 50-100 ms saccade window, any
   flicker or transient inconsistency between the pre-collapse
   prediction and the post-collapse confirmation is hidden from
   perception.

7. **State-purge at session-end.** The thread-local saccade-
   history state is purged on `Drop` ; no state survives the
   session boundary. Combined with the compile-time biometric IFC
   (no network or persistent egress), this produces a "no
   surveillance possible by construction" property.

The novel combination of these elements, operating in a single
render pass within a 0.3 ms / 0.25 ms Stage-2 budget on consumer
GPU hardware, is the inventive step.

---

## 6. Detailed Description

### 6.1 System Overview

The Gaze-Reactive Observation-Collapse pass is Stage-2 of the
12-stage canonical render pipeline. It receives :

- per-eye gaze direction `(gaze_l, gaze_r)` from the eye-tracking
  sensor, wrapped in `Sensitive<gaze>` IFC labels at the source,
- per-eye openness `(open_l, open_r)` indicating blink state,
- the body-presence field representing other users' bodies in the
  shared mixed-reality environment,
- a summary of the world-state from the previous frame
  (`Ω.prev.MERA-summary` per the spec),
- the per-cell consent mask `SigmaMaskPacked` from the substrate
  runtime.

It produces :

- a per-eye fovea mask `(fovea_l, fovea_r)` indicating the per-
  pixel detail-budget allocation,
- a KAN-detail-budget per-tile allocation table that downstream
  stages use to drive detail-emergence,
- a collapse-bias vector that downstream stages use to seed
  cell-level procedural detail.

### 6.2 Gaze Input Wrapping

Gaze data enters the system through a single typed entry-point :

```rust
pub fn from_xr_eye_gaze(
    gaze: Sensitive<EyeGaze>,
) -> GazeInput { ... }
```

The `Sensitive<EyeGaze>` wrapper carries the IFC label
`SensitiveDomain::Gaze` from the source-of-truth. The label
travels with the value through every transformation, so any
attempt to write the value (or a derivative) to a network or
persistent-storage sink is intercepted at compile-time by
`validate_egress`.

The `validate_egress` gate is implemented in `cssl-ifc` as a
trait that, at compile-time, refuses any flow from a
`Sensitive<Gaze>`-labeled value to a sink that is not labeled
`OnDeviceVolatile`. Sinks that are network, persistent-storage,
or cross-session are refused by construction. The refusal is a
compile-time error with a clear diagnostic.

### 6.3 Saccade Predictor

The saccade predictor is a hybrid model :

- An extended-Kalman-filter (EKF) tracks the gaze trajectory in
  state-space, modeling angular position, angular velocity, and
  angular acceleration.
- A convolutional-LSTM (Conv-LSTM) ingests the recent gaze-
  trajectory window (last 100 ms) and predicts the next saccade
  target.

The predictor produces :

- the predicted saccade target `(target_l, target_r)` in
  screen-space normalized coordinates,
- a confidence value indicating prediction quality.

The Conv-LSTM is small (a 4-layer network with hidden state
size 32) and is evaluated once per frame at the start of Stage-2.
The predictor's training is offline and the network weights are
embedded in the runtime as a constant table ; no online learning
modifies the weights at user-runtime, so no user-specific gaze
patterns can be inadvertently captured into the runtime state.

### 6.4 Observation-Collapse Evolver

When a region transitions from peripheral to foveal under the
gaze, the observation-collapse evolver fires. The evolver inputs
are :

- the region's current pre-collapse state (a base spectral
  radiance and a set of unfilled detail axes),
- the recent-glance-history of the user (a ring buffer of the
  last N gaze directions, purged at session-end),
- the per-cell consent masks for the cells composing the region,
- the per-region policy (Public, Private, ConsentRequired).

The evolver produces :

- a fixed seed for the region's procedural detail,
- a set of resolved detail parameters (e.g., procedural-noise
  octave count, color-palette lock, micro-geometry density) that
  override the pre-collapse state.

The evolver is implemented as a Kolmogorov-Arnold-Network with a
4-input / 8-hidden / 4-output topology. The output is the
"collapsed cosmology" of the region — a fixed, replayable, and
deterministic instantiation of one of the previously-superposed
possibilities. The KAN spline coefficients are quantized and
embedded in a per-pass uniform buffer.

Once collapsed, the region remains collapsed for the duration
of the gaze fixation plus a configurable timeout (e.g., 5 seconds
after the gaze leaves). After timeout, the region returns to
the pre-collapse superposition state, ready to collapse to a
potentially different cosmology on the next observation.

### 6.5 Per-Cell Sovereignty Mask Respect

For each cell whose detail is being collapsed, the evolver checks
the cell's `SigmaMaskPacked` per-cell consent flags. The flags
are :

- `READ_ALLOWED` : whether reading the cell's pre-collapse state
  is permitted,
- `WRITE_ALLOWED` : whether writing the cell's collapsed state
  is permitted,
- `RENDER_ALLOWED` : whether the cell's content can appear in
  the rendered output.

When `WRITE_ALLOWED` is false for a cell, the evolver skips that
cell ; the cell remains in its pre-collapse state. When
`RENDER_ALLOWED` is false for a cell, downstream stages mask off
the cell from the rendered output (replaced by a uniform default
or a region-specific opaque color).

This is the load-bearing per-cell sovereignty mechanism that
ensures gaze-driven detail-emergence does not violate per-cell
consent.

### 6.6 Saccadic Suppression and Latency Hiding

The human visual system is functionally blind during the 50-100 ms
window of a saccade (the rapid eye movement between fixations).
This is called saccadic suppression and is a well-documented
neuroscientific phenomenon. The present invention exploits this
window to perform the pre-collapse of the predicted target
region without the user perceiving the latency.

The latency-hiding pipeline is :

1. At the start of frame `f`, the saccade predictor reports the
   predicted target for frame `f + N` (N = 1-3 frames ahead, or
   approximately 11-33 ms ahead at 90 Hz).
2. The observation-collapse evolver pre-collapses the predicted
   target region during frames `f` through `f + N - 1`. This work
   happens in parallel with the existing rendering pipeline ; it
   does not block frame `f`.
3. By frame `f + N`, when the saccade target is reached, the
   pre-collapsed region is already in its collapsed state and
   ready to render at full detail.
4. If the user's actual gaze deviates from the predicted target
   (a prediction error), the deviation is corrected reactively
   in the next frame, with the correction hidden under saccadic
   suppression because the user's eyes are still in transit
   during the correction window.

### 6.7 Opt-In Default and Center-Bias Fallback

The gaze-reactive content-adaptation feature is opt-in. The user
explicitly grants consent through a system-level flag at the
start of the session. If consent is not granted, the system
falls back to center-bias foveation : the foveation index is
computed from a center-bias model that does not consume eye-
tracking data ; downstream stages use the center-bias foveation
index in lieu of the eye-tracked foveation index.

The fallback is the default. The opt-in is an explicit user
action, and the consent flag is itself wrapped in
`Sensitive<consent>` IFC labeling so that the consent itself
cannot egress the device.

### 6.8 Cost Model and Budget Gate

The Stage-2 budget is :

- 0.3 ms at Quest-3 reference hardware,
- 0.25 ms at Vision-Pro reference hardware.

The cost decomposes as :

- Saccade-predictor evaluation : ~0.05 ms (Conv-LSTM is small),
- Observation-collapse evolver KAN evaluation : ~0.10 ms per
  region, with up to 3 regions evaluated per frame in the typical
  case (1 current foveal region, 1 predicted saccade target, 1
  recently-vacated region during timeout),
- Foveation mask computation : ~0.05 ms,
- IFC label propagation overhead : ~0.01 ms (single-instruction-
  level overhead per labeled value, amortized to negligible at
  the per-frame level).

Total : ~0.21 ms at Quest-3 reference hardware, well within
the 0.3 ms budget.

### 6.9 Reference Implementation in CSSLv3

The reference implementation is in :

- `compiler-rs/crates/cssl-gaze-collapse/src/lib.rs` — Stage-2
  module-level documentation and re-exports.
- `compiler-rs/crates/cssl-gaze-collapse/src/gaze_input.rs` —
  `GazeInput` per-eye gaze-direction + confidence + `SaccadeState`,
  constructed only via the `Sensitive<gaze>` typed entry-point.
- `compiler-rs/crates/cssl-gaze-collapse/src/fovea_mask.rs` —
  `FoveaMask` 2D screen-space density-mask (full-detail center,
  coarse periphery).
- `compiler-rs/crates/cssl-gaze-collapse/src/saccade_predictor.rs`
  — `SaccadePredictor` extended-Kalman-filter + Conv-LSTM hybrid,
  `Drop` impl zeroing thread-local state at session-end.
- `compiler-rs/crates/cssl-gaze-collapse/src/observation_collapse.rs`
  — `ObservationCollapseEvolver` KAN-conditioned-on-glance-history
  evolver.
- `compiler-rs/crates/cssl-gaze-collapse/src/pass.rs` —
  `GazeCollapsePass` render-graph node.
- `compiler-rs/crates/cssl-gaze-collapse/src/config.rs` —
  `GazeCollapseConfig` opt-in flag, fallback-to-center-bias-
  foveation, prediction-horizon, KAN-budget bands.
- `compiler-rs/crates/cssl-gaze-collapse/src/error.rs` —
  `GazeCollapseError` error surface, `EgressRefused` variant.
- `compiler-rs/crates/cssl-gaze-collapse/src/attestation.rs` —
  verbatim §11 CREATOR-ATTESTATION block + §1
  ANTI-SURVEILLANCE supplement.
- `compiler-rs/crates/cssl-ifc/src/...` — biometric IFC enforcement
  surface (`SensitiveDomain::Gaze`, `LabeledValue<T>`, `validate_egress`,
  `EgressGrantError::BiometricRefused`).
- `compiler-rs/crates/cssl-substrate-prime-directive/src/sigma.rs`
  — `SigmaMaskPacked` per-cell consent-mask interface.

### 6.10 Algorithmic Pseudocode

```
function GazeCollapsePass::execute(frame_inputs) :
    // Wrap gaze data in IFC labels at the source.
    let gaze_l = Sensitive::<Gaze>::from_xr(frame_inputs.eye_gaze_l)
    let gaze_r = Sensitive::<Gaze>::from_xr(frame_inputs.eye_gaze_r)

    // If consent is not granted, fall back to center-bias foveation.
    if not consent_granted :
        return CenterBiasFoveation::compute(frame_inputs)

    // Saccade prediction (3-5 ms ahead).
    let predicted_target = SaccadePredictor::predict(
        gaze_l, gaze_r, recent_glance_history)

    // Foveation mask computation.
    let fovea_mask = FoveaMask::compute(gaze_l, gaze_r, predicted_target)

    // Observation-collapse for regions newly entering foveal range.
    for region in foveal_transitioned_regions(fovea_mask) :
        if region.policy == Private :
            continue                       // skip private regions
        for cell in region.cells :
            let consent = SigmaMaskPacked::lookup(cell.id)
            if not consent.WRITE_ALLOWED :
                continue                   // skip non-consenting cells
            let collapsed = ObservationCollapseEvolver::collapse(
                cell, recent_glance_history)
            ω_field.write(cell.id, collapsed)

    // Pre-collapse for predicted saccade target (latency hiding).
    for region in regions_at(predicted_target) :
        for cell in region.cells :
            let consent = SigmaMaskPacked::lookup(cell.id)
            if not consent.WRITE_ALLOWED :
                continue
            let pre_collapsed = ObservationCollapseEvolver::collapse(
                cell, recent_glance_history)
            ω_field.write(cell.id, pre_collapsed)

    // Outputs.
    return (fovea_mask, kan_detail_budget(fovea_mask),
            collapse_bias_vector(recent_glance_history))
```

---

## 7. Drawings Needed

The patent attorney should commission the following figures :

- **Figure 1** : System architecture diagram showing Stage-2
  positioned in the 12-stage canonical render pipeline, with
  inputs (per-eye gaze direction, per-eye openness, body-
  presence field, prev-frame world-state summary) and outputs
  (per-eye fovea mask, KAN detail budget, collapse-bias vector).

- **Figure 2** : Gaze-data IFC-label flow diagram showing the
  `Sensitive<Gaze>` wrapper at the eye-tracking source, the
  label propagating through transformations, and the
  `validate_egress` compile-time gate refusing flows to network,
  persistent-storage, or cross-session sinks.

- **Figure 3** : Saccade-predictor topology diagram showing the
  EKF state (angular position, velocity, acceleration), the
  Conv-LSTM input window (last 100 ms of gaze trajectory), and
  the prediction output (target screen-space normalized
  coordinate plus confidence).

- **Figure 4** : Observation-collapse evolver topology diagram
  showing the KAN with 4 input edges (region pre-state, glance-
  history summary, consent-mask summary, region-policy), 8-node
  hidden layer, 4-output (collapsed seed plus 3 detail
  parameters).

- **Figure 5** : Saccadic-suppression timeline diagram showing
  the saccade window (50-100 ms), the pre-collapse work
  performed during the suppression window, and the resulting
  zero-perceived-latency from the user's perspective.

- **Figure 6** : Per-cell sovereignty mask respect diagram
  showing a region of cells, some with `WRITE_ALLOWED = true`
  (collapsed) and some with `WRITE_ALLOWED = false` (skipped,
  remaining in pre-collapse state).

- **Figure 7** : Opt-in default and center-bias fallback diagram
  showing the consent-granted path versus the consent-not-
  granted path, with the center-bias-foveation computation in
  the no-consent branch and the gaze-driven computation in the
  consent-granted branch.

- **Figure 8** : Comparative output renderings showing the same
  scene rendered (a) with no foveation (uniform shading), (b)
  with center-bias foveation (production baseline), (c) with
  eye-tracked foveation (production baseline with eye tracking),
  and (d) with the present invention (gaze-driven content
  adaptation, with detail emerging where the user is looking).

- **Figure 9** : State-purge at session-end diagram showing the
  `Drop` impl on `SaccadePredictor` zeroing the thread-local
  saccade-history state, and the cumulative "no surveillance
  possible by construction" property arising from the combination
  of compile-time IFC and runtime-state purge.

- **Figure 10** : Cost-budget breakdown bar chart showing the
  per-frame Stage-2 cost components (saccade predictor, KAN
  evolver, foveation mask, IFC overhead) summed against the
  0.3 ms / 0.25 ms platform budgets.

---

## 8. Claims (Initial Draft for Attorney)

### Claim 1 (Independent — Method)

A computer-implemented method for gaze-reactive content
adaptation in real-time computer graphics, comprising :
- receiving, at a head-mounted-display device equipped with an
  eye-tracking sensor, per-eye gaze-direction data wrapped in
  a typed information-flow-control label indicating that the
  data is biometrically sensitive ;
- computing, from the gaze-direction data, a per-eye foveation
  mask indicating the per-pixel detail-budget allocation across
  the screen ;
- detecting, from changes in the foveation mask between
  successive frames, regions of the rendered world that have
  transitioned from peripheral to foveal in the user's gaze ;
- evaluating, for each newly-foveal region, an
  observation-collapse function that conditionally synthesizes
  detail in the region by evaluating a Kolmogorov-Arnold-Network
  conditioned on the user's recent-glance-history, producing a
  fixed seed and a set of resolved detail parameters that lock
  the region's content for the duration of the gaze fixation ;
- writing the resolved detail parameters into a world-state
  field, gated by a per-cell consent mask such that cells whose
  consent mask does not permit writing are skipped ;
- enforcing, at compile-time of the surrounding software system,
  that no biometrically-sensitive value can flow to a network
  egress sink, a persistent-storage sink, or a cross-session-
  storage sink, by any code path, including by way of any
  privilege-elevation mechanism.

### Claim 2 (Dependent — Saccade Predictor)

The method of Claim 1 further comprising a saccade-predictor
implemented as a hybrid extended-Kalman-filter plus
convolutional-long-short-term-memory network that predicts a
saccade target for a future frame, and pre-collapsing the
predicted target region during a current frame so that the user
perceives the target region as already-collapsed by the time the
saccade completes.

### Claim 3 (Dependent — Saccadic-Suppression Latency Hiding)

The method of Claim 2 wherein the pre-collapse work occurs
during the human visual system's saccadic-suppression window of
50-100 milliseconds, so that any prediction-correction latency
is hidden from the user's perception.

### Claim 4 (Dependent — Per-Cell Consent)

The method of Claim 1 wherein the per-cell consent mask is
implemented as a packed bit-vector of consent flags per cell,
including at minimum a read-allowed flag, a write-allowed flag,
and a render-allowed flag, and wherein the observation-collapse
function respects each flag at the per-cell level.

### Claim 5 (Dependent — Compile-Time IFC)

The method of Claim 1 wherein the compile-time enforcement of
the no-egress property is implemented as a typed information-
flow-control system in which biometrically-sensitive values are
labeled at the source of the data (the eye-tracking sensor
driver entry-point) and the labels propagate through all
transformations, with the egress-sink type-checks performed by
the compiler at compile-time, producing a compile-time error
diagnostic for any violating code path.

### Claim 6 (Dependent — No-Override Privilege)

The method of Claim 5 wherein the compile-time enforcement is
not overridable by any privilege-elevation mechanism, including
the highest-privilege capability defined by the surrounding
software system, such that the compile-time error cannot be
bypassed by any privilege-claim made at runtime or at
compile-time.

### Claim 7 (Dependent — No Configuration Override)

The method of Claim 5 wherein the compile-time enforcement is
not overridable by any flag, configuration setting, environment
variable, command-line argument, runtime condition, or remote
application-programming-interface call.

### Claim 8 (Dependent — Opt-In Default)

The method of Claim 1 wherein the gaze-driven content-adaptation
is opt-in by default, and wherein when the user has not granted
consent, the foveation mask is computed from a center-bias model
that does not consume eye-tracking data.

### Claim 9 (Dependent — State Purge at Session End)

The method of Claim 1 further comprising purging, at the end of
each user session, all thread-local saccade-history state by
zeroing the corresponding memory upon a destructor call, such
that no per-user gaze-pattern state survives the session
boundary.

### Claim 10 (Dependent — Recent-Glance-History Conditioning)

The method of Claim 1 wherein the Kolmogorov-Arnold-Network of
the observation-collapse function is conditioned on a recent-
glance-history ring buffer comprising the last N gaze directions,
where N is selected such that the buffer captures the user's
recent attention pattern over a window of approximately 0.5 to
2.0 seconds.

### Claim 11 (Independent — System)

A real-time graphics rendering system, comprising :
- a head-mounted-display device equipped with an eye-tracking
  sensor and a graphics processing unit ;
- a gaze-input subsystem configured to wrap per-eye gaze-direction
  data in a typed information-flow-control label ;
- a saccade-predictor subsystem configured to predict the gaze
  target a configurable number of frames ahead ;
- an observation-collapse evolver subsystem configured to
  perform the method of Claim 1 ;
- a fovea-mask subsystem configured to produce a per-eye fovea
  mask consumed by downstream rendering stages ;
- a compile-time information-flow-control compiler configured to
  refuse any flow of biometrically-labeled values to non-on-
  device sinks at compile-time.

### Claim 12 (Dependent — Quest-3 Class)

The system of Claim 11 wherein the gaze-reactive content-
adaptation pass completes in less than 0.3 milliseconds per
frame at a head-mounted-display target frame rate of at least
90 Hertz.

### Claim 13 (Dependent — Vision-Pro Class)

The system of Claim 11 wherein the gaze-reactive content-
adaptation pass completes in less than 0.25 milliseconds per
frame at a head-mounted-display target frame rate of at least
90 Hertz.

### Claim 14 (Independent — Computer-Readable Medium)

A non-transitory computer-readable medium storing instructions
that, when compiled and executed on a head-mounted-display
device equipped with an eye-tracking sensor and a graphics
processing unit, cause the device to perform the method of
Claim 1.

### Claim 15 (Dependent — Sovereignty Compliance)

The medium of Claim 14 wherein the instructions are configured
such that the gaze-reactive content-adaptation pass is replayable
and deterministic given identical inputs (where the gaze input
is the canonical biometrically-labeled input, replayable only
on-device), does not mutate non-consenting per-cell state, and
does not consume any biometric data beyond the gaze-direction
data already wrapped in the typed information-flow-control label.

---

## 9. Embodiments

### Embodiment A : Quest-3 Class Mobile XR

The system runs on a Snapdragon-class mobile GPU with the
embedded eye-tracking sensors of the Quest 3 / Quest 3S. The
opt-in default is enabled. The center-bias foveation fallback is
the no-consent branch. Per-frame budget 11.1 ms (90 Hz) ;
Stage-2 budget 0.3 ms.

### Embodiment B : Vision-Pro Class Mixed Reality

The system runs on Apple M-series GPU with metal3 and the
high-fidelity eye-tracking sensors of Vision Pro. The opt-in
default is enabled. The saccade-predictor uses a longer
prediction horizon (5-7 ms) due to higher available compute
budget. Per-frame budget 8.3 ms (120 Hz) ; Stage-2 budget
0.25 ms.

### Embodiment C : Desktop with External Eye-Tracking

The system runs on a desktop discrete GPU with an external
eye-tracking peripheral (Tobii, Pupil Labs, or similar). The
gaze-input wrapping must additionally enforce an integrity check
that the eye-tracking peripheral is the legitimate source-of-
truth (a hardware attestation step). The IFC label is
preserved through the peripheral driver.

### Embodiment D : Mobile Without Eye-Tracking

The system runs on a mobile device without eye-tracking sensors
(e.g., a phone-based VR with no eye-tracking). The center-bias
foveation fallback is the only available path ; the gaze-
reactive content-adaptation is unavailable.

### Embodiment E : Saccade-Predictor-Disabled Variant

The system disables the saccade-predictor (e.g., for a
debugging or low-power configuration). The observation-collapse
evolver still operates reactively but without latency-hiding ;
the user may perceive a slight lag of 5-15 ms between the
saccade and the detail-emergence at the new target.

### Embodiment F : Multi-User Mixed-Reality Variant

The system is deployed in a multi-user mixed-reality
environment where multiple users' gazes must be tracked
independently. Each user's gaze is wrapped in its own
`Sensitive<Gaze>`-labeled IFC value, and each user's
observation-collapse is performed independently. Per-cell
consent masks may differ between users (a cell may be
WRITE_ALLOWED for User A but not for User B).

### Embodiment G : Procedural-Game-World Variant

The system is deployed in a procedural game world where
unobserved regions are intentionally unrendered. The
observation-collapse evolver fires each time the user looks at
a previously-unseen region, lazily generating the region's
procedural content. The artistic intent is "the world only
exists where you look."

### Embodiment H : Architectural-Visualization Variant

The system is deployed in an architectural-visualization tool
where the foveation mask drives a higher-quality lighting
solver (e.g., higher-bounce path tracing) in the foveal region.
The observation-collapse evolver is disabled in this embodiment
(the architectural model is deterministic, not procedural).

---

## 10. Industrial Applicability

This invention is industrially applicable in :

- Head-mounted-display devices equipped with eye-tracking
  sensors (Vision Pro, Quest 3, Quest Pro, PSVR2, Pico 4 Pro,
  and successor devices).
- Real-time computer-generated imagery for video games, where
  procedural content is artistically driven by the user's
  attention.
- Mixed-reality applications where bystander privacy and the
  user's own biometric privacy are regulatory and ethical
  requirements.
- Architectural and design-visualization applications where the
  foveation mask drives higher-quality rendering in the foveal
  region.
- Medical and clinical applications where eye-tracking is used
  to drive content adaptation (e.g., vision-therapy, ophthalmic
  diagnostic tools) and where patient biometric privacy is a
  regulatory requirement.
- Telepresence applications where multi-user mixed-reality
  environments must respect each user's biometric privacy.

The licensing market includes head-mounted-display original
equipment manufacturers, real-time-rendering middleware vendors,
content-authoring-tool vendors, game studios, medical-device
vendors, and any mixed-reality application developer subject to
biometric-privacy regulations.

---

## 11. Reference Implementation in CSSLv3

The reference implementation is in :

- `compiler-rs/crates/cssl-gaze-collapse/src/lib.rs` — Stage-2
  module documentation and re-exports.
- `compiler-rs/crates/cssl-gaze-collapse/src/gaze_input.rs` —
  `GazeInput`, `Sensitive<gaze>` typed entry-point.
- `compiler-rs/crates/cssl-gaze-collapse/src/fovea_mask.rs` —
  `FoveaMask` 2D screen-space density mask.
- `compiler-rs/crates/cssl-gaze-collapse/src/saccade_predictor.rs`
  — `SaccadePredictor` EKF + Conv-LSTM hybrid, `Drop` impl.
- `compiler-rs/crates/cssl-gaze-collapse/src/observation_collapse.rs`
  — `ObservationCollapseEvolver`.
- `compiler-rs/crates/cssl-gaze-collapse/src/pass.rs` —
  `GazeCollapsePass` render-graph node.
- `compiler-rs/crates/cssl-gaze-collapse/src/config.rs` —
  `GazeCollapseConfig`.
- `compiler-rs/crates/cssl-gaze-collapse/src/error.rs` —
  `GazeCollapseError`, `EgressRefused`.
- `compiler-rs/crates/cssl-gaze-collapse/src/attestation.rs` —
  §11 CREATOR-ATTESTATION + §1 ANTI-SURVEILLANCE supplement.
- `compiler-rs/crates/cssl-ifc/src/...` — `SensitiveDomain::Gaze`,
  `LabeledValue<T>`, `validate_egress`,
  `EgressGrantError::BiometricRefused`.
- `compiler-rs/crates/cssl-substrate-prime-directive/src/sigma.rs`
  — `SigmaMaskPacked` per-cell consent-mask interface.

The verbatim spec anchors are :

- `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.4` —
  the path-V.4 specification (gaze-reactive observation-collapse
  novelty path).
- `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § STAGE 2` —
  Stage-2 pipeline-position, budget, effect-row.
- `Omniverse/07_AESTHETIC/05_VR_RENDERING.csl` — eye-tracking
  integration surface, saccade-prediction latency budget,
  saccadic-suppression treatment.
- `Omniverse/01_AXIOMS/05_OBSERVATION_COLLAPSE.csl.md` — Axiom 5
  (unobserved-region-in-superposition, observation-collapses-to-
  cosmology-consistent-state).

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

Additionally, this invention disclosure references information
that is itself biometrically-related (eye-tracking, gaze-pattern,
saccadic-suppression). The disclosure document itself is the kind
of document that, if shared widely, could provide adversarial
parties with insight into the implementation details of the
biometric-privacy enforcement, which could undermine the
defensive value of the system. Accordingly, this disclosure
document is treated with elevated confidentiality.

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

**End of Invention Disclosure 06.**
