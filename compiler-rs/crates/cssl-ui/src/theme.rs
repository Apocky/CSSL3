//! § Theme — pluggable visual style for widgets.
//!
//! § ROLE
//!   Widgets read style values (colour, spacing, font-size) from the active
//!   `Theme` rather than hard-coding. Games can swap themes per-skin without
//!   recompiling the framework. The default theme is "dark-mode developer
//!   console" — neutral colours, readable contrast, no surprises.
//!
//! § DESIGN
//!   - `Color` is a 4-channel RGBA struct (sRGB premultiplied — the painter
//!     decides what to do at submit-time).
//!   - `Spacing` is a small bag of named constants (`tight` / `normal` /
//!     `loose`) the layout solver consults for default padding.
//!   - `Theme` is a value-type (no allocation, no Arc) so an immediate-mode
//!     `Ui` can stack theme overrides cheaply.
//!   - `ThemeSlot` is an enum that selects which colour applies — widgets
//!     ask the theme via `theme.color(slot)` rather than naming each field.
//!
//! § LANDMINE — pluggable theme
//!   The widget code MUST NOT know which `ThemeSlot` resolves to which
//!   field — else a custom theme that re-purposes slots breaks. Widgets
//!   call `Theme::color(slot)` ; the theme dispatches internally.
//!
//! § PRIME-DIRECTIVE attestation
//!   Theme is pure data. No phone-home, no telemetry, no covert state.

use crate::geometry::Insets;

/// 4-channel RGBA colour in linear-sRGB float space.
///
/// Each channel is `0.0..=1.0` ; alpha is straight (not premultiplied) at
/// the API boundary so themes can be authored intuitively. The `Painter`
/// implementation chooses whether to premultiply at submit-time.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Fully transparent colour.
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    /// Construct from RGBA floats (clamped to `0.0..=1.0`).
    #[must_use]
    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    /// Construct an opaque colour from RGB floats (alpha = 1.0).
    #[must_use]
    pub fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    /// Construct from 8-bit-per-channel RGBA bytes.
    #[must_use]
    pub fn rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
            a: f32::from(a) / 255.0,
        }
    }

    /// Linearly interpolate this colour toward `other` by `t` ∈ `0.0..=1.0`.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// Multiply alpha by `factor` ; useful for hover / disabled states.
    #[must_use]
    pub fn with_alpha(self, alpha: f32) -> Self {
        Self { a: alpha.clamp(0.0, 1.0), ..self }
    }
}

/// Named colour slots that widgets request from the active theme.
///
/// Adding a new variant is a non-breaking change for `Theme` impls because
/// the trait method has a default arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ThemeSlot {
    /// Window / panel background.
    Background,
    /// Default text foreground.
    Foreground,
    /// Faded / disabled text.
    ForegroundMuted,
    /// Default button face.
    ButtonFace,
    /// Button face when the cursor hovers over it.
    ButtonHover,
    /// Button face while a press is held.
    ButtonActive,
    /// Border drawn around frames + buttons + inputs.
    Border,
    /// Accent colour (slider track, focus ring, selection).
    Accent,
    /// Subtle accent, e.g. progress-bar trough.
    AccentMuted,
    /// Highlight colour for selected list / tree row.
    Selection,
    /// Disabled backdrop.
    Disabled,
    /// Error / danger.
    Error,
    /// Caret / text-cursor in a `TextInput`.
    Caret,
}

/// Spacing constants the layout solver consults for default padding +
/// inter-widget gaps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    /// Tight padding (~2px). Used inside `Label`-only rows.
    pub tight: f32,
    /// Normal padding (~6px). Default `Button` inset.
    pub normal: f32,
    /// Loose padding (~12px). Section gaps.
    pub loose: f32,
    /// Inter-widget gap inside a `Vbox` / `Hbox`.
    pub gap: f32,
}

impl Spacing {
    /// Default spacing — the values used by [`Theme::default`].
    pub const DEFAULT: Self = Self { tight: 2.0, normal: 6.0, loose: 12.0, gap: 4.0 };
}

impl Default for Spacing {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Font metrics as a flat record. The framework is platform-neutral so the
/// font stack is a logical name — the painter resolves it.
#[derive(Debug, Clone, PartialEq)]
pub struct FontStyle {
    /// Logical font family ("system", "monospace", "serif", or a custom
    /// name the painter knows how to map).
    pub family: String,
    /// Pixel size at 1x DPI.
    pub size_px: f32,
    /// Bold / regular / light flag.
    pub weight: FontWeight,
    /// Italic / regular flag.
    pub italic: bool,
}

impl Default for FontStyle {
    fn default() -> Self {
        Self {
            family: String::from("system"),
            size_px: 14.0,
            weight: FontWeight::Regular,
            italic: false,
        }
    }
}

/// Font weight — coarse-grained for now ; can grow to OpenType `wght` axis
/// later without API breakage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Light,
    Regular,
    Bold,
}

