// § scene.wgsl — uber-shader for the diagnostic-dense LoA-v13 test-room.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-LOA-RICH-RENDER (W-LOA-rich-render-overhaul)
// § T11-LOA-RAYMARCH    (W-LOA-raymarched-primitives) — 6 SDF/fractal kinds
//
// § ROLE
//   Uber-shader supporting :
//     - Per-vertex material_id + pattern_id indirection into LUTs
//     - 16 procedural patterns computed analytically in the fragment shader
//     - 6 RAYMARCHED SDF/fractal kinds executed by a 64-step sphere-tracer
//       inside the cube bounding-volume (mandelbulb · sphere · torus ·
//       gyroid · quaternion-julia · menger-sponge)
//     - Material BRDF approximation (lambert + ambient + fresnel-flavored
//       specular highlight + emissive)
//     - Time-driven holographic / iridescent / dichroic effects
//     - frag_depth output so raymarched hits depth-test correctly against
//       surrounding geometry (cube bounding-volume passes through default
//       projected depth)
//
// § VERTEX LAYOUT (see geometry.rs Vertex::desc)
//   @location(0) position    : vec3<f32>
//   @location(1) normal      : vec3<f32>
//   @location(2) color       : vec3<f32>   (base tint)
//   @location(3) uv          : vec2<f32>   (procedural-pattern coord)
//   @location(4) material_id : u32         (interpolated as flat)
//   @location(5) pattern_id  : u32         (interpolated as flat)
//
// § BIND GROUP 0 BINDING 0 — Uniforms (single UBO holds view-proj + sun-dir +
//   ambient + time + camera-pos + 16-entry material LUT + 22-entry pattern LUT).
//   Total size : 64 + 16 + 16 + 16 + 16 + 16 × 48 + 22 × 16
//              = 64 + 64 + 768 + 352 = 1248 bytes
//   (well under the 16 KiB UBO limit on every wgpu backend).

struct Material {
    albedo:    vec3<f32>,
    roughness: f32,
    emissive:  vec3<f32>,
    metallic:  f32,
    alpha:     f32,
    // § T11-LOA-FIX-MAT-STRIDE : trailing `_pad: vec3<f32>` removed.
    //   WGSL forces vec3 members to 16-byte alignment, which would push
    //   `_pad` from offset 36 to 48 and bloat the per-element stride to
    //   64 bytes. Without it, the compiler auto-pads the struct end from
    //   36 → 48 to satisfy struct-alignment(16), and the stride matches
    //   the CPU `Material` (48 bytes) exactly. Crash fix : 1136 → 1392
    //   buffer-size mismatch reported by wgpu validation.
};

struct Pattern {
    kind:     u32,
    scale:    f32,
    rotation: f32,
    phase:    f32,
};

// § T11-LOA-FID-STOKES : WGSL representation of a 4×4 Mueller matrix.
// 4 rows × vec4 = 64 bytes ; matches Rust `MuellerWgsl` exactly.
struct MuellerWgsl {
    row0 : vec4<f32>,
    row1 : vec4<f32>,
    row2 : vec4<f32>,
    row3 : vec4<f32>,
};

struct Uniforms {
    view_proj : mat4x4<f32>,
    sun_dir   : vec4<f32>,
    ambient   : vec4<f32>,
    // time.x = seconds since render start (drives time-varying patterns)
    // time.y = frame counter (modulo 1e6) cast to f32
    // time.zw = unused
    time      : vec4<f32>,
    // § T11-LOA-RAYMARCH : real camera world-space position so the
    // sphere-tracer can reconstruct the view ray inside cube-local space.
    // .xyz = world camera position ; .w = unused (reserved for tracer flags).
    camera_pos: vec4<f32>,
    // § T11-LOA-FID-STOKES : sun-light Stokes vector (I, Q, U, V).
    // Atmospheric scattering imparts a slight horizontal-Q bias.
    sun_stokes: vec4<f32>,
    // § T11-LOA-FID-STOKES : Stokes pipeline control word.
    //   .x = polarization_mode (0=Intensity · 1=Q · 2=U · 3=V · 4=DOP)
    //   .y = enable_mueller    (0/1)
    //   .zw = reserved
    stokes_control: vec4<f32>,
    materials : array<Material, 16>,
    patterns  : array<Pattern, 22>,
    // § T11-LOA-FID-STOKES : per-material Mueller-matrix LUT.
    // Each entry is 4 × vec4 = 64 bytes (16 entries = 1024 bytes).
    muellers  : array<MuellerWgsl, 16>,
};

@group(0) @binding(0) var<uniform> u : Uniforms;

struct VsIn {
    @location(0) position    : vec3<f32>,
    @location(1) normal      : vec3<f32>,
    @location(2) color       : vec3<f32>,
    @location(3) uv          : vec2<f32>,
    @location(4) material_id : u32,
    @location(5) pattern_id  : u32,
};

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) world_pos      : vec3<f32>,
    @location(1) world_normal   : vec3<f32>,
    @location(2) base_color     : vec3<f32>,
    @location(3) uv             : vec2<f32>,
    @location(4) @interpolate(flat) material_id : u32,
    @location(5) @interpolate(flat) pattern_id  : u32,
};

