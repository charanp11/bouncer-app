import assert from "node:assert/strict";
import { test } from "node:test";
import { islandScale, WIDTH } from "./size.ts";

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
