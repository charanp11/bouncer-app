import { test } from "node:test";
import assert from "node:assert/strict";
import { FLASH_MS, flashLeft } from "./flash.ts";

const none = { n: 0, from: 0 };

test("a flash shown at once runs its full time", () => {
  const first = flashLeft(1, 1000, 1000, none);
  assert.equal(first.left, FLASH_MS);
  assert.equal(flashLeft(1, 1000, 1000 + FLASH_MS, first.seen).left, 0);
});

test("a flash raised under the 'Rule added' message starts when the pill is back", () => {
  // Raised at 1000, the message hides the pill for 1600 ms.
  const first = flashLeft(2, 1000, 2600, none);
  assert.equal(first.left, FLASH_MS);
  assert.equal(flashLeft(2, 1000, 2600 + 2000, first.seen).left, FLASH_MS - 2000);
});

test("a stale flash is never shown", () => {
  // Raised while a card stayed up for a minute.
  assert.ok(flashLeft(3, 1000, 61_000, none).left <= 0);
});

test("re-renders keep the start time", () => {
  const first = flashLeft(4, 0, 100, none);
  const again = flashLeft(4, 0, 900, first.seen);
  assert.deepEqual(again.seen, first.seen);
  assert.equal(again.left, FLASH_MS - 800);
});
