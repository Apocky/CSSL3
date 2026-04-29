//! § Layout — two-pass solver + container variants.
//!
//! § ROLE
//!   Layout in cssl-ui follows the classic two-pass pattern :
//!     PASS 1 (BOTTOM-UP) : every widget computes its `min_size` given the
//!       parent's `LayoutConstraint`. Containers recurse into children and
//!       sum / max their sizes per the container's policy.
//!     PASS 2 (TOP-DOWN) : the parent assigns each child a final `size +
//!       origin`, propagating constraints from outside in.
//!
//!   This module exposes the `LayoutConstraint` carrier + the `Container`
//!   enum + the solver entry-points. Per-widget layout impls live in the
//!   `widgets/` module + the `Widget::layout` / `Widget::assign_final_size`
//!   trait methods.
//!
//! § CONTAINER VARIANTS
//!   - `Vbox`     — children stacked top-to-bottom ; width = max(children).
//!   - `Hbox`     — children side-by-side left-to-right ; height = max.
//!   - `Grid`     — fixed-row + fixed-column grid ; `cols : u8`.
//!   - `Stack`    — children layered Z-overlay (last child on top).
//!   - `Flex`     — Vbox / Hbox with weighted flexible child sizing.
//!   - `Absolute` — children placed at explicit `(x, y)` ; size honours
//!                  child's `min_size`.
//!
//! § AXIS PRIORITY
//!   `LayoutConstraint::axis_priority` indicates which axis is the
//!   "primary" growth direction — drives Flex weight distribution. Default
//!   `Vertical`.
//!
//! § PRIME-DIRECTIVE attestation
//!   Layout is pure compute over input → output ; deterministic ; no IO.

use crate::geometry::{Insets, Point, Rect, Size};

/// Layout constraint passed from parent → child during pass-1.
///
/// `min_size` and `max_size` bound the child's preferred size ; the child
/// returns a [`Size`] anywhere in `min_size..=max_size`. The parent then
/// computes a final size + origin during pass-2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConstraint {
    pub min_size: Size,
    pub max_size: Size,
    pub axis_priority: Axis,
}

impl LayoutConstraint {
    /// Construct an unbounded constraint — child may pick any size.
    #[must_use]
    pub fn unbounded() -> Self {
        Self {
            min_size: Size::ZERO,
            max_size: Size::new(f32::INFINITY, f32::INFINITY),
            axis_priority: Axis::Vertical,
        }
    }

    /// Construct a tight constraint — child MUST size to `size`.
    #[must_use]
    pub fn tight(size: Size) -> Self {
        Self {
            min_size: size,
            max_size: size,
            axis_priority: Axis::Vertical,
        }
    }

    /// Construct a bounded constraint with `0..=max`.
    #[must_use]
    pub fn loose(max: Size) -> Self {
        Self {
            min_size: Size::ZERO,
            max_size: max,
            axis_priority: Axis::Vertical,
        }
    }

    /// Override the axis priority.
    #[must_use]
    pub fn with_axis(mut self, axis: Axis) -> Self {
        self.axis_priority = axis;
        self
    }

    /// Clamp `size` into the constraint range.
    #[must_use]
    pub fn clamp(&self, size: Size) -> Size {
        Size::new(
            size.w.clamp(self.min_size.w, self.max_size.w),
            size.h.clamp(self.min_size.h, self.max_size.h),
        )
    }

    /// Strip `insets` from the max size (subtract padding) and from the
    /// min size (clamped at zero) ; used to derive child constraint inside
    /// a padded container.
    #[must_use]
    pub fn shrink(&self, insets: Insets) -> Self {
        Self {
            min_size: self.min_size.shrink(insets),
            max_size: self.max_size.shrink(insets),
            axis_priority: self.axis_priority,
        }
    }
}

/// 2-D axis discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

impl Axis {
    /// Return the orthogonal axis.
    #[must_use]
    pub fn cross(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }

    /// Project a [`Size`] onto this axis (returns the relevant component).
    #[must_use]
    pub fn major(self, size: Size) -> f32 {
        match self {
            Self::Horizontal => size.w,
            Self::Vertical => size.h,
        }
    }

    /// Project a [`Size`] onto the orthogonal axis.
    #[must_use]
    pub fn minor(self, size: Size) -> f32 {
        match self {
            Self::Horizontal => size.h,
            Self::Vertical => size.w,
        }
    }

