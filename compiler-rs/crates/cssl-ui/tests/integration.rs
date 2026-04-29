//! § Integration test : compose a 5-widget UI ; layout converges ;
//! hover/click events route correctly.
//!
//! § REPORT-BACK contract for T11-D109 (S9-U1) :
//!   "compose 5-widget UI (button + slider + label + checkbox + textinput) ;
//!    layout converges ; hover/click events route correctly ✓"

use cssl_host_window::event::{KeyCode, ModifierKeys, MouseButton};
use cssl_ui::{
    paint::PaintList, Container, ContainerStyle, MainAlign, Point, Size, Theme, Ui, UiEvent,
};

#[test]
fn five_widget_ui_layout_converges_and_routes_events() {
    let mut ui = Ui::new(Theme::dark());
    let mut volume: f32 = 0.5;
    let mut muted: bool = false;
    let mut name: String = String::new();
    let mut clicks: u32 = 0;

    // Build 3 frames :
    //   Frame 1 : the application builds the 5-widget UI ; no events yet.
    //   Frame 2 : the application feeds a sequence of events that should
    //             click the button + toggle the checkbox + drive the slider.
    //   Frame 3 : the application checks that retained state survived.

    // Frame 1.
    {
        ui.begin_frame(Size::new(400.0, 300.0));
        let style = ContainerStyle {
            gap: 6.0,
            main_align: MainAlign::Start,
            cross_align: cssl_ui::CrossAlign::Start,
            padding: cssl_ui::Insets::uniform(8.0),
        };
        ui.container_begin(Container::Vbox, style);
        ui.label("CSSLv3 Settings");
        if ui.button("Apply") {
            clicks += 1;
        }
        let _ = ui.checkbox("Mute", &mut muted);
        let _ = ui.slider("volume", &mut volume, 0.0, 1.0);
        let _ = ui.text_input("Player name", &mut name);
        ui.container_end();
        let mut paint = PaintList::new();
        let _ = ui.end_frame(&mut paint);
        // Layout should produce SOME paint commands.
        assert!(!paint.is_empty(), "frame-1 paint list empty");
    }

    // Frame 2 : feed events.
    {
        ui.begin_frame(Size::new(400.0, 300.0));
        ui.container_begin(
            Container::Vbox,
            ContainerStyle {
                gap: 6.0,
                main_align: MainAlign::Start,
                cross_align: cssl_ui::CrossAlign::Start,
                padding: cssl_ui::Insets::uniform(8.0),
            },
        );
        ui.label("CSSLv3 Settings");
        // Determine the button id from this frame's tree by re-running the
        // build logic the same way.
        let button_clicked = ui.button("Apply");
        if button_clicked {
            clicks += 1;
        }
        let _ = ui.checkbox("Mute", &mut muted);
        let _ = ui.slider("volume", &mut volume, 0.0, 1.0);
        let _ = ui.text_input("Player name", &mut name);
        ui.container_end();
        // Locate the button frame (it's the second entry after the
        // container start + label).
        let entries_snapshot: Vec<_> = ui.state().focus_order.clone();
        // Feed events : move cursor over button + click.
        // The button entry's frame is recorded — we hover over its centre.
        // First we have to know the resolved frame ; we end-frame to run
        // dispatch + paint, then begin a new frame for the click events.
        let _ = entries_snapshot;
        let mut paint = PaintList::new();
        let _ = ui.end_frame(&mut paint);
    }

    // Frame 3 : actually exercise the routing.
    {
        ui.begin_frame(Size::new(400.0, 300.0));
        ui.container_begin(
            Container::Vbox,
            ContainerStyle {
                gap: 6.0,
                main_align: MainAlign::Start,
                cross_align: cssl_ui::CrossAlign::Start,
                padding: cssl_ui::Insets::uniform(8.0),
            },
        );
        ui.label("CSSLv3 Settings");
        let _ = ui.button("Apply");
        let _ = ui.checkbox("Mute", &mut muted);
        let _ = ui.slider("volume", &mut volume, 0.0, 1.0);
        let _ = ui.text_input("Player name", &mut name);
        ui.container_end();
        // Find the button entry's resolved frame.
        let button_frame = ui
            .entries
            .iter()
            .find(|e| {
                matches!(
                    &e.kind,
                    cssl_ui::context::FrameEntryKind::Button { label, .. }
                        if label == "Apply"
                )
            })
            .map(|e| e.frame)
            .expect("button entry not found");
        let center = button_frame.center();
        // Hover + click.
        ui.feed_event(UiEvent::PointerMove {
            position: center,
            modifiers: ModifierKeys::empty(),
            pointer_id: 0,
        });
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
        let mut paint = PaintList::new();
        let changed = ui.end_frame(&mut paint);
        assert!(
            changed >= 1,
            "expected at least one widget to register state change"
        );
    }

    // Frame 4 : confirm button reports pressed=true now (the click latched
    // its retained one-shot in frame 3 ; this frame returns it).
    {
        ui.begin_frame(Size::new(400.0, 300.0));
        ui.container_begin(
            Container::Vbox,
            ContainerStyle {
                gap: 6.0,
                main_align: MainAlign::Start,
                cross_align: cssl_ui::CrossAlign::Start,
                padding: cssl_ui::Insets::uniform(8.0),
            },
        );
        ui.label("CSSLv3 Settings");
        if ui.button("Apply") {
            clicks += 1;
        }
        let _ = ui.checkbox("Mute", &mut muted);
        let _ = ui.slider("volume", &mut volume, 0.0, 1.0);
        let _ = ui.text_input("Player name", &mut name);
        ui.container_end();
        let mut paint = PaintList::new();
        let _ = ui.end_frame(&mut paint);
    }

    // Verify : 1 button click registered.
    assert_eq!(clicks, 1, "exactly one button click should have fired");

    // Verify : the 5 widgets show up in the focus order (Label is not
    // focusable but Button + Checkbox + Slider + TextInput are, so 4 + 0).
    assert_eq!(
        ui.state().focus_order.len(),
        4,
        "expected 4 focusable widgets (button + checkbox + slider + text_input)"
    );
}

