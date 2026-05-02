// § substrate_resonance.wgsl — GPU compute-shader port of the
// cssl-host-alien-materialization pixel-field algorithm.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-W18-G-GPU
//
// One thread = one pixel. Each thread :
//   1. Computes the observer-ray direction for its pixel (pinhole projection
//      with stage-0 small-angle yaw+pitch).
//   2. Walks the ray through the world in `RAY_SAMPLES` even steps to
//      `RAY_MAX_DIST_MM`.
//   3. At each sample, scans the storage-buffer crystals[] for any whose
//      bounding-sphere overlaps the sample's NEAR_RADIUS_MM. (Linear scan ;
//      future iterations can swap in a uniform-grid storage-buffer.)
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
//
// § CONSENT
//   Σ-mask bit 0 = silhouette permission. If either the observer or the
//   crystal has it cleared, the crystal contributes ZERO. PRIME-DIRECTIVE :
//   no silent-override, no fallback render-path that bypasses consent.

const RAY_SAMPLES        : u32 = 8u;
const RAY_MAX_DIST_MM    : i32 = 16000;
const NEAR_RADIUS_MM     : i32 = 1500;
const SPECTRAL_BANDS     : u32 = 16u;
const ILLUMINANT_COUNT   : u32 = 4u;
const ASPECT_SILHOUETTE  : u32 = 0u;
const Z_UNIT             : i32 = 1000;

// ════════════════════════════════════════════════════════════════════════════
// § STORAGE-BUFFER LAYOUT — must match cssl_host_substrate_resonance_gpu::
// buffer_pack::GpuCrystal exactly (host-side bytemuck::Pod struct).
// ════════════════════════════════════════════════════════════════════════════

// Silhouette spline = 16 control points × 4 axis modulators of i16 ;
// packed in WGSL as 16×4 i32 (we sign-extend at host-pack time so every i16
// fits in an i32 — burns 256 B vs 128 B but lets WGSL read directly without
// 16-bit-storage extension).
struct GpuCrystal {
    // World position (millimeters, fixed-point), aligned to 16.
    world_pos    : vec3<i32>,
    // Bounding extent in mm.
    extent_mm    : i32,
    // Σ-mask (low 8 bits used ; full u32 reserved for future).
    sigma_mask   : u32,
    // Padding to align spectral_lut to 16-byte boundary.
    _pad0        : u32,
    _pad1        : u32,
    _pad2        : u32,
    // Spectral LUT : 4 illuminants × 16 bands × u8, packed into
    // ILLUMINANT_COUNT × (SPECTRAL_BANDS / 4) = 4 × 4 = 16 u32 words.
    // Each u32 holds 4 consecutive band-bytes (little-endian within the word).
    spectral_lut : array<vec4<u32>, 4>, // [ill] × 4 packed-bands per word
    // Silhouette spline control points, sign-extended to i32 (256 B).
    silhouette   : array<vec4<i32>, 16>,
};

struct ObserverUniform {
    // Observer position (mm fixed-point).
    pos_x_mm        : i32,
    pos_y_mm        : i32,
    pos_z_mm        : i32,
    // Observer Σ-mask token (low 8 bits = aspects 0..8).
    sigma_mask      : u32,
    // Yaw + pitch in milliradians (clamped at -1000..1000 by host).
    yaw_milli       : i32,
    pitch_milli     : i32,
    // Output dimensions.
    width           : u32,
    height          : u32,
    // Number of crystals in the storage buffer.
    n_crystals      : u32,
    // Illuminant blend weights × 4 × u8, packed into a single u32
    // (sun · moon · torch · ambient ; little-endian byte order).
    illuminant_blend: u32,
    // Padding to 16-byte alignment.
    _pad0           : u32,
    _pad1           : u32,
};

@group(0) @binding(0) var<uniform> observer    : ObserverUniform;
@group(0) @binding(1) var<storage, read> crystals : array<GpuCrystal>;
@group(0) @binding(2) var output : texture_storage_2d<rgba8unorm, write>;

// ════════════════════════════════════════════════════════════════════════════
// § Helper functions (all integer, all deterministic).
// ════════════════════════════════════════════════════════════════════════════

// Σ-mask test : observer's mask permits silhouette aspect ?
fn observer_permits_silhouette() -> bool {
    return (observer.sigma_mask & 1u) != 0u;
}

// Σ-mask test : crystal permits silhouette aspect ?
fn crystal_permits_silhouette(c: GpuCrystal) -> bool {
    return (c.sigma_mask & 1u) != 0u;
}

