# Invention Disclosure : Hyperspectral KAN-BRDF — Per-Pixel Kolmogorov-Arnold-Network Spline-Based 16-Band Spectral Material Evaluator with Native Iridescence, Dispersion, and Fluorescence on Consumer GPU Hardware

**PRIVATE — NOT FOR PUBLIC DISCLOSURE — PRE-FILING DRAFT**

---

## 1. Inventor Information

- **Inventor** : Apocky <apocky13@gmail.com>
  *(legal-name placeholder ; insert filing-jurisdiction-correct legal name at attorney handoff)*
- **Date of conception** : earliest CSSLv3 spec author-date for
  `Omniverse/07_AESTHETIC/03_SPECTRAL_PATH_TRACING.csl` and
  `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING.csl`. Verify against
  `git log --diff-filter=A` for both spec files (path-V.2 of the
  six-novelty-paths roster).
- **Date of reduction-to-practice** : Stage-6 implementation under
  `compiler-rs/crates/cssl-spectral-render/`, integrated against the
  `KanMaterial::spectral_brdf<16>` evaluator in
  `compiler-rs/crates/cssl-substrate-kan/`. Recovery point of record
  is the parallel-fanout substrate-evolution branch (commit `b69165c`
  bringing the substrate runtime online plus subsequent Stage-6
  follow-on slices).
- **Branch of record** : `cssl/session-6/parallel-fanout`.

---

## 2. Title of Invention

**Method, System, and Computer-Readable-Medium for Per-Pixel Real-Time
Hyperspectral Bidirectional Reflectance Distribution Function Evaluation,
Comprising a Kolmogorov-Arnold-Network Spline-Based Material Evaluator
that Produces, in a Single Forward-Pass on Consumer-Grade GPU Hardware,
a Sixteen-Band Reflectance Spectrum Conditioned on View Direction, Light
Direction, Hero Wavelength, and a Material-Embedding Vector, with
Compositionally-Native Iridescence by Thin-Film Interference, Dispersion
by Wavelength-Dependent Index of Refraction, and Fluorescence by
Excitation-to-Emission Spectral Remapping, Without Mid-Pipeline RGB
Conversion**

Short title : **Hyperspectral KAN-BRDF (KanBrdfEvaluator)**

---

## 3. Technical Field

This invention falls within the field of real-time computer graphics,
specifically :

1. **Spectral rendering** : the rendering of light-transport using a
   wavelength-resolved (rather than tristimulus / RGB) representation
   of radiance throughout the rendering pipeline, producing colorimetric
   output only at the final tonemap stage.
2. **Bidirectional reflectance distribution function (BRDF) evaluation** :
   the computation, at every shaded fragment, of the ratio of outgoing
   radiance in the view direction to incoming irradiance from the light
   direction, as a function of surface material parameters.
3. **Neural shading** : the use of small neural networks evaluated
   per-pixel inside fragment shaders or compute shaders to express
   material appearance.
4. **Special-case material phenomena** : iridescence (peacock feather,
   oil-on-water, butterfly-wing), dispersion (prism, faceted gemstone,
   crystal), and fluorescence (highlighter pigment, biological tissue,
   uranium glass) — all of which are wavelength-dependent in a way
   that RGB rendering cannot represent without ad-hoc hacks.
5. **Mixed-reality and virtual-reality output** : per-eye stereoscopic
   rendering at consumer head-mounted-display refresh rates (Quest 3
   class, Vision Pro class) within tight per-frame budgets.

The invention is targeted at consumer-grade GPU hardware in the
M7-class performance envelope (e.g., Apple M-series with metal3,
NVIDIA RTX 4060 mobile, AMD Radeon 7700S, Snapdragon 8 Gen 3 / Adreno
750) within head-mounted-display per-frame budgets of 11.1 ms (90 Hz)
or 8.3 ms (120 Hz), and within the per-fragment Stage-6 budget of
1.8 ms.

---

## 4. Background / Prior Art

### 4.1 The Status Quo : RGB-Throughout Rendering

Almost every consumer real-time graphics engine in production today
operates on a tristimulus (red-green-blue) representation of light
throughout the entire pipeline. Albedo textures store three channels
of reflectance ; lighting integrates three channels of radiance ;
post-processing operates on three channels ; tonemapping maps three
channels of high-dynamic-range RGB to display-referred RGB. Examples
include but are not limited to Unreal Engine 5, Unity HDRP, Frostbite,
Anvil, REDengine, idTech, Snowdrop, and the rendering paths of every
major head-mounted-display title on the market in the 2024-2026 era.

**Deficiency 1 — color is not RGB.** Real reflectance spectra cannot
be losslessly represented by any three-channel basis. RGB rendering is
fundamentally a perceptual approximation chosen for its convenience,
not for its physical accuracy. Phenomena that depend on the actual
spectral distribution of light — including but not limited to
metameric failure under different illuminants, color shifts under
non-standard whitepoints, and any phenomenon involving wavelength-
dependent interference, refraction, or fluorescence — are either
mishandled or hand-faked by RGB renderers.

