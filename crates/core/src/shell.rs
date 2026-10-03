//! A POSIX shell parser for Bash tool commands (Claude Code runs them with
//! bash, Git Bash on Windows): which commands run, with which words, reading
//! and writing which files. Anything it can't be sure about is reported as an
//! [`Issue`]; the rules engine never auto-allows a command with one.
//!
//! It never fails: odd input still yields its best reading plus issues, so
//! risk checks can look at it too.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// `$(…)`, backticks, `<(…)`, `>(…)`: part of the command is made while it runs.
    Substitution,
    /// `$NAME`, `${…}`, `$'…'`: a value Bouncer can't see.
    Variable,
    /// `( )` and `{ }`: subshells, groups, function bodies, brace expansion.
    Grouping,
    /// `<<` heredocs and `<<<` here-strings.
    Heredoc,
    /// `NAME=value cmd`: changes the command's environment.
    Assignment,
    /// An unterminated quote, a trailing backslash, a redirection with no target.
    Unterminated,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Command {
    /// The words after quote removal; the first is the program.
    pub words: Vec<String>,
    /// Files written by `>`, `>>`, `>|`, `&>`, `<>`, `>&file`.
    pub writes: Vec<String>,
    /// Files read by `<`.
    pub reads: Vec<String>,
    /// Its input comes from the previous command through `|`.
    pub piped: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Parsed {
    pub commands: Vec<Command>,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Copy, PartialEq)]
enum Redirect {
    Read,
    Write,
    /// `>&2`, `2>&1`: copies a file descriptor (a target that isn't a number writes a file).
    Dup,
}

struct Parser {
    chars: Vec<char>,
    at: usize,
    out: Parsed,
    command: Command,
    word: Option<String>,
    redirect: Option<Redirect>,
}

pub fn parse(text: &str) -> Parsed {
    let mut p = Parser {
        chars: text.chars().collect(),
        at: 0,
        out: Parsed::default(),
        command: Command::default(),
        word: None,
        redirect: None,
    };
    p.run();
    p.out
}

impl Parser {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    fn issue(&mut self, issue: Issue) {
        if !self.out.issues.contains(&issue) {
            self.out.issues.push(issue);
        }
    }

    fn push(&mut self, c: char) {
        self.word.get_or_insert_with(String::new).push(c);
    }

    fn end_word(&mut self) {
        let Some(word) = self.word.take() else { return };
        match self.redirect.take() {
            Some(Redirect::Read) => self.command.reads.push(word),
            Some(Redirect::Dup) if word == "-" || word.chars().all(|c| c.is_ascii_digit()) => {}
            Some(Redirect::Write | Redirect::Dup) => self.command.writes.push(word),
            None => {
                if self.command.words.is_empty() && is_assignment(&word) {
                    self.issue(Issue::Assignment);
                }
                self.command.words.push(word);
            }
        }
    }

    /// Ends the current command; the next one is `piped` from it or not.
    fn end_command(&mut self, piped: bool) {
        self.end_word();
        if self.redirect.take().is_some() {
            self.issue(Issue::Unterminated);
        }
        let command = std::mem::take(&mut self.command);
        if !command.words.is_empty() || !command.writes.is_empty() || !command.reads.is_empty() {
            self.out.commands.push(command);
        }
        self.command.piped = piped;
    }

