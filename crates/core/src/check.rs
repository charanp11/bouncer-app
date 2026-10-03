//! Checks one permission request against the rules: may a rule allow it, why
//! it's risky, and which rule "Always allow" would add.
//!
//! A request is allowable only when Bouncer understands all of it: every
//! part of a Bash command parsed with no issue and matched by a rule, no
//! output redirected to a file, every path inside the project, and no risk.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::rules::{PATH_TOOLS, Rule, Rules};
use crate::shell::{self, Command};

/// Where a request runs.
pub struct Context<'a> {
    /// The session's project: the folder it was first seen in.
    pub root: &'a Path,
    /// The current working directory (relative paths start here).
    pub cwd: &'a Path,
    pub home: Option<&'a Path>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Verdict {
    /// The rules that allow it (labels), when a rule may answer for the user.
    pub allow: Option<String>,
    /// Why it's risky, one short clause each. Never empty when risky.
    pub reasons: Vec<&'static str>,
    /// What "Always allow" would add, when it's safe to offer.
    pub offer: Option<Rule>,
}

pub fn check(rules: &Rules, tool: Option<&str>, input: Option<&Value>, ctx: &Context) -> Verdict {
    let root = Resolved::new(ctx);
    match (tool, input) {
        (Some("Bash"), Some(input)) => match input.get("command").and_then(Value::as_str) {
            Some(command) => bash(rules, command, &root),
            None => Verdict::default(),
        },
        (Some(tool), Some(input)) if PATH_TOOLS.contains(&tool) => {
            path_tool(rules, tool, input, &root)
        }
        _ => Verdict::default(),
    }
}

fn bash(rules: &Rules, command: &str, root: &Resolved) -> Verdict {
    let parsed = shell::parse(command);
    let understood = parsed.issues.is_empty()
        && !parsed.commands.is_empty()
        && parsed
            .commands
            .iter()
            .all(|c| !c.words.is_empty() && c.writes.iter().all(|w| w == "/dev/null"));
    let inside = parsed
        .commands
        .iter()
        .flat_map(path_words)
        .all(|w| root.inside(&root.resolve(w)));
    let matched: Option<Vec<String>> = parsed
        .commands
        .iter()
        .map(|c| {
            rules.allow.iter().find_map(|rule| match rule {
                Rule::Command(prefix) if c.words.starts_with(prefix) => Some(rule.label()),
                _ => None,
            })
        })
        .collect();
    let ok = understood && inside && root.usable;
    let mut verdict = Verdict::default();
    if ok && let Some(mut labels) = matched {
        labels.dedup();
        verdict.allow = Some(labels.join(", "));
    } else if ok && let [only] = parsed.commands.as_slice() {
        verdict.offer = offer(&only.words);
    }
    verdict
}

/// Words that name files: arguments, `--opt=value` values, redirection
/// targets, and the program when it's given as a path.
fn path_words(c: &Command) -> impl Iterator<Item = &str> {
    let program = c.words.first().filter(|w| w.contains(['/', '\\']));
    let args = c.words.iter().skip(1).filter_map(|w| {
        if let Some(opt) = w.strip_prefix('-') {
            match w.split_once('=') {
                Some((_, value)) => Some(value),
                // -o/etc/x: a short option with its value attached.
                None if !opt.starts_with('-') && w.contains(['/', '\\', '~']) => w.get(2..),
                None => None,
            }
        } else {
            Some(w.as_str())
        }
    });
    program
        .map(String::as_str)
        .into_iter()
        .chain(args)
        .chain(c.reads.iter().map(String::as_str))
        .chain(
            c.writes
                .iter()
                .map(String::as_str)
                .filter(|w| *w != "/dev/null"),
        )
}

