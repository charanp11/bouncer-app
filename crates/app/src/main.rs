// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod region;

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use bouncer_core::activity;
use bouncer_core::approvals::{Desk, WAIT};
use bouncer_core::away::{self, Away};
use bouncer_core::hooks;
use bouncer_core::ipc::{self, Handler, Server};
use bouncer_core::prefs::{self, Prefs, Size, Style};
use bouncer_core::rules::{self, Mode};
use serde_json::{Value, json};
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
    /// Nothing to show: the island is only the wake strip at the screen's top.
    hidden: AtomicBool,
    /// Where the island sits: its top-centre point in physical pixels. Kept
    /// across resizes and hides, so a dragged island stays where it was put.
    anchor: Mutex<Option<PhysicalPosition<i32>>>,
    /// Where Bouncer last put the window; anywhere else means the user dragged it.
    placed: Mutex<Option<PhysicalPosition<i32>>>,
    /// The size the page last asked for (it measures its own content).
    size: Mutex<LogicalSize<f64>>,
    /// The largest page this screen and Size can show (the page works it
    /// out): the fixed window the island's region sits in, on Windows.
    frame: Mutex<LogicalSize<f64>>,
    /// Told about every window move; see `settle_after_moves`.
    moved: Mutex<mpsc::Sender<()>>,
    /// The settings screen is open (gear or tray); a card still comes first.
    settings: AtomicBool,
    /// The island has the keyboard: taken on purpose (gear, tray), until
    /// another window is clicked or Esc. The page can't tell by itself: in
    /// WebView2 it always thinks it has focus.
    focused: AtomicBool,
    /// The tray's "Wipe history…" asked for Settings' confirm step; sent
    /// once with the next view.
    confirm_wipe: AtomicBool,
    /// Island size and sound, and where they're saved.
    prefs: Mutex<Prefs>,
    prefs_path: Option<PathBuf>,
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
                    keep_on_screen(&window, &handle.state::<Island>());
                }
            });
        }
    });
    tx
}

/// Bounds on what the page may ask for, in logical pixels. The real sizes
/// live in `src/styles.css`; these only stop a broken page from making the
/// window vanish or cover the screen.
const MIN_SIZE: (f64, f64) = (80.0, 4.0);
/// The window is transparent, so the page can draw rounded corners and a
/// shadow. Tauri 2.12 supports this on Windows and macOS without the private
/// API flag. Set to false for a platform where it fails: the page then draws
/// square corners on a solid window.
const ROUNDED: bool = cfg!(any(windows, target_os = "macos"));
const MAX_SIZE: (f64, f64) = (1200.0, 1200.0);

/// WebView2 takes extra browser flags from this variable, e.g. a remote
/// debugging port that would let any local process read and drive the
/// island. Fine for a dev run; release builds ignore it.
const WEBVIEW2_ARGS: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";

/// Removes `WEBVIEW2_ARGS` from this process when `release`. Must run before
/// any thread starts (first thing in `main`).
fn clear_webview_args(release: bool) {
    if release {
        // SAFETY: called at the top of `main`, before any other thread exists,
        // so nothing can read the environment concurrently.
        unsafe { std::env::remove_var(WEBVIEW2_ARGS) };
    }
}

