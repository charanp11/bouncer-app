//! Manual test stand-in for the app: listens on the real endpoint, prints every
//! event, and asks in this console how to answer each permission request.
//!
//! cargo run -p bouncer-core --example answer
//!
//! Type `a` to allow, `d` to deny, anything else to leave it to the terminal.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use bouncer_core::ipc::{self, Decision, Handler, Server};

fn main() -> std::io::Result<()> {
    let path = ipc::endpoint().expect("no endpoint");
    let server = Server::bind(&path)?;
    println!("listening on {}", path.display());
    let console = Arc::new(Mutex::new(()));
    let handler: Handler = Arc::new(move |event| {
        let _one_at_a_time = console.lock().unwrap();
        println!(
            "{} {} {} {}",
            event.session,
            event.kind,
            event.tool.as_deref().unwrap_or("-"),
            event.input.map(|i| i.to_string()).unwrap_or_default()
        );
        if !event.kind.eq("PermissionRequest") {
            return None;
        }
        print!("  allow (a) / deny (d) / terminal (enter)? ");
        std::io::stdout().flush().ok()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).ok()?;
        match line.trim() {
            "a" => Some(Decision::Allow),
            "d" => Some(Decision::Deny),
            _ => None,
        }
    });
    server.run(handler)
}
