//! § TabPanel — tabs above a content stack.
//!
//! § ROLE
//!   `TabPanel` displays a row of tab buttons + a content area showing only
//!   the active tab's payload. The content widgets are owned by the tabs
//!   (each `Tab` carries a `Box<dyn Widget>`).

use crate::event::{EventResult, UiEvent};
use crate::geometry::{Point, Rect, Size};
use crate::layout::LayoutConstraint;
use crate::paint::Painter;
use crate::theme::ThemeSlot;
use crate::widget::{EventContext, PaintContext, Widget};

/// One tab — a label + a content widget.
pub struct Tab {
    pub label: String,
    pub content: Box<dyn Widget>,
}

impl Tab {
    /// Construct a tab.
    pub fn new(label: impl Into<String>, content: Box<dyn Widget>) -> Self {
        Self {
            label: label.into(),
            content,
        }
    }
}

impl std::fmt::Debug for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tab").field("label", &self.label).finish()
    }
}

/// Retained tab-panel widget.
pub struct TabPanel {
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub disabled: bool,
    pub tab_height: f32,
    just_changed: bool,
    size: Size,
}

impl TabPanel {
    /// Construct a tab panel.
    pub fn new(tabs: Vec<Tab>, active: usize) -> Self {
        let n = tabs.len();
        Self {
            tabs,
            active: active.min(n.saturating_sub(1)),
            disabled: false,
            tab_height: 26.0,
            just_changed: false,
            size: Size::ZERO,
        }
    }

    /// `true` if the active tab changed during the last event-pass.
    #[must_use]
    pub fn just_changed(&self) -> bool {
        self.just_changed
    }

    fn tab_widths(&self) -> Vec<f32> {
        let glyph_w = 8.0_f32;
        self.tabs
            .iter()
            .map(|t| t.label.chars().count() as f32 * glyph_w + 16.0)
            .collect()
    }

    fn tab_at_x(&self, x: f32) -> Option<usize> {
        let widths = self.tab_widths();
        let mut acc = 0.0_f32;
        for (i, w) in widths.iter().enumerate() {
            if x >= acc && x < acc + *w {
                return Some(i);
            }
            acc += *w;
        }
        None
    }
}

impl std::fmt::Debug for TabPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabPanel")
            .field("tabs", &self.tabs)
            .field("active", &self.active)
            .field("disabled", &self.disabled)
            .finish()
    }
}

