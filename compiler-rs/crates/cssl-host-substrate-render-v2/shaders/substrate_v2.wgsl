// § substrate_v2.wgsl — RAW-COMPUTE substrate-render kernel.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-W18-L6-V2
//
// § THESIS
//
// The substrate-paradigm does NOT need rasterization. It needs :
//   observer-coord + crystal-list → per-pixel-ω-field-resonance → RGBA buffer
//
// v2 = single compute-pass writes pixels DIRECTLY to a storage-texture.
//   · NO render-pipeline · NO vertex-shader · NO fragment-shader
//   · NO rasterizer · NO depth-buffer · NO MSAA-resolve
//   · 1 compute-dispatch per frame · then 1 texture-blit to swapchain
//
// One thread = one pixel. Each thread :
//   1. Computes the observer-ray direction for its pixel (pinhole projection
//      with stage-0 small-angle yaw+pitch).
//   2. Walks the ray through the world in `RAY_SAMPLES` even steps to
//      `RAY_MAX_DIST_MM`.
//   3. At each sample, scans the storage-buffer crystals[] for any whose
//      bounding-sphere overlaps the sample's NEAR_RADIUS_MM.
//   4. Per contributing crystal, applies the Σ-mask filter (silhouette aspect
//      bit must be set on BOTH observer and crystal), evaluates the silhouette
//      spline, derives a distance-attenuated weight, and accumulates the
//      crystal's spectral-LUT × weight into a per-pixel 16-band spectrum.
//   5. Projects the accumulated spectrum through the observer's illuminant
//      blend → sRGB → writes RGBA8 to the storage texture.
//
// § BIND GROUP 0
//   binding 0  : ObserverUniform                        (uniform)
//   binding 1  : array<GpuCrystal>                      (read-only storage)
//   binding 2  : texture_storage_2d<rgba8unorm, write>  (output)
//
// § DETERMINISM
//   - Linear-scan crystal order = the crystals[] slice order (stable).
//   - Per-pixel spectral accumulation is integer saturating-add, associative
//     within a single thread's fixed iteration order.
//   - Σ-mask check is purely-additive (revoke = skip · grant = include).
//   - Same (observer, crystals, width, height) ⇒ same output texture, byte-
//     for-byte. The `per-frame-determinism` host test verifies this.
//
// § CONSENT (PRIME-DIRECTIVE)
//   Σ-mask bit 0 = silhouette permission. If either the observer or the
//   crystal has it cleared, the crystal contributes ZERO. There is no
//   fallback render-path that bypasses the mask.
//
// § PORT-PARITY WITH W18-G
//   The kernel is bit-for-bit equivalent to substrate_resonance.wgsl. We
//   keep the algorithm verbatim ; the architectural change is host-side
//   (compute-only pipeline · no render-pass · explicit swapchain-blit).
//   Reusing the algorithm guarantees identical pixel output between v1
//   (compute-+-render-pass) and v2 (compute-only-+-blit) for a given
//   (observer, crystals).

const RAY_SAMPLES        : u32 = 8u;
const RAY_MAX_DIST_MM    : i32 = 16000;
const NEAR_RADIUS_MM     : i32 = 1500;
const SPECTRAL_BANDS     : u32 = 16u;
const ILLUMINANT_COUNT   : u32 = 4u;
const ASPECT_SILHOUETTE  : u32 = 0u;
const Z_UNIT             : i32 = 1000;

// § T11-W18-CRYSTAL128 · maximum crystal-count compiled into the kernel.
//   Used as a hard-cap on the inner loop ; the actual per-frame count comes
//   from `observer.n_crystals` (always ≤ MAX_CRYSTALS by host-contract). The
//   cap protects against accidental run-aways and gives the optimizer an
//   upper bound to unroll against.
const MAX_CRYSTALS       : u32 = 128u;

// § T11-W18-CRYSTAL128 · early-exit threshold on accumulated weight. Once
//   weight_total crosses this value the per-pixel sRGB byte is already
//   nearly-saturated (projector divides by weight_total) ; further crystal
//   contributions barely shift the final color. Trades small fidelity in
//   the brightest fringe-cores for ~30-50% inner-loop reduction on those
//   pixels at N=128. The threshold is large enough that dim/dark pixels
//   never trigger early-exit (continuous integration over all crystals).
const EARLY_EXIT_AMP_THRESHOLD : u32 = 32768u;

