// The chat panel's state and words, kept apart from the DOM so they can be
// tested. The transcript lives only here, in memory; main.ts draws it with
// textContent. Events come from crates/app/src/chat.rs (`event`).

export const MODELS = ["Default", "Haiku", "Sonnet", "Opus", "Fable"] as const;
export type Model = (typeof MODELS)[number];

export type Stop =
  | { why: "cancelled" }
  | { why: "timeout" }
  | { why: "silent" }
  | { why: "unsupported"; version: string | null }
  | { why: "key"; source: string }
  | { why: "unlocked"; what: string }
  | { why: "exited"; code: number | null; message: string };

export type ChatEvent = { chat: number } & (
  | { kind: "ready"; model: string; version: string }
  | { kind: "text"; text: string }
  | { kind: "tool"; name: string; target: string }
  | { kind: "denied"; tool: string; reason: string }
  | { kind: "usage"; fiveHour: number | null; sevenDay: number | null }
  | { kind: "done"; error: string | null }
  | { kind: "stopped"; stop: Stop }
);

export type Item =
  | { kind: "me" | "ai" | "error" | "stop" | "new"; text: string }
  | { kind: "tool"; name: string; target: string; denied: boolean };

export type Chat = {
  /** The page's number for the chat; events for any other are ignored. */
  id: number;
  items: Item[];
  /** A message is being answered. */
  busy: boolean;
  /** The chat stopped: the next message starts a new one. */
  ended: boolean;
  model: Model;
  /** The plan's default model, once a Default chat named it. */
  defaultName: string | null;
  /** The plan's use, 0–1, once Claude Code reported it. */
  usage: { five: number | null; seven: number | null } | null;
};

export function fresh(model: Model = "Default"): Chat {
  return { id: 1, items: [], busy: false, ended: false, model, defaultName: null, usage: null };
}

/** A new, empty chat (the head's "New chat"); the plan's facts carry over. */
export function clear(chat: Chat): Chat {
  return { ...chat, id: chat.id + 1, items: [], busy: false, ended: false };
}

/** Another model: the next message starts a new chat, said with a line. */
export function switchModel(chat: Chat, model: Model): Chat {
  if (model === chat.model) return chat;
  const items = chat.items.length && chat.items.at(-1)?.kind !== "new" ? [...chat.items, line(`New chat · ${model}`)] : chat.items;
  return { ...chat, id: chat.id + 1, items, busy: false, ended: false, model };
}

const line = (text: string): Item => ({ kind: "new", text });

/** Sending a session's log: a new chat, with no tools. The old one's words
 * stay on screen, said with a line. */
export function askedChat(chat: Chat): Chat {
  const items = chat.items.length && chat.items.at(-1)?.kind !== "new" ? [...chat.items, line("New chat · no tools")] : chat.items;
  return { ...chat, id: chat.id + 1, items, busy: false, ended: false };
}

/** How long the backend holds a session's log (`HOLD`). */
export const ASK_HOLD_MS = 10 * 60_000;

/** Which sessions Ask works for (the backend decides each one). */
export const ASK_AGENTS = "Ask about this session works for Claude Code and Codex sessions.";

/** Beside the preview: the answer can't see everything, and the preview is
 * the last check. */
export const PARTIAL_LOG = "Partial log: no outputs or subagent work. Redaction can miss things, so read it first.";

/** The user sent `text`: it shows at once, and the chat waits for the answer. */
export function begin(chat: Chat, text: string): Chat {
  const next = chat.ended ? { ...chat, id: chat.id + 1, ended: false } : chat;
  return { ...next, items: [...next.items, { kind: "me", text }], busy: true };
}

/** Sending was refused (`code` from chat_send). */
export function refused(chat: Chat, code: string): Chat {
  return { ...chat, busy: false, items: [...chat.items, { kind: "error", text: sendError(code) }] };
}

