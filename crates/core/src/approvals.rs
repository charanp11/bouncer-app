//! The approval desk: live sessions and the queue of permission requests
//! waiting for the user.
//!
//! Each queued request gets a random, single-use ID. A decision must name that
//! ID; it's removed when answered, timed out or released by a pause. Every
//! path that isn't an explicit decision answers nothing, so Claude Code asks in
//! the terminal.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::code::code_view;
use crate::event::Event;
use crate::ipc::Decision;

/// How long a request waits for the user: well under the relay's budget, so
/// the relay is still listening when the app gives up.
pub const WAIT: Duration =
    Duration::from_secs(bouncer_relay::DECISION_BUDGET.as_secs().saturating_sub(10));
/// Allow is refused this soon after a request is queued.
pub const ARM: Duration = Duration::from_millis(600);

/// Status of a session whose request went to Claude Code's own prompt.
const IN_TERMINAL: &str = "asks in terminal";
/// Steps kept per session for the detail view (it groups repeats and shows
/// the latest few).
const HISTORY: usize = 40;

struct Session {
    id: String,
    agent: &'static str,
    project: String,
    /// What it's doing right now, e.g. "Editing main.rs".
    step: String,
    status: &'static str,
    started: SystemTime,
    last: SystemTime,
    /// Recent steps, oldest first, each with who let it through ("" = Claude
    /// Code's own rules).
    history: VecDeque<(String, &'static str)>,
    /// The code pane for the current tool call, if it has one.
    code: Option<Value>,
}

struct Pending {
    id: String,
    event: Event,
    issued: Instant,
    answer: Sender<Decision>,
}

#[derive(Default)]
struct State {
    sessions: Vec<Session>,
    queue: VecDeque<Pending>,
    paused: bool,
}

impl State {
    fn session(&mut self, event: &Event) -> &mut Session {
        let i = match self.sessions.iter().position(|s| s.id == event.session) {
            Some(i) => i,
            None => {
                self.sessions.push(Session {
                    id: event.session.clone(),
                    agent: event.agent,
                    project: event.project.clone(),
                    step: "Idle".into(),
                    status: "idle",
                    started: SystemTime::now(),
                    last: SystemTime::now(),
                    history: VecDeque::new(),
                    code: None,
                });
                self.sessions.len() - 1
            }
        };
        &mut self.sessions[i]
    }

    fn track(&mut self, event: &Event) {
        let paused = self.paused;
        if event.kind == "SessionEnd" {
            self.sessions.retain(|s| s.id != event.session);
            return;
        }
        let session = self.session(event);
        session.project.clone_from(&event.project);
        session.last = SystemTime::now();
        match event.kind.as_str() {
            "PreToolUse" | "PermissionRequest" => {
                session.step = step(event);
                session.code = code_view(event.tool.as_deref(), event.input.as_ref());
                let how = match event.kind.as_str() {
                    "PreToolUse" => "",
                    _ if paused => "asked in terminal",
                    _ => "waiting for you",
                };
                // A request follows the PreToolUse of the same call: mark it, don't repeat it.
                match session.history.back_mut() {
                    Some(last) if !how.is_empty() && last.0 == session.step => last.1 = how,
                    _ => {
                        session.history.push_back((session.step.clone(), how));
                        if session.history.len() > HISTORY {
                            session.history.pop_front();
                        }
                    }
                }
            }
            "UserPromptSubmit" => session.step = "Thinking".into(),
            "Stop" => session.step = "Idle".into(),
            _ => {}
        }
        session.status = match event.kind.as_str() {
            "PermissionRequest" if paused => IN_TERMINAL,
            "PermissionRequest" => "needs you",
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => "working",
            "Stop" => "idle",
            _ => session.status,
        };
    }