// § T11-W18-WORKGROUP-CACHE · cooperative crystal-load across the 64-thread
//   8×8 workgroup. Every thread in the group needs the same crystal-list ;
//   loading via shared memory cuts L1$ pressure ~64×. 128-slot cache covers
//   any reasonable scene · spill to global beyond.
const WG_CACHE_SIZE      : u32 = 128u;
const WG_THREADS         : u32 = 64u;

// ════════════════════════════════════════════════════════════════════════════
// § STORAGE-BUFFER LAYOUT — must match cssl_host_substrate_resonance_gpu::
// buffer_pack::GpuCrystal exactly (host-side bytemuck::Pod struct).
// ════════════════════════════════════════════════════════════════════════════

struct GpuCrystal {
    world_pos    : vec3<i32>,
    extent_mm    : i32,
    sigma_mask   : u32,
    _pad0        : u32,
    _pad1        : u32,
    _pad2        : u32,
    spectral_lut : array<vec4<u32>, 4>,
    silhouette   : array<vec4<i32>, 16>,
};

struct ObserverUniform {
    pos_x_mm        : i32,
    pos_y_mm        : i32,
    pos_z_mm        : i32,
    sigma_mask      : u32,
    yaw_milli       : i32,
    pitch_milli     : i32,
    width           : u32,
    height          : u32,
    n_crystals      : u32,
    illuminant_blend: u32,
    _pad0           : u32,
    _pad1           : u32,
};

@group(0) @binding(0) var<uniform> observer    : ObserverUniform;
@group(0) @binding(1) var<storage, read> crystals : array<GpuCrystal>;
@group(0) @binding(2) var output : texture_storage_2d<rgba8unorm, write>;

// ════════════════════════════════════════════════════════════════════════════
// § T11-W18-SOA-PACK · 64-byte packed-crystal scaffold (FUTURE PATH).
// ════════════════════════════════════════════════════════════════════════════
//
// `GpuCrystalPacked` is the 64-byte equivalent of `GpuCrystal` shipped in
// W18-SOA-PACK. The host-side struct + pack functions are wired up ; the
// **shader-side** plumbing here is scaffold-only (struct + bit-extract
// helpers) — no entry-point currently uses these symbols, so the kernel
// behaviour is bit-for-bit identical to pre-SOA-PACK. The packed kernel
// path will be wired in a follow-up wave after the host-side telemetry
// confirms the visual delta is sub-perceptual at 1440p.
//
// LAYOUT (64 B · matches Rust `GpuCrystalPacked`)
//
//   ofs0 .. 11 : world_pos_x/y/z (vec3<i32>)
//   ofs12      : extent_mm + sigma + flags packed into ONE u32 :
//                  byte 0..1 = extent_mm_u16  (LE)
//                  byte 2    = sigma_mask_u8
//                  byte 3    = flags_u8
//   ofs16      : spectral_quad : array<u32, 4>  (16 bands × 1 byte)
//   ofs32      : silhouette_quant : array<u32, 8>  (32 i8 packed 4-per-u32)
//
// The total stride is `4 (vec3 = 12 bytes round to 16) + ...` — actually
// `vec3<i32>` in WGSL has 12 B but std430 rounds it to 16. We use four
// `i32` scalars to pin the layout to 12 B and place `extent_sigma_flags`
// at offset 12, exactly matching the host struct.

struct GpuCrystalPacked {
    world_pos_x        : i32,
    world_pos_y        : i32,
    world_pos_z        : i32,
    extent_sigma_flags : u32,
    spectral_quad      : array<vec4<u32>, 1>,  // 16 bytes
    silhouette_quant   : array<vec4<u32>, 2>,  // 32 bytes (8 × u32 = 32 i8 packed)
};

// Helper : extract `extent_mm` (u16 low) from the packed `extent_sigma_flags`.
fn unpack_extent_mm(packed: u32) -> u32 {
    return packed & 0xFFFFu;
}

// Helper : extract `sigma_mask` (byte 2) from the packed word.
fn unpack_sigma_mask(packed: u32) -> u32 {
    return (packed >> 16u) & 0xFFu;
}

// Helper : extract `flags` (byte 3) from the packed word.
fn unpack_flags(packed: u32) -> u32 {
    return (packed >> 24u) & 0xFFu;
}

// Helper : extract a single 1-byte spectral band (b in 0..16) from the
// `spectral_quad` array (4 × u32 = 16 bytes packed LE).
fn unpack_spectral_band(quad: vec4<u32>, band: u32) -> u32 {
    let word_idx = band / 4u;
    let byte_idx = band & 3u;
    var word : u32;
    if      (word_idx == 0u) { word = quad.x; }
    else if (word_idx == 1u) { word = quad.y; }
    else if (word_idx == 2u) { word = quad.z; }
    else                     { word = quad.w; }
    return (word >> (byte_idx * 8u)) & 0xFFu;
}

