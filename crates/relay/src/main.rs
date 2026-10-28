//! Claude Code (and Codex, and Copilot CLI) hook relay.
//!
//! Reads one hook event on stdin and forwards it to the Bouncer app. No
//! arguments, or `--agent claude-code` (what installs write since v0.11.1):
//! Claude Code. `--agent codex`: Codex (its line is marked so,
//! and it only ever gets allow / deny). `--agent copilot --event <name>`:
//! GitHub Copilot CLI (its camelCase input becomes the same line, marked
//! so, and it only ever gets deny). Any other arguments: nothing at all.
//! The only environment variable read is the debug endpoint override; the
//! environment is never logged or forwarded.
//!
//! Fail safe: on any error, timeout or unknown input it prints nothing and exits
//! 0, so Claude Code falls back to its normal terminal prompt. The main thread
//! never does blocking I/O on the app: a worker does, and is abandoned if it
//! overruns its budget.

use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bouncer_relay::questions::{self, ASK, PLAN};
use serde_json::Value;

/// Bigger events are dropped whole rather than cut (a cut command could be
/// approved by someone who never saw its end).
const MAX_INPUT: u64 = 1024 * 1024;
/// Fields that can be huge, or hold text Bouncer never shows (a failed
/// tool's `error` can quote a secret): they never leave the relay.
const DROPPED_FIELDS: &[&str] = &["tool_response", "transcript_path", "error"];
/// What else Codex sends that Bouncer never needs: the model's name, the
/// user's prompt and the model's last message.
const CODEX_DROPPED: &[&str] = &["model", "prompt", "last_assistant_message"];
/// Marks the line for the app as Codex's (`bouncer_core::event::AGENT_FIELD`).
const AGENT_FIELD: &str = "bouncer_agent";

/// Copilot CLI's events Bouncer installs, by their camelCase names (the
/// installed command names the event: Copilot's `sessionStart` input
/// doesn't), with the name Bouncer's line gets.
const COPILOT_EVENTS: &[(&str, &str)] = &[
    ("sessionStart", "SessionStart"),
    ("sessionEnd", "SessionEnd"),
    ("permissionRequest", "PermissionRequest"),
    ("notification", "Notification"),
];

/// Which agent's hook ran this relay, from its command line.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Agent {
    /// Claude Code, named on the command line (`--agent claude-code`).
    ClaudeCode,
    /// No arguments: a Claude Code hook installed before v0.11.1.
    Unflagged,
    Codex,
    /// Copilot CLI, with the event its hook was installed for (an entry of
    /// `COPILOT_EVENTS`).
    Copilot(&'static str, &'static str),
}

impl Agent {
    /// No arguments: Claude Code (an older install); exactly `--agent
    /// claude-code`: Claude Code; exactly `--agent codex`: Codex; exactly
    /// `--agent copilot --event <one of COPILOT_EVENTS>`: Copilot; anything
    /// else: `None` (the relay then does nothing).
    fn from_args(args: &[String]) -> Option<Agent> {
        match args {
            [] => Some(Agent::Unflagged),
            [flag, name] if flag == "--agent" && name == "claude-code" => Some(Agent::ClaudeCode),
            [flag, name] if flag == "--agent" && name == "codex" => Some(Agent::Codex),
            [flag, name, e, event] if flag == "--agent" && name == "copilot" && e == "--event" => {
                let &(camel, kind) = COPILOT_EVENTS.iter().find(|(c, _)| c == event)?;
                Some(Agent::Copilot(camel, kind))
            }
            _ => None,
        }
    }
}
/// Whole-run budget for an event nobody waits on.
const FIRE_AND_FORGET: Duration = Duration::from_secs(2);
/// Longest answer line read from the app.
const MAX_ANSWER: u64 = 64;
/// What Claude Code shows (or, in a `claude -p` run, refuses with) when the
/// guard makes it ask.
const GUARD_REASON: &str = "Bouncer flagged this request as risky (Guard every mode).";
/// What Claude is told when the user keeps planning in the island.
const KEEP_PLANNING: &str = "Not approved in Bouncer: keep planning.";

fn main() {
    if let Some(json) = run() {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{json}");
        let _ = out.flush();
    }
    std::process::exit(0);
}

/// What the worker tells the main thread.
enum Step {
    /// A permission request reached the app; wait up to the decision budget.
    Waiting,
    /// Finished, with the JSON to print if any.
    Done(Option<String>),
}