**Deficiency 2 — iridescence is faked.** Production renderers fake
iridescence with view-angle-modulated hue shifts on top of an RGB
base color, typically with a 1D look-up table in HSV space. The
underlying physics is thin-film interference : two coherent reflections
off a thin film at depth `d` and refractive-index ratio `n` interfere
constructively or destructively as a function of wavelength `λ` and
incidence angle `θ`. The fake produces visually-pleasing rainbow
streaks but does not predict the actual color of a physical sample,
does not respond correctly to non-standard illuminants, and exhibits
incorrect angular dependence.

**Deficiency 3 — dispersion is faked.** Real dispersion (prismatic
splitting of white light by index-of-refraction variation across
wavelength) requires either (a) full spectral path tracing or (b)
ad-hoc per-channel index-of-refraction with three samples (one for
red, green, blue), the latter producing only crude three-band rainbow
effects rather than a continuous spectral fan.

**Deficiency 4 — fluorescence is essentially absent.** RGB renderers
have no way to express that a uranium-glass sample, illuminated by
ultraviolet light invisible to the camera, glows green. The mechanism
— a photon at wavelength `λ_excitation` is absorbed and re-emitted at
a longer wavelength `λ_emission` — has no counterpart in tristimulus
rendering.

### 4.2 The Prior Art in Spectral Rendering

Spectral path tracers in offline production have existed for decades
(Manuka, hero-wavelength sampling per Wilkie, Nawaz, et al., 2014)
and produce physically-correct colorimetry. Their relevance to the
present invention :

**Manuka (Weta Digital, ~2018)** uses a 16-band spectral hero-
wavelength path tracer for offline cinematic rendering. Per-frame
times are in minutes-to-hours per pixel-sampled image. Manuka is not
real-time and is not designed for per-pixel evaluation in a tight
budget.

**Hero-wavelength sampling (Wilkie et al., 2014)** is the foundational
technique for efficient spectral path tracing : pick one "hero"
wavelength `λ₀`, carry a small number `N-1` of "accompanying"
wavelengths `λ₁..λ_{N-1}` in the same path, weight them by the
multiple-importance-sampling balance heuristic, and accumulate
spectrally-correct radiance at a small constant overhead per path
segment.

**Mantiuk's contrast-sensitivity-function (CSF) work (2024)** provides
a perceptual gating model that allows downstream culling of spectral
detail in regions of the image where the human visual system cannot
perceive the full-band difference. CSF is well-known but has not been
deployed inside a consumer real-time spectral pipeline because no
such pipeline exists.

**RGB-via-spectral-upsampling (Smits 1999, Jakob & Hanika 2019)** is
the inverse problem : given an RGB albedo, produce a plausible
spectrum that integrates back to the original RGB under a reference
illuminant. This is conceptually opposite to the present invention,
which carries spectra throughout and produces RGB only at tonemap.

**Neural radiance caches (NRC, Müller et al., 2021)** use multilayer-
perceptron networks evaluated per-fragment to express residual
radiance error after a primary-path tracing step. Their networks are
RGB-output and not spectral.

**Neural BRDFs (Sztrajman et al., 2021 ; Kuznetsov et al., 2021)** use
multilayer-perceptron networks to express measured BRDF data as a
compact representation. They are RGB-output and not spectral.

### 4.3 What Is Missing

What is missing — and what the present invention provides — is a
spectral material evaluator that :

1. is a single forward-pass per pixel within a tight real-time budget,
2. uses a Kolmogorov-Arnold-Network (KAN, Liu et al., 2024)
   spline-based representation rather than a multilayer perceptron,
3. is conditioned on a material-embedding vector that is producable
   per-asset offline,
4. produces 16 spectral bands of reflectance natively without RGB
   conversion mid-pipeline,
5. gates iridescence, dispersion, and fluorescence on
   embedding-encoded thresholds rather than on engine flags or
   shader variants,
6. integrates a Manuka-style hero + accompanying-wavelengths path
   sampling so the renderer pays for `N` bands at a cost much less
   than `N` independent renders,
7. integrates a CSF-aware perceptual gate that elides spectral detail
   where the human visual system cannot perceive it, and
8. attests to bystander-safety and consent-architecture compliance
   per the CSSLv3 PRIME DIRECTIVE so the system can be safely
   deployed in mixed-reality settings.

The novel combination of these elements, operating in a single
fragment-per-pixel evaluation within a 1.8 ms Stage-6 budget on
consumer GPU hardware, has not appeared in any prior art known to
the inventor as of the date of this disclosure.

---

## 5. Summary of the Invention

The invention is a per-fragment hyperspectral material evaluator
that, in a single forward-pass on a consumer-grade GPU shader,
produces a 16-band spectral reflectance from a view direction `ω_v`,
a light direction `ω_l`, a hero wavelength `λ_h`, and a per-asset
material embedding `m ∈ ℝ^M` encoding spline coefficients.

The novel evaluator is implemented as a Kolmogorov-Arnold-Network
(KAN). A KAN replaces the fixed scalar nonlinearities of a standard
multilayer perceptron with learnable univariate B-spline (or wavelet)
edge functions ; in this invention, the spline coefficients are
quantized and packed into the material embedding `m` so each
fragment-shader instance can re-load the per-pixel-relevant subset
of `m` from memory and evaluate the network in flight without a
weight matrix per layer.

