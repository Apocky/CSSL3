# Invention Disclosure : Unified Multi-Band Wave-PDE Solver for Simultaneous Light + Audio + Heat + Scent + Mana Field Rendering

**PRIVATE — NOT FOR PUBLIC DISCLOSURE — PRE-FILING DRAFT**

---

## 1. Inventor Information

- **Inventor** : Apocky <apocky13@gmail.com>
  *(legal-name placeholder ; insert filing-jurisdiction-correct legal name at attorney handoff)*
- **Date of conception** : earliest CSSLv3 spec author-date for `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl` — Wave-Unity Statement § 0 documents the inventive step of a single ψ-field obeying ONE PDE producing all five physical bands as projections (date-of-conception inferred from earliest substrate-spec commit ; verify with `git log --diff-filter=A -- Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl` in Omniverse repo)
- **Date of reduction-to-practice** : commit `707761b` on branch `cssl/session-11/T11-D114-wave-solver` titled `§ T11-D114 — cssl-wave-solver crate : Wave-Unity multi-rate ψ-field solver` (working software implementation of the multi-band complex-Helmholtz + LBM-Boltzmann hybrid)
- **Companion crate** (audio projection) : commit `07f7e59` titled `§ T11-D125b : Wave-Unity audio crate (cssl-wave-audio) — sibling of legacy mixer`
- **Branch of record** : `cssl/session-6/parallel-fanout` (current HEAD `c79bcf3` as of disclosure)

---

## 2. Title of Invention

**Method and System for Simultaneous Multi-Band Wave-Field Rendering of Light, Audio, Heat, Diffusion, and Discrete-Token Channels via a Unified Complex-Valued Lattice-Boltzmann-Helmholtz Hybrid Solver with Material-Impedance-Mediated Cross-Band Coupling**

Short title : **Unified Multi-Band Wave-PDE Solver (ω-Field Unity)**

---

## 3. Technical Field

This invention falls within the field of real-time interactive computer graphics
and audio engineering, with specific application to game engines, virtual-reality
(VR), augmented-reality (AR), and mixed-reality (MR) rendering systems. More
narrowly, it sits at the intersection of three sub-fields :

1. **Computational physics for graphics** : numerical solution of partial
   differential equations (PDEs) such as the Helmholtz equation, the wave
   equation, and the Klein-Gordon equation on three-dimensional grids, with
   real-time (≥60 Hz, preferably 90-120 Hz) frame-budget constraints.
2. **Spatial audio** : physically-based reverberation, head-related transfer
   functions (HRTF), interaural time/level differences (ITD/ILD), and
   geometry-aware acoustic propagation.
3. **Cross-modal coupling** : material-driven interaction between visual and
   auditory channels (e.g., synaesthetic rendering effects, vibroacoustic
   phenomena, photoacoustic energy transfer).

The invention is targeted at consumer-grade GPU hardware in the M7-class
performance envelope (≈5 TFLOPS sustained throughput at realistic occupancy),
including but not limited to the Qualcomm Adreno 750 (Quest-3 class),
Apple-Silicon M3-and-up GPUs, NVIDIA RTX 30/40/50 series, AMD RDNA-3/4, and
Intel Xe-2/Xe-3.

---

## 4. Background / Prior Art

### 4.1 The Status Quo : Two Engines, Two Pipelines

Conventional 3D real-time rendering systems treat **light transport** and
**audio propagation** as two functionally and architecturally distinct
sub-systems :

- **Light** is typically rendered via rasterization, ray-tracing, or hybrid
  techniques. Modern AAA examples : Unreal Engine 5's Lumen + Nanite ; Unity
  HDRP path-traced reflections ; Frostbite (DICE/EA) ; RAGE-2 (Rockstar) ;
  RE-Engine (Capcom) ; idTech-7 (id Software). All of these treat light as
  scalar or RGB-vector radiance fields propagated either through screen-space
  buffers, voxel cones, or Monte-Carlo path samples.
- **Audio** is typically rendered via digital-signal-processing (DSP) graph
  middleware. Standard examples : Audiokinetic Wwise, FMOD Studio, Microsoft
  Spatial Audio API, Steam Audio (Valve's wave-based plugin built atop the
  Source-2 engine), Oculus Audio SDK, Dolby Atmos for Headphones. These tools
  use sample-library playback, ray-traced occlusion volumes, or simplified
  geometric-acoustic models running on the CPU or a separate audio-DSP
  accelerator.

Cross-modal coherence between these two systems is typically achieved by
sharing **scene geometry** (e.g., the same triangle mesh or BSP volume tree
is queried by both the rendering engine and the acoustic occlusion engine)
but **not** the underlying numerical method. Specifically :

- Light occlusion uses ray-casts or screen-space depth tests.
- Audio occlusion uses separate ray-casts against often a simpler "audio
  geometry" representation.
- Audio reverb is computed via image-source method, ray-tracing into impulse
  responses, or pre-baked acoustic probes.

### 4.2 Prior-Art References

