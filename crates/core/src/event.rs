//! One internal event format for every agent: Claude Code's hook JSON, or
//! Codex's, which the relay marks (`AGENT_FIELD`) when its command line
//! says so. The shape alone never decides: the two are nearly the same.

use std::time::SystemTime;

use serde_json::Value;

pub const CLAUDE_CODE: &str = "claude-code";
pub const CODEX: &str = "codex";
pub const COPILOT: &str = "copilot";
pub const CURSOR: &str = "cursor";

/// Set by the relay (`--agent codex`) on the line it sends; any copy in the
/// agent's own JSON is removed first.
pub const AGENT_FIELD: &str = "bouncer_agent";

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Which agent sent it: `CLAUDE_CODE` or `CODEX`.
    pub agent: &'static str,
    pub session: String,
    /// The session's working directory.
    pub project: String,
    /// Hook event name, e.g. `PreToolUse`, `PermissionRequest`.
    pub kind: String,
    pub tool: Option<String>,
    /// Tool input exactly as the agent sent it; never trimmed or rewritten.
    pub input: Option<Value>,
    /// When Bouncer received it.
    pub time: SystemTime,
    /// A `PostToolUseFailure` caused by the user stopping the call (Esc),
    /// not by the tool failing.
    pub interrupted: bool,
    /// Claude Code's permission mode for this event (`default`, `auto`,
    /// `acceptEdits`, `bypassPermissions`, ...), when it sent one. Never set
    /// for Codex (the guard is Claude Code only).
    pub mode: Option<String>,
    /// The agent's own words on why it asks (Codex's `description`): shown
    /// as its text, never part of `input`, so no check ever reads it.
    pub reason: Option<String>,
}

impl Event {
    /// The relay's line: Codex's JSON if the relay marked it so, Claude
    /// Code's if unmarked; any other mark is refused.
    pub fn from_hook(hook: &Value) -> Option<Event> {
        match hook.get(AGENT_FIELD) {
            None => Event::from_claude_code(hook),
            Some(v) if v == CODEX => Event::from_codex(hook, cfg!(windows)),
            Some(v) if v == COPILOT => Event::from_deny_only(hook, COPILOT),
            Some(v) if v == CURSOR => Event::from_deny_only(hook, CURSOR),
            Some(_) => None,
        }
    }

    /// Builds an event from Claude Code hook JSON. `None` if a required field
    /// is missing or has the wrong type.
    pub fn from_claude_code(hook: &Value) -> Option<Event> {
        let text = |key| hook.get(key)?.as_str().map(str::to_owned);
        Some(Event {
            agent: CLAUDE_CODE,
            session: text("session_id")?,
            project: text("cwd")?,
            kind: text("hook_event_name")?,
            tool: match hook.get("tool_name") {
                None => None,
                Some(v) => Some(v.as_str()?.to_owned()),
            },
            input: hook.get("tool_input").cloned(),
            time: SystemTime::now(),
            interrupted: hook.get("is_interrupt") == Some(&Value::Bool(true)),
            mode: text("permission_mode"),
            reason: None,
        })
    }

