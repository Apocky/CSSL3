// § scene.wgsl — minimal vertex+fragment WGSL shader for the LoA test-room.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-LOA-HOST-1 (W-LOA-host-render) : per-vertex color · view+proj uniform ·
// directional sun-light Lambertian + ambient fill. Stage-0 simplicity ; richer
// shading lives in the cssl-spectral-render KAN-BRDF crate.
//
// § PIPELINE-LAYOUT
//   group(0) binding(0) : Uniforms { view_proj : mat4x4<f32>,
//                                    sun_dir   : vec4<f32> ,
//                                    ambient   : vec4<f32> }
//
// § VERTEX-LAYOUT
//   @location(0) position : vec3<f32>
//   @location(1) normal   : vec3<f32>
//   @location(2) color    : vec3<f32>

struct Uniforms {
    view_proj : mat4x4<f32>,
    // sun_dir.xyz = light direction (TOWARD the sun, normalized)
    // sun_dir.w   = padding
    sun_dir   : vec4<f32>,
    // ambient.xyz = ambient fill color (0..1)
    // ambient.w   = padding
    ambient   : vec4<f32>,
};

@group(0) @binding(0) var<uniform> u : Uniforms;

struct VsIn {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) color    : vec3<f32>,
};

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) world_normal   : vec3<f32>,
    @location(1) base_color     : vec3<f32>,
};

@vertex
fn vs_main(in : VsIn) -> VsOut {
    var out : VsOut;
    out.clip_pos     = u.view_proj * vec4<f32>(in.position, 1.0);
    out.world_normal = in.normal;
    out.base_color   = in.color;
    return out;
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let n        = normalize(in.world_normal);
    let l        = normalize(u.sun_dir.xyz);
    let n_dot_l  = max(dot(n, l), 0.0);
    let diffuse  = n_dot_l * 0.85;
    let lit      = in.base_color * (u.ambient.xyz + diffuse);
    // Gamma-ish output - simple 1.0 alpha; wgpu surface format handles sRGB.
    return vec4<f32>(lit, 1.0);
}
