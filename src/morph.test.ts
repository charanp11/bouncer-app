import assert from "node:assert/strict";
import { test } from "node:test";
import { holds, plan } from "./morph.ts";

const pill = { w: 323, h: 40 };
const open = { w: 448, h: 300 };
const page = (s: { w: number; h: number }) => ({ w: s.w + 45, h: s.h + 45 });

test("growing holds the bigger window before the island moves", () => {
  assert.deepEqual(plan(pill, open, page(open), page(pill), false), {
    kind: "grow",
    hold: { w: 493, h: 345 },
  });
});

test("closing keeps the current window until the island has shrunk", () => {
  assert.deepEqual(plan(open, pill, page(pill), page(open), false), {
    kind: "close",
    hold: { w: 493, h: 345 },
  });
});

test("a card, reduced motion or a re-render of the same size never morphs", () => {
  assert.deepEqual(plan(pill, open, page(open), page(pill), true), { kind: "still" });
  assert.deepEqual(plan(open, { w: 448.2, h: 300.3 }, page(open), page(open), false), { kind: "still" });
});

test("wider but shorter holds the larger of each side", () => {
  const wide = { w: 806, h: 200 };
  const p = plan(open, wide, page(wide), page(open), false);
  assert.deepEqual(p, { kind: "grow", hold: { w: 851, h: 345 } });
});

test("fully visible: the viewport holds the whole page, within 2 px", () => {
  assert.ok(holds({ w: 493, h: 344 }, { w: 494.5, h: 345.2 }));
  assert.ok(!holds({ w: 367, h: 85 }, { w: 493, h: 345 }));
  assert.ok(!holds({ w: 493, h: 300 }, { w: 493, h: 345 }));
});
