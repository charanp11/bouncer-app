// Bouncer's motion without the DOM: a fixed-step physics body (gravity,
// squash and stretch) and what each mood does with it. Units are the SVG's
// (viewBox 32), time is seconds. Numbers come from the prototype's Build spec.

export type Mood = "idle" | "working" | "needs" | "risky" | "done" | "paused" | "dance" | "greet";

/** Physics step; frames of any length are cut into these. */
const STEP = 1 / 240;
/** A longer frame (a stalled page) counts as this much. */
const MAX_FRAME = 0.05;
/** Working jump: 9 units high, 0.77 s in the air, 0.13 s on the ground (900 ms). */
const JUMP = 9;
const G = (8 * JUMP) / 0.77 ** 2;
const LAND = 0.13;
/** Squash spring (stiffness, damping) and how much a landing squashes. */
const K = 900;
const C = 30;
const IMPACT = 0.28;
/** Stretch at take-off speed. */
const STRETCH = 0.12;
/** Greeting: dropped from above the pill, bouncing back with half its speed. */
const DROP = 24;
const BOUNCE = 0.5;
/** Landings slower than this stop bouncing. */
const SETTLE_SPEED = 8;
const BREATHE = 3.2;
/** Idle breathes three times after something happens, then holds still. */
const BREATHS = 3;
const PUFF = 1.4;
const HOPS_EVERY = 1.6;
const DANCE = 0.48;
export const LOOK_MAX = 1.5;
const LOOK_RATE = 12;
const BLINK = 0.14;
const DIZZY = 1.5;
const SQUISH = 8;

export type Motion = {
  mood: Mood;
  /** Seconds in this mood. */
  t: number;
  acc: number;
  /** Height above the ground, speed (up is positive). */
  y: number;
  v: number;
  /** Vertical scale and its speed (squash below 1, stretch above). */
  sy: number;
  sv: number;
  /** Seconds since the last landing. */
  ground: number;
  /** Heights of the hops still to do, and the one in the air. */
  hops: number[];
  hop: number;
  /** Needs you: seconds to the next pair of hops. */
  cycle: number;
  bounce: number;
  look: [number, number];
  target: [number, number];
  /** Seconds left of a blink / of being dizzy. */
  blink: number;
  dizzy: number;
};

export type Pose = {
  y: number;
  sx: number;
  sy: number;
  rot: number;
  shadow: number;
  shadowOpacity: number;
  look: [number, number];
  /** Shades' vertical scale (the blink) and tilt (dizzy). */
  lens: number;
  tilt: number;
};

export function motion(mood: Mood = "idle"): Motion {
  const m: Motion = {
    mood: "paused",
    t: 0,
    acc: 0,
    y: 0,
    v: 0,
    sy: 1,
    sv: 0,
    ground: 1,
    hops: [],
    hop: 0,
    cycle: 0,
    bounce: 0,
    look: [0, 0],
    target: [0, 0],
    blink: 0,
    dizzy: 0,
  };
  setMood(m, mood);
  return m;
}

export function setMood(m: Motion, mood: Mood) {
  if (m.mood === mood) return;
  m.mood = mood;
  m.t = 0;
  m.cycle = 0;
  m.bounce = 0;
  m.hops = mood === "done" ? [11, 3, 11, 3] : [];
  if (mood === "greet") {
    m.y = DROP;
    m.v = 0;
    m.bounce = BOUNCE;
  }
  if (mood === "paused") m.target = [0, 0];
}

/** A click: squash, or dizzy on the fourth quick one. */
export function squish(m: Motion, dizzy: boolean) {
  if (m.dizzy > 0) return;
  if (dizzy) m.dizzy = DIZZY;
  else m.sv -= SQUISH;
}

export function blink(m: Motion) {
  if (m.mood !== "paused") m.blink = BLINK;
}

/** Where the face looks, in units (clamped to LOOK_MAX). */
export function lookAt(m: Motion, x: number, y: number) {
  const d = Math.hypot(x, y);
  const k = d > LOOK_MAX ? LOOK_MAX / d : 1;
  m.target = m.mood === "paused" ? [0, 0] : [x * k, y * k];
}

const speed = (h: number) => Math.sqrt(2 * G * h);

