//! Secret redaction for the activity log, run on every text field before it
//! is written. Over-redacting a local log is fine; a missed secret isn't.

pub const MARK: &str = "[redacted]";

/// Token prefixes of well-known secrets; a run starting with one and at least
/// `MIN_AFTER` more token characters is a secret.
const PREFIXES: &[&str] = &[
    "sk-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "whsec_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "xoxs-",
    "AKIA",
    "ASIA",
    "AIza",
    "hf_",
    "npm_",
    "pypi-",
    "eyJ",
];
const MIN_AFTER: usize = 8;
/// Names that make the value next to them a secret (any case).
const SECRET_NAMES: &[&str] = &[
    "pass",
    "pwd",
    "secret",
    "token",
    "key",
    "auth",
    "credential",
    "cookie",
];
/// A run this long that looks random is a secret.
const LONG_RUN: usize = 32;

/// `text` with secrets replaced by `MARK`.
pub fn redact(text: &str) -> String {
    let text = private_keys(text);
    let mut out = String::with_capacity(text.len());
    let mut next_is_secret = false;
    let mut next_is_login = false;
    let mut rest = text.as_str();
    while !rest.is_empty() {
        let space = rest.len() - rest.trim_start().len();
        out += &rest[..space];
        rest = &rest[space..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..end];
        rest = &rest[end..];
        if word.is_empty() {
            continue;
        }
        let whole = next_is_secret;
        let login = next_is_login;
        // `-u user:password` / `--user user:password` (curl, wget, …): the next word is a login.
        next_is_login = word == "-u" || word == "--user";
        let name = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        next_is_secret = (secret_name(name)
            && !word.contains('=')
            && (word.starts_with('-') || word.ends_with(':')))
            || name.eq_ignore_ascii_case("bearer")
            || name.eq_ignore_ascii_case("basic");
        if whole {
            out += MARK;
        } else if login {
            out += &redact_word(&login_password(word));
        } else {
            out += &redact_word(word);
        }
    }
    out
}

fn secret_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !lower.is_empty() && SECRET_NAMES.iter().any(|s| lower.contains(s))
}

/// `-----BEGIN … PRIVATE KEY-----` through its `-----END …-----` line (or the
/// end of the text) becomes `MARK`.
fn private_keys(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("-----BEGIN ") {
        let header_end = rest[start + 11..].find("-----").map(|i| start + 11 + i + 5);
        let is_private = header_end.is_some_and(|h| rest[start..h].contains("PRIVATE KEY"));
        let Some(header_end) = header_end.filter(|_| is_private) else {
            out += &rest[..start + 11];
            rest = &rest[start + 11..];
            continue;
        };
        out += &rest[..start];
        out += MARK;
        rest = match rest[header_end..].find("-----END ") {
            Some(i) => {
                let after = header_end + i + 9;
                match rest[after..].find("-----") {
                    Some(j) => &rest[after + j + 5..],
                    None => "",
                }
            }
            None => "",
        };
    }
    out + rest
}

/// One word: a password glued to `-p` or a login glued to `-u` / `--user=`,
/// a secret-named assignment or JSON value, a password in a URL, known token
/// prefixes and long random-looking runs.
fn redact_word(word: &str) -> String {
    // `mysql -pSECRET`: anything glued onto `-p` (also hides `-pthread`-like
    // option clusters; over-redacting is fine).
    if word.strip_prefix("-p").is_some_and(|rest| !rest.is_empty()) {
        return format!("-p{MARK}");
    }
    if let Some(login) = word.strip_prefix("--user=") {
        return format!("--user={}", login_password(login));
    }
    if let Some(login) = word.strip_prefix("-u").filter(|l| l.contains(':')) {
        return format!("-u{}", login_password(login));
    }
    let word = json_values(word);
    let word = word.as_str();
    if let Some((key, _)) = word.split_once('=') {
        let name = key.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        if secret_name(name) {
            return format!("{key}={MARK}");
        }
    }
    let word = url_password(word);
    let word = runs(&word, is_token_char, |run| {
        PREFIXES
            .iter()
            .any(|p| run.starts_with(p) && run.len() >= p.len() + MIN_AFTER)
    });
    runs(&word, is_long_char, looks_random)
}

