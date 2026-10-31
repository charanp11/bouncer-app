// Installed copies find their update in latest.json by platform key. A key
// the updater plugin doesn't look up isn't an error anyone sees: every check
// just finds nothing, for good. So the release stops here instead (PLAN,
// "v0.13.1"):
//
//   node scripts/check-updater-target.mjs out/latest.json
//
// With a second argument it also checks the plugin builds that key, from an
// updater.rs or, with "pinned", the source Cargo.lock pins. The tag's checks do
// that (release.test.ts), so the release job needn't reach crates.io after the
// build. --windows-only is the release Charan asked for by hand when the Mac
// build can't be fixed in time (PLAN, "Roadmap g"); nothing else skips the Mac keys.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";

const MAC_KEYS = ["darwin-aarch64", "darwin-x86_64"];
const KEYS = ["windows-x86_64", ...MAC_KEYS];
// How the plugin makes those keys: {os}-{arch}.
const LOOKUP = ['Some("windows")', 'Some("darwin")', 'Some("x86_64")', 'Some("aarch64")', 'targets.push(format!("{os}-{arch}"));'];

const fail = (why) => {
  console.error(why);
  process.exit(1);
};
const args = process.argv.slice(2);
const windowsOnly = args.includes("--windows-only");
const [file, source] = args.filter((a) => a !== "--windows-only");
if (!file) fail("usage: check-updater-target.mjs latest.json [updater.rs | pinned] [--windows-only]");

const pinned = () => {
  const meta = execFileSync("cargo", ["metadata", "--locked", "--format-version", "1"], {
    encoding: "utf8",
    maxBuffer: 1 << 28,
  });
  const plugin = JSON.parse(meta).packages.find((p) => p.name === "tauri-plugin-updater");
  if (!plugin) fail("tauri-plugin-updater isn't in Cargo.lock");
  return join(dirname(plugin.manifest_path), "src", "updater.rs");
};
if (source) {
  const code = readFileSync(source === "pinned" ? pinned() : source, "utf8");
  for (const line of LOOKUP) {
    if (!code.includes(line)) fail(`the updater plugin changed its lookup: no ${line}`);
  }
}

const platforms = Object.keys(JSON.parse(readFileSync(file, "utf8")).platforms ?? {}).sort();
const want = KEYS.filter((k) => !(windowsOnly && MAC_KEYS.includes(k))).sort();
if (platforms.join() !== want.join()) {
  fail(`latest.json has [${platforms}], the plugin looks up ${want}`);
}
