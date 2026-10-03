//! The activity log: what happened in each session, for the away summary.
//!
//! One JSON line per event, one file per UTC day (`activity-YYYY-MM-DD.jsonl`)
//! in a private folder next to `rules.toml`. Only the fields of [`Entry`] are
//! written, redacted: never tool outputs, file contents or prompts. A file or folder
//! another account can write is refused, a day file stops at `MAX_DAY`, and a
//! line that doesn't parse (a crash mid-write) is skipped when reading.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::approvals::step;
use crate::event::Event;
use crate::redact::redact;
use crate::rules::{create_private_dir, trust};

/// Largest day file; once full, nothing more is logged that day.
pub const MAX_DAY: u64 = 10 * 1024 * 1024;
/// Longest `label` and `file` kept, in characters.
const MAX_LABEL: usize = 200;
const MAX_FILE: usize = 1000;
const DAY_MS: u64 = 86_400_000;
/// Day files older than this many days are deleted.
pub const KEEP_DAYS: u64 = 30;
/// Tools whose `file` is kept (the away summary counts changed files).
const EDIT_TOOLS: [&str; 4] = ["Edit", "MultiEdit", "Write", "NotebookEdit"];

/// One logged event. `kind` is the hook name, or `Answer` for how a
/// permission request ended (`how`).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Milliseconds since 1970.
    pub t: u64,
    pub session: String,
    pub project: String,
    pub kind: String,
    pub tool: Option<String>,
    /// The step, e.g. "Running cargo test"; "" for events without a tool.
    pub label: String,
    /// Full path, edit tools only.
    pub file: Option<String>,
    pub how: Option<String>,
}

impl Entry {
    /// What gets logged for a hook event.
    pub fn of(event: &Event) -> Entry {
        let file = event
            .tool
            .as_deref()
            .filter(|t| EDIT_TOOLS.contains(t))
            .and_then(|_| {
                let input = event.input.as_ref()?;
                let path = input
                    .get("file_path")
                    .or_else(|| input.get("notebook_path"))?;
                path.as_str().map(str::to_owned)
            });
        Entry {
            t: ms(event.time),
            session: event.session.clone(),
            project: event.project.clone(),
            kind: event.kind.clone(),
            tool: event.tool.clone(),
            label: if event.tool.is_some() {
                step(&whole_command_redacted(event))
            } else {
                String::new()
            },
            file,
            how: None,
        }
    }

    /// How a permission request ended (`how` as in the session history).
    pub fn answer(event: &Event, how: &str, t: u64) -> Entry {
        Entry {
            t,
            kind: "Answer".into(),
            file: None,
            how: Some(how.into()),
            ..Entry::of(event)
        }
    }

    /// The line as written: text redacted, then the label and file cut to
    /// their limits.
    fn line(&self) -> String {
        let cut = |s: &str, max: usize| redact(s).chars().take(max).collect::<String>();
        let mut line = json!({
            "t": self.t,
            "session": self.session,
            "project": redact(&self.project),
            "kind": self.kind,
            "tool": self.tool,
            "label": cut(&self.label, MAX_LABEL),
            "file": self.file.as_deref().map(|f| cut(f, MAX_FILE)),
            "how": self.how,
        })
        .to_string();
        line.push('\n');
        line
    }

    fn parse(line: &str) -> Option<Entry> {
        let v: Value = serde_json::from_str(line).ok()?;
        let text = |key| v.get(key)?.as_str().map(str::to_owned);
        Some(Entry {
            t: v.get("t")?.as_u64()?,
            session: text("session")?,
            project: text("project")?,
            kind: text("kind")?,
            tool: text("tool"),
            label: text("label")?,
            file: text("file"),
            how: text("how"),
        })
    }
}

/// The event with its whole command redacted, so the step's first line can't
/// keep part of a secret that spans lines.
fn whole_command_redacted(event: &Event) -> Event {
    let mut event = event.clone();
    if let Some(Value::String(command)) = event.input.as_mut().and_then(|i| i.get_mut("command")) {
        *command = redact(command);
    }
    event
}