// Helper : extract a single sign-extended i32 from packed silhouette i8
// quant. `point_idx` 0..16 · `axis_idx` 0..2.
fn unpack_silhouette_axis(silhouette: array<vec4<u32>, 2>, point_idx: u32, axis_idx: u32) -> i32 {
    // Total i8 index = point_idx * 2 + axis_idx (range 0..32).
    let i8_idx = point_idx * 2u + axis_idx;
    // Pack/unpack : 8 u32 hold 32 i8 = 4 × i8 / u32. Choose the right u32.
    let word_idx = i8_idx / 4u;
    let byte_idx = i8_idx & 3u;
    var word : u32;
    if      (word_idx == 0u) { word = silhouette[0].x; }
    else if (word_idx == 1u) { word = silhouette[0].y; }
    else if (word_idx == 2u) { word = silhouette[0].z; }
    else if (word_idx == 3u) { word = silhouette[0].w; }
    else if (word_idx == 4u) { word = silhouette[1].x; }
    else if (word_idx == 5u) { word = silhouette[1].y; }
    else if (word_idx == 6u) { word = silhouette[1].z; }
    else                     { word = silhouette[1].w; }
    let raw = (word >> (byte_idx * 8u)) & 0xFFu;
    // Sign-extend i8 → i32.  (`signed` is a reserved WGSL keyword ; use a
    // different name.)
    var extended_i32 : i32 = i32(raw);
    if ((raw & 0x80u) != 0u) {
        extended_i32 = extended_i32 - 256;
    }
    return extended_i32;
}

// § T11-W18-SOA-PACK · _SCAFFOLD_REFERENCE keeps the helpers from being
// pruned by naga DCE. Returning a dummy boolean derived from all helpers
// forces the validator to include them in the module ; the entry-point
// can OPT-IN later by replacing its inner crystal-loop with the packed
// path. The function is unused → compiled-out at runtime.
fn _packed_scaffold_reference(p: GpuCrystalPacked) -> bool {
    let e = unpack_extent_mm(p.extent_sigma_flags);
    let s = unpack_sigma_mask(p.extent_sigma_flags);
    let f = unpack_flags(p.extent_sigma_flags);
    let b = unpack_spectral_band(p.spectral_quad[0], 0u);
    let sa = unpack_silhouette_axis(p.silhouette_quant, 0u, 0u);
    return (e | s | f | b) > 0u || sa != 0;
}

// § T11-W18-WORKGROUP-CACHE · the 64 threads of an 8×8 workgroup cooperatively
//   load the first WG_CACHE_SIZE crystals into shared memory. After a barrier
//   ALL threads scan the cache · cache-resident reads cost ~1 cycle vs ~100
//   for global storage-buffer reads on Intel-Arc/AMD-RDNA · expect ~10-30×
//   speedup on the inner crystal-loop when n_crystals ≤ 128.
var<workgroup> wg_cache : array<GpuCrystal, WG_CACHE_SIZE>;
var<workgroup> wg_cached_count : u32;

// ════════════════════════════════════════════════════════════════════════════
// § Helper functions (all integer, all deterministic).
// ════════════════════════════════════════════════════════════════════════════

fn observer_permits_silhouette() -> bool {
    return (observer.sigma_mask & 1u) != 0u;
}

fn crystal_permits_silhouette(c: GpuCrystal) -> bool {
    return (c.sigma_mask & 1u) != 0u;
}

fn dist_sq_mm_i64(ax: i32, ay: i32, az: i32, bx: i32, by: i32, bz: i32) -> i32 {
    let dx = ax - bx;
    let dy = ay - by;
    let dz = az - bz;
    return dx * dx + dy * dy + dz * dz;
}

fn pixel_direction(px: u32, py: u32) -> vec3<i32> {
    let nx = i32(px) * 2 - i32(observer.width);
    let ny = i32(observer.height) - i32(py) * 2;
    let z_unit = Z_UNIT;
    let x_unit = nx;
    let y_unit = ny;
    let cos_y : i32 = 1000;
    let sin_y : i32 = clamp(observer.yaw_milli, -1000, 1000);
    let xr = (x_unit * cos_y - z_unit * sin_y) / 1000;
    let zr = (x_unit * sin_y + z_unit * cos_y) / 1000;
    let cos_p : i32 = 1000;
    let sin_p : i32 = clamp(observer.pitch_milli, -1000, 1000);
    let yr  = (y_unit * cos_p - zr * sin_p) / 1000;
    let zr2 = (y_unit * sin_p + zr * cos_p) / 1000;
    return vec3<i32>(xr, yr, zr2);
}

