import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { updateButton } from "./update.ts";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

test("update: Update waits while you're answering, and while it installs", () => {
  assert.deepEqual(updateButton({ version: "0.8.1", installing: false }, false), { label: "Update", enabled: true });
  assert.deepEqual(updateButton({ version: "0.8.1", installing: false }, true), { label: "Update after you answer", enabled: false });
  assert.deepEqual(updateButton({ version: "0.8.1", installing: true }, false), { label: "Updating…", enabled: false });
});

test("update: the page only asks to install (no arguments) and flips the switch; the button arms like Allow", () => {
  const calls = [...main.matchAll(/invoke\("(update_install|set_updates)"([^)]*)\)/g)].map((m) => `${m[1]}${m[2]}`);
  assert.deepEqual(calls, ["set_updates, { on: !on }", "update_install"]);
  assert.match(main, /const button = armed\(state\.label, updateSince\);/);
  assert.match(main, /const busy = view\.queue\.length > 0 \|\| chat\.busy;/);
  // The version is agent-free but still text only.
  assert.match(main, /el\("b", "", `Bouncer \$\{update\.version\}`\)/);
});

test("update: Settings says what the check does, word for word as the prototype", () => {
  const proto = readFileSync(new URL("../design/prototype/bouncer-island.html", import.meta.url), "utf8");
  const hint = /setRow\("Check for updates", \[[^\n]*\],\n {4}"([^"]+)"/.exec(proto)?.[1];
  assert.ok(hint, "prototype hint");
  assert.ok(main.includes(JSON.stringify(hint)), "same hint in the page");
});
