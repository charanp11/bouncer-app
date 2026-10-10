import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { apply, begin, cardFocus, CHAT_INPUT, clear, defaultNote, escapeCloses, fresh, heldCard, messageKey, MODELS, percent, refused, sendError, stopText, switchModel, type Chat, type ChatEvent } from "./chat.ts";

const run = (chat: Chat, events: Omit<ChatEvent, "chat">[]): Chat =>
  events.reduce((c, e) => apply(c, { ...e, chat: c.id } as ChatEvent), chat);

test("chat: a streamed answer joins into one bubble, markup stays text, tools and denials sit between", () => {
  let c = begin(fresh("Haiku"), "What's in <b>notes.txt</b>?");
  assert.equal(c.busy, true);
  c = run(c, [
    { kind: "ready", model: "claude-haiku-5-5", version: "2.1.295" },
    { kind: "tool", name: "Read", target: "notes.txt" },
    { kind: "denied", tool: "Read", reason: "--restricted: path outside the working directory" },
    { kind: "text", text: "<img src=x onerror=alert(1)>" },
    { kind: "text", text: " and more" },
    { kind: "done", error: null },
  ]);
  assert.deepEqual(c.items, [
    { kind: "me", text: "What's in <b>notes.txt</b>?" },
    { kind: "tool", name: "Read", target: "notes.txt", denied: false },
    { kind: "tool", name: "Read", target: "--restricted: path outside the working directory", denied: true },
    { kind: "ai", text: "<img src=x onerror=alert(1)> and more" },
  ]);
  assert.equal(c.busy, false);
  assert.equal(c.defaultName, null, "only a Default chat names the default");
});

test("chat: events for another chat are ignored", () => {
  const c = begin(fresh(), "hi");
  assert.equal(apply(c, { chat: c.id + 1, kind: "text", text: "stale" }), c);
  assert.equal(apply(c, { chat: c.id - 1, kind: "done", error: null }), c);
});

test("chat: the Default model is named by its first reply", () => {
  let c = begin(fresh("Default"), "hi");
  assert.equal(defaultNote(c.defaultName), "Default is your plan's default; named after its first reply.");
  c = run(c, [{ kind: "ready", model: "claude-opus-5-5", version: "2.1.295" }]);
  assert.equal(c.defaultName, "claude-opus-5-5");
  assert.equal(defaultNote(c.defaultName), "Default is claude-opus-5-5");
  // It carries over to new chats and other models.
  assert.equal(switchModel(clear(c), "Sonnet").defaultName, "claude-opus-5-5");
  assert.deepEqual(MODELS, ["Default", "Haiku", "Sonnet", "Opus", "Fable"]);
});

test("chat: cancel ends the chat with a New chat line; the next message starts a new one", () => {
  let c = begin(fresh(), "long question");
  c = run(c, [{ kind: "text", text: "Starting" }, { kind: "stopped", stop: { why: "cancelled" } }]);
  assert.deepEqual(c.items.slice(-2), [{ kind: "stop", text: "Cancelled." }, { kind: "new", text: "New chat" }]);
  assert.equal(c.busy, false);
  assert.equal(c.ended, true);
  const id = c.id;
  c = begin(c, "again");
  assert.equal(c.id, id + 1, "a new chat number, so the backend starts a new chat");
  assert.equal(c.ended, false);
});

test("chat: switching model starts a new chat, said with a line; New chat clears", () => {
  let c = run(begin(fresh("Haiku"), "hi"), [{ kind: "text", text: "hello" }, { kind: "done", error: null }]);
  const s = switchModel(c, "Sonnet");
  assert.equal(s.id, c.id + 1);
  assert.equal(s.model, "Sonnet");
  assert.deepEqual(s.items.at(-1), { kind: "new", text: "New chat · Sonnet" });
  assert.equal(switchModel(s, "Sonnet"), s);
  assert.deepEqual(switchModel(fresh("Haiku"), "Opus").items, [], "nothing to separate yet");
  c = clear(s);
  assert.deepEqual(c.items, []);
  assert.equal(c.id, s.id + 1);
  assert.equal(c.model, "Sonnet");
});