/// Milliseconds since 1970.
pub fn ms(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The day file's name for a UTC day number (days since 1970).
fn name(day: u64) -> String {
    // Days to civil date (Howard Hinnant's algorithm).
    let z = day as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("activity-{y:04}-{m:02}-{d:02}.jsonl")
}

/// The open day file.
struct Day {
    n: u64,
    /// `None` when logging is off for the day (see `note`).
    file: Option<File>,
    len: u64,
}

pub struct Log {
    dir: PathBuf,
    day: Option<Day>,
    /// Why nothing is being logged today, for the island.
    note: Option<String>,
}

impl Log {
    pub fn new(dir: PathBuf) -> Log {
        Log {
            dir,
            day: None,
            note: None,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Why logging is off, if it is.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Appends one entry to its day's file, as one write.
    pub fn write(&mut self, entry: &Entry) {
        let n = entry.t / DAY_MS;
        if self.day.as_ref().is_none_or(|d| d.n != n) {
            self.open(n);
        }
        let Some(day) = self.day.as_mut() else { return };
        let Some(file) = day.file.as_mut() else {
            return;
        };
        let line = entry.line();
        if day.len + line.len() as u64 > MAX_DAY {
            day.file = None;
            self.note = Some("History is full for today (10 MB); it starts again tomorrow.".into());
            return;
        }
        match file.write_all(line.as_bytes()) {
            Ok(()) => day.len += line.len() as u64,
            Err(e) => {
                day.file = None;
                self.note = Some(format!("History is off for today: {e}."));
            }
        }
    }

    /// Opens day `n`'s file for appending, after checking that only the user
    /// can write it and its folder.
    fn open(&mut self, n: u64) {
        let path = self.dir.join(name(n));
        let opened = (|| -> Result<(File, u64), String> {
            create_private_dir(&self.dir).map_err(|e| e.to_string())?;
            trust::check(&self.dir)?;
            let mut options = File::options();
            options.append(true).create(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let file = options.open(&path).map_err(|e| e.to_string())?;
            trust::check(&path)?;
            let len = file.metadata().map_err(|e| e.to_string())?.len();
            Ok((file, len))
        })();
        let (file, len, note) = match opened {
            Ok((file, len)) if len < MAX_DAY => (Some(file), len, None),
            Ok(_) => (
                None,
                0,
                Some("History is full for today (10 MB); it starts again tomorrow.".into()),
            ),
            Err(e) => (None, 0, Some(format!("History is off for today: {e}."))),
        };
        self.day = Some(Day { n, file, len });
        self.note = note;
        self.prune(n);
    }

    /// Deletes day files more than `KEEP_DAYS` before day `today`.
    fn prune(&self, today: u64) {
        let oldest = name(today.saturating_sub(KEEP_DAYS));
        for file in day_files(&self.dir) {
            if file
                .file_name()
                .is_some_and(|f| *f.to_string_lossy() < *oldest)
            {
                let _ = fs::remove_file(file);
            }
        }
    }

    /// "Wipe history": deletes every day file. Logging goes on afresh.
    pub fn wipe(&mut self) -> std::io::Result<()> {
        self.day = None;
        self.note = None;
        day_files(&self.dir).iter().try_for_each(fs::remove_file)
    }
}

/// Every day file in `dir` (`activity-….jsonl`).
fn day_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|f| {
                let f = f.to_string_lossy();
                f.starts_with("activity-") && f.ends_with(".jsonl")
            })
        })
        .collect()
}

