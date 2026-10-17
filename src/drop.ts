// A dropped file (roadmap d): what the backend shows about it and the state
// of Claude's answer, kept apart from the DOM so they can be tested. The
// page never sees a path: only the name, Bouncer's own report and the
// drop's random ID (crates/app/src/drop.rs). main.ts draws it with
// textContent.
import { explainApply, explainRefused, type Explain, type ExplainEvent, type Stop } from "./chat.ts";

export type Finding = { risky: boolean; text: string };

export type DropReport = {
  state: "report";
  /** The drop's number: a new drop, or the same one drawn again. */
  n: number;
  /** The drop's random ID: the only thing the page sends back. */
  id: string;
  name: string;
  /** What it really is, from its bytes. */
  kind: string;
  size: number;
  /** Only parts of it were read. */
  partial: boolean;
  risky: boolean;
  findings: Finding[];
  sha256: string | null;
};

export type DropView =
  | null
  | { state: "checking"; name: string; n: number }
  | { state: "refused"; name?: string; text: string; n: number }
  | DropReport;

/** Something is being dragged over the island: one item or several, and
 * where it came in (the page's physical pixels). */
export type DropHover = null | { kind: "one" | "many"; x: number; y: number };

/** A file answer's event: a chat event tagged with the drop's ID. */
export type DropEvent = { drop: string } & (
  | { kind: "ready"; model: string; version: string }
  | { kind: "text"; text: string }
  | { kind: "tool"; name: string; target: string }
  | { kind: "denied"; tool: string; reason: string }
  | { kind: "usage"; fiveHour: number | null; sevenDay: number | null }
  | { kind: "done"; error: string | null }
  | { kind: "stopped"; stop: Stop }
);

/** Claude's answer to "Is this safe to run?": the same shape as an explanation. */
export type Answer = Explain;

/** An event for the answer on screen; anything else is ignored. */
export function answerApply(x: Answer | null, e: DropEvent): Answer | null {
  const { drop, ...rest } = e;
  return explainApply(x, { ...rest, explain: drop } as ExplainEvent);
}

/** Starting it was refused (`code` from drop_ask). */
export function answerRefused(x: Answer | null, id: string, code: string): Answer | null {
  return explainRefused(x, id, code === "gone" ? "file-gone" : code);
}

/** Every report and answer ends with this: Bouncer never says "safe". */
export const OWN_CHECKS = "Bouncer's own checks · can be wrong · you decide";
export const CAN_BE_WRONG = "Can be wrong · you decide";

/** The report's one-line summary: never "safe", never green. */
export function verdict(r: DropReport): string {
  return r.risky ? "Looks risky. Read why below." : "Nothing risky found.";
}

export function hoverText(kind: "one" | "many"): string {
  return kind === "one" ? "Drop to check it" : "One file at a time";
}

/** Bouncer's reactions (prototype v0.31): the gulp as a report appears, the
 * "nope" for a refusal. Each well under 600 ms, and only ever played as or
 * after the screen it belongs to is drawn: never holding anything back. */
export const GULP_MS = 450;
export const NOPE_MS = 360;

/** A drop's one reaction: when it started (`at`), for drop number `n`. */
export type Shot = { n: number; at: number };

/** Where drop `n`'s reaction is at `now`: started the first time it's drawn,
 * `into` ms in while it lasts (a redraw carries on), null once it's over or
 * was cut. */
export function shot(last: Shot | null, n: number, now: number, ms: number): { last: Shot; into: number | null } {
  const s = last && last.n === n ? last : { n, at: now };
  const into = now - s.at;
  return { last: s, into: into >= 0 && into < ms ? into : null };
}

/** A card came: the reaction under way is cut for good (not resumed after). */
export function cut(last: Shot | null): Shot | null {
  return last && { n: last.n, at: -Infinity };
}

/** Bouncer's face over his own report: never happy. */
export function face(r: DropReport): "wary" | "curious" {
  return r.risky ? "wary" : "curious";
}

/** 512 → "512 bytes", 1536 → "1.5 KB" (as the backend's `human`). */
export function sizeText(n: number): string {
  if (n < 1024) return `${n} bytes`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let unit = 0;
  while (v >= 1024 && unit < units.length - 1) {
    v /= 1024;
    unit++;
  }
  return `${v < 10 ? v.toFixed(1) : v.toFixed(0)} ${units[unit]}`;
}

/** A file attached to the next chat message ("Ask about it"). */
export type Attached = { id: string; name: string; size: number; preview: string | null };

/** The attachment, if the shown drop is still the one it came from (a new
 * drop, Close or the 10-minute hold drops it). */
export function stillAttached(a: Attached | null, view: DropView): Attached | null {
  return a && view?.state === "report" && view.id === a.id ? a : null;
}