fn path_tool(rules: &Rules, tool: &str, input: &Value, root: &Resolved) -> Verdict {
    let text = |key: &str| input.get(key).and_then(Value::as_str);
    let paths: Option<Vec<PathBuf>> = match tool {
        "NotebookEdit" => text("notebook_path").map(|p| vec![root.resolve(p)]),
        "Read" | "Edit" | "MultiEdit" | "Write" => text("file_path").map(|p| vec![root.resolve(p)]),
        // Searches: the folder (default: here) and the pattern / glob within it.
        _ => {
            let base = root.resolve(text("path").unwrap_or("."));
            let mut paths = vec![base.clone()];
            for key in ["pattern", "glob"] {
                if (tool == "Glob" || key == "glob")
                    && let Some(p) = text(key)
                {
                    paths.push(root.resolve_from(&base, p));
                }
            }
            Some(paths)
        }
    };
    let Some(paths) = paths else {
        return Verdict::default();
    };
    let mut verdict = Verdict::default();
    if root.usable && paths.iter().all(|p| root.inside(p)) {
        let rule = Rule::Tool(tool.into());
        if rules.allow.contains(&rule) {
            verdict.allow = Some(rule.label());
        } else {
            verdict.offer = Some(rule);
        }
    }
    verdict
}

/// Programs "Always allow" never offers: they run other code, delete,
/// download, or act as someone else. A rule for them is the user's call, by
/// editing the file.
const NEVER_OFFER: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "fish",
    "csh",
    "tcsh",
    "pwsh",
    "powershell",
    "cmd",
    "python",
    "python3",
    "py",
    "node",
    "deno",
    "bun",
    "perl",
    "ruby",
    "php",
    "lua",
    "osascript",
    "env",
    "exec",
    "eval",
    "source",
    ".",
    "command",
    "builtin",
    "xargs",
    "nohup",
    "time",
    "timeout",
    "nice",
    "watch",
    "sudo",
    "doas",
    "su",
    "runas",
    "gsudo",
    "pkexec",
    "rm",
    "rmdir",
    "unlink",
    "shred",
    "del",
    "dd",
    "mkfs",
    "chmod",
    "chown",
    "chgrp",
    "curl",
    "wget",
    "ssh",
    "scp",
    "rsync",
    "nc",
    "ncat",
    "socat",
    "alias",
    "export",
    "set",
    "unset",
    "trap",
    "kill",
    "killall",
    "pkill",
    "shutdown",
    "reboot",
    "find",
    "awk",
    "sed",
    "git",
];

/// Programs with subcommands: a rule names the subcommand too ("cargo test",
/// never just "cargo").
const SUBCOMMANDS: &[&str] = &[
    "cargo",
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "go",
    "docker",
    "kubectl",
    "pip",
    "pip3",
    "uv",
    "gh",
    "dotnet",
    "brew",
    "apt",
    "make",
    "rustup",
    "terraform",
];

/// Git subcommands "Always allow" may offer (`git` alone never).
const GIT_OFFER: &[&str] = &["status", "diff", "log", "show", "fetch", "add", "commit"];

/// The rule "Always allow" offers for a plain command: its program, plus its
/// subcommand when the second word is a plain word.
fn offer(words: &[String]) -> Option<Rule> {
    let program = words.first()?.as_str();
    let plain = |w: &str| {
        w.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._:-".contains(c))
    };
    let sub = words.get(1).filter(|w| plain(w));
    let git = program == "git" && sub.is_some_and(|s| GIT_OFFER.contains(&s.as_str()));
    if !(plain(program) || program.starts_with("./"))
        || (NEVER_OFFER.contains(&program) && !git)
        || (SUBCOMMANDS.contains(&program) && sub.is_none())
    {
        return None;
    }
    let mut prefix = vec![program.to_owned()];
    prefix.extend(sub.cloned());
    Some(Rule::Command(prefix))
}

/// Resolves paths the way the shell and the tools would, and knows the project.
struct Resolved<'a> {
    ctx: &'a Context<'a>,
    root: PathBuf,
    /// False when the project is a drive root or the home folder: too broad to
    /// call anything "inside".
    usable: bool,
}

impl<'a> Resolved<'a> {
    fn new(ctx: &'a Context<'a>) -> Self {
        let root = real(ctx.root);
        let home = ctx.home.map(real);
        let usable = root.parent().is_some() && Some(&root) != home.as_ref();
        Resolved { ctx, root, usable }
    }

    fn resolve(&self, word: &str) -> PathBuf {
        self.resolve_from(&real(self.ctx.cwd), word)
    }

