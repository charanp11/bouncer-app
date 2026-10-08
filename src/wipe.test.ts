import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// Wiping history can't be undone, so it has exactly one path: Settings'
// confirm, whose "Delete history" is armed. The tray only opens that confirm.
const page = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

test("the page wipes only from the armed Delete history after its confirm", () => {
  const calls = [...page.matchAll(/invoke\("wipe"\)/g)];
  assert.equal(calls.length, 1, "one wipe call");
  const before = page.slice(0, calls[0].index);
  const del = before.lastIndexOf('const del = armed("Delete history", wipeSince);');
  assert.ok(del > before.lastIndexOf("function "), "inside the confirm step, on the armed button");
  assert.ok(before.lastIndexOf('del.addEventListener("click"') > del, "on its click");
  assert.match(page.slice(del - 400, del), /Delete all activity history\? [\s\S]*Cancel/);
});

test("the tray's Wipe history… opens that confirm, never wipes", () => {
  const start = page.indexOf("if (view.confirmWipe && !wiping) {");
  assert.ok(start > 0, "the page turns the tray's request into the confirm");
  const block = page.slice(start, page.indexOf("}", start));
  assert.match(block, /wiping = true;/);
  assert.match(block, /wipeSince = performance\.now\(\);/);
  assert.doesNotMatch(block, /invoke/);
});