function step(m: Motion) {
  if (m.y > 0 || m.v > 0) {
    m.v -= G * STEP;
    m.y += m.v * STEP;
    if (m.y <= 0) {
      const impact = -m.v;
      m.y = 0;
      m.v = 0;
      m.ground = 0;
      m.sv -= impact * IMPACT;
      if (m.bounce && impact > SETTLE_SPEED) m.v = impact * m.bounce;
      else m.hop = 0;
    }
  } else {
    m.ground += STEP;
    if (m.mood === "working" && !m.hops.length) m.hops.push(JUMP);
    const gap = m.mood === "working" ? LAND : 0.06;
    const next = m.hops[0];
    if (next !== undefined && m.ground >= gap) {
      m.hops.shift();
      m.hop = next;
      m.v = speed(next);
    }
  }
  const airborne = m.y > 0 || m.v > 0;
  const target = airborne ? 1 + STRETCH * Math.min(1, Math.abs(m.v) / speed(JUMP)) : 1;
  m.sv += (-K * (m.sy - target) - C * m.sv) * STEP;
  m.sy += m.sv * STEP;
}

/** Moves the body on by one frame of `dt` seconds. */
export function advance(m: Motion, dt: number) {
  dt = Math.min(Math.max(dt, 0), MAX_FRAME);
  m.t += dt;
  if (m.mood === "needs") {
    m.cycle -= dt;
    if (m.cycle <= 0) {
      m.hops = [5, 3];
      m.cycle += HOPS_EVERY;
    }
  }
  m.acc += dt;
  while (m.acc >= STEP) {
    step(m);
    m.acc -= STEP;
  }
  const ease = 1 - Math.exp(-LOOK_RATE * dt);
  for (const i of [0, 1]) {
    m.look[i] += (m.target[i] - m.look[i]) * ease;
    if (Math.abs(m.target[i] - m.look[i]) < 0.01) m.look[i] = m.target[i];
  }
  m.blink = Math.max(0, m.blink - dt);
  m.dizzy = Math.max(0, m.dizzy - dt);
}

const breathing = (m: Motion) => m.mood === "idle" && m.t < BREATHE * BREATHS;

/** Whether another frame would change anything. */
export function moving(m: Motion): boolean {
  if (["working", "needs", "risky", "dance"].includes(m.mood)) return true;
  return (
    breathing(m) ||
    m.hops.length > 0 ||
    m.y > 0 ||
    m.v > 0 ||
    Math.abs(m.sy - 1) > 1e-3 ||
    Math.abs(m.sv) > 1e-2 ||
    m.look[0] !== m.target[0] ||
    m.look[1] !== m.target[1] ||
    m.blink > 0 ||
    m.dizzy > 0
  );
}

/** What to draw now. */
export function pose(m: Motion): Pose {
  let y = m.y;
  let sy = m.sy;
  let sx = 2 - m.sy;
  let rot = 0;
  const wave = (period: number) => Math.sin((Math.PI * m.t) / period) ** 2;
  if (breathing(m)) {
    const p = wave(BREATHE);
    sx *= 1 + 0.04 * p;
    sy *= 1 - 0.03 * p;
  }
  if (m.mood === "risky") {
    const p = wave(PUFF);
    sx *= 1 + 0.1 * p;
    sy *= 1 + 0.04 * p;
  }
  if (m.mood === "dance") {
    const p = wave(DANCE * 2);
    rot = -12 + 24 * p;
    y += 3 * p;
  }
  if (m.hop > 9) rot = (-8 * m.y) / m.hop;
  let tilt = 0;
  if (m.dizzy > 0) {
    const fade = m.dizzy / DIZZY;
    rot += 10 * Math.sin(2 * Math.PI * 3 * m.dizzy) * fade;
    tilt = 15 * fade;
  }
  const lift = Math.min(1, y / JUMP);
  const b = m.blink > 0 ? Math.sin((Math.PI * (BLINK - m.blink)) / BLINK) : 0;
  return {
    y,
    sx,
    sy,
    rot,
    shadow: 1 - 0.45 * lift,
    shadowOpacity: 0.4 - 0.25 * lift,
    look: [m.look[0], m.look[1]],
    lens: 1 - 0.8 * b,
    tilt,
  };
}

/** Reduced motion: the mood's still pose. */
export function still(): Pose {
  return { y: 0, sx: 1, sy: 1, rot: 0, shadow: 1, shadowOpacity: 0.35, look: [0, 0], lens: 1, tilt: 0 };
}