The 16-band output spectrum is sampled in a hero-wavelength sense :
one of the 16 bands is designated `λ_h` for the path being shaded ;
the other 15 are accompanying samples ; the multiple-importance
sampling weights produce an unbiased spectral estimator with the
hero-wavelength-sampling property that adjacent pixels on a smooth
surface tend to choose adjacent hero wavelengths so the variance is
spread uniformly across the spectrum at the image level.

Iridescence is evaluated when the embedding axis `m_15` (an
"anisotropy" axis) exceeds a threshold `τ_aniso`. Below that
threshold, the iridescence module is bypassed entirely (a runtime
branch on a uniform value, predictable by the GPU, does not pay
the iridescence cost). Above that threshold, a thin-film
interference model receives the per-fragment film thickness `d`,
the local index-of-refraction ratio `n`, and the incidence angle
`θ`, and produces a per-band reflectance modulation that is
multiplied into the KAN output.

Dispersion is evaluated when the embedding axis `m_14` exceeds
a threshold `τ_disp`. Above the threshold, a wavelength-dependent
index-of-refraction `n(λ)` is computed (from a Cauchy or Sellmeier
form parametrized by the embedding) and used to bend the path of
the light ray's per-band component, producing a continuous
spectral fan rather than a three-band approximation.

Fluorescence is evaluated for materials whose embedding indicates
a non-zero excitation-emission cross section. The implementation
samples the excitation spectrum at a probabilistically-chosen UV
or visible wavelength, evaluates the fluorescence yield at the
emission wavelength according to the per-material fluorescence
matrix, and adds the emitted spectrum to the per-band output.

The CSF-aware perceptual gate, evaluated per-tile rather than
per-pixel, decides whether each tile's reflectance is to be
evaluated at full 16-band resolution or at a reduced 4-band
resolution (corresponding roughly to a luminance + opponent-color
representation). The gate is informed by foveation as well : in
peripheral regions, where the human visual system cannot perceive
fine spectral detail, the 4-band path is taken.

The system attests, per the CSSLv3 PRIME DIRECTIVE, that the
spectral material evaluator is **pure-evaluation** — it does not
mutate substrate state, does not consume the per-cell sovereignty
mask, and is replayable and deterministic by construction. This
makes it safe for deployment in mixed-reality bystander-present
contexts where surveillance-adjacent capture must be impossible by
construction.

The combination of KAN-spline evaluation, 16-band hero-MIS, native
iridescence/dispersion/fluorescence on embedding-gated paths, CSF
perceptual gating, and PRIME-DIRECTIVE compliance, all within a
1.8 ms per-frame budget on consumer hardware, is the inventive step.

---

## 6. Detailed Description

### 6.1 System Overview

The invention sits as Stage-6 of a 12-stage real-time render pipeline.
It consumes the output of Stage-1 (signed-distance-field raymarching
producing `RayHit { p, n, t, mat_id, σ_facet }`), Stage-5
(post-fractal-amplified positions and roughness perturbations), and
the Stage-2 foveation mask. It produces a per-eye, per-pixel,
16-band spectral radiance buffer that is consumed by Stage-7 (radiance
cascades for global illumination), Stage-8 (post-processing in
spectral space), and Stage-10 (CIE-XYZ mapping and ACES-2 tonemap).

### 6.2 Spectral Band Table

The canonical band table comprises 16 bands :

- 10 visible bands at center wavelengths 400, 440, 470, 500, 530,
  560, 590, 620, 650, 680 nm (slightly non-uniform spacing chosen
  to align with CIE 2-degree color-matching function peaks),
- 4 near-infrared bands at 720, 780, 850, 950 nm (for material
  appearance under non-visible illumination, reflective signature
  for AR-overlay opacity hints, and fluorescence excitation seeds),
- 2 ultraviolet bands at 365 and 380 nm (for fluorescence
  excitation and certain natural materials such as flower-petal
  ultraviolet patterning).

Each band has a center wavelength `λ_c`, a half-bandwidth, a CIE
2-degree color-matching response triple `(x̄, ȳ, z̄)`, and a CSF
coefficient `s_λ` indicating relative perceptual weight.

### 6.3 KAN Material Embedding

For each material in the scene, an offline asset-conditioning step
produces an embedding vector `m ∈ ℝ^M`. The choice of `M` is bounded
by the per-pixel memory budget ; in the reference implementation
`M = 32`. The first 16 axes of `m` encode the KAN spline coefficients
of the network's edge functions ; the remaining 16 axes encode :

- `m_24..m_27` : base diffuse spectrum coefficients in a small
  basis-function expansion (Smits-style `4`-coefficient basis),
- `m_28..m_29` : metallic-color modulation,
- `m_30` : surface roughness baseline,
- `m_31` : the seven-bit feature flag set
  `(iridescent | dispersive | fluorescent | subsurface | anisotropic
   | wavelength-shifting | anti-noise-mode)`,
- `m_14` : `n_disp` dispersion modulation strength (also used as
  a gating threshold),
- `m_15` : `aniso_strength` anisotropy strength (also used as the
  iridescence gate),
- `m_16..m_23` : reserved for future spectral phenomena.

The KAN network has a small fixed topology (e.g., 4 input edges to
a hidden layer of 8 nodes, 8 hidden edges to a 16-output layer)
chosen so the per-pixel evaluation cost fits within budget. Each
edge function is a B-spline with `K_spline = 7` knot points ; the
spline coefficients are quantized to 8-bit per coefficient and
packed into `m`'s first 16 axes by a fixed encoding.