    /// Builds an event from Codex hook JSON (Codex 0.163.0's fields). On
    /// Windows Codex runs its "Bash" commands in PowerShell, so they are
    /// checked as PowerShell. For its own tools (`Bash`, `apply_patch`),
    /// `tool_input.description` is the model's reason: moved out of the
    /// input into `reason`. An MCP tool's arguments stay as they came (a
    /// `description` there is part of the request). `None` as above.
    pub fn from_codex(hook: &Value, windows: bool) -> Option<Event> {
        let mut event = Event::from_claude_code(hook)?;
        event.agent = CODEX;
        event.mode = None;
        event.interrupted = false;
        let own = matches!(event.tool.as_deref(), Some("Bash" | "apply_patch"));
        if own && let Some(input) = event.input.as_mut().and_then(Value::as_object_mut) {
            event.reason = match input.remove("description") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s),
                Some(_) => return None,
            };
        }
        if windows && event.tool.as_deref() == Some("Bash") {
            event.tool = Some("PowerShell".into());
        }
        Some(event)
    }

    /// Builds an event from the relay's line for Copilot CLI or Cursor's
    /// CLI (the relay rebuilt it in Claude Code's field names; a Copilot
    /// notification's type in `input`). `None` as above.
    pub fn from_deny_only(hook: &Value, agent: &'static str) -> Option<Event> {
        let mut event = Event::from_claude_code(hook)?;
        event.agent = agent;
        event.mode = None;
        event.interrupted = false;
        Some(event)
    }

    /// Copilot and Cursor get Deny or nothing: their hooks run for every
    /// request (they can't say whether the agent would ask), so Bouncer
    /// never allows for them and its rules never answer.
    pub fn deny_only(&self) -> bool {
        self.agent == COPILOT || self.agent == CURSOR
    }

    /// A Copilot notification's type (`permission_prompt`,
    /// `elicitation_dialog`, `agent_idle`, ...).
    pub fn notification(&self) -> Option<&str> {
        (self.agent == COPILOT && self.kind == "Notification")
            .then(|| self.input.as_ref()?["notification_type"].as_str())?
    }

    pub fn is_permission_request(&self) -> bool {
        self.kind == "PermissionRequest"
    }

    /// A tool call that failed on its own (not stopped by the user).
    pub fn is_failure(&self) -> bool {
        self.kind == "PostToolUseFailure" && !self.interrupted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_tool_event() {
        let e = Event::from_claude_code(&json!({
            "session_id": "s1", "cwd": "/p", "hook_event_name": "PermissionRequest",
            "tool_name": "Bash", "tool_input": {"command": "ls"}
        }))
        .unwrap();
        assert_eq!(
            (e.agent, &*e.session, &*e.project),
            ("claude-code", "s1", "/p")
        );
        assert_eq!(e.tool.as_deref(), Some("Bash"));
        assert_eq!(e.input, Some(json!({"command": "ls"})));
        assert!(e.is_permission_request());
    }

    #[test]
    fn rejects_missing_or_mistyped_fields() {
        let ok = json!({"session_id": "s", "cwd": "/p", "hook_event_name": "Stop"});
        assert!(Event::from_claude_code(&ok).is_some());
        for key in ["session_id", "cwd", "hook_event_name"] {
            let mut bad = ok.clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(Event::from_claude_code(&bad).is_none(), "{key}");
            bad[key] = json!(1);
            assert!(Event::from_claude_code(&bad).is_none(), "{key}");
        }
        let mut bad = ok.clone();
        bad["tool_name"] = json!(["Bash"]);
        assert!(Event::from_claude_code(&bad).is_none());
    }

    /// Only a real failure counts; Esc (`is_interrupt: true`) doesn't, and
    /// anything but `true` is not an interrupt.
    #[test]
    fn a_failure_is_not_an_interrupt() {
        let fail = |interrupt: Value| {
            let mut hook = json!({"session_id": "s", "cwd": "/p", "hook_event_name": "PostToolUseFailure", "tool_name": "Bash"});
            if !interrupt.is_null() {
                hook["is_interrupt"] = interrupt;
            }
            Event::from_claude_code(&hook).unwrap().is_failure()
        };
        assert!(fail(json!(false)));
        assert!(fail(Value::Null));
        assert!(fail(json!("true")));
        assert!(!fail(json!(true)));
        let stop = json!({"session_id": "s", "cwd": "/p", "hook_event_name": "PostToolUse", "is_interrupt": false});
        assert!(!Event::from_claude_code(&stop).unwrap().is_failure());
    }

    fn codex_request(tool: &str, input: Value) -> Value {
        json!({ "session_id": "c", "cwd": "/p", "hook_event_name": "PermissionRequest",
            "permission_mode": "bypassPermissions", "model": "m", "tool_name": tool,
            "tool_input": input, AGENT_FIELD: CODEX })
    }

    /// Only the relay's mark makes a Codex event; no mark is Claude Code;
    /// any other mark (or a mark that isn't text) is refused.
    #[test]
    fn the_relay_mark_decides_the_agent() {
        let plain = json!({ "session_id": "s", "cwd": "/p", "hook_event_name": "Stop" });
        assert_eq!(Event::from_hook(&plain).unwrap().agent, CLAUDE_CODE);
        let mut codex = plain.clone();
        codex[AGENT_FIELD] = json!(CODEX);
        assert_eq!(Event::from_hook(&codex).unwrap().agent, CODEX);
        for agent in [COPILOT, CURSOR] {
            let mut marked = plain.clone();
            marked[AGENT_FIELD] = json!(agent);
            let event = Event::from_hook(&marked).unwrap();
            assert_eq!(event.agent, agent);
            assert!(event.deny_only(), "{agent}");
        }
        assert!(!Event::from_hook(&codex).unwrap().deny_only());
        assert!(!Event::from_hook(&plain).unwrap().deny_only());
        for other in [
            json!("claude-code"),
            json!("Codex"),
            json!("Cursor"),
            json!(""),
            json!(1),
            json!(null),
        ] {
            let mut bad = plain.clone();
            bad[AGENT_FIELD] = other.clone();
            assert!(Event::from_hook(&bad).is_none(), "{other}");
        }
    }

    /// Codex's shell command: PowerShell on Windows, Bash elsewhere; its
    /// `description` leaves the input for `reason`; no permission mode, so
    /// the guard never applies.
    #[test]
    fn a_codex_command_keeps_its_reason_apart() {
        let hook = codex_request(
            "Bash",
            json!({ "command": "ls", "description": "Probe\u{202E}." }),
        );
        for (windows, tool) in [(true, "PowerShell"), (false, "Bash")] {
            let e = Event::from_codex(&hook, windows).unwrap();
            assert_eq!(e.agent, CODEX);
            assert_eq!(e.tool.as_deref(), Some(tool));
            assert_eq!(e.input, Some(json!({ "command": "ls" })));
            assert_eq!(e.reason.as_deref(), Some("Probe\u{202E}."));
            assert_eq!(e.mode, None);
        }
        let none = codex_request("Bash", json!({ "command": "ls", "description": null }));
        assert_eq!(Event::from_codex(&none, true).unwrap().reason, None);
        let bad = codex_request("Bash", json!({ "command": "ls", "description": ["x"] }));
        assert!(Event::from_codex(&bad, true).is_none());
        let patch = codex_request(
            "apply_patch",
            json!({ "command": "*** Begin Patch", "description": "edit" }),
        );
        let e = Event::from_codex(&patch, true).unwrap();
        assert_eq!(
            (e.tool.as_deref(), e.reason.as_deref()),
            (Some("apply_patch"), Some("edit"))
        );
    }

    /// An MCP tool's arguments are the request: a `description` there stays.
    #[test]
    fn an_mcp_tool_keeps_every_argument() {
        let input = json!({ "title": "t", "description": "the issue body" });
        let e = Event::from_codex(
            &codex_request("mcp__github__create_issue", input.clone()),
            true,
        )
        .unwrap();
        assert_eq!(e.input, Some(input));
        assert_eq!(e.reason, None);
    }

    /// Codex 0.163.0's recorded events (paths as `dev`) all parse.
    #[test]
    fn recorded_codex_events_parse() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../relay/tests/codex-fixtures");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let mut hook: Value =
                serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap();
            hook[AGENT_FIELD] = json!(CODEX);
            let e = Event::from_hook(&hook).unwrap();
            assert_eq!(e.agent, CODEX);
            if e.is_permission_request() {
                assert_eq!(e.reason.as_deref(), Some("Probe for the hook checks."));
            }
            count += 1;
        }
        assert_eq!(count, 6);
    }
}
