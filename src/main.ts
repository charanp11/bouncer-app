// The island, built to design/prototype/bouncer-island.html.
//
// Everything that came from an agent goes in as text (textContent / text
// nodes), never HTML; the backend has already turned hidden characters into
// `\u{XXXX}` markers, shown here as red tags. Allow arms after 600 ms, is never
// focused (the window can't take focus at all), and Enter can't reach it.
import { Channel, invoke } from "@tauri-apps/api/core";
import { ball, type BallState } from "./ball.ts";
import { tokenize } from "./tokenize.ts";
import "./styles.css";

type Line = { n: number | null; mark: string; text: string };
type Code = {
  file: string;
  path: string;
  badge: string;
  syntax: string;
  changes: number;
  lines: Line[];
};
type Step = { label: string; how: string };
type Session = {
  id: string;
  agent: string;
  project: string;
  step: string;
  status: string;
  started_ms: number;
  last_ms: number;
  history: Step[];
  code: Code | null;
};
type Request = {
  id: string;
  agent: string;
  session: string;
  project: string;
  tool: string | null;
  text: string;
  code: Code | null;
};
type View = {
  open: boolean;
  hidden: boolean;
  paused: boolean;
  rounded: boolean;
  sessions: Session[];
  queue: Request[];
};

/** Allow stays disabled this long after a card reaches the front. */
const ARM_MS = 600;
const TOAST_MS = 1400;
const ROTATE_MS = 4000;
const PEEK_MS = 300;
const CLOCK_MS = 30_000;
const OPEN_MS = 320;
const CLOSE_MS = 200;
const AGENTS: Record<string, string> = { "claude-code": "Claude Code" };
/** Requests approved with the diff in view (the wide "Approve an edit"). */
const EDITS = new Set(["Edit", "MultiEdit"]);
const ORDER: Record<string, number> = {
  "needs you": 0,
  working: 1,
  "asks in terminal": 2,
  idle: 3,
};
const ROW: Record<string, string> = {
  "needs you": "needs",
  working: "working",
  "asks in terminal": "terminal",
};
/** How a step got through, as the detail rail's tag class. */
const TAG: Record<string, string> = {
  "you allowed": "mine",
  "you denied": "flag",
  "waiting for you": "you",
  "asked in terminal": "you",
};
const HIDDEN = /\\u\{([0-9A-F]{4,6})\}/g;

const stage = document.getElementById("stage")!;
const island = document.getElementById("island")!;

let current: View | null = null;
/** The request shown at the front, and when it got there (for the arm delay). */
let front = { id: "", since: 0 };
/** Shown for TOAST_MS after a decision, in place of the request. */
let toast: { request: Request; ok: boolean; until: number } | null = null;
/** Session shown in Session detail. */
let detail: string | null = null;
/** The wake strip is being hovered: show the idle pill. */
let peek = false;
let dragged = false;

