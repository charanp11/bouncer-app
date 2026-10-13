import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// Releases: only a vX.Y.Z tag from 0.7.0 on, the version checked everywhere,
// a draft, and only the release job may write (PLAN, roadmap c-6).
const root = new URL("../", import.meta.url);
const read = (path: string) => readFileSync(new URL(path, root), "utf8");
const version = (...args: string[]) =>
  spawnSync(process.execPath, ["scripts/version.mjs", ...args], { cwd: root, encoding: "utf8" });

test("only a vX.Y.Z tag from 0.7.0 on is a release", () => {
  for (const [tag, out] of [["v0.7.0", "0.7.0"], ["v0.10.0", "0.10.0"], ["v1.0.0", "1.0.0"]]) {
    const r = version("tag", tag);
    assert.equal(r.status, 0, tag);
    assert.equal(r.stdout.trim(), out);
  }
  for (const tag of ["v0.6.4", "v0.5.9", "v0.7", "v0.7.0-rc1", "0.7.0", "v0.7.0 ", "release"]) {
    assert.notEqual(version("tag", tag).status, 0, tag);
  }
});

test("the version check fails when any place disagrees", () => {
  // The repo isn't stamped (0.0.0, no version in tauri.conf.json): any version fails.
  const r = version("check", "9.9.9");
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /not 9\.9\.9/);
});

test("the release workflow: tags only, a draft, and only its release job writes", () => {
  const wf = read(".github/workflows/release.yml");
  const lines = wf.split("\n").filter((l) => !l.trimStart().startsWith("#"));
  const live = lines.join("\n");
  assert.match(live, /^on:\n {2}push:\n {4}tags: \["v\*\.\*\.\*"\]\n\n/m, "triggered only by pushed tags");
  assert.doesNotMatch(live, /pull_request|workflow_dispatch|branches:/);
  assert.match(live, /^permissions: \{\}$/m, "nothing by default");
  // Split into jobs; only `release` may hold a write permission.
  const jobs = live.split(/\n {2}(?=[a-z-]+:\n)/).slice(1);
  const writers = jobs.filter((j) => /: write/.test(j)).map((j) => j.split(":")[0]);
  assert.deepEqual(writers, ["release"]);
  const release = jobs.find((j) => j.startsWith("release:"))!;
  for (const p of ["contents: write", "id-token: write", "attestations: write"]) assert.ok(release.includes(p), p);
  assert.match(release, /gh release create "\$TAG" --draft --verify-tag/);
  assert.match(wf, /# - name: Sign the installer/, "the signing slot is there, commented");
});

test("every action in the workflows is ours or pinned to a commit", () => {
  for (const f of ["release.yml", "installer.yml", "ci.yml", "macos-smoke.yml", "advisories.yml"]) {
    for (const line of read(`.github/workflows/${f}`).split("\n")) {
      const m = /^\s*(?:- )?uses: (\S+)/.exec(line);
      if (!m) continue;
      assert.ok(m[1].startsWith("./") || /@[0-9a-f]{40}$/.test(m[1]), `${f}: ${m[1]}`);
    }
  }
  assert.doesNotMatch(read(".github/workflows/installer.yml"), /: write/);
});
