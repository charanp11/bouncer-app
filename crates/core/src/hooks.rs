//! Adding and removing Bouncer's entries in a Claude Code settings file, or
//! in Codex's `hooks.json` (same `hooks` layout; `Agent` says which).
//!
//! Only hook commands whose file name is `bouncer-hook[.exe]` are ours (for
//! Codex: exactly the command line `codex_command` writes). Install
//! adds one group per event; uninstall removes our commands, plus any group,
//! event list or `hooks` object that only became empty because of that. Key
//! order is kept (`serde_json` `preserve_order`), so install followed by
//! uninstall gives back the same JSON.
//!
//! Writing is never silent (rule 6): `plan` gives the change, the caller shows
//! its diff and asks, then `Change::write` keeps a dated backup and replaces
//! the file atomically. The `bouncer` CLI and the app's Settings share it.

use std::ffi::OsString;
use std::fs::{self, File, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

/// Events Bouncer listens to, with the hook timeout in seconds. The permission
/// request timeout must stay above the relay's 110 s decision budget.
pub const EVENTS: &[(&str, u64)] = &[
    ("SessionStart", 5),
    ("SessionEnd", 5),
    ("UserPromptSubmit", 5),
    ("PreToolUse", 5),
    ("PostToolUse", 5),
    ("PostToolUseFailure", 5),
    ("PermissionRequest", 120),
    ("Notification", 5),
    ("Stop", 5),
];

/// Codex's events, with the hook timeout in seconds. Never change these:
/// Codex trusts a hook by its definition, so a new timeout (or command)
/// makes the user trust it again in `/hooks`. The permission request's stays
/// above the relay's 110 s budget; SessionEnd's is Codex's 3 s maximum.
pub const CODEX_EVENTS: &[(&str, u64)] = &[
    ("SessionStart", 5),
    ("SessionEnd", 3),
    ("UserPromptSubmit", 5),
    ("PreToolUse", 5),
    ("PostToolUse", 5),
    ("PermissionRequest", 120),
    ("Stop", 5),
];

/// Whose hooks file it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    ClaudeCode,
    Codex,
}

impl Agent {
    fn events(self) -> &'static [(&'static str, u64)] {
        match self {
            Agent::ClaudeCode => EVENTS,
            Agent::Codex => CODEX_EVENTS,
        }
    }

    fn is_ours(self, hook: &Value) -> bool {
        match self {
            Agent::ClaudeCode => is_ours(hook),
            Agent::Codex => is_ours_codex(hook),
        }
    }

    /// Our hook entry running `relay` with `timeout`.
    fn entry(self, relay: &str, timeout: u64) -> Value {
        match self {
            Agent::ClaudeCode => {
                json!({ "type": "command", "command": relay, "args": [], "timeout": timeout })
            }
            Agent::Codex => {
                let command = codex_command(relay, cfg!(windows));
                let mut entry =
                    json!({ "type": "command", "command": command, "timeout": timeout });
                if cfg!(windows) {
                    entry["commandWindows"] = command.into();
                }
                entry
            }
        }
    }
}

fn is_relay_name(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.eq_ignore_ascii_case("bouncer-hook") || name.eq_ignore_ascii_case("bouncer-hook.exe")
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(is_relay_name)
}

