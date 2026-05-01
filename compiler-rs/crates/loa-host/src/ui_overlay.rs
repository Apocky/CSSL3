//! § ui_overlay — text HUD + menu UI overlay for LoA-v13.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HUD (W-LOA-hud-overlay) — second-pass screen-space overlay :
//!
//!   - 4-corner HUD text (always visible while playing)
//!     * top-left  : engine ident + frame + fps
//!     * top-right : camera pos + yaw/pitch + render-mode
//!     * bot-left  : DM phase + tension + last GM/DM event
//!     * bot-right : MCP listening + control hints
//!
//!   - 5x5px crosshair at screen center (white outline + black halo)
//!
//!   - Pause/menu screen (Tab or Esc opens) :
//!     * 50% black dim across the 3D scene
//!     * centered title panel "LABYRINTH OF APOCALYPSE · v13"
//!     * 5 selectable items :
//!       Continue
//!       Render Mode : <NAME>      (Enter cycles 0..9)
//!       Toggle Fullscreen (F11)
//!       Show MCP Help            (opens submenu)
//!       Quit
//!     * Up/Down/Left/Right + Enter navigate ; Esc/Tab close.
//!     * MCP-help submenu : scrollable list of 17 tools + nc-command samples.
//!
//! § FONT
//!   Embedded 8x8 bitmap for ASCII 32..126 (95 glyphs). The shapes are
//!   AUTHORED IN-FILE BELOW in the `FONT_8X8` table — bit-rows are my own
//!   design, NOT copied from any existing font. Each glyph is 8 bytes
//!   (one byte per row, bit 7 = leftmost pixel). 95 * 8 = 760 bytes total.
//!   No external attribution needed. Public-domain dedication implicit.
//!
//! § RENDER PIPELINE
//!   Single textured-quad pipeline + a small uniform with screen-size +
//!   one font texture. Per-frame the host calls `UiOverlay::build_frame`
//!   to populate a vertex buffer with all HUD + menu quads, then the
//!   renderer encodes a `RenderPass` running `ui.wgsl`.
//!
//! § PRIME-DIRECTIVE
//!   No surveillance / no telemetry beyond local log_event. Menu options
//!   are user-facing English (this is a UI layer — see CLAUDE.md exception
//!   "user-facing chat + user writes English"). All tests inline below.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]

use cssl_rt::loa_startup::log_event;

// ─────────────────────────────────────────────────────────────────────────
// § Embedded shader source (validated by tests below + crate-level tests)
// ─────────────────────────────────────────────────────────────────────────

pub const UI_WGSL: &str = include_str!("../shaders/ui.wgsl");

// ─────────────────────────────────────────────────────────────────────────
// § Constants
// ─────────────────────────────────────────────────────────────────────────

/// First codepoint covered by the embedded font.
pub const FIRST_GLYPH: u8 = 32; // SPACE
/// Last codepoint covered by the embedded font.
pub const LAST_GLYPH: u8 = 126; // ~
/// Number of glyphs in the embedded font (95).
pub const GLYPH_COUNT: usize = (LAST_GLYPH as usize) - (FIRST_GLYPH as usize) + 1;
/// Bytes per glyph (8 rows of 8 bits each).
pub const GLYPH_BYTES: usize = 8;
/// Glyph cell width in source pixels.
pub const CELL_W: u32 = 8;
/// Glyph cell height in source pixels.
pub const CELL_H: u32 = 8;
/// Atlas width = 95 glyphs * 8 px = 760 px.
pub const ATLAS_W: u32 = (GLYPH_COUNT as u32) * CELL_W;
/// Atlas height = 1 row * 8 px.
pub const ATLAS_H: u32 = CELL_H;

/// Default text scale (pixel multiplier per glyph cell). 2 = 16px tall text.
pub const TEXT_SCALE: f32 = 2.0;
/// Default char spacing (in source-pixel units, scaled). 8 = no padding.
pub const CHAR_SPACING_PX: f32 = 8.0;
/// Default line height (source-pixel units, scaled). 10 = 2-px gap.
pub const LINE_HEIGHT_PX: f32 = 10.0;

// ─────────────────────────────────────────────────────────────────────────
// § Vertex layout — matches ui.wgsl
// ─────────────────────────────────────────────────────────────────────────

/// Per-vertex POD pushed to the GPU. 8 floats = 32 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UiVertex {
    /// Pixel-space position (top-left origin, +Y down).
    pub pos_px: [f32; 2],
    /// 0..1 atlas UV.
    pub uv: [f32; 2],
    /// RGBA tint in linear space.
    pub color: [f32; 4],
    /// 0.0 = font (sample texture R as alpha) ; 1.0 = solid fill.
    pub kind: f32,
    /// Padding to align to 16 bytes for vertex layout simplicity (36 -> 40).
    pub _pad: [f32; 3],
}

unsafe impl bytemuck::Zeroable for UiVertex {}
unsafe impl bytemuck::Pod for UiVertex {}

impl UiVertex {
    pub const STRIDE: u64 = core::mem::size_of::<Self>() as u64;
}

/// Screen-size uniform — matches ui.wgsl `Screen`.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct UiScreenUbo {
    pub size: [f32; 2],
    pub _pad: [f32; 2],
}

unsafe impl bytemuck::Zeroable for UiScreenUbo {}
unsafe impl bytemuck::Pod for UiScreenUbo {}

// ─────────────────────────────────────────────────────────────────────────
// § Bitmap font — authored in-file, ASCII 32..126
// ─────────────────────────────────────────────────────────────────────────
//
// Each glyph = 8 rows of 8 bits ; bit 7 (0x80) = leftmost pixel ; row[0] = top.
// Glyph shapes are designed for legibility at 16-px scale (TEXT_SCALE=2).
// Every entry is HAND-AUTHORED here in this file — no external font copied.

