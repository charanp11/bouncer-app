//! `bouncer install-hooks` / `bouncer uninstall-hooks`.
//!
//! Never writes silently: shows the diff, asks, writes a dated backup, then
//! replaces the file atomically. Refuses files that aren't valid JSON, and
//! gives up if the file changes while the user is deciding. The same code
//! (`hooks::plan`, `Change::write`) runs Settings' install and remove.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use bouncer_core::hooks;

const USAGE: &str = "usage: bouncer install-hooks [--settings FILE] [--relay PATH]
       bouncer uninstall-hooks [--settings FILE]

FILE defaults to ~/.claude/settings.json ($CLAUDE_CONFIG_DIR/settings.json if set).
PATH defaults to bouncer-hook next to this program.";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bouncer: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    let installing = match command.as_str() {
        "install-hooks" => true,
        "uninstall-hooks" => false,
        _ => return Err(USAGE.into()),
    };
    let (mut settings, mut relay) = (None, None);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or(USAGE)?;
        match flag.as_str() {
            "--settings" => settings = Some(PathBuf::from(value)),
            "--relay" if installing => relay = Some(PathBuf::from(value)),
            _ => return Err(USAGE.into()),
        }
    }
    let path = match settings {
        Some(p) => p,
        None => hooks::user_settings()?,
    };
    if !installing && !path.exists() {
        return done("No settings file; nothing to remove.");
    }
    let relay = match (installing, relay) {
        (false, _) => None,
        (true, Some(p)) => Some(p),
        (true, None) => Some(hooks::relay_beside_me()?),
    };
    let Some(change) = hooks::plan(&path, relay.as_deref())? else {
        return done("Nothing to change.");
    };

    println!("Changes to {}:\n", path.display());
    print!("{}", change.diff());
    print!("\nWrite these changes? [y/N] ");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return done("Nothing written.");
    }
    if let Some(backup) = change.write()? {
        println!("Backup: {}", backup.display());
    }
    done(&format!("Wrote {}.", path.display()))
}

fn done(message: &str) -> Result<(), String> {
    println!("{message}");
    Ok(())
}
