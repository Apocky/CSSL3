# Invention Disclosure : KAN-Runtime Compute-Shader Evaluator with Cooperative-Matrix Tier-Selection, Persistent-Tile Residency, and Per-Axis-Scale INT8 Quantization

**PRIVATE — NOT FOR PUBLIC DISCLOSURE — PRE-FILING DRAFT**

---

## 1. Inventor Information

- **Inventor** : Apocky <apocky13@gmail.com>
  *(legal-name placeholder ; insert filing-jurisdiction-correct legal name at attorney handoff)*
- **Date of conception** : earliest CSSLv3 spec author-date for
  `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING.csl.md`
  (verify with `git log --diff-filter=A -- ...` in the Omniverse repo).
- **Date of reduction-to-practice** : commit `7a3646d` titled
  `§ T11-D143 : cssl-substrate-kan — KAN substrate runtime + Φ-pattern-pool`
  (substrate runtime crate, including the byte-stable `KanNetwork<I, O>`
  shape and the KAN evaluator entry-points referenced by all downstream
  shading crates).
- **Branch of record** : `cssl/session-6/parallel-fanout`.

---

## 2. Title of Invention

**Method, System, and Computer-Readable-Medium for Real-Time Per-Fragment
Evaluation of Kolmogorov-Arnold-Network Spline Functions on Graphics
Processing Units, Comprising Tiered Dispatch over Cooperative-Matrix,
Subgroup-Cooperative, and Scalar Paths ; Persistent-Kernel Shared-Memory
Residency of Spline Coefficients ; and Per-Axis-Scaled INT8 Quantization
with Bounded Quantization Error ; Suitable for Hyperspectral Bidirectional
Reflectance Distribution Function (BRDF) Evaluation, Sub-Pixel Detail
Amplification, Iridescence Stack Modeling, Fluorescence Modeling, and
Material-Property Decoding from Compact Embedding Coordinates**

Short title : **KAN-Runtime Compute-Shader Evaluator (KAN-Shading-Primitive)**

---

## 3. Technical Field

This invention falls within the field of real-time computer graphics, with
specific application to programmable shading systems on graphics-processing
units (GPUs). It sits at the intersection of three sub-fields :

1. **Programmable shading** : the runtime evaluation of mathematical
   functions during the rendering of each pixel, typically via the
   fragment-shader stage of a graphics pipeline.
2. **Neural shading** : the use of trained neural networks (multi-layer
   perceptrons (MLPs), convolutional networks, transformers) to evaluate
   shading functions at runtime, as exemplified by Neural Radiance Fields
   (NeRF), Instant-NGP, and the NVIDIA neural-rendering plugin in Aurora ML.
3. **Cooperative-matrix and tensor-core programming** : the use of recent
   GPU hardware extensions (Vulkan VK_KHR_cooperative_matrix, DirectX 12
   SM6.9 cooperative-vectors, Metal-3 simdgroup-matrix, NVIDIA Tensor
   Cores, AMD WMMA, Intel XMX) to accelerate matrix-multiply operations
   inside shader code.

The Kolmogorov-Arnold-Network (KAN) is a recently-rediscovered alternative
to the multi-layer perceptron in which the per-neuron activation function
is replaced by per-edge spline functions, allowing the network to fit
arbitrary smooth functions with potentially fewer parameters and better
interpretability. KANs were popularized by Liu et al. in mid-2024
(arXiv:2404.19756). To the inventor's knowledge, no production graphics
system has deployed KANs as a per-fragment shading primitive — current
neural-shading research is dominated by MLP-based approaches.

---

## 4. Background / Prior Art

### 4.1 The Status Quo : MLP-Based Neural Shading

The dominant paradigm in neural-shading uses multi-layer perceptrons :

- **Neural Radiance Fields (NeRF)** : Mildenhall, B., et al. "NeRF :
  Representing Scenes as Neural Radiance Fields for View Synthesis." ECCV
  2020. MLP encodes a 5D input (position + view direction) to a 4D output
  (RGBα). Not real-time on consumer hardware. Subsequent work (Instant-NGP,
  Plenoxels, TensoRF, etc.) accelerates this but retains MLP at the core.
- **Instant-NGP** : Müller, T., et al. "Instant Neural Graphics Primitives
  with a Multiresolution Hash Encoding." ACM Trans. Graph. 41, 4, Article
  102 (July 2022). Multi-resolution hash + small MLP. Real-time-ish on
  NVIDIA RTX hardware ; not optimized for cross-vendor cooperative-matrix
  paths.
- **NVIDIA Aurora ML** : NVIDIA Corp. (2023, ongoing). Plugin replacing
  shader-graph nodes with small MLPs trained per-material. Production
  but vendor-locked and MLP-based.
