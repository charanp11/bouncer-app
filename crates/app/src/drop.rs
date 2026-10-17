//! The island's side of a file drop (roadmap d; docs/PLAN.md, "Phase 7
//! audit"). Tauri hands the backend the dropped paths; the page only ever
//! hears "something is over me", "checking <name>", a refusal or the
//! report, never a path. The report is held in memory under a random ID
//! for `HOLD`; the page asks about it by that ID only, and nothing about
//! the file goes anywhere until the user presses Send on the exact text.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bouncer_core::approvals;
use bouncer_core::drop::{self, HOLD, Report};
use serde_json::{Value, json};
use tauri::{AppHandle, DragDropEvent, Manager, State};

use crate::chat::{self, Chats};

#[derive(Default)]
pub struct Drops {
    /// What the island shows: null, checking, refused or a report.
    view: Mutex<Value>,
    /// While something is dragged over the island: `kind` "one" or
    /// "many", and where it came in (`x`, `y`: the page's physical pixels),
    /// so Bouncer looks that way.
    hover: Mutex<Value>,
    /// The shown report, its random ID and when it came.
    held: Mutex<Option<(String, Report, Instant)>>,
    /// Numbers drops: an older check finishing never replaces a newer drop.
    seq: AtomicU64,
    /// Numbers drag enters and leaves: a leave takes the hover away only if
    /// nothing came after it (see `LEAVE_GRACE`).
    moves: AtomicU64,
}

/// A leave counts only if no enter follows within this. Growing the island
/// for the catch target reshapes the window under the cursor, and Windows
/// reports a leave and an enter at once: without the grace the island would
/// flick back to the pill and the catch would start over.
const LEAVE_GRACE: Duration = Duration::from_millis(150);

impl Drops {
    /// What the page's view carries: `drop` and `dropHover`.
    pub fn view(&self) -> (Value, Value) {
        let view = self.view.lock().unwrap().clone();
        let hover = self.hover.lock().unwrap().clone();
        (view, hover)
    }

    /// The held report, if `id` is its ID.
    pub fn report(&self, id: &str) -> Option<Report> {
        match self.held.lock().unwrap().as_ref() {
            Some((held, report, _)) if held == id => Some(report.clone()),
            _ => None,
        }
    }

    /// Forgets the shown drop.
    fn clear(&self) {
        self.seq.fetch_add(1, Ordering::SeqCst);
        *self.held.lock().unwrap() = None;
        *self.view.lock().unwrap() = Value::Null;
    }

    /// Forgets a report held longer than `HOLD`; true if it did.
    pub fn expire(&self) -> bool {
        let old = self
            .held
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|(_, _, at)| at.elapsed() >= HOLD);
        if old {
            self.clear();
        }
        old
    }
}

/// The window's drag-and-drop events (Tauri's handler, `dragDropEnabled`).
pub fn on_drag(app: &AppHandle, event: &DragDropEvent) {
    let Some(drops) = app.try_state::<Drops>() else {
        return;
    };
    match event {
        DragDropEvent::Enter { paths, position } => {
            drops.moves.fetch_add(1, Ordering::SeqCst);
            let kind = if paths.len() == 1 { "one" } else { "many" };
            let hover = json!({"kind": kind, "x": position.x, "y": position.y});
            let mut now = drops.hover.lock().unwrap();
            // Back within the grace: still the same hover, nothing to redraw.
            if now["kind"] == kind {
                return;
            }
            *now = hover;
        }
        DragDropEvent::Leave => {
            let n = drops.moves.fetch_add(1, Ordering::SeqCst) + 1;
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(LEAVE_GRACE);
                let drops = app.state::<Drops>();
                if drops.moves.load(Ordering::SeqCst) == n {
                    *drops.hover.lock().unwrap() = Value::Null;
                    crate::refresh(&app);
                }
            });
            return;
        }
        DragDropEvent::Drop { paths, .. } => {
            drops.moves.fetch_add(1, Ordering::SeqCst);
            *drops.hover.lock().unwrap() = Value::Null;
            dropped(app, &drops, paths.clone());
        }
        _ => return,
    }
    crate::refresh(app);
}

/// A drop: one file is checked on a worker (the report replaces "checking"
/// unless another drop came first); anything else is refused at once.
fn dropped(app: &AppHandle, drops: &Drops, paths: Vec<PathBuf>) {
    if let Some(chats) = app.try_state::<Chats>() {
        chats.stop_drop();
    }
    drops.clear();
    // Every drop's views carry its number: the page plays its gulp or
    // "nope" once per drop, not on every redraw.
    let n = drops.seq.load(Ordering::SeqCst);
    let [path] = paths.as_slice() else {
        *drops.view.lock().unwrap() =
            json!({"state": "refused", "text": drop::ONE_AT_A_TIME, "n": n});
        return;
    };
    let name = drop::name_of(path);
    *drops.view.lock().unwrap() = json!({"state": "checking", "name": name, "n": n});
    let path = path.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let result = drop::check(&path);
        let drops = app.state::<Drops>();
        let id = approvals::request_id();
        // The lock is held while checking the number, so a drop or Close
        // in between can't be overwritten.
        let mut held = drops.held.lock().unwrap();
        if drops.seq.load(Ordering::SeqCst) != n {
            return;
        }
        let mut view = match (result, id) {
            (Ok(report), Some(id)) => {
                let mut view = report.to_json();
                view["state"] = "report".into();
                view["id"] = id.clone().into();
                *held = Some((id, report, Instant::now()));
                view
            }
            (Err(reason), _) => json!({"state": "refused", "name": name, "text": reason}),
            (Ok(_), None) => json!({"state": "refused", "name": name, "text": drop::UNREADABLE}),
        };
        view["n"] = n.into();
        *drops.view.lock().unwrap() = view;
        drop(held);
        crate::refresh(&app);
    });
}

/// Close (×, Esc): the report and anything held go, a file answer stops.
#[tauri::command]
pub fn drop_close(app: AppHandle, drops: State<'_, Drops>, chats: State<'_, Chats>) {
    chats.stop_drop();
    drops.clear();
    crate::refresh(&app);
}

/// The exact text "Send to Claude" would send: "Is this safe to run?"
/// (`ask` false), or "Ask about it" before the question (`ask` true; what's
/// sent is this followed by the question). `gone` if the drop isn't held.
#[tauri::command]
pub fn drop_preview(drops: State<'_, Drops>, id: String, ask: bool) -> Result<String, String> {
    let report = drops.report(&id).ok_or("gone")?;
    Ok(if ask {
        drop::ask_prompt(&report, "")
    } else {
        drop::file_prompt(&report)
    })
}

/// "Send to Claude" after the preview: the held report's prompt (never text
/// from the page) through the locked chat, one run at a time.
#[tauri::command(async)]
pub fn drop_ask(
    app: AppHandle,
    drops: State<'_, Drops>,
    chats: State<'_, Chats>,
    id: String,
    model: String,
) -> Result<(), String> {
    let report = drops.report(&id).ok_or("gone")?;
    chat::ask_drop(&app, &chats, id, drop::file_prompt(&report), &model)
}

/// Stop on the file answer.
#[tauri::command]
pub fn drop_stop(chats: State<'_, Chats>) {
    chats.stop_drop();
}
