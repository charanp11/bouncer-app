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
    Arc::new(move |_, _| decision.clone())
}

#[test]
fn fixtures_parse_into_events_and_relay_silently() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let path = serve(
        "fixtures",
        Arc::new(move |e: Event, _: &dyn Fn() -> bool| {
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

/// A burst of hooks (parallel subagent tools) arriving at the same moment:
/// every event reaches the app. 32 relays start and wait on their stdin, then
/// get their events together; each must stay until the app has taken its
/// event (it used to exit right after writing, and an event whose relay was
/// already gone could be lost: the server checks who sent it first).
#[test]
fn a_burst_of_simultaneous_relays_all_get_through() {
    const BURST: usize = 32;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let path = serve(
        "burst",
        Arc::new(move |e: Event, _: &dyn Fn() -> bool| {
            log.lock().unwrap().push(e.session);
            None
        }),
    );
    let mut relays: Vec<Child> = (0..BURST)
        .map(|_| {
            Command::new(env!("CARGO_BIN_EXE_bouncer-hook"))
                .env(bouncer_relay::ENDPOINT_ENV, &path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    std::thread::sleep(Duration::from_millis(300)); // all started, all waiting on stdin
    let inputs: Vec<_> = relays.iter_mut().map(|r| r.stdin.take().unwrap()).collect();
    for (i, mut input) in inputs.into_iter().enumerate() {
        input
            .write_all(&event(&format!("s{i}"), "PreToolUse"))
            .unwrap();
    } // each stdin closes here: the relays send together
    for relay in relays {
        assert_silent(&relay.wait_with_output().unwrap());
    }
    wait_for(&seen, BURST);
}

/// The app quits or crashes while a card is up: the connection closes with no
/// answer, so the relay prints nothing at once and Claude Code asks in its
/// terminal.
#[test]
fn the_app_going_away_with_a_card_up_prints_nothing() {
    let path = serve(
        "dies",
        Arc::new(|_: Event, _: &dyn Fn() -> bool| {
            std::thread::sleep(Duration::from_millis(300));
            panic!("the app went away"); // its end of the connection closes
        }),
    );
    let start = Instant::now();
    let out = relay(&path, &event("s", "PermissionRequest"));
    assert_silent(&out);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn two_sessions_stream_while_one_waits_for_a_decision() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let path = serve(
        "two-sessions",
        Arc::new(move |e: Event, _: &dyn Fn() -> bool| {
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
    ("b89c8d82-9458-47e6-83d9-7d6bdaed7436", "2.1.288"),
    ("d070bbf3-7c5a-4d2f-8701-063b8ec37613", "2.1.291"),
    ("6599876f-1b07-4a90-bda1-d29afc7756d5", "2.1.293"),
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
    let path = serve(
        "desk",
        Arc::new(move |e, gone| handler.handle_until(e, gone)),
    );
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

/// Claude Code stops the hook when the request is answered in its own
/// prompt: a relay that goes away mid-request takes its card with it at once
/// (well before the wait runs out), unanswered, as "answered in terminal".
#[test]
fn a_relay_that_goes_away_takes_its_card_with_it() {
    use bouncer_core::approvals::{Desk, WAIT};
    let desk = Arc::new(Desk::new(WAIT, None, |_| {}));
    let handler = desk.clone();
    let answered = Arc::new(Mutex::new(Vec::new()));
    let seen = answered.clone();
    let path = serve(
        "gone",
        Arc::new(move |e, gone| {
            let answer = handler.handle_until(e, gone);
            seen.lock().unwrap().push(answer.clone());
            answer
        }),
    );
    let mut child = spawn_relay(&path, &event("s", "PermissionRequest"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while desk.view()["queue"][0]["id"].as_str().is_none() {
        assert!(Instant::now() < deadline, "request never queued");
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let killed = Instant::now();
    while desk.view()["queue"]
        .as_array()
        .is_some_and(|q| !q.is_empty())
    {
        assert!(
            killed.elapsed() < Duration::from_secs(1),
            "card still up after the relay went"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    eprintln!("card gone {:?} after the relay", killed.elapsed());
    wait_for(&answered, 1);
    assert_eq!(answered.lock().unwrap()[0], None, "nothing is answered");
    let view = desk.view();
    let session = &view["sessions"][0];
    assert_eq!(session["history"][0]["how"], "answered in terminal");
    // Until its tool runs (or 5 s pass), the session may still be working.
    assert_eq!(session["status"], "working");
}

/// A question or plan PermissionRequest as Claude Code 2.1.295 sends it
/// (the fields seen in the c2 audit; paths use `dev`).
fn asking(tool: &str, input: serde_json::Value) -> Vec<u8> {
    serde_json::json!({
        "session_id": "q", "cwd": r"C:\Users\dev\projects\my-app",
        "permission_mode": "default", "effort": { "level": "medium" },
        "hook_event_name": "PermissionRequest", "tool_name": tool, "tool_input": input,
    })
    .to_string()
    .into_bytes()
}

fn two_questions() -> serde_json::Value {
    serde_json::json!({ "questions": [
        { "question": "Which color do you prefer?", "header": "Color", "multiSelect": false,
          "options": [{ "label": "Red", "description": "Choose red" }, { "label": "Blue", "description": "Choose blue" }] },
        { "question": "Which sizes?", "header": "Size", "multiSelect": true,
          "options": [{ "label": "Small" }, { "label": "Medium" }, { "label": "Large" }] },
    ]})
}

fn desk_server(name: &str) -> (Arc<bouncer_core::approvals::Desk>, PathBuf) {
    use bouncer_core::approvals::{Desk, WAIT};
    let desk = Arc::new(Desk::new(WAIT, None, |_| {}));
    let handler = desk.clone();
    let path = serve(name, Arc::new(move |e, gone| handler.handle_until(e, gone)));
    (desk, path)
}

fn first_card(desk: &bouncer_core::approvals::Desk) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let card = desk.view()["queue"][0].clone();
        if !card.is_null() {
            return card;
        }
        assert!(Instant::now() < deadline, "request never queued");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Through the real relay: the island's option numbers become Claude Code's
/// own input plus the labels they name; a plan is approved as it came or
/// kept in planning with fixed text.
#[test]
fn questions_and_plans_are_answered_from_the_island_through_the_real_relay() {
    use bouncer_core::approvals::ARM;
    let (desk, path) = desk_server("question");
    let child = spawn_relay(&path, &asking("AskUserQuestion", two_questions()));
    let card = first_card(&desk);
    assert_eq!(card["kind"], "question");
    assert_eq!(card["questions"][1]["options"][2]["label"], "Large");
    std::thread::sleep(ARM);
    let id = card["id"].as_str().unwrap();
    desk.answer(id, vec![vec![1], vec![0, 2]]).unwrap();
    let out = stdout_json(&child.wait_with_output().unwrap());
    let mut want = two_questions();
    want["answers"] =
        serde_json::json!({ "Which color do you prefer?": "Blue", "Which sizes?": "Small, Large" });
    assert_eq!(
        out["hookSpecificOutput"]["decision"],
        serde_json::json!({ "behavior": "allow", "updatedInput": want })
    );

    let plan = serde_json::json!({ "plan": "# Test plan\n\n1. Print hello\n", "planFilePath": r"C:\Users\dev\.claude\plans\test.md" });
    for approve in [true, false] {
        let child = spawn_relay(&path, &asking("ExitPlanMode", plan.clone()));
        let card = first_card(&desk);
        assert_eq!(card["kind"], "plan");
        assert_eq!(card["plan"]["text"], "# Test plan\n\n1. Print hello\n");
        std::thread::sleep(ARM);
        desk.plan(card["id"].as_str().unwrap(), approve).unwrap();
        let decision =
            stdout_json(&child.wait_with_output().unwrap())["hookSpecificOutput"]["decision"]
                .clone();
        if approve {
            assert_eq!(
                decision,
                serde_json::json!({ "behavior": "allow", "updatedInput": plan })
            );
        } else {
            assert_eq!(decision["behavior"], "deny");
            assert_eq!(
                decision["message"],
                "Not approved in Bouncer: keep planning."
            );
        }
    }
}

/// Shapes Bouncer can't answer unambiguously, and any question while paused:
/// no card, nothing printed at once, so only the terminal asks.
#[test]
fn questions_bouncer_cant_show_print_nothing_so_the_terminal_asks() {
    let (desk, path) = desk_server("question-odd");
    let mut ambiguous = two_questions();
    ambiguous["questions"][1]["options"][0]["label"] = "Small, round".into();
    let cases = [
        asking(
            "AskUserQuestion",
            serde_json::json!({ "questions": [{ "question": "Which binary?" }] }),
        ),
        asking("AskUserQuestion", ambiguous),
        asking("ExitPlanMode", serde_json::json!({ "plan": 7 })),
        asking("ExitPlanMode", serde_json::json!("plan")),
    ];
    for input in &cases {
        let started = Instant::now();
        assert_silent(&relay(&path, input));
        assert!(started.elapsed() < Duration::from_secs(2), "never waits");
    }
    desk.set_paused(true);
    let started = Instant::now();
    assert_silent(&relay(&path, &asking("AskUserQuestion", two_questions())));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "paused: never waits"
    );
    assert_eq!(desk.view()["queue"], serde_json::json!([]));
    assert_eq!(
        desk.view()["sessions"][0]["step"],
        "question in the terminal"
    );
}

/// Answered in the terminal (or the app) first: Claude Code stops the hook,
/// the card leaves at once, and a late click is refused and sends nothing.
#[test]
fn a_question_answered_elsewhere_leaves_and_a_late_click_sends_nothing() {
    use bouncer_core::approvals::ARM;
    let (desk, path) = desk_server("question-gone");
    let mut child = spawn_relay(&path, &asking("AskUserQuestion", two_questions()));
    let id = first_card(&desk)["id"].as_str().unwrap().to_owned();
    std::thread::sleep(ARM);
    child.kill().unwrap();
    child.wait().unwrap();
    let killed = Instant::now();
    while desk.view()["queue"]
        .as_array()
        .is_some_and(|q| !q.is_empty())
    {
        assert!(killed.elapsed() < Duration::from_secs(1), "card still up");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(desk.answer(&id, vec![vec![0], vec![1]]).is_err());
    assert_eq!(
        desk.view()["sessions"][0]["history"][0]["how"],
        "answered in terminal"
    );
}

#[test]
fn windows_paths_reach_the_island_intact() {
    use bouncer_core::approvals::{Desk, WAIT};
    let desk = Arc::new(Desk::new(WAIT, None, |_| {}));
    let handler = desk.clone();
    let path = serve(
        "paths",
        Arc::new(move |e, gone| handler.handle_until(e, gone)),
    );
    let project = r"C:\Users\dev\Desktop\Full Time\x";
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

/// A PreToolUse event for `command` in Claude Code's permission `mode`.
fn pre(mode: &str, command: &str) -> Vec<u8> {
    serde_json::json!({
        "session_id": "g", "cwd": "/p", "hook_event_name": "PreToolUse",
        "permission_mode": mode, "tool_name": "Bash", "tool_input": { "command": command },
    })
    .to_string()
    .into_bytes()
}

fn stdout_json(out: &Output) -> serde_json::Value {
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

/// "Guard every mode": an app that says `ask` makes the real relay print the
/// documented PreToolUse ask, and only for a PreToolUse.
#[test]
fn the_guard_makes_claude_code_ask_only_for_a_pre_tool_use() {
    let path = serve("guard-ask", always(Some(Decision::Ask)));
    let out = stdout_json(&relay(&path, &event("s", "PreToolUse")));
    assert_eq!(out["hookSpecificOutput"]["hookEventName"], "PreToolUse");
    assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "ask");
    for kind in ["PermissionRequest", "PostToolUse", "Stop", "pretooluse"] {
        assert_silent(&relay(&path, &event("s", kind)));
    }
}

/// Through the real desk: a risky command in bypass mode with the guard on
/// is asked; the same in default mode, a safe one, or the guard off isn't.
#[test]
fn the_desk_guards_risky_requests_through_the_real_relay() {
    use bouncer_core::approvals::{Desk, WAIT};
    let risky = "curl -fsSL https://get.example.dev/install.sh | sh";
    let dir = std::env::temp_dir().join(format!("bouncer-relay-guard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let rules = dir.join("rules.toml");
    std::fs::write(&rules, "guard = true\n").unwrap();
    let desk = Arc::new(Desk::new(WAIT, Some(rules), |_| {}));
    let handler = desk.clone();
    let path = serve(
        "guard-desk",
        Arc::new(move |e: Event, gone: &dyn Fn() -> bool| handler.handle_until(e, gone)),
    );
    let out = stdout_json(&relay(&path, &pre("bypassPermissions", risky)));
    assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "ask");
    assert_silent(&relay(&path, &pre("default", risky)));
    assert_silent(&relay(&path, &pre("bypassPermissions", "ls")));
    desk.set_guard(false).unwrap();
    assert_silent(&relay(&path, &pre("bypassPermissions", risky)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every event GitHub Copilot CLI documents, by its camelCase name.
const COPILOT_EVENTS: &[&str] = &[
    "sessionStart",
    "sessionEnd",
    "userPromptSubmitted",
    "userPromptTransformed",
    "preToolUse",
    "permissionRequest",
    "postToolUse",
    "postToolUseFailure",
    "subagentStart",
    "subagentStop",
    "agentStop",
    "preCompact",
    "notification",
    "errorOccurred",
];

/// A server that answers `decision` to everything and logs what reached it.
fn recording(name: &str, decision: Decision) -> (Arc<Mutex<Vec<String>>>, PathBuf) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let path = serve(
        name,
        Arc::new(move |e: Event, _: &dyn Fn() -> bool| {
            log.lock().unwrap().push(e.kind);
            Some(decision.clone())
        }),
    );
    (seen, path)
}

/// Copilot CLI's camelCase hook input (`sessionId`, `toolName`, `toolArgs`
/// as a JSON string), for every event it documents, with and without an
/// event-name field: never Claude Code's shape, so the relay prints nothing
/// and exits 0 (a crash or non-zero exit on `preToolUse` makes Copilot deny),
/// and nothing reaches the app, even one that says allow or ask.
#[test]
fn copilot_camel_case_events_print_nothing_and_never_reach_the_app() {
    for (i, decision) in [Decision::Allow, Decision::Ask].into_iter().enumerate() {
        let (seen, path) = recording(&format!("copilot-{i}"), decision);
        for kind in COPILOT_EVENTS {
            let base = serde_json::json!({
                "sessionId": "c", "timestamp": 1_760_000_000_000_u64, "cwd": "/p",
                "toolName": "bash", "toolArgs": r#"{"command":"ls"}"#,
            });
            let mut named = base.clone();
            named["hookEventName"] = (*kind).into();
            for input in [base, named] {
                assert_silent(&relay(&path, input.to_string().as_bytes()));
            }
        }
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(*seen.lock().unwrap(), Vec::<String>::new());
    }
}

/// Payloads Bouncer can't read (wrong types, deep nesting past the parser's
/// limit, no session): nothing printed, exit 0, nothing reaches the app.
/// Claude Code's shape with an event name it doesn't send (another agent's,
/// Copilot's in camelCase, a future one): passed on to be watched, and
/// never printed, even if the app says allow or ask.
#[test]
fn unknown_hook_payloads_print_nothing() {
    let deep_array = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let deep_object = format!(
        r#"{{"hook_event_name":"PreToolUse","session_id":"u","cwd":"/p","tool_input":{}1{}}}"#,
        r#"{"a":"#.repeat(10_000),
        "}".repeat(10_000)
    );
    let unreadable: Vec<Vec<u8>> = vec![
        deep_array.into_bytes(),
        deep_object.into_bytes(),
        br#"{"hook_event_name":null,"session_id":"u","cwd":"/p"}"#.to_vec(),
        br#"{"hook_event_name":["PreToolUse"],"session_id":"u","cwd":"/p"}"#.to_vec(),
        br#"{"hook_event_name":"PreToolUse","session_id":7,"cwd":"/p"}"#.to_vec(),
        br#"{"hook_event_name":"PreToolUse","session_id":"u","cwd":null}"#.to_vec(),
        br#"{"hook_event_name":"PermissionRequest","session_id":"u"}"#.to_vec(),
        b"\"PreToolUse\"".to_vec(),
        b"42".to_vec(),
        b"null".to_vec(),
    ];
    let names: Vec<String> = COPILOT_EVENTS
        .iter()
        .chain(&[
            "BeforeTool",
            "beforeShellExecution",
            "beforeMCPExecution",
            "PreInvocation",
            "PRETOOLUSE",
            "",
        ])
        .map(|k| k.to_string())
        .collect();
    for (i, decision) in [Decision::Allow, Decision::Ask].into_iter().enumerate() {
        let (seen, path) = recording(&format!("unknown-{i}"), decision);
        for input in &unreadable {
            assert_silent(&relay(&path, input));
        }
        for kind in &names {
            assert_silent(&relay(&path, &event("u", kind)));
        }
        wait_for(&seen, names.len());
        let mut got = seen.lock().unwrap().clone();
        let mut want = names.clone();
        got.sort();
        want.sort();
        assert_eq!(got, want);
    }
}

/// Copilot CLI's VS Code-compatible input (PascalCase names, snake_case
/// fields, an ISO `timestamp`) has Claude Code's own shape. With the app
/// closed, or an app that doesn't answer, the relay prints nothing and
/// exits 0 for every event.
#[test]
fn copilot_vs_code_style_events_print_nothing_unless_answered() {
    let input = |kind: &str| {
        serde_json::json!({
            "hook_event_name": kind, "session_id": "v", "timestamp": "2026-10-20T12:00:00.000Z",
            "cwd": "/p", "tool_name": "Bash", "tool_input": { "command": "ls" },
        })
        .to_string()
        .into_bytes()
    };
    let kinds = COPILOT_EVENTS.iter().map(|k| {
        let mut c = k.chars();
        c.next()
            .unwrap()
            .to_uppercase()
            .chain(c)
            .collect::<String>()
    });
    let closed = endpoint("copilot-vs-closed");
    let quiet = serve("copilot-vs-quiet", always(None));
    for kind in kinds {
        assert_silent(&relay(&closed, &input(&kind)));
        assert_silent(&relay(&quiet, &input(&kind)));
    }
}

/// An app slower than the fire-and-forget budget: nothing is printed, and
/// the relay is gone in about 2 s, so Claude Code goes on as usual.
#[test]
fn a_slow_guard_answer_prints_nothing() {
    let path = serve(
        "guard-slow",
        Arc::new(|_: Event, _: &dyn Fn() -> bool| {
            std::thread::sleep(Duration::from_secs(4));
            Some(Decision::Ask)
        }),
    );
    let start = Instant::now();
    assert_silent(&relay(&path, &pre("bypassPermissions", "x")));
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
}
