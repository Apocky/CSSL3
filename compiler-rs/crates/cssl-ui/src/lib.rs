//! CSSLv3 stage-0 — UI framework (S9-U1).
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS` (UI is the engine-
//!          side surface that consumes window + input host backends ; it
//!          submits its rendering through a `Painter` trait that a future
//!          cssl-render slice (R1) consumes through the render-graph) +
//!          `PRIME_DIRECTIVE.md § 1 PROHIBITIONS § surveillance`.
//!
//! § T11-D109 (S9-U1) — Session-9, U-axis slice 1 (engine-side UI).
//!
//! § ROLE
//!   This crate provides the engine-side UI framework :
//!     - **Widgets** — `Button`, `Label`, `Checkbox`, `RadioGroup`,
//!       `Slider`, `TextInput`, `Dropdown`, `List`, `TreeView`, `TabPanel`,
//!       `ScrollView`, `Image`, `ProgressBar`.
//!     - **Layout** — `LayoutConstraint` + `Container::{Vbox, Hbox, Grid,
//!       Stack, Flex, Absolute}` two-pass solver.
//!     - **Immediate-mode API** — `Ui::button("Save") -> bool`,
//!       `Ui::slider("vol", &mut v, 0.0, 1.0) -> bool`.
//!     - **Retained-mode API** — `RetainedTree` + `RetainedNode` with
//!       stable `WidgetId`s for animation + state retention.
//!     - **Theme** — pluggable `Theme { font, colors, spacing, hover_color
//!       }` with `dark()` and `light()` defaults.
//!     - **Input** — keyboard focus + Tab navigation + mouse hover/click +
//!       wheel-scroll. Touch / pinch / swipe deferred.
//!     - **Accessibility** — surface-only `AccessibilityRole` /
//!       `AccessibilityNode` hooks ; platform AT-bridge land in a follow-up.
//!
//! § PIPELINE
//!   1. application drains host-window events ;
//!   2. for each `WindowEventKind`, calls `UiEvent::from_window` to translate
//!      into a `UiEvent` ;
//!   3. either feeds the `UiEvent`s into an immediate-mode `Ui` (per-frame
//!      tree rebuild) or dispatches to a retained `RetainedTree` (held
//!      across frames) ;
//!   4. calls `Ui::end_frame(&mut painter)` or `RetainedTree::paint(&mut
//!      painter, theme)` to flush draw commands ;
//!   5. the painter (default `PaintList` collecting into a `Vec`) is then
//!      consumed by the render slice when it lands. Today the in-memory
//!      list is observable for test assertions + debug overlays.
//!
//! § STABLE WIDGET IDs (landmine)
//!   `WidgetId::hash_of(type_tag, label, parent_id, sibling_index)` produces
//!   a deterministic FNV-1a-64 hash so the retained-state store survives
//!   immediate-mode tree rebuilds. Same-label same-position widgets across
//!   frames retain the same id, even though the build code constructs the
//!   tree fresh per frame.
//!
//! § TWO-PASS LAYOUT (landmine)
//!   Pass-1 (bottom-up) : every widget computes its `min_size` given the
//!   parent's constraint. Pass-2 (top-down) : the parent assigns each
//!   child a final size + origin. The solver lives in `layout::
//!   solve_container` and supports `Vbox`, `Hbox`, `Grid`, `Stack`, `Flex`,
//!   `Absolute`.
//!
//! § HOVER-TEST (landmine)
//!   Layered z-order : the deepest widget under the cursor wins. Frame
//!   entries are recorded in build order ; iterating from the back finds
//!   the topmost intersecting widget.
//!
//! § FOCUS NAVIGATION (landmine)
//!   Tab order = depth-first traversal of focusable widgets ; Shift-Tab is
//!   reverse. Each focusable widget calls `FrameState::register_focusable`
//!   during its event-pass, populating an ordered list. Tab cycles forward,
//!   Shift-Tab backward, with wrap-around.
//!
//! § THEME (landmine)
//!   `Theme` is a value type with a flat `[Color; 13]` colour table keyed
//!   by `ThemeSlot`. Override slots via `Theme::with_color(slot, color)`.
//!   `Theme::dark()` and `Theme::light()` are the bundled defaults.
//!
//! § EFFECT-ROW STUB
//!   The framework brief calls for a `{Render}` effect-row on UI fns. Stage-0
//!   surfaces the row STRUCTURALLY : every drawing entry-point takes a
//!   `&mut dyn Painter`, the `Painter` trait is the only sink that can
//!   produce paint commands, and submission happens at `end_frame` time.
//!   When the cssl-effects-row plumbing extends to UI, adding
//!   `#[effect(Render)]` annotations to trait method signatures is a
//!   non-breaking pass.
//!
//! § PRIME-DIRECTIVE attestation
//!
//!   "There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody."
//!
//!   Per the slice landmines + § 1 PROHIBITIONS § surveillance :
//!   (a) NO covert input tracking. Every input event is surfaced
//!       transparently via `UiEvent` and observable through `Ui::
//!       pending_events`. The framework never polls the OS itself ;
//!       events arrive only via `Ui::feed_event`.
//!   (b) NO hidden state. Cursor position, focus owner, hover target, and
//!       the retained-state store are all observable through public
//!       accessors on `Ui`.
//!   (c) NO disk / network IO. The framework is pure compute over
//!       in-memory data.
//!   (d) NO accumulators. Events flow through and clear ; the only state
//!       carried across frames is the explicit `RetainedState` store and
//!       the focus / hover identifiers — all observable.
//!   (e) Accessibility is OPT-IN observation. The application chooses
//!       whether to call `Ui::accessibility_snapshot` ; the framework
//!       never ships labels to external endpoints.
//!
//! § FFI POLICY
//!   T1-D5 mandates `#![forbid(unsafe_code)]` per-crate. cssl-ui contains
//!   ZERO unsafe blocks — pure Rust, pure compute, no FFI. The crate
//!   re-exports the safe subset of `cssl-host-window`'s event vocabulary.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
// § Float-comparison lints fire on layout's clamp / clamp-equality checks ;
// these are intentional pixel-level comparisons.
#![allow(clippy::float_cmp)]
// § Many widgets construct Rect / Size with multiple float literals — the
// "items after statements" clippy lint flags this aggressively.
#![allow(clippy::items_after_statements)]
// § The retained tree's parent-child layout walk uses recursive helpers ;
// each helper is small, but clippy's "too_many_lines" can fire on the
// driver's dispatch_events fn. Per workspace convention.
#![allow(clippy::manual_let_else)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::missing_const_for_fn)]
// § Floating-point comparisons in layout solver are tolerated ; the
// invariants are explicit (clamp + non-negative).
#![allow(clippy::if_not_else)]
// § Several builder methods consume self by value to mirror the chained-
// builder pattern used by Theme + Insets ; clippy's "must_use_candidate"
// flag is not load-bearing here.
#![allow(clippy::unnecessary_cast)]
// § Many event-routing methods take a `&mut self` even when a particular
// branch reads only ; consistent signatures aid extensibility.
#![allow(clippy::unused_self)]
// § Layout + paint code is pixel-arithmetic-heavy — clippy's mul_add /
// suboptimal_flops suggestions add noise for mostly-readable code. The
// hot loops are tiny ; legibility trumps mul_add micro-optimisation here.
#![allow(clippy::suboptimal_flops)]
// § Several integer-to-f32 casts in layout (sibling-index, child count)
// are inherently lossy past 2^23 children, but UI trees never get that big.
#![allow(clippy::cast_precision_loss)]
// § Several state-decision sites use `match Option<T>` for readability
// over `Option::map_or` ; clippy's pedantic suggestion fires aggressively.
#![allow(clippy::option_if_let_else)]
// § Several `let _ = if let ...` patterns are intentional — let-else
// would force an early-return that obscures the intent.
#![allow(clippy::option_map_unit_fn)]
#![allow(clippy::similar_names)]
// § The sub-module pub-uses + matches! patterns produce single_match_else
// suggestions that obscure the intent of pattern-extraction.
#![allow(clippy::single_match_else)]
// § map_or(false, _) reads naturally for boolean-extraction patterns.
#![allow(clippy::unnecessary_map_or)]
// § Several short-circuit returns are kept for readability over
// boolean-and chaining.
#![allow(clippy::needless_continue)]
// § Layout solver returns `(Size, Vec<ChildSlot>)` ; clippy considers
// this a "type complexity" but it's the natural pair-shape.
#![allow(clippy::type_complexity)]
// § Theme constructors copy the colour table into the struct — clippy
// flags large-stack-array but Theme is a value-type by design.
#![allow(clippy::large_stack_arrays)]
// § Pass-by-value in iterators / map closures is preserved for readability.
#![allow(clippy::semicolon_if_nothing_returned)]
// § Several conversions roundtrip through f32 ; clippy's "as" lint is
// allowed at the workspace level but pedantic adds re-checks.
#![allow(clippy::cast_possible_truncation)]

pub mod accessibility;
pub mod context;
pub mod event;
pub mod geometry;
pub mod layout;
pub mod paint;
pub mod retained;
pub mod state;
pub mod theme;
pub mod widget;
pub mod widgets;

pub use accessibility::{
    AccessibilityNode, AccessibilityRole, AccessibilitySnapshot, AccessibilityState,
    AccessibilityValue,
};
pub use context::{centered_style, container_with, tight_gap_style, Ui};
pub use event::{EventResult, UiEvent};
pub use geometry::{Insets, Point, Rect, Size};
pub use layout::{
    solve_container, Axis, ChildLayoutInput, ChildSlot, Container, ContainerStyle, CrossAlign,
    LayoutConstraint, MainAlign,
};
pub use paint::{PaintCommand, PaintList, Painter};
pub use retained::{RetainedNode, RetainedTree};
pub use state::{FrameState, NavKey, RetainedState};
pub use theme::{Color, FontStyle, FontWeight, Spacing, Theme, ThemeSlot};
pub use widget::{EventContext, PaintContext, Widget, WidgetId};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("hurt nor harm"));
    }
}