/// Returns the JSON to print, or `None` to print nothing. Everything blocking,
/// reading stdin included, happens on the worker under the deadline.
fn run() -> Option<String> {
    let start = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let agent = Agent::from_args(&args)?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(Step::Done(work(agent, || {
            let _ = tx.send(Step::Waiting);
        })));
    });
    let mut deadline = start + FIRE_AND_FORGET;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Step::Waiting) => deadline = start + bouncer_relay::DECISION_BUDGET,
            Ok(Step::Done(json)) => return json,
            Err(_) => return None,
        }
    }
}

/// Reads the event, forwards it, and for a permission request (or a
/// PreToolUse the app guards) turns the app's answer into output. `waiting`
/// is called only for permission requests. Codex gets only allow / deny;
/// Copilot only deny.
fn work(agent: Agent, waiting: impl FnOnce()) -> Option<String> {
    let (line, payload) = read_event(std::io::stdin().lock(), agent)?;
    match payload["hook_event_name"].as_str()? {
        "PermissionRequest" if matches!(agent, Agent::Copilot(..)) => {
            copilot_json(&talk(&line, true, waiting)?)
        }
        "PermissionRequest" if agent == Agent::Codex => {
            decision_json(&talk(&line, true, waiting)?, None, &Value::Null)
        }
        "PreToolUse" if agent == Agent::Codex => {
            talk(&line, false, || {});
            None
        }
        "PermissionRequest" => {
            let answer = talk(&line, true, waiting)?;
            decision_json(
                &answer,
                payload["tool_name"].as_str(),
                &payload["tool_input"],
            )
        }
        "PreToolUse" => guard_json(&talk(&line, false, || {})?),
        _ => {
            talk(&line, false, || {});
            None
        }
    }
}

/// "Guard every mode": the documented PreToolUse output that makes Claude
/// Code ask, for exactly `ask`; anything else is `None`. It never allows or
/// denies. The reason is fixed text: nothing from the agent or the app.
fn guard_json(answer: &str) -> Option<String> {
    (answer == "ask").then(|| {
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "ask",
                "permissionDecisionReason": GUARD_REASON,
            }
        })
        .to_string()
    })
}

/// Copilot CLI's `permissionRequest` output, for exactly `deny`; anything
/// else is `None`. Never `allow`: Copilot's hooks run before its own rules,
/// and a hook's allow overrides its deny rules (PLAN, roadmap h2 step 1).
fn copilot_json(answer: &str) -> Option<String> {
    (answer == "deny").then(|| {
        serde_json::json!({ "behavior": "deny", "message": "Denied in Bouncer" }).to_string()
    })
}

/// The documented `PermissionRequest` output for the app's answer, each only
/// for its own tool: a question's `answer <picks>` (allow, with `updatedInput`
/// built from Claude Code's own `input` plus the chosen labels), a plan's
/// `approve` (allow, the input as it came) or `keep` (deny, fixed text), and
/// `allow` / `deny` for any other tool. Anything else is `None`, so nothing
/// is printed.
fn decision_json(answer: &str, tool: Option<&str>, input: &Value) -> Option<String> {
    let decision = match (tool, answer) {
        (Some(ASK), _) => serde_json::json!({
            "behavior": "allow",
            "updatedInput": questions::answered(input, answer.strip_prefix("answer ")?)?,
        }),
        (Some(PLAN), "approve") if input.is_object() => {
            serde_json::json!({ "behavior": "allow", "updatedInput": input })
        }
        (Some(PLAN), "keep") => serde_json::json!({ "behavior": "deny", "message": KEEP_PLANNING }),
        (Some(PLAN), _) => return None,
        (_, "allow") => serde_json::json!({ "behavior": "allow" }),
        (_, "deny") => serde_json::json!({ "behavior": "deny", "message": "Denied in Bouncer" }),
        _ => return None,
    };
    let out = serde_json::json!({
        "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision }
    });
    Some(out.to_string())
}

