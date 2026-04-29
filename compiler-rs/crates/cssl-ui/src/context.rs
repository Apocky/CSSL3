//! § Ui — the immediate-mode driver.
//!
//! § ROLE
//!   `Ui` is the per-frame context the application uses to build the UI in
//!   the immediate-mode style :
//!     ```ignore
//!     let mut ui = Ui::new(theme);
//!     ui.begin_frame(window_size);
//!     for ev in events { ui.feed_event(ev); }
//!     if ui.button("Save") { /* save */ }
//!     ui.slider("volume", &mut volume, 0.0, 1.0);
//!     ui.end_frame(&mut painter);
//!     ```
//!   The `Ui` owns the per-frame state, the retained-state store, the
//!   queued event log, the active theme, and the focus + hover trackers.
//!
//! § BUILD CONTRACT
//!   Each frame is bracketed by `begin_frame` + `end_frame`. Widgets are
//!   added between them. The order of widget construction defines the
//!   sibling-index used by `WidgetId::hash_of` ; this is what makes the IDs
//!   stable across frames as long as the build code is structurally
//!   identical (the immediate-mode property).
//!
//! § FOCUS / HOVER (landmines)
//!   - Hover : every widget hit-tests `cursor` against its assigned frame
//!     during paint AND during event-processing. The deepest widget wins
//!     (last registered overwrites earlier).
//!   - Focus : `Tab` and `Shift+Tab` cycle through `focus_order`. Widgets
//!     register themselves as focusable on entry to their event-handler.
//!     `Enter` activates the focused widget (where applicable).
//!
//! § PRIME-DIRECTIVE attestation
//!   Input flows through `feed_event`. The `Ui` does not poll the OS
//!   directly. It does not log keystrokes. `pending_events` is observable.
//!   `cursor_position` and `current_focus` are observable. There is no
//!   covert state.

use cssl_host_window::event::{KeyCode, ModifierKeys, MouseButton};

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::{
    solve_container, ChildLayoutInput, Container, ContainerStyle, CrossAlign, MainAlign,
};
use crate::paint::Painter;
use crate::state::{FrameState, NavKey, RetainedState};
use crate::theme::Theme;
use crate::widget::WidgetId;

/// One queued widget pass — the immediate-mode driver records each call as
/// a frame entry then dispatches event + paint phases at end of frame.
///
/// Public for integration-test introspection ; widget code should not
/// depend on this surface directly.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FrameEntry {
    pub id: WidgetId,
    pub parent: WidgetId,
    pub frame: Rect,
    pub kind: FrameEntryKind,
}

/// What kind of widget produced this frame entry.
///
/// Public for integration-test introspection. Internally the immediate-
/// mode driver constructs these ; widget code should use the typed
/// `widgets::*` retained-mode shapes for new widgets.
#[derive(Debug, Clone)]
#[allow(dead_code)]
#[non_exhaustive]
pub enum FrameEntryKind {
    Button {
        label: String,
        pressed: bool,
        hovered: bool,
    },
    Label {
        text: String,
    },
    Checkbox {
        label: String,
        value: bool,
        hovered: bool,
    },
    Slider {
        label: String,
        value: f32,
        min: f32,
        max: f32,
        hovered: bool,
    },
    TextInput {
        label: String,
        text: String,
        hovered: bool,
    },
    ProgressBar {
        value: f32,
        max: f32,
    },
    Separator,
    ContainerStart {
        container: Container,
        style: ContainerStyle,
    },
    ContainerEnd,
}

/// Immediate-mode driver.
///
/// Lifetime is one frame ; you can either re-use the same `Ui` across
/// frames (recommended — preserves retained state) or construct a new one
/// each frame and rely on the application to carry the retained store.
#[derive(Debug)]
pub struct Ui {
    pub(crate) theme: Theme,
    /// Per-frame state.
    ///
    /// Public to integration tests so callers can verify focus / hovered /
    /// retained state across frames. Application code typically reads via
    /// the `state()` accessor.
    pub state: FrameState,
    /// Stack of in-progress containers ; each entry carries the parent id +
    /// the sibling-counter for inserting children.
    pub(crate) container_stack: Vec<ContainerStackEntry>,
    /// Recorded entries for this frame, in build order.
    ///
    /// Public to integration tests so callers can introspect resolved
    /// frames + widget IDs. Application code uses `Ui::is_hovered`,
    /// `Ui::is_focused`, etc. for the common questions.
    pub entries: Vec<FrameEntry>,
    /// Pending events fed in via `feed_event` ; drained during dispatch.
    pub(crate) events: Vec<UiEvent>,
    /// `true` if `begin_frame` has been called and `end_frame` has not.
    pub(crate) frame_open: bool,
}

/// Per-container bookkeeping while the immediate-mode build is in progress.
#[derive(Debug)]
pub(crate) struct ContainerStackEntry {
    pub(crate) id: WidgetId,
    /// Which child counter to use when registering each kind of widget —
    /// keyed by type-tag so two `Button("a")` widgets among `Slider` siblings
    /// still hash uniquely.
    pub(crate) child_counters: std::collections::HashMap<&'static str, u32>,
}

impl ContainerStackEntry {
    fn next_index(&mut self, type_tag: &'static str) -> u32 {
        let counter = self.child_counters.entry(type_tag).or_insert(0);
        let i = *counter;
        *counter += 1;
        i
    }
}

