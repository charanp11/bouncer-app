import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { ARM_MS, armState } from "./arm.ts";

test("arm: counted once from the card being fully visible; a pick never restarts it", () => {
  const shown = 1000;
  assert.deepEqual(armState(0, 5000), { look: "waiting" }, "not visible yet: no fill, disabled");
  assert.deepEqual(armState(shown, shown + 200), { look: "arming", waited: 200, left: ARM_MS - 200 });
  // Pick at +900 ms (a re-render): Send is ready at once, from the card's time.
  assert.deepEqual(armState(shown, shown + 900, true), { look: "ready" });
  // Picked in the first 600 ms: the fill carries on from the card's time.
  assert.deepEqual(armState(shown, shown + 450, true), { look: "arming", waited: 450, left: ARM_MS - 450 });
  assert.deepEqual(armState(shown, shown + ARM_MS), { look: "ready" });
});

test("arm: a button that can't be pressed always looks it (blocked: no fill)", () => {
  assert.deepEqual(armState(1000, 9000, false), { look: "blocked" }, "a question without a pick");
  assert.deepEqual(armState(0, 9000, false), { look: "blocked" });
});

test("arm: only the card's visibility (and the Explain box, which moves the buttons) sets the arm's start", () => {
  const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  const sets = [...main.matchAll(/front\.since = ([^;]+);/g)].map((m) => m[1]);
  assert.deepEqual(sets, ["now", "performance.now()"], "rearm (Explain) and armIfVisible only");
  assert.match(main, /const send = armed\("Send answer", front\.since, ready\(picked\.chosen, qs\.length\)\);/);
  assert.doesNotMatch(main, /\.disabled = true;\s*\n\s*\w+\.classList\.add\("(incomplete|blocked)"\)/, "no button disabled outside armState's looks");
});

test("arm: a press on a disabled control is shown, and a refused answer says why", () => {
  const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(main, /closest<HTMLElement>\("button:disabled"\)[\s\S]{0,200}classList\.add\("notyet"\)/);
  assert.match(main, /notSent = \{ id: request\.id, text: `Not sent: \$\{String\(code\)\}\. Try again\.`/);
  assert.match(css, /\.btn\.allow\.blocked \.arm \{\s*animation: none;/);
  assert.match(css, /\.notyet \{/);
});
