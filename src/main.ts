// The island, built to design/prototype/bouncer-island.html.
//
// Everything that came from an agent goes in as text (textContent / text
// nodes), never HTML; the backend has already turned hidden characters into
// `\u{XXXX}` markers, shown here as red tags. Allow arms after 600 ms, is never
// focused (the window can't take focus at all), and Enter can't reach it.
import { Channel, invoke } from "@tauri-apps/api/core";
import { ball, type BallState } from "./ball.ts";
import { flashLeft, type Seen } from "./flash.ts";
import { finished, greeting, mood } from "./mood.ts";
import { islandScale, openMaxHeight, scaleOf, WIDTH } from "./size.ts";
import { cue, play, type Heard } from "./sound.ts";
import { group, latest, title, waitingInTerminal, type Step } from "./steps.ts";
import { tokenize } from "./tokenize.ts";
import "./styles.css";

type Line = { n: number | null; mark: string; text: string };
type Code = {
  file: string;
  path: string;
  badge: string;
  syntax: string;
  changes: number;
  /** Only the changed text: its line numbers aren't the file's. */
  excerpt: boolean;
  lines: Line[];
};
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
  /** Why it's risky (one sentence), from the rules engine. */
  risk: string | null;
  /** Observe mode: the rules that would have allowed it. */
  would: string | null;
  /** The exact TOML "Always allow" appends (built by the backend). */
  offer: string | null;
};
/** "While you were away", from the activity log (the backend drops it when the island closes). */
type Away = {
  minutes: number;
  files: number;
  commands: number;
  auto: number;
  asked: number;
  stuck: { minutes: number; project: string; what: string } | null;
};
type Flash = { n: number; at_ms: number; strong: string; rest: string; badge: string; risk: boolean };
type Prefs = { size: string; sound: boolean };
/** Read-only facts for the settings screen (asked for when it opens). */
type About = { rules: string | null; hooks: "installed" | "missing" | "unknown" };
type View = {
  open: boolean;
  hidden: boolean;
  paused: boolean;
  rounded: boolean;
  sessions: Session[];
  queue: Request[];
  rules: { mode: string; error: string | null; notice: { n: number; text: string } | null };
  flash: Flash | null;
  away: Away | null;
  /** Why the activity log is off today, if it is. */
  history: { note: string | null };
  /** The settings screen is open. */
  settings: boolean;
  prefs: Prefs;
};

/** Allow stays disabled this long after a card reaches the front. */
const ARM_MS = 600;
const TOAST_MS = 1400;
/** "Rule added." stays a little longer (as the prototype). */
const RULE_TOAST_MS = 1600;
const ROTATE_MS = 4000;
const PEEK_MS = 300;
const CLOCK_MS = 30_000;
/** The launch greeting, and the cheer when a session finishes. */
const GREET_MS = 2500;
const DONE_MS = 2800;
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
  "auto-allowed by rule": "auto",
  "you allowed": "mine",
  "you denied": "flag",
  "waiting for you": "you",
  "asked in terminal": "you",
  "waiting in terminal": "you",
};
const HIDDEN = /\\u\{([0-9A-F]{4,6})\}/g;
const REDUCED = matchMedia("(prefers-reduced-motion: reduce)");

const stage = document.getElementById("stage")!;
const island = document.getElementById("island")!;

let current: View | null = null;
/** The request shown at the front, and when it got there (for the arm delay). */
let front = { id: "", since: 0 };
/** Shown for TOAST_MS after a decision, in place of the request. */
let toast: { request: Request; ok: boolean; until: number; text?: string } | null = null;
/** Session shown in Session detail. */
let detail: string | null = null;
/** The request whose "Always allow" preview is open, and since when (it arms like Allow). */
let always = { id: "", since: 0 };
/** The rules notice the user closed. */
let dismissed = 0;
/** The flash the pill last showed, and since when. */
let flashSeen: Seen = { n: 0, from: 0 };
/** The wake strip is being hovered: show the idle pill. */
let peek = false;
let dragged = false;
/** Greeting until (set on the first view); 0 once a card has shown. */
let greetUntil = -1;
/** Cheering until; each session's last status, to see one finish. */
let doneUntil = 0;
let statuses = new Map<string, string>();
/** What the sounds have already announced. */
let heard: Heard | null = null;

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
    case "PowerShell":
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

