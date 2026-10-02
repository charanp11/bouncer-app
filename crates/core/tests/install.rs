//! `bouncer install-hooks` / `uninstall-hooks` on temp files only.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Formatted like Claude Code writes it (2-space JSON), keys not sorted.
const ORIGINAL: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(npm test:*)"
    ]
  },
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "notify-send done"
          }
        ]
      }
    ]
  },
  "model": "opus"
}"#;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bouncer-install-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bouncer-hook.exe"), "").unwrap();
    dir
}

fn bouncer(dir: &Path, args: &[&str], answer: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bouncer"))
        .args(args)
        .arg("--settings")
        .arg(dir.join("settings.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Ignored: the CLI may exit before reading the answer.
    let _ = child.stdin.take().unwrap().write_all(answer.as_bytes());
    child.wait_with_output().unwrap()
}

fn install(dir: &Path, answer: &str) -> Output {
    let relay = dir.join("bouncer-hook.exe");
    bouncer(
        dir,
        &["install-hooks", "--relay", relay.to_str().unwrap()],
        answer,
    )
}

fn backups(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains(".bouncer-backup-"))
        .map(|p| fs::read_to_string(p).unwrap())
        .collect();
    found.sort();
    found
}

#[test]
fn install_then_uninstall_is_byte_identical() {
    for original in [ORIGINAL.to_owned(), format!("{ORIGINAL}\n")] {
        let dir = temp_dir(&format!("roundtrip-{}", original.len()));
        let settings = dir.join("settings.json");
        fs::write(&settings, &original).unwrap();

        let out = install(&dir, "y\n");
        assert!(out.status.success(), "{out:?}");
        let installed = fs::read_to_string(&settings).unwrap();
        assert!(installed.contains("bouncer-hook.exe"));
        assert!(installed.contains("notify-send done"));
        assert!(String::from_utf8_lossy(&out.stdout).contains("+ "));
        assert_eq!(backups(&dir), vec![original.clone()]);

        let out = bouncer(&dir, &["uninstall-hooks"], "y\n");
        assert!(out.status.success(), "{out:?}");
        assert_eq!(fs::read_to_string(&settings).unwrap(), original);
        assert_eq!(backups(&dir).len(), 2);
        assert!(!fs::read_dir(&dir).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .contains("bouncer-tmp")
        }));
    }
}

#[test]
fn nothing_is_written_without_yes() {
    for answer in ["", "n\n", "no\n", "maybe\n", "\n"] {
        let dir = temp_dir("no");
        let settings = dir.join("settings.json");
        fs::write(&settings, ORIGINAL).unwrap();
        let out = install(&dir, answer);
        assert!(out.status.success());
        assert_eq!(
            fs::read_to_string(&settings).unwrap(),
            ORIGINAL,
            "{answer:?}"
        );
        assert!(backups(&dir).is_empty());
    }
}

#[test]
fn invalid_json_is_never_touched() {
    let dir = temp_dir("invalid");
    let settings = dir.join("settings.json");
    fs::write(&settings, "{ not json").unwrap();
    let out = install(&dir, "y\n");
    assert!(!out.status.success());
    assert_eq!(fs::read_to_string(&settings).unwrap(), "{ not json");
    assert!(backups(&dir).is_empty());
}

#[test]
fn install_creates_a_missing_file_and_needs_the_relay() {
    let dir = temp_dir("missing");
    let settings = dir.join("settings.json");
    let out = bouncer(
        &dir,
        &["install-hooks", "--relay", "/nope/bouncer-hook"],
        "y\n",
    );
    assert!(!out.status.success());
    assert!(!settings.exists());

    assert!(install(&dir, "y\n").status.success());
    assert!(
        fs::read_to_string(&settings)
            .unwrap()
            .contains("PermissionRequest")
    );
    assert!(install(&dir, "").status.success()); // already installed: no prompt needed
    assert!(String::from_utf8_lossy(&install(&dir, "").stdout).contains("Nothing to change"));
}

#[test]
fn bad_arguments_fail() {
    let dir = temp_dir("args");
    for args in [
        &["frobnicate"][..],
        &["install-hooks", "--settings"],
        &["uninstall-hooks", "--relay", "x"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_bouncer"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?}");
    }
}
