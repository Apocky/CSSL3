//! The desktop shell (v0.2.0): a Chat window on the living room -- DIRECT to the owner's PC when
//! it answers (127.0.0.1:19141, or APOCRYPHA_DIRECT_URL such as a Tailscale name), apocky.com
//! otherwise -- the bundled Work lane window, and a tray that keeps both one click away.
//! Owner decisions 2026-09-25: direct + fallback, Windows only, tray + notifications, Work lane,
//! voice and the service panel (the last two live in the direct site itself).

use std::time::Duration;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const LOCAL_DIRECT: &str = "http://127.0.0.1:19141/";
const FALLBACK: &str = "https://www.apocky.com/";

fn direct_alive(base: &str) -> bool {
    let probe = format!("{}api/direct/health", base);
    ureq::AgentBuilder::new().timeout(Duration::from_millis(1500)).build()
        .get(&probe).call().map(|r| r.status() == 200).unwrap_or(false)
}

/// Where Chat opens: the first direct address that answers, else apocky.com.
pub fn chat_url() -> (String, bool) {
    let mut candidates = vec![LOCAL_DIRECT.to_string()];
    if let Ok(extra) = std::env::var("APOCRYPHA_DIRECT_URL") {
        let extra = if extra.ends_with('/') { extra } else { format!("{extra}/") };
        candidates.push(extra);
    }
    for base in candidates {
        if direct_alive(&base) { return (base, true); }
    }
    (FALLBACK.to_string(), false)
}

fn show(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let (url, direct) = chat_url();
    let title = if direct { "Apocrypha — direct" } else { "Apocrypha — apocky.com" };
    WebviewWindowBuilder::new(app, "chat", WebviewUrl::External(url.parse()?))
        .title(title).inner_size(1100.0, 800.0).min_inner_size(360.0, 520.0).build()?;

    let chat = MenuItem::with_id(app, "chat", "Chat", true, None::<&str>)?;
    let work = MenuItem::with_id(app, "work", "Work lane", true, None::<&str>)?;
    let reconnect = MenuItem::with_id(app, "reconnect", "Reconnect (direct / apocky.com)", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&chat, &work, &reconnect, &quit])?;
    let mut tray = TrayIconBuilder::new().menu(&menu).tooltip(title).show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "chat" => show(app, "chat"),
            "work" => show(app, "main"),
            "reconnect" => {
                let (url, direct) = chat_url();
                if let (Some(window), Ok(parsed)) = (app.get_webview_window("chat"), url.parse()) {
                    let _ = window.navigate(parsed);
                    let _ = window.set_title(if direct { "Apocrypha — direct" } else { "Apocrypha — apocky.com" });
                    show(app, "chat");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show(tray.app_handle(), "chat");
            }
        });
    if let Some(icon) = app.default_window_icon() { tray = tray.icon(icon.clone()); }
    tray.build(app)?;
    Ok(())
}

/// Closing a window hides it to the tray; Quit in the tray ends the app.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}
