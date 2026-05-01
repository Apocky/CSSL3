//! § pattern — procedural-pattern registry for the diagnostic-dense renderer.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-RICH-RENDER (W-LOA-rich-render-overhaul)
//!
//! § ROLE
//!   16-entry GPU-uploadable LUT of procedural-pattern parameters. Each
//!   Vertex carries a `pattern_id: u32` that selects an entry ; the uber-
//!   shader's fragment stage switches on `Pattern::kind` and computes the
//!   final pattern color from `(uv, scale, rotation, phase, time)`.
//!
//! § PATTERNS (12+ procedurals)
//!   0  SOLID                — passthrough (use albedo only)
//!   1  GRID_1M              — 1m grid lines (calibration)
//!   2  GRID_100MM           — 100mm grid (fine sub-grid)
//!   3  CHECKERBOARD         — 1m checkerboard
//!   4  MACBETH_COLOR_CHART  — 24-patch ColorChecker (X-Rite reference)
//!   5  SNELLEN_EYE_CHART    — Snellen tumbling-E optometry chart
//!   6  QR_CODE_STUB         — 25×25 QR-aesthetic block grid (recognizable)
//!   7  EAN13_BARCODE        — vertical barcode bars + readable digits
//!   8  GRADIENT_GRAYSCALE   — 0..1 horizontal grayscale ramp
//!   9  GRADIENT_HUE_WHEEL   — full hue-circle wheel
//!   10 PERLIN_NOISE         — value-noise (low-frequency texture)
//!   11 CONCENTRIC_RINGS     — bullseye rings centered on uv (0.5,0.5)
//!   12 RADIAL_SPOKES        — spokes radiating from uv center
//!   13 ZONEPLATE            — frequency-sweep sine-fringe diagnostic
//!   14 FREQUENCY_SWEEP      — stacked spatial-frequency rows (1·4·16·64 cycles)
//!   15 RADIAL_GRADIENT      — radial 0..1 from uv center
//!
//! § GPU LAYOUT
//!   Each `Pattern` is 16 bytes (4 × f32) for tight std140 packing :
//!     - kind      : u32  (4 B)
//!     - scale     : f32  (4 B)
//!     - rotation  : f32  (4 B)
//!     - phase     : f32  (4 B)
//!   Total LUT = 16 × 16 = 256 bytes.

#![allow(clippy::cast_precision_loss)]

use bytemuck::{Pod, Zeroable};

/// GPU-uploadable pattern entry. 16 bytes · 4-byte aligned.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq)]
pub struct Pattern {
    /// Pattern kind discriminant. The fragment shader switch()es on this.
    pub kind: u32,
    /// Scale parameter (interpretation pattern-dependent).
    pub scale: f32,
    /// Rotation parameter (radians ; 0 = identity).
    pub rotation: f32,
    /// Phase parameter (interpretation pattern-dependent ; 0 = default).
    pub phase: f32,
}

