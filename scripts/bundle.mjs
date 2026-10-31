// Builds the installer: the relay and the CLI in release, copied where Tauri
// takes extra programs (binaries/<name>-<target triple>), then `tauri build`
// with tauri.release.json, which lists them. They stay out of tauri.conf.json
// because every plain cargo build would then need them first.
// It never signs for the updater: the release workflow signs the finished
// installer in a step of its own (the updater key never reaches a build).
//
// --universal (macOS): both architectures joined, ad-hoc signed
// (tauri.release.json), and the updater's Bouncer_X_universal.app.tar.gz.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync } from "node:fs";

const windows = process.platform === "win32";
const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit" });
const triple = execFileSync("rustc", ["--print", "host-tuple"]).toString().trim();
const target = process.env.CARGO_TARGET_DIR ?? "target";
const ext = windows ? ".exe" : "";

const universal = process.argv.includes("--universal");
const SLICES = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
const JOINED = "universal-apple-darwin";
const NAMES = ["bouncer-hook", "bouncer"];
const CARGO = ["build", "--release", "--locked", "-p", "bouncer-relay", "-p", "bouncer-core", "--bins"];
const binary = (name, tuple) => `crates/app/binaries/${name}-${tuple}${ext}`;

mkdirSync("crates/app/binaries", { recursive: true });
if (universal) {
  // Tauri wants all three: each slice for its own build, the joined one to bundle.
  for (const tuple of SLICES) {
    run("cargo", [...CARGO, "--target", tuple]);
    for (const name of NAMES) copyFileSync(`${target}/${tuple}/release/${name}`, binary(name, tuple));
  }
  for (const name of NAMES) {
    run("lipo", ["-create", ...SLICES.map((tuple) => binary(name, tuple)), "-output", binary(name, JOINED)]);
  }
} else {
  run("cargo", CARGO);
  for (const name of NAMES) copyFileSync(`${target}/release/${name}${ext}`, binary(name, triple));
}
// The CLI through node itself: no shell, so nothing is ever re-parsed.
const build = ["node_modules/@tauri-apps/cli/tauri.js", "build", "--config", "crates/app/tauri.release.json"];
run(process.execPath, universal ? [...build, "--target", JOINED] : build);

if (universal) {
  // One top-level Bouncer.app: the updater drops each entry's first folder.
  // No AppleDouble (._) entries, which would land outside the bundle.
  process.env.COPYFILE_DISABLE = "1";
  const { version } = JSON.parse(readFileSync("crates/app/tauri.conf.json", "utf8"));
  const macos = `${target}/${JOINED}/release/bundle/macos`;
  run("tar", ["-czf", `${macos}/Bouncer_${version}_universal.app.tar.gz`, "-C", macos, "Bouncer.app"]);
}
