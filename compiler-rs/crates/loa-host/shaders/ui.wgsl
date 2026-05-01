// § ui.wgsl — textured-quad overlay shader for the LoA HUD + menu.
// ════════════════════════════════════════════════════════════════════════════
//
// § T11-LOA-HUD (W-LOA-hud-overlay) : 2D screen-space pass run AFTER the
// scene pass each frame. Rasterizes packed `UiVertex` quads sampled from
// an 8x8 grayscale font atlas, plus solid-fill quads (separated by a
// `kind` flag in the vertex). Alpha blend ON, depth test OFF.
//
// § PIPELINE LAYOUT
//   group(0) binding(0) : Screen { size : vec2<f32>, _pad : vec2<f32> }
//   group(0) binding(1) : font_tex  (texture_2d<f32> ; R-channel = alpha)
//   group(0) binding(2) : font_samp (sampler ; nearest-neighbor)
//
// § VERTEX LAYOUT (matches loa-host/src/ui_overlay.rs UiVertex)
//   @location(0) pos_px : vec2<f32>   // pixel-space, top-left origin
//   @location(1) uv     : vec2<f32>   // 0..1 in font atlas space
//   @location(2) color  : vec4<f32>   // tint multiplied with sample
//   @location(3) kind   : f32         // 0.0 = font (sample R as alpha) ;
//                                     // 1.0 = solid (ignore texture)

struct Screen {
    size : vec2<f32>,
    _pad : vec2<f32>,
};

@group(0) @binding(0) var<uniform> screen : Screen;
@group(0) @binding(1) var font_tex  : texture_2d<f32>;
@group(0) @binding(2) var font_samp : sampler;

struct VsIn {
    @location(0) pos_px : vec2<f32>,
    @location(1) uv     : vec2<f32>,
    @location(2) color  : vec4<f32>,
    @location(3) kind   : f32,
};

struct VsOut {
    @builtin(position) clip : vec4<f32>,
    @location(0) uv         : vec2<f32>,
    @location(1) color      : vec4<f32>,
    @location(2) kind       : f32,
};

@vertex
fn vs_main(in : VsIn) -> VsOut {
    var out : VsOut;
    // Convert top-left-origin pixel coordinates to NDC ([-1,1] with Y-up).
    let nx =  (in.pos_px.x / screen.size.x) * 2.0 - 1.0;
    let ny = -((in.pos_px.y / screen.size.y) * 2.0 - 1.0);
    out.clip  = vec4<f32>(nx, ny, 0.0, 1.0);
    out.uv    = in.uv;
    out.color = in.color;
    out.kind  = in.kind;
    return out;
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    if (in.kind > 0.5) {
        // Solid fill : color as-is. Useful for menu panels + crosshair.
        return in.color;
    }
    // Font path : sample R channel, multiply alpha into tint.
    let sample = textureSample(font_tex, font_samp, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * sample);
}
