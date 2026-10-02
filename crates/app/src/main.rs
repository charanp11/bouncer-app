// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bouncer_core::approvals::{Desk, WAIT};
use bouncer_core::ipc::{self, Handler, Server};
use serde_json::Value;
use tauri::image::Image;
use tauri::ipc::Channel;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, State};

/// Shared app state: the approval desk plus what the island window needs.
struct Island {
    desk: Arc<Desk>,
    /// The island page's feed; replaced when the page (re)subscribes.
    feed: Mutex<Option<Channel<Value>>>,
    /// The user clicked the pill open.
    expanded: AtomicBool,
    /// Where the island sits: its top-centre point in physical pixels. Kept
    /// across resizes and hides, so a dragged island stays where it was put.
    anchor: Mutex<Option<PhysicalPosition<i32>>>,
    /// Where Bouncer last put the window; anywhere else means the user dragged it.
    placed: Mutex<Option<PhysicalPosition<i32>>>,
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![subscribe, decide, expand, drag])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            let desk = Arc::new(Desk::new(WAIT, move |view| show(&handle, view)));
            app.manage(Island {
                desk: desk.clone(),
                feed: Mutex::new(None),
                expanded: AtomicBool::new(false),
                anchor: Mutex::new(None),
                placed: Mutex::new(None),
            });
            start_relay_server(desk.clone());
            tray(app, desk)?;
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

const TOOLTIP: &str = "Bouncer";
const TOOLTIP_PAUSED: &str = "Bouncer (paused): Claude Code asks in the terminal";

/// Tray menu: Pause / Resume and Quit. While paused the icon is greyed and
/// every request goes to the terminal.
fn tray(app: &tauri::App, desk: Arc<Desk>) -> tauri::Result<()> {
    let pause = MenuItem::with_id(app, "pause", "Pause", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Bouncer", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&pause, &quit])?;
    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("app icon".into()))?;
    let (width, height) = (icon.width(), icon.height());
    let active = Image::new_owned(icon.rgba().to_vec(), width, height);
    let paused = Image::new_owned(greyed(icon.rgba()), width, height);
    TrayIconBuilder::with_id("main")
        .icon(active.clone())
        .tooltip(TOOLTIP)
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "pause" => {
                let now_paused = !desk.paused();
                desk.set_paused(now_paused);
                let _ = pause.set_text(if now_paused { "Resume" } else { "Pause" });
                if let Some(tray) = app.tray_by_id("main") {
                    let icon = if now_paused { &paused } else { &active };
                    let _ = tray.set_icon(Some(icon.clone()));
                    let _ =
                        tray.set_tooltip(Some(if now_paused { TOOLTIP_PAUSED } else { TOOLTIP }));
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// The icon in grey at half opacity (RGBA in, RGBA out).
fn greyed(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let luma = ((u32::from(p[0]) * 3 + u32::from(p[1]) * 6 + u32::from(p[2])) / 10) as u8;
            [luma, luma, luma, p[3] / 2]
        })
        .collect()
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
    let mut anchor = island.anchor.lock().unwrap();
    let mut placed = island.placed.lock().unwrap();
    // A visible window that isn't where we put it was dragged: that's the new spot.
    if window.is_visible().unwrap_or(false)
        && let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size())
        && *placed != Some(pos)
    {
        *anchor = Some(PhysicalPosition::new(pos.x + size.width as i32 / 2, pos.y));
    }
    if hidden {
        let _ = window.hide();
        return;
    }
    let (width, height) = if open { (440.0, 380.0) } else { (320.0, 44.0) };
    let _ = window.set_size(LogicalSize::new(width, height));
    let top_centre = anchor.or_else(|| {
        // First show: top centre of the primary monitor's work area.
        let monitor = window.primary_monitor().ok()??;
        let area = monitor.work_area();
        Some(PhysicalPosition::new(
            area.position.x + area.size.width as i32 / 2,
            area.position.y + (8.0 * monitor.scale_factor()) as i32,
        ))
    });
    if let Some(at) = top_centre {
        let scale = window.scale_factor().unwrap_or(1.0);
        let size = ((width * scale) as i32, (height * scale) as i32);
        let monitor = window
            .monitor_from_point(f64::from(at.x), f64::from(at.y))
            .ok()
            .flatten()
            .or_else(|| window.current_monitor().ok().flatten());
        let pos = match monitor {
            Some(m) => {
                let area = m.work_area();
                let area = (
                    (area.position.x, area.position.y),
                    (area.size.width as i32, area.size.height as i32),
                );
                place((at.x, at.y), size, area)
            }
            None => (at.x - size.0 / 2, at.y),
        };
        let pos = PhysicalPosition::new(pos.0, pos.1);
        let _ = window.set_position(pos);
        *placed = Some(pos);
        // The anchor stays where the user put it; only this placement is clamped.
        *anchor = Some(at);
    }
    // The window is not focusable, so showing it never takes the keyboard.
    let _ = window.show();
}

/// Top-left corner for a window of `size` hanging from `top_centre`, kept
/// inside the monitor's work `area` (position, size) so it never opens off-screen.
fn place(top_centre: (i32, i32), size: (i32, i32), area: ((i32, i32), (i32, i32))) -> (i32, i32) {
    let ((ax, ay), (aw, ah)) = area;
    let clamp = |v: i32, lo: i32, len: i32, span: i32| v.min(lo + span - len).max(lo);
    (
        clamp(top_centre.0 - size.0 / 2, ax, size.0, aw),
        clamp(top_centre.1, ay, size.1, ah),
    )
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

/// Moves the island with the mouse while the button is held (the page calls
/// this when a press on the pill or header starts moving).
#[tauri::command]
fn drag(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.start_dragging();
    }
}

/// Opens or closes the island when the user clicks it.
#[tauri::command]
fn expand(app: AppHandle, island: State<'_, Island>, open: bool) {
    island.expanded.store(open, Ordering::Relaxed);
    show(&app, island.desk.view());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_stays_on_screen() {
        let area = ((0, 0), (1920, 1040));
        // Room to spare: centred under the anchor.
        assert_eq!(place((960, 8), (440, 380), area), (740, 8));
        // Dragged to the bottom-right corner: opens up and to the left.
        assert_eq!(place((1900, 1000), (440, 380), area), (1480, 660));
        // Dragged past the top-left: pulled back in.
        assert_eq!(place((-50, -20), (440, 380), area), (0, 0));
        // Second monitor to the left (negative coordinates).
        assert_eq!(
            place((-100, 500), (440, 380), ((-1920, 0), (1920, 1080))),
            (-440, 500)
        );
    }

    #[test]
    fn greyed_icon_is_grey_and_fainter() {
        let out = greyed(&[255, 0, 0, 255, 10, 200, 30, 128]);
        assert_eq!(out, [76, 76, 76, 127, 126, 126, 126, 64]);
    }
}