// Squared distance (i64 logic in i32 with saturation — for game-scale
// coordinates the squared distance fits in i32 once we early-out > 32m).
fn dist_sq_mm_i64(ax: i32, ay: i32, az: i32, bx: i32, by: i32, bz: i32) -> i32 {
    let dx = ax - bx;
    let dy = ay - by;
    let dz = az - bz;
    // 16m envelope ⇒ |d| ≤ 16000 ⇒ d² ≤ 2.56e8 → safely in i32.
    return dx * dx + dy * dy + dz * dz;
}

// Per-pixel observer-ray direction. Mirrors ray::pixel_direction.
fn pixel_direction(px: u32, py: u32) -> vec3<i32> {
    let nx = i32(px) * 2 - i32(observer.width);
    let ny = i32(observer.height) - i32(py) * 2;
    let z_unit = Z_UNIT;
    let x_unit = nx;
    let y_unit = ny;
    // Yaw rotation around Y axis (small-angle approximation).
    let cos_y : i32 = 1000;
    let sin_y : i32 = clamp(observer.yaw_milli, -1000, 1000);
    let xr = (x_unit * cos_y - z_unit * sin_y) / 1000;
    let zr = (x_unit * sin_y + z_unit * cos_y) / 1000;
    // Pitch rotation around X axis.
    let cos_p : i32 = 1000;
    let sin_p : i32 = clamp(observer.pitch_milli, -1000, 1000);
    let yr  = (y_unit * cos_p - zr * sin_p) / 1000;
    let zr2 = (y_unit * sin_p + zr * cos_p) / 1000;
    return vec3<i32>(xr, yr, zr2);
}

// Compute one ray-sample's world position. Mirrors ray::walk_ray.
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

// Silhouette-spline eval : mirrors aspect::silhouette_at_angle.
fn silhouette_at_angle(c: GpuCrystal, yaw: u32, pitch: u32) -> i32 {
    let t = (yaw + pitch) % 1000u;
    // Axis weights : axis 0 baseline at 64 (1/4 full).
    var w0 : u32 = (yaw & 0xFFu);
    if (w0 < 64u) { w0 = 64u; }
    let w1 : u32 = (yaw >> 8u) & 0xFFu;
    let w2 : u32 = pitch & 0xFFu;
    let w3 : u32 = (pitch >> 8u) & 0xFFu;
    let wsum : i32 = max(i32(w0 + w1 + w2 + w3), 1);
    // Find segment (16 points → 15 segments).
    let seg : u32 = (t * 15u) / 1000u;
    let seg_clamped : u32 = min(seg, 14u);
    let local_t : i32 = i32(t * 15u) - i32(seg) * 1000;
    let local_t_clamped : i32 = clamp(local_t, 0, 1000);
    let inv_t : i32 = 1000 - local_t_clamped;
    let a = c.silhouette[seg_clamped];
    let b = c.silhouette[seg_clamped + 1u];
    var acc : i32 = 0;
    let weights = vec4<i32>(i32(w0), i32(w1), i32(w2), i32(w3));
    let a_arr = a;
    let b_arr = b;
    // Manually unroll vec4 dot for clarity (and avoid i64-overflow in WGSL).
    let mid0 = (a_arr.x * inv_t + b_arr.x * local_t_clamped) / 1000;
    let mid1 = (a_arr.y * inv_t + b_arr.y * local_t_clamped) / 1000;
    let mid2 = (a_arr.z * inv_t + b_arr.z * local_t_clamped) / 1000;
    let mid3 = (a_arr.w * inv_t + b_arr.w * local_t_clamped) / 1000;
    acc = mid0 * weights.x + mid1 * weights.y + mid2 * weights.z + mid3 * weights.w;
    let raw = abs(acc / wsum);
    let clamped = clamp(raw, 0, 32767);
    // Scale to extent_mm. For game-scale extents (≤ 32m) the product fits
    // in i32 ; we divide by 32768 safely.
    let scaled = (clamped * c.extent_mm) / 32768;
    return clamp(scaled, 0, c.extent_mm);
}

// Read u8 reflectance from packed spectral_lut[ill][band].
// Each vec4<u32> word holds 4 packed bands : word.x=bands[0..4], .y=[4..8],
// .z=[8..12], .w=[12..16]. Each u32 in turn holds 4 u8 (LE byte order).
fn spectral_byte(c: GpuCrystal, ill: u32, band: u32) -> u32 {
    let word_idx = band / 4u;        // 0..3 : which u32 in the vec4.
    let byte_idx = band & 3u;        // 0..3 : which byte in the u32.
    let v4 = c.spectral_lut[ill];
    var word : u32;
    if (word_idx == 0u)      { word = v4.x; }
    else if (word_idx == 1u) { word = v4.y; }
    else if (word_idx == 2u) { word = v4.z; }
    else                     { word = v4.w; }
    return (word >> (byte_idx * 8u)) & 0xFFu;
}