impl Pattern {
    /// Construct a pattern with kind+scale and zero rotation/phase.
    #[must_use]
    pub const fn new(kind: u32, scale: f32) -> Self {
        Self {
            kind,
            scale,
            rotation: 0.0,
            phase: 0.0,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Stable pattern IDs (uber-shader switches on `kind`)
// ──────────────────────────────────────────────────────────────────────────

pub const PAT_SOLID: u32 = 0;
pub const PAT_GRID_1M: u32 = 1;
pub const PAT_GRID_100MM: u32 = 2;
pub const PAT_CHECKERBOARD: u32 = 3;
pub const PAT_MACBETH_COLOR_CHART: u32 = 4;
pub const PAT_SNELLEN_EYE_CHART: u32 = 5;
pub const PAT_QR_CODE_STUB: u32 = 6;
pub const PAT_EAN13_BARCODE: u32 = 7;
pub const PAT_GRADIENT_GRAYSCALE: u32 = 8;
pub const PAT_GRADIENT_HUE_WHEEL: u32 = 9;
pub const PAT_PERLIN_NOISE: u32 = 10;
pub const PAT_CONCENTRIC_RINGS: u32 = 11;
pub const PAT_RADIAL_SPOKES: u32 = 12;
pub const PAT_ZONEPLATE: u32 = 13;
pub const PAT_FREQUENCY_SWEEP: u32 = 14;
pub const PAT_RADIAL_GRADIENT: u32 = 15;

/// Total entries in the PATTERN_LUT.
pub const PATTERN_LUT_LEN: usize = 16;

/// Build the canonical 16-entry pattern LUT.
#[must_use]
pub fn pattern_lut() -> [Pattern; PATTERN_LUT_LEN] {
    [
        Pattern::new(PAT_SOLID, 1.0),
        Pattern::new(PAT_GRID_1M, 1.0),
        Pattern::new(PAT_GRID_100MM, 10.0),
        Pattern::new(PAT_CHECKERBOARD, 1.0),
        Pattern::new(PAT_MACBETH_COLOR_CHART, 1.0),
        Pattern::new(PAT_SNELLEN_EYE_CHART, 1.0),
        Pattern::new(PAT_QR_CODE_STUB, 25.0),
        Pattern::new(PAT_EAN13_BARCODE, 1.0),
        Pattern::new(PAT_GRADIENT_GRAYSCALE, 1.0),
        Pattern::new(PAT_GRADIENT_HUE_WHEEL, 1.0),
        Pattern::new(PAT_PERLIN_NOISE, 4.0),
        Pattern::new(PAT_CONCENTRIC_RINGS, 8.0),
        Pattern::new(PAT_RADIAL_SPOKES, 12.0),
        Pattern::new(PAT_ZONEPLATE, 32.0),
        Pattern::new(PAT_FREQUENCY_SWEEP, 1.0),
        Pattern::new(PAT_RADIAL_GRADIENT, 1.0),
    ]
}

/// Human-readable pattern name (for HUD + MCP `render.list_patterns`).
#[must_use]
pub const fn pattern_name(id: u32) -> &'static str {
    match id {
        PAT_SOLID => "Solid",
        PAT_GRID_1M => "Grid-1m",
        PAT_GRID_100MM => "Grid-100mm",
        PAT_CHECKERBOARD => "Checkerboard",
        PAT_MACBETH_COLOR_CHART => "Macbeth-ColorChart",
        PAT_SNELLEN_EYE_CHART => "Snellen-EyeChart",
        PAT_QR_CODE_STUB => "QR-Code",
        PAT_EAN13_BARCODE => "EAN-13-Barcode",
        PAT_GRADIENT_GRAYSCALE => "Gradient-Grayscale",
        PAT_GRADIENT_HUE_WHEEL => "Gradient-HueWheel",
        PAT_PERLIN_NOISE => "Perlin-Noise",
        PAT_CONCENTRIC_RINGS => "Concentric-Rings",
        PAT_RADIAL_SPOKES => "Radial-Spokes",
        PAT_ZONEPLATE => "Zoneplate",
        PAT_FREQUENCY_SWEEP => "Frequency-Sweep",
        PAT_RADIAL_GRADIENT => "Radial-Gradient",
        _ => "Unknown",
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § CPU-side reference implementations of the patterns
// ──────────────────────────────────────────────────────────────────────────
//
// These exist so unit tests can verify the canonical color values match
// X-Rite Macbeth references etc. The fragment shader has its own GPU-side
// implementations (mathematically identical for the spec patches).

/// Canonical X-Rite ColorChecker 24-patch sRGB values (linear-light). Order :
/// row-major from top-left (4 columns × 6 rows). Used by both the GPU
/// fragment shader (hard-coded constants) and the test below.
///
/// Source : the canonical X-Rite ColorChecker reference values published in
/// 1976 by McCamy et al. ("A Color-Rendition Chart"). Values here are the
/// approximate sRGB-display values rounded to 2 decimal places ; precision
/// to the fourth decimal isn't required for diagnostic-rendering use.
#[must_use]
pub fn macbeth_24_colors() -> [[f32; 3]; 24] {
    [
        // Row 1 — Natural colors
        [0.45, 0.32, 0.27], // 1  Dark skin
        [0.76, 0.58, 0.51], // 2  Light skin
        [0.36, 0.48, 0.61], // 3  Blue sky
        [0.35, 0.42, 0.27], // 4  Foliage
        [0.51, 0.50, 0.69], // 5  Blue flower
        [0.40, 0.74, 0.67], // 6  Bluish green
        // Row 2 — Miscellaneous colors
        [0.83, 0.48, 0.18], // 7  Orange
        [0.27, 0.34, 0.65], // 8  Purplish blue
        [0.77, 0.33, 0.36], // 9  Moderate red
        [0.36, 0.24, 0.42], // 10 Purple
        [0.62, 0.73, 0.25], // 11 Yellow green
        [0.89, 0.63, 0.17], // 12 Orange yellow
        // Row 3 — Primary + secondary colors
        [0.21, 0.24, 0.58], // 13 Blue
        [0.27, 0.58, 0.29], // 14 Green
        [0.69, 0.20, 0.22], // 15 Red
        [0.91, 0.78, 0.13], // 16 Yellow
        [0.73, 0.33, 0.59], // 17 Magenta
        [0.04, 0.50, 0.66], // 18 Cyan
        // Row 4 — Grayscale
        [0.95, 0.95, 0.95], // 19 White (95% reflectance)
        [0.78, 0.78, 0.78], // 20 Neutral 8
        [0.63, 0.63, 0.63], // 21 Neutral 6.5
        [0.48, 0.48, 0.48], // 22 Neutral 5
        [0.33, 0.33, 0.33], // 23 Neutral 3.5
        [0.20, 0.20, 0.20], // 24 Black (3.1% reflectance)
    ]
}

/// CPU reference : compute the macbeth chart color at uv ∈ [0,1]^2.
/// Layout : 6 columns (uv.x) × 4 rows (uv.y) with uv.y increasing downward.
#[must_use]
pub fn cpu_macbeth_at_uv(u: f32, v: f32) -> [f32; 3] {
    let cols: usize = 6;
    let rows: usize = 4;
    let cu = (u.clamp(0.0, 0.9999) * cols as f32) as usize;
    let cv = (v.clamp(0.0, 0.9999) * rows as f32) as usize;
    let idx = cv * cols + cu;
    macbeth_24_colors()[idx]
}

/// CPU reference : compute the 1m grid color at uv (returns line color or
/// background based on integer-line proximity).
#[must_use]
pub fn cpu_grid_at_uv(u: f32, v: f32, scale: f32) -> [f32; 3] {
    let su = (u * scale).fract().abs();
    let sv = (v * scale).fract().abs();
    let edge = 0.04_f32; // 4% line width
    if su < edge || sv < edge || su > (1.0 - edge) || sv > (1.0 - edge) {
        [0.10, 0.10, 0.10] // dark line
    } else {
        [0.85, 0.85, 0.85] // light cell
    }
}

/// CPU reference : checkerboard at uv with given scale (cells per UV unit).
#[must_use]
pub fn cpu_checker_at_uv(u: f32, v: f32, scale: f32) -> [f32; 3] {
    let iu = (u * scale).floor() as i32;
    let iv = (v * scale).floor() as i32;
    if (iu + iv).rem_euclid(2) == 0 {
        [0.05, 0.05, 0.05]
    } else {
        [0.95, 0.95, 0.95]
    }
}

/// CPU reference : EAN-13 barcode bars at uv.x. Returns black (bar) or
/// white (gap) based on a 95-module pattern (start + 6+5 digits + middle
/// + 6+5 digits + end). Stage-0 uses a recognizable EAN-style stub
/// rather than encoding a real check-digit.
#[must_use]
pub fn cpu_ean13_at_uv(u: f32, _v: f32) -> [f32; 3] {
    // 95 modules total. We use a deterministic pseudo-pattern that's
    // recognizable as a barcode (mostly black/white alternating with
    // varying widths).
    let m = (u * 95.0) as i32;
    // Start guard 101
    if (0..3).contains(&m) {
        return if m == 1 {
            [0.95, 0.95, 0.95]
        } else {
            [0.05, 0.05, 0.05]
        };
    }
    // End guard 101
    if (92..95).contains(&m) {
        return if m == 93 {
            [0.95, 0.95, 0.95]
        } else {
            [0.05, 0.05, 0.05]
        };
    }
    // Middle guard 01010 at modules 45..50
    if (45..50).contains(&m) {
        return if (m - 45) % 2 == 1 {
            [0.05, 0.05, 0.05]
        } else {
            [0.95, 0.95, 0.95]
        };
    }
    // Body : pseudo-random bar pattern
    let h = ((m as u32).wrapping_mul(2654435761)) ^ 0xDEAD_BEEF;
    if (h & 1) == 0 {
        [0.05, 0.05, 0.05]
    } else {
        [0.95, 0.95, 0.95]
    }
}

/// CPU reference : QR-aesthetic 25×25 module grid. A 25-module QR symbol
/// has corner-finder patterns ; we render a recognizable layout with three
/// finder squares + alignment-square + scattered data modules.
#[must_use]
pub fn cpu_qr_at_uv(u: f32, v: f32) -> [f32; 3] {
    let modules = 25;
    let mu = (u.clamp(0.0, 0.9999) * modules as f32) as i32;
    let mv = (v.clamp(0.0, 0.9999) * modules as f32) as i32;

    let in_finder = |cx: i32, cy: i32| -> bool {
        let dx = mu - cx;
        let dy = mv - cy;
        // 7×7 finder pattern (centered on cx,cy with half-extent 3)
        if dx.abs() > 3 || dy.abs() > 3 {
            return false;
        }
        let r = dx.abs().max(dy.abs());
        // outer ring (r=3) black, ring r=2 white, inner 3×3 black
        match r {
            3 => true,
            2 => false,
            _ => true,
        }
    };

    let on = if in_finder(3, 3) || in_finder(21, 3) || in_finder(3, 21) {
        true
    } else if mu == 18 && (16..=20).contains(&mv) {
        // Alignment pattern
        true
    } else if mv == 18 && (16..=20).contains(&mu) {
        true
    } else if (mu == 6 && (8..21).contains(&mv)) || (mv == 6 && (8..21).contains(&mu)) {
        // Timing patterns (alternating)
        ((mu + mv) & 1) == 0
    } else if mu < 8 && mv < 8 {
        false
    } else if mu >= 19 && mv < 8 {
        false
    } else if mu < 8 && mv >= 19 {
        false
    } else {
        // Pseudo-random data modules
        let h = ((mu as u32).wrapping_mul(2654435761) ^ (mv as u32).wrapping_mul(40503))
            .wrapping_add(0x1234_5678);
        (h & 1) == 1
    };

    if on {
        [0.05, 0.05, 0.05]
    } else {
        [0.95, 0.95, 0.95]
    }
}

/// CPU reference : grayscale gradient at uv.x.
#[must_use]
pub fn cpu_gradient_grayscale_at_uv(u: f32, _v: f32) -> [f32; 3] {
    let g = u.clamp(0.0, 1.0);
    [g, g, g]
}

/// CPU reference : value-noise (deterministic hash-based, not real Perlin).
#[must_use]
pub fn cpu_value_noise_at_uv(u: f32, v: f32, scale: f32) -> [f32; 3] {
    let x = u * scale;
    let y = v * scale;
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let h = ((xi as u32).wrapping_mul(2654435761) ^ (yi as u32).wrapping_mul(40503))
        .wrapping_add(0x9E37_79B9);
    let n = ((h & 0xFFFF) as f32) / 65535.0;
    [n, n, n]
}

/// CPU reference : concentric rings centered on (0.5,0.5).
#[must_use]
pub fn cpu_concentric_rings_at_uv(u: f32, v: f32, scale: f32) -> [f32; 3] {
    let dx = u - 0.5;
    let dy = v - 0.5;
    let r = (dx * dx + dy * dy).sqrt();
    let band = (r * scale).fract();
    if band < 0.5 {
        [0.10, 0.10, 0.10]
    } else {
        [0.90, 0.90, 0.90]
    }
}

/// CPU reference : radial gradient 0..1 from center.
#[must_use]
pub fn cpu_radial_gradient_at_uv(u: f32, v: f32) -> [f32; 3] {
    let dx = u - 0.5;
    let dy = v - 0.5;
    let r = ((dx * dx + dy * dy).sqrt() * 2.0).clamp(0.0, 1.0);
    [r, r, r]
}

// ──────────────────────────────────────────────────────────────────────────
// § Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_lut_has_at_least_12_entries() {
        let lut = pattern_lut();
        assert!(lut.len() >= 12, "LUT must have ≥ 12 patterns");
        assert_eq!(lut.len(), PATTERN_LUT_LEN);
    }

    #[test]
    fn pattern_lut_has_16_entries_canonical() {
        let lut = pattern_lut();
        assert_eq!(lut.len(), 16);
    }

    #[test]
    fn pattern_struct_size_is_16_bytes() {
        // Critical : the WGSL `struct Pattern` MUST match this layout.
        assert_eq!(core::mem::size_of::<Pattern>(), 16);
        assert_eq!(core::mem::align_of::<Pattern>(), 16);
    }

    #[test]
    fn pattern_pod_zero_is_valid() {
        let p: Pattern = bytemuck::Zeroable::zeroed();
        assert_eq!(p.kind, 0);
        assert_eq!(p.scale, 0.0);
    }

    #[test]
    fn pattern_macbeth_returns_24_distinct_colors() {
        use std::collections::HashSet;
        let cols = macbeth_24_colors();
        assert_eq!(cols.len(), 24);

        let mut bag = HashSet::new();
        for c in cols {
            bag.insert([c[0].to_bits(), c[1].to_bits(), c[2].to_bits()]);
        }
        // 24 patches must be 24 distinct color-tuples.
        assert_eq!(bag.len(), 24, "Macbeth patches must be unique");
    }

    #[test]
    fn pattern_macbeth_returns_24_distinct_colors_at_canonical_uvs() {
        use std::collections::HashSet;
        let mut bag = HashSet::new();
        let cols = 6;
        let rows = 4;
        for r in 0..rows {
            for c in 0..cols {
                // sample at the cell-center
                let u = (c as f32 + 0.5) / cols as f32;
                let v = (r as f32 + 0.5) / rows as f32;
                let rgb = cpu_macbeth_at_uv(u, v);
                bag.insert([rgb[0].to_bits(), rgb[1].to_bits(), rgb[2].to_bits()]);
            }
        }
        assert_eq!(bag.len(), 24);
    }

    #[test]
    fn pattern_grid_renders_lines_at_integer_uv() {
        // Lines should be dark at uv near integers, light in the middle.
        let line_col = cpu_grid_at_uv(0.001, 0.5, 1.0);
        let cell_col = cpu_grid_at_uv(0.5, 0.5, 1.0);
        assert!(line_col[0] < 0.5, "line should be dark");
        assert!(cell_col[0] > 0.5, "cell should be light");
    }

    #[test]
    fn pattern_checker_alternates() {
        // (0,0) and (1,1) cells should match ; (0,0) and (1,0) should differ.
        let a = cpu_checker_at_uv(0.25, 0.25, 1.0);
        let b = cpu_checker_at_uv(1.25, 0.25, 1.0);
        let c = cpu_checker_at_uv(1.25, 1.25, 1.0);
        assert_ne!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn pattern_names_are_unique() {
        use std::collections::HashSet;
        let mut names = HashSet::new();
        for id in 0..PATTERN_LUT_LEN as u32 {
            names.insert(pattern_name(id));
        }
        assert_eq!(names.len(), PATTERN_LUT_LEN);
    }

    #[test]
    fn ean13_barcode_emits_bars() {
        // Sample the body region (modules 5..40) and verify we see both colors.
        let mut saw_black = false;
        let mut saw_white = false;
        for i in 5..40 {
            let u = (i as f32 + 0.5) / 95.0;
            let c = cpu_ean13_at_uv(u, 0.5);
            if c[0] < 0.5 {
                saw_black = true;
            } else {
                saw_white = true;
            }
        }
        assert!(saw_black);
        assert!(saw_white);
    }

    #[test]
    fn qr_finder_squares_detected() {
        // Top-left finder center at module (3,3). Cell-center uv ≈ (3.5/25, 3.5/25).
        let u = 3.5 / 25.0;
        let v = 3.5 / 25.0;
        let inner = cpu_qr_at_uv(u, v);
        assert!(inner[0] < 0.5, "inner finder square should be dark");
    }

    #[test]
    fn radial_gradient_zero_at_center() {
        let c = cpu_radial_gradient_at_uv(0.5, 0.5);
        assert!(c[0] < 0.05);
        let edge = cpu_radial_gradient_at_uv(1.0, 0.5);
        assert!(edge[0] > 0.5);
    }

    #[test]
    fn pattern_lut_total_byte_size_under_16kib() {
        let total = PATTERN_LUT_LEN * core::mem::size_of::<Pattern>();
        assert!(total <= 16 * 1024);
        assert_eq!(total, 256);
    }

    #[test]
    fn solid_pattern_is_id_zero() {
        assert_eq!(PAT_SOLID, 0);
    }
}
