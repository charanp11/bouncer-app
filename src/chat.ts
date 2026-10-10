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
    default:
      return code;
  }
}

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