    /// Starts a redirection. Digits right before it name a file descriptor,
    /// not a word.
    fn redirect(&mut self, kind: Redirect, len: usize) {
        if self
            .word
            .as_deref()
            .is_some_and(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()))
        {
            self.word = None;
        }
        self.end_word();
        if self.redirect.is_some() {
            self.issue(Issue::Unterminated);
        }
        self.redirect = Some(kind);
        self.at += len;
    }

    /// After a `$` (unquoted or in double quotes): notes what it expands.
    fn dollar(&mut self) {
        match self.peek(1) {
            Some('(') => self.issue(Issue::Substitution),
            Some(c)
                if c == '{' || c == '\'' || c == '"' || c == '_' || c.is_ascii_alphanumeric() =>
            {
                self.issue(Issue::Variable)
            }
            Some('?' | '#' | '@' | '*' | '!' | '$' | '-') => self.issue(Issue::Variable),
            _ => {}
        }
        self.push('$');
        self.at += 1;
    }

    fn run(&mut self) {
        while let Some(c) = self.peek(0) {
            match c {
                ' ' | '\t' => {
                    self.end_word();
                    self.at += 1;
                }
                '\n' | ';' => {
                    self.end_command(false);
                    self.at += 1;
                }
                '#' if self.word.is_none() => {
                    while self.peek(0).is_some_and(|c| c != '\n') {
                        self.at += 1;
                    }
                }
                '&' => match self.peek(1) {
                    Some('>') => {
                        let len = if self.peek(2) == Some('>') { 3 } else { 2 };
                        self.redirect(Redirect::Write, len);
                    }
                    Some('&') => {
                        self.end_command(false);
                        self.at += 2;
                    }
                    _ => {
                        self.end_command(false);
                        self.at += 1;
                    }
                },
                '|' => match self.peek(1) {
                    Some('|') => {
                        self.end_command(false);
                        self.at += 2;
                    }
                    Some('&') => {
                        self.end_command(true);
                        self.at += 2;
                    }
                    _ => {
                        self.end_command(true);
                        self.at += 1;
                    }
                },
                '<' => match self.peek(1) {
                    Some('<') => {
                        self.issue(Issue::Heredoc);
                        let len = if self.peek(2) == Some('<') { 3 } else { 2 };
                        self.redirect(Redirect::Read, len);
                    }
                    Some('(') => {
                        self.issue(Issue::Substitution);
                        self.end_command(false);
                        self.at += 2;
                    }
                    Some('>') => self.redirect(Redirect::Write, 2),
                    Some('&') => self.redirect(Redirect::Dup, 2),
                    _ => self.redirect(Redirect::Read, 1),
                },
                '>' => match self.peek(1) {
                    Some('(') => {
                        self.issue(Issue::Substitution);
                        self.end_command(false);
                        self.at += 2;
                    }
                    Some('>' | '|') => self.redirect(Redirect::Write, 2),
                    Some('&') => self.redirect(Redirect::Dup, 2),
                    _ => self.redirect(Redirect::Write, 1),
                },
                '(' | ')' => {
                    self.issue(Issue::Grouping);
                    self.end_command(false);
                    self.at += 1;
                }
                '{' | '}' => {
                    self.issue(Issue::Grouping);
                    self.push(c);
                    self.at += 1;
                }
                '`' => {
                    self.issue(Issue::Substitution);
                    self.end_command(false);
                    self.at += 1;
                }
                '$' => {
                    if self.peek(1) == Some('(') {
                        self.issue(Issue::Substitution);
                        self.end_command(false);
                        self.at += 2;
                    } else {
                        self.dollar();
                    }
                }
                '\\' => match self.peek(1) {
                    Some('\n') => self.at += 2,
                    Some(next) => {
                        self.push(next);
                        self.at += 2;
                    }
                    None => {
                        self.issue(Issue::Unterminated);
                        self.at += 1;
                    }
                },
                '\'' => {
                    self.word.get_or_insert_with(String::new);
                    self.at += 1;
                    loop {
                        match self.peek(0) {
                            Some('\'') => break,
                            Some(c) => self.push(c),
                            None => {
                                self.issue(Issue::Unterminated);
                                break;
                            }
                        }
                        self.at += 1;
                    }
                    self.at += 1;
                }
                '"' => {
                    self.word.get_or_insert_with(String::new);
                    self.at += 1;
                    loop {
                        match self.peek(0) {
                            Some('"') => break,
                            Some('\\') => match self.peek(1) {
                                Some('\n') => self.at += 1,
                                Some(c @ ('$' | '`' | '"' | '\\')) => {
                                    self.push(c);
                                    self.at += 1;
                                }
                                _ => self.push('\\'),
                            },
                            Some('$') => {
                                self.dollar();
                                continue;
                            }
                            Some('`') => {
                                self.issue(Issue::Substitution);
                                self.push('`');
                            }
                            Some(c) => self.push(c),
                            None => {
                                self.issue(Issue::Unterminated);
                                break;
                            }
                        }
                        self.at += 1;
                    }
                    self.at += 1;
                }
                c => {
                    self.push(c);
                    self.at += 1;
                }
            }
        }
        self.end_command(false);
    }
}

