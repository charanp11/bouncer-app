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
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::activity::{self, Entry, Log};
use crate::away::{self, Summary};
use crate::check::{self, Context, sentence};
use crate::code::{code_view, line_counts};
use crate::event::Event;
use crate::ipc::Decision;
use crate::rules::{self, Loaded, Mode, Rule, Rules};

/// How long a request waits for the user: well under the relay's budget, so
/// the relay is still listening when the app gives up.
pub const WAIT: Duration =
    Duration::from_secs(bouncer_relay::DECISION_BUDGET.as_secs().saturating_sub(10));
/// Allow is refused this soon after a request is queued.
pub const ARM: Duration = Duration::from_millis(600);
/// While a card waits, how often its relay is checked for having hung up.
const GONE_CHECK: Duration = Duration::from_millis(200);
/// A request answered in Claude Code's own prompt, before Bouncer knows
/// more (it can't tell a No from a Yes; the word "denied" is never used).
const ANSWERED: &str = "answered in terminal";
/// ...and then its tool ran (its PostToolUse, same tool and input, came).
const ALLOWED_IN_TERMINAL: &str = "allowed in terminal";
/// ...and nothing ran for it within `NOT_RUN_AFTER` (or the turn moved on).
const NOT_RUN: &str = "not run (answered in terminal)";
const NOT_RUN_AFTER: Duration = Duration::from_secs(5);
/// What a session waiting for its user after `NOT_RUN` shows.
const WAITING_STEP: &str = "Waiting for you";

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
/// A tool call that failed (its error text never reaches Bouncer).
pub const FAILED: &str = "failed";
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
    /// Code's own rules) and whether it ran (its PostToolUse came).
    history: VecDeque<(String, &'static str, bool)>,
    /// The code pane for the current tool call, if it has one.
    code: Option<Value>,
    /// Lines the current edit adds and removes (numbers only), for the pill.
    lines: Option<(usize, Option<usize>)>,
    /// Tool calls that failed so far (for the "tool failed" sound).
    failed: u64,
    /// Requests answered in Claude Code's own prompt that aren't settled
    /// yet: whether each one's tool runs (a long one only reports when it
    /// ends). Each settles by its own tool and input.
    terminal: Vec<Terminal>,
}

