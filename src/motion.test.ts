import assert from "node:assert/strict";
import { test } from "node:test";
import { advance, blink, lookAt, motion, moving, pose, setMood, squish, type Motion } from "./motion.ts";

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

test("a jump lasts about 900 ms", () => {
  const m = motion("working");
  let landings = 0;
  let airborne = false;
  for (let i = 0; i < 9 * 60; i++) {
    advance(m, 1 / 60);
    if (airborne && m.y === 0) landings++;
    airborne = m.y > 0;
  }
  assert.ok(landings >= 9 && landings <= 11, `${landings} landings in 9 s`);
});

test("squash and stretch stay near the spec (1.14 / 1.12)", () => {
  const poses = run(motion("working"), 5, 240);
  const sy = poses.map((p) => p.sy);
  assert.ok(min(sy) > 0.8 && min(sy) < 0.9, `squash ${min(sy)}`);
  assert.ok(max(sy) > 1.06 && max(sy) <= 1.13, `stretch ${max(sy)}`);
});

test("the greeting drop loses height every bounce and comes to rest", () => {
  const m = motion("greet");
  const apexes: number[] = [];
  let rising = false;
  let last = m.y;
  for (let i = 0; i < 4 * 60; i++) {
    advance(m, 1 / 60);
    if (rising && m.y < last) apexes.push(last);
    rising = m.y > last;
    last = m.y;
  }
  assert.ok(apexes.length >= 2, `${apexes.length} bounces`);
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