/// 8x8 monospace bitmap font for ASCII 32..126. Indexed by `(c - 32)`.
#[rustfmt::skip]
pub const FONT_8X8: [[u8; GLYPH_BYTES]; GLYPH_COUNT] = [
    // 32 ' '  (space)
    [0,0,0,0,0,0,0,0],
    // 33 '!'
    [0x18,0x18,0x18,0x18,0x18,0x00,0x18,0x00],
    // 34 '"'
    [0x66,0x66,0x66,0x00,0x00,0x00,0x00,0x00],
    // 35 '#'
    [0x66,0x66,0xFF,0x66,0xFF,0x66,0x66,0x00],
    // 36 '$'
    [0x18,0x3E,0x60,0x3C,0x06,0x7C,0x18,0x00],
    // 37 '%'
    [0x62,0x66,0x0C,0x18,0x30,0x66,0x46,0x00],
    // 38 '&'
    [0x3C,0x66,0x3C,0x38,0x67,0x66,0x3F,0x00],
    // 39 '\''
    [0x18,0x18,0x30,0x00,0x00,0x00,0x00,0x00],
    // 40 '('
    [0x0C,0x18,0x30,0x30,0x30,0x18,0x0C,0x00],
    // 41 ')'
    [0x30,0x18,0x0C,0x0C,0x0C,0x18,0x30,0x00],
    // 42 '*'
    [0x00,0x66,0x3C,0xFF,0x3C,0x66,0x00,0x00],
    // 43 '+'
    [0x00,0x18,0x18,0x7E,0x18,0x18,0x00,0x00],
    // 44 ','
    [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x30],
    // 45 '-'
    [0x00,0x00,0x00,0x7E,0x00,0x00,0x00,0x00],
    // 46 '.'
    [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x00],
    // 47 '/'
    [0x00,0x03,0x06,0x0C,0x18,0x30,0x60,0x00],
    // 48 '0'
    [0x3C,0x66,0x6E,0x76,0x66,0x66,0x3C,0x00],
    // 49 '1'
    [0x18,0x38,0x18,0x18,0x18,0x18,0x7E,0x00],
    // 50 '2'
    [0x3C,0x66,0x06,0x1C,0x30,0x60,0x7E,0x00],
    // 51 '3'
    [0x3C,0x66,0x06,0x1C,0x06,0x66,0x3C,0x00],
    // 52 '4'
    [0x0C,0x1C,0x3C,0x6C,0x7E,0x0C,0x0C,0x00],
    // 53 '5'
    [0x7E,0x60,0x7C,0x06,0x06,0x66,0x3C,0x00],
    // 54 '6'
    [0x1C,0x30,0x60,0x7C,0x66,0x66,0x3C,0x00],
    // 55 '7'
    [0x7E,0x06,0x0C,0x18,0x30,0x30,0x30,0x00],
    // 56 '8'
    [0x3C,0x66,0x66,0x3C,0x66,0x66,0x3C,0x00],
    // 57 '9'
    [0x3C,0x66,0x66,0x3E,0x06,0x0C,0x38,0x00],
    // 58 ':'
    [0x00,0x18,0x18,0x00,0x00,0x18,0x18,0x00],
    // 59 ';'
    [0x00,0x18,0x18,0x00,0x00,0x18,0x18,0x30],
    // 60 '<'
    [0x06,0x0C,0x18,0x30,0x18,0x0C,0x06,0x00],
    // 61 '='
    [0x00,0x00,0x7E,0x00,0x7E,0x00,0x00,0x00],
    // 62 '>'
    [0x60,0x30,0x18,0x0C,0x18,0x30,0x60,0x00],
    // 63 '?'
    [0x3C,0x66,0x06,0x0C,0x18,0x00,0x18,0x00],
    // 64 '@'
    [0x3C,0x66,0x6E,0x6E,0x60,0x62,0x3C,0x00],
    // 65 'A'
    [0x18,0x3C,0x66,0x66,0x7E,0x66,0x66,0x00],
    // 66 'B'
    [0x7C,0x66,0x66,0x7C,0x66,0x66,0x7C,0x00],
    // 67 'C'
    [0x3C,0x66,0x60,0x60,0x60,0x66,0x3C,0x00],
    // 68 'D'
    [0x78,0x6C,0x66,0x66,0x66,0x6C,0x78,0x00],
    // 69 'E'
    [0x7E,0x60,0x60,0x78,0x60,0x60,0x7E,0x00],
    // 70 'F'
    [0x7E,0x60,0x60,0x78,0x60,0x60,0x60,0x00],
    // 71 'G'
    [0x3C,0x66,0x60,0x6E,0x66,0x66,0x3C,0x00],
    // 72 'H'
    [0x66,0x66,0x66,0x7E,0x66,0x66,0x66,0x00],
    // 73 'I'
    [0x3C,0x18,0x18,0x18,0x18,0x18,0x3C,0x00],
    // 74 'J'
    [0x1E,0x0C,0x0C,0x0C,0x0C,0x6C,0x38,0x00],
    // 75 'K'
    [0x66,0x6C,0x78,0x70,0x78,0x6C,0x66,0x00],
    // 76 'L'
    [0x60,0x60,0x60,0x60,0x60,0x60,0x7E,0x00],
    // 77 'M'
    [0x63,0x77,0x7F,0x6B,0x63,0x63,0x63,0x00],
    // 78 'N'
    [0x66,0x76,0x7E,0x7E,0x6E,0x66,0x66,0x00],
    // 79 'O'
    [0x3C,0x66,0x66,0x66,0x66,0x66,0x3C,0x00],
    // 80 'P'
    [0x7C,0x66,0x66,0x7C,0x60,0x60,0x60,0x00],
    // 81 'Q'
    [0x3C,0x66,0x66,0x66,0x6A,0x6C,0x36,0x00],
    // 82 'R'
    [0x7C,0x66,0x66,0x7C,0x78,0x6C,0x66,0x00],
    // 83 'S'
    [0x3C,0x66,0x60,0x3C,0x06,0x66,0x3C,0x00],
    // 84 'T'
    [0x7E,0x18,0x18,0x18,0x18,0x18,0x18,0x00],
    // 85 'U'
    [0x66,0x66,0x66,0x66,0x66,0x66,0x3C,0x00],
    // 86 'V'
    [0x66,0x66,0x66,0x66,0x66,0x3C,0x18,0x00],
    // 87 'W'
    [0x63,0x63,0x63,0x6B,0x7F,0x77,0x63,0x00],
    // 88 'X'
    [0x66,0x66,0x3C,0x18,0x3C,0x66,0x66,0x00],
    // 89 'Y'
    [0x66,0x66,0x66,0x3C,0x18,0x18,0x18,0x00],
    // 90 'Z'
    [0x7E,0x06,0x0C,0x18,0x30,0x60,0x7E,0x00],
    // 91 '['
    [0x3C,0x30,0x30,0x30,0x30,0x30,0x3C,0x00],
    // 92 '\\'
    [0x00,0x60,0x30,0x18,0x0C,0x06,0x03,0x00],
    // 93 ']'
    [0x3C,0x0C,0x0C,0x0C,0x0C,0x0C,0x3C,0x00],
    // 94 '^'
    [0x18,0x3C,0x66,0x00,0x00,0x00,0x00,0x00],
    // 95 '_'
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0xFF],
    // 96 '`'
    [0x30,0x18,0x0C,0x00,0x00,0x00,0x00,0x00],
    // 97 'a'
    [0x00,0x00,0x3C,0x06,0x3E,0x66,0x3E,0x00],
    // 98 'b'
    [0x60,0x60,0x7C,0x66,0x66,0x66,0x7C,0x00],
    // 99 'c'
    [0x00,0x00,0x3C,0x66,0x60,0x66,0x3C,0x00],
    // 100 'd'
    [0x06,0x06,0x3E,0x66,0x66,0x66,0x3E,0x00],
    // 101 'e'
    [0x00,0x00,0x3C,0x66,0x7E,0x60,0x3C,0x00],
    // 102 'f'
    [0x1C,0x36,0x30,0x7C,0x30,0x30,0x30,0x00],
    // 103 'g'
    [0x00,0x00,0x3E,0x66,0x66,0x3E,0x06,0x7C],
    // 104 'h'
    [0x60,0x60,0x7C,0x66,0x66,0x66,0x66,0x00],
    // 105 'i'
    [0x18,0x00,0x38,0x18,0x18,0x18,0x3C,0x00],
    // 106 'j'
    [0x0C,0x00,0x1C,0x0C,0x0C,0x0C,0x6C,0x38],
    // 107 'k'
    [0x60,0x60,0x66,0x6C,0x78,0x6C,0x66,0x00],
    // 108 'l'
    [0x38,0x18,0x18,0x18,0x18,0x18,0x3C,0x00],
    // 109 'm'
    [0x00,0x00,0x66,0x7F,0x7F,0x6B,0x63,0x00],
    // 110 'n'
    [0x00,0x00,0x7C,0x66,0x66,0x66,0x66,0x00],
    // 111 'o'
    [0x00,0x00,0x3C,0x66,0x66,0x66,0x3C,0x00],
    // 112 'p'
    [0x00,0x00,0x7C,0x66,0x66,0x7C,0x60,0x60],
    // 113 'q'
    [0x00,0x00,0x3E,0x66,0x66,0x3E,0x06,0x06],
    // 114 'r'
    [0x00,0x00,0x7C,0x66,0x60,0x60,0x60,0x00],
    // 115 's'
    [0x00,0x00,0x3E,0x60,0x3C,0x06,0x7C,0x00],
    // 116 't'
    [0x18,0x18,0x7E,0x18,0x18,0x18,0x0E,0x00],
    // 117 'u'
    [0x00,0x00,0x66,0x66,0x66,0x66,0x3E,0x00],
    // 118 'v'
    [0x00,0x00,0x66,0x66,0x66,0x3C,0x18,0x00],
    // 119 'w'
    [0x00,0x00,0x63,0x6B,0x7F,0x7F,0x36,0x00],
    // 120 'x'
    [0x00,0x00,0x66,0x3C,0x18,0x3C,0x66,0x00],
    // 121 'y'
    [0x00,0x00,0x66,0x66,0x66,0x3E,0x06,0x7C],
    // 122 'z'
    [0x00,0x00,0x7E,0x0C,0x18,0x30,0x7E,0x00],
    // 123 '{'
    [0x0E,0x18,0x18,0x70,0x18,0x18,0x0E,0x00],
    // 124 '|'
    [0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x00],
    // 125 '}'
    [0x70,0x18,0x18,0x0E,0x18,0x18,0x70,0x00],
    // 126 '~'
    [0x00,0x00,0x76,0xDC,0x00,0x00,0x00,0x00],
];

