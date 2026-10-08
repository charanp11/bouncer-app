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

/// A test folder, deleted with everything in it when dropped.
struct TempDir(PathBuf);

impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<std::ffi::OsStr> for TempDir {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!("bouncer-install-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bouncer-hook.exe"), "").unwrap();
    TempDir(dir)
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

#[cfg(unix)]
#[test]
fn backup_and_new_file_keep_the_original_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let dir = temp_dir("perms");
    let settings = dir.join("settings.json");
    fs::write(&settings, ORIGINAL).unwrap();
    fs::set_permissions(&settings, fs::Permissions::from_mode(0o600)).unwrap();

    assert!(install(&dir, "y\n").status.success());
    assert_eq!(mode(&settings), 0o600);
    let backup: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains(".bouncer-backup-"))
        .collect();
    assert_eq!(backup.len(), 1);
    assert_eq!(mode(&backup[0]), 0o600);
}

#[test]
fn claude_config_dir_is_the_default_target() {
    let dir = temp_dir("config-dir");
    let config = dir.join("config");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("settings.json"), ORIGINAL).unwrap();
    let relay = dir.join("bouncer-hook.exe");
    let mut child = Command::new(env!("CARGO_BIN_EXE_bouncer"))
        .args(["install-hooks", "--relay", relay.to_str().unwrap()])
        .env("CLAUDE_CONFIG_DIR", &config)
        // If CLAUDE_CONFIG_DIR were ignored, this keeps the real home safe.
        .env("HOME", &dir)
        .env("USERPROFILE", &dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let written = fs::read_to_string(config.join("settings.json")).unwrap();
    assert!(written.contains("bouncer-hook.exe"));
    assert!(!dir.join(".claude").exists());
}

/// install-hooks and uninstall-hooks write the target and its dated backup,
/// and nothing else: the other files beside it keep their exact bytes.
#[test]
fn only_the_target_and_its_backup_are_written() {
    let dir = temp_dir("only-target");
    fs::write(dir.join("settings.json"), ORIGINAL).unwrap();
    fs::write(dir.join("settings.local.json"), "{\"keep\": true}").unwrap();
    fs::write(dir.join("other.json"), "untouched").unwrap();
    let snapshot = |dir: &Path| {
        let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .map(|p| {
                (
                    p.file_name().unwrap().to_string_lossy().into_owned(),
                    fs::read(&p).unwrap(),
                )
            })
            .collect();
        files.sort();
        files
    };
    let before = snapshot(&dir);
    assert!(install(&dir, "y\n").status.success());
    assert!(bouncer(&dir, &["uninstall-hooks"], "y\n").status.success());
    let after = snapshot(&dir);
    for (name, bytes) in &before {
        let now = &after.iter().find(|(n, _)| n == name).unwrap().1;
        if name != "settings.json" {
            assert_eq!(now, bytes, "{name} changed");
        }
    }
    let added: Vec<&String> = after
        .iter()
        .map(|(n, _)| n)
        .filter(|n| !before.iter().any(|(b, _)| b == *n))
        .collect();
    assert_eq!(added.len(), 2, "{added:?}");
    assert!(
        added
            .iter()
            .all(|n| n.starts_with("settings.json.bouncer-backup-")),
        "{added:?}"
    );
}