fn main() {
    clear_webview_args(!cfg!(debug_assertions));
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            subscribe, decide, always, expand, drag, fit, settings, set_prefs, wipe, about,
            set_mode, keyboard
        ])
        .on_window_event(|window, event| match event {
            WindowEvent::Moved(_) => {
                if let Some(island) = window.try_state::<Island>() {
                    let _ = island.moved.lock().unwrap().send(());
                }
            }
            // Clicking anywhere else gives the keyboard back for good: the
            // island can't be focused again until it's opened on purpose.
            // (A focus-in is never trusted: Windows reports one at launch
            // while another app is in front.)
            WindowEvent::Focused(false) => {
                let _ = window.set_focusable(false);
                keyboard_now(window.app_handle(), false);
            }
            _ => {}
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            let desk = Arc::new(Desk::new(WAIT, rules::path(), move |view| {
                show(&handle, view)
            }));
            tick(app.handle().clone(), desk.clone());
            let prefs_path = prefs::path();
            app.manage(Island {
                desk: desk.clone(),
                feed: Mutex::new(None),
                expanded: AtomicBool::new(false),
                hidden: AtomicBool::new(true),
                anchor: Mutex::new(None),
                placed: Mutex::new(None),
                size: Mutex::new(LogicalSize::new(320.0, 44.0)),
                frame: Mutex::new(LogicalSize::new(320.0, 44.0)),
                moved: Mutex::new(settle_after_moves(app.handle().clone())),
                settings: AtomicBool::new(false),
                focused: AtomicBool::new(false),
                confirm_wipe: AtomicBool::new(false),
                prefs: Mutex::new(prefs_path.as_deref().map(prefs::load).unwrap_or_default()),
                prefs_path,
            });
            if let Some(window) = app.get_webview_window("main") {
                // Fail safe: if it can't be set up, the window works as before.
                region::no_caption(&window);
            }
            if start_relay_server(desk.clone()) == Relay::AlreadyRunning {
                // A second launch: the first Bouncer has the pipe and the
                // sessions; a second island would never hear anything. Quit
                // before anything shows (an exit request here comes before
                // the event loop and is ignored, so leave at once).
                std::process::exit(0);
            }
            tray(app, desk)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bouncer");
}

#[derive(Debug, PartialEq, Eq)]
enum Relay {
    Listening,
    /// Another Bouncer of this user already answers on the endpoint.
    AlreadyRunning,
    /// Nobody of ours to talk to (no endpoint, or someone else holds it).
    Unavailable,
}

/// Listens for the relay. If this fails, hooks find nobody and Claude Code
/// keeps asking in the terminal, so the app still starts, unless the
/// endpoint is taken by another Bouncer of ours (then this one is a second
/// launch).
fn start_relay_server(desk: Arc<Desk>) -> Relay {
    let Some(path) = ipc::endpoint() else {
        eprintln!("Bouncer: relay endpoint unavailable: no endpoint");
        return Relay::Unavailable;
    };
    match Server::bind(&path) {
        Ok(server) => {
            // While a card waits, a relay that hung up (Claude Code stopped the
            // hook: answered in its own prompt) takes the card away at once.
            let handler: Handler =
                Arc::new(move |event, gone: &dyn Fn() -> bool| desk.handle_until(event, gone));
            std::thread::spawn(move || server.run(handler));
            Relay::Listening
        }
        Err(e) if another_bouncer_answers(&path) => {
            eprintln!("Bouncer: already running ({e})");
            Relay::AlreadyRunning
        }
        Err(e) => {
            eprintln!("Bouncer: relay endpoint unavailable: {e}");
            Relay::Unavailable
        }
    }
}

/// True if a server on `path` answers and passes the relay's own check (the
/// same user). It's sent nothing: an empty connection is ignored.
fn another_bouncer_answers(path: &std::path::Path) -> bool {
    ipc::answers(path)
}

/// How often the background checks run (each is a metadata read or less).
const TICK: Duration = Duration::from_secs(2);

/// Notices edits to the rules file (the island shows every change), drops
/// sessions that went quiet without ending, and opens the away summary when
/// the user comes back after a long idle span.
fn tick(app: AppHandle, desk: Arc<Desk>) {
    std::thread::spawn(move || {
        let mut away = Away::default();
        let limit = away::limit();
        loop {
            std::thread::sleep(TICK);
            desk.reload_rules();
            desk.expire_sessions();
            let now = activity::ms(std::time::SystemTime::now());
            if let Some(idle) = away::user_idle()
                && let Some(since) = away.update(now, idle, limit)
                && desk.came_back(since)
            {
                app.state::<Island>()
                    .expanded
                    .store(true, Ordering::Relaxed);
                show(&app, desk.view());
            }
        }
    });
}

const TOOLTIP: &str = "Bouncer";
const TOOLTIP_PAUSED: &str = "Bouncer (paused): Claude Code asks in the terminal";

/// Tray menu: Pause / Resume, Wipe history… and Quit. While paused the icon is greyed and
/// every request goes to the terminal. "Wipe history…" never wipes: it opens
/// Settings at its confirm step (Cancel, or the armed "Delete history").
fn tray(app: &tauri::App, desk: Arc<Desk>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Bouncer", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause", true, None::<&str>)?;
    let wipe = MenuItem::with_id(app, "wipe", "Wipe history…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Bouncer", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &pause, &wipe, &quit])?;
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
            "open" => {
                app.state::<Island>()
                    .expanded
                    .store(true, Ordering::Relaxed);
                show(app, desk.view());
                take_keyboard(app);
            }
            id @ ("settings" | "wipe") => {
                let island = app.state::<Island>();
                island.settings.store(true, Ordering::Relaxed);
                if id == "wipe" {
                    island.confirm_wipe.store(true, Ordering::Relaxed);
                }
                show(app, desk.view());
                take_keyboard(app);
            }
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
fn show(app: &AppHandle, view: Value) {
    send(app, view);
    // Window work happens on the main thread only: this runs on relay
    // connection threads too, and window calls from them wait for the main
    // thread, which could be waiting on our locks (deadlock).
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window("main") else {
            return;
        };
        lay_out(&window, &handle.state::<Island>());
        // The window is not focusable, so showing it never takes the keyboard.
        let _ = window.show();
    });
}