@vertex
fn vs_main(in : VsIn) -> VsOut {
    var out : VsOut;
    out.clip_pos     = u.view_proj * vec4<f32>(in.position, 1.0);
    out.world_pos    = in.position;
    out.world_normal = in.normal;
    out.base_color   = in.color;
    out.uv           = in.uv;
    out.material_id  = in.material_id;
    out.pattern_id   = in.pattern_id;
    return out;
}

// ─────────────────────────────────────────────────────────────────────────
// § Procedural-pattern functions (one per pattern kind)
// ─────────────────────────────────────────────────────────────────────────

fn pat_grid(uv: vec2<f32>, scale: f32) -> vec3<f32> {
    let su = fract(abs(uv.x * scale));
    let sv = fract(abs(uv.y * scale));
    let edge = 0.04;
    if (su < edge || sv < edge || su > (1.0 - edge) || sv > (1.0 - edge)) {
        return vec3<f32>(0.10, 0.10, 0.10);
    }
    return vec3<f32>(0.85, 0.85, 0.85);
}

fn pat_checker(uv: vec2<f32>, scale: f32) -> vec3<f32> {
    let iu = floor(uv.x * scale);
    let iv = floor(uv.y * scale);
    let p = (iu + iv) - 2.0 * floor((iu + iv) * 0.5);
    if (p < 0.5) {
        return vec3<f32>(0.05, 0.05, 0.05);
    }
    return vec3<f32>(0.95, 0.95, 0.95);
}

// 24-patch X-Rite Macbeth ColorChecker (sRGB display values).
// Order : column-major across 6 cols × 4 rows (uv.y down = next row).
fn pat_macbeth(uv: vec2<f32>) -> vec3<f32> {
    let cu_f = clamp(uv.x, 0.0, 0.9999) * 6.0;
    let cv_f = clamp(uv.y, 0.0, 0.9999) * 4.0;
    let cu = u32(floor(cu_f));
    let cv = u32(floor(cv_f));
    let idx = cv * 6u + cu;

    var color : vec3<f32> = vec3<f32>(0.5);
    switch idx {
        case 0u:  { color = vec3<f32>(0.45, 0.32, 0.27); }
        case 1u:  { color = vec3<f32>(0.76, 0.58, 0.51); }
        case 2u:  { color = vec3<f32>(0.36, 0.48, 0.61); }
        case 3u:  { color = vec3<f32>(0.35, 0.42, 0.27); }
        case 4u:  { color = vec3<f32>(0.51, 0.50, 0.69); }
        case 5u:  { color = vec3<f32>(0.40, 0.74, 0.67); }
        case 6u:  { color = vec3<f32>(0.83, 0.48, 0.18); }
        case 7u:  { color = vec3<f32>(0.27, 0.34, 0.65); }
        case 8u:  { color = vec3<f32>(0.77, 0.33, 0.36); }
        case 9u:  { color = vec3<f32>(0.36, 0.24, 0.42); }
        case 10u: { color = vec3<f32>(0.62, 0.73, 0.25); }
        case 11u: { color = vec3<f32>(0.89, 0.63, 0.17); }
        case 12u: { color = vec3<f32>(0.21, 0.24, 0.58); }
        case 13u: { color = vec3<f32>(0.27, 0.58, 0.29); }
        case 14u: { color = vec3<f32>(0.69, 0.20, 0.22); }
        case 15u: { color = vec3<f32>(0.91, 0.78, 0.13); }
        case 16u: { color = vec3<f32>(0.73, 0.33, 0.59); }
        case 17u: { color = vec3<f32>(0.04, 0.50, 0.66); }
        case 18u: { color = vec3<f32>(0.95, 0.95, 0.95); }
        case 19u: { color = vec3<f32>(0.78, 0.78, 0.78); }
        case 20u: { color = vec3<f32>(0.63, 0.63, 0.63); }
        case 21u: { color = vec3<f32>(0.48, 0.48, 0.48); }
        case 22u: { color = vec3<f32>(0.33, 0.33, 0.33); }
        case 23u: { color = vec3<f32>(0.20, 0.20, 0.20); }
        default:  { color = vec3<f32>(0.5, 0.5, 0.5); }
    }
    return color;
}

