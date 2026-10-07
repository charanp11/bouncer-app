import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { CUES, length, loudness, PACK, voice } from "./pack.ts";

test("every sound is under 1 s; the wipe under 0.6 s", () => {
  for (const c of CUES) assert.ok(length(c) <= 1, `${c}: ${length(c)} s`);
  assert.ok(length("wiped") < 0.6);
});

test("the pack is exactly the sounds preferences.json knows, in its order", () => {
  const rust = readFileSync(new URL("../crates/core/src/prefs.rs", import.meta.url), "utf8");
  const list = rust.match(/pub const SOUNDS: \[&str; \d+\] = \[([^\]]*)\]/)![1];
  assert.deepEqual(CUES, [...list.matchAll(/"(\w+)"/g)].map((m) => m[1]));
});

test("alarms stand out in Soft: louder than the rest, buzz kept, harsh top cut", () => {
  for (const c of CUES) {
    if (c === "needs" || c === "risky") assert.equal(loudness(c, "soft") - loudness("done", "soft"), 5);
    else assert.ok(loudness(c, "soft") <= -3, c);
  }
  for (const p of PACK.risky.parts.filter((p) => p.k === "T")) {
    const soft = voice(p, "soft");
    assert.equal(soft.w, p.w); // sawtooth / square stay
    assert.ok(soft.fl && soft.fl[0] === "lowpass" && soft.fl[2] <= 2200);
  }
  // A non-alarm sound turns to a plain sine in Soft.
  assert.equal(voice(PACK.done.parts[0], "soft").w, "sine");
});

test("a style changes timbre only, never timing or pitch", () => {
  for (const c of CUES) {
    for (const p of PACK[c].parts) {
      const soft = voice(p, "soft");
      assert.deepEqual([soft.t, soft.d, soft.f], [p.t, p.d, p.f]);
      assert.deepEqual(voice(p, "playful"), { ...p, a: p.a ?? 0.004 });
    }
  }
});
