import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// Settings hints are never cut (Charan, 2026-10-21: v0.10.0's Codex hooks
// hint ended "Needs Codex 0.14…"): every hint wraps, in the app and in the
// prototype alike, with no per-row opt-in to forget.
const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const prototype = readFileSync(new URL("../design/prototype/bouncer-island.html", import.meta.url), "utf8");

const rule = (text: string) => /\.setrow \.hint \{([^}]*)\}/.exec(text)?.[1] ?? "";

test("settings: every hint wraps and is never cut, as the prototype", () => {
  for (const [where, text] of [["styles.css", css], ["prototype", prototype]]) {
    const body = rule(text);
    assert.ok(body, `${where} styles .setrow .hint`);
    for (const cut of ["nowrap", "ellipsis", "overflow: hidden", "-webkit-line-clamp"]) {
      assert.ok(!body.includes(cut), `${where}: .setrow .hint has ${cut}`);
    }
    assert.match(body, /overflow-wrap: anywhere/, `${where}: a long word or path breaks too`);
    assert.doesNotMatch(text, /\.hint\.wrap/, `${where}: no opt-in class left`);
  }
  assert.doesNotMatch(main, /classList\.add\("wrap"\)/);
  // The hint goes in whole, as text.
  assert.match(main, /if \(hint\) row\.append\(el\("div", "hint", hint\)\);/);
});