// ---- pieces ----------------------------------------------------------------

type Badge = { text: string; risk: boolean };

function pill(state: BallState, strong: string, rest: string, clickable = true, badge?: Badge): HTMLElement {
  const p = el("div", "pill");
  const label = el("span", "label");
  label.append(el("b", "", strong), rest);
  p.append(ball(state), label);
  if (badge) p.append(el("span", `badge${badge.risk ? " risk" : ""}`, badge.text));
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

function head(state: BallState, title: string, sub = "", onClose?: () => void, extra: HTMLElement[] = []): HTMLElement {
  const h = el("div", "head");
  const t = el("div", "title");
  t.append(ball(state), el("span", "", title));
  if (sub) t.append(el("span", "sub", sub));
  h.append(t);
  if (extra.length || onClose) {
    const buttons = el("div", "buttons");
    buttons.append(...extra);
    if (onClose) buttons.append(closeButton(onClose));
    h.append(buttons);
  }
  draggable(h);
  return h;
}

const SVG = "http://www.w3.org/2000/svg";

/** The settings gear (drawn in code, as the prototype's icon). */
function gearButton(): HTMLElement {
  const button = el("button", "iconbtn");
  button.setAttribute("aria-label", "Settings");
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  const circle = document.createElementNS(SVG, "circle");
  circle.setAttribute("cx", "12");
  circle.setAttribute("cy", "12");
  circle.setAttribute("r", "3");
  const path = document.createElementNS(SVG, "path");
  path.setAttribute("d", "M10.32 5.00 L10.37 2.64 L13.63 2.64 L13.68 5.00 L15.76 5.86 L17.46 4.23 L19.77 6.54 L18.14 8.24 L19.00 10.32 L21.36 10.37 L21.36 13.63 L19.00 13.68 L18.14 15.76 L19.77 17.46 L17.46 19.77 L15.76 18.14 L13.68 19.00 L13.63 21.36 L10.37 21.36 L10.32 19.00 L8.24 18.14 L6.54 19.77 L4.23 17.46 L5.86 15.76 L5.00 13.68 L2.64 13.63 L2.64 10.37 L5.00 10.32 L5.86 8.24 L4.23 6.54 L6.54 4.23 L8.24 5.86Z");
  svg.append(circle, path);
  button.append(svg);
  button.addEventListener("click", () => {
    invoke("settings", { open: true });
    invoke("keyboard", { on: true });
  });
  return button;
}

function stepText(s: Session): string {
  if (s.status === "needs you") return `Needs you · ${s.step}`;
  if (waitingInTerminal(s.status, s.last_ms, Date.now())) return `Waiting in terminal · ${s.step}`;
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
    li.tabIndex = 0;
    li.setAttribute("role", "button");
    li.dataset.key = `row:${s.id}`;
    li.addEventListener("click", () => {
      detail = s.id;
      if (current) render(current);
    });
    li.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        li.click();
      }
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

/** An Allow-style button that stays disabled until ARM_MS after `since`. */
function armed(label: string, since: number): HTMLButtonElement {
  const button = el("button", "btn allow");
  const fill = el("span", "arm");
  button.append(fill, el("span", "label", label));
  const waited = performance.now() - since;
  if (waited < ARM_MS) {
    button.disabled = true;
    // Keep the fill continuous across re-renders.
    fill.style.animationDelay = `${-waited}ms`;
    later(ARM_MS - waited);
  }
  return button;
}

/** Deny and Allow for a request; Allow arms ARM_MS after the card arrived. */
function answerButtons(request: Request, allowLabel: string): HTMLButtonElement[] {
  const deny = el("button", "btn deny", "Deny");
  deny.dataset.key = `deny:${request.id}`;
  const allow = armed(allowLabel, front.since);
  const buttons = [deny, allow];
  deny.addEventListener("click", () => decide(request, false, buttons));
  allow.addEventListener("click", () => decide(request, true, buttons));
  return buttons;
}

/** "Always allow…": the dashed full-width button under Deny / Allow once.
 * It only opens the preview; nothing is written until "Add rule and allow". */
function alwaysButton(request: Request): HTMLButtonElement {
  const button = el("button", "btn always", "Always allow…");
  button.addEventListener("click", () => {
    always = { id: request.id, since: performance.now() };
    if (current) render(current);
  });
  return button;
}

/** The exact rule that will be added (built by the backend), and Cancel /
 * "Add rule and allow", which arms ARM_MS after the preview opens. */
function alwaysPreview(request: Request, offer: string, mode: string): HTMLElement {
  const box = el("div", "rulebox");
  const label = el("div", "lbl");
  label.append("This rule will be added to ", el("b", "", "rules.toml"), ":");
  const rule = el("pre", "cmd");
  agentText(rule, offer.trimEnd());
  const file = /^tool = /m.test(offer);
  const note = el(
    "div",
    "note",
    `Only this exact ${file ? "file" : "command"} in this project. Remove it any time from the rules file.`,
  );
  const cancel = el("button", "btn deny", "Cancel");
  const add = armed("Add rule and allow", always.since);
  const buttons = [cancel, add];
  cancel.addEventListener("click", () => {
    always = { id: "", since: 0 };
    if (current) render(current);
  });
  add.addEventListener("click", () => {
    for (const b of buttons) b.disabled = true;
    invoke("always", { id: request.id })
      .then(() => {
        const what = file ? "This file" : "This command";
        const text =
          mode === "auto"
            ? `Rule added. ${what} is auto-allowed from now on.`
            : "Rule added. In observe mode it still asks.";
        toast = { request, ok: true, until: performance.now() + RULE_TOAST_MS, text };
      })
      .catch(() => {
        // Refused (too soon, already answered, or the file couldn't be saved).
      })
      .finally(() => {
        always = { id: "", since: 0 };
        if (current) render(current);
      });
  });
  const row = el("div", "actions");
  row.append(...buttons);
  box.append(label, rule, note, row);
  return box;
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
  const c = el("section", request.risk ? "card risky" : "card");
  if (done !== undefined) {
    const text = toast?.text ?? (done ? "Allowed. Back to work." : "Denied. Claude Code was told no.");
    c.append(el("div", `toast ${done ? "ok" : "no"}`, text));
    return c;
  }
  const top = el("div", "top");
  const who = el("span");
  who.append(el("b", "", AGENTS[request.agent] ?? request.agent), ` · ${folder(request.project)}`);
  top.append(who, el("span", "", queue.length > 1 ? `1 of ${queue.length}` : ""));
  c.append(top, el("div", "what", what(request.tool)));
  if (request.risk) c.append(el("div", "reason", request.risk));
  if (request.would) {
    const would = el("div", "would");
    would.append(el("span", "badge", "rule"), `Would auto-allow · ${request.would} (observe mode)`);
    c.append(would);
  }
  const cmd = el("pre", "cmd");
  agentText(cmd, cardText(request));
  c.append(cmd);
  // Never on a risky card (the backend offers nothing then either).
  const offer = request.risk ? null : request.offer;
  if (offer && always.id === request.id) {
    c.append(alwaysPreview(request, offer, current?.rules.mode ?? "observe"));
  } else {
    const actions = el("div", "actions");
    actions.append(...answerButtons(request, "Allow once"));
    if (offer) actions.append(alwaysButton(request));
    c.append(actions);
  }
  const next = queue[1];
  if (next) c.append(el("div", "after", `Next: ${what(next.tool)} in ${folder(next.project)}`));
  return c;
}

/** `path` relative to `project` when it's inside it, as the prototype shows. */
function relative(path: string, project: string): string {
  const root = project.replace(/[\\/]+$/, "");
  const inside = path.length > root.length && path.startsWith(root) && "\\/".includes(path[root.length]);
  return inside ? path.slice(root.length + 1) : path;
}

function codePane(code: Code | null, project: string, caret: boolean): HTMLElement {
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
  if (code.excerpt) {
    const excerpt = el("span", "excerpt", "Excerpt");
    excerpt.title = "Only the changed part; line numbers count within it, not the file.";
    bar.append(excerpt);
  }
  const path = el("span", "path", relative(code.path, project));
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
  const { shown, earlier } = latest(group(session?.history ?? []));
  if (earlier) steps.append(el("li", "earlier", `+${earlier} earlier`));
  const stale = session ? waitingInTerminal(session.status, session.last_ms, Date.now()) : false;
  shown.forEach((step, i) => {
    const now = i === shown.length - 1 && session?.status !== "idle";
    // Answered (or still waiting) in Claude Code's own prompt: not running.
    const waiting = now && (step.how === "waiting for you" || stale);
    const icon = !now ? "ok" : waiting ? "wait" : "spin";
    const how = now && stale ? "waiting in terminal" : step.how;
    const li = el("li", now ? "now" : "done");
    li.append(el("span", `ic ${icon}`), el("span", "", title(step)));
    if (how) li.append(el("span", `tag ${TAG[how] ?? ""}`, how));
    steps.append(li);
  });
  who.append(ball(state, "big"), proj, el("div", "agent", `${AGENTS[agent] ?? agent}${age}`), steps);
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
  // One scale for the whole island, kept under 40% of this screen's width.
  // One scale for the whole island (the Size preference), kept under 40% of
  // this screen's width; the open island never passes the work area's height.
  const scale = islandScale(WIDTH[next], screen.availWidth, scaleOf(current?.prefs?.size));
  const root = document.documentElement.style;
  root.setProperty("--scale", String(scale));
  root.setProperty("--open-max-h", `${openMaxHeight(screen.availHeight, scale)}px`);
  if (next !== "pill") island.classList.add(next);
  stage.className = next === "hidden" ? "bare" : "";
}

function renderApproval(view: View, request: Request, done?: boolean) {
  // Risky edits use the card, so the reason and the flipped buttons show.
  // (The wide edit view has no "Always allow", as in the prototype.)
  const edit = EDITS.has(request.tool ?? "") && !request.risk;
  const session = view.sessions.find((s) => s.id === request.session);
  if (edit && request.code) {
    setShape("wide");
    const bar = el("div", "bar");
    draggable(bar);
    const d = el("div", "detail");
    const pane = codePane(request.code, request.project, false);
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
  const top = request.risk && done === undefined
    ? head("risky", "Risky", "· check before allowing")
    : head("needs", "Needs you", waiting);
  island.replaceChildren(top, body);
}

/** The pill's "Auto-allowed" / "Rules changed" message, while it's fresh. */
function freshFlash(view: View): Flash | null {
  const f = view.flash;
  if (!f) return null;
  const shown = flashLeft(f.n, f.at_ms, Date.now(), flashSeen);
  flashSeen = shown.seen;
  if (shown.left <= 0) return null;
  later(shown.left);
  return f;
}

/** Rules file problems and changes, above the session list. */
function rulesNotes(view: View): HTMLElement[] {
  const notes: HTMLElement[] = [];
  if (view.rules.error) {
    notes.push(el("div", "reason", `${view.rules.error}. Using the built-in rules in observe mode until it's fixed.`));
  }
  const notice = view.rules.notice;
  if (notice && notice.n !== dismissed) {
    const row = el("div", "notice");
    row.append(
      el("span", "", notice.text),
      closeButton(() => {
        dismissed = notice.n;
        if (current) render(current);
      }),
    );
    notes.push(row);
  }
  if (view.history.note) notes.push(el("div", "after", view.history.note));
  return notes;
}

/** The prototype's "Away summary": numbers, where it got stuck, then the sessions. */
function renderAway(view: View, away: Away) {
  setShape("open");
  const stats = el("div", "stats");
  const counts: [number, string, string][] = [
    [away.files, "file changed", "files changed"],
    [away.commands, "command", "commands"],
    [away.auto, "auto-allowed", "auto-allowed"],
    [away.asked, "asked you", "asked you"],
  ];
  for (const [n, one, many] of counts) {
    const s = el("div", "stat");
    s.append(el("b", "", String(n)), el("span", "", n === 1 ? one : many));
    stats.append(s);
  }
  const body = el("div", "body");
  body.append(stats);
  if (away.stuck) {
    const stuck = el("div", "stuck");
    stuck.append(el("b", "", `Stuck ${away.stuck.minutes} min `), "in ");
    agentText(stuck, away.stuck.project);
    stuck.append(", waiting on ");
    agentText(stuck, away.stuck.what);
    body.append(stuck);
  }
  if (view.sessions.length) body.append(sessionRows(view.sessions));
  const close = () => invoke("expand", { open: false });
  island.replaceChildren(head("idle", "While you were away", `· ${away.minutes} min`, close), body);
  later(CLOCK_MS);
}

/** Settings facts, asked for once each time the screen opens. */
let about: About | null = null;
let aboutAsked = false;
/** The "Wipe…" confirm is open, and since when (its button arms like Allow). */
let wiping = false;
let wipeSince = 0;
/** The "Turn on auto-allow?" confirm is open, and since when (it arms too). */
let confirmingAuto = false;
let autoSince = 0;

function setRow(name: string, controls: (HTMLElement | string)[], hint?: string): HTMLElement {
  const row = el("div", "setrow");
  const ctl = el("div", "ctl");
  ctl.append(...controls);
  row.append(el("div", "name", name), ctl);
  if (hint) row.append(el("div", "hint", hint));
  return row;
}

function toggle(label: string, on: boolean, onClick: () => void): HTMLButtonElement {
  const t = el("button", "toggle");
  t.setAttribute("role", "switch");
  t.setAttribute("aria-checked", String(on));
  t.setAttribute("aria-label", label);
  t.addEventListener("click", onClick);
  return t;
}

function savePrefs(prefs: Prefs) {
  // The backend saves, then sends the new view.
  invoke("set_prefs", { size: prefs.size, sound: prefs.sound }).catch(() => {});
}

function closeSettings() {
  aboutAsked = false;
  wiping = false;
  confirmingAuto = false;
  invoke("settings", { open: false });
}

/** Observe / auto. Turning auto on asks first (focus on Cancel, Esc cancels,
 * Enter never turns it on, the button arms after ARM_MS); off doesn't ask. */
function autoRow(view: View): HTMLElement {
  const auto = view.rules.mode === "auto";
  const broken = view.rules.error !== null;
  const rerender = () => current && render(current);
  const sw = toggle("Auto-allow", auto, () => {
    if (auto) {
      invoke("set_mode", { mode: "observe" }).catch(() => {});
    } else {
      confirmingAuto = true;
      autoSince = performance.now();
      rerender();
    }
  });
  sw.disabled = broken;
  sw.dataset.key = "auto";
  const row = setRow(
    "Auto-allow",
    [auto ? "On · auto" : "Off · observe", sw],
    broken ? "Fix the rules file first." : auto ? "Rules answer for you; risky still asks." : "Observe: cards say what rules would do.",
  );
  if (confirmingAuto && !auto && !broken) {
    const box = el("div", "confirm");
    const text = el("div");
    text.append(
      el("b", "", "Turn on auto-allow? "),
      "Requests your rules allow are approved without asking. Risky requests always ask. You can turn it off here any time.",
    );
    const cancel = el("button", "btn deny small", "Cancel");
    const on = armed("Turn on auto-allow", autoSince);
    on.classList.add("small");
    const close = () => {
      confirmingAuto = false;
      refocus = "auto";
      rerender();
    };
    cancel.addEventListener("click", close);
    on.addEventListener("click", () => {
      on.disabled = true;
      invoke("set_mode", { mode: "auto" })
        .catch(() => {})
        .finally(close);
    });
    box.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && e.target === on) e.preventDefault();
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    });
    const actions = el("div", "actions");
    actions.append(cancel, on);
    box.append(text, actions);
    row.append(box);
    // Fully in view, focus on Cancel (never on "Turn on").
    requestAnimationFrame(() => {
      box.scrollIntoView({ block: "nearest" });
      if (document.activeElement === document.body || !box.contains(document.activeElement)) {
        cancel.focus({ preventScroll: true });
      }
    });
  }
  if (auto) confirmingAuto = false;
  return row;
}