/// Reads and trims the hook JSON. Returns the line to send and the trimmed
/// event (a question's answer is built from its `tool_input`); `None` if
/// it's too big, not a JSON object, or has no event name. The agent's mark
/// comes only from the relay's own command line: any copy in the input goes.
/// With no flag, input carrying a `timestamp` is refused: Claude Code never
/// sent one and every Copilot input does (Copilot runs a repository's
/// `.claude/settings.json` hooks with input in Claude Code's shape). With
/// `--agent claude-code` the input is Claude Code's whatever it carries, so
/// a `timestamp` Claude Code may add one day never hides it.
fn read_event(input: impl Read, agent: Agent) -> Option<(String, Value)> {
    let mut raw = Vec::new();
    input.take(MAX_INPUT + 1).read_to_end(&mut raw).ok()?;
    if raw.len() as u64 > MAX_INPUT {
        return None;
    }
    let raw = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&raw);
    let mut payload: Value = serde_json::from_slice(raw).ok()?;
    if let Agent::Copilot(camel, kind) = agent {
        payload = from_copilot(&payload, camel, kind)?;
    }
    let map = payload.as_object_mut()?;
    map.get("hook_event_name")?.as_str()?;
    if agent == Agent::Unflagged && map.contains_key("timestamp") {
        return None;
    }
    map.remove(AGENT_FIELD);
    for field in DROPPED_FIELDS {
        map.remove(*field);
    }
    if agent == Agent::Codex {
        for field in CODEX_DROPPED {
            map.remove(*field);
        }
        map.insert(AGENT_FIELD.into(), "codex".into());
    }
    if let Agent::Copilot(..) = agent {
        map.insert(AGENT_FIELD.into(), "copilot".into());
    }
    let mut line = payload.to_string();
    line.push('\n');
    Some((line, payload))
}

/// Copilot CLI's camelCase input (1.0.105's fields) as Bouncer's line, with
/// only what Bouncer uses: the session and its folder; for a permission
/// request the tool (`bash` / `powershell` as Bash / PowerShell) and its
/// input; for a notification its type. The input must be the event the hook
/// was installed for (its `hookName` / `hook_event_name`, where Copilot
/// sends one), else `None`. The prompt, the notification's text and the
/// rest stay behind.
fn from_copilot(input: &Value, camel: &str, kind: &str) -> Option<Value> {
    let text = |key| input.get(key)?.as_str();
    let mut line = serde_json::json!({
        "hook_event_name": kind,
        "session_id": text("sessionId")?,
        "cwd": text("cwd")?,
    });
    match camel {
        "permissionRequest" => {
            if text("hookName")? != camel {
                return None;
            }
            line["tool_name"] = match text("toolName")? {
                "bash" => "Bash",
                "powershell" => "PowerShell",
                other => other,
            }
            .into();
            line["tool_input"] = Value::Object(input.get("toolInput")?.as_object()?.clone());
        }
        "notification" => {
            if text("hook_event_name")? != kind {
                return None;
            }
            line["tool_input"] =
                serde_json::json!({ "notification_type": text("notification_type")? });
        }
        _ => {}
    }
    Some(line)
}

