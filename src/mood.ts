// Which mood Bouncer shows. A request waiting for the user always wins:
// nothing playful (the greeting, a cheer) runs while a card is up.
import type { Mood } from "./motion.ts";

type Status = { id: string; status: string };
type Seen = { paused: boolean; open: boolean; queue: { risk: string | null }[]; sessions: Status[] };

export function mood(view: Seen, done: boolean): Mood {
  if (view.paused) return "paused";
  if (view.queue[0]?.risk) return "risky";
  if (view.queue.length || view.sessions.some((s) => s.status === "needs you")) return "needs";
  if (done) return "done";
  if (view.sessions.some((s) => s.status === "working")) return "working";
  return "idle";
}

/** Whether a session went from working to idle (its turn ended) since `before`. */
export function finished(before: Map<string, string>, sessions: Status[]): boolean {
  return sessions.some((s) => s.status === "idle" && before.get(s.id) === "working");
}

/** The launch greeting shows only on a quiet island: never over a card, the
 * paused pill or an open island. */
export function greeting(view: Seen, now: number, until: number): boolean {
  return now < until && !view.queue.length && !view.paused && !view.open;
}