fn ray_sample_world(dir: vec3<i32>, i: u32) -> vec3<i32> {
    let step_mm = RAY_MAX_DIST_MM / i32(RAY_SAMPLES + 1u);
    let i_plus_1 = i32(i) + 1;
    let off_x = (dir.x * step_mm * i_plus_1) / 1000;
    let off_y = (dir.y * step_mm * i_plus_1) / 1000;
    let off_z = (dir.z * step_mm * i_plus_1) / 1000;
    return vec3<i32>(
        observer.pos_x_mm + off_x,
        observer.pos_y_mm + off_y,
        observer.pos_z_mm + off_z,
    );
}

fn silhouette_at_angle(c: GpuCrystal, yaw: u32, pitch: u32) -> i32 {
    let t = (yaw + pitch) % 1000u;
    var w0 : u32 = (yaw & 0xFFu);
    if (w0 < 64u) { w0 = 64u; }
    let w1 : u32 = (yaw >> 8u) & 0xFFu;
    let w2 : u32 = pitch & 0xFFu;
    let w3 : u32 = (pitch >> 8u) & 0xFFu;
    let wsum : i32 = max(i32(w0 + w1 + w2 + w3), 1);
    let seg : u32 = (t * 15u) / 1000u;
    let seg_clamped : u32 = min(seg, 14u);
    let local_t : i32 = i32(t * 15u) - i32(seg) * 1000;
    let local_t_clamped : i32 = clamp(local_t, 0, 1000);
    let inv_t : i32 = 1000 - local_t_clamped;
    let a = c.silhouette[seg_clamped];
    let b = c.silhouette[seg_clamped + 1u];
    var acc : i32 = 0;
    let weights = vec4<i32>(i32(w0), i32(w1), i32(w2), i32(w3));
    let mid0 = (a.x * inv_t + b.x * local_t_clamped) / 1000;
    let mid1 = (a.y * inv_t + b.y * local_t_clamped) / 1000;
    let mid2 = (a.z * inv_t + b.z * local_t_clamped) / 1000;
    let mid3 = (a.w * inv_t + b.w * local_t_clamped) / 1000;
    acc = mid0 * weights.x + mid1 * weights.y + mid2 * weights.z + mid3 * weights.w;
    let raw = abs(acc / wsum);
    let clamped = clamp(raw, 0, 32767);
    let scaled = (clamped * c.extent_mm) / 32768;
    return clamp(scaled, 0, c.extent_mm);
}

fn spectral_byte(c: GpuCrystal, ill: u32, band: u32) -> u32 {
    let word_idx = band / 4u;
    let byte_idx = band & 3u;
    let v4 = c.spectral_lut[ill];
    var word : u32;
    if (word_idx == 0u)      { word = v4.x; }
    else if (word_idx == 1u) { word = v4.y; }
    else if (word_idx == 2u) { word = v4.z; }
    else                     { word = v4.w; }
    return (word >> (byte_idx * 8u)) & 0xFFu;
}

fn project_blended_to_srgb(spec_acc: array<u32, 16>, weight_total: u32) -> vec3<u32> {
    var spectrum : array<u32, 16>;
    let wt = max(weight_total, 1u);
    for (var b: u32 = 0u; b < 16u; b = b + 1u) {
        spectrum[b] = spec_acc[b] / wt;
    }
    var r : u32 = 0u;
    var g : u32 = 0u;
    var b : u32 = 0u;
    for (var i: u32 = 0u; i < 5u; i = i + 1u) {
        b = b + spectrum[i];
    }
    for (var i: u32 = 5u; i < 10u; i = i + 1u) {
        g = g + spectrum[i];
    }
    for (var i: u32 = 10u; i < 16u; i = i + 1u) {
        r = r + spectrum[i];
    }
    let r_b = min(r / 6u, 255u);
    let g_b = min(g / 5u, 255u);
    let b_b = min(b / 5u, 255u);
    return vec3<u32>(r_b, g_b, b_b);
}

