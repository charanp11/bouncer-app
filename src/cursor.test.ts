import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// Cursor CLI (PLAN, roadmap h3): Deny or nothing, like Copilot (Charan,
// 2026-10-29). Its card says Deny or "Let Cursor decide"; Settings and the
// README say plainly what it gets and the version tested.
const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");

test("a Cursor card never says Allow", () => {
  assert.match(main, /const AGENTS: Record<string, string> = \{[^}]*cursor: "Cursor" \};/);
  assert.match(main, /const LET_DECIDE: Record<string, string> = \{[^}]*cursor: "Let Cursor decide" \};/);
  assert.match(main, /answerButtons\(request, LET_DECIDE\[request\.agent\] \?\? "Allow once"\)/);
});

test("Cursor's hooks: offered only where Cursor is, and say what it gets", () => {
  assert.match(main, /if \(about\?\.cursorHooks && about\.cursorHooks !== "absent"\) list\.append\(hooksRow\(about\.cursorHooks, "cursor"\)\);/);
  assert.match(main, /cursor: \{ name: "Cursor CLI hooks", [^}]*hint: CURSOR_HINT, min: CURSOR_MIN,/);
  const hint = /const CURSOR_HINT[^}]*installed:\s*"([^"]+)"/s.exec(main)?.[1] ?? "";
  for (const part of ["Watch-only", "Deny on risky shell commands", "never allows", "Your own Cursor hooks stay"]) {
    assert.ok(hint.includes(part), part);
  }
  assert.match(main, /const CURSOR_MIN = "Tested with Cursor CLI 2026\.10\.22\.";/);
});

test("the README says the same", () => {
  for (const part of ["Cursor CLI", "Let Cursor decide", "never allows for Cursor", "~/.cursor/hooks.json", "2026.10.22"]) {
    assert.ok(readme.includes(part), part);
  }
});