    /// Construct a [`Size`] from major + minor extents on this axis.
    #[must_use]
    pub fn make_size(self, major: f32, minor: f32) -> Size {
        match self {
            Self::Horizontal => Size::new(major, minor),
            Self::Vertical => Size::new(minor, major),
        }
    }
}

/// Cross-axis alignment when a child is smaller than the cross-axis extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrossAlign {
    #[default]
    Start,
    Center,
    End,
    /// Stretch the child to fill the cross axis.
    Stretch,
}

/// Main-axis alignment when children don't fill the available extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainAlign {
    #[default]
    Start,
    Center,
    End,
    /// Equal space between children, none at the ends.
    SpaceBetween,
    /// Equal space around children, including ends.
    SpaceAround,
}

/// One child entry inside a container ; stores the resolved `Rect` after
/// pass-2 plus the index back into the widget array the container holds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChildSlot {
    pub frame: Rect,
    /// Optional flex weight ; `0.0` = inflexible.
    pub flex: f32,
}

/// Container variants : enum-dispatch makes the layout solver a flat match
/// rather than a `Box<dyn Container>` per node.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum Container {
    /// Vertical box — children stacked top to bottom.
    #[default]
    Vbox,
    /// Horizontal box — children side by side.
    Hbox,
    /// Grid container — N columns ; rows derived from child count.
    Grid { cols: u8 },
    /// Stack container — children Z-overlay.
    Stack,
    /// Flex container — `Vbox` / `Hbox` with weighted children.
    Flex { axis: Axis },
    /// Absolute container — children placed at explicit positions.
    Absolute,
}

/// Auxiliary parameters every container honours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainerStyle {
    pub padding: Insets,
    pub gap: f32,
    pub main_align: MainAlign,
    pub cross_align: CrossAlign,
}

impl Default for ContainerStyle {
    fn default() -> Self {
        Self {
            padding: Insets::ZERO,
            gap: 0.0,
            main_align: MainAlign::Start,
            cross_align: CrossAlign::Start,
        }
    }
}

/// Per-child entry the solver consumes : the child's `min_size` (from
/// pass-1) plus optional flex weight + absolute-position offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChildLayoutInput {
    pub min_size: Size,
    pub flex: f32,
    /// Used only by `Container::Absolute` ; ignored otherwise.
    pub absolute_origin: Point,
}

impl Default for ChildLayoutInput {
    fn default() -> Self {
        Self {
            min_size: Size::ZERO,
            flex: 0.0,
            absolute_origin: Point::ORIGIN,
        }
    }
}

/// Run the two-pass solver for one container given pass-1 child sizes.
///
/// # Pass 1 (caller's responsibility)
///   The caller has already computed each child's `min_size` (typically by
///   walking the widget tree bottom-up). The results are passed in as
///   `children[i].min_size`.
///
/// # Pass 2 (this function)
///   Given the container's own constraint + style + children's pass-1
///   results, produce one `ChildSlot` per child with its final `frame`
///   (origin + size) inside the container's local coordinate system.
///
/// # Returns
///   `(container_size, child_slots)` — the container's own final size
///   (always inside the input constraint) and the per-child frames.
#[must_use]
pub fn solve_container(
    container: &Container,
    constraint: LayoutConstraint,
    style: ContainerStyle,
    children: &[ChildLayoutInput],
) -> (Size, Vec<ChildSlot>) {
    match container {
        Container::Vbox => solve_axis_box(Axis::Vertical, constraint, style, children, false),
        Container::Hbox => solve_axis_box(Axis::Horizontal, constraint, style, children, false),
        Container::Flex { axis } => solve_axis_box(*axis, constraint, style, children, true),
        Container::Stack => solve_stack(constraint, style, children),
        Container::Absolute => solve_absolute(constraint, style, children),
        Container::Grid { cols } => solve_grid(*cols, constraint, style, children),
    }
}