- **Real-Time Neural Appearance Models** : Zeltner, T., et al. "Real-Time
  Neural Appearance Models." ACM Trans. Graph. 43, 4, Article 56 (July
  2024). MLP per-fragment shading on tensor cores. Closest prior art ;
  uses MLP, not KAN, and lacks the per-axis-scaled INT8 quantization
  discipline claimed here.

### 4.2 KAN Literature

- **Original KAN paper** : Liu, Z., et al. "KAN : Kolmogorov-Arnold
  Networks." arXiv:2404.19756 (May 2024). Theoretical introduction ;
  reference implementation in PyTorch ; CPU-and-CUDA training. Not a
  per-fragment GPU shader evaluator and not deployed for graphics.
- **KAN follow-ups** : "Kolmogorov-Arnold Transformer" (KAT, 2024-08),
  "Free-Knot KAN" (2024-09), "Wavelet-KAN" (2024-09), various extensions.
  All retain a CPU/training-time orientation. No production-graphics
  deployment.
- To the inventor's knowledge as of disclosure date, no published paper
  or shipped product describes a per-fragment GPU runtime evaluator for
  KAN networks suitable for the AAA / VR rendering budget.

### 4.3 Cooperative-Matrix Hardware Extensions

The following hardware/API extensions enable matrix-multiply-accumulate
operations from inside shader code, with sub-microsecond per-call latency :

- **NVIDIA Tensor Cores** (Volta 2017+ ; 4th-gen on RTX-50). Exposed via
  `wmma.h`, the cooperative-groups API, or via Vulkan
  VK_KHR_cooperative_matrix.
- **AMD Wave-Matrix-Multiply-Accumulate (WMMA)** : RDNA-3 (2022+),
  RDNA-4 expected. Tile (M=16, N=16, K=8) at FP16/INT8.
- **Intel Xe Matrix Extensions (XMX)** : Xe-HPG (Arc Alchemist 2022+),
  Xe-2/Xe-3.
- **Apple-Silicon simdgroup_matrix** : Metal-3 on M3+ (2023+). Tile
  (8, 8, 8) at FP16.
- **Qualcomm Adreno cooperative-matrix** : Adreno 8x series (Quest-3
  baseline, 2023+).

Libraries such as cuBLAS, rocBLAS, oneMKL, and MIOpen use these
extensions for general matrix algebra outside of shading. The present
invention's use *inside* a per-fragment shading primitive at sub-100ns
budget per evaluation is novel.

### 4.4 Quantization in Shading

- **Texture compression** : BC1 through BC7, ASTC, ETC2, PVRTC. These
  compress *static* texture data, not runtime-evaluated functions.
- **Neural-network quantization** : INT8 post-training quantization,
  per-channel scaling, mixed-precision (FP16/BF16/FP8). Documented in
  TensorRT, ONNX-Runtime, and similar inference frameworks. The present
  invention's *per-axis* scaling for spline control-points (preserving
  spline fidelity by scaling each input axis independently rather than
  per-tensor or per-channel) is, to the inventor's knowledge, not
  documented in the production-graphics literature.

### 4.5 Specific Deficiencies Addressed

- Existing neural-shading approaches use MLPs, which require larger
  parameter counts to fit BRDF-class functions and do not benefit from
  the spline-edge structure that allows aggressive per-axis quantization.
- Existing cooperative-matrix uses target full-matrix algebra at
  millisecond-class budgets, not per-fragment evaluations at 50-100 ns.
- Existing quantization schemes are per-tensor or per-channel ;
  per-axis-on-spline-input scaling is not standard and does not appear
  in production-graphics quantization literature.
- Existing neural-shading does not specify a tier-selection discipline
  that adapts at runtime to the GPU's capabilities while compiling away
  the dispatch decision into a single specialized path (no per-frame
  branching on hardware capability).
- Existing approaches load network weights from global memory per-fragment
  or per-tile, incurring memory bandwidth that the persistent-tile
  residency claimed here eliminates.

---

## 5. Summary of the Invention

The present invention is a **per-fragment GPU evaluator for Kolmogorov-Arnold-Network
spline functions** designed to fit within a 50-100 nanosecond budget per
evaluation per band on consumer-grade GPUs, suitable for direct
incorporation as a programmable-shading primitive in real-time rendering
pipelines.

The inventive step lies in the multiplicative composition of five sub-novelties :

