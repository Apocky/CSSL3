fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "local_bootstrap", "local_new_session", "local_open_session", "local_send",
        "local_cancel", "local_consent", "local_request", "bootstrap", "send_code",
        "create_account", "create_with_password", "sign_in", "refresh",
        "open_conversation", "new_conversation", "send", "sign_out",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("The desktop capability manifest could not be built.");
}
