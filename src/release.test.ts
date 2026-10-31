import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

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
  assert.match(release, /node scripts\/release-notes\.mjs "\$TAG" out\/SHA256SUMS\.txt/, "notes from the script");
  assert.match(wf, /# - name: Sign the installer/, "the signing slot is there, commented");
});

test("release notes: the first release lists features, later ones the changes since the last release", () => {
  const repo = mkdtempSync(join(tmpdir(), "bouncer-notes-"));
  try {
    const git = (...args: string[]) => {
      const r = spawnSync("git", args, { cwd: repo, encoding: "utf8" });
      assert.equal(r.status, 0, r.stderr);
    };
    git("init", "-q");
    const commit = (msg: string, tag?: string) => {
      git("-c", "user.name=dev", "-c", "user.email=dev@example.com", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", msg);
      if (tag) git("tag", tag);
    };
    writeFileSync(join(repo, "SHA256SUMS.txt"), "abc123  Bouncer_x_x64-setup.exe\n");
    const notes = (tag: string) => {
      const r = spawnSync(
        process.execPath,
        [fileURLToPath(new URL("scripts/release-notes.mjs", root)), tag, "SHA256SUMS.txt", "dev/bouncer"],
        { cwd: repo, encoding: "utf8" },
      );
      assert.equal(r.status, 0, r.stderr);
      return r.stdout;
    };
    commit("feat: old work", "v0.6.4");
    commit("feat: the installer", "v0.7.0");
    const first = notes("v0.7.0");
    assert.match(first, /^## First public release$/m, "v0.6.4 is not a release");
    assert.doesNotMatch(first, /## Changes|old work|the installer/);
    assert.match(first, /abc123 {2}Bouncer_x_x64-setup\.exe/);
    assert.match(first, /gh attestation verify Bouncer_0\.7\.0_x64-setup\.exe --repo dev\/bouncer/);
    commit("fix: a card");
    commit("docs: the README", "v0.7.1");
    const next = notes("v0.7.1");
    assert.match(next, /^## Changes since v0\.7\.0\n\n- docs: the README\n- fix: a card$/m);
    assert.doesNotMatch(next, /First public release|the installer/);
    // A tag that was never released (v0.8.0) is skipped: v0.8.1 lists
    // everything since the last real release.
    commit("feat: the updater", "v0.8.0");
    commit("fix: the freeze", "v0.8.1");
    const after = notes("v0.8.1");
    assert.match(after, /^## Changes since v0\.7\.1\n\n- fix: the freeze\n- feat: the updater$/m);
  } finally {
    rmSync(repo, { recursive: true, force: true });
  }
});

test("updater: the key reaches only the step that signs the finished installer; the draft carries latest.json", () => {
  const wf = read(".github/workflows/release.yml");
  const live = wf.split("\n").filter((l) => !l.trimStart().startsWith("#")).join("\n");
  // Each secret appears exactly once, in the step that builds the installer.
  for (const s of ["TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"]) {
    assert.equal(live.split(`secrets.${s} }}`).length - 1, 1, s);
  }
  const steps = live.split(/\n {6}- /);
  const keyed = steps.filter((s) => s.includes("secrets."));
  assert.deepEqual(keyed.map((s) => /name: (.+)/.exec(s)?.[1]), ["Sign for the updater"]);
  // That step runs one command: the pinned Tauri CLI's signer, on the built installer.
  assert.match(keyed[0], /\n {8}run: node node_modules\/@tauri-apps\/cli\/tauri\.js signer sign "target\/release\/bundle\/nsis\/Bouncer_\$\{VERSION\}_x64-setup\.exe"$/);
  // Signing comes after building, and the build step has no secrets and never signs.
  const names = steps.map((s) => /name: (.+)/.exec(s)?.[1]);
  assert.ok(names.indexOf("Sign for the updater") === names.indexOf("Build the installer") + 1);
  const build = steps.find((s) => s.includes("name: Build the installer"))!;
  assert.doesNotMatch(build, /secrets\.|TAURI_SIGNING|--updater|createUpdaterArtifacts/);
  assert.match(live, /test -s "Bouncer_\$\{VERSION\}_x64-setup\.exe\.sig"/);
  assert.match(live, /node scripts\/latest-json\.mjs "\$VERSION" "\$GITHUB_REPOSITORY" "out\/Bouncer_\$\{VERSION\}_x64-setup\.exe\.sig" > out\/latest\.json/);
  assert.match(live, /gh release create "\$TAG" --draft --verify-tag[\s\S]*out\/SHA256SUMS\.txt out\/latest\.json/);
  // Only the release workflow signs: the installer test and CI build without the key.
  for (const f of ["installer.yml", "ci.yml"]) assert.doesNotMatch(read(`.github/workflows/${f}`), /TAURI_SIGNING|--updater/, f);
  assert.doesNotMatch(read("scripts/bundle.mjs"), /createUpdaterArtifacts|TAURI_SIGNING|signer/);
});

test("updater: latest.json points at this release's installer with its signature", () => {
  const dir = mkdtempSync(join(tmpdir(), "bouncer-latest-"));
  try {
    // A signature file carrying the key ID of the public key in tauri.conf.json.
    const conf = JSON.parse(read("crates/app/tauri.conf.json"));
    const pubLine = Buffer.from(conf.plugins.updater.pubkey, "base64").toString("utf8").split("\n")[1];
    const ourId = Buffer.from(pubLine, "base64").subarray(2, 10);
    const sigFile = (id: Buffer) =>
      Buffer.from(
        `untrusted comment: signature from tauri secret key\n${Buffer.concat([Buffer.from("ED"), id, Buffer.alloc(64)]).toString("base64")}\ntrusted comment: x\nabc\n`,
      ).toString("base64");
    const sig = sigFile(ourId);
    writeFileSync(join(dir, "a.sig"), sig + "\n");
    // Another key's signature (a wrong or rotated secret): refused.
    writeFileSync(join(dir, "other.sig"), sigFile(Buffer.from("0123456789abcdef", "hex")));
    const run = (...args: string[]) =>
      spawnSync(process.execPath, [fileURLToPath(new URL("scripts/latest-json.mjs", root)), ...args], { cwd: dir, encoding: "utf8" });
    const r = run("0.8.0", "dev/bouncer", "a.sig");
    assert.equal(r.status, 0, r.stderr);
    const json = JSON.parse(r.stdout);
    assert.equal(json.version, "0.8.0");
    assert.match(json.pub_date, /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/);
    assert.deepEqual(json.platforms, {
      "windows-x86_64": { signature: sig, url: "https://github.com/dev/bouncer/releases/download/v0.8.0/Bouncer_0.8.0_x64-setup.exe" },
    });
    writeFileSync(join(dir, "bad.sig"), "not a signature");
    assert.notEqual(run("0.8.0", "dev/bouncer", "bad.sig").status, 0);
    const other = run("0.8.0", "dev/bouncer", "other.sig");
    assert.notEqual(other.status, 0);
    assert.match(other.stderr, /not made with the updater key/);
    for (const v of ["v0.8.0", "0.8", "0.8.0-rc1", ""]) assert.notEqual(run(v, "dev/bouncer", "a.sig").status, 0, v);
    assert.notEqual(run("0.8.0", "dev/bouncer/x", "a.sig").status, 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("updater: a platform key the pinned plugin doesn't look up stops the release", () => {
  const dir = mkdtempSync(join(tmpdir(), "bouncer-target-"));
  try {
    const run = (...args: string[]) =>
      spawnSync(process.execPath, [fileURLToPath(new URL("scripts/check-updater-target.mjs", root)), ...args], {
        cwd: root,
        encoding: "utf8",
      });
    const file = (name: string, platforms: object) => {
      const path = join(dir, name);
      writeFileSync(path, JSON.stringify({ version: "0.8.0", platforms }));
      return path;
    };
    const good = file("good.json", { "windows-x86_64": {} });
    // The plugin Cargo.lock pins, read from its own source.
    assert.equal(run(good).status, 0);
    const real = run(good, "pinned");
    assert.equal(real.status, 0, real.stderr);
    for (const platforms of [{ "windows-x64": {} }, { "windows-x86_64": {}, "darwin-aarch64": {} }, {}]) {
      const r = run(file("bad.json", platforms));
      assert.notEqual(r.status, 0);
      assert.match(r.stderr, /the plugin looks up windows-x86_64/);
    }
    // A plugin that builds its key another way: stopped, not guessed at.
    const other = join(dir, "updater.rs");
    writeFileSync(other, 'Some("windows") Some("x86_64") targets.push(format!("{arch}-{os}"));');
    const changed = run(good, other);
    assert.notEqual(changed.status, 0);
    assert.match(changed.stderr, /changed its lookup/);
    assert.ok(read(".github/workflows/release.yml").includes("node scripts/check-updater-target.mjs out/latest.json"));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("v0.8.1's notes: the first release with these features, 0.8.0 never released, 0.7.0 users install it by hand once", () => {
  const intro = read("docs/releases/v0.8.1.md");
  assert.match(intro, /^\*\*0\.8\.1 is the first release with these features\.\*\* 0\.8\.0 was never released/);
  assert.match(intro, /install 0\.8\.1 by hand once/);
  assert.doesNotMatch(intro, /0\.8\.0 by hand/);
  assert.match(intro, /Check for updates/);
  assert.match(intro, /Watch → Custom → Releases/);
  assert.match(intro, /^\*\*Known limits:\*\* .*claude -p.*refuses risky requests.*accept edits/m);
  assert.match(read("scripts/release-notes.mjs"), /docs\/releases\/\$\{tag\}\.md/);
});

test("installer test: the WebView2 runtime is read before the first launch, and the 30 s limit stays", () => {
  const script = read("scripts/installer-test.ps1");
  const body = script.slice(script.indexOf("# 3. A card"));
  assert.ok(body.indexOf("ReadAllBytes") !== -1 && body.indexOf("ReadAllBytes") < body.indexOf("$app = Start-App"));
  assert.match(script, /\$pipe = Wait-Until \{[^\n]*\} 30\b/);
});

test("cold start: its own workflow, nothing warmed, and no release or required check waits for it", () => {
  const wf = read(".github/workflows/cold-start.yml");
  assert.match(wf, /scripts\/cold-start\.ps1/);
  assert.doesNotMatch(wf, /workflow_call|installer-test|: write/);
  assert.doesNotMatch(read("scripts/cold-start.ps1"), /ReadAllBytes/);
  assert.doesNotMatch(read(".github/workflows/release.yml"), /cold-start/);
});

test("cold start: under 30 s is ok, over 30 s warns, only over 120 s fails", { skip: process.platform !== "win32" }, () => {
  const judge = (seconds: number) =>
    spawnSync(
      "powershell",
      ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("scripts/cold-start.ps1", root)), "-Seconds", `${seconds}`],
      { encoding: "utf8", env: { ...process.env, GITHUB_STEP_SUMMARY: "" } },
    );
  const ok = judge(29);
  assert.equal(ok.status, 0, ok.stderr);
  assert.match(ok.stdout, /, ok \(image/);
  assert.doesNotMatch(ok.stdout, /::warning::/);
  const slow = judge(31);
  assert.equal(slow.status, 0, slow.stderr);
  assert.match(slow.stdout, /::warning::.*WARN: over 30 s/);
  const edge = judge(120);
  assert.equal(edge.status, 0, edge.stderr);
  assert.match(edge.stdout, /WARN: over 30 s/);
  const fail = judge(121);
  assert.equal(fail.status, 1, fail.stderr);
  assert.match(fail.stdout, /::warning::.*FAIL: no pipe within 120 s/);
});

test("every action in the workflows is ours or pinned to a commit", () => {
  for (const f of ["release.yml", "installer.yml", "cold-start.yml", "ci.yml", "macos-smoke.yml", "advisories.yml"]) {
    for (const line of read(`.github/workflows/${f}`).split("\n")) {
      const m = /^\s*(?:- )?uses: (\S+)/.exec(line);
      if (!m) continue;
      assert.ok(m[1].startsWith("./") || /@[0-9a-f]{40}$/.test(m[1]), `${f}: ${m[1]}`);
    }
  }
  assert.doesNotMatch(read(".github/workflows/installer.yml"), /: write/);
});