/** The prototype's "Settings": Size, Sound, history, rules file, hooks. */
function renderSettings(view: View, state: BallState) {
  setShape("open");
  if (!aboutAsked) {
    aboutAsked = true;
    invoke<About>("about")
      .then((a) => {
        about = a;
        if (current) render(current);
      })
      .catch(() => {});
  }
  const prefs = view.prefs;
  const list = el("div", "setlist");

  const seg = el("div", "seg");
  seg.setAttribute("role", "radiogroup");
  seg.setAttribute("aria-label", "Size");
  for (const [size, label] of [["small", "Small"], ["medium", "Medium"], ["large", "Large"]]) {
    const b = el("button", "", label);
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(prefs.size === size));
    b.addEventListener("click", () => savePrefs({ ...prefs, size }));
    seg.append(b);
  }
  list.append(setRow("Size", [seg], "100 · 112 · 125%. Applies at once."));

  list.append(
    setRow(
      "Sound",
      [el("span", "", prefs.sound ? "On" : "Off"), toggle("Sound", prefs.sound, () => savePrefs({ ...prefs, sound: !prefs.sound }))],
      "Needs you, risky, done. Not while paused.",
    ),
  );

  list.append(autoRow(view));

  const wipe = el("button", "btn deny small", "Wipe…");
  wipe.dataset.key = "wipe";
  wipe.addEventListener("click", () => {
    wiping = true;
    wipeSince = performance.now();
    if (current) render(current);
  });
  const history = setRow("Activity history", wiping ? [] : [wipe], "30 days, on this computer, redacted.");
  if (wiping) {
    const box = el("div", "confirm");
    const text = el("div");
    text.append(el("b", "", "Delete all activity history? "), "This can't be undone.");
    const cancel = el("button", "btn deny small", "Cancel");
    const del = armed("Delete history", wipeSince);
    del.classList.add("small");
    cancel.addEventListener("click", () => {
      wiping = false;
      refocus = "wipe";
      if (current) render(current);
    });
    del.addEventListener("click", () => {
      del.disabled = true;
      invoke("wipe")
        .catch(() => {})
        .finally(() => {
          wiping = false;
          refocus = "wipe";
          if (current) render(current);
        });
    });
    const row = el("div", "actions");
    row.append(cancel, del);
    box.append(text, row);
    history.append(box);
  }
  list.append(history);

  const rules = setRow("Rules file", []);
  rules.append(el("pre", "cmd", about?.rules ?? "…"));
  list.append(rules);

  const hooks = about?.hooks;
  const status = el("span", `status${hooks === "installed" ? "" : " off"}`, hooks === "installed" ? "Installed" : hooks === "missing" ? "Not installed" : "Unknown");
  list.append(
    setRow(
      "Claude Code hooks",
      [status],
      hooks === "installed" ? "Remove: bouncer uninstall-hooks" : "Install: bouncer install-hooks",
    ),
  );

  const body = el("div", "body");
  body.append(list);
  island.replaceChildren(head(state, "Settings", "", closeSettings), body);
}

