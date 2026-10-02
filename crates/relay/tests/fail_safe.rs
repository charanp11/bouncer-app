use std::io::{ErrorKind, Write};
use std::process::{Command, Stdio};

#[test]
fn prints_nothing_and_exits_zero() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bouncer-hook"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The relay may exit before reading stdin; a broken pipe is fine here.
    let written = child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"hook_event_name":"PermissionRequest"}"#);
    if let Err(e) = written {
        assert_eq!(e.kind(), ErrorKind::BrokenPipe, "{e}");
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}
