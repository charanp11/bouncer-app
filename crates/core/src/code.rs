//! The island's code pane: a diff built from the hook's own old and new text
//! (never by reading files), or a command's lines. Every string is made
//! visible-safe; the page renders it as text.

use serde_json::{Value, json};

use crate::approvals::visible;
use crate::diff::line_ops;

/// One shown line: `mark` is `' '`, `'-'`, `'+'`, or `'~'` between separate
/// edits; `n` counts from 1 within the change, old side for `-`, new for `+`.
struct Line {
    n: Option<usize>,
    mark: char,
    text: String,
}

/// The code pane for a tool call, or `None` for tools that have none.
pub fn code_view(tool: Option<&str>, input: Option<&Value>) -> Option<Value> {
    let input = input?;
    let text = |key: &str| input.get(key).and_then(Value::as_str);
    // An edit shows only the changed text, so its line numbers aren't the file's.
    let excerpt = matches!(tool, Some("Edit" | "MultiEdit"));
    let (path, lines) = match tool? {
        "Edit" => (
            text("file_path")?,
            diff(text("old_string")?, text("new_string")?),
        ),
        "MultiEdit" => {
            let mut lines = Vec::new();
            for edit in input.get("edits")?.as_array()? {
                let (old, new) = (edit.get("old_string")?, edit.get("new_string")?);
                if !lines.is_empty() {
                    lines.push(Line {
                        n: None,
                        mark: '~',
                        text: "⋯".into(),
                    });
                }
                lines.extend(diff(old.as_str()?, new.as_str()?));
            }
            (text("file_path")?, lines)
        }
        "Write" => (text("file_path")?, added(text("content")?)),
        "NotebookEdit" => (text("notebook_path")?, added(text("new_source")?)),
        shell @ ("Bash" | "PowerShell") => {
            let lines = text("command")?
                .lines()
                .enumerate()
                .map(|(i, l)| Line {
                    n: Some(i + 1),
                    mark: ' ',
                    text: l.into(),
                })
                .collect();
            let badge = if shell == "Bash" { "SH" } else { "PS" };
            return Some(render("command", "", badge, "shell", 0, false, lines));
        }
        _ => return None,
    };
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let ext = file
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    let (badge, syntax) = language(&ext);
    let changes = hunks(&lines);
    Some(render(file, path, &badge, syntax, changes, excerpt, lines))
}

fn render(
    file: &str,
    path: &str,
    badge: &str,
    syntax: &str,
    changes: usize,
    excerpt: bool,
    lines: Vec<Line>,
) -> Value {
    let lines: Vec<Value> = lines
        .into_iter()
        .map(|l| json!({ "n": l.n, "mark": l.mark.to_string(), "text": visible(&l.text) }))
        .collect();
    json!({
        "file": visible(file),
        "path": visible(path),
        "badge": badge,
        "syntax": syntax,
        "changes": changes,
        "excerpt": excerpt,
        "lines": lines,
    })
}

fn diff(old: &str, new: &str) -> Vec<Line> {
    let (mut old_n, mut new_n) = (1, 1);
    line_ops(old, new)
        .into_iter()
        .map(|(mark, text)| {
            let n = match mark {
                '-' => {
                    old_n += 1;
                    old_n - 1
                }
                '+' => {
                    new_n += 1;
                    new_n - 1
                }
                _ => {
                    old_n += 1;
                    new_n += 1;
                    new_n - 1
                }
            };
            Line {
                n: Some(n),
                mark,
                text: text.into(),
            }
        })
        .collect()
}

/// Lines added and removed by an edit, for the pill's `+N −M`: Edit old vs
/// new text, MultiEdit the sum over its edits, Write `+N` only (the old file
/// is never read, so removed is `None`). Other tools: `None`. Numbers only;
/// `replace_all` counts once (how many places would need the file).
pub fn line_counts(tool: Option<&str>, input: Option<&Value>) -> Option<(usize, Option<usize>)> {
    let input = input?;
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
    let pairs = match tool? {
        "Edit" => vec![(text(input, "old_string")?, text(input, "new_string")?)],
        "MultiEdit" => input
            .get("edits")?
            .as_array()?
            .iter()
            .map(|e| Some((text(e, "old_string")?, text(e, "new_string")?)))
            .collect::<Option<Vec<_>>>()?,
        "Write" => return Some((text(input, "content")?.lines().count(), None)),
        _ => return None,
    };
    let (mut add, mut del) = (0, 0);
    for (old, new) in &pairs {
        for (mark, _) in line_ops(old, new) {
            match mark {
                '+' => add += 1,
                '-' => del += 1,
                _ => {}
            }
        }
    }
    Some((add, Some(del)))
}

fn added(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .map(|(i, l)| Line {
            n: Some(i + 1),
            mark: '+',
            text: l.into(),
        })
        .collect()
}

/// Number of separate changes: runs of removed/added lines.
fn hunks(lines: &[Line]) -> usize {
    let changed = |l: &Line| matches!(l.mark, '-' | '+');
    (0..lines.len())
        .filter(|&i| changed(&lines[i]) && (i == 0 || !changed(&lines[i - 1])))
        .count()
}

