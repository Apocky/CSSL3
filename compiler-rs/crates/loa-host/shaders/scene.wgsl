// § scene.wgsl — uber-shader for the diagnostic-dense LoA-v13 test-room.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-LOA-RICH-RENDER (W-LOA-rich-render-overhaul)
//
// § ROLE
//   Uber-shader supporting :
//     - Per-vertex material_id + pattern_id indirection into LUTs
//     - 16+ procedural patterns computed analytically in the fragment shader
//     - Material BRDF approximation (lambert + ambient + fresnel-flavored
//       specular highlight + emissive)
//     - Time-driven holographic / iridescent / dichroic effects
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
//   ambient + time + 16-entry material LUT + 16-entry pattern LUT).
//   Total size : 80 + 16 × 32 + 16 × 16 = 80 + 512 + 256 = 848 bytes
//   (well under the 16 KiB UBO limit on every wgpu backend).

struct Material {
    albedo:    vec3<f32>,
    roughness: f32,
    emissive:  vec3<f32>,
    metallic:  f32,
    alpha:     f32,
    _pad:      vec3<f32>,
};

struct Pattern {
    kind:     u32,
    scale:    f32,
    rotation: f32,
    phase:    f32,
};

struct Uniforms {
    view_proj : mat4x4<f32>,
    sun_dir   : vec4<f32>,
    ambient   : vec4<f32>,
    // time.x = seconds since render start (drives time-varying patterns)
    // time.y = frame counter (modulo 1e6) cast to f32
    // time.zw = unused
    time      : vec4<f32>,
    materials : array<Material, 16>,
    patterns  : array<Pattern, 16>,
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
// § Fragment shader — material LUT + procedural pattern + lighting
// ─────────────────────────────────────────────────────────────────────────

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let m = u.materials[in.material_id];
    let p = u.patterns[in.pattern_id];

    let n = normalize(in.world_normal);
    // Rough view-dir approx : assume camera at (0,1.55,0) looking forward ;
    // the vertex world_pos minus camera approximates view direction. The
    // sun_dir uniform is normalized.
    let view_dir = normalize(in.world_pos - vec3<f32>(0.0, 1.55, 0.0));

    // Procedural base color — multiplies into the material albedo.
    let pat_col = procedural(p, in.uv, n, view_dir, u.time.x);

    // Special-case : iridescent + holographic materials use the time-driven
    // pattern functions rather than reading p.
    var albedo = m.albedo * pat_col * in.base_color;
    if (in.material_id == 4u) {
        // IRIDESCENT — overlay irid color
        let irid = pat_iridescent(in.uv, n, view_dir, u.time.x);
        albedo = mix(albedo, irid, 0.6);
    } else if (in.material_id == 7u) {
        // HOLOGRAPHIC — overlay holo color
        let holo = pat_holographic(in.uv, u.time.x);
        albedo = mix(albedo, holo, 0.7);
    }

    // Lambert + ambient.
    let l = normalize(u.sun_dir.xyz);
    let n_dot_l = max(dot(n, l), 0.0);
    let diffuse = n_dot_l * 0.85;
    let lit = albedo * (u.ambient.xyz + diffuse);

    // Fresnel-flavored specular highlight (cheap approximation).
    let v = -view_dir;
    let h = normalize(l + v);
    let n_dot_h = max(dot(n, h), 0.0);
    let spec_pow = mix(8.0, 128.0, 1.0 - m.roughness);
    let spec = pow(n_dot_h, spec_pow) * (1.0 - m.roughness) * (0.20 + m.metallic * 0.80);

    let final_rgb = lit + vec3<f32>(spec) + m.emissive;
    return vec4<f32>(final_rgb, m.alpha);
}
