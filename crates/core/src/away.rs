//! "While you were away": notices the user coming back after a long idle
//! span, and sums up the activity log for that span.

use std::path::Path;
use std::time::Duration;

use crate::activity::Entry;

/// Idle this long, then input again, counts as coming back.
pub const AWAY: Duration = Duration::from_secs(10 * 60);
/// A wait on the user shorter than this isn't "stuck".
const STUCK: u64 = 60_000;
const MINUTE: u64 = 60_000;

/// Overrides `AWAY` in seconds, in debug builds only (for the manual check).
pub const AWAY_ENV: &str = "BOUNCER_AWAY_SECS";

/// `AWAY`, or `BOUNCER_AWAY_SECS` in a debug build.
pub fn limit() -> Duration {
    limit_with(std::env::var(AWAY_ENV).ok())
}

fn limit_with(over: Option<String>) -> Duration {
    over.filter(|_| cfg!(debug_assertions))
        .and_then(|v| v.parse().ok())
        .map_or(AWAY, Duration::from_secs)
}

/// Follows OS idle time and says when the user comes back.
#[derive(Default)]
pub struct Away {
    /// When the current long idle span began (ms since 1970).
    since: Option<u64>,
}

impl Away {
    /// Takes the time now and how long there has been no input. Returns when
    /// the idle span began, once, when input resumes after `limit` or more.
    pub fn update(&mut self, now: u64, idle: Duration, limit: Duration) -> Option<u64> {
        if idle >= limit {
            self.since
                .get_or_insert(now.saturating_sub(idle.as_millis() as u64));
            None
        } else {
            self.since.take()
        }
    }
}

/// How long since the user last touched the keyboard or mouse. `None` where
/// the OS can't say (no away summaries then).
#[cfg(windows)]
pub fn user_idle() -> Option<Duration> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid, sized LASTINPUTINFO.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return None;
    }
    // Both are 32-bit millisecond tick counts; wrapping_sub survives the 49-day wrap.
    let ticks = unsafe { GetTickCount() };
    Some(Duration::from_millis(u64::from(
        ticks.wrapping_sub(info.dwTime),
    )))
}

#[cfg(target_os = "macos")]
pub fn user_idle() -> Option<Duration> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event: u32) -> f64;
    }
    // kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType.
    // SAFETY: plain values in, a number out.
    let secs = unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) };
    (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn user_idle() -> Option<Duration> {
    None
}

/// The longest wait on the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Stuck {
    pub minutes: u64,
    pub project: String,
    /// The command, or the step ("editing main.rs").
    pub what: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub minutes: u64,
    pub files: usize,
    pub commands: usize,
    pub auto: usize,
    pub asked: usize,
    pub stuck: Option<Stuck>,
}

/// Sums up `entries` (oldest first) for the span `since..=now`. `None` if
/// nothing happened: no tool call and no request (sessions only starting,
/// stopping or ending are no news).
pub fn summarize(entries: &[Entry], since: u64, now: u64) -> Option<Summary> {
    let entries: Vec<&Entry> = entries
        .iter()
        .filter(|e| (since..=now).contains(&e.t))
        .collect();
    if entries.is_empty() {
        return None;
    }
    let is = |e: &Entry, kind: &str| e.kind == kind;
    let command = |e: &Entry| matches!(e.tool.as_deref(), Some("Bash" | "PowerShell"));
    let by_rule = |e: &Entry| e.how.as_deref() == Some("auto-allowed by rule");
    let mut files: Vec<&str> = entries
        .iter()
        .filter(|e| is(e, "PostToolUse"))
        .filter_map(|e| e.file.as_deref())
        .collect();
    files.sort_unstable();
    files.dedup();
    let commands = entries
        .iter()
        .filter(|e| is(e, "PreToolUse") && command(e))
        .count();
    let calls = entries.iter().filter(|e| is(e, "PreToolUse")).count();
    let requests = entries
        .iter()
        .filter(|e| is(e, "PermissionRequest"))
        .count();
    if calls == 0 && requests == 0 {
        return None;
    }
    let asked = requests.saturating_sub(entries.iter().filter(|e| by_rule(e)).count());

    // Each request not answered by a rule waits until the user answers it on
    // the card, or else its session's next hook event (the terminal prompt was
    // answered), or now. A Notification is only Claude Code saying it waits.
    let mut stuck: Option<(u64, &Entry)> = None;
    for (i, request) in entries.iter().enumerate() {
        if !is(request, "PermissionRequest") {
            continue;
        }
        let mut later = entries[i + 1..]
            .iter()
            .filter(|e| e.session == request.session);
        let next = later.clone().next();
        if next.is_some_and(|e| by_rule(e)) {
            continue;
        }
        let end = later
            .find(|e| !is(e, "Notification") && e.how.as_deref() != Some("asked in terminal"))
            .map_or(now, |e| e.t);
        let waited = end.saturating_sub(request.t);
        if waited >= STUCK && stuck.is_none_or(|(w, _)| waited > w) {
            stuck = Some((waited, request));
        }
    }
    Some(Summary {
        minutes: now.saturating_sub(since) / MINUTE,
        files: files.len(),
        commands,
        auto: calls.saturating_sub(asked),
        asked,
        stuck: stuck.map(|(waited, e)| Stuck {
            minutes: waited / MINUTE,
            project: folder(&e.project),
            what: what(e),
        }),
    })
}