fn solve_axis_box(
    axis: Axis,
    constraint: LayoutConstraint,
    style: ContainerStyle,
    children: &[ChildLayoutInput],
    enable_flex: bool,
) -> (Size, Vec<ChildSlot>) {
    let pad = style.padding;
    let gap = style.gap;
    let inner_max = constraint.max_size.shrink(pad);

    // Pass 2a : sum children's main-axis min, find children's cross-axis max.
    let mut total_min_major: f32 = 0.0;
    let mut total_flex: f32 = 0.0;
    let mut max_minor: f32 = 0.0;
    for (i, c) in children.iter().enumerate() {
        let major = axis.major(c.min_size);
        let minor = axis.minor(c.min_size);
        total_min_major += major;
        if i > 0 {
            total_min_major += gap;
        }
        if enable_flex {
            total_flex += c.flex;
        }
        if minor > max_minor {
            max_minor = minor;
        }
    }

    // Available space for flex distribution.
    let available_major = axis.major(inner_max);
    let extra_major = (available_major - total_min_major).max(0.0);
    let flex_unit = if total_flex > 0.0 {
        extra_major / total_flex
    } else {
        0.0
    };

    // Pass 2b : assign final frames.
    let mut slots = Vec::with_capacity(children.len());
    let mut cursor_major = match axis {
        Axis::Horizontal => pad.left,
        Axis::Vertical => pad.top,
    };
    let cross_origin = match axis {
        Axis::Horizontal => pad.top,
        Axis::Vertical => pad.left,
    };

    // For SpaceBetween / SpaceAround we need a different cursor strategy.
    // Compute leftover only when alignment requires it.
    let used_major: f32 = total_min_major + total_flex * flex_unit;
    let leftover_major = (available_major - used_major).max(0.0);
    let (lead_offset, between_offset) = match style.main_align {
        MainAlign::Start => (0.0_f32, 0.0_f32),
        MainAlign::Center => (leftover_major * 0.5, 0.0),
        MainAlign::End => (leftover_major, 0.0),
        MainAlign::SpaceBetween if children.len() > 1 => {
            (0.0, leftover_major / (children.len() as f32 - 1.0))
        }
        MainAlign::SpaceBetween => (leftover_major * 0.5, 0.0),
        MainAlign::SpaceAround => {
            let slot = leftover_major / (children.len() as f32 + 1.0).max(1.0);
            (slot, slot)
        }
    };

    cursor_major += lead_offset;

    for (i, c) in children.iter().enumerate() {
        let mut child_major = axis.major(c.min_size);
        if enable_flex && c.flex > 0.0 {
            child_major += c.flex * flex_unit;
        }
        let child_minor_min = axis.minor(c.min_size);
        let child_minor = match style.cross_align {
            CrossAlign::Stretch => axis.minor(inner_max),
            _ => child_minor_min,
        };
        let cross_offset = match style.cross_align {
            CrossAlign::Start | CrossAlign::Stretch => 0.0,
            CrossAlign::Center => (axis.minor(inner_max) - child_minor) * 0.5,
            CrossAlign::End => axis.minor(inner_max) - child_minor,
        };
        let origin = match axis {
            Axis::Horizontal => Point::new(cursor_major, cross_origin + cross_offset),
            Axis::Vertical => Point::new(cross_origin + cross_offset, cursor_major),
        };
        let size = axis.make_size(child_major, child_minor);
        slots.push(ChildSlot {
            frame: Rect::new(origin, size),
            flex: c.flex,
        });
        cursor_major += child_major;
        if i + 1 < children.len() {
            cursor_major += gap + between_offset;
        }
    }

    // resolved_major = cursor_major (which already includes pad.start +
    // sum(children) + interior gaps) + pad.end on the closing edge.
    let resolved_major = cursor_major
        + match axis {
            Axis::Horizontal => pad.right,
            Axis::Vertical => pad.bottom,
        };
    let resolved_minor = max_minor + axis.minor(Size::new(pad.horizontal(), pad.vertical()));
    let resolved = constraint.clamp(axis.make_size(resolved_major, resolved_minor));
    (resolved, slots)
}

fn solve_stack(
    constraint: LayoutConstraint,
    style: ContainerStyle,
    children: &[ChildLayoutInput],
) -> (Size, Vec<ChildSlot>) {
    // Stack : every child gets its min_size at origin = padding.left/top.
    // Container size = max child size + padding.
    let pad = style.padding;
    let mut max_size = Size::ZERO;
    for c in children {
        if c.min_size.w > max_size.w {
            max_size.w = c.min_size.w;
        }
        if c.min_size.h > max_size.h {
            max_size.h = c.min_size.h;
        }
    }
    let resolved = constraint.clamp(max_size.expand(pad));
    let inner = resolved.shrink(pad);
    let slots = children
        .iter()
        .map(|c| {
            let size = c.min_size;
            let origin = match style.cross_align {
                CrossAlign::Stretch => Point::new(pad.left, pad.top),
                CrossAlign::Center => Point::new(
                    pad.left + (inner.w - size.w) * 0.5,
                    pad.top + (inner.h - size.h) * 0.5,
                ),
                CrossAlign::End => {
                    Point::new(pad.left + (inner.w - size.w), pad.top + (inner.h - size.h))
                }
                CrossAlign::Start => Point::new(pad.left, pad.top),
            };
            let final_size = if style.cross_align == CrossAlign::Stretch {
                inner
            } else {
                size
            };
            ChildSlot {
                frame: Rect::new(origin, final_size),
                flex: c.flex,
            }
        })
        .collect();
    (resolved, slots)
}