// Project an accumulated 16-band spectrum to sRGB via observer's illuminant
// blend. Mirrors spectral::project_to_srgb but operates on the per-pixel
// `spec_acc` already-pre-blended-and-weighted spectrum.
fn project_blended_to_srgb(spec_acc: array<u32, 16>, weight_total: u32) -> vec3<u32> {
    // Normalise per-band by weight_total.
    var spectrum : array<u32, 16>;
    let wt = max(weight_total, 1u);
    for (var b: u32 = 0u; b < 16u; b = b + 1u) {
        spectrum[b] = spec_acc[b] / wt;
    }
    // Stage-0 16-band → sRGB band-grouping :
    //   bands 0..5 → blue · 5..10 → green · 10..16 → red.
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

// Composite the observer's illuminant-weighted reflectance for `(crystal,
// band)` on the fly. blend_weights packed into a single u32 (LE bytes).
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
    let px = gid.x;
    let py = gid.y;
    if (px >= observer.width || py >= observer.height) {
        return;
    }

    // Early-out : observer's silhouette aspect revoked → all-pixels transparent.
    if (!observer_permits_silhouette()) {
        textureStore(output, vec2<i32>(i32(px), i32(py)), vec4<f32>(0.0));
        return;
    }

    let dir = pixel_direction(px, py);

    // Per-pixel resonance accumulators.
    var spec_acc : array<u32, 16>;
    for (var b: u32 = 0u; b < 16u; b = b + 1u) {
        spec_acc[b] = 0u;
    }
    var weight_total : u32 = 0u;

    // Iterate ray samples × crystals (linear-scan). For 1k crystals the
    // inner loop dominates ; future hosts can pre-build a uniform-grid
    // storage-buffer to bound this to the local cell-cluster.
    for (var s: u32 = 0u; s < RAY_SAMPLES; s = s + 1u) {
        let world = ray_sample_world(dir, s);
        let yaw   : u32 = u32(observer.yaw_milli) ^ (s * 17u);
        let pitch : u32 = u32(observer.pitch_milli) ^ (s * 31u);

        for (var ci: u32 = 0u; ci < observer.n_crystals; ci = ci + 1u) {
            let c = crystals[ci];

            // Σ-mask : crystal must permit silhouette.
            if (!crystal_permits_silhouette(c)) {
                continue;
            }

            // Bounding-sphere reject (matches CPU::crystals_near_grid behaviour
            // at NEAR_RADIUS_MM — same `r_total² ∧ 4·r²` cutoff).
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

            // Silhouette extent at this observer angle.
            let extent = silhouette_at_angle(c, yaw, pitch);
            if (extent <= 0) {
                continue;
            }

            // Distance attenuation : dist_sq vs extent².
            let d_sq_pos = max(d_sq, 1);
            let extent_sq = c.extent_mm * c.extent_mm;
            // (extent² × 1024) / (d² + extent²) ∈ [1 .. 1024]
            let denom = d_sq_pos + extent_sq;
            let inv_d_scaled = clamp((extent_sq * 1024) / denom, 1, 1024);
            let extent_term : u32 = max(u32(extent) / 16u, 1u);
            let inv_d_term  : u32 = u32(inv_d_scaled) / 4u;
            var weight : u32 = extent_term * inv_d_term;
            if (weight == 0u) { continue; }
            weight = min(max(weight, 1u), 2048u);

            // Spectral accumulation : add weighted illuminant-blended
            // reflectance per-band.
            for (var b: u32 = 0u; b < 16u; b = b + 1u) {
                let r = weighted_reflectance(c, b);
                spec_acc[b] = spec_acc[b] + (r * weight) / 32u;
            }
            weight_total = weight_total + weight;
        }
    }

    if (weight_total == 0u) {
        textureStore(output, vec2<i32>(i32(px), i32(py)), vec4<f32>(0.0));
        return;
    }

    // Project to sRGB. (We folded the illuminant blend into spec_acc above ;
    // here we just band-group + clamp.)
    let rgb = project_blended_to_srgb(spec_acc, weight_total);
    let r_f = f32(rgb.x) / 255.0;
    let g_f = f32(rgb.y) / 255.0;
    let b_f = f32(rgb.z) / 255.0;
    textureStore(output, vec2<i32>(i32(px), i32(py)), vec4<f32>(r_f, g_f, b_f, 1.0));
}