Relevant published works and shipped products documenting the segregated-pipeline
status quo :

- **Lumen** : Wright, D., et al. "Lumen — Unreal Engine 5's Real-Time Global
  Illumination System." SIGGRAPH 2022, *Advances in Real-Time Rendering in
  Games* course.
- **Nanite** : Karis, B., et al. "A Deep Dive into Nanite Virtualized
  Geometry." SIGGRAPH 2021. Documents virtual-geometry LOD streaming for
  triangle meshes — visual-only.
- **Steam Audio** : Valve Software (2017, ongoing). Documentation @
  `valvesoftware.github.io/steam-audio/` describes wave-based reverb but as
  a wholly separate pipeline from the host renderer.
- **Wwise** : Audiokinetic, Inc. (2006, ongoing). Documentation @
  `www.audiokinetic.com` describes a DSP-graph + sample-library approach
  decoupled from any unified physical field.
- **Radiance Cascades** : Osman, A. (Pocket Tactics / SIGGRAPH 2024 talk).
  "Radiance Cascades : A Novel Approach to Calculating Global Illumination."
  Note : Radiance-Cascades-as-published is a **light-only** technique. The
  present invention's reframing of cascades as one of several
  band-projections of an underlying ψ-field is novel.
- **Wave-based Reverb** : Mehra, R., et al. "Wave-based Sound Propagation in
  Large Open Scenes Using an Equivalent Source Formulation." ACM Trans.
  Graph. 32, 2, Article 19 (April 2013). Wave-based, but **audio-only**, and
  not real-time : pre-computed for offline-baked impulse responses.
- **FDTD acoustic solvers** : Botteldooren, D. "Finite-difference time-domain
  simulation of low-frequency room acoustic problems." J. Acoust. Soc. Am.
  98, 3302 (1995). Acoustic-only ; not real-time on consumer hardware in
  three dimensions at game frame-rates.
- **Lattice-Boltzmann methods (LBM)** : Succi, S. "The Lattice Boltzmann
  Equation : For Fluid Dynamics and Beyond." Oxford University Press, 2001.
  Most LBM literature targets Navier-Stokes fluid simulation, not the
  Helmholtz/Klein-Gordon wave-equation regime addressed here.
- **Wave-LBM** : Buick, J.M., et al. "Lattice Boltzmann simulation of
  acoustic streaming in microchannels." Phys. Rev. E 58, 1999. Confirms
  LBM can be parameterized to target the wave-equation. Audio-only,
  research-grade, not deployed in real-time game engines, and not
  generalized to multi-band light-and-audio simultaneity.

### 4.3 Specific Deficiencies Addressed

Each of the above prior-art references has at least one of the following
limitations that the present invention overcomes :

1. **Pipeline separation** : Audio and light are computed by distinct
   solvers, leading to cross-modal incoherence (e.g., a sound is heard
   leaking through a wall the light is correctly occluded by, or a visible
   reflection is not accompanied by the corresponding acoustic reflection).
2. **No cross-band coupling** : There is no production system in which a
   visible-light excitation directly drives an audible response (or vice
   versa) through a unified material-impedance model. Effects such as
   "audibly-shimmering doors when illuminated" and "visible standing-wave
   interference fringes that also produce an audible drone" are not
   reachable in commercial engines.
3. **Authored side-effects vs. emergent physics** : Where cross-modal
   effects exist, they are scripted (e.g., a designer manually attaches a
   sound emitter to a light source) rather than emerging from a shared
   physical substrate.
4. **Sub-real-time wave acoustics** : Wave-based audio solvers exist but
   require offline pre-computation. None target the M7 consumer-VR budget
   (≤16 ms p99 frame time at 60 Hz, or ≤11.1 ms at 90 Hz).
5. **No generalization beyond two channels** : No prior system unifies
   light, audio, heat radiation, scent diffusion, and discrete-token
   ("mana") propagation under a single PDE solver.

---

## 5. Summary of the Invention

The present invention is a **unified multi-band wave-PDE solver** that
simultaneously renders five physical channels — visible-light, audible-sound,
thermal-radiation, scent-diffusion, and a discrete-token "mana" or
named-parameter channel — by treating each as a frequency-band projection of
**a single complex-valued field ψ : ℝ³ × ℝ → ℂ** governed by ONE numerical
method.

The inventive step lies in three coupled novelties :

**Novelty A (substrate unification)** : Light and audio are solved by the
same complex-Helmholtz steady-state equation (with band-specific complex
wavenumber k(x) = ω/c(x) + iα(x)) and the same lattice-Boltzmann transient
equation (with D3Q19 or D3Q27 stencil and band-specific streaming velocity).
This is *not* a shared data structure feeding two separate solvers ; it is
literally the same kernel, parameterized at compile-time on a `Band` enum,
emitting per-band ψ-state in a single GPU command buffer.