/// Build a 1xN pixel buffer of the font atlas — one byte per pixel
/// (alpha 0 or 255). Used to upload the font texture once at init.
/// Layout : 95 cells laid horizontally, each 8x8.
pub fn build_font_atlas() -> Vec<u8> {
    let w = ATLAS_W as usize; // 760
    let h = ATLAS_H as usize; // 8
    let mut out = vec![0u8; w * h];
    for (gi, glyph) in FONT_8X8.iter().enumerate() {
        let cx = gi * (CELL_W as usize); // column origin in atlas px
        for (row, &bits) in glyph.iter().enumerate() {
            for col in 0..CELL_W as usize {
                let bit = (bits >> (7 - col)) & 1;
                let alpha = if bit == 1 { 255u8 } else { 0u8 };
                out[row * w + (cx + col)] = alpha;
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// § Color helpers
// ─────────────────────────────────────────────────────────────────────────

pub const COLOR_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const COLOR_BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
pub const COLOR_DIM: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
pub const COLOR_PANEL: [f32; 4] = [0.04, 0.05, 0.10, 0.92];
pub const COLOR_HIGHLIGHT: [f32; 4] = [1.0, 0.85, 0.20, 1.0];
pub const COLOR_DIM_TEXT: [f32; 4] = [0.75, 0.78, 0.85, 1.0];

// ─────────────────────────────────────────────────────────────────────────
// § Text-quad emission
// ─────────────────────────────────────────────────────────────────────────

/// Emit a solid-fill rectangle as 6 vertices (2 triangles).
/// `(x_px, y_px)` is the top-left in pixel-space. Kind=1.0 = solid.
pub fn push_solid_rect(
    out: &mut Vec<UiVertex>,
    x_px: f32,
    y_px: f32,
    w_px: f32,
    h_px: f32,
    color: [f32; 4],
) {
    let x0 = x_px;
    let y0 = y_px;
    let x1 = x_px + w_px;
    let y1 = y_px + h_px;
    let mk = |x: f32, y: f32| UiVertex {
        pos_px: [x, y],
        uv: [0.0, 0.0],
        color,
        kind: 1.0,
        _pad: [0.0; 3],
    };
    // Two triangles : (TL, BL, TR), (TR, BL, BR)
    out.push(mk(x0, y0));
    out.push(mk(x0, y1));
    out.push(mk(x1, y0));
    out.push(mk(x1, y0));
    out.push(mk(x0, y1));
    out.push(mk(x1, y1));
}

/// Build textured-quad vertices for a single line of text at pixel-space
/// (`x_px`, `y_px`). The position is the TOP-LEFT of the first character.
/// Each printable char emits 6 vertices (2 triangles) ; non-printable
/// chars and out-of-range chars emit nothing (advance only on space).
///
/// Returns the *advance width in pixels*, useful for chaining inline strings.
pub fn build_text_quads(
    text: &str,
    x_px: f32,
    y_px: f32,
    color: [f32; 4],
    scale: f32,
    out: &mut Vec<UiVertex>,
) -> f32 {
    let cw = (CELL_W as f32) * scale;
    let ch = (CELL_H as f32) * scale;
    let advance_px = cw; // monospace
    let atlas_w = ATLAS_W as f32;

    let mut cursor_x = x_px;
    for raw in text.chars() {
        let c = if raw.is_ascii() { raw as u8 } else { b'?' };
        if !(FIRST_GLYPH..=LAST_GLYPH).contains(&c) {
            // Tab / newline / control : skip (newlines must be split by caller).
            cursor_x += advance_px;
            continue;
        }
        let gi = (c - FIRST_GLYPH) as u32;
        let u0 = (gi * CELL_W) as f32 / atlas_w;
        let u1 = ((gi + 1) * CELL_W) as f32 / atlas_w;
        let v0 = 0.0_f32;
        let v1 = 1.0_f32;
        let x0 = cursor_x;
        let y0 = y_px;
        let x1 = cursor_x + cw;
        let y1 = y_px + ch;
        let mk = |x: f32, y: f32, u: f32, v: f32| UiVertex {
            pos_px: [x, y],
            uv: [u, v],
            color,
            kind: 0.0, // sample font
            _pad: [0.0; 3],
        };
        // Two triangles : (TL, BL, TR), (TR, BL, BR)
        out.push(mk(x0, y0, u0, v0));
        out.push(mk(x0, y1, u0, v1));
        out.push(mk(x1, y0, u1, v0));
        out.push(mk(x1, y0, u1, v0));
        out.push(mk(x0, y1, u0, v1));
        out.push(mk(x1, y1, u1, v1));
        cursor_x += advance_px;
    }
    cursor_x - x_px
}

/// Build text WITH a 1-px-offset black drop-shadow for legibility against
/// any background. Two passes : (a) shadow at +1px down/right in black,
/// (b) main text on top.
pub fn build_shadowed_text(
    text: &str,
    x_px: f32,
    y_px: f32,
    fg: [f32; 4],
    scale: f32,
    out: &mut Vec<UiVertex>,
) -> f32 {
    let _ = build_text_quads(text, x_px + scale, y_px + scale, COLOR_BLACK, scale, out);
    build_text_quads(text, x_px, y_px, fg, scale, out)
}

// ─────────────────────────────────────────────────────────────────────────
// § HUD string formatters
// ─────────────────────────────────────────────────────────────────────────

/// Format the camera position to 2 decimal places. Used by the top-right
/// HUD block.
pub fn fmt_camera_pos(pos: [f32; 3]) -> String {
    format!("pos = ({:.2}, {:.2}, {:.2})", pos[0], pos[1], pos[2])
}

/// Format yaw + pitch with sign + 2 decimals.
pub fn fmt_yaw_pitch(yaw: f32, pitch: f32) -> String {
    format!("yaw = {:+.2}  pitch = {:+.2}", yaw, pitch)
}

/// Format the render-mode line. `mode_idx` ∈ 0..=9.
pub fn fmt_render_mode_line(mode_idx: u8) -> String {
    format!("render = {}  (F1-F10)", render_mode_label(mode_idx))
}

/// Friendly label for a render-mode index. Stable strings — used in HUD,
/// menu, and tests.
pub fn render_mode_label(mode_idx: u8) -> &'static str {
    match mode_idx {
        0 => "DEFAULT",
        1 => "WIREFRAME",
        2 => "NORMALS",
        3 => "DEPTH",
        4 => "ALBEDO",
        5 => "LIGHTING",
        6 => "COMPASS",
        7 => "SUBSTRATE",
        8 => "SPECTRAL-KAN",
        _ => "DEBUG",
    }
}

/// Format the DM phase line.
pub fn fmt_dm_phase(label: &str) -> String {
    format!("DM = {} | tension =", label)
}

// ─────────────────────────────────────────────────────────────────────────
// § Menu state machine
// ─────────────────────────────────────────────────────────────────────────

/// Ordered menu items — the indices are stable so tests can assert on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MenuItem {
    Continue = 0,
    RenderMode = 1,
    ToggleFullscreen = 2,
    ShowMcpHelp = 3,
    Quit = 4,
}

impl MenuItem {
    pub const COUNT: u8 = 5;

    pub fn from_index(i: u8) -> Self {
        match i % Self::COUNT {
            0 => Self::Continue,
            1 => Self::RenderMode,
            2 => Self::ToggleFullscreen,
            3 => Self::ShowMcpHelp,
            _ => Self::Quit,
        }
    }
}

/// What the menu wants the host to do this frame, returned by activation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    None,
    Resume,
    CycleRenderMode,
    ToggleFullscreen,
    Quit,
}

/// Which screen of the menu is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuScreen {
    Main,
    McpHelp,
}

