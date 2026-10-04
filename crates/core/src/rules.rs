//! The rules file: which permission requests Bouncer may answer for the user.
//!
//! `rules.toml` lives in the user's config folder. It is read strictly: over
//! 64 KB, an unknown key, a wrong type or a rule Bouncer can't check refuses
//! the whole file, and so does a file (or folder) someone else can write. A
//! refused file falls back to the built-in rules in observe mode, which never
//! answer for the user, and the error is shown in the island.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use toml::de::{DeTable, DeValue};

use crate::shell;

/// Largest rules file Bouncer reads.
pub const MAX_SIZE: u64 = 64 * 1024;

/// Tools whose input Bouncer understands (the paths they touch), so a rule
/// may allow them inside the project.
pub const PATH_TOOLS: [&str; 8] = [
    "Read",
    "Edit",
    "MultiEdit",
    "Write",
    "NotebookEdit",
    "Grep",
    "Glob",
    "LS",
];

/// Written on first start. Observe mode: rules only say what they would do.
pub const DEFAULTS: &str = r#"# Bouncer rules. Bouncer checks this file every few seconds and shows every
# change in the island. Anything a rule doesn't allow is asked as usual, and a
# risky request (with its reason) is always asked, whatever the rules say.
#
# mode = "observe": rules never answer for you; a card says what they would do.
# mode = "auto":    requests a rule allows are approved without asking.
mode = "observe"

# A Bash command is allowed when every part of it (`a && b`, `a | b`, ...)
# starts with the words of a `command` rule, and every path it names is inside
# the project. Commands with `$(...)`, backticks, variables, redirections to
# files and similar are never auto-allowed.
#
# A `tool` rule allows Read, Edit, MultiEdit, Write, NotebookEdit, Grep, Glob or
# LS when the paths it touches are inside the project.

[[allow]]
tool = "Read"

[[allow]]
tool = "Grep"

[[allow]]
tool = "Glob"

[[allow]]
tool = "LS"

[[allow]]
command = "ls"

[[allow]]
command = "pwd"

[[allow]]
command = "cat"

[[allow]]
command = "head"

[[allow]]
command = "tail"

[[allow]]
command = "wc"

[[allow]]
command = "grep"

[[allow]]
command = "which"

[[allow]]
command = "git status"

[[allow]]
command = "git diff"

[[allow]]
command = "git log"

[[allow]]
command = "git show"

[[allow]]
command = "git branch --show-current"

[[allow]]
command = "cargo check"

[[allow]]
command = "cargo build"

[[allow]]
command = "cargo test"

[[allow]]
command = "cargo clippy"

[[allow]]
command = "cargo fmt --check"

[[allow]]
command = "npm test"

[[allow]]
command = "npm run build"
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Observe,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A command starting with these words (all of them, if `exact`).
    Command(Vec<String>),
    /// One of `PATH_TOOLS`.
    Tool(String),
}

/// One `[[allow]]` rule. Hand-written rules are usually just a command
/// prefix or a tool; "Always allow" writes exact, project-scoped ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub kind: Kind,
    /// The command must be exactly these words, nothing added.
    pub exact: bool,
    /// A tool rule for this one file only (absolute, resolved).
    pub path: Option<PathBuf>,
    /// Only in this project folder (absolute, resolved).
    pub project: Option<PathBuf>,
}

impl Rule {
    /// A plain prefix rule, as in the defaults.
    pub fn command(words: Vec<String>) -> Rule {
        Rule {
            kind: Kind::Command(words),
            exact: false,
            path: None,
            project: None,
        }
    }

    /// A tool rule for any path inside any project, as in the defaults.
    pub fn tool(name: &str) -> Rule {
        Rule {
            kind: Kind::Tool(name.into()),
            exact: false,
            path: None,
            project: None,
        }
    }

