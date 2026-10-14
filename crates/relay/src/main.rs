//! Claude Code hook relay.
//!
//! Reads one hook event on stdin and forwards it to the Bouncer app.
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
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(Step::Done(work(|| {
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
/// is called only for permission requests.
fn work(waiting: impl FnOnce()) -> Option<String> {
    let (line, payload) = read_event(std::io::stdin().lock())?;
    match payload["hook_event_name"].as_str()? {
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
/// it's too big, not a JSON object, or has no event name.
fn read_event(input: impl Read) -> Option<(String, Value)> {
    let mut raw = Vec::new();
    input.take(MAX_INPUT + 1).read_to_end(&mut raw).ok()?;
    if raw.len() as u64 > MAX_INPUT {
        return None;
    }
    let raw = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&raw);
    let mut payload: Value = serde_json::from_slice(raw).ok()?;
    let map = payload.as_object_mut()?;
    map.get("hook_event_name")?.as_str()?;
    for field in DROPPED_FIELDS {
        map.remove(*field);
    }
    let mut line = payload.to_string();
    line.push('\n');
    Some((line, payload))
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

    #[test]
    fn keeps_event_and_drops_large_fields() {
        let (line, event) = read_event(
            &br#"{"hook_event_name":"PostToolUse","tool_input":{"command":"ls"},"tool_response":"x","transcript_path":"t"}"#[..],
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
        let (line, event) = read_event(
            &br#"{"hook_event_name":"PostToolUseFailure","tool_input":{"command":"cat x"},"error":"Exit code 1\ncat: x: No such file","is_interrupt":false}"#[..],
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
            "answer 0
",
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
        let plan = serde_json::json!({ "plan": "# Plan
1. Print hello", "planFilePath": "/p/plan.md" });
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
            "allow", "deny", "answer 0", "Approve", "approve ", "keep
", "",
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

    #[test]
    fn strips_utf8_bom() {
        assert!(read_event(&b"\xEF\xBB\xBF{\"hook_event_name\":\"Stop\"}"[..]).is_some());
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
            assert!(read_event(bad).is_none());
        }
    }

    #[test]
    fn rejects_oversized_input() {
        let mut big = br#"{"hook_event_name":"Stop","x":""#.to_vec();
        big.resize(MAX_INPUT as usize + 1, b'a');
        big.extend_from_slice(br#""}"#);
        assert!(read_event(&big[..]).is_none());
    }
}