impl Ui {
    /// Construct a new immediate-mode context with the supplied theme.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            state: FrameState::new(),
            container_stack: Vec::new(),
            entries: Vec::new(),
            events: Vec::new(),
            frame_open: false,
        }
    }

    /// Read-only access to the active theme.
    #[must_use]
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Replace the theme. Takes effect on the next paint pass.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Read-only access to the frame state.
    #[must_use]
    pub fn state(&self) -> &FrameState {
        &self.state
    }

    /// The cursor position in window-local coordinates.
    #[must_use]
    pub fn cursor_position(&self) -> Point {
        self.state.cursor
    }

    /// The currently focused widget, if any.
    #[must_use]
    pub fn current_focus(&self) -> Option<WidgetId> {
        self.state.focused
    }

    /// `true` if `id` is currently hovered (cursor inside its bounds).
    #[must_use]
    pub fn is_hovered(&self, id: WidgetId) -> bool {
        self.state.hovered == Some(id)
    }

    /// `true` if `id` currently owns keyboard focus.
    #[must_use]
    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.state.focused == Some(id)
    }

    /// Read the queue of events pending dispatch (observable).
    #[must_use]
    pub fn pending_events(&self) -> &[UiEvent] {
        &self.events
    }

    /// Begin a new frame. Application calls this once per UI build.
    pub fn begin_frame(&mut self, window_size: Size) {
        self.state.begin_frame();
        self.state.window_size = (window_size.w, window_size.h);
        self.container_stack.clear();
        self.entries.clear();
        // Push a synthetic root container.
        self.container_stack.push(ContainerStackEntry {
            id: WidgetId::ROOT,
            child_counters: std::collections::HashMap::new(),
        });
        self.frame_open = true;
    }

    /// Feed one input event into the Ui. Stores into the queue ; dispatch
    /// happens on `end_frame`.
    pub fn feed_event(&mut self, event: UiEvent) {
        // Pre-process certain events that affect frame-state directly :
        // the cursor position is updated even before dispatch so widgets
        // that hit-test during their build can see the latest cursor.
        match &event {
            UiEvent::PointerMove { position, .. }
            | UiEvent::PointerDown { position, .. }
            | UiEvent::PointerUp { position, .. } => {
                self.state.cursor = *position;
            }
            UiEvent::WindowResize { width, height } => {
                self.state.window_size = (*width, *height);
            }
            _ => {}
        }
        match &event {
            UiEvent::PointerDown { button: MouseButton::Left, .. } => {
                self.state.primary_down = true;
            }
            UiEvent::PointerUp { button: MouseButton::Left, .. } => {
                self.state.primary_down = false;
                // Release the active widget when primary releases anywhere.
                self.state.clear_active();
            }
            _ => {}
        }
        self.events.push(event);
    }

    /// End the current frame ; dispatch events to widgets and submit paint.
    /// Returns the number of widgets that reported a state change this
    /// frame (informational ; the application can use this to schedule a
    /// redraw).
    pub fn end_frame(&mut self, painter: &mut dyn Painter) -> usize {
        // Resolve all frame entries' container layout (already filled in by
        // builder methods) — no further pass needed because each widget
        // built records its own resolved frame at build time.
        // (The two-pass solver runs internally inside `container_*` calls.)

        let changed_count = self.dispatch_events();
        self.paint(painter);
        self.events.clear();
        self.frame_open = false;
        changed_count
    }

    /// Dispatch queued events to widget entries based on hit-test + focus.
    /// Returns the count of entries that registered a state change.
    fn dispatch_events(&mut self) -> usize {
        let mut changed = 0_usize;
        // Process events in the order they were fed.
        let events = std::mem::take(&mut self.events);
        for ev in &events {
            match ev {
                UiEvent::PointerMove { position, .. } => {
                    // Re-resolve hover : iterate entries deepest-last (we
                    // record in build order ; later entries are children
                    // of earlier containers in immediate-mode flow).
                    self.state.hovered = self.hit_test_entries(*position);
                }
                UiEvent::PointerDown { position, button: MouseButton::Left, .. } => {
                    if let Some(id) = self.hit_test_entries(*position) {
                        self.state.set_active(id);
                        // Focus moves to the activated widget if it's
                        // focusable.
                        let focusable = self
                            .entries
                            .iter()
                            .any(|e| e.id == id && entry_is_focusable(&e.kind));
                        if focusable {
                            self.state.set_focus(Some(id));
                        }
                    }
                }
                UiEvent::PointerUp { position, button: MouseButton::Left, .. } => {
                    if let Some(active) = self.state.active {
                        if let Some(hit) = self.hit_test_entries(*position) {
                            if hit == active && mark_pressed(&mut self.entries, active) {
                                // Persist the press into retained state
                                // so the next frame's button() / checkbox()
                                // call observes it.
                                self.state.set(active, RetainedState::Bool(true));
                                changed += 1;
                            }
                        }
                    }
                    self.state.clear_active();
                }
                UiEvent::KeyDown { key, modifiers, repeat: _ } => {
                    if let Some(nav) = key_to_nav(*key, *modifiers) {
                        match nav {
                            NavKey::Tab => {
                                self.state.focus_next();
                            }
                            NavKey::ShiftTab => {
                                self.state.focus_prev();
                            }
                            NavKey::Enter => {
                                if let Some(id) = self.state.focused {
                                    if mark_pressed(&mut self.entries, id) {
                                        self.state.set(id, RetainedState::Bool(true));
                                        changed += 1;
                                    }
                                }
                            }
                            NavKey::ArrowLeft | NavKey::ArrowDown => {
                                if let Some(id) = self.state.focused {
                                    if let Some(new_val) =
                                        nudge_value(&mut self.entries, id, -1.0)
                                    {
                                        // Persist new slider value into retained store.
                                        let max = read_entry_slider_max(&self.entries, id)
                                            .unwrap_or(1.0);
                                        self.state.set(id, RetainedState::Range(new_val, max));
                                        changed += 1;
                                    }
                                }
                            }
                            NavKey::ArrowRight | NavKey::ArrowUp => {
                                if let Some(id) = self.state.focused {
                                    if let Some(new_val) =
                                        nudge_value(&mut self.entries, id, 1.0)
                                    {
                                        let max = read_entry_slider_max(&self.entries, id)
                                            .unwrap_or(1.0);
                                        self.state.set(id, RetainedState::Range(new_val, max));
                                        changed += 1;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                UiEvent::Char { ch, .. } => {
                    if let Some(id) = self.state.focused {
                        if append_text(&mut self.entries, id, *ch) {
                            // Persist into retained store.
                            if let Some(text) = read_entry_text(&self.entries, id) {
                                self.state
                                    .set(id, RetainedState::Text(text.to_string()));
                            }
                            changed += 1;
                        }
                    }
                }
                UiEvent::WindowUnfocus => {
                    self.state.hovered = None;
                }
                _ => {}
            }
        }
        changed
    }

    /// Hit-test : returns the id of the deepest entry whose frame contains
    /// `point`. Entries are recorded in build order ; later entries hit
    /// later, so iterating from the back finds the topmost.
    fn hit_test_entries(&self, point: Point) -> Option<WidgetId> {
        for entry in self.entries.iter().rev() {
            if entry.frame.contains(point) && entry_is_pickable(&entry.kind) {
                return Some(entry.id);
            }
        }
        None
    }

    /// Walk the recorded frame entries and submit paint commands.
    fn paint(&mut self, painter: &mut dyn Painter) {
        // Update retained-state text fields back to entries before painting,
        // so visual matches retained store.
        let entries = self.entries.clone();
        for entry in &entries {
            self.paint_entry(entry, painter);
        }
    }

    fn paint_entry(&self, entry: &FrameEntry, painter: &mut dyn Painter) {
        let theme = &self.theme;
        let hovered = self.state.hovered == Some(entry.id);
        let focused = self.state.focused == Some(entry.id);
        let active = self.state.active == Some(entry.id);
        match &entry.kind {
            FrameEntryKind::Button { label, hovered: _, pressed: _ } => {
                let face = if active {
                    theme.color(crate::theme::ThemeSlot::ButtonActive)
                } else if hovered {
                    theme.color(crate::theme::ThemeSlot::ButtonHover)
                } else {
                    theme.color(crate::theme::ThemeSlot::ButtonFace)
                };
                painter.fill_rect(entry.frame, face, theme.corner_radius);
                painter.stroke_rect(
                    entry.frame,
                    theme.color(crate::theme::ThemeSlot::Border),
                    1.0,
                    theme.corner_radius,
                );
                if focused {
                    painter.stroke_rect(
                        entry.frame,
                        theme.color(crate::theme::ThemeSlot::Accent),
                        theme.focus_ring_width,
                        theme.corner_radius,
                    );
                }
                let text_pos = Point::new(
                    entry.frame.origin.x + theme.spacing.normal,
                    entry.frame.origin.y + entry.frame.size.h * 0.5 + theme.font.size_px * 0.35,
                );
                painter.text(
                    text_pos,
                    label,
                    &theme.font,
                    theme.color(crate::theme::ThemeSlot::Foreground),
                );
            }
            FrameEntryKind::Label { text } => {
                let text_pos = Point::new(
                    entry.frame.origin.x,
                    entry.frame.origin.y + entry.frame.size.h * 0.5 + theme.font.size_px * 0.35,
                );
                painter.text(
                    text_pos,
                    text,
                    &theme.font,
                    theme.color(crate::theme::ThemeSlot::Foreground),
                );
            }
            FrameEntryKind::Checkbox { label, value, .. } => {
                let box_rect = Rect::new(
                    Point::new(entry.frame.origin.x, entry.frame.origin.y),
                    Size::new(entry.frame.size.h, entry.frame.size.h),
                );
                let bg = if *value {
                    theme.color(crate::theme::ThemeSlot::Accent)
                } else {
                    theme.color(crate::theme::ThemeSlot::ButtonFace)
                };
                painter.fill_rect(box_rect, bg, theme.corner_radius);
                painter.stroke_rect(
                    box_rect,
                    theme.color(crate::theme::ThemeSlot::Border),
                    1.0,
                    theme.corner_radius,
                );
                if focused {
                    painter.stroke_rect(
                        box_rect,
                        theme.color(crate::theme::ThemeSlot::Accent),
                        theme.focus_ring_width,
                        theme.corner_radius,
                    );
                }
                let text_pos = Point::new(
                    entry.frame.origin.x + entry.frame.size.h + theme.spacing.normal,
                    entry.frame.origin.y + entry.frame.size.h * 0.5 + theme.font.size_px * 0.35,
                );
                painter.text(
                    text_pos,
                    label,
                    &theme.font,
                    theme.color(crate::theme::ThemeSlot::Foreground),
                );
            }
            FrameEntryKind::Slider { label, value, min, max, .. } => {
                // Track.
                let track_rect = Rect::new(
                    Point::new(
                        entry.frame.origin.x,
                        entry.frame.origin.y + entry.frame.size.h * 0.5 - 2.0,
                    ),
                    Size::new(entry.frame.size.w, 4.0),
                );
                painter.fill_rect(
                    track_rect,
                    theme.color(crate::theme::ThemeSlot::AccentMuted),
                    2.0,
                );
                // Knob.
                let t = ((*value - *min) / (*max - *min)).clamp(0.0, 1.0);
                let knob_x = entry.frame.origin.x + t * entry.frame.size.w;
                let knob_y = entry.frame.origin.y + entry.frame.size.h * 0.5;
                painter.fill_circle(
                    Point::new(knob_x, knob_y),
                    entry.frame.size.h * 0.4,
                    theme.color(crate::theme::ThemeSlot::Accent),
                );
                if focused {
                    painter.stroke_rect(
                        entry.frame,
                        theme.color(crate::theme::ThemeSlot::Accent),
                        theme.focus_ring_width,
                        theme.corner_radius,
                    );
                }
                // Value caption (label + value).
                let caption = format!("{label}: {value:.2}");
                let text_pos = Point::new(
                    entry.frame.origin.x,
                    entry.frame.origin.y + entry.frame.size.h + theme.font.size_px,
                );
                painter.text(
                    text_pos,
                    &caption,
                    &theme.font,
                    theme.color(crate::theme::ThemeSlot::ForegroundMuted),
                );
            }
            FrameEntryKind::TextInput { label, text, .. } => {
                painter.fill_rect(
                    entry.frame,
                    theme.color(crate::theme::ThemeSlot::ButtonFace),
                    theme.corner_radius,
                );
                painter.stroke_rect(
                    entry.frame,
                    if focused {
                        theme.color(crate::theme::ThemeSlot::Accent)
                    } else {
                        theme.color(crate::theme::ThemeSlot::Border)
                    },
                    if focused { theme.focus_ring_width } else { 1.0 },
                    theme.corner_radius,
                );
                let text_pos = Point::new(
                    entry.frame.origin.x + theme.spacing.normal,
                    entry.frame.origin.y + entry.frame.size.h * 0.5 + theme.font.size_px * 0.35,
                );
                let display = if text.is_empty() && !focused {
                    label.as_str()
                } else {
                    text.as_str()
                };
                let color = if text.is_empty() && !focused {
                    theme.color(crate::theme::ThemeSlot::ForegroundMuted)
                } else {
                    theme.color(crate::theme::ThemeSlot::Foreground)
                };
                painter.text(text_pos, display, &theme.font, color);
                if focused {
                    let caret_x = entry.frame.origin.x
                        + theme.spacing.normal
                        + (text.chars().count() as f32) * theme.font.size_px * 0.5;
                    let caret_top = entry.frame.origin.y + theme.spacing.normal;
                    let caret_bot = entry.frame.origin.y + entry.frame.size.h - theme.spacing.normal;
                    painter.stroke_line(
                        Point::new(caret_x, caret_top),
                        Point::new(caret_x, caret_bot),
                        theme.color(crate::theme::ThemeSlot::Caret),
                        1.0,
                    );
                }
            }
            FrameEntryKind::ProgressBar { value, max } => {
                painter.fill_rect(
                    entry.frame,
                    theme.color(crate::theme::ThemeSlot::AccentMuted),
                    theme.corner_radius,
                );
                let t = (value / max).clamp(0.0, 1.0);
                let fill_rect = Rect::new(
                    entry.frame.origin,
                    Size::new(entry.frame.size.w * t, entry.frame.size.h),
                );
                painter.fill_rect(
                    fill_rect,
                    theme.color(crate::theme::ThemeSlot::Accent),
                    theme.corner_radius,
                );
            }
            FrameEntryKind::Separator => {
                painter.fill_rect(
                    entry.frame,
                    theme.color(crate::theme::ThemeSlot::Border),
                    0.0,
                );
            }
            FrameEntryKind::ContainerStart { .. } | FrameEntryKind::ContainerEnd => {
                // Containers are layout-only ; no paint of their own
                // (background is the responsibility of an explicit panel
                // widget — to be added).
            }
        }
    }

    // ──────────────────────── Container helpers ────────────────────────

    /// Begin a `Vbox` container. Subsequent widgets become its children
    /// until the matching [`Ui::container_end`].
    pub fn vbox(&mut self) -> WidgetId {
        self.container_begin(Container::Vbox, ContainerStyle::default())
    }

    /// Begin an `Hbox` container.
    pub fn hbox(&mut self) -> WidgetId {
        self.container_begin(Container::Hbox, ContainerStyle::default())
    }

    /// Begin a container with explicit style.
    pub fn container_begin(&mut self, container: Container, style: ContainerStyle) -> WidgetId {
        let parent = self.current_parent();
        let sib = self.next_sibling_index("Container");
        let id = WidgetId::hash_of("Container", "", parent, sib);
        // We don't yet know the child sizes ; record a placeholder rect that
        // gets resolved at `container_end`.
        self.entries.push(FrameEntry {
            id,
            parent,
            frame: Rect::EMPTY,
            kind: FrameEntryKind::ContainerStart { container, style },
        });
        self.container_stack.push(ContainerStackEntry {
            id,
            child_counters: std::collections::HashMap::new(),
        });
        id
    }

    /// End the current container — runs the layout solver to assign final
    /// frames to its children + the container itself.
    pub fn container_end(&mut self) {
        // Pop the active container ; resolve its layout.
        let popped = self
            .container_stack
            .pop()
            .expect("container_end called without matching begin");
        // Find the start entry index.
        let start_idx = self
            .entries
            .iter()
            .rposition(|e| matches!(&e.kind, FrameEntryKind::ContainerStart { .. }) && e.id == popped.id)
            .expect("container start entry missing");
        let (container, style) = match &self.entries[start_idx].kind {
            FrameEntryKind::ContainerStart { container, style } => (container.clone(), *style),
            _ => unreachable!(),
        };
        let child_inputs: Vec<ChildLayoutInput> = self.entries[start_idx + 1..]
            .iter()
            .filter(|e| !matches!(&e.kind, FrameEntryKind::ContainerEnd))
            .map(|e| ChildLayoutInput {
                min_size: entry_min_size(e, &self.theme),
                flex: 0.0,
                absolute_origin: Point::ORIGIN,
            })
            .collect();
        let constraint = crate::layout::LayoutConstraint::loose(Size::new(
            self.state.window_size.0,
            self.state.window_size.1,
        ));
        let (resolved, slots) = solve_container(&container, constraint, style, &child_inputs);
        // Position each child relative to container origin.
        // Container origin defaults to top-left of available space ; nested
        // containers honour their parent's slot.
        let container_origin = self.compute_container_origin(popped.id, resolved);
        // Update the container's own frame.
        self.entries[start_idx].frame = Rect::new(container_origin, resolved);
        // Apply slot frames to children — translate by container origin.
        let mut slot_iter = slots.iter();
        for entry in &mut self.entries[start_idx + 1..] {
            if matches!(&entry.kind, FrameEntryKind::ContainerEnd) {
                continue;
            }
            if let Some(slot) = slot_iter.next() {
                entry.frame =
                    Rect::new(container_origin.translate(slot.frame.origin), slot.frame.size);
            }
        }
        // Push a synthetic ContainerEnd marker.
        self.entries.push(FrameEntry {
            id: popped.id,
            parent: self.current_parent(),
            frame: Rect::EMPTY,
            kind: FrameEntryKind::ContainerEnd,
        });
    }

    fn compute_container_origin(&self, _id: WidgetId, _size: Size) -> Point {
        // Stage-0 : top-level container at window origin. Nested containers
        // inherit the parent's start-of-cursor.
        Point::ORIGIN
    }

    fn current_parent(&self) -> WidgetId {
        self.container_stack
            .last()
            .map_or(WidgetId::ROOT, |e| e.id)
    }

    fn next_sibling_index(&mut self, type_tag: &'static str) -> u32 {
        let entry = self.container_stack.last_mut().expect("container stack empty");
        entry.next_index(type_tag)
    }

    // ───────────────────────── Immediate widgets ──────────────────────

    /// Immediate-mode button. Returns `true` if clicked this frame.
    pub fn button(&mut self, label: impl Into<String>) -> bool {
        let label = label.into();
        let parent = self.current_parent();
        let sib = self.next_sibling_index("Button");
        let id = WidgetId::hash_of("Button", &label, parent, sib);
        // Pre-store retained state for "was-pressed" flag.
        let pressed = self
            .state
            .get(id)
            .and_then(|v| match v {
                RetainedState::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false);
        if pressed {
            // One-shot — clear immediately so next frame returns false.
            self.state.set(id, RetainedState::Bool(false));
        }
        let hovered = self.state.hovered == Some(id);
        let frame = Rect::new(
            Point::ORIGIN,
            self.measure_button(&label),
        );
        self.entries.push(FrameEntry {
            id,
            parent,
            frame,
            kind: FrameEntryKind::Button {
                label,
                pressed,
                hovered,
            },
        });
        // Register focusability.
        self.state.register_focusable(id);
        pressed
    }

    /// Immediate-mode label. No interaction ; returns nothing.
    pub fn label(&mut self, text: impl Into<String>) {
        let text = text.into();
        let parent = self.current_parent();
        let sib = self.next_sibling_index("Label");
        let id = WidgetId::hash_of("Label", &text, parent, sib);
        let frame = Rect::new(Point::ORIGIN, self.measure_label(&text));
        self.entries.push(FrameEntry {
            id,
            parent,
            frame,
            kind: FrameEntryKind::Label { text },
        });
    }

    /// Immediate-mode checkbox. `value` is updated in place when toggled.
    /// Returns `true` if the value changed this frame.
    pub fn checkbox(&mut self, label: impl Into<String>, value: &mut bool) -> bool {
        let label = label.into();
        let parent = self.current_parent();
        let sib = self.next_sibling_index("Checkbox");
        let id = WidgetId::hash_of("Checkbox", &label, parent, sib);
        // Sync retained store with caller value.
        let stored = self
            .state
            .get(id)
            .and_then(|v| v.as_bool())
            .unwrap_or(*value);
        // Detect commit-pending : pressed flag stored in a sibling slot.
        let pressed_id = WidgetId(id.0 ^ 0x1);
        let was_pressed = self
            .state
            .get(pressed_id)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut changed = false;
        let mut current = stored;
        if was_pressed {
            current = !stored;
            self.state.set(id, RetainedState::Bool(current));
            self.state.set(pressed_id, RetainedState::Bool(false));
            *value = current;
            changed = true;
        } else if *value != stored {
            // Caller forced an external state change.
            self.state.set(id, RetainedState::Bool(*value));
            current = *value;
        }
        let hovered = self.state.hovered == Some(id);
        let frame = Rect::new(Point::ORIGIN, self.measure_checkbox(&label));
        self.entries.push(FrameEntry {
            id,
            parent,
            frame,
            kind: FrameEntryKind::Checkbox {
                label,
                value: current,
                hovered,
            },
        });
        self.state.register_focusable(id);
        changed
    }

    /// Immediate-mode slider. `value` is updated in place when retained
    /// state changed since last frame (e.g. arrow-key nudge or drag).
    /// Returns `true` if the caller's value was updated.
    pub fn slider(
        &mut self,
        label: impl Into<String>,
        value: &mut f32,
        min: f32,
        max: f32,
    ) -> bool {
        let label = label.into();
        let parent = self.current_parent();
        let sib = self.next_sibling_index("Slider");
        let id = WidgetId::hash_of("Slider", &label, parent, sib);
        let stored = self
            .state
            .get(id)
            .and_then(|v| v.as_range())
            .map(|(v, _)| v);
        let mut changed = false;
        let current = match stored {
            Some(s) if (s - *value).abs() > f32::EPSILON => {
                // Retained-store value differs from caller — the value
                // changed via arrow-key or drag during the previous frame's
                // event-pass. Propagate to caller.
                *value = s.clamp(min, max);
                changed = true;
                *value
            }
            Some(s) => s,
            None => {
                // First time we see this slider — seed retained from caller.
                self.state.set(id, RetainedState::Range(*value, max));
                *value
            }
        };
        let hovered = self.state.hovered == Some(id);
        let frame = Rect::new(Point::ORIGIN, self.measure_slider());
        self.entries.push(FrameEntry {
            id,
            parent,
            frame,
            kind: FrameEntryKind::Slider {
                label,
                value: current,
                min,
                max,
                hovered,
            },
        });
        self.state.register_focusable(id);
        changed
    }

    /// Immediate-mode text input. The buffer is updated in place when
    /// retained-store text differs from the caller's buffer (i.e. the
    /// user typed last frame). Returns `true` when the caller's buffer
    /// was updated.
    pub fn text_input(&mut self, label: impl Into<String>, buffer: &mut String) -> bool {
        let label = label.into();
        let parent = self.current_parent();
        let sib = self.next_sibling_index("TextInput");
        let id = WidgetId::hash_of("TextInput", &label, parent, sib);
        let stored = self
            .state
            .get(id)
            .and_then(|v| v.as_text().map(str::to_string));
        let mut changed = false;
        let current = match stored {
            Some(s) if &s != buffer => {
                // Retained buffer differs — user typed during the previous
                // frame's event-pass. Propagate to caller.
                *buffer = s.clone();
                changed = true;
                s
            }
            Some(s) => s,
            None => {
                self.state
                    .set(id, RetainedState::Text(buffer.clone()));
                buffer.clone()
            }
        };
        let hovered = self.state.hovered == Some(id);
        let frame = Rect::new(Point::ORIGIN, self.measure_text_input());
        self.entries.push(FrameEntry {
            id,
            parent,
            frame,
            kind: FrameEntryKind::TextInput {
                label,
                text: current,
                hovered,
            },
        });
        self.state.register_focusable(id);
        changed
    }

    /// Immediate-mode progress bar (read-only, non-interactive).
    pub fn progress(&mut self, value: f32, max: f32) {
        let parent = self.current_parent();
        let sib = self.next_sibling_index("Progress");
        let id = WidgetId::hash_of("Progress", "", parent, sib);
        let frame = Rect::new(Point::ORIGIN, Size::new(120.0, 8.0));
        self.entries.push(FrameEntry {
            id,
            parent,
            frame,
            kind: FrameEntryKind::ProgressBar { value, max },
        });
    }

    /// Immediate-mode separator (a thin horizontal line).
    pub fn separator(&mut self) {
        let parent = self.current_parent();
        let sib = self.next_sibling_index("Separator");
        let id = WidgetId::hash_of("Separator", "", parent, sib);
        let frame = Rect::new(Point::ORIGIN, Size::new(120.0, 1.0));
        self.entries.push(FrameEntry {
            id,
            parent,
            frame,
            kind: FrameEntryKind::Separator,
        });
    }

    // ─────────────────── Measurement helpers ───────────────────────────

    fn measure_button(&self, label: &str) -> Size {
        let pad = self.theme.spacing.normal * 2.0;
        let glyph_w = self.theme.font.size_px * 0.55;
        Size::new(label.chars().count() as f32 * glyph_w + pad, self.theme.font.size_px + pad)
    }

    fn measure_label(&self, text: &str) -> Size {
        let glyph_w = self.theme.font.size_px * 0.55;
        Size::new(text.chars().count() as f32 * glyph_w, self.theme.font.size_px + 4.0)
    }

    fn measure_checkbox(&self, label: &str) -> Size {
        let h = self.theme.font.size_px + self.theme.spacing.tight * 2.0;
        let glyph_w = self.theme.font.size_px * 0.55;
        Size::new(
            h + self.theme.spacing.normal + label.chars().count() as f32 * glyph_w,
            h,
        )
    }

    fn measure_slider(&self) -> Size {
        Size::new(160.0, self.theme.font.size_px + 4.0)
    }

    fn measure_text_input(&self) -> Size {
        Size::new(160.0, self.theme.font.size_px + self.theme.spacing.normal * 2.0)
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new(Theme::default())
    }
}

// ──────────────────────────── helpers ─────────────────────────────────

fn entry_is_focusable(kind: &FrameEntryKind) -> bool {
    matches!(
        kind,
        FrameEntryKind::Button { .. }
            | FrameEntryKind::Checkbox { .. }
            | FrameEntryKind::Slider { .. }
            | FrameEntryKind::TextInput { .. }
    )
}

fn entry_is_pickable(kind: &FrameEntryKind) -> bool {
    !matches!(
        kind,
        FrameEntryKind::Label { .. }
            | FrameEntryKind::Separator
            | FrameEntryKind::ProgressBar { .. }
            | FrameEntryKind::ContainerStart { .. }
            | FrameEntryKind::ContainerEnd
    )
}

fn key_to_nav(key: KeyCode, modifiers: ModifierKeys) -> Option<NavKey> {
    let shift = modifiers.contains(ModifierKeys::SHIFT);
    Some(match key {
        KeyCode::Tab => {
            if shift {
                NavKey::ShiftTab
            } else {
                NavKey::Tab
            }
        }
        KeyCode::Enter => NavKey::Enter,
        KeyCode::Escape => NavKey::Escape,
        KeyCode::Up => NavKey::ArrowUp,
        KeyCode::Down => NavKey::ArrowDown,
        KeyCode::Left => NavKey::ArrowLeft,
        KeyCode::Right => NavKey::ArrowRight,
        KeyCode::Home => NavKey::Home,
        KeyCode::End => NavKey::End,
        _ => return None,
    })
}

fn mark_pressed(entries: &mut [FrameEntry], id: WidgetId) -> bool {
    for entry in entries {
        if entry.id == id {
            match &mut entry.kind {
                FrameEntryKind::Button { pressed, .. } => {
                    *pressed = true;
                    return true;
                }
                FrameEntryKind::Checkbox { value, .. } => {
                    *value = !*value;
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

fn nudge_value(entries: &mut [FrameEntry], id: WidgetId, delta: f32) -> Option<f32> {
    for entry in entries {
        if entry.id == id {
            if let FrameEntryKind::Slider { value, min, max, .. } = &mut entry.kind {
                let step = (*max - *min) * 0.05;
                *value = (*value + delta * step).clamp(*min, *max);
                return Some(*value);
            }
        }
    }
    None
}

fn read_entry_slider_max(entries: &[FrameEntry], id: WidgetId) -> Option<f32> {
    for entry in entries {
        if entry.id == id {
            if let FrameEntryKind::Slider { max, .. } = &entry.kind {
                return Some(*max);
            }
        }
    }
    None
}

fn append_text(entries: &mut [FrameEntry], id: WidgetId, ch: char) -> bool {
    for entry in entries {
        if entry.id == id {
            if let FrameEntryKind::TextInput { text, .. } = &mut entry.kind {
                if ch == '\u{0008}' || ch == '\u{007f}' {
                    text.pop();
                } else if !ch.is_control() {
                    text.push(ch);
                } else {
                    return false;
                }
                return true;
            }
        }
    }
    false
}

fn read_entry_text<'a>(entries: &'a [FrameEntry], id: WidgetId) -> Option<&'a str> {
    for entry in entries {
        if entry.id == id {
            if let FrameEntryKind::TextInput { text, .. } = &entry.kind {
                return Some(text.as_str());
            }
        }
    }
    None
}

fn entry_min_size(entry: &FrameEntry, _theme: &Theme) -> Size {
    entry.frame.size
}

/// Fluent helper to build a container with a specific style and run a
/// closure between begin/end. Simplifies the common pattern :
///
/// ```ignore
/// container_with(&mut ui, Container::Vbox, style, |ui| {
///     ui.label("hi");
/// });
/// ```
pub fn container_with<F>(ui: &mut Ui, container: Container, style: ContainerStyle, body: F)
where
    F: FnOnce(&mut Ui),
{
    ui.container_begin(container, style);
    body(ui);
    ui.container_end();
}

/// Convenience : a ContainerStyle pre-configured for tight gap between
/// children + zero padding.
#[must_use]
pub fn tight_gap_style(gap: f32) -> ContainerStyle {
    ContainerStyle {
        gap,
        cross_align: CrossAlign::Start,
        main_align: MainAlign::Start,
        padding: crate::geometry::Insets::ZERO,
    }
}

/// Convenience : a ContainerStyle pre-configured for centered content.
#[must_use]
pub fn centered_style() -> ContainerStyle {
    ContainerStyle {
        gap: 4.0,
        cross_align: CrossAlign::Center,
        main_align: MainAlign::Center,
        padding: crate::geometry::Insets::uniform(8.0),
    }
}

// (axis-related re-exports kept in `crate::layout` ; no in-context use here)

impl EventResult {
    /// Convenience adapter — a no-op for impl-side compatibility hook.
    /// Layout phase outputs use this to thread results without import.
    pub fn from_changed(changed: bool) -> Self {
        if changed {
            Self::Changed
        } else {
            Self::Ignored
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::PaintList;

    fn fresh_ui() -> Ui {
        let mut ui = Ui::new(Theme::default());
        ui.begin_frame(Size::new(800.0, 600.0));
        ui
    }

    #[test]
    fn ui_new_default_theme() {
        let ui = Ui::default();
        assert_eq!(ui.theme().container_padding, Theme::dark().container_padding);
    }

    #[test]
    fn ui_set_theme_replaces() {
        let mut ui = Ui::new(Theme::dark());
        ui.set_theme(Theme::light());
        assert_eq!(
            ui.theme().color(crate::theme::ThemeSlot::Background),
            Theme::light().color(crate::theme::ThemeSlot::Background)
        );
    }

    #[test]
    fn ui_button_records_entry() {
        let mut ui = fresh_ui();
        let _ = ui.button("Save");
        // 1 root container + 1 button = 2 entries (root container has no
        // explicit start ; only begin_frame's synthetic root counts).
        // Actually we don't push a synthetic root entry — only an
        // implicit top-level frame state. So 1 entry after button.
        assert!(ui.entries.iter().any(
            |e| matches!(&e.kind, FrameEntryKind::Button { label, .. } if label == "Save")
        ));
    }

    #[test]
    fn ui_button_returns_false_first_frame() {
        let mut ui = fresh_ui();
        assert!(!ui.button("Save"));
    }

    #[test]
    fn ui_label_records_entry() {
        let mut ui = fresh_ui();
        ui.label("Status: OK");
        assert!(ui.entries.iter().any(
            |e| matches!(&e.kind, FrameEntryKind::Label { text } if text == "Status: OK")
        ));
    }

    #[test]
    fn ui_checkbox_initial_false() {
        let mut ui = fresh_ui();
        let mut value = false;
        let changed = ui.checkbox("Mute", &mut value);
        assert!(!changed);
        assert!(!value);
    }

    #[test]
    fn ui_slider_initial_no_change() {
        let mut ui = fresh_ui();
        let mut v = 0.5;
        let changed = ui.slider("vol", &mut v, 0.0, 1.0);
        assert!(!changed);
        assert!((v - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn ui_text_input_preserves_buffer() {
        let mut ui = fresh_ui();
        let mut buf = String::from("hi");
        let _ = ui.text_input("name", &mut buf);
        assert_eq!(buf, "hi");
    }

    #[test]
    fn ui_progress_records_entry() {
        let mut ui = fresh_ui();
        ui.progress(0.5, 1.0);
        assert!(matches!(
            ui.entries.last().unwrap().kind,
            FrameEntryKind::ProgressBar { .. }
        ));
    }

    #[test]
    fn ui_separator_records_entry() {
        let mut ui = fresh_ui();
        ui.separator();
        assert!(matches!(
            ui.entries.last().unwrap().kind,
            FrameEntryKind::Separator
        ));
    }

    #[test]
    fn vbox_wraps_button_and_label() {
        let mut ui = fresh_ui();
        ui.vbox();
        ui.button("OK");
        ui.label("Hello");
        ui.container_end();
        let cs = ui.entries.iter().filter(|e| matches!(&e.kind, FrameEntryKind::ContainerStart { .. })).count();
        let ce = ui.entries.iter().filter(|e| matches!(&e.kind, FrameEntryKind::ContainerEnd)).count();
        assert_eq!(cs, 1);
        assert_eq!(ce, 1);
    }

    #[test]
    fn ui_feed_event_pointer_move_updates_cursor() {
        let mut ui = fresh_ui();
        ui.feed_event(UiEvent::PointerMove {
            position: Point::new(50.0, 60.0),
            modifiers: ModifierKeys::empty(),
            pointer_id: 0,
        });
        assert!((ui.cursor_position().x - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ui_end_frame_paints_to_painter() {
        let mut ui = fresh_ui();
        ui.label("hello");
        let mut p = PaintList::new();
        let _ = ui.end_frame(&mut p);
        assert!(!p.is_empty());
    }

    #[test]
    fn ui_button_click_returns_true_on_release_inside() {
        // First frame : button records frame.
        let mut ui = Ui::new(Theme::default());
        ui.begin_frame(Size::new(200.0, 200.0));
        let _ = ui.button("Save");
        // Find the button's frame.
        let btn_id = ui.entries[0].id;
        let btn_frame = ui.entries[0].frame;
        let center = btn_frame.center();
        // Move cursor over button.
        ui.feed_event(UiEvent::PointerMove {
            position: center,
            modifiers: ModifierKeys::empty(),
            pointer_id: 0,
        });
        // Press + release on button.
        ui.feed_event(UiEvent::PointerDown {
            position: center,
            button: MouseButton::Left,
            modifiers: ModifierKeys::empty(),
            pointer_id: 0,
        });
        ui.feed_event(UiEvent::PointerUp {
            position: center,
            button: MouseButton::Left,
            modifiers: ModifierKeys::empty(),
            pointer_id: 0,
        });
        let mut p = PaintList::new();
        let _ = ui.end_frame(&mut p);
        // Next frame : button should report pressed=true.
        ui.begin_frame(Size::new(200.0, 200.0));
        let clicked = ui.button("Save");
        let _ = ui.end_frame(&mut p);
        assert!(clicked, "button should report click on next frame ; first id {btn_id:?}");
    }

    #[test]
    fn ui_focusable_widgets_register_in_order() {
        let mut ui = fresh_ui();
        let _ = ui.button("A");
        let _ = ui.button("B");
        let mut p = PaintList::new();
        let _ = ui.end_frame(&mut p);
        // After end_frame, focus_order should retain both.
        assert_eq!(ui.state.focus_order.len(), 2);
    }

    #[test]
    fn ui_label_does_not_register_focusable() {
        let mut ui = fresh_ui();
        ui.label("read-only");
        let mut p = PaintList::new();
        let _ = ui.end_frame(&mut p);
        assert!(ui.state.focus_order.is_empty());
    }
}