/// File-type badge and the page's tokenizer name ("" = no colors).
fn language(ext: &str) -> (String, &'static str) {
    let syntax = match ext {
        "rs" => "rust",
        "ts" | "tsx" | "mts" | "cts" => "ts",
        "js" | "jsx" | "mjs" | "cjs" => "ts",
        "py" => "python",
        "go" => "go",
        "json" => "json",
        "sh" | "bash" | "ps1" => "shell",
        _ => "",
    };
    let badge = if ext.is_empty() {
        "TXT".to_owned()
    } else {
        ext.chars().take(4).collect::<String>().to_uppercase()
    };
    (badge, syntax)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(v: &Value) -> Vec<(Value, String, String)> {
        v["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l["n"].clone(),
                    l["mark"].as_str().unwrap().into(),
                    l["text"].as_str().unwrap().into(),
                )
            })
            .collect()
    }

    #[test]
    fn edit_becomes_a_numbered_diff() {
        let input = json!({
            "file_path": r"C:\Users\chara\Desktop\Full Time\x\crates\core\src\approvals.rs",
            "old_string": "use x;\n\npub const ARM: u64 = 400;\nfn a() {}\n    since > ARM",
            "new_string": "use x;\n\npub const ARM: u64 = 600;\nfn a() {}\n    since >= ARM",
        });
        let v = code_view(Some("Edit"), Some(&input)).unwrap();
        assert_eq!(v["file"], "approvals.rs");
        assert_eq!(v["path"], input["file_path"]);
        assert_eq!(
            (v["badge"].as_str(), v["syntax"].as_str()),
            (Some("RS"), Some("rust"))
        );
        assert_eq!(v["changes"], 2);
        assert_eq!(v["excerpt"], true);
        let m = marks(&v);
        assert_eq!(
            m[2],
            (json!(3), "-".into(), "pub const ARM: u64 = 400;".into())
        );
        assert_eq!(
            m[3],
            (json!(3), "+".into(), "pub const ARM: u64 = 600;".into())
        );
        assert_eq!(m[4], (json!(4), " ".into(), "fn a() {}".into()));
    }

    #[test]
    fn write_multiedit_and_bash() {
        let w = code_view(
            Some("Write"),
            Some(&json!({ "file_path": "/p/new.py", "content": "a\nb" })),
        )
        .unwrap();
        assert_eq!(
            marks(&w),
            [
                (json!(1), "+".into(), "a".into()),
                (json!(2), "+".into(), "b".into())
            ]
        );
        assert_eq!(
            (w["badge"].as_str(), w["changes"].as_u64()),
            (Some("PY"), Some(1))
        );
        assert_eq!(
            w["excerpt"], false,
            "a written file is numbered as the file"
        );

        let m = code_view(
            Some("MultiEdit"),
            Some(&json!({ "file_path": "/p/README", "edits": [
                { "old_string": "a", "new_string": "b" },
                { "old_string": "c", "new_string": "d" }
            ] })),
        )
        .unwrap();
        assert_eq!(m["changes"], 2);
        assert_eq!(m["badge"], "TXT");
        assert!(
            marks(&m)
                .iter()
                .any(|(n, mark, _)| n.is_null() && mark == "~")
        );

        let b = code_view(Some("Bash"), Some(&json!({ "command": "ls\npwd" }))).unwrap();
        assert_eq!(
            (b["file"].as_str(), b["syntax"].as_str()),
            (Some("command"), Some("shell"))
        );
        assert_eq!(marks(&b).len(), 2);

        assert!(code_view(Some("Read"), Some(&json!({ "file_path": "/p/a" }))).is_none());
        assert!(code_view(Some("Edit"), Some(&json!({ "file_path": "/p/a" }))).is_none());
    }

    #[test]
    fn hidden_characters_in_code_are_made_visible() {
        let input = json!({ "file_path": "/p/a\u{202E}txt.rs", "old_string": "", "new_string": "x\u{1b}[31m" });
        let v = code_view(Some("Edit"), Some(&input)).unwrap();
        assert_eq!(v["file"], r"a\u{202E}txt.rs");
        assert_eq!(v["lines"][0]["text"], r"x\u{001B}[31m");
    }

    #[test]
    fn counts_lines_added_and_removed() {
        let edit =
            json!({ "file_path": "/p/a.rs", "old_string": "a\nb\nc", "new_string": "a\nB\nc\nd" });
        assert_eq!(line_counts(Some("Edit"), Some(&edit)), Some((2, Some(1))));
        let multi = json!({ "file_path": "/p/a.rs", "edits": [
            { "old_string": "a", "new_string": "b" },
            { "old_string": "c\nd", "new_string": "" }
        ]});
        assert_eq!(
            line_counts(Some("MultiEdit"), Some(&multi)),
            Some((1, Some(3)))
        );
        // Write: the old file is never read, so nothing is "removed".
        let write = json!({ "file_path": "/p/new.py", "content": "a\nb\nc\n" });
        assert_eq!(line_counts(Some("Write"), Some(&write)), Some((3, None)));
        let bash = json!({ "command": "ls" });
        assert_eq!(line_counts(Some("Bash"), Some(&bash)), None);
        assert_eq!(
            line_counts(Some("Edit"), Some(&json!({ "old_string": 1 }))),
            None
        );
    }
}
