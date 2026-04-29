//! § Geometry primitives — `Size` / `Point` / `Rect` / `Insets`.
//!
//! § ROLE
//!   The cssl-ui framework is pure 2-D in pixel-space at S9-U1. Layout works
//!   with f32 pixels (subpixel placement is fine ; the painter rounds at
//!   submit-time per the platform convention). The widget tree never touches
//!   3-D matrices — that's a future cssl-render concern.
//!
//! § DESIGN
//!   - `Point { x, y }` : a 2-D position (origin = top-left of containing
//!     widget unless explicitly noted screen-relative).
//!   - `Size { w, h }`  : a 2-D extent. Both fields are non-negative by
//!     convention — negative values lower to clamped-zero on use rather than
//!     panicking. Width and height are kept independent so single-axis
//!     constraints can be expressed.
//!   - `Rect`           : a [`Point`] + [`Size`] pair. Helper methods give
//!     `min` / `max` / `center` / `contains` / `intersects`.
//!   - `Insets`         : a 4-tuple of `(left, top, right, bottom)` for
//!     padding / margin. Supports `uniform` / `symmetric` / `zero` builders.
//!
//! § INVARIANTS
//!   - `Size::w >= 0` and `Size::h >= 0` after `.normalised()`.
//!   - `Rect::min().x <= Rect::max().x` and same for y.
//!
//! § PRIME-DIRECTIVE attestation
//!   Geometry is pure data — no side-effects, no IO, no surveillance vector.

/// 2-D position in pixel-space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    /// Origin point at `(0.0, 0.0)`.
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    /// Construct a new point.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Translate by `delta` ; returns a new point.
    #[must_use]
    pub fn translate(self, delta: Self) -> Self {
        Self {
            x: self.x + delta.x,
            y: self.y + delta.y,
        }
    }

    /// Squared distance — avoids the sqrt for hover-test loops.
    #[must_use]
    pub fn distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx.mul_add(dx, dy * dy)
    }
}

/// 2-D extent in pixel-space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    /// Zero size.
    pub const ZERO: Self = Self { w: 0.0, h: 0.0 };

    /// Construct a new size.
    #[must_use]
    pub const fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }

    /// Build a size with both dimensions equal to `s`.
    #[must_use]
    pub const fn square(s: f32) -> Self {
        Self { w: s, h: s }
    }

    /// Clamp negative widths / heights to zero. Layout solvers should never
    /// emit negatives ; this is a defensive convergence helper.
    #[must_use]
    pub fn normalised(self) -> Self {
        Self {
            w: self.w.max(0.0),
            h: self.h.max(0.0),
        }
    }

    /// Per-axis min with another size.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self {
            w: self.w.min(other.w),
            h: self.h.min(other.h),
        }
    }

    /// Per-axis max with another size.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self {
            w: self.w.max(other.w),
            h: self.h.max(other.h),
        }
    }

    /// Total area in pixels-squared.
    #[must_use]
    pub fn area(self) -> f32 {
        self.w * self.h
    }

    /// Add `insets` to expand this size — used when wrapping a content size
    /// with padding.
    #[must_use]
    pub fn expand(self, insets: Insets) -> Self {
        Self {
            w: self.w + insets.left + insets.right,
            h: self.h + insets.top + insets.bottom,
        }
    }

    /// Subtract `insets` to contract this size — used when stripping padding
    /// from a parent constraint to compute child constraint.
    #[must_use]
    pub fn shrink(self, insets: Insets) -> Self {
        Self {
            w: (self.w - insets.left - insets.right).max(0.0),
            h: (self.h - insets.top - insets.bottom).max(0.0),
        }
    }
}

/// Axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    /// Empty rectangle at origin.
    pub const EMPTY: Self = Self {
        origin: Point::ORIGIN,
        size: Size::ZERO,
    };

    /// Construct from origin + size.
    #[must_use]
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// Construct from two corner points (any ordering).
    #[must_use]
    pub fn from_corners(a: Point, b: Point) -> Self {
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        Self {
            origin: Point::new(min_x, min_y),
            size: Size::new(max_x - min_x, max_y - min_y),
        }
    }

    /// Top-left point.
    #[must_use]
    pub fn min(self) -> Point {
        self.origin
    }

    /// Bottom-right point.
    #[must_use]
    pub fn max(self) -> Point {
        Point::new(self.origin.x + self.size.w, self.origin.y + self.size.h)
    }

    /// Centre point.
    #[must_use]
    pub fn center(self) -> Point {
        Point::new(
            self.origin.x + self.size.w * 0.5,
            self.origin.y + self.size.h * 0.5,
        )
    }

    /// `true` if `p` lies inside the rectangle (right + bottom edges
    /// exclusive — matches the half-open convention used by the layout
    /// solver). Negative-size rects always return `false`.
    #[must_use]
    pub fn contains(self, p: Point) -> bool {
        if self.size.w <= 0.0 || self.size.h <= 0.0 {
            return false;
        }
        p.x >= self.origin.x
            && p.y >= self.origin.y
            && p.x < self.origin.x + self.size.w
            && p.y < self.origin.y + self.size.h
    }

    /// `true` if `other` overlaps this rect (any non-empty intersection).
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        let a_min = self.min();
        let a_max = self.max();
        let b_min = other.min();
        let b_max = other.max();
        a_min.x < b_max.x && a_max.x > b_min.x && a_min.y < b_max.y && a_max.y > b_min.y
    }

    /// Translate the rectangle by `delta`.
    #[must_use]
    pub fn translate(self, delta: Point) -> Self {
        Self {
            origin: self.origin.translate(delta),
            size: self.size,
        }
    }

    /// Inset the rectangle by `insets` ; the size shrinks accordingly.
    #[must_use]
    pub fn inset(self, insets: Insets) -> Self {
        Self {
            origin: Point::new(self.origin.x + insets.left, self.origin.y + insets.top),
            size: self.size.shrink(insets),
        }
    }
}

