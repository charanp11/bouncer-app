// Bouncer's sound pack, picked by Charan on the prototype's listen-and-pick
// board (v0.14, 2026-10-06): every sound made here with Web Audio, no files.
// A sound is a list of parts written once; the style only changes the
// timbre (voice()), never timing or pitch contour.

export type Style = "soft" | "playful";

/** T: a tone, N: filtered noise. Times in seconds. f: pitch points (Hz)
 * spread evenly over the part; fl: [filter, from Hz, to Hz, Q]; vib: [rate
 * Hz, depth cents], fade: the vibrato dies away; hold: sustains, then
 * releases (else it rings out); alarm: grabs attention in every style. */
type Part = {
  k: "T" | "N";
  t: number;
  d: number;
  v: number;
  f?: number[];
  w?: OscillatorType;
  fl?: [BiquadFilterType, number, number, number] | null;
  vib?: [number, number];
  fade?: boolean;
  hold?: boolean;
  alarm?: boolean;
  a?: number;
};

const T = (t: number, d: number, f: number | number[], o: Partial<Part> = {}): Part => ({ k: "T", t, d, f: [f].flat(), w: "triangle", v: 1, ...o });
const N = (t: number, d: number, o: Partial<Part> = {}): Part => ({ k: "N", t, d, v: 1, ...o });
const knock = (t: number) => [
  T(t, 0.07, [190, 90], { w: "sine", v: 1.2, alarm: true }),
  N(t, 0.03, { fl: ["bandpass", 900, 900, 1.5], v: 0.5, alarm: true }),
];
const bell = (t: number, d: number, f: number, v: number) => [T(t, d, f, { w: "sine", v }), T(t, d * 0.35, f * 2.76, { w: "sine", v: v * 0.16 })];
const saw = (fl: [number, number, number]): Partial<Part> => ({ w: "sawtooth", fl: ["lowpass", ...fl] });