impl Widget for TabPanel {
    fn type_tag(&self) -> &'static str {
        "TabPanel"
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let widths = self.tab_widths();
        let total_tab_w: f32 = widths.iter().copied().sum();
        let content_size = if let Some(active_tab) = self.tabs.get_mut(self.active) {
            active_tab.content.layout(constraint.shrink(crate::geometry::Insets::new(
                0.0,
                self.tab_height,
                0.0,
                0.0,
            )))
        } else {
            Size::ZERO
        };
        let w = total_tab_w.max(content_size.w);
        let h = self.tab_height + content_size.h;
        constraint.clamp(Size::new(w, h))
    }

    fn assign_final_size(&mut self, final_size: Size) {
        self.size = final_size;
        if let Some(active_tab) = self.tabs.get_mut(self.active) {
            active_tab
                .content
                .assign_final_size(Size::new(final_size.w, (final_size.h - self.tab_height).max(0.0)));
        }
    }

    fn event(&mut self, event: &UiEvent, ctx: EventContext<'_>) -> EventResult {
        self.just_changed = false;
        if self.disabled {
            return EventResult::Ignored;
        }
        match event {
            UiEvent::PointerDown { button, position, .. } => {
                if !matches!(button, cssl_host_window::event::MouseButton::Left)
                    || !ctx.hovered
                {
                    // Forward to active content even outside the tab strip.
                    if let Some(active) = self.tabs.get_mut(self.active) {
                        return active.content.event(event, ctx);
                    }
                    return EventResult::Ignored;
                }
                if position.y < self.tab_height {
                    if let Some(i) = self.tab_at_x(position.x) {
                        if i != self.active {
                            self.active = i;
                            self.just_changed = true;
                            return EventResult::Changed;
                        }
                        return EventResult::Consumed;
                    }
                }
                if let Some(active) = self.tabs.get_mut(self.active) {
                    return active.content.event(event, ctx);
                }
            }
            UiEvent::KeyDown { key, modifiers, .. } => {
                if ctx.focused && modifiers.contains(cssl_host_window::event::ModifierKeys::CTRL) {
                    let n = self.tabs.len();
                    if n == 0 {
                        return EventResult::Ignored;
                    }
                    if *key == cssl_host_window::event::KeyCode::Tab {
                        // Ctrl-Tab : next tab.
                        self.active = (self.active + 1) % n;
                        self.just_changed = true;
                        return EventResult::Changed;
                    }
                }
                if let Some(active) = self.tabs.get_mut(self.active) {
                    return active.content.event(event, ctx);
                }
            }
            _ => {
                if let Some(active) = self.tabs.get_mut(self.active) {
                    return active.content.event(event, ctx);
                }
            }
        }
        EventResult::Ignored
    }

    fn paint(&self, size: Size, painter: &mut dyn Painter, ctx: PaintContext<'_>) {
        let theme = ctx.theme;
        // Tab strip background.
        let strip = Rect::new(Point::ORIGIN, Size::new(size.w, self.tab_height));
        painter.fill_rect(strip, theme.color(ThemeSlot::ButtonFace), 0.0);
        let widths = self.tab_widths();
        let mut x = 0.0_f32;
        for (i, tab) in self.tabs.iter().enumerate() {
            let w = widths[i];
            let rect = Rect::new(Point::new(x, 0.0), Size::new(w, self.tab_height));
            let face = if i == self.active {
                theme.color(ThemeSlot::Background)
            } else {
                theme.color(ThemeSlot::ButtonFace)
            };
            painter.fill_rect(rect, face, 0.0);
            painter.stroke_rect(rect, theme.color(ThemeSlot::Border), 1.0, 0.0);
            painter.text(
                Point::new(
                    x + 8.0,
                    self.tab_height * 0.5 + theme.font.size_px * 0.35,
                ),
                &tab.label,
                &theme.font,
                theme.color(ThemeSlot::Foreground),
            );
            x += w;
        }
        // Content area.
        let content_rect = Rect::new(
            Point::new(0.0, self.tab_height),
            Size::new(size.w, (size.h - self.tab_height).max(0.0)),
        );
        painter.fill_rect(content_rect, theme.color(ThemeSlot::Background), 0.0);
        if let Some(active) = self.tabs.get(self.active) {
            painter.push_clip(content_rect);
            active.content.paint(content_rect.size, painter, ctx);
            painter.pop_clip();
        }
        if ctx.focused {
            painter.stroke_rect(
                Rect::new(Point::ORIGIN, size),
                theme.color(ThemeSlot::Accent),
                theme.focus_ring_width,
                0.0,
            );
        }
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::widgets::Label;
    use cssl_host_window::event::{KeyCode, ModifierKeys, MouseButton};

    fn sample_tabs() -> Vec<Tab> {
        vec![
            Tab::new("One", Box::new(Label::new("content1"))),
            Tab::new("Two", Box::new(Label::new("content2"))),
        ]
    }

    #[test]
    fn tab_panel_clamps_active() {
        let panel = TabPanel::new(sample_tabs(), 99);
        assert_eq!(panel.active, 1);
    }

    #[test]
    fn tab_panel_click_strip_changes_active() {
        let theme = Theme::default();
        let mut panel = TabPanel::new(sample_tabs(), 0);
        // Tab 1 starts at x = width of first tab. Use widths to find x.
        let widths = panel.tab_widths();
        let x_in_tab1 = widths[0] + 4.0;
        let r = panel.event(
            &UiEvent::PointerDown {
                position: Point::new(x_in_tab1, 5.0),
                button: MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert_eq!(r, EventResult::Changed);
        assert_eq!(panel.active, 1);
    }

    #[test]
    fn tab_panel_ctrl_tab_cycles() {
        let theme = Theme::default();
        let mut panel = TabPanel::new(sample_tabs(), 0);
        let r = panel.event(
            &UiEvent::KeyDown {
                key: KeyCode::Tab,
                modifiers: ModifierKeys::CTRL,
                repeat: false,
            },
            EventContext { theme: &theme, hovered: false, focused: true },
        );
        assert_eq!(r, EventResult::Changed);
        assert_eq!(panel.active, 1);
    }

    #[test]
    fn tab_panel_disabled_ignores() {
        let theme = Theme::default();
        let mut panel = TabPanel::new(sample_tabs(), 0);
        panel.disabled = true;
        let r = panel.event(
            &UiEvent::PointerDown {
                position: Point::ORIGIN,
                button: MouseButton::Left,
                modifiers: ModifierKeys::empty(),
                pointer_id: 0,
            },
            EventContext { theme: &theme, hovered: true, focused: false },
        );
        assert_eq!(r, EventResult::Ignored);
    }
}
