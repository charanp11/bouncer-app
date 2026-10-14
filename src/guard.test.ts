import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// "Guard every mode" (PLAN, roadmap c3): the switch's text says exactly what
// it changes, matches the prototype, and the row only ever calls set_guard.
const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const prototype = readFileSync(new URL("../design/prototype/bouncer-island.html", import.meta.url), "utf8");

test("the guard switch says what it changes, word for word as the prototype", () => {
  const hint = /const GUARD_HINT =\s*"([^"]+)";/.exec(main)?.[1];
  assert.ok(hint, "GUARD_HINT in main.ts");
  for (const part of ["auto, accept-edits and bypass", "stop for a card", "claude -p", "refused", "Off by default"]) {
    assert.ok(hint.includes(part), part);
  }
  assert.ok(prototype.includes(`"${hint}"`), "the prototype's Guard every mode row has the same text");
  const row = main.slice(main.indexOf("function guardRow"), main.indexOf("/** Observe / auto."));
  assert.deepEqual([...row.matchAll(/invoke\("([a-z_]+)"/g)].map((m) => m[1]), ["set_guard"]);
  assert.match(row, /invoke\("set_guard", \{ on: !on \}\)/);
  assert.equal([...main.matchAll(/invoke\("set_guard"/g)].length, 1, "nothing else turns it on");
});