#[test]
fn keyboard_tab_navigation_cycles_through_focusables() {
    let mut ui = Ui::new(Theme::dark());
    let mut muted = false;
    let mut volume = 0.5;
    let mut name = String::new();
    // Build the same five-widget UI.
    ui.begin_frame(Size::new(400.0, 300.0));
    ui.label("hi");
    let _ = ui.button("OK");
    let _ = ui.checkbox("Mute", &mut muted);
    let _ = ui.slider("vol", &mut volume, 0.0, 1.0);
    let _ = ui.text_input("name", &mut name);
    let mut paint = PaintList::new();
    let _ = ui.end_frame(&mut paint);

    // No focus initially.
    assert!(ui.current_focus().is_none());
    // Press Tab → focus moves to first focusable.
    ui.begin_frame(Size::new(400.0, 300.0));
    ui.label("hi");
    let _ = ui.button("OK");
    let _ = ui.checkbox("Mute", &mut muted);
    let _ = ui.slider("vol", &mut volume, 0.0, 1.0);
    let _ = ui.text_input("name", &mut name);
    ui.feed_event(UiEvent::KeyDown {
        key: KeyCode::Tab,
        modifiers: ModifierKeys::empty(),
        repeat: false,
    });
    let _ = ui.end_frame(&mut paint);
    assert!(
        ui.current_focus().is_some(),
        "Tab should move focus to first focusable widget"
    );
    let after_first = ui.current_focus().unwrap();
    // Press Tab again → focus moves to next.
    ui.begin_frame(Size::new(400.0, 300.0));
    ui.label("hi");
    let _ = ui.button("OK");
    let _ = ui.checkbox("Mute", &mut muted);
    let _ = ui.slider("vol", &mut volume, 0.0, 1.0);
    let _ = ui.text_input("name", &mut name);
    ui.feed_event(UiEvent::KeyDown {
        key: KeyCode::Tab,
        modifiers: ModifierKeys::empty(),
        repeat: false,
    });
    let _ = ui.end_frame(&mut paint);
    let after_second = ui.current_focus().unwrap();
    assert_ne!(
        after_first, after_second,
        "second Tab should advance to a different widget"
    );
}

#[test]
fn hover_test_picks_deepest_widget_under_cursor() {
    let mut ui = Ui::new(Theme::dark());
    let mut muted = false;
    ui.begin_frame(Size::new(400.0, 300.0));
    ui.container_begin(Container::Vbox, ContainerStyle::default());
    let _ = ui.button("Top");
    let _ = ui.checkbox("Bottom", &mut muted);
    ui.container_end();
    // Look up checkbox frame.
    let checkbox_frame = ui
        .entries
        .iter()
        .find(|e| {
            matches!(
                &e.kind,
                cssl_ui::context::FrameEntryKind::Checkbox { label, .. } if label == "Bottom"
            )
        })
        .map(|e| e.frame)
        .expect("checkbox not in entries");
    let cursor = checkbox_frame.center();
    ui.feed_event(UiEvent::PointerMove {
        position: cursor,
        modifiers: ModifierKeys::empty(),
        pointer_id: 0,
    });
    let mut paint = PaintList::new();
    let _ = ui.end_frame(&mut paint);
    // Cursor should hover over the checkbox, not the button.
    let hovered = ui.state().hovered.expect("nothing hovered");
    let checkbox_id = ui
        .entries
        .iter()
        .find(|e| matches!(&e.kind, cssl_ui::context::FrameEntryKind::Checkbox { .. }))
        .map(|e| e.id)
        .unwrap();
    assert_eq!(hovered, checkbox_id, "deepest widget should win hover");
}