/// The quote characters PowerShell ends a single-quoted string with: `'`
/// and the curly ones it treats the same.
const PS_QUOTES: &[char] = &['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// Codex runs a hook's command through a shell: `powershell -Command` on
/// Windows (`windows`), `sh` elsewhere. The relay's path goes in single
/// quotes, the only quoting with no expansion: PowerShell ends one at any of
/// `PS_QUOTES`, each doubled inside; sh at `'`, written `'\''`. Then
/// `--agent codex`. Nothing in the path can end the quotes or run.
pub fn codex_command(relay: &str, windows: bool) -> String {
    if windows {
        let mut quoted = String::new();
        for c in relay.chars() {
            quoted.push(c);
            if PS_QUOTES.contains(&c) {
                quoted.push(c);
            }
        }
        format!("& '{quoted}' --agent codex")
    } else {
        format!("'{}' --agent codex", relay.replace('\'', r"'\''"))
    }
}

/// The relay path in a command `codex_command` wrote, in either form; `None`
/// for anything else (it must come back out exactly).
fn codex_relay(command: &str) -> Option<String> {
    let ps = command
        .strip_prefix("& '")
        .and_then(|c| c.strip_suffix("' --agent codex"))
        .map(|inner| {
            let mut path = String::new();
            let mut chars = inner.chars().peekable();
            while let Some(c) = chars.next() {
                path.push(c);
                if PS_QUOTES.contains(&c) && chars.peek() == Some(&c) {
                    chars.next();
                }
            }
            (path, true)
        });
    let sh = command
        .strip_prefix('\'')
        .and_then(|c| c.strip_suffix("' --agent codex"))
        .map(|inner| (inner.replace(r"'\''", "'"), false));
    let (path, windows) = ps.or(sh)?;
    (codex_command(&path, windows) == command).then_some(path)
}

fn is_ours_codex(hook: &Value) -> bool {
    ["command", "commandWindows"].iter().any(|key| {
        hook.get(*key)
            .and_then(Value::as_str)
            .and_then(codex_relay)
            .is_some_and(|path| is_relay_name(&path))
    })
}

/// `$CLAUDE_CONFIG_DIR/settings.json` (as Claude Code reads it), else
/// `~/.claude/settings.json`. Empty values count as unset, so an empty variable
/// never means "the current folder".
pub fn settings_path(
    config_dir: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, String> {
    if let Some(dir) = config_dir.filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir).join("settings.json"));
    }
    let home = home
        .filter(|h| !h.is_empty())
        .ok_or("can't find your home folder; pass --settings")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

/// `$CODEX_HOME/hooks.json` (as Codex reads it), else `~/.codex/hooks.json`.
/// Empty values count as unset.
pub fn codex_hooks_path(
    codex_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, String> {
    if let Some(dir) = codex_home.filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir).join("hooks.json"));
    }
    let home = home
        .filter(|h| !h.is_empty())
        .ok_or("can't find your home folder; pass --settings")?;
    Ok(PathBuf::from(home).join(".codex").join("hooks.json"))
}

/// Set (to anything) by tests that run Bouncer's programs: every real
/// profile file `user_file` resolves must then be inside the temp folder.
pub const TEST_GUARD_ENV: &str = "BOUNCER_TEST_GUARD";

/// The agent's user-level hooks file, from this process's environment: the
/// only place Bouncer resolves a real profile file (Claude Code's
/// `settings.json`, Codex's `hooks.json`; Codex's `config.toml` is never
/// read or written). In this crate's test build, or with `TEST_GUARD_ENV`
/// set, it panics unless the path is inside the temp folder, so no test can
/// ever reach the user's own settings (2026-10-21, PLAN roadmap h1).
pub fn user_file(agent: Agent) -> Result<PathBuf, String> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let path = match agent {
        Agent::ClaudeCode => settings_path(std::env::var_os("CLAUDE_CONFIG_DIR"), home)?,
        Agent::Codex => codex_hooks_path(std::env::var_os("CODEX_HOME"), home)?,
    };
    if cfg!(test) || std::env::var_os(TEST_GUARD_ENV).is_some() {
        guard_test_path(&path, &std::env::temp_dir());
    }
    Ok(path)
}

/// Panics unless `path` is inside `temp` (by its parts, case-insensitive on
/// Windows; any `..` refused).
fn guard_test_path(path: &Path, temp: &Path) {
    use std::path::Component;
    let parts = |p: &Path| -> Vec<String> {
        p.components()
            .filter(|c| !matches!(c, Component::CurDir))
            .map(|c| {
                let s = c.as_os_str().to_string_lossy().into_owned();
                if cfg!(windows) { s.to_lowercase() } else { s }
            })
            .collect()
    };
    let (path_parts, temp_parts) = (parts(path), parts(temp));
    let inside = path.is_absolute()
        && !path.components().any(|c| c == Component::ParentDir)
        && path_parts.len() > temp_parts.len()
        && path_parts.starts_with(&temp_parts);
    assert!(
        inside,
        "a test reached a real profile file: {} (only paths inside {} are allowed under test)",
        path.display(),
        temp.display()
    );
}

/// Whether any of our hook commands is in `settings`.
pub fn installed(settings: &Value) -> bool {
    installed_for(Agent::ClaudeCode, settings)
}

