import assert from "node:assert/strict";
import { test } from "node:test";
import { group, latest, title } from "./steps.ts";

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