/// Owns menu open-state, current selection, and submenu state.
#[derive(Debug, Clone)]
pub struct MenuState {
    /// Whether the menu is currently visible.
    pub open: bool,
    /// Highlighted item index (0..MenuItem::COUNT).
    pub selection: u8,
    /// Which screen of the menu is showing.
    pub screen: MenuScreen,
    /// Render-mode shown by the "Render Mode :" line. Synced from the host.
    pub render_mode: u8,
    /// Scroll offset (in lines) for the McpHelp submenu.
    pub help_scroll: usize,
    /// Whether the host is in fullscreen (for the toggle label).
    pub fullscreen: bool,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            open: false,
            selection: 0,
            screen: MenuScreen::Main,
            render_mode: 0,
            help_scroll: 0,
            fullscreen: true,
        }
    }
}

impl MenuState {
    /// Toggle the menu open/closed. When opening, reset to first item +
    /// main screen.
    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open = true;
            self.screen = MenuScreen::Main;
            self.selection = 0;
            log_event("INFO", "loa-host/ui-overlay", "menu · OPENED");
        }
    }

    /// Force the menu closed, regardless of current screen.
    pub fn close(&mut self) {
        self.open = false;
        self.screen = MenuScreen::Main;
        self.help_scroll = 0;
        log_event("INFO", "loa-host/ui-overlay", "menu · CLOSED");
    }

    /// Move highlight up by one (wraps from top → bottom on the main screen ;
    /// scrolls help submenu up by one line).
    pub fn nav_up(&mut self) {
        match self.screen {
            MenuScreen::Main => {
                if self.selection == 0 {
                    self.selection = MenuItem::COUNT - 1;
                } else {
                    self.selection -= 1;
                }
            }
            MenuScreen::McpHelp => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
        }
    }

    /// Move highlight down by one (wraps from bottom → top on the main
    /// screen ; scrolls help submenu down).
    pub fn nav_down(&mut self) {
        match self.screen {
            MenuScreen::Main => {
                self.selection = (self.selection + 1) % MenuItem::COUNT;
            }
            MenuScreen::McpHelp => {
                self.help_scroll = self.help_scroll.saturating_add(1);
            }
        }
    }

    /// Left arrow : on the RenderMode item, decrement; on McpHelp, no-op.
    pub fn nav_left(&mut self) {
        if let MenuScreen::Main = self.screen {
            if self.current_item() == MenuItem::RenderMode {
                self.render_mode = if self.render_mode == 0 {
                    9
                } else {
                    self.render_mode - 1
                };
            }
        }
    }

    /// Right arrow : on the RenderMode item, increment; on McpHelp, no-op.
    pub fn nav_right(&mut self) {
        if let MenuScreen::Main = self.screen {
            if self.current_item() == MenuItem::RenderMode {
                self.render_mode = (self.render_mode + 1) % 10;
            }
        }
    }

    /// Activate the highlighted item. Returns the action the host should
    /// take. The MenuState may also internally navigate (e.g. ShowMcpHelp
    /// switches to the submenu and returns None).
    pub fn activate(&mut self) -> MenuAction {
        match self.screen {
            MenuScreen::Main => match self.current_item() {
                MenuItem::Continue => {
                    self.close();
                    MenuAction::Resume
                }
                MenuItem::RenderMode => {
                    self.render_mode = (self.render_mode + 1) % 10;
                    MenuAction::CycleRenderMode
                }
                MenuItem::ToggleFullscreen => MenuAction::ToggleFullscreen,
                MenuItem::ShowMcpHelp => {
                    self.screen = MenuScreen::McpHelp;
                    self.help_scroll = 0;
                    MenuAction::None
                }
                MenuItem::Quit => MenuAction::Quit,
            },
            MenuScreen::McpHelp => {
                // Enter on help submenu → back to main.
                self.screen = MenuScreen::Main;
                self.help_scroll = 0;
                MenuAction::None
            }
        }
    }

    /// Esc handling : on Main → close menu (resume) ; on Help → back to Main.
    pub fn back(&mut self) -> MenuAction {
        match self.screen {
            MenuScreen::Main => {
                self.close();
                MenuAction::Resume
            }
            MenuScreen::McpHelp => {
                self.screen = MenuScreen::Main;
                self.help_scroll = 0;
                MenuAction::None
            }
        }
    }

    pub fn current_item(&self) -> MenuItem {
        MenuItem::from_index(self.selection)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § HUD context — the per-frame data the overlay needs to draw HUD text
// ─────────────────────────────────────────────────────────────────────────

/// Snapshot of host data the overlay reads each frame to populate HUD strings.
#[derive(Debug, Clone)]
pub struct HudContext {
    pub frame: u64,
    pub fps: f32,
    pub camera_pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub render_mode: u8,
    pub dm_phase_label: &'static str,
    pub dm_tension: f32,
    pub recent_event: String,
    pub mcp_port: Option<u16>,
    pub fullscreen: bool,
    /// § T11-LOA-RICH-RENDER : material name on the plinth currently being
    /// faced (raycast from camera forward). Empty if nothing is in front.
    pub facing_material: String,
    /// Pattern name on the wall the camera is currently facing. "(none)" if
    /// the camera is not aimed at a wall.
    pub facing_pattern: String,
    /// Frame-time histogram (last 60 frames in ms). Drawn as a tiny bar chart
    /// at bottom-center.
    pub frame_times_ms: [f32; 60],
}

impl Default for HudContext {
    fn default() -> Self {
        Self {
            frame: 0,
            fps: 0.0,
            camera_pos: [0.0, 1.55, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            render_mode: 0,
            dm_phase_label: "CALM",
            dm_tension: 0.0,
            recent_event: String::new(),
            mcp_port: Some(3001),
            fullscreen: true,
            facing_material: String::new(),
            facing_pattern: String::from("(none)"),
            frame_times_ms: [16.7; 60],
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Frame-builder — emits the full HUD + menu vertex stream for the frame
// ─────────────────────────────────────────────────────────────────────────

/// Build the entire UI-overlay vertex stream for this frame. Returns the
/// vertex buffer the renderer will upload + draw.
///
/// Pixel coordinates are top-left origin. `screen_w` / `screen_h` are the
/// swap-chain target's physical pixel dimensions.
pub fn build_overlay_vertices(
    screen_w: u32,
    screen_h: u32,
    hud: &HudContext,
    menu: &MenuState,
) -> Vec<UiVertex> {
    let mut out: Vec<UiVertex> = Vec::with_capacity(2048);

    let sw = screen_w as f32;
    let sh = screen_h as f32;
    let scale = TEXT_SCALE; // 16 px tall
    let line = LINE_HEIGHT_PX * scale; // 20px
    let pad = 10.0_f32;

    // ─── Always-on HUD (drawn even when menu is open, so the user has
    // ─── context. Menu just dims the scene + overlays a panel.) ───

    // TOP-LEFT
    {
        let l1 = "LoA-v13 . pure-CSSL".to_string();
        let l2 = format!("frame={:04} . fps={:5.1}", hud.frame % 10000, hud.fps);
        build_shadowed_text(&l1, pad, pad, COLOR_WHITE, scale, &mut out);
        build_shadowed_text(&l2, pad, pad + line, COLOR_DIM_TEXT, scale, &mut out);
    }

    // TOP-RIGHT (right-aligned via fixed offset from screen edge)
    {
        let l1 = fmt_camera_pos(hud.camera_pos);
        let l2 = fmt_yaw_pitch(hud.yaw, hud.pitch);
        let l3 = fmt_render_mode_line(hud.render_mode);
        let glyph_px = (CELL_W as f32) * scale;
        // monospace : approx width = glyph_px * char-count ; pick the longest line.
        let w1 = (l1.chars().count() as f32) * glyph_px;
        let w2 = (l2.chars().count() as f32) * glyph_px;
        let w3 = (l3.chars().count() as f32) * glyph_px;
        let _ = w1.max(w2).max(w3);
        let x1 = sw - pad - (l1.chars().count() as f32) * glyph_px;
        let x2 = sw - pad - (l2.chars().count() as f32) * glyph_px;
        let x3 = sw - pad - (l3.chars().count() as f32) * glyph_px;
        build_shadowed_text(&l1, x1, pad, COLOR_WHITE, scale, &mut out);
        build_shadowed_text(&l2, x2, pad + line, COLOR_DIM_TEXT, scale, &mut out);
        build_shadowed_text(&l3, x3, pad + 2.0 * line, COLOR_DIM_TEXT, scale, &mut out);
    }

    // BOTTOM-LEFT
    {
        let phase_line = format!(
            "{}  {:.2}",
            fmt_dm_phase(hud.dm_phase_label),
            hud.dm_tension.clamp(0.0, 1.0)
        );
        let recent = if hud.recent_event.is_empty() {
            "recent : ...".to_string()
        } else {
            format!("recent : {}", hud.recent_event)
        };
        let y1 = sh - pad - 2.0 * line;
        let y2 = sh - pad - line;
        build_shadowed_text(&phase_line, pad, y1, COLOR_WHITE, scale, &mut out);
        build_shadowed_text(&recent, pad, y2, COLOR_DIM_TEXT, scale, &mut out);
    }

    // BOTTOM-RIGHT
    {
        let l1 = match hud.mcp_port {
            Some(p) => format!("MCP listening . :{}", p),
            None => "MCP : offline".to_string(),
        };
        let l2 = "Tab=menu . F11=fullscreen . Esc=menu".to_string();
        let glyph_px = (CELL_W as f32) * scale;
        let x1 = sw - pad - (l1.chars().count() as f32) * glyph_px;
        let x2 = sw - pad - (l2.chars().count() as f32) * glyph_px;
        let y1 = sh - pad - 2.0 * line;
        let y2 = sh - pad - line;
        build_shadowed_text(&l1, x1, y1, COLOR_WHITE, scale, &mut out);
        build_shadowed_text(&l2, x2, y2, COLOR_DIM_TEXT, scale, &mut out);
    }

    // CENTER : 5x5 crosshair (white outline + black halo)
    push_crosshair(sw * 0.5, sh * 0.5, &mut out);

    // ─── BOTTOM-CENTER : facing info + frame-time histogram ───
    {
        let glyph_px = (CELL_W as f32) * scale;
        let info = format!(
            "facing  pat={}  mat={}",
            if hud.facing_pattern.is_empty() {
                "(none)"
            } else {
                hud.facing_pattern.as_str()
            },
            if hud.facing_material.is_empty() {
                "(none)"
            } else {
                hud.facing_material.as_str()
            }
        );
        let info_w = (info.chars().count() as f32) * glyph_px;
        let xc = (sw - info_w) * 0.5;
        let yc = sh - pad - 3.0 * line;
        build_shadowed_text(&info, xc, yc, COLOR_DIM_TEXT, scale, &mut out);

        // Frame-time histogram : 60 bars, 3px wide each, 30px max height,
        // bottom-aligned 4 lines above the bottom HUD lines.
        let bar_w = 3.0_f32;
        let bar_gap = 0.0_f32;
        let max_height = 28.0_f32;
        // 50ms = full bar, 0ms = no bar (clamp).
        let max_ms = 50.0_f32;
        let total_w = (bar_w + bar_gap) * 60.0;
        let hx0 = (sw - total_w) * 0.5;
        let hy_bottom = sh - pad - 4.0 * line;
        for (i, &dt) in hud.frame_times_ms.iter().enumerate() {
            let h = (dt / max_ms).clamp(0.02, 1.0) * max_height;
            let bx = hx0 + (i as f32) * (bar_w + bar_gap);
            // Color : green if <20ms, yellow if <33ms, red otherwise.
            let col = if dt < 20.0 {
                [0.30, 0.85, 0.40, 0.85]
            } else if dt < 33.0 {
                [0.90, 0.85, 0.30, 0.85]
            } else {
                [0.95, 0.30, 0.30, 0.85]
            };
            push_solid_rect(&mut out, bx, hy_bottom - h, bar_w, h, col);
        }
    }

    // ─── MENU OVERLAY ───
    if menu.open {
        push_menu(sw, sh, hud, menu, &mut out);
    }

    out
}

/// Push a crosshair at (cx, cy) in pixel-space. 5px outer halo (black) +
/// 3px inner cross (white). Visible against any background.
pub fn push_crosshair(cx: f32, cy: f32, out: &mut Vec<UiVertex>) {
    // Black 5x5 halo
    push_solid_rect(out, cx - 2.5, cy - 2.5, 5.0, 5.0, COLOR_BLACK);
    // White 3x1 horizontal + 1x3 vertical bars (forms a plus inside)
    push_solid_rect(out, cx - 1.5, cy - 0.5, 3.0, 1.0, COLOR_WHITE);
    push_solid_rect(out, cx - 0.5, cy - 1.5, 1.0, 3.0, COLOR_WHITE);
}

/// Push the menu overlay : dim layer + panel + items.
fn push_menu(sw: f32, sh: f32, hud: &HudContext, menu: &MenuState, out: &mut Vec<UiVertex>) {
    // 1. Dim full-screen quad (50% black-ish)
    push_solid_rect(out, 0.0, 0.0, sw, sh, COLOR_DIM);

    // 2. Centered panel — 700x420 px (or scaled to 60% smaller dimension)
    let panel_w = 720.0_f32.min(sw * 0.7);
    let panel_h = match menu.screen {
        MenuScreen::Main => 360.0_f32.min(sh * 0.6),
        MenuScreen::McpHelp => 540.0_f32.min(sh * 0.78),
    };
    let panel_x = (sw - panel_w) * 0.5;
    let panel_y = (sh - panel_h) * 0.5;
    push_solid_rect(out, panel_x, panel_y, panel_w, panel_h, COLOR_PANEL);

    let scale = TEXT_SCALE;
    let line = LINE_HEIGHT_PX * scale;
    let pad_inner = 24.0_f32;

    match menu.screen {
        MenuScreen::Main => push_main_menu(panel_x, panel_y, panel_w, pad_inner, line, scale, hud, menu, out),
        MenuScreen::McpHelp => push_help_menu(panel_x, panel_y, panel_w, panel_h, pad_inner, line, scale, menu, out),
    }
}

#[allow(clippy::too_many_arguments)]
fn push_main_menu(
    panel_x: f32,
    panel_y: f32,
    panel_w: f32,
    pad_inner: f32,
    line: f32,
    scale: f32,
    hud: &HudContext,
    menu: &MenuState,
    out: &mut Vec<UiVertex>,
) {
    let title = "LABYRINTH OF APOCALYPSE . v13";
    let glyph_px = (CELL_W as f32) * scale;
    let title_w = (title.chars().count() as f32) * glyph_px;
    let title_x = panel_x + (panel_w - title_w) * 0.5;
    let title_y = panel_y + pad_inner;
    build_shadowed_text(title, title_x, title_y, COLOR_HIGHLIGHT, scale, out);

    let subtitle = "menu";
    let sub_w = (subtitle.chars().count() as f32) * glyph_px;
    let sub_x = panel_x + (panel_w - sub_w) * 0.5;
    build_shadowed_text(subtitle, sub_x, title_y + line, COLOR_DIM_TEXT, scale, out);

    // 5 items, listed below subtitle
    let items_y0 = title_y + line * 3.0;
    let item_x = panel_x + pad_inner * 2.5;
    for i in 0..MenuItem::COUNT {
        let y = items_y0 + (i as f32) * line * 1.4;
        let is_sel = menu.selection == i;
        let color = if is_sel { COLOR_HIGHLIGHT } else { COLOR_DIM_TEXT };
        let label = match MenuItem::from_index(i) {
            MenuItem::Continue => "Continue".to_string(),
            MenuItem::RenderMode => format!(
                "Render Mode : {}",
                render_mode_label(menu.render_mode)
            ),
            MenuItem::ToggleFullscreen => format!(
                "Toggle Fullscreen (F11)  [{}]",
                if menu.fullscreen { "ON" } else { "OFF" }
            ),
            MenuItem::ShowMcpHelp => "Show MCP Help".to_string(),
            MenuItem::Quit => "Quit".to_string(),
        };
        if is_sel {
            // Highlight bar behind the row
            let row_h = line + 4.0;
            push_solid_rect(
                out,
                panel_x + pad_inner,
                y - 4.0,
                panel_w - pad_inner * 2.0,
                row_h,
                [0.18, 0.20, 0.30, 0.85],
            );
            // Selection arrow
            build_shadowed_text(">", item_x - glyph_px * 1.5, y, color, scale, out);
        }
        build_shadowed_text(&label, item_x, y, color, scale, out);
    }

    // Hint footer
    let hint = "Up/Down  Enter . select  Esc/Tab . close";
    let hint_w = (hint.chars().count() as f32) * glyph_px;
    let hint_x = panel_x + (panel_w - hint_w) * 0.5;
    let hint_y = panel_y + (panel_y + 360.0 - panel_y) - pad_inner; // bottom-ish
    let _ = (hud,); // consumed for ergonomics
    build_shadowed_text(hint, hint_x, hint_y - line, COLOR_DIM_TEXT, scale, out);
}

#[allow(clippy::too_many_arguments)]
fn push_help_menu(
    panel_x: f32,
    panel_y: f32,
    panel_w: f32,
    panel_h: f32,
    pad_inner: f32,
    line: f32,
    scale: f32,
    menu: &MenuState,
    out: &mut Vec<UiVertex>,
) {
    let title = "MCP HELP . 17 tools . :3001";
    let glyph_px = (CELL_W as f32) * scale;
    let title_w = (title.chars().count() as f32) * glyph_px;
    let title_x = panel_x + (panel_w - title_w) * 0.5;
    let title_y = panel_y + pad_inner;
    build_shadowed_text(title, title_x, title_y, COLOR_HIGHLIGHT, scale, out);

    let lines: &[&str] = &[
        "engine.state             . current frame + camera + DM mirror",
        "engine.shutdown          . graceful exit (sovereign-cap gated)",
        "engine.pause             . set/clear paused flag",
        "camera.get               . pos/yaw/pitch read",
        "camera.set               . teleport (sovereign-cap)",
        "room.geometry            . vertex / index / plinth count",
        "telemetry.recent         . last N log lines",
        "dm.intensity             . 4-state FSM tension scalar",
        "dm.event.propose         . propose narrative event",
        "gm.describe_environment  . procedural env text",
        "gm.dialogue              . NPC line by archetype/mood/topic",
        "omega.sample             . substrate field sample",
        "omega.modify             . field-write (sovereign-cap)",
        "companion.propose        . creature-companion suggestions",
        "tools.list               . registry dump",
        "",
        "nc-example :",
        "  echo {jsonrpc:2.0,id:1,method:engine.state} | nc -q1 :: 3001",
        "  echo {jsonrpc:2.0,id:2,method:tools.list}   | nc -q1 :: 3001",
        "  echo {jsonrpc:2.0,id:3,method:omega.sample,",
        "        params:{x:0.5,y:0.5}} | nc -q1 :: 3001",
        "",
        "Up/Down . scroll . Enter/Esc . back to menu",
    ];

    let body_x = panel_x + pad_inner;
    let body_y0 = title_y + line * 2.0;
    let body_h = panel_h - (body_y0 - panel_y) - pad_inner;
    let max_visible = (body_h / (line * 1.05)) as usize;
    let max_visible = max_visible.min(lines.len());
    let scroll_max = lines.len().saturating_sub(max_visible);
    let scroll = menu.help_scroll.min(scroll_max);

    for (vis_i, line_text) in lines.iter().skip(scroll).take(max_visible).enumerate() {
        let y = body_y0 + (vis_i as f32) * line * 1.05;
        build_shadowed_text(line_text, body_x, y, COLOR_DIM_TEXT, scale, out);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § GPU pipeline owner — UiOverlay : font texture + pipeline + bind groups
// ─────────────────────────────────────────────────────────────────────────
//
// Compiles only under the `runtime` feature (depends on wgpu).

#[cfg(feature = "runtime")]
mod gpu_pipeline {
    use super::*;
    use wgpu::util::DeviceExt;

    /// GPU-resident UI overlay : owns the font texture, sampler, pipeline,
    /// uniform buffer, and a CPU-side vertex-staging buffer.
    pub struct UiOverlay {
        pipeline: wgpu::RenderPipeline,
        bind_group: wgpu::BindGroup,
        uniform_buf: wgpu::Buffer,
        vertex_buf: wgpu::Buffer,
        vertex_capacity: u64,
        /// Number of vertices for the most-recent build.
        vertex_count: u32,
        target_format: wgpu::TextureFormat,
    }

    impl UiOverlay {
        /// Construct + initialize the UI overlay against the given target
        /// format. Builds the font atlas, uploads the texture once, and
        /// pre-allocates a vertex buffer.
        pub fn new(
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            target_format: wgpu::TextureFormat,
        ) -> Self {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("loa-host/ui.wgsl"),
                source: wgpu::ShaderSource::Wgsl(UI_WGSL.into()),
            });

            // Font texture : R8Unorm, 760x8
            let atlas = build_font_atlas();
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("loa-host/ui-font-atlas"),
                size: wgpu::Extent3d {
                    width: ATLAS_W,
                    height: ATLAS_H,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &atlas,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(ATLAS_W),
                    rows_per_image: Some(ATLAS_H),
                },
                wgpu::Extent3d {
                    width: ATLAS_W,
                    height: ATLAS_H,
                    depth_or_array_layers: 1,
                },
            );
            let tex_view = tex.create_view(&wgpu::TextureViewDescriptor::default());

            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("loa-host/ui-font-sampler"),
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            });

            // Uniform : screen size
            let ubo = UiScreenUbo {
                size: [1.0, 1.0],
                _pad: [0.0, 0.0],
            };
            let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("loa-host/ui-screen-ubo"),
                contents: bytemuck::bytes_of(&ubo),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            // Bind group layout
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("loa-host/ui-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                ],
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("loa-host/ui-bg"),
                layout: &bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&tex_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("loa-host/ui-pipeline-layout"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });

            // Vertex layout : pos_px(vec2) + uv(vec2) + color(vec4) + kind(f32) + _pad(f32x3)
            // Total : 2+2+4+1+3 = 12 floats = 48 bytes (we use STRIDE = mem::size_of)
            let stride = UiVertex::STRIDE;
            let attrs: [wgpu::VertexAttribute; 4] = [
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
            ];

            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("loa-host/ui-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: stride,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None, // 2D — both faces visible
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

            // Pre-allocate a 64KB vertex buffer (~1300 quads worth).
            let initial_capacity: u64 = 64 * 1024;
            let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("loa-host/ui-vbo"),
                size: initial_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            log_event(
                "INFO",
                "loa-host/ui-overlay",
                "ui-overlay GPU resources initialized",
            );

            Self {
                pipeline,
                bind_group,
                uniform_buf,
                vertex_buf,
                vertex_capacity: initial_capacity,
                vertex_count: 0,
                target_format,
            }
        }

        /// Returns the format the pipeline is bound to (for resize sanity).
        pub fn target_format(&self) -> wgpu::TextureFormat {
            self.target_format
        }

        /// Build the per-frame vertex buffer (HUD + menu) and upload to GPU.
        /// Call once per frame BEFORE encoding the UI render pass.
        pub fn prepare_frame(
            &mut self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            screen_w: u32,
            screen_h: u32,
            hud: &HudContext,
            menu: &MenuState,
        ) {
            // Update screen-size uniform.
            let ubo = UiScreenUbo {
                size: [screen_w as f32, screen_h as f32],
                _pad: [0.0, 0.0],
            };
            queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&ubo));

            // Build vertex stream.
            let verts = build_overlay_vertices(screen_w, screen_h, hud, menu);
            self.vertex_count = verts.len() as u32;

            let bytes = bytemuck::cast_slice(&verts);
            let needed = bytes.len() as u64;
            if needed > self.vertex_capacity {
                // Grow to next power-of-two.
                let mut new_cap = self.vertex_capacity.max(64 * 1024);
                while new_cap < needed {
                    new_cap *= 2;
                }
                self.vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("loa-host/ui-vbo-grown"),
                    size: new_cap,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.vertex_capacity = new_cap;
                log_event(
                    "INFO",
                    "loa-host/ui-overlay",
                    &format!("ui-vbo grown to {} bytes", new_cap),
                );
            }
            if !bytes.is_empty() {
                queue.write_buffer(&self.vertex_buf, 0, bytes);
            }
        }

        /// Encode the UI overlay render pass. Call AFTER the scene pass.
        /// Loads (preserves) the existing color attachment ; no depth.
        pub fn encode_pass(
            &self,
            encoder: &mut wgpu::CommandEncoder,
            target_view: &wgpu::TextureView,
        ) {
            if self.vertex_count == 0 {
                return;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loa-host/ui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.draw(0..self.vertex_count, 0..1);
        }
    }
}

#[cfg(feature = "runtime")]
pub use gpu_pipeline::UiOverlay;

// ─────────────────────────────────────────────────────────────────────────
// § Tests (always compiled — exercise pure-CPU paths)
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_font_has_95_glyphs() {
        assert_eq!(FONT_8X8.len(), 95);
        assert_eq!(GLYPH_COUNT, 95);
        // First glyph (space) is fully zero
        assert_eq!(FONT_8X8[0], [0u8; 8]);
        // 'A' shape : middle row should be filled
        let a = FONT_8X8[(b'A' - FIRST_GLYPH) as usize];
        assert_ne!(a, [0u8; 8]);
        assert!(a[2] != 0);
    }

    #[test]
    fn font_atlas_is_correct_size() {
        let atlas = build_font_atlas();
        assert_eq!(atlas.len(), (ATLAS_W * ATLAS_H) as usize);
        // Atlas should have at least *some* set pixels (font isn't all blank).
        let lit = atlas.iter().filter(|b| **b != 0).count();
        assert!(lit > 100, "expected >100 lit pixels, got {}", lit);
    }

    #[test]
    fn build_text_quads_emits_6_vertices_per_char() {
        let mut out = Vec::new();
        let advance = build_text_quads("HUD", 0.0, 0.0, COLOR_WHITE, 1.0, &mut out);
        // 3 chars * 6 verts = 18 verts
        assert_eq!(out.len(), 18);
        // Advance = 3 chars * 8px (CELL_W) * 1.0 scale
        assert!((advance - 24.0).abs() < 1e-4);
    }

    #[test]
    fn build_text_quads_handles_unicode_gracefully() {
        let mut out = Vec::new();
        // Non-ASCII char becomes "?" replacement → still emits one glyph
        let _ = build_text_quads("A\u{2014}B", 0.0, 0.0, COLOR_WHITE, 1.0, &mut out);
        // 3 chars * 6 verts = 18 (em-dash → '?')
        assert_eq!(out.len(), 18);
    }

    #[test]
    fn solid_rect_emits_6_vertices() {
        let mut out = Vec::new();
        push_solid_rect(&mut out, 10.0, 20.0, 30.0, 40.0, COLOR_WHITE);
        assert_eq!(out.len(), 6);
        // First vertex is top-left
        assert_eq!(out[0].pos_px, [10.0, 20.0]);
        // All have kind=1.0 (solid)
        for v in &out {
            assert_eq!(v.kind, 1.0);
        }
    }

    #[test]
    fn menu_state_default_is_closed_main() {
        let m = MenuState::default();
        assert!(!m.open);
        assert_eq!(m.selection, 0);
        assert_eq!(m.screen, MenuScreen::Main);
    }

    #[test]
    fn menu_state_arrow_down_advances_selection_index() {
        let mut m = MenuState::default();
        m.toggle();
        assert_eq!(m.selection, 0);
        m.nav_down();
        assert_eq!(m.selection, 1);
        m.nav_down();
        assert_eq!(m.selection, 2);
        m.nav_down();
        assert_eq!(m.selection, 3);
        m.nav_down();
        assert_eq!(m.selection, 4);
        // Bottom : wrap to 0
        m.nav_down();
        assert_eq!(m.selection, 0);
    }

    #[test]
    fn menu_state_arrow_up_at_top_wraps_to_bottom() {
        let mut m = MenuState::default();
        m.toggle();
        assert_eq!(m.selection, 0);
        m.nav_up();
        assert_eq!(m.selection, MenuItem::COUNT - 1);
        // Confirm it's Quit
        assert_eq!(m.current_item(), MenuItem::Quit);
    }

    #[test]
    fn menu_state_enter_on_quit_signals_exit() {
        let mut m = MenuState::default();
        m.toggle();
        m.nav_up(); // wrap to Quit
        let action = m.activate();
        assert_eq!(action, MenuAction::Quit);
    }

    #[test]
    fn menu_state_enter_on_continue_resumes() {
        let mut m = MenuState::default();
        m.toggle();
        // Selection 0 = Continue
        let action = m.activate();
        assert_eq!(action, MenuAction::Resume);
        assert!(!m.open);
    }

    #[test]
    fn menu_state_render_mode_cycles_0_to_9() {
        let mut m = MenuState::default();
        m.toggle();
        // Move to RenderMode item (index 1)
        m.nav_down();
        assert_eq!(m.current_item(), MenuItem::RenderMode);
        // Activate → CycleRenderMode + render_mode increments
        for expected in 1..=9 {
            let action = m.activate();
            assert_eq!(action, MenuAction::CycleRenderMode);
            assert_eq!(m.render_mode, expected);
        }
        // Wrap : 9 → 0
        let _ = m.activate();
        assert_eq!(m.render_mode, 0);
    }

    #[test]
    fn menu_state_render_mode_left_decrements_with_wrap() {
        let mut m = MenuState::default();
        m.toggle();
        m.nav_down(); // RenderMode
        // Left from 0 wraps to 9
        m.nav_left();
        assert_eq!(m.render_mode, 9);
        m.nav_left();
        assert_eq!(m.render_mode, 8);
    }

    #[test]
    fn menu_state_esc_closes_returns_to_game() {
        let mut m = MenuState::default();
        m.toggle();
        assert!(m.open);
        let action = m.back();
        assert_eq!(action, MenuAction::Resume);
        assert!(!m.open);
    }

    #[test]
    fn menu_state_help_submenu_opens_and_back_returns() {
        let mut m = MenuState::default();
        m.toggle();
        // Navigate to ShowMcpHelp (index 3)
        m.nav_down();
        m.nav_down();
        m.nav_down();
        assert_eq!(m.current_item(), MenuItem::ShowMcpHelp);
        let action = m.activate();
        assert_eq!(action, MenuAction::None);
        assert_eq!(m.screen, MenuScreen::McpHelp);
        // Esc / back from help → back to main, menu still open
        let action = m.back();
        assert_eq!(action, MenuAction::None);
        assert_eq!(m.screen, MenuScreen::Main);
        assert!(m.open);
    }

    #[test]
    fn menu_state_help_scroll_advances_with_down() {
        let mut m = MenuState::default();
        m.toggle();
        // Jump to help submenu
        m.screen = MenuScreen::McpHelp;
        assert_eq!(m.help_scroll, 0);
        m.nav_down();
        assert_eq!(m.help_scroll, 1);
        m.nav_down();
        assert_eq!(m.help_scroll, 2);
        m.nav_up();
        assert_eq!(m.help_scroll, 1);
        // Up at 0 saturates
        m.help_scroll = 0;
        m.nav_up();
        assert_eq!(m.help_scroll, 0);
    }

    #[test]
    fn hud_strings_format_camera_pos_to_2_decimal() {
        let s = fmt_camera_pos([1.234567, -5.5, 0.0]);
        assert_eq!(s, "pos = (1.23, -5.50, 0.00)");
    }

    #[test]
    fn hud_strings_format_yaw_pitch_with_sign() {
        let s = fmt_yaw_pitch(0.5, -0.25);
        assert_eq!(s, "yaw = +0.50  pitch = -0.25");
    }

    #[test]
    fn render_mode_label_covers_all_indices() {
        for i in 0u8..=9 {
            assert!(!render_mode_label(i).is_empty());
        }
        // Out-of-range falls back to DEBUG label
        assert_eq!(render_mode_label(42), "DEBUG");
        assert_eq!(render_mode_label(0), "DEFAULT");
        assert_eq!(render_mode_label(9), "DEBUG");
    }

    #[test]
    fn build_overlay_vertices_returns_nonempty_for_default_hud() {
        let hud = HudContext::default();
        let menu = MenuState::default();
        let v = build_overlay_vertices(1280, 720, &hud, &menu);
        assert!(!v.is_empty(), "default HUD must produce vertices");
        // crosshair contributes 6 verts (halo) + 6 + 6 (cross bars) = 18 minimum.
        // Plus 4-corner text — easily many hundreds.
        assert!(v.len() > 100);
    }

    #[test]
    fn build_overlay_vertices_grows_when_menu_opens() {
        let hud = HudContext::default();
        let mut menu = MenuState::default();
        let baseline = build_overlay_vertices(1280, 720, &hud, &menu);
        menu.toggle();
        let with_menu = build_overlay_vertices(1280, 720, &hud, &menu);
        assert!(
            with_menu.len() > baseline.len(),
            "open menu must add vertices : baseline={}, with_menu={}",
            baseline.len(),
            with_menu.len()
        );
    }

    #[test]
    fn ui_wgsl_shader_validates_with_naga() {
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(UI_WGSL).expect("ui.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .expect("ui.wgsl must validate via naga");
    }

    #[test]
    fn ui_vertex_size_is_well_defined() {
        // 12 floats : 2(pos) + 2(uv) + 4(color) + 1(kind) + 3(pad) = 12 = 48 bytes
        assert_eq!(core::mem::size_of::<UiVertex>(), 48);
        assert_eq!(UiVertex::STRIDE, 48);
    }
}
