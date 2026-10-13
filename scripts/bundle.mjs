// Builds the installer: the relay and the CLI in release, copied where Tauri
// takes extra programs (binaries/<name>-<target triple>), then `tauri build`
// with tauri.release.json, which lists them. They stay out of tauri.conf.json
// because every plain cargo build would then need them first.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";

const windows = process.platform === "win32";
const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit" });
const triple = execFileSync("rustc", ["--print", "host-tuple"]).toString().trim();
const target = process.env.CARGO_TARGET_DIR ?? "target";
const ext = windows ? ".exe" : "";

run("cargo", ["build", "--release", "--locked", "-p", "bouncer-relay", "-p", "bouncer-core", "--bins"]);
mkdirSync("crates/app/binaries", { recursive: true });
for (const name of ["bouncer-hook", "bouncer"]) {
  copyFileSync(`${target}/release/${name}${ext}`, `crates/app/binaries/${name}-${triple}${ext}`);
}
// The CLI through node itself: no shell, so nothing is ever re-parsed.
run(process.execPath, ["node_modules/@tauri-apps/cli/tauri.js", "build", "--config", "crates/app/tauri.release.json"]);
