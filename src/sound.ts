// Bouncer's three sounds, generated with Web Audio (no files): needs you (a
// card, or a question in the terminal), risky (sharper) and done (soft).
// Nothing for auto-allowed or working, nothing while paused or muted. One
// at a time, and the same sound again within REPEAT_MS is dropped. The audio
// context is suspended after each sound, so nothing runs while it's quiet.

export type Cue = "needs" | "risky" | "done";

type Status = { id: string; status: string };
type Seen = { paused: boolean; queue: { id: string; risk: string | null }[]; sessions: Status[] };

/** What the island had last time, to hear only what's new. */
export type Heard = { front: string; asking: Set<string> };

const REPEAT_MS = 2000;
/** Longest sound; another one isn't started before it ends. */
const LONGEST_MS = 450;
const VOLUME = 0.12;

/** The sound this view deserves (or null), and what to remember. A new card
 * at the front: needs / risky. A session newly asking with no card: needs.
 * A session that just finished: done. Risky beats needs beats done. */
export function cue(heard: Heard | null, view: Seen, finished: boolean): { cue: Cue | null; heard: Heard } {
  const front = view.queue[0];
  const asking = new Set(view.sessions.filter((s) => s.status === "needs you").map((s) => s.id));
  const next = { front: front?.id ?? "", asking };
  // The first view only sets what's known (a reload stays quiet).
  if (!heard || view.paused) return { cue: null, heard: next };
  if (front && front.id !== heard.front) return { cue: front.risk ? "risky" : "needs", heard: next };
  if (!front && [...asking].some((id) => !heard.asking.has(id))) return { cue: "needs", heard: next };
  return { cue: finished ? "done" : null, heard: next };
}

/** Whether `kind` may start at `now`, after `last` started at `at`. */
export function allowed(kind: Cue, now: number, last: { kind: Cue | null; at: number }): boolean {
  if (last.kind === null) return true;
  if (now - last.at < LONGEST_MS) return false;
  return !(kind === last.kind && now - last.at < REPEAT_MS);
}

let ctx: AudioContext | null = null;
let last: { kind: Cue | null; at: number } = { kind: null, at: 0 };
let quiet = 0;

/** Notes for each sound: [frequency Hz, start s, length s, wave]. */
const NOTES: Record<Cue, [number, number, number, OscillatorType][]> = {
  // Two soft rising notes: "hey, over here".
  needs: [
    [659.25, 0, 0.16, "sine"],
    [880, 0.13, 0.22, "sine"],
  ],
  // Two quick, sharper falling notes.
  risky: [
    [880, 0, 0.11, "triangle"],
    [622.25, 0.1, 0.24, "triangle"],
  ],
  // One soft bell.
  done: [[1046.5, 0, 0.4, "sine"]],
};

export function play(kind: Cue) {
  const now = performance.now();
  if (!allowed(kind, now, last)) return;
  last = { kind, at: now };
  try {
    ctx ??= new AudioContext();
    void ctx.resume();
    const t0 = ctx.currentTime + 0.01;
    for (const [freq, start, length, wave] of NOTES[kind]) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = wave;
      osc.frequency.value = freq;
      // Quick attack, smooth fall: no clicks.
      gain.gain.setValueAtTime(0, t0 + start);
      gain.gain.linearRampToValueAtTime(VOLUME, t0 + start + 0.012);
      gain.gain.exponentialRampToValueAtTime(0.0001, t0 + start + length);
      osc.connect(gain).connect(ctx.destination);
      osc.start(t0 + start);
      osc.stop(t0 + start + length + 0.02);
    }
    // Nothing keeps running once it's quiet.
    clearTimeout(quiet);
    quiet = setTimeout(() => void ctx?.suspend(), LONGEST_MS + 100);
  } catch {
    // No audio device: Bouncer stays silent.
  }
}
