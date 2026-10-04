import assert from "node:assert/strict";
import { test } from "node:test";
import { allowed, cue, type Heard } from "./sound.ts";

const view = (over: object = {}) => ({ paused: false, queue: [], sessions: [], ...over });
const card = (id: string, risk: string | null = null) => ({ id, risk });
const asking = (id: string) => ({ id, status: "needs you" });

/** Runs views through `cue` in order, returning the sounds. */
function hear(views: [object, boolean?][]): (string | null)[] {
  let heard: Heard | null = null;
  return views.map(([v, finished]) => {
    const r = cue(heard, view(v), finished ?? false);
    heard = r.heard;
    return r.cue;
  });
}

test("the first view is quiet, a new card sounds once", () => {
  assert.deepEqual(hear([[{ queue: [card("a")] }], [{ queue: [card("a")] }], [{ queue: [card("b")] }]]), [null, null, "needs"]);
  assert.deepEqual(hear([[{}], [{ queue: [card("a")] }], [{ queue: [card("a")] }]]), [null, "needs", null]);
});

test("a risky card gets the risky sound; it beats a finished session", () => {
  assert.deepEqual(hear([[{}], [{ queue: [card("a", "pipes a download to sh")] }, true]]), [null, "risky"]);
});

test("a question in the terminal sounds; one that stays doesn't again", () => {
  assert.deepEqual(hear([[{}], [{ sessions: [asking("s")] }], [{ sessions: [asking("s")] }]]), [null, "needs", null]);
  // The same session's card already sounded: its "needs you" status adds nothing.
  assert.deepEqual(hear([[{}], [{ queue: [card("a")], sessions: [asking("s")] }], [{ sessions: [asking("s")] }]]), [null, "needs", null]);
});

test("done when a session finishes; nothing while paused", () => {
  assert.deepEqual(hear([[{}], [{}, true]]), [null, "done"]);
  assert.deepEqual(hear([[{}], [{ paused: true, queue: [card("a")] }], [{ paused: true }, true]]), [null, null, null]);
});

test("one at a time, and the same sound within 2 s is dropped", () => {
  const none = { kind: null, at: 0 };
  assert.equal(allowed("needs", 0, none), true);
  const last = { kind: "needs" as const, at: 1000 };
  assert.equal(allowed("risky", 1200, last), false); // still playing
  assert.equal(allowed("risky", 1500, last), true);
  assert.equal(allowed("needs", 2500, last), false); // repeat within 2 s
  assert.equal(allowed("needs", 3000, last), true);
});
