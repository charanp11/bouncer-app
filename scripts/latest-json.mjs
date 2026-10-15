// The updater's latest.json for a release (PLAN, "Updater"), from the release
// workflow:
//
//   node scripts/latest-json.mjs 0.8.0 owner/repo out/Bouncer_0.8.0_x64-setup.exe.sig > out/latest.json
//
// Installed copies read it from the latest published release only (GitHub's
// "latest" never points at a draft or a prerelease), check the version is
// newer, and check the installer against the signature before running it.
import { readFileSync } from "node:fs";

const [version, repo, sigFile] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "") || !/^[\w.-]+\/[\w.-]+$/.test(repo ?? "") || !sigFile) {
  console.error("usage: latest-json.mjs X.Y.Z owner/repo installer.exe.sig");
  process.exit(1);
}
const signature = readFileSync(sigFile, "utf8").trim();
// A minisign signature file, base64: "untrusted comment: ..." first.
if (!Buffer.from(signature, "base64").toString("utf8").startsWith("untrusted comment:")) {
  console.error(`${sigFile} is not a minisign signature`);
  process.exit(1);
}
const exe = `Bouncer_${version}_x64-setup.exe`;
console.log(
  JSON.stringify(
    {
      version,
      notes: `Bouncer ${version}: see https://github.com/${repo}/releases/tag/v${version}`,
      pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
      platforms: {
        "windows-x86_64": { signature, url: `https://github.com/${repo}/releases/download/v${version}/${exe}` },
      },
    },
    null,
    2,
  ),
);
