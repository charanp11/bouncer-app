import assert from "node:assert/strict";
import { test } from "node:test";
import { islandScale, openMaxHeight, scaleOf, WIDTH } from "./size.ts";

test("125% unless the island would pass 40% of the screen, never under 100%", () => {
  assert.equal(islandScale(WIDTH.pill, 1366), 1.25);
  assert.equal(islandScale(WIDTH.open, 1366), 1.25); // 500 px, 37%
  assert.equal(islandScale(WIDTH.open, 1920), 1.25);
  assert.equal(islandScale(WIDTH.wide, 1366), 1); // 720 is already 53%
  assert.ok(Math.abs(islandScale(WIDTH.wide, 1920) - 768 / 720) < 1e-9);
  assert.equal(islandScale(WIDTH.wide, 2560), 1.25);
  assert.equal(islandScale(WIDTH.open, 1000), 1);
  assert.equal(islandScale(WIDTH.pill, 0), 1.25);
  assert.equal(islandScale(WIDTH.pill, 1366, 1.5), 1.5);
});

test("sizes map to 100 / 125 / 150%, unknown names to the default", () => {
  assert.equal(scaleOf("small"), 1);
  assert.equal(scaleOf("medium"), 1.25);
  assert.equal(scaleOf("large"), 1.5);
  assert.equal(scaleOf("huge"), 1.25);
  assert.equal(scaleOf(undefined), 1.25);
});

test("the open island is never taller than the work area", () => {
  // 768-px screen with a 48-px taskbar: 720 px of work area.
  const h = openMaxHeight(720, 1.25);
  assert.ok((h + 8 + 32) * 1.25 <= 720 + 1e-9);
  assert.equal(openMaxHeight(1400, 1.25), 520);
  assert.ok((openMaxHeight(720, 1.5) + 8 + 32) * 1.5 <= 720 + 1e-9);
  assert.equal(openMaxHeight(0, 1.25), 520);
});