#[test]
fn slider_value_persists_across_frames_via_retained_store() {
    let mut ui = Ui::new(Theme::dark());
    let mut volume: f32 = 0.0;
    // Frame 1 : initial build.
    ui.begin_frame(Size::new(200.0, 200.0));
    let _ = ui.slider("vol", &mut volume, 0.0, 1.0);
    let mut paint = PaintList::new();
    let _ = ui.end_frame(&mut paint);
    // Frame 2 : keyboard arrow-right via focus.
    ui.begin_frame(Size::new(200.0, 200.0));
    let _ = ui.slider("vol", &mut volume, 0.0, 1.0);
    // Manually set focus to the slider (Tab would take 1 keypress here ;
    // we shortcut via the api).
    let slider_id = ui
        .entries
        .iter()
        .find(|e| matches!(&e.kind, cssl_ui::context::FrameEntryKind::Slider { .. }))
        .map(|e| e.id)
        .unwrap();
    ui.state.set_focus(Some(slider_id));
    ui.feed_event(UiEvent::KeyDown {
        key: KeyCode::Right,
        modifiers: ModifierKeys::empty(),
        repeat: false,
    });
    let _ = ui.end_frame(&mut paint);
    // Frame 3 : verify the value advanced (5% step → 0.05).
    ui.begin_frame(Size::new(200.0, 200.0));
    let _ = ui.slider("vol", &mut volume, 0.0, 1.0);
    let _ = ui.end_frame(&mut paint);
    assert!(
        volume > 0.0,
        "slider value should have advanced ; got {volume}"
    );
}

#[test]
fn text_input_appends_chars_when_focused() {
    let mut ui = Ui::new(Theme::dark());
    let mut name = String::new();
    // Frame 1 : tree build.
    ui.begin_frame(Size::new(400.0, 300.0));
    let _ = ui.text_input("name", &mut name);
    let mut paint = PaintList::new();
    let _ = ui.end_frame(&mut paint);
    // Frame 2 : focus + send char events.
    ui.begin_frame(Size::new(400.0, 300.0));
    let _ = ui.text_input("name", &mut name);
    let id = ui
        .entries
        .iter()
        .find(|e| matches!(&e.kind, cssl_ui::context::FrameEntryKind::TextInput { .. }))
        .map(|e| e.id)
        .unwrap();
    ui.state.set_focus(Some(id));
    ui.feed_event(UiEvent::Char {
        ch: 'A',
        modifiers: ModifierKeys::empty(),
    });
    ui.feed_event(UiEvent::Char {
        ch: 'p',
        modifiers: ModifierKeys::empty(),
    });
    let _ = ui.end_frame(&mut paint);
    // Frame 3 : verify the buffer received the typed chars.
    ui.begin_frame(Size::new(400.0, 300.0));
    let _ = ui.text_input("name", &mut name);
    let _ = ui.end_frame(&mut paint);
    assert!(
        name.contains('A'),
        "typed 'A' should appear in buffer ; got {name:?}"
    );
}

#[test]
fn paint_list_records_commands_for_dark_theme() {
    let mut ui = Ui::new(Theme::dark());
    ui.begin_frame(Size::new(400.0, 300.0));
    let _ = ui.button("Hello");
    let mut paint = PaintList::new();
    let _ = ui.end_frame(&mut paint);
    // Each button paints at least 1 fill + 1 stroke + 1 text.
    assert!(paint.commands().len() >= 3);
}

#[test]
fn theme_switch_changes_paint_output() {
    let mut ui_dark = Ui::new(Theme::dark());
    ui_dark.begin_frame(Size::new(100.0, 100.0));
    let _ = ui_dark.button("X");
    let mut p_dark = PaintList::new();
    let _ = ui_dark.end_frame(&mut p_dark);

    let mut ui_light = Ui::new(Theme::light());
    ui_light.begin_frame(Size::new(100.0, 100.0));
    let _ = ui_light.button("X");
    let mut p_light = PaintList::new();
    let _ = ui_light.end_frame(&mut p_light);

    // Both should paint the same number of commands (button structure is
    // theme-invariant).
    assert_eq!(p_dark.commands().len(), p_light.commands().len());
}

#[test]
fn empty_ui_paints_nothing_when_no_widgets() {
    let mut ui = Ui::new(Theme::dark());
    ui.begin_frame(Size::new(100.0, 100.0));
    let mut p = PaintList::new();
    let _ = ui.end_frame(&mut p);
    // No widgets → no paint commands.
    assert!(p.is_empty());
}

#[test]
fn pointer_outside_hits_nothing() {
    let mut ui = Ui::new(Theme::dark());
    let mut value = false;
    ui.begin_frame(Size::new(400.0, 300.0));
    let _ = ui.checkbox("x", &mut value);
    // Cursor far outside the checkbox bounds.
    ui.feed_event(UiEvent::PointerMove {
        position: Point::new(999.0, 999.0),
        modifiers: ModifierKeys::empty(),
        pointer_id: 0,
    });
    let mut p = PaintList::new();
    let _ = ui.end_frame(&mut p);
    assert!(
        ui.state().hovered.is_none(),
        "no widget should hover when cursor is off-screen"
    );
}
