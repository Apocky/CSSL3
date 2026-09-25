#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Apocrypha for Windows.
//!
//! The default workbench uses the local Work host. Legacy cloud-account
//! modules remain available for compatibility but are not the entry screen.
//!
//! The webview is a renderer. It never sees a token, never reaches the network,
//! and cannot name an endpoint: every command below hands work to the Rust
//! controller, which is the only thing holding a session.

mod api;
mod controller;
mod journal;
mod local;
mod local_stream;
mod protocol;
mod session;
mod shell;
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

struct LocalApp(Arc<local_stream::LocalController>);

async fn run_local<F>(state: tauri::State<'_, LocalApp>, job: F) -> Result<serde_json::Value, String>
where
    F: FnOnce(&local_stream::LocalController) -> Result<serde_json::Value, String> + Send + 'static,
{
    let controller = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || job(&controller)).await
        .map_err(|_| "The native local request could not finish. Check task history before retrying.".to_string())?
}

#[tauri::command]
async fn local_bootstrap(state: tauri::State<'_, LocalApp>) -> Result<serde_json::Value, String> {
    run_local(state, |controller| Ok(controller.bootstrap())).await
}

#[tauri::command]
async fn local_new_session(state: tauri::State<'_, LocalApp>, title: String) -> Result<serde_json::Value, String> {
    run_local(state, move |controller| controller.create(&title)).await
}

#[tauri::command]
async fn local_open_session(state: tauri::State<'_, LocalApp>, session_id: String) -> Result<serde_json::Value, String> {
    run_local(state, move |controller| controller.open(local::session_id(&session_id)?)).await
}

#[tauri::command]
async fn local_send(state: tauri::State<'_, LocalApp>, session_id: String, options: local::SendOptions) -> Result<serde_json::Value, String> {
    run_local(state, move |controller| controller.send(local::session_id(&session_id)?, options)).await
}

#[tauri::command]
async fn local_cancel(state: tauri::State<'_, LocalApp>, session_id: String) -> Result<serde_json::Value, String> {
    run_local(state, move |controller| controller.cancel(local::session_id(&session_id)?)).await
}

#[tauri::command]
async fn local_consent(state: tauri::State<'_, LocalApp>, session_id: String, epoch: u64, request_id: String, decision: local::ConsentDecision) -> Result<serde_json::Value, String> {
    run_local(state, move |controller| controller.consent(local::session_id(&session_id)?, epoch, local::session_id(&request_id)?, decision)).await
}

#[tauri::command]
async fn local_request(app: tauri::AppHandle, state: tauri::State<'_, LocalApp>, request: local_stream::LocalRequest) -> Result<serde_json::Value, String> {
    run_local(state, move |controller| controller.request(request, Arc::new(move |event, payload| { let _ = app.emit(event, payload); }))).await
}

fn main() {
    tauri::Builder::default()
        .setup(|app| shell::setup(app))
        .on_window_event(|window, event| shell::on_window_event(window, event))
        .manage(App::new())
        .manage(LocalApp(Arc::new(local_stream::LocalController::new())))
        .invoke_handler(tauri::generate_handler![
            local_bootstrap,
            local_new_session,
            local_open_session,
            local_send,
            local_cancel,
            local_consent,
            local_request,
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