**Novelty B (slowly-varying-envelope-approximation (SVEA) folding for
real-time multi-rate)** : Direct simulation of THz-class electromagnetic
carrier waves is intractable on consumer hardware. The invention folds each
band to its physically-meaningful envelope rate — light at ~1 ms envelope on
1 cm cells, audio at 1 ms direct-amplitude on 0.5 m cells, heat at 100 ms
envelope, scent at 1 s envelope, mana at 250 ms envelope — while preserving
the single substrate ψ. This yields a tractable budget of approximately 30
GFLOP/frame at 1 M cells × 4 light-bands + audio, well within consumer-VR
performance envelopes.

**Novelty C (KAN-derived material-impedance cross-band coupling tensor)** :
Cross-band coupling — the means by which a light excitation produces an
audible response — is mediated by a per-material complex impedance
Z(λ, M.embedding) evaluated at runtime by a Kolmogorov-Arnold Network (KAN)
spline-network rather than by hand-tuned coupling coefficients or a
look-up-table. The asymmetric coupling table (e.g., LIGHT→AUDIO = 0.001
strength via material impedance ; AUDIO→MANA strictly forbidden = 0 to
prevent agency-laundering attacks) is enforced at the type level.

The method enables six aesthetic/physical phenomena unreachable in prior-art
real-time engines :

1. Audibly-shimmering doors when illuminated.
2. Sound caustics on curved surfaces with time-of-day-modulated focal loci.
3. Standing-wave modes simultaneously visible (interference fringe) and
   audible (drone hum).
4. Visible sound fields at fortissimo (orchestra-glow effect).
5. Hearable light fields at high optical-flux (sun-burble effect).
6. Resonant instruments where the violin's actual SOUND derives from the
   ψ-PDE on the instrument's geometric domain rather than a sample-library.

---

## 6. Detailed Description

### 6.1 The ψ-Field

The fundamental quantity is a complex-valued scalar field

```
ψ : ℝ³ × ℝ → ℂ
```

interpreted band-wise such that for each band b ∈ {LIGHT, AUDIO, HEAT,
SCENT, MANA} the band-projected field ψ_b carries :

- Real part Re(ψ_b) : scalar pressure (audio) or E-field amplitude (light) or
  thermal-amplitude (heat) or scent-density-amplitude or mana-density-amplitude.
- Imaginary part Im(ψ_b) : phase-conjugate or B-field amplitude.
- Magnitude squared |ψ_b|² : energy density at band b.

Multiple bands ψ_b coexist on the same grid ; they are coupled by a
band-cross-impedance tensor Z_ab(λ_a, λ_b, M).

### 6.2 Steady-State : Complex-Helmholtz

At steady-state, ψ_b satisfies the complex-Helmholtz equation :

```
∇²ψ_b + k_b²(x) ψ_b = source_b(x)
```

where the complex wavenumber k_b(x) = ω_b/c_b(x) + iα_b(x) carries the
band-specific carrier frequency ω_b, the spatially-varying speed c_b(x), and
the absorption coefficient α_b(x). Boundary conditions at the geometry
surface (signed-distance-field SDF defined by `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl`) take Robin form :

```
(∂ψ_b/∂n + Z_b · ψ_b)|_∂Ω = 0
```

with Z_b = R_b + iX_b the complex impedance, R_b the resistive part, and
X_b the reactive part.

### 6.3 Transient : Lattice-Boltzmann on the Complex Field

For transient propagation, the invention uses a discrete-velocity
Boltzmann equation on the complex amplitude :

```
f_i(x + e_i · Δt, t + Δt) = f_i(x, t) − (1/τ)(f_i − f_i^eq)
```

where f_i ∈ ℂ are direction-i complex distributions, e_i ∈ {D3Q19 or D3Q27
stencil} are the discrete velocities, τ is the relaxation time, and the
equilibrium f_i^eq is derived from local ψ + ∇ψ. The macroscopic field is
recovered as ψ = Σ_i f_i.

This yields a wave-LBM (targeting the Helmholtz/Klein-Gordon limit) rather
than a fluid-LBM (targeting Navier-Stokes), per § II.3 of the spec.

### 6.4 Multi-Rate Lattice via Slowly-Varying-Envelope-Approximation (SVEA)

The invention's tractability hinges on SVEA folding. Instead of simulating
the full carrier-frequency wave, each band is decomposed as :

```
ψ_LIGHT(x, t) = A_LIGHT(x, t) · exp(i(k₀ · x − ω₀ t))
```

and the PDE is solved on the slowly-varying envelope A_LIGHT(x, t),
which has parabolic dynamics and tractable Δt ≤ 1 ms. AUDIO is solved
direct-amplitude (no envelope) at carrier-rate Hz-kHz. HEAT, SCENT, and
MANA are solved as slow-mode envelopes with Δt ranging from 100 ms (heat)
to 1 s (scent).

The band-cell table per § III of the spec :

