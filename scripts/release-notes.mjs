// The draft release's notes, from the release workflow (PLAN, roadmap c-7).
//
//   node scripts/release-notes.mjs v0.7.0 out/SHA256SUMS.txt owner/repo > notes.md
//
// How to check the download, then: the first release (no earlier vX.Y.Z tag
// from 0.7.0 on; the old v0.5.x / v0.6.x tags don't count) gets "First public
// release" and what Bouncer does; every later one, the commits since the
// previous release tag.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

const [tag, sums, repo] = process.argv.slice(2);
const version = /^v(\d+\.\d+\.\d+)$/.exec(tag ?? "")?.[1];
if (!version || !sums || !repo) {
  console.error("usage: release-notes.mjs vX.Y.Z SHA256SUMS.txt owner/repo");
  process.exit(1);
}
const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
const release = (t) => {
  const v = /^v(\d+)\.(\d+)\.(\d+)$/.exec(t)?.slice(1).map(Number);
  return v && (v[0] > 0 || v[1] >= 7);
};
// The nearest earlier tag that is a release (0.7.0 or later).
const previous = git("tag", "--merged", `${tag}^`, "--sort=-version:refname", "--list", "v*.*.*").split("\n").find(release);

const exe = `Bouncer_${version}_x64-setup.exe`;
const notes = [
  `Bouncer ${version} for Windows (64-bit): a per-user install, no admin needed.`,
  "",
  `**Not signed yet.** Windows SmartScreen will say "Windows protected your PC": choose **More info → Run anyway**. Check the file first:`,
  "",
  "```",
  readFileSync(sums, "utf8").trim(),
  "```",
  "",
  `In PowerShell: \`Get-FileHash .\\${exe}\` must show the same SHA-256. With the GitHub CLI: \`gh attestation verify ${exe} --repo ${repo}\` proves it was built by this workflow from this tag.`,
  "",
  "Install, update and uninstall steps, and what Bouncer can't do: see the README.",
  "",
];
// What this release needs said first (docs/releases/vX.Y.Z.md), if anything.
const intro = new URL(`../docs/releases/${tag}.md`, import.meta.url);
if (existsSync(intro)) notes.push(readFileSync(intro, "utf8").trim(), "");
if (previous) {
  notes.push(`## Changes since ${previous}`, "", git("log", "--no-merges", "--format=- %s", `${previous}..${tag}`));
} else {
  notes.push(
    "## First public release",
    "",
    "Bouncer is the security guard for Claude Code on your desktop:",
    "",
    "- Your Claude Code sessions, live, in a small island at the top of the screen.",
    "- Allow or Deny Claude Code's permission requests from a card (Allow arms after 600 ms; Enter never approves).",
    "- Auto-allow what your rules allow; risky requests are flagged with a plain reason and never auto-allowed.",
    "- Explain this command: Claude says what a shell command does and what could go wrong. It can be wrong; you decide.",
    "- A read-only chat that runs your own Claude Code.",
    "- A local activity log with secrets redacted, and a summary when you come back.",
    "- Fail-safe: if Bouncer is closed or stuck, Claude Code asks in its own prompt as usual.",
    "- Settings: install the hooks, Start with Windows (off by default), Ctrl+Alt+B to jump to the island.",
  );
}
console.log(notes.join("\n"));
