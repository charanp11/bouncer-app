//! The chat panel's backend: at most one locked-down chat
//! (`bouncer_core::chat`), owned by a worker thread; its events go to the
//! page on their own channel, tagged with the page's chat number so a chat
//! the page has moved on from is ignored. Nothing is written to disk but
//! the confirmed `claude` path.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use bouncer_core::chat::{self, Binary, Cancel, Chat, Claude, Limits, Lock, Model, Out, Stop};
use bouncer_core::drop::{self, HOLD};
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::drop::Drops;

#[derive(Default)]
pub struct Chats {
    feed: Mutex<Option<Channel<Value>>>,
    run: Mutex<Option<Run>>,
    /// The plan's default model, as the last Default run named it.
    default_model: Mutex<Option<String>>,
    /// The explanation running: its card's request ID, its run number (a
    /// card can be explained again) and how to stop it.
    explain: Mutex<Option<(String, u64, Cancel)>>,
    /// Numbers every explanation and file answer.
    explain_runs: AtomicU64,
    /// "Is this safe to run?" on a dropped file: its drop ID, run number
    /// and how to stop it. Apart from `explain`: cards come and go under it.
    drop_run: Mutex<Option<(String, u64, Cancel)>>,
    /// "Ask about this session": the exact text the page shows, under a
    /// random ID, until sent (once), dropped or `HOLD` is up. The page only
    /// ever sends the ID back.
    asked: Mutex<Option<(String, String, Instant)>>,
}

/// The two one-message runs beside the chat panel.
#[derive(Clone, Copy)]
enum Side {
    /// "Explain this command" on a card.
    Explain,
    /// "Is this safe to run?" on a dropped file.
    Drop,
}

impl Side {
    fn slot(self, chats: &Chats) -> &Mutex<Option<(String, u64, Cancel)>> {
        match self {
            Side::Explain => &chats.explain,
            Side::Drop => &chats.drop_run,
        }
    }

    /// The key the page's events carry instead of a chat number.
    fn key(self) -> &'static str {
        match self {
            Side::Explain => "explain",
            Side::Drop => "drop",
        }
    }
}

struct Run {
    /// The page's number for this chat.
    chat: u32,
    model: Model,
    /// Holds a session's log: no tools, and it never takes a chat's place.
    lock: Lock,
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

