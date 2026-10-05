import assert from "node:assert/strict";
import { test } from "node:test";
import { frameSize, islandScale, openMaxHeight, scaleOf, WIDTH } from "./size.ts";

test("the scale unless the island would pass 40% of the screen, never under 100%", () => {
  assert.equal(islandScale(WIDTH.pill, 1366), 1.12);
  assert.equal(islandScale(WIDTH.open, 1366), 1.12); // 448 px, 33%
  assert.ok(Math.abs(islandScale(WIDTH.open, 1366, 1.5) - 1.366) < 1e-9); // capped at 40%
  assert.equal(islandScale(WIDTH.wide, 1366), 1); // 720 is already 53%
  assert.ok(Math.abs(islandScale(WIDTH.wide, 1920, 1.25) - 768 / 720) < 1e-9);
  assert.equal(islandScale(WIDTH.wide, 2560, 1.25), 1.25);
  assert.equal(islandScale(WIDTH.open, 1000), 1);
  assert.equal(islandScale(WIDTH.pill, 0), 1.12);
});

test("sizes map to 100 / 112 / 125%, unknown names to the default", () => {
  assert.equal(scaleOf("small"), 1);
  assert.equal(scaleOf("medium"), 1.12);
  assert.equal(scaleOf("large"), 1.25);
  assert.equal(scaleOf("huge"), 1.12);
  assert.equal(scaleOf(undefined), 1.12);
});

test("the open island is never taller than the work area", () => {
  // 768-px screen with a 48-px taskbar: 720 px of work area.
  const h = openMaxHeight(720, 1.25);
  assert.ok((h + 8 + 32) * 1.25 <= 720 + 1e-9);
  assert.equal(openMaxHeight(1400, 1.25), 520);
  assert.ok((openMaxHeight(720, 1.5) + 8 + 32) * 1.5 <= 720 + 1e-9);
  assert.equal(openMaxHeight(0, 1.25), 520);
});

test("the frame fits every view and the work area", () => {
  // Charan's screen: 1366 × 768, 720 px of work area, Medium.
  const f = frameSize(1366, 720, 1.12);
  assert.equal(f.w, 760); // the wide view at 100% (720 + 2 × 20)
  assert.ok(f.w >= (400 + 40) * 1.12 && f.w >= (288 + 40) * 1.12);
  assert.ok(f.h <= 720 && f.h >= 600);
  const big = frameSize(2560, 1400, 1.25);
  assert.equal(big.w, Math.ceil(760 * 1.25));
  assert.equal(big.h, Math.ceil((8 + 520 + 32) * 1.25));
});
