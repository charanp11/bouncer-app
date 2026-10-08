import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { advance, blink, BODY_ORIGIN, boxes, gesture, lookAt, motion, moving, pose, setMood, SHADOW_ORIGIN, squish, still, VIEW, type Motion } from "./motion.ts";

/** Runs `seconds` at `hz` (jittered by up to ±50% when asked); returns the poses. */
function run(m: Motion, seconds: number, hz: number, jitter = false) {
  let seed = 7;
  const rand = () => ((seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31);
  const poses = [];
  for (let t = 0; t < seconds; ) {
    const dt = (1 / hz) * (jitter ? 0.5 + rand() : 1);
    advance(m, dt);
    t += dt;
    poses.push(pose(m));
  }
  return poses;
}

const max = (xs: number[]) => Math.max(...xs);
const min = (xs: number[]) => Math.min(...xs);

test("the working jump is 9 units at any frame rate", () => {
  for (const [hz, jitter] of [[30, false], [60, false], [144, false], [240, false], [60, true], [20, true]] as const) {
    const poses = run(motion("working"), 9, hz, jitter);
    assert.ok(Math.abs(max(poses.map((p) => p.y)) - 9) < 0.2, `${hz} Hz`);
    for (const p of poses) assert.ok(Number.isFinite(p.y) && p.y >= 0 && Number.isFinite(p.sy));
  }
});

test("a jump lasts about 900 ms; a working burst is two of them", () => {
  const m = motion("working");
  const landings: number[] = [];
  let airborne = false;
  for (let i = 0; i < 4 * 60; i++) {
    advance(m, 1 / 60);
    if (airborne && m.y === 0) landings.push(i / 60);
    airborne = m.y > 0;
  }
  assert.equal(landings.length, 2, `${landings.length} landings`);
  assert.ok(Math.abs(landings[1] - landings[0] - 0.9) < 0.05, `${landings[1] - landings[0]} s apart`);
});

test("squash and stretch stay near the spec (1.14 / 1.12)", () => {
  const poses = run(motion("working"), 5, 240);
  const sy = poses.map((p) => p.sy);
  assert.ok(min(sy) > 0.8 && min(sy) < 0.9, `squash ${min(sy)}`);
  assert.ok(max(sy) > 1.06 && max(sy) <= 1.13, `stretch ${max(sy)}`);
});

test("the greeting drop bounces three times, lower each time, and rests within 2.2 s", () => {
  const m = motion("greet");
  const apexes: number[] = [];
  let rising = false;
  let last = m.y;
  for (let i = 0; i < 2.2 * 60; i++) {
    advance(m, 1 / 60);
    if (rising && m.y < last) apexes.push(last);
    rising = m.y > last;
    last = m.y;
  }
  assert.equal(apexes.length, 3, `${apexes.length} bounces`);
  for (let i = 1; i < apexes.length; i++) assert.ok(apexes[i] < apexes[i - 1]);
  assert.ok(apexes[0] < 24);
  assert.equal(moving(m), false);
});

test("idle breathes, then holds still", () => {
  const m = motion("idle");
  advance(m, 1 / 60);
  assert.equal(moving(m), true);
  run(m, 10, 60);
  assert.equal(moving(m), false);
});

test("done cheers, then stops", () => {
  const m = motion("done");
  const poses = run(m, 4, 60);
  assert.ok(max(poses.map((p) => p.y)) > 10);
  assert.ok(min(poses.map((p) => p.rot)) < -5);
  setMood(m, "idle");
  run(m, 10, 60);
  assert.equal(moving(m), false);
});

test("paused never moves and never looks", () => {
  const m = motion("paused");
  lookAt(m, 5, 0);
  assert.equal(moving(m), false);
  assert.deepEqual(pose(m).look, [0, 0]);
});

test("a squish springs back without leaving the ground", () => {
  const m = motion("idle");
  run(m, 10, 60);
  squish(m, false);
  const poses = run(m, 2, 60);
  assert.ok(min(poses.map((p) => p.sy)) < 0.9);
  assert.ok(poses.every((p) => p.y === 0));
  assert.equal(moving(m), false);
});

test("dizzy wobbles, ignores clicks, then stops", () => {
  const m = motion("idle");
  run(m, 10, 60);
  squish(m, true);
  advance(m, 0.2);
  assert.notEqual(pose(m).tilt, 0);
  squish(m, false);
  assert.equal(m.sv, 0);
  run(m, 2, 60);
  assert.equal(moving(m), false);
  assert.equal(pose(m).tilt, 0);
});

test("looking is clamped and eases in", () => {
  const m = motion("idle");
  lookAt(m, 30, 40);
  advance(m, 1 / 60);
  const [x, y] = pose(m).look;
  assert.ok(x > 0 && x < 0.9 && y > 0);
  run(m, 2, 60);
  assert.ok(Math.abs(Math.hypot(...pose(m).look) - 1.5) < 1e-9);
});

test("a stalled page doesn't make it jump further", () => {
  const m = motion("working");
  advance(m, 5);
  advance(m, 1e9);
  advance(m, -1);
  assert.ok(m.y >= 0 && m.y < 10 && Number.isFinite(m.v));
});

test("a blink shuts and opens the shades without asking for frames", () => {
  const m = motion("idle");
  run(m, 10, 60);
  blink(m);
  assert.equal(pose(m).lens, 0.2);
  assert.equal(moving(m), false);
  blink(m, true);
  assert.equal(pose(m).lens, 1);
  setMood(m, "paused");
  blink(m);
  assert.equal(pose(m).lens, 1);
});

test("the body and shadow boxes carry the pose as CSS, about the right points", () => {
  assert.deepEqual(boxes(still()), {
    lift: "translateY(0.000%) rotate(0.000deg) scale(1.000,1.000)",
    shade: "scale(1.000,1)",
    fade: "0.350",
  });
  // Up is negative; a unit is 1/32 of the box, whatever its size.
  const p = { ...still(), y: 8, rot: -6, sx: 0.9, sy: 1.1, shadow: 0.6, shadowOpacity: 0.25 };
  assert.deepEqual(boxes(p), {
    lift: "translateY(-25.000%) rotate(-6.000deg) scale(0.900,1.100)",
    shade: "scale(0.600,1)",
    fade: "0.250",
  });
  // styles.css turns the boxes about the same points the old SVG did.
  const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  const origin = (sel: string) =>
    css.match(new RegExp(String.raw`\.ball \.${sel} \{[^}]*transform-origin: ([\d.]+)% ([\d.]+)%`))?.slice(1).map(Number);
  const pct = ([x, y]: readonly [number, number]) => [(x / VIEW) * 100, (y / VIEW) * 100];
  assert.deepEqual(origin("lift"), pct(BODY_ORIGIN));
  assert.deepEqual(origin("shade"), pct(SHADOW_ORIGIN));
});

/** Frames until the loop would stop (moving() false), at 60 Hz, capped. */
function untilStill(m: Motion, cap = 10) {
  let t = 0;
  while (moving(m) && t < cap) {
    advance(m, 1 / 60);
    t += 1 / 60;
  }
  return t;
}

test("every mood moves in one short burst, then the loop stops and he holds still", () => {
  for (const [mood, most] of [["working", 3], ["needs", 2], ["risky", 2], ["done", 3.3], ["greet", 2.5], ["idle", 3.5]] as const) {
    const m = motion(mood);
    advance(m, 1 / 60);
    assert.equal(moving(m), true, `${mood} starts moving`);
    const t = untilStill(m);
    assert.ok(t > 0.5 && t <= most, `${mood} burst ${t.toFixed(2)} s`);
    // Stopped: no frame needed, and time passing changes nothing.
    const held = pose(m);
    advance(m, 5);
    assert.equal(moving(m), false, `${mood} stays still`);
    const now = pose(m);
    for (const k of ["y", "sx", "sy", "rot", "shadow", "shadowOpacity"] as const) {
      // Under moving()'s own threshold: nothing anyone could see.
      assert.ok(Math.abs(now[k] - held[k]) <= 1e-3, `${mood} holds its pose (${k})`);
    }
  }
});

test("a card's first appearance still gets its pair of hops, then only gestures", () => {
  const m = motion("needs");
  const apexes = run(m, 5, 60).filter((p, i, a) => i > 0 && i < a.length - 1 && p.y > a[i - 1].y && p.y >= a[i + 1].y && p.y > 0.5);
  assert.equal(apexes.length, 2, "one pair of hops");
  assert.equal(moving(m), false);
});

test("gestures: one jump, hop or puff under about a second, then still again", () => {
  for (const mood of ["working", "needs", "risky"] as const) {
    const m = motion(mood);
    untilStill(m);
    gesture(m);
    assert.equal(moving(m), true, mood);
    const t = untilStill(m);
    assert.ok(t > 0.3 && t < 1.2, `${mood} gesture ${t.toFixed(2)} s`);
    assert.equal(pose(m).y, 0);
  }
  // Moods without a gesture don't start the loop.
  for (const mood of ["idle", "done", "paused", "greet"] as const) {
    const m = motion(mood);
    untilStill(m);
    gesture(m);
    assert.equal(moving(m), false, mood);
  }
});