    /// A chat answering or an explanation running: an update waits for it.
    pub fn running(&self) -> bool {
        let answering = self
            .run
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|r| r.busy.load(Ordering::SeqCst));
        let explaining = self.explain.lock().unwrap().is_some();
        answering || explaining || self.drop_run.lock().unwrap().is_some()
    }

    /// Holds a session's text for `chat_send`; any earlier one is gone.
    pub fn hold_session(&self, id: String, text: String) {
        *self.asked.lock().unwrap() = Some((id, text, Instant::now()));
    }

    /// Takes the held text, if `id` is its ID and it's in time: single use.
    fn take_session(&self, id: &str) -> Option<String> {
        let mut asked = self.asked.lock().unwrap();
        let (held, text, at) = asked.take()?;
        (held == id && at.elapsed() < HOLD).then_some(text)
    }

    /// Forgets a session's text held longer than `HOLD`.
    pub fn expire_session(&self) {
        let mut asked = self.asked.lock().unwrap();
        if asked
            .as_ref()
            .is_some_and(|(_, _, at)| at.elapsed() >= HOLD)
        {
            *asked = None;
        }
    }

    /// Quitting: the chat, the explanation and the file answer, if any,
    /// are killed.
    pub fn end_all(&self) {
        self.end();
        self.stop_explaining();
        self.stop_drop();
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
/// confirmed one. `file`: a dropped file's ID ("Ask about it"): the message
/// becomes the held report's facts followed by it, exactly as previewed.
/// `session`: a held session's ID ("Ask about this session"): the message
/// is the text the page showed followed by it, nothing else, and the chat
/// runs with no tools; a chat with tools ends first, so the log never joins
/// one. Its follow-ups stay in that run.
/// Errors are short codes the page words: `missing`, `unconfirmed`, `busy`,
/// `model`, `empty`, `gone` (the file isn't held), `session-gone`, or Claude
/// Code's own text.
// The arguments are the page's call, one per thing a message can carry.
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn chat_send(
    app: AppHandle,
    chats: State<'_, Chats>,
    drops: State<'_, Drops>,
    chat: u32,
    prompt: String,
    model: String,
    file: Option<String>,
    session: Option<String>,
) -> Result<(), String> {
    let model = Model::parse(&model).ok_or("model")?;
    if prompt.trim().is_empty() {
        return Err("empty".into());
    }
    let sealed = session.is_some();
    let prompt = match (session, file) {
        (Some(id), _) => {
            let text = chats.take_session(&id).ok_or("session-gone")?;
            format!("{text}{prompt}")
        }
        (None, Some(id)) => drop::ask_prompt(&drops.report(&id).ok_or("gone")?, &prompt),
        (None, None) => prompt,
    };
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
        .is_some_and(|r| r.chat != chat || r.model != model || (sealed && r.lock != Lock::NoTools))
    {
        // The page moved on (New chat, another model), or a log can't join
        // this run: end the old one.
        if let Some(old) = run.take() {
            old.cancel.cancel();
        }
    }
    if run.as_ref().is_some_and(|r| r.busy.load(Ordering::SeqCst)) {
        return Err("busy".into());
    }
    if run.is_none() {
        let dir = chat::folder().ok_or("no chat folder")?;
        let lock = if sealed {
            Lock::NoTools
        } else {
            Lock::ReadOnly
        };
        let started = Chat::start_locked(&bin, &dir, lock, model, Limits::default())?;
        let (prompts, inbox) = mpsc::channel();
        let busy = Arc::new(AtomicBool::new(false));
        *run = Some(Run {
            chat,
            model,
            lock,
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

/// × on the session preview: the held text goes.
#[tauri::command]
pub fn session_ask_drop(chats: State<'_, Chats>) {
    *chats.asked.lock().unwrap() = None;
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

/// Owns one chat: answers each message, sends its events to the page, and
/// ends with the chat (dropping it kills the tree).
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
        if let (Out::Ready { model: name, .. }, Model::Default) = (out, model) {
            *chats.default_model.lock().unwrap() = Some(name.clone());
        }
        chats.send(event(id, out));
    };
    for prompt in inbox {
        if chat.send(&prompt).is_err() {
            break;
        }
        let last = answer(&mut chat, &feed);
        // Free before the page hears, so it can send the next message at once.
        busy.store(false, Ordering::SeqCst);
        let ended = matches!(last, Out::Stopped(_));
        if ended {
            let mut run = chats.run.lock().unwrap();
            if run.as_ref().is_some_and(|r| r.chat == id) {
                *run = None;
            }
        }
        feed(&last);
        if ended {
            return;
        }
    }
}

/// Streams one message's answer to `feed`, text gathered into `BATCH`es,
/// and returns its last event (`Done` or `Stopped`) unsent.
fn answer(chat: &mut Chat, feed: &dyn Fn(&Out)) -> Out {
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
    loop {
        let due = sent + BATCH;
        match chat.recv_by((!text.is_empty()).then_some(due)) {
            None => flush(&mut text, &mut sent),
            Some(Out::Text(t)) => {
                text.push_str(&t);
                if Instant::now() >= due {
                    flush(&mut text, &mut sent);
                }
            }
            Some(out) => {
                flush(&mut text, &mut sent);
                if matches!(out, Out::Done { .. } | Out::Stopped(_)) {
                    return out;
                }
                feed(&out);
            }
        }
    }
}

/// "Explain this command" on a permission card: one message in its own
/// locked chat (the same command line, `claude` and folder as the panel's),
/// streamed to the page tagged with the card's request ID. It never answers
/// the card: it only reads its text. Ends when answered, on Stop, when the
/// card leaves (`keep_explaining`), at quit, or when another one starts.
pub fn explain(
    app: &AppHandle,
    chats: &Chats,
    id: String,
    prompt: String,
    model: &str,
) -> Result<(), String> {
    side(app, chats, Side::Explain, id, prompt, model)
}

/// "Is this safe to run?" on a dropped file (`id`: the drop's ID): the same
/// kind of run as `explain`, in its own slot, so cards coming and going
/// never stop it. Ends when answered, on Stop, Close, a new drop, or quit.
pub fn ask_drop(
    app: &AppHandle,
    chats: &Chats,
    id: String,
    prompt: String,
    model: &str,
) -> Result<(), String> {
    side(app, chats, Side::Drop, id, prompt, model)
}

fn side(
    app: &AppHandle,
    chats: &Chats,
    which: Side,
    id: String,
    prompt: String,
    model: &str,
) -> Result<(), String> {
    let model = Model::parse(model).ok_or("model")?;
    if prompt.len() > chat::MAX_PROMPT {
        return Err("too-long".into());
    }
    let bin = match Claude::now() {
        Claude::Confirmed(bin) => bin,
        Claude::Missing => return Err("missing".into()),
        Claude::Unconfirmed(_) | Claude::Changed(_) => return Err("unconfirmed".into()),
    };
    let dir = chat::folder().ok_or("no chat folder")?;
    let mut slot = which.slot(chats).lock().unwrap();
    if let Some((_, _, old)) = slot.take() {
        old.cancel();
    }
    let mut run = Chat::start(&bin, &dir, model, Limits::default())?;
    let n = chats.explain_runs.fetch_add(1, Ordering::SeqCst);
    *slot = Some((id.clone(), n, run.cancel_handle()));
    drop(slot);
    let app = app.clone();
    std::thread::spawn(move || {
        let chats = app.state::<Chats>();
        let feed = |out: &Out| chats.send(tagged(which.key(), &id, out));
        let last = match run.send(&prompt) {
            Ok(()) => answer(&mut run, &feed),
            Err(e) => Out::Done { error: Some(e) },
        };
        {
            let mut slot = which.slot(&chats).lock().unwrap();
            if slot.as_ref().is_some_and(|(_, m, _)| *m == n) {
                *slot = None;
            }
        }
        feed(&last);
        // `run` drops here: its whole tree is killed.
    });
    Ok(())
}

impl Chats {
    fn send(&self, event: Value) {
        if let Some(feed) = self.feed.lock().unwrap().as_ref() {
            let _ = feed.send(event);
        }
    }

    /// Stops the explanation, if any (Stop on its card).
    pub fn stop_explaining(&self) {
        if let Some((_, _, run)) = self.explain.lock().unwrap().take() {
            run.cancel();
        }
    }

    /// Stops the file answer, if any (Stop, Close, a new drop).
    pub fn stop_drop(&self) {
        if let Some((_, _, run)) = self.drop_run.lock().unwrap().take() {
            run.cancel();
        }
    }

    /// Stops the explanation unless its card is still waiting (`waiting`:
    /// the queue's request IDs). Called with every view, so it ends as soon
    /// as the card is answered, times out or goes.
    pub fn keep_explaining(&self, waiting: &[&str]) {
        let mut slot = self.explain.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|(id, _, _)| !waiting.contains(&id.as_str()))
            && let Some((_, _, run)) = slot.take()
        {
            run.cancel();
        }
    }
}

/// An explanation's or a file answer's event as the page reads it
/// (`src/chat.ts`, `ExplainEvent` / `DropEvent`): a chat event tagged with
/// the card's request ID or the drop's ID (`key`) instead of a chat number.
fn tagged(key: &str, id: &str, out: &Out) -> Value {
    let mut v = event(0, out);
    v.as_object_mut().unwrap().remove("chat");
    v[key] = id.into();
    v
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

    /// An explanation's events are chat events tagged with the card's
    /// request ID, never a chat number (the panel's chat ignores them).
    #[test]
    fn explanation_events_carry_the_card_not_a_chat() {
        assert_eq!(
            tagged("explain", "ab12", &Out::Text("<b>rm</b>".into())),
            json!({"explain": "ab12", "kind": "text", "text": "<b>rm</b>"})
        );
        assert_eq!(
            tagged("explain", "ab12", &Out::Done { error: None }),
            json!({"explain": "ab12", "kind": "done", "error": null})
        );
        assert_eq!(
            tagged("explain", "ab12", &Out::Stopped(Stop::Cancelled)),
            json!({"explain": "ab12", "kind": "stopped", "stop": {"why": "cancelled"}})
        );
        assert_eq!(
            tagged("drop", "cd34", &Out::Text("x".into())),
            json!({"drop": "cd34", "kind": "text", "text": "x"})
        );
    }

    /// A session's text goes once, only under its own ID.
    #[test]
    fn a_held_session_is_single_use() {
        let chats = Chats::default();
        chats.hold_session("a".into(), "text".into());
        assert_eq!(chats.take_session("b"), None);
        // A wrong ID drops it too: the next try can't guess again.
        assert_eq!(chats.take_session("a"), None);
        chats.hold_session("a".into(), "text".into());
        chats.hold_session("b".into(), "newer".into());
        assert_eq!(chats.take_session("a"), None);
        chats.hold_session("b".into(), "newer".into());
        assert_eq!(chats.take_session("b").as_deref(), Some("newer"));
        assert_eq!(chats.take_session("b"), None);
    }

    /// Past the hold, it's gone from memory, not only refused.
    #[test]
    fn a_held_session_expires() {
        let chats = Chats::default();
        chats.hold_session("a".into(), "text".into());
        chats.expire_session();
        assert!(chats.asked.lock().unwrap().is_some());
        chats.asked.lock().unwrap().as_mut().unwrap().2 = Instant::now() - HOLD;
        chats.expire_session();
        assert!(chats.asked.lock().unwrap().is_none());
    }

    #[test]
    fn the_page_sees_which_claude_and_whether_it_is_confirmed() {
        assert_eq!(
            status(&Claude::Missing),
            json!({"state": "missing", "path": null})
        );
    }
}
