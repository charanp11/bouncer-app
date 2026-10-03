// The Session detail step list: repeated steps grouped ("Searching ×3"), and
// only the latest few shown, with a count of the earlier ones.

export type Step = { label: string; how: string };
export type Group = Step & { count: number };

/** Steps shown at most; older ones become "+N earlier". */
export const SHOWN = 6;

/** Merges runs of the same step that got through the same way. */
export function group(history: Step[]): Group[] {
  const out: Group[] = [];
  for (const step of history) {
    const last = out[out.length - 1];
    if (last && last.label === step.label && last.how === step.how) last.count++;
    else out.push({ ...step, count: 1 });
  }
  return out;
}

/** The latest `max` groups, and how many groups came before them. */
export function latest(groups: Group[], max = SHOWN): { shown: Group[]; earlier: number } {
  const earlier = Math.max(0, groups.length - max);
  return { shown: groups.slice(earlier), earlier };
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