/// Sends the event line; if `asks`, calls `waiting` first. Then reads the
/// answer line until the app closes the connection: for any other event the
/// app still closes it (it checks who sent an event before reading it, and an
/// event whose relay had already exited could be lost on a busy machine), and
/// for a PreToolUse it may say `ask` first. Bounded by `FIRE_AND_FORGET` (a
/// permission request, by the decision budget).
fn talk(line: &str, asks: bool, waiting: impl FnOnce()) -> Option<String> {
    let mut stream = bouncer_relay::connect(&bouncer_relay::endpoint()?)?;
    stream.write_all(line.as_bytes()).ok()?;
    if asks {
        waiting();
    }
    let mut answer = String::new();
    (&mut stream)
        .take(MAX_ANSWER)
        .read_to_string(&mut answer)
        .ok()?;
    Some(answer.strip_suffix('\n')?.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_event_claude(input: &[u8]) -> Option<(String, Value)> {
        read_event(input, Agent::Unflagged)
    }

    #[test]
    fn only_no_arguments_or_exactly_agent_codex_run() {
        let args =
            |a: &[&str]| Agent::from_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(args(&[]), Some(Agent::Unflagged));
        assert_eq!(args(&["--agent", "claude-code"]), Some(Agent::ClaudeCode));
        assert_eq!(args(&["--agent", "codex"]), Some(Agent::Codex));
        for bad in [
            &["--agent"][..],
            &["--agent", "Codex"],
            &["--agent", "claude-code", "x"],
            &["--agent", "Claude-Code"],
            &["--agent", "claude"],
            &["--agent", "codex", "x"],
            &["--agent=codex"],
            &["codex"],
            &["--agent", ""],
            &[""],
        ] {
            assert_eq!(args(bad), None, "{bad:?}");
        }
    }

    /// Codex's line is marked by the relay alone, and its model name,
    /// prompt and last message never leave; Claude Code's line never
    /// carries a mark, whatever its input says.
    #[test]
    fn the_mark_comes_only_from_the_command_line() {
        let input = br#"{"hook_event_name":"Stop","bouncer_agent":"codex","model":"m","prompt":"p","last_assistant_message":"l","transcript_path":"t","cwd":"/p"}"#;
        let (_, claude) = read_event(&input[..], Agent::ClaudeCode).unwrap();
        assert_eq!(claude.get(AGENT_FIELD), None);
        assert_eq!(claude["prompt"], "p", "Claude Code's line is as before");
        let (line, codex) = read_event(&input[..], Agent::Codex).unwrap();
        assert_eq!(
            codex,
            serde_json::json!({ "hook_event_name": "Stop", "cwd": "/p", "bouncer_agent": "codex" })
        );
        assert!(line.ends_with('\n'));
        let spoof = br#"{"hook_event_name":"Stop","bouncer_agent":"other"}"#;
        assert_eq!(
            read_event(&spoof[..], Agent::Codex).unwrap().1[AGENT_FIELD],
            "codex"
        );
    }

    #[test]
    fn keeps_event_and_drops_large_fields() {
        let (line, event) = read_event_claude(&br#"{"hook_event_name":"PostToolUse","tool_input":{"command":"ls"},"tool_response":"x","transcript_path":"t"}"#[..],
        )
        .unwrap();
        assert_eq!(
            line,
            "{\"hook_event_name\":\"PostToolUse\",\"tool_input\":{\"command\":\"ls\"}}\n"
        );
        assert_eq!(event["hook_event_name"], "PostToolUse");
    }

    #[test]
    fn a_failure_never_forwards_its_error_text() {
        let (line, event) = read_event_claude(&br#"{"hook_event_name":"PostToolUseFailure","tool_input":{"command":"cat x"},"error":"Exit code 1\ncat: x: No such file","is_interrupt":false}"#[..],
        )
        .unwrap();
        assert_eq!(
            line,
            "{\"hook_event_name\":\"PostToolUseFailure\",\"tool_input\":{\"command\":\"cat x\"},\"is_interrupt\":false}\n"
        );
        assert_eq!(event["hook_event_name"], "PostToolUseFailure");
    }

    #[test]
    fn decisions_match_the_documented_shape() {
        // Compared as JSON values: key order depends on serde_json features.
        let parse = |s: String| serde_json::from_str::<serde_json::Value>(&s).unwrap();
        let bash = Some("Bash");
        let input = serde_json::json!({ "command": "ls" });
        assert_eq!(
            parse(decision_json("allow", bash, &input).unwrap()),
            parse(r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#.into())
        );
        assert_eq!(
            parse(decision_json("deny", bash, &input).unwrap()),
            parse(r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Denied in Bouncer"}}}"#.into())
        );
    }

    #[test]
    fn anything_else_prints_nothing() {
        for answer in [
            "",
            "Allow",
            "allow ",
            " allow",
            "always",
            "yes",
            "allow\nallow",
            "{}",
        ] {
            assert!(
                decision_json(answer, Some("Bash"), &Value::Null).is_none(),
                "{answer:?}"
            );
        }
        // Words for questions and plans mean nothing for any other tool.
        for answer in ["answer 0", "approve", "keep"] {
            assert!(decision_json(answer, Some("Bash"), &serde_json::json!({})).is_none());
            assert!(decision_json(answer, None, &serde_json::json!({})).is_none());
        }
    }

    fn question() -> Value {
        serde_json::json!({ "questions": [{ "question": "Which color?", "header": "Color",
            "multiSelect": false, "options": [{ "label": "Red" }, { "label": "Blue" }] }] })
    }

    /// A question's answer: allow, with `updatedInput` = Claude Code's own
    /// input plus the labels the numbers name. Never a plain allow / deny.
    #[test]
    fn a_question_is_answered_only_with_its_own_labels() {
        let out: Value =
            serde_json::from_str(&decision_json("answer 1", Some(ASK), &question()).unwrap())
                .unwrap();
        let mut input = question();
        input["answers"] = serde_json::json!({ "Which color?": "Blue" });
        assert_eq!(
            out,
            serde_json::json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest",
                "decision": { "behavior": "allow", "updatedInput": input } } })
        );
        for answer in [
            "allow",
            "deny",
            "approve",
            "keep",
            "answer",
            "answer ",
            "answer 2",
            "answer 0,1",
            "answer 0;0",
            "Answer 0",
            "answer  0",
            "answer 0\n",
        ] {
            assert!(
                decision_json(answer, Some(ASK), &question()).is_none(),
                "{answer:?}"
            );
        }
        assert!(decision_json("answer 0", Some(ASK), &Value::Null).is_none());
    }

    /// A plan: approve echoes the input (nothing else, no mode change);
    /// keep is a deny with fixed text. Nothing else prints.
    #[test]
    fn a_plan_is_approved_as_it_came_or_kept_in_planning() {
        let plan =
            serde_json::json!({ "plan": "# Plan\n1. Print hello", "planFilePath": "/p/plan.md" });
        let out: Value =
            serde_json::from_str(&decision_json("approve", Some(PLAN), &plan).unwrap()).unwrap();
        assert_eq!(
            out["hookSpecificOutput"]["decision"],
            serde_json::json!({ "behavior": "allow", "updatedInput": plan })
        );
        let out: Value =
            serde_json::from_str(&decision_json("keep", Some(PLAN), &plan).unwrap()).unwrap();
        assert_eq!(
            out["hookSpecificOutput"]["decision"],
            serde_json::json!({ "behavior": "deny", "message": KEEP_PLANNING })
        );
        for answer in [
            "allow", "deny", "answer 0", "Approve", "approve ", "keep\n", "",
        ] {
            assert!(
                decision_json(answer, Some(PLAN), &plan).is_none(),
                "{answer:?}"
            );
        }
        assert!(decision_json("approve", Some(PLAN), &Value::Null).is_none());
    }

    #[test]
    fn the_guard_prints_ask_for_exactly_ask_and_nothing_else() {
        let parse = |s: String| serde_json::from_str::<serde_json::Value>(&s).unwrap();
        assert_eq!(
            parse(guard_json("ask").unwrap()),
            serde_json::json!({ "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "ask",
                "permissionDecisionReason": GUARD_REASON,
            }})
        );
        for answer in ["", "Ask", "ask ", " ask", "allow", "deny", "ask\nask", "{}"] {
            assert!(guard_json(answer).is_none(), "{answer:?}");
        }
    }

    fn copilot(event: &str) -> Agent {
        Agent::from_args(&["--agent", "copilot", "--event", event].map(String::from)).unwrap()
    }

    #[test]
    fn copilot_runs_only_with_exactly_its_flag_and_an_installed_event() {
        let args =
            |a: &[&str]| Agent::from_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        for &(camel, kind) in COPILOT_EVENTS {
            assert_eq!(copilot(camel), Agent::Copilot(camel, kind));
        }
        for bad in [
            &["--agent", "copilot"][..],
            &["--agent", "copilot", "--event"],
            &["--agent", "copilot", "--event", "preToolUse"],
            &["--agent", "copilot", "--event", "PermissionRequest"],
            &["--agent", "copilot", "--event", "permissionRequest", "x"],
            &["--agent", "Copilot", "--event", "permissionRequest"],
            &["--agent", "copilot", "--events", "permissionRequest"],
            &["--event", "permissionRequest", "--agent", "copilot"],
        ] {
            assert_eq!(args(bad), None, "{bad:?}");
        }
    }

    /// Copilot CLI 1.0.105's inputs (recorded in step 1, paths as `dev`)
    /// become Bouncer's line with only the session, folder, tool and input
    /// (or the notification's type); the prompt and text stay behind.
    #[test]
    fn copilot_input_becomes_a_marked_line_with_only_what_bouncer_uses() {
        let read = |event: &str, input: Value| {
            read_event(input.to_string().as_bytes(), copilot(event)).map(|(_, v)| v)
        };
        let start = serde_json::json!({ "sessionId": "s1", "timestamp": 1_793_172_083_696_u64,
            "cwd": r"C:\Users\dev\p", "source": "new", "initialPrompt": "secret plan" });
        assert_eq!(
            read("sessionStart", start.clone()).unwrap(),
            serde_json::json!({ "hook_event_name": "SessionStart", "session_id": "s1",
                "cwd": r"C:\Users\dev\p", AGENT_FIELD: "copilot" })
        );
        let request = serde_json::json!({ "hookName": "permissionRequest", "sessionId": "s1",
            "timestamp": 1_793_172_086_927_u64, "cwd": r"C:\Users\dev\p", "toolName": "powershell",
            "toolInput": { "command": "Get-Date" }, "permissionSuggestions": [] });
        assert_eq!(
            read("permissionRequest", request.clone()).unwrap(),
            serde_json::json!({ "hook_event_name": "PermissionRequest", "session_id": "s1",
                "cwd": r"C:\Users\dev\p", "tool_name": "PowerShell",
                "tool_input": { "command": "Get-Date" }, AGENT_FIELD: "copilot" })
        );
        let mut bash = request.clone();
        bash["toolName"] = "bash".into();
        assert_eq!(
            read("permissionRequest", bash).unwrap()["tool_name"],
            "Bash"
        );
        let note = serde_json::json!({ "sessionId": "s1", "timestamp": 1_793_172_088_150_u64,
            "cwd": r"C:\Users\dev\p", "message": "Run command: Get-Date", "title": "Permission needed",
            "hook_event_name": "Notification", "notification_type": "permission_prompt" });
        assert_eq!(
            read("notification", note.clone()).unwrap()["tool_input"],
            serde_json::json!({ "notification_type": "permission_prompt" })
        );
        // Another event's input, a missing or mistyped field, or a spoofed
        // mark: refused, or the mark comes from the command line.
        assert!(read("notification", request.clone()).is_none());
        assert!(read("permissionRequest", note.clone()).is_none());
        for key in ["sessionId", "cwd", "hookName", "toolName", "toolInput"] {
            let mut bad = request.clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(read("permissionRequest", bad).is_none(), "{key}");
        }
        let mut bad = request.clone();
        bad["toolInput"] = r#"{"command":"ls"}"#.into();
        assert!(read("permissionRequest", bad).is_none());
        let mut spoof = start;
        spoof[AGENT_FIELD] = "claude-code".into();
        assert_eq!(read("sessionStart", spoof).unwrap()[AGENT_FIELD], "copilot");
    }

    #[test]
    fn copilot_gets_exactly_deny_and_nothing_else() {
        assert_eq!(
            serde_json::from_str::<Value>(&copilot_json("deny").unwrap()).unwrap(),
            serde_json::json!({ "behavior": "deny", "message": "Denied in Bouncer" })
        );
        for answer in [
            "allow", "ask", "", "Deny", "deny ", "approve", "answer 0", "{}",
        ] {
            assert!(copilot_json(answer).is_none(), "{answer:?}");
        }
    }

    /// Claude Code never sends `timestamp`; Copilot always does, also in its
    /// VS Code-style input from a repository's `.claude/settings.json`.
    #[test]
    fn with_no_flag_input_with_a_timestamp_is_refused() {
        let vs = br#"{"hook_event_name":"PreToolUse","session_id":"v","timestamp":"2026-10-28T07:40:06.872Z","cwd":"/p","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
        assert!(read_event_claude(&vs[..]).is_none());
        assert!(
            read_event(&vs[..], Agent::Codex).is_some(),
            "Codex: flag decides"
        );
        let claude = br#"{"hook_event_name":"PreToolUse","session_id":"v","cwd":"/p","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
        assert!(read_event_claude(&claude[..]).is_some());
    }

    /// `--agent claude-code` (installs since v0.11.1): always Claude Code's,
    /// a `timestamp` included, unmarked like an older install's line.
    #[test]
    fn flagged_claude_code_input_is_claude_codes_even_with_a_timestamp() {
        let vs = br#"{"hook_event_name":"PreToolUse","session_id":"v","timestamp":"2026-10-28T07:40:06.872Z","cwd":"/p","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
        let (line, event) = read_event(&vs[..], Agent::ClaudeCode).unwrap();
        assert_eq!(event.get(AGENT_FIELD), None);
        assert_eq!(event["timestamp"], "2026-10-28T07:40:06.872Z");
        let plain = br#"{"hook_event_name":"PreToolUse","session_id":"v","cwd":"/p"}"#;
        assert_eq!(
            read_event(&plain[..], Agent::ClaudeCode).unwrap().0,
            read_event(&plain[..], Agent::Unflagged).unwrap().0,
            "the same line as before"
        );
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn strips_utf8_bom() {
        assert!(read_event_claude(&b"\xEF\xBB\xBF{\"hook_event_name\":\"Stop\"}"[..]).is_some());
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            &b""[..],
            b"not json",
            b"[]",
            b"{}",
            br#"{"hook_event_name":7}"#,
        ] {
            assert!(read_event_claude(bad).is_none());
        }
    }

    #[test]
    fn rejects_oversized_input() {
        let mut big = br#"{"hook_event_name":"Stop","x":""#.to_vec();
        big.resize(MAX_INPUT as usize + 1, b'a');
        big.extend_from_slice(br#""}"#);
        assert!(read_event_claude(&big[..]).is_none());
    }
}
