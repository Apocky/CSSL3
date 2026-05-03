// § substrate_compose.wgsl — fullscreen-quad alpha-blend for the
//                            Substrate-Resonance Pixel Field.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-W18-A-COMPOSITE (W-T11-W18-A-REDUX) +
// § T11-W18-L9-AMOLED-DEEP (DEEP per-profile color-transform · 2026-05-02)
//
// § ROLE
//   Samples a 256×256 RGBA8 texture holding the Substrate-Resonance Pixel
//   Field (produced each frame by `substrate_render::tick`) and composes it
//   over the existing scene/tonemap output via alpha-blending. The compose
//   alpha is multiplied by `u.compose_ctl.x` (default 1.0 → substrate-paradigm
//   primary) so the player sees the substrate pixels as the primary visual.
//
// § PIPELINE (fragment-stage · per-pixel)
//   1. textureSample(substrate_tex, uv) → RGBA8 sRGB
//   2. AMOLED black-threshold gate (compose_ctl.y) :
//        sample.a < y → emit (0,0,0,0) · pure unlit AMOLED
//   3. Snap-to-zero luminance gate (display_ctl.x) :
//        rec709-luminance(sample.rgb) < x → emit (0,0,0,sample.a*overlay)
//        avoids backlight-glow that doesn't exist on AMOLED
//   4. AMOLED S-curve contrast (compose_ctl.z) per channel
//   5. HSV saturation-boost (display_ctl.y) :
//        rgb → hsv → S × boost → rgb
//   6. HDR PQ encoding (display_ctl.w == 1.0 · HdrExt only) :
//        Rec.709 sRGB → Rec.2020 → PQ EOTF (10000-nit normalized)
//   7. alpha = sample.a × overlay-strength
//
// § VS — big-triangle fullscreen (no VBO ; same trick as tonemap.wgsl)
// § FS — bilinear-sample texture · per-profile per-pixel transform
//
// § BIND GROUP 0
//   binding(0) : RGBA8 substrate texture (256×256 default)
//   binding(1) : linear-clamp sampler
//   binding(2) : ComposeUniforms — overlay + threshold + contrast + profile-id +
//                snap-to-zero + saturation-boost + peak-nits + is-hdr-flag
//
// § PRIME-DIRECTIVE attestation (PD)
//   No hurt nor harm in the making of this, to anyone/anything/anybody.

@group(0) @binding(0) var substrate_tex     : texture_2d<f32>;
@group(0) @binding(1) var substrate_sampler : sampler;

struct ComposeUniforms {
    // x = overlay strength (0..1)
    // y = AMOLED black-threshold : alpha below this → emit pure (0,0,0,0)
    //     Default 0.003 (≈ 1/300). Crucial on AMOLED/OLED/HDR-pitch-black
    //     displays where any non-zero RGB lights the pixel + costs power.
    // z = contrast S-curve strength (0 = linear · 1 = strong S-curve)
    //     Default 0.40. Pumps mid-tones · keeps black true-black + crushes
    //     near-black noise so substrate-pixels POP on dark backgrounds.
    // w = display-profile-id : 0 = AMOLED · 1 = OLED · 2 = IPS · 3 = VA · 4 = HDR-EXT
    compose_ctl : vec4<f32>,
    // § T11-W18-L9-AMOLED-DEEP — extended per-profile attributes :
    // x = snap-to-zero luminance threshold (pixels below → pure (0,0,0))
    //     AMOLED 0.003 · Oled 0.008 · Ips 0.020 · Va 0.012 · HdrExt 0.0001
    // y = saturation-boost (HSV-S × this · clamped 0..2)
    //     AMOLED 1.15 · Oled 1.08 · Ips 1.00 · Va 1.05 · HdrExt 1.20
    // z = peak-nits (HDR PQ-encode target · ignored for SDR)
    //     AMOLED 800 · Oled 600 · Ips 400 · Va 500 · HdrExt 1000
    // w = is-hdr flag (1.0 = Rec.2020 + PQ encoding · 0.0 = SDR sRGB)
    display_ctl : vec4<f32>,
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
    let s = cx * cx * (3.0 - 2.0 * cx);
    return mix(cx, s, clamp(c, 0.0, 1.0));
}