/// The full theme — the bag widgets read from.
///
/// `Theme` is a plain value type ; Application code can mutate one in place
/// or build alternate themes (`Theme::dark()`, `Theme::light()`) and switch
/// at runtime.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Lookup table keyed by [`ThemeSlot`]. Stored as a flat array so the
    /// dispatch is O(1) and the type stays `Sized` (no `HashMap` import).
    colors: [Color; 13],
    pub spacing: Spacing,
    pub font: FontStyle,
    /// Default insets applied inside a `Container`.
    pub container_padding: Insets,
    /// Default corner radius for rounded panels (0.0 = sharp corners).
    pub corner_radius: f32,
    /// Width of the focus ring drawn around focused widgets.
    pub focus_ring_width: f32,
}

impl Theme {
    /// Construct the dark theme — the framework default.
    #[must_use]
    pub fn dark() -> Self {
        let mut colors = [Color::TRANSPARENT; 13];
        colors[Self::idx(ThemeSlot::Background)] = Color::rgba_u8(20, 22, 28, 255);
        colors[Self::idx(ThemeSlot::Foreground)] = Color::rgba_u8(220, 222, 228, 255);
        colors[Self::idx(ThemeSlot::ForegroundMuted)] = Color::rgba_u8(140, 142, 148, 255);
        colors[Self::idx(ThemeSlot::ButtonFace)] = Color::rgba_u8(48, 52, 60, 255);
        colors[Self::idx(ThemeSlot::ButtonHover)] = Color::rgba_u8(64, 70, 80, 255);
        colors[Self::idx(ThemeSlot::ButtonActive)] = Color::rgba_u8(36, 40, 48, 255);
        colors[Self::idx(ThemeSlot::Border)] = Color::rgba_u8(70, 74, 84, 255);
        colors[Self::idx(ThemeSlot::Accent)] = Color::rgba_u8(82, 156, 220, 255);
        colors[Self::idx(ThemeSlot::AccentMuted)] = Color::rgba_u8(40, 80, 120, 255);
        colors[Self::idx(ThemeSlot::Selection)] = Color::rgba_u8(60, 100, 160, 200);
        colors[Self::idx(ThemeSlot::Disabled)] = Color::rgba_u8(40, 40, 44, 255);
        colors[Self::idx(ThemeSlot::Error)] = Color::rgba_u8(220, 80, 80, 255);
        colors[Self::idx(ThemeSlot::Caret)] = Color::rgba_u8(220, 222, 228, 255);
        Self {
            colors,
            spacing: Spacing::DEFAULT,
            font: FontStyle::default(),
            container_padding: Insets::uniform(6.0),
            corner_radius: 4.0,
            focus_ring_width: 2.0,
        }
    }

    /// Construct a light theme — for daytime UI / printed-document style.
    #[must_use]
    pub fn light() -> Self {
        let mut colors = [Color::TRANSPARENT; 13];
        colors[Self::idx(ThemeSlot::Background)] = Color::rgba_u8(248, 248, 250, 255);
        colors[Self::idx(ThemeSlot::Foreground)] = Color::rgba_u8(20, 24, 32, 255);
        colors[Self::idx(ThemeSlot::ForegroundMuted)] = Color::rgba_u8(110, 114, 124, 255);
        colors[Self::idx(ThemeSlot::ButtonFace)] = Color::rgba_u8(232, 234, 238, 255);
        colors[Self::idx(ThemeSlot::ButtonHover)] = Color::rgba_u8(216, 220, 228, 255);
        colors[Self::idx(ThemeSlot::ButtonActive)] = Color::rgba_u8(196, 202, 212, 255);
        colors[Self::idx(ThemeSlot::Border)] = Color::rgba_u8(180, 184, 194, 255);
        colors[Self::idx(ThemeSlot::Accent)] = Color::rgba_u8(48, 120, 200, 255);
        colors[Self::idx(ThemeSlot::AccentMuted)] = Color::rgba_u8(160, 196, 232, 255);
        colors[Self::idx(ThemeSlot::Selection)] = Color::rgba_u8(160, 200, 240, 200);
        colors[Self::idx(ThemeSlot::Disabled)] = Color::rgba_u8(220, 222, 228, 255);
        colors[Self::idx(ThemeSlot::Error)] = Color::rgba_u8(200, 60, 60, 255);
        colors[Self::idx(ThemeSlot::Caret)] = Color::rgba_u8(20, 24, 32, 255);
        Self {
            colors,
            spacing: Spacing::DEFAULT,
            font: FontStyle::default(),
            container_padding: Insets::uniform(6.0),
            corner_radius: 4.0,
            focus_ring_width: 2.0,
        }
    }