/** A red "rules" badge on the pill while the rules file is refused. */
function rulesBadge(view: View): Badge | undefined {
  return view.rules.error ? { text: "rules", risk: true } : undefined;
}

/** Keyboard focus survives the island being rebuilt: it goes back to the
 * same control. A new card at the front takes it to its Deny (never Allow). */
function render(view: View) {
  const had = focusKey(document.activeElement);
  const before = front.id;
  draw(view);
  if (!document.hasFocus()) return;
  const card = view.queue[0];
  const target = card && card.id !== before ? `deny:${card.id}` : had;
  const next = (target ? findKey(target) : undefined) ?? (refocus ? findKey(refocus) : undefined);
  refocus = null;
  next?.focus({ preventScroll: true });
}

/** Where focus goes when the focused control disappears (a confirm closed). */
let refocus: string | null = null;

/** What identifies a control across re-renders. */
function focusKey(node: Element | null): string | null {
  if (!(node instanceof HTMLElement) || !island.contains(node)) return null;
  return node.dataset.key ?? `${node.tagName}|${node.className}|${node.getAttribute("aria-label") ?? node.textContent}`;
}

function findKey(key: string): HTMLElement | undefined {
  return [...island.querySelectorAll<HTMLElement>("button, [tabindex]")].find((n) => focusKey(n) === key);
}

