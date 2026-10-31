// The version, from a release tag, into every place that has one; and a check
// that they all agree. Plain X.Y.Z only (the installer needs numbers).
//
//   node scripts/version.mjs tag v0.7.0        prints 0.7.0, or fails: not vX.Y.Z,
//                                              or older than 0.7.0 (the first release)
//   node scripts/version.mjs set 0.7.0         Cargo.toml (workspace), Cargo.lock,
//                                              package.json, tauri.conf.json
//   node scripts/version.mjs check 0.7.0 [dir] fails unless the crates, package.json,
//                                              tauri.conf.json and the installer in dir
//                                              (the .dmg on a Mac) agree
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";

const FIRST = [0, 7, 0];
const CRATES = ["bouncer-relay", "bouncer-core", "bouncer-app"];
const fail = (why) => {
  console.error(`version: ${why}`);
  process.exit(1);
};
const parse = (v) => {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(v ?? "");
  if (!m) fail(`"${v}" is not X.Y.Z`);
  return m.slice(1).map(Number);
};
const json = (path) => JSON.parse(readFileSync(path, "utf8"));
const writeJson = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2) + "\n");

const [mode, arg, dir] = process.argv.slice(2);
if (mode === "tag") {
  const m = /^v(\d+\.\d+\.\d+)$/.exec(arg ?? "");
  if (!m) fail(`tag "${arg}" is not vX.Y.Z`);
  const v = parse(m[1]);
  const older = v.findIndex((n, i) => n !== FIRST[i]);
  if (older !== -1 && v[older] < FIRST[older]) fail(`tag ${arg} is older than the first release, v${FIRST.join(".")}`);
  console.log(m[1]);
} else if (mode === "set") {
  parse(arg);
  const toml = readFileSync("Cargo.toml", "utf8");
  const next = toml.replace(/(\[workspace\.package\][^[]*?\nversion = )"[^"]*"/, `$1"${arg}"`);
  if (next === toml && !toml.includes(`version = "${arg}"`)) fail("no version under [workspace.package] in Cargo.toml");
  writeFileSync("Cargo.toml", next);
  // Our own three crates in Cargo.lock, by text: no network and nothing else
  // changes (`cargo update --offline` needs a crates index a fresh runner
  // doesn't have). The --locked build that follows proves the lock still fits.
  let lock = readFileSync("Cargo.lock", "utf8");
  for (const name of CRATES) {
    const entry = new RegExp(`(\\[\\[package\\]\\]\\nname = "${name}"\\nversion = )"[^"]*"`);
    if (!entry.test(lock)) fail(`no ${name} in Cargo.lock`);
    lock = lock.replace(entry, `$1"${arg}"`);
  }
  writeFileSync("Cargo.lock", lock);
  const pkg = json("package.json");
  writeJson("package.json", { ...pkg, version: arg });
  // A text edit, so the file keeps its layout: "version" after "productName".
  const path = "crates/app/tauri.conf.json";
  const conf = readFileSync(path, "utf8").replace(/\n {2}"version": "[^"]*",/, "");
  const stamped = conf.replace(/(\n {2}"productName": "[^"]*",)/, `$1\n  "version": "${arg}",`);
  if (stamped === conf) fail(`no "productName" line in ${path}`);
  writeFileSync(path, stamped);
} else if (mode === "check") {
  parse(arg);
  const meta = JSON.parse(execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1", "--locked"]));
  const found = Object.fromEntries(meta.packages.map((p) => [p.name, p.version]));
  for (const name of CRATES) if (found[name] !== arg) fail(`${name} is ${found[name]}, not ${arg}`);
  if (json("package.json").version !== arg) fail(`package.json is ${json("package.json").version}, not ${arg}`);
  const conf = json("crates/app/tauri.conf.json").version;
  if (conf !== arg) fail(`tauri.conf.json is ${conf}, not ${arg}`);
  if (dir) {
    const mac = process.platform === "darwin";
    const want = mac ? `Bouncer_${arg}_universal.dmg` : `Bouncer_${arg}_x64-setup.exe`;
    const installers = readdirSync(dir).filter((f) => f.endsWith(mac ? ".dmg" : "-setup.exe"));
    if (installers.length !== 1 || installers[0] !== want) {
      fail(`installer in ${dir} is ${installers.join(", ") || "missing"}, not ${want}`);
    }
  }
  console.log(`version ${arg}: crates, package.json, tauri.conf.json${dir ? " and the installer" : ""} agree`);
} else {
  fail("usage: version.mjs tag vX.Y.Z | set X.Y.Z | check X.Y.Z [installer dir]");
}
