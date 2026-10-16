import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { ready, toggle } from "./questions.ts";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

test("the page sends only the card's ID and option numbers, or approve / keep", () => {
  const sends = [...main.matchAll(/invoke\("(answer|plan)", ([^)]*)\)/g)].map((m) => `${m[1]} ${m[2]}`);
  assert.deepEqual(sends, [
    "answer { id: request.id, picks }",
    "plan { id: request.id, approve: false }",
    "plan { id: request.id, approve: true }",
  ]);
  // The picks are copied from the numbers picked, nothing else.
  assert.match(main, /const picks = picked\.chosen\.map\(\(c\) => \[\.\.\.c\]\);/);
});

test("Send answer and Approve plan arm like Allow (so Enter never reaches them); a pick never sends", () => {
  assert.match(main, /const send = armed\("Send answer", front\.since, ready\(picked\.chosen, qs\.length\)\);/);
  assert.match(main, /const approve = armed\("Approve plan", front\.since\);/);
  const pick = main.slice(main.indexOf("// A pick only selects"), main.indexOf("opts.append(b);"));
  assert.doesNotMatch(pick, /invoke\(/);
  // Question, option and plan text go in as agent text (red tags for hidden characters).
  for (const part of ["agentText(h, q.header)", "agentText(text, q.question)", "agentText(label, o.label)", "agentText(d, o.description)", "answerInto(box, plan.text)", "agentText(file, plan.path)"]) {
    assert.ok(main.includes(part), part);
  }
});

test("one pick for a single-select question; a second press clears it", () => {
  assert.deepEqual(toggle([], 1, false), [1]);
  assert.deepEqual(toggle([1], 0, false), [0]);
  assert.deepEqual(toggle([1], 1, false), []);
});

test("a multi-select question adds and removes picks, kept sorted", () => {
  assert.deepEqual(toggle([], 2, true), [2]);
  assert.deepEqual(toggle([2], 0, true), [0, 2]);
  assert.deepEqual(toggle([0, 2], 2, true), [0]);
});

test("an answer is ready only when every question has a pick", () => {
  assert.equal(ready([[1], [0, 2]], 2), true);
  assert.equal(ready([[1], []], 2), false);
  assert.equal(ready([[1]], 2), false);
  assert.equal(ready([], 0), true);
});