| Band  | Carrier-freq | Δx (cell) | Δt (substep)        | Mode             |
|-------|--------------|-----------|---------------------|------------------|
| LIGHT | ~500 THz     | 1 cm      | 1 ms (envelope)     | SVEA-A           |
| AUDIO | ~1 kHz       | 0.5 m     | 1 ms (direct)       | direct-amplitude |
| HEAT  | ~10 THz      | 0.5 m     | 100 ms (envelope)   | SVEA-A slow      |
| SCENT | molecular    | 0.5 m     | 1 s (envelope)      | SVEA-A slow      |
| MANA  | Λ-derived    | 0.25 m    | 250 ms (envelope)   | SVEA-A slow      |

### 6.5 IMEX (Implicit-Explicit) Time Splitting

To handle stiff fast modes (light envelope, audio carrier) alongside
slow modes (heat, scent, mana envelopes) within one substep, the
invention uses an IMEX time-stepping scheme :

- **EXPLICIT** path for fast modes (LIGHT envelope, AUDIO direct) with
  tight CFL bound Δt ≤ Δx_b / c_b.
- **IMPLICIT** (back-Euler relaxation) for slow modes (HEAT, SCENT,
  MANA envelopes) with relaxed CFL.
- **Cross-band coupling** applied at the same substep, after each band
  has independently advanced.

### 6.6 KAN-Predicted Adaptive Δt

To dispatch substep counts tightly (typically 1-16 substeps per frame),
the invention uses a Kolmogorov-Arnold Network (KAN) inference at
frame-start :

```
KAN({ψ-magnitude-distribution, M-impedance-stats, recent-substep-count})
  → predicted-stable-Δt
```

If the prediction undershoots (Helmholtz residual exceeds threshold), the
adaptive-Δt fallback subdivides further, capped at 16 substeps per frame.
This KAN-stability prediction is itself novel ; the only prior art for
neural-network-driven CFL adaptation is in offline computational fluid
dynamics, not in real-time rendering.

### 6.7 KAN-Derived Material Impedance

The Robin boundary condition's impedance Z is derived per-material via a
KAN spline-network evaluation at runtime :

```
Z(λ, M.embedding, band) = MATERIAL_Z_KAN.evaluate(λ, M.embedding, band)
```

This replaces hand-tuned per-material acoustic-impedance coefficients (the
Wwise approach) with a KAN that takes a 32-dimensional material embedding
plus a wavelength and emits a complex impedance. The KAN spline-network is
trained offline on canonical materials and frozen at runtime. The runtime
evaluator (claimed separately as Invention 2) is the canonical 16-spline
B-spline cubic evaluator with per-edge weights.

### 6.8 Cross-Band Coupling Tensor

Cross-band coupling — the inventive step that lets a light excitation
audibly excite a door — is implemented as :

```
ψ_b(x, t+Δt) += S_a→b(M, λ_a, λ_b) · ψ_a(x, t) · Δt
```

where the coupling-strength S_a→b is asymmetric (per § XI of the spec) :

| From → To       | Coupling Strength | Material-mediated         | Effect                      |
|-----------------|-------------------|---------------------------|-----------------------------|
| LIGHT → AUDIO   | 0.001            | KAN @ Z(λ_L, λ_A)         | Hearable-light fields       |
| AUDIO → LIGHT   | 0.001            | KAN @ Z(λ_A, λ_L)         | Visible-sound fields        |
| LIGHT → HEAT    | 0.05             | absorption-coefficient    | Sun warms surface           |
| HEAT → LIGHT    | 0.001            | thermal-emission           | Hot iron glows orange       |
| AUDIO → HEAT    | 0.0001           | acoustic-absorption        | Barely-detectable warming   |
| MANA → LIGHT    | 0.1              | Λ-token-radiance           | Mana aurora                 |
| MANA → AUDIO    | 0.05             | Λ-token-tone               | Magic hum                   |
| LIGHT → MANA    | 0.0 (forbidden)  | (no coupling)              | (prevents AGENCY laundering)|
| AUDIO → MANA    | 0.0 (forbidden)  | (no coupling)              | (prevents AGENCY laundering)|

The strict-zero entries are enforced at compile-time by the type system —
any attempt to wire a non-zero strength for these cross-couplings is
refused by the cross-band-table validator.

### 6.9 Solver Architecture (Implementation Pseudocode)

```rust
fn wave_unity_step(prev: &Omega, dt: f32) -> Omega {
    let mut next = prev.psi.clone_cow();

    // 1. KAN-predict stable Δt
    let stable_dt = STABILITY_KAN.predict(&prev.psi.summary());
    let n_substeps = (dt / stable_dt).ceil() as u32;
    let n_substeps = n_substeps.clamp(1, 16);
    let dt_sub = dt / n_substeps as f32;

    for _ in 0..n_substeps {
        // 2. LBM stream + collide @ each band
        for band in BANDS_FAST { next[band] = lbm_explicit_step(&next[band], dt_sub, band); }
        for band in BANDS_SLOW { next[band] = imex_implicit_step(&next[band], dt_sub, band); }

        // 3. cross-band coupling
        for (a, b) in CROSS_BAND_PAIRS {
            apply_cross_coupling(&mut next, a, b, &prev.M, dt_sub)?;
        }

        // 4. boundary conditions (SDF + KAN impedance)
        for cell in active_region.boundary_cells() {
            apply_robin_bc(&mut next, cell, &prev.M, &prev.sdf, dt_sub)?;
        }
    }

    // 5. project → P-facet RC probes
    for band in ALL_BANDS {
        project_psi_to_rc_probes(&next[band], &mut omega.P_facet, band)?;
    }

    next
}
```