/// Sends the page the view with the island's own state added; no window work.
fn send(app: &AppHandle, mut view: Value) {
    let island = app.state::<Island>();
    let has = |key: &str| view[key].as_array().is_some_and(|a| !a.is_empty());
    let away = !view["away"].is_null();
    let settings = island.settings.load(Ordering::Relaxed);
    // Opened on purpose (the pill) it stays open with no sessions too, so
    // Settings is always reachable.
    let expanded = island.expanded.load(Ordering::Relaxed);
    let open = has("queue") || settings || expanded;
    let hidden = !has("queue")
        && !has("sessions")
        && !away
        && !settings
        && !expanded
        && view["paused"] != true;
    island.hidden.store(hidden, Ordering::Relaxed);
    view["open"] = open.into();
    view["hidden"] = hidden.into();
    view["rounded"] = ROUNDED.into();
    view["settings"] = settings.into();
    view["keyboard"] = island.focused.load(Ordering::Relaxed).into();
    view["confirmWipe"] = island.confirm_wipe.swap(false, Ordering::Relaxed).into();
    view["prefs"] = island.prefs.lock().unwrap().to_json();
    if let Some(feed) = island.feed.lock().unwrap().as_ref() {
        let _ = feed.send(view);
    }
}

/// Sizes the window to what the page asked for and places it: hanging from
/// the user's spot (or the top centre at first), kept inside the work area.
/// While hidden it is the wake strip at the top edge of the screen the
/// island is on, right above it (the main screen's top centre before any
/// move; the spot is never saved).
/// On Windows the window is the fixed frame and only its region follows the
/// page (`region`); if that fails, the window is sized to the page as before.
/// Main thread only.
fn lay_out(window: &WebviewWindow, island: &Island) {
    let mut anchor = island.anchor.lock().unwrap();
    let mut placed = island.placed.lock().unwrap();
    let hidden = island.hidden.load(Ordering::Relaxed);
    // A visible window that isn't where we put it was dragged: that's the new
    // spot. Checked even when this layout hides the island (closed with ×
    // right after a drag), or the strip would go back to the old spot.
    if window.is_visible().unwrap_or(false)
        && let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size())
        && *placed != Some(pos)
    {
        *anchor = Some(PhysicalPosition::new(pos.x + size.width as i32 / 2, pos.y));
    }
    let logical = *island.size.lock().unwrap();
    let frame = *island.frame.lock().unwrap();
    let screen_top = |gap: f64| {
        // Top centre of the primary monitor's work area.
        let monitor = window.primary_monitor().ok()??;
        let area = monitor.work_area();
        Some(PhysicalPosition::new(
            area.position.x + area.size.width as i32 / 2,
            area.position.y + (gap * monitor.scale_factor()) as i32,
        ))
    };
    let top_centre = if hidden {
        // The strip stays on the screen the island was moved to, at its top
        // edge right above the island; before any move, the main screen.
        anchor
            .and_then(|a| {
                let ((_, top), _) = work_area(&window.as_ref().window(), a)?;
                Some(PhysicalPosition::new(a.x, top))
            })
            .or_else(|| screen_top(0.0))
    } else {
        // The 8-px gap from the top is inside a transparent window (the
        // page's top padding), so the strip and the island share its top edge.
        anchor.or_else(|| screen_top(if ROUNDED { 0.0 } else { 8.0 }))
    };
    let Some(at) = top_centre else {
        let _ = window.set_size(logical);
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let physical = |s: LogicalSize<f64>| ((s.width * scale) as i32, (s.height * scale) as i32);
    let size = physical(logical);
    let area = work_area(&window.as_ref().window(), at);
    let pos = match area {
        Some(area) => place((at.x, at.y), size, area),
        None => (at.x - size.0 / 2, at.y),
    };
    let framed = area.filter(|_| ROUNDED).and_then(|((ax, ay), (aw, ah))| {
        let page = region::Rect {
            x: pos.0,
            y: pos.1,
            w: size.0,
            h: size.1,
        };
        let area = region::Rect {
            x: ax,
            y: ay,
            w: aw,
            h: ah,
        };
        let (frame, shown) = region::frame_around(page, physical(frame), area);
        region::apply(window, frame, shown).then_some((frame.x, frame.y))
    });
    let pos = framed.unwrap_or_else(|| {
        // No region: the window is exactly the page again, so nothing
        // invisible is left catching clicks.
        region::clear(window);
        let _ = window.set_size(logical);
        let _ = window.set_position(PhysicalPosition::new(pos.0, pos.1));
        pos
    });
    let pos = PhysicalPosition::new(pos.0, pos.1);
    *placed = Some(pos);
    // The anchor stays where the user put it; only this placement is clamped.
    if !hidden {
        *anchor = Some(at);
    }
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
/// The island's centre is judged, not the window's (the window can be the
/// larger frame). Main thread only.
fn keep_on_screen(window: &WebviewWindow, island: &Island) {
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let height = (island.size.lock().unwrap().height * scale) as i32;
    let centre = PhysicalPosition::new(pos.x + size.width as i32 / 2, pos.y + height / 2);
    let Some(area) = work_area(&window.as_ref().window(), centre) else {
        return;
    };
    if !contains(area, (centre.x, centre.y)) {
        // Takes the dragged spot as the anchor and places the island inside.
        lay_out(window, island);
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

/// "Always allow": adds the rule the backend offered for this request, then
/// allows it. The page sends only the ID; the rule is never taken from it.
#[tauri::command]
fn always(island: State<'_, Island>, id: String) -> Result<(), String> {
    island.desk.always(&id).map_err(str::to_owned)
}

/// Moves the island with the mouse while the button is held (the page calls
/// this when a press on the pill or header starts moving).
#[tauri::command]
fn drag(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.start_dragging();
    }
}

/// The page reports the size of its content (and the largest it can need,
/// `frame`); the window follows it. A sync command runs on the main thread,
/// so the window is in place when the page's call returns.
#[tauri::command]
fn fit(
    app: AppHandle,
    island: State<'_, Island>,
    width: f64,
    height: f64,
    frame_width: Option<f64>,
    frame_height: Option<f64>,
) {
    if !(width.is_finite() && height.is_finite()) {
        return;
    }
    let size = LogicalSize::new(
        width.clamp(MIN_SIZE.0, MAX_SIZE.0),
        height.clamp(MIN_SIZE.1, MAX_SIZE.1),
    );
    let frame = (frame_width.unwrap_or(0.0), frame_height.unwrap_or(0.0));
    let frame = if frame.0.is_finite() && frame.1.is_finite() {
        frame
    } else {
        (0.0, 0.0)
    };
    *island.size.lock().unwrap() = size;
    // Never smaller than the page, never past the size limit.
    *island.frame.lock().unwrap() = LogicalSize::new(
        frame.0.clamp(size.width, MAX_SIZE.0),
        frame.1.clamp(size.height, MAX_SIZE.1),
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

/// The island takes the keyboard only when the user opens it on purpose (the
/// tray, or the gear); otherwise it is never focusable, so a card arriving
/// can't take typing away from the terminal. Main thread only for the window.
fn take_keyboard(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("main") {
            let _ = window.set_focusable(true);
            let _ = window.set_focus();
        }
        keyboard_now(&handle, true);
    });
}

/// The page asks for the keyboard (the gear was clicked) or gives it back (Esc).
#[tauri::command]
fn keyboard(app: AppHandle, on: bool) {
    if on {
        take_keyboard(&app);
    } else {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.set_focusable(false);
            }
        });
        keyboard_now(&app, false);
    }
}