/// 4-tuple of edge insets.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    /// Zero insets.
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    /// Construct from explicit edges.
    #[must_use]
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Same value on every edge.
    #[must_use]
    pub const fn uniform(value: f32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    /// Same value on left/right and top/bottom.
    #[must_use]
    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }

    /// Total horizontal padding (left + right).
    #[must_use]
    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    /// Total vertical padding (top + bottom).
    #[must_use]
    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_translate_round_trip() {
        let p = Point::new(10.0, 20.0);
        let q = p.translate(Point::new(5.0, -3.0));
        assert_eq!(q, Point::new(15.0, 17.0));
    }

    #[test]
    fn point_distance_squared_zero_when_equal() {
        let p = Point::new(1.0, 2.0);
        assert!(p.distance_squared(p).abs() < f32::EPSILON);
    }

    #[test]
    fn point_distance_squared_pythagoras() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(3.0, 4.0);
        assert!((a.distance_squared(b) - 25.0).abs() < f32::EPSILON);
    }

    #[test]
    fn size_normalises_negatives_to_zero() {
        let s = Size::new(-1.0, 5.0).normalised();
        assert_eq!(s, Size::new(0.0, 5.0));
    }

    #[test]
    fn size_min_max_per_axis() {
        let a = Size::new(10.0, 20.0);
        let b = Size::new(5.0, 30.0);
        assert_eq!(a.min(b), Size::new(5.0, 20.0));
        assert_eq!(a.max(b), Size::new(10.0, 30.0));
    }

    #[test]
    fn size_expand_and_shrink_with_insets() {
        let s = Size::new(100.0, 50.0);
        let pad = Insets::uniform(8.0);
        assert_eq!(s.expand(pad), Size::new(116.0, 66.0));
        assert_eq!(s.shrink(pad), Size::new(84.0, 34.0));
    }

    #[test]
    fn size_shrink_clamps_to_zero() {
        let s = Size::new(10.0, 10.0).shrink(Insets::uniform(8.0));
        assert_eq!(s, Size::new(0.0, 0.0)); // 10 - 16 → 0 (clamped)
    }

    #[test]
    fn rect_contains_inside() {
        let r = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0));
        assert!(r.contains(Point::new(5.0, 5.0)));
    }

    #[test]
    fn rect_contains_excludes_far_edge() {
        let r = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0));
        // Top-left corner inclusive.
        assert!(r.contains(Point::new(0.0, 0.0)));
        // Bottom-right corner exclusive.
        assert!(!r.contains(Point::new(10.0, 10.0)));
    }

    #[test]
    fn rect_contains_rejects_outside() {
        let r = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0));
        assert!(!r.contains(Point::new(-1.0, 5.0)));
        assert!(!r.contains(Point::new(15.0, 5.0)));
    }

    #[test]
    fn rect_empty_never_contains() {
        let r = Rect::EMPTY;
        assert!(!r.contains(Point::new(0.0, 0.0)));
    }

    #[test]
    fn rect_intersects_overlap() {
        let a = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0));
        let b = Rect::new(Point::new(5.0, 5.0), Size::new(10.0, 10.0));
        assert!(a.intersects(b));
    }

    #[test]
    fn rect_intersects_disjoint() {
        let a = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0));
        let b = Rect::new(Point::new(20.0, 20.0), Size::new(10.0, 10.0));
        assert!(!a.intersects(b));
    }

    #[test]
    fn rect_center_midpoint() {
        let r = Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 20.0));
        assert_eq!(r.center(), Point::new(5.0, 10.0));
    }

    #[test]
    fn rect_from_corners_orders_min_max() {
        let r = Rect::from_corners(Point::new(10.0, 5.0), Point::new(2.0, 8.0));
        assert_eq!(r.min(), Point::new(2.0, 5.0));
        assert_eq!(r.max(), Point::new(10.0, 8.0));
    }

    #[test]
    fn rect_inset_shrinks() {
        let r = Rect::new(Point::new(0.0, 0.0), Size::new(100.0, 50.0));
        let inner = r.inset(Insets::uniform(10.0));
        assert_eq!(inner.origin, Point::new(10.0, 10.0));
        assert_eq!(inner.size, Size::new(80.0, 30.0));
    }

    #[test]
    fn insets_uniform_sets_all_edges() {
        let i = Insets::uniform(8.0);
        assert_eq!(i.left, 8.0);
        assert_eq!(i.top, 8.0);
        assert_eq!(i.right, 8.0);
        assert_eq!(i.bottom, 8.0);
    }

    #[test]
    fn insets_symmetric_pairs() {
        let i = Insets::symmetric(4.0, 8.0);
        assert!((i.horizontal() - 8.0).abs() < f32::EPSILON);
        assert!((i.vertical() - 16.0).abs() < f32::EPSILON);
    }
}