fn weighted_reflectance(c: GpuCrystal, band: u32) -> u32 {
    let bw0 : u32 = observer.illuminant_blend & 0xFFu;
    let bw1 : u32 = (observer.illuminant_blend >> 8u)  & 0xFFu;
    let bw2 : u32 = (observer.illuminant_blend >> 16u) & 0xFFu;
    let bw3 : u32 = (observer.illuminant_blend >> 24u) & 0xFFu;
    let r0 = spectral_byte(c, 0u, band) * bw0;
    let r1 = spectral_byte(c, 1u, band) * bw1;
    let r2 = spectral_byte(c, 2u, band) * bw2;
    let r3 = spectral_byte(c, 3u, band) * bw3;
    let wsum = max(bw0 + bw1 + bw2 + bw3, 1u);
    return (r0 + r1 + r2 + r3) / wsum;
}

// ════════════════════════════════════════════════════════════════════════════
// § ENTRY — one thread per pixel. @workgroup_size(8, 8) = 64 threads / WG.
// ════════════════════════════════════════════════════════════════════════════

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // § T11-W18-ITER15 (telemetry-driven · post-iter15) · workgroup-cache
    //   tested · GpuCrystal=352B × 32 = 11KB shared-mem load + barrier
    //   COSTS more than the cache-hit savings on Intel-Arc + 1440p workload.
    //   REVERTED to direct-global-fetch · which the L1$ amortizes well
    //   enough at N=32 crystals × 1440p = 3.7M reads/frame.
    // (workgroup-cache constants kept above for future N>64 + spatial-index
    //  experiments where the cache pattern actually pays off.)

    let px = gid.x;
    let py = gid.y;
    if (px >= observer.width || py >= observer.height) {
        return;
    }

    if (!observer_permits_silhouette()) {
        textureStore(output, vec2<i32>(i32(px), i32(py)), vec4<f32>(0.0));
        return;
    }

    let dir = pixel_direction(px, py);

    var spec_acc : array<u32, 16>;
    for (var b: u32 = 0u; b < 16u; b = b + 1u) {
        spec_acc[b] = 0u;
    }
    var weight_total : u32 = 0u;

    // § T11-W18-NOVEL · ℂ-amplitude bundle (in addition to spec) + holographic
    //   per-pixel phase-aggregate. Phase derives from crystal-index + extent +
    //   per-sample-shift · gives DETERMINISTIC interference fringes between
    //   crystals (constructive bright · destructive cancel · ALIEN visual).
    var amp_re : f32 = 0.0;
    var amp_im : f32 = 0.0;
    var amp_total : f32 = 0.0;

    // § T11-W18-CRYSTAL128 · clamp inner-loop bound to MAX_CRYSTALS so the
    //   compiler can unroll/predict against a fixed upper limit. Host-side
    //   contract is `observer.n_crystals ≤ MAX_CRYSTALS` ; the min() is a
    //   defensive guard.
    let n_crystals_capped = min(observer.n_crystals, MAX_CRYSTALS);

    for (var s: u32 = 0u; s < RAY_SAMPLES; s = s + 1u) {
        let world = ray_sample_world(dir, s);
        let yaw   : u32 = u32(observer.yaw_milli) ^ (s * 17u);
        let pitch : u32 = u32(observer.pitch_milli) ^ (s * 31u);

        // § T11-W18-CRYSTAL128 · early-exit on bright pixels. Once the
        //   accumulated weight has crossed EARLY_EXIT_AMP_THRESHOLD the
        //   pixel's final sRGB barely moves with additional contributors.
        //   Skip the rest of this sample's inner crystal-loop. Outer
        //   sample-loop continues so other ray-depths can still light dim
        //   regions.
        if (weight_total >= EARLY_EXIT_AMP_THRESHOLD) {
            break;
        }

        for (var ci: u32 = 0u; ci < n_crystals_capped; ci = ci + 1u) {
            let c = crystals[ci];

            if (!crystal_permits_silhouette(c)) {
                continue;
            }

            let d_sq = dist_sq_mm_i64(
                c.world_pos.x, c.world_pos.y, c.world_pos.z,
                world.x, world.y, world.z,
            );
            let r_total = c.extent_mm + NEAR_RADIUS_MM;
            let r_total_sq = r_total * r_total;
            let radius_sq_4 = NEAR_RADIUS_MM * NEAR_RADIUS_MM * 4;
            let cutoff = min(r_total_sq, radius_sq_4);
            if (d_sq > cutoff) {
                continue;
            }

            let extent = silhouette_at_angle(c, yaw, pitch);
            if (extent <= 0) {
                continue;
            }

            let d_sq_pos = max(d_sq, 1);
            let extent_sq = c.extent_mm * c.extent_mm;
            let denom = d_sq_pos + extent_sq;
            let inv_d_scaled = clamp((extent_sq * 1024) / denom, 1, 1024);
            let extent_term : u32 = max(u32(extent) / 16u, 1u);
            let inv_d_term  : u32 = u32(inv_d_scaled) / 4u;
            var weight : u32 = extent_term * inv_d_term;
            if (weight == 0u) { continue; }
            weight = min(max(weight, 1u), 2048u);

            for (var b: u32 = 0u; b < 16u; b = b + 1u) {
                let r = weighted_reflectance(c, b);
                spec_acc[b] = spec_acc[b] + (r * weight) / 32u;
            }
            weight_total = weight_total + weight;

            // § T11-W18-NOVEL · ℂ-amplitude bundle. Phase = per-crystal-index
            //   × τ + per-sample-shift + magnitude-from-extent. Crystals at
            //   same observer-distance with SAME phase = constructive bright.
            //   Crystals 180°-out = destructive cancel (alien fringe pattern).
            let amp_lin : f32 = f32(weight) * 0.001;
            let phase_seed : u32 = ci * 0x9E3779B9u + s * 0x85EBCA6Bu
                                 + u32(c.extent_mm) * 0xC2B2AE35u;
            let phase : f32 = f32(phase_seed % 6283u) / 1000.0; // 0..2π
            amp_re = amp_re + amp_lin * cos(phase);
            amp_im = amp_im + amp_lin * sin(phase);
            amp_total = amp_total + amp_lin;
        }
    }

    if (weight_total == 0u) {
        textureStore(output, vec2<i32>(i32(px), i32(py)), vec4<f32>(0.0));
        return;
    }

    let rgb = project_blended_to_srgb(spec_acc, weight_total);
    let r_f = f32(rgb.x) / 255.0;
    let g_f = f32(rgb.y) / 255.0;
    let b_f = f32(rgb.z) / 255.0;

    // § T11-W18-NOVEL · phase-arg-hue HSV blend over spectral RGB.
    //   magnitude² = |∑ amp·e^iφ|² = constructive-interference-intensity
    //   arg(amp) = phase-direction · maps to hue (HSV)
    //   coherence = |∑amp|/∑|amp| · maps to saturation
    //   Result · pure-spectral when phase-decoherent · vivid-hue-fringes when coherent
    let mag_sq  : f32 = amp_re * amp_re + amp_im * amp_im;
    let mag     : f32 = sqrt(mag_sq);
    let coher   : f32 = clamp(mag / max(amp_total, 0.0001), 0.0, 1.0);
    let phase_a : f32 = atan2(amp_im, amp_re); // -π..π
    let hue     : f32 = (phase_a + 3.14159265) / 6.28318530;
    // HSV → RGB · saturated by coherence · valued by intensity (cap to 1).
    let s_v     : f32 = coher;
    let v_v     : f32 = clamp(mag * 0.5, 0.0, 1.0);
    let h6      : f32 = hue * 6.0;
    let cv      : f32 = v_v * s_v;
    let xv      : f32 = cv * (1.0 - abs((h6 % 2.0) - 1.0));
    let mv      : f32 = v_v - cv;
    var hr      : f32 = 0.0; var hg : f32 = 0.0; var hb : f32 = 0.0;
    if      (h6 < 1.0) { hr = cv; hg = xv;          }
    else if (h6 < 2.0) { hr = xv; hg = cv;          }
    else if (h6 < 3.0) {          hg = cv; hb = xv; }
    else if (h6 < 4.0) {          hg = xv; hb = cv; }
    else if (h6 < 5.0) { hr = xv;          hb = cv; }
    else               { hr = cv;          hb = xv; }
    let hue_rgb = vec3<f32>(hr + mv, hg + mv, hb + mv);
    // Mix spectral RGB with hue-RGB · 50/50 · spectral keeps base color identity ·
    // hue-RGB adds interference-fringe-character when crystals phase-coherent.
    let final_rgb = mix(vec3<f32>(r_f, g_f, b_f), hue_rgb, 0.5);
    let alpha     = clamp(v_v + coher * 0.5, 0.0, 1.0);
    textureStore(output, vec2<i32>(i32(px), i32(py)),
                 vec4<f32>(clamp(final_rgb, vec3<f32>(0.0), vec3<f32>(1.0)), alpha));
}