    /// How the rule reads in the island, e.g. `cargo test`, `Read`, or
    /// `Write hello.txt in bouncer-playground`.
    pub fn label(&self) -> String {
        let name = |p: &Path| {
            p.file_name()
                .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into())
        };
        let mut label = match &self.kind {
            Kind::Command(words) => words.join(" "),
            Kind::Tool(tool) => tool.clone(),
        };
        if let Some(path) = &self.path {
            label = format!("{label} {}", name(path));
        }
        if let Some(project) = &self.project {
            label = format!("{label} in {}", name(project));
        }
        label
    }

    /// Exactly what "Always allow" appends to the file.
    pub fn to_toml(&self) -> String {
        let mut out = String::from("[[allow]]\n");
        match &self.kind {
            Kind::Command(words) => {
                let words: Vec<String> = words.iter().map(|w| shell_quoted(w)).collect();
                out += &format!("command = {}\n", quoted(&words.join(" ")));
            }
            Kind::Tool(tool) => out += &format!("tool = {}\n", quoted(tool)),
        }
        if let Some(path) = &self.path {
            out += &format!("path = {}\n", quoted(&path.to_string_lossy()));
        }
        if self.exact {
            out += "exact = true\n";
        }
        if let Some(project) = &self.project {
            out += &format!("project = {}\n", quoted(&project.to_string_lossy()));
        }
        out
    }
}

/// A word the shell parser reads back as itself: as is when it's plain,
/// else single-quoted.
fn shell_quoted(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._/:=@%+,-".contains(c));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    pub mode: Mode,
    pub allow: Vec<Rule>,
}

impl Rules {
    /// The shipped rules, in observe mode.
    pub fn builtin() -> Rules {
        let mut rules = parse(DEFAULTS).expect("built-in rules parse");
        rules.mode = Mode::Observe;
        rules
    }
}

/// The rules in use, and why the file was refused if it was.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub rules: Rules,
    pub error: Option<String>,
}

/// Overrides the rules file in debug and test builds only (dev runs keep it
/// out of the real config folder); release builds ignore it.
pub const RULES_ENV: &str = "BOUNCER_RULES";

/// Where the rules file lives: `%APPDATA%\Bouncer` on Windows,
/// `~/Library/Application Support/Bouncer` on macOS (next to the socket).
pub fn path() -> Option<PathBuf> {
    path_with(std::env::var_os(RULES_ENV))
}

fn path_with(over: Option<std::ffi::OsString>) -> Option<PathBuf> {
    if let Some(path) = over.filter(|v| cfg!(debug_assertions) && !v.is_empty()) {
        return Some(path.into());
    }
    #[cfg(windows)]
    let dir = PathBuf::from(std::env::var_os("APPDATA").filter(|v| !v.is_empty())?);
    #[cfg(target_os = "macos")]
    let dir = std::env::home_dir()?.join("Library/Application Support");
    #[cfg(not(any(windows, target_os = "macos")))]
    let dir = std::env::home_dir()?.join(".config");
    Some(dir.join("Bouncer").join("rules.toml"))
}

/// When the file last changed, to notice edits cheaply.
pub fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Reads the rules file, creating it with the defaults if it's missing. Any
/// problem gives the built-in rules in observe mode plus the reason.
pub fn load(path: &Path) -> Loaded {
    let result = create_if_missing(path)
        .map_err(|e| format!("can't create {}: {e}", path.display()))
        .and_then(|()| read(path))
        .and_then(|text| parse(&text));
    match result {
        Ok(rules) => Loaded { rules, error: None },
        Err(error) => Loaded {
            rules: Rules::builtin(),
            error: Some(error),
        },
    }
}

/// Appends `rule` to the file, after checking the result still parses to
/// exactly the old rules plus this one. Written to a temp file and renamed.
pub fn add(path: &Path, rule: &Rule) -> Result<(), String> {
    let text = read(path)?;
    let old = parse(&text)?;
    let mut new_text = text.clone();
    if !new_text.is_empty() && !new_text.ends_with('\n') {
        new_text.push('\n');
    }
    new_text.push('\n');
    new_text += &rule.to_toml();
    let new = parse(&new_text).map_err(|e| format!("can't add the rule: {e}"))?;
    let mut want = old.allow.clone();
    want.push(rule.clone());
    if new.mode != old.mode || new.allow != want {
        return Err("can't add the rule: the file wouldn't read back as expected".into());
    }
    replace(path, &new_text).map_err(|e| format!("can't save {}: {e}", path.display()))
}