// Snellen tumbling-E chart : letters scale top-to-bottom, with rows
// 1..11 mapping to standard sizes 200/200 down to 20/10.
fn pat_snellen(uv: vec2<f32>) -> vec3<f32> {
    // 11 rows ; row 0 at top is huge, row 10 at bottom is tiny.
    let row_f = clamp(uv.y, 0.0, 0.9999) * 11.0;
    let row = u32(floor(row_f));
    let local_v = fract(row_f); // 0..1 within the row

    // Row letter-size : top row = 1.0, row 10 = 0.10.
    let size_factor = 1.0 - (f32(row) / 11.0) * 0.90;

    // Normalise local_u so the row centers a 5×5 pixel-grid letter.
    let local_u = uv.x * (1.0 / size_factor);
    let cell_u = fract(local_u);
    let cell_v = local_v;

    // Render a tumbling-E shape : 3 horizontal bars across the cell.
    // Bar y-bands : 0.10..0.25 · 0.45..0.55 · 0.75..0.90
    // Stem x-band : 0.10..0.25
    let in_stem = cell_u > 0.10 && cell_u < 0.25;
    let in_bar1 = cell_v > 0.10 && cell_v < 0.25 && cell_u > 0.10 && cell_u < 0.85;
    let in_bar2 = cell_v > 0.45 && cell_v < 0.55 && cell_u > 0.10 && cell_u < 0.65;
    let in_bar3 = cell_v > 0.75 && cell_v < 0.90 && cell_u > 0.10 && cell_u < 0.85;

    if (in_stem || in_bar1 || in_bar2 || in_bar3) {
        return vec3<f32>(0.05, 0.05, 0.05);
    }
    return vec3<f32>(0.95, 0.95, 0.95);
}

// Grayscale gradient.
fn pat_gradient_grayscale(uv: vec2<f32>) -> vec3<f32> {
    let g = clamp(uv.x, 0.0, 1.0);
    return vec3<f32>(g, g, g);
}

// Hue-wheel : maps uv → hue circle around a center disc.
fn pat_hue_wheel(uv: vec2<f32>) -> vec3<f32> {
    let d = uv - vec2<f32>(0.5, 0.5);
    let r = length(d);
    let theta = atan2(d.y, d.x); // -π..π
    let h = (theta + 3.14159265) / 6.2831853; // 0..1
    let s = clamp(r * 2.0, 0.0, 1.0);
    let v = 1.0;
    // HSV → RGB
    let c = v * s;
    let x = c * (1.0 - abs(((h * 6.0) % 2.0) - 1.0));
    let m = v - c;
    var rgb : vec3<f32>;
    let h6 = h * 6.0;
    if (h6 < 1.0) {
        rgb = vec3<f32>(c, x, 0.0);
    } else if (h6 < 2.0) {
        rgb = vec3<f32>(x, c, 0.0);
    } else if (h6 < 3.0) {
        rgb = vec3<f32>(0.0, c, x);
    } else if (h6 < 4.0) {
        rgb = vec3<f32>(0.0, x, c);
    } else if (h6 < 5.0) {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }
    return rgb + vec3<f32>(m);
}

// Hash-based value-noise (deterministic, low-frequency).
fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn pat_value_noise(uv: vec2<f32>, scale: f32) -> vec3<f32> {
    let p = uv * scale;
    let i = floor(p);
    let f = fract(p);
    // bilinear interp of 4 corner hashes for smooth-ish noise
    let h00 = hash21(i);
    let h10 = hash21(i + vec2<f32>(1.0, 0.0));
    let h01 = hash21(i + vec2<f32>(0.0, 1.0));
    let h11 = hash21(i + vec2<f32>(1.0, 1.0));
    let s = f * f * (3.0 - 2.0 * f); // smoothstep weights
    let n0 = mix(h00, h10, s.x);
    let n1 = mix(h01, h11, s.x);
    let n = mix(n0, n1, s.y);
    return vec3<f32>(n, n, n);
}

fn pat_concentric_rings(uv: vec2<f32>, scale: f32) -> vec3<f32> {
    let d = uv - vec2<f32>(0.5, 0.5);
    let r = length(d);
    let band = fract(r * scale);
    if (band < 0.5) {
        return vec3<f32>(0.10, 0.10, 0.10);
    }
    return vec3<f32>(0.90, 0.90, 0.90);
}

fn pat_radial_spokes(uv: vec2<f32>, scale: f32) -> vec3<f32> {
    let d = uv - vec2<f32>(0.5, 0.5);
    let theta = atan2(d.y, d.x);
    let n_spokes = max(scale, 4.0);
    let band = fract((theta + 3.14159265) * n_spokes / 6.2831853);
    if (band < 0.5) {
        return vec3<f32>(0.05, 0.05, 0.05);
    }
    return vec3<f32>(0.95, 0.95, 0.95);
}

fn pat_radial_gradient(uv: vec2<f32>) -> vec3<f32> {
    let d = uv - vec2<f32>(0.5, 0.5);
    let r = clamp(length(d) * 2.0, 0.0, 1.0);
    return vec3<f32>(r, r, r);
}

fn pat_zoneplate(uv: vec2<f32>, scale: f32) -> vec3<f32> {
    // Frequency-sweep zone plate : sin(k·r²) increasing-frequency rings.
    let d = uv - vec2<f32>(0.5, 0.5);
    let r2 = dot(d, d);
    let v = sin(r2 * scale * 3.14159265 * 4.0);
    let g = 0.5 + 0.5 * v;
    return vec3<f32>(g, g, g);
}