/** Every sound, by the name `preferences.json` stores. */
export const PACK = {
  launch: { label: "Launch", parts: [
    T(0, 0.15, [196, 262], { w: "sawtooth", fl: ["bandpass", 650, 950, 3], v: 1.6, hold: true }),
    T(0.21, 0.34, [1400, 2500], { w: "sine", vib: [7, 40], v: 0.7 }),
    N(0.21, 0.1, { fl: ["highpass", 5000, 5000, 0.7], v: 0.15 }),
  ] },
  session: { label: "New session", parts: [...bell(0, 0.55, 659.25, 0.9), ...bell(0.3, 0.7, 523.25, 0.9)] },
  needs: { label: "Needs you", alarm: true, parts: [
    ...knock(0),
    ...knock(0.14),
    T(0.34, 0.09, [200, 185], { ...saw([1400, 1400, 1]), v: 0.8, hold: true, alarm: true }),
    T(0.47, 0.3, [175, 330], { ...saw([1400, 2400, 1]), v: 0.8, hold: true, vib: [6, 25], alarm: true }),
  ] },
  risky: { label: "Risky request", alarm: true, parts: [
    N(0, 0.09, { fl: ["bandpass", 3200, 700, 4], v: 1.4, alarm: true }),
    N(0.09, 0.08, { fl: ["bandpass", 700, 2600, 4], v: 1.4, alarm: true }),
    T(0, 0.17, [520, 180, 420], { ...saw([2500, 2500, 1]), v: 0.35, alarm: true }),
    T(0.22, 0.42, 98, { ...saw([1600, 1600, 1]), v: 0.9, hold: true, alarm: true }),
    T(0.22, 0.42, 104, { w: "square", fl: ["lowpass", 1600, 1600, 1], v: 0.6, hold: true, alarm: true }),
  ] },
  allowed: { label: "Allowed", parts: [
    T(0, 0.05, [260, 900], { w: "sine", v: 1 }),
    N(0, 0.02, { fl: ["highpass", 2500, 2500, 0.7], v: 0.35 }),
    ...bell(0.07, 0.45, 1567.98, 0.6),
  ] },
  denied: { label: "Denied", parts: [
    N(0, 0.14, { fl: ["lowpass", 900, 250, 1], v: 1.4 }),
    T(0, 0.22, [130, 45], { w: "sine", v: 1.3 }),
    T(0.06, 0.2, [360, 170], { w: "triangle", v: 0.8 }),
  ] },
  vip: { label: "“Always allow” rule added", parts: [
    N(0, 0.03, { fl: ["highpass", 2500, 2500, 0.7], v: 0.6 }),
    T(0, 0.05, [1000, 520], { w: "square", fl: ["lowpass", 3000, 3000, 1], v: 0.35 }),
    N(0.13, 0.08, { fl: ["lowpass", 1200, 400, 1], v: 1.2 }),
    T(0.13, 0.18, [170, 60], { w: "sine", v: 1.3 }),
  ] },
  auto: { label: "Auto-allowed", quiet: true, parts: [
    T(0, 0.025, [2200, 1800], { w: "sine", v: 0.4 }),
    N(0, 0.012, { fl: ["highpass", 4000, 4000, 0.7], v: 0.2 }),
  ] },
  done: { label: "Session done", parts: [
    T(0, 0.09, 392, { ...saw([2200, 2200, 1]), v: 0.5, hold: true }),
    ...[523.25, 659.25, 783.99].map((f) => T(0.13, 0.5, f, { ...saw([2600, 1200, 1]), v: 0.35, hold: true, vib: [5.5, 12] })),
  ] },
  fail: { label: "A tool failed", parts: [
    ...[[0, 293.66], [0.2, 277.18], [0.4, 261.63]].map(([t, f]) => T(t, 0.18, f, { ...saw([500, 1300, 1.5]), v: 0.6, hold: true })),
    T(0.6, 0.4, [246.94, 233.08], { ...saw([500, 1300, 2]), v: 0.6, hold: true, vib: [5, 35] }),
  ] },
  back: { label: "Welcome back", parts: [
    N(0, 0.07, { fl: ["bandpass", 1800, 1800, 1], v: 0.35 }),
    T(0.05, 0.5, [185, 330], { w: "sawtooth", fl: ["bandpass", 900, 1500, 1.2], v: 1.4, hold: true, vib: [6, 30] }),
  ] },
  poke: { label: "Poke (click on Bouncer)", parts: [T(0, 0.4, [160, 420], { w: "triangle", v: 1, vib: [16, 350], fade: true })] },
  dizzy: { label: "Dizzy", parts: [T(0, 0.75, [500, 1050, 520, 1100, 600], { w: "sine", v: 0.7, vib: [11, 60], hold: true })] },
  paused: { label: "Paused", parts: [
    T(0, 0.42, [62, 78], { ...saw([380, 520, 2]), v: 1.6, hold: true, vib: [24, 40] }),
    N(0, 0.42, { fl: ["lowpass", 700, 700, 1], v: 0.3, hold: true }),
    T(0.5, 0.4, [1100, 700], { w: "sine", v: 0.35, hold: true }),
  ] },
  resumed: { label: "Resumed", parts: [
    T(0, 0.06, 1318.51, { w: "square", fl: ["lowpass", 4000, 4000, 1], v: 0.4 }),
    T(0.1, 0.08, 1760, { w: "square", fl: ["lowpass", 4000, 4000, 1], v: 0.4 }),
  ] },
  autoon: { label: "Auto-allow turned on", parts: [
    N(0, 0.07, { fl: ["highpass", 3500, 3500, 0.7], v: 0.6 }),
    N(0.05, 0.05, { fl: ["bandpass", 2500, 2500, 2], v: 0.5 }),
    ...[2093, 2637.02, 3135.96].map((f) => T(0.13, 0.55, f, { w: "sine", v: 0.3 })),
    N(0.13, 0.2, { fl: ["highpass", 7000, 7000, 0.7], v: 0.2 }),
  ] },
  // Option C, the broom "swish".
  wiped: { label: "History wiped", parts: [
    N(0, 0.2, { fl: ["bandpass", 900, 3000, 1.6], v: 1, a: 0.08 }),
    N(0.23, 0.25, { fl: ["bandpass", 1300, 3600, 1.6], v: 1.1, a: 0.09 }),
  ] },
} satisfies Record<string, { label: string; parts: Part[]; alarm?: boolean; quiet?: boolean }>;

export type Cue = keyof typeof PACK;
export const CUES = Object.keys(PACK) as Cue[];

/** How long a sound lasts, in seconds (the end of its last part). */
export function length(cue: Cue): number {
  return Math.max(...PACK[cue].parts.map((p) => p.t + p.d));
}

/** The part as `style` plays it: same times and pitches, other timbre.
 * Soft rounds everything off; alarm parts keep their buzz (same wave),
 * losing only the harsh top, so they still grab attention. */