    /// Look up the colour for `slot`.
    #[must_use]
    pub fn color(&self, slot: ThemeSlot) -> Color {
        self.colors[Self::idx(slot)]
    }

    /// Override a single colour slot — fluent builder for skin construction.
    #[must_use]
    pub fn with_color(mut self, slot: ThemeSlot, color: Color) -> Self {
        self.colors[Self::idx(slot)] = color;
        self
    }

    /// Override the spacing.
    #[must_use]
    pub fn with_spacing(mut self, spacing: Spacing) -> Self {
        self.spacing = spacing;
        self
    }

    /// Override the font.
    #[must_use]
    pub fn with_font(mut self, font: FontStyle) -> Self {
        self.font = font;
        self
    }

    /// Index lookup helper for the flat colour array.
    const fn idx(slot: ThemeSlot) -> usize {
        match slot {
            ThemeSlot::Background => 0,
            ThemeSlot::Foreground => 1,
            ThemeSlot::ForegroundMuted => 2,
            ThemeSlot::ButtonFace => 3,
            ThemeSlot::ButtonHover => 4,
            ThemeSlot::ButtonActive => 5,
            ThemeSlot::Border => 6,
            ThemeSlot::Accent => 7,
            ThemeSlot::AccentMuted => 8,
            ThemeSlot::Selection => 9,
            ThemeSlot::Disabled => 10,
            ThemeSlot::Error => 11,
            ThemeSlot::Caret => 12,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_clamps_overflow() {
        let c = Color::rgba(2.0, -1.0, 0.5, 0.7);
        assert!((c.r - 1.0).abs() < f32::EPSILON);
        assert!(c.g.abs() < f32::EPSILON);
    }

    #[test]
    fn color_rgba_u8_round_trip() {
        let c = Color::rgba_u8(255, 0, 128, 255);
        assert!((c.r - 1.0).abs() < f32::EPSILON);
        assert!(c.g.abs() < f32::EPSILON);
        assert!((c.b - 128.0 / 255.0).abs() < f32::EPSILON);
    }

    #[test]
    fn color_lerp_endpoints() {
        let a = Color::rgb(0.0, 0.0, 0.0);
        let b = Color::rgb(1.0, 1.0, 1.0);
        let mid = a.lerp(b, 0.5);
        assert!((mid.r - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn color_lerp_clamps_t() {
        let a = Color::rgb(0.0, 0.0, 0.0);
        let b = Color::rgb(1.0, 1.0, 1.0);
        // t=2.0 → clamp to 1.0 → returns b.
        assert_eq!(a.lerp(b, 2.0), b);
        assert_eq!(a.lerp(b, -1.0), a);
    }

    #[test]
    fn color_with_alpha_overrides() {
        let c = Color::rgb(1.0, 0.0, 0.0).with_alpha(0.5);
        assert!((c.a - 0.5).abs() < f32::EPSILON);
        assert!((c.r - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn theme_dark_default_resolves_all_slots() {
        let t = Theme::default();
        // All slots should resolve to non-transparent colour.
        for slot in [
            ThemeSlot::Background,
            ThemeSlot::Foreground,
            ThemeSlot::ForegroundMuted,
            ThemeSlot::ButtonFace,
            ThemeSlot::ButtonHover,
            ThemeSlot::ButtonActive,
            ThemeSlot::Border,
            ThemeSlot::Accent,
            ThemeSlot::AccentMuted,
            ThemeSlot::Selection,
            ThemeSlot::Disabled,
            ThemeSlot::Error,
            ThemeSlot::Caret,
        ] {
            let c = t.color(slot);
            assert!(c.a > 0.0, "slot {slot:?} resolved to transparent");
        }
    }

    #[test]
    fn theme_light_distinct_from_dark() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert_ne!(
            dark.color(ThemeSlot::Background),
            light.color(ThemeSlot::Background)
        );
    }

    #[test]
    fn theme_with_color_overrides() {
        let t = Theme::dark().with_color(ThemeSlot::Accent, Color::rgb(1.0, 0.0, 0.5));
        let c = t.color(ThemeSlot::Accent);
        assert!((c.r - 1.0).abs() < f32::EPSILON);
        assert!(c.g.abs() < f32::EPSILON);
    }

    #[test]
    fn spacing_default_ascending() {
        let s = Spacing::DEFAULT;
        assert!(s.tight < s.normal);
        assert!(s.normal < s.loose);
    }

    #[test]
    fn font_style_default_is_system_14() {
        let f = FontStyle::default();
        assert_eq!(f.family, "system");
        assert!((f.size_px - 14.0).abs() < f32::EPSILON);
        assert_eq!(f.weight, FontWeight::Regular);
        assert!(!f.italic);
    }
}