**Novelty A (KAN-as-shading-primitive)** : The invention deploys a
Kolmogorov-Arnold spline-network as a programmable-shading primitive
evaluated per-fragment per-band, replacing the multi-layer-perceptron
approach used in prior neural-shading work. The shape variants enumerated
at compile-time (spectral-BRDF, BRDF-params, micro-displacement,
material-property, iridescence-stack, fluorescence) are each fixed-shape
const-generic types so the cooperative-matrix tile-size is known at
shader-compile-time.

**Novelty B (three-tier dispatch with init-time selection)** : The
invention dispatches over three tiers : (1) cooperative-matrix (preferred,
50 ns/eval), (2) subgroup/wave-cooperative (fallback, 200 ns/eval), and
(3) per-thread scalar (low-end-only, 800 ns/eval ; refused on M7-class
hardware). The tier is selected once at engine initialization based on
detected GPU capability, then compiled away into a single specialized
dispatch path so no runtime branching occurs on the critical path.

**Novelty C (persistent-kernel tile residency)** : KAN weights are
loaded once into GPU shared memory (LDS / threadgroup memory) at
compute-tile launch and remain resident across multiple frames,
amortizing the load cost across all shading-event invocations within
the tile lifetime. Tile eviction occurs only on KAN-network hot-swap
(e.g., when an online-trained material's coefficients atomic-swap
into place). The persistent-tile mechanism saves approximately 70%
of the load cost compared to per-fragment or per-tile reload.

