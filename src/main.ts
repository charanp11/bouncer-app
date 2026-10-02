// The island. Everything that came from an agent is set with textContent only;
// the backend has already made hidden characters visible.
import { Channel, invoke } from "@tauri-apps/api/core";
import "./styles.css";

type Session = {
  id: string;
  agent: string;
  project: string;
  tool: string | null;
  status: string;
};
type Request = {
  id: string;
  agent: string;
  session: string;
  project: string;
  tool: string | null;
  text: string;
};
type View = {
  open: boolean;
  paused: boolean;
  sessions: Session[];
  queue: Request[];
};

/** Allow stays disabled this long after a card reaches the front. */
const ARM_MS = 600;
const AGENTS: Record<string, string> = { "claude-code": "Claude Code" };

const root = document.getElementById("island")!;
let front = { id: "", since: 0 };
let current: View | null = null;

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

function summary(view: View): string {
  if (view.paused) return "Paused";
  const n = view.sessions.length;
  const waiting = view.sessions.filter((s) => s.status === "needs you").length;
  const sessions = `${n} session${n === 1 ? "" : "s"}`;
  return waiting ? `${sessions} · ${waiting} need you` : sessions;
}

let dragged = false;

/** A press that moves a few pixels drags the island (the backend moves the
 * window); a press that doesn't is a normal click. */
function draggable(node: HTMLElement) {
  node.addEventListener("mousedown", (down) => {
    if (down.button !== 0) return;
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

function pill(view: View): HTMLElement {
  const button = el("button", "pill");
  button.append(el("span", "dot"), el("span", "", `Bouncer · ${summary(view)}`));
  button.addEventListener("click", () => {
    if (!dragged) invoke("expand", { open: true });
  });
  draggable(button);
  return button;
}

function card(request: Request, count: number): HTMLElement {
  const box = el("section", "card");
  const head = el("header");
  head.append(
    el("span", "agent", AGENTS[request.agent] ?? request.agent),
    el("span", "count", `1 of ${count}`),
  );
  const where = el("p", "where", folder(request.project));
  where.title = request.project;
  const tool = el("p", "tool", request.tool ?? "(no tool)");
  const command = el("pre", "command", request.text);

  const deny = el("button", "deny", "Deny");
  const allow = el("button", "allow", "Allow once");
  const armed = performance.now() - front.since >= ARM_MS;
  allow.disabled = !armed;
  const answer = (yes: boolean) => {
    deny.disabled = allow.disabled = true;
    invoke("decide", { id: request.id, allow: yes }).catch(() => {
      // Refused (too soon, or already answered): show the current state again.
      if (current) render(current);
    });
  };
  deny.addEventListener("click", () => answer(false));
  allow.addEventListener("click", () => answer(true));
  const actions = el("div", "actions");
  actions.append(deny, allow);

  box.append(head, where, tool, command, actions);
  return box;
}

function sessionList(view: View): HTMLElement {
  const list = el("ul", "sessions");
  for (const s of view.sessions) {
    const item = el("li", `session ${s.status.replaceAll(" ", "-")}`);
    item.title = `${s.project}\n${s.id}`;
    item.append(
      el("span", "dot"),
      el("span", "name", folder(s.project)),
      el("span", "id", s.id.slice(0, 8)),
      el("span", "state", s.tool ? `${s.status} · ${s.tool}` : s.status),
    );
    list.append(item);
  }
  return list;
}

function render(view: View) {
  current = view;
  const first = view.queue[0];
  if (first && first.id !== front.id) {
    front = { id: first.id, since: performance.now() };
    setTimeout(() => current && render(current), ARM_MS);
  }
  root.className = view.open ? "open" : "peek";
  if (!view.open) {
    root.replaceChildren(pill(view));
    return;
  }
  const top = el("header", "top");
  draggable(top);
  top.append(el("span", "", `Bouncer · ${summary(view)}`));
  if (!first) {
    const close = el("button", "close", "Close");
    close.addEventListener("click", () => invoke("expand", { open: false }));
    top.append(close);
  }
  root.replaceChildren(top);
  if (first) root.append(card(first, view.queue.length));
  root.append(sessionList(view));
}

// The window is sized to the island's content.
new ResizeObserver(() => {
  const { width, height } = root.getBoundingClientRect();
  invoke("fit", { width: Math.ceil(width), height: Math.ceil(height) });
}).observe(root);

const feed = new Channel<View>();
feed.onmessage = render;
invoke("subscribe", { feed });
