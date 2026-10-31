// The updater's latest.json for a release (PLAN, "Updater"), from the release
// workflow:
//
//   node scripts/latest-json.mjs 0.8.0 owner/repo Bouncer_0.8.0_x64-setup.exe.sig Bouncer_0.8.0_universal.app.tar.gz.sig > out/latest.json
//
// Installed copies read it from the latest published release only (GitHub's
// "latest" never points at a draft or a prerelease), check the version is
// newer, and check the installer against the signature before running it.
// Without the second signature it's a Windows-only release (PLAN, "Roadmap g"):
// check-updater-target.mjs stops that unless told --windows-only.
import { readFileSync } from "node:fs";

const [version, repo, sigFile, macSigFile] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "") || !/^[\w.-]+\/[\w.-]+$/.test(repo ?? "") || !sigFile) {
  console.error("usage: latest-json.mjs X.Y.Z owner/repo installer.exe.sig [app.tar.gz.sig]");
  process.exit(1);
}
// Minisign files, base64: an "untrusted comment:" line, then base64 of the
// algorithm (2 bytes) and the key ID (8 bytes) and the rest.
const keyId = (b64, what) => {
  const [comment, line] = Buffer.from(b64, "base64").toString("utf8").split("\n");
  const id = comment.startsWith("untrusted comment:") && line ? Buffer.from(line, "base64").subarray(2, 10) : null;
  if (id?.length !== 8) {
    console.error(`${what} is not a minisign file`);
    process.exit(1);
  }
  return id.toString("hex");
};
// Signed with the key installed copies trust (tauri.conf.json), or nothing:
// a wrong or rotated secret must not ship an update they would refuse.
const conf = JSON.parse(readFileSync(new URL("../crates/app/tauri.conf.json", import.meta.url), "utf8"));
const signed = (file) => {
  const signature = readFileSync(file, "utf8").trim();
  if (keyId(signature, file) !== keyId(conf.plugins.updater.pubkey, "the public key")) {
    console.error(`${file} was not made with the updater key in tauri.conf.json`);
    process.exit(1);
  }
  return signature;
};
const entry = (file, sig) => ({ signature: signed(sig), url: `https://github.com/${repo}/releases/download/v${version}/${file}` });
const windows = { "windows-x86_64": entry(`Bouncer_${version}_x64-setup.exe`, sigFile) };
// One universal app for both Mac architectures.
const app = macSigFile && entry(`Bouncer_${version}_universal.app.tar.gz`, macSigFile);
console.log(
  JSON.stringify(
    {
      version,
      notes: `Bouncer ${version}: see https://github.com/${repo}/releases/tag/v${version}`,
      pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
      platforms: app ? { ...windows, "darwin-aarch64": app, "darwin-x86_64": app } : windows,
    },
    null,
    2,
  ),
);