fn solve_absolute(
    constraint: LayoutConstraint,
    style: ContainerStyle,
    children: &[ChildLayoutInput],
) -> (Size, Vec<ChildSlot>) {
    let pad = style.padding;
    let mut bounds = Size::ZERO;
    let slots: Vec<_> = children
        .iter()
        .map(|c| {
            let origin = Point::new(
                c.absolute_origin.x + pad.left,
                c.absolute_origin.y + pad.top,
            );
            let frame = Rect::new(origin, c.min_size);
            let max = frame.max();
            if max.x > bounds.w {
                bounds.w = max.x;
            }
            if max.y > bounds.h {
                bounds.h = max.y;
            }
            ChildSlot {
                frame,
                flex: c.flex,
            }
        })
        .collect();
    let resolved = constraint.clamp(Size::new(bounds.w + pad.right, bounds.h + pad.bottom));
    (resolved, slots)
}

fn solve_grid(
    cols: u8,
    constraint: LayoutConstraint,
    style: ContainerStyle,
    children: &[ChildLayoutInput],
) -> (Size, Vec<ChildSlot>) {
    let cols = cols.max(1) as usize;
    let pad = style.padding;
    let gap = style.gap;
    let n = children.len();
    let rows = n.div_ceil(cols);

    // Per-column max width + per-row max height.
    let mut col_widths = vec![0.0_f32; cols];
    let mut row_heights = vec![0.0_f32; rows];
    for (i, c) in children.iter().enumerate() {
        let r = i / cols;
        let cl = i % cols;
        if c.min_size.w > col_widths[cl] {
            col_widths[cl] = c.min_size.w;
        }
        if c.min_size.h > row_heights[r] {
            row_heights[r] = c.min_size.h;
        }
    }

    // Compute column x-origins + row y-origins.
    let mut col_x = Vec::with_capacity(cols);
    let mut x = pad.left;
    for w in &col_widths {
        col_x.push(x);
        x += *w + gap;
    }
    let mut row_y = Vec::with_capacity(rows);
    let mut y = pad.top;
    for h in &row_heights {
        row_y.push(y);
        y += *h + gap;
    }

    // Fill slots.
    let slots = children
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let r = i / cols;
            let cl = i % cols;
            let frame = Rect::new(Point::new(col_x[cl], row_y[r]), c.min_size);
            ChildSlot {
                frame,
                flex: c.flex,
            }
        })
        .collect();

    let total_w: f32 = col_widths.iter().copied().sum::<f32>()
        + (cols.saturating_sub(1) as f32) * gap
        + pad.horizontal();
    let total_h: f32 = row_heights.iter().copied().sum::<f32>()
        + (rows.saturating_sub(1) as f32) * gap
        + pad.vertical();
    (constraint.clamp(Size::new(total_w, total_h)), slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child_size(w: f32, h: f32) -> ChildLayoutInput {
        ChildLayoutInput {
            min_size: Size::new(w, h),
            flex: 0.0,
            absolute_origin: Point::ORIGIN,
        }
    }

    #[test]
    fn constraint_unbounded_clamp_passes_through() {
        let c = LayoutConstraint::unbounded();
        assert_eq!(c.clamp(Size::new(10.0, 20.0)), Size::new(10.0, 20.0));
    }

    #[test]
    fn constraint_tight_forces_size() {
        let c = LayoutConstraint::tight(Size::new(50.0, 30.0));
        assert_eq!(c.clamp(Size::new(10.0, 20.0)), Size::new(50.0, 30.0));
    }

    #[test]
    fn constraint_loose_clamps_max() {
        let c = LayoutConstraint::loose(Size::new(100.0, 100.0));
        assert_eq!(c.clamp(Size::new(50.0, 50.0)), Size::new(50.0, 50.0));
        assert_eq!(c.clamp(Size::new(200.0, 200.0)), Size::new(100.0, 100.0));
    }

    #[test]
    fn constraint_shrink_strips_padding() {
        let c = LayoutConstraint::loose(Size::new(100.0, 100.0));
        let inner = c.shrink(Insets::uniform(10.0));
        assert_eq!(inner.max_size, Size::new(80.0, 80.0));
    }

    #[test]
    fn axis_major_minor_swap() {
        let s = Size::new(10.0, 20.0);
        assert!((Axis::Horizontal.major(s) - 10.0).abs() < f32::EPSILON);
        assert!((Axis::Horizontal.minor(s) - 20.0).abs() < f32::EPSILON);
        assert!((Axis::Vertical.major(s) - 20.0).abs() < f32::EPSILON);
        assert!((Axis::Vertical.minor(s) - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn axis_cross_inverts() {
        assert_eq!(Axis::Horizontal.cross(), Axis::Vertical);
        assert_eq!(Axis::Vertical.cross(), Axis::Horizontal);
    }

    #[test]
    fn vbox_stacks_children_top_to_bottom() {
        let kids = [child_size(50.0, 20.0), child_size(50.0, 30.0)];
        let constraint = LayoutConstraint::loose(Size::new(200.0, 200.0));
        let (size, slots) = solve_container(
            &Container::Vbox,
            constraint,
            ContainerStyle::default(),
            &kids,
        );
        assert_eq!(slots.len(), 2);
        // First child at y=0, second at y=20 (no gap).
        assert!((slots[0].frame.origin.y).abs() < f32::EPSILON);
        assert!((slots[1].frame.origin.y - 20.0).abs() < f32::EPSILON);
        // Container height = 50.
        assert!((size.h - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn vbox_with_gap_inserts_separation() {
        let kids = [child_size(50.0, 20.0), child_size(50.0, 30.0)];
        let style = ContainerStyle {
            gap: 4.0,
            ..Default::default()
        };
        let constraint = LayoutConstraint::loose(Size::new(200.0, 200.0));
        let (_, slots) = solve_container(&Container::Vbox, constraint, style, &kids);
        // Second child at y = 20 + gap(4) = 24.
        assert!((slots[1].frame.origin.y - 24.0).abs() < f32::EPSILON);
    }

    #[test]
    fn hbox_stacks_children_left_to_right() {
        let kids = [child_size(20.0, 50.0), child_size(30.0, 50.0)];
        let constraint = LayoutConstraint::loose(Size::new(200.0, 200.0));
        let (size, slots) = solve_container(
            &Container::Hbox,
            constraint,
            ContainerStyle::default(),
            &kids,
        );
        assert!((slots[0].frame.origin.x).abs() < f32::EPSILON);
        assert!((slots[1].frame.origin.x - 20.0).abs() < f32::EPSILON);
        assert!((size.w - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn flex_distributes_extra_space_by_weight() {
        let mut a = child_size(10.0, 20.0);
        let mut b = child_size(10.0, 20.0);
        a.flex = 1.0;
        b.flex = 2.0;
        let kids = [a, b];
        let constraint = LayoutConstraint::tight(Size::new(100.0, 100.0));
        let (_, slots) = solve_container(
            &Container::Flex {
                axis: Axis::Horizontal,
            },
            constraint,
            ContainerStyle::default(),
            &kids,
        );
        // Total min = 20 ; available = 100 - 20 = 80 ; weights 1+2=3 ;
        // a gets 80/3 ≈ 26.67 extra, b gets 53.33. Final widths : a≈36.67, b≈63.33.
        let total = slots[0].frame.size.w + slots[1].frame.size.w;
        assert!((total - 100.0).abs() < 0.5);
        assert!(slots[1].frame.size.w > slots[0].frame.size.w);
    }

    #[test]
    fn stack_overlays_children_at_origin() {
        let kids = [child_size(40.0, 30.0), child_size(20.0, 10.0)];
        let constraint = LayoutConstraint::loose(Size::new(100.0, 100.0));
        let (size, slots) = solve_container(
            &Container::Stack,
            constraint,
            ContainerStyle::default(),
            &kids,
        );
        // Stack size = max(40,20) x max(30,10) = 40 x 30.
        assert!((size.w - 40.0).abs() < f32::EPSILON);
        assert!((size.h - 30.0).abs() < f32::EPSILON);
        // Both at origin (Start alignment).
        assert!(slots[0].frame.origin.x.abs() < f32::EPSILON);
        assert!(slots[1].frame.origin.x.abs() < f32::EPSILON);
    }

    #[test]
    fn absolute_honors_explicit_origin() {
        let mut a = child_size(20.0, 20.0);
        let mut b = child_size(20.0, 20.0);
        a.absolute_origin = Point::new(10.0, 5.0);
        b.absolute_origin = Point::new(50.0, 50.0);
        let kids = [a, b];
        let (size, slots) = solve_container(
            &Container::Absolute,
            LayoutConstraint::loose(Size::new(200.0, 200.0)),
            ContainerStyle::default(),
            &kids,
        );
        assert!((slots[0].frame.origin.x - 10.0).abs() < f32::EPSILON);
        assert!((slots[1].frame.origin.x - 50.0).abs() < f32::EPSILON);
        // Bounds = 70 x 70 (b at 50,50 + size 20,20).
        assert!((size.w - 70.0).abs() < f32::EPSILON);
    }

    #[test]
    fn grid_three_cols_two_rows() {
        let kids = [
            child_size(20.0, 20.0),
            child_size(30.0, 20.0),
            child_size(40.0, 20.0),
            child_size(20.0, 30.0),
            child_size(30.0, 30.0),
        ];
        let constraint = LayoutConstraint::loose(Size::new(500.0, 500.0));
        let (size, slots) = solve_container(
            &Container::Grid { cols: 3 },
            constraint,
            ContainerStyle::default(),
            &kids,
        );
        // Col widths : 20, 30, 40 ; row heights : 20, 30.
        // First row at y=0, second row at y=20.
        assert!(slots[0].frame.origin.y.abs() < f32::EPSILON);
        assert!((slots[3].frame.origin.y - 20.0).abs() < f32::EPSILON);
        // Total grid width = 20+30+40 = 90.
        assert!((size.w - 90.0).abs() < f32::EPSILON);
        // Total grid height = 20+30 = 50.
        assert!((size.h - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn vbox_padding_offsets_children() {
        let kids = [child_size(50.0, 20.0)];
        let style = ContainerStyle {
            padding: Insets::uniform(8.0),
            ..Default::default()
        };
        let (size, slots) = solve_container(
            &Container::Vbox,
            LayoutConstraint::loose(Size::new(200.0, 200.0)),
            style,
            &kids,
        );
        assert!((slots[0].frame.origin.x - 8.0).abs() < f32::EPSILON);
        assert!((slots[0].frame.origin.y - 8.0).abs() < f32::EPSILON);
        // Container = padding + child + padding = 8 + 50 + 8 = 66 wide.
        assert!((size.w - 66.0).abs() < f32::EPSILON);
        // Container = 8 + 20 + 8 = 36 high.
        assert!((size.h - 36.0).abs() < f32::EPSILON);
    }

    #[test]
    fn vbox_main_align_center_offsets_lead() {
        let kids = [child_size(50.0, 20.0)];
        let style = ContainerStyle {
            main_align: MainAlign::Center,
            ..Default::default()
        };
        let constraint = LayoutConstraint::tight(Size::new(50.0, 100.0));
        let (_, slots) = solve_container(&Container::Vbox, constraint, style, &kids);
        // Leftover = 80 ; centred → 40px lead.
        assert!((slots[0].frame.origin.y - 40.0).abs() < f32::EPSILON);
    }

    #[test]
    fn vbox_cross_align_stretch_widens_child() {
        let kids = [child_size(20.0, 20.0)];
        let style = ContainerStyle {
            cross_align: CrossAlign::Stretch,
            ..Default::default()
        };
        let constraint = LayoutConstraint::tight(Size::new(80.0, 100.0));
        let (_, slots) = solve_container(&Container::Vbox, constraint, style, &kids);
        // Cross axis = horizontal in vbox ; child width = 80.
        assert!((slots[0].frame.size.w - 80.0).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_vbox_resolves_to_padding_only() {
        let style = ContainerStyle {
            padding: Insets::uniform(10.0),
            ..Default::default()
        };
        let (size, slots) = solve_container(
            &Container::Vbox,
            LayoutConstraint::loose(Size::new(100.0, 100.0)),
            style,
            &[],
        );
        assert_eq!(slots.len(), 0);
        // Width = padding L+R = 20 ; height = padding T+B = 20.
        assert!((size.w - 20.0).abs() < f32::EPSILON);
        assert!((size.h - 20.0).abs() < f32::EPSILON);
    }
}