const announcer = document.getElementById("announce")!;

/** A short sentence for screen readers when something happens. */
function announce(kind: string, view: View) {
  const card = view.queue[0];
  const asking = view.sessions.find((s) => s.status === "needs you");
  const finishedNow = view.sessions.find((s) => s.status === "idle");
  let text = "";
  if (kind === "risky" && card) text = `Risky: ${what(card.tool)} in ${folder(card.project)}. ${card.risk ?? ""}`;
  else if (kind === "needs" && card) text = `Needs you: ${what(card.tool)} in ${folder(card.project)}.`;
  else if (kind === "needs" && asking) text = `Needs you: ${folder(asking.project)} is asking in the terminal.`;
  else if (kind === "done" && finishedNow) text = `${folder(finishedNow.project)} finished.`;
  announcer.textContent = text;
}

function draw(view: View) {
  current = view;
  clearTimeout(timer);
  timerAt = 0;
  document.documentElement.classList.toggle("square", !view.rounded);
  const now = performance.now();

  if (greetUntil < 0) greetUntil = now + GREET_MS;
  const ended = finished(statuses, view.sessions);
  if (ended) doneUntil = now + DONE_MS;
  statuses = new Map(view.sessions.map((s) => [s.id, s.status]));
  const sound = cue(heard, view, ended);
  heard = sound.heard;
  if (sound.cue && view.prefs.sound) play(sound.cue);
  if (sound.cue) announce(sound.cue, view);
  const done = now < doneUntil;
  if (done) later(doneUntil - now);

  if (toast && now >= toast.until) toast = null;
  if (toast) {
    // The next card is hidden behind this message: it arms (and takes the
    // keyboard's focus) only once it's actually shown.
    front = { id: "", since: 0 };
    renderApproval(view, toast.request, toast.ok);
    later(toast.until - now);
    return;
  }

  const first = view.queue[0];
  if (first) {
    if (first.id !== front.id) front = { id: first.id, since: now };
    greetUntil = 0;
    renderApproval(view, first);
    return;
  }

  if (view.settings) {
    renderSettings(view, mood(view, done));
    return;
  }
  aboutAsked = false;
  wiping = false;
  confirmingAuto = false;

  if (greeting(view, now, greetUntil, REDUCED.matches)) {
    // Just Bouncer dropping in: no words (Charan).
    setShape("pill");
    const p = el("div", "pill greet");
    p.append(ball("greet"));
    island.replaceChildren(p);
    later(greetUntil - now);
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
    d.append(rail(session, session.project, session.agent, state), codePane(session.code, session.project, session.status === "working"));
    island.replaceChildren(bar, d);
    later(CLOCK_MS);
    return;
  }

  if (view.open && view.away) {
    renderAway(view, view.away);
    return;
  }

  if (view.open) {
    setShape("open");
    const body = el("div", "body");
    body.append(...rulesNotes(view), sessionRows(view.sessions));
    const close = () => {
      detail = null;
      invoke("expand", { open: false });
    };
    island.replaceChildren(head(mood(view, done), "Bouncer", `· ${plural(view.sessions.length, "session")}`, close, [gearButton()]), body);
    later(CLOCK_MS);
    return;
  }

  setShape("pill");
  if (view.paused) {
    island.replaceChildren(pill("paused", "Paused", " · requests go to the terminal"));
    return;
  }
  const badge = rulesBadge(view);
  const flash = freshFlash(view);
  if (flash) {
    const state: BallState = flash.risk ? "risky" : "working";
    island.replaceChildren(pill(state, flash.strong, flash.rest, true, { text: flash.badge, risk: flash.risk }));
    return;
  }
  // A session waiting on the user with no card here: a question in the terminal.
  const asking = view.sessions.find((s) => s.status === "needs you");
  if (asking) {
    island.replaceChildren(pill("needs", folder(asking.project), ` · ${stepText(asking)}`, true, badge));
    return;
  }
  const working = view.sessions
    .filter((s) => s.status === "working")
    .sort((a, b) => b.last_ms - a.last_ms);
  if (working.length) {
    // Several busy sessions: the label rotates every ROTATE_MS.
    const s = working[Math.floor(Date.now() / ROTATE_MS) % working.length];
    island.replaceChildren(pill(mood(view, done), folder(s.project), ` · ${s.step}`, true, badge));
    if (working.length > 1) later(ROTATE_MS - (Date.now() % ROTATE_MS));
    return;
  }
  const n = view.sessions.length;
  island.replaceChildren(pill(mood(view, done), n ? plural(n, "session") : "No sessions", " · all quiet", n > 0, badge));
}