fn installed_for(agent: Agent, settings: &Value) -> bool {
    settings["hooks"].as_object().is_some_and(|events| {
        events
            .values()
            .filter_map(Value::as_array)
            .flatten()
            .any(|group| {
                group["hooks"]
                    .as_array()
                    .is_some_and(|list| list.iter().any(|h| agent.is_ours(h)))
            })
    })
}

/// Adds our hooks (replacing any old ones) pointing at the `relay` executable.
/// Runs exec form (`args`), so no shell ever parses the path.
pub fn install(settings: &mut Value, relay: &str) -> Result<(), String> {
    install_for(Agent::ClaudeCode, settings, relay)
}

/// As `install`, for `agent`. Codex's command goes through its shell,
/// quoted by `codex_command`.
pub fn install_for(agent: Agent, settings: &mut Value, relay: &str) -> Result<(), String> {
    uninstall_for(agent, settings);
    let root = settings
        .as_object_mut()
        .ok_or("the settings file is not a JSON object")?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("\"hooks\" is not an object")?;
    for &(event, timeout) in agent.events() {
        hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or(format!("\"hooks.{event}\" is not a list"))?
            .push(json!({ "hooks": [agent.entry(relay, timeout)] }));
    }
    Ok(())
}

/// Removes our hooks and only the containers that we emptied.
pub fn uninstall(settings: &mut Value) {
    uninstall_for(Agent::ClaudeCode, settings);
}

fn uninstall_for(agent: Agent, settings: &mut Value) {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    let had_events = !hooks.is_empty();
    let mut emptied = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let had_groups = !groups.is_empty();
        groups.retain_mut(|group| {
            let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let had_hooks = !list.is_empty();
            list.retain(|hook| !agent.is_ours(hook));
            !(had_hooks && list.is_empty())
        });
        if had_groups && groups.is_empty() {
            emptied.push(event.clone());
        }
    }
    for event in emptied {
        hooks.shift_remove(&event);
    }
    if had_events && hooks.is_empty() {
        settings
            .as_object_mut()
            .map(|root| root.shift_remove("hooks"));
    }
}

/// Line diff of `old` → `new`: changed lines with two lines of context.
pub fn diff(old: &str, new: &str) -> String {
    let ops = crate::diff::line_ops(old, new);
    let changed_near = |k: usize| {
        ops[k.saturating_sub(2)..(k + 3).min(ops.len())]
            .iter()
            .any(|(op, _)| *op != ' ')
    };
    let mut out = String::new();
    let mut gap = false;
    for (k, (op, line)) in ops.iter().enumerate() {
        if changed_near(k) {
            out += &format!("{op} {line}\n");
            gap = false;
        } else if !gap {
            out += "  ...\n";
            gap = true;
        }
    }
    out
}

/// How our hooks stand in a settings file, against the relay that should run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Every event runs this relay, with its timeout.
    Installed,
    /// Some events have none of our hooks (or an old timeout).
    Incomplete,
    /// Every event has ours, but some point at another relay (a moved app).
    Moved,
    /// None of ours.
    Missing,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Installed => "installed",
            Status::Incomplete => "incomplete",
            Status::Moved => "moved",
            Status::Missing => "missing",
        }
    }
}

/// Checks every event in `EVENTS` for our hook running `relay`.
pub fn status(settings: &Value, relay: &str) -> Status {
    status_for(Agent::ClaudeCode, settings, relay)
}

/// As `status`, for `agent`: each event must have exactly our entry.
pub fn status_for(agent: Agent, settings: &Value, relay: &str) -> Status {
    if !installed_for(agent, settings) {
        return Status::Missing;
    }
    let mut moved = false;
    for &(event, timeout) in agent.events() {
        let ours: Vec<&Value> = settings["hooks"][event]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|group| group["hooks"].as_array())
            .flatten()
            .filter(|hook| agent.is_ours(hook))
            .collect();
        let want = agent.entry(relay, timeout);
        let same_relay = |h: &&Value| want["command"] == h["command"];
        let exact = |h: &&Value| {
            want.as_object()
                .is_some_and(|w| w.iter().all(|(k, v)| h[k] == *v))
        };
        if ours.is_empty() {
            return Status::Incomplete;
        } else if !ours.iter().any(exact) {
            if ours.iter().all(same_relay) {
                return Status::Incomplete;
            }
            moved = true;
        }
    }
    if moved {
        Status::Moved
    } else {
        Status::Installed
    }
}

