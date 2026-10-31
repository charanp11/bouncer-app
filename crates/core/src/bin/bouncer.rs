//! `bouncer install-hooks` / `bouncer uninstall-hooks`, for Claude Code,
//! (`--codex`) Codex, (`--copilot`) GitHub Copilot CLI or (`--cursor`)
//! Cursor CLI.
//!
//! Never writes silently: shows the diff, asks, writes a dated backup, then
//! replaces the file atomically. Refuses files that aren't valid JSON, and
//! gives up if the file changes while the user is deciding. The same code
//! (`hooks::plan_for`, `Change::write`) runs Settings' install and remove.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use bouncer_core::hooks::{self, Agent};

const USAGE: &str =
    "usage: bouncer install-hooks [--codex | --copilot | --cursor] [--settings FILE] [--relay PATH]
       bouncer uninstall-hooks [--codex | --copilot | --cursor] [--settings FILE]

FILE defaults to ~/.claude/settings.json ($CLAUDE_CONFIG_DIR/settings.json if set);
with --codex, to ~/.codex/hooks.json ($CODEX_HOME/hooks.json if set);
with --copilot, to ~/.copilot/hooks/bouncer.json ($COPILOT_HOME/hooks/bouncer.json if set);
with --cursor, to ~/.cursor/hooks.json.
PATH defaults to bouncer-hook next to this program.";

/// The uninstaller runs `uninstall-hooks --uninstalling` in its own console:
/// Claude Code's file, then Codex's, then Copilot's, then Cursor's.
const UNINSTALLING: &str =
    "Bouncer is being uninstalled. Remove its hooks from Claude Code's settings?
Installing Bouncer again next? Answer N to keep them.
";
const UNINSTALLING_CODEX: &str = "Bouncer is being uninstalled. Remove its hooks from Codex?
Installing Bouncer again next? Answer N to keep them.
";
const UNINSTALLING_COPILOT: &str =
    "Bouncer is being uninstalled. Remove its hooks from Copilot CLI?
Installing Bouncer again next? Answer N to keep them.
";
const UNINSTALLING_CURSOR: &str = "Bouncer is being uninstalled. Remove its hooks from Cursor CLI?
Installing Bouncer again next? Answer N to keep them.
";
/// Codex runs a new or changed hook only once the user trusts it.
const CODEX_TRUST: &str = "Codex asks you to trust these hooks once: open Codex and choose\n\
     \"Trust all and continue\" (or review them in /hooks).";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Only after uninstall-hooks: the uninstaller's console, which closes
    // as soon as this ends, so it waits for Enter.
    let uninstalling = args.first().is_some_and(|c| c == "uninstall-hooks")
        && args.iter().any(|a| a == "--uninstalling");
    args.retain(|a| !uninstalling || a != "--uninstalling");
    let code = match run(args, uninstalling) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bouncer: {e}");
            ExitCode::FAILURE
        }
    };
    if uninstalling {
        print!("\nPress Enter to close.");
        let _ = io::stdout().flush();
        let _ = io::stdin().lock().read_line(&mut String::new());
    }
    code
}

fn run(args: Vec<String>, uninstalling: bool) -> Result<(), String> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    let installing = match command.as_str() {
        "install-hooks" => true,
        "uninstall-hooks" => false,
        _ => return Err(USAGE.into()),
    };
    let (mut settings, mut relay, mut agent) = (None, None, Agent::ClaudeCode);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let named = match flag.as_str() {
            "--codex" => Some(Agent::Codex),
            "--copilot" => Some(Agent::Copilot),
            "--cursor" => Some(Agent::Cursor),
            _ => None,
        };
        if let Some(named) = named {
            if agent != Agent::ClaudeCode {
                return Err(USAGE.into());
            }
            agent = named;
            continue;
        }
        let value = it.next().ok_or(USAGE)?;
        match flag.as_str() {
            "--settings" => settings = Some(PathBuf::from(value)),
            "--relay" if installing => relay = Some(PathBuf::from(value)),
            _ => return Err(USAGE.into()),
        }
    }
    if uninstalling {
        // A file given: only that one. Else every agent's user file, each
        // even if an earlier step fails.
        if settings.is_some() {
            return one(agent, settings, false, None, true);
        }
        let claude = one(Agent::ClaudeCode, None, false, None, true);
        println!();
        let codex = one(Agent::Codex, None, false, None, true);
        println!();
        let copilot = one(Agent::Copilot, None, false, None, true);
        println!();
        let cursor = one(Agent::Cursor, None, false, None, true);
        return claude.and(codex).and(copilot).and(cursor);
    }
    let relay = match (installing, relay) {
        (false, _) => None,
        (true, Some(p)) => Some(p),
        (true, None) => Some(hooks::relay_to_install()?),
    };
    one(agent, settings, installing, relay, false)
}

/// Shows and, on a yes, writes one agent's change.
fn one(
    agent: Agent,
    settings: Option<PathBuf>,
    installing: bool,
    relay: Option<PathBuf>,
    uninstalling: bool,
) -> Result<(), String> {
    let path = match settings {
        Some(p) => p,
        None => hooks::user_file(agent)?,
    };
    let what = match agent {
        Agent::ClaudeCode => "settings file",
        Agent::Codex => "Codex hooks file",
        Agent::Copilot => "Copilot CLI hooks file",
        Agent::Cursor => "Cursor CLI hooks file",
    };
    if !installing && !path.exists() {
        return done(&format!("No {what}; nothing to remove."));
    }
    let Some(change) = hooks::plan_for(agent, &path, relay.as_deref())? else {
        return done("Nothing to change.");
    };

    println!("Changes to {}:\n", path.display());
    print!("{}", change.diff());
    // Right above the question: a long diff scrolls the top out of view.
    if uninstalling {
        print!(
            "\n{}",
            match agent {
                Agent::ClaudeCode => UNINSTALLING,
                Agent::Codex => UNINSTALLING_CODEX,
                Agent::Copilot => UNINSTALLING_COPILOT,
                Agent::Cursor => UNINSTALLING_CURSOR,
            }
        );
    }
    print!("\nWrite these changes? [y/N] ");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        if uninstalling && matches!(agent, Agent::Copilot | Agent::Cursor) {
            // Every Copilot and Cursor hook command ends in `exit 0`: a
            // missing relay is no hook failure, so the agent carries on.
            println!(
                "Kept. Without Bouncer, {} carries on as usual; the entries\n\
                 stay in {} until you remove them.",
                match agent {
                    Agent::Cursor => "Cursor CLI",
                    _ => "Copilot CLI",
                },
                path.display()
            );
        } else if uninstalling {
            println!(
                "Kept. Without Bouncer, {} will report a missing hook until\n\
                 the entries running bouncer-hook.exe are removed from {}.",
                match agent {
                    Agent::Codex => "Codex",
                    _ => "Claude Code",
                },
                path.display()
            );
        }
        return done("Nothing written.");
    }
    if let Some(backup) = change.write()? {
        println!("Backup: {}", backup.display());
    }
    done(&format!("Wrote {}.", path.display()))?;
    if installing && agent == Agent::Codex {
        println!("{CODEX_TRUST}");
    }
    Ok(())
}

fn done(message: &str) -> Result<(), String> {
    println!("{message}");
    Ok(())
}