fn pat_frequency_sweep(uv: vec2<f32>) -> vec3<f32> {
    // 4 stacked rows : 1Hz · 4Hz · 16Hz · 64Hz spatial-frequency sinusoids.
    let row = clamp(uv.y, 0.0, 0.9999) * 4.0;
    let band = u32(floor(row));
    var freq : f32 = 1.0;
    switch band {
        case 0u: { freq = 1.0; }
        case 1u: { freq = 4.0; }
        case 2u: { freq = 16.0; }
        case 3u: { freq = 64.0; }
        default: { freq = 1.0; }
    }
    let v = 0.5 + 0.5 * sin(uv.x * freq * 6.2831853);
    return vec3<f32>(v, v, v);
}

// EAN-13-style barcode : 95-module pattern with start guard, body, middle
// guard, body, end guard.
fn pat_ean13(uv: vec2<f32>) -> vec3<f32> {
    let m = i32(floor(clamp(uv.x, 0.0, 0.9999) * 95.0));
    var on : bool = false;

    // Start guard 101
    if (m >= 0 && m < 3) {
        on = (m == 0 || m == 2);
    } else if (m >= 92 && m < 95) {
        // End guard 101
        on = (m == 92 || m == 94);
    } else if (m >= 45 && m < 50) {
        // Middle guard 01010
        on = ((m - 45) % 2 == 0);
    } else {
        // Body : pseudo-random bar
        let h = u32(m) * 2654435761u;
        on = ((h & 1u) == 0u);
    }
    if (on) {
        return vec3<f32>(0.05, 0.05, 0.05);
    }
    return vec3<f32>(0.95, 0.95, 0.95);
}

// QR-aesthetic 25×25 module pattern.
fn pat_qr_code(uv: vec2<f32>) -> vec3<f32> {
    let modules = 25.0;
    let mu = i32(floor(clamp(uv.x, 0.0, 0.9999) * modules));
    let mv = i32(floor(clamp(uv.y, 0.0, 0.9999) * modules));

    // Helper : in-finder pattern at (cx,cy) with 7×7 extent.
    let f1_dx = mu - 3;
    let f1_dy = mv - 3;
    let f2_dx = mu - 21;
    let f2_dy = mv - 3;
    let f3_dx = mu - 3;
    let f3_dy = mv - 21;

    var on : bool = false;
    // Top-left finder
    if (abs(f1_dx) <= 3 && abs(f1_dy) <= 3) {
        let r = max(abs(f1_dx), abs(f1_dy));
        on = (r == 3 || r <= 1);
    }
    // Top-right finder
    else if (abs(f2_dx) <= 3 && abs(f2_dy) <= 3) {
        let r = max(abs(f2_dx), abs(f2_dy));
        on = (r == 3 || r <= 1);
    }
    // Bottom-left finder
    else if (abs(f3_dx) <= 3 && abs(f3_dy) <= 3) {
        let r = max(abs(f3_dx), abs(f3_dy));
        on = (r == 3 || r <= 1);
    }
    // Alignment patterns
    else if ((mu == 18 && mv >= 16 && mv <= 20) || (mv == 18 && mu >= 16 && mu <= 20)) {
        on = true;
    }
    // Timing patterns
    else if ((mu == 6 && mv >= 8 && mv <= 20) || (mv == 6 && mu >= 8 && mu <= 20)) {
        on = ((mu + mv) % 2) == 0;
    }
    // Quiet zones near finders
    else if ((mu < 8 && mv < 8) || (mu >= 19 && mv < 8) || (mu < 8 && mv >= 19)) {
        on = false;
    }
    // Pseudo-random data modules
    else {
        let h = u32(mu) * 2654435761u ^ u32(mv) * 40503u;
        on = (h & 1u) == 1u;
    }

    if (on) {
        return vec3<f32>(0.05, 0.05, 0.05);
    }
    return vec3<f32>(0.95, 0.95, 0.95);
}

// Iridescent thin-film effect (view-angle-dependent hue shift).
fn pat_iridescent(uv: vec2<f32>, normal: vec3<f32>, view_dir: vec3<f32>, t: f32) -> vec3<f32> {
    let cos_theta = clamp(dot(normalize(normal), normalize(view_dir)), 0.0, 1.0);
    let phase = (1.0 - cos_theta) * 6.2831853 + t * 0.5 + uv.x * 3.14159;
    let r = 0.5 + 0.5 * sin(phase);
    let g = 0.5 + 0.5 * sin(phase + 2.094);
    let b = 0.5 + 0.5 * sin(phase + 4.188);
    return vec3<f32>(r, g, b);
}

fn pat_holographic(uv: vec2<f32>, t: f32) -> vec3<f32> {
    // Sparkle dots + dichroic interference, time-varying.
    let p = uv * 30.0 + vec2<f32>(t * 0.3, t * 0.2);
    let n = hash21(floor(p));
    let sparkle = step(0.95, n);
    let h = 0.5 + 0.5 * sin(uv.x * 12.0 + t * 1.5);
    let r = h;
    let g = 0.5 + 0.5 * sin(uv.y * 12.0 + t * 1.7 + 1.5);
    let b = 0.5 + 0.5 * sin((uv.x + uv.y) * 8.0 + t * 1.9 + 3.0);
    return vec3<f32>(r, g, b) * 0.7 + vec3<f32>(sparkle);
}