/// Sets how the latest step `label` that's still unsettled in the terminal
/// (answered, or shown not run) ended.
fn mark(history: &mut VecDeque<(String, &'static str, bool)>, label: &str, how: &'static str) {
    if let Some(entry) = history
        .iter_mut()
        .rev()
        .find(|(l, h, _)| l == label && (*h == ANSWERED || *h == NOT_RUN))
    {
        entry.1 = how;
    }
}

struct Terminal {
    event: Event,
    since: SystemTime,
    /// Already shown as `NOT_RUN` (its PostToolUse can still turn it into
    /// `ALLOWED_IN_TERMINAL` until the next prompt).
    not_run: bool,
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
                    lines: None,
                    failed: 0,
                    terminal: Vec::new(),
                });
                self.sessions.len() - 1
            }
        };
        &mut self.sessions[i]
    }

    fn track(&mut self, event: &Event) {
        let paused = self.paused;
        self.settle_terminal(event);
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
                session.lines = line_counts(event.tool.as_deref(), event.input.as_ref());
                let how = match event.kind.as_str() {
                    "PreToolUse" => "",
                    _ if paused => "asked in terminal",
                    _ => "waiting for you",
                };
                // A request follows the PreToolUse of the same call: mark it, don't repeat it.
                match session.history.back_mut() {
                    Some(last) if !how.is_empty() && last.0 == session.step => last.1 = how,
                    _ => {
                        session
                            .history
                            .push_back((session.step.clone(), how, false));
                        if session.history.len() > HISTORY {
                            session.history.pop_front();
                        }
                    }
                }
            }
            "UserPromptSubmit" => {
                session.step = "Thinking".into();
                session.lines = None;
            }
            "Stop" => {
                session.step = "Idle".into();
                session.lines = None;
            }
            "PostToolUse" => {
                let label = step(event);
                if let Some(entry) = session
                    .history
                    .iter_mut()
                    .rev()
                    .find(|(l, _, ran)| *l == label && !ran)
                {
                    entry.2 = true;
                }
            }
            "PostToolUseFailure" if event.is_failure() => {
                session.failed += 1;
                if let Some(last) = session.history.back_mut()
                    && last.0 == step(event)
                {
                    last.1 = FAILED;
                }
            }
            _ => {}
        }
        session.status = match event.kind.as_str() {
            "PermissionRequest" if paused => IN_TERMINAL,
            "PermissionRequest" => "needs you",
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => "working",
            "Stop" => "idle",
            _ => session.status,
        };
    }

    /// Drops sessions quiet for longer than their limit, except any with a
    /// card in the queue. True if any went.
    fn expire(&mut self, now: SystemTime) -> bool {
        let settled = self.not_run_yet(now);
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
        settled || self.sessions.len() != before
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

    /// A request whose card left because Claude Code answered it in its own
    /// prompt (the relay hung up): "answered in terminal" until its tool
    /// runs or `NOT_RUN_AFTER` passes.
    fn answered_in_terminal(&mut self, id: &str) -> bool {
        let Some(pending) = self.take(id, "working", ANSWERED) else {
            return false;
        };
        if let Some(s) = self
            .sessions
            .iter_mut()
            .find(|s| s.id == pending.event.session)
        {
            s.terminal.push(Terminal {
                event: pending.event,
                since: SystemTime::now(),
                not_run: false,
            });
        }
        true
    }

    /// What this event says about requests answered in the terminal: a
    /// tool ran (PostToolUse, same tool and input) → that one is allowed; the
    /// turn moved on (a new prompt, Stop, the session's end) → all the rest
    /// are not run.
    fn settle_terminal(&mut self, event: &Event) {
        let Some(s) = self.sessions.iter_mut().find(|s| s.id == event.session) else {
            return;
        };
        let mut settled = Vec::new();
        match event.kind.as_str() {
            "PostToolUse" | "PostToolUseFailure" => {
                let ran = s
                    .terminal
                    .iter()
                    .position(|t| t.event.tool == event.tool && t.event.input == event.input);
                if let Some(i) = ran {
                    let t = s.terminal.remove(i);
                    let label = step(&t.event);
                    mark(&mut s.history, &label, ALLOWED_IN_TERMINAL);
                    if t.not_run {
                        s.step = label;
                    }
                    settled.push((t.event, ALLOWED_IN_TERMINAL));
                }
            }
            "UserPromptSubmit" | "Stop" | "SessionEnd" => {
                for t in std::mem::take(&mut s.terminal) {
                    if !t.not_run {
                        mark(&mut s.history, &step(&t.event), NOT_RUN);
                        settled.push((t.event, NOT_RUN));
                    }
                }
            }
            _ => {}
        }
        for (e, how) in settled {
            self.answered(&e, how);
        }
    }

    /// Requests answered in the terminal with nothing run for them after
    /// `NOT_RUN_AFTER` (each by its own clock): not run, and the session is
    /// waiting for its user, not "Running". True if any changed.
    fn not_run_yet(&mut self, now: SystemTime) -> bool {
        let mut gone = Vec::new();
        for s in &mut self.sessions {
            let mut any = false;
            for t in &mut s.terminal {
                if t.not_run || now.duration_since(t.since).unwrap_or_default() < NOT_RUN_AFTER {
                    continue;
                }
                t.not_run = true;
                mark(&mut s.history, &step(&t.event), NOT_RUN);
                gone.push(t.event.clone());
                any = true;
            }
            if any {
                s.status = "idle";
                s.step = WAITING_STEP.into();
                s.lines = None;
            }
        }
        for e in &gone {
            self.answered(e, NOT_RUN);
        }
        !gone.is_empty()
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
                        "PostToolUse" | "PostToolUseFailure" => {
                            p.event.tool == event.tool && p.event.input == event.input
                        }
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
        let how = match event.kind.as_str() {
            "PostToolUse" | "PostToolUseFailure" => ALLOWED_IN_TERMINAL,
            _ => NOT_RUN,
        };
        for id in stale {
            self.take(&id, status, how);
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
        self.handle_until(event, &|| false)
    }

    /// As `handle`, and while the card waits, `gone` is asked every
    /// `GONE_CHECK`: once the relay has hung up (Claude Code stopped the hook,
    /// so it has its answer from its own prompt), the card leaves at once,
    /// unanswered, as "answered in terminal".
    pub fn handle_until(&self, event: Event, gone: &dyn Fn() -> bool) -> Option<Decision> {
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
        let deadline = Instant::now() + self.wait;
        let answer = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left.min(GONE_CHECK)) {
                Ok(decision) => break Some(decision),
                // Taken elsewhere (a stale card): already settled.
                Err(RecvTimeoutError::Disconnected) => break None,
                Err(RecvTimeoutError::Timeout) if left <= GONE_CHECK => break None,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if gone() {
                if self.lock().answered_in_terminal(&id) {
                    self.publish();
                }
                return None;
            }
        };
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
                    "history": s.history.iter().map(|(label, how, ran)| json!({ "label": visible(label), "how": how, "ran": ran })).collect::<Vec<_>>(),
                    "code": s.code,
                    "lines": s.lines.map(|(added, removed)| json!([added, removed])),
                    "failed": s.failed,
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
    use crate::rules::tests::{TempDir, temp_dir};
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
            interrupted: false,
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

    /// Two requests from one session (parallel tools): both wait, in order,
    /// and each answer reaches only its own request.
    #[test]
    fn two_cards_from_one_session_wait_in_order() {
        let desk = desk(WAIT);
        let first = ask(&desk, "a", "mkdir one");
        queued(&desk, 1);
        let second = ask(&desk, "a", "mkdir two");
        let ids = queued(&desk, 2);
        let view = desk.view();
        assert_eq!(view["queue"][0]["text"], "mkdir one");
        assert_eq!(view["queue"][1]["text"], "mkdir two");
        assert_eq!(view["queue"][0]["session"], view["queue"][1]["session"]);
        desk.decide(&ids[1], false).unwrap();
        assert_eq!(second.join().unwrap(), Some(Decision::Deny));
        assert_eq!(queued(&desk, 1), ids[..1]);
        std::thread::sleep(ARM);
        desk.decide(&ids[0], true).unwrap();
        assert_eq!(first.join().unwrap(), Some(Decision::Allow));
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
    /// once (no answer is sent: Claude Code already has one), the step reads
    /// "allowed in terminal" (the tool ran), and the session is working.
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
        assert_eq!(s["history"][0]["how"], ALLOWED_IN_TERMINAL);
    }

    /// A failed tool call (recorded: 2.1.291 `PostToolUseFailure`): its step
    /// is tagged "failed" and the session keeps working; nothing is answered.
    #[test]
    fn a_failed_tool_tags_its_step() {
        let desk = desk(Duration::from_millis(100));
        assert_eq!(desk.handle(event("a", "PreToolUse", "cat nope")), None);
        assert_eq!(
            desk.handle(event("a", "PostToolUseFailure", "cat nope")),
            None
        );
        let view = desk.view();
        let s = &view["sessions"][0];
        assert_eq!(s["status"], "working");
        assert_eq!(s["history"][0]["how"], FAILED);
        assert_eq!(s["failed"], 1);
        // A failure for another call doesn't tag the latest step.
        assert_eq!(desk.handle(event("a", "PreToolUse", "ls")), None);
        assert_eq!(
            desk.handle(event("a", "PostToolUseFailure", "cat nope")),
            None
        );
        assert_eq!(desk.view()["sessions"][0]["history"][1]["how"], "");
        assert_eq!(desk.view()["sessions"][0]["failed"], 2);
        // Esc isn't a failure: no tag, no count.
        assert_eq!(desk.handle(event("a", "PreToolUse", "sleep 9")), None);
        let mut stop = event("a", "PostToolUseFailure", "sleep 9");
        stop.interrupted = true;
        assert_eq!(desk.handle(stop), None);
        let view = desk.view();
        assert_eq!(view["sessions"][0]["history"][2]["how"], "");
        assert_eq!(view["sessions"][0]["failed"], 2);
    }

    /// A step counts as run only once its own PostToolUse comes: a failure,
    /// an Esc or no event at all leave it not run (the island then never
    /// shows the green check for it).
    #[test]
    fn only_a_post_tool_use_marks_a_step_run() {
        let desk = desk(Duration::from_millis(100));
        for e in [
            event("a", "PreToolUse", "ls"),
            event("a", "PreToolUse", "cat nope"),
            event("a", "PostToolUseFailure", "cat nope"),
            event("a", "PreToolUse", "cargo fmt"),
            event("a", "PostToolUse", "ls"),
            event("a", "PreToolUse", "sleep 9"),
            event("a", "Stop", ""),
        ] {
            assert_eq!(desk.handle(e), None);
        }
        let view = desk.view();
        let ran: Vec<(&str, bool)> = view["sessions"][0]["history"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s["label"].as_str().unwrap(), s["ran"].as_bool().unwrap()))
            .collect();
        assert_eq!(
            ran,
            [
                ("Running ls", true),
                ("Running cat nope", false),
                ("Running cargo fmt", false),
                ("Running sleep 9", false)
            ]
        );
    }

    /// The recorded failures (Bash 2.1.291, PowerShell 2.1.293, a non-zero
    /// exit each) mark their step "failed" after the call's own PreToolUse.
    #[test]
    fn recorded_failures_mark_their_step() {
        for recorded in [
            include_str!(
                "../../relay/tests/fixtures/claude-code-2.1.291-PostToolUseFailure-Bash.json"
            ),
            include_str!(
                "../../relay/tests/fixtures/claude-code-2.1.293-PostToolUseFailure-PowerShell.json"
            ),
        ] {
            let desk = desk(Duration::from_millis(100));
            let failure: Value = serde_json::from_str(recorded).unwrap();
            let mut pre = failure.clone();
            pre["hook_event_name"] = json!("PreToolUse");
            for hook in [pre, failure] {
                assert_eq!(desk.handle(Event::from_claude_code(&hook).unwrap()), None);
            }
            let view = desk.view();
            let s = &view["sessions"][0];
            assert_eq!(s["history"][0]["how"], FAILED, "{recorded}");
            assert_eq!(s["failed"], 1);
        }
    }

    /// Answered Yes in the terminal while the card is up, and the tool then
    /// failed: the failure clears the stale card like a PostToolUse.
    #[test]
    fn a_failure_after_a_terminal_yes_clears_the_card() {
        let desk = desk(Duration::from_millis(400));
        let a = ask(&desk, "a", "mkdir x");
        queued(&desk, 1);
        assert_eq!(
            desk.handle(event("a", "PostToolUseFailure", "mkdir x")),
            None
        );
        assert_eq!(desk.view()["queue"], json!([]));
        assert_eq!(a.join().unwrap(), None);
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

    /// Events recorded from a real session.
    fn recorded(jsonl: &str) -> Vec<Event> {
        jsonl
            .lines()
            .map(|line| Event::from_claude_code(&serde_json::from_str(line).unwrap()).unwrap())
            .collect()
    }

    /// A permission request whose relay hangs up once its card is up, as when
    /// Claude Code's own prompt is answered (Claude Code stops the hook).
    fn hang_up(desk: &Arc<Desk>, request: Event) {
        let gone = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (d, g) = (desk.clone(), gone.clone());
        let waiting = std::thread::spawn(move || {
            d.handle_until(request, &|| g.load(std::sync::atomic::Ordering::Relaxed))
        });
        let before = desk.view()["queue"].as_array().map_or(0, Vec::len);
        let deadline = Instant::now() + Duration::from_secs(5);
        while desk.view()["queue"].as_array().map_or(0, Vec::len) == before {
            assert!(Instant::now() < deadline, "request never queued");
            std::thread::sleep(Duration::from_millis(5));
        }
        gone.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(waiting.join().unwrap(), None, "nothing is answered");
    }

    fn replay(desk: &Arc<Desk>, events: Vec<Event>) {
        for e in events {
            if e.is_permission_request() {
                hang_up(desk, e);
            } else {
                assert_eq!(desk.handle(e), None);
            }
        }
    }

    /// Recorded (2.1.293): No in Claude Code's own prompt with the card up.
    /// The relay hangs up at once and nothing more comes for that tool (not
    /// even Stop): "answered in terminal", then after 5 s "not run" and the
    /// session waits for its user instead of showing "Running".
    #[test]
    fn a_no_in_the_terminal_with_the_card_up() {
        let desk = desk(WAIT);
        let events = recorded(include_str!(
            "../tests/fixtures/claude-code-2.1.293-no-in-terminal.jsonl"
        ));
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "UserPromptSubmit",
                "PreToolUse",
                "PermissionRequest",
                "Notification"
            ]
        );
        replay(&desk, events);
        let view = desk.view();
        assert_eq!(view["queue"], json!([]));
        assert_eq!(view["sessions"][0]["history"][0]["how"], ANSWERED);
        desk.lock()
            .expire(SystemTime::now() + NOT_RUN_AFTER + Duration::from_secs(1));
        let view = desk.view();
        let s = &view["sessions"][0];
        assert_eq!(s["history"][0]["how"], NOT_RUN);
        assert_eq!(s["status"], "idle");
        assert_eq!(s["step"], WAITING_STEP);
    }

    /// Recorded (2.1.293): Yes in Claude Code's own prompt with the card up.
    /// The relay hangs up, then the tool's PostToolUse comes: "allowed in
    /// terminal", the session works on, and Stop makes it idle.
    #[test]
    fn a_yes_in_the_terminal_with_the_card_up() {
        let desk = desk(WAIT);
        let mut events = recorded(include_str!(
            "../tests/fixtures/claude-code-2.1.293-yes-in-terminal.jsonl"
        ));
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "UserPromptSubmit",
                "PreToolUse",
                "PermissionRequest",
                "PostToolUse",
                "Stop",
                "Notification"
            ]
        );
        let after = events.split_off(3);
        replay(&desk, events);
        assert_eq!(desk.view()["sessions"][0]["history"][0]["how"], ANSWERED);
        let mut after = after.into_iter();
        assert_eq!(desk.handle(after.next().unwrap()), None); // PostToolUse
        let view = desk.view();
        let s = &view["sessions"][0];
        assert_eq!(s["history"][0]["how"], ALLOWED_IN_TERMINAL);
        assert_eq!(s["history"][0]["ran"], true);
        assert_eq!(s["status"], "working");
        for e in after {
            assert_eq!(desk.handle(e), None);
        }
        desk.lock()
            .expire(SystemTime::now() + NOT_RUN_AFTER + Duration::from_secs(1));
        let view = desk.view();
        let s = &view["sessions"][0];
        assert_eq!(
            s["history"][0]["how"], ALLOWED_IN_TERMINAL,
            "settled: 5 s change nothing"
        );
        assert_eq!(s["status"], "idle");
    }

    /// A long command allowed in the terminal only reports when it ends: shown
    /// "not run" after 5 s, it becomes "allowed in terminal" when its
    /// PostToolUse comes, and the session works again.
    #[test]
    fn a_long_command_allowed_in_the_terminal_turns_allowed_late() {
        let desk = desk(WAIT);
        assert_eq!(desk.handle(event("a", "PreToolUse", "cargo build")), None);
        hang_up(&desk, event("a", "PermissionRequest", "cargo build"));
        desk.lock()
            .expire(SystemTime::now() + NOT_RUN_AFTER + Duration::from_secs(1));
        assert_eq!(desk.view()["sessions"][0]["history"][0]["how"], NOT_RUN);
        assert_eq!(desk.handle(event("a", "PostToolUse", "cargo build")), None);
        let view = desk.view();
        let s = &view["sessions"][0];
        assert_eq!(s["history"][0]["how"], ALLOWED_IN_TERMINAL);
        assert_eq!(s["status"], "working");
        assert_eq!(s["step"], "Running cargo build");
    }

    /// Two cards in one session, both answered in the terminal: each settles
    /// on its own. The one whose PostToolUse comes is allowed; the other is
    /// not run after 5 s (one record no longer overwrites the other).
    #[test]
    fn two_terminal_answers_in_one_session_settle_separately() {
        let desk = desk(WAIT);
        assert_eq!(desk.handle(event("a", "PreToolUse", "mkdir one")), None);
        hang_up(&desk, event("a", "PermissionRequest", "mkdir one"));
        assert_eq!(desk.handle(event("a", "PreToolUse", "mkdir two")), None);
        hang_up(&desk, event("a", "PermissionRequest", "mkdir two"));
        let hows = |desk: &Desk| -> Vec<String> {
            desk.view()["sessions"][0]["history"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| {
                    format!(
                        "{} = {}",
                        h["label"].as_str().unwrap(),
                        h["how"].as_str().unwrap()
                    )
                })
                .collect()
        };
        assert_eq!(
            hows(&desk),
            [
                "Running mkdir one = answered in terminal",
                "Running mkdir two = answered in terminal"
            ]
        );
        assert_eq!(desk.handle(event("a", "PostToolUse", "mkdir one")), None);
        desk.lock()
            .expire(SystemTime::now() + NOT_RUN_AFTER + Duration::from_secs(1));
        assert_eq!(
            hows(&desk),
            [
                "Running mkdir one = allowed in terminal",
                "Running mkdir two = not run (answered in terminal)"
            ]
        );
        let view = desk.view();
        assert_eq!(view["sessions"][0]["status"], "idle");
        assert_eq!(view["sessions"][0]["step"], WAITING_STEP);
    }

    /// Two cards queued: answering one in the terminal (its relay hangs up)
    /// removes only that one.
    #[test]
    fn a_hang_up_removes_only_its_own_card() {
        let desk = desk(WAIT);
        let other = ask(&desk, "a", "mkdir one");
        queued(&desk, 1);
        hang_up(&desk, event("b", "PermissionRequest", "mkdir two"));
        let view = desk.view();
        let queue = view["queue"].as_array().unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0]["text"], "mkdir one");
        let id = queue[0]["id"].as_str().unwrap().to_owned();
        desk.decide(&id, false).unwrap();
        assert_eq!(other.join().unwrap(), Some(Decision::Deny));
    }

    /// A session that crashed or was closed without SessionEnd drops off
    /// after its limit; its next event brings it back.
    #[test]
    fn a_dropped_session_comes_back_with_its_next_event() {
        let desk = desk(WAIT);
        desk.handle(event("gone", "PreToolUse", "ls"));
        desk.lock()
            .expire(SystemTime::now() + IDLE_LIMIT + Duration::from_secs(60));
        assert_eq!(desk.view()["sessions"], json!([]));
        desk.handle(event("gone", "PreToolUse", "cargo test"));
        let view = desk.view();
        assert_eq!(view["sessions"][0]["id"], "gone");
        assert_eq!(view["sessions"][0]["status"], "working");
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
    fn sessions_carry_the_current_edits_line_counts() {
        let desk = desk(WAIT);
        let lines = || desk.view()["sessions"][0]["lines"].clone();
        let mut e = event("a", "PreToolUse", "");
        e.tool = Some("Edit".into());
        e.input =
            Some(json!({ "file_path": "/p/a.rs", "old_string": "a\nb", "new_string": "a\nB\nc" }));
        desk.handle(e.clone());
        assert_eq!(lines(), json!([2, 1]));
        e.tool = Some("Write".into());
        e.input = Some(json!({ "file_path": "/p/b.rs", "content": "x\ny" }));
        desk.handle(e.clone());
        assert_eq!(lines(), json!([2, null]));
        desk.handle(event("a", "PreToolUse", "ls"));
        assert_eq!(lines(), Value::Null);
        desk.handle(e);
        desk.handle(event("a", "Stop", ""));
        assert_eq!(lines(), Value::Null);
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
                { "label": "Running ls", "how": "", "ran": false },
                { "label": "Running cargo test", "how": "waiting for you", "ran": false }
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

    /// A desk reading a fresh rules file with `text` in a private temp
    /// folder, deleted when the returned guard drops.
    fn desk_with_rules(name: &str, text: &str) -> (Arc<Desk>, PathBuf, TempDir) {
        let dir = temp_dir(&format!("desk-{name}"));
        let path = dir.join("rules.toml");
        std::fs::write(&path, text).unwrap();
        (
            Arc::new(Desk::new(WAIT, Some(path.clone()), |_| {})),
            path,
            dir,
        )
    }

    const AUTO: &str = "mode = \"auto\"\n[[allow]]\ncommand = \"ls\"\n";

    #[test]
    fn auto_mode_answers_what_a_rule_allows() {
        let (desk, _, _dir) = desk_with_rules("auto", AUTO);
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
        let (desk, path, _dir) = desk_with_rules("set-mode", "mode = \"observe\"\n");
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
        let (desk, path, _dir) = desk_with_rules("always", "mode = \"auto\"\n");
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
        let (desk, path, _dir) = desk_with_rules("always-observe", "mode = \"observe\"\n");
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
        let (desk, _, _dir) = desk_with_rules("question", AUTO);
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
        let (desk, path, _dir) = desk_with_rules("reload", "mode = \"observe\"\n");
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
        let (desk, path, _dir) = desk_with_rules("log", AUTO);
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
        let (desk, _, _dir) = desk_with_rules("away", AUTO);
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
        let (desk, _, _dir) = desk_with_rules(
            "start",
            "mode = \"auto\"\n[[allow]]\ncommand = \"ls; rm x\"\n",
        );
        let view = desk.view();
        assert_eq!(view["rules"]["mode"], "observe");
        assert!(view["rules"]["error"].as_str().unwrap().contains("line 3"));
    }
}
