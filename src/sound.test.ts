import assert from "node:assert/strict";
import { test } from "node:test";
import { allowed, cue, wanted, type Heard, type Tuning } from "./sound.ts";

const view = (over: object = {}) => ({ paused: false, queue: [], sessions: [], ...over });
const card = (id: string, risk: string | null = null) => ({ id, risk });
const asking = (id: string) => ({ id, status: "needs you" });
const working = (id: string, failed = 0) => ({ id, status: "working", failed });

/** Runs views through `cue` in order, returning the sounds. */
function hear(views: [object, boolean?][]): (string | null)[] {
  let heard: Heard | null = null;
  return views.map(([v, finished]) => {
    const r = cue(heard, view(v), finished ?? false);
    heard = r.heard;
    return r.cue;
  });
}

test("the first view is the launch \"yo!\" only; a card there stays quiet", () => {
  assert.deepEqual(hear([[{ queue: [card("a")] }], [{ queue: [card("a")] }], [{ queue: [card("b")] }]]), ["launch", null, "needs"]);
  assert.deepEqual(hear([[{}], [{ queue: [card("a")] }], [{ queue: [card("a")] }]]), ["launch", "needs", null]);
});

test("launching paused is silent; pausing snores once, resuming bips once", () => {
  assert.deepEqual(hear([[{ paused: true }], [{ paused: true }], [{}], [{}]]), [null, null, "resumed", null]);
  assert.deepEqual(hear([[{}], [{ paused: true }], [{ paused: true }]]), ["launch", "paused", null]);
});

test("nothing else while paused", () => {
  const busy = { paused: true, queue: [card("a", "risky")], sessions: [working("s", 3)], flash: { n: 4, badge: "rule" }, away: {} };
  assert.deepEqual(hear([[{}], [{ paused: true }], [busy], [busy, true]]), ["launch", "paused", null, null]);
});

test("a risky card gets the risky sound; it beats a finished session", () => {
  assert.deepEqual(hear([[{}], [{ queue: [card("a", "pipes a download to sh")] }, true]]), ["launch", "risky"]);
});

test("a question in the terminal sounds; one that stays doesn't again", () => {
  assert.deepEqual(hear([[{}], [{ sessions: [asking("s")] }], [{ sessions: [asking("s")] }]]), ["launch", "needs", null]);
  // The same session's card already sounded: its "needs you" status adds nothing.
  assert.deepEqual(hear([[{}], [{ queue: [card("a")], sessions: [asking("s")] }], [{ sessions: [asking("s")] }]]), ["launch", "needs", null]);
});

test("a failed tool call: once per new failure, in any session", () => {
  const s = (n: number) => ({ sessions: [working("s", n)] });
  assert.deepEqual(hear([[s(0)], [s(1)], [s(1)], [s(2)]]), ["launch", "fail", null, "fail"]);
  // A session that ends takes its count with it; another one's failure still sounds.
  assert.deepEqual(hear([[{ sessions: [working("a", 2), working("b")] }], [{ sessions: [working("b", 1)] }]]), ["launch", "fail"]);
  assert.deepEqual(hear([[{ sessions: [working("a", 2)] }], [{ sessions: [] }]]), ["launch", null]);
});

test("done, a new session, welcome back, auto-allowed", () => {
  assert.deepEqual(hear([[{}], [{}, true]]), ["launch", "done"]);
  assert.deepEqual(hear([[{ sessions: [working("a")] }], [{ sessions: [working("a"), working("b")] }]]), ["launch", "session"]);
  assert.deepEqual(hear([[{}], [{ away: { minutes: 30 } }], [{ away: { minutes: 30 } }], [{}], [{ away: {} }]]), ["launch", "back", null, null, "back"]);
  // Only the auto-allowed flash, and only when its count goes up.
  const f = (n: number, badge = "rule") => ({ flash: { n, badge } });
  // The count is shared with "Rules changed", so it skips one.
  assert.deepEqual(hear([[f(1)], [f(1)], [f(2)], [f(3, "rules")], [f(4)]]), ["launch", null, "auto", null, "auto"]);
});

test("one at a time: an alarm cuts a lighter sound, nothing else does", () => {
  const none = { kind: null, at: 0, until: 0 };
  assert.equal(allowed("needs", 0, none), true);
  const poke = { kind: "poke" as const, at: 1000, until: 1400 };
  assert.equal(allowed("risky", 1100, poke), true);
  assert.equal(allowed("needs", 1100, poke), true);
  assert.equal(allowed("done", 1100, poke), false);
  const needs = { kind: "needs" as const, at: 1000, until: 1770 };
  assert.equal(allowed("risky", 1200, needs), true);
  assert.equal(allowed("needs", 1200, needs), false);
  assert.equal(allowed("fail", 1200, needs), false);
  const risky = { kind: "risky" as const, at: 1000, until: 1640 };
  assert.equal(allowed("needs", 1200, risky), false);
});

test("the same sound within 2 s is dropped once it has ended", () => {
  const last = { kind: "needs" as const, at: 1000, until: 1770 };
  assert.equal(allowed("needs", 2500, last), false);
  assert.equal(allowed("needs", 3000, last), true);
  assert.equal(allowed("done", 1800, last), true);
});

test("sound off, a sound switched off, or paused: silence (the snore excepted)", () => {
  const t: Tuning = { on: true, style: "soft", volume: 50, sounds: { needs: true, paused: true, launch: true }, paused: false };
  assert.equal(wanted("needs", t), true);
  assert.equal(wanted("poke", t), false);
  assert.equal(wanted("needs", { ...t, on: false }), false);
  assert.equal(wanted("launch", { ...t, on: false }), false);
  assert.equal(wanted("launch", { ...t, paused: true }), false);
  assert.equal(wanted("needs", { ...t, paused: true }), false);
  assert.equal(wanted("paused", { ...t, paused: true }), true);
  assert.equal(wanted("paused", { ...t, paused: true, on: false }), false);
});