// Dispatch : look up the pattern entry and call the appropriate helper.
fn procedural(p: Pattern, uv: vec2<f32>, normal: vec3<f32>, view_dir: vec3<f32>, t: f32) -> vec3<f32> {
    var out : vec3<f32> = vec3<f32>(1.0);
    switch p.kind {
        case 0u:  { out = vec3<f32>(1.0); }                              // SOLID
        case 1u:  { out = pat_grid(uv, p.scale); }                       // GRID_1M
        case 2u:  { out = pat_grid(uv, p.scale); }                       // GRID_100MM
        case 3u:  { out = pat_checker(uv, p.scale * 20.0); }             // CHECKERBOARD
        case 4u:  { out = pat_macbeth(uv); }                             // MACBETH
        case 5u:  { out = pat_snellen(uv); }                             // SNELLEN
        case 6u:  { out = pat_qr_code(uv); }                             // QR
        case 7u:  { out = pat_ean13(uv); }                               // EAN13
        case 8u:  { out = pat_gradient_grayscale(uv); }                  // GRAY-GRAD
        case 9u:  { out = pat_hue_wheel(uv); }                           // HUE-WHEEL
        case 10u: { out = pat_value_noise(uv, p.scale); }                // PERLIN
        case 11u: { out = pat_concentric_rings(uv, p.scale); }           // RINGS
        case 12u: { out = pat_radial_spokes(uv, p.scale); }              // SPOKES
        case 13u: { out = pat_zoneplate(uv, p.scale); }                  // ZONEPLATE
        case 14u: { out = pat_frequency_sweep(uv); }                     // FREQ-SWEEP
        case 15u: { out = pat_radial_gradient(uv); }                     // RADIAL-GRAD
        default:  { out = vec3<f32>(1.0); }
    }
    return out;
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-LOA-RAYMARCH — Signed Distance Functions + 64-step sphere-tracer
// ─────────────────────────────────────────────────────────────────────────
//
// Raymarch pattern-IDs 16..21 sphere-trace inside the cube bounding-volume.
// All SDFs operate in cube-LOCAL space (cube center at origin, half-extent
// equal to STRESS_SIZE/2 = 0.4 m), so distances of ~0.001 are tight enough.
//
// To keep iteration counts bounded for the deepest fractals, we use 12 iters
// for mandelbulb / 10 for julia / 4 for menger — the tracer itself caps at
// 64 steps with a 4 m max-t (well past the 0.4 m bounding-volume diagonal).

const RAYMARCH_MAX_STEPS : i32 = 64;
const RAYMARCH_MAX_T : f32 = 4.0;
const RAYMARCH_HIT_EPS : f32 = 0.001;

const KIND_MANDELBULB : u32 = 16u;
const KIND_SPHERE     : u32 = 17u;
const KIND_TORUS      : u32 = 18u;
const KIND_GYROID     : u32 = 19u;
const KIND_JULIA      : u32 = 20u;
const KIND_MENGER     : u32 = 21u;

fn sdf_sphere(p: vec3<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sdf_torus(p: vec3<f32>, big_r: f32, little_r: f32) -> f32 {
    let q = vec2<f32>(length(p.xz) - big_r, p.y);
    return length(q) - little_r;
}

fn sdf_gyroid(p: vec3<f32>, scale: f32, thickness: f32) -> f32 {
    let q = p * scale;
    let g = sin(q.x) * cos(q.y) + sin(q.y) * cos(q.z) + sin(q.z) * cos(q.x);
    return abs(g) / scale - thickness;
}

fn sdf_mandelbulb(p: vec3<f32>) -> f32 {
    var z = p;
    var dr = 1.0;
    var r = 0.0;
    let power = 8.0;
    for (var i: i32 = 0; i < 12; i = i + 1) {
        r = length(z);
        if (r > 2.0) { break; }
        let theta = acos(clamp(z.z / max(r, 1e-6), -1.0, 1.0)) * power;
        let phi = atan2(z.y, z.x) * power;
        let zr = pow(r, power);
        dr = pow(r, power - 1.0) * power * dr + 1.0;
        let st = sin(theta);
        z = zr * vec3<f32>(st * cos(phi), st * sin(phi), cos(theta));
        z = z + p;
    }
    if (r < 1e-6) { return 0.0; }
    return 0.5 * log(r) * r / dr;
}

fn sdf_julia_quaternion(p: vec3<f32>) -> f32 {
    // Quaternion Julia · 4D iteration projected to 3D
    var z = vec4<f32>(p, 0.0);
    let c = vec4<f32>(-0.2, 0.6, 0.2, 0.2);
    var dr = 1.0;
    for (var i: i32 = 0; i < 10; i = i + 1) {
        let r2 = dot(z, z);
        if (r2 > 4.0) { break; }
        dr = 2.0 * length(z) * dr;
        z = vec4<f32>(
            z.x * z.x - dot(z.yzw, z.yzw),
            2.0 * z.x * z.y,
            2.0 * z.x * z.z,
            2.0 * z.x * z.w,
        ) + c;
    }
    let r = length(z);
    if (dr < 1e-6) { return 0.0; }
    return 0.5 * sqrt(r * r / max(dr * dr, 1e-12)) * log(max(r, 1e-6));
}

fn sdf_menger_sponge(p: vec3<f32>) -> f32 {
    // Box-bound : largest cube the sponge fits inside (in cube-local space
    // we use a 0.7-half-extent so the sponge fills most of the bounding cube).
    var d = max(abs(p.x), max(abs(p.y), abs(p.z))) - 1.0;
    var s = 1.0;
    var pp = p;
    for (var i: i32 = 0; i < 4; i = i + 1) {
        let r = abs(fract(pp * s * 0.5 + 0.5) * 2.0 - 1.0);
        let a = max(r.x, r.y);
        let b = max(r.y, r.z);
        let c = max(r.z, r.x);
        let cube_d = (min(min(a, b), c) - 1.0 / 3.0) / s;
        d = max(d, cube_d);
        s = s * 3.0;
    }
    return d;
}

// SDF dispatcher : pick the right SDF given the raymarch kind.
fn sdf_select(kind: u32, p: vec3<f32>, scale: f32, phase: f32) -> f32 {
    switch kind {
        case 16u: { return sdf_mandelbulb(p * 1.25); }
        case 17u: { return sdf_sphere(p, scale); }
        case 18u: { return sdf_torus(p, scale, phase); }
        case 19u: { return sdf_gyroid(p, scale, 0.04); }
        case 20u: { return sdf_julia_quaternion(p * 1.5); }
        case 21u: { return sdf_menger_sponge(p * 1.0); }
        default:  { return length(p) - 1.0; }
    }
}

struct RaymarchHit {
    hit : bool,
    t   : f32,
    pos : vec3<f32>,
};

fn raymarch(ro: vec3<f32>, rd: vec3<f32>, kind: u32, scale: f32, phase: f32) -> RaymarchHit {
    var t : f32 = 0.0;
    for (var i: i32 = 0; i < RAYMARCH_MAX_STEPS; i = i + 1) {
        let p = ro + rd * t;
        let d = sdf_select(kind, p, scale, phase);
        if (d < RAYMARCH_HIT_EPS) {
            return RaymarchHit(true, t, p);
        }
        if (t > RAYMARCH_MAX_T) { break; }
        t = t + d;
    }
    return RaymarchHit(false, 0.0, vec3<f32>(0.0));
}

fn sdf_normal(p: vec3<f32>, kind: u32, scale: f32, phase: f32) -> vec3<f32> {
    let h = 0.001;
    let dx = sdf_select(kind, p + vec3<f32>(h, 0.0, 0.0), scale, phase)
           - sdf_select(kind, p - vec3<f32>(h, 0.0, 0.0), scale, phase);
    let dy = sdf_select(kind, p + vec3<f32>(0.0, h, 0.0), scale, phase)
           - sdf_select(kind, p - vec3<f32>(0.0, h, 0.0), scale, phase);
    let dz = sdf_select(kind, p + vec3<f32>(0.0, 0.0, h), scale, phase)
           - sdf_select(kind, p - vec3<f32>(0.0, 0.0, h), scale, phase);
    return normalize(vec3<f32>(dx, dy, dz));
}

// `true` iff the given pattern-kind triggers fragment-shader sphere-tracing
// (mirror of pattern.rs::pattern_is_raymarch).
fn is_raymarch_kind(kind: u32) -> bool {
    return kind >= 16u && kind <= 21u;
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-LOA-FID-STOKES — Mueller-apply + polarization false-color helpers
// ─────────────────────────────────────────────────────────────────────────

// Apply a Mueller matrix to a Stokes vector. WGSL has no row/col mat4×vec4
// helper that matches our explicit row-major layout, so we expand by hand.
fn mueller_apply(m: MuellerWgsl, s: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        dot(m.row0, s),
        dot(m.row1, s),
        dot(m.row2, s),
        dot(m.row3, s),
    );
}

// Compute degree-of-LINEAR-polarization (sqrt(Q²+U²)/I) safely.
fn dop_linear(s: vec4<f32>) -> f32 {
    let i_safe = max(abs(s.x), 1e-6);
    return sqrt(s.y * s.y + s.z * s.z) / i_safe;
}

// Compute degree-of-TOTAL-polarization (sqrt(Q²+U²+V²)/I) safely.
fn dop_total(s: vec4<f32>) -> f32 {
    let i_safe = max(abs(s.x), 1e-6);
    return sqrt(s.y * s.y + s.z * s.z + s.w * s.w) / i_safe;
}

// Map a signed value in [-1,1] to a red-positive / blue-negative diverging
// false-color used by the polarization-view modes 1-3.
fn signed_to_falsecolor(v: f32) -> vec3<f32> {
    let pos = clamp(v, 0.0, 1.0);
    let neg = clamp(-v, 0.0, 1.0);
    return vec3<f32>(pos, 0.10, neg);
}

// Apply the polarization-view diagnostic. `intensity_rgb` is the standard
// intensity-mode color (the value the renderer would produce without
// Stokes-aware overlay). `s_out` is the post-Mueller Stokes vector.
fn apply_polarization_view(intensity_rgb: vec3<f32>, s_out: vec4<f32>) -> vec3<f32> {
    let mode = u32(u.stokes_control.x + 0.5);
    if (mode == 0u) {
        return intensity_rgb;
    }
    if (mode == 1u) {
        // Q false-color : red=+H · blue=-H · normalize by I.
        let q_norm = s_out.y / max(abs(s_out.x), 1e-6);
        return signed_to_falsecolor(q_norm);
    }
    if (mode == 2u) {
        // U false-color.
        let u_norm = s_out.z / max(abs(s_out.x), 1e-6);
        return signed_to_falsecolor(u_norm);
    }
    if (mode == 3u) {
        // V false-color (circular).
        let v_norm = s_out.w / max(abs(s_out.x), 1e-6);
        return signed_to_falsecolor(v_norm);
    }
    if (mode == 4u) {
        // Total degree-of-polarization (0..1) → grayscale.
        let d = clamp(dop_total(s_out), 0.0, 1.0);
        return vec3<f32>(d, d, d);
    }
    return intensity_rgb;
}

// Compute Mueller-applied Stokes for a given material id.
// When `enable_mueller` is 0, returns the input Stokes unchanged.
fn stokes_for_material(material_id: u32) -> vec4<f32> {
    let s_in = u.sun_stokes;
    let mueller_enable = u.stokes_control.y;
    if (mueller_enable < 0.5) {
        return s_in;
    }
    let m = u.muellers[material_id];
    return mueller_apply(m, s_in);
}

// ─────────────────────────────────────────────────────────────────────────
// § Fragment shader — material LUT + procedural pattern + lighting
// § T11-LOA-RAYMARCH : raymarch branch for pattern-IDs 16..21
// § T11-LOA-FID-STOKES : Mueller per-material + diagnostic polarization view
// ─────────────────────────────────────────────────────────────────────────

struct FsOut {
    @location(0) color : vec4<f32>,
    // Manual frag_depth output : raymarched fragments overwrite the rasterised
    // cube-bounding-volume depth with the actual SDF-hit depth ; cube fragments
    // pass through their original projected depth.
    @builtin(frag_depth) depth : f32,
};

// Convert a world-space position to NDC depth in [0,1] using the global
// view_proj. Used by the raymarch branch to write a corrected frag_depth.
fn world_to_clip_depth(world_pos: vec3<f32>) -> f32 {
    let clip = u.view_proj * vec4<f32>(world_pos, 1.0);
    // wgpu NDC depth ∈ [0, 1] after the OPENGL_TO_WGPU_MATRIX correction
    // baked into camera::proj. Guard against zero-w.
    let inv_w = 1.0 / max(clip.w, 1e-6);
    return clamp(clip.z * inv_w, 0.0, 1.0);
}

// Reconstruct the rasteriser's projected depth for the surrounding cube
// fragment (so cube branches don't disturb depth-test compatibility).
fn rasterised_depth(world_pos: vec3<f32>) -> f32 {
    return world_to_clip_depth(world_pos);
}

@fragment
fn fs_main(in : VsOut) -> FsOut {
    let m = u.materials[in.material_id];
    let p = u.patterns[in.pattern_id];

    var out_color : vec4<f32>;
    var out_depth : f32;

    // ── Raymarch branch : pattern-IDs 16..21 use SDF sphere-tracer ──
    if (is_raymarch_kind(p.kind)) {
        // Cube center : the cube bounding-volume is centered on the stress-
        // object plinth's stress-object-cube center. We recover this by
        // re-anchoring on the closest stress-object center using world_pos
        // proximity logic ; simpler approach : encode the cube center in the
        // SHARED INTERPOLATED quantity. Since base_color is white for all
        // stress objects (see geometry::emit_box(... color=[1,1,1])) we can
        // hijack base_color as a per-cube offset only if vertices carry it.
        // Instead, since the stress-object cubes are axis-aligned with an
        // 0.8 m edge, we anchor on the floor() of world_pos by rounding to
        // the nearest cube-grid center : but that requires knowing the grid.
        //
        // Simplest correct approach : the cube is tiny (0.8 m edge) so the
        // FRAGMENT's world_pos is at most 0.4 m from the cube center. We can
        // get the center by projecting world_pos onto the back face of the
        // bounding cube using the view ray :
        //
        //   ray_origin = camera.world_pos
        //   ray_dir    = normalize(world_pos - camera_pos)
        //   The cube center is approximately :
        //     cube_center = world_pos - in.world_normal * 0.4 + ray_dir * something
        //
        // For a 0.8m cube, the center is along the inward-normal direction
        // by 0.4 m from each face fragment. world_normal is already the
        // cube-face's outward normal, so :
        let cube_center = in.world_pos - in.world_normal * 0.4;

        // Ray in world space.
        let cam = u.camera_pos.xyz;
        let rd_world = normalize(in.world_pos - cam);
        // In cube-local space (cube center at origin · axis-aligned), the
        // ray origin = cam - cube_center, ray direction unchanged.
        let ro_local = cam - cube_center;
        let rd_local = rd_world;

        let hit = raymarch(ro_local, rd_local, p.kind, p.scale, p.phase);
        if (!hit.hit) {
            // Miss : discard so the cube backface or scene-clear shows through.
            discard;
        }
        // Hit world-space position = cube_center + hit.pos (cube-local).
        let hit_world = cube_center + hit.pos;
        let n_local = sdf_normal(hit.pos, p.kind, p.scale, p.phase);
        // SDF gradient is already in cube-local axis-aligned space ;
        // for a non-rotated cube these axes match the world axes exactly.
        let n_world = n_local;

        // Lambert + ambient on the SDF-hit surface, using material albedo.
        let l = normalize(u.sun_dir.xyz);
        let n_dot_l = max(dot(n_world, l), 0.0);
        let diffuse = n_dot_l * 0.85;
        // Material albedo + a touch of pattern-driven hue (use the hit
        // position's normalised radius for a subtle radial tint).
        let r_local = clamp(length(hit.pos) * 1.5, 0.0, 1.0);
        var albedo = m.albedo * (0.7 + 0.3 * r_local) * in.base_color;

        // Material-specific overlays for raymarched stress objects :
        if (in.material_id == 4u) {
            // IRIDESCENT (mandelbulb) : view-angle-dependent rainbow film.
            let view_dir = -rd_world;
            let irid = pat_iridescent(vec2<f32>(r_local, 0.5), n_world, view_dir, u.time.x);
            albedo = mix(albedo, irid, 0.55);
        } else if (in.material_id == 7u) {
            // DICHROIC_VIOLET (menger) : holographic-style time shimmer.
            let holo = pat_holographic(vec2<f32>(r_local, 0.5), u.time.x);
            albedo = mix(albedo, holo, 0.45);
        }

        let lit = albedo * (u.ambient.xyz + diffuse);

        // Fresnel-flavored specular on SDF normal.
        let v = -rd_world;
        let h = normalize(l + v);
        let n_dot_h = max(dot(n_world, h), 0.0);
        let spec_pow = mix(8.0, 128.0, 1.0 - m.roughness);
        let spec = pow(n_dot_h, spec_pow) * (1.0 - m.roughness) * (0.20 + m.metallic * 0.80);

        let final_rgb = lit + vec3<f32>(spec) + m.emissive;
        // § T11-LOA-FID-STOKES : compute Mueller-applied Stokes and apply
        // the diagnostic polarization view (default mode 0 = identity).
        let s_out = stokes_for_material(in.material_id);
        let view_rgb = apply_polarization_view(final_rgb, s_out);
        out_color = vec4<f32>(view_rgb, m.alpha);
        out_depth = world_to_clip_depth(hit_world);
    } else {
        // ── Cube/UV branch : original path (16 textured procedurals) ──
        let n = normalize(in.world_normal);
        let view_dir = normalize(in.world_pos - u.camera_pos.xyz);

        let pat_col = procedural(p, in.uv, n, view_dir, u.time.x);

        var albedo = m.albedo * pat_col * in.base_color;
        if (in.material_id == 4u) {
            let irid = pat_iridescent(in.uv, n, view_dir, u.time.x);
            albedo = mix(albedo, irid, 0.6);
        } else if (in.material_id == 7u) {
            let holo = pat_holographic(in.uv, u.time.x);
            albedo = mix(albedo, holo, 0.7);
        }

        let l = normalize(u.sun_dir.xyz);
        let n_dot_l = max(dot(n, l), 0.0);
        let diffuse = n_dot_l * 0.85;
        let lit = albedo * (u.ambient.xyz + diffuse);

        let v = -view_dir;
        let h = normalize(l + v);
        let n_dot_h = max(dot(n, h), 0.0);
        let spec_pow = mix(8.0, 128.0, 1.0 - m.roughness);
        let spec = pow(n_dot_h, spec_pow) * (1.0 - m.roughness) * (0.20 + m.metallic * 0.80);

        let final_rgb = lit + vec3<f32>(spec) + m.emissive;
        // § T11-LOA-FID-STOKES : Mueller per-material + diagnostic view.
        let s_out = stokes_for_material(in.material_id);
        let view_rgb = apply_polarization_view(final_rgb, s_out);
        out_color = vec4<f32>(view_rgb, m.alpha);
        out_depth = rasterised_depth(in.world_pos);
    }

    var fout : FsOut;
    fout.color = out_color;
    fout.depth = out_depth;
    return fout;
}