fn folder(project: &str) -> String {
    Path::new(project)
        .file_name()
        .map_or(project.to_owned(), |f| f.to_string_lossy().into_owned())
}

/// "go test ./..." for a command, "editing main.rs" for any other step.
fn what(e: &Entry) -> String {
    if let Some(command) = e.label.strip_prefix("Running ") {
        return command.to_owned();
    }
    let mut chars = e.label.chars();
    chars
        .next()
        .map_or(String::new(), |c| c.to_lowercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coming_back_after_the_limit_reports_once() {
        let limit = Duration::from_secs(600);
        let mut away = Away::default();
        let min = |m: u64| m * MINUTE;
        assert_eq!(away.update(min(100), Duration::from_secs(30), limit), None);
        assert_eq!(away.update(min(110), Duration::from_secs(600), limit), None);
        assert_eq!(
            away.update(min(130), Duration::from_secs(1800), limit),
            None
        );
        // Input again: the span began 30 min before 130.
        assert_eq!(
            away.update(min(131), Duration::from_secs(2), limit),
            Some(min(100))
        );
        assert_eq!(away.update(min(132), Duration::from_secs(60), limit), None);
        // A short break isn't "away".
        assert_eq!(away.update(min(140), Duration::from_secs(500), limit), None);
        assert_eq!(away.update(min(141), Duration::from_secs(1), limit), None);
    }

    #[test]
    fn override_is_honored_only_in_debug_builds() {
        // CI also runs this in a release build.
        let over = limit_with(Some("60".into()));
        assert_eq!(over == Duration::from_secs(60), cfg!(debug_assertions));
        assert_eq!(over == AWAY, !cfg!(debug_assertions));
        assert_eq!(limit_with(None), AWAY);
        assert_eq!(
            limit_with(Some("soon".into())),
            AWAY,
            "unparsable: the real limit"
        );
    }

    #[test]
    fn idle_time_is_readable_here() {
        if cfg!(any(windows, target_os = "macos")) {
            assert!(user_idle().is_some());
        }
    }

    fn entry(t: u64, session: &str, kind: &str, tool: Option<&str>, label: &str) -> Entry {
        Entry {
            t,
            session: session.into(),
            project: format!("/code/{session}"),
            kind: kind.into(),
            tool: tool.map(str::to_owned),
            label: label.into(),
            file: None,
            how: None,
            added: None,
            removed: None,
        }
    }

    #[test]
    fn nothing_logged_means_no_summary() {
        assert_eq!(summarize(&[], 0, 1000), None);
        let old = entry(5, "a", "Stop", None, "");
        assert_eq!(summarize(&[old], 10, 1000), None);
    }

    /// Sessions that started, stopped or ended but did nothing: no tool call,
    /// no request. An all-zero summary would open the island for nothing.
    #[test]
    fn a_span_with_no_work_means_no_summary() {
        let quiet = [
            entry(20, "a", "SessionStart", None, ""),
            entry(30, "a", "Notification", None, ""),
            entry(40, "a", "Stop", None, ""),
            entry(50, "a", "SessionEnd", None, ""),
        ];
        assert_eq!(summarize(&quiet, 10, 1000), None);
    }

    /// A 30-minute span over two sessions, in the shape the desk logs real
    /// hook events (see `activity`): (seconds, session, kind, tool, label, file, how).
    #[rustfmt::skip]
    const SPAN: &[(u64, &str, &str, &str, &str, &str, &str)] = &[
        (0, "carbon", "Stop", "", "", "", ""), // before the span: not counted
        (60, "carbon", "UserPromptSubmit", "", "", "", ""),
        (70, "carbon", "PreToolUse", "Edit", "Editing main.go", "/code/carbon-scheduler/main.go", ""),
        (75, "carbon", "PostToolUse", "Edit", "Editing main.go", "/code/carbon-scheduler/main.go", ""),
        (90, "carbon", "PreToolUse", "Edit", "Editing sched.go", "/code/carbon-scheduler/sched.go", ""),
        (95, "carbon", "PostToolUse", "Edit", "Editing sched.go", "/code/carbon-scheduler/sched.go", ""),
        (120, "app", "PreToolUse", "Write", "Editing a.rs", "/code/app/a.rs", ""),
        (121, "app", "PostToolUse", "Write", "Editing a.rs", "/code/app/a.rs", ""),
        (130, "app", "PreToolUse", "Read", "Reading a.rs", "", ""),
        (131, "app", "PostToolUse", "Read", "Reading a.rs", "", ""),
        (200, "carbon", "PreToolUse", "Bash", "Running go build ./...", "", ""),
        (200, "carbon", "PermissionRequest", "Bash", "Running go build ./...", "", ""),
        (200, "carbon", "Answer", "Bash", "Running go build ./...", "", "auto-allowed by rule"),
        (230, "carbon", "PostToolUse", "Bash", "Running go build ./...", "", ""),
        (300, "app", "PreToolUse", "Bash", "Running cargo test", "", ""),
        (300, "app", "PermissionRequest", "Bash", "Running cargo test", "", ""),
        (330, "app", "Answer", "Bash", "Running cargo test", "", "you allowed"),
        (400, "app", "PostToolUse", "Bash", "Running cargo test", "", ""),
        (500, "app", "PreToolUse", "Bash", "Running git status", "", ""),
        (501, "app", "PostToolUse", "Bash", "Running git status", "", ""),
        (510, "app", "Stop", "", "", "", ""),
        (600, "carbon", "PreToolUse", "Edit", "Editing main.go", "/code/carbon-scheduler/main.go", ""),
        (605, "carbon", "PostToolUse", "Edit", "Editing main.go", "/code/carbon-scheduler/main.go", ""),
        (720, "carbon", "PreToolUse", "Bash", "Running go test ./...", "", ""),
        (720, "carbon", "PermissionRequest", "Bash", "Running go test ./...", "", ""),
        (726, "carbon", "Notification", "", "", "", ""),
        (820, "carbon", "Answer", "Bash", "Running go test ./...", "", "asked in terminal"),
        (1260, "carbon", "PostToolUse", "Bash", "Running go test ./...", "", ""),
        (1300, "carbon", "Stop", "", "", "", ""),
    ];

    fn span() -> Vec<Entry> {
        SPAN.iter()
            .map(|&(s, session, kind, tool, label, file, how)| Entry {
                t: s * 1000,
                session: session.into(),
                project: format!(
                    "/code/{}",
                    if session == "carbon" {
                        "carbon-scheduler"
                    } else {
                        "app"
                    }
                ),
                kind: kind.into(),
                tool: (!tool.is_empty()).then(|| tool.into()),
                label: label.into(),
                file: (!file.is_empty()).then(|| file.into()),
                how: (!how.is_empty()).then(|| how.into()),
                added: None,
                removed: None,
            })
            .collect()
    }

    #[test]
    fn a_thirty_minute_span_sums_up() {
        let summary = summarize(&span(), 30_000, 30 * MINUTE + 30_000).unwrap();
        assert_eq!(
            summary,
            Summary {
                minutes: 30,
                files: 3,    // main.go, sched.go, a.rs (main.go twice)
                commands: 4, // go build, cargo test, git status, go test
                auto: 7,     // 9 tool calls, 2 asked
                asked: 2,    // cargo test (card), go test (terminal); go build was a rule
                stuck: Some(Stuck {
                    minutes: 9, // 720 s → 1260 s; the Notification and Answer don't end it
                    project: "carbon-scheduler".into(),
                    what: "go test ./...".into(),
                }),
            }
        );
    }

    #[test]
    fn a_wait_still_open_lasts_until_now() {
        let entries: Vec<Entry> = span().into_iter().filter(|e| e.t < 1_000_000).collect();
        let summary = summarize(&entries, 30_000, 1_500_000).unwrap();
        // go test asked at 720 s and nothing since: stuck until now (1500 s).
        assert_eq!(summary.stuck.unwrap().minutes, 13);
        // Answered within a minute (cargo test, 30 s): not stuck.
        let quick: Vec<Entry> = span().into_iter().filter(|e| e.t <= 600_000).collect();
        assert_eq!(summarize(&quick, 30_000, 600_000).unwrap().stuck, None);
    }

    #[test]
    fn what_reads_like_the_prototype() {
        assert_eq!(
            what(&entry(0, "a", "x", None, "Running go test ./...")),
            "go test ./..."
        );
        assert_eq!(
            what(&entry(0, "a", "x", None, "Editing main.rs")),
            "editing main.rs"
        );
    }
}
