import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { explainStart } from "./chat.ts";
import { answerApply, answerRefused, CAN_BE_WRONG, hoverText, OWN_CHECKS, sizeText, stillAttached, verdict, type DropReport } from "./drop.ts";

const report = (over: Partial<DropReport> = {}): DropReport => ({
  state: "report",
  id: "a".repeat(32),
  name: "run.ps1",
  kind: "A PowerShell script",
  size: 2048,
  partial: false,
  risky: false,
  findings: [],
  sha256: null,
  ...over,
});

test("drop: the answer streams for its own drop only, and stops in words", () => {
  const id = "d1";
  let x = answerApply(explainStart(id), { drop: id, kind: "text", text: "<b>It downloads</b>" });
  x = answerApply(x, { drop: "other", kind: "text", text: "stale" });
  x = answerApply(x, { drop: id, kind: "text", text: " a script." });
  assert.deepEqual(x, { id, text: "<b>It downloads</b> a script.", busy: true, error: null });
  assert.deepEqual(answerApply(x, { drop: id, kind: "done", error: null }), { ...x, busy: false });
  assert.equal(answerApply(x, { drop: id, kind: "stopped", stop: { why: "cancelled" } })?.error, "Stopped.");
  assert.equal(answerApply(null, { drop: id, kind: "text", text: "x" }), null);
});

test("drop: a refused start says why; a file no longer held asks for the drop again", () => {
  assert.equal(answerRefused(explainStart("d"), "d", "gone")?.error, "Bouncer no longer holds that file. Drop it again.");
  assert.equal(answerRefused(explainStart("d"), "d", "unconfirmed")?.error, "Confirm the claude path in Settings first.");
  assert.equal(answerRefused(explainStart("d"), "e", "gone")?.error, null, "another drop's refusal changes nothing");
});

test("drop: Bouncer's report never says safe and always says it can be wrong", () => {
  assert.equal(verdict(report({ risky: true })), "Looks risky. Read why below.");
  assert.equal(verdict(report()), "Nothing risky found.");
  for (const text of [verdict(report()), verdict(report({ risky: true })), OWN_CHECKS, CAN_BE_WRONG]) {
    assert.doesNotMatch(text, /\bsafe\b/i);
  }
  assert.match(OWN_CHECKS, /can be wrong · you decide/);
  assert.equal(hoverText("one"), "Drop to check it");
  assert.equal(hoverText("many"), "One file at a time");
});

test("drop: sizes read as the backend's", () => {
  assert.equal(sizeText(0), "0 bytes");
  assert.equal(sizeText(1023), "1023 bytes");
  assert.equal(sizeText(1536), "1.5 KB");
  assert.equal(sizeText(34 * 1024 * 1024), "34 MB");
  assert.equal(sizeText(8 * 1024 ** 3), "8.0 GB");
});

test("drop: an attachment lasts only while its drop is the one shown", () => {
  const r = report();
  const a = { id: r.id, name: r.name, size: r.size, preview: null };
  assert.equal(stillAttached(a, r), a);
  assert.equal(stillAttached(a, report({ id: "b".repeat(32) })), null, "a new drop");
  assert.equal(stillAttached(a, null), null, "closed or expired");
  assert.equal(stillAttached(a, { state: "checking", name: "x" }), null);
  assert.equal(stillAttached(null, r), null);
});

test("drop: the page never reads or sends a path, and sends only the drop's ID", () => {
  const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  for (const call of main.matchAll(/invoke(?:<[^>]*>)?\("drop_(\w+)"(?:, \{([^}]*)\})?/g)) {
    const args = (call[2] ?? "").split(",").map((a) => a.split(":")[0].trim()).filter(Boolean);
    for (const arg of args) assert.ok(["id", "ask", "model"].includes(arg), `drop_${call[1]} sends ${arg}`);
  }
  assert.doesNotMatch(main, /tauri:\/\/drag|onDragDropEvent|dataTransfer/);
});
