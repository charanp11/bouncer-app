//! The server on a real pipe / socket, talked to like the relay does.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bouncer_core::ipc::{Decision, Handler, Server};

fn endpoint(name: &str) -> PathBuf {
    let id = format!("bouncer-test-{}-{name}", std::process::id());
    #[cfg(windows)]
    return format!(r"\\.\pipe\{id}").into();
    #[cfg(unix)]
    return std::env::temp_dir().join(id).join("bouncer.sock");
}

fn start(name: &str, decision: Option<Decision>) -> PathBuf {
    let path = endpoint(name);
    let server = Server::bind(&path).unwrap();
    let handler: Handler = Arc::new(move |_| decision);
    std::thread::spawn(move || server.run(handler));
    path
}

fn ask(path: &Path, event: &str) -> String {
    let mut stream = bouncer_relay::connect(path).expect("connect");
    let line = format!(r#"{{"session_id":"s","cwd":"/p","hook_event_name":"{event}"}}"#);
    stream.write_all(format!("{line}\n").as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    answer
}

#[test]
fn answers_permission_requests_over_the_endpoint() {
    let path = start("answers", Some(Decision::Allow));
    assert_eq!(ask(&path, "PermissionRequest"), "allow\n");
    assert_eq!(ask(&path, "PreToolUse"), "");
}

#[test]
fn second_server_on_the_same_endpoint_is_refused() {
    let path = start("twice", None);
    assert!(Server::bind(&path).is_err());
}

#[test]
fn many_connections_at_once() {
    let path = start("many", Some(Decision::Deny));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || ask(&path, "PermissionRequest"))
        })
        .collect();
    for t in threads {
        assert_eq!(t.join().unwrap(), "deny\n");
    }
}

#[cfg(unix)]
#[test]
fn refuses_a_folder_others_can_open() {
    use std::os::unix::fs::PermissionsExt;
    let path = endpoint("open-folder");
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Server::bind(&path).is_err());
}
