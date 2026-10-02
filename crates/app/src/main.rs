// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bouncer_core::approvals::{Desk, WAIT};
use bouncer_core::ipc::{self, Handler, Server};
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, State};

/// Shared app state: the approval desk plus what the island window needs.
struct Island {
    desk: Arc<Desk>,
    /// The island page's feed; replaced when the page (re)subscribes.
    feed: Mutex<Option<Channel<Value>>>,
    /// The user clicked the pill open.
    expanded: AtomicBool,
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![subscribe, decide, expand])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            let desk = Arc::new(Desk::new(WAIT, move |view| show(&handle, view)));
            app.manage(Island {
                desk: desk.clone(),
                feed: Mutex::new(None),
                expanded: AtomicBool::new(false),
            });
            start_relay_server(desk);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bouncer");
}

/// Listens for the relay. If this fails, hooks find nobody and Claude Code
/// keeps asking in the terminal, so the app still starts.
fn start_relay_server(desk: Arc<Desk>) {
    let server = ipc::endpoint().ok_or_else(|| std::io::Error::other("no endpoint"));
    match server.and_then(|path| Server::bind(&path)) {
        Ok(server) => {
            let handler: Handler = Arc::new(move |event| desk.handle(event));
            std::thread::spawn(move || server.run(handler));
        }
        Err(e) => eprintln!("Bouncer: relay endpoint unavailable: {e}"),
    }
}

/// Sends the view to the page and sizes the window to match: hidden with
/// nothing to show, a pill while sessions run, open with a request or when
/// the user opened it.
fn show(app: &AppHandle, mut view: Value) {
    let island = app.state::<Island>();
    let has = |key: &str| view[key].as_array().is_some_and(|a| !a.is_empty());
    let open = has("queue") || (has("sessions") && island.expanded.load(Ordering::Relaxed));
    let hidden = !has("queue") && !has("sessions");
    view["open"] = open.into();
    if let Some(feed) = island.feed.lock().unwrap().as_ref() {
        let _ = feed.send(view);
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if hidden {
        let _ = window.hide();
        return;
    }
    let (width, height) = if open { (440.0, 380.0) } else { (320.0, 44.0) };
    let _ = window.set_size(LogicalSize::new(width, height));
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let x = area.position.x + (area.size.width as i32 - (width * scale) as i32) / 2;
        let y = area.position.y + (8.0 * scale) as i32;
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    // The window is not focusable, so showing it never takes the keyboard.
    let _ = window.show();
}

/// The island page asks for the view; it gets it now and after every change.
#[tauri::command]
fn subscribe(app: AppHandle, island: State<'_, Island>, feed: Channel<Value>) {
    *island.feed.lock().unwrap() = Some(feed);
    show(&app, island.desk.view());
}

/// Answers one request by its ID. Unknown, used or too-early IDs are refused.
#[tauri::command]
fn decide(island: State<'_, Island>, id: String, allow: bool) -> Result<(), String> {
    island.desk.decide(&id, allow).map_err(str::to_owned)
}

/// Opens or closes the island when the user clicks it.
#[tauri::command]
fn expand(app: AppHandle, island: State<'_, Island>, open: bool) {
    island.expanded.store(open, Ordering::Relaxed);
    show(&app, island.desk.view());
}
