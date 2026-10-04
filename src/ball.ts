// Bouncer, drawn in code: the SVG from design/prototype ("Meet Bouncer"),
// built element by element (never parsed from a string). The mood is a
// class on the wrapper (colors, which face shows, in styles.css); the motion
// comes from motion.ts, written into the SVG's transforms here.
//
// Frames run only while something moves, and never while the page is
// hidden or the OS asks for reduced motion. The island's Bouncer keeps one
// Motion across re-renders, so a rebuilt island doesn't restart him.
import { advance, blink, LOOK_MAX, lookAt, motion, moving, pose, setMood, squish, still, type Mood, type Motion, type Pose } from "./motion.ts";

export type BallState = Mood;

const NS = "http://www.w3.org/2000/svg";

type Part = [tag: string, attrs: Record<string, string>, text?: string];

/** Blinks come every 8–12 s. */
const BLINK_MIN_MS = 8000;
const BLINK_SPREAD_MS = 4000;
/** Four clicks within this make him dizzy. */
const DIZZY_CLICKS = 4;
const DIZZY_WINDOW_MS = 2000;
/** Cursor distance (px) at which he looks fully that way. */
const LOOK_REACH_PX = 40;

function part([tag, attrs, text]: Part): SVGElement {
  const node = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  if (text !== undefined) node.textContent = text;
  return node;
}

function group(cls: string, parts: (Part | SVGElement)[]): SVGElement {
  const g = part(["g", { class: cls }]);
  g.append(...parts.map((p) => (p instanceof SVGElement ? p : part(p))));
  return g;
}

type Live = {
  svg: SVGElement;
  jumper: SVGElement;
  shadow: SVGElement;
  face: SVGElement;
  lens: SVGElement;
  glint: SVGElement;
  m: Motion;
};

const reduce = matchMedia("(prefers-reduced-motion: reduce)");
const live = new Set<Live>();
/** The island's Bouncer. */
const island = motion("idle");
let raf = 0;
let last = 0;
let blinkTimer = 0;
let clicks: number[] = [];

const n = (x: number) => x.toFixed(3);

function draw(b: Live, p: Pose) {
  // Everything turns about the bottom centre (16, 29) and is transform-only:
  // he never changes the layout around him.
  b.jumper.setAttribute(
    "transform",
    `translate(0 ${n(-p.y)}) rotate(${n(p.rot)} 16 29) translate(16 29) scale(${n(p.sx)} ${n(p.sy)}) translate(-16 -29)`,
  );
  b.shadow.setAttribute("transform", `translate(16 30) scale(${n(p.shadow)} 1) translate(-16 -30)`);
  (b.shadow as SVGElement & ElementCSSInlineStyle).style.opacity = n(p.shadowOpacity);
  b.face.setAttribute("transform", `translate(${n(p.look[0])} ${n(p.look[1])})`);
  b.lens.setAttribute("transform", `rotate(${n(p.tilt)} 16 15.6) translate(0 15.6) scale(1 ${n(p.lens)}) translate(0 -15.6)`);
  b.glint.setAttribute("transform", `translate(${n(p.look[0] * 0.5)} ${n(p.look[1] * 0.3)})`);
}

function frame(now: number) {
  raf = 0;
  if (reduce.matches || document.hidden) {
    last = 0;
    return;
  }
  const dt = last ? (now - last) / 1000 : 0;
  last = now;
  let more = false;
  for (const b of live) {
    if (!b.svg.isConnected) {
      live.delete(b);
      continue;
    }
    advance(b.m, dt);
    draw(b, pose(b.m));
    more ||= moving(b.m);
  }
  if (more) raf = requestAnimationFrame(frame);
  else last = 0;
}

function kick() {
  if (!raf && !reduce.matches && !document.hidden && live.size) raf = requestAnimationFrame(frame);
}

/** One timer for every blink; it stops when no Bouncer is on screen. */
function scheduleBlink() {
  if (blinkTimer || reduce.matches) return;
  blinkTimer = setTimeout(() => {
    blinkTimer = 0;
    for (const b of live) if (b.svg.isConnected) blink(b.m);
    kick();
    if ([...live].some((b) => b.svg.isConnected)) scheduleBlink();
  }, BLINK_MIN_MS + Math.random() * BLINK_SPREAD_MS);
}

