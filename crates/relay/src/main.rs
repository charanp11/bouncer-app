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

fn main() {
    let start = Instant::now();
    let Some(line) = read_event(std::io::stdin().lock()) else {
        std::process::exit(0)
    };
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        send(&line);
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(FIRE_AND_FORGET.saturating_sub(start.elapsed()));
    std::process::exit(0);
}

/// Reads and trims the hook JSON. `None` if it's too big, not a JSON object, or
/// has no event name.
fn read_event(input: impl Read) -> Option<String> {
    let mut raw = Vec::new();
    input.take(MAX_INPUT + 1).read_to_end(&mut raw).ok()?;
    if raw.len() as u64 > MAX_INPUT {
        return None;
    }
    let raw = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&raw);
    let mut payload: serde_json::Value = serde_json::from_slice(raw).ok()?;
    let map = payload.as_object_mut()?;
    map.get("hook_event_name")?.as_str()?;
    for field in DROPPED_FIELDS {
        map.remove(*field);
    }
    let mut line = payload.to_string();
    line.push('\n');
    Some(line)
}

/// Connects to the app and sends the event line.
fn send(line: &str) {
    if let Some(mut stream) = bouncer_relay::endpoint().and_then(|p| bouncer_relay::connect(&p)) {
        let _ = stream.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_event_and_drops_large_fields() {
        let line = read_event(
            &br#"{"hook_event_name":"PostToolUse","tool_input":{"command":"ls"},"tool_response":"x","transcript_path":"t"}"#[..],
        )
        .unwrap();
        assert_eq!(
            line,
            "{\"hook_event_name\":\"PostToolUse\",\"tool_input\":{\"command\":\"ls\"}}\n"
        );
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
