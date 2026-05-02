//! § tauri_shell — Tauri 2.x main entry-point.
//!
//! § T11-W17-J · Apocky-greenlit (2026-05-02) replacement for the prior panic-
//!   stub. The shell is the desktop wrapper around `MyceliumApp` ; the
//!   frontend (React in `frontend/`) sends `IpcCommand` JSON, the backend
//!   dispatches via `commands::handle_command`, and the response is `IpcResponse`.
//!
//! § PROPRIETARY local-intelligence
//!   The chat-path uses `app.run_substrate_turn` (substrate-intelligence
//!   procedural composer) — NO Anthropic API, NO LLM-bridge, NO network
//!   egress. Per Apocky-foundational-axiom (memory/feedback_no_external_
//!   llm_for_loa_intelligence). The cssl-host-llm-bridge crate stays in
//!   the workspace as an opt-in tool-augmentation surface for future Coder-
//!   role flows but is NOT on the canonical chat path.
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![allow(dead_code)]

#[cfg(feature = "tauri-shell")]
fn main() {
    use std::sync::Mutex;

    use cssl_host_mycelium_desktop::{
        commands::{handle_command, IpcCommand, IpcResponse},
        config::AppConfig,
        MyceliumApp,
    };
    use tauri::Manager;

    /// Single Tauri command surface. The frontend sends an `IpcCommand` JSON
    /// payload ; we lock the shared `MyceliumApp`, dispatch, and return the
    /// `IpcResponse`. All chat replies route through `run_substrate_turn`
    /// (proprietary local intelligence).
    #[tauri::command]
    fn ipc_dispatch(
        state: tauri::State<'_, Mutex<MyceliumApp>>,
        command: IpcCommand,
    ) -> IpcResponse {
        let mut app = match state.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                // Recover the data even if a prior panic poisoned the lock —
                // we still want the app to keep responding to the frontend
                // so the user can revoke or restart.
                poisoned.into_inner()
            }
        };
        handle_command(&mut app, command)
    }

    /// Periodic chat-sync tick (mycelium federated bias-share, sovereign-
    /// local). Runs every ~60s on a separate thread.
    #[tauri::command]
    fn chat_sync_tick(state: tauri::State<'_, Mutex<MyceliumApp>>) -> bool {
        if let Ok(app) = state.lock() {
            app.chat_sync_tick();
            true
        } else {
            false
        }
    }

    /// Diagnostic / version banner the frontend can show in the title bar.
    #[tauri::command]
    fn mycelium_banner() -> &'static str {
        "Mycelium · The Infinity Engine · proprietary local intelligence · sovereign-revocable"
    }

    eprintln!("§ Mycelium-Desktop starting · proprietary local intelligence · ¬ network");

    tauri::Builder::default()
        .setup(|app| {
            // Default config. Real config-load (sandbox paths · cap mode ·
            // theme · audit-toggle) happens through the IPC `UpdateConfig`
            // command after the frontend mounts.
            let cfg = AppConfig::default();
            let mycelium = MyceliumApp::new(cfg).map_err(|e| {
                Box::<dyn std::error::Error>::from(format!("mycelium init failed: {e}"))
            })?;
            app.manage(Mutex::new(mycelium));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc_dispatch,
            chat_sync_tick,
            mycelium_banner
        ])
        .run(tauri::generate_context!())
        .expect("Mycelium failed to start");
}

#[cfg(not(feature = "tauri-shell"))]
fn main() {
    eprintln!(
        "Mycelium tauri-shell binary requires --features tauri-shell. \
         Use `cargo build --features tauri-shell --release`."
    );
    std::process::exit(1);
}
