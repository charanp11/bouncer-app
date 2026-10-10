//! The chat panel's backend: at most one locked-down chat
//! (`bouncer_core::chat`), owned by a worker thread; its events go to the
//! page on their own channel, tagged with the page's chat number so a chat
//! the page has moved on from is ignored. Nothing is written to disk but
//! the confirmed `claude` path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use bouncer_core::chat::{self, Binary, Cancel, Chat, Claude, Limits, Model, Out, Stop};
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

#[derive(Default)]
pub struct Chats {
    feed: Mutex<Option<Channel<Value>>>,
    run: Mutex<Option<Run>>,
    /// The plan's default model, as the last Default run named it.
    default_model: Mutex<Option<String>>,
}

struct Run {
    /// The page's number for this chat.
    chat: u32,
    model: Model,
    prompts: mpsc::Sender<String>,
    cancel: Cancel,
    busy: Arc<AtomicBool>,
}

impl Chats {
    /// Ends the chat, if any: its whole process tree is killed.
    pub fn end(&self) {
        if let Some(run) = self.run.lock().unwrap().take() {
            run.cancel.cancel();
        }
    }
}

/// The page's chat feed.
#[tauri::command]
pub fn chat_subscribe(chats: State<'_, Chats>, feed: Channel<Value>) {
    *chats.feed.lock().unwrap() = Some(feed);
}

/// Which `claude` chat would run, for the panel and Settings: its full path
/// and whether it's confirmed (or changed since).
#[tauri::command(async)]
pub fn chat_status(chats: State<'_, Chats>) -> Value {
    let mut status = status(&Claude::now());
    status["defaultModel"] = chats.default_model.lock().unwrap().clone().into();
    status
}

fn status(claude: &Claude) -> Value {
    let (state, bin) = match claude {
        Claude::Missing => ("missing", None),
        Claude::Unconfirmed(b) => ("unconfirmed", Some(b)),
        Claude::Changed(b) => ("changed", Some(b)),
        Claude::Confirmed(b) => ("confirmed", Some(b)),
    };
    json!({"state": state, "path": bin.map(Binary::shown)})
}

/// Confirms the `claude` the page showed. Only the one found now, and only
/// if it is the path the page showed (it may have changed in between).
#[tauri::command(async)]
pub fn chat_confirm(path: String) -> Result<Value, String> {
    let claude = Claude::now();
    let (Claude::Unconfirmed(bin) | Claude::Changed(bin) | Claude::Confirmed(bin)) = &claude else {
        return Err("missing".into());
    };
    if bin.shown() != path {
        return Err("changed".into());
    }
    let file = chat::confirmed_file().ok_or("no place to save it")?;
    chat::confirm(&file, bin)?;
    Ok(status(&Claude::now()))
}

/// Sends one message in chat number `chat` (the page's), starting that chat
/// if it isn't the one running. Refused unless the `claude` found now is the
/// confirmed one. Errors are short codes the page words: `missing`,
/// `unconfirmed`, `busy`, `model`, `empty`, or Claude Code's own text.
#[tauri::command(async)]
pub fn chat_send(
    app: AppHandle,
    chats: State<'_, Chats>,
    chat: u32,
    prompt: String,
    model: String,
) -> Result<(), String> {
    let model = Model::parse(&model).ok_or("model")?;
    if prompt.trim().is_empty() {
        return Err("empty".into());
    }
    if prompt.len() > chat::MAX_PROMPT {
        return Err("too-long".into());
    }
    let bin = match Claude::now() {
        Claude::Confirmed(bin) => bin,
        Claude::Missing => return Err("missing".into()),
        Claude::Unconfirmed(_) | Claude::Changed(_) => return Err("unconfirmed".into()),
    };
    let mut run = chats.run.lock().unwrap();
    if run
        .as_ref()
        .is_some_and(|r| r.chat != chat || r.model != model)
    {
        // The page moved on (New chat, another model): end the old one.
        if let Some(old) = run.take() {
            old.cancel.cancel();
        }
    }
    if run.as_ref().is_some_and(|r| r.busy.load(Ordering::SeqCst)) {
        return Err("busy".into());
    }
    if run.is_none() {
        let dir = chat::folder().ok_or("no chat folder")?;
        let started = Chat::start(&bin, &dir, model, Limits::default())?;
        let (prompts, inbox) = mpsc::channel();
        let busy = Arc::new(AtomicBool::new(false));
        *run = Some(Run {
            chat,
            model,
            prompts,
            cancel: started.cancel_handle(),
            busy: busy.clone(),
        });
        let app = app.clone();
        std::thread::spawn(move || work(app, chat, model, started, inbox, busy));
    }
    let current = run.as_ref().expect("started above");
    current.busy.store(true, Ordering::SeqCst);
    if current.prompts.send(prompt).is_err() {
        *run = None;
        return Err("ended".into());
    }
    Ok(())
}

/// Stops the answer in progress; the chat is over ("New chat").
#[tauri::command]
pub fn chat_cancel(chats: State<'_, Chats>) {
    if let Some(run) = chats.run.lock().unwrap().as_ref() {
        run.cancel.cancel();
    }
}

/// The page started a new chat: the old one ends now.
#[tauri::command]
pub fn chat_new(chats: State<'_, Chats>) {
    chats.end();
}

/// Streamed text goes to the page at most this often (the first words at
/// once). Every update is a frame of the whole window: one per word cost
/// 60–80% of a core.
const BATCH: Duration = Duration::from_millis(500);

