//! `bouncer install-hooks` / `bouncer uninstall-hooks`.
//!
//! Never writes silently: shows the diff, asks, writes a dated backup, then
//! replaces the file atomically. Refuses files that aren't valid JSON, and
//! gives up if the file changes while the user is deciding.

use std::fs::{self, File, Permissions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

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
        None => default_settings()?,
    };

    let before = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("can't read {}: {e}", path.display())),
    };
    let mut value = match &before {
        Some(text) => serde_json::from_str(text).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); not touching it",
                path.display()
            )
        })?,
        None if installing => serde_json::json!({}),
        None => return done("No settings file; nothing to remove."),
    };

    if installing {
        let relay = match relay {
            Some(p) => p,
            None => std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name(format!("bouncer-hook{}", std::env::consts::EXE_SUFFIX)),
        };
        if !relay.is_absolute() || !relay.is_file() {
            return Err(format!("relay not found at {}", relay.display()));
        }
        let relay = relay.to_str().ok_or("relay path is not valid UTF-8")?;
        hooks::install(&mut value, relay)?;
    } else {
        hooks::uninstall(&mut value);
    }

    let mut after = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    if before.as_ref().is_none_or(|t| t.ends_with('\n')) {
        after.push('\n');
    }
    if before.as_deref() == Some(after.as_str()) {
        return done("Nothing to change.");
    }

    println!("Changes to {}:\n", path.display());
    print!("{}", hooks::diff(before.as_deref().unwrap_or(""), &after));
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

    // The file may have changed while we waited (Claude Code writes it too).
    if fs::read_to_string(&path).ok() != before {
        return Err(format!("{} changed meanwhile; run again", path.display()));
    }
    if let Some(text) = &before {
        let backup = backup(&path, text)?;
        println!("Backup: {}", backup.display());
    }
    atomic_write(&path, &after).map_err(|e| format!("write {}: {e}", path.display()))?;
    done(&format!("Wrote {}.", path.display()))
}

fn done(message: &str) -> Result<(), String> {
    println!("{message}");
    Ok(())
}

fn default_settings() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join("settings.json"));
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .ok_or("can't find your home folder; pass --settings")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
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
    use std::time::Duration;

    #[test]
    fn stamps_utc_dates() {
        let at = |s| stamp(UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(at(0), "19700101-000000Z");
        assert_eq!(at(951_782_400), "20000229-000000Z");
        assert_eq!(at(1_790_812_799), "20260930-235959Z");
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
}