/// `user:password` → `user:[redacted]`; a login without `:` is left as is.
fn login_password(login: &str) -> String {
    match login.split_once(':') {
        Some((user, _)) => format!("{user}:{MARK}"),
        None => login.to_owned(),
    }
}

/// JSON written as one word, `{"token":"x"}` / `{"api_key":1}`: the value of
/// a secret-named key becomes `MARK` (quotes kept), the rest is left alone.
fn json_values(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut rest = word;
    while let Some(i) = rest.find("\":") {
        let key = &rest[rest[..i].rfind('"').map_or(0, |q| q + 1)..i];
        out += &rest[..i + 2];
        rest = &rest[i + 2..];
        if !secret_name(key) {
            continue;
        }
        let end = match rest.strip_prefix('"') {
            Some(quoted) => closing_quote(quoted).map_or(rest.len(), |j| j + 2),
            None => rest.find([',', '}']).unwrap_or(rest.len()),
        };
        let quoted = rest.starts_with('"');
        let closed = quoted && end >= 2 && rest[..end].ends_with('"');
        out += if quoted { "\"" } else { "" };
        out += MARK;
        out += if closed { "\"" } else { "" };
        rest = &rest[end..];
    }
    out + rest
}

/// Index of the `"` that ends a JSON string: one not escaped by an odd run
/// of backslashes (`\"` is part of the value, `\\"` ends it).
fn closing_quote(s: &str) -> Option<usize> {
    let mut slashes = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' if slashes % 2 == 0 => return Some(i),
            '\\' => slashes += 1,
            _ => slashes = 0,
        }
    }
    None
}

/// `scheme://user:password@host` → `scheme://user:[redacted]@host`.
fn url_password(word: &str) -> String {
    let Some(scheme) = word.find("://") else {
        return word.to_owned();
    };
    let start = scheme + 3;
    let authority_end = word[start..]
        .find(['/', '?', '#'])
        .map_or(word.len(), |i| start + i);
    let Some(at) = word[start..authority_end].rfind('@').map(|i| start + i) else {
        return word.to_owned();
    };
    match word[start..at].find(':') {
        Some(colon) => format!("{}{MARK}{}", &word[..start + colon + 1], &word[at..]),
        None => word.to_owned(),
    }
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_long_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '=')
}

/// A long run with letters and digits. With `/` or `-` in it (paths, UUIDs,
/// kebab-case names) it must also switch case often, as random base64 does.
fn looks_random(run: &str) -> bool {
    if run.len() < LONG_RUN
        || !run.chars().any(|c| c.is_ascii_digit())
        || !run.chars().any(|c| c.is_ascii_alphabetic())
    {
        return false;
    }
    if !run.contains(['/', '-']) {
        return true;
    }
    let letters: Vec<char> = run.chars().filter(char::is_ascii_alphabetic).collect();
    let switches = letters
        .windows(2)
        .filter(|w| w[0].is_ascii_uppercase() != w[1].is_ascii_uppercase())
        .count();
    switches * 4 >= letters.len().saturating_sub(1)
}

