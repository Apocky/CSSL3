// § cfer.wgsl — CFER volumetric raymarcher (substrate-IS-renderer).
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-LOA-FID-CFER (W-LOA-fidelity-cfer)
//
// § ROLE
//   Volumetric raymarcher that samples the CFER 3D texture (sourced from
//   the OmegaField) along view rays, accumulates radiance, and outputs an
//   alpha-blendable color. Drawn AFTER the triangle scene (depth-tested
//   against scene depth, no depth-write) so triangles + atmosphere coexist.
//
// § PIPELINE
//   1. Vertex stage : full-screen triangle (NDC sweep)
//   2. Fragment stage : reconstruct view ray from NDC + camera_pos, march
//      through the world envelope sampling the 3D texture, accumulate
//      front-to-back alpha-compositing, output (rgb, alpha).
//
// § BIND GROUP 0 — Uniforms
//   binding 0  : Uniforms (inv_view_proj + camera_pos + world_min + world_max + time)
//   binding 1  : 3D texture (rgba16f, world envelope packed at 32×16×32)
//   binding 2  : sampler (linear-clamp)
//
// § ALPHA BLEND CONTRACT
//   The CPU-side pipeline-builder applies BlendState::ALPHA_BLENDING :
//     dst.rgb = src.rgb * src.a + dst.rgb * (1 - src.a)
//     dst.a   = src.a + dst.a * (1 - src.a)
//   So the fragment outputs PRE-MULTIPLIED radiance × alpha is implicit ;
//   we output (rgb, alpha) and the blend equation handles the comp.

struct Uniforms {
    // World-to-NDC inverse for ray reconstruction.
    inv_view_proj : mat4x4<f32>,
    // World-space camera position. .xyz = pos ; .w = unused.
    camera_pos    : vec4<f32>,
    // World envelope minimum corner (matches cfer_render::WORLD_MIN).
    world_min     : vec4<f32>,
    // World envelope maximum corner (matches cfer_render::WORLD_MAX).
    world_max     : vec4<f32>,
    // time.x = seconds since render start (drives time-varying effects).
    // time.y = unused
    // time.z = step-count override (clamped to 1..64 ; default 32)
    // time.w = unused
    time          : vec4<f32>,
};

@group(0) @binding(0) var<uniform> u : Uniforms;
@group(0) @binding(1) var cfer_tex : texture_3d<f32>;
@group(0) @binding(2) var cfer_samp : sampler;

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0)       ndc      : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx : u32) -> VsOut {
    // Full-screen triangle (3 verts) — covers NDC [-1, 1]^2 with one tri.
    //   v0 : (-1, -1)
    //   v1 : ( 3, -1)
    //   v2 : (-1,  3)
    var pos : vec2<f32>;
    if (idx == 0u) {
        pos = vec2<f32>(-1.0, -1.0);
    } else if (idx == 1u) {
        pos = vec2<f32>( 3.0, -1.0);
    } else {
        pos = vec2<f32>(-1.0,  3.0);
    }
    var out : VsOut;
    out.clip_pos = vec4<f32>(pos, 0.0, 1.0);
    out.ndc      = pos;
    return out;
}

// Output of `reconstruct_view_ray` — both the world-space origin + the
// normalized view direction. A struct is required because WGSL forbids
// nested vec types like vec2<vec3<f32>>.
struct Ray {
    origin    : vec3<f32>,
    direction : vec3<f32>,
};

// Reconstruct a world-space view ray from an NDC coordinate.
fn reconstruct_view_ray(ndc : vec2<f32>) -> Ray {
    // NDC -> world : sample at depth=0 (near plane) and depth=1 (far plane),
    // direction is the difference normalized.
    let near_clip = vec4<f32>(ndc, 0.0, 1.0);
    let far_clip  = vec4<f32>(ndc, 1.0, 1.0);
    let near_h = u.inv_view_proj * near_clip;
    let far_h  = u.inv_view_proj * far_clip;
    let near_w = near_h.xyz / max(near_h.w, 1e-6);
    let far_w  = far_h.xyz  / max(far_h.w,  1e-6);
    let dir = normalize(far_w - near_w);
    var r : Ray;
    r.origin    = u.camera_pos.xyz;
    r.direction = dir;
    return r;
}

// Compute the t-range of the ray inside the world envelope AABB.
// Returns (t_enter, t_exit) ; t_exit < t_enter ⇒ no intersection.
fn ray_aabb(ro : vec3<f32>, rd : vec3<f32>) -> vec2<f32> {
    let inv_d = 1.0 / max(abs(rd), vec3<f32>(1e-6)) * sign(rd);
    let t1 = (u.world_min.xyz - ro) * inv_d;
    let t2 = (u.world_max.xyz - ro) * inv_d;
    let tmin3 = min(t1, t2);
    let tmax3 = max(t1, t2);
    let t_enter = max(max(tmin3.x, tmin3.y), tmin3.z);
    let t_exit  = min(min(tmax3.x, tmax3.y), tmax3.z);
    return vec2<f32>(max(t_enter, 0.0), t_exit);
}

// Sample the CFER 3D texture at a world-space point. Returns rgba.
// Returns vec4(0) if the sample is outside the envelope.
fn sample_cfer(p : vec3<f32>) -> vec4<f32> {
    let span = u.world_max.xyz - u.world_min.xyz;
    let uvw = (p - u.world_min.xyz) / max(span, vec3<f32>(1e-6));
    if (any(uvw < vec3<f32>(0.0)) || any(uvw > vec3<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    return textureSampleLevel(cfer_tex, cfer_samp, uvw, 0.0);
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let ray = reconstruct_view_ray(in.ndc);
    let ro  = ray.origin;
    let rd  = ray.direction;

    let t_range = ray_aabb(ro, rd);
    if (t_range.y <= t_range.x) {
        // Ray missed the world envelope — return transparent.
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Number of march steps. 32 is the budget-friendly default ; the CPU
    // can override via time.z (clamped 1..64).
    var steps : i32 = 32;
    if (u.time.z > 0.5) {
        steps = clamp(i32(u.time.z), 1, 64);
    }

    let span_t = t_range.y - t_range.x;
    let dt = span_t / f32(steps);

    var accum_rgb = vec3<f32>(0.0);
    var accum_a   = 0.0;

    for (var i : i32 = 0; i < steps; i = i + 1) {
        if (accum_a > 0.99) {
            break; // saturated — early-out
        }
        let t = t_range.x + (f32(i) + 0.5) * dt;
        let p = ro + rd * t;
        let s = sample_cfer(p);
        // Front-to-back alpha-composite.
        let a_local = s.a * dt * 0.5; // density × step-thickness
        accum_rgb = accum_rgb + (1.0 - accum_a) * s.rgb * a_local;
        accum_a   = accum_a   + (1.0 - accum_a) * a_local;
    }

    // Output is straight-alpha (the CPU-side pipeline applies ALPHA_BLENDING).
    return vec4<f32>(accum_rgb, accum_a);
}
