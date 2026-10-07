// Bouncer's sounds (the pack is in pack.ts): which one a view deserves, and
// the rules for playing it. Sound off, a sound switched off, or Bouncer
// paused (except the pause's own snore) means silence. One at a time: an
// alarm (risky, needs you) cuts a lighter sound still playing; anything
// else waits its turn, and the same sound again within REPEAT_MS is
// dropped. The audio context is suspended after each sound, so nothing
// runs while it's quiet.

import { length, loudness, render, type Cue, type Style } from "./pack.ts";

export type { Cue } from "./pack.ts";

type Status = { id: string; status: string; failed?: number };
type Seen = {
  paused: boolean;
  queue: { id: string; risk: string | null }[];
  sessions: Status[];
  flash?: { n: number; badge: string } | null;
  away?: unknown;
};

/** What the island had last time, to hear only what's new. */
export type Heard = {
  front: string;
  asking: Set<string>;
  /** Each session's failed tool calls. */
  failed: Map<string, number>;
  flash: number;
  paused: boolean;
  away: boolean;
};

const REPEAT_MS = 2000;
/** Which sound wins when two want to play: higher cuts lower. */
const RANK: Partial<Record<Cue, number>> = { risky: 3, needs: 2 };
const rank = (c: Cue) => RANK[c] ?? 1;

/** The sound this view deserves (or null), and what to remember. The first
 * view is the launch: only the greeting's "yo!". Then, most urgent first:
 * pause / resume, a new card (risky / needs), a session newly asking, a
 * failed tool call, a finished session, a new session, the away summary,
 * an auto-allow. Nothing else while paused. */
export function cue(heard: Heard | null, view: Seen, finished: boolean): { cue: Cue | null; heard: Heard } {
  const front = view.queue[0];
  const asking = new Set(view.sessions.filter((s) => s.status === "needs you").map((s) => s.id));
  const failed = new Map(view.sessions.map((s) => [s.id, s.failed ?? 0]));
  // Only the auto-allowed flash ("rule"), not "Rules changed".
  const flash = view.flash?.badge === "rule" ? view.flash.n : (heard?.flash ?? 0);
  const next: Heard = { front: front?.id ?? "", asking, failed, flash, paused: view.paused, away: !!view.away };
  const say = (c: Cue | null) => ({ cue: c, heard: next });
  if (!heard) return say(view.paused ? null : "launch");
  if (view.paused) return say(heard.paused ? null : "paused");
  if (heard.paused) return say("resumed");
  if (front && front.id !== heard.front) return say(front.risk ? "risky" : "needs");
  if (!front && [...asking].some((id) => !heard.asking.has(id))) return say("needs");
  if ([...failed].some(([id, n]) => n > (heard.failed.get(id) ?? 0))) return say("fail");
  if (finished) return say("done");
  if ([...failed.keys()].some((id) => !heard.failed.has(id))) return say("session");
  if (next.away && !heard.away) return say("back");
  if (flash > heard.flash) return say("auto");
  return say(null);
}

/** Whether `kind` may start at `now`, after `last` started at `at` and
 * lasts until `until`. */
export function allowed(kind: Cue, now: number, last: { kind: Cue | null; at: number; until: number }): boolean {
  if (last.kind === null) return true;
  if (now < last.until) return rank(kind) > rank(last.kind);
  return !(kind === last.kind && now - last.at < REPEAT_MS);
}

/** What the user chose, and whether Bouncer is paused. */
export type Tuning = { on: boolean; style: Style; volume: number; sounds: Partial<Record<Cue, boolean>>; paused: boolean };

/** Whether `kind` may sound at all with these settings. */
export function wanted(kind: Cue, t: Tuning): boolean {
  if (!t.on || !t.sounds[kind]) return false;
  return !t.paused || kind === "paused";
}

let tuning: Tuning = { on: false, style: "soft", volume: 50, sounds: {}, paused: false };
/** Called with every view, before any sound it causes. */
export function tune(t: Tuning) {
  tuning = t;
}

let ctx: AudioContext | null = null;
let bus: GainNode | null = null;
let last: { kind: Cue | null; at: number; until: number } = { kind: null, at: 0, until: 0 };
let quiet = 0;
const RATE = 44100;
/** The common level (RMS) every sound is brought to, before its loudness. */
const TARGET = 0.07;
const gains = new Map<string, Promise<number>>();

/** The gain that brings `kind` in `style` to its loudness: rendered offline
 * once, then remembered. Peaks capped so 100% volume never clips. */
function gainOf(kind: Cue, style: Style): Promise<number> {
  const key = `${kind}.${style}`;
  let g = gains.get(key);
  if (!g) {
    const c = new OfflineAudioContext(1, Math.ceil(RATE * (length(kind) + 0.1)), RATE);
    render(c, c.destination, kind, style, 0);
    g = c.startRendering().then((buf) => {
      const x = buf.getChannelData(0);
      let peak = 0, sum = 0;
      for (const v of x) {
        peak = Math.max(peak, Math.abs(v));
        sum += v * v;
      }
      const rms = Math.sqrt(sum / x.length);
      return rms > 0 ? Math.min((TARGET * 10 ** (loudness(kind, style) / 20)) / rms, 0.45 / peak) : 0;
    });
    gains.set(key, g);
  }
  return g;
}

export function play(kind: Cue) {
  const t = tuning;
  if (!wanted(kind, t)) return;
  const now = performance.now();
  if (!allowed(kind, now, last)) return;
  last = { kind, at: now, until: now + length(kind) * 1000 };
  const mine = last;
  gainOf(kind, t.style)
    .then((gain) => {
      if (last !== mine) return; // cut by an alarm while measuring
      ctx ??= new AudioContext();
      void ctx.resume();
      if (bus) {
        // One at a time: the old one fades out fast.
        const old = bus;
        old.gain.setTargetAtTime(0, ctx.currentTime, 0.008);
        setTimeout(() => old.disconnect(), 80);
      }
      bus = ctx.createGain();
      bus.gain.value = gain * (t.volume / 100) * 2;
      bus.connect(ctx.destination);
      render(ctx, bus, kind, t.style, ctx.currentTime + 0.01);
      // Nothing keeps running once it's quiet.
      clearTimeout(quiet);
      quiet = setTimeout(() => void ctx?.suspend(), length(kind) * 1000 + 250);
    })
    .catch(() => {
      // No audio device: Bouncer stays silent.
    });
}