    /// Removes a request from the queue, gives its session `status`, and notes
    /// `how` it ended on the session's latest step.
    fn take(&mut self, id: &str, status: &'static str, how: &'static str) -> Option<Pending> {
        let i = self.queue.iter().position(|p| p.id == id)?;
        let pending = self.queue.remove(i)?;
        if let Some(s) = self
            .sessions
            .iter_mut()
            .find(|s| s.id == pending.event.session)
        {
            s.status = status;
            if let Some(last) = s.history.back_mut() {
                last.1 = how;
            }
        }
        Some(pending)
    }
}

pub struct Desk {
    state: Mutex<State>,
    wait: Duration,
    changed: Box<dyn Fn(Value) + Send + Sync>,
}

impl Desk {
    /// `changed` gets the new view after every change, outside the lock.
    pub fn new(wait: Duration, changed: impl Fn(Value) + Send + Sync + 'static) -> Desk {
        Desk {
            state: Mutex::default(),
            wait,
            changed: Box::new(changed),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn publish(&self) {
        (self.changed)(self.view());
    }

    /// Records the event. A permission request is queued and this blocks until
    /// the user decides, the wait runs out, or Bouncer is paused; only a
    /// decision returns `Some`.
    pub fn handle(&self, event: Event) -> Option<Decision> {
        let (tx, rx) = mpsc::channel();
        let id = {
            let mut state = self.lock();
            state.track(&event);
            // No ID (the OS random source failed) means no card: the terminal asks.
            if event.is_permission_request()
                && !state.paused
                && let Some(id) = request_id()
            {
                state.queue.push_back(Pending {
                    id: id.clone(),
                    event,
                    issued: Instant::now(),
                    answer: tx,
                });
                Some(id)
            } else {
                None
            }
        };
        self.publish();
        let id = id?;
        let answer = rx.recv_timeout(self.wait).ok();
        if answer.is_none()
            && self
                .lock()
                .take(&id, IN_TERMINAL, "asked in terminal")
                .is_some()
        {
            self.publish();
        }
        answer
    }

    /// Answers the request with this ID, once. Allow is refused within `ARM`
    /// of the request arriving.
    pub fn decide(&self, id: &str, allow: bool) -> Result<(), &'static str> {
        let pending = {
            let mut state = self.lock();
            let early = state
                .queue
                .iter()
                .find(|p| p.id == id)
                .ok_or("unknown or already answered request")?
                .issued
                .elapsed()
                < ARM;
            if allow && early {
                return Err("too soon to allow");
            }
            let how = if allow { "you allowed" } else { "you denied" };
            state
                .take(id, "working", how)
                .ok_or("unknown or already answered request")?
        };
        let _ = pending.answer.send(if allow {
            Decision::Allow
        } else {
            Decision::Deny
        });
        self.publish();
        Ok(())
    }

    /// While paused nothing is queued, and pausing releases every queued
    /// request unanswered, so they all go to the terminal.
    pub fn set_paused(&self, paused: bool) {
        {
            let mut state = self.lock();
            state.paused = paused;
            if paused {
                let ids: Vec<String> = state.queue.iter().map(|p| p.id.clone()).collect();
                for id in ids {
                    state.take(&id, IN_TERMINAL, "asked in terminal"); // dropping the sender wakes the waiter with nothing
                }
            }
        }
        self.publish();
    }

    pub fn paused(&self) -> bool {
        self.lock().paused
    }

    /// What the island shows. Agent text is made visible-safe here.
    pub fn view(&self) -> Value {
        let state = self.lock();
        let sessions: Vec<Value> = state
            .sessions
            .iter()
            .map(|s| {
                json!({
                    "id": visible(&s.id),
                    "agent": s.agent,
                    "project": visible(&s.project),
                    "step": visible(&s.step),
                    "status": s.status,
                    "started_ms": epoch_ms(s.started),
                    "last_ms": epoch_ms(s.last),
                    "history": s.history.iter().map(|(label, how)| json!({ "label": visible(label), "how": how })).collect::<Vec<_>>(),
                    "code": s.code,
                })
            })
            .collect();
        let queue: Vec<Value> = state
            .queue
            .iter()
            .map(|p| {
                json!({
                    "id": p.id,
                    "agent": p.event.agent,
                    "session": visible(&p.event.session),
                    "project": visible(&p.event.project),
                    "tool": p.event.tool.as_deref().map(visible),
                    "text": visible(&request_text(&p.event)),
                    "code": code_view(p.event.tool.as_deref(), p.event.input.as_ref()),
                })
            })
            .collect();
        json!({ "paused": state.paused, "sessions": sessions, "queue": queue })
    }
}

fn epoch_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// 128 bits from the OS random source, as hex. `None` if it fails.
fn request_id() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// A short, human description of a tool call for the session list (the
/// approval card always shows the full input).
fn step(event: &Event) -> String {
    let input = event.input.as_ref();
    let text = |key: &str| input.and_then(|i| i.get(key)).and_then(Value::as_str);
    let file = |key: &str| {
        let path = text(key).unwrap_or_default();
        path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
    };
    match event.tool.as_deref() {
        Some("Edit" | "MultiEdit" | "Write") => format!("Editing {}", file("file_path")),
        Some("NotebookEdit") => format!("Editing {}", file("notebook_path")),
        Some("Read") => format!("Reading {}", file("file_path")),
        Some("Bash") => {
            let command = text("command").unwrap_or_default();
            format!("Running {}", command.lines().next().unwrap_or_default())
        }
        Some("Grep" | "Glob") => "Searching".into(),
        Some("WebFetch" | "WebSearch") => "Browsing the web".into(),
        Some("Task" | "Agent") => "Running a subagent".into(),
        Some(tool) => format!("Using {tool}"),
        None => "Working".into(),
    }
}

/// The full thing being approved: a Bash command as is, any other tool's input
/// as pretty JSON. Never shortened.
fn request_text(event: &Event) -> String {
    let Some(input) = &event.input else {
        return String::new();
    };
    match input.get("command").and_then(Value::as_str) {
        Some(command) if event.tool.as_deref() == Some("Bash") => command.to_owned(),
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}

/// Characters that are invisible or reorder text when displayed: controls
/// (ANSI escapes), bidi controls, zero-width characters, tag characters.
pub(crate) fn hidden(c: char) -> bool {
    (c.is_control() && c != '\n' && c != '\t')
        || matches!(c,
            '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}'
            | '\u{E0000}'..='\u{E007F}')
}

/// Shows hidden characters as `\u{XXXX}` so a command can't conceal or
/// reorder what the user approves. Markup is left alone: the island renders
/// with `textContent`, so `<script>` is just text.
pub fn visible(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if hidden(c) {
            out += &format!("\\u{{{:04X}}}", c as u32);
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::SystemTime;

    fn event(session: &str, kind: &str, command: &str) -> Event {
        Event {
            agent: "claude-code",
            session: session.into(),
            project: "/p".into(),
            kind: kind.into(),
            tool: Some("Bash".into()),
            input: Some(json!({ "command": command })),
            time: SystemTime::now(),
        }
    }

    fn desk(wait: Duration) -> Arc<Desk> {
        Arc::new(Desk::new(wait, |_| {}))
    }

    fn ask(desk: &Arc<Desk>, session: &str, command: &str) -> JoinHandle<Option<Decision>> {
        let (desk, e) = (desk.clone(), event(session, "PermissionRequest", command));
        std::thread::spawn(move || desk.handle(e))
    }

    /// Waits until `n` requests are queued and returns their IDs in order.
    fn queued(desk: &Desk, n: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let view = desk.view();
            let queue = view["queue"].as_array().unwrap();
            if queue.len() == n || Instant::now() > deadline {
                return queue
                    .iter()
                    .map(|q| q["id"].as_str().unwrap().to_owned())
                    .collect();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn queue_is_fifo_and_ids_are_single_use() {
        let desk = desk(WAIT);
        let a = ask(&desk, "a", "ls");
        queued(&desk, 1);
        let b = ask(&desk, "b", "pwd");
        let ids = queued(&desk, 2);
        assert_eq!(desk.view()["queue"][0]["text"], "ls");
        assert_eq!(desk.view()["queue"][1]["text"], "pwd");

        assert!(desk.decide("not-an-id", false).is_err());
        desk.decide(&ids[0], false).unwrap();
        assert_eq!(a.join().unwrap(), Some(Decision::Deny));
        assert!(desk.decide(&ids[0], false).is_err(), "replayed");

        std::thread::sleep(ARM);
        desk.decide(&ids[1], true).unwrap();
        assert_eq!(b.join().unwrap(), Some(Decision::Allow));
        assert!(desk.decide(&ids[1], true).is_err(), "replayed");
        assert_eq!(queued(&desk, 0).len(), 0);
    }

    #[test]
    fn allow_is_refused_before_the_arm_delay() {
        let desk = desk(WAIT);
        let a = ask(&desk, "a", "ls");
        let id = queued(&desk, 1).remove(0);
        assert_eq!(desk.decide(&id, true), Err("too soon to allow"));
        std::thread::sleep(ARM);
        desk.decide(&id, true).unwrap();
        assert_eq!(a.join().unwrap(), Some(Decision::Allow));
    }

    #[test]
    fn unanswered_requests_time_out_with_no_answer() {
        let desk = desk(Duration::from_millis(200));
        let a = ask(&desk, "a", "ls");
        assert_eq!(a.join().unwrap(), None);
        assert_eq!(queued(&desk, 0).len(), 0);
        assert_eq!(desk.view()["sessions"][0]["status"], IN_TERMINAL);
    }

    #[test]
    fn pause_releases_the_queue_and_queues_nothing() {
        let desk = desk(WAIT);
        let (a, b) = (ask(&desk, "a", "ls"), ask(&desk, "b", "pwd"));
        let ids = queued(&desk, 2);
        desk.set_paused(true);
        assert_eq!((a.join().unwrap(), b.join().unwrap()), (None, None));
        assert!(desk.decide(&ids[0], false).is_err());
        assert_eq!(ask(&desk, "c", "ls").join().unwrap(), None);
        let view = desk.view();
        let statuses: Vec<_> = view["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| &s["status"])
            .collect();
        assert!(statuses.iter().all(|s| *s == IN_TERMINAL), "{statuses:?}");
        assert_eq!(desk.view()["paused"], true);
        desk.set_paused(false);
        let d = ask(&desk, "d", "ls");
        let id = queued(&desk, 1).remove(0);
        desk.decide(&id, false).unwrap();
        assert_eq!(d.join().unwrap(), Some(Decision::Deny));
    }

    #[test]
    fn sessions_follow_events() {
        let desk = desk(WAIT);
        for (session, kind) in [
            ("a", "SessionStart"),
            ("b", "PreToolUse"),
            ("c", "UserPromptSubmit"),
            ("a", "Stop"),
            ("c", "SessionEnd"),
        ] {
            assert_eq!(desk.handle(event(session, kind, "ls")), None);
        }
        let view = desk.view();
        let sessions = view["sessions"].as_array().unwrap();
        let status: Vec<_> = sessions.iter().map(|s| (&s["id"], &s["status"])).collect();
        assert_eq!(
            status,
            [
                (&json!("a"), &json!("idle")),
                (&json!("b"), &json!("working"))
            ]
        );
        assert_eq!(view["queue"], json!([]));
    }

    #[test]
    fn session_steps_describe_the_current_call() {
        let desk = desk(WAIT);
        let mut e = event("a", "PreToolUse", "cargo test\nsecond line");
        desk.handle(e.clone());
        assert_eq!(desk.view()["sessions"][0]["step"], "Running cargo test");
        e.tool = Some("Edit".into());
        e.input = Some(json!({ "file_path": r"C:\Users\chara\Desktop\Full Time\x\src\main.rs" }));
        desk.handle(e.clone());
        assert_eq!(desk.view()["sessions"][0]["step"], "Editing main.rs");
        e.tool = Some("Read".into());
        e.input = Some(json!({ "file_path": "/p/README.md" }));
        desk.handle(e);
        assert_eq!(desk.view()["sessions"][0]["step"], "Reading README.md");
        desk.handle(event("a", "Stop", ""));
        assert_eq!(desk.view()["sessions"][0]["step"], "Idle");
        desk.handle(event("a", "UserPromptSubmit", ""));
        assert_eq!(desk.view()["sessions"][0]["step"], "Thinking");
    }

    #[test]
    fn history_records_who_let_each_step_through() {
        let desk = desk(WAIT);
        desk.handle(event("a", "PreToolUse", "ls"));
        desk.handle(event("a", "PreToolUse", "cargo test"));
        let ask = ask(&desk, "a", "cargo test");
        let id = queued(&desk, 1).remove(0);
        let history = |d: &Desk| d.view()["sessions"][0]["history"].clone();
        assert_eq!(
            history(&desk),
            json!([
                { "label": "Running ls", "how": "" },
                { "label": "Running cargo test", "how": "waiting for you" }
            ])
        );
        desk.decide(&id, false).unwrap();
        ask.join().unwrap();
        assert_eq!(history(&desk)[1]["how"], "you denied");
        for i in 0..HISTORY + 5 {
            desk.handle(event("a", "PreToolUse", &format!("echo {i}")));
        }
        assert_eq!(history(&desk).as_array().unwrap().len(), HISTORY);
        let view = desk.view();
        let s = &view["sessions"][0];
        assert_eq!(s["code"]["file"], "command");
        assert!(s["last_ms"].as_u64() >= s["started_ms"].as_u64());
    }

    #[test]
    fn request_ids_are_unique_128_bit_hex() {
        let ids: std::collections::HashSet<String> =
            (0..1000).map(|_| request_id().unwrap()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(
            ids.iter()
                .all(|id| id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit()))
        );
    }

    #[test]
    fn hostile_text_becomes_visible() {
        assert_eq!(
            visible("<script>alert(1)</script>"),
            "<script>alert(1)</script>"
        );
        assert_eq!(
            visible("\x1b[31mrm -rf /\x1b[0m"),
            r"\u{001B}[31mrm -rf /\u{001B}[0m"
        );
        assert_eq!(visible("ls \u{202E}txt.exe"), r"ls \u{202E}txt.exe");
        assert_eq!(visible("rm\u{200B} -rf"), r"rm\u{200B} -rf");
        assert_eq!(visible("a\rb\u{7}\u{85}"), r"a\u{000D}b\u{0007}\u{0085}");
        assert_eq!(visible("tag\u{E0041}"), r"tag\u{E0041}");
        assert_eq!(
            visible("line 1\n\tline 2 é 日本"),
            "line 1\n\tline 2 é 日本"
        );
    }

    #[test]
    fn request_text_is_whole() {
        let mut e = event("a", "PermissionRequest", &"x".repeat(100_000));
        assert_eq!(request_text(&e).len(), 100_000);
        e.tool = Some("Write".into());
        e.input = Some(json!({ "file_path": "/p/a", "content": "hi" }));
        assert_eq!(
            request_text(&e),
            "{\n  \"file_path\": \"/p/a\",\n  \"content\": \"hi\"\n}"
        );
    }
}
