import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { group, latest, STALE_MS, stepIcon, title, waitingInTerminal } from "./steps.ts";

const s = (label: string, how = "", ran = false) => ({ label, how, ran });

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
  // A run that ran and one that didn't show different icons: kept apart.
  assert.deepEqual(group([s("ls", "", true), s("ls", "", true), s("ls")]).map(title), ["ls ×2", "ls"]);
});

test("only the latest six are shown, the rest counted", () => {
  const g = group(Array.from({ length: 9 }, (_, i) => s(`Step ${i}`)));
  const { shown, earlier } = latest(g);
  assert.equal(earlier, 3);
  assert.deepEqual(shown.map(title), ["Step 3", "Step 4", "Step 5", "Step 6", "Step 7", "Step 8"]);
  assert.deepEqual(latest(group([s("a")])), { shown: [{ label: "a", how: "", ran: false, count: 1 }], earlier: 0 });
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

test("only a step whose PostToolUse came gets the green check", () => {
  for (const ran of [false, true]) {
    for (const now of [false, true]) {
      for (const waiting of [false, true]) {
        const at = `ran ${ran}, now ${now}, waiting ${waiting}`;
        assert.equal(stepIcon("you denied", ran, now, waiting), "no", at);
        assert.equal(stepIcon("failed", ran, now, waiting), "warn", at);
        assert.notEqual(stepIcon("not run (answered in terminal)", ran, now, waiting), "ok", at);
        assert.notEqual(stepIcon("answered in terminal", ran, now, waiting), "ok", at);
      }
    }
  }
  for (const how of ["you allowed", "auto-allowed by rule", "allowed in terminal", ""]) {
    assert.equal(stepIcon(how, true, false, false), "ok", how);
    // Finished with neither PostToolUse nor PostToolUseFailure: neutral ring.
    assert.equal(stepIcon(how, false, false, false), "mid", how);
  }
  assert.equal(stepIcon("you allowed", false, true, false), "spin");
  assert.equal(stepIcon("waiting for you", false, true, true), "wait");
});

test("a long step never looks frozen: the spinner turns, then settles into a still running mark", () => {
  const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  // Nothing on the island animates forever (motion budget).
  assert.doesNotMatch(css, /infinite/);
  // The turns end exactly when the running mark appears.
  // "running" and "paused" are play states: in the shorthand they never name
  // a keyframes (the hand check of #19 caught a settle step that never ran).
  for (const decl of css.match(/animation:[^;]+;/g) ?? []) assert.doesNotMatch(decl, /\b(running|paused)\b/, decl);
  const spin = css.match(/\.ic\.spin \{[^}]*animation:\s*spin (\d+)ms linear (\d+),\s*settle 0s (\d+)ms forwards;/);
  assert.ok(spin, "the spinner turns, then settles");
  const [turn, turns, settle] = spin.slice(1).map(Number);
  assert.equal(turn * turns, settle);
  const dots = css.match(/\.ic\.spin::after \{[^}]*opacity: 0;[^}]*animation: settle-dots 0s (\d+)ms forwards;/);
  assert.equal(Number(dots?.[1]), settle, "the dots appear with it");
  assert.match(css, /@keyframes settle \{\s*to \{\s*border-color: transparent;\s*background: var\(--sun\);/);
  // Reduced motion: no turns, the mark from the start.
  const reduced = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce)"));
  assert.match(reduced, /\.ic\.spin \{\s*border-color: transparent;\s*background: var\(--sun\);/);
  assert.match(reduced, /\.ic\.spin::after \{\s*opacity: 1;/);
  // Unlike done (green), waiting and neutral (hollow rings): a sun disc.
  for (const other of ["ok", "wait", "mid"]) {
    assert.doesNotMatch(css.match(new RegExp(String.raw`\.ic\.${other} \{[^}]*\}`))![0], /--sun/, other);
  }
});
