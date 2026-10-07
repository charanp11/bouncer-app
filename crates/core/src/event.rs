//! One internal event format for every agent. Claude Code hook JSON is the
//! only input today; a second agent becomes another `from_*` constructor.

use std::time::SystemTime;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Which agent sent it, e.g. `claude-code`.
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
}

impl Event {
    /// Builds an event from Claude Code hook JSON. `None` if a required field
    /// is missing or has the wrong type.
    pub fn from_claude_code(hook: &Value) -> Option<Event> {
        let text = |key| hook.get(key)?.as_str().map(str::to_owned);
        Some(Event {
            agent: "claude-code",
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
        })
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
}
