// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use bouncer_core::approvals::{Desk, WAIT};
use bouncer_core::ipc::{self, Handler, Server};
use serde_json::Value;
use tauri::image::Image;
use tauri::ipc::Channel;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent};

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
    /// The size the page last asked for (it measures its own content).
    size: Mutex<LogicalSize<f64>>,
    /// Told about every window move; see `settle_after_moves`.
    moved: Mutex<mpsc::Sender<()>>,
}

/// How long the island must stay still before it's checked for being out of
/// reach: long enough that a drag in progress is never fought.
const SETTLE: Duration = Duration::from_millis(400);

/// Once the island stops moving (the user let go), brings it back if it
/// ended up out of reach. Checking during the drag made it stutter.
fn settle_after_moves(app: AppHandle) -> mpsc::Sender<()> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(SETTLE).is_ok() {}
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(window) = handle.get_webview_window("main") {
                    keep_on_screen(&window.as_ref().window());
                }
            });
        }
    });
    tx
}

/// Bounds on what the page may ask for, in logical pixels. The real sizes
/// live in `src/styles.css`; these only stop a broken page from making the
/// window vanish or cover the screen.
const MIN_SIZE: (f64, f64) = (80.0, 24.0);
const MAX_SIZE: (f64, f64) = (900.0, 900.0);

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            subscribe, decide, expand, drag, fit
        ])
        .on_window_event(|window, event| {
            if let (WindowEvent::Moved(_), Some(island)) = (event, window.try_state::<Island>()) {
                let _ = island.moved.lock().unwrap().send(());
            }
        })
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
                size: Mutex::new(LogicalSize::new(320.0, 44.0)),
                moved: Mutex::new(settle_after_moves(app.handle().clone())),
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
    // Window work happens on the main thread only: this runs on relay
    // connection threads too, and window calls from them wait for the main
    // thread, which could be waiting on our locks (deadlock).
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window("main") else {
            return;
        };
        if hidden {
            let _ = window.hide();
            return;
        }
        lay_out(&window, &handle.state::<Island>());
        // The window is not focusable, so showing it never takes the keyboard.
        let _ = window.show();
    });
}

/// Sizes the window to what the page asked for and places it: hanging from
/// the user's spot (or the top centre at first), kept inside the work area.
/// Main thread only.
fn lay_out(window: &WebviewWindow, island: &Island) {
    let mut anchor = island.anchor.lock().unwrap();
    let mut placed = island.placed.lock().unwrap();
    // A visible window that isn't where we put it was dragged: that's the new spot.
    if window.is_visible().unwrap_or(false)
        && let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size())
        && *placed != Some(pos)
    {
        *anchor = Some(PhysicalPosition::new(pos.x + size.width as i32 / 2, pos.y));
    }
    let logical = *island.size.lock().unwrap();
    let _ = window.set_size(logical);
    let top_centre = anchor.or_else(|| {
        // First show: top centre of the primary monitor's work area.
        let monitor = window.primary_monitor().ok()??;
        let area = monitor.work_area();
        Some(PhysicalPosition::new(
            area.position.x + area.size.width as i32 / 2,
            area.position.y + (8.0 * monitor.scale_factor()) as i32,
        ))
    });
    let Some(at) = top_centre else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    let size = (
        (logical.width * scale) as i32,
        (logical.height * scale) as i32,
    );
    let pos = match work_area(&window.as_ref().window(), at) {
        Some(area) => place((at.x, at.y), size, area),
        None => (at.x - size.0 / 2, at.y),
    };
    let pos = PhysicalPosition::new(pos.0, pos.1);
    let _ = window.set_position(pos);
    *placed = Some(pos);
    // The anchor stays where the user put it; only this placement is clamped.
    *anchor = Some(at);
}

/// The work area (position, size; taskbar and menu bar excluded) of the
/// monitor holding `point`, in physical pixels.
fn work_area<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    point: PhysicalPosition<i32>,
) -> Option<((i32, i32), (i32, i32))> {
    let monitor = window
        .monitor_from_point(f64::from(point.x), f64::from(point.y))
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())?;
    let area = monitor.work_area();
    Some((
        (area.position.x, area.position.y),
        (area.size.width as i32, area.size.height as i32),
    ))
}

/// A drag that leaves the island's centre under the taskbar or off every
/// screen pushes it back inside the work area, so it can always be reached.
/// Straddling two monitors is fine, so it can be dragged across them.
fn keep_on_screen<R: tauri::Runtime>(window: &tauri::Window<R>) {
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let size = (size.width as i32, size.height as i32);
    let centre = PhysicalPosition::new(pos.x + size.0 / 2, pos.y + size.1 / 2);
    let Some(area) = work_area(window, centre) else {
        return;
    };
    if contains(area, (centre.x, centre.y)) {
        return;
    }
    let inside = place((pos.x + size.0 / 2, pos.y), size, area);
    if inside != (pos.x, pos.y) {
        let _ = window.set_position(PhysicalPosition::new(inside.0, inside.1));
    }
}

/// Whether `point` lies inside `area` (position, size).
fn contains(area: ((i32, i32), (i32, i32)), point: (i32, i32)) -> bool {
    let ((ax, ay), (aw, ah)) = area;
    (ax..ax + aw).contains(&point.0) && (ay..ay + ah).contains(&point.1)
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

/// The page reports the size of its content; the window follows it.
#[tauri::command]
fn fit(app: AppHandle, island: State<'_, Island>, width: f64, height: f64) {
    if !(width.is_finite() && height.is_finite()) {
        return;
    }
    *island.size.lock().unwrap() = LogicalSize::new(
        width.clamp(MIN_SIZE.0, MAX_SIZE.0),
        height.clamp(MIN_SIZE.1, MAX_SIZE.1),
    );
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("main")
            && window.is_visible().unwrap_or(false)
        {
            lay_out(&window, &handle.state::<Island>());
        }
    });
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
    fn reachable_points_need_no_push() {
        // 1920×1080 screen with a 40 px taskbar at the bottom.
        let area = ((0, 0), (1920, 1040));
        assert!(contains(area, (960, 500)));
        assert!(contains(area, (0, 0)));
        assert!(!contains(area, (960, 1060)), "under the taskbar");
        assert!(!contains(area, (1920, 500)), "next monitor over");
        assert!(!contains(area, (-1, 500)));
        // Left monitor: its own area contains points with negative x.
        assert!(contains(((-1920, 0), (1920, 1080)), (-5, 500)));
    }

    #[test]
    fn greyed_icon_is_grey_and_fainter() {
        let out = greyed(&[255, 0, 0, 255, 10, 200, 30, 128]);
        assert_eq!(out, [76, 76, 76, 127, 126, 126, 126, 64]);
    }
}
