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

/// Bigger events are dropped whole rather than cut (a cut command could be
/// approved by someone who never saw its end).
const MAX_INPUT: u64 = 1024 * 1024;
/// Fields that can be huge and that Bouncer never shows.
const DROPPED_FIELDS: &[&str] = &["tool_response", "transcript_path"];
/// Whole-run budget for an event nobody waits on.
const FIRE_AND_FORGET: Duration = Duration::from_secs(2);
/// Whole-run budget for a permission request (under the hook's 120 s timeout).
const DECISION: Duration = Duration::from_secs(110);
/// Longest answer line read from the app.
const MAX_ANSWER: u64 = 64;

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
    /// The request reached the app; wait up to the decision budget.
    Waiting,
    /// Finished, with the app's answer line if there was one.
    Done(Option<String>),
}

/// Returns the JSON to print, or `None` to print nothing.
fn run() -> Option<String> {
    let start = Instant::now();
    let (line, event) = read_event(std::io::stdin().lock())?;
    let asks = event == "PermissionRequest";
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let answer = talk(&line, asks, || {
            let _ = tx.send(Step::Waiting);
        });
        let _ = tx.send(Step::Done(answer));
    });
    let mut deadline = start + FIRE_AND_FORGET;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Step::Waiting) if asks => deadline = start + DECISION,
            Ok(Step::Waiting) => {}
            Ok(Step::Done(answer)) if asks => return decision_json(answer.as_deref()?),
            Ok(Step::Done(_)) | Err(_) => return None,
        }
    }
}

/// The documented `PermissionRequest` output for an exact `allow` / `deny`;
/// anything else is `None`, so nothing is printed.
fn decision_json(answer: &str) -> Option<String> {
    let decision = match answer {
        "allow" => serde_json::json!({ "behavior": "allow" }),
        "deny" => serde_json::json!({ "behavior": "deny", "message": "Denied in Bouncer" }),
        _ => return None,
    };
    let out = serde_json::json!({
        "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision }
    });
    Some(out.to_string())
}

/// Reads and trims the hook JSON. Returns the line to send and the event name;
/// `None` if it's too big, not a JSON object, or has no event name.
fn read_event(input: impl Read) -> Option<(String, String)> {
    let mut raw = Vec::new();
    input.take(MAX_INPUT + 1).read_to_end(&mut raw).ok()?;
    if raw.len() as u64 > MAX_INPUT {
        return None;
    }
    let raw = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&raw);
    let mut payload: serde_json::Value = serde_json::from_slice(raw).ok()?;
    let map = payload.as_object_mut()?;
    let event = map.get("hook_event_name")?.as_str()?.to_owned();
    for field in DROPPED_FIELDS {
        map.remove(*field);
    }
    let mut line = payload.to_string();
    line.push('\n');
    Some((line, event))
}

/// Sends the event line; if `asks`, calls `waiting` and reads one answer line.
fn talk(line: &str, asks: bool, waiting: impl FnOnce()) -> Option<String> {
    let mut stream = bouncer_relay::connect(&bouncer_relay::endpoint()?)?;
    stream.write_all(line.as_bytes()).ok()?;
    if !asks {
        return None;
    }
    waiting();
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
        assert_eq!(event, "PostToolUse");
    }

    #[test]
    fn decisions_match_the_documented_shape() {
        // Compared as JSON values: key order depends on serde_json features.
        let parse = |s: String| serde_json::from_str::<serde_json::Value>(&s).unwrap();
        assert_eq!(
            parse(decision_json("allow").unwrap()),
            parse(r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#.into())
        );
        assert_eq!(
            parse(decision_json("deny").unwrap()),
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
            assert!(decision_json(answer).is_none(), "{answer:?}");
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
