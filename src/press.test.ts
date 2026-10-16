import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { MAX_HOLD_MS, PressGate } from "./press.ts";

function gate() {
  const drawn: string[] = [];
  const queued: (() => void)[] = [];
  let t = 0;
  const g = new PressGate<string>((v) => drawn.push(v), (f) => queued.push(f), () => t);
  return { g, drawn, queued, tick: (ms: number) => (t += ms), flush: () => queued.splice(0).forEach((f) => f()) };
}

test("clicks: no rebuild under a press; the newest view is drawn after the release", () => {
  const { g, drawn, queued, flush } = gate();
  assert.equal(g.hold("a"), false, "no press: draw now");
  g.press();
  assert.equal(g.hold("b"), true);
  assert.equal(g.hold("c"), true);
  assert.deepEqual(drawn, [], "nothing rebuilt while pressed");
  g.release();
  assert.equal(queued.length, 1, "drawn after the click, not inside the release");
  flush();
  assert.deepEqual(drawn, ["c"], "only the newest");
  assert.equal(g.hold("d"), false, "released: draw now");
  g.release();
  assert.equal(queued.length, 0, "a second release draws nothing");
});

test("clicks: a view drawn after the release is never overwritten by the older one that waited", () => {
  // The gear: the keyboard view waits under the press; the click opens
  // Settings and that view is drawn first. The old one must not follow it.
  const { g, drawn, flush } = gate();
  g.press();
  assert.equal(g.hold("keyboard, no settings"), true);
  g.release();
  assert.equal(g.hold("settings open"), false, "drawn by the caller at once");
  flush();
  assert.deepEqual(drawn, [], "the stale view is dropped");
  g.press();
  g.hold("x");
  g.release();
  g.press();
  g.hold("y");
  g.release();
  flush();
  assert.deepEqual(drawn, ["y"], "two quick presses: only the newest is drawn");
});

test("clicks: a press that never ends (a drag) stops holding after MAX_HOLD_MS", () => {
  const { g, tick } = gate();
  g.press();
  assert.equal(g.hold("a"), true);
  tick(MAX_HOLD_MS);
  assert.equal(g.hold("b"), false, "the island can't stall");
  assert.equal(g.hold("c"), false);
});

test("clicks: the page holds every view during a press and releases on pointerup, cancel and blur", () => {
  const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  assert.match(main, /function render\(view: View\) \{\n {2}\/\/[^\n]*\n {2}if \(gate\.hold\(view\)\) return;/);
  for (const e of ["pointerup", "pointercancel", "blur"]) {
    assert.match(main, new RegExp(`window\\.addEventListener\\("${e}", \\(\\) => gate\\.release\\(\\), true\\);`), e);
  }
  assert.match(main, /gate\.press\(\);/);
});