**Novelty D (per-axis-scaled INT8 quantization preserving spline
fidelity)** : The invention uses INT8 storage of spline control-points
with a per-axis FP16 scale factor — that is, each spline-axis (one of
the network's input dimensions) carries its own scale, computed
post-training as (max-absolute-value of weights[axis] / 127). This
preserves spline-fidelity (axis-saturation does not occur) while
yielding 4× memory reduction over FP16 baseline. The fidelity gate
is bounded : L∞-error(INT8+S, FP32) ≤ 2⁻⁶ per output band, enforced
post-quantization at training time.

**Novelty E (compile-time density-budget enforcement)** : The
invention's shading-primitive dispatch is gated by a compile-time
density-budget pass that refuses compilation if the per-frame KAN
budget exceeds 2.5 ms (the canonical M7 allocation). Specifically,
the `density_check<5e6>` annotation on the shading entry-point is
verified against the KAN cost model at compile time ; an over-budget
shape mismatches the budget contract and emits a compile-error
rather than a runtime stutter.

The combination of these five sub-novelties yields a KAN evaluator
that hits 50-100 ns per band on cooperative-matrix-capable hardware,
fits 16-band hyperspectral evaluation within ~0.63 ms per frame at
1080p with foveated rendering, and is the foundational primitive
underneath three downstream inventions (the hyperspectral KAN-BRDF,
the sub-pixel fractal-tessellation amplifier, and the recursive-witness
KAN-confidence evaluator).

---

## 6. Detailed Description

### 6.1 KAN Theoretical Background

A Kolmogorov-Arnold-Network with input dimension I and output
dimension O computes :

```
y_o = Σ_{i=1..I} Σ_{l=1..L} φ_{l,i,o}(x_i)
```

where φ_{l,i,o} is a learnable spline function on edge (i, o) at
layer l. Standard KANs use B-spline basis with cubic order and a
small number of knots (8-12). Compared to MLP, the per-edge spline
provides higher modeling capacity per parameter for smooth functions,
with the spline structure permitting aggressive per-axis quantization.

### 6.2 KAN Shape Variants (Canonical Runtime Shapes)

The invention enumerates the shape variants that occur in graphics
pipelines :

| Variant            | Input | Hidden | Splines | Output | Use                       |
|--------------------|-------|--------|---------|--------|---------------------------|
| spectral-BRDF      | 32    | 2      | 16      | 16     | Hyperspectral BRDF λ      |
| BRDF-params        | 32    | 2      | 16      | 4      | (R, ρ, F0, anisotropy)    |
| micro-displacement | 7     | 3      | 32      | 1      | Sub-pixel fractal         |
| material-property  | 32    | 2      | 16      | 1-3    | Density / cond / etc.     |
| iridescence-stack  | 33    | 3      | 32      | 16     | Thin-film angle × λ       |
| fluorescence       | 17    | 2      | 16      | 16     | Absorption → emission λ   |

Each variant is a distinct compile-time const-generic instantiation, so
the cooperative-matrix tile-size is known when the shader is compiled.
Reflection-driven dispatch is forbidden : the shape enumeration must
fit in a small finite set known at link-time.

### 6.3 Three-Tier Dispatch

At engine initialization, a CPU-side function selects the dispatch tier :

```
fn select_kan_dispatch(gpu: &GpuInfo) -> KanDispatch {
    match gpu.capability {
        cap if cap.has(VK_KHR_cooperative_matrix)
            || cap.has(D3D12_SM69_COOP_VEC)
            || cap.has(MTL3_SIMDGROUP_MATRIX) => KanDispatch::CoopMatrix,
        cap if cap.wave_size >= 32 && cap.shared_mem_kb >= 32 => KanDispatch::SimdWarp,
        _ if gpu.profile == GpuProfile::LowEnd => KanDispatch::Scalar,
        _ => return Err(GpuTooWeak),
    }
}
```

The selected tier is fixed at init and not renegotiated per frame. The
shader is compiled with the selected tier as a specialization constant,
so the runtime code has no branch on capability.

#### 6.3.1 Tier-1 : Cooperative-Matrix

The KAN layer's input × edge-weight tensor multiplication is mapped to
the GPU's cooperative-matrix tile shape. Tile shapes per vendor :

| Vendor / Arch       | Tile (M×N×K) | Format            | Notes                           |
|---------------------|--------------|-------------------|---------------------------------|
| NVIDIA RTX-50       | 16×16×16     | FP16/INT8/FP8     | Tensor-Core 4th-gen             |
| NVIDIA RTX-40       | 16×16×16     | FP16/INT8         | Tensor-Core 3rd-gen             |
| AMD RDNA-4          | 16×16×8      | FP16/INT8         | WMMA                            |
| AMD RDNA-3          | 16×16×8      | FP16              | WMMA, requires-W32-mode         |
| Intel Xe-2/Xe-3     | 16×16×16     | FP16/INT8         | XMX, subgroup-cooperative       |
| Apple-Silicon M3+   | 8×8×8        | FP16              | simdgroup_matrix                |
| Qualcomm Adreno 8x  | 16×16×8      | FP16/INT8         | Quest-3/Quest-4 baseline        |

A KAN-layer logical-shape (IN=32, OUT=16) tiles into (M=16, N=16) blocks,
yielding 2 × 1 = 2 cooperative-matrix-ops per layer at Quest-3 class,
matching the 50 ns/layer tier-1 budget. Output dimensions smaller than
the tile (e.g., OUT=4 for BRDF-params) zero-pad to tile size with the
zero-padded outputs discarded post-evaluation.

#### 6.3.2 Tier-2 : SIMD-Warp Cooperative

When cooperative-matrix is unavailable but the GPU supports a wave size
of at least 32 and shared memory of at least 32 KB, the invention falls
back to a subgroup/wave-cooperative path using `subgroupShuffleXor`
(GLSL) or `WaveReadLaneAt` (HLSL) primitives across the OUT dimension.
Target : 200 ns/eval/band.

#### 6.3.3 Tier-3 : Scalar Per-Thread

For low-end profiles only, the invention provides a scalar dispatch
that evaluates the KAN sequentially per-thread. Target : 800 ns/eval/band.
This tier is refused on M7-class hardware by a compile-time gate ; only
hardware explicitly tagged GpuProfile::LowEnd may enable it.

### 6.4 Shader-Side Primitives

The invention exposes a three-level primitive API at the shader stage :

#### 6.4.1 Level-1 : Per-Axis B-Spline Evaluation

```
fn kan_evaluate_spline<const KNOTS: u32>(
    weights: &[f16; KNOTS + 4],
    input: f16,
    axis_index: u32,
) -> f16
{
    let span = find_knot_span(input, KNOTS);
    let basis = b_spline_basis_cubic(input, span);
    let mut acc: f16 = 0.0;
    #[unroll]
    for j in 0..4 {
        acc += weights[span + j] * basis[j];
    }
    acc
}
```

The `find_knot_span` is implemented as a fully-unrolled binary search at
8 knots, and `b_spline_basis_cubic` is the Cox-de-Boor recurrence with 4
multiply-adds, yielding ~10 ns total per spline evaluation at tier-1.

#### 6.4.2 Level-2 : Per-Layer Forward (N-output)

```
fn kan_layer_forward<const IN: u32, const OUT: u32, const KNOTS: u32>(
    layer: &KanLayer<IN, OUT, KNOTS>,
    input: &[f16; IN],
    scratch: &mut SharedScratch<OUT>,
)
{
    match KAN_DISPATCH_TIER {
        Tier::CoopMatrix => coop_matrix_layer_forward(layer, input, scratch),
        Tier::SimdWarp   => simd_warp_layer_forward(layer, input, scratch),
        Tier::Scalar     => scalar_layer_forward(layer, input, scratch),
    }
}
```

The match on the dispatch tier is a specialization constant so it
compiles away to a single direct call.

#### 6.4.3 Level-3 : Full-Network Forward with Ping-Pong Scratch

```
fn kan_network_forward<NET: KanNetwork>(
    net: &NET,
    input: &[f16; NET::INPUT_DIM],
    output: &mut [f16; NET::OUTPUT_DIM],
)
{
    let mut buf_a = SHARED_SCRATCH_A.subspan::<NET::MAX_HIDDEN_DIM>();
    let mut buf_b = SHARED_SCRATCH_B.subspan::<NET::MAX_HIDDEN_DIM>();

    kan_layer_forward(&net.layer_0, input, &mut buf_a);
    let (mut src, mut dst) = (&buf_a, &mut buf_b);
    #[unroll]
    for l in 1..NET::HIDDEN_LAYERS {
        kan_layer_forward(&net.layers[l], src, dst);
        swap(&mut src, &mut dst);
    }
    kan_layer_forward(&net.layer_out, src, output);
}
```

Two ping-pong scratch buffers in shared memory eliminate global-memory
bounce between layers. At a typical tile configuration (32 × 8 threads ×
~64 bytes per thread = 16 KB per tile), the scratch fits within the
shared-memory budget.

### 6.5 Persistent-Kernel Tile Residency

A long-lived compute dispatch (multi-frame) holds the KAN weight tensor in
shared memory. Fragments stream through the tile via a work queue. The
tile is evicted only on KAN hot-swap, which is rare (e.g., M9+ online
training of material networks). The amortization table :

| Strategy                  | Setup-cost / frame    | Saved      |
|---------------------------|-----------------------|------------|
| Naive : reload per-frag   | 1200 ns × N-fragments | baseline   |
| Reload per-tile           | 800 ns / tile         | 33% saved  |
| Persistent-tile (THIS)    | 0 ns (resident)       | 70% saved  |

Constraints on persistent-tile :
- KAN shape stable across frames (any change triggers tile rebuild).
- Shared-memory budget ≤ 48 KB/tile (RTX-50 / Quest-4 cap).
- Tile occupancy ≥ 50% (else fall back to per-frame dispatch).

### 6.6 Quantization Discipline

The invention defines a precision-tier table :

| Tier      | Format                       | Use                       | Cost       |
|-----------|------------------------------|---------------------------|------------|
| FP32      | IEEE-754 single              | Offline-training only     | High       |
| FP16      | IEEE-754 half (BASELINE)     | Runtime default           | Medium     |
| INT8+S    | INT8 + per-axis FP16 scale   | Low-end GPU profile       | Low        |
| FP8 (e4m3)| OCP FP8 (RTX-50+)            | Experimental @ M9         | Low        |

FP32 is FORBIDDEN at runtime-shader (grep-gate at compile pipeline).
INT8+S is per-axis-scaled, not per-tensor. Per-edge weight initialization
is Glorot-balanced offline. Runtime quantization computes
scale = max-absolute-value(weights[axis]) / 127, ensuring no axis saturation
at INT8.

The quantization fidelity gate :
- L∞-error(FP16, FP32) ≤ 2⁻⁹ per output band — required.
- L∞-error(INT8+S, FP32) ≤ 2⁻⁶ per output band — required.

Both checks are enforced at training time post-quantization.

### 6.7 Compile-Time Density-Budget Enforcement

The invention's primary shading-entry-point carries an annotation
`@compute_pass @density_check<5e6>` (for "5 × 10⁶ visible cells at M7
target") and a deadline annotation `@deadline<3ms>`. The KAN cost model
(see § 6.8 below) is consulted at compile time : if the configured
shape's per-frame cost exceeds the deadline, the compile fails.

### 6.8 Cost Model

For canonical 16-band-spectral evaluation at 1080p :

| Tier       | Per-band ns | × 16 bands | Per-pixel | × 1080p (2.07M px) | / frame  |
|------------|-------------|------------|-----------|--------------------|----------|
| CoopMatrix | 50          | 800        | 800 ns    | 1.66 ms            | ✓        |
| SIMD-warp  | 200         | 3200       | 3.2 µs    | 6.62 ms            | ◐        |
| Scalar     | 800         | 12800      | 12.8 µs   | 26.5 ms            | ✗ over   |

Foveated rendering reduces effective pixel count to ~38% of naive,
yielding (CoopMatrix tier) a foveated 1080p frame cost of 0.63 ms.

Total KAN budget allocation at M7 : ≤ 2.5 ms / frame. Decomposition
(foveated CoopMatrix tier) :

| Pass                         | Cost    | % frame |
|------------------------------|---------|---------|
| Hyperspectral-BRDF (16-band) | 0.63 ms | 5.7%    |
| BRDF-params (4-channel)      | 0.18 ms | 1.6%    |
| Sub-pixel-fractal (fovea)    | 0.42 ms | 3.8%    |
| Iridescence (anisotropy>τ)   | 0.20 ms | 1.8%    |
| Fluorescence (Λ-tokens)      | 0.10 ms | 0.9%    |
| Total KAN-budget             | 1.53 ms | 13.8%   |

### 6.9 Degradation Order

When KAN-eval exceeds 2.5 ms / frame, the runtime degradation order is
deterministic :

| Detected condition           | Degradation order      | Visual impact        |
|------------------------------|------------------------|----------------------|
| KAN-eval > 2.5 ms / frame    | 1. drop iridescence    | Minor : aniso flat   |
|                              | 2. drop fluorescence   | Minor : Λ flat       |
|                              | 3. half-rate fovea-disp| Moderate : detail blur|
|                              | 4. drop spectral→4-band| Significant : wrong λ|
| Quest-3 / 8GB-VRAM pressure  | shrink tile to 32K     | Minor : spline coarsen|
| RC-cascade also over-budget  | shed mid-region KAN    | Major : flat-mids    |

---

## 7. Drawings Needed

- **Figure 1** : System-overview diagram of the KAN-runtime evaluator
  showing init-time tier selection feeding into compiled dispatch path.
- **Figure 2** : The three-tier dispatch hierarchy with per-tier
  performance budgets and capability requirements.
- **Figure 3** : Cooperative-matrix tile-shape mapping for the 32 × 16
  spectral-BRDF KAN layer onto Quest-3 (Adreno 8x) 16 × 16 × 8 tiles.
- **Figure 4** : Persistent-tile residency timeline showing shader
  compute-tile launching once and remaining resident across multiple
  frames, with hot-swap atomic transition shown at frame boundary.
- **Figure 5** : Per-axis INT8+S quantization schematic showing each input
  axis's weights independently scaled, and the resulting bounded
  L∞-error post-quantization.
- **Figure 6** : Ping-pong scratch buffer diagram for the level-3
  network-forward primitive showing shared-memory residency through
  layers.
- **Figure 7** : The compile-time density-budget gate showing the cost
  model rejecting an over-budget KAN-shape configuration.

---

## 8. Claims (Initial Draft for Attorney Review)

### Independent Method Claim

**Claim 1.** A computer-implemented method for evaluating a
Kolmogorov-Arnold-Network spline function during the rendering of each
fragment of a graphics frame, the method comprising :

- (a) at engine initialization, detecting one or more capabilities of a
  graphics-processing unit, said capabilities including the presence of
  a cooperative-matrix instruction extension, a wave or subgroup size,
  and an available shared-memory budget ;
- (b) selecting, based on said detected capabilities, one of a plurality
  of evaluation paths comprising at least a cooperative-matrix path, a
  subgroup-cooperative path, and a per-thread scalar path ;
- (c) compiling a shader specializing on the selected evaluation path
  such that the shader contains no runtime branch on said capability ;
- (d) loading, into shared memory of the GPU, a set of spline-network
  weight tensors corresponding to a Kolmogorov-Arnold-Network with
  fixed compile-time-known input dimension I and output dimension O,
  said weights remaining resident in said shared memory across multiple
  rendering frames ;
- (e) for each fragment of the rendering frame, evaluating the
  spline-network on input data derived from the fragment's geometric
  and material state, by composing per-edge spline evaluations across
  layers using ping-pong scratch buffers in said shared memory ;
- (f) emitting per-fragment outputs corresponding to the spline-network
  output dimension for use in shading the fragment.

### Dependent Claims

**Claim 2.** The method of Claim 1, wherein the spline-network weights
are stored in INT8 format with one FP16 scale factor per input axis,
and wherein the per-axis scale factor is computed as the maximum
absolute value of the weights for that axis divided by 127.

**Claim 3.** The method of Claim 2, wherein a quantization-fidelity
gate is enforced at training time requiring an L∞-error between the
INT8-with-per-axis-scale representation and the floating-point baseline
at most 2⁻⁶ per output band.

**Claim 4.** The method of Claim 1, wherein the spline-network is one
of a plurality of compile-time enumerated shape variants, said variants
including at least a 32-input 16-output spectral-BRDF variant, a
7-input 1-output micro-displacement variant, a 33-input 16-output
iridescence variant, and a 17-input 16-output fluorescence variant.

**Claim 5.** The method of Claim 1, wherein the persistent-tile
residency of step (d) saves at least 70% of the total shared-memory
load cost compared to per-fragment or per-tile reload, and wherein
tile eviction occurs only on a network-hot-swap event in which a new
weight tensor atomically replaces the resident tensor at a frame
boundary.

**Claim 6.** The method of Claim 1, further comprising a compile-time
density-budget pass that refuses compilation if the per-frame cost of
the spline-network evaluation, as estimated by a cost model, exceeds a
configured deadline.

**Claim 7.** The method of Claim 1, wherein the cooperative-matrix path
of step (b) maps the spline-network's per-layer input × edge-weight
tensor product onto the GPU's cooperative-matrix tile shape, with
zero-padding applied when the layer dimension is smaller than the
tile dimension.

**Claim 8.** The method of Claim 1, wherein the per-edge spline
function is a B-spline of cubic order with a knot count between 8 and
12, and the per-axis evaluation is implemented by a fully-unrolled
Cox-de-Boor recurrence.

**Claim 9.** The method of Claim 1, performed on consumer-grade
graphics-processing-unit hardware at a sustained frame rate of at least
60 Hz with a per-evaluation budget of at most 100 nanoseconds per band
on the cooperative-matrix path.

**Claim 10.** The method of Claim 1, wherein step (b) returns an error
when the GPU does not satisfy any of the path-capability requirements,
and wherein the runtime refuses to bootstrap rather than silently
falling back to a degraded path.

### Independent System Claim

**Claim 11.** A computer system for real-time graphics rendering, the
system comprising :

- a graphics-processing unit ;
- an initialization unit that detects GPU capabilities and selects an
  evaluation path from at least a cooperative-matrix path, a
  subgroup-cooperative path, and a per-thread scalar path ;
- a shader compiler that specializes a fragment shader on the selected
  evaluation path ;
- a shared-memory tile that holds Kolmogorov-Arnold-Network weight
  tensors resident across multiple rendering frames ;
- a per-fragment evaluator that performs ping-pong scratch-buffer
  evaluation of the network using compile-time-known input and output
  dimensions ;
- a hot-swap unit that atomically replaces the resident weight tensor
  at frame boundaries when a new tensor is uploaded.

### Dependent System Claims

**Claim 12.** The system of Claim 11, wherein the shared-memory tile
holds weights in INT8 format with per-axis FP16 scale factors, and
wherein the per-fragment evaluator reads INT8-with-scale and emits
FP16 outputs.

**Claim 13.** The system of Claim 11, further comprising a degradation
controller that, upon detecting that the per-fragment evaluator's
cumulative frame cost exceeds a budget, sheds work in a deterministic
order beginning with iridescence evaluation, then fluorescence
evaluation, then sub-pixel-displacement, then spectral band reduction.

### Computer-Readable-Medium Claim

**Claim 14.** A non-transitory computer-readable medium storing
instructions that, when executed by one or more processors of a
computing system having a graphics-processing unit, cause the system
to perform the method of any one of Claims 1 through 10.

---

## 9. Embodiments

### 9.1 Default Embodiment — M7-Class Consumer VR

- 16-band hyperspectral BRDF, 7-input micro-displacement, optional
  iridescence/fluorescence networks.
- Cooperative-matrix dispatch on Quest-3+, RTX-30+, M3+.
- Persistent-tile budget : 48 KB shared memory per tile, 50% occupancy
  minimum.
- Total KAN budget : 2.5 ms per frame at 90 Hz.

### 9.2 Reduced Embodiment — Mobile / Low-End

- 4-band reduced BRDF, no iridescence, no fluorescence.
- SIMD-warp dispatch fallback ; scalar dispatch for legacy Adreno 6xx.
- Reduced shape variants only.

### 9.3 Extended Embodiment — Desktop High-End

- 32-band ultra-spectral BRDF + full iridescence + fluorescence.
- Cooperative-matrix at FP8 precision (RTX-50, Apple M5+).
- Multi-tile persistent residency with cross-tile work-stealing.

### 9.4 Inverse-Rendering / Training-Mode Embodiment

- Forward-mode automatic-differentiation Jet<f32, N> evaluation paths
  added at compile time when training is enabled.
- Training kernel uses the same KAN evaluator but with autodiff tape
  recording.

### 9.5 Generic Function-Approximation Embodiment

- The invention is not limited to graphics shading : the same
  per-fragment KAN evaluator can be applied to any GPU compute task
  that benefits from a small spline-network function approximation
  evaluated at high frequency, including signal processing, real-time
  control, scientific simulation, and audio synthesis.

---

## 10. Industrial Applicability

1. **Game engines** : Unreal Engine, Unity, Frostbite, Source-2,
   in-house first-party engines. Drop-in replacement for shader graph
   evaluation as well as for MLP-based neural shading plugins.
2. **VR/AR/MR headsets** : Meta Quest, Apple Vision Pro, ByteDance
   Pico, HTC Vive, Valve Index. The persistent-tile residency and
   cooperative-matrix paths are particularly valuable for mobile-class
   VR hardware where memory bandwidth is at a premium.
3. **Real-time scientific visualization** : where high-dimensional
   continuous functions (e.g., spectroscopic responses, material
   property functions, optical property functions) must be evaluated
   per-pixel.
4. **Differentiable rendering / inverse rendering** : where the KAN
   evaluator is used both forward (rendering) and backward (gradient
   computation) within the same code base.
5. **Neural-network compression for mobile inference** : the per-axis
   INT8+S quantization discipline transfers to non-graphics neural
   networks where spline-edge structure exists.

---

## 11. Reference Implementation in CSSLv3

### 11.1 Spec Anchors

- **Primary specification** :
  `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING.csl.md`
  Sections § I (Claim) ; § II (KAN-network shape — runtime canonical) ;
  § III (GPU evaluation paths — three-tier dispatch) ; § IV (Shader-side
  primitives — three levels) ; § V (Persistent-kernel KAN-residency) ;
  § VI (Quantization discipline) ; § VII (Cost model) ; § VIII
  (Hyperspectral-KAN-BRDF mapping) ; § IX (Sub-pixel-fractal-tessellation
  amplifier) ; § X (Call-site integration) ; § XI (Acceptance test —
  Quest-3-class GPU).
- **Mathematical primitives axiom** :
  `Omniverse/01_AXIOMS/10_OPUS_MATH.csl.md` (KAN as one of six primitives).
- **Density-budget axiom** :
  `Omniverse/01_AXIOMS/13_DENSITY_SOVEREIGNTY.csl.md`.

### 11.2 Source-Code Reference Implementation

- **Substrate runtime crate** :
  `compiler-rs/crates/cssl-substrate-kan/`
  - `src/lib.rs` : crate-level documentation.
  - `src/kan_network.rs` (lines 1-100+) : `KanNetwork<I, O>`
    const-generic shape, `KAN_LAYERS_MAX = 8`, `KAN_CTRL = 16`,
    `KAN_KNOTS = 32`, `KAN_EDGE_MAX = 64`, `SplineBasis` enum
    (BSpline / CatmullRom / Cubic), forward-evaluation entry-point.
  - `src/kan_material.rs` : `KanMaterial` variants enumerating the
    shape-variant table (spectral-BRDF, BRDF-params, micro-displacement,
    material-property, iridescence-stack, fluorescence,
    physics-impedance for the wave-solver).
  - `src/kan_genome_weights.rs` : ancestry-aware weight storage.
  - `src/pattern.rs` + `src/phi_table.rs` + `src/pool.rs` :
    Φ-pattern-pool with 256-bit blake3 fingerprint for byte-stable
    KAN identity.
  - `src/handle.rs` : `Handle<Phi>` typed reference to the pattern pool.
  - `Cargo.toml` lines 1-28 : crate manifest.

- **Downstream consumer (BRDF)** :
  `compiler-rs/crates/cssl-spectral-render/src/kan_brdf.rs`
  (lines 1-120) : `KanBrdfEvaluator` using `KanMaterial::spectral_brdf<16>`,
  the canonical BRDF call-site of § VIII.

- **Downstream consumer (fractal amplifier)** :
  `compiler-rs/crates/cssl-fractal-amp/src/amplifier.rs` (lines 1-120) :
  `FractalAmplifier` wrapping three KAN-networks (`KanNetwork<7, 1>` for
  micro-displacement, `KanNetwork<7, 1>` for micro-roughness,
  `KanNetwork<7, 3>` for micro-color-perturbation), the canonical
  fractal-amplifier call-site of § IX.

- **Downstream consumer (wave-solver impedance)** :
  `compiler-rs/crates/cssl-wave-solver/src/coupling.rs` :
  reads `KanMaterial::physics_impedance` for cross-band coupling tensor
  evaluation.

### 11.3 Reduction-to-Practice Commits

- `7a3646d` : `§ T11-D143 : cssl-substrate-kan — KAN substrate runtime
  + Φ-pattern-pool` (substrate runtime).
- `162ef07` : `§ T11-D118 : cssl-spectral-render Stage-6 hyperspectral
  KAN-BRDF crate` (downstream BRDF consumer).
- `b6287fd` : `§ T11-D119 : Stage-7 Sub-Pixel Fractal-Tessellation
  Amplifier (cssl-fractal-amp NEW crate)` (downstream amplifier consumer).
- `707761b` : `§ T11-D114 — cssl-wave-solver crate : Wave-Unity
  multi-rate ψ-field solver` (downstream impedance consumer).

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

∎ End of Invention 2 Disclosure.
