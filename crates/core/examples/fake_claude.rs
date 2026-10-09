//! A stand-in for `claude -p` in the chat tests (`tests/chat.rs`); talks to
//! nobody. `--version` prints a version. Otherwise it reads stream-json
//! messages from stdin and acts on each message's first word:
//!
//! - `reject`: complains about an unknown option and exits 1, as an older
//!   `claude` does
//! - `key` / `tools`: an init on an API key / with the Bash tool
//! - `hang`: an init, then nothing; `trickle`: a status event every 50 ms
//! - `child`: starts a grandchild that writes `beat` in the folder every
//!   20 ms, and says its pid
//! - `flood`: a 2 MB line, garbage, then an answer; `exit`: dies with code 3
//! - `error`: answers with Claude Code's plan error
//! - anything else: echoes argv, the environment's names, the prompt and the
//!   raw stdin line as the answer text

use std::io::{BufRead, Write};
use std::time::Duration;

use serde_json::{Value, json};

fn say(out: &mut impl Write, v: Value) {
    writeln!(out, "{v}").unwrap();
    out.flush().unwrap();
}

fn init(out: &mut impl Write, key: &str, tools: &[&str]) {
    say(
        out,
        json!({"type": "system", "subtype": "init", "tools": tools, "mcp_servers": [],
            "model": "claude-fake-1", "permissionMode": "dontAsk", "apiKeySource": key,
            "claude_code_version": "2.1.295"}),
    );
}

fn text(out: &mut impl Write, text: &str) {
    say(
        out,
        json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_delta",
            "index": 0, "delta": {"type": "text_delta", "text": text}}}),
    );
}

fn done(out: &mut impl Write, error: Option<&str>) {
    say(
        out,
        json!({"type": "result", "subtype": "success", "is_error": error.is_some(),
            "result": error.unwrap_or("ok")}),
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        println!("2.1.295 (Claude Code)");
        return;
    }
    if std::env::var_os("FAKE_CLAUDE_GRANDCHILD").is_some() {
        let mut n = 0u64;
        loop {
            let _ = std::fs::write("beat", n.to_string());
            n += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let tools = ["Glob", "Grep", "Read"];
    let mut out = std::io::stdout().lock();
    // Left running on purpose: the tests check that the kill takes them too.
    let mut grandchildren = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        let prompt = v["message"]["content"].as_str().unwrap().to_string();
        match prompt.split_whitespace().next().unwrap_or("") {
            "reject" => {
                eprintln!("error: unknown option '--permission-prompts'");
                std::process::exit(1);
            }
            "key" => init(&mut out, "ANTHROPIC_API_KEY", &tools),
            "tools" => init(&mut out, "none", &["Read", "Bash"]),
            "hang" => {
                init(&mut out, "none", &tools);
                std::thread::sleep(Duration::from_secs(3600));
            }
            "trickle" => {
                init(&mut out, "none", &tools);
                loop {
                    say(&mut out, json!({"type": "system", "subtype": "status"}));
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            "child" => {
                let child = std::process::Command::new(std::env::current_exe().unwrap())
                    .env("FAKE_CLAUDE_GRANDCHILD", "1")
                    .spawn()
                    .unwrap();
                init(&mut out, "none", &tools);
                text(&mut out, &child.id().to_string());
                grandchildren.push(child);
                done(&mut out, None);
            }
            "flood" => {
                init(&mut out, "none", &tools);
                writeln!(out, "{}", "x".repeat(2 << 20)).unwrap();
                writeln!(out, "not json\n[1,2]\n{{\"type\":").unwrap();
                text(&mut out, "after the flood");
                done(&mut out, None);
            }
            "exit" => {
                eprintln!("\n  boom: something broke\nmore");
                std::process::exit(3);
            }
            "error" => {
                init(&mut out, "none", &tools);
                done(&mut out, Some("Claude Fable isn't available on your plan."));
            }
            _ => {
                init(&mut out, "none", &tools);
                let env: Vec<String> = std::env::vars_os()
                    .map(|(k, _)| k.to_string_lossy().into_owned())
                    .collect();
                let echo = json!({"argv": args, "env": env, "prompt": prompt, "raw": line});
                text(&mut out, &echo.to_string());
                done(&mut out, None);
            }
        }
    }
}
