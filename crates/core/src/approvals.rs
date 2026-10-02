//! The approval desk: live sessions and the queue of permission requests
//! waiting for the user.
//!
//! Each queued request gets a random, single-use ID. A decision must name that
//! ID; it's removed when answered, timed out or released by a pause. Every
//! path that isn't an explicit decision answers nothing, so Claude Code asks in
//! the terminal.

use std::collections::VecDeque;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::event::Event;
use crate::ipc::Decision;

/// How long a request waits for the user: well under the relay's budget, so
/// the relay is still listening when the app gives up.
pub const WAIT: Duration =
    Duration::from_secs(bouncer_relay::DECISION_BUDGET.as_secs().saturating_sub(10));
/// Allow is refused this soon after a request is queued.
pub const ARM: Duration = Duration::from_millis(600);

struct Session {
    id: String,
    agent: &'static str,
    project: String,
    tool: Option<String>,
    status: &'static str,
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
    issued: u64,
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
                    tool: None,
                    status: "idle",
                });
                self.sessions.len() - 1
            }
        };
        &mut self.sessions[i]
    }

    fn track(&mut self, event: &Event) {
        if event.kind == "SessionEnd" {
            self.sessions.retain(|s| s.id != event.session);
            return;
        }
        let session = self.session(event);
        session.project.clone_from(&event.project);
        if event.tool.is_some() {
            session.tool.clone_from(&event.tool);
        }
        session.status = match event.kind.as_str() {
            "PermissionRequest" => "needs you",
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => "working",
            "Stop" => "idle",
            _ => session.status,
        };
    }

    /// Removes a request from the queue; its session goes back to "working".
    fn take(&mut self, id: &str) -> Option<Pending> {
        let i = self.queue.iter().position(|p| p.id == id)?;
        let pending = self.queue.remove(i)?;
        if let Some(s) = self
            .sessions
            .iter_mut()
            .find(|s| s.id == pending.event.session)
        {
            s.status = "working";
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
            if event.is_permission_request() && !state.paused {
                state.issued += 1;
                let id = request_id(state.issued);
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
        if answer.is_none() && self.lock().take(&id).is_some() {
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
            state
                .take(id)
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
                    state.take(&id); // dropping the sender wakes the waiter with nothing
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
                    "tool": s.tool.as_deref().map(visible),
                    "status": s.status,
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
                })
            })
            .collect();
        json!({ "paused": state.paused, "sessions": sessions, "queue": queue })
    }
}

/// 128 random-looking bits: two hashes of the counter under OS-seeded SipHash
/// keys (`RandomState`). Unique per counter, unguessable without the keys.
fn request_id(counter: u64) -> String {
    let hash = |part: u8| RandomState::new().hash_one((counter, part));
    format!("{:016x}{:016x}", hash(0), hash(1))
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
fn hidden(c: char) -> bool {
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
        assert_eq!(desk.view()["sessions"][0]["status"], "working");
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
    fn request_ids_are_unique_128_bit_hex() {
        let ids: std::collections::HashSet<String> = (0..1000).map(request_id).collect();
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
