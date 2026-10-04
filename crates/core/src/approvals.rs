//! The approval desk: live sessions and the queue of permission requests
//! waiting for the user.
//!
//! Each queued request gets a random, single-use ID. A decision must name that
//! ID; it's removed when answered, timed out or released by a pause. Every
//! path that isn't an explicit decision answers nothing, so Claude Code asks in
//! the terminal.
//!
//! The rules (`rules.toml`) are checked first: in auto mode a request a rule
//! allows is answered at once; otherwise the card carries the risk reason,
//! what the rules would do, and the rule "Always allow" would add.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::activity::{self, Entry, Log};
use crate::away::{self, Summary};
use crate::check::{self, Context, sentence};
use crate::code::code_view;
use crate::event::Event;
use crate::ipc::Decision;
use crate::rules::{self, Loaded, Mode, Rule, Rules};

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
/// How a step that a rule answered is tagged.
const BY_RULE: &str = "auto-allowed by rule";
/// Tools whose "permission request" is really a question for the user (pick
/// an option, approve a plan). Bouncer never answers them; the terminal does.
const QUESTIONS: &[&str] = &["AskUserQuestion", "ExitPlanMode"];
/// What a session waiting on one of those shows.
const QUESTION_STEP: &str = "question in the terminal";
/// A session with no event for this long is dropped (it may never send
/// `SessionEnd`: crashed, killed, closed terminal). Any later event brings it back.
const IDLE_LIMIT: Duration = Duration::from_secs(30 * 60);
/// Same, for a session waiting on the user: kept longer, so the away
/// summary can still point at it.
const WAITING_LIMIT: Duration = Duration::from_secs(4 * 60 * 60);

struct Session {
    id: String,
    agent: &'static str,
    /// The folder the session was first seen in: what "inside the project" means.
    root: String,
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
    /// Why it's risky, as a sentence.
    risk: Option<String>,
    /// The rules that would allow it (observe mode).
    would: Option<String>,
    /// What "Always allow" adds.
    offer: Option<Rule>,
}

/// The rules in use and where they came from.
struct RuleFile {
    path: Option<PathBuf>,
    loaded: Loaded,
    stamp: Option<(SystemTime, u64)>,
    /// The latest change, until the user dismisses it in the island.
    notice: Option<(u64, String)>,
}

/// A short message the pill shows for a moment.
struct Flash {
    n: u64,
    at: SystemTime,
    strong: &'static str,
    rest: String,
    badge: &'static str,
    risk: bool,
}

struct State {
    sessions: Vec<Session>,
    queue: VecDeque<Pending>,
    paused: bool,
    rules: RuleFile,
    flash: Option<Flash>,
    /// Numbers flashes and notices, so the island can tell them apart.
    count: u64,
    /// The activity log; `None` without a rules folder (tests, no config dir).
    log: Option<Log>,
    /// "While you were away", until the island closes it.
    away: Option<Summary>,
}

impl State {
    /// Logs how a permission request ended.
    fn answered(&mut self, event: &Event, how: &str) {
        if let Some(log) = self.log.as_mut() {
            log.write(&Entry::answer(event, how, activity::ms(SystemTime::now())));
        }
    }