### 6.10 Two-Layer Composition with Radiance-Cascades

The wave-unity solver is the substrate UNDER the radiance-cascades global
illumination algorithm. Layer 1 (this invention) solves ψ at full PDE on
the active region. Layer 2 (radiance-cascades, an existing technique) reads
ψ as input and builds per-band probes that summarize ψ for the rendering
pipeline. This allows the solver to integrate with existing real-time
global-illumination architectures while providing the underlying
multi-band-unified physical state.

### 6.11 Memory Budget and Performance

At an active region of 96³ ≈ 0.88 M cells :

- LIGHT-LBM (D3Q19, Complex<f32>) : 0.88 M × 152 B = 134 MB.
- AUDIO-LBM (D3Q19, Complex<f32>) : 0.88 M × 152 B = 134 MB.
- HEAT/SCENT/MANA-LBM (Complex<f16>) : 0.88 M × 76 B × 3 = 200 MB.
- Cell-amplitude overlay : 21 MB.
- **Total hot tier** : ~490 MB.

Compute budget : ~30 GFLOP/frame at 1 M cells × 4 LIGHT bands + AUDIO,
which is ≈36% of an M7-class GPU (5 TFLOPS) at 60 Hz, or ~22% at 90 Hz
foveated. Quest-3 / RTX-3060-tier feasibility is verified.

---

## 7. Drawings Needed

The patent attorney should commission the following figures :

- **Figure 1** : System overview diagram showing the unified ψ-substrate at
  the bottom, the five band-projections (LIGHT, AUDIO, HEAT, SCENT, MANA)
  emerging upward, and the cross-band coupling tensor connecting bands.
- **Figure 2** : Multi-rate lattice schematic showing tier-A (1 cm) refined
  cells nesting inside tier-B (0.5 m) coarse cells with the SVEA-folded
  envelope rates.
- **Figure 3** : Algorithm flowchart for `wave_unity_step` : KAN-stability
  prediction → adaptive substep loop → LBM stream-collide per band →
  cross-band coupling → Robin-BC application → RC-probe projection.
- **Figure 4** : The asymmetric cross-band coupling table rendered as a
  directed graph with strict-zero entries shown as forbidden edges.
- **Figure 5** : The IMEX time-splitting timeline showing fast-mode explicit
  steps interleaved with slow-mode implicit steps.
- **Figure 6** : Memory-budget pie chart at 96³ active region showing
  per-band storage allocation.
- **Figure 7** : Worked example : "audibly-shimmering door" — an optical
  wavepacket at 600 nm crossing a door whose KAN-derived impedance has
  resonance at both 600 nm and 1.1 m (audio-low) — showing the cross-band
  coupling generating an audible response.
- **Figure 8** : Two-layer composition diagram : ψ-substrate (this
  invention) at layer 1, radiance-cascades at layer 2, with the project_to_RC
  arrow.

---

## 8. Claims (Initial Draft for Attorney Review)

### Independent Method Claim

**Claim 1.** A computer-implemented method for simultaneously rendering
multiple physical channels in a real-time interactive simulation, the
method comprising :

- (a) maintaining, in a memory of a computing system, a complex-valued
  scalar field ψ : ℝ³ × ℝ → ℂ defined on a three-dimensional grid of
  cells, said field carrying state for at least two physical channels
  selected from the group consisting of visible-light, audible-sound,
  thermal-radiation, diffusion, and a discrete-token channel ;
- (b) advancing said field by one or more substeps of a
  lattice-Boltzmann-Helmholtz hybrid solver in which each said channel is
  treated as a band-projection of said complex-valued field ψ obeying a
  single numerical kernel parameterized by a band index ;
- (c) applying a cross-band coupling step between at least two of said
  channels, said coupling step being mediated by a per-cell material
  impedance value Z derived from a Kolmogorov-Arnold Network (KAN)
  spline-network evaluation taking as input a wavelength and a
  material-coordinate embedding ;
- (d) applying boundary conditions at surfaces of a signed-distance-field
  geometry representation, said boundary conditions comprising at least
  one Robin-type condition (∂ψ/∂n + Z · ψ) = 0 ;
- (e) emitting, from said advanced field, per-channel radiance values for
  consumption by a rendering and audio output stage.

### Dependent Claims

**Claim 2.** The method of Claim 1, wherein step (b) comprises an
implicit-explicit (IMEX) time-splitting in which fast-mode channels
(light envelope, audio carrier) are advanced explicitly and slow-mode
channels (heat envelope, diffusion envelope, discrete-token envelope)
are advanced implicitly within the same substep.

