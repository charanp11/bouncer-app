// Run with `npm test` (Node's built-in runner; no test dependency).
import assert from "node:assert/strict";
import { test } from "node:test";
import { tokenize } from "./tokenize.ts";

const classes = (line: string, syntax: string) =>
  tokenize(line, syntax).filter((t) => t.cls).map((t) => `${t.cls}:${t.text}`);

test("rust keywords, types, numbers, strings and comments", () => {
  assert.deepEqual(classes('pub const ARM: Duration = Duration::from_millis(600); // arm', "rust"), [
    "k:pub", "k:const", "ty:ARM", "ty:Duration", "ty:Duration", "num:600", "c:// arm",
  ]);
  assert.deepEqual(classes('let s = "a \\" b";', "rust"), ["k:let", 's:"a \\" b"']);
});

test("rust lifetimes are not strings", () => {
  assert.deepEqual(classes("fn f<'a>(x: &'a str) {}", "rust"), ["k:fn"]);
  assert.deepEqual(classes("let c = 'x';", "rust"), ["k:let", "s:'x'"]);
});

test("pieces always join back to the exact line", () => {
  const lines = [
    "<script>alert(1)</script>",
    "const x = `t ${y}` // c",
    "if [ -f x ]; then echo '#not a comment' # comment; fi",
    'def f(a="x"): return None  # done',
    '{ "a": [1, 2.5, true, null] }',
    "\\u{202E}gnp.exe",
    "",
  ];
  for (const syntax of ["rust", "ts", "python", "go", "json", "shell", "", "cobol"]) {
    for (const line of lines) {
      assert.equal(tokenize(line, syntax).map((t) => t.text).join(""), line, `${syntax}: ${line}`);
    }
  }
});

test("unknown languages are one plain piece", () => {
  assert.deepEqual(tokenize("SELECT 1", "sql"), [{ cls: "", text: "SELECT 1" }]);
  assert.deepEqual(tokenize("", "sql"), []);
});

test("shell comment needs a space before #", () => {
  assert.deepEqual(classes("echo a#b # c", "shell"), ["c:# c"]);
});