/// `NAME=value`: a variable assignment when it comes before the command.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.chars()
            .next()
            .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
            && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<Vec<String>> {
        let parsed = parse(text);
        assert_eq!(parsed.issues, [], "{text}");
        parsed.commands.into_iter().map(|c| c.words).collect()
    }

    fn issues(text: &str) -> Vec<Issue> {
        parse(text).issues
    }

    #[test]
    fn words_quotes_and_escapes() {
        assert_eq!(words("ls -la  src"), [["ls", "-la", "src"]]);
        assert_eq!(
            words("echo 'a b' \"c d\" e\\ f"),
            [["echo", "a b", "c d", "e f"]]
        );
        assert_eq!(
            words("echo '$(x)' \"\\$HOME\""),
            [["echo", "$(x)", "$HOME"]]
        );
        assert_eq!(words("echo a'b'\"c\""), [["echo", "abc"]]);
        assert_eq!(words("echo '' x"), [["echo", "", "x"]]);
        assert_eq!(words("echo \"a\\nb\""), [["echo", "a\\nb"]]);
        assert_eq!(words("ls \\\n -la"), [["ls", "-la"]]);
        assert_eq!(words("ls # rm -rf /"), [["ls"]]);
        assert_eq!(words("echo a#b"), [["echo", "a#b"]]);
        assert_eq!(words("echo $ 5$"), [["echo", "$", "5$"]]);
    }

    #[test]
    fn compound_commands_split() {
        assert_eq!(
            words("ls; pwd && git status || echo x & wc\nhead"),
            [
                vec!["ls"],
                vec!["pwd"],
                vec!["git", "status"],
                vec!["echo", "x"],
                vec!["wc"],
                vec!["head"]
            ]
        );
        let parsed = parse("cat a | grep b |& wc");
        let piped: Vec<_> = parsed.commands.iter().map(|c| c.piped).collect();
        assert_eq!(piped, [false, true, true]);
    }

    #[test]
    fn redirections() {
        let c = &parse("ls >out 2>/dev/null 2>&1 >&2 >>log <in &>all >|clob").commands[0];
        assert_eq!(c.words, ["ls"]);
        assert_eq!(c.writes, ["out", "/dev/null", "log", "all", "clob"]);
        assert_eq!(c.reads, ["in"]);
        let c = &parse("ls 2 > x").commands[0];
        assert_eq!(
            (c.words.clone(), c.writes.clone()),
            (vec!["ls".into(), "2".into()], vec!["x".to_string()])
        );
        assert_eq!(parse("ls >&file").commands[0].writes, ["file"]);
        assert_eq!(issues("ls >"), [Issue::Unterminated]);
        assert_eq!(issues("ls > ; pwd"), [Issue::Unterminated]);
    }

    #[test]
    fn issues_are_noticed() {
        use Issue::*;
        for (text, want) in [
            ("ls $(rm x)", Substitution),
            ("ls `rm x`", Substitution),
            ("echo \"$(rm x)\"", Substitution),
            ("echo \"`rm x`\"", Substitution),
            ("cat <(curl x)", Substitution),
            ("tee >(sh)", Substitution),
            ("echo $HOME", Variable),
            ("echo ${HOME}", Variable),
            ("echo \"$1\"", Variable),
            ("echo $'\\x41'", Variable),
            ("echo $?", Variable),
            ("(rm x)", Grouping),
            ("ls() { rm x; }; ls", Grouping),
            ("echo {a,b}", Grouping),
            ("cat <<EOF\nrm x\nEOF", Heredoc),
            ("cat <<< hi", Heredoc),
            ("FOO=1 ls", Assignment),
            ("_x=1 ls", Assignment),
            ("echo 'abc", Unterminated),
            ("echo \"abc", Unterminated),
            ("echo abc\\", Unterminated),
        ] {
            assert!(issues(text).contains(&want), "{text}: {:?}", issues(text));
        }
        assert_eq!(issues("ls a=b"), [], "assignment only before the command");
        assert_eq!(issues("ls =b 1=x"), []);
    }

    #[test]
    fn substitutions_still_show_their_commands() {
        let parsed = parse("echo $(curl -s x | sh)");
        let firsts: Vec<_> = parsed
            .commands
            .iter()
            .map(|c| c.words[0].as_str())
            .collect();
        assert_eq!(firsts, ["echo", "curl", "sh"]);
    }

    #[test]
    fn odd_characters_stay_inside_words() {
        assert_eq!(words("ls\u{A0}-la"), [["ls\u{A0}-la"]]);
        assert_eq!(words("ｌｓ"), [["ｌｓ"]]);
        assert_eq!(words("ls \u{202E}txt"), [["ls", "\u{202E}txt"]]);
    }
}