function poke(b: Live) {
  if (reduce.matches) return;
  const now = performance.now();
  clicks = clicks.filter((t) => now - t < DIZZY_WINDOW_MS);
  clicks.push(now);
  const dizzy = clicks.length >= DIZZY_CLICKS;
  if (dizzy) clicks = [];
  squish(b.m, dizzy);
  kick();
}

/** The cursor, from this page's own events only (it is over the island). */
document.addEventListener("mousemove", (e) => {
  if (reduce.matches) return;
  for (const b of live) {
    const r = b.svg.getBoundingClientRect();
    if (!r.width) continue;
    const dx = e.clientX - (r.left + r.width / 2);
    const dy = e.clientY - (r.top + r.height / 2);
    const d = Math.hypot(dx, dy) || 1;
    const k = Math.min(1, d / LOOK_REACH_PX) / d;
    lookAt(b.m, dx * k * LOOK_MAX, dy * k * LOOK_MAX);
  }
  kick();
});
document.documentElement.addEventListener("mouseleave", () => {
  for (const b of live) lookAt(b.m, 0, 0);
  kick();
});
document.addEventListener("visibilitychange", kick);
reduce.addEventListener("change", () => {
  for (const b of live) draw(b, reduce.matches ? still() : pose(b.m));
  kick();
  scheduleBlink();
});

/** The character in a mood: `big` is the 72 px detail-rail size, `sheet`
 * the 84 px character sheet. Pass a Motion of its own for a Bouncer other
 * than the island's. */
export function ball(state: BallState, size: "" | "big" | "sheet" = "", m: Motion = island): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = `ball ${state}${size ? ` ${size}` : ""}`;
  const svg = part(["svg", { viewBox: "0 0 32 32", "aria-hidden": "true" }]);
  const shadow = part(["ellipse", { class: "shadow", cx: "16", cy: "30", rx: "8", ry: "1.6" }]);
  const glint = part(["rect", { class: "glint", x: "9.4", y: "14.2", width: "3.4", height: "1.1", rx: ".55" }]);
  const lens = group("lens", [["rect", { class: "shades", x: "7.2", y: "13", width: "17.6", height: "5.2", rx: "2.6" }], glint]);
  const face = group("face", [
    lens,
    ["path", { class: "ink smile", d: "M12.6 22.2q3.4 2.6 6.8 0" }],
    ["path", { class: "ink flat", d: "M12.8 23h6.4" }],
    ["path", { class: "ink sleep", d: "M9.5 16.2q2.4 1.6 4.8 0M17.7 16.2q2.4 1.6 4.8 0" }],
  ]);
  const jumper = group("jumper", [
    ["circle", { class: "body-c", cx: "16", cy: "17", r: "12" }],
    ["path", { class: "deep", d: "M5.2 20.5a12 12 0 0 0 21.6 0a14 14 0 0 1-21.6 0z" }],
    ["ellipse", { class: "shine", cx: "10.5", cy: "10.8", rx: "3.2", ry: "1.9", transform: "rotate(-35 10.5 10.8)" }],
    face,
  ]);
  svg.append(
    shadow,
    jumper,
    part(["text", { class: "zz", x: "25", y: "7" }, "z"]),
    group("alert", [
      ["circle", { cx: "27", cy: "5", r: "4.2" }],
      ["text", { x: "25.9", y: "7.6" }, "!"],
    ]),
  );
  wrap.append(svg);

  // A rebuilt island replaces the old node; the Motion carries on.
  for (const b of live) if (b.m === m) live.delete(b);
  const b: Live = { svg, jumper, shadow, face, lens, glint, m };
  live.add(b);
  setMood(m, state);
  draw(b, reduce.matches ? still() : pose(m));
  // A click squashes him and goes no further (it doesn't open the island).
  svg.addEventListener("click", (e) => {
    e.stopPropagation();
    poke(b);
  });
  kick();
  scheduleBlink();
  return wrap;
}
