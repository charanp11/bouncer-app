// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use bouncer_core::ipc::{self, Handler, Server};

fn main() {
    start_relay_server();
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Bouncer");
}

/// Listens for the relay. If this fails, hooks find nobody and Claude Code
/// keeps asking in the terminal, so the app still starts.
fn start_relay_server() {
    let server = ipc::endpoint().ok_or_else(|| std::io::Error::other("no endpoint"));
    match server.and_then(|path| Server::bind(&path)) {
        // Phase 1: events are only logged in debug builds; no decisions yet,
        // so every permission request falls back to the terminal.
        Ok(server) => {
            let handler: Handler = Arc::new(|event| {
                #[cfg(debug_assertions)]
                eprintln!("{} {} {:?}", event.session, event.kind, event.tool);
                let _ = event;
                None
            });
            std::thread::spawn(move || server.run(handler));
        }
        Err(e) => eprintln!("Bouncer: relay endpoint unavailable: {e}"),
    }
}
