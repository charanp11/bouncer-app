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

/// The CLI with every place it could find a real file pointed into `dir`
/// (home, Claude Code's and Codex's folders), so no test can ever reach the
/// user's own settings, whatever the arguments.
fn hermetic(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bouncer"));
    command
        .env(bouncer_core::hooks::TEST_GUARD_ENV, "1")
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("CLAUDE_CONFIG_DIR", dir.join("claude-config"))
        .env("CODEX_HOME", dir.join("codex-home"));
    command
}

fn bouncer(dir: &Path, args: &[&str], answer: &str) -> Output {
    let mut child = hermetic(dir)
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

/// The uninstaller's console: it says why it asks, a "no" says what Claude
/// Code will show, and it waits for Enter before closing. Same rules.
#[test]
fn the_uninstaller_asks_explains_and_waits() {
    let dir = temp_dir("uninstalling");
    let settings = dir.join("settings.json");
    fs::write(&settings, ORIGINAL).unwrap();
    assert!(install(&dir, "y\n").status.success());
    let installed = fs::read_to_string(&settings).unwrap();

    let out = bouncer(&dir, &["uninstall-hooks", "--uninstalling"], "n\n\n");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(text.contains("Bouncer is being uninstalled"), "{text}");
    assert!(text.contains("Answer N to keep them"), "{text}");
    assert!(text.contains("- ") && text.contains("Kept."), "{text}");
    assert!(text.trim_end().ends_with("Press Enter to close."), "{text}");
    // The why sits right above the question, where a long diff can't push it away.
    let why = text.find("Bouncer is being uninstalled").unwrap();
    assert!(why > text.rfind("\n- ").unwrap(), "{text}");
    assert!(why < text.find("Write these changes?").unwrap(), "{text}");
    assert!(text.contains("missing hook until\nthe entries"), "{text}");
    assert_eq!(fs::read_to_string(&settings).unwrap(), installed);

    let out = bouncer(&dir, &["uninstall-hooks", "--uninstalling"], "y\n\n");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(fs::read_to_string(&settings).unwrap(), ORIGINAL);

    // Only uninstall-hooks takes it.
    let relay = dir.join("bouncer-hook.exe");
    let out = bouncer(
        &dir,
        &[
            "install-hooks",
            "--uninstalling",
            "--relay",
            relay.to_str().unwrap(),
        ],
        "y\n",
    );
    assert!(!out.status.success());
}

/// `uninstall-hooks --uninstalling` with no file given: Claude Code's
/// settings, then Codex's hooks, each with its diff and its own yes; with a
/// file given, only that file (never the user's own). All inside `dir`.
#[test]
fn the_uninstaller_removes_both_agents_hooks_and_a_given_file_only() {
    let dir = temp_dir("uninstall-both");
    let relay = dir.join("bouncer-hook.exe");
    let claude = dir.join("claude-config").join("settings.json");
    let codex = dir.join("codex-home").join("hooks.json");
    fs::create_dir_all(claude.parent().unwrap()).unwrap();
    fs::create_dir_all(codex.parent().unwrap()).unwrap();
    fs::write(&claude, ORIGINAL).unwrap();
    let codex_original = "{}\n";
    fs::write(&codex, codex_original).unwrap();
    let run = |args: &[&str], answer: &str| {
        let mut child = hermetic(&dir)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = child.stdin.take().unwrap().write_all(answer.as_bytes());
        child.wait_with_output().unwrap()
    };
    let relay = relay.to_str().unwrap();
    assert!(
        run(&["install-hooks", "--relay", relay], "y\n")
            .status
            .success()
    );
    let out = run(&["install-hooks", "--codex", "--relay", relay], "y\n");
    assert!(String::from_utf8_lossy(&out.stdout).contains("Trust all and continue"));
    let (claude_in, codex_in) = (
        fs::read_to_string(&claude).unwrap(),
        fs::read_to_string(&codex).unwrap(),
    );
    assert!(claude_in.contains("bouncer-hook.exe") && codex_in.contains("--agent codex"));

    // A file given: only that one is offered; Codex's stays.
    let other = dir.join("settings.json");
    fs::write(&other, &claude_in).unwrap();
    let out = run(
        &[
            "uninstall-hooks",
            "--uninstalling",
            "--settings",
            other.to_str().unwrap(),
        ],
        "y\n\n",
    );
    assert!(out.status.success());
    assert!(!fs::read_to_string(&other).unwrap().contains("bouncer-hook"));
    assert_eq!(
        fs::read_to_string(&claude).unwrap(),
        claude_in,
        "the user's own file untouched"
    );
    assert_eq!(fs::read_to_string(&codex).unwrap(), codex_in);

    // None given: both, each asked; no for Codex keeps it.
    let out = run(&["uninstall-hooks", "--uninstalling"], "y\nn\n\n");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Remove its hooks from Codex?"), "{text}");
    assert_eq!(fs::read_to_string(&claude).unwrap(), ORIGINAL);
    assert_eq!(fs::read_to_string(&codex).unwrap(), codex_in);
    let out = run(&["uninstall-hooks", "--uninstalling"], "y\n\n");
    assert!(String::from_utf8_lossy(&out.stdout).contains("Nothing to change"));
    assert_eq!(fs::read_to_string(&codex).unwrap(), codex_original);
    assert!(text.trim_end().ends_with("Press Enter to close."), "{text}");
}

/// With the test marker set, a CLI that would resolve a file outside the
/// temp folder (here a home in cargo's own target folder, nothing set for
/// Claude Code or Codex) refuses before reading or writing anything. No
/// answer is given, so even a broken guard couldn't write.
#[test]
fn the_cli_refuses_a_real_profile_path_under_test() {
    let home =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("fake-home-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    assert!(
        !home.starts_with(std::env::temp_dir()),
        "the fake home must be outside the temp folder"
    );
    let relay = temp_dir("guard");
    let relay = relay.join("bouncer-hook.exe");
    for extra in [&[][..], &["--codex"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_bouncer"))
            .args(["install-hooks", "--relay", relay.to_str().unwrap()])
            .args(extra)
            .env(bouncer_core::hooks::TEST_GUARD_ENV, "1")
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{extra:?}: {out:?}");
        assert!(
            err.contains("a test reached a real profile file"),
            "{extra:?}: {err}"
        );
        assert!(!String::from_utf8_lossy(&out.stdout).contains("Changes to"));
    }
    assert!(!home.join(".claude").exists() && !home.join(".codex").exists());
    let _ = fs::remove_dir_all(&home);
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
        let out = hermetic(&dir)
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
    let mut child = hermetic(&dir)
        .args(["install-hooks", "--relay", relay.to_str().unwrap()])
        .env("CLAUDE_CONFIG_DIR", &config)
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