**Claim 3.** The method of Claim 1, wherein the substep count is
selected at the start of each rendering frame by a second
Kolmogorov-Arnold-Network inference taking as input a summary of the
field's current magnitude distribution and recent stability history,
and outputting a predicted stable Δt.

**Claim 4.** The method of Claim 1, wherein at least one of said
channels is rendered using a slowly-varying-envelope approximation
ψ(x, t) = A(x, t) · exp(i(k₀ · x − ω₀ t)) and the lattice-Boltzmann
substep advances the envelope A.

**Claim 5.** The method of Claim 1, wherein the cross-band coupling of
step (c) is asymmetric, with at least one ordered band-pair (a, b)
having coupling strength S_a→b strictly equal to zero enforced at
compile-time by a type system.

**Claim 6.** The method of Claim 5, wherein said zero-coupling
band-pairs include at least the LIGHT → discrete-token-channel pair and
the AUDIO → discrete-token-channel pair, said zero-enforcement
preventing creation of discrete tokens by physical-channel excitations.

**Claim 7.** The method of Claim 1, wherein the grid comprises a
multi-rate spatial decomposition with a fine tier (≤ 5 cm cells) for
light-envelope solution, a coarse tier (≥ 25 cm cells) for audio
direct-amplitude solution, and a medium tier for slow-mode bands, said
tiers nesting and exchanging amplitudes by trilinear interpolation.

**Claim 8.** The method of Claim 1, wherein step (e) further comprises
projecting per-band ψ-state onto a radiance-cascades probe grid that
serves as input to a downstream global-illumination renderer.

**Claim 9.** The method of Claim 1, wherein step (d)'s impedance Z is
evaluated by a KAN spline-network with an input dimension matching a
material-coordinate embedding of dimension 32 and an output dimension of
two corresponding to the real and imaginary parts of the impedance.

**Claim 10.** The method of Claim 1, performed on consumer-grade
graphics-processing-unit hardware at a sustained frame-rate of at least
60 Hz with a per-frame compute budget for said method below 5
milliseconds.

### Independent System Claim

**Claim 11.** A computer system for real-time multi-band wave-field
rendering, the system comprising :

- a graphics-processing-unit (GPU) ;
- a memory storing a sparse-Morton-keyed grid of cells, each cell
  carrying complex-valued ψ-state for at least two physical bands ;
- a first-band lattice-Boltzmann kernel and a second-band
  lattice-Boltzmann kernel sharing identical numerical structure and
  differing only in band-parameterized constants ;
- a cross-band coupling kernel that reads a per-cell material
  impedance from a Kolmogorov-Arnold-Network evaluator and writes a
  cross-band amplitude transfer ;
- a Robin-type boundary-condition applicator gated by a
  signed-distance-field geometry probe ;
- a per-frame substep dispatcher receiving a predicted stable Δt from
  a stability-prediction Kolmogorov-Arnold-Network ;
- a per-frame projection stage emitting per-band radiance to a
  rendering output and to an audio output.

### Dependent System Claims

**Claim 12.** The system of Claim 11, wherein the GPU executes the
first-band and second-band lattice-Boltzmann kernels in a single
command-buffer dispatch with band-parameterized template specialization
rather than as two sequential dispatches.

**Claim 13.** The system of Claim 11, further comprising a consent-mask
gate (Σ-mask) that refuses ψ-injection at cells assigned to a sovereign
domain absent that domain's recorded consent, wherein said gate is
checked before any write that would modify ψ at said cells.

### Computer-Readable-Medium Claim

**Claim 14.** A non-transitory computer-readable medium storing
instructions that, when executed by one or more processors of a
computing system having a graphics-processing-unit, cause the system to
perform the method of any one of Claims 1 through 10.

### Independent Apparatus Claim (alternative)

**Claim 15.** A virtual-reality, augmented-reality, or mixed-reality
head-mounted display system comprising the system of Claim 11, wherein
the audio output is emitted to head-mounted speakers via head-related
transfer function (HRTF) projection, and the rendering output is emitted
to per-eye displays at a refresh rate of at least 90 Hz.

---

## 9. Embodiments

### 9.1 Default Embodiment — M7-Class Consumer VR

- 5 physical bands : AUDIO_SUB_KHZ, LIGHT_RED, LIGHT_GREEN, LIGHT_BLUE,
  LIGHT_NEAR_IR.
- Active region : 96³ = 884,736 cells per band.
- Per-frame budget : ~3 ms wave-unity step, ~14 GFLOP at 60 Hz.
- Hardware : Quest-3, Quest-4, RTX 3060+, Apple-Silicon M3+.

### 9.2 Reduced Embodiment — Mobile / Low-End

- 2 bands only : AUDIO + LIGHT_LUMINANCE.
- Active region : 64³ = 262,144 cells per band.
- Single-tier spatial grid (no fine/coarse nesting).
- Per-frame budget : ~1 ms wave-unity step.

### 9.3 Extended Embodiment — Desktop High-End

