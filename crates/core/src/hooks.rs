//! Adding and removing Bouncer's entries in a Claude Code settings file.
//!
//! Only hook commands whose file name is `bouncer-hook[.exe]` are ours. Install
//! adds one group per event; uninstall removes our commands, plus any group,
//! event list or `hooks` object that only became empty because of that. Key
//! order is kept (`serde_json` `preserve_order`), so install followed by
//! uninstall gives back the same JSON.

use std::ffi::OsString;
use std::path::PathBuf;

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

fn is_ours(hook: &Value) -> bool {
    let Some(command) = hook.get("command").and_then(Value::as_str) else {
        return false;
    };
    let name = command.rsplit(['/', '\\']).next().unwrap_or(command);
    name.eq_ignore_ascii_case("bouncer-hook") || name.eq_ignore_ascii_case("bouncer-hook.exe")
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

/// The user's Claude Code settings file, from this process's environment.
pub fn user_settings() -> Result<PathBuf, String> {
    settings_path(
        std::env::var_os("CLAUDE_CONFIG_DIR"),
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }),
    )
}

/// Whether any of our hook commands is in `settings`.
pub fn installed(settings: &Value) -> bool {
    settings["hooks"].as_object().is_some_and(|events| {
        events
            .values()
            .filter_map(Value::as_array)
            .flatten()
            .any(|group| {
                group["hooks"]
                    .as_array()
                    .is_some_and(|list| list.iter().any(is_ours))
            })
    })
}

/// Adds our hooks (replacing any old ones) pointing at the `relay` executable.
/// Runs exec form (`args`), so no shell ever parses the path.
pub fn install(settings: &mut Value, relay: &str) -> Result<(), String> {
    uninstall(settings);
    let root = settings
        .as_object_mut()
        .ok_or("the settings file is not a JSON object")?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("\"hooks\" is not an object")?;
    for &(event, timeout) in EVENTS {
        hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or(format!("\"hooks.{event}\" is not a list"))?
            .push(json!({
                "hooks": [{ "type": "command", "command": relay, "args": [], "timeout": timeout }]
            }));
    }
    Ok(())
}

/// Removes our hooks and only the containers that we emptied.
pub fn uninstall(settings: &mut Value) {
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
            list.retain(|hook| !is_ours(hook));
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
}
