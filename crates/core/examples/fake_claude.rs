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
//! - `slow N`: streams a word every 50 ms for N seconds (the panel's CPU check)
//! - `demo`: a tool, a denial, then an answer (the panel's screenshots)
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
            "slow" => {
                init(&mut out, "none", &tools);
                let secs: u64 = prompt
                    .split_whitespace()
                    .nth(1)
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(5);
                for n in 0..secs * 20 {
                    text(
                        &mut out,
                        if n % 12 == 11 {
                            "word.
"
                        } else {
                            "word "
                        },
                    );
                    std::thread::sleep(Duration::from_millis(50));
                }
                done(&mut out, None);
            }
            "demo" => {
                init(&mut out, "none", &tools);
                say(
                    &mut out,
                    json!({"type": "assistant", "parent_tool_use_id": null, "message": {"content": [
                        {"type": "tool_use", "name": "Read", "input": {"file_path": "notes.txt"}}]}}),
                );
                say(
                    &mut out,
                    json!({"type": "system", "subtype": "permission_denied", "tool_name": "Read",
                        "decision_reason": "--restricted: path outside the working directory"}),
                );
                say(
                    &mut out,
                    json!({"type": "rate_limit_event", "rate_limit_info": {"unifiedWindows": {
                        "five_hour": {"utilization": 0.23}, "seven_day": {"utilization": 0.7}}}}),
                );
                text(
                    &mut out,
                    "notes.txt isn't in my folder (it's empty), and I can't read outside it. ",
                );
                text(&mut out, "Paste the text here and I'll take a look.");
                done(&mut out, None);
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