export function apply(chat: Chat, e: ChatEvent): Chat {
  if (e.chat !== chat.id) return chat;
  const items = [...chat.items];
  switch (e.kind) {
    case "ready":
      return chat.model === "Default" ? { ...chat, defaultName: e.model } : chat;
    case "usage":
      return { ...chat, usage: { five: e.fiveHour, seven: e.sevenDay } };
    case "text": {
      const last = items.at(-1);
      if (last?.kind === "ai") items[items.length - 1] = { kind: "ai", text: last.text + e.text };
      else items.push({ kind: "ai", text: e.text });
      return { ...chat, items };
    }
    case "tool":
      items.push({ kind: "tool", name: e.name, target: e.target, denied: false });
      return { ...chat, items };
    case "denied":
      items.push({ kind: "tool", name: e.tool, target: e.reason, denied: true });
      return { ...chat, items };
    case "done":
      if (e.error) items.push({ kind: "error", text: e.error });
      return { ...chat, items, busy: false };
    case "stopped": {
      const why = stopText(e.stop);
      items.push({ kind: e.stop.why === "cancelled" ? "stop" : "error", text: why }, line("New chat"));
      return { ...chat, items, busy: false, ended: true };
    }
  }
}

export function stopText(stop: Stop): string {
  switch (stop.why) {
    case "cancelled":
      return "Cancelled.";
    case "timeout":
      return "Stopped: no answer within 5 minutes.";
    case "silent":
      return "Stopped: Claude Code went quiet for 60 seconds.";
    case "unsupported":
      return `This Claude Code version isn't supported for chat${stop.version ? ` (${stop.version})` : ""}. Update Claude Code, then start a new chat.`;
    case "key":
      return `Stopped: Claude Code wasn't using your Claude login (it used ${stop.source}). Chat only runs on your plan.`;
    case "unlocked":
      return `Stopped: the chat wasn't locked down (${stop.what}).`;
    case "exited":
      return `Claude Code stopped${stop.code === null ? "" : ` (exit ${stop.code})`}: ${stop.message}`;
  }
}

export function sendError(code: string): string {
  switch (code) {
    case "missing":
      return "Claude Code wasn't found. Install it, then open chat again.";
    case "unconfirmed":
      return "Confirm the claude path in Settings first.";
    case "busy":
      return "Still answering. Wait, or Cancel.";
    case "model":
      return "That model isn't on the list.";
    case "empty":
      return "Type a message first.";
    case "too-long":
      return "That message is too long.";
    case "ended":
      return "That chat ended. Send again to start a new one.";
    case "gone":
      return "That request isn't waiting any more.";
    case "file-gone":
      return "Bouncer no longer holds that file. Drop it again.";
    case "session-gone":
      return "That preview is gone. Press Ask about this session again.";
    case "tool":
      return "Only commands can be explained.";
    default:
      return code;
  }
}

/** Shown in place of the plan's use until a chat reply brings a reading
 * (chats are memory-only: after a restart there is none). */
export const NO_USAGE_YET = "Plan use shows after your first message";

/** 0.23 → "23%"; unknown → "—". */
export function percent(x: number | null | undefined): string {
  return typeof x === "number" && Number.isFinite(x) ? `${Math.round(Math.min(1, Math.max(0, x)) * 100)}%` : "—";
}

export function defaultNote(name: string | null): string {
  return name ? `Default is ${name}` : "Default is your plan's default; named after its first reply.";
}

/** What a key does in the message box: Enter sends, Shift+Enter (or an
 * IME still composing) types a new line. */
export function messageKey(e: { key: string; shiftKey: boolean; isComposing: boolean }): "send" | null {
  return e.key === "Enter" && !e.shiftKey && !e.isComposing ? "send" : null;
}

/** Where the keyboard goes when a card arrives: to its Deny, unless the
 * user was typing a chat message (then nowhere, so a space or Enter in
 * mid-sentence can't answer the card). */
export function cardFocus(had: string | null): "deny" | "nowhere" {
  return had === CHAT_INPUT ? "nowhere" : "deny";
}

export const CHAT_INPUT = "chat-input";

/** What Esc closes: Settings opened over the chat closes alone (back to the
 * chat, keyboard kept); anything else closes all and gives the keyboard
 * back (a card stays up either way). */
