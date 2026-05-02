// § substrate_compose.wgsl — fullscreen-quad alpha-blend for the
//                            Substrate-Resonance Pixel Field.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-W18-A-COMPOSITE (W-T11-W18-A-REDUX)
//
// § ROLE
//   Samples a 256×256 RGBA8 texture holding the Substrate-Resonance Pixel
//   Field (produced each frame by `substrate_render::tick`) and composes it
//   over the existing scene/tonemap output via alpha-blending. The compose
//   alpha is multiplied by `u.compose_ctl.x` (default 0.50 → 50%-overlay)
//   so the player sees BOTH the conventional 3D test-room AND the substrate
//   pixels at the same time — proving the paradigm is live.
//
// § VS — big-triangle fullscreen (no VBO ; same trick as tonemap.wgsl)
// § FS — bilinear-sample texture · multiply alpha by overlay strength
//
// § BIND GROUP 0
//   binding(0) : RGBA8 substrate texture (256×256 default)
//   binding(1) : linear-clamp sampler
//   binding(2) : ComposeUniforms — overlay strength + viewport-aspect bias
//
// § PRIME-DIRECTIVE attestation (PD)
//   No hurt nor harm in the making of this, to anyone/anything/anybody.

@group(0) @binding(0) var substrate_tex     : texture_2d<f32>;
@group(0) @binding(1) var substrate_sampler : sampler;

struct ComposeUniforms {
    // x = overlay strength (0..1)
    // y = AMOLED black-threshold : alpha below this → emit pure (0,0,0,0)
    //     Default 0.04 (≈ 10/255). Crucial on AMOLED/OLED/HDR-pitch-black
    //     displays where any non-zero RGB lights the pixel + costs power.
    // z = contrast S-curve strength (0 = linear · 1 = strong S-curve)
    //     Default 0.35. Pumps mid-tones · keeps black true-black + crushes
    //     near-black noise so substrate-pixels POP on dark backgrounds.
    // w = display-profile-id : 0 = AMOLED · 1 = OLED · 2 = IPS · 3 = VA · 4 = HDR-EXT
    //     Reserved for future per-profile gamut adjustment.
    compose_ctl : vec4<f32>,
};

@group(0) @binding(2) var<uniform> u : ComposeUniforms;

struct VsOut {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0)       uv       : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid : u32) -> VsOut {
    // Big-triangle trick : 3 verts at (-1,-1), (3,-1), (-1, 3) cover NDC.
    var out : VsOut;
    let x = f32((vid << 1u) & 2u); // 0, 2, 0
    let y = f32(vid & 2u);         // 0, 0, 2
    out.clip_pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv       = vec2<f32>(x, y);
    return out;
}

// AMOLED-aware S-curve : pushes mid-tones up · keeps black at zero
// emission (no leakage). `c` ∈ [0,1] is contrast strength. Identity at c=0.
fn amoled_s_curve(x : f32, c : f32) -> f32 {
    let cx = clamp(x, 0.0, 1.0);
    // smoothstep gives a clean S-curve through (0,0)..(1,1) ; mix with
    // identity controls strength. Pure pitch-black at x=0 ALWAYS preserved.
    let s = cx * cx * (3.0 - 2.0 * cx);
    return mix(cx, s, clamp(c, 0.0, 1.0));
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let sample = textureSample(substrate_tex, substrate_sampler, in.uv);
    let strength      = clamp(u.compose_ctl.x, 0.0, 1.0);
    let black_thresh  = clamp(u.compose_ctl.y, 0.0, 1.0);
    let contrast      = clamp(u.compose_ctl.z, 0.0, 1.0);

    // § AMOLED true-black gate : sub-threshold alpha → emit nothing at all.
    //   On AMOLED/OLED any non-zero RGB lights the pixel. Sub-threshold
    //   substrate-confidence is treated as background · pure (0,0,0,0).
    if (sample.a < black_thresh) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // § S-curve contrast per channel : substrate-pixels POP on pitch-black.
    let r = amoled_s_curve(sample.r, contrast);
    let g = amoled_s_curve(sample.g, contrast);
    let b = amoled_s_curve(sample.b, contrast);

    let a = sample.a * strength;
    return vec4<f32>(r, g, b, a);
}
