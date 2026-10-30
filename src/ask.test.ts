import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { askedChat, ASK_AGENTS, begin, fresh, PARTIAL_LOG, sendError, switchModel } from "./chat.ts";

// "Ask about this session" (PLAN, roadmap f). The backend reads the session
// and holds the exact text; the page shows it and sends back only its ID.
const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const proto = readFileSync(new URL("../design/prototype/bouncer-island.html", import.meta.url), "utf8");

function fn(name: string): string {
  const start = main.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, name);
  return main.slice(start, main.indexOf("\n}\n", start));
}

test("sending the log starts a new chat, said with a line, so it never joins one with tools", () => {
  const chat = begin(fresh(), "hi");
  const next = askedChat({ ...chat, busy: false });
  assert.equal(next.id, chat.id + 1);
  assert.deepEqual(next.items.at(-1), { kind: "new", text: "New chat · no tools" });
  assert.equal(askedChat(fresh()).items.length, 0, "nothing to say over an empty chat");
  assert.equal(next.model, chat.model);
  // Another model starts a new chat too: the log stays behind in the old run.
  assert.notEqual(switchModel(next, "Haiku").id, next.id);
  // On Send, not on Ask: × leaves the chat as it was.
  assert.match(fn("sendChat"), /if \(asked\) chat = askedChat\(chat\);/);
  assert.doesNotMatch(fn("askParts"), /askedChat|chat_new/);
});

test("the page sends the held text's ID, never the text", () => {
  const send = fn("sendChat");
  assert.match(send, /session: asked\?\.id \?\? null/);
  assert.doesNotMatch(send, /asked\.text/);
  assert.match(send, /sessionAsk = null;/, "one message only");
});

test("the preview says where it goes, shows the backend's text, and can be dropped", () => {
  const part = fn("sessionPart");
  assert.match(part, /a\.destination/);
  assert.match(part, /PARTIAL_LOG/);
  assert.match(part, /attachBox\("Session", [^,]+, "Don't send the log", a\.text,/);
  assert.match(part, /dropSessionAsk\(\)/);
  assert.match(fn("dropSessionAsk"), /invoke\("session_ask_drop"\)/);
  assert.match(fn("attachBox"), /linesInto\(pre, preview\)/, "drawn as text");
  assert.match(PARTIAL_LOG, /no outputs or subagent work/);
});

test("Ask reads only on the press, and other agents get a line, not a button", () => {
  const parts = fn("askParts");
  assert.match(parts, /if \(!s\.ask\) return \[el\("div", "after", ASK_AGENTS\)\];/);
  const press = parts.indexOf("addEventListener");
  assert.ok(parts.indexOf(`invoke<Omit<SessionAsk, "name">>("session_ask"`) > press);
  assert.equal(main.split(`"session_ask"`).length - 1, 1, "called from one place");
});

test("a send that fails keeps the log and the question", () => {
  const send = fn("sendChat");
  assert.match(send, /if \(asked\) \{\n\s+draft = text;\n\s+if \(code !== "session-gone"\) sessionAsk = asked;/);
});

test("the log and a file never go together, and the page lets go when the backend does", () => {
  assert.match(main, /dropSessionAsk\(\);\n\s+attached = \{ id: d\.id/);
  assert.match(fn("askParts"), /attached = null;/);
  assert.match(fn("askParts"), /setTimeout\([\s\S]*sessionAsk = null;[\s\S]*ASK_HOLD_MS\)/);
});

test("Settings says which agents it works for, and a stale ID is said plainly", () => {
  assert.match(fn("claudeRow"), /\$\{hint\} \$\{ASK_AGENTS\}/);
  assert.equal(ASK_AGENTS, "Ask about this session works for Claude Code and Codex sessions.");
  assert.match(sendError("session-gone"), /Press Ask about this session again/);
});

test("a redraw keeps the reader's place in the log", () => {
  assert.match(fn("sessionPart"), /pre\.dataset\.key = "session-log";/, "focus comes back by key");
  assert.match(fn("sessionPart"), /askScroll = pre\.scrollTop/);
  assert.match(fn("renderChat"), /read\.scrollTop = askScroll/);
});

test("the prototype has the state", () => {
  assert.match(proto, /data-s="chatsession"/);
  assert.match(proto, /Sent to Claude through your Claude Code\. It leaves Codex \(OpenAI\) for Anthropic\./);
  assert.ok(proto.includes(PARTIAL_LOG));
});