/// A change to a settings file, shown before anything is written (rule 6).
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    before: Option<String>,
    after: String,
}

impl Change {
    /// The changed lines with two lines of context, as `diff` gives them.
    pub fn diff(&self) -> String {
        diff(self.before.as_deref().unwrap_or(""), &self.after)
    }

    /// Writes the change: refused if the file changed since it was read
    /// (Claude Code writes it too), a dated backup of the old file first,
    /// then an atomic replace. Returns the backup's path, if there was a file.
    pub fn write(&self) -> Result<Option<PathBuf>, String> {
        let path = &self.path;
        if read(path)? != self.before {
            return Err(format!("{} changed meanwhile; try again", path.display()));
        }
        let backup = match &self.before {
            Some(text) => Some(backup(path, text)?),
            None => None,
        };
        atomic_write(path, &self.after).map_err(|e| format!("write {}: {e}", path.display()))?;
        Ok(backup)
    }
}

/// The file's text, or `None` if it doesn't exist.
fn read(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("can't read {}: {e}", path.display())),
    }
}

/// What installing our hooks (with `relay`) or removing them (`None`) would
/// change in the settings file at `path`. `Ok(None)`: nothing to change.
/// Refuses a file that isn't valid JSON, and a relay that isn't an absolute
/// path to a file.
pub fn plan(path: &Path, relay: Option<&Path>) -> Result<Option<Change>, String> {
    plan_for(Agent::ClaudeCode, path, relay)
}

/// As `plan`, for `agent`'s file.
pub fn plan_for(agent: Agent, path: &Path, relay: Option<&Path>) -> Result<Option<Change>, String> {
    let before = read(path)?;
    let mut value = match &before {
        Some(text) => serde_json::from_str(text).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); not touching it",
                path.display()
            )
        })?,
        None if relay.is_some() => json!({}),
        None => return Ok(None),
    };
    match relay {
        Some(relay) => {
            if !relay.is_absolute() || !relay.is_file() {
                return Err(format!("relay not found at {}", relay.display()));
            }
            let relay = relay.to_str().ok_or("relay path is not valid UTF-8")?;
            install_for(agent, &mut value, relay)?;
        }
        None => uninstall_for(agent, &mut value),
    }
    let mut after = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    if before.as_ref().is_none_or(|t| t.ends_with('\n')) {
        after.push('\n');
    }
    if before.as_deref() == Some(after.as_str()) {
        return Ok(None);
    }
    Ok(Some(Change {
        path: path.to_owned(),
        before,
        after,
    }))
}

/// `bouncer-hook[.exe]` next to the running program.
pub fn relay_beside_me() -> Result<PathBuf, String> {
    Ok(std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name(format!("bouncer-hook{}", std::env::consts::EXE_SUFFIX)))
}

fn file_name(path: &Path) -> Result<&str, String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("bad settings path {}", path.display()))
}

/// Saves `text` as `<file>.bouncer-backup-<UTC time>[-n]`, never overwriting,
/// with the same permissions as `path`.
fn backup(path: &Path, text: &str) -> Result<PathBuf, String> {
    let perms = perms_of(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let base = format!(
        "{}.bouncer-backup-{}",
        file_name(path)?,
        stamp(SystemTime::now())
    );
    for n in 1..100 {
        let name = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        let backup = path.with_file_name(name);
        match write_new(&backup, text, perms.clone()) {
            Ok(()) => return Ok(backup),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("backup {}: {e}", backup.display())),
        }
    }
    Err("too many backups this second".into())
}

