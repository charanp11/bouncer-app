import assert from "node:assert/strict";
import { test } from "node:test";
import { markdown, pieces, type Block } from "./markdown.ts";

const text = (t: string) => ({ kind: "text", text: t });

test("markdown: paragraphs, headings, lists, quotes, rules and code blocks", () => {
  const src = [
    "# Title",
    "Some **bold** and *italic* and `code`.",
    "Second line of the same paragraph.",
    "",
    "- one",
    "- two",
    "",
    "3. three",
    "4. four",
    "",
    "> quoted",
    "---",
    "```rust",
    "fn main() {}",
    "```",
  ].join("\n");
  const blocks = markdown(src);
  assert.deepEqual(blocks.map((b) => b.kind), ["heading", "p", "list", "list", "quote", "rule", "code"]);
  assert.deepEqual(blocks[0], { kind: "heading", level: 1, pieces: [text("Title")] });
  assert.deepEqual((blocks[1] as Extract<Block, { kind: "p" }>).pieces, [
    text("Some "),
    { kind: "bold", text: "bold" },
    text(" and "),
    { kind: "italic", text: "italic" },
    text(" and "),
    { kind: "code", text: "code" },
    text(".\nSecond line of the same paragraph."),
  ]);
  assert.deepEqual(blocks[2], { kind: "list", ordered: false, start: 1, items: [[text("one")], [text("two")]] });
  assert.deepEqual(blocks[3], { kind: "list", ordered: true, start: 3, items: [[text("three")], [text("four")]] });
  assert.deepEqual(blocks[6], { kind: "code", syntax: "rust", text: "fn main() {}" });
  assert.equal((markdown("###### small")[0] as Extract<Block, { kind: "heading" }>).level, 3);
});

test("markdown: what the hand check showed reads as a list with bold", () => {
  const blocks = markdown("I'm a read-only assistant. Here's what I can do:\n\n- **Read files** with the Read tool\n- **Search file names** with Glob (for example `**/*.ts`)");
  assert.equal(blocks[1].kind, "list");
  const items = (blocks[1] as Extract<Block, { kind: "list" }>).items;
  assert.deepEqual(items[0][0], { kind: "bold", text: "Read files" });
  assert.deepEqual(items[1].at(-2), { kind: "code", text: "**/*.ts" }, "stars in code stay code");
});

test("markdown: links are their words and address as text, never a link", () => {
  assert.deepEqual(pieces("see [the docs](https://example.com/a) now"), [text("see the docs (https://example.com/a) now")]);
  assert.deepEqual(pieces("[https://x.dev](https://x.dev)"), [text("https://x.dev")]);
  assert.deepEqual(pieces("[evil](javascript:alert(1))"), [text("evil (javascript:alert(1))")], "a script address is only text");
  for (const block of markdown("[a](javascript:alert(1)) and <a href=x>b</a>")) {
    assert.ok(!("href" in block));
  }
});

test("markdown: tags, hidden-character markers and odd stars stay as text", () => {
  assert.deepEqual(pieces("<img src=x onerror=alert(1)> <script>"), [text("<img src=x onerror=alert(1)> <script>")]);
  assert.deepEqual(pieces("a \\u{202E} b"), [text("a \\u{202E} b")]);
  assert.deepEqual(pieces("2 * 3 * 4"), [text("2 * 3 * 4")]);
  assert.deepEqual(pieces("snake_case_name and __init__"), [text("snake_case_name and "), { kind: "bold", text: "init" }]);
  assert.deepEqual(pieces("**unclosed bold"), [text("**unclosed bold")]);
  assert.deepEqual(pieces("`` a ` b ``"), [{ kind: "code", text: "a ` b" }]);
});

test("markdown: an answer still streaming reads sensibly at every point", () => {
  const full = "Intro\n\n```shell\nls -la\necho hi\n```\n\nDone.";
  for (let n = 0; n <= full.length; n++) {
    const blocks = markdown(full.slice(0, n));
    for (const b of blocks) assert.ok(["p", "code", "heading", "list", "quote", "rule"].includes(b.kind));
  }
  // An unclosed fence is code to the end (the rest is still coming).
  assert.deepEqual(markdown("```py\nprint(1)\n"), [{ kind: "code", syntax: "python", text: "print(1)\n" }]);
  assert.deepEqual(markdown(full).map((b) => b.kind), ["p", "code", "p"]);
});

test("markdown: random input never throws and loses no letters outside markup", () => {
  const alphabet = "ab *_`#->[]()\n1.\\u{}<>";
  let seed = 7;
  const rand = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
  for (let i = 0; i < 2000; i++) {
    let s = "";
    for (let n = Math.floor(rand() * 60); n > 0; n--) s += alphabet[Math.floor(rand() * alphabet.length)];
    const blocks = markdown(s);
    const all = JSON.stringify(blocks);
    for (const letter of s.replace(/[^ab]/g, "")) assert.ok(all.includes(letter), s);
  }
});
