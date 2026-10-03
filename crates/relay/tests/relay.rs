//! The real `bouncer-hook` binary against a real server (or none).

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bouncer_core::event::Event;
use bouncer_core::ipc::{Decision, Handler, Server};

fn endpoint(name: &str) -> PathBuf {
    let id = format!("bouncer-relay-test-{}-{name}", std::process::id());
    #[cfg(windows)]
    return format!(r"\\.\pipe\{id}").into();
    #[cfg(unix)]
    return std::env::temp_dir().join(id).join("bouncer.sock");
}

fn serve(name: &str, handler: Handler) -> PathBuf {
    let path = endpoint(name);
    let server = Server::bind(&path).unwrap();
    std::thread::spawn(move || server.run(handler));
    path
}

fn spawn_relay(path: &Path, input: &[u8]) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bouncer-hook"))
        .env(bouncer_relay::ENDPOINT_ENV, path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The relay may stop reading early (oversized input); that's fine.
    if let Err(e) = child.stdin.take().unwrap().write_all(input) {
        assert_eq!(e.kind(), ErrorKind::BrokenPipe, "{e}");
    }
    child
}

fn relay(path: &Path, input: &[u8]) -> Output {
    spawn_relay(path, input).wait_with_output().unwrap()
}

fn assert_silent(out: &Output) {
    assert!(out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
}

fn event(session: &str, kind: &str) -> Vec<u8> {
    format!(r#"{{"session_id":"{session}","cwd":"/p","hook_event_name":"{kind}","tool_name":"Bash","tool_input":{{"command":"ls"}}}}"#).into_bytes()
}

/// Fire-and-forget relays exit before the server's handler thread has run, so
/// wait (briefly) until it has seen `count` events, then check it saw exactly that.
fn wait_for<T>(seen: &Mutex<Vec<T>>, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while seen.lock().unwrap().len() < count && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(seen.lock().unwrap().len(), count);
}

fn always(decision: Option<Decision>) -> Handler {
    Arc::new(move |_| decision)
}

#[test]
fn fixtures_parse_into_events_and_relay_silently() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let path = serve(
        "fixtures",
        Arc::new(move |e: Event| {
            log.lock().unwrap().push(e.kind);
            Some(Decision::Allow)
        }),
    );
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let file = entry.unwrap().path();
        let raw = std::fs::read(&file).unwrap();
        let json = serde_json::from_slice(&raw).unwrap();
        let parsed = Event::from_claude_code(&json).unwrap_or_else(|| panic!("{file:?}"));
        let out = relay(&path, &raw);
        if parsed.is_permission_request() {
            assert!(String::from_utf8_lossy(&out.stdout).contains(r#""behavior":"allow""#));
        } else {
            assert_silent(&out);
        }
        count += 1;
    }
    assert!(count >= 3, "fixtures missing");
    wait_for(&seen, count);
}

#[test]
fn app_closed_exits_fast_and_silent() {
    let path = endpoint("closed");
    let start = Instant::now();
    let out = relay(&path, &event("s", "PermissionRequest"));
    let took = start.elapsed();
    assert_silent(&out);
    eprintln!("app closed: relay done in {took:?}");
    // Generous bound for slow CI runners; measured locally in the phase notes.
    assert!(took < Duration::from_secs(1), "{took:?}");
}

#[test]
fn malformed_or_huge_input_exits_silent() {
    let path = serve("malformed", always(Some(Decision::Allow)));
    let mut huge =
        br#"{"session_id":"s","cwd":"/p","hook_event_name":"PermissionRequest","x":""#.to_vec();
    huge.resize(5 * 1024 * 1024, b'a');
    huge.extend_from_slice(br#""}"#);
    for input in [
        &b""[..],
        b"\0\xff\xfe garbage",
        b"{\"hook_event_name\":",
        b"[\"PermissionRequest\"]",
        br#"{"hook_event_name":"PermissionRequest"}"#, // no session or cwd
        &huge,
    ] {
        assert_silent(&relay(&path, input));
    }
}

#[test]
fn permission_answers_print_the_documented_json() {
    let cases = [
        (Some(Decision::Allow), Some("allow")),
        (Some(Decision::Deny), Some("deny")),
        (None, None),
    ];
    for (i, (decision, behavior)) in cases.into_iter().enumerate() {
        let path = serve(&format!("answer-{i}"), always(decision));
        let out = relay(&path, &event("s", "PermissionRequest"));
        assert!(out.status.success());
        match behavior {
            None => assert!(out.stdout.is_empty(), "{out:?}"),
            Some(b) => {
                let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                let hook = &json["hookSpecificOutput"];
                assert_eq!(hook["hookEventName"], "PermissionRequest");
                assert_eq!(hook["decision"]["behavior"], b);
            }
        }
    }
}

#[test]
fn other_events_never_print_even_if_the_app_says_allow() {
    let path = serve("observe", always(Some(Decision::Allow)));
    for kind in ["PreToolUse", "PostToolUse", "Stop", "permissionrequest"] {
        assert_silent(&relay(&path, &event("s", kind)));
    }
}

#[test]
fn hung_app_fire_and_forget_gives_up_at_two_seconds() {
    // Bound but never served: the pipe/socket buffer fills and the write blocks.
    let path = endpoint("hung");
    let _server = Server::bind(&path).unwrap();
    let mut big = br#"{"session_id":"s","cwd":"/p","hook_event_name":"PreToolUse","x":""#.to_vec();
    big.resize(900 * 1024, b'a');
    big.extend_from_slice(br#""}"#);
    let start = Instant::now();
    let out = relay(&path, &big);
    let took = start.elapsed();
    assert_silent(&out);
    assert!(took >= Duration::from_millis(1900), "{took:?}");
    assert!(took < Duration::from_secs(4), "{took:?}");
}

#[test]
fn stdin_left_open_gives_up_at_two_seconds() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bouncer-hook"))
        .env(bouncer_relay::ENDPOINT_ENV, endpoint("stdin-open"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&event("s", "PermissionRequest")).unwrap();
    let start = Instant::now();
    let deadline = start + Duration::from_secs(4);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let took = start.elapsed();
    let _ = child.kill();
    let out = child.wait_with_output().unwrap();
    drop(stdin);
    assert!(out.status.success(), "{out:?} after {took:?}");
    assert!(out.stdout.is_empty());
    assert!(took < Duration::from_secs(3), "{took:?}");
}

#[test]
fn hung_app_permission_request_keeps_waiting_past_two_seconds() {
    // The full 110 s budget is too slow for a test; check it outlives the
    // fire-and-forget budget and still prints nothing when killed.
    let path = endpoint("hung-ask");
    let _server = Server::bind(&path).unwrap();
    let mut child = spawn_relay(&path, &event("s", "PermissionRequest"));
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        child.try_wait().unwrap().is_none(),
        "relay gave up too early"
    );
    child.kill().unwrap();
    assert!(child.wait_with_output().unwrap().stdout.is_empty());
}

#[test]
fn two_sessions_stream_while_one_waits_for_a_decision() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let path = serve(
        "two-sessions",
        Arc::new(move |e: Event| {
            let asks = e.is_permission_request();
            log.lock().unwrap().push((e.session, e.kind));
            if asks {
                std::thread::sleep(Duration::from_millis(800));
            }
            Some(Decision::Allow)
        }),
    );
    let waiting = spawn_relay(&path, &event("a", "PermissionRequest"));
    let others: Vec<_> = (0..5)
        .flat_map(|_| ["a", "b"])
        .map(|s| spawn_relay(&path, &event(s, "PreToolUse")))
        .collect();
    for child in others {
        assert_silent(&child.wait_with_output().unwrap());
    }
    let answer = waiting.wait_with_output().unwrap();
    assert!(String::from_utf8_lossy(&answer.stdout).contains(r#""behavior":"allow""#));
    wait_for(&seen, 11);
    let seen = seen.lock().unwrap();
    for s in ["a", "b"] {
        assert_eq!(
            seen.iter()
                .filter(|(x, k)| x == s && k == "PreToolUse")
                .count(),
            5
        );
    }
}

/// Claude Code version of each playground session that recorded fixtures, read
/// from the `"version"` field of that session's transcript.
const RECORDED_WITH: &[(&str, &str)] = &[
    ("29111017-47d8-4697-97be-bc193fecf387", "2.1.143"),
    ("c419a738-bd7a-40e9-bbe9-50b9845911c4", "2.1.143"),
    ("4936ec29-90bc-4f48-93db-b957ead3a613", "2.1.287"),
    ("c6c7d2ff-c166-4191-97cc-0f07a3b38f6c", "2.1.287"),
    ("35fc5f3e-4919-4696-a44d-29e5c9515374", "2.1.287"),
];

#[test]
fn fixture_names_match_their_recording() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for entry in std::fs::read_dir(dir).unwrap() {
        let file = entry.unwrap().path();
        let name = file.file_name().unwrap().to_str().unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let session = json["session_id"].as_str().unwrap();
        let (_, version) = RECORDED_WITH
            .iter()
            .find(|(s, _)| *s == session)
            .unwrap_or_else(|| panic!("{name}: unknown recording session {session}"));
        let mut expected = format!(
            "claude-code-{version}-{}",
            json["hook_event_name"].as_str().unwrap()
        );
        if let Some(tool) = json["tool_name"].as_str() {
            expected += &format!("-{tool}");
        }
        assert_eq!(name, format!("{expected}.json"));
    }
}

#[test]
fn decisions_from_the_desk_reach_claude_code() {
    use bouncer_core::approvals::{ARM, Desk, WAIT};
    let desk = Arc::new(Desk::new(WAIT, None, |_| {}));
    let handler = desk.clone();
    let path = serve("desk", Arc::new(move |e| handler.handle(e)));
    for allow in [false, true] {
        let child = spawn_relay(&path, &event("s", "PermissionRequest"));
        let deadline = Instant::now() + Duration::from_secs(5);
        let id = loop {
            if let Some(id) = desk.view()["queue"][0]["id"].as_str() {
                break id.to_owned();
            }
            assert!(Instant::now() < deadline, "request never queued");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(desk.view()["queue"][0]["text"], "ls");
        std::thread::sleep(ARM);
        desk.decide(&id, allow).unwrap();
        let out = child.wait_with_output().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let behavior = if allow { "allow" } else { "deny" };
        assert_eq!(json["hookSpecificOutput"]["decision"]["behavior"], behavior);
    }
}

#[test]
fn windows_paths_reach_the_island_intact() {
    use bouncer_core::approvals::{Desk, WAIT};
    let desk = Arc::new(Desk::new(WAIT, None, |_| {}));
    let handler = desk.clone();
    let path = serve("paths", Arc::new(move |e| handler.handle(e)));
    let project = r"C:\Users\chara\Desktop\Full Time\x";
    let input = serde_json::json!({
        "session_id": "s",
        "cwd": project,
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": { "file_path": format!(r"{project}\src\main.rs") },
    });
    assert_silent(&relay(&path, input.to_string().as_bytes()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while desk.view()["sessions"][0].is_null() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(desk.view()["sessions"][0]["project"], project);
}