/// Replaces each maximal run of `is_char` characters for which `secret` holds.
fn runs(text: &str, is_char: fn(char) -> bool, secret: impl Fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = None;
    for (i, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (start, is_char(c) && i < text.len()) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                let run = &text[s..i];
                out += if secret(run) { MARK } else { run };
                start = None;
                if i < text.len() {
                    out.push(c);
                }
            }
            (None, false) if i < text.len() => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `-----<what> KEY-----`, built at run time like the fakes below.
    fn armor(what: &str) -> String {
        let dashes = "-".repeat(5);
        format!("{dashes}{what} KEY{dashes}")
    }

    /// Fake secrets, built at run time so secret scanners don't flag the
    /// source. Each pairs a secret with text that carries it.
    pub(crate) fn fakes() -> Vec<(String, String)> {
        let r = |s: &str, n: usize| s.repeat(n);
        let mut cases = Vec::new();
        let mut add = |secret: String, text: String| cases.push((secret, text));
        // Logins come first, before any line naming that HTTP tool: gitleaks
        // reads the tool's name followed within five lines by a login as a
        // real credential.
        let login = format!("loginpass{}", r("7", 4));
        add(login.clone(), format!("xh -u admin:{login} https://x"));
        add(
            login.clone(),
            format!("wget --user 'admin:{login}' https://x"),
        );
        add(
            login.clone(),
            format!("wget --user=admin:{login} https://x"),
        );
        add(login.clone(), format!("xh -uadmin:{login} https://x"));
        let anthropic = format!("sk-ant-api03-{}", r("Ab3dEf9h", 8));
        add(anthropic.clone(), format!("export ANTHROPIC={anthropic}"));
        let openai = format!("sk-proj-{}", r("Zx81", 10));
        add(openai.clone(), format!("curl -d k={openai} https://x"));
        let github = format!("ghp_{}", r("a1B2", 9));
        add(
            github.clone(),
            format!("git clone https://{github}@github.com/o/r"),
        );
        let pat = format!("github_pat_{}", r("11AB", 6));
        add(pat.clone(), format!("gh auth login --with {pat}"));
        let gitlab = format!("glpat-{}", r("xY7", 7));
        add(gitlab.clone(), format!("echo {gitlab}"));
        let slack = format!("xoxb-{}-{}", r("12", 6), r("aB", 8));
        add(slack.clone(), format!("SLACK={slack} node bot.js"));
        let aws_id = format!("AKIA{}", r("Q7ZT", 4));
        add(
            aws_id.clone(),
            format!("aws configure set aws_access_key_id {aws_id}"),
        );
        let aws_secret = format!("wJalrXUtnFEMI/K7MDENG/bPxRfiCY{}", r("Ex4m", 3));
        add(
            aws_secret.clone(),
            format!("aws configure set x {aws_secret}"),
        );
        let google = format!("AIza{}", r("Sy9_", 9));
        add(google.clone(), format!("fetch('?key={google}')"));
        let stripe = format!("sk_live_{}", r("4eC3", 6));
        add(stripe.clone(), format!("stripe login {stripe}"));
        let hf = format!("hf_{}", r("Qw3r", 8));
        add(hf.clone(), format!("huggingface-cli login {hf}"));
        let npm = format!("npm_{}", r("Np0m", 9));
        add(npm.clone(), format!("npm config set //registry/:_t {npm}"));
        let jwt = format!(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.{}",
            r("Sf1KxwRJ", 5)
        );
        add(
            jwt.clone(),
            format!("curl -H \"Authorization: Bearer {jwt}\" x"),
        );
        add(
            "hunter2pass".into(),
            "mysql --password hunter2pass db".into(),
        );
        add(
            "p4ssw0rd!".into(),
            "psql postgres://admin:p4ssw0rd!@db:5432/app".into(),
        );
        add("letmein123".into(), "export DB_PASSWORD=letmein123".into());
        add("s3cr3tv4l".into(), "--client-secret=s3cr3tv4l".into());
        add("abc123tok".into(), "{\"api_token\": \"abc123tok\"}".into());
        add("cookieval9".into(), "curl -b x --cookie: cookieval9".into());
        let basic = "dXNlcjpwYXNz".to_owned();
        add(
            basic.clone(),
            format!("curl -H 'Authorization: Basic {basic}'"),
        );
        let mysql = format!("MyS3cret{}", r("Pw", 2));
        add(mysql.clone(), format!("mysql -u root -p{mysql} app"));
        let token = format!("tok3n{}", r("val", 2));
        add(token.clone(), format!("{{\"token\":\"{token}\"}}"));
        let api = format!("k3y{}", r("val", 3));
        add(
            api.clone(),
            format!("curl -d {{\"user\":\"bob\",\"api_key\":\"{api}\"}} x"),
        );
        add(api.clone(), format!("echo {{\"apiKey\":{api},\"n\":1}}"));
        // An escaped quote inside the value must not end it early.
        let tail = format!("t41l{}", r("val", 2));
        add(tail.clone(), format!("{{\"token\":\"ab\\\"{tail}\"}}"));
        add(
            tail.clone(),
            format!("{{\"secret\":\"\\\\\\\"{tail}\",\"n\":1}}"),
        );
        let hex = r("9f3a", 10);
        add(hex.clone(), format!("deploy --x {hex}"));
        let key_body = r("MIIEvQIBADANBgkqhkiG9w0BAQEFAASC", 2);
        add(
            key_body.clone(),
            format!(
                "echo '{}\n{key_body}\n{}' > k",
                armor("BEGIN PRIVATE"),
                armor("END PRIVATE")
            ),
        );
        let rsa_body = r("MIIBOgIBAAJBAKj34GkxFhD90vcNLYLInFEX", 2);
        add(
            rsa_body.clone(),
            format!("{}\n{rsa_body}", armor("BEGIN RSA PRIVATE")),
        );
        cases
    }

    #[test]
    fn every_fake_secret_is_redacted() {
        for (secret, text) in fakes() {
            let out = redact(&text);
            assert!(!out.contains(&secret), "{text:?} -> {out:?}");
            assert!(out.contains(MARK), "{out}");
        }
    }

    #[test]
    fn ordinary_text_is_kept() {
        for text in [
            "Running cargo test --workspace",
            "Editing main.rs",
            "C:\\Users\\chara\\Desktop\\Full Time\\bouncer-starter\\bouncer-playground",
            "/Users/someone/Projects/bouncer-app/crates/core/src/approvals.rs",
            "git status",
            "npm run build -- --mode production",
            "git commit -m \"add key rotation docs\"",
            "4936ec29-90bc-4f48-93db-b957ead3a613",
            "https://github.com/charanp11/bouncer-app",
            "ssh://git@github.com/o/r.git",
            "-----BEGIN CERTIFICATE-----",
            "ask-for-help sk-short",
            "mkdir -p src/bin",
            "git push -u origin main",
            "xh -u admin https://x",
            "{\"user\":\"bob\",\"n\":1}",
        ] {
            assert_eq!(redact(text), text);
        }
    }

    #[test]
    fn shapes_are_kept_around_the_mark() {
        assert_eq!(
            redact("psql postgres://u:pw@h/db"),
            "psql postgres://u:[redacted]@h/db"
        );
        assert_eq!(redact("export API_KEY=x y"), "export API_KEY=[redacted] y");
        assert_eq!(
            redact("curl -H \"Authorization: Bearer abc\" u"),
            "curl -H \"Authorization: [redacted] [redacted] u"
        );
        assert_eq!(
            redact(&format!(
                "a\n{}\nzz\n{}\nb",
                armor("BEGIN EC PRIVATE"),
                armor("END EC PRIVATE")
            )),
            "a\n[redacted]\nb"
        );
        assert_eq!(redact("  two  spaces "), "  two  spaces ");
        assert_eq!(redact("xh -u admin:pw u"), "xh -u admin:[redacted] u");
        assert_eq!(redact("xh -uadmin:pw u"), "xh -uadmin:[redacted] u");
        assert_eq!(
            redact("mysql -u root -ppw db"),
            "mysql -u root -p[redacted] db"
        );
        assert_eq!(redact("{\"token\":\"x\"}"), "{\"token\":\"[redacted]\"}");
        assert_eq!(
            redact("{\"api_key\":\"x\",\"user\":\"bob\"}"),
            "{\"api_key\":\"[redacted]\",\"user\":\"bob\"}"
        );
        assert_eq!(redact("{\"token\":42}"), "{\"token\":[redacted]}");
        assert_eq!(
            redact("{\"token\":\"a\\\\\",\"user\":\"bob\"}"),
            "{\"token\":\"[redacted]\",\"user\":\"bob\"}"
        );
        assert_eq!(
            redact("{\"token\":\"a\\\"b\",\"user\":\"bob\"}"),
            "{\"token\":\"[redacted]\",\"user\":\"bob\"}"
        );
    }
}