### 6.4 KAN Forward Pass per Fragment

The forward pass evaluates :

```
y(x) = Σ_l Σ_{ij} φ_{l, i, j}(x_i)
```

where `φ_{l, i, j}` are the learnable B-spline edge functions, `l`
is the layer index, and `(i, j)` are the source-and-destination
node indices within that layer. The B-spline evaluation is :

```
φ(t) = Σ_k c_k · B_k(t ; knot_table)
```

where `c_k` are the per-edge coefficients packed into `m` and
`B_k(t)` are the basis splines (computed once per shader invocation
from a constant knot table). The constant knot table is shared
across all materials and resides in a uniform buffer ; the
per-material coefficients `c_k` come from `m`.

The input `x` to the network is the 4-vector
`(cos θ_v, cos θ_l, h, λ_h)` where `h` is the half-vector dot
product `H · N` and `λ_h` is the hero wavelength normalized to
`[0, 1]`. The network output is a 16-vector `r̃(λ_band)` of
band-reflectances.

### 6.5 Hero-Wavelength Multiple-Importance Sampling

For each pixel-path, a hero wavelength `λ_h` is chosen uniformly
from the band table at the start of the path (Stage-1 raymarch).
The KAN evaluator at Stage-6 produces `r̃(λ_band)` for all 16
bands. The 1 hero band carries the full PDF weight of the path
sample ; the 15 accompanying bands carry the MIS-weighted
co-sampling. The accumulator at Stage-7 (radiance cascades) holds
a 16-band per-cell radiance estimate that converges to the
spectrally-correct value as the per-pixel sample count grows.

The variance benefit of hero-wavelength sampling is that nearby
pixels on a smooth surface tend to be assigned different hero
wavelengths (chosen by a low-discrepancy permutation per tile),
so any spectral-band-specific noise is decorrelated across the
image and reads, to the human eye, as low-frequency fine grain
rather than high-frequency band-aliasing.

### 6.6 Iridescence : Thin-Film Interference

When `m_15 > τ_aniso`, the iridescence module is evaluated. The
thin-film model takes the per-fragment film thickness `d` (read
from the embedding or from a thickness texture if present), the
ratio of film-to-substrate refractive indices `n_2/n_1`, and the
local incidence angle `θ`, and produces a per-band reflectance
modulation :

```
R_iridescent(λ, θ, d, n) =
  (R_s(λ, θ, n) + R_p(λ, θ, n)) / 2
  · interference_factor(λ, d, θ, n)

interference_factor(λ, d, θ, n) =
  cos²(2π · d · n · cos θ_t / λ + φ_shift(λ, n))
```

where `R_s` and `R_p` are the Fresnel s- and p-polarized reflectances
at the air-film interface, `θ_t` is the angle inside the film by
Snell's law, and `φ_shift` accounts for the half-wavelength phase
shift on reflection from a high-index substrate. The per-band
modulation is multiplied into the KAN output reflectance. The model
is correct to within the validity range of thin-film optics (single-
film, planar, coherent illumination), which covers the majority of
visible iridescent materials in real-time graphics.

### 6.7 Dispersion : Wavelength-Dependent Index of Refraction