fn create_if_missing(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path).is_ok() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        create_private_dir(dir)?;
    }
    match write_new(path, DEFAULTS) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        other => other,
    }
}

#[cfg(unix)]
pub(crate) fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
pub(crate) fn create_private_dir(dir: &Path) -> io::Result<()> {
    // The folder inherits the user-only access list of %APPDATA%.
    fs::create_dir_all(dir)
}

/// Creates `path` (never overwriting) readable and writable by the user only.
fn write_new(path: &Path, text: &str) -> io::Result<()> {
    let mut options = File::options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    let result = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(path);
    }
    result
}

/// Replaces `path` atomically with a file only the user can write.
pub(crate) fn replace(path: &Path, text: &str) -> io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".tmp-{}", std::process::id()));
    let tmp = path.with_file_name(name);
    write_new(&tmp, text)?;
    let result = fs::rename(&tmp, path);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// The file's text, if it is ours, private, at most `MAX_SIZE` and UTF-8.
pub(crate) fn read(path: &Path) -> Result<String, String> {
    let name = path.display();
    trust::check(path).map_err(|e| format!("{name} ignored: {e}"))?;
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(MAX_SIZE + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("can't read {name}: {e}"))?;
    if bytes.len() as u64 > MAX_SIZE {
        return Err(format!("{name} ignored: it is over 64 KB"));
    }
    String::from_utf8(bytes).map_err(|_| format!("{name} ignored: it isn't UTF-8 text"))
}

/// 1-based line of a byte offset.
fn line(text: &str, span: Option<Range<usize>>) -> String {
    match span {
        Some(span) => {
            let n = text.as_bytes()[..span.start.min(text.len())]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();
            format!("line {}: ", n + 1)
        }
        None => String::new(),
    }
}

/// Parses rules text strictly; errors name the line.
pub fn parse(text: &str) -> Result<Rules, String> {
    if text.len() as u64 > MAX_SIZE {
        return Err("rules.toml ignored: it is over 64 KB".into());
    }
    let fail =
        |span: Range<usize>, msg: String| format!("rules.toml {}{msg}", line(text, Some(span)));
    let table = DeTable::parse(text)
        .map_err(|e| format!("rules.toml {}{}", line(text, e.span()), e.message()))?;
    let mut rules = Rules {
        mode: Mode::Observe,
        allow: Vec::new(),
    };
    for (key, value) in table.get_ref().iter() {
        match (key.get_ref().as_ref(), value.get_ref()) {
            ("mode", DeValue::String(s)) if s == "observe" => rules.mode = Mode::Observe,
            ("mode", DeValue::String(s)) if s == "auto" => rules.mode = Mode::Auto,
            ("mode", _) => {
                return Err(fail(
                    value.span(),
                    r#"`mode` must be "observe" or "auto""#.into(),
                ));
            }
            ("allow", DeValue::Array(items)) => {
                for item in items.iter() {
                    let DeValue::Table(entry) = item.get_ref() else {
                        return Err(fail(
                            item.span(),
                            "each `allow` entry must be an [[allow]] table".into(),
                        ));
                    };
                    rules.allow.push(rule(entry, item.span(), &fail)?);
                }
            }
            ("allow", _) => {
                return Err(fail(
                    value.span(),
                    "`allow` must be a list of [[allow]] tables".into(),
                ));
            }
            (other, _) => {
                return Err(fail(key.span(), format!("unknown key `{other}`")));
            }
        }
    }
    Ok(rules)
}