/// Tells the page whether the island has the keyboard, so it shows focus
/// (and the ring) only then. Only the page hears it: the window isn't laid
/// out or shown again (doing that while it was active painted a white band
/// beside the island's top corners).
fn keyboard_now(app: &AppHandle, on: bool) {
    let Some(island) = app.try_state::<Island>() else {
        return;
    };
    if island.focused.swap(on, Ordering::Relaxed) != on {
        send(app, island.desk.view());
    }
}

/// Opens or closes the settings screen (the gear, its ×, or Esc).
#[tauri::command]
fn settings(app: AppHandle, island: State<'_, Island>, open: bool) {
    island.settings.store(open, Ordering::Relaxed);
    show(&app, island.desk.view());
}

/// Saves the island size and sound. Only known sizes are accepted.
#[tauri::command]
fn set_prefs(
    app: AppHandle,
    island: State<'_, Island>,
    size: String,
    sound: bool,
    style: String,
    volume: u8,
    sounds: Vec<String>,
) -> Result<(), String> {
    let new = Prefs {
        size: Size::parse(&size).ok_or("unknown size")?,
        sound,
        style: Style::parse(&style).ok_or("unknown style")?,
        volume: Some(volume)
            .filter(|v| *v <= 100)
            .ok_or("volume is 0 to 100")?,
        sounds: Prefs::sounds_from(&sounds).ok_or("unknown sound")?,
    };
    let path = island
        .prefs_path
        .as_deref()
        .ok_or("no place to save preferences")?;
    prefs::save(path, new)?;
    *island.prefs.lock().unwrap() = new;
    show(&app, island.desk.view());
    Ok(())
}