/// Owns one chat: answers each message, sends its events to the page (text
/// gathered into `BATCH`es), and ends with the chat (dropping it kills the
/// tree).
fn work(
    app: AppHandle,
    id: u32,
    model: Model,
    mut chat: Chat,
    inbox: mpsc::Receiver<String>,
    busy: Arc<AtomicBool>,
) {
    let chats = app.state::<Chats>();
    let feed = |out: &Out| {
        if let Some(feed) = chats.feed.lock().unwrap().as_ref() {
            let _ = feed.send(event(id, out));
        }
    };
    let mut text = String::new();
    let mut sent = Instant::now()
        .checked_sub(BATCH)
        .unwrap_or_else(Instant::now);
    let flush = |text: &mut String, sent: &mut Instant| {
        if !text.is_empty() {
            feed(&Out::Text(std::mem::take(text)));
            *sent = Instant::now();
        }
    };
    for prompt in inbox {
        if chat.send(&prompt).is_err() {
            break;
        }
        loop {
            let due = sent + BATCH;
            let out = match chat.recv_by((!text.is_empty()).then_some(due)) {
                None => {
                    flush(&mut text, &mut sent);
                    continue;
                }
                Some(Out::Text(t)) => {
                    text.push_str(&t);
                    if Instant::now() >= due {
                        flush(&mut text, &mut sent);
                    }
                    continue;
                }
                Some(out) => {
                    flush(&mut text, &mut sent);
                    out
                }
            };
            if let (Out::Ready { model: name, .. }, Model::Default) = (&out, model) {
                *chats.default_model.lock().unwrap() = Some(name.clone());
            }
            let ended = matches!(out, Out::Stopped(_));
            if ended || matches!(out, Out::Done { .. }) {
                busy.store(false, Ordering::SeqCst);
            }
            if ended {
                let mut run = chats.run.lock().unwrap();
                if run.as_ref().is_some_and(|r| r.chat == id) {
                    *run = None;
                }
            }
            feed(&out);
            if ended {
                return;
            }
            if matches!(out, Out::Done { .. }) {
                break;
            }
        }
    }
}

/// One chat event as the page reads it (`src/chat.ts`, `ChatEvent`).
fn event(chat: u32, out: &Out) -> Value {
    let mut v = match out {
        Out::Ready { model, version } => {
            json!({"kind": "ready", "model": model, "version": version})
        }
        Out::Text(text) => json!({"kind": "text", "text": text}),
        Out::Tool { name, target } => json!({"kind": "tool", "name": name, "target": target}),
        Out::Denied { tool, reason } => json!({"kind": "denied", "tool": tool, "reason": reason}),
        Out::Usage {
            five_hour,
            seven_day,
        } => json!({"kind": "usage", "fiveHour": five_hour, "sevenDay": seven_day}),
        Out::Done { error } => json!({"kind": "done", "error": error}),
        Out::Stopped(stop) => json!({"kind": "stopped", "stop": stopped(stop)}),
    };
    v["chat"] = chat.into();
    v
}

fn stopped(stop: &Stop) -> Value {
    match stop {
        Stop::Cancelled => json!({"why": "cancelled"}),
        Stop::TimedOut => json!({"why": "timeout"}),
        Stop::Silent => json!({"why": "silent"}),
        Stop::Unsupported { version } => json!({"why": "unsupported", "version": version}),
        Stop::KeySource(source) => json!({"why": "key", "source": source}),
        Stop::NotLocked(what) => json!({"why": "unlocked", "what": what}),
        Stop::Exited { code, message } => {
            json!({"why": "exited", "code": code, "message": message})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every event and stop has the shape `src/chat.ts` reads.
    #[test]
    fn chat_events_have_the_shape_the_page_reads() {
        let cases = [
            (
                Out::Ready {
                    model: "claude-opus-5-5".into(),
                    version: "2.1.295".into(),
                },
                json!({"chat": 3, "kind": "ready", "model": "claude-opus-5-5", "version": "2.1.295"}),
            ),
            (
                Out::Text("<b>hi</b>".into()),
                json!({"chat": 3, "kind": "text", "text": "<b>hi</b>"}),
            ),
            (
                Out::Tool {
                    name: "Read".into(),
                    target: "a.txt".into(),
                },
                json!({"chat": 3, "kind": "tool", "name": "Read", "target": "a.txt"}),
            ),
            (
                Out::Denied {
                    tool: "Read".into(),
                    reason: "outside".into(),
                },
                json!({"chat": 3, "kind": "denied", "tool": "Read", "reason": "outside"}),
            ),
            (
                Out::Usage {
                    five_hour: Some(0.23),
                    seven_day: None,
                },
                json!({"chat": 3, "kind": "usage", "fiveHour": 0.23, "sevenDay": null}),
            ),
            (
                Out::Done { error: None },
                json!({"chat": 3, "kind": "done", "error": null}),
            ),
            (
                Out::Stopped(Stop::Unsupported {
                    version: Some("2.1.120".into()),
                }),
                json!({"chat": 3, "kind": "stopped", "stop": {"why": "unsupported", "version": "2.1.120"}}),
            ),
            (
                Out::Stopped(Stop::Exited {
                    code: Some(3),
                    message: "boom".into(),
                }),
                json!({"chat": 3, "kind": "stopped", "stop": {"why": "exited", "code": 3, "message": "boom"}}),
            ),
        ];
        for (out, want) in cases {
            assert_eq!(event(3, &out), want);
        }
        for (stop, why) in [
            (Stop::Cancelled, "cancelled"),
            (Stop::TimedOut, "timeout"),
            (Stop::Silent, "silent"),
            (Stop::KeySource("k".into()), "key"),
            (Stop::NotLocked("w".into()), "unlocked"),
        ] {
            assert_eq!(stopped(&stop)["why"], why);
        }
    }

    #[test]
    fn the_page_sees_which_claude_and_whether_it_is_confirmed() {
        assert_eq!(
            status(&Claude::Missing),
            json!({"state": "missing", "path": null})
        );
    }
}
