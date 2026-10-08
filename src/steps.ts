// The Session detail step list: repeated steps grouped ("Searching ×3"), and
// only the latest few shown, with a count of the earlier ones.

/** `ran`: the step's own PostToolUse came. */
export type Step = { label: string; how: string; ran: boolean };
export type Group = Step & { count: number };

/** Steps shown at most; older ones become "+N earlier". */
export const SHOWN = 6;

/** Merges runs of the same step that got through, and ended, the same way. */
export function group(history: Step[]): Group[] {
  const out: Group[] = [];
  for (const step of history) {
    const last = out[out.length - 1];
    if (last && last.label === step.label && last.how === step.how && last.ran === step.ran) last.count++;
    else out.push({ ...step, count: 1 });
  }
  return out;
}

/** The latest `max` groups, and how many groups came before them. */
export function latest(groups: Group[], max = SHOWN): { shown: Group[]; earlier: number } {
  const earlier = Math.max(0, groups.length - max);
  return { shown: groups.slice(earlier), earlier };
}

/** Icons that follow how a step ended, not whether it's the current one:
 * a refusal is a red cross (never a green check), a terminal answer is
 * neutral until Bouncer knows, and one that didn't run is a grey dash. */
const OUTCOME_ICON: Record<string, string> = {
  "you denied": "no",
  "answered in terminal": "mid",
  "not run (answered in terminal)": "skip",
  failed: "warn",
};

/** The icon for a step: its outcome's; waiting / running while it's the
 * current step; once done, a green check only if it ran (its PostToolUse
 * came), else the neutral ring (no event, e.g. hooks without
 * PostToolUseFailure, or stopped with Esc). */
export function stepIcon(how: string, ran: boolean, now: boolean, waiting: boolean): string {
  return OUTCOME_ICON[how] ?? (now ? (waiting ? "wait" : "spin") : ran ? "ok" : "mid");
}

/** How long a request left to Claude Code's own prompt shows as running. */
export const STALE_MS = 120_000;

/** Claude Code sends no event when a request is answered No in its terminal
 * (nor while it waits there), so after STALE_MS with nothing new the island
 * stops showing the step as running: "Waiting in terminal". */
export function waitingInTerminal(status: string, lastMs: number, now: number): boolean {
  return status === "asks in terminal" && now - lastMs >= STALE_MS;
}

/** "Searching ×3" for a repeated step. */
export function title(g: Group): string {
  return g.count > 1 ? `${g.label} ×${g.count}` : g.label;
}
