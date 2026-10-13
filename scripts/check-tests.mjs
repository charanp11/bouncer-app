// The regression gate's own check: every test docs/TESTS.md names must exist,
// so the table can't drift from the code. Run after `cargo test` (CI does);
// it lists the Rust tests with `cargo test -- --list` and reads the frontend
// test titles from src/*.test.ts. Names marked `windows:` / `macos:` are only
// checked on that OS; quoted paths (a CI job's workflow or script) must exist.
// Exits 1 on any problem.
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";

const os = process.platform === "win32" ? "windows" : process.platform === "darwin" ? "macos" : "other";

const listed = execFileSync("cargo", ["test", "--locked", "--quiet", "--", "--list"], { encoding: "utf8", maxBuffer: 1 << 26 });
const rust = new Set(listed.split("\n").filter((l) => l.endsWith(": test")).map((l) => l.slice(0, -": test".length)));

const frontend = new Set();
for (const file of readdirSync("src").filter((f) => f.endsWith(".test.ts"))) {
  for (const m of readFileSync(`src/${file}`, "utf8").matchAll(/^test\("((?:[^"\\]|\\.)*)"/gm)) {
    frontend.add(JSON.parse(`"${m[1]}"`));
  }
}

const problems = [];
const ids = new Set();
let rows = 0, automated = 0, realClick = 0, hand = 0, named = 0;
for (const line of readFileSync("docs/TESTS.md", "utf8").split("\n")) {
  const cells = line.split("|").slice(1, -1).map((c) => c.trim());
  if (cells.length !== 6 || !/^[A-Z]+\d+$/.test(cells[0])) continue;
  const [id, , , proof, tests, status] = cells;
  rows++;
  if (ids.has(id)) problems.push(`${id}: duplicate ID`);
  ids.add(id);
  if (/\bCI\b/.test(status)) automated++;
  if (/real-click/.test(proof)) realClick++;
  if (/^Hand|; Hand|Phase 9/.test(status)) hand++;
  const quoted = [...tests.matchAll(/`([^`]+)`/g)].map((m) => m[1]);
  // A CI job's own files (its workflow, its script) can be the proof: they
  // must exist. A frontend title may hold a "/" too ("100 / 112 / 125%").
  const isFile = (n) => n.includes("/") && !/^(?:(?:windows|macos): )?ts: /.test(n);
  const names = quoted.filter((n) => !isFile(n));
  const files = quoted.filter(isFile);
  for (const f of files) if (!existsSync(f)) problems.push(`${id}: no such file: ${f}`);
  if (/\bCI\b/.test(status) && !names.length && !files.length && id !== "M1") problems.push(`${id}: automated but names no test`);
  for (const raw of names) {
    const [, only, name] = raw.match(/^(?:(windows|macos): )?(.*)$/);
    if (only && only !== os) continue;
    named++;
    const found = name.startsWith("ts: ") ? frontend.has(name.slice(4)) : rust.has(name);
    if (!found) problems.push(`${id}: no such test: ${raw}`);
  }
}
console.log(`docs/TESTS.md: ${rows} scenarios, ${automated} automated (CI), ${realClick} with real-click checks, ${hand} hand / Phase 9; ${named} test names checked on ${os} (${rust.size} Rust, ${frontend.size} frontend tests exist)`);
for (const p of problems) console.log(`  ${p}`);
process.exit(problems.length ? 1 : 0);