    fn session(&mut self, event: &Event) -> &mut Session {
        let i = match self.sessions.iter().position(|s| s.id == event.session) {
            Some(i) => i,
            None => {
                self.sessions.push(Session {
                    id: event.session.clone(),
                    agent: event.agent,
                    root: event.project.clone(),
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

    /// Drops sessions quiet for longer than their limit, except any with a
    /// card in the queue. True if any went.
    fn expire(&mut self, now: SystemTime) -> bool {
        let before = self.sessions.len();
        let queue = &self.queue;
        self.sessions.retain(|s| {
            let limit = match s.status {
                "needs you" | IN_TERMINAL => WAITING_LIMIT,
                _ => IDLE_LIMIT,
            };
            let quiet = now.duration_since(s.last).unwrap_or_default();
            quiet < limit || queue.iter().any(|p| p.event.session == s.id)
        });
        self.sessions.len() != before
    }

    /// Removes a request from the queue, gives its session `status`, and notes
    /// `how` it ended on the session's latest step.
    fn take(&mut self, id: &str, status: &'static str, how: &'static str) -> Option<Pending> {
        let i = self.queue.iter().position(|p| p.id == id)?;
        let pending = self.queue.remove(i)?;
        self.answered(&pending.event, how);
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

    /// A request answered in Claude Code's own prompt while its card is still
    /// up: the tool ran (its PostToolUse, same tool and input), or the turn
    /// moved on (Stop, a new prompt, the session ended). The card is stale: it
    /// leaves, unanswered (Claude Code already has its answer). Matching the
    /// exact tool and input keeps a card up while a subagent runs another tool.
    fn drop_stale(&mut self, event: &Event) {
        let stale: Vec<String> = self
            .queue
            .iter()
            .filter(|p| {
                p.event.session == event.session
                    && match event.kind.as_str() {
                        "PostToolUse" => p.event.tool == event.tool && p.event.input == event.input,
                        "Stop" | "UserPromptSubmit" | "SessionEnd" => true,
                        _ => false,
                    }
            })
            .map(|p| p.id.clone())
            .collect();
        let status = self
            .sessions
            .iter()
            .find(|s| s.id == event.session)
            .map_or("idle", |s| s.status);
        for id in stale {
            self.take(&id, status, "answered in terminal");
        }
    }

    /// A question Claude Code asks in the terminal: the session needs the user.
    fn waiting_in_terminal(&mut self, event: &Event) {
        self.answered(event, "asked in terminal");
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == event.session) {
            s.status = "needs you";
            s.step = QUESTION_STEP.into();
            if let Some(last) = s.history.back_mut() {
                last.1 = "asked in terminal";
            }
        }
    }

    /// A rule answered this request: tag the step and flash the pill.
    fn auto_allowed(&mut self, event: &Event) {
        self.answered(event, BY_RULE);
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == event.session) {
            s.status = "working";
            if let Some(last) = s.history.back_mut() {
                last.1 = BY_RULE;
            }
        }
        self.flash_allowed(event);
    }

    /// The pill's "Auto-allowed · cargo test in bouncer-app".
    fn flash_allowed(&mut self, event: &Event) {
        let what = match (event.tool.as_deref(), event.input.as_ref()) {
            (Some("Bash" | "PowerShell"), Some(input)) => input
                .get("command")
                .and_then(Value::as_str)
                .and_then(|c| c.lines().next())
                .unwrap_or_default()
                .to_owned(),
            _ => step(event),
        };
        let folder = Path::new(&event.project)
            .file_name()
            .map_or(event.project.clone(), |f| f.to_string_lossy().into_owned());
        self.flash(
            "Auto-allowed",
            format!(" · {what} in {folder}"),
            "rule",
            false,
        );
    }

    /// What the rules say about this request, in its session's project.
    fn check(&self, event: &Event) -> check::Verdict {
        let root = self
            .sessions
            .iter()
            .find(|s| s.id == event.session)
            .map_or(event.project.as_str(), |s| s.root.as_str());
        let home = std::env::home_dir();
        let rules_file = self.rules.path.as_deref();
        let ctx = Context {
            root: Path::new(root),
            cwd: Path::new(&event.project),
            home: home.as_deref(),
            rules_file,
        };
        check::check(
            &self.rules.loaded.rules,
            event.tool.as_deref(),
            event.input.as_ref(),
            &ctx,
        )
    }

    fn flash(&mut self, strong: &'static str, rest: String, badge: &'static str, risk: bool) {
        self.count += 1;
        self.flash = Some(Flash {
            n: self.count,
            at: SystemTime::now(),
            strong,
            rest,
            badge,
            risk,
        });
    }

    /// Re-reads the rules file if it changed (or always, with `force`), and
    /// tells the island what changed. True if anything did.
    fn reload(&mut self, force: bool) -> bool {
        let Some(path) = self.rules.path.clone() else {
            return false;
        };
        let stamp = rules::stamp(&path);
        if !force && stamp.is_some() && stamp == self.rules.stamp {
            return false;
        }
        let loaded = rules::load(&path);
        self.rules.stamp = rules::stamp(&path);
        let old = std::mem::replace(&mut self.rules.loaded, loaded);
        let new = &self.rules.loaded;
        if new.error.is_some() && new.error != old.error {
            self.flash(
                "Rules file ignored",
                " · using the built-in rules in observe mode".into(),
                "rules",
                true,
            );
            return true;
        }
        let Some(change) = changes(&old.rules, &new.rules) else {
            return old.error != new.error;
        };
        self.flash("Rules changed", format!(" · {change}"), "rules", false);
        self.rules.notice = Some((self.count, format!("Rules changed: {change}")));
        true
    }
}

/// What changed between two rule sets, e.g. "mode auto · added cargo test".
fn changes(old: &Rules, new: &Rules) -> Option<String> {
    let mut parts = Vec::new();
    if old.mode != new.mode {
        parts.push(match new.mode {
            Mode::Auto => "mode auto".to_owned(),
            Mode::Observe => "mode observe".to_owned(),
        });
    }
    let missing = |a: &Rules, b: &Rules| -> Vec<String> {
        a.allow
            .iter()
            .filter(|r| !b.allow.contains(r))
            .map(Rule::label)
            .collect()
    };
    let (added, removed) = (missing(new, old), missing(old, new));
    if !added.is_empty() {
        parts.push(format!("added {}", added.join(", ")));
    }
    if !removed.is_empty() {
        parts.push(format!("removed {}", removed.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

pub struct Desk {
    state: Mutex<State>,
    wait: Duration,
    changed: Box<dyn Fn(Value) + Send + Sync>,
}

impl Desk {
    /// `rules` is the rules file (created with defaults if missing); `None`
    /// uses the built-in rules in observe mode. `changed` gets the new view
    /// after every change, outside the lock.
    pub fn new(
        wait: Duration,
        rules: Option<PathBuf>,
        changed: impl Fn(Value) + Send + Sync + 'static,
    ) -> Desk {
        let loaded = match &rules {
            Some(path) => rules::load(path),
            None => Loaded {
                rules: Rules::builtin(),
                error: None,
            },
        };
        let stamp = rules.as_deref().and_then(rules::stamp);
        // The history sits next to the rules file, so dev runs (`BOUNCER_RULES`)
        // never write the real folder.
        let log = rules
            .as_deref()
            .and_then(Path::parent)
            .map(|dir| Log::new(dir.join("history")));
        Desk {
            state: Mutex::new(State {
                sessions: Vec::new(),
                queue: VecDeque::new(),
                paused: false,
                rules: RuleFile {
                    path: rules,
                    loaded,
                    stamp,
                    notice: None,
                },
                flash: None,
                count: 0,
                log,
                away: None,
            }),
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

    /// Records the event. A permission request a rule allows (auto mode) is
    /// answered at once; any other is queued and this blocks until the user
    /// decides, the wait runs out, or Bouncer is paused. Only a decision
    /// returns `Some`.
    pub fn handle(&self, event: Event) -> Option<Decision> {
        let (tx, rx) = mpsc::channel();
        let id = {
            let mut state = self.lock();
            state.track(&event);
            state.drop_stale(&event);
            if let Some(log) = state.log.as_mut() {
                log.write(&Entry::of(&event));
            }
            let question = QUESTIONS.contains(&event.tool.as_deref().unwrap_or_default());
            if event.is_permission_request() && question {
                // No card, no answer: Claude Code asks it in the terminal.
                // The session says so, so the user notices.
                state.waiting_in_terminal(&event);
                None
            } else if !event.is_permission_request() {
                None
            } else if state.paused {
                state.answered(&event, "asked in terminal");
                None
            } else {
                let verdict = state.check(&event);
                if state.rules.loaded.rules.mode == Mode::Auto && verdict.allow.is_some() {
                    state.auto_allowed(&event);
                    drop(state);
                    self.publish();
                    return Some(Decision::Allow);
                }
                // No ID (the OS random source failed) means no card: the terminal asks.
                let id = request_id();
                if id.is_none() {
                    state.answered(&event, "asked in terminal");
                }
                id.inspect(|id| {
                    state.queue.push_back(Pending {
                        id: id.clone(),
                        event,
                        issued: Instant::now(),
                        answer: tx,
                        risk: sentence(&verdict.reasons),
                        would: verdict.allow,
                        offer: verdict.offer,
                    });
                })
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

    /// "Always allow": adds the rule offered for this request to the rules
    /// file, then allows the request. The rule comes from the backend's own
    /// check of the queued request; nothing from the page is written.
    pub fn always(&self, id: &str) -> Result<(), &'static str> {
        let (rule, path, event) = {
            let state = self.lock();
            let pending = state
                .queue
                .iter()
                .find(|p| p.id == id)
                .ok_or("unknown or already answered request")?;
            if pending.issued.elapsed() < ARM {
                return Err("too soon to allow");
            }
            let rule = pending
                .offer
                .clone()
                .ok_or("no rule to add for this request")?;
            let path = state.rules.path.clone().ok_or("no rules file")?;
            (rule, path, pending.event.clone())
        };
        if let Err(e) = rules::add(&path, &rule) {
            eprintln!("Bouncer: {e}");
            return Err("couldn't add the rule");
        }
        self.lock().reload(true);
        self.decide(id, true)?;
        // In auto mode the rule now answers this command: the pill says so,
        // as the prototype shows. In observe mode nothing will be auto-allowed.
        let mut state = self.lock();
        if state.rules.loaded.rules.mode == Mode::Auto {
            state.flash_allowed(&event);
            drop(state);
            self.publish();
        }
        Ok(())
    }

    /// Drops sessions that went quiet without ending; the island hears about it.
    pub fn expire_sessions(&self) {
        if self.lock().expire(SystemTime::now()) {
            self.publish();
        }
    }

    /// The user came back after an idle span that began at `since` (ms since
    /// 1970): sums up the log for it. True if anything happened, so the
    /// island should open on the summary.
    pub fn came_back(&self, since: u64) -> bool {
        let Some(dir) = self.lock().log.as_ref().map(|l| l.dir().to_owned()) else {
            return false;
        };
        // Read outside the lock: relay threads keep going meanwhile.
        let now = activity::ms(SystemTime::now());
        let summary = away::summarize(&activity::read(&dir, since, now), since, now);
        let found = summary.is_some();
        if found {
            self.lock().away = summary;
        }
        found
    }

    /// The island closed: the away summary has been seen.
    pub fn clear_away(&self) {
        self.lock().away = None;
    }

    /// "Wipe history": deletes every day file and the summary.
    pub fn wipe_history(&self) -> std::io::Result<()> {
        let result = {
            let mut state = self.lock();
            state.away = None;
            let result = state.log.as_mut().map_or(Ok(()), Log::wipe);
            match &result {
                Ok(()) => state.flash(
                    "History wiped",
                    " · the activity log is empty".into(),
                    "log",
                    false,
                ),
                Err(_) => state.flash(
                    "History not wiped",
                    " · a file couldn't be deleted".into(),
                    "log",
                    true,
                ),
            }
            result
        };
        self.publish();
        result
    }

    /// The settings switch: writes `mode` into the rules file (checked,
    /// atomic) and reloads it, so the island shows the change like any edit.
    pub fn set_mode(&self, mode: Mode) -> Result<(), &'static str> {
        let path = self.lock().rules.path.clone().ok_or("no rules file")?;
        if let Err(e) = rules::set_mode(&path, mode) {
            eprintln!("Bouncer: {e}");
            return Err("couldn't change the mode");
        }
        self.lock().reload(true);
        self.publish();
        Ok(())
    }

    /// Checks the rules file for changes; the island hears about any.
    pub fn reload_rules(&self) {
        if self.lock().reload(false) {
            self.publish();
        }
    }

    /// While paused nothing is queued or answered by a rule, and pausing
    /// releases every queued request unanswered, so they all go to the terminal.
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
                    "risk": p.risk,
                    "would": p.would.as_deref().map(visible),
                    "offer": p.offer.as_ref().map(|r| visible(&r.to_toml())),
                })
            })
            .collect();
        let rules = &state.rules;
        let mode = match rules.loaded.rules.mode {
            Mode::Auto => "auto",
            Mode::Observe => "observe",
        };
        let flash = state.flash.as_ref().map(|f| {
            json!({
                "n": f.n,
                "at_ms": epoch_ms(f.at),
                "strong": f.strong,
                "rest": visible(&f.rest),
                "badge": f.badge,
                "risk": f.risk,
            })
        });
        json!({
            "paused": state.paused,
            "sessions": sessions,
            "queue": queue,
            "rules": {
                "mode": mode,
                "error": rules.loaded.error.as_deref().map(visible),
                "notice": rules.notice.as_ref().map(|(n, text)| json!({ "n": n, "text": visible(text) })),
            },
            "flash": flash,
            "history": { "note": state.log.as_ref().and_then(Log::note) },
            "away": state.away.as_ref().map(|a| json!({
                "minutes": a.minutes,
                "files": a.files,
                "commands": a.commands,
                "auto": a.auto,
                "asked": a.asked,
                "stuck": a.stuck.as_ref().map(|s| json!({
                    "minutes": s.minutes,
                    "project": visible(&s.project),
                    "what": visible(&s.what),
                })),
            })),
        })
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
pub(crate) fn step(event: &Event) -> String {
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
        Some("Bash" | "PowerShell") => {
            let command = text("command").unwrap_or_default();
            format!("Running {}", command.lines().next().unwrap_or_default())
        }
        Some("Grep" | "Glob") => "Searching".into(),
        Some("WebFetch" | "WebSearch") => "Browsing the web".into(),
        Some("Task" | "Agent") => "Running a subagent".into(),
        Some("AskUserQuestion") => "Asking you a question".into(),
        Some("ExitPlanMode") => "Showing you a plan".into(),
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
        Some(command) if matches!(event.tool.as_deref(), Some("Bash" | "PowerShell")) => {
            command.to_owned()
        }
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
    use std::path::PathBuf;
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
        Arc::new(Desk::new(wait, None, |_| {}))
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

    /// A card that timed out, then answered Yes in Claude Code's own prompt:
    /// the tool runs and its PostToolUse clears "asks in terminal".
    #[test]
    fn a_yes_in_the_terminal_after_a_timeout_clears_the_wait() {
        let desk = desk(Duration::from_millis(100));
        assert_eq!(ask(&desk, "a", "mkdir x").join().unwrap(), None);
        assert_eq!(desk.view()["sessions"][0]["status"], IN_TERMINAL);
        assert_eq!(desk.handle(event("a", "PostToolUse", "mkdir x")), None);
        assert_eq!(desk.view()["sessions"][0]["status"], "working");
        assert_eq!(desk.handle(event("a", "Stop", "")), None);
        assert_eq!(desk.view()["sessions"][0]["status"], "idle");
    }

    /// Answered Yes in Claude Code's own prompt while the card is still up:
    /// the tool's PostToolUse means the card is stale. It leaves the island at
    /// once (no answer is sent: Claude Code already has one), and the session
    /// is working, not "asks in terminal".
    #[test]
    fn answering_in_the_terminal_while_the_card_is_up_clears_it() {
        let desk = desk(Duration::from_millis(400));
        let a = ask(&desk, "a", "mkdir x");
        queued(&desk, 1);
        assert_eq!(desk.handle(event("a", "PostToolUse", "mkdir x")), None);
        assert_eq!(desk.view()["queue"], json!([]));
        assert_eq!(a.join().unwrap(), None);
        let view = desk.view();
        let s = &view["sessions"][0];
        assert_eq!(s["status"], "working");
        assert_eq!(s["history"][0]["how"], "answered in terminal");
    }

    /// Another tool finishing in the same session (a subagent) is not an
    /// answer: the card stays up.
    #[test]
    fn another_tool_finishing_leaves_the_card_up() {
        let desk = desk(Duration::from_millis(400));
        let a = ask(&desk, "a", "mkdir x");
        queued(&desk, 1);
        assert_eq!(desk.handle(event("a", "PostToolUse", "ls")), None);
        assert_eq!(desk.handle(event("b", "Stop", "")), None);
        assert_eq!(queued(&desk, 1).len(), 1);
        assert_eq!(a.join().unwrap(), None);
    }

    /// Recorded: a request answered No in Claude Code's own prompt (2.1.288).
    /// Claude Code sends the request and then nothing at all (no
    /// `PostToolUseFailure`, no `Stop`, no `Notification`), so the session
    /// stays "asks in terminal" with its last event's time; the island shows
    /// "Waiting in terminal" once that is two minutes old.
    #[test]
    fn a_no_in_the_terminal_sends_nothing_more() {
        let desk = desk(Duration::from_millis(100));
        let recorded =
            include_str!("../tests/fixtures/claude-code-2.1.288-denied-in-terminal.jsonl");
        let events: Vec<Event> = recorded
            .lines()
            .map(|line| Event::from_claude_code(&serde_json::from_str(line).unwrap()).unwrap())
            .collect();
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["PreToolUse", "PermissionRequest"]);
        let before = epoch_ms(SystemTime::now());
        for e in events {
            assert_eq!(desk.handle(e), None, "nothing answered; the card timed out");
        }
        let view = desk.view();
        let s = &view["sessions"][0];
        assert_eq!(s["status"], IN_TERMINAL);
        assert_eq!(s["step"], "Running mkdir deny-test");
        assert_eq!(s["history"][0]["how"], "asked in terminal");
        let last = s["last_ms"].as_u64().unwrap();
        assert!(last >= before && last <= epoch_ms(SystemTime::now()));
    }

    #[test]
    fn quiet_sessions_drop_off() {
        let desk = desk(WAIT);
        desk.handle(event("idle", "Stop", ""));
        desk.handle(event("working", "PreToolUse", "ls"));
        // Paused: the request goes to the terminal and the session waits there.
        desk.set_paused(true);
        desk.handle(event("waiting", "PermissionRequest", "pwd"));
        desk.set_paused(false);
        let carded = ask(&desk, "carded", "cargo run");
        let ids = queued(&desk, 1);
        let now = SystemTime::now();
        let left = |at: Duration| {
            desk.lock().expire(now + at);
            let view = desk.view();
            let mut ids: Vec<String> = view["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["id"].as_str().unwrap().to_owned())
                .collect();
            ids.sort();
            ids
        };
        assert_eq!(left(Duration::from_secs(29 * 60)).len(), 4);
        assert_eq!(
            left(IDLE_LIMIT + Duration::from_secs(60)),
            ["carded", "waiting"]
        );
        assert_eq!(left(WAITING_LIMIT + Duration::from_secs(60)), ["carded"]);
        desk.decide(&ids[0], false).unwrap();
        carded.join().unwrap();
        assert_eq!(
            left(WAITING_LIMIT + Duration::from_secs(60)),
            Vec::<String>::new()
        );
        // Back with the next event.
        desk.handle(event("idle", "UserPromptSubmit", ""));
        assert_eq!(left(Duration::ZERO), ["idle"]);
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

    /// A desk reading a fresh rules file with `text` in a private temp folder.
    fn desk_with_rules(name: &str, text: &str) -> (Arc<Desk>, PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bouncer-desk-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rules.toml");
        std::fs::write(&path, text).unwrap();
        (Arc::new(Desk::new(WAIT, Some(path.clone()), |_| {})), path)
    }

    const AUTO: &str = "mode = \"auto\"\n[[allow]]\ncommand = \"ls\"\n";

    #[test]
    fn auto_mode_answers_what_a_rule_allows() {
        let (desk, _) = desk_with_rules("auto", AUTO);
        desk.handle(event("a", "PreToolUse", "ls src"));
        let answer = desk.handle(event("a", "PermissionRequest", "ls src"));
        assert_eq!(answer, Some(Decision::Allow), "answered without a card");
        let view = desk.view();
        assert_eq!(view["queue"], json!([]));
        assert_eq!(view["sessions"][0]["history"][0]["how"], BY_RULE);
        assert_eq!(view["sessions"][0]["status"], "working");
        assert_eq!(view["flash"]["strong"], "Auto-allowed");
        assert_eq!(view["flash"]["rest"], " · ls src in p");
        assert_eq!(view["flash"]["badge"], "rule");
        assert_eq!(view["rules"]["mode"], "auto");

        // Not covered, or risky: a card, with the reason.
        let other = ask(&desk, "a", "pwd");
        queued(&desk, 1); // queued first, before the next one is sent
        let risky = ask(&desk, "a", "sudo ls");
        let ids = queued(&desk, 2);
        let view = desk.view();
        assert_eq!(view["queue"][0]["risk"], json!(null));
        assert_eq!(view["queue"][1]["risk"], "Runs as administrator.");
        assert_eq!(view["queue"][1]["offer"], json!(null));
        let offer = view["queue"][0]["offer"].as_str().unwrap();
        assert!(
            offer.starts_with("[[allow]]\ncommand = \"pwd\"\nexact = true\nproject = \""),
            "{offer}"
        );
        for id in &ids {
            desk.decide(id, false).unwrap();
        }
        assert_eq!(other.join().unwrap(), Some(Decision::Deny));
        assert_eq!(risky.join().unwrap(), Some(Decision::Deny));

        // Paused: rules answer nothing either.
        desk.set_paused(true);
        assert_eq!(desk.handle(event("a", "PermissionRequest", "ls")), None);
    }

    #[test]
    fn observe_mode_only_says_what_it_would_do() {
        let desk = desk(WAIT);
        let a = ask(&desk, "a", "git status");
        let id = queued(&desk, 1).remove(0);
        let view = desk.view();
        assert_eq!(view["rules"]["mode"], "observe");
        assert_eq!(view["queue"][0]["would"], "git status");
        assert_eq!(view["queue"][0]["offer"], json!(null));
        desk.decide(&id, false).unwrap();
        assert_eq!(a.join().unwrap(), Some(Decision::Deny));
    }

    #[test]
    fn set_mode_writes_the_file_and_the_island_shows_it() {
        let (desk, path) = desk_with_rules("set-mode", "mode = \"observe\"\n");
        desk.set_mode(Mode::Auto).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mode = \"auto\"\n");
        let view = desk.view();
        assert_eq!(view["rules"]["mode"], "auto");
        assert_eq!(view["rules"]["notice"]["text"], "Rules changed: mode auto");
        desk.set_mode(Mode::Observe).unwrap();
        assert_eq!(desk.view()["rules"]["mode"], "observe");
    }

    #[test]
    fn always_allow_adds_the_offered_rule_then_allows() {
        let (desk, path) = desk_with_rules("always", "mode = \"auto\"\n");
        let a = ask(&desk, "a", "cargo run --release");
        let id = queued(&desk, 1).remove(0);
        assert_eq!(desk.always(&id), Err("too soon to allow"));
        std::thread::sleep(ARM);
        desk.always(&id).unwrap();
        assert_eq!(a.join().unwrap(), Some(Decision::Allow));
        let text = std::fs::read_to_string(&path).unwrap();
        let added = &text[text.rfind("[[allow]]").unwrap()..];
        assert!(
            added.starts_with(
                "[[allow]]\ncommand = \"cargo run --release\"\nexact = true\nproject = \""
            ),
            "{added}"
        );
        assert_eq!(desk.always(&id), Err("unknown or already answered request"));
        let view = desk.view();
        assert_eq!(
            view["rules"]["notice"]["text"],
            "Rules changed: added cargo run --release in p"
        );
        assert_eq!(view["sessions"][0]["history"][0]["how"], "you allowed");
        // Auto mode: the pill then says the rule allows it (as the prototype).
        assert_eq!(
            (
                &view["flash"]["strong"],
                &view["flash"]["rest"],
                &view["flash"]["badge"]
            ),
            (
                &json!("Auto-allowed"),
                &json!(" · cargo run --release in p"),
                &json!("rule")
            )
        );
        // Next time the rule answers: the same command in the same project.
        assert_eq!(
            desk.handle(event("a", "PermissionRequest", "cargo run --release")),
            Some(Decision::Allow)
        );
        // Not with words added or removed, and not in another project.
        for (session, project, command) in [
            ("a", "/p", "cargo run --release --verbose"),
            ("a", "/p", "cargo run"),
            ("b", "/p2", "cargo run --release"),
            ("c", "/other/p", "cargo run --release"),
        ] {
            let mut e = event(session, "PermissionRequest", command);
            e.project = project.into();
            let (d, e) = (desk.clone(), e);
            let waiting = std::thread::spawn(move || d.handle(e));
            let id = queued(&desk, 1).remove(0);
            desk.decide(&id, false).unwrap();
            assert_eq!(
                waiting.join().unwrap(),
                Some(Decision::Deny),
                "{project} {command}"
            );
        }
        // Nothing to offer: refused.
        let b = ask(&desk, "a", "rm -rf x");
        let id = queued(&desk, 1).remove(0);
        std::thread::sleep(ARM);
        assert_eq!(desk.always(&id), Err("no rule to add for this request"));
        desk.decide(&id, false).unwrap();
        b.join().unwrap();
    }

    #[test]
    fn always_allow_in_observe_mode_flashes_no_auto_allow() {
        let (desk, path) = desk_with_rules("always-observe", "mode = \"observe\"\n");
        let a = ask(&desk, "a", "cargo run --release");
        let id = queued(&desk, 1).remove(0);
        std::thread::sleep(ARM);
        desk.always(&id).unwrap();
        assert_eq!(a.join().unwrap(), Some(Decision::Allow));
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("cargo run --release")
        );
        // The rule change is shown; "Auto-allowed" isn't: nothing is in observe mode.
        assert_eq!(desk.view()["flash"]["strong"], "Rules changed");
    }

    #[test]
    fn questions_go_to_the_terminal_and_flag_the_session() {
        // Auto mode and a rule set: still never answered, never queued.
        let (desk, _) = desk_with_rules("question", AUTO);
        for tool in ["AskUserQuestion", "ExitPlanMode"] {
            let mut e = event("q", "PreToolUse", "");
            e.tool = Some(tool.into());
            e.input = Some(json!({ "questions": [{ "question": "Which one?" }] }));
            desk.handle(e.clone());
            e.kind = "PermissionRequest".into();
            let started = Instant::now();
            assert_eq!(desk.handle(e.clone()), None, "{tool}");
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "{tool}: no wait"
            );
            let view = desk.view();
            assert_eq!(view["queue"], json!([]), "{tool}");
            let s = &view["sessions"][0];
            assert_eq!(s["status"], "needs you", "{tool}");
            assert_eq!(s["step"], QUESTION_STEP, "{tool}");
            let history = s["history"].as_array().unwrap();
            assert_eq!(history.last().unwrap()["how"], "asked in terminal");
            // Answered in the terminal: the tool runs and the session moves on.
            e.kind = "PostToolUse".into();
            desk.handle(e);
            assert_eq!(desk.view()["sessions"][0]["status"], "working", "{tool}");
        }
        assert_eq!(
            desk.view()["sessions"][0]["history"][0]["label"],
            "Asking you a question"
        );
        desk.set_paused(true);
        let mut e = event("q", "PermissionRequest", "");
        e.tool = Some("AskUserQuestion".into());
        assert_eq!(desk.handle(e), None, "paused");
    }

    #[test]
    fn rule_changes_and_errors_reach_the_island() {
        let (desk, path) = desk_with_rules("reload", "mode = \"observe\"\n");
        desk.reload_rules();
        assert_eq!(desk.view()["rules"]["notice"], json!(null), "unchanged");
        // The stamp includes the size, so a same-second edit is still seen.
        std::fs::write(&path, AUTO).unwrap();
        desk.reload_rules();
        let view = desk.view();
        assert_eq!(view["rules"]["mode"], "auto");
        assert_eq!(
            view["rules"]["notice"]["text"],
            "Rules changed: mode auto · added ls"
        );
        assert_eq!(view["flash"]["strong"], "Rules changed");

        std::fs::write(&path, "mode = \"auto\"\nlol = 1\n").unwrap();
        desk.reload_rules();
        let view = desk.view();
        assert_eq!(view["rules"]["mode"], "observe", "broken file: observe");
        assert!(
            view["rules"]["error"]
                .as_str()
                .unwrap()
                .ends_with("line 2: unknown key `lol`")
        );
        assert_eq!(
            (&view["flash"]["strong"], &view["flash"]["risk"]),
            (&json!("Rules file ignored"), &json!(true))
        );
        assert_eq!(desk.handle(event("a", "PreToolUse", "ls")), None);
        let a = ask(&desk, "a", "ls");
        let id = queued(&desk, 1).remove(0);
        desk.decide(&id, false).unwrap();
        assert_eq!(
            a.join().unwrap(),
            Some(Decision::Deny),
            "never auto-allowed"
        );

        std::fs::write(&path, AUTO).unwrap();
        desk.reload_rules();
        assert_eq!(desk.view()["rules"]["error"], json!(null));
    }

    #[test]
    fn every_event_and_answer_is_logged() {
        let (desk, path) = desk_with_rules("log", AUTO);
        let dir = path.parent().unwrap().join("history");
        desk.handle(event("a", "PreToolUse", "ls"));
        assert_eq!(
            desk.handle(event("a", "PermissionRequest", "ls")),
            Some(Decision::Allow)
        );
        let asked = ask(&desk, "a", "pwd");
        let id = queued(&desk, 1).remove(0);
        desk.decide(&id, false).unwrap();
        asked.join().unwrap();
        desk.set_paused(true);
        desk.handle(event("a", "PermissionRequest", "pwd"));
        let log: Vec<_> = activity::read(&dir, 0, activity::ms(SystemTime::now()))
            .into_iter()
            .map(|e| (e.kind, e.label, e.how.unwrap_or_default()))
            .collect();
        let row = |k: &str, l: &str, h: &str| (k.to_owned(), l.to_owned(), h.to_owned());
        assert_eq!(
            log,
            [
                row("PreToolUse", "Running ls", ""),
                row("PermissionRequest", "Running ls", ""),
                row("Answer", "Running ls", BY_RULE),
                row("PermissionRequest", "Running pwd", ""),
                row("Answer", "Running pwd", "you denied"),
                row("PermissionRequest", "Running pwd", ""),
                row("Answer", "Running pwd", "asked in terminal"),
            ]
        );
        assert_eq!(desk.view()["history"]["note"], json!(null));
    }

    #[test]
    fn coming_back_shows_a_summary_until_closed_and_wipe_empties_it() {
        let (desk, _) = desk_with_rules("away", AUTO);
        let since = activity::ms(SystemTime::now()) - 1;
        assert!(!desk.came_back(since), "nothing happened");
        desk.handle(event("a", "PreToolUse", "ls"));
        desk.handle(event("a", "PermissionRequest", "ls"));
        desk.handle(event("a", "PostToolUse", "ls"));
        assert!(desk.came_back(since));
        let away = desk.view()["away"].clone();
        assert_eq!(
            (&away["commands"], &away["auto"], &away["asked"]),
            (&json!(1), &json!(1), &json!(0))
        );
        desk.clear_away();
        assert_eq!(desk.view()["away"], json!(null));
        assert!(desk.came_back(since));
        desk.wipe_history().unwrap();
        let view = desk.view();
        assert_eq!(view["away"], json!(null));
        assert_eq!(view["flash"]["strong"], "History wiped");
        assert!(!desk.came_back(since), "nothing left to sum up");
        // No rules folder, no log: nothing to show, wiping is a no-op.
        let plain = Desk::new(WAIT, None, |_| {});
        plain.handle(event("a", "PreToolUse", "ls"));
        assert!(!plain.came_back(0));
        plain.wipe_history().unwrap();
    }

    #[test]
    fn a_broken_file_at_start_shows_its_error() {
        let (desk, _) = desk_with_rules(
            "start",
            "mode = \"auto\"\n[[allow]]\ncommand = \"ls; rm x\"\n",
        );
        let view = desk.view();
        assert_eq!(view["rules"]["mode"], "observe");
        assert!(view["rules"]["error"].as_str().unwrap().contains("line 3"));
    }
}