When `m_14 > τ_disp`, the dispersion module computes a per-band
index of refraction `n(λ)` from a parametric form (Cauchy or
Sellmeier, selected by another bit in `m_31`'s flag set) :

```
n_Cauchy(λ) = a_0 + a_1 / λ² + a_2 / λ⁴

n_Sellmeier(λ) = sqrt(1 + Σ_i B_i · λ² / (λ² - C_i))
```

The Cauchy form's three coefficients `(a_0, a_1, a_2)` and the
Sellmeier form's six coefficients `(B_1..B_3, C_1..C_3)` are
packed into `m_24..m_29` for dispersive materials, sharing storage
with the diffuse spectrum coefficients (the flag in `m_31`
indicates which interpretation to use ; the union saves 6 axes
of embedding for the common case where a material is either
dispersive *or* matte-diffuse-spectrum, not both).

The per-band ray direction is computed by Snell's law at the
intersection point with `n_band = n(λ_band)`, producing 16
slightly different refracted ray directions. The refracted rays
are accumulated using the same hero-MIS scheme as the reflectance
evaluation. The visual result is a continuous spectral fan, not
the three-band rainbow approximation common in RGB engines.

### 6.8 Fluorescence : Excitation-Emission Spectral Remap

For fluorescent materials, a per-band fluorescence matrix `F_ij`
relates excitation at band `i` to emission at band `j`. The
matrix is sparse (only a few off-diagonal entries are nonzero)
and is encoded in `m_16..m_23` as a low-rank approximation
`F ≈ u v^T` where `u, v ∈ ℝ^16`.

At runtime, the incoming radiance at each excitation band is
multiplied by the fluorescence yield `F_ij`, producing emitted
radiance at the emission band, which is added to the BRDF output
(net of any reflectance loss on the same band). For uranium glass
and analogous strong fluors, this produces the correct UV-to-green
glow under blacklight illumination, which RGB rendering cannot
reproduce.

### 6.9 CSF-Aware Perceptual Gate

The Mantiuk-2024 contrast-sensitivity-function model provides a
per-tile perceptual weight that estimates whether the human visual
system can distinguish the full 16-band reflectance from a
4-band luminance-plus-opponent-color approximation under the
expected viewing conditions (HMD pixel pitch, frame rate, viewing
geometry).

The gate operates per-tile (16x16 pixels, matching the foveation
mask resolution). When the per-tile CSF score is below a threshold
`τ_csf`, the tile is shaded with the 4-band path ; when above, the
full 16-band path is used. The 4-band path costs ~25% of the
16-band path ; tiles in the periphery (large foveation index)
typically score below `τ_csf` and so the 4-band path is used.

### 6.10 Stage-6 Cost Model

Per-fragment cost decomposes as :

- KAN forward pass : ~0.43 ms / 1080p frame (cooperative-matrix
  dispatch where supported, fall-back to subgroup-FMA otherwise),
- Hero-MIS bookkeeping : ~0.07 ms,
- Iridescence (when active) : ~0.15 ms,
- Dispersion (when active) : ~0.20 ms,
- Fluorescence (when active) : ~0.10 ms,
- CSF gate evaluation : ~0.05 ms (per-tile, amortized to per-pixel),
- Foveation-driven 4-band fallback : ~0.05 ms / fragment for those tiles.

Total budget : 1.8 ms. With foveated rendering reducing peripheral
fragments and the CSF gate reducing peripheral spectral work, the
typical realized cost is ~1.0-1.4 ms in the foveal region and
~0.4-0.6 ms in the periphery.

### 6.11 Reference Implementation in CSSLv3

The reference implementation lives in
`compiler-rs/crates/cssl-spectral-render/`. Key entry points :

- `band::SpectralBand` — 16-band wavelength sampling.
- `radiance::SpectralRadiance` — hero-wavelength + accompanying
  spectral storage.
- `hero_mis::HeroWavelengthMIS` — Manuka-style hero + 4-8
  accompanying samples with PDF preservation.
- `kan_brdf::KanBrdfEvaluator` — per-fragment KAN evaluator that
  wires `KanMaterial::spectral_brdf<16>` to `(view_dir, light_dir,
  λ_hero) → reflectance(λ-band[16])`.
- `iridescence::IridescenceModel` — thin-film interference + dispersion.
- `fluorescence::Fluorescence` / `fluorescence::Phosphorescence` —
  excitation-to-emission spectral remap.
- `tristimulus::SpectralTristimulus` — spectral → CIE-XYZ → display
  RGB ACES-2 tonemap (Stage-10).
- `csf::CsfPerceptualGate` — Mantiuk-2024 perceptual gate.
- `pipeline::SpectralRenderStage` — Stage-6 entry point.
- `cost::PerFragmentCost` — compile/run cost model + Quest-3 budget.

The companion crate `cssl-substrate-kan` provides the
`KanMaterial::spectral_brdf<16>` evaluator referenced by the BRDF
pipeline, including persistent-tile residency on the GPU, cooperative-
matrix dispatch where the GPU supports it, and subgroup-FMA fallback
elsewhere.

### 6.12 Algorithmic Pseudocode

```
function SpectralRenderStage::evaluate(ray_hit, view_dir, light_dirs[]) :
    let m = material_embedding(ray_hit.mat_id)
    let λ_h = hero_wavelength(ray_hit.path_id)
    let csf_path = perceptual_gate(ray_hit.tile_id, foveation_mask)
    let path_count = csf_path == FullSpectral ? 16 : 4
    let mut radiance = SpectralRadiance::zero(path_count)
    for light_dir in light_dirs :
        let kan_reflectance = kan_forward(view_dir, light_dir,
                                          half_vec_dot, λ_h, m,
                                          path_count)
        let mut reflectance = kan_reflectance
        if m[15] > τ_aniso :
            reflectance *= iridescence(view_dir, light_dir, m, path_count)
        if m[14] > τ_disp :
            reflectance = dispersion_modulate(reflectance,
                                              ray_hit, m,
                                              path_count)
        if m[31].fluorescent :
            reflectance += fluorescence_emit(reflectance,
                                             excitation_spectrum,
                                             m[16..23])
        radiance += reflectance * incoming_irradiance(light_dir, λ_h)
    return radiance
```

---

## 7. Drawings Needed

The patent attorney should commission the following figures :

- **Figure 1** : System architecture diagram showing Stage-6
  positioned in the 12-stage canonical render pipeline, with input
  RayHit from Stage-1, foveation mask from Stage-2, amplified
  positions from Stage-5, and 16-band spectral radiance output to
  Stage-7, Stage-8, and Stage-10.

- **Figure 2** : KAN network topology diagram showing 4 input
  edges (`cos θ_v`, `cos θ_l`, `h = H · N`, `λ_h`), a hidden
  layer of 8 nodes, 8 hidden edges, and a 16-output layer, with
  each edge depicted as a B-spline.

- **Figure 3** : Spectral band table chart showing the 16-band
  center wavelengths along the visible-plus-NIR-plus-UV
  spectrum, with overlaid CIE 2-degree color-matching functions
  and Mantiuk-2024 CSF coefficients.

- **Figure 4** : Hero-wavelength MIS diagram showing per-pixel
  hero-wavelength assignment from a low-discrepancy tile-permutation,
  the accompanying-15-band PDF weighting, and the spectrum-image
  decorrelation property.

- **Figure 5** : Thin-film interference diagram showing two
  reflections off a film of thickness `d` and indices `n_1`, `n_2`,
  the resulting per-wavelength constructive/destructive interference,
  and the angular dependence of the interference factor.

- **Figure 6** : Dispersion diagram showing 16 slightly-different
  refracted rays at 16 different `n(λ_band)` values producing a
  continuous spectral fan, contrasted with a 3-band RGB
  approximation.

- **Figure 7** : Fluorescence diagram showing UV excitation
  absorbed at λ=365 nm and re-emitted at λ=530 nm via the
  fluorescence matrix, with the visual-spectrum result appearing
  green under invisible UV illumination.

- **Figure 8** : CSF-perceptual-gate flow diagram with the
  per-tile decision between full 16-band path and 4-band reduced
  path, conditioned on tile foveation index and Mantiuk-2024 CSF
  score.

- **Figure 9** : Per-fragment cost breakdown bar chart showing the
  individual cost components (KAN forward pass, hero-MIS, iridescence
  when active, dispersion when active, fluorescence when active,
  CSF gate, foveation fallback), summed against the 1.8 ms
  Stage-6 budget at Quest-3 reference hardware.

- **Figure 10** : Comparative output renderings showing the same
  scene rendered (a) with a baseline RGB engine, (b) with a
  3-band approximation of the present invention, and (c) with the
  full 16-band invention, showing the visual difference for an
  iridescent peacock-feather sample, a dispersive prism, and a
  fluorescent uranium-glass sample.

---

## 8. Claims (Initial Draft for Attorney)

The following claims are drafted in standard patent-claim format
for attorney review and refinement. They are intended as a starting
point and are expected to be revised in collaboration with patent
counsel.

### Claim 1 (Independent — Method)

A computer-implemented method for real-time hyperspectral
bidirectional reflectance evaluation, comprising :
- receiving, at a graphics processing unit, a per-fragment ray
  hit record specifying a surface point, a surface normal, a
  material identifier, and a hero wavelength ;
- looking up a material embedding vector indexed by the material
  identifier, the embedding vector containing quantized spline
  coefficients of a Kolmogorov-Arnold-Network and a feature flag
  set indicating whether the material is iridescent, dispersive,
  fluorescent, or any combination thereof ;
- computing, in a single forward-pass on the graphics processing
  unit, a sixteen-band spectral reflectance from a four-vector
  comprising the cosine of the view-direction-to-normal angle,
  the cosine of the light-direction-to-normal angle, a half-vector
  dot product, and the hero wavelength, by evaluating the
  Kolmogorov-Arnold-Network with the embedding-stored spline
  coefficients ;
- conditionally evaluating, gated by an iridescence threshold on
  a designated axis of the embedding, a thin-film interference
  modulation function and multiplying the resulting per-band
  modulation into the spectral reflectance ;
- conditionally evaluating, gated by a dispersion threshold on a
  designated axis of the embedding, a wavelength-dependent
  refractive index function and modulating the spectral
  reflectance accordingly ;
- conditionally evaluating, gated by a fluorescence flag in the
  feature flag set, a low-rank fluorescence excitation-emission
  matrix and adding emitted spectral radiance to the spectral
  reflectance ;
- accumulating the resulting spectral reflectance into a
  hero-wavelength multiple-importance-sampled spectral radiance
  buffer for downstream pipeline consumption.

### Claim 2 (Dependent — KAN Topology)

The method of Claim 1 wherein the Kolmogorov-Arnold-Network
comprises a fixed topology of four input edges, an eight-node
hidden layer, eight hidden edges, and a sixteen-output layer,
each edge being a B-spline with seven knot points and per-edge
coefficients quantized to eight bits packed into the embedding
vector.

### Claim 3 (Dependent — Hero MIS)

The method of Claim 1 wherein the hero wavelength is selected
per-pixel-path from the sixteen-band table by a low-discrepancy
tile-permutation that decorrelates per-band noise across the
image, and wherein the fifteen accompanying bands are sampled
with multiple-importance-sampling weights that produce an
unbiased spectral-radiance estimator.

### Claim 4 (Dependent — Thin-Film Interference)

The method of Claim 1 wherein the thin-film interference modulation
function is evaluated as
`R_iridescent(λ) = ((R_s + R_p)/2) · cos²(2π·d·n·cos θ_t / λ + φ_shift)`
with `d` a per-fragment film thickness, `n` an embedding-encoded
ratio of refractive indices, `θ_t` the angle inside the film by
Snell's law, and `φ_shift` a half-wavelength phase shift on
reflection from a higher-index substrate.

### Claim 5 (Dependent — Dispersion)

The method of Claim 1 wherein the wavelength-dependent refractive
index function is selected from one of a Cauchy form and a
Sellmeier form by a flag in the feature flag set, and wherein the
sixteen per-band ray directions are computed by Snell's law at the
intersection point with the band-specific index of refraction,
producing a continuous spectral fan upon dispersive refraction.

### Claim 6 (Dependent — Fluorescence)

The method of Claim 1 wherein the fluorescence excitation-emission
matrix is encoded as a low-rank approximation `F ≈ u v^T` with
sixteen-component vectors `u` and `v` packed into the material
embedding, and wherein at runtime the matrix is multiplied against
the incoming radiance band-vector to produce emitted-band radiance.

### Claim 7 (Dependent — Perceptual Gate)

The method of Claim 1 further comprising evaluating, per
sixteen-by-sixteen-pixel tile, a contrast-sensitivity-function
score per Mantiuk 2024, and selectively reducing the spectral
evaluation to a four-band luminance-plus-opponent-color path for
tiles whose CSF score is below a threshold.

### Claim 8 (Dependent — Foveation Coupling)

The method of Claim 7 wherein the contrast-sensitivity-function
score is conditioned on a per-tile foveation index produced by an
upstream eye-tracking-driven foveated-rendering stage, such that
peripheral tiles preferentially take the four-band reduced path.

### Claim 9 (Dependent — No Mid-Pipeline RGB Conversion)

The method of Claim 1 wherein no conversion from spectral
representation to red-green-blue tristimulus is performed at any
stage of the pipeline preceding a final tonemap stage, the
sixteen-band spectral radiance being accumulated through global
illumination, post-processing, and color-grading stages in
spectral form.

### Claim 10 (Independent — System)

A real-time graphics rendering system, comprising :
- a graphics processing unit ;
- a material asset store containing per-asset embedding vectors
  produced by an offline conditioning step, each embedding vector
  containing quantized spline coefficients of a
  Kolmogorov-Arnold-Network ;
- a fragment-shader compute kernel resident on the graphics
  processing unit, configured to perform the method of Claim 1
  per fragment within a per-frame budget of less than two
  milliseconds at a rendering resolution of one thousand
  eighty progressive pixels ;
- a sixteen-band spectral radiance buffer ;
- a downstream tonemap stage configured to convert the
  sixteen-band spectral radiance to a display-referred red-green-
  blue output via a CIE color-matching transform and an ACES-2
  tonemap operator.

### Claim 11 (Dependent — VR Application)

The system of Claim 10 wherein the rendering system is configured
for output to a head-mounted-display device producing per-eye
stereoscopic imagery at a refresh rate of at least ninety Hertz.

### Claim 12 (Dependent — Cooperative Matrix)

The system of Claim 10 wherein the fragment-shader compute kernel
employs a cooperative-matrix instruction-set extension when
available on the graphics processing unit and falls back to a
subgroup fused-multiply-add primitive otherwise.

### Claim 13 (Independent — Computer-Readable Medium)

A non-transitory computer-readable medium storing instructions
that, when executed by a graphics processing unit, cause the
graphics processing unit to perform the method of Claim 1.

### Claim 14 (Dependent — Sovereignty Compliance)

The medium of Claim 13 wherein the instructions are configured
such that the spectral material evaluator does not mutate
per-cell sovereignty-mask state and does not consume biometric
data, and wherein the resulting evaluator is replayable and
deterministic given identical inputs.

### Claim 15 (Dependent — KAN Coefficient Quantization)

The method of Claim 1 wherein the spline coefficients of the
Kolmogorov-Arnold-Network are quantized to a fixed-point
representation of fewer than ten bits per coefficient, and
wherein the coefficient-quantization error is bounded such that
the resulting spectral reflectance differs from a full-precision
evaluation by less than a perceptually-detectable threshold under
the Mantiuk-2024 contrast-sensitivity-function model.

---

## 9. Embodiments

### Embodiment A : Quest-3 Class Mobile XR

In this embodiment, the system runs on a Snapdragon-class mobile
GPU in a tethered or standalone head-mounted display. The
hero-wavelength MIS path is enabled. The CSF-aware perceptual
gate is enabled with foveation coupling per Claim 8. The
cooperative-matrix path of Claim 12 is enabled where the mobile
GPU supports it ; otherwise the subgroup fused-multiply-add
fallback is used. Per-frame budget is 11.1 ms (90 Hz) ; Stage-6
realized cost is approximately 1.0-1.4 ms in foveal regions and
0.4-0.6 ms in peripheral regions.

### Embodiment B : Vision-Pro Class Mixed Reality

In this embodiment, the system runs on a higher-budget Apple
M-series GPU with metal3. The cooperative-matrix path is the
default. The CSF-aware perceptual gate uses a tighter threshold
because the higher-pixel-pitch HMD permits perception of more
fine spectral detail. Per-frame budget is 8.3 ms (120 Hz) ;
Stage-6 realized cost is approximately 0.6-0.9 ms.

### Embodiment C : Desktop High-End Discrete GPU

In this embodiment, the system runs on a desktop NVIDIA or AMD
discrete GPU. The hero-wavelength MIS path uses a larger number
of accompanying samples (8 instead of 4) for higher-fidelity
output. The full 16-band path is the default for all tiles ; the
CSF-aware perceptual-gate fallback is used only as a far-periphery
optimization. Per-frame budget is 6.9 ms (144 Hz) ; Stage-6
realized cost is approximately 0.5-0.7 ms.

### Embodiment D : Offline Spectral Path Tracing

In this embodiment, the same KAN evaluator and embedding format
are used for offline path-tracing reference renders. The hero-MIS
path is replaced by full per-band path tracing. The 16-band table
is expanded to 32 bands or 64 bands at the cost of additional
embedding-axis storage. The output is reference-quality colorimetric
imagery suitable for benchmarking the real-time embodiments.

### Embodiment E : Inverse-Rendering / Differentiable Mode

In this embodiment, the KAN evaluator is wrapped in a
forward-mode-differentiable jet (per
`compiler-rs/crates/cssl-autodiff/src/jet.rs`) so that
per-pixel-radiance gradients can be computed with respect to
embedding axes. This enables material-asset-conditioning
(producing an embedding from a measured BRDF) by
gradient-based optimization, and enables real-time
material-fitting for content-creation workflows.

### Embodiment F : Reduced-Embedding Mobile Variant

In this embodiment, for the most resource-constrained mobile
chipsets, the embedding dimension is reduced from `M = 32` to
`M = 16`, the KAN topology is reduced to 4-input / 4-hidden /
8-output, and the spectral output is reduced to 8 bands. This
trades fidelity for cost at the lowest performance tier.

### Embodiment G : Spectral Anti-Surveillance Variant

In this embodiment, the system enforces, at compile-time, that
no spectral output buffer can be written to a network egress
sink, persistent storage, or any non-volatile destination
without explicit consent ratchet. This is implemented via
information-flow-control labeling per the
`cssl-ifc::SensitiveDomain` mechanism. The embodiment is the
default in the CSSLv3 reference implementation.

---

## 10. Industrial Applicability

This invention is industrially applicable in :

- Real-time computer-generated imagery for video games,
  cinematic real-time previews, and broadcast graphics.
- Mixed-reality and virtual-reality applications (head-mounted
  displays, smart glasses, see-through AR), where physically-
  correct color appearance under the HMD's specific display
  primaries is critical for visual realism.
- Interactive simulation for industrial design, automotive
  styling, fashion design, jewelry preview, and any product-
  visualization application where iridescence, dispersion, or
  fluorescence are visually load-bearing.
- Real-time scientific visualization, particularly bio-imaging
  applications where fluorescent stains are used and accurate
  rendering of fluorescence is required.
- Forensic and cultural-heritage visualization where the
  rendering must reproduce the spectral signature of a sample
  (gemstone, art pigment, biological tissue) under varying
  illumination.
- Real-time previews for spectral-sensor capture (multispectral
  cameras, hyperspectral cameras) where the renderer is the
  ground-truth simulator.

The licensing market includes head-mounted-display original
equipment manufacturers, real-time-rendering middleware vendors,
content-authoring-tool vendors, and game studios that wish to
integrate the technology under license rather than re-implement
it.

---

## 11. Reference Implementation in CSSLv3

The reference implementation is in :

- `compiler-rs/crates/cssl-spectral-render/src/lib.rs` — Stage-6
  module-level documentation and re-exports.
- `compiler-rs/crates/cssl-spectral-render/src/band.rs` — 16-band
  wavelength sampling table and per-band CIE/CSF coefficient
  storage.
- `compiler-rs/crates/cssl-spectral-render/src/radiance.rs` — the
  hero-wavelength + accompanying-bands `SpectralRadiance` storage
  type.
- `compiler-rs/crates/cssl-spectral-render/src/hero_mis.rs` —
  Manuka-style hero + accompanying-wavelength sampling with
  multiple-importance-sampling weights.
- `compiler-rs/crates/cssl-spectral-render/src/kan_brdf.rs` — the
  per-fragment KAN evaluator (`KanBrdfEvaluator`), wiring
  `KanMaterial::spectral_brdf<16>` to (view_dir, light_dir,
  λ_hero) input.
- `compiler-rs/crates/cssl-spectral-render/src/iridescence.rs` —
  thin-film interference module.
- `compiler-rs/crates/cssl-spectral-render/src/fluorescence.rs` —
  excitation-to-emission spectral remap module.
- `compiler-rs/crates/cssl-spectral-render/src/tristimulus.rs` —
  spectral-to-CIE-XYZ-to-display-RGB ACES-2 tonemap stage.
- `compiler-rs/crates/cssl-spectral-render/src/csf.rs` —
  Mantiuk-2024 contrast-sensitivity-function-aware perceptual gate.
- `compiler-rs/crates/cssl-spectral-render/src/pipeline.rs` —
  Stage-6 render-graph entry point.
- `compiler-rs/crates/cssl-spectral-render/src/cost.rs` —
  per-fragment cost model and Quest-3 budget validation.
- `compiler-rs/crates/cssl-substrate-kan/src/kan_material.rs` —
  the `KanMaterial::spectral_brdf<16>` evaluator that the
  Stage-6 pipeline wires.

The verbatim spec anchors are :

- `Omniverse/07_AESTHETIC/03_SPECTRAL_PATH_TRACING.csl.md` —
  hero-wavelength sampling, MIS, dispersion, iridescence,
  fluorescence, ACES-2 tonemap discipline.
- `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING.csl.md` — KAN
  network shape variants, cooperative-matrix dispatch, 16-band
  spectral-BRDF call-site signature, persistent-tile residency.
- `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.2` —
  the path-V.2 specification of the six-novelty-paths roster
  ("Renaissance palette + perceptual coherence").
- `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-6` —
  Stage-6 pipeline-position, budget, effect-row.

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

**End of Invention Disclosure 04.**