export function voice(p: Part, style: Style): Part {
  const q = { ...p, a: p.a ?? 0.004 };
  if (style !== "soft") return q;
  if (p.alarm) {
    q.a = Math.max(q.a, 0.008);
    if (p.k === "T") {
      const top = Math.min(p.fl?.[2] ?? 2200, 2200);
      q.w = p.w === "sine" ? "triangle" : p.w;
      q.fl = ["lowpass", top, top, 0.7];
    } else if (p.fl) q.fl = [p.fl[0], p.fl[1] * 0.85, p.fl[2] * 0.85, p.fl[3]];
    return q;
  }
  q.a = Math.max(q.a, 0.02);
  q.v = p.v * (p.k === "N" ? 0.5 : 0.85);
  if (p.k === "N" && p.fl) q.fl = [p.fl[0], p.fl[1] * 0.6, p.fl[2] * 0.6, Math.min(p.fl[3], 1)];
  if (p.k === "T") {
    q.w = "sine";
    q.fl = null;
  }
  if (p.vib) q.vib = [p.vib[0], p.vib[1] * (p.fade ? 1 : 0.5)];
  return q;
}

const noiseOf = new WeakMap<BaseAudioContext, AudioBuffer>();
/** One second of white noise, per context. */
function noise(c: BaseAudioContext): AudioBuffer {
  let b = noiseOf.get(c);
  if (!b) {
    b = c.createBuffer(1, c.sampleRate, c.sampleRate);
    const d = b.getChannelData(0);
    for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
    noiseOf.set(c, b);
  }
  return b;
}

/** A value along its points, gliding evenly over `d`. */
function glide(param: AudioParam, pts: number[], t0: number, d: number) {
  param.setValueAtTime(pts[0], t0);
  pts.slice(1).forEach((f, i) => param.exponentialRampToValueAtTime(f, t0 + (d * (i + 1)) / (pts.length - 1)));
}

const LEVEL = 0.2;
/** Schedules `cue` in `style` on context `c` into `dest` from `t0`. */
export function render(c: BaseAudioContext, dest: AudioNode, cue: Cue, style: Style, t0: number) {
  for (const raw of PACK[cue].parts) {
    const p = voice(raw, style);
    const s = t0 + p.t, e = s + p.d, a = Math.min(p.a ?? 0.004, p.d * 0.5);
    const g = c.createGain(), level = LEVEL * p.v;
    g.gain.setValueAtTime(0, s);
    g.gain.linearRampToValueAtTime(level, s + a);
    if (p.hold) {
      g.gain.setValueAtTime(level, Math.max(s + a, s + p.d * 0.7));
      g.gain.linearRampToValueAtTime(0, e);
    } else g.gain.exponentialRampToValueAtTime(0.0001, e);
    let src: AudioScheduledSourceNode;
    if (p.k === "T") {
      const o = c.createOscillator();
      o.type = p.w ?? "triangle";
      glide(o.frequency, p.f ?? [440], s, p.d);
      if (p.vib) {
        const lfo = c.createOscillator(), depth = c.createGain();
        lfo.frequency.value = p.vib[0];
        depth.gain.setValueAtTime(p.vib[1], s);
        if (p.fade) depth.gain.linearRampToValueAtTime(0, e);
        lfo.connect(depth).connect(o.detune);
        lfo.start(s);
        lfo.stop(e + 0.02);
      }
      src = o;
    } else {
      const n = c.createBufferSource();
      n.buffer = noise(c);
      src = n;
    }
    let out: AudioNode = src;
    if (p.fl) {
      const f = c.createBiquadFilter();
      f.type = p.fl[0];
      f.Q.value = p.fl[3];
      glide(f.frequency, [p.fl[1], p.fl[2]], s, p.d);
      out = out.connect(f);
    }
    out.connect(g).connect(dest);
    src.start(s);
    src.stop(e + 0.02);
  }
}

/** Loudness in dB against the common level: alarms 2 above in both styles,
 * the rest of Soft 3 below (so Soft's alarms stand out by 5), the tiny
 * click 6 below. */
export function loudness(cue: Cue, style: Style): number {
  const s = PACK[cue] as { alarm?: boolean; quiet?: boolean };
  if (s.alarm) return 2;
  return (s.quiet ? -6 : 0) + (style === "soft" ? -3 : 0);
}
