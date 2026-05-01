// § tonemap.wgsl — ACES RRT+ODT tonemap fullscreen pass.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-LOA-FID-MAINSTREAM (W-LOA-fidelity-mainstream)
//
// § ROLE
//   Reads the HDR (Rgba16Float) intermediate target produced by the scene
//   pass, applies the ACES Reference Rendering Transform + Output Display
//   Transform (Stephen Hill's well-known fitted curve · public-domain),
//   then writes to the surface (BGRA8UnormSrgb). Renders as a single
//   fullscreen triangle — no vertex buffer, no index buffer, just three
//   `vertex_index` values producing big-triangle clip-space coverage.
//
// § ATTESTATION (PD)
//   No hurt nor harm in the making of this, to anyone/anything/anybody.

// ─────────────────────────────────────────────────────────────────────────
// § Bind group 0 — HDR intermediate texture + sampler
// ─────────────────────────────────────────────────────────────────────────

@group(0) @binding(0) var hdr_tex      : texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler  : sampler;

// ─────────────────────────────────────────────────────────────────────────
// § Vertex — fullscreen triangle (no VBO ; uses vertex_index)
// ─────────────────────────────────────────────────────────────────────────
//
//   For vertex_index = 0,1,2 the triangle covers clip-space such that the
//   visible region [-1, 1] x [-1, 1] is exactly contained. UV is computed
//   so the centre of the screen samples (0.5, 0.5) of the HDR texture.

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0)       uv       : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid : u32) -> VsOut {
    // Big-triangle trick : 3 verts at (-1,-1), (3,-1), (-1, 3) cover the
    // entire NDC square. UV is derived directly from clip-space.
    var out : VsOut;
    let x = f32((vid << 1u) & 2u);  // 0, 2, 0
    let y = f32(vid & 2u);          // 0, 0, 2
    out.clip_pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv       = vec2<f32>(x, y);
    return out;
}

// ─────────────────────────────────────────────────────────────────────────
// § Fragment — ACES RRT+ODT
// ─────────────────────────────────────────────────────────────────────────

// Stephen Hill's fitted ACES curve (public domain) — input in scene-linear,
// output in display-linear (gamma-correction is applied by the surface
// format `BGRA8UnormSrgb` automatically).
fn aces_rrt_odt(x: vec3<f32>) -> vec3<f32> {
    let a = (x * 2.51) + 0.03;
    let b = (x * (2.43 * x + 0.59)) + 0.14;
    return clamp((x * a) / b, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let hdr = textureSample(hdr_tex, hdr_sampler, in.uv);
    let mapped = aces_rrt_odt(hdr.rgb);
    return vec4<f32>(mapped, 1.0);
}