// Enter never approves: not Allow, Allow edit, "Add rule and allow", "Turn
// on auto-allow" or "Delete history" (all armed buttons). Space works once
// armed; a disabled button can't be pressed at all.
document.addEventListener(
  "keydown",
  (e) => {
    if (e.key === "Enter" && e.target instanceof Element && e.target.closest(".btn.allow")) {
      e.preventDefault();
      e.stopPropagation();
    }
  },
  true,
);

// Tab stays inside the island (it wraps around), so the page never loses
// focus to the window frame.
document.addEventListener("keydown", (e) => {
  if (e.key !== "Tab") return;
  const all = [...island.querySelectorAll<HTMLElement>("button:not(:disabled), [tabindex='0']")];
  if (!all.length) return;
  const at = all.indexOf(document.activeElement as HTMLElement);
  const next = e.shiftKey ? (at <= 0 ? all.length - 1 : at - 1) : at === -1 || at === all.length - 1 ? 0 : at + 1;
  e.preventDefault();
  all[next].focus();
});

// Esc gives the keyboard back: settings and the session list close; a card
// stays (it can only be answered), but the keyboard goes back anyway.
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || e.defaultPrevented) return;
  if (current?.settings) closeSettings();
  if (current?.open && !current.queue.length) {
    detail = null;
    invoke("expand", { open: false });
  }
  invoke("keyboard", { on: false });
});

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