export function escapeCloses(view: { settings: boolean; chat: boolean; queue: unknown[] }): "settings" | "all" {
  return view.settings && view.chat && !view.queue.length ? "settings" : "all";
}

/** The card the keyboard is kept off, after a render: one that arrived while
 * a chat message was being typed keeps it nowhere through its re-renders
 * (its arm, the clock), until the user moves it (Tab) or the card goes. */
export function heldCard(held: string, card: string | null, newCard: boolean, had: string | null): string {
  if (card && newCard && cardFocus(had) === "nowhere") return card;
  if (!card || card !== held || (had && !newCard)) return "";
  return held;
}

// ---- "Explain this command" (roadmap b) -------------------------------------
// The locked chat explains a waiting card's shell command. It only reads:
// nothing here can answer the card, and the card's buttons never wait on it.

/** `pressed` for a press on an answer that acts (Allow, Send answer,
 * Approve plan): the keyboard arriving with it goes nowhere. */
export const PRESSED_ANSWER = "answer";

/** One of this card's own controls that may hold the keyboard: an option,
 * Deny, Keep planning, the plan's box, Explain (or its box). Never Allow,
 * Send answer or Approve plan. */
function ownControl(card: string, key: string): boolean {
  return [`explain:${card}`, `deny:${card}`, `keep:${card}`, `plan:${card}`].includes(key) || key.startsWith(`opt:${card}:`);
}

/** Where the keyboard goes when it arrives with a card up. Brought by a
 * press on one of that card's own controls (`pressed`, its focus key): it
 * stays there, so the ring follows the click (Charan, 2026-10-16). Brought
 * by a press on an answer that acts (`PRESSED_ANSWER`): nowhere (""). Else
 * the card's own start: a question's first option (a pick only selects), a
 * plan's Keep planning (the "no"), a permission's Deny. Never Allow, Send
 * answer or Approve plan. */
export function arrivalFocus(card: string, pressed: string | null, kind = "permission"): string {
  if (pressed === PRESSED_ANSWER) return "";
  if (pressed && ownControl(card, pressed)) return pressed;
  if (kind === "question") return `opt:${card}:0:0`;
  if (kind === "plan") return `keep:${card}`;
  return `deny:${card}`;
}

/** Cards whose command can be explained (the backend checks again). */
export const EXPLAINED_TOOLS = new Set(["Bash", "PowerShell"]);

/** An explanation's event: a chat event tagged with the card's request ID. */
export type ExplainEvent = { explain: string } & (
  | { kind: "ready"; model: string; version: string }
  | { kind: "text"; text: string }
  | { kind: "tool"; name: string; target: string }
  | { kind: "denied"; tool: string; reason: string }
  | { kind: "usage"; fiveHour: number | null; sevenDay: number | null }
  | { kind: "done"; error: string | null }
  | { kind: "stopped"; stop: Stop }
);

export type Explain = {
  /** The card's request ID. */
  id: string;
  text: string;
  /** Still answering (Stop shows); else Hide. */
  busy: boolean;
  /** Why it stopped short, in words. */
  error: string | null;
};

export function explainStart(id: string): Explain {
  return { id, text: "", busy: true, error: null };
}

/** An event for the explanation on screen; anything else is ignored. */
export function explainApply(x: Explain | null, e: ExplainEvent): Explain | null {
  if (!x || e.explain !== x.id || !x.busy) return x;
  switch (e.kind) {
    case "text":
      return { ...x, text: x.text + e.text };
    case "done":
      return { ...x, busy: false, error: e.error };
    case "stopped":
      return { ...x, busy: false, error: e.stop.why === "cancelled" ? "Stopped." : stopText(e.stop) };
    default:
      // Tools, usage and the model name aren't shown for an explanation.
      return x;
  }
}

/** Starting it was refused (`code` from explain). */
export function explainRefused(x: Explain | null, id: string, code: string): Explain | null {
  return x?.id === id && x.busy ? { ...x, busy: false, error: sendError(code) } : x;
}