/// One `[[allow]]` table: exactly one of `command` or `tool`; optionally
/// `exact` (with `command`), `path` (with `tool`) and `project`.
fn rule(
    entry: &DeTable,
    span: Range<usize>,
    fail: &impl Fn(Range<usize>, String) -> String,
) -> Result<Rule, String> {
    let mut kind = None;
    let mut exact = None;
    let mut path = None;
    let mut project = None;
    for (key, value) in entry.iter() {
        let name = key.get_ref().as_ref();
        let text = || match value.get_ref() {
            DeValue::String(text) => Ok(text.as_ref()),
            _ => Err(fail(value.span(), format!("`{name}` must be a string"))),
        };
        let full_path = |text: &str| {
            let p = PathBuf::from(text);
            match p.is_absolute() {
                true => Ok(p),
                false => Err(fail(value.span(), format!("`{name}` must be a full path"))),
            }
        };
        match name {
            "command" | "tool" if kind.is_some() => {
                return Err(fail(
                    key.span(),
                    "an [[allow]] rule has either `command` or `tool`, not both".into(),
                ));
            }
            "command" => {
                let parsed = shell::parse(text()?);
                match parsed.commands.as_slice() {
                    [c] if parsed.issues.is_empty()
                        && c.writes.is_empty()
                        && c.reads.is_empty() =>
                    {
                        kind = Some(Kind::Command(c.words.clone()));
                    }
                    _ => {
                        return Err(fail(
                            value.span(),
                            "`command` must be plain words, like \"cargo test\"".into(),
                        ));
                    }
                }
            }
            "tool" => {
                let text = text()?;
                if !PATH_TOOLS.contains(&text) {
                    return Err(fail(
                        value.span(),
                        format!("`tool` must be one of {}", PATH_TOOLS.join(", ")),
                    ));
                }
                kind = Some(Kind::Tool(text.into()));
            }
            "exact" => match value.get_ref() {
                DeValue::Boolean(b) => exact = Some((*b, key.span())),
                _ => return Err(fail(value.span(), "`exact` must be true or false".into())),
            },
            "path" => path = Some((full_path(text()?)?, key.span())),
            "project" => project = Some(full_path(text()?)?),
            other => return Err(fail(key.span(), format!("unknown key `{other}`"))),
        }
    }
    let kind =
        kind.ok_or_else(|| fail(span, "an [[allow]] rule needs `command` or `tool`".into()))?;
    match (&kind, &exact, &path) {
        (Kind::Tool(_), Some((_, at)), _) => Err(fail(
            at.clone(),
            "`exact` goes with `command`, not `tool`".into(),
        )),
        (Kind::Command(_), _, Some((_, at))) => Err(fail(
            at.clone(),
            "`path` goes with `tool`, not `command`".into(),
        )),
        _ => Ok(Rule {
            kind,
            exact: exact.is_some_and(|(b, _)| b),
            path: path.map(|(p, _)| p),
            project,
        }),
    }
}

/// A TOML basic string.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            c if c.is_control() => out += &format!("\\u{:04X}", c as u32),
            c => out.push(c),
        }
    }
    out + "\""
}

/// Who can write the rules file. Only the user (plus SYSTEM and
/// Administrators on Windows) may; a symlink is refused.
pub(crate) mod trust {
    use super::*;