/// The permissions of `path`, or `None` if it doesn't exist.
fn perms_of(path: &Path) -> io::Result<Option<Permissions>> {
    match fs::metadata(path) {
        Ok(meta) => Ok(Some(meta.permissions())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Creates `path` (failing if it exists) with `perms` set before any byte is
/// written, then syncs it to disk. Removes it again on error.
fn write_new(path: &Path, text: &str, perms: Option<Permissions>) -> io::Result<()> {
    let mut file = File::create_new(path)?;
    let result = perms
        .map_or(Ok(()), |p| file.set_permissions(p))
        .and_then(|()| file.write_all(text.as_bytes()))
        .and_then(|()| file.sync_all());
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(path);
    }
    result
}

/// Writes a temp file next to `path` (same permissions), then renames it over.
fn atomic_write(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_file_name(format!(
        "{}.bouncer-tmp-{}",
        file_name(path).map_err(io::Error::other)?,
        std::process::id()
    ));
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    write_new(&tmp, text, perms_of(path)?)?;
    let result = fs::rename(&tmp, path);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// UTC `YYYYMMDD-HHMMSSZ`.
fn stamp(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELAY: &str = r"C:\Program Files\Bouncer\bouncer-hook.exe";

    fn user_settings() -> Value {
        json!({
            "model": "opus",
            "hooks": {
                "Stop": [{ "hooks": [{ "type": "command", "command": "notify-send done" }] }],
                "PreToolUse": [{ "matcher": "Bash", "hooks": [] }]
            },
            "env": { "A": "1" }
        })
    }

    #[test]
    fn installed_sees_only_our_hooks() {
        let mut s = user_settings();
        assert!(!installed(&s));
        install(&mut s, RELAY).unwrap();
        assert!(installed(&s));
        uninstall(&mut s);
        assert!(!installed(&s));
        assert!(!installed(&json!([])));
        assert!(!installed(&json!({ "hooks": { "Stop": "x" } })));
    }

    #[test]
    fn install_adds_one_group_per_event_and_keeps_user_hooks() {
        let mut s = user_settings();
        install(&mut s, RELAY).unwrap();
        install(&mut s, RELAY).unwrap(); // idempotent
        for &(event, timeout) in EVENTS {
            let ours: Vec<_> = s["hooks"][event]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|g| g["hooks"].as_array().unwrap())
                .filter(|h| is_ours(h))
                .collect();
            assert_eq!(ours.len(), 1, "{event}");
            assert_eq!(ours[0]["command"], RELAY);
            assert_eq!(ours[0]["args"], json!([]));
            assert_eq!(ours[0]["timeout"], timeout);
        }
        assert_eq!(
            s["hooks"]["Stop"][0]["hooks"][0]["command"],
            "notify-send done"
        );
        assert_eq!(s["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    }

    /// The budgets nest: the card gives up first (WAIT), then the relay
    /// (110 s), and Claude Code's hook timeout last, so Claude Code never
    /// kills a relay that is still waiting for an answer.
    #[test]
    fn the_wait_budgets_nest_inside_the_hook_timeout() {
        let hook = EVENTS
            .iter()
            .find(|(e, _)| *e == "PermissionRequest")
            .map(|&(_, t)| std::time::Duration::from_secs(t))
            .unwrap();
        let relay = bouncer_relay::DECISION_BUDGET;
        let card = crate::approvals::WAIT;
        assert!(
            card + std::time::Duration::from_secs(5) <= relay,
            "{card:?} vs {relay:?}"
        );
        assert!(
            relay + std::time::Duration::from_secs(5) <= hook,
            "{relay:?} vs {hook:?}"
        );
    }

    /// Installing again after Bouncer moved replaces our entry (one per
    /// event, the new path); the user's hooks stay.
    #[test]
    fn reinstall_from_a_new_place_replaces_ours() {
        let mut s = user_settings();
        install(&mut s, RELAY).unwrap();
        let moved = "/Applications/Bouncer.app/Contents/MacOS/bouncer-hook";
        install(&mut s, moved).unwrap();
        for &(event, _) in EVENTS {
            let ours: Vec<_> = s["hooks"][event]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|g| g["hooks"].as_array().unwrap())
                .filter(|h| is_ours(h))
                .collect();
            assert_eq!(ours.len(), 1, "{event}");
            assert_eq!(ours[0]["command"], moved, "{event}");
        }
        assert_eq!(
            s["hooks"]["Stop"][0]["hooks"][0]["command"],
            "notify-send done"
        );
    }

    #[test]
    fn uninstall_restores_the_original_exactly() {
        // Known limit: an empty `"hooks": {}` the user had comes back removed.
        for original in [user_settings(), json!({})] {
            let mut s = original.clone();
            install(&mut s, RELAY).unwrap();
            uninstall(&mut s);
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                serde_json::to_string(&original).unwrap()
            );
        }
    }

    #[test]
    fn uninstall_leaves_lookalikes_alone() {
        let keep = json!({ "hooks": { "Stop": [{ "hooks": [
            { "type": "command", "command": "bouncer-hook-extra" },
            { "type": "command", "command": "/bin/not-bouncer-hook.sh" }
        ] }] } });
        let mut s = keep.clone();
        uninstall(&mut s);
        assert_eq!(s, keep);
        assert!(is_ours(
            &json!({ "command": "/usr/local/bin/bouncer-hook" })
        ));
        assert!(is_ours(&json!({ "command": r"C:\x\BOUNCER-HOOK.EXE" })));
    }

    #[test]
    fn install_refuses_odd_shapes() {
        for bad in [
            json!([]),
            json!({ "hooks": [] }),
            json!({ "hooks": { "Stop": {} } }),
        ] {
            assert!(install(&mut bad.clone(), RELAY).is_err(), "{bad}");
        }
    }

    #[test]
    fn diff_shows_changes_with_context() {
        let d = diff("a\nb\nc\nd\ne\nf\ng\n", "a\nb\nc\nd\nX\nf\ng\n");
        assert_eq!(d, "  ...\n  c\n  d\n- e\n+ X\n  f\n  g\n");
    }

    #[test]
    fn stamps_utc_dates() {
        let at = |s| stamp(UNIX_EPOCH + std::time::Duration::from_secs(s));
        assert_eq!(at(0), "19700101-000000Z");
        assert_eq!(at(951_782_400), "20000229-000000Z");
        assert_eq!(at(1_790_812_799), "20260930-235959Z");
    }

    #[test]
    fn settings_path_follows_claude_config_dir() {
        let path = |dir: Option<&str>, home: Option<&str>| {
            settings_path(dir.map(Into::into), home.map(Into::into))
        };
        let home = Path::new("h").join(".claude").join("settings.json");
        assert_eq!(
            path(Some("cfg"), Some("h")),
            Ok(Path::new("cfg").join("settings.json"))
        );
        assert_eq!(path(None, Some("h")), Ok(home.clone()));
        assert_eq!(path(Some(""), Some("h")), Ok(home));
        assert!(path(None, None).is_err());
        assert!(path(Some(""), Some("")).is_err());
    }

    #[test]
    fn write_new_applies_permissions_and_still_writes() {
        let dir = std::env::temp_dir().join(format!("bouncer-perms-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("original"), "").unwrap();
        let mut readonly = fs::metadata(dir.join("original")).unwrap().permissions();
        readonly.set_readonly(true);
        let file = dir.join("copy");
        write_new(&file, "text", Some(readonly)).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "text");
        let mut perms = fs::metadata(&file).unwrap().permissions();
        assert!(perms.readonly());
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(&file, perms).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn status_tells_installed_incomplete_moved_and_missing() {
        let mut s = user_settings();
        assert_eq!(status(&s, RELAY), Status::Missing);
        install(&mut s, RELAY).unwrap();
        assert_eq!(status(&s, RELAY), Status::Installed);
        assert_eq!(status(&s, r"D:\elsewhere\bouncer-hook.exe"), Status::Moved);
        // One event lost: incomplete, wherever the rest point.
        let mut lost = s.clone();
        lost["hooks"]
            .as_object_mut()
            .unwrap()
            .shift_remove("Notification");
        assert_eq!(status(&lost, RELAY), Status::Incomplete);
        // An old timeout on the permission hook: incomplete, re-install fixes it.
        let mut old = s.clone();
        for group in old["hooks"]["PermissionRequest"].as_array_mut().unwrap() {
            for hook in group["hooks"].as_array_mut().unwrap() {
                hook["timeout"] = json!(60);
            }
        }
        assert_eq!(status(&old, RELAY), Status::Incomplete);
        // Only a lookalike: missing.
        let fake =
            json!({ "hooks": { "Stop": [{ "hooks": [{ "command": "bouncer-hook-extra" }] }] } });
        assert_eq!(status(&fake, RELAY), Status::Missing);
    }

    /// Paths a user can have: a quote in the name (O'Brien), PowerShell's
    /// curly quotes, `$`, backticks, `;`, `&`, `%`, brackets, spaces.
    const ODD: &str = "C:\\Users\\O'Brien \u{2018}x\u{2019} \u{201A}\u{201B} $(calc) `n; & echo %PATH% [a] {b}\\bouncer-hook.exe";

    #[test]
    fn codex_commands_quote_the_path_for_each_shell() {
        assert_eq!(
            codex_command(r"C:\Users\O'Brien\Bouncer\bouncer-hook.exe", true),
            r"& 'C:\Users\O''Brien\Bouncer\bouncer-hook.exe' --agent codex"
        );
        assert_eq!(
            codex_command("C:\\a\u{2018}b\u{2019}\\bouncer-hook.exe", true),
            "& 'C:\\a\u{2018}\u{2018}b\u{2019}\u{2019}\\bouncer-hook.exe' --agent codex"
        );
        assert_eq!(
            codex_command("/Users/o'brien/bouncer-hook", false),
            r"'/Users/o'\''brien/bouncer-hook' --agent codex"
        );
        for windows in [true, false] {
            let command = codex_command(ODD, windows);
            assert_eq!(codex_relay(&command).as_deref(), Some(ODD), "{command}");
            assert!(is_ours_codex(&json!({ "command": command })));
        }
    }

    /// Only exactly what `codex_command` writes, running bouncer-hook, is
    /// ours: an unescaped quote, other arguments, another file or no
    /// arguments are the user's.
    #[test]
    fn codex_lookalikes_are_not_ours() {
        for command in [
            r"& 'C:\x\not-bouncer-hook.exe' --agent codex",
            r"& 'C:\a'b\bouncer-hook.exe' --agent codex",
            r"& 'C:\x\bouncer-hook.exe' --agent codex; calc",
            r"& 'C:\x\bouncer-hook.exe' --agent codexx",
            r"& 'C:\x\bouncer-hook.exe'",
            r"C:\x\bouncer-hook.exe --agent codex",
            r"'/a'b/bouncer-hook' --agent codex",
            "'/x/bouncer-hook.sh' --agent codex",
        ] {
            assert!(!is_ours_codex(&json!({ "command": command })), "{command}");
        }
        assert!(is_ours_codex(
            &json!({ "command": "x", "commandWindows": r"& 'C:\x\bouncer-hook.exe' --agent codex" })
        ));
    }

    /// The timeouts are part of the hook's definition, which Codex trusts:
    /// they never change by accident.
    #[test]
    fn codex_timeouts_are_fixed() {
        assert_eq!(
            CODEX_EVENTS,
            &[
                ("SessionStart", 5),
                ("SessionEnd", 3),
                ("UserPromptSubmit", 5),
                ("PreToolUse", 5),
                ("PostToolUse", 5),
                ("PermissionRequest", 120),
                ("Stop", 5)
            ]
        );
        let hook = CODEX_EVENTS
            .iter()
            .find(|(e, _)| *e == "PermissionRequest")
            .unwrap()
            .1;
        assert!(
            bouncer_relay::DECISION_BUDGET + std::time::Duration::from_secs(5)
                <= std::time::Duration::from_secs(hook)
        );
    }

    /// Install keeps the user's own Codex hooks, one entry per event (the
    /// Windows form with `commandWindows`); remove gives the file back; the
    /// status tells installed, incomplete, moved and missing.
    #[test]
    fn codex_install_status_and_uninstall() {
        let original = json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "notify done" }] }] } });
        let mut s = original.clone();
        assert_eq!(status_for(Agent::Codex, &s, ODD), Status::Missing);
        install_for(Agent::Codex, &mut s, ODD).unwrap();
        install_for(Agent::Codex, &mut s, ODD).unwrap();
        assert!(
            !installed(&s),
            "Claude Code's check doesn't see Codex's entries"
        );
        for &(event, timeout) in CODEX_EVENTS {
            let ours: Vec<_> = s["hooks"][event]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|g| g["hooks"].as_array().unwrap())
                .filter(|h| is_ours_codex(h))
                .collect();
            assert_eq!(ours.len(), 1, "{event}");
            assert_eq!(ours[0]["timeout"], timeout);
            assert_eq!(ours[0]["command"], codex_command(ODD, cfg!(windows)));
            assert_eq!(ours[0].get("commandWindows").is_some(), cfg!(windows));
        }
        assert_eq!(status_for(Agent::Codex, &s, ODD), Status::Installed);
        assert_eq!(
            status_for(Agent::Codex, &s, r"D:\other\bouncer-hook.exe"),
            Status::Moved
        );
        let mut old = s.clone();
        old["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"] = json!(60);
        assert_eq!(status_for(Agent::Codex, &old, ODD), Status::Incomplete);
        uninstall_for(Agent::Codex, &mut s);
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            serde_json::to_string(&original).unwrap()
        );
        // A Claude Code settings file's entries are never Codex's, and back.
        let mut claude = json!({});
        install(&mut claude, RELAY).unwrap();
        let before = claude.clone();
        uninstall_for(Agent::Codex, &mut claude);
        assert_eq!(claude, before);
    }

    /// Under test only temp paths pass: a home-folder file, the temp folder
    /// itself, a `..` escape or a relative path panic.
    #[test]
    fn the_test_guard_allows_only_temp_paths() {
        let temp = std::env::temp_dir();
        guard_test_path(&temp.join("bouncer-x").join("settings.json"), &temp);
        let home = std::env::home_dir().unwrap();
        for bad in [
            home.join(".claude").join("settings.json"),
            home.join(".codex").join("hooks.json"),
            temp.clone(),
            temp.join("..").join("settings.json"),
            PathBuf::from("settings.json"),
        ] {
            let refused = std::panic::catch_unwind(|| guard_test_path(&bad, &temp)).is_err();
            assert!(refused, "{}", bad.display());
        }
    }

    /// This crate's own test build resolves no real profile file: with the
    /// test process's environment `user_file` panics unless it points into
    /// the temp folder.
    #[test]
    fn user_file_refuses_real_profile_paths_under_test() {
        for agent in [Agent::ClaudeCode, Agent::Codex] {
            let resolved = std::panic::catch_unwind(|| user_file(agent));
            if let Ok(Ok(path)) = &resolved {
                assert!(path.starts_with(std::env::temp_dir()), "{}", path.display());
            } else {
                assert!(resolved.is_err(), "refused by a panic, not an error");
            }
        }
    }

    #[test]
    fn codex_hooks_path_follows_codex_home() {
        let path = |dir: Option<&str>, home: Option<&str>| {
            codex_hooks_path(dir.map(Into::into), home.map(Into::into))
        };
        let home = Path::new("h").join(".codex").join("hooks.json");
        assert_eq!(
            path(Some("c"), Some("h")),
            Ok(Path::new("c").join("hooks.json"))
        );
        assert_eq!(path(None, Some("h")), Ok(home.clone()));
        assert_eq!(path(Some(""), Some("h")), Ok(home));
        assert!(path(None, None).is_err());
    }

    /// Settings' path: plan, show, write. A backup of the old file, the new
    /// one in place; a file changed after the plan is never overwritten.
    #[test]
    fn plan_then_write_backs_up_and_refuses_a_file_changed_meanwhile() {
        let dir = std::env::temp_dir().join(format!("bouncer-plan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let relay = dir.join(format!("bouncer-hook{}", std::env::consts::EXE_SUFFIX));
        let file = dir.join("settings.json");
        let original = "{\n  \"model\": \"opus\"\n}\n";
        fs::write(&file, original).unwrap();

        // No relay there yet: refused.
        assert!(
            plan(&file, Some(&relay))
                .unwrap_err()
                .contains("relay not found")
        );
        fs::write(&relay, "").unwrap();
        assert!(plan(&file, Some(Path::new("bouncer-hook"))).is_err());

        let change = plan(&file, Some(&relay)).unwrap().unwrap();
        assert!(change.diff().contains("+ "));
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            original,
            "planning writes nothing"
        );
        let backup = change.write().unwrap().unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), original);
        let written: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(status(&written, relay.to_str().unwrap()), Status::Installed);
        assert_eq!(
            plan(&file, Some(&relay)).unwrap(),
            None,
            "nothing more to do"
        );

        // Remove, but the file changes before the write: refused, untouched.
        let remove = plan(&file, None).unwrap().unwrap();
        fs::write(&file, original).unwrap();
        assert!(remove.write().unwrap_err().contains("changed meanwhile"));
        assert_eq!(fs::read_to_string(&file).unwrap(), original);

        // Not JSON: refused, untouched. No file: nothing to remove.
        fs::write(&file, "{ not json").unwrap();
        assert!(
            plan(&file, Some(&relay))
                .unwrap_err()
                .contains("not valid JSON")
        );
        assert_eq!(plan(&dir.join("none.json"), None).unwrap(), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}