/// The observe / auto switch. Takes a mode name only, never file text; the
/// page asks before turning auto on.
#[tauri::command]
fn set_mode(island: State<'_, Island>, mode: String) -> Result<(), String> {
    let mode = match mode.as_str() {
        "auto" => Mode::Auto,
        "observe" => Mode::Observe,
        _ => return Err("unknown mode".into()),
    };
    island.desk.set_mode(mode).map_err(str::to_owned)
}

/// "Wipe history" from the settings screen (after its confirm step).
#[tauri::command]
fn wipe(island: State<'_, Island>) -> Result<(), String> {
    island.desk.wipe_history().map_err(|e| e.to_string())
}

/// Read-only facts for the settings screen: where the rules file is and
/// whether our hooks are in the user's Claude Code settings.
#[tauri::command]
fn about() -> Value {
    json!({
        "rules": rules::path().map(|p| p.display().to_string()),
        "hooks": hook_status(),
    })
}

/// "installed", "missing" (no file, or not in it) or "unknown" (unreadable).
fn hook_status() -> &'static str {
    let Ok(path) = hooks::user_settings() else {
        return "unknown";
    };
    let mut text = String::new();
    match std::fs::File::open(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return "missing",
        Err(_) => return "unknown",
        Ok(file) => {
            if file.take(MAX_SETTINGS).read_to_string(&mut text).is_err() {
                return "unknown";
            }
        }
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(settings) if hooks::installed(&settings) => "installed",
        Ok(_) => "missing",
        Err(_) => "unknown",
    }
}

/// Claude Code settings files bigger than this aren't read for the status.
const MAX_SETTINGS: u64 = 1024 * 1024;