    pub fn check(path: &Path) -> Result<(), String> {
        let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
        for p in [Some(path), dir].into_iter().flatten() {
            let meta = fs::symlink_metadata(p).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err(format!("{} is a link", p.display()));
            }
            owner_only(p, &meta)?;
        }
        Ok(())
    }

    #[cfg(unix)]
    fn owner_only(p: &Path, meta: &fs::Metadata) -> Result<(), String> {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != bouncer_relay::unix::uid() {
            return Err(format!("{} isn't owned by you", p.display()));
        }
        if meta.mode() & 0o022 != 0 {
            return Err(format!("others can write {}", p.display()));
        }
        Ok(())
    }

    #[cfg(windows)]
    fn owner_only(p: &Path, _: &fs::Metadata) -> Result<(), String> {
        let mine = bouncer_relay::win::current_user_sid().ok_or("can't read your user ID")?;
        let sddl =
            win::sddl(p).map_err(|e| format!("can't read who may change {}: {e}", p.display()))?;
        sddl_owner_only(&sddl, &mine).map_err(|e| format!("{} {e}", p.display()))
    }

    #[cfg(not(any(unix, windows)))]
    fn owner_only(_: &Path, _: &fs::Metadata) -> Result<(), String> {
        Err("can't check who may change it on this system".into())
    }

    /// Checks a security descriptor in SDDL text form: owned by `mine`,
    /// SYSTEM or Administrators, and no other account may write, delete or
    /// re-permission it.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn sddl_owner_only(sddl: &str, mine: &str) -> Result<(), String> {
        // SDDL writes the built-in Administrator account (RID 500) as `LA`,
        // so that's "mine" when the user is that account.
        let me = |sid: &str| sid == mine || (sid == "LA" && mine.ends_with("-500"));
        let trusted =
            |sid: &str| me(sid) || matches!(sid, "SY" | "BA" | "S-1-5-18" | "S-1-5-32-544");
        let section = |tag: &str| {
            let start = sddl.find(tag)? + tag.len();
            let end = ["O:", "G:", "D:", "S:"]
                .iter()
                .filter_map(|t| sddl[start..].find(t))
                .min()
                .map_or(sddl.len(), |i| start + i);
            Some(&sddl[start..end])
        };
        let owner = section("O:").ok_or("has no owner")?;
        if !trusted(owner) {
            return Err(format!("is owned by someone else ({owner})"));
        }
        let dacl = section("D:").ok_or("has no access list")?;
        let (flags, aces) = dacl.split_once('(').unwrap_or((dacl, ""));
        if flags.contains("NO_ACCESS_CONTROL") {
            return Err("is open to everyone".into());
        }
        for ace in format!("({aces}").split('(').filter(|a| !a.is_empty()) {
            let fields: Vec<&str> = ace.trim_end_matches(')').split(';').collect();
            let [kind, flags, rights, _, _, sid, ..] = fields[..] else {
                return Err("has an access list Bouncer can't read".into());
            };
            let allows = matches!(kind, "A" | "OA" | "XA" | "ZA");
            let inherit_only = flags.contains("IO");
            if allows && !inherit_only && !trusted(sid) && writes(rights) {
                return Err(format!("can be changed by another account ({sid})"));
            }
        }
        Ok(())
    }

    /// Whether SDDL access rights include writing, appending, deleting or
    /// changing permissions or owner. Unknown codes count as writing.
    fn writes(rights: &str) -> bool {
        const WRITE: u32 = 0x2
            | 0x4
            | 0x40
            | 0x1_0000
            | 0x4_0000
            | 0x8_0000
            | 0x200_0000
            | 0x1000_0000
            | 0x4000_0000;
        let mask = if let Some(hex) = rights.strip_prefix("0x").or(rights.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).unwrap_or(u32::MAX)
        } else if !rights.len().is_multiple_of(2) || !rights.is_ascii() {
            u32::MAX
        } else {
            (0..rights.len()).step_by(2).fold(0, |mask, i| {
                mask | match &rights[i..i + 2] {
                    "GA" => 0x1000_0000,
                    "GR" => 0x8000_0000,
                    "GW" => 0x4000_0000,
                    "GX" => 0x2000_0000,
                    "RC" => 0x2_0000,
                    "SD" => 0x1_0000,
                    "WD" => 0x4_0000,
                    "WO" => 0x8_0000,
                    "FA" => 0x1F_01FF,
                    "FR" => 0x12_0089,
                    "FW" => 0x12_0116,
                    "FX" => 0x12_00A0,
                    "CC" => 0x1,
                    "DC" => 0x2,
                    "LC" => 0x4,
                    "SW" => 0x8,
                    "RP" => 0x10,
                    "WP" => 0x20,
                    "DT" => 0x40,
                    "LO" => 0x80,
                    "CR" => 0x100,
                    _ => u32::MAX,
                }
            })
        };
        mask & WRITE != 0
    }

    #[cfg(windows)]
    mod win {
        use std::os::windows::ffi::OsStrExt;
        use std::path::Path;

        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        };
        use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION};

        /// The owner and access list of a file or folder as SDDL text.
        pub fn sddl(path: &Path) -> std::io::Result<String> {
            let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            let info = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
            // SAFETY: `name` is NUL-terminated; out-pointers are valid; the
            // descriptor and string are LocalAlloc'd and freed on every path.
            unsafe {
                let mut sd = std::ptr::null_mut();
                let err = GetNamedSecurityInfoW(
                    name.as_ptr(),
                    SE_FILE_OBJECT,
                    info,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut sd,
                );
                if err != 0 {
                    return Err(std::io::Error::from_raw_os_error(err as i32));
                }
                let mut text = std::ptr::null_mut();
                let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(
                    sd,
                    SDDL_REVISION_1,
                    info,
                    &mut text,
                    std::ptr::null_mut(),
                );
                let err = std::io::Error::last_os_error();
                LocalFree(sd);
                if ok == 0 {
                    return Err(err);
                }
                let len = (0..).take_while(|&i| *text.add(i) != 0).count();
                let sddl = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
                LocalFree(text.cast());
                Ok(sddl)
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh private folder under the system temp folder.
    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("bouncer-rules-{name}-{nanos}"));
        create_private_dir(&dir).unwrap();
        dir
    }

    fn err(text: &str) -> String {
        parse(text).unwrap_err()
    }

    #[test]
    fn override_is_honored_only_in_debug_builds() {
        let over = path_with(Some("x-rules.toml".into()));
        assert_eq!(over == Some("x-rules.toml".into()), cfg!(debug_assertions));
        assert!(path_with(Some("".into())).is_some_and(|p| p.ends_with("Bouncer/rules.toml")));
    }

    #[test]
    fn defaults_parse_in_observe_mode() {
        let rules = parse(DEFAULTS).unwrap();
        assert_eq!(rules.mode, Mode::Observe);
        assert!(
            rules
                .allow
                .contains(&Rule::command(vec!["git".into(), "status".into()]))
        );
        assert!(rules.allow.contains(&Rule::tool("Read")));
        assert_eq!(Rules::builtin(), rules);
    }

    #[test]
    fn strict_parsing_names_the_line() {
        assert_eq!(
            err("mode = \"auto\"\nfoo = 1\n"),
            "rules.toml line 2: unknown key `foo`"
        );
        assert_eq!(
            err("mode = \"yes\""),
            r#"rules.toml line 1: `mode` must be "observe" or "auto""#
        );
        assert_eq!(
            err("mode = 1"),
            r#"rules.toml line 1: `mode` must be "observe" or "auto""#
        );
        assert_eq!(
            err("allow = \"ls\""),
            "rules.toml line 1: `allow` must be a list of [[allow]] tables"
        );
        assert_eq!(
            err("allow = [1]"),
            "rules.toml line 1: each `allow` entry must be an [[allow]] table"
        );
        assert_eq!(
            err("\n[[allow]]\ncommand = 5\n"),
            "rules.toml line 3: `command` must be a string"
        );
        assert_eq!(
            err("[[allow]]\ncommand = \"ls\"\ntool = \"Read\"\n"),
            "rules.toml line 3: an [[allow]] rule has either `command` or `tool`, not both"
        );
        assert_eq!(
            err("[[allow]]\n\n[[allow]]\nwhat = \"ls\"\n"),
            "rules.toml line 1: an [[allow]] rule needs `command` or `tool`"
        );
        assert!(
            err("[[allow]]\ntool = \"Bash\"\n")
                .starts_with("rules.toml line 2: `tool` must be one of Read,")
        );
        for bad in ["ls && rm x", "ls $(x)", "ls > x", "FOO=1 ls", "", "ls; pwd"] {
            assert_eq!(
                err(&format!("[[allow]]\ncommand = {}\n", quoted(bad))),
                "rules.toml line 2: `command` must be plain words, like \"cargo test\"",
                "{bad}"
            );
        }
        assert!(err("mode = \"auto\"\n[[allow]\n").starts_with("rules.toml line 2: "));
        assert!(err(&"#".repeat(MAX_SIZE as usize + 1)).contains("over 64 KB"));
    }

    #[test]
    fn rules_read_back_from_their_toml() {
        for rule in [
            Rule::command(vec!["cargo".into(), "test".into()]),
            Rule::command(vec!["./build.sh".into(), "--all".into()]),
            Rule::tool("Edit"),
        ] {
            let rules = parse(&rule.to_toml()).unwrap();
            assert_eq!(rules.allow, [rule]);
        }
    }

    #[test]
    fn scoped_rules_read_back_exactly() {
        let project = std::env::temp_dir().join("Full Time").join("my proj");
        let words = |s: &[&str]| s.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        for rule in [
            Rule {
                kind: Kind::Command(words(&["git", "diff", r"src\main.rs", "it's", "a b", ""])),
                exact: true,
                path: None,
                project: Some(project.clone()),
            },
            Rule {
                kind: Kind::Tool("Write".into()),
                exact: false,
                path: Some(project.join("hello.txt")),
                project: Some(project.clone()),
            },
        ] {
            let text = rule.to_toml();
            assert_eq!(parse(&text).unwrap().allow, [rule], "{text}");
        }
    }

    #[test]
    fn scoped_rule_keys_are_checked() {
        let full = std::env::temp_dir().to_string_lossy().replace('\\', "\\\\");
        let ok = format!("[[allow]]\ncommand = \"ls\"\nexact = true\nproject = \"{full}\"\n");
        assert!(parse(&ok).unwrap().allow[0].exact);
        for (text, want) in [
            (
                "[[allow]]\ncommand = \"ls\"\nexact = \"yes\"\n",
                "line 3: `exact` must be true or false",
            ),
            (
                "[[allow]]\ntool = \"Read\"\nexact = true\n",
                "line 3: `exact` goes with `command`, not `tool`",
            ),
            (
                &format!("[[allow]]\ncommand = \"ls\"\npath = \"{full}\"\n"),
                "line 3: `path` goes with `tool`, not `command`",
            ),
            (
                "[[allow]]\ntool = \"Read\"\npath = \"src/main.rs\"\n",
                "line 3: `path` must be a full path",
            ),
            (
                "[[allow]]\ncommand = \"ls\"\nproject = \"proj\"\n",
                "line 3: `project` must be a full path",
            ),
            (
                "[[allow]]\ncommand = \"ls\"\nproject = 5\n",
                "line 3: `project` must be a string",
            ),
            (
                "[[allow]]\ncommand = \"ls\"\nscope = \"x\"\n",
                "line 3: unknown key `scope`",
            ),
        ] {
            assert_eq!(err(text), format!("rules.toml {want}"), "{text}");
        }
    }

    #[test]
    fn missing_file_is_created_with_defaults() {
        let path = temp_dir("create").join("Bouncer").join("rules.toml");
        let loaded = load(&path);
        assert_eq!(
            loaded,
            Loaded {
                rules: Rules::builtin(),
                error: None
            }
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULTS);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let dir = path.parent().unwrap();
            assert_eq!(
                fs::metadata(dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn broken_files_fall_back_to_observe_defaults() {
        let dir = temp_dir("broken");
        let path = dir.join("rules.toml");
        for (text, want) in [
            (
                "mode = \"auto\"\nbogus = true\n",
                "line 2: unknown key `bogus`",
            ),
            (
                "mode = \"auto\"\n[[allow]]\ncommand = \"rm\" \"x\"\n",
                "line 3",
            ),
            (
                "mode = \"auto\"\n[[allow]]\ntool = \"Bash\"\n",
                "line 3: `tool` must be one of",
            ),
        ] {
            fs::write(&path, text).unwrap();
            let loaded = load(&path);
            assert_eq!(loaded.rules, Rules::builtin(), "{text}");
            assert_eq!(loaded.rules.mode, Mode::Observe);
            assert!(
                loaded.error.as_deref().unwrap().contains(want),
                "{:?}",
                loaded.error
            );
        }
        fs::write(
            &path,
            format!("mode = \"auto\"\n{}", "#".repeat(MAX_SIZE as usize)),
        )
        .unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.rules, Rules::builtin());
        assert!(loaded.error.unwrap().ends_with("it is over 64 KB"));
        fs::write(&path, b"mode = \"auto\"\n\xff\n").unwrap();
        assert!(load(&path).error.unwrap().ends_with("it isn't UTF-8 text"));
        fs::write(&path, "mode = \"auto\"\n").unwrap();
        assert_eq!(load(&path).rules.mode, Mode::Auto);
    }

    #[test]
    fn add_appends_one_rule_atomically() {
        let path = temp_dir("add").join("rules.toml");
        fs::write(&path, "mode = \"auto\" # mine\n[[allow]]\ncommand = \"ls\"").unwrap();
        let rule = Rule::command(vec!["cargo".into(), "test".into()]);
        add(&path, &rule).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "mode = \"auto\" # mine\n[[allow]]\ncommand = \"ls\"\n\n[[allow]]\ncommand = \"cargo test\"\n"
        );
        let loaded = load(&path);
        assert_eq!(loaded.error, None);
        assert_eq!(loaded.rules.allow.last(), Some(&rule));
        // An inline list can't take an [[allow]] table: refused, file untouched.
        fs::write(&path, "allow = [{ command = \"ls\" }]\n").unwrap();
        assert!(
            add(&path, &rule)
                .unwrap_err()
                .starts_with("can't add the rule")
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "allow = [{ command = \"ls\" }]\n"
        );
        assert_eq!(
            fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1,
            "no temp file left"
        );
    }

    #[cfg(unix)]
    #[test]
    fn files_others_can_write_are_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("perm");
        let path = dir.join("rules.toml");
        fs::write(&path, "mode = \"auto\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.rules, Rules::builtin());
        assert!(loaded.error.unwrap().contains("others can write"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(load(&path).error.unwrap().contains("others can write"));
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(load(&path).error, None);
        let link = dir.join("link.toml");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(load(&link).error.unwrap().contains("is a link"));
    }

    #[cfg(windows)]
    #[test]
    fn own_files_pass_the_windows_access_check() {
        let path = temp_dir("acl").join("rules.toml");
        fs::write(&path, "mode = \"auto\"\n").unwrap();
        assert_eq!(load(&path).error, None, "{:?}", trust::check(&path));
    }

    #[test]
    fn access_lists_with_other_writers_are_refused() {
        let me = "S-1-5-21-1-2-3-1001";
        let ok = |sddl: &str| trust::sddl_owner_only(sddl, me);
        // Typical %APPDATA% file: inherited full access for SYSTEM, Administrators and me.
        assert_eq!(
            ok(&format!(
                "O:{me}D:AI(A;ID;FA;;;SY)(A;ID;FA;;;BA)(A;ID;FA;;;{me})"
            )),
            Ok(())
        );
        assert_eq!(ok("O:BAD:P(A;;FA;;;SY)"), Ok(()));
        // The built-in Administrator account appears as `LA`: trusted only
        // when that is the current user.
        let admin = "S-1-5-21-1-2-3-500";
        let sddl = "O:LAD:AI(A;ID;FA;;;SY)(A;ID;FA;;;BA)(A;ID;FA;;;LA)";
        assert_eq!(trust::sddl_owner_only(sddl, admin), Ok(()));
        assert!(trust::sddl_owner_only(sddl, me).is_err());
        let sddl = format!("O:{me}D:(A;;FA;;;{me})(A;;FA;;;LA)");
        assert!(trust::sddl_owner_only(&sddl, me).is_err());
        // Others may read and run, not write.
        assert_eq!(
            ok(&format!(
                "O:{me}D:(A;;FA;;;{me})(A;;FR;;;BU)(A;;0x1200a9;;;WD)"
            )),
            Ok(())
        );
        // Deny entries and inherit-only entries don't grant anything here.
        assert_eq!(
            ok(&format!(
                "O:{me}D:(A;;FA;;;{me})(D;;FA;;;WD)(A;OICIIO;GA;;;CO)"
            )),
            Ok(())
        );
        for bad in [
            format!("O:S-1-5-21-9-9-9-500D:(A;;FA;;;{me})"),
            format!("O:{me}D:NO_ACCESS_CONTROL"),
            format!("O:{me}"),
            format!("O:{me}D:(A;;FA;;;{me})(A;;FW;;;BU)"),
            format!("O:{me}D:(A;;FA;;;{me})(A;;GA;;;WD)"),
            format!("O:{me}D:(A;;FA;;;{me})(A;;0x2;;;AU)"),
            format!("O:{me}D:(A;;FA;;;{me})(A;;WD;;;BU)"),
            format!("O:{me}D:(A;;FA;;;{me})(A;;SD;;;BU)"),
            format!("O:{me}D:(A;;FA;;;{me})(A;;ZZ;;;BU)"),
            format!("O:{me}D:(A;;FA;;;{me})(A;;FA)"),
        ] {
            assert!(ok(&bad).is_err(), "{bad}");
        }
    }
}