/// Entries with `since <= t <= until` from the day files in `dir`, oldest
/// first. Files someone else can write and lines that don't parse are skipped.
pub fn read(dir: &Path, since: u64, until: u64) -> Vec<Entry> {
    let mut out = Vec::new();
    // Older day files are deleted, so a longer span reads no more than this.
    let first = (since / DAY_MS).max((until / DAY_MS).saturating_sub(KEEP_DAYS));
    for n in first..=until / DAY_MS {
        let path = dir.join(name(n));
        if trust::check(&path).is_err() {
            continue;
        }
        let mut bytes = Vec::new();
        if File::open(&path)
            .and_then(|f| f.take(MAX_DAY).read_to_end(&mut bytes))
            .is_err()
        {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        out.extend(
            text.lines()
                .filter_map(Entry::parse)
                .filter(|e| (since..=until).contains(&e.t)),
        );
    }
    out.sort_by_key(|e| e.t);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::rules::tests::temp_dir;
    use serde_json::json;
    use std::time::{Duration, UNIX_EPOCH};

    pub(crate) fn event(kind: &str, tool: Option<&str>, input: Value, t: u64) -> Event {
        Event {
            agent: "claude-code",
            session: "s1".into(),
            project: "/p".into(),
            kind: kind.into(),
            tool: tool.map(str::to_owned),
            input: Some(input),
            time: UNIX_EPOCH + Duration::from_millis(t),
        }
    }

    #[test]
    fn day_names_are_utc_dates() {
        assert_eq!(name(0), "activity-1970-01-01.jsonl");
        assert_eq!(name(19_723), "activity-2024-01-01.jsonl");
        assert_eq!(name(19_782), "activity-2024-02-29.jsonl");
        assert_eq!(name(20_729), "activity-2026-10-03.jsonl");
    }

    #[test]
    fn only_the_listed_fields_are_stored() {
        let content = "super secret file body";
        let e = event(
            "PostToolUse",
            Some("Write"),
            json!({ "file_path": "/p/a.txt", "content": content }),
            1_000,
        );
        let entry = Entry::of(&e);
        assert_eq!(entry.label, "Editing a.txt");
        assert_eq!(entry.file.as_deref(), Some("/p/a.txt"));
        let line = entry.line();
        assert!(!line.contains(content));
        let keys: Vec<String> = serde_json::from_str::<Value>(&line)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            keys,
            [
                "t", "session", "project", "kind", "tool", "label", "file", "how"
            ]
        );
        // Only the first line of a command, cut to MAX_LABEL.
        let long = format!("{}\nrm -rf /", "x".repeat(500));
        let e = event("PreToolUse", Some("Bash"), json!({ "command": long }), 1);
        let parsed = Entry::parse(&Entry::of(&e).line()).unwrap();
        assert_eq!(parsed.label.chars().count(), MAX_LABEL);
        assert!(!parsed.label.contains("rm -rf"));
        assert_eq!(parsed.file, None);
        // Prompts and other text never reach the line.
        let e = event(
            "UserPromptSubmit",
            None,
            json!({ "prompt": "my password is hunter2" }),
            1,
        );
        assert!(!Entry::of(&e).line().contains("hunter2"));
    }

    #[test]
    fn writes_and_reads_back_skipping_bad_lines() {
        let dir = temp_dir("log").join("history");
        let mut log = Log::new(dir.clone());
        let day = 20_729 * DAY_MS;
        let entries: Vec<Entry> = (0..3)
            .map(|i| {
                Entry::of(&event(
                    "PreToolUse",
                    Some("Bash"),
                    json!({ "command": format!("echo {i}") }),
                    day + i * 1000,
                ))
            })
            .collect();
        for e in &entries {
            log.write(e);
        }
        assert_eq!(log.note(), None);
        assert_eq!(read(&dir, day, day + 10_000), entries);
        assert_eq!(read(&dir, day + 1000, day + 1000), entries[1..2]);
        // A crash mid-line and junk: skipped, the rest still read.
        let path = dir.join(name(20_729));
        let mut f = File::options().append(true).open(&path).unwrap();
        f.write_all(b"not json\n{\"t\": 5, \"session\"").unwrap();
        drop(f);
        assert_eq!(read(&dir, day, day + 10_000), entries);
        // A new day goes to a new file.
        let next = Entry::of(&event("Stop", None, json!({}), day + DAY_MS));
        let mut log = Log::new(dir.clone());
        log.write(&next);
        assert!(dir.join(name(20_730)).exists());
        assert_eq!(read(&dir, day, day + DAY_MS).len(), 4);
    }

    #[test]
    fn no_fake_secret_reaches_the_file() {
        let dir = temp_dir("secrets").join("history");
        let mut log = Log::new(dir.clone());
        let fakes = crate::redact::tests::fakes();
        for (i, (_, text)) in fakes.iter().enumerate() {
            let t = 20_729 * DAY_MS + i as u64;
            for (tool, input) in [
                ("Bash", json!({ "command": text })),
                ("PowerShell", json!({ "command": text })),
                (
                    "Write",
                    json!({ "file_path": format!("/p/{text}"), "content": text }),
                ),
            ] {
                let mut e = event("PreToolUse", Some(tool), input, t);
                e.project = format!("/p/{text}");
                log.write(&Entry::of(&e));
                log.write(&Entry::answer(&e, "you allowed", t));
            }
        }
        assert_eq!(log.note(), None);
        let written = fs::read_to_string(dir.join(name(20_729))).unwrap();
        assert!(written.lines().count() >= fakes.len() * 6);
        for (secret, _) in &fakes {
            // The file holds JSON, so compare with the secret as JSON escapes it.
            let escaped = serde_json::to_string(secret).unwrap();
            let escaped = escaped.trim_matches('"');
            assert!(!written.contains(escaped), "{secret} on disk");
        }
    }

    #[test]
    fn old_days_are_deleted_and_wipe_deletes_all() {
        let dir = temp_dir("retention").join("history");
        create_private_dir(&dir).unwrap();
        let today = 20_729;
        for n in [
            today - 40,
            today - KEEP_DAYS - 1,
            today - KEEP_DAYS,
            today - 1,
        ] {
            fs::write(dir.join(name(n)), "").unwrap();
        }
        fs::write(dir.join("notes.txt"), "not ours").unwrap();
        let mut log = Log::new(dir.clone());
        log.write(&Entry::of(&event("Stop", None, json!({}), today * DAY_MS)));
        let mut left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                name(today - KEEP_DAYS),
                name(today - 1),
                name(today),
                "notes.txt".into()
            ]
        );
        log.wipe().unwrap();
        assert_eq!(day_files(&dir), Vec::<PathBuf>::new());
        assert!(dir.join("notes.txt").exists(), "only our files");
        assert_eq!(read(&dir, 0, (today + 1) * DAY_MS), []);
        // A huge span reads only the days that can exist (this hung once).
        assert_eq!(read(&dir, 0, u64::MAX), []);
        // Logging goes on after a wipe.
        log.write(&Entry::of(&event("Stop", None, json!({}), today * DAY_MS)));
        assert_eq!(read(&dir, 0, (today + 1) * DAY_MS).len(), 1);
    }

    #[test]
    fn a_full_day_stops_logging_with_a_note() {
        let dir = temp_dir("full").join("history");
        let day = 20_729 * DAY_MS;
        create_private_dir(&dir).unwrap();
        let path = dir.join(name(20_729));
        fs::write(&path, vec![b'\n'; (MAX_DAY - 100) as usize]).unwrap();
        let mut log = Log::new(dir.clone());
        let e = Entry::of(&event(
            "PreToolUse",
            Some("Bash"),
            json!({ "command": "ls" }),
            day,
        ));
        log.write(&e);
        assert!(log.note().unwrap().contains("full"));
        assert_eq!(fs::metadata(&path).unwrap().len(), MAX_DAY - 100);
        // The next day logs again.
        let mut e = e;
        e.t += DAY_MS;
        log.write(&e);
        assert_eq!(log.note(), None);
        assert_eq!(read(&dir, e.t, e.t), [e]);
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private_and_shared_folders_refused() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_dir("private");
        let dir = root.join("history");
        let mut log = Log::new(dir.clone());
        let e = Entry::of(&event("Stop", None, json!({}), 0));
        log.write(&e);
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join(name(0))), 0o600);
        // A folder others can write: nothing written, a note instead.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();
        let mut log = Log::new(dir.clone());
        log.write(&Entry { t: DAY_MS, ..e });
        assert!(log.note().unwrap().contains("others can write"));
        assert!(!dir.join(name(1)).exists());
    }

    #[cfg(windows)]
    #[test]
    fn shared_folders_are_refused() {
        let dir = temp_dir("shared").join("history");
        create_private_dir(&dir).unwrap();
        // Give Everyone write access to the folder.
        let status = std::process::Command::new("icacls")
            .arg(&dir)
            .args(["/grant", "*S-1-1-0:(OI)(CI)W"])
            .output()
            .unwrap();
        assert!(status.status.success());
        let mut log = Log::new(dir.clone());
        log.write(&Entry::of(&event("Stop", None, json!({}), 0)));
        assert!(
            log.note().unwrap().contains("another account"),
            "{:?}",
            log.note()
        );
        assert!(!dir.join(name(0)).exists());
    }
}
