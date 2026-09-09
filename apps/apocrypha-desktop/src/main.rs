#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Apocrypha for Windows.
//!
//! A thin account client over the same public contract the phone apps use:
//! `https://www.apocky.com/api/mobile/{config,status,turn,sessions}`. Nothing
//! about Apocrypha runs locally — this window signs in, sends a message, and
//! reads the account's own history back from the service.
//!
//! The webview is a renderer. It never sees a token, never reaches the network,
//! and cannot name an endpoint: every command below hands work to the Rust
//! controller, which is the only thing holding a session.

mod api;
mod controller;
mod journal;
mod protocol;
mod session;
mod store;
mod stream;

use std::sync::{Arc, Mutex};

use tauri::Emitter;

use controller::{Controller, View};

/// Carries one fragment of a reply to the window while it is being written.
const TURN_DELTA_EVENT: &str = "apocrypha://turn-delta";

struct App {
    controller: Arc<Mutex<Option<Controller>>>,
    /// Set when local secure storage was unavailable at startup.
    unavailable: String,
}

impl App {
    fn new() -> Self {
        match Controller::new() {
            Ok(controller) => Self {
                controller: Arc::new(Mutex::new(Some(controller))),
                unavailable: String::new(),
            },
            Err(error) => Self {
                controller: Arc::new(Mutex::new(None)),
                unavailable: error,
            },
        }
    }
}

/// Runs one controller flow off the interface thread.
///
/// A single mutex serialises flows, which is what stops a second send from
/// racing an unresolved one; the window disables its own controls to match.
async fn run<F>(state: tauri::State<'_, App>, job: F) -> Result<View, String>
where
    F: FnOnce(&mut Controller) -> View + Send + 'static,
{
    if !state.unavailable.is_empty() {
        return Ok(View {
            notice: state.unavailable.clone(),
            ..View::default()
        });
    }
    let handle = state.controller.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = handle
            .lock()
            .map_err(|_| "The application state is unavailable. Restart Apocrypha.".to_string())?;
        match guard.as_mut() {
            Some(controller) => Ok(job(controller)),
            None => Err("Apocrypha could not open local storage. Restart Apocrypha.".to_string()),
        }
    })
    .await
    .map_err(|_| "That request could not be completed. Retry.".to_string())?
}

#[tauri::command]
async fn bootstrap(state: tauri::State<'_, App>) -> Result<View, String> {
    run(state, |controller| controller.initialize()).await
}

#[tauri::command]
async fn send_code(state: tauri::State<'_, App>, email: String) -> Result<View, String> {
    run(state, move |controller| controller.send_code(&email, false)).await
}

#[tauri::command]
async fn create_account(state: tauri::State<'_, App>, email: String) -> Result<View, String> {
    run(state, move |controller| controller.send_code(&email, true)).await
}

#[tauri::command]
async fn create_with_password(state: tauri::State<'_, App>, email: String, password: String) -> Result<View, String> {
    run(state, move |controller| controller.create_with_password(&email, &password)).await
}

#[tauri::command]
async fn sign_in(state: tauri::State<'_, App>, email: String, secret: String, password: bool) -> Result<View, String> {
    run(state, move |controller| controller.sign_in(&email, &secret, password)).await
}

#[tauri::command]
async fn refresh(state: tauri::State<'_, App>) -> Result<View, String> {
    run(state, |controller| controller.refresh()).await
}

#[tauri::command]
async fn open_conversation(state: tauri::State<'_, App>, id: String) -> Result<View, String> {
    run(state, move |controller| controller.open_conversation(&id)).await
}

#[tauri::command]
async fn new_conversation(state: tauri::State<'_, App>) -> Result<View, String> {
    run(state, |controller| controller.new_conversation()).await
}

/// Sends a message and streams the reply into the window as it is written.
///
/// Each fragment is emitted as it arrives rather than held until the turn ends,
/// so the person watches the reply being composed instead of a spinner.
#[tauri::command]
async fn send(app: tauri::AppHandle, state: tauri::State<'_, App>, text: String) -> Result<View, String> {
    run(state, move |controller| {
        let mut emit = |delta: &str| {
            let _ = app.emit(TURN_DELTA_EVENT, delta);
        };
        controller.send(&text, &mut emit)
    })
    .await
}

#[tauri::command]
async fn sign_out(state: tauri::State<'_, App>) -> Result<View, String> {
    run(state, |controller| controller.sign_out()).await
}

fn main() {
    tauri::Builder::default()
        .manage(App::new())
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            send_code,
            create_account,
            create_with_password,
            sign_in,
            refresh,
            open_conversation,
            new_conversation,
            send,
            sign_out,
        ])
        .run(tauri::generate_context!())
        .expect("Apocrypha could not open its window.");
}
