import assert from "node:assert/strict";
import { test } from "node:test";
import { finished, greeting, mood } from "./mood.ts";

const view = (over: object = {}) => ({ paused: false, open: false, queue: [], sessions: [], ...over });
const card = { risk: null };
const risky = { risk: "Downloads a script and runs it" };

test("a card always wins over the cheer", () => {
  assert.equal(mood(view({ queue: [card] }), true), "needs");
  assert.equal(mood(view({ queue: [risky] }), true), "risky");
  assert.equal(mood(view({ sessions: [{ id: "a", status: "needs you" }] }), true), "needs");
});

test("moods in order", () => {
  assert.equal(mood(view({ paused: true, queue: [risky] }), true), "paused");
  assert.equal(mood(view({ sessions: [{ id: "a", status: "working" }] }), true), "done");
  assert.equal(mood(view({ sessions: [{ id: "a", status: "working" }] }), false), "working");
  assert.equal(mood(view(), false), "idle");
});

test("done is a working session going idle", () => {
  const before = new Map([["a", "working"], ["b", "idle"]]);
  assert.equal(finished(before, [{ id: "a", status: "idle" }]), true);
  assert.equal(finished(before, [{ id: "b", status: "idle" }]), false);
  assert.equal(finished(before, [{ id: "a", status: "needs you" }]), false);
  assert.equal(finished(new Map(), [{ id: "a", status: "idle" }]), false);
});

test("the greeting never shows over a card, paused or an open island", () => {
  assert.equal(greeting(view(), 1, 2), true);
  assert.equal(greeting(view(), 2, 2), false);
  assert.equal(greeting(view({ queue: [card] }), 1, 2), false);
  assert.equal(greeting(view({ paused: true }), 1, 2), false);
  assert.equal(greeting(view({ open: true }), 1, 2), false);
});
