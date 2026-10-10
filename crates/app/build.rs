fn main() {
    // Every app command must be listed here and granted in a capability file;
    // unlisted commands cannot be called from any window.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "subscribe",
        "decide",
        "always",
        "expand",
        "drag",
        "fit",
        "settings",
        "set_prefs",
        "wipe",
        "about",
        "set_mode",
        "keyboard",
        "chat_open",
        "chat_subscribe",
        "chat_status",
        "chat_confirm",
        "chat_send",
        "chat_cancel",
        "chat_new",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to run tauri-build");
}