// One pending re-render (arm, toast, label rotation, clock). Cleared on every
// render, so nothing runs while the island is hidden.
let timer = 0;
let timerAt = 0;
function later(ms: number) {
  const at = performance.now() + ms;
  if (timerAt && timerAt <= at) return;
  clearTimeout(timer);
  timerAt = at;
  timer = setTimeout(() => {
    timerAt = 0;
    if (current) render(current);
  }, ms);
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className = "",
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function folder(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

function what(tool: string | null): string {
  switch (tool) {
    case "Bash":
      return "Run a command";
    case "Write":
      return "Write a file";
    case "Edit":
    case "MultiEdit":
      return "Edit a file";
    case "NotebookEdit":
      return "Edit a notebook";
    case "Read":
      return "Read a file";
    case "Grep":
    case "Glob":
      return "Search files";
    case "WebFetch":
      return "Fetch a web page";
    case "WebSearch":
      return "Search the web";
    case "Task":
    case "Agent":
      return "Start a subagent";
    case null:
      return "Use a tool";
    default:
      return `Use ${tool}`;
  }
}

function ago(ms: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  return `${Math.floor(s / 3600)}h`;
}

function minutes(ms: number): string {
  const m = Math.max(0, Math.floor((Date.now() - ms) / 60_000));
  return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${m % 60} min`;
}

/** Agent text as text nodes; `\u{XXXX}` markers become red tags. */
function agentText(parent: HTMLElement, text: string, syntax = "") {
  const plain = (part: string) => {
    for (const t of tokenize(part, syntax)) {
      parent.append(t.cls ? el("span", t.cls, t.text) : t.text);
    }
  };
  let last = 0;
  for (const m of text.matchAll(HIDDEN)) {
    plain(text.slice(last, m.index));
    parent.append(el("span", "hidden-char", `U+${m[1]}`));
    last = m.index + m[0].length;
  }
  plain(text.slice(last));
}

/** A press that moves a few pixels drags the island (the backend moves the
 * window); a press that doesn't is a normal click. */
function draggable(node: HTMLElement) {
  node.addEventListener("mousedown", (down) => {
    if (down.button !== 0 || (down.target as HTMLElement).closest("button")) return;
    dragged = false;
    const move = (e: MouseEvent) => {
      if (Math.abs(e.screenX - down.screenX) + Math.abs(e.screenY - down.screenY) > 4) {
        stop();
        dragged = true;
        invoke("drag");
      }
    };
    const stop = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", stop);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", stop);
  });
}

function mood(view: View): BallState {
  if (view.paused) return "paused";
  if (view.queue.length || view.sessions.some((s) => s.status === "needs you")) return "needs";
  if (view.sessions.some((s) => s.status === "working")) return "working";
  return "idle";
}

// ---- pieces ----------------------------------------------------------------

function pill(state: BallState, strong: string, rest: string, clickable = true): HTMLElement {
  const p = el("div", "pill");
  const label = el("span", "label");
  label.append(el("b", "", strong), rest);
  p.append(ball(state), label);
  if (clickable) {
    p.setAttribute("role", "button");
    p.addEventListener("click", () => {
      if (!dragged) invoke("expand", { open: true });
    });
    draggable(p);
  }
  return p;
}

function closeButton(onClick: () => void): HTMLElement {
  const close = el("button", "iconbtn", "×");
  close.setAttribute("aria-label", "Close");
  close.addEventListener("click", onClick);
  return close;
}

function head(state: BallState, title: string, sub = "", onClose?: () => void): HTMLElement {
  const h = el("div", "head");
  const t = el("div", "title");
  t.append(ball(state), el("span", "", title));
  if (sub) t.append(el("span", "sub", sub));
  h.append(t);
  if (onClose) h.append(closeButton(onClose));
  draggable(h);
  return h;
}

function stepText(s: Session): string {
  if (s.status === "needs you") return `Needs you · ${s.step}`;
  if (s.status === "asks in terminal") return `Asking in the terminal · ${s.step}`;
  return s.step;
}

function sessionRows(sessions: Session[]): HTMLElement {
  const ul = el("ul", "rows");
  const sorted = [...sessions].sort(
    (a, b) => (ORDER[a.status] ?? 9) - (ORDER[b.status] ?? 9) || b.last_ms - a.last_ms,
  );
  for (const s of sorted) {
    const li = el("li", `row ${ROW[s.status] ?? ""}`);
    li.title = s.project;
    li.append(
      el("span", "dot"),
      el("span", "name", folder(s.project)),
      el("span", "time", ago(s.last_ms)),
      el("span", "step", stepText(s)),
    );
    li.addEventListener("click", () => {
      detail = s.id;
      if (current) render(current);
    });
    ul.append(li);
  }
  return ul;
}

function decide(request: Request, allow: boolean, buttons: HTMLButtonElement[]) {
  for (const b of buttons) b.disabled = true;
  invoke("decide", { id: request.id, allow })
    .then(() => {
      toast = { request, ok: allow, until: performance.now() + TOAST_MS };
    })
    .catch(() => {
      // Refused (too soon, or already answered): show the current state again.
    })
    .finally(() => current && render(current));
}

/** Deny and Allow for a request; Allow arms ARM_MS after the card arrived. */
function answerButtons(request: Request, allowLabel: string): HTMLButtonElement[] {
  const deny = el("button", "btn deny", "Deny");
  const allow = el("button", "btn allow");
  const fill = el("span", "arm");
  allow.append(fill, el("span", "label", allowLabel));
  const waited = performance.now() - front.since;
  if (waited < ARM_MS) {
    allow.disabled = true;
    // Keep the fill continuous across re-renders.
    fill.style.animationDelay = `${-waited}ms`;
    later(ARM_MS - waited);
  }
  const buttons = [deny, allow];
  deny.addEventListener("click", () => decide(request, false, buttons));
  allow.addEventListener("click", () => decide(request, true, buttons));
  return buttons;
}

/** What the card shows: a file write as its path, a blank line, then the whole
 * content (as the prototype); anything else exactly as the backend sent it. */
function cardText(request: Request): string {
  if (request.tool === "Write" && request.code) {
    return `${request.code.path}\n\n${request.code.lines.map((l) => l.text).join("\n")}`;
  }
  return request.text;
}

function card(request: Request, queue: Request[], done?: boolean): HTMLElement {
  const c = el("section", "card");
  if (done !== undefined) {
    c.append(
      el("div", `toast ${done ? "ok" : "no"}`, done ? "Allowed. Back to work." : "Denied. Claude Code was told no."),
    );
    return c;
  }
  const top = el("div", "top");
  const who = el("span");
  who.append(el("b", "", AGENTS[request.agent] ?? request.agent), ` · ${folder(request.project)}`);
  top.append(who, el("span", "", queue.length > 1 ? `1 of ${queue.length}` : ""));
  const cmd = el("pre", "cmd");
  agentText(cmd, cardText(request));
  const actions = el("div", "actions");
  actions.append(...answerButtons(request, "Allow once"));
  c.append(top, el("div", "what", what(request.tool)), cmd, actions);
  const next = queue[1];
  if (next) c.append(el("div", "after", `Next: ${what(next.tool)} in ${folder(next.project)}`));
  return c;
}

function codePane(code: Code | null, caret: boolean): HTMLElement {
  const pane = el("div", "code");
  const bar = el("div", "filebar");
  const lines = el("div", "lines");
  if (!code) {
    bar.append(el("span", "fname", "—"));
    lines.append(el("div", "empty", "Nothing to show for this step."));
    pane.append(bar, lines);
    return pane;
  }
  bar.append(el("span", "lang", code.badge), el("span", "fname", code.file));
  if (code.changes) bar.append(el("span", "mod"));
  const path = el("span", "path", code.path);
  path.title = code.path;
  bar.append(path);
  const lastAdd = caret ? code.lines.map((l) => l.mark).lastIndexOf("+") : -1;
  code.lines.forEach((line, i) => {
    const kind = line.mark === "-" ? " del" : line.mark === "+" ? " add" : line.mark === "~" ? " gap" : "";
    const row = el("div", `ln${kind}`);
    const t = el("span", "t");
    agentText(t, line.text, line.mark === "~" ? "" : code.syntax);
    if (i === lastAdd) t.append(el("span", "caret"));
    const mark = line.mark === "-" || line.mark === "+" ? line.mark : "";
    row.append(el("span", "n", line.n === null ? "" : String(line.n)), el("span", "m", mark), t);
    lines.append(row);
  });
  pane.append(bar, lines);
  return pane;
}

function rail(session: Session | undefined, project: string, agent: string, state: BallState): HTMLElement {
  const who = el("div", "who");
  const proj = el("div", "proj", folder(project));
  proj.title = project;
  const age = session ? ` · ${minutes(session.started_ms)}` : "";
  const steps = el("ul", "steps");
  const history = session?.history ?? [];
  history.forEach((step, i) => {
    const now = i === history.length - 1 && session?.status !== "idle";
    const icon = !now ? "ok" : step.how === "waiting for you" ? "wait" : "spin";
    const li = el("li", now ? "now" : "done");
    li.append(el("span", `ic ${icon}`), el("span", "", step.label));
    if (step.how) li.append(el("span", `tag ${TAG[step.how] ?? ""}`, step.how));
    steps.append(li);
  });
  who.append(ball(state, true), proj, el("div", "agent", `${AGENTS[agent] ?? agent}${age}`), steps);
  return who;
}

// ---- states ----------------------------------------------------------------

type Shape = "hidden" | "pill" | "open" | "wide";
let shape: Shape = "hidden";
let growUntil = 0;

function setShape(next: Shape) {
  const order = { hidden: 0, pill: 1, open: 2, wide: 3 };
  if (next !== shape) {
    const closing = order[next] < order[shape];
    island.classList.toggle("closing", closing);
    // While the width animates, the window keeps the larger of old and new.
    growUntil = performance.now() + (closing ? CLOSE_MS : OPEN_MS) + 40;
    setTimeout(report, (closing ? CLOSE_MS : OPEN_MS) + 60);
    shape = next;
  }
  island.classList.remove("hidden", "open", "wide");
  if (next !== "pill") island.classList.add(next);
  stage.className = next === "hidden" ? "bare" : "";
}

function renderApproval(view: View, request: Request, done?: boolean) {
  const edit = EDITS.has(request.tool ?? "") && request.code;
  const session = view.sessions.find((s) => s.id === request.session);
  if (edit && request.code) {
    setShape("wide");
    const bar = el("div", "bar");
    draggable(bar);
    const d = el("div", "detail");
    const pane = codePane(request.code, false);
    const ask = el("div", "askbar");
    if (done !== undefined) {
      ask.append(
        el("div", `toast ${done ? "ok" : "no"}`, done ? "Allowed. Writing the change." : "Denied. The file is unchanged."),
      );
    } else {
      const q = el("div", "q");
      const count = view.queue.length > 1 ? ` · 1 of ${view.queue.length}` : "";
      q.append("Wants to ", el("b", "", `edit ${request.code.file}`), ` (${plural(request.code.changes, "change")})${count}`);
      ask.append(q, ...answerButtons(request, "Allow edit"));
    }
    pane.append(ask);
    d.append(rail(session, request.project, request.agent, "needs"), pane);
    island.replaceChildren(bar, d);
    return;
  }
  setShape("open");
  const waiting = view.queue.length > 1 ? `· ${view.queue.length} waiting` : "";
  const body = el("div", "body");
  body.append(card(request, view.queue, done));
  island.replaceChildren(head("needs", "Needs you", waiting), body);
}

function render(view: View) {
  current = view;
  clearTimeout(timer);
  timerAt = 0;
  document.documentElement.classList.toggle("square", !view.rounded);
  const now = performance.now();

  if (toast && now >= toast.until) toast = null;
  if (toast) {
    renderApproval(view, toast.request, toast.ok);
    later(toast.until - now);
    return;
  }

  const first = view.queue[0];
  if (first) {
    if (first.id !== front.id) front = { id: first.id, since: now };
    renderApproval(view, first);
    return;
  }

  if (view.hidden && !peek) {
    setShape("hidden");
    island.replaceChildren();
    return;
  }

  const session = detail ? view.sessions.find((s) => s.id === detail) : undefined;
  if (!session) detail = null;

  if (view.open && session) {
    setShape("wide");
    const bar = el("div", "bar");
    bar.append(
      closeButton(() => {
        detail = null;
        if (current) render(current);
      }),
    );
    draggable(bar);
    const state: BallState =
      session.status === "needs you" ? "needs" : session.status === "working" ? "working" : "idle";
    const d = el("div", "detail");
    d.append(rail(session, session.project, session.agent, state), codePane(session.code, session.status === "working"));
    island.replaceChildren(bar, d);
    later(CLOCK_MS);
    return;
  }

  if (view.open) {
    setShape("open");
    const body = el("div", "body");
    body.append(sessionRows(view.sessions));
    const close = () => {
      detail = null;
      invoke("expand", { open: false });
    };
    island.replaceChildren(head(mood(view), "Bouncer", `· ${plural(view.sessions.length, "session")}`, close), body);
    later(CLOCK_MS);
    return;
  }

  setShape("pill");
  if (view.paused) {
    island.replaceChildren(pill("paused", "Paused", " · requests go to the terminal"));
    return;
  }
  const working = view.sessions
    .filter((s) => s.status === "working")
    .sort((a, b) => b.last_ms - a.last_ms);
  if (working.length) {
    // Several busy sessions: the label rotates every ROTATE_MS.
    const s = working[Math.floor(Date.now() / ROTATE_MS) % working.length];
    island.replaceChildren(pill("working", folder(s.project), ` · ${s.step}`));
    if (working.length > 1) later(ROTATE_MS - (Date.now() % ROTATE_MS));
    return;
  }
  const n = view.sessions.length;
  island.replaceChildren(pill("idle", n ? plural(n, "session") : "No sessions", " · all quiet", n > 0));
}

// The wake strip peeks the idle pill after PEEK_MS of hover.
let peekTimer = 0;
island.addEventListener("mouseenter", () => {
  if (!current?.hidden) return;
  peekTimer = setTimeout(() => {
    peek = true;
    if (current) render(current);
  }, PEEK_MS);
});
island.addEventListener("mouseleave", () => {
  clearTimeout(peekTimer);
  if (peek) {
    peek = false;
    if (current) render(current);
  }
});

// The window is sized to the island (plus room for its shadow). While the
// width animates it keeps the larger size, then settles.
let lastW = 0;
let lastH = 0;
function report() {
  const r = stage.getBoundingClientRect();
  let w = Math.ceil(r.width);
  let h = Math.ceil(r.height);
  if (performance.now() < growUntil) {
    w = Math.max(w, lastW);
    h = Math.max(h, lastH);
  }
  if (w !== lastW || h !== lastH) {
    lastW = w;
    lastH = h;
    invoke("fit", { width: w, height: h });
  }
}
new ResizeObserver(report).observe(stage);

const feed = new Channel<View>();
feed.onmessage = render;
invoke("subscribe", { feed });