// Rec.709 luminance (used by snap-to-zero gate).
fn luminance_rec709(rgb : vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// RGB → HSV. Returns (h, s, v) ∈ [0,1]^3.
fn rgb_to_hsv(c : vec3<f32>) -> vec3<f32> {
    let max_c = max(c.r, max(c.g, c.b));
    let min_c = min(c.r, min(c.g, c.b));
    let d     = max_c - min_c;
    var h : f32 = 0.0;
    if (d > 1e-6) {
        if (max_c == c.r) {
            h = ((c.g - c.b) / d) / 6.0;
        } else if (max_c == c.g) {
            h = (((c.b - c.r) / d) + 2.0) / 6.0;
        } else {
            h = (((c.r - c.g) / d) + 4.0) / 6.0;
        }
        if (h < 0.0) { h = h + 1.0; }
    }
    let s = select(0.0, d / max_c, max_c > 1e-6);
    return vec3<f32>(h, s, max_c);
}

// HSV → RGB. Inverse of `rgb_to_hsv`.
fn hsv_to_rgb(c : vec3<f32>) -> vec3<f32> {
    let h6 = c.x * 6.0;
    let i  = floor(h6);
    let f  = h6 - i;
    let p  = c.z * (1.0 - c.y);
    let q  = c.z * (1.0 - c.y * f);
    let t  = c.z * (1.0 - c.y * (1.0 - f));
    let m  = i32(i) % 6;
    switch (m) {
        case 0:  { return vec3<f32>(c.z, t, p); }
        case 1:  { return vec3<f32>(q, c.z, p); }
        case 2:  { return vec3<f32>(p, c.z, t); }
        case 3:  { return vec3<f32>(p, q, c.z); }
        case 4:  { return vec3<f32>(t, p, c.z); }
        default: { return vec3<f32>(c.z, p, q); }
    }
}

// Apply HSV-saturation boost. boost = 1.0 = identity. Clamped to [0, 2].
fn boost_saturation(rgb : vec3<f32>, boost : f32) -> vec3<f32> {
    let b = clamp(boost, 0.0, 2.0);
    if (abs(b - 1.0) < 1e-4) {
        return rgb;
    }
    var hsv = rgb_to_hsv(rgb);
    hsv.y = clamp(hsv.y * b, 0.0, 1.0);
    return hsv_to_rgb(hsv);
}

// Rec.709 → Rec.2020 primaries matrix (BT.2087-0).
fn rec709_to_rec2020(rgb : vec3<f32>) -> vec3<f32> {
    let m = mat3x3<f32>(
        vec3<f32>(0.6274, 0.0691, 0.0164),
        vec3<f32>(0.3293, 0.9195, 0.0880),
        vec3<f32>(0.0433, 0.0114, 0.8956),
    );
    return m * rgb;
}

// PQ EOTF (SMPTE ST 2084) inverse · linear-light → PQ-encoded.
// Input rgb is in linear-light normalized to peak-nits (so 1.0 = peak-nits).
// Output is the 10-bit code-value normalized to [0,1].
fn pq_eotf_inverse(linear_rgb : vec3<f32>) -> vec3<f32> {
    let m1 = 0.1593017578125;     // 2610/16384
    let m2 = 78.84375;            // 2523/4096 * 128
    let c1 = 0.8359375;           // 3424/4096
    let c2 = 18.8515625;          // 2413/4096 * 32
    let c3 = 18.6875;             // 2392/4096 * 32
    let lp = pow(max(linear_rgb, vec3<f32>(0.0)), vec3<f32>(m1));
    let num = c1 + c2 * lp;
    let den = vec3<f32>(1.0) + c3 * lp;
    return pow(num / den, vec3<f32>(m2));
}

// HDR-PQ encode : SDR-sRGB → Rec.2020 → scale-to-peak-nits → PQ-EOTF-inverse.
fn hdr_pq_encode(sdr_srgb : vec3<f32>, peak_nits : f32) -> vec3<f32> {
    let wide = rec709_to_rec2020(sdr_srgb);
    // PQ EOTF reference is 10000 nits → normalize peak-nits / 10000.
    let scale = clamp(peak_nits / 10000.0, 0.0001, 1.0);
    return pq_eotf_inverse(wide * scale);
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let sample = textureSample(substrate_tex, substrate_sampler, in.uv);
    let strength      = clamp(u.compose_ctl.x, 0.0, 1.0);
    let black_thresh  = clamp(u.compose_ctl.y, 0.0, 1.0);
    let contrast      = clamp(u.compose_ctl.z, 0.0, 1.0);
    let snap_thr      = clamp(u.display_ctl.x, 0.0, 1.0);
    let sat_boost     = clamp(u.display_ctl.y, 0.0, 2.0);
    let peak_nits     = clamp(u.display_ctl.z, 50.0, 10000.0);
    let is_hdr        = u.display_ctl.w >= 0.5;

    // § AMOLED true-black gate : sub-threshold alpha → emit nothing at all.
    if (sample.a < black_thresh) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // § S-curve contrast per channel : substrate-pixels POP on pitch-black.
    var rgb = vec3<f32>(
        amoled_s_curve(sample.r, contrast),
        amoled_s_curve(sample.g, contrast),
        amoled_s_curve(sample.b, contrast),
    );

    // § Snap-to-zero luminance gate : pixels with luminance below threshold
    //   clamp to pure (0,0,0). On AMOLED this preserves the unlit-pixel =
    //   zero-power physics (no backlight-glow leakage). The alpha is
    //   PRESERVED (so the substrate pixel still contributes to the dst
    //   coverage) but the RGB is forced to zero.
    let lum = luminance_rec709(rgb);
    if (lum < snap_thr) {
        rgb = vec3<f32>(0.0, 0.0, 0.0);
    }

    // § Per-profile saturation-boost (HSV-S × boost · clamped).
    rgb = boost_saturation(rgb, sat_boost);

    // § HDR PQ encoding (Rec.2020 + PQ EOTF) when display is HdrExt.
    if (is_hdr) {
        rgb = hdr_pq_encode(rgb, peak_nits);
    }

    let a = sample.a * strength;
    return vec4<f32>(rgb, a);
}
