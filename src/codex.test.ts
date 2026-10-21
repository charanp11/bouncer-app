import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// Codex (PLAN, roadmap h1): its sessions and cards say Codex; its own reason
// is shown only as text, labelled as its words; Settings offers its hooks
// only where Codex is, and says to trust them once in Codex.
const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const body = (name: string) => main.slice(main.indexOf(`function ${name}(`), main.indexOf("\n}\n", main.indexOf(`function ${name}(`)));

test("sessions and cards name Codex", () => {
  assert.match(main, /const AGENTS: Record<string, string> = \{ "claude-code": "Claude Code", codex: "Codex" \};/);
  assert.match(main, /AGENTS\[request\.agent\] \?\? request\.agent/, "the card");
  assert.match(main, /AGENTS\[agent\] \?\? agent/, "the session rail");
});

test("Codex's reason is its own text, labelled, never markup", () => {
  const line = body("reasonLine");
  assert.match(line, /el\("span", "badge", `\$\{AGENTS\[request\.agent\] \?\? request\.agent\} says`\)/, "labelled with the agent's name");
  // As text only (no HTML sink anywhere is crates/app/tests/no_html_sinks.rs).
  assert.match(line, /agentText\(line, request\.reason/);
  assert.match(main, /if \(request\.reason\) c\.append\(reasonLine\(request\)\);/);
});

test("Codex's hooks: offered only where Codex is, previewed by agent, trusted once in Codex", () => {
  assert.match(main, /if \(about\?\.codexHooks && about\.codexHooks !== "absent"\) list\.append\(hooksRow\(about\.codexHooks, "codex"\)\);/);
  const row = body("hooksRow");
  assert.match(row, /invoke<HooksChange \| null>\("hooks_preview", \{ install, agent \}\)/);
  assert.match(row, /if \(codex && installing\) hooksNote \+= `\\n\$\{CODEX_TRUST\}`/);
  assert.match(main, /const CODEX_TRUST = "Codex asks you to trust these hooks once/);
  // The minimum version, the same as the README's.
  assert.match(main, /const CODEX_MIN = "Needs Codex 0\.145 or newer; older versions ignore these hooks\.";/);
  assert.match(row, /\$\{CODEX_MIN\}/);
  const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");
  assert.match(readme, /It needs Codex 0\.145 or newer/);
  assert.deepEqual([...row.matchAll(/invoke(?:<[^>]+>)?\("([a-z_]+)"/g)].map((m) => m[1]), ["hooks_write", "hooks_preview"]);
});
