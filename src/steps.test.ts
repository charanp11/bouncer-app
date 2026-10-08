import assert from "node:assert/strict";
import { test } from "node:test";
import { group, latest, STALE_MS, stepIcon, title, waitingInTerminal } from "./steps.ts";

const s = (label: string, how = "") => ({ label, how });

test("repeated steps group, different outcomes stay apart", () => {
  const g = group([
    s("Searching"),
    s("Searching"),
    s("Searching"),
    s("Editing a.rs", "you denied"),
    s("Editing a.rs", "waiting for you"),
    s("Searching"),
  ]);
  assert.deepEqual(g.map(title), ["Searching ×3", "Editing a.rs", "Editing a.rs", "Searching"]);
  assert.deepEqual(g.map((x) => x.how), ["", "you denied", "waiting for you", ""]);
});

test("only the latest six are shown, the rest counted", () => {
  const g = group(Array.from({ length: 9 }, (_, i) => s(`Step ${i}`)));
  const { shown, earlier } = latest(g);
  assert.equal(earlier, 3);
  assert.deepEqual(shown.map(title), ["Step 3", "Step 4", "Step 5", "Step 6", "Step 7", "Step 8"]);
  assert.deepEqual(latest(group([s("a")])), { shown: [{ label: "a", how: "", count: 1 }], earlier: 0 });
  assert.deepEqual(latest([]), { shown: [], earlier: 0 });
});

test("a request left in the terminal stops spinning after two quiet minutes", () => {
  const asked = 1_000_000;
  assert.equal(waitingInTerminal("asks in terminal", asked, asked + STALE_MS - 1), false);
  assert.equal(waitingInTerminal("asks in terminal", asked, asked + STALE_MS), true);
  // Only that state: working, waiting on a card, or idle sessions never do.
  for (const status of ["working", "needs you", "idle"]) {
    assert.equal(waitingInTerminal(status, asked, asked + 10 * STALE_MS), false, status);
  }
});

test("a denied or failed step never gets the green check; only allowed outcomes do", () => {
  for (const now of [false, true]) {
    for (const waiting of [false, true]) {
      assert.equal(stepIcon("you denied", now, waiting), "no", `now ${now}, waiting ${waiting}`);
      assert.equal(stepIcon("failed", now, waiting), "warn", `now ${now}, waiting ${waiting}`);
      assert.notEqual(stepIcon("not run (answered in terminal)", now, waiting), "ok");
      assert.notEqual(stepIcon("answered in terminal", now, waiting), "ok");
    }
  }
  for (const allowed of ["you allowed", "auto-allowed by rule", "allowed in terminal", ""]) {
    assert.equal(stepIcon(allowed, false, false), "ok", allowed);
  }
  assert.equal(stepIcon("you allowed", true, false), "spin");
  assert.equal(stepIcon("waiting for you", true, true), "wait");
});