/// Opens or closes the island when the user clicks it.
#[tauri::command]
fn expand(app: AppHandle, island: State<'_, Island>, open: bool) {
    island.expanded.store(open, Ordering::Relaxed);
    if !open {
        // Closing the island means the away summary was seen.
        island.desk.clear_away();
    }
    show(&app, island.desk.view());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second launch finds the first one answering; a free endpoint has
    /// nobody (the first launch).
    #[test]
    fn a_second_launch_sees_the_first_one() {
        let id = format!("bouncer-app-test-{}-second", std::process::id());
        #[cfg(windows)]
        let path = std::path::PathBuf::from(format!(r"\\.\pipe\{id}"));
        #[cfg(unix)]
        let path = std::env::temp_dir().join(&id).join("bouncer.sock");
        assert!(!another_bouncer_answers(&path));
        let server = Server::bind(&path).unwrap();
        let handler: Handler = Arc::new(|_, _: &dyn Fn() -> bool| None);
        std::thread::spawn(move || server.run(handler));
        assert!(another_bouncer_answers(&path));
        assert!(Server::bind(&path).is_err());
    }

    /// History is wiped only by the `wipe` command, which the page calls from
    /// Settings' armed "Delete history" after its confirm. The tray's item
    /// only opens that confirm; nothing else in the app wipes.
    #[test]
    fn only_the_confirmed_wipe_command_wipes() {
        let src = include_str!("main.rs");
        let code = &src[..src
            .find(
                "#[cfg(test)]
mod tests",
            )
            .unwrap()];
        let calls: Vec<usize> = code
            .match_indices(".wipe_history(")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(calls.len(), 1, "one wipe path");
        let caller = &code[code[..calls[0]]
            .rfind(
                "
fn ",
            )
            .unwrap()..];
        assert!(
            caller.starts_with(
                "
fn wipe("
            ),
            "{}",
            &caller[..40]
        );
        let tray = &code[code
            .find(
                "
fn tray(",
            )
            .unwrap()..];
        let tray = &tray[..tray
            .find(
                "
}
",
            )
            .unwrap()];
        assert!(tray.contains(r#""wipe", "Wipe history…""#));
        assert!(tray.contains("confirm_wipe.store(true"));
    }

    /// The shipped page may run only its own script and style (no inline
    /// code, no eval, nothing remote), and the window may call only our own
    /// commands: no Tauri plugin (files, shell, HTTP) is reachable from it.
    #[test]
    fn strict_csp_and_only_our_commands() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let security = &conf["app"]["security"];
        let csp = &security["csp"];
        for (directive, want) in [
            ("default-src", "'self'"),
            ("script-src", "'self'"),
            ("style-src", "'self'"),
            ("img-src", "'self'"),
            ("connect-src", "ipc: http://ipc.localhost"),
            ("object-src", "'none'"),
            ("base-uri", "'none'"),
            ("form-action", "'none'"),
            ("frame-ancestors", "'none'"),
        ] {
            assert_eq!(csp[directive], want, "{directive}");
        }
        let all = csp.to_string();
        for loose in [
            "unsafe-inline",
            "unsafe-eval",
            "http:",
            "https:",
            "*",
            "data:",
        ] {
            let hits = all.matches(loose).count();
            let allowed = usize::from(loose == "http:"); // only http://ipc.localhost
            assert_eq!(hits, allowed, "csp allows {loose}");
        }
        assert_eq!(security["freezePrototype"], true);
        let caps: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        let ours = [
            "subscribe",
            "decide",
            "always",
            "expand",
            "drag",
            "fit",
            "settings",
            "set-prefs",
            "wipe",
            "about",
            "set-mode",
            "keyboard",
        ];
        let granted: Vec<&str> = caps["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        assert_eq!(granted, ours.map(|c| format!("allow-{c}")));
        assert_eq!(caps["windows"], serde_json::json!(["main"]));
    }

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
    fn release_builds_drop_webview_browser_arguments() {
        // SAFETY: no other test in this binary reads or writes the environment.
        unsafe { std::env::set_var(WEBVIEW2_ARGS, "--remote-debugging-port=9222") };
        clear_webview_args(false);
        assert!(std::env::var_os(WEBVIEW2_ARGS).is_some(), "debug keeps it");
        clear_webview_args(true);
        assert!(
            std::env::var_os(WEBVIEW2_ARGS).is_none(),
            "release clears it"
        );
    }

    #[test]
    fn greyed_icon_is_grey_and_fainter() {
        let out = greyed(&[255, 0, 0, 255, 10, 200, 30, 128]);
        assert_eq!(out, [76, 76, 76, 127, 126, 126, 126, 64]);
    }
}