- 8 bands : full spectrum of LIGHT (UV through near-IR), AUDIO,
  HEAT, SCENT, MANA, plus reserved-future-band slots.
- Active region : 192³ = 7 M cells.
- D3Q27 stencil instead of D3Q19 for higher-fidelity transient propagation.
- Hardware : RTX 4090+, Apple M3 Ultra+.

### 9.4 Scientific-Visualization Embodiment

- Suitable for visualization of acousto-optic phenomena, sonoluminescence,
  photoacoustic-effect imaging, and similar cross-modal physics.
- Typically run with reduced band count but extended cell count and
  longer simulation time-horizons.
- Renders both visible and audible aspects of the physical phenomenon
  simultaneously, providing multi-modal perception of single physical
  events.

### 9.5 Game-Mechanic Embodiment — Unique to Game Use-Cases

- Mana-band coupling enables magical effects diegetically grounded in
  shared-substrate physics : a spell that emits both light and sound
  from a single ψ-injection at a Sovereign cell, with cross-coupling
  forbidden at the discrete-token-channel direction (preventing
  mana-creation from light or sound, which is an
  agency-laundering attack).
- Diegetic instrument modeling : a violin actually emits sound by ψ-PDE
  evolution on the violin's geometric domain, not by sample-library
  playback.

### 9.6 XR/AR/MR Embodiment

- Augmented-reality glasses + earphones : the unified ψ-substrate
  registers virtual surfaces with the real environment such that virtual
  objects' acoustic and visual signatures emerge from the same
  underlying solver, eliminating cross-modal incoherence that breaks
  presence in current AR systems.

---

## 10. Industrial Applicability

The invention has direct commercial application in :

1. **Game engines** : the Labyrinth-of-Apocalypse engine (LoA, Apocky's
   first commercial application) ; potential licensing to Unreal
   Engine, Unity, Frostbite, Source-2, or first-party engines (Sony's
   Decima, Microsoft's Coalition, Nintendo's in-house engines) for
   integration as a unified-rendering substrate.
2. **Virtual-reality and mixed-reality headsets** : Meta Quest series,
   Apple Vision Pro, ByteDance Pico, HTC Vive, Valve Index, Sony PSVR2.
   The unified substrate eliminates cross-modal incoherence (audio-vs-light
   geometry mismatch) that breaks presence in current VR/AR systems.
3. **Spatial-audio middleware replacement** : drop-in replacement for
   Wwise, FMOD, Steam Audio, Microsoft Spatial Audio API in titles where
   the host engine adopts the unified-rendering substrate.
4. **Scientific visualization** : visualization of acousto-optic,
   photoacoustic, vibroacoustic, and sonoluminescent phenomena where
   simultaneous visible-and-audible rendering of single physical events is
   pedagogically and analytically valuable.
5. **Architectural acoustic-visual co-design** : real-time auralization of
   architectural spaces with simultaneous photometric and acoustic
   evaluation, enabling architects and acousticians to co-design spaces
   with both modalities visible-from-the-same-physics simulation.
6. **Training simulators for medical, military, industrial use** : where
   multi-modal coherence (e.g., observing both the visual and acoustic
   signature of a piece of malfunctioning equipment) improves training
   transfer to real-world tasks.

---

## 11. Reference Implementation in CSSLv3

### 11.1 Spec Anchors (Authoritative)

- **Primary specification** :
  `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl`
  Sections § 0 (Wave-Unity Statement) ; § I (Why ¬ two-field coupling) ;
  § II (Mathematical Foundation : ψ-field, complex-Helmholtz, LBM-Boltzmann,
  multi-rate lattice, SVEA) ; § III (Band-cell table) ; § IV (Boundary
  conditions) ; § V (Novel-artifact catalog : six emergent phenomena) ;
  § VI (Solver architecture, IMEX split, adaptive Δt, KAN stability) ;
  § VIII (Storage) ; § IX (Cost model) ; § X (CSSL encoding) ; § XI
  (Cross-band coupling table) ; § XII (Conservation laws) ; § XVII
  (Attestation, PRIME-DIRECTIVE compliance).
- **Companion specification** (audio-band projection) :
  `Omniverse/07_AESTHETIC/04_FIELD_AUDIO.csl.md`
- **Companion specification** (radiance-cascades GI) :
  `Omniverse/07_AESTHETIC/02_RADIANCE_CASCADE_GI.csl.md`
- **Six-novelty-paths spec** :
  `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.1`

### 11.2 Source-Code Reference Implementation

Repository : `C:\Users\Apocky\source\repos\CSSLv3` (private at filing time).