test("chat: errors read as Claude Code wrote them or say plainly what's wrong", () => {
  let c = run(begin(fresh("Fable"), "hi"), [{ kind: "done", error: "Claude Fable isn't available on your plan." }]);
  assert.deepEqual(c.items.at(-1), { kind: "error", text: "Claude Fable isn't available on your plan." });
  assert.equal(c.ended, false, "a plan error doesn't end the chat");
  c = refused(begin(fresh(), "hi"), "missing");
  assert.deepEqual(c.items.at(-1), { kind: "error", text: "Claude Code wasn't found. Install it, then open chat again." });
  assert.equal(c.busy, false);
  assert.equal(sendError("unconfirmed"), "Confirm the claude path in Settings first.");
  assert.equal(sendError("can't create C:\\x: denied"), "can't create C:\\x: denied");
  assert.equal(
    stopText({ why: "unsupported", version: "2.1.120" }),
    "This Claude Code version isn't supported for chat (2.1.120). Update Claude Code, then start a new chat.",
  );
  assert.equal(stopText({ why: "unsupported", version: null }), "This Claude Code version isn't supported for chat. Update Claude Code, then start a new chat.");
  assert.match(stopText({ why: "key", source: "ANTHROPIC_API_KEY" }), /wasn't using your Claude login/);
  assert.equal(stopText({ why: "exited", code: 3, message: "boom" }), "Claude Code stopped (exit 3): boom");
  assert.equal(stopText({ why: "timeout" }), "Stopped: no answer within 5 minutes.");
  assert.equal(stopText({ why: "silent" }), "Stopped: Claude Code went quiet for 60 seconds.");
  const unsupported = run(begin(fresh(), "hi"), [{ kind: "stopped", stop: { why: "unsupported", version: "2.1.120" } }]);
  assert.equal(unsupported.items.at(-2)?.kind, "error");
  assert.deepEqual(unsupported.items.at(-1), { kind: "new", text: "New chat" });
});

test("chat: the usage line shows the plan's 5-hour and 7-day use", () => {
  const c = run(begin(fresh(), "hi"), [{ kind: "usage", fiveHour: 0.234, sevenDay: 0.7 }]);
  assert.deepEqual(c.usage, { five: 0.234, seven: 0.7 });
  assert.equal(percent(c.usage?.five), "23%");
  assert.equal(percent(c.usage?.seven), "70%");
  assert.equal(percent(null), "—");
  assert.equal(percent(undefined), "—");
  assert.equal(percent(1.4), "100%");
  assert.equal(percent(Number.NaN), "—");
  assert.equal(clear(c).usage, c.usage, "the plan's use carries over");
});

test("chat: Enter sends, Shift+Enter and an IME still composing make a new line", () => {
  assert.equal(messageKey({ key: "Enter", shiftKey: false, isComposing: false }), "send");
  assert.equal(messageKey({ key: "Enter", shiftKey: true, isComposing: false }), null);
  assert.equal(messageKey({ key: "Enter", shiftKey: false, isComposing: true }), null);
  assert.equal(messageKey({ key: "a", shiftKey: false, isComposing: false }), null);
  assert.equal(messageKey({ key: " ", shiftKey: false, isComposing: false }), null);
});

test("chat: a card arriving while you type takes the keyboard nowhere, else to Deny (never Allow)", () => {
  assert.equal(cardFocus(CHAT_INPUT), "nowhere");
  assert.equal(cardFocus(null), "deny");
  assert.equal(cardFocus("auto"), "deny");
});

test("chat: a card that came while you typed keeps the keyboard off through its re-renders", () => {
  // The hand-made bug: the card's arm re-rendered, focus fell back to Deny,
  // and a typed space denied it.
  let held = heldCard("", "c1", true, CHAT_INPUT);
  assert.equal(held, "c1");
  held = heldCard(held, "c1", false, null);
  assert.equal(held, "c1", "the arm's re-render keeps it nowhere");
  assert.equal(heldCard(held, "c1", false, "deny:c1"), "", "Tab to Deny on purpose ends the hold");
  assert.equal(heldCard(held, null, false, null), "", "the card went");
  assert.equal(heldCard(held, "c2", true, null), "", "the next card goes to its Deny as usual");
  assert.equal(heldCard("", "c3", true, "auto"), "", "not typing: Deny as usual");
});

test("chat: the panel never resizes while streaming and nothing in it moves forever", () => {
  const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  /** The body of the rule whose selector is exactly `sel`. */
  const rule = (sel: string) => {
    const at = css.indexOf(`\n${sel} {`);
    assert.ok(at >= 0, `no rule ${sel}`);
    return css.slice(at, css.indexOf("}", at));
  };
  // A fixed height: the window holds still while an answer streams in; only the log scrolls.
  assert.match(rule(".island.chat"), /height: min\(var\(--chat-h\), var\(--open-max-h\)\);/);
  assert.match(css, /--chat-h: 460px;/);
  assert.match(rule(".log"), /overflow-y: auto;/);
  // Never wider than the panel, whatever the model or a tool sends.
  assert.match(rule(".log"), /grid-template-columns: minmax\(0, 1fr\);/);
  assert.match(rule(".msg"), /overflow-wrap: anywhere;/);
  assert.match(rule(".body.chat"), /overflow: hidden;/);
  // The message box's caret doesn't blink (a blink repaints for as long as it has focus).
  assert.match(rule(".compose textarea"), /caret-animation: manual;/);
  // Answers are selectable text; the island around them isn't.
  assert.match(rule(".log"), /user-select: text;/);
});

test("chat: Esc in Settings opened from the chat closes only Settings", () => {
  assert.equal(escapeCloses({ settings: true, chat: true, queue: [] }), "settings");
  assert.equal(escapeCloses({ settings: true, chat: false, queue: [] }), "all");
  assert.equal(escapeCloses({ settings: false, chat: true, queue: [] }), "all");
  assert.equal(escapeCloses({ settings: true, chat: true, queue: [{}] }), "all", "a card up: as before");
});