    /// `word` as a path from `base`: `~` is home, globs count up to their
    /// fixed part, symlinks are followed as far as the path exists.
    fn resolve_from(&self, base: &Path, word: &str) -> PathBuf {
        let fixed = match word.find(['*', '?', '[']) {
            Some(i) => &word[..word[..i].rfind(['/', '\\']).map_or(0, |j| j + 1)],
            None => word,
        };
        let path = match fixed.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => match self.ctx.home {
                Some(home) => home.join(rest.trim_start_matches(['/', '\\'])),
                // No home: somewhere we can't know, so never inside.
                None => return PathBuf::from("/"),
            },
            // ~user: another user's home.
            Some(_) => return PathBuf::from("/"),
            None => base.join(fixed),
        };
        real(&path)
    }

    fn inside(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }
}

/// The real path, as the OS would open it: `.` dropped, and `..` applied
/// after following symlinks so far (so `link/..` is the link target's
/// parent), the longest existing part canonicalised.
fn real(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out = existing_real(&out);
                // At the root, `..` stays at the root.
                out.pop();
            }
            other => out.push(other),
        }
    }
    existing_real(&out)
}

/// Canonicalises the longest existing part of `path` and appends the rest.
fn existing_real(path: &Path) -> PathBuf {
    let mut base = path;
    let mut rest = Vec::new();
    loop {
        if let Ok(canon) = base.canonicalize() {
            return canon.join(rest.iter().rev().collect::<PathBuf>());
        }
        match (base.file_name(), base.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                base = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rules() -> Rules {
        Rules::builtin()
    }

    fn temp_project(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir()
            .join(format!("bouncer-check-{name}-{nanos}"))
            .join("proj");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        dir
    }

    fn bash_at(dir: &Path, command: &str) -> Verdict {
        let home = dir.parent().unwrap().join("home");
        let ctx = Context {
            root: dir,
            cwd: dir,
            home: Some(&home),
        };
        check(
            &rules(),
            Some("Bash"),
            Some(&json!({ "command": command })),
            &ctx,
        )
    }

    #[test]
    fn paths_resolve_like_the_shell() {
        let dir = temp_project("paths");
        let ctx = Context {
            root: &dir,
            cwd: &dir.join("src"),
            home: Some(Path::new("/home/me")),
        };
        let r = Resolved::new(&ctx);
        let root = real(&dir);
        assert_eq!(r.resolve("main.rs"), root.join("src").join("main.rs"));
        assert_eq!(r.resolve("../a/./b/../c"), root.join("a").join("c"));
        assert!(r.inside(&r.resolve("..")));
        assert!(!r.inside(&r.resolve("../..")));
        assert!(
            !r.inside(&r.resolve("../../proj-evil/x")),
            "a sibling sharing the prefix"
        );
        assert!(r.inside(&r.resolve("*.rs")));
        assert!(!r.inside(&r.resolve("../../*")));
        assert!(!r.inside(&r.resolve("~/x")));
        assert!(!r.inside(&r.resolve("~root/x")));
        assert!(!r.inside(&r.resolve("/etc/passwd")));
        assert!(!r.inside(&r.resolve("/../../etc")));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_out_of_the_project_are_outside() {
        let dir = temp_project("link");
        std::os::unix::fs::symlink("/etc", dir.join("etc")).unwrap();
        let v = bash_at(&dir, "cat etc/passwd");
        assert_eq!(v.allow, None);
        // The OS applies `..` after the link: etc/.. is /, not the project.
        assert_eq!(bash_at(&dir, "cat etc/../src").allow, None);
        assert_eq!(bash_at(&dir, "cat src/x").allow.as_deref(), Some("cat"));
    }

    #[test]
    fn compound_commands_need_every_part_allowed() {
        let dir = temp_project("compound");
        assert_eq!(
            bash_at(&dir, "git status && ls src | wc -l")
                .allow
                .as_deref(),
            Some("git status, ls, wc")
        );
        assert_eq!(bash_at(&dir, "ls; rm x").allow, None);
        assert_eq!(bash_at(&dir, "ls > out.txt").allow, None);
        assert_eq!(bash_at(&dir, "ls 2>/dev/null").allow.as_deref(), Some("ls"));
        assert_eq!(bash_at(&dir, "ls $(pwd)").allow, None);
        assert_eq!(bash_at(&dir, "cat ../../x").allow, None);
        assert_eq!(bash_at(&dir, "cat --file=../../x").allow, None);
        assert_eq!(bash_at(&dir, "cat -f../../x").allow, None);
        assert_eq!(bash_at(&dir, "cat -n src/x").allow.as_deref(), Some("cat"));
        assert_eq!(bash_at(&dir, "grep -r x -f/etc/x").allow, None);
    }

    #[test]
    fn root_or_home_projects_never_allow() {
        let dir = temp_project("root");
        let root = dir.ancestors().last().unwrap().to_path_buf();
        let ctx = Context {
            root: &root,
            cwd: &root,
            home: None,
        };
        assert_eq!(
            check(
                &rules(),
                Some("Bash"),
                Some(&json!({"command": "ls"})),
                &ctx
            )
            .allow,
            None
        );
        let ctx = Context {
            root: &dir,
            cwd: &dir,
            home: Some(&dir),
        };
        assert_eq!(
            check(
                &rules(),
                Some("Bash"),
                Some(&json!({"command": "ls"})),
                &ctx
            )
            .allow,
            None
        );
    }

    #[test]
    fn path_tools() {
        let dir = temp_project("tools");
        let ctx = Context {
            root: &dir,
            cwd: &dir,
            home: None,
        };
        let run = |tool: &str, input: Value| check(&rules(), Some(tool), Some(&input), &ctx);
        let inside = dir.join("src/main.rs").to_string_lossy().into_owned();
        assert_eq!(
            run("Read", json!({ "file_path": inside })).allow.as_deref(),
            Some("Read")
        );
        assert_eq!(
            run("Read", json!({ "file_path": "/etc/passwd" })).allow,
            None
        );
        assert_eq!(run("Read", json!({})).allow, None);
        let edit = run("Edit", json!({ "file_path": inside }));
        assert_eq!(
            (edit.allow, edit.offer),
            (None, Some(Rule::Tool("Edit".into())))
        );
        assert_eq!(run("Edit", json!({ "file_path": "/etc/x" })).offer, None);
        assert_eq!(
            run("Grep", json!({ "pattern": "fn main" }))
                .allow
                .as_deref(),
            Some("Grep")
        );
        assert_eq!(
            run("Grep", json!({ "pattern": "x", "path": "/" })).allow,
            None
        );
        assert_eq!(
            run("Grep", json!({ "pattern": "x", "glob": "../../**" })).allow,
            None
        );
        assert_eq!(
            run("Glob", json!({ "pattern": "**/*.rs" }))
                .allow
                .as_deref(),
            Some("Glob")
        );
        assert_eq!(run("Glob", json!({ "pattern": "../../**/*" })).allow, None);
        assert_eq!(run("Glob", json!({ "pattern": "/etc/*" })).allow, None);
        assert_eq!(
            run("WebFetch", json!({ "url": "https://x" })),
            Verdict::default()
        );
        assert_eq!(check(&rules(), None, None, &ctx), Verdict::default());
    }

    #[test]
    fn always_allow_offers_narrow_rules() {
        let w = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        let cmd = |s: &str| Some(Rule::Command(w(s)));
        assert_eq!(offer(&w("cargo run --release")), cmd("cargo run"));
        assert_eq!(offer(&w("pytest -x tests")), cmd("pytest"));
        assert_eq!(offer(&w("pytest tests")), cmd("pytest tests"));
        assert_eq!(offer(&w("./build.sh fast")), cmd("./build.sh fast"));
        assert_eq!(offer(&w("git fetch origin")), cmd("git fetch"));
        for never in [
            "cargo",
            "cargo --version",
            "git -C x status",
            "git push",
            "git",
            "bash x.sh",
            "python -m http.server",
            "rm -rf x",
            "sudo ls",
            "env ls",
            "curl x",
            "/bin/ls",
            "find . -delete",
        ] {
            assert_eq!(offer(&w(never)), None, "{never}");
        }
    }
}