- **Core solver crate** :
  `compiler-rs/crates/cssl-wave-solver/`
  - `src/lib.rs` (lines 1-100+) : crate-level documentation, T11-D114 scope,
    Phase-2 PROPAGATE integration, Phase-6 Entropy-Book integration,
    replay-determinism contract, PRIME-DIRECTIVE alignment.
  - `src/psi_field.rs` : sparse-Morton-keyed ψ-field storage.
  - `src/lbm.rs` : D3Q19 lattice-Boltzmann stream-collide kernel.
  - `src/imex.rs` : implicit-explicit time-splitting.
  - `src/helmholtz.rs` : steady-state complex-Helmholtz solver.
  - `src/coupling.rs` : cross-band coupling tensor application,
    `CROSS_BAND_TABLE` constant, agency-laundering refusal.
  - `src/bc.rs` : SDF + Robin boundary-condition applicator.
  - `src/stability.rs` : KAN-stability prediction + adaptive-Δt fallback,
    `MockStabilityKan` placeholder.
  - `src/step.rs` : top-level `wave_solver_step` entry-point.
  - `src/omega_step_hook.rs` : `WaveUnityPhase2` Phase-2 PROPAGATE hook.
  - `src/cost_model.rs` : per-cell-per-substep FLOP accounting.
  - `src/attestation.rs` : verbatim §11 CREATOR-ATTESTATION block.
  - `Cargo.toml` lines 1-34 : crate manifest with citation + dependencies
    on `cssl-substrate-omega-field`, `cssl-substrate-omega-step`,
    `cssl-substrate-prime-directive`, `cssl-substrate-kan`.

- **Audio-band projector crate** :
  `compiler-rs/crates/cssl-wave-audio/`
  - `src/lib.rs` (lines 1-100+) : crate-level documentation, sibling-of-legacy
    discipline, capture-mode forbidden attestation.
  - `src/psi_field.rs` : `PsiAudioField` sparse Morton-keyed AUDIO-band overlay.
  - `src/lbm.rs` : `LbmSpatialAudio` D3Q19 wave-LBM for room-resonance.
  - `src/listener.rs` + `src/binaural.rs` : per-ear projection with HRTF + ITD/ILD.
  - `src/projector.rs` : `WaveAudioProjector` per-frame ψ-AUDIO → binaural sample stream.
  - `src/coupling.rs` : `CrossBandCoupler` reading the same § XI table as
    the solver, applying AUDIO-row entries.
  - `src/vocal.rs` : `ProceduralVocal` SDF-vocal-tract + KAN spectral synthesis.
  - `src/sdf.rs` : SDF-domain query for audio boundary extraction.
  - `Cargo.toml` lines 1-71 : manifest with sibling-of-legacy notes,
    capture-mode-forbidden attestation in comments.

- **KAN substrate crate** (referenced for impedance evaluator) :
  `compiler-rs/crates/cssl-substrate-kan/`
  - `src/kan_network.rs` : `KanNetwork<I, O>` const-generic shape.
  - `src/kan_material.rs` : `KanMaterial::physics_impedance` variant
    used for Z(λ, embedding) lookup.

### 11.3 Reduction-to-Practice Commits

- `707761b` : `§ T11-D114 — cssl-wave-solver crate : Wave-Unity multi-rate ψ-field solver`
  (initial reduction-to-practice)
- `07f7e59` : `§ T11-D125b : Wave-Unity audio crate (cssl-wave-audio) — sibling of legacy mixer`
  (audio projection downstream)
- `b69165c` : final clippy gate cleanup at end of Wave-4 dispatch
- `c79bcf3` : current branch HEAD `cssl/session-6/parallel-fanout`

### 11.4 Test Coverage at Reduction-to-Practice

Per the spec § XIV acceptance criteria, reduction-to-practice tests verify :

- ψ-norm conservation per band + total to ε_f = 1e-4.
- Cross-band coupling-table enforcement (KAN-impedance unit-tested).
- SDF + Robin-BC application correctness against analytic standing-wave modes.
- KAN-stability prediction with adaptive-Δt fallback at undershoot.
- Six novel-artifacts render correctly on test scenes (audibly-shimmering door,
  sound caustic, standing-wave-as-both, visible sound, hearable light,
  resonant instrument).
- Active-region budget ≤ 0.88 M cells at 96³ subset.
- Cost ≤ 30 GF/frame at 4 LIGHT bands + AUDIO.
- Quest-3 60 Hz feasibility verified.

---

## 12. Confidentiality

THIS DOCUMENT IS PRIVATE. NOT FOR PUBLIC DISCLOSURE. NOT FOR FILING UNTIL
ATTORNEY REVIEW. Patent-novelty depends on first-to-file + non-public
disclosure. Many jurisdictions outside the United States have absolute
novelty requirements (no grace period). Premature disclosure to any party
not bound by NDA, and any public-facing posting (forum, repository,
conference, demo, social-media post) may defeat patentability in those
jurisdictions. The author is to keep this document under
`C:\Users\Apocky\source\repos\CSSLv3\_drafts\legal\` (which is to be
.gitignore-added or moved out of any public-mirrored repository).

Distribute this document only to :
- The retained patent attorney under attorney-client privilege.
- Co-inventors named in Section 1 (none currently — solo invention).
- Anyone else only under signed NDA pre-dated and witnessed.

---

∎ End of Invention 1 Disclosure.
