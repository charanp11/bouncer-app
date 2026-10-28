import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// Copilot CLI (PLAN, roadmap h2): Bouncer never allows for Copilot. Its card
// says Deny or "Let Copilot decide"; Settings and the README say plainly
// what it gets, and the minimum version (Charan, 2026-10-28, condition 2).
const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");

test("a Copilot card never says Allow", () => {
  assert.match(main, /const LET_COPILOT = "Let Copilot decide";/);
  assert.match(main, /answerButtons\(request, request\.agent === "copilot" \? LET_COPILOT : "Allow once"\)/);
  assert.match(main, /copilot \? "Left to Copilot: its own rules decide\." : "Allowed\. Back to work\."/);
});

test("Copilot's hooks: offered only where Copilot is, and say what it gets", () => {
  assert.match(main, /if \(about\?\.copilotHooks && about\.copilotHooks !== "absent"\) list\.append\(hooksRow\(about\.copilotHooks, "copilot"\)\);/);
  assert.match(main, /copilot: \{ name: "Copilot CLI hooks", [^}]*hint: COPILOT_HINT, min: COPILOT_MIN,/);
  const hint = /installed:\s*"(Watch-only[^"]+)"/.exec(main)?.[1] ?? "";
  for (const part of ["Watch-only", "Deny on risky commands", "never allows", "sandbox"]) {
    assert.ok(hint.includes(part), part);
  }
  assert.match(main, /const COPILOT_MIN = "Needs Copilot CLI 1\.0\.67 or newer\.";/);
});

test("the README says the same", () => {
  for (const part of ["Copilot CLI 1.0.67 or newer", "watch-only", "Deny", "never allows", "sandbox"]) {
    assert.ok(readme.includes(part), part);
  }
});
