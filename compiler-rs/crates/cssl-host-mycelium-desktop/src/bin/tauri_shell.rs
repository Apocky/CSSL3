//! § tauri_shell — feature-gated Tauri 2.x main entry-point.
//!
//! § Status : SCAFFOLD — the file exists, the structure is right, but the
//! actual `tauri` dep is intentionally NOT yet declared in Cargo.toml so
//! the default workspace `cargo build` stays fast (200+ transitive crates
//! avoided). When Apocky enables real Tauri runtime, follow the steps in
//! `frontend/README.md` ; until then, building with `--features tauri-shell`
//! produces a clear-error-stub-main rather than a Tauri app.
//!
//! § Future shape (commented for reference)
//!   ```ignore
//!   tauri::Builder::default()
//!       .setup(|app| {
//!           let cfg = AppConfig::default();
//!           let mycelium = MyceliumApp::new(cfg).expect("mycelium init");
//!           app.manage(Mutex::new(mycelium));
//!           Ok(())
//!       })
//!       .invoke_handler(tauri::generate_handler![ipc_dispatch])
//!       .run(tauri::generate_context!())
//!       .expect("Mycelium failed to start");
//!   ```
//!
//! § ipc_dispatch (future)
//!   ```ignore
//!   #[tauri::command]
//!   fn ipc_dispatch(
//!       state: tauri::State<Mutex<MyceliumApp>>,
//!       command: IpcCommand,
//!   ) -> IpcResponse {
//!       let mut app = state.lock().expect("mycelium mutex poisoned");
//!       handle_command(&mut app, command)
//!   }
//!   ```

#![allow(dead_code)]

#[cfg(feature = "tauri-shell")]
fn main() {
    // To enable the real Tauri runtime :
    //   1. In Cargo.toml, add under [dependencies] :
    //        tauri = { version = "2", optional = true }
    //   2. Change the [features] line to :
    //        tauri-shell = ["dep:tauri"]
    //   3. `npm install` in `frontend/` ; `cargo install tauri-cli --version "^2.0"`.
    //   4. `cargo tauri dev` for live-reload, `cargo tauri build` for installer.
    // See `frontend/README.md` for the full Apocky-action checklist.
    panic!(
        "tauri-shell feature is enabled but the Tauri dep is not yet uncommented in Cargo.toml \
         — see frontend/README.md for the Apocky-action steps to enable the real runtime."
    );
}

#[cfg(not(feature = "tauri-shell"))]
fn main() {
    eprintln!(
        "Mycelium tauri-shell binary requires --features tauri-shell. \
         Use `cargo build --features tauri-shell --release` (and follow frontend/README.md to \
         enable the real Tauri dep)."
    );
    std::process::exit(1);
}
